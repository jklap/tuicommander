//! Executes a parsed [`super::args::TmuxOp`] against a [`TuicBackend`].
//!
//! `execute` is the only place in this module that talks to anything —
//! everything else (`args`, `format`, `target`) is pure. `execute` itself
//! never calls `println!`/`eprintln!`: it returns an [`Outcome`] the thin
//! `tmux_compat()` wrapper prints, so a test can assert on returned strings
//! instead of captured process output, and a real `std::process::exit` in
//! the wrapper can flush stdout first (`process::exit` skips the runtime's
//! own flush — the pre-refactor `has-session` bypassed this entirely by
//! calling `process::exit` from inside the match arm itself).
//!
//! The four legacy arms that must stay byte-identical for existing `tuic
//! alias` users (`list-sessions`, `kill-server`'s TUIC-session half,
//! `capture-pane`, `attach-session`, bare `tmux`) go through
//! [`crate::dispatch`] — that function does its own printing, same as
//! before this refactor. Every new arm below is pure `Outcome` data.

use super::args::TmuxOp;
use super::format::{FormatCtx, render_format};
use super::target::{parse_target, resolve_pane, resolve_session, resolve_window};
use crate::Command;
use serde_json::Value;

/// Real tmux's `new-session`/`split-window`/`new-window` default `cwd` to the
/// *calling* client's own current directory when `-c` is absent — never to
/// some fixed global default. Claude Code's swarm path never passes `-c` at
/// all (confirmed empirically: every captured `new-session`/`split-window`
/// call omits it), so without this fallback every swarm-created pane spawns
/// with `cwd: None`, which `spawn_pty_session` (session.rs) leaves as
/// whatever directory the TUICommander *app process* happens to be running
/// in — unrelated to the repo the lead session actually lives in. That
/// wrong cwd then fails `resolveRepoOwner` on the frontend, and the tab
/// silently lands under whichever repo happens to be active in the sidebar
/// at that moment (confirmed live 2026-09-04: a 4-teammate swarm spawned
/// from a `commerce-journal` session landed all 4 panes under
/// `databricks-sql-cli`, the repo the user happened to be focused on).
///
/// `inherited` — an existing pane's already-resolved cwd in the SAME
/// session/window, when one exists — takes priority over a fresh
/// `std::env::current_dir()` read. This matters for `split-window`/
/// `new-window`: each `tmux` subcommand is its own OS subprocess, so a
/// second `current_dir()` call is a genuinely independent read that could
/// in principle disagree with the first pane's cwd (a code-review finding,
/// 2026-09-04 — not observed live, but cheap to close off entirely rather
/// than rely on the calling process's cwd staying constant across several
/// separate subprocess invocations). Inheriting from topology instead
/// guarantees every pane in one swarm shares the exact cwd the *first* pane
/// resolved, which also matches real tmux's own actual semantics more
/// closely — real `split-window`/`new-window` without `-c` default to the
/// pane/session being split from, not to the invoking client. `new-session`
/// has nothing to inherit from (it's the first pane), so its call site
/// passes `inherited = None`.
///
/// `env_repo_cwd()` — the lead agent's own stable `TUIC_WORKTREE_PATH`/
/// `TUIC_MAIN_REPO_PATH` env var (see its own doc comment) — now sits ahead
/// of BOTH `inherited` and `current_dir()`. Found live 2026-09-23: a
/// 6-teammate swarm spawned from `ssh-connections` (repo: `tuicommander`)
/// put 4 teammates in an unrelated directory (`~/bin`) — the calling
/// agent's shell had transiently `cd`'d there to run a one-off script at
/// the exact moment Claude Code's agent-teams internals fired
/// `tmux new-session`, so `current_dir()`'s snapshot was wrong for the
/// *entire* swarm, and every `split-window` teammate faithfully inherited
/// that one bad reading via `inherited` — topology-inheritance guarantees
/// pane-to-pane *consistency*, not correctness. Checking the env var first
/// closes this off two ways: it directly fixes `new-session`'s own
/// resolution (a `cd` never touches an already-set env var, only the live
/// process cwd), and — since every `resolve_cwd()` call site independently
/// re-reads it — it also self-heals any pane whose `inherited` value was
/// ALREADY wrong from an earlier bad reading, rather than faithfully
/// propagating it forward. This can only ever narrow (not change) behavior
/// for a caller with no TUIC_* env at all (a plain `tuic alias`
/// general-purpose user, or any process not spawned through TUICommander):
/// `env_repo_cwd()` returns `None` and this is exactly the prior
/// `inherited.or_else(current_dir)` chain, unchanged.
fn resolve_cwd(cwd: Option<String>, inherited: Option<&str>) -> Option<String> {
    cwd.or_else(env_repo_cwd)
        .or_else(|| inherited.map(String::from))
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
        })
}

/// The calling process's own stable worktree/repo-root env var, injected
/// once at PTY spawn time (`pty.rs::inject_worktree_env`) and never touched
/// again for that PTY's whole life — unlike `std::env::current_dir()`,
/// which tracks the shell's LIVE working directory and silently drifts the
/// moment it `cd`s anywhere else. `tuic-cli`, exec'd as the `tmux` alias by
/// Claude Code's agent-teams feature, is a child process of that same
/// lead-agent shell and inherits this exact environment untouched.
///
/// Prefers `TUIC_WORKTREE_PATH` (a linked worktree's own root) over
/// `TUIC_MAIN_REPO_PATH` (which for a worktree session deliberately points
/// at the *main* checkout instead, not itself — see `script_env.rs`), so a
/// swarm spawned from inside a worktree lands its teammates in that
/// worktree, not the main checkout. Returns `None` (never an empty string)
/// when neither var is set — any process not spawned through TUICommander
/// at all, e.g. a `tuic alias` general-purpose user working outside the app.
fn env_repo_cwd() -> Option<String> {
    std::env::var("TUIC_WORKTREE_PATH")
        .or_else(|_| std::env::var("TUIC_MAIN_REPO_PATH"))
        .ok()
        .filter(|v| !v.is_empty())
}

