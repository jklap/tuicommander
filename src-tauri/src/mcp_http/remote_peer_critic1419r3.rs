//! Round 3 security critic tests for story 1419-ab18: replay budget fairness and
//! accounting, concurrent retirement, and disconnect racing a handshake.

use super::*;
use crate::mcp_http::tests::test_state;

fn message(id: &str, from: &str) -> crate::state::AgentMessage {
    crate::state::AgentMessage {
        id: id.to_string(),
        from_tuic_session: from.to_string(),
        from_name: from.to_string(),
        content: "one lifecycle notice".to_string(),
        timestamp: 0,
        delivered_via_channel: false,
    }
}

/// `recipients` recipients named `{tag}-{n}`, each holding `per` ids from `sender`.
fn fill(state: &AppState, tag: &str, recipients: usize, per: usize, sender: &str) {
    for r in 0..recipients {
        for i in 0..per {
            let id = format!("{tag}-{r}-{i}");
            assert_eq!(
                record_forwarded(state, &format!("{tag}-{r}"), &message(&id, sender)),
                Ok(true),
                "{id}"
            );
        }
    }
}

// Catches: one sender spreading 100 ids over 656 recipients consumes the global
// 65536-id budget, so every other host's mail to every recipient is rejected.
#[test]
fn one_sender_cannot_starve_every_other_sender_of_the_replay_budget() {
    let state = test_state();
    fill(&state, "hostile", 656, 100, "mint/hostile");
    assert_eq!(
        record_forwarded(
            &state,
            "fresh-recipient",
            &message("victim-1", "other/victim")
        ),
        Ok(true),
        "a different sender's first notice must not be rejected by another sender's volume"
    );
}

// Catches: sender windows are released only when the RECIPIENT unregisters, so a
// long-lived orchestrator that receives notices from many short-lived remote
// sessions permanently fills the global budget and every forwarded notice to
// every recipient is rejected until restart.
#[test]
fn departed_senders_do_not_pin_the_budget_of_a_live_recipient() {
    let state = test_state();
    for n in 0..MAX_FORWARDED_RECORDS {
        assert_eq!(
            record_forwarded(
                &state,
                "orchestrator",
                &message(&format!("m-{n}"), &format!("mint/s-{n}"))
            ),
            Ok(true)
        );
    }
    assert_eq!(
        record_forwarded(&state, "orchestrator", &message("next", "mint/new-session")),
        Ok(true),
        "65536 retired one-notice senders must not block the next live sender"
    );
}

// Catches: the global counter drifts when a window evicts (counted twice, or not
// at all), so the budget trips early or lets the 65537th id in.
#[test]
fn budget_is_exact_after_window_eviction_and_full_budget_still_dedupes() {
    let state = test_state();
    for i in 0..250 {
        assert_eq!(
            record_forwarded(&state, "churn", &message(&format!("c-{i}"), "mint/s")),
            Ok(true)
        );
    }
    // churn holds exactly its last 100 ids.
    assert_eq!(
        record_forwarded(&state, "churn", &message("c-150", "mint/s")),
        Ok(false)
    );
    assert_eq!(
        record_forwarded(&state, "churn", &message("c-149", "mint/s")),
        Ok(true)
    );
    // 100 (churn) + 654*100 + 36 = 65536 exactly.
    fill(&state, "a", 654, 100, "mint/other");
    fill(&state, "b", 1, 36, "mint/other");
    assert!(
        record_forwarded(&state, "z", &message("over", "mint/other")).is_err(),
        "65537th id must be rejected"
    );
    assert_eq!(
        record_forwarded(&state, "a-0", &message("a-0-0", "mint/other")),
        Ok(false),
        "a replay is still recognised when the budget is full"
    );
    assert_eq!(
        record_forwarded(&state, "churn", &message("c-new", "mint/s")),
        Ok(true),
        "a full window evicts in place and needs no extra budget"
    );
}

// Catches: unregister returns the wrong amount to the budget (not at all, or
// per sender instead of per id).
#[test]
fn unregister_returns_exactly_the_recipients_ids_to_the_budget() {
    let state = test_state();
    for s in 0..3 {
        for i in 0..100 {
            assert_eq!(
                record_forwarded(
                    &state,
                    "gone",
                    &message(&format!("g-{s}-{i}"), &format!("mint/s{s}"))
                ),
                Ok(true)
            );
        }
    }
    // 300 + 652*100 + 36 = 65536 exactly.
    fill(&state, "a", 652, 100, "mint/other");
    fill(&state, "b", 1, 36, "mint/other");
    assert!(record_forwarded(&state, "z", &message("over", "mint/other")).is_err());
    unregister_peer(&state, "gone");
    // Three senders x 100 ids are all net-new (a window evicts only past 100),
    // so exactly the 300 released ids fit and the fourth sender is rejected.
    for s in 0..3 {
        for i in 0..100 {
            assert_eq!(
                record_forwarded(
                    &state,
                    "z",
                    &message(&format!("z-{s}-{i}"), &format!("mint/x{s}"))
                ),
                Ok(true),
                "z-{s}-{i}: 300 ids were released"
            );
        }
    }
    assert!(record_forwarded(&state, "z", &message("z-over", "mint/x3")).is_err());
}

