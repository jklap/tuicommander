//! Notification sound playback via the system audio output (rodio).
//!
//! Generates tones natively to bypass WebKit AudioContext restrictions.
//! Each note uses a custom Source with selectable waveform (sine/triangle)
//! and an integrated ADSR amplitude envelope for smooth, click-free playback.

use rodio::cpal::traits::HostTrait;
use rodio::{Decoder, DeviceSinkBuilder, DeviceTrait, MixerDeviceSink, Player, Source};
use serde::Serialize;
use std::fs::File;
use std::io::BufReader;
use std::num::NonZero;
use std::time::Duration;

const SAMPLE_RATE: u32 = 48_000;

/// Notification sound types — mirrors the TypeScript `NotificationSound` union.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum NotificationSound {
    Question,
    Completion,
    Error,
    Warning,
    Info,
    Attention,
}

/// Waveform shape for tone generation.
#[derive(Debug, Clone, Copy)]
enum Waveform {
    Sine,
    Triangle,
}

/// A single note in a sound sequence.
struct Note {
    frequency: f32,
    duration: Duration,
    waveform: Waveform,
}

/// A sequence of notes with a gap between them.
struct SoundSequence {
    notes: Vec<Note>,
    gap: Duration,
    /// Per-sequence attenuation applied on top of the user's volume.
    gain: f32,
}

/// Get the sound definition for a notification type.
/// Waveform choices match the original Web Audio implementation:
/// sine for melodic sounds, triangle for warmer/softer tones.
fn sound_sequence(sound: NotificationSound) -> SoundSequence {
    match sound {
        // Gentle two-note ascending chime: C5 -> E5
        NotificationSound::Question => SoundSequence {
            notes: vec![
                Note {
                    frequency: 523.0,
                    duration: Duration::from_millis(120),
                    waveform: Waveform::Sine,
                },
                Note {
                    frequency: 659.0,
                    duration: Duration::from_millis(120),
                    waveform: Waveform::Sine,
                },
            ],
            gain: 1.0,
            gap: Duration::from_millis(30),
        },
        // Satisfying major triad arpeggio: C5 -> E5 -> G5
        NotificationSound::Completion => SoundSequence {
            notes: vec![
                Note {
                    frequency: 523.0,
                    duration: Duration::from_millis(100),
                    waveform: Waveform::Sine,
                },
                Note {
                    frequency: 659.0,
                    duration: Duration::from_millis(100),
                    waveform: Waveform::Sine,
                },
                Note {
                    frequency: 784.0,
                    duration: Duration::from_millis(100),
                    waveform: Waveform::Sine,
                },
            ],
            gain: 1.0,
            gap: Duration::from_millis(30),
        },
        // Low descending minor interval: E4 -> C4 (triangle = warmer)
        NotificationSound::Error => SoundSequence {
            notes: vec![
                Note {
                    frequency: 330.0,
                    duration: Duration::from_millis(150),
                    waveform: Waveform::Triangle,
                },
                Note {
                    frequency: 262.0,
                    duration: Duration::from_millis(150),
                    waveform: Waveform::Triangle,
                },
            ],
            gain: 1.0,
            gap: Duration::from_millis(40),
        },
        // Quick double-tap: A4 x 2 (triangle = softer)
        NotificationSound::Warning => SoundSequence {
            notes: vec![
                Note {
                    frequency: 440.0,
                    duration: Duration::from_millis(80),
                    waveform: Waveform::Triangle,
                },
                Note {
                    frequency: 440.0,
                    duration: Duration::from_millis(80),
                    waveform: Waveform::Triangle,
                },
            ],
            gain: 1.0,
            gap: Duration::from_millis(60),
        },
        // Soft single pluck: G5
        NotificationSound::Info => SoundSequence {
            notes: vec![Note {
                frequency: 784.0,
                duration: Duration::from_millis(80),
                waveform: Waveform::Sine,
            }],
            gain: 1.0,
            gap: Duration::ZERO,
        },
        // Callback motif: two quick G4 knocks followed by a longer E5 call.
        // Triangle waves remain audible across a room without the harsh odd
        // harmonics of the old square-wave buzzer. The repeated opening makes
        // the pattern unmistakable, while the longer rise supplies urgency.
        NotificationSound::Attention => SoundSequence {
            notes: vec![
                Note {
                    frequency: 392.0,
                    duration: Duration::from_millis(75),
                    waveform: Waveform::Triangle,
                },
                Note {
                    frequency: 392.0,
                    duration: Duration::from_millis(75),
                    waveform: Waveform::Triangle,
                },
                Note {
                    frequency: 659.0,
                    duration: Duration::from_millis(140),
                    waveform: Waveform::Triangle,
                },
            ],
            gain: 0.8,
            gap: Duration::from_millis(50),
        },
    }
}

