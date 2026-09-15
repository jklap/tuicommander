//! The 250ms tick that turns "something changed" into pixels on the panel,
//! and raw button presses into dispatched actions.
//!
//! **Doorbell, not data stream.** `StateSource::subscribe()` yields
//! `Doorbell`s, never session data itself — every doorbell (including a
//! `Lagged`) just sets `dirty = true`, and the *next* tick re-fetches the
//! full picture via `StateSource::snapshot()`. This means lag recovery and
//! the steady state are the same code path, and this module never needs to
//! know anything about the event vocabulary its host adapter listens to.
//!
//! **Coalescing.** At most `MAX_WRITES_PER_TICK` image writes go out per
//! 250ms tick, awaiting-input slots served first, so a burst of changes
//! (e.g. attaching after being unplugged) ramps up smoothly rather than
//! flooding the device's write queue — see `device::actor`'s own bounded
//! channel for the second half of that protection.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::device::{DeviceHandle, DeviceMsg, InputEvent};
use crate::dispatch::{self, Gesture, GestureResolver};
use crate::leds::{self, AmbientLed};
use crate::policy::layout::default_layout;
use crate::policy::rank::priority_of;
use crate::policy::{ActionKind, KeyRole, Priority, SlotContent, SlotPlanner};
use crate::port::{ActionSink, SessionSnapshot, StateSource};
use crate::render::{FaceState, Glyph, KeyFace, Label, RenderCache};

pub const TICK_PERIOD: Duration = Duration::from_millis(250);
const MAX_WRITES_PER_TICK: usize = 6;
/// Stale bucket for elapsed-idle text — re-encodes at most once a minute
/// rather than four times a second, per the `KeyFace` hashing discipline.
const MINUTE_MS: u64 = 60_000;

pub struct Coordinator {
    planner: SlotPlanner,
    layout: Vec<KeyRole>,
    render: RenderCache,
    last_pushed: HashMap<u8, KeyFace>,
    gestures: GestureResolver,
    /// slot -> session_id, refreshed every tick from the latest `Plan` —
    /// this is what lets a fixed `Action` verb key (Approve/Reject/
    /// Interrupt) know which session it currently targets: whichever
    /// session sits in the last-focused/highest-priority slot. v1 keeps
    /// this simple: the verb keys always target the single highest-priority
    /// live session, which is usually also whichever one the user just
    /// focused by tapping its tile.
    last_snapshots: HashMap<String, SessionSnapshot>,
    /// The `now_ms` passed to the most recent `tick()` call — reused by
    /// `fire`/`highest_priority_session` so a gesture resolved between
    /// ticks still ranks sessions against a consistent clock, rather than
    /// re-sampling wall-clock time inconsistently with the render loop.
    last_now_ms: u64,
    led_count: u8,
    /// The ambient color last *returned* by `ambient_led_update` — not
    /// necessarily the color last actually written to the device, since the
    /// caller (`run_one_device`) may skip the write entirely when RGB isn't
    /// supported. `None` until the first call, so the very first tick always
    /// reports a color (even Green) rather than staying silent.
    last_led: Option<AmbientLed>,
}

impl Coordinator {
    pub fn new(num_lcd_slots: u8, led_count: u8, key_px: u32, jpeg_quality: u8) -> Self {
        Self {
            planner: SlotPlanner::new(num_lcd_slots),
            layout: default_layout(num_lcd_slots),
            render: RenderCache::new(key_px, jpeg_quality, 256),
            last_pushed: HashMap::new(),
            gestures: GestureResolver::new(),
            last_snapshots: HashMap::new(),
            last_now_ms: 0,
            led_count,
            last_led: None,
        }
    }

    pub fn set_pinned(&mut self, pinned: impl IntoIterator<Item = String>) {
        self.planner.set_pinned(pinned);
    }

    /// One tick: fetch (if dirty), plan, render+push changed slots, commit.
    /// `dirty` is consumed (reset to false) by this call.
    pub async fn tick(
        &mut self,
        state: &dyn StateSource,
        device: &DeviceHandle,
        dirty: &mut bool,
        now_ms: u64,
    ) {
        self.last_now_ms = now_ms;
        if *dirty {
            let sessions = state.snapshot();
            self.last_snapshots = sessions
                .into_iter()
                .map(|s| (s.session_id.clone(), s))
                .collect();
            *dirty = false;
        }

        let sessions: Vec<SessionSnapshot> = self.last_snapshots.values().cloned().collect();
        let plan = self.planner.plan(&sessions, now_ms);

        let mut faces: Vec<(u8, KeyFace)> = Vec::new();
        for (slot, content) in plan.slots.iter().enumerate() {
            let slot = slot as u8;
            let role = self
                .layout
                .get(slot as usize)
                .cloned()
                .unwrap_or(KeyRole::Blank);
            let face = self.face_for(&role, content, plan.overflow_count, now_ms);
            faces.push((slot, face));
        }

        // Awaiting-input slots first, so a burst of changes always shows
        // the thing the user most needs to see even if the per-tick budget
        // is exhausted before the rest catch up.
        faces.sort_by_key(|(_, face)| std::cmp::Reverse(face.state == FaceState::NeedsInput));

        let mut writes = 0usize;
        for (slot, face) in faces {
            if writes >= MAX_WRITES_PER_TICK {
                break;
            }
            if self.last_pushed.get(&slot) == Some(&face) {
                continue;
            }
            let jpeg = self.render.render(&face);
            if device
                .send(DeviceMsg::SetKeyImage { slot, jpeg })
                .await
                .is_err()
            {
                return; // device actor gone — coordinator's caller will notice via health watch
            }
            self.last_pushed.insert(slot, face);
            writes += 1;
        }
        if writes > 0 {
            let _ = device.send(DeviceMsg::Commit).await;
        }
    }

