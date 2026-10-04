use super::*;
use crate::telegram::outbound::Outbound;

// Catches: UTF-8 slicing panic, astral overflow or whitespace loss in final replies.
#[tokio::test]
async fn final_chunks_preserve_exact_unicode_and_whitespace() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(
        (1..=4)
            .map(|id| {
                (
                    StatusCode::OK,
                    json!({"ok":true,"result":{"message_id":id}}),
                )
            })
            .collect(),
    )
    .await;
    let mut runtime = runtime(paths, server.address).await;
    let text = format!(" {}\n{}  ", "😀".repeat(2500), "é".repeat(4000));
    runtime
        .tool(
            PEER,
            crate::telegram::tool::Input::Send {
                chat_id: None,
                text: text.clone(),
                buttons: vec![],
            },
        )
        .await
        .unwrap();
    let requests = server.requests();
    let parts: Vec<_> = requests
        .iter()
        .map(|(method, body)| {
            assert!(method.ends_with("sendMessage"));
            body["text"].as_str().unwrap()
        })
        .collect();
    assert_eq!(parts.concat(), text);
    assert!(parts.iter().all(|p| p.encode_utf16().count() <= 4096));
    assert!(
        runtime
            .tool(
                PEER,
                crate::telegram::tool::Input::Send {
                    chat_id: None,
                    text: String::new(),
                    buttons: vec![]
                }
            )
            .await
            .is_err()
    );
    assert_eq!(server.requests().len(), requests.len());
}

// Catches: activity replaces the draft id, finalization leaves refresh running,
// or a revoked destination is sent after the outbound wait.
#[tokio::test]
async fn draft_refresh_and_final_retire_one_request_and_recheck_revocation() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![
        (StatusCode::OK, json!({"ok":true,"result":true})),
        (StatusCode::OK, json!({"ok":true,"result":true})),
        (StatusCode::OK, json!({"ok":true,"result":{"message_id":7}})),
    ])
    .await;
    let mut outbound = Outbound::new(
        paths.clone(),
        BotApi::loopback(paths.clone(), server.address),
    );
    let draft = outbound
        .begin("request".into(), PEER.into(), "pty".into(), 1, 1111111)
        .await
        .unwrap();
    outbound.activity("request", "checking code").unwrap();
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    outbound.refresh().await.unwrap();
    assert_eq!(
        outbound.finish("request", " exact reply\n").await.unwrap(),
        vec![7]
    );
    outbound.refresh().await.unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].1["text"], "");
    assert_eq!(requests[1].1["draft_id"], draft);
    assert!(
        requests[1].1["text"]
            .as_str()
            .unwrap()
            .starts_with("Preview — not approved.")
    );
    assert_eq!(requests[2].1["text"], " exact reply\n");
    write_private(&paths.file("allowed_chat_ids"), "2222222\n");
    assert!(outbound.send(1111111, "revoked", None).await.is_err());
    assert_eq!(server.requests().len(), 3);
}

// Catches: a draft expires when activity does not change for 20 seconds.
#[tokio::test]
async fn unchanged_draft_refreshes_at_twenty_seconds() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![
        (StatusCode::OK, json!({"ok":true,"result":true})),
        (StatusCode::OK, json!({"ok":true,"result":true})),
    ])
    .await;
    let mut outbound = Outbound::new(paths.clone(), BotApi::loopback(paths, server.address));
    outbound
        .begin("request".into(), PEER.into(), "pty".into(), 1, 1111111)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_secs(20)).await;
    outbound.refresh().await.unwrap();
    assert_eq!(server.requests().len(), 2);
    assert_eq!(
        server.requests()[0].1["draft_id"],
        server.requests()[1].1["draft_id"]
    );
}

