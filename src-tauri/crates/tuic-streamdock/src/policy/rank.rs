//! Session priority, derived from a `SessionSnapshot` (the crate's flat DTO
//! — see `port.rs` — not `AppState::SessionState` directly, so this module
//! has no dependency on the main crate at all).
//!
//! `NeedsInput` always outranks everything else, but critically an
//! `awaiting` session never displaces *another* `awaiting` session (see
//! `SlotPlanner::plan` in `plan.rs`) — churn between two sessions that both
//! want you is worse than one of them sitting behind an overflow badge for
//! a few seconds.

use crate::port::SessionSnapshot;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    Stale = 0,
    Idle = 1,
    CompletedUnread = 2,
    Working = 3,
    Error = 4,
    NeedsInput = 5,
}

/// A session is "stale" once idle for longer than this — the `ready (12m)`
/// escalation in `stream-deck-claude-code`'s own design, which independently
/// arrived at "decay idle-done into a lower-urgency state after a while."
pub const STALE_AFTER_MS: u64 = 10 * 60 * 1000;

pub fn priority_of(session: &SessionSnapshot, now_ms: u64) -> Priority {
    if session.awaiting_input || session.choice_prompt_pending {
        return Priority::NeedsInput;
    }
    if session.rate_limited {
        return Priority::Error;
    }
    match session.agent_state.as_deref() {
        Some("working") => return Priority::Working,
        Some("completed") => return Priority::CompletedUnread,
        Some("idle") | None => {}
        _ => return Priority::Working, // "starting" and anything unrecognized: treat as active, not stale.
    }
    if session.suggested_actions_pending {
        return Priority::CompletedUnread;
    }
    // Plain shells: shell_state is the only signal.
    if session.agent_state.is_none() && session.shell_state.as_deref() == Some("busy") {
        return Priority::Working;
    }
    let idle_for = now_ms.saturating_sub(session.last_activity_ms);
    if idle_for > STALE_AFTER_MS {
        Priority::Stale
    } else {
        Priority::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(now_ms: u64) -> SessionSnapshot {
        SessionSnapshot {
            session_id: "s1".into(),
            label: "tc-1".into(),
            secondary: String::new(),
            agent_state: None,
            shell_state: None,
            awaiting_input: false,
            choice_prompt_pending: false,
            rate_limited: false,
            suggested_actions_pending: false,
            last_activity_ms: now_ms,
        }
    }

    #[test]
    fn awaiting_input_always_wins() {
        let mut s = base(1000);
        s.awaiting_input = true;
        s.rate_limited = true; // even over an error condition
        assert_eq!(priority_of(&s, 1000), Priority::NeedsInput);
    }

    #[test]
    fn choice_prompt_counts_as_needs_input() {
        let mut s = base(1000);
        s.choice_prompt_pending = true;
        assert_eq!(priority_of(&s, 1000), Priority::NeedsInput);
    }

    #[test]
    fn rate_limited_is_error_priority() {
        let mut s = base(1000);
        s.rate_limited = true;
        assert_eq!(priority_of(&s, 1000), Priority::Error);
    }

    #[test]
    fn working_agent_state_outranks_idle() {
        let mut s = base(1000);
        s.agent_state = Some("working".into());
        assert_eq!(priority_of(&s, 1000), Priority::Working);
    }

    #[test]
    fn idle_session_decays_to_stale_after_ten_minutes() {
        let mut s = base(0);
        s.agent_state = Some("idle".into());
        assert_eq!(
            priority_of(&s, 5 * 60 * 1000),
            Priority::Idle,
            "5 minutes: still just idle"
        );
        assert_eq!(
            priority_of(&s, 11 * 60 * 1000),
            Priority::Stale,
            "11 minutes: decays to stale"
        );
    }

    #[test]
    fn plain_shell_uses_shell_state_not_agent_state() {
        let mut s = base(1000);
        s.agent_state = None;
        s.shell_state = Some("busy".into());
        assert_eq!(priority_of(&s, 1000), Priority::Working);
    }

    #[test]
    fn priority_ordering_is_total_and_matches_intent() {
        assert!(Priority::NeedsInput > Priority::Error);
        assert!(Priority::Error > Priority::Working);
        assert!(Priority::Working > Priority::CompletedUnread);
        assert!(Priority::CompletedUnread > Priority::Idle);
        assert!(Priority::Idle > Priority::Stale);
    }
}
