//! Deterministic TUIC-side Stop race; no Bot API calls or real credentials.
use super::{Active, Outbound};
use crate::state::AppState;
use serde_json::json;
use std::sync::Arc;

// Catches: Stop checks epoch N, then writes Esc into newly submitted epoch N+1.
#[tokio::test]
async fn stop_does_not_interrupt_a_replacement_turn_after_epoch_check() {
    let dir = tempfile::Builder::new()
        .prefix("tg-stop-critic")
        .tempdir_in(tuic_test_support::test_temp_root())
        .unwrap();
    let paths = super::super::Paths::new(dir.path().to_path_buf());
    let allowlist = paths.file("allowed_chat_ids");
    std::fs::write(&allowlist, "1111111\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::set_permissions(&allowlist, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let state = Arc::new(AppState::new(
        dir.path().to_path_buf(),
        dir.path().join("worktrees"),
        crate::config::AppConfig::default(),
        Arc::new(parking_lot::Mutex::new(
            crate::app_logger::LogRingBuffer::new(10),
        )),
    ));
    let pty = "telegram-critic-owned-pty";
    let bytes = crate::test_support::insert_recording_session(&state, pty);
    crate::test_support::agent_session(&state, pty, crate::pty::SHELL_BUSY);
    crate::pty::note_submitted_input(&state, pty);
    let epoch = state.session_state_with_shell(pty).unwrap().turn_epoch;
    let mut outbound = Outbound::new(paths.clone(), super::super::BotApi::new(paths).unwrap());
    // Active is TUIC-owned correlation state. The input below models the parsed
    // Stop boundary only; it makes no claim about live Telegram wire behavior.
    outbound.active = Some(Active {
        request: "critic-request".into(),
        peer: pty.into(),
        pty: pty.into(),
        epoch,
        chat: 1111111,
        draft: 7,
        text: String::new(),
        sent: tokio::time::Instant::now(),
        dirty: false,
    });
    let stop =
        json!({"stopped_message_generation":{"chat":{"id":1111111,"type":"private"},"draft_id":7}});
    let result = outbound.stop(
        &stop,
        |_, target, expected| {
            let matched = state.session_state_with_shell(target).unwrap().turn_epoch == expected;
            // This is the legal concurrent schedule: a new real submission
            // completes after the snapshot, before the native writer acquires
            // its lock. No hand-set epoch and no wall-clock race are needed.
            crate::pty::note_submitted_input(&state, target);
            matched
        },
        |target| {
            crate::mcp_http::session::write_pty_input(&state, target, "\u{1b}")
                .map_err(|_| super::super::Error::State)
        },
    );
    // Reap only the fixture child we started, even when the assertion fails.
    {
        let session = state.session_maps.sessions.get(pty).unwrap();
        let mut session = session.lock();
        session._child.kill().unwrap();
        session._child.wait().unwrap();
    }
    assert_eq!(
        state.session_state_with_shell(pty).unwrap().turn_epoch,
        epoch + 1
    );
    assert!(
        bytes.lock().unwrap().is_empty(),
        "stale Telegram Stop wrote Esc into the replacement turn: {result:?}"
    );
}
