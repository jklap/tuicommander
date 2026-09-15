//! `StateSource` implementation reading `AppState` directly — the in-process
//! side of the `tuic_streamdock::port` trait boundary. If this integration
//! ever moves to a sidecar, only a new impl of these same two methods
//! (reading `GET /sessions` / `GET /events` instead) needs to exist; nothing
//! in `tuic_streamdock` itself changes.
//!
//! **Doorbell, not data stream** — see `tuic_streamdock::coordinator`'s own
//! doc comment for the full reasoning. This impl deliberately never parses
//! `AppEvent::PtyParsed`'s inner `parsed["type"]`; every event this crate
//! cares about, whichever session it's for, just sets `dirty = true`, and
//! the coordinator re-derives the real picture from `snapshot()` on its own
//! schedule. That is what keeps this file decoupled from `output_parser.rs`'s
//! fast-moving ParsedEvent vocabulary.

use std::sync::Arc;

use tuic_streamdock::port::{BoxStream, Doorbell, SessionSnapshot, StateSource};

use crate::AppState;

pub(crate) struct AppStateSource {
    pub(crate) state: Arc<AppState>,
}

impl StateSource for AppStateSource {
    /// Reads `AppState::session_state_with_shell` — **never** the raw
    /// `session_states` DashMap directly, which loses `shell_state`,
    /// `queued_commands`, and rate-limit expiry (see that function's own
    /// doc comment in `state.rs`).
    fn snapshot(&self) -> Vec<SessionSnapshot> {
        self.state
            .sessions
            .iter()
            .map(|entry| {
                let session_id = entry.key().clone();
                let (cwd, display_name) = {
                    let session = entry.value().lock();
                    (session.cwd.clone(), session.display_name.clone())
                };
                // `term_aliases` (e.g. "tc-1") is the short, human label the
                // render pipeline is designed around; fall back to the
                // display name, then the cwd's last path component, then
                // the raw session id so a face never renders blank.
                let label = self
                    .state
                    .term_aliases
                    .get(&session_id)
                    .map(|a| a.value().clone())
                    .or(display_name)
                    .or_else(|| {
                        cwd.as_deref()
                            .and_then(|p| std::path::Path::new(p).file_name())
                            .and_then(|n| n.to_str())
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| session_id.clone());

                let st = self.state.session_state_with_shell(&session_id);
                let secondary = st
                    .as_ref()
                    .and_then(|s| s.current_task.clone().or_else(|| s.agent_intent.clone()))
                    .unwrap_or_default();
                let choice_prompt_options = st
                    .as_ref()
                    .and_then(|s| s.choice_prompt.as_ref())
                    .map(|c| c.options.iter().map(|o| o.key.clone()).collect())
                    .unwrap_or_default();

                SessionSnapshot {
                    session_id,
                    label,
                    secondary,
                    agent_state: st.as_ref().and_then(|s| s.agent_state.clone()),
                    shell_state: st.as_ref().and_then(|s| s.shell_state.clone()),
                    awaiting_input: st.as_ref().is_some_and(|s| s.awaiting_input),
                    choice_prompt_pending: st.as_ref().is_some_and(|s| s.choice_prompt.is_some()),
                    choice_prompt_options,
                    rate_limited: st.as_ref().is_some_and(|s| s.rate_limited),
                    suggested_actions_pending: st
                        .as_ref()
                        .is_some_and(|s| s.suggested_actions.is_some()),
                    last_activity_ms: st.as_ref().map_or(0, |s| s.last_activity_ms),
                }
            })
            .collect()
    }

    fn subscribe(&self) -> BoxStream<Doorbell> {
        let mut rx = self.state.event_bus.subscribe();
        let stream = async_stream::stream! {
            loop {
                match rx.recv().await {
                    Ok(_event) => yield Doorbell::Dirty,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                        yield Doorbell::Lagged { missed };
                    }
                    // The sender lives as long as AppState does — in practice this
                    // never actually closes while the app is running.
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                }
            }
        };
        Box::pin(stream)
    }
}
