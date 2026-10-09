---
id: 1613-67f3
title: "Automations: Scheduler admission and catch-up"
status: pending
priority: P2
type: feature
created: "2026-10-09T16:14:58.211Z"
updated: "2026-10-09T16:15:27.749Z"
dependencies: ["1611-fa3f", "1612-70d1"]
plan: plans/automations-scheduler.md
plan_step: Step 4
depends_on: ["stories/1611-fa3f-pending-P2-automations-cron-and-timezone-semantics.md", "stories/1612-70d1-pending-P2-automations-durable-run-ledger.md"]
---

# Automations: Scheduler admission and catch-up

## Problem Statement

Implement scheduler admission and catch-up for the approved Automations scheduler. See the linked plan for the fixed contract and exclusions.

## Acceptance Criteria

- [ ] Implement a deterministic tick against the real ledger: latest due occurrence only, grace-inclusive run-once catch-up, older skipped_missed and no replay backlog.
- [ ] Apply enabled state, per-automation overlap skip and global configured concurrency (default 2), counting pending/dispatching/running/needs_you; reserve decisions durably.
- [ ] Treat full global capacity as skipped_concurrency for that occurrence; Run Now obeys overlap and cap even when paused and never advances scheduled occurrence cursors.
- [ ] Exercise duplicate ticks, clock rollback, sleep/wake gaps, restart interruption, concurrent manual/scheduled admission and limit reduction without cancelling current runs.

## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## Work Log

