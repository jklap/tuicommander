use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum NativeNoticeTarget {
    Terminal {
        id: String,
    },
    Progress {
        project: String,
        #[serde(rename = "ptyId")]
        pty_id: Option<String>,
    },
}

/// The Tauri plugin sends desktop notices but drops notify-rust's response
/// handle. On macOS retain that handle so a click can open the exact target.
#[tauri::command]
pub(crate) async fn show_native_notification(
    app: tauri::AppHandle,
    title: String,
    body: String,
    target: NativeNoticeTarget,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use tauri::{Emitter, Manager};

        let identifier = app.config().identifier.clone();
        let handle = tauri::async_runtime::spawn_blocking(move || {
            notify_rust::set_application(if tauri::is_dev() {
                "com.apple.Terminal"
            } else {
                &identifier
            })
            .map_err(|error| error.to_string())?;
            notify_rust::Notification::new()
                .summary(&title)
                .body(&body)
                .show()
                .map_err(|error| error.to_string())
        })
        .await
        .map_err(|error| error.to_string())??;

        tauri::async_runtime::spawn_blocking(move || {
            if let Err(error) =
                handle.wait_for_response(|response: &notify_rust::NotificationResponse| {
                    if !matches!(response, notify_rust::NotificationResponse::Default) {
                        return;
                    }
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    let _ = app.emit("native-notification-click", target);
                })
            {
                tracing::warn!(
                    source = "native_notification",
                    "Native notification failed: {error}"
                );
            }
        });
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, title, body, target);
        Err("Click-aware native notifications are available only on macOS".into())
    }
}
