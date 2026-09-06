use std::path::{Path, PathBuf};

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
mod manager;

pub use manager::AcpClientManager;

/// What a session is allowed to reach, resupplied by the caller every time.
///
/// It is never restored from a stored snapshot: an authority that outlived the
/// window in which it was granted is a wider authority than anyone gave, and
/// reconnect is exactly when that would happen unnoticed.
#[derive(Debug, Clone, PartialEq, Eq)]
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
        .terminal(false);

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

#[derive(Debug, Clone, PartialEq)]
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
    /// The connection has settled. A new one is the only way forward.
    TransportClosed,
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
}

impl std::fmt::Display for AcpConnectionId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExtensionAvailability {
    Absent,
    Available,
    Mismatched,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
        client_form_elicitation: false,
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
