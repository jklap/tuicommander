use super::{BotApi, Error, Paths};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::Instant;

/// Deliberately conservative UTF-16 budget, including astral characters.
pub(super) fn chunks(text: &str) -> Result<Vec<&str>, Error> {
    if text.is_empty() || text.len() > 64 * 1024 {
        return Err(Error::Config);
    }
    let mut result = Vec::new();
    let mut start = 0;
    let mut units = 0;
    for (index, ch) in text.char_indices() {
        if units + ch.len_utf16() > 4096 {
            result.push(&text[start..index]);
            start = index;
            units = 0;
        }
        units += ch.len_utf16();
    }
    result.push(&text[start..]);
    Ok(result)
}

pub(super) struct Active {
    pub request: String,
    pub peer: String,
    pub pty: String,
    pub epoch: u64,
    pub chat: i64,
    pub draft: i64,
    text: String,
    sent: Instant,
    dirty: bool,
}

/// One worker serializes draft refresh and final delivery. Polling runs separately.
pub(super) struct Outbound {
    pub paths: Paths,
    pub api: BotApi,
    pub active: Option<Active>,
    next_send: Instant,
    stopped: Option<Error>,
}
impl Outbound {
    pub fn new(paths: Paths, api: BotApi) -> Self {
        Self {
            paths,
            api,
            active: None,
            next_send: Instant::now(),
            stopped: None,
        }
    }

    pub async fn call(&mut self, chat: i64, method: &str, body: Value) -> Result<Value, Error> {
        if let Some(error) = self.stopped {
            return Err(error);
        }
        // A long Telegram throttle must not hold the worker (and Stop) asleep.
        if let Some(wait) = self.next_send.checked_duration_since(Instant::now())
            && wait > Duration::from_secs(1)
        {
            return Err(Error::RateLimited(wait.as_secs().saturating_add(1)));
        }
        tokio::time::sleep_until(self.next_send).await;
        // Revocation is checked immediately before every operation, after waiting.
        if !self.paths.allowlist()?.contains(&chat) {
            return Err(Error::Config);
        }
        self.next_send = Instant::now() + Duration::from_secs(1);
        let result = self.api.request(method, body).await;
        if let Err(Error::RateLimited(seconds)) = result {
            self.next_send = Instant::now() + Duration::from_secs(seconds.clamp(1, 3600));
        }
        if let Err(error @ (Error::Unauthorized | Error::Conflict | Error::Rejected(403 | 404))) =
            result
        {
            self.stopped = Some(error);
            self.active = None;
        }
        result
    }

    pub async fn begin(
        &mut self,
        request: String,
        peer: String,
        pty: String,
        epoch: u64,
        chat: i64,
    ) -> Result<i64, Error> {
        if self.active.is_some() {
            return Err(Error::State);
        }
        let bytes = *uuid::Uuid::new_v4().as_bytes();
        let draft = (i64::from_be_bytes(bytes[..8].try_into().map_err(|_| Error::State)?)
            & i64::MAX)
            .max(1);
        let accepted = self.call(
            chat,
            "sendMessageDraft",
            json!({"chat_id":chat,"draft_id":draft,"text":"","can_stop":true,"keep_on_stop":false}),
        )
        .await?;
        if accepted != Value::Bool(true) {
            return Err(Error::Protocol);
        }
        self.active = Some(Active {
            request,
            peer,
            pty,
            epoch,
            chat,
            draft,
            text: String::new(),
            sent: Instant::now(),
            dirty: false,
        });
        Ok(draft)
    }

    pub fn activity(&mut self, request: &str, text: &str) -> Result<(), Error> {
        let active = self
            .active
            .as_mut()
            .filter(|a| a.request == request)
            .ok_or(Error::State)?;
        // Activity is a bounded unapproved preview, never an approval state.
        let text = text.chars().take(500).collect::<String>();
        active.text = format!("Preview — not approved.\n{text}");
        active.dirty = true;
        Ok(())
    }

    pub async fn refresh(&mut self) -> Result<(), Error> {
        let Some(active) = &self.active else {
            return Ok(());
        };
        let elapsed = active.sent.elapsed();
        if elapsed < Duration::from_secs(20) && !(active.dirty && elapsed >= Duration::from_secs(2))
        {
            return Ok(());
        }
        let chat = active.chat;
        let body = json!({"chat_id":chat,"draft_id":active.draft,"text":active.text,"can_stop":true,"keep_on_stop":false});
        if self.call(chat, "sendMessageDraft", body).await? != Value::Bool(true) {
            return Err(Error::Protocol);
        }
        if let Some(active) = &mut self.active {
            active.sent = Instant::now();
            active.dirty = false;
        }
        Ok(())
    }

    pub async fn send(
        &mut self,
        chat: i64,
        text: &str,
        keyboard: Option<Value>,
    ) -> Result<Vec<i64>, Error> {
        let parts = chunks(text)?;
        let mut ids = Vec::new();
        for (index, part) in parts.iter().enumerate() {
            let mut body = json!({"chat_id":chat,"text":part});
            if index + 1 == parts.len()
                && let Some(keyboard) = &keyboard
            {
                body["reply_markup"] = keyboard.clone();
            }
            let message = self.call(chat, "sendMessage", body).await?;
            let id = message["message_id"]
                .as_i64()
                .filter(|id| *id > 0)
                .ok_or(Error::Protocol)?;
            ids.push(id);
        }
        Ok(ids)
    }

    pub async fn finish(&mut self, request: &str, text: &str) -> Result<Vec<i64>, Error> {
        chunks(text)?;
        if !self.active.as_ref().is_some_and(|a| a.request == request) {
            return Err(Error::State);
        }
        // Retire before any network side effect: a partial/uncertain final cannot
        // be followed by a draft refresh, nor retried as though nothing was sent.
        let active = self.active.take().ok_or(Error::State)?;
        self.send(active.chat, text, None).await
    }
}
