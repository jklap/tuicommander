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
| `streaming.rs` | Streaming transcription loop with adaptive windows and shared speech gates |
| `vad.rs` | Tail-silence detector (energy-based, ported from whisper.cpp; not a whole-window speech gate) |
| `corrections.rs` | Post-processing text corrections |
| `continuous.rs` | Hands-free mode: utterance segmentation and session-bound delivery |
| `speaker.rs` | The reply queue: `Speaker` (bounded, generation-stamped), the `Output` port and its rodio adapter |
| `speech.rs` | The speech port: `Speech`, `SpeechAudio`, `SpeechError`, `SpeechCancel`, `budget_seconds` — no engine named |
| `speech/pocket/` | The Pocket TTS adapter: `pocket.rs` (the port impl), `bundle.rs` (manifest + streaming state), `tokenizer.rs` (SentencePiece Unigram), `engine.rs` (the four ONNX graphs) |
| `speech/external.rs` | The bring-your-own-engine adapter: a configured command plus a RIFF/WAVE reader |
| `echo.rs` | Acoustic echo cancellation: the `Canceller` port, the `FarEnd` pacing buffer and `EchoGuard` |
| `echo/webrtc.rs` | The WebRTC AEC3 adapter |

## Tauri Commands

### Recording

| Command | Description |
|---------|-------------|
| `start_dictation()` | Start recording + streaming transcription |
| `stop_dictation_and_transcribe()` | Stop streaming, final pass on full captured audio, return `TranscribeResponse { text, skip_reason, duration_s, truncated_s }` |
| `inject_text(text)` | Apply corrections to text (called after transcription) |

### Hands-free

| Command | HTTP | Description |
|---------|------|-------------|
| `arm_hands_free_dictation(sessionId, owner)` | `POST /dictation/hands-free/arm` | Bind the delivery target and the audio owner, open a generation, return `HandsFreeStatus`. Refused when the target cannot take a Compose entry. Does **not** open a microphone. |
| `disarm_hands_free_dictation()` | `POST /dictation/hands-free/disarm` | Disarm the whole mode, cancel the voice entries it still owns, return `HandsFreeDisarmed`. Idempotent. |
| `get_hands_free_status()` | `GET /dictation/hands-free` | `HandsFreeStatus`. |

Both transports serialize the same structs, camelCase on the wire:
`HandsFreeStatus { armed, phase, sessionId, owner, generation, pendingText, queuedIds, holdBackMs, error }` and
`HandsFreeDisarmed { wasArmed, generation, cancelled, alreadyDelivered, discardedPending, discardedCapture, status }`.
`sessionId` and `owner` are bounded at 256 bytes and may not be blank.
`phase` is one of `disarmed`, `waiting`, `capturing`, `transcribing`,
`holding_back`, `delivered`, `error`.

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
| `set_dictation_config(config)` | Save dictation configuration (includes `hands_free_hold_back_ms`, `hands_free_activation_phrase` and `speech_command`). Writes the whole document — see "Configuration persistence" |
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
    pub hands_free: Mutex<HandsFree>,
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
    │       │       ├── Skip all-zero window; otherwise use shared speech gates
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

The legacy `vad_simple` helper detects a quiet **tail**, not an entirely silent
window. Streaming must not use it to discard a complete window: a short phrase
followed by a pause still contains speech. Streaming skips only all-zero windows
and passes other audio to the transcriber's shared RMS and speech-confidence
gates. The complete retained recording still goes through the final pass.

The tail detector is ported from whisper.cpp `common.cpp` `vad_simple()`:

- **Algorithm:** Compare absolute energy of last `last_ms` (1000ms) vs entire buffer
- **High-pass filter:** First-order RC at 100Hz removes ambient noise (HVAC, fans)
- **Threshold:** `vad_thold = 0.6` — if `energy_last / energy_all < 0.6`, silence detected
- **Relative:** Microphone gain doesn't affect detection (ratio-based)

## Hands-free mode (`continuous.rs`)

Push-to-talk is unchanged by this module and does not use it. Hands-free is a
separate mode with two independent state machines, neither of which reads a
clock — the caller supplies frame durations and `now_ms`.

