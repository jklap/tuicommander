//! Hearing the user over our own voice.
//!
//! The microphone hears the speaker. Without this, the energy VAD in
//! [`continuous`](super::continuous) opens a turn on the reply the application
//! is speaking, transcribes it, and answers itself. Muting capture while
//! speaking would stop that and would also stop the user interrupting, which
//! is the one thing hands-free has to get right — so the capture stream stays
//! open and the echo is subtracted from it instead.
//!
//! ## The two streams must arrive in step
//!
//! A canceller subtracts a delayed, filtered copy of the **far end** (what we
//! played) from the **near end** (what the microphone heard). It can only do
//! that if it is fed both at the same pace: one 10 ms frame of each, in turn.
//!
//! Our two sources do not behave that way. Capture arrives in small chunks as
//! the device produces them, while [`speaker`](super::speaker) hands over a
//! whole rendered utterance at once — seconds of audio in one call, before a
//! single sample of it has left the speaker. Pushing that straight into a
//! canceller would put it seconds ahead of the microphone and it would subtract
//! nothing.
//!
//! [`FarEnd`] is the buffer that fixes the pace: the reply goes in whole, and
//! comes out only as fast as capture is consumed, padded with silence whenever
//! nothing is playing. Alignment is therefore by sample count rather than by
//! wall clock, and the residual offset — the device's own output latency — is
//! what the canceller's delay estimator is for.
//!
//! ## Everything here runs at 16 kHz
//!
//! Not a preference: [`audio`](super::audio) already converts capture to mono
//! 16 kHz for Whisper, and 16 kHz is one of the rates the WebRTC APM accepts,
//! so the near end needs no conversion at all. The far end is whatever the
//! speech engine rendered — 24 kHz for Pocket TTS, anything for a
//! user-supplied command — and is converted on the way in.

// Same reason as `speech.rs` and `speaker.rs`: the port and its adapter are
// finished and tested, and the consumer that arms them is the hands-free
// runtime in `continuous.rs`. Drop this when that wiring lands.
#![allow(dead_code)]

pub mod webrtc;

use std::collections::VecDeque;

use super::speech::SpeechAudio;

/// The rate both streams are fed at, which is the rate capture already
/// arrives at. See the module note.
pub const SAMPLE_RATE: u32 = 16_000;

/// 10 ms. The APM takes this and nothing else.
pub const FRAME_SAMPLES: usize = SAMPLE_RATE as usize / 100;

/// How much un-consumed reply audio the far end will hold: two seconds.
///
/// Reached only when playback and capture are out of step — the mode was
/// disarmed mid-reply, or the device stopped delivering. Holding more would
/// not improve cancellation, because audio this old no longer corresponds to
/// anything the microphone is about to hear.
const FAR_END_CAPACITY: usize = SAMPLE_RATE as usize * 2;

/// Subtracts our own voice from what the microphone heard.
///
/// A port so the pace-keeping in [`EchoGuard`] and the wiring in
/// [`continuous`](super::continuous) can be proven without a canceller, and so
/// a build without one degrades to [`PassThrough`] instead of failing to arm.
pub trait Canceller: Send {
    /// Subtract `far_end` from `near_end`, in place. Both are exactly
    /// [`FRAME_SAMPLES`] long, mono, at [`SAMPLE_RATE`].
    fn cancel(&mut self, far_end: &[f32], near_end: &mut [f32]);
}

/// No cancellation. What a build without an echo canceller gets.
///
/// Not a silent fallback: whoever installs this owns saying so. It exists
/// because a hands-free mode that refuses to arm is worse than one that cannot
/// be interrupted over the speaker — headphones still work.
#[derive(Debug, Default)]
pub struct PassThrough;

impl Canceller for PassThrough {
    fn cancel(&mut self, _far_end: &[f32], _near_end: &mut [f32]) {}
}

/// Reply audio waiting to be matched against capture.
///
/// Filled a whole utterance at a time, drained one capture chunk at a time.
#[derive(Debug, Default)]
pub struct FarEnd {
    samples: VecDeque<f32>,
    /// Samples dropped because capture never caught up. Reported rather than
    /// hidden: a climbing count means playback and capture have lost step,
    /// which shows up as an echo that stops being cancelled.
    dropped: u64,
}

