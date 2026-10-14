//! Native window adapter. Calls stay on the desktop event loop.
#[cfg(target_os = "macos")]
use crate::AppState;
#[cfg(target_os = "macos")]
use std::sync::Arc;
use tauri::Manager;

pub(super) struct DesktopWindows<'a>(pub &'a tauri::AppHandle);
impl DesktopWindows<'_> {
    fn window(&self, label: &str) -> Result<tauri::WebviewWindow, String> {
        self.0
            .get_webview_window(label)
            .ok_or_else(|| format!("Window missing: {label}"))
    }
}
impl super::super::hidden::Windows for DesktopWindows<'_> {
    fn visible_labels(&self) -> Vec<String> {
        self.0
            .webview_windows()
            .into_iter()
            .filter(|(_, window)| window.is_visible().unwrap_or(false))
            .map(|(label, _)| label)
            .collect()
    }
    fn hide(&self, label: &str) -> Result<(), String> {
        self.window(label)?.hide().map_err(|e| e.to_string())
    }
    fn show(&self, label: &str) -> Result<(), String> {
        self.window(label)?.show().map_err(|e| e.to_string())
    }
    fn unminimize(&self, label: &str) -> Result<(), String> {
        self.window(label)?.unminimize().map_err(|e| e.to_string())
    }
    fn focus(&self, label: &str) -> Result<(), String> {
        self.window(label)?.set_focus().map_err(|e| e.to_string())
    }
    fn background_resources(&self, hidden: bool) {
        #[cfg(target_os = "macos")]
        {
            let state = self.0.state::<Arc<AppState>>();
            if !hidden || state.config.read().background.hide_dock_icon {
                let policy = if hidden {
                    tauri::ActivationPolicy::Accessory
                } else {
                    tauri::ActivationPolicy::Regular
                };
                if let Err(error) = self.0.set_activation_policy(policy) {
                    tracing::warn!(%error, "Cannot change Dock visibility");
                }
            }
        }
        #[cfg(windows)]
        for window in self.0.webview_windows().into_values() {
            if let Err(error) = window.with_webview(move |view| {
                use webview2_com::Microsoft::Web::WebView2::Win32::{
                    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
                    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL, ICoreWebView2_19,
                };
                use windows_core::Interface;
                // Older WebView2 runtimes do not implement this optional interface.
                let result = unsafe { view.controller().CoreWebView2() }
                    .and_then(|webview| webview.cast::<ICoreWebView2_19>())
                    .and_then(|webview| unsafe {
                        webview.SetMemoryUsageTargetLevel(if hidden {
                            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
                        } else {
                            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
                        })
                    });
                if let Err(error) = result {
                    tracing::debug!(%error, "WebView memory hint unavailable");
                }
            }) {
                tracing::warn!(%error, "Cannot schedule WebView memory hint");
            }
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        let _ = hidden;
    }
}