/// The first pane's `cwd` found anywhere under the given session, or `None`
/// if the session has no panes with a recorded cwd yet. Used by `new-window`
/// to inherit the swarm's already-established cwd instead of independently
/// re-reading `std::env::current_dir()`.
fn topology_cwd_for_session<'a>(topology: &'a Value, session_id: &str) -> Option<&'a str> {
    topology["sessions"].as_array()?.iter().find_map(|s| {
        if s["id"].as_str() != Some(session_id) {
            return None;
        }
        s["windows"]
            .as_array()?
            .iter()
            .flat_map(|w| w["panes"].as_array().into_iter().flatten())
            .find_map(|p| p["cwd"].as_str())
    })
}

/// The first pane's `cwd` found under the given window, or `None`. Used by
/// `split-window` to inherit the pane it's splitting from, rather than
/// independently re-reading `std::env::current_dir()`.
fn topology_cwd_for_window<'a>(topology: &'a Value, window_id: &str) -> Option<&'a str> {
    topology["sessions"].as_array()?.iter().find_map(|s| {
        s["windows"].as_array()?.iter().find_map(|w| {
            if w["id"].as_str() != Some(window_id) {
                return None;
            }
            w["panes"]
                .as_array()?
                .iter()
                .find_map(|p| p["cwd"].as_str())
        })
    })
}

pub(crate) struct Outcome {
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
    pub exit: i32,
}

impl Outcome {
    fn ok() -> Self {
        Self {
            stdout: vec![],
            stderr: vec![],
            exit: 0,
        }
    }

    fn ok_line(line: impl Into<String>) -> Self {
        Self {
            stdout: vec![line.into()],
            stderr: vec![],
            exit: 0,
        }
    }

    fn err(msg: impl Into<String>) -> Self {
        Self {
            stdout: vec![],
            stderr: vec![format!("tmux: {}", msg.into())],
            exit: 1,
        }
    }

    fn exit_only(code: i32) -> Self {
        Self {
            stdout: vec![],
            stderr: vec![],
            exit: code,
        }
    }

    pub(crate) fn usage_error(e: &super::args::ArgError) -> Self {
        Self {
            stdout: vec![],
            stderr: vec![format!("tmux: {e}")],
            exit: 1,
        }
    }

    /// Wrap a `crate::dispatch(...)` result exactly as the pre-refactor
    /// `tmux_compat()` did: `dispatch` already printed via its own
    /// `println!`/`print!` calls, so this only needs to translate the
    /// error into the historical `tmux: {e}` stderr line and exit code.
    fn from_dispatch(result: Result<(), String>) -> Self {
        match result {
            Ok(()) => Self::ok(),
            Err(e) => Self::err(e),
        }
    }
}

/// Everything `execute` needs from a running TUICommander instance. A trait
/// so tests can supply an in-memory fake instead of a real socket.
pub(crate) trait TuicBackend {
    /// Not `ipc::ensure_running()` — this trait must never auto-launch the
    /// app or block waiting for it (that call can stall a `split-window` for
    /// up to 10s, and on macOS launches the *installed* app, not whatever
    /// dev build a caller has `TUIC_SOCKET` pointed at). Implementations
    /// call this up front to fail fast with a clean message instead of a raw
    /// connection-refused error.
    fn is_running(&self) -> bool;
    fn write(&self, session_id: &str, data: &str) -> Result<(), String>;

    fn get_topology(&self, label: &str) -> Result<Value, String>;
    /// Returns `{session_id, window_id, pane_id}`.
    fn create_tmux_session(
        &self,
        label: &str,
        name: &str,
        window_name: Option<&str>,
        cwd: Option<&str>,
    ) -> Result<Value, String>;
    fn delete_tmux_session(&self, label: &str, session_id: &str) -> Result<(), String>;
    /// Returns `{window_id, pane_id}`.
    fn create_tmux_window(
        &self,
        label: &str,
        session_id: &str,
        name: Option<&str>,
        cwd: Option<&str>,
    ) -> Result<Value, String>;
    /// Returns `{pane_id, tuic_session_id}` — materialises immediately.
    fn create_tmux_pane(
        &self,
        label: &str,
        window_id: &str,
        cwd: Option<&str>,
    ) -> Result<Value, String>;
    /// Idempotent: returns the existing `tuic_session_id` if the pane is
    /// already materialised.
    fn materialize_pane(
        &self,
        label: &str,
        pane_id: &str,
        cwd: Option<&str>,
    ) -> Result<String, String>;
    fn rename_pane(&self, label: &str, pane_id: &str, title: Option<&str>) -> Result<(), String>;
    fn kill_pane(&self, label: &str, pane_id: &str) -> Result<(), String>;
    /// `set-option ... window-style|pane-border-style|pane-active-border-style`.
    /// `value` is the RAW tmux option value (e.g. `bg=default,fg=blue`) —
    /// this crate has no dependency on the main app's color palette, so
    /// resolution happens app-side (`mcp_http::tmux_routes::resolve_tmux_color`).
    fn set_pane_accent_color(&self, label: &str, pane_id: &str, value: &str) -> Result<(), String>;
    /// `select-layout tiled`/`main-vertical`. The app resolves which of the
    /// window's panes are materialized and announces the arrangement
    /// itself — this call carries no pane list, only the target and layout
    /// name.
    fn request_window_layout(
        &self,
        label: &str,
        window_id: &str,
        layout: &str,
    ) -> Result<(), String>;

    /// The byte-identical legacy path: run a plain `tuic` command exactly as
    /// it behaves outside tmux mode (own printing, own error text).
    fn dispatch_legacy(&self, cmd: Command) -> Result<(), String>;
}

/// Talks to the real running instance over `ipc::` — the only
/// [`TuicBackend`] used outside tests.
pub(crate) struct IpcBackend;

impl TuicBackend for IpcBackend {
    fn is_running(&self) -> bool {
        crate::ipc::is_running()
    }

    fn write(&self, session_id: &str, data: &str) -> Result<(), String> {
        let body = serde_json::json!({ "data": data });
        let resp = crate::ipc::post(&format!("/sessions/{session_id}/write"), &body.to_string())
            .map_err(|e| e.to_string())?;
        if !resp.is_success() {
            return Err(format!("Failed to send keys: {}", resp.body));
        }
        Ok(())
    }

    fn get_topology(&self, label: &str) -> Result<Value, String> {
        if !self.is_running() {
            return Err("TUICommander is not running".to_string());
        }
        let resp = crate::ipc::get(&format!("/tmux/topology?label={}", crate::urlencod(label)))
            .map_err(|e| e.to_string())?;
        if !resp.is_success() {
            return Err(format!("Cannot fetch tmux topology: {}", resp.body));
        }
        resp.json().map_err(|e| e.to_string())
    }

