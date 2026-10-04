use super::{Error, outbound::Outbound};
use serde_json::Value;

impl Outbound {
    /// Retire the draft before a native epoch-conditioned interrupt request.
    /// A write is a request, not proof of stop.
    pub fn stop(
        &mut self,
        update: &Value,
        interrupt: impl FnOnce(&str, &str, u64) -> Result<bool, Error>,
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
        let result = interrupt(&active.peer, &active.pty, active.epoch);
        tracing::info!(source="telegram",request_id=%active.request,peer=%active.peer,draft_id=draft,written=matches!(result,Ok(true)),"Telegram Stop interrupt requested");
        result
    }
}
