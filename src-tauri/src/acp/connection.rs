//! The one place that speaks on a live ACP connection.
//!
//! The SDK's connection handle exists only inside the future passed to
//! `connect_with`, so nothing outside can send on it. Everything the manager
//! offers therefore arrives here as a [`Command`] carrying its own reply
//! channel, and one actor owns the whole of a connection's mutable semantics.
//!
//! That is not a workaround for the SDK's shape, it is the shape the semantics
//! want. Attachment state, in-flight operations and pending human interactions
//! all have to agree with each other and with what is on the wire; a lock
//! around each would let two commands interleave between the capability check
//! and the write. Here a command is decided and written before the next one is
//! read, so "advertised, then sent" is one step.
//!
//! Ego owns durable session identity, history, lineage, leases, run state and
//! cost. What lives here is attachment and correlation only — what *this*
//! connection is currently attached to, and which reply belongs to which
//! caller.

use std::collections::HashMap;
use std::sync::Arc;

use agent_client_protocol::schema::v1;
use agent_client_protocol::{Agent, ConnectionTo};
use tokio::sync::oneshot;

use super::{
    AcpAttachKind, AcpAttachmentSnapshot, AcpAttachmentState, AcpCapabilitySnapshot,
    AcpClientError, AcpConnectionId, AcpDetachKind, AcpOperation, AcpSessionAuthority,
};

/// One request from the manager, with the channel its answer goes back on.
///
/// A dropped receiver is not an error here: it means the caller went away, and
/// the effect either happened or did not on its own terms.
pub(super) enum Command {
    NewSession {
        authority: AcpSessionAuthority,
        reply: oneshot::Sender<Result<AcpAttachmentSnapshot, AcpClientError>>,
    },
    ListSessions {
        request: v1::ListSessionsRequest,
        reply: oneshot::Sender<Result<v1::ListSessionsResponse, AcpClientError>>,
    },
    Attach {
        kind: AcpAttachKind,
        session_id: v1::SessionId,
        authority: AcpSessionAuthority,
        reply: oneshot::Sender<Result<AcpAttachmentSnapshot, AcpClientError>>,
    },
    Detach {
        kind: AcpDetachKind,
        session_id: v1::SessionId,
        reply: oneshot::Sender<Result<(), AcpClientError>>,
    },
}

/// Everything one connection knows that is not on the wire.
pub(super) struct ConnectionActor {
    connection_id: AcpConnectionId,
    capabilities: Arc<AcpCapabilitySnapshot>,
    attachments: HashMap<v1::SessionId, AcpAttachmentSnapshot>,
}

impl ConnectionActor {
    pub(super) fn new(
        connection_id: AcpConnectionId,
        capabilities: Arc<AcpCapabilitySnapshot>,
    ) -> Self {
        Self {
            connection_id,
            capabilities,
            attachments: HashMap::new(),
        }
    }

    /// The attachments in a stable order, for the connection snapshot.
    ///
    /// A `HashMap` iterated raw would reorder the list between two reads of an
    /// unchanged connection, which a host would render as sessions jumping
    /// about. Sorting by id is enough to stop that; it also happens to be
    /// creation order against ego, whose session ids are UUIDv7, but the
    /// ordering is chosen for stability and does not depend on that.
    pub(super) fn attachments(&self) -> Vec<AcpAttachmentSnapshot> {
        let mut attachments: Vec<_> = self.attachments.values().cloned().collect();
        attachments.sort_by(|left, right| left.session_id.0.cmp(&right.session_id.0));
        attachments
    }

    /// Decide one command and write it, in that order.
    ///
    /// `publish` receives the new attachment list whenever it changed, and is
    /// always called *before* the caller is answered. A caller that reads the
    /// connection snapshot the instant its own call returns must not see a
    /// connection that does not yet know about the session it was just handed.
    pub(super) async fn handle(
        &mut self,
        command: Command,
        connection: &ConnectionTo<Agent>,
        publish: impl Fn(Vec<AcpAttachmentSnapshot>),
    ) {
        match command {
            Command::NewSession { authority, reply } => {
                let outcome = self.new_session(authority, connection).await;
                if outcome.is_ok() {
                    publish(self.attachments());
                }
                let _ = reply.send(outcome);
            }
            Command::ListSessions { request, reply } => {
                let outcome = self.list_sessions(request, connection).await;
                let _ = reply.send(outcome);
            }
            Command::Attach {
                kind,
                session_id,
                authority,
                reply,
            } => {
                let outcome = self.attach(kind, session_id, authority, connection).await;
                if outcome.is_ok() {
                    publish(self.attachments());
                }
                let _ = reply.send(outcome);
            }
            Command::Detach {
                kind,
                session_id,
                reply,
            } => {
                let outcome = self.detach(kind, session_id, connection).await;
                if outcome.is_ok() {
                    publish(self.attachments());
                }
                let _ = reply.send(outcome);
            }
        }
    }

    async fn new_session(
        &mut self,
        authority: AcpSessionAuthority,
        connection: &ConnectionTo<Agent>,
    ) -> Result<AcpAttachmentSnapshot, AcpClientError> {
        self.require_authority(&authority)?;

        let mut request = v1::NewSessionRequest::new(authority.cwd.clone());
        request
            .additional_directories
            .clone_from(&authority.additional_directories);
        request.mcp_servers.clone_from(&authority.mcp_servers);

        let response = self.send(request, connection, None).await?;
        Ok(self.record(response.session_id, response.config_options, authority))
    }