    fn create_tmux_session(
        &self,
        label: &str,
        name: &str,
        window_name: Option<&str>,
        cwd: Option<&str>,
    ) -> Result<Value, String> {
        let body = serde_json::json!({
            "label": label, "name": name, "window_name": window_name, "cwd": cwd,
        });
        let resp =
            crate::ipc::post("/tmux/sessions", &body.to_string()).map_err(|e| e.to_string())?;
        if !resp.is_success() {
            return Err(format!("Failed to create tmux session: {}", resp.body));
        }
        resp.json().map_err(|e| e.to_string())
    }

    fn delete_tmux_session(&self, label: &str, session_id: &str) -> Result<(), String> {
        let resp = crate::ipc::delete(&format!(
            "/tmux/sessions/{session_id}?label={}",
            crate::urlencod(label)
        ))
        .map_err(|e| e.to_string())?;
        if !resp.is_success() {
            return Err(format!("Failed to kill tmux session: {}", resp.body));
        }
        Ok(())
    }

    fn create_tmux_window(
        &self,
        label: &str,
        session_id: &str,
        name: Option<&str>,
        cwd: Option<&str>,
    ) -> Result<Value, String> {
        let body = serde_json::json!({
            "label": label, "session_id": session_id, "name": name, "cwd": cwd,
        });
        let resp =
            crate::ipc::post("/tmux/windows", &body.to_string()).map_err(|e| e.to_string())?;
        if !resp.is_success() {
            return Err(format!("Failed to create tmux window: {}", resp.body));
        }
        resp.json().map_err(|e| e.to_string())
    }

    fn create_tmux_pane(
        &self,
        label: &str,
        window_id: &str,
        cwd: Option<&str>,
    ) -> Result<Value, String> {
        let body = serde_json::json!({ "label": label, "window_id": window_id, "cwd": cwd });
        let resp = crate::ipc::post("/tmux/panes", &body.to_string()).map_err(|e| e.to_string())?;
        if !resp.is_success() {
            return Err(format!("Failed to create tmux pane: {}", resp.body));
        }
        resp.json().map_err(|e| e.to_string())
    }

    fn materialize_pane(
        &self,
        label: &str,
        pane_id: &str,
        cwd: Option<&str>,
    ) -> Result<String, String> {
        let body = serde_json::json!({ "cwd": cwd });
        // The server-side handler blocks for its own shell-readiness gate,
        // bounded at `PANE_READY_TIMEOUT_MS` (5s, `tmux_routes.rs`) — the
        // default 3s client socket timeout is shorter than that, so a
        // legitimately slow (not hung) shell would otherwise time out
        // client-side and report failure even though the server would have
        // returned `Ok` moments later. Must stay comfortably above the
        // server's own bound, with real margin, per this codebase's own
        // "outer bound strictly larger than every bound inside it" rule.
        let resp = crate::ipc::post_with_timeout(
            &format!(
                "/tmux/panes/{}/materialize?label={}",
                crate::urlencod(pane_id),
                crate::urlencod(label)
            ),
            &body.to_string(),
            std::time::Duration::from_secs(8),
        )
        .map_err(|e| e.to_string())?;
        if !resp.is_success() {
            return Err(format!("Failed to materialize pane: {}", resp.body));
        }
        let v = resp.json().map_err(|e| e.to_string())?;
        v["tuic_session_id"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| "Server returned no tuic_session_id".to_string())
    }

    fn rename_pane(&self, label: &str, pane_id: &str, title: Option<&str>) -> Result<(), String> {
        let body = serde_json::json!({ "title": title });
        let resp = crate::ipc::put(
            &format!(
                "/tmux/panes/{}?label={}",
                crate::urlencod(pane_id),
                crate::urlencod(label)
            ),
            &body.to_string(),
        )
        .map_err(|e| e.to_string())?;
        if !resp.is_success() {
            return Err(format!("Failed to rename pane: {}", resp.body));
        }
        Ok(())
    }

    fn kill_pane(&self, label: &str, pane_id: &str) -> Result<(), String> {
        let resp = crate::ipc::delete(&format!(
            "/tmux/panes/{}?label={}",
            crate::urlencod(pane_id),
            crate::urlencod(label)
        ))
        .map_err(|e| e.to_string())?;
        if !resp.is_success() {
            return Err(format!("Failed to kill pane: {}", resp.body));
        }
        Ok(())
    }

    fn set_pane_accent_color(&self, label: &str, pane_id: &str, value: &str) -> Result<(), String> {
        let body = serde_json::json!({ "value": value });
        let resp = crate::ipc::put(
            &format!(
                "/tmux/panes/{}/accent-color?label={}",
                crate::urlencod(pane_id),
                crate::urlencod(label)
            ),
            &body.to_string(),
        )
        .map_err(|e| e.to_string())?;
        if !resp.is_success() {
            return Err(format!("Failed to set pane accent color: {}", resp.body));
        }
        Ok(())
    }

    fn request_window_layout(
        &self,
        label: &str,
        window_id: &str,
        layout: &str,
    ) -> Result<(), String> {
        let body = serde_json::json!({ "label": label, "layout": layout });
        let resp = crate::ipc::post(
            &format!("/tmux/windows/{window_id}/layout"),
            &body.to_string(),
        )
        .map_err(|e| e.to_string())?;
        if !resp.is_success() {
            return Err(format!("Failed to request window layout: {}", resp.body));
        }
        Ok(())
    }

    fn dispatch_legacy(&self, cmd: Command) -> Result<(), String> {
        crate::dispatch(cmd)
    }
}

fn ctx_for(session_name: Option<&str>, window: Option<&Value>, pane: Option<&Value>) -> FormatCtx {
    FormatCtx {
        session_name: session_name.map(String::from),
        window_id: window.and_then(|w| w["id"].as_str()).map(String::from),
        window_name: window.and_then(|w| w["name"].as_str()).map(String::from),
        pane_id: pane.and_then(|p| p["id"].as_str()).map(String::from),
        pane_title: pane.and_then(|p| p["title"].as_str()).map(String::from),
    }
}

