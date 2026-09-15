//! The StreamDock device table.
//!
//! Everything model-specific lives here as **data**, following the schema
//! `bitfocus/companion-surface-mirabox-stream-dock` uses for its own model
//! files (`src/models/M18V3.ts` et al.). Adding a second Mirabox/Ajazz deck
//! should be a new `DeviceModel` const and nothing else — no new match arms
//! anywhere in `device/actor.rs`, `device/reader.rs`, or `coordinator.rs`.
//!
//! The M18-specific constants here are transcribed from the vendor SDK
//! (`StreamDock-Device-SDK/CPP-SDK/src/HotspotDevice/StreamDockM18/streamdockM18.cpp`):
//! PIDs at :4-11, geometry + key map at :51-76, firmware feature negotiation
//! at :100-137. `mirajazz::Device` already implements the wire framing those
//! constants describe (CRT opcodes, image chunking, ACK/OK response framing) —
//! this module supplies only the identity, geometry, the
//! hardware-byte -> logical-slot decode `mirajazz::Device::get_reader` requires
//! us to provide, and (empirically, not from the SDK — see `KeyDef`'s doc
//! comment) the logical-slot -> write-key translation `device/actor.rs`
//! needs before addressing `Device::write_image`.

use mirajazz::{error::MirajazzError, types::DeviceInput};

/// One HID vendor/product id pair this model answers to.
///
/// The M18 ships under three: the plain Mirabox PID, the VSD-branded PID
/// (our unit), and the M18E variant. All three share the same geometry and
/// protocol per the vendor SDK, so one `DeviceModel` covers all three ids.
pub type UsbId = (u16, u16);

/// Vendor-defined HID usage page StreamDock devices report under.
/// Not a keyboard/mouse usage, which is why macOS never raises an Input
/// Monitoring / TCC prompt for this device.
pub const STREAMDOCK_USAGE_PAGE: u16 = 0xFFA0;
pub const STREAMDOCK_USAGE_ID: u16 = 0x01;

/// Where a key sits, in the vocabulary `mirajazz::Device` and
/// `DeviceStateReader` use: a flat `key` index into a `Vec<bool>` sized
/// `key_count`. The M18's plain push-buttons are wired into the *same* index
/// space as the LCD keys (there is no separate `encoder_count` concept for
/// them), so `KeyKind` only distinguishes them for the render/policy layers
/// above — it carries no meaning down at the transport layer.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum KeyKind {
    /// One of the 15 LCD keys, addressable by `Device::write_image`.
    Lcd,
    /// One of the 3 plain push buttons — reports presses, has no display.
    Button,
}

/// One key's identity: its position in **visual reading order** (`slot`,
/// what everything above `device/` speaks in — render, policy, and
/// coordinator code assume `slot 0` is the true physical top-left key and
/// never see `hw` or `write_key` at all) plus two **independent** raw wire
/// numbers, confirmed empirically to differ from each other on this
/// firmware (see `M18_KEYS`'s doc comment for the evidence):
///
/// - `hw`: the byte a button-press *report* carries, used only by
///   `process_input`'s hw-byte -> slot lookup.
/// - `write_key`: the argument to pass to `Device::write_image`/
///   `clear_button_image` (mirajazz sends `write_key + 1` as the wire key
///   id) to address this physical key's LCD.
///
/// On an "ordinary" device these might well be the same number, which is
/// what this crate originally assumed — `write_key` defaulting to `slot`
/// with no separate field. Boss's VSD-branded unit disproved that: writing
/// wire key id `0x0B` (i.e. `write_key = 10`) lights the physical top-left
/// key, while that same key's *read* hw byte is `0x01` — two different
/// values for the same physical position, depending on direction.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct KeyDef {
    /// Position in **true physical visual order** (0 = top-left, reading
    /// left-to-right then top-to-bottom) — the only field anything outside
    /// `device/` ever sees or reasons about.
    pub slot: u8,
    /// The M18's raw hardware byte a press on this physical key reports,
    /// empirically confirmed against real hardware (not transcribed from
    /// `streamdockM18.cpp:69-76` — see `M18_KEYS`'s doc comment for why
    /// that comment doesn't apply to this unit). Consumed only by
    /// `process_input`'s hw-byte -> slot lookup.
    pub hw: u8,
    /// The argument to pass to `Device::write_image`/`clear_button_image`
    /// to address this physical key — **not** always equal to `slot` (see
    /// this struct's doc comment). Consumed only by `device/actor.rs`,
    /// which translates an incoming `slot` to this value before calling
    /// into `mirajazz`.
    pub write_key: u8,
    pub kind: KeyKind,
}

