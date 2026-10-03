//! Authenticated star-topology mail. The desktop opens the only duplex link;
//! daemons keep local delivery independent of that link. No process-control
//! actions cross this protocol.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

use crate::AppState;

/// Bound outstanding calls and frame size independently of the PTY buffers.
const MAX_CALLS: usize = 64;
// One 64 KiB message may expand sixfold when JSON escapes control bytes.
const MAX_FRAME: usize = 512 * 1024;
/// Three missed 15-second pongs mean the authenticated connection is gone.
const HEARTBEAT: Duration = Duration::from_secs(15);
const LINK_IDLE: Duration = Duration::from_secs(45);
/// Network give-up budget, separate from native agent wait's own deadline.
const CALL_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Default)]
pub(crate) struct RemoteMail {
    connections: DashMap<String, Arc<Link>>,
    hub: Mutex<Option<(String, Arc<Link>)>>,
    // Role transitions are short; network handshakes serialize only per host.
    role_lock: Mutex<()>,
    connect_locks: Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>,
    forwarded_history: Mutex<ForwardedHistory>,
    own_host: Mutex<Option<String>>,
    notice_notify: Arc<tokio::sync::Notify>,
    notice_started: AtomicBool,
    supervisors: DashMap<String, tokio::task::AbortHandle>,
}

/// Keep deduplication independent of inbox reads. History matches the bounded
/// 100-message outbox horizon and caps recipient count as well as per-peer entries.
#[derive(Default)]
struct ForwardedHistory {
    recipients: HashMap<String, VecDeque<(String, [u8; 32])>>,
    order: VecDeque<String>,
}

/// Called under the native identity/enqueue lock. Only fingerprints are retained,
/// never 64-KiB message bodies (at most 1024 * 100 compact records).
pub(super) fn record_forwarded(
    state: &AppState,
    recipient: &str,
    message: &crate::state::AgentMessage,
) -> Result<bool, &'static str> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update((message.from_tuic_session.len() as u64).to_le_bytes());
    hasher.update(message.from_tuic_session.as_bytes());
    hasher.update(message.content.as_bytes());
    let fingerprint: [u8; 32] = hasher.finalize().into();
    let mut history = state.remote_mail.forwarded_history.lock();
    if let Some(records) = history.recipients.get(recipient)
        && let Some((_, previous)) = records.iter().find(|(id, _)| id == &message.id)
    {
        return if *previous == fingerprint {
            Ok(false)
        } else {
            Err("Forwarded message identity collision")
        };
    }
    history.order.retain(|id| id != recipient);
    if !history.recipients.contains_key(recipient)
        && history.recipients.len() >= 1024
        && let Some(oldest) = history.order.pop_front()
    {
        history.recipients.remove(&oldest);
    }
    history.order.push_back(recipient.to_string());
    let records = history.recipients.entry(recipient.to_string()).or_default();
    if records.len() >= 100 {
        records.pop_front();
    }
    records.push_back((message.id.clone(), fingerprint));
    Ok(true)
}

