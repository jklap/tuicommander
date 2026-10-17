//! Legacy desktop transport adapters retain owner signatures and publish only after success.
use crate::{AppState, config};
use std::sync::Arc;

#[tauri::command]
pub(crate) fn save_notification_config(
    state: tauri::State<'_, Arc<AppState>>,
    base: config::NotificationConfig,
    config: config::NotificationConfig,
) -> Result<(), String> {
    config::save_notification_config(base, config)?;
    super::api::publish_change(&state, "notifications", true);
    Ok(())
}

#[tauri::command]
pub(crate) fn save_ui_prefs(
    state: tauri::State<'_, Arc<AppState>>,
    base: config::UIPrefsConfig,
    config: config::UIPrefsConfig,
) -> Result<(), String> {
    config::save_ui_prefs(base, config)?;
    super::api::publish_change(&state, "preferences", true);
    Ok(())
}

#[tauri::command]
pub(crate) fn save_repo_settings(
    state: tauri::State<'_, Arc<AppState>>,
    base: config::RepoSettingsMap,
    config: config::RepoSettingsMap,
) -> Result<(), String> {
    config::save_repo_settings(base, config)?;
    super::api::publish_change(&state, "repositories", true);
    Ok(())
}

#[tauri::command]
pub(crate) fn save_repo_defaults(
    state: tauri::State<'_, Arc<AppState>>,
    base: config::RepoDefaultsConfig,
    config: config::RepoDefaultsConfig,
) -> Result<(), String> {
    config::save_repo_defaults(base, config)?;
    super::api::publish_change(&state, "repo_defaults", true);
    Ok(())
}

#[tauri::command]
pub(crate) fn save_prompt_library(
    state: tauri::State<'_, Arc<AppState>>,
    base: config::PromptLibraryConfig,
    config: config::PromptLibraryConfig,
) -> Result<(), String> {
    config::save_prompt_library(base, config)?;
    super::api::publish_change(&state, "prompts", true);
    Ok(())
}

#[tauri::command]
pub(crate) fn save_agents_config(
    state: tauri::State<'_, Arc<AppState>>,
    base: config::AgentsConfig,
    config: config::AgentsConfig,
) -> Result<(), String> {
    config::save_agents_config(base, config)?;
    super::api::publish_change(&state, "agents", true);
    Ok(())
}

#[tauri::command]
pub(crate) fn save_keybindings(
    state: tauri::State<'_, Arc<AppState>>,
    base: serde_json::Value,
    config: serde_json::Value,
) -> Result<(), String> {
    config::save_keybindings(base, config)?;
    super::api::publish_change(&state, "keybindings", true);
    Ok(())
}

#[tauri::command]
pub(crate) fn save_pane_layout(
    state: tauri::State<'_, Arc<AppState>>,
    base: serde_json::Value,
    layout: serde_json::Value,
) -> Result<(), String> {
    config::save_pane_layout(base, layout)?;
    super::api::publish_change(&state, "pane_layout", true);
    Ok(())
}
