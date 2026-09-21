//! Continuous (hands-free) dictation: utterance segmentation and safe delivery.
//!
//! Two separate machines live here, and keeping them apart is the point:
//!
//! * [`Segmenter`] turns a stream of audio frames into bounded utterances. It
//!   owns pre-roll, end-of-speech and the maximum utterance length, and it never
//!   grows a buffer while nobody is speaking.
//! * [`HandsFree`] owns the mode: what it is bound to, which generation is
//!   current, the visible hold-back before a send, and what a disarm discards.
//!
//! Neither reads a clock. Both take `now_ms` (or frame durations) from the
//! caller, so a test states the timeline instead of sleeping through it and a
//! loaded machine cannot turn a behaviour assertion into a flake.
//!
//! Delivery is a port: [`VoiceQueue`]. The only production implementation is
//! [`PtyVoiceQueue`], which appends to the existing Compose FIFO through
//! `pty::enqueue_voice_command`. There is deliberately no other way out of this
//! module — no PTY write, no `sendCommand`, no submit, no ACP prompt.

use crate::state::{AppState, VoiceCancellation};

// ---------------------------------------------------------------------------
// Utterance segmentation
// ---------------------------------------------------------------------------

/// Capture sample rate. Must match `audio::AudioCapture`.
pub const SAMPLE_RATE: u32 = 16_000;

/// Analysis frame. 20 ms is short enough that end-of-speech lands inside one
/// trailing-silence budget, and long enough that the RMS of one frame is a
/// meaningful energy measurement rather than a sample-level accident.
pub const FRAME_MS: u32 = 20;

/// Samples in one analysis frame.
pub const FRAME_SAMPLES: usize = (SAMPLE_RATE as usize * FRAME_MS as usize) / 1000;

/// How an utterance ended. The caller reports a truncated one differently: the
/// speaker was cut off, so the text is not the whole thought.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UtteranceEnd {
    /// The configured quiet interval elapsed.
    Silence,
    /// The maximum utterance length was reached while the speaker kept going.
    MaxLength,
}

/// One bounded utterance, pre-roll included.
#[derive(Clone, Debug, PartialEq)]
pub struct Utterance {
    pub audio: Vec<f32>,
    pub end: UtteranceEnd,
    /// Milliseconds of *active* audio, pre-roll and trailing silence excluded.
    pub speech_ms: u32,
}

/// Utterance boundaries, all in milliseconds except the energy floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmenterConfig {
    /// Audio retained before speech starts, so a soft first syllable survives.
    pub pre_roll_ms: u32,
    /// Quiet interval that ends an utterance.
    pub trailing_silence_ms: u32,
    /// Utterances with less active audio than this are discarded, not sent.
    pub min_speech_ms: u32,
    /// Hard cap on a single utterance.
    pub max_utterance_ms: u32,
    /// Frame RMS at or above which a frame counts as speech.
    pub activity_rms: f32,
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        Self {
            pre_roll_ms: 300,
            trailing_silence_ms: 800,
            min_speech_ms: 200,
            max_utterance_ms: 30_000,
            activity_rms: 0.01,
        }
    }
}

/// Speech-boundary state machine over fixed-size frames.
///
/// While nobody is speaking the only retained audio is the pre-roll ring, so an
/// armed-and-silent microphone holds a constant, small amount of memory however
/// long it stays armed — and runs no inference at all.
pub struct Segmenter {
    config: SegmenterConfig,
    /// Samples not yet forming a whole frame.
    remainder: Vec<f32>,
    /// Bounded pre-roll, only while idle.
    pre_roll: std::collections::VecDeque<f32>,
    /// `Some` while an utterance is open.
    open: Option<OpenUtterance>,
}

struct OpenUtterance {
    audio: Vec<f32>,
    speech_ms: u32,
    silence_ms: u32,
}

impl Segmenter {
    pub fn new(config: SegmenterConfig) -> Self {
        Self {
            config,
            remainder: Vec::with_capacity(FRAME_SAMPLES),
            pre_roll: std::collections::VecDeque::new(),
            open: None,
        }
    }

    /// True while an utterance is open — what the UI renders as "capturing".
    pub fn is_capturing(&self) -> bool {
        self.open.is_some()
    }

    /// Samples currently retained. Bounded by the pre-roll while idle and by
    /// `max_utterance_ms` while capturing; the assertion a silence test makes.
    pub fn retained_samples(&self) -> usize {
        self.remainder.len()
            + self.pre_roll.len()
            + self.open.as_ref().map_or(0, |open| open.audio.len())
    }

    /// Feed captured audio. Returns every utterance that closed inside this
    /// chunk — a long chunk can close more than one.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Utterance> {
        let mut closed = Vec::new();
        self.remainder.extend_from_slice(samples);
        let mut start = 0;
        while start + FRAME_SAMPLES <= self.remainder.len() {
            let frame_range = start..start + FRAME_SAMPLES;
            if let Some(utterance) = self.push_frame(frame_range.clone()) {
                closed.push(utterance);
            }
            start += FRAME_SAMPLES;
        }
        self.remainder.drain(..start);
        closed
    }

    /// Drop everything captured so far.
    ///
    /// Nothing calls it in production today: a disarm ends the runtime, and the
    /// next arm builds a fresh `Capture`, so the segmenter is never reused
    /// across generations. It stays because that is the invariant — if a future
    /// caller ever keeps one alive across an arm, this is what it must call.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.remainder.clear();
        self.pre_roll.clear();
        self.open = None;
    }

    fn push_frame(&mut self, range: std::ops::Range<usize>) -> Option<Utterance> {
        let active = frame_rms(&self.remainder[range.clone()]) >= self.config.activity_rms;
        let frame: &[f32] = &self.remainder[range];
        match self.open.as_mut() {
            None => {
                if active {
                    let mut audio: Vec<f32> = self.pre_roll.drain(..).collect();
                    audio.extend_from_slice(frame);
                    self.open = Some(OpenUtterance {
                        audio,
                        speech_ms: FRAME_MS,
                        silence_ms: 0,
                    });
                } else {
                    let cap = ms_to_samples(self.config.pre_roll_ms);
                    self.pre_roll.extend(frame.iter().copied());
                    while self.pre_roll.len() > cap {
                        self.pre_roll.pop_front();
                    }
                }
                None
            }
            Some(open) => {
                open.audio.extend_from_slice(frame);
                if active {
                    open.speech_ms += FRAME_MS;
                    open.silence_ms = 0;
                } else {
                    open.silence_ms += FRAME_MS;
                }
                let end = if open.audio.len() >= ms_to_samples(self.config.max_utterance_ms) {
                    Some(UtteranceEnd::MaxLength)
                } else if open.silence_ms >= self.config.trailing_silence_ms {
                    Some(UtteranceEnd::Silence)
                } else {
                    None
                };
                end.and_then(|end| self.close(end))
            }
        }
    }

    /// Close the open utterance, discarding it when it holds too little speech.
    fn close(&mut self, end: UtteranceEnd) -> Option<Utterance> {
        let open = self.open.take()?;
        self.pre_roll.clear();
        if open.speech_ms < self.config.min_speech_ms {
            return None;
        }
        Some(Utterance {
            audio: open.audio,
            end,
            speech_ms: open.speech_ms,
        })
    }
}

fn ms_to_samples(ms: u32) -> usize {
    (SAMPLE_RATE as usize * ms as usize) / 1000
}

fn frame_rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum: f32 = frame.iter().map(|s| s * s).sum();
    (sum / frame.len() as f32).sqrt()
}

