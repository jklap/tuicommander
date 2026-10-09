---
id: 1616-1882
title: "Automations: Completion and maximum duration"
status: pending
priority: P2
type: feature
created: "2026-10-09T16:14:58.215Z"
updated: "2026-10-09T16:15:28.816Z"
dependencies: ["1615-57cf"]
plan: plans/automations-scheduler.md
plan_step: Step 7
depends_on: ["stories/1615-57cf-pending-P2-automations-dispatch-and-shared-runtime-boot.md"]
---

# Automations: Completion and maximum duration

## Problem Statement

Implement completion and maximum duration for the approved Automations scheduler. See the linked plan for the fixed contract and exclusions.

## Acceptance Criteria

- [ ] Consume task/PTy/progress signals for completed/failed/needs_you; idle alone is insufficient evidence of success. Handle broadcast lag by reconciling task/session state.
- [ ] Bound runs by persisted start/deadline and stop only the owned PTY/process when max duration expires; needs_you still counts as active and expires.
- [ ] Finalize lost/unverifiable completion as unknown; preserve interrupted on boot, no retry, and ignore stale/duplicate signals after finalization.
- [ ] Capture a bounded final output snapshot for history and expose failed/needs_you transitions through event_bus plus desktop dual-emission.

## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## Work Log

