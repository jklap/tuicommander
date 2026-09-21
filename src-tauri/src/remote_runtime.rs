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
pub(crate) struct RemoteRuntime {
    entries: DashMap<String, Entry>,
    /// One client for every probe, seed and event stream this module makes.
    ///
    /// A `reqwest::Client` owns the connection pool; building one per call threw
    /// the pool away each time, so every 5s heartbeat paid a fresh TCP and TLS
    /// handshake against a daemon it had just talked to.
    client: reqwest::Client,
}

impl Default for RemoteRuntime {
    fn default() -> Self {
        Self {
            entries: DashMap::new(),
            // No client-wide timeout on purpose: the probes set their own, and
            // the mirror's `/events` stream is long-lived by design — a deadline
            // here would cut it every time it succeeded.
            client: reqwest::Client::builder().build().expect(
                "the default HTTP client must build — Client::new panics on the same failure",
            ),
        }
    }
}

impl RemoteRuntime {
    /// The shared HTTP client. Cloning is an `Arc` bump, not a new pool.
    pub(crate) fn http_client(&self) -> reqwest::Client {
        self.client.clone()
    }

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

    /// Where the heartbeat sends its next probe, whatever the status.
    ///
    /// [`base_url`](Self::base_url) is the ROUTING answer and withholds the URL
    /// unless the connection is connected — right for a caller about to send a
    /// real call, wrong for the probe, which has to keep asking precisely while
    /// the connection is broken. Reading the routing answer here is what made an
    /// errored connection unable to notice that the daemon came back.
    fn probe_base_url(&self, id: &str) -> Option<String> {
        self.entries.get(id).and_then(|e| e.base_url.clone())
    }

