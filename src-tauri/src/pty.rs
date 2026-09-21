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

#[cfg(feature = "desktop")]
mod commands;
#[cfg(feature = "desktop")]
pub(crate) use commands::*;

// Not desktop-gated: shared by both the (desktop-only) Tauri command in
// `commands.rs` and the HTTP route in `mcp_http/session.rs`, which compiles
// regardless of the `desktop` feature.
mod explain;
pub(crate) use explain::*;

/// Get the platform-appropriate default shell when no override is configured.
pub(crate) fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "powershell.exe".to_string())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }
}

/// Convert a Windows drive-letter path to a WSL `/mnt/` path.
/// E.g. `C:\Users\foo\repos` → `/mnt/c/Users/foo/repos`.
/// Returns the input unchanged if it's not a Windows drive-letter path.
pub(crate) fn windows_to_wsl_path(path: &str) -> String {
    let bytes = path.as_bytes();
    // Match "X:\" or "X:/" where X is an ASCII letter
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        let drive = (bytes[0] as char).to_ascii_lowercase();
        let rest = &path[3..].replace('\\', "/");
        format!("/mnt/{drive}/{rest}")
    } else {
        path.to_string()
    }
}

/// Check whether a shell string targets WSL (e.g. `wsl.exe`, `wsl.exe -d Ubuntu`).
/// Handles both forward-slash and backslash path separators so it works
/// correctly regardless of compilation target (cross-compiled from macOS/Linux).
pub(crate) fn is_wsl_shell(shell: &str) -> bool {
    let exe = shell.split_whitespace().next().unwrap_or("");
    // Extract filename from the last path separator (either / or \)
    let filename = exe.rsplit(['/', '\\']).next().unwrap_or(exe);
    // Strip .exe extension if present
    let stem = filename
        .strip_suffix(".exe")
        .or_else(|| filename.strip_suffix(".EXE"))
        .unwrap_or(filename);
    stem.eq_ignore_ascii_case("wsl")
}

/// Remove parent-process preferences that must not become defaults for a new
/// independent PTY. Call this immediately after constructing the command so an
/// explicit per-agent environment may still restore the variable deliberately.
pub(crate) fn sanitize_pty_parent_env(cmd: &mut CommandBuilder) {
    // TUICommander may itself be launched from Codex, whose NO_COLOR belongs
    // to that parent process. Do not leak the opt-out into independent PTY
    // sessions. Commands can still request monochrome output through their own
    // explicit CLI flags or per-command environment.
    cmd.env_remove("NO_COLOR");
}

/// Inject the Unix-style env vars that Claude Code / Ink need to detect
/// terminal capabilities (color, kitty keyboard protocol, etc.).
/// Give the PTY the identity its agent will announce, and record which terminal
/// currently backs it.
///
/// Every session-creating path must call this. Before it existed only `create_pty`
/// injected `TUIC_SESSION`, so a tab opened through the worktree, agent-spawn or
/// HTTP paths ran with no identity at all: its bridge sent no `x-tuic-session`
/// header, the server minted an MCP-scoped UUID at `register`, and that UUID
/// matched no PTY — leaving the agent addressable by mail but unreachable through
/// its own terminal.
///
/// `tuic_session` is the caller's stable identity when it has one (a desktop tab
/// persists it across restarts for `claude --resume $TUIC_SESSION` and for goose's
/// `--name`). Paths without one fall back to the PTY key itself, which makes
/// identity and terminal trivially the same value for those sessions.
pub(crate) fn bind_pty_identity(
    state: &AppState,
    cmd: &mut CommandBuilder,
    session_id: &str,
    tuic_session: Option<&str>,
) {
    let identity = tuic_session.unwrap_or(session_id);
    cmd.env("TUIC_SESSION", identity);
    cmd.env(
        "TUIC_CONFIG_DIR",
        crate::config::config_dir().to_string_lossy().as_ref(),
    );
    state.bind_live_pty(identity, session_id);
}

/// Inject `TUIC_*` worktree/repo context (main checkout, branch, base ref,
/// etc. — see `script_env::ScriptContext`) into a PTY spawn, so a Run Script
/// typed into the new terminal — and every command a user types afterward —
/// can see it, the same way a Setup/Archive script or a Smart Prompt child
/// already can.
///
/// A PTY's env is fixed at spawn time: if the user later `cd`s to a different
/// worktree in this same tab, these vars keep describing the spawn cwd, not
/// wherever the shell currently is. `TUIC_SESSION` has the same property, so
/// this is consistent with the rest of the terminal's identity — don't try to
/// keep it live off OSC 7, a running process's environment can't be mutated
/// from outside it.
///
/// Every `bind_pty_identity` call site should also call this one, immediately
/// after, passing the same `cwd` the PTY itself is about to be spawned in.
pub(crate) fn inject_worktree_env(cmd: &mut CommandBuilder, cwd: Option<&str>) {
    let Some(cwd) = cwd else {
        return;
    };
    let expanded = crate::cli::expand_tilde(cwd);
    crate::script_env::ScriptContext::derive(
        crate::script_env::ScriptKind::Run,
        std::path::Path::new(&expanded),
    )
    .apply_pty(cmd);
}

fn inject_unix_terminal_env(cmd: &mut CommandBuilder) {
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    // Signal kitty keyboard protocol support so apps (e.g. Claude Code / Ink)
    // detect it via heuristic precheck and proceed to query confirmation.
    cmd.env("KITTY_WINDOW_ID", "1");
    // Announce as ghostty so Claude Code's terminal detection allow-list
    // enables kitty keyboard protocol. CC ≥v2.1.52 only recognizes
    // WezTerm, ghostty, and iTerm.app — "kitty" was removed from the list.
    // ghostty is chosen because it has no iTerm/WezTerm-specific side effects.
    // On macOS this also prevents /etc/zshrc sourcing zshrc_Apple_Terminal.
    cmd.env("TERM_PROGRAM", "ghostty");
    // iTerm2 feature-reporting protocol: advertise capabilities so tools
    // (cargo, uv, mise, etc.) can detect support without a TERM_PROGRAM whitelist.
    // T2=24-bit color, P=OSC 9;4 progress, H=OSC 8 hyperlinks, U=unicode,
    // B=bracketed paste, Sy=synchronized output, M=mouse, F=focus reporting.
    cmd.env("TERM_FEATURES", "T2PHUBSyMF");
    // CC also checks TERM_PROGRAM_VERSION — missing or matching /^[0-2]\./
    // causes rejection.  Use a value that passes the gate.
    cmd.env("TERM_PROGRAM_VERSION", "3.0.0");
    // Prevent nested-session detection when TUICommander itself runs
    // inside a Claude Code session (CLAUDECODE env var would propagate).
    cmd.env_remove("CLAUDECODE");
    if let Ok(lang) = std::env::var("LANG") {
        cmd.env("LANG", lang);
    } else {
        // Fallback: ensure UTF-8 is available even when LANG is completely unset
        cmd.env("LANG", "en_US.UTF-8");
    }
    // Agent Teams: always inject feature flag so CC unlocks team tools
    cmd.env("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS", "1");
    // Bare `imgcat`/`imgls`/`divider` on PATH (color-tools plan, Phase 9) —
    // `tuic imgcat`/etc. already work without this; these shims exist purely
    // so a user (or script) coming from iTerm2's own conventions finds the
    // bare names too. Silently absent if the `tuic` sidecar can't be
    // resolved (e.g. a from-source checkout with no built sidecar) — never
    // worth failing a PTY spawn over.
    if let Some(shim_dir) = crate::image_cli_shims::shim_dir() {
        let existing_path = std::env::var("PATH").unwrap_or_default();
        cmd.env(
            "PATH",
            format!("{}:{existing_path}", shim_dir.to_string_lossy()),
        );
    }
}

#[cfg(test)]
mod inject_unix_terminal_env_tests {
    use super::*;

    /// The image-CLI shim dir, when resolvable, must be *prepended* (not
    /// appended or set alone) so it wins over any same-named binary already
    /// on the user's `PATH`, while every existing entry is preserved.
    ///
    /// `image_cli_shims::shim_dir()` caches its result in a process-wide
    /// `OnceLock` on first call, so the `set_config_dir_override` below only
    /// actually redirects it away from the real config dir when THIS is the
    /// first call in the process — true under `cargo nextest run` (one
    /// process per test, the repo's real gate), not guaranteed under a bare
    /// `cargo test` if another test calls `shim_dir()` first. Same caveat
    /// `config.rs`'s own `CONFIG_DIR_OVERRIDE` tests already carry.
    #[test]
    fn prepends_the_image_shim_dir_to_path_when_resolvable() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());

        // This test only asserts something meaningful when the dev/test
        // environment can actually resolve a `tuic` sidecar (same
        // requirement `resolve_sidecar_path` has everywhere else) — skip
        // cleanly rather than fail the build in an environment with none.
        let Some(shim_dir) = crate::image_cli_shims::shim_dir() else {
            return;
        };
        let mut cmd = CommandBuilder::new("/bin/sh");
        inject_unix_terminal_env(&mut cmd);
        let path = cmd
            .get_env("PATH")
            .expect("PATH must be set when a shim dir was resolved")
            .to_string_lossy()
            .to_string();
        assert!(
            path.starts_with(&shim_dir.to_string_lossy().to_string()),
            "shim dir must be the first PATH entry, got {path}"
        );
    }
}

/// Attempts made before a PTY spawn is reported as failed.
pub(crate) const PTY_SPAWN_ATTEMPTS: usize = 3;

/// Open a PTY pair and spawn a command into it, retrying transient allocation failures.
///
/// Story 059 added this retry to `create_pty` after a spawn regression, but the
/// other six production spawn sites kept a single `openpty`/`spawn_command` and
/// failed hard — so whether a burst of tab creation survived a momentarily
/// exhausted PTY table depended on *which* code path opened the terminal. This
/// helper is deliberately the retry policy and nothing else: the sites diverge
/// for real reasons (dimension clamping, shell-integration injection, env
/// sanitising, cwd inheritance) and unifying past this point would force a false
/// abstraction.
///
/// Command-spawn failures are never retried: invalid binaries, cwd, permissions,
/// and arguments do not become valid after sleeping. Async entry points use the
/// companion async wrapper so this bounded blocking backoff runs only on Tokio's
/// blocking pool.
///
/// This is also the one place `TUIC_PTY_TTY` gets stamped onto the child's
/// environment — every production caller opens its pty here, so it's the only
/// point that has the master handle (and therefore `tty_name()`) available
/// *and* is guaranteed to run after every caller's own `cmd.env(...)` calls,
/// which happen inside `build_command()`.
pub(crate) fn spawn_pty_pair_with_retry<F>(
    size: PtySize,
    build_command: F,
) -> Result<
    (
        portable_pty::PtyPair,
        Box<dyn portable_pty::Child + Send + Sync>,
    ),
    String,
>
where
    F: FnOnce() -> CommandBuilder,
{
    let pty_system = native_pty_system();
    let pair = retry_transient(
        || pty_system.openpty(size),
        is_transient_pty_open_error,
        |attempt| {
            std::thread::sleep(std::time::Duration::from_millis(100 * attempt as u64));
        },
    )
    .map_err(|(attempt, error)| format!("Failed to open PTY (attempt {attempt}): {error}"))?;

    let mut cmd = build_command();
    // Claude Code (and the other agents this drives) spawns its hook
    // subprocesses detached from any controlling terminal, so a hook cannot
    // discover this tty by walking its own ancestry (see
    // `crates/tuic-hook/src/tty.rs`). We created the pty and already know its
    // device path — hand it over explicitly rather than make the hook guess.
    // `tty_name()` is `#[cfg(unix)]`; hook instrumentation stays unvalidated
    // on Windows regardless (see `tty.rs`'s own Windows fallback comment).
    //
    // Two known, currently-unhandled limitations, named here rather than
    // rediscovered: (1) no multi-pane/tmux disambiguation — two agents in two
    // tmux panes under one outer pty share one TUIC_PTY_TTY, with no way to
    // attribute an emission to the right pane; (2) device-path staleness — an
    // escaped daemonized grandchild can hold a stale TUIC_PTY_TTY after its
    // pty closes, and the OS can reassign that device path to an unrelated
    // later session. Both narrow and low-severity; see todo.md.
    #[cfg(unix)]
    if let Some(tty) = pair.master.tty_name() {
        cmd.env("TUIC_PTY_TTY", tty.to_string_lossy().as_ref());
    }

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|error| format!("Failed to spawn shell: {error}"))?;
    Ok((pair, child))
}

fn retry_transient<T, E, O, C, S>(
    mut operation: O,
    is_transient: C,
    mut sleep_before_retry: S,
) -> Result<T, (usize, E)>
where
    O: FnMut() -> Result<T, E>,
    C: Fn(&E) -> bool,
    S: FnMut(usize),
{
    for attempt in 1..=PTY_SPAWN_ATTEMPTS {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) if attempt < PTY_SPAWN_ATTEMPTS && is_transient(&error) => {
                sleep_before_retry(attempt);
            }
            Err(error) => return Err((attempt, error)),
        }
    }
    unreachable!("bounded retry loop always returns")
}

fn is_transient_pty_open_error(error: &anyhow::Error) -> bool {
    let Some(io_error) = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<std::io::Error>())
    else {
        return false;
    };
    if matches!(
        io_error.kind(),
        std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
    ) {
        return true;
    }
    let Some(code) = io_error.raw_os_error() else {
        return false;
    };
    #[cfg(unix)]
    if matches!(
        code,
        libc::EAGAIN | libc::EINTR | libc::EMFILE | libc::ENFILE | libc::ENOSPC | libc::ENXIO
    ) {
        return true;
    }
    #[cfg(windows)]
    if matches!(code, 8 | 14 | 170 | 1450 | 1816) {
        return true;
    }
    false
}

/// Run the synchronous PTY allocation policy without occupying an async worker.
pub(crate) async fn spawn_pty_pair_with_retry_async<F>(
    size: PtySize,
    build_command: F,
) -> Result<
    (
        portable_pty::PtyPair,
        Box<dyn portable_pty::Child + Send + Sync>,
    ),
    String,
>
where
    F: FnOnce() -> CommandBuilder + Send + 'static,
{
    run_pty_spawn_blocking(move || spawn_pty_pair_with_retry(size, build_command)).await
}

async fn run_pty_spawn_blocking<T, F>(operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| format!("PTY spawn task panicked: {error}"))?
}

/// Build a CommandBuilder for the given shell with platform-appropriate flags.
///
/// The `shell` string may contain arguments (e.g. `wsl.exe -d Ubuntu`).
/// The first whitespace-delimited token is the executable; the rest are args.
pub(crate) fn build_shell_command(shell: &str) -> CommandBuilder {
    let mut parts = shell.split_whitespace();
    let exe = parts.next().unwrap_or(shell);
    #[allow(unused_mut)]
    let mut cmd = CommandBuilder::new(exe);
    sanitize_pty_parent_env(&mut cmd);
    for arg in parts {
        cmd.arg(arg);
    }

    #[cfg(not(windows))]
    {
        // Login shell flag is Unix-only; PowerShell/cmd.exe don't support -l
        cmd.arg("-l");
        inject_unix_terminal_env(&mut cmd);
    }

    #[cfg(windows)]
    {
        // On Windows, if the shell targets WSL, inject Unix-style env vars
        // so that tools inside WSL (Claude Code, etc.) detect terminal
        // capabilities correctly. These are passed through to the Linux
        // environment by wsl.exe.
        if is_wsl_shell(shell) {
            inject_unix_terminal_env(&mut cmd);
        }
    }

    cmd
}

/// Niceness applied to every PTY child process. A child inherits the parent's
/// nice value at fork time, so deprioritizing the shell deprioritizes every
/// process it later spawns — compilers, bundlers, test runners. The intent is
/// that a heavy `cargo build` yields CPU to TUIC's own render thread and the
/// rest of the system *under contention*, while still running at full speed on
/// an idle machine (`nice` only bites when something else wants the core).
///
/// +10 was chosen over macOS QoS-background (`taskpolicy -b`), which pins the
/// workload to the E-cores on Apple Silicon and makes builds crawl even when
/// the P-cores are idle.
///
/// Overridable at launch via `TUIC_PTY_NICE` so the right value can be tuned on
/// the real app without recompiling (nice 0..19; values outside that range are
/// clamped by the kernel).
#[cfg(unix)]
const PTY_CHILD_NICE_DEFAULT: i32 = 10;

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
/// `spawn_session_for_agent`, `register_pty_session`
/// (`mcp_http::session`, itself shared by `spawn_pty_session`, the HTTP
/// agent-spawn route, and MCP `agent action=spawn`), and `agent::spawn_agent`.
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
) {
    state.emit_dual(crate::state::AppEvent::SessionCreated {
        session_id: session_id.to_string(),
        cwd,
        agent_type,
        display_name,
    });
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

/// Resolve the nice value to apply to PTY children: `TUIC_PTY_NICE` env override
/// if set and parseable, else [`PTY_CHILD_NICE_DEFAULT`].
#[cfg(unix)]
fn pty_child_nice() -> i32 {
    std::env::var("TUIC_PTY_NICE")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(PTY_CHILD_NICE_DEFAULT)
}

/// Lower the scheduling priority of a freshly-spawned PTY child so the workloads
/// it spawns don't starve TUIC and the system.
///
/// Failure is logged and ignored: a build at the default priority is a degraded
/// experience, not a broken one. Lowering priority on a process owned by the
/// same user is always permitted, so a non-zero return here is unexpected.
///
/// Unix (macOS, Linux): `setpriority` to nice +10.
#[cfg(unix)]
fn lower_pty_child_priority(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    let nice = pty_child_nice();
    // SAFETY: setpriority takes scalar args and is async-signal-safe; `pid` is
    // the id of the child we just spawned.
    let rc = unsafe { libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, nice) };
    if rc != 0 {
        tracing::warn!(
            pid,
            nice,
            error = %std::io::Error::last_os_error(),
            "failed to lower PTY child priority"
        );
    }
}

/// Windows: `BELOW_NORMAL_PRIORITY_CLASS` — the priority-class analog of nice
/// +10. NOT `IDLE_PRIORITY_CLASS`, which only runs the process when the system
/// is otherwise idle (the Windows equivalent of macOS QoS-background) and would
/// make builds crawl. macOS/Windows lack hard CPU affinity that works on the
/// primary target, so priority lowering is the one strategy portable to all
/// three platforms.
#[cfg(windows)]
fn lower_pty_child_priority(pid: Option<u32>) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        BELOW_NORMAL_PRIORITY_CLASS, OpenProcess, PROCESS_SET_INFORMATION, SetPriorityClass,
    };
    let Some(pid) = pid else { return };
    // SAFETY: Win32 calls with scalar/handle args; `pid` is the id of the child
    // we just spawned. The handle is closed on every path once obtained.
    unsafe {
        let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if handle.is_null() {
            tracing::warn!(
                pid,
                error = %std::io::Error::last_os_error(),
                "failed to open PTY child to lower priority"
            );
            return;
        }
        if SetPriorityClass(handle, BELOW_NORMAL_PRIORITY_CLASS) == 0 {
            tracing::warn!(
                pid,
                error = %std::io::Error::last_os_error(),
                "failed to lower PTY child priority"
            );
        }
        CloseHandle(handle);
    }
}

#[cfg(not(any(unix, windows)))]
fn lower_pty_child_priority(_pid: Option<u32>) {}

/// macOS thread QoS for the interactive terminal path.
///
/// `lower_pty_child_priority` nices compiler/test workloads *down*, but on
/// Apple Silicon the scheduler is QoS-band driven: nice only reorders threads
/// *within* a band, so under a saturating `cargo build` our PTY reader, frame
/// ticker, and keystroke-write threads — all at default QoS — still waited
/// behind the compiler's many default-QoS worker threads. Raising our own
/// threads to USER_INTERACTIVE puts the interactive path in a higher band, the
/// lever that keeps typing/echo responsive under load (the native trick AppKit
/// apps like iTerm get for free on the foreground GUI thread).
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

/// Resolve the shell to use: explicit override > env default > platform default.
pub(crate) fn resolve_shell(override_shell: Option<String>) -> String {
    let shell = override_shell.unwrap_or_else(default_shell);
    crate::cli::expand_tilde(&shell)
}

/// Which family of shell is running inside a PTY.
///
/// Used by the frontend to decide whether control characters like Ctrl-U are
/// honoured (POSIX readline) or echoed literally (`cmd.exe`, PowerShell).
/// Classifying by the shell command rather than by host OS is the whole point
/// of story 1274-2e38: Git Bash, Cygwin, MSYS and WSL all run on Windows yet
/// support Ctrl-U, so a host-OS check alone is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ShellFamily {
    /// POSIX shell with readline semantics: sh, bash, zsh, fish, dash, ksh,
    /// and friends — including WSL (spawns a Linux shell) and Git Bash /
    /// Cygwin / MSYS (bash compiled for Windows).
    Posix,
    /// Native Windows shell that treats Ctrl-U as a literal character:
    /// cmd.exe, PowerShell, pwsh.
    WindowsNative,
    /// Shell basename didn't match any known set. Callers should fall back to
    /// the safer default for their host (on Windows: skip Ctrl-U; on
    /// Unix: send it).
    Unknown,
}

/// Classify a shell command string (as passed to `portable_pty`) into a
/// [`ShellFamily`]. Pure function — no I/O, no env lookups — so it's easy to
/// test against the set of strings the UI actually produces.
///
/// Parses the leading binary path first (supports Windows paths with spaces
/// like `C:\Program Files\Git\bin\bash.exe`), then matches the basename
/// case-insensitively with any `.exe` suffix stripped.
pub(crate) fn classify_shell(cmd: &str) -> ShellFamily {
    let trimmed = cmd.trim().trim_matches('"');
    // Locate the binary portion: if there's a case-insensitive `.exe`, take
    // everything up to and including it; otherwise split on first whitespace.
    // This keeps `C:\Program Files\...\bash.exe` intact while still trimming
    // trailing args like `wsl.exe -d Ubuntu`.
    let exe = match trimmed.to_ascii_lowercase().find(".exe") {
        Some(idx) => &trimmed[..idx + ".exe".len()],
        None => trimmed.split_whitespace().next().unwrap_or(""),
    };
    let filename = exe.rsplit(['/', '\\']).next().unwrap_or(exe);
    let stem = filename
        .strip_suffix(".exe")
        .or_else(|| filename.strip_suffix(".EXE"))
        .or_else(|| filename.strip_suffix(".Exe"))
        .unwrap_or(filename)
        .to_ascii_lowercase();

    match stem.as_str() {
        // POSIX shells (same set we pattern-match elsewhere in pty.rs)
        "sh" | "bash" | "zsh" | "fish" | "dash" | "ksh" | "ash" | "tcsh" | "csh" | "mksh" => {
            ShellFamily::Posix
        }
        // WSL spawns a Linux shell — readline semantics apply.
        "wsl" => ShellFamily::Posix,
        // Native Windows shells: Ctrl-U is not line-kill.
        "cmd" | "powershell" | "pwsh" => ShellFamily::WindowsNative,
        _ => ShellFamily::Unknown,
    }
}

/// How long the agent must be silent after printing a `?`-ending line before
/// we treat it as a question waiting for input. 10s is long enough to avoid
/// false positives from AI agents that pause while thinking between API calls.
const SILENCE_QUESTION_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(10);

/// Maximum non-`?` chunks allowed after a `?` candidate before considering it stale.
/// Claude Code prints 2-3 decoration chunks after a question (mode line, separator).
/// Anything beyond this threshold means the agent continued working — not waiting.
const STALE_QUESTION_CHUNKS: u32 = 10;

/// How long the agent must be silent after printing a tool-error line before
/// we treat it as a turn-ending error (fire `playError()`). Shorter than the
/// question threshold because tool errors are typically followed by immediate
/// turn end (no retry) — 5s is enough to rule out a same-chunk recovery.
const SILENCE_TOOL_ERROR_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(5);

/// How long a retry line ("Retrying … attempt N/M", "Unable to connect to API")
/// holds the agent BUSY after it was last seen. During an API connection-retry
/// loop the agent is mid-turn but its TUI freezes between attempts (the spinner
/// stops repainting while the network call blocks), producing no changed rows —
/// so the movement-based BUSY evidence (#446-596f) drops and the silence/ready
/// path would flip the session idle mid-retry. Each new attempt line re-arms the
/// hold; once retries stop (recovery or final failure) the hold self-expires and
/// idle detection resumes. Long enough to bridge a stalled TCP connect (~10s).
const AGENT_RETRY_HOLD: std::time::Duration = std::time::Duration::from_secs(15);

/// Detect a turn-ending tool-failure line like Claude Code's
/// `⎿  Error: Exit code 1`. Anchored to line-start with only non-letter,
/// non-quote prefix characters (whitespace, box-drawing glyphs) so source
/// code or markdown that merely quotes the literal `"Error: Exit code N"`
/// does NOT match — avoids false-positive red notifications when the user's
/// own pty.rs tests are displayed in a terminal.
fn is_tool_error_line(line: &str) -> bool {
    lazy_static::lazy_static! {
        static ref TOOL_ERROR_RE: regex::Regex =
            regex::Regex::new(r#"^[^A-Za-z"]*Error:\s*Exit code\s+\d+"#).unwrap();
    }
    TOOL_ERROR_RE.is_match(line)
}

/// Detect an in-flight API connection-retry line, e.g. Claude's subagent SDK
/// `Unable to connect to API (ECONNRESET) · Retrying in 0s · attempt 6/10` or
/// the stream-error `retrying 5/5` form. Presence of such a line means the agent
/// is still mid-turn (auto-retrying), not idle — see `AGENT_RETRY_HOLD`. The
/// `attempt N/M` / `N/M` counter is required so plain prose mentioning "retrying"
/// or a code line containing the string does not latch the session busy.
fn is_retry_line(line: &str) -> bool {
    lazy_static::lazy_static! {
        static ref RETRY_RE: regex::Regex = regex::Regex::new(
            r"(?i)(unable to connect to api|retrying\b[^\n]{0,40}attempt\s+\d+\s*/\s*\d+|retrying\s+\d+\s*/\s*\d+)"
        ).unwrap();
    }
    RETRY_RE.is_match(line)
}

/// How often the timer thread wakes up to check for silence.
const SILENCE_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// If the wall-clock gap between two consecutive silence-timer ticks exceeds
/// this threshold, the system was likely asleep (lid closed). The tick is
/// skipped and timestamps are reset so stale elapsed times don't trigger
/// false idle transitions or completion sounds for every terminal.
const SLEEP_WAKE_GAP: std::time::Duration = std::time::Duration::from_secs(5);

/// Grace period after a PTY resize during which parsed events (Question, RateLimit,
/// ApiError) are suppressed. The shell redraws visible output after SIGWINCH, which
/// would otherwise re-trigger notifications for content already on screen.
const RESIZE_GRACE: std::time::Duration = std::time::Duration::from_millis(1000);

/// How long after user input to ignore `?`-ending echo lines from the PTY.
const ECHO_SUPPRESS_WINDOW: std::time::Duration = std::time::Duration::from_millis(500);

/// Grace period after PTY session start during which notifications (Question,
/// RateLimit, ApiError) are suppressed. When a CLI tool replays conversation
/// history (e.g. `claude --continue`), the burst of historical output contains
/// old errors and questions that would otherwise trigger stale notifications.
/// The grace ends when output pauses for STARTUP_SETTLE_SILENCE seconds,
/// indicating the replay is over and live output is starting.
const STARTUP_SETTLE_SILENCE: std::time::Duration = std::time::Duration::from_secs(5);

/// Safety cap: startup grace never lasts longer than this, even if output
/// never pauses (e.g. continuous build log).
const STARTUP_GRACE_MAX: std::time::Duration = std::time::Duration::from_secs(120);

/// Shell idle threshold: 500ms without real PTY output → transition busy→idle.
/// Matches the frontend's previous 500ms setTimeout in checkIdle.
const SHELL_IDLE_MS: u64 = 500;

/// Agent idle threshold: 2.5s without real PTY output → transition busy→idle.
/// AI agents produce output in bursts with natural thinking pauses (>500ms).
/// Using the shell threshold causes visible blue→green→blue oscillation.
/// Combined with the 2s frontend debounce, this gives ~4.5s total hold.
const AGENT_IDLE_MS: u64 = 2500;

/// How long an *ambiguous* non-shell foreground (unrecognized, resolved only
/// via the run-config preset fallback — not a direct `classify_agent` match)
/// must persist before `session_states.agent_seen_running` latches. Guards
/// against a fast-failing intermediate wrapper hop (e.g. `direnv exec .
/// mytool` erroring out before `mytool` itself ever runs) prematurely
/// confirming a preset as "seen running," which would let a shell reappearing
/// moments later wipe it. A direct `classify_agent` match has no such
/// ambiguity and confirms immediately, no debounce. See
/// `get_session_foreground_process_impl`.
const AGENT_SEEN_RUNNING_CONFIRM_MS: u64 = 1000;

/// Retry horizon for the payload-free orchestrator mail notice after an
/// ambiguous PTY write. Ordinary payload injection remains non-retriable.
const ORCHESTRATOR_WAKE_UNCERTAIN_RETRY: std::time::Duration = std::time::Duration::from_secs(5);

/// A ready prompt must remain visible across multiple silence-timer ticks before
/// it can end an agent turn. Ink redraws are multi-chunk (erase, then repaint),
/// so a single snapshot can briefly show the prompt without its working row.
const AGENT_READY_CONFIRM: std::time::Duration = std::time::Duration::from_millis(1500);
/// Escape hatch for a launch-instrumented agent whose terminal-ready screen
/// remains stable after its authoritative completion signal was lost.
const PROTOCOL_STALE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Interrupt intent is only a hint: Ctrl-C/Escape may be ignored or handled
/// asynchronously. Keep it long enough to correlate the subsequent explicit
/// interrupted screen, then discard it without changing shell state.
const INTERRUPT_PENDING_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// How long a plain shell latched BUSY by OSC 133 must stay silent before its
/// foreground process group is inspected for a nested prompt. A command that is
/// genuinely running (build, test, `dd`) either prints inside this window or
/// keeps a non-shell process in the group, so the probe stays off the hot path
/// and costs nothing while work is actually happening.
const SHELL_PROMPT_PROBE_SILENCE_MS: u64 = 3_000;

/// Maximum time active_sub_tasks can block idle transition (30s).
/// If the parser sets active_sub_tasks > 0 but the agent exits or the
/// mode-line disappears without emitting count=0, the terminal would stay
/// busy forever. After this timeout with no real output, we force-clear
/// the stale counter and allow idle transition.
const SUBTASK_STALE_MS: u64 = 30_000;

/// AtomicU8 encoding for shell_states DashMap.
pub(crate) const SHELL_NULL: u8 = 0;
pub(crate) const SHELL_BUSY: u8 = 1;
pub(crate) const SHELL_IDLE: u8 = 2;

/// Wire representation of an observed shell state. `SHELL_NULL` means no
/// lifecycle evidence has arrived yet and must remain absent/starting rather
/// than being serialized as idle.
pub(crate) fn shell_state_wire(state: u8) -> Option<&'static str> {
    match state {
        SHELL_BUSY => Some("busy"),
        SHELL_IDLE => Some("idle"),
        _ => None,
    }
}

// Re-export from chrome module for use by this module and tests.
use crate::chrome::is_chrome_row;

/// Searches all changed rows (not just the last non-empty one) so a question row
/// is found even when a mode/status line with a higher row index arrives in the same chunk.
/// Applies content filters to reject lines that are clearly not questions (code comments,
/// diff context, markdown headers, code syntax).
pub(crate) fn extract_question_line(changed_rows: &[ChangedRow]) -> Option<String> {
    changed_rows
        .iter()
        .rev()
        .find(|r| !r.text.is_empty() && r.text.ends_with('?') && is_plausible_question(&r.text))
        .map(|r| r.text.clone())
}

/// Returns false for lines that are clearly not questions: code comments, diff context,
/// markdown headers, prompt-echoed user input, and lines containing code-specific syntax.
fn is_plausible_question(line: &str) -> bool {
    let trimmed = line.trim_start();
    if crate::output_parser::line_is_diff_or_code_context(line) {
        return false;
    }
    // Prompt-prefixed lines are user input echoed in the conversation, not agent questions.
    if is_prompt_line(trimmed) {
        return false;
    }
    // Comment/diff/markdown prefixes
    if trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with('*')
        || trimmed.starts_with('+')
        || trimmed.starts_with('-')
        || trimmed.starts_with('>')
    {
        return false;
    }
    // Code syntax markers — real questions don't contain these
    if line.contains("->") || line.contains("=>") || line.contains("::") {
        return false;
    }
    // Code try-syntax: word_or_> followed by (...)? — e.g. foo()?, bar(x)?, Vec<T>()?
    // But NOT human option parentheticals like (y/n)?, (yes/no)? where `(` is
    // preceded by whitespace or start-of-line, not a word character.
    lazy_static::lazy_static! {
        static ref CODE_TRY_RE: regex::Regex =
            regex::Regex::new(r"[\w>]\([^)]*\)\?").unwrap();
    }
    if CODE_TRY_RE.is_match(line) {
        return false;
    }
    true
}

/// Returns true if a changed_row text looks like a suggest token line.
/// Used to exclude suggest rows from "real output" classification so they
/// don't reset the silence timer or stale pending questions.
fn is_suggest_row(text: &str) -> bool {
    let t = text.trim();
    t.contains("suggest:") && t.contains('|')
}

/// Verify that a question candidate is still visible among the bottom rows of the
/// terminal screen. Returns true only if the exact question text appears as a
/// complete row (trimmed) within the last `max_bottom_rows` non-empty lines.
/// This prevents ghost notifications from stale `?` lines that have scrolled off.
pub(crate) fn verify_question_on_screen(
    screen_rows: &[String],
    question: &str,
    max_bottom_rows: usize,
) -> bool {
    let q = question.trim();
    screen_rows
        .iter()
        .rev()
        .filter(|r| !r.is_empty())
        .take(max_bottom_rows)
        .any(|r| {
            let t = r.trim();
            // Exact match or prefix match (question may be truncated/wrapped on screen)
            t == q || (!q.is_empty() && t.starts_with(q))
        })
}

use crate::chrome::{is_prompt_line, is_separator_line};

/// Returns true when the line is a TUIC protocol token (`suggest:` or `intent:`
/// with pipe-separated items). These are structural markers consumed by the
/// frontend, not agent chat content — they must be skipped by question detection.
fn is_protocol_token_line(text: &str) -> bool {
    let t = text.trim_start();
    (t.starts_with("suggest:") || t.starts_with("intent:")) && t.contains('|')
}

/// Returns the set of row indices occupied by a protocol token (including
/// terminal-wrapped continuation rows). A continuation row is a row that
/// immediately follows a `suggest:` or `intent:` row and contains `|` but
/// does NOT start a new token prefix. Used to exclude the entire suggest/intent
/// block from "last chat line" detection — without this, the continuation row
/// gets mistaken for real chat content and steals the question slot.
fn collect_protocol_token_indices(screen_rows: &[String]) -> std::collections::HashSet<usize> {
    let mut indices = std::collections::HashSet::new();
    for (i, row) in screen_rows.iter().enumerate() {
        if is_protocol_token_line(row) {
            indices.insert(i);
            // Walk forward to find continuation rows (wrapped by terminal width)
            for (j, row) in screen_rows.iter().enumerate().skip(i + 1) {
                let trimmed = row.trim();
                if trimmed.is_empty() {
                    break;
                }
                // Stop at rows that start a new protocol token or chat content
                if is_protocol_token_line(row)
                    || trimmed.starts_with('>')
                    || trimmed.starts_with('›')
                    || trimmed.starts_with('❯')
                    || trimmed.starts_with('●')
                    || trimmed.starts_with('⏺')
                {
                    break;
                }
                // A continuation row must contain the `|` separator — without
                // it, the row is regular text (like an answer) that happens
                // to follow the suggest line.
                if !trimmed.contains('|') {
                    break;
                }
                indices.insert(j);
            }
        }
    }
    indices
}

/// Find the last chat line above the prompt box and, if it is a plausible
/// `?`-ending question, return it. Suggest/intent protocol blocks (including
/// wrapped continuations) are transparently skipped because they sit between
/// the agent's question and the prompt but are not real chat content — the
/// agent emits the question first and the suggest arrives after.
///
/// Only the single last chat line is inspected. We deliberately do NOT walk
/// deeper looking for an older `?`: a multi-line scan would scavenge past
/// the current agent turn and pick up the user's own previous input (e.g.
/// `❯ tutto ok?`) or stale content from earlier in the conversation, firing
/// phantom notifications 10s after the reply.
pub(crate) fn find_last_chat_question(screen_rows: &[String]) -> Option<String> {
    let prompt_idx = screen_rows
        .iter()
        .enumerate()
        .rev()
        .find(|(_, row)| is_prompt_line(row))?
        .0;

    let protocol_indices = collect_protocol_token_indices(screen_rows);

    for i in (0..prompt_idx).rev() {
        if protocol_indices.contains(&i) {
            continue;
        }
        let trimmed = screen_rows[i].trim();
        if trimmed.is_empty() || is_separator_line(trimmed) || is_chrome_row(trimmed) {
            continue;
        }
        // First non-skip row above the prompt — this is the last chat line.
        // Check it for a question, otherwise give up: we do not scavenge
        // deeper into the buffer.
        if trimmed.ends_with('?') && is_plausible_question(trimmed) {
            return Some(trimmed.to_string());
        }
        return None;
    }
    None
}

/// Whether the screen has a current input box and, if so, whether the last chat
/// content above it is a question. This distinction matters to the silence
/// fallback: `None` from `find_last_chat_question` can mean either "no prompt
/// anchor" or "the current turn ends in non-question content". Only the former
/// may use a changed-row fallback; the latter must not scavenge an older question
/// from scrollback.
fn current_chat_question(screen_rows: &[String]) -> CurrentChatQuestion {
    if screen_rows.iter().any(|row| is_prompt_line(row)) {
        CurrentChatQuestion::PromptAnchored(find_last_chat_question(screen_rows))
    } else {
        CurrentChatQuestion::NoPromptAnchor
    }
}

#[derive(Debug, PartialEq, Eq)]
enum CurrentChatQuestion {
    NoPromptAnchor,
    PromptAnchored(Option<String>),
}

/// Relative strength of evidence backing a busy/idle/awaiting verdict. A
/// higher rank overrides a lower one; equal-or-lower rank evidence is
/// rejected rather than clobbering something stronger already recorded for
/// the opposite verdict (see `TurnEvidence::record_busy`/`record_idle`).
///
/// Ordering matches #744-138c: wall-clock silence is the weakest signal,
/// screen-content classification is stronger, a background-process check is
/// stronger still, and a turn-granular protocol marker (OSC 7770, a submitted
/// line on a ready-adapter agent, a `suggest:`/completion marker) is
/// authoritative.
///
/// Rank is about what a signal *knows*, not how it travelled (#745-8ff1).
/// OSC 133 is the cautionary case and is deliberately **not** Protocol rank:
/// it is shell integration, so `133;C` fires when a foreground command starts
/// and `133;D` when it exits — on a long-lived TUI agent, once at launch and
/// once at death. It knows a process is running and nothing about turns, so it
/// records at [`EvidenceRank::Screen`] and a stable Ready screen may close it.
///
/// [`EvidenceRank::Process`] appears on the idle side only, from the two
/// places that read the process table: the foreground probe (`"process"`,
/// `foreground_probe`) and the `"protocol-stale"` give-up. Nothing records it
/// for busy, and **child exit deliberately records nothing at all** — an exit
/// removes the session rather than transitioning it (#771-4733).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EvidenceRank {
    Silence,
    Screen,
    Process,
    Protocol,
}

/// A single piece of ranked evidence, with the detector name that produced it
/// (used for `activity_source` in transition logs) and when it was recorded.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Evidence {
    pub(crate) rank: EvidenceRank,
    pub(crate) source: &'static str,
    pub(crate) at: std::time::Instant,
}

/// The current turn's ranked evidence. Busy and idle are mutually exclusive —
/// recording one clears the other unless the incoming evidence is too weak to
/// outrank what is already held (see `record_busy`/`record_idle`). Awaiting is
/// independent (a session can be busy or idle while a question is pending).
///
/// This is the single model #744-138c replaces the nine independently
/// mutated `SilenceState` booleans with: `completion_declared` and
/// `explicit_idle` both become Protocol-rank `idle` evidence with different
/// `source` tags; `explicit_busy`/`hook_busy`/`turn_started_by_input` become
/// Protocol-rank `busy` evidence tagged `"hook-busy"`/`"osc133-busy"`/
/// `"user-submit"`; `idle_confirmed` is derived from the recorded idle
/// evidence's rank/source rather than stored; `ready_since` is the `at` of a
/// `"agent-ready-screen"` idle evidence (preserved across repeated
/// observations, reset by any other evidence — see `record_idle`).
#[derive(Debug, Clone, Default)]
pub(crate) struct TurnEvidence {
    busy: Option<Evidence>,
    idle: Option<Evidence>,
    awaiting: Option<Evidence>,
    activity_seen: bool,
}

impl TurnEvidence {
    /// Record busy evidence. Rejected (no-op, returns `false`) if idle
    /// evidence of strictly higher rank is already held — e.g. a stale
    /// Working screen row (`Screen` rank) cannot reopen a turn an explicit
    /// OSC idle marker or a declared completion (`Protocol` rank) already
    /// closed, unless the caller has already decided the reopen is valid and
    /// passes an elevated rank for it (see `apply_working_evidence`).
    fn record_busy(&mut self, rank: EvidenceRank, source: &'static str) -> bool {
        if self.idle.is_some_and(|idle| idle.rank > rank)
            || self.busy.is_some_and(|busy| busy.rank > rank)
        {
            return false;
        }
        self.busy = Some(Evidence {
            rank,
            source,
            at: std::time::Instant::now(),
        });
        self.idle = None;
        true
    }

    /// Record idle evidence. Rejected if busy evidence of strictly higher
    /// rank is already held. `at` is preserved across repeated observations
    /// of the same (rank, source) — this is the `AGENT_READY_CONFIRM`
    /// debounce clock a caller reads via the returned `Evidence`.
    fn record_idle(&mut self, rank: EvidenceRank, source: &'static str) -> bool {
        if self.busy.is_some_and(|busy| busy.rank > rank)
            || self.idle.is_some_and(|idle| idle.rank > rank)
        {
            return false;
        }
        let at = match self.idle {
            Some(existing) if existing.rank == rank && existing.source == source => existing.at,
            _ => std::time::Instant::now(),
        };
        self.idle = Some(Evidence { rank, source, at });
        self.busy = None;
        true
    }

    /// Drop any held idle evidence and its debounce clock without recording
    /// new busy evidence. Used when a detector determines its own evidence is
    /// currently invalid (e.g. an unstable/unknown screen, or a ready screen
    /// gated by `injection_delivery_uncertain`/API-retry/no-activity-yet).
    fn clear_idle(&mut self) {
        self.idle = None;
    }

    /// Record idle evidence unconditionally, bypassing the busy-rank gate in
    /// `record_idle`. Used only by call sites that have already performed
    /// their own precise, narrower busy-evidence gate (e.g. `note_ready_screen`
    /// only withholds ready-confirmation for `"hook-busy"`/`"user-submit"`
    /// busy evidence with no activity seen yet — a bare `"osc133-busy"`
    /// marker must NOT block screen-confirmed readiness, unlike the generic
    /// rank gate `record_idle` applies for e.g. the silence-timeout fallback).
    fn force_idle(&mut self, rank: EvidenceRank, source: &'static str) -> Evidence {
        let at = match self.idle {
            Some(existing) if existing.rank == rank && existing.source == source => existing.at,
            _ => std::time::Instant::now(),
        };
        let evidence = Evidence { rank, source, at };
        self.idle = Some(evidence);
        self.busy = None;
        evidence
    }

    /// Record awaiting (question/dialog) evidence. Rejected if awaiting
    /// evidence of strictly higher rank is already held — this is the
    /// generic form of the old state.rs sticky guard "a low-confidence
    /// (silence-heuristic) question must not overwrite an already-active
    /// high-confidence one": confident sources are `Protocol` rank, heuristic
    /// ones `Screen` rank, so a `Screen` observation is rejected while a
    /// `Protocol` one is held, and any same-or-higher rank observation
    /// updates (a confident question's text can still change).
    pub(crate) fn record_awaiting(&mut self, rank: EvidenceRank, source: &'static str) -> bool {
        if self.awaiting.is_some_and(|existing| existing.rank > rank) {
            return false;
        }
        self.awaiting = Some(Evidence {
            rank,
            source,
            at: std::time::Instant::now(),
        });
        true
    }

    pub(crate) fn clear_awaiting(&mut self) {
        self.awaiting = None;
    }

    /// The rank of the currently recorded awaiting evidence, if any. Used by
    /// callers that only clear on a WEAK (non-`Protocol`) awaiting verdict —
    /// mirrors the old `!question_confident` guard in state.rs's status-line
    /// and question-cleared handling.
    pub(crate) fn awaiting_rank(&self) -> Option<EvidenceRank> {
        self.awaiting.map(|a| a.rank)
    }

    /// True while the recorded idle evidence is strong/current enough to act
    /// on downstream (standby, peer injection): an explicit protocol marker
    /// or a screen adapter's confirmed ready/interrupted state, or a plain
    /// (non-agent) shell's silence timeout. An agent silence-timeout with no
    /// screen confirmation is NOT confirmed — mirrors the old `idle_confirmed`.
    fn idle_confirmed(&self) -> bool {
        match self.idle {
            Some(Evidence {
                rank: EvidenceRank::Silence,
                source,
                ..
            }) => source == "silence-timeout-shell",
            Some(_) => true,
            None => false,
        }
    }
}

/// The verdict `decide()` reaches for the busy/idle shell-state axis.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Transition {
    ToBusy(Evidence),
    ToIdle(Evidence),
}

/// The single arbiter every busy/idle transition site routes through
/// (#744-138c). Pure: given the evidence recorded so far and whether the
/// shell is currently busy, says whether — and on what evidence — it should
/// flip. All the interesting gating (rank comparisons against the opposite
/// verdict, debounce, staleness) already happened when the evidence was
/// recorded (`record_busy`/`record_idle`); this function only compares the
/// surviving evidence against the current shell state.
fn decide(
    evidence: &TurnEvidence,
    shell_is_busy: bool,
    _now: std::time::Instant,
) -> Option<Transition> {
    if shell_is_busy {
        evidence.idle.map(Transition::ToIdle)
    } else {
        evidence.busy.map(Transition::ToBusy)
    }
}

/// What kind of evidence-recorder call a [`TrailEntry`] describes. Distinct
/// from [`EvidenceRank`] (which axis, how strong) — this is which recorder
/// was invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrailKind {
    /// `record_busy`.
    Busy,
    /// `record_idle`, or `force_idle` (`forced` distinguishes the two).
    Idle,
    /// A one-shot busy-evidence clear (the `evidence.busy = None` sites) —
    /// not itself rank-gated, so always `accepted: true`, no `outranked_by`.
    ClearBusy,
    /// `clear_idle` — drops idle evidence without recording busy.
    ClearIdle,
    /// `record_awaiting`.
    Awaiting,
    /// `clear_awaiting` — not itself rank-gated, always `accepted: true`.
    ClearAwaiting,
    /// `note_user_submission`'s turn boundary. Carries no rank/source of its
    /// own; a reader segments the trail into turns by this marker instead of
    /// a plumbed epoch (deliberately not added — see `DecisionTrail`'s doc
    /// comment).
    UserSubmit,
}

/// One entry in a [`DecisionTrail`]. Every field is `Copy`, so a push never
/// allocates. `rank`/`source` are `None` only for `ClearBusy`/`UserSubmit`,
/// which carry no evidence of their own.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TrailEntry {
    pub(crate) at: std::time::Instant,
    pub(crate) kind: TrailKind,
    pub(crate) rank: Option<EvidenceRank>,
    pub(crate) source: Option<&'static str>,
    /// Whether the recorder accepted this attempt. Always `true` for
    /// `force_idle` (bypasses the gate by design) and for the non-rank-gated
    /// kinds; `record_busy`/`record_idle` can be `false`.
    pub(crate) accepted: bool,
    /// True when this `Idle` entry came from `force_idle` rather than the
    /// gated `record_idle`. Always `false` for other kinds.
    pub(crate) forced: bool,
    /// The `(rank, source)` of the opposite-verdict evidence that rejected
    /// this attempt, when `accepted` is `false`. This is the single most
    /// useful field in the trail: a rejection is otherwise invisible, since
    /// every pre-existing call site discarded `record_busy`/`record_idle`'s
    /// returned `bool`.
    pub(crate) outranked_by: Option<(EvidenceRank, &'static str)>,
}

/// Fixed capacity of a [`DecisionTrail`] ring: ~48 bytes/entry, so 64 entries
/// is a few KB per session, bounded by `MAX_CONCURRENT_SESSIONS`.
const TRAIL_CAPACITY: usize = 64;

/// A bounded, always-on ring of the last [`TRAIL_CAPACITY`] evidence-recorder
/// calls a session's [`SilenceState`] made — including REJECTED attempts,
/// which `record_busy`/`record_idle`'s `bool` return makes invisible today
/// (every call site before this discarded it). This is exactly what a "why is
/// this session in the wrong state" dump needs: what was attempted, at what
/// rank, and — when rejected — what outranked it.
///
/// Lives here, as a sibling field of `SilenceState::evidence`, NOT inside
/// `TurnEvidence` itself: `TurnEvidence` is `Clone`d to escape the lock on the
/// PTY reader's hot path (`apply_working_evidence`, and the reader chunk path
/// — once per chunk), and today that clone is ~4 words of `Copy` data. A ring
/// inside it would deep-copy several KB on every chunk. `SilenceState` itself
/// is only ever reached behind its own mutex, so a push here costs one write
/// no existing caller wasn't already paying for — no extra lock, no
/// allocation (every field of `TrailEntry` is `Copy`).
///
/// Always-on by design, not gated behind a runtime toggle: a badge is
/// reported wrong *after* it's already stuck, so a trail you'd have to
/// enable first would be empty exactly when it's needed.
///
/// Carries no turn/session id: the recorders that push here (`SilenceState`
/// methods and their callers) have no epoch in scope, and plumbing one
/// through every call site was evaluated and rejected in favor of a `Copy`
/// ring plus a `UserSubmit` marker entry at the real turn boundary
/// (`note_user_submission`) — a reader segments the trail by that marker
/// instead. Do not "fix" this by threading an epoch through every recorder.
#[derive(Debug, Clone)]
pub(crate) struct DecisionTrail(std::collections::VecDeque<TrailEntry>);

impl DecisionTrail {
    fn new() -> Self {
        Self(std::collections::VecDeque::with_capacity(TRAIL_CAPACITY))
    }

    /// Collapses an exact repeat of the immediately-previous outcome (same
    /// `kind`/`rank`/`source`/`accepted`/`forced`/`outranked_by` — `at` is
    /// deliberately excluded from the comparison, see below) into that same
    /// entry instead of pushing a new one.
    ///
    /// Without this, a no-op call floods the bounded ring with duplicate
    /// noise on the PTY reader's hot path: `note_busy_evidence()`'s
    /// `clear_idle()` call fires on every "working"/"real activity" chunk
    /// regardless of whether there was any idle evidence to clear, and
    /// `record_busy` re-affirms the identical (rank, source) on every chunk
    /// of a long streaming turn. Confirmed by review: either alone can evict
    /// the entire 64-entry ring within under a second of active output,
    /// wiping out the one rejected-evidence entry ("outranked_by") someone
    /// opened the dump specifically to find — exactly the failure mode this
    /// trail exists to prevent.
    ///
    /// A repeat keeps the FIRST occurrence's `at`, not the latest — so its
    /// reported age answers "how long has this been true", not "when was it
    /// last redundantly re-recorded", which matches every other timestamp in
    /// this payload (e.g. `Evidence.at` has the identical "preserved across
    /// repeats" behavior in `record_idle`/`force_idle` for the same reason).
    fn push(&mut self, entry: TrailEntry) {
        if let Some(last) = self.0.back()
            && last.kind == entry.kind
            && last.rank == entry.rank
            && last.source == entry.source
            && last.accepted == entry.accepted
            && last.forced == entry.forced
            && last.outranked_by == entry.outranked_by
        {
            return;
        }
        if self.0.len() == TRAIL_CAPACITY {
            self.0.pop_front();
        }
        self.0.push_back(entry);
    }

    /// Snapshot for the state-explain payload, oldest first.
    pub(crate) fn entries(&self) -> impl Iterator<Item = &TrailEntry> {
        self.0.iter()
    }
}

impl Default for DecisionTrail {
    fn default() -> Self {
        Self::new()
    }
}

/// The last `Notification`-sourced `state=awaiting` classification this
/// session made — the same five values `process_chunk` already logs via
/// `tracing::debug!` ("Notification-sourced state=awaiting classified"), just
/// also kept for query instead of log-only. `notification_type` needs an
/// owned `String` (unlike the `&'static str` sources elsewhere in this file),
/// so this is deliberately NOT part of `DecisionTrail`'s `Copy` ring — a
/// `Notification` hook fire is rare, so one small allocation here is fine, as
/// long as it never rides the per-chunk path (it doesn't: this is written
/// only from the one call site that already computes all five values).
#[derive(Debug, Clone)]
pub(crate) struct NotificationClassification {
    pub(crate) at: std::time::Instant,
    pub(crate) notification_type: Option<String>,
    pub(crate) has_message: bool,
    pub(crate) shell_already_idle: bool,
    /// `Some(bool)` only when this fire actually produced a `Question` event
    /// (`payload == "awaiting"`); mirrors the `confident` local already
    /// logged at the call site.
    pub(crate) confident: Option<bool>,
    /// Whether the classification suppressed this fire outright (no event
    /// reached state.rs's reducer at all).
    pub(crate) suppressed: bool,
}

/// Shared state between the PTY reader thread and the silence-detection timer thread.
#[derive(Clone)]
pub(crate) struct SilenceState {
    /// When the last chunk of output was received from the PTY.
    pub(crate) last_output_at: std::time::Instant,
    /// The last line ending with `?` that hasn't been resolved yet.
    pub(crate) pending_question_line: Option<String>,
    /// Whether a Question event has already been emitted for the current pending line
    /// (either by the instant regex detector or by the silence timer).
    pub(crate) question_already_emitted: bool,
    /// When the last resize was requested. Used to suppress re-parsing of redrawn output.
    last_resize_at: Option<std::time::Instant>,
    /// Deadline until which `on_chunk` ignores `?`-ending lines (suppresses PTY echo).
    /// Set by `suppress_user_input()` so the echo of user-typed text doesn't
    /// re-enable silence-based question detection.
    pub(crate) suppress_echo_until: Option<std::time::Instant>,
    /// When the last chunk of ANY kind (real or chrome-only) was processed.
    /// Used by the backup idle timer to distinguish "no output at all" (reader
    /// blocked on read()) from "only chrome-only ticks arriving". The backup
    /// timer should only fire when truly no chunks arrive.
    pub(crate) last_chunk_at: std::time::Instant,
    /// When the last StatusLine (spinner) event was seen. If recent,
    /// silence-based question detection is suppressed — spinner means the agent is working.
    pub(crate) last_status_line_at: Option<std::time::Instant>,
    /// How many non-`?` chunks arrived after the current pending question candidate.
    /// Used to detect stale candidates: if the agent continued producing significant
    /// output after the `?` line, it was not a real question.
    output_chunks_after_question: u32,
    /// The text of the last question emitted (by silence timer or check_silence).
    /// Used to prevent re-emission of the same question when scrolling causes the
    /// `?` line to reappear in changed_rows at a different row position.
    /// Cleared on user input (new conversation cycle).
    last_emitted_text: Option<String>,
    /// When this session was created. Used with `startup_settled` to suppress
    /// notifications during the initial output burst (e.g. `--continue` replay).
    created_at: std::time::Instant,
    /// True once the session has settled after the initial output burst.
    /// Settled = output paused for STARTUP_SETTLE_SILENCE seconds, or
    /// STARTUP_GRACE_MAX has elapsed since creation.
    pub(crate) startup_settled: bool,
    /// The last `Error: Exit code N` line seen, awaiting silence verification.
    /// Cleared if real output (non-chrome, non-error) arrives — that means the
    /// agent recovered and the error is not turn-ending.
    pending_tool_error: Option<String>,
    /// Error lines already surfaced via `ToolError` in the current "input epoch"
    /// (since the last user line submit / session start). Persists across
    /// `clear_tool_error_on_recovery` so that scroll-induced reappearances of
    /// the same error in `changed_rows` do not re-fire the notification.
    /// Cleared on explicit user input so a recurring failure in a later turn
    /// can notify again.
    surfaced_tool_errors: std::collections::HashSet<String>,
    /// Parked request to reopen `OutputParser`'s error dedup. The parser lives
    /// in the reader thread's `ChunkProcessor` and never sees the input path, so
    /// a submitted line leaves the request here and `process_chunk` drains it
    /// before the next parse. Same "the user is engaging again" epoch as
    /// `surfaced_tool_errors`, for the parser-side half of the same dedup.
    parser_dedup_reset_pending: bool,
    /// Parsed `suggest:` items awaiting silence-based flush. The parser detects
    /// the token synchronously with output, but we hold the event here until
    /// `check_suggest` confirms the turn has ended (`SILENCE_SUGGEST_THRESHOLD`
    /// elapsed since the last real output chunk). Eliminates the frontend
    /// `pendingSuggest` race: the event never reaches the UI before idle.
    pending_suggest_items: Option<Vec<String>>,
    /// Input-turn epoch associated with `pending_suggest_items`.
    pending_suggest_turn_epoch: u64,
    /// Timestamp when `pending_suggest_items` was parked. Currently for
    /// diagnostics only — the flush decision is driven by `last_output_at`,
    /// not the park time.
    pending_suggest_at: Option<std::time::Instant>,
    /// The agent emitted the protocol's explicit end-of-task marker for the
    /// current input epoch. Unlike the pending item payload, this survives the
    /// one-shot Suggest event drain so status/list can distinguish completed
    /// work from a merely quiet ready prompt.
    ///
    /// Deliberately NOT folded into `TurnEvidence` (#744-138c): a `suggest:`
    /// marker is consulted by OTHER decisions (`apply_working_evidence`'s
    /// reopen gate, `completion_adjusted_screen_activity`) as "the agent
    /// already declared this turn done", but it must NOT itself flip the
    /// shell atomic to idle — it is parsed mid-output, often while the shell
    /// still reads BUSY. Recording it as `idle` evidence would make `decide()`
    /// treat parsing the marker as proof the shell is idle, which is not what
    /// today's behavior is.
    completion_declared: bool,
    /// Input-turn epoch that declared completion.
    completion_turn_epoch: u64,
    /// Claude's own hook payload (the `bgtasks` OSC 7770 verb, scraped from
    /// `background_tasks` on `Stop`/`StopFailure`) declared at least one
    /// background task still running as of `declared_background_work_turn_epoch`.
    /// Deliberately separate from `SessionState::background_work` (the OS
    /// process-tree observation): that field is demand-gated by a 1s
    /// process-tree refresher which would silently overwrite a hook-derived
    /// value on its very next tick (see `set_background_work_for_epoch_with_hook`).
    /// This declaration instead follows `completion_declared`'s shape exactly
    /// — single writer, epoch-stamped, self-expiring, untouched by any
    /// polling loop.
    declared_background_work: bool,
    /// Input-turn epoch that made the declaration above.
    declared_background_work_turn_epoch: u64,
    /// Ranked busy/idle/awaiting evidence for the current turn (#744-138c).
    /// Replaces eight independently-mutated booleans (explicit_busy, hook_busy,
    /// explicit_idle, idle_confirmed, turn_started_by_input, turn_activity_seen,
    /// ready_since, and the screen/protocol distinction previously spread
    /// across them) with one struct: every busy/idle/awaiting transition
    /// records `Evidence` here and is arbitrated by `decide()`, instead of
    /// each call site toggling its own subset of the old flags.
    evidence: TurnEvidence,
    /// Always-on ring of evidence-recorder calls, including rejections. See
    /// `DecisionTrail`'s doc comment for why it's a sibling of `evidence`
    /// rather than a field inside it.
    trail: DecisionTrail,
    /// The last `Notification`-sourced `state=awaiting` classification, kept
    /// for query. See `NotificationClassification`'s doc comment.
    pub(crate) last_notification_classification: Option<NotificationClassification>,
    /// True only after OSC 7770 `state=` was observed (OSC 133 shell markers do
    /// not prove that an agent's configured hooks are actually running). This
    /// is session-lifetime latch metadata, not per-turn evidence — it never
    /// resets, so it does not belong in `TurnEvidence`.
    hook_state_seen: bool,
    /// Last screen classification and when it was computed, shared between the
    /// reader chunk path (which computes it fresh on every chunk) and the
    /// silence timer (which reuses this instead of re-classifying, so
    /// `detect_agent_screen_activity` runs at most once per session per
    /// `SILENCE_CHECK_INTERVAL` — see `cached_screen_activity()`).
    cached_screen_activity: AgentScreenActivity,
    /// Recent user request to interrupt (Ctrl-C or bare Escape). This never
    /// changes shell state by itself; it only strengthens a matching interrupted
    /// screen emitted by the agent.
    interrupt_requested_at: Option<std::time::Instant>,
    /// Debounce clock for `note_ready_screen`: first observation of a stable
    /// agent ready prompt. Not part of `TurnEvidence` — it is a pending
    /// observation, not yet evidence; only committed via `force_idle` once
    /// stable for `AGENT_READY_CONFIRM`, so busy evidence is not cleared early.
    screen_ready_pending_since: Option<std::time::Instant>,
    /// Monotonic owner for an IDLE→BUSY transition reserved by terminal
    /// injection. The saved bool is the confirmed-idle value to restore only
    /// when no PTY byte was written and this claim still owns the state.
    active_injection_claim: Option<(u64, bool)>,
    next_injection_claim: u64,
    /// A payload may have been partially written or flushed without a complete
    /// Enter. Such sessions remain conservatively BUSY and are surfaced in
    /// status; automatic retry would risk duplicate or corrupted input.
    pub(crate) injection_delivery_uncertain: bool,
    injection_uncertain_since: Option<std::time::Instant>,
    injection_uncertainty_retryable: bool,
    /// Deadline until which an in-flight API connection-retry holds the agent
    /// BUSY. Armed by `mark_api_retry` when `is_retry_line` matches a changed
    /// row; blocks both the ready-screen and silence idle paths until it expires
    /// or is cleared by recovery/user input. See `AGENT_RETRY_HOLD`.
    api_retry_hold_until: Option<std::time::Instant>,
}

impl SilenceState {
    pub(crate) fn new() -> Self {
        Self {
            last_output_at: std::time::Instant::now(),
            pending_question_line: None,
            question_already_emitted: false,
            last_chunk_at: std::time::Instant::now(),
            last_resize_at: None,
            suppress_echo_until: None,
            last_status_line_at: None,
            output_chunks_after_question: 0,
            last_emitted_text: None,
            created_at: std::time::Instant::now(),
            startup_settled: false,
            pending_tool_error: None,
            surfaced_tool_errors: std::collections::HashSet::new(),
            parser_dedup_reset_pending: false,
            pending_suggest_items: None,
            pending_suggest_turn_epoch: 0,
            pending_suggest_at: None,
            completion_declared: false,
            completion_turn_epoch: 0,
            declared_background_work: false,
            declared_background_work_turn_epoch: 0,
            evidence: TurnEvidence::default(),
            trail: DecisionTrail::new(),
            last_notification_classification: None,
            hook_state_seen: false,
            cached_screen_activity: AgentScreenActivity::Unknown,
            interrupt_requested_at: None,
            screen_ready_pending_since: None,
            active_injection_claim: None,
            next_injection_claim: 0,
            injection_delivery_uncertain: false,
            injection_uncertain_since: None,
            injection_uncertainty_retryable: false,
            api_retry_hold_until: None,
        }
    }

    fn begin_injection_claim(&mut self, prior_idle_confirmed: bool) -> u64 {
        self.next_injection_claim = self.next_injection_claim.wrapping_add(1).max(1);
        let token = self.next_injection_claim;
        self.active_injection_claim = Some((token, prior_idle_confirmed));
        self.injection_delivery_uncertain = false;
        self.injection_uncertain_since = None;
        self.injection_uncertainty_retryable = false;
        token
    }

    fn commit_injection_claim(&mut self, token: u64) -> bool {
        if self
            .active_injection_claim
            .is_some_and(|(owner, _)| owner == token)
        {
            self.active_injection_claim = None;
            self.injection_delivery_uncertain = false;
            self.injection_uncertain_since = None;
            self.injection_uncertainty_retryable = false;
            true
        } else {
            false
        }
    }

    fn rollback_injection_claim(&mut self, token: u64) -> Option<bool> {
        let (_, prior_idle_confirmed) = self
            .active_injection_claim
            .filter(|(owner, _)| *owner == token)?;
        if self.evidence.activity_seen || self.busy_source_is("hook-busy") {
            self.active_injection_claim = None;
            return None;
        }
        self.active_injection_claim = None;
        self.injection_delivery_uncertain = false;
        self.injection_uncertain_since = None;
        self.injection_uncertainty_retryable = false;
        // Restore the pre-claim idle confirmation without restoring the exact
        // prior `Evidence` (not retained) — a synthetic marker reproducing the
        // same `idle_confirmed()` verdict is all any reader consults.
        let (rank, source) = if prior_idle_confirmed {
            (EvidenceRank::Protocol, "restored-confirmed-idle")
        } else {
            (EvidenceRank::Silence, "silence-timeout-agent")
        };
        self.force_idle(rank, source);
        Some(prior_idle_confirmed)
    }

    fn mark_injection_uncertain(&mut self, token: u64) {
        self.mark_injection_uncertain_with_retry(token, false);
    }

    fn mark_orchestrator_notice_uncertain(&mut self, token: u64) {
        self.mark_injection_uncertain_with_retry(token, true);
    }

    fn mark_injection_uncertain_with_retry(&mut self, token: u64, retryable: bool) {
        if self
            .active_injection_claim
            .is_some_and(|(owner, _)| owner == token)
        {
            self.active_injection_claim = None;
            self.injection_delivery_uncertain = true;
            self.injection_uncertain_since = Some(std::time::Instant::now());
            self.injection_uncertainty_retryable = retryable;
        }
    }

    fn invalidate_injection_claim(&mut self) {
        self.active_injection_claim = None;
        self.injection_delivery_uncertain = false;
        self.injection_uncertain_since = None;
        self.injection_uncertainty_retryable = false;
    }

    fn expire_orchestrator_notice_uncertainty(&mut self) -> bool {
        if !self.injection_delivery_uncertain
            || !self.injection_uncertainty_retryable
            || self
                .injection_uncertain_since
                .is_none_or(|since| since.elapsed() < ORCHESTRATOR_WAKE_UNCERTAIN_RETRY)
        {
            return false;
        }
        self.injection_delivery_uncertain = false;
        self.injection_uncertain_since = None;
        self.injection_uncertainty_retryable = false;
        self.screen_ready_pending_since = None;
        true
    }

    /// Any recorded busy evidence currently comes from `source` (one of the
    /// explicit markers: `"hook-busy"`/`"osc133-busy"`/`"user-submit"`).
    fn busy_source_is(&self, source: &str) -> bool {
        self.evidence.busy.is_some_and(|busy| busy.source == source)
    }

    /// An **explicit** busy marker (OSC hook/shell-integration, or a submitted
    /// line on a ready-adapter agent) is currently in effect. Narrower than
    /// "any busy evidence": screen/raw-activity evidence is deliberately
    /// one-shot (see `apply_working_evidence` and the reader chunk path) and
    /// never sets this, exactly as the old `explicit_busy` was never set by
    /// `note_working_screen`/`note_real_activity`.
    ///
    /// **This is a provenance predicate, not a rank predicate** — the three
    /// sources it accepts do not share a rank, and its doc used to claim they
    /// were all Protocol. They are not: `note_explicit_state` records
    /// `osc133-busy` at [`EvidenceRank::Screen`], because OSC 133 is *shell*
    /// integration. `133;C` fires when a foreground command starts and `133;D`
    /// when it exits, so on a long-lived TUI agent it fires once at launch and
    /// once at death — process-granularity evidence that knows nothing about
    /// turns. A caller deciding whether evidence may hold a turn against the
    /// screen wants `self.evidence.busy.rank`, never this. Reading this as a
    /// rank test is what produced the contradiction #745-8ff1 settled: it made
    /// a stuck `osc133-busy` outlive a stable Ready screen for the whole
    /// process, which is #535-d4f5.
    fn explicit_busy(&self) -> bool {
        matches!(
            self.evidence.busy.map(|b| b.source),
            Some("hook-busy" | "osc133-busy" | "user-submit")
        )
    }

    /// A Protocol-rank explicit idle marker (OSC hook/shell-integration) is
    /// the current idle evidence. Narrower than "any idle evidence": a
    /// screen-confirmed ready/interrupted state does not count — callers that
    /// need "is idle confirmed at all" want `idle_confirmed()` instead.
    fn explicit_idle(&self) -> bool {
        matches!(
            self.evidence.idle.map(|i| i.source),
            Some("hook-idle" | "osc133-idle")
        )
    }

    pub(crate) fn idle_confirmed(&self) -> bool {
        self.evidence.idle_confirmed()
    }

    /// Record awaiting (question/dialog) evidence — see
    /// `TurnEvidence::record_awaiting`. Exposed on `SilenceState` (rather than
    /// the `evidence` field, which stays private) so state.rs's PtyParsed
    /// dispatcher can share the same ranked model without pty.rs giving up
    /// direct control of the busy/idle axis. Wrapped (like `record_busy`/
    /// `record_idle`) to log a `DecisionTrail` entry, including rejections —
    /// a rejected awaiting attempt (a low-confidence screen-scrape question
    /// blocked by an already-held confident one) is exactly the kind of fact
    /// the trail exists to surface, and this axis is the one this repo's own
    /// AGENTS.md calls out repeatedly as the hardest to debug.
    /// Shared constructor for every `DecisionTrail` push site, so the 7-field
    /// `TrailEntry` literal (and the `at: Instant::now()` it always wants)
    /// lives in exactly one place. Adding a field to `TrailEntry` only means
    /// touching this one call, not each of the 8 sites that used to build it
    /// inline (a review finding: a future field would otherwise silently
    /// default/stale-out at any site the change missed).
    fn push_trail(
        &mut self,
        kind: TrailKind,
        rank: Option<EvidenceRank>,
        source: Option<&'static str>,
        accepted: bool,
        forced: bool,
        outranked_by: Option<(EvidenceRank, &'static str)>,
    ) {
        self.trail.push(TrailEntry {
            at: std::time::Instant::now(),
            kind,
            rank,
            source,
            accepted,
            forced,
            outranked_by,
        });
    }

    pub(crate) fn record_awaiting(&mut self, rank: EvidenceRank, source: &'static str) -> bool {
        let accepted = self.evidence.record_awaiting(rank, source);
        let outranked_by = (!accepted)
            .then(|| self.evidence.awaiting.filter(|a| a.rank > rank))
            .flatten()
            .map(|e| (e.rank, e.source));
        self.push_trail(
            TrailKind::Awaiting,
            Some(rank),
            Some(source),
            accepted,
            false,
            outranked_by,
        );
        accepted
    }

    pub(crate) fn clear_awaiting(&mut self) {
        self.evidence.clear_awaiting();
        self.push_trail(TrailKind::ClearAwaiting, None, None, true, false, None);
    }

    pub(crate) fn awaiting_rank(&self) -> Option<EvidenceRank> {
        self.evidence.awaiting_rank()
    }

    /// BUSY evidence came from an observed agent hook (OSC 7770 `state=busy`
    /// with a live hook), not a bare OSC 133 shell-integration marker.
    #[cfg(test)]
    fn hook_busy(&self) -> bool {
        self.busy_source_is("hook-busy")
    }

    /// A user/injected prompt started the current turn on a ready-adapter
    /// agent (`note_user_submission(true)`).
    #[cfg(test)]
    fn turn_started_by_input(&self) -> bool {
        self.busy_source_is("user-submit")
    }

    /// Wraps `TurnEvidence::record_busy`, appending a `DecisionTrail` entry —
    /// including on rejection, which the bare `bool` return leaves invisible
    /// to every existing call site. Every call to `evidence.record_busy` in
    /// this module goes through this wrapper instead, so the trail can never
    /// silently miss one.
    fn record_busy(&mut self, rank: EvidenceRank, source: &'static str) -> bool {
        let accepted = self.evidence.record_busy(rank, source);
        let outranked_by = (!accepted)
            .then(|| {
                self.evidence
                    .idle
                    .filter(|idle| idle.rank > rank)
                    .or(self.evidence.busy.filter(|busy| busy.rank > rank))
                    .map(|e| (e.rank, e.source))
            })
            .flatten();
        self.push_trail(
            TrailKind::Busy,
            Some(rank),
            Some(source),
            accepted,
            false,
            outranked_by,
        );
        accepted
    }

    /// Wraps `TurnEvidence::record_idle` — see `record_busy` above.
    fn record_idle(&mut self, rank: EvidenceRank, source: &'static str) -> bool {
        let accepted = self.evidence.record_idle(rank, source);
        let outranked_by = (!accepted)
            .then(|| {
                self.evidence
                    .busy
                    .filter(|busy| busy.rank > rank)
                    .or(self.evidence.idle.filter(|idle| idle.rank > rank))
                    .map(|e| (e.rank, e.source))
            })
            .flatten();
        self.push_trail(
            TrailKind::Idle,
            Some(rank),
            Some(source),
            accepted,
            false,
            outranked_by,
        );
        accepted
    }

    /// Wraps `TurnEvidence::force_idle` — always accepted, bypasses the
    /// rank gate by design, so `outranked_by` is always `None`.
    fn force_idle(&mut self, rank: EvidenceRank, source: &'static str) -> Evidence {
        let evidence = self.evidence.force_idle(rank, source);
        self.push_trail(TrailKind::Idle, Some(rank), Some(source), true, true, None);
        evidence
    }

    /// Wraps `TurnEvidence::clear_idle`.
    fn clear_idle(&mut self) {
        self.evidence.clear_idle();
        self.push_trail(TrailKind::ClearIdle, None, None, true, false, None);
    }

    /// One-shot busy-evidence clear (the `evidence.busy = None` call sites),
    /// recorded in the trail as `ClearBusy`. Callers already gate this on
    /// their own condition (e.g. "was this busy evidence's source the one
    /// this chunk just applied") — this only performs the clear and logs it.
    fn clear_busy_evidence(&mut self) {
        self.evidence.busy = None;
        self.push_trail(TrailKind::ClearBusy, None, None, true, false, None);
    }

    fn note_explicit_state(&mut self, state: u8, hook_state: bool) {
        self.invalidate_injection_claim();
        self.hook_state_seen |= hook_state;
        self.screen_ready_pending_since = None;
        match state {
            SHELL_BUSY => {
                let source = if hook_state {
                    "hook-busy"
                } else {
                    "osc133-busy"
                };
                let rank = if hook_state {
                    EvidenceRank::Protocol
                } else {
                    EvidenceRank::Screen
                };
                self.record_busy(rank, source);
                self.last_status_line_at = Some(std::time::Instant::now());
            }
            SHELL_IDLE => {
                let source = if hook_state {
                    "hook-idle"
                } else {
                    "osc133-idle"
                };
                let rank = if hook_state {
                    EvidenceRank::Protocol
                } else {
                    EvidenceRank::Screen
                };
                self.record_idle(rank, source);
                self.last_status_line_at = None;
                self.interrupt_requested_at = None;
                self.evidence.activity_seen = false;
            }
            _ => {}
        }
    }

    fn note_busy_evidence(&mut self) {
        self.clear_idle();
        self.screen_ready_pending_since = None;
    }

    fn note_working_screen(&mut self) {
        self.invalidate_injection_claim();
        self.note_busy_evidence();
        self.evidence.activity_seen = true;
        // Keep silence-based question/tool-error detection aligned with shell
        // activity. Previously the working marker refreshed last_output_ms but
        // not SilenceState, allowing contradictory question events.
        self.last_status_line_at = Some(std::time::Instant::now());
    }

    fn note_real_activity(&mut self) {
        self.invalidate_injection_claim();
        self.note_busy_evidence();
        self.evidence.activity_seen = true;
    }

    fn note_ready_screen(&mut self) -> bool {
        // Only Protocol-rank busy evidence may hold a turn against a stable
        // Ready screen. Deliberately `== Protocol` and not `>= Screen`: the
        // ladder is used asymmetrically here, because Process-rank evidence
        // (the foreground probe) is about the process, not the turn. The probe
        // runs AFTER this returns true and refines the recorded evidence from
        // `Screen` to `Process` (#771-4733); it never gets to override a
        // Protocol-rank hold, which is what this guard exists to protect.
        //
        // Do NOT widen this to accept `explicit_busy()`'s sources (#745-8ff1).
        // That set includes `osc133-busy`, which is shell integration and knows
        // only that a foreground command is running — on a long-lived TUI agent
        // it is set once at launch and cleared only at exit, so honouring it
        // here strands the tab BUSY for the whole process (#535-d4f5). The
        // three `*_recovers_long_lived_shell_busy` tests build a byte-identical
        // `SilenceState`, and a `SilenceState` carries no agent type, so they
        // must all agree; widening this guard turns grok green and goose and
        // opencode red, which moves the contradiction instead of resolving it.
        if self.evidence.busy.is_some_and(|busy| {
            busy.rank == EvidenceRank::Protocol
                && (busy.source != "user-submit" || !self.evidence.activity_seen)
        }) {
            self.screen_ready_pending_since
                .get_or_insert_with(std::time::Instant::now);
            return false;
        }
        if self.injection_delivery_uncertain || self.is_api_retry_active() {
            self.screen_ready_pending_since = None;
            return false;
        }
        let now = std::time::Instant::now();
        let since = *self.screen_ready_pending_since.get_or_insert(now);
        if since.elapsed() < AGENT_READY_CONFIRM {
            return false;
        }
        self.force_idle(EvidenceRank::Screen, "agent-ready-screen");
        self.last_status_line_at = None;
        self.interrupt_requested_at = None;
        self.evidence.activity_seen = false;
        true
    }

    fn note_interrupted_screen(&mut self) -> bool {
        let pending = self
            .interrupt_requested_at
            .is_some_and(|at| at.elapsed() < INTERRUPT_PENDING_TTL);
        if pending {
            self.force_idle(EvidenceRank::Screen, "interrupted-screen");
            self.last_status_line_at = None;
            self.screen_ready_pending_since = None;
            self.interrupt_requested_at = None;
            self.evidence.activity_seen = false;
            return true;
        }
        self.note_ready_screen()
    }

    fn note_unknown_screen(&mut self) {
        self.screen_ready_pending_since = None;
        if self
            .interrupt_requested_at
            .is_some_and(|at| at.elapsed() >= INTERRUPT_PENDING_TTL)
        {
            self.interrupt_requested_at = None;
        }
    }

    pub(crate) fn note_interrupt_requested(&mut self) {
        self.interrupt_requested_at = Some(std::time::Instant::now());
        self.screen_ready_pending_since = None;
    }

    pub(crate) fn note_user_submission(&mut self, protocol_instrumented: bool) {
        self.interrupt_requested_at = None;
        self.completion_declared = false;
        self.note_busy_evidence();
        // The real turn boundary — the trail carries no epoch of its own (see
        // `DecisionTrail`'s doc comment), so a reader segments it into turns
        // by this marker instead.
        self.push_trail(TrailKind::UserSubmit, None, None, true, false, None);
        if protocol_instrumented {
            self.record_busy(EvidenceRank::Protocol, "user-submit");
            self.last_status_line_at = Some(std::time::Instant::now());
            self.evidence.activity_seen = false;
        }
    }

    fn protocol_busy_is_stale(&self) -> bool {
        self.evidence.busy.is_some_and(|busy| {
            busy.rank == EvidenceRank::Protocol
                && busy.at.elapsed() >= PROTOCOL_STALE_TIMEOUT
                && self.last_output_at.elapsed() >= PROTOCOL_STALE_TIMEOUT
                && self
                    .screen_ready_pending_since
                    .is_some_and(|ready| ready.elapsed() >= PROTOCOL_STALE_TIMEOUT)
        })
    }

    #[cfg(test)]
    pub(crate) fn confirm_idle(&mut self) {
        self.force_idle(EvidenceRank::Protocol, "test-confirmed-idle");
    }

    /// Test-only: idle, but not confirmed (mirrors an agent silence-timeout
    /// with no ready-screen adapter — `idle_confirmed()` derives `false`).
    #[cfg(test)]
    pub(crate) fn force_idle_unconfirmed(&mut self) {
        self.force_idle(EvidenceRank::Silence, "silence-timeout-agent");
    }

    /// Called by resize_pty when the terminal is resized.
    /// Marks the start of a grace period during which parsed events are suppressed.
    pub(crate) fn on_resize(&mut self) {
        self.last_resize_at = Some(std::time::Instant::now());
    }

    /// Returns true if we are within the resize grace period.
    /// Parsed events (Question, RateLimit, ApiError) should be suppressed during this window
    /// because the shell redraws visible output after SIGWINCH, causing false re-detections.
    pub(crate) fn is_resize_grace(&self) -> bool {
        self.last_resize_at
            .map(|t| t.elapsed() < RESIZE_GRACE)
            .unwrap_or(false)
    }

    /// Returns true if we are still in the startup grace period.
    /// During this window, notifications are suppressed to avoid reacting to
    /// historical output replayed by `--continue` or similar session restore.
    pub(crate) fn is_startup_grace(&self) -> bool {
        !self.startup_settled
    }

    /// Check if the startup grace should end and update the flag.
    /// Called by the silence timer thread every second.
    pub(crate) fn check_startup_settle(&mut self) {
        if self.startup_settled {
            return;
        }
        // Safety cap: always settle after STARTUP_GRACE_MAX
        if self.created_at.elapsed() >= STARTUP_GRACE_MAX {
            self.startup_settled = true;
            self.pending_suggest_items = None;
            self.pending_suggest_at = None;
            return;
        }
        // Settle after STARTUP_SETTLE_SILENCE without output
        if self.last_output_at.elapsed() >= STARTUP_SETTLE_SILENCE {
            self.startup_settled = true;
            self.pending_suggest_items = None;
            self.pending_suggest_at = None;
        }
    }

    /// Called by the reader thread after each chunk.
    /// `regex_found_question`: true if `parse()` already emitted a Question event.
    /// `last_question_line`: the last `?`-ending line in the chunk, if any.
    /// `has_status_line`: true if the chunk contained a StatusLine parsed event.
    /// `status_line_only`: true if the chunk contained ONLY status-line/mode-line updates.
    ///   Mode-line timer ticks (elapsed time updating every second) are not significant
    ///   output — they must not reset the silence timer or the spinner timestamp,
    ///   or questions asked by Ink agents will never be detected.
    pub(crate) fn on_chunk(
        &mut self,
        regex_found_question: bool,
        last_question_line: Option<String>,
        has_status_line: bool,
        status_line_only: bool,
        suggest_only: bool,
    ) {
        // Always track that a chunk arrived — used by the backup idle timer
        // to distinguish "reader blocked on read()" from "chrome ticks arriving".
        self.last_chunk_at = std::time::Instant::now();

        // Suggest-only chunks are not significant output — they are protocol
        // tokens consumed by the frontend, not real agent text.
        let insignificant = status_line_only || suggest_only;

        if !insignificant {
            self.last_output_at = std::time::Instant::now();
        }

        // Only mark spinner active when the status line accompanies real output.
        // Mode-line timer ticks and suggest-only chunks are not agent activity
        // and must not suppress question detection.
        if has_status_line && !insignificant {
            self.last_status_line_at = Some(std::time::Instant::now());
        }

        // Within the echo suppress window, ignore `?`-ending lines — they are
        // the PTY echoing back user-typed text, not agent questions.
        let in_echo_window = self
            .suppress_echo_until
            .map(|deadline| std::time::Instant::now() < deadline)
            .unwrap_or(false);

        if regex_found_question {
            // The instant detector already fired — suppress the silence timer.
            self.pending_question_line = None;
            self.question_already_emitted = true;
            self.output_chunks_after_question = 0;
        } else if let Some(line) = last_question_line {
            if in_echo_window {
                // Ignore — this is the PTY echo of user input.
            } else if self.question_already_emitted
                && (self.pending_question_line.as_deref() == Some(&line)
                    || self.last_emitted_text.as_deref() == Some(&line))
            {
                // Same `?` text as already emitted (either still pending, or
                // previously emitted and reappearing because new output scrolled
                // it to a different row). Don't reset — otherwise the silence
                // timer will re-fire for every scroll of the same question.
            } else {
                // New candidate for silence-based detection.
                self.pending_question_line = Some(line);
                self.question_already_emitted = false;
                self.output_chunks_after_question = 0;
            }
        } else if self.pending_question_line.is_some() && !insignificant {
            // Non-`?` chunk with real output after a pending candidate — track staleness.
            // Insignificant chunks (mode-line ticks, suggest tokens) are NOT real output
            // and must not count toward staleness, or they will clear the pending question
            // before the silence timer has a chance to detect it.
            self.output_chunks_after_question = self.output_chunks_after_question.saturating_add(1);
            // Once stale, clear pending so the repaint guard won't block the
            // same question text from being detected again in a future session.
            if self.output_chunks_after_question > STALE_QUESTION_CHUNKS {
                self.pending_question_line = None;
            }
        }
    }

    /// Called by write_pty when the user submits a line of input.
    /// Clears any pending question candidate since it was typed by the user, not the agent.
    /// Also opens a time window to ignore the PTY echo of the typed text.
    pub(crate) fn suppress_user_input(&mut self) {
        self.pending_question_line = None;
        // A new turn may legitimately ask the same text again. Re-open the
        // screen-anchored strategy while retaining `last_emitted_text`; the
        // unanchored changed-row fallback uses that memory to reject historical
        // repaints of the prior turn.
        self.question_already_emitted = false;
        self.suppress_echo_until = Some(std::time::Instant::now() + ECHO_SUPPRESS_WINDOW);
    }

    /// Returns true if a spinner/status-line was seen recently.
    /// Uses the same threshold as silence detection (10s) so that agents with
    /// pauses between status-line updates (API calls, file reads) don't trigger
    /// false question notifications during those gaps.
    fn is_spinner_active(&self) -> bool {
        self.explicit_busy()
            || self
                .last_status_line_at
                .map(|t| t.elapsed() < SILENCE_QUESTION_THRESHOLD)
                .unwrap_or(false)
    }

    /// Returns true if any chunk (real or chrome-only) was received recently.
    /// The backup idle timer uses this to avoid false idle transitions when the
    /// reader thread IS processing chunks (even chrome-only status-line ticks).
    /// Status-line ticking proves the agent is alive — the backup timer should
    /// only fire when truly no chunks arrive (reader blocked on read()).
    /// The 2s threshold matches the frontend debounce hold (BUSY_HOLD_MS).
    #[allow(dead_code)] // called from tests; kept for backup-idle-timer reintegration
    pub(crate) fn has_recent_chunks(&self) -> bool {
        self.last_chunk_at.elapsed() < std::time::Duration::from_secs(2)
    }

    /// Called by the timer thread. Returns the question text if the silence
    /// threshold has been reached and we haven't emitted yet.
    pub(crate) fn check_silence(&mut self) -> Option<String> {
        if self.question_already_emitted {
            return None;
        }
        // Spinner active means the agent is working — not waiting for input.
        if self.is_spinner_active() {
            return None;
        }
        // Too much output after the `?` line — the agent continued working,
        // so the `?` was not a real question (e.g. code comment, markdown).
        if self.output_chunks_after_question > STALE_QUESTION_CHUNKS {
            return None;
        }
        if let Some(ref line) = self.pending_question_line
            && self.last_output_at.elapsed() >= SILENCE_QUESTION_THRESHOLD
        {
            if self.last_emitted_text.as_deref() == Some(line.as_str()) {
                return None;
            }
            self.question_already_emitted = true;
            self.last_emitted_text = Some(line.clone());
            return Some(line.clone());
        }
        None
    }

    /// Clear a stale question candidate that failed screen verification.
    /// Prevents the timer from re-checking the same stale candidate every second.
    pub(crate) fn clear_stale_question(&mut self) {
        self.pending_question_line = None;
        self.question_already_emitted = true;
    }

    /// Register an `Error: Exit code N` line seen in visible output. The silence
    /// timer will emit a ToolError event if the session goes idle without any
    /// real-output chunk clearing the candidate (= agent did not recover).
    ///
    /// Idempotent across scroll-induced re-appearances: if this exact line has
    /// already surfaced in the current input epoch, we drop it. Without this,
    /// Ink-based TUIs (Claude Code, Codex) cause `changed_rows` to include the
    /// old error line every time the viewport scrolls, re-arming the candidate
    /// and re-firing the red notification long after the user has resumed.
    pub(crate) fn mark_tool_error_candidate(&mut self, line: String) {
        if self.surfaced_tool_errors.contains(&line) {
            return;
        }
        if self.pending_tool_error.as_deref() == Some(&line) {
            return;
        }
        self.pending_tool_error = Some(line);
    }

    /// Arm (or re-arm) the API connection-retry BUSY hold. Called when
    /// `is_retry_line` matches a changed row: the agent is auto-retrying a failed
    /// API call and is still mid-turn even though its TUI has frozen between
    /// attempts. See `AGENT_RETRY_HOLD` for why this is needed.
    pub(crate) fn mark_api_retry(&mut self) {
        self.api_retry_hold_until = Some(std::time::Instant::now() + AGENT_RETRY_HOLD);
    }

    /// True while an in-flight API retry holds the agent BUSY. Consulted by the
    /// ready-screen and silence idle paths to suppress a premature idle flip.
    pub(crate) fn is_api_retry_active(&self) -> bool {
        self.api_retry_hold_until
            .is_some_and(|deadline| std::time::Instant::now() < deadline)
    }

    /// Called on every real-output chunk that is NOT an error line. Clears the
    /// pending tool-error candidate: if the agent produced real output after an
    /// error, it recovered (e.g. retry) and the error is not turn-ending.
    ///
    /// Does NOT reset `surfaced_tool_errors` — recovery is a transient backend
    /// signal; the user-facing "I've already told you about this error" state
    /// must survive it and only reset on explicit user input.
    pub(crate) fn clear_tool_error_on_recovery(&mut self) {
        self.pending_tool_error = None;
        // Real non-error, non-retry output means the agent recovered from the
        // connection-retry loop — release the BUSY hold so idle detection resumes.
        self.api_retry_hold_until = None;
    }

    /// Clear the "already surfaced" memory so the next occurrence of any error
    /// line — including one we've already fired — can notify again. Called
    /// from `write_pty` when the user submits a line (or Ctrl+C), mirroring
    /// the api-error dedup reset in `OutputParser::parse_clean_lines`.
    pub(crate) fn reset_tool_error_memory(&mut self) {
        self.pending_tool_error = None;
        self.surfaced_tool_errors.clear();
        self.api_retry_hold_until = None;
    }

    /// Park a request to reopen `OutputParser::reset_input_dedup`. Called on the
    /// input thread; the reader consumes it with `take_parser_dedup_reset` on the
    /// next chunk, which is the first moment the parser is reachable again.
    pub(crate) fn request_parser_dedup_reset(&mut self) {
        self.parser_dedup_reset_pending = true;
    }

    /// Consume a parked parser-dedup reset. Returns true at most once per
    /// submitted line.
    pub(crate) fn take_parser_dedup_reset(&mut self) -> bool {
        std::mem::take(&mut self.parser_dedup_reset_pending)
    }

    /// Called by the timer thread. Returns the error text if the silence
    /// threshold has been reached and we haven't emitted yet. Semantics mirror
    /// `check_silence` but use the shorter tool-error threshold.
    pub(crate) fn check_tool_error(&mut self) -> Option<String> {
        if self.is_spinner_active() {
            return None;
        }
        let should_fire = self.pending_tool_error.is_some()
            && self.last_output_at.elapsed() >= SILENCE_TOOL_ERROR_THRESHOLD;
        if !should_fire {
            return None;
        }
        let line = self.pending_tool_error.take()?;
        self.surfaced_tool_errors.insert(line.clone());
        Some(line)
    }

    /// Park `suggest:` items parsed from output. The silence timer will flush
    /// them to the frontend once the shell state transitions to idle — this
    /// is the single source of truth for "turn ended". A newer set overwrites
    /// an older pending set: if the agent updates its suggestions mid-turn,
    /// we deliver the latest.
    pub(crate) fn mark_suggest_candidate(&mut self, items: Vec<String>, turn_epoch: u64) {
        if items.is_empty() {
            return;
        }
        self.completion_declared = true;
        self.completion_turn_epoch = turn_epoch;
        self.pending_suggest_items = Some(items);
        self.pending_suggest_turn_epoch = turn_epoch;
        self.pending_suggest_at = Some(std::time::Instant::now());
    }

    fn drain_pending_suggest_with_epoch(&mut self) -> Option<(u64, Vec<String>)> {
        self.pending_suggest_at = None;
        self.pending_suggest_items
            .take()
            .map(|items| (self.pending_suggest_turn_epoch, items))
    }

    /// Drain parked suggest items. No gates — trust the caller to invoke only
    /// when the shell state is IDLE (the silence timer does exactly that).
    /// Returns the items once and clears the park slot; a second call returns
    /// `None` until new items are parked.
    #[cfg(test)]
    pub(crate) fn drain_pending_suggest(&mut self) -> Option<Vec<String>> {
        self.drain_pending_suggest_with_epoch()
            .map(|(_, items)| items)
    }

    /// Drop any parked suggest on user input. Parallels `reset_tool_error_memory`:
    /// the user is engaging again, so stale suggestions from the previous turn
    /// must not fire after a new input cycle starts.
    ///
    /// Deliberately does NOT touch `declared_background_work` — see
    /// `reset_declared_background_work`'s doc comment for why that's a
    /// separate method, not folded in here. This method is also called from
    /// `apply_working_evidence`'s "reopen a stale completed/idle turn on
    /// renewed screen evidence" path, which is not a real new turn boundary.
    pub(crate) fn reset_suggest_memory(&mut self) {
        self.pending_suggest_items = None;
        self.pending_suggest_turn_epoch = 0;
        self.pending_suggest_at = None;
        self.completion_declared = false;
        self.completion_turn_epoch = 0;
    }

    /// Clear a `bgtasks`-declared background-work claim. Call this ONLY at a
    /// genuine new-turn boundary (a real line/interrupt submitted to the
    /// agent) — never from `apply_working_evidence`'s reopening path.
    ///
    /// This used to be folded into `reset_suggest_memory` on the assumption
    /// that "a new turn invalidates any prior declaration" — true, but
    /// `reset_suggest_memory` is ALSO called when renewed screen evidence
    /// reopens a stale completed/idle Claude turn (`apply_working_evidence`),
    /// which is not a new turn at all: for an agent orchestrating background
    /// teammates, waking up to poll them is expected and doesn't mean the
    /// teammates finished. Folding the two together meant every such poll
    /// silently erased a still-accurate `declared_background_work=true` the
    /// moment the parent's own screen showed renewed activity, until the
    /// next `Stop` hook happened to re-assert it — see the `ai-usage`
    /// Agent-Teams incident (2026-09-16) this split was extracted from.
    pub(crate) fn reset_declared_background_work(&mut self) {
        self.declared_background_work = false;
        self.declared_background_work_turn_epoch = 0;
    }

    #[cfg(test)]
    pub(crate) fn completion_declared(&self) -> bool {
        self.completion_declared
    }

    pub(crate) fn completion_declared_for_epoch(&self, turn_epoch: u64) -> bool {
        self.completion_declared && self.completion_turn_epoch == turn_epoch
    }

    /// Set from the `bgtasks` OSC 7770 verb (`pty.rs`'s `TermEvent::Tuic`
    /// dispatch) — Claude's own hook declared at least one background task
    /// with a "still running" status as of `turn_epoch`.
    pub(crate) fn set_declared_background_work(&mut self, active: bool, turn_epoch: u64) {
        self.declared_background_work = active;
        self.declared_background_work_turn_epoch = turn_epoch;
    }

    /// Mirrors `completion_declared_for_epoch`: only true for the CURRENT
    /// turn — a new turn silently invalidates a stale declaration even
    /// without an explicit clear (see `reset_declared_background_work`).
    pub(crate) fn declared_background_work_for_epoch(&self, turn_epoch: u64) -> bool {
        self.declared_background_work && self.declared_background_work_turn_epoch == turn_epoch
    }

    /// Returns true if the session has been silent long enough and the spinner
    /// is not active. Used by the timer thread before reading the screen.
    pub(crate) fn is_silent(&self) -> bool {
        !self.question_already_emitted
            && !self.is_spinner_active()
            && self.last_output_at.elapsed() >= SILENCE_QUESTION_THRESHOLD
    }

    /// Retraction is independent from question emission. Once a low-confidence
    /// wait is active, the timer must keep reconciling it even though
    /// `question_already_emitted` deliberately blocks another SET.
    fn is_quiet_for_question_retraction(&self) -> bool {
        !self.is_spinner_active() && self.last_output_at.elapsed() >= SILENCE_QUESTION_THRESHOLD
    }

    /// Mark that a question has been emitted (prevents re-emission).
    /// Stores the emitted text so that scroll-induced reappearances of the same
    /// `?` line in changed_rows are recognized as duplicates, not new questions.
    pub(crate) fn mark_emitted(&mut self, text: &str) {
        self.question_already_emitted = true;
        self.last_emitted_text = Some(text.to_string());
    }
}

/// Attempt a shell state transition using compare_exchange.
/// Returns true if the transition was performed (and a ShellState event should be emitted).
/// Attempt an atomic shell-state transition.
///
/// When `notify_parent` is true and the transition is BUSY→IDLE, pushes a
/// state_change message to the parent's inbox (used during normal idle detection).
/// Pass `notify_parent=false` from process-exit paths — the sole "exited"
/// notification from `mark_session_exited` is sufficient; suppressing the
/// intermediate "idle" avoids the orchestrator double-firing on exit.
///
/// RE-ENTRANCY INVARIANT (CONC-C, story 099-6526): this fn does its own
/// `shell_states.get(session_id)` below. Callers MUST NOT hold a `shell_states`
/// Ref for the same key across this call — a held Ref plus this second get on the
/// same shard can deadlock under parking_lot writer-fairness when a concurrent
/// session create/destroy is queued to write the shard between the two reads.
/// Load what you need, drop the Ref, then call. Internally the Ref is dropped
/// BEFORE any post-transition work for the same reason:
/// `flush_pending_injections_blocking` re-reads `shell_states` through
/// `should_inject_now` on this very thread. (`push_state_change_to_parent` reaches
/// the same read via `deliver_notice_to_pty`, but hands it to the injection
/// worker, so it is no longer this thread's re-entrancy to manage.)
fn try_shell_transition(
    state: &crate::state::AppState,
    session_id: &str,
    expected: u8,
    new: u8,
    notify_parent: bool,
) -> bool {
    let observed_turn_epoch = state
        .session_maps
        .session_states
        .get(session_id)
        .map(|session| session.turn_epoch);
    try_shell_transition_for_epoch(
        state,
        session_id,
        expected,
        new,
        notify_parent,
        observed_turn_epoch,
    )
}

fn try_shell_transition_for_epoch(
    state: &crate::state::AppState,
    session_id: &str,
    expected: u8,
    new: u8,
    notify_parent: bool,
    observed_turn_epoch: Option<u64>,
) -> bool {
    try_shell_transition_with_hooks(
        ShellTransitionRequest {
            state,
            session_id,
            expected,
            new,
            notify_parent,
            observed_turn_epoch,
        },
        ShellTransitionHooks {
            after_epoch_snapshot: || {},
            after_cas: || {},
            before_parent_dispatch: || {},
        },
    )
}

#[cfg(test)]
fn try_shell_transition_with_hook<F: FnOnce()>(
    state: &crate::state::AppState,
    session_id: &str,
    expected: u8,
    new: u8,
    notify_parent: bool,
    after_cas: F,
) -> bool {
    let observed_turn_epoch = state
        .session_maps
        .session_states
        .get(session_id)
        .map(|session| session.turn_epoch);
    try_shell_transition_with_hooks(
        ShellTransitionRequest {
            state,
            session_id,
            expected,
            new,
            notify_parent,
            observed_turn_epoch,
        },
        ShellTransitionHooks {
            after_epoch_snapshot: || {},
            after_cas,
            before_parent_dispatch: || {},
        },
    )
}

#[derive(Clone, Copy)]
struct ShellTransitionRequest<'a> {
    state: &'a crate::state::AppState,
    session_id: &'a str,
    expected: u8,
    new: u8,
    notify_parent: bool,
    observed_turn_epoch: Option<u64>,
}

struct ShellTransitionHooks<B: FnOnce(), A: FnOnce(), D: FnOnce()> {
    after_epoch_snapshot: B,
    after_cas: A,
    before_parent_dispatch: D,
}

fn try_shell_transition_with_hooks<B: FnOnce(), A: FnOnce(), D: FnOnce()>(
    transition: ShellTransitionRequest<'_>,
    hooks: ShellTransitionHooks<B, A, D>,
) -> bool {
    // One lifecycle lock covers CAS through the authoritative parent inbox
    // enqueue. Submitted-turn reservations take the same lock, so a new epoch
    // cannot begin between an IDLE CAS and the preceding turn's notification.
    (hooks.after_epoch_snapshot)();
    let silence = transition
        .state
        .session_maps
        .silence_states
        .get(transition.session_id)
        .map(|entry| Arc::clone(entry.value()));
    let (transitioned, parent_dispatch) = {
        let mut silence_guard = silence.as_ref().map(|silence| silence.lock());
        let silence_state = silence_guard.as_deref_mut();
        try_shell_transition_locked(transition, silence_state, hooks.after_cas)
    };
    if let Some(dispatch) = parent_dispatch {
        (hooks.before_parent_dispatch)();
        dispatch_parent_lifecycle(transition.state, dispatch);
    }
    transitioned
}

/// Perform a shell transition while the caller owns the lifecycle lock.
/// `note_submitted_input` uses this form so epoch mutation and IDLE→BUSY are
/// one critical section instead of recursively acquiring `SilenceState`.
fn try_shell_transition_locked<F: FnOnce()>(
    transition: ShellTransitionRequest<'_>,
    mut silence_state: Option<&mut SilenceState>,
    after_cas: F,
) -> (bool, Option<ParentLifecycleDispatch>) {
    let ShellTransitionRequest {
        state,
        session_id,
        expected,
        new,
        notify_parent,
        observed_turn_epoch,
    } = transition;
    if expected == SHELL_BUSY
        && new == SHELL_IDLE
        && observed_turn_epoch.is_some_and(|observed| {
            state
                .session_maps
                .session_states
                .get(session_id)
                .is_some_and(|session| session.turn_epoch != observed)
        })
    {
        return (false, None);
    }
    let ok = match state.session_maps.shell_states.get(session_id) {
        Some(atom) => atom
            .compare_exchange(
                expected,
                new,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Relaxed,
            )
            .is_ok(),
        None => return (false, None),
    };
    let mut parent_dispatch = None;
    if ok {
        after_cas();
    }
    // Ref dropped here — post-transition work below re-enters shell_states.
    if ok {
        if new == SHELL_BUSY
            && let Some(silence) = silence_state.as_mut()
        {
            silence.note_busy_evidence();
            invalidate_background_probe_boundary_locked(state, session_id);
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        // Insert with the correct timestamp immediately so concurrent
        // readers never observe a transient 0 between or_insert and store.
        state
            .session_maps
            .shell_state_since_ms
            .entry(session_id.to_string())
            .and_modify(|a| a.store(now_ms, std::sync::atomic::Ordering::Relaxed))
            .or_insert_with(|| std::sync::atomic::AtomicU64::new(now_ms));
        // Notify orchestrator when an agent goes idle (BUSY→IDLE only).
        // Plain shell sessions are excluded — only registered agent sessions qualify.
        if notify_parent && expected == SHELL_BUSY && new == SHELL_IDLE {
            let session_lifecycle = state
                .session_maps
                .session_states
                .get(session_id)
                .map(|s| (s.agent_type.is_some(), s.turn_epoch));
            let completion_declared = session_lifecycle.is_some_and(|(_, turn_epoch)| {
                silence_state
                    .as_ref()
                    .is_some_and(|silence| silence.completion_declared_for_epoch(turn_epoch))
            });
            let is_agent = session_lifecycle.is_some_and(|(is_agent, _)| is_agent);
            let has_background_work = state
                .session_maps
                .session_states
                .get(session_id)
                .is_some_and(|session| session.background_work);
            let background_probe_pending = state
                .session_maps
                .session_states
                .get(session_id)
                .is_some_and(|session| session.has_pending_background_probe());
            // `silence_state` (the caller's already-locked guard) — NOT
            // `state.declared_background_work_for`, which would try to
            // re-lock the same non-reentrant mutex and deadlock.
            let declared_background_work = session_lifecycle.is_some_and(|(_, turn_epoch)| {
                silence_state
                    .as_ref()
                    .is_some_and(|silence| silence.declared_background_work_for_epoch(turn_epoch))
            });
            if is_agent
                && !completion_declared
                && !has_background_work
                && !background_probe_pending
                && !declared_background_work
            {
                parent_dispatch = enqueue_state_change_to_parent(
                    state,
                    session_id,
                    serde_json::json!({
                        "type": "state_change",
                        "state": "idle",
                        "session_id": session_id,
                    }),
                );
            }
        }
    }
    (ok, parent_dispatch)
}

/// Decision from `should_transition_idle`.
///
/// `force_cleared_subtasks` is true only on the stale-subtask recovery path —
/// callers must emit `ActiveSubtasks { count: 0 }` so the frontend store and
/// notification gate reset (story 1366-2b3e/H1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IdleDecision {
    should_transition: bool,
    force_cleared_subtasks: bool,
    turn_epoch: Option<u64>,
}

impl IdleDecision {
    const NO: Self = Self {
        should_transition: false,
        force_cleared_subtasks: false,
        turn_epoch: None,
    };

    const fn yes(turn_epoch: Option<u64>) -> Self {
        Self {
            should_transition: true,
            force_cleared_subtasks: false,
            turn_epoch,
        }
    }
}

/// Current wall-clock time as milliseconds since the Unix epoch.
fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Check whether the session should transition to idle (busy → idle).
/// Conditions: last real output > threshold ago AND no active sub-tasks.
/// Agent sessions use a longer threshold (AGENT_IDLE_MS) because AI agents
/// produce output in bursts with natural thinking pauses between them.
fn should_transition_idle(state: &crate::state::AppState, session_id: &str) -> IdleDecision {
    should_transition_idle_with_hook(state, session_id, || {})
}

fn should_transition_idle_with_hook<F: FnOnce()>(
    state: &crate::state::AppState,
    session_id: &str,
    after_silence_evidence: F,
) -> IdleDecision {
    // Capture the originating turn before reading the silence evidence. A new
    // submission updates the epoch before stamping last_output_ms; either this
    // decision sees the fresh timestamp, or the transition rejects its stale
    // epoch. Reading these in the opposite order can pair old silence with the
    // new turn and immediately idle a just-submitted task.
    //
    // Read the snapshot in a scoped block so the DashMap shard read-lock is
    // released before we take a write-lock below — same shard would otherwise
    // deadlock the runtime in the force-clear branch.
    let (is_agent, sub_tasks, turn_epoch) = {
        let session = state.session_maps.session_states.get(session_id);
        (
            session
                .as_ref()
                .map(|s| s.agent_type.is_some())
                .unwrap_or(false),
            session.as_ref().map(|s| s.active_sub_tasks).unwrap_or(0),
            session.as_ref().map(|s| s.turn_epoch),
        )
    };
    let last_ms = state
        .session_maps
        .last_output_ms
        .get(session_id)
        .map(|ts| ts.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(0);
    after_silence_evidence();
    if last_ms == 0 {
        return IdleDecision::NO;
    }
    let threshold = if is_agent {
        AGENT_IDLE_MS
    } else {
        SHELL_IDLE_MS
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let elapsed = now.saturating_sub(last_ms);
    if elapsed < threshold {
        return IdleDecision::NO;
    }
    if sub_tasks == 0 {
        return IdleDecision::yes(turn_epoch);
    }
    // Sub-tasks are active but no output for SUBTASK_STALE_MS — the mode-line
    // disappeared without emitting count=0 (agent exited, user cleared, etc.).
    // Force-clear the stale counter so we don't stay busy forever.
    if elapsed >= SUBTASK_STALE_MS {
        if let Some(mut entry) = state.session_maps.session_states.get_mut(session_id) {
            entry.active_sub_tasks = 0;
        }
        return IdleDecision {
            should_transition: true,
            force_cleared_subtasks: true,
            turn_epoch,
        };
    }
    IdleDecision::NO
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgentScreenActivity {
    Working,
    Ready,
    Interrupted,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProcessTreeEntry {
    pid: u32,
    parent_pid: u32,
    name: String,
    command: String,
    /// Seconds this process has been alive, when the platform snapshot reports
    /// it. `None` on Windows, whose `PROCESSENTRY32` carries no creation time —
    /// see [`started_with_agent`] for what the absence costs.
    age_seconds: Option<u64>,
}

#[derive(Default)]
struct ProcessSnapshotState {
    generation: u64,
    current: Option<Arc<Vec<ProcessTreeEntry>>>,
}

#[derive(Default)]
pub(crate) struct ProcessSnapshotCache {
    state: parking_lot::RwLock<ProcessSnapshotState>,
}

impl ProcessSnapshotCache {
    fn store(&self, snapshot: Option<Vec<ProcessTreeEntry>>) {
        let mut state = self.state.write();
        state.generation = state.generation.wrapping_add(1);
        state.current = snapshot.map(Arc::new);
    }

    fn load(&self) -> Option<(u64, Arc<Vec<ProcessTreeEntry>>)> {
        let state = self.state.read();
        Some((state.generation, Arc::clone(state.current.as_ref()?)))
    }

    fn generation(&self) -> u64 {
        self.state.read().generation
    }
}

fn normalized_process_name(value: &str) -> &str {
    value
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(value)
        .trim_end_matches(".exe")
}

/// Apply the same basename/path convention used by `process_name_from_pid`.
/// Claude's installer notably uses a version number as the executable basename,
/// so the containing `claude/versions/` path is authoritative.
fn classify_agent_name_or_path(value: &str) -> Option<&'static str> {
    let normalized = value.to_ascii_lowercase();
    let basename = normalized_process_name(&normalized);
    classify_agent(basename)
        .or_else(|| normalized.split(['/', '\\']).rev().find_map(classify_agent))
}

fn is_persistent_agent_helper(process: &ProcessTreeEntry) -> bool {
    is_persistent_agent_helper_with_command_line(process, cfg!(not(windows)))
}

fn is_standalone_timed_caffeinate(command: &str) -> bool {
    let mut argv = command.split_whitespace();
    let executable = argv.next().map(normalized_process_name).unwrap_or("");
    if executable != "caffeinate" {
        return false;
    }
    let first = argv.next();
    let second = argv.next();
    let third = argv.next();
    if argv.next().is_some() {
        return false;
    }
    let positive_timeout = |value: &str| value.parse::<u64>().is_ok_and(|seconds| seconds > 0);
    matches!((first, second, third), (Some("-i"), Some("-t"), Some(value)) if positive_timeout(value))
        || matches!((first, second, third), (Some("-t"), Some(value), Some("-i")) if positive_timeout(value))
}

fn is_persistent_agent_helper_with_command_line(
    process: &ProcessTreeEntry,
    command_line_authoritative: bool,
) -> bool {
    let name = process.name.to_ascii_lowercase();
    let name = normalized_process_name(&name);
    let command = process.command.to_ascii_lowercase();
    let mut argv = command.split_whitespace();
    let executable = argv.next().map(normalized_process_name).unwrap_or("");
    let script = argv.next().map(normalized_process_name).unwrap_or("");
    matches!(name, "mdkb" | "tuic-bridge" | "node_repl")
        || (command_line_authoritative
            && (matches!(executable, "mdkb" | "tuic-bridge" | "node_repl")
                || (matches!(executable, "node" | "nodejs")
                    && script.trim_end_matches(".js") == "node_repl")
                || is_standalone_timed_caffeinate(&command)))
}

/// How long after the agent's own start a descendant may appear and still count
/// as session plumbing rather than work.
///
/// Measured against a live 14-session instance: every integration daemon came up
/// within 18s of its agent (`codex-code-mode-host` 12–18s, MCP servers 0–10s),
/// while work spawned by a turn was hundreds to thousands of seconds younger
/// than its agent. 60s sits in that gap with room for a cold MCP start.
const AGENT_STARTUP_WINDOW_SECS: u64 = 60;

/// Whether `descendant` came up alongside the agent instead of being spawned by
/// a turn.
///
/// [`is_persistent_agent_helper`] answers the same question by name, and a name
/// list cannot keep up: `codex-code-mode-host` arrived with Codex 0.149.0, and
/// an MCP server started through `npm exec` reports as `npm` — a name that must
/// stay meaningful because a turn also runs npm. Both pinned every session on
/// this machine to `working` forever, because `background_work` outranks both
/// `completion_declared` and an idle shell in the agent-state ladder.
///
/// Age is the property that actually separates the two, and it needs no
/// per-tool knowledge. When either age is missing this returns false, leaving
/// the name list as the sole rule — which is exactly the Windows behaviour, and
/// errs toward reporting work rather than hiding it.
// DEFERRED (2026-08-23) — a daemon that dies and respawns mid-session escapes
// this window and is then counted as work for the rest of the session. Measured
// once over the live 14-session instance: 1 session, whose
// `codex-code-mode-host` had restarted 2494s after its agent. The remaining 13
// were classified correctly, against 14 wrong before the window existed. Fixing
// it needs per-session memory of pids already judged plumbing, which is state
// this pure function does not have — do not reach for a wider window instead,
// that is the same name-list mistake measured in seconds.
//
// DEFERRED (2026-08-25) — the mirror blind spot: real work spawned inside the
// agent's own first 60s is classified as plumbing, and the difference of two
// ages is constant, so the misclassification lasts that process's whole life.
// It bites a fast first turn that declares completion while a build it started
// keeps running — the session then reads idle. Do NOT "fix" it by skipping the
// window while the agent is young: every integration daemon comes up in the
// first 18s, so that trades this narrow false-idle for a guaranteed
// false-working minute on every session ever opened. Same per-session pid
// memory as above is the real fix.
fn started_with_agent(descendant: &ProcessTreeEntry, agent_age_seconds: Option<u64>) -> bool {
    let (Some(agent_age), Some(descendant_age)) = (agent_age_seconds, descendant.age_seconds)
    else {
        return false;
    };
    agent_age.saturating_sub(descendant_age) <= AGENT_STARTUP_WINDOW_SECS
}

fn agent_process_root(
    session_root: u32,
    agent_type: &str,
    processes: &[ProcessTreeEntry],
) -> Option<u32> {
    let mut children = std::collections::HashMap::<u32, Vec<&ProcessTreeEntry>>::new();
    let mut by_pid = std::collections::HashMap::<u32, &ProcessTreeEntry>::new();
    for process in processes {
        children
            .entry(process.parent_pid)
            .or_default()
            .push(process);
        by_pid.insert(process.pid, process);
    }
    by_pid.get(&session_root)?;
    let mut queue = std::collections::VecDeque::from([session_root]);
    while let Some(pid) = queue.pop_front() {
        if let Some(process) = by_pid.get(&pid) {
            let executable_arg = process.command.split_whitespace().next().unwrap_or("");
            if classify_agent_name_or_path(&process.name) == Some(agent_type)
                || classify_agent_name_or_path(executable_arg) == Some(agent_type)
            {
                return Some(pid);
            }
        }
        if let Some(descendants) = children.get(&pid) {
            queue.extend(descendants.iter().map(|process| process.pid));
        }
    }
    // A configured custom alias may have no classifiable executable path. The
    // process-group leader is then the established foreground-process fallback;
    // descendants, rather than the alias process itself, represent background work.
    Some(session_root)
}

/// Return whether `root_pid` owns at least one meaningful live descendant.
/// Helper roots and their entire subtrees are ignored: integration daemons are
/// session plumbing, not evidence that the agent still owns autonomous work.
/// A daemon is recognised either by name ([`is_persistent_agent_helper`]) or by
/// having started with the agent ([`started_with_agent`]).
fn has_meaningful_descendant(root_pid: u32, processes: &[ProcessTreeEntry]) -> bool {
    let mut children = std::collections::HashMap::<u32, Vec<&ProcessTreeEntry>>::new();
    let mut agent_age = None;
    for process in processes {
        children
            .entry(process.parent_pid)
            .or_default()
            .push(process);
        if process.pid == root_pid {
            agent_age = process.age_seconds;
        }
    }
    let mut stack = vec![root_pid];
    while let Some(parent) = stack.pop() {
        let Some(descendants) = children.get(&parent) else {
            continue;
        };
        for descendant in descendants {
            if is_persistent_agent_helper(descendant) || started_with_agent(descendant, agent_age) {
                continue;
            }
            return true;
        }
    }
    false
}

fn background_work_from_snapshot(
    session_root: u32,
    agent_type: &str,
    processes: &[ProcessTreeEntry],
) -> Option<bool> {
    let agent_root = agent_process_root(session_root, agent_type, processes)?;
    Some(has_meaningful_descendant(agent_root, processes))
}

/// Interactive shells, and the privilege wrappers that exist only to start one.
/// `login` and `doas` are here for the same reason as `sudo`/`su`: on their own
/// they are not work, they are the two hops between the outer prompt and the
/// inner one.
const PROMPT_SHELL_NAMES: &[&str] = &[
    "sh", "bash", "zsh", "fish", "dash", "ksh", "mksh", "csh", "tcsh", "ash",
];
const PROMPT_SHELL_WRAPPERS: &[&str] = &["sudo", "su", "doas", "login"];

/// Whether this process is a shell sitting at a prompt rather than running a
/// script. A login shell reports as `-zsh`, so the leading dash is stripped;
/// `-c` means the shell was handed a command and is therefore work.
fn is_prompt_shell_process(process: &ProcessTreeEntry) -> bool {
    let name = process.name.to_ascii_lowercase();
    let name = normalized_process_name(&name)
        .trim_start_matches('-')
        .to_string();
    if PROMPT_SHELL_WRAPPERS.contains(&name.as_str()) {
        return true;
    }
    if !PROMPT_SHELL_NAMES.contains(&name.as_str()) {
        return false;
    }
    !process
        .command
        .split_whitespace()
        .skip(1)
        .any(|argument| argument == "-c")
}

/// Whether the PTY's foreground process group is nothing but shells.
///
/// OSC 133 marks a command busy once and clears it once, so an interactive
/// subshell (`sh`, `sudo su`, `bash -l`) latches the outer shell BUSY for its
/// entire life — the inner shell has no integration of its own and never emits
/// the closing marker. The user is looking at an idle prompt while the tab says
/// working, which is what this repairs.
///
/// The whole subtree must qualify, not just its root: `sudo dd …` is a wrapper
/// with real work underneath it, and `sudo` on macOS allocates its own PTY, so
/// the inner shell is only reachable through the parent chain.
fn foreground_group_at_prompt(root_pid: u32, processes: &[ProcessTreeEntry]) -> bool {
    let mut children = std::collections::HashMap::<u32, Vec<&ProcessTreeEntry>>::new();
    let mut root = None;
    for process in processes {
        children
            .entry(process.parent_pid)
            .or_default()
            .push(process);
        if process.pid == root_pid {
            root = Some(process);
        }
    }
    let Some(root) = root else {
        return false;
    };
    let mut stack = vec![root];
    while let Some(process) = stack.pop() {
        if !is_prompt_shell_process(process) {
            return false;
        }
        if let Some(descendants) = children.get(&process.pid) {
            stack.extend(descendants.iter().copied());
        }
    }
    true
}

/// The pid whose process tree represents this session's foreground work.
fn session_foreground_pid(state: &AppState, session_id: &str) -> Option<u32> {
    let entry = state.session_maps.sessions.get(session_id)?;
    let session = entry.value().lock();
    #[cfg(not(windows))]
    {
        session.master.process_group_leader().map(|pid| pid as u32)
    }
    #[cfg(windows)]
    {
        session._child.process_id()
    }
}

/// Cheap precondition for the nested-prompt probe: a plain shell, currently
/// BUSY, silent long enough that no running command would still be quiet.
/// Shared by the probe itself and by the process-snapshot demand check, so the
/// snapshot is only enumerated while a session could actually use it.
fn prompt_probe_applies(state: &AppState, session_id: &str) -> bool {
    if state
        .session_maps
        .session_states
        .get(session_id)
        .is_none_or(|session| session.agent_type.is_some())
    {
        return false;
    }
    if state
        .session_maps
        .shell_states
        .get(session_id)
        .is_none_or(|shell| shell.load(std::sync::atomic::Ordering::Acquire) != SHELL_BUSY)
    {
        return false;
    }
    let last_ms = state
        .session_maps
        .last_output_ms
        .get(session_id)
        .map(|ts| ts.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(0);
    last_ms != 0 && now_epoch_ms().saturating_sub(last_ms) >= SHELL_PROMPT_PROBE_SILENCE_MS
}

/// Whether an explicit OSC 133 busy marker should be overruled because the
/// session is parked at a nested shell prompt. Must be evaluated before the
/// SilenceState lock is taken: it locks the PtySession to read the foreground
/// process group.
fn explicit_busy_is_a_nested_prompt(state: &AppState, session_id: &str) -> bool {
    if !prompt_probe_applies(state, session_id) {
        return false;
    }
    let Some((_, processes)) = state.process_snapshot_cache.load() else {
        return false;
    };
    let Some(root_pid) = session_foreground_pid(state, session_id) else {
        return false;
    };
    foreground_group_at_prompt(root_pid, &processes)
}

#[cfg(not(windows))]
fn process_tree_snapshot() -> Option<Vec<ProcessTreeEntry>> {
    let output = std::process::Command::new("ps")
        .args(["-ww", "-axo", "pid=,ppid=,etime=,comm=,args="])
        .output()
        .ok()?;
    parse_process_tree_snapshot(
        output.status.success(),
        &String::from_utf8_lossy(&output.stdout),
    )
}

#[cfg(not(windows))]
fn parse_process_tree_snapshot(success: bool, text: &str) -> Option<Vec<ProcessTreeEntry>> {
    if !success {
        return None;
    }
    let mut result = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let (pid, rest) = take_process_snapshot_field(line)?;
        let (parent_pid, rest) = take_process_snapshot_field(rest)?;
        let (elapsed, rest) = take_process_snapshot_field(rest)?;
        let (name, command) = take_process_snapshot_field(rest)?;
        result.push(ProcessTreeEntry {
            pid: pid.parse().ok()?,
            parent_pid: parent_pid.parse().ok()?,
            name: name.to_string(),
            command: command.trim_start().to_string(),
            age_seconds: parse_elapsed_time(elapsed),
        });
    }
    (!result.is_empty()).then_some(result)
}

/// Parse the POSIX `ps -o etime` field — `[[dd-]hh:]mm:ss` — into seconds.
///
/// Returns `None` for anything else so an unparsed field degrades to "age
/// unknown" rather than to a fabricated age. `ps` always emits at least
/// `mm:ss`, so a lone number is not a valid reading.
#[cfg(not(windows))]
fn parse_elapsed_time(value: &str) -> Option<u64> {
    let (days, clock) = match value.split_once('-') {
        Some((days, clock)) => (days.parse::<u64>().ok()?, clock),
        None => (0, value),
    };
    let mut seconds: u64 = 0;
    let mut fields = 0;
    for field in clock.split(':') {
        seconds = seconds
            .checked_mul(60)?
            .checked_add(field.parse::<u64>().ok()?)?;
        fields += 1;
    }
    (2..=3).contains(&fields).then_some(days * 86400 + seconds)
}

#[cfg(not(windows))]
fn take_process_snapshot_field(value: &str) -> Option<(&str, &str)> {
    let value = value.trim_start();
    let end = value.find(char::is_whitespace).unwrap_or(value.len());
    (end > 0).then(|| (&value[..end], &value[end..]))
}

#[cfg(windows)]
fn process_tree_snapshot() -> Option<Vec<ProcessTreeEntry>> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next, TH32CS_SNAPPROCESS,
    };

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return None;
        }
        let mut result = Vec::new();
        let mut entry: PROCESSENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;
        if Process32First(snapshot, &mut entry) == 0 {
            CloseHandle(snapshot);
            return valid_process_snapshot(false, result);
        }
        loop {
            let name_bytes: Vec<u8> = entry
                .szExeFile
                .iter()
                .take_while(|&&byte| byte != 0)
                .map(|&byte| byte as u8)
                .collect();
            let name = String::from_utf8_lossy(&name_bytes).into_owned();
            result.push(ProcessTreeEntry {
                pid: entry.th32ProcessID,
                parent_pid: entry.th32ParentProcessID,
                command: String::new(),
                name,
                age_seconds: None,
            });
            if Process32Next(snapshot, &mut entry) == 0 {
                break;
            }
        }
        CloseHandle(snapshot);
        valid_process_snapshot(true, result)
    }
}

#[cfg(any(windows, test))]
fn valid_process_snapshot(
    enumeration_succeeded: bool,
    processes: Vec<ProcessTreeEntry>,
) -> Option<Vec<ProcessTreeEntry>> {
    (enumeration_succeeded && !processes.is_empty()).then_some(processes)
}

fn emit_suggest_event(state: &AppState, session_id: &str, turn_epoch: u64, items: Vec<String>) {
    let parsed = ParsedEvent::Suggest { items };
    if let Ok(mut json) = serde_json::to_value(&parsed) {
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

fn set_background_work_for_epoch(
    state: &AppState,
    session_id: &str,
    observed_turn_epoch: u64,
    snapshot_generation: u64,
    active: bool,
) -> bool {
    set_background_work_for_epoch_with_hook(
        state,
        session_id,
        observed_turn_epoch,
        snapshot_generation,
        active,
        || {},
    )
}

fn set_background_work_for_epoch_with_hook<F: FnOnce()>(
    state: &AppState,
    session_id: &str,
    observed_turn_epoch: u64,
    snapshot_generation: u64,
    active: bool,
    after_lifecycle_snapshot: F,
) -> bool {
    let Some(silence) = state
        .session_maps
        .silence_states
        .get(session_id)
        .map(|entry| Arc::clone(entry.value()))
    else {
        return false;
    };
    after_lifecycle_snapshot();
    let mut silence_state = silence.lock();
    let still_owns_lifecycle = state
        .session_maps
        .silence_states
        .get(session_id)
        .is_some_and(|current| Arc::ptr_eq(current.value(), &silence));
    if !still_owns_lifecycle || !state.session_maps.shell_states.contains_key(session_id) {
        return false;
    }
    let Some(mut session) = state.session_maps.session_states.get_mut(session_id) else {
        return false;
    };
    if session.turn_epoch != observed_turn_epoch
        || snapshot_generation <= session.background_snapshot_generation
    {
        return false;
    }
    let reconciled_probe = if session.has_pending_background_probe() {
        let Some(boundary) = session.background_probe_after_generation else {
            return false;
        };
        if snapshot_generation <= boundary {
            return false;
        }
        session.background_probe_turn_epoch = None;
        session.background_probe_after_generation = None;
        session.background_probe_satisfied_turn_epoch = Some(observed_turn_epoch);
        true
    } else if !session.background_work {
        return false;
    } else {
        false
    };
    session.background_snapshot_generation = snapshot_generation;
    if session.background_work == active {
        if !reconciled_probe || active {
            return true;
        }
    } else {
        session.background_work = active;
    }
    drop(session);

    let mut parent_dispatch = None;
    let settled_idle = !active
        && state
            .session_maps
            .shell_states
            .get(session_id)
            .is_some_and(|shell| shell.load(Ordering::Acquire) == SHELL_IDLE);
    if settled_idle {
        let completion = silence_state.drain_pending_suggest_with_epoch();
        match completion {
            Some((turn_epoch, items)) if turn_epoch == observed_turn_epoch => {
                emit_suggest_event(state, session_id, turn_epoch, items);
                parent_dispatch = enqueue_state_change_to_parent(
                    state,
                    session_id,
                    serde_json::json!({
                        "type": "state_change",
                        "state": "completed",
                        "session_id": session_id,
                    }),
                );
            }
            Some((turn_epoch, _)) => {
                if silence_state.completion_turn_epoch == turn_epoch {
                    silence_state.completion_declared = false;
                    silence_state.completion_turn_epoch = 0;
                }
            }
            None => {
                parent_dispatch = enqueue_state_change_to_parent(
                    state,
                    session_id,
                    serde_json::json!({
                        "type": "state_change",
                        "state": "idle",
                        "session_id": session_id,
                    }),
                );
            }
        }
    }
    drop(silence_state);
    if let Some(dispatch) = parent_dispatch {
        dispatch_parent_lifecycle(state, dispatch);
    }
    if settled_idle {
        reevaluate_orchestrator_mail_wake(state, session_id);
    }
    true
}

/// What the foreground probe knows about the current turn.
///
/// It used to be a bare `bool` meaning "satisfied or not applicable", which
/// gated the idle transition while recording nothing (#771-4733). The gate is
/// only two of the three answers; the third is a real observation of the
/// process table and belongs in the evidence model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForegroundProbe {
    /// A process snapshot taken after the probe was armed was reconciled
    /// inside this turn, and it found no meaningful live descendant under the
    /// agent's process root. The agent owns nothing that is still running —
    /// [`EvidenceRank::Process`] evidence, not a gate result.
    Quiet,
    /// The gate is open without a process observation: either the session has
    /// no agent process to probe at all, or the reconciled snapshot still
    /// shows work under the agent root. Neither held the transition before
    /// this change and neither holds it now.
    Open,
    /// No snapshot newer than the arming boundary has landed yet. One is now
    /// armed for this turn; hold the transition until it reconciles.
    Pending,
}

fn foreground_probe(
    state: &AppState,
    session_id: &str,
    silence: &Arc<Mutex<SilenceState>>,
) -> ForegroundProbe {
    let still_owns_lifecycle = state
        .session_maps
        .silence_states
        .get(session_id)
        .is_some_and(|current| Arc::ptr_eq(current.value(), silence));
    if !still_owns_lifecycle || !state.session_maps.shell_states.contains_key(session_id) {
        return ForegroundProbe::Pending;
    }
    let Some(mut session) = state.session_maps.session_states.get_mut(session_id) else {
        return ForegroundProbe::Pending;
    };
    if session.agent_type.is_none() {
        return ForegroundProbe::Open;
    }
    let turn_epoch = session.turn_epoch;
    if session.background_probe_satisfied_turn_epoch == Some(turn_epoch) {
        // `background_work` is the verdict of the snapshot that satisfied the
        // probe (`set_background_work_for_epoch_with_hook`). Only its negative
        // is an observation worth ranking: a tree that still shows work says
        // nothing about *this* turn ending, and it never held the transition.
        return if session.background_work {
            ForegroundProbe::Open
        } else {
            ForegroundProbe::Quiet
        };
    }
    if !session.has_pending_background_probe() {
        session.background_probe_turn_epoch = Some(turn_epoch);
        session.background_probe_after_generation = Some(state.process_snapshot_cache.generation());
    }
    ForegroundProbe::Pending
}

/// Invalidate only the process-snapshot boundary for the current working
/// episode. The caller must hold this session's SilenceState lifecycle lock.
fn invalidate_background_probe_boundary_locked(state: &AppState, session_id: &str) {
    let Some(mut session) = state.session_maps.session_states.get_mut(session_id) else {
        return;
    };
    session.background_probe_turn_epoch = None;
    session.background_probe_after_generation = None;
    session.background_probe_satisfied_turn_epoch = None;
}

fn arm_explicit_idle_background_probe(state: &AppState, session_id: &str, turn_epoch: u64) {
    let Some(mut session) = state.session_maps.session_states.get_mut(session_id) else {
        return;
    };
    if session.agent_type.is_none() || session.turn_epoch != turn_epoch {
        return;
    }
    session.background_probe_turn_epoch = Some(turn_epoch);
    session.background_probe_after_generation = Some(state.process_snapshot_cache.generation());
    session.background_probe_satisfied_turn_epoch = None;
}

fn refresh_background_work(state: &AppState, session_id: &str) {
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|session| session.agent_type.clone());
    let observed_turn_epoch = state
        .session_maps
        .session_states
        .get(session_id)
        .map(|session| session.turn_epoch);
    let root_pid = session_foreground_pid(state, session_id);
    let (Some(root_pid), Some(agent_type), Some(observed_turn_epoch)) =
        (root_pid, agent_type, observed_turn_epoch)
    else {
        return;
    };
    refresh_background_work_from_cached_snapshot(
        state,
        session_id,
        root_pid,
        &agent_type,
        observed_turn_epoch,
        state.process_snapshot_cache.load(),
    );
}

fn refresh_background_work_from_cached_snapshot(
    state: &AppState,
    session_id: &str,
    root_pid: u32,
    agent_type: &str,
    observed_turn_epoch: u64,
    cached: Option<(u64, Arc<Vec<ProcessTreeEntry>>)>,
) -> bool {
    let Some((generation, processes)) = cached else {
        return false;
    };
    let Some(active) = background_work_from_snapshot(root_pid, agent_type, &processes) else {
        return false;
    };
    set_background_work_for_epoch(state, session_id, observed_turn_epoch, generation, active)
}

fn process_snapshot_is_demanded(state: &AppState) -> bool {
    state.session_maps.session_states.iter().any(|session| {
        (if session.agent_type.is_some() {
            session.has_pending_background_probe() || session.background_work
        } else {
            prompt_probe_applies(state, session.key())
        }) && state
            .session_maps
            .silence_states
            .contains_key(session.key())
            && state.session_maps.shell_states.contains_key(session.key())
    })
}

fn reconcile_process_snapshot_demand(state: &AppState) {
    let sessions: Vec<String> = state
        .session_maps
        .session_states
        .iter()
        .filter(|session| {
            session.agent_type.is_some()
                && (session.has_pending_background_probe() || session.background_work)
                && state
                    .session_maps
                    .silence_states
                    .contains_key(session.key())
                && state.session_maps.shell_states.contains_key(session.key())
        })
        .map(|session| session.key().clone())
        .collect();
    for session_id in sessions {
        refresh_background_work(state, &session_id);
    }
}

fn refresh_process_snapshot_if_demanded<F>(state: &AppState, enumerate: F) -> bool
where
    F: FnOnce() -> Option<Vec<ProcessTreeEntry>>,
{
    if !process_snapshot_is_demanded(state) {
        return false;
    }
    state.process_snapshot_cache.store(enumerate());
    reconcile_process_snapshot_demand(state);
    true
}

/// Enumerate the OS process table at most once per lifecycle cadence on
/// Tokio's blocking pool while a probe or tracked child needs reconciliation.
/// Every demanding session reads the resulting app-wide cache.
pub(crate) fn spawn_process_snapshot_refresher(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let refresh_state = Arc::clone(&state);
            let _ = tokio::task::spawn_blocking(move || {
                refresh_process_snapshot_if_demanded(&refresh_state, process_tree_snapshot)
            })
            .await;
        }
    });
}

/// Inspect Codex's live prompt neighborhood on the UNFILTERED screen.
///
/// `find_chrome_cutoff` cannot be used here: Codex separators delimit tool
/// output from summaries, not its prompt box. When a recent separator sits
/// above `• Working`, the generic cutoff intentionally trims the whole region
/// and used to hide the strongest activity signal from both reader and timer.
/// Restricting the match to a few rows immediately above the lowest `›` prompt
/// prevents a historical Working line elsewhere in the viewport from latching
/// the session busy.
fn detect_codex_screen_activity(rows: &[String]) -> AgentScreenActivity {
    const PROMPT_NEIGHBORHOOD: usize = 6;

    let Some(prompt_idx) = find_live_prompt_row(rows, |row| {
        let t = row.trim_start();
        matches!(t.chars().next(), Some('\u{203A}' | '\u{00BB}'))
            && !t.starts_with("\u{203A}\u{203A}")
    }) else {
        return AgentScreenActivity::Unknown;
    };
    let start = prompt_idx.saturating_sub(PROMPT_NEIGHBORHOOD);
    let neighborhood = &rows[start..prompt_idx];

    if neighborhood
        .iter()
        .any(|row| crate::chrome::is_working_status_row(row))
    {
        return AgentScreenActivity::Working;
    }
    if neighborhood
        .iter()
        .any(|row| row.trim_start().starts_with("■ Conversation interrupted"))
    {
        return AgentScreenActivity::Interrupted;
    }
    AgentScreenActivity::Ready
}

/// Find a prompt only in the current bottom chrome zone.
///
/// The rendered viewport includes transcript history, so a whole-screen search
/// can mistake an old submitted prompt or markdown quote for the live composer.
/// Prefer the structurally detected input box (including tall custom HUDs); if
/// no box can be identified, accept only the final three non-padding rows.
fn find_live_prompt_row<F>(rows: &[String], is_prompt: F) -> Option<usize>
where
    F: Fn(&str) -> bool,
{
    let content_end = rows
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .map_or(0, |index| index + 1);
    if content_end == 0 {
        return None;
    }
    let refs: Vec<&str> = rows[..content_end].iter().map(String::as_str).collect();
    if let Some(prompt) = crate::chrome::find_input_box_prompt_row(&refs)
        && is_prompt(&rows[prompt])
    {
        return Some(prompt);
    }
    (content_end.saturating_sub(3)..content_end)
        .rev()
        .find(|&index| is_prompt(&rows[index]))
}

/// Claude's active status is presence-based because current Claude versions can
/// keep the empty composer visible while a long tool call is still running.
/// The live marker is deliberately semantic rather than glyph-only: an animated
/// spinner prefix plus an ellipsis in the phase name (`✽ Nucleating… (3m 50s)`)
/// means active, while completed summaries (`✻ Sautéed for 1m 25s`), HUD bars,
/// hints, and banner art remain inert. This also holds BUSY when DEC 2026 frame
/// coalescing makes consecutive spinner paints text-identical.
fn detect_claude_screen_activity(rows: &[String]) -> AgentScreenActivity {
    let content_end = rows
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .map_or(0, |idx| idx + 1);
    let chrome_start = content_end.saturating_sub(crate::chrome::CHROME_SCAN_ROWS);
    let prompt_idx = rows[chrome_start..content_end]
        .iter()
        .rposition(|row| row.trim() == "\u{276F}")
        .map(|idx| chrome_start + idx);
    let activity_end = prompt_idx.unwrap_or(content_end);
    let activity_start = activity_end.saturating_sub(crate::chrome::CHROME_SCAN_ROWS);
    if rows[activity_start..activity_end].iter().any(|row| {
        crate::chrome::is_spinner_row(row)
            && row.contains('\u{2026}')
            && row.contains('(')
            && row.contains(')')
    }) {
        return AgentScreenActivity::Working;
    }
    if prompt_idx.is_some() {
        AgentScreenActivity::Ready
    } else {
        AgentScreenActivity::Unknown
    }
}

fn gemini_prompt_present(rows: &[String]) -> bool {
    find_live_prompt_row(rows, |row| {
        let t = row.trim_start();
        t == ">" || t.starts_with("> ")
    })
    .is_some()
}

/// Prompt-based only — see `detect_claude_screen_activity` for the rationale.
fn detect_gemini_screen_activity(rows: &[String]) -> AgentScreenActivity {
    if gemini_prompt_present(rows) {
        AgentScreenActivity::Ready
    } else {
        AgentScreenActivity::Unknown
    }
}

/// Prompt-based only — see `detect_claude_screen_activity` for the rationale.
/// During generation Aider has no bottom input box (prompt_toolkit returned),
/// so the screen reads Unknown and BUSY is held by spinner movement + silence.
fn detect_aider_screen_activity(rows: &[String]) -> AgentScreenActivity {
    if rows.iter().rev().take(3).any(|row| row.trim() == ">") {
        AgentScreenActivity::Ready
    } else {
        AgentScreenActivity::Unknown
    }
}

/// True for grok's composer row. Builds from 0.2.11x draw it inside a rounded box
/// (`│ ❯                    │`); earlier builds emitted a bare `❯ Ask anything`. Missing the
/// boxed form left the session stuck BUSY for the whole process, because Ready never fired.
fn is_grok_composer_row(row: &str) -> bool {
    let trimmed = row.trim_start();
    let inner = trimmed
        .strip_prefix('\u{2502}')
        .unwrap_or(trimmed)
        .trim_start();
    let mut chars = inner.chars();
    chars.next() == Some('\u{276F}') && chars.next().is_none_or(char::is_whitespace)
}

/// Grok keeps its composer visible while a turn is running, so the prompt
/// alone is not enough to declare the session ready. Its turn-status row is
/// structurally stronger: it starts with the animated braille spinner already
/// recognized by `is_spinner_row` and disappears when the turn completes.
fn detect_grok_screen_activity(rows: &[String]) -> AgentScreenActivity {
    let content_end = rows
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .map_or(0, |idx| idx + 1);
    let chrome_start = content_end.saturating_sub(crate::chrome::CHROME_SCAN_ROWS);
    let footer = &rows[chrome_start..content_end];

    if footer.iter().any(|row| crate::chrome::is_spinner_row(row)) {
        return AgentScreenActivity::Working;
    }
    if footer.iter().any(|row| is_grok_composer_row(row)) {
        AgentScreenActivity::Ready
    } else {
        AgentScreenActivity::Unknown
    }
}

/// True for pi's bottom status row: `↑1.3k ↓1.8k … 3.4%/272k (auto)   (openai) gpt-5.6-sol • medium`.
/// The context-usage `N%/Nk` pair plus the ` • ` model separator is unique to that row and
/// present in every state, so it identifies a pi screen without asserting readiness.
fn is_pi_status_row(row: &str) -> bool {
    let trimmed = row.trim();
    if !trimmed.contains(" \u{2022} ") {
        return false;
    }
    // `%/` only ever appears in the context gauge (`3.4%/272k`).
    let Some(pos) = trimmed.find("%/") else {
        return false;
    };
    trimmed[..pos]
        .chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_digit())
}

/// pi keeps its composer, separators and status row on screen for the whole turn, and the
/// composer carries no prompt glyph (it is a bare reverse-video cursor block), so readiness
/// cannot be read from a prompt char. What does change is the composer row itself: while a
/// turn runs it is replaced by an animated ` ⠏ Working...` row that `is_spinner_row` already
/// recognises. Ready is therefore "this is a pi screen and nothing is spinning".
fn detect_pi_screen_activity(rows: &[String]) -> AgentScreenActivity {
    let content_end = rows
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .map_or(0, |idx| idx + 1);
    let chrome_start = content_end.saturating_sub(crate::chrome::CHROME_SCAN_ROWS);
    let footer = &rows[chrome_start..content_end];

    if footer.iter().any(|row| crate::chrome::is_spinner_row(row)) {
        return AgentScreenActivity::Working;
    }
    if footer.iter().any(|row| is_pi_status_row(row)) {
        AgentScreenActivity::Ready
    } else {
        AgentScreenActivity::Unknown
    }
}

/// True for a row of OpenCode's composer frame: the heavy vertical `┃` (U+2503)
/// running down the left edge of the prompt box.
fn is_opencode_frame_row(row: &str) -> bool {
    row.trim_start().starts_with('\u{2503}')
}

/// True for the row that closes OpenCode's composer frame: `╹` (U+2579) followed by a
/// run of `▀` (U+2580). Present in every OpenCode state — welcome, mid-turn, finished.
fn is_opencode_frame_close_row(row: &str) -> bool {
    row.trim_start()
        .strip_prefix('\u{2579}')
        .is_some_and(|rest| rest.starts_with("\u{2580}\u{2580}\u{2580}\u{2580}"))
}

/// OpenCode is a full-screen Bubble Tea TUI, so neither of the two generic signals works:
/// it paints no prompt glyph (`❯`/`›`/`>`), and its activity indicator is a `⬝`/`■`
/// progress bar rather than anything `is_spinner_row` recognises. What IS stable across
/// every state is the composer frame — `┃` rows closed by a `╹▀▀▀…` run — with the status
/// bar painted underneath it. OpenCode only offers `esc interrupt` in that status bar while
/// a turn is running (verified live on v1.18.5 across the model phase AND a tool phase, at
/// both 120 and 62 columns), so Ready is "this is an OpenCode screen and nothing down there
/// is offering an interrupt".
///
/// Declaring Ready additionally requires the status bar's `ctrl+p commands` hint, which is
/// present in every state: without it a frame whose status bar has not been painted yet
/// would read Ready mid-turn — exactly the false idle that lets auto-standby SIGSTOP a live
/// session. The interrupt hint is checked first so a working screen is never downgraded.
fn detect_opencode_screen_activity(rows: &[String]) -> AgentScreenActivity {
    const STATUS_BAR_HINT: &str = "ctrl+p commands";
    const INTERRUPT_HINT: &str = "esc interrupt";

    let Some(close_idx) = rows
        .iter()
        .rposition(|row| is_opencode_frame_close_row(row))
    else {
        return AgentScreenActivity::Unknown;
    };
    if !rows[..close_idx]
        .iter()
        .any(|row| is_opencode_frame_row(row))
    {
        return AgentScreenActivity::Unknown;
    }
    let status_bar = &rows[close_idx + 1..];

    if status_bar.iter().any(|row| row.contains(INTERRUPT_HINT)) {
        return AgentScreenActivity::Working;
    }
    if status_bar.iter().any(|row| row.contains(STATUS_BAR_HINT)) {
        AgentScreenActivity::Ready
    } else {
        AgentScreenActivity::Unknown
    }
}

/// goose keeps a one-line composer footer at the bottom of the screen and swaps
/// it for a spinner row while a turn runs. Captured live on goose 1.49.0 at 120
/// columns (#699-c6e0), the two states are:
///
/// ```text
/// ready:    > Enter to send · Ctrl+J newline
/// working:  ◓  Merging memory matrices...  (Ctrl+C to interrupt)
/// ```
///
/// Neither generic signal works here. The spinner glyph cycles `◐◓◒`, which
/// `is_spinner_row` does not recognise, and the message beside it is whimsical
/// and changes between turns — "Merging memory matrices…" is one of a set, so
/// matching it would pin the adapter to a string goose is free to reword. What
/// does not move is the **hint** at each end: `Ctrl+C to interrupt` appears only
/// while a turn can be interrupted, and `Enter to send` only when the composer
/// is accepting input.
///
/// The interrupt hint is tested first so a working screen is never downgraded,
/// and Ready demands the composer footer rather than merely the absence of a
/// spinner — a half-painted screen must read Unknown, not idle. A false Ready is
/// the expensive direction: it is what lets auto-standby SIGSTOP a live turn.
fn detect_goose_screen_activity(rows: &[String]) -> AgentScreenActivity {
    const INTERRUPT_HINT: &str = "Ctrl+C to interrupt";
    const COMPOSER_HINT: &str = "Enter to send";

    let content_end = rows
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .map_or(0, |idx| idx + 1);
    let chrome_start = content_end.saturating_sub(crate::chrome::CHROME_SCAN_ROWS);
    let footer = &rows[chrome_start..content_end];

    if footer.iter().any(|row| row.contains(INTERRUPT_HINT)) {
        return AgentScreenActivity::Working;
    }
    if footer.iter().any(|row| row.contains(COMPOSER_HINT)) {
        AgentScreenActivity::Ready
    } else {
        AgentScreenActivity::Unknown
    }
}

/// #744-138c: call counter so a test can measure that the reader chunk path
/// and the silence timer no longer each classify the screen independently —
/// see `cached_screen_activity`. Not gated behind `#[cfg(test)]` on the
/// counter itself (the increment is one relaxed atomic add, negligible), only
/// the accessor used to read it is test-only.
static SCREEN_CLASSIFY_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
fn screen_classify_calls() -> usize {
    SCREEN_CLASSIFY_CALLS.load(std::sync::atomic::Ordering::Relaxed)
}

fn detect_agent_screen_activity(agent_type: Option<&str>, rows: &[String]) -> AgentScreenActivity {
    SCREEN_CLASSIFY_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    match agent_type {
        Some("claude") => detect_claude_screen_activity(rows),
        Some("codex") => detect_codex_screen_activity(rows),
        Some("gemini") => detect_gemini_screen_activity(rows),
        Some("aider") => detect_aider_screen_activity(rows),
        Some("grok") => detect_grok_screen_activity(rows),
        Some("pi") => detect_pi_screen_activity(rows),
        Some("opencode") => detect_opencode_screen_activity(rows),
        Some("goose") => detect_goose_screen_activity(rows),
        _ => AgentScreenActivity::Unknown,
    }
}

/// Classify the existing screen signal for an MCP submission receipt.
/// Raw-output movement is checked by the caller first; `terminal_output` means
/// the child moved but its agent adapter has no stronger current-state label.
pub(crate) fn agent_submission_ack_kind(state: &AppState, session_id: &str) -> &'static str {
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|session| session.agent_type.clone());
    let activity = state
        .grid
        .vt_log_buffers
        .get(session_id)
        .map(|vt| detect_agent_screen_activity(agent_type.as_deref(), &vt.lock().screen_rows()))
        .unwrap_or(AgentScreenActivity::Unknown);
    match activity {
        AgentScreenActivity::Working => "working_screen",
        AgentScreenActivity::Ready => "ready_screen",
        AgentScreenActivity::Interrupted => "interrupted_screen",
        AgentScreenActivity::Unknown => "terminal_output",
    }
}

/// Agents listed here recover to idle from the screen. An agent that is MISSING here and
/// whose foreground command is long-lived stays busy for the whole process, because OSC 133
/// marks that command busy once and nothing else ever clears it (#523-1df4, #534-e30c,
/// #535-d4f5).
///
/// DEFERRED (2026-08-02, narrowed 2026-09-07) — amp, cursor and droid were audited for
/// the same failure while fixing opencode and could NOT be verified: none of those three
/// binaries is installed on this machine, and an adapter written from documentation
/// instead of a live capture is how grok first shipped green tests over a UI that stayed
/// stuck BUSY. They are tracked in `to-test.md`; give each one an adapter only after
/// capturing its real screens. **goose left this list on 2026-09-07** — it was installed,
/// captured live at 1.49.0, and now has `detect_goose_screen_activity`.
///
/// **The remaining three cannot be excused instead of adapted, and that is proved rather
/// than assumed.** An agent needs no entry here only if something else returns it to idle:
/// either a protocol signal, or a foreground command short-lived enough that OSC 133
/// clears on its own. Checked 2026-09-07, both routes are shut for all three:
/// `HOOK_SUPPORT` in `src/agents.ts` is `false` for amp, cursor and droid, so no explicit
/// Stop ever sets `idle_confirmed`; and each launches as a long-lived interactive process
/// (`amp "{prompt}"`, `cursor-agent`, `droid`), which is exactly the shape the paragraph
/// above describes as latching busy forever. `pi` is the contrast that proves the rule —
/// also `HOOK_SUPPORT: false`, and it is in the list precisely because a screen adapter is
/// the only thing that can idle it. So the three need captures from installed binaries; no
/// amount of static analysis substitutes for that.
pub(crate) fn has_ready_screen_adapter(agent_type: Option<&str>) -> bool {
    matches!(
        agent_type,
        Some("claude" | "codex" | "gemini" | "aider" | "grok" | "pi" | "opencode" | "goose")
    )
}

fn stamp_last_output_now(state: &crate::state::AppState, session_id: &str, now_ms: u64) {
    if let Some(ts) = state.session_maps.last_output_ms.get(session_id) {
        ts.store(now_ms, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Apply positive working evidence immediately. In particular this repairs an
/// already-false-idle session: working evidence is an edge into BUSY, not merely
/// a keepalive that only runs while the state happens to be busy. An explicit
/// idle marker (agent hook) outranks it until the next busy evidence.
///
/// Two evidence strengths call this (#446-596f):
/// - `"working-screen"` — presence-based `• Working (… esc to interrupt)`;
///   this holds an open turn but cannot reopen a completed Codex turn because
///   completed screens can retain a stale static Working row.
/// - `"working-screen-movement"` — that exact semantic row occurred among the
///   post-cutoff `changed_rows`. Text-equality diffing means this fires only
///   while the row actually animates or its elapsed time advances, so it can
///   safely reopen an internal Codex continuation that had no PTY submission.
fn apply_working_evidence(
    state: &crate::state::AppState,
    silence: &Arc<Mutex<SilenceState>>,
    session_id: &str,
    now_ms: u64,
    source: &'static str,
) {
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|session| session.agent_type.clone());
    let can_reopen_completed = agent_type.as_deref() == Some("claude")
        || (agent_type.as_deref() == Some("codex") && source == "working-screen-movement");
    let (reopened_completion, evidence_snapshot) = {
        let mut sl = silence.lock();
        let turn_completed = state
            .session_maps
            .session_states
            .get(session_id)
            .is_some_and(|session| sl.completion_declared_for_epoch(session.turn_epoch));
        if turn_completed && !can_reopen_completed {
            return;
        }
        if sl.explicit_idle() && !can_reopen_completed {
            return;
        }
        let reopen = can_reopen_completed && (turn_completed || sl.explicit_idle());
        if reopen {
            // Claude can emit Stop/suggest before a blocking Stop hook finishes;
            // Codex can start an internal continuation without a PTY submission.
            // Current semantic movement is stronger than either stale boundary.
            sl.reset_suggest_memory();
        }
        sl.note_working_screen();
        invalidate_background_probe_boundary_locked(state, session_id);
        // One-shot evidence: it must win THIS decision (working-screen evidence
        // is deliberately allowed to override even Protocol-rank idle once the
        // reopen checks above already cleared it), but must not persist and
        // block a later, unrelated idle evidence recording — see the comment
        // on the reader chunk path's `real_activity` CAS for the same reasoning.
        let rank = if reopen {
            EvidenceRank::Protocol
        } else {
            EvidenceRank::Screen
        };
        sl.record_busy(rank, source);
        (reopen, sl.evidence.clone())
    };
    if reopened_completion
        && let Some(mut session) = state.session_maps.session_states.get_mut(session_id)
    {
        session.suggested_actions = None;
    }
    stamp_last_output_now(state, session_id, now_ms);
    let prev = state
        .session_maps
        .shell_states
        .get(session_id)
        .map(|atom| atom.load(std::sync::atomic::Ordering::Acquire));
    if let Some(prev) = prev
        && let Some(Transition::ToBusy(evidence)) = decide(
            &evidence_snapshot,
            prev == SHELL_BUSY,
            std::time::Instant::now(),
        )
        && try_shell_transition(state, session_id, prev, SHELL_BUSY, true)
    {
        tracing::debug!(
            session_id,
            activity_source = evidence.source,
            rank = ?evidence.rank,
            "Shell state → busy"
        );
        emit_shell_state(state, session_id, "busy");
    }
    let mut silence = silence.lock();
    if silence
        .evidence
        .busy
        .is_some_and(|busy| busy.source == source)
    {
        silence.clear_busy_evidence();
    }
}

/// A submitted line to a known agent is strong BUSY evidence even before the
/// first model token or spinner repaint. Adapter-backed agents hold that state
/// until a ready screen/explicit Stop; unknown agents retain the timing fallback.
pub(crate) fn note_submitted_input(state: &AppState, session_id: &str) {
    note_submitted_input_with_hook(state, session_id, || {});
}

fn note_submitted_input_with_hook<F: FnOnce()>(state: &AppState, session_id: &str, after_epoch: F) {
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|s| s.agent_type.clone());
    let Some(agent_type) = agent_type else {
        if let Some(sl) = state.session_maps.silence_states.get(session_id) {
            let mut silence = sl.lock();
            silence.note_user_submission(false);
            silence.reset_suggest_memory();
            silence.reset_declared_background_work();
        }
        return;
    };

    let silence = state
        .session_maps
        .silence_states
        .entry(session_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(SilenceState::new())))
        .clone();
    let transitioned_busy = {
        // Lock order for submitted turns is SilenceState → SessionState → shell
        // atomics. Completion drains and Suggest parsing use the same order.
        let mut silence = silence.lock();
        if let Some(mut session) = state.session_maps.session_states.get_mut(session_id) {
            session.turn_epoch = session.turn_epoch.wrapping_add(1);
            session.suggested_actions = None;
            // The denominator for marker compliance (#4421): one submitted turn
            // is one chance for the agent to emit its markers.
            state.note_marker(session_id, crate::state::MarkerKind::TurnSubmitted);
        }
        after_epoch();
        // The gate is the ready-screen adapter, NOT `hook_instrumented`: those
        // are different properties and swapping them regressed every
        // ready-adapter agent that runs without hooks. A Protocol-rank
        // "user-submit" needs something that can later retract it, and the
        // adapter is that something — a hook-instrumented agent emits its own
        // hook-busy anyway, so gating on hooks both misses the agents that need
        // this evidence and is redundant for the ones that do not.
        silence.note_user_submission(has_ready_screen_adapter(Some(&agent_type)));
        silence.reset_suggest_memory();
        silence.reset_declared_background_work();
        stamp_last_output_now(state, session_id, now_epoch_ms());
        let prev = state
            .session_maps
            .shell_states
            .get(session_id)
            .map(|atom| atom.load(std::sync::atomic::Ordering::Acquire));
        let transitioned = prev.is_some_and(|prev| {
            prev != SHELL_BUSY
                && try_shell_transition_locked(
                    ShellTransitionRequest {
                        state,
                        session_id,
                        expected: prev,
                        new: SHELL_BUSY,
                        notify_parent: true,
                        observed_turn_epoch: None,
                    },
                    Some(&mut silence),
                    || {},
                )
                .0
        });
        if transitioned {
            emit_shell_state(state, session_id, "busy");
        }
        transitioned
    };
    if transitioned_busy {
        tracing::debug!(
            session_id,
            activity_source = "user-submit",
            rank = ?EvidenceRank::Protocol,
            "Shell state → busy"
        );
    }
}

/// Emit a ShellState parsed event via both event bus and Tauri IPC.
fn emit_shell_state(state: &crate::state::AppState, session_id: &str, shell_state: &str) {
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|s| s.agent_type.clone());
    let parsed = ParsedEvent::ShellState {
        state: shell_state.to_string(),
        agent_type,
    };
    match serde_json::to_value(&parsed) {
        Ok(json) => {
            state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                session_id: session_id.to_string(),
                parsed: json.into(),
            });
        }
        Err(e) => tracing::error!(session_id, "Failed to serialize ShellState event: {e}"),
    }
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        let _ = app.emit(&format!("pty-parsed-{session_id}"), &parsed);
    }
}

/// Apply an authoritative shell-state marker and emit the new state if it
/// changed. Shared by OSC 133 A/C and OSC 7770 `state=` handlers. Returns
/// whether this call caused a real transition (as opposed to a same-state
/// re-affirmation or a stale/unknown-session no-op) — callers that need to
/// distinguish a genuine idle↔busy edge from a redundant re-affirmation
/// (e.g. turn-level block synthesis) use this instead of re-deriving it.
fn transition_explicit_shell_state(
    state: &crate::state::AppState,
    session_id: &str,
    target: u8,
    label: &str,
    hook_state: bool,
) -> bool {
    transition_explicit_shell_state_with_hook(state, session_id, target, label, hook_state, || {})
}

fn transition_explicit_shell_state_with_hook<F: FnOnce()>(
    state: &crate::state::AppState,
    session_id: &str,
    target: u8,
    label: &str,
    hook_state: bool,
    before_transaction: F,
) -> bool {
    // A hook busy/idle transition proves the agent is no longer blocked on a
    // question, so it retracts the awaiting badge. Emit ONLY when a badge is
    // actually set: the badge is sticky state, not a stream, so this is an edge
    // — one event per real clear, never one per transition.
    let clears_awaiting = hook_state
        && matches!(target, SHELL_BUSY | SHELL_IDLE)
        && state
            .session_maps
            .session_states
            .get(session_id)
            .is_some_and(|session| session.awaiting_input);
    if clears_awaiting {
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: session_id.to_string(),
            parsed: serde_json::json!({ "type": "protocol-question-cleared" }).into(),
        });
    }
    let evidence_turn_epoch = state
        .session_maps
        .session_states
        .get(session_id)
        .map(|session| session.turn_epoch);
    before_transaction();
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .map(|entry| Arc::clone(entry.value()));
    let (transitioned, evidence, parent_dispatch) = {
        let mut silence_guard = silence.as_ref().map(|silence| silence.lock());
        if target == SHELL_IDLE
            && evidence_turn_epoch.is_some_and(|observed| {
                state
                    .session_maps
                    .session_states
                    .get(session_id)
                    .is_some_and(|session| session.turn_epoch != observed)
            })
        {
            return false;
        }
        if let Some(silence) = silence_guard.as_mut() {
            silence.note_explicit_state(target, hook_state);
            if target == SHELL_BUSY {
                invalidate_background_probe_boundary_locked(state, session_id);
            }
        }
        if target == SHELL_BUSY {
            stamp_last_output_now(state, session_id, now_epoch_ms());
        }
        let evidence = match decide(
            silence_guard
                .as_deref()
                .map(|s| &s.evidence)
                .unwrap_or(&TurnEvidence::default()),
            target == SHELL_IDLE,
            std::time::Instant::now(),
        ) {
            Some(Transition::ToBusy(evidence) | Transition::ToIdle(evidence)) => Some(evidence),
            None => None,
        };
        let prev = match state.session_maps.shell_states.get(session_id) {
            Some(atom) => atom.load(std::sync::atomic::Ordering::Acquire),
            None => return false,
        };
        if prev == target {
            return false;
        }
        if prev == SHELL_BUSY
            && target == SHELL_IDLE
            && let Some(turn_epoch) = evidence_turn_epoch
        {
            arm_explicit_idle_background_probe(state, session_id, turn_epoch);
        }
        tracing::debug!(
            session_id = %session_id,
            prev,
            target,
            label,
            hook_state,
            "shell_state edge attempt (research: unexpected state transitions)"
        );
        let (transitioned, parent_dispatch) = try_shell_transition_locked(
            ShellTransitionRequest {
                state,
                session_id,
                expected: prev,
                new: target,
                notify_parent: true,
                observed_turn_epoch: evidence_turn_epoch,
            },
            silence_guard.as_deref_mut(),
            || {},
        );
        (transitioned, evidence, parent_dispatch)
    };
    if let Some(dispatch) = parent_dispatch {
        dispatch_parent_lifecycle(state, dispatch);
    }
    tracing::debug!(
        session_id = %session_id,
        target,
        label,
        transitioned,
        "shell_state edge result (research: unexpected state transitions)"
    );
    if transitioned {
        if let Some(evidence) = evidence {
            tracing::debug!(
                session_id,
                activity_source = evidence.source,
                rank = ?evidence.rank,
                "Shell state → {label}"
            );
        }
        if !hook_state && target == SHELL_IDLE {
            // OSC 133's own prompt marker (`'A'`) only fires once the real
            // shell redraws its prompt — it cannot fire while any foreground
            // child (agent or not) still owns the terminal, so this is an
            // immediate, reliable "the agent has genuinely exited" signal.
            // Deliberately NOT extended to the `hook_state` (OSC 7770) path:
            // a hook-instrumented agent's own "idle" means it finished this
            // turn and is waiting for the next prompt while the SAME process
            // stays alive — clearing there would wipe `agent_type` on every
            // ordinary turn boundary, not just on exit. Clearing here (before
            // `emit_shell_state` below, which reads `agent_type` fresh for
            // its payload) makes the LastPromptBar disappear as fast as the
            // shell's own prompt redraws, instead of waiting for the next
            // `get_session_foreground_process` poll.
            clear_agent_type_on_confirmed_shell(state, session_id);
        }
        emit_shell_state(state, session_id, label);
        // Publish IDLE before a queued delivery claims IDLE→BUSY again. Reversing
        // this order leaves the backend BUSY while the frontend's last event is
        // the stale IDLE emitted by this caller.
        if target == SHELL_IDLE {
            reevaluate_orchestrator_mail_wake(state, session_id);
            // The session's own reader thread, not a tokio worker: it must not
            // race ahead of the bytes it is about to publish.
            flush_pending_injections_blocking(state, session_id);
        }
    }
    transitioned
}

/// Emit an ActiveSubtasks parsed event via both event bus and Tauri IPC.
/// Used by the stale-subtasks recovery path to keep the frontend store in
/// sync after `should_transition_idle` force-clears the in-memory counter.
fn emit_active_subtasks(
    state: &crate::state::AppState,
    session_id: &str,
    count: u32,
    task_type: &str,
) {
    let parsed = ParsedEvent::ActiveSubtasks {
        count,
        task_type: task_type.to_string(),
    };
    match serde_json::to_value(&parsed) {
        Ok(json) => {
            state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                session_id: session_id.to_string(),
                parsed: json.into(),
            });
        }
        Err(e) => tracing::error!(session_id, "Failed to serialize ActiveSubtasks event: {e}"),
    }
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        let _ = app.emit(&format!("pty-parsed-{session_id}"), &parsed);
    }
}

/// Extract a signal number from portable_pty's signal string.
/// Format is typically "Killed: 9", "Interrupt: 2", or "Signal 15".
pub(crate) fn parse_signal_number(sig: &str) -> i32 {
    sig.rsplit(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(0)
}

fn parse_osc7_cwd(url: &str) -> Result<String, ()> {
    let rest = url.strip_prefix("file://").ok_or(())?;
    let path_start = rest.find('/').ok_or(())?;
    let raw = &rest[path_start..];
    if raw.is_empty() {
        return Err(());
    }
    let decoded = percent_decode(raw)?;
    let path = if decoded.len() > 1 && decoded.ends_with('/') {
        &decoded[..decoded.len() - 1]
    } else {
        &decoded
    };
    if !path.starts_with('/') {
        return Err(());
    }
    Ok(path.to_string())
}

fn percent_decode(s: &str) -> Result<String, ()> {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = hex_val(bytes[i + 1]).ok_or(())?;
            let lo = hex_val(bytes[i + 2]).ok_or(())?;
            out.push(hi << 4 | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn parse_osc133_exit_code(command: char, params: &str) -> Option<i32> {
    if command == 'D' && !params.is_empty() {
        params.parse::<i32>().ok()
    } else {
        None
    }
}

/// Detect Claude Code tool call headers: `⏺ ToolName(args)`.
/// The ⏺ (U+23FA) bullet followed by a capitalized word and `(` is unique to
/// CC's expanded tool-call rendering — agent prose after ⏺ starts with a
/// lowercase word or a proper noun without parens.
fn is_cc_tool_call_header(text: &str) -> bool {
    let trimmed = text.trim_start();
    let rest = if let Some(r) = trimmed.strip_prefix('\u{23FA}') {
        r
    } else {
        return false;
    };
    let rest = rest.trim_start();
    if rest.is_empty() {
        return false;
    }
    // Must start with uppercase ASCII (ToolName) or `mcp__` prefix.
    let first = rest.as_bytes()[0];
    if !first.is_ascii_uppercase() && !rest.starts_with("mcp__") {
        return false;
    }
    // Find the opening paren — everything before it must be a single
    // identifier (no spaces). Rejects prose like "Boss, ci sono (molti)".
    rest.find('(').is_some_and(|pos| {
        let before = &rest[..pos];
        !before.is_empty()
            && before
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}

/// Detect a `❯ ` prompt line in a Claude Code fullscreen transcript-mode `[`
/// dump — see `synthesize_transcript_dump_block_events`'s doc comment.
/// Returns the prompt text (everything after "❯ ", possibly empty).
fn transcript_dump_prompt_text(text: &str) -> Option<&str> {
    text.trim_start().strip_prefix("\u{276F} ")
}

/// Detect a `✻ ... · done ...` turn-completion line in a `[` dump.
fn is_transcript_dump_turn_end(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with('\u{273B}') && trimmed.contains("\u{b7} done")
}

/// Synthesize `AgentBlock` events from a Claude Code fullscreen
/// transcript-mode `[` dump — the "write the full conversation to native
/// scrollback" gesture (`Ctrl+O` to enter transcript mode, then `[`).
/// Verified live (throwaway session via the HTTP API, 2026-09-15): pressing
/// `[` genuinely exits the alternate screen buffer and prints the whole
/// conversation as plain, human-formatted text into REAL primary-screen
/// scrollback — `❯ <prompt>` starts each turn, `✻ ... · done <time>` ends
/// it. Unlike live/transcript-mode fullscreen (which has no real scrollback
/// to anchor into at all — see `is_alternate_screen()`'s callers elsewhere in
/// this file), this dump IS real, monotonically-increasing primary content,
/// so blocks synthesized from it get `on_alt_screen: false` and work with
/// every row-anchored feature (gutter, scrollbar, fold, jump-nav,
/// block-scoped search) exactly like a real shell block would.
///
/// Deliberately narrow: only for a hook-instrumented Claude Code session
/// (`agent_type == "claude" && hook_instrumented`) — a `❯ ` prompt glyph is
/// not exclusive to this dump (this repo's own zsh prompt can use one), so
/// this stays scoped to exactly the one agent/scenario it was verified
/// against. The caller is also expected to only invoke this for chunks where
/// the alternate screen is NOT active — this function does not check that
/// itself, since by construction (`on_alt_screen: false` always) it has
/// nothing else to gate on.
///
/// Per the docs, `[` re-dumps the *entire* conversation from scratch each
/// time it's pressed — every `❯ ` line here is just treated as a fresh block
/// boundary, the same as `synthesize_cc_block_events` does for `⏺` headers,
/// regardless of whether the same logical turn was already recorded by an
/// earlier dump. What stops a repeat gesture from leaving a second,
/// overlapping copy of the same blocks in `commandBlocks[]` forever
/// (2026-09-15) is `is_new_generation`, not text-level dedup: the caller
/// tracks whether the alternate screen was visited since the last dump
/// activity (only possible between two dumps, since `[` is only reachable
/// from transcript mode) and passes `true` exactly once, for the chunk that
/// resumes dump activity after such a visit. This function marks the first
/// `start` event it synthesizes from that chunk `new_dump_generation: true`;
/// the frontend responds by pruning every prior `fromTranscriptDump: true`
/// block (including a still-open `activeBlock`) before adding the new one —
/// see `terminals.ts`'s `handleOsc133`. A dump that only ever *adds* turns to
/// the screen without an intervening alt-screen visit (impossible today,
/// since Claude Code's fullscreen renderer owns the whole session between
/// dumps, but not a structural assumption this function makes) would
/// correctly keep `is_new_generation: false` and never prune.
///
/// `teardown_end_line` closes a still-open dump block the moment this
/// function's own gate (`agent_type`/`hook_instrumented`) stops holding —
/// mirrors `synthesize_cc_block_events`'s `!agent_active` branch. Without
/// this, `last_dump_block_line` could stay `Some` forever if the gate flips
/// before a `✻ … · done` marker ever appears (the agent exits). A dump left
/// open by leaving transcript mode instead (no gate flip) is handled by the
/// generation-prune above once the next dump starts, not by this teardown.
fn synthesize_transcript_dump_block_events(
    changed_rows: &[crate::state::ChangedRow],
    total_scrolled: usize,
    agent_type: Option<&str>,
    hook_instrumented: bool,
    teardown_end_line: usize,
    last_dump_block_line: &mut Option<usize>,
    is_new_generation: bool,
) -> Vec<ParsedEvent> {
    if agent_type != Some("claude") || !hook_instrumented {
        if let Some(prev) = last_dump_block_line.take() {
            return vec![ParsedEvent::AgentBlock {
                action: "end".into(),
                line: teardown_end_line.max(prev + 1) as i64,
                exit_code: None,
                prompt_text: None,
                on_alt_screen: false,
                from_transcript_dump: true,
                new_dump_generation: false,
            }];
        }
        return Vec::new();
    }
    let mut events = Vec::new();
    // Only the very first `start` event synthesized after `is_new_generation`
    // came in true carries `new_dump_generation: true` — consumed here so a
    // chunk containing several `❯` rows doesn't repeat the signal for every one.
    let mut pending_new_generation = is_new_generation;
    for row in changed_rows {
        let abs_line = total_scrolled + row.row_index;
        if let Some(prompt_text) = transcript_dump_prompt_text(&row.text) {
            if Some(abs_line) == *last_dump_block_line {
                continue;
            }
            // Mirrors synthesize_cc_block_events's clamp: the new block's
            // start must never precede the end just emitted for the block
            // it's closing.
            let start_line = if let Some(prev) = *last_dump_block_line {
                let end_line = abs_line.max(prev + 1);
                events.push(ParsedEvent::AgentBlock {
                    action: "end".into(),
                    line: end_line as i64,
                    exit_code: None,
                    prompt_text: None,
                    on_alt_screen: false,
                    from_transcript_dump: true,
                    new_dump_generation: false,
                });
                end_line
            } else {
                abs_line
            };
            events.push(ParsedEvent::AgentBlock {
                action: "start".into(),
                line: start_line as i64,
                exit_code: None,
                prompt_text: if prompt_text.is_empty() {
                    None
                } else {
                    Some(prompt_text.to_string())
                },
                on_alt_screen: false,
                from_transcript_dump: true,
                new_dump_generation: std::mem::take(&mut pending_new_generation),
            });
            *last_dump_block_line = Some(start_line);
        } else if is_transcript_dump_turn_end(&row.text)
            && let Some(prev) = last_dump_block_line.take()
        {
            events.push(ParsedEvent::AgentBlock {
                action: "end".into(),
                line: abs_line.max(prev + 1) as i64,
                exit_code: None,
                prompt_text: None,
                on_alt_screen: false,
                from_transcript_dump: true,
                new_dump_generation: false,
            });
        }
    }
    events
}

/// Synthesize `AgentBlock` start/end events from Claude Code `⏺ ToolName(args)`
/// tool-call headers — the fallback block source for sessions without hook
/// instrumentation (see `has_tuic_state_integration`; the turn-level idle↔busy
/// edge is the primary source and is unconditionally preferred once present).
///
/// `end` always carries the *exclusive* upper bound of the block being
/// closed — the next header's absolute line, or (on agent teardown) one past
/// the last written row — never the closing block's own start line and never
/// `abs_line - 1`. `CommandBlock.endLine` is exclusive throughout the
/// frontend (fold height, block-scoped search), so a block whose `endLine`
/// equals its own `promptLine` silently breaks folding.
///
/// `total_scrolled` (the grid's eviction-stable running total — see
/// `TerminalGrid::total_scrolled_count`) replaced a plain `history_size` here
/// 2026-09-15: `history_size` plateaus once the scrollback ring saturates
/// while `row_index` keeps cycling through the same on-screen rows, so two
/// unrelated headers far apart in real time could land on the identical
/// `abs_line` — not just risk `end < start` for the block being closed right
/// now (the `.max(prev + 1)` clamp below still guards that narrower case),
/// but alias a long-past block's stored row onto a brand new one. See
/// src-tauri/AGENTS.md > Command Blocks > "Scrollback-ring eviction".
///
/// `on_alt_screen` reflects whether the alternate screen buffer was active
/// for this chunk. `total_scrolled`/`row_index` are read from whichever screen
/// (primary or alt) was active at that instant — a fullscreen TUI never grows
/// real alt-screen history, so `abs_line` there is a transient on-screen
/// cursor row, not a valid scrollback anchor. Tagged through unchanged so
/// row-anchored consumers can skip these blocks without this function having
/// to know anything about rendering.
fn synthesize_cc_block_events(
    changed_rows: &[crate::state::ChangedRow],
    total_scrolled: usize,
    agent_active: bool,
    teardown_end_line: usize,
    on_alt_screen: bool,
    last_agent_block_line: &mut Option<usize>,
) -> Vec<ParsedEvent> {
    let mut events = Vec::new();
    if !agent_active {
        if let Some(prev) = last_agent_block_line.take() {
            events.push(ParsedEvent::AgentBlock {
                action: "end".into(),
                line: teardown_end_line.max(prev + 1) as i64,
                exit_code: None,
                prompt_text: None,
                on_alt_screen,
                from_transcript_dump: false,
                new_dump_generation: false,
            });
        }
        return events;
    }
    for row in changed_rows {
        if !is_cc_tool_call_header(&row.text) {
            continue;
        }
        let abs_line = total_scrolled + row.row_index;
        if Some(abs_line) == *last_agent_block_line {
            continue;
        }
        // The new block's start must never precede the end just emitted for
        // the block it's closing — otherwise the same scrollback-saturation
        // regression that requires clamping `end` (see doc comment above)
        // produces an `end` ahead of an unclamped, regressed `start`,
        // overlapping the two blocks. Reuse the clamped end line as the new
        // start whenever there was a previous block to close.
        let start_line = if let Some(prev) = *last_agent_block_line {
            let end_line = abs_line.max(prev + 1);
            events.push(ParsedEvent::AgentBlock {
                action: "end".into(),
                line: end_line as i64,
                exit_code: None,
                prompt_text: None,
                on_alt_screen,
                from_transcript_dump: false,
                new_dump_generation: false,
            });
            end_line
        } else {
            abs_line
        };
        events.push(ParsedEvent::AgentBlock {
            action: "start".into(),
            line: start_line as i64,
            exit_code: None,
            prompt_text: None,
            on_alt_screen,
            from_transcript_dump: false,
            new_dump_generation: false,
        });
        *last_agent_block_line = Some(start_line);
    }
    events
}

/// Emit an `Inferred` command outcome for shells that don't speak OSC 133.
/// Called right after a busy→idle transition; no-op once we've ever observed
/// a marker for this session (shell-integration path is authoritative then).
/// The command text is unknown in this mode, but cwd + snippet still populate
/// context summary and cwd history.
fn record_inferred_outcome_if_no_osc133(state: &AppState, session_id: &str) {
    use crate::ai_agent::knowledge::{CommandOutcome, OutcomeClass};

    if state
        .session_maps
        .has_osc133_integration
        .contains_key(session_id)
    {
        return;
    }
    // try_lock to avoid blocking the timer thread if write_pty holds
    // the session lock. Inferred outcomes are best-effort — missing cwd
    // for one record is acceptable vs risking contention.
    let cwd = state
        .session_maps
        .sessions
        .get(session_id)
        .and_then(|s| s.try_lock().and_then(|s| s.cwd.clone()))
        .unwrap_or_default();
    let output_snippet = state
        .grid
        .vt_log_buffers
        .get(session_id)
        .map(|b| {
            let buf = b.lock();
            buf.screen_rows().join("\n")
        })
        .unwrap_or_default();
    let mut tail_start = output_snippet.len().saturating_sub(500);
    while tail_start > 0 && !output_snippet.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    let output_snippet = output_snippet[tail_start..].to_string();

    let outcome = CommandOutcome {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        command: String::new(),
        cwd,
        exit_code: None,
        output_snippet,
        classification: OutcomeClass::Inferred,
        duration_ms: 0,
        id: 0,
    };
    state.record_outcome(session_id, outcome);
}

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

        let protocol_stale =
            screen_activity == AgentScreenActivity::Ready && silence.protocol_busy_is_stale();
        let screen_confirms_idle = match screen_activity {
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
        let is_busy = state
            .session_maps
            .shell_states
            .get(session_id)
            .is_some_and(|atom| atom.load(std::sync::atomic::Ordering::Acquire) == SHELL_BUSY);
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
        let decision = if screen_confirms_idle && probe != ForegroundProbe::Pending {
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
        if !screen_confirms_idle {
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
                expected: SHELL_BUSY,
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
                }
                continue;
            }

            if orchestrator_recipient_for_pty(&state, &session_id)
                .and_then(|recipient| state.orchestrator_wake_needed_through(&recipient))
                .is_some()
            {
                silence.lock().expire_orchestrator_notice_uncertainty();
            }

            // Reconcile high-confidence screen evidence before the silence
            // fallback. Working here means Codex's presence-based status line
            // (the only screen classifier that returns Working, #446-596f); it
            // runs regardless of current state so it repairs an already-false-
            // idle session instead of merely keeping a pre-existing BUSY alive.
            // Claude/Gemini/Aider BUSY is movement-driven in the reader.
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
                    tracing::debug!(
                        session_id,
                        activity_source = transition.evidence.map(|e| e.source).unwrap_or("unknown"),
                        rank = ?transition.evidence.map(|e| e.rank),
                        idle_confirmed = silence.lock().idle_confirmed(),
                        "Shell state → idle"
                    );
                    emit_shell_state(&state, &session_id, "idle");
                    reevaluate_orchestrator_mail_wake(&state, &session_id);
                    flush_pending_injections(&state, &session_id);
                    record_inferred_outcome_if_no_osc133(&state, &session_id);
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

/// Retract a low-confidence `awaiting_input` once the screen is quiet and no
/// question is visible any more.
///
/// The three existing clears all need an event that may never arrive: a typed
/// non-empty line (`UserInput`), a choice-prompt key (`resolve_choice_prompt_input`),
/// or a parsed `status-line`. A user who answers an approval dialog with a bare
/// Enter, or an agent whose spinner never parses as a status line, produces
/// none of them — the badge then reads "question" for the rest of the session
/// with the prompt long gone from the screen.
///
/// Only the heuristic (`confident == false`) state is retracted. A confident
/// question stays sticky on purpose: grok keeps repainting while it waits, so
/// "not on screen right now" is not proof that it was answered. A live
/// `choice_prompt` owns its own resolution and is left alone.
fn emit_question_cleared_if_stale(state: &Arc<AppState>, session_id: &str) {
    let turn_epoch = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|s| {
            // Research note (2026-09-01): this guard is BY DESIGN never allowed to
            // retract a confident question or one with an open choice_prompt — see the
            // module-level docs on why (a confident source can still repaint while
            // genuinely waiting; screen absence alone isn't proof of an answer). If a
            // session is stuck "awaiting" and this log line below never appears for
            // it, that's the tell: the badge is confident/choice-prompt-owned, so this
            // backstop was never going to be the thing that clears it — look at the
            // hook busy re-affirmation path (`tuic_state_awaiting_event`) or
            // `resolve_choice_prompt_input`/`choice-cleared` instead.
            if s.awaiting_input && (s.question_confident || s.choice_prompt.is_some()) {
                tracing::debug!(
                    session_id = %session_id,
                    confident = s.question_confident,
                    has_choice_prompt = s.choice_prompt.is_some(),
                    "silence_timer: awaiting_input is stale-eligible on screen but the \
                     confident/choice_prompt guard blocks this backstop from clearing it \
                     (research: unexpected state transitions)"
                );
            }
            (s.awaiting_input && !s.question_confident && s.choice_prompt.is_none())
                .then_some(s.turn_epoch)
        });
    let Some(turn_epoch) = turn_epoch else {
        return;
    };
    tracing::debug!(
        session_id = %session_id,
        "silence_timer: retracting stale awaiting_input (no question on screen)"
    );
    let parsed = ParsedEvent::QuestionCleared;
    if let Ok(mut json) = serde_json::to_value(&parsed) {
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

/// Publish the explicit end-of-task marker only after the shell has settled.
/// A completed lifecycle event is emitted from the same drain point, so an
/// orchestrator never has to reinterpret an ambiguous BUSY→IDLE transition.
fn emit_pending_suggest_if_idle(
    state: &AppState,
    silence: &Arc<Mutex<SilenceState>>,
    session_id: &str,
) -> bool {
    let shell_is_idle = state
        .session_maps
        .shell_states
        .get(session_id)
        .map(|atom| atom.load(std::sync::atomic::Ordering::Acquire) == SHELL_IDLE)
        .unwrap_or(false);
    if !shell_is_idle {
        return false;
    }
    // Serialize completion emission against note_submitted_input, which takes
    // this same lock before advancing SessionState.turn_epoch and clearing the
    // old turn. Whichever owns the lock first defines the lifecycle order.
    let mut silence_state = silence.lock();
    let Some((current_turn_epoch, background_work, background_probe_pending)) = state
        .session_maps
        .session_states
        .get(session_id)
        .map(|session| {
            (
                session.turn_epoch,
                session.background_work,
                session.has_pending_background_probe(),
            )
        })
    else {
        return false;
    };
    // `background_probe_pending`: an open question (a ready screen or explicit
    // idle marker was seen, but no process snapshot newer than that observation
    // has confirmed or denied a live descendant yet) counts as work, same as
    // every other reader of these three signals (`state.rs::session_state_with_shell`,
    // `try_shell_transition_locked`'s parent-idle suppression) — publishing
    // "completed" before the probe resolves would race a real background
    // descendant that just hasn't been confirmed yet.
    if background_work
        || background_probe_pending
        || silence_state.declared_background_work_for_epoch(current_turn_epoch)
    {
        return false;
    }
    let Some((turn_epoch, items)) = silence_state.drain_pending_suggest_with_epoch() else {
        return false;
    };
    if turn_epoch != current_turn_epoch {
        if silence_state.completion_turn_epoch == turn_epoch {
            silence_state.completion_declared = false;
            silence_state.completion_turn_epoch = 0;
        }
        return false;
    }
    emit_suggest_event(state, session_id, turn_epoch, items);
    let parent_dispatch = enqueue_state_change_to_parent(
        state,
        session_id,
        serde_json::json!({
            "type": "state_change",
            "state": "completed",
            "session_id": session_id,
        }),
    );
    drop(silence_state);
    if let Some(dispatch) = parent_dispatch {
        dispatch_parent_lifecycle(state, dispatch);
    }
    reevaluate_orchestrator_mail_wake(state, session_id);
    true
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
///   `last_prompt`) — fires on a *real* idle→busy edge (`busy_transitioned`) OR
///   whenever the session is currently awaiting (`currently_awaiting`).
///
///   The `currently_awaiting` half of that OR is the 2026-09-01 fix: `claude_hook_map()`
///   has a narrow `PostToolUse(AskUserQuestion|ExitPlanMode)` busy re-affirmation
///   (deliberately kept, specifically to clear the awaiting state after the tool
///   resolves). But `PreToolUse(AskUserQuestion)`'s `awaiting` override never touches
///   the shell busy/idle bit at all (see `handle_tuic_state`'s doc comment — there is
///   no SHELL_AWAITING), so the *normal* case — `AskUserQuestion` firing mid-turn, shell
///   already SHELL_BUSY before and after — left `busy_transitioned` false on that very
///   re-affirmation, silently dropping the one clear path a confident question has
///   besides real user input. Live-reproduced 2026-09-01 on two unattended/auto-approve
///   sessions: the badge stuck "awaiting" through the rest of the run, including full
///   task completion, because the agent answered its own `AskUserQuestion` with no
///   keystroke ever reaching the PTY. Gating on `busy_transitioned` alone was correct
///   for suppressing a *duplicate* green "you submitted a prompt" scrollbar tick on
///   every redundant busy re-affirmation — that concern is unaffected, since
///   `handle_tuic_state`'s own transition (and thus its `AgentBlock` emission) is
///   still edge-gated; only this awaiting-clear decision, a separate concern reusing
///   the same bool, needed to stop being edge-only.
/// - anything else (incl. `idle`, unknown) → `None`
fn tuic_state_awaiting_event(
    payload: &str,
    line: i64,
    busy_transitioned: bool,
    currently_awaiting: bool,
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
        // `line` is the absolute prompt row (history_size + cursor row) at the
        // busy transition — the row the user's submitted prompt sits on. Carried
        // so the frontend can mark user-prompt lines on the scrollbar.
        "busy" if busy_transitioned || currently_awaiting => {
            if !busy_transitioned {
                // The edge-repair path: a redundant busy re-affirmation that would
                // otherwise be dropped, kept alive only because the session is
                // still (confidently or not) awaiting_input. No log at the normal
                // edge-transitioned call site below — this arm exists specifically
                // to catch the case that one can't.
                tracing::debug!(
                    "tuic_state_awaiting_event: clearing awaiting_input via non-edge \
                     busy re-affirmation (shell already busy, no idle↔busy edge) — \
                     the PostToolUse(AskUserQuestion|ExitPlanMode) re-affirmation case"
                );
            }
            Some(ParsedEvent::UserInput {
                content: String::new(),
                line,
            })
        }
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
/// parser first classifies whether the protocol notification actually requires
/// a response. A qualifying OSC 777 notification is the only awaiting signal
/// for a hook-instrumented agent whose prompt is not
/// `PreToolUse(AskUserQuestion)` — a plan or skill Ink picker emits no hook
/// state at all, which is why such a session sat blocked behind a "working" dot.
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
    hook_instrumented: bool,
    awaiting_input: bool,
    has_choice_prompt: bool,
    question_this_tick: bool,
) -> Option<ParsedEvent> {
    if hook_instrumented || awaiting_input || has_choice_prompt || question_this_tick {
        return None;
    }
    crate::output_parser::ink_dialog_footer(screen).map(|footer| ParsedEvent::Question {
        prompt_text: footer.to_string(),
        confident: true,
    })
}

/// Per-session mutable state for processing PTY output chunks.
/// Holds dedup state, parser, and session CWD for PlanFile resolution.
/// Used by `spawn_reader_thread`.
struct ChunkProcessor {
    parser: OutputParser,
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
    /// Structured terminal mode with nesting depth and app detection.
    terminal_mode: crate::ai_agent::tui_detect::TerminalMode,
    /// One-shot flag: inject ESC[2J before the next ESC[H cursor-home.
    /// Set on alt-buffer entry and when content may have shrunk (detected via
    /// cursor-up ESC[nA with n > previous). Consumed after inject fires.
    alt_buffer_needs_clear: bool,
    /// Tracks the largest cursor-up (ESC[nA) value seen since last clear.
    /// When a new ESC[nA arrives with n < last_cursor_up_n, content has shrunk
    /// and we need a clear to prevent ghost artifacts.
    last_cursor_up_n: u16,
    /// Last VtLogBuffer total_lines observed — distinguishes a chunk that
    /// scrolled in new output from one that only repainted existing rows.
    last_vt_log_total: usize,
    /// Command text captured on OSC 133 C — used when the matching D arrives
    /// to build a `CommandOutcome`. Cleared after D.
    pending_command: Option<String>,
    /// `Instant` when OSC 133 C arrived; used for `duration_ms`.
    pending_command_started: Option<std::time::Instant>,
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

impl ChunkProcessor {
    fn new(session_cwd: Option<String>, tuic_session: Option<String>) -> Self {
        Self {
            parser: OutputParser::new(),
            last_status_task: None,
            last_question_text: None,
            raw_carry: String::new(),
            last_choice_prompt_sig: None,
            session_cwd,
            pending_planfiles: Vec::new(),
            emitted_planfiles: std::collections::HashSet::new(),
            gaveup_planfiles: std::collections::HashSet::new(),
            in_alt_buffer: false,
            terminal_mode: crate::ai_agent::tui_detect::TerminalMode::Shell,
            alt_buffer_needs_clear: false,
            last_cursor_up_n: 0,
            last_vt_log_total: 0,
            pending_command: None,
            pending_command_started: None,
            tuic_session,
            last_session_conflict_mark: None,
            last_agent_block_line: None,
            last_dump_block_line: None,
            dump_saw_alt_screen: false,
            title_awaiting: false,
            screen_buf: Vec::new(),
            pending_open_urls: Vec::new(),
        }
    }

    /// Handle OSC 7770 `state=idle|busy` from the TUIC protocol. Returns
    /// whether this was a real transition (used by the caller to gate the
    /// sibling `UserInput`/green-tick emission in `tuic_state_awaiting_event`,
    /// which is a separate consumer of the same OSC event) and, on a real
    /// idle↔busy edge, the `AgentBlock` marking a turn-level command block's
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
            "busy" => (SHELL_BUSY, "busy"),
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

    /// Handle a single OSC 133 event from the VTE handler.
    /// On 'C' captures the command text; on 'D' builds a `CommandOutcome`.
    fn handle_osc133_event(
        &mut self,
        command: char,
        params: &str,
        session_id: &str,
        state: &AppState,
    ) {
        use crate::ai_agent::knowledge::{CommandOutcome, OutcomeClass, classify_error};

        // Deterministic state transitions from shell integration markers.
        // A = prompt shown (idle), C = command execution started (busy).
        // These bypass the silence timer entirely when OSC 133 is available.
        match command {
            'A' => {
                transition_explicit_shell_state(state, session_id, SHELL_IDLE, "idle", false);
            }
            'C' => {
                transition_explicit_shell_state(state, session_id, SHELL_BUSY, "busy", false);
                let cmd = state
                    .session_maps
                    .input_buffers
                    .get(session_id)
                    .map(|b| b.lock().content())
                    .unwrap_or_default();
                self.pending_command = Some(cmd);
                self.pending_command_started = Some(std::time::Instant::now());
            }
            'D' => {
                // A 'D' (command finished) with no preceding 'C' (command
                // started) means no command actually ran — e.g. Enter on an
                // empty prompt, where the shell still emits D carrying the
                // previous command's exit code. Recording it would create a
                // phantom outcome with an empty command and "unknown" error
                // type, polluting both the knowledge panel and the agent's
                // injected prompt. Skip it.
                if self.pending_command_started.is_none() {
                    self.pending_command = None;
                    return;
                }
                let exit_code = params.parse::<i32>().unwrap_or(0);
                let command = self.pending_command.take().unwrap_or_default();
                let duration_ms = self
                    .pending_command_started
                    .take()
                    .map(|t| t.elapsed().as_millis() as u64)
                    .unwrap_or(0);
                let cwd = self.session_cwd.clone().unwrap_or_default();
                let output_snippet = state
                    .grid
                    .vt_log_buffers
                    .get(session_id)
                    .map(|b| {
                        let buf = b.lock();
                        buf.screen_rows().join("\n")
                    })
                    .unwrap_or_default();
                let mut tail_start = output_snippet.len().saturating_sub(500);
                while tail_start > 0 && !output_snippet.is_char_boundary(tail_start) {
                    tail_start += 1;
                }
                let output_snippet = output_snippet[tail_start..].to_string();

                let classification = if exit_code == 0 {
                    OutcomeClass::Success
                } else if let Some(error_type) = classify_error(&output_snippet) {
                    OutcomeClass::Error { error_type }
                } else {
                    OutcomeClass::Error {
                        error_type: "unknown".into(),
                    }
                };

                let outcome = CommandOutcome {
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                    command,
                    cwd,
                    exit_code: Some(exit_code),
                    output_snippet,
                    classification,
                    duration_ms,
                    id: 0,
                };
                state.knowledge_entry(session_id).lock().terminal_mode = self.terminal_mode.clone();
                state.record_outcome(session_id, outcome);
            }
            _ => {}
        }
    }

    /// Classify an inline TUI (mouse reporting on the primary screen) the same
    /// way `transform_xterm` classifies `1049h`. Alt-screen nesting still owns
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

    /// Colorize `intent:` tokens and apply alternate-buffer fixes on the xterm
    /// stream. Suggest tokens are NOT concealed here — the frontend's
    /// `eraseSuggestFromBuffer()` handles that via rAF after xterm renders.
    fn transform_xterm<'a>(&mut self, data: &'a str) -> Option<std::borrow::Cow<'a, str>> {
        // Track alternate screen buffer state for the clear-before-home fix below.
        if data.contains("\x1b[?1049h") {
            self.in_alt_buffer = true;
            self.alt_buffer_needs_clear = true;
            self.terminal_mode = self.terminal_mode.on_alt_enter();
        } else if data.contains("\x1b[?1049l") {
            self.in_alt_buffer = false;
            self.alt_buffer_needs_clear = false;
            self.terminal_mode = self.terminal_mode.on_alt_exit();
        }

        // Detect render-height change in alternate buffer: when Ink's cursor-up
        // (ESC[nA) value changes, the chrome area may have shifted vertically.
        // Ink never sends ESC[K (erase to end of line), so rows that were chrome
        // in the previous render but aren't overwritten in the new one persist as
        // ghost artifacts — starting from the bottom and expanding upward.
        if self.in_alt_buffer
            && let Some(n) = extract_largest_cursor_up(data)
        {
            if n != self.last_cursor_up_n && self.last_cursor_up_n > 0 {
                self.alt_buffer_needs_clear = true;
            }
            self.last_cursor_up_n = n;
        }

        // Inject ESC[2J (clear screen) before the first positioning sequence when
        // needed. Tries cursor-home (ESC[H) first, then falls back to cursor-up
        // (ESC[nA). Ink re-renders use cursor-up for repositioning, not cursor-home,
        // so the fallback is essential — without it the flag accumulates forever.
        // Borrowed unless an injection actually fires: the overwhelming majority
        // of chunks pass straight through, and this used to copy every one.
        if !self.alt_buffer_needs_clear {
            return Some(std::borrow::Cow::Borrowed(data));
        }
        let injected = inject_clear_before_cursor_home(data);
        if injected.len() != data.len() {
            self.alt_buffer_needs_clear = false;
            return Some(std::borrow::Cow::Owned(injected));
        }
        let injected = inject_clear_before_cursor_up(data);
        if injected.len() != data.len() {
            self.alt_buffer_needs_clear = false;
            return Some(std::borrow::Cow::Owned(injected));
        }
        Some(std::borrow::Cow::Borrowed(data))
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
    /// the SAME borrowed bytes to `transform_xterm`. It used to return an owned
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
            on_alt_screen,
            total_scrolled,
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
                total_scrolled,
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
                let total_scrolled = vt.grid_total_scrolled();
                let alt_screen = vt.is_alternate_screen();
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
                    total_scrolled,
                    alt_screen,
                    mouse_reporting,
                    tevts,
                    pending_replies,
                    kitty_image_store,
                    kitty_pending_jobs,
                )
            };

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
            if let Some(screen) = screen_ref
                && !changed.is_empty()
            {
                let refs: Vec<&str> = screen.iter().map(String::as_str).collect();
                // Fails OPEN by contract: no anchor found is `None`, and `None`
                // must mean "parse everything", never "parse nothing".
                if let Some(cutoff) = crate::chrome::find_chrome_cutoff(&refs) {
                    changed.retain(|r| r.row_index < cutoff);
                }
            }
            let changed = changed;

            // Screen classification runs on EVERY chunk, borrowed, never cloned: a
            // repaint that is byte-identical produces no changed rows, and holding
            // BUSY through exactly that case (a frozen spinner, DEC 2026 frame
            // coalescing) is the point of `detect_agent_screen_activity`.
            let screen_activity = screen_ref
                .map(|rows| detect_agent_screen_activity(agent_type.as_deref(), rows))
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
            let cursor_row = vt.cursor_point().0;
            let logical_prefix = vt.logical_prefix_at_cursor();
            let physical_prefix = vt.physical_prefix_at_cursor();

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
                alt_screen,
                total_scrolled,
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
                false,
                0,
            )
        };

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
                        #[cfg(feature = "desktop")]
                        if let Some(a) = state.app_handle.read().as_ref() {
                            let _ = a.emit(&format!("pty-title-{session_id}"), &title);
                        }
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
                            tuic_events.push(ParsedEvent::Question {
                                prompt_text: clean_action_required_title(&title),
                                confident: true,
                            });
                        }
                        self.title_awaiting = title_awaiting;
                    }
                    TermEvent::ResetTitle => {
                        #[cfg(feature = "desktop")]
                        if let Some(a) = state.app_handle.read().as_ref() {
                            let _ = a.emit(&format!("pty-title-{session_id}"), "");
                        }
                        self.title_awaiting = false;
                    }
                    TermEvent::ClipboardStore(text) => {
                        #[cfg(feature = "desktop")]
                        if let Some(a) = state.app_handle.read().as_ref() {
                            let _ = a.emit(&format!("pty-clipboard-store-{session_id}"), &text);
                        }
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
                        "state" => {
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
                            let (transitioned, block_event) = self.handle_tuic_state(
                                &payload,
                                session_id,
                                line as i64,
                                on_alt_screen,
                                state,
                            );
                            if let Some(evt) = block_event {
                                tuic_events.push(evt);
                            }
                            let currently_awaiting = state
                                .session_maps
                                .session_states
                                .get(session_id)
                                .is_some_and(|s| s.awaiting_input);
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
                                transitioned,
                                currently_awaiting,
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
                            tuic_events.push(ParsedEvent::Intent { text, title });
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
        if let Some(ring) = state.session_maps.output_buffers.get(session_id) {
            let mut ring_guard = ring.lock();
            ring_guard.write(data.as_bytes());
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
        // Cursor-completeness guard: parse a suggest token from the bounded grid
        // prefix through the cursor, never from stale cells to its right. When a
        // soft-wrapped continuation changes in a later chunk, replace its whole
        // physical range with one synthetic logical row so the unchanged anchor
        // remains available to the existing parser. Intent deferral is unchanged.
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
        events.extend(
            self.parser
                .parse_clean_lines(rows, agent_active_for_parse)
                .into_iter()
                .filter(|e| !suppress_heuristic_question(hook_instrumented, e)),
        );

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
        if slash_on && let Some(screen) = screen_cache {
            // This runs in the per-PTY-chunk hot path. Do not log each parse:
            // a stale slash-mode flag during sustained output previously sent
            // thousands of identical records through the application logger,
            // adding avoidable lock/contention pressure to terminal delivery.
            if let Some(evt) = crate::output_parser::parse_slash_menu(screen) {
                events.push(evt);
            }
        }

        // ChoicePrompt detection — numbered confirmation dialogs rendered below
        // the prompt line (edit-confirm, bash-confirm, apply-patch). Runs on
        // every chunk (unlike slash_menu which is gated by slash_mode) because
        // these dialogs appear asynchronously when the agent requests input.
        // Parser uses a strict shape (title with ?/verb + ≥2 numbered options)
        // so false-positive cost is low. Dedup via last_choice_prompt_sig
        // guards against repaint re-emission.
        //
        // Suppressed entirely for a hook-instrumented session (2026-09-01 fix):
        // `suppress_heuristic_question` already drops the generic `Question`-type
        // heuristic here, but that filter never covered `ChoicePrompt` — a numbered
        // confirmation dialog kept getting screen-scraped for Claude sessions whose
        // hooks already report the exact same block authoritatively via
        // `state=awaiting`. Confirmed live: a hook-instrumented session logged both
        // `[ChoicePrompt] ... title="Do you want to proceed?"` AND a hook-driven
        // confident `question — awaitingInput transition` for the same dialog,
        // seconds apart — a confusing duplicate signal, not a second real event.
        // Skip detection outright rather than filtering the event after the fact,
        // so a hook-instrumented session never carries `last_choice_prompt_sig`/
        // `choice_prompt` state for a signal it has deliberately opted out of.
        if !hook_instrumented && let Some(screen) = &screen_cache {
            match crate::output_parser::parse_choice_prompt(screen) {
                Some(evt) => {
                    tracing::debug!(
                        session_id = %session_id,
                        event = ?evt,
                        "ChoicePrompt detected (screen-scrape, non-hook-instrumented session)"
                    );
                    events.push(evt);
                }
                // Dialog is no longer on screen — retire its dedup signature so the
                // same dialog is detected again the next time it appears, instead of
                // being swallowed for the rest of the session.
                None => {
                    if self.last_choice_prompt_sig.take().is_some() {
                        events.push(ParsedEvent::ChoiceCleared);
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
        // spurious clear, never one per frame. Hook-instrumented sessions are
        // excluded for the same reason `suppress_heuristic_question` excludes them
        // — OSC 7770 owns their state.
        //
        // DEFERRED (2026-08-21) — hook-instrumented agents keep the same gap: a
        // multi-question AskUserQuestion fires PreToolUse once, so sub-questions 2+
        // have no hook signal either. Needs a capture with hooks ON to confirm
        // before widening this to them.
        if let Some(screen) = screen_cache {
            let (awaiting, has_choice) = state
                .session_maps
                .session_states
                .get(session_id)
                .map(|s| (s.awaiting_input, s.choice_prompt.is_some()))
                .unwrap_or((false, false));
            let question_this_tick = events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. }));
            if let Some(evt) = rearm_awaiting_for_open_dialog(
                screen,
                hook_instrumented,
                awaiting,
                has_choice,
                question_this_tick,
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
                    state.note_marker(session_id, crate::state::MarkerKind::Intent)
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
            // spinner tick — so skipping the dedup for it is safe.
            if let ParsedEvent::Question { prompt_text, .. } = event
                && !prompt_text.is_empty()
            {
                if self.last_question_text.as_deref() == Some(prompt_text.as_str()) {
                    continue;
                }
                self.last_question_text = Some(prompt_text.clone());
            }

            // Dedup choice-prompt: skip if same (title + option keys) already emitted.
            // Signature keeps option order but ignores highlighted drift so cursor
            // movement within the dialog doesn't re-fire. Retired when the dialog
            // leaves the screen (see the parse site above).
            if let ParsedEvent::ChoicePrompt { title, options, .. } = event {
                let sig = format!(
                    "{}|{}",
                    title,
                    options
                        .iter()
                        .map(|o| o.key.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                );
                if self.last_choice_prompt_sig.as_deref() == Some(sig.as_str()) {
                    continue;
                }
                self.last_choice_prompt_sig = Some(sig);
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
        //   Aider Knight Rider) AND no row contains real agent output. A row is
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
            sl.cached_screen_activity = screen_activity;
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
        // Spinner rows (dingbats ✻, braille ⠋, Aider ░█) prove the agent is
        // alive even though they are chrome-only — keeping the timestamp fresh
        // prevents should_transition_idle from firing mid-think.
        //
        // Spinner detection runs on the SAME post-cutoff `changed_rows` as
        // everything else. Real spinners (Gemini braille, Aider Knight Rider,
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
                    .is_some_and(|session| sl.completion_declared_for_epoch(session.turn_epoch));
                let blocked = (turn_completed || sl.explicit_idle()) && !can_reopen_completed;
                if !blocked {
                    let reopen = can_reopen_completed && (turn_completed || sl.explicit_idle());
                    if reopen {
                        // Claude can emit Stop/suggest before a blocking Stop hook
                        // finishes; Codex can start an internal continuation
                        // without a PTY submission. Current semantic movement is
                        // stronger than either stale boundary.
                        sl.reset_suggest_memory();
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
            if real_activity {
                if has_spinner {
                    sl.note_working_screen();
                } else {
                    sl.note_real_activity();
                }
                invalidate_background_probe_boundary_locked(state, session_id);
                if sl.evidence.busy.is_none() {
                    let source = if has_spinner {
                        "spinner-active"
                    } else {
                        "real-activity"
                    };
                    sl.record_busy(EvidenceRank::Screen, source);
                }
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
            && let Some(Transition::ToBusy(evidence)) = decide(
                &evidence_snapshot,
                prev == SHELL_BUSY,
                std::time::Instant::now(),
            )
            && try_shell_transition(state, session_id, prev, SHELL_BUSY, true)
        {
            tracing::debug!(
                session_id,
                activity_source = evidence.source,
                rank = ?evidence.rank,
                "Shell state → busy"
            );
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
    if let Err(error) = state.write_pty_parts(session_id, &[response]) {
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

/// Drop every trace of a peer identity nobody can reach any more, and tell
/// subscribers the address is gone.
///
/// Two callers below retire an identity for different reasons — the PTY backing
/// it died, or the last child naming it as parent did — and both must clear the
/// same maps. Keeping one list here is the same discipline
/// [`remove_live_session_state`] enforces for per-session state: a new
/// peer-keyed map belongs in this function and nowhere else.
fn retire_peer_identity(state: &AppState, tuic_session: &str) {
    state.peer_agents.remove(tuic_session);
    state.orchestrator_peers.remove(tuic_session);
    state.agent_inbox.remove(tuic_session);
    state.agent_inbox_evictions.remove(tuic_session);
    state.agent_read_cursor.remove(tuic_session);
    state.active_agent_waiters.remove(tuic_session);
    state.pending_injections.remove(tuic_session);
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::PeerUnregistered {
            tuic_session: tuic_session.to_string(),
        });
}

/// Per-session state owned by the running process: streams, input, shell status,
/// and the swarm identities the PTY was backing. Reaped the moment the process
/// dies, whether or not a readable tombstone outlives it.
///
/// This and [`remove_post_mortem_session_state`] are the *only* two enumerations
/// of per-session maps. Three call sites compose them — `cleanup_session` runs
/// both, `tombstone_transient_cleanup` runs this one, `spawn_tombstone_sweeper`
/// runs the other. Each used to keep its own hand-written list, and the three had
/// drifted: an explicit close left every peer identity behind, and a session that
/// exited normally leaked its terminal alias for the life of the process.
/// **A new per-session map belongs in one of these two functions and nowhere else.**
fn remove_live_session_state(session_id: &str, state: &AppState) {
    state.ws_clients.remove(session_id);
    // Drop the per-session PTY event channel alongside ws_clients. Any final
    // SessionClosed already emitted stays buffered for live subscribers (broadcast
    // drains buffered messages before signalling Closed), so no close frame is lost.
    state.session_maps.pty_event_channels.remove(session_id);
    #[cfg(feature = "desktop")]
    state.grid.channels.remove(session_id);
    state.grid.watch.remove(session_id);
    state.grid.gates.remove(session_id);
    state.grid.pending_scroll.remove(session_id);
    state.session_maps.kitty_states.remove(session_id);
    state.session_maps.input_buffers.remove(session_id);
    state.session_maps.silence_states.remove(session_id);
    state.session_maps.shell_states.remove(session_id);
    state.session_maps.last_prompts.remove(session_id);
    state.session_maps.pty_descriptions.remove(session_id);
    state.session_maps.terminal_rows.remove(session_id);
    state.session_maps.resize_locks.remove(session_id);
    state.session_maps.pty_accent_colors.remove(session_id);
    // Input mode and shell integration describe the process that just died.
    state.session_maps.slash_mode.remove(session_id);
    state.session_maps.last_input_ms.remove(session_id);
    // These three integration/flag markers must not leak — every session that
    // ever spoke OSC 133 or OSC 7770 left a permanent dead entry keyed by its
    // UUID otherwise, on both this path and the explicit close/kill path
    // (`cleanup_session`, which composes this function).
    state.session_maps.has_osc133_integration.remove(session_id);
    state
        .session_maps
        .has_tuic_state_integration
        .remove(session_id);
    state.session_maps.turn_error_flags.remove(session_id);
    // Swarm maps — inserted at spawn/register time, must be cleaned on exit.
    state.session_maps.shell_state_since_ms.remove(session_id);
    // A peer that announced its own `$TUIC_SESSION` is filed under that identity,
    // not under the PTY key — so the `peer_agents.remove(session_id)` below has
    // never matched it, and its registration outlived the terminal for the whole
    // process lifetime. Retire the identities this PTY was backing as well.
    for orphaned in state.unbind_live_pty(session_id) {
        retire_peer_identity(state, &orphaned);
    }
    state.pending_injections.remove(session_id);
    state.pending_initial_prompts.remove(session_id);
    state.active_agent_waiters.remove(session_id);
    state.peer_agents.remove(session_id);
    state.orchestrator_peers.remove(session_id);
    state.agent_inbox.remove(session_id);
    state.agent_inbox_evictions.remove(session_id);
    // The inbox read position is meaningless once the inbox is gone.
    state.agent_read_cursor.remove(session_id);
    #[cfg(unix)]
    state.session_maps.standby_sessions.remove(session_id);
    // DEFERRED (2026-08-25) — a parent identity retained ONLY because this child
    // named it (`peer_identity_is_reapable`) is never re-examined once the child
    // goes: the reaper walks the peers of the MCP session it is collecting, and the
    // parent's was collected long ago. The address then survives for the life of
    // the process as the phantom `retire_repaired_phantom_identity` describes —
    // advertised by `list_peers`, swallowing every message sent to it.
    //
    // Retiring it HERE was tried and is wrong twice over: `mark_session_exited` has
    // just pushed this child's `state_change` into that inbox, and a headerless
    // orchestrator can still reclaim the identity later with `register
    // replaces=<old_uuid>` to collect exactly that mail. Both make "no live PTY, no
    // live MCP session, no child" too weak a test for deletion. The real fix is a
    // periodic sweep over ALL peers with a mail-retention rule, which is a policy
    // decision, not a cleanup tweak.
    state.session_maps.session_parent.remove(session_id);
    // mcp_to_session maps mcp_session_id → tuic_session. The reverse index
    // session_to_mcp lets us drop O(k) entries (k = mcp sessions for this
    // tuic_session, typically 1) instead of scanning every entry.
    if let Some((_, mcp_sids)) = state.mcp.session_to_mcp.remove(session_id) {
        for sid in &mcp_sids {
            state.mcp.to_session.remove(sid);
        }
    }
}

/// Per-session state a tombstone keeps readable after the process is gone: the
/// buffers, the exit code, the alias the tab still shows, and the accumulated
/// knowledge a background task has yet to flush. Reaped when the tombstone ages
/// out — or immediately, when the session is closed outright.
///
/// See [`remove_live_session_state`] for why these are the only two lists.
fn remove_post_mortem_session_state(session_id: &str, state: &AppState) {
    state.session_maps.output_buffers.remove(session_id);
    state.grid.vt_log_buffers.remove(session_id);
    state.grid.pty_raw_rings.remove(session_id);
    state.session_maps.last_output_ms.remove(session_id);
    state.session_maps.exit_codes.remove(session_id);
    state.session_maps.term_aliases.remove(session_id);
    state.session_maps.marker_stats.remove(session_id);
    state.session_maps.session_visibility.remove(session_id);
    state.ai.ai_suggestions_enabled.remove(session_id);
    state
        .session_maps
        .scrollback_capture_marks
        .remove(session_id);
}

// NOT A DEFERRAL — four session-keyed maps are deliberately NOT reaped by
// either half, because the session is not what owns them:
//   * `file_sandboxes` / `unrestricted_sessions` belong to the L2 conversation,
//     which registers in ACTIVE_CONVERSATIONS and removes both when its task
//     exits (`ai_agent::conversation_engine`). A conversation outlives its PTY —
//     it can sit in an approval wait with no deadline — so a session-lifetime
//     reap pulls the sandbox out from under a running file tool.
//   * `session_knowledge` / `knowledge_dirty` ARE the cross-session memory:
//     `knowledge::summarize_for_repo` and the agent prompt builder read the live
//     map, never the files, so reaping a closed session removes knowledge the
//     next session in that repo is supposed to inherit. Residency is bounded at
//     startup (40 newest), not during a run.
// Both need an owner-scoped lifetime, not a session-scoped one. Tie them to
// ACTIVE_CONVERSATIONS and to a running residency bound respectively.

/// Fully remove session state from all DashMaps.
/// Called on explicit close/kill — caller has already consumed any output they need.
pub(crate) fn cleanup_session(session_id: &str, state: &AppState, reason: &str) {
    // Before `remove_live_session_state`, same ordering requirement as
    // `close_pty_core`/`kill_pty_core`: that call reaps
    // `state.pty_event_channels`, and emitting after it would silently drop
    // the session-scoped WS "closed" frame.
    emit_session_closed(state, session_id, reason);
    if state.session_maps.sessions.remove(session_id).is_some() {
        state
            .metrics
            .active_sessions
            .fetch_sub(1, Ordering::Relaxed);
    }
    remove_live_session_state(session_id, state);
    remove_post_mortem_session_state(session_id, state);
}

/// Reap the state the dead process owned, and stamp `last_output_ms` so the
/// tombstone sweeper can age the entry out. What a post-mortem read needs stays —
/// see [`remove_post_mortem_session_state`].
fn tombstone_transient_cleanup(session_id: &str, state: &AppState) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    state
        .session_maps
        .last_output_ms
        .entry(session_id.to_string())
        .or_insert_with(|| AtomicU64::new(0))
        .store(now_ms, Ordering::Relaxed);
    // Capture scrollback under this session's $TUIC_SESSION identity before
    // remove_live_session_state unbinds it below. This is the only point that
    // catches a tab closed mid-run — every exit path (close, kill, process
    // died on its own) funnels through here.
    let (restore_scrollback, max_scrollback_lines) = {
        let cfg = state.config.read();
        (
            cfg.restore_scrollback,
            cfg.restore_scrollback_lines as usize,
        )
    };
    if restore_scrollback && let Some(tuic_session) = state.tuic_session_for_live_pty(session_id) {
        crate::scrollback_store::capture_session(
            state,
            session_id,
            &tuic_session,
            max_scrollback_lines,
            now_ms,
        );
    }
    remove_live_session_state(session_id, state);
}

struct ParentLifecycleDispatch {
    parent_id: String,
    message_id: String,
    message_timestamp: u64,
    framed: String,
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
    bool,
    usize,
);

/// Render one lifecycle payload as a single human-facing line, without the
/// `[TUIC] ` marker so it also composes into a multi-event summary.
///
/// Shared by the direct framed delivery and the orchestrator summary notice:
/// the two describe the same events from different sources (the payload being
/// enqueued vs. the copy read back out of the inbox) and must never word them
/// differently.
///
/// The result is injected into an agent's composer, so it MUST stay one short
/// line — a multi-line paste submits itself halfway through.
fn describe_lifecycle_payload(child_session: &str, payload: &serde_json::Value) -> String {
    let child = short_session(child_session);
    if payload.get("type").and_then(|t| t.as_str()) == Some("prompt_delivered") {
        return format!("child agent {child} has taken its initial prompt after all");
    }
    if payload.get("type").and_then(|t| t.as_str()) == Some("prompt_delivery_failed") {
        return match payload.get("reason").and_then(|r| r.as_str()) {
            Some("startup_dialog") => format!(
                "child agent {child} is stalled on a startup dialog; its prompt is queued and will be typed once it is answered"
            ),
            _ => format!(
                "child agent {child} has not taken its initial prompt yet; it stays queued for the child's next ready window"
            ),
        };
    }
    let state_desc = payload
        .get("state")
        .and_then(|s| s.as_str())
        .unwrap_or("changed");
    let prompt_excerpt = payload
        .get("prompt")
        .and_then(|p| p.as_str())
        .map(|p| {
            let flat = p.split_whitespace().collect::<Vec<_>>().join(" ");
            if flat.chars().count() > 120 {
                format!("{}…", flat.chars().take(120).collect::<String>())
            } else {
                flat
            }
        })
        .filter(|p| !p.is_empty());
    match (
        payload.get("exit_code").and_then(|c| c.as_i64()),
        prompt_excerpt,
    ) {
        (Some(code), _) => format!("child agent {child} {state_desc} (exit {code})"),
        (None, Some(prompt)) => format!(
            "child agent {child} is now {state_desc} — answer it with session action=input: {prompt}"
        ),
        (None, None) => format!("child agent {child} is now {state_desc}"),
    }
}

/// Enqueue the authoritative parent lifecycle message without touching the
/// parent's PTY lifecycle lock. BUSY→IDLE and completed paths call this while
/// holding the child's SilenceState transaction lock.
fn enqueue_state_change_to_parent(
    state: &AppState,
    session_id: &str,
    payload: serde_json::Value,
) -> Option<ParentLifecycleDispatch> {
    let parent_id = state
        .session_maps
        .session_parent
        .get(session_id)
        .map(|e| e.value().clone())?;
    if parent_id == session_id {
        return None;
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let msg = crate::state::AgentMessage {
        id: format!("tuic-auto-{}-{}", session_id, now_ms),
        from_tuic_session: session_id.to_string(),
        from_name: "tuic".to_string(),
        content: serde_json::to_string(&payload).unwrap_or_default(),
        timestamp: now_ms,
        delivered_via_channel: false,
    };
    let message_id = msg.id.clone();
    // DEFERRED (2026-08-05) — this push does not take PEER_IDENTITY_BIND_LOCK, so a
    // lifecycle notice resolved against a parent that is being retired
    // (retire_repaired_phantom_identity) can still land in a drained inbox. The
    // peer-to-peer `send` path was serialized against the retire in story 546-33cb;
    // this one needs the resolution of `parent_id` and the push to share that guard
    // too. Left out of that story's scope deliberately — it needs its own repro,
    // since the parent id here comes from session_parent rather than a caller.
    let message_timestamp = state.push_agent_inbox(&parent_id, msg);
    let framed = format!(
        "[TUIC] {}",
        describe_lifecycle_payload(session_id, &payload)
    );
    let dispatch = ParentLifecycleDispatch {
        parent_id: parent_id.clone(),
        message_id: message_id.clone(),
        message_timestamp,
        framed,
    };
    // Role selection and ownership are deliberately deferred together until
    // after the child lifecycle lock is released. Splitting those decisions
    // allowed a concurrent orchestrator-role removal to create a generic wake
    // and an ordinary payload delivery for the same buffered notification.
    Some(dispatch)
}

/// Wake/dispatch only after the child lifecycle lock has been released. This
/// may acquire the parent's SilenceState lock through terminal delivery.
fn dispatch_parent_lifecycle(state: &AppState, dispatch: ParentLifecycleDispatch) {
    if route_registered_orchestrator_mail(
        state,
        &dispatch.parent_id,
        &dispatch.message_id,
        dispatch.message_timestamp,
    )
    .is_some()
    {
        return;
    }
    if state.assign_agent_delivery(
        &dispatch.parent_id,
        &dispatch.message_id,
        state.live_pty_for_peer(&dispatch.parent_id).is_some(),
    ) != crate::state::AgentDeliveryAssignment::Terminal
    {
        return;
    }
    let outcome = deliver_notice_to_managed_pty(state, &dispatch.parent_id, &dispatch.framed);
    settle_terminal_delivery(state, &dispatch.parent_id, &dispatch.message_id, outcome);
}

/// Push a state_change message and wake the parent when no child lifecycle
/// transaction is active (for example, process exit and direct test helpers).
pub(crate) fn push_state_change_to_parent(
    state: &Arc<AppState>,
    session_id: &str,
    payload: serde_json::Value,
) {
    if let Some(dispatch) = enqueue_state_change_to_parent(state, session_id, payload) {
        // The inbox push above already happened on this thread — that is the
        // authoritative copy an `agent wait` can observe. Only the terminal wake
        // is deferred, because it is the part that sleeps `INJECT_ENTER_GAP`, and
        // both live producers here are tokio workers (the session-state
        // accumulator and the reader thread's exit path).
        let state = Arc::clone(state);
        spawn_injection_job(move || dispatch_parent_lifecycle(&state, dispatch));
    }
}

/// Emit the single exceptional-path notification for an initial prompt that has
/// not reached the child's composer yet.
///
/// The prompt is deliberately NOT dropped. A child that stalls on a startup
/// dialog ("Do you trust the contents of this directory?") is not a child whose
/// task is void — once the dialog is answered it reaches a ready prompt and the
/// queued entry is typed. Removing the marker here (as this used to) reported
/// the failure *and* silently discarded the work, so nothing retried and the
/// child sat idle as if it had been spawned with nothing to do.
///
/// `notified` keeps the watchdog one-shot without that loss, and the payload
/// carries both the detected cause and the prompt itself so a parent that
/// prefers to re-deliver by hand has the text.
pub(crate) fn notify_initial_prompt_timeout_if_pending(
    state: &Arc<AppState>,
    session_id: &str,
) -> bool {
    let prompt = {
        let Some(mut pending) = state.pending_initial_prompts.get_mut(session_id) else {
            return false;
        };
        if pending.notified {
            return false;
        }
        pending.notified = true;
        pending.prompt.clone()
    };
    let Some(parent_id) = state
        .session_maps
        .session_parent
        .get(session_id)
        .map(|entry| entry.value().clone())
    else {
        tracing::warn!(session = %session_id, "Initial prompt delivery timed out without a registered parent");
        return false;
    };
    let now_ms = now_epoch_ms();
    // Dialog detection, rather than reporting every stall as a bare timeout: a
    // confident question is the one cause the server can name, and it is the
    // one that resolves by itself the moment a human answers it.
    let reason = if blocked_on_confident_question(state, session_id) {
        "startup_dialog"
    } else {
        "timeout"
    };
    let payload = serde_json::json!({
        "type": "prompt_delivery_failed",
        "reason": reason,
        "session_id": session_id,
        // The prompt is still queued for the child's next ready window; a parent
        // that wants to re-deliver it itself does not have to have kept a copy.
        "retrying": true,
        "prompt": prompt,
    });
    let message_id = format!("tuic-auto-prompt-{session_id}-{now_ms}");
    let message_timestamp = state.push_agent_inbox(
        &parent_id,
        crate::state::AgentMessage {
            id: message_id.clone(),
            from_tuic_session: session_id.to_string(),
            from_name: "tuic".to_string(),
            content: serde_json::to_string(&payload).unwrap_or_default(),
            timestamp: now_ms,
            delivered_via_channel: false,
        },
    );
    if route_registered_orchestrator_mail(state, &parent_id, &message_id, message_timestamp)
        .is_some()
    {
        return true;
    }
    if state.assign_agent_delivery(
        &parent_id,
        &message_id,
        state.session_maps.sessions.contains_key(&parent_id),
    ) != crate::state::AgentDeliveryAssignment::Terminal
    {
        return true;
    }
    // Fired from a tokio watchdog task, so the wake goes to the injection worker.
    let framed = format!(
        "[TUIC] {}",
        describe_lifecycle_payload(session_id, &payload)
    );
    let state = Arc::clone(state);
    spawn_injection_job(move || {
        let outcome = deliver_notice_to_managed_pty(&state, &parent_id, &framed);
        settle_terminal_delivery(&state, &parent_id, &message_id, outcome);
    });
    true
}

/// Tell the parent that a prompt it was warned about has now been typed.
///
/// Only emitted after a `prompt_delivery_failed` notice for the same child: an
/// orchestrator that was told the task never landed must not be left believing
/// that. Silence would be the worse of the two lies, because the only recovery
/// it leaves is re-delivering a prompt that is already running.
fn notify_initial_prompt_delivered(state: &AppState, session_id: &str) {
    if let Some(dispatch) = enqueue_state_change_to_parent(
        state,
        session_id,
        serde_json::json!({
            "type": "prompt_delivered",
            "session_id": session_id,
        }),
    ) {
        dispatch_parent_lifecycle(state, dispatch);
    }
}

/// First 8 chars of a session UUID, for compact human-facing labels.
fn short_session(session_id: &str) -> &str {
    session_id.get(..8).unwrap_or(session_id)
}

/// Whether a framed peer message should be typed into `session_id` right now
/// rather than queued. True only for an agent session that is idle and not
/// blocked on a *confident* user-facing question — writing into a busy Ink TUI
/// can corrupt its render, and writing into a plain shell would execute the
/// message as a command.
///
/// The gate is `question_confident`, NOT `awaiting_input`: agents that idle at
/// a ready prompt (codex) sit permanently at `awaiting_input=true` via the
/// low-confidence silence heuristic, which would starve delivery forever
/// (story 091). Confident questions (Ink footer, cliclack `◆ …?`, "Action
/// Required" titles) still block injection so a peer message never answers a
/// real approval prompt.
fn idle_is_confirmed(state: &AppState, session_id: &str) -> bool {
    let confirmed = state
        .session_maps
        .silence_states
        .get(session_id)
        .map(|sl| sl.lock().idle_confirmed())
        .unwrap_or(false);
    if confirmed {
        return true;
    }
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|s| s.agent_type.clone());
    // Preserve legacy behavior for agents without a verified ready-screen
    // adapter. Hook-enabled variants become confirmed via explicit Stop; the
    // remaining heuristics cannot yet provide a stronger proof.
    !has_ready_screen_adapter(agent_type.as_deref())
}

/// Whether an agent owns this session's composer. Injection is agent-only: in a
/// plain shell the idle atom says nothing about what holds stdin, so typed text
/// would reach whatever program is running rather than the shell.
fn session_is_agent(state: &AppState, session_id: &str) -> bool {
    state
        .session_maps
        .session_states
        .get(session_id)
        .map(|s| s.agent_type.is_some())
        .unwrap_or(false)
}

/// Whether a confident user-facing question currently owns this composer — the
/// startup trust dialog, an approval prompt, an Ink footer choice. Named
/// separately from `should_inject_now` because the prompt-delivery watchdog
/// reports it as a cause, not merely as a reason to wait.
pub(crate) fn blocked_on_confident_question(state: &AppState, session_id: &str) -> bool {
    state
        .session_maps
        .session_states
        .get(session_id)
        .map(|s| s.question_confident)
        .unwrap_or(false)
}

pub(crate) fn should_inject_now(state: &AppState, session_id: &str) -> bool {
    if !session_is_agent(state, session_id) {
        return false;
    }
    let idle = state
        .session_maps
        .shell_states
        .get(session_id)
        .map(|a| a.load(std::sync::atomic::Ordering::Relaxed) == SHELL_IDLE)
        .unwrap_or(false);
    let blocked_on_question = state
        .session_maps
        .session_states
        .get(session_id)
        .map(|s| s.question_confident)
        .unwrap_or(false);
    idle && idle_is_confirmed(state, session_id)
        && !blocked_on_question
        && !has_partial_user_input(state, session_id)
}

/// True while the user has characters sitting in the composer. Injecting then
/// would splice our text into what they are typing.
fn has_partial_user_input(state: &AppState, session_id: &str) -> bool {
    state
        .session_maps
        .input_buffers
        .get(session_id)
        .is_some_and(|buffer| !buffer.lock().content().is_empty())
}

/// Reserve an idle agent composer for one injected command.
///
/// `should_inject_now` is only a snapshot. The agent may become busy between
/// that read and the PTY write, so the final IDLE→BUSY transition must be an
/// atomic compare-exchange. A lost race leaves the message queued instead of
/// typing it into an active composer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InjectionClaim {
    token: u64,
}

fn claim_idle_for_injection(state: &AppState, session_id: &str) -> Option<InjectionClaim> {
    if !should_inject_now(state, session_id) {
        return None;
    }
    let prior_idle_confirmed = state
        .session_maps
        .silence_states
        .get(session_id)
        .map(|silence| silence.lock().idle_confirmed())
        .unwrap_or(false);
    if !try_shell_transition(state, session_id, SHELL_IDLE, SHELL_BUSY, true) {
        return None;
    }
    // The composer is re-read after the atom is ours: `should_inject_now` was a
    // snapshot, and the user can start typing in between. Revert before the
    // claim exists so no spurious busy/idle pair reaches the UI.
    if has_partial_user_input(state, session_id) {
        try_shell_transition(state, session_id, SHELL_BUSY, SHELL_IDLE, true);
        return None;
    }
    let token = state
        .session_maps
        .silence_states
        .get(session_id)
        .map(|silence| silence.lock().begin_injection_claim(prior_idle_confirmed))
        .unwrap_or(0);
    emit_shell_state(state, session_id, "busy");
    Some(InjectionClaim { token })
}

fn rollback_injection_claim(state: &AppState, session_id: &str, claim: InjectionClaim) -> bool {
    let owns_claim = state
        .session_maps
        .silence_states
        .get(session_id)
        .and_then(|silence| silence.lock().rollback_injection_claim(claim.token))
        .is_some();
    if !owns_claim {
        return false;
    }
    if try_shell_transition(state, session_id, SHELL_BUSY, SHELL_IDLE, true) {
        emit_shell_state(state, session_id, "idle");
        true
    } else {
        false
    }
}

fn mark_injection_uncertain(state: &AppState, session_id: &str, claim: InjectionClaim) {
    if let Some(silence) = state.session_maps.silence_states.get(session_id) {
        silence.lock().mark_injection_uncertain(claim.token);
    }
}

fn mark_orchestrator_notice_uncertain(state: &AppState, session_id: &str, claim: InjectionClaim) {
    if let Some(silence) = state.session_maps.silence_states.get(session_id) {
        silence
            .lock()
            .mark_orchestrator_notice_uncertain(claim.token);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AgentSubmissionWrite {
    Complete {
        acknowledgement_offset: u64,
    },
    Rejected {
        reason: &'static str,
        composer_state: &'static str,
        /// What is parked ahead of this submission, for `queued_commands_pending`.
        /// Empty for every other reason.
        pending: Vec<PendingInjectionSummary>,
    },
    Failed(String),
    Uncertain(String),
}

/// One parked entry, as a rejected submission reports it.
///
/// `queued_commands_pending` used to be a bare string against a `queued_commands`
/// count that deliberately excluded server entries, so a caller looking at an
/// empty composer and an empty Compose queue had nothing to act on. The blocker
/// now names itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PendingInjectionSummary {
    pub id: u64,
    pub kind: &'static str,
    /// First line, truncated. Enough to recognise the entry; the full text is on
    /// the Compose queue listing.
    pub preview: String,
}

const PENDING_PREVIEW_MAX_CHARS: usize = 80;

fn summarize_pending_injections(
    state: &AppState,
    session_id: &str,
) -> Vec<PendingInjectionSummary> {
    state
        .pending_injections
        .get(session_id)
        .map(|queue| {
            queue
                .iter()
                .map(|entry| PendingInjectionSummary {
                    id: entry.id(),
                    kind: entry.kind(),
                    preview: truncate_chars(
                        entry.text().lines().next().unwrap_or_default(),
                        PENDING_PREVIEW_MAX_CHARS,
                    ),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    format!("{}…", text.chars().take(max).collect::<String>())
}

fn agent_submission_rejection(
    state: &AppState,
    session_id: &str,
) -> Option<(&'static str, &'static str)> {
    if !state.session_maps.sessions.contains_key(session_id) {
        return Some(("session_not_found", "unknown"));
    }
    if !session_is_agent(state, session_id) {
        return Some(("not_managed_agent", "unknown"));
    }
    if has_partial_user_input(state, session_id) {
        return Some(("partial_composer", "partial"));
    }
    if state
        .session_maps
        .session_states
        .get(session_id)
        .is_some_and(|session| session.question_confident)
    {
        return Some(("awaiting_input", "empty"));
    }
    // Readiness is checked BEFORE the queue, and the order is the fix. An agent
    // whose idle is unconfirmed can never drain its queue — `flush_pending_injections`
    // is gated on the same predicate — so reporting `queued_commands_pending`
    // named the symptom and hid the cause, and the caller retried submit for
    // minutes against a queue that by construction could not move.
    // `agent_not_ready` is the truth, and it is the state that actually changes.
    if !should_inject_now(state, session_id) {
        return Some(("agent_not_ready", "empty"));
    }
    // The agent IS ready, so anything still parked can be typed right now.
    // Level-triggered: the BUSY→IDLE edge that normally drains this queue may
    // already have passed, and nothing else would fire it. Draining here is the
    // same write the transition would have made, one item per idle window.
    if state
        .pending_injections
        .get(session_id)
        .is_some_and(|queue| !queue.is_empty())
    {
        flush_pending_injections_blocking(state, session_id);
    }
    if state
        .pending_injections
        .get(session_id)
        .is_some_and(|queue| !queue.is_empty())
    {
        return Some(("queued_commands_pending", "empty"));
    }
    // Re-read: a flush that emptied the queue typed one entry and left the
    // session BUSY, so the caller is now waiting on that turn, not on a queue.
    if !should_inject_now(state, session_id) {
        return Some(("agent_not_ready", "empty"));
    }
    None
}

/// Claim and write one MCP-managed agent command without queueing it.
///
/// This is the ordering half of `session action=submit`. The caller owns the
/// existing input-bookkeeping FSM and the bounded acknowledgement wait after a
/// complete write. Rejections happen before the first byte. Once the claim is
/// held, concurrent peer delivery observes BUSY and queues behind this command.
pub(crate) fn write_agent_submission_to_pty(
    state: &AppState,
    session_id: &str,
    text: &str,
) -> AgentSubmissionWrite {
    if let Some((reason, composer_state)) = agent_submission_rejection(state, session_id) {
        return AgentSubmissionWrite::Rejected {
            reason,
            composer_state,
            pending: summarize_pending_injections(state, session_id),
        };
    }
    let Some(claim) = claim_idle_for_injection(state, session_id) else {
        let (reason, composer_state) =
            agent_submission_rejection(state, session_id).unwrap_or(("claim_lost", "unknown"));
        return AgentSubmissionWrite::Rejected {
            reason,
            composer_state,
            pending: summarize_pending_injections(state, session_id),
        };
    };

    let (outcome, acknowledgement_offset) =
        write_agent_command_with_boundary(state, session_id, text);
    match outcome {
        InjectionOutcome::Submitted => {
            // Terminal movement during the split write can invalidate the claim;
            // that is independent evidence, not a reason to discard a completed
            // write. Clear the token when it is still ours. The MCP caller advances
            // the turn through InputLineBuffer exactly once.
            if let Some(silence) = state.session_maps.silence_states.get(session_id) {
                silence.lock().commit_injection_claim(claim.token);
            }
            AgentSubmissionWrite::Complete {
                acknowledgement_offset,
            }
        }
        InjectionOutcome::NotStarted(error) => {
            rollback_injection_claim(state, session_id, claim);
            AgentSubmissionWrite::Failed(error)
        }
        InjectionOutcome::Uncertain(error) => {
            mark_injection_uncertain(state, session_id, claim);
            AgentSubmissionWrite::Uncertain(error)
        }
    }
}

/// Build the first write of an injection: Ctrl-U clears any pending input, and
/// multiline text rides inside a bracketed paste (ESC[200~ … ESC[201~) so the
/// TUI keeps embedded newlines as paste content and the trailing CR (sent as a
/// separate write) lands as a real Enter keypress. Mirrors the frontend
/// `sendCommand.ts` recipe exactly — raw multiline text merely PREFILLS
/// codex/claude without submitting (verified live, story 091).
fn injection_payload(text: &str) -> String {
    if text.contains('\n') {
        format!("\x15\x1b[200~{text}\x1b[201~")
    } else {
        format!("\x15{text}")
    }
}

/// Real-time gap inserted between the payload write and the Enter write of an
/// injection. Ink/raw-mode agents (Codex, Claude Code) only treat the trailing
/// CR as a submit when it arrives in a SEPARATE `read()` from the text; a
/// microsecond-apart back-to-back write — even with a flush in between — is
/// coalesced into one read and the CR is swallowed as part of the typed buffer,
/// so the message just sits at the prompt unsubmitted (verified live against
/// Codex: back-to-back hangs, CR after a gap submits).
/// 50ms comfortably clears the child's read-scheduling latency while staying
/// imperceptible for a wake message.
///
/// This comment used to claim the frontend `sendCommand.ts` recipe "gets this
/// gap for free — its two `writeFn` calls are separate IPC round-trips". It does
/// NOT: a Tauri IPC round-trip completes well inside the child's read latency,
/// so both writes land in one `read()` and a clicked suggestion renders as a
/// newline instead of submitting. `sendCommand.ts` now waits the same 50ms
/// (`AGENT_ENTER_GAP_MS`) whenever an agent is attached. Keep the two constants
/// in step — separate flushes never guaranteed separate reads, only time does.
const INJECT_ENTER_GAP: std::time::Duration = std::time::Duration::from_millis(50);

/// One piece of injection work, handed off by a caller that must not block.
type InjectionJob = Box<dyn FnOnce() + Send + 'static>;

/// The thread that pays `INJECT_ENTER_GAP` so tokio workers do not.
///
/// Three producers reach injection from a tokio worker — the session-state
/// accumulator, the per-session silence timer, and the `agent wait` guard's
/// `Drop` — and each would park that worker for 50ms per message. They enqueue
/// here instead.
///
/// ONE thread, not one per job, and that is the whole design: lifecycle
/// notifications for a parent (`idle` → `completed` → `exited`) must reach its
/// composer in the order they were produced, and a thread per job would let
/// `exited` overtake `idle`. A single FIFO consumer preserves the ordering the
/// callers used to get for free by being synchronous.
///
/// The channel is unbounded on purpose: a bounded one could block the very
/// caller this exists to unblock, and a job may enqueue more work re-entrantly
/// (a delivery that transitions a session re-runs the flush).
static INJECTION_QUEUE: LazyLock<std::sync::mpsc::Sender<InjectionJob>> = LazyLock::new(|| {
    let (tx, rx) = std::sync::mpsc::channel::<InjectionJob>();
    std::thread::Builder::new()
        .name("tuic-injection".to_string())
        .spawn(move || {
            for job in rx {
                job();
            }
        })
        .expect("injection worker thread");
    tx
});

/// Block until every injection enqueued before this call has run.
///
/// The worker is FIFO, so a job that signals us cannot run ahead of the ones
/// queued before it. Tests asserting on the *result* of a detached injection
/// need this; polling for the effect instead turns each such assertion into a
/// timing race that reports a scheduling delay as a delivery bug.
#[cfg(test)]
pub(crate) fn wait_for_injection_queue() {
    let (tx, rx) = std::sync::mpsc::channel();
    spawn_injection_job(move || {
        let _ = tx.send(());
    });
    rx.recv().expect("injection worker must drain");
}

/// Run `job` on the injection worker instead of the calling thread.
pub(crate) fn spawn_injection_job(job: impl FnOnce() + Send + 'static) {
    // The receiver lives for the process, so this can only fail if the worker
    // panicked. Running the job inline would reintroduce exactly the block this
    // exists to remove, so report the loss instead of hiding it in a stall.
    if INJECTION_QUEUE.send(Box::new(job)).is_err() {
        tracing::error!("injection worker is gone; a queued injection was dropped");
    }
}

/// Write prompt text and a submitting Enter to an agent PTY using the exact
/// framing and timing required by raw-mode TUIs. The caller owns bookkeeping:
/// peer delivery records a synthetic submission, while MCP session input feeds
/// the original text and Enter through its input-state FSM. One writer guard
/// spans the real scheduling gap, so another producer cannot splice the line.
pub(crate) fn write_agent_command_to_pty(
    state: &AppState,
    session_id: &str,
    text: &str,
) -> Result<(), String> {
    match write_agent_command_with_boundary(state, session_id, text).0 {
        InjectionOutcome::Submitted => Ok(()),
        InjectionOutcome::NotStarted(error) | InjectionOutcome::Uncertain(error) => Err(error),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum InjectionOutcome {
    Submitted,
    NotStarted(String),
    Uncertain(String),
}

fn write_all_with_progress(
    writer: &mut dyn Write,
    bytes: &[u8],
    prior_bytes_written: usize,
) -> Result<(), (usize, String)> {
    let mut written = 0usize;
    while written < bytes.len() {
        match writer.write(&bytes[written..]) {
            Ok(0) => {
                return Err((
                    prior_bytes_written + written,
                    "Write failed: writer returned zero bytes".to_string(),
                ));
            }
            Ok(n) => written += n,
            Err(error) => {
                return Err((
                    prior_bytes_written + written,
                    format!("Write failed: {error}"),
                ));
            }
        }
    }
    Ok(())
}

fn write_agent_command_with_boundary(
    state: &AppState,
    session_id: &str,
    text: &str,
) -> (InjectionOutcome, u64) {
    let payload = injection_payload(text);
    let writer = match state.pty_writer(session_id) {
        Some(writer) => writer,
        None => {
            return (
                InjectionOutcome::NotStarted("Session not found".to_string()),
                0,
            );
        }
    };
    // One writer guard spans payload, scheduling gap, and Enter. The injection
    // claim orders managed peers; this mutex also keeps raw/UI writers from
    // splicing bytes into the command while the child is allowed to consume the
    // payload as a separate read.
    let mut writer = writer.lock();
    if let Err((written, error)) = write_all_with_progress(writer.as_mut(), payload.as_bytes(), 0) {
        return (
            if written == 0 {
                InjectionOutcome::NotStarted(error)
            } else {
                InjectionOutcome::Uncertain(error)
            },
            0,
        );
    }
    if let Err(error) = writer.flush() {
        return (
            InjectionOutcome::Uncertain(format!("Flush failed: {error}")),
            0,
        );
    }

    // Blocks the calling thread, under the writer guard, for the whole gap. Both
    // properties are load-bearing and neither is negotiable here: the child only
    // reads the CR as a submit when it arrives in a separate `read()`, and
    // `agent_submission_writer_lock_prevents_raw_input_splicing` pins the byte
    // sequence this guard protects. A caller that must not block therefore does
    // not shorten the gap — it stops being the thread that waits, by handing the
    // whole sequence to `INJECTION_QUEUE`.
    std::thread::sleep(INJECT_ENTER_GAP);

    // Exclude payload echo already observable before Enter. The async handler
    // checks this boundary only after the complete Enter write returns; movement
    // beyond it is child PTY output, never TUICommander's own turn bookkeeping.
    let acknowledgement_offset = state
        .session_maps
        .output_buffers
        .get(session_id)
        .map(|buffer| buffer.lock().total_written)
        .unwrap_or(0);
    if let Err((_, error)) = write_all_with_progress(writer.as_mut(), b"\r", payload.len()) {
        return (InjectionOutcome::Uncertain(error), acknowledgement_offset);
    }
    if let Err(error) = writer.flush() {
        return (
            InjectionOutcome::Uncertain(format!("Flush failed: {error}")),
            acknowledgement_offset,
        );
    }

    (InjectionOutcome::Submitted, acknowledgement_offset)
}

fn write_claimed_agent_command(state: &AppState, session_id: &str, text: &str) -> InjectionOutcome {
    write_agent_command_with_boundary(state, session_id, text).0
}

fn commit_injection_claim(state: &AppState, session_id: &str, claim: InjectionClaim) {
    let committed = state
        .session_maps
        .silence_states
        .get(session_id)
        .map(|silence| silence.lock().commit_injection_claim(claim.token))
        .unwrap_or(false);
    if committed {
        note_submitted_input(state, session_id);
    }
}

/// What an ambiguous write is allowed to do next.
///
/// Retrying a peer message risks typing it twice; an orchestrator notice is
/// either payload-free or a re-derivable state summary, so it is idempotent
/// enough to retry. This used to be inferred by comparing the text against
/// `PEER_MAIL_WAKE` — which silently stopped covering the notice once
/// it could also be a lifecycle summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClaimedInjectionKind {
    Message,
    OrchestratorNotice,
}

fn run_claimed_injection(
    state: &AppState,
    session_id: &str,
    text: &str,
    claim: InjectionClaim,
    kind: ClaimedInjectionKind,
) -> InjectionOutcome {
    let outcome = write_claimed_agent_command(state, session_id, text);
    apply_claimed_injection_outcome(state, session_id, text, claim, outcome, kind)
}

/// The only thing a peer `send` is ever allowed to put on a recipient's screen.
///
/// It is a pointer, not the message. Typing the payload itself was the whole
/// defect: a recipient's TUI cannot tell an injected line from something its
/// user typed, so mail arrived as keystrokes in the composer — rendered as
/// literal prompt text by one agent, and left sitting unsubmitted (composer
/// "partial") by another. The inbox is where mail lives; this line only tells
/// the recipient to go read it.
pub(crate) const PEER_MAIL_WAKE: &str =
    "[TUIC] message available — read it with: agent action=inbox";

/// Longest self-acknowledging summary we are willing to type into a composer.
/// Past this the notice stops being a cheap one-liner, so we fall back to the
/// generic wake — which is always correct, just one `inbox` call more expensive.
const ORCHESTRATOR_SUMMARY_MAX_CHARS: usize = 240;

/// Render the reserved wake group as a self-contained notice, or `None` when
/// the recipient must be sent to its inbox instead.
///
/// Why this exists: an orchestrator's inbox is dominated by server-authored
/// lifecycle notifications (`idle`, `completed`, `exited`) whose entire payload
/// is a state name. Making the orchestrator spend a tool call to discover
/// "child 8c26 went idle, then exited(0)" is pure round-trip with no
/// information gain, and it happens once per finished child.
///
/// Why it is conditional — the group must be lifecycle-only:
///   1. Peer `send` payloads are agent-authored, arbitrary length and
///      arbitrary content. They stay out of the composer, full stop (that is
///      the invariant `assign_orchestrator_delivery_with_wake_outcome` exists
///      to protect).
///   2. A partial summary would be worse than none: the recipient, satisfied
///      by what it read, would never call `inbox` and would silently lose the
///      messages the summary omitted. So a single non-lifecycle message in the
///      window disqualifies the whole group rather than being skipped.
fn summarize_lifecycle_group(
    state: &AppState,
    recipient: &str,
    group: crate::state::OrchestratorWakeGroup,
) -> Option<String> {
    let mut parts = Vec::new();
    {
        let inbox = state.agent_inbox.get(recipient)?;
        for message in inbox.iter().filter(|message| {
            message.timestamp > group.observed_through && message.timestamp <= group.wake_through
        }) {
            if !message
                .id
                .starts_with(crate::state::LIFECYCLE_MSG_ID_PREFIX)
            {
                return None;
            }
            let payload = serde_json::from_str::<serde_json::Value>(&message.content).ok()?;
            parts.push(describe_lifecycle_payload(
                &message.from_tuic_session,
                &payload,
            ));
        }
    }
    if parts.is_empty() {
        return None;
    }
    let summary = format!("[TUIC] {}", parts.join("; "));
    (summary.chars().count() <= ORCHESTRATOR_SUMMARY_MAX_CHARS).then_some(summary)
}

/// Submit one notification only when the registered parent's canonical
/// lifecycle still says idle/completed. Unlike ordinary managed-peer delivery,
/// a lost idle race is never queued: working and unknown lifecycle states
/// remain inbox-only and are not steered on a later transition.
///
/// The line is either a self-acknowledging lifecycle summary (see
/// `summarize_lifecycle_group`) or the payload-free generic wake.
fn submit_orchestrator_mail_wake(
    state: &AppState,
    session_id: &str,
    recipient: &str,
    group: crate::state::OrchestratorWakeGroup,
) -> crate::state::OrchestratorWakeAttemptOutcome {
    use crate::state::OrchestratorWakeAttemptOutcome;

    let wake_allowed = state
        .session_state_with_shell(session_id)
        .and_then(|session| session.agent_state)
        .is_some_and(|agent_state| matches!(agent_state.as_str(), "idle" | "completed"));
    if !wake_allowed {
        return OrchestratorWakeAttemptOutcome::NotStarted;
    }
    #[cfg(unix)]
    if let Err(error) = wake_session(state, session_id) {
        tracing::debug!(session = %session_id, error, "Orchestrator mail wake failed");
    }
    let Some(claim) = claim_idle_for_injection(state, session_id) else {
        return OrchestratorWakeAttemptOutcome::NotStarted;
    };
    let summary = summarize_lifecycle_group(state, recipient, group);
    let text = summary.as_deref().unwrap_or(PEER_MAIL_WAKE);
    match run_claimed_injection(
        state,
        session_id,
        text,
        claim,
        ClaimedInjectionKind::OrchestratorNotice,
    ) {
        // An uncertain write must NOT acknowledge: the cursor may only advance
        // behind a line we know reached the composer.
        InjectionOutcome::Submitted if summary.is_some() => {
            OrchestratorWakeAttemptOutcome::SummarySubmitted
        }
        InjectionOutcome::Submitted => OrchestratorWakeAttemptOutcome::Submitted,
        InjectionOutcome::NotStarted(_) => OrchestratorWakeAttemptOutcome::NotStarted,
        InjectionOutcome::Uncertain(_) => OrchestratorWakeAttemptOutcome::Uncertain,
    }
}

/// Route mail for a peer that has authoritatively acted as an orchestrator by
/// spawning a managed child: coalesced wakes, a self-acknowledging lifecycle
/// summary where the window allows one, and a strict no-queue policy.
///
/// Returns `None` for ordinary managed agents, which take the simpler
/// SSE-channel-or-`PEER_MAIL_WAKE` route. Both keep the same invariant — the
/// payload never reaches a composer — so what differs is only how the wake is
/// batched, not what a recipient may be shown.
pub(crate) fn route_registered_orchestrator_mail(
    state: &AppState,
    recipient: &str,
    message_id: &str,
    message_timestamp: u64,
) -> Option<crate::state::OrchestratorDeliveryAssignment> {
    if !state.orchestrator_peers.contains(recipient) {
        return None;
    }
    let pty_session = state.live_pty_for_peer(recipient);
    let wake_allowed = pty_session
        .as_deref()
        .and_then(|session_id| state.session_state_with_shell(session_id))
        .and_then(|session| session.agent_state)
        .is_some_and(|agent_state| matches!(agent_state.as_str(), "idle" | "completed"));
    let assignment = state.assign_orchestrator_delivery_with_wake_outcome(
        recipient,
        message_id,
        message_timestamp,
        wake_allowed,
        |group| match pty_session.as_deref() {
            Some(session_id) => submit_orchestrator_mail_wake(state, session_id, recipient, group),
            None => crate::state::OrchestratorWakeAttemptOutcome::NotStarted,
        },
    );
    // A self-acknowledging notice covers only the window it reserved. Mail that
    // landed while it was being typed keeps `orchestrator_wake_needed_through`
    // set, and nothing else would surface it: the idle/completed transition that
    // normally drives `reevaluate_orchestrator_mail_wake` has already happened.
    // Chase it here instead. Bounded: each pass spends one of
    // ORCHESTRATOR_WAKE_ATTEMPT_LIMIT attempts, and no budget is granted inside
    // one delivery, so the recursion stops at the limit.
    if assignment == crate::state::OrchestratorDeliveryAssignment::WakeSummarySubmitted
        && pty_session.is_some()
    {
        chase_orchestrator_mail_wake(state, recipient);
    }
    Some(assignment)
}

/// Retry buffered orchestrator mail when the managed PTY has reached a
/// canonical idle/completed lifecycle. Busy and unknown states remain inbox-only.
fn orchestrator_recipient_for_pty(state: &AppState, pty_session: &str) -> Option<String> {
    if state.orchestrator_peers.contains(pty_session) {
        Some(pty_session.to_string())
    } else {
        state.orchestrator_peers.iter().find_map(|peer| {
            let peer_id = peer.key();
            (state.live_pty_for_peer(peer_id).as_deref() == Some(pty_session))
                .then(|| peer_id.clone())
        })
    }
}

fn reevaluate_orchestrator_mail_wake(state: &AppState, pty_session: &str) {
    let Some(recipient) = orchestrator_recipient_for_pty(state, pty_session) else {
        return;
    };
    // An idle edge is new evidence, not a repeat of the attempt that failed to
    // start: re-arm before chasing, or a single unclaimable composer silences
    // every later retry for the rest of the session.
    state.rearm_orchestrator_wake_budget(&recipient);
    chase_orchestrator_mail_wake(state, &recipient);
}

/// Re-offer an outstanding wake on the budget the recipient already has. Unlike
/// [`reevaluate_orchestrator_mail_wake`] this grants nothing, so it is what a
/// caller inside one delivery uses to chase its own leftovers.
fn chase_orchestrator_mail_wake(state: &AppState, recipient: &str) {
    let Some(needed_through) = state.orchestrator_wake_needed_through(recipient) else {
        return;
    };
    let _ = route_registered_orchestrator_mail(
        state,
        recipient,
        "tuic-orchestrator-mail-notice",
        needed_through,
    );
}

fn apply_claimed_injection_outcome(
    state: &AppState,
    session_id: &str,
    text: &str,
    claim: InjectionClaim,
    outcome: InjectionOutcome,
    kind: ClaimedInjectionKind,
) -> InjectionOutcome {
    match &outcome {
        InjectionOutcome::Submitted => {
            commit_injection_claim(state, session_id, claim);
            if state
                .pending_initial_prompts
                .get(session_id)
                .is_some_and(|pending| pending.prompt == text)
            {
                // Delivery finally happened. If the watchdog already told the
                // parent it had not, close that loop rather than leaving the
                // parent holding a failure notice for work that is now running.
                let notified = state
                    .pending_initial_prompts
                    .remove(session_id)
                    .is_some_and(|(_, pending)| pending.notified);
                if notified {
                    notify_initial_prompt_delivered(state, session_id);
                }
            }
        }
        InjectionOutcome::NotStarted(error) => {
            tracing::debug!(session = %session_id, error, "agent command injection did not start");
            rollback_injection_claim(state, session_id, claim);
        }
        InjectionOutcome::Uncertain(error) => {
            tracing::warn!(session = %session_id, error, "agent command injection outcome uncertain; preserving busy state");
            match kind {
                ClaimedInjectionKind::OrchestratorNotice => {
                    mark_orchestrator_notice_uncertain(state, session_id, claim)
                }
                ClaimedInjectionKind::Message => mark_injection_uncertain(state, session_id, claim),
            }
        }
    }
    outcome
}

fn requeue_injection_front(
    state: &AppState,
    session_id: &str,
    injection: crate::state::PendingInjection,
) {
    state
        .pending_injections
        .entry(session_id.to_string())
        .or_default()
        .push_front(injection);
}

/// What actually became of a peer message handed to the terminal path.
///
/// The distinction is the whole point: `Queued` is NOT delivery. The composer was
/// busy, so the message only sits in `pending_injections` until the next BUSY→IDLE
/// transition — and a teardown before that flush drops the queue (see the tombstone
/// cleanup). Collapsing this into "the session still exists" is what let a caller
/// mark a never-typed message `TerminalDispatched`, which the waiter filter then
/// hides, stranding it in the inbox with nothing left to surface it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PtyDelivery {
    /// Written into the composer and submitted, or written ambiguously enough that
    /// a retry would risk duplicating it. Either way the terminal owns it.
    Typed,
    /// Parked in `pending_injections`; nothing reached the terminal yet.
    Queued,
    /// Not an agent, or the session is gone — the terminal path cannot take it.
    Unavailable,
}

/// Type a server-authored notice into a recipient's terminal, waking it. Injects
/// immediately when the recipient is an idle agent; otherwise queues it to flush
/// on the recipient's next BUSY→IDLE transition. No-op for non-agent sessions.
/// The caller has already buffered the authoritative copy in the inbox.
///
/// `framed` is a notice, never a peer payload: the payload-free `PEER_MAIL_WAKE`
/// or a child lifecycle summary. Peer `send` content stays in the inbox — see
/// `PendingInjection`.
pub(crate) fn deliver_notice_to_pty(
    state: &AppState,
    session_id: &str,
    framed: &str,
) -> PtyDelivery {
    // Never queue for a non-agent — shells and dead sessions have no wake path.
    if !session_is_agent(state, session_id) {
        return PtyDelivery::Unavailable;
    }
    if let Some(claim) = claim_idle_for_injection(state, session_id) {
        if matches!(
            run_claimed_injection(
                state,
                session_id,
                framed,
                claim,
                ClaimedInjectionKind::Message
            ),
            InjectionOutcome::NotStarted(_)
        ) {
            requeue_injection_front(
                state,
                session_id,
                crate::state::PendingInjection::notice(framed),
            );
            return PtyDelivery::Queued;
        }
        // Submitted, or Uncertain — an ambiguous write must not be retried, so the
        // terminal keeps ownership either way.
        PtyDelivery::Typed
    } else {
        // One parked mail wake covers the whole inbox: the recipient answers it
        // by reading every message. Pushing one per sender would type the same
        // pointer N times and, worse, keep the queue non-empty for N idle
        // windows — the state that blocks `submit`.
        let already_parked = framed == PEER_MAIL_WAKE
            && state
                .pending_injections
                .get(session_id)
                .is_some_and(|queue| queue.iter().any(|entry| entry.text() == PEER_MAIL_WAKE));
        if !already_parked {
            state
                .pending_injections
                .entry(session_id.to_string())
                .or_default()
                .push_back(crate::state::PendingInjection::notice(framed));
        }
        // CONC-A (story 101-20e3): the should_inject_now read above and this push are
        // not atomic vs a concurrent BUSY→IDLE flush. If the silence timer transitions
        // the session to idle and drains the (still-empty) queue in the window between
        // them, our message would sit queued until the NEXT idle cycle — exactly the
        // auto-wake this feature exists to deliver. Re-flush after enqueuing: if the
        // session went idle during the window, flush_pending_injections (self-guarded
        // by should_inject_now) delivers it ourselves. A double flush is harmless — it
        // drains under a get_mut write lock, so the racing flush that loses just finds
        // an empty queue.
        // Blocking on purpose: the emptiness check below IS the return value.
        flush_pending_injections_blocking(state, session_id);
        // That flush drains the whole queue under a write lock, so an empty queue
        // means everything — ours included — reached the composer. A non-empty
        // queue may still hold this message, and reporting Queued in the ambiguous
        // case is the safe direction: the worst outcome is that teardown later
        // hands a message the waiter can still see, instead of losing it.
        if state
            .pending_injections
            .get(session_id)
            .is_some_and(|queue| !queue.is_empty())
        {
            PtyDelivery::Queued
        } else {
            PtyDelivery::Typed
        }
    }
}

/// Settle wake ownership from what the terminal path actually did.
///
/// `Queued` deliberately does nothing, and that is the fix: the message stays
/// `TerminalPending`, which is the truthful state — the terminal owns it and will
/// type it on the next idle transition, but nothing has been typed yet. Marking it
/// `TerminalDispatched` here (as every call site used to, because the old boolean
/// only meant "the session exists") claimed a delivery that had not happened.
pub(crate) fn settle_terminal_delivery(
    state: &AppState,
    tuic_session: &str,
    message_id: &str,
    outcome: PtyDelivery,
) {
    match outcome {
        PtyDelivery::Typed => state.mark_terminal_delivery_dispatched(tuic_session, message_id),
        PtyDelivery::Queued => {}
        PtyDelivery::Unavailable => state.release_terminal_delivery(tuic_session, message_id),
    }
}

/// Deliver only while the recipient still has a managed PTY and agent state.
/// Reports what the terminal path actually did, so the caller can keep wake
/// ownership only for a message that truly reached the composer. `Unavailable`
/// means teardown won the race and the authoritative inbox copy must stay
/// available to `agent wait`.
pub(crate) fn deliver_notice_to_managed_pty(
    state: &AppState,
    session_id: &str,
    framed: &str,
) -> PtyDelivery {
    let available = state.session_maps.sessions.contains_key(session_id)
        && state
            .session_maps
            .session_states
            .get(session_id)
            .is_some_and(|session| session.agent_type.is_some());
    if !available {
        return PtyDelivery::Unavailable;
    }
    let outcome = deliver_notice_to_pty(state, session_id, framed);
    // Teardown can still win between the check above and the write.
    if state.session_maps.sessions.contains_key(session_id) {
        outcome
    } else {
        PtyDelivery::Unavailable
    }
}

/// `flush_pending_injections_blocking` off the calling thread.
///
/// This is the entry point for every caller that runs on a tokio worker — the
/// session-state accumulator, the silence timer, desktop input bookkeeping.
/// The flush itself sleeps `INJECT_ENTER_GAP` under the session writer mutex,
/// so waiting for it here would park a worker for 50ms per queued message.
///
/// Callers that must observe the result before returning — `deliver_notice_to_pty`
/// reads the queue to tell `Typed` from `Queued`, and the OSC handler already
/// runs on the session's own reader thread — call the blocking form directly.
pub(crate) fn flush_pending_injections(state: &Arc<AppState>, session_id: &str) {
    let state = Arc::clone(state);
    let session_id = session_id.to_string();
    spawn_injection_job(move || flush_pending_injections_blocking(&state, &session_id));
}

/// Drain and inject any messages queued for a session that can receive them now.
/// Self-guarded by `should_inject_now`: skips (leaves queued) unless the session
/// is an idle agent not blocked on a confident question, so a peer message never
/// answers a user-facing approval prompt and never corrupts a busy TUI. Called
/// from the BUSY→IDLE transition, the post-enqueue race re-check, and the
/// unblock path when a confident question clears while the agent is idle.
pub(crate) fn flush_pending_injections_blocking(state: &AppState, session_id: &str) {
    if state
        .pending_injections
        .get(session_id)
        .is_none_or(|pending| pending.is_empty())
    {
        return;
    }
    let claim = match claim_idle_for_injection(state, session_id) {
        Some(claim) => claim,
        None => return,
    };
    let pending = match state.pending_injections.get_mut(session_id) {
        Some(mut q) => q.pop_front(),
        None => return,
    };
    if let Some(injection) = pending
        && matches!(
            run_claimed_injection(
                state,
                session_id,
                injection.text(),
                claim,
                ClaimedInjectionKind::Message
            ),
            InjectionOutcome::NotStarted(_)
        )
    {
        requeue_injection_front(state, session_id, injection);
    }
}

/// Everything still parked for a session, of any kind.
///
/// Counting only `UserCommand` here is what made a stuck queue undiagnosable: a
/// single server entry blocked `submit` with `queued_commands_pending` while the
/// Compose badge read 0 and the listing was empty, so there was nothing to see
/// and nothing to delete. Every parked entry is now counted, listed and
/// removable; `kind` is what tells them apart.
pub(crate) fn queued_command_count(state: &AppState, session_id: &str) -> usize {
    state
        .pending_injections
        .get(session_id)
        .map(|queue| queue.len())
        .unwrap_or(0)
}

/// One parked entry, as the Compose panel lists it.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct QueuedCommand {
    pub id: u64,
    pub text: String,
    /// `user_command`, `notice` or `initial_prompt` — see `PendingInjection`.
    pub kind: &'static str,
}

/// Everything still parked, in delivery order.
pub(crate) fn list_queued_commands(state: &AppState, session_id: &str) -> Vec<QueuedCommand> {
    state
        .pending_injections
        .get(session_id)
        .map(|queue| {
            queue
                .iter()
                .map(|entry| QueuedCommand {
                    id: entry.id(),
                    text: entry.text().to_string(),
                    kind: entry.kind(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Drop a single parked entry by id. Returns false when the id is unknown —
/// the entry may have been typed already, which is not an error for the caller.
pub(crate) fn remove_queued_command(state: &AppState, session_id: &str, id: u64) -> bool {
    state
        .pending_injections
        .get_mut(session_id)
        .map(|mut queue| {
            let before = queue.len();
            queue.retain(|entry| entry.id() != id);
            before != queue.len()
        })
        .unwrap_or(false)
}

/// What the idle gate did with a user-composed command.
#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct EnqueuedCommand {
    /// The agent was already idle, so the text was typed and submitted at once.
    pub typed: bool,
    /// Commands still waiting, this one included when `typed` is false.
    pub queued: usize,
}

/// Route a user-composed command through the same idle gate peer messages use:
/// typed immediately when the agent is idle, otherwise parked until the next
/// BUSY→IDLE transition. This is the whole point of the Compose panel's enqueue
/// action — the user wants the text delivered *without* steering a running turn.
///
/// The command is always appended before the flush, never handed straight to
/// `deliver_notice_to_pty`: injecting ahead of any accepted peer message or
/// Compose command would reorder delivery. `flush_pending_injections_blocking` pops one
/// typed entry and leaves the session BUSY, so the shared queue drains one item
/// per idle transition and stays FIFO across both producers.
///
/// Agent sessions only (see `session_is_agent`).
pub(crate) fn enqueue_user_command(
    state: &AppState,
    session_id: &str,
    text: &str,
) -> Result<EnqueuedCommand, String> {
    if text.trim().is_empty() {
        return Err("Command text is empty".to_string());
    }
    if !state.session_maps.sessions.contains_key(session_id) {
        return Err("Session not found".to_string());
    }
    if !session_is_agent(state, session_id) {
        return Err("Session is not running an agent".to_string());
    }
    state
        .pending_injections
        .entry(session_id.to_string())
        .or_default()
        .push_back(crate::state::PendingInjection::user_command(text));
    // Blocking on purpose: `typed` below is read from the post-flush queue.
    flush_pending_injections_blocking(state, session_id);
    let queued = queued_command_count(state, session_id);
    // An empty queue after the flush means our command was the only one waiting
    // and reached the composer; any remaining entry means it is still parked.
    Ok(EnqueuedCommand {
        typed: queued == 0,
        queued,
    })
}

/// Drop everything still waiting for this session. Returns the count removed.
///
/// This is the drain a stuck queue needs: leaving server entries behind meant
/// "Clear" emptied the visible list while the composer stayed blocked on what
/// was left. Nothing here is load-bearing — a dropped mail wake costs at most
/// one `agent action=inbox` (the mail itself never left the inbox), and a
/// dropped initial prompt is still on record in `pending_initial_prompts`.
pub(crate) fn clear_queued_commands(state: &AppState, session_id: &str) -> usize {
    state
        .pending_injections
        .get_mut(session_id)
        .map(|mut queue| {
            let before = queue.len();
            queue.clear();
            before
        })
        .unwrap_or(0)
}

/// Keeps `output_buffers`, `vt_log_buffers`, `last_output_ms`, and `exit_codes`
/// alive so MCP consumers can read final output + exit status post-mortem.
/// Tombstones are reaped by `spawn_tombstone_sweeper` after `TOMBSTONE_TTL_MS`.
/// Drive any task tracking `session_id` to its terminal state. This is what makes
/// a task handle worth polling: the outcome is recorded even if no client was
/// waiting when the agent finished.
///
/// A missing exit code is read as success — the session is gone and we have no
/// evidence of failure, so an orchestrator should collect a result rather than
/// see a phantom error.
fn finish_session_tasks(state: &AppState, session_id: &str, exit_code: Option<i32>) {
    let failed = exit_code.is_some_and(|code| code != 0);
    for task_id in state.tasks.live_ids_for_session(session_id) {
        let (status, update) = if failed {
            (
                crate::tasks::TaskStatus::Failed,
                crate::tasks::TaskUpdate {
                    error: Some(format!(
                        "agent session exited with code {}",
                        exit_code.unwrap_or_default()
                    )),
                    ..Default::default()
                },
            )
        } else {
            (
                crate::tasks::TaskStatus::Completed,
                crate::tasks::TaskUpdate {
                    result: Some(serde_json::json!({
                        "session_id": session_id,
                        "exit_code": exit_code,
                    })),
                    ..Default::default()
                },
            )
        };
        if let Err(e) = state.tasks.set_status(&task_id, status, update) {
            // Terminal already (a cancel that raced the exit) is expected, not an
            // error worth a warning — `live_ids_for_session` just read it as live.
            tracing::debug!(source = "tasks", task_id = %task_id, error = %e, "Task not finished on session exit");
        }
    }
}

pub(crate) fn mark_session_exited(session_id: &str, state: &Arc<AppState>) {
    // Capture exit code before dropping the session entry.
    // portable_pty::ExitStatus carries both exit_code() and signal().
    // Signal-killed processes get 128+signum (shell convention) so the
    // caller can distinguish SIGKILL (137) from normal exit(1).
    if let Some(entry) = state.session_maps.sessions.get(session_id)
        && let Ok(Some(status)) = entry.value().lock()._child.try_wait()
    {
        let code = if let Some(sig) = status.signal() {
            let signum = parse_signal_number(sig);
            128 + signum
        } else {
            status.exit_code() as i32
        };
        state
            .session_maps
            .exit_codes
            .insert(session_id.to_string(), code);
    }
    if state.session_maps.sessions.remove(session_id).is_some() {
        state
            .metrics
            .active_sessions
            .fetch_sub(1, Ordering::Relaxed);
    }

    // Notify orchestrator (if any) that this agent has exited.
    let exit_code = state
        .session_maps
        .exit_codes
        .get(session_id)
        .map(|e| *e.value());
    finish_session_tasks(state, session_id, exit_code);
    push_state_change_to_parent(
        state,
        session_id,
        serde_json::json!({
            "type": "state_change",
            "state": "exited",
            "session_id": session_id,
            "exit_code": exit_code,
        }),
    );

    // SIMP-1: drain HTML tabs registered by this session and emit close.
    // Same helper used by `session(close)` and `session(kill)` so all three
    // exit paths drain `session_html_tabs` identically (no orphan tabs).
    crate::mcp_http::mcp_transport::emit_close_html_tabs(state, session_id);

    tombstone_transient_cleanup(session_id, state);
}

/// Time a tombstoned session's buffers remain readable after process exit.
pub(crate) const TOMBSTONE_TTL_MS: u64 = 5 * 60 * 1000; // 5 minutes

/// Session ids whose tombstone has aged past the TTL.
///
/// Discovery walks `last_output_ms`, not `output_buffers`: an explicit close runs
/// the full cleanup, and the reader thread can afterwards reach EOF and re-stamp
/// the timestamp through the tombstone path. That leaves a lone entry with no
/// buffers — invisible to a buffer-driven walk, and so never reaped at all. The
/// stamp is the one thing every tombstone has.
fn aged_out_tombstones(state: &AppState, now_ms: u64) -> Vec<String> {
    // A tombstone is: a stamp present, session entry absent, aged past TTL.
    state
        .session_maps
        .last_output_ms
        .iter()
        .filter_map(|entry| {
            let id = entry.key();
            if state.session_maps.sessions.contains_key(id) {
                return None;
            }
            let last_ms = entry.value().load(Ordering::Relaxed);
            if last_ms == 0 || now_ms.saturating_sub(last_ms) < TOMBSTONE_TTL_MS {
                return None;
            }
            Some(id.clone())
        })
        .collect()
}

/// Reap each candidate's post-mortem state.
///
/// Liveness is re-checked here, not only at selection: the HTTP spawn path accepts
/// a caller-supplied id, so an aged id can be reclaimed by a live session between
/// the two. Reaping it then would delete that session's buffers and alias.
///
/// DEFERRED (2026-08-18) — the re-check narrows the window, it does not close it:
/// a reclaim landing between this load and the removal still loses. Closing it
/// needs a per-id generation stamped at insert and compared at removal, which is
/// a `sessions` API change; not worth it while ids are random UUIDs in practice.
fn reap_tombstones(state: &AppState, candidates: &[String]) {
    for id in candidates {
        if state.session_maps.sessions.contains_key(id) {
            continue;
        }
        remove_post_mortem_session_state(id, state);
        tracing::debug!(source = "pty", session_id = %id, "Tombstone reaped");
    }
}

/// Background sweeper that reaps tombstoned session buffers once they age out.
/// Started once at boot from the HTTP server runtime.
pub(crate) fn spawn_tombstone_sweeper(state: Arc<AppState>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(30));
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            reap_tombstones(&state, &aged_out_tombstones(&state, now_ms));
        }
    });
}

/// Detect anomalous ANSI sequences that may cause scroll-jump-to-top or viewport resets.
/// Returns a list of human-readable labels for each detected sequence.
/// These are logged as warnings for diagnostic purposes — data is never modified.
fn detect_anomalous_sequences(data: &str) -> Vec<&'static str> {
    let bytes = data.as_bytes();
    let len = bytes.len();
    let mut found = Vec::new();
    let mut i = 0;

    while i < len {
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            i += 2; // skip ESC[

            // Check for ESC[? private mode sequences (alt screen)
            if i < len && bytes[i] == b'?' {
                i += 1;
                let num_start = i;
                while i < len && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i < len {
                    let num_str = std::str::from_utf8(&bytes[num_start..i]).unwrap_or("");
                    match (num_str, bytes[i]) {
                        ("1049", b'h') => found.push("ESC[?1049h (Alt Screen Enter)"),
                        ("1049", b'l') => found.push("ESC[?1049l (Alt Screen Exit)"),
                        _ => {}
                    }
                    i += 1;
                }
                // No continue — let the outer while loop re-evaluate i < len
            } else {
                // Parse numeric params: n or n;m
                let num_start = i;
                while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b';') {
                    i += 1;
                }
                if i < len {
                    let params = std::str::from_utf8(&bytes[num_start..i]).unwrap_or("");
                    match bytes[i] {
                        b'J' => match params {
                            "2" => found.push("ESC[2J (Clear Screen)"),
                            "3" => found.push("ESC[3J (Clear Scrollback)"),
                            _ => {}
                        },
                        b'H' => {
                            // ESC[H or ESC[1;1H = Cursor Home
                            if params.is_empty() {
                                found.push("ESC[H (Cursor Home)");
                            } else if params == "1;1" {
                                found.push("ESC[1;1H (Cursor Home)");
                            }
                            // Other ESC[n;mH = regular cursor position, not anomalous
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    found
}

/// Extract the largest ESC[nA (cursor-up) value from `data`.
/// Ink emits ESC[nA where n equals the previous render height before redrawing.
/// A decrease in n between consecutive redraws signals content shrinkage.
fn extract_largest_cursor_up(data: &str) -> Option<u16> {
    let bytes = data.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut max_n: Option<u16> = None;

    while i < len {
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            i += 2;
            let num_start = i;
            while i < len && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < len
                && bytes[i] == b'A'
                && i > num_start
                && let Ok(n) = std::str::from_utf8(&bytes[num_start..i])
                    .unwrap_or("")
                    .parse::<u16>()
            {
                max_n = Some(max_n.map_or(n, |prev: u16| prev.max(n)));
            }
            if i < len {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    max_n
}

/// Inject ESC[2J (clear screen) before the first ESC[H or ESC[1;1H (cursor home) in `data`.
///
/// Ink-based TUIs render differentially: they position the cursor at home and overwrite
/// changed cells but never send ESC[K (erase to end of line). When output shrinks between
/// redraws, old characters — especially box-drawing separators — persist as ghost artifacts.
///
/// Injecting a single ESC[2J before the cursor-home ensures the screen is blank before
/// the redraw starts. Because xterm.js processes the entire write() atomically (clear +
/// cursor home + new content happen before the next paint), no intermediate blank frame
/// is ever rendered to the user.
///
/// Only injects once per call (before the first cursor-home) to avoid unnecessary clears
/// for chunks that contain multiple ESC[H sequences (common in Ink's rapid redraws).
fn inject_clear_before_cursor_home(data: &str) -> String {
    let bytes = data.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            let seq_start = i;
            i += 2; // skip ESC[
            // Parse optional numeric parameters
            let num_start = i;
            while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b';') {
                i += 1;
            }
            if i < len && bytes[i] == b'H' {
                let params = std::str::from_utf8(&bytes[num_start..i]).unwrap_or("");
                // ESC[H (no params) or ESC[1;1H — both mean cursor home
                if params.is_empty() || params == "1;1" {
                    // Inject ESC[2J before this cursor-home sequence
                    let mut result = String::with_capacity(len + 4);
                    result.push_str(&data[..seq_start]);
                    result.push_str("\x1b[2J");
                    result.push_str(&data[seq_start..]);
                    return result;
                }
            }
            if i < len {
                i += 1; // skip command byte
            }
        } else {
            i += 1;
        }
    }

    // No cursor-home found — return as-is
    data.to_string()
}

/// Inject ESC[2J before the first ESC[nA (cursor-up, n > 0) in `data`.
///
/// Fallback for `inject_clear_before_cursor_home`: Ink re-renders reposition via
/// cursor-up (ESC[nA), not cursor-home (ESC[H). Without this path the
/// `alt_buffer_needs_clear` flag is set but never consumed, and ghost rows
/// from previous renders accumulate from the bottom upward.
fn inject_clear_before_cursor_up(data: &str) -> String {
    let bytes = data.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            let seq_start = i;
            i += 2; // skip ESC[
            let num_start = i;
            while i < len && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < len && bytes[i] == b'A' && i > num_start {
                // ESC[nA with n > 0 — inject ESC[2J before it
                let mut result = String::with_capacity(len + 4);
                result.push_str(&data[..seq_start]);
                result.push_str("\x1b[2J");
                result.push_str(&data[seq_start..]);
                return result;
            }
            if i < len {
                i += 1; // skip command byte
            }
        } else {
            i += 1;
        }
    }

    data.to_string()
}

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
/// Called from every interactive input entry point (desktop `write_pty` +
/// HTTP/PWA `write_to_session`).
pub(crate) fn stamp_input_ms(state: &AppState, session_id: &str) {
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
    // Reader sets dirty flag; ticker serializes+sends at fixed interval.
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
        // This is also the deadline a HIDDEN tab races on purpose: it acks on a
        // trailing timer (HIDDEN_ACK_INTERVAL_MS in canvasTerminalUtils.ts), so
        // the two constants are coupled and neither may be re-tuned alone. The
        // difference between them is the frontend's drift budget — at 400 vs 500
        // it was 100 ms and lost constantly, logging the warning below for tabs
        // that were merely in the background.
        const MAX_IN_FLIGHT_MS: u64 = 500;
        // After this many consecutive force-resets, back off for STUCK_PAUSE_MS
        // to let the JS event loop drain the Tauri channel backlog before
        // sending more frames. Kept short (1s, chunked) so a transient JS stall
        // — e.g. a repo-changed git/IPC burst that blocks the WebView thread for
        // ~1-2s — doesn't freeze an otherwise-healthy terminal for the full
        // pause. The loop re-applies the back-off if the frontend is still
        // behind, so persistent saturation still gets cumulative backpressure.
        const MAX_STUCK_BEFORE_PAUSE: u32 = 3;
        const STUCK_PAUSE_MS: u64 = 1_000;
        // Send-rate floors live in grid_send_min_interval_ms() (unit-tested).
        // How long after a keystroke the typing-throttle stays armed.
        const INPUT_THROTTLE_WINDOW_MS: u64 = 150;
        const LOAD_SATURATION_RATIO: f64 = 1.0; // 1-min load >= cores
        const LOAD_SAMPLE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
        let mut stuck_since: Option<std::time::Instant> = None;
        let mut stuck_count: u32 = 0;
        let mut dirty_run: u32 = 0;
        let mut last_sent: Option<std::time::Instant> = None;
        let mut last_load_check: Option<std::time::Instant> = None;
        let mut system_saturated = false;
        while ticker_running.load(Ordering::Relaxed) {
            std::thread::sleep(TICK);
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
            // DEFERRED (2026-08-20) — parking the THREAD itself, which is what
            // F28 asked for. What is left after the skip above is timer churn, not
            // work: ~62.5 wakeups/s per session, each an atomic load and two
            // checks, and macOS coalesces them. Parking needs a condvar the PTY
            // writer signals, plus a `wait_timeout` for the DEC 2026 sync flush
            // above, which must keep running headlessly — and a missed notify
            // shows up as a terminal that silently stops painting. Small win,
            // worst failure mode of the group.
            if !grid_has_subscriber(&ticker_state, &ticker_sid) {
                if let Some(target) = take_pending_scroll(&ticker_state, &ticker_sid)
                    && let Some(vt) = ticker_state.grid.vt_log_buffers.get(&ticker_sid)
                {
                    vt.lock().grid_scroll_to_offset(target);
                }
                dirty_run = 0;
                continue;
            }
            dirty_run = dirty_run.saturating_add(1);
            // Clone the Arc out: the guard below would otherwise hold a DashMap
            // shard read lock across the stuck back-off sleep, blocking every
            // writer on that shard for up to a second.
            let gate = ticker_state
                .grid
                .gates
                .get(&ticker_sid)
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
            if gate.as_ref().is_some_and(|g| !g.is_open()) {
                let now = std::time::Instant::now();
                let since = stuck_since.get_or_insert(now);
                let elapsed = now.duration_since(*since).as_millis() as u64;
                if elapsed > MAX_IN_FLIGHT_MS {
                    stuck_count += 1;
                    tracing::warn!(
                        session_id = %ticker_sid,
                        elapsed_ms = elapsed,
                        stuck_count,
                        watchers,
                        outstanding = gate.as_ref().map_or(0, |g| g.outstanding()),
                        "grid frame gate stuck, abandoning the outstanding frame"
                    );
                    if let Some(g) = gate.as_ref() {
                        g.abandon();
                    }
                    stuck_since = None;
                    if stuck_count >= MAX_STUCK_BEFORE_PAUSE {
                        stuck_count = 0;
                        // Back off to let JS drain the channel backlog before retrying.
                        // Sleep in short chunks so (a) a recovered frontend resumes
                        // within ~one chunk rather than the full pause, and (b) session
                        // close isn't delayed up to the full pause on shutdown.
                        //
                        // Skipped entirely when a browser is watching: this pause is
                        // the desktop's recovery time, and spending it on the thread
                        // that is the only frame source for the other transport
                        // freezes a client that never fell behind.
                        let mut waited = 0u64;
                        while !watchers
                            && waited < STUCK_PAUSE_MS
                            && ticker_running.load(Ordering::Relaxed)
                        {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            waited += 100;
                        }
                    }
                }
                if !watchers {
                    ticker_dirty.store(true, Ordering::Relaxed);
                    continue;
                }
            } else {
                stuck_since = None;
                stuck_count = 0;
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
            if min_interval > 0
                && let Some(last) = last_sent
                && (now.duration_since(last).as_millis() as u64) < min_interval
            {
                ticker_dirty.store(true, Ordering::Relaxed); // keep pending for a later tick
                continue;
            }
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
                    g.grid_scroll_to_offset(target);
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
                        state.metrics.bytes_emitted.fetch_add(n, Ordering::Relaxed);
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

                        if chunk_processed
                            && let Some(xterm_data) = processor.transform_xterm(&data)
                        {
                            let clamped_data = xterm_data;

                            let agent_active = state
                                .session_maps
                                .session_states
                                .get(&session_id)
                                .map(|s| s.agent_type.is_some())
                                .unwrap_or(false);
                            // Also diagnostic-only: the ESC scan plus
                            // `detect_anomalous_sequences` ran on every shell-session
                            // chunk purely to emit a warning nothing acts on.
                            if crate::cpu_watchdog::diagnostic_mode()
                                && !processor.in_alt_buffer
                                && !agent_active
                                && clamped_data.as_bytes().contains(&0x1b)
                            {
                                let anomalies = detect_anomalous_sequences(&clamped_data);
                                for label in &anomalies {
                                    tracing::warn!(source = "terminal", session_id = %session_id, "Anomalous ANSI sequence: {label}");
                                }
                            }

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
                                &clamped_data,
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

                        frame_dirty.store(true, Ordering::Relaxed);
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

/// Spawn a headless PTY session for agent orchestration (no Tauri command context).
/// Extracts AppHandle from `state.app_handle` and creates a minimal session.
pub(crate) async fn spawn_session_for_agent(
    state: &Arc<AppState>,
    cwd: Option<String>,
    display_name: Option<String>,
) -> Result<String, String> {
    let session_id = Uuid::new_v4().to_string();
    let rows: u16 = 24;
    let cols: u16 = 80;

    let shell = resolve_shell(None);

    let spawn_cwd = cwd.clone();
    let spawn_shell = shell.clone();
    let data_dir = state.data_dir.clone();
    let state_for_env = state.clone();
    let session_id_for_env = session_id.clone();
    let (pair, child) = spawn_pty_pair_with_retry_async(
        PtySize {
            rows,
            cols,
            pixel_width: cols.saturating_mul(crate::terminal_grid::DEFAULT_CELL_WIDTH_PX),
            pixel_height: rows.saturating_mul(crate::terminal_grid::DEFAULT_CELL_HEIGHT_PX),
        },
        move || {
            let mut cmd = build_shell_command(&spawn_shell);

            if let Some(ref dir) = spawn_cwd {
                let expanded = crate::cli::expand_tilde(dir);
                cmd.cwd(expanded);
            }

            crate::shell_integration::inject(&data_dir, &spawn_shell, &mut cmd);
            // No caller-supplied identity on this path, so the PTY key is the
            // identity — see bind_pty_identity.
            bind_pty_identity(&state_for_env, &mut cmd, &session_id_for_env, None);
            inject_worktree_env(&mut cmd, spawn_cwd.as_deref());
            cmd
        },
    )
    .await?;
    lower_pty_child_priority(child.process_id());

    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("Failed to get PTY writer: {e}"))?;

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("Failed to get PTY reader: {e}"))?;

    // Captured before `cwd`/`display_name` move into the `PtySession` struct
    // below — `emit_session_created` at the end of this function needs the
    // same values.
    let created_cwd = cwd.clone();
    let created_display_name = display_name.clone();

    let paused = Arc::new(AtomicBool::new(false));
    state.session_maps.sessions.insert(
        session_id.clone(),
        Mutex::new(PtySession {
            writer: Arc::new(Mutex::new(writer)),
            master: pair.master,
            _child: child,
            paused: paused.clone(),
            worktree: None,
            cwd,
            display_name: display_name.clone(),
            display_name_is_custom: false,
            is_remote: true,
            shell: shell.clone(),
        }),
    );
    state.assign_term_alias(&session_id, None);
    state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
    state
        .metrics
        .active_sessions
        .fetch_add(1, Ordering::Relaxed);

    state.session_maps.output_buffers.insert(
        session_id.clone(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );
    let vt_log = state.new_vt_log_buffer(rows, cols, VT_LOG_BUFFER_CAPACITY);
    state
        .grid
        .vt_log_buffers
        .insert(session_id.clone(), Mutex::new(vt_log));
    let grid_watch_tx = crate::grid_gate::new_grid_watch();
    state.grid.watch.insert(session_id.clone(), grid_watch_tx);
    state
        .session_maps
        .last_output_ms
        .insert(session_id.clone(), AtomicU64::new(0));
    state
        .session_maps
        .terminal_rows
        .insert(session_id.clone(), std::sync::atomic::AtomicU16::new(rows));
    state
        .session_maps
        .session_states
        .insert(session_id.clone(), crate::state::SessionState::default());

    // Announce BEFORE the reader thread starts, same convention as
    // `create_pty`/`create_pty_with_worktree`. Previously the desktop half of
    // this announcement dropped `cwd` entirely (bus half had it via a
    // second `session_maps` lookup) — one call now carries the same values
    // on both transports.
    emit_session_created(
        &state,
        &session_id,
        created_cwd,
        None, // no agent_type input on this orchestrated-spawn path
        created_display_name,
    );

    spawn_reader_thread(reader, paused, session_id.clone(), state.clone(), None);

    Ok(session_id)
}

#[cfg(feature = "desktop")]
async fn write_pty_parts_off_thread(
    state: Arc<AppState>,
    session_id: String,
    parts: Vec<String>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || write_pty_parts_blocking(&state, &session_id, &parts))
        .await
        .map_err(|e| format!("Task join error: {e}"))?
}

#[cfg(feature = "desktop")]
fn write_pty_parts_blocking(
    state: &Arc<AppState>,
    session_id: &str,
    parts: &[String],
) -> Result<(), String> {
    // Keystroke delivery to the PTY: run on the high QoS band so the write (and
    // thus the echo round-trip) isn't starved by a saturating build. Scoped, not
    // a bare raise: this thread comes from the shared blocking pool and goes back
    // into it, so an unrestored bump would promote the pool the terminal is
    // trying to out-schedule.
    let _qos = interactive_io_boost();
    // Restore cursor if hidden — Ink-based agents send DECTCEM hide for
    // spinners but may not send CNORM when returning to the prompt.
    // Best-effort try_lock: this is cosmetic (touches the local grid, not the PTY)
    // and MUST NOT block input delivery. Under an output flood the ticker
    // (serialize_dirty_rows) and reader thrash this same vt lock; a blocking lock
    // here would starve input. If contended, skip — the next frame restores the
    // cursor anyway.
    if let Some(vt) = state.grid.vt_log_buffers.get(session_id)
        && let Some(mut vt) = vt.try_lock()
        && !vt.is_cursor_visible()
    {
        vt.process(b"\x1b[?25h");
    }

    let data_len: usize = parts.iter().map(String::len).sum();
    tracing::trace!(session_id = %session_id, data_len, part_count = parts.len(), "write_pty");
    for data in parts {
        if data.contains("\x1b[?1049")
            || data.contains("\x1b[?1047")
            || data.contains("\x1b[?47l")
            || data.contains("\x1b[?25h")
        {
            tracing::error!(source = "terminal", session_id = %session_id,
                "write_pty received DEC private mode sequences! data({} bytes)={:?}",
                data.len(), data.as_bytes().iter().take(200).collect::<Vec<_>>());
        }
    }

    let byte_parts: Vec<&[u8]> = parts.iter().map(|part| part.as_bytes()).collect();
    let t0 = std::time::Instant::now();
    state.write_pty_parts(session_id, &byte_parts)?;
    let total_ms = t0.elapsed().as_millis();
    if total_ms > 100 {
        tracing::warn!(session_id = %session_id, total_ms = %total_ms,
            data_len, part_count = parts.len(), "write_pty SLOW — lock or write blocked");
    }

    for data in parts {
        if crate::pty_capture::is_enabled() {
            let capture_geometry = state.grid.vt_log_buffers.get(session_id).map(|vt| {
                let vt = vt.lock();
                (vt.grid_screen_lines() as u16, vt.grid_columns() as u16)
            });
            crate::pty_capture::record_input_with_geometry(
                session_id,
                data.as_bytes(),
                capture_geometry,
            );
        }
        apply_desktop_input_bookkeeping(state, session_id, data);
    }

    Ok(())
}

#[cfg(feature = "desktop")]
fn apply_desktop_input_bookkeeping(state: &Arc<AppState>, session_id: &str, data: &str) {
    // Stamp last-input time so the grid ticker can throttle frame sends while
    // the user types under CPU saturation (keeps the WebView thread free for
    // keystroke dispatch + echo).
    stamp_input_ms(state, session_id);
    crate::state::resolve_choice_prompt_input(state, session_id, data);

    // Feed input through the line buffer to reconstruct user-typed lines.
    // Release both the inner mutex and DashMap entry guard before callbacks
    // below. In particular, flush_pending_injections -> should_inject_now
    // reads input_buffers again; retaining input_entry there self-deadlocks
    // this shard and can park the entire IPC Tokio runtime under load.
    let (actions, buffer_empty, buffer_is_slash) = {
        let input_entry = state
            .session_maps
            .input_buffers
            .entry(session_id.to_string())
            .or_insert_with(|| parking_lot::Mutex::new(InputLineBuffer::new()));
        let mut buf = input_entry.lock();
        let actions = buf.feed(data);
        // Two bits, not the line: `content()` here collected the whole typed
        // line into a fresh String on every keystroke, to be dropped below.
        (actions, buf.is_empty(), buf.starts_with('/'))
    };
    let mut line_submitted = false;
    for action in actions {
        match action {
            InputAction::Line(content) => {
                line_submitted = true;
                // Keystroke-reconstructed: no grid context, so no prompt row
                // (line = -1). The OSC 7770 busy path supplies an absolute
                // scrollbar marker when available.
                record_submitted_line(state, session_id, content, -1);
            }
            InputAction::Interrupt => {
                line_submitted = true;
                if let Some(ss) = state.session_maps.silence_states.get(session_id) {
                    ss.lock().note_interrupt_requested();
                }
            }
        }
    }
    // Codex advertises Escape as its normal interrupt key. A bare Escape is
    // only intent evidence; it never flips idle until the agent redraws an
    // interrupted/ready prompt. CSI-prefixed navigation keys are excluded.
    if data == "\x1b"
        && let Some(ss) = state.session_maps.silence_states.get(session_id)
    {
        ss.lock().note_interrupt_requested();
    }

    // On any line submit (Enter or Ctrl+C) reset the tool-error dedup
    // memory: the user is explicitly engaging again, so a recurrence of
    // the same failure in a later turn must be allowed to notify.
    // Mirrors `OutputParser`'s reset of `last_api_error_match` on UserInput.
    if line_submitted && let Some(ss) = state.session_maps.silence_states.get(session_id) {
        let mut sl = ss.lock();
        sl.reset_tool_error_memory();
        sl.reset_suggest_memory();
        sl.reset_declared_background_work();
    }

    // Track slash command mode: true when the input buffer starts with /
    // Fallback: when ESC is sent before "/" (TerminalKeybar's handleSlash),
    // the InputLineBuffer consumes "/" as an unknown escape-sequence suffix
    // and never inserts it. Detect bare "/" writes that the buffer missed.
    let in_slash = if line_submitted {
        false
    } else {
        buffer_is_slash || (buffer_empty && data == "/")
    };
    // Look up before inserting: `entry` needs an owned key, so the allocation is
    // paid only when the map entry does not already exist.
    match state.session_maps.slash_mode.get(session_id) {
        Some(flag) => flag.store(in_slash, std::sync::atomic::Ordering::Relaxed),
        None => {
            state
                .session_maps
                .slash_mode
                .entry(session_id.to_string())
                .or_insert_with(|| std::sync::atomic::AtomicBool::new(false))
                .store(in_slash, std::sync::atomic::Ordering::Relaxed);
        }
    }

    if buffer_empty {
        flush_pending_injections(state, session_id);
    }
}

/// Shared lookup for `last_prompts`, kept in one place so the IPC-exposed
/// getter (`get_last_prompt`, `pty/commands.rs`) and the internal consumer
/// (`ChunkProcessor::handle_tuic_state`'s `AgentBlock.prompt_text`) can't drift
/// if the lookup semantics ever change (trimming, a different word-count
/// threshold, etc.).
pub(crate) fn last_prompt_text(state: &AppState, session_id: &str) -> Option<String> {
    state
        .session_maps
        .last_prompts
        .get(session_id)
        .map(|v| v.clone())
}
/// Shared resize core for the Tauri command and the HTTP route (story 056-7545).
///
/// Order matters: the grid must adopt the new dimensions BEFORE the PTY ioctl
/// delivers SIGWINCH to the child. With the old PTY-first order the child could
/// repaint for the new width while the grid still wrapped at the old one (the
/// window grows with vt-lock contention under bursty output); wide lines then
/// autowrapped in the narrow grid, breaking Ink's cursor-up arithmetic and
/// stranding intermediate render rows in scrollback as duplicated blocks.
///
/// Same-dims calls are a no-op (returns None): they would otherwise deliver a
/// gratuitous SIGWINCH (full Ink repaint) per redundant caller (MCP/HTTP/multi
/// -client — the desktop frontend already guards, others don't).
///
/// Returns the post-resize full frame to flush, if the grid was resized.
/// Which thread last ran the reflow for a session.
///
/// Keyed by session, not process-wide: the suite runs tests in parallel, and a
/// single slot would report some other test's thread. Test-only — it is how a
/// test proves the reflow left the caller's thread without reaching into tokio.
#[cfg(test)]
static RESIZE_THREADS: std::sync::LazyLock<dashmap::DashMap<String, std::thread::ThreadId>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

#[cfg(test)]
pub(crate) fn resize_thread(session_id: &str) -> Option<std::thread::ThreadId> {
    RESIZE_THREADS.get(session_id).map(|t| *t)
}

/// [`resize_session_core`] on the blocking pool.
///
/// The reflow is the single most expensive thing a terminal does: it rewraps the
/// whole ring — up to 10,000 rows — and then serializes a full frame, all while
/// holding the VT mutex the PTY reader wants. Run inline in a `#[tauri::command]`
/// that is on macOS the main thread, a drag-resize froze the WebView for the
/// length of every reflow it triggered.
///
/// Shared by both transports, like [`vt_try_read`]: the HTTP route is `async` but
/// its await point is worthless if the body blocks a tokio worker for a whole
/// rewrap. Neither transport can quietly go back to blocking.
pub(crate) async fn resize_session_off_thread(
    state: &Arc<AppState>,
    session_id: String,
    rows: u16,
    cols: u16,
    cell_width_px: Option<u16>,
    cell_height_px: Option<u16>,
) -> Result<Option<crate::grid_gate::GridFrame>, String> {
    let state = Arc::clone(state);
    tokio::task::spawn_blocking(move || {
        resize_session_core(
            &state,
            &session_id,
            rows,
            cols,
            cell_width_px,
            cell_height_px,
        )
    })
    .await
    .map_err(|e| format!("resize failed: {e}"))?
}

pub(crate) fn resize_session_core(
    state: &AppState,
    session_id: &str,
    rows: u16,
    cols: u16,
    cell_width_px: Option<u16>,
    cell_height_px: Option<u16>,
) -> Result<Option<crate::grid_gate::GridFrame>, String> {
    if rows == 0 || cols == 0 {
        return Err("Invalid dimensions: rows and cols must be > 0".to_string());
    }
    #[cfg(test)]
    {
        RESIZE_THREADS.insert(session_id.to_string(), std::thread::current().id());
    }
    // Update cell pixel metrics unconditionally, ahead of the no-op dimension
    // guard below — a `resize_pty` call carrying only new device-pixel cell
    // metrics (e.g. a DPR change with no row/col change) still lands them for
    // future CSI 14t/16t replies, even on a call that no-ops for TIOCSWINSZ.
    if let (Some(w), Some(h)) = (cell_width_px, cell_height_px)
        && let Some(vt_log) = state.grid.vt_log_buffers.get(session_id)
    {
        vt_log.lock().set_cell_pixel_size(w, h);
    }
    // Serialize the whole grid+PTY resize for this session under one lock so two
    // concurrent differing resizes (Tauri `resize_pty` + HTTP route) cannot interleave
    // their two critical sections and leave the grid and PTY at mismatched dimensions
    // (CONC-B, story 100-e303). Clone the Arc and drop the DashMap Ref before locking
    // so we never hold a `resize_locks` shard guard across the resize.
    let resize_lock = state
        .session_maps
        .resize_locks
        .entry(session_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new((0, 0))))
        .clone();
    let mut applied = resize_lock.lock();
    // Seed the last-applied dims from the live grid the first time we see this session:
    // at creation the grid and PTY share the openpty size, so a first resize that only
    // matches the startup dims no-ops instead of firing a gratuitous SIGWINCH. `(0, 0)`
    // is the never-applied sentinel (real dims are guarded > 0 above).
    if *applied == (0, 0)
        && let Some(vt_log) = state.grid.vt_log_buffers.get(session_id)
    {
        let vt = vt_log.lock();
        *applied = (vt.grid_screen_lines() as u16, vt.grid_columns() as u16);
    }
    // No-op guard compares against the last dims that actually reached the PTY, not just
    // the grid: a prior call that resized the grid but then failed `master.resize` leaves
    // them divergent, and a grid-only guard would skip the PTY forever (CONC-B criterion 2).
    if *applied == (rows, cols) {
        return Ok(None);
    }
    // Resize the grid and capture a fresh full frame, holding the vt lock so no
    // PTY chunk can land between the check and the resize. `resize`
    // marks the grid fully damaged, so `serialize_dirty_rows` yields the whole
    // viewport. The caller must flush it: the reader thread only sends frames on
    // PTY data or the ticker, so a resize/zoom over idle or static content would
    // otherwise leave the viewport blank until a scroll forces
    // `terminal_request_frame`. If the grid already matches (PTY-only retry after a
    // prior `master.resize` failure) skip the grid work but still re-apply the PTY.
    let resize_frame = match state.grid.vt_log_buffers.get(session_id) {
        Some(vt_log) => {
            let mut vt = vt_log.lock();
            if vt.grid_screen_lines() == rows as usize && vt.grid_columns() == cols as usize {
                None
            } else {
                vt.resize(rows, cols);
                Some(vt.serialize_dirty_rows())
            }
        }
        None => None,
    };
    // Update terminal rows for cursor-up clamping in the reader thread.
    if let Some(r) = state.session_maps.terminal_rows.get(session_id) {
        r.store(rows, Ordering::Relaxed);
    }
    // Mark resize in silence state so the reader thread suppresses re-parsed events
    // from the shell's prompt redraw triggered by SIGWINCH.
    if let Some(ss) = state.session_maps.silence_states.get(session_id) {
        ss.lock().on_resize();
    }
    // Only now signal the child (TIOCSWINSZ → SIGWINCH): everything it repaints
    // from here on meets a grid that already wraps at the new width.
    let entry = state
        .session_maps
        .sessions
        .get(session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    let (cell_w, cell_h) = match state.grid.vt_log_buffers.get(session_id) {
        Some(vt_log) => vt_log.lock().cell_pixel_size(),
        None => (
            crate::terminal_grid::DEFAULT_CELL_WIDTH_PX,
            crate::terminal_grid::DEFAULT_CELL_HEIGHT_PX,
        ),
    };
    entry
        .lock()
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: cols.saturating_mul(cell_w),
            pixel_height: rows.saturating_mul(cell_h),
        })
        .map_err(|e| format!("Failed to resize PTY: {e}"))?;
    // Record the dims only now that they've reached the PTY, still under `applied`, so
    // a racing resize either waits behind this lock or observes a consistent value.
    // On a `master.resize` failure above we return via `?` WITHOUT updating `applied`,
    // so a later retry re-applies the PTY instead of no-opping on a grid-only match.
    *applied = (rows, cols);
    Ok(resize_frame)
}

/// Periodically checks all sessions for standby eligibility.
/// A session enters standby when:
/// 1. standby_timeout_minutes > 0
/// 2. session_visibility == false (tab not focused)
/// 3. shell_state == SHELL_IDLE
/// 4. idle duration >= timeout
/// 5. not already in standby
/// 6. startup_settled == true
#[cfg(unix)]
fn background_activity_blocks_standby(state: &AppState, session_id: &str) -> bool {
    background_activity_blocks_standby_with_silence(state, session_id, None)
}

/// `locked_silence`: pass the caller's already-held `SilenceState` guard when
/// one is held (e.g. `standby_session`'s `_lifecycle_guard`) — `None` re-locks
/// internally via `AppState::declared_background_work_for`. Never lock
/// `state.session_maps.silence_states` again here unconditionally: `SilenceState`'s mutex
/// is not reentrant, and `standby_session` calls this while already holding it.
#[cfg(unix)]
fn background_activity_blocks_standby_with_silence(
    state: &AppState,
    session_id: &str,
    locked_silence: Option<&SilenceState>,
) -> bool {
    let Some((turn_epoch, base_activity)) =
        state
            .session_maps
            .session_states
            .get(session_id)
            .map(|session| {
                (
                    session.turn_epoch,
                    session.background_work || session.has_pending_background_probe(),
                )
            })
    else {
        return false;
    };
    let declared_background_work = match locked_silence {
        Some(silence) => silence.declared_background_work_for_epoch(turn_epoch),
        None => state.declared_background_work_for(session_id, turn_epoch),
    };
    base_activity || declared_background_work
}

#[cfg(unix)]
pub(crate) fn spawn_standby_checker(state: Arc<AppState>) {
    use std::time::Duration;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let timeout_min = state.config.read().standby_timeout_minutes;
            if timeout_min == 0 {
                // Standby disabled: wake any sessions still parked (SIGSTOP'd)
                // from a previous non-zero timeout. Otherwise their stopped
                // badge persists until the user manually focuses each tab
                // (to-test.md:236, story 095).
                wake_all_standby(&state);
                continue;
            }
            let timeout_ms = u64::from(timeout_min) * 60_000;
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            let vis_count = state.session_maps.session_visibility.len();
            let sessions_count = state.session_maps.sessions.len();
            tracing::trace!(
                vis_count,
                sessions_count,
                timeout_min,
                "Standby checker tick"
            );

            for entry in state.session_maps.session_visibility.iter() {
                let session_id = entry.key();
                let visible = *entry.value();
                if visible {
                    continue;
                }
                if state
                    .session_maps
                    .standby_sessions
                    .contains_key(session_id.as_str())
                {
                    continue;
                }

                let shell_raw = state
                    .session_maps
                    .shell_states
                    .get(session_id.as_str())
                    .map(|a| a.load(Ordering::Acquire));
                let is_idle = shell_raw == Some(SHELL_IDLE);
                if !is_idle {
                    continue;
                }

                // For agents with a verified ready-screen adapter, a silence-only
                // idle is not strong enough to SIGSTOP the process group. Require
                // explicit Stop/OSC or a stable ready screen. Legacy agents that
                // lack an adapter retain their prior timeout behavior.
                let is_agent = state
                    .session_maps
                    .session_states
                    .get(session_id.as_str())
                    .map(|s| s.agent_type.is_some())
                    .unwrap_or(false);
                if is_agent && !idle_is_confirmed(&state, session_id.as_str()) {
                    tracing::trace!(
                        session_id = session_id.as_str(),
                        "Standby skipped: agent idle is heuristic-only"
                    );
                    continue;
                }
                if background_activity_blocks_standby(&state, session_id.as_str()) {
                    tracing::trace!(
                        session_id = session_id.as_str(),
                        "Standby skipped: background work or probe pending"
                    );
                    continue;
                }

                let idle_since = state
                    .session_maps
                    .shell_state_since_ms
                    .get(session_id.as_str())
                    .map(|a| a.load(Ordering::Acquire))
                    .unwrap_or(now_ms);
                let idle_ms = now_ms.saturating_sub(idle_since);
                if idle_ms < timeout_ms {
                    continue;
                }

                let settled = state
                    .session_maps
                    .silence_states
                    .get(session_id.as_str())
                    .map(|e| e.lock().startup_settled)
                    .unwrap_or(false);
                if !settled {
                    continue;
                }

                tracing::debug!(
                    session_id = session_id.as_str(),
                    idle_ms,
                    "Standby: all conditions met, stopping"
                );
                if let Err(e) = standby_session(&state, session_id) {
                    tracing::warn!(session_id, error = %e, "Standby failed");
                }
            }
        }
    });
}

/// SIGSTOP the entire process group of a session.
/// Returns Ok(true) if stopped, Ok(false) if already in standby or session gone.
#[cfg(unix)]
pub(crate) fn standby_session(state: &AppState, session_id: &str) -> Result<bool, String> {
    if state.session_maps.standby_sessions.contains_key(session_id) {
        return Ok(false);
    }
    // Serialize the final eligibility check with background-work updates. This
    // closes the gap between the periodic check above and the actual SIGSTOP.
    let silence = state
        .session_maps
        .silence_states
        .entry(session_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(SilenceState::new())))
        .clone();
    let _lifecycle_guard = silence.lock();
    if background_activity_blocks_standby_with_silence(state, session_id, Some(&_lifecycle_guard)) {
        return Ok(false);
    }
    let pgid = {
        let entry = state
            .session_maps
            .sessions
            .get(session_id)
            .ok_or_else(|| format!("Session not found: {session_id}"))?;
        let session = entry.value().lock();
        session
            .master
            .process_group_leader()
            .ok_or_else(|| "No process group leader".to_string())?
    };
    if pgid <= 1 || pgid == unsafe { libc::getpgid(0) } {
        return Err(format!("Unsafe pgid {pgid} — refusing SIGSTOP"));
    }
    let ret = unsafe { libc::kill(-pgid, libc::SIGSTOP) };
    if ret != 0 {
        return Err(format!(
            "SIGSTOP failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    state
        .session_maps
        .standby_sessions
        .insert(session_id.to_string(), now);
    tracing::info!(session_id, pgid, "Session entered standby (SIGSTOP)");
    emit_standby_event(state, session_id, true);
    Ok(true)
}

/// SIGCONT a session in standby. Returns Ok(true) if woken, Ok(false) if not in standby.
#[cfg(unix)]
pub(crate) fn wake_session(state: &AppState, session_id: &str) -> Result<bool, String> {
    if state
        .session_maps
        .standby_sessions
        .remove(session_id)
        .is_none()
    {
        return Ok(false);
    }
    let pgid = {
        let entry = state
            .session_maps
            .sessions
            .get(session_id)
            .ok_or_else(|| format!("Session not found: {session_id}"))?;
        let session = entry.value().lock();
        session
            .master
            .process_group_leader()
            .ok_or_else(|| "No process group leader".to_string())?
    };
    let ret = unsafe { libc::kill(-pgid, libc::SIGCONT) };
    if ret != 0 {
        return Err(format!(
            "SIGCONT failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    tracing::info!(session_id, pgid, "Session woken from standby (SIGCONT)");
    emit_standby_event(state, session_id, false);
    Ok(true)
}

/// Wake every session currently in standby. Used when the user disables standby
/// (timeout=0) so already-parked sessions resume instead of staying SIGSTOP'd.
/// Returns the number of sessions for which a wake was attempted.
///
/// Keys are collected into a Vec first: `wake_session` calls
/// `standby_sessions.remove`, and mutating a DashMap while holding an `iter()`
/// shard guard on the same map deadlocks. A session killed between the snapshot
/// and the wake is handled by `wake_session` (removes its entry, then returns
/// Err on the missing session — no panic).
#[cfg(unix)]
pub(crate) fn wake_all_standby(state: &AppState) -> usize {
    let parked: Vec<String> = state
        .session_maps
        .standby_sessions
        .iter()
        .map(|e| e.key().clone())
        .collect();
    for session_id in &parked {
        if let Err(e) = wake_session(state, session_id) {
            tracing::warn!(session_id, error = %e, "Standby wake-all (timeout=0) failed");
        }
    }
    parked.len()
}

#[cfg(unix)]
fn emit_standby_event(state: &AppState, session_id: &str, standby: bool) {
    #[cfg(feature = "desktop")]
    if let Some(ref app) = *state.app_handle.read() {
        let _ = app.emit(
            "session-standby",
            serde_json::json!({
                "session_id": session_id,
                "standby": standby,
            }),
        );
    }
}

/// SIGKILL the foreground process group of a PTY session.
///
/// An agent (e.g. claude) runs as a *grandchild* inside the PTY's shell and, under
/// job control, sits in its own foreground process group. SIGKILL on the shell
/// alone leaves that group orphaned — the cloned reader fd keeps the pty master
/// open, so the kernel never delivers SIGHUP to the foreground group, and the
/// agent is reparented to init and keeps running. killpg nukes the agent plus
/// every descendant in one shot. The shell (the session leader, in its own
/// process group) is reaped separately by the caller's `_child.kill()`.
#[cfg(unix)]
fn kill_foreground_process_group(session: &PtySession, session_id: &str) {
    let Some(pgid) = session.master.process_group_leader() else {
        return;
    };
    // Never signal pid <= 1 or our own group — that would take down TUIC itself.
    if pgid <= 1 || pgid == unsafe { libc::getpgid(0) } {
        tracing::warn!(session_id, pgid, "Refusing killpg on unsafe pgid");
        return;
    }
    if unsafe { libc::kill(-pgid, libc::SIGKILL) } != 0 {
        let err = std::io::Error::last_os_error();
        // ESRCH just means the group already exited — not worth a warning.
        if err.raw_os_error() != Some(libc::ESRCH) {
            tracing::warn!(session_id, pgid, "killpg(SIGKILL) failed: {err}");
        }
    }
}

/// Close a PTY session core: sends Ctrl-C, waits briefly for graceful exit,
/// captures the exit code for the tombstone, and leaves `output_buffers` +
/// `vt_log_buffers` + `last_output_ms` + `exit_codes` alive so post-mortem
/// MCP reads can still return final output and exit status.
///
/// Shared between the Tauri `close_pty` command and the MCP `close` action —
/// both paths must tombstone identically, or post-mortem reads break.
/// Returns the worktree path when `cleanup_worktree` is true and the session
/// had one, so the caller can run `remove_worktree_internal` outside this fn.
pub(crate) fn close_pty_core(
    state: &AppState,
    session_id: &str,
    cleanup_worktree: bool,
    reason: &str,
) -> Option<crate::state::WorktreeInfo> {
    let (_, session_mutex) = state.session_maps.sessions.remove(session_id)?;
    state
        .metrics
        .active_sessions
        .fetch_sub(1, Ordering::Relaxed);
    let mut session = session_mutex.into_inner();

    // Send Ctrl-C (0x03) to give the process a chance to clean up
    let mut writer = session.writer.lock();
    let _ = writer.write_all(&[0x03]);
    let _ = writer.flush();
    drop(writer);

    // Wait up to 100ms for process to exit gracefully
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
    loop {
        match session._child.try_wait() {
            Ok(Some(_)) => break, // Process exited cleanly
            Ok(None) if std::time::Instant::now() >= deadline => break,
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }

    // If the child is still alive after the grace window, force-kill it.
    // Without this, agents that ignore Ctrl-C (e.g. claude) become orphans —
    // the cloned reader fd keeps the pty master alive, the slave never sees
    // EOF, and the reader thread spins forever.
    if matches!(session._child.try_wait(), Ok(None)) {
        // Nuke the agent's foreground process group first; SIGKILL on the shell
        // alone leaves the agent (a grandchild) orphaned. See
        // kill_foreground_process_group.
        #[cfg(unix)]
        kill_foreground_process_group(&session, session_id);

        if let Err(e) = session._child.kill() {
            tracing::warn!(session_id = %session_id, "close_pty_core SIGKILL fallback failed: {e}");
        }
        // Brief wait so try_wait can observe the termination and record the code.
        let kill_deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
        loop {
            match session._child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if std::time::Instant::now() >= kill_deadline => break,
                _ => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        }
    }

    // Capture exit code for the tombstone before dropping the child handle.
    if let Ok(Some(status)) = session._child.try_wait() {
        state
            .session_maps
            .exit_codes
            .insert(session_id.to_string(), status.exit_code() as i32);
    }

    // Announce the close BEFORE tombstone_transient_cleanup: it removes
    // `state.pty_event_channels` (via `remove_live_session_state`), so
    // emitting after it means `emit_pty_event` finds no per-session channel
    // and the session-scoped WS `"closed"` frame is silently never
    // delivered — see `mcp_http::session::close_session`'s doc comment on
    // this exact hazard.
    emit_session_closed(state, session_id, reason);

    // Preserve output_buffers, vt_log_buffers, last_output_ms, exit_codes.
    // Tombstone sweeper reaps them after TOMBSTONE_TTL_MS.
    tombstone_transient_cleanup(session_id, state);

    let worktree_to_cleanup = if cleanup_worktree {
        session.worktree.clone()
    } else {
        None
    };
    // Independent of `cleanup_worktree`: if this was the last session attached
    // to the worktree, drop the lock taken on attach (`lock_worktree_for_session`
    // in `create_pty_with_worktree`) regardless of whether the worktree itself
    // is being removed or just detached from.
    let worktree_to_unlock = session.worktree.clone();

    // Drop session to release file handles (forcibly kills if still running)
    drop(session);

    if let Some(wt) = worktree_to_unlock {
        // `state.session_maps.sessions.remove` above already dropped this session, so any
        // remaining hit here is a genuinely different, still-live session.
        if state.live_sessions_in_worktree(&wt.path).is_empty() {
            crate::worktree::unlock_worktree(&wt.base_repo, &wt.path);
        }
    }

    worktree_to_cleanup
}

/// Force-kill a PTY session and tombstone it. Used by the MCP `kill` action.
/// Unlike `close_pty_core`, skips the Ctrl-C grace period — sends SIGKILL
/// immediately. The child exits near-instantly so `try_wait` captures the
/// exit code before the tombstone is stamped.
pub(crate) fn kill_pty_core(state: &AppState, session_id: &str, reason: &str) -> bool {
    let Some((_, session_mutex)) = state.session_maps.sessions.remove(session_id) else {
        return false;
    };
    state
        .metrics
        .active_sessions
        .fetch_sub(1, Ordering::Relaxed);
    let mut session = session_mutex.into_inner();

    // Nuke the agent's foreground process group first; SIGKILL on the shell
    // alone leaves the agent (a grandchild) orphaned. See
    // kill_foreground_process_group.
    #[cfg(unix)]
    kill_foreground_process_group(&session, session_id);

    if let Err(e) = session._child.kill() {
        tracing::warn!(session_id = %session_id, "SIGKILL failed: {e}");
    }

    // Give the kernel a brief window to reap the child so try_wait sees it.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
    loop {
        match session._child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() >= deadline => break,
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }

    if let Ok(Some(status)) = session._child.try_wait() {
        state
            .session_maps
            .exit_codes
            .insert(session_id.to_string(), status.exit_code() as i32);
    }

    // See `close_pty_core`'s identical comment: must precede
    // `tombstone_transient_cleanup`, which reaps the per-session WS channel
    // this emit needs to deliver the session-scoped "closed" frame.
    emit_session_closed(state, session_id, reason);

    tombstone_transient_cleanup(session_id, state);
    drop(session);
    true
}

/// Script interpreters that execute an agent CLI in their own process image.
/// For these the executable path names the interpreter, not the tool the user
/// launched, so the real identity has to come from argv[0]. Windows exposes no
/// argv in its process snapshot, so only the macOS and Linux arms use this.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn is_script_interpreter(name: &str) -> bool {
    matches!(
        name,
        "node" | "bun" | "deno" | "python" | "python3" | "ruby" | "perl"
    )
}

/// Look up the process name for a given PID using OS-native syscalls.
/// On macOS uses `proc_pidpath`, on Linux reads `/proc/{pid}/comm`.
/// Returns None if the lookup fails.
#[cfg(target_os = "macos")]
pub(crate) fn process_name_from_pid(pid: u32) -> Option<String> {
    let mut buf = [0u8; libc::MAXPATHLEN as usize];
    // SAFETY: proc_pidpath writes into the provided buffer up to buffersize bytes.
    // The buffer is stack-allocated with known size. pid is a valid u32 cast to i32.
    let ret = unsafe { libc::proc_pidpath(pid as i32, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if ret <= 0 {
        return None;
    }
    let path = std::str::from_utf8(&buf[..ret as usize]).ok()?;
    if let Some(agent_type) = classify_agent_name_or_path(path) {
        return Some(agent_type.to_string());
    }
    let basename = normalized_process_name(path);
    // An npm-installed agent CLI (pi ships as a node script) reports the node
    // binary here, so classify_agent would never see the tool's own name and the
    // session stayed an unclassified shell — no ready-screen adapter, stuck BUSY.
    // Only interpreters pay the extra syscall; every native binary returns above.
    if is_script_interpreter(basename)
        && let Some(argv0) = crate::process_env::read_process_argv0(pid)
    {
        if let Some(agent_type) = classify_agent_name_or_path(&argv0) {
            return Some(agent_type.to_string());
        }
        return Some(normalized_process_name(&argv0).to_string());
    }
    Some(basename.to_string())
}

#[cfg(target_os = "linux")]
pub(crate) fn process_name_from_pid(pid: u32) -> Option<String> {
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())?;
    // See the macOS arm: an npm-installed agent CLI reports its interpreter here,
    // so fall back to argv[0] to recover the tool's own name.
    if is_script_interpreter(&comm)
        && let Some(argv0) = crate::process_env::read_process_argv0(pid)
    {
        return Some(normalized_process_name(&argv0).to_string());
    }
    Some(comm)
}

#[cfg(windows)]
pub(crate) fn process_name_from_pid(pid: u32) -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next, TH32CS_SNAPPROCESS,
    };

    // SAFETY: CreateToolhelp32Snapshot/Process32First/Process32Next are Windows API
    // functions that operate on a process snapshot handle. We zero-initialize the
    // PROCESSENTRY32 struct and set dwSize before use (required by the API). The
    // snapshot handle is closed via CloseHandle before returning. All pointer
    // arguments point to stack-local owned memory with valid lifetimes.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return None;
        }

        let mut entry: PROCESSENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;

        let mut found = None;
        if Process32First(snapshot, &mut entry) != 0 {
            loop {
                if entry.th32ProcessID == pid {
                    // szExeFile is a [i8; 260] (MAX_PATH) null-terminated C string
                    let name_bytes: Vec<u8> = entry
                        .szExeFile
                        .iter()
                        .take_while(|&&b| b != 0)
                        .map(|&b| b as u8)
                        .collect();
                    // Use from_utf8_lossy to handle non-ASCII process names
                    // (e.g. apps with accented characters) instead of silently dropping them
                    let name = String::from_utf8_lossy(&name_bytes);
                    // Strip .exe suffix for consistent matching with classify_agent
                    let name = name.strip_suffix(".exe").unwrap_or(&name).to_string();
                    found = Some(name);
                    break;
                }
                if Process32Next(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
        found
    }
}

/// Walk the process tree from `root_pid` and return the deepest descendant PID.
/// On Windows, this finds the "foreground" process in a PTY session by following
/// Normalize a path by resolving `.` and `..` components logically
/// (without requiring the path to exist on disk).
fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut result = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            other => result.push(other),
        }
    }
    result
}

/// the chain: shell → agent CLI (e.g. claude.exe).
#[cfg(windows)]
pub(crate) fn deepest_descendant_pid(root_pid: u32) -> Option<u32> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next, TH32CS_SNAPPROCESS,
    };

    // SAFETY: Same API contract as process_name_from_pid above. We take a full
    // process snapshot, iterate it to collect (pid, parent_pid) pairs into owned
    // Vecs, then close the handle. The PROCESSENTRY32 struct is zero-initialized
    // with dwSize set before the first call, satisfying the API precondition.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return None;
        }

        // Collect all (pid, parent_pid) pairs and build parent->children map
        let mut children_map: std::collections::HashMap<u32, Vec<u32>> =
            std::collections::HashMap::new();
        let mut entry: PROCESSENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;

        if Process32First(snapshot, &mut entry) != 0 {
            loop {
                children_map
                    .entry(entry.th32ParentProcessID)
                    .or_default()
                    .push(entry.th32ProcessID);
                if Process32Next(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);

        // Walk from root_pid to the deepest single child — O(depth) via HashMap
        let mut current = root_pid;
        while let Some([only_child]) = children_map.get(&current).map(Vec::as_slice) {
            current = *only_child;
        }

        Some(current)
    }
}

/// Map a process name to a known agent type, or None for non-agent processes.
///
/// A versioned basename counts as the tool. grok 1.0.5 installs
/// `~/.grok/bin/grok` as a symlink to `grok-1.0.5`, and `proc_pidpath` resolves
/// the link, so the foreground process reads `grok-1.0.5`. Against an
/// exact-match table that is None: the session gets no `agent_type`, so
/// `has_ready_screen_adapter` is false and the OSC 133 busy bit set once by the
/// long-lived `grok` command is never cleared — the tab stays working for the
/// whole process. An agent that renames its binary per release must not be able
/// to un-detect itself.
pub(crate) fn classify_agent(process_name: &str) -> Option<&'static str> {
    exact_agent_name(process_name).or_else(|| exact_agent_name(strip_version_suffix(process_name)))
}

/// Drop a trailing `-<version>` from an executable basename (`grok-1.0.5` →
/// `grok`). The suffix must start with a digit, so a hyphenated tool name
/// (`cursor-agent`) keeps its own identity.
fn strip_version_suffix(name: &str) -> &str {
    match name.rsplit_once('-') {
        Some((base, suffix)) if suffix.starts_with(|c: char| c.is_ascii_digit()) => base,
        _ => name,
    }
}

fn exact_agent_name(process_name: &str) -> Option<&'static str> {
    match process_name {
        "claude" => Some("claude"),
        "gemini" => Some("gemini"),
        "opencode" => Some("opencode"),
        "aider" => Some("aider"),
        "codex" => Some("codex"),
        "amp" => Some("amp"),
        "cursor-agent" => Some("cursor"),
        "goose" => Some("goose"),
        "grok" => Some("grok"),
        "droid" => Some("droid"),
        "pi" => Some("pi"),
        _ => None,
    }
}

/// Plain-function body of [`get_session_foreground_process`], factored out so
/// tests can call it directly with a plain `&AppState` instead of having to
/// construct a `tauri::State` wrapper, and so the HTTP/remote transport
/// (`mcp_http::session::get_foreground_process`) can share the exact same
/// detection-and-mirror logic instead of drifting from a second copy — none
/// of what this function calls is desktop/Tauri-specific, only the
/// `#[tauri::command]` wrapper above is.
pub(crate) fn get_session_foreground_process_impl(
    state: &AppState,
    session_id: &str,
) -> Option<String> {
    const SHELLS: &[&str] = &[
        "zsh",
        "bash",
        "fish",
        "sh",
        "dash",
        "ksh",
        "csh",
        "tcsh",
        "nushell",
        "nu",
        "powershell",
        "pwsh",
        "cmd",
        // Less common but real login shells. A shell missing from this list
        // isn't just unrecognized for classification — since 2026-09-10 it
        // also can never trigger the confirmed-shell agent_type clear below,
        // so its user's LastPromptBar/agent-idle-threshold would stay stuck
        // exactly like the bug this list's newer consumer was fixing.
        "xonsh",
        "elvish",
        "ion",
        "murex",
    ];

    let (detected, fg_is_shell) = {
        let entry = state.session_maps.sessions.get(session_id)?;
        let session = entry.value().lock();
        // TUIC already knows exactly which shell binary it launched for THIS
        // session (`session.shell`, set from `resolve_shell()` at PTY
        // creation) — matching against it directly covers any login shell,
        // not just the ones on the static `SHELLS` list above, which can
        // never be exhaustive by construction.
        // Strip a trailing ".exe" (case-insensitive) to match
        // `process_name_from_pid`'s Windows arm, which does the same for
        // consistency with `classify_agent` — without this, `session.shell`
        // (which `default_shell()`/`resolve_shell()` always populate WITH the
        // suffix on Windows, e.g. "powershell.exe") would never match the
        // detected foreground name (which never carries it), silently
        // defeating this fallback for every Windows shell not already on the
        // static `SHELLS` list.
        let session_shell_name = std::path::Path::new(&session.shell)
            .file_name()
            .and_then(|f| f.to_str())
            .map(|s| match s.len().checked_sub(4) {
                Some(cut) if s[cut..].eq_ignore_ascii_case(".exe") => s[..cut].to_string(),
                _ => s.to_string(),
            });
        #[cfg(not(windows))]
        {
            let pgid = session.master.process_group_leader()?;
            let name = process_name_from_pid(pgid as u32)?;
            let is_shell = SHELLS.contains(&name.as_str())
                || session_shell_name.as_deref() == Some(name.as_str());
            (classify_agent(&name).map(|s| s.to_string()), is_shell)
        }
        #[cfg(windows)]
        {
            let child_pid = session._child.process_id()?;
            let leaf = deepest_descendant_pid(child_pid)?;
            let name = process_name_from_pid(leaf)?;
            let is_shell = SHELLS.contains(&name.as_str())
                || session_shell_name.as_deref() == Some(name.as_str());
            (classify_agent(&name).map(|s| s.to_string()), is_shell)
        }
    };

    // Fallback: unrecognised non-shell foreground + pre-set agent type → use preset.
    // Covers custom commands (aliases, symlinks, wrappers) from run configs.
    let effective = detected.clone().or_else(|| {
        if fg_is_shell {
            return None;
        }
        state
            .session_maps
            .session_states
            .get(session_id)
            .and_then(|s| s.agent_type.clone())
    });

    // Mirror the detected agent type into session_states so the PTY reader's
    // `agent_active_for_parse` check flips on and plain-prefix structured
    // tokens (`intent:`, `action:`, `suggest:`) start being parsed. Without
    // this sync, sessions started by running `claude` inside a plain shell
    // (as opposed to via the /agent spawn route) never enable plain-prefix
    // parsing, so intents never rename the tab.
    //
    // Sticky on unrecognized foreground: only set on Some, never clear on None
    // from an *unrecognized* foreground. Foreground-pgid sampling is inherently
    // flaky during subprocess transitions — when claude spawns a short-lived
    // grandchild (git, sed, rg) the pgid leader briefly points to that
    // unrecognized binary and classify_agent returns None. Writing that None
    // back would flip agent_active off and drop the very next
    // `suggest:`/`intent:` token even though claude is still the live agent.
    // Frontend useAgentPolling.ts applies the same stickiness (streak +
    // source=idle) on its store mirror; backend must match or the parser
    // gates off while the UI still shows the agent active.
    //
    // A *confirmed* shell foreground is not that flaky case — it's a positive
    // match against the known SHELLS list, not "unrecognized" — so once we've
    // actually seen this session's agent running at least once
    // (`agent_seen_running`), it's a reliable signal that the agent has since
    // exited and control returned to the shell. Clearing here (rather than
    // leaving the mirror permanently sticky for the session's lifetime) lets
    // the idle-threshold selection in `should_transition_idle_with_hook` drop
    // back to the shorter shell-idle window immediately, instead of the
    // longer agent-idle window outliving the agent that justified it.
    //
    // `agent_seen_running` gates this deliberately: a freshly created session
    // whose `agent_type` is only a run-config *preset* (a custom/unrecognized
    // launcher's binary hasn't been exec'd yet — see `PtyConfig::agent_type`)
    // is ALSO a confirmed-shell foreground at this point, since nothing has
    // launched yet. Clearing unconditionally on that would permanently wipe
    // the preset the instant the shell's first idle event fires (which races
    // with, and can land before, the pending init command actually running) —
    // `classify_agent` will never subsequently recognise a custom binary, so
    // nothing would ever restore it. Session teardown also clears
    // session_states entirely, independent of this.
    //
    // A direct `classify_agent` match (`detected.is_some()`) confirms
    // `agent_seen_running` immediately — there's no ambiguity about what's
    // running. The fallback path (unrecognized non-shell, resolved only via
    // the preset) is genuinely ambiguous — it can't tell "the preset's own
    // launcher" from "an intermediate wrapper hop" (`direnv exec`, a
    // non-`exec`'d wrapper script) — so it requires the foreground to
    // persist across `AGENT_SEEN_RUNNING_CONFIRM_MS` before confirming,
    // so a wrapper that fails almost immediately can't strand the preset.
    if let Some(mut entry) = state.session_maps.session_states.get_mut(session_id) {
        if !fg_is_shell && effective.is_some() {
            if entry.agent_seen_running {
                // Already confirmed — no more debounce bookkeeping needed.
            } else if detected.is_some() {
                entry.agent_seen_running = true;
                entry.agent_seen_running_pending_since_ms = None;
            } else {
                let now = now_epoch_ms();
                match entry.agent_seen_running_pending_since_ms {
                    None => entry.agent_seen_running_pending_since_ms = Some(now),
                    Some(first_seen)
                        if now.saturating_sub(first_seen) >= AGENT_SEEN_RUNNING_CONFIRM_MS =>
                    {
                        entry.agent_seen_running = true;
                        entry.agent_seen_running_pending_since_ms = None;
                    }
                    _ => {}
                }
            }
            if entry.agent_type != effective {
                entry.agent_type = effective.clone();
                entry.hook_instrumented = hook_instrumented_for(
                    &crate::config::load_agents_config(),
                    entry.agent_type.as_deref(),
                );
            }
        } else if fg_is_shell {
            // A pending ambiguous-confirmation window that never reached its
            // threshold means whatever was running already ended — correctly
            // never confirmed, but the stale timestamp must not leak into a
            // later, unrelated episode.
            entry.agent_seen_running_pending_since_ms = None;
        }
    }

    if fg_is_shell {
        clear_agent_type_on_confirmed_shell(state, session_id);
    }

    effective
}

/// Clear the sticky `session_states.agent_type` mirror once the shell has
/// genuinely reclaimed the foreground AND the session's agent had actually
/// been observed running (`agent_seen_running`) — never touches an
/// unconfirmed run-config preset. Shared by two independent "shell is back"
/// signals: the fast, event-driven OSC 133 prompt marker
/// (`transition_explicit_shell_state_with_hook`, OSC 133 only — deliberately
/// NOT OSC 7770, whose "idle" means "agent finished this turn, still
/// running," not "agent exited"; see that function's call site) and the
/// pgid-polling fallback above (for shells without injected shell
/// integration, or when the fast path is unavailable).
fn clear_agent_type_on_confirmed_shell(state: &AppState, session_id: &str) {
    if let Some(mut entry) = state.session_maps.session_states.get_mut(session_id)
        && entry.agent_type.is_some()
        && entry.agent_seen_running
    {
        entry.agent_type = None;
        entry.hook_instrumented = false;
        entry.agent_seen_running = false;
        entry.agent_seen_running_pending_since_ms = None;
    }
}

/// Info about an active PTY session for frontend reconnection
#[derive(Clone, Serialize)]
pub(crate) struct ActiveSessionInfo {
    session_id: String,
    cwd: Option<String>,
    worktree_path: Option<String>,
    worktree_branch: Option<String>,
    display_name: Option<String>,
    display_name_is_custom: bool,
    is_remote: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pty_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    accent_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<crate::state::SessionState>,
}

/// Set (or clear, with `color: None`) a PTY session's accent color — the
/// desktop IPC twin of `mcp_http::session::set_session_accent_color`. Real
/// consumer: the tmux compatibility shim's `set-option ... *-border-style`
/// dispatch (`mcp_http::tmux_routes`), delivering Claude Code's per-teammate
/// `--agent-color`. Unlike `set_session_name`, the value isn't a field on
/// `PtySession` — `AppState::set_pty_accent_color` owns storage (a separate
/// `pty_accent_colors` map, mirroring `pty_descriptions`), the
/// unchanged-value no-op guard, and the dual emit, so this command is a
/// thin existence check plus a call-through.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn set_session_accent_color(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    color: Option<String>,
) -> Result<(), String> {
    if !state.session_maps.sessions.contains_key(&session_id) {
        return Err(format!("Session not found: {session_id}"));
    }
    state.set_pty_accent_color(&session_id, color);
    Ok(())
}

/// List all active PTY sessions for reconnection after frontend reload
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn list_active_sessions(state: State<'_, Arc<AppState>>) -> Vec<ActiveSessionInfo> {
    list_active_sessions_impl(&state)
}

/// The real body of `list_active_sessions`, taking a plain `&AppState`
/// rather than a `tauri::State` — no test anywhere in this codebase
/// constructs a `tauri::State` outside a running app, so a
/// `#[tauri::command]` fn with one is otherwise untestable. Extracted so
/// this logic (specifically: does it actually surface `pty_accent_colors`
/// per-session, same as its HTTP twin `mcp_http::session::list_sessions`)
/// has a direct test.
fn list_active_sessions_impl(state: &AppState) -> Vec<ActiveSessionInfo> {
    state
        .session_maps
        .sessions
        .iter()
        .map(|entry| {
            let session_id = entry.key().clone();
            let session = entry.value().lock();
            ActiveSessionInfo {
                session_id,
                cwd: session.cwd.clone(),
                worktree_path: session
                    .worktree
                    .as_ref()
                    .map(|w| w.path.to_string_lossy().to_string()),
                worktree_branch: session.worktree.as_ref().and_then(|w| w.branch.clone()),
                display_name: session.display_name.clone(),
                display_name_is_custom: session.display_name_is_custom,
                is_remote: session.is_remote,
                pty_description: state
                    .session_maps
                    .pty_descriptions
                    .get(entry.key())
                    .map(|value| value.value().clone()),
                accent_color: state
                    .session_maps
                    .pty_accent_colors
                    .get(entry.key())
                    .map(|value| value.value().clone()),
                state: state.session_state_with_shell(entry.key()),
            }
        })
        .collect()
}

/// Per-process resource usage for the process manager modal.
#[derive(Clone, Serialize)]
pub(crate) struct ProcessStats {
    pub(crate) session_id: Option<String>,
    pub(crate) name: String,
    pub(crate) pid: u32,
    pub(crate) rss_kb: u64,
    pub(crate) cpu_pct: f32,
}

/// Collect CPU/memory stats for TUIC itself and all PTY child process trees.
pub(crate) fn collect_process_stats(state: &AppState) -> Vec<ProcessStats> {
    let mut pids: Vec<(Option<String>, String, u32)> = Vec::new();

    // TUIC's own process
    let own_pid = std::process::id();
    pids.push((None, "TUICommander".to_string(), own_pid));

    // One process-table query serves every session below.
    let parent_map = process_parent_map();

    // Collect child PIDs from all PTY sessions
    for entry in state.session_maps.sessions.iter() {
        let session_id = entry.key().clone();
        let session = entry.value().lock();
        let display = session
            .display_name
            .clone()
            .unwrap_or_else(|| session_id.chars().take(8).collect());

        #[cfg(not(windows))]
        let child_pid = session.master.process_group_leader().map(|p| p as u32);
        #[cfg(windows)]
        let child_pid = session._child.process_id();
        drop(session);

        if let Some(pid) = child_pid {
            pids.push((Some(session_id.clone()), display.clone(), pid));
            // Walk descendants out of the shared map
            for dpid in parent_map
                .as_ref()
                .map(|map| descendants_from_parent_map(map, pid))
                .unwrap_or_default()
            {
                let name = process_name_from_pid(dpid).unwrap_or_else(|| format!("pid:{dpid}"));
                pids.push((Some(session_id.clone()), name, dpid));
            }
        }
    }

    if pids.is_empty() {
        return vec![];
    }

    let pid_list: Vec<u32> = pids.iter().map(|(_, _, p)| *p).collect();
    let stats_map = query_process_stats(&pid_list);

    pids.into_iter()
        .map(|(sid, name, pid)| {
            let (rss_kb, cpu_pct) = stats_map.get(&pid).copied().unwrap_or((0, 0.0));
            ProcessStats {
                session_id: sid,
                name,
                pid,
                rss_kb,
                cpu_pct,
            }
        })
        .collect()
}

/// Map every live process onto its children, from a SINGLE OS query.
///
/// The process-manager refresh walks one subtree per session. Querying the
/// table inside that loop forked `ps` once per session — N+1 forks per refresh,
/// each one taken while the session lock was held. One shared map serves every
/// root, so the cost no longer scales with the number of sessions.
fn process_parent_map() -> Option<std::collections::HashMap<u32, Vec<u32>>> {
    #[cfg(not(windows))]
    {
        let output = std::process::Command::new("ps")
            .args(["-eo", "pid,ppid"])
            .output()
            .ok()?;
        let parent_map = parse_process_parent_map(&String::from_utf8_lossy(&output.stdout));
        (!parent_map.is_empty()).then_some(parent_map)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
        let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snap.is_null() {
            return None;
        }
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut parent_map: std::collections::HashMap<u32, Vec<u32>> =
            std::collections::HashMap::new();
        if unsafe { Process32FirstW(snap, &mut entry) } != 0 {
            loop {
                parent_map
                    .entry(entry.th32ParentProcessID)
                    .or_default()
                    .push(entry.th32ProcessID);
                if unsafe { Process32NextW(snap, &mut entry) } == 0 {
                    break;
                }
            }
        }
        let _ = unsafe { CloseHandle(snap) };
        (!parent_map.is_empty()).then_some(parent_map)
    }
}

/// Parse `ps -eo pid,ppid` output into a parent -> children map.
///
/// Rows that do not read as two PIDs (the header, a truncated line) are
/// skipped. Aborting on the first unreadable row would report every session as
/// childless, and one shared map makes that failure global instead of local.
#[cfg(not(windows))]
fn parse_process_parent_map(text: &str) -> std::collections::HashMap<u32, Vec<u32>> {
    let mut parent_map: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(Ok(pid)), Some(Ok(parent_pid))) = (
            parts.next().map(str::parse::<u32>),
            parts.next().map(str::parse::<u32>),
        ) else {
            continue;
        };
        parent_map.entry(parent_pid).or_default().push(pid);
    }
    parent_map
}

/// Every transitive descendant of `root`, excluding the root itself.
///
/// `seen` guards the walk: the table comes from the OS, and a self-parented row
/// would otherwise spin forever inside a stats refresh.
fn descendants_from_parent_map(
    parent_map: &std::collections::HashMap<u32, Vec<u32>>,
    root: u32,
) -> Vec<u32> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::from([root]);
    let mut stack = vec![root];
    while let Some(pid) = stack.pop() {
        let Some(children) = parent_map.get(&pid) else {
            continue;
        };
        for &child in children {
            if seen.insert(child) {
                result.push(child);
                stack.push(child);
            }
        }
    }
    result
}

/// Query RSS (KB) and CPU% for a batch of PIDs using `ps` on Unix.
#[cfg(not(windows))]
fn query_process_stats(pids: &[u32]) -> std::collections::HashMap<u32, (u64, f32)> {
    let mut map = std::collections::HashMap::new();
    if pids.is_empty() {
        return map;
    }
    let pid_args: Vec<String> = pids.iter().map(|p| p.to_string()).collect();
    let Ok(output) = std::process::Command::new("ps")
        .args(["-o", "pid,rss,%cpu", "-p"])
        .arg(pid_args.join(","))
        .output()
    else {
        return map;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3
            && let (Ok(pid), Ok(rss), Ok(cpu)) = (
                parts[0].parse::<u32>(),
                parts[1].parse::<u64>(),
                parts[2].parse::<f32>(),
            )
        {
            map.insert(pid, (rss, cpu));
        }
    }
    map
}

/// Query RSS (KB) and CPU% for a batch of PIDs on Windows.
#[cfg(windows)]
fn query_process_stats(pids: &[u32]) -> std::collections::HashMap<u32, (u64, f32)> {
    let mut map = std::collections::HashMap::new();
    for &pid in pids {
        if let Some((rss, cpu)) = query_single_process_windows(pid) {
            map.insert(pid, (rss, cpu));
        }
    }
    map
}

#[cfg(windows)]
fn query_single_process_windows(pid: u32) -> Option<(u64, f32)> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };
    let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut mem_info: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    mem_info.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    let ok = unsafe {
        GetProcessMemoryInfo(
            handle,
            &mut mem_info,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    };
    let _ = unsafe { CloseHandle(handle) };
    if ok == 0 {
        return None;
    }
    let rss_kb = mem_info.WorkingSetSize / 1024;
    Some((rss_kb as u64, 0.0))
}

#[derive(Debug, Clone, Serialize)]
pub struct VtLogChunk {
    pub lines: Vec<crate::state::LogLine>,
    pub screen: Vec<crate::state::LogLine>,
    pub total_lines: usize,
    pub oldest: usize,
}

/// Consume the coalesced scroll target a client left for this session.
///
/// Both transports write it (`terminal_scroll_to_offset` as a Tauri command and
/// as an HTTP route) without taking the vt lock, and the frame ticker is the only
/// reader — so the swap here is what makes "latest wins" true: whatever arrived
/// since the last tick is applied once, and `-1` means nothing is pending.
fn take_pending_scroll(state: &AppState, session_id: &str) -> Option<usize> {
    let target = state
        .grid
        .pending_scroll
        .get(session_id)?
        .swap(-1, Ordering::Relaxed);
    (target >= 0).then_some(target as usize)
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
    if state.grid.channels.contains_key(session_id) {
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
    let desktop_wants_it = state.grid.channels.contains_key(session_id);
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
            if crate::grid_gate::claim_grid_frame(&watch_tx, order, for_watch)
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
    let gate = state.grid.gates.get(session_id);
    if let Some(gate) = gate.as_deref() {
        gate.mark_sent();
    }
    // `tauri::ipc::Response` is what keeps this binary. A `Vec<u8>` matches
    // only the blanket `IpcResponse` impl, i.e. `serde_json::to_string`, so a
    // 110 KB frame left Rust as a ~280 KB string of decimal numbers, took the
    // over-threshold path (one extra IPC round trip per frame) and arrived in
    // JS as a `number[]` to be walked back into bytes. `Response` carries the
    // bytes as `Raw` and the frontend already accepts an ArrayBuffer.
    if let Err(error) = ch.send(tauri::ipc::Response::new(bytes)) {
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
