//! Browser-origin and DNS-rebinding checks for the TCP listener, before auth.
use super::auth::is_private_ip;
use crate::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::net::IpAddr;
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};

const APP_ORIGINS: &[&str] = &[
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
    "http://localhost",
    "http://127.0.0.1",
    "http://127.0.0.1:1421",
    "http://localhost:1421",
];

/// This machine's own names (lowercase): the hostname and its `.local` form. A
/// MagicDNS short name is the hostname, so peers reach the daemon by it.
fn own_hostnames() -> Vec<String> {
    #[cfg(unix)]
    let raw = {
        let mut buf = [0u8; 256];
        // SAFETY: buf is valid for buf.len() bytes; gethostname NUL-terminates on success.
        let ok = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0;
        ok.then(|| {
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            String::from_utf8_lossy(&buf[..end]).into_owned()
        })
    };
    #[cfg(not(unix))]
    let raw = std::env::var("COMPUTERNAME").ok();
    let Some(raw) = raw else {
        return Vec::new();
    };
    let host = raw.trim().trim_end_matches(".local").to_ascii_lowercase();
    if host.is_empty() {
        return Vec::new();
    }
    vec![format!("{host}.local"), host]
}

pub(super) struct RequestBoundary {
    local_ips: Vec<String>,
    hostnames: Vec<String>,
    state: Arc<AppState>,
}

impl RequestBoundary {
    pub(super) fn new(state: Arc<AppState>) -> Arc<Self> {
        Arc::new(Self {
            local_ips: crate::get_local_ips_impl(&state)
                .into_iter()
                .map(|e| e.ip)
                .collect(),
            hostnames: own_hostnames(),
            state,
        })
    }

    fn allowed_host(&self, headers: &HeaderMap, uri: &Uri) -> Option<String> {
        // Multiple Host headers and malformed authorities must not be interpreted
        // differently by a proxy and by our handler. Never trust forwarded headers.
        // HTTP/2 carries the authority in the URI and sends no Host header.
        let host = match headers.get_all(header::HOST).iter().count() {
            0 => uri.authority()?.as_str(),
            1 => headers.get(header::HOST)?.to_str().ok()?,
            _ => return None,
        };
        let authority = host.parse::<axum::http::uri::Authority>().ok()?;
        if authority.as_str().contains('@') {
            return None;
        }
        let name = authority
            .host()
            .trim_start_matches('[')
            .trim_end_matches(']');
        let allowed = name.eq_ignore_ascii_case("localhost")
            || name
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.is_loopback() || is_private_ip(&ip))
            || self.local_ips.iter().any(|ip| ip == name)
            || self.hostnames.iter().any(|h| name.eq_ignore_ascii_case(h))
            || matches!(&*self.state.tailscale_state.read(),
                crate::tailscale::TailscaleState::Running { fqdn, .. }
                    if name.eq_ignore_ascii_case(fqdn));
        allowed.then(|| host.to_string())
    }

    fn allowed_origin(&self, origin: &HeaderValue, host: &str) -> bool {
        let Ok(origin) = origin.to_str() else {
            return false;
        };
        APP_ORIGINS.contains(&origin)
            || origin == format!("http://{host}")
            || origin == format!("https://{host}")
    }

    pub(super) fn cors(self: &Arc<Self>) -> CorsLayer {
        let boundary = Arc::clone(self);
        CorsLayer::new()
            .allow_origin(AllowOrigin::predicate(move |origin, parts| {
                origin
                    .to_str()
                    .is_ok_and(|origin| APP_ORIGINS.contains(&origin))
                    || boundary
                        .allowed_host(&parts.headers, &parts.uri)
                        .is_some_and(|host| boundary.allowed_origin(origin, &host))
            }))
            .allow_credentials(true)
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::PUT,
                axum::http::Method::DELETE,
                axum::http::Method::PATCH,
                axum::http::Method::OPTIONS,
            ])
            .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
    }
}