// Catches: stale/foreign/duplicate Stop interrupts a newer request, or a
// successful Stop leaves a refresh able to resurrect the stopped draft.
#[cfg(unix)]
#[tokio::test]
async fn stop_matches_chat_draft_and_live_epoch_only_once() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![
        (StatusCode::OK, json!({"ok":true,"result":true})),
        (StatusCode::OK, json!({"ok":true,"result":{"message_id":7}})),
    ])
    .await;
    let mut runtime = runtime(paths, server.address).await;
    let bytes = crate::test_support::insert_recording_session(&runtime.state, PEER);
    crate::test_support::agent_session(&runtime.state, PEER, crate::pty::SHELL_BUSY);
    crate::pty::note_submitted_input(&runtime.state, PEER);
    runtime
        .track(crate::telegram::mail::PendingMail {
            id: "request".into(),
            recipient: PEER.into(),
            content: json!({"chat_id":"1111111"}).to_string(),
        })
        .unwrap();
    let begun = runtime
        .tool(
            PEER,
            crate::telegram::tool::Input::Begin {
                request_id: "request".into(),
            },
        )
        .await
        .unwrap();
    let draft = begun["draft_id"].as_i64().unwrap();
    let stopped = |chat, id| json!({"stopped_message_generation":{"chat":{"id":chat,"type":"private"},"draft_id":id}});
    runtime.update(stopped(2222222, draft)).await.unwrap();
    runtime.update(stopped(1111111, draft ^ 1)).await.unwrap();
    assert!(bytes.lock().unwrap().is_empty());
    assert_eq!(server.requests().len(), 1);
    runtime.update(stopped(1111111, draft)).await.unwrap();
    runtime.update(stopped(1111111, draft)).await.unwrap();
    runtime.tick().await;
    let result = bytes.lock().unwrap().clone();
    let requests = server.requests();
    let (_, session) = runtime.state.session_maps.sessions.remove(PEER).unwrap();
    let mut session = session.lock();
    session._child.kill().unwrap();
    session._child.wait().unwrap();
    assert_eq!(result, vec![27]);
    assert_eq!(requests.len(), 2);
    assert!(requests[1].0.ends_with("sendMessage"));
    assert_eq!(requests[1].1["text"], "Stop requested.");
}

// Catches: matching Stop writes into a newer turn that reuses the same PTY.
#[cfg(unix)]
#[tokio::test]
async fn stale_epoch_stop_retires_without_writing() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![(StatusCode::OK, json!({"ok":true,"result":true}))]).await;
    let mut runtime = runtime(paths, server.address).await;
    let bytes = crate::test_support::insert_recording_session(&runtime.state, PEER);
    crate::test_support::agent_session(&runtime.state, PEER, crate::pty::SHELL_BUSY);
    crate::pty::note_submitted_input(&runtime.state, PEER);
    runtime
        .track(crate::telegram::mail::PendingMail {
            id: "request".into(),
            recipient: PEER.into(),
            content: json!({"chat_id":"1111111"}).to_string(),
        })
        .unwrap();
    let begun = runtime
        .tool(
            PEER,
            crate::telegram::tool::Input::Begin {
                request_id: "request".into(),
            },
        )
        .await
        .unwrap();
    crate::pty::note_submitted_input(&runtime.state, PEER);
    runtime.update(json!({"stopped_message_generation":{"chat":{"id":1111111,"type":"private"},"draft_id":begun["draft_id"]}})).await.unwrap();
    runtime.tick().await;
    let result = bytes.lock().unwrap().clone();
    let requests = server.requests();
    let late = runtime
        .tool(
            PEER,
            crate::telegram::tool::Input::Finish {
                request_id: "request".into(),
                text: "too late".into(),
            },
        )
        .await;
    let (_, session) = runtime.state.session_maps.sessions.remove(PEER).unwrap();
    let mut session = session.lock();
    session._child.kill().unwrap();
    session._child.wait().unwrap();
    assert!(result.is_empty());
    assert_eq!(requests.len(), 1);
    assert!(late.is_err());
}

pub(in crate::telegram) async fn runtime(
    paths: Paths,
    address: std::net::SocketAddr,
) -> crate::telegram::runtime::Runtime {
    use crate::mcp_http::mcp_transport::local_peer_call_with_message_id;
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    state.config.write().disabled_native_tools.clear();
    for (sid, peer) in [
        ("adapter-mcp", "22222222-2222-4222-8222-222222222222"),
        ("target-mcp", PEER),
    ] {
        let result = local_peer_call_with_message_id(
            &state,
            &json!({"action":"register","tuic_session":peer,"name":sid}),
            Some(sid),
            None,
        )
        .await;
        assert!(result.get("error").is_none(), "{result}");
    }
    let config = Config::load(&paths).unwrap().unwrap();
    let mut runtime =
        crate::telegram::runtime::Runtime::new(state, config, paths.clone(), "adapter-mcp".into())
            .unwrap();
    runtime.outbound = Outbound::new(paths.clone(), BotApi::loopback(paths, address));
    runtime
}

