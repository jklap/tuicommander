use dashmap::DashMap;
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::Serialize;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
#[cfg(feature = "desktop")]
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::input_line_buffer::{InputAction, InputLineBuffer};
use crate::output_parser::{OutputParser, ParsedEvent};
use crate::state::{
    AppState, ChangedRow, EscapeAwareBuffer, KittyAction, KittyKeyboardState,
    MAX_CONCURRENT_SESSIONS, OUTPUT_RING_BUFFER_CAPACITY, OrchestratorStats, OutputRingBuffer,
    PtyConfig, PtySession, Utf8ReadBuffer, VT_LOG_BUFFER_CAPACITY, strip_kitty_sequences,
};
use crate::worktree::{
    WorktreeConfig, WorktreeResult, create_worktree_with_stale_recovery, remove_worktree_internal,
};

mod bg_wake_diagnostics;
pub(crate) mod frame_priority;
mod injection;
mod lifecycle;
mod screen;

pub(crate) use injection::*;
pub(crate) use lifecycle::*;
pub(crate) use screen::*;

#[cfg(all(test, unix))]
mod child_reaping_tests;
#[cfg(feature = "desktop")]
mod commands;
#[cfg(feature = "desktop")]
pub(crate) use commands::*;

// Not desktop-gated: shared by both the (desktop-only) Tauri command in
// `commands.rs` and the HTTP route in `mcp_http/session.rs`, which compiles
// regardless of the `desktop` feature.
mod explain;
pub(crate) use explain::*;

/// Capacity of the per-session raw-byte flight recorder (story 056-7545).
/// 2 MiB ≈ several minutes of heavy agent output — enough to capture the
/// corruption window when a duplication shows up in the wild.
const PTY_RAW_RING_CAP: usize = 2 * 1024 * 1024;

/// One watcher-line batch per this window. Each batch is a Tauri event the
/// WebView main thread must deserialize and dispatch; per-chunk emission
/// starved the event loop under an output flood (`yes`), so keydown never ran
/// and Ctrl+C never reached write_pty.
const WATCHER_LINE_WINDOW: std::time::Duration = std::time::Duration::from_millis(100);

/// Emit early once this much text is batched, so a flood cannot grow the batch
/// without bound.
const WATCHER_BATCH_CAP: usize = 256 * 1024;

/// One "this session produced output" pulse per this window.
///
/// DROPPING PULSES INSIDE THE WINDOW IS CORRECT, and that is what separates this
/// from the `pty-output` throttle deleted in `cda39f31`. That one dropped chunks
/// of a byte stream whose reassembler (`LineBuffer`) carried a partial line
/// across the gap, so a drop spliced the tail of one chunk onto the head of a
/// later one and produced a line that never existed (audit F1). This pulse
/// carries no payload and is idempotent: "output happened" does not accumulate,
/// so N pulses in a window and one pulse in a window mean the same thing.
/// Nothing reassembles it, nothing can be spliced, and there is no state to
/// carry across a dropped pulse.
///
/// So do NOT "fix" this into a coalescer with a buffer behind it. There is
/// nothing for such a buffer to hold.
///
/// One per second, not the old ten: the only consumers are a last-seen timestamp
/// rendered at second resolution and a boolean unread flag that latches on the
/// first pulse.
const ACTIVITY_PULSE_WINDOW: std::time::Duration = std::time::Duration::from_secs(1);

/// The ONLY way to announce a new session. Every creation path calls this —
/// `create_pty`/`create_pty_with_worktree` (`pty/commands.rs`),
/// `register_pty_session` (`mcp_http::session`, itself shared by
/// `spawn_pty_session`, the HTTP agent-spawn route, and MCP `agent
/// action=spawn` — the latter publishes bus-only for a print-mode spawn), and
/// `agent::spawn_agent`.
///
/// It must be the FIRST session-scoped announcement for a new id: anything
/// else (`assign_term_alias`, an accent color, an initial rename) goes after.
///
/// `agent_type` MUST be the exact same value the caller's own
/// `session_states` insert used (see `apply_event_to_session_state`'s
/// `SessionCreated` arm, `state.rs`, which unconditionally overwrites
/// `agent_type` from this event on the entry it just created/updated) — a
/// caller that presets `agent_type` locally and then emits `None` here wipes
/// its own preset.
pub(crate) fn emit_session_created(
    state: &AppState,
    session_id: &str,
    cwd: Option<String>,
    agent_type: Option<String>,
    display_name: Option<String>,
    parent_session: Option<String>,
) {
    state.emit_dual(session_created_event(
        state,
        session_id,
        cwd,
        agent_type,
        display_name,
        parent_session,
    ));
}

/// The `SessionCreated` event for `session_id`, carrying the live TUIC identity
/// bound to that PTY (`tuic_session`) so a new tab can match its caller's
/// worktree declaration without a reconnect. Bus-only publishers (MCP print-mode
/// spawns) build the same event through here, so both shapes stay identical.
pub(crate) fn session_created_event(
    state: &AppState,
    session_id: &str,
    cwd: Option<String>,
    agent_type: Option<String>,
    display_name: Option<String>,
    parent_session: Option<String>,
) -> crate::state::AppEvent {
    crate::state::AppEvent::SessionCreated {
        session_id: session_id.to_string(),
        tuic_session: crate::mcp_http::session::live_tuic_sessions_by_pty(state).remove(session_id),
        cwd,
        agent_type,
        display_name,
        parent_session,
    }
}

/// The ONLY way to announce a closed session. Every close path calls this —
/// `close_pty_core`, `kill_pty_core`, `cleanup_session`, and the reader
/// thread's own EOF handling.
///
/// Reads `agent_type` from `session_states` BEFORE building/sending the
/// event, not after: the accumulator's own `SessionClosed` arm removes that
/// entry the moment this event is applied, so a read that happens after
/// `emit_dual` returns can already find nothing there. This ordering is what
/// fixes the historical case where the reader-thread's EOF path "usually"
/// reported `agent_type: None`.
pub(crate) fn emit_session_closed(state: &AppState, session_id: &str, reason: &str) {
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|s| s.agent_type.clone());
    state.emit_dual(crate::state::AppEvent::SessionClosed {
        session_id: session_id.to_string(),
        reason: reason.to_string(),
        agent_type,
    });
}

/// Tell this session's frontends that output is flowing. Payload-free by design
/// — see [`ACTIVITY_PULSE_WINDOW`].
///
/// Dual-emitted because there is no bus→window forwarder: the desktop Tauri
/// event and the bus push are two separate writes of one signal. Desktop reads
/// `pty-activity-{id}`, browser/PWA reads the `{"type":"activity"}` frame on the
/// session WebSocket, and both arrive through `subscribePty` on the frontend.
fn emit_pty_activity(state: &AppState, session_id: &str) {
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        let _ = app.emit(
            &format!("pty-activity-{session_id}"),
            serde_json::json!({ "session_id": session_id }),
        );
    }
    state.emit_pty_event(crate::state::AppEvent::PtyActivity {
        session_id: session_id.to_string(),
    });
}

/// Rate limiter for [`emit_pty_activity`], owned by the PTY reader thread.
///
/// A struct rather than a bare `Option<Instant>` in the read loop so the
/// throttle can be driven by a test: the loop itself needs a live PTY, but the
/// decision of when a pulse is due does not.
struct ActivityPulse {
    last: Option<std::time::Instant>,
}

impl ActivityPulse {
    fn new() -> Self {
        Self { last: None }
    }

    /// Emit a pulse if one is due. `None` fires immediately, so a session that
    /// emits one short burst and then goes quiet still reports it.
    fn pulse(&mut self, state: &AppState, session_id: &str) {
        if self
            .last
            .is_none_or(|t| t.elapsed() >= ACTIVITY_PULSE_WINDOW)
        {
            emit_pty_activity(state, session_id);
            self.last = Some(std::time::Instant::now());
        }
    }
}

/// Push one batch of assembled lines to the frontends of this session.
fn emit_watcher_lines(
    state: &AppState,
    session_id: &str,
    lines: Vec<crate::output_watchers::WatcherLine>,
) {
    if lines.is_empty() {
        return;
    }
    #[cfg(feature = "desktop")]
    {
        if let Some(app) = state.app_handle.read().as_ref() {
            let _ = app.emit(
                &format!("pty-watcher-lines-{session_id}"),
                crate::output_watchers::WatcherLines {
                    session_id: session_id.to_string(),
                    lines: lines.clone(),
                },
            );
        }
    }
    // The bus carries the same batch to browser/PWA clients over their session
    // WebSocket and to the SSE stream. Those are bounded broadcast channels: a
    // client that lags far enough behind loses events, and nothing replays them.
    // Batching is what keeps that theoretical — a session emits at most ten of
    // these per second regardless of how many lines match — but a browser
    // watcher is best-effort where a desktop one is not.
    state.emit_pty_event(crate::state::AppEvent::PluginWatcherLines {
        session_id: session_id.to_string(),
        lines,
    });
}

/// Assemble the lines of one PTY chunk and match them against the plugin
/// OutputWatchers. Runs on the reader thread: the WebView is woken for the
/// lines that matched instead of ANSI-stripping and regex-testing every line on
/// the thread that paints the terminal (audit F3).
///
/// Rust is the only line assembler. When no watcher is registered the chunk
/// still goes through [`StreamLines`], because a watcher that registers
/// mid-line must still see that line whole — with two assemblers the line was
/// split between them and seen by neither.
fn assemble_watcher_lines(
    state: &AppState,
    session_id: &str,
    chunk: &str,
    lines: &mut crate::output_watchers::StreamLines,
    batcher: &parking_lot::Mutex<crate::output_watchers::WatcherLineBatcher>,
    eof: bool,
) {
    // One read lock for the whole chunk: the compiled set and its "I still need
    // every line" answer must come from the same snapshot, or a line published
    // between the two is matched by neither side.
    let watchers = state.plugin_output_watchers.read();
    if watchers.is_idle() {
        drop(watchers);
        lines.push_discarding(chunk);
        return;
    }
    let needs_all = watchers.needs_all_lines();
    let mut assembled = lines.push(chunk);
    // At end of stream the unterminated tail is a line too: `printf DONE` put
    // `DONE` on the wire and then exited, and nothing is coming to close it.
    if eof && let Some(tail) = lines.flush() {
        assembled.push(tail);
    }
    let mut pending = Vec::new();
    for raw_line in assembled {
        let text = crate::output_watchers::clean_line(&raw_line);
        let matched_ids = watchers.matching_ids(&text);
        if matched_ids.is_empty() && !needs_all {
            continue;
        }
        pending.push(crate::output_watchers::WatcherLine { text, matched_ids });
    }
    drop(watchers);
    if pending.is_empty() {
        return;
    }

    let now = std::time::Instant::now();
    // Emit under the batcher lock: releasing it between take and emit lets the
    // frame ticker interleave and deliver an older tail after a newer batch.
    let mut batch = batcher.lock();
    for line in pending {
        if let Some(due) = batch.push(line, now) {
            emit_watcher_lines(state, session_id, due);
        }
    }
    drop(batch);
}

/// macOS thread QoS for the interactive terminal path.
///
/// `lower_pty_child_priority` nices compiler/test workloads *down*, but on
/// Apple Silicon the scheduler is QoS-band driven: nice only reorders threads
/// *within* a band, so under a saturating `cargo build` our PTY reader, frame
/// ticker, and keystroke-write threads — all at default QoS — still waited
/// behind the compiler's many default-QoS worker threads. Raising our own
/// threads to USER_INTERACTIVE puts the interactive path in a higher band, the
/// lever that keeps typing/echo responsive under load (the native trick AppKit
/// apps get for free on the foreground GUI thread).
///
/// macOS-only: Linux/Windows have no per-thread QoS equivalent that helps here
/// (raising priority needs privilege); there we rely on lowering children.
#[cfg(target_os = "macos")]
mod thread_qos {
    use std::os::raw::{c_int, c_uint};

    /// `QOS_CLASS_USER_INTERACTIVE` from `<sys/qos.h>`.
    const QOS_CLASS_USER_INTERACTIVE: c_uint = 0x21;

    unsafe extern "C" {
        fn pthread_set_qos_class_self_np(qos_class: c_uint, relative_priority: c_int) -> c_int;
        // NOT cfg(test): this was test-only when the only reader was a test probe,
        // but `QosBoost` reads the class in every build to restore it afterwards.
        // Leaving the gate on compiled fine under `cargo test` and broke clippy,
        // the release build and the headless `tuic-remote` binary.
        fn pthread_get_qos_class_np(
            thread: libc::pthread_t,
            qos_class: *mut c_uint,
            relative_priority: *mut c_int,
        ) -> c_int;
    }

    /// Raise the calling thread to USER_INTERACTIVE QoS. Best-effort: a failure
    /// leaves the thread at its current QoS (degraded latency, not broken), so
    /// the non-zero return is intentionally ignored.
    pub(super) fn raise_self_to_user_interactive() {
        set_self_qos(QOS_CLASS_USER_INTERACTIVE, 0);
    }

    fn set_self_qos(class: c_uint, relative_priority: c_int) {
        // SAFETY: extern "C" call with scalar args; affects only the calling thread.
        unsafe {
            pthread_set_qos_class_self_np(class, relative_priority);
        }
    }

    /// Read back the calling thread's QoS class and relative priority.
    fn current_qos() -> (c_uint, c_int) {
        let mut class: c_uint = 0;
        let mut rel: c_int = 0;
        // SAFETY: out-params point to valid stack locals; pthread_self is always valid.
        unsafe {
            pthread_get_qos_class_np(libc::pthread_self(), &mut class, &mut rel);
        }
        (class, rel)
    }

    #[cfg(test)]
    pub(super) fn current_qos_class() -> c_uint {
        current_qos().0
    }

    /// The full pair, so a test can prove the restore is exact rather than
    /// merely landing back in the same band.
    #[cfg(test)]
    pub(super) fn current_qos_pair() -> (c_uint, c_int) {
        current_qos()
    }

    /// Raises the calling thread to USER_INTERACTIVE for as long as it lives, then
    /// puts the thread back exactly where it was found.
    ///
    /// For a thread the process owns end to end — the PTY reader, the frame ticker
    /// — the plain raise is right and this is unnecessary. It exists for work that
    /// runs on a *borrowed* thread: a keystroke served by the tokio blocking pool
    /// hands its thread back when it is done, and an unrestored bump leaves that
    /// thread in the interactive band for whatever unrelated blocking work lands on
    /// it next. A few keystrokes and the pool the terminal competes against is the
    /// pool the terminal promoted.
    pub(super) struct QosBoost {
        previous: (c_uint, c_int),
    }

    impl QosBoost {
        pub(super) fn user_interactive() -> Self {
            let previous = current_qos();
            raise_self_to_user_interactive();
            Self { previous }
        }
    }

    impl Drop for QosBoost {
        fn drop(&mut self) {
            set_self_qos(self.previous.0, self.previous.1);
        }
    }
}

/// Raise the calling thread's scheduling QoS for the interactive terminal I/O
/// path. macOS-only (see [`thread_qos`]); a no-op on other platforms.
#[cfg(target_os = "macos")]
fn raise_thread_for_interactive_io() {
    thread_qos::raise_self_to_user_interactive();
}

#[cfg(not(target_os = "macos"))]
fn raise_thread_for_interactive_io() {}

/// Raise the calling thread for the duration of the returned guard, then put it
/// back. Use this — never the bare raise — on a thread the caller does not own,
/// such as one borrowed from the tokio blocking pool. See [`thread_qos::QosBoost`].
#[cfg(target_os = "macos")]
#[must_use = "the QoS is restored when the guard drops; dropping it immediately bumps nothing"]
fn interactive_io_boost() -> thread_qos::QosBoost {
    thread_qos::QosBoost::user_interactive()
}

/// The guard the other platforms have nothing to restore into. It exists so the
/// call reads identically everywhere and `#[must_use]` keeps meaning "hold this",
/// rather than degrading to a unit that a caller binds and clippy rejects.
#[cfg(not(target_os = "macos"))]
pub(crate) struct QosBoost;

#[cfg(not(target_os = "macos"))]
#[must_use = "the QoS is restored when the guard drops; dropping it immediately bumps nothing"]
fn interactive_io_boost() -> QosBoost {
    QosBoost
}

// Re-export from chrome module for use by this module and tests.
use crate::chrome::is_chrome_row;
use alacritty_terminal::vte;

use crate::chrome::{is_prompt_line, is_separator_line};

/// How many bottom screen rows to check when verifying a question candidate.
/// Wide enough to cover agent footer layouts (mode line, spinner, Wiz HUD,
/// suggest/intent blocks, trailing disclaimer text) that push the actual
/// question several rows above the prompt box.
const SCREEN_VERIFY_ROWS: usize = 20;

struct TimerIdleTransition {
    transitioned: bool,
    force_cleared_subtasks: bool,
    /// Read only by tests: it separates a transition the screen confirmed from
    /// one the silence timer forced, which `transitioned` alone cannot. The
    /// `cfg_attr` keeps the lint armed in test builds, so the field still goes
    /// dead-code the moment the last assertion on it disappears.
    #[cfg_attr(not(test), allow(dead_code))]
    screen_confirms_idle: bool,
    // Returned provenance is inspected by regression tests; production uses the transition result.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "test-observed transition provenance")
    )]
    evidence: Option<Evidence>,
}

fn try_timer_idle_transition(
    state: &AppState,
    silence: &Arc<Mutex<SilenceState>>,
    session_id: &str,
    screen_activity: AgentScreenActivity,
    agent_type: Option<&str>,
    evidence_turn_epoch: Option<u64>,
) -> TimerIdleTransition {
    let lifecycle = Arc::clone(silence);
    // Read the foreground process group before taking the lifecycle lock: the
    // probe locks the PtySession, which the reader thread holds while it takes
    // this same SilenceState.
    let nested_prompt = agent_type.is_none() && explicit_busy_is_a_nested_prompt(state, session_id);
    let (transitioned, force_cleared_subtasks, screen_confirms_idle, evidence, parent_dispatch) = {
        let mut silence = silence.lock();
        if evidence_turn_epoch.is_some_and(|observed| {
            state
                .session_maps
                .session_states
                .get(session_id)
                .is_some_and(|session| session.turn_epoch != observed)
        }) {
            return TimerIdleTransition {
                transitioned: false,
                force_cleared_subtasks: false,
                screen_confirms_idle: false,
                evidence: None,
            };
        }

        let idle_was_confirmed = silence.idle_confirmed();
        let submission_stale = screen_activity == AgentScreenActivity::Unknown
            && agent_type == Some("claude")
            && silence.unacknowledged_submission_is_stale();
        let protocol_stale =
            screen_activity == AgentScreenActivity::Ready && silence.protocol_busy_is_stale();
        let screen_confirms_idle = match screen_activity {
            AgentScreenActivity::Ready
                if agent_type == Some("ego") && silence.injection_delivery_uncertain =>
            {
                silence.recover_uncertain_injection_on_empty_composer()
            }
            AgentScreenActivity::Ready if protocol_stale => {
                tracing::warn!(
                    session_id = %session_id,
                    activity_source = "protocol-stale",
                    "authoritative busy signal became stale on a stable ready screen"
                );
                silence.force_idle(EvidenceRank::Process, "protocol-stale");
                true
            }
            AgentScreenActivity::Ready => silence.note_ready_screen(),
            AgentScreenActivity::Interrupted => silence.note_interrupted_screen(),
            AgentScreenActivity::Unknown => {
                silence.note_unknown_screen();
                false
            }
            AgentScreenActivity::Working => false,
        };
        let observed_shell = state
            .session_maps
            .shell_states
            .get(session_id)
            .map(|atom| atom.load(std::sync::atomic::Ordering::Acquire));
        // A headless agent spawn never typed a launch line, so no shell edge
        // ever marked it BUSY: the atom is still SHELL_NULL and a TUI that
        // paints no working screen (Gemini) would never reach IDLE. For an
        // agent session a screen-confirmed Ready is the same evidence that
        // releases BUSY, so it releases NULL too. A plain shell
        // (`agent_type == None`) keeps NULL until its own OSC 133 edge.
        let is_busy = observed_shell == Some(SHELL_BUSY)
            || (observed_shell == Some(SHELL_NULL) && agent_type.is_some() && screen_confirms_idle);
        let released_from = observed_shell.unwrap_or(SHELL_NULL);
        if !is_busy && screen_confirms_idle && !idle_was_confirmed {
            tracing::debug!(
                session_id,
                queued_commands = queued_command_count(state, session_id),
                "Ready confirmed after shell became idle"
            );
        }
        if !is_busy || screen_activity == AgentScreenActivity::Working {
            return TimerIdleTransition {
                transitioned: false,
                force_cleared_subtasks: false,
                screen_confirms_idle,
                evidence: None,
            };
        }

        let hold_for_ready_confirmation = matches!(
            screen_activity,
            AgentScreenActivity::Ready | AgentScreenActivity::Interrupted
        ) && !screen_confirms_idle;
        let probe = if screen_confirms_idle {
            foreground_probe(state, session_id, &lifecycle)
        } else {
            ForegroundProbe::Open
        };
        let decision =
            if submission_stale || (screen_confirms_idle && probe != ForegroundProbe::Pending) {
                IdleDecision::yes(evidence_turn_epoch)
            } else if screen_confirms_idle
                || (silence.explicit_busy() && !nested_prompt)
                || hold_for_ready_confirmation
                || silence.is_api_retry_active()
            {
                IdleDecision::NO
            } else {
                should_transition_idle(state, session_id)
            };
        if !decision.should_transition {
            return TimerIdleTransition {
                transitioned: false,
                force_cleared_subtasks: false,
                screen_confirms_idle,
                evidence: None,
            };
        }
        if probe == ForegroundProbe::Quiet {
            // The probe looked at the process table and found nothing left
            // running under the agent. That outranks the ready screen that
            // asked for it, so the turn closes on `activity_source=process`
            // rather than on `agent-ready-screen` (#771-4733) — a reader can
            // now tell a close backed by a live process observation from a
            // `protocol-stale` give-up and from a bare silence timeout.
            //
            // `force_idle`, like the two screen adapters: the `else if` chain
            // above already decided this transition is allowed, so the generic
            // busy-rank gate in `record_idle` must not re-reject it.
            silence.force_idle(EvidenceRank::Process, "process");
        }
        if submission_stale {
            silence.force_idle(EvidenceRank::Process, "submission-stale");
        } else if !screen_confirms_idle {
            // Silence-timeout evidence, forced in regardless of rank: the
            // `else if` chain above (mirroring the old checks exactly, incl.
            // `nested_prompt`) already decided this is allowed, so the generic
            // busy-rank gate in `record_idle` must not re-reject it.
            let source = if agent_type.is_none() {
                "silence-timeout-shell"
            } else {
                "silence-timeout-agent"
            };
            silence.force_idle(EvidenceRank::Silence, source);
        }
        let evidence = match decide(&silence.evidence, true, std::time::Instant::now()) {
            Some(Transition::ToIdle(evidence)) => Some(evidence),
            _ => None,
        };
        let (transitioned, parent_dispatch) = try_shell_transition_locked(
            ShellTransitionRequest {
                state,
                session_id,
                expected: released_from,
                new: SHELL_IDLE,
                notify_parent: true,
                observed_turn_epoch: decision.turn_epoch,
            },
            Some(&mut silence),
            || {},
        );
        (
            transitioned,
            decision.force_cleared_subtasks,
            screen_confirms_idle,
            evidence,
            parent_dispatch,
        )
    };
    if let Some(dispatch) = parent_dispatch {
        dispatch_parent_lifecycle(state, dispatch);
    }
    TimerIdleTransition {
        transitioned,
        force_cleared_subtasks,
        screen_confirms_idle,
        evidence,
    }
}

fn completion_adjusted_screen_activity(
    state: &AppState,
    silence: &Arc<Mutex<SilenceState>>,
    session_id: &str,
    screen_activity: AgentScreenActivity,
) -> AgentScreenActivity {
    if screen_activity != AgentScreenActivity::Working {
        return screen_activity;
    }
    // Claude may declare completion before running a blocking Stop hook, while
    // keeping an active phase marker on screen for minutes. That live marker
    // reopens the same turn in `apply_working_evidence`; only adapters whose
    // completed screen can retain a stale Working row need this downgrade.
    if state
        .session_maps
        .session_states
        .get(session_id)
        .is_some_and(|session| session.agent_type.as_deref() == Some("claude"))
    {
        return AgentScreenActivity::Working;
    }
    let silence = silence.lock();
    if silence
        .evidence
        .busy
        .is_some_and(|busy| busy.rank > EvidenceRank::Screen)
    {
        return AgentScreenActivity::Working;
    }
    if state
        .session_maps
        .session_states
        .get(session_id)
        .is_some_and(|session| silence.completion_declared_for_epoch(session.turn_epoch))
    {
        AgentScreenActivity::Ready
    } else {
        screen_activity
    }
}

/// If the silence timer's tool-error candidate has genuinely fired (turn-ending,
/// not recovered — see `SilenceState::check_tool_error`), flag the currently-open
/// turn-level block (the fallback-tier red-tick signal, for non-hook-instrumented
/// agents) and emit the `ToolError` event.
/// Only fires post-`check_tool_error()`, not at the raw `mark_tool_error_candidate`
/// call, so a *recovered* error (the agent retries and the turn ends normally)
/// never flags. Extracted from `spawn_silence_timer`'s loop body for testability.
fn fire_tool_error_if_ready(silence: &Mutex<SilenceState>, session_id: &str, state: &AppState) {
    let Some(text) = silence.lock().check_tool_error() else {
        return;
    };
    state
        .session_maps
        .turn_error_flags
        .insert(session_id.to_string(), ());
    let parsed = ParsedEvent::ToolError { matched_text: text };
    if let Ok(json) = serde_json::to_value(&parsed) {
        #[cfg(feature = "desktop")]
        if let Some(app) = state.app_handle.read().as_ref() {
            let _ = app.emit(&format!("pty-parsed-{session_id}"), &json);
        }
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: session_id.to_string(),
            parsed: json.into(),
        });
    }
}

