//! App-side tmux pane topology, backing the `tuic`-as-`tmux` compatibility
//! shim's `split-window`/`new-window`/`list-panes`/etc. arms.
//!
//! The app owns the tmux object graph (sessions `$N` → windows `@N` → panes
//! `%N` → TUIC session uuid) and PTY materialisation; the CLI
//! (`tuic-cli/src/tmux/`) owns argv parsing, `#{…}` format rendering, target
//! resolution and exit codes — all pure, all unit-testable with no I/O. See
//! `tmux-swarm-shim.md` (repo root) for the feature this backs.
//!
//! Topology is partitioned by **tmux server label** — the `-L`/`-S` value a
//! `tuic`-as-`tmux` invocation passes ahead of the subcommand
//! (`"claude-swarm-<pid>"` on Claude Code's swarm path), or `"default"` when
//! neither is given. This is required, not a nicety: the *session* Claude
//! Code creates is always the fixed name `claude-swarm`, so two concurrent
//! Claude Code processes are only kept apart by this label.
//!
//! State is in-memory `AppState` only, by design: a swarm cannot outlive its
//! lead process, so losing topology on app restart is correct, not a bug —
//! every label simply starts empty again. What *is* handled is a
//! user-closes-the-tab-by-hand race: [`reconcile`] runs at the top of every
//! handler and reverts (not deletes) any pane whose backing TUIC session has
//! disappeared, so a later operation on that same tmux pane id re-materialises
//! a fresh PTY instead of 404ing forever.

use crate::AppState;
use crate::pty::resolve_shell;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize)]
pub(crate) struct TmuxTopology {
    next_session: u32,
    next_window: u32,
    next_pane: u32,
    sessions: Vec<TmuxSession>,
}

#[derive(Debug, Serialize)]
struct TmuxSession {
    id: String,   // "$0"
    name: String, // "claude-swarm" on the swarm path
    active_window: Option<String>,
    windows: Vec<TmuxWindow>,
}

#[derive(Debug, Serialize)]
struct TmuxWindow {
    id: String, // "@0"
    name: String,
    index: u32,
    active_pane: Option<String>,
    panes: Vec<TmuxPane>,
}

#[derive(Debug, Serialize)]
struct TmuxPane {
    id: String, // "%0"
    index: u32,
    title: Option<String>,
    cwd: Option<String>,
    /// `None` is the "virtual until first use" state: a pane id has been
    /// allocated (tmux always creates one on `new-session`/`new-window`) but
    /// no PTY has been spawned for it yet.
    tuic_session_id: Option<String>,
    /// Set by `set-option ... window-style|pane-border-style|
    /// pane-active-border-style` (Claude Code's per-teammate
    /// `--agent-color`) — a CSS-usable color string, already resolved by
    /// [`resolve_tmux_color`]. Like `title`, this can be set while the pane
    /// is still virtual; [`materialize`] applies it retroactively the
    /// moment a real session exists, mirroring `title`'s own handling.
    accent_color: Option<String>,
}

impl TmuxTopology {
    fn alloc_session(&mut self) -> String {
        let id = format!("${}", self.next_session);
        self.next_session += 1;
        id
    }
    fn alloc_window(&mut self) -> String {
        let id = format!("@{}", self.next_window);
        self.next_window += 1;
        id
    }
    fn alloc_pane(&mut self) -> String {
        let id = format!("%{}", self.next_pane);
        self.next_pane += 1;
        id
    }

    fn find_session_mut(&mut self, session_id: &str) -> Option<&mut TmuxSession> {
        self.sessions.iter_mut().find(|s| s.id == session_id)
    }

    fn find_window_mut(&mut self, window_id: &str) -> Option<&mut TmuxWindow> {
        self.sessions
            .iter_mut()
            .flat_map(|s| s.windows.iter_mut())
            .find(|w| w.id == window_id)
    }

    fn find_pane_mut(&mut self, pane_id: &str) -> Option<&mut TmuxPane> {
        self.sessions
            .iter_mut()
            .flat_map(|s| s.windows.iter_mut())
            .flat_map(|w| w.panes.iter_mut())
            .find(|p| p.id == pane_id)
    }

    fn find_pane(&self, pane_id: &str) -> Option<&TmuxPane> {
        self.sessions
            .iter()
            .flat_map(|s| s.windows.iter())
            .flat_map(|w| w.panes.iter())
            .find(|p| p.id == pane_id)
    }
}

/// Drop (revert to virtual) any pane whose recorded TUIC session id is no
/// longer present in `live`. Returns the reverted pane ids, for the caller to
/// log. Pure — no I/O, no lock, takes what it needs by value.
pub(crate) fn reconcile(topology: &mut TmuxTopology, live: &HashSet<String>) -> Vec<String> {
    let mut reverted = Vec::new();
    for session in &mut topology.sessions {
        for window in &mut session.windows {
            for pane in &mut window.panes {
                if let Some(id) = &pane.tuic_session_id
                    && !live.contains(id)
                {
                    reverted.push(pane.id.clone());
                    pane.tuic_session_id = None;
                }
            }
        }
    }
    reverted
}

fn live_session_ids(state: &AppState) -> HashSet<String> {
    state
        .session_maps
        .sessions
        .iter()
        .map(|e| e.key().clone())
        .collect()
}

/// Resolve a tmux `set-option` style value (e.g. `bg=default,fg=blue`, or a
/// bare `fg=colour208`) to a CSS-usable color string, or `None` if there is
/// no meaningful foreground color to apply (a `default`/`none` fg, or no
/// `fg=` component at all).
///
/// Claude Code's own color→tmux mapping (`TmuxBackend`'s `T()`, recovered
/// from the shipped binary) only ever emits 8 possible `fg=` values: the 6
/// real ANSI names below, plus two 256-color indices (`colour208`
/// "orange", `colour205` "pink"). The 6 names are already valid CSS
/// keywords; only the numeric indices need real resolution — reusing
/// [`crate::terminal_grid::xterm_color_rgb`] rather than a second palette.
pub(crate) fn resolve_tmux_color(value: &str) -> Option<String> {
    let fg = value
        .split(',')
        .find_map(|part| part.trim().strip_prefix("fg="))?
        .trim();
    if fg.is_empty() || fg.eq_ignore_ascii_case("default") || fg.eq_ignore_ascii_case("none") {
        return None;
    }
    const ANSI_NAMES: &[&str] = &[
        "red", "green", "yellow", "blue", "magenta", "cyan", "white", "black",
    ];
    if ANSI_NAMES.iter().any(|n| n.eq_ignore_ascii_case(fg)) {
        return Some(fg.to_ascii_lowercase());
    }
    if let Some(hex) = fg.strip_prefix('#') {
        // Real tmux's own hex-color form (`#rrggbb`, tmux >= 3.0). Validate
        // against CSS's own valid hex-color lengths (3/4/6/8 hex digits)
        // before passing through — this is the only value here that isn't
        // either a fixed ANSI keyword or something this function itself
        // computed from a validated `u8` index, so it's the one path that
        // must not trust its input shape.
        let is_valid_hex =
            matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit());
        return is_valid_hex.then(|| fg.to_string());
    }
    // Case-insensitive, matching the ANSI-name/`default`/`none` handling
    // above — real tmux itself treats color names case-insensitively.
    let lower = fg.to_ascii_lowercase();
    let index_str = lower
        .strip_prefix("colour")
        .or_else(|| lower.strip_prefix("color"))?;
    let index: u8 = index_str.parse().ok()?;
    let rgb = crate::terminal_grid::xterm_color_rgb(index);
    Some(format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b))
}

