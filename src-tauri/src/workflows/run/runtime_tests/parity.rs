use super::*;
use tower::ServiceExt;

#[tokio::test]
async fn public_graph_start_pins_revision_and_retries_without_duplicate_roots() {
    // Catches: public start using the empty legacy ledger, or retry creating another root.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let project = project.path().to_str().unwrap();
    let definition = definition(project, false, true);
    let state = state(config.path());
    WorkflowRuntime::spawn(&state);
    owner_ready(&state).await;
    let input = serde_json::json!({"action":"start_graph", "target":{"type":"story","id":story},
        "expected_revision": StoryStore::open().unwrap().get_story(&story).unwrap().revision,
        "definition_id":definition.id, "definition_revision":definition.revision,
        "request_id":"public-start"});
    let app = crate::mcp_http::build_router(state.clone(), false, true);
    let mut http_request = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/workflows/run/action?path={project}"))
        .header("host", "127.0.0.1:9876")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(input.to_string()))
        .unwrap();
    http_request
        .extensions_mut()
        .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            0,
        ))));
    let response = app.oneshot(http_request).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let RunReply::Snapshot(run) = serde_json::from_slice::<RunReply>(&body).unwrap() else {
        panic!("snapshot");
    };
    assert_eq!(run.root_target, Some(RunTarget::Story(story)));
    assert_eq!(run.graph_executions[0].definition, definition);
    assert_eq!(run.graph_executions[0].activations[1].node_id, "implement");
    let RunReply::Snapshot(retry) = run_action_with_events(
        &state,
        project,
        serde_json::from_value(input.clone()).unwrap(),
    )
    .unwrap() else {
        panic!("snapshot");
    };
    assert_eq!(retry.id, run.id);
    let mut changed = input.clone();
    changed["expected_revision"] = serde_json::json!(0);
    assert!(
        run_action_with_events(&state, project, serde_json::from_value(changed).unwrap())
            .unwrap_err()
            .contains("different payload")
    );
    let foreign = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    assert!(
        run_action_with_events(
            &state,
            foreign.path().to_str().unwrap(),
            serde_json::from_value(input).unwrap()
        )
        .unwrap_err()
        .contains("project")
    );
    let RunReply::Events(events) = run_action(
        project,
        RunAction::Events {
            run_id: run.id.clone(),
            after_sequence: 0,
            limit: 2,
        },
    )
    .unwrap() else {
        panic!("events");
    };
    assert_eq!(
        events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    let RunReply::Events(next) = run_action(
        project,
        RunAction::Events {
            run_id: run.id.clone(),
            after_sequence: 2,
            limit: 2,
        },
    )
    .unwrap() else {
        panic!("events");
    };
    assert_eq!(
        next.iter().map(|event| event.sequence).collect::<Vec<_>>(),
        [3, 4]
    );
    // Stop this test's actor without depending on whether missing-profile handling ran.
    let store = RunStore::open().unwrap();
    loop {
        match store.command(&run.id, "test-cancel", RunCommand::Cancel) {
            Ok(_) => break,
            // Cleanup races the actor's first turn: honor the service's explicit retry contract.
            Err(error) if error.contains("preflight; retry") => tokio::task::yield_now().await,
            Err(error) => panic!("cancel isolated test run: {error}"),
        }
    }
    state.workflow_runtime.wake(&run.id);
}

#[tokio::test]
async fn public_graph_start_requires_owner_and_refuses_plan_dispatch_visibly() {
    // Catches: public starts silently returning a record-only plan, or bypassing owner fencing.
    let (config, project, plan, _story, template, _guard) = fixture();
    let state = state(config.path());
    let input = serde_json::json!({"action":"start_graph", "target":{"type":"plan","id":plan},
        "definition_id":template, "definition_revision":1, "request_id":"plan-start"});
    assert!(
        run_action_with_events(
            &state,
            project.path().to_str().unwrap(),
            serde_json::from_value(input.clone()).unwrap()
        )
        .unwrap_err()
        .contains("executor unavailable")
    );
    WorkflowRuntime::spawn(&state);
    owner_ready(&state).await;
    let error = run_action_with_events(
        &state,
        project.path().to_str().unwrap(),
        serde_json::from_value(input).unwrap(),
    )
    .unwrap_err();
    assert!(error.contains("not executable"), "{error}");
    assert!(
        RunStore::open()
            .unwrap()
            .list_plan_runs(project.path().to_str().unwrap(), &plan, 20)
            .unwrap()
            .is_empty()
    );
}
