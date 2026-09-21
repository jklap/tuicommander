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

/// Named `asset` rather than `id` so the wire key says what it identifies —
/// the IPC twin's parameter is `asset` too, and the two must match.
#[derive(serde::Deserialize)]
pub(super) struct SpeechAssetRequest {
    pub asset: String,
}

pub(super) async fn get_speech_assets_http(State(state): State<Arc<AppState>>) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    Json(dictation::commands::get_speech_assets(dictation)).into_response()
}

pub(super) async fn download_speech_asset_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SpeechAssetRequest>,
) -> Response {
    let app_handle = state.app_handle.read().clone();
    let Some(app) = app_handle else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    json_result(dictation::commands::download_speech_asset(app, body.asset).await)
}

pub(super) async fn cancel_speech_download_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SpeechAssetRequest>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::cancel_speech_download(
        dictation, body.asset,
    ))
}

pub(super) async fn delete_speech_asset_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SpeechAssetRequest>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::delete_speech_asset(
        dictation, body.asset,
    ))
}

/// `turn` is optional on the wire and on IPC: omitting it means "the turn that
/// is current now", which is what a caller answering immediately wants.
#[derive(serde::Deserialize)]
pub(super) struct SpeakRequest {
    pub text: String,
    #[serde(default)]
    pub turn: Option<u64>,
}

#[derive(serde::Deserialize)]
pub(super) struct SpeechStatusQuery {
    #[serde(default)]
    pub utterance: Option<String>,
}

pub(super) async fn speak_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SpeakRequest>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::speak_reply(
        dictation, body.text, body.turn,
    ))
}

pub(super) async fn stop_speech_http(State(state): State<Arc<AppState>>) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::stop_speech(dictation))
}

pub(super) async fn get_speech_status_http(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(query): axum::extract::Query<SpeechStatusQuery>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    Json(dictation::commands::get_speech_status(
        dictation,
        query.utterance,
    ))
    .into_response()
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

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ArmHandsFreeRequest {
    pub session_id: String,
    pub owner: String,
}

/// Browser/PWA counterpart of `arm_hands_free_dictation`.
///
/// The owner is the caller's own endpoint identity: a remote client that arms
/// here binds *itself*, so a later disconnect can disarm the mode it owns.
pub(super) async fn arm_hands_free_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ArmHandsFreeRequest>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::arm_hands_free(
        &state,
        &dictation,
        &body.session_id,
        &body.owner,
    ))
}

pub(super) async fn disarm_hands_free_http(State(state): State<Arc<AppState>>) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    Json(dictation::commands::disarm_hands_free(&state, &dictation)).into_response()
}

pub(super) async fn get_hands_free_status_http(State(state): State<Arc<AppState>>) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    Json(dictation::commands::hands_free_status(&dictation)).into_response()
}

pub(super) async fn get_dictation_config_http() -> impl IntoResponse {
    Json(dictation::commands::get_dictation_config())
}

pub(super) async fn set_dictation_config_http(
    State(state): State<Arc<AppState>>,
    Json(config): Json<dictation::commands::DictationConfig>,
) -> Response {
    // The state is passed when the app has it, so a language changed from a
    // browser cancels the reply being spoken on the desktop. Before startup
    // finishes there is no conversation to cancel, and the file is still
    // written — the setting is not lost, it simply has nothing live to affect.
    let app_handle = state.app_handle.read();
    let dictation = app_handle.as_ref().map(|app| app.state::<DictationState>());
    json_result(dictation::commands::save_dictation_config(
        config,
        dictation.as_deref(),
    ))
}
