use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use agent_client_protocol::{AcpAgent, AcpAgentConfig, Agent, Client, ConnectionTo};
use parking_lot::Mutex;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use super::connection::{Command, ConnectionActor};
use super::{
    AcpAttachmentSnapshot, AcpCapabilitySnapshot, AcpClientError, AcpConnectRequest,
    AcpConnectionId, AcpConnectionSettlement, AcpConnectionSettlementReason, AcpConnectionSnapshot,
    AcpConnectionState, AcpReconnectRequest, AcpSessionAuthority, EgoAcpConfig,
    build_initialize_request, capability_snapshot, launch_spec,
};

const INITIAL_GENERATION: u64 = 1;

/// How many commands may wait for the actor before a caller is made to wait.
///
/// Bounded rather than unbounded on purpose: an unbounded queue turns a stalled
/// agent into unbounded memory and hides the stall until something else breaks.
/// The number is small because a queue this deep already means the connection
/// is not keeping up, and the honest thing then is backpressure on the caller.
const COMMAND_QUEUE: usize = 32;

pub struct AcpClientManager {
    config: EgoAcpConfig,
    connections: Arc<Mutex<HashMap<AcpConnectionId, ConnectionHandle>>>,
    next_generation: AtomicU64,
}

struct ConnectionHandle {
    snapshot: AcpConnectionSnapshot,
    commands: mpsc::Sender<Command>,
    shutdown: Option<oneshot::Sender<()>>,
    supervisor: Option<JoinHandle<()>>,
}

struct InitializedConnection {
    agent_info: Option<agent_client_protocol::schema::v1::Implementation>,
    capabilities: AcpCapabilitySnapshot,
}

#[derive(Debug, Clone, Copy)]
enum SupervisorExit {
    NotReady,
    Disconnected,
    Eof,
}

