//! Gesture resolution: raw `InputEvent`s -> tap / double-tap / hold -> an
//! `ActionSink` call.
//!
//! **The actual hardware model, confirmed 2026-09-15 across four rounds of
//! testing (quick tap, slow tap, quick double-tap, and a ~3.5s hold), after
//! two earlier — wrong — hypotheses:**
//!
//! 1. First hypothesis: a hold shows up as a back-to-back *run* of repeated
//!    Down/Up pairs while held (based on how `mirajazz`'s
//!    `!supports_both_keypress_states` synthesis works). Disproven: a real
//!    hold produces no repeats at all.
//! 2. Second hypothesis (from that disproof, plus a 3-second observation
//!    window that happened to end before anything else arrived): a
//!    physical press produces exactly one Down+Up pair, full stop, with a
//!    *second* pair on a quick tap being wire-level bounce to be debounced
//!    away. Disproven by exactly the data debouncing predicted wouldn't
//!    happen: a ~29s-idle press resolved as a lone `Tap` on schedule, and
//!    then — 3499ms later — a *second*, independent-looking Down/Up pair
//!    arrived and *also* resolved as its own `Tap`. Two events, 3.5 seconds
//!    apart, from one continuous physical hold-then-release.
//!
//! **The actual model:** every physical press generates exactly two
//! Down+Up pairs — one at the press edge, one at the release edge — no
//! matter how long the key was held in between. `59ms`, `239ms`, and
//! `3499ms` gaps have all been observed between a press-pair and its
//! matching release-pair. There is no way to distinguish a press-edge
//! report from a release-edge report except by *pairing*: the first Down
//! for a slot with no pending state is the press; the next Down is that
//! press's release, and the gap between them is genuine, real hold
//! duration — which also means **`Hold` is back as a real, distinguishable
//! gesture**, resolved once the release-Down arrives (there is no way to
//! detect it earlier — nothing signals "still held" on this hardware, only
//! "here is how long it was held, now that it's over").
//!
//! A quick, deliberate double-tap (four raw Downs: press1, release1,
//! press2, release2) is disambiguated from a single slow tap by pairing
//! Downs strictly in order — the resolver never has more than one pending
//! press per slot at a time, so a double-tap's own press/release pairing
//! is unambiguous once each pair is tracked correctly.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::device::InputEvent;
use crate::policy::{ActionKind, KeyRole};
use crate::port::ActionSink;

/// A press-release gap at or above this is a `Hold`, not a `Tap`. Set
/// comfortably above the largest observed "just a deliberate slow tap" gap
/// (239ms) and comfortably below the smallest observed genuine hold
/// (3499ms) — there's a wide, unambiguous margin between those two numbers,
/// so this doesn't need to be precisely tuned.
const HOLD_THRESHOLD: Duration = Duration::from_millis(600);
/// After one tap completes (press+release, held under `HOLD_THRESHOLD`), a
/// second press-edge Down arriving within this long counts as the start of
/// a `DoubleTap`; measured against a real quick-double-tap sample (gaps of
/// 76ms/99ms/80ms between the four raw events, all comfortably inside this
/// window).
const DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(400);
/// Safety net only: an `AwaitingRelease` entry older than this is dropped
/// silently by `tick` rather than left pending forever, in case a release
/// report is ever lost (device unplugged mid-press, a dropped USB report).
/// Set far above any observed real hold so it never fires in practice.
const STUCK_PRESS_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Gesture {
    Tap,
    DoubleTap,
    Hold,
}

enum SlotState {
    /// Waiting for the release-edge Down that completes this press.
    /// `is_second_of_double` is set when this press-edge arrived within
    /// `DOUBLE_TAP_WINDOW` of a prior completed tap, so its *own* release
    /// (if under `HOLD_THRESHOLD`) resolves as `DoubleTap` rather than
    /// starting a fresh single-tap wait.
    AwaitingRelease {
        press_at: Instant,
        is_second_of_double: bool,
    },
    /// One tap has fully completed (press+release, not a hold); waiting to
    /// see whether a second press starts within `DOUBLE_TAP_WINDOW`.
    CompletedOnce { completed_at: Instant },
}

/// Per-slot gesture state. Owned by the coordinator, fed `InputEvent`s as
/// they arrive from `device::reader`, and polled periodically (via `tick`)
/// so a lone completed tap resolves once its double-tap window has passed
/// with no second press.
pub struct GestureResolver {
    state: HashMap<u8, SlotState>,
}

