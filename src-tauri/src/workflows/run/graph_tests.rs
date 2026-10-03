use super::graph::*;
use super::*;
use crate::workflows::{
    AgentRole, Edge, JoinMode, Node, NodeKind, WorkflowGraph, WorkflowKind, WorkflowStore,
    validate_executable_graph,
};

fn evidence() -> DecisionEvidence {
    DecisionEvidence {
        actor: DecisionActor::Session {
            session_id: "reviewer-session".into(),
        },
        reason: "Recorded current review evidence".into(),
        references: vec!["review:current-artifact".into()],
    }
}

// Native stores are the fixture authority; no fabricated model output is used.
fn graph_fixture() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    RunStore,
    RunSnapshot,
    String,
    impl Drop,
) {
    let (config, project, plan_id, story_id, definition_id, guard) = super::tests::fixture();
    let definitions = WorkflowStore::open().unwrap();
    let plan = definitions.get_draft(&definition_id).unwrap();
    let (story_definition_id, story_revision) = plan
        .graph
        .nodes
        .iter()
        .find_map(|node| {
            if let NodeKind::StoryDispatch {
                story_template_id,
                story_revision,
            } = &node.kind
            {
                Some((story_template_id.clone(), *story_revision))
            } else {
                None
            }
        })
        .unwrap();
    let mut story = definitions.get_draft(&story_definition_id).unwrap();
    assert_eq!(story_revision, 1);
    for node in &mut story.graph.nodes {
        if let NodeKind::Pause { resume_to } = &mut node.kind {
            *resume_to = Some("implement".into());
        }
    }
    let story = definitions
        .update_draft(&story.id, story.draft_revision, story.graph)
        .unwrap();
    let published = definitions
        .publish_executable(&story.id, story.draft_revision)
        .unwrap();
    let mut graph = plan.graph;
    for node in &mut graph.nodes {
        if let NodeKind::StoryDispatch { story_revision, .. } = &mut node.kind {
            *story_revision = published.revision;
        }
        if let NodeKind::Pause { resume_to } = &mut node.kind {
            *resume_to = Some("coordinate".into());
        }
    }
    let plan = definitions
        .update_draft(&plan.id, plan.draft_revision, graph)
        .unwrap();
    let plan = definitions
        .update_checks(
            &plan.id,
            plan.draft_revision,
            published.required_checks.clone(),
        )
        .unwrap();
    let published = definitions
        .publish_executable(&plan.id, plan.draft_revision)
        .unwrap();
    let store = RunStore::open_at(&config.path().join("graph-runs.sqlite3")).unwrap();
    let run = store
        .start_plan(
            project.path().to_str().unwrap(),
            &plan_id,
            &definition_id,
            published.revision,
            RunLimits::default(),
        )
        .unwrap();
    (config, project, store, run, story_id, guard)
}

fn transition(
    store: &RunStore,
    run: &str,
    command_id: &str,
    transition: GraphTransition,
) -> RunReceipt {
    let sequence = store.snapshot(run).unwrap().sequence;
    store
        .command_expected(run, command_id, sequence, RunCommand::Graph { transition })
        .unwrap()
}

fn start_graph(store: &RunStore, run: &str, story: &str) -> RunReceipt {
    transition(
        store,
        run,
        "graph-start",
        GraphTransition::Start {
            execution_id: "story-execution".into(),
            target_id: story.into(),
        },
    )
}

fn visit(
    store: &RunStore,
    run: &str,
    expected_node: &str,
    outcome: Option<EdgeOutcome>,
    reason: Option<DecisionEvidence>,
) -> RunReceipt {
    let snapshot = store.snapshot(run).unwrap();
    let activation = snapshot.graph_executions[0]
        .activations
        .iter()
        .find(|a| a.state == ActivationState::Ready)
        .unwrap();
    assert_eq!(activation.node_id, expected_node);
    let id = activation.id.clone();
    transition(
        store,
        run,
        &format!("activate-{id}"),
        GraphTransition::Activate {
            execution_id: "story-execution".into(),
            activation_id: id.clone(),
        },
    );
    transition(
        store,
        run,
        &format!("complete-{id}"),
        GraphTransition::Complete {
            execution_id: "story-execution".into(),
            activation_id: id,
            outcome,
            evidence: reason,
        },
    )
}