// ---------------------------------------------------------------------------
// Hands-free mode
// ---------------------------------------------------------------------------

/// What the mode is bound to while armed.
///
/// Both halves are pinned at arming. `session_id` is the delivery target and
/// `owner` is the audio endpoint that armed it — the desktop adapter, or one
/// remote client. A focus change touches neither, which is the whole reason
/// this struct exists instead of reading "the active tab" at send time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub session_id: String,
    pub owner: String,
}

/// What the user sees. Model state and audio state are separate elsewhere;
/// this is the capture/delivery half.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Disarmed,
    /// Armed, microphone open, nobody speaking.
    Waiting,
    Capturing,
    Transcribing,
    /// Transcribed and visible, counting down the hold-back before enqueue.
    HoldingBack,
    /// Handed to the Compose queue.
    Delivered,
    Error,
}

impl Phase {
    /// The label both transports report. IPC and HTTP serialize the same
    /// status struct, so this string is the only spelling a client ever sees.
    pub fn as_wire(&self) -> &'static str {
        match self {
            Self::Disarmed => "disarmed",
            Self::Waiting => "waiting",
            Self::Capturing => "capturing",
            Self::Transcribing => "transcribing",
            Self::HoldingBack => "holding_back",
            Self::Delivered => "delivered",
            Self::Error => "error",
        }
    }
}

/// Why the mode disarmed. Every one of these is terminal: nothing re-arms by
/// itself, per Boss's decision that a manual abort kills the mode rather than
/// the utterance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisarmReason {
    /// Hotkey or mic button.
    Manual,
    TargetClosed,
    OwnerDisconnected,
    DeviceFailed(String),
}

/// Why arming was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArmError {
    AlreadyArmed,
    /// The target cannot accept Compose-queue entries (not an agent PTY
    /// session, or an ACP target). It stays unavailable — there is no fallback.
    UnsupportedTarget,
}

/// What a disarm leaves for the caller to finish.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disarmed {
    /// The generation that was current. Anything asynchronous still carrying it
    /// is stale from here on.
    pub generation: u64,
    /// The target the cancelled ids were enqueued against. Carried here because
    /// the binding is gone by the time a caller sees this, and cancelling needs
    /// the session name the entries are parked under.
    pub session_id: String,
    /// Voice-owned queue ids to cancel. Only ever entries this mode enqueued.
    pub cancel_ids: Vec<u64>,
    /// A transcript was waiting out its hold-back and never reached the queue.
    pub discarded_pending: bool,
    /// An utterance was open or a transcription was in flight.
    pub discarded_capture: bool,
    pub reason: DisarmReason,
}

/// What happened to a transcript handed back by the transcriber.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscriptOutcome {
    /// Accepted and visible; it will be enqueued at `send_at_ms` unless the
    /// user disarms first.
    HeldBack { send_at_ms: u64 },
    /// Carried an old generation — a result from before a disarm or re-arm.
    Stale,
    /// Nothing was recognised, so nothing is sent.
    Empty,
    /// The mode is not armed; late results cannot resurrect it.
    NotArmed,
}

/// A transcript whose hold-back has expired, ready for the Compose queue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceSend {
    pub generation: u64,
    pub session_id: String,
    pub text: String,
}

struct PendingSend {
    generation: u64,
    text: String,
    send_at_ms: u64,
}

/// The hands-free mode state machine.
pub struct HandsFree {
    hold_back_ms: u64,
    generation: u64,
    binding: Option<Binding>,
    phase: Phase,
    pending: Option<PendingSend>,
    /// Queue ids this mode owns, in enqueue order.
    owned: Vec<u64>,
    last_error: Option<String>,
}

