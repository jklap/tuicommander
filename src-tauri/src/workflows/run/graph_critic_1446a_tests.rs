//! Critic tests for 1446 slice A: pause fences, Pause targets, legacy run migration.
use super::graph::*;
use super::reducer::apply_event;
use super::*;
use crate::workflows::{
    AgentRole, Edge, JoinMode, Node, NodeKind, PublishedWorkflow, WorkflowClosure, WorkflowGraph,
    WorkflowKind, validate_executable_graph,
};

fn node(id: &str, kind: NodeKind) -> Node {
    Node {
        id: id.into(),
        kind,
    }
}

fn edge(from: &str, to: &str, outcome: Option<&str>) -> Edge {
    Edge {
        from: from.into(),
        to: to.into(),
        outcome: outcome.map(Into::into),
    }
}

fn reviewer() -> NodeKind {
    NodeKind::Agent {
        role: AgentRole::Reviewer,
        capabilities: vec!["story_read".into()],
        prompt_template: "review".into(),
    }
}

/// Story graph with a read-only Fork/Join and one Pause whose target is `resume_to`.
fn fork_graph(resume_to: &str) -> WorkflowGraph {
    WorkflowGraph {
        nodes: vec![
            node("start", NodeKind::Start),
            node(
                "impl",
                NodeKind::Agent {
                    role: AgentRole::Implementer,
                    capabilities: vec!["story_read".into()],
                    prompt_template: "implement".into(),
                },
            ),
            node(
                "fork",
                NodeKind::Fork {
                    join_id: "join".into(),
                },
            ),
            node("r1", reviewer()),
            node("r2", reviewer()),
            node(
                "join",
                NodeKind::Join {
                    mode: JoinMode::All,
                    fork_id: Some("fork".into()),
                },
            ),
            node("judge", NodeKind::Judge),
            node("repair", NodeKind::Loop { max_iterations: 3 }),
            node(
                "pause",
                NodeKind::Pause {
                    resume_to: Some(resume_to.into()),
                },
            ),
            node("end", NodeKind::End),
        ],
        edges: vec![
            edge("start", "impl", None),
            edge("impl", "fork", None),
            edge("fork", "r1", None),
            edge("fork", "r2", None),
            edge("r1", "join", None),
            edge("r2", "join", None),
            edge("join", "judge", None),
            edge("judge", "end", Some("yes")),
            edge("judge", "repair", Some("no")),
            edge("judge", "pause", Some("uncertain")),
            edge("repair", "impl", Some("repeat")),
            edge("repair", "pause", Some("exhausted")),
        ],
    }
}

#[test]
fn fork_graph_control_is_valid_with_end_target() {
    validate_executable_graph(&fork_graph("end"), WorkflowKind::Story, true)
        .expect("control graph must be executable-valid");
}

/// Catches: a Pause resuming into the middle of a Fork scope, so the branch runs
/// without its Fork and the all Join can never release.
#[test]
fn pause_resume_target_cannot_enter_a_fork_branch() {
    assert!(
        validate_executable_graph(&fork_graph("r1"), WorkflowKind::Story, true).is_err(),
        "resume_to inside a Fork scope must be refused"
    );
}

fn evidence() -> DecisionEvidence {
    DecisionEvidence {
        actor: DecisionActor::Session {
            session_id: "s".into(),
        },
        reason: "r".into(),
        references: vec!["x".into()],
    }
}

fn event(snapshot: &RunSnapshot, kind: RunEventKind) -> RunEvent {
    RunEvent {
        sequence: snapshot.sequence + 1,
        command_id: format!("c{}", snapshot.sequence + 1),
        command_hash: None,
        at_ms: 0,
        kind,
    }
}

fn step(snapshot: RunSnapshot, kind: RunEventKind) -> RunSnapshot {
    let e = event(&snapshot, kind);
    apply_event(Some(snapshot), &e).expect("event applies")
}

