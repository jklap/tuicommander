//! Round 2 critic tests for story 1419-ab18: dedup horizon, session address
//! ownership, teardown cleanup and secrets in errors.

use super::*;
use crate::mcp_http::tests::test_state;

fn message(id: &str, from: &str, content: &str) -> crate::state::AgentMessage {
    crate::state::AgentMessage {
        id: id.to_string(),
        from_tuic_session: from.to_string(),
        from_name: from.to_string(),
        content: content.to_string(),
        timestamp: 0,
        delivered_via_channel: false,
    }
}

// Catches: the per-recipient 100-id window is shared by every sender, so a
// second peer that mails the same recipient 100 times pushes sender A's id out
// and A's lost-ack retry (or a captured-id replay) is delivered again, although
// A's own outbox still holds the message.
#[test]
fn another_senders_traffic_cannot_evict_an_id_and_let_a_replay_through() {
    let state = test_state();
    let victim = message("m-victim", "mint/a", "pay");
    assert_eq!(record_forwarded(&state, "r", &victim), Ok(true));
    for n in 0..100 {
        let flood = message(&format!("flood-{n}"), "other/b", "x");
        assert_eq!(record_forwarded(&state, "r", &flood), Ok(true));
    }
    assert_eq!(
        record_forwarded(&state, "r", &victim),
        Ok(false),
        "replay of an id from another sender's window was delivered again"
    );
}

// Catches: the 1024-recipient cap evicting the oldest recipient wholesale, so
// traffic to 1024 other recipients erases the dedup history of the first one.
#[test]
fn recipient_cap_eviction_does_not_reopen_replay_for_an_old_recipient() {
    let state = test_state();
    let first = message("m1", "mint/a", "once");
    assert_eq!(record_forwarded(&state, "r0", &first), Ok(true));
    for n in 1..=1024 {
        let other = message(&format!("o-{n}"), "mint/a", "x");
        assert_eq!(record_forwarded(&state, &format!("r{n}"), &other), Ok(true));
    }
    assert_eq!(
        record_forwarded(&state, "r0", &first),
        Ok(false),
        "recipient eviction let a replayed id through"
    );
}

// Catches: an empty or whitespace id/sender/content boundary hashing to the
// same fingerprint (ambiguous concatenation) and a changed body under the same
// id being accepted as a duplicate.
#[test]
fn same_id_with_shifted_sender_content_boundary_is_a_collision() {
    let state = test_state();
    assert_eq!(
        record_forwarded(&state, "r", &message("m", "ab", "c")),
        Ok(true)
    );
    assert_eq!(
        record_forwarded(&state, "r", &message("m", "a", "bc")),
        Err("Forwarded message identity collision")
    );
}

// Catches: an ambiguous LOCAL address ("matches two desktop sessions") falling
// through to remote prefix matching, so a submit/read meant for a local session
// silently lands on the one remote row that shares the prefix.
#[test]
fn ambiguous_local_address_never_selects_a_remote_session() {
    let state = test_state();
    for id in [
        "11111111-89ab-cdef-0123-456789abcdef",
        "11111111-89ab-cdef-0123-456789abcdef0",
    ] {
        crate::state::tests_support::insert_dummy_session(&state, id);
    }
    crate::remote_mirror::store_seed_for_test(
        &state,
        "mint",
        vec![crate::mcp_http::types::SessionInfo {
            session_id: "11111111-remote".into(),
            ..Default::default()
        }],
    );
    let resolved = super::super::remote_mcp_sessions::resolve(
        &state,
        &json!({"action":"submit","session_id":"11111111","input":"x"}),
    );
    assert!(
        !matches!(resolved, Some(Ok(_))),
        "ambiguous local address was routed to a remote host: {resolved:?}"
    );
}

// Catches: probes for unknown hosts leaving permanent entries in connect_locks.
#[tokio::test]
async fn unknown_host_probes_do_not_accumulate_connect_locks() {
    let state = test_state();
    for n in 0..50 {
        let _ = connection(&state, &format!("ghost-{n}")).await;
    }
    assert!(
        state.remote_mail.connect_locks.lock().len() <= 1,
        "connect_locks leaked {} entries",
        state.remote_mail.connect_locks.lock().len()
    );
}

// Catches: a failed peer handshake echoing the connection token (it travels in
// the websocket query string) in the error returned to the MCP caller.
#[tokio::test]
async fn failed_handshake_error_does_not_contain_the_token() {
    let state = test_state();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    state
        .remote
        .force_connected_for_test("mint", &url, Some("SECRET-TOKEN-1419"));
    let error = connection(&state, "mint").await.err().expect("must fail");
    assert!(!error.to_string().contains("SECRET-TOKEN-1419"), "{error}");
}

// Catches: `disconnect` removing the link itself, so the teardown task's
// `remove_if(..).is_some()` is false and the daemon's shadow identities stay
// registered (addressable, listed) after the configured connection is gone.
#[tokio::test]
async fn disconnect_removes_the_shadow_identities_of_that_host() {
    let hub = test_state();
    let mint = test_state();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let token = mint.session_token.read().clone();
    hub.remote
        .force_connected_for_test("mint", &url, Some(&token));
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            super::super::build_remote_router(mint)
                .into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
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
    connection(&hub, "mint").await.expect("link opens");
    assert!(hub.remote_mail.connections.contains_key("mint"));
    disconnect(&hub, "mint");
    let mut cleaned = false;
    for _ in 0..40 {
        if !hub.peer_agents.contains_key("mint/ghost") {
            cleaned = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    server.abort();
    assert!(cleaned, "shadow identity survived disconnect of its host");
}