/// Feature set negotiated from the firmware version string at connect time
/// (`streamdockM18.cpp:100-137`). Never inferred from the model table alone —
/// an M18 unit's firmware decides this, and every RGB/dual-screen code path
/// must gate on this, not on `DeviceModel` directly.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct FeatureSet {
    pub rgb: bool,
    pub dual: bool,
}

impl FeatureSet {
    /// `V2.M18`: no RGB, not dual (`streamdockM18.cpp:100-107`).
    pub const V2: Self = Self {
        rgb: false,
        dual: false,
    };
    /// `V25.M18` / `V3.M18`: RGB + dual (`streamdockM18.cpp:108-124`).
    pub const V3: Self = Self {
        rgb: true,
        dual: true,
    };

    /// Classify a firmware version string. An unrecognized string
    /// conservatively degrades to `V2` (no RGB) rather than assuming
    /// capabilities the unit may not have — see AGENTS.md's guidance on
    /// firmware-variant feature negotiation for the M18.
    ///
    /// Matches on the **version prefix** (`V2.`/`V25.`/`V3.`), not the
    /// literal `streamdockM18.cpp` substrings (`"V2.M18"`/`"V25.M18"`/
    /// `"V3.M18"`) that comment describes — those assume the model name
    /// immediately follows the version with no vendor branding in between.
    /// A real VSD-branded M18 reports `"V3.VSDM18_HBOE.02.01"`: the version
    /// prefix is `V3.` as expected, but `"VSDM18"` is not `"M18"`, so the
    /// literal-substring check silently misclassified this exact unit as
    /// `V2` (no RGB) despite its firmware being solidly `V3` — confirmed
    /// empirically against real hardware, not a hypothetical.
    pub fn from_firmware_string(fw: &str) -> Self {
        if fw.starts_with("V25.") || fw.starts_with("V3.") {
            Self::V3
        } else {
            Self::V2
        }
    }
}

/// A device family's fixed geometry + protocol parameters, plus its key
/// table. `mirajazz`-facing fields (`protocol_version`, `key_count`,
/// `encoder_count`) are exactly what `mirajazz::device::Device::connect`
/// requires; `mirajazz` derives packet size (512 vs 1024 bytes) from
/// `protocol_version` internally, so we do not duplicate that here.
pub struct DeviceModel {
    pub product_name: &'static str,
    pub usb_ids: &'static [UsbId],
    /// Protocol version to pass to `Device::connect`. The M18 (via
    /// Companion's `M18V3.ts` and corroborated by the vendor blob's opcode
    /// strings) is a pv2 device: 1024-byte packets, per-device serial.
    pub protocol_version: usize,
    pub key_px: u16,
    pub key_count: usize,
    pub encoder_count: usize,
    pub led_count: u8,
    pub heartbeat_secs: u64,
    pub keys: &'static [KeyDef],
}

