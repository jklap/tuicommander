//! Desktop visibility and remote observation have independent lifetimes.
use crate::AppState;
use std::collections::HashMap;
use std::sync::Arc;
#[cfg(any(feature = "desktop", test))]
use std::sync::atomic::Ordering;

#[derive(Default)]
pub(crate) struct Visibility {
    saved: Option<HashMap<String, bool>>,
    attention: bool,
    desktop: HashMap<String, bool>,
    windows: Vec<String>,
    watchers: HashMap<String, usize>,
    all_watchers: usize,
    /// Remote claims per viewer (session id -> viewer id -> last claim), so one remote client
    /// blurring a tab never clears another remote client's claim. Bounded by
    /// [`MAX_VIEWER_ID_LEN`] and [`MAX_VIEWERS_PER_SESSION`]: viewer ids are
    /// client-supplied, and without a cap a client could grow this without bound.
    /// A claim older than [`REMOTE_CLAIM_TTL`] no longer counts.
    remote_visible: HashMap<String, HashMap<String, std::time::Instant>>,
    last_remote_activity: HashMap<String, std::time::Instant>,
    last_global_observation: Option<std::time::Instant>,
}

impl Visibility {
    pub(crate) fn watched(&self, state: &AppState, id: &str) -> bool {
        self.all_watchers > 0
            || self.watchers.get(id).copied().unwrap_or(0) > 0
            || self
                .remote_visible
                .get(id)
                .is_some_and(|viewers| viewers.values().any(|claimed| claim_is_fresh(*claimed)))
            || state
                .ws_clients
                .get(id)
                .is_some_and(|clients| clients.iter().any(|tx| !tx.is_closed()))
    }

    #[cfg(any(unix, test))]
    pub(crate) fn recently_active(&self, state: &AppState, id: &str) -> bool {
        let timeout = std::time::Duration::from_secs(
            u64::from(state.config.read().standby_timeout_minutes) * 60,
        );
        self.last_remote_activity
            .get(id)
            .into_iter()
            .chain(self.last_global_observation.as_ref())
            .any(|last| last.elapsed() < timeout)
    }

    fn refresh(&self, state: &AppState, id: &str) {
        self.refresh_waking(state, id, true);
    }

    /// [`Self::refresh`]; `wake: false` records visibility without SIGCONT-ing a
    /// session parked in standby (the frontend's periodic keep-alive).
    fn refresh_waking(&self, state: &AppState, id: &str, wake: bool) {
        let desktop = self.saved.is_none() && self.desktop.get(id).copied().unwrap_or(false);
        let visible = desktop || self.watched(state, id);
        state
            .session_maps
            .session_visibility
            .insert(id.into(), visible);
        wake_visible(state, id, visible && wake);
    }
}

fn wake_visible(state: &AppState, id: &str, visible: bool) {
    #[cfg(unix)]
    if visible && let Err(error) = crate::pty::wake_session(state, id) {
        tracing::warn!(session_id = id, %error, "Cannot wake visible session");
    }
    #[cfg(not(unix))]
    let _ = (state, id, visible);
}

/// Fallback viewer for a remote client that sends no `viewer_id` (an older
/// build): its claims collapse onto one shared key, the pre-per-viewer behaviour.
pub(crate) const LEGACY_VIEWER_ID: &str = "legacy";
/// Legit viewer ids (`CLIENT_INSTANCE_ID`, a UUID, [`LEGACY_VIEWER_ID`]) are far
/// shorter; an oversized one is ignored.
const MAX_VIEWER_ID_LEN: usize = 128;
/// Past this, a new viewer's claim evicts the oldest existing one.
const MAX_VIEWERS_PER_SESSION: usize = 16;
/// How long a remote viewer's `visible: true` claim lasts without a re-assert.
/// The frontend re-asserts on its 30 s snapshot timer, so a live viewer never
/// gets near this; a viewer that vanished without sending `visible: false` (a
/// crashed tab, a dropped connection) stops holding its session visible, and
/// out of standby, after it.
pub(crate) const REMOTE_CLAIM_TTL: std::time::Duration = std::time::Duration::from_secs(90);

