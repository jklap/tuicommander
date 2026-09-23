//! Remote connection config model.
//!
//! Persists named connections (SSH, Direct, or Local) to `connections.json`
//! in the app config directory. Each connection has a UUID, a human-readable
//! name, a transport, optional auth info, and an enabled flag.
//!
//! The SSH transport nests its settings as `ssh: SshConnectionParams` — the
//! same struct a `TunnelProfile` carries. Files written before that change
//! stored them flat (`ssh_host`/`ssh_port`/`ssh_user`/`identity_file`);
//! `RemoteTransport`'s reader still accepts that shape, and
//! [`migrate_legacy_connections_file`] rewrites it once at startup, keeping a
//! timestamped backup of the original next to it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::ssh_connection::{SshConnectionParams, StrictHostKeyChecking};

const CONNECTIONS_FILE: &str = "connections.json";

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A saved remote connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RemoteConnection {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) transport: RemoteTransport,
    /// The Basic Auth username, optional: a desktop TUICommander on the LAN
    /// with `lan_auth_bypass`, or a deployed daemon reached by pairing token,
    /// needs none. When a password IS stored and this is absent, the token
    /// exchange sends an empty username, which every daemon rejects — so a
    /// missing username fails closed rather than authenticating as anybody.
    /// The password half lives in the vault, never here
    /// (`Credential::RemoteConnection`).
    #[serde(default)]
    pub(crate) auth_username: Option<String>,
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) auto_update: bool,
    #[serde(default)]
    pub(crate) deploy: DeployMode,
    #[serde(default = "default_survive_secs")]
    pub(crate) survive_secs: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DeployMode {
    #[default]
    Never,
    OnConnect,
    Installed,
}

const fn default_survive_secs() -> u64 {
    1_800
}

/// What "Update & restart remote" reports for a `Local` connection. Connect
/// works (`remote_runtime::resolve_local_base_url`), but streaming a binary to
/// another instance on this machine would have it replace its own executable —
/// a local install is updated like one, so the runtime refuses instead.
/// Deployment needs SSH and refuses a Local transport on its own.
pub(crate) const LOCAL_TRANSPORT_UPDATE_UNSUPPORTED: &str = "Update & restart is not available for a Local connection: it is another instance on this machine — update that install directly";

/// Transport layer for a remote connection.
///
/// Serialized internally tagged (`"type": "Ssh" | "Direct" | "Local"`) with
/// the SSH settings nested under `ssh`. Deserialization goes through
/// [`RemoteTransportWire`], which also accepts the pre-nested flat SSH shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", try_from = "RemoteTransportWire")]
pub(crate) enum RemoteTransport {
    Ssh {
        /// Host/port/user/identity/keepalive config — shared with
        /// `tunnels::profile::TunnelProfile` via `SshConnectionParams`.
        /// `strict_host_key_checking` is stored but the runtime always uses
        /// `AcceptNew` for the tunnel it opens on the user's behalf
        /// (`remote_runtime::ssh_profile`).
        ssh: SshConnectionParams,
        remote_daemon_port: u16,
    },
    Direct {
        url: String,
        /// SHA-256 fingerprint (lowercase hex) of a pinned self-signed/
        /// untrusted certificate, set only after the user explicitly confirms
        /// it (`direct_proxy::probe_direct_tls` → `NeedsConfirmation`). When
        /// set, Connect talks to the daemon only through a relay that accepts
        /// exactly this certificate (`direct_proxy::DirectProxies`). `None` for
        /// `http://`, a CA-trusted `https://`, or a target not yet confirmed —
        /// and then an untrusted certificate fails Connect closed. Omitted from
        /// the file when `None`, so an unpinned connection keeps the exact shape
        /// older builds wrote (they ignore the key when present).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tls_fingerprint: Option<String>,
    },
    /// Another named/isolated TUICommander instance running on this same
    /// machine (`tuic-remote --instance <id>`, or `TUIC_APP_INSTANCE=<id>`
    /// for the desktop app). Exactly one of `port`/`instance_id` is set:
    /// when `instance_id` is set, the real port is resolved by reading that
    /// instance's own `config.json` off disk at connect time (see
    /// `resolve_local_instance_port`), never cached, so a restarted instance
    /// that landed on a different port via the 9876→9877→9878 retry chain
    /// doesn't leave a stale port behind. `port` alone covers the unnamed
    /// case (e.g. a plain `make dev` second debug instance on 9877, which
    /// has no `instances/<id>/` directory to discover).
    ///
    /// Connect resolves it to `http://127.0.0.1:<port>` with the ordinary
    /// token handshake; update refuses it with
    /// [`LOCAL_TRANSPORT_UPDATE_UNSUPPORTED`] and deployment (SSH only) refuses
    /// it too.
    Local {
        port: Option<u16>,
        instance_id: Option<String>,
    },
}

/// Both accepted shapes of a [`RemoteTransport`].
#[derive(Deserialize)]
#[serde(tag = "type")]
enum RemoteTransportWire {
    Ssh(SshTransportWire),
    Direct {
        url: String,
        #[serde(default)]
        tls_fingerprint: Option<String>,
    },
    Local {
        #[serde(default)]
        port: Option<u16>,
        #[serde(default)]
        instance_id: Option<String>,
    },
}

/// An `Ssh` transport either nested (`ssh: {...}`) or flat, as every build
/// before the nested model wrote it. Exactly one of the two must be present.
#[derive(Deserialize)]
struct SshTransportWire {
    #[serde(default)]
    ssh: Option<SshConnectionParams>,
    #[serde(default)]
    ssh_host: Option<String>,
    #[serde(default)]
    ssh_port: Option<u16>,
    #[serde(default)]
    ssh_user: Option<String>,
    #[serde(default)]
    identity_file: Option<String>,
    remote_daemon_port: u16,
}

/// The flat shape never stored keepalive, host-key or compression settings;
/// these reproduce what it actually ran with (`remote_runtime::ssh_profile`
/// built every tunnel with keepalive 15/3, `Compression=yes` and
/// `StrictHostKeyChecking=accept-new`).
fn legacy_remote_ssh_params(
    host: String,
    port: u16,
    user: String,
    identity_file: Option<String>,
) -> SshConnectionParams {
    let mut ssh = SshConnectionParams::new(host, user);
    ssh.port = port;
    ssh.identity_file = identity_file.map(PathBuf::from);
    ssh.strict_host_key_checking = StrictHostKeyChecking::AcceptNew;
    ssh
}

impl TryFrom<RemoteTransportWire> for RemoteTransport {
    type Error = String;

    fn try_from(wire: RemoteTransportWire) -> Result<Self, String> {
        Ok(match wire {
            RemoteTransportWire::Direct {
                url,
                tls_fingerprint,
            } => Self::Direct {
                url,
                tls_fingerprint,
            },
            RemoteTransportWire::Local { port, instance_id } => Self::Local { port, instance_id },
            RemoteTransportWire::Ssh(ssh) => {
                let flat = ssh.ssh_host.is_some()
                    || ssh.ssh_port.is_some()
                    || ssh.ssh_user.is_some()
                    || ssh.identity_file.is_some();
                let params = match (ssh.ssh, flat) {
                    (Some(params), false) => params,
                    (Some(_), true) => {
                        return Err("Ssh transport has both a nested `ssh` object and legacy \
                             flat ssh_host/ssh_port/ssh_user/identity_file fields"
                            .to_string());
                    }
                    (None, _) => {
                        let missing = |field: &str| format!("missing field `{field}`");
                        legacy_remote_ssh_params(
                            ssh.ssh_host.ok_or_else(|| missing("ssh"))?,
                            ssh.ssh_port.ok_or_else(|| missing("ssh_port"))?,
                            ssh.ssh_user.ok_or_else(|| missing("ssh_user"))?,
                            ssh.identity_file,
                        )
                    }
                };
                Self::Ssh {
                    ssh: params,
                    remote_daemon_port: ssh.remote_daemon_port,
                }
            }
        })
    }
}