impl GestureResolver {
    pub fn new() -> Self {
        Self {
            state: HashMap::new(),
        }
    }

    /// Feed one input event. `Up` carries no information (it's synthesized
    /// alongside `Down`, not an independent release signal — `Down` pairing
    /// is what carries the real press/release semantics here) and is
    /// ignored. Resolves immediately for `Hold` and the completing half of
    /// a `DoubleTap`; a lone `Tap` only resolves later, via `tick`, once
    /// its double-tap window has passed with nothing following it.
    pub fn on_event(&mut self, event: InputEvent, now: Instant) -> Option<(u8, Gesture)> {
        let InputEvent::Down(slot) = event else {
            return None;
        };
        match self.state.remove(&slot) {
            None => {
                // A press-edge Down with nothing pending: start waiting for
                // its release.
                self.state.insert(
                    slot,
                    SlotState::AwaitingRelease {
                        press_at: now,
                        is_second_of_double: false,
                    },
                );
                None
            }
            Some(SlotState::AwaitingRelease {
                press_at,
                is_second_of_double,
            }) => {
                // This Down is the release-edge completing the pending
                // press. Classify by the measured hold duration.
                let hold = now.duration_since(press_at);
                if hold >= HOLD_THRESHOLD {
                    Some((slot, Gesture::Hold))
                } else if is_second_of_double {
                    Some((slot, Gesture::DoubleTap))
                } else {
                    self.state
                        .insert(slot, SlotState::CompletedOnce { completed_at: now });
                    None
                }
            }
            Some(SlotState::CompletedOnce { completed_at }) => {
                // A new press-edge Down. Within the double-tap window of
                // the prior completed tap, it's the start of a DoubleTap;
                // otherwise it's an unrelated, fresh press.
                let is_second_of_double = now.duration_since(completed_at) <= DOUBLE_TAP_WINDOW;
                self.state.insert(
                    slot,
                    SlotState::AwaitingRelease {
                        press_at: now,
                        is_second_of_double,
                    },
                );
                None
            }
        }
    }

    /// Call periodically (the coordinator's own 250ms tick is a fine
    /// cadence) to resolve any lone completed tap whose double-tap window
    /// has expired with no second press, and to drop any pathologically
    /// stuck `AwaitingRelease` entry (see `STUCK_PRESS_TIMEOUT`).
    pub fn tick(&mut self, now: Instant) -> Vec<(u8, Gesture)> {
        let mut resolved = Vec::new();
        let mut stuck = Vec::new();
        for (&slot, state) in &self.state {
            match state {
                SlotState::CompletedOnce { completed_at }
                    if now.duration_since(*completed_at) > DOUBLE_TAP_WINDOW =>
                {
                    resolved.push(slot);
                }
                SlotState::AwaitingRelease { press_at, .. }
                    if now.duration_since(*press_at) > STUCK_PRESS_TIMEOUT =>
                {
                    stuck.push(slot);
                }
                _ => {}
            }
        }
        for slot in &resolved {
            self.state.remove(slot);
        }
        for slot in &stuck {
            tracing::warn!(
                "streamdock: slot {slot} stuck awaiting a release for over {STUCK_PRESS_TIMEOUT:?}; dropping"
            );
            self.state.remove(slot);
        }
        resolved
            .into_iter()
            .map(|slot| (slot, Gesture::Tap))
            .collect()
    }
}

