# Audio dependency alignment — 2036-f740

## Contract and upstream review

Preserve notification synthesis, volume, output selection and fallback, microphone
capture to 16 kHz mono, and Edge speech MP3 decoding. The real device configuration
comes from CPAL; tests of configuration ranges are API-boundary cases, not recorded
hardware fixtures or proof of a particular microphone's capabilities.

Sources inspected before manifest changes:

- `https://static.crates.io/crates/rodio/rodio-0.23.0.crate`: `CHANGELOG.md`,
  `UPGRADE.md`, `Cargo.toml` and `src/stream.rs`.
- `https://static.crates.io/crates/cpal/cpal-0.18.2.crate`: `CHANGELOG.md`,
  `UPGRADING.md`, `Cargo.toml` and configuration APIs in `src/lib.rs`.

Rodio 0.23.0 is a minimal CPAL upgrade. Its MSRV is Rust 1.87; CPAL 0.18.2
requires Rust 1.85. Native validation used Rust 1.98.1. CPAL retains CoreAudio,
WASAPI and ALSA support; this change does not opt into a new host or realtime
feature. Its Apple dependency requires mach2 0.6.

CPAL replaces per-operation errors with `Error`/`ErrorKind`; the application only
formats these errors, so no typed error matching needs migration. Stream builders
now take configuration by value. Capture already calls `play()` explicitly;
rodio also calls it for output. Rodio retains its 100 ms output buffer policy.

CPAL's default format preference can now put F64/I32/I24 above I16. Capture keeps
a supported F32/I16 default unchanged. If the default is unsupported by our
callback, it selects a compatible advertised range, prefers F32, and retains the
default rate if that range supports it; otherwise it uses the range maximum.
The callback's sample conversion, metering, buffering and resampling are unchanged.
If no supported F32/I16 range exists, capture returns an error before stream creation.

## Exact lockfile scope

The baseline already had one CPAL version. This migration updates that shared line;
it does not claim to remove a duplicate that was absent.

| Package | Before | After | Reason |
| --- | --- | --- | --- |
| rodio | 0.22.2 | 0.23.0 | All three manifest owners use the newer CPAL line |
| cpal | 0.17.3 | 0.18.2 | Rodio requirement and direct dictation dependency |
| mach2 | 0.5.0 | 0.6.0 | CPAL's Apple target requirement; removes its libc edge |
| objc2-avf-audio | 0.3.2 | 0.3.2 | Feature-selected dependency edge adds existing bitflags 2.13.1 |

CPAL's own dependency edges add existing `block2` and `windows-core 0.62.2`, switch
from existing `jni 0.21.1` to existing `jni 0.22.4`, and remove its direct
`wasm-bindgen`/`wasm-bindgen-futures` edges. These packages already exist elsewhere
in the lockfile; their versions and package records do not change. No other
package records change. Rust third-party notices were regenerated and change only
the CPAL and rodio version rows.

Resolved all-target, all-feature workspace graph:

```text
cpal 0.18.2
├── rodio 0.23.0
│   ├── tuicommander
│   ├── tuic-core (audio-output)
│   └── tuic-dictation
└── tuic-dictation (direct capture)
```

## Native verification

Every Cargo command used the mbx shim through build-slot in the worktree. Tests
also used `scripts/with-test-tmp.sh` and a Gits-local temporary root.

- `mbx doctor`: zero failures and warnings.
- `cargo tree --manifest-path src-tauri/Cargo.toml --locked --target all --workspace --all-features -i cpal`: one CPAL 0.18.2.
- `node scripts/third-party-notices.mjs --only rust`: succeeded.
- `cargo nextest run --manifest-path src-tauri/Cargo.toml --locked -p tuicommander -p tuic-dictation --lib -E 'test(notification_sound::tests::) | test(audio::tests::) | test(speaker::tests::device_output_) | test(loudness::tests::) | test(two_joined_requests_decode_to_both_of_them) | test(bytes_that_are_not_mp3_are_a_typed_failure) | test(a_stream_cut_mid_frame_never_yields_empty_audio)'`: **49 passed**, 1.196 s test execution after 4 min 31 s compilation. Filtered-out tests are not full-suite evidence.
- `cargo check --manifest-path src-tauri/Cargo.toml --locked --features dictation --lib`: passed, 2 min 44 s.
- `cargo check --manifest-path src-tauri/Cargo.toml --locked --no-default-features --bin tuic-remote`: passed, 36.04 s.
- Instruction-link validation: two instruction pairs valid.

Native build warnings were not test failures: WebRTC's C++ build reported the
libc++ deployment-platform warning, the large application test binary reported
an unwind-table linker warning, and mbx reported a compiler invocation without
an output file. No test emitted unexpected runtime noise.

This is a dependency refactor, not a demonstrated pre-existing bug. No failing
pre-fix test is claimed. Three new API-boundary tests protect compatible format
selection, an integer-only fallback at its supported rate, and rejection when
no supported format exists. Existing tests protect capture conversion, stop/drain,
notification envelopes/attenuation, deviceless playback controls, loudness, and
joined/truncated/invalid MP3 decoding.

## Remaining evidence

Linux rb gates are tracked in the story worklog and final handoff. Windows
execution requires its CI runner. Native compile and deviceless tests do not
prove microphone permission, physical capture, audible volume, output routing or
end-click behavior. `to-test.md` records those hardware checks after Boss manually
restarts or rebuilds the application. This peer did not restart, replace or launch
a desktop application instance.