/// Spawn the silence-detection timer thread. Shared by desktop and headless readers.
///
/// Two strategies run in priority order:
/// 1. **Screen-based**: read the terminal screen, find the last chat line above the
///    prompt box (delimited by two separator lines), check if it ends with `?`.
/// 2. **Chunk-based fallback**: use `check_silence()` with `pending_question_line`
///    for agents that don't have a prompt box (plain shell, etc.).
fn spawn_silence_timer(
    silence: Arc<Mutex<SilenceState>>,
    running: Arc<AtomicBool>,
    session_id: String,
    state: Arc<AppState>,
) {
    tokio::spawn(async move {
        // Track the inter-tick gap in WALL-CLOCK time, not `Instant`.
        // `should_transition_idle` measures idle elapsed against the wall clock
        // (`last_output_ms` is epoch millis), so sleep detection MUST use the
        // same clock. On macOS, `Instant` (mach_absolute_time) does not advance
        // while the system is asleep — an Instant-based gap stays ~1s across a
        // lid-close sleep and never detects the wake, letting the wall-clock
        // jump fire a false busy→idle (completion sound) on every terminal.
        let mut last_tick_ms = now_epoch_ms();
        while running.load(Ordering::Relaxed) {
            tokio::time::sleep(SILENCE_CHECK_INTERVAL).await;
            if !running.load(Ordering::Relaxed) {
                break;
            }

            // Sleep-wake detection: if the wall-clock gap between consecutive
            // ticks is much larger than SILENCE_CHECK_INTERVAL, the system was
            // asleep (lid closed) or the clock stepped. Reset timestamps so
            // stale elapsed times don't trigger false idle transitions /
            // completion sounds.
            let epoch_now = now_epoch_ms();
            let tick_gap = std::time::Duration::from_millis(epoch_now.saturating_sub(last_tick_ms));
            last_tick_ms = epoch_now;
            if tick_gap >= SLEEP_WAKE_GAP {
                tracing::info!(
                    source = "silence_timer",
                    session_id = %session_id,
                    gap_secs = tick_gap.as_secs(),
                    "Sleep-wake detected — resetting timestamps"
                );
                if let Some(ts) = state.session_maps.last_output_ms.get(&session_id) {
                    ts.store(epoch_now, std::sync::atomic::Ordering::Release);
                }
                {
                    let mut sl = silence.lock();
                    let now = std::time::Instant::now();
                    sl.last_output_at = now;
                    sl.last_chunk_at = now;
                    if sl.injection_delivery_uncertain {
                        // Resetting clocks is not a new composer repaint.
                        sl.injection_uncertain_since = Some(now);
                        sl.screen_ready_pending_since = None;
                    }
                }
                continue;
            }

            if orchestrator_recipient_for_pty(&state, &session_id)
                .and_then(|recipient| state.orchestrator_wake_needed_through(&recipient))
                .is_some()
            {
                silence.lock().expire_orchestrator_notice_uncertainty();
            }

            // Discovery must not depend on a frontend polling the desktop IPC.
            // Headless/manual launches need the same parser and mail eligibility.
            refresh_session_agent(&state, &session_id);

            // Reconcile high-confidence screen evidence before the silence
            // fallback. Working here means Codex's presence-based status line
            // (the only screen classifier that returns Working, #446-596f); it
            // runs regardless of current state so it repairs an already-false-
            // idle session instead of merely keeping a pre-existing BUSY alive.
            // Claude/Gemini BUSY is movement-driven in the reader.
            let idle_evidence_turn_epoch = state
                .session_maps
                .session_states
                .get(&session_id)
                .map(|session| session.turn_epoch);
            let agent_type = state
                .session_maps
                .session_states
                .get(&session_id)
                .and_then(|s| s.agent_type.clone());
            // Reused from the reader chunk path (#744-138c) instead of a fresh
            // `detect_agent_screen_activity` call: with no chunk having
            // arrived since the last classification, the screen the function
            // would see is byte-identical, so the cached verdict is the same
            // answer, not a stale one. Keeps the classifier to at most one
            // call per session per `SILENCE_CHECK_INTERVAL` (previously two:
            // one here, one in the reader).
            let screen_activity = silence.lock().cached_screen_activity;
            let screen_activity =
                completion_adjusted_screen_activity(&state, &silence, &session_id, screen_activity);
            let tracked_background_work = state
                .session_maps
                .session_states
                .get(&session_id)
                .is_some_and(|session| session.background_work);
            let shell_is_busy = state
                .session_maps
                .shell_states
                .get(&session_id)
                .is_some_and(|shell| shell.load(Ordering::Acquire) == SHELL_BUSY);
            if tracked_background_work
                || (shell_is_busy
                    && matches!(
                        screen_activity,
                        AgentScreenActivity::Ready | AgentScreenActivity::Interrupted
                    ))
            {
                refresh_background_work(&state, &session_id);
            }
            if screen_activity == AgentScreenActivity::Working {
                apply_working_evidence(&state, &silence, &session_id, epoch_now, "working-screen");
            } else {
                // Evidence mutation, silence decision, and BUSY→IDLE CAS share
                // one lifecycle transaction. A new submitted epoch therefore
                // wins before any stale Ready/Interrupted/Unknown evidence can
                // alter its SilenceState.
                let transition = try_timer_idle_transition(
                    &state,
                    &silence,
                    &session_id,
                    screen_activity,
                    agent_type.as_deref(),
                    idle_evidence_turn_epoch,
                );
                if transition.transitioned {
                    if transition.force_cleared_subtasks {
                        emit_active_subtasks(&state, &session_id, 0, "");
                    }
                    if let Some(vt) = state.grid.vt_log_buffers.get(&session_id) {
                        vt.lock().process(b"\x1b[?25h");
                    }
                    emit_shell_state(&state, &session_id, "idle");
                    reevaluate_orchestrator_mail_wake(&state, &session_id);
                    flush_pending_injections(&state, &session_id);
                } else if transition.screen_confirms_idle
                    && !shell_is_busy
                    && queued_command_count(&state, &session_id) > 0
                {
                    // Silence can mark the shell idle before the Ready screen
                    // stabilizes. That later confirmation has no second shell
                    // edge, so it must retry the existing self-guarded flush.
                    flush_pending_injections(&state, &session_id);
                }
            }

            // Update startup grace state (checks if output has settled).
            {
                let mut sl = silence.lock();
                sl.check_startup_settle();
                if sl.is_startup_grace() {
                    continue; // Still in startup burst — suppress question detection
                }
            }

            // Tool-error turn-end: `Error: Exit code N` + silence = fire playError.
            // Checked before question detection — a tool error is not a question.
            fire_tool_error_if_ready(&silence, &session_id, &state);

            // Suggest turn-end: drain parked `suggest:` items once the shell
            // has transitioned to IDLE. The reader parks them at parse time
            // (see write_pty's emit loop); gating the drain on shell_state ==
            // IDLE makes the frontend's `pendingSuggest` race impossible —
            // the event physically cannot reach the UI before idle.
            emit_pending_suggest_if_idle(&state, &silence, &session_id);
            emit_open_intent_if_idle(&state, &silence, &session_id);

            // Retraction is a reconciliation loop, not part of the one-shot
            // question-emission gate. Once a low-confidence wait has fired,
            // `question_already_emitted` is true by design; gating this check on
            // `is_silent()` made the documented backstop unreachable forever.
            let quiet_for_retraction = silence.lock().is_quiet_for_question_retraction();
            if quiet_for_retraction {
                let active_question =
                    state
                        .session_maps
                        .session_states
                        .get(&session_id)
                        .and_then(|s| {
                            (s.awaiting_input && !s.question_confident && s.choice_prompt.is_none())
                                .then(|| s.question_text.clone())
                                .flatten()
                        });
                if let Some(active_question) = active_question {
                    let still_current =
                        state
                            .grid
                            .vt_log_buffers
                            .get(&session_id)
                            .is_some_and(|vt| {
                                match current_chat_question(&vt.lock().screen_rows()) {
                                    CurrentChatQuestion::PromptAnchored(Some(current)) => {
                                        current.trim() == active_question.trim()
                                    }
                                    CurrentChatQuestion::PromptAnchored(None) => false,
                                    CurrentChatQuestion::NoPromptAnchor => false,
                                }
                            });
                    if !still_current {
                        emit_question_cleared_if_stale(&state, &session_id);
                    }
                }
            }

            // Chat-question heuristics, like open-dialog recovery in the reader,
            // require a live agent. Its old title can remain above the returned
            // shell prompt and must not become a new wait after ten quiet seconds.
            if agent_type.is_none() {
                silence.lock().clear_stale_question();
                emit_question_cleared_if_stale(&state, &session_id);
                continue;
            }

            // Check temporal conditions first (shared by both strategies).
            // Snapshot the epoch while holding the lifecycle mutex shared with
            // `note_submitted_input`. If input begins after this point, the
            // accumulator rejects the old-epoch Question; if it began before,
            // `suppress_user_input` makes `is_silent` false.
            let (is_silent, question_turn_epoch) = {
                let sl = silence.lock();
                let epoch = state
                    .session_maps
                    .session_states
                    .get(&session_id)
                    .map(|session| session.turn_epoch)
                    .unwrap_or(0);
                (sl.is_silent(), epoch)
            };
            if !is_silent {
                continue;
            }

            // Strategy 1: screen-based — walk upward from the prompt box looking
            // for the most recent plausible question within a bounded window.
            // This is robust to trailing non-question text between the question
            // and the prompt box (e.g. "(stopping here — waiting for your answer)").
            let current_question = state.grid.vt_log_buffers.get(&session_id).map(|vt| {
                let rows = vt.lock().screen_rows();
                let question = current_chat_question(&rows);
                tracing::trace!(
                    session_id = %session_id,
                    found = matches!(&question, CurrentChatQuestion::PromptAnchored(Some(_))),
                    "DIAG silence_timer: screen strategy"
                );
                question
            });

            // Strategy 2: chunk-based fallback — pending_question_line + screen verify.
            let prompt_text = match current_question {
                Some(CurrentChatQuestion::PromptAnchored(Some(line))) => line,
                // A current prompt exists and later non-question content is above
                // the historical candidate. That is decisive turn-order evidence:
                // never let the fallback dig through it to resurrect an old `?`.
                Some(CurrentChatQuestion::PromptAnchored(None)) => {
                    silence.lock().clear_stale_question();
                    emit_question_cleared_if_stale(&state, &session_id);
                    continue;
                }
                Some(CurrentChatQuestion::NoPromptAnchor) | None => {
                    let question = silence.lock().check_silence();
                    match question {
                        Some(ref text) => {
                            let on_screen = state
                                .grid
                                .vt_log_buffers
                                .get(&session_id)
                                .map(|vt| {
                                    verify_question_on_screen(
                                        &vt.lock().screen_rows(),
                                        text,
                                        SCREEN_VERIFY_ROWS,
                                    )
                                })
                                .unwrap_or(false);
                            tracing::debug!(
                                session_id = %session_id,
                                question = %text,
                                on_screen = on_screen,
                                "silence_timer: chunk fallback"
                            );
                            if !on_screen {
                                silence.lock().clear_stale_question();
                                emit_question_cleared_if_stale(&state, &session_id);
                                continue;
                            }
                            text.clone()
                        }
                        None => {
                            tracing::trace!(
                                session_id = %session_id,
                                "silence_timer: silent but no question candidate"
                            );
                            emit_question_cleared_if_stale(&state, &session_id);
                            continue;
                        }
                    }
                }
            };

            // Suppress heuristics only after a hook marker was observed at
            // runtime. A persisted config flag alone can be stale after a failed
            // install or an agent-version change.
            let hook_configured = state
                .session_maps
                .session_states
                .get(&session_id)
                .map(|s| s.hook_instrumented)
                .unwrap_or(false);
            if hook_configured && silence.lock().hook_state_seen {
                silence.lock().clear_stale_question();
                continue;
            }

            // Emit question event.
            silence.lock().mark_emitted(&prompt_text);
            let parsed = ParsedEvent::Question {
                prompt_text: prompt_text.clone(),
                confident: false,
            };
            if let Ok(mut json) = serde_json::to_value(&parsed) {
                if let Some(object) = json.as_object_mut() {
                    object.insert("_turn_epoch".to_string(), question_turn_epoch.into());
                }
                #[cfg(feature = "desktop")]
                if let Some(app) = state.app_handle.read().as_ref() {
                    let _ = app.emit(&format!("pty-parsed-{session_id}"), &json);
                }
                state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                    session_id: session_id.clone(),
                    parsed: json.into(),
                });
            }
        }
    });
}

// ---------------------------------------------------------------------------
// ChunkProcessor: shared output processing logic for desktop & headless readers
// ---------------------------------------------------------------------------

/// Extract a clean prompt from grok's "⚠ Action Required" OSC 0 title.
/// Strips the leading warning / "Action Required" marker, the spinner braille
/// frame, and separators, leaving the human-readable action description.
/// `"⚠ Action Required - ⠙ - Running: echo x - Execute Shell …"` → `"Running: echo x - Execute Shell …"`.
fn clean_action_required_title(title: &str) -> String {
    let after = title.split("Action Required").nth(1).unwrap_or(title);
    let cleaned = after
        .trim_start_matches(|c: char| {
            c == '-' || c == ' ' || ('\u{2800}'..='\u{28FF}').contains(&c)
        })
        .trim();
    if cleaned.is_empty() {
        "grok is awaiting approval".to_string()
    } else {
        cleaned.to_string()
    }
}

/// Whether a `Notification` hook's `notification_type` describes a fire that
/// genuinely needs a response, per Claude Code's own closed set of values —
/// far more reliable than sniffing the free-text `message` wording the way
/// `output_parser.rs::parse_osc777_notifies` has to for the (unconfirmed-live)
/// native OSC 777 path. See `agent-signal-architecture.html`'s "OSC 777 vs OSC
/// 7770" section for the full background and the 2026-08-29/2026-09-02
/// incident this fixes.
enum NotificationOutcome {
    /// A genuine block — a confident, sticky `Question`.
    Blocking,
    /// Purely informational — never a question at all (e.g. a background
    /// session finishing, auth succeeding). Distinct from "not confident":
    /// this never even flashes the badge, rather than flashing and
    /// self-clearing.
    Informational,
    /// A value outside Claude Code's documented set as of this writing — a
    /// future addition this binary predates. Deliberately NOT folded into
    /// `Blocking` or `Informational`: guessing either one risks either
    /// silently swallowing a real future block, or reproducing this exact
    /// incident under a new type name. The caller falls back to the
    /// `message` wording heuristic instead of guessing here.
    Unknown,
}

/// Classify one `notification_type` value. `idle_prompt` is handled by the
/// caller, not here — its correct outcome depends on whether the session is
/// already idle (see `tuic_state_awaiting_event`'s doc comment), which this
/// function has no access to.
fn classify_notification_type(notification_type: &str) -> NotificationOutcome {
    match notification_type {
        // A tool/network permission prompt, an MCP elicitation form or URL
        // dialog, a multi-agent teammate question, or Claude waiting on a
        // stale quota resume — all genuinely need a response.
        "permission_prompt"
        | "elicitation_dialog"
        | "elicitation_url_dialog"
        | "agent_needs_input"
        | "quota_auto_resume_stale" => NotificationOutcome::Blocking,
        // Background-session/auth/elicitation lifecycle noise and quota
        // auto-resume outcomes — none of these are the agent asking the user
        // anything.
        "auth_success"
        | "elicitation_complete"
        | "elicitation_response"
        | "agent_completed"
        | "quota_auto_resume_fired"
        | "quota_auto_resume_disabled" => NotificationOutcome::Informational,
        _ => NotificationOutcome::Unknown,
    }
}

/// The wording rule `output_parser.rs::parse_osc777_notifies` already uses
/// for its own ambiguous `message` text — the fallback for a `Notification`
/// fire this binary can't classify by `notification_type` alone (an older
/// Claude Code build that predates the field, or a value outside its
/// documented set).
fn message_wording_confidence(notify_message: Option<&str>) -> bool {
    notify_message.is_some_and(crate::output_parser::is_confident_permission_wording)
}

/// Whether a `Notification`-sourced `state=awaiting` should badge at all, and
/// if so how confidently. `None` means "no `notify=`/`notifytype=` verb
/// preceded this fire" — i.e. it came from `PreToolUse(AskUserQuestion|
/// ExitPlanMode)` or `Elicitation`, neither of which scrapes either (see
/// `tuic-hook`'s `DERIVATIONS` table) — always a genuine block, so this always
/// returns `Some(true)` in that case.
///
/// `shell_already_idle` resolves the one `notification_type` that can't be
/// classified by itself: `idle_prompt` fires both for Claude's own ~60s
/// heartbeat after a turn that already ended (`Stop` already fired, shell
/// already idle — nothing pending, any future need for input arrives through
/// its own signal the next time the user acts) and, per Claude Code's own
/// docs, whenever the session has gone 60s without a keystroke — which can
/// also mean it's genuinely still stuck mid-turn on an un-hooked plan/skill
/// picker (shell still busy, no `Stop` yet). Only the first case is safe to
/// drop outright; the second is the one signal that gap has at all, so it
/// still surfaces, just not confidently.
fn notification_awaiting_outcome(
    notify_message: Option<&str>,
    notification_type: Option<&str>,
    shell_already_idle: bool,
) -> Option<bool> {
    if notify_message.is_none() && notification_type.is_none() {
        return Some(true);
    }
    match notification_type {
        Some("idle_prompt") => {
            if shell_already_idle {
                None
            } else {
                Some(false)
            }
        }
        Some(other) => match classify_notification_type(other) {
            NotificationOutcome::Blocking => Some(true),
            NotificationOutcome::Informational => None,
            NotificationOutcome::Unknown => Some(message_wording_confidence(notify_message)),
        },
        // notification_type absent entirely (an older Claude Code build that
        // predates the field) but a message was scraped.
        None => Some(message_wording_confidence(notify_message)),
    }
}

/// Map a TUIC `state=` verb to the awaiting-input `ParsedEvent` it implies.
///
/// busy/idle shell transitions are handled by `handle_tuic_state`; this covers
/// only the separate `awaiting_input` field, which is driven by Question /
/// UserInput events in `state.rs`:
/// - `awaiting` → `Question` per `notification_awaiting_outcome` above: a
///   genuine block stays a confident, sticky question; Claude's own idle-timer
///   heartbeat after a turn that already ended is dropped outright; the same
///   heartbeat firing mid-turn (no `Stop` yet) surfaces non-confidently, so
///   the silence-timer backstop (`emit_question_cleared_if_stale`) can retract
///   it once the screen goes quiet with nothing really pending.
/// - `busy`     → `UserInput` clear (hook busy is authoritative — clears an awaiting
///   set by a prior `PreToolUse(AskUserQuestion)`; empty content never overwrites
///   `last_prompt`). Fires on every tool call too, so it carries no prompt row
///   (`line = -1`).
/// - `prompt`   → same clear, sent only by the user-prompt-submit hook, and the only
///   one that carries the prompt row for the scrollbar marker
/// - anything else (incl. `idle`, unknown) → `None`
fn tuic_state_awaiting_event(
    payload: &str,
    line: i64,
    notify_message: Option<&str>,
    notification_type: Option<&str>,
    shell_already_idle: bool,
) -> Option<ParsedEvent> {
    match payload {
        "awaiting" => {
            let confident = notification_awaiting_outcome(
                notify_message,
                notification_type,
                shell_already_idle,
            )?;
            Some(ParsedEvent::Question {
                prompt_text: String::new(),
                confident,
            })
        }
        "busy" => Some(ParsedEvent::UserInput {
            content: String::new(),
            line: -1,
        }),
        // `line` is the absolute prompt row (history_size + cursor row) at the
        // submit — the row the user's prompt sits on. Carried so the frontend can
        // mark user-prompt lines on the scrollbar.
        "prompt" => Some(ParsedEvent::UserInput {
            content: String::new(),
            line,
        }),
        _ => None,
    }
}

/// Inverse of `tuic-hook`'s `payload::encode` (percent-encoding over the RFC
/// 3986 unreserved set). Any `%XX` escape that isn't valid hex, or that would
/// run past the end of the string, is left as a literal `%` rather than
/// dropped or erroring — a hook must never be able to desync this parser, so
/// a malformed escape degrades to "pass the bytes through," not a panic.
/// Invalid UTF-8 after decoding degrades the same way, via lossy replacement.
fn percent_decode_osc_payload(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 3 <= bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Whether `agent_type`'s config enables native-hook instrumentation. Resolved
/// once when the session's agent type becomes known (config changes apply on the
/// next agent launch, matching when the hooks themselves take effect).
pub(crate) fn hook_instrumented_for(
    agents: &crate::config::AgentsConfig,
    agent_type: Option<&str>,
) -> bool {
    let Some(agent_type) = agent_type else {
        return false;
    };
    // ego emits OSC 7770 natively: there is no hook to install or disable.
    if agent_type == "ego" {
        return true;
    }
    let settings = agents.agents.get(agent_type);
    if matches!(agent_type, "claude" | "codex") {
        settings
            .and_then(|s| s.native_status_signals)
            .unwrap_or(true)
    } else {
        settings
            .and_then(|s| s.hook_instrumentation)
            .unwrap_or(false)
    }
}

/// Events carried by the RAW byte stream, before any VT rendering — sequences
/// the vt100/alacritty parsers consume and that are therefore invisible in the
/// clean rows every other parser reads.
///
/// Deliberately separate from the clean-row parsers: everything appended here
/// skips `suppress_heuristic_question`. That filter exists to stop regex
/// *guesses* from double-firing against the hook's `state=awaiting`; the OSC 777
/// parser accepts only unambiguous permission or approval wording. Claude's
/// generic idle notification is not a question. Plan and skill Ink pickers
/// without a hook state use the visible footer's presence recovery instead.
///
/// Shared with the fixture harness (`awaiting_signal_fixtures`) so a test can
/// never assert against a composition that production does not run.
fn raw_stream_events(carry: &mut String, data: &str, out: &mut Vec<ParsedEvent>) {
    let combined = if carry.is_empty() {
        std::borrow::Cow::Borrowed(data)
    } else {
        let mut joined = std::mem::take(carry);
        joined.push_str(data);
        std::borrow::Cow::Owned(joined)
    };
    if let Some(evt) = crate::output_parser::parse_osc94(&combined) {
        out.push(evt);
    }
    out.extend(crate::output_parser::parse_osc777_notifies(&combined));
    *carry = unterminated_osc_tail(&combined);
}

/// Longest suffix of a chunk that opens an OSC sequence but never closes it.
///
/// Only an *unterminated* tail is carried, so a sequence can be matched once and
/// only once: a complete one leaves nothing behind. A tail longer than
/// [`MAX_RAW_CARRY`] is dropped rather than grown without bound — at that length
/// it is not a notification, it is a payload we do not parse (or a stream that
/// never terminates it), and holding it would pin memory for the session.
fn unterminated_osc_tail(data: &str) -> String {
    // Anchored on the last ESC, not on the last `ESC]`: a read can end on the
    // ESC itself, with the `]` arriving in the next chunk. Anchoring on the
    // pair dropped that ESC and left the next chunk starting at `]777;…`,
    // which is no longer an escape sequence at all.
    let Some(start) = data.rfind('\x1b') else {
        return String::new();
    };
    let tail = &data[start..];
    // A lone trailing ESC may still become an OSC introducer.
    if tail == "\x1b" {
        return tail.to_string();
    }
    // Anything else that is not an OSC introducer (CSI, ST, charset select) is
    // consumed by the VT parser, not by us.
    if !tail.starts_with("\x1b]") {
        return String::new();
    }
    // BEL, or ST (ESC backslash) — the ESC of an ST is not the introducer's own.
    if tail.contains('\x07') || tail[1..].contains("\x1b\\") {
        return String::new();
    }
    if tail.len() > MAX_RAW_CARRY {
        return String::new();
    }
    tail.to_string()
}

/// Cap for [`unterminated_osc_tail`]. Comfortably above any OSC we parse: the
/// longest observed notify body is under 60 bytes.
const MAX_RAW_CARRY: usize = 512;

/// Record an `intent:` marker as a Progress journal entry.
///
/// Silent on every skip. Three of them are ordinary and none is the user's
/// problem: collection is off, the session is not inside a registered project,
/// or the text does not survive validation. A missing project is deliberately
/// NOT resolved to the focused UI repository — that files one agent's work
/// under whatever the human happened to be looking at.
///
/// The write is synchronous. This runs on the PTY reader thread, which is not
/// the async executor, and an intent arrives a few times a minute against a
/// sub-millisecond WAL insert.
fn record_intent_in_journal(state: &AppState, session_id: &str, text: &str) -> bool {
    let (agent_type, agent_name) = crate::progress::session_identity(state, session_id);
    if !crate::progress::progress_tracking_enabled(state, agent_type.as_deref()) {
        return true;
    }
    let Some(project) = crate::progress::project_for_session(state, session_id) else {
        return true;
    };
    match crate::progress::record_intent(
        state,
        Some(&project),
        text,
        agent_name,
        agent_type.as_deref(),
        Some(session_id),
    ) {
        Ok(entry) => {
            crate::mcp_http::mcp_transport::emit_progress_entry(state, entry);
            true
        }
        Err(error) => {
            tracing::warn!(
            source = "progress",
            session_id = %session_id,
            error = %error,
            "intent: not recorded in the Progress journal"
            );
            false
        }
    }
}

fn publish_intent_event(state: &AppState, session_id: &str, event: &ParsedEvent, turn_epoch: u64) {
    let ParsedEvent::Intent { text, .. } = event else {
        return;
    };
    if !record_intent_in_journal(state, session_id, text) {
        return;
    }
    state.note_marker(session_id, crate::state::MarkerKind::Intent);
    if let Ok(mut json) = serde_json::to_value(event) {
        if let Some(object) = json.as_object_mut() {
            object.insert("_turn_epoch".to_string(), turn_epoch.into());
        }
        #[cfg(feature = "desktop")]
        if let Some(app) = state.app_handle.read().as_ref() {
            let _ = app.emit(&format!("pty-parsed-{session_id}"), &json);
        }
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: session_id.to_string(),
            parsed: json.into(),
        });
    }
}

/// Whether a heuristic `Question` event should be suppressed for this session.
/// Hook-instrumented agents report awaiting via OSC 7770 (`state=awaiting`), so
/// the silence/regex question heuristics would only double-fire. Only `Question`
/// is suppressed — idle/busy transitions and every other event pass through.
fn suppress_heuristic_question(hook_instrumented: bool, event: &ParsedEvent) -> bool {
    hook_instrumented && matches!(event, ParsedEvent::Question { .. })
}

/// Restore the awaiting badge while an Ink dialog is still open on screen.
///
/// The badge is `SessionState.awaiting_input`, driven by events; the dialog is a
/// screen condition that outlives them. A multi-question `AskUserQuestion` is the
/// case where the two part ways: answering sub-question 1 clears the badge, and
/// sub-question 2 repaints its title and options but NOT the footer row — the one
/// row `parse_clean_lines` needs to see change in order to fire again. The result
/// is a tab reading "working" while the agent waits.
///
/// Presence of the footer is the entire signal. Nothing structural is read: title,
/// option list and the `⊠ … ✓ Submit` tab bar all move as the wizard advances,
/// while the footer is byte-identical throughout — useless as a change signal,
/// exact as a presence one.
///
/// Returns an event only when the badge is actually off, so this is one event per
/// spurious clear, never one per repaint. A live `choice_prompt` owns the awaiting
/// state through its own resolve path and is left alone.
///
/// `question_this_tick` is the same rule read one step earlier. `awaiting_input`
/// comes from `SessionState`, which this tick's events have not reached yet, so on
/// the FIRST sub-question — the footer row genuinely changed, `parse_clean_lines`
/// parsed the real question, badge still off — both fire. Two `Question` events
/// land, and the accumulator keeps the LAST `prompt_text`: the tab then shows
/// `⊠ … ✓ Submit` where the question should be. Not exotic, this is every
/// non-hook `AskUserQuestion`'s opening frame.
fn rearm_awaiting_for_open_dialog(
    screen: &[String],
    awaiting_input: bool,
    has_choice_prompt: bool,
    question_this_tick: bool,
) -> Option<ParsedEvent> {
    if awaiting_input || has_choice_prompt || question_this_tick {
        return None;
    }
    crate::output_parser::ink_dialog_footer(screen).map(|footer| ParsedEvent::Question {
        prompt_text: footer.to_string(),
        confident: true,
    })
}

/// Only the first, default-No workspace trust picker of a managed Claude child.
/// Require the question and both choices on the rendered screen so ordinary
/// permission prompts, chat text and a manually changed selection stay intact.
fn managed_claude_trust_dialog(screen: &[String]) -> bool {
    let text = screen.join(" ");
    if !text.contains("Quick safety check:")
        || !text.contains("Is this a project you created or one you trust?")
    {
        return false;
    }
    let yes = screen
        .iter()
        .position(|row| row.trim_start().starts_with("Yes,"));
    let selected_no = screen.iter().position(|row| {
        let row = row.trim_start();
        let choice = row
            .strip_prefix('❯')
            .or_else(|| row.strip_prefix('›'))
            .or_else(|| row.strip_prefix('>'));
        choice.is_some_and(|choice| choice.trim_start().contains("No, exit"))
    });
    matches!((yes, selected_no), (Some(yes), Some(no)) if no < yes)
}

#[cfg(test)]
mod managed_claude_trust_tests {
    use super::managed_claude_trust_dialog;