impl FarEnd {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a rendered reply.
    pub fn push(&mut self, audio: &SpeechAudio) {
        if audio.sample_rate == 0 || audio.samples.is_empty() {
            return;
        }
        if audio.sample_rate == SAMPLE_RATE {
            self.samples.extend(audio.samples.iter().copied());
        } else {
            self.samples
                .extend(resample(&audio.samples, audio.sample_rate));
        }
        if self.samples.len() > FAR_END_CAPACITY {
            let excess = self.samples.len() - FAR_END_CAPACITY;
            // Drop the oldest. What is kept is what the microphone is about to
            // hear; the head is audio the speaker has already finished with.
            self.samples.drain(..excess);
            self.dropped += excess as u64;
        }
    }

    /// Take the next `count` samples, padding with silence.
    ///
    /// Silence is the truthful answer when nothing is playing, and it is also
    /// what keeps the canceller converged through the gaps between replies:
    /// skipping the call instead would make it lose its place.
    pub fn take(&mut self, count: usize) -> Vec<f32> {
        let mut frame: Vec<f32> = self
            .samples
            .drain(..count.min(self.samples.len()))
            .collect();
        frame.resize(count, 0.0);
        frame
    }

    /// Forget the queued reply. Called when playback is stopped: that audio is
    /// never going to reach the microphone, so matching capture against it
    /// would subtract a sound that is not there.
    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// Is any reply audio still waiting to be matched?
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

/// Linear interpolation to [`SAMPLE_RATE`].
///
/// Linear rather than the nearest-neighbour [`audio`](super::audio) uses on
/// the capture path. There it feeds a recogniser that tolerates the artefacts;
/// here it feeds a canceller that models the far end as a *linear* filter of
/// this signal, so every artefact this adds is echo it cannot subtract.
fn resample(samples: &[f32], from_rate: u32) -> Vec<f32> {
    let ratio = f64::from(SAMPLE_RATE) / f64::from(from_rate);
    let out_len = ((samples.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for n in 0..out_len {
        let position = n as f64 / ratio;
        let left = position.floor() as usize;
        if left + 1 >= samples.len() {
            out.push(samples[samples.len() - 1]);
            continue;
        }
        let fraction = (position - left as f64) as f32;
        out.push(samples[left] * (1.0 - fraction) + samples[left + 1] * fraction);
    }
    out
}

/// The far-end buffer and the canceller behind it, kept in step.
///
/// One of these is shared by [`speaker`](super::speaker), which fills it, and
/// the hands-free tick, which drains it.
pub struct EchoGuard {
    far_end: FarEnd,
    canceller: Box<dyn Canceller>,
    /// Capture samples left over from the last call: the canceller works in
    /// whole 10 ms frames and a device chunk is not a multiple of one.
    remainder: Vec<f32>,
}

impl EchoGuard {
    pub fn new(canceller: Box<dyn Canceller>) -> Self {
        Self {
            far_end: FarEnd::new(),
            canceller,
            remainder: Vec::new(),
        }
    }

    /// A reply about to be played. Called by the render thread, before the
    /// audio reaches the device, so the far end is never behind the near end.
    pub fn note_rendered(&mut self, audio: &SpeechAudio) {
        self.far_end.push(audio);
    }

    /// Playback was stopped. Drop the queued reply and the partial frame with
    /// it — both describe a sound that is no longer going to happen.
    pub fn note_stopped(&mut self) {
        self.far_end.clear();
    }

    /// Clean one chunk of capture.
    ///
    /// Returns the samples it could clean, which is everything but a partial
    /// trailing frame — under 10 ms, carried into the next call. The caller
    /// gets fewer samples than it gave, never different ones, and never out of
    /// order.
    pub fn clean(&mut self, capture: &[f32]) -> Vec<f32> {
        self.remainder.extend_from_slice(capture);
        let frames = self.remainder.len() / FRAME_SAMPLES;
        if frames == 0 {
            return Vec::new();
        }

        let mut cleaned: Vec<f32> = self.remainder.drain(..frames * FRAME_SAMPLES).collect();
        for frame in cleaned.chunks_exact_mut(FRAME_SAMPLES) {
            let far_end = self.far_end.take(FRAME_SAMPLES);
            self.canceller.cancel(&far_end, frame);
        }
        cleaned
    }

    /// Is a reply still queued against capture? The hands-free tick asks this
    /// to tell "the user is talking" from "we are still hearing ourselves".
    pub fn is_playing(&self) -> bool {
        !self.far_end.is_empty()
    }

    pub fn dropped_far_end(&self) -> u64 {
        self.far_end.dropped()
    }
}

/// Installs a canceller, saying out loud which one.
///
/// Never fails: a hands-free mode that refuses to arm is worse than one that
/// cannot be interrupted over the speaker, and headphones still work. The
/// warning is the whole point — [`PassThrough`] must not be reached quietly.
pub fn install() -> EchoGuard {
    match webrtc::WebRtc::new() {
        Ok(canceller) => EchoGuard::new(Box::new(canceller)),
        Err(reason) => {
            tracing::warn!(
                "dictation: no echo cancellation ({reason}). Hands-free will hear its own \
                 replies unless you wear headphones."
            );
            EchoGuard::new(Box::new(PassThrough))
        }
    }
}

/// A [`speaker::Output`](super::speaker::Output) that tells an [`EchoGuard`]
/// what it is about to play.
///
/// The far-end tap belongs here rather than inside
/// [`Speaker`](super::speaker::Speaker): the queue's job is deciding *what* is
/// played and when it stops, and both of those are already expressed through
/// the `Output` port. Wrapping the port catches every one of them without the
/// queue growing an opinion about microphones.
pub struct FarEndTap {
    inner: std::sync::Arc<dyn super::speaker::Output>,
    guard: std::sync::Arc<parking_lot::Mutex<EchoGuard>>,
}

impl FarEndTap {
    pub fn new(
        inner: std::sync::Arc<dyn super::speaker::Output>,
        guard: std::sync::Arc<parking_lot::Mutex<EchoGuard>>,
    ) -> Self {
        Self { inner, guard }
    }
}

impl super::speaker::Output for FarEndTap {
    fn play(&self, audio: &SpeechAudio) -> Result<(), String> {
        // Noted before the device is handed the audio, never after: the far
        // end may run ahead of what the microphone hears, because alignment is
        // by sample count and the canceller's estimator absorbs the offset. It
        // may not run behind, because reference that arrives after its own
        // echo cannot be subtracted from it.
        self.guard.lock().note_rendered(audio);
        self.inner.play(audio)
    }

    fn stop(&self) {
        // Device first. The far end is what remains to be cancelled, so
        // forgetting it before the speaker is actually quiet would leave the
        // last few milliseconds of the reply in the microphone with no
        // reference to subtract.
        self.inner.stop();
        self.guard.lock().note_stopped();
    }

    fn is_speaking(&self) -> bool {
        self.inner.is_speaking()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(samples: Vec<f32>, sample_rate: u32) -> SpeechAudio {
        SpeechAudio {
            samples,
            sample_rate,
        }
    }

    /// A guard whose canceller can be read back afterwards.
    struct Probe {
        guard: EchoGuard,
        seen: std::sync::Arc<parking_lot::Mutex<Vec<Vec<f32>>>>,
    }

    /// Records what it was asked to subtract, and subtracts it exactly. A real
    /// canceller is adaptive; this one lets a test state what reached it.
    struct Tee(std::sync::Arc<parking_lot::Mutex<Vec<Vec<f32>>>>);

    impl Canceller for Tee {
        fn cancel(&mut self, far_end: &[f32], near_end: &mut [f32]) {
            self.0.lock().push(far_end.to_vec());
            for (near, far) in near_end.iter_mut().zip(far_end) {
                *near -= far;
            }
        }
    }

    impl Probe {
        fn new() -> Self {
            let seen = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
            Self {
                guard: EchoGuard::new(Box::new(Tee(std::sync::Arc::clone(&seen)))),
                seen,
            }
        }

        /// The far-end frames the canceller was handed, flattened.
        fn far_end_seen(&self) -> Vec<f32> {
            self.seen.lock().iter().flatten().copied().collect()
        }
    }

    #[test]
    fn a_whole_reply_is_released_only_as_fast_as_capture_is_consumed() {
        // The case the buffer exists for: the speaker hands over a second of
        // audio at once, long before any of it has been heard.
        let mut probe = Probe::new();
        probe
            .guard
            .note_rendered(&audio(vec![1.0; SAMPLE_RATE as usize], SAMPLE_RATE));

        let cleaned = probe.guard.clean(&vec![0.0; FRAME_SAMPLES]);

        assert_eq!(cleaned.len(), FRAME_SAMPLES, "one frame in, one frame out");
        assert_eq!(
            probe.far_end_seen().len(),
            FRAME_SAMPLES,
            "the canceller was handed one frame, not a second of audio"
        );
        assert!(probe.guard.is_playing(), "the rest is still queued");
    }

    #[test]
    fn a_partial_frame_is_carried_rather_than_cleaned_early_or_dropped() {
        // A device chunk is not a multiple of 10 ms. The tail must reappear,
        // in order, in the next call — losing it would notch the audio the
        // recogniser sees.
        let mut probe = Probe::new();
        let first: Vec<f32> = (0..FRAME_SAMPLES + 40).map(|n| n as f32).collect();
        let second: Vec<f32> = (0..FRAME_SAMPLES).map(|n| (n + 1000) as f32).collect();

        let a = probe.guard.clean(&first);
        let b = probe.guard.clean(&second);

        assert_eq!(a.len(), FRAME_SAMPLES);
        assert_eq!(b.len(), FRAME_SAMPLES);
        let seen: Vec<f32> = a.into_iter().chain(b).collect();
        let expected: Vec<f32> = first
            .iter()
            .chain(second.iter())
            .take(2 * FRAME_SAMPLES)
            .copied()
            .collect();
        assert_eq!(seen, expected, "samples were reordered or lost");
    }

    #[test]
    fn capture_shorter_than_a_frame_cleans_nothing_yet() {
        let mut probe = Probe::new();
        assert!(probe.guard.clean(&[0.1; 40]).is_empty());
        assert!(
            probe.seen.lock().is_empty(),
            "the canceller was called early"
        );
    }

    #[test]
    fn silence_is_fed_when_nothing_is_playing() {
        // Skipping the call instead would let an adaptive canceller lose its
        // place between replies, so the first frame of the next one comes
        // through uncancelled.
        let mut probe = Probe::new();

        probe.guard.clean(&vec![0.5; FRAME_SAMPLES]);

        assert_eq!(probe.far_end_seen(), vec![0.0; FRAME_SAMPLES]);
        assert!(!probe.guard.is_playing());
    }

    #[test]
    fn stopping_playback_forgets_the_reply_that_will_not_be_heard() {
        // hush() stopped the device. Matching capture against audio nobody
        // played would subtract a sound that is not in it.
        let mut probe = Probe::new();
        probe
            .guard
            .note_rendered(&audio(vec![1.0; SAMPLE_RATE as usize], SAMPLE_RATE));

        probe.guard.note_stopped();
        probe.guard.clean(&vec![0.0; FRAME_SAMPLES]);

        assert_eq!(probe.far_end_seen(), vec![0.0; FRAME_SAMPLES]);
        assert!(!probe.guard.is_playing());
    }

    #[test]
    fn the_echo_is_actually_subtracted_from_the_capture_that_is_returned() {
        let mut probe = Probe::new();
        probe
            .guard
            .note_rendered(&audio(vec![0.25; FRAME_SAMPLES], SAMPLE_RATE));

        let cleaned = probe.guard.clean(&vec![0.75; FRAME_SAMPLES]);

        assert_eq!(cleaned, vec![0.5; FRAME_SAMPLES]);
    }

    #[test]
    fn a_reply_at_the_engines_rate_is_converted_on_the_way_in() {
        // Pocket TTS renders at 24 kHz. Feeding that to a 16 kHz canceller
        // unconverted would hand it a far end that is 1.5x too fast, which
        // correlates with nothing the microphone heard.
        let mut far_end = FarEnd::new();
        far_end.push(&audio(vec![1.0; 2_400], 24_000));

        assert_eq!(
            far_end.take(10_000).iter().filter(|s| **s != 0.0).count(),
            1_600
        );
    }

    #[test]
    fn conversion_interpolates_rather_than_picking_the_nearest_sample() {
        // A ramp resampled by nearest neighbour comes out as steps, and every
        // step is echo a linear canceller cannot model.
        let ramp: Vec<f32> = (0..300).map(|n| n as f32).collect();

        let converted = resample(&ramp, 30_000);

        // 30 kHz -> 16 kHz: output sample n sits at input position n * 30/16.
        assert_eq!(converted.len(), 160);
        assert!((converted[1] - 1.875).abs() < 1e-4, "{}", converted[1]);
        assert!((converted[3] - 5.625).abs() < 1e-4, "{}", converted[3]);
    }

    #[test]
    fn a_reply_already_at_the_capture_rate_is_passed_through_untouched() {
        let mut far_end = FarEnd::new();
        let samples: Vec<f32> = (0..FRAME_SAMPLES).map(|n| n as f32).collect();
        far_end.push(&audio(samples.clone(), SAMPLE_RATE));

        assert_eq!(far_end.take(FRAME_SAMPLES), samples);
    }

    #[test]
    fn a_reply_nobody_consumes_is_bounded_rather_than_unbounded() {
        // Disarming mid-reply, or a device that stops delivering. The buffer
        // must not grow with the conversation.
        let mut far_end = FarEnd::new();
        for _ in 0..10 {
            far_end.push(&audio(vec![1.0; SAMPLE_RATE as usize], SAMPLE_RATE));
        }

        assert!(far_end.samples.len() <= FAR_END_CAPACITY);
        assert!(far_end.dropped() > 0, "dropping silently is the failure");
    }

    #[test]
    fn the_newest_audio_is_the_audio_that_is_kept() {
        // What the microphone is about to hear is the end of the queue, not
        // the start. Dropping from the tail would keep the wrong seconds.
        let mut far_end = FarEnd::new();
        far_end.push(&audio(vec![1.0; FAR_END_CAPACITY], SAMPLE_RATE));
        far_end.push(&audio(vec![2.0; FRAME_SAMPLES], SAMPLE_RATE));

        let all = far_end.take(FAR_END_CAPACITY);
        assert_eq!(
            &all[all.len() - FRAME_SAMPLES..],
            &vec![2.0; FRAME_SAMPLES][..]
        );
    }

    #[test]
    fn an_engine_that_reports_nothing_playable_is_ignored_rather_than_queued() {
        let mut far_end = FarEnd::new();
        far_end.push(&audio(Vec::new(), SAMPLE_RATE));
        far_end.push(&audio(vec![1.0; 100], 0));

        assert!(far_end.is_empty());
    }

    #[test]
    fn without_a_canceller_the_capture_is_returned_as_it_arrived() {
        // The degraded build. It must still be a working microphone.
        let mut guard = EchoGuard::new(Box::new(PassThrough));
        guard.note_rendered(&audio(vec![1.0; FRAME_SAMPLES], SAMPLE_RATE));
        let capture: Vec<f32> = (0..FRAME_SAMPLES).map(|n| n as f32 * 0.01).collect();

        assert_eq!(guard.clean(&capture), capture);
    }

    #[test]
    fn frames_reach_the_canceller_in_the_order_they_were_rendered() {
        // Two replies queued back to back come out as one continuous far end.
        let mut probe = Probe::new();
        probe
            .guard
            .note_rendered(&audio(vec![1.0; FRAME_SAMPLES], SAMPLE_RATE));
        probe
            .guard
            .note_rendered(&audio(vec![2.0; FRAME_SAMPLES], SAMPLE_RATE));

        probe.guard.clean(&vec![0.0; FRAME_SAMPLES * 2]);

        let seen = probe.far_end_seen();
        assert_eq!(seen[..FRAME_SAMPLES], vec![1.0; FRAME_SAMPLES][..]);
        assert_eq!(seen[FRAME_SAMPLES..], vec![2.0; FRAME_SAMPLES][..]);
    }
}