    fn face_for(
        &self,
        role: &KeyRole,
        content: &SlotContent,
        overflow_count: usize,
        now_ms: u64,
    ) -> KeyFace {
        match role {
            KeyRole::Session | KeyRole::PinnedSession { .. } => match content {
                SlotContent::Empty => KeyFace::empty(),
                SlotContent::Session {
                    session_id,
                    priority,
                } => self
                    .last_snapshots
                    .get(session_id)
                    .map(|s| face_for_session(s, *priority, now_ms))
                    .unwrap_or_else(KeyFace::empty),
            },
            KeyRole::Action(kind) => face_for_action(*kind),
            KeyRole::Overflow => face_for_overflow(overflow_count),
            KeyRole::Blank => KeyFace::empty(),
        }
    }

    /// Feed one input event from `device::reader`. Resolves immediately to
    /// a dispatched `Hold` (once the release-edge `Down` for a long press
    /// arrives — see `dispatch::GestureResolver`'s doc comment for why this
    /// can only be known in retrospect, never while still held) or
    /// `DoubleTap` (once a second press's own release arrives within the
    /// window); a lone `Tap` is only resolved by `tick_gestures` below,
    /// once its double-tap window has passed with no second press.
    pub fn on_input(&mut self, sink: &dyn ActionSink, event: InputEvent, now: Instant) {
        if let Some((slot, gesture)) = self.gestures.on_event(event, now) {
            self.fire(sink, slot, gesture);
        }
    }

    /// Call once per coordinator tick (same cadence as `tick`) to resolve
    /// any gesture run that has gone quiet.
    pub fn tick_gestures(&mut self, sink: &dyn ActionSink, now: Instant) {
        for (slot, gesture) in self.gestures.tick(now) {
            self.fire(sink, slot, gesture);
        }
    }

    fn fire(&self, sink: &dyn ActionSink, slot: u8, gesture: Gesture) {
        let Some(role) = self.layout.get(slot as usize) else {
            return;
        };
        let target = self.highest_priority_session();
        let (session_id, options) = match target {
            Some(s) => (Some(s.session_id.as_str()), self.choice_option_keys(s)),
            None => (None, Vec::new()),
        };
        dispatch::dispatch(sink, role, gesture, session_id, &options);
    }

    /// v1's answer to "which session do the fixed verb keys act on": the
    /// single highest-priority live session. Awaiting-input outranks
    /// everything, so with a session actually waiting on you, Approve/
    /// Reject/Interrupt target it — the common case. With nothing waiting,
    /// they target whichever session is busiest, which is a reasonable
    /// default and the one most likely to be what the user just focused.
    fn highest_priority_session(&self) -> Option<&SessionSnapshot> {
        self.last_snapshots
            .values()
            .max_by_key(|s| priority_of(s, self.last_now_ms))
    }

    fn choice_option_keys(&self, session: &SessionSnapshot) -> Vec<String> {
        session.choice_prompt_options.clone()
    }

    /// Recompute the ambient LED color from the latest snapshot and return
    /// the colors to write **only if it changed** since the last call — the
    /// "one write per state change" discipline from the design plan. Call
    /// once per tick, after `tick()` itself has refreshed `last_snapshots`;
    /// the caller decides whether to actually send it (gated on the
    /// negotiated `FeatureSet::rgb`, which this module knows nothing about).
    pub fn ambient_led_update(&mut self, now_ms: u64) -> Option<Vec<[u8; 3]>> {
        let sessions: Vec<SessionSnapshot> = self.last_snapshots.values().cloned().collect();
        let led = leds::ambient_for(&sessions, now_ms);
        if self.last_led == Some(led) {
            return None;
        }
        self.last_led = Some(led);
        Some(led.colors(self.led_count))
    }
}

