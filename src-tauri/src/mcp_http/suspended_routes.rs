//! One IPC/HTTP entry point for durable tabs and verified resume commands.
use crate::{AppState, suspended_sessions};
use axum::{
    Extension, Json,
    extract::{ConnectInfo, State},
    response::{IntoResponse, Response},
};
use std::sync::Arc;

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn suspended_session_action(
    state: tauri::State<'_, Arc<AppState>>,
    request: serde_json::Value,
) -> Result<serde_json::Value, String> {
    dispatch(state.inner().clone(), request).await
}

pub(crate) async fn dispatch(
    state: Arc<AppState>,
    request: serde_json::Value,
) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || action(&state, &request))
        .await
        .map_err(|e| e.to_string())?
}

fn action(state: &Arc<AppState>, request: &serde_json::Value) -> Result<serde_json::Value, String> {
    match request["action"].as_str().unwrap_or_default() {
        "list" => serde_json::to_value(suspended_sessions::list()?).map_err(|e| e.to_string()),
        "resolve_resume" => {
            let input =
                serde_json::from_value::<crate::session_resume::ResumeRequest>(request.clone())
                    .map_err(|e| e.to_string())?;
            serde_json::to_value(crate::session_resume::resolve(&input)).map_err(|e| e.to_string())
        }
        "suspend" => {
            let session_id = request["sessionId"]
                .as_str()
                .ok_or("sessionId is required")?;
            suspended_sessions::suspend(state, session_id, &request["metadata"])?;
            Ok(serde_json::json!({"ok": true}))
        }
        "sync" => {
            let session_id = request["sessionId"]
                .as_str()
                .ok_or("sessionId is required")?;
            if state.session_maps.sessions.contains_key(session_id) {
                state
                    .suspend_metadata
                    .insert(session_id.into(), request["metadata"].clone());
            }
            Ok(serde_json::json!({"ok": true}))
        }
        "forget" => {
            suspended_sessions::remove(
                request["tuicSession"]
                    .as_str()
                    .ok_or("tuicSession is required")?,
            )?;
            Ok(serde_json::json!({"ok": true}))
        }
        "resume" => {
            let _guard = state.suspend_lock.lock();
            let identity = request["tuicSession"]
                .as_str()
                .ok_or("tuicSession is required")?;
            let records = suspended_sessions::list()?;
            let record = records.get(identity).ok_or("not suspended")?;
            let command = if record.agent_type.is_some() {
                Some(
                    crate::session_resume::resolve(&record.resume_request())
                        .cmd
                        .ok_or("no resumable agent session")?,
                )
            } else {
                None
            };
            if state.session_maps.sessions.len() >= crate::MAX_CONCURRENT_SESSIONS {
                return Err("Max concurrent sessions reached".into());
            }
            if state
                .session_maps
                .live_pty_by_tuic_session
                .contains_key(identity)
            {
                return Err("session is already live".into());
            }
            let session_id = super::session::spawn_pty_session(
                state.clone(),
                record.shell.clone(),
                Some(record.cwd.clone()),
                24,
                80,
                record.worktree.clone(),
                super::session::RequestedIdentity {
                    session_id: None,
                    tuic_session: Some(identity.into()),
                    alias: record.alias.clone(),
                    startup_command: command,
                },
            )
            .map_err(|(_, Json(error))| error.to_string())?;
            if let Err(error) = suspended_sessions::remove(identity) {
                crate::pty::close_pty_core_with_reason(
                    state,
                    &session_id,
                    false,
                    "resume_storage_failed",
                );
                return Err(error);
            }
            Ok(serde_json::json!({"ok": true, "session_id": session_id}))
        }
        _ => Err("Unknown suspended session action".into()),
    }
}

pub(super) async fn action_http(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    auth: Option<Extension<super::guards::Authenticated>>,
    Json(request): Json<serde_json::Value>,
) -> Response {
    if let Err(response) = super::guards::require_local_or_auth(&addr, auth.is_some()) {
        return response.into_response();
    }
    match dispatch(state, request).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": error})),
        )
            .into_response(),
    }
}
