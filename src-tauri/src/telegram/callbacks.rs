use super::{Error, mail::PendingMail, runtime::Runtime, tool::Button};
use serde_json::{Value, json};
use std::collections::VecDeque;
use tokio::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct Callbacks {
    entries: VecDeque<Choice>,
}
struct Choice {
    handle: String,
    chat: i64,
    message: i64,
    label: String,
    data: String,
    expires: Instant,
    selected: Option<PendingMail>,
    mailed: bool,
}
impl Runtime {
    pub async fn send_buttons(
        &mut self,
        chat: i64,
        text: &str,
        rows: Vec<Vec<Button>>,
    ) -> Result<Value, Error> {
        let count: usize = rows.iter().map(Vec::len).sum();
        if count > 20
            || rows.iter().any(|row| row.is_empty())
            || rows
                .iter()
                .flatten()
                .any(|b| b.label.is_empty() || b.label.chars().count() > 80 || b.data.len() > 4096)
        {
            return Err(Error::Config);
        }
        let mut choices = Vec::new();
        let mut keyboard = Vec::new();
        for row in rows {
            let mut wire_row = Vec::new();
            for button in row {
                let handle = format!("tc1:{}", uuid::Uuid::new_v4().simple());
                wire_row.push(json!({"text":button.label,"callback_data":handle}));
                choices.push((handle, button));
            }
            keyboard.push(wire_row);
        }
        let markup = (!keyboard.is_empty()).then(|| json!({"inline_keyboard":keyboard}));
        let ids = self.outbound.send(chat, text, markup).await?;
        let message = *ids.last().ok_or(Error::Protocol)?;
        self.callbacks
            .entries
            .retain(|c| c.expires > Instant::now());
        for (handle, button) in choices {
            if self.callbacks.entries.len() >= 100 {
                self.callbacks.entries.pop_front();
            }
            self.callbacks.entries.push_back(Choice {
                handle,
                chat,
                message,
                label: button.label,
                data: button.data,
                expires: Instant::now() + Duration::from_secs(3600),
                selected: None,
                mailed: false,
            });
        }
        Ok(json!({"message_ids":ids}))
    }

    pub async fn callback(&mut self, value: &Value) -> Result<(), Error> {
        let Some(query) = value.get("callback_query") else {
            return Ok(());
        };
        let message = &query["message"];
        if !message["date"].as_i64().is_some_and(|date| date > 0) {
            return Ok(());
        }
        let chat = &message["chat"];
        let Some(chat_id) = chat["id"].as_i64() else {
            return Ok(());
        };
        if chat["type"] != "private"
            || !self.outbound.paths.allowlist()?.contains(&chat_id)
            || query["from"]["id"].as_i64() != Some(chat_id)
        {
            return Ok(());
        }
        let Some(handle) = query["data"].as_str() else {
            return Ok(());
        };
        let Some(query_id) = query["id"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 256)
        else {
            return Ok(());
        };
        let Some(index) = self.callbacks.entries.iter().position(|c| {
            c.handle == handle
                && c.chat == chat_id
                && Some(c.message) == message["message_id"].as_i64()
                && c.expires > Instant::now()
        }) else {
            return Ok(());
        };
        let choice = &self.callbacks.entries[index];
        let message_id = choice.message;
        let selected_label = choice.label.clone();
        let selected = choice.selected.clone().unwrap_or_else(|| {
            let id = format!("tg:{}:choice:{}",self.config.bot_alias,handle);
            PendingMail { id: id.clone(),recipient:self.config.target_tuic_session.clone(),content:json!({"channel":"telegram","kind":"callback","request_id":id,"chat_id":chat_id.to_string(),"message_id":message_id,"callback_id":query_id,"data":choice.data}).to_string() }
        });
        let mailed = choice.mailed;
        // First choice retires every button on this message before any await.
        for choice in self
            .callbacks
            .entries
            .iter_mut()
            .filter(|c| c.chat == chat_id && c.message == message_id)
        {
            choice.selected = Some(selected.clone());
            choice.label = selected_label.clone();
        }
        if !mailed {
            super::native::offer(&self.state, &self.adapter_sid, &selected).await?;
            for choice in self
                .callbacks
                .entries
                .iter_mut()
                .filter(|c| c.chat == chat_id && c.message == message_id)
            {
                choice.mailed = true;
            }
        }
        let label = format!("Selected: {selected_label}");
        // Opaque callbacks are ordinary choices, never deterministic publish consent.
        self.outbound
            .call(
                chat_id,
                "answerCallbackQuery",
                json!({"callback_query_id":query_id,"text":label}),
            )
            .await?;
        // Preserve all delivered text while visibly retiring the choice keyboard.
        self.outbound.call(chat_id,"editMessageReplyMarkup",json!({"chat_id":chat_id,"message_id":message_id,"reply_markup":{"inline_keyboard":[[{"text":label,"callback_data":handle,"disabled":true}]]}})).await?;
        Ok(())
    }
}
