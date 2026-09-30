use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::Stream;
use serde::Deserialize;

use crate::AppState;
use crate::event_wire::{event_payload, event_type_name};
#[cfg(test)]
use crate::state::AppEvent;

struct SseClientGuard(Arc<AppState>);

impl SseClientGuard {
    fn new(state: Arc<AppState>) -> Self {
        state.sse_client_count.fetch_add(1, Ordering::Relaxed);
        state
            .remote_client_generation
            .fetch_add(1, Ordering::Relaxed);
        Self(state)
    }
}

impl Drop for SseClientGuard {
    fn drop(&mut self) {
        self.0.sse_client_count.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Deserialize)]
pub(super) struct SseQuery {
    /// Comma-separated event type filter (e.g. "repo-changed,session-created").
    /// When omitted, all events are forwarded.
    pub types: Option<String>,
    /// Client-chosen id of this stream. With one, the filter above is only the
    /// *initial* value and `POST /events/types` can widen it while the stream
    /// runs. Without one the filter is fixed for the life of the connection.
    pub stream_id: Option<String>,
}

/// Live type filters of the open `/events` streams, keyed by the client-supplied
/// `stream_id`.
///
/// A client learns it needs a new event type when a panel mounts, long after it
/// connected. Reopening the stream with a wider filter loses every event
/// published between the close and the new subscription — the bus has no replay
/// — so the filter is updated in place instead.
#[derive(Clone, Default)]
pub(crate) struct SseFilters {
    inner: Arc<parking_lot::Mutex<FilterRegistry>>,
}

#[derive(Default)]
struct FilterRegistry {
    streams: std::collections::HashMap<String, FilterSlot>,
    /// Distinguishes two registrations of the same id, so the guard of an
    /// already-replaced stream cannot deregister its successor.
    next_generation: u64,
}

struct FilterSlot {
    generation: u64,
    tx: tokio::sync::watch::Sender<Option<Vec<String>>>,
}

/// Streams that may hold a live filter at once. A guard deregisters each stream
/// as it ends, so this is a backstop, not a working limit: past it a stream
/// still runs, with the filter it connected with.
const MAX_FILTERED_STREAMS: usize = 64;

/// Deregisters a stream's filter when the stream ends. Held by the stream
/// itself, so a client disconnect drops it.
pub(crate) struct FilterGuard {
    filters: SseFilters,
    stream_id: String,
    generation: u64,
}

impl Drop for FilterGuard {
    fn drop(&mut self) {
        let mut registry = self.filters.inner.lock();
        if registry
            .streams
            .get(&self.stream_id)
            .is_some_and(|slot| slot.generation == self.generation)
        {
            registry.streams.remove(&self.stream_id);
        }
    }
}

impl SseFilters {
    /// Register `stream_id` with its initial filter. `None` is "every type".
    /// Returns nothing when the registry is full — the caller then keeps the
    /// filter it was given, and `update` answers false so the client reconnects.
    fn register(
        &self,
        stream_id: &str,
        initial: Option<Vec<String>>,
    ) -> Option<(
        tokio::sync::watch::Receiver<Option<Vec<String>>>,
        FilterGuard,
    )> {
        let mut registry = self.inner.lock();
        if registry.streams.len() >= MAX_FILTERED_STREAMS
            && !registry.streams.contains_key(stream_id)
        {
            tracing::warn!(
                source = "http",
                "Refusing to track the filter of SSE stream \"{stream_id}\": \
                 {MAX_FILTERED_STREAMS} streams already tracked. It runs with a fixed filter."
            );
            return None;
        }
        registry.next_generation += 1;
        let generation = registry.next_generation;
        let (tx, rx) = tokio::sync::watch::channel(initial);
        // Replaces any earlier slot for this id — an EventSource that
        // auto-reconnected reuses its id, and the old guard is about to drop.
        registry
            .streams
            .insert(stream_id.to_string(), FilterSlot { generation, tx });
        Some((
            rx,
            FilterGuard {
                filters: self.clone(),
                stream_id: stream_id.to_string(),
                generation,
            },
        ))
    }