// Catches: opaque payload is used as wire callback data, a stranger or wrong
// message selects it, or a second button mails a second decision.
#[tokio::test]
async fn buttons_route_one_opaque_choice_through_native_mail_and_retire_keyboard() {
    use crate::telegram::tool::{Button, Input};
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![
        (StatusCode::OK, json!({"ok":true,"result":{"message_id":7}})),
        (StatusCode::OK, json!({"ok":true,"result":true})),
        (StatusCode::OK, json!({"ok":true,"result":true})),
        (StatusCode::OK, json!({"ok":true,"result":true})),
        (StatusCode::OK, json!({"ok":true,"result":true})),
    ])
    .await;
    let mut runtime = runtime(paths, server.address).await;
    let opaque = "opaque:".to_string() + &"x".repeat(100);
    runtime
        .tool(
            PEER,
            Input::Send {
                chat_id: None,
                text: " exact text ".into(),
                buttons: vec![vec![
                    Button {
                        label: "Yes".into(),
                        data: opaque.clone(),
                    },
                    Button {
                        label: "No".into(),
                        data: "no".into(),
                    },
                ]],
            },
        )
        .await
        .unwrap();
    let requests = server.requests();
    let wire = requests[0].1["reply_markup"]["inline_keyboard"][0][0]["callback_data"]
        .as_str()
        .unwrap();
    let other = requests[0].1["reply_markup"]["inline_keyboard"][0][1]["callback_data"]
        .as_str()
        .unwrap();
    assert!(wire.len() <= 64 && wire != opaque);
    let query = |handle: &str, chat, msg, from| json!({"callback_query":{"id":"query","from":{"id":from},"message":{"message_id":msg,"date":1,"chat":{"id":chat,"type":"private"}},"data":handle}});
    for value in [
        query(wire, 2222222, 7, 2222222),
        query(wire, 1111111, 8, 1111111),
        query(wire, 1111111, 7, 2222222),
    ] {
        runtime.callback(&value).await.unwrap();
    }
    assert_eq!(server.requests().len(), 1);
    runtime
        .callback(&query(wire, 1111111, 7, 1111111))
        .await
        .unwrap();
    runtime
        .callback(&query(other, 1111111, 7, 1111111))
        .await
        .unwrap();
    let inbox = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        &runtime.state,
        &json!({"action":"inbox","since":0}),
        Some("target-mcp"),
        None,
    )
    .await;
    assert_eq!(inbox["count"], 1, "{inbox}");
    let body: Value =
        serde_json::from_str(inbox["messages"][0]["content"].as_str().unwrap()).unwrap();
    assert_eq!(body["kind"], "callback");
    assert_eq!(body["data"], opaque);
    let requests = server.requests();
    assert!(requests[1].0.ends_with("answerCallbackQuery"));
    assert!(requests[2].0.ends_with("editMessageReplyMarkup"));
    assert_eq!(
        requests[2].1["reply_markup"]["inline_keyboard"][0][0],
        json!({"text":"Selected: Yes","disabled":{}})
    );
}

// Catches: a foreign caller sends to the phone, an unknown request starts a
// draft, or multiple allowlisted chats are broadcast/chosen implicitly.
#[tokio::test]
async fn tool_requires_bound_caller_pending_request_and_explicit_multiple_chat_selection() {
    use crate::telegram::tool::Input;
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = runtime(paths.clone(), server.address).await;
    assert!(
        runtime
            .tool(
                "foreign",
                Input::Send {
                    chat_id: None,
                    text: "hello".into(),
                    buttons: vec![]
                }
            )
            .await
            .is_err()
    );
    assert!(
        runtime
            .tool(
                PEER,
                Input::Begin {
                    request_id: "unknown".into()
                }
            )
            .await
            .is_err()
    );
    write_private(&paths.file("allowed_chat_ids"), "1111111\n2222222\n");
    assert!(
        runtime
            .tool(
                PEER,
                Input::Send {
                    chat_id: None,
                    text: "hello".into(),
                    buttons: vec![]
                }
            )
            .await
            .is_err()
    );
    assert!(
        runtime
            .tool(
                PEER,
                Input::Send {
                    chat_id: Some("3333333".into()),
                    text: "hello".into(),
                    buttons: vec![]
                }
            )
            .await
            .is_err()
    );
    assert!(server.requests().is_empty());
}

