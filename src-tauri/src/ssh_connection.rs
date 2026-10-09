//! Shared SSH connection parameters.
//!
//! Single source of truth for "what does it take to reach a host over SSH,"
//! used by both `tunnels::profile::TunnelProfile` (port-forwarding profiles)
//! and `remote_connection::RemoteTransport::Ssh` (remote-server connections).
//! Before this module existed, the two types each carried their own flat copy
//! of these fields and had already drifted (`TunnelProfile` exposed
//! `server_alive_count_max` in its `ProfileOptions`; `RemoteTransport::Ssh`
//! didn't expose keepalive tuning at all) — see the SSH Tunnels + Remote
//! Servers consolidation plan's "Merged connection model" section.
//!
//! Both persisted files that embed this struct (`connections.json`,
//! `tunnels/*.toml`) used to store these fields flat. Their readers accept
//! either shape and a one-time, backed-up migration rewrites the old one —
//! see [`legacy`].

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Everything needed to open an SSH connection to a host, independent of what
/// happens over that connection (port forwards for a tunnel, a forwarded
/// `tuic-remote` daemon port for a remote server connection).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConnectionParams {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub identity_file: Option<PathBuf>,
    pub server_alive_interval: u16,
    pub server_alive_count_max: u16,
    pub strict_host_key_checking: StrictHostKeyChecking,
    /// `ssh -C`. On by default, and the reason the WebSocket layer refuses to
    /// deflate a loopback peer: a tunnelled client reaches this machine through
    /// the local ssh process, so its address is loopback and its link is
    /// already compressed here. Turn it off for a link that is fast and a CPU
    /// that is not — a tunnel to another machine on the same LAN.
    #[serde(default = "compression_on")]
    pub compression: bool,
}

/// Serde's default for params written before `compression` existed, matching
/// [`SshConnectionParams::new`] so an old record and a new one behave the same.
pub(crate) fn compression_on() -> bool {
    true
}

/// ssh's `StrictHostKeyChecking`, honoured as stored by every tunnel and
/// remote-server SSH command. Stored as `"Yes"` / `"AcceptNew"`; any other
/// value (a hand-edited `connections.json` or `tunnels/*.toml`, a future
/// variant read by an older build) loads as the strictest policy, `Yes`,
/// instead of failing the whole file or silently trusting a new host key.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum StrictHostKeyChecking {
    Yes,
    AcceptNew,
}

impl<'de> Deserialize<'de> for StrictHostKeyChecking {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "AcceptNew" => Self::AcceptNew,
            "Yes" => Self::Yes,
            other => {
                tracing::warn!(
                    source = "ssh",
                    value = %other,
                    "Unknown strict_host_key_checking value; using the strictest policy (Yes)"
                );
                Self::Yes
            }
        })
    }
}

