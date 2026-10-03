//! Critic tests for story 1419-ab18: authentication, provenance, replay and
//! head-of-line behaviour of the authenticated /mcp/peer star.

use super::*;
use crate::mcp_http::tests::test_state;
use std::net::SocketAddr;

fn peer(state: &Arc<AppState>, id: &str) {
    state.peer_agents.insert(
        id.to_string(),
        crate::state::PeerAgent {
            tuic_session: id.to_string(),
            mcp_session_id: format!("sid-{id}"),
            name: id.to_string(),
            project: None,
            registered_at: 0,
        },
    );
    state
        .mcp
        .to_session
        .insert(format!("sid-{id}"), id.to_string());
}

async fn serve(state: Arc<AppState>) -> SocketAddr {
    let router = axum::Router::new()
        .route("/mcp/peer", axum::routing::get(endpoint))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    addr
}

async fn open(
    addr: SocketAddr,
    query: &str,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tokio_tungstenite::tungstenite::Error,
> {
    tokio_tungstenite::connect_async(format!("ws://{addr}/mcp/peer?{query}"))
        .await
        .map(|(socket, _)| socket)
}

// Catches: an endpoint that upgrades without the daemon token, with a wrong
// token, or with an empty configured token equal to an empty `token=` param,
// or that accepts a connection qualifier the hub could confuse with "local".
#[tokio::test]
async fn peer_endpoint_rejects_missing_wrong_empty_token_and_bad_qualifier() {
    let state = test_state();
    *state.session_token.write() = "secret-token".into();
    let addr = serve(state.clone()).await;
    assert!(open(addr, "connection_id=mint").await.is_err(), "no token");
    assert!(
        open(addr, "connection_id=mint&token=wrong").await.is_err(),
        "wrong token"
    );
    assert!(
        open(addr, "connection_id=local&token=secret-token")
            .await
            .is_err(),
        "qualifier local"
    );
    assert!(
        open(addr, "connection_id=a%2Fb&token=secret-token")
            .await
            .is_err(),
        "qualifier with slash"
    );
    let _good = open(addr, "connection_id=mint&token=secret-token")
        .await
        .expect("the correct token must open, or the rejections above prove nothing");
    assert!(
        open(addr, "connection_id=other&token=secret-token")
            .await
            .is_err(),
        "a second hub must not replace the live one"
    );

    let empty = test_state();
    *empty.session_token.write() = String::new();
    let addr = serve(empty).await;
    assert!(
        open(addr, "connection_id=mint&token=").await.is_err(),
        "empty configured token must not authenticate an empty token param"
    );
}

// Catches: a spoke supplying `sender.host` of its own choosing ("local" or a
// foreign host) and mail arriving attributed to a desktop-local or other-host peer.
#[tokio::test]
async fn spoke_supplied_sender_host_is_overwritten_by_the_connection() {
    let state = test_state();
    peer(&state, "b");
    let result = process(
        state.clone(),
        &Role::Hub("mint".into()),
        Some(Sender {
            host: "local".into(),
            id: "x".into(),
            name: "x".into(),
        }),
        json!({"action": "send", "to": "local/b", "message": "hi"}),
        None,
    )
    .await;
    assert!(result.get("error").is_none(), "{result}");
    let inbox = state.agent_inbox.get("b").expect("mail filed under b");
    assert_eq!(inbox[0].from_tuic_session, "mint/x");
}

// Catches: a spoke reading, waiting on or registering as a peer of another host
// through the hub (only send/list_peers may arrive daemon-to-hub).
#[tokio::test]
async fn spoke_cannot_read_wait_or_register_through_the_hub() {
    let state = test_state();
    peer(&state, "b");
    for action in ["inbox", "wait", "register", "spawn", "kill"] {
        let result = process(
            state.clone(),
            &Role::Hub("mint".into()),
            Some(Sender {
                host: "mint".into(),
                id: "b".into(),
                name: "b".into(),
            }),
            json!({"action": action, "timeout_ms": 1}),
            None,
        )
        .await;
        assert!(result.get("error").is_some(), "{action}: {result}");
    }
}

// Catches: an empty session_id resolving, via `starts_with("")`, to the only
// remote PTY, so a missing target submits to a remote agent.
#[tokio::test]
async fn empty_session_id_never_selects_a_remote_pty() {
    let state = test_state();
    crate::remote_mirror::store_seed_for_test(
        &state,
        "mint",
        vec![crate::mcp_http::types::SessionInfo {
            session_id: "remote-pty".into(),
            ..Default::default()
        }],
    );
    for args in [
        json!({"action": "submit", "session_id": "", "input": "x"}),
        json!({"action": "submit", "session_id": "", "connection_id": "mint", "input": "x"}),
        json!({"action": "submit", "session_id": "mint/", "input": "x"}),
    ] {
        let resolved = super::super::remote_mcp_sessions::resolve(&state, &args);
        assert!(
            !matches!(resolved, Some(Ok(_))),
            "{args} resolved to a remote PTY"
        );
    }
}

// Catches: lifecycle outbox replay after a lost acknowledgement re-delivering a
// message the recipient already read (dedupe only while still in the inbox).
#[tokio::test]
async fn replayed_message_id_is_not_redelivered_after_the_recipient_read_it() {
    let state = test_state();
    peer(&state, "a");
    peer(&state, "b");
    let args = json!({"action": "send", "to": "b", "message": "hi"});
    let first = super::super::mcp_transport::local_peer_call_with_message_id(
        &state,
        &args,
        Some("sid-a"),
        Some("m1".into()),
    )
    .await;
    assert!(first.get("error").is_none(), "{first}");
    state.agent_inbox.get_mut("b").unwrap().clear(); // recipient read it
    let _ = super::super::mcp_transport::local_peer_call_with_message_id(
        &state,
        &args,
        Some("sid-a"),
        Some("m1".into()),
    )
    .await;
    assert!(
        state
            .agent_inbox
            .get("b")
            .is_none_or(|inbox| inbox.is_empty()),
        "same forwarded message id was delivered twice"
    );
}

// Catches: one silent (blackholed) connection holding the global connect lock
// for its whole 20 s handshake budget and starving mail to every other host.
#[tokio::test]
async fn a_stalled_connection_does_not_block_calls_to_other_connections() {
    let state = test_state();
    peer(&state, "a");
    let stall = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", stall.local_addr().unwrap());
    state
        .remote
        .force_connected_for_test("stall", &url, Some("t"));
    let slow_state = state.clone();
    let slow = tokio::spawn(async move {
        dispatch(
            &slow_state,
            &json!({"action": "send", "to": "stall/x", "message": "m"}),
            Some("sid-a"),
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    let other = tokio::time::timeout(
        Duration::from_secs(3),
        dispatch(
            &state,
            &json!({"action": "send", "to": "ghost/x", "message": "m"}),
            Some("sid-a"),
        ),
    )
    .await;
    slow.abort();
    assert!(
        other.is_ok(),
        "a call to an unrelated connection waited behind another host's handshake"
    );
}
