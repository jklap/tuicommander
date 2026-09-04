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
    state.sessions.iter().map(|e| e.key().clone()).collect()
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
    if state.sessions.len() >= crate::MAX_CONCURRENT_SESSIONS {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error": "Max concurrent sessions reached"})),
        ));
    }
    let shell = resolve_shell(None);
    let state_clone = state.clone();
    let spawn = tokio::task::spawn_blocking(move || {
        super::session::spawn_pty_session(state_clone, shell, cwd, 24, 80, None, None)
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
    let pending_title = if let Some(mut topology) = state.tmux_servers.get_mut(label)
        && let Some(pane) = topology.find_pane_mut(pane_id)
    {
        pane.tuic_session_id = Some(spawn.clone());
        pane.title.clone()
    } else {
        None
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
                    },
                    TmuxPane {
                        id: pid2,
                        index: 1,
                        title: None,
                        cwd: None,
                        tuic_session_id: Some("uuid-dead".to_string()),
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
}
