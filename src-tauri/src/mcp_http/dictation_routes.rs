//! HTTP mirrors of the `dictation::commands` Tauri commands.
//!
//! Every handler answers exactly what its IPC twin resolves to — a bare string
//! for `inject_text`, `null` for the `Result<(), String>` commands — because the
//! same store code (`src/stores/dictation.ts`) reads both transports.
//!
//! Desktop-only: `crate::dictation` is gated on the `desktop` feature, so the
//! routes are registered in the desktop half of `build_router`.

use crate::AppState;
use crate::dictation::{self, DictationState};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::Manager;

use super::json_result;

pub(super) async fn get_dictation_status_http(State(state): State<Arc<AppState>>) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::get_dictation_status(dictation))
}

pub(super) async fn get_model_info_http() -> impl IntoResponse {
    Json(dictation::commands::get_model_info())
}

#[derive(serde::Deserialize)]
pub(super) struct ModelNameRequest {
    pub model: String,
}

pub(super) async fn download_whisper_model_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ModelNameRequest>,
) -> Response {
    let app_handle = state.app_handle.read().clone();
    let Some(app) = app_handle else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    json_result(dictation::commands::download_whisper_model(app, body.model).await)
}

pub(super) async fn delete_whisper_model_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ModelNameRequest>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::delete_whisper_model(
        dictation, body.model,
    ))
}

pub(super) async fn start_dictation_http(State(state): State<Arc<AppState>>) -> Response {
    let app_handle = state.app_handle.read().clone();
    let Some(app) = app_handle else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::start_dictation(app.clone(), dictation))
}

pub(super) async fn stop_dictation_http(State(state): State<Arc<AppState>>) -> Response {
    let app_handle = state.app_handle.read().clone();
    let Some(app) = app_handle else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    json_result(dictation::commands::stop_dictation_and_transcribe(app).await)
}

pub(super) async fn get_correction_map_http(State(state): State<Arc<AppState>>) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    Json(dictation::commands::get_correction_map(dictation)).into_response()
}

#[derive(serde::Deserialize)]
pub(super) struct CorrectionMapRequest {
    pub map: HashMap<String, String>,
}

pub(super) async fn set_correction_map_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CorrectionMapRequest>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::set_correction_map(dictation, body.map))
}

pub(super) async fn list_audio_devices_http() -> impl IntoResponse {
    Json(dictation::commands::list_audio_devices())
}

#[derive(serde::Deserialize)]
pub(super) struct InjectTextRequest {
    pub text: String,
}

pub(super) async fn inject_text_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<InjectTextRequest>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::inject_text(dictation, body.text))
}

pub(super) async fn get_dictation_config_http() -> impl IntoResponse {
    Json(dictation::commands::get_dictation_config())
}

pub(super) async fn set_dictation_config_http(
    Json(config): Json<dictation::commands::DictationConfig>,
) -> Response {
    json_result(dictation::commands::set_dictation_config(config))
}
