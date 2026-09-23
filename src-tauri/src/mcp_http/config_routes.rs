use crate::{AppState, MAX_CONCURRENT_SESSIONS};
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use std::net::SocketAddr;
use std::sync::Arc;

use super::guards::{Authenticated, require_local_or_auth};
use super::types::*;
use super::{json_result, validate_repo_path};

pub(super) async fn get_config(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    let config = state.config.read().clone();
    let mut json = match serde_json::to_value(config) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to serialize config: {e}")})),
            )
                .into_response();
        }
    };
    // Strip sensitive fields from nested services config
    if let Some(services) = json.pointer_mut("/services") {
        if let Some(auth) = services.pointer_mut("/auth")
            && let Some(o) = auth.as_object_mut()
        {
            o.remove("password_hash");
            o.remove("session_token");
        }
        if let Some(push) = services.pointer_mut("/push")
            && let Some(o) = push.as_object_mut()
        {
            o.remove("vapid_private_key");
        }
        if let Some(relay) = services.pointer_mut("/relay")
            && let Some(o) = relay.as_object_mut()
        {
            o.remove("token");
        }
    }
    Json(json).into_response()
}

pub(super) async fn put_config(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(incoming): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    // The merge runs INSIDE the config write lock (commit_config_change) so a partial
    // body is applied to the config as it is at write time, not to a snapshot another
    // writer has already replaced. Blocking pool: the critical section does disk I/O.
    let saved = {
        let state = state.clone();
        tokio::task::spawn_blocking(move || {
            crate::config::commit_config_change(&state, |current| {
                crate::config::merge_partial_app_config(current, incoming)
            })
        })
        .await
    };

    let effects = match saved {
        Ok(Ok(effects)) => effects,
        Ok(Err(e)) => {
            // A merge/validation failure is the caller's fault; a write failure is ours.
            let code = if e.starts_with("Invalid config") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            return (code, Json(serde_json::json!({"error": e})));
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("config save task failed: {e}")})),
            );
        }
    };

    if effects.tools_changed {
        let _ = state.mcp.tools_changed.send(());
    }
    // Parity with the IPC `save_config`: rebind the listener so the running process
    // cannot keep serving a config the disk disagrees with.
    if effects.server_changed {
        super::restart_after_server_settings_change(
            &state,
            "remote-access configuration changed over HTTP",
        );
    }
    // Parity with the IPC `save_config` — see its own comment. `tuic-remote`
    // (no `desktop` feature) has no `state.streamdock` field at all, since
    // `tuic_streamdock` is a desktop-only optional dependency.
    #[cfg(feature = "desktop")]
    if effects.streamdock_changed {
        let streamdock_state = state.clone();
        tokio::spawn(async move {
            streamdock_state
                .streamdock
                .apply_config(&streamdock_state)
                .await;
        });
    }
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(super) async fn hash_password_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(req): Json<HashPasswordRequest>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    // bcrypt at cost 12 is CPU-heavy (~300ms) — run it off the async runtime.
    let password = req.password;
    let hash_result = tokio::task::spawn_blocking(move || bcrypt::hash(&password, 12)).await;
    match hash_result {
        Ok(Ok(hash)) => (StatusCode::OK, Json(serde_json::json!({"hash": hash}))),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to hash: {e}")})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Hash task failed: {e}")})),
        ),
    }
}

pub(super) async fn rotate_session_token(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    // Blocking pool: the rotation takes the config write lock and touches disk.
    let rotated = tokio::task::spawn_blocking(move || crate::config::rotate_session_token(&state))
        .await
        .map_err(|e| format!("token rotation task failed: {e}"))
        .and_then(|r| r);
    if let Err(e) = rotated {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to persist token: {e}")})),
        );
    }
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(super) async fn get_notification_config() -> impl IntoResponse {
    Json(crate::config::load_notification_config())
}

