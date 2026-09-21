//! The reply side of a voice conversation: a bounded queue that renders text
//! and gets it out of the speaker, and that stops on a word.
//!
//! [`continuous`](super::continuous) is the listening half and owns the
//! generation counter that says which turn is current. This is the speaking
//! half and carries the same number, for one reason: **a reply must be able to
//! arrive too late.** Synthesis takes real time, so a reply can still be
//! rendering when the user starts talking again. Playing it then is not a
//! small glitch — it is the application talking over the person it is
//! listening to, and every stage here is arranged so that cannot happen.
//!
//! ```text
//!   say(generation, text)
//!        │
//!        ▼
//!   [queue]  bounded; a reply whose generation is over is dropped, not rendered
//!        │
//!        ▼
//!   synthesis   cancellable; the in-flight request holds a SpeechCancel
//!        │
//!        ▼
//!   generation checked again   <-- audio rendered for a turn that ended is
//!        │                         discarded here rather than played
//!        ▼
//!   output.play()
//! ```
//!
//! Three separate checks rather than one, because the reply can become stale
//! at three different moments and only the last of them has audio to throw
//! away.
//!
//! ## What this module does not do
//!
//! It does not decide *when* to interrupt. Hearing the user over the speaker
//! is acoustic echo cancellation, which is story 816-cbbf's other half and is
//! not in this file: whoever detects near-end speech calls [`Speaker::hush`]
//! and this stops. That split is deliberate — the queue's correctness can be
//! proven without a microphone, and a microphone test cannot prove the queue.

// Same reason as `speech.rs`: the queue is finished and tested, and its
// consumers are the MCP capability (817-f67c) and the Dictation UI (818-2a29).
// Drop this with the first of them that lands.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::num::NonZero;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::{Condvar, Mutex};

use super::speech::{Speech, SpeechAudio, SpeechCancel, SpeechError};

/// How many replies may wait to be spoken.
///
/// A voice conversation that is four replies behind has stopped being a
/// conversation, and the audio for all four still has to be rendered and then
/// played end to end. Refusing the fifth tells the caller something is wrong
/// while the queue is still short enough to drain.
pub const MAX_QUEUED: usize = 4;

/// Why a reply was not accepted.
///
/// None of these is a synthesis failure: the reply never reached an engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeakError {
    /// [`MAX_QUEUED`] replies are already waiting.
    Full,
    /// The reply belongs to a turn that is already over.
    Stale { generation: u64, current: u64 },
    /// The speaker is shutting down.
    Stopped,
}

impl std::fmt::Display for SpeakError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => write!(f, "too many replies are already waiting to be spoken"),
            Self::Stale {
                generation,
                current,
            } => write!(
                f,
                "reply belongs to turn {generation}, and turn {current} is current"
            ),
            Self::Stopped => write!(f, "the speaker is shutting down"),
        }
    }
}

impl std::error::Error for SpeakError {}

/// Identifies one reply for its whole life.
///
/// A caller that queues a reply gets one of these back and can ask what became
/// of it afterwards. That indirection is the whole point: [`Speaker::say`]
/// returns the moment the reply is *accepted*, which is several seconds and
/// three failure modes before anybody hears it, and a caller told only
/// "accepted" would have no way to tell a spoken reply from a discarded one.
///
/// Meaningful only inside the [`Speaker`] that issued it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UtteranceId(u64);

impl std::fmt::Display for UtteranceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for UtteranceId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

/// What became of one reply.
///
/// The three terminal states are kept apart because a caller reacts
/// differently to each: `Finished` is the conversation working, `Interrupted`
/// is the user talking and is not an error, and `Failed` is the only one worth
/// surfacing. Collapsing the first two into "done" is the mistake that makes a
/// voice assistant claim it said something it never said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Utterance {
    /// Accepted, waiting for the renderer.
    Queued,
    /// The engine is rendering it.
    Rendering,
    /// Handed to the audio device, still coming out.
    Speaking,
    /// Played to the end. Set only after the device reported it had nothing
    /// left — never when the audio was merely handed over.
    Finished,
    /// The turn ended before it was heard. Not an error.
    Interrupted,
    /// Synthesis or the audio device refused it.
    Failed(String),
}

impl Utterance {
    /// Is there anything left to wait for?
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Finished | Self::Interrupted | Self::Failed(_))
    }
}

impl std::fmt::Display for Utterance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Queued => write!(f, "queued"),
            Self::Rendering => write!(f, "rendering"),
            Self::Speaking => write!(f, "speaking"),
            Self::Finished => write!(f, "finished"),
            Self::Interrupted => write!(f, "interrupted"),
            Self::Failed(reason) => write!(f, "failed: {reason}"),
        }
    }
}

/// How many past replies a [`Speaker`] remembers the fate of.
///
/// A caller polls for the reply it just queued, so it only ever needs the
/// newest few. The bound is here because a long conversation would otherwise
/// grow a map nothing prunes.
const MAX_TRACKED: usize = 64;

/// How often the render thread looks up from an empty queue to see whether the
/// device has gone quiet.
///
/// It only waits this way while something is actually playing; an idle speaker
/// blocks on the condvar and costs nothing.
const PLAYBACK_POLL: Duration = Duration::from_millis(25);

/// Where rendered audio goes.
///
/// A trait because the two things that have to be tested here — that
/// interruption reaches the device, and that a device failure is reported
/// rather than swallowed — cannot be tested against a real speaker on a build
/// machine, and because a reply that is never played is indistinguishable from
/// one that is unless something records the difference.
pub trait Output: Send + Sync {
    /// Hand audio to the device. Returns once it is queued, not once it is
    /// heard: the caller must stay free to interrupt.
    fn play(&self, audio: &SpeechAudio) -> Result<(), String>;

    /// Stop now and drop whatever was queued. Must not block: it is called
    /// from whatever thread noticed the user talking.
    fn stop(&self);

    /// Is audio still coming out?
    fn is_speaking(&self) -> bool;
}