impl SshConnectionParams {
    /// New params with this codebase's existing SSH defaults: port 22,
    /// `ServerAliveInterval=15`, `ServerAliveCountMax=3`,
    /// `StrictHostKeyChecking=yes`, `Compression=yes`.
    pub fn new(host: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port: 22,
            user: user.into(),
            identity_file: None,
            server_alive_interval: 15,
            server_alive_count_max: 3,
            strict_host_key_checking: StrictHostKeyChecking::Yes,
            compression: compression_on(),
        }
    }

    /// Validate without mutating. Callers that also want whitespace trimmed
    /// into the stored value (e.g. `TunnelProfile::validate`, which already
    /// trims its own `name` field the same way) should trim `host`/`user`
    /// themselves before calling this — this method only checks.
    pub fn validate(&self) -> Result<(), String> {
        if self.host.trim().is_empty() {
            return Err("host must not be empty".to_string());
        }
        if self.user.trim().is_empty() {
            return Err("user must not be empty".to_string());
        }
        // Defense in depth alongside the `--` end-of-options marker every ssh
        // argv builder puts before the destination (`tunnels::command`,
        // `tunnels::exec` — the actual fix): a host/user
        // starting with `-` is real OpenSSH option-injection when placed as
        // the destination argv token (confirmed exploitable via
        // `ProxyCommand`, security review 2026-09-23). `--` alone already
        // closes this regardless of what's validated, but rejecting it here
        // too gives a clear error at Save time instead of a confusing ssh
        // failure, for the callers that reach this validator (Test
        // Connection's ad hoc unsaved form data does not, by design — it's
        // covered by `--` alone).
        if self.host.trim_start().starts_with('-') {
            return Err("host must not start with '-'".to_string());
        }
        if self.user.trim_start().starts_with('-') {
            return Err("user must not start with '-'".to_string());
        }
        // An unambiguous destination (Batch 32/34 reviews): `user@host` is
        // built by joining the two, and scp reads `host:path`, so `@` in
        // either half, `:` in the user, or a `:` that isn't part of an IPv6
        // literal in the host would make the displayed and the dialled
        // destination differ (or turn part of the host into an scp path).
        // Whitespace and control characters are never part of a host or user
        // (checked untrimmed: not every caller trims before validating, and
        // the raw value is what reaches ssh).
        let bad_char = |c: char| c.is_whitespace() || c.is_control() || c == '@';
        if self
            .host
            .chars()
            .any(|c| bad_char(c) || c == '[' || c == ']')
        {
            return Err(
                "host must not contain '@', brackets, whitespace or control characters".to_string(),
            );
        }
        if self.host.contains(':') && !is_ipv6_literal(&self.host) {
            return Err("host must not contain ':' (only a bare IPv6 address may)".to_string());
        }
        if self.user.chars().any(|c| bad_char(c) || c == ':') {
            return Err(
                "user must not contain '@', ':', whitespace or control characters".to_string(),
            );
        }
        if self.port == 0 {
            return Err("SSH port must be in range 1-65535".to_string());
        }
        Ok(())
    }
}

