//! Story 1156: ego reaches the `tuicommander` MCP server over ACP.
//!
//! The stdio `tuic-bridge` entry opened a fresh HTTP MCP session for every ego
//! tool operation. What replaces it is the connection that already exists: the
//! session is told the server lives on the ACP transport, and `mcp/connect`,
//! `mcp/message` and `mcp/disconnect` arrive on it. These tests hold the
//! connection half — what the session is given, that every MCP frame reaches
//! the host under the identity ego was launched with, and that nothing the host
//! opened outlives the connection. The host itself is the application's MCP
//! handler and is tested beside it.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Map, Value, json};
use tuicommander_lib::acp::{
    AcpConnectionSettlementReason, McpNotify, McpOverAcpError, McpOverAcpHost, McpReply,
};

mod acp_support;

use acp_support::{Fixture, PATIENCE, authority};

const PEER: &str = "550e8400-e29b-41d4-a716-446655440a01";

/// A host that answers from a script and remembers everything it was asked.
#[derive(Default)]
struct Recorder {
    next: AtomicUsize,
    connects: Mutex<Vec<Option<String>>>,
    notifiers: Mutex<Vec<McpNotify>>,
    messages: Mutex<Vec<(String, String)>>,
    notifications: Mutex<Vec<(String, String)>>,
    disconnects: Mutex<Vec<String>>,
    /// Set when a call that never answers is dropped, which is what an abort is.
    abandoned: Arc<std::sync::atomic::AtomicBool>,
}

/// Records its own drop: the only evidence that a cancelled call stopped.
struct Abandoned(Arc<std::sync::atomic::AtomicBool>);

impl Drop for Abandoned {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl McpOverAcpHost for Recorder {
    fn connect(&self, peer_id: Option<&str>, notify: McpNotify) -> Result<String, McpOverAcpError> {
        self.connects.lock().push(peer_id.map(str::to_owned));
        self.notifiers.lock().push(notify);
        Ok(format!(
            "mcp-{}",
            self.next.fetch_add(1, Ordering::SeqCst) + 1
        ))
    }

    fn message(
        &self,
        connection_id: &str,
        method: String,
        _params: Option<Map<String, Value>>,
    ) -> McpReply {
        self.messages
            .lock()
            .push((connection_id.to_owned(), method.clone()));
        let abandoned = Abandoned(Arc::clone(&self.abandoned));
        Box::pin(async move {
            if method == "tools/call" {
                let _held = abandoned;
                return std::future::pending().await;
            }
            match method.as_str() {
                "tools/list" => Ok(json!({ "tools": [{ "name": "session" }] })),
                other => Err(McpOverAcpError::new(
                    -32601,
                    format!("Method not found: {other}"),
                )),
            }
        })
    }

    fn notification(
        &self,
        connection_id: &str,
        method: String,
        _params: Option<Map<String, Value>>,
    ) {
        self.notifications
            .lock()
            .push((connection_id.to_owned(), method));
    }