    #[test]
    fn leaves_other_questions_and_changed_selections_untouched() {
        for rows in [
            vec!["Allow this command?", "  Yes, allow", "❯ No, exit"],
            vec![
                "Quick safety check: Is this a project you created or one you trust?",
                "❯ Yes, I trust this folder",
                "  No, exit",
            ],
            vec![
                "Quick safety check: Is this a project you created or one you trust?",
                "❯ No, exit",
            ],
        ] {
            assert!(
                !managed_claude_trust_dialog(
                    &rows.into_iter().map(str::to_string).collect::<Vec<_>>()
                ),
                "must not answer a different or manually changed choice"
            );
        }
    }
}

fn accept_managed_claude_trust_dialog(state: &AppState, session_id: &str) -> Result<(), String> {
    let writer = state
        .pty_writer(session_id)
        .ok_or_else(|| "Session not found".to_string())?;
    let mut writer = writer.lock();
    state.retire_turn_interrupt(session_id);
    writer
        .write_all(b"\x1b[B")
        .and_then(|()| writer.flush())
        .map_err(|error| format!("Trust selection failed: {error}"))?;
    std::thread::sleep(INJECT_ENTER_GAP);
    writer
        .write_all(b"\r")
        .and_then(|()| writer.flush())
        .map_err(|error| format!("Trust confirmation failed: {error}"))
}

/// Per-session mutable state for processing PTY output chunks.
/// Holds dedup state, parser, and session CWD for PlanFile resolution.
/// Used by `spawn_reader_thread`.
struct ChunkProcessor {
    /// An explicit idle marker asks the reader to flush after publishing this chunk.
    queued_idle_flush: bool,
    parser: OutputParser,
    intent_break_parser: vte::Parser,
    /// Dedup: only emit StatusLine when task_name actually changes *within a
    /// turn*, stored as `(turn_epoch, task_name)`. The epoch is part of the key
    /// because agents may name every turn identically — Codex always reports
    /// "Working" — and a session-lifetime dedup would then swallow the status
    /// line of every turn after the first. The suppressed event is the only
    /// thing that clears the previous turn's `suggested_actions`, which
    /// `session_state_with_shell` treats as a completion marker, so a working
    /// agent would stay reported as completed/idle for the rest of the session.
    last_status_task: Option<(u64, String)>,
    /// Dedup: don't re-emit the same question prompt_text
    last_question_text: Option<String>,
    /// Tail of the previous chunk holding an OSC sequence the read split in
    /// half. A PTY read boundary falls wherever the kernel decides, so a chunk
    /// can end mid-escape; the raw-stream parsers match on complete sequences
    /// only (correctly — a truncated one must never match, or its fields would
    /// run on into unrelated later output), so without this the signal is simply
    /// lost. Observed: an Ink repaint split `ESC]777;notify;…BEL` and the
    /// awaiting badge never lit. Bounded by [`MAX_RAW_CARRY`].
    raw_carry: String,
    /// Dedup: last emitted ChoicePrompt signature (title + option keys).
    /// Prevents re-emit on repaint while the dialog stays on screen.
    last_choice_prompt_sig: Option<String>,
    /// Whether a published slash menu still needs an explicit retraction.
    slash_menu_visible: bool,
    /// A busy hook cleared the choice; wait for a dialog-row repaint before
    /// admitting the same signature again, so stale screen cells cannot re-arm it.
    choice_prompt_needs_repaint: bool,
    /// Session CWD for resolving relative plan-file paths
    session_cwd: Option<String>,
    /// Plan files awaiting creation on disk (agent announces before writing).
    /// Tuples of (absolute_path, deadline). Checked each chunk until file appears
    /// or 10s deadline expires. Already-emitted paths tracked for dedup.
    pending_planfiles: Vec<(String, std::time::Instant)>,
    /// Plan file paths already emitted — prevents re-emitting on spinner redraws.
    emitted_planfiles: std::collections::HashSet<String>,
    /// Plan file paths that exhausted their retry window without appearing on
    /// disk. Tombstoned so a still-on-screen reference (re-parsed every chunk)
    /// is not re-queued forever — that was a source of endless retry-log spam.
    gaveup_planfiles: std::collections::HashSet<String>,
    /// Tracks whether the terminal is in alternate screen buffer mode.
    /// Set on ESC[?1049h, cleared on ESC[?1049l.
    pub(crate) in_alt_buffer: bool,
    /// Mirrors `vt.is_alternate_screen()` (the real parsed `Term::mode()`
    /// state, backed by `swap_alt()`) — NOT the same signal as
    /// `in_alt_buffer` above, which scans raw bytes for `1049h`/`1049l` and
    /// can theoretically desync (a split escape sequence, a `1047`/`47`
    /// variant). Used only to edge-detect a real alt-screen swap so
    /// `pending_scroll` can be cleared on the transition — see that field's
    /// doc comment and the call site in `process_chunk`.
    last_grid_alt_screen: bool,
    /// Structured terminal mode with nesting depth and app detection.
    terminal_mode: crate::ai_agent::tui_detect::TerminalMode,
    /// Last VtLogBuffer total_lines observed — distinguishes a chunk that
    /// scrolled in new output from one that only repainted existing rows.
    last_vt_log_total: usize,
    /// Report a TUIC-managed agent entering alternate screen only once per PTY.
    alt_screen_warned: bool,
    /// The startup toast fires only for an alternate-screen entry before this instant.
    startup_deadline: std::time::Instant,
    /// The startup toast is shown once per PTY.
    alt_screen_toasted: bool,
    /// TUIC_SESSION UUID for this PTY — used to create flag files that
    /// signal the shell wrapper to stop injecting `--session-id`.
    tuic_session: Option<String>,
    /// Last time we created a no-session-inject flag file in response to
    /// an `AgentSessionConflict` event. Gates subsequent marks so a single
    /// burst of conflict output fires the mitigation exactly once.
    last_session_conflict_mark: Option<std::time::Instant>,
    /// Absolute buffer line of the last heuristic agent-block start.
    /// Used to emit AgentBlock end when the next block starts or agent exits.
    last_agent_block_line: Option<usize>,
    /// Absolute buffer line of the last block start synthesized from a
    /// Claude Code fullscreen transcript-mode `[` dump (see
    /// `synthesize_transcript_dump_block_events`). Independent of
    /// `last_agent_block_line` — the dump is a one-shot snapshot printed to
    /// the primary screen, structurally unrelated to either live block
    /// source, so it needs its own open/close bookkeeping.
    last_dump_block_line: Option<usize>,
    /// Set whenever a chunk is processed with the alternate screen active;
    /// consumed (and reset false) the next time a primary-screen chunk
    /// synthesizes a fresh dump `start` event. Since `[` is only reachable
    /// from transcript mode (the alternate screen), a `true` reading there
    /// means real primary-screen dump activity resumed after a visit back to
    /// the agent — i.e. a genuinely new `[`-dump, not a continuation of the
    /// one already in progress. Drives `new_dump_generation` — see
    /// `synthesize_transcript_dump_block_events`'s doc comment.
    dump_saw_alt_screen: bool,
    /// Edge-detect an "Action Required" OSC 0 title so a permission prompt fires
    /// the question notification exactly once (the title repaints every spinner
    /// tick). Agent-agnostic: any agent that puts "Action Required" in its title
    /// (grok, Codex, …) drives this. True while the last title signalled
    /// awaiting-approval.
    title_awaiting: bool,
    /// Question raised by Codex's approval title. A later cancellation may
    /// clear only this question, even if another confident question arrived.
    codex_approval_question: Option<String>,
    /// A cancellation row painted since the current Codex approval began.
    codex_approval_canceled: bool,
    /// The display name to restore an OSC title to, once captured — see
    /// `osc_title::apply_osc_title`'s doc comment for why this is
    /// `Option<Option<String>>` rather than a plain `Option<String>` (outer
    /// `None` = not yet captured; `Some(None)` = captured, and the base was
    /// itself no name). Lives exactly as long as the PTY (this struct is
    /// constructed once per session), so it needs no teardown reap.
    #[allow(clippy::option_option)] // deliberate 3-state disambiguation, see doc comment above
    osc_title_base: Option<Option<String>>,
    /// Reusable screen snapshot handed to the post-lock consumers
    /// (`parse_slash_menu`, `parse_choice_prompt`, the question-dedup absence
    /// check and `rearm_awaiting_for_open_dialog`). Retained across chunks so
    /// the snapshot reuses the row `String` allocations instead of allocating
    /// one per visible row on every chunk that moved anything.
    screen_buf: Vec<String>,
    /// URLs from OSC 1337 `OpenURL` seen during the current `process_chunk`
    /// call, drained by the caller (which holds an owned `Arc<AppState>`,
    /// needed to spawn the confirm-then-open background task — `process_chunk`
    /// itself only has `&AppState`).
    pending_open_urls: Vec<String>,
}

/// A terminal that enters the alternate screen this soon after spawn is treated
/// as starting there; later entries are ordinary full-screen apps (vim, less).
const ALT_SCREEN_STARTUP_WINDOW: std::time::Duration = std::time::Duration::from_secs(15);

/// Toast attributed to `session_id`, on the desktop window and the event bus.
fn emit_session_toast(
    state: &AppState,
    session_id: &str,
    title: &str,
    message: String,
    level: &str,
) {
    #[cfg(feature = "desktop")]
    if let Some(ref app) = *state.app_handle.read() {
        let _ = app.emit(
            "mcp-toast",
            serde_json::json!({
                "title": title,
                "message": message,
                "level": level,
                "sound": null,
                "urgency": "normal",
                "origin_session_id": session_id,
            }),
        );
    }
    let _ = state.event_bus.send(crate::state::AppEvent::McpToast {
        title: title.into(),
        message: Some(message),
        level: level.into(),
        sound: None,
        urgency: "normal".into(),
        origin_repo_path: None,
        origin_session_id: Some(session_id.into()),
    });
}

fn alt_screen_toast_message(label: &str, agent_type: Option<&str>) -> String {
    let fix = match agent_type {
        Some("claude") => {
            "Set CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN=1 and CLAUDE_CODE_DISABLE_AGENT_VIEW=1 (or disableAgentView in settings)."
        }
        Some("codex" | "grok") => "Start it with --no-alt-screen.",
        Some("opencode") => "Start it with --mini.",
        _ => "Check the program for a no-alternate-screen option.",
    };
    format!(
        "{label} switched to the alternate screen at startup. This breaks native scrollback and TUIC agent state detection. {fix}"
    )
}

impl ChunkProcessor {
    fn should_warn_alt_screen(&mut self, agent_type: Option<&str>, alt_screen: bool) -> bool {
        if agent_type.is_none() || !alt_screen || self.alt_screen_warned {
            return false;
        }
        self.alt_screen_warned = true;
        true
    }

    /// True once per PTY, when the screen is alternate on the first startup-window chunk that shows it.
    fn should_toast_alt_screen(&mut self, alt_screen: bool) -> bool {
        if !alt_screen
            || self.alt_screen_toasted
            || std::time::Instant::now() > self.startup_deadline
        {
            return false;
        }
        self.alt_screen_toasted = true;
        true
    }

    fn new(session_cwd: Option<String>, tuic_session: Option<String>) -> Self {
        Self {
            queued_idle_flush: false,
            parser: OutputParser::new(),
            intent_break_parser: vte::Parser::new(),
            last_status_task: None,
            last_question_text: None,
            raw_carry: String::new(),
            last_choice_prompt_sig: None,
            slash_menu_visible: false,
            choice_prompt_needs_repaint: false,
            session_cwd,
            pending_planfiles: Vec::new(),
            emitted_planfiles: std::collections::HashSet::new(),
            gaveup_planfiles: std::collections::HashSet::new(),
            in_alt_buffer: false,
            last_grid_alt_screen: false,
            terminal_mode: crate::ai_agent::tui_detect::TerminalMode::Shell,
            last_vt_log_total: 0,
            alt_screen_warned: false,
            startup_deadline: std::time::Instant::now() + ALT_SCREEN_STARTUP_WINDOW,
            alt_screen_toasted: false,
            tuic_session,
            last_session_conflict_mark: None,
            last_agent_block_line: None,
            last_dump_block_line: None,
            dump_saw_alt_screen: false,
            title_awaiting: false,
            codex_approval_question: None,
            codex_approval_canceled: false,
            osc_title_base: None,
            screen_buf: Vec::new(),
            pending_open_urls: Vec::new(),
        }
    }

    /// Handle OSC 7770 `state=idle|busy|prompt` from the TUIC protocol. Returns
    /// whether this was a real transition and, on a real idle↔busy edge, the `AgentBlock` marking a turn-level command block's
    /// start/end — the primary block source for any hook-instrumented
    /// session, matching the original one-block-per-prompt+output-cycle
    /// design intent independent of the agent's terminal rendering.
    ///
    /// `on_alt_screen` reflects whether the alternate screen buffer was
    /// active when `line` was computed (by the caller, from
    /// `history_size() + cursor row` against whichever screen was active at
    /// that instant). A fullscreen TUI (Claude Code's default renderer) never
    /// grows real alt-screen history, so `line` there is a transient
    /// on-screen cursor row, not a valid scrollback anchor — tagged through
    /// unchanged so row-anchored consumers can skip it.
    fn handle_tuic_state(
        &self,
        payload: &str,
        session_id: &str,
        line: i64,
        on_alt_screen: bool,
        state: &AppState,
    ) -> (bool, Option<ParsedEvent>) {
        let (target, label) = match payload {
            "idle" => (SHELL_IDLE, "idle"),
            "busy" | "prompt" => (SHELL_BUSY, "busy"),
            _ => return (false, None),
        };
        let transitioned = transition_explicit_shell_state(state, session_id, target, label, true);
        if !transitioned {
            return (false, None);
        }
        let block_event = match target {
            SHELL_BUSY => {
                // Clear any stale flag left over from the previous turn. The
                // ToolError/ApiError fallback tier is gated by a 5s silence
                // threshold (SILENCE_TOOL_ERROR_THRESHOLD) that typically
                // fires well after the hook-driven Stop/idle transition
                // already read-and-cleared turn_error_flags (finding it
                // still empty) for a hook-instrumented session — without
                // this, that belated flag would incorrectly attach to
                // whichever turn happens to be running when it finally sets.
                state.session_maps.turn_error_flags.remove(session_id);
                Some(ParsedEvent::AgentBlock {
                    action: "start".into(),
                    line,
                    exit_code: None,
                    prompt_text: last_prompt_text(state, session_id),
                    on_alt_screen,
                    from_transcript_dump: false,
                    new_dump_generation: false,
                })
            }
            SHELL_IDLE => {
                // Read-and-clear: a flag set by a `toolfail` OSC event (from a
                // PostToolUseFailure or StopFailure hook) or the ToolError/ApiError
                // text-pattern fallback becomes this block's red-tick exit code.
                // Cleared unconditionally so it never leaks into the next turn.
                let flagged = state
                    .session_maps
                    .turn_error_flags
                    .remove(session_id)
                    .is_some();
                Some(ParsedEvent::AgentBlock {
                    action: "end".into(),
                    line,
                    exit_code: if flagged { Some(1) } else { None },
                    prompt_text: None,
                    on_alt_screen,
                    from_transcript_dump: false,
                    new_dump_generation: false,
                })
            }
            _ => None,
        };
        (true, block_event)
    }

    /// Apply shell activity transitions from OSC 133 markers.
    fn handle_osc133_event(
        &mut self,
        command: char,
        _params: &str,
        session_id: &str,
        state: &AppState,
    ) {
        match command {
            'A' => {
                crate::codex_binding::revoke(state, session_id);
                transition_explicit_shell_state(state, session_id, SHELL_IDLE, "idle", false);
            }
            'C' => {
                transition_explicit_shell_state(state, session_id, SHELL_BUSY, "busy", false);
            }
            _ => {}
        }
    }

    /// Classify an inline TUI (mouse reporting on the primary screen) the same
    /// way `observe_alt_buffer` classifies `1049h`. Alt-screen nesting still owns
    /// `terminal_mode` once `1049h` has been seen; this only covers the
    /// `grok --no-alt-screen` case that never sends it.
    fn apply_inline_tui_mode(
        &mut self,
        alt_screen: bool,
        mouse_reporting: bool,
        agent_type: Option<&str>,
    ) {
        if alt_screen || self.in_alt_buffer {
            return;
        }
        if mouse_reporting {
            if !self.terminal_mode.is_fullscreen() {
                self.terminal_mode = crate::ai_agent::tui_detect::TerminalMode::FullscreenTui {
                    app_hint: agent_type.map(str::to_string),
                    depth: 1,
                };
            }
            return;
        }
        if matches!(
            self.terminal_mode,
            crate::ai_agent::tui_detect::TerminalMode::FullscreenTui { depth: 1, .. }
        ) {
            self.terminal_mode = crate::ai_agent::tui_detect::TerminalMode::Shell;
        }
    }

    /// Track terminal mode without changing the bytes sent to output watchers.
    fn observe_alt_buffer(&mut self, data: &str) {
        if data.contains("\x1b[?1049h") {
            self.in_alt_buffer = true;
            self.terminal_mode = self.terminal_mode.on_alt_enter();
        } else if data.contains("\x1b[?1049l") {
            self.in_alt_buffer = false;
            self.terminal_mode = self.terminal_mode.on_alt_exit();
        }
    }

    /// Resolve a relative plan-file path to absolute using session CWD.
    /// Returns None if the path is relative and no CWD is available.
    fn resolve_planfile_path(&self, path: &str) -> Option<String> {
        // Both shapes of absolute: `Path::is_absolute` covers `C:\…` on Windows
        // but not a leading `/`, and the agents that emit these lines write
        // either one there. Joining an absolute path onto the session cwd would
        // produce a path that does not exist.
        if path.starts_with('/') || std::path::Path::new(path).is_absolute() {
            Some(path.to_string())
        } else if let Some(ref cwd) = self.session_cwd {
            let joined = std::path::PathBuf::from(cwd).join(path);
            Some(normalize_path(&joined).to_string_lossy().into_owned())
        } else {
            None
        }
    }

    /// Create a flag file that tells the shell wrapper to stop injecting
    /// `--session-id $TUIC_SESSION` into `claude` invocations. This is the
    /// safe alternative to writing `export TUIC_SESSION=…` into the PTY,
    /// which can corrupt fullscreen TUI output or race with user input.
    ///
    /// Guarded by a 3-second cooldown: Claude prints the error line multiple
    /// times as it exits, and we want exactly one flag per conflict burst.
    fn mark_session_no_inject(&mut self, kind: &str) {
        const COOLDOWN: std::time::Duration = std::time::Duration::from_secs(3);
        let now = std::time::Instant::now();
        if self
            .last_session_conflict_mark
            .is_some_and(|t| now.duration_since(t) < COOLDOWN)
        {
            return;
        }
        self.last_session_conflict_mark = Some(now);

        let Some(ref tuic_session) = self.tuic_session else {
            return;
        };

        let flag_path =
            crate::config::config_dir().join(format!("no-session-inject.{tuic_session}"));
        match std::fs::write(&flag_path, b"") {
            Ok(()) => {
                tracing::info!(
                    tuic_session = %tuic_session,
                    kind = %kind,
                    "Created no-session-inject flag after agent-session-conflict"
                );
            }
            Err(e) => {
                tracing::warn!(
                    tuic_session = %tuic_session,
                    error = %e,
                    "Failed to create no-session-inject flag"
                );
            }
        }
    }

