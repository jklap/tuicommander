use tuicommander_lib::acp::{
    AcpClientErrorCode, AcpClientManager, AcpConnectRequest, AcpConnectionId,
    AcpConnectionSettlementReason, AcpConnectionSnapshot, AcpConnectionState, AcpReconnectRequest,
};

mod acp_support;

use acp_support::{Fixture, authority};

async fn settled_snapshot(
    manager: &AcpClientManager,
    id: AcpConnectionId,
) -> AcpConnectionSnapshot {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let snapshot = manager.snapshot(id).unwrap();
            if snapshot.settlement.is_some() {
                return snapshot;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("connection did not settle")
}

#[tokio::test]
async fn connect_directly_launches_configured_ego_and_stores_ready_snapshot() {
    let fixture = Fixture::with("ready");
    let snapshot = fixture.connect().await;
    assert_eq!(snapshot.state, AcpConnectionState::Ready);
    assert!(snapshot.capabilities.is_some());
    assert_eq!(
        fixture.manager.snapshot(snapshot.connection_id).unwrap(),
        snapshot
    );
    fixture
        .manager
        .disconnect(snapshot.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn malformed_non_v1_and_early_eof_leave_no_registered_connection() {
    for scenario in ["malformed", "non-v1", "early-eof"] {
        let fixture = Fixture::with(scenario);
        let error = fixture
            .manager
            .connect(
                &Fixture::config(),
                AcpConnectRequest {
                    root: fixture.root(),
                },
            )
            .await
            .unwrap_err();
        assert!(
            matches!(
                error.code,
                AcpClientErrorCode::InitializationFailed | AcpClientErrorCode::UnsupportedProtocol
            ),
            "{scenario}: {error:?}"
        );
        assert!(fixture.manager.connection_ids().is_empty(), "{scenario}");
    }
}

#[tokio::test]
async fn unknown_snapshot_is_typed_not_found() {
    let manager = AcpClientManager::new();
    let error = manager.snapshot(AcpConnectionId::new()).unwrap_err();
    assert_eq!(error.code, AcpClientErrorCode::NotFound);
}

/// Clean EOF, whether or not the child is still there to explain it.
///
/// Both scenarios end the protocol the same way and must settle the same way,
/// and that is the point of running them together: `ready-eof` exits, so a
/// client watching the process would also notice, while `stdout-closed-alive`
/// closes the pipe and keeps running, so the process is no help at all. The
/// stream is the only thing either of them agrees on.
#[tokio::test]
async fn a_connection_settles_on_eof_whether_or_not_the_child_outlives_its_stdout() {
    for scenario in ["ready-eof", "stdout-closed-alive"] {
        let fixture = Fixture::with(scenario);
        let snapshot = fixture.connect().await;
        let settled = settled_snapshot(&fixture.manager, snapshot.connection_id).await;
        assert_eq!(settled.state, AcpConnectionState::Failed, "{scenario}");
        assert_eq!(
            settled.settlement.unwrap().reason,
            AcpConnectionSettlementReason::Eof,
            "{scenario}"
        );
    }
}

#[tokio::test]
async fn reconnect_settles_old_connection_and_starts_a_new_generation() {
    let fixture = Fixture::with("ready");
    let old = fixture.connect().await;
    let fresh = fixture
        .manager
        .reconnect(
            &Fixture::config(),
            AcpReconnectRequest {
                connection_id: old.connection_id,
                root: fixture.root(),
            },
        )
        .await
        .unwrap();
    assert_ne!(fresh.connection_id, old.connection_id);
    assert!(fresh.generation > old.generation);
    assert!(fresh.attachments.is_empty());
    let old_settled = fixture.manager.snapshot(old.connection_id).unwrap();
    assert_eq!(old_settled.state, AcpConnectionState::Closed);
    assert_eq!(
        old_settled.settlement.unwrap().reason,
        AcpConnectionSettlementReason::Disconnected
    );
    assert_eq!(fresh.state, AcpConnectionState::Ready);
    assert!(fresh.settlement.is_none());
    fixture
        .manager
        .disconnect(fresh.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn kill_settles_and_retains_a_killed_snapshot_without_session_operations() {
    let fixture = Fixture::with("ready");
    let ready = fixture.connect().await;
    let settlement = fixture.manager.kill(ready.connection_id).await.unwrap();
    assert_eq!(settlement.reason, AcpConnectionSettlementReason::Killed);
    let killed = fixture.manager.snapshot(ready.connection_id).unwrap();
    assert_eq!(killed.state, AcpConnectionState::Killed);
    assert_eq!(killed.generation, ready.generation);
    assert_eq!(
        killed.settlement.unwrap().reason,
        AcpConnectionSettlementReason::Killed
    );
    assert!(killed.attachments.is_empty());
}

/// How many settled connections the manager is expected to keep.
///
/// Must match `manager::SETTLED_RETAINED`, which is private because it is a
/// retention policy rather than part of the vocabulary a host speaks. Pinned
/// here instead: the number is the contract, and a change to it should have to
/// be made twice on purpose.
const SETTLED_RETAINED: usize = 8;

/// A settled connection is kept so it can be read, not kept forever.
///
/// Reading one after it ends is the whole reason it stays: the settlement
/// reason, the final attachments, the tail of the stream. But every one of them
/// holds a journal of up to a thousand events, and reconnecting is an ordinary
/// thing to do repeatedly — a `reconnect` loop against an agent that keeps
/// dying would otherwise grow this map, and the memory behind it, for as long
/// as the app runs.
#[tokio::test]
async fn settled_connections_are_kept_to_be_read_and_then_forgotten() {
    let fixture = Fixture::with("ready");
    let mut ended = Vec::new();
    for _ in 0..SETTLED_RETAINED + 2 {
        let connection = fixture.connect().await;
        fixture
            .manager
            .disconnect(connection.connection_id)
            .await
            .expect("disconnect");
        ended.push(connection.connection_id);
        // Checked every time round, not only at the end. Forgetting one too
        // many leaves the same eight here after ten, and the only place the
        // difference shows is the settlement that made room it did not need.
        assert_eq!(
            fixture.manager.connection_ids().len(),
            ended.len().min(SETTLED_RETAINED),
            "after {} settlements",
            ended.len()
        );
    }

    let (forgotten, kept) = ended.split_at(ended.len() - SETTLED_RETAINED);
    for id in forgotten {
        assert_eq!(
            fixture.manager.snapshot(*id).unwrap_err().code,
            AcpClientErrorCode::NotFound,
            "the oldest settled connections are the ones let go"
        );
    }
    for id in kept {
        assert!(
            fixture.manager.snapshot(*id).unwrap().settlement.is_some(),
            "a connection that just ended is still there to be asked about"
        );
    }
}

/// An agent that denies a method it advertised.
///
/// Every later decision this client makes is read off the capability snapshot
/// taken at `initialize`, and the snapshot is immutable on purpose: a host that
/// was told an operation exists must not find out otherwise one operation at a
/// time. So a `method_not_found` for something the agent published is not a
/// refusal to hand back and carry on from — it is the agent contradicting the
/// only thing this client knows about it, and the connection has nothing left
/// to offer. `session/new` is the clearest case, because v1 makes it mandatory:
/// no negotiated option is involved, only the protocol version the agent itself
/// answered with.
#[tokio::test]
async fn an_advertised_method_denied_on_the_wire_fails_the_whole_connection() {
    let fixture = Fixture::with("denies-advertised-method");
    let ready = fixture.connect().await;
    let error = fixture
        .manager
        .new_session(ready.connection_id, authority(fixture.root()))
        .await
        .unwrap_err();
    assert_eq!(error.code, AcpClientErrorCode::ProtocolViolation);
    assert!(
        !error.retryable,
        "the same agent will contradict itself the same way"
    );

    let settled = settled_snapshot(&fixture.manager, ready.connection_id).await;
    assert_eq!(settled.state, AcpConnectionState::Failed);
    assert_eq!(
        settled.settlement.unwrap().reason,
        AcpConnectionSettlementReason::ProtocolViolation
    );
    assert_eq!(
        settled.capabilities, ready.capabilities,
        "the snapshot is what was contradicted, so it is left exactly as taken"
    );
}