/// Pure mapping from a session's derived state to what its tile should
/// show. No I/O, fully unit-testable — this is the function that actually
/// encodes the state color vocabulary decision.
pub fn face_for_session(session: &SessionSnapshot, priority: Priority, now_ms: u64) -> KeyFace {
    let state = match priority {
        Priority::NeedsInput => FaceState::NeedsInput,
        Priority::Error => FaceState::Error,
        Priority::Working => FaceState::Working,
        Priority::CompletedUnread => FaceState::CompletedUnread,
        Priority::Idle => FaceState::Idle,
        Priority::Stale => FaceState::StaleReady,
    };
    let glyph = if state.pulses() {
        Glyph::Pulse(pulse_phase(now_ms))
    } else {
        Glyph::Dot
    };
    let secondary = if matches!(priority, Priority::Stale) {
        format!(
            "ready ({}m)",
            now_ms.saturating_sub(session.last_activity_ms) / MINUTE_MS
        )
    } else {
        session.secondary.clone()
    };
    KeyFace {
        state,
        glyph,
        primary: Label::from_str_truncated(&session.label),
        secondary: Label::from_str_truncated(&secondary),
        badge: None,
    }
}

fn face_for_action(kind: ActionKind) -> KeyFace {
    let (primary, secondary) = match kind {
        ActionKind::ApproveFocused => ("Approve", ""),
        ActionKind::RejectFocused => ("Reject", ""),
        ActionKind::InterruptFocused => ("Stop", ""),
        ActionKind::JumpWaitingSession => ("Waiting", "jump"),
        ActionKind::ActivityDashboard => ("Activity", ""),
        ActionKind::DeckSleep => ("Sleep", ""),
    };
    KeyFace {
        state: FaceState::Idle,
        glyph: Glyph::None,
        primary: Label::from_str_truncated(primary),
        secondary: Label::from_str_truncated(secondary),
        badge: None,
    }
}

fn face_for_overflow(count: usize) -> KeyFace {
    KeyFace {
        state: FaceState::Idle,
        glyph: Glyph::None,
        primary: Label::from_str_truncated("more"),
        secondary: Label::from_str_truncated(&format!("+{count}")),
        badge: if count > 0 {
            Some(count.min(255) as u8)
        } else {
            None
        },
    }
}

/// 4 discrete phase buckets at a 1s period — see `render/palette.rs`'s doc
/// comment on why the pulse is bucketed rather than continuous: a
/// continuous value would defeat `KeyFace`'s content-hash cache entirely.
fn pulse_phase(now_ms: u64) -> u8 {
    ((now_ms / 250) % 4) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(state: &str) -> SessionSnapshot {
        SessionSnapshot {
            session_id: "s1".into(),
            label: "tc-1".into(),
            secondary: "refactor".into(),
            agent_state: Some(state.into()),
            shell_state: None,
            awaiting_input: false,
            choice_prompt_pending: false,
            choice_prompt_options: Vec::new(),
            rate_limited: false,
            suggested_actions_pending: false,
            last_activity_ms: 0,
        }
    }

    #[test]
    fn needs_input_pulses_and_others_dont() {
        let s = session("idle");
        let face = face_for_session(&s, Priority::NeedsInput, 0);
        assert!(matches!(face.glyph, Glyph::Pulse(_)));
        let face = face_for_session(&s, Priority::Working, 0);
        assert!(matches!(face.glyph, Glyph::Dot));
    }

    #[test]
    fn stale_shows_minutes_elapsed() {
        let mut s = session("idle");
        s.last_activity_ms = 0;
        let face = face_for_session(&s, Priority::Stale, 12 * MINUTE_MS);
        assert_eq!(face.secondary.as_str(), "ready (12m)");
    }

    #[test]
    fn pulse_phase_cycles_through_four_buckets_over_one_second() {
        let phases: Vec<u8> = (0..8).map(|i| pulse_phase(i * 125)).collect();
        assert_eq!(phases, vec![0, 0, 1, 1, 2, 2, 3, 3]);
    }

    #[test]
    fn overflow_face_shows_the_count() {
        let face = face_for_overflow(7);
        assert_eq!(face.badge, Some(7));
        assert_eq!(face.secondary.as_str(), "+7");
    }

    #[test]
    fn empty_overflow_has_no_badge() {
        let face = face_for_overflow(0);
        assert_eq!(face.badge, None);
    }

    #[test]
    fn ambient_led_update_reports_once_then_dedups_until_it_changes() {
        let mut c = Coordinator::new(15, 24, 64, 90);
        // First call always reports, even for the boring "nothing going on"
        // case — `last_led` starts at `None`, not `Some(Green)`, precisely
        // so the very first tick still turns the ring on.
        let first = c.ambient_led_update(0);
        assert_eq!(first, Some(AmbientLed::Green.colors(24)));

        // Unchanged state: no write needed.
        assert_eq!(c.ambient_led_update(1), None);

        // A session shows up needing input: reports the change once...
        let mut s = session("idle");
        s.awaiting_input = true;
        c.last_snapshots.insert(s.session_id.clone(), s);
        assert_eq!(c.ambient_led_update(2), Some(AmbientLed::Peach.colors(24)));
        // ...then goes quiet again until it changes once more.
        assert_eq!(c.ambient_led_update(3), None);
    }
}