/// M18 key map, empirically derived from Boss's VSD-branded unit — **not**
/// transcribed from the vendor SDK's `streamdockM18.cpp:69-76` comment,
/// which does not match this hardware in either direction. Two rounds of
/// hardware testing (2026-09-15) established:
///
/// 1. **Read direction.** Pressing the physical key that `write_image(10)`
///    had labeled "slot 10" reported raw hw byte `0x01`; the key labeled
///    "slot 0" is symmetric at `0x0B`. Middle row (`0x06`-`0x0A`) already
///    read correctly. So reading goes `0x01..0x05` for one outer row,
///    `0x0B..0x0F` for the other, `0x06..0x0A` for the middle row.
/// 2. **Write direction, and which physical row is which.** Boss confirmed
///    by eye which corner is which: `write_image(10)` (wire key id `0x0B`)
///    lands on the physical **top**-left key; `write_image(0)` (wire key id
///    `0x01`) lands on the physical **bottom**-left key. That is the
///    opposite of this crate's original assumption (`slot 0` = top-left)
///    baked into the old table's row layout — which is what this table
///    fixes, via the `write_key` indirection below, rather than by
///    reassigning wire key ids (mirajazz's `write_image(key)` always sends
///    `key + 1`; we cannot change that formula, only which `slot` maps to
///    which `key` argument).
///
/// Net result: `write_key` and `hw` are two independent numbering spaces
/// for the *same* physical key (top-left is `write_key: 10, hw: 0x01`;
/// bottom-left is `write_key: 0, hw: 0x0B`) — confirmed, not assumed, and
/// `slot` is renumbered here so it finally means what its own doc comment
/// promises: true physical visual order, top-left = 0.
///
/// The three plain push buttons get slots 15..18, appended after the 15
/// LCD keys; their own hw bytes are unverified against real hardware so
/// far (only that they generate *some* recognized event, per the double-tap
/// bug also seen on them) and are left as documented by the vendor SDK
/// until specifically tested.
#[rustfmt::skip]
pub static M18_KEYS: &[KeyDef] = &[
    // True physical top row (slots 0..5): hw 0x01..0x05, write_key 10..15
    KeyDef { slot: 0,  hw: 0x01, write_key: 10, kind: KeyKind::Lcd },
    KeyDef { slot: 1,  hw: 0x02, write_key: 11, kind: KeyKind::Lcd },
    KeyDef { slot: 2,  hw: 0x03, write_key: 12, kind: KeyKind::Lcd },
    KeyDef { slot: 3,  hw: 0x04, write_key: 13, kind: KeyKind::Lcd },
    KeyDef { slot: 4,  hw: 0x05, write_key: 14, kind: KeyKind::Lcd },
    // Middle row (slots 5..10): hw and write_key both unaffected (== slot)
    KeyDef { slot: 5,  hw: 0x06, write_key: 5,  kind: KeyKind::Lcd },
    KeyDef { slot: 6,  hw: 0x07, write_key: 6,  kind: KeyKind::Lcd },
    KeyDef { slot: 7,  hw: 0x08, write_key: 7,  kind: KeyKind::Lcd },
    KeyDef { slot: 8,  hw: 0x09, write_key: 8,  kind: KeyKind::Lcd },
    KeyDef { slot: 9,  hw: 0x0A, write_key: 9,  kind: KeyKind::Lcd },
    // True physical bottom row (slots 10..15): hw 0x0B..0x0F, write_key 0..5
    KeyDef { slot: 10, hw: 0x0B, write_key: 0,  kind: KeyKind::Lcd },
    KeyDef { slot: 11, hw: 0x0C, write_key: 1,  kind: KeyKind::Lcd },
    KeyDef { slot: 12, hw: 0x0D, write_key: 2,  kind: KeyKind::Lcd },
    KeyDef { slot: 13, hw: 0x0E, write_key: 3,  kind: KeyKind::Lcd },
    KeyDef { slot: 14, hw: 0x0F, write_key: 4,  kind: KeyKind::Lcd },
    // Three plain push buttons (slots 15..18) — hw/write_key not yet
    // individually confirmed against this unit; left as the vendor SDK
    // documents them, and assumed symmetric (write_key == hw's implied
    // index) until specifically tested.
    KeyDef { slot: 15, hw: 0x25, write_key: 15, kind: KeyKind::Button },
    KeyDef { slot: 16, hw: 0x30, write_key: 16, kind: KeyKind::Button },
    KeyDef { slot: 17, hw: 0x31, write_key: 17, kind: KeyKind::Button },
];

pub static M18: DeviceModel = DeviceModel {
    product_name: "StreamDock M18",
    usb_ids: &[
        (0x6603, 0x1009), // plain Mirabox M18
        (0x5548, 0x1000), // VSD-branded M18 — our unit
        (0x6603, 0x1012), // M18E
    ],
    protocol_version: 2,
    key_px: 64,
    key_count: 18,
    encoder_count: 0,
    led_count: 24,
    heartbeat_secs: 10,
    keys: M18_KEYS,
};

/// Every model this crate knows about. `device/hotplug.rs` enumerates against
/// the union of every model's `usb_ids`.
pub static MODELS: &[&DeviceModel] = &[&M18];

