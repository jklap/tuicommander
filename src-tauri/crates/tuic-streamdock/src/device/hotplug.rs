//! Device discovery and hot-plug watching.
//!
//! macOS gives no attach/detach callback for this device class (vendor SDK
//! note: hot-plug detection there is a 2s poll, full stop). `async-hid`'s
//! `HidBackend::watch()` — which `mirajazz::device::DeviceWatcher` wraps —
//! may or may not do better than that internally; either way this module
//! doesn't need to know, because `DeviceWatcher` already gives us a single
//! `Stream<DeviceLifecycleEvent>` abstraction over whatever the platform can
//! actually offer.

use futures_lite::Stream;
use mirajazz::device::{DeviceQuery, DeviceWatcher, list_devices};
use mirajazz::types::{DeviceLifecycleEvent, HidDevice, HidDeviceInfo};

use crate::device::model::{DeviceModel, MODELS, STREAMDOCK_USAGE_ID, STREAMDOCK_USAGE_PAGE};

/// One `DeviceQuery` per (model, usb_id) pair across every model this crate
/// knows about — the union `list_devices`/`DeviceWatcher::watch` filter
/// against.
pub fn all_queries() -> Vec<DeviceQuery> {
    MODELS
        .iter()
        .flat_map(|m| {
            m.usb_ids.iter().map(|&(vid, pid)| {
                DeviceQuery::new(STREAMDOCK_USAGE_PAGE, STREAMDOCK_USAGE_ID, vid, pid)
            })
        })
        .collect()
}

/// Which registered model a discovered device matches, if any. Takes
/// `&HidDeviceInfo` but works for a `&HidDevice` argument too via deref
/// coercion (`HidDevice: Deref<Target = HidDeviceInfo>`) — both
/// `list_connected`'s `HidDevice`s and a `DeviceLifecycleEvent`'s
/// `HidDeviceInfo` resolve through this one function.
pub fn model_for(info: &HidDeviceInfo) -> Option<&'static DeviceModel> {
    MODELS
        .iter()
        .copied()
        .find(|m| m.matches_usb_id(info.vendor_id, info.product_id))
}

/// `list_devices` returns the richer `HidDevice` (an opened-handle-capable
/// wrapper around `HidDeviceInfo`, per `mirajazz`'s own `types.rs`
/// aliases), not a plain `HidDeviceInfo` — the caller typically wants to
/// pass this straight into `Device::connect`, which itself only needs a
/// `&HidDeviceInfo` and gets it via the same deref coercion.
pub async fn list_connected() -> Result<Vec<HidDevice>, mirajazz::error::MirajazzError> {
    let queries = all_queries();
    Ok(list_devices(&queries).await?.into_iter().collect())
}

/// Owns the underlying `DeviceWatcher`. `mirajazz::device::DeviceWatcher`
/// can only be `watch()`ed **once** per instance and its stream borrows
/// `&mut self` for that instance's lifetime (see its own doc comment) — so
/// this type exists to give that borrow somewhere to live across a loop,
/// rather than trying to return an owned/`'static` stream, which the
/// underlying crate's API does not offer.
pub struct HotplugWatcher {
    watcher: DeviceWatcher,
    /// Owned here, not built fresh per call: `DeviceWatcher::watch`'s
    /// returned stream borrows its `queries` argument for the stream's
    /// whole lifetime, so the slice has to live at least as long as `self`
    /// does, not just as long as one `events()` call's stack frame.
    queries: Vec<DeviceQuery>,
}

impl HotplugWatcher {
    pub fn new() -> Self {
        Self {
            watcher: DeviceWatcher::new(),
            queries: all_queries(),
        }
    }

    /// Starts watching. Must be called exactly once (mirrors
    /// `DeviceWatcher::watch`'s own one-shot contract) and borrows `self`
    /// for as long as the returned stream is used — the caller keeps this
    /// `HotplugWatcher` alive in the same scope as the stream, e.g. inside
    /// one `tokio::select!` loop, rather than passing the stream elsewhere.
    pub async fn events(
        &mut self,
    ) -> Result<impl Stream<Item = DeviceLifecycleEvent> + '_, mirajazz::error::MirajazzError> {
        self.watcher.watch(&self.queries).await
    }
}

impl Default for HotplugWatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_cover_every_registered_usb_id() {
        let queries = all_queries();
        let total_ids: usize = MODELS.iter().map(|m| m.usb_ids.len()).sum();
        assert_eq!(queries.len(), total_ids);
    }
}
