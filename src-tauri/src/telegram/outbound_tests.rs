use super::*;
use crate::telegram::outbound::Outbound;

// Catches: UTF-8 slicing panic, astral overflow or whitespace loss in final replies.
#[test]
fn final_chunks_preserve_exact_unicode_and_whitespace() {
    let text = format!(" {}\n{}  ", "😀".repeat(2500), "é".repeat(4000));
    let parts = crate::telegram::outbound::chunks(&text).unwrap();
    assert_eq!(parts.concat(), text);
    assert!(parts.iter().all(|p| p.encode_utf16().count() <= 4096));
    assert!(crate::telegram::outbound::chunks("").is_err());
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
