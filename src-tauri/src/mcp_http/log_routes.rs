//! HTTP endpoints for the application log ring buffer.

use axum::Json;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::AppState;
use crate::app_logger::LogEntry;

#[derive(Deserialize)]
pub(crate) struct GetLogsQuery {
    #[serde(default)]
    limit: usize,
    /// Optional minimum level filter: "debug", "info", "warn", "error"
    #[serde(default)]
    level: Option<String>,
    /// Optional source filter: "app", "plugin", "git", "terminal", etc.
    #[serde(default)]
    source: Option<String>,
    /// Optional audience filter: "user" or "diagnostic".
    #[serde(default)]
    audience: Option<String>,
}

/// GET /logs — retrieve log entries from the ring buffer.
pub(crate) async fn get_logs(
    State(state): State<Arc<AppState>>,
    Query(q): Query<GetLogsQuery>,
) -> Json<Vec<LogEntry>> {
    let buf = state.log_buffer.lock();
    let mut entries = buf.get_entries(0);
    drop(buf);

    // Apply optional filters BEFORE the limit: filtering after slicing to the
    // last N entries starves any query where the matching entries aren't
    // among the most recent N (#655 — `?level=error&limit=50` would return
    // nothing if the last 50 lines happened to all be info).
    if let Some(ref level) = q.level {
        entries.retain(|e| e.level == *level);
    }
    if let Some(ref source) = q.source {
        entries.retain(|e| e.source == *source);
    }
    if let Some(ref audience) = q.audience {
        entries.retain(|e| e.audience == *audience);
    }

    if q.limit > 0 && entries.len() > q.limit {
        entries.drain(0..entries.len() - q.limit);
    }

    Json(entries)
}

#[derive(Deserialize)]
pub(crate) struct PushLogBody {
    level: String,
    source: String,
    message: String,
    data_json: Option<String>,
    #[serde(default)]
    audience: Option<String>,
}

/// POST /logs — push a log entry into the ring buffer, and persist it to the
/// log file (see `app_logger::emit_frontend_log`) so it survives past the
/// ring buffer's 1000-entry cap — the browser/PWA transport twin of the Tauri
/// `push_log` command.
pub(crate) async fn push_log(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PushLogBody>,
) -> StatusCode {
    crate::app_logger::emit_frontend_log(
        &body.level,
        &body.source,
        &body.message,
        body.data_json.as_deref(),
        body.audience.as_deref(),
    );
    let mut buf = state.log_buffer.lock();
    buf.push_with_audience(
        body.level,
        body.source,
        body.message,
        body.data_json,
        body.audience,
    );
    StatusCode::NO_CONTENT
}

/// DELETE /logs — clear all log entries.
pub(crate) async fn clear_logs(State(state): State<Arc<AppState>>) -> StatusCode {
    let mut buf = state.log_buffer.lock();
    buf.clear();
    StatusCode::NO_CONTENT
}

// ---------------------------------------------------------------------------
// Diagnostic mode toggle
// ---------------------------------------------------------------------------

/// GET /diagnostics — current diagnostic mode state.
pub(crate) async fn diagnostics_get() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "enabled": crate::cpu_watchdog::diagnostic_mode(),
    }))
}

/// GET /diagnostics/memory — where the process's memory is.
///
/// The 2026-09-08 40 GB growth could not be attributed to any structure while it
/// was happening: a 40 GB process is not debuggable, so every candidate had to be
/// excluded by reading code. This answers the question in one request instead.
pub(crate) async fn memory_report_get(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    Json(crate::memory_report::report(&state))
}

/// GET /diagnostics/markers — per-session protocol-marker compliance.
///
/// Exists because the only way to ask "are the agents still emitting `intent:`
/// and `suggest:`?" used to be grepping scrollback, which counts every mention
/// of the word and is capped by buffer size (#4421). These are counts of parsed
/// events against submitted turns.
///
/// A session whose markers are switched off reports `enabled: false` rather than
/// zero compliance — the difference between an agent ignoring the protocol and
/// an agent that was never asked to follow it.
pub(crate) async fn marker_compliance_get(
    State(state): State<std::sync::Arc<crate::state::AppState>>,
) -> Json<serde_json::Value> {
    let sessions: Vec<serde_json::Value> = state
        .session_maps
        .session_states
        .iter()
        .map(|entry| {
            let session_id = entry.key().clone();
            let agent_type = entry.value().agent_type.clone();
            let stats = state.marker_stats_for(&session_id);
            let (intent_enabled, suggest_enabled) =
                crate::mcp_http::mcp_transport::marker_flags_for_agent(
                    &state,
                    agent_type.as_deref(),
                );
            serde_json::json!({
                "session_id": session_id,
                "agent_type": agent_type,
                "turns": stats.turns,
                "intent": stats.intent,
                "suggest": stats.suggest,
                "intent_enabled": intent_enabled,
                "suggest_enabled": suggest_enabled,
            })
        })
        .collect();
    Json(serde_json::json!({ "sessions": sessions }))
}

