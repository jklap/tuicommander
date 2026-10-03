//! Private form bootstrap: only the native window receives the capability.
use super::*;
use crate::AppState;
use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use std::sync::Arc;

pub(crate) async fn form_http(
    State(state): State<Arc<AppState>>,
    Path(nonce): Path<String>,
) -> Response {
    let reply = match state.secrets.form(&nonce) {
        Ok(form) => Json(form).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Unknown or expired secret form"})),
        )
            .into_response(),
    };
    private_response(reply)
}

pub(crate) async fn submit_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Submission>,
) -> Response {
    let nonce = Zeroizing::new(body.nonce.clone());
    let id = state.secrets.form(&nonce).map(|f| f.id).unwrap_or_default();
    let reply = match state.secrets.submit(&nonce, body) {
        Ok(result) => {
            close_window(&state, &id);
            Json(result).into_response()
        }
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": error})),
        )
            .into_response(),
    };
    private_response(reply)
}

pub(super) fn private_response(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("no-referrer"),
    );
    response.headers_mut().insert(
        header::X_FRAME_OPTIONS,
        axum::http::HeaderValue::from_static("DENY"),
    );
    response.headers_mut().insert(header::CONTENT_SECURITY_POLICY, axum::http::HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; object-src 'none'; base-uri 'none'"));
    response
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn secret_form_bootstrap(
    state: tauri::State<'_, Arc<AppState>>,
    window: tauri::WebviewWindow,
) -> Result<Form, String> {
    let inner = state.secrets.inner.lock();
    let pending = inner
        .pending
        .as_ref()
        .ok_or("Unknown or expired secret form")?;
    if window.label() != format!("secret-{}", pending.form.id)
        || !state.secrets.windows.lock().contains(window.label())
    {
        return Err("Secret bootstrap requires the private native window".into());
    }
    Ok(pending.form.clone())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn secret_form_submit(
    state: tauri::State<'_, Arc<AppState>>,
    window: tauri::WebviewWindow,
    submission: Submission,
) -> Result<Status, String> {
    // Native caller identity cannot be supplied or spoofed in invoke arguments.
    {
        let inner = state.secrets.inner.lock();
        let pending = inner
            .pending
            .as_ref()
            .ok_or("Unknown or expired secret form")?;
        if window.label() != format!("secret-{}", pending.form.id)
            || !state.secrets.windows.lock().contains(window.label())
        {
            return Err("Secret submit requires the private native window".into());
        }
    }
    let nonce = Zeroizing::new(submission.nonce.clone());
    let id = state.secrets.form(&nonce)?.id;
    let result = state.secrets.submit(&nonce, submission)?;
    close_window(&state, &id);
    Ok(result)
}

pub(super) async fn ask(state: &Arc<AppState>, form: Form) -> Result<Status, String> {
    #[cfg(not(feature = "desktop"))]
    {
        let _ = (state, form);
        Err("Secret requests require a desktop host to open the private form".into())
    }
    #[cfg(feature = "desktop")]
    {
        let handle = state
            .app_handle
            .read()
            .clone()
            .ok_or("Secret entry requires a desktop host")?;
        let form = state.secrets.open(form)?;
        let (mobile_url, mobile_task) = match super::mobile::start(state, &form.nonce).await {
            Ok(server) => server,
            Err(error) => {
                cancel(state, &form.id);
                return Err(error);
            }
        };
        struct MobileLifetime(tokio::task::JoinHandle<()>);
        impl Drop for MobileLifetime {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let _mobile = MobileLifetime(mobile_task);
        if let Some(pending) = state.secrets.inner.lock().pending.as_mut() {
            pending.form.mobile_url = Some(mobile_url);
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        if let Some(pending) = state.secrets.inner.lock().pending.as_mut() {
            pending.response = Some(tx);
        }
        let label = format!("secret-{}", form.id);
        state.secrets.windows.lock().insert(label.clone());
        let built = tauri::WebviewWindowBuilder::new(
            &handle,
            &label,
            tauri::WebviewUrl::App("secret-form.html".into()),
        )
        .title("TUICommander — Private secret form")
        .inner_size(540.0, 620.0)
        .resizable(true)
        .devtools(false)
        .on_navigation(|url| {
            url.path() == "/secret-form.html"
                && ((url.scheme() == "tauri" && url.host_str() == Some("localhost"))
                    || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost"))
                    || (cfg!(debug_assertions)
                        && url.scheme() == "http"
                        && url.host_str() == Some("127.0.0.1")
                        && url.port() == Some(1421)))
        })
        .build();
        let window = match built {
            Ok(window) => window,
            Err(_) => {
                state.secrets.windows.lock().remove(&label);
                cancel(state, &form.id);
                return Err("Could not open private secret form".into());
            }
        };
        let callback_state = Arc::downgrade(state);
        let callback_label = label.clone();
        let id = form.id.clone();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Destroyed)
                && let Some(state) = callback_state.upgrade()
            {
                state.secrets.windows.lock().remove(&callback_label);
                cancel(&state, &id);
            }
        });
        // Cancel even if the MCP caller disconnects and drops its future.
        struct CancelOnDrop {
            state: Arc<AppState>,
            id: String,
        }
        impl Drop for CancelOnDrop {
            fn drop(&mut self) {
                cancel(&self.state, &self.id);
                close_window(&self.state, &self.id);
            }
        }
        let _guard = CancelOnDrop {
            state: state.clone(),
            id: form.id,
        };
        let _ = window.set_focus();
        match tokio::time::timeout(std::time::Duration::from_secs(300), rx).await {
            Ok(Ok(status)) => Ok(status),
            _ => Err("Secret form expired or was closed".into()),
        }
    }
}

#[cfg(feature = "desktop")]
fn cancel(state: &Arc<AppState>, id: &str) {
    let mut inner = state.secrets.inner.lock();
    if inner.pending.as_ref().is_some_and(|p| p.form.id == id)
        && let Some(mut pending) = inner.pending.take()
    {
        let result = Status {
            names: pending.form.fields.iter().map(|f| f.name.clone()).collect(),
            status: "declined".into(),
        };
        if let Some(tx) = pending.response.take() {
            let _ = tx.send(result);
        }
    }
}

fn close_window(state: &Arc<AppState>, id: &str) {
    #[cfg(feature = "desktop")]
    {
        use tauri::Manager;
        if let Some(handle) = state.app_handle.read().clone() {
            if let Some(window) = handle.get_webview_window(&format!("secret-{id}")) {
                let _ = window.destroy();
            }
        }
    }
    #[cfg(not(feature = "desktop"))]
    let _ = (state, id);
}
