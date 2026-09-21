//! Read-only "why is this session in this state" snapshot — the shared
//! backend half of the state-explain troubleshooting feature.
//! `explain_session_state_impl` is the single entry point both the Tauri
//! command (`pty/commands.rs`) and the HTTP route (`mcp_http/session.rs`)
//! call, so the two transports can never disagree. See
//! `docs/backend/pty.md`'s "Session state explain" section for the full
//! design rationale (the four-layer chain, why there is no rule-manifest
//! concept here, why the trail lives on `SilenceState` and not
//! `TurnEvidence`).
//!
//! A child module of `pty`, not a top-level module: `SilenceState` and
//! `TurnEvidence`'s fields are private to `pty`, and a child module inherits
//! that visibility for free, so this file never needs its own set of
//! `pub(crate)` accessors that would rot the moment a field is added —
//! exactly the drift this feature exists to prevent.
//!
//! Deliberately does not call three functions that look like the obvious
//! choice:
//! - `get_session_foreground_process_impl` — mirrors `agent_type` into
//!   `session_states` as a side effect; an inspect command must not mutate
//!   the thing it inspects. This reports the *stored* `agent_type` instead.
//! - `detect_agent_screen_activity` — bumps a counter a test asserts on, and
//!   a fresh classification isn't the one that produced the current state.
//!   This reports `SilenceState::cached_screen_activity` instead.
//! - Nothing re-derives `session_state_with_shell`'s `agent_state` ladder:
//!   `session_state_with_shell_detailed` (`state.rs`) already returns the
//!   rung alongside it, so the two can never disagree.
//!
//! `decide()` IS called here, on the snapshotted evidence — it's pure
//! (`_now` is unused), so re-running it is genuinely side-effect free, and
//! `decide_now` disagreeing with `visible.shell_state` is the single fastest
//! way this payload can localize a "wrong badge" bug to a missed transition
//! rather than bad evidence.

use super::*;

/// A rank + source pair, as reported for held evidence or the evidence that
/// rejected an attempt (`TrailEntryExplain::outranked_by`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RankedSource {
    pub(crate) rank: &'static str,
    pub(crate) source: &'static str,
}

fn rank_label(rank: EvidenceRank) -> &'static str {
    match rank {
        EvidenceRank::Silence => "silence",
        EvidenceRank::Screen => "screen",
        EvidenceRank::Process => "process",
        EvidenceRank::Protocol => "protocol",
    }
}

fn screen_activity_label(activity: AgentScreenActivity) -> &'static str {
    match activity {
        AgentScreenActivity::Working => "working",
        AgentScreenActivity::Ready => "ready",
        AgentScreenActivity::Interrupted => "interrupted",
        AgentScreenActivity::Unknown => "unknown",
    }
}

/// One held axis's evidence (busy, idle, or awaiting), with its age.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct EvidenceSnapshot {
    pub(crate) rank: &'static str,
    pub(crate) source: &'static str,
    pub(crate) age_ms: u64,
}

