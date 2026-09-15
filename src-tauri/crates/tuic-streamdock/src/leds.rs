//! Ambient LED summary for the 24-LED ring around the panel.
//!
//! Per the design plan: the ring is an ambient summary, not a per-key
//! signal — solid green when nothing wants you, peach when at least one
//! session needs input, red on error. One write per *state change*, never
//! per tick (`Coordinator::ambient_led_update` dedups against the last
//! value it returned); gated entirely by the caller (`run_one_device`) on
//! the negotiated `FeatureSet::rgb`, never on the model table — see
//! `device::model`'s doc comment on why RGB support can't be inferred from
//! the model alone.

use crate::policy::{Priority, priority_of};
use crate::port::SessionSnapshot;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AmbientLed {
    Green,
    Peach,
    Red,
}

impl AmbientLed {
    /// Matches the render palette's `NeedsInput`/`Error` backgrounds
    /// exactly (`render::palette::FaceState`), so the ring and the panel
    /// never disagree about what "needs input" or "error" look like.
    /// Green is `CompletedUnread`'s background — "all good" reuses the
    /// same color the panel already uses for "done, nothing pending."
    pub const fn rgb(self) -> [u8; 3] {
        match self {
            AmbientLed::Green => [0x2E, 0x9E, 0x5B],
            AmbientLed::Peach => [0xE8, 0x86, 0x3C],
            AmbientLed::Red => [0xC4, 0x3B, 0x3B],
        }
    }

    pub fn colors(self, led_count: u8) -> Vec<[u8; 3]> {
        vec![self.rgb(); led_count as usize]
    }
}

/// Aggregate every live session's priority into one ambient color: any
/// `Error` session wins outright (red); otherwise any `NeedsInput` session
/// wins (peach); otherwise green. A single pass, no allocation beyond the
/// caller's own snapshot list.
pub fn ambient_for(sessions: &[SessionSnapshot], now_ms: u64) -> AmbientLed {
    let mut saw_needs_input = false;
    for session in sessions {
        match priority_of(session, now_ms) {
            Priority::Error => return AmbientLed::Red,
            Priority::NeedsInput => saw_needs_input = true,
            _ => {}
        }
    }
    if saw_needs_input {
        AmbientLed::Peach
    } else {
        AmbientLed::Green
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(now_ms: u64) -> SessionSnapshot {
        SessionSnapshot {
            session_id: "s1".into(),
            label: "tc-1".into(),
            secondary: String::new(),
            agent_state: None,
            shell_state: None,
            awaiting_input: false,
            choice_prompt_pending: false,
            choice_prompt_options: Vec::new(),
            rate_limited: false,
            suggested_actions_pending: false,
            last_activity_ms: now_ms,
        }
    }

    #[test]
    fn no_sessions_is_green() {
        assert_eq!(ambient_for(&[], 0), AmbientLed::Green);
    }

    #[test]
    fn idle_sessions_are_green() {
        let mut s = session(0);
        s.agent_state = Some("idle".into());
        assert_eq!(ambient_for(&[s], 0), AmbientLed::Green);
    }

    #[test]
    fn any_needs_input_session_turns_peach() {
        let mut idle = session(0);
        idle.agent_state = Some("idle".into());
        let mut waiting = session(0);
        waiting.awaiting_input = true;
        assert_eq!(ambient_for(&[idle, waiting], 0), AmbientLed::Peach);
    }

    #[test]
    fn error_outranks_needs_input() {
        let mut waiting = session(0);
        waiting.awaiting_input = true;
        let mut broken = session(0);
        broken.rate_limited = true;
        assert_eq!(ambient_for(&[waiting, broken], 0), AmbientLed::Red);
    }

    #[test]
    fn colors_fills_the_led_count() {
        assert_eq!(AmbientLed::Peach.colors(24).len(), 24);
        assert!(
            AmbientLed::Peach
                .colors(24)
                .iter()
                .all(|c| *c == [0xE8, 0x86, 0x3C])
        );
    }
}
