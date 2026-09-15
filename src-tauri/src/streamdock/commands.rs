//! HTTP routes for StreamDock status/device-listing. Full IPC/HTTP parity —
//! see `tauri_commands.rs` for the twins these share their return types
//! with, per AGENTS.md's IPC/HTTP parity rule.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use crate::AppState;

use super::StreamDockStatus;

pub(crate) async fn get_status(State(state): State<Arc<AppState>>) -> Json<StreamDockStatus> {
    Json(state.streamdock.status())
}

#[derive(serde::Serialize)]
pub(crate) struct StreamDockDeviceInfo {
    pub(crate) product_name: &'static str,
    pub(crate) serial_number: String,
    pub(crate) vendor_id: u16,
    pub(crate) product_id: u16,
}

/// Enumerate currently-connected StreamDock-family devices, for the
/// Settings UI's device picker (`notification_sound.rs`'s
/// `list_audio_output_devices` is the "enumerate and pick" precedent this
/// mirrors). Independent of whether the integration is currently enabled —
/// listing devices should work even while deciding whether to turn it on.
pub(crate) async fn list_devices() -> Json<Vec<StreamDockDeviceInfo>> {
    let devices = tuic_streamdock::device::hotplug::list_connected()
        .await
        .unwrap_or_default();
    Json(
        devices
            .into_iter()
            .filter_map(|d| {
                let model = tuic_streamdock::device::hotplug::model_for(&d)?;
                Some(StreamDockDeviceInfo {
                    product_name: model.product_name,
                    serial_number: d.serial_number.clone().unwrap_or_default(),
                    vendor_id: d.vendor_id,
                    product_id: d.product_id,
                })
            })
            .collect(),
    )
}
