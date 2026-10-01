use std::collections::BTreeSet;
use std::io::BufReader;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde::Serialize;

use super::discovery::{self, ConfigHost, DiscoveredHost, DiscoveredHosts};
use super::profile::TunnelProfile;
use super::storage::ProfileStore;
use crate::AppState;

/// JSON error helper.
fn err_json(status: StatusCode, msg: &str) -> Response {
    (status, Json(serde_json::json!({"error": msg}))).into_response()
}

// ── Profile CRUD ────────────────────────────────────────────

/// GET /tunnels/profiles — list all saved profiles.
pub(crate) async fn list_tunnel_profiles(State(state): State<Arc<AppState>>) -> Response {
    match ProfileStore::load_all(&state.data_dir, None) {
        Ok(profiles) => (StatusCode::OK, Json(serde_json::json!(profiles))).into_response(),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// POST /tunnels/profiles — create or update a profile.
pub(crate) async fn save_tunnel_profile(
    State(state): State<Arc<AppState>>,
    Json(mut profile): Json<TunnelProfile>,
) -> Response {
    if let Err(e) = profile.validate() {
        return err_json(StatusCode::BAD_REQUEST, &e);
    }
    match ProfileStore::save(&state.data_dir, &profile) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"id": profile.id}))).into_response(),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// DELETE /tunnels/profiles/:id — delete a profile, stopping its tunnel if active.
pub(crate) async fn delete_tunnel_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    state.tunnel_manager.stop_if_running(&id);

    match ProfileStore::delete(&state.data_dir, None, &id) {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({"deleted": true}))).into_response(),
        Ok(false) => err_json(StatusCode::NOT_FOUND, "profile not found"),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// ── Tunnel lifecycle ────────────────────────────────────────

/// POST /tunnels/start/:id — load profile from storage and start its tunnel.
pub(crate) async fn start_tunnel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    let profiles = match ProfileStore::load_all(&state.data_dir, None) {
        Ok(p) => p,
        Err(e) => return err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let profile = match profiles.into_iter().find(|p| p.id == id) {
        Some(p) => p,
        None => return err_json(StatusCode::NOT_FOUND, "profile not found"),
    };

    let result = state.tunnel_manager.start(profile).await;

    match result {
        Ok(tunnel_id) => {
            (StatusCode::OK, Json(serde_json::json!({"id": tunnel_id}))).into_response()
        }
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

/// POST /tunnels/stop/:id — stop an active tunnel.
pub(crate) async fn stop_tunnel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match state.tunnel_manager.stop(&id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"stopped": true}))).into_response(),
        Err(e) => err_json(StatusCode::NOT_FOUND, &e),
    }
}

// ── Status queries ──────────────────────────────────────────

/// GET /tunnels/active — list all running tunnels with status.
pub(crate) async fn list_active_tunnels(State(state): State<Arc<AppState>>) -> Response {
    let list = state.tunnel_manager.list();
    let entries: Vec<serde_json::Value> = list
        .into_iter()
        .map(|(id, status)| serde_json::json!({"id": id, "status": status}))
        .collect();
    (StatusCode::OK, Json(serde_json::json!(entries))).into_response()
}

/// GET /tunnels/status/:id — single tunnel status.
pub(crate) async fn get_tunnel_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match state.tunnel_manager.get_status(&id) {
        Some(status) => (
            StatusCode::OK,
            Json(serde_json::json!({"id": id, "status": status})),
        )
            .into_response(),
        None => err_json(StatusCode::NOT_FOUND, "tunnel not found"),
    }
}

// ── Audit log ───────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct AuditQuery {
    limit: Option<usize>,
}

