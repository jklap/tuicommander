//! A connected remote machine's sessions, mirrored onto this one.
//!
//! The desktop renders idle / busy / question badges from two things: the rows
//! `list_active_sessions` returns, and the `session-state-changed` push that
//! moves them between polls. A remote daemon runs its own accumulator and
//! publishes exactly the same pair on its own `/events` — so parity needs no
//! second state machine here, only a pipe.
//!
//! That is what this module is. Per connected connection one task:
//!
//! * seeds from the daemon's `GET /sessions`, so a tab that was already running
//!   when we connected has a badge before anything moves;
//! * consumes the daemon's `/events` with **no** `types=` filter and repeats
//!   every frame on the local bus, with the same dual-emit a local producer
//!   uses. An unfiltered stream is the point: a future event type crosses with
//!   no change here.
//!
//! A mirrored row carries `connection_id`, which is the only field that tells
//! it apart from a local one. Everything downstream — the badge, the
//! notification, the queue gate — is the local code path, unchanged.
//!
//! The daemon's session ids are UUIDs minted on the daemon, so they cannot
//! collide with this machine's.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use futures_util::StreamExt;

use crate::AppState;
use crate::mcp_http::types::SessionInfo;
use crate::state::AppEvent;

/// Wait before re-seeding after the event stream ends. A daemon restart, a
/// dropped tunnel and a network blip all land here; the status poll in
/// `remote_runtime` is what decides whether the connection is still up, so this
/// only has to avoid a hot loop.
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

/// Timeout for the one-shot `GET /sessions` seed. The event stream gets none —
/// it is long-lived by design.
const SEED_TIMEOUT: Duration = Duration::from_secs(10);

/// Sessions running on remote machines, keyed by connection id then session id.
///
/// A map per connection rather than one flat map because a disconnect has to
/// drop exactly one machine's rows, and "which machine ran this" is not
/// recoverable from a session id.
#[derive(Default)]
pub(crate) struct RemoteSessions {
    by_connection: DashMap<String, HashMap<String, SessionInfo>>,
}

/// Every mirrored session, as session-list rows.
pub(crate) fn mirrored_rows(state: &AppState) -> Vec<SessionInfo> {
    state
        .remote_sessions
        .by_connection
        .iter()
        .flat_map(|entry| entry.value().values().cloned().collect::<Vec<_>>())
        .collect()
}

/// Replace one connection's rows with what its daemon just reported.
///
/// A replace rather than a merge: the daemon's answer is the whole truth about
/// that machine, so a session it no longer lists is gone.
fn store_seed(state: &AppState, connection_id: &str, rows: Vec<SessionInfo>) {
    let rows = rows
        .into_iter()
        .map(|mut row| {
            row.connection_id = Some(connection_id.to_string());
            (row.session_id.clone(), row)
        })
        .collect();
    state
        .remote_sessions
        .by_connection
        .insert(connection_id.to_string(), rows);
}

/// Put rows in the mirror without a daemon, so a router test can prove the HTTP
/// transport carries them.
#[cfg(test)]
pub(crate) fn store_seed_for_test(state: &AppState, connection_id: &str, rows: Vec<SessionInfo>) {
    store_seed(state, connection_id, rows);
}

/// Read the daemon's session list.
async fn seed(
    client: &reqwest::Client,
    base_url: &str,
    token: Option<&str>,
) -> Result<Vec<SessionInfo>, String> {
    let url = format!("{}/sessions", base_url.trim_end_matches('/'));
    let mut request = client.get(&url).timeout(SEED_TIMEOUT);
    if let Some(token) = token {
        request = request.query(&[("token", token)]);
    }
    let response = request.send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("GET /sessions answered {}", response.status()));
    }
    response.json().await.map_err(|e| e.to_string())
}

/// One decoded SSE frame.
struct Frame {
    event: String,
    data: String,
}

