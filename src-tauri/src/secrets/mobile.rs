//! A per-form origin prevents an app-origin service worker from observing entry.
#[cfg(feature = "desktop")]
pub(super) async fn start(
    state: &std::sync::Arc<crate::AppState>,
    nonce: &str,
) -> Result<(String, tokio::task::JoinHandle<()>), String> {
    use axum::{
        Router,
        routing::{get, post},
    };
    let tls = state.secrets.tls.read().clone();
    let fqdn = match &*state.tailscale_state.read() {
        crate::tailscale::TailscaleState::Running {
            fqdn,
            https_enabled: true,
        } if tls.is_some() => Some(fqdn.clone()),
        _ => None,
    };
    // Never send a password over plaintext LAN. Without an existing TLS host,
    // the dedicated HTTP origin is accessible only from this machine.
    let bind = if fqdn.is_some() {
        "0.0.0.0:0"
    } else {
        "127.0.0.1:0"
    };
    let listener =
        std::net::TcpListener::bind(bind).map_err(|_| "Could not bind private form server")?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "Could not configure private form server")?;
    let port = listener
        .local_addr()
        .map_err(|_| "Could not locate private form server")?
        .port();
    let mut app = Router::new()
        .route("/secrets/forms/{nonce}", get(super::forms::form_http))
        .route("/secrets/forms/submit", post(super::forms::submit_http));
    app = app.route(
        "/{*path}",
        get(crate::mcp_http::static_files::serve_secret_static),
    );
    let app =
        app.with_state(state.clone())
            .layer(axum::middleware::map_response(
                |response: axum::response::Response| async move {
                    super::forms::private_response(response)
                },
            ));
    let (base, task) = if let (Some(tls), Some(host)) = (tls, fqdn) {
        let task = tokio::spawn(async move {
            if let Ok(server) = axum_server::from_tcp_rustls(listener, tls) {
                let _ = server.serve(app.into_make_service()).await;
            }
        });
        (format!("https://{host}:{port}"), task)
    } else {
        let listener = tokio::net::TcpListener::from_std(listener)
            .map_err(|_| "Could not start private form server")?;
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://127.0.0.1:{port}"), task)
    };
    Ok((format!("{base}/secret-form.html#nonce={nonce}"), task))
}