/// GET /tunnels/audit/:id — audit log for a tunnel.
pub(crate) async fn get_tunnel_audit(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<AuditQuery>,
) -> Response {
    let limit = query.limit.unwrap_or(20);
    match state.tunnel_audit.lock().query_by_tunnel(&id, limit) {
        Ok(events) => (StatusCode::OK, Json(serde_json::json!(events))).into_response(),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// ── SSH config hosts ────────────────────────────────────────

const SSH_PROBE_CACHE_TTL: Duration = Duration::from_secs(60);
const SSH_PROBE_TIMEOUT: Duration = Duration::from_secs(7);
const SSH_PROBE_CONCURRENCY: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HostAuth {
    Shell,
    NoShell,
    AuthFailed,
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SshHostStatus {
    pub(crate) host: String,
    pub(crate) port: Option<u16>,
    pub(crate) auth: HostAuth,
}

struct ProbeCacheEntry {
    stored_at: Instant,
    hosts: Vec<DiscoveredHost>,
    statuses: Vec<SshHostStatus>,
}

static SSH_PROBE_CACHE: std::sync::OnceLock<tokio::sync::Mutex<Option<ProbeCacheEntry>>> =
    std::sync::OnceLock::new();

fn ssh_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh").join("config"))
}

pub(crate) fn load_ssh_config_hosts() -> Result<Vec<String>, String> {
    let Some(path) = ssh_config_path() else {
        return Ok(Vec::new());
    };
    parse_ssh_config_hosts(&path)
}

fn parse_ssh_config_hosts(path: &FsPath) -> Result<Vec<String>, String> {
    Ok(parse_ssh_config_entries(path)?
        .into_iter()
        .map(|entry| entry.alias)
        .collect())
}

fn parse_ssh_config_entries(path: &FsPath) -> Result<Vec<ConfigHost>, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("failed to read SSH config: {error}")),
    };
    let mut reader = BufReader::new(file);
    let config = ssh2_config::SshConfig::default()
        .parse(&mut reader, ssh2_config::ParseRule::ALLOW_UNKNOWN_FIELDS)
        .map_err(|error| format!("failed to parse SSH config: {error}"))?;
    let aliases: BTreeSet<&str> = config
        .get_hosts()
        .iter()
        .flat_map(|host| &host.pattern)
        .filter(|clause| !clause.negated && !discovery::is_wildcard(&clause.pattern))
        .map(|clause| clause.pattern.as_str())
        .collect();
    Ok(aliases
        .into_iter()
        .map(|alias| {
            let params = config.query(alias);
            ConfigHost {
                alias: alias.to_string(),
                hostname: params.host_name,
                user: params.user,
                port: params.port,
            }
        })
        .collect())
}

fn known_hosts_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh").join("known_hosts"))
}

fn read_known_hosts(path: &FsPath) -> Result<discovery::KnownHosts, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(discovery::parse_known_hosts(&text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Default::default()),
        Err(error) => Err(format!("failed to read known_hosts: {error}")),
    }
}

pub(crate) fn load_discovered_hosts() -> Result<DiscoveredHosts, String> {
    let config = match ssh_config_path() {
        Some(path) => parse_ssh_config_entries(&path)?,
        None => Vec::new(),
    };
    let known = match known_hosts_path() {
        Some(path) => read_known_hosts(&path)?,
        None => Default::default(),
    };
    Ok(discovery::merge_discovered(config, known))
}

/// GET /tunnels/ssh-hosts/discovered — config aliases plus known_hosts names.
pub(crate) async fn list_discovered_ssh_hosts_http() -> Response {
    match load_discovered_hosts() {
        Ok(discovered) => (StatusCode::OK, Json(serde_json::json!(discovered))).into_response(),
        Err(error) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &error),
    }
}

/// GET /tunnels/ssh-hosts — parse ~/.ssh/config and return host aliases.
pub(crate) async fn list_ssh_config_hosts() -> Response {
    match load_ssh_config_hosts() {
        Ok(hosts) => (StatusCode::OK, Json(serde_json::json!(hosts))).into_response(),
        Err(error) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &error),
    }
}

pub(crate) async fn probe_ssh_config_hosts_http() -> Response {
    match probe_ssh_config_hosts().await {
        Ok(statuses) => (StatusCode::OK, Json(serde_json::json!(statuses))).into_response(),
        Err(error) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &error),
    }
}

