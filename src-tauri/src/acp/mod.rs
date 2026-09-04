use std::path::{Path, PathBuf};

use agent_client_protocol::schema::{ProtocolVersion, v1};
use serde::{Deserialize, Serialize};

const EGO_PAUSE_METHOD: &str = "_ego/pause";
const EGO_RESUME_METHOD: &str = "_ego/resume";
const EGO_COMPACT_METHOD: &str = "_ego/compact";

mod manager;

pub use manager::AcpClientManager;

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
    let config_options = v1::SessionConfigOptionsCapabilities::new()
        .boolean(v1::BooleanConfigOptionCapabilities::new());
    let session = v1::ClientSessionCapabilities::new().config_options(config_options);
    let elicitation =
        v1::ElicitationCapabilities::new().form(v1::ElicitationFormCapabilities::new());
    let client_capabilities = v1::ClientCapabilities::new()
        .fs(v1::FileSystemCapabilities::new())
        .terminal(false)
        .session(session)
        .elicitation(elicitation);

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpConnectRequest {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpConnectionSnapshot {
    pub connection_id: AcpConnectionId,
    pub generation: u64,
    pub state: AcpConnectionState,
    pub agent_info: Option<v1::Implementation>,
    pub capabilities: Option<AcpCapabilitySnapshot>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AcpConnectionSettlementReason {
    Disconnected,
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
    pub ego_pause_version: Option<u32>,
    pub ego_resume_version: Option<u32>,
    pub ego_compact_version: Option<u32>,
    pause_availability: ExtensionAvailability,
    resume_availability: ExtensionAvailability,
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
            AcpOperation::McpStdio => None,
            AcpOperation::McpHttp => advertised(self.mcp_http),
            AcpOperation::McpSse => advertised(self.mcp_sse),
            AcpOperation::ClientFormElicitation => included(self.client_form_elicitation),
            AcpOperation::ClientBooleanConfig => included(self.client_boolean_config),
            AcpOperation::Pause => extension_reason(self.pause_availability),
            AcpOperation::ResumeTurn => extension_reason(self.resume_availability),
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
    let pause = extension_availability(response, "pause", EGO_PAUSE_METHOD);
    let resume = extension_availability(response, "resume", EGO_RESUME_METHOD);
    let compact = extension_availability(response, "compact", EGO_COMPACT_METHOD);

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
        mcp_stdio: true,
        mcp_http: mcp.http,
        mcp_sse: mcp.sse,
        client_form_elicitation: true,
        client_boolean_config: true,
        ego_pause_version: exact_extension_version(pause),
        ego_resume_version: exact_extension_version(resume),
        ego_compact_version: exact_extension_version(compact),
        pause_availability: pause,
        resume_availability: resume,
        compact_availability: compact,
    })
}

fn extension_availability(
    response: &v1::InitializeResponse,
    name: &str,
    expected_method: &str,
) -> ExtensionAvailability {
    let Some(extension) = response
        .meta
        .as_ref()
        .and_then(|meta| meta.get("ego"))
        .and_then(serde_json::Value::as_object)
        .and_then(|ego| ego.get(name))
    else {
        return ExtensionAvailability::Absent;
    };

    let version = extension.get("version").and_then(serde_json::Value::as_u64);
    let method = extension.get("method").and_then(serde_json::Value::as_str);
    if version == Some(1) && method == Some(expected_method) {
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