**`Segmenter`** cuts the capture stream into utterances over 20 ms frames:

| Setting | Default | Purpose |
|---|---|---|
| `pre_roll_ms` | 300 | audio kept ahead of the first speech frame, so a soft first syllable survives |
| `trailing_silence_ms` | 800 | quiet interval that ends an utterance |
| `min_speech_ms` | 200 | below this the utterance is discarded, not sent |
| `max_utterance_ms` | 30 000 | hard cap; a monologue is cut rather than buffered without limit |
| `activity_rms` | 0.01 | frame RMS at or above which a frame counts as speech |

While nobody speaks the only retained audio is the bounded pre-roll ring, so an
armed microphone in a quiet room holds constant memory and runs no inference.
`vad_simple` is **not** used here: it answers "is the tail quiet", which is a
different question from "where does this utterance end".

**`HandsFree`** owns the mode. Arming binds a target session **and** an audio
owner and opens a generation. A focus change is not an input to this machine at
all, which is what makes a redirected delivery impossible rather than merely
unlikely. Every asynchronous result carries the generation it was captured
under; one that no longer matches is rejected as stale.

A transcript is held back for a visible interval **before** enqueue, so an
unintended turn can be stopped while nothing has been queued yet. A manual abort
disarms the whole mode — pending capture, transcription and the held-back
transcript are discarded, and nothing re-arms by itself. Target closure, audio
owner disconnect and device failure disarm the same way.

**Delivery is the existing Compose queue and nothing else.** `VoiceQueue` is the
only exit, and its one production implementation appends through
`pty::enqueue_voice_command` — the same FIFO, idle gate and id space the Compose
panel uses. There is no PTY write, no `sendCommand`, no submit and no ACP
prompt. A target that cannot take a Compose entry (not an agent PTY session)
is refused at `arm` and at the queue; it stays unavailable, with no fallback.

Queue entries carry ownership: `PendingInjection::VoiceCommand` (wire `kind`
`voice_command`) holds the hands-free generation. `pty::cancel_voice_commands`
removes only entries that are both voice-owned and named by the caller, so a
disarm can never clear a human's Compose command, a peer notice or an exit hint.
Ids that already left the queue come back in `already_delivered` rather than
being reported as cancelled — the composer has them and nothing can retract them.

### The runtime that drives it

The state machines above are inert. `spawn_runtime` starts the thread that feeds
them: it wakes every `POLL_INTERVAL_MS` (50 ms) and calls `tick`, which is the
whole capture -> segment -> transcribe -> hold-back -> enqueue path in one
clock-free function. `tick` takes `now_ms` from the driver, so every test drives
the real production path with a fake clock instead of sleeping.

One tick, in order:

1. read the binding; if the mode is not armed, do nothing;
2. `TargetProbe::accepts` — a session that has gone away disarms with
   `TargetClosed`;
3. `VoiceEndpoint::connected` — a released audio owner disarms with
   `OwnerDisconnected`;
4. `VoiceEndpoint::drain` — an error disarms with `DeviceFailed` carrying the
   device's own message, and a stream that returns no samples at all for
   `DEVICE_SILENCE_TIMEOUT_MS` (5 s) disarms the same way. A silent *room* still
   delivers samples, so this catches a dead device rather than a quiet one;
5. push the samples through the `Segmenter`;
6. for each closed utterance: mark transcribing, read the generation, **release
   the mode lock**, transcribe, re-acquire and offer the transcript. The lock is
   deliberately not held across inference, which is what lets a manual abort
   land mid-transcription and reject the result that arrives after it;
7. `deliver_due` — enqueue whatever the hold-back has now cleared.

Three details that are load-bearing rather than incidental:

- **Never lock the mode in a `match` scrutinee.** `parking_lot` is not
  reentrant and a temporary in the scrutinee lives for the whole `match`, so
  `match deliver_due(&mut mode.lock(), ..)` deadlocks the runtime — and every
  status poll behind it — the first time the queue refuses a delivery. Bind the
  result in its own statement.
- A refused delivery is **not** a disarm: `note_send_failed` records the message
  and returns the mode to `Waiting`, still armed.