impl RemoteConnection {
    /// Create a new SSH connection with default port (22) and daemon port (9877).
    pub(crate) fn new_ssh(
        name: impl Into<String>,
        host: impl Into<String>,
        user: impl Into<String>,
    ) -> Self {
        let ssh_user = user.into();
        let mut ssh = SshConnectionParams::new(host, ssh_user.clone());
        ssh.strict_host_key_checking = StrictHostKeyChecking::AcceptNew;
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            transport: RemoteTransport::Ssh {
                ssh,
                remote_daemon_port: 9877,
            },
            auth_username: Some(ssh_user),
            enabled: true,
            auto_update: false,
            deploy: DeployMode::Never,
            survive_secs: default_survive_secs(),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if uuid::Uuid::parse_str(&self.id).is_err() {
            return Err("id must be a valid UUID".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("name must not be empty".to_string());
        }
        match &self.transport {
            RemoteTransport::Ssh { ssh, .. } => {
                ssh.validate()?;
            }
            RemoteTransport::Direct {
                url,
                tls_fingerprint,
            } => {
                if url.trim().is_empty() {
                    return Err("url must not be empty".to_string());
                }
                if let Some(pin) = tls_fingerprint {
                    if crate::direct_proxy::normalize_fingerprint(pin).is_none() {
                        return Err(
                            "tls_fingerprint must be a SHA-256 fingerprint (64 hex digits)"
                                .to_string(),
                        );
                    }
                    if !matches!(crate::direct_proxy::https_target(url), Ok(Some(_))) {
                        return Err("tls_fingerprint requires an https:// url".to_string());
                    }
                }
            }
            RemoteTransport::Local { port, instance_id } => {
                let has_instance = instance_id.as_ref().is_some_and(|s| !s.trim().is_empty());
                match (port, has_instance) {
                    (None, false) => {
                        return Err(
                            "Local connection requires either a port or an instance_id".to_string()
                        );
                    }
                    (Some(_), true) => {
                        return Err(
                            "Local connection must specify either a port or an instance_id, not both"
                                .to_string(),
                        );
                    }
                    (Some(0), false) => {
                        return Err("port must be in range 1-65535".to_string());
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Create a new Direct connection.
    pub(crate) fn new_direct(
        name: impl Into<String>,
        url: impl Into<String>,
        auth_username: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            transport: RemoteTransport::Direct {
                url: url.into(),
                tls_fingerprint: None,
            },
            auth_username: Some(auth_username.into()),
            enabled: true,
            auto_update: false,
            deploy: DeployMode::Never,
            survive_secs: default_survive_secs(),
        }
    }

    /// Create a new Local connection pointing at a named instance, resolved
    /// by instance id rather than a manually-entered port.
    #[cfg(test)]
    pub(crate) fn new_local_instance(
        name: impl Into<String>,
        instance_id: impl Into<String>,
    ) -> Self {
        Self::new_local(
            name,
            RemoteTransport::Local {
                port: None,
                instance_id: Some(instance_id.into()),
            },
        )
    }

    /// Create a new Local connection pointing at a manually-entered port
    /// (the unnamed-instance case — e.g. a plain `make dev` second debug
    /// instance, which has no `instances/<id>/` directory to discover).
    #[cfg(test)]
    pub(crate) fn new_local_port(name: impl Into<String>, port: u16) -> Self {
        Self::new_local(
            name,
            RemoteTransport::Local {
                port: Some(port),
                instance_id: None,
            },
        )
    }

    #[cfg(test)]
    fn new_local(name: impl Into<String>, transport: RemoteTransport) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            transport,
            auth_username: None,
            enabled: true,
            auto_update: false,
            deploy: DeployMode::Never,
            survive_secs: default_survive_secs(),
        }
    }
}

// ---------------------------------------------------------------------------
// Local instance port resolution
// ---------------------------------------------------------------------------

/// Distinguishes "this instance id has no on-disk config directory at all"
/// (a typo, or an instance that was never started) from "the directory
/// exists but its `config.json` couldn't be read/parsed" (a real instance,
/// transient or corrupt state) — Test Connection needs to tell these apart in
/// its UI, and Connect needs the same distinction to give a useful error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LocalInstancePortError {
    InstanceNotFound,
    Unreadable(String),
}

impl std::fmt::Display for LocalInstancePortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InstanceNotFound => write!(f, "instance not found"),
            Self::Unreadable(msg) => write!(f, "{msg}"),
        }
    }
}

/// Minimal shape of `config.json` needed to pull out
/// `services.server.port` without dragging in the full `AppConfig` type
/// (and its many `#[serde(default = ...)]` helpers) into this module. Missing
/// sub-objects default to port 0, which `resolve_local_instance_port_at`
/// never accepts — a `config.json` with no `services.server.port` key at all
/// is at least as "unreadable" as one with a bad type.
#[derive(Debug, Default, Deserialize)]
struct MinimalAppConfigForPort {
    #[serde(default)]
    services: MinimalServicesConfigForPort,
}

#[derive(Debug, Default, Deserialize)]
struct MinimalServicesConfigForPort {
    #[serde(default)]
    server: MinimalServerConfigForPort,
}

#[derive(Debug, Default, Deserialize)]
struct MinimalServerConfigForPort {
    #[serde(default)]
    port: u16,
}

/// Resolve a named instance's `remote_access_port` (`services.server.port`
/// in `config.json`) by reading that instance's own config directory off
/// disk, using the real platform config dir and home dir. Re-read on every
/// call, never cached — see `RemoteTransport::Local`'s doc comment for why.
pub(crate) fn resolve_local_instance_port(
    instance_id: &str,
) -> Result<u16, LocalInstancePortError> {
    let platform_config = dirs::config_dir();
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    resolve_local_instance_port_at(instance_id, platform_config.as_deref(), &home)
}