    /// The credential the next probe signs with, whatever the status. Same
    /// reason as [`probe_base_url`](Self::probe_base_url).
    fn probe_token(&self, id: &str) -> Option<String> {
        self.entries.get(id).and_then(|e| e.token.clone())
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

    /// Record the tunnel a connection owns, exactly as `resolve_base_url` does
    /// once it has started one. Lets a test ask who stops it without standing up
    /// an ssh server, and it must run AFTER `force_connected_for_test`, which
    /// replaces the whole entry.
    #[cfg(test)]
    pub(crate) fn adopt_tunnel_for_test(&self, id: &str, tunnel_id: &str) {
        self.entries.entry(id.to_string()).or_default().tunnel_id = Some(tunnel_id.to_string());
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

/// What the daemon says when the credential is wrong, in the one place both
/// callers read it from.
const REJECTED_CREDENTIALS: &str =
    "The remote daemon rejected these credentials — check the username and password.";

/// Record a failure and retire whatever the connection was still showing.
///
/// The rows are announced closed as well as dropped, and that is the whole
/// point: a badge is sticky by construction, so a mirrored session whose row
/// simply stops being updated keeps rendering the state the machine was in when
/// the link died — a remote tab frozen mid-question, with nothing on screen
/// saying the answer can no longer reach it. `drop_connection` returns at once
/// when there is nothing left to drop, so a connection that errors on every
/// probe announces the closure once rather than every five seconds.
fn set_error(state: &Arc<AppState>, id: &str, status: RemoteStatus, error: String) {
    tracing::warn!(source = "remote", connection = id, %error, "Remote connection failed");
    update(state, id, |e| {
        e.status = Some(status);
        e.token = None;
        e.error = Some(error);
    });
    crate::remote_mirror::drop_connection(state, id);
}

// ---------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------

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
async fn read_health(client: &reqwest::Client, base_url: &str) -> Result<Health, String> {
    let url = format!("{}/health", base_url.trim_end_matches('/'));
    let response = client
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
async fn probe_authenticated(
    client: &reqwest::Client,
    base_url: &str,
    token: Option<&str>,
) -> Probe {
    let url = format!("{}/api/version", base_url.trim_end_matches('/'));
    let mut request = client.get(&url).timeout(PROBE_TIMEOUT);
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

/// A connect attempt that did not finish, and which state it leaves behind.
///
/// Carried rather than applied at each step so that every failure leaves through
/// one door: the handshake has six ways to end badly and each one of them may be
/// holding an SSH tunnel.
struct ConnectFailure {
    status: RemoteStatus,
    message: String,
}

impl ConnectFailure {
    /// The link, the daemon or the machine. Retrying may work.
    fn error(message: String) -> Self {
        Self {
            status: RemoteStatus::Error,
            message,
        }
    }

    /// The credential. Retrying the same one will not work.
    fn unauthenticated(message: String) -> Self {
        Self {
            status: RemoteStatus::Unauthenticated,
            message,
        }
    }
}

/// Bring a connection up: resolve where it answers, prove it is reachable,
/// authenticate, then start the status poll and the mirror.
///
/// Idempotent while in flight: a second call on a connecting or connected
/// connection is a no-op, so a double click cannot open two tunnels.
pub(crate) async fn connect(state: &Arc<AppState>, id: &str) -> Result<(), String> {
    let connection = load_connection(state, id)?;
    let Some(connecting) = claim_for_connect(state, id) else {
        return Ok(());
    };
    publish(state, &connecting);
    tracing::info!(source = "remote", connection = id, name = %connection.name, "Connecting");

    match handshake(state, id, &connection).await {
        Ok(token) => {
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
        Err(failure) => {
            // Every failure lands here, and that is the whole reason the steps
            // return a `ConnectFailure` instead of calling `set_error`
            // themselves: on the SSH transport `resolve_base_url` has already
            // started a tunnel by the time the health check, the token exchange
            // or the probe can fail. Leaving it running meant the next attempt
            // opened a SECOND one and orphaned the first — a `TunnelHandle` has
            // no `Drop`, so its supervisor and its ssh child outlived every
            // trace of the connection.
            stop_tunnel(state, id);
            set_error(state, id, failure.status, failure.message.clone());
            Err(failure.message)
        }
    }
}

/// Move a connection to `Connecting`, or report that someone else already has.
///
/// The check and the transition share ONE `entry()` scope, so the shard lock
/// holds across both. Read-then-write across two DashMap calls is a TOCTOU: two
/// concurrent connects — a double click, or the UI and an auto-connect racing at
/// startup — both read `Disconnected`, both wrote `Connecting`, and both went on
/// to open a tunnel.
///
/// Returns the snapshot to announce, or `None` when the connection is already up
/// or on its way.
fn claim_for_connect(state: &Arc<AppState>, id: &str) -> Option<RemoteConnectionStatus> {
    let mut entry = state.remote.entries.entry(id.to_string()).or_default();
    if matches!(
        entry.status,
        Some(RemoteStatus::Connecting | RemoteStatus::Connected)
    ) {
        return None;
    }
    entry.status = Some(RemoteStatus::Connecting);
    entry.error = None;
    Some(entry.snapshot(id))
}

/// Resolve, prove, authenticate — everything between `Connecting` and
/// `Connected`. Returns the session token, which is `None` when the daemon needs
/// none.
async fn handshake(
    state: &Arc<AppState>,
    id: &str,
    connection: &RemoteConnection,
) -> Result<Option<String>, ConnectFailure> {
    let client = state.remote.http_client();

    let base_url = resolve_base_url(state, connection)
        .await
        .map_err(ConnectFailure::error)?;
    update(state, id, |e| e.base_url = Some(base_url.clone()));

    let health = read_health(&client, &base_url)
        .await
        .map_err(ConnectFailure::error)?;
    // A connection that resolves back to this very process mirrors every local
    // event onto the bus that produced it, and both `/events` and the window
    // emit repeat it — the origin marker stops the second hop, but nothing
    // downstream can make sense of a machine mirroring itself. Refuse it where
    // the user can still read why.
    if health.instance_id.as_deref() == Some(crate::app_instance::instance_identity()) {
        return Err(ConnectFailure::error(format!(
            "{base_url} is this very TUICommander instance — a machine cannot mirror itself. \
             Point this connection at another machine's daemon."
        )));
    }
    update(state, id, |e| e.protocol_version = health.protocol_version);

    let token = authenticate(connection, &base_url)
        .await
        .map_err(ConnectFailure::unauthenticated)?;

    match probe_authenticated(&client, &base_url, token.as_deref()).await {
        Probe::Ok => Ok(token),
        Probe::Rejected => Err(ConnectFailure::unauthenticated(
            REJECTED_CREDENTIALS.to_string(),
        )),
        Probe::Failed(e) => Err(ConnectFailure::error(e)),
    }
}

/// Stop the tunnel this connection opened, by the id the tunnel manager knows it
/// under, and forget it.
///
/// Taking the id out matters as much as stopping it: a second call must not ask
/// the manager to stop a tunnel that a later attempt has since re-used the slot
/// for.
fn stop_tunnel(state: &Arc<AppState>, id: &str) {
    let tunnel_id = state
        .remote
        .entries
        .get_mut(id)
        .and_then(|mut e| e.tunnel_id.take());
    if let Some(tunnel_id) = tunnel_id {
        state.tunnel_manager.stop_if_running(&tunnel_id);
    }
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

/// Take a connection all the way down, from whatever state it is in.
///
/// **The one teardown path.** Disconnect and delete are the same four steps and
/// used to be neither shared nor complete: the two delete handlers stopped a
/// tunnel under the CONNECTION's id — which is not the tunnel's, so the call was
/// a no-op — and left the poll, the mirror, the mirrored rows and a live session
/// token running for a connection that no longer appeared anywhere. The
/// frontend's pre-delete disconnect was the only thing holding that together,
/// from the one caller that happened to remember it.
///
/// Idempotent and total. The entry is REMOVED rather than reset, because a
/// connection absent from the map is already `Disconnected` by definition — a
/// husk left behind is a status for a connection that may no longer be
/// configured, and the reason `disconnect` on an unknown id used to conjure one
/// and announce it.
pub(crate) fn teardown(state: &Arc<AppState>, id: &str) {
    let Some((_, mut entry)) = state.remote.entries.remove(id) else {
        return;
    };
    if let Some(poll) = entry.poll.take() {
        poll.abort();
    }
    // Abort before dropping the rows: a frame still in flight would otherwise
    // re-seed the map we just cleared.
    if let Some(mirror) = entry.mirror.take() {
        mirror.abort();
    }
    crate::remote_mirror::drop_connection(state, id);
    if let Some(tunnel_id) = entry.tunnel_id.take() {
        state.tunnel_manager.stop_if_running(&tunnel_id);
    }
    // The entry is gone, so there is nothing left for `update` to diff against:
    // the departure is announced by hand, and only when the connection was not
    // already sitting at `Disconnected`.
    if entry.snapshot(id).status != RemoteStatus::Disconnected {
        publish(state, &Entry::default().snapshot(id));
        tracing::info!(source = "remote", connection = id, "Disconnected");
    }
}

/// Whether the heartbeat keeps beating while a connection is in `status`.
///
/// **`Error` survives, and that is the decision this story made.** The loop used
/// to return the moment the status left `Connected`, and nothing ever restarted
/// it — so ONE probe that timed out ended the heartbeat for good. A five-second
/// blip, a laptop lid, a tunnel that reconnected by itself: each cost a
/// connection that was healthy again a second later, until a person noticed the
/// red badge and pressed Connect. The probe is the only thing that can see the
/// daemon come back, so it has to outlive the failure it reported, and a later
/// `Probe::Ok` puts the connection back to `Connected` on its own.
///
/// The alternative — `set_error` tearing the connection down — was rejected for
/// the same reason: it turns a transient fault into a manual reconnect, and it
/// drops an SSH tunnel that was very likely still fine.
///
/// `Unauthenticated` does not survive, and that is not an inconsistency: the
/// credential is wrong, re-sending it every five seconds only asks the daemon to
/// rate-limit us, and `poll_once` already spends one re-authentication before it
/// concludes that. `Disconnected` is how [`teardown`] stops this task even when
/// the abort loses a race with the sleep.
fn poll_survives(status: RemoteStatus) -> bool {
    matches!(status, RemoteStatus::Connected | RemoteStatus::Error)
}

/// Re-prove a connection every [`STATUS_POLL`] for as long as
/// [`poll_survives`] says it is worth asking.
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
            if !poll_survives(task_state.remote.status_of(&task_id)) {
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
    let Some(base_url) = state.remote.probe_base_url(id) else {
        return;
    };
    let token = state.remote.probe_token(id);
    match probe_authenticated(&state.remote.http_client(), &base_url, token.as_deref()).await {
        // Also the recovery path: an errored connection whose daemon answers
        // again is connected again, with no one having to press anything.
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
    match probe_authenticated(&state.remote.http_client(), base_url, token.as_deref()).await {
        Probe::Ok => update(state, id, |e| {
            e.status = Some(RemoteStatus::Connected);
            e.token = token;
            e.error = None;
        }),
        Probe::Rejected => set_error(
            state,
            id,
            RemoteStatus::Unauthenticated,
            REJECTED_CREDENTIALS.to_string(),
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
    teardown(&state.inner().clone(), &id);
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

    /// A client for the probe tests, which have no `AppState` to borrow one
    /// from. The same builder the runtime uses, so a test cannot pass against a
    /// client shaped differently from the real one.
    fn test_client() -> reqwest::Client {
        RemoteRuntime::default().http_client()
    }

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
            read_health(&test_client(), &server.url()).await.unwrap(),
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
        assert_eq!(
            read_health(&test_client(), &server.url())
                .await
                .unwrap()
                .instance_id,
            None
        );
    }

    #[tokio::test]
    async fn health_failing_names_the_status_rather_than_the_network() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/health")
            .with_status(503)
            .create_async()
            .await;
        let error = read_health(&test_client(), &server.url())
            .await
            .unwrap_err();
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
            probe_authenticated(&test_client(), &server.url(), Some("t ok")).await,
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
            probe_authenticated(&test_client(), &server.url(), None).await,
            Probe::Rejected
        );

        let mut broken = mockito::Server::new_async().await;
        let _server_error = broken
            .mock("GET", "/api/version")
            .with_status(500)
            .create_async()
            .await;
        match probe_authenticated(&test_client(), &broken.url(), None).await {
            Probe::Failed(e) => assert!(e.contains("500"), "{e}"),
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_unreachable_daemon_names_itself_unreachable() {
        // Port 1 on loopback refuses immediately: no DNS, no timeout, no flake.
        match probe_authenticated(&test_client(), "http://127.0.0.1:1", None).await {
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

    /// The sessions announced closed on the bus since the last drain, in order.
    fn drain_closed_sessions(rx: &mut tokio::sync::broadcast::Receiver<AppEvent>) -> Vec<String> {
        let mut seen = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let AppEvent::RemoteMirrored { event, payload, .. } = event
                && event == "session-closed"
            {
                seen.push(payload["session_id"].as_str().unwrap_or("?").to_string());
            }
        }
        seen
    }

    /// Put a real tunnel in the manager without needing one that works.
    ///
    /// The ssh binary does not exist, which is the point: `start_with_binary`
    /// returns as soon as the supervision loop is spawned, so the entry lands in
    /// the map and the test can ask who stops it.
    async fn stand_up_a_tunnel(state: &Arc<AppState>) -> String {
        state
            .tunnel_manager
            .start_with_binary_for_test(
                crate::tunnels::profile::TunnelProfile::new(
                    "remote connection vps",
                    "example.invalid",
                    "boss",
                ),
                PathBuf::from("/nonexistent/ssh"),
            )
            .await
            .expect("the manager records a tunnel before its ssh child matters")
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
        teardown(&state, &id);
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
        teardown(&state, &id);
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

        teardown(&state, &id);

        assert_eq!(drain_statuses(&mut events), vec!["disconnected"]);
        assert!(state.remote.token(&id).is_none());
        assert!(state.remote.base_url(&id).is_none());
        assert!(
            state.remote.entries.get(&id).is_none(),
            "teardown removes the entry rather than resetting it — a husk is a \
             status, a poll handle and a token for a connection that may not \
             even be configured any more"
        );
    }

    /// Teardown on an id nobody connected must not invent one.
    ///
    /// `disconnect` used to open the entry with `or_default()`, which CREATED it,
    /// and then announced a `disconnected` status for a connection this process
    /// had never heard of. Delete calls this, so every deletion of a
    /// never-connected connection left a phantom behind.
    #[tokio::test]
    async fn tearing_down_a_connection_nobody_connected_conjures_nothing() {
        let state = test_state();
        let mut events = state.event_bus.subscribe();

        teardown(&state, "never-seen");
        teardown(&state, "never-seen");

        assert!(
            state.remote.snapshot().is_empty(),
            "an unknown id left an entry behind: {:?}",
            state.remote.snapshot()
        );
        assert!(
            drain_statuses(&mut events).is_empty(),
            "an unknown id was announced to every client"
        );
    }

    /// A failed connect must not leave the tunnel it opened behind.
    ///
    /// `resolve_base_url` starts the SSH tunnel BEFORE the health check, the
    /// token exchange and the probe, so any of those failing used to return with
    /// the tunnel still running and its id still in the entry. The next attempt
    /// started a second one — and `TunnelHandle` has no `Drop`, so the first
    /// supervisor and its ssh child outlived every trace of the connection.
    ///
    /// Twice, because one attempt cannot tell a leak from a cleanup.
    #[tokio::test]
    async fn every_failed_connect_stops_the_tunnel_it_started() {
        let mut server = mockito::Server::new_async().await;
        let _health = server
            .mock("GET", "/health")
            .with_status(503)
            .create_async()
            .await;

        let state = test_state();
        let id = direct_connection(&state, &server.url());

        for attempt in 1..=2 {
            let tunnel_id = stand_up_a_tunnel(&state).await;
            // Exactly what `resolve_base_url` records on the SSH transport.
            update(&state, &id, |e| e.tunnel_id = Some(tunnel_id.clone()));

            connect(&state, &id)
                .await
                .expect_err("/health answered 503");

            assert!(
                state.tunnel_manager.list().is_empty(),
                "attempt {attempt} left a tunnel running: {:?}",
                state.tunnel_manager.list()
            );
            assert_eq!(state.remote.status_of(&id), RemoteStatus::Error);
        }
    }

    /// Only one connect may claim a connection, and the claim is what says so.
    ///
    /// The check and the transition share one `entry()` scope so the shard lock
    /// holds across both; read-then-write across two DashMap calls let two
    /// concurrent connects both see `Disconnected` and both open a tunnel.
    #[test]
    fn only_one_connect_can_claim_a_connection() {
        let state = test_state();

        let first = claim_for_connect(&state, "vps").expect("a fresh connection is free");
        assert_eq!(first.status, RemoteStatus::Connecting);
        assert_eq!(
            state.remote.status_of("vps"),
            RemoteStatus::Connecting,
            "the claim must land in the map, not only in the caller's hand"
        );
        assert!(
            claim_for_connect(&state, "vps").is_none(),
            "a second connect claimed a connection that is already coming up"
        );

        // A connected one is equally claimed, and a failed one is free again.
        update(&state, "vps", |e| e.status = Some(RemoteStatus::Connected));
        assert!(claim_for_connect(&state, "vps").is_none());
        update(&state, "vps", |e| e.status = Some(RemoteStatus::Error));
        assert!(
            claim_for_connect(&state, "vps").is_some(),
            "a connection that failed must be retryable"
        );
    }

    /// Eight connects racing on real threads still produce one handshake.
    ///
    /// Probabilistic by nature — the window between the read and the write was
    /// nanoseconds — but it cannot fail spuriously: `expect(1)` is what the
    /// fixed code always does.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_connects_run_one_handshake() {
        let mut server = mockito::Server::new_async().await;
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

        let mut racers = Vec::new();
        for _ in 0..8 {
            let state = Arc::clone(&state);
            let id = id.clone();
            racers.push(tokio::spawn(async move { connect(&state, &id).await }));
        }
        for racer in racers {
            racer
                .await
                .unwrap()
                .expect("a no-op connect is not an error");
        }

        health.assert_async().await;
        teardown(&state, &id);
    }

    /// The heartbeat outlives the failure it reported.
    ///
    /// One probe that timed out used to end the poll for good, so a five-second
    /// blip cost a connection that was healthy again a second later — until a
    /// person noticed the red badge and pressed Connect.
    #[test]
    fn the_heartbeat_outlives_an_error_but_not_a_rejection() {
        assert!(poll_survives(RemoteStatus::Connected));
        assert!(
            poll_survives(RemoteStatus::Error),
            "nothing but the probe can see the daemon come back"
        );
        assert!(
            !poll_survives(RemoteStatus::Unauthenticated),
            "re-sending a credential the daemon refused only asks to be rate-limited"
        );
        assert!(
            !poll_survives(RemoteStatus::Disconnected),
            "an absent entry is how teardown stops this task"
        );
    }

    /// An errored connection recovers by itself when the daemon answers again.
    ///
    /// Two things had to change for this: the poll survives `Error`, and it
    /// reads the raw base URL rather than the routing one — which is withheld
    /// unless connected, so the probe used to return before it sent anything.
    #[tokio::test]
    async fn an_errored_connection_recovers_without_anyone_pressing_connect() {
        let mut server = mockito::Server::new_async().await;
        let probe = server
            .mock("GET", "/api/version")
            .match_query(mockito::Matcher::Any)
            .with_body("{}")
            .create_async()
            .await;

        let state = test_state();
        let id = direct_connection(&state, &server.url());
        update(&state, &id, |e| {
            e.status = Some(RemoteStatus::Error);
            e.base_url = Some(server.url());
            e.error = Some("Unreachable: the link went away".into());
        });

        poll_once(&state, &id).await;

        probe.assert_async().await;
        assert_eq!(state.remote.status_of(&id), RemoteStatus::Connected);
        assert!(
            state.remote.snapshot()[0].error.is_none(),
            "the recovered connection still shows the error it recovered from"
        );
        assert_eq!(
            state.remote.base_url(&id).as_deref(),
            Some(server.url().as_str())
        );
    }

    /// A connection that broke retires its badges, once.
    ///
    /// A mirrored row's badge is sticky by construction: a session whose row
    /// stops being updated keeps rendering the state the machine was in when the
    /// link died — a remote tab frozen mid-question. Two failing probes must
    /// still announce one closure, not one every five seconds.
    #[tokio::test]
    async fn an_errored_connection_announces_its_sessions_closed_once() {
        let state = test_state();
        let id = direct_connection(&state, "http://127.0.0.1:1");
        crate::remote_mirror::store_seed_for_test(
            &state,
            &id,
            vec![crate::mcp_http::types::SessionInfo {
                session_id: "s1".into(),
                ..Default::default()
            }],
        );
        update(&state, &id, |e| {
            e.status = Some(RemoteStatus::Connected);
            e.base_url = Some("http://127.0.0.1:1".into());
        });
        let mut events = state.event_bus.subscribe();

        poll_once(&state, &id).await;
        poll_once(&state, &id).await;

        assert_eq!(state.remote.status_of(&id), RemoteStatus::Error);
        assert_eq!(
            drain_closed_sessions(&mut events),
            vec!["s1"],
            "the badge must be retired exactly once"
        );
        assert!(crate::remote_mirror::mirrored_rows(&state).is_empty());
    }

    /// Two probes against one daemon share one TCP connection.
    ///
    /// `Client::new()` per call threw the connection pool away each time, so
    /// every 5s heartbeat paid a fresh handshake against a daemon it had just
    /// talked to. The counter is the only honest way to see it: the probe
    /// answers identically either way.
    #[tokio::test]
    async fn the_probes_share_one_pooled_client() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let accepted = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&accepted);
        let daemon = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                counter.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut request = [0u8; 2048];
                    // Empty bodies: nothing to drain means nothing that can stop
                    // the connection going back to the pool.
                    while matches!(socket.read(&mut request).await, Ok(n) if n > 0) {
                        if socket
                            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n")
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                });
            }
        });

        let runtime = RemoteRuntime::default();
        let base = format!("http://{addr}");
        for _ in 0..2 {
            assert_eq!(
                probe_authenticated(&runtime.http_client(), &base, None).await,
                Probe::Ok
            );
        }

        assert_eq!(
            accepted.load(Ordering::SeqCst),
            1,
            "the second probe opened a second connection — the pool was thrown away"
        );
        daemon.abort();
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
        teardown(&state, &id);
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
