//! The IPC surface of the ACP client: one command per manager method.
//!
//! Thin on purpose. Every command reads its arguments, calls exactly one
//! manager method, and returns what that method returned. Nothing here decides
//! whether an operation is available, validates a permission option, or invents
//! a fallback — those are the client's judgements and they must be identical
//! whether a person is on the desktop app or on a phone over HTTP.
//!
//! The one thing this layer does own is process authority. `connect` and
//! `reconnect` read the ego executable from configuration and pass it down; no
//! argument can name a binary, so no caller over IPC or HTTP can choose what
//! this host launches.

use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::schema::v1;
#[cfg(feature = "desktop")]
use tauri::State;

use crate::AppState;
use crate::acp::{
    AcpAttachKind, AcpAttachmentSnapshot, AcpClientError, AcpConnectRequest, AcpConnectionId,
    AcpConnectionSettlement, AcpConnectionSnapshot, AcpDetachKind, AcpHostRequestId,
    AcpInteractionSettlement, AcpPendingInteraction, AcpReconnectRequest, AcpSessionAuthority,
    AcpStreamFrame, AcpTurnId, EgoAcpConfig, EgoCompactRequest, EgoCompactResponse, EgoHoldRequest,
    EgoHoldResponse,
};

/// The one executable this host may launch for ACP, as configured right now.
///
/// Read per call rather than remembered, so a person who corrects the setting
/// does not have to restart the app to have the correction take effect. An
/// empty setting is refused here rather than deep in a launch failure, because
/// "not configured" and "configured wrongly" are different things to be told.
pub(crate) fn ego_config(state: &AppState) -> Result<EgoAcpConfig, AcpClientError> {
    let executable = state.config.read().ego_executable.clone();
    if executable.trim().is_empty() {
        return Err(AcpClientError::invalid_input(
            "no ego executable is configured; set it before connecting over ACP",
        ));
    }
    Ok(EgoAcpConfig {
        executable: PathBuf::from(executable),
    })
}

pub(crate) async fn connect(
    state: &AppState,
    root: PathBuf,
) -> Result<AcpConnectionSnapshot, AcpClientError> {
    let config = ego_config(state)?;
    state.acp.connect(&config, AcpConnectRequest { root }).await
}

pub(crate) async fn reconnect(
    state: &AppState,
    connection_id: AcpConnectionId,
    root: PathBuf,
) -> Result<AcpConnectionSnapshot, AcpClientError> {
    let config = ego_config(state)?;
    state
        .acp
        .reconnect(
            &config,
            AcpReconnectRequest {
                connection_id,
                root,
            },
        )
        .await
}

