//! Remote connection config model.
//!
//! Persists named connections (SSH, Direct, or Local) to `connections.json`
//! in the app config directory. Each connection has a UUID, a human-readable
//! name, a transport, optional auth info, and an enabled flag.
//!
//! No on-disk migration for the Phase 1 shape change (flat SSH fields →
//! nested `SshConnectionParams`, `auth_username` → `Option<String>`, new
//! `Local` transport variant): verified 2026-09-23 that no `connections.json`
//! or `tunnels/*.toml` file exists anywhere on this machine (default config
//! dir or any named `instances/<id>/` dir) — this feature (`61a388b83`,
//! 2026-05-09) has never had a real persisted connection to migrate. Per the
//! consolidation plan's Phase 1 note, additive/renamed fields are safe to
//! ship without a migration when there's nothing on disk to break; revisit if
//! a real install is ever found with a pre-Phase-1 `connections.json`.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ssh_connection::SshConnectionParams;

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
    /// Optional now (story: SSH Tunnels + Remote Servers consolidation,
    /// Phase 1) — previously required but never actually used to
    /// authenticate anything. The password half of auth (never stored here)
    /// lives in the credential vault, keyed by `id` — see
    /// `credentials::Credential::RemoteConnection`.
    #[serde(default)]
    pub(crate) auth_username: Option<String>,
    pub(crate) enabled: bool,
}

/// Transport layer for a remote connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum RemoteTransport {
    Ssh {
        /// Host/port/user/identity/keepalive config — shared with
        /// `tunnels::profile::TunnelProfile` via `SshConnectionParams`.
        ssh: SshConnectionParams,
        remote_daemon_port: u16,
    },
    Direct {
        url: String,
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
    Local {
        port: Option<u16>,
        instance_id: Option<String>,
    },
}

