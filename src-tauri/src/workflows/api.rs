use super::{
    CheckDefinition, PublishedWorkflow, WorkflowClosure, WorkflowDraft, WorkflowGraph,
    WorkflowKind, WorkflowStore,
};
use serde::{Deserialize, Serialize};

/// Authority selected by the host transport, never deserialized from a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkflowActor {
    Human,
    LocalApi,
    ManagedSession,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkflowAction {
    SeedTemplates,
    CreateDraft {
        name: String,
        kind: WorkflowKind,
        graph: WorkflowGraph,
    },
    ListDrafts,
    GetDraft {
        id: String,
    },
    UpdateDraft {
        id: String,
        expected_revision: i64,
        graph: WorkflowGraph,
    },
    UpdateClosure {
        id: String,
        expected_revision: i64,
        closure: WorkflowClosure,
    },
    UpdateChecks {
        id: String,
        expected_revision: i64,
        checks: Vec<CheckDefinition>,
    },
    Publish {
        id: String,
        expected_revision: i64,
    },
    GetPublished {
        id: String,
        revision: i64,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum WorkflowReply {
    Draft(WorkflowDraft),
    Drafts(Vec<WorkflowDraft>),
    Published(PublishedWorkflow),
}

fn draft_in_project(
    store: &WorkflowStore,
    id: &str,
    project: &str,
) -> Result<WorkflowDraft, String> {
    let draft = store.get_draft(id)?;
    if draft.project != project {
        return Err("workflow does not belong to project".into());
    }
    Ok(draft)
}

fn check_draft_size(graph: &WorkflowGraph) -> Result<(), String> {
    if graph.nodes.len() > 128
        || graph.edges.len() > 512
        || serde_json::to_vec(graph)
            .map_err(|e| format!("encode workflow draft: {e}"))?
            .len()
            > 256_000
    {
        return Err("workflow draft is too large".into());
    }
    Ok(())
}

/// Project-scoped definition operations shared by desktop and HTTP transports.
pub fn definition_action(project: &str, action: WorkflowAction) -> Result<WorkflowReply, String> {
    definition_action_for_actor(project, action, WorkflowActor::Human)
}

pub fn definition_action_for_actor(
    project: &str,
    action: WorkflowAction,
    actor: WorkflowActor,
) -> Result<WorkflowReply, String> {
    if actor != WorkflowActor::Human
        && matches!(
            &action,
            WorkflowAction::CreateDraft { .. }
                | WorkflowAction::UpdateDraft { .. }
                | WorkflowAction::UpdateClosure { .. }
                | WorkflowAction::UpdateChecks { .. }
                | WorkflowAction::Publish { .. }
        )
    {
        return Err("workflow policy mutation requires an authenticated user action".into());
    }

    if !crate::fs::is_absolute_on_any_platform(project) {
        return Err("project must be an absolute path".into());
    }
    let project = crate::progress::resolve_owning_project(Some(project))?
        .to_string_lossy()
        .to_string();
    let store = WorkflowStore::open()?;
    match action {
        WorkflowAction::SeedTemplates => Ok(WorkflowReply::Drafts(store.seed_templates(&project)?)),
        WorkflowAction::CreateDraft { name, kind, graph } => {
            check_draft_size(&graph)?;
            Ok(WorkflowReply::Draft(
                store.create_draft(&project, &name, kind, graph)?,
            ))
        }
        WorkflowAction::ListDrafts => {
            store.seed_templates(&project)?;
            Ok(WorkflowReply::Drafts(store.list_drafts(&project)?))
        }
        WorkflowAction::GetDraft { id } => Ok(WorkflowReply::Draft(draft_in_project(
            &store, &id, &project,
        )?)),
        WorkflowAction::UpdateDraft {
            id,
            expected_revision,
            graph,
        } => {
            draft_in_project(&store, &id, &project)?;
            check_draft_size(&graph)?;
            Ok(WorkflowReply::Draft(store.update_draft(
                &id,
                expected_revision,
                graph,
            )?))
        }
        WorkflowAction::UpdateClosure {
            id,
            expected_revision,
            closure,
        } => {
            draft_in_project(&store, &id, &project)?;
            Ok(WorkflowReply::Draft(store.update_closure(
                &id,
                expected_revision,
                closure,
            )?))
        }
        WorkflowAction::UpdateChecks {
            id,
            expected_revision,
            checks,
        } => {
            draft_in_project(&store, &id, &project)?;
            Ok(WorkflowReply::Draft(store.update_checks(
                &id,
                expected_revision,
                checks,
            )?))
        }
        WorkflowAction::Publish {
            id,
            expected_revision,
        } => {
            draft_in_project(&store, &id, &project)?;
            Ok(WorkflowReply::Published(
                store.publish(&id, expected_revision)?,
            ))
        }
        WorkflowAction::GetPublished { id, revision } => {
            let published = store.get_published(&id, revision)?;
            if published.project != project {
                return Err("workflow does not belong to project".into());
            }
            Ok(WorkflowReply::Published(published))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_actions_are_scoped_to_the_canonical_owner() {
        let config = tempfile::tempdir().expect("config");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let first = tempfile::tempdir().expect("first");
        let second = tempfile::tempdir().expect("second");
        let first = first.path().to_str().expect("first path");
        let second = second.path().to_str().expect("second path");
        let WorkflowReply::Drafts(templates) =
            definition_action(first, WorkflowAction::SeedTemplates).expect("seed")
        else {
            panic!("draft list");
        };
        let plan = templates
            .iter()
            .find(|draft| draft.kind == WorkflowKind::Plan)
            .expect("plan");
        assert!(
            definition_action(
                second,
                WorkflowAction::GetDraft {
                    id: plan.id.clone()
                }
            )
            .is_err()
        );
        assert!(matches!(
            definition_action(
                &format!("{first}/."),
                WorkflowAction::GetDraft {
                    id: plan.id.clone()
                }
            ),
            Ok(WorkflowReply::Draft(_))
        ));
    }
}

#[cfg(test)]
mod actor_tests {
    use super::*;

    #[test]
    fn local_and_managed_calls_cannot_publish_check_policy_as_a_user() {
        // catches: loopback agents publish arbitrary argv under implicit Human authority.
        for actor in [WorkflowActor::LocalApi, WorkflowActor::ManagedSession] {
            for action in [
                WorkflowAction::UpdateChecks {
                    id: "draft".into(),
                    expected_revision: 1,
                    checks: vec![],
                },
                WorkflowAction::Publish {
                    id: "draft".into(),
                    expected_revision: 1,
                },
            ] {
                assert!(
                    definition_action_for_actor("relative", action, actor)
                        .unwrap_err()
                        .contains("authenticated user")
                );
            }
        }
        // Human routing passes the authority gate and reaches project validation.
        assert!(
            definition_action_for_actor(
                "relative",
                WorkflowAction::Publish {
                    id: "draft".into(),
                    expected_revision: 1
                },
                WorkflowActor::Human
            )
            .unwrap_err()
            .contains("absolute path")
        );
    }
}