/// Read one connection's events from `after_sequence` onwards, as frames.
///
/// The frames are what both transports carry: the desktop Channel and the
/// browser WebSocket send the same JSON, so a host written against one is
/// written against the other. A gap is a frame rather than a dropped
/// connection, because the subscriber has to learn that it missed something.
pub(crate) fn stream(
    state: &AppState,
    connection_id: AcpConnectionId,
    after_sequence: u64,
    mut send: impl FnMut(AcpStreamFrame) -> bool + Send + 'static,
) -> Result<(), AcpClientError> {
    let mut events = state.acp.subscribe(connection_id, after_sequence)?;
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            let frame = match event {
                Ok(envelope) => AcpStreamFrame::Event(Box::new(envelope)),
                Err(gap) => AcpStreamFrame::Gap(gap),
            };
            let fatal = matches!(frame, AcpStreamFrame::Gap(_));
            if !send(frame) || fatal {
                return;
            }
        }
        send(AcpStreamFrame::End);
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Tauri commands. One per manager method, in the order the plan lists them.
// ---------------------------------------------------------------------------

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_connect(
    state: State<'_, Arc<AppState>>,
    root: PathBuf,
) -> Result<AcpConnectionSnapshot, AcpClientError> {
    connect(&state, root).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_reconnect(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    root: PathBuf,
) -> Result<AcpConnectionSnapshot, AcpClientError> {
    reconnect(&state, connection_id, root).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_disconnect(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
) -> Result<AcpConnectionSettlement, AcpClientError> {
    state.acp.disconnect(connection_id).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_kill(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
) -> Result<AcpConnectionSettlement, AcpClientError> {
    state.acp.kill(connection_id).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn acp_connection_snapshot(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
) -> Result<AcpConnectionSnapshot, AcpClientError> {
    state.acp.snapshot(connection_id)
}

/// The dedicated-Channel exception, matched by a dedicated WebSocket.
///
/// Not an `AppEvent`: a turn's chunks are high-frequency and belong to one
/// connection, and putting them on the global broadcast would either drown the
/// 256-entry bus or make every other subscriber pay to filter them out.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn acp_subscribe(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    after_sequence: u64,
    channel: tauri::ipc::Channel<AcpStreamFrame>,
) -> Result<(), AcpClientError> {
    stream(&state, connection_id, after_sequence, move |frame| {
        channel.send(frame).is_ok()
    })
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_new(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    authority: AcpSessionAuthority,
) -> Result<AcpAttachmentSnapshot, AcpClientError> {
    state.acp.new_session(connection_id, authority).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_list(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    cwd: Option<PathBuf>,
    cursor: Option<String>,
) -> Result<v1::ListSessionsResponse, AcpClientError> {
    let mut request = v1::ListSessionsRequest::new();
    request.cwd = cwd;
    request.cursor = cursor;
    state.acp.list_sessions(connection_id, request).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_load(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    authority: AcpSessionAuthority,
) -> Result<AcpAttachmentSnapshot, AcpClientError> {
    state
        .acp
        .attach(connection_id, AcpAttachKind::Load, session_id, authority)
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_resume(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    authority: AcpSessionAuthority,
) -> Result<AcpAttachmentSnapshot, AcpClientError> {
    state
        .acp
        .attach(connection_id, AcpAttachKind::Resume, session_id, authority)
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_fork(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    authority: AcpSessionAuthority,
) -> Result<AcpAttachmentSnapshot, AcpClientError> {
    state
        .acp
        .attach(connection_id, AcpAttachKind::Fork, session_id, authority)
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_delete(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
) -> Result<(), AcpClientError> {
    state
        .acp
        .detach(connection_id, AcpDetachKind::Delete, session_id)
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_close(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
) -> Result<(), AcpClientError> {
    state
        .acp
        .detach(connection_id, AcpDetachKind::Close, session_id)
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_prompt(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    prompt: Vec<v1::ContentBlock>,
) -> Result<AcpTurnId, AcpClientError> {
    state.acp.prompt(connection_id, session_id, prompt).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_cancel(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
) -> Result<(), AcpClientError> {
    state.acp.cancel(connection_id, session_id).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_set_config_option(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    config_id: v1::SessionConfigId,
    value: v1::SessionConfigOptionValue,
) -> Result<Vec<v1::SessionConfigOption>, AcpClientError> {
    state
        .acp
        .set_config_option(
            connection_id,
            v1::SetSessionConfigOptionRequest::new(session_id, config_id, value),
        )
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_turn_pause(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    request_id: uuid::Uuid,
) -> Result<EgoHoldResponse, AcpClientError> {
    state
        .acp
        .pause_turn(
            connection_id,
            EgoHoldRequest {
                session_id,
                request_id,
            },
        )
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_turn_resume(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    request_id: uuid::Uuid,
) -> Result<EgoHoldResponse, AcpClientError> {
    state
        .acp
        .resume_turn(
            connection_id,
            EgoHoldRequest {
                session_id,
                request_id,
            },
        )
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_compact(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    request_id: uuid::Uuid,
) -> Result<EgoCompactResponse, AcpClientError> {
    state
        .acp
        .compact(
            connection_id,
            EgoCompactRequest {
                session_id,
                request_id,
            },
        )
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_pending_interactions(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
) -> Result<Vec<AcpPendingInteraction>, AcpClientError> {
    state.acp.pending_interactions(connection_id).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_respond_permission(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    request_id: AcpHostRequestId,
    outcome: v1::RequestPermissionOutcome,
) -> Result<AcpInteractionSettlement, AcpClientError> {
    state
        .acp
        .respond_permission(connection_id, request_id, outcome)
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_respond_elicitation(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    request_id: AcpHostRequestId,
    action: v1::ElicitationAction,
) -> Result<AcpInteractionSettlement, AcpClientError> {
    state
        .acp
        .respond_elicitation(connection_id, request_id, action)
        .await
}
