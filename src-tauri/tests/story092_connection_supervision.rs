use tuicommander_lib::acp::{
    AcpClientErrorCode, AcpClientManager, AcpConnectRequest, AcpConnectionId,
    AcpConnectionSettlementReason, AcpConnectionSnapshot, AcpConnectionState, AcpReconnectRequest,
};

mod acp_support;

use acp_support::Fixture;

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

#[tokio::test]
async fn ready_connection_settles_when_child_stdout_reaches_clean_eof() {
    let fixture = Fixture::with("ready-eof");
    let snapshot = fixture.connect().await;
    let settled = settled_snapshot(&fixture.manager, snapshot.connection_id).await;
    assert_eq!(settled.state, AcpConnectionState::Failed);
    assert_eq!(
        settled.settlement.unwrap().reason,
        AcpConnectionSettlementReason::Eof
    );
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