/// Resolve `target` (already tmux-topology-space) to a materialised TUIC
/// session id, materialising a still-virtual pane on demand. Returns `None`
/// only when the target doesn't resolve to any pane in this label's
/// topology at all — callers fall back to the legacy name/uuid resolver.
fn resolve_and_materialize(
    backend: &dyn TuicBackend,
    label: &str,
    topology: &Value,
    target_str: &str,
) -> Option<Result<String, String>> {
    let target = parse_target(target_str);
    let pane = resolve_pane(topology, &target)?;
    let pane_id = pane["id"].as_str()?.to_string();
    if let Some(id) = pane["tuic_session_id"].as_str() {
        return Some(Ok(id.to_string()));
    }
    let cwd = pane["cwd"].as_str().map(String::from);
    Some(backend.materialize_pane(label, &pane_id, cwd.as_deref()))
}

/// Fetch topology fresh and resolve `target` down to a pane id, or a
/// ready-to-return `Outcome::err`. Shared by `SelectPane` and `KillPane`,
/// which differ only in what they do once they have the id.
fn resolve_pane_id_or_error(
    backend: &dyn TuicBackend,
    label: &str,
    target: &str,
) -> Result<String, Outcome> {
    let topology = backend.get_topology(label).map_err(Outcome::err)?;
    let parsed = parse_target(target);
    resolve_pane(&topology, &parsed)
        .and_then(|p| p["id"].as_str())
        .map(String::from)
        .ok_or_else(|| Outcome::err(format!("no pane found matching '{target}'")))
}

