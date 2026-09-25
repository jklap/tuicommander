mod api;
mod model;
mod reducer;
mod store;

pub use api::*;
pub use model::*;
pub use store::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stories::{NewPlan, NewStory, StoryCommand, StoryOrigin, StoryStore};
    use crate::workflows::{WorkflowKind, WorkflowStore};

    fn fixture() -> (
        tempfile::TempDir,
        tempfile::TempDir,
        String,
        String,
        String,
        impl Drop,
    ) {
        let config = tempfile::tempdir().expect("config");
        let project = tempfile::tempdir().expect("project");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project_path = project
            .path()
            .canonicalize()
            .expect("canonical project")
            .to_string_lossy()
            .to_string();
        let stories = StoryStore::open().expect("stories");
        let plan = stories
            .create_plan(NewPlan {
                project: project_path.clone(),
                title: "Resolve".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let story = stories
            .create_story(NewStory {
                plan_id: plan.id.clone(),
                title: "Work".into(),
                criteria: vec!["Done".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("story");
        let definitions = WorkflowStore::open().expect("definitions");
        let plan_template = definitions
            .seed_templates(&project_path)
            .expect("templates")
            .into_iter()
            .find(|draft| draft.kind == WorkflowKind::Plan)
            .expect("plan template");
        (config, project, plan.id, story.id, plan_template.id, _guard)
    }

    #[test]
    fn bound_agent_report_is_typed_owned_idempotent_and_replayable() {
        let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
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
            .expect("start");
        let started = store
            .command(
                &run.id,
                "attempt",
                RunCommand::StartAttempt {
                    story_id: story_id.clone(),
                    node_id: "implement".into(),
                },
            )
            .expect("attempt");
        let attempt = &started.snapshot.attempts[0];
        let reserved = store
            .command(
                &run.id,
                "reserve",
                RunCommand::ReserveEffect {
                    key: format!("spawn:{}", attempt.id),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .expect("reserve spawn");
        let binding = AgentBinding {
            session_id: "managed-session".into(),
            task_id: Some("task-1".into()),
            effect_id: reserved.snapshot.effects[0].id.clone(),
            prompt_contract_version: 1,
            prompt_sha256: "a".repeat(64),
            audit_preview: "redacted prompt".into(),
        };
        let bound = store
            .bind_agent(&run.id, &attempt.id, binding.clone())
            .expect("bind");
        assert_eq!(bound.snapshot.effects[0].state, EffectState::Succeeded);
        assert_eq!(bound.snapshot.attempts[0].agent.as_ref(), Some(&binding));
        assert_eq!(
            store
                .bind_agent(&run.id, &attempt.id, binding)
                .unwrap()
                .sequence,
            bound.sequence
        );
        assert!(
            store
                .command(
                    &run.id,
                    "untyped",
                    RunCommand::ReportAttempt {
                        attempt_id: attempt.id.clone(),
                        generation: attempt.generation,
                        outcome: AttemptOutcome::Completed,
                    }
                )
                .is_err()
        );

        let story = StoryStore::open().unwrap().get_story(&story_id).unwrap();
        let report = AttemptReport {
            contract_version: 1,
            run_id: run.id.clone(),
            story_id,
            story_revision: story.revision,
            attempt_id: attempt.id.clone(),
            generation: attempt.generation,
            outcome: AttemptOutcome::Completed,
            summary: "Implemented and checked".into(),
            criterion_results: vec![CriterionResult {
                index: 0,
                satisfied: true,
                evidence: "focused test passed".into(),
            }],
            evidence: vec!["test receipt 123".into()],
        };
        assert!(
            store
                .report_bound_agent(report.clone(), "other-session")
                .is_err()
        );
        let accepted = store
            .report_bound_agent(report.clone(), "managed-session")
            .expect("report");
        assert_eq!(accepted.snapshot.attempts[0].report.as_ref(), Some(&report));
        assert_eq!(
            store
                .report_bound_agent(report, "managed-session")
                .unwrap()
                .sequence,
            accepted.sequence
        );
        assert!(
            store
                .interrupt_agent_session("managed-session")
                .unwrap()
                .is_empty()
        );

        let next = store
            .command(
                &run.id,
                "attempt-after-report",
                RunCommand::StartAttempt {
                    story_id: story.id.clone(),
                    node_id: "implement".into(),
                },
            )
            .expect("next attempt");
        let next_attempt = next.snapshot.attempts.last().unwrap();
        let reserved = store
            .command(
                &run.id,
                "reserve-after-report",
                RunCommand::ReserveEffect {
                    key: format!("spawn:{}", next_attempt.id),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .expect("reserve next spawn");
        store
            .bind_agent(
                &run.id,
                &next_attempt.id,
                AgentBinding {
                    session_id: "managed-exit".into(),
                    task_id: None,
                    effect_id: reserved.snapshot.effects.last().unwrap().id.clone(),
                    prompt_contract_version: 1,
                    prompt_sha256: "b".repeat(64),
                    audit_preview: "redacted prompt".into(),
                },
            )
            .expect("bind next");
        let interrupted = store
            .interrupt_agent_session("managed-exit")
            .expect("observe exit");
        assert_eq!(interrupted.len(), 1);
        assert_eq!(interrupted[0].status, RunStatus::Paused);
        assert_eq!(
            interrupted[0].attempts.last().unwrap().state,
            AttemptState::Interrupted
        );
        assert!(interrupted[0].attempts.last().unwrap().report.is_none());
        assert!(
            store
                .interrupt_agent_session("managed-exit")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            store.replay(&run.id).unwrap(),
            store.snapshot(&run.id).unwrap()
        );
    }

    #[test]
    fn cancellation_records_an_unfinished_spawn_as_uncertain() {
        let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
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
        let started = store
            .command(
                &run.id,
                "attempt",
                RunCommand::StartAttempt {
                    story_id,
                    node_id: "implement".into(),
                },
            )
            .unwrap();
        let attempt_id = started.snapshot.attempts[0].id.clone();
        let reserved = store
            .command(
                &run.id,
                "spawn",
                RunCommand::ReserveEffect {
                    key: format!("spawn:{attempt_id}"),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .unwrap();
        let cancelled = store
            .command(&run.id, "cancel", RunCommand::Cancel)
            .unwrap();
        assert_eq!(cancelled.snapshot.status, RunStatus::Cancelled);
        assert_eq!(cancelled.snapshot.effects[0].state, EffectState::Uncertain);
        assert_eq!(
            cancelled.snapshot.attempts[0].state,
            AttemptState::Interrupted
        );
        assert!(
            store
                .bind_agent(
                    &run.id,
                    &attempt_id,
                    AgentBinding {
                        session_id: "late-agent".into(),
                        task_id: None,
                        effect_id: reserved.snapshot.effects[0].id.clone(),
                        prompt_contract_version: 1,
                        prompt_sha256: "a".repeat(64),
                        audit_preview: "redacted".into(),
                    }
                )
                .is_err()
        );
        assert_eq!(store.replay(&run.id).unwrap(), cancelled.snapshot);
    }

    #[test]
    fn plan_coordinator_attempt_does_not_create_a_story_execution() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
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
            .expect("start");
        let started = store
            .command(
                &run.id,
                "coordinate",
                RunCommand::StartPlanAgent {
                    node_id: "coordinate".into(),
                },
            )
            .expect("plan coordinator");
        assert_eq!(started.snapshot.attempts[0].story_id, plan_id);
        assert!(started.snapshot.stories.is_empty());
        assert_eq!(
            store.replay(&run.id).unwrap(),
            store.snapshot(&run.id).unwrap()
        );
    }

    #[test]
    fn only_the_bound_coordinator_can_create_a_story_once() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
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
        let started = store
            .command(
                &run.id,
                "coordinate",
                RunCommand::StartPlanAgent {
                    node_id: "coordinate".into(),
                },
            )
            .unwrap();
        let attempt_id = started.snapshot.attempts[0].id.clone();
        let reserved = store
            .command(
                &run.id,
                "spawn",
                RunCommand::ReserveEffect {
                    key: format!("spawn:{attempt_id}"),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .unwrap();
        store
            .bind_agent(
                &run.id,
                &attempt_id,
                AgentBinding {
                    session_id: "coordinator".into(),
                    task_id: None,
                    effect_id: reserved.snapshot.effects[0].id.clone(),
                    prompt_contract_version: 1,
                    prompt_sha256: "a".repeat(64),
                    audit_preview: "redacted".into(),
                },
            )
            .unwrap();
        let proposed = NewStory {
            plan_id: plan_id.clone(),
            title: "Follow up".into(),
            criteria: vec!["Covered".into()],
            priority: 1,
            origin: StoryOrigin::PlanStep {
                step: "follow-up".into(),
            },
            file_scope: vec![],
        };
        assert!(
            store
                .create_story_from_coordinator(&run.id, "stranger", "proposal-1", proposed.clone())
                .is_err()
        );
        let first = store
            .create_story_from_coordinator(&run.id, "coordinator", "proposal-1", proposed.clone())
            .unwrap();
        let second = store
            .create_story_from_coordinator(&run.id, "coordinator", "proposal-1", proposed.clone())
            .unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(
            StoryStore::open()
                .unwrap()
                .list_stories(&plan_id)
                .unwrap()
                .len(),
            2
        );
        assert!(
            store
                .snapshot(&run.id)
                .unwrap()
                .effects
                .iter()
                .any(|effect| {
                    effect.key == "create-story:proposal-1"
                        && effect.state == EffectState::Succeeded
                })
        );
        let mut changed = proposed;
        changed.title = "Different".into();
        assert!(
            store
                .create_story_from_coordinator(&run.id, "coordinator", "proposal-1", changed)
                .is_err()
        );
    }

    #[test]
    fn effect_outcomes_remain_recordable_when_story_shape_reopens_planning() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
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
            .expect("start");
        store
            .command(&run.id, "close", RunCommand::ClosePlanning)
            .expect("close planning");
        let first = store
            .command(
                &run.id,
                "effect-1",
                RunCommand::ReserveEffect {
                    key: "spawn-first".into(),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .expect("first effect")
            .snapshot
            .effects[0]
            .id
            .clone();
        let second = store
            .command(
                &run.id,
                "effect-2",
                RunCommand::ReserveEffect {
                    key: "spawn-second".into(),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .expect("second effect")
            .snapshot
            .effects[1]
            .id
            .clone();
        StoryStore::open()
            .unwrap()
            .create_story(NewStory {
                plan_id,
                title: "New evidence".into(),
                criteria: vec!["Reviewed".into()],
                priority: 2,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("change planning fingerprint");
        let marked = store
            .command(
                &run.id,
                "mark-first",
                RunCommand::MarkEffect {
                    effect_id: first,
                    succeeded: true,
                },
            )
            .expect("mark observed success");
        assert!(matches!(
            marked.event.kind,
            RunEventKind::EffectChanged {
                state: EffectState::Succeeded,
                ..
            }
        ));
        let uncertain = store
            .reconcile(&run.id)
            .expect("reconcile remaining intent");
        assert_eq!(uncertain.effects[1].state, EffectState::Uncertain);
        let resolved = store
            .command(
                &run.id,
                "resolve-second",
                RunCommand::ResolveUncertainEffect {
                    effect_id: second,
                    succeeded: false,
                },
            )
            .expect("resolve observed failure");
        assert!(matches!(
            resolved.event.kind,
            RunEventKind::EffectChanged {
                state: EffectState::Failed,
                ..
            }
        ));
        assert_eq!(
            store.replay(&run.id).unwrap(),
            store.snapshot(&run.id).unwrap()
        );
    }

    #[test]
    fn command_events_and_projection_replay_after_reopen() {
        let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
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
            .expect("start");
        let closed = store
            .command(&run.id, "close-plan", RunCommand::ClosePlanning)
            .expect("close planning");
        assert!(closed.snapshot.planning_fingerprint.is_some());
        let same = store
            .command(&run.id, "close-plan", RunCommand::ClosePlanning)
            .expect("duplicate");
        assert_eq!(same.sequence, closed.sequence);
        let attempt = store
            .command(
                &run.id,
                "attempt-1",
                RunCommand::StartAttempt {
                    story_id,
                    node_id: "implement".into(),
                },
            )
            .expect("attempt");
        assert_eq!(attempt.snapshot.attempts.len(), 1);
        drop(store);
        let reopened = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("reopen");
        assert_eq!(
            reopened.replay(&run.id).expect("replay"),
            reopened.snapshot(&run.id).expect("snapshot")
        );
        assert_eq!(
            reopened
                .events_after(&run.id, 0, 100)
                .expect("events")
                .len(),
            3
        );
        assert_eq!(
            reopened.reconcile_active().expect("restart reconciliation"),
            1
        );
        assert_eq!(
            reopened.snapshot(&run.id).unwrap().status,
            RunStatus::Paused
        );
        let late = reopened
            .command(
                &run.id,
                "late-after-restart",
                RunCommand::ReportAttempt {
                    attempt_id: attempt.snapshot.attempts[0].id.clone(),
                    generation: attempt.snapshot.attempts[0].generation,
                    outcome: AttemptOutcome::Completed,
                },
            )
            .expect("audit late report");
        assert!(matches!(
            late.event.kind,
            RunEventKind::LateReportIgnored { .. }
        ));
        assert_eq!(late.snapshot.attempts[0].state, AttemptState::Interrupted);
    }

    #[test]
    fn stale_attempt_report_is_audited_without_advancing() {
        let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
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
            .expect("start");
        let started = store
            .command(
                &run.id,
                "attempt-1",
                RunCommand::StartAttempt {
                    story_id,
                    node_id: "implement".into(),
                },
            )
            .expect("attempt");
        let attempt = &started.snapshot.attempts[0];
        let stale = store
            .command(
                &run.id,
                "late",
                RunCommand::ReportAttempt {
                    attempt_id: attempt.id.clone(),
                    generation: attempt.generation + 1,
                    outcome: AttemptOutcome::Completed,
                },
            )
            .expect("audited late report");
        assert!(matches!(
            stale.event.kind,
            RunEventKind::LateReportIgnored { .. }
        ));
        assert_eq!(stale.snapshot.attempts[0].state, AttemptState::Running);
        let report = store
            .command(
                &run.id,
                "report",
                RunCommand::ReportAttempt {
                    attempt_id: attempt.id.clone(),
                    generation: attempt.generation,
                    outcome: AttemptOutcome::Completed,
                },
            )
            .expect("report");
        assert_eq!(report.snapshot.attempts[0].state, AttemptState::Reported);
    }

    #[test]
    fn uncertain_effect_is_not_replayed_after_restart_and_limits_hold() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let limits = RunLimits {
            max_loops: 1,
            max_story_creations: 1,
            max_spawns: 1,
            max_duration_secs: 3600,
        };
        let run = store
            .start_plan(&project_path, &plan_id, &definition_id, 1, limits)
            .expect("start");
        store
            .command(
                &run.id,
                "spawn-1",
                RunCommand::ReserveEffect {
                    key: "spawn-one".into(),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .expect("spawn intent");
        assert!(
            store
                .command(
                    &run.id,
                    "spawn-2",
                    RunCommand::ReserveEffect {
                        key: "spawn-two".into(),
                        kind: EffectKind::SpawnAgent
                    }
                )
                .is_err()
        );
        store
            .command(&run.id, "loop-1", RunCommand::AdvanceLoop)
            .expect("loop");
        assert!(
            store
                .command(&run.id, "loop-2", RunCommand::AdvanceLoop)
                .is_err()
        );
        drop(store);
        let reopened = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("reopen");
        assert_eq!(
            reopened.reconcile_active().expect("reconcile active runs"),
            1
        );
        let snapshot = reopened.snapshot(&run.id).expect("snapshot");
        assert_eq!(snapshot.effects[0].state, EffectState::Uncertain);
        assert_eq!(snapshot.status, RunStatus::Paused);
        assert!(
            reopened
                .command(&run.id, "resume-too-soon", RunCommand::Resume)
                .is_err()
        );
        reopened
            .command(
                &run.id,
                "resolve",
                RunCommand::ResolveUncertainEffect {
                    effect_id: snapshot.effects[0].id.clone(),
                    succeeded: false,
                },
            )
            .expect("resolve uncertain effect");
        reopened
            .command(&run.id, "resume", RunCommand::Resume)
            .expect("resume");
        assert_eq!(
            reopened.snapshot(&run.id).unwrap().status,
            RunStatus::Running
        );
    }

    #[test]
    fn adding_a_story_reopens_planning_before_the_next_command() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
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
            .expect("start");
        store
            .command(&run.id, "close", RunCommand::ClosePlanning)
            .expect("close");
        StoryStore::open()
            .unwrap()
            .create_story(NewStory {
                plan_id,
                title: "New work".into(),
                criteria: vec!["New criterion".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("add story");
        let receipt = store
            .command(&run.id, "verify", RunCommand::FinalVerificationPassed)
            .expect("reopen planning");
        assert!(matches!(receipt.event.kind, RunEventKind::PlanningReopened));
        assert!(receipt.snapshot.planning_fingerprint.is_none());
        assert!(receipt.snapshot.verification_fingerprint.is_none());
    }

    #[test]
    fn command_identity_is_bound_to_the_payload_and_only_one_run_can_be_active() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
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
            .expect("start");
        assert!(
            store
                .start_plan(
                    &project_path,
                    &plan_id,
                    &definition_id,
                    1,
                    RunLimits::default()
                )
                .is_err()
        );
        store
            .command(&run.id, "same", RunCommand::ClosePlanning)
            .expect("first command");
        assert!(store.command(&run.id, "same", RunCommand::Pause).is_err());
    }

    #[test]
    fn concurrent_retry_commits_one_event_and_one_projection() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
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
            .expect("start");
        std::thread::scope(|scope| {
            let first = scope.spawn(|| store.command(&run.id, "same-pause", RunCommand::Pause));
            let second = scope.spawn(|| store.command(&run.id, "same-pause", RunCommand::Pause));
            assert_eq!(first.join().unwrap().unwrap().sequence, 2);
            assert_eq!(second.join().unwrap().unwrap().sequence, 2);
        });
        assert_eq!(store.snapshot(&run.id).unwrap().sequence, 2);
        assert_eq!(store.events_after(&run.id, 0, 10).unwrap().len(), 2);
        assert_eq!(
            store.replay(&run.id).unwrap(),
            store.snapshot(&run.id).unwrap()
        );
    }

    #[test]
    fn duration_budget_pauses_before_processing_an_expired_command() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
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
                RunLimits {
                    max_duration_secs: 1,
                    ..RunLimits::default()
                },
            )
            .expect("start");
        let receipt = store
            .command_at(
                &run.id,
                "late",
                RunCommand::ClosePlanning,
                run.started_ms + 1001,
            )
            .expect("pause");
        assert!(matches!(receipt.event.kind, RunEventKind::Paused));
        assert_eq!(receipt.snapshot.status, RunStatus::Paused);
        assert!(receipt.snapshot.planning_fingerprint.is_none());
    }

    #[test]
    fn run_api_scopes_reads_and_commands_to_the_canonical_project() {
        let (_config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let other = tempfile::tempdir().expect("other project");
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let RunReply::Snapshot(run) = run_action(
            &project_path,
            RunAction::StartPlan {
                plan_id: plan_id.clone(),
                definition_id: definition_id.clone(),
                definition_revision: 1,
                limits: RunLimits::default(),
            },
        )
        .expect("start") else {
            panic!("snapshot reply");
        };
        let RunReply::Runs(runs) = run_action(
            &project_path,
            RunAction::ListPlanRuns {
                plan_id: plan_id.clone(),
                limit: 20,
            },
        )
        .expect("list runs") else {
            panic!("runs reply")
        };
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, run.id);
        assert!(
            run_action(
                other.path().to_str().unwrap(),
                RunAction::ListPlanRuns {
                    plan_id: plan_id.clone(),
                    limit: 20,
                }
            )
            .is_err()
        );
        assert!(
            run_action(
                &project_path,
                RunAction::ListPlanRuns {
                    plan_id: plan_id.clone(),
                    limit: 0,
                }
            )
            .is_err()
        );
        assert!(
            run_action(
                other.path().to_str().unwrap(),
                RunAction::Get {
                    run_id: run.id.clone()
                }
            )
            .is_err()
        );
        assert!(
            run_action(
                other.path().to_str().unwrap(),
                RunAction::Command {
                    run_id: run.id.clone(),
                    command_id: "other-project".into(),
                    expected_sequence: run.sequence,
                    command: RunCommand::Pause,
                }
            )
            .is_err()
        );
        let RunReply::Events(events) = run_action(
            &format!("{project_path}/."),
            RunAction::Events {
                run_id: run.id.clone(),
                after_sequence: 0,
                limit: 10,
            },
        )
        .expect("events") else {
            panic!("events reply");
        };
        assert_eq!(events.len(), 1);
        let RunReply::Receipt(paused) = run_action(
            &project_path,
            RunAction::Command {
                run_id: run.id.clone(),
                command_id: "pause".into(),
                expected_sequence: run.sequence,
                command: RunCommand::Pause,
            },
        )
        .expect("pause") else {
            panic!("receipt reply");
        };
        assert_eq!(paused.sequence, run.sequence + 1);
        assert!(
            run_action(
                &project_path,
                RunAction::Command {
                    run_id: run.id,
                    command_id: "stale-cancel".into(),
                    expected_sequence: run.sequence,
                    command: RunCommand::Cancel,
                }
            )
            .is_err()
        );
        let RunReply::Receipt(cancelled) = run_action(
            &project_path,
            RunAction::Command {
                run_id: paused.snapshot.id,
                command_id: "cancel-after-pause".into(),
                expected_sequence: paused.sequence,
                command: RunCommand::Cancel,
            },
        )
        .expect("cancel") else {
            panic!("cancel receipt")
        };
        assert_eq!(cancelled.snapshot.status, RunStatus::Cancelled);
        let RunReply::Snapshot(newest) = run_action(
            &project_path,
            RunAction::StartPlan {
                plan_id: plan_id.clone(),
                definition_id,
                definition_revision: 1,
                limits: RunLimits::default(),
            },
        )
        .expect("restart plan") else {
            panic!("new run")
        };
        let RunReply::Runs(latest) =
            run_action(&project_path, RunAction::ListPlanRuns { plan_id, limit: 1 })
                .expect("latest run")
        else {
            panic!("latest runs")
        };
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].id, newest.id);
    }

    #[test]
    fn resolve_plan_requires_closure_accepted_stories_and_fresh_verification() {
        let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
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
            .expect("start");
        assert!(
            store
                .command(&run.id, "too-early", RunCommand::Complete)
                .is_err()
        );
        store
            .command(&run.id, "close", RunCommand::ClosePlanning)
            .expect("close");
        assert!(
            store
                .command(&run.id, "still-early", RunCommand::Complete)
                .is_err()
        );
        let stories = StoryStore::open().expect("stories");
        let story = stories.get_story(&story_id).expect("story");
        let story = stories
            .claim(&story_id, "tab", story.revision)
            .expect("claim");
        let story = stories
            .transition(&story_id, story.revision, StoryCommand::CheckCriterion(0))
            .expect("check");
        let story = stories
            .transition(&story_id, story.revision, StoryCommand::SubmitReview)
            .expect("review");
        stories
            .transition(&story_id, story.revision, StoryCommand::Approve)
            .expect("approve");
        store
            .command(
                &run.id,
                "accept",
                RunCommand::AcceptStory {
                    story_id: story_id.clone(),
                },
            )
            .expect("accept");
        let mut revised = stories.get_story(&story_id).expect("accepted story");
        revised.revision += 1;
        assert!(
            super::store::ready_to_verify(&store.snapshot(&run.id).unwrap(), &[revised]).is_err()
        );
        store
            .command(&run.id, "verify", RunCommand::FinalVerificationPassed)
            .expect("verify");
        let finished = store
            .command(&run.id, "complete", RunCommand::Complete)
            .expect("complete");
        assert_eq!(finished.snapshot.status, RunStatus::Completed);
    }
}
