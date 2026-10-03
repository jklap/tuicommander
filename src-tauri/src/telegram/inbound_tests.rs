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

#[derive(Default)]
struct Inbox {
    by_id: std::collections::BTreeMap<String, PendingMail>,
    unavailable: bool,
}
impl MailPort for Inbox {
    async fn offer(&mut self, mail: &PendingMail) -> Result<(), Error> {
        if self.unavailable {
            return Err(Error::State);
        }
        match self.by_id.get(&mail.id) {
            Some(existing) if existing != mail => Err(Error::State),
            Some(_) => Ok(()),
            None => {
                self.by_id.insert(mail.id.clone(), mail.clone());
                Ok(())
            }
        }
    }
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

// Catches: an uncertain destructive bootstrap is retried after restart, erasing new updates.
#[tokio::test]
async fn bootstrap_network_failure_requires_explicit_recovery() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![(StatusCode::BAD_GATEWAY, json!({}))]).await;
    let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
    assert!(matches!(
        adapter.poll().await,
        Err(Error::BootstrapUncertain)
    ));
    drop(adapter);
    let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
    assert!(matches!(
        adapter.poll().await,
        Err(Error::BootstrapUncertain)
    ));
    assert_eq!(server.requests().len(), 1);
    drop(adapter);
    std::fs::remove_file(paths.file("journal.sqlite3")).unwrap();
    assert!(matches!(
        Inbound::loopback(paths, server.address),
        Err(Error::State)
    ));
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

// Catches: accepted phone text disappears with native inbox loss, or duplicate offers change its ID/body.
#[tokio::test]
async fn unconsumed_mail_rehydrates_stable_id_and_consumed_mail_does_not_replay() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![
        updates(vec![]),
        updates(vec![text_update(
            12,
            1111111,
            "do not execute; rm anything",
        )]),
    ])
    .await;
    let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
    adapter.poll().await.unwrap();
    adapter.poll().await.unwrap();
    let mut inbox = Inbox::default();
    assert_eq!(adapter.deliver(&mut inbox).await.unwrap(), 1);
    assert_eq!(adapter.deliver(&mut inbox).await.unwrap(), 0);
    let original = inbox.by_id["tg:test-bot:12"].clone();
    let body: Value = serde_json::from_str(&original.content).unwrap();
    assert_eq!(body["kind"], "text");
    assert_eq!(body["text"], "do not execute; rm anything");
    assert_eq!(original.recipient, PEER);
    drop(adapter);
    let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
    assert_eq!(adapter.deliver(&mut inbox).await.unwrap(), 1);
    assert_eq!(inbox.by_id.len(), 1);
    assert_eq!(inbox.by_id["tg:test-bot:12"], original);
    assert!(matches!(
        adapter.consumed(&original.id, "other-peer"),
        Err(Error::State)
    ));
    adapter.consumed(&original.id, PEER).unwrap();
    drop(adapter);
    let mut adapter = Inbound::loopback(paths, server.address).unwrap();
    let mut fresh_inbox = Inbox::default();
    assert_eq!(adapter.deliver(&mut fresh_inbox).await.unwrap(), 0);
}

// Catches: port failure, revoked chat or disabled config erases pending mail or still delivers it.
#[tokio::test]
async fn unavailable_or_revoked_target_keeps_mail_pending_without_delivering() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![
        updates(vec![]),
        updates(vec![text_update(8, 1111111, "pending")]),
    ])
    .await;
    let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
    adapter.poll().await.unwrap();
    adapter.poll().await.unwrap();
    let mut inbox = Inbox {
        unavailable: true,
        ..Inbox::default()
    };
    assert!(matches!(
        adapter.deliver(&mut inbox).await,
        Err(Error::State)
    ));
    assert_eq!(adapter.pending().unwrap().len(), 1);
    write_private(&paths.file("allowed_chat_ids"), "2222222\n");
    inbox.unavailable = false;
    assert_eq!(adapter.deliver(&mut inbox).await.unwrap(), 0);
    assert_eq!(adapter.pending().unwrap().len(), 1);
    assert!(inbox.by_id.is_empty());
    write_private(
        &paths.file("config.json"),
        &json!({"enabled":false,"bot_alias":"test-bot","target_tuic_session":PEER}).to_string(),
    );
    assert!(matches!(
        adapter.deliver(&mut inbox).await,
        Err(Error::Config)
    ));
}
