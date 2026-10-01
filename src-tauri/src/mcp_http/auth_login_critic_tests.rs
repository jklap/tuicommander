//! Adversarial tests for the in-app login (story 1359-57b4), written by the critic.
//! Each test names the plausible bug it catches.

use super::*;
use base64::Engine;
use tower::ServiceExt;

const IP: [u8; 4] = [198, 51, 100, 7];

fn state_with(max_failures: u32, duration_secs: u64) -> Arc<AppState> {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    {
        let mut config = state.config.write();
        config.services.auth.username = "boss".to_string();
        config.services.auth.password_hash = bcrypt::hash("co:rrect", 4).unwrap();
        config.services.auth.auth_rate_limit_max = max_failures;
        config.services.auth.auth_rate_limit_window_secs = 300;
        config.services.auth.session_token_duration_secs = duration_secs;
    }
    state
}

fn app(state: &Arc<AppState>) -> axum::Router {
    axum::Router::new()
        .route("/auth/login", axum::routing::post(login_handler))
        .route("/mobile/login", axum::routing::get(|| async { "form" }))
        .route("/mobile", axum::routing::get(|| async { "app" }))
        .route("/mobile/session/a", axum::routing::get(|| async { "app" }))
        .layer(axum::middleware::from_fn_with_state(
            Arc::clone(state),
            basic_auth_middleware,
        ))
        .with_state(Arc::clone(state))
}

fn post_from(ip: [u8; 4]) -> axum::http::request::Builder {
    Request::post("/auth/login")
        .header(header::HOST, "tuic.test:9876")
        .header(header::ORIGIN, "http://tuic.test:9876")
        .header(header::CONTENT_TYPE, "application/json")
        .extension(ConnectInfo(SocketAddr::from((ip, 40000))))
}

fn body(user: &str, pass: &str) -> axum::body::Body {
    serde_json::json!({ "username": user, "password": pass })
        .to_string()
        .into()
}

async fn send(app: &axum::Router, req: Request<axum::body::Body>) -> Response {
    app.clone().oneshot(req).await.unwrap()
}

fn nav(path: &str) -> axum::http::request::Builder {
    Request::get(path)
        .header(header::ACCEPT, "text/html")
        .extension(ConnectInfo(SocketAddr::from((IP, 40000))))
}

fn empty() -> axum::body::Body {
    axum::body::Body::empty()
}

/// Plausible bug: `login_redirect` encodes only `uri.path()`, so a deep link with
/// a query (the share target opens `/mobile?shared=<key>`) loses it at login.
#[tokio::test]
async fn redirect_to_login_keeps_the_query_of_the_deep_link() {
    let app = app(&state_with(5, 3600));
    let response = send(&app, nav("/mobile?shared=abc").body(empty()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::FOUND);
    let location = response.headers().get(header::LOCATION).unwrap().to_str().unwrap();
    assert!(location.contains("shared%3Dabc"), "query dropped: {location}");
}

/// Plausible bug: the per-IP budget keyed on a client-supplied header, so a
/// rotating `X-Forwarded-For` mints a fresh budget per guess.
#[tokio::test]
async fn forwarded_for_header_does_not_open_a_second_budget() {
    let app = app(&state_with(2, 3600));
    for (i, guess) in ["a", "b"].iter().enumerate() {
        let mut req = post_from(IP).body(body("boss", guess)).unwrap();
        req.headers_mut()
            .insert("x-forwarded-for", format!("10.9.9.{i}").parse().unwrap());
        assert_eq!(send(&app, req).await.status(), StatusCode::UNAUTHORIZED);
    }
    let mut req = post_from(IP).body(body("boss", "co:rrect")).unwrap();
    req.headers_mut()
        .insert("x-forwarded-for", "10.9.9.200".parse().unwrap());
    req.headers_mut().insert("x-real-ip", "10.9.9.201".parse().unwrap());
    assert_eq!(send(&app, req).await.status(), StatusCode::TOO_MANY_REQUESTS);
}

/// Plausible bug: admission counted after bcrypt finishes, so a burst of
/// concurrent guesses all get verified before the budget closes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_guesses_never_exceed_the_budget() {
    let app = app(&state_with(3, 3600));
    let mut set = tokio::task::JoinSet::new();
    for i in 0..12 {
        let app = app.clone();
        set.spawn(async move {
            let req = post_from(IP).body(body("boss", &format!("wrong-{i}"))).unwrap();
            app.oneshot(req).await.unwrap().status()
        });
    }
    let mut verified = 0;
    let mut limited = 0;
    while let Some(status) = set.join_next().await {
        match status.unwrap() {
            StatusCode::UNAUTHORIZED => verified += 1,
            StatusCode::TOO_MANY_REQUESTS => limited += 1,
            other => panic!("unexpected {other}"),
        }
    }
    assert_eq!(verified, 3, "verified {verified}, limited {limited}");
    assert_eq!(limited, 9);
}

