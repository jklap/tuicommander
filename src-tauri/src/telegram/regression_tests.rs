use super::inbound::{text_update, updates};
use super::*;

pub(super) async fn elapse_backoff() {
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(61)).await;
    tokio::time::resume();
}

fn offset(paths: &Paths) -> i64 {
    rusqlite::Connection::open(paths.file("journal.sqlite3"))
        .unwrap()
        .query_row("SELECT offset FROM state", [], |r| r.get(0))
        .unwrap()
}

fn padded_update(bytes: usize) -> Value {
    // Synthetic adversarial envelope, not a recorded Telegram fixture.
    json!({"update_id":7,"padding":"x".repeat(bytes)})
}

// Catches: oversize batches never shrink, advance the cursor on failure,
// or permanently leave polling at a reduced limit after a successful batch.
#[tokio::test]
async fn oversized_batches_shrink_without_acknowledging_and_restore_after_success() {
    let (_dir, paths) = setup();
    let oversized = updates(vec![padded_update(1024 * 1024)]);
    let server = FakeServer::start(vec![
        updates(vec![]),
        oversized.clone(),
        oversized,
        updates(vec![text_update(7, 2222222, "ignored")]),
        updates(vec![text_update(8, 1111111, "owner")]),
    ])
    .await;
    let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
    adapter.poll().await.unwrap();
    for expected_limit in [100, 50] {
        assert!(matches!(adapter.poll().await.unwrap(), Poll::Backoff(_)));
        assert_eq!(server.requests().last().unwrap().1["limit"], expected_limit);
        assert_eq!(offset(&paths), 0);
        let count = server.requests().len();
        assert!(matches!(adapter.poll().await.unwrap(), Poll::Backoff(_)));
        assert_eq!(server.requests().len(), count);
        elapse_backoff().await;
    }
    assert!(matches!(adapter.poll().await.unwrap(), Poll::Accepted(0)));
    assert_eq!(server.requests().last().unwrap().1["limit"], 25);
    assert_eq!(offset(&paths), 8);
    assert!(matches!(adapter.poll().await.unwrap(), Poll::Accepted(1)));
    assert_eq!(server.requests().last().unwrap().1["limit"], 100);
    assert_eq!(server.requests().last().unwrap().1["offset"], 8);
}

// Catches: one large update either wedges forever at the batch cap,
// is silently skipped, or an oversize stop is forgotten on restart.
#[tokio::test]
async fn single_update_budget_recovers_or_durably_stops_without_skipping() {
    for (bytes, stops) in [
        (1024 * 1024, false),
        (8 * 1024 * 1024 - 100, false),
        (8 * 1024 * 1024, true),
    ] {
        let (_dir, paths) = setup();
        let mut responses = vec![updates(vec![])];
        responses.extend((0..7).map(|_| updates(vec![padded_update(bytes)])));
        responses.push(updates(vec![]));
        let server = FakeServer::start(responses).await;
        let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
        adapter.poll().await.unwrap();
        for limit in [100, 50, 25, 12, 6, 3] {
            assert!(matches!(adapter.poll().await.unwrap(), Poll::Backoff(_)));
            assert_eq!(server.requests().last().unwrap().1["limit"], limit);
            assert_eq!(offset(&paths), 0);
            elapse_backoff().await;
        }
        if stops {
            assert!(matches!(adapter.poll().await, Err(Error::OversizeUpdate)));
            assert_eq!(offset(&paths), 0);
            let count = server.requests().len();
            assert!(matches!(adapter.poll().await, Err(Error::OversizeUpdate)));
            drop(adapter);
            let mut adapter = Inbound::loopback(paths, server.address).unwrap();
            assert!(matches!(adapter.poll().await, Err(Error::OversizeUpdate)));
            assert_eq!(server.requests().len(), count);
        } else {
            assert!(matches!(adapter.poll().await.unwrap(), Poll::Accepted(0)));
            assert_eq!(offset(&paths), 8);
            adapter.poll().await.unwrap();
            assert_eq!(server.requests().last().unwrap().1["limit"], 100);
        }
    }
}

// Catches: 403/404 stops live only in RAM, or successful-HTTP error
// envelopes bypass the same durable stop as HTTP status errors.
#[tokio::test]
async fn forbidden_and_missing_bot_latch_from_status_or_envelope_across_restart() {
    for (status, code) in [
        (StatusCode::FORBIDDEN, 403),
        (StatusCode::NOT_FOUND, 404),
        (StatusCode::OK, 403),
        (StatusCode::OK, 404),
    ] {
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![
            updates(vec![]),
            (status, json!({"ok":false,"error_code":code})),
        ])
        .await;
        let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
        adapter.poll().await.unwrap();
        assert_eq!(adapter.poll().await.err(), Some(Error::Rejected(code)));
        assert_eq!(offset(&paths), 0);
        drop(adapter);
        let mut adapter = Inbound::loopback(paths, server.address).unwrap();
        assert_eq!(adapter.poll().await.err(), Some(Error::Rejected(code)));
        assert_eq!(server.requests().len(), 2);
    }
}