    /// Attach to a session ego already owns.
    ///
    /// The three kinds differ in the method they send and in whether the answer
    /// names a new id; everything else — the gate, the request body, and what
    /// the attachment ends up being — is shared, so it is written once.
    async fn attach(
        &mut self,
        kind: AcpAttachKind,
        session_id: v1::SessionId,
        authority: AcpSessionAuthority,
        connection: &ConnectionTo<Agent>,
    ) -> Result<AcpAttachmentSnapshot, AcpClientError> {
        let operation = kind.operation();
        self.require(operation)?;
        self.require_authority(&authority)?;
        let operation = Some(operation);
        let cwd = authority.cwd.clone();
        let roots = authority.additional_directories.clone();
        let servers = authority.mcp_servers.clone();

        let (attached, config_options) = match kind {
            AcpAttachKind::Load => {
                let mut request = v1::LoadSessionRequest::new(session_id.clone(), cwd);
                request.additional_directories = roots;
                request.mcp_servers = servers;
                let response = self.send(request, connection, operation).await?;
                (session_id, response.config_options)
            }
            AcpAttachKind::Fork => {
                let mut request = v1::ForkSessionRequest::new(session_id, cwd);
                request.additional_directories = roots;
                request.mcp_servers = servers;
                let response = self.send(request, connection, operation).await?;
                (response.session_id, response.config_options)
            }
            AcpAttachKind::Resume => {
                let mut request = v1::ResumeSessionRequest::new(session_id.clone(), cwd);
                request.additional_directories = roots;
                request.mcp_servers = servers;
                let response = self.send(request, connection, operation).await?;
                (session_id, response.config_options)
            }
        };
        Ok(self.record(attached, config_options, authority))
    }

    /// Let go of a session, and only then forget it here.
    ///
    /// The local attachment outlives a refused detach on purpose: an agent that
    /// answered "no" still has the session, and dropping it here would leave a
    /// live session no part of this client can reach.
    async fn detach(
        &mut self,
        kind: AcpDetachKind,
        session_id: v1::SessionId,
        connection: &ConnectionTo<Agent>,
    ) -> Result<(), AcpClientError> {
        let operation = kind.operation();
        self.require(operation)?;
        let operation = Some(operation);

        match kind {
            AcpDetachKind::Close => {
                let request = v1::CloseSessionRequest::new(session_id.clone());
                self.send(request, connection, operation).await?;
            }
            AcpDetachKind::Delete => {
                let request = v1::DeleteSessionRequest::new(session_id.clone());
                self.send(request, connection, operation).await?;
            }
        }
        self.attachments.remove(&session_id);
        Ok(())
    }

    /// Remember what this connection is now attached to.
    fn record(
        &mut self,
        session_id: v1::SessionId,
        config_options: Option<Vec<v1::SessionConfigOption>>,
        authority: AcpSessionAuthority,
    ) -> AcpAttachmentSnapshot {
        let attachment = AcpAttachmentSnapshot {
            session_id,
            state: AcpAttachmentState::Idle,
            cwd: authority.cwd,
            additional_directories: authority.additional_directories,
            config_options: config_options.unwrap_or_default(),
            usage: None,
            active_turn: None,
            pending_permission_ids: Vec::new(),
            pending_elicitation_ids: Vec::new(),
        };
        self.attachments
            .insert(attachment.session_id.clone(), attachment.clone());
        attachment
    }

    /// Refuse extra roots an agent never said it honours.
    ///
    /// The session request carrying them is baseline, which is the danger: an
    /// agent that does not know the field answers with a session anyway, and
    /// the caller is left believing it spans directories the agent will never
    /// touch.
    fn require_authority(&self, authority: &AcpSessionAuthority) -> Result<(), AcpClientError> {
        if authority.additional_directories.is_empty() {
            return Ok(());
        }
        self.require(AcpOperation::AdditionalDirectories)
    }

    async fn list_sessions(
        &self,
        request: v1::ListSessionsRequest,
        connection: &ConnectionTo<Agent>,
    ) -> Result<v1::ListSessionsResponse, AcpClientError> {
        self.require(AcpOperation::List)?;
        self.send(request, connection, Some(AcpOperation::List))
            .await
    }

    /// Refuse an operation this connection's peer never advertised.
    fn require(&self, operation: AcpOperation) -> Result<(), AcpClientError> {
        let availability = self.capabilities.availability(operation);
        if availability.available {
            return Ok(());
        }
        Err(AcpClientError::capability_unavailable(
            self.connection_id,
            operation,
            availability.reason,
        ))
    }

    /// Send one short request and wait for its single answer.
    ///
    /// Short is the whole justification for awaiting inline: no other command
    /// is read until this one lands, which is what makes "checked, then sent"
    /// indivisible. An operation that can take a human's time or a model's —
    /// `session/prompt`, a permission — must not come through here.
    async fn send<Request>(
        &self,
        request: Request,
        connection: &ConnectionTo<Agent>,
        operation: Option<AcpOperation>,
    ) -> Result<Request::Response, AcpClientError>
    where
        Request: agent_client_protocol::JsonRpcRequest,
    {
        connection
            .send_request(request)
            .block_task()
            .await
            .map_err(|error| {
                AcpClientError::agent_error(self.connection_id, operation, error.to_string())
            })
    }
}
