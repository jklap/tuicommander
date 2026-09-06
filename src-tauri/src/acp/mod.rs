use std::path::{Path, PathBuf};

use agent_client_protocol::JsonRpcResponse;
use agent_client_protocol::schema::{ProtocolVersion, v1};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const EGO_PAUSE_METHOD: &str = "_ego/pause";
const EGO_RESUME_METHOD: &str = "_ego/resume";
const EGO_COMPACT_METHOD: &str = "_ego/compact";

/// The extension metadata ego attaches to its advertised capabilities.
///
/// It hangs off `agentCapabilities`, not off the initialize response, because
/// it *is* a capability. Pause and resume arrive together under `hold` with one
/// version covering both method names — they are two halves of one extension,
/// and separate versions could disagree in a way no agent can actually be in.
const EGO_EXTENSIONS: &str = "ego";
const EGO_HOLD_EXTENSION: &str = "hold";
const EGO_COMPACT_EXTENSION: &str = "compact";

mod connection;
mod ego_ext;
mod events;
mod manager;

pub use events::{AcpEventJournal, AcpEventStream};
pub use manager::AcpClientManager;

/// The three ways a connection attaches to a session ego already owns.
///
/// They are one operation with three names because the request is the same —
/// a durable id plus the authority the session is to run under — and only the
/// method and what comes back differ. Fork is the one that answers with an id
/// the caller did not name, because a fork is a second session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AcpAttachKind {
    /// Attach and replay the history, so the host can render what happened.
    Load,
    /// Attach a copy that shares the original's history and diverges from it.
    Fork,
    /// Attach without replay, for a host that already has the history.
    Resume,
}

/// The two ways a connection lets a session go.
///
/// Both end the attachment; only one ends the session. Which of the two ego
/// was asked for is not a detail the client may blur, so they never collapse
/// into a single "forget it".
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AcpDetachKind {
    /// Stop serving the session here. It stays in `session/list`.
    Close,
    /// Remove the session from ego for good.
    Delete,
}

impl AcpAttachKind {
    /// The capability that has to be advertised before this is sent.
    #[must_use]
    pub fn operation(self) -> AcpOperation {
        match self {
            Self::Load => AcpOperation::Load,
            Self::Fork => AcpOperation::Fork,
            Self::Resume => AcpOperation::Resume,
        }
    }
}

impl AcpDetachKind {
    /// The capability that has to be advertised before this is sent.
    #[must_use]
    pub fn operation(self) -> AcpOperation {
        match self {
            Self::Close => AcpOperation::Close,
            Self::Delete => AcpOperation::Delete,
        }
    }
}

/// What a session is allowed to reach, resupplied by the caller every time.
///
/// It is never restored from a stored snapshot: an authority that outlived the
/// window in which it was granted is a wider authority than anyone gave, and
/// reconnect is exactly when that would happen unnoticed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpSessionAuthority {
    pub cwd: PathBuf,
    pub additional_directories: Vec<PathBuf>,
    pub mcp_servers: Vec<v1::McpServer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgoAcpConfig {
    pub executable: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
}

pub fn launch_spec(config: &EgoAcpConfig, root: &Path) -> Result<LaunchSpec, AcpClientError> {
    if !config.executable.is_absolute() {
        return Err(AcpClientError::invalid_input(
            "ego executable must be an absolute path",
        ));
    }
    if !root.is_absolute() {
        return Err(AcpClientError::invalid_input(
            "ACP root must be an absolute path",
        ));
    }
    let root = root
        .to_str()
        .ok_or_else(|| AcpClientError::invalid_input("ACP root must be valid UTF-8"))?;

    Ok(LaunchSpec {
        program: config.executable.clone(),
        args: vec!["acp".to_string(), "-C".to_string(), root.to_string()],
    })
}

