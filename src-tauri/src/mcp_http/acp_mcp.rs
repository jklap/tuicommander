//! The `tuicommander` MCP server as ego reaches it: over the ACP connection.
//!
//! Not a second server. Every request goes through [`mcp_post`], the handler
//! behind HTTP `/mcp`, with the headers a bridge would have sent — so the tool
//! registry, the collapsed surface ego is given and the peer binding are the
//! ones every other client gets. What this module owns is only what HTTP gets
//! from the transport: a protocol session per MCP connection, bound to the
//! identity ego was launched under, and the server-to-client stream that GET
//! `/mcp` would have carried.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use parking_lot::Mutex;
use serde_json::{Map, Value, json};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use super::mcp_transport::{
    MCP_SESSION_HEADER, TUIC_SESSION_HEADER, end_mcp_session, mcp_post, refresh_mcp_session,
};
use crate::AppState;
use crate::acp::{McpNotify, McpOverAcpError, McpOverAcpHost, McpReply};

/// JSON-RPC's "invalid params", for a connection id this host never issued.
const INVALID_PARAMS: i32 = -32602;
const INTERNAL_ERROR: i32 = -32603;

/// The address a request over ACP is served as: ego runs on this machine, and
/// the IPC listener hands its requests the same synthetic loopback peer.
fn loopback() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 0))
}

pub(crate) struct AcpMcpHost {
    state: Weak<AppState>,
    connections: Mutex<HashMap<String, Link>>,
}

/// What one MCP connection holds beyond its protocol session.
struct Link {
    peer_id: Option<String>,
    /// The stream GET `/mcp` would have carried, forwarded to ego.
    forwarder: JoinHandle<()>,
}

impl AcpMcpHost {
    pub(crate) fn new(state: &Arc<AppState>) -> Self {
        Self {
            state: Arc::downgrade(state),
            connections: Mutex::new(HashMap::new()),
        }
    }

    fn state(&self) -> Result<Arc<AppState>, McpOverAcpError> {
        self.state
            .upgrade()
            .ok_or_else(|| McpOverAcpError::new(INTERNAL_ERROR, "TUICommander is shutting down"))
    }
}

/// Serve `tuicommander` to every ACP connection opened from now on.
///
/// Installed next to the maintenance sweep, on the desktop app and on the
/// daemon alike: both launch ego, and a connection opened before this runs
/// answers `mcp/connect` with "this client serves no MCP over ACP".
pub(crate) fn install(state: &Arc<AppState>) {
    state.acp.set_mcp_host(Arc::new(AcpMcpHost::new(state)));
}

impl McpOverAcpHost for AcpMcpHost {
    fn connect(&self, peer_id: Option<&str>, notify: McpNotify) -> Result<String, McpOverAcpError> {
        let state = self.state()?;
        let id = uuid::Uuid::new_v4().to_string();
        refresh_mcp_session(&state, &id, false, peer_id);
        // Subscribed under the session entry, the order `mcp_get` keeps:
        // `mcp_sessions` before `messaging_channels`.
        let messages = {
            let Some(mut meta) = state.mcp.sessions.get_mut(&id) else {
                return Err(McpOverAcpError::new(
                    INTERNAL_ERROR,
                    "MCP session vanished while opening",
                ));
            };
            meta.has_sse_stream = true;
            state
                .session_maps
                .messaging_channels
                .entry(id.clone())
                .or_insert_with(|| broadcast::channel(64).0)
                .subscribe()
        };
        let tools = state.mcp.tools_changed.subscribe();
        let forwarder = tokio::spawn(forward(tools, messages, notify));
        self.connections.lock().insert(
            id.clone(),
            Link {
                peer_id: peer_id.map(str::to_owned),
                forwarder,
            },
        );
        Ok(id)
    }