impl HandsFree {
    pub fn new(hold_back_ms: u64) -> Self {
        Self {
            hold_back_ms,
            generation: 0,
            binding: None,
            phase: Phase::Disarmed,
            pending: None,
            owned: Vec::new(),
            last_error: None,
        }
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    /// The hold-back this mode will apply to the next transcript.
    pub fn hold_back_ms(&self) -> u64 {
        self.hold_back_ms
    }

    /// Take the configured hold-back. Refused while armed: changing it under a
    /// transcript that is already counting down would move a deadline the user
    /// is currently watching.
    pub fn set_hold_back_ms(&mut self, hold_back_ms: u64) {
        if self.binding.is_none() {
            self.hold_back_ms = hold_back_ms;
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn binding(&self) -> Option<&Binding> {
        self.binding.as_ref()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// The transcript currently waiting out its hold-back, for the UI to show.
    pub fn pending_text(&self) -> Option<&str> {
        self.pending.as_ref().map(|pending| pending.text.as_str())
    }

    /// Bind a target session and an audio owner, and open a new generation.
    ///
    /// `target_supported` is the caller's answer to "can this session take a
    /// Compose-queue entry"; a `false` keeps the mode unavailable rather than
    /// arming it against a target that would need a bypass to reach.
    pub fn arm(
        &mut self,
        session_id: impl Into<String>,
        owner: impl Into<String>,
        target_supported: bool,
    ) -> Result<u64, ArmError> {
        if self.binding.is_some() {
            return Err(ArmError::AlreadyArmed);
        }
        if !target_supported {
            return Err(ArmError::UnsupportedTarget);
        }
        self.generation += 1;
        self.binding = Some(Binding {
            session_id: session_id.into(),
            owner: owner.into(),
        });
        self.phase = Phase::Waiting;
        self.last_error = None;
        Ok(self.generation)
    }

    /// An utterance opened.
    pub fn note_capturing(&mut self) {
        if self.binding.is_some() {
            self.phase = Phase::Capturing;
        }
    }

    /// An utterance closed and went to the transcriber.
    pub fn note_transcribing(&mut self) {
        if self.binding.is_some() {
            self.phase = Phase::Transcribing;
        }
    }

    /// Hand back a transcription result. `generation` is the one the capture
    /// carried, which is how a result that outlived its mode is rejected.
    pub fn accept_transcript(
        &mut self,
        generation: u64,
        text: &str,
        now_ms: u64,
    ) -> TranscriptOutcome {
        if self.binding.is_none() {
            return TranscriptOutcome::NotArmed;
        }
        if generation != self.generation {
            return TranscriptOutcome::Stale;
        }
        if text.trim().is_empty() {
            self.phase = Phase::Waiting;
            return TranscriptOutcome::Empty;
        }
        let send_at_ms = now_ms + self.hold_back_ms;
        self.pending = Some(PendingSend {
            generation,
            text: text.trim().to_string(),
            send_at_ms,
        });
        self.phase = Phase::HoldingBack;
        TranscriptOutcome::HeldBack { send_at_ms }
    }

    /// Take the pending transcript once its hold-back has expired.
    ///
    /// Returns `None` while the hold-back is still running, which is what makes
    /// the hold-back visible *and* cancellable: nothing has been enqueued yet.
    pub fn poll_send(&mut self, now_ms: u64) -> Option<VoiceSend> {
        let ready = self
            .pending
            .as_ref()
            .is_some_and(|pending| now_ms >= pending.send_at_ms);
        if !ready {
            return None;
        }
        let pending = self.pending.take()?;
        let session_id = self.binding.as_ref()?.session_id.clone();
        Some(VoiceSend {
            generation: pending.generation,
            session_id,
            text: pending.text,
        })
    }

    /// Record the queue id the Compose FIFO gave a delivered voice entry.
    pub fn note_enqueued(&mut self, generation: u64, id: u64) {
        if generation == self.generation && self.binding.is_some() {
            self.owned.push(id);
            self.phase = Phase::Delivered;
        }
    }

    /// Queue ids this mode still owns.
    pub fn owned_ids(&self) -> &[u64] {
        &self.owned
    }

    /// Forget an id the queue has typed. Nothing can retract it any more, so
    /// keeping it would make a later cancel claim a delivered message back.
    pub fn note_delivered(&mut self, id: u64) {
        self.owned.retain(|owned| *owned != id);
    }

    /// The Compose queue refused a delivery.
    ///
    /// The mode stays armed — the target is still bound and the next utterance
    /// may well land — but the failure is reported rather than swallowed, so a
    /// message that never reached a model does not look like one that did.
    pub fn note_send_failed(&mut self, message: &str) {
        if self.binding.is_some() {
            self.last_error = Some(message.to_string());
            self.phase = Phase::Waiting;
        }
    }

    /// Disarm the whole mode. Never re-arms itself; `arm` is the only way back.
    pub fn disarm(&mut self, reason: DisarmReason) -> Option<Disarmed> {
        let session_id = self.binding.as_ref()?.session_id.clone();
        let discarded_capture = matches!(self.phase, Phase::Capturing | Phase::Transcribing);
        let disarmed = Disarmed {
            generation: self.generation,
            session_id,
            cancel_ids: std::mem::take(&mut self.owned),
            discarded_pending: self.pending.take().is_some(),
            discarded_capture,
            reason: reason.clone(),
        };
        self.binding = None;
        self.phase = match &reason {
            DisarmReason::DeviceFailed(message) => {
                self.last_error = Some(message.clone());
                Phase::Error
            }
            _ => Phase::Disarmed,
        };
        Some(disarmed)
    }

    /// The bound target went away.
    pub fn note_session_closed(&mut self, session_id: &str) -> Option<Disarmed> {
        if self.binding.as_ref()?.session_id != session_id {
            return None;
        }
        self.disarm(DisarmReason::TargetClosed)
    }

    /// The audio owner went away — a closed browser tab, a lost WS client.
    pub fn note_owner_disconnected(&mut self, owner: &str) -> Option<Disarmed> {
        if self.binding.as_ref()?.owner != owner {
            return None;
        }
        self.disarm(DisarmReason::OwnerDisconnected)
    }

    /// The capture device failed.
    pub fn note_device_failed(&mut self, message: impl Into<String>) -> Option<Disarmed> {
        self.disarm(DisarmReason::DeviceFailed(message.into()))
    }
}

// ---------------------------------------------------------------------------
// Delivery port
// ---------------------------------------------------------------------------

/// The only exit from hands-free capture to a model.
pub trait VoiceQueue {
    /// Append to the Compose FIFO. Returns the entry's queue id.
    fn enqueue(&self, session_id: &str, text: &str, generation: u64) -> Result<u64, String>;
    /// Drop the named voice-owned entries that are still parked.
    fn cancel(&self, session_id: &str, ids: &[u64]) -> VoiceCancellation;
}

/// Production adapter: the existing Compose queue, nothing else.
pub struct PtyVoiceQueue<'a>(pub &'a AppState);

impl VoiceQueue for PtyVoiceQueue<'_> {
    fn enqueue(&self, session_id: &str, text: &str, generation: u64) -> Result<u64, String> {
        crate::pty::enqueue_voice_command(self.0, session_id, text, generation)
            .map(|enqueued| enqueued.id)
    }

    fn cancel(&self, session_id: &str, ids: &[u64]) -> VoiceCancellation {
        crate::pty::cancel_voice_commands(self.0, session_id, ids)
    }
}

/// Deliver a transcript whose hold-back expired, and record its queue identity.
///
/// Split out so the enqueue/record pair cannot drift apart: an id recorded
/// without an enqueue would cancel a stranger's entry, and an enqueue without a
/// recorded id would leave a voice message no disarm can retract.
pub fn deliver_due(
    mode: &mut HandsFree,
    queue: &dyn VoiceQueue,
    now_ms: u64,
) -> Option<Result<u64, String>> {
    let send = mode.poll_send(now_ms)?;
    match queue.enqueue(&send.session_id, &send.text, send.generation) {
        Ok(id) => {
            mode.note_enqueued(send.generation, id);
            Some(Ok(id))
        }
        Err(error) => Some(Err(error)),
    }
}

// ---------------------------------------------------------------------------
// Capture port and the runtime loop
// ---------------------------------------------------------------------------

/// One armed capture endpoint: a microphone and the recogniser behind it.
///
/// The desktop adapter is `commands::DesktopVoiceEndpoint`. The browser/remote
/// endpoint is Step 8 (story 818) and is deliberately absent rather than
/// stubbed — an unimplemented endpoint must refuse to arm, not arm and go deaf.
pub trait VoiceEndpoint: Send {
    /// Audio captured since the last call, empty when nothing arrived yet.
    /// `Err` is a hard capture failure and disarms the mode.
    fn drain(&mut self) -> Result<Vec<f32>, String>;
    /// False once the endpoint that armed the mode is gone.
    fn connected(&self) -> bool;
    /// Transcribe one closed utterance. Called without the mode lock held.
    fn transcribe(&self, audio: &[f32]) -> Result<String, String>;
}

/// Whether the bound session can still take a Compose-queue entry.
pub trait TargetProbe {
    fn accepts(&self, session_id: &str) -> bool;
}

/// Production probe: the same predicate `arm` checked, re-asked every tick.
pub struct PtyTargetProbe<'a>(pub &'a AppState);

impl TargetProbe for PtyTargetProbe<'_> {
    fn accepts(&self, session_id: &str) -> bool {
        crate::pty::session_accepts_voice(self.0, session_id)
    }
}

/// How long a stream may deliver nothing before it counts as a dead device.
///
/// A live microphone in a silent room still delivers samples — silence is a
/// value, not an absence. No samples at all means the stream stopped, which
/// cpal reports through a callback the capture owner cannot return from.
pub const DEVICE_SILENCE_TIMEOUT_MS: u64 = 5_000;

/// Everything the loop carries between ticks.
pub struct Capture {
    segmenter: Segmenter,
    /// When audio last arrived, for the starvation rule above.
    last_audio_ms: u64,
    device_silence_timeout_ms: u64,
}

impl Capture {
    pub fn new(config: SegmenterConfig, device_silence_timeout_ms: u64, now_ms: u64) -> Self {
        Self {
            segmenter: Segmenter::new(config),
            last_audio_ms: now_ms,
            device_silence_timeout_ms,
        }
    }

    /// Retained audio. Nothing in production needs this number; it exists so
    /// the bounded-memory rule can be asserted rather than asserted about.
    #[allow(dead_code)]
    pub fn retained_samples(&self) -> usize {
        self.segmenter.retained_samples()
    }
}

/// What one tick did.
#[derive(Debug)]
pub enum Tick {
    /// Nothing to do: the mode is not armed. The driver stops.
    NotArmed,
    /// Still armed. `enqueued` is the queue id a hold-back that expired this
    /// tick produced; `send_error` is a delivery the queue refused.
    Running {
        enqueued: Option<u64>,
        send_error: Option<String>,
    },
    /// The mode disarmed itself. The driver cancels what it owned and stops.
    Disarmed(Disarmed),
}

