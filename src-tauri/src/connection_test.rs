//! Test Connection — a shared, no-side-effects reachability check for a
//! `RemoteTransport` that may not be saved yet.
//!
//! Story: SSH Tunnels + Remote Servers consolidation, Phase 2. Takes the
//! in-progress form data straight from the (not-yet-saved) editor — including
//! a plaintext `password` for this one call only, never persisted to the
//! keyring or anywhere else, just used in-memory here and dropped — so this
//! works before Save/before anything reaches the credential vault.
//!
//! - **SSH:** a one-shot, no-forwards SSH connectivity check
//!   (`tunnels::command::build_ssh_test_args`), classified through the
//!   existing `tunnels::classifier::classify_exit`.
//! - **Direct / Local:** a single `GET <base>/health` via a short-lived
//!   `reqwest::Client`, with Basic Auth attached if a username/password were
//!   given. Classified by status code and, for a 401, by the exact response
//!   body text `mcp_http::auth::validate_basic_auth` already produces
//!   (`"Invalid credentials"` for a real auth failure vs. the
//!   `NotConfigured`/`MissingHeader` body for "reachable but has no
//!   credentials set yet").
//! - **Local, `instance_id` set:** resolves the port via
//!   `remote_connection::resolve_local_instance_port` (the same on-disk read
//!   Connect will use) before testing — a missing/unreadable instance
//!   directory reports `InstanceNotFound`/`Unreachable`, distinct from a real
//!   daemon that's just down.
//!
//! **Known limitation, not fixed here (out of this phase's scope):** a
//! headless `tuic-remote` daemon's `/health` route
//! (`mcp_http::build_remote_router`) is deliberately public/unauthenticated —
//! it lives in a `public_routes` sub-router merged *before* the auth
//! middleware layer, so it never runs `basic_auth_middleware` at all. A
//! Direct/Local Test Connection against such a daemon therefore always
//! classifies as `Reachable` on a 2xx, regardless of whether credentials
//! were supplied or correct — `AuthFailed`/`NotConfigured` can only ever be
//! observed against a full desktop instance's `build_router` (where
//! `/health` DOES sit behind the auth layer). This is a pre-existing
//! asymmetry in how `/health` is wired for the two router flavors, not
//! something Test Connection itself can paper over — flagging it here rather
//! than silently masking it.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::remote_connection::{
    LocalInstancePortError, RemoteTransport, resolve_local_instance_port,
};
use crate::tunnels::classifier::{ExitReason, classify_exit};
use crate::tunnels::command::{build_ssh_env, build_ssh_test_args};

/// Result of testing a (possibly unsaved) connection. Kept as one flat,
/// frontend-facing shape across all three transport kinds rather than a
/// per-transport result type, per the plan's explicit ask: the Test
/// Connection UI (Phase 3) needs to render one of a small, fixed set of
/// outcomes no matter which Kind the user picked.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub(crate) enum ConnectionTestResult {
    /// The target answered successfully (SSH: connected, authenticated, and
    /// ran a trivial remote command; Direct/Local: `/health` returned 2xx).
    Reachable,
    /// Reached the target, but credentials were rejected (SSH: `Permission
    /// denied`; Direct/Local: a 401 whose body is
    /// `mcp_http::auth::AuthResult::Invalid`'s exact text, "Invalid
    /// credentials").
    AuthFailed,
    /// Direct/Local only: the daemon is reachable, but has no username/
    /// password configured at all yet (`AuthResult::NotConfigured` or
    /// `::MissingHeader` — both share the same 401 body text and, from the
    /// client's point of view, mean the same thing: "nothing to authenticate
    /// against yet"). Distinct from `AuthFailed` so the UI can say "reachable,
    /// but not configured" instead of "wrong password".
    NotConfigured,
    /// Local only, `instance_id` set: no on-disk instance config directory
    /// exists for that id (typo, or an instance that was never started) —
    /// distinct from `Unreachable`, which means "the id resolved fine but the
    /// daemon didn't answer."
    InstanceNotFound,
    /// Everything else: network failure, timeout, an unreadable Local
    /// instance config, an SSH host-key mismatch, a port already in use,
    /// etc. Carries a short human-readable reason for the UI to display.
    Unreachable { reason: String },
}

/// Everything Test Connection needs for one (possibly unsaved) check. A
/// plain request DTO rather than reusing `RemoteConnection` directly: this
/// call happens before Save, so there is no `id` yet, and it carries a
/// plaintext `password` that must never round-trip through
/// `RemoteConnection`/`connections.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TestConnectionRequest {
    pub(crate) transport: RemoteTransport,
    #[serde(default)]
    pub(crate) auth_username: Option<String>,
    /// Plaintext, in-memory only for the duration of this one call — never
    /// persisted to the keyring or `connections.json`.
    #[serde(default)]
    pub(crate) password: Option<String>,
}