/// The testable body of [`resolve_local_instance_port`], taking explicit
/// `platform_config`/`home` bases instead of reading them from `dirs::` —
/// mirrors `config::resolve_real_config_dir`'s own split so a test can point
/// it at a tempdir instead of the real platform config location.
fn resolve_local_instance_port_at(
    instance_id: &str,
    platform_config: Option<&Path>,
    home: &Path,
) -> Result<u16, LocalInstancePortError> {
    let instance = crate::app_instance::AppInstance::named(instance_id)
        .map_err(|_| LocalInstancePortError::InstanceNotFound)?;
    let dir = instance.config_dir_from(platform_config, home);
    if !dir.is_dir() {
        return Err(LocalInstancePortError::InstanceNotFound);
    }

    let config_path = dir.join("config.json");
    let content = std::fs::read_to_string(&config_path).map_err(|e| {
        LocalInstancePortError::Unreadable(format!("failed to read {}: {e}", config_path.display()))
    })?;
    let parsed: MinimalAppConfigForPort = serde_json::from_str(&content).map_err(|e| {
        LocalInstancePortError::Unreadable(format!(
            "failed to parse {}: {e}",
            config_path.display()
        ))
    })?;
    let port = parsed.services.server.port;
    if port == 0 {
        return Err(LocalInstancePortError::Unreadable(format!(
            "{} has no valid services.server.port",
            config_path.display()
        )));
    }
    Ok(port)
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

pub(crate) struct RemoteConnectionStore;

#[derive(Deserialize)]
pub(crate) struct RemoteConnectionSaveRequest {
    pub(crate) base: Option<RemoteConnection>,
    pub(crate) connection: RemoteConnection,
}

impl RemoteConnectionStore {
    /// Mutate the latest array under the same process and file locks used by
    /// other config domains. Connection ids, rather than array positions, are
    /// the unit of change.
    pub(crate) fn update<R, F>(config_dir: &Path, mutate: F) -> Result<R, String>
    where
        F: FnOnce(&mut Vec<RemoteConnection>) -> Result<(R, bool), String>,
    {
        crate::config::ConfigFile::<Vec<RemoteConnection>>::at_path(
            config_dir.join(CONNECTIONS_FILE),
        )
        .update_with_strict(mutate)
    }

    /// Load connections from `<config_dir>/connections.json`.
    /// Returns an empty vec if the file does not exist.
    pub(crate) fn load(config_dir: &Path) -> anyhow::Result<Vec<RemoteConnection>> {
        let path = config_dir.join(CONNECTIONS_FILE);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {e}", path.display()))?;
        let connections = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse {}: {e}", path.display()))?;
        Ok(connections)
    }

    /// Save connections to `<config_dir>/connections.json` atomically.
    pub(crate) fn save(config_dir: &Path, connections: &[RemoteConnection]) -> anyhow::Result<()> {
        std::fs::create_dir_all(config_dir)
            .map_err(|e| anyhow::anyhow!("Failed to create config dir: {e}"))?;
        let json = serde_json::to_string_pretty(connections)
            .map_err(|e| anyhow::anyhow!("Failed to serialize connections: {e}"))?;
        let target = config_dir.join(CONNECTIONS_FILE);
        let temp = target.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&temp, json.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to write temp file: {e}"))?;
        std::fs::rename(&temp, &target).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            anyhow::anyhow!("Failed to commit connections file: {e}")
        })?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// One-time migration of the pre-nested SSH shape
// ---------------------------------------------------------------------------

/// Rewrite one `transport` object from the flat SSH shape to the nested one,
/// in place. Keys are moved rather than re-serialized so nothing else in the
/// object (`remote_daemon_port`, anything newer) is touched. Returns whether
/// it changed; a non-SSH or already-nested transport is left alone.
fn migrate_transport_value(transport: &mut serde_json::Map<String, serde_json::Value>) -> bool {
    use serde_json::Value;
    if transport.get("type").and_then(Value::as_str) != Some("Ssh")
        || transport.contains_key("ssh")
        || !transport.contains_key("ssh_host")
    {
        return false;
    }
    let mut ssh = serde_json::Map::new();
    for (from, to) in [
        ("ssh_host", "host"),
        ("ssh_port", "port"),
        ("ssh_user", "user"),
        ("identity_file", "identity_file"),
    ] {
        ssh.insert(
            to.to_string(),
            transport.remove(from).unwrap_or(Value::Null),
        );
    }
    // What the flat shape ran with — see `legacy_remote_ssh_params`.
    let defaults = legacy_remote_ssh_params(String::new(), 22, String::new(), None);
    ssh.insert(
        "server_alive_interval".to_string(),
        defaults.server_alive_interval.into(),
    );
    ssh.insert(
        "server_alive_count_max".to_string(),
        defaults.server_alive_count_max.into(),
    );
    ssh.insert(
        "strict_host_key_checking".to_string(),
        serde_json::to_value(&defaults.strict_host_key_checking)
            .expect("a unit enum always serializes"),
    );
    ssh.insert("compression".to_string(), defaults.compression.into());
    transport.insert("ssh".to_string(), Value::Object(ssh));
    true
}

/// Rewrite a whole `connections.json` document. Errors when it is not the
/// array this file has always been; returns whether anything changed.
fn migrate_connections_document(doc: &mut serde_json::Value) -> Result<bool, String> {
    let entries = doc
        .as_array_mut()
        .ok_or_else(|| "connections.json is not a JSON array".to_string())?;
    let mut changed = false;
    for entry in entries {
        if let Some(transport) = entry
            .get_mut("transport")
            .and_then(serde_json::Value::as_object_mut)
        {
            changed |= migrate_transport_value(transport);
        }
    }
    Ok(changed)
}

/// Rewrite `<config_dir>/connections.json` from the pre-nested SSH shape to
/// the nested one, once. Returns the backup's path when it rewrote the file,
/// `None` when there was nothing to do (no file, or already nested — so a
/// second run is a no-op that writes nothing).
///
/// Runs under the same in-process and cross-process locks as every other
/// writer of this file. The original bytes are kept verbatim as
/// `connections.json.pre-nested-ssh-<UTC timestamp>.bak`, written before the
/// file is replaced; the replacement is atomic (temp file, fsync, rename).
/// The migrated document must parse as `Vec<RemoteConnection>` before
/// anything is written — otherwise the file is left exactly as it was and the
/// error is returned. Every field of every entry (auth username, deploy mode,
/// survive_secs, auto_update, unknown keys) is carried over untouched; secrets
/// were never in this file, so the vault (passwords, pairing tokens) is not
/// involved at all.
pub(crate) fn migrate_legacy_connections_file(
    config_dir: &Path,
) -> Result<Option<PathBuf>, String> {
    crate::config::ConfigFile::<Vec<RemoteConnection>>::at_path(config_dir.join(CONNECTIONS_FILE))
        .with_locks(|path| {
            let original = match std::fs::read(path) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(format!("Failed to read {}: {e}", path.display())),
            };
            let mut doc: serde_json::Value = serde_json::from_slice(&original)
                .map_err(|e| format!("Failed to parse {}: {e}", path.display()))?;
            if !migrate_connections_document(&mut doc)? {
                return Ok(None);
            }
            serde_json::from_value::<Vec<RemoteConnection>>(doc.clone()).map_err(|e| {
                format!(
                    "{} was left unmigrated: the rewritten document would not load: {e}",
                    path.display()
                )
            })?;
            let migrated = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
            crate::ssh_connection::legacy::backup_then_replace(path, &original, migrated.as_bytes())
                .map(Some)
        })
}

/// Validate then upsert `connection` into the store at `data_dir`.
///
/// Shared by the Tauri `save_remote_connection` command and the HTTP
/// `put_remote_connection` route so both enforce `validate()` identically
/// (IPC/HTTP parity) — the Tauri command used to skip validation. Callers hold
/// `state.connections_lock` around this to serialize concurrent writers.
pub(crate) fn upsert_remote_connection(
    data_dir: &Path,
    base: Option<RemoteConnection>,
    connection: RemoteConnection,
) -> Result<(), String> {
    connection.validate()?;
    if base.as_ref().is_some_and(|base| base.id != connection.id) {
        return Err("base and connection ids differ".to_string());
    }
    let delta = base
        .as_ref()
        .map(|base| {
            let base = serde_json::to_value(base).map_err(|e| e.to_string())?;
            let desired = serde_json::to_value(&connection).map_err(|e| e.to_string())?;
            Ok::<_, String>(crate::config::json_merge_delta(&base, &desired))
        })
        .transpose()?;
    RemoteConnectionStore::update(data_dir, move |connections| {
        let current = connections.iter_mut().find(|c| c.id == connection.id);
        match (base, current) {
            (None, None) => {
                connections.push(connection);
                Ok(((), true))
            }
            (None, Some(_)) => Err("connection already exists; load it before editing".to_string()),
            (Some(_), None) => Err("connection was deleted since it was loaded".to_string()),
            (Some(_), Some(current)) => {
                let Some(Some(delta)) = delta else {
                    return Ok(((), false));
                };
                crate::config::apply_typed_json_merge_delta(current, &delta)?;
                current.validate()?;
                Ok(((), true))
            }
        }
    })
}