impl EvidenceSnapshot {
    fn from(evidence: Evidence) -> Self {
        Self {
            rank: rank_label(evidence.rank),
            source: evidence.source,
            age_ms: evidence.at.elapsed().as_millis() as u64,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AgentExplain {
    pub(crate) agent_type: Option<String>,
    pub(crate) agent_seen_running: bool,
    pub(crate) hook_instrumented: bool,
    pub(crate) hook_state_seen: bool,
    /// False for `amp`/`cursor`/`droid` (documented gap) — a structural
    /// reason a session can never reach idle from a screen adapter alone.
    pub(crate) has_ready_screen_adapter: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct VisibleExplain {
    pub(crate) shell_state: Option<String>,
    pub(crate) agent_state: Option<String>,
    /// Which rung of `resolve_agent_state`'s ladder produced `agent_state`
    /// (e.g. `"shell_busy"`, `"background_work"`, `"no_agent_type"`).
    pub(crate) agent_state_rung: &'static str,
    pub(crate) awaiting_input: bool,
    pub(crate) question_confident: bool,
    pub(crate) choice_prompt_present: bool,
    pub(crate) background_work: bool,
    pub(crate) declared_background_work: bool,
    pub(crate) rate_limited: bool,
    pub(crate) queued_commands: u32,
    pub(crate) turn_epoch: u64,
    pub(crate) active_sub_tasks: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EvidenceExplain {
    pub(crate) busy: Option<EvidenceSnapshot>,
    pub(crate) idle: Option<EvidenceSnapshot>,
    pub(crate) awaiting: Option<EvidenceSnapshot>,
    pub(crate) activity_seen: bool,
    pub(crate) idle_confirmed: bool,
    pub(crate) shell_is_busy: bool,
    /// What `decide()` would return right now, re-run on this exact
    /// snapshot. Disagreeing with `visible.shell_state` means a transition
    /// was missed, not that the evidence itself is wrong.
    pub(crate) decide_now: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EpochFlagExplain {
    pub(crate) declared: bool,
    pub(crate) declared_turn_epoch: u64,
    /// Whether `declared_turn_epoch` matches the session's current
    /// `turn_epoch` — a stale declaration from a prior turn reads `false`.
    pub(crate) applies_now: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EpochFlagsExplain {
    pub(crate) completion_declared: EpochFlagExplain,
    pub(crate) declared_background_work: EpochFlagExplain,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ScreenExplain {
    pub(crate) cached_activity: &'static str,
    pub(crate) screen_ready_pending_since_ms: Option<u64>,
    /// True when held busy evidence is `Protocol` rank — the closest analog
    /// to "screen detection is being overridden by a full lifecycle
    /// authority".
    pub(crate) skipped_by_protocol_authority: bool,
    pub(crate) no_adapter_for_agent: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SilenceExplain {
    pub(crate) last_output_ms_ago: Option<u64>,
    pub(crate) last_chunk_ms_ago: u64,
    pub(crate) threshold_ms: u64,
    pub(crate) threshold_reason: &'static str,
    pub(crate) remaining_before_fire_ms: Option<u64>,
    pub(crate) startup_settled: bool,
    pub(crate) last_status_line_ms_ago: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HoldsExplain {
    pub(crate) interrupt_requested_ms_ago: Option<u64>,
    pub(crate) api_retry_hold_remaining_ms: Option<u64>,
    pub(crate) injection_delivery_uncertain: bool,
    pub(crate) active_injection_claim: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct NotificationExplain {
    pub(crate) notification_type: Option<String>,
    pub(crate) has_message: bool,
    pub(crate) shell_already_idle: bool,
    pub(crate) confident: Option<bool>,
    pub(crate) suppressed: bool,
    pub(crate) age_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TrailEntryExplain {
    pub(crate) seq: usize,
    pub(crate) age_ms: u64,
    pub(crate) kind: &'static str,
    pub(crate) rank: Option<&'static str>,
    pub(crate) source: Option<&'static str>,
    pub(crate) accepted: bool,
    pub(crate) forced: bool,
    /// The single most useful field in the whole payload: on a rejected
    /// attempt, what already-held evidence outranked it.
    pub(crate) outranked_by: Option<RankedSource>,
}

fn trail_kind_label(kind: TrailKind) -> &'static str {
    match kind {
        TrailKind::Busy => "busy",
        TrailKind::Idle => "idle",
        TrailKind::ClearBusy => "clear_busy",
        TrailKind::ClearIdle => "clear_idle",
        TrailKind::Awaiting => "awaiting",
        TrailKind::ClearAwaiting => "clear_awaiting",
        TrailKind::UserSubmit => "user_submit",
    }
}

impl TrailEntryExplain {
    fn from(seq: usize, entry: &TrailEntry) -> Self {
        Self {
            seq,
            age_ms: entry.at.elapsed().as_millis() as u64,
            kind: trail_kind_label(entry.kind),
            rank: entry.rank.map(rank_label),
            source: entry.source,
            accepted: entry.accepted,
            forced: entry.forced,
            outranked_by: entry.outranked_by.map(|(rank, source)| RankedSource {
                rank: rank_label(rank),
                source,
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionStateExplain {
    pub(crate) session_id: String,
    pub(crate) captured_at_ms: u64,
    pub(crate) agent: AgentExplain,
    pub(crate) visible: VisibleExplain,
    pub(crate) evidence: EvidenceExplain,
    pub(crate) epoch_flags: EpochFlagsExplain,
    pub(crate) screen: ScreenExplain,
    pub(crate) silence: SilenceExplain,
    pub(crate) holds: HoldsExplain,
    pub(crate) notification: Option<NotificationExplain>,
    /// Oldest first.
    pub(crate) trail: Vec<TrailEntryExplain>,
}

/// Assemble a read-only explain snapshot for `session_id`. Mutates nothing
/// beyond `session_state_with_shell_detailed`'s own idempotent rate-limit
/// expiry (see that method's doc comment) — see this module's own doc
/// comment for the three functions this deliberately does NOT call because
/// they have side effects or re-derive state this already has a single
/// source of truth for.
///
/// Lock ordering: (1) `session_state_with_shell_detailed` takes and releases
/// the `SilenceState` mutex internally; (2) this function then takes that
/// mutex ONCE for everything else — evidence, trail, epoch flags,
/// classification — so those axes can't tear relative to each other. Never
/// nest a second acquisition inside the first.
pub(crate) fn explain_session_state_impl(
    state: &AppState,
    session_id: &str,
) -> Option<SessionStateExplain> {
    let (session, agent_state_rung, completion_declared, background_work) =
        state.session_state_with_shell_detailed(session_id)?;

    let agent = AgentExplain {
        agent_type: session.agent_type.clone(),
        agent_seen_running: session.agent_seen_running,
        hook_instrumented: session.hook_instrumented,
        hook_state_seen: false, // filled in below, once the SilenceState lock is held
        has_ready_screen_adapter: has_ready_screen_adapter(session.agent_type.as_deref()),
    };

    let visible = VisibleExplain {
        shell_state: session.shell_state.clone(),
        agent_state: session.agent_state.clone(),
        agent_state_rung,
        awaiting_input: session.awaiting_input,
        question_confident: session.question_confident,
        choice_prompt_present: session.choice_prompt.is_some(),
        background_work,
        declared_background_work: session.declared_background_work,
        rate_limited: session.rate_limited,
        queued_commands: session.queued_commands,
        turn_epoch: session.turn_epoch,
        active_sub_tasks: session.active_sub_tasks,
    };

    let shell_is_busy = session.shell_state.as_deref() == Some("busy");

    // One lock section for everything SilenceState holds, per this module's
    // doc comment on lock ordering.
    let sl = state.session_maps.silence_states.get(session_id)?;
    let sl = sl.lock();

    let decide_now = match decide(&sl.evidence, shell_is_busy, std::time::Instant::now()) {
        Some(Transition::ToBusy(_)) => Some("to_busy"),
        Some(Transition::ToIdle(_)) => Some("to_idle"),
        None => None,
    };
    let evidence = EvidenceExplain {
        busy: sl.evidence.busy.map(EvidenceSnapshot::from),
        idle: sl.evidence.idle.map(EvidenceSnapshot::from),
        awaiting: sl.evidence.awaiting.map(EvidenceSnapshot::from),
        activity_seen: sl.evidence.activity_seen,
        idle_confirmed: sl.idle_confirmed(),
        shell_is_busy,
        decide_now,
    };

    let epoch_flags = EpochFlagsExplain {
        completion_declared: EpochFlagExplain {
            declared: sl.completion_declared,
            declared_turn_epoch: sl.completion_turn_epoch,
            applies_now: sl.completion_declared && sl.completion_turn_epoch == session.turn_epoch,
        },
        declared_background_work: EpochFlagExplain {
            declared: sl.declared_background_work,
            declared_turn_epoch: sl.declared_background_work_turn_epoch,
            applies_now: sl.declared_background_work
                && sl.declared_background_work_turn_epoch == session.turn_epoch,
        },
    };
    // `completion_declared` (the local computed by `session_state_with_shell_detailed`,
    // OR-ing in `suggested_actions.is_some()`) is intentionally NOT re-derived
    // here — `epoch_flags.completion_declared` reports the raw `SilenceState`
    // field instead, since the OR'd local isn't itself epoch-scoped the same
    // way. `completion_declared` the parameter is still used by nothing below
    // besides having already fed `agent_state_rung`; silence an unused-binding
    // lint by referencing it explicitly.
    let _ = completion_declared;

    let screen = ScreenExplain {
        cached_activity: screen_activity_label(sl.cached_screen_activity),
        screen_ready_pending_since_ms: sl
            .screen_ready_pending_since
            .map(|at| at.elapsed().as_millis() as u64),
        skipped_by_protocol_authority: sl
            .evidence
            .busy
            .is_some_and(|busy| busy.rank == EvidenceRank::Protocol),
        no_adapter_for_agent: !agent.has_ready_screen_adapter,
    };

    let is_agent = session.agent_type.is_some();
    let threshold_ms = if is_agent {
        AGENT_IDLE_MS
    } else {
        SHELL_IDLE_MS
    };
    let last_output_ms_ago = state
        .session_maps
        .last_output_ms
        .get(session_id)
        .map(|ts| ts.load(std::sync::atomic::Ordering::Relaxed))
        .filter(|&ms| ms > 0)
        .map(|ms| now_epoch_ms().saturating_sub(ms));
    let silence_explain = SilenceExplain {
        last_output_ms_ago,
        last_chunk_ms_ago: sl.last_chunk_at.elapsed().as_millis() as u64,
        threshold_ms,
        threshold_reason: if is_agent {
            "agent_type_present"
        } else {
            "plain_shell"
        },
        remaining_before_fire_ms: last_output_ms_ago.map(|ago| threshold_ms.saturating_sub(ago)),
        startup_settled: sl.startup_settled,
        last_status_line_ms_ago: sl
            .last_status_line_at
            .map(|at| at.elapsed().as_millis() as u64),
    };

    let holds = HoldsExplain {
        interrupt_requested_ms_ago: sl
            .interrupt_requested_at
            .map(|at| at.elapsed().as_millis() as u64),
        api_retry_hold_remaining_ms: sl.api_retry_hold_until.map(|until| {
            until
                .saturating_duration_since(std::time::Instant::now())
                .as_millis() as u64
        }),
        injection_delivery_uncertain: sl.injection_delivery_uncertain,
        active_injection_claim: sl.active_injection_claim.is_some(),
    };

    let notification = sl
        .last_notification_classification
        .as_ref()
        .map(|n| NotificationExplain {
            notification_type: n.notification_type.clone(),
            has_message: n.has_message,
            shell_already_idle: n.shell_already_idle,
            confident: n.confident,
            suppressed: n.suppressed,
            age_ms: n.at.elapsed().as_millis() as u64,
        });

    let trail: Vec<TrailEntryExplain> = sl
        .trail
        .entries()
        .enumerate()
        .map(|(seq, entry)| TrailEntryExplain::from(seq, entry))
        .collect();

    let hook_state_seen = sl.hook_state_seen;
    drop(sl);

    Some(SessionStateExplain {
        session_id: session_id.to_string(),
        captured_at_ms: now_epoch_ms(),
        agent: AgentExplain {
            hook_state_seen,
            ..agent
        },
        visible,
        evidence,
        epoch_flags,
        screen,
        silence: silence_explain,
        holds,
        notification,
        trail,
    })
}
