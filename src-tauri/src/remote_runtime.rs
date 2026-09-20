//! The live half of a remote connection: status, base URL, session token.
//!
//! `remote_connection.rs` next door persists WHAT a connection is. This module
//! owns WHETHER it is up, WHERE it answers and WITH WHICH credential — the state
//! machine that used to run in the WebView (`remoteConnections.ts`, before
//! #790-ef85).
//!
//! It moved for a reason that is not tidiness. A base URL and a token that exist
//! only in the JS heap can only be used by JS: no Rust task can reach a remote
//! daemon at all, so nothing on this side can mirror that daemon's events or
//! seed its session list. Every feature that makes a remote session behave like
//! a local one needs a backend able to talk to it, and that starts here.
//!
//! Two invariants carried over from the WebView implementation, both load
//! bearing:
//!
//! * **Status is not read from `/health`.** That is the one route the daemon
//!   serves without a credential, so it answers 200 to a client holding nothing.
//!   Reading "connected" off it was the original defect (#781-9652): every real
//!   call then 401'd while the UI showed a healthy connection. The probe is
//!   `/api/version`, which sits behind the auth middleware.
//! * **An unauthenticated connection routes nothing.** It gets no poll task and
//!   no base URL in the snapshot, so `rpcImpl` refuses rather than retrying a
//!   401 on a widening backoff.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use crate::remote_connection::{RemoteConnection, RemoteConnectionStore, RemoteTransport};
use crate::state::{AppEvent, AppState};

/// How often a connected connection re-proves itself against `/api/version`.
const STATUS_POLL: Duration = Duration::from_secs(5);
/// Budget for one probe. Generous: a tunnel over a slow link is not a failure.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long `connect` waits for an SSH tunnel to report Connected.
const TUNNEL_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const TUNNEL_POLL: Duration = Duration::from_millis(250);

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The five states a connection can be in, spelled exactly as the frontend
/// renders them.
///
/// `Unauthenticated` is deliberately distinct from `Error`: the network is fine
/// and the daemon is answering, so the fix is the password, not the route. They
/// were one state once, and the resulting "connection error" sent people to
/// debug their tunnel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RemoteStatus {
    Disconnected,
    Connecting,
    Connected,
    Unauthenticated,
    Error,
}

/// What a client needs to render a connection and to route a call to it.
///
/// `token` is the daemon's in-memory session token. It is handed to the client
/// because `rpcImpl`, the terminal WebSocket and the `/events` SSE all have to
/// put it in a query string — a WebSocket upgrade carries no `Authorization`
/// header. The password it was traded for never leaves the backend, and this
/// token is never written to disk on either side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RemoteConnectionStatus {
    pub(crate) id: String,
    pub(crate) status: RemoteStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) protocol_version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

/// Live state for one connection. Never serialized as-is: `snapshot` builds the
/// client view, which omits the tunnel and the task handle because neither is
/// the client's business.
#[derive(Default)]
struct Entry {
    status: Option<RemoteStatus>,
    base_url: Option<String>,
    token: Option<String>,
    protocol_version: Option<u64>,
    error: Option<String>,
    tunnel_id: Option<String>,
    poll: Option<tokio::task::JoinHandle<()>>,
    /// The task that mirrors this daemon's sessions and events (#791-055e).
    mirror: Option<tokio::task::JoinHandle<()>>,
}

impl Entry {
    fn snapshot(&self, id: &str) -> RemoteConnectionStatus {
        let status = self.status.unwrap_or(RemoteStatus::Disconnected);
        // Base URL and token are answers about where to send a call. A
        // connection that is not connected has no such answer, and handing one
        // out anyway is how a call reaches a daemon that rejected us.
        //
        // The protocol version is withheld for the same reason and for a second
        // one: it is learned from `/health` halfway through connecting, so
        // publishing it early emits a second `connecting` push that says nothing
        // a client can act on.
        let connected = status == RemoteStatus::Connected;
        RemoteConnectionStatus {
            id: id.to_string(),
            status,
            base_url: connected.then(|| self.base_url.clone()).flatten(),
            token: connected.then(|| self.token.clone()).flatten(),
            protocol_version: connected.then_some(self.protocol_version).flatten(),
            error: self.error.clone(),
        }
    }
}

/// Live state for every connection that has been asked to connect at least once.
///
/// A connection absent from the map is `Disconnected` — the map holds what is
/// happening, not what is configured. `connections.json` is the list.
#[derive(Default)]
pub(crate) struct RemoteRuntime {
    entries: DashMap<String, Entry>,
}

impl RemoteRuntime {
    /// Every connection this runtime knows about, in no particular order.
    pub(crate) fn snapshot(&self) -> Vec<RemoteConnectionStatus> {
        self.entries
            .iter()
            .map(|e| e.value().snapshot(e.key()))
            .collect()
    }

    /// Base URL for a connected connection, or `None`. The routing answer.
    pub(crate) fn base_url(&self, id: &str) -> Option<String> {
        self.entries.get(id).and_then(|e| e.snapshot(id).base_url)
    }