struct Link {
    outbound: mpsc::Sender<String>,
    pending: DashMap<String, oneshot::Sender<Value>>,
    slots: tokio::sync::Semaphore,
    shutdown: tokio::sync::Notify,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Sender {
    pub host: String,
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Frame {
    Call {
        id: String,
        sender: Option<Sender>,
        arguments: Value,
        message_id: Option<String>,
    },
    Reply {
        id: String,
        result: Value,
    },
}

#[derive(Deserialize)]
pub(super) struct PeerQuery {
    connection_id: String,
}

fn error(connection: &str, detail: impl std::fmt::Display) -> Value {
    json!({"error": format!("Remote connection '{connection}': {detail}"), "connection_id": connection})
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 128
        && host != "local"
        && host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

fn allowed(args: &Value) -> bool {
    matches!(
        args["action"].as_str(),
        Some("register" | "list_peers" | "send" | "inbox" | "wait")
    )
}

impl Link {
    async fn call(&self, sender: Option<Sender>, arguments: Value) -> Value {
        self.call_with_id(sender, arguments, None).await
    }

    async fn call_with_id(
        &self,
        sender: Option<Sender>,
        arguments: Value,
        message_id: Option<String>,
    ) -> Value {
        let Ok(_slot) = self.slots.try_acquire() else {
            return json!({"error": "Remote mail link has too many outstanding calls"});
        };
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.insert(id.clone(), tx);
        let frame = Frame::Call {
            id: id.clone(),
            sender,
            arguments: arguments.clone(),
            message_id,
        };
        let budget = if arguments["action"] == "wait" {
            Duration::from_millis(
                arguments["timeout_ms"]
                    .as_u64()
                    .unwrap_or(60_000)
                    .min(300_000)
                    + 20_000,
            )
        } else {
            CALL_TIMEOUT
        };
        let result = tokio::time::timeout(budget, async {
            let text = serde_json::to_string(&frame).map_err(|e| e.to_string())?;
            if text.len() > MAX_FRAME {
                return Err("Remote mail frame exceeds size limit".into());
            }
            self.outbound
                .send(text)
                .await
                .map_err(|_| "Remote mail link is disconnected".to_string())?;
            rx.await
                .map_err(|_| "Remote mail link closed before acknowledgement".to_string())
        })
        .await;
        self.pending.remove(&id);
        match result {
            Ok(Ok(value)) => value,
            Ok(Err(detail)) => json!({"error": detail}),
            Err(_) => {
                json!({"error": "Remote mail acknowledgement timed out; delivery is uncertain, do not resend blindly"})
            }
        }
    }
}

/// Require the real daemon token even on loopback or with LAN auth bypass.
pub(super) async fn endpoint(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PeerQuery>,
    uri: Uri,
    ws: WebSocketUpgrade,
) -> Response {
    let token = state.session_token.read().clone();
    if token.is_empty() || !super::auth::has_valid_token_query(&uri, &token) {
        return (
            StatusCode::UNAUTHORIZED,
            axum::Json(json!({"error": "Peer mail requires the daemon connection token"})),
        )
            .into_response();
    }
    if !valid_host(&query.connection_id) {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({"error": "Invalid peer connection qualifier"})),
        )
            .into_response();
    }
    if state
        .remote
        .snapshot()
        .iter()
        .any(|connection| connection.base_url.is_some())
    {
        return (
            StatusCode::CONFLICT,
            axum::Json(json!({"error":"A mail hub cannot also become a daemon spoke"})),
        )
            .into_response();
    }
    if state.remote_mail.hub.lock().is_some() {
        return (
            StatusCode::CONFLICT,
            axum::Json(json!({"error": "This daemon already has a mail hub"})),
        )
            .into_response();
    }
    ws.max_message_size(MAX_FRAME)
        .on_upgrade(move |socket| async move {
            let (tx, rx) = mpsc::channel(MAX_CALLS);
            let link = Arc::new(Link {
                outbound: tx,
                pending: DashMap::new(),
                slots: tokio::sync::Semaphore::new(MAX_CALLS),
                shutdown: tokio::sync::Notify::new(),
            });
            // Recheck after upgrade: concurrent authenticated upgrades cannot replace
            // a live hub and strand its outstanding calls.
            {
                let _role_guard = state.remote_mail.role_lock.lock();
                if state
                    .remote
                    .snapshot()
                    .iter()
                    .any(|connection| connection.base_url.is_some())
                {
                    return;
                }
                let mut hub = state.remote_mail.hub.lock();
                if hub.is_some() {
                    return;
                }
                *hub = Some((query.connection_id.clone(), link.clone()));
                *state.remote_mail.own_host.lock() = Some(query.connection_id.clone());
            }
            start_notices(&state);
            state.remote_mail.notice_notify.notify_one();
            drive(
                socket,
                state.clone(),
                link.clone(),
                rx,
                Role::Daemon(query.connection_id),
            )
            .await;
            let mut hub = state.remote_mail.hub.lock();
            if hub
                .as_ref()
                .is_some_and(|(_, current)| Arc::ptr_eq(current, &link))
            {
                *hub = None;
            }
            drop(hub);
            cleanup_shadows(&state, None);
        })
        .into_response()
}

enum Role {
    Hub(String),
    Daemon(String),
}

trait MailFrame: Sized {
    fn text(value: String) -> Self;
    fn ping() -> Self;
    fn payload(&self) -> Option<&str>;
    fn closed(&self) -> bool;
}

impl MailFrame for Message {
    fn text(value: String) -> Self {
        Self::Text(value.into())
    }
    fn ping() -> Self {
        Self::Ping(Vec::new().into())
    }
    fn payload(&self) -> Option<&str> {
        if let Self::Text(text) = self {
            Some(text.as_str())
        } else {
            None
        }
    }
    fn closed(&self) -> bool {
        matches!(self, Self::Close(_))
    }
}

impl MailFrame for tokio_tungstenite::tungstenite::Message {
    fn text(value: String) -> Self {
        Self::Text(value.into())
    }
    fn ping() -> Self {
        Self::Ping(Vec::new().into())
    }
    fn payload(&self) -> Option<&str> {
        if let Self::Text(text) = self {
            Some(text.as_str())
        } else {
            None
        }
    }
    fn closed(&self) -> bool {
        matches!(self, Self::Close(_))
    }
}

async fn drive<S, M, E>(
    mut socket: S,
    state: Arc<AppState>,
    link: Arc<Link>,
    mut rx: mpsc::Receiver<String>,
    role: Role,
) where
    S: Stream<Item = Result<M, E>> + Sink<M> + Unpin,
    M: MailFrame,
    E: std::fmt::Display,
{
    let mut heartbeat = tokio::time::interval(HEARTBEAT);
    let mut last_frame = tokio::time::Instant::now();
    let mut calls = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            received = socket.next() => {
                let Some(Ok(message)) = received else { break; };
                last_frame = tokio::time::Instant::now();
                if message.closed() { break; }
                let Some(text) = message.payload() else { continue; };
                if text.len() > MAX_FRAME { break; }
                let frame = match serde_json::from_str::<Frame>(text) {
                    Ok(frame) => frame,
                    Err(_) => break,
                };
                match frame {
                    Frame::Reply { id, result } => {
                        if let Some((_, response)) = link.pending.remove(&id) { let _ = response.send(result); }
                    }
                    Frame::Call { id, sender, arguments, message_id } => {
                        if calls.len() >= MAX_CALLS { break; }
                        let request = process(state.clone(), &role, sender, arguments, message_id);
                        let tx = link.outbound.clone();
                        calls.spawn(async move {
                            let result = request.await;
                            match serde_json::to_string(&Frame::Reply { id: id.clone(), result }) {
                                Ok(text) if text.len() <= MAX_FRAME => { let _ = tx.send(text).await; }
                                _ => {
                                    let fallback = Frame::Reply { id, result: json!({"error":"Peer reply exceeds 512 KiB; read the inbox with a smaller limit"}) };
                                    if let Ok(text) = serde_json::to_string(&fallback) { let _ = tx.send(text).await; }
                                }
                            }
                        });
                    }
                }
            }
            outbound = rx.recv() => {
                let Some(text) = outbound else { break; };
                if !matches!(tokio::time::timeout(HEARTBEAT, socket.send(M::text(text))).await, Ok(Ok(()))) { break; }
            }
            _ = heartbeat.tick() => {
                if last_frame.elapsed() >= LINK_IDLE || !matches!(tokio::time::timeout(HEARTBEAT, socket.send(M::ping())).await, Ok(Ok(()))) { break; }
            }
            _ = calls.join_next(), if !calls.is_empty() => {}
            _ = link.shutdown.notified() => break,
        }
    }
    calls.abort_all();
    link.pending.clear();
    tracing::info!(source = "remote_mail", "Peer mail link closed");
}

