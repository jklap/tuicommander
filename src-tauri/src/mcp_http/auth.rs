use crate::AppState;
use axum::extract::{ConnectInfo, State};
use axum::http::{Request, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::collections::{HashSet, VecDeque};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Cookie name used to persist the session after successful Basic Auth.
/// The browser sends cookies automatically in fetch() calls (unlike stored Basic Auth),
/// which is why we need this: JS API calls would otherwise fail with 401 every time.
const SESSION_COOKIE: &str = "tui-session";

/// Failed header digests retained for one IP and one rate-limit window.
const MAX_CACHED_FAILURES_PER_IP: usize = 64;

type CredentialDigest = [u8; 32];

/// One IP's admission state. The mutex covers only state transitions; bcrypt
/// always runs after it is released.
pub(crate) struct AuthRateLimit {
    state: Mutex<AuthRateLimitState>,
    verifying_changed: tokio::sync::Notify,
}

struct AuthRateLimitState {
    attempts: u32,
    window_start: Instant,
    config_digest: CredentialDigest,
    failed: VecDeque<CredentialDigest>,
    verifying: HashSet<CredentialDigest>,
}

impl AuthRateLimit {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(AuthRateLimitState {
                attempts: 0,
                window_start: Instant::now(),
                config_digest: [0; 32],
                failed: VecDeque::new(),
                verifying: HashSet::new(),
            }),
            verifying_changed: tokio::sync::Notify::new(),
        }
    }
}

/// Owns one credential's in-progress verification slot. A request can be
/// cancelled while bcrypt runs, so dropping this guard must release waiters.
struct AuthAttemptGuard {
    limit: Arc<AuthRateLimit>,
    credential: CredentialDigest,
    finished: bool,
}

impl AuthAttemptGuard {
    fn new(limit: Arc<AuthRateLimit>, credential: CredentialDigest) -> Self {
        Self {
            limit,
            credential,
            finished: false,
        }
    }

    fn finish(mut self, failed: bool) {
        finish_auth_attempt(&self.limit, self.credential, failed);
        self.finished = true;
    }
}

impl Drop for AuthAttemptGuard {
    fn drop(&mut self) {
        if !self.finished {
            release_verifying_slot(&self.limit, self.credential);
        }
    }
}

enum AuthAdmission {
    Verify,
    CachedFailure,
    Limited(Duration),
    Wait,
}

/// Result of checking Basic Auth credentials against a config.
pub(super) enum AuthResult {
    /// Credentials are valid
    Ok,
    /// Missing Authorization header
    MissingHeader,
    /// Credentials are invalid (wrong user, wrong password, bad format)
    Invalid,
    /// Auth not configured (no username/password in config)
    NotConfigured,
}

/// Validate a Basic Auth header value against expected credentials.
/// Pure function for testability. NOTE: calls bcrypt::verify — CPU-intensive.
/// Always call this from spawn_blocking in async contexts.
pub(super) fn validate_basic_auth(
    auth_header: Option<&str>,
    expected_username: &str,
    expected_password_hash: &str,
) -> AuthResult {
    if expected_username.is_empty() || expected_password_hash.is_empty() {
        return AuthResult::NotConfigured;
    }

    let Some(auth_value) = auth_header else {
        return AuthResult::MissingHeader;
    };

    let Some(encoded) = auth_value.strip_prefix("Basic ") else {
        return AuthResult::Invalid;
    };

    let Ok(decoded_bytes) =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
    else {
        return AuthResult::Invalid;
    };

    let Ok(decoded) = String::from_utf8(decoded_bytes) else {
        return AuthResult::Invalid;
    };

    let Some((username, password)) = decoded.split_once(':') else {
        return AuthResult::Invalid;
    };

    if username != expected_username {
        return AuthResult::Invalid;
    }

    match bcrypt::verify(password, expected_password_hash) {
        Ok(true) => AuthResult::Ok,
        _ => AuthResult::Invalid,
    }
}