    /// Drain pending plan files: emit event for files that now exist, drop expired ones.
    fn check_pending_planfiles(&mut self, session_id: &str, state: &AppState) {
        if self.pending_planfiles.is_empty() {
            return;
        }
        let now = std::time::Instant::now();
        let mut i = 0;
        while i < self.pending_planfiles.len() {
            let (ref path, deadline) = self.pending_planfiles[i];
            if now > deadline {
                tracing::debug!("[plan-file] Retry expired (10s), dropping: {path}");
                let path = self.pending_planfiles.swap_remove(i).0;
                // Tombstone so the still-visible reference isn't re-queued forever.
                self.gaveup_planfiles.insert(path);
                continue;
            }
            if std::path::Path::new(path).is_file() {
                let path = self.pending_planfiles.swap_remove(i).0;
                tracing::info!("[plan-file] Retry succeeded: {path}");
                self.emitted_planfiles.insert(path.clone());
                let evt = ParsedEvent::PlanFile { path };
                if let Ok(json) = serde_json::to_value(&evt).map(std::sync::Arc::new) {
                    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                        session_id: session_id.to_string(),
                        parsed: std::sync::Arc::clone(&json),
                    });
                    #[cfg(feature = "desktop")]
                    if let Some(a) = state.app_handle.read().as_ref() {
                        let _ = a.emit(
                            "pty-parsed",
                            serde_json::json!({
                                "session_id": session_id,
                                "parsed": &*json,
                            }),
                        );
                    }
                }
                continue;
            }
            i += 1;
        }
    }

    /// Process a chunk of PTY output after kitty-sequence stripping.
    /// Handles: VT log buffer, ring buffer, WebSocket broadcast, event parsing,
    /// dedup, resize-grace filtering, PlanFile resolution, event emission,
    /// silence state, last_output_ms, and shell state transitions.
    ///
    /// Returns true when the chunk was non-empty, i.e. the caller should hand
    /// the SAME borrowed bytes to the output watchers. It used to return an owned
    /// copy of the chunk, which allocated and memcpy'd up to 64 KB per PTY read
    /// for a value the caller already held.
    /// `app` is Some for desktop mode (emits Tauri IPC), None for headless.
    fn process_chunk(
        &mut self,
        data: &str,
        silence: &Arc<Mutex<SilenceState>>,
        session_id: &str,
        state: &AppState,
    ) -> bool {
        if data.is_empty() {
            return false;
        }

        // Check pending plan files: emit if file appeared, drop if deadline expired.
        self.check_pending_planfiles(session_id, state);

        // Read once, before the vt_log lock: the screen classifier needs it inside
        // that lock, and taking a session_states shard while holding the vt_log
        // mutex would introduce a lock order this file does not otherwise have.
        let agent_type = state
            .session_maps
            .session_states
            .get(session_id)
            .and_then(|s| s.agent_type.clone());

        // The screen snapshot is refilled in place: `screen_rows()` is
        // `prev_rows.clone()`, one allocation per visible row per chunk. Taking
        // the buffer out of `self` keeps the later `&mut self` uses (parser,
        // dedup markers) borrow-checkable; it is put back at the end.
        let mut screen_buf = std::mem::take(&mut self.screen_buf);
        let mut ego_question_rows = None;
        let mut unexpected_alt_screen = false;
        let mut startup_alt_screen = false;
        let mut choice_changed_rows = Vec::new();

        // Feed raw data (post-kitty-strip) into VT100 log buffer.
        // `total_lines` comes back with it: a chunk that grew the buffer produced
        // real output, a chunk that did not merely repainted the screen.
        let (
            changed_rows,
            vt_output_grew,
            term_events,
            screen_present,
            screen_activity,
            cursor_row,
            logical_prefix,
            physical_prefix,
            _history_size,
            intent_origin,
            intent_candidate,
            on_alt_screen,
        ): VtProcessResult = if let Some(vt_log) = state.grid.vt_log_buffers.get(session_id) {
            // Phase 1: process the chunk and drain events under the lock,
            // but do NOT write any reply while holding it — write_terminal_reply's
            // write_all/flush can block (a SIGSTOP'd/standby child, or a full
            // PTY input queue), and blocking here would stall every other
            // consumer of this session's grid (the frame ticker, HTTP
            // terminal reads) for as long as the write is stuck. Collect
            // replies to flush once the lock is dropped, below.
            use crate::terminal_grid::TermEvent;
            let (
                mut changed,
                total,
                hist,
                intent_origin,
                alt_screen,
                mouse_reporting,
                tevts,
                pending_replies,
                kitty_image_store,
                kitty_pending_jobs,
            ) = {
                let mut vt = vt_log.lock();
                let changed = vt.process(data.as_bytes());
                // Cloned Arc handles, not the grid itself — resolving any
                // Kitty decode job these bytes just queued happens below,
                // AFTER this lock drops (color-tools plan: a security
                // review found Kitty image decode running fully
                // synchronously under this lock, a real continuous cost for
                // a sustained video stream like `mpv --vo=kitty`).
                let (kitty_image_store, kitty_pending_jobs) = vt.grid_kitty_decode_handles();
                // Publish the real sync state (a nested BSU keeps it open) so the
                // frame ticker knows whether this session can have a stalled
                // synchronized update worth taking the lock for.
                if let Some(flag) = state.grid.sync_update_active.get(session_id) {
                    flag.store(vt.is_sync_update_active(), Ordering::Relaxed);
                }
                let total = vt.total_lines();
                let hist = vt.grid_history_size();
                let intent_origin = vt.grid_screen_origin();
                let alt_screen = vt.is_alternate_screen();
                unexpected_alt_screen =
                    self.should_warn_alt_screen(agent_type.as_deref(), alt_screen);
                startup_alt_screen = self.should_toast_alt_screen(alt_screen);
                let mouse_reporting = vt.is_mouse_reporting();
                let tevts = vt.grid_drain_events();
                let mut pending_replies: Vec<String> = Vec::new();
                let tevts: Vec<TermEvent> = tevts
                    .into_iter()
                    .filter(|evt| {
                        let TermEvent::PtyWrite(response) = evt else {
                            return true;
                        };
                        // Four substring scans of a terminal reply, for a
                        // diagnostic error line only. Behind the toggle.
                        if crate::cpu_watchdog::diagnostic_mode()
                            && (response.contains("\x1b[?1049")
                                || response.contains("\x1b[?1047")
                                || response.contains("\x1b[?47l")
                                || response.contains("\x1b[?25h"))
                        {
                            tracing::error!(source = "terminal", session_id = %session_id,
                                "PtyWrite contains DEC private mode sequences! response={:?}",
                                response.as_bytes().iter().take(200).collect::<Vec<_>>());
                        }
                        pending_replies.push(response.clone());
                        false
                    })
                    .collect();
                (
                    changed,
                    total,
                    hist,
                    intent_origin,
                    alt_screen,
                    mouse_reporting,
                    tevts,
                    pending_replies,
                    kitty_image_store,
                    kitty_pending_jobs,
                )
            };

            // A wheel/scrollbar gesture computes its target against whichever
            // grid is active when the gesture fires, but `pending_scroll` is
            // consumed asynchronously by the frame ticker on its next tick —
            // if `swap_alt()` (entering/exiting an app's alternate screen,
            // e.g. launching or quitting Claude Code) lands in between, that
            // stale target gets clamped against and applied to the NEW grid,
            // landing the view short of true bottom by whatever the clamp
            // allows. Clear it on every real alt-screen transition so a
            // leftover gesture from the old grid is discarded instead of
            // replayed. Lock-free by design (see `pending_scroll`'s doc
            // comment) — deliberately done outside the `vt_log` lock above.
            clear_pending_scroll_on_alt_screen_transition(
                &state.grid.pending_scroll,
                session_id,
                alt_screen,
                &mut self.last_grid_alt_screen,
            );

            // CPR/DSR/DA1 replies (`device_status`/`identify_terminal` in the
            // alacritty fork's Handler impl) are latency-sensitive: a pager
            // like `leaf` sets a short internal deadline waiting on the
            // cursor-position reply and prints the raw escape sequence as
            // text once that deadline passes. Flush them to the child's
            // stdin now — lock-free, and before the chrome-filter/
            // classification work below, which clones the full screen on a
            // chunk's first paint, exactly the case where a freshly-launched
            // TUI queries cursor position.
            for response in &pending_replies {
                write_terminal_reply(state, session_id, response.as_bytes(), "PtyWrite");
            }

            // Resolve any Kitty image transmission this chunk queued for
            // deferred decode (color-tools plan) — genuinely lock-free now,
            // using only the two handles cloned above, never `vt_log`.
            resolve_kitty_decode_jobs(state, session_id, &kitty_image_store, &kitty_pending_jobs);

            // Did this chunk produce real output, or merely repaint rows that were
            // already there (SIGWINCH reflow, cursor blink, statusline)? In the
            // PRIMARY screen a repaint never grows the durable log while real work
            // scrolls new lines into it, so the total answers the question.
            //
            // In the ALTERNATE screen it cannot: `VtLogBuffer::process` skips log
            // capture entirely while alt is active, so the total is frozen however
            // much the agent writes, and "did not grow" is not evidence of a
            // repaint. Nothing is lost by reporting growth there — the only reader
            // is the resize-grace extension below, which covers a reflow, and the
            // grid performs no reflow in alt (`VtLogBuffer::resize` picks
            // `ReflowMode::None`). Reading a frozen total as a repaint instead
            // re-armed the grace on every chunk, so a single resize suppressed
            // low-confidence questions, rate-limit and API-error events and the
            // BUSY transition until the agent paused for a full second.
            let vt_output_grew = alt_screen || total > self.last_vt_log_total;
            self.last_vt_log_total = self.last_vt_log_total.max(total);
            // Grid is the source of truth for mouse DECSET (including combined
            // `?1000;1002;1006h`). String-matching the chunk would miss grok.
            self.apply_inline_tui_mode(alt_screen, mouse_reporting, agent_type.as_deref());

            // Did ANYTHING on screen move? Taken before the chrome filter below,
            // because that filter drops rows under the input-area border and a
            // choice dialog can render there.
            //
            // DEFERRED (2026-09-06) — moving this AFTER the chrome filter was
            // proposed to stop a 1 Hz status line paying for the snapshot, and
            // it is wrong: Claude Code renders its slash menu BELOW the input
            // box, so on a slash-menu tick every changed row is under the
            // cutoff and the menu would never be parsed. Reproduced — flipping
            // the two lines turns `chunk_path_scenarios_emit_the_same_events`
            // from `["slash-menu"]` into `[]`. Any future attempt needs a
            // per-consumer gate, not one shared flag.
            let any_row_changed = !changed.is_empty();
            if self.last_choice_prompt_sig.is_some() {
                choice_changed_rows.extend(changed.iter().map(|row| row.row_index));
            }

            // Phase 2: re-lock for the screen-diff/classification work below.
            // Nothing else mutates this session's grid between phase 1 and
            // here (each session has exactly one reader thread), so this is
            // just a second short, uncontended acquisition — the point above
            // was only to keep the potentially-blocking write out of the
            // critical section, not to avoid a second lock/unlock.
            let vt = vt_log.lock();

            // ONE borrow of the rendered screen, shared by all three consumers
            // below (chrome cutoff, screen classification, snapshot refill).
            // They used to take three independent `screen_rows_ref()` borrows
            // and the last one cloned.
            let screen_ref = vt.screen_rows_ref();

            // Filter out changed rows below the input area border (horizontal rule).
            // Claude Code (and similar agents) render a quota/budget status bar below
            // the input box separator. Those rows are cosmetic chrome — processing them
            // resets the silence timer and causes false busy→idle→question transitions.
            //
            // `retain` in place: the filter used to rebuild the whole Vec even
            // when the cutoff dropped nothing.
            // Goose appends the reply BELOW their submitted `> prompt`;
            // it has no fixed input-area border. Trimming there erases the
            // reply's activity evidence and strands user-submit BUSY
            // (#1812-022d).
            let chrome_cutoff = if !matches!(agent_type.as_deref(), Some("goose"))
                && let Some(screen) = screen_ref
                && !changed.is_empty()
            {
                let refs: Vec<&str> = screen.iter().map(String::as_str).collect();
                // Fails OPEN by contract: no anchor found is `None`, and `None`
                // must mean "parse everything", never "parse nothing".
                crate::chrome::find_agent_chrome_cutoff(&refs, agent_type.as_deref())
            } else {
                None
            };
            if let Some(cutoff) = chrome_cutoff {
                changed.retain(|r| r.row_index < cutoff);
            }
            let changed = changed;

            // Screen classification runs on EVERY chunk, borrowed, never cloned: a
            // repaint that is byte-identical produces no changed rows, and holding
            // BUSY through exactly that case (a frozen spinner, DEC 2026 frame
            // coalescing) is the point of `detect_agent_screen_activity`.
            let screen_activity = screen_ref
                .map(|rows| {
                    detect_agent_screen_activity_at(
                        agent_type.as_deref(),
                        rows,
                        Some(vt.grid_columns()),
                    )
                })
                .unwrap_or(AgentScreenActivity::Unknown);

            // ONE snapshot per tick, cloned into the retained buffer and handed
            // to every post-lock consumer. A slash menu or a choice dialog
            // cannot have appeared on a screen where nothing moved, so a chunk
            // that changed no row skips the snapshot entirely — that is the
            // per-chunk hot path.
            //
            // `any_row_changed` is deliberately the PRE-cutoff answer: Claude
            // Code renders its slash menu BELOW the input-box chrome, so gating
            // on the trimmed rows would stop the menu being seen at all
            // (proved by `chunk_path_scenarios_emit_the_same_events`).
            let screen_present = match screen_ref.filter(|_| any_row_changed) {
                Some(rows) => {
                    screen_buf.truncate(rows.len());
                    for (slot, row) in screen_buf.iter_mut().zip(rows) {
                        slot.clear();
                        slot.push_str(row);
                    }
                    screen_buf.extend(rows[screen_buf.len().min(rows.len())..].iter().cloned());
                    true
                }
                None => false,
            };
            // Ego selectors are terminal-soft-wrapped, not prose paragraphs.
            // Join from grid metadata before the parser loses the boundary.
            if screen_present && agent_type.as_deref() == Some("ego") {
                ego_question_rows = Some(
                    crate::redaction::join_wrapped_rows(
                        screen_buf.iter().zip(vt.screen_row_wraps()),
                    )
                    .split('\n')
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
                );
            }
            let cursor_row = vt.cursor_point().0;
            let logical_prefix = vt.logical_prefix_at_cursor();
            let physical_prefix = vt.physical_prefix_at_cursor();
            let intent_candidate = agent_type.as_ref().and_then(|_| {
                // A repaint can change many rows below the same anchor. Cache
                // its result for this tick, including rejected candidates.
                let mut cache = std::collections::HashMap::new();
                changed.iter().rev().find_map(|row| {
                    // A later read may update only an indented continuation.
                    // Search its bounded predecessors for the unchanged anchor.
                    (row.row_index
                        .saturating_sub(crate::output_parser::MAX_INTENT_CONTINUATION_ROWS)
                        ..=row.row_index)
                        .rev()
                        .find_map(|anchor_row| {
                            if screen_ref.is_some_and(|screen| {
                                !screen
                                    .get(anchor_row)
                                    .is_some_and(|text| text.contains("intent:"))
                            }) {
                                return None;
                            }
                            let cached = cache.entry(anchor_row).or_insert_with(|| {
                                #[cfg(test)]
                                INTENT_CANDIDATE_GRID_READS
                                    .with(|reads| reads.set(reads.get() + 1));
                                let mut line = vt.logical_line_at_row(anchor_row)?;
                                if chrome_cutoff.is_some_and(|cutoff| line.end_row >= cutoff) {
                                    return None;
                                }
                                if crate::output_parser::structured_token_anchor(&line.text)
                                    != Some(crate::output_parser::StructuredTokenAnchor::Intent)
                                {
                                    return None;
                                }
                                let anchor_text = line.text.clone();
                                let mut block = anchor_text.clone();
                                let mut continuation_ends = Vec::new();
                                let mut physical_widths = Vec::new();
                                physical_widths.push(
                                    screen_ref
                                        .and_then(|screen| screen.get(line.end_row))
                                        .map_or_else(
                                            || {
                                                unicode_width::UnicodeWidthStr::width(
                                                    anchor_text.as_str(),
                                                )
                                            },
                                            |row| {
                                                unicode_width::UnicodeWidthStr::width(
                                                    row.trim_end(),
                                                )
                                            },
                                        ),
                                );
                                let mut next = line.end_row + 1;
                                // DEFERRED (2026-09-25) — Stop at the chrome cutoff once a
                                // production-path test captures a task panel under an intent.
                                for _ in 0..crate::output_parser::MAX_INTENT_CONTINUATION_ROWS {
                                    if crate::output_parser::intent_row_is_complete(&block) {
                                        break;
                                    }
                                    if chrome_cutoff.is_some_and(|cutoff| next >= cutoff) {
                                        break;
                                    }
                                    #[cfg(test)]
                                    INTENT_CONTINUATION_GRID_READS
                                        .with(|reads| reads.set(reads.get() + 1));
                                    let Some(continuation) = vt.logical_line_at_row(next) else {
                                        break;
                                    };
                                    if continuation.start_row != next {
                                        break;
                                    }
                                    block.push('\n');
                                    block.push_str(&continuation.text);
                                    continuation_ends.push(continuation.end_row);
                                    physical_widths.push(
                                        screen_ref
                                            .and_then(|screen| screen.get(continuation.end_row))
                                            .map_or_else(
                                                || {
                                                    unicode_width::UnicodeWidthStr::width(
                                                        continuation.text.as_str(),
                                                    )
                                                },
                                                |row| {
                                                    unicode_width::UnicodeWidthStr::width(
                                                        row.trim_end(),
                                                    )
                                                },
                                            ),
                                    );
                                    let (joined, absorbed) =
                                        crate::output_parser::dewrap_intent_continuation_with_rows(
                                            &block,
                                            Some((vt.grid_columns(), &physical_widths)),
                                        );
                                    if absorbed != continuation_ends.len() {
                                        break;
                                    }
                                    if crate::output_parser::intent_row_is_complete(
                                        joined.lines().next().unwrap_or_default(),
                                    ) {
                                        break;
                                    }
                                    next = continuation.end_row + 1;
                                }
                                let (dewrapped, absorbed) =
                                    crate::output_parser::dewrap_intent_continuation_with_rows(
                                        &block,
                                        Some((vt.grid_columns(), &physical_widths)),
                                    );
                                line.text =
                                    dewrapped.lines().next().unwrap_or_default().to_string();
                                if absorbed > 0 {
                                    line.end_row = continuation_ends[absorbed - 1];
                                }
                                Some((line, anchor_text))
                            });
                            let (line, anchor_text) = cached.as_ref()?;
                            (line.start_row..=line.end_row)
                                .contains(&row.row_index)
                                .then(|| (line.clone(), anchor_text.clone()))
                        })
                })
            });

            (
                changed,
                vt_output_grew,
                tevts,
                screen_present,
                screen_activity,
                Some(cursor_row),
                logical_prefix,
                physical_prefix,
                hist,
                intent_origin,
                intent_candidate,
                alt_screen,
            )
        } else {
            (
                Vec::new(),
                false,
                Vec::new(),
                false,
                AgentScreenActivity::Unknown,
                None,
                None,
                None,
                0,
                0,
                None,
                false,
            )
        };
        // The grid's eviction-stable screen origin (`TerminalGrid::screen_origin`,
        // i.e. `total_scrolled()`): the coordinate every stored block row is built
        // from, so heuristic/transcript-dump blocks match OSC 133 ones.
        let total_scrolled = intent_origin;

        if screen_present
            && agent_type.as_deref() == Some("claude")
            && state.managed_trust_dialogs.contains(session_id)
            && managed_claude_trust_dialog(&screen_buf)
            && state.managed_trust_dialogs.remove(session_id).is_some()
            && let Err(error) = accept_managed_claude_trust_dialog(state, session_id)
        {
            tracing::warn!(source = "terminal", session_id, %error, "Could not accept managed Claude workspace trust dialog");
        }

        if startup_alt_screen {
            let label = state
                .session_maps
                .sessions
                .get(session_id)
                .and_then(|entry| entry.lock().display_name.clone())
                .unwrap_or_else(|| session_id.to_string());
            emit_session_toast(
                state,
                session_id,
                "Terminal is on the alternate screen",
                alt_screen_toast_message(&label, agent_type.as_deref()),
                "warn",
            );
        }

        if unexpected_alt_screen {
            let agent = agent_type.as_deref().unwrap_or("unknown").to_string();
            let session_id = session_id.to_string();
            std::thread::spawn(move || {
                let version = crate::agent::detect_agent_binary_sync(agent.clone())
                    .version
                    .unwrap_or_else(|| "unknown".to_string());
                tracing::warn!(
                    source = "terminal",
                    session_id,
                    agent,
                    version,
                    "Agent entered alternate screen despite native scrollback default"
                );
            });
        }

        // Nothing is emitted for scrollback growth. There was a throttled
        // `pty-vt-log-total-{session_id}` here whose comment claimed the frontend
        // listened for it and refreshed the scrollback overlay; no such listener
        // ever existed on either transport. `Manager::emit` serializes the
        // payload before it consults the listener registry, so a dead event is
        // not free — and a comment describing a consumer that is not there costs
        // more, because the next reader builds the frontend half rather than
        // deleting the emit. The overlay reads the totals when it fetches a
        // chunk. `last_vt_log_total` stays: `vt_output_grew` is a real reader.

        // Handle terminal events from alacritty (title, clipboard, PTY writes, OSC 133, TUIC)
        let mut tuic_events: Vec<ParsedEvent> = Vec::new();
        // Lifecycle markers retract recovery; they do not announce it. Track the
        // pending snapshot too, since these events are published after this loop.
        let mut recovery_active = state
            .session_maps
            .session_states
            .get(session_id)
            .is_some_and(|session| session.provider_recovery.is_some());
        let mut explicit_idle_in_chunk = false;
        // Set by "notify"/"notifytype" verbs, consumed by the very next
        // "state" verb — a Claude Code `Notification` hook fire always emits
        // `notify=<message>` (and, when Claude Code sends the field,
        // `notifytype=<notification_type>`) immediately followed by
        // `state=awaiting` in the same `write_all` (see `tuic-hook`'s
        // `DERIVATIONS` table and `emit.rs`), so all land in this same chunk
        // in that order. `PreToolUse(AskUserQuestion|ExitPlanMode)` and
        // `Elicitation` never scrape either, so both stay `None` for those —
        // see `notification_awaiting_outcome`.
        let mut pending_notify_message: Option<String> = None;
        let mut pending_notification_type: Option<String> = None;
        if !term_events.is_empty() {
            use crate::terminal_grid::{Osc133Event, TermEvent};
            for evt in term_events {
                match evt {
                    TermEvent::Bell => {
                        state.emit_pty_event(crate::state::AppEvent::PtyBell {
                            session_id: session_id.to_string(),
                        });
                        #[cfg(feature = "desktop")]
                        if let Some(a) = state.app_handle.read().as_ref() {
                            let _ = a.emit(&format!("pty-bell-{session_id}"), ());
                        }
                    }
                    TermEvent::PtyWrite(_) => {
                        // Flushed synchronously right after grid_drain_events,
                        // above, before the screen diff/classification —
                        // CPR/DSR replies are latency-sensitive and must not
                        // wait behind that work. Unreachable in practice; log
                        // if it isn't, since it means the early flush above
                        // was skipped.
                        tracing::error!(source = "terminal", session_id = %session_id,
                            "PtyWrite reached the deferred event loop instead of being flushed early");
                    }
                    TermEvent::Title(title) => {
                        // Both transports now converge on one display name —
                        // see osc_title.rs's module doc comment for why this
                        // used to be a desktop-only app.emit with no bus arm
                        // and no write to PtySession.display_name anywhere.
                        crate::osc_title::apply_osc_title(
                            state,
                            session_id,
                            Some(&title),
                            &mut self.osc_title_base,
                        );
                        // Some agents signal an awaiting-approval permission prompt by
                        // putting "Action Required" in their OSC 0 title (grok prefixes
                        // "⚠ Action Required - ⠙ - Running: echo … - Execute Shell …";
                        // Codex uses "[ . ] Action Required | …"). Agent-agnostic: any
                        // such title drives this. The title repaints every spinner tick,
                        // so edge-detect the false→true transition and fire the question
                        // exactly once.
                        // DEFERRED (2026-06-11) — grok 0.2.45 in always-approve mode
                        // emits titles like "Run Shell Command echo … - grok" with NO
                        // "Action Required" prefix (verified live). The prefix may be
                        // version/permission-mode specific; the on-screen "◆ …?" prompt
                        // (cliclack path in output_parser) covers real approvals. Re-verify
                        // grok's title in default (non-always-approve) mode before removing.
                        let title_awaiting = title.contains("Action Required");
                        if title_awaiting && !self.title_awaiting {
                            let prompt_text = clean_action_required_title(&title);
                            if agent_type.as_deref() == Some("codex") {
                                self.codex_approval_question = Some(prompt_text.clone());
                                self.codex_approval_canceled = false;
                            }
                            tuic_events.push(ParsedEvent::Question {
                                prompt_text,
                                confident: true,
                            });
                        }
                        self.title_awaiting = title_awaiting;
                    }
                    TermEvent::ResetTitle => {
                        crate::osc_title::apply_osc_title(
                            state,
                            session_id,
                            None,
                            &mut self.osc_title_base,
                        );
                        self.title_awaiting = false;
                    }
                    TermEvent::ClipboardStore(text) => {
                        // Was a suffixed desktop-only `app.emit` with no bus
                        // arm at all (D.7) — a browser/PWA client never saw
                        // an OSC 52 clipboard-store. Converted to the
                        // unsuffixed `PtyClipboardStore` variant so it can
                        // ride SSE/WS like every other session-scoped event.
                        state.emit_dual(crate::state::AppEvent::PtyClipboardStore {
                            session_id: session_id.to_string(),
                            text: text.clone(),
                        });
                    }
                    TermEvent::RequestFocus => {
                        if state.config.read().osc1337_focus_attention {
                            #[cfg(feature = "desktop")]
                            if let Some(a) = state.app_handle.read().as_ref()
                                && let Some(window) = a.get_webview_window("main")
                            {
                                let _ = window.unminimize();
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                    TermEvent::RequestAttention(value) => {
                        if state.config.read().osc1337_focus_attention
                            && let Some(level) = attention_level_for_value(&value)
                        {
                            #[cfg(feature = "desktop")]
                            if let Some(a) = state.app_handle.read().as_ref()
                                && let Some(window) = a.get_webview_window("main")
                            {
                                let attention = match level {
                                    AttentionLevel::Cancel => None,
                                    AttentionLevel::Informational => {
                                        Some(tauri::UserAttentionType::Informational)
                                    }
                                    AttentionLevel::Critical => {
                                        Some(tauri::UserAttentionType::Critical)
                                    }
                                };
                                let _ = window.request_user_attention(attention);
                            }
                        }
                    }
                    TermEvent::OpenUrl(url) => {
                        self.pending_open_urls.push(url);
                    }
                    TermEvent::Osc133 {
                        command,
                        params,
                        line,
                        on_alt_screen,
                    } => {
                        explicit_idle_in_chunk |= command == 'A';
                        if command == 'A' && recovery_active {
                            recovery_active = false;
                            tuic_events.push(ParsedEvent::ProviderRecovery { recovery: None });
                        }
                        state
                            .session_maps
                            .has_osc133_integration
                            .insert(session_id.to_string(), ());
                        self.handle_osc133_event(command, &params, session_id, state);
                        // Dual-emitted: there is no bus→window forwarder, so the
                        // desktop event and the bus push are two separate writes of
                        // one signal. The bus copy is what gives a browser/PWA
                        // client its command blocks, gutter marks and Cmd+Up/Down
                        // navigation, through the `osc133` grid-WS frame.
                        let exit_code = parse_osc133_exit_code(command, &params);
                        #[cfg(feature = "desktop")]
                        if let Some(a) = state.app_handle.read().as_ref() {
                            let _ = a.emit(
                                &format!("pty-osc133-{session_id}"),
                                &Osc133Event {
                                    marker: command.to_string(),
                                    line,
                                    exit_code,
                                    on_alt_screen,
                                },
                            );
                        }
                        state.emit_pty_event(crate::state::AppEvent::PtyOsc133 {
                            session_id: session_id.to_string(),
                            marker: command.to_string(),
                            line,
                            exit_code,
                            on_alt_screen,
                        });
                    }
                    TermEvent::Osc7(url) => {
                        #[allow(clippy::collapsible_if)]
                        if let Ok(cwd) = parse_osc7_cwd(&url) {
                            if let Some(entry) = state.session_maps.sessions.get(session_id) {
                                entry.lock().cwd = Some(cwd.clone());
                            }
                            // `{ cwd }` rather than a bare string so this payload
                            // is identical to the `cwd` grid-WS frame — a WS frame
                            // must carry a `type` discriminator and therefore
                            // cannot be a bare string. Same shape on both
                            // transports means no branch in CanvasTerminal.
                            #[cfg(feature = "desktop")]
                            if let Some(a) = state.app_handle.read().as_ref() {
                                let _ = a.emit(
                                    &format!("pty-cwd-{session_id}"),
                                    serde_json::json!({ "cwd": cwd }),
                                );
                            }
                            state.emit_pty_event(crate::state::AppEvent::PtyCwd {
                                session_id: session_id.to_string(),
                                cwd,
                            });
                        }
                    }
                    TermEvent::Tuic {
                        verb,
                        payload,
                        line,
                        on_alt_screen,
                    } => match verb.as_str() {
                        "ego-recovery" if agent_type.as_deref() == Some("ego") => {
                            if let Some(event) =
                                crate::output_parser::parse_provider_recovery(&payload)
                            {
                                if let ParsedEvent::ProviderRecovery { recovery } = &event {
                                    recovery_active = recovery.is_some();
                                }
                                tuic_events.push(event);
                            }
                        }
                        "codex-session" => crate::codex_binding::receive(
                            state,
                            session_id,
                            self.tuic_session.as_deref(),
                            &payload,
                        ),
                        "state" => {
                            // Codex can emit completion while an approval or /plan
                            // question is still unanswered. Require the same currently
                            // rendered dialog and its active controls. Real answer input
                            // retires the controls before subsequent idle is accepted.
                            let open_codex_dialog = payload == "idle"
                                && agent_type.as_deref() == Some("codex")
                                && state
                                    .session_maps
                                    .session_states
                                    .get(session_id)
                                    .is_some_and(|session| {
                                        session.awaiting_input
                                            && session.choice_prompt.as_ref().is_some_and(|prompt| {
                                                matches!(
                                                    crate::output_parser::parse_choice_prompt(&screen_buf),
                                                    Some(ParsedEvent::ChoicePrompt { title, .. })
                                                        if title == prompt.title
                                                )
                                            })
                                    });
                            if open_codex_dialog {
                                tracing::warn!(
                                    session_id,
                                    "Ignoring delayed idle while native Codex dialog is unanswered"
                                );
                                continue;
                            }
                            // idle/busy drive the shell-state machine; awaiting is
                            // ignored here (it's a separate field). The awaiting_input
                            // field is driven by Question/UserInput events instead.
                            explicit_idle_in_chunk |= payload == "idle";
                            // Presence of any state event at all (not just idle/busy)
                            // proves this session's hook (or whatever emits OSC 7770)
                            // is wired up — suppresses the `⏺` heuristic fallback.
                            // Guard the insert: this event repeats for the rest of the
                            // session's life (UserPromptSubmit, every PreToolUse/
                            // PostToolUse, Stop...), so skip the allocation + DashMap
                            // write lock once it's already set.
                            if !state
                                .session_maps
                                .has_tuic_state_integration
                                .contains_key(session_id)
                            {
                                state
                                    .session_maps
                                    .has_tuic_state_integration
                                    .insert(session_id.to_string(), ());
                            }
                            let (_, block_event) = self.handle_tuic_state(
                                &payload,
                                session_id,
                                line as i64,
                                on_alt_screen,
                                state,
                            );
                            if let Some(evt) = block_event {
                                tuic_events.push(evt);
                            }
                            if matches!(payload.as_str(), "busy" | "idle") && recovery_active {
                                recovery_active = false;
                                tuic_events.push(ParsedEvent::ProviderRecovery { recovery: None });
                            }
                            // Consumed here regardless of payload — a notify/
                            // notifytype pair only ever precedes its own paired
                            // state verb (see the declaration above), so nothing
                            // legitimate is lost by clearing it on a "busy"/"idle"
                            // state too.
                            let notify_message = pending_notify_message.take();
                            let notification_type = pending_notification_type.take();
                            // Unaffected by the `handle_tuic_state` call above for
                            // an "awaiting" payload (it only mutates on "idle"/
                            // "busy") — this is the shell state as it stood BEFORE
                            // this fire, which is exactly what distinguishes "the
                            // turn already ended" from "still stuck mid-turn."
                            let shell_already_idle = state
                                .session_maps
                                .shell_states
                                .get(session_id)
                                .is_some_and(|s| s.load(Ordering::Relaxed) == SHELL_IDLE);
                            let evt = tuic_state_awaiting_event(
                                &payload,
                                line as i64,
                                notify_message.as_deref(),
                                notification_type.as_deref(),
                                shell_already_idle,
                            );
                            // Notification-sourced classification is the one
                            // decision in this arm with no other trace when it
                            // suppresses outright (`evt` is `None`, so no event
                            // reaches state.rs's reducer at all — unlike a real
                            // state change, a "correctly did nothing" outcome
                            // would otherwise be invisible to a future
                            // investigation). Logged for every Notification-
                            // sourced fire, not just the suppressed case, so a
                            // stuck-badge report can see the full classification
                            // — inputs and outcome — in one place. See
                            // `agent-signal-architecture.html`'s Investigation
                            // Playbook.
                            if payload == "awaiting"
                                && (notify_message.is_some() || notification_type.is_some())
                            {
                                let confident = match &evt {
                                    Some(ParsedEvent::Question { confident, .. }) => {
                                        Some(*confident)
                                    }
                                    _ => None,
                                };
                                tracing::debug!(
                                    session_id = %session_id,
                                    notification_type = notification_type.as_deref().unwrap_or("<none>"),
                                    has_message = notify_message.is_some(),
                                    shell_already_idle,
                                    confident = ?confident,
                                    suppressed = evt.is_none(),
                                    "Notification-sourced state=awaiting classified \
                                     (research: notification confidence)"
                                );
                                // Kept for query (state-explain), not just the log
                                // line above — a rare fire, so one small
                                // allocation here is fine; never on the per-chunk
                                // path.
                                if let Some(sl) = state.session_maps.silence_states.get(session_id)
                                {
                                    sl.lock().last_notification_classification =
                                        Some(NotificationClassification {
                                            at: std::time::Instant::now(),
                                            notification_type,
                                            has_message: notify_message.is_some(),
                                            shell_already_idle,
                                            confident,
                                            suppressed: evt.is_none(),
                                        });
                                }
                            }
                            if let Some(evt) = evt {
                                tuic_events.push(evt);
                            }
                        }
                        "suggest" => {
                            // Tolerate an optional `[ … ]` wrapper so this OSC
                            // channel accepts the same payload as the text token
                            // (`suggest: [ A | B | C ]`).
                            let inner = payload.trim();
                            let inner = inner.strip_prefix('[').unwrap_or(inner);
                            let inner = inner.strip_suffix(']').unwrap_or(inner);
                            let items: Vec<String> = inner
                                .split('|')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            if !items.is_empty() {
                                tuic_events.push(ParsedEvent::Suggest { items });
                            }
                        }
                        "intent" => {
                            let (text, title) = if let Some(paren_start) = payload.rfind('(') {
                                let desc = payload[..paren_start].trim().to_string();
                                let t = payload[paren_start + 1..]
                                    .trim_end_matches(')')
                                    .trim()
                                    .to_string();
                                (desc, if t.is_empty() { None } else { Some(t) })
                            } else {
                                (payload.clone(), None)
                            };
                            if let Some(event) = silence.lock().accept_intent(text, title) {
                                tuic_events.push(event);
                            }
                        }
                        "block" => {
                            let (action, exit_code) =
                                if let Some(rest) = payload.strip_prefix("end;") {
                                    ("end".to_string(), rest.parse::<i32>().ok())
                                } else {
                                    (payload.clone(), None)
                                };
                            if action == "start" || action == "end" {
                                tuic_events.push(ParsedEvent::AgentBlock {
                                    action,
                                    line: line as i64,
                                    exit_code,
                                    prompt_text: None,
                                    on_alt_screen,
                                    from_transcript_dump: false,
                                    new_dump_generation: false,
                                });
                            }
                        }
                        "toolfail" => {
                            // From a PostToolUseFailure hook (payload = tuic-hook's
                            // natively-extracted exit code, or its own fallback sentinel
                            // if extraction failed) or a StopFailure hook (payload = a fixed sentinel —
                            // the event firing at all, rather than Stop, is itself the
                            // failure signal). Presence is all that matters — read and
                            // cleared as an arbitrary non-zero sentinel exit code at the
                            // next busy→idle edge in `handle_tuic_state`; the actual
                            // payload value is intentionally never parsed here.
                            state
                                .session_maps
                                .turn_error_flags
                                .insert(session_id.to_string(), ());
                        }
                        "bgtasks" => {
                            // From a Stop/StopFailure hook: `tuic-hook` scraped
                            // the raw, comma-joined `status` of every entry in
                            // Claude Code's own `background_tasks` array (see
                            // `background_task_statuses` in tuic-hook's
                            // main.rs) — classification of which statuses mean
                            // "still running" happens here, not in tuic-hook
                            // (see that crate's AGENTS.md). A side-table write
                            // only, like `toolfail` above — not `AgentMetadata`,
                            // which has no consumer today. Stamped with the
                            // CURRENT turn epoch so it self-expires the moment
                            // a new prompt is submitted, exactly like
                            // `completion_declared`/`mark_suggest_candidate`.
                            //
                            // Classified fail-safe, not as an exact `"running"`
                            // match: Claude Code's `background_tasks[].status`
                            // vocabulary is not a documented closed set anywhere
                            // (unlike `notification_type`'s enumerated values) —
                            // only `"completed"`/`"failed"` are confirmed terminal
                            // from real production captures. Any OTHER status
                            // (a future/unrecognized value, or a transient one
                            // like "pending"/"queued" this binary has never seen)
                            // is treated as still active, so an unrecognized
                            // status can never silently clear a real declaration
                            // — the same "unanswered evidence counts as work"
                            // philosophy `has_pending_background_probe` already
                            // uses for the OS-heuristic path.
                            const KNOWN_TERMINAL_STATUSES: &[&str] = &["completed", "failed"];
                            let decoded = percent_decode_osc_payload(&payload);
                            let running = decoded.split(',').any(|status| {
                                !status.is_empty() && !KNOWN_TERMINAL_STATUSES.contains(&status)
                            });
                            if let Some(turn_epoch) = state
                                .session_maps
                                .session_states
                                .get(session_id)
                                .map(|s| s.turn_epoch)
                                && let Some(silence) =
                                    state.session_maps.silence_states.get(session_id)
                            {
                                silence
                                    .lock()
                                    .set_declared_background_work(running, turn_epoch);
                            }
                        }
                        "userwrap" => {
                            // From the zsh deferred integration's `ask` branch
                            // (see `shell_integration.rs`'s module doc comment):
                            // the shell found the user's own `claude`/`codex`/
                            // `goose` function with no recorded decision for its
                            // fingerprint. Payload `<agent>:<fingerprint>`.
                            // Terminal output is untrusted (any program can print
                            // an OSC sequence), so both halves are validated
                            // (agent allow-list, digits-and-dash fingerprint)
                            // before use. Even a forged verb can only open the
                            // consent dialog — wrapping still needs the user's
                            // explicit answer.
                            if let Some((agent, fingerprint)) = payload.split_once(':')
                                && crate::agent_wrap_prompt::is_wrappable_agent(agent)
                                && crate::shell_integration::is_user_function_fingerprint(
                                    fingerprint,
                                )
                            {
                                crate::agent_wrap_prompt::request(state, agent, fingerprint);
                            }
                        }
                        // `ccsession`/`cwd`/`transcript`/`tool`/`notify`: free-text
                        // metadata `tuic-hook` extracted natively from a Claude Code
                        // hook's stdin JSON (SessionStart/Pre/PostToolUse/
                        // Notification). Percent-encoded on the wire since these
                        // carry arbitrary text (paths, tool names, messages) that
                        // could otherwise contain the OSC param delimiter (`;`) or
                        // control bytes; decode once here and forward as a generic
                        // `AgentMetadata` event for the frontend to pick up as
                        // features consume it (none do yet — see `output_parser.rs`).
                        "ccsession" => tuic_events.push(ParsedEvent::AgentMetadata {
                            field: "session_id".to_string(),
                            value: percent_decode_osc_payload(&payload),
                        }),
                        "cwd" => tuic_events.push(ParsedEvent::AgentMetadata {
                            field: "cwd".to_string(),
                            value: percent_decode_osc_payload(&payload),
                        }),
                        "transcript" => tuic_events.push(ParsedEvent::AgentMetadata {
                            field: "transcript_path".to_string(),
                            value: percent_decode_osc_payload(&payload),
                        }),
                        "tool" => tuic_events.push(ParsedEvent::AgentMetadata {
                            field: "tool_name".to_string(),
                            value: percent_decode_osc_payload(&payload),
                        }),
                        "notify" => {
                            let decoded = percent_decode_osc_payload(&payload);
                            // Stashed for the "state" arm's very next iteration —
                            // see `pending_notify_message`'s declaration above.
                            pending_notify_message = Some(decoded.clone());
                            tuic_events.push(ParsedEvent::AgentMetadata {
                                field: "message".to_string(),
                                value: decoded,
                            });
                        }
                        "notifytype" => {
                            let decoded = percent_decode_osc_payload(&payload);
                            // Stashed for the "state" arm's very next iteration —
                            // see `pending_notification_type`'s declaration above.
                            pending_notification_type = Some(decoded.clone());
                            tuic_events.push(ParsedEvent::AgentMetadata {
                                field: "notification_type".to_string(),
                                value: decoded,
                            });
                        }
                        _ => {}
                    },
                    TermEvent::MouseCursorDirty | TermEvent::CursorBlinkingChange => {}
                    TermEvent::ImagePlacement(info) => {
                        #[cfg(feature = "desktop")]
                        if let Some(a) = state.app_handle.read().as_ref() {
                            let _ = a.emit(
                                &format!("pty-image-placement-{session_id}"),
                                serde_json::json!({
                                    "placementId": info.placement_id,
                                    "imageId": info.image_id,
                                    "absRow": info.abs_row,
                                    "col": info.col,
                                    "rows": info.rows,
                                    "cols": info.cols,
                                    "zIndex": info.z_index,
                                }),
                            );
                        }
                        state.emit_pty_event(crate::state::AppEvent::PtyImagePlacement {
                            session_id: session_id.to_string(),
                            placement_id: info.placement_id,
                            image_id: info.image_id,
                            abs_row: info.abs_row,
                            col: info.col,
                            rows: info.rows,
                            cols: info.cols,
                            z_index: info.z_index,
                        });
                    }
                    TermEvent::ImagePlacementsCleared => {
                        #[cfg(feature = "desktop")]
                        if let Some(a) = state.app_handle.read().as_ref() {
                            let _ = a.emit(
                                &format!("pty-image-placements-cleared-{session_id}"),
                                serde_json::json!({}),
                            );
                        }
                        state.emit_pty_event(crate::state::AppEvent::PtyImagePlacementsCleared {
                            session_id: session_id.to_string(),
                        });
                    }
                }
            }
        }

        // Write to ring buffer and broadcast to WebSocket clients while
        // holding the ring lock. Serializing these two steps prevents a race
        // with WS catch-up: a newly-connecting handler that also takes
        // ring.lock() for its snapshot cannot observe a state where the byte
        // is in the ring but also still queued for live delivery, which
        // would cause the catch-up and the live stream to replay the same
        // bytes to the client.
        let mut output_offset_after_chunk = 0;
        if let Some(ring) = state.session_maps.output_buffers.get(session_id) {
            let mut ring_guard = ring.lock();
            ring_guard.write(data.as_bytes());
            output_offset_after_chunk = ring_guard.total_written;
            crate::state::broadcast_to_ws_clients(&state.ws_clients, session_id, data);
            drop(ring_guard);
        }

        // Parse events: OSC 9;4 progress from raw stream, others from clean rows.
        // One critical section for every flag this chunk reads out of
        // SilenceState — `hook_state_seen` used to take the lock a second time
        // a few lines below, for a single bool.
        let (in_resize_grace, in_startup_grace, parser_dedup_reset, hook_state_seen) = {
            let mut sl = silence.lock();
            (
                sl.is_resize_grace(),
                sl.is_startup_grace(),
                sl.take_parser_dedup_reset(),
                sl.hook_state_seen,
            )
        };
        // A line was submitted since the last chunk: the same API error or
        // session conflict recurring now is a new failure, not a repaint.
        if parser_dedup_reset {
            self.parser.reset_input_dedup();
        }
        let suppress_notifications = in_resize_grace || in_startup_grace;
        let mut events = tuic_events;
        // Hook-instrumented sessions get awaiting from OSC 7770; drop heuristic
        // (regex) Question events from the parser so they don't double-fire.
        let hook_instrumented = state
            .session_maps
            .session_states
            .get(session_id)
            .map(|s| s.hook_instrumented)
            .unwrap_or(false)
            && hook_state_seen;
        // Capture tap: off by default, one relaxed atomic load when it is.
        // Recorded before any parsing so a fixture replays exactly the bytes
        // the detectors saw, chunk boundaries included — those boundaries are
        // themselves a failure mode (a split OSC matches nothing).
        if crate::pty_capture::is_enabled() {
            let capture_geometry = state.grid.vt_log_buffers.get(session_id).map(|vt| {
                let vt = vt.lock();
                (vt.grid_screen_lines() as u16, vt.grid_columns() as u16)
            });
            // Elided for images (color-tools plan, Phase 8) — a `.tcap` capture
            // caps at 512 KB total, and a single image transmission would
            // otherwise blow that cap outright. `.tcap` is a pure debugging
            // artifact (src-tauri/AGENTS.md's "capture before you theorise"), unlike
            // `output_buffers`/`broadcast_to_ws_clients` just above, which is a
            // real client's live stream and reconnect-replay source and must
            // never be touched here.
            let elided_for_capture =
                crate::image_payload_elision::elide_image_payloads(data.as_bytes());
            crate::pty_capture::record_with_geometry(
                session_id,
                elided_for_capture.as_deref().unwrap_or(data.as_bytes()),
                capture_geometry,
            );
        }

        raw_stream_events(&mut self.raw_carry, data, &mut events);
        let agent_active_for_parse = state
            .session_maps
            .session_states
            .get(session_id)
            .map(|s| s.agent_type.is_some())
            .unwrap_or(false);
        let mut breaks = IntentBreaks::default();
        if agent_active_for_parse
            && (intent_candidate.is_some() || silence.lock().open_intent.is_some())
        {
            self.intent_break_parser
                .advance(&mut breaks, data.as_bytes());
        }
        let mut intent_events = Vec::new();
        if agent_active_for_parse {
            let candidate = intent_candidate.and_then(|(line, anchor_text)| {
                let ParsedEvent::Intent { text, title } =
                    crate::output_parser::parse_intent(&line.text, true)?
                else {
                    return None;
                };
                Some((line, anchor_text, text, title))
            });
            let mut sl = silence.lock();
            let mut candidate_grew = false;
            let mut same_anchor_repaint = false;
            if let Some((line, anchor_text, text, title)) = candidate {
                candidate_grew = sl.open_intent.as_ref().map_or(!breaks.strong, |open| {
                    text.starts_with(&open.text) && text.len() > open.text.len()
                });
                same_anchor_repaint = sl
                    .open_intent
                    .as_ref()
                    .is_some_and(|open| open.anchor_text == anchor_text);
                let compatible = sl.open_intent.as_ref().is_some_and(|open| {
                    text.starts_with(&open.text)
                        || open.text.starts_with(&text)
                        || same_anchor_repaint
                });
                if sl.open_intent.is_some()
                    && !compatible
                    && let Some(event) = sl.close_open_intent()
                {
                    intent_events.push(event);
                }
                // Ink can erase the continuation row, briefly paint the next
                // paragraph there, then move the intact anchor up one row and
                // finish its title. Keep the longer candidate during that gap.
                if let Some(title) = title {
                    sl.open_intent = None;
                    if let Some(event) = sl.accept_intent(text, Some(title)) {
                        intent_events.push(event);
                    }
                } else if sl.last_intent.as_ref() != Some(&(text.clone(), None))
                    && !(same_anchor_repaint
                        && sl
                            .open_intent
                            .as_ref()
                            .is_some_and(|open| !text.starts_with(&open.text)))
                {
                    let start_row = if same_anchor_repaint {
                        sl.open_intent
                            .as_ref()
                            .map_or(intent_origin + line.start_row, |open| open.start_row)
                    } else {
                        intent_origin + line.start_row
                    };
                    sl.open_intent = Some(OpenIntent {
                        text,
                        anchor_text,
                        start_row,
                        end_row: start_row + line.end_row.saturating_sub(line.start_row),
                    });
                }
            }
            let close = sl.open_intent.as_ref().is_some_and(|open| {
                let end_row = open.end_row.saturating_sub(intent_origin);
                let prose_below = !candidate_grew
                    && !same_anchor_repaint
                    && changed_rows.iter().any(|row| {
                        row.row_index > end_row
                            && !row.text.trim().is_empty()
                            && !is_chrome_row(&row.text)
                            && crate::output_parser::structured_token_anchor(&row.text).is_none()
                    });
                let broken_line = breaks.any
                    && !candidate_grew
                    && !same_anchor_repaint
                    && cursor_row.is_some_and(|row| row > end_row);
                let replaced = !same_anchor_repaint
                    && changed_rows.iter().any(|row| {
                        intent_origin + row.row_index == open.start_row
                            && crate::output_parser::structured_token_anchor(&row.text)
                                != Some(crate::output_parser::StructuredTokenAnchor::Intent)
                    });
                (prose_below || broken_line || replaced) && !incomplete_intent_title(&open.text)
                    || explicit_idle_in_chunk
            });
            if close && let Some(event) = sl.close_open_intent() {
                intent_events.push(event);
            }
        }
        // Cursor-completeness guard: parse a suggest token from the bounded grid
        // prefix through the cursor, never from stale cells to its right. When a
        // soft-wrapped continuation changes in a later chunk, replace its whole
        // physical range with one synthetic logical row so the unchanged anchor
        // remains available to the existing parser. Intent capture is handled
        // by the open state above.
        let mut structured_rows = None;
        let structured_prefix = logical_prefix
            .filter(|prefix| crate::output_parser::structured_token_anchor(&prefix.text).is_some())
            .or_else(|| {
                physical_prefix.filter(|prefix| {
                    self.parser
                        .is_complete_suggest(&prefix.text, agent_active_for_parse)
                })
            });
        if let Some(prefix) = structured_prefix {
            let intersects = changed_rows
                .iter()
                .any(|row| (prefix.start_row..=prefix.end_row).contains(&row.row_index));
            if intersects
                && let Some(anchor) = crate::output_parser::structured_token_anchor(&prefix.text)
            {
                let complete_suggest = anchor
                    == crate::output_parser::StructuredTokenAnchor::Suggest
                    && self
                        .parser
                        .is_complete_suggest(&prefix.text, agent_active_for_parse);
                let mut rows: Vec<_> = changed_rows
                    .iter()
                    .filter(|row| !(prefix.start_row..=prefix.end_row).contains(&row.row_index))
                    .cloned()
                    .collect();
                if complete_suggest {
                    rows.push(crate::state::ChangedRow {
                        row_index: prefix.start_row,
                        text: prefix.text,
                    });
                    rows.sort_by_key(|row| row.row_index);
                }
                structured_rows = Some(rows);
            }
        } else if let Some(cursor_row) = cursor_row
            && changed_rows.iter().any(|row| {
                row.row_index == cursor_row
                    && crate::output_parser::structured_token_anchor(&row.text).is_some()
            })
        {
            structured_rows = Some(
                changed_rows
                    .iter()
                    .filter(|row| row.row_index != cursor_row)
                    .cloned()
                    .collect(),
            );
        }
        let rows = structured_rows.as_deref().unwrap_or(&changed_rows);
        // Claude redraws earlier responses while submitting the next composer.
        // Only suggestions below the latest submitted prompt belong to this
        // turn. Exclude the live input box so an unsent draft is not a boundary.
        let suggest_floor = if agent_type.as_deref() == Some("claude") && screen_present {
            let refs: Vec<&str> = screen_buf.iter().map(String::as_str).collect();
            let end = crate::chrome::find_agent_chrome_cutoff(&refs, Some("claude"))
                .unwrap_or(refs.len());
            refs[..end].iter().rposition(|row| {
                row.strip_prefix('❯')
                    .is_some_and(|draft| !draft.trim().is_empty())
            })
        } else {
            None
        };
        events.extend(
            self.parser
                .parse_clean_lines_with_suggest_floor(rows, agent_active_for_parse, suggest_floor)
                .into_iter()
                .filter(|e| !suppress_heuristic_question(hook_instrumented, e)),
        );
        events.extend(intent_events);

        // Heuristic agent-block detection for Claude Code tool calls — the
        // fallback source for sessions without hook instrumentation. Once a
        // session has ever received an OSC 7770 `state=` event, the
        // idle↔busy-edge turn-level source (handle_tuic_state) is
        // authoritative and this is suppressed so the two can't produce
        // conflicting blocks.
        if state
            .session_maps
            .has_tuic_state_integration
            .contains_key(session_id)
        {
            // Suppression can activate mid-turn (e.g. the hook installs a
            // beat after a `⏺` header already opened a heuristic block, since
            // UserPromptSubmit's busy event is the common but not only path
            // to setting this flag). Without this, that block would stay
            // open forever — never folded, no exit code, no scrollbar tick —
            // since the heuristic that alone can close it never runs again.
            // Close it now, at the current cursor position, then let the
            // primary source take over for everything after.
            if let Some(prev) = self.last_agent_block_line.take() {
                let close_line = total_scrolled + cursor_row.map_or(0, |r| r + 1);
                events.push(ParsedEvent::AgentBlock {
                    action: "end".into(),
                    line: close_line.max(prev + 1) as i64,
                    exit_code: None,
                    prompt_text: None,
                    on_alt_screen,
                    from_transcript_dump: false,
                    new_dump_generation: false,
                });
            }
        } else {
            let teardown_end_line = total_scrolled + cursor_row.map_or(0, |r| r + 1);
            events.extend(synthesize_cc_block_events(
                &changed_rows,
                total_scrolled,
                agent_active_for_parse,
                teardown_end_line,
                on_alt_screen,
                &mut self.last_agent_block_line,
            ));
        }

        // A Claude Code fullscreen transcript-mode `[` dump — independent of
        // (and never conflicting with) the two sources above: it never fires
        // during a live hook-driven or heuristic turn, only when the user
        // explicitly bridges the fullscreen conversation into real primary
        // scrollback. See `synthesize_transcript_dump_block_events`'s doc
        // comment.
        //
        // Recorded unconditionally (not just inside the `!on_alt_screen`
        // branch below) — a visit back to the alternate screen is exactly
        // the signal `is_new_generation` needs, and it can only be observed
        // on the chunk where it actually happens.
        if on_alt_screen {
            self.dump_saw_alt_screen = true;
        }
        if !on_alt_screen {
            let dump_teardown_end_line = total_scrolled + cursor_row.map_or(0, |r| r + 1);
            let is_new_dump_generation = std::mem::take(&mut self.dump_saw_alt_screen);
            events.extend(synthesize_transcript_dump_block_events(
                &changed_rows,
                total_scrolled,
                agent_type.as_deref(),
                hook_instrumented,
                dump_teardown_end_line,
                &mut self.last_dump_block_line,
                is_new_dump_generation,
            ));
        }

        // The snapshot was refilled once inside the vt_log lock scope above and
        // is handed to every consumer below as one borrowed slice.
        let screen_cache: Option<&[String]> = screen_present.then_some(screen_buf.as_slice());

        // Slash menu detection — use full screen rows (not chrome-trimmed).
        // Claude Code v2.1+ renders autocomplete items BELOW the prompt chrome,
        // so trimming to above-chrome would discard the menu. parse_slash_menu
        // scans bottom-up, skips empty rows, and stops at the first non-matching
        // row (separator/chrome), so it safely finds items regardless of position.
        let slash_on = state
            .session_maps
            .slash_mode
            .get(session_id)
            .is_some_and(|v| v.load(std::sync::atomic::Ordering::Relaxed));
        if let Some(screen) = screen_cache {
            let menu = slash_on
                .then(|| crate::output_parser::parse_slash_menu(screen))
                .flatten();
            if let Some(event) = menu {
                self.slash_menu_visible = true;
                events.push(event);
            } else if self.slash_menu_visible {
                // Exact commands and zero matches close completion without a
                // submitted line. Retract once, not on every busy output chunk.
                self.slash_menu_visible = false;
                events.push(ParsedEvent::SlashMenu { items: Vec::new() });
            }
        }

        // ChoicePrompt detection — numbered confirmation dialogs rendered below
        // the prompt line (edit-confirm, bash-confirm, apply-patch). Runs on
        // every chunk (unlike slash_menu which is gated by slash_mode) because
        // these dialogs appear asynchronously when the agent requests input.
        // Parser uses a strict shape (title with ?/verb + ≥2 numbered options)
        // so false-positive cost is low. Dedup via last_choice_prompt_sig
        // guards against repaint re-emission.
        if events
            .iter()
            .any(|event| matches!(event, ParsedEvent::UserInput { .. }))
        {
            self.choice_prompt_needs_repaint = true;
        }
        if let Some(screen) = screen_cache {
            let choice = if agent_type.as_deref() == Some("claude") {
                crate::output_parser::parse_claude_ask_user_question(screen)
                    .or_else(|| crate::output_parser::parse_choice_prompt(screen))
            } else if agent_type.as_deref() == Some("ego") {
                let rows = ego_question_rows.as_deref().unwrap_or(screen);
                crate::output_parser::parse_ego_question(rows)
                    .or_else(|| crate::output_parser::parse_ego_approval(rows))
            } else if agent_type.as_deref() == Some("grok") {
                crate::output_parser::parse_grok_question(screen)
                    .or_else(|| crate::output_parser::parse_choice_prompt(screen))
            } else {
                crate::output_parser::parse_choice_prompt(screen)
            };
            // A native radio form can switch to free text without leaving the
            // question. Retire old buttons before publishing the text-only wait.
            if matches!(choice, Some(ParsedEvent::Question { .. }))
                && self.last_choice_prompt_sig.take().is_some()
            {
                events.push(ParsedEvent::ChoiceCleared);
            }
            match choice {
                Some(evt) => events.push(evt),
                // Dialog is no longer on screen — retire its dedup signature so the
                // same dialog is detected again the next time it appears, instead of
                // being swallowed for the rest of the session.
                None => {
                    self.choice_prompt_needs_repaint = false;
                    if self.last_choice_prompt_sig.take().is_some() {
                        events.push(ParsedEvent::ChoiceCleared);
                    }
                    // A ready composer shares the empty `>` shape. Only enrich
                    // an existing confident ego wait, after retiring an old selector.
                    if agent_type.as_deref() == Some("ego") {
                        let confident_wait = events
                            .iter()
                            .rev()
                            .find_map(|event| match event {
                                ParsedEvent::Question { confident, .. } => Some(*confident),
                                ParsedEvent::UserInput { .. } => Some(false),
                                _ => None,
                            })
                            .unwrap_or_else(|| {
                                state
                                    .session_maps
                                    .session_states
                                    .get(session_id)
                                    .is_some_and(|session| {
                                        session.awaiting_input
                                            && session.question_confident
                                            && session.choice_prompt.is_none()
                                    })
                            });
                        if confident_wait
                            && let Some(question) =
                                crate::output_parser::parse_ego_free_text_question(screen)
                        {
                            events.push(question);
                        }
                    }
                }
            }
        }

        // Retire the question dedup as soon as its prompt leaves the screen. The
        // marker exists only to stop an Ink menu repaint from re-notifying while
        // the SAME prompt is still displayed; it used to live for the session's
        // lifetime, and since every Ink footer is the byte-identical
        // "Enter to select · ↑/↓ to navigate · Esc to cancel", the first menu of a
        // session permanently swallowed every later one — the awaiting badge was a
        // one-shot per session. Screen absence is the real end-of-prompt signal:
        // the user answering, the agent withdrawing the prompt, and a repaint that
        // scrolls it away all collapse into it.
        if let Some(screen) = screen_cache {
            let prompt_gone = self
                .last_question_text
                .as_deref()
                .is_some_and(|last| !screen.iter().any(|row| row.contains(last)));
            if prompt_gone {
                self.last_question_text = None;
            }
        }

        // Re-arm awaiting while an Ink dialog is still on screen.
        //
        // `parse_clean_lines` only sees CHANGED rows, and the footer row is
        // byte-identical across the sub-questions of a multi-question
        // AskUserQuestion ("⊠ CLI.md · □ Exit codes · ✓ Submit"). Answering the
        // first sub-question clears awaiting; the second one repaints its title
        // and options but NOT the footer, so nothing ever set it again and the tab
        // read "working" while the agent sat blocked on the user.
        //
        // Presence of the footer is the whole signal — no title, option or tab-bar
        // parsing, none of which survives the wizard advancing. It re-arms only
        // when the badge is actually off, so a repaint cannot storm: one event per
        // spurious clear, never one per frame. This applies to hooked sessions
        // too: later busy hooks can clear awaiting while the dialog stays open,
        // and a multi-question tool emits its awaiting hook only once.
        // A returned shell can retain the agent's footer above its prompt. Only
        // a live agent identity makes those retained rows an open agent dialog.
        if agent_type.is_some()
            && let Some(screen) = screen_cache
        {
            let (awaiting, has_choice) = state
                .session_maps
                .session_states
                .get(session_id)
                .map(|s| (s.awaiting_input, s.choice_prompt.is_some()))
                .unwrap_or((false, false));
            // The accumulator has not seen this chunk yet. Respect its LAST
            // awaiting mutation: an earlier Question or ChoicePrompt followed
            // by hook-busy's UserInput no longer protects the badge from being cleared.
            let pending_awaiting = events.iter().rev().find_map(|event| match event {
                ParsedEvent::Question { .. } | ParsedEvent::ChoicePrompt { .. } => Some(true),
                ParsedEvent::UserInput { .. } | ParsedEvent::ChoiceCleared => Some(false),
                _ => None,
            });
            if let Some(evt) = rearm_awaiting_for_open_dialog(
                screen,
                pending_awaiting.unwrap_or(awaiting),
                has_choice,
                pending_awaiting == Some(true),
            ) {
                // Clear the dedup: the badge is off, so this event must reach state.
                self.last_question_text = None;
                events.push(evt);
            }
        }

        let regex_found_question = if suppress_notifications {
            false
        } else {
            events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. }))
        };

        // Read the turn epoch once so every event in this chunk is attributed to
        // the same turn, and per-turn dedup cannot straddle a boundary mid-chunk.
        let turn_epoch = state
            .session_maps
            .session_states
            .get(session_id)
            .map(|session| session.turn_epoch)
            .unwrap_or(0);

        // Emit events with dedup, grace filtering, and PlanFile resolution.
        for event in &events {
            // During startup/resize grace, suppress low-confidence notifications to
            // avoid boot-noise false positives — but let CONFIDENT questions through.
            // An agent can signal an approval prompt via its "Action Required" title
            // (confident), yet its continuous animation keeps resetting last_output,
            // so the startup grace never settles by silence and would otherwise
            // suppress the approval prompt for the full 120s safety cap.
            let suppress_this = suppress_notifications
                && match event {
                    ParsedEvent::Question { confident, .. } => !*confident,
                    ParsedEvent::RateLimit { .. } | ParsedEvent::ApiError { .. } => true,
                    _ => false,
                };
            if suppress_this {
                continue;
            }

            // Count what the model actually emitted, at the funnel every parsed
            // event passes through. Counted here rather than at emission because a
            // suggest parked for a turn that ends early is still a marker the
            // agent produced (#4421).
            match event {
                ParsedEvent::Intent { .. } => {
                    publish_intent_event(state, session_id, event, turn_epoch);
                    continue;
                }
                ParsedEvent::Suggest { .. } => {
                    state.note_marker(session_id, crate::state::MarkerKind::Suggest)
                }
                // Fallback-tier red-tick signal, alongside ToolError: flags the
                // currently-open turn-level block. No recovery-awareness here (unlike
                // ToolError's silence-timer gate) — a self-recovered API retry loop
                // still flags the block, a minor documented over-flagging risk.
                // Already excluded above during startup grace via `suppress_this`.
                ParsedEvent::ApiError { .. } => {
                    state
                        .session_maps
                        .turn_error_flags
                        .insert(session_id.to_string(), ());
                }
                _ => {}
            }

            if let ParsedEvent::AgentSessionConflict { kind, .. } = event
                && matches!(
                    self.terminal_mode,
                    crate::ai_agent::tui_detect::TerminalMode::Shell
                )
            {
                self.mark_session_no_inject(kind);
                continue;
            }

            // Suggest: park in SilenceState and defer emission until silence
            // confirms the turn has ended. The frontend used to buffer these
            // events in `pendingSuggest` to compensate for suggest arriving
            // before `shell-state: idle`; gating the emission backend-side
            // removes the race and simplifies the Terminal event handler.
            if let ParsedEvent::Suggest { items } = event {
                let mut silence_state = silence.lock();
                silence_state.mark_suggest_candidate(items.clone(), turn_epoch);
                continue;
            }

            // Dedup status-line: skip only a repeat within the same turn.
            if let ParsedEvent::StatusLine { task_name, .. } = event {
                let seen = (turn_epoch, task_name.clone());
                if self.last_status_task.as_ref() == Some(&seen) {
                    continue;
                }
                self.last_status_task = Some(seen);
            }

            // Dedup question: skip if same prompt_text already emitted. Retired as
            // soon as the prompt leaves the screen (see the screen-absence reset
            // above), so this guards one pending prompt, not the whole session.
            //
            // Only applies to a NON-EMPTY prompt_text. `tuic_state_awaiting_event`'s
            // hook-based `state=awaiting` mapping always carries an empty prompt_text
            // (it has no real question text to offer) — keying the dedup on that
            // empty placeholder made every hook-based AskUserQuestion after the
            // FIRST one in a session look like a repeat of it, and the screen-
            // absence reset below can never retire an empty string (every row
            // trivially "contains" ""), so the empty placeholder stuck forever and
            // silently swallowed every later AskUserQuestion's awaiting signal.
            // Confirmed via a real two-`AskUserQuestion` capture
            // (`claude_double_askuserquestion_second_missed.tcap`): the raw OSC 7770
            // stream carries `state=awaiting` for BOTH questions, but only the first
            // survived this dedup. A discrete hook firing doesn't need repaint
            // suppression the way a screen-scraped heuristic question does — it
            // fires once per real `PreToolUse(AskUserQuestion)`, not once per
            // spinner tick — so skipping the dedup for it is safe. This also
            // covers ego's empty title, a native awaiting marker rather than
            // screen content: each marker establishes a wait independently of
            // the previous dialog's presence, shape or repaint bookkeeping.
            if let ParsedEvent::Question { prompt_text, .. } = event
                && !prompt_text.is_empty()
            {
                if self.last_question_text.as_deref() == Some(prompt_text.as_str()) {
                    continue;
                }
                self.last_question_text = Some(prompt_text.clone());
            }

            // Dedup choice-prompt: skip if same (title + option keys) already emitted.
            // Signature keeps option order and checkbox labels, but ignores highlighted drift so cursor
            // movement within the dialog doesn't re-fire. Retired when the dialog
            // leaves the screen (see the parse site above).
            if let ParsedEvent::ChoicePrompt {
                title,
                options,
                selection_mode,
                ..
            } = event
            {
                let mut sig = format!(
                    "{}|{}",
                    title,
                    options
                        .iter()
                        .map(|o| o.key.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                );
                if *selection_mode == Some(crate::output_parser::ChoiceSelectionMode::ToggleEnter) {
                    sig.push('|');
                    sig.push_str(
                        &options
                            .iter()
                            .map(|o| o.label.as_str())
                            .collect::<Vec<_>>()
                            .join("|"),
                    );
                }
                let dialog_row_repainted = choice_changed_rows.iter().any(|&row_index| {
                    screen_cache
                        .and_then(|screen| screen.get(row_index))
                        .is_some_and(|row| {
                            row.contains(title)
                                || options.iter().any(|option| row.contains(&option.label))
                        })
                });
                // Ego emits awaiting for each native question. After busy has
                // retired an answer, scrolling its retained rows is not a new
                // wait. A fresh protocol question can still restore the same
                // signature; other agents retain their screen-only recovery.
                let repaint_can_rearm = agent_type.as_deref() != Some("ego")
                    || events
                        .iter()
                        .rev()
                        .find_map(|event| match event {
                            ParsedEvent::Question { confident, .. } => Some(*confident),
                            ParsedEvent::UserInput { .. } => Some(false),
                            _ => None,
                        })
                        .unwrap_or_else(|| {
                            silence.lock().awaiting_rank() == Some(EvidenceRank::Protocol)
                        });
                if self.last_choice_prompt_sig.as_deref() == Some(sig.as_str())
                    && !(self.choice_prompt_needs_repaint
                        && dialog_row_repainted
                        && repaint_can_rearm)
                {
                    continue;
                }
                self.last_choice_prompt_sig = Some(sig);
                self.choice_prompt_needs_repaint = false;
            }

            // Resolve relative plan-file paths to absolute using session CWD.
            // If the file doesn't exist yet (agent announces before writing),
            // queue it for retry — checked each chunk for up to 10 seconds.
            let resolved = if let ParsedEvent::PlanFile { path } = event {
                match self.resolve_planfile_path(path) {
                    Some(p)
                        if self.emitted_planfiles.contains(&p)
                            || self.gaveup_planfiles.contains(&p) =>
                    {
                        // Already emitted, or it exhausted its retry window — skip
                        // (spinner redraws re-parse the same on-screen line).
                        continue;
                    }
                    Some(p) if std::path::Path::new(&p).is_file() => {
                        tracing::info!("[plan-file] Detected: {p} (cwd={:?})", self.session_cwd);
                        self.emitted_planfiles.insert(p.clone());
                        Some(ParsedEvent::PlanFile { path: p })
                    }
                    Some(p) => {
                        // File not on disk yet — queue for retry if not already pending
                        if !self.pending_planfiles.iter().any(|(pp, _)| pp == &p) {
                            tracing::debug!(
                                "[plan-file] Queued for retry: {p} (cwd={:?})",
                                self.session_cwd
                            );
                            let deadline =
                                std::time::Instant::now() + std::time::Duration::from_secs(10);
                            self.pending_planfiles.push((p, deadline));
                        }
                        continue;
                    }
                    None => {
                        tracing::warn!(
                            "[plan-file] Cannot resolve relative path: {path} (cwd={:?})",
                            self.session_cwd
                        );
                        continue;
                    }
                }
            } else {
                None
            };

            let emit_event = resolved.as_ref().unwrap_or(event);

            // Serialize once, reuse for both broadcast and Tauri IPC
            if let Ok(mut json) = serde_json::to_value(emit_event) {
                if let Some(object) = json.as_object_mut() {
                    object.insert("_turn_epoch".to_string(), turn_epoch.into());
                }
                #[cfg(feature = "desktop")]
                if let Some(app) = state.app_handle.read().as_ref() {
                    let _ = app.emit(&format!("pty-parsed-{session_id}"), &json);
                }
                state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                    session_id: session_id.to_string(),
                    parsed: json.into(),
                });
            }
        }

        // Codex can cancel an approval with Esc without emitting a typed line
        // or protocol busy marker. The title has left Action Required and the
        // recorded screen shows the cancellation above its ready composer.
        // Carry the originating prompt so a later, different question cannot
        // be cleared. Require a newly painted cancellation as well: old ones
        // remain in the transcript when another approval opens.
        if agent_type.as_deref() == Some("codex")
            && self.codex_approval_question.is_some()
            && changed_rows
                .iter()
                .any(|row| row.text.contains("You canceled the request"))
        {
            self.codex_approval_canceled = true;
        }
        let canceled_codex_approval = if agent_type.as_deref() == Some("codex")
            && !self.title_awaiting
            && self.codex_approval_canceled
            && matches!(
                screen_activity,
                AgentScreenActivity::Ready | AgentScreenActivity::Interrupted
            )
            && screen_cache.is_some_and(|screen| {
                screen
                    .iter()
                    .any(|row| row.contains("You canceled the request"))
            })
            && !events.iter().any(|event| {
                matches!(
                    event,
                    ParsedEvent::Question { .. } | ParsedEvent::ChoicePrompt { .. }
                )
            }) {
            self.codex_approval_question
                .as_deref()
                .and_then(|expected| {
                    state
                        .session_maps
                        .session_states
                        .get(session_id)
                        .and_then(|session| {
                            (session.awaiting_input
                                && session.question_confident
                                && session.question_text.as_deref() == Some(expected))
                            .then(|| (expected.to_string(), session.turn_epoch))
                        })
                })
        } else {
            None
        };
        if let Some((expected_question_text, turn_epoch)) = canceled_codex_approval {
            self.codex_approval_question = None;
            self.codex_approval_canceled = false;
            state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                session_id: session_id.to_string(),
                parsed: serde_json::json!({
                    "type": "protocol-question-cleared",
                    "expected_question_text": expected_question_text,
                    "_turn_epoch": turn_epoch,
                })
                .into(),
            });
        }

        // Claude prints this tool result when Esc dismisses AskUserQuestion.
        // A bare Esc has no line-input event, so its confident notification
        // otherwise stays latched after the completed turn. Require the newly
        // painted result and a ready composer; a dialog still on screen must
        // retain its badge, including while Claude repaints its status line.
        let declined_screen = agent_type.as_deref() == Some("claude")
            && screen_activity == AgentScreenActivity::Ready
            && changed_rows.iter().any(|row| {
                row.text.contains("User declined") && row.text.contains("answer questions")
            })
            && screen_cache
                .is_some_and(|screen| crate::output_parser::ink_dialog_footer(screen).is_none())
            && !events.iter().any(|event| {
                matches!(
                    event,
                    ParsedEvent::Question { .. } | ParsedEvent::ChoicePrompt { .. }
                )
            });
        let declined_session = declined_screen
            .then(|| state.session_maps.session_states.get(session_id))
            .flatten()
            .filter(|session| session.awaiting_input && session.question_confident)
            .map(|session| {
                // A live choice overlay owns its own clear; only the shell
                // state below is ours then.
                let question = if session.choice_prompt.is_none() {
                    session.question_text.clone()
                } else {
                    None
                };
                (question, session.turn_epoch)
            });
        if let Some((question, turn_epoch)) = declined_session {
            if let Some(expected_question_text) = question {
                state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                    session_id: session_id.to_string(),
                    parsed: serde_json::json!({
                        "type": "protocol-question-cleared",
                        "expected_question_text": expected_question_text,
                        "_turn_epoch": turn_epoch,
                    })
                    .into(),
                });
            }
            // Esc ends the turn without a Stop hook (live capture: busy, busy,
            // awaiting, then nothing), so the hook-driven BUSY would stay latched
            // with the queue stuck until the next input. The ready composer under
            // the fresh cancellation is the Stop it never sent.
            transition_explicit_shell_state(state, session_id, SHELL_IDLE, "idle", true);
        }

        // Update silence state for fallback question detection.
        let has_status_line = events
            .iter()
            .any(|e| matches!(e, ParsedEvent::StatusLine { .. }));
        let last_q_line = extract_question_line(&changed_rows);
        // A chunk is chrome-only when no real output reached the screen.
        // Path 0: changed_rows is empty — nothing visible happened (cursor
        //   blink, OSC title update, mouse report, SGR-only sequence). Must
        //   count as chrome-only or these periodic re-emits latch the shell
        //   state to busy forever during genuine idle.
        // Path 1: every row has a chrome marker (is_chrome_row).
        // Path 2: parse_status_line detected a spinner pattern (Gemini braille,
        //   Goose status text) AND no row contains real agent output. A row is
        //   "real output" if it is not chrome and not blank — this prevents
        //   has_status_line from suppressing chunks that mix spinner + output.
        let all_chrome_markers = changed_rows.iter().all(|r| is_chrome_row(&r.text));
        let has_suggest = events
            .iter()
            .any(|e| matches!(e, ParsedEvent::Suggest { .. }))
            || rows.iter().any(|row| {
                self.parser
                    .is_complete_suggest(&row.text, agent_active_for_parse)
            });
        let no_real_output = changed_rows.iter().all(|r| {
            is_chrome_row(&r.text)
                || r.text.trim().is_empty()
                || crate::chrome::is_separator_line(&r.text)
                || crate::chrome::is_prompt_line(&r.text)
                // Suggest tokens are protocol markers, not real agent output.
                // Without this, a visible suggest row makes the chunk look like
                // "real output" and increments the question staleness counter.
                || (has_suggest && is_suggest_row(&r.text))
        });
        let chrome_only = !regex_found_question
            && last_q_line.is_none()
            && (changed_rows.is_empty()
                || all_chrome_markers
                || ((has_status_line || has_suggest) && no_real_output));
        // Suggest-only: chunk produced only Suggest events (no real text).
        let suggest_only = has_suggest
            && !regex_found_question
            && last_q_line.is_none()
            && !has_status_line
            && no_real_output;
        // Tool-error detection: scan visible rows for `Error: Exit code N`
        // emitted by Claude Code / Codex at the end of a failing tool call.
        // Fires playError() via silence_timer when followed only by chrome
        // until SILENCE_TOOL_ERROR_THRESHOLD elapses (= turn ended on error).
        //
        // The scan is two regexes per changed row and it used to run INSIDE the
        // SilenceState critical section, holding the lock the silence timer and
        // every sibling reader contend for while it matched. Only its verdict
        // needs the lock.
        let mut error_line: Option<String> = None;
        let mut retry_seen = false;
        for row in changed_rows.iter() {
            if is_retry_line(&row.text) {
                retry_seen = true;
            } else if is_tool_error_line(&row.text) {
                error_line = Some(row.text.trim().to_string());
            }
        }
        {
            let mut sl = silence.lock();
            // Shared with the silence timer (#744-138c): the screen has not
            // changed since this classification unless a later chunk arrives
            // to overwrite it, so the timer reuses this instead of calling
            // `detect_agent_screen_activity` itself — see `cached_screen_activity`.
            sl.record_screen_activity(screen_activity, output_offset_after_chunk);
            sl.on_chunk(
                regex_found_question,
                last_q_line,
                has_status_line,
                chrome_only,
                suggest_only,
            );

            if retry_seen {
                // Agent is auto-retrying a failed API call — hold BUSY across the
                // frozen gap between attempts. Takes precedence over the recovery
                // clear below: the retry line IS real output but is not recovery.
                sl.mark_api_retry();
            } else if let Some(line) = error_line {
                sl.mark_tool_error_candidate(line);
            } else if !chrome_only {
                // Real output without an error/retry line → agent recovered/continued.
                sl.clear_tool_error_on_recovery();
            }
        }

        // Screen activity is evaluated on the full, unfiltered snapshot. The
        // generic chrome cutoff is a presentation/logging boundary and must not
        // erase agent-specific liveness evidence (Codex tool separators are the
        // canonical counterexample).
        // `screen_activity` was classified inside the vt_log lock above, from a
        // borrowed screen — see the note there.
        let working_status_moved = changed_rows
            .iter()
            .any(|row| crate::chrome::is_working_status_row(&row.text));
        let apply_working =
            screen_activity == AgentScreenActivity::Working && !explicit_idle_in_chunk;
        let working_source = if working_status_moved {
            "working-screen-movement"
        } else {
            "working-screen"
        };
        let can_reopen_completed = apply_working && {
            let working_agent_type = state
                .session_maps
                .session_states
                .get(session_id)
                .and_then(|session| session.agent_type.clone());
            working_agent_type.as_deref() == Some("claude")
                || (working_agent_type.as_deref() == Some("codex")
                    && working_source == "working-screen-movement")
        };

        // Stamp last_output_ms for real output and for active spinner repaints.
        // Spinner rows (dingbats ✻, braille ⠋) prove the agent is
        // alive even though they are chrome-only — keeping the timestamp fresh
        // prevents should_transition_idle from firing mid-think.
        //
        // Spinner detection runs on the SAME post-cutoff `changed_rows` as
        // everything else. Real spinners (Gemini braille,
        // Claude `✻ Thinking…`) all render ABOVE the input separator and LEAD
        // their row, so they survive the chrome cutoff and still keep the agent
        // alive here. A status-line HUD's `█░` progress bar or a `·`-bearing
        // footer is NOT a spinner (`is_spinner_row` requires the glyph to lead
        // the line, #446-596f), so it can never keep a session busy even if it
        // renders above the cutoff.
        let has_spinner = chrome_only
            && changed_rows
                .iter()
                .any(|r| crate::chrome::is_spinner_row(&r.text));
        //
        // This, the resize-grace re-arm and the BUSY gate below all read or
        // write the same SilenceState, and each used to take the lock for
        // itself. They are one critical section now; nothing between them
        // touches SilenceState (`stamp_last_output_now` writes an AppState
        // atomic, `begin_suggest_working_turn` the parser), and the ordering
        // inside the section is the ordering the three had.
        let real_activity = (!chrome_only || has_spinner) && !explicit_idle_in_chunk;
        // The working-evidence gate (turn_completed/explicit_idle blocking a
        // stale Working row, unless `can_reopen_completed`) folded in here so
        // this is the ONLY SilenceState lock in the chunk path — previously
        // `apply_working_evidence` took its own separate lock ahead of this
        // one (DEFERRED 2026-09-06). Working evidence and real/spinner
        // activity are both one-shot busy evidence (see the comment on
        // `apply_working_evidence`): recorded to win THIS chunk's CAS via
        // `decide()`, then cleared so they cannot block a later, unrelated
        // idle-evidence recording (e.g. the silence-timeout fallback for a
        // plain shell or an agent with no OSC133/hook integration).
        let (in_resize_grace_after, evidence_snapshot, working_applied, reopened_completion) = {
            let mut sl = silence.lock();
            let mut working_applied = false;
            let mut reopened_completion = false;
            if apply_working {
                let turn_completed = state
                    .session_maps
                    .session_states
                    .get(session_id)
                    .is_some_and(|session| sl.completion_settled_for_epoch(session.turn_epoch));
                let blocked = (turn_completed || sl.explicit_idle()) && !can_reopen_completed;
                if !blocked {
                    let reopen = can_reopen_completed && (turn_completed || sl.explicit_idle());
                    if reopen {
                        // Claude can emit Stop/suggest before a blocking Stop hook
                        // finishes; Codex can start an internal continuation
                        // without a PTY submission. Current semantic movement is
                        // stronger than either stale boundary.
                        sl.reopen_suggest_completion();
                        reopened_completion = true;
                    }
                    sl.note_working_screen();
                    invalidate_background_probe_boundary_locked(state, session_id);
                    let rank = if reopen {
                        EvidenceRank::Protocol
                    } else {
                        EvidenceRank::Screen
                    };
                    sl.record_busy(rank, working_source);
                    working_applied = true;
                }
            }
            let activity_source = if has_spinner {
                "spinner-active"
            } else {
                "real-activity"
            };
            // Arbitrate BEFORE updating activity metadata: those updates clear
            // idle evidence. Doing them first bypasses record_busy's rank gate
            // and lets decorative repaints reopen a hook-completed turn.
            if real_activity
                && (sl.evidence.busy.is_some()
                    || sl.record_busy(EvidenceRank::Screen, activity_source))
            {
                if has_spinner {
                    sl.note_working_screen();
                } else {
                    sl.note_real_activity();
                }
                invalidate_background_probe_boundary_locked(state, session_id);
            }
            // SIGWINCH reflow repaints content rows for longer than the initial 1s
            // resize grace, but a reflow never grows the buffer — it only repaints
            // existing rows. While such pure-repaint chunks keep arriving within the
            // grace window, re-arm the grace so a resize never flips an idle agent to
            // busy. A growing chunk (genuine new output) is NOT extended, so real work
            // started right after a resize still registers as busy. An already-busy
            // session is unaffected (idle transitions are silence-timer only).
            //
            // The extension has no stop condition of its own — each qualifying chunk
            // pushes the deadline a full RESIZE_GRACE forward — so `vt_output_grew`
            // is the ONLY thing that ends it. It must stay a signal that a working
            // agent actually trips; see its definition for why the alternate screen
            // needs its own answer rather than the durable-log total.
            if !vt_output_grew && sl.is_resize_grace() {
                sl.on_resize();
            }
            (
                sl.is_resize_grace(),
                sl.evidence.clone(),
                working_applied,
                reopened_completion,
            )
        };
        if working_applied {
            stamp_last_output_now(state, session_id, now_epoch_ms());
        }
        if reopened_completion
            && let Some(mut session) = state.session_maps.session_states.get_mut(session_id)
        {
            session.suggested_actions = None;
        }
        if real_activity {
            stamp_last_output_now(state, session_id, now_epoch_ms());
        }

        // Suggest dedup is intentionally not reset on submission: the previous
        // marker may repaint while still visible. Once this turn has real
        // working evidence, however, an identical terminal marker is a valid
        // new completion. Update the parser after this chunk was parsed so a
        // stale marker repainted alongside the first activity remains ignored.
        if !explicit_idle_in_chunk
            && (screen_activity == AgentScreenActivity::Working || !chrome_only || has_spinner)
            && let Some(turn_epoch) = state
                .session_maps
                .session_states
                .get(session_id)
                .map(|session| session.turn_epoch)
        {
            self.parser.begin_suggest_working_turn(turn_epoch);
        }

        // Shell state: reader transitions → BUSY on real output OR active spinner.
        // Idle transitions are handled exclusively by the silence timer to
        // eliminate the two-path race that caused 15+ fix/revert cycles.
        // Load `prev` and drop the shell_states Ref before try_shell_transition (which
        // re-gets the same key): holding a Ref across that second get risks the CONC-C
        // re-entrant-read deadlock (story 099-6526).
        //
        // Working-evidence's CAS is unconditional once recorded (matching the
        // old `apply_working_evidence`, which was never gated by resize grace);
        // real/spinner activity's CAS keeps its own resize-grace gate.
        let prev = if working_applied || (real_activity && !in_resize_grace_after) {
            state
                .session_maps
                .shell_states
                .get(session_id)
                .map(|atom| atom.load(std::sync::atomic::Ordering::Acquire))
        } else {
            None
        };
        if let Some(prev) = prev
            && let Some(Transition::ToBusy) = decide(
                &evidence_snapshot,
                prev == SHELL_BUSY,
                std::time::Instant::now(),
            )
            && try_shell_transition(state, session_id, prev, SHELL_BUSY, true)
        {
            emit_shell_state(state, session_id, "busy");
        }
        if working_applied || real_activity {
            // One-shot: this evidence must not persist to block a later,
            // unrelated idle-evidence recording (silence-timeout fallback,
            // ready-screen confirmation) — see the comment above.
            let mut silence = silence.lock();
            if silence.evidence.busy.is_some_and(|busy| {
                busy.source == working_source
                    || busy.source == "spinner-active"
                    || busy.source == "real-activity"
            }) {
                silence.clear_busy_evidence();
            }
        }

        // Update terminal mode in SessionState when it changes.
        // Detect TUI app from visible screen rows while in alternate buffer.
        if self.terminal_mode.is_fullscreen() {
            let row_texts: Vec<&str> = changed_rows.iter().map(|r| r.text.as_str()).collect();
            if let Some(app) = crate::ai_agent::tui_detect::detect_app_from_rows(&row_texts) {
                self.terminal_mode = self.terminal_mode.with_app_hint(app.to_string());
            }
        }
        if let Some(mut entry) = state.session_maps.session_states.get_mut(session_id) {
            let new_mode = if self.terminal_mode.is_fullscreen() {
                Some(self.terminal_mode.clone())
            } else {
                None
            };
            if entry.terminal_mode != new_mode {
                entry.terminal_mode = new_mode;
            }
        }

        self.screen_buf = screen_buf;
        self.queued_idle_flush |= explicit_idle_in_chunk;
        true
    }
}