// Catches: nonpermanent status, malformed envelope or invalid update
// errors bypass the backoff, reset it before commit, or advance the cursor.
#[tokio::test]
async fn rejection_and_protocol_failures_back_off_exponentially_then_reset_on_commit() {
    for (status, body, expected) in [
        (StatusCode::BAD_REQUEST, json!({}), Error::Rejected(400)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({}),
            Error::Rejected(422),
        ),
        (StatusCode::OK, json!({"ok":true}), Error::Protocol),
        (
            StatusCode::OK,
            json!({"ok":true,"result":[{"update_id":-1}]}),
            Error::Protocol,
        ),
    ] {
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![
            updates(vec![]),
            (status, body.clone()),
            (status, body.clone()),
            updates(vec![]),
            (status, body),
        ])
        .await;
        let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
        adapter.poll().await.unwrap();
        for (minimum, maximum) in [(900, 1255), (1900, 2255)] {
            assert_eq!(adapter.poll().await.err(), Some(expected));
            let Poll::Backoff(delay) = adapter.poll().await.unwrap() else {
                panic!("missing backoff")
            };
            assert!((minimum..=maximum).contains(&delay.as_millis()));
            assert_eq!(offset(&paths), 0);
            elapse_backoff().await;
        }
        assert_eq!(server.requests().len(), 3);
        adapter.poll().await.unwrap();
        assert_eq!(adapter.poll().await.err(), Some(expected));
        let Poll::Backoff(delay) = adapter.poll().await.unwrap() else {
            panic!("missing reset")
        };
        assert!(delay.as_millis() <= 1255);
    }
}

// Catches: huge retry_after stalls polling indefinitely, zero yields a
// hot loop, or a normal server retry delay is replaced by local backoff.
#[tokio::test]
async fn retry_after_is_honored_with_a_one_hour_cap() {
    for (seconds, expected) in [(0u64, 1u64), (7, 7), (3600, 3600), (u64::MAX, 3600)] {
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![
            updates(vec![]),
            (
                StatusCode::TOO_MANY_REQUESTS,
                json!({"ok":false,"error_code":429,"parameters":{"retry_after":seconds}}),
            ),
            updates(vec![]),
        ])
        .await;
        let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
        adapter.poll().await.unwrap();
        let Poll::Backoff(delay) = adapter.poll().await.unwrap() else {
            panic!("missing throttle")
        };
        assert_eq!(delay.as_secs(), expected);
        assert!(matches!(adapter.poll().await.unwrap(), Poll::Backoff(_)));
        assert_eq!(server.requests().len(), 2);
        assert_eq!(offset(&paths), 0);
        tokio::time::pause();
        tokio::time::advance(std::time::Duration::from_secs(expected + 1)).await;
        tokio::time::resume();
        assert!(matches!(adapter.poll().await.unwrap(), Poll::Accepted(0)));
    }
}

// Catches: byte-capacity errors hot-loop, do not increase backoff on a
// repeated failed commit, or lost mail cannot recover after capacity is released.
#[tokio::test]
async fn byte_capacity_backoff_preserves_mail_and_recovers_after_consumption() {
    let (_dir, paths) = setup();
    let text = "x".repeat(62000);
    let batch: Vec<_> = (1..=17).map(|id| text_update(id, 1111111, &text)).collect();
    // Each wire batch is below 1 MiB; cumulative retained payload exceeds it.
    let server = FakeServer::start(vec![
        updates(vec![]),
        updates(batch[..16].to_vec()),
        updates(batch[16..].to_vec()),
        updates(batch[16..].to_vec()),
        updates(batch[16..].to_vec()),
        updates(vec![]),
    ])
    .await;
    let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
    adapter.poll().await.unwrap();
    adapter.poll().await.unwrap();
    for minimum in [900, 1900] {
        assert_eq!(adapter.poll().await.err(), Some(Error::Capacity));
        let count = server.requests().len();
        let Poll::Backoff(delay) = adapter.poll().await.unwrap() else {
            panic!("missing capacity delay")
        };
        assert!(delay.as_millis() >= minimum);
        assert_eq!(server.requests().len(), count);
        assert_eq!(offset(&paths), 17);
        assert_eq!(adapter.pending().unwrap().len(), 16);
        elapse_backoff().await;
    }
    adapter.consumed("tg:test-bot:1", PEER).unwrap();
    assert!(matches!(adapter.poll().await.unwrap(), Poll::Accepted(1)));
    assert_eq!(offset(&paths), 18);
}