// ---------------------------------------------------------------------------
// Request/response shapes
// ---------------------------------------------------------------------------

/// Every route falls back to this label when the caller passes neither
/// `-L`/`-S` (i.e. a `tuic alias` invocation outside Claude Code's swarm
/// path, which always sets `-L`).
const DEFAULT_LABEL: &str = "default";

fn resolve_label(label: Option<String>) -> String {
    label.unwrap_or_else(|| DEFAULT_LABEL.to_string())
}

#[derive(Deserialize)]
pub(crate) struct LabelQuery {
    #[serde(default)]
    label: Option<String>,
}

fn label_of(q: &LabelQuery) -> String {
    resolve_label(q.label.clone())
}

#[derive(Deserialize)]
pub(crate) struct CreateTmuxSessionRequest {
    label: Option<String>,
    name: String,
    #[serde(default)]
    window_name: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct CreateTmuxWindowRequest {
    label: Option<String>,
    session_id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct CreateTmuxPaneRequest {
    label: Option<String>,
    window_id: String,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct MaterializePaneRequest {
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct RenamePaneRequest {
    title: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SetPaneAccentColorRequest {
    /// Raw `set-option` value (e.g. `bg=default,fg=blue`, or a bare
    /// `fg=colour208`) as sent by the tmux CLI — resolved server-side via
    /// [`resolve_tmux_color`], not a pre-resolved CSS color. The CLI crate
    /// cannot resolve it itself: it has no dependency on the main crate's
    /// `xterm_color_rgb` palette. `None`/empty clears the accent.
    value: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct RequestWindowLayoutRequest {
    label: Option<String>,
    layout: String,
}

fn not_found(what: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": format!("{what} not found")})),
    )
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub(crate) async fn get_topology(
    State(state): State<Arc<AppState>>,
    Query(q): Query<LabelQuery>,
) -> impl IntoResponse {
    let label = label_of(&q);
    let live = live_session_ids(&state);
    let mut entry = state.tmux_servers.entry(label).or_default();
    reconcile(&mut entry, &live);
    let body = serde_json::to_value(&*entry).unwrap_or(serde_json::Value::Null);
    (StatusCode::OK, Json(body))
}

pub(crate) async fn create_tmux_session(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateTmuxSessionRequest>,
) -> impl IntoResponse {
    let label = resolve_label(body.label);
    let live = live_session_ids(&state);
    let mut topology = state.tmux_servers.entry(label).or_default();
    reconcile(&mut topology, &live);

    let session_id = topology.alloc_session();
    let window_id = topology.alloc_window();
    let pane_id = topology.alloc_pane();
    topology.sessions.push(TmuxSession {
        id: session_id.clone(),
        name: body.name,
        active_window: Some(window_id.clone()),
        windows: vec![TmuxWindow {
            id: window_id.clone(),
            name: body.window_name.unwrap_or_else(|| "0".to_string()),
            index: 0,
            active_pane: Some(pane_id.clone()),
            panes: vec![TmuxPane {
                id: pane_id.clone(),
                index: 0,
                title: None,
                cwd: body.cwd,
                tuic_session_id: None, // virtual until first use
                accent_color: None,
            }],
        }],
    });

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "session_id": session_id,
            "window_id": window_id,
            "pane_id": pane_id,
        })),
    )
}

pub(crate) async fn delete_tmux_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(q): Query<LabelQuery>,
) -> impl IntoResponse {
    let label = label_of(&q);
    let live = live_session_ids(&state);
    let Some(mut topology) = state.tmux_servers.get_mut(&label) else {
        return not_found("session");
    };
    reconcile(&mut topology, &live);
    let Some(pos) = topology.sessions.iter().position(|s| s.id == session_id) else {
        return not_found("session");
    };
    let removed = topology.sessions.remove(pos);
    let pane_ids: Vec<String> = removed
        .windows
        .iter()
        .flat_map(|w| w.panes.iter())
        .filter_map(|p| p.tuic_session_id.clone())
        .collect();
    drop(topology);
    for id in pane_ids {
        let _ = super::session::close_session(State(state.clone()), Path(id)).await;
    }
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(crate) async fn create_tmux_window(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateTmuxWindowRequest>,
) -> impl IntoResponse {
    let label = resolve_label(body.label);
    let live = live_session_ids(&state);
    let Some(mut topology) = state.tmux_servers.get_mut(&label) else {
        return not_found("session").into_response();
    };
    reconcile(&mut topology, &live);
    let window_id = topology.alloc_window();
    let pane_id = topology.alloc_pane();
    {
        let Some(session) = topology.find_session_mut(&body.session_id) else {
            return not_found("session").into_response();
        };
        let index = session.windows.len() as u32;
        session.windows.push(TmuxWindow {
            id: window_id.clone(),
            name: body.name.unwrap_or_else(|| index.to_string()),
            index,
            active_pane: Some(pane_id.clone()),
            panes: vec![TmuxPane {
                id: pane_id.clone(),
                index: 0,
                title: None,
                cwd: body.cwd,
                tuic_session_id: None, // virtual until first use
                accent_color: None,
            }],
        });
        session.active_window = Some(window_id.clone());
    }
    (
        StatusCode::CREATED,
        Json(serde_json::json!({ "window_id": window_id, "pane_id": pane_id })),
    )
        .into_response()
}

pub(crate) async fn create_tmux_pane(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateTmuxPaneRequest>,
) -> impl IntoResponse {
    // No capacity pre-check here — `materialize()` below already enforces
    // `MAX_CONCURRENT_SESSIONS` before spawning (this used to check it
    // twice: once here, once again inside `materialize()`, on every
    // successful split-window). The pane is still inserted into topology
    // first (so `materialize()` can find it by id), so a failure there
    // rolls the insertion back explicitly, below — unlike the plain
    // session-create path, this one is not atomic for free.
    let label = resolve_label(body.label);
    let live = live_session_ids(&state);

    let (pane_id, previous_active_pane) = {
        let Some(mut topology) = state.tmux_servers.get_mut(&label) else {
            return not_found("window").into_response();
        };
        reconcile(&mut topology, &live);
        let pane_id = topology.alloc_pane();
        let Some(window) = topology.find_window_mut(&body.window_id) else {
            return not_found("window").into_response();
        };
        let index = window.panes.len() as u32;
        let previous_active_pane = window.active_pane.clone();
        window.panes.push(TmuxPane {
            id: pane_id.clone(),
            index,
            title: None,
            cwd: body.cwd.clone(),
            tuic_session_id: None,
            accent_color: None,
        });
        window.active_pane = Some(pane_id.clone());
        (pane_id, previous_active_pane)
    };

    // split-window materialises immediately, unlike new-session/new-window's
    // implicit initial pane (which stays virtual until first use) — this
    // pane is the one Claude Code's respawn-pane will actually target.
    match materialize(&state, &label, &pane_id, body.cwd).await {
        Ok(tuic_session_id) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "pane_id": pane_id, "tuic_session_id": tuic_session_id })),
        )
            .into_response(),
        Err(err) => {
            // Roll back the insertion above — a failed split-window must
            // not leave a permanent phantom pane (marked active, no less)
            // that nothing actually created.
            if let Some(mut topology) = state.tmux_servers.get_mut(&label)
                && let Some(window) = topology.find_window_mut(&body.window_id)
            {
                window.panes.retain(|p| p.id != pane_id);
                window.active_pane = previous_active_pane;
            }
            err.into_response()
        }
    }
}