/// Process kitty keyboard actions (push/pop/query) shared by both reader threads.
fn process_kitty_actions(kitty_actions: &[KittyAction], session_id: &str, state: &AppState) {
    if kitty_actions.is_empty() {
        return;
    }
    let entry = state
        .session_maps
        .kitty_states
        .entry(session_id.to_string())
        .or_insert_with(|| Mutex::new(KittyKeyboardState::new()));
    let mut ks = entry.lock();
    for action in kitty_actions {
        match action {
            KittyAction::Push(flags) => ks.push(*flags),
            KittyAction::Pop => ks.pop(),
            KittyAction::Query => {
                let flags = ks.current_flags();
                let response = format!("\x1b[?{}u", flags);
                write_terminal_reply(state, session_id, response.as_bytes(), "kitty query");
            }
        }
    }
    let flags = ks.current_flags();
    drop(ks);
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        let _ = app.emit(&format!("kitty-keyboard-{session_id}"), flags);
    }
}

/// Whether the session's line discipline would swallow a reply written now.
///
/// **Measured 2026-09-07** (capture `f2bddfb0`, frames 25-35): Claude Code
/// emits `ESC[c` *before* it switches the tty out of cooked mode, so our
/// `ESC[?6c` was painted as the literal text `^[[?6c` at the top of the startup
/// banner and never delivered. Claude, having received nothing, re-queried
/// 100ms later from raw mode and got a clean answer. Withholding the premature
/// reply therefore costs no information: the querier retries once it can read.
///
/// **The predicate is `ICANON`, not `ECHO`, and the difference is load-bearing.**
/// `ECHO` decides whether the bytes are *also* painted on screen; `ICANON`
/// decides whether they are *delivered at all*, because a canonical-mode read
/// blocks until a newline that a terminal reply never contains. In cbreak
/// (`ICANON` off, `ECHO` on) the reply reaches the querier immediately — ugly,
/// but read. Gating on `ECHO` there would withhold a reply nothing else will
/// resend and hang the querier, trading a cosmetic defect for a hang.
///
/// A failure to look the tty up answers "no": the old behaviour was to always
/// write, and a reply we cannot prove is undeliverable is better sent than lost.
#[cfg(not(windows))]
fn tty_would_swallow_reply(state: &AppState, session_id: &str) -> bool {
    let Some(entry) = state.session_maps.sessions.get(session_id) else {
        return false;
    };
    let Some(fd) = entry.value().lock().master.as_raw_fd() else {
        return false;
    };
    let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
    // SAFETY: `fd` is the live PTY master owned by the session we just locked,
    // and `tcgetattr` only writes through the pointer when it returns 0.
    if unsafe { libc::tcgetattr(fd, termios.as_mut_ptr()) } != 0 {
        return false;
    }
    unsafe { termios.assume_init() }.c_lflag & libc::ICANON != 0
}