// Catches: a sender's retirement (host disconnect) purges its windows, so a
// lost-ack retry after reconnect is delivered twice.
#[test]
fn dedupe_survives_the_senders_own_retirement() {
    let state = test_state();
    assert_eq!(
        record_forwarded(&state, "r", &message("n1", "mint/s")),
        Ok(true)
    );
    unregister_peer(&state, "mint/s");
    assert_eq!(
        record_forwarded(&state, "r", &message("n1", "mint/s")),
        Ok(false)
    );
}

// Catches: register/enqueue/unregister interleavings leave replay state for an
// unregistered recipient or let the global counter drift from the stored ids.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_enqueue_and_retirement_keep_history_consistent() {
    let state = test_state();
    let register = |state: &AppState| {
        state.peer_agents.insert(
            "R".into(),
            crate::state::PeerAgent {
                tuic_session: "R".into(),
                mcp_session_id: "sid-R".into(),
                name: "R".into(),
                project: None,
                registered_at: 0,
            },
        );
    };
    let producer = {
        let state = state.clone();
        tokio::task::spawn_blocking(move || {
            for i in 0..3000 {
                if i % 7 == 0 {
                    register(&state);
                }
                let _ = enqueue_forwarded(&state, "R", message(&format!("id-{i}"), "mint/s"));
            }
        })
    };
    let retirer = {
        let state = state.clone();
        tokio::task::spawn_blocking(move || {
            for _ in 0..3000 {
                unregister_peer(&state, "R");
                state.agent_inbox.remove("R");
            }
        })
    };
    producer.await.unwrap();
    retirer.await.unwrap();
    let history = state.remote_mail.forwarded_history.lock();
    let stored: usize = history.recipients.values().map(|r| r.ids.len()).sum();
    assert_eq!(
        history.records, stored,
        "global counter drifted from stored ids"
    );
    if !state.peer_agents.contains_key("R") {
        assert!(
            !history.recipients.contains_key("R"),
            "orphan replay state for an unregistered peer"
        );
    }
}

async fn held_daemon(
    hub: &Arc<AppState>,
) -> (
    tokio::task::JoinHandle<()>,
    tokio::sync::oneshot::Receiver<()>,
    Arc<tokio::sync::Notify>,
) {
    let remote = test_state();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    hub.remote
        .force_connected_for_test("mint", &url, Some(&remote.session_token.read().clone()));
    let (tx, entered) = tokio::sync::oneshot::channel();
    let signal = Arc::new(parking_lot::Mutex::new(Some(tx)));
    let release = Arc::new(tokio::sync::Notify::new());
    let router = super::super::build_remote_router(remote).layer(axum::middleware::from_fn({
        let release = release.clone();
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let release = release.clone();
            let signal = signal.clone();
            async move {
                if request.uri().path() == "/mcp/peer" {
                    if let Some(tx) = signal.lock().take() {
                        let _ = tx.send(());
                    }
                    release.notified().await;
                }
                next.run(request).await
            }
        }
    }));
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (server, entered, release)
}

// Catches: disconnect + immediate reconnect while a handshake holds the host
// lock skips the deferred retirement (generation moved on), so the old link's
// shadow peers survive although the identical sequence without a racing
// handshake retires them synchronously.
#[tokio::test]
async fn disconnect_then_reconnect_during_a_handshake_still_retires_old_shadows() {
    let hub = test_state();
    let (server, entered, release) = held_daemon(&hub).await;
    hub.peer_agents.insert(
        "mint/ghost".into(),
        crate::state::PeerAgent {
            tuic_session: "mint/ghost".into(),
            mcp_session_id: "remote-mail:test:ghost".into(),
            name: "ghost".into(),
            project: None,
            registered_at: 0,
        },
    );
    let opening = {
        let hub = hub.clone();
        tokio::spawn(async move { connection(&hub, "mint").await })
    };
    entered.await.unwrap();
    disconnect(&hub, "mint");
    connect_configured(&hub, "mint".into());
    let mut relinked = false;
    for _ in 0..100 {
        release.notify_one();
        if hub.remote_mail.connections.contains_key("mint") {
            relinked = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let first = opening.await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let survived = hub.peer_agents.contains_key("mint/ghost");
    disconnect(&hub, "mint");
    server.abort();
    assert!(
        first.is_err(),
        "the pre-disconnect handshake must not publish"
    );
    assert!(relinked, "the reconnect must still establish its own link");
    assert!(
        !survived,
        "shadow peer of the disconnected link survived the race"
    );
}