/// What the UI and the status endpoints need to know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeakerStatus {
    /// The turn replies must belong to in order to be spoken.
    pub generation: u64,
    /// Replies waiting, not counting one being rendered.
    pub queued: usize,
    /// Is a reply being rendered right now?
    pub rendering: bool,
    /// Is audio coming out of the speaker right now?
    pub speaking: bool,
    /// The last synthesis or device failure, if any. Cleared by the next
    /// reply that succeeds.
    pub last_error: Option<String>,
}

struct Reply {
    id: UtteranceId,
    generation: u64,
    text: String,
    voice: String,
}

struct InFlight {
    id: UtteranceId,
    generation: u64,
    cancel: SpeechCancel,
}

struct State {
    generation: u64,
    queue: VecDeque<Reply>,
    in_flight: Option<InFlight>,
    last_error: Option<String>,
    shutdown: bool,
    /// Next identity to hand out. Never reused, so a stale caller polling an
    /// old id gets that id's own fate rather than a newer reply's.
    next_id: u64,
    /// What became of each reply, oldest first, capped at [`MAX_TRACKED`].
    tracked: VecDeque<(UtteranceId, Utterance)>,
    /// Handed to the device and not yet known to have finished, in the order
    /// the device will play them. The device reports one boolean for the whole
    /// queue, so these resolve together when it goes quiet — which is correct,
    /// since it drains in order.
    playing: VecDeque<UtteranceId>,
}

impl State {
    /// Hand out an identity and start tracking it.
    fn track(&mut self, state: Utterance) -> UtteranceId {
        let id = UtteranceId(self.next_id);
        self.next_id += 1;
        self.tracked.push_back((id, state));
        while self.tracked.len() > MAX_TRACKED {
            self.tracked.pop_front();
        }
        id
    }

    /// Record a transition. Silently ignores an id that has aged out, which is
    /// the only way it can be missing.
    fn set(&mut self, id: UtteranceId, state: Utterance) {
        if let Some(entry) = self.tracked.iter_mut().find(|(tracked, _)| *tracked == id) {
            entry.1 = state;
        }
    }

    fn get(&self, id: UtteranceId) -> Option<Utterance> {
        self.tracked
            .iter()
            .find(|(tracked, _)| *tracked == id)
            .map(|(_, state)| state.clone())
    }
}

struct Shared {
    state: Mutex<State>,
    /// Woken by a new reply, by an interruption and by shutdown.
    wake: Condvar,
}

/// The queue. Dropping it stops everything.
pub struct Speaker {
    shared: Arc<Shared>,
    output: Arc<dyn Output>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Speaker {
    /// Start the rendering thread.
    ///
    /// `generation` is the turn replies must carry to be spoken; it comes from
    /// the hands-free state machine so that both halves agree on which turn is
    /// current from the first reply onwards.
    pub fn new(speech: Arc<dyn Speech>, output: Arc<dyn Output>, generation: u64) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                generation,
                queue: VecDeque::new(),
                in_flight: None,
                last_error: None,
                shutdown: false,
                next_id: 1,
                tracked: VecDeque::new(),
                playing: VecDeque::new(),
            }),
            wake: Condvar::new(),
        });
        let worker = std::thread::Builder::new()
            .name("speech-queue".to_string())
            .spawn({
                let shared = Arc::clone(&shared);
                let output = Arc::clone(&output);
                move || render_loop(&shared, speech.as_ref(), output.as_ref())
            })
            .expect("a thread for the speech queue");
        Self {
            shared,
            output,
            worker: Some(worker),
        }
    }

    /// Queue a reply.
    ///
    /// Returns as soon as it is queued. A reply for a turn that is already
    /// over is refused here rather than rendered and thrown away later.
    ///
    /// The returned [`UtteranceId`] is how the caller finds out what happened
    /// next: accepting a reply says nothing about whether anybody heard it.
    pub fn say(&self, generation: u64, text: &str, voice: &str) -> Result<UtteranceId, SpeakError> {
        let mut state = self.shared.state.lock();
        if state.shutdown {
            return Err(SpeakError::Stopped);
        }
        if generation != state.generation {
            return Err(SpeakError::Stale {
                generation,
                current: state.generation,
            });
        }
        if state.queue.len() >= MAX_QUEUED {
            return Err(SpeakError::Full);
        }
        let id = state.track(Utterance::Queued);
        state.queue.push_back(Reply {
            id,
            generation,
            text: text.to_string(),
            voice: voice.to_string(),
        });
        self.shared.wake.notify_all();
        Ok(id)
    }

    /// What became of a reply, or `None` if this speaker never issued that id
    /// or has forgotten it — see [`MAX_TRACKED`].
    pub fn utterance(&self, id: UtteranceId) -> Option<Utterance> {
        self.shared.state.lock().get(id)
    }

    /// Stop talking, now, and forget everything queued for this turn.
    ///
    /// This is what the user talking over the reply calls. It returns the new
    /// generation: nothing rendered for the old one will be played, including
    /// audio that is already finished and on its way back from the engine.
    ///
    /// Idempotent in effect but not in number — every call opens a new turn,
    /// which is correct: two interruptions in a row are two turns.
    pub fn hush(&self) -> u64 {
        let generation = {
            // Read before stopping the device, and while nothing else can
            // queue: a reply that has already been heard in full must not be
            // reported as interrupted just because an interruption followed
            // it. A cheap atomic read, not an interlock.
            let heard_everything = !self.output.is_speaking();
            let mut state = self.shared.state.lock();
            state.generation += 1;
            for reply in state.queue.drain(..).collect::<Vec<_>>() {
                state.set(reply.id, Utterance::Interrupted);
            }
            if let Some(in_flight) = state.in_flight.take() {
                in_flight.cancel.cancel();
                state.set(in_flight.id, Utterance::Interrupted);
            }
            let outcome = if heard_everything {
                Utterance::Finished
            } else {
                Utterance::Interrupted
            };
            while let Some(id) = state.playing.pop_front() {
                state.set(id, outcome.clone());
            }
            state.generation
        };
        // Outside the lock: the render thread takes it when synthesis returns,
        // and an interruption must not wait for that.
        self.output.stop();
        self.shared.wake.notify_all();
        generation
    }

    pub fn status(&self) -> SpeakerStatus {
        let state = self.shared.state.lock();
        SpeakerStatus {
            generation: state.generation,
            queued: state.queue.len(),
            rendering: state.in_flight.is_some(),
            speaking: self.output.is_speaking(),
            last_error: state.last_error.clone(),
        }
    }

    /// The turn replies must belong to right now.
    pub fn generation(&self) -> u64 {
        self.shared.state.lock().generation
    }
}