/// Feed raw SSE bytes in, get whole frames out.
///
/// Hand-written rather than an SSE client crate for one reason: the frames are
/// republished verbatim, so nothing here needs to understand them, and a
/// dependency that parses into its own event type would only have to be undone.
/// Comment lines (`:` keep-alives) and `id:` are dropped — the daemon's ids are
/// its own counter and mean nothing on this machine.
#[derive(Default)]
struct FrameDecoder {
    buffer: String,
    event: Option<String>,
    data: Vec<String>,
}

impl FrameDecoder {
    /// Append a chunk and return every frame it completed.
    fn push(&mut self, chunk: &str) -> Vec<Frame> {
        self.buffer.push_str(chunk);
        let mut frames = Vec::new();
        // A frame ends at a blank line, so the last (possibly partial) line
        // stays in the buffer until more bytes arrive.
        while let Some(newline) = self.buffer.find('\n') {
            let line = self.buffer[..newline].trim_end_matches('\r').to_string();
            self.buffer.drain(..=newline);
            if line.is_empty() {
                if let Some(frame) = self.take_frame() {
                    frames.push(frame);
                }
                continue;
            }
            if line.starts_with(':') {
                continue;
            }
            let (field, value) = match line.split_once(':') {
                Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
                None => (line.as_str(), ""),
            };
            match field {
                "event" => self.event = Some(value.to_string()),
                "data" => self.data.push(value.to_string()),
                _ => {}
            }
        }
        frames
    }

    fn take_frame(&mut self) -> Option<Frame> {
        let event = self.event.take();
        let data = std::mem::take(&mut self.data);
        // A frame with no `event:` is a plain `message`, which this backend
        // never sends; one with no `data:` carries nothing to republish.
        let event = event?;
        if data.is_empty() {
            return None;
        }
        Some(Frame {
            event,
            data: data.join("\n"),
        })
    }
}

/// Apply one frame: patch the mirrored rows, then repeat it locally.
///
/// Returns true when the row set itself may have moved and a re-seed is owed —
/// a created or closed session changes the list, and the daemon's event carries
/// less than its `GET /sessions` row does.
fn apply_frame(state: &Arc<AppState>, connection_id: &str, frame: &Frame) -> bool {
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(&frame.data) else {
        tracing::warn!(
            source = "remote",
            connection = connection_id,
            event = %frame.event,
            "Dropping a mirrored event with a body that is not JSON"
        );
        return false;
    };
    let reseed = match frame.event.as_str() {
        "session-state-changed" => {
            patch_state(state, connection_id, &payload);
            false
        }
        "session-created" | "session-closed" | "pty-exit" => true,
        _ => false,
    };
    republish(state, connection_id, &frame.event, payload);
    reseed
}

/// Move a mirrored row's state, so the next `list_active_sessions` agrees with
/// the push that just went out.
fn patch_state(state: &Arc<AppState>, connection_id: &str, payload: &serde_json::Value) {
    let Some(session_id) = payload.get("session_id").and_then(|v| v.as_str()) else {
        return;
    };
    let Ok(session_state) =
        serde_json::from_value::<crate::state::SessionState>(payload["state"].clone())
    else {
        return;
    };
    if let Some(mut rows) = state.remote_sessions.by_connection.get_mut(connection_id)
        && let Some(row) = rows.get_mut(session_id)
    {
        row.state = Some(session_state);
    }
}

/// Dual-emit, exactly as a local producer does: nothing forwards the bus to the
/// desktop window, so the window listener is fed here and the bus feeds the
/// local `/events` SSE — the IPC/HTTP parity rule in AGENTS.md.
///
/// The desktop event name is the daemon's own, so a window listener registered
/// for `session-state-changed` receives a remote one without knowing it exists.
fn republish(state: &Arc<AppState>, connection_id: &str, event: &str, payload: serde_json::Value) {
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        use tauri::Emitter;
        let _ = app.emit(event, &payload);
    }
    let _ = state.event_bus.send(AppEvent::RemoteMirrored {
        connection_id: connection_id.to_string(),
        event: event.to_string(),
        payload,
    });
}