/// One pass of the hands-free runtime: check the bindings still hold, segment
/// whatever audio arrived, transcribe what closed, and deliver what is due.
///
/// Clock-free like the machines it drives — `now_ms` comes from the driver, so
/// a test states the timeline instead of sleeping through it.
///
/// The mode lock is taken in short sections and **released across
/// `transcribe`**: a whisper pass takes seconds, and an abort issued during one
/// has to be able to land. That is what makes the generation check on the way
/// back out load-bearing rather than decorative.
pub fn tick(
    capture: &mut Capture,
    mode: &parking_lot::Mutex<HandsFree>,
    endpoint: &mut dyn VoiceEndpoint,
    target: &dyn TargetProbe,
    queue: &dyn VoiceQueue,
    now_ms: u64,
) -> Tick {
    let Some(binding) = mode.lock().binding().cloned() else {
        return Tick::NotArmed;
    };

    if !target.accepts(&binding.session_id) {
        return disarmed_or_not_armed(mode.lock().note_session_closed(&binding.session_id));
    }
    if !endpoint.connected() {
        return disarmed_or_not_armed(mode.lock().note_owner_disconnected(&binding.owner));
    }

    let samples = match endpoint.drain() {
        Ok(samples) => samples,
        Err(error) => return disarmed_or_not_armed(mode.lock().note_device_failed(error)),
    };
    if samples.is_empty() {
        if now_ms.saturating_sub(capture.last_audio_ms) > capture.device_silence_timeout_ms {
            return disarmed_or_not_armed(mode.lock().note_device_failed(format!(
                "The capture device delivered no audio for {}s",
                capture.device_silence_timeout_ms / 1_000
            )));
        }
    } else {
        capture.last_audio_ms = now_ms;
    }

    let closed = capture.segmenter.push(&samples);
    if closed.is_empty() && capture.segmenter.is_capturing() {
        mode.lock().note_capturing();
    }

    for utterance in closed {
        // The generation is read with the lock, before the pass starts. What
        // comes back is checked against the generation *then* current, which is
        // how a result that outlived its mode is refused.
        let generation = {
            let mut mode = mode.lock();
            mode.note_transcribing();
            mode.generation()
        };
        match endpoint.transcribe(&utterance.audio) {
            Ok(text) => {
                mode.lock().accept_transcript(generation, &text, now_ms);
            }
            Err(error) => {
                return disarmed_or_not_armed(mode.lock().note_device_failed(error));
            }
        }
    }

    // Bound in its own statement, never in the `match` scrutinee: a temporary
    // there lives until the end of the whole match, and the error arm locks the
    // mode again. `parking_lot` is not reentrant, so that shape deadlocks the
    // runtime — and every status poll behind it — the first time the queue
    // refuses a delivery.
    let delivered = deliver_due(&mut mode.lock(), queue, now_ms);
    match delivered {
        Some(Ok(id)) => Tick::Running {
            enqueued: Some(id),
            send_error: None,
        },
        Some(Err(error)) => {
            mode.lock().note_send_failed(&error);
            Tick::Running {
                enqueued: None,
                send_error: Some(error),
            }
        }
        None => Tick::Running {
            enqueued: None,
            send_error: None,
        },
    }
}

/// How often the driver thread ticks. Short enough that end-of-speech is not
/// noticeably late, long enough to cost nothing while nobody is speaking.
pub const POLL_INTERVAL_MS: u64 = 50;

/// The thread that drives [`tick`] while the mode is armed.
///
/// Dropping it stops the thread and waits for it — bounded by one poll plus,
/// at worst, one transcription in flight.
pub struct HandsFreeRuntime {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl HandsFreeRuntime {
    /// True once the loop has returned — it disarmed itself, or the mode was
    /// disarmed from elsewhere. The caller uses this to release the capture
    /// device, which cannot travel to this thread (cpal streams are `!Send`).
    pub fn is_finished(&self) -> bool {
        self.handle
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished)
    }
}