// ---------------------------------------------------------------------------
// User-selectable sound source (preset borrow or custom audio file)
// ---------------------------------------------------------------------------

fn default_preset_name() -> String {
    "default".to_string()
}

/// A user's chosen sound source for one notification type. Passed fresh on
/// every `play_notification_sound` call from the frontend's persisted config
/// (`config::NotificationSoundChoices`) — this module never reads config
/// itself, exactly like `volume`/`device` already arrive as plain call
/// arguments rather than being read back off disk here.
///
/// `preset` is deliberately a plain string, not an enum: the set of valid
/// values is owned by the frontend (`src/notifications.ts`'s `SoundPreset`
/// union) and interpreted here by `resolve_sequence`, which falls back gracefully
/// for anything it doesn't recognize. Keeping this a string means a preset
/// this module doesn't know about (an older Rust build, a hand-edited
/// config) degrades to that sound's own default tone instead of failing to
/// deserialize the whole command call.
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct SoundChoice {
    #[serde(default = "default_preset_name")]
    pub(crate) preset: String,
    #[serde(default)]
    pub(crate) custom_path: Option<String>,
}

impl Default for SoundChoice {
    fn default() -> Self {
        Self {
            preset: default_preset_name(),
            custom_path: None,
        }
    }
}

/// The preset name each `NotificationSound` is known by when borrowed as
/// another sound's preset. Exhaustive on purpose: adding a `NotificationSound`
/// variant without giving it a preset name here fails to *compile*, unlike a
/// plain `match preset { "question" => ... }` which would just silently never
/// offer the new sound as a borrowable preset.
fn preset_name_for(sound: NotificationSound) -> &'static str {
    match sound {
        NotificationSound::Question => "question",
        NotificationSound::Completion => "completion",
        NotificationSound::Error => "error",
        NotificationSound::Warning => "warning",
        NotificationSound::Info => "info",
        NotificationSound::Attention => "attention",
    }
}

/// Resolve a chosen preset name to the tone sequence to play. Presets other
/// than "default" simply borrow another notification type's own built-in
/// motif — there is no separate library of generic tones to keep in sync.
/// An unrecognized name (including "custom", which the caller handles before
/// ever reaching this function) falls back to the sound's own default.
fn resolve_sequence(sound: NotificationSound, preset: &str) -> SoundSequence {
    for candidate in [
        NotificationSound::Question,
        NotificationSound::Completion,
        NotificationSound::Error,
        NotificationSound::Warning,
        NotificationSound::Info,
        NotificationSound::Attention,
    ] {
        if preset_name_for(candidate) == preset {
            return sound_sequence(candidate);
        }
    }
    sound_sequence(sound)
}

/// Open and probe-decode a user-supplied sound file.
fn open_custom_sound(path: &str) -> Result<Decoder<BufReader<File>>, String> {
    let file = File::open(path).map_err(|e| format!("Could not open \"{path}\": {e}"))?;
    Decoder::new(BufReader::new(file)).map_err(|e| format!("Could not decode \"{path}\": {e}"))
}

/// What actually gets played: a decoded custom file, or a procedural tone
/// sequence. A separate type (rather than inlining this decision inside
/// `play()`) so the fallback logic is unit-testable without touching real
/// audio hardware or a background thread.
enum PlaybackSource {
    Custom(Decoder<BufReader<File>>),
    Sequence(SoundSequence),
}

/// Decide what to play for `sound` given the user's `choice`. A "custom"
/// choice whose file can't be opened or decoded (moved, deleted, corrupted,
/// unsupported format) falls back to the sound's own default tone — same as
/// "custom" selected with no file configured yet, or any unrecognized preset
/// name — rather than producing no sound at all. The failure is logged, not
/// propagated: nothing downstream of this function can distinguish "played
/// the default tone because the user chose it" from "fell back to it," and
/// that's intentional — playing *something* always beats erroring out.
fn resolve_playback_source(sound: NotificationSound, choice: &SoundChoice) -> PlaybackSource {
    if choice.preset == "custom"
        && let Some(path) = choice.custom_path.as_deref()
    {
        match open_custom_sound(path) {
            Ok(decoder) => return PlaybackSource::Custom(decoder),
            Err(e) => {
                tracing::warn!(
                    source = "notification_sound",
                    path,
                    "Failed to open/decode custom sound, falling back to default tone: {e}"
                );
            }
        }
    }
    PlaybackSource::Sequence(resolve_sequence(sound, &choice.preset))
}

