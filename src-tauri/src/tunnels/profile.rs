use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ssh_connection::{SshConnectionParams, StrictHostKeyChecking};

/// Schema version for future migration support.
pub const SCHEMA_VERSION: u32 = 1;

/// A port-forwarding profile.
///
/// Serialized with the SSH settings nested under `ssh`. Deserialization also
/// accepts the older flat shape (`host`/`port`/`user`/`identity_file` at the top
/// level plus an `[options]` table) — that is what every profile written before
/// the nested model looks like, and what an older HTTP client still sends — via
/// [`TunnelProfileWire`]. The one-time file rewrite is [`migrate_legacy_toml`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "TunnelProfileWire")]
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

/// Both on-disk/wire shapes of a [`TunnelProfile`]: the nested `ssh` table, or
/// the pre-nested flat fields plus `options`. Exactly one must be present.
#[derive(Deserialize)]
struct TunnelProfileWire {
    id: String,
    name: String,
    #[serde(default)]
    ssh: Option<SshConnectionParams>,
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    identity_file: Option<PathBuf>,
    #[serde(default)]
    options: Option<LegacyProfileOptions>,
    forwards: Vec<ForwardSpec>,
    #[serde(default)]
    auto_connect: bool,
}

/// The pre-nested `[options]` table. Its fields were required (only
/// `compression` had a default), and still are when the flat shape is used.
#[derive(Deserialize)]
struct LegacyProfileOptions {
    server_alive_interval: u16,
    server_alive_count_max: u16,
    strict_host_key_checking: StrictHostKeyChecking,
    #[serde(default = "crate::ssh_connection::compression_on")]
    compression: bool,
}

impl TryFrom<TunnelProfileWire> for TunnelProfile {
    type Error = String;

    fn try_from(wire: TunnelProfileWire) -> Result<Self, String> {
        let legacy = wire.host.is_some()
            || wire.port.is_some()
            || wire.user.is_some()
            || wire.identity_file.is_some()
            || wire.options.is_some();
        let ssh = match (wire.ssh, legacy) {
            (Some(ssh), false) => ssh,
            (Some(_), true) => {
                return Err(
                    "tunnel profile has both a nested `ssh` table and legacy flat \
                     host/port/user/identity_file/options fields"
                        .to_string(),
                );
            }
            (None, _) => {
                let missing = |field: &str| format!("missing field `{field}`");
                let options = wire.options.ok_or_else(|| missing("options"))?;
                SshConnectionParams {
                    host: wire.host.ok_or_else(|| missing("host"))?,
                    port: wire.port.ok_or_else(|| missing("port"))?,
                    user: wire.user.ok_or_else(|| missing("user"))?,
                    identity_file: wire.identity_file,
                    server_alive_interval: options.server_alive_interval,
                    server_alive_count_max: options.server_alive_count_max,
                    strict_host_key_checking: options.strict_host_key_checking,
                    compression: options.compression,
                }
            }
        };
        Ok(Self {
            id: wire.id,
            name: wire.name,
            ssh,
            forwards: wire.forwards,
            auto_connect: wire.auto_connect,
        })
    }
}

/// Top-level keys of the pre-nested shape that move into the `ssh` table
/// (every key of its `[options]` table moves there too).
const LEGACY_TOP_LEVEL_SSH_KEYS: [&str; 4] = ["host", "port", "user", "identity_file"];

