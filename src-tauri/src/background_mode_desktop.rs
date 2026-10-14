use super::*;
use crate::{AppState, state::AppEvent};
use std::sync::Arc;
use tauri::{Emitter, Manager};

#[derive(Default)]
pub(crate) struct BackgroundMode(parking_lot::Mutex<CloseState>);

pub(crate) fn close_window(window: &tauri::Window, api: &tauri::CloseRequestApi) {
    let state = window.state::<Arc<AppState>>();
    let config = state.config.read();
    let action = window_close(
        window.label(),
        config.close_behavior,
        state.session_maps.sessions.len(),
        config.confirm_before_quit,
    );
    drop(config);
    if let Some(action) = action {
        api.prevent_close();
        apply(window.app_handle(), action);
    }
}

#[tauri::command]
pub(crate) fn request_quit(app: tauri::AppHandle) {
    {
        let mode = app.state::<BackgroundMode>();
        let mut close = mode.0.lock();
        if close.pending == Some(CloseAction::Ask) {
            close.pending = None;
        }
    }
    let state = app.state::<Arc<AppState>>();
    let confirm = state.config.read().confirm_before_quit;
    apply(
        &app,
        decide(
            CloseBehavior::Quit,
            state.session_maps.sessions.len(),
            confirm,
        ),
    );
}

pub(crate) fn is_hidden(app: &tauri::AppHandle) -> bool {
    app.state::<BackgroundMode>().0.lock().hidden
}

pub(crate) fn shown(app: &tauri::AppHandle) {
    app.state::<BackgroundMode>().0.lock().hidden = false;
}

fn apply(app: &tauri::AppHandle, action: CloseAction) {
    let mode = app.state::<BackgroundMode>();
    let mut close = mode.0.lock();
    apply_locked(app, action, &mut close);
}

fn apply_locked(app: &tauri::AppHandle, action: CloseAction, close: &mut CloseState) {
    if close.quitting {
        return;
    }
    // Re-emit a pending prompt on another close: a WebView reload may have lost it.
    let action = close.pending.unwrap_or(action);
    match action {
        CloseAction::Hide => {
            if let Some(window) = app.get_webview_window("main") {
                match window.hide() {
                    Ok(()) => close.hidden = true,
                    Err(error) => tracing::warn!("Cannot hide main window: {error}"),
                }
            }
        }
        CloseAction::Ask | CloseAction::ConfirmQuit => {
            close.pending = Some(action);
            // Explicit quit may come from the menu while the main window is hidden.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                close.hidden = false;
            }
            let state = app.state::<Arc<AppState>>();
            let payload = serde_json::json!({
                "kind": action,
                "sessions": state.session_maps.sessions.len(),
            });
            let _ = app.emit("close-dialog", &payload);
            let _ = state.event_bus.send(AppEvent::CloseDialog { payload });
        }
        CloseAction::Quit => {
            close.quitting = true;
            let app = app.clone();
            let state = app.state::<Arc<AppState>>().inner().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = tokio::task::spawn_blocking(move || {
                    let ids: Vec<String> = state
                        .session_maps
                        .sessions
                        .iter()
                        .map(|s| s.key().clone())
                        .collect();
                    for id in ids {
                        crate::pty::close_pty_core(&state, &id, false);
                    }
                })
                .await
                {
                    tracing::error!("Quit cleanup failed: {error}");
                }
                // Some(0) exit bypasses the implicit-last-window background guard.
                app.exit(0);
            });
        }
    }
}

#[tauri::command]
pub(crate) async fn resolve_close(
    app: tauri::AppHandle,
    choice: CloseChoice,
    remember: bool,
) -> Result<(), String> {
    let handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let mode = handle.state::<BackgroundMode>();
        let mut close = mode.0.lock();
        // Do not consume the dialog on a failed config write.
        let mut next = CloseState {
            hidden: close.hidden,
            quitting: close.quitting,
            pending: close.pending,
        };
        let (action, remembered) = next.resolve(choice, remember)?;
        if let Some(behavior) = remembered {
            let state = handle.state::<Arc<AppState>>();
            crate::config::commit_config_change(&state, |config| {
                let mut next = config.clone();
                next.close_behavior = behavior;
                Ok(next)
            })?;
        }
        *close = next;
        if let Some(action) = action {
            apply_locked(&handle, action, &mut close);
        }
        Ok::<_, String>(())
    })
    .await
    .map_err(|error| error.to_string())??;
    Ok(())
}
