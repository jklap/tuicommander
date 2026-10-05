use super::*;
use crate::stories::{NewPlan, NewStory, StoryCommand, StoryOrigin};

#[test]
fn story_reservation_blocks_second_root() {
    // catches: concurrent plan/direct/manual ownership (28), including changed-payload retries.
    let (config, project, plan, story, template, _guard) = fixture();
    let store = RunStore::open_at(&config.path().join("workflow_runs.sqlite3")).unwrap();
    let published = definition(project.path().to_str().unwrap(), false, true);
    let start = request(
        project.path().to_str().unwrap(),
        &story,
        &published,
        "reservation",
    );
    let (first, retry) = std::thread::scope(|scope| {
        let a = scope.spawn(|| store.start_graph_run(&start));
        let b = scope.spawn(|| store.start_graph_run(&start));
        (a.join().unwrap().unwrap(), b.join().unwrap().unwrap())
    });
    assert_eq!(first.id, retry.id);
    let mut other = start;
    other.request_id = "second-root".into();
    assert!(store.start_graph_run(&other).is_err());
    assert!(
        store
            .start_plan(
                project.path().to_str().unwrap(),
                &plan,
                &template,
                1,
                RunLimits::default()
            )
            .is_err()
    );
    let stories = StoryStore::open().unwrap();
    let revision = stories.get_story(&story).unwrap().revision;
    assert!(
        stories
            .claim(&story, "manual", revision)
            .unwrap_err()
            .contains("reserved")
    );
    assert!(
        stories
            .transition(&story, revision, StoryCommand::StartManual)
            .unwrap_err()
            .contains("reserved")
    );
    store
        .command(&first.id, "release", RunCommand::Cancel)
        .unwrap();
    stories.claim(&story, "manual", revision).unwrap();
    other.expected_revision = Some(stories.get_story(&story).unwrap().revision);
    assert!(
        store
            .start_graph_run(&other)
            .unwrap_err()
            .contains("manual claim")
    );
}

#[tokio::test]
async fn parallel_plan_runs_share_project_reservations() {
    // catches: one paused run starving another daemon actor (37, B foundation).
    // Dispatch waves and writable effects are added by C/E, not asserted here.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let state = state(config.path());
    WorkflowRuntime::spawn(&state);
    owner_ready(&state).await;
    let project = project.path().to_str().unwrap();
    let stories = StoryStore::open().unwrap();
    let plan = stories
        .create_plan(NewPlan {
            project: project.into(),
            title: "Independent".into(),
            source: "second.md".into(),
        })
        .unwrap();
    let second = stories
        .create_story(NewStory {
            plan_id: plan.id,
            title: "Second".into(),
            criteria: vec!["Done".into()],
            priority: 1,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    let published = definition(project, false, false);
    let first = state
        .workflow_runtime
        .start_graph(&state, &request(project, &story, &published, "one"))
        .unwrap();
    let mut second_request = request(project, &second.id, &published, "two");
    second_request.limits.max_duration_secs = 1;
    let mut hints = state.event_bus.subscribe();
    let second = state
        .workflow_runtime
        .start_graph(&state, &second_request)
        .unwrap();
    run_action_with_events(
        &state,
        project,
        RunAction::Command {
            run_id: first.id.clone(),
            command_id: "pause-one".into(),
            expected_sequence: first.sequence,
            command: Box::new(RunCommand::Pause),
        },
    )
    .unwrap();
    let store = RunStore::open().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            hints.recv().await.unwrap();
            if store.snapshot(&second.id).unwrap().status == RunStatus::Paused {
                break;
            }
        }
    })
    .await
    .expect("a paused run starved an independent run's timer");
    let first = store.snapshot(&first.id).unwrap();
    let second = store.snapshot(&second.id).unwrap();
    assert_eq!(first.status, RunStatus::Paused);
    assert!(
        store
            .events_after(&second.id, 0, 20)
            .unwrap()
            .iter()
            .any(|event| matches!(event.kind, RunEventKind::DeadlineExpired { .. }))
    );
    run_action_with_events(
        &state,
        project,
        RunAction::Command {
            run_id: first.id.clone(),
            command_id: "cancel-one".into(),
            expected_sequence: first.sequence,
            command: Box::new(RunCommand::Cancel),
        },
    )
    .unwrap();
    assert_eq!(store.snapshot(&second.id).unwrap(), second);
    assert_eq!(store.replay(&second.id).unwrap(), second);
}

#[tokio::test]
async fn unsupported_graph_start_and_non_owner_mutations_are_refused() {
    // catches: executing an unsupported Gate, or a second process executing a graph.
    let (config, project, plan, story, template, _guard) = fixture();
    let state = state(config.path());
    let error = run_action_with_events(
        &state,
        project.path().to_str().unwrap(),
        RunAction::StartPlan {
            plan_id: plan,
            definition_id: template,
            definition_revision: 1,
            limits: RunLimits::default(),
        },
    )
    .unwrap_err();
    assert!(error.contains("executor unavailable"));
    WorkflowRuntime::spawn(&state);
    owner_ready(&state).await;
    let published = definition(project.path().to_str().unwrap(), true, false);
    let mut start = request(
        project.path().to_str().unwrap(),
        &story,
        &published,
        "unsupported",
    );
    assert!(
        state
            .workflow_runtime
            .start_graph(&state, &start)
            .unwrap_err()
            .contains("not executable")
    );
    start.definition_revision += 1;
    assert!(
        state.workflow_runtime.start_graph(&state, &start).is_err(),
        "an unpublished revision cannot start"
    );
}