/// Serialize a terminal-generated protocol reply with every other PTY write.
///
/// The writer has its own mutex, separate from the session metadata. Waiting
/// here is safe: the reader remains able to drain PTY output even when another
/// thread is blocked in a kernel write, so the old session-lock deadlock cannot
/// occur and mandatory replies are never discarded merely due to contention.
///
/// Replies are withheld while the tty is canonical — see
/// [`tty_would_swallow_reply`].
fn write_terminal_reply(state: &AppState, session_id: &str, response: &[u8], kind: &str) {
    #[cfg(not(windows))]
    if tty_would_swallow_reply(state, session_id) {
        tracing::debug!(source = "terminal", session_id = %session_id, %kind,
            "Terminal reply withheld: tty is canonical, the querier cannot read it yet");
        return;
    }
    // Terminal protocol replies are not replacement user input.
    let result = state
        .pty_writer(session_id)
        .ok_or_else(|| "Session not found".to_string())
        .and_then(|writer| {
            let mut writer = writer.lock();
            writer
                .write_all(response)
                .and_then(|()| writer.flush())
                .map_err(|error| format!("Write failed: {error}"))
        });
    if let Err(error) = result {
        tracing::warn!(source = "terminal", session_id = %session_id, %kind, %error,
            "Terminal reply failed");
    }
}

