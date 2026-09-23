use serde::{Deserialize, Serialize};

use crate::ssh_connection::SshConnectionParams;

/// Schema version for future migration support.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelProfile {
    pub id: String,
    pub name: String,
    /// SSH host/port/user/identity/keepalive config — shared with
    /// `RemoteTransport::Ssh` via `ssh_connection::SshConnectionParams`, so
    /// the two can never present different capabilities again.
    pub ssh: SshConnectionParams,
    pub forwards: Vec<ForwardSpec>,
    #[serde(default)]
    pub auto_connect: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ForwardSpec {
    Local {
        bind_port: u16,
        remote_host: String,
        remote_port: u16,
    },
    Remote {
        bind_port: u16,
        local_host: String,
        local_port: u16,
    },
}

impl TunnelProfile {
    pub fn new(name: impl Into<String>, host: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            ssh: SshConnectionParams::new(host, user),
            forwards: Vec::new(),
            auto_connect: false,
        }
    }

    pub fn validate(&mut self) -> Result<(), String> {
        if uuid::Uuid::parse_str(&self.id).is_err() {
            return Err("id must be a valid UUID".to_string());
        }
        self.name = self.name.trim().to_string();
        if self.name.is_empty() {
            return Err("name must not be empty".to_string());
        }
        self.ssh.host = self.ssh.host.trim().to_string();
        self.ssh.user = self.ssh.user.trim().to_string();
        self.ssh.validate()?;
        for forward in &self.forwards {
            match forward {
                ForwardSpec::Local {
                    bind_port,
                    remote_port,
                    ..
                } => {
                    if *bind_port == 0 {
                        return Err("forward bind_port must be in range 1-65535".to_string());
                    }
                    if *remote_port == 0 {
                        return Err("forward remote_port must be in range 1-65535".to_string());
                    }
                }
                ForwardSpec::Remote {
                    bind_port,
                    local_port,
                    ..
                } => {
                    if *bind_port == 0 {
                        return Err("forward bind_port must be in range 1-65535".to_string());
                    }
                    if *local_port == 0 {
                        return Err("forward local_port must be in range 1-65535".to_string());
                    }
                }
            }
        }
        // Check for duplicate bind ports across all forwards
        let mut seen_bind_ports = std::collections::HashSet::new();
        for forward in &self.forwards {
            let bind_port = match forward {
                ForwardSpec::Local { bind_port, .. } | ForwardSpec::Remote { bind_port, .. } => {
                    *bind_port
                }
            };
            if !seen_bind_ports.insert(bind_port) {
                return Err(format!("duplicate bind_port {bind_port} across forwards"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh_connection::StrictHostKeyChecking;
    use std::path::PathBuf;

    fn make_profile() -> TunnelProfile {
        TunnelProfile::new("my-tunnel", "example.com", "alice")
    }

    #[test]
    fn toml_round_trip() {
        let mut profile = make_profile();
        profile.ssh.port = 2222;
        profile.ssh.identity_file = Some(PathBuf::from("/home/alice/.ssh/id_ed25519"));
        profile.forwards = vec![
            ForwardSpec::Local {
                bind_port: 8080,
                remote_host: "internal.example.com".to_string(),
                remote_port: 80,
            },
            ForwardSpec::Remote {
                bind_port: 9090,
                local_host: "127.0.0.1".to_string(),
                local_port: 9090,
            },
        ];

        let serialized = toml::to_string(&profile).expect("serialize");
        let deserialized: TunnelProfile = toml::from_str(&serialized).expect("deserialize");

        assert_eq!(deserialized.id, profile.id);
        assert_eq!(deserialized.name, profile.name);
        assert_eq!(deserialized.ssh.host, profile.ssh.host);
        assert_eq!(deserialized.ssh.port, profile.ssh.port);
        assert_eq!(deserialized.ssh.user, profile.ssh.user);
        assert_eq!(deserialized.ssh.identity_file, profile.ssh.identity_file);
        assert_eq!(deserialized.forwards.len(), 2);
        assert_eq!(
            deserialized.ssh.server_alive_interval,
            profile.ssh.server_alive_interval
        );
        assert_eq!(
            deserialized.ssh.server_alive_count_max,
            profile.ssh.server_alive_count_max
        );
    }

    #[test]
    fn validate_ssh_port_zero_rejected() {
        let mut profile = make_profile();
        profile.ssh.port = 0;
        assert!(profile.validate().is_err());
    }

    #[test]
    fn validate_forward_bind_port_zero_rejected() {
        let mut profile = make_profile();
        profile.forwards = vec![ForwardSpec::Local {
            bind_port: 0,
            remote_host: "host".to_string(),
            remote_port: 80,
        }];
        assert!(profile.validate().is_err());
    }

    #[test]
    fn validate_forward_remote_port_zero_rejected() {
        let mut profile = make_profile();
        profile.forwards = vec![ForwardSpec::Local {
            bind_port: 8080,
            remote_host: "host".to_string(),
            remote_port: 0,
        }];
        assert!(profile.validate().is_err());
    }

    #[test]
    fn validate_duplicate_bind_ports_rejected() {
        let mut profile = make_profile();
        profile.forwards = vec![
            ForwardSpec::Local {
                bind_port: 8080,
                remote_host: "host".to_string(),
                remote_port: 80,
            },
            ForwardSpec::Remote {
                bind_port: 8080,
                local_host: "127.0.0.1".to_string(),
                local_port: 9000,
            },
        ];
        let err = profile.validate().unwrap_err();
        assert!(err.contains("duplicate bind_port"));
    }

    #[test]
    fn validate_empty_name_rejected() {
        let mut profile = make_profile();
        profile.name = String::new();
        assert!(profile.validate().is_err());
    }

    #[test]
    fn validate_empty_host_rejected() {
        let mut profile = make_profile();
        profile.ssh.host = String::new();
        assert!(profile.validate().is_err());
    }

    #[test]
    fn validate_empty_user_rejected() {
        let mut profile = make_profile();
        profile.ssh.user = String::new();
        assert!(profile.validate().is_err());
    }

    #[test]
    fn validate_valid_profile_ok() {
        let mut profile = make_profile();
        assert!(profile.validate().is_ok());
    }

    #[test]
    fn default_ssh_params_values() {
        let profile = make_profile();
        assert_eq!(profile.ssh.server_alive_interval, 15);
        assert_eq!(profile.ssh.server_alive_count_max, 3);
        assert!(matches!(
            profile.ssh.strict_host_key_checking,
            StrictHostKeyChecking::Yes
        ));
    }

    #[test]
    fn forward_local_serializes_with_type_tag() {
        let forward = ForwardSpec::Local {
            bind_port: 8080,
            remote_host: "host".to_string(),
            remote_port: 80,
        };
        let serialized = toml::to_string(&forward).expect("serialize");
        assert!(serialized.contains("type = \"Local\""));
        assert!(serialized.contains("bind_port"));
        assert!(serialized.contains("remote_host"));
        assert!(serialized.contains("remote_port"));
    }

    #[test]
    fn forward_remote_serializes_with_type_tag() {
        let forward = ForwardSpec::Remote {
            bind_port: 9090,
            local_host: "127.0.0.1".to_string(),
            local_port: 9090,
        };
        let serialized = toml::to_string(&forward).expect("serialize");
        assert!(serialized.contains("type = \"Remote\""));
        assert!(serialized.contains("bind_port"));
        assert!(serialized.contains("local_host"));
        assert!(serialized.contains("local_port"));
    }

    #[test]
    fn schema_version_is_one() {
        assert_eq!(SCHEMA_VERSION, 1);
    }

    #[test]
    fn validate_invalid_uuid_rejected() {
        let mut profile = make_profile();
        profile.id = "../../malicious".to_string();
        let err = profile.validate().unwrap_err();
        assert!(
            err.contains("valid UUID"),
            "expected UUID error, got: {err}"
        );
    }

    #[test]
    fn validate_whitespace_only_name_rejected() {
        let mut profile = make_profile();
        profile.name = "   ".to_string();
        assert!(profile.validate().is_err());
    }

    #[test]
    fn validate_whitespace_only_host_rejected() {
        let mut profile = make_profile();
        profile.ssh.host = "   ".to_string();
        assert!(profile.validate().is_err());
    }

    #[test]
    fn validate_whitespace_only_user_rejected() {
        let mut profile = make_profile();
        profile.ssh.user = "   ".to_string();
        assert!(profile.validate().is_err());
    }
}