/// GET /diagnostics/sessions — per-session overload attribution, live.
///
/// Answers "which session is hot right now" on demand, instead of only after
/// the fact in a `CPU SPIKE`/`SESSION OVERLOAD` log line — diagnosing the
/// `0b421c3a`/`cddded98` incident took ~45 minutes of manual correlation
/// across `debug logs`, `explain_state`, and `lsof` precisely because nothing
/// gave a direct answer to this question. A peek, not a drain
/// (`state::peek_counter`): reading this must not reset the counters the
/// watchdog's own periodic tick relies on to compute a rate.
pub(crate) async fn session_overload_get(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    // The set of sessions worth reporting is "anything with a measurable
    // number right now," not literally every open PTY — a quiet session
    // would just be a row of zeros. Union the three counter maps' keys with
    // `grid.gates`'s (outstanding frames can be nonzero with no event/byte/lag
    // activity at all, e.g. a backgrounded tab that never acks).
    let mut session_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for m in [
        &state.session_maps.session_event_counts,
        &state.session_maps.session_output_bytes,
        &state.session_maps.session_ws_lag,
    ] {
        session_ids.extend(m.iter().map(|e| e.key().clone()));
    }
    session_ids.extend(state.grid.gates.iter().map(|e| e.key().clone()));

    let sessions: Vec<serde_json::Value> = session_ids
        .into_iter()
        .filter_map(|session_id| {
            let events_since_last_tick =
                crate::state::peek_counter(&state.session_maps.session_event_counts, &session_id);
            let output_bytes_since_last_tick =
                crate::state::peek_counter(&state.session_maps.session_output_bytes, &session_id);
            let cumulative_ws_lag_since_last_tick =
                crate::state::peek_counter(&state.session_maps.session_ws_lag, &session_id);
            let outstanding_grid_frames = state
                .grid
                .gates
                .get(&session_id)
                .map(|g| g.outstanding())
                .unwrap_or(0);
            // `drain_counter_map` zeroes a counter but never removes its key
            // (the entry stays so a later bump doesn't need to re-`entry()`
            // it) — so a session the watchdog already drained back to zero is
            // still in the union above. Without this filter it would sit here
            // forever as a dead all-zero row, exactly the "quiet session is
            // just a row of zeros" outcome the comment above already says to
            // avoid; only re-check happens here, at read time, since that's
            // the only place both "did the watchdog reset this" and "is it
            // truly idle" can be told apart from "never had any activity."
            let has_signal = events_since_last_tick > 0
                || output_bytes_since_last_tick > 0
                || cumulative_ws_lag_since_last_tick > 0
                || outstanding_grid_frames > 0;
            if !has_signal {
                return None;
            }
            Some(serde_json::json!({
                "session_id": session_id,
                "events_since_last_tick": events_since_last_tick,
                "output_bytes_since_last_tick": output_bytes_since_last_tick,
                "cumulative_ws_lag_since_last_tick": cumulative_ws_lag_since_last_tick,
                "outstanding_grid_frames": outstanding_grid_frames,
            }))
        })
        .collect();
    Json(serde_json::json!({ "sessions": sessions }))
}

/// POST /diagnostics — toggle diagnostic mode. Body: `{ "enabled": true }`.
pub(crate) async fn diagnostics_set(
    Json(body): Json<super::types::SetApiDebugRequest>,
) -> Json<serde_json::Value> {
    crate::cpu_watchdog::set_diagnostic_mode(body.enabled);
    Json(serde_json::json!({
        "ok": true,
        "enabled": body.enabled,
    }))
}

// ---------------------------------------------------------------------------
// Raw PTY capture tap
// ---------------------------------------------------------------------------

/// GET /diagnostics/capture — is the tap recording, and how much has it written.
pub(crate) async fn capture_get() -> Json<serde_json::Value> {
    Json(crate::pty_capture::status())
}