/// Consume the daemon's `/events` until it ends or errors.
///
/// No `types=`: the allowlist is the one thing that would have to be edited
/// every time an event type is added, and a filtered mirror is a mirror that
/// silently lags the local machine.
async fn consume_stream(
    state: &Arc<AppState>,
    client: &reqwest::Client,
    connection_id: &str,
    base_url: &str,
    token: Option<&str>,
) -> Result<(), String> {
    let url = format!("{}/events", base_url.trim_end_matches('/'));
    let mut request = client.get(&url);
    if let Some(token) = token {
        request = request.query(&[("token", token)]);
    }
    let response = request.send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("GET /events answered {}", response.status()));
    }
    let mut decoder = FrameDecoder::default();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&chunk).into_owned();
        for frame in decoder.push(&text) {
            if apply_frame(state, connection_id, &frame) {
                match seed(client, base_url, token).await {
                    Ok(rows) => store_seed(state, connection_id, rows),
                    Err(e) => tracing::warn!(
                        source = "remote",
                        connection = connection_id,
                        %e,
                        "Could not re-read the remote session list"
                    ),
                }
            }
        }
    }
    Ok(())
}

/// Run the mirror for one connection until the task is aborted.
///
/// `retry_delay` is a parameter rather than the constant so a test can prove the
/// re-seed ordering without waiting on it: the delay is not what is under test,
/// and a test that sleeps on the real one is a test of how fast this machine is.
async fn run(state: Arc<AppState>, connection_id: String, retry_delay: Duration) {
    let client = reqwest::Client::new();
    loop {
        let Some(base_url) = state.remote.base_url(&connection_id) else {
            // The connection is no longer connected. `disconnect` aborts this
            // task, so this only happens in the window between the two.
            tokio::time::sleep(retry_delay).await;
            continue;
        };
        let token = state.remote.token(&connection_id);
        match seed(&client, &base_url, token.as_deref()).await {
            Ok(rows) => store_seed(&state, &connection_id, rows),
            Err(e) => {
                tracing::warn!(
                    source = "remote",
                    connection = %connection_id,
                    %e,
                    "Could not read the remote session list"
                );
                tokio::time::sleep(retry_delay).await;
                continue;
            }
        }
        if let Err(e) =
            consume_stream(&state, &client, &connection_id, &base_url, token.as_deref()).await
        {
            tracing::warn!(
                source = "remote",
                connection = %connection_id,
                %e,
                "Remote event stream ended"
            );
        }
        tokio::time::sleep(retry_delay).await;
    }
}

/// Start mirroring one connection. The handle belongs to the caller, which
/// aborts it on disconnect.
pub(crate) fn spawn(state: &Arc<AppState>, connection_id: String) -> tokio::task::JoinHandle<()> {
    let task_state = Arc::clone(state);
    tokio::spawn(run(task_state, connection_id, RECONNECT_DELAY))
}

