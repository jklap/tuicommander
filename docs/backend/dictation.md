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
| `speech/assets.rs` | The pinned, allowlisted catalogue and its verifying downloader: `Asset`, `Fetch`, `status`, `stage`, `promote`, `remove` |
| `speech/library.rs` | `SpeechLibrary`: one engine per language, loaded lazily, and the order that keeps installing from racing speaking |
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

### Speech assets

The catalogue is an allowlist: `asset` is an id from it, and an id that is not
in it is refused before it can become a path or a URL. See "Where the bytes come
from" below.

| Command | Description |
|---------|-------------|
| `get_speech_assets()` | Every installable asset with its state: `absent`, `downloading`, `incomplete` (with the missing files named) or `ready` |
| `download_speech_asset(asset)` | Download and install, verifying every sha256 (emits `speech-download-progress`) |
| `cancel_speech_download(asset)` | Abandon a download in flight; succeeds with a note when there was none |
| `delete_speech_asset(asset)` | Unload the engine, then remove the files |

### Spoken replies

Available only while hands-free is armed **and** the conversation opened with a
working voice. Every one of them answers `available: false` with a reason rather
than failing, so a caller can always ask.

| Command | HTTP | Description |
|---------|------|-------------|
| `speak_reply(text, turn?)` | `POST /dictation/speech/speak` | Queue one reply, at most 2000 characters. Returns `SpokenReply { utteranceId, state, error?, turn }` with `state: "queued"` — never `"finished"`. `turn` refuses a reply written for a turn the user has already talked over; omitting it means "now". |
| `stop_speech()` | `POST /dictation/speech/stop` | Stop now, drop the queue, open a new turn. Returns the `SpeechStatus` afterwards, so the caller learns the new turn. |
| `get_speech_status(utterance?)` | `GET /dictation/speech/status?utterance=` | `SpeechStatus`. With `utterance` it also carries that one reply's `SpokenReply`; an id this conversation no longer remembers comes back as `state: "unknown"` rather than as an absent field. |

A model does not call these. It calls the `voice` MCP tool, which supplies its
own identity so the binding can be checked — see "Who may speak" below.

### Configuration