/// Constant-time byte-slice comparison. `mcp_http` serves non-loopback clients
/// (LAN/relay/Tailscale), so comparing secrets with `==` — which short-circuits
/// on the first mismatching byte — is a timing side-channel an attacker could
/// use to recover the session token/cookie byte-by-byte. Always scans the full
/// length of both inputs; never exits early on a byte mismatch. The upfront
/// length check is not itself a secret-dependent branch (input lengths are
/// attacker-visible regardless), so it doesn't reintroduce the leak this
/// guards against.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Check whether the request carries a valid session cookie.
/// This is the fast path — avoids bcrypt on every API call after the first auth.
fn has_valid_session_cookie(req: &Request<axum::body::Body>, session_token: &str) -> bool {
    let cookie_header = req
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let expected = format!("{SESSION_COOKIE}={session_token}");
    cookie_header
        .split(';')
        .map(str::trim)
        .any(|c| ct_eq(c.as_bytes(), expected.as_bytes()))
}

/// Check whether the request carries a valid `?token=<session_token>` query param.
/// This is the primary auth method for remote devices: the QR code URL includes the token,
/// and scanning it authenticates the device (a session cookie is then set for subsequent calls).
fn has_valid_url_token(req: &Request<axum::body::Body>, session_token: &str) -> bool {
    let query = req.uri().query().unwrap_or("");
    let expected = format!("token={session_token}");
    query
        .split('&')
        .any(|param| ct_eq(param.as_bytes(), expected.as_bytes()))
}

/// Build a Set-Cookie header value for the session token.
/// `max_age_secs` controls cookie lifetime (0 = session cookie that expires on browser close).
fn session_cookie_value(token: &str, max_age_secs: u64, secure: bool) -> String {
    // HttpOnly: JS cannot read the cookie (XSS protection)
    // SameSite=Strict: only sent on same-origin requests (stronger CSRF protection)
    // Path=/: valid for all routes
    // Secure: only sent over HTTPS (when TLS is active)
    let secure_flag = if secure { "; Secure" } else { "" };
    let base = format!("{SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/{secure_flag}");
    if max_age_secs > 0 {
        format!("{base}; Max-Age={max_age_secs}")
    } else {
        base // session cookie — expires when browser closes
    }
}

/// Check whether an IP address belongs to a private/LAN network.
/// Covers RFC1918 (10/8, 172.16/12, 192.168/16), CGNAT/Tailscale (100.64/10),
/// IPv6 ULA (fc00::/7), and IPv6 link-local (fe80::/10).
pub(crate) fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_private_ipv4(v4),
        IpAddr::V6(v6) => is_private_ipv6(v6),
    }
}

fn is_private_ipv4(ip: &Ipv4Addr) -> bool {
    let o = ip.octets();
    // 10.0.0.0/8
    if o[0] == 10 {
        return true;
    }
    // 172.16.0.0/12
    if o[0] == 172 && (16..=31).contains(&o[1]) {
        return true;
    }
    // 192.168.0.0/16
    if o[0] == 192 && o[1] == 168 {
        return true;
    }
    // 100.64.0.0/10 (CGNAT / Tailscale)
    if o[0] == 100 && (64..=127).contains(&o[1]) {
        return true;
    }
    false
}

fn is_private_ipv6(ip: &Ipv6Addr) -> bool {
    let seg = ip.segments();
    // fc00::/7 — Unique Local Address (ULA)
    if (seg[0] & 0xfe00) == 0xfc00 {
        return true;
    }
    // fe80::/10 — Link-local
    if (seg[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    false
}

/// Check if a string IP address belongs to the Tailscale CGNAT range (100.64/10)
/// or the Tailscale IPv6 prefix (fd7a:115c:a1e0::/48).
pub(crate) fn is_tailscale_ip(ip_str: &str) -> bool {
    use std::net::IpAddr;
    let ip: IpAddr = match ip_str.parse() {
        Ok(ip) => ip,
        Err(_) => return false,
    };
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 100 && (64..=127).contains(&o[1])
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0
        }
    }
}

