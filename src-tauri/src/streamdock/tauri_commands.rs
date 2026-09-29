//! Tauri IPC twins of `commands.rs`'s HTTP routes — see AGENTS.md's IPC/HTTP
//! parity rule. Both return the exact same serialized shape as their HTTP
//! counterpart so the frontend store code is transport-agnostic.

use std::sync::Arc;

use tauri::State;

use super::StreamDockStatus;
use super::commands::StreamDockDeviceInfo;
use crate::AppState;

#[tauri::command]
pub(crate) fn streamdock_status(state: State<'_, Arc<AppState>>) -> StreamDockStatus {
    state.streamdock.status()
}

#[tauri::command]
pub(crate) async fn streamdock_list_devices() -> Vec<StreamDockDeviceInfo> {
    let devices = tuic_streamdock::device::hotplug::list_connected()
        .await
        .unwrap_or_default();
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
        .collect()
}
