use std::io::BufReader;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use super::profile::TunnelProfile;
use super::storage::ProfileStore;
use crate::AppState;

/// JSON error helper.
fn err_json(status: StatusCode, msg: &str) -> Response {
    (status, Json(serde_json::json!({"error": msg}))).into_response()
}

// ── Profile CRUD ────────────────────────────────────────────

/// GET /tunnels/profiles — list all saved profiles.
pub(crate) async fn list_tunnel_profiles(State(state): State<Arc<AppState>>) -> Response {
    match ProfileStore::load_all(&state.data_dir, None) {
        Ok(profiles) => (StatusCode::OK, Json(serde_json::json!(profiles))).into_response(),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// Error from [`save_tunnel_profile_impl`] — kept as two variants (rather
/// than a single `String`) so each transport can map it to its own idiomatic
/// shape: the HTTP route needs a 400 for a validation failure vs. a 500 for
/// a storage failure, while the Tauri command flattens both into one
/// `Result<String, String>`.
#[derive(Debug)]
pub(crate) enum SaveTunnelProfileError {
    /// Bad JSON shape, or `TunnelProfile::validate()` rejected it.
    Validation(String),
    /// `ProfileStore::save` failed (disk I/O).
    Storage(String),
}

/// Shared body of the HTTP `save_tunnel_profile` route and the Tauri
/// `save_tunnel_profile` command (`tauri_commands::save_tunnel_profile`) —
/// mints a fresh UUID for an empty/missing `id` before validating, so both
/// transports agree on "empty id means generate one" instead of one
/// generating a UUID and the other rejecting it as "not a valid UUID" (see
/// this file's test module history: `known_bug_http_save_does_not_generate_an_id_for_an_empty_id`,
/// now fixed). Takes a plain `&Path` rather than `&AppState` since it's the
/// only field either caller needs, and a raw `serde_json::Value` rather than
/// `TunnelProfile` directly so the empty-id check can run before
/// deserialization, exactly like the Tauri command always did.
pub(crate) fn save_tunnel_profile_impl(
    data_dir: &std::path::Path,
    mut profile_json: serde_json::Value,
) -> Result<String, SaveTunnelProfileError> {
    if profile_json
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .is_empty()
    {
        profile_json["id"] = serde_json::Value::String(uuid::Uuid::new_v4().to_string());
    }
    let mut profile: TunnelProfile = serde_json::from_value(profile_json)
        .map_err(|e| SaveTunnelProfileError::Validation(e.to_string()))?;
    profile
        .validate()
        .map_err(SaveTunnelProfileError::Validation)?;
    let id = profile.id.clone();
    ProfileStore::save(data_dir, &profile)
        .map_err(|e| SaveTunnelProfileError::Storage(e.to_string()))?;
    Ok(id)
}

/// POST /tunnels/profiles — create or update a profile.
pub(crate) async fn save_tunnel_profile(
    State(state): State<Arc<AppState>>,
    Json(profile_json): Json<serde_json::Value>,
) -> Response {
    match save_tunnel_profile_impl(&state.data_dir, profile_json) {
        Ok(id) => (StatusCode::OK, Json(serde_json::json!({"id": id}))).into_response(),
        Err(SaveTunnelProfileError::Validation(e)) => err_json(StatusCode::BAD_REQUEST, &e),
        Err(SaveTunnelProfileError::Storage(e)) => {
            err_json(StatusCode::INTERNAL_SERVER_ERROR, &e)
        }
    }
}

/// Error from [`delete_tunnel_profile_impl`] — see
/// [`SaveTunnelProfileError`]'s doc comment for why this isn't a single
/// `String`: the HTTP route needs a 404 for "not found" vs. a 500 for a real
/// storage failure.
#[derive(Debug)]
pub(crate) enum DeleteTunnelProfileError {
    /// No profile with this id exists (repo or global scope).
    NotFound,
    /// `ProfileStore::delete` failed (disk I/O).
    Storage(String),
}

/// Shared body of the HTTP `delete_tunnel_profile` route and the Tauri
/// `delete_tunnel_profile` command — stops any running tunnel for this
/// profile id, then deletes it, treating "no such profile" as an error on
/// both transports (previously the Tauri command returned `Ok(false)`
/// silently for this case while HTTP returned 404 — see this file's test
/// module history: `known_bug_http_delete_returns_404_for_a_missing_profile`,
/// now fixed by making the Tauri side error too, matching HTTP's existing,
/// more REST-idiomatic behavior).
pub(crate) fn delete_tunnel_profile_impl(
    state: &AppState,
    id: &str,
) -> Result<(), DeleteTunnelProfileError> {
    state.tunnel_manager.stop_if_running(id);
    match ProfileStore::delete(&state.data_dir, None, id) {
        Ok(true) => Ok(()),
        Ok(false) => Err(DeleteTunnelProfileError::NotFound),
        Err(e) => Err(DeleteTunnelProfileError::Storage(e.to_string())),
    }
}

/// DELETE /tunnels/profiles/:id — delete a profile, stopping its tunnel if active.
pub(crate) async fn delete_tunnel_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match delete_tunnel_profile_impl(&state, &id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"deleted": true}))).into_response(),
        Err(DeleteTunnelProfileError::NotFound) => {
            err_json(StatusCode::NOT_FOUND, "profile not found")
        }
        Err(DeleteTunnelProfileError::Storage(e)) => {
            err_json(StatusCode::INTERNAL_SERVER_ERROR, &e)
        }
    }
}

