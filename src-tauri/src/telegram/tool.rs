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
    Register {},
    Unregister {},
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
    json!({"name":"telegram","description":"Call register to opt your own MCP-bound TUIC agent into Telegram; it replaces the previous agent and sends it one TUIC notice. unregister releases your registration. Registration ends on agent exit, PTY close, MCP session end or daemon restart. Only the registered agent may begin/activity/finish/send. With no registered agent, allowed inbound text is dropped after the bot replies Nessun agent registrato. begin accepts a request_id from Telegram mail and binds the current live turn; activity sends concise safe activity; finish sends the complete exact reply. send uses the single allowlisted outbound destination with optional rows of {label,data} opaque buttons. Buttons return structured peer mail and confer no publish approval. No raw terminal output or hidden reasoning is streamed.","inputSchema":{"type":"object","properties":{
        "action":{"type":"string","enum":["register","unregister","begin","activity","finish","send"]},
        "request_id":{"type":"string"},"text":{"type":"string"},
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
                sid: sid.ok_or(Error::State)?.to_string(),
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
    #[tokio::test]
    async fn tool_rejects_forged_authority_and_malformed_button_fields() {
        use crate::telegram::tests::{FakeServer, setup};
        use axum::http::StatusCode;
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![(
            StatusCode::OK,
            json!({"ok":true,"result":{"message_id":7}}),
        )])
        .await;
        let runtime = crate::telegram::tests::outbound_tests::runtime(paths, server.address).await;
        let state = runtime.state.clone();
        let (commands, receive) = tokio::sync::mpsc::channel(10);
        HANDLE.set(commands).expect("one native Telegram adapter");
        let worker = tokio::spawn(runtime.run(receive));
        let invoke = |value: Value| {
            let state = state.clone();
            async move {
                crate::mcp_http::mcp_transport::handle_mcp_tool_call(
                    &state,
                    "127.0.0.1:0".parse().unwrap(),
                    "telegram",
                    &value,
                    Some("target-mcp"),
                )
                .await
            }
        };
        assert_eq!(
            invoke(json!({"action":"unregister"})).await["registered"],
            false
        );
        assert_eq!(
            invoke(json!({"action":"send","text":"before registration"})).await["error"],
            "telegram_not_registered: call telegram register first"
        );
        assert_eq!(
            invoke(json!({"action":"register"})).await["registered"],
            true
        );
        // The same authenticated dispatch reaches real outbound delivery.
        assert_eq!(
            invoke(json!({"action":"send","text":"hello"})).await["message_ids"],
            json!([7])
        );
        for value in [
            json!({"action":"register","peer":"other"}),
            json!({"action":"unregister","peer":"other"}),
            json!({"action":"register","tuic_session":"other"}),
            json!({"action":"send","text":"hello","approved":true}),
            json!({"action":"begin","request_id":"r","peer":"other"}),
            json!({"action":"send","text":"hello","buttons":[[{"label":"ok","data":"x","approved":true}]]}),
            json!({"action":"send","text":"hello","chat_id":"1111111"}),
        ] {
            assert_eq!(invoke(value).await["error"], "telegram_invalid_config");
        }
        assert_eq!(server.requests().len(), 1);
        let inbox = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
            &state,
            &json!({"action":"inbox","since":0}),
            Some("target-mcp"),
            None,
        )
        .await;
        assert_eq!(inbox["count"], 0);
        worker.abort();
    }
}