/// Basic Auth middleware that validates credentials against config.
///
/// Flow:
/// 1. Localhost connections bypass auth (local Tauri app).
/// 2. Requests with a valid session cookie pass through (fast path — no bcrypt).
/// 3. Requests with a valid `Authorization: Basic` header pass through AND get
///    a session cookie set so subsequent JS fetch() calls are authenticated.
/// 4. Everything else → 401.
///
/// Why session cookies? Browsers store Basic Auth credentials for direct navigation
/// but do NOT send them in JS `fetch()` calls. The session cookie is sent automatically
/// with all same-origin fetch() calls, allowing the SPA to work after the initial auth.
pub async fn basic_auth_middleware(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Mark the request as authenticated for downstream route guards
    // (require_local_or_auth). Reaching a handler implies the request passed
    // one of the auth gates below (loopback/LAN bypass, session cookie, URL
    // token, or Basic Auth); every failed path short-circuits with 401/429
    // here and never runs the handler, so the marker only ever propagates to
    // authenticated handler invocations. (Boss 2026-06-27: token-auth = full
    // trust across config + agent-spawn + prompt routes.)
    req.extensions_mut().insert(super::guards::Authenticated);

    // Localhost bypass: only in desktop mode where the Tauri webview connects
    // locally. Headless mode binds 0.0.0.0 so loopback must be authenticated
    // like any other address — otherwise any local process gets full access.
    #[cfg(feature = "desktop")]
    if addr.ip().is_loopback() {
        return next.run(req).await;
    }

    // LAN bypass: skip auth for private/RFC1918 addresses when configured
    if state.config.read().services.auth.lan_auth_bypass && is_private_ip(&addr.ip()) {
        return next.run(req).await;
    }

    let session_token = state.session_token.read().clone();
    let token_duration_secs = state
        .config
        .read()
        .services
        .auth
        .session_token_duration_secs;

    // Detect TLS for Secure cookie flag (dual-protocol injects Protocol extension)
    let is_tls = req
        .extensions()
        .get::<axum_server_dual_protocol::Protocol>()
        .is_some_and(|p| matches!(p, axum_server_dual_protocol::Protocol::Tls));

    // Fast path: valid session cookie skips bcrypt entirely.
    // The cookie is re-issued on every hit so the expiry slides: with an absolute
    // Max-Age a phone was logged out exactly `session_token_duration_secs` (1 day
    // by default) after scanning the QR, even while in constant use, and fell back
    // to the Basic Auth prompt because the SPA stores the token nowhere.
    if has_valid_session_cookie(&req, &session_token) {
        let mut response = next.run(req).await;
        if let Ok(val) = session_cookie_value(&session_token, token_duration_secs, is_tls).parse() {
            response.headers_mut().insert(header::SET_COOKIE, val);
        }
        return response;
    }

    // Primary remote auth: valid ?token=<session_token> in URL.
    // The QR code embeds this token, so scanning it authenticates the device.
    // We set a session cookie so the SPA's subsequent fetch() calls are also authenticated.
    if has_valid_url_token(&req, &session_token) {
        state.auth_rate_limits.remove(&addr.ip());
        let mut response = next.run(req).await;
        if let Ok(val) = session_cookie_value(&session_token, token_duration_secs, is_tls).parse() {
            response.headers_mut().insert(header::SET_COOKIE, val);
        }
        return response;
    }

    let (rate_max, rate_window_secs, username, hash) = {
        let config = state.config.read();
        (
            config.services.auth.auth_rate_limit_max,
            config.services.auth.auth_rate_limit_window_secs,
            config.services.auth.username.clone(),
            config.services.auth.password_hash.clone(),
        )
    };
    let client_ip = addr.ip();

    // Fallback: Basic Auth. Admission happens before bcrypt so a full IP
    // window remains a brute-force bound, while known stale credentials do
    // not consume it repeatedly.
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let config_digest = auth_config_digest(&username, &hash);
    let credential_digest = failed_credential_digest(auth_header.as_deref(), &config_digest);
    let limit = state
        .auth_rate_limits
        .entry(client_ip)
        .or_insert_with(|| Arc::new(AuthRateLimit::new()))
        .clone();

    let result = loop {
        let notified = limit.verifying_changed.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let admission = admit_auth_attempt(
            &limit,
            credential_digest,
            config_digest,
            rate_max,
            rate_window_secs,
        );
        match admission {
            AuthAdmission::CachedFailure => break AuthResult::Invalid,
            AuthAdmission::Limited(retry_after) => return rate_limited_response(retry_after),
            // A duplicate is already in bcrypt. Wait for the owner to finish
            // instead of polling the admission mutex.
            AuthAdmission::Wait => notified.await,
            AuthAdmission::Verify => {
                let attempt = AuthAttemptGuard::new(Arc::clone(&limit), credential_digest);
                // bcrypt::verify is CPU-intensive (~100ms). Run it on a blocking
                // thread to avoid stalling the single-threaded tokio runtime.
                let result = tokio::task::spawn_blocking({
                    let auth_header = auth_header.clone();
                    let username = username.clone();
                    let hash = hash.clone();
                    move || validate_basic_auth(auth_header.as_deref(), &username, &hash)
                })
                .await
                .unwrap_or_else(|e| {
                    tracing::error!(source = "auth", error = %e, "spawn_blocking for bcrypt panicked or was cancelled");
                    AuthResult::Invalid
                });
                attempt.finish(matches!(result, AuthResult::Invalid));
                break result;
            }
        }
    };

    match result {
        AuthResult::Ok => {
            // A success supersedes every stale failure for this IP.
            state.auth_rate_limits.remove(&client_ip);
            let mut response = next.run(req).await;
            if let Ok(val) =
                session_cookie_value(&session_token, token_duration_secs, is_tls).parse()
            {
                response.headers_mut().insert(header::SET_COOKIE, val);
            }
            response
        }
        AuthResult::MissingHeader | AuthResult::NotConfigured => {
            unauthorized_response("Scan the QR code or authenticate with Basic Auth")
        }
        AuthResult::Invalid => {
            tracing::warn!(source = "auth", ip = %client_ip, "Failed auth attempt");
            unauthorized_response("Invalid credentials")
        }
    }
}