// Catches: partial or failed final delivery starts streaming again or permits
// a blind resend that could duplicate a message accepted upstream.
#[tokio::test]
async fn failed_final_retires_draft_and_refuses_a_blind_retry() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![
        (StatusCode::OK, json!({"ok":true,"result":true})),
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"ok":false,"error_code":500}),
        ),
    ])
    .await;
    let mut outbound = Outbound::new(paths.clone(), BotApi::loopback(paths, server.address));
    outbound
        .begin("request".into(), PEER.into(), "pty".into(), 1, 1111111)
        .await
        .unwrap();
    assert!(outbound.finish("request", "reply").await.is_err());
    assert!(outbound.finish("request", "reply").await.is_err());
    outbound.refresh().await.unwrap();
    assert_eq!(server.requests().len(), 2);
}

// Catches: permanent credentials errors loop on later sends, or a 429 is
// ignored and the next notification immediately retries upstream.
#[tokio::test]
async fn outbound_permanent_errors_latch_and_rate_limits_do_not_retry_early() {
    for (status, envelope, expected) in [
        (
            StatusCode::UNAUTHORIZED,
            json!({"ok":false,"error_code":401}),
            Error::Unauthorized,
        ),
        (
            StatusCode::TOO_MANY_REQUESTS,
            json!({"ok":false,"error_code":429,"parameters":{"retry_after":60}}),
            Error::RateLimited(60),
        ),
    ] {
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![(status, envelope)]).await;
        let mut outbound = Outbound::new(paths.clone(), BotApi::loopback(paths, server.address));
        assert_eq!(
            outbound.send(1111111, "first", None).await.unwrap_err(),
            expected
        );
        assert!(outbound.send(1111111, "second", None).await.is_err());
        assert_eq!(server.requests().len(), 1);
    }
}

// Catches: begin/Stop read the raw cached agent_state (unset in production)
// instead of the authoritative snapshot derived from foreground/shell state.
#[cfg(unix)]
#[tokio::test]
async fn begin_uses_derived_agent_lifecycle_and_retires_when_turn_completes() {
    use crate::telegram::tool::Input;
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![(StatusCode::OK, json!({"ok":true,"result":true}))]).await;
    let mut runtime = runtime(paths, server.address).await;
    crate::state::tests_support::insert_dummy_session(&runtime.state, PEER);
    crate::test_support::agent_session(&runtime.state, PEER, crate::pty::SHELL_BUSY);
    runtime
        .track(crate::telegram::mail::PendingMail {
            id: "request".into(),
            recipient: PEER.into(),
            content: json!({"chat_id":"1111111"}).to_string(),
        })
        .unwrap();
    let begun = runtime
        .tool(
            PEER,
            Input::Begin {
                request_id: "request".into(),
            },
        )
        .await
        .unwrap();
    assert!(begun["draft_id"].as_i64().unwrap() > 0);
    runtime
        .tool(
            PEER,
            Input::Activity {
                request_id: "request".into(),
                text: "checking".into(),
            },
        )
        .await
        .unwrap();
    // A completed turn is derived from the real completion marker seam.
    runtime
        .state
        .session_maps
        .session_states
        .get_mut(PEER)
        .unwrap()
        .suggested_actions = Some(vec![]);
    runtime.tick().await;
    assert!(
        runtime
            .tool(
                PEER,
                Input::Finish {
                    request_id: "request".into(),
                    text: "too late".into()
                }
            )
            .await
            .is_err()
    );
    assert_eq!(server.requests().len(), 1);
    let (_, session) = runtime.state.session_maps.sessions.remove(PEER).unwrap();
    let _ = session.lock()._child.kill();
}