impl Drop for HandsFreeRuntime {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Run the hands-free loop against a real session until it disarms.
///
/// The thread owns nothing but the audio endpoint: the target probe and the
/// queue are both built from `AppState` on each tick, so this path is the same
/// Compose FIFO a hand-typed command uses and there is no second way out.
pub fn spawn_runtime(
    state: std::sync::Arc<AppState>,
    mode: std::sync::Arc<parking_lot::Mutex<HandsFree>>,
    mut endpoint: Box<dyn VoiceEndpoint>,
    config: SegmenterConfig,
) -> HandsFreeRuntime {
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_clone = stop.clone();
    let handle = std::thread::Builder::new()
        .name("hands-free-dictation".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            let mut capture = Capture::new(config, DEVICE_SILENCE_TIMEOUT_MS, 0);
            loop {
                if stop_clone.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                let now_ms = started.elapsed().as_millis() as u64;
                let outcome = tick(
                    &mut capture,
                    &mode,
                    endpoint.as_mut(),
                    &PtyTargetProbe(&state),
                    &PtyVoiceQueue(&state),
                    now_ms,
                );
                match outcome {
                    Tick::NotArmed => break,
                    Tick::Disarmed(disarmed) => {
                        // Whatever this mode parked in the Compose queue goes
                        // with it. An entry the composer already typed is
                        // reported by `cancel`, not silently claimed back.
                        let cancellation = cancel_disarmed(&PtyVoiceQueue(&state), &disarmed);
                        tracing::info!(
                            source = "dictation",
                            "Hands-free disarmed: {:?} (cancelled {:?}, already delivered {:?})",
                            disarmed.reason,
                            cancellation.cancelled,
                            cancellation.already_delivered
                        );
                        break;
                    }
                    Tick::Running {
                        enqueued,
                        send_error,
                    } => {
                        if let Some(id) = enqueued {
                            tracing::info!(
                                source = "dictation",
                                "Hands-free turn queued as Compose entry {id}"
                            );
                        }
                        if let Some(error) = send_error {
                            tracing::warn!(
                                source = "dictation",
                                "Hands-free delivery refused: {error}"
                            );
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
            }
        })
        .expect("Failed to spawn hands-free thread");
    HandsFreeRuntime {
        stop,
        handle: Some(handle),
    }
}

/// A disarm that found nothing to disarm means someone else got there first.
fn disarmed_or_not_armed(disarmed: Option<Disarmed>) -> Tick {
    match disarmed {
        Some(disarmed) => Tick::Disarmed(disarmed),
        None => Tick::NotArmed,
    }
}

/// Cancel everything a disarm made obsolete.
pub fn cancel_disarmed(queue: &dyn VoiceQueue, disarmed: &Disarmed) -> VoiceCancellation {
    if disarmed.cancel_ids.is_empty() {
        return VoiceCancellation::default();
    }
    queue.cancel(&disarmed.session_id, &disarmed.cancel_ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const SR: f32 = SAMPLE_RATE as f32;

    fn speech(ms: u32) -> Vec<f32> {
        let n = ms_to_samples(ms);
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR).sin() * 0.5)
            .collect()
    }

    fn silence(ms: u32) -> Vec<f32> {
        vec![0.0; ms_to_samples(ms)]
    }

    fn test_config() -> SegmenterConfig {
        SegmenterConfig {
            pre_roll_ms: 200,
            trailing_silence_ms: 400,
            min_speech_ms: 100,
            max_utterance_ms: 2_000,
            activity_rms: 0.01,
        }
    }

    // --- Segmentation -----------------------------------------------------

    /// The defect 811-9313 fixed one layer down, stated as a boundary rule: a
    /// phrase that ends in a pause is exactly what a hands-free utterance is,
    /// so the pause must close it and the speech must survive whole.
    #[test]
    fn a_phrase_that_ends_in_a_pause_closes_and_keeps_its_speech() {
        let mut segmenter = Segmenter::new(test_config());
        let mut input = speech(500);
        input.extend(silence(600));

        let closed = segmenter.push(&input);

        assert_eq!(
            closed.len(),
            1,
            "the pause must close exactly one utterance"
        );
        assert_eq!(closed[0].end, UtteranceEnd::Silence);
        assert!(
            closed[0].speech_ms >= 480,
            "the whole phrase is speech, got {}ms",
            closed[0].speech_ms
        );
        assert!(
            closed[0].audio.len() >= ms_to_samples(500),
            "the utterance must carry the phrase, not a trimmed window"
        );
    }

    /// A soft first syllable is below the activity floor, so without pre-roll
    /// the utterance starts a frame late and the word is clipped.
    ///
    /// Measured against the same input with the pre-roll turned off, because a
    /// length assertion on its own is vacuous here: the utterance also carries
    /// its trailing silence, which is longer than the pre-roll and hides its
    /// absence completely.
    #[test]
    fn speech_carries_the_pre_roll_that_preceded_it() {
        let mut input = silence(1_000);
        input.extend(speech(300));
        input.extend(silence(600));

        let mut with_pre_roll = Segmenter::new(test_config());
        let mut without = Segmenter::new(SegmenterConfig {
            pre_roll_ms: 0,
            ..test_config()
        });

        let kept = with_pre_roll.push(&input);
        let clipped = without.push(&input);

        assert_eq!(kept.len(), 1);
        assert_eq!(clipped.len(), 1);
        let pre_roll = kept[0].audio.len() - clipped[0].audio.len();
        assert!(
            pre_roll >= ms_to_samples(200) - FRAME_SAMPLES,
            "expected ~200ms of audio ahead of the first speech frame, got {pre_roll} samples"
        );
    }

    /// A cough or a chair is speech-shaped for one frame. Sending it would put
    /// noise in front of a model.
    #[test]
    fn a_blip_too_short_to_be_speech_sends_nothing() {
        let mut segmenter = Segmenter::new(test_config());
        let mut input = speech(40);
        input.extend(silence(600));

        assert!(
            segmenter.push(&input).is_empty(),
            "40ms is below the 100ms floor and must be discarded"
        );
    }

    /// A pause *inside* a sentence is shorter than the quiet interval and must
    /// not split the sentence in two turns.
    #[test]
    fn a_short_pause_inside_a_sentence_does_not_split_it() {
        let mut segmenter = Segmenter::new(test_config());
        let mut input = speech(300);
        input.extend(silence(200));
        input.extend(speech(300));
        input.extend(silence(600));

        let closed = segmenter.push(&input);

        assert_eq!(closed.len(), 1, "one sentence, one utterance");
        assert!(closed[0].audio.len() >= ms_to_samples(800));
    }

    /// Someone who never stops talking must not grow an unbounded buffer, and
    /// must not stall delivery forever either.
    #[test]
    fn an_endless_monologue_is_cut_at_the_maximum_length() {
        let mut segmenter = Segmenter::new(test_config());

        let closed = segmenter.push(&speech(5_000));

        assert!(!closed.is_empty(), "the cap must close an utterance");
        assert_eq!(closed[0].end, UtteranceEnd::MaxLength);
        assert!(
            closed[0].audio.len() <= ms_to_samples(2_000) + FRAME_SAMPLES,
            "an utterance may not exceed the configured maximum"
        );
        assert!(
            segmenter.retained_samples() <= ms_to_samples(2_000) + FRAME_SAMPLES,
            "and the segmenter may not hold more than one utterance's worth"
        );
    }

    /// An armed microphone in a quiet room is the steady state of this feature.
    /// It may not accumulate audio, and it may not produce work.
    #[test]
    fn silence_never_grows_a_buffer_or_produces_an_utterance() {
        let mut segmenter = Segmenter::new(test_config());
        let pre_roll = ms_to_samples(200);

        for _ in 0..600 {
            assert!(segmenter.push(&silence(100)).is_empty());
        }

        assert!(
            segmenter.retained_samples() <= pre_roll + FRAME_SAMPLES,
            "60s of silence retained {} samples; the pre-roll is {pre_roll}",
            segmenter.retained_samples()
        );
        assert!(!segmenter.is_capturing());
    }

    /// Audio arrives in device-sized chunks that do not align to frames. The
    /// boundaries must not depend on where the chunks happen to split.
    #[test]
    fn chunking_does_not_change_the_boundaries() {
        let mut input = speech(500);
        input.extend(silence(600));

        let mut whole = Segmenter::new(test_config());
        let expected = whole.push(&input);

        let mut split = Segmenter::new(test_config());
        let mut closed = Vec::new();
        for chunk in input.chunks(377) {
            closed.extend(split.push(chunk));
        }

        assert_eq!(
            closed, expected,
            "frame alignment must not leak into the cut"
        );
    }

    // --- Mode -------------------------------------------------------------

    fn armed() -> HandsFree {
        let mut mode = HandsFree::new(1_500);
        mode.arm("target", "desktop", true).expect("arm");
        mode
    }

    #[test]
    fn an_unsupported_target_stays_unavailable() {
        let mut mode = HandsFree::new(1_500);

        assert_eq!(
            mode.arm("ego", "desktop", false),
            Err(ArmError::UnsupportedTarget)
        );
        assert_eq!(*mode.phase(), Phase::Disarmed);
        assert!(
            mode.binding().is_none(),
            "a refused arm must not bind anything"
        );
    }

    /// The reason this story exists: whatever the user clicks on afterwards,
    /// the transcript goes to the session that was bound when they armed.
    #[test]
    fn delivery_follows_the_bound_session_not_the_focus() {
        let mut mode = armed();
        let generation = mode.generation();
        mode.note_capturing();
        mode.note_transcribing();

        // A focus change is not an input to this machine at all — nothing here
        // reads "the active tab", which is what makes the redirect impossible.
        assert_eq!(
            mode.accept_transcript(generation, "run the tests", 0),
            TranscriptOutcome::HeldBack { send_at_ms: 1_500 }
        );
        let send = mode.poll_send(1_500).expect("hold-back expired");

        assert_eq!(send.session_id, "target");
        assert_eq!(send.text, "run the tests");
    }

    #[test]
    fn nothing_is_enqueued_before_the_hold_back_expires() {
        let mut mode = armed();
        let generation = mode.generation();
        mode.accept_transcript(generation, "delete everything", 1_000);

        assert!(
            mode.poll_send(2_499).is_none(),
            "the hold-back must still be running"
        );
        assert_eq!(*mode.phase(), Phase::HoldingBack);
        assert_eq!(mode.pending_text(), Some("delete everything"));
        assert!(mode.poll_send(2_500).is_some());
    }

    /// Boss's rule: the abort kills the mode, not just the utterance.
    #[test]
    fn a_manual_abort_disarms_the_whole_mode_and_discards_the_pending_send() {
        let mut mode = armed();
        let generation = mode.generation();
        mode.accept_transcript(generation, "never send this", 0);

        let disarmed = mode.disarm(DisarmReason::Manual).expect("was armed");

        assert!(disarmed.discarded_pending);
        assert_eq!(*mode.phase(), Phase::Disarmed);
        assert!(mode.binding().is_none());
        assert!(
            mode.poll_send(u64::MAX).is_none(),
            "a discarded transcript may not surface later"
        );
        assert_eq!(
            mode.accept_transcript(generation, "late result", 10_000),
            TranscriptOutcome::NotArmed,
            "a disarmed mode must not re-arm itself on the next phrase"
        );
    }

    #[test]
    fn a_transcript_from_a_previous_generation_cannot_send() {
        let mut mode = armed();
        let stale_generation = mode.generation();
        mode.disarm(DisarmReason::Manual);
        mode.arm("target", "desktop", true).expect("re-arm");

        assert_eq!(
            mode.accept_transcript(stale_generation, "from the last session", 0),
            TranscriptOutcome::Stale
        );
        assert!(mode.poll_send(u64::MAX).is_none());
    }

    #[test]
    fn an_empty_transcription_sends_nothing_and_returns_to_waiting() {
        let mut mode = armed();
        let generation = mode.generation();
        mode.note_transcribing();

        assert_eq!(
            mode.accept_transcript(generation, "   ", 0),
            TranscriptOutcome::Empty
        );
        assert_eq!(*mode.phase(), Phase::Waiting);
        assert!(mode.poll_send(u64::MAX).is_none());
    }

    #[test]
    fn a_closed_target_disarms_and_a_different_session_does_not() {
        let mut mode = armed();

        assert!(mode.note_session_closed("someone-else").is_none());
        assert!(mode.binding().is_some());

        let disarmed = mode.note_session_closed("target").expect("bound target");
        assert_eq!(disarmed.reason, DisarmReason::TargetClosed);
        assert!(mode.binding().is_none());
    }

    #[test]
    fn a_disconnected_owner_disarms_and_a_different_owner_does_not() {
        let mut mode = armed();

        assert!(mode.note_owner_disconnected("browser-42").is_none());
        assert!(mode.binding().is_some());

        assert_eq!(
            mode.note_owner_disconnected("desktop")
                .expect("bound owner")
                .reason,
            DisarmReason::OwnerDisconnected
        );
    }

    #[test]
    fn a_failed_device_disarms_into_a_reported_error() {
        let mut mode = armed();
        mode.note_capturing();

        let disarmed = mode
            .note_device_failed("input device disappeared")
            .expect("armed");

        assert!(disarmed.discarded_capture);
        assert_eq!(*mode.phase(), Phase::Error);
        assert_eq!(mode.last_error(), Some("input device disappeared"));
        assert!(mode.binding().is_none());
    }

    // --- Delivery ---------------------------------------------------------

    /// A queue that records what it was asked to do, and nothing else — the
    /// point being that a delivery path with any other exit would show up here
    /// as a message that never reached `enqueue`.
    #[derive(Default)]
    struct FakeQueue {
        next_id: RefCell<u64>,
        enqueued: RefCell<Vec<(String, String, u64, u64)>>,
        /// Ids the queue has already typed; a cancel cannot retract them.
        delivered: RefCell<Vec<u64>>,
        parked: RefCell<Vec<u64>>,
        fail: RefCell<Option<String>>,
    }

    impl FakeQueue {
        fn mark_delivered(&self, id: u64) {
            self.parked.borrow_mut().retain(|parked| *parked != id);
            self.delivered.borrow_mut().push(id);
        }
    }

    impl VoiceQueue for FakeQueue {
        fn enqueue(&self, session_id: &str, text: &str, generation: u64) -> Result<u64, String> {
            if let Some(error) = self.fail.borrow().as_ref() {
                return Err(error.clone());
            }
            let mut next = self.next_id.borrow_mut();
            *next += 1;
            self.enqueued.borrow_mut().push((
                session_id.to_string(),
                text.to_string(),
                generation,
                *next,
            ));
            self.parked.borrow_mut().push(*next);
            Ok(*next)
        }

        fn cancel(&self, _session_id: &str, ids: &[u64]) -> VoiceCancellation {
            let mut cancellation = VoiceCancellation::default();
            for id in ids {
                if self.parked.borrow().contains(id) {
                    self.parked.borrow_mut().retain(|parked| parked != id);
                    cancellation.cancelled.push(*id);
                } else {
                    cancellation.already_delivered.push(*id);
                }
            }
            cancellation
        }
    }

    #[test]
    fn a_held_back_transcript_reaches_the_queue_with_its_ownership() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeQueue::default();
        mode.accept_transcript(generation, "run the tests", 0);

        assert!(deliver_due(&mut mode, &queue, 1_000).is_none());
        let delivered = deliver_due(&mut mode, &queue, 1_500).expect("hold-back expired");

        assert_eq!(delivered, Ok(1));
        assert_eq!(
            queue.enqueued.borrow().as_slice(),
            [(
                "target".to_string(),
                "run the tests".to_string(),
                generation,
                1
            )]
        );
        assert_eq!(mode.owned_ids(), [1]);
        assert_eq!(*mode.phase(), Phase::Delivered);
    }

    /// The cancellation half of criterion 6: a disarm drops what is still
    /// parked, leaves everything it does not own alone, and says plainly that
    /// the entry already typed is gone for good.
    #[test]
    fn a_disarm_cancels_only_its_own_parked_entries_and_reports_the_delivered_one() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeQueue::default();
        // An unrelated Compose entry already sits in the FIFO.
        queue.parked.borrow_mut().push(99);

        mode.accept_transcript(generation, "first", 0);
        let _ = deliver_due(&mut mode, &queue, 1_500).expect("first send");
        mode.accept_transcript(generation, "second", 2_000);
        let _ = deliver_due(&mut mode, &queue, 3_500).expect("second send");
        // The composer typed the first one before the user aborted.
        queue.mark_delivered(1);

        let disarmed = mode.disarm(DisarmReason::Manual).expect("armed");
        let cancellation = cancel_disarmed(&queue, &disarmed);

        assert_eq!(cancellation.cancelled, [2]);
        assert_eq!(
            cancellation.already_delivered,
            [1],
            "a typed message cannot be retracted, and saying so is the contract"
        );
        assert_eq!(
            queue.parked.borrow().as_slice(),
            [99],
            "an unrelated Compose entry must survive the disarm"
        );
    }

