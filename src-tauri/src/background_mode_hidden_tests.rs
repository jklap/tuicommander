use crate::background_mode::*;

// Window adapter state is observable here; native API behavior is checked after restart.
#[derive(Default)]
struct TestWindows(std::cell::RefCell<std::collections::BTreeMap<String, bool>>);

impl TestWindows {
    fn new() -> Self {
        Self(std::cell::RefCell::new(std::collections::BTreeMap::from([
            ("main".into(), true),
            ("panel-git".into(), true),
            ("panel-closed".into(), false),
        ])))
    }
    fn visible(&self, label: &str) -> bool {
        self.0.borrow()[label]
    }
}
impl crate::background_mode::hidden::Windows for TestWindows {
    fn visible_labels(&self) -> Vec<String> {
        self.0
            .borrow()
            .iter()
            .filter(|(_, v)| **v)
            .map(|(k, _)| k.clone())
            .collect()
    }
    fn show(&self, label: &str) -> Result<(), String> {
        self.0.borrow_mut().insert(label.into(), true);
        Ok(())
    }
    fn background_resources(&self, _hidden: bool) {}

    fn hide(&self, label: &str) -> Result<(), String> {
        self.0.borrow_mut().insert(label.into(), false);
        Ok(())
    }
    fn unminimize(&self, _label: &str) -> Result<(), String> {
        Ok(())
    }
    fn focus(&self, _label: &str) -> Result<(), String> {
        Ok(())
    }
}

// Catches: the hidden active terminal remains visible to standby and suppresses phone push.
#[test]
fn entering_background_marks_sessions_hidden_and_desktop_unfocused() {
    use std::sync::atomic::Ordering;
    let state = crate::state::tests_support::make_test_app_state();
    state
        .session_maps
        .session_visibility
        .insert("active".into(), true);
    state
        .session_maps
        .session_visibility
        .insert("inactive".into(), false);
    state.desktop_window_focused.store(true, Ordering::Relaxed);
    let windows = TestWindows::new();
    hidden::enter_background(&state, &windows).unwrap();
    assert!(!*state.session_maps.session_visibility.get("active").unwrap());
    assert!(!state.desktop_window_focused.load(Ordering::Relaxed));
    assert!(!windows.visible("main"));
    assert!(!windows.visible("panel-git"));
}

// Catches: show restores a window but leaves its terminal SIGSTOPped.
#[cfg(unix)]
#[test]
fn show_restores_previous_visibility_and_wakes_visible_session() {
    let state = crate::state::tests_support::make_test_app_state();
    crate::state::tests_support::insert_dummy_session(&state, "active");
    struct Cleanup<'a>(&'a crate::AppState);
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            crate::pty::close_pty_core(self.0, "active", false);
        }
    }
    let _cleanup = Cleanup(&state);
    state
        .session_maps
        .session_visibility
        .insert("active".into(), true);
    state
        .session_maps
        .session_visibility
        .insert("inactive".into(), false);
    let windows = TestWindows::new();
    hidden::enter_background(&state, &windows).unwrap();
    assert_eq!(crate::pty::standby_session(&state, "active"), Ok(true));
    let pgid = state
        .session_maps
        .sessions
        .get("active")
        .unwrap()
        .lock()
        .master
        .process_group_leader()
        .unwrap();
    let mut status = 0;
    assert_eq!(
        unsafe { libc::waitpid(pgid, &mut status, libc::WUNTRACED) },
        pgid
    );
    assert!(libc::WIFSTOPPED(status));
    hidden::leave_background(&state, &windows).unwrap();
    assert!(!state.session_maps.standby_sessions.contains_key("active"));
    assert_eq!(
        unsafe { libc::waitpid(pgid, &mut status, libc::WCONTINUED) },
        pgid
    );
    assert!(
        libc::WIFCONTINUED(status),
        "the real child must receive SIGCONT"
    );
    assert!(*state.session_maps.session_visibility.get("active").unwrap());
    assert!(
        !*state
            .session_maps
            .session_visibility
            .get("inactive")
            .unwrap()
    );
    assert!(windows.visible("main"));
    assert!(windows.visible("panel-git"));
    assert!(!windows.visible("panel-closed"));
}

// Catches: background defaults disappear from the wire/catalogue or the user cannot keep a Dock icon.
#[test]
fn background_mode_dock_preference_defaults_and_round_trips() {
    let mut config = serde_json::to_value(crate::config::AppConfig::default()).unwrap();
    assert_eq!(config["background"]["hide_dock_icon"], true);
    config["background"]["hide_dock_icon"] = false.into();
    let saved: crate::config::AppConfig = serde_json::from_value(config).unwrap();
    assert!(!saved.background.hide_dock_icon);
}

// Catches: repeat hide overwrites the saved foreground terminal, or a hidden UI focus event wakes it.
#[test]
fn background_mode_repeat_hide_ignores_hidden_desktop_visibility_and_restores_once() {
    let state = crate::state::tests_support::make_test_app_state();
    hidden::set_visible(&state, "active", true, true);
    hidden::set_visible(&state, "inactive", false, true);
    let windows = TestWindows::new();
    hidden::enter_background(&state, &windows).unwrap();
    hidden::set_visible(&state, "inactive", true, true);
    hidden::enter_background(&state, &windows).unwrap();
    assert!(
        !*state
            .session_maps
            .session_visibility
            .get("inactive")
            .unwrap()
    );
    hidden::leave_background(&state, &windows).unwrap();
    hidden::leave_background(&state, &windows).unwrap();
    assert!(*state.session_maps.session_visibility.get("active").unwrap());
    assert!(
        !*state
            .session_maps
            .session_visibility
            .get("inactive")
            .unwrap()
    );
}

