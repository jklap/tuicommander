//! Dedicated per-id WebSocket bridges for the high-frequency AI token streams —
//! event-bridge plan Steps 3 (conversation) & 4 (chat). These deliberately do
//! NOT ride the global `event_bus`/SSE: a single conversation emits 20+ events/sec,
//! which would exhaust the 256-cap broadcast and Lag unrelated SSE consumers.
//! Each connection taps the same engine stream the desktop Tauri Channel uses, so
//! browser/PWA clients get byte-identical `ConversationEvent`/`ChatEvent` frames.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::Response;
use futures_util::stream::{SplitSink, SplitStream, StreamExt};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::ai_agent::conversation_engine::{
    ConversationEvent, batched_conversation_stream, build_config,
    start_conversation as engine_start, subscribe_conversation,
};
use crate::ai_chat_registry::{ChatEvent, chat_registry, validate_id};

/// Serialize a frame to JSON and push it down the socket. Returns false if the
/// client has disconnected. Shared by the conversation and chat bridges.
async fn send_json<T: Serialize>(sink: &mut SplitSink<WebSocket, Message>, frame: &T) -> bool {
    let Ok(json) = serde_json::to_string(frame) else {
        // Skip an unserializable frame but keep the stream alive — log it so a
        // silent gap in the client's stream is at least visible server-side.
        tracing::warn!("ai_stream: failed to serialize frame, skipping");
        return true;
    };
    futures_util::SinkExt::send(sink, Message::Text(json.into()))
        .await
        .is_ok()
}

/// Forward frames from `rx` to the socket until the producer stops or the client
/// goes away. Watching `stream` is what makes a *quiet* producer safe: a send
/// failure alone only surfaces on the next frame, which may never come, so the
/// subscription and the socket would be held forever. Shared by both bridges.
async fn forward_until_closed<T: Serialize>(
    sink: &mut SplitSink<WebSocket, Message>,
    stream: &mut SplitStream<WebSocket>,
    rx: &mut tokio::sync::mpsc::Receiver<T>,
) {
    loop {
        tokio::select! {
            maybe = rx.recv() => match maybe {
                Some(ev) => {
                    if !send_json(sink, &ev).await {
                        break;
                    }
                }
                None => break, // producer dropped the sender
            },
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(_)) => break,
                Some(Ok(_)) => {} // ignore other client→server frames
            },
        }
    }
}

/// Start params for a conversation stream — mirrors the `start_conversation`
/// Tauri command args (minus `sessionId`, which is in the path, and `onEvent`,
/// which is the WebSocket itself). Sent by the client as the first text frame.
#[derive(Deserialize)]
struct StartConversationParams {
    message: String,
    autonomy: Option<String>,
    #[serde(rename = "maxSteps")]
    max_steps: Option<usize>,
    temperature: Option<f32>,
    #[serde(rename = "modelOverride")]
    model_override: Option<String>,
    #[serde(rename = "bypassedTools")]
    bypassed_tools: Option<Vec<String>>,
    #[serde(rename = "reasoningEffort")]
    reasoning_effort: Option<String>,
}

/// `GET /ai/conversation/{session_id}/stream` — WebSocket upgrade.
pub(super) async fn conversation_ws(
    ws: WebSocketUpgrade,
    Path(session_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| bridge_conversation(socket, session_id, state))
}