// ---------------------------------------------------------------------------
// Custom rodio Source: waveform + integrated ADSR envelope
// ---------------------------------------------------------------------------

/// A tone source with selectable waveform and linear attack/release envelope.
///
/// The envelope ramps amplitude from 0 to `volume` over `attack_samples`,
/// holds at `volume` for the sustain region, then ramps down to 0 over
/// `release_samples`. Total duration = attack + sustain + release.
struct EnvelopedTone {
    sample_rate: u32,
    sample_index: u64,
    volume: f32,
    waveform: Waveform,
    attack_samples: u64,
    sustain_end: u64,
    total_samples: u64,
    /// Precomputed: sample_rate / frequency
    period: f32,
}

impl EnvelopedTone {
    fn new(frequency: f32, duration: Duration, volume: f32, waveform: Waveform) -> Self {
        let attack = Duration::from_millis(10);
        let release = Duration::from_millis(30);

        let attack_samples = (attack.as_secs_f64() * SAMPLE_RATE as f64) as u64;
        let release_samples = (release.as_secs_f64() * SAMPLE_RATE as f64) as u64;
        let duration_samples = (duration.as_secs_f64() * SAMPLE_RATE as f64) as u64;

        // Total = note duration + release tail (release extends beyond the note)
        let total_samples = duration_samples + release_samples;
        let sustain_end = duration_samples;

        Self {
            sample_rate: SAMPLE_RATE,
            sample_index: 0,
            volume,
            waveform,
            attack_samples,
            sustain_end,
            total_samples,
            period: SAMPLE_RATE as f32 / frequency,
        }
    }

    /// Compute the amplitude envelope at the current sample position.
    fn envelope(&self) -> f32 {
        let i = self.sample_index;
        if i < self.attack_samples {
            // Linear ramp up: 0 -> 1
            i as f32 / self.attack_samples as f32
        } else if i < self.sustain_end {
            // Full amplitude
            1.0
        } else {
            // Linear ramp down: 1 -> 0
            let release_len = self.total_samples - self.sustain_end;
            if release_len == 0 {
                return 0.0;
            }
            let release_pos = i - self.sustain_end;
            1.0 - (release_pos as f32 / release_len as f32)
        }
    }

    /// Generate one sample of the selected waveform at the given phase.
    fn waveform_sample(&self, phase: f32) -> f32 {
        match self.waveform {
            Waveform::Sine => (std::f32::consts::TAU * phase).sin(),
            // Triangle: rises from -1 to +1 in first half, falls back in second half
            Waveform::Triangle => 4.0 * (phase - (phase + 0.5).floor()).abs() - 1.0,
        }
    }
}

impl Iterator for EnvelopedTone {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.sample_index >= self.total_samples {
            return None;
        }

        let phase = (self.sample_index as f32 / self.period).fract();
        let sample = self.waveform_sample(phase);
        let amplitude = self.envelope() * self.volume;

        self.sample_index += 1;
        Some(sample * amplitude)
    }
}

impl Source for EnvelopedTone {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> NonZero<u16> {
        NonZero::new(1).expect("one channel is non-zero")
    }

    fn sample_rate(&self) -> NonZero<u32> {
        NonZero::new(self.sample_rate).expect("sample rate is non-zero")
    }

    fn total_duration(&self) -> Option<Duration> {
        let secs = self.total_samples as f64 / self.sample_rate as f64;
        Some(Duration::from_secs_f64(secs))
    }
}

// ---------------------------------------------------------------------------
// Output device enumeration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AudioOutputDevice {
    pub name: String,
    pub is_default: bool,
}

