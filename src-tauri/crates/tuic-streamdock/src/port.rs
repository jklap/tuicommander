//! The entire coupling surface between this crate and its host.
//!
//! Today the host is an in-process adapter in `src-tauri/src/streamdock/`
//! reading `AppState` directly. If build weight or crash isolation ever
//! justifies moving to a sidecar process, only the host-side impl of these
//! two traits changes (to one reading `GET /sessions` / `GET /events` over
//! HTTP) — nothing in `device/`, `render/`, `policy/`, `coordinator.rs`, or
//! `dispatch.rs` needs to move or change, because none of it has ever seen
//! `AppState`, `SessionState`, or the ParsedEvent vocabulary. This is the
//! whole point of the trait boundary: it's deliberately drawn at "a flat
//! DTO and four verbs," which is the same shape whether the caller is a
//! function call away or an HTTP round-trip away.

use std::pin::Pin;

use futures_lite::Stream;

/// A flat, already-derived snapshot of one session — deliberately **not**
/// the main crate's `SessionState`. Building this is the host adapter's
/// job (`streamdock/source.rs`, reading it via
/// `AppState::session_state_with_shell`, never the raw `session_states`
/// DashMap — see that module's doc comment for why). Everything in
/// `policy/` and `render/` operates only on this type, which is what keeps
/// them independent of the main crate's fast-moving state model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub session_id: String,
    /// Primary label — the `term_aliases` short id (`tc-1`, `nr-2`) when
    /// available, else a truncated display name or branch. Already
    /// truncated/sanitized by the host adapter; `render/face.rs` truncates
    /// again defensively but should never actually need to.
    pub label: String,
    /// One-word activity / `ready (Nm)` escalation text / choice-prompt
    /// option letters — whatever the host adapter decided is worth a
    /// second line. Empty string = no secondary line.
    pub secondary: String,
    /// `None` for a plain shell. Otherwise one of `"starting"`, `"working"`,
    /// `"awaiting_input"`, `"completed"`, `"idle"` — mirroring
    /// `AppState::session_state_with_shell`'s derived `agent_state`
    /// verbatim, so the host adapter does no additional interpretation.
    pub agent_state: Option<String>,
    /// `"busy"` / `"idle"` / `None`, mirroring `shell_state` verbatim — used
    /// only when `agent_state` is `None` (a plain shell).
    pub shell_state: Option<String>,
    pub awaiting_input: bool,
    pub choice_prompt_pending: bool,
    /// The pending choice prompt's option keys, in display order (e.g.
    /// `["y", "n"]`), if `choice_prompt_pending` — empty otherwise. This is
    /// what `dispatch::dispatch` answers a choice prompt with: the
    /// highlighted/first option on a tap, the second on a double-tap. Kept
    /// on the snapshot itself (not fetched separately at press time) so the
    /// coordinator never needs a second round-trip back through
    /// `StateSource` between "gesture resolved" and "action dispatched."
    pub choice_prompt_options: Vec<String>,
    pub rate_limited: bool,
    /// `suggested_actions.is_some()` — the turn-completion declaration.
    pub suggested_actions_pending: bool,
    pub last_activity_ms: u64,
}

/// One "something changed" signal. Doorbell, not data — see
/// `coordinator.rs`'s doc comment for why the whole ingest is built this
/// way (every event just sets `dirty = true`; the actual data comes from
/// the next `StateSource::snapshot()` call).
#[derive(Clone, Debug)]
pub enum Doorbell {
    Dirty,
    /// The event stream lagged or dropped messages — treated identically
    /// to `Dirty` by the coordinator, but surfaced separately so it can be
    /// logged.
    Lagged {
        missed: u64,
    },
}

pub type BoxStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>;

/// Where session state comes from.
pub trait StateSource: Send + Sync {
    fn snapshot(&self) -> Vec<SessionSnapshot>;
    fn subscribe(&self) -> BoxStream<Doorbell>;
}

/// Where a keypress goes. Every method here maps to a real, already-existing
/// TUICommander surface except `focus_session` and `run_ui_action`, which
/// are the two new `AppEvent` variants this integration adds (Phase 4).
pub trait ActionSink: Send + Sync {
    /// `POST /sessions/{id}/write-parts` with `parts` as separate array
    /// elements — never joined into one string. This is not optional:
    /// `write-parts`' per-input bookkeeping runs once per part, and
    /// answering a choice prompt or sending an interrupt byte both depend
    /// on that.
    fn write_parts(&self, session_id: &str, parts: Vec<String>) -> Result<(), String>;
    /// `POST /sessions/{id}/focus` -> `AppEvent::SessionFocusRequested`.
    fn focus_session(&self, session_id: &str) -> Result<(), String>;
    /// `POST /ui/action {name}` -> `AppEvent::UiActionRequested`, gated by
    /// a Rust-side allowlist in the host adapter — never the full action
    /// registry.
    fn run_ui_action(&self, name: &str) -> Result<(), String>;
    /// `POST /mcp/confirm-response`.
    fn answer_confirm(&self, request_id: &str, confirmed: bool) -> Result<(), String>;
}