    /// Replace the filter of a live stream. False when that stream is unknown —
    /// it ended, it never sent an id, or the registry was full.
    fn update(&self, stream_id: &str, types: Option<Vec<String>>) -> bool {
        let registry = self.inner.lock();
        match registry.streams.get(stream_id) {
            Some(slot) => {
                let _ = slot.tx.send(types);
                true
            }
            None => false,
        }
    }

    #[cfg(test)]
    fn tracked(&self) -> usize {
        self.inner.lock().streams.len()
    }
}

/// Body of `POST /events/types`.
#[derive(Deserialize)]
pub(super) struct SseTypesBody {
    stream_id: String,
    /// The full set the client wants from now on, not a delta. An empty list is
    /// an empty allowlist, exactly as `?types=` is.
    types: Vec<String>,
}

/// `POST /events/types` — widen (or narrow) the filter of a live SSE stream.
///
/// 404 means the stream is not tracked; the client falls back to reconnecting
/// with a wider `?types=`, which is lossy but still correct.
pub(super) async fn sse_update_types(
    State(state): State<Arc<AppState>>,
    axum::Json(body): axum::Json<SseTypesBody>,
) -> axum::http::StatusCode {
    if state.sse_filters.update(&body.stream_id, Some(body.types)) {
        axum::http::StatusCode::NO_CONTENT
    } else {
        axum::http::StatusCode::NOT_FOUND
    }
}

