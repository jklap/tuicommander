use super::{Error, runtime::Runtime};
use crate::state::AppState;
use serde_json::Value;
use std::{io::Write, sync::Arc};

/// Arm a draft under the same writer lock that retires it before replacement input.
pub(super) fn arm(state: &AppState, peer: &str, pty: &str, epoch: u64) -> Result<Arc<()>, Error> {
    let writer = state.pty_writer(pty).ok_or(Error::State)?;
    let _writer = writer.lock();
    if !super::runtime::live(state, peer, pty, epoch) {
        return Err(Error::State);
    }
    let token = Arc::new(());
    state
        .session_maps
        .session_states
        .get_mut(pty)
        .ok_or(Error::State)?
        .turn_interrupt = Some(token.clone());
    Ok(token)
}

impl Runtime {
    pub(super) fn stop(&mut self, update: &Value) -> Result<bool, Error> {
        let Some(stopped) = update.get("stopped_message_generation") else {
            return Ok(false);
        };
        let chat = &stopped["chat"];
        let Some(chat_id) = chat["id"].as_i64() else {
            return Err(Error::Protocol);
        };
        if chat["type"] != "private" || !self.outbound.paths.allowlist()?.contains(&chat_id) {
            return Ok(false);
        }
        let draft = stopped["draft_id"].as_i64().ok_or(Error::Protocol)?;
        if !self
            .outbound
            .active
            .as_ref()
            .is_some_and(|a| a.chat == chat_id && a.draft == draft)
        {
            return Ok(false);
        }
        // Consume before writing: failures and duplicate updates never retry Escape.
        let active = self.outbound.active.take().ok_or(Error::State)?;
        let Some(token) = active.interrupt else {
            return Ok(false);
        };
        let writer = self.state.pty_writer(&active.pty).ok_or(Error::State)?;
        let mut writer = writer.lock();
        if !self.live(&active.peer, &active.pty, active.epoch) {
            return Ok(false);
        }
        let owned = {
            let mut session = self
                .state
                .session_maps
                .session_states
                .get_mut(&active.pty)
                .ok_or(Error::State)?;
            if !session
                .turn_interrupt
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &token))
            {
                return Ok(false);
            }
            session.turn_interrupt.take()
        };
        drop(owned);
        // This is an interrupt request, not composer text. Feeding a bare ESC
        // into InputLineBuffer would consume the replacement's first character.
        // Release SilenceState before write: captured writers process output inline.
        if let Some(silence) = self.state.session_maps.silence_states.get(&active.pty) {
            silence.lock().note_interrupt_requested();
        }
        writer
            .write_all(b"\x1b")
            .and_then(|()| writer.flush())
            .map_err(|_| Error::Transport)?;
        crate::pty_capture::record_input(&active.pty, b"\x1b");
        Ok(true)
    }
}
