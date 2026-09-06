use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use agent_client_protocol::schema::v1;
use agent_client_protocol::{AcpAgent, AcpAgentConfig, Agent, Client, ConnectionTo, Responder};
use parking_lot::Mutex;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use futures_util::StreamExt;

use super::connection::{
    Accepted, Answer, Command, ConnectionActor, InFlight, Inbound, Interaction,
};
use super::events::{AcpEventJournal, AcpEventStream};
use super::{
    AcpAttachKind, AcpAttachmentSnapshot, AcpCapabilitySnapshot, AcpClientError, AcpConnectRequest,
    AcpConnectionId, AcpConnectionSettlement, AcpConnectionSettlementReason, AcpConnectionSnapshot,
    AcpConnectionState, AcpDetachKind, AcpHostRequestId, AcpInteractionSettlement,
    AcpPendingInteraction, AcpReconnectRequest, AcpSessionAuthority, AcpTurnId, EgoAcpConfig,
    EgoCompactRequest, EgoCompactResponse, EgoHoldRequest, EgoHoldResponse,
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

/// How many incoming updates may wait for the actor.
///
/// Deep because the producer is the SDK dispatch loop, which cannot be made to
/// wait without stalling the protocol, and a replayed session arrives as a
/// burst of chunks. Overflowing it is a fatal connection error rather than a
/// dropped update: a host that renders a turn with a hole in the middle of it
/// is confidently wrong about what the agent said.
const UPDATE_QUEUE: usize = 4096;

pub struct AcpClientManager {
    config: EgoAcpConfig,
    connections: Arc<Mutex<HashMap<AcpConnectionId, ConnectionHandle>>>,
    next_generation: AtomicU64,
}

struct ConnectionHandle {
    snapshot: AcpConnectionSnapshot,
    commands: mpsc::Sender<Command>,
    journal: Arc<AcpEventJournal>,
    shutdown: Option<oneshot::Sender<()>>,
    supervisor: Option<JoinHandle<()>>,
}

struct InitializedConnection {
    agent_info: Option<v1::Implementation>,
    capabilities: AcpCapabilitySnapshot,
}

/// What one pass of the supervisor's select decided to do.
enum Step {
    Command(Command),
    Accept(Accepted),
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
        let (inbound, updates_rx) = mpsc::channel(UPDATE_QUEUE);
        let journal = Arc::new(AcpEventJournal::new(connection_id, generation));

        let supervisor = tokio::spawn(supervise_connection(
            connection_id,
            generation,
            agent,
            SupervisorWiring {
                initialized: initialized_tx,
                registered: registered_rx,
                commands: commands_rx,
                inbound,
                updates: updates_rx,
                shutdown: shutdown_rx,
                journal: Arc::clone(&journal),
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

        // The bounds come from the journal here too, so what this returns and
        // what a later `snapshot` reports answer to one convention rather than
        // to a literal that happens to agree with an empty journal.
        let (earliest_sequence, latest_sequence) = journal.bounds();
        let snapshot = AcpConnectionSnapshot {
            connection_id,
            generation,
            state: AcpConnectionState::Ready,
            agent_info: initialized.agent_info,
            capabilities: Some(initialized.capabilities),
            attachments: Vec::new(),
            earliest_sequence,
            latest_sequence,
            settlement: None,
        };
        self.connections.lock().insert(
            connection_id,
            ConnectionHandle {
                snapshot: snapshot.clone(),
                commands: commands_tx,
                journal,
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

    /// Attach this connection to a session ego already owns.
    ///
    /// The returned attachment names the session that is now attached, which
    /// for a fork is not the one that was asked for.
    pub async fn attach(
        &self,
        connection_id: AcpConnectionId,
        kind: AcpAttachKind,
        session_id: v1::SessionId,
        authority: AcpSessionAuthority,
    ) -> Result<AcpAttachmentSnapshot, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::Attach {
            kind,
            session_id,
            authority,
            reply,
        })
        .await
    }

    pub async fn detach(
        &self,
        connection_id: AcpConnectionId,
        kind: AcpDetachKind,
        session_id: v1::SessionId,
    ) -> Result<(), AcpClientError> {
        self.dispatch(connection_id, |reply| Command::Detach {
            kind,
            session_id,
            reply,
        })
        .await
    }

    pub async fn list_sessions(
        &self,
        connection_id: AcpConnectionId,
        request: v1::ListSessionsRequest,
    ) -> Result<v1::ListSessionsResponse, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::ListSessions {
            request,
            reply,
        })
        .await
    }

    /// Start a turn and get back its id, not its outcome.
    ///
    /// The outcome is an event, because a turn outlives the call that started
    /// it and more than one reader needs to know how it ended.
    pub async fn prompt(
        &self,
        connection_id: AcpConnectionId,
        session_id: v1::SessionId,
        prompt: Vec<v1::ContentBlock>,
    ) -> Result<AcpTurnId, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::Prompt {
            session_id,
            prompt,
            reply,
        })
        .await
    }

    /// Ask the running turn to stop. It settles on its own response.
    pub async fn cancel(
        &self,
        connection_id: AcpConnectionId,
        session_id: v1::SessionId,
    ) -> Result<(), AcpClientError> {
        self.dispatch(connection_id, |reply| Command::Cancel { session_id, reply })
            .await
    }

    /// Set one config option and take the full set back.
    ///
    /// The whole set is the answer because it is what the agent sent: setting
    /// one option can change what the others offer, and a client that returned
    /// only the one it set would leave a host rendering a stale list.
    pub async fn set_config_option(
        &self,
        connection_id: AcpConnectionId,
        request: v1::SetSessionConfigOptionRequest,
    ) -> Result<Vec<v1::SessionConfigOption>, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::SetConfigOption {
            request,
            reply,
        })
        .await
    }

    /// Ask ego to hold this session at the next boundary.
    ///
    /// `pending` is not a failure: it means the request is recorded and the
    /// turn has not reached a boundary yet. Only `paused` says nothing more
    /// runs.
    pub async fn pause_turn(
        &self,
        connection_id: AcpConnectionId,
        request: EgoHoldRequest,
    ) -> Result<EgoHoldResponse, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::Hold {
            release: false,
            request,
            reply,
        })
        .await
    }

    /// Release a hold. Not `session/resume`, which attaches a session.
    pub async fn resume_turn(
        &self,
        connection_id: AcpConnectionId,
        request: EgoHoldRequest,
    ) -> Result<EgoHoldResponse, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::Hold {
            release: true,
            request,
            reply,
        })
        .await
    }

    /// Compact a session into a successor.
    ///
    /// The receipt's publication is the part that matters: a target that may
    /// have been published is never retried as a new compaction, because that
    /// risks a second successor for one source.
    pub async fn compact(
        &self,
        connection_id: AcpConnectionId,
        request: EgoCompactRequest,
    ) -> Result<EgoCompactResponse, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::Compact { request, reply })
            .await
    }

    /// Give a person's decision to the permission request that is waiting on it.
    pub async fn respond_permission(
        &self,
        connection_id: AcpConnectionId,
        request_id: AcpHostRequestId,
        outcome: v1::RequestPermissionOutcome,
    ) -> Result<AcpInteractionSettlement, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::Respond {
            request_id,
            answer: Answer::Permission(outcome),
            reply,
        })
        .await
    }

    /// Give a person's decision to the elicitation that is waiting on it.
    pub async fn respond_elicitation(
        &self,
        connection_id: AcpConnectionId,
        request_id: AcpHostRequestId,
        action: v1::ElicitationAction,
    ) -> Result<AcpInteractionSettlement, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::Respond {
            request_id,
            answer: Answer::Elicitation(action),
            reply,
        })
        .await
    }

    /// Every request this connection is waiting on a person for, in the order
    /// the agent asked.
    pub async fn pending_interactions(
        &self,
        connection_id: AcpConnectionId,
    ) -> Result<Vec<AcpPendingInteraction>, AcpClientError> {
        self.dispatch(connection_id, |reply| Command::PendingInteractions {
            reply,
        })
        .await
    }

    /// Read what happened on a connection, from `from` onwards.
    ///
    /// Works on a settled connection too: what it recorded is still true, and a
    /// host that reconnects to read the end of a turn should not be told the
    /// connection is gone before it has seen how the turn ended.
    pub fn subscribe(
        &self,
        connection_id: AcpConnectionId,
        from: u64,
    ) -> Result<AcpEventStream, AcpClientError> {
        let journal = {
            let connections = self.connections.lock();
            let connection = connections
                .get(&connection_id)
                .ok_or_else(|| AcpClientError::not_found(connection_id))?;
            Arc::clone(&connection.journal)
        };
        journal.subscribe(from)
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
            .map(|connection| {
                // Read from the journal rather than from a copy kept in step
                // with it: the bounds move with every event, and a copy would
                // have to be republished on each one just to stay true.
                let (earliest, latest) = connection.journal.bounds();
                AcpConnectionSnapshot {
                    earliest_sequence: earliest,
                    latest_sequence: latest,
                    ..connection.snapshot.clone()
                }
            })
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

/// Hand one reverse request to the actor, or refuse it here if it cannot be.
///
/// A full queue is the one case this cannot do: the request is settled on the
/// spot with the answer that grants nothing, out of the order the agent asked
/// in. That is the honest trade — the alternative is an agent waiting forever
/// on a seat that was never taken.
fn queue_interaction(
    inbound: &mpsc::Sender<Inbound>,
    interaction: Interaction,
) -> Result<(), agent_client_protocol::Error> {
    if let Err(error) = inbound.try_send(Inbound::Interaction(Box::new(interaction))) {
        let Inbound::Interaction(interaction) = error.into_inner() else {
            unreachable!("the value returned is the one that was just sent");
        };
        interaction.refuse();
    }
    Ok(())
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
    /// Both halves of the inbound channel: the sender belongs to the SDK
    /// dispatch callbacks, which are registered here rather than by the caller.
    inbound: mpsc::Sender<Inbound>,
    updates: mpsc::Receiver<Inbound>,
    shutdown: oneshot::Receiver<()>,
    journal: Arc<AcpEventJournal>,
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
        inbound,
        mut updates,
        shutdown,
        journal,
    } = wiring;
    let ready = Arc::new(AtomicBool::new(false));
    let closure_ready = Arc::clone(&ready);
    let actor_connections = Arc::clone(&connections);
    // One sender per callback, one channel for all of them: updates and the
    // requests the agent is waiting on arrive interleaved on the wire, and the
    // order between them is the record. Two channels would let a permission
    // request overtake the update that explains why it was asked.
    let updates_in = inbound.clone();
    let permissions_in = inbound.clone();
    let elicitations_in = inbound;
    let outcome = Client
        .builder()
        // Deliberately short, because it holds the SDK's dispatch loop: the
        // update is handed to the actor and nothing else happens here.
        .on_receive_notification(
            async move |notification: v1::SessionNotification, _connection| {
                updates_in
                    .try_send(Inbound::Update(Box::new(notification)))
                    .map_err(|error| {
                        // Never a silent drop. A host that renders a turn with
                        // a hole in it is confidently wrong about what the
                        // agent said, which is worse than a connection that
                        // failed.
                        agent_client_protocol::Error::internal_error()
                            .data(format!("ACP update queue: {error}"))
                    })
            },
            agent_client_protocol::on_receive_notification!(),
        )
        // The responder is handed to the actor rather than answered here: a
        // person decides this, and holding the dispatch loop until they do
        // would stop every other message on the connection — including the
        // updates that say what the agent is asking about.
        .on_receive_request(
            async move |request: v1::RequestPermissionRequest,
                        responder: Responder<v1::RequestPermissionResponse>,
                        _connection| {
                queue_interaction(
                    &permissions_in,
                    Interaction::Permission {
                        request: Box::new(request),
                        responder,
                    },
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: v1::CreateElicitationRequest,
                        responder: Responder<v1::CreateElicitationResponse>,
                        _connection| {
                queue_interaction(
                    &elicitations_in,
                    Interaction::Elicitation {
                        request: Box::new(request),
                        responder,
                    },
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
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

            let mut actor = ConnectionActor::new(connection_id, capabilities, journal);
            let mut shutdown = shutdown;
            let mut in_flight = InFlight::new();
            loop {
                // What the select produces is decided here and acted on below,
                // because acting inside the select would still hold borrows on
                // everything the other branches are watching.
                let step = tokio::select! {
                    biased;
                    _ = &mut shutdown => return Ok(SupervisorExit::Disconnected),
                    () = connection.incoming_closed() => return Ok(SupervisorExit::Eof),
                    // Updates come first, and specifically before the answers
                    // in flight. The SDK hands us an update before it routes
                    // the response that follows it on the wire, so an update
                    // waiting here is one the agent sent *before* whatever is
                    // now settling. Taking the settlement first would file a
                    // turn's last words after the record of it ending.
                    inbound = updates.recv() => match inbound {
                        Some(inbound) => Step::Accept(Accepted::Inbound(Box::new(inbound))),
                        None => return Ok(SupervisorExit::Disconnected),
                    },
                    // An empty `FuturesUnordered` yields `None`, which fails
                    // this pattern and disables the branch for that pass.
                    Some(pending) = in_flight.next() => Step::Accept(Accepted::Settled(Box::new(pending))),
                    command = commands.recv() => match command {
                        Some(command) => Step::Command(command),
                        // The manager holds the only sender, so this is the
                        // manager itself going away, not a caller hanging up.
                        None => return Ok(SupervisorExit::Disconnected),
                    },
                };

                match step {
                    Step::Command(command) => actor.handle(command, &connection, &in_flight),
                    Step::Accept(accepted) => {
                        if actor.accept(accepted) {
                            publish_attachments(
                                &actor_connections,
                                connection_id,
                                actor.attachments(),
                            );
                        }
                    }
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