| Command | Description |
|---------|-------------|
| `get_dictation_status()` | Model status, recording/processing state, and normalized `audio_level` (0–1). The preview polls this shared IPC/HTTP response while recording. |
| `get_dictation_config()` | Load dictation configuration (includes `rms_threshold` and `no_speech_threshold` — see "Speech gates") |
| `set_dictation_config(config)` | Save dictation configuration (includes `hands_free_hold_back_ms`, `hands_free_activation_phrase`, `hands_free_notify_model`, `speech_command` and `speech_voice`). Writes the whole document — see "Configuration persistence" |
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
    /// The armed conversation's reply queue and voice. `None` while nothing is
    /// armed, and while a conversation is armed without a working voice.
    pub speaker: Mutex<Option<speaker::Armed>>,
}
```

Managed as Tauri state alongside `AppState`.

`speaker` is emptied **first** on both `shutdown` and `disarm_hands_free`, before
the engine and the runtime go: dropping the queue cancels the reply in flight and
stops the device, which is what makes disarm revoke speech whether or not the
model ever acknowledged anything.

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
endpoint is story 832-e730 and is deliberately absent rather than stubbed, so a
remote client cannot silently be served Boss's local microphone. Nothing in the
UI can reach that refusal either: `SettingsPanel.tsx` hides the whole Dictation
tab outside Tauri, so a browser client is never offered a control that cannot
work.

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

### Telling the model the mode changed (821-842a)

A model cannot see a microphone open. The `voice` tool is listed whether or not
anything is armed, and a model with no reason to speak writes text — so without
a notice the feature ships a voice nobody ever hears. Two constants in
`continuous.rs` carry it:

| Constant | Sent when | Says |
|---|---|---|
| `MODE_ENTRY_HINT` | `arm_hands_free_with`, before the runtime starts | what arrives from now on was spoken, and the `voice` tool can answer out loud |
| `MODE_EXIT_HINT` | both disarm paths | the tool can no longer speak here; reply as text |

`hands_free_notify_model` (default **true**) controls both. Off sends neither
and changes nothing else — in particular, a disarm still revokes speech, because
that is a fact about this machine rather than a message to a model.

Five rules, and each of them is a test:

- **The Compose FIFO, like everything else.** `deliver_entry_hint` goes through
  `VoiceQueue::enqueue`, so a notice queues behind whatever the terminal is
  doing, waits out the busy/dialog gate, and lands in the session the mode bound
  to rather than in whatever tab the user has since focused. It is one line, for
  the reason `compose_entry` gives: the queue types an entry and submits it, and
  a newline submits half of it.
- **A parked notice is owned, so it is cancellable.** `note_hint_enqueued`
  records the id in `owned` and in `entry_hint`, so a disarm pulls it back out
  of the FIFO like any other voice entry. It does **not** move the phase: the
  phase describes what the user's speech is doing, and nothing has been said.
- **The exit notice is owed only to a model that read the entry notice.** The
  evidence is the cancellation: an id reported `cancelled` was pulled back
  before the composer typed it, so the model never read it and an exit notice
  after it would be the only thing it ever heard about a mode it never had. An
  id reported `already_delivered` cannot be retracted, so the model believes it
  can speak and has to be told otherwise. That is `deliver_exit_hint`, and it is
  why a rapid arm/disarm leaves no contradictory pair.
- **Driven by what this arm sent, never by the setting as it now reads.** A user
  who turns the notices off mid-conversation has changed what the *next* arm
  says; a model already holding "you can answer out loud" still gets its exit
  notice. `disarm_hands_free` therefore never re-reads the config.
- **Both disarm paths, not just the user's.** `report_exit_hint` is called from
  `disarm_hands_free` and from the runtime thread's `Tick::Disarmed` arm, so a
  closed target, a lost owner and a dead microphone all end the model's
  expectation too. A target that has gone away refuses the enqueue; that is
  reported, not swallowed — there is nobody left to tell.

A refused entry notice does not fail the arm. The microphone works and the
Compose queue works for ordinary turns; the reason lands in
`HandsFreeStatus::error` via `note_send_failed`, and the mode owns nothing, so
no exit notice follows either.

Push-to-talk shares none of this. It never calls `arm_hands_free`, so it opens
no VAD runtime, reads no activation phrase, makes no speech available and sends
no notice — asserted at the surface in
`push_to_talk_alone_arms_nothing_and_tells_the_model_nothing`.

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

### Where the bytes come from (`speech/assets.rs`)

The catalogue is Rust, not a JSON file beside the binary, and that is the whole
security property: a manifest the installer could rewrite is not an allowlist.
Every file is named by a URL pinned to an immutable revision and checked against
a sha256 written in the source, so an upstream that is compromised — or merely
re-tagged — produces a refused install rather than a different model.

| Part | Published by | Source | Pinned to |
|---|---|---|---|
| ONNX graphs, tokenizer, `bundle.json` | `KevinAHM/pocket-tts-onnx` (exports of Kyutai's weights) | Hugging Face | commit `58a6d00c` |
| Speaker embeddings (voices) | Kyutai, **re-published by us** | TUICommander release | tag `speech-voices-v1` |
| onnxruntime | Microsoft | GitHub release | `v1.23.0` |

Two upstreams for one reason: `kyutai/pocket-tts` is a **gated** repository, so
an application cannot download the voices on a user's behalf — not even their
metadata is readable without a token. CC-BY-4.0 allows redistribution with
attribution, so the embedding files (4.6 MB each, against 125 MB for a
language) are re-published unmodified on our own release. The graphs are public
and are not re-hosted.

**onnxruntime is pinned at 1.23.0 rather than the newest release.** 1.24 dropped
the macOS Intel and universal2 builds and TUICommander still ships for Intel
Macs. `ort` 2.0.0-rc.13 needs API version 17 or later and 1.23 provides 23, so
nothing is given up.

Microsoft ships a whole SDK, of which one shared library is wanted. The member
is found by rule, not by a hardcoded path: a regular file under a `lib/`
component whose name starts with the platform stem and a dot. The trailing dot
is what excludes `libonnxruntime_providers_shared.so`; the `lib/` component is
what excludes `pkgconfig/libonnxruntime.pc`; "regular file" is what excludes the
symlinks Linux uses for the unversioned name. macOS ships both
`libonnxruntime.dylib` and `libonnxruntime.1.23.0.dylib` as real files, so an
exact name is preferred rather than required — and two candidates with no exact
match are refused rather than guessed between.

### Installing is two steps, and the split is the point

```text
download and verify into .staging   <- no lock held; the engine may be speaking
unload the engine for this language <- waits for the sentence in flight
rename .staging into place          <- microseconds, nothing can speak
next synthesis loads the new files  <- lazily, as it always did
```

The long part holds nothing; the part that excludes synthesis is a rename.
`SpeechLibrary` (`speech/library.rs`) owns that order — `assets.rs` deliberately
does not take the lock itself, so the two cannot be collapsed by accident.

The old directory is removed *before* the new one is renamed in, which leaves a
window where neither exists. That is the right way round: a crash inside it
leaves the language absent, which the status query reports honestly and the user
can fix by downloading again. The other order can leave a directory half
belonging to each version, which passes every existence check and then fails
somewhere inside onnxruntime.

**Readiness is checked by size, not by hash.** The hash is verified once, while
the bytes are arriving; re-reading 125 MB to answer a status query would make
opening a settings panel cost a disk sweep. A file truncated after install still
shows the wrong size, which is the failure this has to catch. The three states
are distinct on purpose — `absent` offers a download, `incomplete` names what is
missing, and only `ready` lets a language be used.

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

### Utterance identity: accepting a reply is not the user hearing it

`say` returns a `UtteranceId`, and the queue remembers the last 64 of them with
what became of each. A caller polls `Speaker::utterance(id)` — through
`get_speech_status` or the `voice` tool — to find out.

| State | Means |
|---|---|
| `queued` | accepted, nothing has been rendered |
| `rendering` | synthesis is running |
| `speaking` | handed to the device |
| `finished` | the device went quiet with this reply behind it — **the only state that means somebody heard it** |
| `interrupted` | the turn ended first, or `hush` arrived mid-sentence |
| `failed: <reason>` | never audible; the reason is the engine's or the device's |

The distinction is structural, not documentary. `finished` is written in exactly
one place — `note_playback_drained`, which runs only when `Output::is_speaking()`
is false. And `hush` reads the device **before** stopping it, so a reply already
heard to the end is not rewritten as `interrupted` just because an interruption
followed it.

The device reports one boolean for its whole queue, so replies handed to it
resolve together when it goes quiet. That is correct rather than approximate: it
drains in order.

Finding out costs a timed condvar wait of 25 ms — but only while something is
actually playing. An idle speaker blocks on the condvar and never queries the
device at all.

### Who may speak

Speech belongs to a conversation, not to the application. `Caller` is the whole
rule:

| Caller | May drive |
|---|---|
| `Owner` — the user's own UI, on either transport | always |
| `Model(session)` — an MCP connection, named by the TUIC session it is bound to | only the conversation armed for that same session |

An MCP connection with no TUIC session is refused outright rather than falling
back to `Owner`: that path exists for the user's UI, and handing it to a model
would let any client speak into whichever conversation happened to be armed.
`status` checks the binding too — the fields alone say what somebody else's
conversation is doing, and a model that can see a queue will try to speak into
it.

### The same speech contract on every transport (817-f67c)

There is one implementation per operation and every transport reaches it. That
is the parity, and it is structural rather than mirrored by hand:

| Transport | Entry point | Caller |
|---|---|---|
| Tauri IPC | `#[tauri::command] speak_reply` / `stop_speech` / `get_speech_status` | `Owner` |
| HTTP | `dictation_routes.rs` → the **same** `dictation::commands` functions | `Owner` |
| MCP (HTTP and the collapsed `call_tool` path) | `mcp_transport.rs::handle_voice` | `Model(session)` |