fn claim_is_fresh(claimed: std::time::Instant) -> bool {
    claimed.elapsed() < REMOTE_CLAIM_TTL
}

/// IPC visibility belongs to the desktop; HTTP visibility belongs to a remote client.
/// Production callers pass `wake` through [`set_visible_waking`].
#[cfg(test)]
pub(crate) fn set_visible(state: &AppState, id: &str, visible: bool, desktop: bool) {
    set_visible_for_viewer(state, id, visible, desktop, LEGACY_VIEWER_ID);
}

/// IPC visibility with an explicit `wake`: `false` refreshes the claim
/// without waking a session parked in standby (IPC `set_session_visible`'s
/// keep-alive, same `wake` field as `POST /sessions/{id}/visible`).
pub(crate) fn set_visible_waking(
    state: &AppState,
    id: &str,
    visible: bool,
    desktop: bool,
    wake: bool,
) {
    set_visible_for_viewer_waking(state, id, visible, desktop, LEGACY_VIEWER_ID, wake);
}

/// [`set_visible_for_viewer_waking`] with `wake: true`.
#[cfg(test)]
pub(crate) fn set_visible_for_viewer(
    state: &AppState,
    id: &str,
    visible: bool,
    desktop: bool,
    viewer: &str,
) {
    set_visible_for_viewer_waking(state, id, visible, desktop, viewer, true);
}

/// [`set_visible_waking`] for one remote viewer: a remote session stays visible
/// while ANY remote viewer claims it. `viewer` is ignored for the desktop, which
/// owns a single claim per session. `wake: false` = the frontend's 30 s
/// keep-alive: refresh the claim WITHOUT SIGCONT-ing a parked session, so a
/// keep-alive never holds a hidden/minimised client's session awake. A real
/// focus (`wake` absent/`true`) still wakes it.
pub(crate) fn set_visible_for_viewer_waking(
    state: &AppState,
    id: &str,
    visible: bool,
    desktop: bool,
    viewer: &str,
    wake: bool,
) {
    if !desktop && viewer.len() > MAX_VIEWER_ID_LEN {
        return;
    }
    let mut visibility = state.background_visibility.lock();
    if desktop {
        if visibility.saved.is_some() {
            return;
        }
        visibility.desktop.insert(id.into(), visible);
    } else if visible {
        let viewers = visibility.remote_visible.entry(id.into()).or_default();
        viewers.retain(|_, claimed| claim_is_fresh(*claimed));
        if viewers.len() >= MAX_VIEWERS_PER_SESSION
            && !viewers.contains_key(viewer)
            && let Some(oldest) = viewers
                .iter()
                .min_by_key(|&(_, &claimed)| claimed)
                .map(|(viewer, _)| viewer.clone())
        {
            viewers.remove(&oldest);
        }
        viewers.insert(viewer.into(), std::time::Instant::now());
    } else if let Some(viewers) = visibility.remote_visible.get_mut(id) {
        viewers.remove(viewer);
        if viewers.is_empty() {
            visibility.remote_visible.remove(id);
        }
    }
    if !desktop {
        visibility
            .last_remote_activity
            .insert(id.into(), std::time::Instant::now());
    }
    visibility.refresh_waking(state, id, wake);
}

/// Drop every remote claim older than [`REMOTE_CLAIM_TTL`] and recompute the
/// affected sessions' visibility, returning how many sessions lost a claim. The
/// standby checker runs this each tick: nothing else would notice a claim aging
/// out, so the derived `session_visibility` would keep saying visible. Never
/// wakes a session: losing a claim can only make it less visible.
pub(crate) fn expire_remote_claims(state: &AppState) -> usize {
    let mut visibility = state.background_visibility.lock();
    let mut expired = Vec::new();
    visibility.remote_visible.retain(|id, viewers| {
        let before = viewers.len();
        viewers.retain(|_, claimed| claim_is_fresh(*claimed));
        if viewers.len() != before {
            expired.push(id.clone());
        }
        !viewers.is_empty()
    });
    for id in &expired {
        visibility.refresh_waking(state, id, false);
    }
    expired.len()
}