    fn disconnect(&self, connection_id: &str) {
        self.disconnects.lock().push(connection_id.to_owned());
    }
}

/// Wait, bounded, until `ready` holds.
async fn eventually(what: &str, ready: impl Fn() -> bool) {
    let deadline = tokio::time::Instant::now() + PATIENCE;
    while !ready() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "{what} within {PATIENCE:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn ego_reaches_the_tuicommander_server_over_the_acp_connection() {
    let fixture = Fixture::with("mcp-over-acp");
    let recorder = Arc::new(Recorder::default());
    fixture.manager.set_mcp_host(recorder.clone());
    let connection = fixture
        .manager
        .connect_with_peer(
            &Fixture::config(),
            tuicommander_lib::acp::AcpConnectRequest {
                root: fixture.root(),
            },
            PEER.to_owned(),
        )
        .await
        .expect("connect");

    fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new with the ACP-transport entry");

    // TUIC speaks first only once ego has said something on the connection.
    eventually("ego's initialized notification", || {
        !recorder.notifications.lock().is_empty()
    })
    .await;
    assert_eq!(
        *recorder.notifications.lock(),
        vec![("mcp-1".to_owned(), "notifications/initialized".to_owned())]
    );
    let notify = recorder.notifiers.lock()[0].clone();
    let mut params = Map::new();
    params.insert("uri".to_owned(), json!("tuic://workspace"));
    notify("notifications/resources/updated".to_owned(), Some(params));

    eventually("the second mcp/connect", || {
        recorder.connects.lock().len() == 2
    })
    .await;
    assert_eq!(
        *recorder.connects.lock(),
        vec![Some(PEER.to_owned()), Some(PEER.to_owned())],
        "every MCP connection carries the identity ego was launched under"
    );
    assert_eq!(
        *recorder.messages.lock(),
        vec![
            ("mcp-1".to_owned(), "tools/list".to_owned()),
            ("mcp-1".to_owned(), "tools/boom".to_owned()),
        ],
        "a message for an unknown connection never reaches the host"
    );
    assert_eq!(*recorder.disconnects.lock(), vec!["mcp-1".to_owned()]);

    let settlement = fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .expect("disconnect");
    assert_eq!(
        settlement.reason,
        AcpConnectionSettlementReason::Disconnected,
        "the fixture agent ran every step: an early exit would settle as EOF"
    );
    assert_eq!(
        *recorder.disconnects.lock(),
        vec!["mcp-1".to_owned(), "mcp-2".to_owned()],
        "the connection ego left open is released with the ACP connection"
    );
}

/// The snapshot says which transport this client carries: ACP, and not stdio.
///
/// Neither is something ego advertises for this client to read — they are
/// what *this client* puts in a session — so the snapshot is where a host
/// reads them, and a snapshot still claiming stdio would describe a bridge
/// that is no longer sent.
#[test]
fn the_snapshot_reports_the_acp_transport_this_client_carries() {
    let response = agent_client_protocol::schema::v1::InitializeResponse::new(
        agent_client_protocol::schema::ProtocolVersion::V1,
    );
    let snapshot = tuicommander_lib::acp::capability_snapshot(&response).expect("a v1 snapshot");
    assert!(
        snapshot.mcp_acp,
        "every attended session carries the ACP server"
    );
    assert!(
        !snapshot.mcp_stdio,
        "no session carries a stdio bridge any more"
    );
}

/// A cancelled tool call is answered as cancelled, and stops.
///
/// ego keeps per-call cancellation, so dropping a late reply is not enough: a
/// call that keeps running after ego gave up on it is still driving terminals.
#[tokio::test]
async fn ego_cancels_a_pending_mcp_request_by_its_acp_id() {
    let fixture = Fixture::with("mcp-over-acp-cancel");
    let recorder = Arc::new(Recorder::default());
    fixture.manager.set_mcp_host(recorder.clone());
    let connection = fixture
        .manager
        .connect_with_peer(
            &Fixture::config(),
            tuicommander_lib::acp::AcpConnectRequest {
                root: fixture.root(),
            },
            PEER.to_owned(),
        )
        .await
        .expect("connect");

    eventually("the cancelled call to be dropped", || {
        recorder.abandoned.load(Ordering::SeqCst)
    })
    .await;

    let settlement = fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .expect("disconnect");
    assert_eq!(
        settlement.reason,
        AcpConnectionSettlementReason::Disconnected,
        "the fixture agent saw -32800: a missing or different answer would have ended it early"
    );
}

/// Mail wakes an idle ego with one prompt, and never interrupts a turn.
#[tokio::test]
async fn an_idle_ego_is_woken_once_and_a_busy_one_is_left_alone() {
    const NOTICE: &str = "[TUIC] message available — read it with: agent action=inbox";
    let fixture = Fixture::with("wake-idle-peer");
    let connection = fixture
        .manager
        .connect_with_peer(
            &Fixture::config(),
            tuicommander_lib::acp::AcpConnectRequest {
                root: fixture.root(),
            },
            PEER.to_owned(),
        )
        .await
        .expect("connect");
    assert_eq!(
        fixture
            .manager
            .wake_idle_peer(PEER, NOTICE)
            .await
            .expect("no conversation yet"),
        None,
        "a peer with no session has nothing to wake"
    );
    fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");

    let woken = fixture
        .manager
        .wake_idle_peer(PEER, NOTICE)
        .await
        .expect("wake");
    assert!(woken.is_some(), "an idle session is prompted");
    assert_eq!(
        fixture
            .manager
            .wake_idle_peer(PEER, NOTICE)
            .await
            .expect("second wake"),
        None,
        "the notice turn is running, so nothing more is sent"
    );

    let settlement = fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .expect("disconnect");
    assert_eq!(
        settlement.reason,
        AcpConnectionSettlementReason::Disconnected,
        "a second prompt would have ended the fixture agent early"
    );
}
