use std::sync::Arc;

use crate::AppState;

#[tauri::command]
pub(crate) async fn start_design_mode(
    state: tauri::State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<serde_json::Value, String> {
    super::start(state.inner(), session_id).await
}

#[tauri::command]
pub(crate) async fn stop_design_mode(
    state: tauri::State<'_, Arc<AppState>>,
    repo_path: String,
) -> Result<serde_json::Value, String> {
    super::stop(state.inner(), &repo_path).await
}

#[tauri::command]
pub(crate) async fn get_design_mode_status(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<serde_json::Value>, String> {
    Ok(super::statuses(state.inner()).await)
}
