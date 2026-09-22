use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StartBody {
    session_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StopBody {
    repo_path: String,
}

fn response(result: Result<serde_json::Value, String>) -> Response {
    match result {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": error })),
        )
            .into_response(),
    }
}

pub(crate) async fn start(
    State(state): State<Arc<AppState>>,
    Json(body): Json<StartBody>,
) -> Response {
    response(super::start(&state, body.session_id).await)
}

pub(crate) async fn stop(
    State(state): State<Arc<AppState>>,
    Json(body): Json<StopBody>,
) -> Response {
    response(super::stop(&state, &body.repo_path).await)
}

pub(crate) async fn statuses(State(state): State<Arc<AppState>>) -> Json<Vec<serde_json::Value>> {
    Json(super::statuses(&state).await)
}
