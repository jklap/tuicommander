//! Test Connection — a shared, no-side-effects reachability check for a
//! `RemoteTransport` that may not be saved yet.
//!
//! Story: SSH Tunnels + Remote Servers consolidation, Phase 2. Takes the
//! in-progress form data straight from the (not-yet-saved) editor — including
//! a plaintext `password` for this one call only, never persisted to the
//! keyring or anywhere else, never logged, never echoed back — so this works
//! before Save/before anything reaches the credential vault.
//!
//! - **SSH:** a one-shot, no-forwards SSH connectivity check
//!   (`tunnels::exec::ssh_check`, args from `tunnels::command::build_ssh_test_args`
//!   — the same options a real tunnel uses, off any multiplexed master),
//!   classified through the existing `tunnels::classifier::classify_exit`.
//! - **Direct / Local:** a single `GET <base>/health` via a short-lived
//!   `reqwest::Client` that follows no redirects, with Basic Auth attached when
//!   a username or password was given. Classified by status code and, for a
//!   401, by the exact response body `mcp_http::auth` produces
//!   (`"Invalid credentials"` for a rejected credential vs. the
//!   `NotConfigured`/`MissingHeader` body for "reachable, nothing to
//!   authenticate against yet").
//! - **Local, `instance_id` set:** resolves the port via
//!   `remote_connection::resolve_local_instance_port` (the same on-disk read
//!   Connect will use) before testing — a missing/unreadable instance
//!   directory reports `InstanceNotFound`/`Unreachable`, distinct from a real
//!   daemon that's just down. Test Connection only probes `/health`; Connect
//!   resolves the port the same way (`remote_runtime::resolve_local_base_url`)
//!   and then runs the full token handshake.
//!
//! A password with no username is sent as Basic `:<password>`, exactly like
//! Connect's token exchange: a daemon refuses it (`AuthFailed`) rather than the
//! check quietly testing without credentials.
//!
//! **Known limitation, not fixed here:** a headless `tuic-remote` daemon's
//! `/health` route (`mcp_http::build_remote_router`) is deliberately public —
//! it never runs the auth middleware. A Direct/Local Test Connection against
//! such a daemon therefore classifies a 2xx as `Reachable` regardless of the
//! credentials supplied; `AuthFailed`/`NotConfigured` are only observable where
//! `/health` sits behind authentication.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::remote_connection::{
    LocalInstancePortError, RemoteTransport, resolve_local_instance_port,
};
use crate::tunnels::classifier::ExitReason;

/// Result of testing a (possibly unsaved) connection. One flat,
/// frontend-facing shape across all three transport kinds: the Test
/// Connection UI renders one of a small, fixed set of outcomes no matter which
/// kind the user picked. Never carries a credential.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub(crate) enum ConnectionTestResult {
    /// The target answered successfully (SSH: connected, authenticated, and
    /// ran a trivial remote command; Direct/Local: `/health` returned 2xx).
    Reachable,
    /// Reached the target, but credentials were rejected (SSH: `Permission
    /// denied`; Direct/Local: a 401 whose body is "Invalid credentials").
    AuthFailed,
    /// Direct/Local only: the daemon is reachable, but no credentials were
    /// presented or it has none configured (`AuthResult::NotConfigured` /
    /// `::MissingHeader` share one 401 body). Distinct from `AuthFailed` so the
    /// UI can say "reachable, not configured" instead of "wrong password".
    NotConfigured,
    /// Local only, `instance_id` set: no on-disk instance config directory
    /// exists for that id.
    InstanceNotFound,
    /// Everything else: network failure, timeout, an unreadable Local
    /// instance config, an SSH host-key mismatch, an unexpected status, etc.
    /// Carries a short human-readable reason that never includes the URL (so
    /// URL userinfo cannot leak) or the password.
    Unreachable { reason: String },
}