/// Shared body of the Tauri `test_connection` command and the HTTP
/// `POST /config/remote-connections/test` route.
pub(crate) async fn test_connection_impl(request: &TestConnectionRequest) -> ConnectionTestResult {
    let username = request.auth_username.as_deref();
    let password = request.password.as_deref();
    match &request.transport {
        RemoteTransport::Ssh { ssh, .. } => test_ssh_connection(Path::new("ssh"), ssh).await,
        RemoteTransport::Direct { url } => test_http_health(url, username, password).await,
        RemoteTransport::Local { port, instance_id } => {
            let resolved_port = match (port, instance_id) {
                (_, Some(id)) if !id.trim().is_empty() => match resolve_local_instance_port(id) {
                    Ok(p) => p,
                    Err(LocalInstancePortError::InstanceNotFound) => {
                        return ConnectionTestResult::InstanceNotFound;
                    }
                    Err(LocalInstancePortError::Unreadable(msg)) => {
                        return ConnectionTestResult::Unreachable { reason: msg };
                    }
                },
                (Some(p), _) => *p,
                (None, _) => {
                    return ConnectionTestResult::Unreachable {
                        reason: "Local connection has neither a port nor an instance_id"
                            .to_string(),
                    };
                }
            };
            let url = format!("http://127.0.0.1:{resolved_port}");
            test_http_health(&url, username, password).await
        }
    }
}

// ---------------------------------------------------------------------------
// SSH
// ---------------------------------------------------------------------------

/// Overall wall-clock bound for the one-shot SSH check: `ConnectTimeout=5`
/// (baked into `build_ssh_test_args`) already bounds the network phase, so
/// this is only a safety net against ssh itself hanging (e.g. a
/// misbehaving `ProxyCommand`) — strictly larger than the ConnectTimeout it
/// wraps, per this crate's own "which timing assertions are load-bearing"
/// guidance.
const SSH_TEST_TIMEOUT: Duration = Duration::from_secs(10);

async fn test_ssh_connection(
    ssh_binary: &Path,
    ssh: &crate::ssh_connection::SshConnectionParams,
) -> ConnectionTestResult {
    let args = build_ssh_test_args(ssh);
    let agent_socket = crate::tunnels::agent::discover_agent_socket();
    let env = build_ssh_env(agent_socket.as_deref());

    let mut cmd = tokio::process::Command::new(ssh_binary);
    cmd.args(&args[1..])
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return ConnectionTestResult::Unreachable {
                reason: format!("failed to spawn ssh: {e}"),
            };
        }
    };

    match tokio::time::timeout(SSH_TEST_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(output)) => {
            if output.status.success() {
                ConnectionTestResult::Reachable
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                classify_ssh_failure(&stderr, output.status.code())
            }
        }
        Ok(Err(e)) => ConnectionTestResult::Unreachable {
            reason: format!("ssh process error: {e}"),
        },
        Err(_) => ConnectionTestResult::Unreachable {
            reason: "timed out waiting for ssh".to_string(),
        },
    }
}

/// Map `classify_exit`'s `ExitReason` onto `ConnectionTestResult`.
/// `AuthFailed` maps to its own dedicated variant; everything else
/// (`HostKeyMismatch`, `PortInUse` — never actually reachable here since a
/// Test Connection has no forwards to bind, but classified consistently
/// anyway — `ConnectionRefused`, `NetworkDown`, `Timeout`, `UserKilled`,
/// `Unknown`) becomes `Unreachable` carrying the reason as a string, per the
/// plan's own guidance: keep the frontend-facing shape simple.
fn classify_ssh_failure(stderr: &str, code: Option<i32>) -> ConnectionTestResult {
    match classify_exit(stderr, code) {
        ExitReason::AuthFailed => ConnectionTestResult::AuthFailed,
        other => ConnectionTestResult::Unreachable {
            reason: format!("{other:?}"),
        },
    }
}

// ---------------------------------------------------------------------------
// Direct / Local (HTTP health check)
// ---------------------------------------------------------------------------

/// Response body text `mcp_http::auth::basic_auth_middleware` sends for a
/// real credential mismatch (`AuthResult::Invalid`) — kept as the single
/// source of truth here so a future wording change to that middleware fails
/// this module's own test instead of silently breaking classification.
const AUTH_INVALID_BODY: &str = "Invalid credentials";

const HTTP_TEST_TIMEOUT: Duration = Duration::from_secs(5);