    /// Session token for a connected connection, or `None` when it needs none.
    pub(crate) fn token(&self, id: &str) -> Option<String> {
        self.entries.get(id).and_then(|e| e.snapshot(id).token)
    }

    fn status_of(&self, id: &str) -> RemoteStatus {
        self.entries
            .get(id)
            .and_then(|e| e.status)
            .unwrap_or(RemoteStatus::Disconnected)
    }

    /// Park a connection in `Connected` with a known route, so a test of
    /// something that reads the route does not have to run the whole handshake.
    #[cfg(test)]
    pub(crate) fn force_connected_for_test(&self, id: &str, base_url: &str, token: Option<&str>) {
        self.entries.insert(
            id.to_string(),
            Entry {
                status: Some(RemoteStatus::Connected),
                base_url: Some(base_url.to_string()),
                token: token.map(str::to_string),
                ..Default::default()
            },
        );
    }
}

// ---------------------------------------------------------------------------
// Status publication
// ---------------------------------------------------------------------------

/// The wire body of a status change, shared by the desktop window event and the
/// `/events` SSE arm — one builder, because the pairs built from separate code
/// in separate files are the ones that drift.
pub(crate) fn remote_connection_status_payload(
    status: &RemoteConnectionStatus,
) -> serde_json::Value {
    serde_json::to_value(status).unwrap_or_else(|_| serde_json::json!({ "id": status.id }))
}

/// Apply `mutate` to a connection's entry and announce the result if the client
/// view moved.
///
/// Dedup is on the snapshot, not on the entry: a tunnel id changing is not news
/// for anyone, and a poll that keeps answering 200 must not emit 12 events a
/// minute.
fn update<F: FnOnce(&mut Entry)>(state: &Arc<AppState>, id: &str, mutate: F) {
    let before = state.remote.entries.get(id).map(|e| e.value().snapshot(id));
    let after = {
        let mut entry = state.remote.entries.entry(id.to_string()).or_default();
        mutate(entry.value_mut());
        entry.value().snapshot(id)
    };
    if before.as_ref() == Some(&after) {
        return;
    }
    publish(state, &after);
}

/// Dual-emit. Nothing forwards the bus to the desktop window, so the window
/// listener is fed here and the bus feeds `/events` SSE — the IPC/HTTP parity
/// rule in AGENTS.md. Both carry the same payload.
fn publish(state: &Arc<AppState>, status: &RemoteConnectionStatus) {
    let payload = remote_connection_status_payload(status);
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        use tauri::Emitter;
        let _ = app.emit("remote-connection-status", &payload);
    }
    let _ = state
        .event_bus
        .send(AppEvent::RemoteConnectionStatusChanged {
            payload: payload.clone(),
        });
    let _ = payload;
}

fn set_error(state: &Arc<AppState>, id: &str, status: RemoteStatus, error: String) {
    tracing::warn!(source = "remote", connection = id, %error, "Remote connection failed");
    update(state, id, |e| {
        e.status = Some(status);
        e.token = None;
        e.error = Some(error);
    });
}

// ---------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------

fn http_client() -> reqwest::Client {
    reqwest::Client::new()
}

/// What `/health` says about the daemon behind a base URL.
#[derive(Debug, Default, PartialEq, Eq)]
struct Health {
    protocol_version: Option<u64>,
    /// Which process answered. `None` from a daemon older than the field —
    /// unknown identity cannot prove a self-connection, so it is not treated as
    /// one.
    instance_id: Option<String>,
}

