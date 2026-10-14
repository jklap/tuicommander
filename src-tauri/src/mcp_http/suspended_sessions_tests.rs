fn suspend_shell(state: &Arc<AppState>, cwd: &str) -> String {
    let sid = uuid::Uuid::new_v4().to_string();
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new("/bin/sh");
    command.arg("-i");
    command.cwd(cwd);
    let child = pair.slave.spawn_command(command).unwrap();
    let writer = pair.master.take_writer().unwrap();
    state.session_maps.sessions.insert(
        sid.clone(),
        parking_lot::Mutex::new(crate::state::PtySession {
            launch_receipt: None,
            writer: Arc::new(parking_lot::Mutex::new(writer)),
            master: pair.master,
            _child: child,
            paused: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            worktree: None,
            initial_cwd: Some(cwd.into()),
            cwd: Some(cwd.into()),
            display_name: Some("Durable shell".into()),
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: false,
            shell: "/bin/sh".into(),
        }),
    );
    state
        .metrics
        .active_sessions
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    state.bind_live_pty(&sid, &sid);
    state
        .session_maps
        .session_states
        .insert(sid.clone(), Default::default());
    state.session_maps.shell_states.insert(
        sid.clone(),
        std::sync::atomic::AtomicU8::new(crate::pty::SHELL_IDLE),
    );
    sid
}

// Catches: the backend refusing to end an idle PTY when no WebView or SSE client exists.
#[tokio::test]
async fn suspend_works_with_no_ui_attached() {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().into());
    let state = test_state();
    let sid = suspend_shell(&state, root.path().to_str().unwrap());
    let pid = state
        .session_maps
        .sessions
        .get(&sid)
        .unwrap()
        .lock()
        ._child
        .process_id()
        .unwrap();
    assert!(
        state
            .session_maps
            .sessions
            .get(&sid)
            .unwrap()
            .lock()
            ._child
            .try_wait()
            .unwrap()
            .is_none()
    );
    let result = suspend(&state, &sid).await;
    let still_live = state.session_maps.sessions.contains_key(&sid);
    crate::pty::close_pty_core_with_reason(&state, &sid, false, "test_cleanup");
    assert_eq!(result, serde_json::json!({"ok": true}));
    assert!(!still_live, "successful suspend must actually end the PTY");
    // SAFETY: signal zero only probes the fixture child; it sends no signal.
    assert_eq!(
        unsafe { libc::kill(pid as libc::pid_t, 0) },
        -1,
        "the shell process must have exited"
    );
}

// Catches: reporting a successful suspend without durable data for the next frontend.
#[tokio::test]
async fn suspended_record_survives_restart_and_frontend_renders_it() {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().into());
    let state = test_state();
    let sid = suspend_shell(&state, root.path().to_str().unwrap());
    let result = suspend(&state, &sid).await;
    crate::pty::close_pty_core_with_reason(&state, &sid, false, "test_cleanup");
    assert_eq!(result, serde_json::json!({"ok": true}));
    drop(state);
    let bytes = std::fs::read(root.path().join("suspended-sessions.json")).unwrap();
    let records: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(records[&sid]["tuic_session"], sid);
    assert_eq!(records[&sid]["cwd"], root.path().to_str().unwrap());
    assert_eq!(records[&sid]["reason"], "user");
    let restarted = test_state();
    let rendered = crate::mcp_http::suspended_routes::dispatch(
        restarted,
        serde_json::json!({"action": "list"}),
    )
    .await
    .unwrap();
    assert_eq!(rendered[&sid]["name"], "Durable shell");
    assert_eq!(rendered[&sid]["tuic_session"], sid);
}

