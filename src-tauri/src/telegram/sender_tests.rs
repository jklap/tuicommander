use super::tests::{FakeServer, PEER, outbound_tests, setup};
use super::{mail::Update, tool::Input};
use serde_json::{Value, json};

const SENDER: &str = "33333333-3333-4333-8333-333333333333";

async fn add_sender(runtime: &super::runtime::Runtime) {
    let result = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        &runtime.state,
        &json!({"action":"register","tuic_session":SENDER,"name":"coordinator"}),
        Some("sender-mcp"), None,
    ).await;
    assert!(result.get("error").is_none(), "{result}");
}

// Catches: outbound permission remains coupled to inbound ownership.
#[tokio::test]
async fn non_owner_send_preserves_owner_and_prefixes_server_identity() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = outbound_tests::runtime(paths, server.address).await;
    add_sender(&runtime).await;
    let result = runtime.tool(SENDER, "sender-mcp", Input::Send {
        text: "[target-mcp] release ready".into(), buttons: vec![],
    }).await;
    assert!(result.is_ok(), "non-owner send refused: {result:?}");
    assert_eq!(runtime.registered_peer(), Some(PEER));
    assert_eq!(server.requests()[0].1["text"], "[coordinator] [target-mcp] release ready");
    assert_eq!(server.requests()[0].1["chat_id"], 1111111);
}

// Catches: reply_to_message is lost before delivery, so replies go to the owner.
#[tokio::test]
async fn reply_to_sender_reaches_sender_native_inbox_not_owner() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = outbound_tests::runtime(paths, server.address).await;
    add_sender(&runtime).await;
    // Synthetic adversarial update, not claimed as a recording from Telegram.
    let value = json!({"update_id":77,"message":{"message_id":78,"chat":{"id":1111111,"type":"private"},"text":"ship it","reply_to_message":{"message_id":76,"text":"[coordinator] release ready"}}});
    let mail = Update::parse(&value, &runtime.outbound.paths.allowlist().unwrap(), "test-bot", "").unwrap().mail.unwrap();
    runtime.deliver(mail).await.unwrap();
    for (sid, expected) in [("sender-mcp", 1), ("target-mcp", 0)] {
        let inbox: Value = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
            &runtime.state, &json!({"action":"inbox","since":0}), Some(sid), None,
        ).await;
        assert_eq!(inbox["count"], expected, "{sid}: {inbox}");
    }
    assert_eq!(runtime.registered_peer(), Some(PEER));
}
