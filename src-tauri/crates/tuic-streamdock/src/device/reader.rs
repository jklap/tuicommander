//! Button-state polling task.
//!
//! `mirajazz::state::DeviceStateReader` already owns the whole "poll with a
//! timeout, diff against last-known state, emit Down/Up edges" loop —
//! including synthesizing a Down+Up pair for any device below protocol v3
//! that only ever reports a single "pressed" state (see
//! `DeviceStateReader::input_to_updates` in the vendored crate). That is
//! also the mechanism covering the M18's own release-only-firmware risk:
//! we do not need to guess about it here, mirajazz's diffing layer already
//! degrades correctly for a pv2 device like ours. This module is therefore
//! just the poll loop plus translating `DeviceStateUpdate` into our own
//! `InputEvent`, which speaks `slot`, not the reader's raw index (which
//! *is* already `slot` here, per the model's `key_count`-many boolean
//! vector — the `hw` byte never crosses this boundary at all).

use std::sync::Arc;
use std::time::Duration;

use mirajazz::device::Device;
use mirajazz::state::DeviceStateUpdate;
use tokio::sync::mpsc;

use crate::device::model::{DeviceModel, process_input_fn};

/// How long a single poll-read blocks before returning "no data" and
/// looping again. 200ms keeps input latency well under the coordinator's
/// own 250ms tick without hammering the device with reads.
const READ_TIMEOUT: Duration = Duration::from_millis(200);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum InputEvent {
    Down(u8),
    Up(u8),
}

/// Spawns the reader task. Runs until the device stops responding
/// (`DeviceStateReader::read` returning `Err`), at which point it sends
/// nothing further and simply exits — the actor's own heartbeat failure is
/// what actually marks the device unhealthy; this task exiting quietly is
/// just a consequence of the same underlying disconnect.
pub fn spawn(model: &'static DeviceModel, device: Arc<Device>, tx: mpsc::Sender<InputEvent>) {
    let process_input = process_input_fn(model);
    tokio::spawn(async move {
        let reader = device.get_reader(process_input);
        loop {
            match reader.read(Some(READ_TIMEOUT)).await {
                Ok(updates) => {
                    for update in updates {
                        let event = match update {
                            DeviceStateUpdate::ButtonDown(slot) => Some(InputEvent::Down(slot)),
                            DeviceStateUpdate::ButtonUp(slot) => Some(InputEvent::Up(slot)),
                            // The M18 has no encoders; ignore anything that
                            // would only make sense on a device that does.
                            DeviceStateUpdate::EncoderDown(_)
                            | DeviceStateUpdate::EncoderUp(_)
                            | DeviceStateUpdate::EncoderTwist(_, _) => None,
                        };
                        if let Some(event) = event
                            && tx.send(event).await.is_err()
                        {
                            return; // coordinator side dropped — nothing left to report to.
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "streamdock: reader loop ending, device stopped responding: {e}"
                    );
                    return;
                }
            }
        }
    });
}