pub(crate) async fn probe_ssh_config_hosts() -> Result<Vec<SshHostStatus>, String> {
    let hosts = load_discovered_hosts()?.hosts;
    let cache = SSH_PROBE_CACHE.get_or_init(|| tokio::sync::Mutex::new(None));
    probe_cached(cache, hosts, FsPath::new("ssh"), SSH_PROBE_TIMEOUT).await
}

async fn probe_cached(
    cache: &tokio::sync::Mutex<Option<ProbeCacheEntry>>,
    hosts: Vec<DiscoveredHost>,
    binary: &FsPath,
    timeout: Duration,
) -> Result<Vec<SshHostStatus>, String> {
    let mut cached = cache.lock().await;
    if let Some(entry) = cached.as_ref()
        && entry.hosts == hosts
        && entry.stored_at.elapsed() < SSH_PROBE_CACHE_TTL
    {
        return Ok(entry.statuses.clone());
    }
    let statuses = probe_hosts_with_binary(hosts.clone(), binary, timeout).await;
    *cached = Some(ProbeCacheEntry {
        stored_at: Instant::now(),
        hosts,
        statuses: statuses.clone(),
    });
    Ok(statuses)
}

async fn probe_hosts_with_binary(
    hosts: Vec<DiscoveredHost>,
    binary: &FsPath,
    timeout: Duration,
) -> Vec<SshHostStatus> {
    use futures_util::StreamExt;
    futures_util::stream::iter(hosts.into_iter().map(|host| async move {
        let auth = probe_host_with_binary(&host.host, host.probe_port(), binary, timeout).await;
        SshHostStatus {
            host: host.host,
            port: host.port,
            auth,
        }
    }))
    .buffer_unordered(SSH_PROBE_CONCURRENCY)
    .collect()
    .await
}

fn probe_args(host: &str, port: Option<u16>) -> Vec<String> {
    let mut args = vec![
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "ConnectTimeout=5".into(),
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
    ];
    if let Some(port) = port {
        args.push("-p".into());
        args.push(port.to_string());
    }
    args.push(host.to_string());
    args.push("true".into());
    args
}

async fn probe_host_with_binary(
    host: &str,
    port: Option<u16>,
    binary: &FsPath,
    timeout: Duration,
) -> HostAuth {
    let mut command = tokio::process::Command::new(binary);
    command
        .args(probe_args(host, port))
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let output = match tokio::time::timeout(timeout, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(_)) | Err(_) => return HostAuth::Unreachable,
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    classify_probe(output.status.success(), &stdout, &stderr)
}

fn classify_probe(success: bool, stdout: &str, stderr: &str) -> HostAuth {
    if success {
        return HostAuth::Shell;
    }
    let message = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    if message.contains("does not provide shell access")
        || message.contains("shell access is disabled")
    {
        HostAuth::NoShell
    } else if matches!(
        super::classifier::classify_exit(stderr, Some(255)),
        super::classifier::ExitReason::AuthFailed
    ) {
        HostAuth::AuthFailed
    } else {
        HostAuth::Unreachable
    }
}

// ── SSH agent keys ──────────────────────────────────────────