fn reach_judge(store: &RunStore, run: &str, initial: bool) {
    if initial {
        visit(store, run, "start", None, None);
    }
    visit(store, run, "implement", None, None);
    visit(store, run, "review", None, None);
}

// Catches: judge selecting two outcomes or duplicate retries producing a second successor (case 12).
#[test]
fn judge_selects_one_published_edge_once() {
    let (_config, _project, store, run, story, _guard) = graph_fixture();
    start_graph(&store, &run.id, &story);
    reach_judge(&store, &run.id, true);
    let snapshot = store.snapshot(&run.id).unwrap();
    let judge_id = snapshot.graph_executions[0]
        .activations
        .last()
        .unwrap()
        .id
        .clone();
    transition(
        &store,
        &run.id,
        "judge-active",
        GraphTransition::Activate {
            execution_id: "story-execution".into(),
            activation_id: judge_id.clone(),
        },
    );
    let seq = store.snapshot(&run.id).unwrap().sequence;
    let invalid = RunCommand::Graph {
        transition: GraphTransition::Complete {
            execution_id: "story-execution".into(),
            activation_id: judge_id.clone(),
            outcome: Some(EdgeOutcome::Pass),
            evidence: Some(evidence()),
        },
    };
    assert!(
        store
            .command(&run.id, "invalid-judge-outcome", invalid)
            .unwrap_err()
            .contains("published edge")
    );
    let invalid = RunCommand::Graph {
        transition: GraphTransition::Complete {
            execution_id: "story-execution".into(),
            activation_id: judge_id.clone(),
            outcome: Some(EdgeOutcome::Yes),
            evidence: None,
        },
    };
    assert!(
        store
            .command(&run.id, "judge-without-evidence", invalid)
            .unwrap_err()
            .contains("evidence")
    );
    assert_eq!(store.snapshot(&run.id).unwrap().sequence, seq);
    let command = RunCommand::Graph {
        transition: GraphTransition::Complete {
            execution_id: "story-execution".into(),
            activation_id: judge_id.clone(),
            outcome: Some(EdgeOutcome::Yes),
            evidence: Some(evidence()),
        },
    };
    let first = store
        .command_expected(&run.id, "judge-decision", seq, command.clone())
        .unwrap();
    assert_eq!(
        store
            .command_expected(&run.id, "judge-decision", seq, command)
            .unwrap(),
        first
    );
    let conflicting = RunCommand::Graph {
        transition: GraphTransition::Complete {
            execution_id: "story-execution".into(),
            activation_id: judge_id,
            outcome: Some(EdgeOutcome::No),
            evidence: Some(evidence()),
        },
    };
    assert!(
        store
            .command(&run.id, "second-decision", conflicting.clone())
            .unwrap_err()
            .contains("not running")
    );
    assert!(
        store
            .command_expected(&run.id, "judge-decision", seq, conflicting)
            .unwrap_err()
            .contains("different payload")
    );
    let graph = &first.snapshot.graph_executions[0];
    assert_eq!(graph.decisions.len(), 1);
    assert_eq!(graph.activations.last().unwrap().node_id, "end");
    assert_eq!(graph.decisions[0].evidence.actor, evidence().actor);
    assert_eq!(store.replay(&run.id).unwrap(), first.snapshot);
}

