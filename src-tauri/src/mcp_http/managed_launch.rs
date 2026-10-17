//! Common managed-agent assembly; callers retain their own authorization fences.
use super::mcp_transport_session_agent::handle_session;
use crate::state::AppState;
use serde_json::{Value, json};
use std::{net::SocketAddr, sync::Arc};

/// Compatibility projection for existing run bindings. All creation and policy
/// now lives in the keyed transaction; call only from a blocking worker.
pub(super) fn launch(
    state: &Arc<AppState>,
    addr: SocketAddr,
    origin: Option<&str>,
    profile: &str,
    name: &str,
    prompt: &str,
    workspace: &str,
    owner_scope: &str,
    idempotency_key: &str,
    keep_open: bool,
) -> Value {
    let parent = origin.and_then(|origin| {
        state
            .mcp
            .to_session
            .get(origin)
            .map(|entry| entry.value().clone())
    });
    let request = json!({
        "idempotency_key":idempotency_key,
        "parent_session_id":parent.as_deref().unwrap_or(owner_scope),
        "name":name, "keep_open":keep_open,
        "workspace":{"existing_path":workspace},
        "spawn":{"agent_type":profile, "prompt":prompt},
    });
    let runtime = match tokio::runtime::Handle::try_current() {
        Ok(runtime) => runtime,
        Err(error) => {
            return json!({"error":format!("Managed launch requires the runtime worker: {error}")});
        }
    };
    let receipt = runtime.block_on(async {
        if parent.is_some() {
            super::launch_transaction::execute(state, addr, &request).await
        } else {
            super::launch_transaction::execute_owned(state, addr, &request).await
        }
    });
    if receipt.get("error").is_some() {
        return receipt;
    }
    if receipt["stage"] == "blocked" || receipt["live"] != true {
        return json!({"error":receipt["reason"], "launch_id":receipt["launch_id"]});
    }
    let mut child = receipt["child"].as_object().map(|value| Value::Object(value.clone())).unwrap_or_else(|| {
        let session = receipt["session_id"].as_str().unwrap_or_default();
        json!({"session_id":session, "task_id":state.tasks.live_ids_for_session(session).first(), "name":name})
    });
    child["launch_id"] = receipt["launch_id"].clone();
    child
}

/// A definition names a saved run configuration, never an implicit CLI fallback.
pub(crate) fn launch_named(
    state: &Arc<AppState>,
    profile: &str,
    name: &str,
    prompt: &str,
    workspace: &str,
    run_id: &str,
) -> Result<Value, String> {
    let config = crate::config::load_agents_config();
    if !config.agents.values().any(|agent| {
        agent
            .run_configs
            .iter()
            .any(|run| run.name.eq_ignore_ascii_case(profile))
    }) {
        return Err(format!(
            "Automation run configuration '{profile}' not found"
        ));
    }
    let spawned = launch(
        state,
        SocketAddr::from(([127, 0, 0, 1], 0)),
        None,
        profile,
        name,
        prompt,
        workspace,
        &format!("automation:{run_id}"),
        run_id,
        true,
    );
    if let Some(error) = spawned["error"].as_str() {
        return Err(error.into());
    }
    Ok(spawned)
}

pub(crate) fn stop(state: &Arc<AppState>, session: &str) -> Result<(), String> {
    let result = handle_session(state, &json!({"action":"kill", "session_id":session}), None);
    if let Some(error) = result["error"].as_str() {
        return Err(error.into());
    }
    Ok(())
}
