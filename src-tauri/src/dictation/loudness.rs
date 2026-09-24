//! The loudness stage: every spoken reply at one level, whatever the voice.
//!
//! Voices differ by more than 10 dB in their rendered level, and one reply
//! drifts within itself. This stage runs offline on the whole buffer before it
//! is played, in four steps:
//!
//! 1. **Levelling.** A 300 ms running RMS envelope drives a gain toward the
//!    reply's own speech level, with a ratio from `levelling` (0 = off, 1 = 4:1)
//!    and at most +12 dB of boost. Samples whose envelope is below the -50 dBFS
//!    gate are silence and are not levelled.
//! 2. **Gain** to `volume_db`, measured over speech only. Gated samples are
//!    never boosted, so the noise floor between sentences stays where it was.
//! 3. **rodio's `Limit`**, for peaks the gain pushed near full scale.
//! 4. **A hard clamp** at -1 dBFS, because a limiter with an attack time can
//!    still let the first samples of a transient through.

// Same reason as `echo.rs`: the stage is finished and tested, and its consumer
// is `speaker.rs` `render_loop`, wired in Step 6 of
// plans/pocket-voices-and-loudness.md. Drop this when that wiring lands.
#![allow(dead_code)]

use std::num::NonZero;

use rodio::Source;
use rodio::buffer::SamplesBuffer;
use rodio::source::LimitSettings;

use super::speech::SpeechAudio;

/// Seconds of audio the levelling envelope averages over.
const ENVELOPE_SECONDS: f32 = 0.3;
/// Seconds of audio the level measurement averages over. Short, so that the
/// silence next to a sentence does not count as speech and pull the level down.
const MEASURE_SECONDS: f32 = 0.02;
/// Below this level a sample is silence.
const GATE_DB: f32 = -50.0;
const MAX_BOOST_DB: f32 = 12.0;
const CEILING_DB: f32 = -1.0;

/// How a reply is levelled. `volume_db` is the speech level in dBFS
/// (-30..=-12); `levelling` is the strength of the levelling (0..=1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Loudness {
    pub volume_db: f32,
    pub levelling: f32,
}

/// Compression ratio of the levelling: 0 → 1:1 (off), 1 → 4:1.
fn ratio(levelling: f32) -> f32 {
    1.0 + 3.0 * levelling
}

/// Bring `audio` to `s.volume_db` in place. Empty audio, a zero sample rate and
/// audio with no sample above the gate are left untouched.
pub fn process(audio: &mut SpeechAudio, s: Loudness) {
    let Some(rate) = NonZero::new(audio.sample_rate) else {
        return;
    };
    let gate = db_to_power(GATE_DB);
    let measure = window(rate, MEASURE_SECONDS);
    let Some(reference) = speech_power(&audio.samples, measure, gate) else {
        return;
    };
    let envelope = running_mean_square(&audio.samples, window(rate, ENVELOPE_SECONDS));

    // Power ratio to amplitude gain: half the exponent. A ratio of 1 gives an
    // exponent of 0, and the loop is skipped so the gain stays exactly 1.
    let exponent = 0.5 * (1.0 - 1.0 / ratio(s.levelling.clamp(0.0, 1.0)));
    if exponent > 0.0 {
        let max_boost = db_to_amplitude(MAX_BOOST_DB);
        for (x, &p) in audio.samples.iter_mut().zip(&envelope) {
            if p >= gate {
                *x *= (reference / p).powf(exponent).min(max_boost);
            }
        }
    }

    let Some(level) = speech_power(&audio.samples, measure, gate) else {
        return;
    };
    let makeup = (db_to_power(s.volume_db) / level).sqrt();
    let silence_gain = makeup.min(1.0);
    for (x, &p) in audio.samples.iter_mut().zip(&envelope) {
        *x *= if p >= gate { makeup } else { silence_gain };
    }

    let ceiling = db_to_amplitude(CEILING_DB);
    let mono = NonZero::new(1).expect("one channel is not zero");
    let samples = std::mem::take(&mut audio.samples);
    audio.samples = SamplesBuffer::new(mono, rate, samples)
        .limit(LimitSettings::default().with_threshold(CEILING_DB))
        .map(|x| x.clamp(-ceiling, ceiling))
        .collect();
}

