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
        let resp = crate::ipc::post(
            &format!(
                "/tmux/panes/{pane_id}/materialize?label={}",
                crate::urlencod(label)
            ),
            &body.to_string(),
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
            &format!("/tmux/panes/{pane_id}?label={}", crate::urlencod(label)),
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
            "/tmux/panes/{pane_id}?label={}",
            crate::urlencod(label)
        ))
        .map_err(|e| e.to_string())?;
        if !resp.is_success() {
            return Err(format!("Failed to kill pane: {}", resp.body));
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
                return Outcome::from_dispatch(backend.dispatch_legacy(Command::New {
                    name: session_name,
                    repo: cwd,
                }));
            }
            let name = session_name.unwrap_or_else(|| "0".to_string());
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
            let tuic_id = match pane["tuic_session_id"].as_str() {
                Some(id) => id.to_string(),
                None => match backend.materialize_pane(&label, &pane_id, cwd.as_deref()) {
                    Ok(id) => id,
                    Err(e) => return Outcome::err(e),
                },
            };
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
            let mut payload = String::new();
            if kill {
                payload.push('\x03');
            }
            payload.push_str(&command.join(" "));
            if let Err(e) = backend.write(&tuic_id, &payload) {
                return Outcome::err(e);
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            match backend.write(&tuic_id, "\r") {
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