// The boxed future breaks the duplex dispatch/connect task's recursive Send
// type while keeping each request owned by its link's JoinSet.
fn process(
    state: Arc<AppState>,
    role: &Role,
    mut sender: Option<Sender>,
    arguments: Value,
    message_id: Option<String>,
) -> futures_util::future::BoxFuture<'static, Value> {
    let (is_hub, host) = match role {
        Role::Hub(id) => (true, id.clone()),
        Role::Daemon(id) => (false, id.clone()),
    };
    Box::pin(async move {
        if !allowed(&arguments) {
            return json!({"error": "Peer mail endpoint permits register/list_peers/send/inbox/wait only"});
        }
        if message_id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 160)
        {
            return json!({"error":"Invalid forwarded message identity"});
        }
        if let Some(sender) = sender.as_mut() {
            if sender.id.is_empty()
                || sender.id.len() > 128
                || sender.id.contains('/')
                || sender.name.len() > 256
            {
                return json!({"error": "Invalid peer sender identity"});
            }
            if is_hub {
                // The connection, never the remote body, owns sender provenance.
                sender.host = host.clone();
            } else if sender.host == host || (sender.host != "local" && !valid_host(&sender.host)) {
                return json!({"error": "The hub cannot impersonate a daemon-local peer"});
            }
        }
        if is_hub {
            // A daemon may send as its own peer or discover the hub's directory;
            // it may not wait/read/register as a peer on another host.
            match arguments["action"].as_str() {
                Some("send") => route_send(&state, sender, arguments, message_id).await,
                Some("list_peers") => directory(&state, arguments).await,
                _ => json!({"error": "Daemon-to-hub calls permit send/list_peers only"}),
            }
        } else {
            native_call(&state, sender, arguments, message_id).await
        }
    })
}

