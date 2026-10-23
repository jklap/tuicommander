//! A story workflow on an install with no `sol`/`sonnet` profile, seen through the run store and the public API.
use super::*;

/// Start the Agent workflow under this saved config and wait until the daemon pauses it.
async fn paused_run_under(
    agents_json: serde_json::Value,
) -> (Arc<crate::state::AppState>, String, RunSnapshot) {
    let (config, project, _plan, story, _template, guard) = fixture();
    let project_path = project.path().to_str().unwrap().to_owned();
    let published = definition(&project_path, false, true);
    crate::config::save_agents_config(
        crate::config::AgentsConfig::default(),
        serde_json::from_value(agents_json).unwrap(),
    )
    .unwrap();
    let state = state(config.path());
    WorkflowRuntime::spawn(&state);
    owner_ready(&state).await;
    let mut hints = state.event_bus.subscribe();
    let run = state
        .workflow_runtime
        .start_graph(
            &state,
            &request(&project_path, &story, &published, "default-install"),
        )
        .unwrap();
    let store = RunStore::open().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            hints.recv().await.unwrap();
            if store.snapshot(&run.id).unwrap().status == RunStatus::Paused {
                break;
            }
        }
    })
    .await
    .expect("run did not pause");
    let current = store.snapshot(&run.id).unwrap();
    store
        .command(&run.id, "test:cancel", RunCommand::Cancel)
        .unwrap();
    // The fixture guard and temp dirs must outlive the run; leak them for the test's lifetime.
    std::mem::forget((config, project, guard));
    (state, project_path, current)
}

#[tokio::test]
async fn a_run_on_a_config_without_sol_or_sonnet_reaches_the_agent_spawn_effect() {
    // catches: the first Agent step pausing before any spawn because the profile names sol/sonnet are hardcoded.
    let (_state, _project, run) = paused_run_under(serde_json::json!({"agents": {
        "codex": {"codex_bypass_migrated": true},
        "claude": {"run_configs": [{"name": "mine", "command": "claude"}]}
    }}))
    .await;
    assert_eq!(
        run.effects.len(),
        1,
        "the run paused before reaching the agent spawn: {run:?}"
    );
}

#[tokio::test]
async fn incidents_name_the_pause_cause_the_daemon_knows() {
    // catches: Incidents saying no cause is retained while the pause reason is known.
    let (state, project, run) = paused_run_under(serde_json::json!({"agents": {
        "codex": {"codex_bypass_migrated": true},
        "claude": {"run_configs": [
            {"name": "alpha", "command": "claude"},
            {"name": "beta", "command": "claude"}
        ]}
    }}))
    .await;
    let crate::workflows::RunReply::Incidents(incidents) =
        crate::workflows::run_action_with_events(
            &state,
            &project,
            crate::workflows::RunAction::Incidents { run_id: run.id },
        )
        .unwrap()
    else {
        panic!("incidents expected");
    };
    assert_eq!(incidents.len(), 1, "{incidents:?}");
    assert!(
        !incidents[0].cause.contains("no more specific cause")
            && incidents[0].cause.contains("default"),
        "{}",
        incidents[0].cause
    );
}