// Catches: one disconnect removes another observer's protection, or closed sessions reappear on show.
#[test]
fn background_mode_remote_watchers_survive_hide_and_release_independently() {
    use std::sync::Arc;
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    hidden::set_visible(&state, "active", true, true);
    hidden::set_visible(&state, "watched", false, true);
    let windows = TestWindows::new();
    let first = hidden::RemoteWatch::new(state.clone(), Some("watched".into()));
    hidden::enter_background(&state, &windows).unwrap();
    assert!(
        *state
            .session_maps
            .session_visibility
            .get("watched")
            .unwrap()
    );
    assert!(!*state.session_maps.session_visibility.get("active").unwrap());
    let second = hidden::RemoteWatch::new(state.clone(), Some("watched".into()));
    drop(first);
    assert!(
        *state
            .session_maps
            .session_visibility
            .get("watched")
            .unwrap()
    );
    drop(second);
    assert!(
        !*state
            .session_maps
            .session_visibility
            .get("watched")
            .unwrap()
    );
    state.session_maps.session_visibility.remove("active");
    hidden::forget_session(&state, "active");
    hidden::leave_background(&state, &windows).unwrap();
    assert!(!state.session_maps.session_visibility.contains_key("active"));
    assert!(
        !*state
            .session_maps
            .session_visibility
            .get("watched")
            .unwrap()
    );
}

// Catches: the desktop IPC path (`desktop: true`, what `pty::set_session_visible` passes)
// recording a terminal-store id ("term-6") that names no PTY session. Teardown only reaps
// entries of real sessions, so a recorded one would never be removed.
#[test]
fn set_visible_from_client_desktop_path_ignores_an_id_that_names_no_session() {
    let state = crate::state::tests_support::make_test_app_state();
    for visible in [true, false] {
        crate::background_mode::hidden::set_visible_from_client(&state, "term-6", visible, true);
        assert!(
            !state.session_maps.session_visibility.contains_key("term-6"),
            "desktop visibility for a non-session id must not be recorded (visible={visible})"
        );
    }
}

// Catches: a `visible: true` for an unknown id passing silently. It looks exactly like a
// wake of a session that is simply not parked, so only the warning reveals the caller bug.
#[test]
fn set_visible_from_client_warns_only_when_showing_an_unknown_id() {
    #[derive(Clone)]
    struct Sink(std::sync::Arc<parking_lot::Mutex<Vec<u8>>>);
    impl std::io::Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Sink {
        type Writer = Sink;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }
    let state = crate::state::tests_support::make_test_app_state();
    for (desktop, visible) in [(true, true), (false, true), (true, false), (false, false)] {
        let output = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(Sink(output.clone()))
            .with_ansi(false)
            .without_time()
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            crate::background_mode::hidden::set_visible_from_client(
                &state, "term-6", visible, desktop,
            );
        });
        let log = String::from_utf8(output.lock().clone()).unwrap();
        let warnings: Vec<_> = log
            .lines()
            .filter(|line| line.contains("set_session_visible for a session that does not exist"))
            .collect();
        assert_eq!(
            warnings.len(),
            usize::from(visible),
            "desktop={desktop} visible={visible}: {log}"
        );
        if visible {
            assert!(warnings[0].contains("WARN"), "{log}");
            assert!(warnings[0].contains("term-6"), "{log}");
        }
        assert!(
            !state.session_maps.session_visibility.contains_key("term-6"),
            "desktop={desktop} visible={visible}: nothing may be recorded"
        );
    }
}

// Catches: the generated `set_session_visible` IPC command (arguments extracted by
// Tauri, not by hand) bypassing the unknown-id guard. Opt-in feature, like
// `settings_mock_ipc_tests`: `cargo nextest run -p tuicommander --lib --features settings-ipc-test`.
#[cfg(feature = "settings-ipc-test")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_session_visible_ipc_command_ignores_an_id_that_names_no_session() {
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    let app = tauri::test::mock_builder()
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![crate::pty::set_session_visible])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "visibility-test", Default::default())
        .build()
        .unwrap();
    for visible in [true, false] {
        let response = tauri::test::get_ipc_response(
            &webview,
            tauri::webview::InvokeRequest {
                cmd: "set_session_visible".into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: if cfg!(any(windows, target_os = "android")) {
                    "http://tauri.localhost"
                } else {
                    "tauri://localhost"
                }
                .parse()
                .unwrap(),
                body: tauri::ipc::InvokeBody::Json(
                    serde_json::json!({"sessionId": "term-6", "visible": visible}),
                ),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_owned(),
            },
        );
        assert!(response.is_ok(), "visible={visible}: {response:?}");
        assert!(
            !state.session_maps.session_visibility.contains_key("term-6"),
            "the IPC command must not record a non-session id (visible={visible})"
        );
    }
}

#[path = "background_mode_stream_tests.rs"]
mod streams;

#[path = "background_mode_tray_tests.rs"]
mod tray_tests;