// ── Tunnel lifecycle ────────────────────────────────────────

/// POST /tunnels/start/:id — load profile from storage and start its tunnel.
pub(crate) async fn start_tunnel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    let profiles = match ProfileStore::load_all(&state.data_dir, None) {
        Ok(p) => p,
        Err(e) => return err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let profile = match profiles.into_iter().find(|p| p.id == id) {
        Some(p) => p,
        None => return err_json(StatusCode::NOT_FOUND, "profile not found"),
    };

    let result = state.tunnel_manager.start(profile).await;

    match result {
        Ok(tunnel_id) => {
            (StatusCode::OK, Json(serde_json::json!({"id": tunnel_id}))).into_response()
        }
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

/// POST /tunnels/stop/:id — stop an active tunnel.
pub(crate) async fn stop_tunnel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match state.tunnel_manager.stop(&id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"stopped": true}))).into_response(),
        Err(e) => err_json(StatusCode::NOT_FOUND, &e),
    }
}

// ── Status queries ──────────────────────────────────────────

/// GET /tunnels/active — list all running tunnels with status.
pub(crate) async fn list_active_tunnels(State(state): State<Arc<AppState>>) -> Response {
    let list = state.tunnel_manager.list();
    let entries: Vec<serde_json::Value> = list
        .into_iter()
        .map(|(id, status)| serde_json::json!({"id": id, "status": status}))
        .collect();
    (StatusCode::OK, Json(serde_json::json!(entries))).into_response()
}

/// GET /tunnels/status/:id — single tunnel status.
pub(crate) async fn get_tunnel_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match state.tunnel_manager.get_status(&id) {
        Some(status) => (
            StatusCode::OK,
            Json(serde_json::json!({"id": id, "status": status})),
        )
            .into_response(),
        None => err_json(StatusCode::NOT_FOUND, "tunnel not found"),
    }
}

// ── Audit log ───────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct AuditQuery {
    limit: Option<usize>,
}

