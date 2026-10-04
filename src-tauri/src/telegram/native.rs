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
        if value.get("callback_query").is_none() {
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

/// Explicit headless startup only; absent/disabled config does not open secrets.
pub(crate) fn start(state: &Arc<AppState>) {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let paths = super::Paths::new(home.join(".config/tuic-telegram"));
    let config = match super::Config::load(&paths) {
        Ok(Some(config)) => config,
        Ok(None) => return,
        Err(error) => {
            super::runtime::alert(state, error);
            return;
        }
    };
    let sid = format!("telegram:{}", uuid::Uuid::new_v4());
    let (commands, receive) = mpsc::channel(100);
    let port = NativeMail::new(commands.clone());
    let inbound = match super::inbound::Inbound::with_port(paths.clone(), port) {
        Ok(Some(inbound)) => inbound,
        Ok(None) => return,
        Err(error) => {
            super::runtime::alert(state, error);
            return;
        }
    };
    let runtime = match Runtime::new(state.clone(), config, paths, sid.clone()) {
        Ok(runtime) => runtime,
        Err(error) => {
            super::runtime::alert(state, error);
            return;
        }
    };
    if HANDLE.set(commands).is_err() {
        super::runtime::alert(state, Error::AlreadyOwned);
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        let registered = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
            &state,
            &json!({"action":"register","name":"telegram-adapter"}),
            Some(&sid),
            None,
        )
        .await;
        if registered.get("error").is_some() {
            super::runtime::alert(&state, Error::State);
            return;
        }
        let worker = tokio::spawn(runtime.run(receive));
        let mut inbound = inbound;
        loop {
            match inbound.poll().await {
                Ok(super::inbound::Poll::Backoff(delay)) => tokio::time::sleep(delay).await,
                Ok(super::inbound::Poll::Accepted(_)) => {}
                Err(
                    error @ (Error::Unauthorized | Error::Conflict | Error::Rejected(403 | 404)),
                ) => {
                    super::runtime::alert(&state, error);
                    worker.abort();
                    break;
                }
                Err(error) => {
                    super::runtime::alert(&state, error);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    });
}