async fn materialize(
    state: &Arc<AppState>,
    label: &str,
    pane_id: &str,
    cwd: Option<String>,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let live = live_session_ids(state);
    if let Some(mut topology) = state.tmux_servers.get_mut(label) {
        reconcile(&mut topology, &live);
    }
    if let Some(existing) = state
        .tmux_servers
        .get(label)
        .and_then(|t| t.find_pane(pane_id).and_then(|p| p.tuic_session_id.clone()))
    {
        return Ok(existing);
    }
    // `respawn-pane` — the only caller that ever materializes `new-session`'s
    // initial pane (it stays virtual until first use) — structurally never
    // sends a cwd of its own: real tmux's respawn-pane has no `-c` flag, so
    // `tuic-cli`'s executor correctly never fakes one. Without this fallback,
    // that first pane's PTY silently spawned with no cwd at all (inheriting
    // the TUICommander app process's own cwd) even though the CORRECT cwd
    // was already sitting right here in topology, recorded back when
    // `create_tmux_session`/`create_tmux_pane` first allocated this pane.
    // Found live 2026-09-23 alongside the client-side `resolve_cwd()` fix
    // (`tuic-cli/src/tmux/exec.rs`) — this is the server-side half of the
    // same gap.
    let cwd = cwd.or_else(|| {
        state
            .tmux_servers
            .get(label)
            .and_then(|t| t.find_pane(pane_id).and_then(|p| p.cwd.clone()))
    });
    if state.session_maps.sessions.len() >= crate::MAX_CONCURRENT_SESSIONS {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error": "Max concurrent sessions reached"})),
        ));
    }
    let shell = resolve_shell(None);
    let state_clone = state.clone();
    let spawn = tokio::task::spawn_blocking(move || {
        super::session::spawn_pty_session(
            state_clone,
            shell,
            cwd,
            24,
            80,
            None,
            super::session::RequestedIdentity::default(),
        )
    })
    .await
    .unwrap_or_else(|error| {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("PTY spawn task panicked: {error}")})),
        ))
    })?;
    // `select-pane -T` against a still-virtual pane (every swarm's first
    // teammate — `new-session`'s initial pane, always virtual until this
    // function's caller materializes it) can only record the title in
    // topology at the time (`rename_pane` below has no real session to
    // rename yet). Apply it now, retroactively, the moment a real session
    // exists — otherwise that title is silently lost forever and the tab
    // keeps its default name (found live, 2026-09-03).
    // Same reasoning applies to a color set on a still-virtual pane — see
    // `TmuxPane::accent_color`'s doc comment. Read both pending values in
    // the SAME lock acquisition as recording `tuic_session_id`, so there is
    // no window for a concurrent `set-option`/`select-pane -T` call between
    // the two — reading them separately could otherwise apply one but miss
    // a write that lands in between.
    let (pending_title, pending_accent_color) = if let Some(mut topology) =
        state.tmux_servers.get_mut(label)
        && let Some(pane) = topology.find_pane_mut(pane_id)
    {
        pane.tuic_session_id = Some(spawn.clone());
        (pane.title.clone(), pane.accent_color.clone())
    } else {
        (None, None)
    };
    if let Some(title) = pending_title {
        let _ = super::session::set_session_name(
            State(state.clone()),
            Path(spawn.clone()),
            Json(super::types::SetNameRequest {
                name: Some(title),
                is_custom: Some(true),
            }),
        )
        .await;
    }
    if let Some(color) = pending_accent_color {
        state.set_pty_accent_color(&spawn, Some(color));
    }
    Ok(spawn)
}

pub(crate) async fn materialize_pane(
    State(state): State<Arc<AppState>>,
    Path(pane_id): Path<String>,
    Query(q): Query<LabelQuery>,
    Json(body): Json<MaterializePaneRequest>,
) -> impl IntoResponse {
    let label = label_of(&q);
    let exists = state
        .tmux_servers
        .get(&label)
        .is_some_and(|t| t.find_pane(&pane_id).is_some());
    if !exists {
        return not_found("pane").into_response();
    }
    match materialize(&state, &label, &pane_id, body.cwd).await {
        Ok(tuic_session_id) => (
            StatusCode::OK,
            Json(serde_json::json!({ "tuic_session_id": tuic_session_id })),
        )
            .into_response(),
        Err(err) => err.into_response(),
    }
}