pub fn build_initialize_request() -> v1::InitializeRequest {
    let client_capabilities = v1::ClientCapabilities::new()
        .fs(v1::FileSystemCapabilities::new())
        .terminal(false)
        // Form elicitation is advertised because this client seats an
        // `elicitation/create` and lets a person answer it. Ego treats the
        // absence of this as "no one to ask" and settles its questions
        // `Unavailable` without ever sending one, so leaving it off would make
        // the seat unreachable rather than merely unused.
        .elicitation(
            v1::ElicitationCapabilities::new().form(v1::ElicitationFormCapabilities::new()),
        );

    v1::InitializeRequest::new(ProtocolVersion::V1).client_capabilities(client_capabilities)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AcpOperation {
    Load,
    List,
    Resume,
    Fork,
    Delete,
    Close,
    AdditionalDirectories,
    PromptImage,
    PromptAudio,
    PromptEmbeddedContext,
    McpStdio,
    McpHttp,
    McpSse,
    ClientFormElicitation,
    ClientBooleanConfig,
    Pause,
    ResumeTurn,
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpUnavailableReason {
    NotAdvertised,
    WrongExtensionVersion,
    NotOfferedBySession,
    ExcludedByContract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcpAvailability {
    pub operation: AcpOperation,
    pub available: bool,
    pub reason: Option<AcpUnavailableReason>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct AcpConnectionId(uuid::Uuid);

impl AcpConnectionId {
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }
}

impl Default for AcpConnectionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct AcpTurnId(uuid::Uuid);

impl AcpTurnId {
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }
}

impl Default for AcpTurnId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct AcpHostRequestId(uuid::Uuid);

impl AcpHostRequestId {
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }
}

impl Default for AcpHostRequestId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct AcpOperationId(uuid::Uuid);

impl AcpOperationId {
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }
}

