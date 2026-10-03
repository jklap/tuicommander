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