impl AcpClientManager {
    #[must_use]
    pub fn new(config: EgoAcpConfig) -> Self {
        Self {
            config,
            connections: Arc::new(Mutex::new(HashMap::new())),
            next_generation: AtomicU64::new(INITIAL_GENERATION),
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
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let (initialized_tx, initialized_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (registered_tx, registered_rx) = oneshot::channel();
        let (commands_tx, commands_rx) = mpsc::channel(COMMAND_QUEUE);

        let supervisor = tokio::spawn(supervise_connection(
            connection_id,
            generation,
            agent,
            SupervisorWiring {
                initialized: initialized_tx,
                registered: registered_rx,
                commands: commands_rx,
                shutdown: shutdown_rx,
            },
            Arc::clone(&self.connections),
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
            generation,
            state: AcpConnectionState::Ready,
            agent_info: initialized.agent_info,
            capabilities: Some(initialized.capabilities),
            attachments: Vec::new(),
            earliest_sequence: 0,
            latest_sequence: 0,
            settlement: None,
        };
        self.connections.lock().insert(
            connection_id,
            ConnectionHandle {
                snapshot: snapshot.clone(),
                commands: commands_tx,
                shutdown: Some(shutdown_tx),
                supervisor: Some(supervisor),
            },
        );
        let _ = registered_tx.send(());

        Ok(snapshot)
    }

    pub async fn reconnect(
        &self,
        request: AcpReconnectRequest,
    ) -> Result<AcpConnectionSnapshot, AcpClientError> {
        self.snapshot(request.connection_id)?;
        self.disconnect(request.connection_id).await?;
        self.connect(AcpConnectRequest { root: request.root }).await
    }

    /// Open a durable session on a live connection.
    ///
    /// The authority is supplied here rather than remembered, because a session
    /// created against a wider authority than the caller currently holds is a
    /// privilege the caller was never granted.
    pub async fn new_session(
        &self,
        connection_id: AcpConnectionId,
        authority: AcpSessionAuthority,
    ) -> Result<AcpAttachmentSnapshot, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::NewSession {
            authority,
            reply,
        })
        .await
    }

    pub async fn list_sessions(
        &self,
        connection_id: AcpConnectionId,
        request: agent_client_protocol::schema::v1::ListSessionsRequest,
    ) -> Result<agent_client_protocol::schema::v1::ListSessionsResponse, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::ListSessions {
            request,
            reply,
        })
        .await
    }

    /// Hand one command to a connection's actor and wait for its answer.
    ///
    /// A connection this manager never had and one that has settled are
    /// different answers on purpose: the first is a caller naming something
    /// that does not exist, the second is a caller holding an id that was real
    /// and no longer is, and only the second means "make a new connection".
    async fn dispatch<T>(
        &self,
        connection_id: AcpConnectionId,
        command: impl FnOnce(oneshot::Sender<Result<T, AcpClientError>>) -> Command,
    ) -> Result<T, AcpClientError> {
        let commands = {
            let connections = self.connections.lock();
            let connection = connections
                .get(&connection_id)
                .ok_or_else(|| AcpClientError::not_found(connection_id))?;
            if connection.snapshot.settlement.is_some() {
                return Err(AcpClientError::transport_closed(connection_id));
            }
            connection.commands.clone()
        };

        let (reply, answer) = oneshot::channel();
        // Both failures below mean the actor is gone: the connection settled
        // between the check above and now, which is exactly the race the
        // settled state describes rather than a failure of the operation.
        commands
            .send(command(reply))
            .await
            .map_err(|_| AcpClientError::transport_closed(connection_id))?;
        answer
            .await
            .map_err(|_| AcpClientError::transport_closed(connection_id))?
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
        let (shutdown, supervisor, settled) = {
            let mut connections = self.connections.lock();
            let connection = connections
                .get_mut(&connection_id)
                .ok_or_else(|| AcpClientError::not_found(connection_id))?;
            if let Some(settlement) = connection.snapshot.settlement {
                (None, None, Some(settlement))
            } else {
                connection.snapshot.state = AcpConnectionState::Closing;
                (
                    connection.shutdown.take(),
                    connection.supervisor.take(),
                    None,
                )
            }
        };
        if let Some(settlement) = settled {
            return Ok(settlement);
        }

        if let Some(shutdown) = shutdown {
            let _ = shutdown.send(());
        }
        if let Some(supervisor) = supervisor {
            let _ = supervisor.await;
        }

        self.snapshot(connection_id)?.settlement.ok_or_else(|| {
            AcpClientError::initialization_failed(
                connection_id,
                "ACP supervisor ended without settling the connection",
            )
        })
    }

    pub async fn kill(
        &self,
        connection_id: AcpConnectionId,
    ) -> Result<AcpConnectionSettlement, AcpClientError> {
        let (shutdown, supervisor, generation, settled) = {
            let mut connections = self.connections.lock();
            let connection = connections
                .get_mut(&connection_id)
                .ok_or_else(|| AcpClientError::not_found(connection_id))?;
            (
                connection.shutdown.take(),
                connection.supervisor.take(),
                connection.snapshot.generation,
                connection.snapshot.settlement,
            )
        };
        if let Some(settlement) = settled {
            return Ok(settlement);
        }

        if let Some(supervisor) = supervisor {
            supervisor.abort();
            let _ = supervisor.await;
        }
        drop(shutdown);
        settle_connection(
            &self.connections,
            connection_id,
            generation,
            AcpConnectionSettlementReason::Killed,
        );
        self.snapshot(connection_id)?.settlement.ok_or_else(|| {
            AcpClientError::initialization_failed(connection_id, "ACP kill did not settle")
        })
    }
}

/// Every channel a supervisor is wired to, in the order it uses them.
///
/// They travel together because they are one handshake: the supervisor reports
/// initialization, waits to be registered, then serves commands until told to
/// stop. Passing them separately said nothing extra and only made the call
/// site a list of look-alike channels.
struct SupervisorWiring {
    initialized: oneshot::Sender<Result<InitializedConnection, AcpClientError>>,
    registered: oneshot::Receiver<()>,
    commands: mpsc::Receiver<Command>,
    shutdown: oneshot::Receiver<()>,
}