fn graph_step(snapshot: RunSnapshot, transition: GraphTransition) -> RunSnapshot {
    step(
        snapshot,
        RunEventKind::Graph {
            event: GraphEvent::Transition { transition },
        },
    )
}

fn pause_graph() -> WorkflowGraph {
    WorkflowGraph {
        nodes: vec![
            node("start", NodeKind::Start),
            node("judge", NodeKind::Judge),
            node(
                "pause",
                NodeKind::Pause {
                    resume_to: Some("end".into()),
                },
            ),
            node("end", NodeKind::End),
        ],
        edges: vec![
            edge("start", "judge", None),
            edge("judge", "end", Some("yes")),
            edge("judge", "end", Some("no")),
            edge("judge", "pause", Some("uncertain")),
        ],
    }
}

/// Catches: ResolvePause forcing status Running although an unresolved uncertain
/// effect still holds the run paused (Resume refuses in that state).
#[test]
fn resolve_pause_keeps_the_uncertain_effect_fence() {
    let (config, project, plan_id, _story, definition_id, _guard) = super::tests::fixture();
    let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
    let project_path = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let run = store
        .start_plan(
            &project_path,
            &plan_id,
            &definition_id,
            1,
            RunLimits::default(),
        )
        .unwrap();
    let definition = PublishedWorkflow {
        id: run.definition_id.clone(),
        project: run.project.clone(),
        name: "g".into(),
        kind: WorkflowKind::Story,
        closure: WorkflowClosure::Human,
        required_checks: vec![crate::workflows::CheckDefinition {
            id: "c".into(),
            argv: vec!["true".into()],
            timeout_secs: 5,
        }],
        graph: pause_graph(),
        revision: run.definition_revision,
    };
    let execution = GraphExecution::start("g1".into(), run.plan_id.clone(), definition).unwrap();
    let mut s = step(
        run,
        RunEventKind::Graph {
            event: GraphEvent::Started {
                execution: Box::new(execution),
            },
        },
    );
    let t = |a: &str| (("g1".to_string()), a.to_string());
    let (e, a) = t("a0");
    s = graph_step(
        s,
        GraphTransition::Activate {
            execution_id: e.clone(),
            activation_id: a.clone(),
        },
    );
    s = graph_step(
        s,
        GraphTransition::Complete {
            execution_id: e.clone(),
            activation_id: a,
            outcome: None,
            evidence: None,
        },
    );
    s = graph_step(
        s,
        GraphTransition::Activate {
            execution_id: e.clone(),
            activation_id: "a1".into(),
        },
    );
    s = graph_step(
        s,
        GraphTransition::Complete {
            execution_id: e.clone(),
            activation_id: "a1".into(),
            outcome: Some(EdgeOutcome::Uncertain),
            evidence: Some(evidence()),
        },
    );
    s = graph_step(
        s,
        GraphTransition::Activate {
            execution_id: e.clone(),
            activation_id: "a2".into(),
        },
    );
    s = graph_step(
        s,
        GraphTransition::Complete {
            execution_id: e.clone(),
            activation_id: "a2".into(),
            outcome: None,
            evidence: Some(evidence()),
        },
    );
    assert_eq!(s.status, RunStatus::Paused);
    s = step(
        s,
        RunEventKind::EffectReserved {
            effect: EffectIntent {
                id: "e1".into(),
                key: "k".into(),
                kind: EffectKind::SpawnAgent,
                state: EffectState::Intended,
            },
        },
    );
    s = step(
        s,
        RunEventKind::EffectChanged {
            effect_id: "e1".into(),
            state: EffectState::Uncertain,
        },
    );
    s = graph_step(
        s,
        GraphTransition::ResolvePause {
            execution_id: e,
            activation_id: "a2".into(),
            resolution: PauseResolution::Retry {
                reason: "go".into(),
            },
        },
    );
    assert_eq!(
        s.status,
        RunStatus::Paused,
        "an uncertain effect must still hold the run paused"
    );
}
