use super::inbound::{text_update, updates};
use super::*;

fn offset(paths: &Paths) -> i64 {
    rusqlite::Connection::open(paths.file("journal.sqlite3"))
        .unwrap()
        .query_row("SELECT offset FROM state", [], |row| row.get(0))
        .unwrap()
}

// Catches: unauthorized chat content is journaled or duplicates are delivered as new mail after restart.
#[tokio::test]
async fn unknown_chats_are_dropped_and_duplicate_updates_leave_one_mail() {
    let (_dir, paths) = setup();
    let value = text_update(10, 1111111, "authorized");
    let server = FakeServer::start(vec![
        updates(vec![]),
        updates(vec![
            text_update(9, 2222222, "private unauthorized payload"),
            value.clone(),
            value.clone(),
        ]),
        updates(vec![value]),
    ])
    .await;
    let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
    adapter.poll().await.unwrap();
    assert!(matches!(adapter.poll().await.unwrap(), Poll::Accepted(1)));
    assert!(matches!(adapter.poll().await.unwrap(), Poll::Accepted(0)));
    assert_eq!(offset(&paths), 11);
    assert_eq!(adapter.pending().unwrap().len(), 1);
    let db = rusqlite::Connection::open(paths.file("journal.sqlite3")).unwrap();
    let stored: Vec<String> = db
        .prepare("SELECT content FROM updates")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert!(!stored[0].contains("unauthorized"));
}

// Catches: a failed write advances the offset and acknowledges a phone message that never persisted.
#[tokio::test]
async fn journal_write_failure_rolls_back_mail_and_offset() {
    let (_dir, paths) = setup();
    let value = text_update(3, 1111111, "durable");
    let server = FakeServer::start(vec![
        updates(vec![]),
        updates(vec![value.clone()]),
        updates(vec![value]),
    ])
    .await;
    let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
    adapter.poll().await.unwrap();
    let db = rusqlite::Connection::open(paths.file("journal.sqlite3")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_phone_insert BEFORE INSERT ON updates BEGIN SELECT RAISE(ABORT,'injected write fault'); END;").unwrap();
    assert!(matches!(adapter.poll().await, Err(Error::Store)));
    assert_eq!(offset(&paths), 0);
    assert!(adapter.pending().unwrap().is_empty());
    assert!(matches!(adapter.poll().await.unwrap(), Poll::Backoff(_)));
    assert_eq!(server.requests().len(), 2);
    db.execute_batch("DROP TRIGGER fail_phone_insert;").unwrap();
    super::regression::elapse_backoff().await;
    assert!(matches!(adapter.poll().await.unwrap(), Poll::Accepted(1)));
    assert_eq!(server.requests()[2].1["offset"], 0);
    assert_eq!(offset(&paths), 4);
}

// Catches: capacity eviction or cursor advancement silently loses accepted unread phone messages.
#[tokio::test]
async fn full_pending_queue_rejects_batch_without_advancing_cursor() {
    let (_dir, paths) = setup();
    let first = (0..100)
        .map(|id| text_update(id, 1111111, "queued"))
        .collect();
    let server = FakeServer::start(vec![
        updates(vec![]),
        updates(first),
        updates(vec![text_update(100, 1111111, "overflow")]),
    ])
    .await;
    let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
    adapter.poll().await.unwrap();
    adapter.poll().await.unwrap();
    assert!(matches!(adapter.poll().await, Err(Error::Capacity)));
    assert_eq!(offset(&paths), 100);
    assert_eq!(adapter.pending().unwrap().len(), 100);
}

// Catches: corrupt or conflicting update IDs acknowledge a later batch before validation completes.
#[tokio::test]
async fn malformed_or_colliding_updates_do_not_acknowledge_the_batch() {
    let (_dir, paths) = setup();
    let mut malformed = text_update(5, 1111111, "malformed");
    malformed["update_id"] = json!(-1);
    let server = FakeServer::start(vec![
        updates(vec![]),
        updates(vec![text_update(4, 1111111, "would be lost"), malformed]),
        updates(vec![
            text_update(6, 1111111, "first"),
            text_update(6, 1111111, "different"),
        ]),
    ])
    .await;
    let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
    adapter.poll().await.unwrap();
    assert!(matches!(adapter.poll().await, Err(Error::Protocol)));
    assert_eq!(offset(&paths), 0);
    assert!(adapter.pending().unwrap().is_empty());
    super::regression::elapse_backoff().await;
    assert!(matches!(adapter.poll().await, Err(Error::Protocol)));
    assert_eq!(offset(&paths), 0);
}

// Catches: a configuration edit routes recovered phone mail into a different peer's inbox.
#[tokio::test]
async fn changing_the_bound_peer_cannot_adopt_the_existing_journal() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![updates(vec![])]).await;
    let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
    adapter.poll().await.unwrap();
    drop(adapter);
    write_private(&paths.file("config.json"),&json!({"enabled":true,"bot_alias":"test-bot","target_tuic_session":"22222222-2222-4222-8222-222222222222"}).to_string());
    assert!(matches!(
        Inbound::loopback(paths, server.address),
        Err(Error::State)
    ));
    assert_eq!(server.requests().len(), 1);
}