- `HandsFreeRuntime` stops and joins its thread on `Drop`, so dropping it out of
  `DictationState` is the only shutdown handshake there is.

### The desktop endpoint, and the one that does not exist yet

`VoiceEndpoint` is the port: a microphone plus a recogniser. Today there is
exactly one adapter, `DesktopVoiceEndpoint` (`commands.rs`), and `arm` accepts
only the owner `DESKTOP_OWNER` (`"desktop"`). Any other owner is refused with
`Audio endpoint '<owner>' is not available on this build` — the browser/remote
endpoint is story 818 and is deliberately absent rather than stubbed, so a remote
client cannot silently be served Boss's local microphone.

The desktop adapter shares push-to-talk's transcriber `Arc` (loading a second
multi-gigabyte Whisper model would be absurd) and its permission and model
helpers, but **not** its capture. `cpal::Stream` is `!Send`, so the capture stays
in `DictationState::hands_free_audio` — a slot distinct from `audio`, which is
push-to-talk's — and only the sample buffer handle crosses to the worker thread.
The two modes therefore cannot arm or stop each other. Because the runtime thread
cannot drop a `!Send` stream, a runtime that disarmed itself is reaped on the
next `hands_free_status` poll, which is where the device is actually released.

`DictationState::shutdown` clears the owner flag and disarms with
`OwnerDisconnected` itself before dropping the runtime, rather than relying on
the thread noticing — the thread may already be parked on its way out.

### The activation phrase

`hands_free_activation_phrase` is optional. Empty, every recognised utterance is
a turn. Set, it must open each new turn, and three properties are the contract:

- **It is local.** Whisper runs on this machine on every utterance *before* the
  gate sees anything — the gate reads text, not audio. A phrase does not reduce
  what is recognised; it reduces what is submitted.
- **It gates new model input, never the microphone.** The match sits in
  `HandsFree::accept_transcript`, after transcription and before the send slot.
  A rejected transcript never enters that slot, and `poll_send` is the only
  thing `deliver_due` can hand to the Compose FIFO — which is the module's only
  exit. "It never reaches PTY, ACP or MCP" is therefore structural, not a check
  repeated per call site.
- **It bounds a conversation, not a sentence.** One accepted turn opens
  `ACTIVATION_WINDOW_MS` (15s) in which follow-ups need no phrase. Every
  accepted turn restarts it; every disarm — manual, target closed, owner
  disconnected, capture failure — closes it, so a fresh arm is always gated.

Matching is on **complete leading words**, where a word is a maximal run of
alphanumeric characters. Case and punctuation therefore never decide a match
(`"Attività, Tuic!"` matches `attività tuic`), Unicode case folding handles
Italian accents, and a longer word that merely starts with the phrase is a
different word rather than a prefix — `Tuicommander` does not match `tuic`. A
phrase heard mid-sentence does not activate: it must lead. The phrase is
stripped from what is submitted, including when it is repeated inside an open
window, so a model never reads it. Spoken alone, it opens the window and submits
nothing.

The window is anchored on **acceptance, not on the model's reply**: this module
enqueues and observes nothing coming back, which is what keeps the Compose FIFO
its only exit. A user who waits out a long answer will need the phrase again.
Story 816-cbbf adds spoken playback and is the first caller that will know when
an answer ended; the anchor is worth revisiting there. Interrupting playback is
that story's work and belongs *upstream* of this gate — stopping a speaker is
not new model input — but whatever stops playback, the words that follow are new
model input and stay gated whenever a phrase is configured.

### Configuration persistence

`set_dictation_config` writes the whole document, and every hands-free field
carries `#[serde(default)]` so a config written before the field existed still
loads. A payload that omits a field therefore **silently resets it** rather than
failing. Any caller that rebuilds a `DictationConfig` must carry every field:
load-modify-save, never build-from-scratch. `dictationStore.saveConfig` reads
the stored config and overrides only the fields the UI owns, and
`src/__tests__/stores/dictation.test.ts` drives that field list off this crate's
struct so a newly added field is covered by whoever adds it.

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

