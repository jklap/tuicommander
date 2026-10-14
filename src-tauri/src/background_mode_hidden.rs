//! Desktop visibility and remote observation have independent lifetimes.
use crate::AppState;
use std::collections::{HashMap, HashSet};
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
    remote_visible: HashSet<String>,
}

impl Visibility {
    pub(crate) fn watched(&self, state: &AppState, id: &str) -> bool {
        self.all_watchers > 0
            || self.watchers.get(id).copied().unwrap_or(0) > 0
            || self.remote_visible.contains(id)
            || state
                .ws_clients
                .get(id)
                .is_some_and(|clients| clients.iter().any(|tx| !tx.is_closed()))
    }

    fn refresh(&self, state: &AppState, id: &str) {
        let desktop = self.saved.is_none() && self.desktop.get(id).copied().unwrap_or(false);
        let visible = desktop || self.watched(state, id);
        state
            .session_maps
            .session_visibility
            .insert(id.into(), visible);
        wake_visible(state, id, visible);
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

/// IPC visibility belongs to the desktop; HTTP visibility belongs to a remote client.
pub(crate) fn set_visible(state: &AppState, id: &str, visible: bool, desktop: bool) {
    let mut visibility = state.background_visibility.lock();
    if desktop {
        if visibility.saved.is_some() {
            return;
        }
        visibility.desktop.insert(id.into(), visible);
    } else if visible {
        visibility.remote_visible.insert(id.into());
    } else {
        visibility.remote_visible.remove(id);
    }
    visibility.refresh(state, id);
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
            refresh_watch(&state, &visibility, id.as_deref());
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
        refresh_watch(&self.state, &visibility, self.id.as_deref());
    }
}
fn refresh_watch(state: &AppState, visibility: &Visibility, id: Option<&str>) {
    if let Some(id) = id {
        if state.session_maps.session_visibility.contains_key(id)
            || state.session_maps.sessions.contains_key(id)
        {
            visibility.refresh(state, id);
        }
    } else {
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

pub(crate) fn forget_session(state: &AppState, id: &str) {
    let mut visibility = state.background_visibility.lock();
    visibility.remote_visible.remove(id);
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

#[cfg(any(feature = "desktop", test))]
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