/// Plausible bug: cookie flags dropped on the login path (no HttpOnly, no
/// SameSite=Strict, or no Secure under TLS).
#[tokio::test]
async fn login_cookie_flags_plain_and_tls() {
    let app = app(&state_with(5, 3600));
    let plain = send(&app, post_from(IP).body(body("boss", "co:rrect")).unwrap()).await;
    assert_eq!(plain.status(), StatusCode::OK);
    let cookie = plain.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().to_string();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"), "{cookie}");
    assert!(cookie.contains("Path=/"), "{cookie}");
    assert!(!cookie.contains("Secure"), "plain HTTP must not set Secure: {cookie}");

    let tls = send(
        &app,
        post_from(IP)
            .extension(axum_server_dual_protocol::Protocol::Tls)
            .body(body("boss", "co:rrect"))
            .unwrap(),
    )
    .await;
    let cookie = tls.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(cookie.contains("Secure"), "{cookie}");
}

/// Plausible bug: sliding renewal uses a different (larger) lifetime than the
/// configured one, or `0` (session cookie) gets a Max-Age.
#[tokio::test]
async fn sliding_cookie_never_exceeds_the_configured_lifetime() {
    let state = state_with(5, 3600);
    let app = app(&state);
    let login = send(&app, post_from(IP).body(body("boss", "co:rrect")).unwrap()).await;
    let pair = login.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap()
        .split(';').next().unwrap().to_string();
    for _ in 0..3 {
        let hit = send(&app, nav("/mobile").header(header::COOKIE, pair.clone()).body(empty()).unwrap()).await;
        let renewed = hit.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap();
        assert!(renewed.contains("Max-Age=3600"), "{renewed}");
    }
    state.config.write().services.auth.session_token_duration_secs = 0;
    let hit = send(&app, nav("/mobile").header(header::COOKIE, pair).body(empty()).unwrap()).await;
    assert!(!hit.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().contains("Max-Age"));
}

/// Plausible bug: Sec-Fetch-Site accepted for `same-site`/`none`, or an Origin
/// whose port differs from Host accepted (a sibling-port page on the same host).
#[tokio::test]
async fn only_exact_same_origin_signals_are_accepted() {
    let state = state_with(5, 3600);
    let app = app(&state);
    for site in ["same-site", "none", "cross-site"] {
        let mut req = post_from(IP).body(body("boss", "co:rrect")).unwrap();
        req.headers_mut().insert("sec-fetch-site", site.parse().unwrap());
        assert_eq!(send(&app, req).await.status(), StatusCode::FORBIDDEN, "{site}");
    }
    let mut other_port = post_from(IP).body(body("boss", "co:rrect")).unwrap();
    other_port.headers_mut().insert(header::ORIGIN, "http://tuic.test:9999".parse().unwrap());
    assert_eq!(send(&app, other_port).await.status(), StatusCode::FORBIDDEN);
    let mut null_origin = post_from(IP).body(body("boss", "co:rrect")).unwrap();
    null_origin.headers_mut().insert(header::ORIGIN, "null".parse().unwrap());
    assert_eq!(send(&app, null_origin).await.status(), StatusCode::FORBIDDEN);
    assert!(state.auth_rate_limits.is_empty(), "refused POSTs must not spend budget");
}