The `Owner` surfaces are the user's own UI on either transport; the MCP surface
is the model's, and the split is the ownership rule in *Who may speak* above.
There is no WebSocket surface for speech and none is wanted: a WS lane is for
high-frequency streams, and a reply produces a handful of state changes.

Three tests hold it. `command_table_paths_all_hit_a_registered_route` proves
every `COMMAND_TABLE` speech path resolves to a registered route.
`the_speech_status_wire_shape_names_every_field_a_client_reads` and
`a_reply_looks_the_same_whether_it_was_accepted_or_polled_for` pin the two wire
shapes — the second compares the *keys* of an accepted reply against the keys of
the same reply polled for, because the render thread moves the state between the
two reads and a value comparison would be a race dressed as a contract.
`voice_binds_to_the_calling_terminal_on_the_direct_and_collapsed_paths` proves
the MCP identity survives both dispatch paths.

**The push half is missing, on purpose and under protest.** Nothing is emitted
when an utterance changes state, so every consumer polls `speech_status`; that
part is at least equal on all transports. What is *not* equal is
`speech-download-progress`, a desktop-only `emit` with no `/events` arm — a
browser or PWA client sees an asset download start and finish with nothing in
between, and `dictation-download-progress` beside it has the same gap. Bridging
either one needs a new `AppEvent` variant, and `state.rs` has been held
uncommitted by another agent since 2026-09-21. Both sites carry a dated
`DEFERRED` comment saying so. Story **833-6fd4** owns the work.

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