/// `mirajazz::Device::get_reader` requires a bare `fn(u8, u8) -> Result<...>`
/// pointer — not a closure, so it cannot capture a `&DeviceModel` reference.
/// Since every model we know about is a fixed `static`, the free function
/// below can simply refer to it directly; this lookup is the one place that
/// bridges "which model is this device" (decided once, at connect time) back
/// to "which free function do I hand to `get_reader`". Adding a second model
/// means adding both a new `DeviceModel` const/static above and a matching
/// free `process_input` function + arm here — `verify_bijection`'s test
/// coverage does not extend to this wiring, so a mismatch here would only
/// surface as every keypress on the new model decoding against the wrong
/// table. Keep the two changes next to each other.
pub fn process_input_fn(
    model: &'static DeviceModel,
) -> fn(u8, u8) -> Result<DeviceInput, MirajazzError> {
    if std::ptr::eq(model, &M18) {
        m18_process_input
    } else {
        // Unreachable for any model actually returned by `MODELS`/hotplug
        // discovery today (there is only one), but fails safe rather than
        // panicking if that ever changes without this function being
        // updated in step.
        |_hw, _state| Ok(DeviceInput::NoData)
    }
}

fn m18_process_input(hw: u8, state: u8) -> Result<DeviceInput, MirajazzError> {
    M18.process_input(hw, state)
}

impl DeviceModel {
    /// hw byte -> logical slot, built once. Every entry in `keys` must
    /// appear exactly once as both a `slot` in `0..keys.len()` and a unique
    /// `hw` byte — enforced by `verify_bijection` below, which every model
    /// must pass (see the test at the bottom of this file).
    fn hw_to_slot(&self, hw: u8) -> Option<u8> {
        self.keys.iter().find(|k| k.hw == hw).map(|k| k.slot)
    }

    /// slot -> the argument to pass to `Device::write_image`/
    /// `clear_button_image` to address that physical key. **Not** the same
    /// as `slot` on this firmware — see `KeyDef`'s doc comment. Every
    /// `device/actor.rs` write path must go through this, never use a raw
    /// `slot` directly against `mirajazz`.
    pub fn write_key_of_slot(&self, slot: u8) -> Option<u8> {
        self.keys
            .iter()
            .find(|k| k.slot == slot)
            .map(|k| k.write_key)
    }

    /// Panics (in a `debug_assert`-style check, called from tests and from
    /// startup) if `keys` is not a bijection between `0..keys.len()` and
    /// its `hw` bytes, AND a separate bijection between `0..keys.len()` and
    /// its `write_key` values (the two are independent numbering spaces on
    /// this firmware — see `KeyDef`'s doc comment — so each needs its own
    /// uniqueness check; a collision in either is the most likely
    /// transcription bug in the whole crate).
    pub fn verify_bijection(&self) -> Result<(), String> {
        let mut seen_slots = vec![false; self.keys.len()];
        let mut seen_hw = std::collections::HashSet::new();
        let mut seen_write_key = std::collections::HashSet::new();
        for k in self.keys {
            let slot = k.slot as usize;
            if slot >= self.keys.len() {
                return Err(format!(
                    "{}: slot {slot} out of range 0..{}",
                    self.product_name,
                    self.keys.len()
                ));
            }
            if seen_slots[slot] {
                return Err(format!("{}: slot {slot} appears twice", self.product_name));
            }
            seen_slots[slot] = true;
            if !seen_hw.insert(k.hw) {
                return Err(format!(
                    "{}: hw byte 0x{:02X} appears twice",
                    self.product_name, k.hw
                ));
            }
            if !seen_write_key.insert(k.write_key) {
                return Err(format!(
                    "{}: write_key {} appears twice",
                    self.product_name, k.write_key
                ));
            }
        }
        if let Some((slot, _)) = seen_slots.iter().enumerate().find(|(_, seen)| !**seen) {
            return Err(format!(
                "{}: slot {slot} is never assigned",
                self.product_name
            ));
        }
        if seen_write_key.len() != self.keys.len()
            || (0..self.keys.len() as u8).any(|w| !seen_write_key.contains(&w))
        {
            return Err(format!(
                "{}: write_key values are not a permutation of 0..{}",
                self.product_name,
                self.keys.len()
            ));
        }
        if self.key_count != self.keys.len() {
            return Err(format!(
                "{}: key_count {} does not match keys.len() {}",
                self.product_name,
                self.key_count,
                self.keys.len()
            ));
        }
        Ok(())
    }