/// POST /diagnostics/capture — start/stop recording raw PTY bytes.
/// Body: `{ "enabled": true, "session_id": "<optional filter>" }`.
///
/// Turn it on when a state-detection bug is reproducible but not yet understood:
/// the capture it leaves behind becomes a `pty::tests` fixture, which is the only
/// way a detector regression stops recurring. See `pty_capture` for the why.
pub(crate) async fn capture_set(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::SetCaptureRequest>,
) -> Json<serde_json::Value> {
    Json(crate::pty_capture::set_enabled_in_config_dir(
        &state,
        body.enabled,
        body.session_id,
    ))
}

// ---------------------------------------------------------------------------
// invoke_js — execute a debug script in the main WebView (loopback only)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct InvokeJsBody {
    script: String,
}

/// POST /debug/invoke_js — execute JavaScript in the main WebView.
///
/// Loopback-only (this is an RCE surface): mirrors the MCP `debug
/// action=invoke_js` path so the dev build — reachable only over HTTP, not the
/// MCP stdio transport — is scriptable for diagnostics. Fire-and-forget: the
/// result + captured console output are pushed to the ring buffer with
/// source="eval_js"; read them back via GET /logs?source=eval_js.
pub(crate) async fn invoke_js_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<InvokeJsBody>,
) -> impl IntoResponse {
    if let Err(resp) = super::guards::localhost_only(&addr) {
        return resp.into_response();
    }
    Json(eval_debug_script(&state, &body.script)).into_response()
}

/// Wrap `script` in the standard debug harness (console capture + result
/// serialization) and evaluate it in the main WebView. The harness invokes the
/// `push_log` command so the result lands in the ring buffer as source="eval_js".
/// Shared by the MCP `debug` tool and the HTTP `/debug/invoke_js` route.
#[cfg(feature = "desktop")]
pub(crate) fn eval_debug_script(state: &Arc<AppState>, script: &str) -> serde_json::Value {
    if state.secrets.tools_blocked() {
        return serde_json::json!({"error": "Agent inspection is disabled while a private secret form is open"});
    }
    use tauri::Manager;
    let app_handle = state.app_handle.read().clone();
    let Some(handle) = app_handle else {
        return serde_json::json!({"error": "AppHandle not initialized"});
    };
    let Some(window) = handle.get_webview_window("main") else {
        return serde_json::json!({"error": "main window not found"});
    };
    let wrapped = format!(
        r#"(async () => {{
  const __src = "eval_js";
  const __logs = [];
  const __origLog = console.log;
  const __origWarn = console.warn;
  const __origError = console.error;
  const __origInfo = console.info;
  const __fmt = (a) => typeof a === "string" ? a : JSON.stringify(a);
  console.log = (...a) => {{ __logs.push(a.map(__fmt).join(" ")); __origLog(...a); }};
  console.info = (...a) => {{ __logs.push(a.map(__fmt).join(" ")); __origInfo(...a); }};
  console.warn = (...a) => {{ __logs.push("[WARN] " + a.map(__fmt).join(" ")); __origWarn(...a); }};
  console.error = (...a) => {{ __logs.push("[ERROR] " + a.map(__fmt).join(" ")); __origError(...a); }};
  try {{
    const __result = await (async () => {{ {script} }})();
    const __val = __result === undefined ? "(undefined)" : JSON.stringify(__result, null, 2);
    const __msg = __logs.length > 0 ? __logs.join("\n") + "\n---\n" + __val : __val;
    window.__TAURI__.core.invoke("push_log", {{ level: "info", source: __src, message: __msg, dataJson: null }});
  }} catch (__e) {{
    const __val = __e instanceof Error ? `${{__e.name}}: ${{__e.message}}\n${{__e.stack}}` : String(__e);
    const __msg = __logs.length > 0 ? __logs.join("\n") + "\n---\n" + __val : __val;
    window.__TAURI__.core.invoke("push_log", {{ level: "error", source: __src, message: __msg, dataJson: null }});
  }} finally {{
    console.log = __origLog;
    console.info = __origInfo;
    console.warn = __origWarn;
    console.error = __origError;
  }}
}})()"#
    );
    match window.eval(&wrapped) {
        Ok(()) => serde_json::json!({
            "ok": true,
            "hint": "Result logged with source='eval_js'. Read via: GET /logs?source=eval_js&limit=1"
        }),
        Err(e) => serde_json::json!({"error": format!("eval failed: {e}")}),
    }
}