### Barge-in: who actually calls `hush`

The capture loop in `continuous.rs` does, through the `Interruptible` port it
holds for the armed conversation. The rule is three lines and each one is
load-bearing:

- **After the canceller.** `capture.echo.lock().clean(...)` runs first, so
  whatever opens the energy gate is the user and not the reply coming back
  through the microphone. Wired the other way round, every reply interrupts
  itself on its own first word.
- **On the edge, not the level.** `hush` opens a new turn on *every* call, so a
  level trigger would open one per 50 ms tick and every reply the model wrote
  for the turn in progress would be refused as stale while the user was still
  speaking one sentence.
- **The slot, not the queue.** The port is `commands::ArmedSpeaker`, which holds
  `DictationState.speaker` itself rather than the `Speaker` that was in it when
  the loop started. Under Auto there *is* no queue at arm time — the language is
  unknown until somebody speaks — so a port bound to one queue would be bound to
  nothing for the whole conversation. It `try_lock`s, because this runs on the
  capture loop and anything it waits for delays the next chunk of the user's own
  voice; the only writer is a rebuild in `speak`, which has already hushed the
  queue it is replacing.

The queue is built in `arm_hands_free_with` **before** the runtime is spawned
whenever a language is already known, for this reason alone: a loop started
first would spend its first ticks unable to interrupt anything.

### What barge-in measures (816-cbbf)

The barge-in tests above this one all use `Subtract`, a perfect non-adaptive
canceller. It answers *is the wiring right* and cannot answer *how long does it
take* or *how often does it fire when nobody spoke*, because a perfect
subtraction leaves no residual to misjudge.
`talking_over_the_reply_stops_it_without_losing_the_first_words` in
`continuous.rs` drives the shipping AEC3 adapter through the same `tick` and
turns those into numbers. Measured 2026-09-22, macOS 27 arm64:

| | |
|---|---|
| Stop latency | **50 ms** — one `POLL_INTERVAL_MS`, the floor. Cancellation and detection add nothing measurable on top of the poll. |
| False triggers | **0** over 1200 ms of reply reaching the microphone with no user speech at all |
| Pre-roll preserved | **280 ms** of audio ahead of the user's first sample, out of the 300 ms ring |
| Utterance length | **1720 ms**, against 280 ms of pre-roll plus 600 ms of speech — nothing was truncated |