/// GET /tunnels/audit/:id — audit log for a tunnel.
pub(crate) async fn get_tunnel_audit(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<AuditQuery>,
) -> Response {
    let limit = query.limit.unwrap_or(20);
    match state.tunnel_audit.lock().query_by_tunnel(&id, limit) {
        Ok(events) => (StatusCode::OK, Json(serde_json::json!(events))).into_response(),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// ── SSH config hosts ────────────────────────────────────────

/// GET /tunnels/ssh-hosts — parse ~/.ssh/config and return host aliases.
pub(crate) async fn list_ssh_config_hosts() -> Response {
    let config_path = match dirs::home_dir() {
        Some(h) => h.join(".ssh").join("config"),
        None => return (StatusCode::OK, Json(serde_json::json!([]))).into_response(),
    };

    let file = match std::fs::File::open(&config_path) {
        Ok(f) => f,
        Err(_) => return (StatusCode::OK, Json(serde_json::json!([]))).into_response(),
    };

    let mut reader = BufReader::new(file);
    let config = match ssh2_config::SshConfig::default()
        .parse(&mut reader, ssh2_config::ParseRule::ALLOW_UNKNOWN_FIELDS)
    {
        Ok(c) => c,
        Err(e) => {
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to parse SSH config: {e}"),
            );
        }
    };

    let hosts: Vec<String> = config
        .get_hosts()
        .iter()
        .flat_map(|host| {
            host.pattern.iter().filter_map(|clause| {
                // Skip negated patterns and the wildcard-only pattern.
                if clause.negated || clause.pattern == "*" {
                    None
                } else {
                    Some(clause.pattern.clone())
                }
            })
        })
        .collect();

    (StatusCode::OK, Json(serde_json::json!(hosts))).into_response()
}

// ── SSH agent keys ──────────────────────────────────────────

/// GET /tunnels/agent-keys — list loaded SSH agent key fingerprints.
pub(crate) async fn list_agent_keys() -> Response {
    let output = match tokio::process::Command::new("ssh-add")
        .arg("-l")
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to run ssh-add: {e}"),
            );
        }
    };

    // Exit code 1 means "no identities" — return empty list, not an error.
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no identities") || output.status.code() == Some(1) {
            return (StatusCode::OK, Json(serde_json::json!([]))).into_response();
        }
        return err_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("ssh-add failed: {}", stderr.trim()),
        );
    }

    // Each line: "256 SHA256:xxxxx user@host (ED25519)"
    let stdout = String::from_utf8_lossy(&output.stdout);
    let keys: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(4, ' ').collect();
            if parts.len() >= 3 {
                serde_json::json!({
                    "bits": parts[0],
                    "fingerprint": parts[1],
                    "comment": parts.get(2).unwrap_or(&""),
                    "type": parts.get(3).map(|s| s.trim_matches(|c| c == '(' || c == ')')),
                })
            } else {
                serde_json::json!({"raw": line})
            }
        })
        .collect();

    (StatusCode::OK, Json(serde_json::json!(keys))).into_response()
}