    /// Build the `process_input` function `mirajazz::Device::get_reader`
    /// needs: raw `(hw_byte, state_byte)` -> `DeviceInput`.
    ///
    /// `mirajazz` already normalizes `state` to a plain 0/1 and synthesizes
    /// Down+Up pairs itself for any device below protocol v3
    /// (`supports_both_keypress_states = protocol_version > 2`, see
    /// `DeviceStateReader::input_to_updates`) — but only for whichever slots
    /// *this function* reports as pressed. **This function must treat only
    /// `state == 0x01` as pressed, not `state != 0`.**
    ///
    /// Confirmed empirically 2026-09-15: a single, quick physical
    /// press-and-release on this firmware produces TWO separate input
    /// reports — one at the press edge (`state == 0x01`) and a second at the
    /// release edge (`state == 0x02`, per the documented Python SDK
    /// convention: "0x02=release for some firmwares") — not one report that
    /// simply stops arriving. An earlier version of this function used
    /// `state != 0`, which treats BOTH reports as "pressed", so mirajazz's
    /// `!supports_both_keypress_states` synthesis fired an immediate
    /// Down+Up pair for each of them: a single tap produced two synthesized
    /// pairs close together, which `dispatch::GestureResolver` then
    /// misread as a `DoubleTap`, and a held-then-released key produced one
    /// pair on press and a second, spurious pair on release (misread as two
    /// separate taps). Filtering to `state == 0x01` only makes the release
    /// report `buttons[slot] = false`, which mirajazz's
    /// `!supports_both_keypress_states` branch (`if *their { ... }`) simply
    /// does not act on — exactly restoring "one physical press = one
    /// synthesized Down+Up pair, full stop" regardless of hold duration.
    pub fn process_input(&self, hw: u8, state: u8) -> Result<DeviceInput, MirajazzError> {
        let Some(slot) = self.hw_to_slot(hw) else {
            // An unrecognized hw byte (e.g. a stray report, or a button this
            // table doesn't know about) is not an error worth failing the
            // read loop over — report it as no-op input.
            return Ok(DeviceInput::NoData);
        };
        let mut buttons = vec![false; self.key_count];
        buttons[slot as usize] = state == 0x01;
        Ok(DeviceInput::ButtonStateChange(buttons))
    }

    pub fn matches_usb_id(&self, vid: u16, pid: u16) -> bool {
        self.usb_ids.contains(&(vid, pid))
    }