/// Read `/health` — the one route served without a credential — to learn the
/// protocol version and prove the daemon is reachable at all.
async fn read_health(base_url: &str) -> Result<Health, String> {
    let url = format!("{}/health", base_url.trim_end_matches('/'));
    let response = http_client()
        .get(&url)
        .timeout(PROBE_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("Unreachable: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Health check failed: {}", response.status()));
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Malformed health response: {e}"))?;
    Ok(Health {
        protocol_version: body
            .get("protocol_version")
            .and_then(serde_json::Value::as_u64),
        instance_id: body
            .get("instance_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    })
}

/// Outcome of a probe against a route that requires the credential.
#[derive(Debug, PartialEq, Eq)]
enum Probe {
    Ok,
    Rejected,
    Failed(String),
}

/// Probe `/api/version`, which sits behind the auth middleware.
async fn probe_authenticated(base_url: &str, token: Option<&str>) -> Probe {
    let url = format!("{}/api/version", base_url.trim_end_matches('/'));
    let mut request = http_client().get(&url).timeout(PROBE_TIMEOUT);
    if let Some(token) = token {
        request = request.query(&[("token", token)]);
    }
    match request.send().await {
        Ok(response) if response.status().is_success() => Probe::Ok,
        Ok(response) if response.status() == reqwest::StatusCode::UNAUTHORIZED => Probe::Rejected,
        Ok(response) => Probe::Failed(format!("Status check failed: {}", response.status())),
        Err(e) => Probe::Failed(format!("Unreachable: {e}")),
    }
}

/// Trade the stored password for the daemon's token, when there is one stored.
///
/// No stored password is not a failure: a desktop TUICommander on the LAN with
/// `lan_auth_bypass` needs none. The probe that follows decides.
async fn authenticate(
    connection: &RemoteConnection,
    base_url: &str,
) -> Result<Option<String>, String> {
    if !crate::remote_connection::connection_password_exists(&connection.id)? {
        return Ok(None);
    }
    crate::remote_connection::fetch_connection_token(
        &connection.id,
        base_url,
        &connection.auth_username,
    )
    .await
    .map(Some)
}

// ---------------------------------------------------------------------------
// Connect / disconnect
// ---------------------------------------------------------------------------

fn load_connection(state: &Arc<AppState>, id: &str) -> Result<RemoteConnection, String> {
    RemoteConnectionStore::load(&state.data_dir)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("Unknown remote connection {id}"))
}

/// Bring a connection up: resolve where it answers, prove it is reachable,
/// authenticate, then start the status poll.
///
/// Idempotent while in flight: a second call on a connecting or connected
/// connection is a no-op, so a double click cannot open two tunnels.
pub(crate) async fn connect(state: &Arc<AppState>, id: &str) -> Result<(), String> {
    let connection = load_connection(state, id)?;
    match state.remote.status_of(id) {
        RemoteStatus::Connecting | RemoteStatus::Connected => return Ok(()),
        _ => {}
    }
    update(state, id, |e| {
        e.status = Some(RemoteStatus::Connecting);
        e.error = None;
    });
    tracing::info!(source = "remote", connection = id, name = %connection.name, "Connecting");

    let base_url = match resolve_base_url(state, &connection).await {
        Ok(url) => url,
        Err(e) => {
            set_error(state, id, RemoteStatus::Error, e.clone());
            return Err(e);
        }
    };
    update(state, id, |e| e.base_url = Some(base_url.clone()));

    match read_health(&base_url).await {
        Ok(health) => {
            // A connection that resolves back to this very process mirrors every
            // local event onto the bus that produced it, and both `/events` and
            // the window emit repeat it — the origin marker stops the second hop,
            // but nothing downstream can make sense of a machine mirroring
            // itself. Refuse it where the user can still read why.
            if health.instance_id.as_deref() == Some(crate::app_instance::instance_identity()) {
                let msg = format!(
                    "{base_url} is this very TUICommander instance — a machine cannot mirror itself. \
                     Point this connection at another machine's daemon."
                );
                set_error(state, id, RemoteStatus::Error, msg.clone());
                return Err(msg);
            }
            update(state, id, |e| e.protocol_version = health.protocol_version);
        }
        Err(e) => {
            set_error(state, id, RemoteStatus::Error, e.clone());
            return Err(e);
        }
    }

    let token = match authenticate(&connection, &base_url).await {
        Ok(token) => token,
        Err(e) => {
            set_error(state, id, RemoteStatus::Unauthenticated, e.clone());
            return Err(e);
        }
    };

    match probe_authenticated(&base_url, token.as_deref()).await {
        Probe::Ok => {}
        Probe::Rejected => {
            let msg =
                "The remote daemon rejected these credentials — check the username and password."
                    .to_string();
            set_error(state, id, RemoteStatus::Unauthenticated, msg.clone());
            return Err(msg);
        }
        Probe::Failed(e) => {
            set_error(state, id, RemoteStatus::Error, e.clone());
            return Err(e);
        }
    }

    update(state, id, |e| {
        e.status = Some(RemoteStatus::Connected);
        e.token = token;
        e.error = None;
    });
    spawn_status_poll(state, id.to_string());
    spawn_mirror(state, id.to_string());
    tracing::info!(source = "remote", connection = id, "Connected");
    Ok(())
}

/// Where the daemon answers.
///
/// The SSH profile is built in memory and handed straight to the tunnel manager:
/// it is an implementation detail of this connection, not a profile the user
/// owns, so it does not belong in the tunnels directory or in the Tunnels panel.
/// (The WebView implementation persisted one named `__remote_<id>` and deleted
/// it on disconnect, which left litter behind whenever the app exited first.)
async fn resolve_base_url(
    state: &Arc<AppState>,
    connection: &RemoteConnection,
) -> Result<String, String> {
    match &connection.transport {
        RemoteTransport::Direct { url } => Ok(url.trim_end_matches('/').to_string()),
        RemoteTransport::Ssh {
            ssh_host,
            ssh_port,
            ssh_user,
            identity_file,
            remote_daemon_port,
        } => {
            use crate::tunnels::profile::{
                ForwardSpec, ProfileOptions, StrictHostKeyChecking, TunnelProfile,
            };
            let local_port = crate::tunnels::port::find_free_port()
                .await
                .map_err(|e| format!("No free local port for the tunnel: {e}"))?;
            let mut profile = TunnelProfile::new(
                format!("remote connection {}", connection.name),
                ssh_host.clone(),
                ssh_user.clone(),
            );
            profile.port = *ssh_port;
            profile.identity_file = identity_file.as_ref().map(PathBuf::from);
            profile.forwards = vec![ForwardSpec::Local {
                bind_port: local_port,
                remote_host: "127.0.0.1".to_string(),
                remote_port: *remote_daemon_port,
            }];
            // Only the host-key policy differs from a hand-made profile: this
            // tunnel is created on the user's behalf, so a first connection
            // cannot stop to ask about a fingerprint. Everything else —
            // including `Compression=yes`, which is what keeps the terminal
            // stream small on this exact link — stays at the default.
            profile.options = ProfileOptions {
                strict_host_key_checking: StrictHostKeyChecking::AcceptNew,
                ..ProfileOptions::default()
            };
            let tunnel_id = state.tunnel_manager.start(profile).await?;
            update(state, &connection.id, |e| {
                e.tunnel_id = Some(tunnel_id.clone())
            });
            wait_for_tunnel(state, &tunnel_id).await?;
            Ok(format!("http://127.0.0.1:{local_port}"))
        }
    }
}

async fn wait_for_tunnel(state: &Arc<AppState>, tunnel_id: &str) -> Result<(), String> {
    use crate::tunnels::supervisor::TunnelStatus;
    let deadline = std::time::Instant::now() + TUNNEL_CONNECT_TIMEOUT;
    loop {
        match state.tunnel_manager.get_status(tunnel_id) {
            Some(TunnelStatus::Connected) => return Ok(()),
            Some(TunnelStatus::Error { message }) => {
                return Err(format!("SSH tunnel failed: {message}"));
            }
            Some(TunnelStatus::Stopped { reason }) => {
                return Err(format!("SSH tunnel stopped: {reason}"));
            }
            _ => {}
        }
        if std::time::Instant::now() >= deadline {
            return Err("SSH tunnel did not connect in time".to_string());
        }
        tokio::time::sleep(TUNNEL_POLL).await;
    }
}

/// Take a connection down: stop the tasks, drop its sessions, forget the token,
/// stop the tunnel.
pub(crate) async fn disconnect(state: &Arc<AppState>, id: &str) {
    let (poll, mirror, tunnel_id) = {
        let mut entry = state.remote.entries.entry(id.to_string()).or_default();
        (
            entry.poll.take(),
            entry.mirror.take(),
            entry.tunnel_id.take(),
        )
    };
    if let Some(poll) = poll {
        poll.abort();
    }
    // Abort before dropping the rows: a frame still in flight would otherwise
    // re-seed the map we just cleared.
    if let Some(mirror) = mirror {
        mirror.abort();
    }
    crate::remote_mirror::drop_connection(state, id);
    if let Some(tunnel_id) = tunnel_id {
        state.tunnel_manager.stop_if_running(&tunnel_id);
    }
    update(state, id, |e| {
        e.status = Some(RemoteStatus::Disconnected);
        e.base_url = None;
        e.token = None;
        e.protocol_version = None;
        e.error = None;
    });
    tracing::info!(source = "remote", connection = id, "Disconnected");
}

/// Re-prove a connected connection every [`STATUS_POLL`].
///
/// The daemon mints its token in memory and forgets it on restart, so ours goes
/// stale while `/health` keeps answering 200. One re-authentication on a 401 is
/// what turns that restart into a reconnect instead of a dead panel.
fn spawn_status_poll(state: &Arc<AppState>, id: String) {
    let previous = {
        let mut entry = state.remote.entries.entry(id.clone()).or_default();
        entry.poll.take()
    };
    if let Some(previous) = previous {
        previous.abort();
    }
    let task_state = Arc::clone(state);
    let task_id = id.clone();
    let handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(STATUS_POLL).await;
            if task_state.remote.status_of(&task_id) != RemoteStatus::Connected {
                return;
            }
            poll_once(&task_state, &task_id).await;
        }
    });
    let mut entry = state.remote.entries.entry(id).or_default();
    entry.poll = Some(handle);
}