fn auth_config_digest(username: &str, password_hash: &str) -> CredentialDigest {
    let mut digest = Sha256::new();
    digest.update(b"tuicommander-auth-config-v1\0");
    digest.update(username.as_bytes());
    digest.update(b"\0");
    digest.update(password_hash.as_bytes());
    digest.finalize().into()
}

fn failed_credential_digest(
    auth_header: Option<&str>,
    config_digest: &CredentialDigest,
) -> CredentialDigest {
    let mut digest = Sha256::new();
    digest.update(b"tuicommander-failed-basic-v1\0");
    digest.update(config_digest);
    digest.update(b"\0");
    digest.update(auth_header.unwrap_or("").as_bytes());
    digest.finalize().into()
}

fn admit_auth_attempt(
    limit: &AuthRateLimit,
    credential: CredentialDigest,
    config: CredentialDigest,
    rate_max: u32,
    window_secs: u64,
) -> AuthAdmission {
    let window = Duration::from_secs(window_secs);
    let mut state = limit.state.lock();
    if state.window_start.elapsed() >= window || state.config_digest != config {
        state.attempts = 0;
        state.window_start = Instant::now();
        state.config_digest = config;
        state.failed.clear();
        state.verifying.clear();
    }
    if state.failed.contains(&credential) {
        return AuthAdmission::CachedFailure;
    }
    if state.verifying.contains(&credential) {
        return AuthAdmission::Wait;
    }
    if rate_max > 0 && state.attempts >= rate_max {
        return AuthAdmission::Limited(window.saturating_sub(state.window_start.elapsed()));
    }
    state.attempts += 1;
    state.verifying.insert(credential);
    AuthAdmission::Verify
}

