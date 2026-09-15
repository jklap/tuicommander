//! Phase 0 hardware spike. Five escalating, independently-informative
//! checks against a real StreamDock M18 — see the build plan's "Transport"
//! section for the full rationale. **Run this before writing anything that
//! depends on step 3 having passed.**
//!
//! Preconditions this binary cannot check for you: quit the Mirabox
//! Creator app first (it claims the HID handle exclusively — a connect
//! failure with the device physically present and enumerable is almost
//! always this), and make sure no other process using `libtransport.dylib`
//! is running.
//!
//! ```text
//! cargo run -p tuic-streamdock --example probe
//! ```

use std::time::Duration;

use mirajazz::device::{Device, DeviceQuery, list_devices};
use tuic_streamdock::device::model::{
    M18, STREAMDOCK_USAGE_ID, STREAMDOCK_USAGE_PAGE, process_input_fn,
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    println!("== StreamDock M18 hardware spike ==");
    println!(
        "Preconditions: Mirabox Creator app quit, no other libtransport-based process running.\n"
    );

    // --- Step 1: enumerate ---------------------------------------------
    println!("[1/5] Enumerating HID devices matching M18's known VID/PIDs...");
    let queries: Vec<DeviceQuery> = M18
        .usb_ids
        .iter()
        .map(|&(vid, pid)| DeviceQuery::new(STREAMDOCK_USAGE_PAGE, STREAMDOCK_USAGE_ID, vid, pid))
        .collect();
    let devices = match list_devices(&queries).await {
        Ok(d) => d,
        Err(e) => {
            eprintln!("    enumerate() failed: {e}");
            std::process::exit(1);
        }
    };
    if devices.is_empty() {
        eprintln!(
            "    No matching device found. This is an interface/enumeration problem, not a protocol one."
        );
        eprintln!("    Checked VID:PID pairs: {:?}", M18.usb_ids);
        std::process::exit(1);
    }
    for d in &devices {
        println!(
            "    found: vid=0x{:04X} pid=0x{:04X} serial={:?} manufacturer={:?} name={:?} usage_page=0x{:04X} usage_id=0x{:02X}",
            d.vendor_id,
            d.product_id,
            d.serial_number,
            d.manufacturer,
            d.name,
            d.usage_page,
            d.usage_id
        );
    }
    let dev_info = devices.into_iter().next().unwrap();
    println!("    OK — proceeding with the first match.\n");

    // --- Step 2: open + firmware string ---------------------------------
    println!(
        "[2/5] Reading firmware version (the single highest-information byte in this project)..."
    );
    let firmware = match Device::read_firmware_version(&dev_info).await {
        Ok(fw) => fw,
        Err(e) => {
            eprintln!("    read_firmware_version failed: {e}");
            eprintln!("    If this is a permission/open error with the device physically present,");
            eprintln!(
                "    the Mirabox Creator app (or another process) is very likely holding the HID handle."
            );
            std::process::exit(1);
        }
    };
    println!("    firmware = {firmware:?}");
    let features = tuic_streamdock::device::model::FeatureSet::from_firmware_string(
        firmware.as_deref().unwrap_or(""),
    );
    println!("    negotiated features: {features:?}\n");

    // --- Connect (needed for steps 3-5) ---------------------------------
    let device = match Device::connect(
        &dev_info,
        M18.protocol_version,
        M18.key_count,
        M18.encoder_count,
    )
    .await
    {
        Ok(d) => d,
        Err(e) => {
            eprintln!("    Device::connect failed: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "    connected. vid=0x{:04X} pid=0x{:04X} serial={} protocol_version reported by mirajazz internally.\n",
        device.vid,
        device.pid,
        device.serial_number()
    );

    // --- Step 3: brightness — THE GO/NO-GO ------------------------------
    // A 100 -> 10 -> 60 sweep is too subtle to notice by eye — the panel's
    // own backlight floor makes a "10%" step barely different from "60%".
    // A full-off / full-on strobe is unmissable instead, and still proves
    // exactly the same thing: if `LIG` moves the backlight at all, CRT
    // framing on this unit matches.
    println!("[3/5] Backlight STROBE — THIS IS THE GO/NO-GO CHECK.");
    println!("    Watch the panel. It should visibly flash between fully OFF and fully ON");
    println!("    four times over the next ~6 seconds. If every write below reports \"sent OK\"");
    println!("    but you see no flashing at all, THAT is the real NO-GO signal — a successful");
    println!("    write with no visible effect means the panel isn't reacting to LIG, not that");
    println!("    the eye just missed a subtle dim.\n");
    let mut go_no_go_failed = false;
    for cycle in 1..=4 {
        for (label, pct) in [("OFF", 0u8), ("ON", 100u8)] {
            print!("    [{cycle}/4] set_brightness({pct}) -> panel should be {label} now... ");
            match device.set_brightness(pct).await {
                Ok(()) => println!("sent OK"),
                Err(e) => {
                    println!("FAILED: {e}");
                    go_no_go_failed = true;
                }
            }
            tokio::time::sleep(Duration::from_millis(700)).await;
        }
    }
    let _ = device.set_brightness(70).await; // leave it at a normal level, not fully off, when this exits
    if go_no_go_failed {
        eprintln!(
            "\n    GO/NO-GO: FAILED. CRT framing on this VSD unit may differ from Companion's M18V3."
        );
        eprintln!(
            "    Do not proceed to Phase 1 — follow the three-tier oracle plan in the design doc:"
        );
        eprintln!(
            "    (1) USB-capture the vendor Python SDK's hello-world once and diff the prefix bytes;"
        );
        eprintln!(
            "    (2) nm/strings the vendor libtransport.dylib around its known CRT opcode strings;"
        );
        eprintln!(
            "    (3) diff field-by-field against companion-surface-mirabox-stream-dock's M18V3.ts."
        );
        std::process::exit(1);
    }
    println!(
        "    Did the backlight visibly flash 4 times? If yes: GO. If every write said \"sent OK\""
    );
    println!("    but nothing visibly flashed: NO-GO — see the oracle plan in the design doc.\n");

    // --- Step 4: one image on wire key id 0x01 and 0x0B -------------------
    println!("[4/5] Writing one 64x64 test image via write_image(0) [wire key id 0x01]");
    println!("      and write_image(10) [wire key id 0x0B], to see which physical key each hits.");
    let _ = device.clear_all_button_images().await;
    let test_jpeg = sample_jpeg();
    // NOTE: these are raw arguments straight to mirajazz's Device (this
    // spike bypasses device::model entirely) — mirajazz sends `arg + 1` as
    // the wire key id, so this writes to wire key id 0x01 and 0x0B. DO NOT
    // assume which physical key that is: empirically (2026-09-15, two
    // rounds of hardware testing — see device::model::KeyDef's doc comment)
    // wire key id 0x01 lands on the physical BOTTOM-left key and 0x0B on
    // the physical TOP-left key — the opposite of what an earlier version
    // of this comment assumed, and independent of `hw` (the byte a *press*
    // reports), which uses yet another numbering for the same physical
    // keys. If you need "slot 0 means true top-left", go through
    // `device::model::M18.write_key_of_slot`, not a raw literal like this.
    if let Err(e) = device.write_image(0, &test_jpeg).await {
        eprintln!("    write_image(0) failed: {e}");
    }
    if let Err(e) = device.write_image(10, &test_jpeg).await {
        eprintln!("    write_image(10) failed: {e}");
    }
    if let Err(e) = device.flush().await {
        eprintln!("    flush failed: {e}");
    }
    println!("    Check the panel: two keys should now show the test image (both writes must");
    println!("    succeed for the CRT framing go/no-go to be meaningful). Which physical key each");
    println!("    landed on is informational, not pass/fail — see this step's own comment above");
    println!("    for the confirmed mapping on Boss's unit; a different unit may differ.\n");

    // --- Step 5: read one press, then keep reading through a HOLD --------
    // The first version of this step broke out of the loop the instant a
    // single read() call returned any event, which meant it could never
    // actually observe repeat behavior during a hold — a tap and a 2-second
    // hold looked identical to it, because it never issued a *second*
    // read() call to find out. Fixed: once the first event arrives, keep
    // reading and timestamping for a fixed observation window, so a real
    // hold shows up as multiple Down/Up pairs spaced ~200ms apart (mirajazz's
    // poll-driven synthesis for a pv2 device like this one), while a tap
    // shows exactly one pair and then silence for the rest of the window.
    const HOLD_OBSERVATION_WINDOW: Duration = Duration::from_secs(3);

    println!(
        "[5/5] PRESS AND HOLD any key for about 2 seconds, then release. Waiting up to 15s for the first event..."
    );
    let reader = device.get_reader(process_input_fn(&M18));
    let first_event_deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let mut first_event_at: Option<tokio::time::Instant> = None;
    let mut events: Vec<(Duration, mirajazz::state::DeviceStateUpdate)> = Vec::new();

    loop {
        let now = tokio::time::Instant::now();
        let deadline = match first_event_at {
            Some(t0) => t0 + HOLD_OBSERVATION_WINDOW,
            None => first_event_deadline,
        };
        if now >= deadline {
            break;
        }
        match reader.read(Some(Duration::from_millis(200))).await {
            Ok(updates) => {
                for u in updates {
                    let t0 = *first_event_at.get_or_insert(now);
                    let elapsed = now.saturating_duration_since(t0);
                    println!("    t+{:>5}ms  decoded: {u:?}", elapsed.as_millis());
                    events.push((elapsed, u));
                }
            }
            Err(e) => {
                eprintln!("    read failed: {e}");
                break;
            }
        }
    }

    if events.is_empty() {
        println!("    No press decoded within 15s. Either nothing was pressed, or the hw byte");
        println!("    this unit reports isn't in device::model::M18_KEYS — check with a raw HID");
        println!("    sniff if this repeats.");
    } else {
        println!(
            "    OK — {} event(s) decoded over a {:.1}s observation window starting at the first press.",
            events.len(),
            HOLD_OBSERVATION_WINDOW.as_secs_f32()
        );
        println!("    Confirm by eye: did the printed slot(s) match the physical key you pressed?");
        println!();
        if events.len() <= 2 {
            println!(
                "    RESULT: a SINGLE Down+Up pair, even though you held the key. This confirms"
            );
            println!("    dispatch.rs's assumption: this device does NOT repeat while held, so");
            println!("    GestureResolver's renewal-based hold heuristic will NEVER fire — every");
            println!("    press looks identical to a tap no matter how long it's physically held.");
            println!(
                "    dispatch.rs needs a different hold mechanism (e.g. a fixed timer started"
            );
            println!(
                "    on the single Down that fires Hold if no matching pattern re-arrives, or"
            );
            println!("    dropping true \"hold\" as a distinguishable gesture on this hardware).");
        } else {
            println!(
                "    RESULT: MULTIPLE Down/Up pairs while held (~{}ms apart on average) — this",
                {
                    let deltas: Vec<u128> = events
                        .windows(2)
                        .map(|w| w[1].0.as_millis().saturating_sub(w[0].0.as_millis()))
                        .collect();
                    if deltas.is_empty() {
                        0
                    } else {
                        deltas.iter().sum::<u128>() / deltas.len() as u128
                    }
                }
            );
            println!(
                "    matches dispatch.rs's renewal-based hold heuristic as designed — no change needed."
            );
        }
    }

    let _ = device.clear_all_button_images().await;
    let _ = device.shutdown().await;
}

/// A tiny valid 64x64 JPEG for step 4 — solid mid-gray, generated with the
/// crate's own render pipeline rather than hand-rolling JPEG bytes, so this
/// binary has no separate image-encoding path to get subtly wrong.
fn sample_jpeg() -> Vec<u8> {
    use tuic_streamdock::render::text::FontFace;
    use tuic_streamdock::render::{FaceState, Glyph, KeyFace, Label};
    let font = FontFace::bundled();
    let face = KeyFace {
        state: FaceState::Working,
        glyph: Glyph::Dot,
        primary: Label::from_str_truncated("TEST"),
        secondary: Label::from_str_truncated("probe"),
        badge: None,
    };
    tuic_streamdock::render::draw::render_face_jpeg(&font, &face, M18.key_px as u32, 90)
}
