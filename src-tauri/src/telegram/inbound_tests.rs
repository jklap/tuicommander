use super::*;

pub(super) fn text_update(id: i64, chat: i64, text: &str) -> Value {
    let mut value = public_update();
    value["update_id"] = json!(id);
    value["message"]["chat"]["id"] = json!(chat);
    // The recorded older public example omits Chat.type; supply the modern
    // documented required field explicitly. This is not a live capture.
    value["message"]["chat"]["type"] = json!("private");
    value["message"]["text"] = json!(text);
    value
}
pub(super) fn updates(values: Vec<Value>) -> (StatusCode, Value) {
    (StatusCode::OK, json!({"ok":true,"result":values}))
}

// Catches: every restart repeats destructive offset=-1 and drops new phone messages.
#[tokio::test]
async fn first_start_discards_backlog_once_then_restart_uses_committed_offset() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![
        updates(vec![public_update()]),
        updates(vec![text_update(10001, 1111111, "new message")]),
    ])
    .await;
    let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
    assert!(matches!(adapter.poll().await.unwrap(), Poll::Accepted(0)));
    assert!(adapter.pending().unwrap().is_empty());
    drop(adapter);
    let mut adapter = Inbound::loopback(paths, server.address).unwrap();
    assert!(matches!(adapter.poll().await.unwrap(), Poll::Accepted(1)));
    assert_eq!(adapter.pending().unwrap().len(), 1);
    let requests = server.requests();
    assert_eq!(requests[0].1["offset"], -1);
    assert_eq!(requests[0].1["limit"], 1);
    assert_eq!(requests[1].1["offset"], 10001);
    assert_eq!(requests[1].1["timeout"], 25);
}

// Catches: an empty bootstrap never persists initialized, so future arrivals are discarded.
#[tokio::test]
async fn empty_bootstrap_persists_zero_without_reentering_discard_mode() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![
        updates(vec![]),
        updates(vec![text_update(7, 1111111, "after empty")]),
    ])
    .await;
    let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
    adapter.poll().await.unwrap();
    drop(adapter);
    let mut adapter = Inbound::loopback(paths, server.address).unwrap();
    adapter.poll().await.unwrap();
    assert_eq!(server.requests()[1].1["offset"], 0);
    assert_eq!(adapter.pending().unwrap().len(), 1);
}

// Catches: 401/409 retry forever or reset their stopped state when the daemon restarts.
#[tokio::test]
async fn unauthorized_and_conflict_latch_across_restart_without_more_polls() {
    for (status, expected) in [
        (StatusCode::UNAUTHORIZED, Error::Unauthorized),
        (StatusCode::CONFLICT, Error::Conflict),
    ] {
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![
            updates(vec![]),
            (status, json!({"description":"synthetic private body"})),
        ])
        .await;
        let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
        adapter.poll().await.unwrap();
        assert!(matches!(adapter.poll().await,Err(e) if e==expected));
        assert!(matches!(adapter.poll().await,Err(e) if e==expected));
        drop(adapter);
        let mut adapter = Inbound::loopback(paths, server.address).unwrap();
        assert!(matches!(adapter.poll().await,Err(e) if e==expected));
        assert_eq!(server.requests().len(), 2);
    }
}

// Catches: retry_after or transport backoff is ignored by callers repeatedly invoking the poll port.
#[tokio::test]
async fn retry_delay_prevents_immediate_repoll_and_preserves_offset() {
    for (status, body, min_delay) in [
        (
            StatusCode::TOO_MANY_REQUESTS,
            json!({"ok":false,"error_code":429,"parameters":{"retry_after":7}}),
            6,
        ),
        (StatusCode::BAD_GATEWAY, json!({}), 0),
    ] {
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![updates(vec![]), (status, body)]).await;
        let mut adapter = Inbound::loopback(paths, server.address).unwrap();
        adapter.poll().await.unwrap();
        let Poll::Backoff(delay) = adapter.poll().await.unwrap() else {
            panic!("must back off")
        };
        assert!(delay > std::time::Duration::from_secs(min_delay));
        assert!(matches!(adapter.poll().await.unwrap(), Poll::Backoff(_)));
        assert_eq!(server.requests().len(), 2);
        assert_eq!(server.requests()[1].1["offset"], 0);
    }
}