/// A remote read or input is activity even without a persistent stream. Serialize
/// with standby so it either prevents the stop or wakes the recorded group.
pub(crate) fn record_remote_activity(state: &AppState, id: &str) {
    let mut visibility = state.background_visibility.lock();
    if state.session_maps.sessions.contains_key(id) {
        visibility
            .last_remote_activity
            .insert(id.into(), std::time::Instant::now());
        wake_visible(state, id, true);
    }
}

/// Held by the stream/socket, including its catch-up phase, and dropped on disconnect.
pub(crate) struct RemoteWatch {
    state: Arc<AppState>,
    id: Option<String>,
}
impl RemoteWatch {
    pub(crate) fn new(state: Arc<AppState>, id: Option<String>) -> Self {
        {
            let mut visibility = state.background_visibility.lock();
            if let Some(ref id) = id {
                *visibility.watchers.entry(id.clone()).or_default() += 1;
            } else {
                visibility.all_watchers += 1;
            }
            refresh_watch(&state, &mut visibility, id.as_deref());
        }
        Self { state, id }
    }
}
impl Drop for RemoteWatch {
    fn drop(&mut self) {
        let mut visibility = self.state.background_visibility.lock();
        if let Some(ref id) = self.id {
            if let Some(count) = visibility.watchers.get_mut(id) {
                *count -= 1;
                if *count == 0 {
                    visibility.watchers.remove(id);
                }
            }
        } else {
            visibility.all_watchers -= 1;
        }
        refresh_watch(&self.state, &mut visibility, self.id.as_deref());
    }
}
fn refresh_watch(state: &AppState, visibility: &mut Visibility, id: Option<&str>) {
    if let Some(id) = id {
        if state.session_maps.session_visibility.contains_key(id)
            || state.session_maps.sessions.contains_key(id)
        {
            visibility
                .last_remote_activity
                .insert(id.into(), std::time::Instant::now());
            visibility.refresh(state, id);
        }
    } else {
        visibility.last_global_observation = Some(std::time::Instant::now());
        let ids: Vec<String> = state
            .session_maps
            .session_visibility
            .iter()
            .map(|e| e.key().clone())
            .collect();
        for id in ids {
            visibility.refresh(state, &id);
        }
    }
}

/// The remote viewer ids currently claiming `id` (test-only).
#[cfg(test)]
pub(crate) fn remote_viewers_for_test(state: &AppState, id: &str) -> Vec<String> {
    state
        .background_visibility
        .lock()
        .remote_visible
        .get(id)
        .map(|viewers| viewers.keys().cloned().collect())
        .unwrap_or_default()
}

pub(crate) fn forget_session(state: &AppState, id: &str) {
    let mut visibility = state.background_visibility.lock();
    visibility.remote_visible.remove(id);
    visibility.last_remote_activity.remove(id);
    visibility.desktop.remove(id);
    if let Some(saved) = visibility.saved.as_mut() {
        saved.remove(id);
    }
}

#[cfg(any(feature = "desktop", test))]
pub(crate) trait Windows {
    fn visible_labels(&self) -> Vec<String>;
    fn ensure_main(&self) -> Result<(), String> {
        Ok(())
    }
    fn all_labels(&self) -> Vec<String> {
        self.visible_labels()
    }
    fn destroy(&self, _label: &str) -> Result<(), String> {
        Err("Destroy unsupported".into())
    }
    fn hide(&self, label: &str) -> Result<(), String>;
    fn show(&self, label: &str) -> Result<(), String>;
    fn unminimize(&self, label: &str) -> Result<(), String>;
    fn focus(&self, label: &str) -> Result<(), String>;
    fn background_resources(&self, hidden: bool);
    fn tray_available(&self) -> bool {
        true
    }
    fn minimize(&self, _label: &str) -> Result<(), String> {
        Ok(())
    }
    fn tray_status(&self, _text: &str, _attention: bool) {}
}