    pub fn kind_of_slot(&self, slot: u8) -> Option<KeyKind> {
        self.keys.iter().find(|k| k.slot == slot).map(|k| k.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_model_is_a_bijection() {
        for model in MODELS {
            model.verify_bijection().unwrap_or_else(|e| panic!("{e}"));
        }
    }

    #[test]
    fn m18_hw_map_matches_boss_vsd_unit_not_the_vendor_sdk_comment() {
        // `slot` is true physical visual order (0 = top-left). Reading and
        // writing this unit use two INDEPENDENT numbering spaces for the
        // same physical key — see `KeyDef`'s doc comment — so this test
        // only covers the read direction (`hw`); `write_key_of_slot`
        // assertions live in their own test below.
        assert_eq!(M18.hw_to_slot(0x01), Some(0), "top-left reads as hw 0x01");
        assert_eq!(M18.hw_to_slot(0x05), Some(4), "top-right reads as hw 0x05");
        assert_eq!(M18.hw_to_slot(0x06), Some(5), "middle row unaffected");
        assert_eq!(M18.hw_to_slot(0x0A), Some(9), "middle row unaffected");
        assert_eq!(
            M18.hw_to_slot(0x0B),
            Some(10),
            "bottom-left reads as hw 0x0B"
        );
        assert_eq!(
            M18.hw_to_slot(0x0F),
            Some(14),
            "bottom-right reads as hw 0x0F"
        );
        assert_eq!(M18.hw_to_slot(0x25), Some(15));
        assert_eq!(M18.hw_to_slot(0x30), Some(16));
        assert_eq!(M18.hw_to_slot(0x31), Some(17));
        assert_eq!(M18.hw_to_slot(0x99), None);
    }

    #[test]
    fn write_key_differs_from_slot_and_from_hw_for_the_outer_rows() {
        // The confirmed asymmetry (2026-09-15, two rounds of hardware
        // testing — see M18_KEYS's doc comment): the SAME physical key has
        // a different byte value depending on whether you're reading a
        // press (`hw`) or writing an image (`write_key`, which mirajazz
        // sends as `write_key + 1` on the wire). Top-left is slot 0 but
        // must be written via write_key 10 (wire key id 0x0B) to actually
        // land there; bottom-left is slot 10 but written via write_key 0
        // (wire key id 0x01). Middle row has no asymmetry at all.
        assert_eq!(
            M18.write_key_of_slot(0),
            Some(10),
            "top-left writes via write_key 10, not 0"
        );
        assert_eq!(
            M18.write_key_of_slot(4),
            Some(14),
            "top-right writes via write_key 14, not 4"
        );
        assert_eq!(
            M18.write_key_of_slot(5),
            Some(5),
            "middle row: write_key == slot"
        );
        assert_eq!(
            M18.write_key_of_slot(9),
            Some(9),
            "middle row: write_key == slot"
        );
        assert_eq!(
            M18.write_key_of_slot(10),
            Some(0),
            "bottom-left writes via write_key 0, not 10"
        );
        assert_eq!(
            M18.write_key_of_slot(14),
            Some(4),
            "bottom-right writes via write_key 4, not 14"
        );
        assert_eq!(M18.write_key_of_slot(99), None);
    }

    #[test]
    fn our_vsd_pid_is_registered() {
        assert!(
            M18.matches_usb_id(0x5548, 0x1000),
            "the VSD-branded unit must resolve to M18"
        );
    }

    #[test]
    fn firmware_feature_negotiation() {
        assert_eq!(FeatureSet::from_firmware_string("V2.M18"), FeatureSet::V2);
        assert_eq!(
            FeatureSet::from_firmware_string("V25.M18-1.0.3"),
            FeatureSet::V3
        );
        assert_eq!(
            FeatureSet::from_firmware_string("V3.M18-2.1.0"),
            FeatureSet::V3
        );
        // The real string reported by Boss's VSD-branded unit — confirmed
        // 2026-09-14. Regression test for the literal-substring bug: this
        // string does NOT contain "V3.M18" (it has "V3.VSDM18" instead),
        // so it must be matched on the version prefix, not the old check.
        assert_eq!(
            FeatureSet::from_firmware_string("V3.VSDM18_HBOE.02.01"),
            FeatureSet::V3
        );
        // Unrecognized -> conservative default, never assume RGB exists.
        assert_eq!(FeatureSet::from_firmware_string("garbage"), FeatureSet::V2);
        assert_eq!(FeatureSet::from_firmware_string(""), FeatureSet::V2);
    }

    #[test]
    fn process_input_reports_the_expected_slot() {
        // Physically pressing the top-left LCD key (hw 0x01) must decode
        // to slot 0, state pressed.
        let input = M18.process_input(0x01, 0x01).unwrap();
        match input {
            DeviceInput::ButtonStateChange(buttons) => {
                assert!(buttons[0], "slot 0 should read pressed");
                assert!(
                    buttons[1..].iter().all(|b| !*b),
                    "only slot 0 should be set"
                );
            }
            other => panic!("expected ButtonStateChange, got {other:?}"),
        }
    }

    #[test]
    fn process_input_fn_resolves_to_the_m18_table() {
        let f = process_input_fn(&M18);
        let input = f(0x01, 0x01).unwrap();
        match input {
            DeviceInput::ButtonStateChange(buttons) => assert!(buttons[0]),
            other => panic!("expected ButtonStateChange, got {other:?}"),
        }
    }

    #[test]
    fn only_state_0x01_counts_as_pressed() {
        // The regression test for the double-tap bug: a quick physical
        // press-and-release sends two reports (press edge state=0x01,
        // release edge state=0x02 or similar nonzero), and only the first
        // may register as pressed — see process_input's doc comment.
        let pressed = M18.process_input(0x01, 0x01).unwrap();
        assert!(matches!(&pressed, DeviceInput::ButtonStateChange(b) if b[0]));

        for release_byte in [0x00u8, 0x02, 0xFF] {
            let released = M18.process_input(0x01, release_byte).unwrap();
            assert!(
                matches!(&released, DeviceInput::ButtonStateChange(b) if !b[0]),
                "state byte 0x{release_byte:02X} must NOT register as pressed"
            );
        }
    }

    #[test]
    fn process_input_tolerates_an_unknown_hw_byte() {
        let input = M18.process_input(0xEE, 0x01).unwrap();
        assert!(
            input.is_empty(),
            "an unrecognized hw byte must not error the read loop"
        );
    }
}