// ── Tests ───────────────────────────────────────────────────
//
// Route-level HTTP tests using the `build_router(...).oneshot(request)`
// pattern established in `mcp_http/mod.rs` / `mcp_http/config_routes.rs`.
// None of these handlers call `require_local_or_auth` (unlike most of
// `config_routes.rs`), so requests don't need a `ConnectInfo` extension.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests_support::make_test_app_state;
    use crate::tunnels::audit::EventKind;
    use crate::tunnels::profile::TunnelProfile;
    use crate::tunnels::storage::ProfileStore;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn test_state() -> Arc<AppState> {
        Arc::new(make_test_app_state())
    }

    fn app(state: Arc<AppState>) -> axum::Router {
        crate::mcp_http::build_router(state, false, true)
    }

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("read body");
        serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            panic!(
                "response body was not JSON: {e}, raw: {}",
                String::from_utf8_lossy(&bytes)
            )
        })
    }

    fn get(path: &str) -> Request<Body> {
        Request::get(path).body(Body::empty()).unwrap()
    }

    fn post_json(path: &str, body: &serde_json::Value) -> Request<Body> {
        Request::post(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    fn delete(path: &str) -> Request<Body> {
        Request::delete(path).body(Body::empty()).unwrap()
    }

    fn valid_profile_json() -> serde_json::Value {
        serde_json::to_value(TunnelProfile::new("my-tunnel", "example.com", "alice")).unwrap()
    }

    // ── list/save/delete profiles ───────────────────────────

    #[tokio::test]
    async fn list_tunnel_profiles_returns_empty_when_none_saved() {
        let resp = app(test_state())
            .oneshot(get("/tunnels/profiles"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json, serde_json::json!([]));
    }

    #[tokio::test]
    async fn list_tunnel_profiles_returns_profiles_saved_directly_to_disk() {
        let state = test_state();
        let profile = TunnelProfile::new("staging", "host.example.com", "alice");
        ProfileStore::save(&state.data_dir, &profile).unwrap();

        let resp = app(state).oneshot(get("/tunnels/profiles")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "staging");
    }

    #[tokio::test]
    async fn save_tunnel_profile_persists_a_valid_profile() {
        let state = test_state();
        let profile = valid_profile_json();
        let expected_id = profile["id"].as_str().unwrap().to_string();

        let resp = app(state.clone())
            .oneshot(post_json("/tunnels/profiles", &profile))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["id"], expected_id);

        let saved = ProfileStore::load_all(&state.data_dir, None).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].id, expected_id);
    }

    #[tokio::test]
    async fn save_tunnel_profile_rejects_invalid_profile_with_400() {
        let mut profile = valid_profile_json();
        profile["ssh"]["host"] = serde_json::Value::String(String::new());

        let resp = app(test_state())
            .oneshot(post_json("/tunnels/profiles", &profile))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let json = body_json(resp).await;
        assert!(
            json["error"].as_str().unwrap().contains("host"),
            "expected a host-related error, got: {json}"
        );
    }

    // --- IPC/HTTP parity bug, NOW FIXED (see plan Phase 1): `save_tunnel_profile` ---
    //
    // Both the HTTP route and the Tauri command (`tauri_commands::save_tunnel_profile`)
    // now call the shared `save_tunnel_profile_impl`, which mints a fresh UUID
    // for an empty/missing `id` before validating — previously only the Tauri
    // command did this (verified pre-fix: the HTTP route deserialized
    // `Json<TunnelProfile>` directly with no such step, so an empty `id`
    // reached `validate()` unchanged and was rejected as "not a valid UUID").
    #[tokio::test]
    async fn http_save_generates_an_id_for_an_empty_id_matching_ipc() {
        let mut profile = valid_profile_json();
        profile["id"] = serde_json::Value::String(String::new());

        let resp = app(test_state())
            .oneshot(post_json("/tunnels/profiles", &profile))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let generated_id = json["id"].as_str().expect("id must be a string");
        assert!(
            uuid::Uuid::parse_str(generated_id).is_ok(),
            "expected a generated UUID, got: {generated_id}"
        );
    }

    // The IPC-side counterpart (`tauri_commands::save_tunnel_profile`) now
    // calls the exact same `save_tunnel_profile_impl` this HTTP test exercises
    // above — previously that logic could only be verified by reading source,
    // since `tauri::State<'_, Arc<AppState>>` has no public constructor
    // outside a running Tauri app (see `pty.rs`'s
    // `get_session_foreground_process_impl` doc comment, the established
    // pattern this fix follows). Exercising `save_tunnel_profile_impl`
    // directly proves both transports now share one code path, not just one
    // behavior.
    #[test]
    fn save_tunnel_profile_impl_generates_an_id_for_an_empty_id() {
        let dir = tempfile::tempdir().unwrap();
        let mut profile = valid_profile_json();
        profile["id"] = serde_json::Value::String(String::new());

        let id = save_tunnel_profile_impl(dir.path(), profile).unwrap();
        assert!(uuid::Uuid::parse_str(&id).is_ok(), "expected a UUID: {id}");

        let saved = ProfileStore::load_all(dir.path(), None).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].id, id);
    }

    #[test]
    fn save_tunnel_profile_impl_rejects_invalid_profile() {
        let dir = tempfile::tempdir().unwrap();
        let mut profile = valid_profile_json();
        profile["ssh"]["host"] = serde_json::Value::String(String::new());

        let err = save_tunnel_profile_impl(dir.path(), profile).unwrap_err();
        assert!(matches!(err, SaveTunnelProfileError::Validation(_)));
    }

    #[tokio::test]
    async fn delete_tunnel_profile_returns_ok_true_when_deleted() {
        let state = test_state();
        let profile = TunnelProfile::new("to-delete", "host.example.com", "alice");
        ProfileStore::save(&state.data_dir, &profile).unwrap();

        let resp = app(state.clone())
            .oneshot(delete(&format!("/tunnels/profiles/{}", profile.id)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["deleted"], true);

        let remaining = ProfileStore::load_all(&state.data_dir, None).unwrap();
        assert!(remaining.is_empty());
    }

    // --- IPC/HTTP parity bug, NOW FIXED (see plan Phase 1): `delete_tunnel_profile` ---
    //
    // Both transports now share `delete_tunnel_profile_impl`, which treats a
    // missing profile as an error (`DeleteTunnelProfileError::NotFound`,
    // mapped to HTTP 404 / IPC `Err("profile not found")`) — previously the
    // Tauri command returned `Ok(false)` silently for this case (verified
    // pre-fix by reading its source: `ProfileStore::delete(...).map_err(...)`
    // passed the bool straight through with no not-found handling) while HTTP
    // already answered 404. HTTP's existing, more REST-idiomatic behavior is
    // the one both sides now agree on.
    #[tokio::test]
    async fn http_delete_returns_404_for_a_missing_profile() {
        let resp = app(test_state())
            .oneshot(delete("/tunnels/profiles/does-not-exist"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // The IPC-side counterpart (`tauri_commands::delete_tunnel_profile`) now
    // calls the exact same `delete_tunnel_profile_impl` this HTTP test
    // exercises above — see the comment above
    // `save_tunnel_profile_impl_generates_an_id_for_an_empty_id` for why this
    // is now directly testable instead of only readable-by-source.
    #[test]
    fn delete_tunnel_profile_impl_errors_for_a_missing_profile() {
        let state = test_state();
        let err = delete_tunnel_profile_impl(&state, "does-not-exist").unwrap_err();
        assert!(matches!(err, DeleteTunnelProfileError::NotFound));
    }

    #[test]
    fn delete_tunnel_profile_impl_succeeds_for_an_existing_profile() {
        let state = test_state();
        let profile = TunnelProfile::new("to-delete", "host.example.com", "alice");
        ProfileStore::save(&state.data_dir, &profile).unwrap();

        assert!(delete_tunnel_profile_impl(&state, &profile.id).is_ok());
        assert!(ProfileStore::load_all(&state.data_dir, None).unwrap().is_empty());
    }

    // ── tunnel lifecycle ────────────────────────────────────

    #[tokio::test]
    async fn start_tunnel_returns_404_for_a_missing_profile() {
        let resp = app(test_state())
            .oneshot(
                Request::post("/tunnels/start/does-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn stop_tunnel_returns_404_for_an_unknown_tunnel() {
        let resp = app(test_state())
            .oneshot(
                Request::post("/tunnels/stop/does-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── status queries ──────────────────────────────────────

    #[tokio::test]
    async fn list_active_tunnels_returns_empty_list_when_none_running() {
        let resp = app(test_state())
            .oneshot(get("/tunnels/active"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_json(resp).await, serde_json::json!([]));
    }

    #[tokio::test]
    async fn get_tunnel_status_returns_404_for_an_unknown_tunnel() {
        let resp = app(test_state())
            .oneshot(get("/tunnels/status/does-not-exist"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── audit log ───────────────────────────────────────────

    #[tokio::test]
    async fn get_tunnel_audit_returns_empty_list_for_an_unknown_tunnel() {
        let resp = app(test_state())
            .oneshot(get("/tunnels/audit/never-existed"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_json(resp).await, serde_json::json!([]));
    }

    #[tokio::test]
    async fn get_tunnel_audit_default_limit_is_20() {
        let state = test_state();
        for i in 0..25_i64 {
            state
                .tunnel_audit
                .lock()
                .insert("t1", EventKind::Retry, serde_json::json!({"seq": i}))
                .unwrap();
        }

        let resp = app(state).oneshot(get("/tunnels/audit/t1")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 20, "default limit must be 20, got {}", arr.len());
    }

    #[tokio::test]
    async fn get_tunnel_audit_respects_the_limit_query_param() {
        let state = test_state();
        for i in 0..5_i64 {
            state
                .tunnel_audit
                .lock()
                .insert("t2", EventKind::Retry, serde_json::json!({"seq": i}))
                .unwrap();
        }

        let resp = app(state)
            .oneshot(get("/tunnels/audit/t2?limit=2"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }

    // ── SSH config hosts / agent keys ───────────────────────

    #[tokio::test]
    async fn list_ssh_config_hosts_returns_a_json_array() {
        // Environment-dependent (reads the real `~/.ssh/config`, if any), so
        // this only pins the contract: always 200, always a JSON array.
        let resp = app(test_state())
            .oneshot(get("/tunnels/ssh-hosts"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert!(json.is_array(), "expected a JSON array, got: {json}");
    }

    #[tokio::test]
    async fn list_agent_keys_runs_its_handler_and_returns_json() {
        // Environment-dependent (shells out to the real `ssh-add -l`), so this
        // only pins that the route runs the real handler rather than a stub:
        // either a 200 with an array, or a 500 whose error mentions ssh-add.
        let resp = app(test_state())
            .oneshot(get("/tunnels/agent-keys"))
            .await
            .unwrap();
        let status = resp.status();
        let json = body_json(resp).await;
        if status == StatusCode::OK {
            assert!(json.is_array(), "expected a JSON array, got: {json}");
        } else {
            assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
            assert!(
                json["error"].as_str().unwrap_or("").contains("ssh-add"),
                "expected an ssh-add-related error, got: {json}"
            );
        }
    }
}
