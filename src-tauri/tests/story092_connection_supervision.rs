use std::path::PathBuf;

use tempfile::tempdir;
use tuicommander_lib::acp::{
    AcpClientErrorCode, AcpClientManager, AcpConnectRequest, AcpConnectionId,
    AcpConnectionSettlementReason, AcpConnectionState, AcpReconnectRequest, EgoAcpConfig,
};

async fn settled_snapshot(
    manager: &AcpClientManager,
    id: AcpConnectionId,
) -> tuicommander_lib::acp::AcpConnectionSnapshot {
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

fn fake() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_story092_acp_fake"))
}

#[tokio::test]
async fn connect_directly_launches_configured_ego_and_stores_ready_snapshot() {
    let root = tempdir().unwrap();
    std::fs::copy(
        "tests/fixtures/acp/ready.json",
        root.path().join("scenario.json"),
    )
    .unwrap();
    let manager = AcpClientManager::new(EgoAcpConfig { executable: fake() });
    let snapshot = manager
        .connect(AcpConnectRequest {
            root: root.path().to_path_buf(),
        })
        .await
        .unwrap();
    assert_eq!(snapshot.state, AcpConnectionState::Ready);
    assert!(snapshot.capabilities.is_some());
    assert_eq!(manager.snapshot(snapshot.connection_id).unwrap(), snapshot);
    manager.disconnect(snapshot.connection_id).await.unwrap();
}

#[tokio::test]
async fn malformed_non_v1_and_early_eof_leave_no_registered_connection() {
    for scenario in ["malformed.json", "non-v1.json", "early-eof.json"] {
        let root = tempdir().unwrap();
        std::fs::copy(
            format!("tests/fixtures/acp/{scenario}"),
            root.path().join("scenario.json"),
        )
        .unwrap();
        let manager = AcpClientManager::new(EgoAcpConfig { executable: fake() });
        let error = manager
            .connect(AcpConnectRequest {
                root: root.path().to_path_buf(),
            })
            .await
            .unwrap_err();
        assert!(matches!(
            error.code,
            AcpClientErrorCode::InitializationFailed | AcpClientErrorCode::UnsupportedProtocol
        ));
        assert!(manager.connection_ids().is_empty());
    }
}

#[tokio::test]
async fn unknown_snapshot_is_typed_not_found() {
    let manager = AcpClientManager::new(EgoAcpConfig { executable: fake() });
    let error = manager.snapshot(AcpConnectionId::new()).unwrap_err();
    assert_eq!(error.code, AcpClientErrorCode::NotFound);
}

#[tokio::test]
async fn ready_connection_settles_when_child_stdout_reaches_clean_eof() {
    let root = tempdir().unwrap();
    std::fs::copy(
        "tests/fixtures/acp/ready-eof.json",
        root.path().join("scenario.json"),
    )
    .unwrap();
    let manager = AcpClientManager::new(EgoAcpConfig { executable: fake() });
    let snapshot = manager
        .connect(AcpConnectRequest {
            root: root.path().to_path_buf(),
        })
        .await
        .unwrap();
    let settled = settled_snapshot(&manager, snapshot.connection_id).await;
    assert_eq!(settled.state, AcpConnectionState::Failed);
    assert_eq!(
        settled.settlement.unwrap().reason,
        AcpConnectionSettlementReason::Eof
    );
}

#[tokio::test]
async fn reconnect_settles_old_connection_and_starts_a_new_generation() {
    let root = tempdir().unwrap();
    std::fs::copy(
        "tests/fixtures/acp/ready.json",
        root.path().join("scenario.json"),
    )
    .unwrap();
    let manager = AcpClientManager::new(EgoAcpConfig { executable: fake() });
    let old = manager
        .connect(AcpConnectRequest {
            root: root.path().to_path_buf(),
        })
        .await
        .unwrap();
    let fresh = manager
        .reconnect(AcpReconnectRequest {
            connection_id: old.connection_id,
            root: root.path().to_path_buf(),
        })
        .await
        .unwrap();
    assert_ne!(fresh.connection_id, old.connection_id);
    assert!(fresh.generation > old.generation);
    assert!(fresh.attachments.is_empty());
    let old_settled = manager.snapshot(old.connection_id).unwrap();
    assert_eq!(old_settled.state, AcpConnectionState::Closed);
    assert_eq!(
        old_settled.settlement.unwrap().reason,
        AcpConnectionSettlementReason::Disconnected
    );
    assert_eq!(fresh.state, AcpConnectionState::Ready);
    assert!(fresh.settlement.is_none());
    manager.disconnect(fresh.connection_id).await.unwrap();
}