/// Plausible bug: Content-Type compared as an exact string, so the charset
/// parameter browsers/fetch libraries add is refused; or a `text/plain`
/// simple-request body is accepted (CORS-preflight-free CSRF).
#[tokio::test]
async fn content_type_parameters_ok_but_simple_request_types_refused() {
    let app = app(&state_with(5, 3600));
    let mut ok = post_from(IP).body(body("boss", "co:rrect")).unwrap();
    ok.headers_mut().insert(header::CONTENT_TYPE, "application/json; charset=utf-8".parse().unwrap());
    assert_eq!(send(&app, ok).await.status(), StatusCode::OK);
    for ct in ["text/plain", "application/jsonx", "multipart/form-data"] {
        let mut req = post_from(IP).body(body("boss", "co:rrect")).unwrap();
        req.headers_mut().insert(header::CONTENT_TYPE, ct.parse().unwrap());
        assert_eq!(send(&app, req).await.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE, "{ct}");
    }
}

/// Plausible bug: the Basic header is built as `user:pass` and split at the first
/// colon, so a password that contains `:` can never log in via the form.
/// (The fixture password is `co:rrect`.)
#[tokio::test]
async fn password_containing_a_colon_logs_in() {
    let app = app(&state_with(5, 3600));
    let response = send(&app, post_from(IP).body(body("boss", "co:rrect")).unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// Plausible bug: hostile `next` forms that start with `/mobile` but leave the app
/// (userinfo `@`, sibling host label, NEL/tab control characters, huge value).
#[test]
fn safe_next_rejects_prefix_lookalikes() {
    let long = format!("/mobile/{}", "a".repeat(3000));
    for hostile in [
        "/mobile@evil.test",
        "/mobile.evil.test",
        "/mobilex",
        "/mobile/\u{85}x",
        "/mobile/\tx",
        long.as_str(),
    ] {
        assert_eq!(safe_next(Some(hostile)), "/mobile", "{hostile:?}");
    }
}

/// Plausible bug: after a token rotation the stale cookie gets the 401 dead end
/// again instead of the login redirect; or a wrong Basic header on a navigation is
/// redirected (hiding the challenge and the budget it spends).
#[tokio::test]
async fn stale_cookie_redirects_but_wrong_basic_keeps_the_challenge() {
    let app = app(&state_with(5, 3600));
    let stale = send(
        &app,
        nav("/mobile/session/a").header(header::COOKIE, "tui-session=old-token").body(empty()).unwrap(),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::FOUND);

    let wrong = base64::engine::general_purpose::STANDARD.encode("boss:nope");
    let basic = send(
        &app,
        nav("/mobile").header(header::AUTHORIZATION, format!("Basic {wrong}")).body(empty()).unwrap(),
    )
    .await;
    assert_eq!(basic.status(), StatusCode::UNAUTHORIZED);
    assert!(basic.headers().contains_key(header::WWW_AUTHENTICATE));
}

/// Plausible bug: a non-string `next`, or an empty credential pair, panics or
/// is treated as a success.
#[tokio::test]
async fn degenerate_login_bodies_never_succeed() {
    let app = app(&state_with(5, 3600));
    let bad_next = r#"{"username":"boss","password":"co:rrect","next":5}"#;
    assert_eq!(
        send(&app, post_from(IP).body(bad_next.into()).unwrap()).await.status(),
        StatusCode::BAD_REQUEST
    );
    let empty_pair = send(&app, post_from(IP).body(body("", "")).unwrap()).await;
    assert_eq!(empty_pair.status(), StatusCode::UNAUTHORIZED);
    assert!(!empty_pair.headers().contains_key(header::SET_COOKIE));
}