impl Default for AcpOperationId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpConnectRequest {
    pub root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpReconnectRequest {
    pub connection_id: AcpConnectionId,
    pub root: PathBuf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpConnectionState {
    Starting,
    Initializing,
    Ready,
    Closing,
    Closed,
    Failed,
    Killed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpAttachmentState {
    Attaching,
    Idle,
    Prompting,
    Cancelling,
    PausePending,
    Paused,
    Closing,
    Closed,
    Detached,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpTurnState {
    Running,
    Cancelling,
    Settled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AcpUsageSnapshot {
    pub context: v1::UsageUpdate,
    pub end_turn: Option<v1::Usage>,
}

/// One frame on a subscriber's stream, whichever transport carries it.
///
/// A gap is a frame rather than a closed stream, and it is the last one: the
/// events it stands for cannot be reconstructed from anything this client
/// holds, so a subscriber that kept reading would render a turn with a hole in
/// the middle and no way to know. The recovery is a fresh subscription and an
/// ego `session/load`, which replays from the only place that actually knows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[non_exhaustive]
pub enum AcpStreamFrame {
    /// One journal entry, byte for byte as every other subscriber saw it.
    Event(Box<AcpEventEnvelope>),
    /// The subscriber fell behind and the missing events are gone.
    Gap(AcpClientError),
    /// The connection can produce nothing further.
    End,
}

/// One thing that happened on a connection, in the order it happened.
///
/// Every event carries the generation it belongs to. A late event from a
/// connection that has since been replaced is not a fresher view of the same
/// thing; it is news about a process that is already gone, and the generation
/// is what lets a reader say so rather than apply it to the wrong connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AcpEventEnvelope {
    pub connection_id: AcpConnectionId,
    pub generation: u64,
    pub sequence: u64,
    pub session_id: Option<v1::SessionId>,
    pub turn_id: Option<AcpTurnId>,
    pub event: AcpClientEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[non_exhaustive]
pub enum AcpClientEvent {
    ConnectionState(AcpConnectionState),
    AttachmentState(AcpAttachmentState),
    TurnStarted,
    /// Ego's own update, forwarded whole rather than reduced.
    ///
    /// The client has no business deciding which parts of what the agent said
    /// a host is allowed to render.
    ///
    /// Boxed because it dwarfs every other variant, and a journal retains a
    /// thousand of these per connection: unboxed, a bare `TurnStarted` would
    /// cost as much to keep as the update it followed.
    SessionUpdate(Box<v1::SessionUpdate>),
    TurnSettled {
        stop_reason: v1::StopReason,
        usage: Option<v1::Usage>,
    },
    /// The agent is waiting on a person, and this is what it asked.
    ///
    /// It travels on the stream rather than being handed to whoever called
    /// last, because the caller that started the turn is not necessarily the
    /// one at the keyboard, and may not be listening at all by now.
    PermissionRequested {
        request_id: AcpHostRequestId,
        request: Box<v1::RequestPermissionRequest>,
    },
    PermissionSettled {
        request_id: AcpHostRequestId,
        outcome: v1::RequestPermissionOutcome,
    },
    ElicitationRequested {
        request_id: AcpHostRequestId,
        request: Box<v1::CreateElicitationRequest>,
    },
    ElicitationSettled {
        request_id: AcpHostRequestId,
        action: v1::ElicitationAction,
    },
}

/// Why a host might want to come and look, and where.
///
/// Deliberately not the event itself. Every chunk of a turn is an event; only
/// these four are worth waking a client that is not currently reading the
/// stream, and only they are cheap enough to put on a broadcast that every
/// other subscriber in the app shares.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpNoticeKind {
    /// The connection finished initializing and can be used.
    Ready,
    /// Something that was in flight has finished.
    ///
    /// A turn settling and a connection settling are both this: the host was
    /// waiting on one of them, and the two are told apart by whether the
    /// notice names a session, not by needing a fifth kind.
    Settled,
    /// The agent is waiting on a person.
    InteractionPending,
    /// A pending question was answered — possibly by somebody else's client.
    InteractionSettled,
}

/// A low-frequency wake signal about one connection.
///
/// It says where to look and nothing more: the ordered payload stays on the
/// per-connection stream, and the current picture stays in the snapshot and
/// the interactions list. A host that reacts to a notice by fetching one of
/// those two reads the same truth as a host that never missed a frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpNotice {
    pub connection_id: AcpConnectionId,
    pub generation: u64,
    pub session_id: Option<v1::SessionId>,
    pub request_id: Option<AcpHostRequestId>,
    /// The sequence of the event this notice was derived from, so a host can
    /// resume its stream from exactly here rather than from the beginning.
    pub sequence: u64,
    pub kind: AcpNoticeKind,
}

impl AcpNotice {
    /// The notice one stamped event deserves, if it deserves one at all.
    ///
    /// Derived at the journal rather than at each producer: every event on a
    /// connection is appended there exactly once, so a notice cannot be
    /// forgotten by a new call site, and it cannot be sent twice.
    #[must_use]
    pub fn from_envelope(envelope: &AcpEventEnvelope) -> Option<Self> {
        let (kind, request_id) = match &envelope.event {
            AcpClientEvent::ConnectionState(AcpConnectionState::Ready) => {
                (AcpNoticeKind::Ready, None)
            }
            AcpClientEvent::ConnectionState(
                AcpConnectionState::Closed
                | AcpConnectionState::Failed
                | AcpConnectionState::Killed,
            )
            | AcpClientEvent::TurnSettled { .. } => (AcpNoticeKind::Settled, None),
            AcpClientEvent::PermissionRequested { request_id, .. }
            | AcpClientEvent::ElicitationRequested { request_id, .. } => {
                (AcpNoticeKind::InteractionPending, Some(*request_id))
            }
            AcpClientEvent::PermissionSettled { request_id, .. }
            | AcpClientEvent::ElicitationSettled { request_id, .. } => {
                (AcpNoticeKind::InteractionSettled, Some(*request_id))
            }
            _ => return None,
        };
        Some(Self {
            connection_id: envelope.connection_id,
            generation: envelope.generation,
            session_id: envelope.session_id.clone(),
            request_id,
            sequence: envelope.sequence,
            kind,
        })
    }
}

/// One request the agent is waiting on an answer to.
///
/// Carried in snapshots as well as on the stream, so a frontend that was not
/// running when the agent asked still finds the question when it comes back.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[non_exhaustive]
pub enum AcpPendingInteraction {
    Permission {
        request_id: AcpHostRequestId,
        session_id: v1::SessionId,
        request: Box<v1::RequestPermissionRequest>,
    },
    Elicitation {
        request_id: AcpHostRequestId,
        session_id: v1::SessionId,
        request: Box<v1::CreateElicitationRequest>,
    },
}

impl AcpPendingInteraction {
    #[must_use]
    pub fn request_id(&self) -> AcpHostRequestId {
        match self {
            Self::Permission { request_id, .. } | Self::Elicitation { request_id, .. } => {
                *request_id
            }
        }
    }

    #[must_use]
    pub fn session_id(&self) -> &v1::SessionId {
        match self {
            Self::Permission { session_id, .. } | Self::Elicitation { session_id, .. } => {
                session_id
            }
        }
    }
}

/// The receipt for an interaction this client answered.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpInteractionSettlement {
    pub request_id: AcpHostRequestId,
}

/// What a `_ego/pause` or `_ego/resume` found or established.
///
/// The two are not distinguished, because ego does not distinguish them and
/// the shared vocabulary is the better contract: a session told `paused` may
/// assume nothing further runs under it, and a session told `pending` may not.
/// That is the distinction the hold boundary exists to make, and it is the
/// same distinction whichever of the two methods asked.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpHoldState {
    /// Nothing is holding this session.
    Running,
    /// A hold is recorded and the turn has not reached a boundary yet.
    Pending,
    /// The hold is durable: nothing more runs until it is released.
    Paused,
}

/// One `_ego/pause` or `_ego/resume`, named by the caller.
///
/// The request id is the caller's, generated once and reused only to retrieve
/// the same idempotent result. Ego rejects a nil UUID, so this client rejects
/// one before the write rather than letting a round trip say it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EgoHoldRequest {
    pub session_id: v1::SessionId,
    pub request_id: uuid::Uuid,
}

