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
            } => write!(f, "reply belongs to turn {generation}, and turn {current} is current"),
            Self::Stopped => write!(f, "the speaker is shutting down"),
        }
    }
}

impl std::error::Error for SpeakError {}

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
    generation: u64,
    text: String,
    voice: String,
}

struct InFlight {
    generation: u64,
    cancel: SpeechCancel,
}

struct State {
    generation: u64,
    queue: VecDeque<Reply>,
    in_flight: Option<InFlight>,
    last_error: Option<String>,
    shutdown: bool,
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
    pub fn say(&self, generation: u64, text: &str, voice: &str) -> Result<(), SpeakError> {
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
        state.queue.push_back(Reply {
            generation,
            text: text.to_string(),
            voice: voice.to_string(),
        });
        self.shared.wake.notify_all();
        Ok(())
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
            let mut state = self.shared.state.lock();
            state.generation += 1;
            state.queue.clear();
            if let Some(in_flight) = state.in_flight.take() {
                in_flight.cancel.cancel();
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
        let Some((reply, cancel)) = next_reply(shared) else {
            return;
        };

        let rendered = speech.synthesize(&reply.text, &reply.voice, &cancel);

        let mut state = shared.state.lock();
        state.in_flight = None;
        if state.shutdown {
            return;
        }
        if state.generation != reply.generation {
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
                Ok(()) => state.last_error = None,
                Err(reason) => {
                    tracing::warn!("speech: the audio device refused a reply: {reason}");
                    state.last_error = Some(reason);
                }
            },
            Err(SpeechError::Cancelled) => {
                // Not a failure. Somebody asked for this.
            }
            Err(error) => {
                tracing::warn!("speech: {error}");
                state.last_error = Some(error.to_string());
            }
        }
    }
}

/// Block until there is a reply worth rendering, or until shutdown.
///
/// Returns `None` only on shutdown.
fn next_reply(shared: &Shared) -> Option<(Reply, SpeechCancel)> {
    let mut state = shared.state.lock();
    loop {
        if state.shutdown {
            return None;
        }
        match state.queue.pop_front() {
            Some(reply) if reply.generation == state.generation => {
                let cancel = SpeechCancel::new();
                state.in_flight = Some(InFlight {
                    generation: reply.generation,
                    cancel: cancel.clone(),
                });
                return Some((reply, cancel));
            }
            // Queued before the turn changed. Dropping it here costs nothing;
            // rendering it first would cost a second of CPU and a second of
            // latency for the reply that does matter.
            Some(_) => continue,
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
        self.player
            .append(rodio::buffer::SamplesBuffer::new(
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
    }

    /// An output that remembers instead of making a noise.
    #[derive(Default)]
    struct FakeOutput {
        recorded: Mutex<Recorded>,
        fail_with: Option<String>,
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
    }

    impl Output for FakeOutput {
        fn play(&self, audio: &SpeechAudio) -> Result<(), String> {
            if let Some(reason) = &self.fail_with {
                return Err(reason.clone());
            }
            self.recorded
                .lock()
                .played
                .push(format!("{:.0}", audio.samples[0]));
            Ok(())
        }

        fn stop(&self) {
            self.recorded.lock().stops += 1;
        }

        fn is_speaking(&self) -> bool {
            false
        }
    }

    /// An engine that renders one sample carrying the text's length, so a test
    /// can recognise which reply came out without a real voice.
    struct FakeSpeech {
        /// Held for as long as a test wants synthesis to be in progress.
        hold: Option<Arc<Mutex<()>>>,
        started: Arc<AtomicUsize>,
        cancelled: Arc<AtomicUsize>,
    }

    impl FakeSpeech {
        fn instant() -> Self {
            Self {
                hold: None,
                started: Arc::new(AtomicUsize::new(0)),
                cancelled: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn blocking(hold: Arc<Mutex<()>>) -> Self {
            Self {
                hold: Some(hold),
                started: Arc::new(AtomicUsize::new(0)),
                cancelled: Arc::new(AtomicUsize::new(0)),
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
            self.started.fetch_add(1, Ordering::SeqCst);
            if let Some(hold) = &self.hold {
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

        assert_eq!(speaker.say(0, "one too many", "v").unwrap_err(), SpeakError::Full);
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
        assert!(output.stops() >= 1, "the device was silenced on the way out");
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

        assert_eq!(speaker.say(0, "ciao", "v").unwrap_err(), SpeakError::Stopped);
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
        assert!(source_parameters(&empty).unwrap_err().contains("no samples"));
    }
}