/// Start (or restart) the task that mirrors this daemon's sessions.
///
/// Separate from the status poll on purpose: the poll is a request/response
/// heartbeat and the mirror is a long-lived stream, so one failing must not take
/// the other down.
fn spawn_mirror(state: &Arc<AppState>, id: String) {
    let previous = {
        let mut entry = state.remote.entries.entry(id.clone()).or_default();
        entry.mirror.take()
    };
    if let Some(previous) = previous {
        previous.abort();
    }
    let handle = crate::remote_mirror::spawn(state, id.clone());
    let mut entry = state.remote.entries.entry(id).or_default();
    entry.mirror = Some(handle);
}

async fn poll_once(state: &Arc<AppState>, id: &str) {
    let Some(base_url) = state.remote.base_url(id) else {
        return;
    };
    let token = state.remote.token(id);
    match probe_authenticated(&base_url, token.as_deref()).await {
        Probe::Ok => update(state, id, |e| {
            e.status = Some(RemoteStatus::Connected);
            e.error = None;
        }),
        Probe::Rejected => reauthenticate(state, id, &base_url).await,
        Probe::Failed(e) => set_error(state, id, RemoteStatus::Error, e),
    }
}

/// One re-authentication attempt, then the truth either way.
async fn reauthenticate(state: &Arc<AppState>, id: &str, base_url: &str) {
    let Ok(connection) = load_connection(state, id) else {
        set_error(
            state,
            id,
            RemoteStatus::Error,
            format!("Remote connection {id} is no longer configured"),
        );
        return;
    };
    let token = match authenticate(&connection, base_url).await {
        Ok(token) => token,
        Err(e) => {
            set_error(state, id, RemoteStatus::Unauthenticated, e);
            return;
        }
    };
    match probe_authenticated(base_url, token.as_deref()).await {
        Probe::Ok => update(state, id, |e| {
            e.status = Some(RemoteStatus::Connected);
            e.token = token;
            e.error = None;
        }),
        Probe::Rejected => set_error(
            state,
            id,
            RemoteStatus::Unauthenticated,
            "The remote daemon rejected these credentials — check the username and password."
                .to_string(),
        ),
        Probe::Failed(e) => set_error(state, id, RemoteStatus::Error, e),
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn connect_remote_connection(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    connect(&state.inner().clone(), &id).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn disconnect_remote_connection(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    disconnect(&state.inner().clone(), &id).await;
    Ok(())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn remote_connection_statuses(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<RemoteConnectionStatus>, String> {
    Ok(state.remote.snapshot())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connection_nobody_touched_is_disconnected() {
        let runtime = RemoteRuntime::default();
        assert_eq!(runtime.status_of("nope"), RemoteStatus::Disconnected);
        assert!(runtime.base_url("nope").is_none());
        assert!(runtime.snapshot().is_empty());
    }

    #[test]
    fn an_unauthenticated_connection_hands_out_no_route() {
        // The defect this pins: a base URL survives in the entry so a later
        // reconnect can reuse it, but handing it to a caller would let a call
        // reach a daemon that just rejected us.
        let entry = Entry {
            status: Some(RemoteStatus::Unauthenticated),
            base_url: Some("http://host:9877".into()),
            token: Some("stale".into()),
            protocol_version: Some(4),
            error: Some("rejected".into()),
            ..Entry::default()
        };
        let snapshot = entry.snapshot("id");
        assert_eq!(snapshot.status, RemoteStatus::Unauthenticated);
        assert!(snapshot.base_url.is_none());
        assert!(snapshot.token.is_none());
        assert!(snapshot.protocol_version.is_none());
        assert_eq!(snapshot.error.as_deref(), Some("rejected"));
    }

    #[test]
    fn a_connected_connection_answers_where_and_with_what() {
        let entry = Entry {
            status: Some(RemoteStatus::Connected),
            base_url: Some("http://host:9877".into()),
            token: Some("t0ken".into()),
            protocol_version: Some(3),
            ..Entry::default()
        };
        let snapshot = entry.snapshot("id");
        assert_eq!(snapshot.base_url.as_deref(), Some("http://host:9877"));
        assert_eq!(snapshot.token.as_deref(), Some("t0ken"));
        assert_eq!(snapshot.protocol_version, Some(3));
    }

    #[test]
    fn status_serializes_as_the_frontend_spells_it() {
        // The frontend renders these strings directly; a rename here is a silent
        // "unknown status" there.
        for (status, expected) in [
            (RemoteStatus::Disconnected, "\"disconnected\""),
            (RemoteStatus::Connecting, "\"connecting\""),
            (RemoteStatus::Connected, "\"connected\""),
            (RemoteStatus::Unauthenticated, "\"unauthenticated\""),
            (RemoteStatus::Error, "\"error\""),
        ] {
            assert_eq!(serde_json::to_string(&status).unwrap(), expected);
        }
    }

    #[test]
    fn a_disconnected_snapshot_carries_no_secret() {
        let status = Entry::default().snapshot("id");
        let json = serde_json::to_string(&status).unwrap();
        assert!(!json.contains("token"), "token leaked into {json}");
        assert!(!json.contains("base_url"), "base_url leaked into {json}");
    }

    #[tokio::test]
    async fn health_reports_the_protocol_version() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/health")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"protocol_version":4,"instance_id":"other-process"}"#)
            .create_async()
            .await;
        assert_eq!(
            read_health(&server.url()).await.unwrap(),
            Health {
                protocol_version: Some(4),
                instance_id: Some("other-process".into()),
            }
        );
        mock.assert_async().await;
    }

    /// A daemon too old to publish an identity cannot be proven to be this
    /// process, and an unprovable self-connection must still connect: the
    /// alternative refuses every pre-#801 daemon on the network.
    #[tokio::test]
    async fn health_without_an_identity_is_not_a_self_connection() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/health")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"protocol_version":1}"#)
            .create_async()
            .await;
        assert_eq!(read_health(&server.url()).await.unwrap().instance_id, None);
    }

    #[tokio::test]
    async fn health_failing_names_the_status_rather_than_the_network() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/health")
            .with_status(503)
            .create_async()
            .await;
        let error = read_health(&server.url()).await.unwrap_err();
        assert!(error.contains("503"), "{error}");
    }

    #[tokio::test]
    async fn the_probe_carries_the_token_in_the_query_string() {
        // Not a header: the same credential has to work for a WebSocket upgrade,
        // which cannot set one. If this ever moves to a header, the terminal
        // stream breaks and nothing else does — a split that is hard to see.
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/api/version")
            .match_query(mockito::Matcher::UrlEncoded("token".into(), "t ok".into()))
            .with_status(200)
            .with_body("{}")
            .create_async()
            .await;
        assert_eq!(
            probe_authenticated(&server.url(), Some("t ok")).await,
            Probe::Ok
        );
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn a_rejected_probe_is_told_apart_from_a_broken_one() {
        let mut server = mockito::Server::new_async().await;
        let _unauthorized = server
            .mock("GET", "/api/version")
            .with_status(401)
            .create_async()
            .await;
        assert_eq!(
            probe_authenticated(&server.url(), None).await,
            Probe::Rejected
        );

        let mut broken = mockito::Server::new_async().await;
        let _server_error = broken
            .mock("GET", "/api/version")
            .with_status(500)
            .create_async()
            .await;
        match probe_authenticated(&broken.url(), None).await {
            Probe::Failed(e) => assert!(e.contains("500"), "{e}"),
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_unreachable_daemon_names_itself_unreachable() {
        // Port 1 on loopback refuses immediately: no DNS, no timeout, no flake.
        match probe_authenticated("http://127.0.0.1:1", None).await {
            Probe::Failed(e) => assert!(e.contains("Unreachable"), "{e}"),
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Flow: the state machine against a mock daemon.
    //
    // These are the assertions the WebView implementation used to carry
    // (`remoteConnections.test.ts` before #790-ef85). They test the connect →
    // authenticate → poll → disconnect sequence end to end, because that is
    // where the defects were: every individual probe was already right.
    //
    // The vault is the process-wide `#[cfg(test)]` mock keyring in
    // `credentials.rs`, so nothing here opens the real Keychain. Each test
    // stores its password under a fresh connection UUID.
    // -----------------------------------------------------------------------

    fn test_state() -> Arc<AppState> {
        Arc::new(crate::state::tests_support::make_test_app_state())
    }

    /// Save a Direct connection pointing at `url`, where `connect` will find it.
    fn direct_connection(state: &Arc<AppState>, url: &str) -> String {
        let connection = crate::remote_connection::RemoteConnection::new_direct(
            "vps",
            url.trim_end_matches('/'),
            "boss",
        );
        let id = connection.id.clone();
        RemoteConnectionStore::save(&state.data_dir, std::slice::from_ref(&connection)).unwrap();
        id
    }

    /// The statuses announced on the bus since the last drain, in order.
    fn drain_statuses(rx: &mut tokio::sync::broadcast::Receiver<AppEvent>) -> Vec<String> {
        let mut seen = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let AppEvent::RemoteConnectionStatusChanged { payload } = event {
                seen.push(payload["status"].as_str().unwrap_or("?").to_string());
            }
        }
        seen
    }

    #[test]
    fn a_machine_with_no_remote_connections_runs_nothing() {
        // The runtime is lazy by construction: an entry exists only where
        // `connect` put one, and only an entry can hold a poll task. A machine
        // that never configured a remote connection therefore pays nothing —
        // no task, no socket, no timer — and this is the regression that says
        // so, because "it is lazy" is invisible in a profile until it is not.
        let state = test_state();
        assert!(state.remote.snapshot().is_empty());
        assert!(state.remote.base_url("anything").is_none());
        assert!(state.remote.token("anything").is_none());
        assert_eq!(
            state.remote.status_of("anything"),
            RemoteStatus::Disconnected
        );
    }

    #[tokio::test]
    async fn connecting_announces_each_step_and_ends_with_a_route() {
        let mut server = mockito::Server::new_async().await;
        let _health = server
            .mock("GET", "/health")
            .with_body(r#"{"protocol_version":4}"#)
            .create_async()
            .await;
        let _version = server
            .mock("GET", "/api/version")
            .with_body("{}")
            .create_async()
            .await;

        let state = test_state();
        let id = direct_connection(&state, &server.url());
        let mut events = state.event_bus.subscribe();

        connect(&state, &id).await.unwrap();

        // Two changes, not one: a connection that jumps straight to connected
        // leaves the panel with no way to show that anything is happening.
        assert_eq!(drain_statuses(&mut events), vec!["connecting", "connected"]);
        assert_eq!(
            state.remote.base_url(&id).as_deref(),
            Some(server.url().trim_end_matches('/'))
        );
        assert_eq!(state.remote.snapshot()[0].protocol_version, Some(4));
        disconnect(&state, &id).await;
    }

    /// A Direct connection aimed at this machine's own daemon mirrors every
    /// local event back onto the bus that produced it. The origin marker keeps
    /// that from looping, but the connection itself is meaningless, so it is
    /// refused at the one place that can still explain why.
    #[tokio::test]
    async fn connecting_to_this_very_process_is_refused_by_identity() {
        let mut server = mockito::Server::new_async().await;
        let _health = server
            .mock("GET", "/health")
            .with_body(format!(
                r#"{{"protocol_version":4,"instance_id":"{}"}}"#,
                crate::app_instance::instance_identity()
            ))
            .create_async()
            .await;
        // Answering this one proves the refusal happened before the probe: a
        // connect that reached it would have succeeded.
        let version = server
            .mock("GET", "/api/version")
            .with_body("{}")
            .expect(0)
            .create_async()
            .await;

        let state = test_state();
        let id = direct_connection(&state, &server.url());

        let error = connect(&state, &id)
            .await
            .expect_err("a self-connection is not a connection");
        assert!(
            error.contains("this very TUICommander instance"),
            "the error must name the cause: {error}"
        );
        assert_eq!(state.remote.status_of(&id), RemoteStatus::Error);
        assert!(state.remote.token(&id).is_none());
        version.assert_async().await;
    }

    #[tokio::test]
    async fn a_daemon_that_rejects_the_password_leaves_no_route_and_no_poll() {
        let mut server = mockito::Server::new_async().await;
        let _health = server
            .mock("GET", "/health")
            .with_body("{}")
            .create_async()
            .await;
        let _token = server
            .mock("GET", "/api/auth/session-token")
            .with_status(401)
            .create_async()
            .await;

        let state = test_state();
        let id = direct_connection(&state, &server.url());
        crate::remote_connection::set_connection_password(&id, "wrong").unwrap();
        let mut events = state.event_bus.subscribe();

        connect(&state, &id).await.unwrap_err();

        assert_eq!(
            drain_statuses(&mut events),
            vec!["connecting", "unauthenticated"]
        );
        // Unauthenticated is reachable and answering, so it is tempting to keep
        // routing to it. Nothing may: every call would 401.
        assert!(state.remote.base_url(&id).is_none());
        assert!(state.remote.token(&id).is_none());
        assert!(
            state.remote.entries.get(&id).unwrap().poll.is_none(),
            "a rejected connection must not poll"
        );
    }

    #[tokio::test]
    async fn the_stored_password_becomes_the_token_that_signs_the_probe() {
        let mut server = mockito::Server::new_async().await;
        let _health = server
            .mock("GET", "/health")
            .with_body("{}")
            .create_async()
            .await;
        let token_route = server
            .mock("GET", "/api/auth/session-token")
            .match_header("authorization", mockito::Matcher::Any)
            .with_body(r#"{"token":"tok-1"}"#)
            .create_async()
            .await;
        let probe = server
            .mock("GET", "/api/version")
            .match_query(mockito::Matcher::UrlEncoded("token".into(), "tok-1".into()))
            .with_body("{}")
            .create_async()
            .await;

        let state = test_state();
        let id = direct_connection(&state, &server.url());
        crate::remote_connection::set_connection_password(&id, "s3cret").unwrap();

        connect(&state, &id).await.unwrap();

        token_route.assert_async().await;
        probe.assert_async().await;
        assert_eq!(state.remote.token(&id).as_deref(), Some("tok-1"));
        // The password was traded for the token here and must not follow it out
        // to the client — that is the whole reason the exchange moved to Rust.
        let published = serde_json::to_string(&state.remote.snapshot()).unwrap();
        assert!(
            !published.contains("s3cret"),
            "password leaked into {published}"
        );
        disconnect(&state, &id).await;
    }

    #[tokio::test]
    async fn disconnecting_forgets_the_token_and_the_route() {
        let mut server = mockito::Server::new_async().await;
        let _health = server
            .mock("GET", "/health")
            .with_body("{}")
            .create_async()
            .await;
        let _token = server
            .mock("GET", "/api/auth/session-token")
            .with_body(r#"{"token":"tok-1"}"#)
            .create_async()
            .await;
        // `Matcher::Any`: mockito's default is an exact query match, and this
        // probe carries the token it was just handed.
        let _probe = server
            .mock("GET", "/api/version")
            .match_query(mockito::Matcher::Any)
            .with_body("{}")
            .create_async()
            .await;

        let state = test_state();
        let id = direct_connection(&state, &server.url());
        crate::remote_connection::set_connection_password(&id, "s3cret").unwrap();
        connect(&state, &id).await.unwrap();
        let mut events = state.event_bus.subscribe();

        disconnect(&state, &id).await;

        assert_eq!(drain_statuses(&mut events), vec!["disconnected"]);
        assert!(state.remote.token(&id).is_none());
        assert!(state.remote.base_url(&id).is_none());
        let entry = state.remote.entries.get(&id).unwrap();
        assert!(
            entry.poll.is_none(),
            "the poll task outlived the connection"
        );
        assert!(entry.token.is_none(), "the token survived in the entry");
    }

    #[tokio::test]
    async fn connecting_twice_opens_one_connection() {
        let mut server = mockito::Server::new_async().await;
        // `expect(1)`: a second connect must not re-probe, because on the SSH
        // transport the same call would open a second tunnel.
        let health = server
            .mock("GET", "/health")
            .with_body("{}")
            .expect(1)
            .create_async()
            .await;
        let _probe = server
            .mock("GET", "/api/version")
            .with_body("{}")
            .create_async()
            .await;

        let state = test_state();
        let id = direct_connection(&state, &server.url());
        let mut events = state.event_bus.subscribe();

        connect(&state, &id).await.unwrap();
        connect(&state, &id).await.unwrap();

        health.assert_async().await;
        assert_eq!(drain_statuses(&mut events), vec!["connecting", "connected"]);
        disconnect(&state, &id).await;
    }

    #[tokio::test]
    async fn a_daemon_restart_costs_exactly_one_reauthentication() {
        // The daemon mints its token in memory. After a restart ours is unknown
        // to it and `/health` still answers 200, so only the authenticated probe
        // can see it — and the answer is a new token, not an error.
        let mut server = mockito::Server::new_async().await;
        let stale = server
            .mock("GET", "/api/version")
            .match_query(mockito::Matcher::UrlEncoded("token".into(), "tok-1".into()))
            .with_status(401)
            .create_async()
            .await;
        let fresh = server
            .mock("GET", "/api/version")
            .match_query(mockito::Matcher::UrlEncoded("token".into(), "tok-2".into()))
            .with_body("{}")
            .create_async()
            .await;
        let token_route = server
            .mock("GET", "/api/auth/session-token")
            .with_body(r#"{"token":"tok-2"}"#)
            .expect(1)
            .create_async()
            .await;

        let state = test_state();
        let id = direct_connection(&state, &server.url());
        crate::remote_connection::set_connection_password(&id, "s3cret").unwrap();
        update(&state, &id, |e| {
            e.status = Some(RemoteStatus::Connected);
            e.base_url = Some(server.url());
            e.token = Some("tok-1".into());
        });
        let mut events = state.event_bus.subscribe();

        poll_once(&state, &id).await;

        stale.assert_async().await;
        fresh.assert_async().await;
        token_route.assert_async().await;
        assert_eq!(state.remote.token(&id).as_deref(), Some("tok-2"));
        // One event: the connection never left `connected`, only its token did.
        assert_eq!(drain_statuses(&mut events), vec!["connected"]);
    }

    #[tokio::test]
    async fn a_poll_that_keeps_succeeding_announces_nothing() {
        let mut server = mockito::Server::new_async().await;
        let _probe = server
            .mock("GET", "/api/version")
            .with_body("{}")
            .create_async()
            .await;

        let state = test_state();
        let id = direct_connection(&state, &server.url());
        update(&state, &id, |e| {
            e.status = Some(RemoteStatus::Connected);
            e.base_url = Some(server.url());
        });
        let mut events = state.event_bus.subscribe();

        poll_once(&state, &id).await;
        poll_once(&state, &id).await;

        // Dedup is on the client view: 12 identical pushes a minute would be
        // the whole cost of the poll.
        assert!(drain_statuses(&mut events).is_empty());
    }

    #[tokio::test]
    async fn a_poll_that_cannot_reach_the_daemon_is_an_error_not_a_rejection() {
        let state = test_state();
        let id = direct_connection(&state, "http://127.0.0.1:1");
        update(&state, &id, |e| {
            e.status = Some(RemoteStatus::Connected);
            e.base_url = Some("http://127.0.0.1:1".into());
            e.token = Some("tok-1".into());
        });

        poll_once(&state, &id).await;

        // `error`, not `unauthenticated`: the password is fine, the link is not,
        // and telling the user to check their credentials sends them nowhere.
        assert_eq!(state.remote.status_of(&id), RemoteStatus::Error);
        assert!(state.remote.token(&id).is_none());
    }

    #[test]
    fn the_payload_builder_survives_every_status() {
        for status in [
            RemoteStatus::Disconnected,
            RemoteStatus::Connecting,
            RemoteStatus::Connected,
            RemoteStatus::Unauthenticated,
            RemoteStatus::Error,
        ] {
            let payload = remote_connection_status_payload(&RemoteConnectionStatus {
                id: "abc".into(),
                status,
                base_url: None,
                token: None,
                protocol_version: None,
                error: None,
            });
            assert_eq!(payload["id"], "abc");
            assert!(payload.get("status").is_some());
        }
    }
}
