use std::{collections::HashMap, path::Path};

use agent_client_protocol::{AcpAgent, AcpAgentConfig, Agent, Client, ConnectionTo};
use parking_lot::Mutex;
use tokio::{sync::oneshot, task::JoinHandle};

use super::{
    AcpCapabilitySnapshot, AcpClientError, AcpConnectRequest, AcpConnectionId,
    AcpConnectionSettlement, AcpConnectionSettlementReason, AcpConnectionSnapshot,
    AcpConnectionState, EgoAcpConfig, build_initialize_request, capability_snapshot, launch_spec,
};

const INITIAL_GENERATION: u64 = 1;

pub struct AcpClientManager {
    config: EgoAcpConfig,
    connections: Mutex<HashMap<AcpConnectionId, ConnectionHandle>>,
}

struct ConnectionHandle {
    snapshot: AcpConnectionSnapshot,
    shutdown: oneshot::Sender<()>,
    supervisor: JoinHandle<()>,
}

struct InitializedConnection {
    agent_info: Option<agent_client_protocol::schema::v1::Implementation>,
    capabilities: AcpCapabilitySnapshot,
}

impl AcpClientManager {
    #[must_use]
    pub fn new(config: EgoAcpConfig) -> Self {
        Self {
            config,
            connections: Mutex::new(HashMap::new()),
        }
    }

    pub async fn connect(
        &self,
        request: AcpConnectRequest,
    ) -> Result<AcpConnectionSnapshot, AcpClientError> {
        let executable = canonical_executable(&self.config.executable).await?;
        let root = canonical_root(&request.root).await?;
        let spec = launch_spec(&EgoAcpConfig { executable }, &root)?;
        let agent = AcpAgent::new(AcpAgentConfig::new(spec.program).args(spec.args));
        let connection_id = AcpConnectionId::new();
        let (initialized_tx, initialized_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let supervisor = tokio::spawn(supervise_connection(
            connection_id,
            agent,
            initialized_tx,
            shutdown_rx,
        ));

        let initialized = match initialized_rx.await {
            Ok(Ok(initialized)) => initialized,
            Ok(Err(error)) => {
                let _ = supervisor.await;
                return Err(error);
            }
            Err(_) => {
                let _ = supervisor.await;
                return Err(AcpClientError::initialization_failed(
                    connection_id,
                    "ACP process ended before initialization completed",
                ));
            }
        };

        let snapshot = AcpConnectionSnapshot {
            connection_id,
            generation: INITIAL_GENERATION,
            state: AcpConnectionState::Ready,
            agent_info: initialized.agent_info,
            capabilities: Some(initialized.capabilities),
        };
        self.connections.lock().insert(
            connection_id,
            ConnectionHandle {
                snapshot: snapshot.clone(),
                shutdown: shutdown_tx,
                supervisor,
            },
        );

        Ok(snapshot)
    }

    pub fn snapshot(
        &self,
        connection_id: AcpConnectionId,
    ) -> Result<AcpConnectionSnapshot, AcpClientError> {
        self.connections
            .lock()
            .get(&connection_id)
            .map(|connection| connection.snapshot.clone())
            .ok_or_else(|| AcpClientError::not_found(connection_id))
    }

    #[must_use]
    pub fn connection_ids(&self) -> Vec<AcpConnectionId> {
        self.connections.lock().keys().copied().collect()
    }

    pub async fn disconnect(
        &self,
        connection_id: AcpConnectionId,
    ) -> Result<AcpConnectionSettlement, AcpClientError> {
        let connection = self
            .connections
            .lock()
            .remove(&connection_id)
            .ok_or_else(|| AcpClientError::not_found(connection_id))?;
        let _ = connection.shutdown.send(());
        let _ = connection.supervisor.await;

        Ok(AcpConnectionSettlement {
            connection_id,
            generation: connection.snapshot.generation,
            reason: AcpConnectionSettlementReason::Disconnected,
        })
    }
}

async fn supervise_connection(
    connection_id: AcpConnectionId,
    agent: AcpAgent,
    initialized: oneshot::Sender<Result<InitializedConnection, AcpClientError>>,
    shutdown: oneshot::Receiver<()>,
) {
    let _ = Client
        .builder()
        .connect_with(agent, move |connection: ConnectionTo<Agent>| async move {
            let response = match connection
                .send_request(build_initialize_request())
                .block_task()
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    let initialization_error = AcpClientError::initialization_failed(
                        connection_id,
                        format!("ACP initialization failed: {error}"),
                    );
                    let _ = initialized.send(Err(initialization_error));
                    return Err(error);
                }
            };

            let capabilities = match capability_snapshot(&response) {
                Ok(capabilities) => capabilities,
                Err(error) => {
                    let _ = initialized.send(Err(error.with_connection_id(connection_id)));
                    return Ok(());
                }
            };
            let ready = InitializedConnection {
                agent_info: response.agent_info,
                capabilities,
            };
            if initialized.send(Ok(ready)).is_err() {
                return Ok(());
            }

            let _ = shutdown.await;
            Ok(())
        })
        .await;
}

async fn canonical_executable(path: &Path) -> Result<std::path::PathBuf, AcpClientError> {
    if !path.is_absolute() {
        return Err(AcpClientError::invalid_input(
            "ego executable must be an absolute path",
        ));
    }
    let executable = tokio::fs::canonicalize(path).await.map_err(|error| {
        AcpClientError::invalid_input(format!("invalid ego executable: {error}"))
    })?;
    let metadata = tokio::fs::metadata(&executable).await.map_err(|error| {
        AcpClientError::invalid_input(format!("invalid ego executable: {error}"))
    })?;
    if !metadata.is_file() {
        return Err(AcpClientError::invalid_input(
            "ego executable must be a regular file",
        ));
    }
    Ok(executable)
}

async fn canonical_root(path: &Path) -> Result<std::path::PathBuf, AcpClientError> {
    let root = tokio::fs::canonicalize(path)
        .await
        .map_err(|error| AcpClientError::invalid_input(format!("invalid ACP root: {error}")))?;
    let metadata = tokio::fs::metadata(&root)
        .await
        .map_err(|error| AcpClientError::invalid_input(format!("invalid ACP root: {error}")))?;
    if !metadata.is_dir() {
        return Err(AcpClientError::invalid_input(
            "ACP root must be a directory",
        ));
    }
    Ok(root)
}