The fixture is a **room model, not a recording**: the microphone hears the
rendered reply 40 ms late and at 0.35 of its level, with the user's voice added
on top at a different pitch so the canceller cannot subtract the user by
subtracting the echo and still look correct. That is the signal an echo
canceller is specified against — a linear path with a delay — which is what
makes the numbers comparable between runs. It deliberately does not model
reverberation, the microphone's noise floor, or a speaker driven into
distortion.

`without_the_canceller_the_same_room_interrupts_the_reply_on_its_own_echo` is
the control, and the only reason the zero above means anything: the same room
with `PassThrough` installed *must* interrupt the reply before the user has said
a word. Without it, a false-trigger count of zero would be equally consistent
with a fixture too quiet to trip anything.

**Neither is a substitute for a real microphone and a real speaker in a room.**
That probe is in `to-test.md`.

### The eight states the conversation has to survive (820-21a5)

Story 820 names eight scenarios and asks for each one to be verified rather than
argued about. They are spread across `continuous.rs` and `commands.rs` because
they belong to two different layers — the mode's own bookkeeping, and the
commands that arm and disarm it — so this table is the index. A row with no test
beside it is a gap, not an omission from the documentation.

| Scenario | Held by |
|---|---|
| Optional activation, on and off | `an_empty_activation_phrase_lets_every_turn_through`, `speech_without_the_activation_phrase_never_reaches_the_queue`, `a_configured_phrase_gates_a_turn_and_is_stripped_from_what_is_sent` |
| Phrase-only timeout | `the_phrase_alone_opens_the_window_without_sending_anything`, `follow_up_speech_inside_the_window_needs_no_phrase`, `speech_after_the_window_expires_needs_the_phrase_again` |
| Hold-back cancellation | `nothing_is_enqueued_before_the_hold_back_expires`, `an_abort_inside_the_hold_back_sends_nothing` |
| Manual disarm | `a_manual_abort_disarms_the_whole_mode_and_discards_the_pending_send`, `disarming_a_mode_that_was_never_armed_reports_no_work` |
| Busy or dialog target | `a_busy_target_or_one_holding_a_dialog_parks_the_turn_and_stays_a_target`, `arming_against_a_target_that_cannot_take_a_compose_entry_is_refused` |
| Target closure | `a_closed_target_disarms_the_running_mode`, `a_closed_target_disarms_and_a_different_session_does_not`, `closing_the_bound_session_disarms_the_running_mode_and_releases_the_device` |
| Owner disconnect | `a_disconnected_owner_disarms_the_running_mode`, `a_disconnected_owner_disarms_and_a_different_owner_does_not` |
| No stale submission | `a_transcript_from_a_previous_generation_cannot_send`, `a_transcription_that_finishes_after_a_disarm_never_reaches_the_queue`, `an_abort_during_transcription_lands_and_its_result_is_refused` |
| No stale playback | `disarming_while_a_reply_is_playing_stops_the_device`, `a_reply_written_for_a_turn_the_user_talked_over_is_refused` |

Two of those rows are worth reading before changing anything near them.

**Busy is not closed.** `PtyTargetProbe::accepts` asks whether the target exists
and can take voice at all — never what it is doing. A probe that also answered
"is it free right now" would end the conversation on the first reply the user
asked for. Waiting is the Compose queue's job, and it is the *only* exit from
this module: a second delivery path would be a way to type into a working agent
or an open permission prompt. That is why the busy row asserts on
`VoiceEnqueued::typed` for three sessions at once — idle, busy, and idle with a
confident question — with the idle one as the control. On its own, "it parked"
is equally consistent with a fixture that could never deliver anything.

**Disarming silences by dropping, not by hushing.** `disarm_hands_free` sets the
speaker slot to `None`; `Drop for Speaker` cancels the render in flight and calls
`Output::stop`. Asserting that the slot is empty is therefore not the same
assertion as asserting the room went quiet, and only the second one is what a
user experiences. The test uses an output that reports itself speaking until it
is stopped, because `QuietOutput` is never speaking and cannot tell a silenced
device from one nobody ever asked to stop.

## The language of the conversation

Boss's requirement is one sentence: **the model and the voice use the language
Whisper is transcribing, and nothing replies in English by accident.** Four
places enforce it, and they all read the same source.

