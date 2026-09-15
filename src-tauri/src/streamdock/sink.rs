//! `ActionSink` implementation — the in-process side of a keypress reaching
//! a real backend effect. Every method here calls the exact same shared
//! function the HTTP route and/or Tauri IPC command already call, so a
//! macropad press is indistinguishable from any other loopback client's
//! request — no new trust boundary, no new surface to keep in sync.

use std::sync::Arc;

use tuic_streamdock::port::ActionSink;

use crate::AppState;

pub(crate) struct AppStateSink {
    pub(crate) state: Arc<AppState>,
}

impl ActionSink for AppStateSink {
    /// Must be `write_pty_input_parts` (N atomic parts), never a joined
    /// `/write` string — per-input bookkeeping runs once per part, and
    /// answering a choice prompt or sending an interrupt byte both depend
    /// on that (see `mcp_http/mod.rs`'s comment on `/sessions/{id}/write-parts`).
    fn write_parts(&self, session_id: &str, parts: Vec<String>) -> Result<(), String> {
        let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
        crate::mcp_http::session::write_pty_input_parts(&self.state, session_id, &refs)
    }

    fn focus_session(&self, session_id: &str) -> Result<(), String> {
        crate::mcp_http::session::focus_session_impl(&self.state, session_id)
    }

    fn run_ui_action(&self, name: &str) -> Result<(), String> {
        crate::mcp_http::session::run_ui_action_impl(&self.state, name)
    }

    fn answer_confirm(&self, request_id: &str, confirmed: bool) -> Result<(), String> {
        crate::mcp_http::resolve_mcp_confirm(&self.state, request_id, confirmed);
        Ok(())
    }
}
