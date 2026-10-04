use super::*;
use crate::telegram::{runtime::Runtime, tool::Input};

const OTHER: &str = "33333333-3333-4333-8333-333333333333";

async fn inbox(runtime: &Runtime, sid: &str) -> Value {
    crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        &runtime.state,
        &json!({"action":"inbox","since":0}),
        Some(sid),
        None,
    )
    .await
}

// Catches: registration is still restricted to a configured peer, replacement
// sends repeated notices, or old buttons/requests migrate to the new agent.
#[tokio::test]
async fn second_register_replaces_first_and_notifies_it_once() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![(
        StatusCode::OK,
        json!({"ok":true,"result":{"message_id":7}}),
    )])
    .await;
    let mut runtime = outbound_tests::runtime(paths, server.address).await;
    let registered = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        &runtime.state,
        &json!({"action":"register","tuic_session":OTHER,"name":"second"}),
        Some("second-mcp"),
        None,
    )
    .await;
    assert!(registered.get("error").is_none());
    runtime
        .tool(
            PEER,
            "target-mcp",
            Input::Send {
                text: "Old choice".into(),
                buttons: vec![vec![crate::telegram::tool::Button {
                    label: "Yes".into(),
                    data: "old".into(),
                }]],
            },
        )
        .await
        .unwrap();
    let old_handle =
        server.requests()[0].1["reply_markup"]["inline_keyboard"][0][0]["callback_data"].clone();
    runtime
        .track(PendingMail {
            id: "old".into(),
            recipient: PEER.into(),
            content: json!({"chat_id":"1111111"}).to_string(),
        })
        .unwrap();
    runtime
        .tool(OTHER, "second-mcp", Input::Register)
        .await
        .unwrap();
    runtime
        .tool(OTHER, "second-mcp", Input::Register)
        .await
        .unwrap();
    runtime.update(json!({"callback_query":{"id":"old-query","from":{"id":1111111},
        "message":{"message_id":7,"date":1,"chat":{"id":1111111,"type":"private"}},"data":old_handle}})).await.unwrap();
    let mail = inbox(&runtime, "target-mcp").await;
    assert_eq!(mail["count"], 1, "{mail}");
    assert!(
        mail["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("replaced")
    );
    assert!(runtime.pending.is_empty());
    assert_eq!(
        runtime
            .tool(PEER, "target-mcp", Input::Unregister)
            .await
            .unwrap_err(),
        Error::NotRegistered
    );
    assert_eq!(inbox(&runtime, "second-mcp").await["count"], 0);
    assert_eq!(server.requests().len(), 1);
}

// Catches: unregistered outbound actions succeed, unregister is a no-op, or a
// daemon restart reloads the registration from config.
#[tokio::test]
async fn unregistered_caller_cannot_send_and_restart_has_no_registration() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = outbound_tests::runtime(paths.clone(), server.address).await;
    runtime
        .tool(PEER, "target-mcp", Input::Unregister)
        .await
        .unwrap();
    for input in [
        Input::Send {
            text: "hello".into(),
            buttons: vec![],
        },
        Input::Begin {
            request_id: "r".into(),
        },
        Input::Activity {
            request_id: "r".into(),
            text: "working".into(),
        },
        Input::Finish {
            request_id: "r".into(),
            text: "done".into(),
        },
    ] {
        let error = runtime.tool(PEER, "target-mcp", input).await.unwrap_err();
        assert_eq!(error, Error::NotRegistered);
        assert!(error.to_string().contains("register"));
    }
    runtime
        .tool(PEER, "target-mcp", Input::Register)
        .await
        .unwrap();
    let fresh = Runtime::new(
        runtime.state.clone(),
        Config::load(&paths).unwrap().unwrap(),
        paths,
        "adapter-mcp".into(),
    )
    .unwrap();
    assert!(fresh.registered_peer().is_none());
    assert!(server.requests().is_empty());
}

// Catches: a registered agent retains phone authority after its MCP transport
// ends, after PTY close, or after agent exit while its shell remains alive.
#[cfg(unix)]
#[tokio::test]
async fn exited_agent_mcp_session_and_closed_pty_unregister() {
    for end in ["mcp", "pty"] {
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![]).await;
        let mut runtime = outbound_tests::runtime(paths, server.address).await;
        crate::state::tests_support::insert_dummy_session(&runtime.state, PEER);
        crate::test_support::agent_session(&runtime.state, PEER, crate::pty::SHELL_IDLE);
        runtime
            .tool(PEER, "target-mcp", Input::Register)
            .await
            .unwrap();
        let mut removed = None;
        match end {
            "mcp" => {
                runtime.state.mcp.to_session.remove("target-mcp");
            }
            "pty" => {
                removed = runtime.state.session_maps.sessions.remove(PEER);
            }
            _ => unreachable!(),
        }
        runtime.tick().await;
        assert!(runtime.registered_peer().is_none(), "{end}");
        assert_eq!(
            runtime
                .tool(
                    PEER,
                    "target-mcp",
                    Input::Send {
                        text: "late".into(),
                        buttons: vec![],
                    }
                )
                .await
                .unwrap_err(),
            Error::NotRegistered
        );
        if let Some((_, session)) =
            removed.or_else(|| runtime.state.session_maps.sessions.remove(PEER))
        {
            let _ = session.lock()._child.kill();
        }
        assert!(server.requests().is_empty());
    }
}

