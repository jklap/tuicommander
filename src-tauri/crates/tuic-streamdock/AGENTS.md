# tuic-streamdock — crate rules

Mirabox/VSD StreamDock M18 macropad integration: device transport (built on the
`mirajazz` crate), rendering, gesture resolution, and slot-assignment policy —
all decoupled from `AppState` behind the `StateSource`/`ActionSink` traits in
`src/port.rs`. See `plans/streamdock-m18-integration.md` (repo root, gitignored
local reference) for the original design plan and phase-by-phase implementation
notes; `docs/FEATURES.md` §26 for the user-facing feature description;
`docs/sync-matrix.md`'s "StreamDock M18 Macropad" section for which files to
touch together when changing this feature.

## No USB access in most sandboxes — read the doc comments before assuming a bug

`examples/probe.rs` (hardware spike) and `src/bin/demo.rs` (standalone panel
driver) both need a real M18 physically attached; most agent sandboxes have no
USB access and can't run either. Every hardware-derived constant in this crate
was confirmed against Boss's real VSD-branded unit and is recorded, with the
actual test evidence, in the owning module's doc comment — **read
`device/model.rs`'s `KeyDef`/`M18_KEYS` doc comments and `dispatch.rs`'s module
doc comment before changing anything hardware-related.** Both record disproven
hypotheses (the hw/write_key numbering-space asymmetry; two wrong gesture
models before the correct press/release-pairing one) precisely so a future
agent doesn't independently reintroduce the same wrong guess the vendor SDK's
own comments would lead you toward. If you must change a hardware constant
without being able to test on real hardware, say so explicitly and flag it for
Boss to verify — don't assume the vendor SDK's C++ source is correct for this
firmware; it wasn't, twice.

## Testing this crate alone

`cargo nextest run -p tuic-streamdock` (69 tests as of this writing — device
model bijection, protocol/gesture logic, render determinism, policy eviction
rules, LED aggregation — all pure/unit, no hardware needed). `cargo clippy -p
tuic-streamdock --release -- -D warnings` (workspace turns on
`clippy::pedantic`).

## `mirajazz`'s `get_reader` needs a bare `fn` pointer, not a closure

Per-model input decoding (`device/model.rs`'s `process_input_fn`) can't close
over a `&'static DeviceModel` the normal way — `mirajazz::Device::get_reader`
requires `fn(u8, u8) -> Result<DeviceInput, MirajazzError>`. Adding a second
model means adding both a new `DeviceModel` const/static in `model.rs` AND a
matching free function + match arm in `process_input_fn` — the crate's own
bijection test (`verify_bijection`) does not cover this second piece of
wiring, so a mismatch there silently decodes every keypress on the new model
against the wrong table with no test failure. Keep the two changes adjacent
when adding a model.

## Two independent numbering spaces for the same physical key

On this firmware, reading a button press (`hw` byte) and writing that same
key's LCD image (`write_key`, what `Device::write_image` takes) are NOT the
same number — confirmed empirically, not documented anywhere by the vendor.
`device/actor.rs` is the only place allowed to translate a `slot` (true
physical visual order — what every module above `device/` speaks in) into a
`write_key` before calling into `mirajazz`; never pass a raw `slot` to
`mirajazz` directly, and never assume `hw == write_key` for a future model
without testing it on real hardware first.

## Ambient LEDs are gated on negotiated `FeatureSet`, never the model table

A `V2.M18` unit has no addressable LEDs at all; RGB support is negotiated from
the firmware version string at connect time (`FeatureSet::from_firmware_string`
in `device/model.rs`), not inferred from `DeviceModel` itself. `leds.rs`'s
`ambient_for`/`AmbientLed::colors` are pure and always computable — the caller
(`src-tauri/src/streamdock/mod.rs`'s `run_one_device`) is responsible for
skipping the actual device write when RGB isn't supported. Follow this same
gating for any future RGB-dependent feature; don't move the check into this
crate's own coordinator, which deliberately has no concept of firmware
capability.