// Catches: pause resolution losing its pinned target, evidence or repair count on reopen (cases 14, 22).
#[test]
fn pause_pins_resume_target_and_pending_request() {
    let (config, _project, store, run, story, _guard) = graph_fixture();
    start_graph(&store, &run.id, &story);
    reach_judge(&store, &run.id, true);
    visit(
        &store,
        &run.id,
        "judge",
        Some(EdgeOutcome::No),
        Some(evidence()),
    );
    visit(&store, &run.id, "repair", None, None);
    reach_judge(&store, &run.id, false);
    visit(
        &store,
        &run.id,
        "judge",
        Some(EdgeOutcome::Uncertain),
        Some(evidence()),
    );
    let paused = visit(&store, &run.id, "pause", None, Some(evidence()));
    assert_eq!(paused.snapshot.status, RunStatus::Paused);
    let graph = &paused.snapshot.graph_executions[0];
    let pause_id = graph.activations.last().unwrap().id.clone();
    assert_eq!(graph.loops[0].repeats, 1);
    let definitions = WorkflowStore::open().unwrap();
    let draft = definitions.get_draft(&run.story_definition_id).unwrap();
    let mut graph = draft.graph;
    for node in &mut graph.nodes {
        if let NodeKind::Pause { resume_to } = &mut node.kind {
            *resume_to = Some("end".into());
        }
    }
    let edited = definitions
        .update_draft(&draft.id, draft.draft_revision, graph)
        .unwrap();
    definitions
        .publish_executable(&edited.id, edited.draft_revision)
        .unwrap();
    let reopened = RunStore::open_at(&config.path().join("graph-runs.sqlite3")).unwrap();
    assert_eq!(reopened.replay(&run.id).unwrap(), paused.snapshot);
    assert!(
        reopened
            .command(&run.id, "untyped-resume", RunCommand::Resume)
            .unwrap_err()
            .contains("typed activation")
    );
    let invalid = RunCommand::Graph {
        transition: GraphTransition::ResolvePause {
            execution_id: "story-execution".into(),
            activation_id: pause_id.clone(),
            resolution: PauseResolution::Input {
                answer: String::new(),
            },
        },
    };
    assert!(
        reopened
            .command(&run.id, "empty-resolution", invalid)
            .unwrap_err()
            .contains("empty")
    );
    let resumed = transition(
        &reopened,
        &run.id,
        "typed-resume",
        GraphTransition::ResolvePause {
            execution_id: "story-execution".into(),
            activation_id: pause_id,
            resolution: PauseResolution::Input {
                answer: "Current evidence supplied".into(),
            },
        },
    );
    let graph = &resumed.snapshot.graph_executions[0];
    assert_eq!(graph.activations.last().unwrap().node_id, "implement");
    assert_eq!(graph.definition.revision, run.story_definition_revision);
    assert_eq!(graph.loops[0].repeats, 1);
    assert_eq!(graph.pauses[0].evidence, evidence());
    assert!(graph.pauses[0].resolution.is_some());
    assert_eq!(reopened.replay(&run.id).unwrap(), resumed.snapshot);
}

// Catches: arbitrary AdvanceLoop bypassing predecessors, node cap or resumed retry counts (case 15).
#[test]
fn advance_loop_rejects_unreached_node_and_uses_pinned_cap() {
    let (_config, _project, store, run, story, _guard) = graph_fixture();
    start_graph(&store, &run.id, &story);
    let arbitrary = RunCommand::Graph {
        transition: GraphTransition::Complete {
            execution_id: "story-execution".into(),
            activation_id: "repair".into(),
            outcome: None,
            evidence: None,
        },
    };
    assert!(
        store
            .command(&run.id, "unreached-loop", arbitrary)
            .unwrap_err()
            .contains("not reached")
    );
    assert!(
        store
            .command(&run.id, "unscoped-loop", RunCommand::AdvanceLoop)
            .unwrap_err()
            .contains("typed activation")
    );
    for cycle in 0..4 {
        reach_judge(&store, &run.id, cycle == 0);
        visit(
            &store,
            &run.id,
            "judge",
            Some(EdgeOutcome::No),
            Some(evidence()),
        );
        let loop_result = visit(&store, &run.id, "repair", None, None);
        assert_eq!(
            loop_result.snapshot.graph_executions[0]
                .activations
                .last()
                .unwrap()
                .node_id,
            if cycle < 3 { "implement" } else { "pause" }
        );
    }
    let snapshot = store.snapshot(&run.id).unwrap();
    assert_eq!(snapshot.loops, 3);
    assert_eq!(snapshot.graph_executions[0].loops[0].repeats, 3);
    assert_eq!(store.replay(&run.id).unwrap(), snapshot);
}