/// POST /debug/reload_webview — reload the main WebView from the native side.
///
/// The escape hatch for a WebView whose main JS thread is blocked or whose web
/// content process is gone: the UI is white, `/debug/invoke_js` is useless
/// because it needs that thread to run the script, and the only remedy left was
/// restarting the app — which kills every PTY session with it.
///
/// It runs entirely on the native side, so it works precisely when the JS side
/// does not. Sessions live in the backend, so this costs nothing but a repaint.
/// Loopback-only, like its neighbour.
pub(crate) async fn reload_webview_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(resp) = super::guards::localhost_only(&addr) {
        return resp.into_response();
    }
    tracing::info!(source = "webview", caller = %addr, "HTTP WebView reload requested");
    Json(reload_main_webview(&state)).into_response()
}

/// Put the `main` webview back on the app.
///
/// Deliberately a `navigate`, not a `reload`. On 2026-09-08 this endpoint
/// answered `{"ok":true}` and left the window white for an hour: the frame was
/// on `about:srcdoc`, and reloading a blank document reloads the blank document.
/// The same recovery serves the automatic poller — see `webview_recovery`.
pub(crate) fn reload_main_webview(state: &Arc<AppState>) -> serde_json::Value {
    crate::webview_recovery::navigate_home(state, "http_route")
}

