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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StrictHostKeyChecking {
    Yes,
    AcceptNew,
}

impl SshConnectionParams {
    /// New params with this codebase's existing SSH defaults: port 22,
    /// `ServerAliveInterval=15`, `ServerAliveCountMax=3`,
    /// `StrictHostKeyChecking=yes`.
    pub fn new(host: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port: 22,
            user: user.into(),
            identity_file: None,
            server_alive_interval: 15,
            server_alive_count_max: 3,
            strict_host_key_checking: StrictHostKeyChecking::Yes,
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
        // Defense in depth alongside `tunnels::command`'s `--` end-of-options
        // marker (the actual fix — see its doc comment): a host/user
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
        if self.port == 0 {
            return Err("SSH port must be in range 1-65535".to_string());
        }
        Ok(())
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
    }

    #[test]
    fn toml_round_trip_preserves_all_fields() {
        let params = SshConnectionParams::new("host.example.com", "alice");
        let toml_str = toml::to_string(&params).expect("serialize");
        let decoded: SshConnectionParams = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(decoded.host, params.host);
        assert_eq!(decoded.port, params.port);
        assert_eq!(decoded.user, params.user);
    }
}