// Catches: orphan, unbounded cycle, missing End or duplicate links entering a published execution (cases 18-21).
#[test]
fn executable_publication_rejects_invalid_graphs() {
    let (_config, _project, _store, run, _story, _guard) = graph_fixture();
    let definitions = WorkflowStore::open().unwrap();
    let original = definitions.get_draft(&run.story_definition_id).unwrap();
    let mut invalid = vec![];
    let mut graph = original.graph.clone();
    graph.nodes.push(Node {
        id: "orphan".into(),
        kind: NodeKind::End,
    });
    invalid.push((graph, "unreachable"));
    let mut graph = original.graph.clone();
    graph
        .edges
        .iter_mut()
        .find(|edge| edge.from == "review")
        .unwrap()
        .to = "implement".into();
    invalid.push((graph, "unreachable"));
    let mut graph = original.graph.clone();
    graph
        .nodes
        .iter_mut()
        .find(|node| node.id == "end")
        .unwrap()
        .kind = NodeKind::Notify;
    invalid.push((graph, "End"));
    let mut graph = original.graph.clone();
    graph.edges.push(graph.edges[0].clone());
    invalid.push((graph, "duplicate"));
    for (graph, expected) in invalid {
        let draft = definitions.get_draft(&original.id).unwrap();
        let draft = definitions
            .update_draft(&draft.id, draft.draft_revision, graph)
            .unwrap();
        assert!(
            definitions
                .publish_executable(&draft.id, draft.draft_revision)
                .unwrap_err()
                .contains(expected)
        );
    }
    // All nodes reachable: Judge no introduces a cycle outside the bounded Loop.
    let mut graph = original.graph.clone();
    graph
        .edges
        .iter_mut()
        .find(|edge| edge.from == "repair" && edge.outcome.as_deref() == Some("exhausted"))
        .unwrap()
        .to = "end".into();
    graph
        .edges
        .iter_mut()
        .find(|edge| edge.from == "judge" && edge.outcome.as_deref() == Some("yes"))
        .unwrap()
        .to = "repair".into();
    graph
        .edges
        .iter_mut()
        .find(|edge| edge.from == "judge" && edge.outcome.as_deref() == Some("no"))
        .unwrap()
        .to = "implement".into();
    assert!(
        validate_executable_graph(&graph, WorkflowKind::Story, true)
            .unwrap_err()
            .contains("cycle")
    );
}

