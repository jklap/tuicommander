pub mod audio;
pub mod browser;
pub mod commands;
pub mod continuous;
pub mod corrections;
pub mod echo;
pub mod fn_key_monitor;
pub mod language;
pub mod loudness;
pub mod model;
pub mod permission;
pub mod speaker;
pub mod speech;
pub mod streaming;
pub mod transcribe;
pub mod vad;

use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Shared dictation state accessible from Tauri commands.
/// Tauri's `.manage()` wraps this in `Arc` internally, so we don't double-wrap.
pub struct DictationState {
    pub audio: Mutex<Option<audio::AudioCapture>>,
    /// Name of the model currently loaded in `transcriber_arc` (e.g. "large-v3-turbo")
    pub active_model: Mutex<Option<String>>,
    pub corrections: Arc<Mutex<corrections::TextCorrector>>,
    pub recording: AtomicBool,
    pub processing: Arc<AtomicBool>,
    /// Active streaming session (None when not streaming).
    pub streaming: Mutex<Option<streaming::StreamingSession>>,
    /// Arc-wrapped transcriber for sharing with the streaming thread.
    pub transcriber_arc: Mutex<Option<Arc<dyn transcribe::Transcriber>>>,
    /// Concatenation of all streaming partials (for accuracy comparison logging).
    pub accumulated_partials: Arc<Mutex<String>>,
    /// Hands-free mode. Separate from `recording` on purpose: push-to-talk and
    /// hands-free are different modes and neither arms the other.
    ///
    /// `Arc` because the runtime thread drives it between ticks.
    pub hands_free: Arc<Mutex<continuous::HandsFree>>,
    /// The thread driving hands-free capture while it is armed.
    pub hands_free_runtime: Mutex<Option<continuous::HandsFreeRuntime>>,
    /// The microphone hands-free captures from. Deliberately **not** `audio`:
    /// that one belongs to push-to-talk, and a shared slot would let either
    /// mode close the other's device.
    ///
    /// It stays on this side of the thread boundary because a `cpal::Stream` is
    /// `!Send` — the runtime only ever sees the buffer handle.
    pub hands_free_audio: Mutex<Option<audio::AudioCapture>>,
    /// Liveness of the desktop audio endpoint. Cleared when that endpoint is
    /// released, which is how the runtime learns its owner is gone.
    pub hands_free_owner_alive: Arc<AtomicBool>,
    /// Acoustic echo cancellation, shared by the two threads that must agree
    /// about it: the hands-free runtime cleans capture with it, and the reply
    /// queue's [`echo::FarEndTap`] tells it what is being played.
    ///
    /// One instance for the life of the app rather than one per arm. The
    /// canceller's delay estimator is state worth keeping — rebuilding it every
    /// time the mode is armed would make the first second of every session the
    /// worst one.
    pub echo: Arc<Mutex<echo::EchoGuard>>,
    /// The downloadable speech assets, and the engines loaded from them.
    ///
    /// One for the app rather than one per reply: it is what serialises
    /// replacing a language against speaking it, which only works if both
    /// sides go through the same instance.
    pub speech: Arc<speech::library::SpeechLibrary>,
    /// The reply queue for the armed conversation.
    ///
    /// `None` whenever hands-free is not armed, and that is the answer the
    /// voice capability gives a model that calls it then: speech belongs to a
    /// conversation, not to the application. Built on arm because it needs a
    /// loaded engine and an open audio device, both of which are worth holding
    /// only while somebody is listening.
    ///
    /// `Arc` because the hands-free capture loop holds the same slot as its
    /// barge-in port: it must interrupt whatever is speaking *now*, which
    /// under Auto is a voice that did not exist when the loop started.
    pub speaker: Arc<Mutex<Option<speaker::Armed>>>,
    /// Where an utterance transition goes once a conversation is armed.
    ///
    /// Held here rather than passed in because the two places that build a
    /// reply queue — arming, and the first `speak` under Auto — have no
    /// `AppHandle` between them. Installed once at startup by the one caller
    /// that does. `None` in a test and in the headless build: a speaker nobody
    /// watches is the ordinary case for a unit test, not a misconfiguration.
    pub utterance_observer: Mutex<Option<Arc<dyn speaker::UtteranceObserver>>>,
    /// Browser and remote clients holding an audio socket, by owner id.
    ///
    /// Lives here rather than in `AppState` because it is only ever reached
    /// from the two dictation paths that already take this state: the socket
    /// handler that registers a client, and arming, which looks one up. Empty
    /// on a desktop-only install, which is the ordinary case.
    pub browser_endpoints: Arc<browser::BrowserEndpoints>,
}

impl DictationState {
    pub fn new() -> Self {
        Self {
            audio: Mutex::new(None),
            active_model: Mutex::new(None),
            corrections: Arc::new(Mutex::new(corrections::TextCorrector::load_or_default())),
            recording: AtomicBool::new(false),
            processing: Arc::new(AtomicBool::new(false)),
            streaming: Mutex::new(None),
            transcriber_arc: Mutex::new(None),
            accumulated_partials: Arc::new(Mutex::new(String::new())),
            hands_free: Arc::new(Mutex::new(continuous::HandsFree::new(
                commands::default_hold_back_ms().into(),
            ))),
            hands_free_runtime: Mutex::new(None),
            hands_free_audio: Mutex::new(None),
            hands_free_owner_alive: Arc::new(AtomicBool::new(false)),
            echo: Arc::new(Mutex::new(echo::install())),
            speech: Arc::new(speech::library::SpeechLibrary::new()),
            speaker: Arc::new(Mutex::new(None)),
            utterance_observer: Mutex::new(None),
            browser_endpoints: Arc::new(browser::BrowserEndpoints::default()),
        }
    }

    /// Clean shutdown: stop the streaming thread, release audio, then drop the
    /// transcriber. Order matters — the streaming thread holds an Arc clone of
    /// the transcriber, so we must join it before the WhisperContext can be freed.
    pub fn shutdown(&self) {
        // 0. Release the desktop audio endpoint and disarm hands-free.
        //    The disarm is done here rather than left to the runtime: dropping
        //    the runtime stops its thread, so racing it for the owner flag
        //    would leave the mode armed on some runs and not others.
        self.hands_free_owner_alive
            .store(false, std::sync::atomic::Ordering::Release);
        self.hands_free
            .lock()
            .disarm(continuous::DisarmReason::OwnerDisconnected);
        *self.hands_free_runtime.lock() = None;
        *self.hands_free_audio.lock() = None;
        // 0b. Stop talking. Before the engine goes away below: dropping the
        //     queue cancels the reply in flight, and an `ort::Session` freed
        //     under a thread still rendering from it is undefined rather than
        //     merely abrupt.
        *self.speaker.lock() = None;
        // 1. Stop audio capture (upstream source)
        *self.audio.lock() = None;
        // 2. Stop + join the streaming thread (Drop impl signals stop flag)
        //    This releases the thread's Arc<dyn Transcriber> clone.
        *self.streaming.lock() = None;
        // 3. Now safe to drop the transcriber — no other Arc holders remain.
        *self.transcriber_arc.lock() = None;
        // 4. Let go of the speech graphs. Separate from the transcriber and
        //    last because it waits for any synthesis in flight: a reply being
        //    spoken as the app closes finishes its sentence rather than being
        //    cut off mid-word by a freed ONNX session.
        self.speech.shutdown();
    }
}