/// SSE endpoint: `GET /events?types=repo-changed,pty-parsed`
///
/// Subscribes to the broadcast channel and streams events to the client.
/// Supports optional `?types=` filter for comma-separated event names.
/// Uses monotonic event IDs from `state.event_counter`.
pub(super) async fn sse_events(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SseQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.event_bus.subscribe();
    let client_guard = SseClientGuard::new(state.clone());
    let initial_types: Option<Vec<String>> = query.types.map(|t| {
        t.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    });
    // With a stream id the filter is live: the client widens it in place rather
    // than reconnecting, which would drop everything published in between. The
    // guard rides the stream, so the entry disappears when the client goes.
    let tracked = query
        .stream_id
        .as_ref()
        .and_then(|id| state.sse_filters.register(id, initial_types.clone()));
    let (filter_rx, filter_guard) = match tracked {
        Some((rx, guard)) => (Some(rx), Some(guard)),
        None => (None, None),
    };

    let stream = async_stream::stream! {
        let _client_guard = client_guard;
        // Moved in so it lives exactly as long as the stream does.
        let _filter_guard = filter_guard;
        // Send retry directive as first event
        yield Ok(Event::default().retry(Duration::from_secs(5)));

        // See `cpu_watchdog::should_disconnect_for_lag` — `consecutive` resets on
        // every clean recv, `cumulative` never resets for this connection's life.
        // This is the global bus, not a per-session channel, so there is no single
        // session to attribute the lag to — only the connection itself decides
        // whether to keep going.
        let mut consecutive_lag: u32 = 0;
        let mut cumulative_lag: u64 = 0;

        loop {
            match rx.recv().await {
                Ok(event) => {
                    consecutive_lag = 0;
                    let event_name = event_type_name(&event);
                    let allowed = match filter_rx {
                        Some(ref live) => allows(&live.borrow(), event_name),
                        None => allows(&initial_types, event_name),
                    };
                    if !allowed {
                        continue;
                    }
                    let id = state.event_counter.fetch_add(1, Ordering::Relaxed);
                    let payload = match serde_json::to_string(&event_payload(&event)) {
                        Ok(json) => json,
                        Err(_) => continue,
                    };
                    yield Ok(
                        Event::default()
                            .event(event_name)
                            .id(id.to_string())
                            .data(payload)
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    // Client fell behind — tell it, and only keep going while the
                    // gap stays within recoverable bounds. Left unbounded, this used
                    // to loop forever re-lagging with no server-side signal at all
                    // (the sibling per-session WS handlers had the identical gap —
                    // see `mcp_http/session.rs` — which is what the `0b421c3a`
                    // incident's climbing 419ms->12.4s lag traced back to).
                    yield Ok(
                        Event::default()
                            .event("lagged")
                            .data(format!("{{\"missed\":{n}}}")),
                    );
                    consecutive_lag += 1;
                    cumulative_lag = cumulative_lag.saturating_add(n);
                    if crate::cpu_watchdog::should_disconnect_for_lag(consecutive_lag, cumulative_lag) {
                        tracing::warn!(
                            source = "diagnostics",
                            consecutive_lag,
                            cumulative_lag,
                            "SSE broadcast lag exceeded the recovery threshold — closing so the client reconnects with a fresh snapshot"
                        );
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

/// `None` is "every type"; a list is an allowlist, so an empty one passes
/// nothing. That asymmetry is the wire contract: omitting `?types=` asks for
/// everything, sending it empty asks for nothing.
fn allows(filter: &Option<Vec<String>>, event_name: &str) -> bool {
    match filter {
        Some(types) => types.iter().any(|t| t == event_name),
        None => true,
    }
}

/// Let another module's test check what name an event leaves this machine under.
/// The name itself lives in `crate::event_wire` (shared with `AppState::emit_dual`).
#[cfg(test)]
pub(crate) fn event_type_name_for_test(event: &AppEvent) -> &str {
    event_type_name(event)
}

/// Extract just the payload (without the wrapping `event`/`payload` tags).
/// The SSE `event:` field already carries the type, so we only need the inner data.
/// Let another module's test compare this payload against the desktop one.
///
/// The two are built by different code in different files, which is exactly why
/// they drift; a test that can only see one of them cannot catch it.
#[cfg(test)]
pub(crate) fn event_payload_for_test(event: &AppEvent) -> serde_json::Value {
    event_payload(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use futures_util::StreamExt;

    /// Read the next SSE chunk, or `None` if nothing arrives promptly. The
    /// generator is a pull stream: it only advances while polled, so "nothing
    /// arrives" needs a timeout rather than an immediate poll.
    async fn next_chunk(body: &mut axum::body::BodyDataStream) -> Option<String> {
        match tokio::time::timeout(Duration::from_millis(250), body.next()).await {
            Ok(Some(Ok(bytes))) => Some(String::from_utf8_lossy(&bytes).to_string()),
            _ => None,
        }
    }

    fn repo_changed() -> AppEvent {
        AppEvent::RepoChanged {
            repo_path: "/repo".into(),
            kind: crate::repo_watcher::RepoChangeKind::WorkingTree,
        }
    }

    fn dir_changed() -> AppEvent {
        AppEvent::DirChanged {
            dir_path: "/dir".into(),
        }
    }

    #[tokio::test]
    async fn event_stream_lifetime_tracks_only_the_connected_client() {
        let state = crate::mcp_http::tests::test_state();
        let _internal = state.event_bus.subscribe();
        assert_eq!(state.sse_client_count.load(Ordering::Relaxed), 0);

        let response = sse_events(
            State(state.clone()),
            Query(SseQuery {
                types: None,
                stream_id: None,
            }),
        )
        .await
        .into_response();
        assert_eq!(state.sse_client_count.load(Ordering::Relaxed), 1);

        drop(response);
        assert_eq!(state.sse_client_count.load(Ordering::Relaxed), 0);
    }

    /// A panel that mounts late adds an event type the stream was not opened
    /// with. Reopening the stream to widen the filter loses everything published
    /// between the close and the new subscription — the bus has no replay — so
    /// the filter has to change on the live stream.
    #[tokio::test]
    async fn a_live_stream_widens_its_filter_without_reconnecting() {
        let state = crate::mcp_http::tests::test_state();
        let response = sse_events(
            State(state.clone()),
            Query(SseQuery {
                types: Some("repo-changed".into()),
                stream_id: Some("s1".into()),
            }),
        )
        .await
        .into_response();
        let mut body = response.into_body().into_data_stream();
        assert!(
            next_chunk(&mut body)
                .await
                .is_some_and(|c| c.contains("retry")),
            "the stream opens with the retry directive"
        );

        let _ = state.event_bus.send(repo_changed());
        assert!(
            next_chunk(&mut body)
                .await
                .is_some_and(|c| c.contains("repo-changed")),
            "the type it connected with arrives"
        );

        let _ = state.event_bus.send(dir_changed());
        assert!(
            next_chunk(&mut body).await.is_none(),
            "a type outside the filter is dropped"
        );

        assert_eq!(
            sse_update_types(
                State(state.clone()),
                axum::Json(SseTypesBody {
                    stream_id: "s1".into(),
                    types: vec!["repo-changed".into(), "dir-changed".into()],
                }),
            )
            .await,
            axum::http::StatusCode::NO_CONTENT
        );

        let _ = state.event_bus.send(dir_changed());
        assert!(
            next_chunk(&mut body)
                .await
                .is_some_and(|c| c.contains("dir-changed")),
            "the widened type arrives on the same connection"
        );
    }

    #[tokio::test]
    async fn a_stream_without_an_id_keeps_the_filter_it_connected_with() {
        let state = crate::mcp_http::tests::test_state();
        let response = sse_events(
            State(state.clone()),
            Query(SseQuery {
                types: Some("repo-changed".into()),
                stream_id: None,
            }),
        )
        .await
        .into_response();
        let mut body = response.into_body().into_data_stream();
        next_chunk(&mut body).await;

        // Nothing to update: the client must reconnect, and it learns that from
        // the 404 rather than from a silent success.
        assert_eq!(
            sse_update_types(
                State(state.clone()),
                axum::Json(SseTypesBody {
                    stream_id: "unknown".into(),
                    types: vec!["dir-changed".into()],
                }),
            )
            .await,
            axum::http::StatusCode::NOT_FOUND
        );

        let _ = state.event_bus.send(dir_changed());
        assert!(next_chunk(&mut body).await.is_none());
    }

    /// Before this fix, a lagging SSE stream sent a `"lagged"` event and kept
    /// looping forever — the exact shape behind the `0b421c3a` incident's
    /// climbing 419ms->12.4s lag on the sibling per-session WS handlers (this
    /// route rides the *global* bus, so no single session owns the lag, but
    /// the same unbounded-retry gap existed here too). Publishing far more
    /// than the channel's capacity before ever polling forces the very first
    /// `recv()` to report a `missed` count comfortably past
    /// `cpu_watchdog::MAX_CUMULATIVE_LAG` in one shot, so this doesn't need to
    /// orchestrate several consecutive `Lagged`s to prove the point.
    #[tokio::test]
    async fn a_stream_that_lags_past_the_cumulative_bound_closes_instead_of_looping_forever() {
        let state = crate::mcp_http::tests::test_state();
        let response = sse_events(
            State(state.clone()),
            Query(SseQuery {
                types: None,
                stream_id: None,
            }),
        )
        .await
        .into_response();
        let mut body = response.into_body().into_data_stream();
        assert!(
            next_chunk(&mut body)
                .await
                .is_some_and(|c| c.contains("retry")),
            "the stream opens with the retry directive"
        );

        // Far more than the bus's capacity (256), all before the stream is
        // ever polled again, so the first recv() sees one big Lagged report.
        for _ in 0..2000 {
            let _ = state.event_bus.send(repo_changed());
        }

        let lagged_chunk = next_chunk(&mut body)
            .await
            .expect("a lag this large must be reported, not silently dropped");
        assert!(
            lagged_chunk.contains("lagged"),
            "expected a lagged event, got: {lagged_chunk}"
        );

        // If the loop had kept going (the pre-fix behavior), this next event —
        // published with no filter active to hide it — would show up as the
        // very next chunk. It must not: the stream already closed.
        let _ = state.event_bus.send(dir_changed());
        assert!(
            next_chunk(&mut body).await.is_none(),
            "the stream must have closed after crossing the cumulative lag bound, \
             not kept accepting further events"
        );
    }

    /// The registry must not outlive the streams it describes: every entry is
    /// dropped with its stream, and an EventSource that auto-reconnects reuses
    /// its id, so the guard of the *previous* connection must not take the new
    /// slot with it.
    #[test]
    fn a_filter_entry_dies_with_its_stream_and_never_takes_its_successor() {
        let filters = SseFilters::default();
        let (_rx, first) = filters.register("s1", None).expect("registered");
        assert_eq!(filters.tracked(), 1);

        let (_rx2, second) = filters.register("s1", None).expect("re-registered");
        assert_eq!(filters.tracked(), 1, "one id is one slot");
        drop(first);
        assert!(
            filters.update("s1", Some(vec!["repo-changed".into()])),
            "the reconnected stream still owns the slot"
        );

        drop(second);
        assert_eq!(filters.tracked(), 0);
        assert!(!filters.update("s1", None));
    }

    #[test]
    fn the_filter_registry_is_bounded() {
        let filters = SseFilters::default();
        let guards: Vec<_> = (0..MAX_FILTERED_STREAMS)
            .map(|i| {
                filters
                    .register(&format!("s{i}"), None)
                    .expect("registered")
            })
            .collect();
        assert!(
            filters.register("one-too-many", None).is_none(),
            "past the cap a stream runs with the filter it connected with"
        );
        // An already-tracked id is not a new stream, so it still updates.
        assert!(filters.register("s0", None).is_some());
        drop(guards);
    }

    #[test]
    fn an_absent_filter_passes_everything_and_an_empty_one_nothing() {
        assert!(allows(&None, "repo-changed"));
        assert!(!allows(&Some(Vec::new()), "repo-changed"));
        assert!(allows(&Some(vec!["repo-changed".into()]), "repo-changed"));
        assert!(!allows(&Some(vec!["repo-changed".into()]), "dir-changed"));
    }

    #[test]
    fn session_created_preserves_stable_display_name() {
        let event = AppEvent::SessionCreated {
            session_id: "session-1".into(),
            cwd: Some("/repo".into()),
            agent_type: Some("codex".into()),
            display_name: Some("linux-primary".into()),
            parent_session: Some("tuic-parent".into()),
            is_remote: true,
        };

        assert_eq!(event_type_name(&event), "session-created");
        let body = event_payload(&event);
        assert_eq!(body["session_id"], "session-1");
        assert_eq!(body["cwd"], "/repo");
        assert_eq!(body["agent_type"], "codex");
        assert_eq!(body["display_name"], "linux-primary");
        // Browser clients tag sub-agent tabs from this field, as the desktop does.
        assert_eq!(body["parent_session"], "tuic-parent");
    }

    #[test]
    fn session_created_payload_carries_is_remote() {
        for is_remote in [true, false] {
            let event = AppEvent::SessionCreated {
                session_id: "session-1".into(),
                cwd: None,
                agent_type: None,
                display_name: None,
                parent_session: None,
                is_remote,
            };
            let body = event_payload(&event);
            assert_eq!(body["is_remote"], is_remote, "body: {body}");
        }
    }

    #[test]
    fn pty_description_changed_has_matching_sse_name_and_payload() {
        let event = AppEvent::PtyDescriptionChanged {
            session_id: "session-1".into(),
            description: None,
        };

        assert_eq!(event_type_name(&event), "pty-description-changed");
        assert_eq!(
            event_payload(&event),
            serde_json::json!({"session_id": "session-1", "description": null})
        );
    }

    #[test]
    fn term_alias_assigned_has_matching_sse_name_and_payload() {
        let event = AppEvent::TermAliasAssigned {
            session_id: "session-1".into(),
            alias: "tu-3".into(),
        };

        assert_eq!(event_type_name(&event), "term-alias-assigned");
        assert_eq!(
            event_payload(&event),
            serde_json::json!({"session_id": "session-1", "alias": "tu-3"})
        );
    }

    #[test]
    fn session_accent_color_changed_has_matching_sse_name_and_payload() {
        let event = AppEvent::SessionAccentColorChanged {
            session_id: "session-1".into(),
            color: Some("blue".into()),
        };
        assert_eq!(event_type_name(&event), "session-accent-color-changed");
        assert_eq!(
            event_payload(&event),
            serde_json::json!({"session_id": "session-1", "color": "blue"})
        );
    }

    #[test]
    fn tmux_window_layout_requested_has_matching_sse_name_and_payload() {
        let event = AppEvent::TmuxWindowLayoutRequested {
            session_ids: vec!["a".into(), "b".into()],
            layout: "tiled".into(),
        };
        assert_eq!(event_type_name(&event), "tmux-window-layout-requested");
        assert_eq!(
            event_payload(&event),
            serde_json::json!({"session_ids": ["a", "b"], "layout": "tiled"})
        );
    }

    /// The GitHub Ops lifecycle events share a `{repo_path, payload}` shape.
    /// Each must map to its own SSE `event:` name and round-trip the payload
    /// verbatim so browser/PWA clients receive the same data as desktop.
    ///
    /// Two of the three original cases (`review-progress`, `proposals-ready`)
    /// went with the embedded engine that produced them (#784-0aec); the arm
    /// they shared is what this still guards.
    #[test]
    fn ops_lifecycle_events_have_distinct_names_and_passthrough_payload() {
        let payload = serde_json::json!({ "pr_number": 42, "phase": "done", "done": true });
        let cases: Vec<(AppEvent, &str)> = vec![
            (
                AppEvent::ConflictAssistStatus {
                    repo_path: "/repo".into(),
                    payload: payload.clone(),
                },
                "conflict-assist-status",
            ),
            (
                AppEvent::ProgressRecorded {
                    repo_path: "/repo".into(),
                    payload: payload.clone(),
                },
                "progress-recorded",
            ),
            (
                AppEvent::ReviewProgress {
                    repo_path: "/repo".into(),
                    payload: payload.clone(),
                },
                "review-progress",
            ),
            (
                AppEvent::ProposalsReady {
                    repo_path: "/repo".into(),
                    payload: payload.clone(),
                },
                "proposals-ready",
            ),
        ];

        // Names must all be distinct and match the expected kebab-case tag.
        let mut seen = std::collections::HashSet::new();
        for (event, expected_name) in &cases {
            assert_eq!(event_type_name(event), *expected_name);
            assert!(
                seen.insert(*expected_name),
                "duplicate event name {expected_name}"
            );
            let body = event_payload(event);
            assert_eq!(body["repo_path"], "/repo");
            assert_eq!(body["payload"], payload);
        }
    }

    #[test]
    fn progress_recorded_has_the_same_receipt_and_event_envelope_on_sse() {
        let payload = serde_json::json!({
            "receipt": {"status":"recorded", "revision":7, "eventId":"event-7"},
            "event": {"id":"event-7", "revision":7, "type":"milestone", "summary":"Done."}
        });
        let event = AppEvent::ProgressRecorded {
            repo_path: "/repo".into(),
            payload: payload.clone(),
        };

        assert_eq!(event_type_name(&event), "progress-recorded");
        assert_eq!(
            event_payload(&event),
            serde_json::json!({"repo_path":"/repo", "payload":payload})
        );
    }

    #[test]
    fn workflow_run_changed_is_a_cursor_wake_hint() {
        let payload = serde_json::json!({ "runId": "run-1", "sequence": 7 });
        let event = AppEvent::WorkflowRunChanged {
            repo_path: "/repo".into(),
            payload: payload.clone(),
        };
        assert_eq!(event_type_name(&event), "workflow-run-changed");
        assert_eq!(
            event_payload(&event),
            serde_json::json!({
                "repo_path": "/repo", "payload": payload,
            })
        );
    }

    /// A browser learns a session's lifecycle from this arm; the desktop learns
    /// it from the window event of the same name. Both must hand the frontend
    /// the shape `list_active_sessions` returns per session — `{session_id,
    /// state}` with `state` in snake_case — because one applier consumes all
    /// three and a renamed key silently stops updating a badge.
    #[test]
    fn session_state_changed_carries_a_list_active_sessions_entry() {
        let event = AppEvent::SessionStateChanged {
            session_id: "sess-1".into(),
            state: Box::new(crate::state::SessionState {
                awaiting_input: true,
                question_confident: true,
                shell_state: Some("idle".into()),
                agent_state: Some("awaiting_input".into()),
                queued_commands: 2,
                ..Default::default()
            }),
        };
        assert_eq!(event_type_name(&event), "session-state-changed");
        let body = event_payload(&event);
        assert_eq!(body["session_id"], "sess-1");
        assert_eq!(body["state"]["awaiting_input"], true);
        assert_eq!(body["state"]["question_confident"], true);
        assert_eq!(body["state"]["shell_state"], "idle");
        assert_eq!(body["state"]["agent_state"], "awaiting_input");
        assert_eq!(body["state"]["queued_commands"], 2);
        // Not flattened: the frontend reads `payload.state.*`, exactly as it
        // reads `session.state.*` from the polled snapshot it replaces.
        assert!(body.get("awaiting_input").is_none());
    }

    #[test]
    fn worktree_sync_events_use_camelcase_matching_window_events() {
        // Mirrors `worktree_create_failed_uses_camelcase_matching_window_event`:
        // `worktree::run_worktree_file_sync` dual-emits all three of these on
        // the bus (SSE) AND the Tauri window with identical camelCase keys, so
        // a single frontend listener consumes both transports unchanged.
        let started = AppEvent::WorktreeSyncStarted {
            repo_path: "/repo".into(),
            branch: "feat-x".into(),
        };
        assert_eq!(event_type_name(&started), "worktree-sync-started");
        let body = event_payload(&started);
        assert_eq!(body["repoPath"], "/repo");
        assert_eq!(body["branch"], "feat-x");
        assert!(body.get("repo_path").is_none());

        let progress = AppEvent::WorktreeSyncProgress {
            repo_path: "/repo".into(),
            branch: "feat-x".into(),
            copied: 3,
            total: 10,
        };
        assert_eq!(event_type_name(&progress), "worktree-sync-progress");
        let body = event_payload(&progress);
        assert_eq!(body["repoPath"], "/repo");
        assert_eq!(body["branch"], "feat-x");
        assert_eq!(body["copied"], 3);
        assert_eq!(body["total"], 10);

        let completed = AppEvent::WorktreeSyncCompleted {
            repo_path: "/repo".into(),
            branch: "feat-x".into(),
            copied: 9,
            total: 10,
            errors: vec!["missing.txt: source path does not exist".into()],
        };
        assert_eq!(event_type_name(&completed), "worktree-sync-completed");
        let body = event_payload(&completed);
        assert_eq!(body["repoPath"], "/repo");
        assert_eq!(body["branch"], "feat-x");
        assert_eq!(body["copied"], 9);
        assert_eq!(body["total"], 10);
        assert_eq!(body["errors"][0], "missing.txt: source path does not exist");
    }

    #[test]
    fn worktree_warm_events_use_camelcase_matching_window_events() {
        let started = AppEvent::WorktreeWarmStarted {
            repo_path: "/repo".into(),
            branch: "feat-x".into(),
            worktree_path: "/wt/feat-x".into(),
            total: 3,
        };
        assert_eq!(event_type_name(&started), "worktree-warm-started");
        assert_eq!(
            event_payload(&started),
            serde_json::json!({"repoPath": "/repo", "branch": "feat-x", "worktreePath": "/wt/feat-x", "total": 3})
        );

        let progress = AppEvent::WorktreeWarmProgress {
            repo_path: "/repo".into(),
            branch: "feat-x".into(),
            worktree_path: "/wt/feat-x".into(),
            copied: 2,
            total: 3,
            current: Some("node_modules".into()),
        };
        assert_eq!(event_type_name(&progress), "worktree-warm-progress");
        let body = event_payload(&progress);
        assert_eq!(body["copied"], 2);
        assert_eq!(body["total"], 3);
        assert_eq!(body["current"], "node_modules");
        assert_eq!(body["worktreePath"], "/wt/feat-x");

        let completed = AppEvent::WorktreeWarmCompleted {
            repo_path: "/repo".into(),
            branch: "feat-x".into(),
            worktree_path: "/wt/feat-x".into(),
            warmed: 2,
            warnings: vec!["target stayed cold".into()],
        };
        assert_eq!(event_type_name(&completed), "worktree-warm-completed");
        let body = event_payload(&completed);
        assert_eq!(body["warmed"], 2);
        assert_eq!(body["warnings"][0], "target stayed cold");
        assert!(body.get("worktree_path").is_none());
    }

    #[test]
    fn worktree_setup_script_completed_uses_camelcase_matching_window_event() {
        // `worktree::spawn_worktree_setup_chain` dual-emits this on the bus
        // (SSE) AND the Tauri window with identical camelCase keys.
        let event = AppEvent::WorktreeSetupScriptCompleted {
            repo_path: "/repo".into(),
            branch: "feat-x".into(),
            worktree_path: "/repo/worktrees/feat-x".into(),
            outcome: crate::state::SetupChainOutcome::Completed,
            exit_code: Some(1),
            error: None,
        };
        assert_eq!(event_type_name(&event), "worktree-setup-script-completed");
        let body = event_payload(&event);
        assert_eq!(body["repoPath"], "/repo");
        assert_eq!(body["branch"], "feat-x");
        assert_eq!(body["worktreePath"], "/repo/worktrees/feat-x");
        assert_eq!(body["outcome"], "completed");
        assert_eq!(body["exitCode"], 1);
        assert!(body["error"].is_null());
        assert!(body.get("repo_path").is_none());
        assert!(body.get("worktree_path").is_none());
    }

    #[test]
    fn design_mode_changed_carries_the_bound_session_and_status() {
        let event = AppEvent::DesignModeChanged {
            repo_path: "/repo".into(),
            session_id: "agent-1".into(),
            status: "armed".into(),
        };
        assert_eq!(event_type_name(&event), "design-mode-changed");
        assert_eq!(
            event_payload(&event),
            serde_json::json!({
                "repo_path": "/repo", "session_id": "agent-1", "status": "armed"
            })
        );
    }

    /// An ACP wake signal reaches a browser unchanged.
    ///
    /// Field-for-field the object the `/acp` routes and the desktop commands
    /// return, camelCase included: a client that reacts by fetching the
    /// connection snapshot has to be able to use the ids it was just handed,
    /// and a renamed key here would be a second spelling for one thing.
    #[test]
    fn an_acp_notice_reaches_the_browser_as_the_object_the_acp_routes_return() {
        let connection_id = crate::acp::AcpConnectionId::new();
        let request_id = crate::acp::AcpHostRequestId::new();
        let event = AppEvent::AcpNotice(crate::acp::AcpNotice {
            connection_id,
            generation: 3,
            session_id: Some(agent_client_protocol::schema::v1::SessionId::new("sess-1")),
            request_id: Some(request_id),
            sequence: 42,
            kind: crate::acp::AcpNoticeKind::InteractionPending,
        });
        assert_eq!(event_type_name(&event), "acp-notice");
        let body = event_payload(&event);
        assert_eq!(body["connectionId"], serde_json::json!(connection_id));
        assert_eq!(body["requestId"], serde_json::json!(request_id));
        assert_eq!(body["sessionId"], "sess-1");
        assert_eq!(body["generation"], 3);
        assert_eq!(body["sequence"], 42);
        assert_eq!(body["kind"], "interaction_pending");
        assert!(body.get("connection_id").is_none());
    }
}