    #[test]
    fn a_delivered_entry_is_no_longer_owned() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeQueue::default();
        mode.accept_transcript(generation, "typed already", 0);
        deliver_due(&mut mode, &queue, 1_500);

        mode.note_delivered(1);

        assert!(mode.owned_ids().is_empty());
        let disarmed = mode.disarm(DisarmReason::Manual).expect("armed");
        assert!(
            disarmed.cancel_ids.is_empty(),
            "nothing may be cancelled on behalf of a message already typed"
        );
    }

    #[test]
    fn a_rejected_enqueue_is_reported_and_owns_nothing() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeQueue::default();
        *queue.fail.borrow_mut() = Some("Session is not running an agent".to_string());
        mode.accept_transcript(generation, "run the tests", 0);

        let outcome = deliver_due(&mut mode, &queue, 1_500).expect("hold-back expired");

        assert_eq!(outcome, Err("Session is not running an agent".to_string()));
        assert!(mode.owned_ids().is_empty());
        assert!(queue.enqueued.borrow().is_empty());
    }

    // --- The real Compose queue -------------------------------------------

    /// The fake above proves the mode's bookkeeping; these prove the queue it
    /// is bookkeeping *for*. Ownership is enforced in `pty`, so a test that
    /// only ever sees `FakeQueue` would pass with the guard deleted.
    #[test]
    fn the_real_queue_cancels_only_voice_entries_the_caller_owns() {
        let state = crate::state::tests_support::make_test_app_state();
        let session = "voice-target";
        let (mine, stranger, human) = {
            let mut queue = state
                .pending_injections
                .entry(session.to_string())
                .or_default();
            let mine = crate::state::PendingInjection::voice_command("mine", 7);
            let stranger = crate::state::PendingInjection::voice_command("another mode", 7);
            let human = crate::state::PendingInjection::user_command("typed by hand");
            let ids = (mine.id(), stranger.id(), human.id());
            queue.push_back(mine);
            queue.push_back(stranger);
            queue.push_back(human);
            ids
        };

        // The caller owns `mine`, and names `human` as well — an id it must not
        // be able to clear, and one that was never delivered either.
        let cancellation = crate::pty::cancel_voice_commands(&state, session, &[mine, human]);

        assert_eq!(cancellation.cancelled, [mine]);
        assert_eq!(
            cancellation.already_delivered,
            [human],
            "a non-voice entry is never cancelled, and the caller is told so"
        );
        let remaining: Vec<u64> = state
            .pending_injections
            .get(session)
            .expect("queue")
            .iter()
            .map(|entry| entry.id())
            .collect();
        assert_eq!(
            remaining,
            [stranger, human],
            "unrelated Compose and peer entries keep their FIFO order"
        );
    }

    /// An id that already left the queue is delivered, not cancelled.
    #[test]
    fn the_real_queue_reports_an_id_it_no_longer_holds_as_delivered() {
        let state = crate::state::tests_support::make_test_app_state();

        let cancellation = crate::pty::cancel_voice_commands(&state, "gone", &[42]);

        assert!(cancellation.cancelled.is_empty());
        assert_eq!(cancellation.already_delivered, [42]);
    }

    /// "Unsupported targets stay unavailable" is a refusal at the queue, not a
    /// fallback somewhere else: there is no other exit from this module.
    #[test]
    fn the_real_queue_refuses_a_target_that_cannot_take_a_compose_entry() {
        let state = crate::state::tests_support::make_test_app_state();

        assert_eq!(
            crate::pty::enqueue_voice_command(&state, "no-such-session", "hello", 1),
            Err("Session not found".to_string())
        );
        assert_eq!(
            crate::pty::enqueue_voice_command(&state, "no-such-session", "   ", 1),
            Err("Command text is empty".to_string())
        );
    }

    /// Late asynchronous work is the failure this whole generation scheme
    /// exists for: a whisper pass that finishes after the abort must not send.
    #[test]
    fn a_transcription_that_finishes_after_a_disarm_never_reaches_the_queue() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeQueue::default();
        mode.note_transcribing();
        mode.disarm(DisarmReason::Manual);

        assert_eq!(
            mode.accept_transcript(generation, "too late", 0),
            TranscriptOutcome::NotArmed
        );
        assert!(deliver_due(&mut mode, &queue, u64::MAX).is_none());
        assert!(queue.enqueued.borrow().is_empty());
    }

    // --- The runtime loop -------------------------------------------------

    /// A microphone and a recogniser, under the test's control.
    ///
    /// `drain` hands back one queued chunk per call, so a test states the audio
    /// timeline chunk by chunk exactly as the desktop adapter delivers it.
    struct FakeEndpoint {
        chunks: RefCell<std::collections::VecDeque<Vec<f32>>>,
        connected: std::cell::Cell<bool>,
        drain_error: RefCell<Option<String>>,
        transcript: String,
        calls: std::cell::Cell<usize>,
        /// Run inside `transcribe`, to model work that lands while a whisper
        /// pass is still running.
        during_transcribe: RefCell<Option<Box<dyn Fn() + Send>>>,
    }

    impl FakeEndpoint {
        fn new(transcript: &str) -> Self {
            Self {
                chunks: RefCell::new(std::collections::VecDeque::new()),
                connected: std::cell::Cell::new(true),
                drain_error: RefCell::new(None),
                transcript: transcript.to_string(),
                calls: std::cell::Cell::new(0),
                during_transcribe: RefCell::new(None),
            }
        }

        fn feed(&self, samples: Vec<f32>) {
            self.chunks.borrow_mut().push_back(samples);
        }
    }

    impl VoiceEndpoint for FakeEndpoint {
        fn drain(&mut self) -> Result<Vec<f32>, String> {
            if let Some(error) = self.drain_error.borrow().as_ref() {
                return Err(error.clone());
            }
            Ok(self.chunks.borrow_mut().pop_front().unwrap_or_default())
        }

        fn connected(&self) -> bool {
            self.connected.get()
        }

        fn transcribe(&self, _audio: &[f32]) -> Result<String, String> {
            self.calls.set(self.calls.get() + 1);
            if let Some(during) = self.during_transcribe.borrow().as_ref() {
                during();
            }
            Ok(self.transcript.clone())
        }
    }

    /// A target that answers whatever the test last said.
    struct FakeTarget(std::cell::Cell<bool>);

    impl TargetProbe for FakeTarget {
        fn accepts(&self, _session_id: &str) -> bool {
            self.0.get()
        }
    }

    fn runtime_capture() -> Capture {
        Capture::new(test_config(), 5_000, 0)
    }

    fn armed_shared() -> parking_lot::Mutex<HandsFree> {
        let mut mode = HandsFree::new(1_000);
        mode.arm("target", "desktop", true).expect("arm");
        parking_lot::Mutex::new(mode)
    }

    /// The whole point of the pass: audio in one end, a Compose-queue entry out
    /// the other, with nobody touching a PTY in between.
    #[test]
    fn a_spoken_phrase_travels_from_capture_to_the_queue() {
        let mode = armed_shared();
        let generation = mode.lock().generation();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("run the tests");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeQueue::default();

        let mut input = speech(500);
        input.extend(silence(600));
        endpoint.feed(input);

        let first = tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);
        assert!(
            matches!(first, Tick::Running { enqueued: None, .. }),
            "the hold-back must still be running, got {first:?}"
        );
        assert_eq!(
            mode.lock().pending_text(),
            Some("run the tests"),
            "the transcript must be visible while it is held back"
        );
        assert!(queue.enqueued.borrow().is_empty());

        let sent = tick(&mut capture, &mode, &mut endpoint, &target, &queue, 1_200);

        assert!(
            matches!(
                sent,
                Tick::Running {
                    enqueued: Some(1),
                    ..
                }
            ),
            "the expired hold-back must enqueue, got {sent:?}"
        );
        assert_eq!(
            queue.enqueued.borrow().as_slice(),
            [(
                "target".to_string(),
                "run the tests".to_string(),
                generation,
                1
            )]
        );
        assert_eq!(mode.lock().owned_ids(), [1]);
    }

    /// An armed microphone in an empty room must cost nothing: no inference at
    /// all, and a retained-audio figure that does not move with time.
    #[test]
    fn an_armed_and_silent_microphone_never_infers_and_stays_bounded() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("should never be asked");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeQueue::default();

        // 60s of room silence, delivered the way cpal delivers it.
        for step in 0..1_200u64 {
            endpoint.feed(silence(50));
            tick(
                &mut capture,
                &mode,
                &mut endpoint,
                &target,
                &queue,
                step * 50,
            );
        }

        assert_eq!(
            endpoint.calls.get(),
            0,
            "silence must never reach the recogniser"
        );
        assert!(queue.enqueued.borrow().is_empty());
        assert!(
            capture.retained_samples() <= ms_to_samples(test_config().pre_roll_ms) + FRAME_SAMPLES,
            "silence retained {} samples — the pre-roll is the only buffer",
            capture.retained_samples()
        );
    }

    /// Whisper hands back an empty string for a cough. Nothing may be sent, and
    /// the mode must go back to listening rather than sit in `transcribing`.
    #[test]
    fn a_transcript_the_recogniser_rejects_sends_nothing() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("   ");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeQueue::default();

        let mut input = speech(500);
        input.extend(silence(600));
        endpoint.feed(input);

        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 0);
        // Well past any hold-back, with the stream still alive.
        endpoint.feed(silence(50));
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 3_000);

        assert_eq!(endpoint.calls.get(), 1, "the utterance was transcribed");
        assert!(
            queue.enqueued.borrow().is_empty(),
            "an empty transcript is not a message"
        );
        assert_eq!(*mode.lock().phase(), Phase::Waiting);
    }

    /// The reason the mode lock is released across a whisper pass: an abort
    /// during transcription has to be able to land, and the result that arrives
    /// afterwards has to be refused.
    #[test]
    fn an_abort_during_transcription_lands_and_its_result_is_refused() {
        let mode = std::sync::Arc::new(armed_shared());
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("too late");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeQueue::default();

        let aborting = mode.clone();
        *endpoint.during_transcribe.borrow_mut() = Some(Box::new(move || {
            let mut mode = aborting
                .try_lock()
                .expect("the runtime must not hold the mode lock across a transcription");
            mode.disarm(DisarmReason::Manual);
        }));

        let mut input = speech(500);
        input.extend(silence(600));
        endpoint.feed(input);

        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 0);
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 10_000);

        assert_eq!(endpoint.calls.get(), 1);
        assert!(
            queue.enqueued.borrow().is_empty(),
            "a result that outlived its mode must not reach a model"
        );
        assert!(mode.lock().binding().is_none());
    }

    /// The bound tab is closed while the mode is armed.
    #[test]
    fn a_closed_target_disarms_the_running_mode() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("unused");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeQueue::default();

        target.0.set(false);
        let outcome = tick(&mut capture, &mode, &mut endpoint, &target, &queue, 0);

        match outcome {
            Tick::Disarmed(disarmed) => assert_eq!(disarmed.reason, DisarmReason::TargetClosed),
            other => panic!("a closed target must disarm, got {other:?}"),
        }
        assert!(mode.lock().binding().is_none());
    }

    /// The endpoint that armed the mode goes away — the desktop audio endpoint
    /// released on shutdown, or (in 818) a remote client's socket closing.
    #[test]
    fn a_disconnected_owner_disarms_the_running_mode() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("unused");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeQueue::default();

        endpoint.connected.set(false);
        let outcome = tick(&mut capture, &mode, &mut endpoint, &target, &queue, 0);

        match outcome {
            Tick::Disarmed(disarmed) => {
                assert_eq!(disarmed.reason, DisarmReason::OwnerDisconnected)
            }
            other => panic!("a disconnected owner must disarm, got {other:?}"),
        }
    }

    /// A hard capture error is a device failure, and the message reaches the
    /// status rather than a log nobody reads.
    #[test]
    fn a_capture_error_disarms_with_its_message() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("unused");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeQueue::default();

        *endpoint.drain_error.borrow_mut() = Some("input device disappeared".to_string());
        let outcome = tick(&mut capture, &mode, &mut endpoint, &target, &queue, 0);

        match outcome {
            Tick::Disarmed(disarmed) => assert_eq!(
                disarmed.reason,
                DisarmReason::DeviceFailed("input device disappeared".to_string())
            ),
            other => panic!("a capture error must disarm, got {other:?}"),
        }
        assert_eq!(mode.lock().last_error(), Some("input device disappeared"));
    }

    /// A microphone that was unplugged does not report an error — it simply
    /// stops delivering samples. Silence in the *audio* is normal; silence in
    /// the *stream* is a dead device, and the mode must not stay armed on it.
    #[test]
    fn a_device_that_stops_delivering_samples_disarms_after_its_timeout() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("unused");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeQueue::default();

        // Still inside the timeout: no samples yet, but not a failure either.
        assert!(matches!(
            tick(&mut capture, &mode, &mut endpoint, &target, &queue, 4_000),
            Tick::Running { .. }
        ));

        let outcome = tick(&mut capture, &mode, &mut endpoint, &target, &queue, 5_001);

        match outcome {
            Tick::Disarmed(disarmed) => assert!(
                matches!(disarmed.reason, DisarmReason::DeviceFailed(_)),
                "got {:?}",
                disarmed.reason
            ),
            other => panic!("a silent stream must disarm, got {other:?}"),
        }
    }

    /// Audio arriving resets the starvation clock: a long dictation session
    /// must not disarm itself just because it passed the timeout.
    #[test]
    fn audio_keeps_the_device_alive_past_the_timeout() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("unused");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeQueue::default();

        for step in 0..300u64 {
            endpoint.feed(silence(50));
            let outcome = tick(
                &mut capture,
                &mode,
                &mut endpoint,
                &target,
                &queue,
                step * 50,
            );
            assert!(
                matches!(outcome, Tick::Running { .. }),
                "a live stream must stay armed at {}ms, got {outcome:?}",
                step * 50
            );
        }
    }

    /// A queue that refuses everything, and is `Send` so the tick under test
    /// can run on a thread the test can put a deadline on.
    struct RefusingQueue;

    impl VoiceQueue for RefusingQueue {
        fn enqueue(&self, _session_id: &str, _text: &str, _generation: u64) -> Result<u64, String> {
            Err("Session not found".to_string())
        }

        fn cancel(&self, _session_id: &str, _ids: &[u64]) -> VoiceCancellation {
            VoiceCancellation::default()
        }
    }

    /// A refused delivery is an ordinary outcome — the target can disappear
    /// between the transcript and the send — so the runtime must report it and
    /// keep going.
    ///
    /// It runs on its own thread with a deadline because the failure this
    /// guards against is a *deadlock*, not a wrong answer: holding the mode
    /// lock across the error arm parks the runtime and every status poll behind
    /// it forever. The 5s is a harness bound on a clock-free call, not a
    /// behaviour assertion — nothing inside `tick` waits for anything.
    #[test]
    fn a_refused_delivery_is_reported_without_parking_the_runtime() {
        let mode = std::sync::Arc::new(armed_shared());
        let generation = mode.lock().generation();
        mode.lock()
            .accept_transcript(generation, "run the tests", 0);

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let mode = mode.clone();
            scope.spawn(move || {
                let mut capture = runtime_capture();
                let mut endpoint = FakeEndpoint::new("unused");
                endpoint.feed(silence(50));
                let outcome = tick(
                    &mut capture,
                    &mode,
                    &mut endpoint,
                    &FakeTarget(std::cell::Cell::new(true)),
                    &RefusingQueue,
                    2_000,
                );
                let _ = tx.send(matches!(
                    outcome,
                    Tick::Running {
                        send_error: Some(_),
                        ..
                    }
                ));
            });

            let reported = rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("tick parked on the mode lock instead of reporting the refusal");
            assert!(reported, "the refusal must reach the caller");
        });

        let mode = mode
            .try_lock()
            .expect("the mode lock must have been released");
        assert_eq!(mode.last_error(), Some("Session not found"));
        assert!(
            mode.binding().is_some(),
            "a refused send is not a reason to disarm the mode"
        );
    }

    /// The hold-back is only a safety net if the abort inside it works.
    #[test]
    fn an_abort_inside_the_hold_back_sends_nothing() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("delete everything");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeQueue::default();

        let mut input = speech(500);
        input.extend(silence(600));
        endpoint.feed(input);
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 0);

        let disarmed = mode.lock().disarm(DisarmReason::Manual).expect("armed");
        assert!(
            disarmed.discarded_pending,
            "the held transcript was dropped"
        );

        assert!(matches!(
            tick(&mut capture, &mode, &mut endpoint, &target, &queue, 10_000),
            Tick::NotArmed
        ));
        assert!(queue.enqueued.borrow().is_empty());
    }
}