pub(crate) fn list_output_devices() -> Vec<AudioOutputDevice> {
    let host = rodio::cpal::default_host();
    let default_name: Option<String> = host
        .default_output_device()
        .and_then(|d| d.description().ok().map(|desc| desc.name().to_string()));

    host.output_devices()
        .map(|devices| {
            devices
                .filter_map(|d| {
                    let name = d.description().ok()?.name().to_string();
                    Some(AudioOutputDevice {
                        is_default: default_name.as_deref() == Some(name.as_str()),
                        name,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Sequence -> playback step expansion (pure, unit-testable independent of
// real audio I/O — the actual note/gap interleaving logic lives here so it
// doesn't have to be exercised through a live rodio player to be tested).
// ---------------------------------------------------------------------------

/// One playback step: either a tone or a timed gap of silence.
enum PlaybackStep {
    Tone {
        frequency: f32,
        duration: Duration,
        waveform: Waveform,
    },
    Gap(Duration),
}

/// Expand a sound sequence into the ordered list of playback steps — a gap
/// follows every note except the last, and a zero-length gap is skipped
/// entirely rather than emitted as a no-op step.
fn playback_steps(seq: &SoundSequence) -> Vec<PlaybackStep> {
    let mut steps = Vec::with_capacity(seq.notes.len() * 2);
    for (i, note) in seq.notes.iter().enumerate() {
        steps.push(PlaybackStep::Tone {
            frequency: note.frequency,
            duration: note.duration,
            waveform: note.waveform,
        });
        if i < seq.notes.len() - 1 && !seq.gap.is_zero() {
            steps.push(PlaybackStep::Gap(seq.gap));
        }
    }
    steps
}

// ---------------------------------------------------------------------------
// Playback
// ---------------------------------------------------------------------------

/// Resolve an output device by name, falling back to default.
fn resolve_output_stream(device_name: Option<&str>) -> Option<MixerDeviceSink> {
    if let Some(name) = device_name {
        let host = rodio::cpal::default_host();
        let device = host.output_devices().ok()?.find(|d| {
            d.description()
                .map(|description| description.name() == name)
                .unwrap_or(false)
        });
        if let Some(dev) = device {
            match DeviceSinkBuilder::from_device(dev).and_then(|builder| builder.open_stream()) {
                Ok(stream) => return Some(stream),
                Err(e) => {
                    tracing::warn!(
                        source = "notification_sound",
                        device = name,
                        "Failed to open selected device, falling back to default: {e}"
                    );
                }
            }
        } else {
            tracing::warn!(
                source = "notification_sound",
                device = name,
                "Configured device not found, falling back to default"
            );
        }
    }
    DeviceSinkBuilder::open_default_sink().ok()
}

/// Play a notification sound on a background thread.
///
/// Volume is 0.0-1.0. `device_name` selects a specific output device;
/// `None` uses the system default. `choice` selects the sound source: the
/// built-in default tone, another sound's tone borrowed as a preset, or a
/// user-supplied audio file — see `resolve_playback_source` for the
/// fallback rules. Returns immediately; audio plays asynchronously on a
/// short-lived thread, including resolving the custom-file decoder, so a
/// slow disk/decode never blocks the caller (e.g. the IPC dispatch thread).
pub(crate) fn play(
    sound: NotificationSound,
    volume: f32,
    device_name: Option<String>,
    choice: SoundChoice,
) {
    let volume = volume.clamp(0.0, 1.0);
    std::thread::spawn(move || {
        let Some(stream) = resolve_output_stream(device_name.as_deref()) else {
            tracing::warn!(source = "notification_sound", "Failed to open audio output");
            return;
        };
        let player = Player::connect_new(stream.mixer());

        match resolve_playback_source(sound, &choice) {
            PlaybackSource::Custom(decoder) => {
                player.append(decoder.amplify(volume));
            }
            PlaybackSource::Sequence(seq) => {
                let volume = volume * seq.gain;
                for step in playback_steps(&seq) {
                    match step {
                        PlaybackStep::Tone {
                            frequency,
                            duration,
                            waveform,
                        } => {
                            player
                                .append(EnvelopedTone::new(frequency, duration, volume, waveform));
                        }
                        PlaybackStep::Gap(gap) => {
                            player.append(
                                rodio::source::Zero::new(
                                    NonZero::new(1).expect("one channel is non-zero"),
                                    NonZero::new(SAMPLE_RATE).expect("sample rate is non-zero"),
                                )
                                .take_duration(gap),
                            );
                        }
                    }
                }
            }
        }

        player.sleep_until_end();
    });
}

/// Tauri command: play a notification sound. Fire-and-forget, like every
/// other failure mode here (missing output device, mid-playback error, a
/// broken custom file) — see `resolve_playback_source`'s doc comment for why
/// there is nothing meaningful left to return.
#[tauri::command]
pub(crate) fn play_notification_sound(
    sound: NotificationSound,
    volume: f32,
    device: Option<String>,
    choice: SoundChoice,
) {
    play(sound, volume, device, choice);
}

/// Tauri command: list available audio output devices.
#[tauri::command]
pub(crate) fn list_audio_output_devices() -> Vec<AudioOutputDevice> {
    list_output_devices()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_sound_types_produce_sequences() {
        let types = [
            NotificationSound::Question,
            NotificationSound::Completion,
            NotificationSound::Error,
            NotificationSound::Warning,
            NotificationSound::Info,
            NotificationSound::Attention,
        ];
        for sound in types {
            let seq = sound_sequence(sound);
            assert!(
                !seq.notes.is_empty(),
                "{sound:?} should have at least one note"
            );
            for note in &seq.notes {
                assert!(
                    note.frequency > 0.0,
                    "{sound:?} note frequency must be positive"
                );
                assert!(
                    !note.duration.is_zero(),
                    "{sound:?} note duration must be non-zero"
                );
            }
        }
    }

    #[test]
    fn envelope_attack_ramps_up() {
        let tone = EnvelopedTone::new(440.0, Duration::from_millis(100), 1.0, Waveform::Sine);
        // At sample 0, envelope should be 0
        assert!(tone.envelope().abs() < f32::EPSILON);
        // At half of attack (attack = 10ms = 480 samples at 48kHz)
        let mut tone = tone;
        tone.sample_index = 240;
        let env = tone.envelope();
        assert!(
            (env - 0.5).abs() < 0.01,
            "Expected ~0.5 at half-attack, got {env}"
        );
    }

    #[test]
    fn envelope_sustain_is_full() {
        let mut tone = EnvelopedTone::new(440.0, Duration::from_millis(100), 1.0, Waveform::Sine);
        // After attack ends (480 samples), envelope should be 1.0
        tone.sample_index = 500;
        assert!((tone.envelope() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn envelope_release_ramps_down() {
        let tone = EnvelopedTone::new(440.0, Duration::from_millis(100), 1.0, Waveform::Sine);
        let sustain_end = tone.sustain_end;
        let total = tone.total_samples;
        let release_mid = sustain_end + (total - sustain_end) / 2;

        let mut tone = tone;
        tone.sample_index = release_mid;
        let env = tone.envelope();
        assert!(
            (env - 0.5).abs() < 0.02,
            "Expected ~0.5 at mid-release, got {env}"
        );

        // At the very end, should be ~0
        tone.sample_index = total - 1;
        let env = tone.envelope();
        assert!(env < 0.05, "Expected ~0 at end of release, got {env}");
    }

    #[test]
    fn source_produces_correct_sample_count() {
        let tone = EnvelopedTone::new(440.0, Duration::from_millis(100), 0.5, Waveform::Sine);
        let expected = tone.total_samples as usize;
        let count = tone.count(); // consumes iterator
        assert_eq!(count, expected);
    }

    #[test]
    fn triangle_waveform_range() {
        let tone = EnvelopedTone::new(440.0, Duration::from_millis(50), 1.0, Waveform::Triangle);
        // Collect all samples and verify they're in [-1, 1] (before volume scaling)
        for sample in tone {
            assert!(
                (-1.0..=1.0).contains(&sample),
                "Triangle sample out of range: {sample}"
            );
        }
    }

    #[test]
    fn sine_waveform_range() {
        let tone = EnvelopedTone::new(440.0, Duration::from_millis(50), 1.0, Waveform::Sine);
        for sample in tone {
            assert!(
                (-1.0..=1.0).contains(&sample),
                "Sine sample out of range: {sample}"
            );
        }
    }

    #[test]
    fn error_and_warning_use_triangle() {
        let error_seq = sound_sequence(NotificationSound::Error);
        let warning_seq = sound_sequence(NotificationSound::Warning);
        for note in &error_seq.notes {
            assert!(
                matches!(note.waveform, Waveform::Triangle),
                "Error notes should use triangle"
            );
        }
        for note in &warning_seq.notes {
            assert!(
                matches!(note.waveform, Waveform::Triangle),
                "Warning notes should use triangle"
            );
        }
    }

    /// The attention call is a deliberate two-knock-and-rise motif.
    #[test]
    fn attention_uses_the_approved_double_knock_motif() {
        let seq = sound_sequence(NotificationSound::Attention);
        assert_eq!(seq.notes.len(), 3);
        assert_eq!(seq.gap, Duration::from_millis(50));
        assert_eq!(
            seq.notes
                .iter()
                .map(|note| note.frequency as u32)
                .collect::<Vec<_>>(),
            vec![392, 392, 659]
        );
        assert_eq!(
            seq.notes
                .iter()
                .map(|note| note.duration)
                .collect::<Vec<_>>(),
            vec![
                Duration::from_millis(75),
                Duration::from_millis(75),
                Duration::from_millis(140),
            ]
        );
        for note in &seq.notes {
            assert!(matches!(note.waveform, Waveform::Triangle));
        }
    }

    /// The call is prominent but still below the ordinary chimes' full gain.
    #[test]
    fn attention_attenuates_itself_below_the_chimes() {
        let attention = sound_sequence(NotificationSound::Attention);
        assert!(
            attention.gain < 1.0,
            "attention call must stay below full gain"
        );
        for other in [
            NotificationSound::Question,
            NotificationSound::Completion,
            NotificationSound::Error,
            NotificationSound::Warning,
            NotificationSound::Info,
        ] {
            assert!(
                (sound_sequence(other).gain - 1.0).abs() < f32::EPSILON,
                "{other:?} plays at the user's configured volume"
            );
        }
    }

    #[test]
    fn playback_steps_single_note_has_no_gap() {
        let seq = sound_sequence(NotificationSound::Info);
        let steps = playback_steps(&seq);
        assert_eq!(steps.len(), 1);
        assert!(matches!(steps[0], PlaybackStep::Tone { .. }));
    }

    #[test]
    fn playback_steps_interleaves_gaps_between_notes_but_not_after_the_last() {
        let seq = sound_sequence(NotificationSound::Question); // 2 notes, non-zero gap
        let steps = playback_steps(&seq);
        assert_eq!(steps.len(), 3, "tone, gap, tone — no trailing gap");
        assert!(matches!(steps[0], PlaybackStep::Tone { .. }));
        assert!(matches!(steps[1], PlaybackStep::Gap(gap) if gap == seq.gap));
        assert!(matches!(steps[2], PlaybackStep::Tone { .. }));
    }

    #[test]
    fn playback_steps_omits_gap_step_entirely_when_sequence_gap_is_zero() {
        let seq = SoundSequence {
            notes: vec![
                Note {
                    frequency: 440.0,
                    duration: Duration::from_millis(10),
                    waveform: Waveform::Sine,
                },
                Note {
                    frequency: 550.0,
                    duration: Duration::from_millis(10),
                    waveform: Waveform::Sine,
                },
            ],
            gap: Duration::ZERO,
            gain: 1.0,
        };
        let steps = playback_steps(&seq);
        // Zero-length gaps are skipped outright, not emitted as no-op steps.
        assert_eq!(steps.len(), 2);
        assert!(steps.iter().all(|s| matches!(s, PlaybackStep::Tone { .. })));
    }

    #[test]
    fn playback_steps_matches_note_count_for_every_sound() {
        for sound in [
            NotificationSound::Question,
            NotificationSound::Completion,
            NotificationSound::Error,
            NotificationSound::Warning,
            NotificationSound::Info,
            NotificationSound::Attention,
        ] {
            let seq = sound_sequence(sound);
            let tone_count = playback_steps(&seq)
                .iter()
                .filter(|s| matches!(s, PlaybackStep::Tone { .. }))
                .count();
            assert_eq!(
                tone_count,
                seq.notes.len(),
                "{sound:?} should emit exactly one Tone step per note"
            );
        }
    }

    #[test]
    fn resolve_output_stream_falls_back_to_default_for_an_unknown_device_name() {
        // Whether or not this environment has real audio hardware, an unresolvable
        // device name must fall back to exactly the same outcome as no device name
        // at all — this is the "configured device not found" branch.
        let default_present = resolve_output_stream(None).is_some();
        let unknown_present =
            resolve_output_stream(Some("definitely-not-a-real-device-xyz-123")).is_some();
        assert_eq!(
            default_present, unknown_present,
            "an unresolvable device name must fall back to the same outcome as the default device"
        );
    }

    /// Minimal valid mono 16-bit PCM WAV file, just enough for rodio's WAV
    /// decoder to accept it — content doesn't matter, only that it parses.
    fn minimal_wav_bytes() -> Vec<u8> {
        let sample_rate: u32 = 8000;
        let num_channels: u16 = 1;
        let bits_per_sample: u16 = 16;
        let samples: [i16; 8] = [0, 1000, -1000, 500, -500, 0, 0, 0];
        let data_bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let byte_rate = sample_rate * num_channels as u32 * (bits_per_sample as u32 / 8);
        let block_align = num_channels * (bits_per_sample / 8);

        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        let riff_chunk_size = 4 + (8 + 16) + (8 + data_bytes.len() as u32);
        buf.extend_from_slice(&riff_chunk_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        buf.extend_from_slice(&num_channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        buf.extend_from_slice(&block_align.to_le_bytes());
        buf.extend_from_slice(&bits_per_sample.to_le_bytes());
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&(data_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(&data_bytes);
        buf
    }

    #[test]
    fn sound_choice_default_is_the_built_in_default_preset_with_no_custom_path() {
        let choice = SoundChoice::default();
        assert_eq!(choice.preset, "default");
        assert!(choice.custom_path.is_none());
    }

    #[test]
    fn resolve_sequence_default_uses_the_sounds_own_sequence() {
        let seq = resolve_sequence(NotificationSound::Question, "default");
        let own = sound_sequence(NotificationSound::Question);
        assert_eq!(seq.notes.len(), own.notes.len());
        assert_eq!(seq.gap, own.gap);
        assert!((seq.gain - own.gain).abs() < f32::EPSILON);
    }

    #[test]
    fn resolve_sequence_named_preset_borrows_that_sounds_sequence_regardless_of_target() {
        let borrowed = resolve_sequence(NotificationSound::Warning, "attention");
        let attention_own = sound_sequence(NotificationSound::Attention);
        assert_eq!(borrowed.notes.len(), attention_own.notes.len());
        assert_eq!(borrowed.gap, attention_own.gap);
        assert!((borrowed.gain - attention_own.gain).abs() < f32::EPSILON);
        // And it must NOT match the target sound's own (different) sequence.
        let warning_own = sound_sequence(NotificationSound::Warning);
        assert_ne!(borrowed.notes.len(), warning_own.notes.len());
    }

    #[test]
    fn resolve_sequence_unknown_name_falls_back_to_the_sounds_own_default() {
        let seq = resolve_sequence(NotificationSound::Info, "totally-bogus-preset-xyz");
        let own = sound_sequence(NotificationSound::Info);
        assert_eq!(seq.notes.len(), own.notes.len());
        assert_eq!(seq.gap, own.gap);
    }

    #[test]
    fn open_custom_sound_decodes_a_valid_wav_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        std::fs::write(&path, minimal_wav_bytes()).unwrap();
        assert!(open_custom_sound(path.to_str().unwrap()).is_ok());
    }

    #[test]
    fn open_custom_sound_errors_on_a_missing_file() {
        assert!(open_custom_sound("/definitely/not/a/real/path/xyz-123.wav").is_err());
    }

    #[test]
    fn open_custom_sound_errors_on_an_unparseable_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("garbage.wav");
        std::fs::write(&path, b"not a real audio file, just text").unwrap();
        assert!(open_custom_sound(path.to_str().unwrap()).is_err());
    }

    /// Asserts a `PlaybackSource` resolved to the fallback sequence, and that
    /// it's the RIGHT sequence (`sound`'s own default) — not just "any
    /// sequence, not the custom decoder."
    fn assert_falls_back_to_default(source: PlaybackSource, sound: NotificationSound) {
        match source {
            PlaybackSource::Sequence(seq) => {
                let own = sound_sequence(sound);
                assert_eq!(seq.notes.len(), own.notes.len());
                assert_eq!(seq.gap, own.gap);
            }
            PlaybackSource::Custom(_) => {
                panic!("expected a fallback to the default tone, got a custom decoder")
            }
        }
    }

    #[test]
    fn resolve_playback_source_falls_back_to_default_when_the_custom_file_is_missing() {
        // The exact regression a code review caught: this must fall back to
        // playing something, not silently produce no sound at all.
        let choice = SoundChoice {
            preset: "custom".to_string(),
            custom_path: Some("/definitely/not/a/real/path/xyz-123.wav".to_string()),
        };
        assert_falls_back_to_default(
            resolve_playback_source(NotificationSound::Info, &choice),
            NotificationSound::Info,
        );
    }

    #[test]
    fn resolve_playback_source_falls_back_to_default_when_the_custom_file_is_unparseable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("garbage.wav");
        std::fs::write(&path, b"not a real audio file, just text").unwrap();
        let choice = SoundChoice {
            preset: "custom".to_string(),
            custom_path: Some(path.to_str().unwrap().to_string()),
        };
        assert_falls_back_to_default(
            resolve_playback_source(NotificationSound::Warning, &choice),
            NotificationSound::Warning,
        );
    }

    #[test]
    fn resolve_playback_source_uses_the_custom_decoder_for_a_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        std::fs::write(&path, minimal_wav_bytes()).unwrap();
        let choice = SoundChoice {
            preset: "custom".to_string(),
            custom_path: Some(path.to_str().unwrap().to_string()),
        };
        assert!(matches!(
            resolve_playback_source(NotificationSound::Info, &choice),
            PlaybackSource::Custom(_)
        ));
    }

    #[test]
    fn resolve_playback_source_falls_back_for_every_built_in_preset_without_touching_the_filesystem()
     {
        for preset in [
            "default",
            "question",
            "completion",
            "error",
            "warning",
            "info",
            "attention",
        ] {
            let choice = SoundChoice {
                preset: preset.to_string(),
                custom_path: None,
            };
            assert!(
                matches!(
                    resolve_playback_source(NotificationSound::Warning, &choice),
                    PlaybackSource::Sequence(_)
                ),
                "preset {preset:?} should resolve to a sequence, never a custom decoder"
            );
        }
    }

    #[test]
    fn resolve_playback_source_falls_back_when_custom_is_chosen_with_no_path_set_yet() {
        let choice = SoundChoice {
            preset: "custom".to_string(),
            custom_path: None,
        };
        assert_falls_back_to_default(
            resolve_playback_source(NotificationSound::Info, &choice),
            NotificationSound::Info,
        );
    }

    #[test]
    fn resolve_playback_source_ignores_a_stray_custom_path_when_the_preset_is_not_custom() {
        // A custom_path can linger in a config even after the user switches
        // back to a named preset (the frontend's setSoundChoice always nulls
        // it out on that path, but this proves the fallback logic itself
        // doesn't depend on that call-site discipline) — it must not even
        // attempt to open the bogus path.
        let choice = SoundChoice {
            preset: "attention".to_string(),
            custom_path: Some("/definitely/not/a/real/path/xyz-123.wav".to_string()),
        };
        match resolve_playback_source(NotificationSound::Info, &choice) {
            PlaybackSource::Sequence(seq) => {
                let attention = sound_sequence(NotificationSound::Attention);
                assert_eq!(seq.notes.len(), attention.notes.len());
            }
            PlaybackSource::Custom(_) => {
                panic!("preset != \"custom\" — must never touch custom_path")
            }
        }
    }

    #[test]
    fn resolve_output_stream_opens_a_named_device_when_one_is_available() {
        let devices = list_output_devices();
        let Some(device) = devices.first() else {
            eprintln!("skipping: no audio output devices available in this test environment");
            return;
        };
        assert!(
            resolve_output_stream(Some(&device.name)).is_some(),
            "a device name returned by list_output_devices() should always resolve"
        );
    }

    #[test]
    fn list_output_devices_has_at_most_one_default_and_no_duplicate_names() {
        let devices = list_output_devices();
        let default_count = devices.iter().filter(|d| d.is_default).count();
        assert!(
            default_count <= 1,
            "at most one device should be marked default, got {default_count}"
        );
        let mut names: Vec<&str> = devices.iter().map(|d| d.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            devices.len(),
            "device names returned by list_output_devices() should be unique"
        );
    }

    #[test]
    fn question_completion_info_use_sine() {
        let types = [
            NotificationSound::Question,
            NotificationSound::Completion,
            NotificationSound::Info,
        ];
        for sound in types {
            let seq = sound_sequence(sound);
            for note in &seq.notes {
                assert!(
                    matches!(note.waveform, Waveform::Sine),
                    "{sound:?} should use sine"
                );
            }
        }
    }
}