async fn test_http_health(
    base_url: &str,
    auth_username: Option<&str>,
    password: Option<&str>,
) -> ConnectionTestResult {
    let client = match reqwest::Client::builder()
        .timeout(HTTP_TEST_TIMEOUT)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return ConnectionTestResult::Unreachable {
                reason: format!("failed to build HTTP client: {e}"),
            };
        }
    };

    let url = format!("{}/health", base_url.trim_end_matches('/'));
    let mut req = client.get(&url);
    if let Some(user) = auth_username.filter(|u| !u.is_empty()) {
        req = req.basic_auth(user, password);
    }

    match req.send().await {
        Ok(resp) => classify_http_response(resp).await,
        Err(e) => ConnectionTestResult::Unreachable {
            reason: describe_reqwest_error(&e),
        },
    }
}

async fn classify_http_response(resp: reqwest::Response) -> ConnectionTestResult {
    let status = resp.status();
    if status.is_success() {
        return ConnectionTestResult::Reachable;
    }
    if status == reqwest::StatusCode::UNAUTHORIZED {
        let body = resp.text().await.unwrap_or_default();
        return if body.contains(AUTH_INVALID_BODY) {
            ConnectionTestResult::AuthFailed
        } else {
            // Either `AuthResult::NotConfigured` or `::MissingHeader` — both
            // share the same body text and both mean "reachable, nothing to
            // authenticate against yet" from the client's point of view.
            ConnectionTestResult::NotConfigured
        };
    }
    ConnectionTestResult::Unreachable {
        reason: format!("unexpected HTTP status {status}"),
    }
}