/// Resolve every Kitty decode job queued for `session_id` since the last
/// call (color-tools plan: decode deferred off `vt_log`), writing each
/// outcome's deferred OK/error reply and firing `image-decoded` on success.
///
/// **Every code path that replays bytes through `Term`/the `Handler` impl
/// outside the ordinary `process_chunk` path can queue one of these jobs and
/// must call this itself** — exactly the same invariant src-tauri/AGENTS.md's PtyWrite
/// drain section already documents for `Event::PtyWrite`, now extended to
/// this second, separate (non-`TermEvent`) queue. `flush_sync_timeout_if_needed`/
/// `force_stop_sync_if_buffered` (both call `Processor::stop_sync`, which
/// replays buffered bytes through the same `Term`) are the two production
/// call sites besides `process_chunk` itself — a code review caught the
/// first as a real regression: before deferred decode existed, that flush
/// path decoded a Kitty image inline and got a reply out immediately;
/// without this, a job it queues would sit pending until an unrelated later
/// PTY chunk happens to arrive, or forever if the stalled stream never sends
/// one (exactly the workload — a killed `mpv --vo=kitty` — this whole
/// feature targets). Always call with the handles obtained BEFORE dropping
/// whatever lock guarded the flush, but invoke this function itself only
/// after that lock is dropped.
fn resolve_kitty_decode_jobs(
    state: &AppState,
    session_id: &str,
    image_store: &crate::terminal_image_transmission::KittyImageStoreHandle,
    pending_jobs: &crate::terminal_image_transmission::KittyPendingJobsHandle,
) {
    for outcome in crate::terminal_image_transmission::drain_and_run_pending_kitty_decode_jobs(
        image_store,
        pending_jobs,
    ) {
        if let Some(reply) = crate::terminal_image_transmission::format_kitty_reply(&outcome) {
            write_terminal_reply(state, session_id, reply.as_bytes(), "PtyWrite");
        }
        if outcome.result.is_ok() {
            emit_pty_image_decoded(state, session_id, outcome.image_id);
        }
    }
}

/// Fan out "this Kitty image finished deferred decode" (color-tools plan) —
/// mirrors `TermEvent::ImagePlacement`'s own forwarding exactly (desktop
/// Tauri `emit` + `state.emit_pty_event` for WS/SSE). Free function, not a
/// `ChunkProcessor` method, since `resolve_kitty_decode_jobs` above is
/// called from more than one context (`process_chunk` and the frame
/// ticker's stalled-sync flush).
fn emit_pty_image_decoded(state: &AppState, session_id: &str, image_id: u32) {
    #[cfg(feature = "desktop")]
    if let Some(a) = state.app_handle.read().as_ref() {
        let _ = a.emit(
            &format!("pty-image-decoded-{session_id}"),
            serde_json::json!({ "imageId": image_id }),
        );
    }
    state.emit_pty_event(crate::state::AppEvent::PtyImageDecoded {
        session_id: session_id.to_string(),
        image_id,
    });
}

/// Flush remaining bytes at EOF and write to ring buffer + WebSocket.
/// Returns the flushed data (may be empty).
fn flush_eof(
    utf8_buf: &mut Utf8ReadBuffer,
    esc_buf: &mut EscapeAwareBuffer,
    session_id: &str,
    state: &AppState,
) -> String {
    let utf8_tail = utf8_buf.flush();
    let esc_remaining = if utf8_tail.is_empty() {
        esc_buf.flush()
    } else {
        let mut flushed = esc_buf.push(&utf8_tail);
        flushed.push_str(&esc_buf.flush());
        flushed
    };
    if !esc_remaining.is_empty()
        && let Some(ring) = state.session_maps.output_buffers.get(session_id)
    {
        let mut ring_guard = ring.lock();
        ring_guard.write(esc_remaining.as_bytes());
        crate::state::broadcast_to_ws_clients(&state.ws_clients, session_id, &esc_remaining);
        drop(ring_guard);
    }
    esc_remaining
}

type VtProcessResult = (
    Vec<crate::state::ChangedRow>,
    bool,
    Vec<crate::terminal_grid::TermEvent>,
    // Whether the reusable screen snapshot was refilled this tick.
    bool,
    AgentScreenActivity,
    Option<usize>,
    Option<crate::terminal_grid::LogicalPrefix>,
    Option<crate::terminal_grid::LogicalPrefix>,
    usize,
    usize,
    Option<(crate::terminal_grid::LogicalPrefix, String)>,
    // Whether the alternate screen was active when this chunk finished.
    bool,
);

#[cfg(feature = "dictation")]
pub use tuic_dictation::continuous::{VoiceHold, VoiceWrite};

/// Spawn a reader thread that reads from a PTY, processes output, and emits events.
/// Unified for both desktop (Tauri IPC) and headless (event_bus only) modes.
/// 1-minute system load average divided by the online CPU count — a measure of
/// machine-wide CPU oversubscription (NOT this process's own usage, which the
/// cpu_watchdog covers via getrusage). >= 1.0 means the run queue is as long as
/// there are cores: things are queueing and the WebView main thread gets starved.
/// Used to gate the typing frame-throttle so it only kicks in under real load.
/// Returns 0.0 where unavailable (Windows) — throttle stays off, behaviour unchanged.
#[cfg(unix)]
fn system_load_per_core() -> f64 {
    let mut avg = [0f64; 3];
    let n = unsafe { libc::getloadavg(avg.as_mut_ptr(), 3) };
    if n < 1 {
        return 0.0;
    }
    let ncpu = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
    let ncpu = if ncpu < 1 { 1.0 } else { ncpu as f64 };
    avg[0] / ncpu
}

#[cfg(not(unix))]
fn system_load_per_core() -> f64 {
    0.0
}

/// Minimum interval (ms) the grid ticker must wait between frame sends.
/// `0` = no floor (send at the full 16 ms tick / ~60 fps), for short bursts so
/// latency stays low. The two floors give the WebView main thread breathing room:
///  - `input_recent` (user typing under CPU saturation) → ~20 fps, the most
///    aggressive floor, so keystroke dispatch + echo aren't stuck behind output.
///  - sustained animation (grid dirty ≥ 6 consecutive ticks, e.g. a spinner TUI)
///    → ~30 fps.
///
/// Typing wins over sustained because it's the latency-critical case.
fn grid_send_min_interval_ms(input_recent: bool, dirty_run: u32) -> u64 {
    const SUSTAINED_DIRTY_TICKS: u32 = 6;
    const SUSTAINED_MIN_INTERVAL_MS: u64 = 33; // ~30 fps while animating
    const INPUT_MIN_INTERVAL_MS: u64 = 50; // ~20 fps while typing under load
    if input_recent {
        INPUT_MIN_INTERVAL_MS
    } else if dirty_run >= SUSTAINED_DIRTY_TICKS {
        SUSTAINED_MIN_INTERVAL_MS
    } else {
        0
    }
}

/// Stamp the per-session last-input timestamp (epoch ms). Read by the grid
/// ticker to throttle frame sends while the user types under CPU saturation,
/// keeping the WebView/browser main thread free for keystroke dispatch + echo.
/// Called before an interactive PTY write (desktop and HTTP/PWA), so a fast
/// echo cannot beat priority arming. Only reader-applied output wakes the ticker.
pub(crate) fn stamp_input_ms(state: &AppState, session_id: &str) {
    record_input_ms(state, session_id);
    if let Some(priority) = state.grid.input_frame_priority.get(session_id) {
        priority.arm(std::time::Instant::now());
    }
}

/// Timestamp-only bookkeeping after successful writes, including automated input.
/// It must not re-arm priority after the reader already applied a fast echo.
pub(crate) fn record_input_ms(state: &AppState, session_id: &str) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    state
        .session_maps
        .last_input_ms
        .entry(session_id.to_string())
        .or_insert_with(|| std::sync::atomic::AtomicU64::new(0))
        .store(now_ms, std::sync::atomic::Ordering::Relaxed);
}

/// Apply the semantic effects of a submitted terminal line for every transport.
/// Empty content is still a submission: bare Enter resolves highlighted choices
/// and confirmation prompts, so it must clear an active wait and advance the
/// turn just like a non-empty reply.
pub(crate) fn record_submitted_line(
    state: &Arc<AppState>,
    session_id: &str,
    content: String,
    line: i64,
) {
    note_submitted_input(state, session_id);
    if content.split_whitespace().count() >= 10 {
        state
            .session_maps
            .last_prompts
            .insert(session_id.to_string(), content.clone());
    } else {
        // Keep last_prompts in sync with the actual last submission rather
        // than leaving a stale value in place: without this, a short
        // follow-up ("fix it") would inherit the previous turn's prompt text
        // for both consumers (get_last_prompt, and AgentBlock.prompt_text at
        // the busy edge in handle_tuic_state) instead of correctly having
        // none.
        state.session_maps.last_prompts.remove(session_id);
    }
    let parsed = ParsedEvent::UserInput { content, line };
    if let Ok(json) = serde_json::to_value(&parsed).map(std::sync::Arc::new) {
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: session_id.to_string(),
            parsed: std::sync::Arc::clone(&json),
        });
        #[cfg(feature = "desktop")]
        if let Some(app) = state.app_handle.read().as_ref() {
            let _ = app.emit(&format!("pty-parsed-{session_id}"), &*json);
        }
    }
    if let Some(ss) = state.session_maps.silence_states.get(session_id) {
        let mut sl = ss.lock();
        sl.suppress_user_input();
        // The parser's api-error / session-conflict dedup lives in the reader
        // thread and cannot observe this event; park the reset for it.
        sl.request_parser_dedup_reset();
    }
}

/// The window-attention action an OSC 1337 `RequestAttention` value maps to.
/// Kept independent of `tauri::UserAttentionType` (which does not exist at
/// all without the `desktop` feature — `tauri` itself is an optional,
/// desktop-only dependency) so the mapping stays a pure, always-compiled,
/// unit-testable function; the caller converts to the real Tauri type only
/// inside its own `#[cfg(feature = "desktop")]` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttentionLevel {
    /// "no" — cancel a pending attention request.
    Cancel,
    /// "once" — a single bounce.
    Informational,
    /// "yes" (bounce until focused) or "fireworks" (macOS-only cursor
    /// animation, with no direct Tauri equivalent — mapped to the same
    /// continuous-bounce behavior as "yes").
    Critical,
}

/// Maps an OSC 1337 `RequestAttention` value to the action to take. `None`
/// means "do nothing" — only reachable if a value slips through that
/// `Term::request_attention` (the alacritty patch) did not already validate
/// to one of "yes"/"once"/"no"/"fireworks".
fn attention_level_for_value(value: &str) -> Option<AttentionLevel> {
    match value {
        "no" => Some(AttentionLevel::Cancel),
        "once" => Some(AttentionLevel::Informational),
        "yes" | "fireworks" => Some(AttentionLevel::Critical),
        _ => None,
    }
}

/// Confirm an OSC 1337 `OpenURL` request with the human, then — only if they
/// say yes — tell every client to actually open it (`handleOpenUrl` on the
/// frontend). Runs on the async runtime via `async_rt.spawn` in
/// `spawn_reader_thread`, never awaited inline on the blocking PTY reader
/// thread: the confirm can wait on a human for up to the confirm timeout.
async fn confirm_and_notify_open_url(state: Arc<AppState>, session_id: String, url: String) {
    let confirmed = crate::mcp_http::confirm_open_url(&state, &session_id, &url).await;
    if !confirmed {
        return;
    }
    #[cfg(feature = "desktop")]
    if let Some(a) = state.app_handle.read().as_ref() {
        let _ = a.emit(
            "pty-open-url",
            serde_json::json!({ "session_id": session_id, "url": url }),
        );
    }
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::PtyOpenUrl { session_id, url });
}

pub(crate) fn spawn_reader_thread(
    mut reader: Box<dyn Read + Send>,
    paused: Arc<AtomicBool>,
    session_id: String,
    state: Arc<AppState>,
    tuic_session: Option<String>,
) {
    // HTTP/MCP and generic shell spawns already bind the child's inherited
    // TUIC_SESSION before starting this reader. Use that same live identity
    // when the caller has no explicit one, including restored stable identities
    // that differ from the PTY key. Native hook ownership stays fail-closed.
    let tuic_session = tuic_session.or_else(|| {
        crate::mcp_http::session::live_tuic_sessions_by_pty(&state).remove(&session_id)
    });
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    let running = Arc::new(AtomicBool::new(true));

    state
        .session_maps
        .silence_states
        .insert(session_id.clone(), silence.clone());
    state.session_maps.shell_states.insert(
        session_id.clone(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );

    spawn_silence_timer(
        silence.clone(),
        running.clone(),
        session_id.clone(),
        state.clone(),
    );

    // A `tokio::runtime::Handle` the reader thread below (a plain
    // `std::thread::spawn`, not a tokio task) can use to dispatch the async
    // OSC 1337 `OpenURL` confirm-then-notify flow — see `confirm_and_notify_open_url`.
    // Mirrors `dir_watcher::start_watching`'s identical desktop/headless split.
    let async_rt = {
        #[cfg(feature = "desktop")]
        {
            tauri::async_runtime::handle().inner().clone()
        }
        #[cfg(not(feature = "desktop"))]
        {
            tokio::runtime::Handle::current()
        }
    };

    // Frame ticker: decouples PTY read() from frame serialization.
    // Reader sets dirty flag; ticker serializes+sends on paced or input wakes.
    // Coalesces rapid writes (spinner erase+rewrite) into a single frame.
    let frame_dirty = Arc::new(AtomicBool::new(false));
    state
        .grid
        .frame_dirty
        .insert(session_id.clone(), frame_dirty.clone());
    // Scroll target the ticker consumes, created alongside the dirty flag the
    // same handler sets. It belongs to the session, not to one of its front
    // ends: when the desktop `subscribe_terminal_grid` owned it, a session only
    // a browser had ever rendered had nowhere to record a scroll, and closing
    // the desktop terminal took the entry away from an attached browser.
    state
        .grid
        .pending_scroll
        .insert(session_id.clone(), Arc::new(AtomicI64::new(-1)));
    let input_priority = Arc::new(frame_priority::InputFramePriority::default());
    state
        .grid
        .input_frame_priority
        .insert(session_id.clone(), input_priority.clone());
    let ticker_priority = input_priority.clone();
    let sync_active = Arc::new(AtomicBool::new(false));
    state
        .grid
        .sync_update_active
        .insert(session_id.clone(), sync_active.clone());
    // Shared by the PTY reader (which batches lines) and the frame ticker (which
    // drains a tail the reader cannot: read() blocks, so the last line of a burst
    // would otherwise wait for output that may never come).
    let watcher_batcher = Arc::new(parking_lot::Mutex::new(
        crate::output_watchers::WatcherLineBatcher::new(WATCHER_LINE_WINDOW, WATCHER_BATCH_CAP),
    ));
    let ticker_batcher = watcher_batcher.clone();
    let ticker_running = running.clone();
    let ticker_dirty = frame_dirty.clone();
    let ticker_sync_active = Some(sync_active);
    let ticker_state = state.clone();
    let ticker_sid = session_id.clone();
    std::thread::spawn(move || {
        // Frame serialize+emit is the Rust side of the echo→render path; keep it
        // in the high QoS band so output stays live under a saturating build.
        raise_thread_for_interactive_io();
        const TICK: std::time::Duration = std::time::Duration::from_millis(16);
        // Safety net: if in_flight stays true for this long (~500 ms),
        // force-reset it so frame delivery resumes. Prevents permanent blank
        // terminal when the frontend fails to ack (crash, corrupt frame, etc.).
        //
        // Hidden desktop subscriptions are excluded before this gate. Only a
        // visible viewport (or a frame already queued before hide) can time out.
        const MAX_IN_FLIGHT_MS: u64 = 500;
        // Send-rate floors live in grid_send_min_interval_ms() (unit-tested).
        // How long after a keystroke the typing-throttle stays armed.
        const INPUT_THROTTLE_WINDOW_MS: u64 = 150;
        const LOAD_SATURATION_RATIO: f64 = 1.0; // 1-min load >= cores
        const LOAD_SAMPLE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
        let mut stuck_since: Option<std::time::Instant> = None;
        let mut dirty_run: u32 = 0;
        let mut last_sent: Option<std::time::Instant> = None;
        let mut last_load_check: Option<std::time::Instant> = None;
        let mut system_saturated = false;
        ticker_priority.bind_ticker();
        let mut next_tick = std::time::Instant::now() + TICK;
        let mut priority_retry: Option<std::time::Instant> = None;
        while ticker_running.load(Ordering::Relaxed) {
            let wake_at = priority_retry.map_or(next_tick, |retry| retry.min(next_tick));
            std::thread::park_timeout(wake_at.saturating_duration_since(std::time::Instant::now()));
            let now = std::time::Instant::now();
            let periodic_tick = now >= next_tick;
            priority_retry = None;
            if periodic_tick {
                next_tick = now + TICK;
            } else if ticker_priority.request(now, last_sent).is_none() {
                // Spurious/late notifications do not accelerate background output.
                continue;
            }
            // Drain a watcher-line tail the reader left batched. read() blocks, so
            // without this the last line of a burst waits for output that may
            // never arrive and a rare-line watcher matches minutes late. It must
            // run BEFORE the dirty guard below: the tick that clears frame_dirty
            // is usually the one *before* the batching window expires, and every
            // later idle tick would return early and never look at the batch.
            {
                let mut batch = ticker_batcher.lock();
                // Emit under the lock so this tail cannot be interleaved behind a
                // newer batch the reader emits concurrently.
                if let Some(due) = batch.flush_due(std::time::Instant::now()) {
                    emit_watcher_lines(&ticker_state, &ticker_sid, due);
                }
                drop(batch);
            }
            let mut effective_dirty = ticker_dirty.swap(false, Ordering::Relaxed);
            // DEC 2026: the vendored VTE records a 150ms deadline but never fires
            // it, so a synchronized update left open (delayed/lost ESU, or a stream
            // that simply stops mid-update) would buffer forever and wedge the
            // terminal. This is the only wakeup that can end it — no PTY bytes are
            // coming — so it must run BEFORE the non-dirty early return. The
            // atomic hint keeps idle sessions from touching the vt lock at all.
            let mut sync_timeout_flush = false;
            let mut stalled_replies: Vec<String> = Vec::new();
            let mut stalled_kitty_handles: Option<(
                crate::terminal_image_transmission::KittyImageStoreHandle,
                crate::terminal_image_transmission::KittyPendingJobsHandle,
            )> = None;
            if ticker_sync_active
                .as_ref()
                .is_some_and(|f| f.load(Ordering::Relaxed))
                && let Some(vt) = ticker_state.grid.vt_log_buffers.get(&ticker_sid)
            {
                let mut g = vt.lock();
                if g.flush_sync_timeout_if_needed() {
                    sync_timeout_flush = true;
                    effective_dirty = true;
                    // A DSR/CPR or DA1/DA2 query buried inside the stalled
                    // update is replayed by the flush above and queues a
                    // PtyWrite reply — but this ticker only forwards screen
                    // frames below, it never runs the ordinary process_chunk
                    // path that would otherwise flush that reply. Without
                    // this, the reply sits queued until an unrelated later
                    // PTY chunk happens to arrive (or forever, if no more
                    // output ever comes). Only the PtyWrite events are
                    // drained here — other kinds (title, OSC 133, TUIC) stay
                    // queued for the next real chunk, which is the only
                    // place equipped to act on them.
                    stalled_replies = g.grid_drain_pty_write_events();
                    // Same reasoning, for the second (non-`TermEvent`) queue
                    // a Kitty image transmission buried in the stalled
                    // update can populate — see `resolve_kitty_decode_jobs`'s
                    // doc comment. Handles cloned now, while `g` is already
                    // locked for this flush; resolved below, after `drop(g)`.
                    stalled_kitty_handles = Some(g.grid_kitty_decode_handles());
                }
                let still_active = g.is_sync_update_active();
                drop(g);
                if let Some(f) = ticker_sync_active.as_ref() {
                    f.store(still_active, Ordering::Relaxed);
                }
            }
            for reply in &stalled_replies {
                write_terminal_reply(&ticker_state, &ticker_sid, reply.as_bytes(), "PtyWrite");
            }
            if let Some((image_store, pending_jobs)) = &stalled_kitty_handles {
                resolve_kitty_decode_jobs(&ticker_state, &ticker_sid, image_store, pending_jobs);
            }
            if !effective_dirty {
                // Idle tick: leave sustained-animation mode so the next burst
                // (keystroke, fresh output) gets full 60 fps low-latency response.
                dirty_run = 0;
                continue;
            }
            // F28: nobody is looking. Everything below — the vt lock and a full
            // serialize_dirty_rows — would produce bytes that send_grid_frame
            // drops on the floor, which is what a PTY still running behind a
            // closed tab used to pay on every dirty tick.
            //
            // Nothing is lost by skipping. The damage stays on the vt because
            // serialize_dirty_rows is what would have cleared it, and both
            // subscribe paths repaint from scratch anyway: terminal_request_frame
            // forces full damage before serializing, and the WS path
            // (mcp_http/session.rs full_frame_for_single_client) forces it twice
            // and re-arms this ticker.
            //
            // A pending scroll is the one thing that must NOT be skipped with the
            // frame: it is session state, not pixels. `/terminal/scroll-info`,
            // the row reads and the next full frame all answer from the grid's
            // display offset, so a target dropped here would silently disagree
            // with every later read — and an HTTP client that scrolls without
            // holding a grid WebSocket is exactly the browser/PWA case. It costs
            // the vt lock only when a client actually asked for a scroll.
            //
            if !grid_has_subscriber(&ticker_state, &ticker_sid) {
                if let Some(target) = take_pending_scroll(&ticker_state, &ticker_sid)
                    && let Some(vt) = ticker_state.grid.vt_log_buffers.get(&ticker_sid)
                {
                    apply_scroll_target(&mut vt.lock(), target);
                }
                dirty_run = 0;
                continue;
            }
            dirty_run = dirty_run.saturating_add(1);
            // Clone the Arc out so recovery never retains a DashMap shard guard.
            let gate = ticker_state
                .grid
                .gates
                .get(&ticker_sid)
                .filter(|_| {
                    #[cfg(feature = "desktop")]
                    {
                        desktop_grid_visible(&ticker_state, &ticker_sid)
                    }
                    #[cfg(not(feature = "desktop"))]
                    {
                        false
                    }
                })
                .map(|g| Arc::clone(g.value()));
            // The gate belongs to the desktop WebView and to nothing else. It used
            // to stop this tick outright, which is correct only while the desktop
            // is the sole consumer: a browser/PWA client rides a different
            // transport with its own flow control, and a stalled WebView is not
            // its problem. So the stall accounting below still runs — the gate is
            // still what decides whether the desktop CHANNEL gets this frame, in
            // `send_grid_frame` — but it may only stop the tick when there is
            // nobody else to serve.
            let watchers = grid_has_watcher(&ticker_state, &ticker_sid);
            if gate.as_ref().is_some_and(|g| g.is_paused()) {
                stuck_since = None;
                if !watchers {
                    if let Some(target) = take_pending_scroll(&ticker_state, &ticker_sid)
                        && let Some(vt) = ticker_state.grid.vt_log_buffers.get(&ticker_sid)
                    {
                        apply_scroll_target(&mut vt.lock(), target);
                    }
                    continue;
                }
            } else if gate.as_ref().is_some_and(|g| !g.is_open()) {
                let now = std::time::Instant::now();
                let since = stuck_since.get_or_insert(now);
                let elapsed = now.duration_since(*since).as_millis() as u64;
                if elapsed > MAX_IN_FLIGHT_MS {
                    if let Some(g) = gate.as_ref() {
                        g.abandon();
                        tracing::warn!(
                            session_id = %ticker_sid,
                            elapsed_ms = elapsed,
                            stuck_count = g.abandoned_count(),
                            watchers,
                            paused = g.is_paused(),
                            "grid frame gate stuck, abandoning the outstanding frame"
                        );
                    }
                    stuck_since = None;
                }
                if !watchers {
                    ticker_dirty.store(true, Ordering::Relaxed);
                    continue;
                }
            } else {
                stuck_since = None;
            }
            // Adaptive frame-rate floor: a TUI that animates continuously (e.g.
            // grok's spinner repaints its whole bordered UI and walks the cursor
            // around every tick) keeps the grid dirty 100% of the time, so the
            // ticker would emit ~50 multi-KB frames/s. The in_flight gate prevents
            // queue overflow but NOT WebView main-thread starvation — it paints
            // flat-out and never yields to input/console, so the UI looks frozen.
            // Once dirtiness is sustained, cap the send rate to ~30 fps to give the
            // JS thread breathing room. Short bursts stay at the full 60 fps tick.
            let now = std::time::Instant::now();
            // Refresh the machine-saturation gate ~once/sec (cheap getloadavg).
            if last_load_check.is_none_or(|t| now.duration_since(t) >= LOAD_SAMPLE_INTERVAL) {
                system_saturated = system_load_per_core() >= LOAD_SATURATION_RATIO;
                last_load_check = Some(now);
            }
            // Typing-under-load throttle: only when saturated AND the user typed
            // recently. now_epoch_ms() is computed only on the saturated path.
            let input_recent = system_saturated && {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                ticker_state
                    .session_maps
                    .last_input_ms
                    .get(&ticker_sid)
                    .map(|ts| {
                        now_ms.saturating_sub(ts.load(Ordering::Relaxed)) < INPUT_THROTTLE_WINDOW_MS
                    })
                    .unwrap_or(false)
            };
            // Pick the send-rate floor: typing-under-load (~20 fps) wins, else the
            // sustained-animation floor (~30 fps), else full 60 fps for short bursts.
            // A sync-timeout flush is a protocol deadline, not animation — the
            // frame-rate floor must not defer it into the next tick.
            let min_interval = if sync_timeout_flush {
                0
            } else {
                grid_send_min_interval_ms(input_recent, dirty_run)
            };
            let floor_pending = min_interval > 0
                && last_sent.is_some_and(|last| {
                    now.duration_since(last).as_millis() < min_interval as u128
                });
            let priority_request = ticker_priority.request(now, last_sent);
            let priority_due = priority_request.is_some_and(|request| request.ready_at <= now);
            if !sync_timeout_flush && (floor_pending || !periodic_tick) && !priority_due {
                // Retry only a coalescing deadline, never a closed gate. The
                // periodic tick still owns gate/sync maintenance and background frames.
                priority_retry = priority_request.map(|request| request.ready_at);
                ticker_dirty.store(true, Ordering::Relaxed);
                continue;
            }
            let priority_bypass = !sync_timeout_flush && (floor_pending || !periodic_tick);
            // The WebView missed frames while its gate was closed and has caught
            // up: those rows went to the WebSocket subscribers and left the shared
            // damage, so a delta now would land on a row map with holes in it.
            // Pay the debt with a full frame private to this channel — it consumes
            // no damage, so the delta below still reaches everyone else.
            let desktop_owed_full_frame = gate
                .as_ref()
                .is_some_and(|g| g.is_open() && g.take_missed());
            if let Some(vt) = ticker_state.grid.vt_log_buffers.get(&ticker_sid) {
                let mut g = vt.lock();
                if let Some(target) = take_pending_scroll(&ticker_state, &ticker_sid) {
                    apply_scroll_target(&mut g, target);
                }
                // Cut before the delta, from the same locked state, so the rows the
                // delta carries are already in it.
                let repair = desktop_owed_full_frame.then(|| g.serialize_full_frame());
                let frame = g.serialize_dirty_rows();
                drop(g);
                #[cfg(feature = "desktop")]
                if let Some(repair) = repair {
                    send_desktop_grid_frame(&ticker_state, &ticker_sid, repair);
                }
                #[cfg(not(feature = "desktop"))]
                let _ = repair;
                send_grid_frame(&ticker_state, &ticker_sid, frame);
                last_sent = Some(now);
                if let Some(request) = priority_request {
                    ticker_priority.sent(request, now, priority_bypass);
                }
            }
        }
        // Final flush after reader exits. Session teardown is the other "no more
        // PTY bytes arrive" case: drain any still-buffered synchronized update
        // BEFORE serializing, or its content is dropped with the session.
        // No watcher-line drain here on purpose: teardown has exactly one owner,
        // the reader's EOF path, which assembles the flush_eof remainder and
        // drains whatever is still batched, in order. A second drain racing from
        // this thread could deliver the older tail after it.
        if let Some(vt) = ticker_state.grid.vt_log_buffers.get(&ticker_sid) {
            let mut g = vt.lock();
            g.force_stop_sync_if_buffered();
            // Same reasoning as the stalled-sync-update flush above: this
            // can also queue a Kitty decode job. No PTY reply will ever be
            // read at this point (the child has already exited), but
            // resolving the job still matters for the *store* — leaving a
            // placeholder permanently `is_pending()` would make any later
            // `image_bytes`/`image_meta` read against this now-closing
            // session's residual state (e.g. a final scrollback view) see
            // "still loading" forever instead of a clean ready/failed
            // result.
            let (image_store, pending_jobs) = g.grid_kitty_decode_handles();
            let frame = g.serialize_dirty_rows();
            drop(g);
            resolve_kitty_decode_jobs(&ticker_state, &ticker_sid, &image_store, &pending_jobs);
            send_grid_frame(&ticker_state, &ticker_sid, frame);
        }
        ticker_state.grid.frame_dirty.remove(&ticker_sid);
        ticker_state.grid.sync_update_active.remove(&ticker_sid);
        ticker_state.grid.input_frame_priority.remove(&ticker_sid);
    });

    std::thread::spawn(move || {
        // PTY reader drives byte intake → echo; keep it above default-QoS builds.
        raise_thread_for_interactive_io();
        let sid_for_panic = session_id.clone();
        let state_for_panic = state.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut buf = [0u8; 65536];
            let mut utf8_buf = Utf8ReadBuffer::new();
            let mut esc_buf = EscapeAwareBuffer::new();
            let session_cwd: Option<String> = state
                .session_maps
                .sessions
                .get(&session_id)
                .and_then(|s| s.lock().cwd.clone());
            let mut processor = ChunkProcessor::new(session_cwd, tuic_session);
            // Line reassembly for plugin watcher matching. Separate from the VT
            // parser: watchers match the byte stream as it scrolls past, not the
            // screen contents.
            let mut watcher_lines = crate::output_watchers::StreamLines::new();
            let mut activity_pulse = ActivityPulse::new();
            loop {
                while paused.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        // A chunk already being processed before input cannot
                        // consume the new input's wake hint.
                        let input_generation = input_priority.generation();
                        state.metrics.bytes_emitted.fetch_add(n, Ordering::Relaxed);
                        // Per-session sibling of the global counter above — see
                        // `state::drain_counter_map`. Answers "which session
                        // produced the most raw output recently," which the
                        // global-only counter never could.
                        crate::state::bump_counter(
                            &state.session_maps.session_output_bytes,
                            &session_id,
                            n as u64,
                        );
                        // Flight recorder: keep the last PTY_RAW_RING_CAP raw bytes
                        // (pre-transform) so a wild rendering corruption can be
                        // dumped and replayed offline (story 056-7545).
                        //
                        // Inline-image payloads (OSC 1337 / Kitty APC) are elided
                        // to a short placeholder first (color-tools plan, Phase
                        // 8) — this ring is diagnostic-only, and a single large
                        // image transmission would otherwise evict most of its
                        // history. The real parser downstream still processes
                        // `buf[..n]` itself, unelided.
                        {
                            let elided =
                                crate::image_payload_elision::elide_image_payloads(&buf[..n]);
                            let to_store = elided.as_deref().unwrap_or(&buf[..n]);
                            let ring = state
                                .grid
                                .pty_raw_rings
                                .entry(session_id.clone())
                                .or_default();
                            let mut ring = ring.lock();
                            ring.extend(to_store);
                            if ring.len() > PTY_RAW_RING_CAP {
                                let excess = ring.len() - PTY_RAW_RING_CAP;
                                ring.drain(..excess);
                            }
                        }
                        let utf8_data = utf8_buf.push(&buf[..n]);
                        let esc_data = esc_buf.push(&utf8_data);
                        let (kitty_clean, kitty_actions) = strip_kitty_sequences(&esc_data);
                        // Diagnostic-only substring scan over the whole chunk. It ran
                        // on every PTY read to catch a DECRST leak that has not been
                        // seen since; behind the Diagnostics toggle it costs one
                        // relaxed atomic load instead of two passes over 64 KB.
                        // Enable with POST /diagnostics {"enabled":true} to get it back.
                        if crate::cpu_watchdog::diagnostic_mode()
                            && kitty_clean.contains("1049l")
                            && !kitty_clean.contains("\x1b[?1049l")
                        {
                            tracing::error!(source = "terminal", session_id = %session_id,
                            "DECRST leak: kitty_clean has bare '1049l' without ESC[? prefix. \
                             esc_data({} bytes)={:?}, kitty_clean({} bytes)={:?}, actions={:?}",
                            esc_data.len(), esc_data.as_bytes().iter().take(200).collect::<Vec<_>>(),
                            kitty_clean.len(), kitty_clean.as_bytes().iter().take(200).collect::<Vec<_>>(),
                            kitty_actions);
                        }
                        let data = kitty_clean;

                        process_kitty_actions(&kitty_actions, &session_id, &state);

                        let chunk_processed =
                            processor.process_chunk(&data, &silence, &session_id, &state);
                        // Drained regardless of whether process_chunk returned data to
                        // render — an OpenURL sequence carries no visible output of its
                        // own. Each confirm-then-notify flow can block on a human answer
                        // for up to OPEN_URL_CONFIRM_TIMEOUT, so it must run on the async
                        // runtime, not this blocking reader thread.
                        for url in processor.pending_open_urls.drain(..) {
                            async_rt.spawn(confirm_and_notify_open_url(
                                state.clone(),
                                session_id.clone(),
                                url,
                            ));
                        }

                        if chunk_processed {
                            processor.observe_alt_buffer(&data);

                            // Plugin OutputWatcher matching. The canvas renders
                            // from grid frames and never read this text; the only
                            // consumer was pluginRegistry, which reassembled lines
                            // in the WebView. It happens here now, on the reader
                            // thread, and only the lines that matter cross the
                            // boundary — throttled but LOSSLESS.
                            //
                            // Losslessness is the point: the original throttle
                            // DROPPED chunks inside its window, which spliced the
                            // tail of one chunk onto the head of a later one and
                            // reported a line that never existed (audit F1).
                            assemble_watcher_lines(
                                &state,
                                &session_id,
                                &data,
                                &mut watcher_lines,
                                &watcher_batcher,
                                false,
                            );

                            // "Output happened" for the activity dot and the
                            // last-seen timestamp. Sits here, in the same block the
                            // deleted `pty-output` emit occupied, so it reports on
                            // exactly the chunks that one did — a chunk the
                            // processor swallowed whole was never activity.
                            activity_pulse.pulse(&state, &session_id);
                        }

                        if std::mem::take(&mut processor.queued_idle_flush)
                            && state
                                .pending_injections
                                .get(&session_id)
                                .is_some_and(|queue| !queue.is_empty())
                        {
                            flush_pending_injections(&state, &session_id);
                        }

                        frame_dirty.store(true, Ordering::Relaxed);
                        input_priority.applied(input_generation, std::time::Instant::now());
                    }
                    Err(e) => {
                        tracing::error!(session_id = %session_id, "PTY reader error: {e}");
                        break;
                    }
                }
            }
            running.store(false, Ordering::Relaxed);

            if try_shell_transition(&state, &session_id, SHELL_BUSY, SHELL_IDLE, false) {
                emit_shell_state(&state, &session_id, "idle");
                // EOF bypass: should_transition_idle was not called, so force-clear
                // active_sub_tasks to avoid leaving the frontend notification gate
                // in an inconsistent state after process crash/exit.
                let needs_clear = state
                    .session_maps
                    .session_states
                    .get_mut(&session_id)
                    .filter(|e| e.active_sub_tasks > 0)
                    .map(|mut e| {
                        e.active_sub_tasks = 0;
                    })
                    .is_some();
                if needs_clear {
                    emit_active_subtasks(&state, &session_id, 0, "");
                }
            }

            let remaining = flush_eof(&mut utf8_buf, &mut esc_buf, &session_id, &state);
            // The remainder goes through the assembler, not straight out: it may
            // close a line the last read left partial. Then drain unconditionally
            // — teardown has no later tick to flush the batch.
            assemble_watcher_lines(
                &state,
                &session_id,
                &remaining,
                &mut watcher_lines,
                &watcher_batcher,
                true,
            );
            {
                // Hold the lock across the emit: the ticker may still be running
                // its own flush, and releasing between take and emit would let it
                // deliver an earlier tail after this final batch.
                let mut batch = watcher_batcher.lock();
                if let Some(due) = batch.take() {
                    emit_watcher_lines(&state, &session_id, due);
                }
                drop(batch);
            }

            // Restore an OSC-title-overwritten name to its captured base
            // before announcing the exit and the close — see
            // `osc_title::restore_base_on_exit`'s doc comment. This is what
            // makes step C.1.5 (the ported frontend's own exit-time
            // restore) work on every transport instead of only desktop. It
            // must precede `PtyExit`, not just `session-closed`: a tab drops
            // its `sessionId` on exit (Terminal.tsx), so a rename that lands
            // after the exit frame (desktop `pty-exit-{id}`, or the remote
            // per-session WS `exit` frame, which also disposes the socket)
            // finds no tab to apply to.
            crate::osc_title::restore_base_on_exit(&state, &session_id, &processor.osc_title_base);
            state.emit_pty_event(crate::state::AppEvent::PtyExit {
                session_id: session_id.clone(),
            });
            #[cfg(feature = "desktop")]
            if let Some(app) = state.app_handle.read().as_ref() {
                let _ = app.emit(
                    &format!("pty-exit-{session_id}"),
                    serde_json::json!({ "session_id": session_id }),
                );
            }
            tracing::info!(source = "pty", session_id = %session_id, "Session closed: process exited");
            emit_session_closed(&state, &session_id, "process_exit");

            mark_session_exited(&session_id, &state);
        })); // end catch_unwind
        if let Err(panic_info) = result {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic payload".to_string()
            };
            tracing::error!(session_id = %sid_for_panic, "READER THREAD PANICKED: {msg}");
            // The store that ends the frame ticker and the 1 Hz silence timer
            // lives inside the closure above, so a panic skips it: without this
            // the ticker keeps waking ~62 times a second and the tokio timer
            // keeps ticking for the life of the process, and the ticker never
            // reaches the code after its loop that removes this session's
            // grid_frame_dirty / sync_update_active entries.
            running.store(false, Ordering::Relaxed);
            mark_session_exited(&sid_for_panic, &state_for_panic);
        }
    });
}