async fn connection(state: &Arc<AppState>, id: &str) -> Result<Arc<Link>, Value> {
    // Weak locks disappear after the handshake callers finish, so unknown
    // host probes cannot accumulate permanent lock entries.
    let host_lock = {
        let mut locks = state.remote_mail.connect_locks.lock();
        locks.retain(|_, lock| lock.strong_count() > 0);
        let slot = locks.entry(id.to_string()).or_default();
        if let Some(lock) = slot.upgrade() {
            lock
        } else {
            let lock = Arc::new(tokio::sync::Mutex::new(()));
            *slot = Arc::downgrade(&lock);
            lock
        }
    };
    let _guard = host_lock.lock().await;
    {
        let _role_guard = state.remote_mail.role_lock.lock();
        if state.remote_mail.own_host.lock().is_some() {
            return Err(error(
                id,
                "a daemon spoke routes remote mail through its desktop hub",
            ));
        }
    }
    if let Some(link) = state.remote_mail.connections.get(id)
        && !link.outbound.is_closed()
    {
        return Ok(link.clone());
    }
    let base = state
        .remote
        .base_url(id)
        .ok_or_else(|| error(id, "connection is unavailable"))?;
    let token = state
        .remote
        .token(id)
        .ok_or_else(|| error(id, "connection has no authenticated daemon token"))?;
    let mut url = reqwest::Url::parse(&base).map_err(|e| error(id, e))?;
    let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
    url.set_scheme(scheme)
        .map_err(|_| error(id, "unsupported connection URL scheme"))?;
    url.set_path("/mcp/peer");
    url.set_query(None);
    url.query_pairs_mut()
        .append_pair("token", &token)
        .append_pair("connection_id", id);
    let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(MAX_FRAME))
        .max_frame_size(Some(MAX_FRAME));
    let (socket, _) = match tokio::time::timeout(
        CALL_TIMEOUT,
        tokio_tungstenite::connect_async_with_config(url.as_str(), Some(config), false),
    )
    .await
    {
        Ok(Ok(connected)) => connected,
        Ok(Err(_)) => {
            return Err(error(
                id,
                "authenticated peer mail link could not open; the daemon must support /mcp/peer",
            ));
        }
        Err(_) => return Err(error(id, "peer mail link handshake timed out")),
    };
    let (tx, rx) = mpsc::channel(MAX_CALLS);
    let link = Arc::new(Link {
        outbound: tx,
        pending: DashMap::new(),
        slots: tokio::sync::Semaphore::new(MAX_CALLS),
        shutdown: tokio::sync::Notify::new(),
    });
    {
        let _role_guard = state.remote_mail.role_lock.lock();
        if state.remote_mail.own_host.lock().is_some()
            || state.remote.base_url(id).as_deref() != Some(base.as_str())
            || state.remote.token(id).as_deref() != Some(token.as_str())
        {
            return Err(error(id, "connection changed during peer handshake"));
        }
        state
            .remote_mail
            .connections
            .insert(id.to_string(), link.clone());
    }
    start_notices(state);
    state.remote_mail.notice_notify.notify_one();
    let task_state = state.clone();
    let task_link = link.clone();
    let id = id.to_string();
    let task_host_lock = host_lock.clone();
    tokio::spawn(async move {
        drive(
            socket,
            task_state.clone(),
            task_link.clone(),
            rx,
            Role::Hub(id.clone()),
        )
        .await;
        let _guard = task_host_lock.lock().await;
        if task_state
            .remote_mail
            .connections
            .remove_if(&id, |_, current| Arc::ptr_eq(current, &task_link))
            .is_some()
        {
            cleanup_shadows(&task_state, Some(&id));
        }
    });
    Ok(link)
}

fn qualify_rows(value: &mut Value, host: &str) {
    if let Some(rows) = value["peers"].as_array_mut() {
        rows.retain(|row| {
            row["tuic_session"]
                .as_str()
                .is_some_and(|id| !id.contains('/'))
        });
        for row in rows {
            if let Some(id) = row["tuic_session"].as_str() {
                row["address"] = json!(format!("{host}/{id}"));
                row["connection_id"] = json!(host);
            }
        }
    }
}

async fn directory(state: &Arc<AppState>, args: Value) -> Value {
    let mut local = super::mcp_transport::local_peer_call(state, &args, None).await;
    qualify_rows(&mut local, "local");
    let mut peers = local["peers"].as_array().cloned().unwrap_or_default();
    let ids: Vec<_> = if let Some(id) = args["connection_id"].as_str() {
        vec![id.to_string()]
    } else {
        state
            .remote
            .snapshot()
            .into_iter()
            .map(|status| status.id)
            .collect()
    };
    let mut failures = Vec::new();
    if args["connection_id"] == "local" {
        return json!({"peers": peers});
    }
    if args.get("connection_id").is_some() {
        peers.clear();
    }
    for id in ids {
        let link = match connection(state, &id).await {
            Ok(link) => link,
            Err(detail) => {
                failures.push(detail);
                continue;
            }
        };
        let mut remote = link
            .call(
                None,
                json!({"action":"list_peers", "path":args.get("path")}),
            )
            .await;
        qualify_rows(&mut remote, &id);
        if let Some(rows) = remote["peers"].as_array() {
            peers.extend(rows.iter().cloned());
        } else {
            failures.push(error(&id, remote));
        }
    }
    if args.get("connection_id").is_some() && !failures.is_empty() {
        return failures.remove(0);
    }
    let mut result = json!({"peers":peers});
    if !failures.is_empty() {
        result["connection_errors"] = json!(failures);
    }
    result
}