#[cfg(any(feature = "desktop", test))]
pub(crate) fn enter_background(state: &AppState, windows: &impl Windows) -> Result<(), String> {
    let mut visibility = state.background_visibility.lock();
    if visibility.saved.is_some() {
        return Ok(());
    }
    if state.config.read().background.tray_icon == super::TrayPreference::Never
        || !windows.tray_available()
    {
        // Keep both the window and desktop terminal visibility reachable without a tray.
        return windows.minimize("main");
    }
    let labels = windows.visible_labels();
    // Main first: a failure must leave the desktop and its terminal state intact.
    windows.hide("main")?;
    visibility.windows = labels;
    for label in &visibility.windows {
        if label != "main"
            && let Err(error) = windows.hide(label)
        {
            tracing::warn!(%label, %error, "Cannot hide panel");
        }
    }
    visibility.saved = Some(
        state
            .session_maps
            .session_visibility
            .iter()
            .map(|e| {
                (
                    e.key().clone(),
                    visibility
                        .desktop
                        .get(e.key())
                        .copied()
                        .unwrap_or(*e.value()),
                )
            })
            .collect(),
    );
    let ids: Vec<String> = state
        .session_maps
        .session_visibility
        .iter()
        .map(|e| e.key().clone())
        .collect();
    for id in ids {
        visibility.refresh(state, &id);
    }
    state.desktop_window_focused.store(false, Ordering::Relaxed);
    windows.background_resources(true);
    drop(visibility);
    refresh_tray(state, windows);
    super::unload::notify(state, "hidden");
    Ok(())
}

#[cfg(any(feature = "desktop", test))]
pub(crate) fn leave_background(state: &AppState, windows: &impl Windows) -> Result<(), String> {
    let mut visibility = state.background_visibility.lock();
    // Keep the saved record on failure so another Show can retry.
    windows.ensure_main()?;
    windows.background_resources(false);
    windows.show("main")?;
    windows.unminimize("main")?;
    if let Some(saved) = visibility.saved.take() {
        for (id, visible) in saved {
            visibility.desktop.insert(id.clone(), visible);
            if let Some(mut current) = state.session_maps.session_visibility.get_mut(&id) {
                *current = visible;
            } else {
                visibility.desktop.remove(&id);
                continue;
            }
            visibility.refresh(state, &id);
        }
        for label in visibility.windows.drain(..) {
            if label != "main"
                && let Err(error) = windows.show(&label)
            {
                tracing::warn!(%label, %error, "Cannot restore panel");
            }
        }
    }
    visibility.attention = false;
    drop(visibility);
    refresh_tray(state, windows);
    windows.focus("main")?;
    state.frontend_liveness.rebaseline();
    state.webview_unloaded.store(false, Ordering::Release);
    super::unload::notify(state, "visible");
    Ok(())
}

pub(crate) fn is_hidden(state: &AppState) -> bool {
    state.background_visibility.lock().saved.is_some()
}

#[cfg(any(feature = "desktop", test))]
pub(crate) fn refresh_tray(state: &AppState, windows: &impl Windows) {
    let (text, awaiting) = super::tray::status(state);
    let mut visibility = state.background_visibility.lock();
    if visibility.saved.is_some() && awaiting {
        visibility.attention = true;
    }
    let attention = visibility.attention;
    drop(visibility);
    windows.tray_status(&text, attention);
}

#[cfg(test)]
mod activity_tests {
    use super::*;