impl Drop for Speaker {
    fn drop(&mut self) {
        {
            let mut state = self.shared.state.lock();
            state.shutdown = true;
            state.queue.clear();
            if let Some(in_flight) = state.in_flight.take() {
                // Without this the join below waits for a whole utterance to
                // render. Both adapters check the flag as they go.
                in_flight.cancel.cancel();
            }
        }
        self.output.stop();
        self.shared.wake.notify_all();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Take replies, render them, play what is still wanted.
fn render_loop(shared: &Shared, speech: &dyn Speech, output: &dyn Output) {
    loop {
        let Some((reply, cancel)) = next_reply(shared, output) else {
            return;
        };

        let rendered = speech.synthesize(&reply.text, &reply.voice, &cancel);

        let mut state = shared.state.lock();
        state.in_flight = None;
        if state.shutdown {
            return;
        }
        if state.generation != reply.generation {
            state.set(reply.id, Utterance::Interrupted);
            // The turn ended while this was rendering. This is the case the
            // whole module exists for, and the audio is finished and correct —
            // which is exactly why it has to be thrown away here.
            tracing::debug!(
                "speech: discarding a reply rendered for turn {} while turn {} is current",
                reply.generation,
                state.generation
            );
            continue;
        }

        match rendered {
            // Played with the lock held, on purpose. Released first, an
            // interruption between the generation check above and this line
            // would bump the generation, stop a silent device, and then have
            // the stale audio appended behind it. `play` only queues, so
            // `hush` waits on the order of a mixer append, not an utterance.
            Ok(audio) => match output.play(&audio) {
                Ok(()) => {
                    state.last_error = None;
                    // Handed over, not heard. `Finished` is set by the poll in
                    // `next_reply` once the device reports it has nothing left.
                    state.set(reply.id, Utterance::Speaking);
                    state.playing.push_back(reply.id);
                }
                Err(reason) => {
                    tracing::warn!("speech: the audio device refused a reply: {reason}");
                    state.set(reply.id, Utterance::Failed(reason.clone()));
                    state.last_error = Some(reason);
                }
            },
            Err(SpeechError::Cancelled) => {
                // Not a failure. Somebody asked for this.
                state.set(reply.id, Utterance::Interrupted);
            }
            Err(error) => {
                tracing::warn!("speech: {error}");
                state.set(reply.id, Utterance::Failed(error.to_string()));
                state.last_error = Some(error.to_string());
            }
        }
    }
}

/// Resolve replies the device has finished playing.
///
/// Called only from the render thread, which is why it may take the device's
/// word for it: `is_speaking` is a status field, and by the time it reads
/// false everything handed over has come out in order.
fn note_playback_drained(state: &mut State, output: &dyn Output) {
    if state.playing.is_empty() || output.is_speaking() {
        return;
    }
    while let Some(id) = state.playing.pop_front() {
        state.set(id, Utterance::Finished);
    }
}

/// Block until there is a reply worth rendering, or until shutdown.
///
/// Returns `None` only on shutdown. While waiting with audio still coming out
/// it wakes periodically to notice the device going quiet — that is what turns
/// a `Speaking` reply into a `Finished` one, and it is done here rather than on
/// its own thread because this loop already owns the state.
fn next_reply(shared: &Shared, output: &dyn Output) -> Option<(Reply, SpeechCancel)> {
    let mut state = shared.state.lock();
    loop {
        if state.shutdown {
            return None;
        }
        note_playback_drained(&mut state, output);
        match state.queue.pop_front() {
            Some(reply) if reply.generation == state.generation => {
                let cancel = SpeechCancel::new();
                state.in_flight = Some(InFlight {
                    id: reply.id,
                    generation: reply.generation,
                    cancel: cancel.clone(),
                });
                state.set(reply.id, Utterance::Rendering);
                return Some((reply, cancel));
            }
            // Queued before the turn changed. Dropping it here costs nothing;
            // rendering it first would cost a second of CPU and a second of
            // latency for the reply that does matter.
            Some(reply) => {
                state.set(reply.id, Utterance::Interrupted);
                continue;
            }
            // A timed wait only while something is still playing: an idle
            // speaker must not poll a device that has nothing to report.
            None if !state.playing.is_empty() => {
                shared.wake.wait_for(&mut state, PLAYBACK_POLL);
            }
            None => shared.wake.wait(&mut state),
        }
    }
}

// ---------------------------------------------------------------------------
// The real device
// ---------------------------------------------------------------------------

/// Playback through the system's audio output.
pub struct DeviceOutput {
    /// Held for as long as the player: dropping the stream silences it.
    _stream: rodio::MixerDeviceSink,
    player: rodio::Player,
}

impl DeviceOutput {
    /// Open the named output device, or the system default.
    ///
    /// Fails rather than falling back to silence: a voice conversation whose
    /// speaker never opened is not a degraded conversation, it is a monologue
    /// the user cannot hear.
    pub fn open(device_name: Option<&str>) -> Result<Self, String> {
        // The same resolution notification sounds use: named device first,
        // system default second. One spelling of "which speaker" per app.
        let stream = crate::notification_sound::resolve_output_stream(device_name)
            .ok_or_else(|| "no audio output device could be opened".to_string())?;
        let player = rodio::Player::connect_new(stream.mixer());
        Ok(Self {
            _stream: stream,
            player,
        })
    }
}

impl Output for DeviceOutput {
    fn play(&self, audio: &SpeechAudio) -> Result<(), String> {
        let (channels, rate) = source_parameters(audio)?;
        // The engine's own rate, not a rate assumed here: rodio resamples to
        // whatever the device wants. Pocket TTS renders at 24 kHz, a
        // user-supplied command at whatever its engine likes.
        //
        // After a `stop` this waits for the mixer to drop the stopped source
        // before it resumes the player — one `periodic_access` tick, ~5 ms.
        self.player.append(rodio::buffer::SamplesBuffer::new(
            channels,
            rate,
            audio.samples.clone(),
        ));
        Ok(())
    }

    fn stop(&self) {
        // `stop`, not `clear`: rodio's `clear` sleeps until the current source
        // ends, and this is called by the thread that just heard the user
        // start talking. `stop` only sets an atomic, and a later `append`
        // resumes the player by itself.
        self.player.stop();
    }

    fn is_speaking(&self) -> bool {
        // Reads rodio's queue length, which the mixer thread decrements. After
        // `stop` it stays true for about one mixer tick — a status field, not
        // an interlock, and nothing here waits on it.
        !self.player.empty()
    }
}

/// Channel count and sample rate for a buffer, or why it cannot be played.
///
/// Separate from [`DeviceOutput::play`] because `SamplesBuffer::new` **panics**
/// on a zero sample rate, and a zero rate is reachable: it is whatever an
/// engine reported. A panic in the render thread would take the queue down
/// silently.
fn source_parameters(audio: &SpeechAudio) -> Result<(NonZero<u16>, NonZero<u32>), String> {
    if audio.samples.is_empty() {
        return Err("the engine returned no samples".to_string());
    }
    let rate = NonZero::new(audio.sample_rate)
        .ok_or_else(|| "the engine reported a sample rate of zero".to_string())?;
    // The port carries mono; adapters mix down before they return.
    Ok((NonZero::new(1).expect("one channel is not zero"), rate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    /// Long enough for the render thread to get somewhere, short enough that a
    /// test that is wrong fails instead of hanging.
    const SETTLE: Duration = Duration::from_millis(50);
    const PATIENCE: Duration = Duration::from_secs(5);

    /// Wait for something the render thread does on its own time.
    fn eventually(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + PATIENCE;
        while Instant::now() < deadline {
            if done() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("timed out waiting for {what}");
    }

    #[derive(Default)]
    struct Recorded {
        played: Vec<String>,
        stops: usize,
        /// Set by `play`, cleared by `stop`. A real device clears it when the
        /// mixer drains; here it is exact, which is what a test needs.
        speaking: bool,
    }

    /// An output that remembers instead of making a noise.
    #[derive(Default)]
    struct FakeOutput {
        recorded: Mutex<Recorded>,
        fail_with: Option<String>,
        /// How many times the render thread asked whether audio was still
        /// coming out. A test reads it to prove the thread is not polling.
        speaking_queries: AtomicUsize,
    }

    impl FakeOutput {
        fn failing(reason: &str) -> Self {
            Self {
                fail_with: Some(reason.to_string()),
                ..Self::default()
            }
        }

        /// Replies played, named by their first sample so a test can tell them
        /// apart without carrying the text through the fake engine.
        fn played(&self) -> Vec<String> {
            self.recorded.lock().played.clone()
        }

        fn stops(&self) -> usize {
            self.recorded.lock().stops
        }

        /// The device draining by itself, which a real one does when the audio
        /// ends and a fake one cannot do on its own. Tests drive it explicitly
        /// rather than sleeping, so "played to the end" is a fact rather than
        /// a timing guess.
        fn finish_playing(&self) {
            self.recorded.lock().speaking = false;
        }

        fn speaking_queries(&self) -> usize {
            self.speaking_queries.load(Ordering::SeqCst)
        }
    }

    impl Output for FakeOutput {
        fn play(&self, audio: &SpeechAudio) -> Result<(), String> {
            if let Some(reason) = &self.fail_with {
                return Err(reason.clone());
            }
            let mut recorded = self.recorded.lock();
            recorded.played.push(format!("{:.0}", audio.samples[0]));
            recorded.speaking = true;
            Ok(())
        }

        fn stop(&self) {
            let mut recorded = self.recorded.lock();
            recorded.stops += 1;
            recorded.speaking = false;
        }

        fn is_speaking(&self) -> bool {
            self.speaking_queries.fetch_add(1, Ordering::SeqCst);
            self.recorded.lock().speaking
        }
    }

    /// An engine that renders one sample carrying the text's length, so a test
    /// can recognise which reply came out without a real voice.
    struct FakeSpeech {
        /// Held for as long as a test wants synthesis to be in progress.
        hold: Option<Arc<Mutex<()>>>,
        /// How many replies render instantly before `hold` starts applying.
        /// Lets one reply reach the speaker before the next one gets stuck.
        render_freely: usize,
        started: Arc<AtomicUsize>,
        cancelled: Arc<AtomicUsize>,
    }

    impl FakeSpeech {
        fn instant() -> Self {
            Self {
                hold: None,
                render_freely: 0,
                started: Arc::new(AtomicUsize::new(0)),
                cancelled: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn blocking(hold: Arc<Mutex<()>>) -> Self {
            Self {
                hold: Some(hold),
                ..Self::instant()
            }
        }

        fn blocking_after(hold: Arc<Mutex<()>>, render_freely: usize) -> Self {
            Self {
                render_freely,
                ..Self::blocking(hold)
            }
        }
    }

    impl Speech for FakeSpeech {
        fn synthesize(
            &self,
            text: &str,
            _voice: &str,
            cancel: &SpeechCancel,
        ) -> Result<SpeechAudio, SpeechError> {
            let nth = self.started.fetch_add(1, Ordering::SeqCst);
            if let Some(hold) = self.hold.as_ref().filter(|_| nth >= self.render_freely) {
                // Wait for the test to release the lock, checking the flag the
                // way a real adapter does between frames.
                loop {
                    if cancel.is_cancelled() {
                        self.cancelled.fetch_add(1, Ordering::SeqCst);
                        return Err(SpeechError::Cancelled);
                    }
                    if hold.try_lock().is_some() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
            if cancel.is_cancelled() {
                self.cancelled.fetch_add(1, Ordering::SeqCst);
                return Err(SpeechError::Cancelled);
            }
            Ok(SpeechAudio {
                samples: vec![text.len() as f32; 4],
                sample_rate: 24_000,
            })
        }
    }

    /// An engine that finishes what it started, cancel flag or not.
    ///
    /// Not a strawman: an adapter checks the flag between frames, and the last
    /// frame has no "between" after it, so a request cancelled at the wrong
    /// microsecond still returns audio. The queue has to discard it.
    struct DeafSpeech {
        hold: Arc<Mutex<()>>,
    }

    impl Speech for DeafSpeech {
        fn synthesize(
            &self,
            text: &str,
            _voice: &str,
            _cancel: &SpeechCancel,
        ) -> Result<SpeechAudio, SpeechError> {
            let _held = self.hold.lock();
            Ok(SpeechAudio {
                samples: vec![text.len() as f32; 4],
                sample_rate: 24_000,
            })
        }
    }

    struct BrokenSpeech;

    impl Speech for BrokenSpeech {
        fn synthesize(
            &self,
            _text: &str,
            _voice: &str,
            _cancel: &SpeechCancel,
        ) -> Result<SpeechAudio, SpeechError> {
            Err(SpeechError::ModelUnavailable {
                what: "italian/bundle.json".to_string(),
                reason: "not downloaded".to_string(),
            })
        }
    }

    #[test]
    fn a_reply_is_rendered_and_played() {
        let output = Arc::new(FakeOutput::default());
        let speaker = Speaker::new(Arc::new(FakeSpeech::instant()), Arc::clone(&output) as _, 7);

        speaker.say(7, "ciao", "alba").unwrap();

        eventually("the reply to be played", || output.played().len() == 1);
        assert_eq!(output.played(), vec!["4"], "the four-character reply");
    }

    #[test]
    fn replies_are_spoken_in_the_order_they_were_asked_for() {
        // A conversation whose second sentence arrives first is not a
        // conversation. One render thread, one queue, in order.
        let output = Arc::new(FakeOutput::default());
        let speaker = Speaker::new(Arc::new(FakeSpeech::instant()), Arc::clone(&output) as _, 0);

        speaker.say(0, "a", "v").unwrap();
        speaker.say(0, "bb", "v").unwrap();
        speaker.say(0, "ccc", "v").unwrap();

        eventually("three replies", || output.played().len() == 3);
        assert_eq!(output.played(), vec!["1", "2", "3"]);
    }

    #[test]
    fn a_reply_for_a_turn_that_is_over_is_refused_rather_than_rendered() {
        let output = Arc::new(FakeOutput::default());
        let speech = Arc::new(FakeSpeech::instant());
        let started = Arc::clone(&speech.started);
        let speaker = Speaker::new(speech, Arc::clone(&output) as _, 3);

        let current = speaker.hush();

        let refused = speaker.say(3, "late", "v").unwrap_err();
        assert_eq!(
            refused,
            SpeakError::Stale {
                generation: 3,
                current
            }
        );
        std::thread::sleep(SETTLE);
        assert_eq!(started.load(Ordering::SeqCst), 0, "nothing was rendered");
        assert!(output.played().is_empty());
    }

    #[test]
    fn queued_replies_are_dropped_when_the_turn_ends() {
        // Interrupting while replies are still waiting: the user talked, and
        // the answers to what they said before are no longer answers.
        let hold = Arc::new(Mutex::new(()));
        let guard = hold.lock();
        let output = Arc::new(FakeOutput::default());
        let speech = Arc::new(FakeSpeech::blocking(Arc::clone(&hold)));
        let started = Arc::clone(&speech.started);
        let speaker = Speaker::new(speech, Arc::clone(&output) as _, 0);

        speaker.say(0, "first", "v").unwrap();
        eventually("the first reply to start rendering", || {
            started.load(Ordering::SeqCst) == 1
        });
        speaker.say(0, "second", "v").unwrap();
        speaker.say(0, "third", "v").unwrap();
        assert_eq!(speaker.status().queued, 2);

        speaker.hush();
        drop(guard);

        std::thread::sleep(SETTLE);
        assert!(output.played().is_empty(), "nothing from the old turn");
        assert_eq!(speaker.status().queued, 0);
        assert_eq!(
            started.load(Ordering::SeqCst),
            1,
            "the queued replies were never handed to the engine"
        );
    }

    #[test]
    fn a_reply_rendered_after_the_turn_ended_is_discarded_rather_than_played() {
        // The case this module exists for: synthesis finished, the audio is
        // correct, and playing it would be the application talking over the
        // user.
        let hold = Arc::new(Mutex::new(()));
        let guard = hold.lock();
        let output = Arc::new(FakeOutput::default());
        let speech = Arc::new(FakeSpeech::blocking(Arc::clone(&hold)));
        let started = Arc::clone(&speech.started);
        let speaker = Speaker::new(speech, Arc::clone(&output) as _, 0);

        speaker.say(0, "ciao", "v").unwrap();
        eventually("rendering to start", || started.load(Ordering::SeqCst) == 1);

        speaker.hush();
        // Only now does the engine get to finish. It returns audio for a turn
        // that is over.
        drop(guard);

        std::thread::sleep(SETTLE);
        assert!(
            output.played().is_empty(),
            "audio from the previous turn reached the speaker"
        );
    }

    #[test]
    fn interrupting_while_rendering_cancels_the_engine_rather_than_waiting_for_it() {
        let hold = Arc::new(Mutex::new(()));
        let _guard = hold.lock();
        let output = Arc::new(FakeOutput::default());
        let speech = Arc::new(FakeSpeech::blocking(Arc::clone(&hold)));
        let started = Arc::clone(&speech.started);
        let cancelled = Arc::clone(&speech.cancelled);
        let speaker = Speaker::new(speech, Arc::clone(&output) as _, 0);

        speaker.say(0, "ciao", "v").unwrap();
        eventually("rendering to start", || started.load(Ordering::SeqCst) == 1);

        speaker.hush();

        eventually("the engine to notice the cancellation", || {
            cancelled.load(Ordering::SeqCst) == 1
        });
        assert!(output.played().is_empty());
    }

    #[test]
    fn interrupting_reaches_the_device_without_waiting_for_the_render_thread() {
        // The user is mid-sentence. `hush` must stop the speaker on the
        // caller's thread, not after the engine gets round to returning.
        let hold = Arc::new(Mutex::new(()));
        let _guard = hold.lock();
        let output = Arc::new(FakeOutput::default());
        let speech = Arc::new(FakeSpeech::blocking(Arc::clone(&hold)));
        let started = Arc::clone(&speech.started);
        let speaker = Speaker::new(speech, Arc::clone(&output) as _, 0);

        speaker.say(0, "ciao", "v").unwrap();
        eventually("rendering to start", || started.load(Ordering::SeqCst) == 1);

        let at = Instant::now();
        speaker.hush();
        let took = at.elapsed();

        assert_eq!(output.stops(), 1, "the device was told to stop");
        assert!(took < Duration::from_millis(100), "hush took {took:?}");
    }

    #[test]
    fn interrupting_during_playback_stops_the_device_and_no_stale_reply_resumes() {
        // The third cancellation point: audio is out of the engine and coming
        // out of the speaker, with more replies behind it. Interrupting must
        // silence the device and leave nothing to resume.
        let hold = Arc::new(Mutex::new(()));
        let guard = hold.lock();
        let output = Arc::new(FakeOutput::default());
        // The first reply renders and plays; the second one gets stuck in the
        // engine, so the interruption arrives with the device still speaking.
        let speech = Arc::new(FakeSpeech::blocking_after(Arc::clone(&hold), 1));
        let started = Arc::clone(&speech.started);
        let speaker = Speaker::new(speech, Arc::clone(&output) as _, 0);

        speaker.say(0, "first", "v").unwrap();
        speaker.say(0, "second", "v").unwrap();
        speaker.say(0, "third", "v").unwrap();
        eventually("the first reply to reach the speaker", || {
            speaker.status().speaking
        });
        eventually("the second reply to start rendering", || {
            started.load(Ordering::SeqCst) == 2
        });

        speaker.hush();
        drop(guard);

        std::thread::sleep(SETTLE);
        let status = speaker.status();
        assert!(!status.speaking, "the device kept talking through the user");
        assert_eq!(output.stops(), 1);
        assert_eq!(output.played(), vec!["5"], "only the reply from before");
        assert_eq!(status.queued, 0);
        assert!(!status.rendering);
    }

    #[test]
    fn a_fifth_waiting_reply_is_refused_while_the_queue_can_still_drain() {
        let hold = Arc::new(Mutex::new(()));
        let _guard = hold.lock();
        let output = Arc::new(FakeOutput::default());
        let speech = Arc::new(FakeSpeech::blocking(Arc::clone(&hold)));
        let started = Arc::clone(&speech.started);
        let speaker = Speaker::new(speech, Arc::clone(&output) as _, 0);

        // The first is taken by the render thread and blocks there, so
        // MAX_QUEUED more fit behind it.
        speaker.say(0, "first", "v").unwrap();
        eventually("rendering to start", || started.load(Ordering::SeqCst) == 1);
        for n in 0..MAX_QUEUED {
            speaker.say(0, &format!("reply {n}"), "v").unwrap();
        }

        assert_eq!(
            speaker.say(0, "one too many", "v").unwrap_err(),
            SpeakError::Full
        );
        assert_eq!(speaker.status().queued, MAX_QUEUED);
    }

    #[test]
    fn a_device_that_refuses_the_audio_is_reported_and_the_queue_keeps_going() {
        let output = Arc::new(FakeOutput::failing("device disconnected"));
        let speaker = Speaker::new(Arc::new(FakeSpeech::instant()), Arc::clone(&output) as _, 0);

        speaker.say(0, "ciao", "v").unwrap();

        eventually("the failure to be recorded", || {
            speaker.status().last_error.is_some()
        });
        assert_eq!(
            speaker.status().last_error.as_deref(),
            Some("device disconnected")
        );
        // And the thread is still there to take the next one.
        speaker.say(0, "ancora", "v").unwrap();
    }

    #[test]
    fn an_engine_that_cannot_render_is_reported_rather_than_retried_forever() {
        let output = Arc::new(FakeOutput::default());
        let speaker = Speaker::new(Arc::new(BrokenSpeech), Arc::clone(&output) as _, 0);

        speaker.say(0, "ciao", "v").unwrap();

        eventually("the failure to surface", || {
            speaker.status().last_error.is_some()
        });
        let reported = speaker.status().last_error.unwrap();
        assert!(reported.contains("italian/bundle.json"), "{reported}");
        assert!(output.played().is_empty());
    }

    #[test]
    fn a_successful_reply_clears_an_earlier_failure() {
        // A stale error in the status bar is a bug report the user will make
        // about a problem that is over.
        let output = Arc::new(FakeOutput::default());
        let speaker = Speaker::new(Arc::new(FakeSpeech::instant()), Arc::clone(&output) as _, 0);
        {
            let mut state = speaker.shared.state.lock();
            state.last_error = Some("an old problem".to_string());
        }

        speaker.say(0, "ciao", "v").unwrap();

        eventually("the reply to play", || output.played().len() == 1);
        assert_eq!(speaker.status().last_error, None);
    }

    #[test]
    fn shutting_down_stops_the_speaker_and_does_not_wait_for_the_engine() {
        let hold = Arc::new(Mutex::new(()));
        let _guard = hold.lock();
        let output = Arc::new(FakeOutput::default());
        let speech = Arc::new(FakeSpeech::blocking(Arc::clone(&hold)));
        let started = Arc::clone(&speech.started);
        let speaker = Speaker::new(speech, Arc::clone(&output) as _, 0);
        speaker.say(0, "ciao", "v").unwrap();
        eventually("rendering to start", || started.load(Ordering::SeqCst) == 1);

        let at = Instant::now();
        drop(speaker);
        let took = at.elapsed();

        assert!(took < Duration::from_secs(1), "dropping took {took:?}");
        assert!(
            output.stops() >= 1,
            "the device was silenced on the way out"
        );
        assert!(output.played().is_empty());
    }

    #[test]
    fn a_reply_asked_for_after_shutdown_is_refused_rather_than_queued_forever() {
        let output = Arc::new(FakeOutput::default());
        let speaker = Speaker::new(Arc::new(FakeSpeech::instant()), Arc::clone(&output) as _, 0);
        {
            let mut state = speaker.shared.state.lock();
            state.shutdown = true;
        }

        assert_eq!(
            speaker.say(0, "ciao", "v").unwrap_err(),
            SpeakError::Stopped
        );
    }

    #[test]
    fn two_interruptions_are_two_turns() {
        let output = Arc::new(FakeOutput::default());
        let speaker = Speaker::new(Arc::new(FakeSpeech::instant()), Arc::clone(&output) as _, 0);

        assert_eq!(speaker.hush(), 1);
        assert_eq!(speaker.hush(), 2);
        assert_eq!(speaker.generation(), 2);
        // And a reply for the turn that is now current is accepted.
        speaker.say(2, "ciao", "v").unwrap();
    }

    #[test]
    fn the_engines_own_sample_rate_reaches_the_device() {
        // Pocket TTS renders at 24 kHz and a user-supplied command at whatever
        // its engine likes. Assuming a rate here would pitch-shift every
        // external engine.
        let audio = SpeechAudio {
            samples: vec![0.1; 8],
            sample_rate: 22_050,
        };

        let (channels, rate) = source_parameters(&audio).unwrap();

        assert_eq!(rate.get(), 22_050);
        assert_eq!(channels.get(), 1);
    }

    #[test]
    fn audio_that_would_panic_the_mixer_is_refused_first() {
        // `SamplesBuffer::new` panics on a zero sample rate, and the rate is
        // whatever an engine reported. A panic on the render thread would take
        // the queue down with no message.
        let zero_rate = SpeechAudio {
            samples: vec![0.1; 8],
            sample_rate: 0,
        };
        let empty = SpeechAudio {
            samples: Vec::new(),
            sample_rate: 24_000,
        };

        assert!(source_parameters(&zero_rate).unwrap_err().contains("zero"));
        assert!(
            source_parameters(&empty)
                .unwrap_err()
                .contains("no samples")
        );
    }

    // -----------------------------------------------------------------------
    // Utterance identity (817-f67c)
    //
    // The property under test throughout: a caller is told what actually
    // happened to its reply, and "we accepted it" is never allowed to stand in
    // for "the user heard it".
    // -----------------------------------------------------------------------

    #[test]
    fn accepting_a_reply_is_not_the_same_as_speaking_it() {
        // The defect this exists for: `say` returns in microseconds, and a
        // caller that treats that as completion reports a spoken reply while
        // the engine has not even started.
        let output = Arc::new(FakeOutput::default());
        let speaker = Speaker::new(Arc::new(FakeSpeech::instant()), Arc::clone(&output) as _, 3);

        let id = speaker.say(3, "ciao", "alba").unwrap();

        eventually("the reply to reach the device", || {
            speaker.utterance(id) == Some(Utterance::Speaking)
        });
        assert_eq!(output.played().len(), 1);
        // Still coming out of the speaker, so still not finished.
        std::thread::sleep(SETTLE);
        assert_eq!(speaker.utterance(id), Some(Utterance::Speaking));

        output.finish_playing();
        eventually("the device to be noticed going quiet", || {
            speaker.utterance(id) == Some(Utterance::Finished)
        });
    }

    #[test]
    fn every_reply_gets_an_identity_of_its_own() {
        let output = Arc::new(FakeOutput::default());
        let speaker = Speaker::new(Arc::new(FakeSpeech::instant()), Arc::clone(&output) as _, 0);

        let first = speaker.say(0, "ciao", "alba").unwrap();
        let second = speaker.say(0, "arrivederci", "alba").unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn an_identity_this_speaker_never_issued_is_unknown() {
        // The MCP caller supplies the id, so an id from another conversation —
        // or from nowhere — must not resolve to whatever is current.
        let output = Arc::new(FakeOutput::default());
        let speaker = Speaker::new(Arc::new(FakeSpeech::instant()), output as _, 0);
        assert_eq!(speaker.utterance("999".parse().unwrap()), None);
    }

    #[test]
    fn a_reply_interrupted_before_it_was_rendered_says_interrupted() {
        // The user talked over the queue. Nothing failed.
        let hold = Arc::new(Mutex::new(()));
        let held = hold.lock();
        let output = Arc::new(FakeOutput::default());
        let speaker = Speaker::new(
            Arc::new(FakeSpeech::blocking(Arc::clone(&hold))),
            Arc::clone(&output) as _,
            0,
        );

        let first = speaker.say(0, "primo", "alba").unwrap();
        let second = speaker.say(0, "secondo", "alba").unwrap();
        eventually("the first reply to reach the engine", || {
            speaker.utterance(first) == Some(Utterance::Rendering)
        });
        assert_eq!(speaker.utterance(second), Some(Utterance::Queued));

        speaker.hush();
        drop(held);

        assert_eq!(speaker.utterance(second), Some(Utterance::Interrupted));
        eventually("the rendering reply to report interrupted", || {
            speaker.utterance(first) == Some(Utterance::Interrupted)
        });
    }

    #[test]
    fn a_reply_the_engine_refused_is_failed_and_says_why() {
        // A failure has to be distinguishable from an interruption: one is
        // worth telling the user about and the other is the conversation
        // working as designed.
        let output = Arc::new(FakeOutput::default());
        let speaker = Speaker::new(Arc::new(BrokenSpeech), Arc::clone(&output) as _, 0);

        let id = speaker.say(0, "ciao", "alba").unwrap();

        eventually("the failure to be recorded", || {
            matches!(speaker.utterance(id), Some(Utterance::Failed(_)))
        });
        let Some(Utterance::Failed(reason)) = speaker.utterance(id) else {
            unreachable!("just asserted")
        };
        assert!(reason.contains("not downloaded"), "{reason}");
        assert!(output.played().is_empty());
    }

    #[test]
    fn a_reply_the_device_refused_is_failed_rather_than_finished() {
        let output = Arc::new(FakeOutput::failing("the speaker is gone"));
        let speaker = Speaker::new(Arc::new(FakeSpeech::instant()), Arc::clone(&output) as _, 0);

        let id = speaker.say(0, "ciao", "alba").unwrap();

        eventually("the device failure to be recorded", || {
            matches!(speaker.utterance(id), Some(Utterance::Failed(_)))
        });
        assert_eq!(
            speaker.utterance(id),
            Some(Utterance::Failed("the speaker is gone".to_string()))
        );
    }

    #[test]
    fn a_reply_already_heard_in_full_is_not_rewritten_as_interrupted() {
        // Two interruptions in a row, the second after the speaker has gone
        // quiet, must not retract a reply the user actually heard.
        let output = Arc::new(FakeOutput::default());
        let speaker = Speaker::new(Arc::new(FakeSpeech::instant()), Arc::clone(&output) as _, 0);

        let id = speaker.say(0, "ciao", "alba").unwrap();
        eventually("the reply to reach the device", || {
            speaker.utterance(id) == Some(Utterance::Speaking)
        });

        output.finish_playing();
        speaker.hush();

        assert_eq!(speaker.utterance(id), Some(Utterance::Finished));
    }

    #[test]
    fn a_reply_cut_off_mid_sentence_is_interrupted_not_finished() {
        let output = Arc::new(FakeOutput::default());
        let speaker = Speaker::new(Arc::new(FakeSpeech::instant()), Arc::clone(&output) as _, 0);

        let id = speaker.say(0, "ciao", "alba").unwrap();
        eventually("the reply to reach the device", || {
            speaker.utterance(id) == Some(Utterance::Speaking)
        });

        // Still coming out when the user starts talking.
        speaker.hush();

        assert_eq!(speaker.utterance(id), Some(Utterance::Interrupted));
        assert_eq!(output.stops(), 1);
    }

    #[test]
    fn a_reply_rendered_for_a_turn_that_ended_reports_interrupted() {
        // The audio is finished and correct, and is thrown away because the
        // turn moved on while it rendered. Reachable whenever an engine does
        // not notice the cancel flag — it checks between frames, and the last
        // frame has no "between" after it. The caller has to see this as an
        // interruption rather than as a reply nobody ever accounted for.
        let hold = Arc::new(Mutex::new(()));
        let held = hold.lock();
        let output = Arc::new(FakeOutput::default());
        let speaker = Speaker::new(
            Arc::new(DeafSpeech {
                hold: Arc::clone(&hold),
            }),
            Arc::clone(&output) as _,
            0,
        );

        let id = speaker.say(0, "ciao", "alba").unwrap();
        eventually("the reply to reach the engine", || {
            speaker.utterance(id) == Some(Utterance::Rendering)
        });

        speaker.hush();
        drop(held);

        eventually("the stale reply to be discarded", || {
            speaker.utterance(id) == Some(Utterance::Interrupted)
        });
        assert!(
            output.played().is_empty(),
            "audio for a turn that ended reached the speaker"
        );
    }

    #[test]
    fn the_fates_it_remembers_are_bounded() {
        // A long conversation must not grow a map nothing prunes. Forgetting
        // the oldest is right: a caller polls for the reply it just queued.
        let output = Arc::new(FakeOutput::default());
        let speaker = Speaker::new(Arc::new(FakeSpeech::instant()), Arc::clone(&output) as _, 0);

        let first = speaker.say(0, "ciao", "alba").unwrap();
        eventually("the first reply to reach the device", || {
            speaker.utterance(first) == Some(Utterance::Speaking)
        });

        // Well past the bound, one at a time so the queue never fills.
        for n in 0..MAX_TRACKED + 2 {
            let id = speaker.say(0, &format!("reply {n}"), "alba").unwrap();
            eventually("the reply to reach the device", || {
                speaker.utterance(id) == Some(Utterance::Speaking)
            });
        }

        assert_eq!(speaker.utterance(first), None, "the oldest is forgotten");
    }

    #[test]
    fn an_idle_speaker_does_not_poll_the_device() {
        // The playback poll exists to notice a device going quiet. An idle
        // speaker has nothing to notice, and a loop spinning at 40 Hz for the
        // life of the app is exactly the kind of cost that never gets found.
        let output = Arc::new(FakeOutput::default());
        let speaker = Speaker::new(Arc::new(FakeSpeech::instant()), Arc::clone(&output) as _, 0);

        let id = speaker.say(0, "ciao", "alba").unwrap();
        eventually("the reply to reach the device", || {
            speaker.utterance(id) == Some(Utterance::Speaking)
        });
        output.finish_playing();
        eventually("playback to be resolved", || {
            speaker.utterance(id) == Some(Utterance::Finished)
        });

        let before = output.speaking_queries();
        std::thread::sleep(PLAYBACK_POLL * 8);
        assert_eq!(
            output.speaking_queries(),
            before,
            "the render thread woke up with nothing playing"
        );
    }
}