// Catches: moving suspend to Rust either bypasses busy guards or refuses idle shells.
#[tokio::test]
async fn refusal_rules_unchanged() {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().into());
    let state = test_state();
    let sid = suspend_shell(&state, root.path().to_str().unwrap());
    for (candidate, reason) in [
        (
            crate::state::SessionState {
                awaiting_input: true,
                ..Default::default()
            },
            "waiting for input",
        ),
        (
            crate::state::SessionState {
                agent_type: Some("claude".into()),
                background_work: true,
                ..Default::default()
            },
            "agent working",
        ),
        (
            crate::state::SessionState {
                agent_type: Some("claude".into()),
                foreground_input_blocked: true,
                ..Default::default()
            },
            "agent working",
        ),
    ] {
        state
            .session_maps
            .session_states
            .insert(sid.clone(), candidate);
        assert_eq!(
            suspend(&state, &sid).await["error"],
            format!("Cannot suspend: {reason}")
        );
        assert!(state.session_maps.sessions.contains_key(&sid));
    }
    state
        .session_maps
        .session_states
        .insert(sid.clone(), Default::default());
    let result = suspend(&state, &sid).await;
    crate::pty::close_pty_core_with_reason(&state, &sid, false, "test_cleanup");
    assert_eq!(result, serde_json::json!({"ok": true}));
}

// Catches: losing a live session when durable storage fails before the close.
#[tokio::test]
async fn suspend_storage_failure_keeps_the_live_pty() {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().into());
    let state = test_state();
    let sid = suspend_shell(&state, root.path().to_str().unwrap());
    std::fs::write(root.path().join("suspended-sessions.json"), "broken JSON").unwrap();
    let result = suspend(&state, &sid).await;
    let live = state.session_maps.sessions.contains_key(&sid);
    crate::pty::close_pty_core_with_reason(&state, &sid, false, "test_cleanup");
    assert!(result.get("error").is_some());
    assert!(live);
}

// Catches: a backend resume losing the stable peer identity or leaving no real PTY.
#[tokio::test]
async fn suspended_shell_resumes_from_disk_with_the_same_identity() {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().into());
    let state = test_state();
    let sid = suspend_shell(&state, root.path().to_str().unwrap());
    assert_eq!(suspend(&state, &sid).await, serde_json::json!({"ok": true}));
    drop(state);
    let restarted = test_state();
    let result = crate::mcp_http::suspended_routes::dispatch(
        restarted.clone(),
        serde_json::json!({"action": "resume", "tuicSession": sid}),
    )
    .await
    .unwrap();
    let session = result["session_id"].as_str().unwrap();
    let is_live = restarted.session_maps.sessions.contains_key(session);
    let identity = restarted
        .session_maps
        .live_pty_by_tuic_session
        .get(&sid)
        .map(|entry| entry.value().clone());
    let records = crate::suspended_sessions::list().unwrap();
    crate::pty::close_pty_core_with_reason(&restarted, session, false, "test_cleanup");
    assert!(is_live);
    assert_eq!(identity.as_deref(), Some(session));
    assert!(!records.contains_key(&sid));
}

// Catches: Resume reuses a tombstoned PTY key and the next Suspend rejects the live shell as exited.
#[tokio::test]
async fn resumed_idle_shell_can_suspend_again_without_inheriting_the_old_exit() {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().to_path_buf());
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    let id = crate::mcp_http::session::spawn_pty_session(
        state.clone(),
        crate::test_support::host_shell().0.into(),
        Some(root.path().to_string_lossy().into_owned()),
        24,
        80,
        None,
        crate::mcp_http::session::RequestedIdentity::default(),
    )
    .expect("real idle shell must start");
    let record = crate::suspended_sessions::suspend(&state, &id, &serde_json::Value::Null)
        .expect("first suspend must work");
    let resumed = crate::mcp_http::suspended_routes::dispatch(
        state.clone(),
        serde_json::json!({"action": "resume", "tuicSession": record.tuic_session}),
    )
    .await
    .expect("resume must work");
    let resumed_id = resumed["session_id"].as_str().unwrap();
    let second = crate::suspended_sessions::suspend(&state, resumed_id, &serde_json::Value::Null);
    crate::pty::close_pty_core_with_reason(&state, resumed_id, false, "test_cleanup");
    assert!(
        second.is_ok(),
        "resumed idle shell must suspend again: {second:?}"
    );
}