fn window(rate: NonZero<u32>, seconds: f32) -> usize {
    ((rate.get() as f32 * seconds) as usize).max(1)
}

/// Mean square over a centred window of `width` samples, O(n) with a running
/// sum. Outside the buffer counts as silence, so the first and last words of a
/// reply are levelled like a word after a pause.
fn running_mean_square(samples: &[f32], width: usize) -> Vec<f32> {
    let half = width / 2;
    let n = samples.len();
    let square = |i: usize| {
        let x = samples[i] as f64;
        x * x
    };
    let mut sum: f64 = (0..half.min(n)).map(square).sum();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        if i + half < n {
            sum += square(i + half);
        }
        if i > half {
            sum -= square(i - half - 1);
        }
        out.push((sum.max(0.0) / width as f64) as f32);
    }
    out
}

/// Mean power of the samples whose short envelope is above the gate, or `None`
/// when there are none.
fn speech_power(samples: &[f32], width: usize, gate: f32) -> Option<f32> {
    let envelope = running_mean_square(samples, width);
    let (sum, count) = samples
        .iter()
        .zip(&envelope)
        .filter(|&(_, &p)| p >= gate)
        .fold((0.0f64, 0usize), |(sum, count), (&x, _)| {
            (sum + (x as f64) * (x as f64), count + 1)
        });
    (count > 0 && sum > 0.0).then(|| (sum / count as f64) as f32)
}

fn db_to_power(db: f32) -> f32 {
    10f32.powf(db / 10.0)
}

