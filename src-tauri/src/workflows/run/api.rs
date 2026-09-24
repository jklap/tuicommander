use super::{RunCommand, RunEvent, RunLimits, RunReceipt, RunSnapshot, RunStore};
use serde::{Deserialize, Serialize};
#[cfg(feature = "desktop")]
use tauri::Emitter;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum RunAction {
    StartPlan {
        plan_id: String,
        definition_id: String,
        definition_revision: i64,
        limits: RunLimits,
    },
    Get {
        run_id: String,
    },
    Events {
        run_id: String,
        after_sequence: i64,
        limit: usize,
    },
    Command {
        run_id: String,
        command_id: String,
        expected_sequence: i64,
        command: RunCommand,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum RunReply {
    Snapshot(RunSnapshot),
    Events(Vec<RunEvent>),
    Receipt(RunReceipt),
}

pub fn run_action(project: &str, action: RunAction) -> Result<RunReply, String> {
    if !crate::fs::is_absolute_on_any_platform(project) {
        return Err("project must be an absolute path".into());
    }
    let owner = crate::progress::resolve_owning_project(Some(project))?
        .to_string_lossy()
        .to_string();
    let store = RunStore::open()?;
    match action {
        RunAction::StartPlan {
            plan_id,
            definition_id,
            definition_revision,
            limits,
        } => Ok(RunReply::Snapshot(store.start_plan(
            &owner,
            &plan_id,
            &definition_id,
            definition_revision,
            limits,
        )?)),
        RunAction::Get { run_id } => Ok(RunReply::Snapshot(scoped_snapshot(
            &store, &owner, &run_id,
        )?)),
        RunAction::Events {
            run_id,
            after_sequence,
            limit,
        } => {
            scoped_snapshot(&store, &owner, &run_id)?;
            Ok(RunReply::Events(store.events_after(
                &run_id,
                after_sequence,
                limit,
            )?))
        }
        RunAction::Command {
            run_id,
            command_id,
            expected_sequence,
            command,
        } => {
            scoped_snapshot(&store, &owner, &run_id)?;
            Ok(RunReply::Receipt(store.command_expected(
                &run_id,
                &command_id,
                expected_sequence,
                command,
            )?))
        }
    }
}

/// Send a wake hint after a committed mutation. The event stream remains the
/// source of truth when a client misses or receives duplicate hints.
pub fn run_action_with_events(
    state: &crate::state::AppState,
    project: &str,
    action: RunAction,
) -> Result<RunReply, String> {
    let mutation = matches!(
        action,
        RunAction::StartPlan { .. } | RunAction::Command { .. }
    );
    let reply = run_action(project, action)?;
    if mutation {
        let (repo_path, run_id, sequence) = match &reply {
            RunReply::Snapshot(snapshot) => (&snapshot.project, &snapshot.id, snapshot.sequence),
            RunReply::Receipt(receipt) => (
                &receipt.snapshot.project,
                &receipt.snapshot.id,
                receipt.sequence,
            ),
            RunReply::Events(_) => unreachable!("mutations return a snapshot or receipt"),
        };
        let payload = serde_json::json!({ "runId": run_id, "sequence": sequence });
        #[cfg(feature = "desktop")]
        if let Some(app) = state.app_handle.read().as_ref() {
            let _ = app.emit(
                "workflow-run-changed",
                serde_json::json!({
                    "repo_path": repo_path, "payload": &payload,
                }),
            );
        }
        let _ = state
            .event_bus
            .send(crate::state::AppEvent::WorkflowRunChanged {
                repo_path: repo_path.clone(),
                payload,
            });
    }
    Ok(reply)
}

fn scoped_snapshot(store: &RunStore, owner: &str, run_id: &str) -> Result<RunSnapshot, String> {
    let snapshot = store.snapshot(run_id)?;
    if snapshot.project != owner {
        return Err("workflow run does not belong to project".into());
    }
    Ok(snapshot)
}
