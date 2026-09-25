//! Application adapter for the terminal PTY capture engine.

use std::path::PathBuf;

use crate::AppState;
use crate::state::AppEvent;

#[cfg(test)]
pub(crate) use tuic_terminal::pty_capture::{CaptureDirection, decode, decode_capture};
pub(crate) use tuic_terminal::pty_capture::{
    capture_status as status, is_enabled, record_input, record_input_with_geometry,
    record_with_geometry, set_enabled,
};

/// Start or stop the tap in the configured capture directory and report state.
///
/// Every entry point (the Tauri command, `POST /diagnostics/capture`, a raw
/// curl POST from outside the app) goes through here, so this is the single
/// emitter of `AppEvent::PtyCaptureChanged` — a tab-bar "recording" badge
/// learns about a toggle no matter who flipped it. (The capture engine itself
/// lives in `tuic_terminal::pty_capture` and has no `AppState`.)
pub(crate) fn set_enabled_in_config_dir(
    state: &AppState,
    enabled: bool,
    session_filter: Option<String>,
) -> serde_json::Value {
    let announce = |enabled: bool, session_filter: Option<String>| {
        state.emit_dual(AppEvent::PtyCaptureChanged {
            enabled,
            session_filter,
        });
    };
    if !enabled {
        set_enabled(false, None, PathBuf::new());
        announce(false, None);
        return status();
    }

    let dir = match std::env::var_os("TUIC_CAPTURE_DIR") {
        Some(value) => {
            let dir = PathBuf::from(value);
            if !dir.is_absolute() {
                set_enabled(false, None, PathBuf::new());
                announce(false, None);
                return serde_json::json!({
                    "enabled": false,
                    "error": "TUIC_CAPTURE_DIR must be absolute",
                });
            }
            dir
        }
        None => crate::config::config_dir().join("captures"),
    };
    set_enabled(true, session_filter.clone(), dir);
    announce(true, session_filter);
    status()
}

/// Toggle PTY capture from the desktop app.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn set_pty_capture(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    enabled: bool,
    session_id: Option<String>,
) -> serde_json::Value {
    set_enabled_in_config_dir(&state, enabled, session_id)
}

/// Read PTY capture state from the desktop app.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_pty_capture() -> serde_json::Value {
    status()
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn capture_directory_override_selects_an_absolute_directory() {
        let _guard = TEST_LOCK.lock();
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(root.path().join("config"));
        let selected_dir = root.path().join("selected");
        let state = crate::state::tests_support::make_test_app_state();

        let previous = std::env::var_os("TUIC_CAPTURE_DIR");
        unsafe { std::env::remove_var("TUIC_CAPTURE_DIR") };
        let default_status = set_enabled_in_config_dir(&state, true, Some("session-a".into()));
        assert_eq!(
            default_status["dir"],
            serde_json::json!(
                root.path()
                    .join("config")
                    .join("captures")
                    .display()
                    .to_string()
            )
        );

        unsafe { std::env::set_var("TUIC_CAPTURE_DIR", &selected_dir) };
        let selected_status = set_enabled_in_config_dir(&state, true, Some("session-a".into()));
        assert_eq!(
            selected_status["dir"],
            serde_json::json!(selected_dir.display().to_string())
        );
        record_with_geometry("session-a", b"ready", Some((24, 80)));
        assert!(selected_dir.join("session-a.tcap").exists());

        set_enabled_in_config_dir(&state, false, None);
        unsafe { std::env::set_var("TUIC_CAPTURE_DIR", "relative") };
        let invalid_status = set_enabled_in_config_dir(&state, true, None);
        assert_eq!(invalid_status["enabled"], false);
        assert_eq!(invalid_status["error"], "TUIC_CAPTURE_DIR must be absolute");

        match previous {
            Some(value) => unsafe { std::env::set_var("TUIC_CAPTURE_DIR", value) },
            None => unsafe { std::env::remove_var("TUIC_CAPTURE_DIR") },
        }
    }

    /// A tab-bar "recording" badge has to learn about a capture toggle
    /// regardless of who flipped it — the Tauri command, the HTTP route, or
    /// a raw curl POST from outside the app — so the one app-side mutation
    /// point must be the emitter, not each caller.
    #[test]
    fn set_enabled_in_config_dir_emits_pty_capture_changed() {
        let _guard = TEST_LOCK.lock();
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(root.path().join("config"));
        let previous = std::env::var_os("TUIC_CAPTURE_DIR");
        unsafe { std::env::remove_var("TUIC_CAPTURE_DIR") };
        let state = crate::state::tests_support::make_test_app_state();
        let mut rx = state.event_bus.subscribe();

        set_enabled_in_config_dir(&state, true, Some("s".into()));
        match rx.try_recv().expect("PtyCaptureChanged on start") {
            AppEvent::PtyCaptureChanged {
                enabled,
                session_filter,
            } => {
                assert!(enabled);
                assert_eq!(session_filter, Some("s".to_string()));
            }
            other => panic!("expected PtyCaptureChanged, got {other:?}"),
        }

        set_enabled_in_config_dir(&state, false, None);
        match rx.try_recv().expect("PtyCaptureChanged on stop") {
            AppEvent::PtyCaptureChanged {
                enabled,
                session_filter,
            } => {
                assert!(!enabled);
                assert_eq!(session_filter, None);
            }
            other => panic!("expected PtyCaptureChanged, got {other:?}"),
        }

        if let Some(value) = previous {
            unsafe { std::env::set_var("TUIC_CAPTURE_DIR", value) };
        }
    }
}
