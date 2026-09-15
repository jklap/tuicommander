//! The state color vocabulary. Five colors, the same set three independent
//! prior-art projects (the Codex Micro's LED states, `stream-deck-claude-code`,
//! `devops-streamdeck`) converged on independently, plus the two extras this
//! design adds (`StaleReady`, `Empty`).

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn as_tiny_skia(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.0, self.1, self.2, 255)
    }
}

/// The state a key face is rendering. Deliberately flat and small — one
/// background color, one text color, per state. `StaleReady` is
/// `CompletedUnread` past the 10-minute idle escalation (`ready (12m)`);
/// `Empty` is an unoccupied slot.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum FaceState {
    Idle,
    Working,
    NeedsInput,
    CompletedUnread,
    StaleReady,
    Error,
    Empty,
}

impl FaceState {
    pub const fn background(self) -> Rgb {
        match self {
            FaceState::Idle => Rgb(0x2B, 0x2F, 0x36),
            FaceState::Working => Rgb(0x1E, 0x6F, 0xD9),
            FaceState::NeedsInput => Rgb(0xE8, 0x86, 0x3C),
            FaceState::CompletedUnread => Rgb(0x2E, 0x9E, 0x5B),
            // 50% desaturated CompletedUnread green, computed once by hand
            // rather than at render time so this stays a plain constant.
            FaceState::StaleReady => Rgb(0x47, 0x7A, 0x5D),
            FaceState::Error => Rgb(0xC4, 0x3B, 0x3B),
            FaceState::Empty => Rgb(0x00, 0x00, 0x00),
        }
    }

    /// Text is white at ~92% everywhere except on the peach NeedsInput
    /// background, where near-black reads far better. Contrast-checked once
    /// by eye against the rendered samples; not recomputed per frame.
    pub const fn text_color(self) -> Rgb {
        match self {
            FaceState::NeedsInput => Rgb(0x1A, 0x12, 0x00),
            _ => Rgb(0xEB, 0xEB, 0xEB),
        }
    }

    /// Only NeedsInput pulses. Motion in peripheral vision is a scarce
    /// signal reserved for "you, specifically" — see AGENTS-level design
    /// notes on why an amber "thinking" pulse (what most prior art does)
    /// makes "needs you" stop being noticeable.
    pub const fn pulses(self) -> bool {
        matches!(self, FaceState::NeedsInput)
    }

    /// Background color for one of the 4 discrete pulse phase buckets
    /// (`render::Glyph::Pulse`). A dim/bright alternation every 2 buckets —
    /// at the coordinator's 250ms-per-bucket cadence that's a clearly
    /// visible ~2Hz pulse, not a subtle brightness wobble. Only meaningful
    /// (and only ever called) for a state where `pulses()` is true.
    pub fn pulsing_background(self, phase: u8) -> Rgb {
        let base = self.background();
        let factor = if phase % 4 < 2 { 0.68 } else { 1.0 };
        Rgb(
            (base.0 as f32 * factor).round() as u8,
            (base.1 as f32 * factor).round() as u8,
            (base.2 as f32 * factor).round() as u8,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_has_a_distinct_background() {
        let states = [
            FaceState::Idle,
            FaceState::Working,
            FaceState::NeedsInput,
            FaceState::CompletedUnread,
            FaceState::StaleReady,
            FaceState::Error,
            FaceState::Empty,
        ];
        let mut seen = std::collections::HashSet::new();
        for s in states {
            assert!(
                seen.insert(s.background()),
                "{s:?} collides with another state's background"
            );
        }
    }

    #[test]
    fn only_needs_input_pulses() {
        assert!(FaceState::NeedsInput.pulses());
        assert!(!FaceState::Working.pulses());
        assert!(!FaceState::Idle.pulses());
        assert!(!FaceState::CompletedUnread.pulses());
        assert!(!FaceState::StaleReady.pulses());
        assert!(!FaceState::Error.pulses());
        assert!(!FaceState::Empty.pulses());
    }
}
