//! Key roles: the split between dynamic session tiles (nouns) and curated
//! verb keys, and the default v1 layout for the M18's 15 LCD keys + 3 plain
//! buttons.
//!
//! The full per-key config schema (`StreamDockConfig`/`KeyBinding` in the
//! plan) lives in the main crate, because it goes through
//! `commit_config_change`/`ConfigSaveEffects` and has no business here.
//! This module only defines the vocabulary those config types resolve
//! *into*, and the default layout used when nothing overrides it.

use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionKind {
    JumpWaitingSession,
    InterruptFocused,
    ApproveFocused,
    RejectFocused,
    ActivityDashboard,
    DeckSleep,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyRole {
    /// Dynamic — the planner assigns whichever session lands here.
    Session,
    /// Sticky to one specific session regardless of the planner.
    PinnedSession {
        session_id: String,
    },
    Action(ActionKind),
    Overflow,
    Blank,
}

/// v1 default layout, per the plan: top two rows (slots 0..10) are dynamic
/// session tiles; the bottom row (slots 10..15) is the curated verb row;
/// the three plain buttons (slots 15..18) are global actions.
pub fn default_layout(num_lcd_slots: u8) -> Vec<KeyRole> {
    let mut roles = Vec::with_capacity(num_lcd_slots as usize + 3);
    for slot in 0..num_lcd_slots {
        roles.push(if slot < 10 {
            KeyRole::Session
        } else {
            match slot {
                10 => KeyRole::Action(ActionKind::ApproveFocused),
                11 => KeyRole::Action(ActionKind::RejectFocused),
                12 => KeyRole::Action(ActionKind::InterruptFocused),
                13 => KeyRole::Action(ActionKind::JumpWaitingSession),
                _ => KeyRole::Overflow,
            }
        });
    }
    roles.push(KeyRole::Action(ActionKind::JumpWaitingSession));
    roles.push(KeyRole::Action(ActionKind::ActivityDashboard));
    roles.push(KeyRole::Action(ActionKind::DeckSleep));
    roles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_has_ten_session_tiles() {
        let roles = default_layout(15);
        let session_tiles = roles
            .iter()
            .filter(|r| matches!(r, KeyRole::Session))
            .count();
        assert_eq!(session_tiles, 10);
    }

    #[test]
    fn default_layout_covers_every_slot_with_no_gaps() {
        let roles = default_layout(15);
        assert_eq!(roles.len(), 18, "15 LCD keys + 3 plain buttons");
    }
}
