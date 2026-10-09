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
//! - **Direct, `https://`:** the certificate is judged first, the way Connect
//!   judges it (`direct_proxy::probe_direct_tls`): a pinned certificate that
//!   still matches is tested through a one-shot pinned relay; an unpinned
//!   certificate nobody vouches for, or a pin the server no longer matches,
//!   reports `Unreachable` before any request (or credential) is sent.
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
        RemoteTransport::Direct {
            url,
            tls_fingerprint,
        } => test_direct(url, tls_fingerprint.as_deref(), username, password).await,
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
            // The credential below is sent with the health request itself, so
            // the instance's identity is proven first (same check as Connect).
            if let Err(reason) = verify_local_target(&url, instance_id.as_deref()).await {
                return ConnectionTestResult::Unreachable { reason };
            }
            test_http_health(&url, username, password).await
        }
    }
}

/// Unauthenticated `/health` + `remote_runtime::verify_local_instance_identity`
/// for a Local target, before any credential goes to it.
async fn verify_local_target(url: &str, instance_id: Option<&str>) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(HTTP_TEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("failed to build HTTP client: {}", e.without_url()))?;
    let health = crate::remote_runtime::read_health(&client, url).await?;
    crate::remote_runtime::verify_local_instance_identity(
        &health,
        instance_id.map(str::trim).filter(|id| !id.is_empty()),
    )
    .await
}

// ---------------------------------------------------------------------------
// Direct (certificate trust first, then the HTTP health check)
// ---------------------------------------------------------------------------

/// Direct: decide whether the URL's certificate can be trusted exactly the way
/// Connect does (`remote_runtime::resolve_direct_base_url`), and only then send
/// the health request — and the credentials with it.
///
/// * `http://`, or `https://` the OS trusts: the URL itself.
/// * `https://` pinned and still presenting the pin: through a one-shot pinned
///   relay (`direct_proxy::DirectProxies`), the same verifier and the same
///   loopback base URL Connect hands out — so a pinned self-signed daemon tests
///   as reachable instead of failing on the system trust store.
/// * Fails closed, before anything is sent: an `https://` certificate nobody
///   vouches for and nothing pins, a pin the server no longer matches, or a
///   garbled pin.
async fn test_direct(
    url: &str,
    tls_fingerprint: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
) -> ConnectionTestResult {
    use crate::direct_proxy::{ProbeResult, https_target, normalize_fingerprint, probe_direct_tls};
    let url = url.trim().trim_end_matches('/');
    let target = match https_target(url) {
        Ok(Some(target)) => target,
        Ok(None) => return test_http_health(url, username, password).await,
        Err(reason) => return ConnectionTestResult::Unreachable { reason },
    };
    let probe = match probe_direct_tls(url, tls_fingerprint).await {
        Ok(probe) => probe,
        Err(reason) => {
            let reason = reason
                .strip_prefix("Unreachable: ")
                .map(str::to_string)
                .unwrap_or(reason);
            return ConnectionTestResult::Unreachable { reason };
        }
    };
    match probe {
        ProbeResult::NoTlsNeeded | ProbeResult::Trusted => {
            test_http_health(url, username, password).await
        }
        ProbeResult::NeedsConfirmation { fingerprint } => ConnectionTestResult::Unreachable {
            reason: format!(
                "certificate not trusted by this system (SHA-256 {fingerprint}); \
                 Connect lets you compare and pin it. Nothing was sent to the server."
            ),
        },
        ProbeResult::PinnedMismatch {
            presented_fingerprint,
        } => ConnectionTestResult::Unreachable {
            reason: format!(
                "certificate changed: pinned SHA-256 {}, the server now presents \
                 {presented_fingerprint}. Nothing was sent to the server.",
                tls_fingerprint.unwrap_or_default()
            ),
        },
        ProbeResult::PinnedMatch => {
            let Some(pin) = tls_fingerprint.and_then(normalize_fingerprint) else {
                return ConnectionTestResult::Unreachable {
                    reason: "stored certificate pin is not a SHA-256 fingerprint".to_string(),
                };
            };
            let relay = OneShotRelay::default();
            let port = match relay.0.start(OneShotRelay::ID, target, &pin).await {
                Ok(port) => port,
                Err(reason) => return ConnectionTestResult::Unreachable { reason },
            };
            // The bare relay origin, exactly what `resolve_direct_base_url`
            // hands Connect, so the test sees what Connect will see.
            test_http_health(&format!("http://127.0.0.1:{port}"), username, password).await
        }
    }
}

/// A pinned relay that lives exactly as long as one Test Connection call —
/// stopped on drop, so a cancelled request cannot leave its accept loop behind.
#[derive(Default)]
struct OneShotRelay(crate::direct_proxy::DirectProxies);

impl OneShotRelay {
    const ID: &'static str = "test-connection";
}