pub(super) async fn check(
    State(boundary): State<Arc<RequestBoundary>>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let Some(host) = boundary.allowed_host(req.headers(), req.uri()) else {
        return (StatusCode::FORBIDDEN, "Untrusted Host").into_response();
    };
    let origins = req.headers().get_all(header::ORIGIN);
    if origins.iter().count() > 1
        || origins
            .iter()
            .any(|origin| !boundary.allowed_origin(origin, &host))
        || (req
            .headers()
            .get("sec-fetch-site")
            .is_some_and(|site| site == "cross-site")
            && !req
                .headers()
                .get(header::ORIGIN)
                .and_then(|origin| origin.to_str().ok())
                .is_some_and(|origin| APP_ORIGINS.contains(&origin)))
    {
        return (StatusCode::FORBIDDEN, "Untrusted Origin").into_response();
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;
    use tower::ServiceExt;

    fn state() -> Arc<AppState> {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        *state.session_token.write() = "boundary-token".into();
        state
    }

    fn request(
        path: &str,
        host: &str,
        origin: Option<&str>,
        token: bool,
        peer: [u8; 4],
    ) -> Request<Body> {
        let uri = if token {
            format!("{path}?token=boundary-token")
        } else {
            path.into()
        };
        let mut req = Request::post(uri)
            .header(header::HOST, host)
            .header(header::CONTENT_TYPE, "application/json")
            .extension(ConnectInfo(SocketAddr::from((peer, 12345))))
            .body(Body::from("{}"))
            .unwrap();
        if let Some(origin) = origin {
            req.headers_mut()
                .insert(header::ORIGIN, origin.parse().unwrap());
        }
        req
    }

    /// Catches a drive-by page reaching privileged handlers even with a valid token.
    #[tokio::test]
    async fn foreign_origin_cannot_reach_debug_or_session_write() {
        let app = super::super::build_router(state(), true, true);
        for path in ["/debug/invoke_js", "/sessions/missing/write"] {
            let response = app
                .clone()
                .oneshot(request(
                    path,
                    "127.0.0.1:9876",
                    Some("https://evil.example"),
                    true,
                    [127, 0, 0, 1],
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
        }
    }

    /// Catches DNS rebinding bypassing origin checks with an attacker-controlled Host.
    #[tokio::test]
    async fn rebinding_host_cannot_reach_debug_or_session_write() {
        let app = super::super::build_router(state(), true, true);
        for path in ["/debug/invoke_js", "/sessions/missing/write"] {
            let response = app
                .clone()
                .oneshot(request(
                    path,
                    "attacker.example:9876",
                    Some("http://attacker.example:9876"),
                    true,
                    [127, 0, 0, 1],
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
        }
    }

    /// Catches #1535: the daemon answering 403 "Untrusted Host" to its own hostname
    /// (how a desktop reaches it over MagicDNS), while a foreign name stays rejected.
    #[tokio::test]
    async fn own_hostname_is_trusted_and_foreign_name_is_not() {
        let hostname = own_hostnames().pop().expect("machine has a hostname");
        let app = super::super::build_remote_router(state());
        for (host, expected) in [
            (format!("{hostname}:9877"), StatusCode::OK),
            (format!("{hostname}.local:9877"), StatusCode::OK),
            ("attacker.example:9877".to_string(), StatusCode::FORBIDDEN),
        ] {
            let req = Request::get("/health")
                .header(header::HOST, &host)
                .extension(ConnectInfo(SocketAddr::from(([100, 64, 0, 3], 12345))))
                .body(Body::empty())
                .unwrap();
            assert_eq!(
                app.clone().oneshot(req).await.unwrap().status(),
                expected,
                "{host}"
            );
        }
    }

    /// Catches TCP loopback or the legacy LAN preference granting unauthenticated writes.
    #[tokio::test]
    async fn local_http_still_requires_credentials() {
        let state = state();
        state.config.write().services.auth.lan_auth_bypass = true;
        let app = super::super::build_router(state, true, true);
        for peer in [[127, 0, 0, 1], [192, 168, 1, 12]] {
            for path in ["/debug/invoke_js", "/sessions/missing/write"] {
                let response = app
                    .clone()
                    .oneshot(request(path, "127.0.0.1:9876", None, false, peer))
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    StatusCode::UNAUTHORIZED,
                    "{path}: {peer:?}"
                );
            }
        }
    }

    /// Catches hardening breaking the existing WebView, native HTTP and remote PWA clients.
    #[tokio::test]
    async fn legitimate_clients_with_token_reach_real_handlers() {
        let app = super::super::build_router(state(), true, true);
        for (origin, host, peer) in [
            (Some("tauri://localhost"), "127.0.0.1:9876", [127, 0, 0, 1]),
            (
                Some("http://tauri.localhost"),
                "127.0.0.1:9876",
                [127, 0, 0, 1],
            ),
            (
                Some("http://127.0.0.1:1421"),
                "127.0.0.1:9876",
                [127, 0, 0, 1],
            ),
            (None, "127.0.0.1:9876", [127, 0, 0, 1]),
            (None, "100.64.0.2:9876", [100, 64, 0, 3]),
            (
                Some("https://192.168.1.2:9876"),
                "192.168.1.2:9876",
                [192, 168, 1, 3],
            ),
        ] {
            let mut req = request("/api/auth/session-token", host, origin, true, peer);
            *req.method_mut() = axum::http::Method::GET;
            if origin == Some("tauri://localhost") {
                req.headers_mut()
                    .insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
            }
            let response = app.clone().oneshot(req).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{origin:?} {host}");
            let body = axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(body["token"], "boundary-token");
        }
    }

    /// Catches accepting opaque origins, absent/duplicate hosts or cross-site GETs.
    #[tokio::test]
    async fn malformed_browser_requests_never_reach_handlers() {
        let app = super::super::build_router(state(), true, true);
        for case in [
            "missing-host",
            "duplicate-host",
            "opaque-origin",
            "cross-site",
        ] {
            let mut req = request(
                "/api/auth/session-token",
                "127.0.0.1:9876",
                None,
                true,
                [127, 0, 0, 1],
            );
            *req.method_mut() = axum::http::Method::GET;
            match case {
                "missing-host" => {
                    req.headers_mut().remove(header::HOST);
                }
                "duplicate-host" => {
                    req.headers_mut()
                        .append(header::HOST, HeaderValue::from_static("attacker.example"));
                }
                "opaque-origin" => {
                    req.headers_mut()
                        .insert(header::ORIGIN, HeaderValue::from_static("null"));
                }
                "cross-site" => {
                    req.headers_mut()
                        .insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
                }
                _ => unreachable!(),
            }
            assert_eq!(
                app.clone().oneshot(req).await.unwrap().status(),
                StatusCode::FORBIDDEN,
                "{case}"
            );
        }
    }

    /// Catches putting auth/health/preflight outside the origin boundary on the daemon.
    #[tokio::test]
    async fn daemon_health_and_preflight_cannot_escape_boundary() {
        let app = super::super::build_remote_router(state());
        for method in [axum::http::Method::GET, axum::http::Method::OPTIONS] {
            let req = Request::builder()
                .method(method)
                .uri("/health")
                .header(header::HOST, "attacker.example")
                .header(header::ORIGIN, "http://attacker.example")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
                .body(Body::empty())
                .unwrap();
            assert_eq!(
                app.clone().oneshot(req).await.unwrap().status(),
                StatusCode::FORBIDDEN
            );
        }
        let req = Request::builder()
            .method("OPTIONS")
            .uri("/sessions/missing/write")
            .header(header::HOST, "127.0.0.1:9876")
            .header(header::ORIGIN, "tauri://localhost")
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
            .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "tauri://localhost"
        );
    }
}

#[cfg(test)]
#[path = "request_boundary_critic_tests.rs"]
mod critic_tests;