pub(crate) fn remove_remote_connection(data_dir: &Path, id: &str) -> Result<bool, String> {
    RemoteConnectionStore::update(data_dir, |connections| {
        let before = connections.len();
        connections.retain(|connection| connection.id != id);
        let removed = connections.len() != before;
        Ok((removed, removed))
    })
}

/// How long the token exchange may take. The daemon answers from memory, so the
/// only thing this waits for is the network (and, on the SSH transport, a tunnel
/// that has already reported connected).
const TOKEN_FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Store (or, on an empty value, forget) the Basic Auth password for a
/// connection. The secret goes to the credential vault, never to
/// `connections.json` — that file is plain JSON on disk and is read by the
/// `/config/remote-connections` route.
pub(crate) fn set_connection_password(id: &str, password: &str) -> Result<(), String> {
    if password.is_empty() {
        return crate::credentials::delete(crate::credentials::Credential::RemoteConnection(id));
    }
    crate::credentials::set(
        crate::credentials::Credential::RemoteConnection(id),
        password,
    )
}

pub(crate) fn connection_password_exists(id: &str) -> Result<bool, String> {
    crate::credentials::get(crate::credentials::Credential::RemoteConnection(id))
        .map(|v| v.is_some())
}

pub(crate) fn set_pairing_token(id: &str, token: &str) -> Result<(), String> {
    if token.is_empty() {
        return crate::credentials::delete(crate::credentials::Credential::RemotePairingToken(id));
    }
    crate::credentials::set(
        crate::credentials::Credential::RemotePairingToken(id),
        token,
    )
}

pub(crate) fn pairing_token(id: &str) -> Result<Option<String>, String> {
    crate::credentials::get(crate::credentials::Credential::RemotePairingToken(id))
}

/// Delete a connection by id — the one path both the Tauri
/// `delete_remote_connection` command and the HTTP
/// `DELETE /config/remote-connections/{id}` route take, so they cannot drift.
///
/// Order matters: tear the runtime down first (`teardown_deleted` stops the
/// tunnel by the id it recorded, and drops the poll, mirror, rows and session
/// token — everything keyed by an id about to name nothing), then remove the
/// record under `connections_lock`, then forget BOTH vault entries, the
/// password and the pairing token: the key is the connection's fresh UUID, so a
/// secret left behind is unreachable and permanent. The HTTP route used to
/// forget only the password and leave the pairing token behind.
///
/// Returns whether a connection with this id existed. The vault entries are
/// cleared either way.
pub(crate) async fn delete_remote_connection_impl(
    state: &Arc<crate::AppState>,
    id: &str,
) -> Result<bool, String> {
    crate::remote_runtime::teardown_deleted(state, id);
    let removed = {
        let _guard = state.connections_lock.lock().await;
        remove_remote_connection(&state.data_dir, id)?
    };
    delete_connection_credentials(id)?;
    Ok(removed)
}

fn delete_connection_credentials(id: &str) -> Result<(), String> {
    let password = crate::credentials::delete(crate::credentials::Credential::RemoteConnection(id));
    let pairing =
        crate::credentials::delete(crate::credentials::Credential::RemotePairingToken(id));
    match (password, pairing) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(password), Ok(())) => Err(password),
        (Ok(()), Err(pairing)) => Err(pairing),
        (Err(password), Err(pairing)) => Err(format!(
            "failed to delete remote password ({password}) and pairing token ({pairing})"
        )),
    }
}

/// Trade the stored Basic Auth credentials for the daemon's session token.
///
/// A WebSocket upgrade cannot carry an `Authorization` header and the daemon
/// serves `Access-Control-Allow-Origin: *`, which forbids credentialed cookies,
/// so browser WebSocket upgrades carry `?token=` while native HTTP uses the
/// existing session cookie header. The exchange runs here rather than in the WebView so the password never leaves
/// the backend.
///
/// The token lives in the daemon's memory and changes on every restart, so the
/// caller re-runs this on every connect and never persists the result.
pub(crate) async fn fetch_connection_token(
    id: &str,
    base_url: &str,
    username: &str,
) -> Result<String, String> {
    let password = crate::credentials::get(crate::credentials::Credential::RemoteConnection(id))?
        .ok_or_else(|| "No password stored for this connection".to_string())?;
    request_session_token(base_url, username, &password).await
}

