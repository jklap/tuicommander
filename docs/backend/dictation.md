# Voice Dictation

**Module:** `src-tauri/src/dictation/`

Local voice-to-text using Whisper with Metal acceleration on macOS. Push-to-talk workflow with streaming partial results: hold hotkey to record, see partial transcriptions in real-time, release to finalize.

## Module Structure

| File | Purpose |
|------|---------|
| `mod.rs` | `DictationState` — shared state for all dictation operations |
| `audio.rs` | Audio capture from microphone via CPAL (VecDeque ring buffer) |
| `commands.rs` | Tauri command handlers |
| `model.rs` | Whisper model download and management |
| `transcribe.rs` | `Transcriber` trait + `WhisperTranscriber` implementation via whisper-rs |
| `streaming.rs` | Streaming transcription loop with adaptive windows and VAD |
| `vad.rs` | Voice Activity Detection (energy-based, ported from whisper.cpp) |
| `corrections.rs` | Post-processing text corrections |

## Tauri Commands

### Recording

| Command | Description |
|---------|-------------|
| `start_dictation()` | Start recording + streaming transcription |
| `stop_dictation_and_transcribe()` | Stop streaming, final pass on full captured audio, return `TranscribeResponse { text, skip_reason, duration_s, truncated_s }` |
| `inject_text(text)` | Apply corrections to text (called after transcription) |

### Tauri Events

| Event | Direction | Payload |
|-------|-----------|---------|
| `dictation-partial` | Rust → Frontend | `String` — partial transcription text |
| `dictation-download-progress` | Rust → Frontend | `{ downloaded, total, percent }` |

### Model Management

| Command | Description |
|---------|-------------|
| `get_model_info()` | List available Whisper models with download status |
| `download_whisper_model(model_name)` | Download model (emits progress events) |
| `delete_whisper_model(model_name)` | Delete a downloaded model |

### Configuration

| Command | Description |
|---------|-------------|
| `get_dictation_status()` | Model status, recording/processing state, and normalized `audio_level` (0–1). The preview polls this shared IPC/HTTP response while recording. |
| `get_dictation_config()` | Load dictation configuration (includes `rms_threshold` and `no_speech_threshold` — see "Speech gates") |
| `set_dictation_config(config)` | Save dictation configuration |
| `get_correction_map()` | Load text correction dictionary |
| `set_correction_map(map)` | Save text correction dictionary |
| `list_audio_devices()` | List available audio input devices |

## DictationState

```rust
pub struct DictationState {
    pub audio: Mutex<Option<AudioCapture>>,
    pub active_model: Mutex<Option<String>>,
    pub corrections: Mutex<TextCorrector>,
    pub recording: AtomicBool,
    pub processing: AtomicBool,
    pub streaming: Mutex<Option<StreamingSession>>,
    pub transcriber_arc: Mutex<Option<Arc<dyn Transcriber>>>,
    pub accumulated_partials: Arc<Mutex<String>>,
}
```

Managed as Tauri state alongside `AppState`.

## Transcriber Trait

```rust
pub trait Transcriber: Send + Sync {
    fn transcribe(&self, audio: &[f32], language: Option<&str>) -> Result<TranscribeResult, String>;
}
```

`WhisperTranscriber` implements this trait using whisper-rs. The trait abstraction enables mock implementations for testing without requiring a Whisper model.

## Recording Guard (TOCTOU)

`start_dictation()` uses `compare_exchange(false, true, AcqRel, Acquire)` on the `recording` flag to prevent TOCTOU races from concurrent IPC calls. If two calls arrive simultaneously, only the first succeeds; the second returns `"Already recording"`. A drop guard resets `recording = false` on any early error return.

## Streaming Architecture

```
User holds hotkey
    │
    ▼
start_dictation()
    ├── Load/reuse WhisperTranscriber (Arc-wrapped)
    ├── Start CPAL AudioCapture → VecDeque<f32> buffer
    ├── Start StreamingSession (background thread)
    │       │
    │       ├── Poll audio buffer (50ms interval)
    │       ├── Accumulate in step_buf
    │       ├── When step_buf >= window size:
    │       │       ├── VAD check → skip if silence
    │       │       ├── Build window: [keep_tail | step_buf]
    │       │       ├── whisper_full(window)
    │       │       └── Send partial via mpsc::channel
    │       └── Adaptive growth: 1.5s → 2.0s → 2.5s → 3.0s (max)
    │
    ├── Spawn event forwarder thread
    │       └── mpsc::Receiver → emit("dictation-partial")
    │
    └── Set recording = true
    │
User releases hotkey
    │
    ▼
stop_dictation_and_transcribe()  [async]
    ├── Set recording=false, processing=true (synchronous, UI updates immediately)
    ├── Stop cpal stream (buffer preserved)
    ├── Signal StreamingSession stop → join thread
    ├── Collect ALL audio (processed + unprocessed + capture buffer remainder)
    ├── spawn_blocking: Final transcription on full captured audio (if >= 0.5s)
    │   ├── ProcessingGuard (drop guard) clears processing=false on completion/panic
    │   ├── Apply text corrections
    │   └── Return TranscribeResponse
    └── Return TranscribeResponse { text, skip_reason, duration_s, truncated_s }
    │
    ▼
Frontend injects text into focus target
```