/// A bare IPv6 address, optionally with a `%zone` (`fe80::1%en0`) — the only
/// host form allowed to contain `:`. ssh takes it as written; scp needs it
/// bracketed (`tunnels::exec::scp_destination`).
pub(crate) fn is_ipv6_literal(host: &str) -> bool {
    let (addr, zone) = host.split_once('%').unwrap_or((host, ""));
    addr.parse::<std::net::Ipv6Addr>().is_ok()
        && (host.split_once('%').is_none()
            || (!zone.is_empty()
                && zone
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')))
}

/// One-time rewrite of the pre-nested (flat) on-disk shapes.
///
/// The readers (`RemoteTransport`'s and `TunnelProfile`'s `Deserialize`) accept
/// both shapes, so nothing breaks before or without this. The rewrite exists so
/// a file is not left in a shape the next writer would silently change anyway,
/// and so the change happens once, visibly, with the original kept beside it.
///
/// Every step works on the untyped document (`serde_json::Value` /
/// `toml::Value`) and moves keys rather than round-tripping through the typed
/// structs, so a field this build does not know about survives the rewrite.
/// The migrated document must still parse as the typed struct before anything
/// is written; if it does not, the file is left untouched.
pub(crate) mod legacy {
    use std::path::{Path, PathBuf};

    /// Suffix that marks a pre-migration backup. Kept out of every loader's
    /// pattern on purpose: `tunnels/` only reads `*.toml`, and nothing reads
    /// `connections.json.*`.
    const BACKUP_TAG: &str = "pre-nested-ssh";

    /// `<file>.pre-nested-ssh-<UTC timestamp>.bak` next to `path`; a random
    /// suffix keeps two migrations in the same millisecond from sharing a name,
    /// so an existing backup is never overwritten.
    pub(crate) fn backup_path(path: &Path) -> PathBuf {
        let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let base = path.with_file_name(format!("{name}.{BACKUP_TAG}-{stamp}.bak"));
        if !base.exists() {
            return base;
        }
        let unique = uuid::Uuid::new_v4().simple().to_string();
        path.with_file_name(format!("{name}.{BACKUP_TAG}-{stamp}-{unique}.bak"))
    }

    /// Keep `original` as a backup next to `path`, then atomically replace
    /// `path` with `migrated`. The backup is written (and fsynced) first, so a
    /// crash between the two steps leaves the original in both places rather
    /// than nowhere.
    pub(crate) fn backup_then_replace(
        path: &Path,
        original: &[u8],
        migrated: &[u8],
    ) -> Result<PathBuf, String> {
        let backup = backup_path(path);
        crate::config::persist_atomic(&backup, original)
            .map_err(|e| format!("could not back up {}: {e}", path.display()))?;
        crate::config::persist_atomic(path, migrated)
            .map_err(|e| format!("could not rewrite {}: {e}", path.display()))?;
        Ok(backup)
    }

    /// Boot-time entry point: migrate `connections.json` and the global
    /// `tunnels/*.toml` profiles under `config_dir`. Never fails the boot — a
    /// file that could not be rewritten is logged and keeps being read in its
    /// old shape.
    pub(crate) fn migrate_persisted_shapes(config_dir: &Path) {
        match crate::remote_connection::migrate_legacy_connections_file(config_dir) {
            Ok(Some(backup)) => tracing::info!(
                source = "remote",
                backup = %backup.display(),
                "Migrated connections.json to the nested SSH shape; the original is kept as the backup"
            ),
            Ok(None) => {}
            Err(error) => tracing::warn!(
                source = "remote",
                %error,
                "connections.json was not migrated; it is still read in its old shape"
            ),
        }
        match crate::tunnels::storage::ProfileStore::migrate_legacy_global_profiles(config_dir) {
            Ok(backups) => {
                for backup in backups {
                    tracing::info!(
                        source = "tunnels",
                        backup = %backup.display(),
                        "Migrated a tunnel profile to the nested SSH shape; the original is kept as the backup"
                    );
                }
            }
            Err(error) => tracing::warn!(
                source = "tunnels",
                error = %error,
                "Some tunnel profiles were not migrated; they are still read in their old shape"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_has_expected_defaults() {
        let params = SshConnectionParams::new("example.com", "alice");
        assert_eq!(params.host, "example.com");
        assert_eq!(params.port, 22);
        assert_eq!(params.user, "alice");
        assert!(params.identity_file.is_none());
        assert_eq!(params.server_alive_interval, 15);
        assert_eq!(params.server_alive_count_max, 3);
        assert_eq!(params.strict_host_key_checking, StrictHostKeyChecking::Yes);
        assert!(params.compression);
    }

    /// Catches: an unrecognised stored host-key policy failing the whole
    /// connections/tunnels file, or (worse) loading as `AcceptNew`.
    #[test]
    fn an_unknown_host_key_policy_loads_as_the_strictest() {
        let parse = |v: &str| -> StrictHostKeyChecking {
            serde_json::from_value(serde_json::Value::String(v.to_string())).unwrap()
        };
        assert_eq!(parse("Yes"), StrictHostKeyChecking::Yes);
        assert_eq!(parse("AcceptNew"), StrictHostKeyChecking::AcceptNew);
        for unknown in ["No", "Off", "accept-new", "acceptnew", "ask", ""] {
            assert_eq!(parse(unknown), StrictHostKeyChecking::Yes, "{unknown:?}");
        }
        assert!(serde_json::from_value::<StrictHostKeyChecking>(serde_json::json!(false)).is_err());
    }

    #[test]
    fn validate_valid_params_ok() {
        assert!(SshConnectionParams::new("host", "user").validate().is_ok());
    }

    #[test]
    fn validate_empty_host_rejected() {
        let params = SshConnectionParams::new("", "user");
        let err = params.validate().unwrap_err();
        assert!(err.contains("host must not be empty"), "{err}");
    }

    #[test]
    fn validate_whitespace_only_host_rejected() {
        let params = SshConnectionParams::new("   ", "user");
        assert!(params.validate().is_err());
    }

    #[test]
    fn validate_empty_user_rejected() {
        let params = SshConnectionParams::new("host", "");
        let err = params.validate().unwrap_err();
        assert!(err.contains("user must not be empty"), "{err}");
    }

    // Regression tests for a real, confirmed-exploitable option-injection
    // finding (security review 2026-09-23) — see `tunnels::command`'s `--`
    // marker doc comment for the full exploit chain this defends in depth.
    #[test]
    fn validate_host_starting_with_dash_rejected() {
        let params = SshConnectionParams::new("-oProxyCommand=touch /tmp/pwned", "user");
        let err = params.validate().unwrap_err();
        assert!(err.contains("host must not start with '-'"), "{err}");
    }

    #[test]
    fn validate_user_starting_with_dash_rejected() {
        let params = SshConnectionParams::new("host", "-oProxyCommand=touch /tmp/pwned");
        let err = params.validate().unwrap_err();
        assert!(err.contains("user must not start with '-'"), "{err}");
    }

    /// Batch 32/34 reviews: `@`, `:` and whitespace/control characters made
    /// the destination ambiguous (and `host:path` mis-parse in scp).
    #[test]
    fn validate_rejects_ambiguous_destination_characters() {
        for (host, user) in [
            ("evil@host", "user"),
            ("host", "us@er"),
            ("host", "us:er"),
            ("host:22", "user"),
            ("host:/etc/passwd", "user"),
            ("ho st", "user"),
            ("host", "us er"),
            ("host\tname", "user"),
            ("host\u{7}", "user"),
            ("host", "user\n"),
            ("[::1]", "user"),
            ("-::1", "user"),
            ("::1%", "user"),
        ] {
            assert!(
                SshConnectionParams::new(host, user).validate().is_err(),
                "{host:?} / {user:?} must be rejected"
            );
        }
    }

    #[test]
    fn validate_accepts_hostnames_ipv4_and_bare_ipv6() {
        for host in [
            "host.example.com",
            "10.0.0.1",
            "::1",
            "2001:db8::42",
            "fe80::1%en0",
            "my-host_1",
        ] {
            assert!(
                SshConnectionParams::new(host, "deploy.user-1")
                    .validate()
                    .is_ok(),
                "{host:?} must be accepted"
            );
        }
    }

    #[test]
    fn validate_port_zero_rejected() {
        let mut params = SshConnectionParams::new("host", "user");
        params.port = 0;
        let err = params.validate().unwrap_err();
        assert!(err.contains("SSH port must be in range"), "{err}");
    }

    #[test]
    fn json_round_trip_preserves_all_fields() {
        let mut params = SshConnectionParams::new("host.example.com", "alice");
        params.port = 2222;
        params.identity_file = Some(PathBuf::from("/home/alice/.ssh/id_ed25519"));
        params.strict_host_key_checking = StrictHostKeyChecking::AcceptNew;
        params.compression = false;

        let json = serde_json::to_string(&params).unwrap();
        let decoded: SshConnectionParams = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.host, params.host);
        assert_eq!(decoded.port, params.port);
        assert_eq!(decoded.user, params.user);
        assert_eq!(decoded.identity_file, params.identity_file);
        assert_eq!(decoded.server_alive_interval, params.server_alive_interval);
        assert_eq!(
            decoded.server_alive_count_max,
            params.server_alive_count_max
        );
        assert_eq!(
            decoded.strict_host_key_checking,
            params.strict_host_key_checking
        );
        assert!(!decoded.compression);
    }

    #[test]
    fn toml_round_trip_preserves_all_fields() {
        let params = SshConnectionParams::new("host.example.com", "alice");
        let toml_str = toml::to_string(&params).expect("serialize");
        let decoded: SshConnectionParams = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(decoded.host, params.host);
        assert_eq!(decoded.port, params.port);
        assert_eq!(decoded.user, params.user);
        assert!(decoded.compression);
    }

    #[test]
    fn params_written_before_compression_existed_still_compress() {
        let decoded: SshConnectionParams = serde_json::from_value(serde_json::json!({
            "host": "h", "port": 22, "user": "u", "identity_file": null,
            "server_alive_interval": 15, "server_alive_count_max": 3,
            "strict_host_key_checking": "Yes"
        }))
        .unwrap();
        assert!(decoded.compression);
    }

    #[test]
    fn backup_path_is_next_to_the_file_and_never_reused() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let file = dir.path().join("connections.json");
        let first = legacy::backup_path(&file);
        assert_eq!(first.parent(), Some(dir.path()));
        let name = first.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("connections.json.pre-nested-ssh-") && name.ends_with(".bak"),
            "{name}"
        );
        std::fs::write(&first, b"x").unwrap();
        let second = legacy::backup_path(&file);
        assert_ne!(
            first, second,
            "an existing backup must never be overwritten"
        );
    }
}
