//! In-process supervisor for the StreamDock M18 macropad integration.
//!
//! Owns lifecycle only — device I/O, rendering, and gesture resolution all
//! live in the `tuic-streamdock` crate behind its `StateSource`/`ActionSink`
//! port boundary (`tuic_streamdock::port`). This module's entire job is
//! "spawn/stop the coordinator loop when config says to," modeled on
//! `tunnels/manager.rs`'s start-race discipline (a `Starting`-shaped guard
//! around the single owned task).
//!
//! `#[cfg(feature = "desktop")]`-only: `tuic_streamdock` is a desktop-only
//! optional dependency (its HID/rasterizer deps have no reason to weigh on
//! `tuic-remote`'s build), so this whole module — and every call into it —
//! is compiled out of that profile. `config::StreamDockConfig` stays
//! available in both profiles (it deliberately doesn't reference any
//! `tuic_streamdock` type), so a `tuic-remote` build still round-trips the
//! setting even though it can never act on it.

pub(crate) mod commands;
mod sink;
mod source;
pub(crate) mod tauri_commands;

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{Mutex as AsyncMutex, oneshot};

use tuic_streamdock::device::{actor, hotplug, reader};
use tuic_streamdock::port::{Doorbell, StateSource};

use crate::AppState;

use sink::AppStateSink;
use source::AppStateSource;

#[derive(Clone, Debug, Default, serde::Serialize)]
pub(crate) struct StreamDockStatus {
    pub(crate) enabled: bool,
    pub(crate) running: bool,
    pub(crate) device: Option<String>,
    pub(crate) last_error: Option<String>,
    pub(crate) restarts: u32,
}

struct RunningTask {
    handle: tokio::task::JoinHandle<()>,
    shutdown: oneshot::Sender<()>,
}

pub(crate) struct StreamDockManager {
    task: AsyncMutex<Option<RunningTask>>,
    status: Arc<parking_lot::RwLock<StreamDockStatus>>,
}

impl StreamDockManager {
    pub(crate) fn new() -> Self {
        Self {
            task: AsyncMutex::new(None),
            status: Arc::new(parking_lot::RwLock::new(StreamDockStatus::default())),
        }
    }

    pub(crate) fn status(&self) -> StreamDockStatus {
        self.status.read().clone()
    }

    /// Idempotent reconcile: start the supervisor task if config says
    /// enabled and it isn't running; stop it if config says disabled and it
    /// is. Call this both at app startup (next to
    /// `global_hotkey::restore_from_config`) and after any config save
    /// where `ConfigSaveEffects::streamdock_changed` is set — "app starts"
    /// and "the user flips the toggle" are deliberately the same code path.
    pub(crate) async fn apply_config(&self, state: &Arc<AppState>) {
        let cfg = state.config.read().streamdock.clone();
        let mut task = self.task.lock().await;

        if cfg.enabled && task.is_none() {
            self.status.write().enabled = true;
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            let handle = tokio::spawn(run_supervisor(
                Arc::clone(state),
                shutdown_rx,
                Arc::clone(&self.status),
            ));
            *task = Some(RunningTask {
                handle,
                shutdown: shutdown_tx,
            });
        } else if !cfg.enabled && task.is_some() {
            if let Some(running) = task.take() {
                let _ = running.shutdown.send(());
                let _ = running.handle.await;
            }
            let mut status = self.status.write();
            status.enabled = false;
            status.running = false;
            status.device = None;
        }
        // Brightness/pinning changes while already running are picked up
        // live by the supervisor loop re-reading config each reconnect and
        // each tick — see `run_one_device`'s doc comment. No restart needed.
    }

    /// Stop the supervisor task, if running. Called on app shutdown.
    pub(crate) async fn shutdown(&self) {
        let mut task = self.task.lock().await;
        if let Some(running) = task.take() {
            let _ = running.shutdown.send(());
            let _ = running.handle.await;
        }
    }
}

impl Default for StreamDockManager {
    fn default() -> Self {
        Self::new()
    }
}