    // Catches: observation grace never expires, or cleanup leaves an old session protected.
    #[test]
    fn remote_observation_grace_expires_at_timeout_and_is_forgotten() {
        let state = crate::state::tests_support::make_test_app_state();
        state.config.write().standby_timeout_minutes = 1;
        let mut visibility = state.background_visibility.lock();
        let now = std::time::Instant::now();
        visibility.last_remote_activity.insert("viewed".into(), now);
        assert!(visibility.recently_active(&state, "viewed"));
        assert!(!visibility.recently_active(&state, "other"));
        visibility
            .last_remote_activity
            .insert("viewed".into(), now - std::time::Duration::from_secs(60));
        assert!(!visibility.recently_active(&state, "viewed"));
        visibility.last_remote_activity.insert("viewed".into(), now);
        drop(visibility);
        forget_session(&state, "viewed");
        assert!(
            !state
                .background_visibility
                .lock()
                .recently_active(&state, "viewed")
        );
    }

    // Catches (audit C2): a crashed remote viewer's `visible: true` claim
    // holding its session visible, and out of standby, until the session closes.
    #[test]
    fn an_expired_remote_claim_stops_pinning_visibility() {
        let state = crate::state::tests_support::make_test_app_state();
        crate::state::tests_support::insert_dummy_session(&state, "s1");
        set_visible_for_viewer(&state, "s1", true, false, "crashed");
        set_visible_for_viewer(&state, "s1", true, false, "live");
        assert!(state.is_session_visible("s1"));

        let stale = std::time::Instant::now() - REMOTE_CLAIM_TTL;
        let backdate = |viewer: &str| {
            state
                .background_visibility
                .lock()
                .remote_visible
                .get_mut("s1")
                .unwrap()
                .insert(viewer.into(), stale);
        };
        backdate("crashed");
        assert_eq!(expire_remote_claims(&state), 1);
        assert_eq!(remote_viewers_for_test(&state, "s1"), vec!["live"]);
        assert!(
            state.is_session_visible("s1"),
            "the live viewer still counts"
        );

        backdate("live");
        assert!(
            !state.background_visibility.lock().watched(&state, "s1"),
            "an expired claim no longer counts even before the sweep"
        );
        assert_eq!(expire_remote_claims(&state), 1);
        assert!(remote_viewers_for_test(&state, "s1").is_empty());
        assert!(!state.is_session_visible("s1"));
        assert_eq!(expire_remote_claims(&state), 0);
        crate::pty::close_pty_core(&state, "s1", false);
    }

    // Catches: a new claim keeping expired siblings, so one crashed viewer
    // occupies a slot of the per-session cap forever.
    #[test]
    fn a_new_claim_drops_expired_siblings() {
        let state = crate::state::tests_support::make_test_app_state();
        crate::state::tests_support::insert_dummy_session(&state, "s1");
        set_visible_for_viewer(&state, "s1", true, false, "crashed");
        state
            .background_visibility
            .lock()
            .remote_visible
            .get_mut("s1")
            .unwrap()
            .insert(
                "crashed".into(),
                std::time::Instant::now() - REMOTE_CLAIM_TTL,
            );
        set_visible_for_viewer(&state, "s1", true, false, "fresh");
        assert_eq!(remote_viewers_for_test(&state, "s1"), vec!["fresh"]);
        crate::pty::close_pty_core(&state, "s1", false);
    }

    // Catches: an all-session SSE disconnect omits grace or protects sessions forever.
    #[test]
    fn global_observation_grace_expires_and_zero_timeout_disables_grace() {
        let state = crate::state::tests_support::make_test_app_state();
        state.config.write().standby_timeout_minutes = 1;
        let mut visibility = state.background_visibility.lock();
        let now = std::time::Instant::now();
        visibility.last_global_observation = Some(now);
        assert!(visibility.recently_active(&state, "any-session"));
        visibility.last_global_observation = Some(now - std::time::Duration::from_secs(60));
        assert!(!visibility.recently_active(&state, "any-session"));
        visibility.last_global_observation = Some(now);
        state.config.write().standby_timeout_minutes = 0;
        assert!(!visibility.recently_active(&state, "any-session"));
    }
}
