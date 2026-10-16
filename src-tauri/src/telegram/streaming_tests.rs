use super::tests::{FakeServer, PEER, setup};
use super::{mail::PendingMail, tool::Input};
use axum::http::StatusCode;
use serde_json::json;

// Catches: a routine retirement makes an authenticated final answer disappear.
#[tokio::test]
async fn finish_after_draft_retired_still_delivers_reply() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![(StatusCode::OK,
        json!({"ok":true,"result":{"message_id":7}}))]).await;
    let mut runtime = super::tests::outbound_tests::runtime(paths, server.address).await;
    runtime.track(PendingMail {
        id: "request".into(), recipient: PEER.into(),
        content: json!({"chat_id":"1111111"}).to_string(),
    }).unwrap();
    // No draft remains (the same state after an epoch change or a lag).
    assert!(runtime.outbound.active.is_none());
    let result = runtime.tool(PEER, "target-mcp", Input::Finish {
        request_id: "request".into(), text: " exact final\n".into(),
    }).await.unwrap();
    assert_eq!(result["message_ids"], json!([7]));
    assert_eq!(server.requests()[0].1["text"], " exact final\n");
}