struct SenderBinding {
    state: Arc<AppState>,
    sid: Option<String>,
}

impl Drop for SenderBinding {
    fn drop(&mut self) {
        if let Some(sid) = self.sid.as_deref() {
            self.state.mcp.to_session.remove(sid);
        }
    }
}

async fn native_call(
    state: &Arc<AppState>,
    sender: Option<Sender>,
    mut args: Value,
    message_id: Option<String>,
) -> Value {
    let sid = if let Some(sender) = sender {
        let identity = format!("{}/{}", sender.host, sender.id);
        let sid = format!("remote-mail:{}:{identity}", uuid::Uuid::new_v4());
        if state.peer_agents.len() >= 1024 && !state.peer_agents.contains_key(&identity) {
            return json!({"error":"Too many remote peer identities"});
        }
        // Qualified shadow peers can never bind or impersonate a local PTY.
        state
            .peer_agents
            .entry(identity.clone())
            .or_insert_with(|| crate::state::PeerAgent {
                tuic_session: identity.clone(),
                mcp_session_id: sid.clone(),
                name: sender.name,
                project: None,
                registered_at: 0,
            });
        state.mcp.to_session.insert(sid.clone(), identity);
        Some(sid)
    } else {
        None
    };
    let binding = SenderBinding {
        state: state.clone(),
        sid,
    };
    let sid = binding.sid.as_deref();
    if args["action"] == "register" {
        let identity = sid
            .as_deref()
            .and_then(|sid| state.mcp.to_session.get(sid).map(|p| p.value().clone()));
        if let Some(sid) = sid.as_deref() {
            state.mcp.to_session.remove(sid);
        }
        return json!({"tuic_session":identity});
    }
    if args["action"] != "list_peers" && sid.is_none() {
        return json!({"error":"Peer mail requires a bound sender identity"});
    }
    if let Some(object) = args.as_object_mut() {
        object.remove("connection_id");
    }
    let result = super::mcp_transport::local_peer_call_with_message_id(
        state,
        &args,
        sid.as_deref(),
        message_id,
    )
    .await;
    if let Some(sid) = sid.as_deref() {
        state.mcp.to_session.remove(sid);
    }
    result
}

async fn route_send(
    state: &Arc<AppState>,
    sender: Option<Sender>,
    mut args: Value,
    message_id: Option<String>,
) -> Value {
    let Some(sender) = sender else {
        return json!({"error":"Register before sending cross-host mail"});
    };
    let Some(address) = args["to"].as_str() else {
        return json!({"error":"Peer send requires to"});
    };
    let Some((host, recipient)) = address.split_once('/') else {
        return json!({"error":"Cross-host mail requires connection-qualified recipient: connection/id"});
    };
    if recipient.is_empty() || recipient.contains('/') || (host != "local" && !valid_host(host)) {
        return json!({"error":"Invalid connection-qualified recipient"});
    }
    let host = host.to_string();
    args["to"] = json!(recipient);
    let mut result = if host == "local" {
        native_call(state, Some(sender), args, message_id).await
    } else {
        match connection(state, &host).await {
            Ok(link) => link.call_with_id(Some(sender), args, message_id).await,
            Err(detail) => return detail,
        }
    };
    result["connection_id"] = json!(host);
    result
}

