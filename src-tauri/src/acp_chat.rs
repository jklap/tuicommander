//! Durable launch authority for a conversation with its own ego process.
use crate::{
    AppState,
    acp::{
        AcpAttachKind, AcpClientError, AcpConnectRequest, AcpConnectionSnapshot,
        AcpSessionAuthority, EgoAcpConfig,
    },
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatLaunch {
    pub executable: String,
    pub profile: String,
    pub workspace: PathBuf,
    pub peer_id: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ChatOpenRequest {
    pub session_id: Option<String>,
    pub executable: Option<String>,
    pub profile: Option<String>,
    pub workspace: Option<PathBuf>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatOpened {
    pub connection: AcpConnectionSnapshot,
    pub session_id: String,
    pub launch: ChatLaunch,
    pub replayed: bool,
}

impl ChatLaunch {
    fn ego_config(&self) -> EgoAcpConfig {
        EgoAcpConfig {
            executable: PathBuf::from(&self.executable),
            profile: self.profile.clone(),
        }
    }
}

async fn resolve(
    state: &AppState,
    request: &ChatOpenRequest,
) -> Result<ChatLaunch, AcpClientError> {
    if let Some(session) = &request.session_id {
        if request.executable.is_some() || request.profile.is_some() || request.workspace.is_some()
        {
            return Err(AcpClientError::invalid_input(
                "saved conversation launch options cannot be replaced",
            ));
        }
        return state
            .config
            .read()
            .ai_chat_launches
            .get(session)
            .cloned()
            .ok_or_else(|| {
                AcpClientError::invalid_input("no saved launch configuration for this conversation")
            });
    }
    let (executable, profile) = {
        let settings = state.config.read();
        (
            request
                .executable
                .clone()
                .unwrap_or_else(|| settings.ego_executable.clone()),
            request
                .profile
                .clone()
                .unwrap_or_else(|| settings.ego_profile.clone()),
        )
    };
    let workspace = match &request.workspace {
        Some(path) => {
            if !path.is_absolute() {
                return Err(AcpClientError::invalid_input(
                    "conversation workspace must be absolute",
                ));
            }
            tokio::fs::create_dir_all(path).await.map_err(|error| {
                AcpClientError::invalid_input(format!(
                    "cannot create conversation workspace: {error}"
                ))
            })?;
            tokio::fs::canonicalize(path).await.map_err(|error| {
                AcpClientError::invalid_input(format!(
                    "cannot open conversation workspace: {error}"
                ))
            })?
        }
        None => crate::acp_commands::workspace_root(state).await?,
    };
    let launch = ChatLaunch {
        executable,
        profile,
        workspace,
        peer_id: uuid::Uuid::new_v4().to_string(),
    };
    crate::acp::launch_spec(&launch.ego_config(), &launch.workspace)?;
    Ok(launch)
}

pub(crate) async fn open(
    state: &Arc<AppState>,
    request: ChatOpenRequest,
) -> Result<ChatOpened, AcpClientError> {
    let launch = resolve(state, &request).await?;
    let connection = state
        .acp
        .connect_with_peer(
            &launch.ego_config(),
            AcpConnectRequest {
                root: launch.workspace.clone(),
            },
            launch.peer_id.clone(),
        )
        .await?;
    let authority = AcpSessionAuthority {
        cwd: launch.workspace.clone(),
        additional_directories: vec![],
        mcp_servers: vec![],
    };
    let mut replayed = false;
    let session_id = if let Some(session) = request.session_id {
        if !connection
            .attachments
            .iter()
            .any(|attachment| attachment.session_id.to_string() == session)
        {
            replayed = true;
            if let Err(error) = state
                .acp
                .attach(
                    connection.connection_id,
                    AcpAttachKind::Load,
                    session.clone().into(),
                    authority,
                )
                .await
            {
                let _ = state.acp.disconnect(connection.connection_id).await;
                return Err(error);
            }
        }
        session
    } else {
        let attachment = match crate::acp_commands::session_new(
            state,
            connection.connection_id,
            authority,
        )
        .await
        {
            Ok(attachment) => attachment,
            Err(error) => {
                let _ = state.acp.disconnect(connection.connection_id).await;
                return Err(error);
            }
        };
        let session = attachment.session_id.to_string();
        if let Err(error) = persist(state, &session, &launch).await {
            let _ = state.acp.disconnect(connection.connection_id).await;
            return Err(error);
        }
        session
    };
    Ok(ChatOpened {
        connection: state.acp.snapshot(connection.connection_id)?,
        session_id,
        launch,
        replayed,
    })
}

async fn persist(
    state: &Arc<AppState>,
    session: &str,
    launch: &ChatLaunch,
) -> Result<(), AcpClientError> {
    let saved = session.to_owned();
    let saved_launch = launch.clone();
    let saved_state = Arc::clone(state);
    tokio::task::spawn_blocking(move || {
        crate::config::commit_config_change(&saved_state, |current| {
            let mut next = current.clone();
            next.ai_chat_launches
                .insert(saved.clone(), saved_launch.clone());
            Ok(next)
        })
    })
    .await
    .map_err(|error| {
        AcpClientError::invalid_input(format!("cannot save conversation launch: {error}"))
    })?
    .map(|_| ())
    .map_err(|error| {
        AcpClientError::invalid_input(format!("cannot save conversation launch: {error}"))
    })
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_chat_open(
    state: tauri::State<'_, Arc<AppState>>,
    request: ChatOpenRequest,
) -> Result<ChatOpened, AcpClientError> {
    open(&state, request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches: a conversation profile changes global defaults or another chat's ACP launch args.
    #[tokio::test]
    async fn conversation_override_reaches_launch_args_without_changing_defaults() {
        let dir = tempfile::TempDir::new_in(tuic_test_support::test_temp_root()).unwrap();
        let state = crate::state::tests_support::make_test_app_state();
        state.config.write().ego_executable = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        state.config.write().ego_profile = "daily".into();
        state.config.write().ai_chat_workspace = dir.path().to_string_lossy().into_owned();
        let defaults = state.config.read().clone();
        let custom_executable = dir.path().join("observer-ego");
        let custom_workspace = dir.path().join("observer-workspace");
        let custom = resolve(
            &state,
            &ChatOpenRequest {
                profile: Some("coordinator".into()),
                executable: Some(custom_executable.to_string_lossy().into_owned()),
                workspace: Some(custom_workspace),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let spec = crate::acp::launch_spec(&custom.ego_config(), &custom.workspace).unwrap();
        assert_eq!(spec.program, custom_executable);
        assert_ne!(custom.workspace, dir.path());
        assert_eq!(
            spec.args,
            vec![
                "acp",
                "-C",
                custom.workspace.to_str().unwrap(),
                "--profile",
                "coordinator"
            ]
        );
        let other = resolve(&state, &ChatOpenRequest::default()).await.unwrap();
        let other_spec = crate::acp::launch_spec(&other.ego_config(), &other.workspace).unwrap();
        assert_eq!(other_spec.args.last().unwrap(), "daily");
        assert_ne!(custom.peer_id, other.peer_id);
        assert_eq!(state.config.read().ego_profile, defaults.ego_profile);
        assert_eq!(state.config.read().ego_executable, defaults.ego_executable);
        assert_eq!(
            state.config.read().ai_chat_workspace,
            defaults.ai_chat_workspace
        );
    }

    /// Catches: reopening silently uses changed global defaults or loses the durable peer identity.
    #[tokio::test]
    async fn conversation_reopen_keeps_persisted_launch_options() {
        let dir = tempfile::TempDir::new_in(tuic_test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(dir.path().join("config"));
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let launch = ChatLaunch {
            executable: std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            profile: "coordinator".into(),
            workspace: dir.path().to_owned(),
            peer_id: uuid::Uuid::new_v4().to_string(),
        };
        persist(&state, "conversation", &launch).await.unwrap();
        inherit(&state, "conversation", "forked").await.unwrap();
        let child = state
            .config
            .read()
            .ai_chat_launches
            .get("forked")
            .unwrap()
            .clone();
        assert_eq!(child.profile, launch.profile);
        assert_eq!(child.executable, launch.executable);
        assert_eq!(child.workspace, launch.workspace);
        assert_ne!(child.peer_id, launch.peer_id);
        let saved = crate::config::load_app_config();
        let restarted = crate::state::tests_support::make_test_app_state();
        *restarted.config.write() = saved;
        restarted.config.write().ego_profile = "other-default".into();
        let restored = resolve(
            &restarted,
            &ChatOpenRequest {
                session_id: Some("conversation".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(restored, launch);
        let error = resolve(
            &restarted,
            &ChatOpenRequest {
                session_id: Some("conversation".into()),
                profile: Some("replace".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
        assert!(error.message.contains("cannot be replaced"));
    }
}

/// Successor conversations must retain launch authority rather than fall into defaults.
async fn inherit(state: &Arc<AppState>, source: &str, target: &str) -> Result<(), AcpClientError> {
    let saved = state.config.read().ai_chat_launches.get(source).cloned();
    if let Some(mut launch) = saved {
        launch.peer_id = uuid::Uuid::new_v4().to_string();
        persist(state, target, &launch).await?;
    }
    Ok(())
}

pub(crate) async fn fork(
    state: &Arc<AppState>,
    connection_id: crate::acp::AcpConnectionId,
    session_id: agent_client_protocol::schema::v1::SessionId,
    authority: AcpSessionAuthority,
    at_message_id: Option<String>,
) -> Result<crate::acp::AcpAttachmentSnapshot, AcpClientError> {
    let source = session_id.to_string();
    let attachment = state
        .acp
        .attach_at_message(
            connection_id,
            AcpAttachKind::Fork,
            session_id,
            authority,
            at_message_id,
        )
        .await?;
    inherit(state, &source, &attachment.session_id.to_string()).await?;
    Ok(attachment)
}

pub(crate) async fn compact(
    state: &Arc<AppState>,
    connection_id: crate::acp::AcpConnectionId,
    session_id: agent_client_protocol::schema::v1::SessionId,
    request_id: uuid::Uuid,
) -> Result<crate::acp::EgoCompactResponse, AcpClientError> {
    let source = session_id.to_string();
    let result = state
        .acp
        .compact(
            connection_id,
            crate::acp::EgoCompactRequest {
                session_id,
                request_id,
            },
        )
        .await?;
    // A failed publication produces no successor to persist.
    if !result.publication.retry_safe() {
        inherit(state, &source, &result.target_session_id.to_string()).await?;
    }
    Ok(result)
}
