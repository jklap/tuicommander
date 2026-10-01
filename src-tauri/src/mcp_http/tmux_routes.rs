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
    /// The layout string ("tiled", "main-vertical", …) from the most recent
    /// real `select-layout` requested for this window, if any. `None` means
    /// this window has never been arranged into a split — a new pane joining
    /// it should NOT force one into existence. Once set, [`create_tmux_pane`]
    /// reuses it to self-trigger a re-arrangement after every subsequent
    /// split, so a later pane isn't stranded waiting for a select-layout call
    /// that may never come (see `arrange_window_layout`'s doc comment).
    last_layout: Option<String>,
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
    /// The TUIC terminal that issued the `split-window`/`respawn-pane` which
    /// created this pane — i.e. the Claude Code lead whose Agent Teams swarm
    /// this teammate belongs to. Sent by `tuic-cli` as its own `TUIC_SESSION`
    /// (the shim runs inside the lead's PTY) and validated against live
    /// sessions. `None` for a plain `tuic alias` user, an older cli, or a pane
    /// created before the lead was known. Lets a teammate's terminal be tied
    /// back to its lead: the teammate's own hooks carry no parent reference.
    lead_session_id: Option<String>,
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

    fn find_window(&self, window_id: &str) -> Option<&TmuxWindow> {
        self.sessions
            .iter()
            .flat_map(|s| s.windows.iter())
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

/// Whether a tmux server label identifies an automated multi-agent swarm
/// rather than a human's own `tuic alias`-as-tmux daily-driver session.
///
/// Claude Code's Agent Teams feature always prefixes its swarm-path calls
/// with `-L claude-swarm-<pid>` (see this module's doc comment and
/// `tuic-cli/src/tmux/args.rs`'s `label()`) — a general-purpose `tuic alias`
/// user who never passes `-L` gets the fixed `"default"` label instead.
/// `materialize()` uses this to decide whether a freshly spawned pane should
/// get `TUIC_NONINTERACTIVE_HINT`: a real human using this shim as their own
/// interactive multiplexer must keep their normal prompt/plugin startup, so
/// the hint must not go out unconditionally to every materialized pane.
fn is_automated_swarm_label(label: &str) -> bool {
    label.starts_with("claude-swarm")
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
    /// The calling shim's own `TUIC_SESSION` (the lead). See
    /// [`TmuxPane::lead_session_id`]; ignored unless it names a live session.
    #[serde(default)]
    origin_session_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct MaterializePaneRequest {
    #[serde(default)]
    cwd: Option<String>,
    /// See [`CreateTmuxPaneRequest::origin_session_id`]. A virtual pane
    /// (`new-session`/`new-window`'s initial pane) only learns its lead here.
    #[serde(default)]
    origin_session_id: Option<String>,
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

/// Accept a caller-supplied lead id only if it names a currently live TUIC
/// session. The field arrives over HTTP, so an unknown/empty/stale id is
/// dropped rather than trusted.
fn validated_origin(live: &HashSet<String>, origin: Option<&str>) -> Option<String> {
    origin
        .filter(|id| !id.is_empty() && live.contains(*id))
        .map(str::to_string)
}

/// TUIC session ids of the live teammate terminals whose pane names `lead_session_id`
/// as its lead, across every tmux server label. Reads only the topology and the
/// shell-state map (no session or silence locks), so it is safe to call while
/// holding a `SilenceState` guard.
///
/// Liveness is `shell_states` membership (created with the session, removed on
/// close). The topology is only reconciled lazily inside route handlers, so a
/// closed teammate's pane otherwise keeps its `tuic_session_id`; counting it as a
/// linked teammate would let a corpse satisfy the fail-safe's "declared <= linked"
/// check after Claude had already dropped that teammate from the lead's list,
/// masking a genuinely unaccounted-for one. The cost of excluding it: until the
/// lead's next `Stop` refreshes its list, a just-closed teammate still counts as
/// declared-but-unlinked, i.e. the lead keeps reading working (the old behavior,
/// self-correcting) rather than idle.
pub(crate) fn teammate_session_ids(state: &AppState, lead_session_id: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    state
        .tmux_servers
        .iter()
        .flat_map(|server| {
            server
                .sessions
                .iter()
                .flat_map(|s| s.windows.iter())
                .flat_map(|w| w.panes.iter())
                .filter(|p| p.lead_session_id.as_deref() == Some(lead_session_id))
                .filter_map(|p| p.tuic_session_id.clone())
                .filter(|id| id != lead_session_id)
                .collect::<Vec<_>>()
        })
        // Distinct teammates, not distinct panes: the fail-safe compares this count
        // with the number of teammates the hook declared, so one terminal
        // represented by two pane entries (a split-window/respawn-pane race) must
        // not count twice and mask a genuinely unaccounted-for teammate.
        .filter(|id| seen.insert(id.clone()))
        .filter(|id| state.session_maps.shell_states.contains_key(id))
        .collect()
}

/// Test-only: register a one-pane topology under `label` in which
/// `teammate_session_id` is a materialized teammate pane owned by `lead_session_id`.
#[cfg(test)]
pub(crate) fn link_teammate_for_test(
    state: &AppState,
    label: &str,
    lead_session_id: &str,
    teammate_session_id: &str,
) {
    let pane = TmuxPane {
        id: "%0".to_string(),
        index: 0,
        title: None,
        cwd: None,
        tuic_session_id: Some(teammate_session_id.to_string()),
        lead_session_id: Some(lead_session_id.to_string()),
        accent_color: None,
    };
    let topology = TmuxTopology {
        next_session: 1,
        next_window: 1,
        next_pane: 1,
        sessions: vec![TmuxSession {
            id: "$0".to_string(),
            name: "claude-swarm".to_string(),
            active_window: Some("@0".to_string()),
            windows: vec![TmuxWindow {
                id: "@0".to_string(),
                name: "swarm-view".to_string(),
                index: 0,
                active_pane: Some("%0".to_string()),
                panes: vec![pane],
                last_layout: None,
            }],
        }],
    };
    state.tmux_servers.insert(label.to_string(), topology);
}

/// The lead that owns `teammate_session_id`'s pane, if it was recorded.
pub(crate) fn lead_of_teammate(state: &AppState, teammate_session_id: &str) -> Option<String> {
    state.tmux_servers.iter().find_map(|server| {
        server
            .sessions
            .iter()
            .flat_map(|s| s.windows.iter())
            .flat_map(|w| w.panes.iter())
            .find(|p| p.tuic_session_id.as_deref() == Some(teammate_session_id))
            .and_then(|p| p.lead_session_id.clone())
            .filter(|lead| lead != teammate_session_id)
    })
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
                lead_session_id: None,
                accent_color: None,
            }],
            last_layout: None,
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
                lead_session_id: None,
                accent_color: None,
            }],
            last_layout: None,
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
            lead_session_id: validated_origin(&live, body.origin_session_id.as_deref()),
            accent_color: None,
        });
        window.active_pane = Some(pane_id.clone());
        (pane_id, previous_active_pane)
    };

    // split-window materialises immediately, unlike new-session/new-window's
    // implicit initial pane (which stays virtual until first use) — this
    // pane is the one Claude Code's respawn-pane will actually target.
    match materialize(&state, &label, &pane_id, body.cwd).await {
        Ok(tuic_session_id) => {
            // If this window has already been arranged into a split at least
            // once, re-derive and re-emit that arrangement now that this new
            // pane has joined it. Without this, the real caller (Claude
            // Code's Agent Teams flow) issues exactly one select-layout per
            // teammate, immediately after that teammate's own split-window —
            // so the LAST teammate added to a window has no SUBSEQUENT
            // select-layout call to ever include its session id, and stays
            // permanently un-docked from the split even though its PTY is
            // running fine. Self-triggering here means every split-window
            // always gets docked regardless of whether the caller happens to
            // issue one more select-layout afterward. A no-op for a window
            // that was never split (`last_layout` still `None`) — a plain
            // split-window shouldn't force a split view into existence on
            // its own.
            let last_layout = state.tmux_servers.get(&label).and_then(|t| {
                t.find_window(&body.window_id)
                    .and_then(|w| w.last_layout.clone())
            });
            if let Some(layout) = last_layout {
                arrange_window_layout(&state, &label, &body.window_id, layout);
            }
            (
                StatusCode::CREATED,
                Json(serde_json::json!({ "pane_id": pane_id, "tuic_session_id": tuic_session_id })),
            )
                .into_response()
        }
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
        // `pane.tuic_session_id` is recorded (below) BEFORE the shell-readiness
        // gate runs, so a second concurrent `materialize` call for the same
        // still-settling pane — a real, documented shape: `tuic-cli`'s own
        // `respawn-pane` retry-on-eager-materialize comment names exactly this
        // (a `split-window` that eagerly materializes racing `respawn-pane` for
        // the same pane) — must not skip the gate just because it hit this fast
        // path instead of the fresh-spawn path below. `apply_pane_readiness_gate`
        // is cheap to call redundantly: `wait_for_shell_idle`'s own fast path
        // returns immediately once the shell is already idle, which is the
        // common case here (the pane was materialized a while ago).
        apply_pane_readiness_gate(
            state,
            pane_id,
            &existing,
            crate::mcp_http::mcp_transport::SHELL_READINESS_TIMEOUT_MS,
        )
        .await;
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
    let requested = super::session::RequestedIdentity {
        extra_env: if is_automated_swarm_label(label) {
            vec![("TUIC_NONINTERACTIVE_HINT".to_string(), "1".to_string())]
        } else {
            Vec::new()
        },
        ..Default::default()
    };
    let spawn = tokio::task::spawn_blocking(move || {
        super::session::spawn_pty_session(state_clone, shell, cwd, 24, 80, None, requested)
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
    if let Some(title) = pending_title.filter(|title| !title.is_empty()) {
        state.rename_session_from_backend(&spawn, title, true);
    }
    if let Some(color) = pending_accent_color {
        state.set_pty_accent_color(&spawn, Some(color));
    }
    // Shell-readiness gate: block until the freshly spawned shell reaches a real
    // prompt (or we give up), so a caller's very next write — `respawn-pane`'s
    // launch-command write, in the tmux shim's actual usage — cannot arrive
    // while the shell is still mid-startup. Closes the p10k-wizard-hijack race
    // (plans/p10k-wizard-hijack-agent-pane-spawn-race.md) at its source: raw
    // keystrokes landing during `.zshrc` sourcing used to get eaten by
    // Instant Prompt's own remediation menu instead of reaching the shell.
    apply_pane_readiness_gate(
        state,
        pane_id,
        &spawn,
        crate::mcp_http::mcp_transport::SHELL_READINESS_TIMEOUT_MS,
    )
    .await;
    Ok(spawn)
}

/// Fail-open, not fail-hard: a shell with no detectable prompt marker (no OSC
/// 133 integration — bash/fish without it sourced) must not hang pane creation
/// forever, so a timeout only logs rather than erroring — `materialize` always
/// returns `Ok` regardless of which branch this takes. `wait_for_shell_idle`
/// unifies both readiness signals already (OSC 133 `'A'` sets `SHELL_IDLE`
/// immediately; the silence-timer fallback reaches the same state once real
/// prompt output goes quiet), so this one predicate covers both without new
/// detection logic. `timeout_ms` is a parameter (not baked into the body) so a
/// test can exercise the fail-open branch without a multi-second real wait.
async fn apply_pane_readiness_gate(
    state: &Arc<AppState>,
    pane_id: &str,
    session_id: &str,
    timeout_ms: u64,
) {
    if !crate::mcp_http::mcp_transport::wait_for_shell_idle(state, session_id, timeout_ms).await {
        tracing::warn!(
            "materialize: pane {pane_id} (session {session_id}) never reached a ready shell \
             prompt within {timeout_ms}ms — proceeding anyway rather than hanging pane creation"
        );
    }
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
    // A virtual pane (the initial pane of `new-session`/`new-window`, which is
    // what a swarm's FIRST teammate uses) only learns its lead here. Never
    // overwrite one already recorded at `split-window` time.
    if let Some(lead) =
        validated_origin(&live_session_ids(&state), body.origin_session_id.as_deref())
        && let Some(mut topology) = state.tmux_servers.get_mut(&label)
        && let Some(pane) = topology.find_pane_mut(&pane_id)
        && pane.lead_session_id.is_none()
    {
        pane.lead_session_id = Some(lead);
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
    // A tmux rename starts in the backend, so it goes through
    // `rename_session_from_backend` (which emits `session-renamed` to every UI)
    // rather than the frontend-originated `PUT /sessions/{id}/name`, which by
    // design never emits. Agents' status tickers repeat `select-pane -T` with an
    // unchanged title on every repaint, so an unchanged title is a no-op.
    if let Some(tuic_id) = tuic_session_id
        && let Some(title) = body.title.filter(|title| !title.is_empty())
    {
        let unchanged = state
            .session_maps
            .sessions
            .get(&tuic_id)
            .is_some_and(|entry| {
                let session = entry.lock();
                session.display_name.as_deref() == Some(title.as_str())
                    && session.display_name_is_custom
            });
        if !unchanged {
            state.rename_session_from_backend(&tuic_id, title, true);
        }
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
/// Re-derives `session_ids` for `window_id` from currently-materialized panes
/// and dual-emits `TmuxWindowLayoutRequested` — the same work
/// `request_window_layout` itself does for a real `select-layout` call, and
/// also called by `create_tmux_pane` right after a new pane joins an
/// already-split window, so a rebuild is driven by identical logic
/// regardless of which caller asks for it. Records `layout` onto the window
/// as `last_layout` so the next `create_tmux_pane` call knows to self-trigger
/// too. Silently does nothing if the window is gone or nothing is
/// materialized yet — callers that need a 404 for "window doesn't exist"
/// check that themselves first.
fn arrange_window_layout(state: &Arc<AppState>, label: &str, window_id: &str, layout: String) {
    let live = live_session_ids(state);
    let session_ids: Vec<String> = {
        let Some(mut topology) = state.tmux_servers.get_mut(label) else {
            return;
        };
        reconcile(&mut topology, &live);
        let Some(window) = topology.find_window_mut(window_id) else {
            return;
        };
        window.last_layout = Some(layout.clone());
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
        return;
    }
    state.emit_pty_event(crate::state::AppEvent::TmuxWindowLayoutRequested {
        session_ids: session_ids.clone(),
        layout: layout.clone(),
    });
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        use tauri::Emitter;
        let _ = app.emit(
            "tmux-window-layout-requested",
            serde_json::json!({
                "session_ids": session_ids,
                "layout": layout,
            }),
        );
    }
}

pub(crate) async fn request_window_layout(
    State(state): State<Arc<AppState>>,
    Path(window_id): Path<String>,
    Json(body): Json<RequestWindowLayoutRequest>,
) -> impl IntoResponse {
    let label = resolve_label(body.label);
    let live = live_session_ids(&state);
    {
        let Some(mut topology) = state.tmux_servers.get_mut(&label) else {
            return not_found("window").into_response();
        };
        reconcile(&mut topology, &live);
        if topology.find_window_mut(&window_id).is_none() {
            return not_found("window").into_response();
        }
    }
    arrange_window_layout(&state, &label, &window_id, body.layout);
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Claude Code's swarm path always prefixes its label with
    /// `claude-swarm` (see `tuic-cli/src/tmux/args.rs`'s `label()`); a
    /// general-purpose `tuic alias` user who never passes `-L` gets the
    /// fixed `"default"` label. `materialize()`'s `TUIC_NONINTERACTIVE_HINT`
    /// must only go out for the former — a human using this shim as their
    /// own interactive multiplexer must keep their normal startup.
    #[test]
    fn is_automated_swarm_label_matches_only_claude_swarm_labels() {
        assert!(is_automated_swarm_label("claude-swarm-42"));
        assert!(is_automated_swarm_label("claude-swarm"));
        assert!(!is_automated_swarm_label("default"));
        assert!(!is_automated_swarm_label("my-own-session"));
    }

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
                        lead_session_id: None,
                        accent_color: None,
                    },
                    TmuxPane {
                        id: pid2,
                        index: 1,
                        title: None,
                        cwd: None,
                        tuic_session_id: Some("uuid-dead".to_string()),
                        lead_session_id: None,
                        accent_color: None,
                    },
                ],
                last_layout: None,
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
                origin_session_id: None,
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
    /// the backend rename must not re-emit `session-renamed` when the title
    /// hasn't actually changed (see `session::set_session_name_never_emits_session_renamed`
    /// for the frontend-originated half of that loop).
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
        // `rename_pane` only renames the tab when the
        // pane already has a `tuic_session_id`; the opposite order (rename
        // while still virtual, materialize after) is covered by
        // `materialize_applies_a_title_recorded_while_the_pane_was_still_virtual`
        // below — that path used to lose the title silently.
        let materialized = materialize_pane(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
            Json(MaterializePaneRequest {
                cwd: None,
                origin_session_id: None,
            }),
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
        // The shell-readiness gate (`materialize`'s own `apply_pane_readiness_gate`
        // call, both on the fresh-spawn path above and its "already materialized"
        // fast path) means a real, still-running shell can emit ordinary
        // background traffic (e.g. a subsequent `PtyOsc133` prompt marker) on
        // this same event bus between the subscribe above and the rename below —
        // drain until `SessionRenamed` turns up rather than assuming it's the
        // very first event, matching the sibling pattern a few tests below this
        // one for the identical reason.
        let mut found = None;
        while let Ok(event) = rx.try_recv() {
            if let crate::state::AppEvent::SessionRenamed { .. } = &event {
                found = Some(event);
                break;
            }
        }
        match found {
            Some(crate::state::AppEvent::SessionRenamed {
                name, is_custom, ..
            }) => {
                assert_eq!(name, "build");
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
        // Same background-traffic caveat as above: assert no `SessionRenamed`
        // specifically, not that the channel is silent — the still-running real
        // shell can legitimately put other event kinds (e.g. `PtyOsc133`) on
        // this bus regardless of the no-op rename.
        while let Ok(event) = rx.try_recv() {
            assert!(
                !matches!(event, crate::state::AppEvent::SessionRenamed { .. }),
                "repeated select-pane -T with an unchanged title must not re-emit session-renamed, got {event:?}"
            );
        }

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
        let mut found = None;
        while let Ok(event) = rx.try_recv() {
            if let crate::state::AppEvent::SessionRenamed { .. } = &event {
                found = Some(event);
                break;
            }
        }
        match found {
            Some(crate::state::AppEvent::SessionRenamed { name, .. }) => {
                assert_eq!(name, "test");
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
            Json(MaterializePaneRequest {
                cwd: None,
                origin_session_id: None,
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
                name,
                is_custom,
            }) => {
                assert_eq!(session_id, tuic_session_id);
                assert_eq!(name, "src-lister");
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
        // Real directories: `spawn_pty_session` refuses a cwd that does not exist.
        let dirs = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let real_repo = dirs.path().join("real-repo");
        std::fs::create_dir_all(&real_repo).unwrap();
        let real_repo = real_repo.to_string_lossy().into_owned();

        let created = create_tmux_session(
            State(state.clone()),
            Json(CreateTmuxSessionRequest {
                label: Some(label.to_string()),
                name: "s".to_string(),
                window_name: None,
                cwd: Some(real_repo.clone()),
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
            Json(MaterializePaneRequest {
                cwd: None,
                origin_session_id: None,
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
            Some(real_repo.as_str()),
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
        // Real directories: `spawn_pty_session` refuses a cwd that does not exist.
        let dirs = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let session_repo = dirs.path().join("session-repo");
        std::fs::create_dir_all(&session_repo).unwrap();
        let session_repo = session_repo.to_string_lossy().into_owned();
        let window_repo = dirs.path().join("window-repo");
        std::fs::create_dir_all(&window_repo).unwrap();
        let window_repo = window_repo.to_string_lossy().into_owned();

        let created_session = create_tmux_session(
            State(state.clone()),
            Json(CreateTmuxSessionRequest {
                label: Some(label.to_string()),
                name: "s".to_string(),
                window_name: None,
                cwd: Some(session_repo.clone()),
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
                cwd: Some(window_repo.clone()),
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
            Json(MaterializePaneRequest {
                cwd: None,
                origin_session_id: None,
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
            Some(window_repo.as_str()),
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
        // Real directories: `spawn_pty_session` refuses a cwd that does not exist.
        let dirs = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let topology_repo = dirs.path().join("topology-repo");
        std::fs::create_dir_all(&topology_repo).unwrap();
        let topology_repo = topology_repo.to_string_lossy().into_owned();
        let explicit_repo = dirs.path().join("explicit-repo");
        std::fs::create_dir_all(&explicit_repo).unwrap();
        let explicit_repo = explicit_repo.to_string_lossy().into_owned();

        let created = create_tmux_session(
            State(state.clone()),
            Json(CreateTmuxSessionRequest {
                label: Some(label.to_string()),
                name: "s".to_string(),
                window_name: None,
                cwd: Some(topology_repo.clone()),
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
                cwd: Some(explicit_repo.clone()),
                origin_session_id: None,
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
            Some(explicit_repo.as_str()),
            "an explicit request cwd must never be silently overridden by a \
             stale topology-recorded value"
        );
    }

    /// A pane materialized with no prior `select-pane -T` call must not emit
    /// a spurious rename — `pending_title` is `None` and `materialize` must
    /// skip the tab rename entirely.
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
            Json(MaterializePaneRequest {
                cwd: None,
                origin_session_id: None,
            }),
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

    /// The shell-readiness gate's integration point: a real `materialize()`
    /// spawn must reach `SHELL_IDLE` before its response is returned. Proves
    /// `materialize` actually calls the gate (not just that `wait_for_shell_idle`
    /// works in isolation, which `mcp_transport`'s own tests already cover).
    #[tokio::test]
    async fn materialize_shell_readiness_gate_reaches_idle_before_returning() {
        let state = super::super::tests::test_state();
        let label = "test-materialize-readiness-gate";

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
            Path(pane_id),
            label_query(label),
            Json(MaterializePaneRequest {
                cwd: None,
                origin_session_id: None,
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

        let shell_state = state
            .session_maps
            .shell_states
            .get(tuic_session_id)
            .map(|v| v.load(std::sync::atomic::Ordering::Relaxed));
        assert_eq!(
            shell_state,
            Some(crate::pty::SHELL_IDLE),
            "materialize must not return until the spawned shell reaches SHELL_IDLE \
             (or the gate times out, which a real quick shell here should not hit)"
        );
    }

    /// Fail-open at the integration point: a session rigged to never reach
    /// idle must not make `materialize`'s gate call hang — bounded here with a
    /// short parameterized timeout rather than the real 5s
    /// `SHELL_READINESS_TIMEOUT_MS`, per this repo's rule against baking a
    /// load-bearing timing bound into a test's wall-clock budget.
    #[tokio::test]
    async fn apply_pane_readiness_gate_does_not_hang_when_shell_never_goes_idle() {
        use std::sync::atomic::AtomicU8;

        let state = super::super::tests::test_state();
        state.session_maps.shell_states.insert(
            "never-idle-pane".to_string(),
            AtomicU8::new(crate::pty::SHELL_BUSY),
        );
        // Bounded by the test harness itself: if this hangs, the test times out
        // rather than the suite — proving the gate's own short timeout is what
        // returns control, not an external bound saving it.
        tokio::time::timeout(
            std::time::Duration::from_millis(500),
            apply_pane_readiness_gate(&state, "%0", "never-idle-pane", 50),
        )
        .await
        .expect("apply_pane_readiness_gate must return on its own timeout, not hang");
    }

    /// Regression for a race a code review caught: `pane.tuic_session_id` is
    /// recorded (so the "already materialized" fast path can find it) BEFORE
    /// the fresh-spawn path's own readiness-gate call runs — so a second
    /// concurrent `materialize` call for the same pane, landing after that
    /// record but before the first call's gate resolves, used to hit the fast
    /// path and return immediately with no wait at all, defeating the whole
    /// point of this feature for exactly the concurrent-caller shape it exists
    /// to close (`tuic-cli`'s own `respawn-pane` retry comment documents a real
    /// instance of two callers racing to materialize the same pane). Proven
    /// here directly against `materialize()`'s fast-path branch: a pane
    /// already holding a `tuic_session_id` whose shell is rigged to not be
    /// idle yet must still block the fast path until it is.
    #[tokio::test]
    async fn materialize_fast_path_still_waits_for_a_not_yet_idle_shell() {
        use std::sync::atomic::Ordering;

        let state = super::super::tests::test_state();
        let label = "test-materialize-fast-path-waits";

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

        // Materialize it for real once, then rig its shell back to BUSY — this
        // simulates a second caller's `materialize` landing on the fast path
        // (pane.tuic_session_id already set) while the shell isn't confirmed
        // idle right now, without needing genuine spawn-level concurrency.
        let materialized = materialize_pane(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
            Json(MaterializePaneRequest {
                cwd: None,
                origin_session_id: None,
            }),
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
        state
            .session_maps
            .shell_states
            .get(&tuic_session_id)
            .unwrap()
            .store(crate::pty::SHELL_BUSY, Ordering::Release);

        let mut waiter = tokio::spawn({
            let waiting_state = state.clone();
            let waiting_pane_id = pane_id.clone();
            async move {
                materialize_pane(
                    State(waiting_state),
                    Path(waiting_pane_id),
                    label_query(label),
                    Json(MaterializePaneRequest {
                        cwd: None,
                        origin_session_id: None,
                    }),
                )
                .await
                .into_response()
            }
        });

        // The load-bearing assertion: with a not-yet-idle shell, the fast path
        // must NOT have completed yet. A bare final-status check can't catch
        // this bug — the fast path returns `Ok` either way, with or without
        // waiting — so this checks ORDERING, not just the eventual outcome.
        let still_pending = tokio::time::timeout(std::time::Duration::from_millis(50), &mut waiter)
            .await
            .is_err();
        assert!(
            still_pending,
            "materialize's fast path must block on a not-yet-idle shell, not return immediately"
        );

        state
            .session_maps
            .shell_states
            .get(&tuic_session_id)
            .unwrap()
            .store(crate::pty::SHELL_IDLE, Ordering::Release);
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: tuic_session_id.clone(),
            parsed: serde_json::json!({"type": "shell-state", "state": "idle"}).into(),
        });

        let response = tokio::time::timeout(std::time::Duration::from_secs(2), waiter)
            .await
            .expect("the fast path must return once idle, not hang")
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
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
            Json(MaterializePaneRequest {
                cwd: None,
                origin_session_id: None,
            }),
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
        // Same background-traffic caveat as the rename tests above: the
        // shell-readiness gate lets a real, still-running shell put other
        // event kinds (e.g. `PtyOsc133`) on this bus around the same time —
        // drain until the expected event turns up rather than assuming it's
        // the very first one.
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
        while let Ok(event) = rx.try_recv() {
            assert!(
                !matches!(
                    event,
                    crate::state::AppEvent::SessionAccentColorChanged { .. }
                ),
                "the same resolved color from a sibling set-option call must not re-emit, got {event:?}"
            );
        }
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
            Json(MaterializePaneRequest {
                cwd: None,
                origin_session_id: None,
            }),
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
            Json(MaterializePaneRequest {
                cwd: None,
                origin_session_id: None,
            }),
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
                origin_session_id: None,
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
                lead_session_id: None,
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
        // Two real materialized panes are already running by the time this
        // subscribes — same background-traffic caveat as the rename/accent-color
        // tests above — drain until the expected event turns up.
        let mut found = None;
        while let Ok(event) = rx.try_recv() {
            if let crate::state::AppEvent::TmuxWindowLayoutRequested { .. } = &event {
                found = Some(event);
                break;
            }
        }
        match found {
            Some(crate::state::AppEvent::TmuxWindowLayoutRequested {
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

    // Regression (2026-09-28): the real caller (Claude Code's Agent Teams flow)
    // issues exactly one select-layout per teammate, immediately after that
    // teammate's own split-window — never a trailing call after the LAST
    // teammate joins. Before this fix, `request_window_layout` was the ONLY
    // thing that ever emitted `TmuxWindowLayoutRequested`, so the last
    // teammate's session id was never handed to the frontend's
    // `arrangeSessionsAsLayout` and it stayed permanently un-docked from the
    // split even though its PTY was running fine (live-reproduced against a
    // real debug instance: 2 of 3 swarm teammates landed in the split, the
    // third — the last one added — did not).
    #[tokio::test]
    async fn create_tmux_pane_self_triggers_arrangement_for_the_last_teammate_with_no_further_select_layout()
     {
        let state = super::super::tests::test_state();
        let label = "test-last-teammate-self-trigger";

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

        // Teammate 1: split-window off the initial pane, then a real
        // select-layout — mirrors the real per-teammate onboarding sequence.
        let split1 = create_tmux_pane(
            State(state.clone()),
            Json(CreateTmuxPaneRequest {
                label: Some(label.to_string()),
                window_id: window_id.clone(),
                cwd: None,
                origin_session_id: None,
            }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(split1.into_body(), usize::MAX)
            .await
            .unwrap();
        let tuic1 = serde_json::from_slice::<serde_json::Value>(&body).unwrap()["tuic_session_id"]
            .as_str()
            .unwrap()
            .to_string();

        let resp = request_window_layout(
            State(state.clone()),
            Path(window_id.clone()),
            Json(RequestWindowLayoutRequest {
                label: Some(label.to_string()),
                layout: "tiled".to_string(),
            }),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::OK);

        // Teammate 2 — the LAST one. Nothing joins after it, and critically no
        // explicit select-layout call follows this split either.
        let mut rx = state.event_bus.subscribe();
        let split2 = create_tmux_pane(
            State(state.clone()),
            Json(CreateTmuxPaneRequest {
                label: Some(label.to_string()),
                window_id: window_id.clone(),
                cwd: None,
                origin_session_id: None,
            }),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(split2.into_body(), usize::MAX)
            .await
            .unwrap();
        let tuic2 = serde_json::from_slice::<serde_json::Value>(&body).unwrap()["tuic_session_id"]
            .as_str()
            .unwrap()
            .to_string();

        let mut found = None;
        while let Ok(event) = rx.try_recv() {
            if let crate::state::AppEvent::TmuxWindowLayoutRequested { .. } = &event {
                found = Some(event);
                break;
            }
        }
        match found {
            Some(crate::state::AppEvent::TmuxWindowLayoutRequested {
                session_ids,
                layout,
            }) => {
                assert_eq!(
                    session_ids,
                    vec![tuic1, tuic2],
                    "the last teammate's own split-window must self-trigger a re-arrangement \
                     that includes it, with no further select-layout call needed"
                );
                assert_eq!(layout, "tiled");
            }
            other => panic!(
                "expected create_tmux_pane to self-trigger TmuxWindowLayoutRequested for the \
                 last teammate, got {other:?}"
            ),
        }
    }

    // Control for the regression above: a plain split-window on a window that
    // has NEVER been arranged into a split (no select-layout call has ever
    // named it) must not force one into existence on its own.
    #[tokio::test]
    async fn create_tmux_pane_does_not_self_trigger_a_layout_for_a_window_never_split_before() {
        let state = super::super::tests::test_state();
        let label = "test-never-split-no-self-trigger";

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
        let split = create_tmux_pane(
            State(state.clone()),
            Json(CreateTmuxPaneRequest {
                label: Some(label.to_string()),
                window_id,
                cwd: None,
                origin_session_id: None,
            }),
        )
        .await
        .into_response();
        assert_eq!(split.status(), StatusCode::CREATED);

        while let Ok(event) = rx.try_recv() {
            assert!(
                !matches!(
                    event,
                    crate::state::AppEvent::TmuxWindowLayoutRequested { .. }
                ),
                "a window that was never split before must not get one from a plain split-window"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Characterization tests (teammate-background-work-busy, step 1).
    //
    // These pin TODAY's wire shapes and lifecycle behavior BEFORE a planned
    // `lead_session_id` is added to `TmuxPane` / the pane-create request (see
    // plans/teammate-background-work-busy.md). A test that fails after that
    // change is a deliberate shape change to reconcile, not a regression to
    // silence — update the expectation and the consumers together.
    // -----------------------------------------------------------------------

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// Creates a (virtual-pane only, no PTY) session under `label` and returns
    /// `(window_id, initial_pane_id)`.
    async fn new_virtual_session(state: &Arc<AppState>, label: &str) -> (String, String) {
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
        let created = body_json(created).await;
        (
            created["window_id"].as_str().unwrap().to_string(),
            created["pane_id"].as_str().unwrap().to_string(),
        )
    }

    #[test]
    fn tmux_pane_serializes_with_exactly_todays_keys() {
        // Consumers (`tuic-cli` target resolution, the UI) read this JSON; a
        // new optional field must not remove or rename any of these.
        let pane = TmuxPane {
            id: "%3".to_string(),
            index: 2,
            title: Some("mate".to_string()),
            cwd: Some("/repo".to_string()),
            tuic_session_id: Some("uuid-1".to_string()),
            lead_session_id: Some("lead-1".to_string()),
            accent_color: Some("blue".to_string()),
        };
        assert_eq!(
            serde_json::to_value(&pane).unwrap(),
            serde_json::json!({
                "id": "%3",
                "index": 2,
                "title": "mate",
                "cwd": "/repo",
                "tuic_session_id": "uuid-1",
                "lead_session_id": "lead-1",
                "accent_color": "blue",
            })
        );

        // A virtual pane serializes its optional fields as explicit nulls
        // (not omitted) — `tuic-cli` distinguishes "virtual" by `null`.
        let virtual_pane = TmuxPane {
            id: "%0".to_string(),
            index: 0,
            title: None,
            cwd: None,
            tuic_session_id: None,
            lead_session_id: None,
            accent_color: None,
        };
        assert_eq!(
            serde_json::to_value(&virtual_pane).unwrap(),
            serde_json::json!({
                "id": "%0",
                "index": 0,
                "title": null,
                "cwd": null,
                "tuic_session_id": null,
                "lead_session_id": null,
                "accent_color": null,
            })
        );
    }

    #[test]
    fn tmux_topology_serializes_its_full_nested_shape() {
        let mut t = sample();
        t.sessions[0].windows[0].last_layout = Some("tiled".to_string());
        let v = serde_json::to_value(&t).unwrap();
        // Counters are part of the wire shape (allocator state).
        assert_eq!(v["next_session"], 1);
        assert_eq!(v["next_window"], 1);
        assert_eq!(v["next_pane"], 2);
        let session = &v["sessions"][0];
        assert_eq!(session["id"], "$0");
        assert_eq!(session["name"], "claude-swarm");
        assert_eq!(session["active_window"], "@0");
        let window = &session["windows"][0];
        assert_eq!(window["id"], "@0");
        assert_eq!(window["name"], "swarm-view");
        assert_eq!(window["index"], 0);
        assert_eq!(window["active_pane"], "%1");
        assert_eq!(window["last_layout"], "tiled");
        assert_eq!(window["panes"].as_array().unwrap().len(), 2);
        assert_eq!(window["panes"][0]["tuic_session_id"], "uuid-live");
        assert_eq!(window["panes"][1]["tuic_session_id"], "uuid-dead");
        // The exact key set of a window (no field silently added/dropped).
        let mut keys: Vec<&str> = window
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["active_pane", "id", "index", "last_layout", "name", "panes"]
        );
    }

    #[test]
    fn pane_create_and_materialize_requests_ignore_unknown_fields_and_require_window_id() {
        // Forward/backward-compat: an OLDER server must accept a NEWER cli's
        // extra body field, and a newer server must accept an older cli's
        // body without it. Pin both directions of the pane-create request.
        let today: CreateTmuxPaneRequest = serde_json::from_value(serde_json::json!({
            "label": "claude-swarm-1", "window_id": "@0", "cwd": "/r"
        }))
        .unwrap();
        assert_eq!(today.label.as_deref(), Some("claude-swarm-1"));
        assert_eq!(today.window_id, "@0");
        assert_eq!(today.cwd.as_deref(), Some("/r"));

        let extra: CreateTmuxPaneRequest = serde_json::from_value(serde_json::json!({
            "label": "l", "window_id": "@0", "cwd": null, "some_future_field": "x"
        }))
        .expect("an unknown field must be ignored, not rejected");
        assert!(extra.cwd.is_none());

        // `label` and `cwd` are optional; `window_id` is not.
        let minimal: CreateTmuxPaneRequest =
            serde_json::from_value(serde_json::json!({ "window_id": "@2" })).unwrap();
        assert!(minimal.label.is_none() && minimal.cwd.is_none());
        assert!(
            serde_json::from_value::<CreateTmuxPaneRequest>(serde_json::json!({ "label": "l" }))
                .is_err(),
            "window_id is required"
        );

        // Materialize: body is `{cwd}` only; empty and extra-field bodies parse.
        let empty: MaterializePaneRequest = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(empty.cwd.is_none());
        let extra: MaterializePaneRequest =
            serde_json::from_value(serde_json::json!({ "cwd": "/r", "future": 1 })).unwrap();
        assert_eq!(extra.cwd.as_deref(), Some("/r"));
    }

    #[tokio::test]
    async fn get_topology_defaults_to_the_default_label_and_isolates_labels() {
        let state = super::super::tests::test_state();
        new_virtual_session(&state, "test-topology-isolation-swarm-7").await;

        // No `?label=` -> "default", a different (empty) topology.
        let default_topo = get_topology(State(state.clone()), Query(LabelQuery { label: None }))
            .await
            .into_response();
        assert_eq!(default_topo.status(), StatusCode::OK);
        let default_topo = body_json(default_topo).await;
        assert_eq!(default_topo["sessions"].as_array().unwrap().len(), 0);
        assert!(state.tmux_servers.contains_key(DEFAULT_LABEL));

        let swarm = get_topology(
            State(state.clone()),
            label_query("test-topology-isolation-swarm-7"),
        )
        .await
        .into_response();
        let swarm = body_json(swarm).await;
        assert_eq!(swarm["sessions"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn get_topology_serializes_a_fresh_virtual_pane_with_null_session_id() {
        let state = super::super::tests::test_state();
        let label = "test-topology-virtual-pane-shape";
        let (window_id, pane_id) = new_virtual_session(&state, label).await;

        let topo = body_json(
            get_topology(State(state.clone()), label_query(label))
                .await
                .into_response(),
        )
        .await;
        let pane = &topo["sessions"][0]["windows"][0]["panes"][0];
        assert_eq!(pane["id"], pane_id.as_str());
        assert_eq!(topo["sessions"][0]["windows"][0]["id"], window_id.as_str());
        assert!(pane["tuic_session_id"].is_null(), "virtual until first use");
        assert!(pane["accent_color"].is_null());
        assert!(pane["title"].is_null());
    }

    #[tokio::test]
    async fn get_topology_reverts_a_pane_whose_tuic_session_is_no_longer_live() {
        // reconcile() runs on READ: a pane pointing at a dead TUIC session
        // reverts to virtual (null) the next time the topology is fetched.
        let state = super::super::tests::test_state();
        let label = "test-topology-reconcile-on-read";
        let (_window, pane_id) = new_virtual_session(&state, label).await;
        {
            let mut topo = state.tmux_servers.get_mut(label).unwrap();
            topo.find_pane_mut(&pane_id).unwrap().tuic_session_id =
                Some("ghost-session-not-in-session_maps".to_string());
        }

        let topo = body_json(
            get_topology(State(state.clone()), label_query(label))
                .await
                .into_response(),
        )
        .await;
        assert!(
            topo["sessions"][0]["windows"][0]["panes"][0]["tuic_session_id"].is_null(),
            "a dead session id must not leak through get_topology"
        );
    }

    #[tokio::test]
    async fn delete_tmux_session_removes_the_session_and_404s_when_unknown() {
        let state = super::super::tests::test_state();
        let label = "test-delete-session";
        new_virtual_session(&state, label).await;

        let gone = delete_tmux_session(
            State(state.clone()),
            Path("$0".to_string()),
            label_query(label),
        )
        .await
        .into_response();
        assert_eq!(gone.status(), StatusCode::OK);
        assert!(state.tmux_servers.get(label).unwrap().sessions.is_empty());

        // Second delete: session no longer exists.
        let again = delete_tmux_session(
            State(state.clone()),
            Path("$0".to_string()),
            label_query(label),
        )
        .await
        .into_response();
        assert_eq!(again.status(), StatusCode::NOT_FOUND);

        // Unknown label: no server at all.
        let no_server = delete_tmux_session(
            State(state.clone()),
            Path("$0".to_string()),
            label_query("test-delete-session-no-such-label"),
        )
        .await
        .into_response();
        assert_eq!(no_server.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn kill_pane_removes_a_virtual_pane_and_404s_for_unknown_pane_or_label() {
        let state = super::super::tests::test_state();
        let label = "test-kill-pane-virtual";
        let (_window, pane_id) = new_virtual_session(&state, label).await;

        let unknown_pane = kill_pane(
            State(state.clone()),
            Path("%99".to_string()),
            label_query(label),
        )
        .await
        .into_response();
        assert_eq!(unknown_pane.status(), StatusCode::NOT_FOUND);

        let unknown_label = kill_pane(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query("test-kill-pane-virtual-no-such-label"),
        )
        .await
        .into_response();
        assert_eq!(unknown_label.status(), StatusCode::NOT_FOUND);

        let killed = kill_pane(State(state.clone()), Path(pane_id), label_query(label))
            .await
            .into_response();
        assert_eq!(killed.status(), StatusCode::OK);
        let topo = state.tmux_servers.get(label).unwrap();
        assert!(topo.sessions[0].windows[0].panes.is_empty());
        assert_eq!(
            topo.sessions[0].windows[0].active_pane, None,
            "killing the only pane leaves no active pane"
        );
    }

    #[tokio::test]
    async fn create_tmux_pane_404s_for_an_unknown_label_or_window_without_spawning() {
        let state = super::super::tests::test_state();
        let label = "test-create-pane-404";
        let (_window, _pane) = new_virtual_session(&state, label).await;
        let sessions_before = state.session_maps.sessions.len();

        let unknown_window = create_tmux_pane(
            State(state.clone()),
            Json(CreateTmuxPaneRequest {
                label: Some(label.to_string()),
                window_id: "@99".to_string(),
                cwd: None,
                origin_session_id: None,
            }),
        )
        .await
        .into_response();
        assert_eq!(unknown_window.status(), StatusCode::NOT_FOUND);

        let unknown_label = create_tmux_pane(
            State(state.clone()),
            Json(CreateTmuxPaneRequest {
                label: Some("test-create-pane-404-no-such-label".to_string()),
                window_id: "@0".to_string(),
                cwd: None,
                origin_session_id: None,
            }),
        )
        .await
        .into_response();
        assert_eq!(unknown_label.status(), StatusCode::NOT_FOUND);

        assert_eq!(
            state.session_maps.sessions.len(),
            sessions_before,
            "a 404 must not spawn a PTY"
        );
    }

    #[tokio::test]
    async fn create_tmux_pane_response_shape_and_recorded_pane_today() {
        // Pins the split-window response (`pane_id` + `tuic_session_id`) and
        // what topology records for the new pane, with NO notion of which
        // terminal issued the request (that linkage is what step 4 adds).
        let state = super::super::tests::test_state();
        let label = "test-create-pane-shape";
        let (window_id, initial_pane) = new_virtual_session(&state, label).await;

        let resp = create_tmux_pane(
            State(state.clone()),
            Json(CreateTmuxPaneRequest {
                label: Some(label.to_string()),
                window_id: window_id.clone(),
                cwd: Some("/tmp".to_string()),
                origin_session_id: None,
            }),
        )
        .await
        .into_response();
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "split-window answers 201 Created, not 200"
        );
        let resp = body_json(resp).await;
        let mut keys: Vec<&str> = resp
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["pane_id", "tuic_session_id"]);
        let pane_id = resp["pane_id"].as_str().unwrap();
        let tuic_id = resp["tuic_session_id"].as_str().unwrap();
        assert_ne!(pane_id, initial_pane);

        let topo = state.tmux_servers.get(label).unwrap();
        let window = &topo.sessions[0].windows[0];
        assert_eq!(window.active_pane.as_deref(), Some(pane_id));
        let pane = topo.find_pane(pane_id).unwrap();
        assert_eq!(pane.tuic_session_id.as_deref(), Some(tuic_id));
        assert_eq!(pane.cwd.as_deref(), Some("/tmp"));
        assert!(state.session_maps.sessions.contains_key(tuic_id));
    }

    #[test]
    fn teammate_session_ids_counts_a_terminal_once_even_if_two_panes_reference_it() {
        let state = super::super::tests::test_state();
        for live in ["mate-dup", "mate-other", "mate-y"] {
            state
                .session_maps
                .shell_states
                .insert(live.to_string(), std::sync::atomic::AtomicU8::new(0));
        }
        link_teammate_for_test(&state, "dup-a", "lead-x", "mate-dup");
        link_teammate_for_test(&state, "dup-b", "lead-x", "mate-dup");
        link_teammate_for_test(&state, "dup-c", "lead-x", "mate-other");
        let mut ids = teammate_session_ids(&state, "lead-x");
        ids.sort();
        assert_eq!(ids, vec!["mate-dup".to_string(), "mate-other".to_string()]);
        // ...so two declared teammates with one real terminal are NOT fully accounted for.
        link_teammate_for_test(&state, "only-dup-a", "lead-y", "mate-y");
        link_teammate_for_test(&state, "only-dup-b", "lead-y", "mate-y");
        assert_eq!(teammate_session_ids(&state, "lead-y").len(), 1);
        assert!(state.lead_teammates_may_be_working("lead-y", 2));
    }

    #[test]
    fn a_closed_teammate_terminal_is_not_a_linked_teammate_but_still_names_its_lead() {
        let state = super::super::tests::test_state();
        link_teammate_for_test(&state, "closed-a", "lead-c", "mate-alive");
        link_teammate_for_test(&state, "closed-b", "lead-c", "mate-closed");
        state.session_maps.shell_states.insert(
            "mate-alive".to_string(),
            std::sync::atomic::AtomicU8::new(0),
        );
        // "mate-closed" has no shell_states entry: its session was torn down but its
        // pane is still in the (lazily reconciled) topology.
        assert_eq!(
            teammate_session_ids(&state, "lead-c"),
            vec!["mate-alive".to_string()]
        );
        // The reverse lookup must still resolve a closed teammate so the lead is
        // republished when the teammate's row disappears.
        assert_eq!(
            lead_of_teammate(&state, "mate-closed").as_deref(),
            Some("lead-c")
        );
        // Two declared, one live linked, one closed: the closed one no longer vouches,
        // so the fail-safe still sees an unaccounted-for teammate.
        assert!(state.lead_teammates_may_be_working("lead-c", 2));
    }

    #[test]
    fn validated_origin_accepts_only_a_live_non_empty_session_id() {
        let live: HashSet<String> = ["lead-live".to_string()].into_iter().collect();
        assert_eq!(
            validated_origin(&live, Some("lead-live")),
            Some("lead-live".to_string())
        );
        assert_eq!(validated_origin(&live, Some("gone")), None);
        assert_eq!(validated_origin(&live, Some("")), None);
        assert_eq!(validated_origin(&live, None), None);
    }

    #[tokio::test]
    async fn split_window_records_the_callers_live_session_as_the_panes_lead() {
        let state = super::super::tests::test_state();
        let label = "test-lead-link-split";
        let (window_id, _initial) = new_virtual_session(&state, label).await;
        let split = |origin: Option<String>| {
            let state = state.clone();
            let window_id = window_id.clone();
            async move {
                let resp = create_tmux_pane(
                    State(state),
                    Json(CreateTmuxPaneRequest {
                        label: Some(label.to_string()),
                        window_id,
                        cwd: Some("/tmp".to_string()),
                        origin_session_id: origin,
                    }),
                )
                .await
                .into_response();
                body_json(resp).await
            }
        };

        // A first pane with no origin stands in for the lead's own terminal.
        let lead = split(None).await;
        let lead_session = lead["tuic_session_id"].as_str().unwrap().to_string();
        let lead_pane = lead["pane_id"].as_str().unwrap().to_string();
        // A teammate pane created with that live id as origin is linked.
        let mate = split(Some(lead_session.clone())).await;
        let mate_session = mate["tuic_session_id"].as_str().unwrap().to_string();
        // An origin naming no live session is dropped, not trusted.
        let stranger = split(Some("not-a-live-session".to_string())).await;
        let stranger_pane = stranger["pane_id"].as_str().unwrap().to_string();

        let topo = state.tmux_servers.get(label).unwrap();
        assert_eq!(topo.find_pane(&lead_pane).unwrap().lead_session_id, None);
        assert_eq!(
            topo.find_pane(mate["pane_id"].as_str().unwrap())
                .unwrap()
                .lead_session_id
                .as_deref(),
            Some(lead_session.as_str())
        );
        assert_eq!(
            topo.find_pane(&stranger_pane).unwrap().lead_session_id,
            None
        );
        drop(topo);

        assert_eq!(
            teammate_session_ids(&state, &lead_session),
            vec![mate_session.clone()]
        );
        assert_eq!(
            lead_of_teammate(&state, &mate_session).as_deref(),
            Some(lead_session.as_str())
        );
        assert_eq!(lead_of_teammate(&state, &lead_session), None);
        assert!(teammate_session_ids(&state, "someone-else").is_empty());
    }

    #[tokio::test]
    async fn materializing_a_virtual_pane_learns_its_lead_once_and_never_overwrites_it() {
        let state = super::super::tests::test_state();
        let label = "test-lead-link-materialize";
        let (window_id, initial_pane) = new_virtual_session(&state, label).await;
        // A live session to act as the lead.
        let lead_resp = create_tmux_pane(
            State(state.clone()),
            Json(CreateTmuxPaneRequest {
                label: Some(label.to_string()),
                window_id,
                cwd: Some("/tmp".to_string()),
                origin_session_id: None,
            }),
        )
        .await
        .into_response();
        let lead_session = body_json(lead_resp).await["tuic_session_id"]
            .as_str()
            .unwrap()
            .to_string();

        for origin in [
            Some(lead_session.clone()),
            Some("another-live-or-not".to_string()),
        ] {
            let resp = materialize_pane(
                State(state.clone()),
                Path(initial_pane.clone()),
                Query(LabelQuery {
                    label: Some(label.to_string()),
                }),
                Json(MaterializePaneRequest {
                    cwd: Some("/tmp".to_string()),
                    origin_session_id: origin,
                }),
            )
            .await
            .into_response();
            assert_eq!(resp.status(), StatusCode::OK);
        }
        let topo = state.tmux_servers.get(label).unwrap();
        assert_eq!(
            topo.find_pane(&initial_pane)
                .unwrap()
                .lead_session_id
                .as_deref(),
            Some(lead_session.as_str()),
            "the first valid origin wins; a later call never overwrites it"
        );
    }

    #[tokio::test]
    async fn killing_a_materialized_pane_closes_its_tuic_session_and_topology_forgets_it() {
        let state = super::super::tests::test_state();
        let label = "test-kill-materialized-pane";
        let (window_id, _initial) = new_virtual_session(&state, label).await;
        let resp = body_json(
            create_tmux_pane(
                State(state.clone()),
                Json(CreateTmuxPaneRequest {
                    label: Some(label.to_string()),
                    window_id,
                    cwd: None,
                    origin_session_id: None,
                }),
            )
            .await
            .into_response(),
        )
        .await;
        let pane_id = resp["pane_id"].as_str().unwrap().to_string();
        let tuic_id = resp["tuic_session_id"].as_str().unwrap().to_string();
        assert!(state.session_maps.sessions.contains_key(&tuic_id));

        let killed = kill_pane(
            State(state.clone()),
            Path(pane_id.clone()),
            label_query(label),
        )
        .await
        .into_response();
        assert_eq!(killed.status(), StatusCode::OK);
        assert!(
            !state.session_maps.sessions.contains_key(&tuic_id),
            "kill-pane must close the backing TUIC session"
        );
        assert!(
            state
                .tmux_servers
                .get(label)
                .unwrap()
                .find_pane(&pane_id)
                .is_none()
        );
    }

    #[test]
    fn resolve_label_defaults_only_when_absent_and_default_is_not_an_automated_swarm() {
        assert_eq!(resolve_label(None), "default");
        assert_eq!(
            resolve_label(Some("claude-swarm-123".to_string())),
            "claude-swarm-123"
        );
        assert!(!is_automated_swarm_label(DEFAULT_LABEL));
        // tuic-cli's `-S <path>` label format (`S-<hex>`) is NOT a swarm label
        // either, so a swarm driven through the `-S` leader path would not get
        // `TUIC_NONINTERACTIVE_HINT` — pinned so a future lead-linking design
        // that assumes a `claude-swarm-<pid>` label knows this is `-L`-only.
        assert!(!is_automated_swarm_label("S-1a2b3c"));
    }
}