async fn bridge_conversation(socket: WebSocket, session_id: String, state: Arc<AppState>) {
    let (mut sink, mut stream) = socket.split();

    // Validate the session id up front (mirrors `bridge_chat`'s `validate_id`).
    if let Err(e) = validate_id(&session_id) {
        let _ = send_json(&mut sink, &ConversationEvent::Error { message: e }).await;
        return;
    }

    // First frame carries the start params (atomic start+subscribe — no
    // POST-then-subscribe race where early TextChunks would be missed).
    let params: StartConversationParams = match stream.next().await {
        Some(Ok(Message::Text(t))) => match serde_json::from_str(&t) {
            Ok(p) => p,
            Err(e) => {
                let _ = send_json(
                    &mut sink,
                    &ConversationEvent::Error {
                        message: format!("invalid start params: {e}"),
                    },
                )
                .await;
                return;
            }
        },
        _ => return, // closed before sending params
    };

    let config = build_config(
        params.autonomy,
        params.max_steps,
        params.temperature,
        params.model_override,
        params.bypassed_tools,
        params.reasoning_effort,
    )
    .await;

    let rx = match engine_start(state, session_id.clone(), params.message, config).await {
        Ok(rx) => rx,
        // A conversation is already running on this session — re-attach to its
        // live stream instead of erroring. The reconnecting client keeps its own
        // transcript, so live events (no backfill) resume the stream. If the
        // conversation ended in the race between start and subscribe, surface the
        // original start error.
        Err(e) => match subscribe_conversation(&session_id) {
            Some(rx) => rx,
            None => {
                let _ = send_json(&mut sink, &ConversationEvent::Error { message: e }).await;
                return;
            }
        },
    };

    // Same 50ms batcher the desktop Channel bridge uses. We stop forwarding on a
    // client disconnect but leave the conversation running (matches desktop:
    // closing the panel doesn't cancel — use the explicit cancel endpoint).
    // Dropping `batched` closes the batcher's mpsc, which the batcher observes via
    // `tx.closed()` and exits, releasing the engine's broadcast subscription.
    let mut batched = batched_conversation_stream(rx);
    forward_until_closed(&mut sink, &mut stream, &mut batched).await;
}

// ── Chat registry stream (event-bridge plan Step 4) ────────────────────

/// `GET /ai/chat/{chat_id}/stream` — WebSocket upgrade. Mirrors the desktop
/// `chat_subscribe` command: the first frame is a `ChatEvent::Snapshot`, then
/// live `ChatEvent`s (chunk/error/cleared/snapshot) as they are fanned out.
/// Closing the socket unsubscribes (no explicit `chat_unsubscribe` needed).
pub(super) async fn chat_ws(ws: WebSocketUpgrade, Path(chat_id): Path<String>) -> Response {
    ws.on_upgrade(move |socket| bridge_chat(socket, chat_id))
}