## Pocket TTS speech synthesis (`speech/pocket/`)

Spoken replies are rendered locally by [Pocket TTS](https://github.com/kyutai-labs/pocket-tts),
run in-process over ONNX Runtime. There is no Python, no external service, and
no fallback to either.

Kokoro was here first and was removed. Its Italian voices graded C, Boss
rejected all twelve evaluation samples, and it dragged GPL-3.0 espeak-ng into
an Apache-2.0 repository. That is the reason `speech.rs` is a port and names
no engine: the engine behind it has already changed once.

### Nothing of the model is in this repository

A bundle is a directory downloaded at runtime under the dictation models
directory, not a build artifact:

```text
<config dir>/models/speech/
  onnxruntime/libonnxruntime.dylib     the runtime library, also downloaded
  italian/
    bundle.json  tokenizer.model  *.onnx
    voices/giovanni.safetensors
```

The weights are Kyutai's, CC-BY-4.0, attributed in `THIRD_PARTY_NOTICES.md`.
Every absence is a typed `SpeechError::ModelUnavailable` naming the missing
file, never a panic and never silence.

### `bundle.json` is the contract, not the code

The manifest publishes the streaming-state shape — 18 flow-LM and 56 Mimi
tensors, each with input name, output name, module, key, dtype, fill and shape
— plus sample rate, frame rate, chunk size and text-preparation flags.
`bundle.rs` carries that state between steps, so a bundle for a new language
loads unmodified. This is why English works with no language-specific branch.

A voice file is the flow-LM cache itself, keyed `module/key`. Voices disagree
about its size (Italian `giovanni` holds 94 frames in 12 tensors, English
`cosette` 126 frames in 18, with an extra `pad` key), so `copy_overlap` copies
the voice into the corner of the graph's larger cache instead of assuming a
shape.

### The runtime library is loaded explicitly

`ort` is built in `load-dynamic` mode: its default feature downloads
onnxruntime during the build and then has to be bundled on three platforms,
while here the library travels with the model it serves.

**That mode panics if left to itself.** `ort` resolves the library through the
system loader on first use and `expect`s the result, so a half-finished
download would abort the process. `load_runtime` calls `ort::init_from` on an
explicit path before any session is built, turning it into
`ModelUnavailable`. It memoises only success, so a user who downloads the
runtime after a failed attempt gets speech on the next request rather than
after a restart. `ORT_DYLIB_PATH` still wins when it is set.

### The tokenizer is not the `sentencepiece` crate

That crate statically links protobuf 3.14 while onnxruntime links 3.21, and
the two abort the process on first use. `tokenizer.rs` reads the model proto
with `sentencepiece-model` and runs the Unigram Viterbi through `tokenizers`,
both pure Rust. `ids_match_the_reference_tokenizer` holds it byte-identical to
Python `sentencepiece` on the real Italian bundle.

### Cancellation and the budget

Unlike the Kokoro C ABI, this one can stop: generation is a loop this process
drives, so `SpeechCancel` is checked at the chunk, frame and decode loops and
an abandoned reply stops within a frame.

Generative TTS also runs away — one evaluation candidate produced 195 KB and
675 KB of audio for the same sentence across four runs. `budget_seconds`
states the ceiling once for every adapter, `Bundle::frames_for` converts it to
frames, and `remaining_frames` returns `SpeechError::Runaway` rather than
letting a chunk mint audio the input never warranted.

### Threading

An `ort::Session` is driven by `&mut` while the port hands out `&self`, so the
engine sits behind a mutex and synthesis serialises. One spoken reply at a
time is the actual requirement, not a limitation.

## Bring your own engine (`speech/external.rs`)

The bundled bundles cover English, French, German, Italian, Portuguese and
Spanish. Japanese, Chinese, Korean and Russian are not among them, and no
engine stays the best one for long. `speech_command` in the dictation config
makes the engine a setting: TUICommander hands a command the text and a path
to write, and reads the audio back through the same port the bundled adapter
implements. Neither adapter knows the other exists.

### The template

The command is **argv, not a shell line**. Three markers say where the pieces
go, and each is replaced inside the argument that holds it — so
`--output={out}` works as well as `-o {out}`.

| Marker | Meaning |
|---|---|
| `{out}` | the file the command must write. **Required** |
| `{text}` | the text to speak. Omit it and the text goes to the command's stdin instead |
| `{voice}` | the voice identifier, as the engine spells it |

Japanese, with [piper](https://github.com/rhasspy/piper) — a language the
bundled engine does not reach:

```json
"speech_command": [
  "piper",
  "--model", "/Users/me/voices/ja_JP-test-medium.onnx",
  "--output_file", "{out}"
]
```

piper reads its text on stdin, so there is no `{text}`. macOS `say` takes it
as an argument and names its voices, so both markers appear:

```json
"speech_command": [
  "say", "-v", "{voice}", "-o", "{out}",
  "--data-format=LEF32@22050", "{text}"
]
```

### It runs as you

**The configured command runs as the user who is running TUICommander, with
that user's environment, permissions and files** — exactly like a shell alias
they wrote. Nothing here sandboxes it, and nothing here should: the point of
the setting is to run an engine the application does not know about.

What the application does guarantee is narrower and worth stating exactly:
no shell is involved. The command is spawned directly, so the transcribed
text is one argument (or stdin), never part of a line something parses. A
sentence containing `;` or a backtick is a sentence. A `{voice}` that is not a
plain identifier is rejected as `UnknownVoice` before spawning, because that
one does become argv.

### Failure is never silence

| What the command does | What the port returns |
|---|---|
| is not installed | `ModelUnavailable` naming the program |
| has no `{out}`, or none is configured | `ModelUnavailable` naming what is missing, before anything is spawned |
| exits non-zero | `Failed` with the exit status and the tail of its stderr |
| exits 0 and writes nothing, or an empty file | `Failed` — the silent success this exists to prevent |
| writes something that is not 16-bit PCM or 32-bit float WAVE | `Failed` naming the encoding it did write |
| does not finish in time | `Failed`, after being killed. The budget is 30 s or four times the audio the text justifies, whichever is larger — a cold engine loads a model before it says the first word |
| is abandoned mid-run | `Cancelled`, within a poll interval, with the child killed and reaped |
| returns more audio than the text can justify | `Runaway`, at the same ceiling the bundled adapter stops itself at |

Stereo is mixed down to mono rather than refused. `WAVE_FORMAT_EXTENSIBLE`
headers are read through to the encoding inside them.

## Speaking the replies (`speaker.rs`)

`Speaker` is a bounded queue and one render thread between the model's text and
the speaker. It carries the same generation counter the hands-free state
machine uses for turns, and it exists for one case: **a reply can finish
rendering after the turn it answers is over.**

```text
  say(generation, text)
       │
       ▼
  [queue]  <= MAX_QUEUED (4); a reply whose generation is over never reaches an engine
       │
       ▼
  synthesis   holds a SpeechCancel; both adapters check it as they go
       │
       ▼
  generation checked again   <-- finished, correct audio is discarded here
       │
       ▼
  output.play()
```

Three checks, because a reply can go stale at three moments and only the last
of them has audio to throw away. Cancellation covers the first two; nothing
can cancel a request that already returned.

### What `hush` guarantees

`Speaker::hush()` is what the user talking over the reply calls. It bumps the
generation, clears the queue, cancels the in-flight request and stops the
device, then returns the new generation. It does the device stop **outside**
the state lock and never waits on the render thread, so its cost is an atomic
store rather than the remainder of an utterance.

The render thread plays **while holding the state lock**, on purpose. Released
between the generation check and the `play`, an interruption landing in that
window would bump the generation, stop an idle device, and then have the stale
audio appended behind it. `play` only queues a buffer, so `hush` waits on the
order of a mixer append.

Two `hush` calls are two turns. That is not an accident of the counter: two
interruptions in a row *are* two turns, and a reply addressed to the first one
is no more current than a reply addressed to the turn before it.

### Failures are reported, never retried in place

| What happens | What the queue does |
|---|---|
| the engine cannot render (missing bundle, bad voice) | records the message in `SpeakerStatus.last_error`, drops the reply, takes the next one |
| the device refuses the audio | same, and the render thread stays alive |
| the engine returns `Cancelled` | nothing — somebody asked for that |
| a fifth reply is queued | refused as `SpeakError::Full` while the queue is still short enough to drain |
| a reply arrives for a turn that is over | refused as `SpeakError::Stale`, before any engine is called |
| the `Speaker` is dropped | shutdown flag, queue cleared, in-flight request cancelled, device stopped, worker joined |

`last_error` is cleared by the next reply that plays, so the UI cannot show a
problem that is over.

### The engine's own sample rate reaches the device

`DeviceOutput` passes `SpeechAudio::sample_rate` straight through and lets
rodio resample to whatever the device wants. Pocket TTS renders at 24 kHz; a
user-supplied command renders at whatever its engine likes, and assuming a rate
here would pitch-shift every external engine. A zero rate or an empty buffer is
rejected before the buffer is built, because `SamplesBuffer::new` **panics** on
a zero rate and a panic on the render thread would take the queue down with no
message.

### Echo cancellation is not in this file

Deciding *when* to interrupt means hearing the user over the speaker, which is
acoustic echo cancellation. Whoever detects near-end speech calls `hush`; this
module has no microphone and no opinion. The split is deliberate — the queue's
correctness is provable without audio hardware, and an audio-hardware test
cannot prove the queue. It lives in `echo.rs`, below.

## Hearing the user over our own voice (`echo.rs`)

The microphone hears the speaker. Left alone, the energy VAD in `continuous.rs`
opens a turn on the reply the application is speaking, transcribes it, and
answers itself. Muting capture while speaking would stop that and would also
stop the user interrupting, which is the one thing hands-free has to get right,
so the capture stream stays open and the echo is subtracted from it instead.

```
speaker::render_loop ──note_rendered(SpeechAudio)──┐
                                                   v
                                              ┌─────────┐
                                              │ FarEnd  │  16 kHz, ≤2 s
                                              └────┬────┘
                                                   │ take(n), padded with silence
 capture (mono 16 kHz) ──clean(&[f32])──> EchoGuard┴──> Canceller ──> cleaned capture
                                          10 ms frames          │        │
                                          remainder carried     │        v
                                                                │   segmenter.push
                                          hush ──note_stopped───┘
```

### The two streams must arrive in step

A canceller subtracts a delayed, filtered copy of the **far end** (what we
played) from the **near end** (what the microphone heard). It can only do that
if it is fed both at the same pace: one 10 ms frame of each, in turn.

Our two sources do not behave that way. Capture arrives in small chunks as the
device produces them, while `speaker.rs` hands over a whole rendered utterance
at once — seconds of audio in one call, before a single sample of it has left
the speaker. Pushing that straight into a canceller would put it seconds ahead
of the microphone, and it would subtract nothing.

`FarEnd` is the buffer that fixes the pace: the reply goes in whole and comes
out only as fast as capture is consumed, padded with silence whenever nothing is
playing. Alignment is therefore by **sample count**, not by wall clock, and the
residual offset — the device's own output latency — is what the canceller's
delay estimator is for. `note_stopped` (called on `hush`) clears the buffer,
because audio that will never be heard must never be subtracted.

The buffer is bounded at two seconds. It is only reached when playback and
capture are out of step — the mode was disarmed mid-reply, or the device stopped
delivering — and the oldest samples are dropped, counted in `dropped_far_end()`.
Holding more would not improve cancellation: audio that old no longer
corresponds to anything the microphone is about to hear.

### Everything runs at 16 kHz

Not a preference. `audio.rs` already converts capture to mono 16 kHz for
Whisper, and 16 kHz is one of the rates the WebRTC APM accepts, so the near end
needs no conversion at all. The far end is whatever the speech engine rendered —
24 kHz for Pocket TTS, anything at all for a user-supplied command — and is
resampled on the way in. That resampling is **linear**, not the nearest-neighbour
`audio.rs` uses for Whisper: a canceller subtracts a waveform, and
nearest-neighbour's step artefacts are error it would have to model.

### The port exists so the pacing is provable without hardware

| Adapter | What it is for |
|---|---|
| `webrtc::WebRtc` | The real one: WebRTC AEC3, one instance, 10 ms frames |
| `PassThrough` | No cancellation. A build without a canceller, and the tests for everything in `EchoGuard` that is not the subtraction itself |

`PassThrough` is not a silent fallback — whoever installs it owns saying so. It
exists because a hands-free mode that refuses to arm is worse than one that
cannot be interrupted over the speaker: headphones still work.

AEC3 runs with `stream_delay_ms: None`, so it estimates the offset itself. We
could not supply an honest number anyway: the far end is handed over when the
reply is *rendered*, and how long the OS and the device then hold it before it
reaches the air is not something this process is told. One consequence worth
knowing — the estimator has to converge, so the first fraction of a second of a
reply is cancelled poorly or not at all.

The render frame is **analyzed, never processed**. The APM offers to filter the
playback stream too; by the time we see it that audio is already on its way to
the device, so any change to our copy would be a change nobody hears while the
microphone still hears the original.

Noise suppression, gain control and high-pass filtering are all left off. The
same capture stream feeds Whisper, which was trained on speech that has not been
gated or gain-ridden.

### The fork this needs (`patches/webrtc-audio-processing-sys/`)

`webrtc-audio-processing` 2.1.0 is used with the `bundled` feature — it
statically links the APM rather than looking for a system library, because there
is one on Linux, a brew-only one on macOS, and none at all on Windows. Upstream
CI runs `ubuntu-latest` and `macos-latest` only, and the `bundled` build does not
work on Windows at all; four of our five patches exist for that, and the fifth
for a macOS link failure. They are listed, with the exact symptom each one fixes,
in the `[patch.crates-io]` comment in `src-tauri/Cargo.toml`.

#### Build prerequisites

Every platform needs **meson**, **ninja**, a C++20 compiler and **libclang**
(for bindgen) on `PATH`, plus network access: the meson wrap fetches
abseil-cpp 20240722.0 from github.com while the build runs. None of this
reaches `tuic-remote` — `cargo tree --no-default-features -i
webrtc-audio-processing` matches no packages, so the headless binary needs
neither meson nor ninja.

| Platform | Extra | Why |
|---|---|---|
| Windows | `set CARGO_TARGET_DIR=C:\t` | abseil's `hashtablez_sampler_force_weak_definition.cc` resolves one character past `MAX_PATH` (260) from the meson build directory under a normal target path. `cl` reports it as `C1083: Cannot open source file` for a file that is plainly there. Not fixable by a patch — it is a path-length limit. |
| Linux | a libclang with its builtin headers (`clang-devel`, not the pip `libclang` wheel) | The wheel ships `libclang.so` and no resource directory, so bindgen fails on `'stddef.h' file not found`. Recoverable with `BINDGEN_EXTRA_CLANG_ARGS=-I/usr/lib/gcc/<triple>/<ver>/include`. |
| macOS | — | Xcode's clang and a brew meson are enough. |

**Do not `tar` the fork on a Mac without `COPYFILE_DISABLE=1`.** bsdtar writes
an AppleDouble `._abseil-cpp.wrap` beside the real one, meson globs `*.wrap`,
and the build dies with `UnicodeDecodeError: 'utf-8' codec can't decode byte
0xa3` — a message that points at the wrap file rather than at the sidecar.

#### Packaging evidence

A probe that builds the fork, links it, and asserts the canceller measurably
changes a 440 Hz capture frame:

| Platform | Toolchain | Result |
|---|---|---|
| macOS 27 (arm64) | Xcode clang, meson 1.12.0 | `PROBE OK` |
| Windows 10.0.26200 (x86_64) | VS 2022 BuildTools 14.44.35207, meson 1.12.0 | `PROBE OK` |
| RHEL 9.4 (x86_64) | gcc 11, meson 1.11.2 | `PROBE OK` |

Linux is not a formality here: patch 3 replaced the pkg-config abseil lookup
with an unconditional vendored subproject, and Linux is the one platform that
*has* a system abseil for pkg-config to find. It is therefore the platform
that patch most plausibly broke, and the only one upstream CI covered.