/// The HTTP half of [`fetch_connection_token`], split out so the failure modes
/// that matter — a rejected password, an older daemon with no such route — are
/// testable without an interactive keyring.
async fn request_session_token(
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<String, String> {
    let url = format!("{}/api/auth/session-token", base_url.trim_end_matches('/'));
    let response = reqwest::Client::new()
        .get(&url)
        .basic_auth(username, Some(password))
        .timeout(TOKEN_FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("Token request failed: {}", e.without_url()))?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Authentication rejected by the remote daemon".to_string());
    }
    if !status.is_success() {
        return Err(format!(
            "Remote daemon answered {status} for the session-token request"
        ));
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Malformed token response: {}", e.without_url()))?;
    let token = body
        .get("token")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if token.is_empty() {
        // An older daemon has no such route and the request fell through to an
        // SPA shell or an empty body; say so instead of handing back "".
        return Err("Remote daemon returned no session token — is it running a build with /api/auth/session-token?".to_string());
    }
    Ok(token.to_string())
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn list_remote_connections(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<Vec<RemoteConnection>, String> {
    RemoteConnectionStore::load(&state.data_dir).map_err(|e| e.to_string())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn save_remote_connection(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
    base: Option<RemoteConnection>,
    connection: RemoteConnection,
) -> Result<(), String> {
    let _guard = state.connections_lock.lock().await;
    upsert_remote_connection(&state.data_dir, base, connection)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn delete_remote_connection(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
    id: String,
) -> Result<(), String> {
    // A missing id is not an error here (it never was on this side); the HTTP
    // route answers 404 for the same result.
    delete_remote_connection_impl(state.inner(), &id)
        .await
        .map(|_| ())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn set_remote_connection_password(id: String, password: String) -> Result<(), String> {
    set_connection_password(&id, &password)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn remote_connection_password_exists(id: String) -> Result<bool, String> {
    connection_password_exists(&id)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn fetch_remote_connection_token(
    id: String,
    base_url: String,
    username: String,
) -> Result<String, String> {
    fetch_connection_token(&id, &base_url, &username).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_json_round_trip() {
        let conn = RemoteConnection::new_ssh("my-server", "example.com", "alice");
        let json = serde_json::to_string(&conn).unwrap();
        let decoded: RemoteConnection = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, conn.name);
        assert_eq!(decoded.auth_username, conn.auth_username);
        assert!(decoded.enabled);
        match decoded.transport {
            RemoteTransport::Ssh {
                ssh,
                remote_daemon_port,
            } => {
                assert_eq!(ssh.host, "example.com");
                assert_eq!(ssh.port, 22);
                assert_eq!(ssh.user, "alice");
                assert!(ssh.identity_file.is_none());
                assert_eq!(
                    ssh.strict_host_key_checking,
                    StrictHostKeyChecking::AcceptNew
                );
                assert_eq!(remote_daemon_port, 9877);
            }
            other => panic!("expected Ssh, got {other:?}"),
        }
    }

    #[test]
    fn direct_json_round_trip() {
        let conn = RemoteConnection::new_direct("office", "http://office:9877", "bob");
        let json = serde_json::to_string(&conn).unwrap();
        let decoded: RemoteConnection = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "office");
        assert_eq!(decoded.auth_username.as_deref(), Some("bob"));
        assert!(decoded.enabled);
        match decoded.transport {
            RemoteTransport::Direct {
                url,
                tls_fingerprint,
            } => {
                assert_eq!(url, "http://office:9877");
                assert!(tls_fingerprint.is_none());
            }
            other => panic!("expected Direct, got {other:?}"),
        }
    }

    #[test]
    fn serde_tag_produces_correct_type_field() {
        let ssh = RemoteConnection::new_ssh("s", "h", "u");
        let ssh_json = serde_json::to_string(&ssh).unwrap();
        let ssh_val: serde_json::Value = serde_json::from_str(&ssh_json).unwrap();
        assert_eq!(ssh_val["transport"]["type"], "Ssh");

        let direct = RemoteConnection::new_direct("d", "http://x", "u");
        let direct_json = serde_json::to_string(&direct).unwrap();
        let direct_val: serde_json::Value = serde_json::from_str(&direct_json).unwrap();
        assert_eq!(direct_val["transport"]["type"], "Direct");

        let local = RemoteConnection::new_local_port("l", 9877);
        let local_val = serde_json::to_value(&local).unwrap();
        assert_eq!(local_val["transport"]["type"], "Local");
    }

    #[test]
    fn local_instance_json_round_trip() {
        let conn = RemoteConnection::new_local_instance("dev-instance", "dev-box");
        let json = serde_json::to_string(&conn).unwrap();
        let decoded: RemoteConnection = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.auth_username, None);
        match decoded.transport {
            RemoteTransport::Local { port, instance_id } => {
                assert!(port.is_none());
                assert_eq!(instance_id.as_deref(), Some("dev-box"));
            }
            other => panic!("expected Local, got {other:?}"),
        }
    }

    #[test]
    fn local_port_json_round_trip() {
        let conn = RemoteConnection::new_local_port("debug-instance", 9877);
        let json = serde_json::to_string(&conn).unwrap();
        let decoded: RemoteConnection = serde_json::from_str(&json).unwrap();
        match decoded.transport {
            RemoteTransport::Local { port, instance_id } => {
                assert_eq!(port, Some(9877));
                assert!(instance_id.is_none());
            }
            other => panic!("expected Local, got {other:?}"),
        }
    }

    /// The nested shape is the only one written: no flat key survives a save.
    #[test]
    fn an_ssh_transport_is_written_nested_only() {
        let value = serde_json::to_value(RemoteConnection::new_ssh("s", "h", "u")).unwrap();
        let transport = value["transport"].as_object().unwrap();
        let mut keys: Vec<&str> = transport.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["remote_daemon_port", "ssh", "type"]);
        assert_eq!(transport["ssh"]["host"], "h");
    }

    #[test]
    fn missing_auth_username_defaults_to_none_and_null_is_accepted() {
        let mut value =
            serde_json::to_value(RemoteConnection::new_direct("d", "http://x", "u")).unwrap();
        value["auth_username"] = serde_json::Value::Null;
        let decoded: RemoteConnection = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(decoded.auth_username, None);
        value.as_object_mut().unwrap().remove("auth_username");
        let decoded: RemoteConnection = serde_json::from_value(value).unwrap();
        assert_eq!(decoded.auth_username, None);
    }

    #[test]
    fn an_ssh_transport_mixing_both_shapes_is_rejected() {
        let mut value = serde_json::to_value(RemoteConnection::new_ssh("s", "h", "u")).unwrap();
        value["transport"]["ssh_host"] = "other".into();
        let err = serde_json::from_value::<RemoteConnection>(value).unwrap_err();
        assert!(err.to_string().contains("both a nested"), "{err}");
    }

    #[test]
    fn store_save_then_load_returns_same_data() {
        let dir = tempfile::tempdir().unwrap();
        let conn1 = RemoteConnection::new_ssh("server1", "host1.example.com", "alice");
        let conn2 = RemoteConnection::new_direct("direct1", "http://10.0.0.1:9877", "bob");
        let connections = vec![conn1, conn2];

        RemoteConnectionStore::save(dir.path(), &connections).unwrap();
        let loaded = RemoteConnectionStore::load(dir.path()).unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].name, "server1");
        assert_eq!(loaded[1].name, "direct1");
        assert_eq!(loaded[0].id, connections[0].id);
        assert_eq!(loaded[1].id, connections[1].id);
    }

    #[test]
    fn store_load_nonexistent_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = RemoteConnectionStore::load(dir.path()).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn new_ssh_defaults() {
        let conn = RemoteConnection::new_ssh("test", "myhost", "myuser");
        assert!(conn.enabled);
        match conn.transport {
            RemoteTransport::Ssh {
                ssh,
                remote_daemon_port,
            } => {
                assert_eq!(ssh.port, 22);
                assert_eq!(remote_daemon_port, 9877);
                assert_eq!(ssh.user, "myuser");
            }
            other => panic!("expected Ssh, got {other:?}"),
        }
        assert_eq!(conn.auth_username.as_deref(), Some("myuser"));
    }

    #[test]
    fn new_direct_enabled() {
        let conn = RemoteConnection::new_direct("d", "http://x", "u");
        assert!(conn.enabled);
    }

    // --- Story 781-9652: the password lives in the vault, never on disk -------

    /// `connections.json` is plain JSON that `GET /config/remote-connections`
    /// serves to any authenticated caller. A password field added to
    /// `RemoteConnection` would be published by both, silently — this is the
    /// guard that fails first.
    #[test]
    fn serialized_connection_carries_no_secret() {
        let conn = RemoteConnection::new_direct("office", "http://office:9877", "bob");
        let value: serde_json::Value = serde_json::to_value(&conn).unwrap();
        let keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "id",
                "name",
                "transport",
                "auth_username",
                "enabled",
                "auto_update",
                "deploy",
                "survive_secs"
            ],
            "connections.json gained a field — if it holds a secret, it must go to the vault instead"
        );
        let serialized = serde_json::to_string(&conn).unwrap();
        assert!(!serialized.contains("password"));
        assert!(!serialized.contains("token"));
    }

    #[test]
    fn legacy_connection_defaults_to_no_deploy_and_thirty_minutes() {
        let mut value = serde_json::to_value(RemoteConnection::new_direct(
            "office",
            "http://office:9877",
            "bob",
        ))
        .unwrap();
        value.as_object_mut().unwrap().remove("deploy");
        value.as_object_mut().unwrap().remove("survive_secs");
        value.as_object_mut().unwrap().remove("auto_update");

        let decoded: RemoteConnection = serde_json::from_value(value).unwrap();

        assert_eq!(decoded.deploy, DeployMode::Never);
        assert_eq!(decoded.survive_secs, 1_800);
        assert!(!decoded.auto_update);
    }

    // Catches: explicitly echoed base URLs or reqwest URLs leaking URL userinfo in token-request errors.
    #[tokio::test]
    async fn token_request_error_redacts_url_credentials() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        drop(listener);
        let base = format!("http://username:URL_SECRET_1457@{address}");
        let error = request_session_token(&base, "user", "password")
            .await
            .expect_err("closed port");
        assert!(error.contains("failed"), "{error}");
        assert!(
            !error.contains("URL_SECRET_1457"),
            "URL credential in error: {error}"
        );
        assert!(
            !error.contains("password"),
            "Basic credential in error: {error}"
        );
    }

    #[tokio::test]
    async fn session_token_request_reads_the_token_field() {
        let mut server = mockito::Server::new_async().await;
        let route = server
            .mock("GET", "/api/auth/session-token")
            // Basic dm9tOnMzY3JldA== is "vom:s3cret" — proof the credential is
            // sent as a header on this one call, before anything moves to ?token=.
            .match_header("authorization", "Basic dm9tOnMzY3JldA==")
            .with_status(200)
            .with_body(r#"{"token":"tok-abc"}"#)
            .create_async()
            .await;

        let token = request_session_token(&server.url(), "vom", "s3cret")
            .await
            .unwrap();

        assert_eq!(token, "tok-abc");
        route.assert_async().await;
    }

    /// A wrong password and an unreachable daemon need different fixes, so 401
    /// must not be reported as a generic upstream failure.
    #[tokio::test]
    async fn session_token_request_names_a_rejected_password() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/api/auth/session-token")
            .with_status(401)
            .with_body("Scan the QR code or authenticate with Basic Auth")
            .create_async()
            .await;

        let err = request_session_token(&server.url(), "vom", "wrong")
            .await
            .unwrap_err();

        assert!(
            err.contains("Authentication rejected"),
            "expected a rejection, got: {err}"
        );
    }

    /// With `auth_username` absent the exchange sends an EMPTY username
    /// (`remote_runtime::authenticate`); a daemon refuses it (see
    /// `mcp_http::auth`'s `basic_auth_empty_username_fails_closed_*`), and the
    /// refusal surfaces as a rejection, not a token.
    #[tokio::test]
    async fn session_token_request_with_no_username_fails_closed() {
        let mut server = mockito::Server::new_async().await;
        let route = server
            .mock("GET", "/api/auth/session-token")
            // Basic OnMzY3JldA== is ":s3cret".
            .match_header("authorization", "Basic OnMzY3JldA==")
            .with_status(401)
            .create_async()
            .await;

        let err = request_session_token(&server.url(), "", "s3cret")
            .await
            .unwrap_err();

        assert!(err.contains("Authentication rejected"), "{err}");
        assert!(
            !err.contains("s3cret"),
            "password leaked into the error: {err}"
        );
        route.assert_async().await;
    }

    /// A daemon older than this route has no handler for it: the request falls
    /// through to something without a `token` field. Handing back "" would make
    /// every later call 401 with no explanation.
    #[tokio::test]
    async fn session_token_request_rejects_a_response_without_a_token() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/api/auth/session-token")
            .with_status(200)
            .with_body(r#"{"ok":true}"#)
            .create_async()
            .await;

        let err = request_session_token(&server.url(), "vom", "s3cret")
            .await
            .unwrap_err();

        assert!(
            err.contains("no session token"),
            "expected a missing-token error, got: {err}"
        );
    }

    /// The base URL comes from a user-typed field; a trailing slash there would
    /// otherwise produce `//api/auth/session-token`, which axum does not match.
    #[tokio::test]
    async fn session_token_request_tolerates_a_trailing_slash() {
        let mut server = mockito::Server::new_async().await;
        let route = server
            .mock("GET", "/api/auth/session-token")
            .with_status(200)
            .with_body(r#"{"token":"tok-abc"}"#)
            .create_async()
            .await;

        let token = request_session_token(&format!("{}/", server.url()), "vom", "s3cret")
            .await
            .unwrap();

        assert_eq!(token, "tok-abc");
        route.assert_async().await;
    }

    #[test]
    fn validate_valid_ssh_connection() {
        let conn = RemoteConnection::new_ssh("server", "host.example.com", "alice");
        assert!(conn.validate().is_ok());
    }

    #[test]
    fn validate_invalid_uuid_rejected() {
        let mut conn = RemoteConnection::new_ssh("server", "host", "alice");
        conn.id = "../../malicious".to_string();
        let err = conn.validate().unwrap_err();
        assert!(
            err.contains("valid UUID"),
            "expected UUID error, got: {err}"
        );
    }

    #[test]
    fn validate_whitespace_name_rejected() {
        let conn = RemoteConnection::new_ssh("  ", "host", "alice");
        assert!(conn.validate().is_err());
    }

    #[test]
    fn validate_empty_ssh_host_rejected() {
        let conn = RemoteConnection::new_ssh("s", "", "alice");
        assert!(conn.validate().is_err());
    }

    #[test]
    fn validate_empty_url_rejected() {
        let conn = RemoteConnection::new_direct("d", "  ", "u");
        assert!(conn.validate().is_err());
    }

    fn direct_with_pin(url: &str, pin: &str) -> RemoteConnection {
        let mut conn = RemoteConnection::new_direct("d", url, "u");
        conn.transport = RemoteTransport::Direct {
            url: url.to_string(),
            tls_fingerprint: Some(pin.to_string()),
        };
        conn
    }

    /// A pin is only ever a SHA-256 fingerprint on an https:// URL: anything
    /// else is refused at save, so a garbled value can never stand in for one.
    #[test]
    fn validate_checks_a_pinned_certificate_fingerprint() {
        let pin = "ab".repeat(32);
        assert!(direct_with_pin("https://box:9877", &pin).validate().is_ok());
        assert!(
            direct_with_pin("https://box:9877", "nope")
                .validate()
                .is_err()
        );
        assert!(direct_with_pin("http://box:9877", &pin).validate().is_err());
    }

    /// The pin is additive: an unpinned Direct transport serializes exactly as
    /// before (older builds read it unchanged), a pinned one round-trips, and
    /// a file without the key reads as unpinned.
    #[test]
    fn tls_fingerprint_is_omitted_when_unset_and_round_trips_when_set() {
        let plain = RemoteConnection::new_direct("d", "https://box:9877", "u");
        let json = serde_json::to_value(&plain).unwrap();
        assert_eq!(
            json["transport"],
            serde_json::json!({"type": "Direct", "url": "https://box:9877"})
        );
        let pinned = direct_with_pin("https://box:9877", &"cd".repeat(32));
        let back: RemoteConnection =
            serde_json::from_str(&serde_json::to_string(&pinned).unwrap()).unwrap();
        assert!(matches!(
            back.transport,
            RemoteTransport::Direct { tls_fingerprint: Some(ref p), .. } if *p == "cd".repeat(32)
        ));
    }

    #[test]
    fn validate_ssh_connection_without_auth_username_is_ok() {
        let mut conn = RemoteConnection::new_ssh("server", "host.example.com", "alice");
        conn.auth_username = None;
        assert!(conn.validate().is_ok());
    }

    #[test]
    fn validate_local_requires_port_or_instance_id() {
        let mut conn = RemoteConnection::new_local_port("local", 9877);
        conn.transport = RemoteTransport::Local {
            port: None,
            instance_id: None,
        };
        let err = conn.validate().unwrap_err();
        assert!(err.contains("port or an instance_id"), "{err}");
    }

    #[test]
    fn validate_local_rejects_both_port_and_instance_id() {
        let mut conn = RemoteConnection::new_local_port("local", 9877);
        conn.transport = RemoteTransport::Local {
            port: Some(9877),
            instance_id: Some("dev-box".to_string()),
        };
        let err = conn.validate().unwrap_err();
        assert!(err.contains("not both"), "{err}");
    }

    #[test]
    fn validate_local_port_and_instance_id_accepted() {
        assert!(
            RemoteConnection::new_local_port("l", 9877)
                .validate()
                .is_ok()
        );
        assert!(
            RemoteConnection::new_local_instance("l", "dev-box")
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn validate_local_zero_port_rejected() {
        let err = RemoteConnection::new_local_port("l", 0)
            .validate()
            .unwrap_err();
        assert!(err.contains("port must be in range"), "{err}");
    }

    #[test]
    fn validate_local_blank_instance_id_treated_as_absent() {
        let mut conn = RemoteConnection::new_local_port("local", 9877);
        conn.transport = RemoteTransport::Local {
            port: None,
            instance_id: Some("   ".to_string()),
        };
        let err = conn.validate().unwrap_err();
        assert!(err.contains("port or an instance_id"), "{err}");
    }

    // --- Pre-nested (flat) connections.json: reader + one-time migration -------

    /// A `connections.json` exactly as the pre-nested build wrote it — one entry
    /// per transport, every main-era field set to a non-default value — plus a
    /// key this build does not know, to prove the migration moves keys instead
    /// of re-serializing the typed struct.
    fn legacy_connections_json() -> serde_json::Value {
        serde_json::json!([
            {
                "id": "6a0f1c2e-4b5d-4e6f-8a7b-9c0d1e2f3a4b",
                "name": "vps",
                "transport": {
                    "type": "Ssh",
                    "ssh_host": "vps.example.com",
                    "ssh_port": 2222,
                    "ssh_user": "deploy",
                    "identity_file": "/home/deploy/.ssh/id_ed25519",
                    "remote_daemon_port": 9877
                },
                "auth_username": "boss",
                "enabled": false,
                "auto_update": true,
                "deploy": "on_connect",
                "survive_secs": 600,
                "future_field": {"kept": true}
            },
            {
                "id": "7b1f2d3e-5c6d-4f70-9b8c-0d1e2f3a4b5c",
                "name": "office",
                "transport": {"type": "Direct", "url": "http://office:9877"},
                "auth_username": "bob",
                "enabled": true,
                "auto_update": false,
                "deploy": "installed",
                "survive_secs": 1800
            },
            {
                "id": "8c2a3e4f-6d7e-4081-8c9d-1e2f3a4b5c6d",
                "name": "no-identity",
                "transport": {
                    "type": "Ssh",
                    "ssh_host": "h",
                    "ssh_port": 22,
                    "ssh_user": "u",
                    "identity_file": null,
                    "remote_daemon_port": 9877
                },
                "auth_username": "u",
                "enabled": true
            }
        ])
    }

    fn assert_is_the_legacy_vps(conn: &RemoteConnection) {
        assert_eq!(conn.id, "6a0f1c2e-4b5d-4e6f-8a7b-9c0d1e2f3a4b");
        assert_eq!(conn.name, "vps");
        assert_eq!(conn.auth_username.as_deref(), Some("boss"));
        assert!(!conn.enabled);
        assert!(conn.auto_update);
        assert_eq!(conn.deploy, DeployMode::OnConnect);
        assert_eq!(conn.survive_secs, 600);
        let RemoteTransport::Ssh {
            ssh,
            remote_daemon_port,
        } = &conn.transport
        else {
            panic!("expected Ssh, got {:?}", conn.transport);
        };
        assert_eq!(*remote_daemon_port, 9877);
        assert_eq!(ssh.host, "vps.example.com");
        assert_eq!(ssh.port, 2222);
        assert_eq!(ssh.user, "deploy");
        assert_eq!(
            ssh.identity_file,
            Some(PathBuf::from("/home/deploy/.ssh/id_ed25519"))
        );
        // What the flat shape actually ran with.
        assert_eq!(ssh.server_alive_interval, 15);
        assert_eq!(ssh.server_alive_count_max, 3);
        assert_eq!(
            ssh.strict_host_key_checking,
            StrictHostKeyChecking::AcceptNew
        );
        assert!(ssh.compression);
    }

    #[test]
    fn the_flat_pre_nested_shape_still_loads_with_every_field() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        std::fs::write(
            dir.path().join(CONNECTIONS_FILE),
            serde_json::to_vec_pretty(&legacy_connections_json()).unwrap(),
        )
        .unwrap();

        let loaded = RemoteConnectionStore::load(dir.path()).unwrap();

        assert_eq!(loaded.len(), 3);
        assert_is_the_legacy_vps(&loaded[0]);
        assert!(
            matches!(&loaded[1].transport, RemoteTransport::Direct { url, .. } if url == "http://office:9877")
        );
        assert_eq!(loaded[1].deploy, DeployMode::Installed);
        // Main-era entry without the newer fields still gets their defaults.
        assert_eq!(loaded[2].deploy, DeployMode::Never);
        assert_eq!(loaded[2].survive_secs, 1_800);
    }

    /// Old file -> migrate -> load gives the same connections, the original
    /// bytes are kept verbatim as a backup, unknown keys survive, and a second
    /// run writes nothing.
    #[test]
    fn migrating_connections_json_round_trips_every_field_and_is_idempotent() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join(CONNECTIONS_FILE);
        let original = serde_json::to_vec_pretty(&legacy_connections_json()).unwrap();
        std::fs::write(&path, &original).unwrap();
        let before = RemoteConnectionStore::load(dir.path()).unwrap();

        let backup = migrate_legacy_connections_file(dir.path())
            .unwrap()
            .expect("a flat file is migrated");

        assert_eq!(
            std::fs::read(&backup).unwrap(),
            original,
            "backup must be the original bytes"
        );
        assert_eq!(backup.parent(), Some(dir.path()));
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        for entry in raw.as_array().unwrap() {
            let transport = entry["transport"].as_object().unwrap();
            for gone in ["ssh_host", "ssh_port", "ssh_user", "identity_file"] {
                assert!(!transport.contains_key(gone), "{gone} survived: {entry}");
            }
        }
        assert_eq!(raw[0]["future_field"], serde_json::json!({"kept": true}));
        assert_eq!(raw[0]["transport"]["remote_daemon_port"], 9877);
        assert!(raw[2]["transport"]["ssh"]["identity_file"].is_null());

        let after = RemoteConnectionStore::load(dir.path()).unwrap();
        assert_is_the_legacy_vps(&after[0]);
        assert_eq!(
            serde_json::to_value(&before).unwrap(),
            serde_json::to_value(&after).unwrap(),
            "the migrated file must load as exactly what the flat one loaded as"
        );

        let rewritten = std::fs::read(&path).unwrap();
        assert_eq!(migrate_legacy_connections_file(dir.path()).unwrap(), None);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            rewritten,
            "second run must not write"
        );
        let backups = std::fs::read_dir(dir.path())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".bak")
            })
            .count();
        assert_eq!(
            backups, 1,
            "an idempotent run must not leave another backup"
        );
    }

    #[test]
    fn migration_is_a_no_op_without_a_file_or_on_a_nested_file() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        assert_eq!(migrate_legacy_connections_file(dir.path()).unwrap(), None);

        RemoteConnectionStore::save(
            dir.path(),
            &[
                RemoteConnection::new_ssh("s", "h", "u"),
                RemoteConnection::new_local_port("l", 9877),
            ],
        )
        .unwrap();
        let bytes = std::fs::read(dir.path().join(CONNECTIONS_FILE)).unwrap();
        assert_eq!(migrate_legacy_connections_file(dir.path()).unwrap(), None);
        assert_eq!(
            std::fs::read(dir.path().join(CONNECTIONS_FILE)).unwrap(),
            bytes
        );
    }

    /// A flat file the typed reader would reject after rewriting (here: a port
    /// that is not a number) is left exactly as it was, with no backup.
    #[test]
    fn a_migration_that_would_not_load_writes_nothing() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join(CONNECTIONS_FILE);
        let mut doc = legacy_connections_json();
        doc[0]["transport"]["ssh_port"] = "not-a-port".into();
        let original = serde_json::to_vec_pretty(&doc).unwrap();
        std::fs::write(&path, &original).unwrap();

        let err = migrate_legacy_connections_file(dir.path()).unwrap_err();

        assert!(err.contains("left unmigrated"), "{err}");
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            2,
            "file + its .lock only"
        );
    }

    // --- resolve_local_instance_port_at ---

    #[test]
    fn resolve_local_instance_port_missing_directory_is_not_found() {
        let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let err = resolve_local_instance_port_at("no-such-instance", None, &home).unwrap_err();
        assert_eq!(err, LocalInstancePortError::InstanceNotFound);
    }

    #[test]
    fn resolve_local_instance_port_invalid_id_is_not_found() {
        let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        // "default" is reserved by `AppInstance::named` and rejected outright.
        let err = resolve_local_instance_port_at("default", None, &home).unwrap_err();
        assert_eq!(err, LocalInstancePortError::InstanceNotFound);
    }

    fn instance_dir(home: &Path, id: &str) -> PathBuf {
        let dir = crate::app_instance::AppInstance::named(id)
            .unwrap()
            .config_dir_from(None, home);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_local_instance_port_missing_config_file_is_unreadable() {
        let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let home = tmp.path().join("home");
        instance_dir(&home, "dev-box");
        let err = resolve_local_instance_port_at("dev-box", None, &home).unwrap_err();
        assert!(matches!(err, LocalInstancePortError::Unreadable(_)));
    }

    #[test]
    fn resolve_local_instance_port_corrupt_json_is_unreadable() {
        let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let home = tmp.path().join("home");
        let dir = instance_dir(&home, "dev-box");
        std::fs::write(dir.join("config.json"), "not valid json{{{").unwrap();
        let err = resolve_local_instance_port_at("dev-box", None, &home).unwrap_err();
        assert!(matches!(err, LocalInstancePortError::Unreadable(_)));
    }

    #[test]
    fn resolve_local_instance_port_zero_port_is_unreadable() {
        let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let home = tmp.path().join("home");
        let dir = instance_dir(&home, "dev-box");
        std::fs::write(
            dir.join("config.json"),
            serde_json::json!({"services": {"server": {"port": 0}}}).to_string(),
        )
        .unwrap();
        let err = resolve_local_instance_port_at("dev-box", None, &home).unwrap_err();
        assert!(matches!(err, LocalInstancePortError::Unreadable(_)));
    }

    #[test]
    fn resolve_local_instance_port_reads_the_real_port_ignoring_other_fields() {
        let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let home = tmp.path().join("home");
        let dir = instance_dir(&home, "dev-box");
        std::fs::write(
            dir.join("config.json"),
            serde_json::json!({
                "shell": "/bin/zsh",
                "services": {
                    "server": {"port": 9878, "enabled": true},
                    "auth": {"username": "boss"},
                },
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(
            resolve_local_instance_port_at("dev-box", None, &home).unwrap(),
            9878
        );
    }

    // --- IPC/HTTP parity: shared save path enforces validate() (story 127-89ec) -
    // `upsert_remote_connection` is the single persist path behind both the Tauri
    // `save_remote_connection` command (which previously skipped validation) and
    // the HTTP `put_remote_connection` route. This proves invalid input is
    // rejected before it can be persisted, on either transport.

    #[test]
    fn upsert_rejects_invalid_before_persisting() {
        let dir = tempfile::tempdir().unwrap();
        let mut bad = RemoteConnection::new_ssh("server", "host", "alice");
        bad.id = "../../escape".to_string(); // invalid UUID
        let err = upsert_remote_connection(dir.path(), None, bad).unwrap_err();
        assert!(
            err.contains("valid UUID"),
            "expected UUID error, got: {err}"
        );
        assert!(
            RemoteConnectionStore::load(dir.path()).unwrap().is_empty(),
            "invalid input must not be written to the store"
        );
    }

    // `known_bug_http_delete_stops_a_running_tunnel_before_deleting` (wip) is
    // gone on purpose: its premise — HTTP delete stopping a tunnel keyed by the
    // CONNECTION id — never held on this design, where IPC and HTTP both reach
    // `teardown_deleted` through `delete_remote_connection_impl`. The HTTP
    // half is covered by `config_routes::tests::deleting_a_connection_over_http_*`.

    #[tokio::test]
    async fn delete_remote_connection_impl_reports_a_missing_id() {
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        assert!(
            !delete_remote_connection_impl(&state, "does-not-exist")
                .await
                .unwrap()
        );
    }

    /// Both vault entries go with the record — the password AND the pairing
    /// token — on the one path both transports take.
    #[tokio::test]
    async fn delete_remote_connection_impl_forgets_password_and_pairing_token() {
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        let conn = RemoteConnection::new_ssh("test", "127.0.0.1", "nobody");
        let id = conn.id.clone();
        upsert_remote_connection(&state.data_dir, None, conn).unwrap();
        set_connection_password(&id, "hunter2").unwrap();
        set_pairing_token(&id, "pair-secret").unwrap();

        assert!(delete_remote_connection_impl(&state, &id).await.unwrap());

        assert!(
            RemoteConnectionStore::load(&state.data_dir)
                .unwrap()
                .is_empty()
        );
        assert!(!connection_password_exists(&id).unwrap());
        assert_eq!(pairing_token(&id).unwrap(), None);
    }

    #[test]
    fn upsert_persists_valid_and_updates_existing() {
        let dir = tempfile::tempdir().unwrap();
        let conn = RemoteConnection::new_ssh("server", "host.example.com", "alice");
        let id = conn.id.clone();
        upsert_remote_connection(dir.path(), None, conn).unwrap();
        assert_eq!(RemoteConnectionStore::load(dir.path()).unwrap().len(), 1);

        // Same id updates in place rather than appending a duplicate.
        let mut updated = RemoteConnection::new_ssh("renamed", "host.example.com", "alice");
        updated.id = id.clone();
        let base = RemoteConnectionStore::load(dir.path()).unwrap().remove(0);
        upsert_remote_connection(dir.path(), Some(base), updated).unwrap();
        let loaded = RemoteConnectionStore::load(dir.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, id);
        assert_eq!(loaded[0].name, "renamed");
    }

    #[test]
    fn stale_remote_connection_saves_preserve_independent_fields() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let original = RemoteConnection::new_direct("first", "https://host.example", "alice");
        upsert_remote_connection(dir.path(), None, original.clone()).unwrap();

        let mut first = original.clone();
        first.auto_update = true;
        let mut second = original.clone();
        second.name = "renamed".to_string();
        upsert_remote_connection(dir.path(), Some(original.clone()), first).unwrap();
        upsert_remote_connection(dir.path(), Some(original), second).unwrap();

        let saved = RemoteConnectionStore::load(dir.path()).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].name, "renamed");
        assert!(
            saved[0].auto_update,
            "an unchanged stale field must not revert another writer"
        );
    }
}