async fn bridge_chat(socket: WebSocket, chat_id: String) {
    let (mut sink, mut stream) = socket.split();

    if let Err(e) = validate_id(&chat_id) {
        let _ = send_json(&mut sink, &ChatEvent::Error { message: e }).await;
        return;
    }

    let reg = chat_registry();
    let (result, mut rx) = reg.subscribe_ws(&chat_id).await;

    // First frame: the state snapshot (carries `kind: "snapshot"` so a client can
    // handle it like any other event). There is no client: the registry has no
    // producer, so this snapshot is always the empty default and no event ever
    // follows it — the frontend consumer was removed in story `600-d664` because
    // applying it wiped the history loaded from disk.
    if !send_json(&mut sink, &ChatEvent::Snapshot(result.snapshot)).await {
        reg.unsubscribe(&chat_id, result.subscription_id).await;
        return;
    }

    forward_until_closed(&mut sink, &mut stream, &mut rx).await;

    reg.unsubscribe(&chat_id, result.subscription_id).await;
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_agent::conversation_engine::{ACTIVE_CONVERSATIONS, ConversationHandle};
    use crate::ai_agent::engine::AgentState;
    use futures_util::SinkExt as _;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;
    use tokio::sync::{Notify, broadcast};

    /// Register a fake active conversation so `bridge_conversation` takes the
    /// re-attach path (`engine_start` reports "already active") and subscribes to
    /// a broadcast sender the test owns — no LLM call, no engine task, and above
    /// all no events, which is exactly the "quiet conversation" case.
    fn register_quiet_conversation(session_id: &str) -> broadcast::Sender<ConversationEvent> {
        let (event_tx, _) = broadcast::channel(256);
        ACTIVE_CONVERSATIONS.insert(
            session_id.to_string(),
            ConversationHandle {
                cancel: Arc::new(AtomicBool::new(false)),
                state: Arc::new(parking_lot::RwLock::new(AgentState::Running)),
                pause_notify: Arc::new(Notify::new()),
                event_tx: event_tx.clone(),
                approval_tx: Arc::new(parking_lot::Mutex::new(None)),
            },
        );
        event_tx
    }

    /// Poll the broadcast subscriber count for up to 2s.
    async fn wait_for_receivers(tx: &broadcast::Sender<ConversationEvent>, want: usize) -> bool {
        for _ in 0..200 {
            if tx.receiver_count() == want {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }

    async fn serve_conversation_ws() -> u16 {
        let app = axum::Router::new()
            .route(
                "/ai/conversation/{session_id}/stream",
                axum::routing::get(conversation_ws),
            )
            .with_state(super::super::tests::test_state());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        port
    }

    type TestWsClient = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    /// Open the bridge on a silent conversation and hand back the live client.
    async fn connect_to_quiet_conversation(
        session_id: &str,
        event_tx: &broadcast::Sender<ConversationEvent>,
    ) -> TestWsClient {
        let port = serve_conversation_ws().await;
        let url = format!("ws://127.0.0.1:{port}/ai/conversation/{session_id}/stream");
        let (mut client, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        client
            .send(tokio_tungstenite::tungstenite::Message::Text(
                r#"{"message":"hi","reasoningEffort":"high"}"#.into(),
            ))
            .await
            .unwrap();
        assert!(
            wait_for_receivers(event_tx, 1).await,
            "bridge never subscribed to the conversation stream"
        );
        client
    }

    #[tokio::test]
    async fn conversation_ws_reacts_to_close_while_conversation_is_silent() {
        let session_id = "ai-stream-close-while-silent";
        let event_tx = register_quiet_conversation(session_id);
        let mut client = connect_to_quiet_conversation(session_id, &event_tx).await;

        // A Close frame is the only thing the client ever sends after the start
        // params. Without a select! on the socket the bridge sits in `recv()` and
        // never polls the stream, so the connection is never torn down.
        client
            .send(tokio_tungstenite::tungstenite::Message::Close(None))
            .await
            .unwrap();

        let torn_down = tokio::time::timeout(Duration::from_secs(2), async {
            while let Some(msg) = client.next().await {
                if msg.is_err()
                    || matches!(msg, Ok(tokio_tungstenite::tungstenite::Message::Close(_)))
                {
                    break;
                }
            }
        })
        .await;
        ACTIVE_CONVERSATIONS.remove(session_id);
        assert!(
            torn_down.is_ok(),
            "the bridge must notice a client Close on a silent conversation and drop \
             the socket instead of parking on the event stream"
        );
    }

    /// End-to-end proof that a closed socket releases the *engine* subscription,
    /// not just the bridge's own handles: the `broadcast::Receiver` lives in the
    /// task spawned by `batched_conversation_stream`, which learns the consumer is
    /// gone via `tx.closed()`. Without that the count stays pinned at 1 forever on
    /// a conversation that never emits another event.
    #[tokio::test]
    async fn conversation_ws_close_releases_subscription() {
        let session_id = "ai-stream-close-releases-subscription";
        let event_tx = register_quiet_conversation(session_id);
        let mut client = connect_to_quiet_conversation(session_id, &event_tx).await;

        // The client goes away while the conversation stays silent.
        client.close(None).await.unwrap();
        drop(client);

        let released = wait_for_receivers(&event_tx, 0).await;
        ACTIVE_CONVERSATIONS.remove(session_id);
        assert!(
            released,
            "closing the socket must release the conversation subscription; a quiet \
             conversation would otherwise hold it until an event that may never come"
        );
    }
}
