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

/// First wait after a connect attempt that failed, and the ceiling it doubles
/// towards. The floor is not zero on purpose: a machine that is off answers its
/// TCP connect instantly with a refusal, so a retry with no floor is a hot loop
/// wearing a backoff's clothes.
const RETRY_BACKOFF_MIN: Duration = Duration::from_secs(2);
const RETRY_BACKOFF_MAX: Duration = Duration::from_secs(60);
/// Fraction of the current backoff spread randomly across each wait.
///
/// Without it, N machines registered against one laptop all fail at the same
/// suspend and then retry in lockstep for as long as they stay down — the
/// thundering herd is self-inflicted and costs nothing to avoid.
const RETRY_JITTER: f64 = 0.25;
/// How often a supervisor re-reads a status somebody else is moving.
///
/// Short because it is the gap between an explicit connect settling and the
/// supervisor taking over, and it costs one in-memory read of a status it
/// already holds — never a request. A handshake over SSH can take
/// [`TUNNEL_CONNECT_TIMEOUT`], so a heartbeat-sized wait here would be a
/// two-minute hole in the retry schedule for a machine that failed at second
/// one.
const CONNECTING_POLL: Duration = Duration::from_millis(250);

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
    /// The task that owns this connection's whole lifecycle: bring it up, keep
    /// it up, retry while it is down. Its presence IS the desired state — there
    /// is deliberately no second `wanted_up` flag to fall out of sync with it,
    /// because `teardown` removes the entry and a connection absent from the map
    /// is one nobody asked for.
    supervisor: Option<tokio::task::JoinHandle<()>>,
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
    /// Bumped by [`teardown`], read by every supervisor.
    ///
    /// `teardown` removes the entry and aborts, but an abort only lands at the
    /// task's next await — and a supervisor sitting inside `connect_inner` has
    /// several, after which `claim_for_connect` inserts the entry again. Without
    /// this the window is small and the consequence is not: an explicit
    /// Disconnect silently undone a moment later by the task that was supposed
    /// to have stopped. A supervisor holding a stale generation cleans up after
    /// itself and returns instead.
    generations: DashMap<String, u64>,
    /// Serialises the two operations that decide whether an entry EXISTS:
    /// [`claim_and_supervise`] inserting one and [`teardown`] removing one.
    ///
    /// DashMap's per-entry lock cannot do this, because the two read and write
    /// different maps: `teardown` bumps the generation and then removes the
    /// entry, while a claim reads the generation and then inserts one. Ordered
    /// `bump, remove, read, insert` the claim is refused; interleaved
    /// `read, bump, remove, insert` it is not, and the entry the user asked to
    /// remove is back. One mutex over both steps is the whole fix. Nothing
    /// awaits while holding it, so it can be a plain `std::sync::Mutex`.
    lifecycle: std::sync::Mutex<()>,
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
            generations: DashMap::new(),
            lifecycle: std::sync::Mutex::new(()),
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

    /// The generation a supervisor spawned now belongs to.
    fn generation(&self, id: &str) -> u64 {
        self.generations.get(id).map(|g| *g).unwrap_or(0)
    }

    /// Retire every supervisor currently running for `id`.
    fn retire_generation(&self, id: &str) {
        *self.generations.entry(id.to_string()).or_default() += 1;
    }

    /// The guard over "does this entry exist", held by the three places that
    /// decide it. See the [`lifecycle`](Self::lifecycle) field.
    ///
    /// A poisoned mutex is recovered rather than propagated: it guards the
    /// ordering of two map operations, not an invariant a panic could have left
    /// half-written, and refusing to manage connections for the rest of the
    /// process is a worse answer than carrying on.
    fn lifecycle(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lifecycle.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Take back an entry that a retired supervisor put here after its
    /// `teardown`, and only that.
    ///
    /// The predicate is the whole point. Every legitimate owner installs its
    /// supervisor handle under the [`lifecycle`](Self::lifecycle) lock, in the
    /// same critical section that creates the entry, so an entry carrying no
    /// handle cannot belong to anyone: it is the husk an aborted handshake left
    /// behind when its `update` re-created what `teardown` had just removed. An
    /// entry that does carry one belongs to a Connect that arrived after the
    /// teardown, and is not the retired task's to remove.
    fn discard_unowned(&self, id: &str) -> Option<Entry> {
        let _lifecycle = self.lifecycle();
        self.entries
            .remove_if(id, |_, e| e.supervisor.is_none())
            .map(|(_, entry)| entry)
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
/// authenticate, then start the mirror.
///
/// Idempotent while in flight: a second call on a connecting or connected
/// connection is a no-op, so a double click cannot open two tunnels.
///
/// The caller gets the outcome of THIS attempt, which is what a person pressing
/// Connect is owed — but the attempt is not the whole story. A
/// [`spawn_supervisor`] is left running either way, so a failure returned here
/// is one the app will keep working on by itself rather than a final answer.
///
/// **The claim is taken before the supervisor exists, and that ordering is the
/// contract.** Spawned first, the supervisor's own loop woke on `Disconnected`,
/// won the claim, and did the connecting — leaving this function to find a
/// `Connecting` connection and return `Ok(())` for an attempt it never made. A
/// person pressing Connect on a machine that is off then saw success. With the
/// claim held first the supervisor sees `Connecting` and parks, which is what
/// its `Connecting` arm is for.
pub(crate) async fn connect(state: &Arc<AppState>, id: &str) -> Result<(), String> {
    // Resolved here as well as in `attempt`, so an id that is not configured is
    // an error for the caller rather than a supervisor spawned for nothing.
    let connection = load_connection(state, id)?;
    let generation = state.remote.generation(id);
    let Some(connecting) = claim_and_supervise(state, id, generation) else {
        // Someone already owns this: a double click, or a supervisor mid-retry.
        // It is up or on its way, and a second tunnel is exactly what the claim
        // exists to prevent.
        return Ok(());
    };
    // Run on its own task, so a caller that goes away cannot stop this halfway.
    //
    // `POST /config/remote-connections/{id}/connect` is awaited inside an axum
    // handler under the router's `REQUEST_TIMEOUT`, which DROPS the handler
    // future when it fires — and a browser that navigates away does the same.
    // The claim below is already in the map by then, so the connection was left
    // in `Connecting` with nothing running and nothing coming: the same claim
    // then made every later connect a silent no-op, and the machine could not
    // be brought up again without restarting the app.
    let state = Arc::clone(state);
    let id = id.to_string();
    match tokio::spawn(
        async move { attempt(&state, &id, connection, connecting, generation).await },
    )
    .await
    {
        Ok(outcome) => outcome,
        // Re-raised for the same reason as the unattended turn's: with the body
        // inline a panic unwound through the caller, and "failed to connect" is
        // the wrong sentence for a bug of ours.
        Err(join) => std::panic::resume_unwind(join.into_panic()),
    }
}

/// Take the claim and make the attempt, for a caller that is not reporting the
/// outcome to anyone — the supervisor's retry.
async fn connect_inner(state: &Arc<AppState>, id: &str, generation: u64) -> Result<(), String> {
    let connection = load_connection(state, id)?;
    let Some(connecting) = claim_and_supervise(state, id, generation) else {
        return Ok(());
    };
    attempt(state, id, connection, connecting, generation).await
}

/// Everything between a held claim and a settled status.
///
/// Split from [`connect_inner`] so that `connect` can hold the claim itself —
/// see its doc comment — without the two growing separate handshakes.
async fn attempt(
    state: &Arc<AppState>,
    id: &str,
    connection: RemoteConnection,
    connecting: RemoteConnectionStatus,
    generation: u64,
) -> Result<(), String> {
    publish(state, &connecting);
    tracing::info!(source = "remote", connection = id, name = %connection.name, "Connecting");

    let settled = handshake(state, id, &connection).await;
    // A Disconnect that arrived while we were in flight wins, and must be seen
    // BEFORE either branch below writes: both go through `update`, which
    // re-creates the entry `teardown` removed. Without this, a handshake started
    // a moment before a Disconnect still published `Connected`, with its token
    // and its tunnel, over a connection the user had just put down.
    if state.remote.generation(id) != generation {
        if let Some(husk) = state.remote.discard_unowned(id) {
            dispose(state, id, husk);
        }
        return Err("disconnected while connecting".to_string());
    }
    match settled {
        Ok(token) => {
            update(state, id, |e| {
                e.status = Some(RemoteStatus::Connected);
                e.token = token;
                e.error = None;
            });
            // No heartbeat is started here on purpose: the supervisor owns it,
            // and it is already running — either because `connect` spawned it
            // or because this call came from inside it.
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

/// Move a connection to `Connecting` and make sure something is supervising it,
/// or report that someone else already has.
///
/// The check and the transition share ONE `entry()` scope, so the shard lock
/// holds across both. Read-then-write across two DashMap calls is a TOCTOU: two
/// concurrent connects — a double click, or the UI and an auto-connect racing at
/// startup — both read `Disconnected`, both wrote `Connecting`, and both went on
/// to open a tunnel.
///
/// `generation` is the caller's, and a caller whose generation has been retired
/// is refused: it is a supervisor that a [`teardown`] already stopped, and the
/// entry it would insert here is the Disconnect being silently undone. The
/// [`lifecycle`](RemoteRuntime::lifecycle) lock is what makes that check mean
/// something — it holds across the generation read AND the insert, on the same
/// mutex `teardown` holds across its bump and its removal.
///
/// The supervisor is started in the same critical section so that an entry
/// never exists without one, which is the predicate
/// [`discard_unowned`](RemoteRuntime::discard_unowned) decides ownership by.
///
/// Returns the snapshot to announce, or `None` when the connection is already up
/// or on its way — or when the caller has been retired.
fn claim_and_supervise(
    state: &Arc<AppState>,
    id: &str,
    generation: u64,
) -> Option<RemoteConnectionStatus> {
    let _lifecycle = state.remote.lifecycle();
    if state.remote.generation(id) != generation {
        return None;
    }
    let claimed = {
        let mut entry = state.remote.entries.entry(id.to_string()).or_default();
        if matches!(
            entry.status,
            Some(RemoteStatus::Connecting | RemoteStatus::Connected)
        ) {
            None
        } else {
            entry.status = Some(RemoteStatus::Connecting);
            entry.error = None;
            Some(entry.snapshot(id))
        }
    };
    // Whether or not we took the claim: a connection somebody is connecting is
    // still one that needs something to keep it up afterwards.
    spawn_supervisor(state, id.to_string());
    claimed
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
    // The bump and the removal are one critical section, under the same mutex
    // `claim_and_supervise` holds across its generation read and its insert.
    // Separately they interleave: a supervisor that read its generation before
    // the bump can insert after the removal, and the connection the user put
    // down is back up.
    //
    // The bump is unconditional and comes first: a supervisor may be mid-connect
    // with no entry in the map yet, and retiring its generation is the only
    // thing that stops it. Bumping only when an entry exists would miss exactly
    // the task that is about to create one.
    let removed = {
        let _lifecycle = state.remote.lifecycle();
        state.remote.retire_generation(id);
        state.remote.entries.remove(id)
    };
    // Outside the lock: nothing in `dispose` decides whether an entry exists,
    // and it aborts tasks, stops a tunnel and publishes.
    let Some((_, entry)) = removed else {
        return;
    };
    dispose(state, id, entry);
}

/// Stop everything one entry owns and announce the departure.
///
/// Split out of [`teardown`] because a retired supervisor has to run the same
/// steps on an entry it resurrected, while NOT retiring a generation — doing
/// that would take down the supervisor of a Connect that arrived in between.
fn dispose(state: &Arc<AppState>, id: &str, mut entry: Entry) {
    if let Some(supervisor) = entry.supervisor.take() {
        supervisor.abort();
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

/// The wait before the next connect attempt, grown and jittered.
///
/// Split out so the growth is testable without waiting for it: the property
/// that matters is that it doubles, stops at the ceiling, and never returns the
/// same value twice in a row for two connections that failed together.
fn next_backoff(current: Duration) -> Duration {
    let doubled = current.saturating_mul(2).min(RETRY_BACKOFF_MAX);
    doubled.max(RETRY_BACKOFF_MIN)
}

/// `base` with up to [`RETRY_JITTER`] of itself added.
fn jittered(base: Duration) -> Duration {
    // Nanos of the monotonic clock: a cheap, dependency-free source of spread.
    // This picks WHEN to retry, never WHAT is sent, so it does not need to be a
    // cryptographic random and must not pull in a generator that is.
    let spread = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| f64::from(d.subsec_nanos()) / 1e9)
        .unwrap_or(0.0);
    base + base.mul_f64(RETRY_JITTER * spread)
}

/// One task per connection, owning its whole lifecycle.
///
/// **Why one task and not two.** Before this, the heartbeat was spawned only
/// from the SUCCESS branch of [`connect_inner`], so a connection that failed its
/// first attempt got no task at all and never retried — the self-heal documented
/// on [`poll_survives`] only ever protected a connection that had been up once
/// in this process. Adding a second, retry-only task beside the heartbeat would
/// have left two loops racing to own one connection. This is the same loop:
///
/// * not connected -> [`connect_inner`], then back off and try again;
/// * connected     -> [`poll_once`] every [`STATUS_POLL`], backoff reset.
///
/// `connect_inner` is what retries must call, not `poll_once`: the error path
/// runs `stop_tunnel`, so on the SSH transport the stored base URL points at a
/// forwarded port that is no longer listening. Only a real connect rebuilds the
/// tunnel, and a probe-only retry would spin against that dead port forever.
///
/// Racing with an explicit connect is free: an explicit [`connect`] takes the
/// claim before it spawns us, so the `Connecting` arm below parks rather than
/// starting a second handshake — and the caller keeps the outcome that is
/// rightfully theirs to report.
///
/// The entry guard is held across the `tokio::spawn`, so the entry and its
/// handle appear together. There is no await in between, and the atomicity is
/// load bearing: an entry seen without a handle is what
/// [`discard_unowned`](RemoteRuntime::discard_unowned) removes.
fn spawn_supervisor(state: &Arc<AppState>, id: String) {
    let mut entry = state.remote.entries.entry(id.clone()).or_default();
    // A live supervisor already IS the desired state; replacing it would
    // reset a backoff that is deliberately wide. A finished one is the
    // `Unauthenticated` exit below, and a fresh credential must be able to
    // start it again.
    match entry.supervisor.as_ref() {
        Some(handle) if !handle.is_finished() => return,
        _ => entry.supervisor = None,
    }
    let task_state = Arc::clone(state);
    let task_id = id.clone();
    let generation = state.remote.generation(&id);
    let handle = tokio::spawn(async move {
        let mut backoff = RETRY_BACKOFF_MIN;
        loop {
            // First thing after every await, including the connect below. A
            // stale generation means `teardown` ran while we were mid-flight, so
            // anything this task put back has to come out again — `teardown` is
            // idempotent and total, which is exactly the cleanup needed.
            if task_state.remote.generation(&task_id) != generation {
                if let Some(entry) = task_state.remote.discard_unowned(&task_id) {
                    dispose(&task_state, &task_id, entry);
                }
                return;
            }
            match task_state.remote.status_of(&task_id) {
                RemoteStatus::Connected => {
                    backoff = RETRY_BACKOFF_MIN;
                    tokio::time::sleep(STATUS_POLL).await;
                    // Re-read rather than trusting the status we slept on: a
                    // teardown during the sleep must not be followed by a probe
                    // that re-conjures the entry it just removed.
                    if !poll_survives(task_state.remote.status_of(&task_id)) {
                        return;
                    }
                    poll_once(&task_state, &task_id).await;
                }
                // The credential is wrong. Retrying the same one only asks the
                // daemon to rate-limit us, and no amount of waiting turns a bad
                // password into a good one — this needs a person. `connect`
                // spawns us again once they supply one.
                RemoteStatus::Unauthenticated => return,
                // Somebody else's attempt is in flight — an explicit `connect`,
                // which claims before it spawns us. Watch, do not join in, and
                // do not grow the backoff on an attempt that is not ours.
                RemoteStatus::Connecting => tokio::time::sleep(CONNECTING_POLL).await,
                // Disconnected or Error. Disconnected is also where a fresh
                // supervisor starts, so this arm must attempt rather than bail:
                // a teardown is recognised by the generation above, never by the
                // absence of an entry, which is indistinguishable from the boot
                // state `autoconnect_all` spawns into.
                _ => {
                    let _ = connect_inner(&task_state, &task_id, generation).await;
                    if task_state.remote.status_of(&task_id) != RemoteStatus::Connected {
                        tokio::time::sleep(jittered(backoff)).await;
                        backoff = next_backoff(backoff);
                    }
                }
            }
        }
    });
    entry.supervisor = Some(handle);
}

/// Ensure every configured connection has a supervisor, so a machine the user
/// registered is one the app brings up by itself.
///
/// Called once at startup. Nothing used to connect a remote machine on boot —
/// `auto_connect_saved_upstreams` next door covers upstream MCP servers and not
/// these — so every restart left every remote repository pointing at a machine
/// the app had decided not to talk to, and the only cure was a trip into
/// Settings.
///
/// Registered means wanted: there is no per-connection opt-out today, and the
/// desired-state shape above is what makes adding one later a single condition.
pub(crate) fn autoconnect_all(state: &Arc<AppState>) {
    let connections = match RemoteConnectionStore::load(&state.data_dir) {
        Ok(connections) => connections,
        Err(e) => {
            tracing::warn!(source = "remote", error = %e, "Cannot read remote connections to autoconnect");
            return;
        }
    };
    for connection in connections {
        tracing::info!(
            source = "remote",
            connection = %connection.id,
            name = %connection.name,
            "Autoconnecting remote machine"
        );
        spawn_supervisor(state, connection.id);
    }
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

    /// Whether the connection's supervisor ends on its own within a bound.
    ///
    /// A bound rather than one `yield_now`: the task may still be queued when
    /// the assertion runs, and "not finished yet" would read exactly like "it is
    /// still probing". The budget is setup reaching a state, not the behaviour
    /// under test, so it is sized so it cannot plausibly fail.
    async fn supervisor_finishes(state: &Arc<AppState>, id: &str) -> bool {
        for _ in 0..200 {
            let finished = state
                .remote
                .entries
                .get(id)
                .and_then(|e| e.supervisor.as_ref().map(|h| h.is_finished()));
            if finished == Some(true) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        false
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
    ///
    /// Appended, not written over the store: `save` takes the WHOLE list, so a
    /// second call used to delete the first connection. Only a test that reads
    /// the store as a list — `autoconnect_all` — could see it, and it saw one
    /// machine where it had registered two.
    fn direct_connection(state: &Arc<AppState>, url: &str) -> String {
        let connection = crate::remote_connection::RemoteConnection::new_direct(
            "vps",
            url.trim_end_matches('/'),
            "boss",
        );
        let id = connection.id.clone();
        let mut connections = RemoteConnectionStore::load(&state.data_dir).unwrap_or_default();
        connections.push(connection);
        RemoteConnectionStore::save(&state.data_dir, &connections).unwrap();
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
        // The supervisor is spawned by `connect` before the attempt, so the
        // invariant is no longer "no task exists" but "the task gave up": a bad
        // password must not be re-sent every five seconds until the daemon
        // rate-limits us. It ends by itself on `Unauthenticated`.
        assert!(
            supervisor_finishes(&state, &id).await,
            "a rejected connection must not keep probing"
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

    /// A caller that goes away must not strand a connection in `Connecting`.
    ///
    /// `POST .../connect` is awaited inside an axum handler under the router's
    /// `REQUEST_TIMEOUT`, which drops the handler future when it fires, and a
    /// browser that navigates away does the same. `claim_for_connect` has
    /// already written `Connecting` by then, so the handshake stopped where it
    /// stood and the claim stayed: every later connect saw `Connecting`,
    /// returned `Ok(())` without doing anything, and the machine could not be
    /// brought up again short of restarting the app.
    ///
    /// **The budget is zero on purpose, and that makes the drop deterministic.**
    /// `timeout` polls the inner future before it polls its own sleep, so
    /// `connect` always runs far enough to spawn; a deadline already in the past
    /// is then unconditionally ready, so the caller is dropped at its first
    /// await every time. A 1 ms budget read the same on an idle machine and did
    /// not under load — the spawned handshake answered a local mock before the
    /// timer wheel was next inspected, `timeout` found the inner future ready,
    /// and the test failed claiming a caller had not been dropped when what had
    /// really happened is that it succeeded.
    #[tokio::test]
    async fn a_dropped_connect_does_not_strand_a_connection_in_connecting() {
        let mut server = mockito::Server::new_async().await;
        let _health = server
            .mock("GET", "/health")
            .with_body("{}")
            .create_async()
            .await;
        let _probe = server
            .mock("GET", "/api/version")
            .with_body("{}")
            .create_async()
            .await;

        let state = test_state();
        let id = direct_connection(&state, &server.url());

        tokio::time::timeout(Duration::ZERO, connect(&state, &id))
            .await
            .expect_err("the handshake must still be in flight when the caller is dropped");

        // 5s is a hang bound: a handshake against a local mock takes
        // milliseconds. What is asserted is that it finished at all.
        for _ in 0..500 {
            if state.remote.status_of(&id) == RemoteStatus::Connected {
                teardown(&state, &id);
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!(
            "the dropped caller left the connection at {:?}",
            state.remote.status_of(&id)
        );
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
    #[tokio::test]
    async fn only_one_connect_can_claim_a_connection() {
        let state = test_state();

        let first = claim_and_supervise(&state, "vps", 0).expect("a fresh connection is free");
        assert_eq!(first.status, RemoteStatus::Connecting);
        assert_eq!(
            state.remote.status_of("vps"),
            RemoteStatus::Connecting,
            "the claim must land in the map, not only in the caller's hand"
        );
        assert!(
            state
                .remote
                .entries
                .get("vps")
                .is_some_and(|e| e.supervisor.is_some()),
            "an entry without a supervisor is one `discard_unowned` may remove"
        );
        assert!(
            claim_and_supervise(&state, "vps", 0).is_none(),
            "a second connect claimed a connection that is already coming up"
        );

        // A connected one is equally claimed, and a failed one is free again.
        update(&state, "vps", |e| e.status = Some(RemoteStatus::Connected));
        assert!(claim_and_supervise(&state, "vps", 0).is_none());
        update(&state, "vps", |e| e.status = Some(RemoteStatus::Error));
        assert!(
            claim_and_supervise(&state, "vps", 0).is_some(),
            "a connection that failed must be retryable"
        );

        // And a caller `teardown` already retired is refused, whatever the
        // status: its claim would be the Disconnect undone.
        state.remote.retire_generation("vps");
        update(&state, "vps", |e| e.status = Some(RemoteStatus::Error));
        assert!(
            claim_and_supervise(&state, "vps", 0).is_none(),
            "a retired supervisor claimed a connection it no longer owns"
        );

        teardown(&state, "vps");
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

    // -----------------------------------------------------------------------
    // Supervisor: autoconnect, retry, and the two ways it must stop
    // -----------------------------------------------------------------------

    #[test]
    fn the_backoff_doubles_up_to_the_ceiling_and_stops_there() {
        let mut wait = RETRY_BACKOFF_MIN;
        let mut seen = vec![wait];
        for _ in 0..12 {
            wait = next_backoff(wait);
            seen.push(wait);
        }
        assert_eq!(seen[0], RETRY_BACKOFF_MIN);
        assert_eq!(seen[1], RETRY_BACKOFF_MIN * 2);
        assert!(
            seen.windows(2).all(|w| w[1] >= w[0]),
            "a backoff that shrinks re-hammers a machine that is still down: {seen:?}"
        );
        assert_eq!(
            *seen.last().unwrap(),
            RETRY_BACKOFF_MAX,
            "the wait must settle at the ceiling instead of growing without bound"
        );
    }

    #[test]
    fn jitter_only_ever_adds_and_stays_inside_its_share() {
        for base in [RETRY_BACKOFF_MIN, RETRY_BACKOFF_MAX] {
            let jittered = jittered(base);
            assert!(
                jittered >= base,
                "jitter that subtracts can retry sooner than the backoff allows"
            );
            assert!(
                jittered <= base.mul_f64(1.0 + RETRY_JITTER),
                "jitter must spread the herd, not extend the outage"
            );
        }
    }

    /// The gap this whole mechanism exists to close.
    ///
    /// The heartbeat used to be spawned only from the success branch of
    /// `connect_inner`, so a connection whose FIRST attempt failed got no task
    /// at all: it sat in `Error` until a person pressed Connect. Measured on a
    /// live instance — mac-mint answered 200 throughout while the app showed it
    /// unreachable for forty minutes.
    #[tokio::test]
    async fn a_connect_that_fails_still_leaves_something_retrying() {
        let state = test_state();
        // A port nothing answers on, so the attempt fails at connect time rather
        // than on a status code — the same shape as a machine that is asleep.
        let id = direct_connection(&state, "http://127.0.0.1:1");

        connect(&state, &id).await.unwrap_err();

        assert_eq!(state.remote.status_of(&id), RemoteStatus::Error);
        let supervising = state
            .remote
            .entries
            .get(&id)
            .and_then(|e| e.supervisor.as_ref().map(|h| !h.is_finished()));
        assert_eq!(
            supervising,
            Some(true),
            "a failed connect left nothing to try again, so the machine can only \
             come back if a person notices the red badge"
        );
        teardown(&state, &id);
    }

    /// Registered means wanted: nothing used to connect a remote machine at boot.
    #[tokio::test]
    async fn autoconnect_supervises_every_configured_machine() {
        let state = test_state();
        let first = direct_connection(&state, "http://127.0.0.1:1");
        let second = direct_connection(&state, "http://127.0.0.1:2");

        autoconnect_all(&state);

        for id in [&first, &second] {
            assert!(
                state
                    .remote
                    .entries
                    .get(id)
                    .and_then(|e| e.supervisor.as_ref().map(|h| !h.is_finished()))
                    .unwrap_or(false),
                "{id} was registered but nothing is bringing it up"
            );
            teardown(&state, id);
        }
    }

    /// The failure mode a retry loop introduces, and the reason for the
    /// generation counter: a Disconnect the user asked for must stay done.
    ///
    /// Without it the supervisor wakes from its backoff, calls `connect_inner`,
    /// and `claim_for_connect` inserts the entry `teardown` just removed — the
    /// machine reconnects itself moments after being told not to.
    #[tokio::test]
    async fn an_explicit_disconnect_is_not_undone_by_the_retry() {
        let state = test_state();
        let id = direct_connection(&state, "http://127.0.0.1:1");

        connect(&state, &id).await.unwrap_err();
        teardown(&state, &id);

        // Longer than the first backoff, so a supervisor that ignored the
        // teardown has had its chance to wake up and reconnect.
        tokio::time::sleep(RETRY_BACKOFF_MIN + Duration::from_millis(500)).await;

        assert_eq!(state.remote.status_of(&id), RemoteStatus::Disconnected);
        assert!(
            state.remote.entries.get(&id).is_none(),
            "a retired supervisor put the entry back: {:?}",
            state.remote.snapshot()
        );
    }

    /// Connect after Disconnect must work, which is what the generation counter
    /// is at risk of breaking: the retired supervisor and the new one share an
    /// id, and cleanup that is not scoped to its own task takes down the wrong
    /// one.
    #[tokio::test]
    async fn a_reconnect_after_a_disconnect_gets_its_own_supervisor() {
        let state = test_state();
        let id = direct_connection(&state, "http://127.0.0.1:1");

        connect(&state, &id).await.unwrap_err();
        teardown(&state, &id);
        connect(&state, &id).await.unwrap_err();

        tokio::time::sleep(RETRY_BACKOFF_MIN + Duration::from_millis(500)).await;

        assert!(
            state
                .remote
                .entries
                .get(&id)
                .and_then(|e| e.supervisor.as_ref().map(|h| !h.is_finished()))
                .unwrap_or(false),
            "the retired supervisor took the new one down with it"
        );
        teardown(&state, &id);
    }
}