/// Forget one connection's sessions.
///
/// Every row is announced closed before it is dropped: a badge is sticky by
/// construction, so a tab whose row simply vanishes keeps rendering the last
/// state the machine was in. `session-closed` is what a local session sends
/// when it goes, and a disconnect is the same news for a mirrored one.
pub(crate) fn drop_connection(state: &Arc<AppState>, connection_id: &str) {
    let Some((_, rows)) = state.remote_sessions.by_connection.remove(connection_id) else {
        return;
    };
    for session_id in rows.into_keys() {
        republish(
            state,
            connection_id,
            "session-closed",
            serde_json::json!({ "session_id": session_id, "reason": "remote-disconnected" }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests_support::make_test_app_state;

    fn frames(chunks: &[&str]) -> Vec<(String, String)> {
        let mut decoder = FrameDecoder::default();
        let mut out = Vec::new();
        for chunk in chunks {
            for frame in decoder.push(chunk) {
                out.push((frame.event, frame.data));
            }
        }
        out
    }

    #[test]
    fn a_whole_frame_decodes_to_its_event_and_its_body() {
        assert_eq!(
            frames(&["event: pty-activity\ndata: {\"session_id\":\"s1\"}\n\n"]),
            vec![(
                "pty-activity".to_string(),
                "{\"session_id\":\"s1\"}".to_string()
            )]
        );
    }

    #[test]
    fn a_frame_split_across_chunks_is_one_frame() {
        assert_eq!(
            frames(&["event: pty-ac", "tivity\ndata: {\"a\":", "1}\n\n"]),
            vec![("pty-activity".to_string(), "{\"a\":1}".to_string())]
        );
    }

    #[test]
    fn a_keep_alive_comment_produces_nothing() {
        assert_eq!(frames(&[":ping\n\n:ping\n\n"]), vec![]);
    }

    #[test]
    fn an_id_line_is_dropped_and_the_frame_still_decodes() {
        assert_eq!(
            frames(&["id: 7\nevent: pty-exit\ndata: {}\n\n"]),
            vec![("pty-exit".to_string(), "{}".to_string())]
        );
    }

    #[test]
    fn two_frames_in_one_chunk_both_come_out() {
        assert_eq!(
            frames(&["event: a\ndata: 1\n\nevent: b\ndata: 2\n\n"]),
            vec![
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "2".to_string())
            ]
        );
    }

    #[test]
    fn a_multi_line_body_is_rejoined_with_newlines() {
        assert_eq!(
            frames(&["event: a\ndata: {\ndata: }\n\n"]),
            vec![("a".to_string(), "{\n}".to_string())]
        );
    }

    #[test]
    fn a_seeded_row_is_tagged_with_the_connection_that_owns_it() {
        let state = Arc::new(make_test_app_state());
        store_seed(
            &state,
            "vps",
            vec![SessionInfo {
                session_id: "s1".into(),
                ..Default::default()
            }],
        );
        let rows = mirrored_rows(&state);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].connection_id.as_deref(), Some("vps"));
    }

    #[test]
    fn a_second_seed_replaces_the_rows_it_no_longer_lists() {
        let state = Arc::new(make_test_app_state());
        store_seed(
            &state,
            "vps",
            vec![
                SessionInfo {
                    session_id: "s1".into(),
                    ..Default::default()
                },
                SessionInfo {
                    session_id: "s2".into(),
                    ..Default::default()
                },
            ],
        );
        store_seed(
            &state,
            "vps",
            vec![SessionInfo {
                session_id: "s2".into(),
                ..Default::default()
            }],
        );
        let rows = mirrored_rows(&state);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].session_id, "s2");
    }

    #[test]
    fn a_remote_state_change_moves_the_row_a_list_call_returns() {
        let state = Arc::new(make_test_app_state());
        store_seed(
            &state,
            "vps",
            vec![SessionInfo {
                session_id: "s1".into(),
                ..Default::default()
            }],
        );
        let frame = Frame {
            event: "session-state-changed".into(),
            data:
                r#"{"session_id":"s1","state":{"awaiting_input":true,"question_text":"pick one"}}"#
                    .into(),
        };
        assert!(!apply_frame(&state, "vps", &frame));
        let rows = mirrored_rows(&state);
        let session_state = rows[0].state.as_ref().expect("the row carries state");
        assert!(session_state.awaiting_input);
        assert_eq!(session_state.question_text.as_deref(), Some("pick one"));
    }

    #[test]
    fn a_mirrored_event_reaches_the_local_bus_under_the_daemons_own_name() {
        let state = Arc::new(make_test_app_state());
        let mut rx = state.event_bus.subscribe();
        let frame = Frame {
            event: "session-state-changed".into(),
            data: r#"{"session_id":"s1","state":{"awaiting_input":true}}"#.into(),
        };
        apply_frame(&state, "vps", &frame);
        match rx.try_recv().expect("one event was published") {
            AppEvent::RemoteMirrored {
                connection_id,
                event,
                payload,
            } => {
                assert_eq!(connection_id, "vps");
                assert_eq!(event, "session-state-changed");
                assert_eq!(payload["session_id"], "s1");
            }
            other => panic!("expected a mirrored event, got {other:?}"),
        }
    }

    #[test]
    fn a_created_session_asks_for_a_fresh_list_and_a_state_change_does_not() {
        let state = Arc::new(make_test_app_state());
        assert!(apply_frame(
            &state,
            "vps",
            &Frame {
                event: "session-created".into(),
                data: r#"{"session_id":"s1"}"#.into(),
            }
        ));
        assert!(!apply_frame(
            &state,
            "vps",
            &Frame {
                event: "pty-activity".into(),
                data: r#"{"session_id":"s1"}"#.into(),
            }
        ));
    }

    #[test]
    fn a_body_that_is_not_json_is_dropped_rather_than_republished() {
        let state = Arc::new(make_test_app_state());
        let mut rx = state.event_bus.subscribe();
        apply_frame(
            &state,
            "vps",
            &Frame {
                event: "pty-activity".into(),
                data: "not json".into(),
            },
        );
        assert!(rx.try_recv().is_err(), "nothing was published");
    }

    #[test]
    fn disconnecting_closes_every_mirrored_session_before_forgetting_it() {
        let state = Arc::new(make_test_app_state());
        store_seed(
            &state,
            "vps",
            vec![SessionInfo {
                session_id: "s1".into(),
                ..Default::default()
            }],
        );
        let mut rx = state.event_bus.subscribe();
        drop_connection(&state, "vps");
        match rx.try_recv().expect("the row was announced closed") {
            AppEvent::RemoteMirrored { event, payload, .. } => {
                assert_eq!(event, "session-closed");
                assert_eq!(payload["session_id"], "s1");
            }
            other => panic!("expected a mirrored event, got {other:?}"),
        }
        assert!(mirrored_rows(&state).is_empty());
    }

    #[test]
    fn disconnecting_a_connection_that_never_mirrored_anything_says_nothing() {
        let state = Arc::new(make_test_app_state());
        let mut rx = state.event_bus.subscribe();
        drop_connection(&state, "vps");
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn the_seed_signs_its_call_and_returns_the_daemons_own_rows() {
        let mut server = mockito::Server::new_async().await;
        let route = server
            .mock("GET", "/sessions")
            .match_query(mockito::Matcher::Regex("^token=tok$".into()))
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{"session_id":"s1","display_name":"vps claude",
                     "state":{"awaiting_input":true}}]"#,
            )
            .create_async()
            .await;
        let rows = seed(&reqwest::Client::new(), &server.url(), Some("tok"))
            .await
            .expect("the daemon answered");
        route.assert_async().await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].display_name.as_deref(), Some("vps claude"));
        assert!(
            rows[0]
                .state
                .as_ref()
                .expect("state came back")
                .awaiting_input
        );
    }

    #[tokio::test]
    async fn the_event_stream_is_read_unfiltered_and_every_frame_is_repeated_locally() {
        let mut server = mockito::Server::new_async().await;
        // `^token=tok$` is the assertion that matters: a `types=` allowlist here
        // would silently drop every event type nobody thought to name.
        let route = server
            .mock("GET", "/events")
            .match_query(mockito::Matcher::Regex("^token=tok$".into()))
            .with_header("content-type", "text/event-stream")
            .with_body(concat!(
                ":ping\n\n",
                "event: session-state-changed\n",
                "data: {\"session_id\":\"s1\",\"state\":{\"awaiting_input\":true}}\n\n",
                "event: pty-activity\ndata: {\"session_id\":\"s1\"}\n\n",
            ))
            .create_async()
            .await;
        let state = Arc::new(make_test_app_state());
        store_seed(
            &state,
            "vps",
            vec![SessionInfo {
                session_id: "s1".into(),
                ..Default::default()
            }],
        );
        let mut rx = state.event_bus.subscribe();
        consume_stream(
            &state,
            &reqwest::Client::new(),
            "vps",
            &server.url(),
            Some("tok"),
        )
        .await
        .expect("the stream ended cleanly");
        route.assert_async().await;

        let mut seen = Vec::new();
        while let Ok(AppEvent::RemoteMirrored { event, .. }) = rx.try_recv() {
            seen.push(event);
        }
        assert_eq!(seen, vec!["session-state-changed", "pty-activity"]);
        assert!(
            mirrored_rows(&state)[0]
                .state
                .as_ref()
                .expect("the state change landed on the row")
                .awaiting_input
        );
    }

    /// The end-to-end shape the story asks for: a daemon says a session is
    /// awaiting, and what leaves this machine's own `/events` is a
    /// `session-state-changed` frame a client cannot tell from a local one.
    /// That is what makes the existing badge and notification handlers fire
    /// with no new subscription.
    #[tokio::test]
    async fn a_remote_question_leaves_this_machine_as_an_ordinary_state_change() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/events")
            .match_query(mockito::Matcher::Any)
            .with_header("content-type", "text/event-stream")
            .with_body(concat!(
                "event: session-state-changed\n",
                "data: {\"session_id\":\"s1\",\"state\":",
                "{\"awaiting_input\":true,\"question_confident\":true,",
                "\"question_text\":\"Which one?\"}}\n\n",
            ))
            .create_async()
            .await;
        let state = Arc::new(make_test_app_state());
        let mut rx = state.event_bus.subscribe();
        consume_stream(&state, &reqwest::Client::new(), "vps", &server.url(), None)
            .await
            .expect("the stream ended cleanly");

        let event = rx.try_recv().expect("one event crossed");
        assert_eq!(
            crate::mcp_http::sse_routes::event_type_name_for_test(&event),
            "session-state-changed"
        );
        let payload = crate::mcp_http::sse_routes::event_payload_for_test(&event);
        assert_eq!(payload["session_id"], "s1");
        assert_eq!(payload["state"]["awaiting_input"], true);
        assert_eq!(payload["state"]["question_confident"], true);
        assert_eq!(payload["state"]["question_text"], "Which one?");
    }

    /// A dropped stream must not leave a badge frozen at whatever the last
    /// frame said. The loop re-reads `GET /sessions` before it follows the
    /// stream again, so every reconnect starts from the daemon's own truth.
    ///
    /// The retry delay is passed in at 1ms: this proves the ordering, not how
    /// long the loop waits.
    #[tokio::test]
    async fn every_stream_reconnect_re_reads_the_session_list() {
        let mut server = mockito::Server::new_async().await;
        let list = server
            .mock("GET", "/sessions")
            .match_query(mockito::Matcher::Any)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"session_id":"s1"}]"#)
            .expect_at_least(3)
            .create_async()
            .await;
        // Answers immediately and ends, so the task loops: seed, stream, wait,
        // seed again.
        let stream = server
            .mock("GET", "/events")
            .match_query(mockito::Matcher::Any)
            .with_header("content-type", "text/event-stream")
            .with_body("")
            .expect_at_least(3)
            .create_async()
            .await;

        let state = Arc::new(make_test_app_state());
        state
            .remote
            .force_connected_for_test("vps", &server.url(), None);
        let task = tokio::spawn(run(
            Arc::clone(&state),
            "vps".to_string(),
            Duration::from_millis(1),
        ));
        tokio::time::sleep(Duration::from_millis(400)).await;
        task.abort();

        list.assert_async().await;
        stream.assert_async().await;
        assert_eq!(mirrored_rows(&state).len(), 1);
    }

    #[test]
    fn a_machine_with_no_remote_connection_mirrors_nothing() {
        let state = Arc::new(make_test_app_state());
        assert!(mirrored_rows(&state).is_empty());
        assert!(
            crate::mcp_http::session::session_rows_including_remote(&state).is_empty(),
            "a list call on a machine with no remote connection is the local list"
        );
    }

    #[tokio::test]
    async fn a_daemon_that_rejects_the_token_is_an_error_not_an_empty_mirror() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/sessions")
            .match_query(mockito::Matcher::Any)
            .with_status(401)
            .create_async()
            .await;
        let error = seed(&reqwest::Client::new(), &server.url(), Some("stale"))
            .await
            .expect_err("401 is not a session list");
        assert!(error.contains("401"), "{error}");
    }
}