/// The long-lived task started by `apply_config`. Runs until `shutdown`
/// fires: waits for a matching device via hot-plug discovery, then runs
/// `run_one_device` until it exits (device unhealthy, or shutdown), then
/// loops back to waiting — so unplug/replug recovers on its own.
async fn run_supervisor(
    state: Arc<AppState>,
    mut shutdown: oneshot::Receiver<()>,
    status: Arc<parking_lot::RwLock<StreamDockStatus>>,
) {
    use futures_util::StreamExt;

    // Anything already connected at startup, so we don't wait for a fresh
    // hot-plug event just because the device was plugged in before we
    // started watching.
    let mut pending: std::collections::VecDeque<mirajazz::types::HidDeviceInfo> =
        match hotplug::list_connected().await {
            Ok(devices) => devices.into_iter().map(|d| (*d).clone()).collect(),
            Err(e) => {
                status.write().last_error = Some(format!("enumerate failed: {e}"));
                Default::default()
            }
        };

    let mut watcher = hotplug::HotplugWatcher::new();
    let events = match watcher.events().await {
        Ok(s) => s,
        Err(e) => {
            status.write().last_error = Some(format!("hotplug watch failed: {e}"));
            return;
        }
    };
    tokio::pin!(events);

    loop {
        let dev_info = if let Some(d) = pending.pop_front() {
            d
        } else {
            tokio::select! {
                _ = &mut shutdown => return,
                ev = events.next() => match ev {
                    Some(mirajazz::types::DeviceLifecycleEvent::Connected(info)) => info,
                    Some(mirajazz::types::DeviceLifecycleEvent::Disconnected(_)) => continue,
                    None => return,
                },
            }
        };

        let Some(model) = hotplug::model_for(&dev_info) else {
            continue; // not a device we have a table for
        };

        match mirajazz::device::Device::connect(
            &dev_info,
            model.protocol_version,
            model.key_count,
            model.encoder_count,
        )
        .await
        {
            Ok(device) => {
                {
                    let mut s = status.write();
                    s.running = true;
                    s.device = Some(model.product_name.to_string());
                    s.last_error = None;
                }
                run_one_device(&state, model, device, &mut shutdown, &status).await;
                {
                    let mut s = status.write();
                    s.running = false;
                    s.device = None;
                    s.restarts += 1;
                }
                if shutdown.try_recv() != Err(oneshot::error::TryRecvError::Empty) {
                    return; // shutdown fired (or its sender dropped) while we were running
                }
            }
            Err(e) => {
                status.write().last_error = Some(format!(
                    "connect failed ({e}) — is the Mirabox Creator app running? it holds the device exclusively"
                ));
            }
        }
    }
}

/// Runs one connected device's actor + reader + coordinator tick loop until
/// it becomes unhealthy or `shutdown` fires. Re-reads `state.config`'s
/// `streamdock` block once at the start (brightness, pinned sessions) —
/// picking up a config change made while disconnected requires nothing
/// more than the next reconnect, which hot-plug polling already provides
/// on its own 2s cadence if the device is still physically present.
async fn run_one_device(
    state: &Arc<AppState>,
    model: &'static tuic_streamdock::device::model::DeviceModel,
    device: mirajazz::device::Device,
    shutdown: &mut oneshot::Receiver<()>,
    status: &Arc<parking_lot::RwLock<StreamDockStatus>>,
) {
    use futures_util::StreamExt;

    let device = Arc::new(device);
    let handle = actor::spawn(model, Arc::clone(&device));
    let (input_tx, mut input_rx) = tokio::sync::mpsc::channel(64);
    reader::spawn(model, Arc::clone(&device), input_tx);

    let cfg = state.config.read().streamdock.clone();
    let _ = handle
        .send(actor::DeviceMsg::ScreenBrightness(cfg.screen_brightness))
        .await;
    if model_supports_rgb(model, &device).await {
        let _ = handle
            .send(actor::DeviceMsg::LedBrightness(cfg.led_brightness))
            .await;
    }

    let source = AppStateSource {
        state: Arc::clone(state),
    };
    let sink = AppStateSink {
        state: Arc::clone(state),
    };
    let mut coordinator = tuic_streamdock::Coordinator::new(15, model.key_px as u32, 90);
    coordinator.set_pinned(cfg.pinned_sessions.iter().cloned());

    let mut dirty = true;
    let mut health = handle.health.clone();
    let doorbell = source.subscribe();
    tokio::pin!(doorbell);
    let mut tick = tokio::time::interval(tuic_streamdock::coordinator::TICK_PERIOD);

    loop {
        tokio::select! {
            _ = &mut *shutdown => {
                let _ = handle.send(actor::DeviceMsg::ClearAll).await;
                handle.shutdown().await;
                return;
            }
            _ = health.changed() => {
                if *health.borrow() == actor::DeviceHealth::Unhealthy {
                    status.write().last_error = Some(
                        "device stopped responding (write timeout or lost connection)".to_string(),
                    );
                    return; // supervisor treats this exactly like a detach
                }
            }
            db = doorbell.next() => match db {
                Some(Doorbell::Dirty) => dirty = true,
                Some(Doorbell::Lagged { .. }) => dirty = true,
                None => return, // event bus gone — AppState is shutting down
            },
            ev = input_rx.recv() => match ev {
                Some(event) => coordinator.on_input(&sink, event, Instant::now()),
                None => return, // reader task ended — device gone
            },
            _ = tick.tick() => {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                coordinator.tick(&source, &handle, &mut dirty, now_ms).await;
                coordinator.tick_gestures(&sink, Instant::now());
            }
        }
    }
}

/// Whether to bother sending an LED command at all — gated on the firmware
/// string, not the model table (`FeatureSet` — see `device::model`'s doc
/// comment on why this can't be inferred from the model alone). Best-effort:
/// a firmware read failure just skips LED setup rather than failing startup.
async fn model_supports_rgb(
    _model: &'static tuic_streamdock::device::model::DeviceModel,
    device: &mirajazz::device::Device,
) -> bool {
    device
        .firmware_version
        .as_deref()
        .map(tuic_streamdock::device::model::FeatureSet::from_firmware_string)
        .unwrap_or_default()
        .rgb
}