pub(crate) fn execute(
    op: TmuxOp,
    globals: &super::args::GlobalOpts,
    backend: &dyn TuicBackend,
) -> Outcome {
    let label = globals.label();
    match op {
        TmuxOp::Version => Outcome::ok_line("tmux 3.4"),
        TmuxOp::Bare => Outcome::from_dispatch(backend.dispatch_legacy(Command::New {
            name: None,
            repo: None,
        })),
        TmuxOp::ListSessions => {
            Outcome::from_dispatch(backend.dispatch_legacy(Command::Ls { json: false }))
        }
        TmuxOp::CapturePane { target, lines } => {
            Outcome::from_dispatch(backend.dispatch_legacy(Command::Capture {
                target: target.unwrap_or_default(),
                format: "text".to_string(),
                lines,
            }))
        }
        TmuxOp::AttachSession => {
            let _ = crate::open_deep_link("tuic://focus");
            Outcome::ok()
        }
        TmuxOp::KillServer => {
            // Scoped to THIS server (this `-L`/`-S` label) only — matching
            // real tmux semantics, where `tmux -L a kill-server` never
            // touches a session on `-L b`, let alone the user's own
            // manually-opened tabs. The previous implementation closed
            // EVERY TUIC session app-wide via an unscoped `list_sessions()`
            // and wiped topology for every label: a swarm under one label
            // running routine `kill-server` cleanup could nuke a
            // concurrent swarm's live sessions, or the user's own tabs.
            if let Ok(topology) = backend.get_topology(&label)
                && let Some(sessions) = topology["sessions"].as_array()
            {
                let session_ids: Vec<String> = sessions
                    .iter()
                    .filter_map(|s| s["id"].as_str().map(String::from))
                    .collect();
                for sid in session_ids {
                    let _ = backend.delete_tmux_session(&label, &sid);
                }
            }
            Outcome::ok()
        }
        TmuxOp::ResizePane { target, x, y } => {
            if x.is_none() && y.is_none() {
                // `-Z` alone (or no dimensions at all): a real resize needs
                // real numbers, and layout doesn't apply to TUIC tabs anyway.
                return Outcome::ok();
            }
            let size = format!("{}x{}", x.unwrap_or(80), y.unwrap_or(24));
            Outcome::from_dispatch(backend.dispatch_legacy(Command::Resize {
                target: target.unwrap_or_default(),
                size,
            }))
        }
        TmuxOp::HasSession { target } => {
            let target = target.unwrap_or_default();
            if target.is_empty() {
                return Outcome::exit_only(1);
            }
            if let Ok(topology) = backend.get_topology(&label) {
                let parsed = parse_target(&target);
                if resolve_session(&topology, &parsed).is_some() {
                    return Outcome::exit_only(0);
                }
            }
            match crate::resolve_session_id(&target) {
                Ok(_) => Outcome::exit_only(0),
                Err(_) => Outcome::exit_only(1),
            }
        }
        TmuxOp::KillSession { target } => {
            let target = target.unwrap_or_default();
            if target.is_empty() {
                return Outcome::err("missing -t");
            }
            let topology = backend.get_topology(&label).ok();
            let session_id = topology.as_ref().and_then(|t| {
                let parsed = parse_target(&target);
                resolve_session(t, &parsed)
                    .and_then(|s| s["id"].as_str())
                    .map(String::from)
            });
            if let Some(id) = session_id {
                return match backend.delete_tmux_session(&label, &id) {
                    Ok(()) => Outcome::ok(),
                    Err(e) => Outcome::err(e),
                };
            }
            Outcome::from_dispatch(backend.dispatch_legacy(Command::Kill { target }))
        }
        TmuxOp::SendKeys { target, keys } => {
            let target = target.unwrap_or_default();
            if let Ok(topology) = backend.get_topology(&label)
                && let Some(result) = resolve_and_materialize(backend, &label, &topology, &target)
            {
                return match result.and_then(|id| backend.write(&id, &crate::translate_keys(&keys)))
                {
                    Ok(()) => Outcome::ok(),
                    Err(e) => Outcome::err(e),
                };
            }
            Outcome::from_dispatch(backend.dispatch_legacy(Command::Send { target, keys }))
        }
        TmuxOp::DisplayMessage { target, format } => {
            let topology = backend.get_topology(&label).unwrap_or(Value::Null);
            let parsed = target.as_deref().map(parse_target);
            let (session_name, window, pane) = match &parsed {
                Some(t) => (
                    resolve_session(&topology, t).and_then(|s| s["name"].as_str()),
                    resolve_window(&topology, t),
                    resolve_pane(&topology, t),
                ),
                None => (
                    topology["sessions"]
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(|s| s["name"].as_str()),
                    None,
                    None,
                ),
            };
            let ctx = ctx_for(session_name, window, pane);
            Outcome::ok_line(render_format(&format, &ctx))
        }
        TmuxOp::SelectPane { target, title } => {
            let target = target.unwrap_or_default();
            let pane_id = match resolve_pane_id_or_error(backend, &label, &target) {
                Ok(id) => id,
                Err(outcome) => return outcome,
            };
            match backend.rename_pane(&label, &pane_id, title.as_deref()) {
                Ok(()) => Outcome::ok(),
                Err(e) => Outcome::err(e),
            }
        }
        TmuxOp::ListPanes { target, format } => {
            let topology = match backend.get_topology(&label) {
                Ok(t) => t,
                Err(e) => return Outcome::err(e),
            };
            let parsed = target.as_deref().map(parse_target);
            let window = match &parsed {
                Some(t) => resolve_window(&topology, t),
                None => topology["sessions"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|s| s["windows"].as_array())
                    .and_then(|w| w.first()),
            };
            let Some(window) = window else {
                return Outcome::err("no such window");
            };
            let fmt = format.as_deref().unwrap_or("#{pane_id}");
            // Resolve the OWNING session for this window specifically — not
            // just the topology's first session, which is wrong the moment a
            // label has more than one (a latent bug caught by testing this
            // arm directly rather than only through the single-session
            // swarm flow, where it happened to be invisible).
            let session_name = match &parsed {
                Some(t) => resolve_session(&topology, t).and_then(|s| s["name"].as_str()),
                None => topology["sessions"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|s| s["name"].as_str()),
            };
            let lines = window["panes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|pane| render_format(fmt, &ctx_for(session_name, Some(window), Some(pane))))
                .collect();
            Outcome {
                stdout: lines,
                stderr: vec![],
                exit: 0,
            }
        }
        TmuxOp::ListWindows { target, format } => {
            let topology = match backend.get_topology(&label) {
                Ok(t) => t,
                Err(e) => return Outcome::err(e),
            };
            let session = match target.as_deref().map(parse_target) {
                Some(t) => resolve_session(&topology, &t),
                None => topology["sessions"].as_array().and_then(|a| a.first()),
            };
            let Some(session) = session else {
                return Outcome::err("no such session");
            };
            let fmt = format.as_deref().unwrap_or("#{window_name}");
            let session_name = session["name"].as_str();
            let lines = session["windows"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|window| render_format(fmt, &ctx_for(session_name, Some(window), None)))
                .collect();
            Outcome {
                stdout: lines,
                stderr: vec![],
                exit: 0,
            }
        }
        TmuxOp::KillPane { target } => {
            let target = target.unwrap_or_default();
            let pane_id = match resolve_pane_id_or_error(backend, &label, &target) {
                Ok(id) => id,
                Err(outcome) => return outcome,
            };
            match backend.kill_pane(&label, &pane_id) {
                Ok(()) => Outcome::ok(),
                Err(e) => Outcome::err(e),
            }
        }
        TmuxOp::NewSession {
            session_name,
            window_name,
            cwd,
            detached: _,
            print,
            format,
            command: _,
        } => {
            if !print {
                // Legacy path: byte-identical to today's behavior for
                // `tuic alias` users who never pass -P (which Claude Code's
                // swarm backend always does). `repo` is this path's cwd.
                // Deliberately NOT resolve_cwd()'d — this path's existing
                // `None`-means-app-default behavior predates the swarm
                // path and must stay byte-identical for it.
                return Outcome::from_dispatch(backend.dispatch_legacy(Command::New {
                    name: session_name,
                    repo: cwd,
                }));
            }
            let name = session_name.unwrap_or_else(|| "0".to_string());
            let cwd = resolve_cwd(cwd, None);
            let created = match backend.create_tmux_session(
                &label,
                &name,
                window_name.as_deref(),
                cwd.as_deref(),
            ) {
                Ok(v) => v,
                Err(e) => return Outcome::err(e),
            };
            let ctx = FormatCtx {
                session_name: Some(name),
                window_id: created["window_id"].as_str().map(String::from),
                window_name,
                pane_id: created["pane_id"].as_str().map(String::from),
                pane_title: None,
            };
            Outcome::ok_line(render_format(
                format.as_deref().unwrap_or("#{session_name}"),
                &ctx,
            ))
        }
        TmuxOp::NewWindow {
            target,
            name,
            cwd,
            print,
            format,
            command: _,
        } => {
            let topology = match backend.get_topology(&label) {
                Ok(t) => t,
                Err(e) => return Outcome::err(e),
            };
            let session_id = target
                .as_deref()
                .map(parse_target)
                .and_then(|t| resolve_session(&topology, &t))
                .and_then(|s| s["id"].as_str());
            let Some(session_id) = session_id else {
                return Outcome::err(format!(
                    "no such session: '{}'",
                    target.as_deref().unwrap_or("")
                ));
            };
            let inherited = topology_cwd_for_session(&topology, session_id);
            let cwd = resolve_cwd(cwd, inherited);
            let created = match backend.create_tmux_window(
                &label,
                session_id,
                name.as_deref(),
                cwd.as_deref(),
            ) {
                Ok(v) => v,
                Err(e) => return Outcome::err(e),
            };
            if !print {
                return Outcome::ok();
            }
            let ctx = FormatCtx {
                session_name: None,
                window_id: created["window_id"].as_str().map(String::from),
                window_name: name,
                pane_id: created["pane_id"].as_str().map(String::from),
                pane_title: None,
            };
            Outcome::ok_line(render_format(
                format.as_deref().unwrap_or("#{window_id}"),
                &ctx,
            ))
        }
        TmuxOp::SplitWindow {
            target,
            cwd,
            print,
            format,
            command: _,
        } => {
            let topology = match backend.get_topology(&label) {
                Ok(t) => t,
                Err(e) => return Outcome::err(e),
            };
            let window_id = target
                .as_deref()
                .map(parse_target)
                .and_then(|t| resolve_window(&topology, &t))
                .and_then(|w| w["id"].as_str());
            let Some(window_id) = window_id else {
                return Outcome::err(format!(
                    "no such window: '{}'",
                    target.as_deref().unwrap_or("")
                ));
            };
            // The trailing command on the swarm path is always the
            // placeholder `cat` — the real command arrives later via
            // respawn-pane, so it is intentionally never written here.
            let inherited = topology_cwd_for_window(&topology, window_id);
            let cwd = resolve_cwd(cwd, inherited);
            let created = match backend.create_tmux_pane(&label, window_id, cwd.as_deref()) {
                Ok(v) => v,
                Err(e) => return Outcome::err(e),
            };
            if !print {
                return Outcome::ok();
            }
            let ctx = FormatCtx {
                session_name: None,
                window_id: Some(window_id.to_string()),
                window_name: None,
                pane_id: created["pane_id"].as_str().map(String::from),
                pane_title: None,
            };
            Outcome::ok_line(render_format(
                format.as_deref().unwrap_or("#{pane_id}"),
                &ctx,
            ))
        }
        TmuxOp::RespawnPane {
            target,
            kill,
            command,
        } => {
            let target = target.unwrap_or_default();
            let topology = match backend.get_topology(&label) {
                Ok(t) => t,
                Err(e) => return Outcome::err(e),
            };
            let parsed = parse_target(&target);
            let Some(pane) = resolve_pane(&topology, &parsed) else {
                return Outcome::err(format!("no pane found matching '{target}'"));
            };
            let pane_id = pane["id"].as_str().unwrap_or_default().to_string();
            let cwd = pane["cwd"].as_str().map(String::from);
            // Whether *this* pane already had a live PTY before this call —
            // e.g. one `split-window` eagerly materialised. Only in that case
            // is there an actual foreground process for `-k` to interrupt.
            let was_already_materialized = pane["tuic_session_id"].as_str().is_some();

            // `-k` interrupts whatever is running (real tmux's respawn-pane
            // always kills the pane's process first) before delivering the
            // command — never through `translate_keys`, since the command
            // line may itself contain a token spelled `Enter`/`Space`/`Tab`.
            //
            // The command text and the submitting Enter are sent as TWO
            // separate writes with a gap between them, matching
            // `cmd_agent`'s `AgentAction::Type` framing (`main.rs`) and the
            // app-side `write_agent_command_to_pty`/`INJECT_ENTER_GAP`
            // contract (`pty.rs`) — a raw-mode Ink TUI (the exact target of
            // this arm: a freshly spawned teammate agent) treats a combined
            // `text\r` in one write as an unsubmitted prefill, not a
            // submitted command. Sending it as one write here would leave
            // every teammate's launch command typed but never launched.
            //
            // Up to two attempts. Confirmed live (2026-09-03, see
            // tmux-shim.html): a pane materialised for the first time by
            // THIS call can have its freshly-spawned PTY die (app log:
            // "Session closed: process exited") within single-digit
            // milliseconds of creation — before this call ever writes to
            // it — so the very first write can fail against a session id
            // that is already gone. `materialize_pane` re-runs the app's
            // `reconcile()` against live sessions before its idempotent
            // early-return, so a dead id from attempt 1 is detected and
            // cleared before attempt 2 — the retry spawns a genuinely fresh
            // PTY rather than handing back the same dead id twice. Never
            // send `-k`'s kill byte on a pane THIS call just (re)materialised
            // — attempt 1 has nothing to interrupt if it was virtual before
            // this call, and attempt 2 is by construction a brand-new PTY
            // either way.
            let mut last_err = String::new();
            let mut delivered: Option<String> = None;
            for attempt in 0..2 {
                let tuic_id = match backend.materialize_pane(&label, &pane_id, cwd.as_deref()) {
                    Ok(id) => id,
                    Err(e) => return Outcome::err(e),
                };
                let mut payload = String::new();
                if kill && was_already_materialized && attempt == 0 {
                    payload.push('\x03');
                }
                payload.push_str(&command.join(" "));
                match backend.write(&tuic_id, &payload) {
                    Ok(()) => {
                        delivered = Some(tuic_id);
                        break;
                    }
                    Err(e) => last_err = e,
                }
            }
            let Some(tuic_id) = delivered else {
                return Outcome::err(last_err);
            };
            std::thread::sleep(std::time::Duration::from_millis(100));
            match backend.write(&tuic_id, "\r") {
                Ok(()) => Outcome::ok(),
                Err(e) => Outcome::err(e),
            }
        }
        TmuxOp::SetOption {
            target,
            scope_window: _,
            name,
            value,
        } => {
            // Only the three color options carry anything TUIC can act on
            // (see `resolve_tmux_color` app-side) — every other option name
            // (`remain-on-exit`, `pane-border-format`, `pane-border-status`,
            // a general `tuic alias` user's own arbitrary option) stays a
            // pure local no-op with NO backend call at all, exactly as it
            // behaved when this whole subcommand was a blanket `Noop`. This
            // matters, not just for efficiency: it means `set-option` for
            // an irrelevant option name still succeeds even when
            // TUICommander isn't running, same as before this variant
            // existed — only the three real color options now require a
            // live instance.
            const COLOR_OPTIONS: &[&str] = &[
                "window-style",
                "pane-border-style",
                "pane-active-border-style",
            ];
            if !COLOR_OPTIONS.contains(&name.as_str()) {
                return Outcome::ok();
            }
            let Some(target) = target else {
                // No -t on a color option (e.g. a hypothetical `-g` global
                // style): nothing to color. Same "harmless no-op" treatment.
                return Outcome::ok();
            };
            let pane_id = match resolve_pane_id_or_error(backend, &label, &target) {
                Ok(id) => id,
                Err(outcome) => return outcome,
            };
            match backend.set_pane_accent_color(&label, &pane_id, &value) {
                Ok(()) => Outcome::ok(),
                Err(e) => Outcome::err(e),
            }
        }
        TmuxOp::SelectLayout { target, layout } => {
            // Only `tiled` (the sole layout reachable from TUIC's
            // environment — see `tmux-swarm-shim.md`) triggers a real
            // request; `main-vertical` is built alongside for near-zero
            // extra cost even though it's unreachable today. Anything else
            // (or no layout name at all) is a pure local no-op, same
            // reasoning as `SetOption` above — no backend call, no
            // dependency on the app being reachable.
            let is_real_layout = matches!(layout.as_deref(), Some("tiled") | Some("main-vertical"));
            if !is_real_layout {
                return Outcome::ok();
            }
            let layout = layout.unwrap_or_default();
            let topology = match backend.get_topology(&label) {
                Ok(t) => t,
                Err(e) => return Outcome::err(e),
            };
            let window_id = target
                .as_deref()
                .map(parse_target)
                .and_then(|t| resolve_window(&topology, &t))
                .and_then(|w| w["id"].as_str());
            let Some(window_id) = window_id else {
                return Outcome::err(format!(
                    "no such window: '{}'",
                    target.as_deref().unwrap_or("")
                ));
            };
            match backend.request_window_layout(&label, window_id, &layout) {
                Ok(()) => Outcome::ok(),
                Err(e) => Outcome::err(e),
            }
        }
        TmuxOp::Noop(_) => Outcome::ok(),
        TmuxOp::Unknown(name, args) => {
            // Logged by the caller (tmux_compat's wrapper) before this
            // returns — see mod.rs. Still an honest failure: an unhandled
            // subcommand must not look like success to a caller parsing
            // stdout/exit code.
            let _ = args;
            Outcome::err(format!("unknown command '{name}'"))
        }
    }
}

#[cfg(test)]
mod resolve_cwd_tests {
    use super::{resolve_cwd, topology_cwd_for_session, topology_cwd_for_window};
    use serde_json::json;

    const ENV_VARS: [&str; 2] = ["TUIC_WORKTREE_PATH", "TUIC_MAIN_REPO_PATH"];

    /// Ensures `TUIC_WORKTREE_PATH`/`TUIC_MAIN_REPO_PATH` are absent for the
    /// duration of a test and restored afterward. This repo's own dev/agent
    /// shells are routinely launched FROM a real TUIC-spawned terminal,
    /// which sets these exact vars ambiently (see this codebase's own agent
    /// memory on ambient TUIC_* env leaking into tests) — without this
    /// guard, `resolve_cwd`'s new env-preferring behavior would make these
    /// tests non-deterministic depending on whatever shell `cargo test`/
    /// `cargo nextest run` happens to be invoked from. Also serializes
    /// mutation of this process-global state across tests, matching this
    /// file's existing `$TUIC_SOCKET` precedent (see
    /// `ipc_backend_pane_id_url_encoding_tests`).
    struct EnvVarGuard {
        saved: Vec<(&'static str, Option<String>)>,
    }

    impl EnvVarGuard {
        fn scrub() -> Self {
            let saved = ENV_VARS
                .iter()
                .map(|&k| (k, std::env::var(k).ok()))
                .collect();
            for &k in &ENV_VARS {
                unsafe { std::env::remove_var(k) };
            }
            Self { saved }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            for (k, v) in &self.saved {
                match v {
                    Some(val) => unsafe { std::env::set_var(k, val) },
                    None => unsafe { std::env::remove_var(k) },
                }
            }
        }
    }

    #[test]
    #[serial_test::serial]
    fn explicit_cwd_wins_over_the_fallback() {
        let _guard = EnvVarGuard::scrub();
        assert_eq!(
            resolve_cwd(Some("/explicit/path".to_string()), Some("/inherited/path")),
            Some("/explicit/path".to_string())
        );
    }

    #[test]
    #[serial_test::serial]
    fn inherited_cwd_wins_over_current_dir_when_no_explicit_value_and_no_env() {
        // Each `tmux` subcommand is its own OS subprocess, so a second
        // std::env::current_dir() read for split-window/new-window is a
        // genuinely independent read from the one new-session made earlier
        // — inheriting from topology instead guarantees every pane in one
        // swarm shares the exact cwd the first pane resolved, regardless.
        // No TUIC_* env is set here, so this exercises the same fallback
        // chain that existed before the env-var fix.
        let _guard = EnvVarGuard::scrub();
        assert_eq!(
            resolve_cwd(None, Some("/inherited/path")),
            Some("/inherited/path".to_string())
        );
    }

    #[test]
    #[serial_test::serial]
    fn absent_cwd_falls_back_to_the_calling_processs_own_current_dir() {
        // Real tmux defaults new-session/split-window/new-window's cwd to
        // the calling client's own directory when -c is absent — never a
        // fixed global default. Claude Code's swarm path never passes -c
        // (confirmed empirically), so without this fallback every
        // swarm-created pane's cwd silently defaults to whatever directory
        // the TUICommander app process happens to be running in instead —
        // which is how a live 2026-09-04 swarm spawned from a
        // `commerce-journal` session landed all 4 teammate panes under an
        // unrelated repo (`databricks-sql-cli`, whichever was active in the
        // sidebar at that moment). No TUIC_* env is set here (the final
        // fallback rung, reached only when neither an explicit cwd, an env
        // var, nor an inherited topology cwd exists).
        let _guard = EnvVarGuard::scrub();
        let expected = std::env::current_dir()
            .expect("current_dir must resolve in a test process")
            .to_string_lossy()
            .into_owned();
        assert_eq!(resolve_cwd(None, None), Some(expected));
    }

    #[test]
    #[serial_test::serial]
    fn env_repo_path_wins_over_inherited_and_current_dir() {
        let _guard = EnvVarGuard::scrub();
        unsafe { std::env::set_var("TUIC_MAIN_REPO_PATH", "/from/env") };
        assert_eq!(
            resolve_cwd(None, Some("/inherited/path")),
            Some("/from/env".to_string())
        );
    }

    #[test]
    #[serial_test::serial]
    fn env_worktree_path_wins_over_env_main_repo_path() {
        // A linked worktree's own TUIC_MAIN_REPO_PATH deliberately points at
        // the main checkout, not itself (script_env.rs) — TUIC_WORKTREE_PATH
        // is the one that names where this session actually lives.
        let _guard = EnvVarGuard::scrub();
        unsafe {
            std::env::set_var("TUIC_MAIN_REPO_PATH", "/main/checkout");
            std::env::set_var("TUIC_WORKTREE_PATH", "/worktree/root");
        }
        assert_eq!(resolve_cwd(None, None), Some("/worktree/root".to_string()));
    }

    #[test]
    #[serial_test::serial]
    fn explicit_cwd_still_wins_over_env_repo_path() {
        let _guard = EnvVarGuard::scrub();
        unsafe { std::env::set_var("TUIC_MAIN_REPO_PATH", "/from/env") };
        assert_eq!(
            resolve_cwd(Some("/explicit/path".to_string()), None),
            Some("/explicit/path".to_string())
        );
    }

    #[test]
    #[serial_test::serial]
    fn a_transient_cd_does_not_affect_the_env_derived_cwd() {
        // Regression test for the live 2026-09-23 bug: the calling agent's
        // shell had `cd`'d to an unrelated directory (running a one-off
        // script) at the exact moment Claude Code's agent-teams internals
        // fired `tmux new-session` — current_dir() faithfully reported that
        // unrelated directory, corrupting the whole swarm. The stable env
        // var must win regardless of where the live process cwd has
        // wandered off to.
        let _guard = EnvVarGuard::scrub();
        unsafe { std::env::set_var("TUIC_MAIN_REPO_PATH", "/the/real/repo") };
        let live_cwd = std::env::current_dir()
            .expect("current_dir must resolve in a test process")
            .to_string_lossy()
            .into_owned();
        assert_ne!(
            live_cwd, "/the/real/repo",
            "test process cwd must not coincidentally match, or this test proves nothing"
        );
        assert_eq!(resolve_cwd(None, None), Some("/the/real/repo".to_string()));
    }

    #[test]
    #[serial_test::serial]
    fn empty_env_var_is_treated_as_absent() {
        let _guard = EnvVarGuard::scrub();
        unsafe { std::env::set_var("TUIC_MAIN_REPO_PATH", "") };
        assert_eq!(
            resolve_cwd(None, Some("/inherited/path")),
            Some("/inherited/path".to_string())
        );
    }

    #[test]
    fn topology_cwd_for_session_finds_the_first_panes_cwd_across_windows() {
        let topology = json!({
            "sessions": [{
                "id": "$0",
                "windows": [
                    { "id": "@0", "panes": [{ "id": "%0", "cwd": null }] },
                    { "id": "@1", "panes": [{ "id": "%1", "cwd": "/repo/path" }] }
                ]
            }]
        });
        assert_eq!(
            topology_cwd_for_session(&topology, "$0"),
            Some("/repo/path")
        );
        assert_eq!(topology_cwd_for_session(&topology, "$nonexistent"), None);
    }

    #[test]
    fn topology_cwd_for_window_finds_the_first_panes_cwd() {
        let topology = json!({
            "sessions": [{
                "id": "$0",
                "windows": [
                    { "id": "@0", "panes": [{ "id": "%0", "cwd": "/repo/path" }, { "id": "%1", "cwd": "/other" }] }
                ]
            }]
        });
        assert_eq!(topology_cwd_for_window(&topology, "@0"), Some("/repo/path"));
        assert_eq!(topology_cwd_for_window(&topology, "@nonexistent"), None);
    }
}

/// Regression coverage for the missing-`urlencod(pane_id)` bug (fixed
/// alongside these tests): every `IpcBackend` method that puts a tmux pane
/// id (always `"%<number>"`) into a URL *path* segment must percent-encode
/// it first, or a two-digit id round-trips through axum's path decoder as a
/// single mangled byte and every one of these calls 404s with "pane not
/// found" — which is exactly what made `respawn-pane -k` (the call that
/// actually launches a swarm teammate's real `claude` process) silently
/// never start a single teammate past the 10th pane allocated in a label's
/// lifetime. `tmux::mod`'s `FakeBackend`-driven integration suite cannot
/// catch this class of bug at all: it never builds a URL, so it exercised
/// only `TuicBackend`'s pure dispatch logic, never `IpcBackend`'s actual HTTP
/// wire format. These tests are the only place that does.
///
/// Mirrors `ipc.rs`'s own `round_trip` test helper exactly — same
/// join-the-client-thread-don't-detach-the-server reasoning (see that
/// module's doc comment), same reason every test here needs
/// `#[serial_test::serial]`: `$TUIC_SOCKET` is a process-global env var.
#[cfg(all(test, unix))]
mod ipc_backend_pane_id_url_encoding_tests {
    use super::{IpcBackend, TuicBackend};
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;

    /// Runs `call` against a real `IpcBackend`, capturing the raw HTTP
    /// request it actually sends over the wire, and answers it with a fixed
    /// 200 response whose body is valid enough for every caller in this
    /// module (`{"ok":true,"tuic_session_id":"s1"}` covers both the
    /// `Result<(), _>` callers, which ignore the body, and
    /// `materialize_pane`, which needs `tuic_session_id`).
    fn capture_request<T: Send + 'static>(
        call: impl FnOnce() -> T + Send + 'static,
    ) -> (String, T) {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("mcp.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        unsafe {
            std::env::set_var("TUIC_SOCKET", &sock_path);
        }
        let client = std::thread::spawn(call);

        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap();
        let request = String::from_utf8_lossy(&buf[..n]).into_owned();

        let body = r#"{"ok":true,"tuic_session_id":"s1"}"#;
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .unwrap();
        stream.flush().unwrap();
        drop(stream);

        (request, client.join().unwrap())
    }

    /// The request line only — enough to assert method + exact path, without
    /// coupling the test to header ordering/casing.
    fn request_line(raw: &str) -> &str {
        raw.lines().next().unwrap_or_default()
    }

    #[test]
    #[serial_test::serial]
    fn materialize_pane_percent_encodes_a_two_digit_pane_id() {
        let (raw, result) =
            capture_request(|| IpcBackend.materialize_pane("claude-swarm-1", "%13", None));
        assert_eq!(
            request_line(&raw),
            "POST /tmux/panes/%2513/materialize?label=claude-swarm-1 HTTP/1.1"
        );
        assert_eq!(result, Ok("s1".to_string()));
    }

    #[test]
    #[serial_test::serial]
    fn rename_pane_percent_encodes_a_two_digit_pane_id() {
        let (raw, result) =
            capture_request(|| IpcBackend.rename_pane("claude-swarm-1", "%14", Some("title")));
        assert_eq!(
            request_line(&raw),
            "PUT /tmux/panes/%2514?label=claude-swarm-1 HTTP/1.1"
        );
        assert_eq!(result, Ok(()));
    }

    #[test]
    #[serial_test::serial]
    fn kill_pane_percent_encodes_a_two_digit_pane_id() {
        let (raw, result) = capture_request(|| IpcBackend.kill_pane("claude-swarm-1", "%15"));
        assert_eq!(
            request_line(&raw),
            "DELETE /tmux/panes/%2515?label=claude-swarm-1 HTTP/1.1"
        );
        assert_eq!(result, Ok(()));
    }

    #[test]
    #[serial_test::serial]
    fn set_pane_accent_color_percent_encodes_a_two_digit_pane_id() {
        let (raw, result) = capture_request(|| {
            IpcBackend.set_pane_accent_color("claude-swarm-1", "%16", "fg=yellow")
        });
        assert_eq!(
            request_line(&raw),
            "PUT /tmux/panes/%2516/accent-color?label=claude-swarm-1 HTTP/1.1"
        );
        assert_eq!(result, Ok(()));
    }
}