fn finish_auth_attempt(limit: &AuthRateLimit, credential: CredentialDigest, failed: bool) {
    let mut state = limit.state.lock();
    state.verifying.remove(&credential);
    if failed && !state.failed.contains(&credential) {
        if state.failed.len() == MAX_CACHED_FAILURES_PER_IP {
            state.failed.pop_front();
        }
        state.failed.push_back(credential);
    }
    drop(state);
    limit.verifying_changed.notify_waiters();
}

fn release_verifying_slot(limit: &AuthRateLimit, credential: CredentialDigest) {
    limit.state.lock().verifying.remove(&credential);
    limit.verifying_changed.notify_waiters();
}

fn unauthorized_response(message: &'static str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"TUICommander\"")],
        message,
    )
        .into_response()
}

fn rate_limited_response(retry_after: Duration) -> Response {
    let retry_after = retry_after.as_secs() + 1;
    tracing::warn!(
        source = "auth",
        retry_after,
        "Rate limited — too many failed auth attempts"
    );
    (
        StatusCode::TOO_MANY_REQUESTS,
        [
            (header::RETRY_AFTER, retry_after.to_string()),
            (
                header::WWW_AUTHENTICATE,
                "Basic realm=\"TUICommander\"".to_string(),
            ),
        ],
        "Too many failed authentication attempts",
    )
        .into_response()
}