async fn supervise_connection(
    connection_id: AcpConnectionId,
    generation: u64,
    agent: AcpAgent,
    wiring: SupervisorWiring,
    connections: Arc<Mutex<HashMap<AcpConnectionId, ConnectionHandle>>>,
) {
    let SupervisorWiring {
        initialized,
        registered,
        mut commands,
        shutdown,
    } = wiring;
    let ready = Arc::new(AtomicBool::new(false));
    let closure_ready = Arc::clone(&ready);
    let actor_connections = Arc::clone(&connections);
    let outcome = Client
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
                Ok(capabilities) => Arc::new(capabilities),
                Err(error) => {
                    let _ = initialized.send(Err(error.with_connection_id(connection_id)));
                    return Ok(SupervisorExit::NotReady);
                }
            };
            let ready = InitializedConnection {
                agent_info: response.agent_info,
                capabilities: AcpCapabilitySnapshot::clone(&capabilities),
            };
            if initialized.send(Ok(ready)).is_err() {
                return Ok(SupervisorExit::NotReady);
            }
            closure_ready.store(true, Ordering::Release);
            if registered.await.is_err() {
                return Ok(SupervisorExit::NotReady);
            }

            let mut actor = ConnectionActor::new(connection_id, capabilities);
            let mut shutdown = shutdown;
            loop {
                tokio::select! {
                    biased;
                    _ = &mut shutdown => return Ok(SupervisorExit::Disconnected),
                    () = connection.incoming_closed() => return Ok(SupervisorExit::Eof),
                    command = commands.recv() => match command {
                        // The manager holds the only sender, so this is the
                        // manager itself going away, not a caller hanging up.
                        None => return Ok(SupervisorExit::Disconnected),
                        Some(command) => {
                            actor
                                .handle(command, &connection, |attachments| {
                                    publish_attachments(
                                        &actor_connections,
                                        connection_id,
                                        attachments,
                                    );
                                })
                                .await;
                        }
                    },
                }
            }
        })
        .await;

    let reason = match outcome {
        Ok(SupervisorExit::Disconnected) => AcpConnectionSettlementReason::Disconnected,
        Ok(SupervisorExit::Eof) => AcpConnectionSettlementReason::Eof,
        Ok(SupervisorExit::NotReady) => return,
        Err(_) if ready.load(Ordering::Acquire) => AcpConnectionSettlementReason::TransportError,
        _ => return,
    };
    settle_connection(&connections, connection_id, generation, reason);
}

/// Republish what a connection is attached to.
///
/// A settled connection is left alone: its attachment list is the last true
/// thing anyone knew about it, and a session recorded after settlement would
/// claim an attachment on a connection that can no longer carry one.
fn publish_attachments(
    connections: &Mutex<HashMap<AcpConnectionId, ConnectionHandle>>,
    connection_id: AcpConnectionId,
    attachments: Vec<AcpAttachmentSnapshot>,
) {
    let mut connections = connections.lock();
    let Some(connection) = connections.get_mut(&connection_id) else {
        return;
    };
    if connection.snapshot.settlement.is_some() {
        return;
    }
    connection.snapshot.attachments = attachments;
}

fn settle_connection(
    connections: &Mutex<HashMap<AcpConnectionId, ConnectionHandle>>,
    connection_id: AcpConnectionId,
    generation: u64,
    reason: AcpConnectionSettlementReason,
) {
    let mut connections = connections.lock();
    let Some(connection) = connections.get_mut(&connection_id) else {
        return;
    };
    if connection.snapshot.settlement.is_some() {
        return;
    }

    connection.snapshot.state = match reason {
        AcpConnectionSettlementReason::Disconnected => AcpConnectionState::Closed,
        AcpConnectionSettlementReason::Killed => AcpConnectionState::Killed,
        AcpConnectionSettlementReason::Eof
        | AcpConnectionSettlementReason::TransportError
        | AcpConnectionSettlementReason::WriteError
        | AcpConnectionSettlementReason::InitializationFailed
        | AcpConnectionSettlementReason::ProtocolViolation => AcpConnectionState::Failed,
    };
    connection.snapshot.settlement = Some(AcpConnectionSettlement {
        connection_id,
        generation,
        reason,
    });
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
