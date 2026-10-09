use super::{
    Error,
    mail::{MailPort, PendingMail},
    runtime::{Command, HANDLE, Runtime},
};
use crate::state::AppState;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

pub(super) async fn offer(
    state: &Arc<AppState>,
    sid: &str,
    mail: &PendingMail,
) -> Result<(), Error> {
    let result = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        state,
        &json!({"action":"send","to":mail.recipient,"message":mail.content,"urgency":"normal"}),
        Some(sid),
        Some(mail.id.clone()),
    )
    .await;
    if result.get("error").is_some() {
        return Err(Error::State);
    }
    // Inbox-only is a valid handoff; readiness/wake arbitration belongs to TUIC.
    Ok(())
}
pub(super) struct NativeMail {
    commands: mpsc::Sender<Command>,
}
impl NativeMail {
    pub(super) fn new(commands: mpsc::Sender<Command>) -> Self {
        Self { commands }
    }
}
impl MailPort for NativeMail {
    async fn offer(&mut self, mail: &PendingMail) -> Result<bool, Error> {
        let (reply, receive) = oneshot::channel();
        self.commands
            .send(Command::Deliver {
                mail: mail.clone(),
                reply,
            })
            .await
            .map_err(|_| Error::State)?;
        let value = receive.await.map_err(|_| Error::State)??;
        value["accepted"].as_bool().ok_or(Error::Protocol)
    }
    async fn update(&mut self, value: &Value) -> Result<(), Error> {
        if value.get("callback_query").is_none()
            && value.get("stopped_message_generation").is_none()
        {
            return Ok(());
        }
        let (reply, receive) = oneshot::channel();
        self.commands
            .send(Command::Update {
                value: value.clone(),
                reply,
            })
            .await
            .map_err(|_| Error::State)?;
        receive.await.map_err(|_| Error::State)??;
        Ok(())
    }
}

/// One daemon owner, restarted explicitly by setup changes; desktop never polls.
pub(crate) fn start(state: &Arc<AppState>) {
    let Ok(paths) = super::settings::paths() else {
        return;
    };
    let (commands, mut receive) = mpsc::channel(100);
    if HANDLE.set(commands.clone()).is_err() {
        super::runtime::alert(state, Error::AlreadyOwned);
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        let sid = format!("telegram:{}", uuid::Uuid::new_v4());
        loop {
            super::settings::registration_status(&paths, None);
            let revision = super::settings::revision(&paths);
            let config = match super::Config::load(&paths) {
                Ok(Some(config)) => config,
                Ok(None) => {
                    super::settings::status(false, None, false);
                    super::settings::changed(&paths, &revision).await;
                    continue;
                }
                Err(error) => {
                    super::settings::status(false, Some(error), false);
                    super::runtime::alert(&state, error);
                    super::settings::changed(&paths, &revision).await;
                    continue;
                }
            };
            let port = NativeMail::new(commands.clone());
            let mut inbound = match super::inbound::Inbound::with_port(paths.clone(), port) {
                Ok(Some(inbound)) => inbound,
                Ok(None) => continue,
                Err(error) => {
                    super::settings::status(false, Some(error), false);
                    super::settings::changed(&paths, &revision).await;
                    continue;
                }
            };
            let runtime = match Runtime::new(state.clone(), config, paths.clone(), sid.clone()) {
                Ok(runtime) => runtime,
                Err(error) => {
                    super::settings::status(false, Some(error), false);
                    super::settings::changed(&paths, &revision).await;
                    continue;
                }
            };
            let registered = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
                &state,
                &json!({"action":"register","name":"telegram-adapter"}),
                Some(&sid),
                None,
            )
            .await;
            if registered.get("error").is_some() {
                super::settings::status(false, Some(Error::State), false);
                super::settings::changed(&paths, &revision).await;
                continue;
            }
            tokio::select! {
                _ = super::settings::changed(&paths, &revision) => {},
                _ = runtime.run_ref(&mut receive) => {},
                _ = async {
                    loop {
                        match inbound.poll().await {
                            Ok(super::inbound::Poll::Backoff(delay)) => tokio::time::sleep(delay).await,
                            Ok(super::inbound::Poll::Accepted(count)) => super::settings::status(true, None, count > 0),
                            Err(error) => {
                                super::settings::status(false, Some(error), false);
                                super::runtime::alert(&state, error);
                                if matches!(error, Error::Unauthorized | Error::Conflict | Error::Rejected(403 | 404)) {
                                    super::settings::changed(&paths, &revision).await;
                                    break;
                                }
                                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            }
                        }
                    }
                } => {},
            }
            super::settings::registration_status(&paths, None);
            super::settings::status(false, None, false);
        }
    });
}