fn db_to_amplitude(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;
    use std::time::{Duration, Instant};

    const RATE: u32 = 24_000;
    const DEFAULT: Loudness = Loudness {
        volume_db: -18.0,
        levelling: 0.67,
    };

    fn samples(seconds: f32) -> usize {
        (RATE as f32 * seconds) as usize
    }

    /// A sine at `rms_db` dBFS RMS.
    fn sine(freq: f32, rms_db: f32, seconds: f32) -> Vec<f32> {
        let amplitude = db_to_amplitude(rms_db) * 2f32.sqrt();
        (0..samples(seconds))
            .map(|i| amplitude * (2.0 * PI * freq * i as f32 / RATE as f32).sin())
            .collect()
    }

    fn rms_db(samples: &[f32]) -> f32 {
        let power = samples
            .iter()
            .map(|&x| (x as f64) * (x as f64))
            .sum::<f64>()
            / samples.len() as f64;
        10.0 * (power.log10() as f32)
    }

    fn audio(samples: Vec<f32>) -> SpeechAudio {
        SpeechAudio {
            samples,
            sample_rate: RATE,
        }
    }

    /// Two one-second bursts at `a` and `b` dBFS, each after half a second of
    /// `floor`, with half a second of `floor` at the end. Returns the audio and
    /// the sample ranges of the two bursts.
    fn two_bursts(
        a: f32,
        b: f32,
        floor: &dyn Fn(f32) -> Vec<f32>,
    ) -> (Vec<f32>, [std::ops::Range<usize>; 2]) {
        let mut out = floor(0.5);
        let first = out.len()..out.len() + samples(1.0);
        out.extend(sine(440.0, a, 1.0));
        out.extend(floor(1.0));
        let second = out.len()..out.len() + samples(1.0);
        out.extend(sine(440.0, b, 1.0));
        out.extend(floor(0.5));
        (out, [first, second])
    }

    fn zeros(seconds: f32) -> Vec<f32> {
        vec![0.0; samples(seconds)]
    }

    #[test]
    fn a_quiet_reply_comes_out_at_the_configured_level() {
        // A voice that renders 14 dB too quiet is the case the stage exists
        // for: without it, the user turns the system volume up for one voice
        // and is shouted at by the next.
        let (input, bursts) = two_bursts(-32.0, -32.0, &zeros);
        let mut reply = audio(input);
        process(&mut reply, DEFAULT);
        for burst in bursts {
            let level = rms_db(&reply.samples[burst]);
            assert!(
                (level - DEFAULT.volume_db).abs() <= 1.0,
                "burst at {level} dBFS, target {} dBFS",
                DEFAULT.volume_db
            );
        }
    }

    #[test]
    fn no_sample_ends_above_minus_one_dbfs() {
        // A transient at full scale inside quiet speech: the gain that lifts the
        // speech would clip it, and the loudest setting leaves no headroom.
        let mut input = sine(440.0, -32.0, 2.0);
        let click = samples(1.0);
        input[click..click + 24].fill(0.99);
        input[click + 24..click + 48].fill(-0.99);
        let mut reply = audio(input);
        process(
            &mut reply,
            Loudness {
                volume_db: -12.0,
                levelling: 1.0,
            },
        );
        let peak = reply.samples.iter().fold(0f32, |m, &x| m.max(x.abs()));
        assert!(
            peak <= db_to_amplitude(CEILING_DB),
            "peak {} dBFS",
            20.0 * peak.log10()
        );
    }

    #[test]
    fn the_noise_floor_between_sentences_is_not_boosted() {
        // A -60 dBFS hum in the pauses. Lifting it with the speech would turn
        // every pause into audible hiss.
        let floor = |seconds: f32| sine(100.0, -60.0, seconds);
        let (input, bursts) = two_bursts(-32.0, -32.0, &floor);
        // The middle of the pause: far enough from both bursts that the
        // envelope window holds nothing but the floor.
        let pause = bursts[0].end + samples(0.25)..bursts[1].start - samples(0.25);
        let before = rms_db(&input[pause.clone()]);
        let mut reply = audio(input);
        process(&mut reply, DEFAULT);
        let after = rms_db(&reply.samples[pause]);
        assert!(
            (after - before).abs() < 0.1,
            "floor moved from {before} to {after} dBFS"
        );
    }

    #[test]
    fn levelling_off_is_a_pure_gain() {
        // With levelling at 0 the user asked for a volume knob and nothing
        // else: the 8 dB between the two bursts must survive, sample by sample.
        let (input, bursts) = two_bursts(-32.0, -40.0, &zeros);
        let mut reply = audio(input.clone());
        process(
            &mut reply,
            Loudness {
                volume_db: -18.0,
                levelling: 0.0,
            },
        );
        let gain = reply.samples[bursts[0].start + 100] / input[bursts[0].start + 100];
        assert!(gain > 1.0, "the quiet reply was not lifted: gain {gain}");
        for burst in bursts.clone() {
            for i in burst.filter(|&i| input[i].abs() > 1e-3) {
                let g = reply.samples[i] / input[i];
                assert!(
                    (g - gain).abs() <= gain * 1e-4,
                    "gain {g} at sample {i}, expected {gain}"
                );
            }
        }
        assert!(
            (rms_db(&input[bursts[0].clone()]) - rms_db(&input[bursts[1].clone()]) - 8.0).abs()
                < 0.01
        );
    }

    #[test]
    fn a_thirty_second_reply_is_processed_within_budget() {
        // The stage runs between synthesis and playback, so its time is heard
        // as delay before the reply starts.
        let mut input = Vec::new();
        while input.len() < samples(30.0) {
            input.extend(sine(220.0, -30.0, 2.0));
            input.extend(zeros(0.5));
        }
        input.truncate(samples(30.0));
        let budget = if cfg!(debug_assertions) {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(5)
        };
        let mut reply = audio(input);
        let started = Instant::now();
        process(&mut reply, DEFAULT);
        let took = started.elapsed();
        eprintln!("loudness: 30 s at {RATE} Hz took {took:?}");
        assert!(
            took < budget,
            "30 s of audio took {took:?}, budget {budget:?}"
        );
    }

    #[test]
    fn empty_and_silent_audio_are_unchanged() {
        for input in [Vec::new(), zeros(1.0)] {
            let mut reply = audio(input.clone());
            process(&mut reply, DEFAULT);
            assert_eq!(reply, audio(input));
        }
        let mut zero_rate = SpeechAudio {
            samples: sine(440.0, -32.0, 0.1),
            sample_rate: 0,
        };
        let before = zero_rate.clone();
        process(&mut zero_rate, DEFAULT);
        assert_eq!(
            zero_rate, before,
            "a zero rate must not reach SamplesBuffer, which panics on it"
        );
    }
}