/// Ego's answer to a hold request, in ego's own field names.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct EgoHoldResponse {
    pub v: u32,
    pub session_id: v1::SessionId,
    pub request_id: uuid::Uuid,
    pub state: AcpHoldState,
}

/// One `_ego/compact`, named by the caller.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EgoCompactRequest {
    pub session_id: v1::SessionId,
    pub request_id: uuid::Uuid,
}

/// Whether the successor a compaction produced is durably on record.
///
/// The uncertain case is the reason this is three values and not a boolean. A
/// target that may or may not have been published cannot be retried as a new
/// compaction — that would risk a second successor for one source — and it
/// cannot be treated as absent either.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpTargetPublication {
    NotPublished { diagnostic: String },
    PublishedDurably { diagnostic: Option<String> },
    PublishedDurabilityUncertain { diagnostic: String },
}

impl AcpTargetPublication {
    /// Whether asking for this compaction again is safe.
    ///
    /// Only a target that was definitely not published may be retried. Both
    /// other answers mean a successor may exist, and asking again could make
    /// a second one.
    #[must_use]
    pub fn retry_safe(&self) -> bool {
        matches!(self, Self::NotPublished { .. })
    }
}

/// Ego's answer to a compaction, in ego's own field names.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct EgoCompactResponse {
    pub v: u32,
    pub source_session_id: v1::SessionId,
    pub source_seq: u64,
    pub target_session_id: v1::SessionId,
    pub publication: AcpTargetPublication,
    pub successor_start_request_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpTurnSnapshot {
    pub turn_id: AcpTurnId,
    pub state: AcpTurnState,
    pub stop_reason: Option<v1::StopReason>,
    pub usage: Option<v1::Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AcpAttachmentSnapshot {
    pub session_id: v1::SessionId,
    pub state: AcpAttachmentState,
    pub cwd: PathBuf,
    pub additional_directories: Vec<PathBuf>,
    pub config_options: Vec<v1::SessionConfigOption>,
    pub usage: Option<AcpUsageSnapshot>,
    pub active_turn: Option<AcpTurnSnapshot>,
    pub pending_permission_ids: Vec<AcpHostRequestId>,
    pub pending_elicitation_ids: Vec<AcpHostRequestId>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AcpConnectionSnapshot {
    pub connection_id: AcpConnectionId,
    pub generation: u64,
    pub state: AcpConnectionState,
    pub agent_info: Option<v1::Implementation>,
    pub capabilities: Option<AcpCapabilitySnapshot>,
    pub attachments: Vec<AcpAttachmentSnapshot>,
    pub earliest_sequence: u64,
    pub latest_sequence: u64,
    pub settlement: Option<AcpConnectionSettlement>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpConnectionSettlementReason {
    Disconnected,
    Eof,
    TransportError,
    WriteError,
    InitializationFailed,
    ProtocolViolation,
    Killed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpConnectionSettlement {
    pub connection_id: AcpConnectionId,
    pub generation: u64,
    pub reason: AcpConnectionSettlementReason,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpClientErrorCode {
    InvalidInput,
    InitializationFailed,
    NotFound,
    UnsupportedProtocol,
    /// The peer never advertised this operation, so nothing was sent.
    CapabilityUnavailable,
    /// The agent answered, and its answer was a refusal.
    AgentError,
    /// The agent denied a method it had advertised.
    ///
    /// Not a refusal but a contradiction of the snapshot every later decision
    /// on this connection is read from, so it settles the connection rather
    /// than being handed back as one operation's bad luck.
    ProtocolViolation,
    /// The connection has settled. A new one is the only way forward.
    TransportClosed,
    /// The events a subscriber asked for are no longer held.
    ///
    /// Said out loud rather than papered over: the missing chunks cannot be
    /// reconstructed from anything this client holds, and a stream that
    /// silently resumed past a hole would render as a turn that skipped part
    /// of what the model said. The recovery is a fresh connection and
    /// `session/load`, which replays from the one place that actually knows.
    StreamGap,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpClientError {
    pub code: AcpClientErrorCode,
    pub message: String,
    pub connection_id: Option<AcpConnectionId>,
    pub session_id: Option<v1::SessionId>,
    pub operation: Option<AcpOperation>,
    pub retryable: bool,
}

impl AcpClientError {
    pub(super) fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(AcpClientErrorCode::InvalidInput, message)
    }

    pub(super) fn initialization_failed(
        connection_id: AcpConnectionId,
        message: impl Into<String>,
    ) -> Self {
        Self::new(AcpClientErrorCode::InitializationFailed, message)
            .with_connection_id(connection_id)
    }

    pub(super) fn not_found(connection_id: AcpConnectionId) -> Self {
        Self::new(
            AcpClientErrorCode::NotFound,
            format!("ACP connection {connection_id} was not found"),
        )
        .with_connection_id(connection_id)
    }

    /// Refused before the wire, naming the operation and why it is not there.
    ///
    /// Never retryable: the answer comes from an immutable snapshot taken at
    /// `initialize`, so the identical request on this connection will refuse
    /// identically. A host that wants it must reconnect to an agent that has
    /// it, which is a different action, not a retry.
    pub(super) fn capability_unavailable(
        connection_id: AcpConnectionId,
        operation: AcpOperation,
        reason: Option<AcpUnavailableReason>,
    ) -> Self {
        Self::new(
            AcpClientErrorCode::CapabilityUnavailable,
            match reason {
                Some(AcpUnavailableReason::WrongExtensionVersion) => format!(
                    "the agent advertises {operation:?} at a version this client does not speak"
                ),
                Some(AcpUnavailableReason::NotOfferedBySession) => {
                    format!("this session does not offer {operation:?}")
                }
                Some(AcpUnavailableReason::ExcludedByContract) => {
                    format!("{operation:?} is not part of this client's contract")
                }
                Some(AcpUnavailableReason::NotAdvertised) | None => {
                    format!("the agent did not advertise {operation:?}")
                }
            },
        )
        .with_connection_id(connection_id)
        .with_operation(operation)
    }

    /// The agent answered and its answer was a refusal.
    ///
    /// Not retryable on its own account: the agent decided, and asking again
    /// with the same bytes invites the same decision. Whether a *different*
    /// request would work is the caller's judgement, not this one's.
    pub(super) fn agent_error(
        connection_id: AcpConnectionId,
        operation: Option<AcpOperation>,
        message: impl Into<String>,
    ) -> Self {
        let mut error =
            Self::new(AcpClientErrorCode::AgentError, message).with_connection_id(connection_id);
        error.operation = operation;
        error
    }

    /// The agent denied a method it advertised.
    ///
    /// Never retryable, for the same reason `capability_unavailable` is not:
    /// the request was sent because a snapshot said it could be, and that
    /// snapshot does not change. Repeating it invites the same contradiction
    /// from an agent this client can no longer believe about anything else
    /// either — which is why the connection settles behind this error rather
    /// than staying up for the next caller to rediscover.
    pub(super) fn protocol_violation(
        connection_id: AcpConnectionId,
        operation: Option<AcpOperation>,
        message: impl Into<String>,
    ) -> Self {
        let mut error = Self::new(AcpClientErrorCode::ProtocolViolation, message)
            .with_connection_id(connection_id);
        error.operation = operation;
        error
    }

    /// The connection is gone. Retryable, because a fresh one may not be.
    pub(super) fn transport_closed(connection_id: AcpConnectionId) -> Self {
        let mut error = Self::new(
            AcpClientErrorCode::TransportClosed,
            format!("ACP connection {connection_id} has settled"),
        )
        .with_connection_id(connection_id);
        error.retryable = true;
        error
    }

    /// The caller named a session this connection is not attached to.
    ///
    /// Distinct from an unknown connection: the connection is fine, and it is
    /// the session that was never attached here or has since been let go.
    pub(super) fn not_attached(connection_id: AcpConnectionId, session_id: v1::SessionId) -> Self {
        Self::new(
            AcpClientErrorCode::NotFound,
            format!("ACP connection {connection_id} is not attached to session {session_id}"),
        )
        .with_connection_id(connection_id)
        .with_session_id(session_id)
    }

    /// A second prompt arrived while the first was still running.
    ///
    /// Retryable, because the answer changes on its own: the turn settles and
    /// the session takes prompts again.
    pub(super) fn turn_in_progress(
        connection_id: AcpConnectionId,
        session_id: v1::SessionId,
    ) -> Self {
        let mut error = Self::new(
            AcpClientErrorCode::InvalidInput,
            format!("ACP session {session_id} already has a turn in progress"),
        )
        .with_connection_id(connection_id)
        .with_session_id(session_id);
        error.retryable = true;
        error
    }

    /// A config option, or a value for one, that this session never offered.
    ///
    /// Refused here rather than forwarded because the offer is the whole
    /// contract: ego resolves an incoming value by matching it against the list
    /// it published, so a value it did not publish has no meaning to send.
    pub(super) fn not_offered(
        connection_id: AcpConnectionId,
        session_id: v1::SessionId,
        detail: impl std::fmt::Display,
    ) -> Self {
        Self::new(
            AcpClientErrorCode::CapabilityUnavailable,
            format!("ACP session {session_id} does not offer {detail}"),
        )
        .with_connection_id(connection_id)
        .with_session_id(session_id)
    }

    /// A cancel arrived for a session that has nothing running.
    pub(super) fn no_active_turn(
        connection_id: AcpConnectionId,
        session_id: v1::SessionId,
    ) -> Self {
        Self::new(
            AcpClientErrorCode::InvalidInput,
            format!("ACP session {session_id} has no turn to cancel"),
        )
        .with_connection_id(connection_id)
        .with_session_id(session_id)
    }

    /// The events asked for fell out of the journal before they were read.
    ///
    /// Not retryable: asking again for the same cursor gets the same answer,
    /// and the events are not coming back. The caller has to either accept the
    /// gap and resume from `earliest`, or reload the session from ego.
    pub(super) fn stream_gap(
        connection_id: AcpConnectionId,
        requested: u64,
        earliest: u64,
    ) -> Self {
        Self::new(
            AcpClientErrorCode::StreamGap,
            format!(
                "ACP connection {connection_id} no longer holds event {requested}; \
                 the earliest it still holds is {earliest}"
            ),
        )
        .with_connection_id(connection_id)
    }

    /// An answer arrived for a request nothing is waiting on.
    ///
    /// One code for two histories on purpose: the request was answered already,
    /// or it was never one this connection held. Both mean the same thing to
    /// the caller — the seat is not open — and the client does not keep settled
    /// seats around just so it could tell the two apart.
    pub(super) fn interaction_settled(
        connection_id: AcpConnectionId,
        request_id: AcpHostRequestId,
    ) -> Self {
        Self::new(
            AcpClientErrorCode::NotFound,
            format!("ACP connection {connection_id} is not waiting on request {request_id}"),
        )
        .with_connection_id(connection_id)
    }

    /// The answer named an option the agent never offered.
    ///
    /// Refused here rather than forwarded: the agent decides what a permission
    /// means, and it can only do that for the options it named. An id it does
    /// not know is not a decision it can read.
    pub(super) fn unoffered_option(
        connection_id: AcpConnectionId,
        session_id: v1::SessionId,
        option: &v1::PermissionOptionId,
    ) -> Self {
        Self::new(
            AcpClientErrorCode::InvalidInput,
            format!("the agent did not offer permission option {}", option.0),
        )
        .with_connection_id(connection_id)
        .with_session_id(session_id)
    }

    /// A live subscriber fell far enough behind that it missed events.
    pub(super) fn stream_lagged(connection_id: AcpConnectionId) -> Self {
        Self::new(
            AcpClientErrorCode::StreamGap,
            format!("ACP connection {connection_id} produced events faster than this subscriber read them"),
        )
        .with_connection_id(connection_id)
    }

    pub(super) fn unsupported_protocol(message: impl Into<String>) -> Self {
        Self::new(AcpClientErrorCode::UnsupportedProtocol, message)
    }

    fn new(code: AcpClientErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            connection_id: None,
            session_id: None,
            operation: None,
            retryable: false,
        }
    }

    pub(super) fn with_connection_id(mut self, connection_id: AcpConnectionId) -> Self {
        self.connection_id = Some(connection_id);
        self
    }

    fn with_operation(mut self, operation: AcpOperation) -> Self {
        self.operation = Some(operation);
        self
    }

    fn with_session_id(mut self, session_id: v1::SessionId) -> Self {
        self.session_id = Some(session_id);
        self
    }
}

impl std::fmt::Display for AcpConnectionId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::fmt::Display for AcpHostRequestId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ExtensionAvailability {
    Absent,
    Available,
    Mismatched,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpCapabilitySnapshot {
    pub protocol: ProtocolVersion,
    pub load: bool,
    pub list: bool,
    pub resume: bool,
    pub fork: bool,
    pub delete: bool,
    pub close: bool,
    pub additional_directories: bool,
    pub prompt_image: bool,
    pub prompt_audio: bool,
    pub prompt_embedded_context: bool,
    pub mcp_stdio: bool,
    pub mcp_http: bool,
    pub mcp_sse: bool,
    pub client_form_elicitation: bool,
    pub client_boolean_config: bool,
    /// One version for `_ego/pause` and `_ego/resume`, which arrive together.
    pub ego_hold_version: Option<u32>,
    pub ego_compact_version: Option<u32>,
    hold_availability: ExtensionAvailability,
    compact_availability: ExtensionAvailability,
}

impl AcpCapabilitySnapshot {
    #[must_use]
    pub fn availability(&self, operation: AcpOperation) -> AcpAvailability {
        let reason = match operation {
            AcpOperation::Load => advertised(self.load),
            AcpOperation::List => advertised(self.list),
            AcpOperation::Resume => advertised(self.resume),
            AcpOperation::Fork => advertised(self.fork),
            AcpOperation::Delete => advertised(self.delete),
            AcpOperation::Close => advertised(self.close),
            AcpOperation::AdditionalDirectories => advertised(self.additional_directories),
            AcpOperation::PromptImage => advertised(self.prompt_image),
            AcpOperation::PromptAudio => advertised(self.prompt_audio),
            AcpOperation::PromptEmbeddedContext => advertised(self.prompt_embedded_context),
            AcpOperation::McpStdio => included(self.mcp_stdio),
            AcpOperation::McpHttp => advertised(self.mcp_http),
            AcpOperation::McpSse => advertised(self.mcp_sse),
            AcpOperation::ClientFormElicitation => included(self.client_form_elicitation),
            AcpOperation::ClientBooleanConfig => included(self.client_boolean_config),
            AcpOperation::Pause | AcpOperation::ResumeTurn => {
                extension_reason(self.hold_availability)
            }
            AcpOperation::Compact => extension_reason(self.compact_availability),
        };

        AcpAvailability {
            operation,
            available: reason.is_none(),
            reason,
        }
    }
}

pub fn capability_snapshot(
    response: &v1::InitializeResponse,
) -> Result<AcpCapabilitySnapshot, AcpClientError> {
    if response.protocol_version != ProtocolVersion::V1 {
        return Err(AcpClientError::unsupported_protocol(format!(
            "unsupported ACP protocol version {}",
            response.protocol_version
        )));
    }

    let agent = &response.agent_capabilities;
    let session = &agent.session_capabilities;
    let prompt = &agent.prompt_capabilities;
    let mcp = &agent.mcp_capabilities;
    let hold = hold_availability(agent);
    let compact = compact_availability(agent);

    Ok(AcpCapabilitySnapshot {
        protocol: response.protocol_version,
        load: agent.load_session,
        list: session.list.is_some(),
        resume: session.resume.is_some(),
        fork: session.fork.is_some(),
        delete: session.delete.is_some(),
        close: session.close.is_some(),
        additional_directories: session.additional_directories.is_some(),
        prompt_image: prompt.image,
        prompt_audio: prompt.audio,
        prompt_embedded_context: prompt.embedded_context,
        mcp_stdio: false,
        mcp_http: mcp.http,
        mcp_sse: mcp.sse,
        // What this client can do, not what the agent said. It is here rather
        // than beside the request because a host asking "can a question be
        // answered on this connection?" reads one snapshot, and an answer that
        // needs both sides is still one answer.
        client_form_elicitation: true,
        client_boolean_config: false,
        ego_hold_version: exact_extension_version(hold),
        ego_compact_version: exact_extension_version(compact),
        hold_availability: hold,
        compact_availability: compact,
    })
}

/// The one `ego` object under the agent's advertised capabilities.
fn ego_extensions(agent: &v1::AgentCapabilities) -> Option<&serde_json::Map<String, Value>> {
    agent.meta.as_ref()?.get(EGO_EXTENSIONS)?.as_object()
}

/// Whether `_ego/pause` and `_ego/resume` may be sent on this connection.
///
/// Both method names are checked, not just the one the caller happens to want:
/// a `hold` that names a pause this client knows and a resume it does not is
/// not half-available, it is an agent speaking a version of the extension this
/// client was not written against.
fn hold_availability(agent: &v1::AgentCapabilities) -> ExtensionAvailability {
    let Some(hold) = ego_extensions(agent).and_then(|ego| ego.get(EGO_HOLD_EXTENSION)) else {
        return ExtensionAvailability::Absent;
    };
    let matches = hold.get("version").and_then(Value::as_u64) == Some(1)
        && hold.get("pause").and_then(Value::as_str) == Some(EGO_PAUSE_METHOD)
        && hold.get("resume").and_then(Value::as_str) == Some(EGO_RESUME_METHOD);
    if matches {
        ExtensionAvailability::Available
    } else {
        ExtensionAvailability::Mismatched
    }
}

fn compact_availability(agent: &v1::AgentCapabilities) -> ExtensionAvailability {
    let Some(compact) = ego_extensions(agent).and_then(|ego| ego.get(EGO_COMPACT_EXTENSION)) else {
        return ExtensionAvailability::Absent;
    };
    let matches = compact.get("version").and_then(Value::as_u64) == Some(1)
        && compact.get("method").and_then(Value::as_str) == Some(EGO_COMPACT_METHOD);
    if matches {
        ExtensionAvailability::Available
    } else {
        ExtensionAvailability::Mismatched
    }
}

fn exact_extension_version(availability: ExtensionAvailability) -> Option<u32> {
    (availability == ExtensionAvailability::Available).then_some(1)
}

fn extension_reason(availability: ExtensionAvailability) -> Option<AcpUnavailableReason> {
    match availability {
        ExtensionAvailability::Absent => Some(AcpUnavailableReason::NotAdvertised),
        ExtensionAvailability::Available => None,
        ExtensionAvailability::Mismatched => Some(AcpUnavailableReason::WrongExtensionVersion),
    }
}

fn advertised(available: bool) -> Option<AcpUnavailableReason> {
    (!available).then_some(AcpUnavailableReason::NotAdvertised)
}

fn included(available: bool) -> Option<AcpUnavailableReason> {
    (!available).then_some(AcpUnavailableReason::ExcludedByContract)
}