/// GET /tunnels/agent-keys — list loaded SSH agent key fingerprints.
pub(crate) async fn list_agent_keys() -> Response {
    let output = match tokio::process::Command::new("ssh-add")
        .arg("-l")
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to run ssh-add: {e}"),
            );
        }
    };

    // Exit code 1 means "no identities" — return empty list, not an error.
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no identities") || output.status.code() == Some(1) {
            return (StatusCode::OK, Json(serde_json::json!([]))).into_response();
        }
        return err_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("ssh-add failed: {}", stderr.trim()),
        );
    }

    // Each line: "256 SHA256:xxxxx user@host (ED25519)"
    let stdout = String::from_utf8_lossy(&output.stdout);
    let keys: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(4, ' ').collect();
            if parts.len() >= 3 {
                serde_json::json!({
                    "bits": parts[0],
                    "fingerprint": parts[1],
                    "comment": parts.get(2).unwrap_or(&""),
                    "type": parts.get(3).map(|s| s.trim_matches(|c| c == '(' || c == ')')),
                })
            } else {
                serde_json::json!({"raw": line})
            }
        })
        .collect();

    (StatusCode::OK, Json(serde_json::json!(keys))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_hosts_are_deduplicated_and_wildcards_are_omitted() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        std::fs::write(
            &config,
            "Host alpha beta\n  HostName example.test\nHost alpha\n  User boss\nHost * !internal\n  ServerAliveInterval 10\n",
        )
        .unwrap();

        assert_eq!(
            parse_ssh_config_hosts(&config).unwrap(),
            vec!["alpha".to_string(), "beta".to_string()]
        );
    }

    #[test]
    fn ssh_hosts_probe_uses_the_noninteractive_bounded_command() {
        assert_eq!(
            probe_args("vps", None),
            vec![
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=5",
                "-o",
                "StrictHostKeyChecking=accept-new",
                "vps",
                "true",
            ]
        );
        assert_eq!(SSH_PROBE_CONCURRENCY, 4);
    }

    #[tokio::test]
    async fn ssh_hosts_probe_classifies_shell_no_shell_auth_and_timeout() {
        let shell = crate::test_support::fake_ssh_script("ssh-hosts-shell", "exit 0", "exit /b 0");
        let no_shell = crate::test_support::fake_ssh_script(
            "ssh-hosts-no-shell",
            "echo 'This service does not provide shell access.' >&2; exit 1",
            "echo This service does not provide shell access. 1>&2& exit /b 1",
        );
        let auth = crate::test_support::fake_ssh_script(
            "ssh-hosts-auth",
            "echo 'Permission denied (publickey).' >&2; exit 255",
            "echo Permission denied (publickey). 1>&2& exit /b 255",
        );
        let timeout = crate::test_support::fake_ssh_script(
            "ssh-hosts-timeout",
            "sleep 2",
            "ping -n 3 127.0.0.1 >nul",
        );

        assert_eq!(
            probe_host_with_binary("host", None, &shell, Duration::from_secs(1)).await,
            HostAuth::Shell
        );
        assert_eq!(
            probe_host_with_binary("host", None, &no_shell, Duration::from_secs(1)).await,
            HostAuth::NoShell
        );
        assert_eq!(
            probe_host_with_binary("host", None, &auth, Duration::from_secs(1)).await,
            HostAuth::AuthFailed
        );
        assert_eq!(
            probe_host_with_binary("host", None, &timeout, Duration::from_millis(50)).await,
            HostAuth::Unreachable
        );
    }

    #[tokio::test]
    async fn ssh_hosts_probe_cache_reuses_results_for_sixty_seconds() {
        let cache = tokio::sync::Mutex::new(None);
        let shell =
            crate::test_support::fake_ssh_script("ssh-hosts-cache-shell", "exit 0", "exit /b 0");
        let auth = crate::test_support::fake_ssh_script(
            "ssh-hosts-cache-auth",
            "echo 'Permission denied' >&2; exit 255",
            "echo Permission denied 1>&2& exit /b 255",
        );
        let hosts = vec![DiscoveredHost {
            host: "cached".to_string(),
            user: None,
            port: None,
            source: discovery::HostSource::Config,
        }];

        let first = probe_cached(&cache, hosts.clone(), &shell, Duration::from_secs(1))
            .await
            .unwrap();
        let second = probe_cached(&cache, hosts, &auth, Duration::from_secs(1))
            .await
            .unwrap();

        assert_eq!(first[0].auth, HostAuth::Shell);
        assert_eq!(second, first, "fresh cache must skip the second process");
    }
}

#[cfg(test)]
mod hostile_probe_tests {
    use super::*;

    /// Catches: a host name starting with `-` (from an ssh config alias) being
    /// passed to `ssh` as an option because no `--` precedes it.
    #[test]
    fn probe_args_cannot_let_a_host_become_an_option() {
        let args = probe_args("-oProxyCommand=evil", None);
        let host_at = args.iter().position(|a| a == "-oProxyCommand=evil").unwrap();
        assert_eq!(args[host_at - 1], "--", "host must follow `--`: {args:?}");
    }
}