## VAD (Voice Activity Detection)

Ported from whisper.cpp `common.cpp` `vad_simple()`:

- **Algorithm:** Compare absolute energy of last `last_ms` (1000ms) vs entire buffer
- **High-pass filter:** First-order RC at 100Hz removes ambient noise (HVAC, fans)
- **Threshold:** `vad_thold = 0.6` — if `energy_last / energy_all < 0.6`, silence detected
- **Relative:** Microphone gain doesn't affect detection (ratio-based)

## Speech gates

Whisper transcribes whatever it is given. On room noise it invents subtitle
boilerplate, so three gates in `transcribe()` decide whether audio is speech at
all. They run in order and each returns a `skip_reason` the UI shows verbatim.

| Gate | Rejects | Tunable |
|---|---|---|
| RMS floor | audio quieter than `rms_threshold` — never reaches Whisper | yes |
| `no_speech_probability` | the segments Whisper itself scores above `no_speech_threshold` | yes |
| `is_hallucination` | known subtitle boilerplate and bare thanks | no |

**The two thresholds are settings, not constants** (`VoiceGates`, read from
`DictationConfig` on every start). The right RMS floor depends on the room and
the microphone: a headset a metre away picks up enough noise to clear a fixed
floor, which is how an empty room ends up transcribed. Settings > Dictation
exposes both against a live meter — see the user guide.

`no_speech_probability()` is read per segment and **filters per segment**
(`filter_speech_segments`): a segment above the threshold is dropped, the rest
are kept, and only a run where every segment was rejected skips the whole
transcript. It generalises where a phrase list cannot, because it rejects
whatever the model invented rather than only the wordings someone remembered to
add to a list. Note that whisper.cpp does not implement the `no_speech_thold`
*parameter*, so the value must be compared after the run, not set on
`FullParams`.

The gate used to take the worst score across the run and discard everything on
it. That was harmless while a run was one segment, and wrong as soon as
recordings longer than one window started decoding into many: an ordinary pause
inside a long dictation scores as no-speech, so one silent segment threw away a
transcript that was almost entirely speech.

**`is_hallucination` matches per sentence, not on the whole trimmed string.**
The short-phrase list (`HALLUCINATION_EXACT`) holds words a user genuinely
dictates, so it only fires when *every* sentence is boilerplate — `"Grazie."`
is filtered, `"Grazie. Ora committa e pusha."` is not. Matching the trimmed
string as one unit missed the repeated form: streaming windows are 1.5–3 s and
produce one bare `"Grazie."`, but the final pass runs on the whole buffer, where
Whisper loops into `"Grazie. Grazie."` — the form that actually reached the
terminal. `HALLUCINATION_SUBSTRING` holds channel boilerplate nobody dictates,
so one occurrence anywhere condemns the transcript.

## Decode flags depend on audio length

`decode_flags_for` picks `single_segment` and `no_timestamps` from the sample
count, split at `SINGLE_WINDOW_SAMPLES` (30 s — one whisper encoder window).
Both flags are set at or under one window and cleared above it.

`no_timestamps` suppresses every timestamp token outright (`whisper.cpp:6191`),
so `has_ts` never becomes true. With either flag set, the end of a segment forces
the window shift to a whole chunk (`whisper.cpp:7381`, `seek_delta =
100*WHISPER_CHUNK_SIZE`) and `seek += seek_delta` (`whisper.cpp:7734`) advances a
full 30 s no matter how much the decoder actually reached. An early end-of-text
token or the 220-token decode limit (`whisper.cpp:7184`) then drops the rest of
that window permanently — with timestamps on, `seek` would instead advance only
to the last decoded timestamp and the remainder would be re-decoded.

A 127.7 s dictation came back with 1175 characters against 1442 from the
streaming partials before this split existed. At or under one window the loss
cannot happen, because the loop breaks once `seek` reaches the end of the audio
(`whisper.cpp:7008`) — which is why short dictation keeps both flags and the
hallucination suppression they were added for (whisper.cpp issue 1724).

Streaming windows are 1.5–3 s, so they keep the flags by the same rule; a
`MAX_BUFFER_S` forced flush is the one window that can cross the line, and
clearing the flags there is correct for the same reason.

## Recording cap

Dictation is push-to-talk, so a real utterance lasts seconds. `MAX_RECORDING_S`
(300 s) bounds the audio kept for the final pass — without it a stuck key is both
an unbounded allocation and an unbounded whisper pass. The cap keeps the newest
audio and drops the oldest, `TRIM_HYSTERESIS_S` at a time.

The cap is applied twice, because the streaming thread only sees what it drained
itself: once per poll inside `streaming_loop` (past `TRIM_HYSTERESIS_S`, so the
memmove is rare), and once by `cap_finished_recording` in
`stop_dictation_and_transcribe` after the capture-buffer tail is appended to the
joined result. A slow final whisper window makes that tail long, so without the
second pass the buffer handed to the final transcription can exceed the cap.