/// Called only after the public MCP dispatcher enforces its loopback boundary.
pub(super) async fn dispatch(
    state: &Arc<AppState>,
    args: &Value,
    sid: Option<&str>,
) -> Option<Value> {
    let hub = state.remote_mail.hub.lock().clone();
    let own_host = state.remote_mail.own_host.lock().clone();
    let action = args["action"].as_str()?;
    if action == "list_peers" {
        return Some(if let Some((_, link)) = hub {
            link.call(None, args.clone()).await
        } else if let Some(host) = own_host {
            if args["connection_id"]
                .as_str()
                .is_some_and(|requested| requested != host)
            {
                return Some(error(
                    &host,
                    "mail hub is disconnected; only daemon-local peers are available",
                ));
            }
            let mut rows = super::mcp_transport::local_peer_call(state, args, sid).await;
            qualify_rows(&mut rows, &host);
            rows
        } else {
            directory(state, args.clone()).await
        });
    }
    if action != "send" {
        return None;
    }
    let to = args["to"].as_str()?;
    let qualified = if let Some(host) = args["connection_id"].as_str() {
        if let Some((qualified_host, _)) = to.split_once('/') {
            if qualified_host != host {
                return Some(json!({"error":"connection_id conflicts with qualified recipient"}));
            }
            to.to_string()
        } else {
            format!("{host}/{to}")
        }
    } else if to.contains('/') {
        to.to_string()
    } else {
        return None;
    };
    let identity = sid.and_then(|sid| state.mcp.to_session.get(sid).map(|id| id.value().clone()));
    let sender = identity.and_then(|identity| {
        state.peer_agents.get(&identity).map(|peer| Sender {
            host: own_host.clone().unwrap_or_else(|| "local".into()),
            id: identity,
            name: peer.name.clone(),
        })
    });
    let mut args = args.clone();
    args["to"] = json!(qualified);
    if let Some(own_host) = own_host {
        if let Some(target) = qualified.strip_prefix(&format!("{own_host}/")) {
            args["to"] = json!(target);
            return Some(super::mcp_transport::local_peer_call(state, &args, sid).await);
        }
        Some(if let Some((_, link)) = hub {
            link.call(sender, args).await
        } else {
            error(
                &own_host,
                "mail hub is disconnected; intra-host mail remains available",
            )
        })
    } else {
        Some(route_send(state, sender, args, None).await)
    }
}

fn cleanup_shadows(state: &AppState, host: Option<&str>) {
    let identities: Vec<_> = state
        .peer_agents
        .iter()
        .filter(|peer| {
            peer.value().mcp_session_id.starts_with("remote-mail:")
                && host.is_none_or(|host| peer.key().starts_with(&format!("{host}/")))
        })
        .map(|peer| peer.key().clone())
        .collect();
    for identity in identities {
        state.peer_agents.remove(&identity);
        // Do not drop retained cross-host lifecycle mail: it is the outbox
        // until the owning machine acknowledges its durable inbox copy.
    }
}

/// Retire the mail link when its configured connection is disconnected.
pub(crate) fn disconnect(state: &AppState, host: &str) {
    if let Some((_, supervisor)) = state.remote_mail.supervisors.remove(host) {
        supervisor.abort();
    }
    if let Some((_, link)) = state.remote_mail.connections.remove(host) {
        link.shutdown.notify_one();
    }
}