impl Drop for OneShotRelay {
    fn drop(&mut self) {
        self.0.stop(Self::ID);
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
///
/// The TLS check runs BEFORE `is_connect()`: reqwest reports a failed TLS
/// handshake as a connect error too, so checked after it the TLS branch could
/// never fire and every certificate/handshake failure read as a generic
/// "connection failed" (code review 2026-09-23).
fn describe_reqwest_error(e: reqwest::Error) -> String {
    let timeout = e.is_timeout();
    let connect = e.is_connect();
    let e = e.without_url();
    if timeout {
        "timed out waiting for a response".to_string()
    } else if mentions_tls(&e) {
        format!("TLS error: {e}")
    } else if connect {
        format!("connection failed: {e}")
    } else {
        e.to_string()
    }
}

/// Whether `e` failed in TLS: any error in its `source()` chain is a
/// `rustls::Error` (directly, or inside `io::Error`s — the TLS connector
/// nests one in another) or names TLS. reqwest's own `Display` stops at
/// "error sending request"; the handshake failure that explains it sits
/// further down, and rustls' wording ("received corrupt message of type
/// InvalidContentType") does not say "TLS" at all.
fn mentions_tls(e: &reqwest::Error) -> bool {
    fn is_rustls(err: &(dyn std::error::Error + 'static)) -> bool {
        if err.is::<rustls::Error>() {
            return true;
        }
        match err
            .downcast_ref::<std::io::Error>()
            .and_then(|io| io.get_ref())
        {
            Some(inner) => is_rustls(inner),
            None => false,
        }
    }
    let mut current: Option<&(dyn std::error::Error + 'static)> = Some(e);
    while let Some(err) = current {
        if is_rustls(err) || err.to_string().to_lowercase().contains("tls") {
            return true;
        }
        current = err.source();
    }
    false
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

    /// End to end through the real runner with a fake `ssh` that exits 0 —
    /// the spawn/timeout wiring itself, not just the classifier.
    #[tokio::test]
    async fn an_ssh_that_exits_zero_is_reachable() {
        let ssh_binary = fake_ssh_script("connection_test_exit_zero", "exit 0", "exit /b 0");
        let result = crate::tunnels::exec::ssh_check_with_binary(
            &SshConnectionParams::new("example.com", "alice"),
            Duration::from_secs(30),
            &ssh_binary,
        )
        .await;
        assert_eq!(classify_ssh_result(result), ConnectionTestResult::Reachable);
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

    /// A server that answers a TLS ClientHello with plain HTTP is a handshake
    /// failure, which reqwest also flags `is_connect()` — it must still read as
    /// a TLS error, while a refused port keeps reading "connection failed".
    #[tokio::test]
    async fn a_tls_handshake_failure_is_reported_as_tls_not_as_a_connect_failure() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let _ = stream
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n")
                    .await;
            }
        });
        let result = test_http_health(&format!("https://{address}"), None, None).await;
        let ConnectionTestResult::Unreachable { reason } = result else {
            panic!("expected Unreachable, got {result:?}");
        };
        assert!(reason.starts_with("TLS error"), "{reason}");

        let refused = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let refused_address = refused.local_addr().expect("address");
        drop(refused);
        let result = test_http_health(&format!("http://{refused_address}"), None, None).await;
        let ConnectionTestResult::Unreachable { reason } = result else {
            panic!("expected Unreachable, got {result:?}");
        };
        assert!(reason.starts_with("connection failed"), "{reason}");
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

    /// Test Connection sends the password WITH its health request, so a
    /// Local port held by something that can't prove it is a TUICommander
    /// instance of this user is reported Unreachable and gets no credential.
    #[tokio::test]
    async fn local_port_held_by_an_unverified_listener_gets_no_credential() {
        let mut server = mockito::Server::new_async().await;
        let _plain = server
            .mock("GET", "/health")
            .match_header("authorization", mockito::Matcher::Missing)
            .with_body(r#"{"instance_id":"squatter","socket_path":"/tmp/nope.sock"}"#)
            .create_async()
            .await;
        let with_credential = server
            .mock("GET", "/health")
            .match_header("authorization", mockito::Matcher::Any)
            .with_body("{}")
            .expect(0)
            .create_async()
            .await;
        let port: u16 = server.url().rsplit(':').next().unwrap().parse().unwrap();
        let mut request = local(Some(port), None);
        request.auth_username = Some("boss".into());
        request.password = Some("vault-secret".into());

        match test_connection_impl(&request).await {
            ConnectionTestResult::Unreachable { reason } => {
                assert!(reason.contains("could not be verified"), "{reason}");
            }
            other => panic!("expected Unreachable, got {other:?}"),
        }
        with_credential.assert_async().await;
    }

    // --- test_connection_impl: Direct over https with a self-signed cert ---

    /// An HTTPS `/health` server on 127.0.0.1 presenting a throwaway
    /// self-signed certificate (what a daemon's `selfsigned.rs` serves). It
    /// answers 200 to every request and records each request's head, so a
    /// test can prove whether — and with which credential — anything was sent.
    struct SelfSignedHealthServer {
        url: String,
        fingerprint: String,
        requests: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for SelfSignedHealthServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    impl SelfSignedHealthServer {
        fn requests(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
        }
    }

    async fn start_self_signed_health_server() -> SelfSignedHealthServer {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()]).unwrap();
        let fingerprint = crate::direct_proxy::cert_fingerprint_sha256(cert.der());
        let key = rustls::pki_types::PrivateKeyDer::Pkcs8(signing_key.serialize_der().into());
        let config = rustls::ServerConfig::builder_with_provider(std::sync::Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert.der().clone()], key)
        .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    continue;
                };
                let acceptor = acceptor.clone();
                let recorded = recorded.clone();
                tokio::spawn(async move {
                    let Ok(mut tls) = acceptor.accept(stream).await else {
                        return;
                    };
                    let mut head = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                        match tls.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => head.extend_from_slice(&buf[..n]),
                        }
                    }
                    recorded
                        .lock()
                        .unwrap()
                        .push(String::from_utf8_lossy(&head).into_owned());
                    let _ = tls
                        .write_all(
                            b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok",
                        )
                        .await;
                    let _ = tls.shutdown().await;
                });
            }
        });
        SelfSignedHealthServer {
            url: format!("https://{addr}"),
            fingerprint,
            requests,
            task,
        }
    }

    fn direct(url: &str, pin: Option<&str>) -> TestConnectionRequest {
        TestConnectionRequest {
            transport: RemoteTransport::Direct {
                url: url.to_string(),
                tls_fingerprint: pin.map(str::to_string),
            },
            auth_username: Some("boss".to_string()),
            password: Some("vault-secret".to_string()),
        }
    }

    /// Catches: Test Connection ignoring the pin and judging a pinned
    /// self-signed daemon by the system trust store (always "TLS error").
    #[tokio::test]
    async fn a_pinned_self_signed_daemon_is_reachable_through_the_pin() {
        let server = start_self_signed_health_server().await;
        let pin = server.fingerprint.to_ascii_uppercase();
        assert_eq!(
            test_connection_impl(&direct(&server.url, Some(&pin))).await,
            ConnectionTestResult::Reachable
        );
        let requests = server.requests();
        assert_eq!(requests.len(), 1, "{requests:?}");
        assert!(requests[0].starts_with("GET /health "), "{requests:?}");
    }

    /// A pin the server no longer matches fails closed: a clear "certificate
    /// changed" naming both fingerprints, and not one byte of HTTP is sent.
    #[tokio::test]
    async fn a_pin_mismatch_is_reported_as_changed_and_sends_nothing() {
        let server = start_self_signed_health_server().await;
        let wrong = "0".repeat(64);
        match test_connection_impl(&direct(&server.url, Some(&wrong))).await {
            ConnectionTestResult::Unreachable { reason } => {
                assert!(reason.starts_with("certificate changed"), "{reason}");
                assert!(reason.contains(&wrong), "{reason}");
                assert!(reason.contains(&server.fingerprint), "{reason}");
                assert!(!reason.contains("vault-secret"), "{reason}");
            }
            other => panic!("expected Unreachable, got {other:?}"),
        }
        assert!(server.requests().is_empty(), "{:?}", server.requests());
    }

    /// Unpinned self-signed: reported as not trusted, with the fingerprint to
    /// compare, and no request (so no credential) reaches the server.
    #[tokio::test]
    async fn an_unpinned_self_signed_daemon_is_untrusted_and_gets_nothing() {
        let server = start_self_signed_health_server().await;
        match test_connection_impl(&direct(&server.url, None)).await {
            ConnectionTestResult::Unreachable { reason } => {
                assert!(reason.contains("not trusted"), "{reason}");
                assert!(reason.contains(&server.fingerprint), "{reason}");
            }
            other => panic!("expected Unreachable, got {other:?}"),
        }
        assert!(server.requests().is_empty(), "{:?}", server.requests());
    }

    #[tokio::test]
    async fn a_garbled_pin_fails_closed_instead_of_falling_back_to_first_use() {
        let server = start_self_signed_health_server().await;
        match test_connection_impl(&direct(&server.url, Some("not-a-pin"))).await {
            ConnectionTestResult::Unreachable { reason } => {
                assert!(reason.contains("not a SHA-256 fingerprint"), "{reason}");
            }
            other => panic!("expected Unreachable, got {other:?}"),
        }
        assert!(server.requests().is_empty(), "{:?}", server.requests());
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