/// Consume the coalesced scroll target a client left for this session.
///
/// Both transports write it (`terminal_scroll_to_offset` as a Tauri command and
/// as an HTTP route) without taking the vt lock, and the frame ticker is the only
/// reader — so the swap here is what makes "latest wins" true: whatever arrived
/// since the last tick is applied once, and `-1` means nothing is pending.
/// Nonnegative values are display offsets; <= -2 encodes an all-time row anchor.
fn take_pending_scroll(state: &AppState, session_id: &str) -> Option<i64> {
    let target = state
        .grid
        .pending_scroll
        .get(session_id)?
        .swap(-1, Ordering::Relaxed);
    (target != -1).then_some(target)
}

/// Enqueue an anchored target in the same latest-wins slot as offset requests.
/// -1 is empty, nonnegative values are offsets, <= -2 encode all-time anchors.
/// None follows the tail. Saturation keeps untrusted usize values in range.
pub(crate) fn queue_scroll_anchor(state: &AppState, session_id: &str, anchor: Option<usize>) {
    let target = anchor.map_or(0, |row| -2 - row.min((i64::MAX - 1) as usize) as i64);
    if let Some(pending) = state.grid.pending_scroll.get(session_id) {
        pending.store(target, Ordering::Relaxed);
    }
    if let Some(dirty) = state.grid.frame_dirty.get(session_id) {
        dirty.store(true, Ordering::Relaxed);
    }
}

fn apply_scroll_target(vt: &mut crate::state::VtLogBuffer, target: i64) {
    if target <= -2 {
        vt.grid_scroll_to_anchor(Some((-2 - target) as usize));
    } else {
        vt.grid_scroll_to_offset(target as usize);
    }
}

#[cfg(feature = "desktop")]
fn desktop_grid_visible(state: &AppState, session_id: &str) -> bool {
    state
        .grid
        .channels
        .get(session_id)
        .is_some_and(|ch| ch.visible.load(Ordering::Relaxed))
}

/// Discard a pending coalesced scroll target on a real alt-screen transition.
///
/// `current_alt_screen` is `vt.is_alternate_screen()` for the chunk just
/// processed; `last_alt_screen` is the caller's own tracking of the previous
/// chunk's value (updated in place). A target left in `pending_scroll` was
/// computed against whichever grid was active when the gesture fired — if
/// `swap_alt()` (`patches/alacritty_terminal`) swaps to a different grid with
/// its own independent `display_offset`/history before the frame ticker
/// consumes it, applying that stale target lands the view short of the new
/// grid's true bottom. See `pending_scroll`'s doc comment in `state.rs`.
fn clear_pending_scroll_on_alt_screen_transition(
    pending_scroll: &DashMap<String, Arc<AtomicI64>>,
    session_id: &str,
    current_alt_screen: bool,
    last_alt_screen: &mut bool,
) {
    if current_alt_screen != *last_alt_screen {
        if let Some(slot) = pending_scroll.get(session_id) {
            slot.store(-1, Ordering::Relaxed);
        }
        *last_alt_screen = current_alt_screen;
    }
}

#[cfg(test)]
mod pending_scroll_alt_screen_tests {
    use super::{AtomicI64, Ordering, clear_pending_scroll_on_alt_screen_transition};
    use dashmap::DashMap;
    use std::sync::Arc;

    #[test]
    fn clears_on_transition_into_alt_screen() {
        let map: DashMap<String, Arc<AtomicI64>> = DashMap::new();
        map.insert("s1".to_string(), Arc::new(AtomicI64::new(42)));
        let mut last = false;

        clear_pending_scroll_on_alt_screen_transition(&map, "s1", true, &mut last);

        assert_eq!(map.get("s1").unwrap().load(Ordering::Relaxed), -1);
        assert!(last);
    }

    #[test]
    fn clears_on_transition_out_of_alt_screen() {
        let map: DashMap<String, Arc<AtomicI64>> = DashMap::new();
        map.insert("s1".to_string(), Arc::new(AtomicI64::new(7)));
        let mut last = true;

        clear_pending_scroll_on_alt_screen_transition(&map, "s1", false, &mut last);

        assert_eq!(map.get("s1").unwrap().load(Ordering::Relaxed), -1);
        assert!(!last);
    }

    #[test]
    fn leaves_pending_target_alone_when_state_is_unchanged() {
        let map: DashMap<String, Arc<AtomicI64>> = DashMap::new();
        map.insert("s1".to_string(), Arc::new(AtomicI64::new(99)));
        let mut last = true;

        clear_pending_scroll_on_alt_screen_transition(&map, "s1", true, &mut last);

        assert_eq!(map.get("s1").unwrap().load(Ordering::Relaxed), 99);
        assert!(last);
    }

    #[test]
    fn missing_session_never_panics() {
        let map: DashMap<String, Arc<AtomicI64>> = DashMap::new();
        let mut last = false;

        clear_pending_scroll_on_alt_screen_transition(&map, "does-not-exist", true, &mut last);

        assert!(last);
    }
}

/// Is anyone waiting for this session's grid frames?
///
/// The two consumers `send_grid_frame` knows about: the desktop IPC channel, and
/// the watch that feeds browser/PWA clients. A watch *entry* is not a consumer —
/// the sender outlives its receivers, so the count is what decides.
///
/// Used by the frame ticker to skip the encode entirely rather than serialize a
/// frame `send_grid_frame` would drop. It must stay in step with that function:
/// a consumer this misses is a client that stops repainting.
pub(crate) fn grid_has_subscriber(state: &AppState, session_id: &str) -> bool {
    #[cfg(feature = "desktop")]
    if desktop_grid_visible(state, session_id) {
        return true;
    }
    grid_has_watcher(state, session_id)
}

/// Does this session have a browser/PWA client on the grid WebSocket?
///
/// Narrower than [`grid_has_subscriber`] on purpose: the frame ticker uses this
/// one to decide whether a stalled *desktop* WebView may stop the frames, and a
/// desktop channel is exactly what must not count towards that answer.
fn grid_has_watcher(state: &AppState, session_id: &str) -> bool {
    state
        .grid
        .watch
        .get(session_id)
        .is_some_and(|tx| tx.receiver_count() > 0)
}

/// Repair after a frame lost the ordering race.
///
/// The frame that was dropped carries rows the delivered one does not: the
/// damage behind them was consumed when it was serialized, so nothing will ever
/// send them again. Dropping it silently leaves those rows stale on every client
/// until something else happens to repaint them. Damage the grid again and wake
/// the ticker instead — the next frame is a full one and every transport is
/// whole. Unlike a per-subscriber resync (`serialize_full_frame`), re-damaging is
/// the *right* answer here: the loss is shared, so the repair has to be.
fn repaint_after_reorder(state: &AppState, session_id: &str) {
    tracing::debug!(session_id = %session_id, "grid frame arrived out of order, forcing a full repaint");
    if let Some(vt) = state.grid.vt_log_buffers.get(session_id) {
        vt.lock().grid_force_full_damage();
    }
    if let Some(dirty) = state.grid.frame_dirty.get(session_id) {
        dirty.store(true, Ordering::Relaxed);
    }
}

/// Send a grid frame through the session's channel and close the delivery gate.
/// Also publishes to the watch channel for WebSocket subscribers.
///
/// Frames are serialized under the vt lock and arrive here after it was
/// released, so two producers can reach this point in the opposite order. The
/// `order` the frame carries was stamped inside that critical section and is the
/// only record of which one is newer; a frame that lost the race is dropped here
/// and repaired with a full repaint rather than painted over a newer screen.
pub(crate) fn send_grid_frame(
    state: &AppState,
    session_id: &str,
    frame: crate::grid_gate::GridFrame,
) {
    if frame.is_empty() {
        return;
    }
    let crate::grid_gate::GridFrame { order, bytes } = frame;
    // Clone only when both consumers want the frame. A browser-only session has
    // no desktop channel, so the watch can take the original; cloning first and
    // then finding nothing to hand the original to was pure copy.
    #[cfg(feature = "desktop")]
    let desktop_wants_it = desktop_grid_visible(state, session_id);
    #[cfg(not(feature = "desktop"))]
    let desktop_wants_it = false;

    let frame = match state.grid.watch.get(session_id) {
        Some(watch_tx) => {
            let watched = watch_tx.receiver_count() > 0;
            let (for_watch, for_desktop) = match (watched, desktop_wants_it) {
                (true, true) => (Some(bytes.clone()), Some(bytes)),
                (true, false) => (Some(bytes), None),
                (false, _) => (None, Some(bytes)),
            };
            // The claim and the publish share one critical section: claiming
            // first and publishing after would let two producers claim in the
            // order they serialized and then publish in the other one.
            if crate::grid_watch::claim_grid_frame(&watch_tx, order, for_watch)
                == crate::grid_gate::FrameOrder::Stale
            {
                repaint_after_reorder(state, session_id);
                return;
            }
            match for_desktop {
                Some(bytes) => bytes,
                None => return,
            }
        }
        None => bytes,
    };

    #[cfg(feature = "desktop")]
    {
        // A closed gate means the WebView has not painted the frame before this
        // one. The ticker used to stop entirely here, which held the damage on
        // the vt but starved every browser subscriber; they keep receiving now,
        // so the rows in this frame have already left the shared damage without
        // reaching this channel. Record the debt and let the ticker pay it with a
        // full frame once the gate reopens — sending the delta now would only
        // deepen the backlog this gate exists to drain.
        let stalled = desktop_wants_it && {
            let gate = state.grid.gates.get(session_id);
            match gate.as_deref() {
                Some(gate) if !gate.is_open() => {
                    gate.note_missed();
                    true
                }
                _ => false,
            }
        };
        if stalled {
            // Arm the ticker so the debt is paid even if the session falls silent
            // the instant the WebView recovers: the repair rides a tick, and an
            // undirty session never takes one.
            if let Some(dirty) = state.grid.frame_dirty.get(session_id) {
                dirty.store(true, Ordering::Relaxed);
            }
            return;
        }
        send_desktop_grid_frame(state, session_id, frame);
    }
}

/// Hand `bytes` to the desktop IPC channel and count the frame against the
/// delivery gate. No-op when the session has no desktop channel.
///
/// Separate from [`send_grid_frame`] because the two callers differ in what the
/// frame IS: the broadcast path sends the shared delta to every transport, while
/// the ticker's stall repair sends a full frame to this channel and to nothing
/// else.
#[cfg(feature = "desktop")]
fn send_desktop_grid_frame(state: &AppState, session_id: &str, bytes: Vec<u8>) {
    let Some(ch) = state.grid.channels.get(session_id) else {
        return;
    };
    if !ch.visible.load(Ordering::Relaxed) {
        return;
    }
    let gate = state.grid.gates.get(session_id);
    if let Some(gate) = gate.as_deref() {
        if gate.is_paused() {
            gate.note_missed();
            return;
        }
        gate.mark_sent();
    }
    // `tauri::ipc::Response` is what keeps this binary. A `Vec<u8>` matches
    // only the blanket `IpcResponse` impl, i.e. `serde_json::to_string`, so a
    // 110 KB frame left Rust as a ~280 KB string of decimal numbers, took the
    // over-threshold path (one extra IPC round trip per frame) and arrived in
    // JS as a `number[]` to be walked back into bytes. `Response` carries the
    // bytes as `Raw` and the frontend already accepts an ArrayBuffer.
    if let Err(error) = ch.channel.send(tauri::ipc::Response::new(bytes)) {
        // A frame that never reached the webview will never be acked, and the
        // counters are absolute: leaving this one counted would put the gate one
        // frame behind for the rest of the session, i.e. every later frame would
        // travel at the ticker's 500 ms give-up rate. Give up on it now instead.
        tracing::debug!(session_id = %session_id, %error, "grid frame send failed");
        if let Some(gate) = gate.as_deref() {
            gate.abandon();
        }
    }
}

// --- Scroll commands ---

// DEFERRED (2026-08-18) — `set_ansi_colors` (above) is the fifth: it locks EVERY
// vt buffer in a loop on the IPC thread, so its stall grows with session count.
// Same reordering objection as below — two concurrent calls could leave some
// buffers on the old palette — and it fires once, when the user picks a theme.

// DEFERRED (2026-08-18) — the four grid commands that *mutate* before serializing
// (`terminal_scroll`, `terminal_scroll_to`, `terminal_request_frame`,
// `terminal_exit_alt_screen`) still take the vt lock inline on the IPC thread.
// They carry the same stall as the reads, but not the same safety: two
// `spawn_blocking` hops for the same session can run in either order, and
// `terminal_scroll_to(line)` is absolute — reordering two of them lands the
// viewport on the wrong line. The reads are idempotent, so they moved (F95);
// these need the coalescing `terminal_scroll_to_offset` already has, which is
// also why they are the low-frequency path: the wheel and the scrollbar drag go
// through the offset command and never touch this lock.

/// Run a read against a session's VT buffer on the blocking pool.
///
/// Every grid read takes the VT mutex, and the PTY reader holds that same mutex
/// through a full `serialize_dirty_rows`. A command that waits for it inline in
/// the IPC handler — on macOS, the main thread — freezes the WebView for the
/// length of someone else's serialize.
///
/// All of them go through here, including the ones that only read a single row.
/// The cost that matters is not the work the closure does, it is the wait for
/// the lock, and a one-cell read waits exactly as long as a whole-scrollback
/// search. A line drawn between "cheap" and "expensive" reads would only rot.
///
/// The lock is taken *inside* the closure, on the pool thread. Taking it before
/// the hop would put the wait straight back on the thread this exists to keep
/// free, which is the whole bug.
///
/// This is the shared unit between the two transports rather than the command:
/// the grid commands are `#[cfg(feature = "desktop")]`, so the HTTP routes —
/// which also compile into the headless `tuic-remote` binary — cannot call them.
/// Both call this instead, so neither transport can quietly go back to blocking.
///
/// `None` means the session is gone, which the desktop commands read as a
/// default and the HTTP routes as a 404.
pub(crate) async fn vt_try_read<T, F>(
    state: &Arc<AppState>,
    session_id: String,
    f: F,
) -> Result<Option<T>, String>
where
    F: FnOnce(&mut crate::state::VtLogBuffer) -> T + Send + 'static,
    T: Send + 'static,
{
    let state = Arc::clone(state);
    tokio::task::spawn_blocking(move || {
        state
            .grid
            .vt_log_buffers
            .get(&session_id)
            .map(|vt| f(&mut vt.lock()))
    })
    .await
    .map_err(|e| format!("terminal read failed: {e}"))
}

/// [`vt_try_read`] for the callers that answer a closed session with a default.
///
/// A tab can be closed while a hover, a selection or a row-cache fill is still
/// in flight; that is a race the frontend already tolerates, not an error worth
/// surfacing.
pub(crate) async fn vt_read<T, F>(
    state: &Arc<AppState>,
    session_id: String,
    f: F,
) -> Result<T, String>
where
    F: FnOnce(&mut crate::state::VtLogBuffer) -> T + Send + 'static,
    T: Default + Send + 'static,
{
    vt_try_read(state, session_id, f)
        .await
        .map(Option::unwrap_or_default)
}

// --- Search command ---

// --- Row text command ---

#[cfg(test)]
mod tests;

/// Read-only launch receipt evidence. Silence/output movement alone cannot prove
/// a new agent has reached its native composer.
pub(crate) fn managed_launch_screen_ready(state: &AppState, session_id: &str) -> bool {
    state
        .session_maps
        .silence_states
        .get(session_id)
        .is_some_and(|silence| silence.lock().cached_screen_activity == AgentScreenActivity::Ready)
}