/// Open the reverse path when the configured connection becomes ready, so a
/// daemon can mail the hub before the desktop makes its first mail call.
pub(crate) fn connect_configured(state: &Arc<AppState>, host: String) {
    if let Some((_, previous)) = state.remote_mail.supervisors.remove(&host) {
        previous.abort();
    }
    let task_state = state.clone();
    let task_host = host.clone();
    let handle = tokio::spawn(async move {
        let mut warned = false;
        while task_state.remote.base_url(&task_host).is_some() {
            match connection(&task_state, &task_host).await {
                Ok(link) => {
                    warned = false;
                    link.outbound.closed().await;
                }
                Err(detail) if !warned => {
                    warned = true;
                    tracing::warn!(
                        source = "remote_mail",
                        connection = task_host,
                        "Peer mail unavailable: {detail}"
                    );
                }
                Err(_) => {}
            }
            // Retry the existing connection only; this never deploys or starts
            // a daemon and never substitutes SSH for the configured transport.
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
    state
        .remote_mail
        .supervisors
        .insert(host, handle.abort_handle());
}

/// Lifecycle producers already write one durable bounded inbox. Qualified
/// recipients use that same FIFO as an outbox; reconnect resumes delivery.
pub(crate) fn notice_stored(state: &AppState, recipient: &str) {
    if recipient.contains('/') {
        state.remote_mail.notice_notify.notify_one();
    }
}

fn start_notices(state: &Arc<AppState>) {
    if state
        .remote_mail
        .notice_started
        .swap(true, Ordering::AcqRel)
    {
        return;
    }
    let weak = Arc::downgrade(state);
    let notify = state.remote_mail.notice_notify.clone();
    tokio::spawn(async move {
        loop {
            notify.notified().await;
            let Some(state) = weak.upgrade() else {
                break;
            };
            let messages: Vec<_> = state
                .agent_inbox
                .iter()
                .filter(|entry| entry.key().contains('/'))
                .flat_map(|entry| {
                    entry
                        .value()
                        .iter()
                        .cloned()
                        .map(|message| (entry.key().clone(), message))
                        .collect::<Vec<_>>()
                })
                .collect();
            for (recipient, message) in messages {
                let own_host = state.remote_mail.own_host.lock().clone();
                let sender = Sender {
                    host: own_host.clone().unwrap_or_else(|| "local".into()),
                    id: message.from_tuic_session.clone(),
                    name: message.from_name.clone(),
                };
                let args = json!({"action":"send", "to":recipient, "message":message.content});
                let hub = state.remote_mail.hub.lock().clone();
                let result = if let Some((_, link)) = hub {
                    link.call_with_id(Some(sender), args, Some(message.id.clone()))
                        .await
                } else if own_host.is_none() {
                    route_send(&state, Some(sender), args, Some(message.id.clone())).await
                } else {
                    continue;
                };
                if result.get("message_id").is_some() && result.get("error").is_none() {
                    if let Some(mut inbox) = state.agent_inbox.get_mut(&recipient) {
                        inbox.retain(|entry| entry.id != message.id);
                    }
                } else {
                    tracing::warn!(
                        source = "remote_mail",
                        recipient,
                        "Lifecycle mail remains in outbox: {result}"
                    );
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::super::tests::test_state;
    use super::*;

    async fn peer(state: &Arc<AppState>, name: &str) -> (String, String) {
        let sid = uuid::Uuid::new_v4().to_string();
        let result = super::super::mcp_transport::local_peer_call(
            state,
            &json!({"action":"register","name":name}),
            Some(&sid),
        )
        .await;
        (sid, result["tuic_session"].as_str().unwrap().to_string())
    }

    async fn daemon(
        hub: &Arc<AppState>,
        id: &str,
        state: Arc<AppState>,
    ) -> tokio::task::JoinHandle<()> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        hub.remote
            .force_connected_for_test(id, &url, Some(&state.session_token.read().clone()));
        tokio::spawn(async move {
            axum::serve(
                listener,
                super::super::build_remote_router(state)
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        })
    }

    // Catches: /mcp/peer bypasses authentication on loopback or grants arbitrary
    // process tools rather than the approved mail-only boundary.
    #[tokio::test]
    async fn remote_peer_requires_the_connection_token_and_rejects_spawn() {
        let hub = test_state();
        let remote = test_state();
        let server = daemon(&hub, "mint", remote.clone()).await;
        let url = hub.remote.base_url("mint").unwrap().replace("http:", "ws:");
        let rejected = tokio_tungstenite::connect_async(format!(
            "{url}/mcp/peer?connection_id=mint&token=wrong"
        ))
        .await;
        assert!(rejected.is_err(), "loopback must not bypass the peer token");
        let link = connection(&hub, "mint")
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        let rejected = link
            .call(None, json!({"action":"spawn","prompt":"do not spawn"}))
            .await;
        assert!(rejected["error"].as_str().unwrap().contains("only"));
        assert!(remote.session_maps.sessions.is_empty());
        let impersonation = link
            .call(
                Some(Sender {
                    host: "mint".into(),
                    id: "victim".into(),
                    name: "forged".into(),
                }),
                json!({"action":"inbox"}),
            )
            .await;
        assert!(
            impersonation["error"]
                .as_str()
                .unwrap()
                .contains("impersonate")
        );
        assert!(!remote.peer_agents.contains_key("victim"));
        disconnect(&hub, "mint");
        server.abort();
    }

    // Catches: remote peer enumeration remains local-only, remote sends do not
    // wake the owning daemon's waiter, or replies stop in a shadow inbox.
    #[tokio::test]
    async fn remote_peer_star_delivers_remote_to_remote_and_reply_to_the_mac() {
        let hub = test_state();
        let mint = test_state();
        let other = test_state();
        let (mac_sid, mac_id) = peer(&hub, "mac").await;
        let (mint_sid, mint_id) = peer(&mint, "mint-agent").await;
        let (other_sid, other_id) = peer(&other, "other-agent").await;
        let first = daemon(&hub, "mint", mint.clone()).await;
        let second = daemon(&hub, "other", other.clone()).await;
        let listed = dispatch(&hub, &json!({"action":"list_peers"}), Some(&mac_sid))
            .await
            .unwrap();
        assert!(
            listed["peers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|peer| peer["address"] == format!("mint/{mint_id}"))
        );
        assert!(
            listed["peers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|peer| peer["address"] == format!("other/{other_id}"))
        );
        let wait_args = json!({"action":"wait","timeout_ms":60_000});
        let mut remote_wait = Box::pin(super::super::mcp_transport::local_peer_call(
            &other,
            &wait_args,
            Some(&other_sid),
        ));
        assert!(matches!(
            futures_util::poll!(&mut remote_wait),
            std::task::Poll::Pending
        ));
        assert!(other.has_active_agent_waiter(&other_id));
        let sent = dispatch(&mint,&json!({"action":"send","to":format!("other/{other_id}"),"message":"across both spokes"}),Some(&mint_sid)).await.unwrap();
        assert_eq!(
            sent["delivered"], true,
            "owner must deliver through its real waiter: {sent}"
        );
        assert_eq!(sent["delivery_path"], "waiter_and_inbox");
        let received = remote_wait.await;
        assert_eq!(received["messages"][0]["content"], "across both spokes");
        assert_eq!(
            received["messages"][0]["from_tuic_session"],
            format!("mint/{mint_id}")
        );
        let mut mac_wait = Box::pin(super::super::mcp_transport::local_peer_call(
            &hub,
            &wait_args,
            Some(&mac_sid),
        ));
        assert!(matches!(
            futures_util::poll!(&mut mac_wait),
            std::task::Poll::Pending
        ));
        let reply = dispatch(
            &other,
            &json!({"action":"send","to":format!("local/{mac_id}"),"message":"reply to Mac"}),
            Some(&other_sid),
        )
        .await
        .unwrap();
        assert_eq!(
            reply["delivered"], true,
            "reply must reach the hub: {reply}"
        );
        let received = mac_wait.await;
        assert_eq!(
            received["messages"][0]["from_tuic_session"],
            format!("other/{other_id}")
        );
        assert_eq!(received["messages"][0]["content"], "reply to Mac");
        disconnect(&hub, "mint");
        disconnect(&hub, "other");
        first.abort();
        second.abort();
    }

    // Catches: reconnect retries a lifecycle notice and enqueues/wakes it twice.
    #[tokio::test]
    async fn remote_peer_retried_lifecycle_identity_does_not_duplicate_the_inbox() {
        let remote = test_state();
        let (_, recipient) = peer(&remote, "recipient").await;
        let sender = Sender {
            host: "local".into(),
            id: "reporter".into(),
            name: "reporter".into(),
        };
        let arguments = json!({"action":"send", "to":recipient, "message":"child completed"});
        let first = native_call(
            &remote,
            Some(sender.clone()),
            arguments.clone(),
            Some("notice-1".into()),
        )
        .await;
        assert_eq!(first["message_id"], "notice-1", "{first}");
        let second = native_call(
            &remote,
            Some(sender.clone()),
            arguments.clone(),
            Some("notice-1".into()),
        )
        .await;
        assert_eq!(second["delivery_path"], "inbox_duplicate", "{second}");
        assert_eq!(remote.agent_inbox.get(&recipient).unwrap().len(), 1);
        let conflicting = native_call(
            &remote,
            Some(sender),
            json!({"action":"send", "to":recipient, "message":"different payload"}),
            Some("notice-1".into()),
        )
        .await;
        assert!(conflicting.get("error").is_some(), "{conflicting}");
        assert_eq!(remote.agent_inbox.get(&recipient).unwrap().len(), 1);
        assert!(
            !remote
                .mcp
                .to_session
                .iter()
                .any(|entry| entry.key().starts_with("remote-mail:"))
        );
    }

    // Catches: star routing disables local delivery when the hub is down, or a
    // qualified unavailable host degrades to the misleading not-registered error.
    #[tokio::test]
    async fn remote_peer_hub_down_keeps_local_mail_and_names_unavailable_connections() {
        let remote = test_state();
        let (sid, sender) = peer(&remote, "sender").await;
        let (_, recipient) = peer(&remote, "recipient").await;
        *remote.remote_mail.own_host.lock() = Some("mint".into());
        let local_args = json!({"action":"send","to":recipient,"message":"offline local"});
        assert!(dispatch(&remote, &local_args, Some(&sid)).await.is_none());
        let local =
            super::super::mcp_transport::local_peer_call(&remote, &local_args, Some(&sid)).await;
        assert!(
            local.get("message_id").is_some(),
            "native local mail must survive hub loss: {local}"
        );
        let messages = remote.agent_inbox.get(&recipient).unwrap();
        assert_eq!(messages[0].from_tuic_session, sender);
        drop(messages);
        let unavailable = dispatch(
            &remote,
            &json!({"action":"send","to":"other/peer","message":"offline remote"}),
            Some(&sid),
        )
        .await
        .unwrap();
        assert!(unavailable["error"].as_str().unwrap().contains("mint"));
        assert!(
            !unavailable["error"]
                .as_str()
                .unwrap()
                .contains("not registered")
        );
        let hub = test_state();
        let (sid, _) = peer(&hub, "mac").await;
        let missing = dispatch(
            &hub,
            &json!({"action":"send","to":"missing/peer","message":"unknown host"}),
            Some(&sid),
        )
        .await
        .unwrap();
        assert!(missing["error"].as_str().unwrap().contains("missing"));
    }
}

#[cfg(test)]
#[path = "remote_peer_critic1419.rs"]
mod critic1419;

#[cfg(test)]
#[path = "remote_peer_critic1419r2.rs"]
mod critic1419r2;