#[cfg(not(feature = "desktop"))]
pub(crate) fn eval_debug_script(_state: &Arc<AppState>, _script: &str) -> serde_json::Value {
    serde_json::json!({"error": "invoke_js requires desktop feature"})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::MarkerKind;
    use axum::extract::Query;

    fn session_entry(sessions: &serde_json::Value, session_id: &str) -> serde_json::Value {
        sessions["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["session_id"] == session_id)
            .unwrap_or_else(|| panic!("session {session_id} missing from {sessions}"))
            .clone()
    }

    #[tokio::test]
    async fn marker_compliance_reports_counts_and_the_and_rule_gate() {
        let dir = std::env::temp_dir().join("test-marker-compliance-route");
        let _ = std::fs::create_dir_all(&dir);
        let _guard = crate::config::set_config_dir_override(dir);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        state.config.write().intent_tab_title = true;
        state.config.write().suggest_followups = true;
        state.session_maps.session_states.insert(
            "on-session".to_string(),
            crate::state::SessionState {
                agent_type: Some("claude".to_string()),
                ..Default::default()
            },
        );
        state.note_marker("on-session", MarkerKind::TurnSubmitted);
        state.note_marker("on-session", MarkerKind::Intent);

        // suggest never fired for this session, and its markers are globally off —
        // both must be visible in the same response without one masking the other.
        state.session_maps.session_states.insert(
            "off-session".to_string(),
            crate::state::SessionState {
                agent_type: Some("cursor".to_string()),
                ..Default::default()
            },
        );
        let mut agents_cfg = crate::config::AgentsConfig::default();
        agents_cfg.agents.insert(
            "cursor".to_string(),
            crate::config::AgentSettings {
                intent_tab_title: Some(false),
                suggest_followups: Some(false),
                ..Default::default()
            },
        );
        crate::config::save_agents_config(crate::config::load_agents_config(), agents_cfg).unwrap();
        state.note_marker("off-session", MarkerKind::TurnSubmitted);

        let result = marker_compliance_get(State(state)).await.0;

        let on = session_entry(&result, "on-session");
        assert_eq!(on["turns"], 1);
        assert_eq!(on["intent"], 1);
        assert_eq!(on["suggest"], 0);
        assert_eq!(on["intent_enabled"], true);
        assert_eq!(on["suggest_enabled"], true);

        let off = session_entry(&result, "off-session");
        assert_eq!(off["turns"], 1);
        assert_eq!(off["intent"], 0);
        assert_eq!(off["suggest"], 0);
        assert_eq!(
            off["intent_enabled"], false,
            "a per-agent override disabled this session's marker — a zero count \
            alone can't distinguish that from the agent ignoring the protocol"
        );
        assert_eq!(off["suggest_enabled"], false);
    }

    /// The "who's hot right now" endpoint the `0b421c3a`/`cddded98` incident
    /// motivated — see `cpu_watchdog.rs`'s `SessionRates`/`check_session_overload`
    /// for the same numbers' periodic log-line twin.
    #[tokio::test]
    async fn session_overload_reports_live_counters_without_resetting_them() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state
            .session_maps
            .session_event_counts
            .entry("hot".to_string())
            .or_default()
            .fetch_add(42, std::sync::atomic::Ordering::Relaxed);
        state
            .session_maps
            .session_output_bytes
            .entry("hot".to_string())
            .or_default()
            .fetch_add(2048, std::sync::atomic::Ordering::Relaxed);
        state
            .session_maps
            .session_ws_lag
            .entry("hot".to_string())
            .or_default()
            .fetch_add(7, std::sync::atomic::Ordering::Relaxed);

        let result = session_overload_get(State(state.clone())).await.0;
        let hot = session_entry(&result, "hot");
        assert_eq!(hot["events_since_last_tick"], 42);
        assert_eq!(hot["output_bytes_since_last_tick"], 2048);
        assert_eq!(hot["cumulative_ws_lag_since_last_tick"], 7);
        assert_eq!(hot["outstanding_grid_frames"], 0);

        // A second read must see the same numbers — an on-demand GET must not
        // steal what the watchdog's own next tick needs to compute a rate.
        let result_again = session_overload_get(State(state)).await.0;
        assert_eq!(
            session_entry(&result_again, "hot")["events_since_last_tick"],
            42
        );
    }

    #[tokio::test]
    async fn session_overload_omits_a_session_with_no_activity_at_all() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = session_overload_get(State(state)).await.0;
        assert_eq!(result["sessions"].as_array().unwrap().len(), 0);
    }

    /// `drain_counter_map` zeroes a counter's value but never removes its key
    /// (a code-review finding on the first version of this endpoint) — so a
    /// session that was briefly hot, then drained back to zero by a watchdog
    /// tick, must NOT keep showing up as a permanent all-zero row for the rest
    /// of its life. This is the actual gap the "never had activity" test above
    /// doesn't cover: that test's session never had an entry created at all,
    /// while this one simulates the real production shape — an entry that
    /// exists, with value zero.
    #[tokio::test]
    async fn session_overload_omits_a_session_the_watchdog_already_drained_to_zero() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state
            .session_maps
            .session_event_counts
            .entry("was-hot-now-quiet".to_string())
            .or_default()
            .fetch_add(99, std::sync::atomic::Ordering::Relaxed);

        // Simulate the watchdog's own periodic tick draining it back to zero —
        // the key stays in the map (matching `drain_counter_map`'s real
        // behavior), only the value resets.
        let drained = crate::state::drain_counter_map(&state.session_maps.session_event_counts);
        assert_eq!(drained, vec![("was-hot-now-quiet".to_string(), 99)]);
        assert!(
            state
                .session_maps
                .session_event_counts
                .contains_key("was-hot-now-quiet"),
            "drain_counter_map resets the value, not the key — pinning that \
             behavior here since the route's correctness depends on it"
        );

        let result = session_overload_get(State(state)).await.0;
        assert_eq!(
            result["sessions"].as_array().unwrap().len(),
            0,
            "a session drained back to zero must not linger as a dead row"
        );
    }

    /// A session with outstanding grid frames but nothing in any of the three
    /// counter maps must still be listed — the union in `session_overload_get`
    /// pulls session ids from `grid.gates` too, not just the three counters.
    #[tokio::test]
    async fn session_overload_includes_a_session_known_only_via_grid_gates() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let gate = std::sync::Arc::new(crate::grid_gate::GridGate::new());
        gate.mark_sent();
        gate.mark_sent();
        state.grid.gates.insert("frame-only".to_string(), gate);

        let result = session_overload_get(State(state)).await.0;
        let entry = session_entry(&result, "frame-only");
        assert_eq!(entry["outstanding_grid_frames"], 2);
        assert_eq!(entry["events_since_last_tick"], 0);
    }

    /// The newest entries in the buffer are all "info"; the older ones are the
    /// 3 "error" lines we care about. A limit-then-filter implementation slices
    /// the last 50 (all info) before filtering, so `?level=error&limit=50`
    /// would come back empty. Filtering first must still surface the 3 older
    /// error entries, in their original chronological order.
    #[tokio::test]
    async fn get_logs_filters_before_applying_limit() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        {
            let mut buf = state.log_buffer.lock();
            for i in 0..3 {
                buf.push_with_audience(
                    "error".to_string(),
                    "app".to_string(),
                    format!("error-{i}"),
                    None,
                    None,
                );
            }
            for i in 0..200 {
                buf.push_with_audience(
                    "info".to_string(),
                    "app".to_string(),
                    format!("info-{i}"),
                    None,
                    None,
                );
            }
        }

        let query = Query(GetLogsQuery {
            limit: 50,
            level: Some("error".to_string()),
            source: None,
            audience: None,
        });
        let Json(entries) = get_logs(State(state), query).await;

        let messages: Vec<&str> = entries.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(
            messages,
            vec!["error-0", "error-1", "error-2"],
            "expected the 3 older error entries, in order, not truncated away by the limit"
        );
    }
}