The trim is reported, never silent: both passes count the dropped samples,
`StreamingSession::stop()` returns the loop's count in `StreamingAudio`, and the
command converts the sum to `truncated_s`. `useDictation` turns a non-zero value
into a status message instead of "Ready", so a transcription missing its
beginning cannot read as a complete one.

`StreamingAudio.interrupted` marks a panicked streaming thread. Its audio is
gone, so what remains in the capture buffer is a fragment; the command returns
`skip_reason` rather than transcribing that fragment and presenting it as the
recording.

## Streaming Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `INITIAL_STEP_MS` | 1500 | First window size (fast first partial) |
| `MAX_STEP_MS` | 3000 | Maximum window size |
| `STEP_GROWTH_MS` | 500 | Growth per iteration |
| `KEEP_MS` | 200 | Overlap from previous window |
| `POLL_INTERVAL_MS` | 50 | Audio buffer polling interval |
| `MAX_BUFFER_S` | 30 | Force flush on very long recordings |
| `MAX_RECORDING_S` | 300 | Cap on the audio retained for the final pass |
| `TRIM_HYSTERESIS_S` | 30 | Slack before the cap trims, to make trimming rare |
| `VAD_THRESHOLD` | 0.6 | Energy ratio threshold |
| `VAD_FREQ_THRESHOLD` | 100.0 | High-pass cutoff Hz |

## Audio Pipeline

```
Microphone → CPAL callback → try_lock() → VecDeque<f32> ← drain_samples() ← StreamingSession
                                                                                    │
                                                                              whisper_full()
                                                                                    │
                                                                              mpsc::channel
                                                                                    │
                                                                         event forwarder thread
                                                                                    │
                                                                          "dictation-partial"
                                                                                    │
                                                                         DictationToast (UI)
```

Key design: `try_lock()` in the CPAL callback ensures the real-time audio thread **never blocks**. On contention, samples are silently dropped — acceptable for dictation at 16kHz mono (~64KB/s).

## Audio Resampling

`process_audio_chunk()` converts raw microphone input to the 16kHz mono f32 PCM format required by Whisper:

1. **I16 → F32 conversion** — If the audio device provides `I16` samples, they are normalized to `[-1.0, 1.0]` by dividing by `i16::MAX`.
2. **Stereo → mono** — Multi-channel frames are averaged (`frame.sum() / channels`).
3. **Nearest-neighbor resampling to 16kHz** — For sample rates other than 16kHz (e.g., 48kHz), the output length is calculated as `input_len * (16000 / sample_rate)` and samples are picked by index mapping (`src_idx = i / ratio`).

Pre-allocated scratch buffers (`mono_buf`, `resample_buf`) are captured in the CPAL closure to avoid per-callback heap allocations. The buffer is capped at 30 seconds (480k samples) to prevent unbounded growth.

## Model Storage

Models stored in: `<config_dir>/models/`

Available models (GGML format):

| Model | Size | Quality |
|-------|------|---------|
| `small` | ~488 MB | Good |
| `small.en` | ~488 MB | Good (English-only) |
| `large-v2` | ~3.0 GB | Highest accuracy (slow) |
| `large-v3-turbo` | ~1.6 GB | Best (recommended, default) |

## Text Corrections

User-configurable dictionary for post-processing:

```json
{
  "new line": "\n",
  "tab": "\t",
  "period": ".",
  "comma": ","
}
```

Stored in dictation config. Applied after transcription, before injecting into terminal.

## Platform Notes

- **macOS:** Metal acceleration via whisper-rs (GPU-accelerated, always)
- **Linux:** CPU-only (optional `cuda`/`vulkan` build feature)
- **Windows:** CPU-only — the whisper.cpp Vulkan backend's `vulkan-shaders-gen` sub-build is chronically broken on the Windows CI runner (MAX_PATH/MSBuild), so we ship CPU; re-enable `vulkan` once stabilized
- Microphone permissions deferred until first use (avoids startup permission popup)

## Microphone Permission Detection

**Module:** `src-tauri/src/dictation/permission.rs`

On macOS, microphone access is gated by the TCC (Transparency, Consent, and Control) framework. The `MicPermission` enum tracks the current state:

| State | Meaning |
|-------|---------|
| `NotDetermined` | User hasn't been asked yet — system will prompt on first access |
| `Authorized` | User granted access |
| `Denied` | User denied access — must be changed in System Settings |
| `Restricted` | System policy prevents access (e.g., MDM) |

**API:**
- `MicPermission::check()` — queries `AVCaptureDevice` authorization status via Objective-C bridge (`objc2`, `objc2-av-foundation`)
- `MicPermission::open_settings()` — opens macOS System Settings at the Privacy & Security > Microphone pane

**Platform behavior:**
- **macOS:** Full TCC integration via AVFoundation
- **Linux/Windows:** Always returns `Authorized` (no TCC framework)