pub(crate) async fn rename_pane(
    State(state): State<Arc<AppState>>,
    Path(pane_id): Path<String>,
    Query(q): Query<LabelQuery>,
    Json(body): Json<RenamePaneRequest>,
) -> impl IntoResponse {
    let label = label_of(&q);
    let live = live_session_ids(&state);
    let tuic_session_id = {
        let Some(mut topology) = state.tmux_servers.get_mut(&label) else {
            return not_found("pane").into_response();
        };
        reconcile(&mut topology, &live);
        let Some(pane) = topology.find_pane_mut(&pane_id) else {
            return not_found("pane").into_response();
        };
        pane.title = body.title.clone();
        pane.tuic_session_id.clone()
    };
    if let Some(tuic_id) = tuic_session_id {
        let _ = super::session::set_session_name(
            State(state.clone()),
            Path(tuic_id),
            Json(super::types::SetNameRequest {
                name: body.title,
                is_custom: Some(true),
            }),
        )
        .await;
    }
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

/// `PUT /tmux/panes/{id}/accent-color` — the real half of the tmux
/// compatibility shim's `set-option ... window-style|pane-border-style|
/// pane-active-border-style` dispatch (`tuic-cli/src/tmux/exec.rs`).
/// Mirrors `rename_pane`'s shape exactly, including its still-virtual-pane
/// handling: a color set before the pane materializes is stashed on
/// `TmuxPane.accent_color` and applied retroactively by [`materialize`].
pub(crate) async fn set_pane_accent_color(
    State(state): State<Arc<AppState>>,
    Path(pane_id): Path<String>,
    Query(q): Query<LabelQuery>,
    Json(body): Json<SetPaneAccentColorRequest>,
) -> impl IntoResponse {
    let label = label_of(&q);
    let color = body.value.as_deref().and_then(resolve_tmux_color);
    let live = live_session_ids(&state);
    let tuic_session_id = {
        let Some(mut topology) = state.tmux_servers.get_mut(&label) else {
            return not_found("pane").into_response();
        };
        reconcile(&mut topology, &live);
        let Some(pane) = topology.find_pane_mut(&pane_id) else {
            return not_found("pane").into_response();
        };
        pane.accent_color = color.clone();
        pane.tuic_session_id.clone()
    };
    if let Some(tuic_id) = tuic_session_id {
        state.set_pty_accent_color(&tuic_id, color);
    }
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

pub(crate) async fn kill_pane(
    State(state): State<Arc<AppState>>,
    Path(pane_id): Path<String>,
    Query(q): Query<LabelQuery>,
) -> impl IntoResponse {
    let label = label_of(&q);
    let live = live_session_ids(&state);
    let tuic_session_id = {
        let Some(mut topology) = state.tmux_servers.get_mut(&label) else {
            return not_found("pane").into_response();
        };
        reconcile(&mut topology, &live);
        let mut removed = None;
        for session in &mut topology.sessions {
            for window in &mut session.windows {
                if let Some(pos) = window.panes.iter().position(|p| p.id == pane_id) {
                    removed = Some(window.panes.remove(pos));
                    // The killed pane may have been this window's
                    // active_pane, in which case that pointer is now
                    // stale — target resolution against the session/window
                    // (no explicit pane) would otherwise fail to find any
                    // pane at all even with others still live. Reassign to
                    // whatever remains, matching real tmux's "another pane
                    // becomes active" behavior on kill-pane.
                    if window.active_pane.as_deref() == Some(pane_id.as_str()) {
                        window.active_pane = window.panes.last().map(|p| p.id.clone());
                    }
                    break;
                }
            }
        }
        let Some(removed) = removed else {
            return not_found("pane").into_response();
        };
        removed.tuic_session_id
    };
    if let Some(id) = tuic_session_id {
        let _ = super::session::close_session(State(state.clone()), Path(id)).await;
    }
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

/// `POST /tmux/windows/{id}/layout` — the real half of the tmux
/// compatibility shim's `select-layout tiled`/`main-vertical` dispatch
/// (`tuic-cli/src/tmux/exec.rs`). The app is authoritative for topology, so
/// this resolves the window's *materialized* panes itself rather than
/// trusting a pane list from the caller — a still-virtual pane (no TUIC
/// session yet) is simply omitted, not represented as a gap, matching
/// `materialize`'s own "nothing to arrange yet" precedent for other
/// virtual-pane cases. The frontend owns actually arranging the split view
/// (`paneLayoutStore`) — this only announces the request.
pub(crate) async fn request_window_layout(
    State(state): State<Arc<AppState>>,
    Path(window_id): Path<String>,
    Json(body): Json<RequestWindowLayoutRequest>,
) -> impl IntoResponse {
    let label = resolve_label(body.label);
    let live = live_session_ids(&state);
    let session_ids: Vec<String> = {
        let Some(mut topology) = state.tmux_servers.get_mut(&label) else {
            return not_found("window").into_response();
        };
        reconcile(&mut topology, &live);
        let Some(window) = topology.find_window_mut(&window_id) else {
            return not_found("window").into_response();
        };
        window
            .panes
            .iter()
            .filter_map(|p| p.tuic_session_id.clone())
            .collect()
    };
    if session_ids.is_empty() {
        // Nothing materialized yet — nothing to arrange. Not an error: this
        // is the normal state right after `new-session`/`new-window`
        // creates a still-virtual initial pane.
        return (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response();
    }
    state.emit_pty_event(crate::state::AppEvent::TmuxWindowLayoutRequested {
        session_ids: session_ids.clone(),
        layout: body.layout.clone(),
    });
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        use tauri::Emitter;
        let _ = app.emit(
            "tmux-window-layout-requested",
            serde_json::json!({
                "session_ids": session_ids,
                "layout": body.layout,
            }),
        );
    }
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TmuxTopology {
        let mut t = TmuxTopology::default();
        let sid = t.alloc_session();
        let wid = t.alloc_window();
        let pid1 = t.alloc_pane();
        let pid2 = t.alloc_pane();
        t.sessions.push(TmuxSession {
            id: sid,
            name: "claude-swarm".to_string(),
            active_window: Some(wid.clone()),
            windows: vec![TmuxWindow {
                id: wid,
                name: "swarm-view".to_string(),
                index: 0,
                active_pane: Some(pid2.clone()),
                panes: vec![
                    TmuxPane {
                        id: pid1,
                        index: 0,
                        title: None,
                        cwd: None,
                        tuic_session_id: Some("uuid-live".to_string()),
                        accent_color: None,
                    },
                    TmuxPane {
                        id: pid2,
                        index: 1,
                        title: None,
                        cwd: None,
                        tuic_session_id: Some("uuid-dead".to_string()),
                        accent_color: None,
                    },
                ],
            }],
        });
        t
    }

    #[test]
    fn ids_are_monotone_and_never_reused() {
        let mut t = TmuxTopology::default();
        assert_eq!(t.alloc_session(), "$0");
        assert_eq!(t.alloc_session(), "$1");
        assert_eq!(t.alloc_window(), "@0");
        assert_eq!(t.alloc_pane(), "%0");
        assert_eq!(t.alloc_pane(), "%1");
    }

    #[test]
    fn reconcile_reverts_only_dead_panes() {
        let mut t = sample();
        let live: HashSet<String> = ["uuid-live".to_string()].into_iter().collect();
        let reverted = reconcile(&mut t, &live);
        assert_eq!(reverted, vec!["%1".to_string()]);
        assert_eq!(
            t.sessions[0].windows[0].panes[0].tuic_session_id,
            Some("uuid-live".to_string())
        );
        assert_eq!(t.sessions[0].windows[0].panes[1].tuic_session_id, None);
    }

    #[test]
    fn reconcile_is_a_noop_when_everything_is_live() {
        let mut t = sample();
        let live: HashSet<String> = ["uuid-live".to_string(), "uuid-dead".to_string()]
            .into_iter()
            .collect();
        assert!(reconcile(&mut t, &live).is_empty());
    }

    #[test]
    fn find_pane_locates_across_sessions_and_windows() {
        let t = sample();
        assert!(t.find_pane("%0").is_some());
        assert!(t.find_pane("%1").is_some());
        assert!(t.find_pane("%99").is_none());
    }

    fn label_query(label: &str) -> Query<LabelQuery> {
        Query(LabelQuery {
            label: Some(label.to_string()),
        })
    }

    #[tokio::test]
    async fn kill_pane_reassigns_a_stale_active_pane_pointer() {
        // Regression: killing the window's active pane left `active_pane`
        // pointing at an id that no longer exists in `panes` — later target
        // resolution against the session/window (no explicit -t pane)
        // then found nothing at all, even with another pane still live.
        let state = super::super::tests::test_state();
        let label = "test-kill-pane-active";

        let created = create_tmux_session(
            State(state.clone()),
            Json(CreateTmuxSessionRequest {
                label: Some(label.to_string()),
                name: "s".to_string(),
                window_name: None,
                cwd: None,
            }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(created.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let window_id = created["window_id"].as_str().unwrap().to_string();
        let initial_pane_id = created["pane_id"].as_str().unwrap().to_string();

        // split-window adds a second, real pane and makes IT active.
        let split = create_tmux_pane(
            State(state.clone()),
            Json(CreateTmuxPaneRequest {
                label: Some(label.to_string()),
                window_id,
                cwd: None,
            }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(split.into_body(), usize::MAX)
            .await
            .unwrap();
        let split: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let second_pane_id = split["pane_id"].as_str().unwrap().to_string();

        // Kill the now-active second pane.
        let _ = kill_pane(
            State(state.clone()),
            Path(second_pane_id),
            label_query(label),
        )
        .await;

        let topology = state.tmux_servers.get(label).unwrap();
        let window = &topology.sessions[0].windows[0];
        assert_eq!(
            window.active_pane.as_deref(),
            Some(initial_pane_id.as_str()),
            "active_pane must be reassigned to a pane that still exists, not left dangling"
        );
    }

    /// Regression for the tab-name-flapping bug: `select-pane -T` (this route)
    /// is called every time an agent's OSC/tmux status ticker repaints, often
    /// with an unchanged title. `rename_pane` call-through to
    /// `session::set_session_name` must not re-emit `session-renamed` when the
    /// title hasn't actually changed — see that function's own regression
    /// test (`set_session_name_skips_emit_when_unchanged`) for the full loop
    /// this was creating between the frontend's OSC-title sync and this route.
    #[tokio::test]
    async fn rename_pane_is_idempotent_and_only_emits_on_real_change() {
        let state = super::super::tests::test_state();
        let label = "test-rename-pane-idempotent";

        let created = create_tmux_session(
            State(state.clone()),
            Json(CreateTmuxSessionRequest {
                label: Some(label.to_string()),
                name: "s".to_string(),
                window_name: None,
                cwd: None,
            }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(created.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let pane_id = created["pane_id"].as_str().unwrap().to_string();

        // Materialize the pane to a real live session BEFORE renaming it —
        // this test is specifically about the already-materialized path.
        // `rename_pane` only calls through to `set_session_name` when the
        // pane already has a `tuic_session_id`; the opposite order (rename
        // while still virtual, materialize after) is covered by
        // `materialize_applies_a_title_recorded_while_the_pane_was_still_virtual`
        // below — that path used to lose the title silently.
        let materialized = materialize_pane(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
            Json(MaterializePaneRequest { cwd: None }),
        )
        .await
        .into_response();
        assert_eq!(materialized.status(), StatusCode::OK);

        let mut rx = state.event_bus.subscribe();

        let _ = rename_pane(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
            Json(RenamePaneRequest {
                title: Some("build".to_string()),
            }),
        )
        .await;
        match rx.try_recv() {
            Ok(crate::state::AppEvent::SessionRenamed {
                display_name,
                is_custom,
                ..
            }) => {
                assert_eq!(display_name, Some("build".to_string()));
                assert!(
                    is_custom,
                    "a tmux select-pane -T rename must mark the tab custom"
                );
            }
            other => panic!("expected SessionRenamed on the first real tmux rename, got {other:?}"),
        }

        // The exact scenario that caused the bug: the same title repeated
        // (a status ticker repainting, or the frontend echoing its own
        // OSC-title sync back through this route) must not re-emit.
        let _ = rename_pane(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
            Json(RenamePaneRequest {
                title: Some("build".to_string()),
            }),
        )
        .await;
        assert!(
            rx.try_recv().is_err(),
            "repeated select-pane -T with an unchanged title must not re-emit session-renamed"
        );

        // A genuinely different title still renames and emits.
        let _ = rename_pane(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
            Json(RenamePaneRequest {
                title: Some("test".to_string()),
            }),
        )
        .await;
        match rx.try_recv() {
            Ok(crate::state::AppEvent::SessionRenamed { display_name, .. }) => {
                assert_eq!(display_name, Some("test".to_string()));
            }
            other => panic!("expected SessionRenamed on a genuine tmux rename, got {other:?}"),
        }
    }

    /// Regression, found live 2026-09-03: `select-pane -T` against a pane
    /// that is STILL VIRTUAL (`new-session`'s initial pane — every swarm's
    /// first teammate, always) recorded the title in topology but never
    /// applied it to a real tab, and nothing re-applied it once the pane
    /// materialized later — the tab kept its default name forever. Confirmed
    /// live: `tauri-lister` (a `split-window` pane, materialized before its
    /// rename ran) got renamed correctly; `src-lister` (the `new-session`
    /// initial pane, still virtual at rename time) showed `"general-purpose"`
    /// instead. Fixed by having `materialize()` apply any pane title already
    /// recorded in topology the moment it spawns a real session.
    #[tokio::test]
    async fn materialize_applies_a_title_recorded_while_the_pane_was_still_virtual() {
        let state = super::super::tests::test_state();
        let label = "test-deferred-title-on-materialize";

        let created = create_tmux_session(
            State(state.clone()),
            Json(CreateTmuxSessionRequest {
                label: Some(label.to_string()),
                name: "s".to_string(),
                window_name: None,
                cwd: None,
            }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(created.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let pane_id = created["pane_id"].as_str().unwrap().to_string();

        // Rename it while it's still virtual — exactly `select-pane -t %0 -T
        // src-lister` before `respawn-pane` ever materializes %0.
        let _ = rename_pane(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
            Json(RenamePaneRequest {
                title: Some("src-lister".to_string()),
            }),
        )
        .await;
        {
            let topology = state.tmux_servers.get(label).unwrap();
            let pane = topology.find_pane(&pane_id).unwrap();
            assert_eq!(pane.title.as_deref(), Some("src-lister"));
            assert!(
                pane.tuic_session_id.is_none(),
                "pane must still be virtual at this point"
            );
        }

        let mut rx = state.event_bus.subscribe();

        // Materialize it — the real command-delivery path (respawn-pane).
        let materialized = materialize_pane(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
            Json(MaterializePaneRequest { cwd: None }),
        )
        .await
        .into_response();
        assert_eq!(materialized.status(), StatusCode::OK);
        let body = axum::body::to_bytes(materialized.into_body(), usize::MAX)
            .await
            .unwrap();
        let materialized: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tuic_session_id = materialized["tuic_session_id"].as_str().unwrap();

        // `spawn_pty_session` also broadcasts `SessionCreated` for the same
        // materialize call — drain events until the expected `SessionRenamed`
        // turns up (or the buffer is exhausted), rather than assuming it's
        // the very first message on the bus.
        let mut found = None;
        while let Ok(event) = rx.try_recv() {
            if let crate::state::AppEvent::SessionRenamed { .. } = &event {
                found = Some(event);
                break;
            }
        }
        match found {
            Some(crate::state::AppEvent::SessionRenamed {
                session_id,
                display_name,
                is_custom,
            }) => {
                assert_eq!(session_id, tuic_session_id);
                assert_eq!(display_name, Some("src-lister".to_string()));
                assert!(
                    is_custom,
                    "a deferred tmux select-pane -T rename must mark the tab custom"
                );
            }
            other => panic!(
                "expected SessionRenamed the moment the previously-virtual pane materialized, got {other:?}"
            ),
        }
    }

    /// Regression, found live 2026-09-23 alongside the client-side
    /// `resolve_cwd()` fix (`tuic-cli/src/tmux/exec.rs`): `respawn-pane` —
    /// the only caller that ever materializes `new-session`'s initial pane —
    /// structurally never sends a cwd of its own (real tmux's respawn-pane
    /// has no `-c` flag). Before this fix, `materialize()` passed that
    /// missing cwd straight through to `spawn_pty_session` as `None`,
    /// silently discarding the CORRECT cwd already recorded in topology at
    /// `create_tmux_session` time — the spawned PTY inherited the
    /// TUICommander app process's own cwd instead of the swarm's real repo.
    #[tokio::test]
    async fn materialize_falls_back_to_the_panes_own_recorded_cwd_when_the_request_has_none() {
        let state = super::super::tests::test_state();
        let label = "test-materialize-cwd-fallback";

        let created = create_tmux_session(
            State(state.clone()),
            Json(CreateTmuxSessionRequest {
                label: Some(label.to_string()),
                name: "s".to_string(),
                window_name: None,
                cwd: Some("/the/real/repo".to_string()),
            }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(created.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let pane_id = created["pane_id"].as_str().unwrap().to_string();
        {
            let topology = state.tmux_servers.get(label).unwrap();
            let pane = topology.find_pane(&pane_id).unwrap();
            assert!(
                pane.tuic_session_id.is_none(),
                "pane must still be virtual at this point"
            );
        }

        // Materialize it exactly the way `respawn-pane` does: no cwd of its
        // own, relying entirely on whatever `materialize()` can recover.
        let materialized = materialize_pane(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
            Json(MaterializePaneRequest { cwd: None }),
        )
        .await
        .into_response();
        assert_eq!(materialized.status(), StatusCode::OK);
        let body = axum::body::to_bytes(materialized.into_body(), usize::MAX)
            .await
            .unwrap();
        let materialized: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tuic_session_id = materialized["tuic_session_id"].as_str().unwrap();

        let spawned_cwd = state
            .session_maps
            .sessions
            .get(tuic_session_id)
            .expect("materialized session must exist")
            .lock()
            .cwd
            .clone();
        assert_eq!(
            spawned_cwd.as_deref(),
            Some("/the/real/repo"),
            "the spawned PTY must inherit the pane's own topology-recorded cwd, \
             not silently fall through to the app process's own cwd"
        );
    }

    /// Same gap, same fix, different creation path: `new-window`'s initial
    /// pane (`create_tmux_window`) is virtual until first use exactly like
    /// `new-session`'s — only `create_tmux_session` had a dedicated
    /// regression test for the topology-cwd fallback. A window's own `cwd`
    /// must win, not the session's (they can legitimately differ — a real
    /// swarm's `new-window` fires only when the `swarm-view` window is
    /// missing, which can happen well after the session's own cwd was
    /// recorded).
    #[tokio::test]
    async fn materialize_falls_back_to_the_windows_own_recorded_cwd_via_new_window() {
        let state = super::super::tests::test_state();
        let label = "test-materialize-cwd-fallback-window";

        let created_session = create_tmux_session(
            State(state.clone()),
            Json(CreateTmuxSessionRequest {
                label: Some(label.to_string()),
                name: "s".to_string(),
                window_name: None,
                cwd: Some("/session/repo".to_string()),
            }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(created_session.into_body(), usize::MAX)
            .await
            .unwrap();
        let created_session: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let session_id = created_session["session_id"].as_str().unwrap().to_string();

        let created_window = create_tmux_window(
            State(state.clone()),
            Json(CreateTmuxWindowRequest {
                label: Some(label.to_string()),
                session_id,
                name: None,
                cwd: Some("/window/repo".to_string()),
            }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(created_window.into_body(), usize::MAX)
            .await
            .unwrap();
        let created_window: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let pane_id = created_window["pane_id"].as_str().unwrap().to_string();
        {
            let topology = state.tmux_servers.get(label).unwrap();
            let pane = topology.find_pane(&pane_id).unwrap();
            assert!(
                pane.tuic_session_id.is_none(),
                "new-window's own pane must still be virtual at this point"
            );
        }

        // Materialize it exactly the way `respawn-pane` does: no cwd of its own.
        let materialized = materialize_pane(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
            Json(MaterializePaneRequest { cwd: None }),
        )
        .await
        .into_response();
        assert_eq!(materialized.status(), StatusCode::OK);
        let body = axum::body::to_bytes(materialized.into_body(), usize::MAX)
            .await
            .unwrap();
        let materialized: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tuic_session_id = materialized["tuic_session_id"].as_str().unwrap();

        let spawned_cwd = state
            .session_maps
            .sessions
            .get(tuic_session_id)
            .expect("materialized session must exist")
            .lock()
            .cwd
            .clone();
        assert_eq!(
            spawned_cwd.as_deref(),
            Some("/window/repo"),
            "a new-window pane must inherit ITS OWN recorded cwd, not the \
             session's, and not fall through to the app process's own cwd"
        );
    }

    /// Safety property for the topology-cwd fallback above: an explicit cwd
    /// on the materialize request must always win over whatever is already
    /// sitting in topology — the fallback exists only to cover the case
    /// where the request truly has none (`respawn-pane`'s structural gap),
    /// never to let a stale topology value override a caller who DID supply
    /// one.
    #[tokio::test]
    async fn materialize_prefers_an_explicit_cwd_over_the_panes_recorded_one() {
        let state = super::super::tests::test_state();
        let label = "test-materialize-explicit-cwd-wins";

        let created = create_tmux_session(
            State(state.clone()),
            Json(CreateTmuxSessionRequest {
                label: Some(label.to_string()),
                name: "s".to_string(),
                window_name: None,
                cwd: Some("/topology/repo".to_string()),
            }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(created.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let pane_id = created["pane_id"].as_str().unwrap().to_string();

        let materialized = materialize_pane(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
            Json(MaterializePaneRequest {
                cwd: Some("/explicit/repo".to_string()),
            }),
        )
        .await
        .into_response();
        assert_eq!(materialized.status(), StatusCode::OK);
        let body = axum::body::to_bytes(materialized.into_body(), usize::MAX)
            .await
            .unwrap();
        let materialized: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tuic_session_id = materialized["tuic_session_id"].as_str().unwrap();

        let spawned_cwd = state
            .session_maps
            .sessions
            .get(tuic_session_id)
            .expect("materialized session must exist")
            .lock()
            .cwd
            .clone();
        assert_eq!(
            spawned_cwd.as_deref(),
            Some("/explicit/repo"),
            "an explicit request cwd must never be silently overridden by a \
             stale topology-recorded value"
        );
    }

    /// A pane materialized with no prior `select-pane -T` call must not emit
    /// a spurious rename — `pending_title` is `None` and `materialize` must
    /// skip the `set_session_name` call entirely.
    #[tokio::test]
    async fn materialize_without_a_prior_rename_emits_nothing() {
        let state = super::super::tests::test_state();
        let label = "test-materialize-no-pending-title";

        let created = create_tmux_session(
            State(state.clone()),
            Json(CreateTmuxSessionRequest {
                label: Some(label.to_string()),
                name: "s".to_string(),
                window_name: None,
                cwd: None,
            }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(created.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let pane_id = created["pane_id"].as_str().unwrap().to_string();

        let mut rx = state.event_bus.subscribe();
        let materialized = materialize_pane(
            State(state.clone()),
            Path(pane_id),
            label_query(label),
            Json(MaterializePaneRequest { cwd: None }),
        )
        .await
        .into_response();
        assert_eq!(materialized.status(), StatusCode::OK);
        // `SessionCreated` is expected (the materialize itself); `SessionRenamed`
        // must not appear anywhere in the buffer.
        while let Ok(event) = rx.try_recv() {
            assert!(
                !matches!(event, crate::state::AppEvent::SessionRenamed { .. }),
                "no SessionRenamed without a prior select-pane -T, got {event:?}"
            );
        }
    }

    #[test]
    fn resolve_tmux_color_passes_through_real_ansi_names() {
        for name in ["red", "green", "yellow", "blue", "magenta", "cyan"] {
            let value = format!("bg=default,fg={name}");
            assert_eq!(resolve_tmux_color(&value), Some(name.to_string()));
        }
    }

    #[test]
    fn resolve_tmux_color_resolves_the_two_256_color_indices_claude_code_uses() {
        // "orange" and "pink" in Claude Code's own color→tmux mapping.
        assert_eq!(
            resolve_tmux_color("fg=colour208"),
            Some("#ff8700".to_string())
        );
        assert_eq!(
            resolve_tmux_color("fg=colour205"),
            Some("#ff5faf".to_string())
        );
    }

    #[test]
    fn resolve_tmux_color_accepts_a_bare_fg_with_no_bg() {
        assert_eq!(
            resolve_tmux_color("fg=blue"),
            Some("blue".to_string()),
            "pane-border-style/pane-active-border-style never carry a bg="
        );
    }

    #[test]
    fn resolve_tmux_color_passes_through_a_literal_hex() {
        assert_eq!(
            resolve_tmux_color("fg=#123456"),
            Some("#123456".to_string())
        );
    }

    #[test]
    fn resolve_tmux_color_clears_on_default_or_missing_fg() {
        assert_eq!(resolve_tmux_color("bg=default,fg=default"), None);
        assert_eq!(resolve_tmux_color("bg=default"), None, "no fg= at all");
        assert_eq!(resolve_tmux_color(""), None);
    }

    #[test]
    fn resolve_tmux_color_rejects_garbage_without_panicking() {
        assert_eq!(resolve_tmux_color("fg=colourNotANumber"), None);
        assert_eq!(resolve_tmux_color("fg=colour999"), None, "out of u8 range");
    }

    #[test]
    fn resolve_tmux_color_is_case_insensitive_for_the_colour_color_prefix() {
        // Real tmux treats color names case-insensitively; the ANSI-name and
        // default/none handling already was — the colour/color prefix match
        // wasn't, until this test (a general `tuic alias` user typing
        // `COLOUR208` got silently dropped instead of resolving).
        assert_eq!(
            resolve_tmux_color("fg=COLOUR208"),
            resolve_tmux_color("fg=colour208")
        );
        assert_eq!(
            resolve_tmux_color("fg=Color205"),
            resolve_tmux_color("fg=colour205")
        );
    }

    #[test]
    fn resolve_tmux_color_rejects_a_malformed_hex_value() {
        // Real tmux's hex form is #rrggbb; CSS also allows #rgb/#rgba/#rrggbbaa.
        // Anything else must not be passed through verbatim as a "valid" color.
        assert_eq!(resolve_tmux_color("fg=#zzzzzz"), None, "non-hex digits");
        assert_eq!(resolve_tmux_color("fg=#12345"), None, "invalid length (5)");
        assert_eq!(resolve_tmux_color("fg=#"), None, "empty after the hash");
    }

    #[test]
    fn resolve_tmux_color_accepts_every_valid_css_hex_length() {
        for hex in ["#abc", "#abcd", "#aabbcc", "#aabbccdd"] {
            assert_eq!(
                resolve_tmux_color(&format!("fg={hex}")),
                Some(hex.to_string())
            );
        }
    }

    /// Mirrors `rename_pane_is_idempotent_and_only_emits_on_real_change`:
    /// this is the real, live-observed order (split-window materializes
    /// eagerly, so every color `set-option` the swarm path ever sends
    /// targets an already-live pane).
    #[tokio::test]
    async fn set_pane_accent_color_404s_for_an_unknown_pane() {
        let state = super::super::tests::test_state();
        let resp = set_pane_accent_color(
            State(state.clone()),
            Path("%99".to_string()),
            label_query("test-accent-color-404"),
            Json(SetPaneAccentColorRequest {
                value: Some("fg=blue".to_string()),
            }),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn request_window_layout_404s_for_an_unknown_window() {
        let state = super::super::tests::test_state();
        let resp = request_window_layout(
            State(state.clone()),
            Path("@99".to_string()),
            Json(RequestWindowLayoutRequest {
                label: Some("test-window-layout-404".to_string()),
                layout: "tiled".to_string(),
            }),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn set_pane_accent_color_resolves_and_applies_when_already_materialized() {
        let state = super::super::tests::test_state();
        let label = "test-accent-color-materialized";

        let created = create_tmux_session(
            State(state.clone()),
            Json(CreateTmuxSessionRequest {
                label: Some(label.to_string()),
                name: "s".to_string(),
                window_name: None,
                cwd: None,
            }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(created.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let pane_id = created["pane_id"].as_str().unwrap().to_string();

        let materialized = materialize_pane(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
            Json(MaterializePaneRequest { cwd: None }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(materialized.into_body(), usize::MAX)
            .await
            .unwrap();
        let materialized: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tuic_session_id = materialized["tuic_session_id"]
            .as_str()
            .unwrap()
            .to_string();

        let mut rx = state.event_bus.subscribe();
        let resp = set_pane_accent_color(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
            Json(SetPaneAccentColorRequest {
                value: Some("bg=default,fg=blue".to_string()),
            }),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        match rx.try_recv() {
            Ok(crate::state::AppEvent::SessionAccentColorChanged { session_id, color }) => {
                assert_eq!(session_id, tuic_session_id);
                assert_eq!(color, Some("blue".to_string()));
            }
            other => panic!("expected SessionAccentColorChanged, got {other:?}"),
        }

        // window-style/pane-border-style/pane-active-border-style are always
        // sent together and resolve to the SAME color — the second and
        // third calls must not re-emit (same unchanged-value guard as
        // rename, protecting against the identical class of bug).
        let _ = set_pane_accent_color(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
            Json(SetPaneAccentColorRequest {
                value: Some("fg=blue".to_string()),
            }),
        )
        .await;
        assert!(
            rx.try_recv().is_err(),
            "the same resolved color from a sibling set-option call must not re-emit"
        );
    }

    /// Mirrors `materialize_applies_a_title_recorded_while_the_pane_was_still_virtual`
    /// — defensive/forward-compat coverage: the live swarm flow never colors
    /// a still-virtual pane in practice (split-window always materializes
    /// eagerly before any set-option lands), but `TmuxPane::accent_color`
    /// makes the same "recorded while virtual, applied on materialize"
    /// promise as `title` and must be tested the same way.
    #[tokio::test]
    async fn materialize_applies_an_accent_color_recorded_while_the_pane_was_still_virtual() {
        let state = super::super::tests::test_state();
        let label = "test-deferred-accent-color-on-materialize";

        let created = create_tmux_session(
            State(state.clone()),
            Json(CreateTmuxSessionRequest {
                label: Some(label.to_string()),
                name: "s".to_string(),
                window_name: None,
                cwd: None,
            }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(created.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let pane_id = created["pane_id"].as_str().unwrap().to_string();

        let _ = set_pane_accent_color(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
            Json(SetPaneAccentColorRequest {
                value: Some("fg=green".to_string()),
            }),
        )
        .await;
        {
            let topology = state.tmux_servers.get(label).unwrap();
            let pane = topology.find_pane(&pane_id).unwrap();
            assert_eq!(pane.accent_color.as_deref(), Some("green"));
            assert!(
                pane.tuic_session_id.is_none(),
                "pane must still be virtual at this point"
            );
        }

        let mut rx = state.event_bus.subscribe();
        let materialized = materialize_pane(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
            Json(MaterializePaneRequest { cwd: None }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(materialized.into_body(), usize::MAX)
            .await
            .unwrap();
        let materialized: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tuic_session_id = materialized["tuic_session_id"].as_str().unwrap();

        let mut found = None;
        while let Ok(event) = rx.try_recv() {
            if let crate::state::AppEvent::SessionAccentColorChanged { .. } = &event {
                found = Some(event);
                break;
            }
        }
        match found {
            Some(crate::state::AppEvent::SessionAccentColorChanged { session_id, color }) => {
                assert_eq!(session_id, tuic_session_id);
                assert_eq!(color, Some("green".to_string()));
            }
            other => panic!(
                "expected SessionAccentColorChanged the moment the previously-virtual pane materialized, got {other:?}"
            ),
        }
    }

    #[tokio::test]
    async fn request_window_layout_emits_only_materialized_session_ids_in_pane_order() {
        let state = super::super::tests::test_state();
        let label = "test-window-layout-materialized-only";

        let created = create_tmux_session(
            State(state.clone()),
            Json(CreateTmuxSessionRequest {
                label: Some(label.to_string()),
                name: "s".to_string(),
                window_name: None,
                cwd: None,
            }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(created.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let window_id = created["window_id"].as_str().unwrap().to_string();
        let first_pane_id = created["pane_id"].as_str().unwrap().to_string();

        // Materialize the first (initial) pane.
        let materialized = materialize_pane(
            State(state.clone()),
            Path(first_pane_id.clone()),
            label_query(label),
            Json(MaterializePaneRequest { cwd: None }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(materialized.into_body(), usize::MAX)
            .await
            .unwrap();
        let first_tuic_id =
            serde_json::from_slice::<serde_json::Value>(&body).unwrap()["tuic_session_id"]
                .as_str()
                .unwrap()
                .to_string();

        // split-window materializes eagerly, adding a second real pane.
        let split = create_tmux_pane(
            State(state.clone()),
            Json(CreateTmuxPaneRequest {
                label: Some(label.to_string()),
                window_id: window_id.clone(),
                cwd: None,
            }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(split.into_body(), usize::MAX)
            .await
            .unwrap();
        let second_tuic_id =
            serde_json::from_slice::<serde_json::Value>(&body).unwrap()["tuic_session_id"]
                .as_str()
                .unwrap()
                .to_string();

        // A third pane, allocated but never materialized — must be
        // reflected as an omission, not a gap/null in the emitted list.
        let mut topology = state.tmux_servers.get_mut(label).unwrap();
        let virtual_pane_id = topology.alloc_pane();
        topology
            .find_window_mut(&window_id)
            .unwrap()
            .panes
            .push(TmuxPane {
                id: virtual_pane_id,
                index: 2,
                title: None,
                cwd: None,
                tuic_session_id: None,
                accent_color: None,
            });
        drop(topology);

        let mut rx = state.event_bus.subscribe();
        let resp = request_window_layout(
            State(state.clone()),
            Path(window_id),
            Json(RequestWindowLayoutRequest {
                label: Some(label.to_string()),
                layout: "tiled".to_string(),
            }),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        match rx.try_recv() {
            Ok(crate::state::AppEvent::TmuxWindowLayoutRequested {
                session_ids,
                layout,
            }) => {
                assert_eq!(session_ids, vec![first_tuic_id, second_tuic_id]);
                assert_eq!(layout, "tiled");
            }
            other => panic!("expected TmuxWindowLayoutRequested, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn request_window_layout_is_a_silent_noop_when_nothing_is_materialized() {
        let state = super::super::tests::test_state();
        let label = "test-window-layout-nothing-materialized";

        let created = create_tmux_session(
            State(state.clone()),
            Json(CreateTmuxSessionRequest {
                label: Some(label.to_string()),
                name: "s".to_string(),
                window_name: None,
                cwd: None,
            }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(created.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let window_id = created["window_id"].as_str().unwrap().to_string();

        let mut rx = state.event_bus.subscribe();
        let resp = request_window_layout(
            State(state.clone()),
            Path(window_id),
            Json(RequestWindowLayoutRequest {
                label: Some(label.to_string()),
                layout: "tiled".to_string(),
            }),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            rx.try_recv().is_err(),
            "new-session's still-virtual initial pane means nothing to arrange yet"
        );
    }
}