/// Evict rate-limit entries whose window has fully elapsed. Called periodically
/// by the background reaper so the per-IP failure cache cannot outlive its TTL.
pub(super) fn sweep_expired_rate_limits(
    rate_limits: &dashmap::DashMap<std::net::IpAddr, Arc<AuthRateLimit>>,
    window_secs: u64,
) -> usize {
    let window = Duration::from_secs(window_secs);
    let before = rate_limits.len();
    rate_limits.retain(|_, limit| limit.state.lock().window_start.elapsed() < window);
    before - rate_limits.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ct_eq tests ---

    #[test]
    fn ct_eq_matches_equal_slices() {
        assert!(ct_eq(b"same-token-value", b"same-token-value"));
    }

    #[test]
    fn ct_eq_rejects_different_content_same_length() {
        assert!(!ct_eq(b"token-aaaaaaaaaa", b"token-bbbbbbbbbb"));
    }

    #[test]
    fn ct_eq_rejects_different_length() {
        assert!(!ct_eq(b"short", b"much-longer-value"));
    }

    #[test]
    fn ct_eq_empty_slices_are_equal() {
        assert!(ct_eq(b"", b""));
    }

    // --- is_private_ip tests ---

    #[test]
    fn private_ipv4_rfc1918() {
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(10, 255, 255, 255))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 31, 255, 255))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 68, 111))));
    }

    #[test]
    fn private_ipv4_cgnat_tailscale() {
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(
            100, 127, 255, 255
        ))));
    }

    #[test]
    fn public_ipv4_not_private() {
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 32, 0, 1))));
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(100, 128, 0, 1))));
    }

    #[test]
    fn private_ipv6_ula() {
        // fd00::1 — ULA
        assert!(is_private_ip(&IpAddr::V6(Ipv6Addr::new(
            0xfd00, 0, 0, 0, 0, 0, 0, 1
        ))));
        // fc00::1 — ULA
        assert!(is_private_ip(&IpAddr::V6(Ipv6Addr::new(
            0xfc00, 0, 0, 0, 0, 0, 0, 1
        ))));
    }

    #[test]
    fn private_ipv6_link_local() {
        assert!(is_private_ip(&IpAddr::V6(Ipv6Addr::new(
            0xfe80, 0, 0, 0, 0, 0, 0, 1
        ))));
    }

    #[test]
    fn public_ipv6_not_private() {
        assert!(!is_private_ip(&IpAddr::V6(Ipv6Addr::new(
            0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888
        ))));
    }

    #[test]
    fn session_cookie_with_max_age() {
        let cookie = session_cookie_value("abc-123", 86400, false);
        assert!(cookie.contains("tui-session=abc-123"));
        assert!(cookie.contains("Max-Age=86400"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(!cookie.contains("Secure"));
    }

    #[test]
    fn session_cookie_zero_duration_omits_max_age() {
        let cookie = session_cookie_value("abc-123", 0, false);
        assert!(cookie.contains("tui-session=abc-123"));
        assert!(!cookie.contains("Max-Age"));
        assert!(cookie.contains("HttpOnly"));
    }

    #[test]
    fn session_cookie_never_duration() {
        let cookie = session_cookie_value("tok", 31536000, false);
        assert!(cookie.contains("Max-Age=31536000"));
    }

    #[test]
    fn session_cookie_secure_flag_on_tls() {
        let cookie = session_cookie_value("tok", 86400, true);
        assert!(cookie.contains("; Secure"));
    }

    #[test]
    fn tailscale_ip_detection() {
        assert!(is_tailscale_ip("100.80.90.53"));
        assert!(is_tailscale_ip("100.64.0.1"));
        assert!(is_tailscale_ip("100.127.255.255"));
        assert!(!is_tailscale_ip("100.128.0.1"));
        assert!(!is_tailscale_ip("192.168.1.1"));
        assert!(is_tailscale_ip("fd7a:115c:a1e0::c601:5a3a"));
        assert!(!is_tailscale_ip("fe80::1"));
        assert!(!is_tailscale_ip("not-an-ip"));
    }

    #[test]
    fn valid_session_cookie_matches() {
        let req = Request::get("/")
            .header(header::COOKIE, "tui-session=my-token; other=val")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(has_valid_session_cookie(&req, "my-token"));
    }

    #[test]
    fn invalid_session_cookie_rejected() {
        let req = Request::get("/")
            .header(header::COOKIE, "tui-session=wrong-token")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(!has_valid_session_cookie(&req, "correct-token"));
    }

    // --- validate_basic_auth tests ---

    fn basic_header(user: &str, pass: &str) -> String {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
        format!("Basic {encoded}")
    }

    #[test]
    fn basic_auth_valid_credentials() {
        let hash = bcrypt::hash("secret123", 4).unwrap(); // cost=4 for fast tests
        assert!(matches!(
            validate_basic_auth(Some(&basic_header("admin", "secret123")), "admin", &hash),
            AuthResult::Ok
        ));
    }

    #[test]
    fn basic_auth_wrong_password() {
        let hash = bcrypt::hash("correct", 4).unwrap();
        assert!(matches!(
            validate_basic_auth(Some(&basic_header("admin", "wrong")), "admin", &hash),
            AuthResult::Invalid
        ));
    }

    #[test]
    fn basic_auth_missing_header() {
        let hash = bcrypt::hash("pass", 4).unwrap();
        assert!(matches!(
            validate_basic_auth(None, "admin", &hash),
            AuthResult::MissingHeader
        ));
    }

    #[test]
    fn basic_auth_empty_config_not_configured() {
        assert!(matches!(
            validate_basic_auth(Some(&basic_header("admin", "pass")), "", ""),
            AuthResult::NotConfigured
        ));
    }

    #[test]
    fn basic_auth_malformed_base64() {
        assert!(matches!(
            validate_basic_auth(Some("Basic !!!not-base64!!!"), "admin", "somehash"),
            AuthResult::Invalid
        ));
    }

    #[test]
    fn basic_auth_wrong_username() {
        let hash = bcrypt::hash("pass", 4).unwrap();
        assert!(matches!(
            validate_basic_auth(Some(&basic_header("hacker", "pass")), "admin", &hash),
            AuthResult::Invalid
        ));
    }

    #[test]
    fn basic_auth_no_colon_separator() {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode("nocolon");
        assert!(matches!(
            validate_basic_auth(Some(&format!("Basic {encoded}")), "admin", "somehash"),
            AuthResult::Invalid
        ));
    }

    #[test]
    fn basic_auth_not_basic_scheme() {
        assert!(matches!(
            validate_basic_auth(Some("Bearer some-token"), "admin", "somehash"),
            AuthResult::Invalid
        ));
    }

    #[test]
    fn valid_url_token_matches() {
        let req = Request::get("/?token=abc&other=1")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(has_valid_url_token(&req, "abc"));
    }

    #[test]
    fn invalid_url_token_rejected() {
        let req = Request::get("/?token=wrong")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(!has_valid_url_token(&req, "correct"));
    }

    // --- rate limiting tests ---

    #[test]
    fn repeated_failed_header_is_admitted_once_per_window() {
        let limit = AuthRateLimit::new();
        let config = auth_config_digest("boss", "hash");
        let credential = failed_credential_digest(Some("Basic stale"), &config);
        assert!(matches!(
            admit_auth_attempt(&limit, credential, config, 2, 300),
            AuthAdmission::Verify
        ));
        finish_auth_attempt(&limit, credential, true);
        for _ in 0..20 {
            assert!(matches!(
                admit_auth_attempt(&limit, credential, config, 2, 300),
                AuthAdmission::CachedFailure
            ));
        }
        assert_eq!(limit.state.lock().attempts, 1);
    }

    #[test]
    fn unseen_candidates_exhaust_the_ip_budget() {
        let limit = AuthRateLimit::new();
        let config = auth_config_digest("boss", "hash");
        for header in ["Basic wrong-a", "Basic wrong-b"] {
            let credential = failed_credential_digest(Some(header), &config);
            assert!(matches!(
                admit_auth_attempt(&limit, credential, config, 2, 300),
                AuthAdmission::Verify
            ));
            finish_auth_attempt(&limit, credential, true);
        }
        let correct = failed_credential_digest(Some("Basic correct"), &config);
        assert!(matches!(
            admit_auth_attempt(&limit, correct, config, 2, 300),
            AuthAdmission::Limited(_)
        ));
    }

    #[tokio::test]
    async fn dropping_a_verifying_request_releases_its_slot() {
        let limit = Arc::new(AuthRateLimit::new());
        let config = auth_config_digest("boss", "hash");
        let credential = failed_credential_digest(Some("Basic stale"), &config);
        assert!(matches!(
            admit_auth_attempt(&limit, credential, config, 2, 300),
            AuthAdmission::Verify
        ));

        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let request = tokio::spawn({
            let limit = Arc::clone(&limit);
            async move {
                let _slot = AuthAttemptGuard::new(limit, credential);
                entered_tx.send(()).unwrap();
                std::future::pending::<()>().await;
            }
        });
        entered_rx.await.unwrap();
        request.abort();
        let _ = request.await;

        assert!(matches!(
            admit_auth_attempt(&limit, credential, config, 2, 300),
            AuthAdmission::Verify
        ));
    }

    #[test]
    fn sweep_evicts_only_expired_rate_limits() {
        let map = dashmap::DashMap::new();
        let expired: IpAddr = "10.0.0.1".parse().unwrap();
        let fresh: IpAddr = "10.0.0.2".parse().unwrap();
        let expired_limit = Arc::new(AuthRateLimit::new());
        expired_limit.state.lock().window_start = Instant::now() - Duration::from_secs(301);
        map.insert(expired, expired_limit);
        map.insert(fresh, Arc::new(AuthRateLimit::new()));

        let removed = sweep_expired_rate_limits(&map, 300);

        assert_eq!(removed, 1, "only the expired entry should be evicted");
        assert!(map.get(&expired).is_none(), "expired entry gone");
        assert!(map.get(&fresh).is_some(), "fresh entry retained");
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn sweep_empty_map_is_noop() {
        let map: dashmap::DashMap<IpAddr, Arc<AuthRateLimit>> = dashmap::DashMap::new();
        assert_eq!(sweep_expired_rate_limits(&map, 300), 0);
    }

    /// A phone authenticates once by scanning the QR and then never sends the
    /// token again — the SPA stores it nowhere, so the cookie is the whole
    /// session. With an absolute `Max-Age` the device was logged out exactly
    /// `session_token_duration_secs` after the scan (1 day by default) even
    /// while in daily use, and fell back to the Basic Auth prompt. Every
    /// cookie-authenticated request must therefore re-issue the cookie.
    #[tokio::test]
    async fn cookie_fast_path_slides_the_expiry() {
        use tower::ServiceExt;

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state
            .config
            .write()
            .services
            .auth
            .session_token_duration_secs = 86400;

        let app = axum::Router::new()
            .route("/ping", axum::routing::get(|| async { "pong" }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                basic_auth_middleware,
            ));

        // A public address: neither the desktop loopback bypass nor the LAN
        // bypass may carry this request — only the cookie.
        let req = Request::get("/ping")
            .header(header::COOKIE, "tui-session=test-token")
            .extension(ConnectInfo(SocketAddr::from(([203, 0, 113, 5], 51234))))
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .expect("a cookie-authenticated request must refresh the cookie")
            .to_str()
            .unwrap();
        assert!(cookie.contains("tui-session=test-token"), "got {cookie}");
        assert!(cookie.contains("Max-Age=86400"), "got {cookie}");
    }

    /// A browser may replay its stale Basic header while it waits for a new
    /// challenge. Those replays must not exhaust the whole IP budget before
    /// the user can provide the correct password.
    #[tokio::test]
    async fn stale_basic_header_is_challenged_once_then_correct_login_succeeds() {
        use tower::ServiceExt;

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        {
            let mut config = state.config.write();
            config.services.auth.username = "boss".to_string();
            config.services.auth.password_hash = bcrypt::hash("correct", 4).unwrap();
            config.services.auth.auth_rate_limit_max = 2;
            config.services.auth.auth_rate_limit_window_secs = 300;
        }

        let app = axum::Router::new()
            .route("/ping", axum::routing::get(|| async { "pong" }))
            .layer(axum::middleware::from_fn_with_state(
                state,
                basic_auth_middleware,
            ));
        let remote = ConnectInfo(SocketAddr::from(([203, 0, 113, 5], 51234)));

        for _ in 0..20 {
            let response = app
                .clone()
                .oneshot(
                    Request::get("/ping")
                        .header(header::AUTHORIZATION, basic_header("boss", "wrong"))
                        .extension(remote)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert_eq!(
                response.headers().get(header::WWW_AUTHENTICATE),
                Some(&header::HeaderValue::from_static(
                    "Basic realm=\"TUICommander\""
                ))
            );
        }

        let response = app
            .oneshot(
                Request::get("/ping")
                    .header(header::AUTHORIZATION, basic_header("boss", "correct"))
                    .extension(remote)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().contains_key(header::SET_COOKIE));
    }

    #[test]
    fn config_change_invalidates_cached_failures() {
        let limit = AuthRateLimit::new();
        let old_config = auth_config_digest("boss", "old-hash");
        let old_header = failed_credential_digest(Some("Basic old"), &old_config);
        assert!(matches!(
            admit_auth_attempt(&limit, old_header, old_config, 1, 300),
            AuthAdmission::Verify
        ));
        finish_auth_attempt(&limit, old_header, true);

        let new_config = auth_config_digest("boss", "new-hash");
        let new_header = failed_credential_digest(Some("Basic new"), &new_config);
        assert!(matches!(
            admit_auth_attempt(&limit, new_header, new_config, 1, 300),
            AuthAdmission::Verify
        ));
    }
}
