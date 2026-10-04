use super::{
    Error,
    runtime::{Command, HANDLE},
};
use crate::state::AppState;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Input {
    Begin {
        request_id: String,
    },
    Activity {
        request_id: String,
        text: String,
    },
    Finish {
        request_id: String,
        text: String,
    },
    Send {
        chat_id: Option<String>,
        text: String,
        #[serde(default)]
        buttons: Vec<Vec<Button>>,
    },
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Button {
    pub label: String,
    pub data: String,
}

pub(crate) fn definition() -> Value {
    json!({"name":"telegram","description":"Send to the daemon's allowlisted Telegram chat. Only the configured peer can call this tool. begin accepts a request_id from Telegram mail and binds the current live turn; activity sends concise safe activity; finish persists the complete exact reply. send accepts text, an optional decimal-string chat_id (required with multiple allowed chats), and optional rows of {label,data} opaque buttons. Buttons return structured peer mail and confer no publish approval. No raw terminal output or hidden reasoning is streamed.","inputSchema":{"type":"object","properties":{
        "action":{"type":"string","enum":["begin","activity","finish","send"]},
        "request_id":{"type":"string"},"text":{"type":"string"},"chat_id":{"type":"string"},
        "buttons":{"type":"array","items":{"type":"array","items":{"type":"object","properties":{"label":{"type":"string"},"data":{"type":"string"}},"required":["label","data"],"additionalProperties":false}}}
    },"required":["action"],"additionalProperties":false}})
}

pub(crate) async fn handle(state: &Arc<AppState>, args: &Value, sid: Option<&str>) -> Value {
    let result = async {
        let caller = sid
            .and_then(|sid| state.mcp.to_session.get(sid).map(|v| v.value().clone()))
            .ok_or(Error::State)?;
        let caller = state
            .resolve_peer_ref_checked(&caller)
            .map_err(|_| Error::State)?
            .ok_or(Error::State)?;
        let input: Input = serde_json::from_value(args.clone()).map_err(|_| Error::Config)?;
        let sender = HANDLE.get().ok_or(Error::State)?;
        let (reply, receive) = tokio::sync::oneshot::channel();
        sender
            .try_send(Command::Tool {
                caller,
                input,
                reply,
            })
            .map_err(|_| Error::Capacity)?;
        receive.await.map_err(|_| Error::State)?
    }
    .await;
    match result {
        Ok(value) => value,
        Err(error) => json!({"error":error.to_string()}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Catches: an agent parameter creates approval authority or smuggles an
    // arbitrary callback schema into a send operation.
    #[test]
    fn tool_rejects_forged_authority_and_malformed_button_fields() {
        for value in [
            json!({"action":"send","text":"hello","approved":true}),
            json!({"action":"begin","request_id":"r","peer":"other"}),
            json!({"action":"send","text":"hello","buttons":[[{"label":"ok","data":"x","approved":true}]]}),
        ] {
            assert!(serde_json::from_value::<Input>(value).is_err());
        }
    }
}