// Catches: revoked mail survives after restart or after already being
// offered, and reauthorizing a chat resurrects acknowledged private text.
#[tokio::test]
async fn revalidation_purges_revoked_mail_before_poll_or_offer_including_offered_mail() {
    for (offer_first, restart, poll) in [
        (false, false, false),
        (true, false, false),
        (false, true, true),
    ] {
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![
            updates(vec![]),
            updates(vec![
                text_update(1, 1111111, "erase me"),
                text_update(2, 2222222, "retain me"),
            ]),
            updates(vec![]),
        ])
        .await;
        write_private(&paths.file("allowed_chat_ids"), "1111111\n2222222\n");
        let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
        adapter.poll().await.unwrap();
        adapter.poll().await.unwrap();
        struct Sink;
        impl MailPort for Sink {
            async fn offer(&mut self, _: &PendingMail) -> Result<(), Error> {
                Ok(())
            }
        }
        if offer_first {
            assert_eq!(adapter.deliver(&mut Sink).await.unwrap(), 2);
        }
        if restart {
            drop(adapter);
            adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
        }
        write_private(&paths.file("allowed_chat_ids"), "2222222\n");
        if poll {
            adapter.poll().await.unwrap();
        } else {
            adapter.deliver(&mut Sink).await.unwrap();
        }
        let retained = adapter.pending().unwrap();
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].id, "tg:test-bot:2");
        assert_eq!(offset(&paths), 3);
        write_private(&paths.file("allowed_chat_ids"), "1111111\n2222222\n");
        drop(adapter);
        let adapter = Inbound::loopback(paths, server.address).unwrap();
        assert_eq!(adapter.pending().unwrap(), retained);
    }
}

// Catches: removing production redirect-none, HTTPS-only, or no-proxy
// passes tests that exercise only an independently configured loopback client.
#[tokio::test]
async fn production_client_refuses_redirects_http_and_seeded_proxies() {
    let (_dir, paths) = setup();
    let api = BotApi::new(paths).unwrap();
    let description = format!("{:?}", api.client());
    assert!(
        description.contains("redirect_policy") && description.contains("None"),
        "{description}"
    );
    let error = api
        .client()
        .get("http://127.0.0.1:9")
        .send()
        .await
        .unwrap_err();
    assert!(error.is_builder(), "HTTP must fail before any connection");
    let seeded =
        reqwest::Client::builder().proxy(reqwest::Proxy::all("http://127.0.0.1:9").unwrap());
    let secured = super::super::api::secure_client_builder(seeded)
        .build()
        .unwrap();
    assert!(
        !format!("{secured:?}").contains("proxies"),
        "production configuration must clear proxies"
    );
}

// Catches: SQL insert, cursor-update or cleanup errors bypass retry scheduling
// or commit a partial batch, acknowledging mail that failed durable storage.
#[tokio::test]
async fn journal_transaction_faults_back_off_and_retry_the_same_cursor() {
    for trigger in [
        "CREATE TRIGGER fail BEFORE INSERT ON updates BEGIN SELECT RAISE(ABORT,'synthetic'); END;",
        "CREATE TRIGGER fail BEFORE UPDATE OF offset ON state BEGIN SELECT RAISE(ABORT,'synthetic'); END;",
        "CREATE TRIGGER fail BEFORE DELETE ON updates BEGIN SELECT RAISE(ABORT,'synthetic'); END;",
    ] {
        let (_dir, paths) = setup();
        let batch = vec![
            text_update(1, 1111111, "retain"),
            text_update(2, 2222222, "ignore"),
        ];
        let server = FakeServer::start(vec![
            updates(vec![]),
            updates(batch.clone()),
            updates(batch),
        ])
        .await;
        let mut adapter = Inbound::loopback(paths.clone(), server.address).unwrap();
        adapter.poll().await.unwrap();
        let db = rusqlite::Connection::open(paths.file("journal.sqlite3")).unwrap();
        db.execute_batch(trigger).unwrap();
        assert_eq!(adapter.poll().await.err(), Some(Error::Store));
        assert_eq!(offset(&paths), 0);
        assert!(adapter.pending().unwrap().is_empty());
        assert!(matches!(adapter.poll().await.unwrap(), Poll::Backoff(_)));
        assert_eq!(server.requests().len(), 2);
        db.execute_batch("DROP TRIGGER fail;").unwrap();
        elapse_backoff().await;
        assert!(matches!(adapter.poll().await.unwrap(), Poll::Accepted(1)));
        assert_eq!(server.requests()[2].1["offset"], 0);
        assert_eq!(offset(&paths), 3);
    }
}