/// Rewrite one profile document from the flat shape to the nested one, in
/// place. Returns `Ok(false)` (untouched) when it is already nested.
///
/// Keys are moved, not re-serialized from the typed struct, so nothing in the
/// document is dropped: the four connection fields and EVERY key of
/// `[options]` (known or not) land in `ssh`, and every other top-level key
/// (`id`, `name`, `forwards`, `auto_connect`, anything newer) stays where it
/// is. A profile written before `compression` existed gets
/// `compression = true`, which is what serde's default already gave it.
pub(crate) fn migrate_legacy_toml(doc: &mut toml::Table) -> Result<bool, String> {
    if doc.contains_key("ssh") || !doc.contains_key("host") {
        return Ok(false);
    }
    let options = match doc.get("options") {
        None => toml::Table::new(),
        Some(toml::Value::Table(options)) => options.clone(),
        Some(other) => {
            return Err(format!("`options` is a {}, not a table", other.type_str()));
        }
    };
    doc.remove("options");
    let mut ssh = toml::Table::new();
    for key in LEGACY_TOP_LEVEL_SSH_KEYS {
        if let Some(value) = doc.remove(key) {
            ssh.insert(key.to_string(), value);
        }
    }
    ssh.extend(options);
    ssh.entry("compression")
        .or_insert(toml::Value::Boolean(crate::ssh_connection::compression_on()));
    doc.insert("ssh".to_string(), toml::Value::Table(ssh));
    Ok(true)
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

    fn make_profile() -> TunnelProfile {
        TunnelProfile::new("my-tunnel", "example.com", "alice")
    }

    /// A profile as every pre-nested build wrote it (docs/features/ssh-tunnels.md's
    /// documented example, plus every optional field set to a non-default value).
    const LEGACY_PROFILE_TOML: &str = r#"
id = "550e8400-e29b-41d4-a716-446655440000"
name = "prod-db-tunnel"
host = "bastion.example.com"
port = 2222
user = "deploy"
identity_file = "/home/deploy/.ssh/id_ed25519"
auto_connect = true

[[forwards]]
type = "Local"
bind_port = 5432
remote_host = "db.internal"
remote_port = 5432

[[forwards]]
type = "Remote"
bind_port = 9090
local_host = "127.0.0.1"
local_port = 9090

[options]
server_alive_interval = 30
server_alive_count_max = 5
strict_host_key_checking = "AcceptNew"
compression = false
"#;

    fn assert_is_the_legacy_profile(profile: &TunnelProfile) {
        assert_eq!(profile.id, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(profile.name, "prod-db-tunnel");
        assert_eq!(profile.ssh.host, "bastion.example.com");
        assert_eq!(profile.ssh.port, 2222);
        assert_eq!(profile.ssh.user, "deploy");
        assert_eq!(
            profile.ssh.identity_file,
            Some(PathBuf::from("/home/deploy/.ssh/id_ed25519"))
        );
        assert_eq!(profile.ssh.server_alive_interval, 30);
        assert_eq!(profile.ssh.server_alive_count_max, 5);
        assert_eq!(
            profile.ssh.strict_host_key_checking,
            StrictHostKeyChecking::AcceptNew
        );
        assert!(!profile.ssh.compression);
        assert!(profile.auto_connect);
        assert_eq!(profile.forwards.len(), 2);
        assert!(matches!(
            &profile.forwards[0],
            ForwardSpec::Local { bind_port: 5432, remote_host, remote_port: 5432 } if remote_host == "db.internal"
        ));
        assert!(matches!(
            &profile.forwards[1],
            ForwardSpec::Remote { bind_port: 9090, local_host, local_port: 9090 } if local_host == "127.0.0.1"
        ));
    }

    #[test]
    fn a_profile_written_before_compression_existed_still_compresses() {
        // Every profile already on disk lacks the key. Deserializing it to
        // `false` would silently stop compressing the tunnels Boss already has,
        // which is the opposite of what adding the option was for.
        let legacy = LEGACY_PROFILE_TOML.replace("compression = false\n", "");
        let profile: TunnelProfile =
            toml::from_str(&legacy).expect("an old profile must still parse");
        assert!(profile.ssh.compression);

        let mut doc: toml::Table = toml::from_str(&legacy).unwrap();
        assert!(migrate_legacy_toml(&mut doc).unwrap());
        let migrated: TunnelProfile = toml::from_str(&toml::to_string(&doc).unwrap()).unwrap();
        assert!(migrated.ssh.compression);
    }

    #[test]
    fn the_flat_pre_nested_shape_still_parses_with_every_field() {
        let profile: TunnelProfile = toml::from_str(LEGACY_PROFILE_TOML).expect("legacy shape");
        assert_is_the_legacy_profile(&profile);
    }

    #[test]
    fn an_older_http_client_can_still_send_the_flat_json_shape() {
        let profile: TunnelProfile = serde_json::from_value(serde_json::json!({
            "id": "", "name": "t", "host": "h", "port": 22, "user": "u",
            "identity_file": null, "forwards": [],
            "options": { "server_alive_interval": 15, "server_alive_count_max": 3,
                         "strict_host_key_checking": "Yes" },
            "auto_connect": false
        }))
        .expect("flat JSON");
        assert_eq!(profile.ssh.host, "h");
        assert!(profile.ssh.compression);
        // ...and what goes back out is the nested shape only.
        let out = serde_json::to_value(&profile).unwrap();
        assert!(out.get("host").is_none() && out.get("options").is_none());
        assert_eq!(out["ssh"]["host"], "h");
    }

    #[test]
    fn a_profile_mixing_both_shapes_is_rejected() {
        let mut nested = toml::to_string(&make_profile()).unwrap();
        nested.insert_str(0, "host = \"other\"\n");
        let err = toml::from_str::<TunnelProfile>(&nested).unwrap_err();
        assert!(err.to_string().contains("both a nested"), "{err}");
    }

    #[test]
    fn the_flat_shape_without_options_is_still_an_error() {
        let legacy = LEGACY_PROFILE_TOML
            .split("[options]")
            .next()
            .unwrap()
            .to_string();
        let err = toml::from_str::<TunnelProfile>(&legacy).unwrap_err();
        assert!(err.to_string().contains("missing field `options`"), "{err}");
    }

    /// The round trip the migration exists for: old file -> migrated document
    /// -> typed profile is the same profile, and nothing in the document was
    /// dropped on the way (an unknown key at either level survives).
    #[test]
    fn migrating_the_legacy_document_loses_nothing() {
        let with_unknowns = LEGACY_PROFILE_TOML
            .replace(
                "auto_connect = true\n",
                "auto_connect = true\nfuture_top = \"kept\"\n",
            )
            .replace(
                "compression = false\n",
                "compression = false\nfuture_option = 7\n",
            );
        let mut doc: toml::Table = toml::from_str(&with_unknowns).unwrap();

        assert!(migrate_legacy_toml(&mut doc).unwrap());

        for gone in ["host", "port", "user", "identity_file", "options"] {
            assert!(!doc.contains_key(gone), "{gone} must have moved into ssh");
        }
        assert_eq!(doc["future_top"].as_str(), Some("kept"));
        assert_eq!(doc["ssh"]["future_option"].as_integer(), Some(7));
        let rendered = toml::to_string(&doc).unwrap();
        let migrated: TunnelProfile = toml::from_str(&rendered).unwrap();
        assert_is_the_legacy_profile(&migrated);

        // Idempotent: a second pass is a no-op.
        assert!(!migrate_legacy_toml(&mut doc).unwrap());
        assert_eq!(toml::to_string(&doc).unwrap(), rendered);
    }

    #[test]
    fn a_nested_profile_is_never_migrated() {
        let mut doc: toml::Table =
            toml::from_str(&toml::to_string(&make_profile()).unwrap()).unwrap();
        let before = doc.clone();
        assert!(!migrate_legacy_toml(&mut doc).unwrap());
        assert_eq!(doc, before);
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
