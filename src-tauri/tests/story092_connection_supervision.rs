use std::path::PathBuf;

use tempfile::tempdir;
use tuicommander_lib::acp::{
    AcpClientErrorCode, AcpClientManager, AcpConnectRequest, AcpConnectionId, AcpConnectionState,
    EgoAcpConfig,
};

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
