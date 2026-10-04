use super::{BotApi, Config, Error, Paths, mail::PendingMail, outbound::Outbound, tool::Input};
use crate::state::{AppEvent, AppState};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, OnceLock},
};
use tokio::sync::{mpsc, oneshot};

pub(super) static HANDLE: OnceLock<mpsc::Sender<Command>> = OnceLock::new();
pub(super) enum Command {
    Tool {
        caller: String,
        input: Input,
        reply: oneshot::Sender<Result<Value, Error>>,
    },
    Track {
        mail: PendingMail,
        reply: oneshot::Sender<Result<Value, Error>>,
    },
    Update {
        value: Value,
        reply: oneshot::Sender<Result<Value, Error>>,
    },
}

pub(super) struct Runtime {
    pub state: Arc<AppState>,
    pub config: Config,
    pub outbound: Outbound,
    pub pending: VecDeque<(String, i64)>,
    pub callbacks: super::callbacks::Callbacks,
    pub adapter_sid: String,
}
impl Runtime {
    pub fn new(
        state: Arc<AppState>,
        config: Config,
        paths: Paths,
        adapter_sid: String,
    ) -> Result<Self, Error> {
        Ok(Self {
            state,
            config,
            outbound: Outbound::new(paths.clone(), BotApi::new(paths)?),
            pending: VecDeque::new(),
            callbacks: Default::default(),
            adapter_sid,
        })
    }
    pub fn live(&self, peer: &str, pty: &str, epoch: u64) -> bool {
        peer == self.config.target_tuic_session && live(&self.state, peer, pty, epoch)
    }
    pub fn current(&self) -> Result<(String, u64), Error> {
        let pty = self
            .state
            .live_pty_for_peer(&self.config.target_tuic_session)
            .ok_or(Error::State)?;
        let state = self
            .state
            .session_state_with_shell(&pty)
            .ok_or(Error::State)?;
        if !matches!(
            state.agent_state.as_deref(),
            Some("working" | "awaiting_input")
        ) {
            return Err(Error::State);
        }
        Ok((pty.clone(), state.turn_epoch))
    }
    pub fn destination(&self) -> Result<i64, Error> {
        self.outbound
            .paths
            .allowlist()?
            .into_iter()
            .next()
            .ok_or(Error::Config)
    }
    pub async fn tool(&mut self, caller: &str, input: Input) -> Result<Value, Error> {
        // Check opt-in and target at use, including after a queued command.
        let config = Config::load(&self.outbound.paths)?.ok_or(Error::Config)?;
        if caller != self.config.target_tuic_session
            || config.target_tuic_session != caller
            || config.bot_alias != self.config.bot_alias
        {
            return Err(Error::State);
        }
        match input {
            Input::Begin { request_id } => {
                let chat = self
                    .pending
                    .iter()
                    .find(|(id, _)| id == &request_id)
                    .map(|(_, chat)| *chat)
                    .ok_or(Error::State)?;
                let (pty, epoch) = self.current()?;
                let draft = self
                    .outbound
                    .begin(request_id.clone(), caller.into(), pty, epoch, chat)
                    .await?;
                self.pending.retain(|(id, _)| id != &request_id);
                Ok(json!({"draft_id":draft}))
            }
            Input::Activity { request_id, text } => {
                let a = self.outbound.active.as_ref().ok_or(Error::State)?;
                if !self.live(&a.peer, &a.pty, a.epoch) {
                    return Err(Error::State);
                }
                self.outbound.activity(&request_id, &text)?;
                Ok(json!({"accepted":true}))
            }
            Input::Finish { request_id, text } => {
                let a = self.outbound.active.as_ref().ok_or(Error::State)?;
                if !self.live(&a.peer, &a.pty, a.epoch) {
                    return Err(Error::State);
                }
                self.callbacks.clear();
                let ids = self.outbound.finish(&request_id, &text).await?;
                Ok(json!({"message_ids":ids}))
            }
            Input::Send { text, buttons } => {
                let chat = self.destination()?;
                self.send_buttons(chat, &text, buttons).await
            }
        }
    }
    pub fn track(&mut self, mail: PendingMail) -> Result<Value, Error> {
        let envelope: Value = serde_json::from_str(&mail.content).map_err(|_| Error::Protocol)?;
        let chat = envelope["chat_id"]
            .as_str()
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or(Error::Protocol)?;
        if !self.pending.iter().any(|(id, _)| id == &mail.id) {
            // Correlation follows the bounded native inbox; no retained text.
            if self.pending.len() >= crate::state::AGENT_INBOX_CAPACITY {
                self.pending.pop_front();
            }
            self.pending.push_back((mail.id, chat));
        }
        Ok(json!({"accepted":true}))
    }
    pub async fn update(&mut self, value: Value) -> Result<Value, Error> {
        if !self.enabled() {
            return Err(Error::Config);
        }
        self.callback(&value).await?;
        Ok(json!({"accepted":true}))
    }
    fn enabled(&self) -> bool {
        Config::load(&self.outbound.paths)
            .ok()
            .flatten()
            .is_some_and(|c| {
                c.target_tuic_session == self.config.target_tuic_session
                    && c.bot_alias == self.config.bot_alias
            })
    }
    pub async fn event(&mut self, event: AppEvent) {
        if !self.enabled() {
            self.outbound.active = None;
            return;
        }
        let Some(pty) = self
            .state
            .live_pty_for_peer(&self.config.target_tuic_session)
        else {
            self.outbound.active = None;
            return;
        };
        if let Some(text) = super::notifications::notice(&event, &pty) {
            match self.destination() {
                Ok(chat) => {
                    self.callbacks.clear();
                    if let Err(error) = self.outbound.send(chat, &text, None).await {
                        alert(&self.state, error);
                    }
                }
                Err(error) => alert(&self.state, error),
            }
        }
        if let AppEvent::PtyParsed { session_id, parsed } = event
            && let Some(a) = &self.outbound.active
            && session_id == a.pty
            && parsed["_turn_epoch"].as_u64() == Some(a.epoch)
            && parsed["type"] == "intent"
            && let Some(text) = parsed["text"].as_str()
        {
            let request = a.request.clone();
            if let Err(error) = self.outbound.activity(&request, text) {
                alert(&self.state, error);
            }
        }
    }
    pub async fn tick(&mut self) {
        if !self.enabled() {
            self.outbound.active = None;
            return;
        }
        if let Some(a) = &self.outbound.active
            && !self.live(&a.peer, &a.pty, a.epoch)
        {
            self.outbound.active = None;
            tracing::warn!(
                source = "telegram",
                "Telegram response ended without finish; draft retired"
            );
        }
        if let Err(error) = self.outbound.refresh().await {
            alert(&self.state, error);
        }
    }
    pub async fn run(mut self, mut commands: mpsc::Receiver<Command>) {
        let mut events = self.state.event_bus.subscribe();
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            tokio::select! {
                command = commands.recv() => {
                    let Some(command) = command else { break; };
                    let (reply,result) = match command {
                        Command::Tool { caller,input,reply } => (reply,self.tool(&caller,input).await),
                        Command::Track { mail,reply } => (reply,self.track(mail)),
                        Command::Update { value,reply } => (reply,self.update(value).await),
                    };
                    let _ = reply.send(result);
                },
                event = events.recv() => match event {
                    Ok(event) => self.event(event).await,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        self.outbound.active = None;
                        tracing::warn!(source="telegram","Telegram events lagged; draft retired, notifications may be missing");
                    },
                    Err(_) => break,
                },
                _ = interval.tick() => self.tick().await,
            }
        }
    }
}

fn live(state: &AppState, peer: &str, pty: &str, epoch: u64) -> bool {
    state.live_pty_for_peer(peer).as_deref() == Some(pty)
        && !state.session_maps.exit_codes.contains_key(pty)
        && state.session_state_with_shell(pty).is_some_and(|s| {
            s.turn_epoch == epoch
                && matches!(s.agent_state.as_deref(), Some("working" | "awaiting_input"))
        })
}
pub(super) fn alert(state: &AppState, error: Error) {
    let _ = state.event_bus.send(AppEvent::McpToast {
        title: "Telegram adapter unavailable".into(),
        message: Some(error.to_string()),
        level: "error".into(),
        sound: None,
        origin_repo_path: None,
        origin_session_id: None,
    });
    tracing::error!(source="telegram",error=%error,"Telegram adapter unavailable; inspect daemon logs");
}
