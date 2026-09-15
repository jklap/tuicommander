//! Phase 1 standalone demo: connects to the first discovered StreamDock M18,
//! fills all 18 keys with their slot number on a distinct color (confirming
//! the slot<->hw map in both directions when pressed), and logs every
//! resolved gesture. No TUICommander app involvement — this is exactly the
//! "iterate without costing Boss a restart" binary the crate boundary
//! exists to make possible.
//!
//! ```text
//! cargo run -p tuic-streamdock --bin streamdock-demo
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use mirajazz::device::Device;
use tokio::sync::mpsc;
use tuic_streamdock::device::model::M18;
use tuic_streamdock::device::{actor, hotplug, reader};
use tuic_streamdock::dispatch::GestureResolver;
use tuic_streamdock::render::draw::render_face_jpeg;
use tuic_streamdock::render::text::FontFace;
use tuic_streamdock::render::{FaceState, Glyph, KeyFace, Label};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    println!("Looking for a StreamDock M18...");
    let devices = hotplug::list_connected().await.expect("enumeration failed");
    let Some(dev_info) = devices.into_iter().next() else {
        eprintln!(
            "No StreamDock device found. Is it plugged in, and is the Mirabox Creator app quit?"
        );
        std::process::exit(1);
    };
    let model = hotplug::model_for(&dev_info).unwrap_or(&M18);

    println!("Connecting to {} ...", model.product_name);
    let device = Device::connect(
        &dev_info,
        model.protocol_version,
        model.key_count,
        model.encoder_count,
    )
    .await
    .expect("connect failed");
    let device = Arc::new(device);

    let handle = actor::spawn(model, device.clone());
    let (input_tx, mut input_rx) = mpsc::channel(64);
    reader::spawn(model, device.clone(), input_tx);

    println!(
        "Filling all {} keys with their slot number...",
        model.key_count
    );
    let font = FontFace::bundled();
    let _ = handle.send(actor::DeviceMsg::ClearAll).await;
    for key in model.keys {
        let state = [
            FaceState::Idle,
            FaceState::Working,
            FaceState::NeedsInput,
            FaceState::CompletedUnread,
            FaceState::Error,
        ][key.slot as usize % 5];
        let face = KeyFace {
            state,
            glyph: Glyph::Dot,
            primary: Label::from_str_truncated(&format!("slot {}", key.slot)),
            secondary: Label::from_str_truncated(&format!("hw 0x{:02X}", key.hw)),
            badge: None,
        };
        let jpeg: Arc<[u8]> = render_face_jpeg(&font, &face, model.key_px as u32, 90).into();
        if key.kind == tuic_streamdock::device::KeyKind::Lcd
            && handle
                .send(actor::DeviceMsg::SetKeyImage {
                    slot: key.slot,
                    jpeg,
                })
                .await
                .is_err()
        {
            eprintln!("device actor stopped while filling keys");
            return;
        }
    }
    let _ = handle.send(actor::DeviceMsg::Commit).await;

    println!("Ready. Press any key on the panel (Ctrl+C to quit).");
    println!("Watch for: does the printed slot match the key you physically pressed?");
    println!("Every physical press now prints TWO Down/Up pairs: one at press, one at release —");
    println!("  the gap between them (Δ+Nms on the second pair) is real hold duration.");
    println!(
        "  A quick or slow single tap -> resolves to Tap (once its double-tap window passes)."
    );
    println!("  A deliberate double-tap -> resolves to DoubleTap on the second release.");
    println!(
        "  Holding well past ~600ms before releasing -> resolves to Hold, immediately on release.\n"
    );

    let mut gestures = GestureResolver::new();
    let mut health = handle.health.clone();
    // Per-slot timestamp of the previous raw event, purely for the
    // diagnostic `Δ+Nms` printed below — lets a real press-bounce gap (or a
    // genuine double-tap gap) be read directly off the terminal instead of
    // guessed at. Not used by GestureResolver itself.
    let mut last_event_at: std::collections::HashMap<u8, Instant> =
        std::collections::HashMap::new();

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("shutting down...");
                let _ = handle.send(actor::DeviceMsg::ClearAll).await;
                handle.shutdown().await;
                return;
            }
            _ = health.changed() => {
                if *health.borrow() == actor::DeviceHealth::Unhealthy {
                    eprintln!("device reported unhealthy (write timeout or lost connection) — exiting");
                    return;
                }
            }
            event = input_rx.recv() => {
                let Some(event) = event else { break };
                let now = Instant::now();
                let slot = match event {
                    tuic_streamdock::device::InputEvent::Down(s) | tuic_streamdock::device::InputEvent::Up(s) => s,
                };
                let delta = last_event_at.insert(slot, now).map(|prev| now.duration_since(prev).as_millis());
                match delta {
                    Some(ms) => println!("raw event: {event:?}  (Δ+{ms}ms since this slot's last event)"),
                    None => println!("raw event: {event:?}  (first event seen for this slot)"),
                }
                if let Some((slot, gesture)) = gestures.on_event(event, now) {
                    println!("  -> resolved immediately: slot={slot} gesture={gesture:?}");
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(250)) => {
                for (slot, gesture) in gestures.tick(Instant::now()) {
                    println!("  -> resolved on tick: slot={slot} gesture={gesture:?}");
                }
            }
        }
    }
}