// Catches: a carried configured agent keeps its registration after the native
// foreground detector observes that the agent has returned to its live shell.
#[cfg(unix)]
#[tokio::test]
async fn foreground_agent_exit_unregisters_even_when_shell_stays() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = outbound_tests::runtime(paths, server.address).await;
    let state = runtime.state.clone();
    let agent =
        crate::test_support::ForegroundIdentityProbe::shell_parent(state.clone(), PEER, "claude");
    state
        .session_maps
        .session_states
        .get_mut(PEER)
        .unwrap()
        .seed_configured_agent(Some("claude".into()));
    assert_eq!(
        crate::pty::refresh_session_agent(&state, PEER).as_deref(),
        Some("claude")
    );
    runtime
        .tool(PEER, "target-mcp", Input::Register)
        .await
        .unwrap();
    let seen = state.session_maps.session_states.get(PEER).unwrap().clone();
    drop(agent);
    let _shell =
        crate::test_support::ForegroundIdentityProbe::shell_root(state.clone(), PEER, "bash");
    state.session_maps.session_states.insert(PEER.into(), seen);
    assert_eq!(crate::pty::refresh_session_agent(&state, PEER), None);
    assert!(state.session_maps.sessions.contains_key(PEER));
    runtime.tick().await;
    assert_eq!(
        runtime
            .tool(
                PEER,
                "target-mcp",
                Input::Send {
                    text: "late".into(),
                    buttons: vec![],
                }
            )
            .await
            .unwrap_err(),
        Error::NotRegistered
    );
    assert!(server.requests().is_empty());
}

// Catches: no-agent inbound is retained or silently lost, stranger text gets a
// reply, or a drop prevents committing the polling cursor.
#[tokio::test]
async fn inbound_without_registration_replies_and_drops_but_strangers_stay_silent() {
    let (_dir, paths) = setup();
    write_private(&paths.file("next_offset"), "7\n");
    let server = FakeServer::start(vec![
        inbound::updates(vec![
            inbound::text_update(7, 1111111, "allowed"),
            inbound::text_update(8, 3333333, "stranger"),
        ]),
        (StatusCode::OK, json!({"ok":true,"result":{"message_id":9}})),
    ])
    .await;
    let mut runtime = outbound_tests::runtime(paths.clone(), server.address).await;
    runtime
        .tool(PEER, "target-mcp", Input::Unregister)
        .await
        .unwrap();
    let state = runtime.state.clone();
    let (commands, receive) = tokio::sync::mpsc::channel(10);
    let port = crate::telegram::native::NativeMail::new(commands);
    let worker = tokio::spawn(runtime.run(receive));
    let mut poller =
        crate::telegram::inbound::Inbound::with_loopback(paths.clone(), server.address, port)
            .unwrap();
    assert!(matches!(poller.poll().await.unwrap(), Poll::Accepted(0)));
    assert_eq!(
        std::fs::read_to_string(paths.file("next_offset"))
            .unwrap()
            .trim(),
        "9"
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].0.ends_with("sendMessage"));
    assert_eq!(requests[1].1["chat_id"], 1111111);
    assert_eq!(requests[1].1["text"], "Nessun agent registrato");
    assert!(
        state
            .agent_inbox
            .iter()
            .all(|entry| entry.value().is_empty())
    );
    worker.abort();
}

// Catches: inbound parsing still addresses the removed configured UUID rather
// than letting the serialized runtime resolve its current registered agent.
#[tokio::test]
async fn inbound_registered_agent_receives_native_mail_without_phone_error() {
    let (_dir, paths) = setup();
    write_private(&paths.file("next_offset"), "7\n");
    let server = FakeServer::start(vec![inbound::updates(vec![inbound::text_update(
        7,
        1111111,
        "exact inbound",
    )])])
    .await;
    let runtime = outbound_tests::runtime(paths.clone(), server.address).await;
    let state = runtime.state.clone();
    let (commands, receive) = tokio::sync::mpsc::channel(10);
    let port = crate::telegram::native::NativeMail::new(commands);
    let worker = tokio::spawn(runtime.run(receive));
    let mut poller =
        crate::telegram::inbound::Inbound::with_loopback(paths.clone(), server.address, port)
            .unwrap();
    assert!(matches!(poller.poll().await.unwrap(), Poll::Accepted(1)));
    let mail = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        &state,
        &json!({"action":"inbox","since":0}),
        Some("target-mcp"),
        None,
    )
    .await;
    assert_eq!(mail["count"], 1, "{mail}");
    let envelope: Value =
        serde_json::from_str(mail["messages"][0]["content"].as_str().unwrap()).unwrap();
    assert_eq!(envelope["text"], "exact inbound");
    assert_eq!(envelope["request_id"], "tg:test-bot:7");
    assert_eq!(server.requests().len(), 1);
    worker.abort();
}