/// Everything Test Connection needs for one (possibly unsaved) check. A plain
/// request DTO rather than `RemoteConnection`: there is no `id` before Save,
/// and it carries a plaintext `password` that must never round-trip through
/// `RemoteConnection`/`connections.json`. Deserialize-only on purpose — nothing
/// serializes it back out — and its `Debug` redacts the password.
#[derive(Clone, Deserialize)]
pub(crate) struct TestConnectionRequest {
    pub(crate) transport: RemoteTransport,
    #[serde(default)]
    pub(crate) auth_username: Option<String>,
    /// Plaintext, in-memory only for the duration of this one call.
    #[serde(default)]
    pub(crate) password: Option<String>,
}

impl std::fmt::Debug for TestConnectionRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestConnectionRequest")
            .field("transport", &self.transport)
            .field("auth_username", &self.auth_username)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// Shared body of the Tauri `test_connection` command and the HTTP
/// `POST /config/remote-connections/test` route.
pub(crate) async fn test_connection_impl(request: &TestConnectionRequest) -> ConnectionTestResult {
    let username = request.auth_username.as_deref();
    let password = request.password.as_deref();
    match &request.transport {
        RemoteTransport::Ssh { ssh, .. } => {
            classify_ssh_result(crate::tunnels::exec::ssh_check(ssh, SSH_TEST_TIMEOUT).await)
        }
        // A pinned self-signed certificate is honoured by Connect (the pinned
        // relay in `direct_proxy`), not here: such a target reports
        // Unreachable with the TLS error, exactly as before pinning existed.
        RemoteTransport::Direct { url, .. } => test_http_health(url, username, password).await,
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
/// (in `build_ssh_test_args`) already bounds the network phase, so this is only
/// a safety net against ssh itself hanging (e.g. a misbehaving `ProxyCommand`)
/// — strictly larger than the ConnectTimeout it wraps.
const SSH_TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Map the runner's result onto `ConnectionTestResult`. `AuthFailed` has its
/// own variant; every other `ExitReason` becomes `Unreachable` carrying the
/// reason.
fn classify_ssh_result(result: Result<(), ExitReason>) -> ConnectionTestResult {
    match result {
        Ok(()) => ConnectionTestResult::Reachable,
        Err(ExitReason::AuthFailed) => ConnectionTestResult::AuthFailed,
        Err(ExitReason::Timeout) => ConnectionTestResult::Unreachable {
            reason: "timed out waiting for ssh".to_string(),
        },
        Err(other) => ConnectionTestResult::Unreachable {
            reason: format!("{other:?}"),
        },
    }
}

// ---------------------------------------------------------------------------
// Direct / Local (HTTP health check)
// ---------------------------------------------------------------------------

/// Response body `mcp_http::auth::basic_auth_middleware` sends for a real
/// credential mismatch (`AuthResult::Invalid`) — kept as the single source of
/// truth here so a wording change there fails this module's test instead of
/// silently breaking classification.
const AUTH_INVALID_BODY: &str = "Invalid credentials";

const HTTP_TEST_TIMEOUT: Duration = Duration::from_secs(5);

async fn test_http_health(
    base_url: &str,
    auth_username: Option<&str>,
    password: Option<&str>,
) -> ConnectionTestResult {
    // No redirects: a health check has no reason to follow one, and a
    // followed redirect is how a Basic Auth header would reach a host the
    // user never typed.
    let client = match reqwest::Client::builder()
        .timeout(HTTP_TEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return ConnectionTestResult::Unreachable {
                reason: format!("failed to build HTTP client: {}", e.without_url()),
            };
        }
    };

    let url = format!("{}/health", base_url.trim_end_matches('/'));
    let mut req = client.get(&url);
    let username = auth_username.unwrap_or_default();
    let password = password.filter(|p| !p.is_empty());
    if !username.is_empty() || password.is_some() {
        req = req.basic_auth(username, password);
    }

    match req.send().await {
        Ok(resp) => classify_http_response(resp).await,
        Err(e) => ConnectionTestResult::Unreachable {
            reason: describe_reqwest_error(e),
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
            // `AuthResult::NotConfigured` or `::MissingHeader` — same body,
            // same meaning from here: nothing to authenticate against yet.
            ConnectionTestResult::NotConfigured
        };
    }
    ConnectionTestResult::Unreachable {
        reason: format!("unexpected HTTP status {status}"),
    }
}

/// Human-readable classification of a `reqwest::Error` for the UI. The URL is
/// stripped first (`without_url`): a Direct URL can carry `user:pass@`
/// userinfo, and an error message is not a place for it.
fn describe_reqwest_error(e: reqwest::Error) -> String {
    let timeout = e.is_timeout();
    let connect = e.is_connect();
    let e = e.without_url();
    if timeout {
        "timed out waiting for a response".to_string()
    } else if connect {
        format!("connection failed: {e}")
    } else if e.to_string().to_lowercase().contains("tls") {
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
    use crate::test_support::{fail_with_stderr_script, fake_ssh_script};

    // --- SSH classification ---

    #[test]
    fn classify_ssh_result_maps_auth_and_everything_else() {
        assert_eq!(classify_ssh_result(Ok(())), ConnectionTestResult::Reachable);
        assert_eq!(
            classify_ssh_result(Err(ExitReason::AuthFailed)),
            ConnectionTestResult::AuthFailed
        );
        for (reason, expected) in [
            (ExitReason::ConnectionRefused, "ConnectionRefused"),
            (ExitReason::HostKeyMismatch, "HostKeyMismatch"),
            (ExitReason::NetworkDown, "NetworkDown"),
        ] {
            match classify_ssh_result(Err(reason)) {
                ConnectionTestResult::Unreachable { reason } => {
                    assert!(reason.contains(expected), "{reason}");
                }
                other => panic!("expected Unreachable, got {other:?}"),
            }
        }
        assert_eq!(
            classify_ssh_result(Err(ExitReason::Timeout)),
            ConnectionTestResult::Unreachable {
                reason: "timed out waiting for ssh".to_string()
            }
        );
    }

    /// End to end through the real runner with a fake `ssh` that fails the
    /// way OpenSSH does on a rejected key.
    #[tokio::test]
    async fn an_ssh_permission_denied_is_auth_failed() {
        let ssh_binary = fake_ssh_script(
            "connection_test_permission_denied",
            &fail_with_stderr_script("user@host: Permission denied (publickey).", 255),
            &fail_with_stderr_script("user@host: Permission denied (publickey).", 255),
        );
        let result = crate::tunnels::exec::ssh_check_with_binary(
            &SshConnectionParams::new("example.com", "alice"),
            Duration::from_secs(30),
            &ssh_binary,
        )
        .await;
        assert_eq!(
            classify_ssh_result(result),
            ConnectionTestResult::AuthFailed
        );
    }

    // --- classify_http_response ---

    async fn health_response(status: usize, body: &str) -> reqwest::Response {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/health")
            .with_status(status)
            .with_body(body)
            .create_async()
            .await;
        reqwest::get(format!("{}/health", server.url()))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn classify_http_response_by_status_and_body() {
        assert_eq!(
            classify_http_response(health_response(200, r#"{"ok":true}"#).await).await,
            ConnectionTestResult::Reachable
        );
        assert_eq!(
            classify_http_response(health_response(401, "Invalid credentials").await).await,
            ConnectionTestResult::AuthFailed
        );
        assert_eq!(
            classify_http_response(
                health_response(401, "Scan the QR code or authenticate with Basic Auth").await
            )
            .await,
            ConnectionTestResult::NotConfigured
        );
        match classify_http_response(health_response(500, "").await).await {
            ConnectionTestResult::Unreachable { reason } => {
                assert!(reason.contains("500"), "{reason}")
            }
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    /// The 401 bodies classified above are the ones the real middleware sends.
    #[test]
    fn the_auth_bodies_match_what_the_middleware_sends() {
        let source = include_str!("mcp_http/auth.rs");
        assert!(source.contains(&format!("unauthorized_response(\"{AUTH_INVALID_BODY}\")")));
        assert!(source.contains(
            "unauthorized_response(\"Scan the QR code or authenticate with Basic Auth\")"
        ));
    }

    // --- test_http_health (end-to-end through the real reqwest client) ---

    #[tokio::test]
    async fn test_http_health_reachable_without_credentials() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/health")
            .match_header("authorization", mockito::Matcher::Missing)
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

    /// A password with no username is sent as `:<password>`, like Connect's
    /// token exchange — never silently dropped, so the check sees what Connect
    /// would see: a refusal.
    #[tokio::test]
    async fn a_password_without_a_username_is_sent_and_refused_not_dropped() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/health")
            .match_header("authorization", "Basic Omh1bnRlcjI=") // :hunter2
            .with_status(401)
            .with_body("Invalid credentials")
            .create_async()
            .await;

        let result = test_http_health(&server.url(), Some(""), Some("hunter2")).await;
        assert_eq!(result, ConnectionTestResult::AuthFailed);
    }

    /// A redirect is not followed, so the Basic Auth header never reaches the
    /// redirect target.
    #[tokio::test]
    async fn a_redirect_is_not_followed_with_the_credentials() {
        let mut target = mockito::Server::new_async().await;
        let never = target.mock("GET", "/health").expect(0).create_async().await;
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/health")
            .with_status(302)
            .with_header("location", &format!("{}/health", target.url()))
            .create_async()
            .await;

        let result = test_http_health(&server.url(), Some("alice"), Some("hunter2")).await;
        assert!(
            matches!(result, ConnectionTestResult::Unreachable { ref reason } if reason.contains("302")),
            "{result:?}"
        );
        never.assert_async().await;
    }

    #[tokio::test]
    async fn an_unreachable_error_names_neither_the_url_credentials_nor_the_password() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        drop(listener);
        let result = test_http_health(
            &format!("http://user:URL_SECRET_1457@{address}"),
            Some("alice"),
            Some("PASSWORD_SECRET_1457"),
        )
        .await;
        let ConnectionTestResult::Unreachable { reason } = result else {
            panic!("expected Unreachable, got {result:?}");
        };
        assert!(!reason.is_empty());
        assert!(!reason.contains("URL_SECRET_1457"), "{reason}");
        assert!(!reason.contains("PASSWORD_SECRET_1457"), "{reason}");
    }

    #[test]
    fn the_request_debug_output_redacts_the_password() {
        let request = TestConnectionRequest {
            transport: RemoteTransport::Direct {
                url: "http://h".to_string(),
                tls_fingerprint: None,
            },
            auth_username: Some("alice".to_string()),
            password: Some("hunter2".to_string()),
        };
        let debug = format!("{request:?}");
        assert!(!debug.contains("hunter2"), "{debug}");
        assert!(debug.contains("<redacted>"), "{debug}");
    }

    // --- test_connection_impl: Local dispatch ---

    fn local(port: Option<u16>, instance_id: Option<String>) -> TestConnectionRequest {
        TestConnectionRequest {
            transport: RemoteTransport::Local { port, instance_id },
            auth_username: None,
            password: None,
        }
    }

    #[tokio::test]
    async fn local_with_unknown_instance_id_is_instance_not_found() {
        let request = local(
            None,
            Some(format!("no-such-instance-{}", uuid::Uuid::new_v4())),
        );
        assert_eq!(
            test_connection_impl(&request).await,
            ConnectionTestResult::InstanceNotFound
        );
    }

    #[tokio::test]
    async fn local_with_neither_port_nor_instance_id_is_unreachable() {
        match test_connection_impl(&local(None, None)).await {
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
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        drop(listener);
        assert!(matches!(
            test_connection_impl(&local(Some(port), None)).await,
            ConnectionTestResult::Unreachable { .. }
        ));
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
            transport: RemoteTransport::Direct {
                url: server.url(),
                tls_fingerprint: None,
            },
            auth_username: None,
            password: None,
        };
        assert_eq!(
            test_connection_impl(&request).await,
            ConnectionTestResult::Reachable
        );
    }
}
