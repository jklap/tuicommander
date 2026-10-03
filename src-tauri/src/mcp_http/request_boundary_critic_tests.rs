//! Adversarial tests for the local API origin boundary (story 1456-351c), written by the critic.
//! Each test names the plausible bug it catches.

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

fn get(uri: &str, host: Option<&str>, origin: Option<&str>, peer: [u8; 4]) -> Request<Body> {
    let mut builder = Request::get(uri);
    if let Some(host) = host {
        builder = builder.header(header::HOST, host);
    }
    if let Some(origin) = origin {
        builder = builder.header(header::ORIGIN, origin);
    }
    builder
        .extension(ConnectInfo(SocketAddr::from((peer, 4000))))
        .body(Body::empty())
        .unwrap()
}

/// Catches HTTP/2 (ALPN `h2` is advertised on the Tailscale HTTPS listener): hyper hands
/// the handler a request whose authority lives in the URI and has NO `Host` header, so
/// requiring the header locks every HTTPS mobile PWA / browser out with 403 "Untrusted Host".
#[tokio::test]
async fn http2_request_authority_in_uri_without_host_header_is_accepted() {
    let app = super::super::build_router(state(), true, true);
    let response = app
        .oneshot(get(
            "https://100.64.0.2:9876/api/auth/session-token?token=boundary-token",
            None,
            Some("https://100.64.0.2:9876"),
            [100, 64, 0, 3],
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

/// Catches the h2 fallback above trusting a hostile URI authority: rebinding over h2
/// must still be rejected.
#[tokio::test]
async fn http2_hostile_uri_authority_without_host_header_is_rejected() {
    let app = super::super::build_router(state(), true, true);
    let response = app
        .oneshot(get(
            "https://attacker.example:9876/debug/invoke_js?token=boundary-token",
            None,
            Some("https://attacker.example:9876"),
            [127, 0, 0, 1],
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// Catches suffix/prefix/parse confusion in the Host allow-list (rebinding names that
/// merely contain a loopback token, 0.0.0.0, userinfo tricks, mapped IPv6, trailing dot).
#[tokio::test]
async fn hostile_host_spellings_are_rejected_on_any_path() {
    let app = super::super::build_router(state(), true, true);
    for host in [
        "127.0.0.1.attacker.example:9876",
        "localhost.attacker.example:9876",
        "attacker.example:9876@127.0.0.1",
        "127.0.0.1@attacker.example",
        "0.0.0.0:9876",
        "[::ffff:127.0.0.1]:9876",
        "localhost.:9876",
        "",
    ] {
        let response = app
            .clone()
            .oneshot(get(
                "/no/such/route?token=boundary-token",
                Some(host),
                None,
                [127, 0, 0, 1],
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{host:?}");
    }
}

/// Catches an Origin that shares a prefix with the Host, a different port on the same
/// loopback host (another local web server), or a different scheme being accepted.
#[tokio::test]
async fn near_miss_origins_are_rejected_even_with_valid_host_and_token() {
    let app = super::super::build_router(state(), true, true);
    for origin in [
        "http://127.0.0.1:9876.attacker.example",
        "http://127.0.0.1:3000",
        "http://localhost:3000",
        "http://127.0.0.1:9876/",
        "ftp://127.0.0.1:9876",
        "null",
    ] {
        let response = app
            .clone()
            .oneshot(get(
                "/api/auth/session-token?token=boundary-token",
                Some("127.0.0.1:9876"),
                Some(origin),
                [127, 0, 0, 1],
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{origin}");
    }
}

/// Catches a WebSocket upgrade or CORS preflight from a foreign page reaching a handler
/// (browsers always send Origin on both).
#[tokio::test]
async fn foreign_origin_upgrade_and_preflight_are_rejected() {
    let app = super::super::build_router(state(), true, true);
    let mut upgrade = get(
        "/api/auth/session-token?token=boundary-token",
        Some("127.0.0.1:9876"),
        Some("https://evil.example"),
        [127, 0, 0, 1],
    );
    upgrade
        .headers_mut()
        .insert(header::UPGRADE, HeaderValue::from_static("websocket"));
    upgrade
        .headers_mut()
        .insert(header::CONNECTION, HeaderValue::from_static("Upgrade"));
    assert_eq!(
        app.clone().oneshot(upgrade).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    let preflight = Request::builder()
        .method("OPTIONS")
        .uri("/sessions/x/write")
        .header(header::HOST, "127.0.0.1:9876")
        .header(header::ORIGIN, "https://evil.example")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4000))))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(preflight).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        !response
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
}