pub(super) async fn put_notification_config(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<crate::config::NotificationConfig>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_notification_config(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

pub(super) async fn get_ui_prefs() -> impl IntoResponse {
    Json(crate::config::load_ui_prefs())
}

pub(super) async fn put_ui_prefs(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<crate::config::UIPrefsConfig>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_ui_prefs(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

pub(super) async fn get_repo_settings() -> impl IntoResponse {
    Json(crate::config::load_repo_settings())
}

pub(super) async fn put_repo_settings(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<crate::config::RepoSettingsMap>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_repo_settings(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

pub(super) async fn check_has_custom_settings_http(
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    Json(crate::config::check_has_custom_settings(q.path))
}

pub(super) async fn get_repositories() -> impl IntoResponse {
    Json(crate::config::load_repositories())
}

pub(super) async fn put_repositories(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Json(config): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_repositories_request(config) {
        Ok(changed) => {
            if changed {
                state.notify_repositories_changed();
            }
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
        Err(e) => {
            let status = match &e {
                crate::config::RepositorySaveError::Conflict(_) => StatusCode::CONFLICT,
                crate::config::RepositorySaveError::Invalid(_) => StatusCode::BAD_REQUEST,
                crate::config::RepositorySaveError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(serde_json::json!({"error": e.to_string()})))
        }
    }
}

pub(super) async fn get_stale_temp_repository_candidates() -> impl IntoResponse {
    Json(crate::config::list_stale_temp_repository_candidates())
}

#[derive(serde::Deserialize)]
pub(super) struct RepairStaleTempRepositoriesBody {
    paths: Vec<String>,
}

pub(super) async fn post_repair_stale_temp_repositories(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<RepairStaleTempRepositoriesBody>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::repair_stale_temp_repositories_request(body.paths) {
        Ok(summary) => {
            state.notify_repositories_changed();
            (StatusCode::OK, Json(serde_json::to_value(summary).unwrap()))
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

pub(super) async fn get_pane_layout() -> impl IntoResponse {
    Json(crate::config::load_pane_layout())
}

pub(super) async fn put_pane_layout(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(layout): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_pane_layout(layout) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

pub(super) async fn clear_caches(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    state.clear_caches();
    Json(serde_json::json!({"ok": true})).into_response()
}

/// HTTP/MCP equivalent of the desktop `clear_saved_scrollback` command — see
/// `scrollback_store.rs`. `{"session": "<tuic_session>"}` clears one tab;
/// an empty body (or `session: null`) clears every saved tab.
pub(super) async fn clear_saved_scrollback_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    body: Option<Json<serde_json::Value>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    let session = body
        .as_ref()
        .and_then(|Json(v)| v.get("session"))
        .and_then(|v| v.as_str());
    let result = match session {
        Some(tuic_session) => {
            crate::scrollback_store::clear(tuic_session);
            Ok(())
        }
        None => crate::scrollback_store::clear_all(),
    };
    match result {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

pub(super) async fn clear_repo_caches(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    if let Some(path) = body.get("path").and_then(|v| v.as_str()) {
        state.invalidate_repo_caches(path);
    }
    Json(serde_json::json!({"ok": true})).into_response()
}

pub(super) async fn get_repo_local_config(Query(q): Query<PathQuery>) -> impl IntoResponse {
    Json(crate::config::load_repo_local_config_from_path(
        std::path::Path::new(&q.path),
    ))
}

pub(super) async fn get_prompt_library() -> impl IntoResponse {
    Json(crate::config::load_prompt_library())
}

pub(super) async fn put_prompt_library(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<crate::config::PromptLibraryConfig>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_prompt_library(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- Repo Defaults ---

pub(super) async fn get_repo_defaults() -> impl IntoResponse {
    Json(crate::config::load_repo_defaults())
}

pub(super) async fn put_repo_defaults(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<crate::config::RepoDefaultsConfig>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_repo_defaults(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- Notes ---

pub(super) async fn get_notes() -> impl IntoResponse {
    match crate::config::load_notes() {
        Ok(v) => (StatusCode::OK, Json(v)),
        // 500 so the client stays un-hydrated and never overwrites the file it could not read.
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

pub(super) async fn put_notes(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_notes(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- Activity ---

pub(super) async fn get_activity() -> impl IntoResponse {
    Json(crate::config::load_activity())
}

pub(super) async fn put_activity(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(items): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_activity(items) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- Keybindings ---

pub(super) async fn get_keybindings() -> impl IntoResponse {
    Json(crate::config::load_keybindings())
}

pub(super) async fn put_keybindings(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_keybindings(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- Agents Config ---

pub(super) async fn get_agents_config() -> impl IntoResponse {
    Json(crate::config::load_agents_config())
}

pub(super) async fn put_agents_config(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<crate::config::AgentsConfig>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_agents_config(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- Agent hook instrumentation (browser-mode parity for the toggle) ---

pub(super) async fn get_agent_hook_state(Path(agent): Path<String>) -> impl IntoResponse {
    Json(serde_json::json!({
        "state": crate::agent_hook_commands::get_agent_hook_state(agent),
    }))
}

pub(super) async fn put_agent_hook_instrumentation(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Path(agent): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    let enabled = body
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    match crate::agent_hook_commands::set_agent_hook_instrumentation(agent, enabled) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

pub(super) async fn get_agent_native_status_signals(
    Path(agent): Path<String>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "enabled": crate::agent_hook_commands::get_agent_native_status_signals(agent),
    }))
}

pub(super) async fn put_agent_native_status_signals(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Path(agent): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    let Some(enabled) = body.get("enabled").and_then(serde_json::Value::as_bool) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "enabled must be a boolean"})),
        );
    };
    match crate::agent_hook_commands::set_agent_native_status_signals(agent, enabled) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- Provider Registry ---

pub(super) async fn get_provider_registry() -> impl IntoResponse {
    Json(crate::provider_registry::load_provider_registry())
}

pub(super) async fn put_provider_registry(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(registry): Json<crate::provider_registry::ProviderRegistry>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::provider_registry::save_provider_registry(registry) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- Provider API keys (keyring-proxied — story 072) ---
// State-free: the underlying commands talk to the credential keyring directly,
// so the HTTP handlers call them verbatim (loopback router, local trust boundary).

#[derive(serde::Deserialize)]
pub(super) struct ProviderIdRef {
    #[serde(rename = "providerId")]
    pub provider_id: String,
}

#[derive(serde::Deserialize)]
pub(super) struct SaveProviderKeyReq {
    #[serde(rename = "providerId")]
    pub provider_id: String,
    pub key: String,
}

#[derive(serde::Deserialize)]
pub(super) struct SlotTestReq {
    pub slot: crate::provider_registry::SlotName,
}

pub(super) async fn provider_key_exists_http(Query(q): Query<ProviderIdRef>) -> Response {
    json_result(crate::provider_registry::get_provider_api_key_exists(
        q.provider_id,
    ))
}

pub(super) async fn save_provider_key_http(Json(b): Json<SaveProviderKeyReq>) -> Response {
    json_result(crate::provider_registry::save_provider_api_key(
        b.provider_id,
        b.key,
    ))
}

pub(super) async fn delete_provider_key_http(Json(b): Json<ProviderIdRef>) -> Response {
    json_result(crate::provider_registry::delete_provider_api_key(
        b.provider_id,
    ))
}

pub(super) async fn test_slot_connection_http(Json(b): Json<SlotTestReq>) -> Response {
    json_result(crate::provider_registry::test_slot_connection(b.slot).await)
}

pub(super) async fn check_ollama_models_http(Json(b): Json<ProviderIdRef>) -> impl IntoResponse {
    Json(crate::provider_registry::check_ollama_models(b.provider_id).await)
}

// --- MCP Status ---

pub(super) async fn get_mcp_status_http(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Real connect attempt — file.exists() is unreliable for stale sockets.
    #[cfg(unix)]
    let running = tokio::net::UnixStream::connect(super::socket_path())
        .await
        .is_ok();
    #[cfg(not(unix))]
    let running = false;
    Json(serde_json::json!({
        "enabled": true,
        "running": running,
        "active_sessions": state.session_maps.sessions.len(),
        "mcp_clients": state.mcp.sessions.len(),
        "max_sessions": MAX_CONCURRENT_SESSIONS,
    }))
}

// ---------------------------------------------------------------------------
// Remote connections
// ---------------------------------------------------------------------------

pub(super) async fn get_remote_connections(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    match crate::remote_connection::RemoteConnectionStore::load(&state.data_dir) {
        Ok(connections) => match serde_json::to_value(connections) {
            Ok(v) => Json(v).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to serialize connections: {e}")})),
            )
                .into_response(),
        },
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub(super) async fn put_remote_connection(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Json(connection): Json<crate::remote_connection::RemoteConnection>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    // Validate up front so a malformed request gets a precise 400 (the shared
    // upsert helper re-validates as its canonical gate — that path stays a 500).
    if let Err(e) = connection.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }
    let _guard = state.connections_lock.lock().await;
    match crate::remote_connection::upsert_remote_connection(&state.data_dir, connection) {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

pub(super) async fn delete_remote_connection(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    let _guard = state.connections_lock.lock().await;
    // Shared with the Tauri `delete_remote_connection` command
    // (`remote_connection::delete_remote_connection`) — see
    // `delete_remote_connection_impl`'s doc comment (plan Phase 1) for why
    // this used to only happen on the HTTP side.
    match crate::remote_connection::delete_remote_connection_impl(&state, &id) {
        Ok(true) => Json(serde_json::json!({"ok": true})).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("connection '{id}' not found")})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

/// POST /config/remote-connections/test — Test Connection (story: SSH
/// Tunnels + Remote Servers consolidation, Phase 2). Shared with the Tauri
/// `test_connection` command via `connection_test::test_connection_impl`.
/// Guarded the same way as `put_remote_connection`/`delete_remote_connection`
/// above: it accepts a plaintext password (never persisted) and can make the
/// backend open outbound SSH/HTTP connections to an arbitrary
/// caller-supplied host, so an unauthenticated remote caller must never
/// reach it.
pub(super) async fn test_connection_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(request): Json<crate::connection_test::TestConnectionRequest>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    Json(crate::connection_test::test_connection_impl(&request).await).into_response()
}

// --- Direct connection TLS proxy (story: SSH Tunnels + Remote Servers
// consolidation, Phase 4 — "Self-signed HTTPS for Direct") ---
//
// Guarded the same way as test_connection_http above: these can make the
// backend open outbound connections to an arbitrary caller-supplied host
// (probe) or start a long-lived local listener + outbound relay (start), so
// an unauthenticated remote caller must never reach them.

#[derive(serde::Deserialize)]
pub(super) struct ProbeDirectTlsRequest {
    pub url: String,
    #[serde(default)]
    pub tls_fingerprint: Option<String>,
}

pub(super) async fn probe_direct_tls_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(request): Json<ProbeDirectTlsRequest>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    json_result(
        crate::direct_proxy::probe_direct_tls(&request.url, request.tls_fingerprint.as_deref()).await,
    )
    .into_response()
}

#[derive(serde::Deserialize)]
pub(super) struct StartDirectProxyRequest {
    pub url: String,
    #[serde(default)]
    pub tls_fingerprint: Option<String>,
    #[serde(default)]
    pub use_native_roots: bool,
}

pub(super) async fn start_direct_proxy_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Path(connection_id): Path<String>,
    Json(request): Json<StartDirectProxyRequest>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    json_result(
        crate::direct_proxy::start_direct_proxy_impl(
            &state,
            &connection_id,
            &request.url,
            request.tls_fingerprint.as_deref(),
            request.use_native_roots,
        )
        .await,
    )
    .into_response()
}

pub(super) async fn stop_direct_proxy_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Path(connection_id): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    state.direct_proxy_manager.stop(&connection_id);
    Json(serde_json::json!({"ok": true})).into_response()
}

// --- Remote connection password (keyring) — plan Phase 3 auth wiring ---
//
// Auth-gated the same way as put_remote_connection/delete_remote_connection
// above (unlike the provider-key routes, which are State-free/loopback-only):
// a remote connection's password can be set/read-existence/cleared over a
// connection reachable to an unauthenticated LAN caller otherwise.

#[derive(serde::Deserialize)]
pub(super) struct SaveRemoteConnectionPasswordReq {
    pub password: String,
}

pub(super) async fn remote_connection_password_exists_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    json_result(crate::remote_connection::remote_connection_password_exists(
        id,
    ))
    .into_response()
}

pub(super) async fn save_remote_connection_password_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Path(id): Path<String>,
    Json(body): Json<SaveRemoteConnectionPasswordReq>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    json_result(crate::remote_connection::save_remote_connection_password(
        id,
        body.password,
    ))
    .into_response()
}

pub(super) async fn delete_remote_connection_password_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    json_result(crate::remote_connection::delete_remote_connection_password(
        id,
    ))
    .into_response()
}

// --- Story 066: config / themes / notes / misc stateless parity (loopback router) ---
//
// Mutating / action handlers carry the same `require_local_or_auth` guard as the
// other config writes in this file. Pure reads skip it (matching get_prompt_library
// / get_repo_local_config). `/exec/shell-script` and `/agent/open-in-custom` run
// processes, so they are guarded.

pub(super) async fn get_ai_prompts_http() -> impl IntoResponse {
    Json(crate::config::load_ai_prompts())
}

pub(super) async fn put_ai_prompts_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<crate::config::AiPromptsConfig>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    json_result(crate::config::save_ai_prompts(config))
}

pub(super) async fn save_repo_local_config_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<SaveRepoLocalConfigRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    json_result(crate::config::save_repo_local_config(body.repo_path))
}

pub(super) async fn set_branch_label_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<SetBranchLabelRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    json_result(crate::config::set_branch_label(
        body.repo_path,
        body.branch_name,
        body.label,
    ))
}

pub(super) async fn save_note_image_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<SaveNoteImageRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    json_result(crate::config::save_note_image(
        body.note_id,
        body.data_base64,
        body.extension,
    ))
}

pub(super) async fn delete_note_assets_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<DeleteNoteAssetsRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    json_result(crate::config::delete_note_assets(body.note_id))
}

pub(super) async fn delete_note_assets_batch_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<DeleteNoteAssetsBatchRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    json_result(crate::config::delete_note_assets_batch(body.note_ids))
}

pub(super) async fn list_themes_http() -> impl IntoResponse {
    let themes_dir = crate::config::config_dir().join("themes");
    Json(crate::themes::load_themes(&themes_dir))
}

pub(super) async fn execute_shell_script_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<ExecuteShellScriptRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    json_result(
        crate::smart_prompt::execute_shell_script(
            body.script_content,
            body.timeout_ms,
            body.repo_path,
        )
        .await,
    )
}

pub(super) async fn list_audio_output_devices_http() -> impl IntoResponse {
    // `notification_sound` is desktop-only; a headless remote daemon has no audio
    // output context, so it reports an empty device list.
    #[cfg(feature = "desktop")]
    let devices = crate::notification_sound::list_output_devices();
    #[cfg(not(feature = "desktop"))]
    let devices: Vec<serde_json::Value> = Vec::new();
    Json(devices)
}

pub(super) async fn discover_agent_session_http(
    Json(body): Json<DiscoverAgentSessionRequest>,
) -> impl IntoResponse {
    Json(crate::agent_session::discover_agent_session(
        body.agent_type,
        body.cwd,
        body.claimed_ids,
        body.agent_pid,
        body.env_overrides,
    ))
}

pub(super) async fn claude_project_dir_http(
    Json(body): Json<ClaudeProjectDirRequest>,
) -> axum::response::Response {
    json_result(crate::agent_session::claude_project_dir(
        body.cwd,
        body.claude_config_dir,
    ))
}

pub(super) async fn open_in_custom_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<OpenInCustomRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    json_result(crate::agent::open_in_custom(
        body.executable,
        body.args,
        body.ctx,
    ))
}

pub(super) async fn generate_value_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<GenerateValueRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    json_result(crate::generators::generate_value(body.request))
}

pub(super) async fn fetch_plugin_registry_http() -> axum::response::Response {
    json_result(crate::registry::fetch_plugin_registry().await)
}

pub(super) async fn set_project_mcp_upstreams_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<SetProjectMcpUpstreamsRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    json_result(crate::mcp_upstream_config::set_project_mcp_upstreams_inner(
        &state,
        &body.repo_path,
        body.upstream_names,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loopback() -> SocketAddr {
        "127.0.0.1:1".parse().unwrap()
    }
    fn lan() -> SocketAddr {
        "192.168.1.2:1".parse().unwrap()
    }
    fn authed() -> Option<Extension<Authenticated>> {
        Some(Extension(Authenticated))
    }
    fn req() -> Json<HashPasswordRequest> {
        Json(HashPasswordRequest {
            password: "hunter2".to_string(),
        })
    }

    // hash_password_http is a representative config route: it shares the exact
    // `require_local_or_auth(&addr, auth.is_some())` guard every config handler
    // uses, and needs no AppState — so it cleanly proves the config guard wiring.

    #[tokio::test]
    async fn config_guard_loopback_passes() {
        let resp = hash_password_http(ConnectInfo(loopback()), None, req())
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn config_guard_authenticated_remote_passes() {
        let resp = hash_password_http(ConnectInfo(lan()), authed(), req())
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn config_guard_unauthenticated_remote_rejected() {
        let resp = hash_password_http(ConnectInfo(lan()), None, req())
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn clear_saved_scrollback_http_rejects_unauthenticated_remote() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let resp = clear_saved_scrollback_http(ConnectInfo(lan()), None, None)
            .await
            .into_response();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn clear_saved_scrollback_http_with_no_body_clears_every_tab() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        crate::scrollback_store::save("tuic-a", &[], 80, 1).unwrap();
        crate::scrollback_store::save("tuic-b", &[], 80, 1).unwrap();

        let resp = clear_saved_scrollback_http(ConnectInfo(loopback()), None, None)
            .await
            .into_response();

        assert_eq!(resp.status(), StatusCode::OK);
        assert!(crate::scrollback_store::load("tuic-a").is_none());
        assert!(crate::scrollback_store::load("tuic-b").is_none());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn clear_saved_scrollback_http_with_a_session_clears_only_that_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        crate::scrollback_store::save("tuic-a", &[], 80, 1).unwrap();
        crate::scrollback_store::save("tuic-b", &[], 80, 1).unwrap();

        let resp = clear_saved_scrollback_http(
            ConnectInfo(loopback()),
            None,
            Some(Json(serde_json::json!({"session": "tuic-a"}))),
        )
        .await
        .into_response();

        assert_eq!(resp.status(), StatusCode::OK);
        assert!(crate::scrollback_store::load("tuic-a").is_none());
        assert!(crate::scrollback_store::load("tuic-b").is_some());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn clear_saved_scrollback_http_treats_null_session_as_clear_all() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        crate::scrollback_store::save("tuic-a", &[], 80, 1).unwrap();

        let resp = clear_saved_scrollback_http(
            ConnectInfo(loopback()),
            None,
            Some(Json(serde_json::json!({"session": null}))),
        )
        .await
        .into_response();

        assert_eq!(resp.status(), StatusCode::OK);
        assert!(crate::scrollback_store::load("tuic-a").is_none());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn stale_repository_delta_returns_http_conflict() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let original = serde_json::json!({"path":"/repo","displayName":"Original","branches":{}});
        crate::config::replace_repositories_for_test(serde_json::json!({
            "repos": {"/repo": original.clone()},
            "repoOrder": ["/repo"]
        }))
        .expect("seed repositories");
        crate::config::save_repositories_request(serde_json::json!({
            "mutationVersion": 1,
            "repos": [{
                "id":"/repo",
                "before":original.clone(),
                "after":{"path":"/repo","displayName":"First","branches":{}}
            }],
            "groups": []
        }))
        .expect("first mutation");

        let response = put_repositories(
            ConnectInfo(loopback()),
            None,
            State(super::super::tests::test_state()),
            Json(serde_json::json!({
                "mutationVersion": 1,
                "repos": [{
                    "id":"/repo",
                    "before":original,
                    "after":{"path":"/repo","displayName":"Stale","branches":{}}
                }],
                "groups": []
            })),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    /// The whole point of the write is the announcement: a second client only learns
    /// the document moved because this fires. Both tests subscribe before the request,
    /// so a broadcast dropped or made unconditional fails here rather than in a
    /// two-window session nobody can reproduce on demand.
    #[tokio::test]
    #[serial_test::serial]
    async fn an_accepted_delta_announces_the_change() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let state = super::super::tests::test_state();
        let mut events = state.event_bus.subscribe();

        let response = put_repositories(
            ConnectInfo(loopback()),
            None,
            State(state.clone()),
            Json(serde_json::json!({
                "mutationVersion": 1,
                "repos": [{
                    "id":"/repo",
                    "before":null,
                    "after":{"path":"/repo","displayName":"Added","branches":{}}
                }],
                "groups": []
            })),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            matches!(
                events.try_recv(),
                Ok(crate::state::AppEvent::RepositoriesChanged)
            ),
            "an accepted delta must announce itself on the event bus"
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn a_delta_that_changes_nothing_stays_quiet() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let record = serde_json::json!({"path":"/repo","displayName":"Added","branches":{}});
        crate::config::replace_repositories_for_test(serde_json::json!({
            "repos": {"/repo": record.clone()},
            "repoOrder": ["/repo"]
        }))
        .expect("seed repositories");
        let state = super::super::tests::test_state();
        let mut events = state.event_bus.subscribe();

        // Same record, already on disk: accepted, but nothing moved. Announcing it
        // would make every client re-read for nothing.
        let response = put_repositories(
            ConnectInfo(loopback()),
            None,
            State(state.clone()),
            Json(serde_json::json!({
                "mutationVersion": 1,
                "repos": [{"id":"/repo","before":record.clone(),"after":record}],
                "groups": []
            })),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            events.try_recv().is_err(),
            "a no-op delta must not announce a change"
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn malformed_repository_delta_returns_http_bad_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let response = put_repositories(
            ConnectInfo(loopback()),
            None,
            State(super::super::tests::test_state()),
            Json(serde_json::json!({
                "mutationVersion": 1,
                "repos": [{"id":"/repo","before":null,"after":"not-an-object"}],
                "groups": []
            })),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn unversioned_repository_document_returns_http_bad_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let response = put_repositories(
            ConnectInfo(loopback()),
            None,
            State(super::super::tests::test_state()),
            Json(serde_json::json!({"repos": {}, "repoOrder": []})),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn list_audio_output_devices_http_returns_a_device_array() {
        let resp = list_audio_output_devices_http().await.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("valid JSON");
        assert!(value.is_array(), "response body must be a JSON array");

        // A headless (non-desktop) build has no audio output context at all and
        // must report an empty list rather than erroring.
        #[cfg(not(feature = "desktop"))]
        assert_eq!(
            value.as_array().unwrap().len(),
            0,
            "non-desktop builds report no audio devices"
        );
    }

    // ── delete_remote_connection ─────────────────────────────
    //
    // Thin wiring tests: the actual logic (stop-tunnel-then-delete, keyring
    // cleanup, not-found handling) is shared with the Tauri command via
    // `remote_connection::delete_remote_connection_impl` and tested directly
    // there — these just confirm this route maps that shared result to the
    // right HTTP status/body (plan Phase 1).

    #[tokio::test]
    async fn delete_remote_connection_http_returns_404_for_a_missing_connection() {
        let state = super::super::tests::test_state();
        let resp = delete_remote_connection(
            ConnectInfo(loopback()),
            None,
            State(state),
            Path("does-not-exist".to_string()),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_remote_connection_http_removes_an_existing_connection() {
        let state = super::super::tests::test_state();
        let conn = crate::remote_connection::RemoteConnection::new_ssh("t", "h", "u");
        let id = conn.id.clone();
        crate::remote_connection::upsert_remote_connection(&state.data_dir, conn).unwrap();

        let resp = delete_remote_connection(
            ConnectInfo(loopback()),
            None,
            State(state.clone()),
            Path(id.clone()),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            crate::remote_connection::RemoteConnectionStore::load(&state.data_dir)
                .unwrap()
                .is_empty()
        );
    }

    // ── remote connection password routes ─────────────────────
    //
    // Thin wiring tests, same shape as delete_remote_connection_http's above:
    // the actual keyring CRUD is shared with the Tauri commands and tested
    // directly in `remote_connection::tests` — these just confirm the routes
    // are auth-gated and delegate to the shared functions correctly.

    #[tokio::test]
    async fn remote_connection_password_routes_require_local_or_auth() {
        crate::credentials::reset_test_faults();
        let not_local = std::net::SocketAddr::from(([203, 0, 113, 5], 12345));
        let id = uuid::Uuid::new_v4().to_string();

        let resp =
            remote_connection_password_exists_http(ConnectInfo(not_local), None, Path(id.clone()))
                .await
                .into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let resp = save_remote_connection_password_http(
            ConnectInfo(not_local),
            None,
            Path(id.clone()),
            Json(SaveRemoteConnectionPasswordReq {
                password: "x".to_string(),
            }),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let resp = delete_remote_connection_password_http(ConnectInfo(not_local), None, Path(id))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn remote_connection_password_http_round_trips() {
        crate::credentials::reset_test_faults();
        let id = uuid::Uuid::new_v4().to_string();

        let resp =
            remote_connection_password_exists_http(ConnectInfo(loopback()), None, Path(id.clone()))
                .await
                .into_response();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"false");

        let resp = save_remote_connection_password_http(
            ConnectInfo(loopback()),
            None,
            Path(id.clone()),
            Json(SaveRemoteConnectionPasswordReq {
                password: "hunter2".to_string(),
            }),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp =
            remote_connection_password_exists_http(ConnectInfo(loopback()), None, Path(id.clone()))
                .await
                .into_response();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"true");

        let resp =
            delete_remote_connection_password_http(ConnectInfo(loopback()), None, Path(id.clone()))
                .await
                .into_response();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = remote_connection_password_exists_http(ConnectInfo(loopback()), None, Path(id))
            .await
            .into_response();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"false");
    }

    // ── test_connection_http ─────────────────────────────────
    //
    // Thin wiring test: the actual classification logic (SSH one-shot check,
    // HTTP health-check dispatch, Local instance-id resolution) is shared
    // with the Tauri command via `connection_test::test_connection_impl` and
    // tested directly there (`connection_test::tests`) — this just confirms
    // the route deserializes the request body and returns that shared
    // result as JSON (plan Phase 2, mirrors `delete_remote_connection_http`'s
    // own "thin wiring" test comment above).

    #[tokio::test]
    async fn test_connection_http_returns_the_shared_classification_as_json() {
        // Port 1 is reserved and nothing listens there — a fast, reliable
        // Unreachable classification with no real network dependency.
        let resp = test_connection_http(
            ConnectInfo(loopback()),
            None,
            Json(crate::connection_test::TestConnectionRequest {
                transport: crate::remote_connection::RemoteTransport::Local {
                    port: Some(1),
                    instance_id: None,
                },
                auth_username: None,
                password: None,
            }),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("valid JSON");
        assert_eq!(json["type"], "Unreachable");
    }
}
