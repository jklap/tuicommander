//! SSH host discovery: merges `~/.ssh/config` aliases with `~/.ssh/known_hosts`
//! names into one deduplicated list. Pure functions; file access lives in
//! `commands.rs`.

use std::collections::HashSet;

use serde::Serialize;

const DEFAULT_SSH_PORT: u16 = 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HostSource {
    Config,
    KnownHosts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DiscoveredHost {
    pub(crate) host: String,
    pub(crate) user: Option<String>,
    pub(crate) port: Option<u16>,
    pub(crate) source: HostSource,
}

impl DiscoveredHost {
    /// Port to pass to `ssh -p`. A config alias carries its own port in the
    /// config, which ssh applies by itself.
    pub(crate) fn probe_port(&self) -> Option<u16> {
        match self.source {
            HostSource::Config => None,
            HostSource::KnownHosts => self.port,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(crate) struct DiscoveredHosts {
    pub(crate) hosts: Vec<DiscoveredHost>,
    /// known_hosts lines whose name is hashed: present on disk, not listable.
    pub(crate) hashed_count: usize,
}

/// A non-wildcard `Host` alias from `~/.ssh/config` with the parameters that
/// identify the machine behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigHost {
    pub(crate) alias: String,
    pub(crate) hostname: Option<String>,
    pub(crate) user: Option<String>,
    pub(crate) port: Option<u16>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct KnownHosts {
    pub(crate) hosts: Vec<(String, Option<u16>)>,
    pub(crate) hashed_count: usize,
}

pub(crate) fn is_wildcard(pattern: &str) -> bool {
    pattern.contains(['*', '?'])
}

/// Parse OpenSSH known_hosts text. Hashed names (`|1|…`) are counted, marker
/// lines (`@cert-authority`, `@revoked`) and wildcard/negated patterns are
/// skipped (a leading `-` would reach ssh as an option); `[host]:port` and comma-separated names are expanded.
pub(crate) fn parse_known_hosts(text: &str) -> KnownHosts {
    let mut known = KnownHosts::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('@') {
            continue;
        }
        let Some(names) = line.split_whitespace().next() else {
            continue;
        };
        if names.starts_with("|1|") {
            known.hashed_count += 1;
            continue;
        }
        for name in names.split(',') {
            if name.is_empty() || name.starts_with(['!', '-']) || is_wildcard(name) {
                continue;
            }
            known.hosts.push(split_host_port(name));
        }
    }
    known
}

fn split_host_port(name: &str) -> (String, Option<u16>) {
    if let Some(rest) = name.strip_prefix('[')
        && let Some((host, tail)) = rest.split_once(']')
    {
        let port = tail.strip_prefix(':').and_then(|p| p.parse().ok());
        return (host.to_string(), port);
    }
    (name.to_string(), None)
}

fn dedupe_key(host: &str, port: Option<u16>) -> (String, u16) {
    (host.to_ascii_lowercase(), port.unwrap_or(DEFAULT_SSH_PORT))
}

/// Config aliases first, then known_hosts names whose resolved (host, port)
/// no alias and no earlier line already covers.
pub(crate) fn merge_discovered(config: Vec<ConfigHost>, known: KnownHosts) -> DiscoveredHosts {
    let mut seen = HashSet::new();
    let mut hosts = Vec::new();
    for entry in config {
        let resolved = entry.hostname.as_deref().unwrap_or(&entry.alias);
        if seen.insert(dedupe_key(resolved, entry.port)) {
            hosts.push(DiscoveredHost {
                host: entry.alias,
                user: entry.user,
                port: entry.port,
                source: HostSource::Config,
            });
        }
    }
    for (host, port) in known.hosts {
        if seen.insert(dedupe_key(&host, port)) {
            hosts.push(DiscoveredHost {
                host,
                user: None,
                port,
                source: HostSource::KnownHosts,
            });
        }
    }
    DiscoveredHosts {
        hosts,
        hashed_count: known.hashed_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alias(alias: &str, hostname: Option<&str>, port: Option<u16>) -> ConfigHost {
        ConfigHost {
            alias: alias.to_string(),
            hostname: hostname.map(str::to_string),
            user: None,
            port,
        }
    }

    #[test]
    fn known_hosts_hashed_and_marker_lines_are_not_listed_as_hosts() {
        let text = "\
# comment
|1|c2FsdA==|aGFzaA== ssh-ed25519 AAAA
@cert-authority *.corp.example ssh-rsa AAAA
@revoked revoked.example ssh-rsa AAAA
plain.example ssh-ed25519 AAAA
";
        let known = parse_known_hosts(text);
        assert_eq!(known.hosts, vec![("plain.example".to_string(), None)]);
        assert_eq!(known.hashed_count, 1);
    }

    #[test]
    fn known_hosts_expands_ports_comma_names_and_skips_patterns() {
        let text = "\
[host.example]:2222 ssh-ed25519 AAAA
name.example,10.0.0.5 ssh-rsa AAAA
[::1]:2200 ssh-rsa AAAA
*.wild.example ssh-rsa AAAA
ok.example,!neg.example ssh-rsa AAAA
-oProxyCommand=evil ssh-rsa AAAA
";
        let known = parse_known_hosts(text);
        assert_eq!(
            known.hosts,
            vec![
                ("host.example".to_string(), Some(2222)),
                ("name.example".to_string(), None),
                ("10.0.0.5".to_string(), None),
                ("::1".to_string(), Some(2200)),
                ("ok.example".to_string(), None),
            ]
        );
        assert_eq!(known.hashed_count, 0);
    }

    #[test]
    fn a_host_in_config_and_known_hosts_is_listed_once() {
        let config = vec![alias("vps", Some("vps.example"), None)];
        let known = parse_known_hosts(
            "vps.example ssh-ed25519 AAAA\nVPS.example ssh-rsa BBBB\n[vps.example]:2222 ssh-rsa CCCC\n",
        );
        let merged = merge_discovered(config, known);
        let names: Vec<(&str, Option<u16>, HostSource)> = merged
            .hosts
            .iter()
            .map(|h| (h.host.as_str(), h.port, h.source))
            .collect();
        assert_eq!(
            names,
            vec![
                ("vps", None, HostSource::Config),
                ("vps.example", Some(2222), HostSource::KnownHosts),
            ],
            "same host+port must not repeat; a different port is a different machine entry"
        );
    }

    #[test]
    fn config_alias_without_hostname_dedupes_by_alias_itself() {
        let merged = merge_discovered(
            vec![alias("bare.example", None, None)],
            parse_known_hosts("bare.example ssh-ed25519 AAAA\n"),
        );
        assert_eq!(merged.hosts.len(), 1);
        assert_eq!(merged.hosts[0].source, HostSource::Config);
    }

    #[test]
    fn probe_port_is_only_passed_for_known_hosts_entries() {
        let merged = merge_discovered(
            vec![alias("a", Some("a.example"), Some(2200))],
            parse_known_hosts("[b.example]:2222 ssh-rsa AAAA\n"),
        );
        assert_eq!(merged.hosts[0].probe_port(), None);
        assert_eq!(merged.hosts[1].probe_port(), Some(2222));
    }
}
