mod api;
mod definition;
mod run;
mod store;

pub use api::*;
pub use definition::*;
pub use run::*;
pub use store::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn story_graph() -> WorkflowGraph {
        WorkflowGraph {
            nodes: vec![
                Node {
                    id: "start".into(),
                    kind: NodeKind::Start,
                },
                Node {
                    id: "work".into(),
                    kind: NodeKind::Agent {
                        role: AgentRole::Implementer,
                        capabilities: vec!["story_read".into(), "story_report".into()],
                        prompt_template: "Implement {{story.title}}".into(),
                    },
                },
                Node {
                    id: "end".into(),
                    kind: NodeKind::End,
                },
            ],
            edges: vec![
                Edge {
                    from: "start".into(),
                    to: "work".into(),
                    outcome: None,
                },
                Edge {
                    from: "work".into(),
                    to: "end".into(),
                    outcome: None,
                },
            ],
        }
    }

    #[test]
    fn validation_rejects_unreachable_outcomes_joins_capabilities_and_cycles() {
        let mut graph = story_graph();
        graph.nodes.push(Node {
            id: "lost".into(),
            kind: NodeKind::Notify,
        });
        assert!(validate_graph(&graph, WorkflowKind::Story).is_err());

        let mut graph = story_graph();
        graph.nodes[1].kind = NodeKind::Judge;
        assert!(validate_graph(&graph, WorkflowKind::Story).is_err());

        let mut graph = story_graph();
        graph.nodes[1].kind = NodeKind::Join;
        assert!(validate_graph(&graph, WorkflowKind::Plan).is_err());

        let mut graph = story_graph();
        if let NodeKind::Agent { capabilities, .. } = &mut graph.nodes[1].kind {
            capabilities.push("shell_exec".into());
        }
        assert!(validate_graph(&graph, WorkflowKind::Story).is_err());

        let mut graph = story_graph();
        graph.edges.push(Edge {
            from: "work".into(),
            to: "start".into(),
            outcome: None,
        });
        assert!(validate_graph(&graph, WorkflowKind::Story).is_err());

        let mut graph = story_graph();
        graph.nodes[1].kind = NodeKind::Loop { max_iterations: 0 };
        assert!(validate_graph(&graph, WorkflowKind::Story).is_err());
    }

    #[test]
    fn published_revisions_are_immutable_when_a_draft_changes() {
        let dir = tempfile::tempdir().expect("db dir");
        let store = WorkflowStore::open_at(&dir.path().join("workflow.sqlite3")).expect("store");
        let draft = store
            .create_draft("/project", "Delivery", WorkflowKind::Story, story_graph())
            .expect("draft");
        let first = store
            .publish(&draft.id, draft.draft_revision)
            .expect("publish");
        assert_eq!(first.revision, 1);
        let mut changed = story_graph();
        if let NodeKind::Agent {
            prompt_template, ..
        } = &mut changed.nodes[1].kind
        {
            *prompt_template = "Revised prompt".into();
        }
        let draft = store
            .update_draft(&draft.id, draft.draft_revision, changed)
            .expect("edit draft");
        let second = store
            .publish(&draft.id, draft.draft_revision)
            .expect("publish second");
        assert_eq!(second.revision, 2);
        assert_ne!(
            store.get_published(&draft.id, 1).expect("first").graph,
            store.get_published(&draft.id, 2).expect("second").graph
        );
        assert!(store.publish(&draft.id, draft.draft_revision).is_err());
    }

    #[test]
    fn seeded_resolve_plan_pins_the_story_delivery_template() {
        let dir = tempfile::tempdir().expect("db dir");
        let store = WorkflowStore::open_at(&dir.path().join("workflow.sqlite3")).expect("store");
        let templates = store.seed_templates("/project").expect("seed");
        let again = store.seed_templates("/project").expect("seed again");
        assert_eq!(templates, again);
        let plan = templates
            .iter()
            .find(|template| template.kind == WorkflowKind::Plan)
            .expect("plan");
        let story = templates
            .iter()
            .find(|template| template.kind == WorkflowKind::Story)
            .expect("story");
        let published = store.get_published(&plan.id, 1).expect("published plan");
        assert!(published.graph.nodes.iter().any(|node| matches!(&node.kind,
            NodeKind::StoryDispatch { story_template_id, story_revision }
            if story_template_id == &story.id && *story_revision == 1)));
        assert!(
            published
                .graph
                .nodes
                .iter()
                .any(|node| matches!(node.kind, NodeKind::CreateStories))
        );
        let mut invalid = plan.graph.clone();
        for node in &mut invalid.nodes {
            if let NodeKind::StoryDispatch {
                story_template_id, ..
            } = &mut node.kind
            {
                *story_template_id = "missing-template".into();
            }
        }
        let edited = store
            .update_draft(&plan.id, plan.draft_revision, invalid)
            .expect("edit plan");
        assert!(store.publish(&plan.id, edited.draft_revision).is_err());
        assert_eq!(
            store
                .get_published(&plan.id, 1)
                .expect("original revision")
                .revision,
            1
        );
    }
}
