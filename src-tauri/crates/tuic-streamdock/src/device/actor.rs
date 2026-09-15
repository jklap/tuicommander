//! `DeviceActor`: the only task that writes to the device.
//!
//! `mirajazz::device::Device` already serializes its own reader/writer
//! internally (`Arc<Mutex<DeviceReader/Writer>>`), so this is not strictly
//! the *only* mechanism preventing interleaved writes — but it is still the
//! single logical choke point for: batching an image write with the
//! `flush()` that commits it, bounding a stalled write so it can never
//! starve the mandatory 10s heartbeat (`keep_alive`), and giving shutdown a
//! deterministic order (per the vendor SDK: stop heartbeat -> stop reader ->
//! clear+disconnect -> drop).

use std::sync::Arc;
use std::time::Duration;

use mirajazz::device::Device;
use tokio::sync::{mpsc, oneshot, watch};

use crate::device::model::DeviceModel;

/// Every write to the device goes through a `tokio::time::timeout` at this
/// bound. A stalled `write_output_report` (a wedged USB endpoint, a
/// SIGSTOP'd... well, there's no child process here, but the analogous
/// case is a device that stopped acking) must not be allowed to delay the
/// heartbeat indefinitely — see the risk section in the design plan.
const WRITE_TIMEOUT: Duration = Duration::from_millis(500);

/// Bounded so a stalled device can't make this queue grow without limit.
/// Drop-oldest-per-slot semantics live in the coordinator (which only ever
/// enqueues a slot's *latest* intended face), not here — the actor itself
/// just refuses to block forever on a full queue.
const QUEUE_CAPACITY: usize = 32;

#[derive(Debug)]
pub enum DeviceMsg {
    SetKeyImage {
        slot: u8,
        jpeg: Arc<[u8]>,
    },
    ClearKey {
        slot: u8,
    },
    ClearAll,
    ScreenBrightness(u8),
    LedBrightness(u8),
    SetLedColors(Vec<[u8; 3]>),
    /// Commits any buffered `write_image` calls (`Device::flush`, which
    /// sends each buffered image then the `STP` opcode).
    Commit,
    Shutdown(oneshot::Sender<()>),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DeviceHealth {
    Healthy,
    /// A write timed out or the reader loop ended — the actor has stopped
    /// itself. The coordinator should treat this exactly like a detach: drop
    /// the handle and let hot-plug discovery reattach when the device (or a
    /// working state of it) reappears.
    Unhealthy,
}

pub struct DeviceHandle {
    tx: mpsc::Sender<DeviceMsg>,
    pub model: &'static DeviceModel,
    pub health: watch::Receiver<DeviceHealth>,
}

impl DeviceHandle {
    pub async fn send(&self, msg: DeviceMsg) -> Result<(), String> {
        self.tx
            .send(msg)
            .await
            .map_err(|_| "device actor has stopped".to_string())
    }

    pub async fn shutdown(&self) {
        let (done_tx, done_rx) = oneshot::channel();
        if self.tx.send(DeviceMsg::Shutdown(done_tx)).await.is_ok() {
            let _ = tokio::time::timeout(Duration::from_secs(2), done_rx).await;
        }
    }
}

/// Spawns the actor task. Does not spawn the reader task — see
/// `device::reader::spawn`, called separately by whoever owns the
/// `Device`, since the reader needs its own `Arc<Device>` clone and outlives
/// this function's local scope.
pub fn spawn(model: &'static DeviceModel, device: Arc<Device>) -> DeviceHandle {
    let (tx, mut rx) = mpsc::channel(QUEUE_CAPACITY);
    let (health_tx, health_rx) = watch::channel(DeviceHealth::Healthy);
    let heartbeat_period = Duration::from_secs(model.heartbeat_secs);

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(heartbeat_period);
        ticker.tick().await; // first tick fires immediately; skip it, the loop below acts before it matters

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if timeout_call(device.keep_alive()).await.is_err() {
                        tracing::warn!("streamdock: heartbeat write timed out or failed; marking device unhealthy");
                        let _ = health_tx.send(DeviceHealth::Unhealthy);
                        break;
                    }
                }
                msg = rx.recv() => {
                    match msg {
                        None => break,
                        Some(DeviceMsg::Shutdown(done)) => {
                            // Ordered per the vendor SDK: clear, then a
                            // "disconnect"-equivalent, before dropping.
                            // mirajazz's `shutdown()` already sends the
                            // clear+disconnect opcodes in that order.
                            let _ = timeout_call(device.clear_all_button_images()).await;
                            let _ = timeout_call(device.shutdown()).await;
                            let _ = done.send(());
                            break;
                        }
                        Some(other) => {
                            if apply(&device, model, other).await.is_err() {
                                tracing::warn!("streamdock: device write timed out; marking device unhealthy");
                                let _ = health_tx.send(DeviceHealth::Unhealthy);
                                break;
                            }
                        }
                    }
                }
            }
        }
    });

    DeviceHandle {
        tx,
        model,
        health: health_rx,
    }
}

async fn timeout_call<
    F: std::future::Future<Output = Result<(), mirajazz::error::MirajazzError>>,
>(
    fut: F,
) -> Result<(), ()> {
    match tokio::time::timeout(WRITE_TIMEOUT, fut).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => {
            tracing::warn!("streamdock: device call failed: {e}");
            Err(())
        }
        Err(_) => Err(()), // timed out
    }
}

/// Translates a `slot` (true physical visual order — what every message
/// variant carries) to the argument `mirajazz` needs for
/// `write_image`/`clear_button_image`, per `DeviceModel::write_key_of_slot`
/// — **never** pass a raw `slot` to those calls directly, the two numbering
/// spaces differ on this firmware (see `device::model::KeyDef`'s doc
/// comment). An out-of-range slot (should be unreachable — the coordinator
/// only ever emits slots the model itself produced) falls back to the slot
/// value unchanged rather than silently dropping the write.
fn write_key_of(model: &DeviceModel, slot: u8) -> u8 {
    model.write_key_of_slot(slot).unwrap_or_else(|| {
        tracing::warn!(
            "streamdock: slot {slot} has no write_key mapping in {}; using slot as-is",
            model.product_name
        );
        slot
    })
}

async fn apply(device: &Device, model: &DeviceModel, msg: DeviceMsg) -> Result<(), ()> {
    match msg {
        DeviceMsg::SetKeyImage { slot, jpeg } => {
            timeout_call(device.write_image(write_key_of(model, slot), &jpeg)).await
        }
        DeviceMsg::ClearKey { slot } => {
            timeout_call(device.clear_button_image(write_key_of(model, slot))).await
        }
        DeviceMsg::ClearAll => timeout_call(device.clear_all_button_images()).await,
        DeviceMsg::ScreenBrightness(pct) => timeout_call(device.set_brightness(pct)).await,
        DeviceMsg::LedBrightness(pct) => timeout_call(device.set_led_brightness(pct)).await,
        DeviceMsg::SetLedColors(colors) => timeout_call(device.set_led_colors(&colors)).await,
        DeviceMsg::Commit => timeout_call(device.flush()).await,
        DeviceMsg::Shutdown(_) => unreachable!("Shutdown is handled before calling apply"),
    }
}