    fn message(
        &self,
        connection_id: &str,
        method: String,
        params: Option<Map<String, Value>>,
    ) -> McpReply {
        let prepared = self.state().and_then(|state| {
            let peer_id = self
                .connections
                .lock()
                .get(connection_id)
                .map(|link| link.peer_id.clone())
                .ok_or_else(|| {
                    McpOverAcpError::new(
                        INVALID_PARAMS,
                        format!("no MCP connection {connection_id}"),
                    )
                })?;
            let mut headers = HeaderMap::new();
            let header = |value: &str| {
                value.parse().map_err(|_| {
                    McpOverAcpError::new(INVALID_PARAMS, "identity is not a header value")
                })
            };
            headers.insert(MCP_SESSION_HEADER, header(connection_id)?);
            if let Some(peer_id) = &peer_id {
                headers.insert(TUIC_SESSION_HEADER, header(peer_id)?);
            }
            Ok((state, headers))
        });
        let body = json!({ "jsonrpc": "2.0", "id": 0, "method": method, "params": params });
        Box::pin(async move {
            let (state, headers) = prepared?;
            let response = mcp_post(State(state), ConnectInfo(loopback()), headers, Json(body))
                .await
                .into_response();
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .map_err(|error| McpOverAcpError::new(INTERNAL_ERROR, error.to_string()))?;
            let reply: Value = serde_json::from_slice(&bytes)
                .map_err(|error| McpOverAcpError::new(INTERNAL_ERROR, error.to_string()))?;
            match reply.get("error") {
                Some(error) => Err(McpOverAcpError {
                    code: error["code"]
                        .as_i64()
                        .and_then(|code| i32::try_from(code).ok())
                        .unwrap_or(INTERNAL_ERROR),
                    message: error["message"].as_str().unwrap_or_default().to_owned(),
                    data: error.get("data").cloned(),
                }),
                None => Ok(reply.get("result").cloned().unwrap_or(Value::Null)),
            }
        })
    }

    /// Nothing ego notifies changes server state: `notifications/initialized`
    /// is an acknowledgement, and `notifications/cancelled` is served by the
    /// ACP request cancellation of the `mcp/message` it names.
    fn notification(
        &self,
        _connection_id: &str,
        _method: String,
        _params: Option<Map<String, Value>>,
    ) {
    }

    fn disconnect(&self, connection_id: &str) {
        let Some(link) = self.connections.lock().remove(connection_id) else {
            return;
        };
        link.forwarder.abort();
        if let Ok(state) = self.state() {
            end_mcp_session(&state, connection_id);
            state.session_maps.messaging_channels.remove(connection_id);
        }
    }
}

