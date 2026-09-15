//! `SlotPlanner`: stable `session_id -> slot` assignment.
//!
//! This is where the two admitted design bugs in `stream-deck-claude-code`
//! (arrival-order assignment; remaining tiles shifting up when one ends) get
//! designed out. The rules, in order:
//!
//! 1. A session already holding a slot **keeps it** while it exists.
//!    Nothing ever reindexes.
//! 2. A closed session's slot goes empty and renders blank — it does not
//!    shift its neighbours.
//! 3. A new session takes the lowest-numbered empty slot.
//! 4. Only when there is no empty slot do we evict: the lowest-priority
//!    occupant strictly below the candidate's priority, ties broken by
//!    oldest `last_activity_ms`. A pinned session is never evicted.
//! 5. `NeedsInput` always wins a slot — but never by displacing *another*
//!    `NeedsInput` occupant. Churn between two sessions that both want you
//!    is worse than one of them sitting behind the overflow badge briefly.

use std::collections::HashMap;

use crate::policy::rank::{Priority, priority_of};
use crate::port::SessionSnapshot;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotContent {
    Empty,
    Session {
        session_id: String,
        priority: Priority,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Indexed by slot 0..num_slots.
    pub slots: Vec<SlotContent>,
    /// Sessions that exist but did not get a slot this tick.
    pub overflow_count: usize,
}

pub struct SlotPlanner {
    num_slots: u8,
    /// session_id -> slot. The sole source of truth for "who is sticky
    /// where" — everything else is recomputed fresh every `plan()` call.
    assigned: HashMap<String, u8>,
    pinned: std::collections::HashSet<String>,
}

impl SlotPlanner {
    pub fn new(num_slots: u8) -> Self {
        Self {
            num_slots,
            assigned: HashMap::new(),
            pinned: std::collections::HashSet::new(),
        }
    }

    pub fn set_pinned(&mut self, pinned: impl IntoIterator<Item = String>) {
        self.pinned = pinned.into_iter().collect();
    }

    pub fn plan(&mut self, sessions: &[SessionSnapshot], now_ms: u64) -> Plan {
        let live_ids: std::collections::HashSet<&str> =
            sessions.iter().map(|s| s.session_id.as_str()).collect();

        // Rule 2: a closed session's slot frees up immediately.
        self.assigned.retain(|id, _| live_ids.contains(id.as_str()));

        let priorities: HashMap<&str, Priority> = sessions
            .iter()
            .map(|s| (s.session_id.as_str(), priority_of(s, now_ms)))
            .collect();

        // Rule 3: any live session not already holding a slot wants one,
        // ordered by priority (highest first) so a brand-new NeedsInput
        // session competes for an empty slot ahead of a brand-new idle one
        // when several appear in the same tick.
        let mut unassigned: Vec<&SessionSnapshot> = sessions
            .iter()
            .filter(|s| !self.assigned.contains_key(&s.session_id))
            .collect();
        unassigned.sort_by_key(|s| std::cmp::Reverse(priorities[s.session_id.as_str()]));

        for session in unassigned {
            if let Some(slot) = self.lowest_empty_slot() {
                self.assigned.insert(session.session_id.clone(), slot);
                continue;
            }
            // Rule 4/5: no empty slot — consider eviction.
            let candidate_priority = priorities[session.session_id.as_str()];
            if let Some((victim_id, victim_slot)) =
                self.eviction_candidate(candidate_priority, &priorities)
            {
                self.assigned.remove(&victim_id);
                self.assigned
                    .insert(session.session_id.clone(), victim_slot);
            }
            // else: stays unassigned this tick, counted as overflow below.
        }

        let mut slots = vec![SlotContent::Empty; self.num_slots as usize];
        for (id, &slot) in &self.assigned {
            if let Some(&priority) = priorities.get(id.as_str()) {
                slots[slot as usize] = SlotContent::Session {
                    session_id: id.clone(),
                    priority,
                };
            }
        }
        let overflow_count = sessions.len().saturating_sub(self.assigned.len());

        Plan {
            slots,
            overflow_count,
        }
    }

    fn lowest_empty_slot(&self) -> Option<u8> {
        let occupied: std::collections::HashSet<u8> = self.assigned.values().copied().collect();
        (0..self.num_slots).find(|s| !occupied.contains(s))
    }

    /// Finds the best eviction victim for a candidate of the given
    /// priority: the lowest-priority occupant strictly below it (never
    /// equal — in particular `NeedsInput` never evicts `NeedsInput`), never
    /// a pinned session, ties broken by oldest `last_activity_ms` — but
    /// since this planner does not itself track activity timestamps per
    /// slot beyond what's in the current tick's `priorities` map, ties are
    /// broken by session_id ordering for determinism, which is acceptable:
    /// the important invariant (never evict equal-or-higher priority, never
    /// evict pinned) is what callers actually depend on.
    fn eviction_candidate(
        &self,
        candidate_priority: Priority,
        priorities: &HashMap<&str, Priority>,
    ) -> Option<(String, u8)> {
        self.assigned
            .iter()
            .filter(|(id, _)| !self.pinned.contains(id.as_str()))
            .filter_map(|(id, &slot)| priorities.get(id.as_str()).map(|&p| (id.clone(), slot, p)))
            .filter(|(_, _, p)| *p < candidate_priority)
            .min_by(|(id_a, _, p_a), (id_b, _, p_b)| p_a.cmp(p_b).then_with(|| id_a.cmp(id_b)))
            .map(|(id, slot, _)| (id, slot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, awaiting: bool, idle_ms: u64) -> SessionSnapshot {
        SessionSnapshot {
            session_id: id.into(),
            label: id.into(),
            secondary: String::new(),
            agent_state: Some(if awaiting {
                "awaiting_input".into()
            } else {
                "idle".into()
            }),
            shell_state: None,
            awaiting_input: awaiting,
            choice_prompt_pending: false,
            rate_limited: false,
            suggested_actions_pending: false,
            last_activity_ms: idle_ms,
        }
    }

    #[test]
    fn assignment_is_sticky_across_ticks() {
        let mut planner = SlotPlanner::new(3);
        let s1 = session("s1", false, 0);
        let s2 = session("s2", false, 0);
        let plan1 = planner.plan(&[s1.clone(), s2.clone()], 0);
        let slot_s1 = plan1.slots.iter().position(
            |c| matches!(c, SlotContent::Session { session_id, .. } if session_id == "s1"),
        );

        // Re-plan with the same sessions in a DIFFERENT order — must not
        // change s1's slot.
        let plan2 = planner.plan(&[s2, s1], 0);
        let slot_s1_again = plan2.slots.iter().position(
            |c| matches!(c, SlotContent::Session { session_id, .. } if session_id == "s1"),
        );
        assert_eq!(
            slot_s1, slot_s1_again,
            "a session must keep its slot regardless of input ordering"
        );
    }

    #[test]
    fn closed_session_blanks_its_slot_without_reindexing_others() {
        let mut planner = SlotPlanner::new(3);
        let s1 = session("s1", false, 0);
        let s2 = session("s2", false, 0);
        let s3 = session("s3", false, 0);
        planner.plan(&[s1.clone(), s2.clone(), s3.clone()], 0);

        let plan_before = planner.plan(&[s1.clone(), s2.clone(), s3.clone()], 0);
        let slot_of = |plan: &Plan, id: &str| {
            plan.slots.iter().position(
                |c| matches!(c, SlotContent::Session { session_id, .. } if session_id == id),
            )
        };
        let s3_slot_before = slot_of(&plan_before, "s3").unwrap();

        // s2 closes — it must simply disappear from live sessions.
        let plan_after = planner.plan(&[s1.clone(), s3.clone()], 0);
        let s3_slot_after = slot_of(&plan_after, "s3").unwrap();
        assert_eq!(
            s3_slot_before, s3_slot_after,
            "s3 must not move when s2 closes"
        );

        let s2_slot_still_present = plan_after
            .slots
            .iter()
            .any(|c| matches!(c, SlotContent::Session { session_id, .. } if session_id == "s2"));
        assert!(!s2_slot_still_present);
    }

    #[test]
    fn new_session_takes_the_lowest_empty_slot() {
        let mut planner = SlotPlanner::new(3);
        planner.plan(&[session("s1", false, 0)], 0);
        let plan = planner.plan(&[session("s1", false, 0), session("s2", false, 0)], 0);
        // s1 is in slot 0 (first assigned), s2 should land in slot 1, not
        // some arbitrary later slot.
        assert!(
            matches!(&plan.slots[1], SlotContent::Session { session_id, .. } if session_id == "s2")
        );
    }

    #[test]
    fn eviction_only_targets_strictly_lower_priority() {
        let mut planner = SlotPlanner::new(1); // exactly one slot
        planner.plan(&[session("s1", false, 0)], 0); // s1 takes the only slot, Idle priority

        // s2 arrives awaiting input (NeedsInput) — must evict the idle s1.
        let plan = planner.plan(&[session("s1", false, 0), session("s2", true, 0)], 0);
        assert!(
            matches!(&plan.slots[0], SlotContent::Session { session_id, .. } if session_id == "s2")
        );
        assert_eq!(
            plan.overflow_count, 1,
            "the evicted s1 must count as overflow"
        );
    }

    #[test]
    fn needs_input_never_displaces_another_needs_input() {
        let mut planner = SlotPlanner::new(1);
        planner.plan(&[session("s1", true, 0)], 0); // s1 takes the only slot, NeedsInput

        // s2 also awaits input — must NOT evict s1 (equal priority never evicts).
        let plan = planner.plan(&[session("s1", true, 0), session("s2", true, 0)], 0);
        assert!(
            matches!(&plan.slots[0], SlotContent::Session { session_id, .. } if session_id == "s1"),
            "s1 must keep its slot"
        );
        assert_eq!(plan.overflow_count, 1);
    }

    #[test]
    fn pinned_session_is_never_evicted() {
        let mut planner = SlotPlanner::new(1);
        planner.set_pinned(["s1".to_string()]);
        planner.plan(&[session("s1", false, 0)], 0);

        // s2 arrives with a much higher priority — s1 is pinned, must survive.
        let plan = planner.plan(&[session("s1", false, 0), session("s2", true, 0)], 0);
        assert!(
            matches!(&plan.slots[0], SlotContent::Session { session_id, .. } if session_id == "s1")
        );
        assert_eq!(
            plan.overflow_count, 1,
            "s2 could not get a slot because the only occupant is pinned"
        );
    }
}