// Catches: Resolve plan vacuous final verification or legacy Pause silently acquiring a guessed resume target.
#[test]
fn executable_contract_requires_checks_and_explicit_pause_target() {
    let (_config, _project, _store, run, _story, _guard) = graph_fixture();
    let definitions = WorkflowStore::open().unwrap();
    let draft = definitions.get_draft(&run.definition_id).unwrap();
    let draft = definitions
        .update_checks(&draft.id, draft.draft_revision, vec![])
        .unwrap();
    assert!(
        definitions
            .publish_executable(&draft.id, draft.draft_revision)
            .unwrap_err()
            .contains("deterministic final checks")
    );
    let legacy = definitions
        .get_published(&run.story_definition_id, 1)
        .unwrap();
    assert!(
        validate_executable_graph(&legacy.graph, WorkflowKind::Story, true)
            .unwrap_err()
            .contains("resume_to")
    );
    let join: NodeKind = serde_json::from_str(r#"{"type":"join"}"#).unwrap();
    assert_eq!(
        join,
        NodeKind::Join {
            mode: JoinMode::Merge,
            fork_id: None
        }
    );
}

// Catches: writable or foreign arrivals entering a supposedly read-only paired all-Join.
#[test]
fn fork_join_schema_rejects_writable_and_outside_branches() {
    let node = |id: &str, kind| Node {
        id: id.into(),
        kind,
    };
    let edge = |from: &str, to: &str| Edge {
        from: from.into(),
        to: to.into(),
        outcome: None,
    };
    let agent = |role| NodeKind::Agent {
        role,
        capabilities: vec!["story_read".into(), "story_report".into()],
        prompt_template: "Review current artifact".into(),
    };
    let mut graph = WorkflowGraph {
        nodes: vec![
            node("start", NodeKind::Start),
            node(
                "fork",
                NodeKind::Fork {
                    join_id: "join".into(),
                },
            ),
            node("review", agent(AgentRole::Reviewer)),
            node("validate", agent(AgentRole::Validator)),
            node(
                "join",
                NodeKind::Join {
                    mode: JoinMode::All,
                    fork_id: Some("fork".into()),
                },
            ),
            node("end", NodeKind::End),
        ],
        edges: vec![
            edge("start", "fork"),
            edge("fork", "review"),
            edge("fork", "validate"),
            edge("review", "join"),
            edge("validate", "join"),
            edge("join", "end"),
        ],
    };
    validate_executable_graph(&graph, WorkflowKind::Story, true).unwrap();
    let roundtrip: WorkflowGraph =
        serde_json::from_str(&serde_json::to_string(&graph).unwrap()).unwrap();
    assert_eq!(roundtrip, graph);
    graph.nodes[2].kind = agent(AgentRole::Implementer);
    assert!(
        validate_executable_graph(&graph, WorkflowKind::Story, true)
            .unwrap_err()
            .contains("read-only")
    );
    graph.nodes[2].kind = agent(AgentRole::Reviewer);
    graph.edges[2].to = "join".into();
    assert!(validate_executable_graph(&graph, WorkflowKind::Story, true).is_err());
}

// Catches: additive snapshot fields preventing cancellation or manufacturing positions for old runs.
#[test]
fn legacy_run_replays_and_cancels_without_guessed_graph_state() {
    let (config, _project, store, run, _story, _guard) = graph_fixture();
    let db = config.path().join("graph-runs.sqlite3");
    let connection = rusqlite::Connection::open(&db).unwrap();
    let mut snapshot = serde_json::to_value(&run).unwrap();
    snapshot
        .as_object_mut()
        .unwrap()
        .remove("eventContractVersion");
    snapshot.as_object_mut().unwrap().remove("graphExecutions");
    let receipt = store.events_after(&run.id, 0, 1).unwrap().remove(0);
    let mut event = serde_json::to_value(receipt).unwrap();
    // Started contains the pre-commit sequence (zero), not the sequence-one
    // projection returned by start_plan. Preserve the captured native event.
    event["kind"]["initial"]
        .as_object_mut()
        .unwrap()
        .remove("eventContractVersion");
    event["kind"]["initial"]
        .as_object_mut()
        .unwrap()
        .remove("graphExecutions");
    connection
        .execute(
            "UPDATE workflow_runs SET snapshot_json=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&snapshot).unwrap(), run.id],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE workflow_events SET event_json=?1 WHERE run_id=?2",
            rusqlite::params![serde_json::to_string(&event).unwrap(), run.id],
        )
        .unwrap();
    connection.pragma_update(None, "user_version", 1).unwrap();
    drop(connection);
    let upgraded = RunStore::open_at(&db).unwrap();
    let legacy = upgraded.replay(&run.id).unwrap();
    assert_eq!(legacy.event_contract_version, 0);
    assert!(legacy.graph_executions.is_empty());
    assert_eq!(legacy, upgraded.snapshot(&run.id).unwrap());
    for (key, command) in [
        ("legacy-resume", RunCommand::Resume),
        ("legacy-loop", RunCommand::AdvanceLoop),
    ] {
        assert!(
            upgraded
                .command(&run.id, key, command)
                .unwrap_err()
                .contains("inspect and cancel only")
        );
    }
    let cancelled = upgraded
        .command(&run.id, "legacy-cancel", RunCommand::Cancel)
        .unwrap();
    assert_eq!(cancelled.snapshot.status, RunStatus::Cancelled);
    assert!(cancelled.snapshot.graph_executions.is_empty());
    assert_eq!(upgraded.replay(&run.id).unwrap(), cancelled.snapshot);
}

// Catches: operator HTTP/IPC run commands forging daemon graph transitions.
#[test]
fn public_run_transport_rejects_internal_graph_mutation() {
    let (_config, project, plan_id, story_id, definition_id, _guard) = super::tests::fixture();
    let store = RunStore::open().unwrap();
    let project = project.path().to_str().unwrap();
    let run = store
        .start_plan(project, &plan_id, &definition_id, 1, RunLimits::default())
        .unwrap();
    let action = RunAction::Command {
        run_id: run.id.clone(),
        command_id: "forged-graph".into(),
        expected_sequence: run.sequence,
        command: RunCommand::Graph {
            transition: GraphTransition::Start {
                execution_id: "forged".into(),
                target_id: story_id,
            },
        },
    };
    assert!(
        run_action(project, action)
            .unwrap_err()
            .contains("internal backend service")
    );
    assert_eq!(store.snapshot(&run.id).unwrap(), run);
}
