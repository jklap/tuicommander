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

// Catches: selected markup uses Boolean disabled plus callback_data, violating
// Telegram's DisabledButton object and exactly-one-button-type contract.
// Oracle: https://core.telegram.org/bots/api#inlinekeyboardbutton and
// https://core.telegram.org/bots/api#disabledbutton (read 2026-10-04).
#[tokio::test]
async fn selected_button_uses_disabled_object_without_callback_data() {
    use axum::{Json, Router, extract::State, http::Uri, routing::post};
    use serde_json::Value;
    type Requests = Arc<std::sync::Mutex<Vec<(String, Value)>>>;
    async fn endpoint(
        State(requests): State<Requests>,
        uri: Uri,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        requests.lock().unwrap().push((uri.path().into(), body));
        // These responses only let the TUIC producer reach its edit operation;
        // they do not validate wire payloads or claim live Telegram equivalence.
        Json(
            json!({"ok":true,"result":if uri.path().ends_with("/sendMessage") {
            json!({"message_id":7})
        } else { json!(true) }}),
        )
    }
    let dir = tempfile::Builder::new()
        .prefix("tg-button-critic")
        .tempdir_in(tuic_test_support::test_temp_root())
        .unwrap();
    let paths = super::super::Paths::new(dir.path().to_path_buf());
    let peer = "11111111-2222-4333-8444-555555555555";
    for (file, text) in [
        ("allowed_chat_ids", "1111111\n".to_string()),
        ("bot.token", "123456:FAKE_critic_token".to_string()),
        (
            "config.json",
            json!({"enabled":true,"bot_alias":"critic","target_tuic_session":peer}).to_string(),
        ),
    ] {
        use std::os::unix::fs::PermissionsExt;
        let path = paths.file(file);
        std::fs::write(&path, text).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let requests = Requests::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = Router::new()
        .route("/{*path}", post(endpoint))
        .with_state(requests.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    state.config.write().disabled_native_tools.clear();
    for (sid, identity, name) in [
        ("critic-target-mcp", peer, "critic-target"),
        (
            "critic-adapter",
            "22222222-2222-4222-8222-222222222222",
            "critic-adapter",
        ),
    ] {
        let registered = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
            &state,
            &json!({"action":"register","tuic_session":identity,"name":name}),
            Some(sid),
            None,
        )
        .await;
        assert!(registered.get("error").is_none(), "{registered}");
    }
    let mut runtime = super::super::runtime::Runtime::new(
        state,
        super::super::Config::load(&paths).unwrap().unwrap(),
        paths.clone(),
        "critic-adapter".into(),
    )
    .unwrap();
    runtime.outbound = Outbound::new(
        paths.clone(),
        super::super::BotApi::loopback(paths, address),
    );
    runtime
        .send_buttons(
            1111111,
            "Choose",
            vec![vec![super::super::tool::Button {
                label: "Yes".into(),
                data: "choice".into(),
            }]],
        )
        .await
        .unwrap();
    let handle =
        requests.lock().unwrap()[0].1["reply_markup"]["inline_keyboard"][0][0]["callback_data"]
            .as_str()
            .unwrap()
            .to_string();
    runtime
        .callback(
            &json!({"callback_query":{"id":"critic-query","from":{"id":1111111},
        "message":{"message_id":7,"date":1,"chat":{"id":1111111,"type":"private"}},"data":handle}}),
        )
        .await
        .unwrap();
    server.abort();
    let requests = requests.lock().unwrap();
    let edit = &requests
        .iter()
        .find(|(path, _)| path.ends_with("/editMessageReplyMarkup"))
        .unwrap()
        .1;
    let button = &edit["reply_markup"]["inline_keyboard"][0][0];
    assert!(
        button["disabled"].is_object(),
        "Telegram DisabledButton must be an object, got {button}"
    );
    assert!(
        button.get("callback_data").is_none(),
        "disabled and callback_data specify two button types: {button}"
    );
}