/// Human-readable classification of a `reqwest::Error` for the UI — mainly
/// distinguishes a TLS failure (a self-signed cert with no pinning support
/// yet — Phase 4 adds pinning; today this always surfaces here as
/// `Unreachable`, never a silent success) from a plain connection failure or
/// a timeout.
fn describe_reqwest_error(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "timed out waiting for a response".to_string()
    } else if e.is_connect() {
        format!("connection failed: {e}")
    } else if e.is_request() && e.to_string().to_lowercase().contains("tls") {
        format!("TLS error: {e}")
    } else {
        e.to_string()
    }
}

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn test_connection(request: TestConnectionRequest) -> ConnectionTestResult {
    test_connection_impl(&request).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh_connection::SshConnectionParams;

    // --- classify_ssh_failure ---

    #[test]
    fn classify_ssh_failure_auth_denied() {
        let stderr = "user@host: Permission denied (publickey).";
        assert_eq!(
            classify_ssh_failure(stderr, Some(255)),
            ConnectionTestResult::AuthFailed
        );
    }

    #[test]
    fn classify_ssh_failure_connection_refused_is_unreachable() {
        let stderr = "ssh: connect to host example.com port 22: Connection refused";
        match classify_ssh_failure(stderr, Some(255)) {
            ConnectionTestResult::Unreachable { reason } => {
                assert!(reason.contains("ConnectionRefused"), "{reason}");
            }
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    #[test]
    fn classify_ssh_failure_host_key_mismatch_is_unreachable() {
        let stderr = "Host key verification failed.";
        match classify_ssh_failure(stderr, Some(255)) {
            ConnectionTestResult::Unreachable { reason } => {
                assert!(reason.contains("HostKeyMismatch"), "{reason}");
            }
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    #[test]
    fn classify_ssh_failure_network_down_is_unreachable() {
        let stderr = "ssh: connect to host example.com port 22: Network is unreachable";
        match classify_ssh_failure(stderr, Some(255)) {
            ConnectionTestResult::Unreachable { reason } => {
                assert!(reason.contains("NetworkDown"), "{reason}");
            }
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    // --- classify_http_response ---

    #[tokio::test]
    async fn classify_http_response_2xx_is_reachable() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/health")
            .with_status(200)
            .with_body(r#"{"ok":true}"#)
            .create_async()
            .await;

        let resp = reqwest::get(format!("{}/health", server.url()))
            .await
            .unwrap();
        assert_eq!(
            classify_http_response(resp).await,
            ConnectionTestResult::Reachable
        );
    }

    #[tokio::test]
    async fn classify_http_response_401_invalid_credentials_is_auth_failed() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/health")
            .with_status(401)
            .with_body("Invalid credentials")
            .create_async()
            .await;

        let resp = reqwest::get(format!("{}/health", server.url()))
            .await
            .unwrap();
        assert_eq!(
            classify_http_response(resp).await,
            ConnectionTestResult::AuthFailed
        );
    }

    #[tokio::test]
    async fn classify_http_response_401_not_configured_body_is_not_configured() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/health")
            .with_status(401)
            .with_body("Scan the QR code or authenticate with Basic Auth")
            .create_async()
            .await;

        let resp = reqwest::get(format!("{}/health", server.url()))
            .await
            .unwrap();
        assert_eq!(
            classify_http_response(resp).await,
            ConnectionTestResult::NotConfigured
        );
    }

    #[tokio::test]
    async fn classify_http_response_500_is_unreachable() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/health")
            .with_status(500)
            .create_async()
            .await;

        let resp = reqwest::get(format!("{}/health", server.url()))
            .await
            .unwrap();
        match classify_http_response(resp).await {
            ConnectionTestResult::Unreachable { reason } => {
                assert!(reason.contains("500"), "{reason}");
            }
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    // --- test_http_health (end-to-end through the real reqwest client) ---

    #[tokio::test]
    async fn test_http_health_reachable_without_credentials() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/health")
            .with_status(200)
            .create_async()
            .await;

        let result = test_http_health(&server.url(), None, None).await;
        assert_eq!(result, ConnectionTestResult::Reachable);
    }

    #[tokio::test]
    async fn test_http_health_sends_basic_auth_header_when_credentials_given() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/health")
            .match_header("authorization", "Basic YWxpY2U6aHVudGVyMg==") // alice:hunter2
            .with_status(200)
            .create_async()
            .await;

        let result = test_http_health(&server.url(), Some("alice"), Some("hunter2")).await;
        assert_eq!(result, ConnectionTestResult::Reachable);
    }

    #[tokio::test]
    async fn test_http_health_omits_auth_header_when_username_is_empty() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/health")
            .match_header("authorization", mockito::Matcher::Missing)
            .with_status(200)
            .create_async()
            .await;

        let result = test_http_health(&server.url(), Some(""), Some("hunter2")).await;
        assert_eq!(result, ConnectionTestResult::Reachable);
    }

    #[tokio::test]
    async fn test_http_health_unreachable_address_reports_a_reason() {
        // Port 1 is reserved and nothing listens there — a fast, reliable
        // connection failure without depending on a "does not resolve" DNS
        // error being consistent across CI/sandbox network configs.
        let result = test_http_health("http://127.0.0.1:1", None, None).await;
        match result {
            ConnectionTestResult::Unreachable { reason } => assert!(!reason.is_empty()),
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    // --- test_connection_impl: Local dispatch ---

    #[tokio::test]
    async fn local_with_unknown_instance_id_is_instance_not_found() {
        let request = TestConnectionRequest {
            transport: RemoteTransport::Local {
                port: None,
                instance_id: Some(format!("no-such-instance-{}", uuid::Uuid::new_v4())),
            },
            auth_username: None,
            password: None,
        };
        assert_eq!(
            test_connection_impl(&request).await,
            ConnectionTestResult::InstanceNotFound
        );
    }

    #[tokio::test]
    async fn local_with_neither_port_nor_instance_id_is_unreachable() {
        let request = TestConnectionRequest {
            transport: RemoteTransport::Local {
                port: None,
                instance_id: None,
            },
            auth_username: None,
            password: None,
        };
        match test_connection_impl(&request).await {
            ConnectionTestResult::Unreachable { reason } => {
                assert!(
                    reason.contains("neither a port nor an instance_id"),
                    "{reason}"
                );
            }
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn local_with_a_raw_port_and_nothing_listening_is_unreachable() {
        let request = TestConnectionRequest {
            transport: RemoteTransport::Local {
                port: Some(1), // reserved, nothing listens here
                instance_id: None,
            },
            auth_username: None,
            password: None,
        };
        match test_connection_impl(&request).await {
            ConnectionTestResult::Unreachable { .. } => {}
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    // --- test_connection_impl: Direct dispatch ---

    #[tokio::test]
    async fn direct_dispatches_to_the_http_health_check() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/health")
            .with_status(200)
            .create_async()
            .await;

        let request = TestConnectionRequest {
            transport: RemoteTransport::Direct { url: server.url() },
            auth_username: None,
            password: None,
        };
        assert_eq!(
            test_connection_impl(&request).await,
            ConnectionTestResult::Reachable
        );
    }

    // --- test_connection_impl: SSH dispatch (argument construction only —
    // spawning a real `ssh` against a fake host is covered by
    // `tunnels::supervisor`'s existing fake-ssh-script pattern; this module
    // sticks to what's unit-testable without a real network round trip, per
    // this phase's own instructions) ---

    #[test]
    fn ssh_test_args_are_what_test_ssh_connection_would_actually_spawn() {
        // Not a spawn test — just proves the exact argv `test_ssh_connection`
        // hands to `Command` is `build_ssh_test_args`'s output with argv[0]
        // stripped (mirrors `tunnels::supervisor::supervision_loop`'s own
        // `args[1..]` convention), so a future refactor can't silently drop
        // this sharing.
        let ssh = SshConnectionParams::new("example.com", "alice");
        let args = build_ssh_test_args(&ssh);
        assert_eq!(args[0], "ssh");
        assert!(args.len() > 1, "must have more than just argv[0]");
    }
}