impl Default for GestureResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// What a resolved gesture should do, given the role bound to that slot and
/// whether the focused session (if this is a session tile) currently has a
/// choice prompt pending. This function is pure — it decides *what* to do;
/// the caller (`coordinator.rs`) is responsible for actually calling
/// `ActionSink` and knowing which `session_id` a `Session`-role slot
/// currently holds.
pub fn dispatch(
    sink: &dyn ActionSink,
    role: &KeyRole,
    gesture: Gesture,
    session_id: Option<&str>,
    choice_option_keys: &[String],
) {
    match role {
        KeyRole::Session | KeyRole::PinnedSession { .. } => {
            let Some(session_id) = session_id else { return };
            match gesture {
                Gesture::Hold => {
                    let _ = sink.write_parts(session_id, vec!["\u{3}".to_string()]);
                }
                Gesture::Tap => {
                    if let Some(key) = choice_option_keys.first() {
                        let _ = sink.write_parts(session_id, vec![key.clone()]);
                    } else {
                        let _ = sink.focus_session(session_id);
                    }
                }
                Gesture::DoubleTap => {
                    if let Some(key) = choice_option_keys.get(1) {
                        let _ = sink.write_parts(session_id, vec![key.clone()]);
                    } else {
                        let _ = sink.focus_session(session_id);
                    }
                }
            }
        }
        KeyRole::Action(kind) => {
            if !matches!(gesture, Gesture::Tap) {
                return; // verb keys are single-purpose, already-immediate actions — no hold/double-tap variant in v1
            }
            match kind {
                ActionKind::JumpWaitingSession => {
                    let _ = sink.run_ui_action("jump-waiting-terminal");
                }
                ActionKind::ActivityDashboard => {
                    let _ = sink.run_ui_action("activity-dashboard");
                }
                ActionKind::InterruptFocused => {
                    if let Some(session_id) = session_id {
                        let _ = sink.write_parts(session_id, vec!["\u{3}".to_string()]);
                    }
                }
                ActionKind::ApproveFocused | ActionKind::RejectFocused => {
                    if let (Some(session_id), Some(key)) = (
                        session_id,
                        if matches!(kind, ActionKind::ApproveFocused) {
                            choice_option_keys.first()
                        } else {
                            choice_option_keys.get(1)
                        },
                    ) {
                        let _ = sink.write_parts(session_id, vec![key.clone()]);
                    }
                }
                ActionKind::DeckSleep => { /* handled by the coordinator directly — no ActionSink call */
                }
            }
        }
        KeyRole::Overflow | KeyRole::Blank => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn a_quick_tap_resolves_to_tap_once_the_release_down_arrives() {
        // Real sample: press, then release 59ms later.
        let mut resolver = GestureResolver::new();
        let t0 = Instant::now();
        assert_eq!(
            resolver.on_event(InputEvent::Down(0), t0),
            None,
            "press-edge must not resolve anything yet"
        );
        assert_eq!(
            resolver.on_event(InputEvent::Down(0), t0 + Duration::from_millis(59)),
            None,
            "completes a tap, but must wait for the double-tap window before firing"
        );
        let resolved = resolver
            .tick(t0 + Duration::from_millis(59) + DOUBLE_TAP_WINDOW + Duration::from_millis(10));
        assert_eq!(resolved, vec![(0, Gesture::Tap)]);
    }

    #[test]
    fn a_slow_tap_still_resolves_to_tap_not_hold() {
        // Real sample: press, then release 239ms later — well under HOLD_THRESHOLD.
        let mut resolver = GestureResolver::new();
        let t0 = Instant::now();
        resolver.on_event(InputEvent::Down(0), t0);
        assert_eq!(
            resolver.on_event(InputEvent::Down(0), t0 + Duration::from_millis(239)),
            None
        );
        let resolved = resolver
            .tick(t0 + Duration::from_millis(239) + DOUBLE_TAP_WINDOW + Duration::from_millis(10));
        assert_eq!(resolved, vec![(0, Gesture::Tap)]);
    }

    #[test]
    fn a_real_hold_resolves_to_hold_immediately_on_release() {
        // Real sample: press, then release 3499ms later.
        let mut resolver = GestureResolver::new();
        let t0 = Instant::now();
        resolver.on_event(InputEvent::Down(0), t0);
        assert_eq!(
            resolver.on_event(InputEvent::Down(0), t0 + Duration::from_millis(3499)),
            Some((0, Gesture::Hold)),
            "a long press/release gap must resolve to Hold the instant the release-Down arrives"
        );
    }

    #[test]
    fn a_quick_double_tap_resolves_to_double_tap_on_the_second_release() {
        // Real sample: four raw Downs, gaps 76ms/99ms/80ms.
        let mut resolver = GestureResolver::new();
        let t0 = Instant::now();
        let d1 = t0;
        let d2 = d1 + Duration::from_millis(76); // release of tap 1 (hold 76ms)
        let d3 = d2 + Duration::from_millis(99); // press of tap 2
        let d4 = d3 + Duration::from_millis(80); // release of tap 2 (hold 80ms)
        assert_eq!(resolver.on_event(InputEvent::Down(0), d1), None);
        assert_eq!(
            resolver.on_event(InputEvent::Down(0), d2),
            None,
            "completes tap 1, waits to see if a double-tap follows"
        );
        assert_eq!(
            resolver.on_event(InputEvent::Down(0), d3),
            None,
            "press-edge of tap 2, must wait for its own release"
        );
        assert_eq!(
            resolver.on_event(InputEvent::Down(0), d4),
            Some((0, Gesture::DoubleTap)),
            "release of tap 2, within the double-tap window of tap 1's completion"
        );
    }

    #[test]
    fn a_late_second_tap_is_two_separate_taps_not_a_double_tap() {
        let mut resolver = GestureResolver::new();
        let t0 = Instant::now();
        resolver.on_event(InputEvent::Down(5), t0);
        resolver.on_event(InputEvent::Down(5), t0 + Duration::from_millis(50));
        let first_resolved = resolver
            .tick(t0 + Duration::from_millis(50) + DOUBLE_TAP_WINDOW + Duration::from_millis(10));
        assert_eq!(first_resolved, vec![(5, Gesture::Tap)]);

        let t1 = t0 + Duration::from_secs(2); // well outside DOUBLE_TAP_WINDOW
        assert_eq!(resolver.on_event(InputEvent::Down(5), t1), None);
        resolver.on_event(InputEvent::Down(5), t1 + Duration::from_millis(50));
        let second_resolved = resolver
            .tick(t1 + Duration::from_millis(50) + DOUBLE_TAP_WINDOW + Duration::from_millis(10));
        assert_eq!(second_resolved, vec![(5, Gesture::Tap)]);
    }

    #[test]
    fn a_hold_after_a_completed_tap_still_resolves_to_hold_not_double_tap() {
        // Tap once (quick), then hold the second press for a long time —
        // Hold must win over the "this looked like it was starting a
        // double-tap" context.
        let mut resolver = GestureResolver::new();
        let t0 = Instant::now();
        resolver.on_event(InputEvent::Down(0), t0);
        resolver.on_event(InputEvent::Down(0), t0 + Duration::from_millis(50)); // completes tap 1
        let second_press = t0 + Duration::from_millis(150);
        resolver.on_event(InputEvent::Down(0), second_press); // press-edge of a would-be double-tap
        assert_eq!(
            resolver.on_event(InputEvent::Down(0), second_press + Duration::from_secs(2)),
            Some((0, Gesture::Hold)),
            "a long hold must win over double-tap classification"
        );
    }

    #[test]
    fn different_slots_do_not_interfere_with_each_others_double_tap_window() {
        let mut resolver = GestureResolver::new();
        let t0 = Instant::now();
        resolver.on_event(InputEvent::Down(1), t0);
        resolver.on_event(InputEvent::Down(1), t0 + Duration::from_millis(50));
        resolver.on_event(InputEvent::Down(2), t0);
        resolver.on_event(InputEvent::Down(2), t0 + Duration::from_millis(50));
        let resolved = resolver
            .tick(t0 + Duration::from_millis(50) + DOUBLE_TAP_WINDOW + Duration::from_millis(10));
        assert_eq!(resolved.len(), 2);
        assert!(resolved.contains(&(1, Gesture::Tap)));
        assert!(resolved.contains(&(2, Gesture::Tap)));
    }

    #[test]
    fn up_events_are_ignored() {
        let mut resolver = GestureResolver::new();
        assert_eq!(resolver.on_event(InputEvent::Up(4), Instant::now()), None);
        assert_eq!(
            resolver.tick(Instant::now()),
            Vec::new(),
            "an Up with no preceding Down must never resolve anything"
        );
    }

    #[test]
    fn a_pathologically_stuck_press_is_dropped_by_tick_not_left_forever() {
        let mut resolver = GestureResolver::new();
        let t0 = Instant::now();
        resolver.on_event(InputEvent::Down(6), t0); // release report never arrives
        let resolved = resolver.tick(t0 + STUCK_PRESS_TIMEOUT + Duration::from_secs(1));
        assert!(
            resolved.is_empty(),
            "a stuck press must be dropped silently, not resolved as a gesture"
        );
        // A fresh press afterward must behave normally, not still be stuck.
        assert_eq!(
            resolver.on_event(
                InputEvent::Down(6),
                t0 + STUCK_PRESS_TIMEOUT + Duration::from_secs(2)
            ),
            None
        );
    }

    struct RecordingSink {
        calls: Mutex<Vec<String>>,
    }
    impl ActionSink for RecordingSink {
        fn write_parts(&self, session_id: &str, parts: Vec<String>) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("write_parts({session_id}, {parts:?})"));
            Ok(())
        }
        fn focus_session(&self, session_id: &str) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("focus_session({session_id})"));
            Ok(())
        }
        fn run_ui_action(&self, name: &str) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("run_ui_action({name})"));
            Ok(())
        }
        fn answer_confirm(&self, request_id: &str, confirmed: bool) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("answer_confirm({request_id}, {confirmed})"));
            Ok(())
        }
    }

    #[test]
    fn tap_on_session_tile_focuses_when_no_choice_prompt() {
        let sink = RecordingSink {
            calls: Mutex::new(vec![]),
        };
        dispatch(&sink, &KeyRole::Session, Gesture::Tap, Some("s1"), &[]);
        assert_eq!(
            sink.calls.lock().unwrap().as_slice(),
            &["focus_session(s1)".to_string()]
        );
    }

    #[test]
    fn tap_on_session_tile_answers_choice_prompt_when_pending() {
        let sink = RecordingSink {
            calls: Mutex::new(vec![]),
        };
        let options = vec!["y".to_string(), "n".to_string()];
        dispatch(&sink, &KeyRole::Session, Gesture::Tap, Some("s1"), &options);
        assert_eq!(
            sink.calls.lock().unwrap().as_slice(),
            &[r#"write_parts(s1, ["y"])"#.to_string()]
        );
    }

    #[test]
    fn double_tap_on_session_tile_answers_the_second_choice_option() {
        let sink = RecordingSink {
            calls: Mutex::new(vec![]),
        };
        let options = vec!["y".to_string(), "n".to_string()];
        dispatch(
            &sink,
            &KeyRole::Session,
            Gesture::DoubleTap,
            Some("s1"),
            &options,
        );
        assert_eq!(
            sink.calls.lock().unwrap().as_slice(),
            &[r#"write_parts(s1, ["n"])"#.to_string()]
        );
    }

    #[test]
    fn hold_on_session_tile_interrupts() {
        let sink = RecordingSink {
            calls: Mutex::new(vec![]),
        };
        dispatch(
            &sink,
            &KeyRole::Session,
            Gesture::Hold,
            Some("s1"),
            &["y".to_string()],
        );
        assert_eq!(
            sink.calls.lock().unwrap().as_slice(),
            &["write_parts(s1, [\"\\u{3}\"])".to_string()]
        );
    }

    #[test]
    fn double_tap_has_no_effect_on_a_fixed_verb_key() {
        let sink = RecordingSink {
            calls: Mutex::new(vec![]),
        };
        dispatch(
            &sink,
            &KeyRole::Action(ActionKind::JumpWaitingSession),
            Gesture::DoubleTap,
            None,
            &[],
        );
        assert!(sink.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn hold_has_no_effect_on_a_fixed_verb_key() {
        let sink = RecordingSink {
            calls: Mutex::new(vec![]),
        };
        dispatch(
            &sink,
            &KeyRole::Action(ActionKind::InterruptFocused),
            Gesture::Hold,
            Some("s1"),
            &[],
        );
        assert!(sink.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn interrupt_action_key_still_works_via_a_plain_tap() {
        let sink = RecordingSink {
            calls: Mutex::new(vec![]),
        };
        dispatch(
            &sink,
            &KeyRole::Action(ActionKind::InterruptFocused),
            Gesture::Tap,
            Some("s1"),
            &[],
        );
        assert_eq!(
            sink.calls.lock().unwrap().as_slice(),
            &["write_parts(s1, [\"\\u{3}\"])".to_string()]
        );
    }

    #[test]
    fn jump_waiting_action_key_runs_the_allowlisted_ui_action() {
        let sink = RecordingSink {
            calls: Mutex::new(vec![]),
        };
        dispatch(
            &sink,
            &KeyRole::Action(ActionKind::JumpWaitingSession),
            Gesture::Tap,
            None,
            &[],
        );
        assert_eq!(
            sink.calls.lock().unwrap().as_slice(),
            &["run_ui_action(jump-waiting-terminal)".to_string()]
        );
    }
}
