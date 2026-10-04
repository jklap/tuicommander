use super::{Error, outbound::Outbound};
use serde_json::Value;

impl Outbound {
    /// Retire the draft before requesting Esc. No network await separates the
    /// live-turn check from the write. A write is a request, not proof of stop.
    pub fn stop(
        &mut self,
        update: &Value,
        current: impl FnOnce(&str, &str, u64) -> bool,
        write: impl FnOnce(&str) -> Result<(), Error>,
    ) -> Result<bool, Error> {
        let Some(stopped) = update.get("stopped_message_generation") else {
            return Ok(false);
        };
        let chat = &stopped["chat"];
        let Some(chat_id) = chat["id"].as_i64() else {
            return Err(Error::Protocol);
        };
        if chat["type"] != "private" || !self.paths.allowlist()?.contains(&chat_id) {
            return Ok(false);
        }
        let draft = stopped["draft_id"].as_i64().ok_or(Error::Protocol)?;
        if !self
            .active
            .as_ref()
            .is_some_and(|a| a.chat == chat_id && a.draft == draft)
        {
            return Ok(false);
        }
        let active = self.active.take().ok_or(Error::State)?;
        let live = current(&active.peer, &active.pty, active.epoch);
        if !live {
            tracing::info!(source="telegram",request_id=%active.request,draft_id=draft,"Stale Telegram Stop ignored");
            return Ok(false);
        }
        let result = write(&active.pty);
        tracing::info!(source="telegram",request_id=%active.request,peer=%active.peer,draft_id=draft,written=result.is_ok(),"Telegram Stop interrupt requested");
        result.map(|()| true)
    }
}