| Where | What it does |
|---|---|
| `transcribe.rs` | `TranscribeResult.language` — the two-letter code whisper used, read off `full_lang_id_from_state()` |
| `continuous.rs` | `HandsFree.turn_language`, set by `accept_transcript`, cleared on arm and disarm |
| `continuous.rs` | `compose_entry` appends `(reply in <Name>)` to every voice entry |
| `commands.rs` | `speech_language` picks the voice, and `Armed.language` records which one |

**There is one language, not one per subsystem.** The dictation setting is the
source when it names a language; under `auto` the source is what whisper
actually detected, carried out of the transcript by `Transcript.language`. There
is no separate TTS language setting, and the `voice` MCP tool takes no language
and no voice — a model that could pass either would be a second source, and the
two would disagree the first time the user switched languages.

**`auto` before the first turn has no language, and that is a state.**
`speech_status` reports `available: false` with a reason naming Auto, and
`speak` refuses. It is the one moment in a conversation where no reply can be
spoken, and inventing English to fill it is exactly the bug this exists to
prevent. The voice opens on the first `speak` after somebody has spoken.

**Confidence is not available on this path.** `whisper_full_lang_id_from_state`
returns the id and nothing else, so an ambiguous detection is indistinguishable
from a certain one. The probabilities live behind `WhisperState::lang_detect`,
which needs its own mel and encoder pass and would double the cost of every
utterance. What *is* handled is a missing answer: an id that maps to no code
leaves `language: None`, which reaches the model as the transcript alone — no
requirement rather than a guessed one.

**A language with no bundle is named, never substituted.** `for_language_code`
answers `None` for Korean, and the status says `No speech bundle ships for
language "ko"`. Picking the Italian voice because it is installed is how an
assistant answers a Korean conversation in Italian.

**Which of the language's voices speaks is a setting; which language speaks is
not.** `speech_voice` names one of the voices the chosen language ships — the
Dictation panel offers them from `voices` on the asset, and the setting is empty
in an untouched configuration, which `choose_voice` reads as "whatever this
language ships first". That is also what every configuration written before the
setting existed says, so no migration is needed. A user-supplied engine ignores
the setting: it names its own voices inside its command template.

**A named voice the language does not ship is an error, not a fall back.**
`choose_voice` answers with the offered names instead of speaking in the first
one. Two things get you here — a catalogue that dropped a voice, and a language
the user changed underneath the setting — and in both, speaking in a voice
nobody chose is worse than saying why nothing was spoken. The message reaches
the user through the hands-free status rather than a log.

**The requirement travels in the Compose entry.** `esegui i test` is queued as
`esegui i test (reply in Italian)`. In the entry rather than in a mode hint,
because hints are optional and this is not; on one line, because the queue types
the entry into a terminal and submits it, and a newline in the middle submits
half a sentence. The English name comes from `dictation/language.rs`, whose
table is checked against `WHISPER_LANGUAGES` in `src/stores/dictation.ts` by a
test that reads the TypeScript.

**Changing the language stops the replies written for the old one.**
`save_dictation_config` drops `DictationState.speaker` when `language`,
`speech_command` or `speech_voice` moves — and on nothing else, because cutting
a reply off mid-word because somebody moved a threshold slider would be the
worse bug. A voice belongs to a conversation as much as a language does: a
sentence half said in one voice does not finish in another. The
next `speak` opens a voice for the language now configured. The same rebuild
happens mid-conversation under Auto: `speak` compares `Armed.language` against
the turn language and, when they differ, hushes the old queue before building
the new one. That `hush` is what invalidates the replies in flight — it opens a
new turn, and `say` refuses anything addressed to the turn before it.

**Lock order: the mode, then the speaker.** Resolving a language takes
`hands_free`; rebuilding takes `speaker`. `speak` and `speech_status` both
resolve everything they need from the mode *before* taking the speaker lock, and
`open_speaker_for` is handed the language rather than looking it up, so the
rebuild under the speaker lock cannot invert the order.

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
