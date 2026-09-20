//! The HTTP half of the ego command-line surface, one route per `ego_*` command.
//!
//! Every handler calls the same `ego_cli` core the Tauri command calls, so the
//! desktop and a browser cannot get different answers about which model is the
//! default or which provider has a credential.
//!
//! Both routes take the spawn guard, and this is the one place the ego surface
//! differs from `/acp/*` on purpose. There, only `connect` and `reconnect` are
//! guarded, because only they start a process and every other route needs a
//! connection id one of those two handed out. Here **every** route starts a
//! process, and `config set` writes a person's ego configuration — so an
//! unauthenticated remote caller gets neither the read nor the write.

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use std::net::SocketAddr;

use super::guards::{Authenticated, require_local_or_auth};
use crate::AppState;
use crate::ego_cli::{self, EgoCliError, EgoCliErrorCode, EgoProviders};

/// Sub-router mounted at `/ego`.
pub(super) fn ego_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/providers", get(providers))
        .route("/providers/model", post(set_default_model))
}

/// The HTTP status that carries an ego CLI error code.
///
/// A translation, never a second opinion: the code in the body stays the truth.
/// A refusal by ego is 502 because the failing party is the program this host
/// ran, not the caller; output this host cannot read is 502 for the same
/// reason. "Not configured" is 409 rather than 404 — nothing is missing from
/// the URL, the installation is simply not in a state where the question has an
/// answer yet.
///
/// Exhaustive rather than defaulted, so the next code somebody adds is a
/// compile error here instead of silently inheriting a status nobody chose.
fn status_for(code: EgoCliErrorCode) -> StatusCode {
    match code {
        EgoCliErrorCode::NotConfigured => StatusCode::CONFLICT,
        EgoCliErrorCode::InvalidInput => StatusCode::BAD_REQUEST,
        EgoCliErrorCode::LaunchFailed => StatusCode::FAILED_DEPENDENCY,
        EgoCliErrorCode::CommandFailed | EgoCliErrorCode::UnreadableOutput => {
            StatusCode::BAD_GATEWAY
        }
    }
}

/// Serialize the result, with the error itself as the body on failure.
///
/// The body is the `EgoCliError` rather than this module's own shape, so a
/// browser reading `stderr` reads exactly what the desktop reads — including
/// the words ego printed, which is the whole point of the type.
fn answer(result: Result<EgoProviders, EgoCliError>) -> Response {
    match result {
        Ok(value) => Json(value).into_response(),
        Err(error) => (status_for(error.code), Json(error)).into_response(),
    }
}

#[derive(Deserialize)]
struct ProvidersQuery {
    /// Ask ego to re-enumerate its sources. Off unless asked: it is the only
    /// thing on this surface that reaches a provider over the network.
    refresh: Option<bool>,
}

#[derive(Deserialize)]
struct ModelBody {
    model: String,
}

async fn providers(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<axum::Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Query(query): Query<ProvidersQuery>,
) -> Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    answer(ego_cli::providers(&state, query.refresh.unwrap_or(false)).await)
}

async fn set_default_model(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<axum::Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<ModelBody>,
) -> Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    answer(ego_cli::set_default_model(&state, body.model).await)
}
