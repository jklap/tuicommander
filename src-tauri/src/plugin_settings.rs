//! Plugin lifecycle integration and transport entry point; schemas and transactions live in settings.
use crate::{
    AppState,
    plugins::PluginManifest,
    settings::{
        plugins::{PluginAction, PluginStore},
        service::Store,
    },
};
use serde_json::Value;

pub(crate) fn prepare_settings(
    manifest: &PluginManifest,
    state: Option<&AppState>,
) -> Result<Option<PluginStore>, String> {
    let definition = crate::settings::plugin_migrations::for_plugin(&manifest.id)
        .or_else(|| manifest.settings.clone());
    let path = crate::config::config_dir()
        .join("plugins")
        .join(&manifest.id)
        .join("data/config.json");
    let Some(definition) = definition else {
        if path.exists() {
            return Err("Configurable plugin requires a settings schema".into());
        }
        return Ok(None);
    };
    let id = manifest.id.clone();
    let event_bus = state.map(|s| s.event_bus.clone());
    #[cfg(feature = "desktop")]
    let app = state.and_then(|s| s.app_handle.read().clone());
    let notify = std::sync::Arc::new(move || {
        let plugin_ids = vec![id.clone()];
        #[cfg(feature = "desktop")]
        if let Some(app) = &app {
            use tauri::Emitter;
            let _ = app.emit("plugin-changed", plugin_ids.clone());
        }
        if let Some(bus) = &event_bus {
            let _ = bus.send(crate::state::AppEvent::PluginChanged { plugin_ids });
        }
    });
    PluginStore::new(&manifest.id, definition, path, notify)
        .map(Some)
        .map_err(|_| "Plugin settings schema is invalid".into())
}

/// Raw plugin consumption retains credentials; settings reads use the redacting service instead.
pub(crate) fn read_config(plugin_id: &str) -> Result<Option<String>, String> {
    let manifest = crate::plugins::read_single_manifest(plugin_id)?;
    let store = prepare_settings(&manifest, None)?
        .ok_or("Configurable plugin requires a settings schema")?;
    let value = store
        .read()
        .map_err(|_| "Plugin settings read failed".to_owned())?;
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|_| "Plugin settings read failed".into())
}

pub(crate) fn write_config(plugin_id: &str, content: &str) -> Result<(), String> {
    if content.len() > 262_144 {
        return Err("Plugin settings payload is too large".into());
    }
    let manifest = crate::plugins::read_single_manifest(plugin_id)?;
    let store = prepare_settings(&manifest, None)?
        .ok_or("Configurable plugin requires a settings schema")?;
    let value: Value =
        serde_json::from_str(content).map_err(|_| "Plugin settings JSON is invalid")?;
    store
        .replace(value)
        .map_err(|_| "Plugin settings write rejected".into())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn plugin_settings_action(
    plugin_id: String,
    action: PluginAction,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<Value, crate::settings::request::Failure> {
    plugin_settings_action_impl(&state, plugin_id, action).await
}

pub(crate) async fn plugin_settings_action_impl(
    state: &AppState,
    plugin_id: String,
    action: PluginAction,
) -> Result<Value, crate::settings::request::Failure> {
    state.plugin_settings.action(&plugin_id, action).await
}