impl RemoteConnection {
    /// Create a new SSH connection with default port (22) and daemon port (9877).
    pub(crate) fn new_ssh(
        name: impl Into<String>,
        host: impl Into<String>,
        user: impl Into<String>,
    ) -> Self {
        let ssh_user = user.into();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            transport: RemoteTransport::Ssh {
                ssh: SshConnectionParams::new(host, ssh_user.clone()),
                remote_daemon_port: 9877,
            },
            auth_username: Some(ssh_user),
            enabled: true,
        }
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
            transport: RemoteTransport::Direct { url: url.into() },
            auth_username: Some(auth_username.into()),
            enabled: true,
        }
    }

    /// Create a new Local connection pointing at a named instance, resolved
    /// by instance id rather than a manually-entered port.
    pub(crate) fn new_local_instance(
        name: impl Into<String>,
        instance_id: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            transport: RemoteTransport::Local {
                port: None,
                instance_id: Some(instance_id.into()),
            },
            auth_username: None,
            enabled: true,
        }
    }

    /// Create a new Local connection pointing at a manually-entered port
    /// (the unnamed-instance case — e.g. a plain `make dev` second debug
    /// instance, which has no `instances/<id>/` directory to discover).
    pub(crate) fn new_local_port(name: impl Into<String>, port: u16) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            transport: RemoteTransport::Local {
                port: Some(port),
                instance_id: None,
            },
            auth_username: None,
            enabled: true,
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
            RemoteTransport::Direct { url } => {
                if url.trim().is_empty() {
                    return Err("url must not be empty".to_string());
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
}

// ---------------------------------------------------------------------------
// Local instance port resolution
// ---------------------------------------------------------------------------

/// Distinguishes "this instance id has no on-disk config directory at all"
/// (a typo, or an instance that was never started) from "the directory
/// exists but its `config.json` couldn't be read/parsed" (a real instance,
/// transient or corrupt state) — Test Connection (plan Phase 2) needs to
/// tell these apart in its UI, and Connect needs the same distinction to
/// give a useful error.
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
/// sub-objects default to a `ServerConfig`-shaped zero value with port 0,
/// which `resolve_local_instance_port_at` never accepts as valid (see its own
/// port-zero handling below) — a `config.json` with no `services.server.port`
/// key at all is at least as "unreadable" as one with a bad type.
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
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
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

impl RemoteConnectionStore {
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

/// Validate then upsert `connection` into the store at `data_dir`.
///
/// Shared by the Tauri `save_remote_connection` command and the HTTP
/// `put_remote_connection` route so both enforce `validate()` identically
/// (IPC/HTTP parity) — the Tauri command used to skip validation. Callers hold
/// `state.connections_lock` around this to serialize concurrent writers.
pub(crate) fn upsert_remote_connection(
    data_dir: &Path,
    connection: RemoteConnection,
) -> Result<(), String> {
    connection.validate()?;
    let mut connections = RemoteConnectionStore::load(data_dir).map_err(|e| e.to_string())?;
    if let Some(existing) = connections.iter_mut().find(|c| c.id == connection.id) {
        *existing = connection;
    } else {
        connections.push(connection);
    }
    RemoteConnectionStore::save(data_dir, &connections).map_err(|e| e.to_string())
}

/// Delete a connection by id: stops any tunnel running for it, deletes any
/// stored keyring credential for it, then removes it from `connections.json`.
///
/// Shared by the Tauri `delete_remote_connection` command and the HTTP
/// `delete_remote_connection` route (`mcp_http::config_routes`) so both do
/// the exact same cleanup — the IPC command previously skipped the
/// tunnel-stop step (see `known_bug_http_delete_stops_a_running_tunnel_before_deleting`
/// on `AppState::tunnel_manager`, now `_now_fixed`). Callers hold
/// `state.connections_lock` around this the same way `upsert_remote_connection`
/// does.
///
/// Returns `Ok(true)` if a connection with this id was found and removed,
/// `Ok(false)` if no connection with this id existed.
pub(crate) fn delete_remote_connection_impl(
    state: &crate::AppState,
    id: &str,
) -> Result<bool, String> {
    state.tunnel_manager.stop_if_running(id);
    let _ = crate::credentials::delete(crate::credentials::Credential::RemoteConnection(id));

    let mut connections =
        RemoteConnectionStore::load(&state.data_dir).map_err(|e| e.to_string())?;
    let before = connections.len();
    connections.retain(|c| c.id != id);
    if connections.len() == before {
        return Ok(false);
    }
    RemoteConnectionStore::save(&state.data_dir, &connections).map_err(|e| e.to_string())?;
    Ok(true)
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
    connection: RemoteConnection,
) -> Result<(), String> {
    let _guard = state.connections_lock.lock().await;
    upsert_remote_connection(&state.data_dir, connection)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn delete_remote_connection(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
    id: String,
) -> Result<(), String> {
    let _guard = state.connections_lock.lock().await;
    delete_remote_connection_impl(&state, &id)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Remote connection password (keyring) — story: SSH Tunnels + Remote Servers
// consolidation, Phase 3 auth wiring.
//
// State-free (the keyring vault is a process-global, not per-`AppState`), so
// these mirror `provider_registry.rs`'s `get_provider_api_key_exists` /
// `save_provider_api_key` / `delete_provider_api_key` exactly: only the
// `#[tauri::command]` attribute is conditional on the `desktop` feature, the
// function body itself is not gated, so both the Tauri command AND the HTTP
// handler (`mcp_http::config_routes`) call the same function directly with no
// separate `_impl` split needed.
//
// The password itself is NEVER written to `connections.json` — see
// `RemoteConnection::auth_username`'s doc comment and
// `credentials::Credential::RemoteConnection`.
// ---------------------------------------------------------------------------

#[cfg_attr(feature = "desktop", tauri::command)]
pub fn remote_connection_password_exists(id: String) -> Result<bool, String> {
    crate::credentials::get(crate::credentials::Credential::RemoteConnection(&id))
        .map(|v| v.is_some())
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub fn save_remote_connection_password(id: String, password: String) -> Result<(), String> {
    if password.is_empty() {
        return Err("Password must not be empty".to_string());
    }
    crate::credentials::set(
        crate::credentials::Credential::RemoteConnection(&id),
        &password,
    )
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub fn delete_remote_connection_password(id: String) -> Result<(), String> {
    crate::credentials::delete(crate::credentials::Credential::RemoteConnection(&id))
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
        assert_eq!(decoded.auth_username, Some("bob".to_string()));
        assert!(decoded.enabled);
        match decoded.transport {
            RemoteTransport::Direct { url } => assert_eq!(url, "http://office:9877"),
            other => panic!("expected Direct, got {other:?}"),
        }
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
                assert_eq!(instance_id, Some("dev-box".to_string()));
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

    /// A pre-Phase-1-shaped `auth_username: String` (always present, never
    /// null/missing) must still deserialize now that the field is
    /// `Option<String>` — this is the only backward-compat surface Phase 1
    /// actually needs (see this file's top doc comment on why a full
    /// migration wasn't built).
    #[test]
    fn legacy_string_auth_username_still_deserializes() {
        let json = serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "name": "legacy",
            "transport": {"type": "Direct", "url": "http://x"},
            "auth_username": "alice",
            "enabled": true,
        });
        let decoded: RemoteConnection = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.auth_username, Some("alice".to_string()));
    }

    #[test]
    fn missing_auth_username_defaults_to_none() {
        let json = serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "name": "no-auth",
            "transport": {"type": "Direct", "url": "http://x"},
            "enabled": true,
        });
        let decoded: RemoteConnection = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.auth_username, None);
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
        let local_json = serde_json::to_string(&local).unwrap();
        let local_val: serde_json::Value = serde_json::from_str(&local_json).unwrap();
        assert_eq!(local_val["transport"]["type"], "Local");
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
        assert_eq!(conn.auth_username, Some("myuser".to_string()));
    }

    #[test]
    fn new_direct_enabled() {
        let conn = RemoteConnection::new_direct("d", "http://x", "u");
        assert!(conn.enabled);
    }

    #[test]
    fn validate_valid_ssh_connection() {
        let conn = RemoteConnection::new_ssh("server", "host.example.com", "alice");
        assert!(conn.validate().is_ok());
    }

    #[test]
    fn validate_ssh_connection_without_auth_username_is_ok() {
        // auth_username is optional now — a connection with none set must
        // still validate (story: SSH Tunnels + Remote Servers consolidation).
        let mut conn = RemoteConnection::new_ssh("server", "host.example.com", "alice");
        conn.auth_username = None;
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

    #[test]
    fn validate_local_requires_port_or_instance_id() {
        let conn = RemoteConnection {
            id: uuid::Uuid::new_v4().to_string(),
            name: "local".to_string(),
            transport: RemoteTransport::Local {
                port: None,
                instance_id: None,
            },
            auth_username: None,
            enabled: true,
        };
        let err = conn.validate().unwrap_err();
        assert!(err.contains("port or an instance_id"), "{err}");
    }

    #[test]
    fn validate_local_rejects_both_port_and_instance_id() {
        let conn = RemoteConnection {
            id: uuid::Uuid::new_v4().to_string(),
            name: "local".to_string(),
            transport: RemoteTransport::Local {
                port: Some(9877),
                instance_id: Some("dev-box".to_string()),
            },
            auth_username: None,
            enabled: true,
        };
        let err = conn.validate().unwrap_err();
        assert!(err.contains("not both"), "{err}");
    }

    #[test]
    fn validate_local_port_accepted() {
        let conn = RemoteConnection::new_local_port("l", 9877);
        assert!(conn.validate().is_ok());
    }

    #[test]
    fn validate_local_instance_id_accepted() {
        let conn = RemoteConnection::new_local_instance("l", "dev-box");
        assert!(conn.validate().is_ok());
    }

    #[test]
    fn validate_local_zero_port_rejected() {
        let conn = RemoteConnection::new_local_port("l", 0);
        let err = conn.validate().unwrap_err();
        assert!(err.contains("port must be in range"), "{err}");
    }

    #[test]
    fn validate_local_blank_instance_id_treated_as_absent() {
        let conn = RemoteConnection {
            id: uuid::Uuid::new_v4().to_string(),
            name: "local".to_string(),
            transport: RemoteTransport::Local {
                port: None,
                instance_id: Some("   ".to_string()),
            },
            auth_username: None,
            enabled: true,
        };
        let err = conn.validate().unwrap_err();
        assert!(err.contains("port or an instance_id"), "{err}");
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
        let err = upsert_remote_connection(dir.path(), bad).unwrap_err();
        assert!(
            err.contains("valid UUID"),
            "expected UUID error, got: {err}"
        );
        assert!(
            RemoteConnectionStore::load(dir.path()).unwrap().is_empty(),
            "invalid input must not be written to the store"
        );
    }

    // --- IPC/HTTP parity bug, NOW FIXED (see plan Phase 1): `delete_remote_connection` ---
    //
    // Both the Tauri `delete_remote_connection` command and the HTTP
    // `delete_remote_connection` route now call the shared
    // `delete_remote_connection_impl`, which stops any running tunnel for
    // this connection id before removing it — previously only the HTTP route
    // did this (verified by reading its source pre-fix: the IPC command went
    // straight from loading the store to `retain`/`save`, with no
    // `tunnel_manager` reference at all). The IPC side still cannot be
    // exercised directly by a test here for the same reason as
    // `tunnels::tauri_commands`'s parity-bug tests (`tauri::State<'_, Arc<AppState>>`
    // has no public constructor outside a running Tauri app) — but since both
    // commands now call `delete_remote_connection_impl` with nothing else in
    // between, exercising it directly (as this test does) proves both sides.
    #[tokio::test]
    async fn delete_remote_connection_impl_stops_a_running_tunnel_before_deleting() {
        use crate::tunnels::profile::TunnelProfile;

        let state = crate::state::tests_support::make_test_app_state();
        let conn = RemoteConnection::new_ssh("test", "127.0.0.1", "nobody");
        let id = conn.id.clone();
        upsert_remote_connection(&state.data_dir, conn).unwrap();

        // Seed a "running" entry in the real `TunnelManager` under the same
        // id, via its only public entry point (there is no test-only way to
        // reach its private map from outside `tunnels::manager`). The
        // profile's host/port don't need to actually be reachable — `start`
        // publishes the entry into the manager's map synchronously, before
        // its background supervision loop ever attempts a real connection.
        let mut seed_profile = TunnelProfile::new("seed", "127.0.0.1", "nobody");
        seed_profile.id = id.clone();
        seed_profile.ssh.port = 1;
        state
            .tunnel_manager
            .start(seed_profile)
            .await
            .expect("seeding a tunnel manager entry must succeed");
        assert!(
            state.tunnel_manager.get_status(&id).is_some(),
            "seed tunnel must be visible in the manager before delete"
        );

        let deleted = delete_remote_connection_impl(&state, &id).unwrap();
        assert!(deleted, "connection must have been found and removed");

        assert!(
            state.tunnel_manager.get_status(&id).is_none(),
            "delete must stop the running tunnel for this connection id"
        );
        assert!(
            RemoteConnectionStore::load(&state.data_dir)
                .unwrap()
                .is_empty(),
            "connection must be removed from the store"
        );
    }

    #[test]
    fn delete_remote_connection_impl_returns_false_for_missing_id() {
        let state = crate::state::tests_support::make_test_app_state();
        let deleted = delete_remote_connection_impl(&state, "does-not-exist").unwrap();
        assert!(!deleted);
    }

    // --- Remote connection password commands (plan Phase 3 auth wiring) ---

    #[test]
    fn remote_connection_password_exists_false_when_unset() {
        crate::credentials::reset_test_faults();
        let id = uuid::Uuid::new_v4().to_string();
        assert_eq!(remote_connection_password_exists(id).unwrap(), false);
    }

    #[test]
    fn save_remote_connection_password_then_exists_is_true() {
        crate::credentials::reset_test_faults();
        let id = uuid::Uuid::new_v4().to_string();
        save_remote_connection_password(id.clone(), "hunter2".to_string()).unwrap();
        assert_eq!(remote_connection_password_exists(id.clone()).unwrap(), true);
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteConnection(&id)).unwrap(),
            Some("hunter2".to_string())
        );
    }

    #[test]
    fn save_remote_connection_password_rejects_empty() {
        crate::credentials::reset_test_faults();
        let id = uuid::Uuid::new_v4().to_string();
        let err = save_remote_connection_password(id, String::new()).unwrap_err();
        assert!(err.contains("must not be empty"), "{err}");
    }

    #[test]
    fn delete_remote_connection_password_clears_it() {
        crate::credentials::reset_test_faults();
        let id = uuid::Uuid::new_v4().to_string();
        save_remote_connection_password(id.clone(), "hunter2".to_string()).unwrap();
        delete_remote_connection_password(id.clone()).unwrap();
        assert_eq!(remote_connection_password_exists(id).unwrap(), false);
    }

    #[test]
    fn delete_remote_connection_impl_deletes_the_stored_credential() {
        crate::credentials::reset_test_faults();
        let state = crate::state::tests_support::make_test_app_state();
        let conn = RemoteConnection::new_ssh("test", "127.0.0.1", "nobody");
        let id = conn.id.clone();
        upsert_remote_connection(&state.data_dir, conn).unwrap();
        crate::credentials::set(
            crate::credentials::Credential::RemoteConnection(&id),
            "hunter2",
        )
        .unwrap();
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteConnection(&id)).unwrap(),
            Some("hunter2".to_string())
        );

        delete_remote_connection_impl(&state, &id).unwrap();

        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteConnection(&id)).unwrap(),
            None
        );
    }

    #[test]
    fn upsert_persists_valid_and_updates_existing() {
        let dir = tempfile::tempdir().unwrap();
        let conn = RemoteConnection::new_ssh("server", "host.example.com", "alice");
        let id = conn.id.clone();
        upsert_remote_connection(dir.path(), conn).unwrap();
        assert_eq!(RemoteConnectionStore::load(dir.path()).unwrap().len(), 1);

        // Same id updates in place rather than appending a duplicate.
        let mut updated = RemoteConnection::new_ssh("renamed", "host.example.com", "alice");
        updated.id = id.clone();
        upsert_remote_connection(dir.path(), updated).unwrap();
        let loaded = RemoteConnectionStore::load(dir.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, id);
        assert_eq!(loaded[0].name, "renamed");
    }

    // --- resolve_local_instance_port_at ---

    #[test]
    fn resolve_local_instance_port_missing_directory_is_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let err = resolve_local_instance_port_at("no-such-instance", None, &home).unwrap_err();
        assert_eq!(err, LocalInstancePortError::InstanceNotFound);
    }

    #[test]
    fn resolve_local_instance_port_invalid_id_is_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        // "default" is reserved by `AppInstance::named` and rejected outright.
        let err = resolve_local_instance_port_at("default", None, &home).unwrap_err();
        assert_eq!(err, LocalInstancePortError::InstanceNotFound);
    }

    #[test]
    fn resolve_local_instance_port_missing_config_file_is_unreadable() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let instance = crate::app_instance::AppInstance::named("dev-box").unwrap();
        let dir = instance.config_dir_from(None, &home);
        std::fs::create_dir_all(&dir).unwrap();
        // Directory exists, but no config.json inside it.

        let err = resolve_local_instance_port_at("dev-box", None, &home).unwrap_err();
        assert!(matches!(err, LocalInstancePortError::Unreadable(_)));
    }

    #[test]
    fn resolve_local_instance_port_corrupt_json_is_unreadable() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let instance = crate::app_instance::AppInstance::named("dev-box").unwrap();
        let dir = instance.config_dir_from(None, &home);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), "not valid json{{{").unwrap();

        let err = resolve_local_instance_port_at("dev-box", None, &home).unwrap_err();
        assert!(matches!(err, LocalInstancePortError::Unreadable(_)));
    }

    #[test]
    fn resolve_local_instance_port_zero_port_is_unreadable() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let instance = crate::app_instance::AppInstance::named("dev-box").unwrap();
        let dir = instance.config_dir_from(None, &home);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.json"),
            serde_json::json!({"services": {"server": {"port": 0}}}).to_string(),
        )
        .unwrap();

        let err = resolve_local_instance_port_at("dev-box", None, &home).unwrap_err();
        assert!(matches!(err, LocalInstancePortError::Unreadable(_)));
    }

    #[test]
    fn resolve_local_instance_port_reads_the_real_port() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let instance = crate::app_instance::AppInstance::named("dev-box").unwrap();
        let dir = instance.config_dir_from(None, &home);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.json"),
            serde_json::json!({"services": {"server": {"port": 9878}}}).to_string(),
        )
        .unwrap();

        let port = resolve_local_instance_port_at("dev-box", None, &home).unwrap();
        assert_eq!(port, 9878);
    }

    #[test]
    fn resolve_local_instance_port_ignores_unrelated_config_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let instance = crate::app_instance::AppInstance::named("dev-box").unwrap();
        let dir = instance.config_dir_from(None, &home);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.json"),
            serde_json::json!({
                "shell": "/bin/zsh",
                "services": {
                    "server": {"port": 9877, "enabled": true},
                    "auth": {"username": "boss"},
                },
            })
            .to_string(),
        )
        .unwrap();

        let port = resolve_local_instance_port_at("dev-box", None, &home).unwrap();
        assert_eq!(port, 9877);
    }
}