/// Carry what GET `/mcp` would have streamed to ego, until the connection ends.
async fn forward(
    mut tools: broadcast::Receiver<()>,
    mut messages: broadcast::Receiver<String>,
    notify: McpNotify,
) {
    use broadcast::error::RecvError;
    loop {
        tokio::select! {
            changed = tools.recv() => match changed {
                Ok(()) => notify("notifications/tools/list_changed".to_owned(), None),
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return,
            },
            message = messages.recv() => match message {
                Ok(text) => {
                    let Ok(frame) = serde_json::from_str::<Value>(&text) else {
                        tracing::warn!(source = "acp_mcp", "dropped a channel frame that is not JSON");
                        continue;
                    };
                    let Some(method) = frame["method"].as_str() else {
                        continue;
                    };
                    notify(method.to_owned(), frame.get("params").and_then(Value::as_object).cloned());
                }
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::test_state;
    use super::*;

    const PEER: &str = "550e8400-e29b-41d4-a716-446655440a01";

    type Heard = Arc<Mutex<Vec<(String, Option<Map<String, Value>>)>>>;

    fn listener() -> (McpNotify, Heard) {
        let heard: Heard = Arc::default();
        let sink = Arc::clone(&heard);
        (
            Arc::new(move |method, params| sink.lock().push((method, params))),
            heard,
        )
    }

    fn object(value: Value) -> Option<Map<String, Value>> {
        value.as_object().cloned()
    }

    /// ego's per-request identity, as it sends it on every MCP request.
    fn ego_meta() -> Value {
        json!({ "io.modelcontextprotocol/clientInfo": { "name": "ego", "version": "test" } })
    }

    fn names(result: &Value) -> Vec<String> {
        let mut names: Vec<String> = result["tools"]
            .as_array()
            .expect("a tools list")
            .iter()
            .map(|tool| tool["name"].as_str().unwrap_or_default().to_owned())
            .collect();
        names.sort();
        names
    }

    async fn eventually(what: &str, ready: impl Fn() -> bool) {
        for _ in 0..200 {
            if ready() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("{what} within 2s");
    }

    /// One registry, one surface: what ego lists over ACP is what HTTP `/mcp`
    /// lists for the same client, and the MCP connection carries ego's peer.
    #[tokio::test]
    async fn an_acp_connection_reaches_the_http_handler_under_egos_identity() {
        let state = test_state();
        let host = AcpMcpHost::new(&state);
        let (notify, _) = listener();
        let id = host.connect(Some(PEER), notify).expect("mcp/connect");

        assert_eq!(
            state
                .mcp
                .to_session
                .get(&id)
                .map(|peer| peer.value().clone()),
            Some(PEER.to_owned()),
            "the MCP connection is bound to the identity ego was launched under"
        );

        let over_acp = host
            .message(
                &id,
                "tools/list".to_owned(),
                object(json!({ "_meta": ego_meta() })),
            )
            .await
            .expect("tools/list over ACP");
        let over_http = mcp_post(
            State(Arc::clone(&state)),
            ConnectInfo("127.0.0.1:0".parse().unwrap()),
            HeaderMap::new(),
            Json(json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/list",
                "params": { "_meta": ego_meta() }
            })),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(over_http.into_body(), usize::MAX)
            .await
            .unwrap();
        let over_http: Value = serde_json::from_slice(&body).unwrap();
        assert!(!names(&over_acp).is_empty(), "{over_acp}");
        assert_eq!(names(&over_acp), names(&over_http["result"]));

        let peers = host
            .message(
                &id,
                "tools/call".to_owned(),
                object(json!({ "name": "agent", "arguments": { "action": "list_peers" } })),
            )
            .await
            .expect("tools/call over ACP");
        assert!(
            peers.to_string().contains(PEER),
            "a tool call runs as ego's peer: {peers}"
        );
    }

    #[tokio::test]
    async fn an_unknown_method_is_an_mcp_error_not_a_result() {
        let state = test_state();
        let host = AcpMcpHost::new(&state);
        let (notify, _) = listener();
        let id = host.connect(Some(PEER), notify).expect("mcp/connect");
        let error = host
            .message(&id, "tools/boom".to_owned(), None)
            .await
            .expect_err("no such method");
        assert_eq!(error.code, -32601);
    }

    /// What GET `/mcp` would have streamed reaches ego as notifications.
    #[tokio::test]
    async fn server_notifications_reach_ego_on_its_connection() {
        let state = test_state();
        let host = AcpMcpHost::new(&state);
        let (notify, heard) = listener();
        let id = host.connect(Some(PEER), notify).expect("mcp/connect");

        let _ = state.mcp.tools_changed.send(());
        eventually("tools/list_changed", || !heard.lock().is_empty()).await;
        assert_eq!(heard.lock()[0].0, "notifications/tools/list_changed");

        let channel = state
            .session_maps
            .messaging_channels
            .get(&id)
            .map(|sender| sender.value().clone())
            .expect("the connection has a delivery channel, as an SSE stream would");
        let _ = channel.send(
            json!({ "jsonrpc": "2.0", "method": "notifications/claude/channel", "params": { "content": "hi" } })
                .to_string(),
        );
        eventually("the channel notification", || heard.lock().len() == 2).await;
        let (method, params) = heard.lock()[1].clone();
        assert_eq!(method, "notifications/claude/channel");
        assert_eq!(
            params.and_then(|p| p.get("content").cloned()),
            Some(json!("hi"))
        );
    }

    /// Nothing the connection opened survives it.
    #[tokio::test]
    async fn disconnecting_releases_the_protocol_session_and_the_identity() {
        let state = test_state();
        let host = AcpMcpHost::new(&state);
        let (notify, heard) = listener();
        let id = host.connect(Some(PEER), notify).expect("mcp/connect");
        assert!(state.mcp.sessions.contains_key(&id));

        host.disconnect(&id);

        assert!(!state.mcp.sessions.contains_key(&id), "protocol session");
        assert!(!state.mcp.to_session.contains_key(&id), "identity route");
        assert!(
            !state.peer_agents.contains_key(PEER),
            "peer with no transport left"
        );
        assert!(
            !state.session_maps.messaging_channels.contains_key(&id),
            "delivery channel"
        );
        let _ = state.mcp.tools_changed.send(());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            heard.lock().is_empty(),
            "a released connection hears nothing"
        );
    }
}
