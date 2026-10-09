---
id: 1612-70d1
title: "Automations: Durable run ledger"
status: pending
priority: P2
type: feature
created: "2026-10-09T16:14:58.209Z"
updated: "2026-10-09T16:15:27.312Z"
dependencies: ["1610-ac85"]
plan: plans/automations-scheduler.md
plan_step: Step 3
depends_on: ["stories/1610-ac85-pending-P2-automations-definition-storage.md"]
---

# Automations: Durable run ledger

## Problem Statement

Implement durable run ledger for the approved Automations scheduler. See the linked plan for the fixed contract and exclusions.

## Acceptance Criteria

- [x] Create versioned automation_runs.sqlite3 using workflows/run owner-lock, WAL/busy-timeout and transaction patterns; fail closed on unsupported schemas.
- [x] Reserve a scheduled occurrence uniquely by automation id and UTC occurrence before external effects; manual requests have unique run ids.
- [x] Persist definition snapshot, trigger, status, timestamps, task/session/workspace pointers, bounded output, precheck result and reason; retain history after deletion.
- [x] At boot mark every open run interrupted transactionally, never retry; distinguish completed, failed, needs_you (active), unknown, timed_out, interrupted and skip reasons; terminal transitions are idempotent.
- [x] Provide paginated history and UTC 24h/7d aggregates; age retention prunes only final rows.
- [ ] RED: durable_occurrence_reservation_survives_reopen_and_rejects_duplicate_dispatch — catches: duplicate scheduled dispatch after restart; base 0f87a8c2f missing API compile-only RED (exit 101), no runtime assertion evidence
- [x] GREEN: targeted automations::store::tests nextest --lib --no-default-features: 9 passed; cargo build --bin tuic-remote --no-default-features passed
## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## Work Log

### 2026-10-09T16:19:10.674Z - OpenClaw reference addition: preserve notification attempted/confirmed/unknown separately from execution status; canonical saved output remains the result report. See plan OpenClaw reference; no new delivery framework.

### 2026-10-09T16:39:29.035Z - RED: durable_occurrence_reservation_survives_reopen_and_rejects_duplicate_dispatch — catches: duplicate scheduled dispatch after restart. Base 0f87a8c2f; targeted nextest exited 101 with missing automations::run/RunOwner API (compile-only feature RED, not runtime assertion evidence). Log: ~/Gits/.tmp/results/tuic-ledger-red.log.

### 2026-10-09T16:47:07.378Z - REALITY: effect=reserve before dispatch, inspect immutable history; state=owner-guarded instance SQLite; assumptions=UTC milliseconds from scheduler, no invented agent evidence; radius=run/store modules plus minimal registration and storage docs. RED on 0f87a8c2f: missing ledger API compile-only failure, not runtime assertion evidence. GREEN: nextest --lib --no-default-features -E test(automations::store::tests): 9 passed; cargo build --bin tuic-remote --no-default-features passed. Native macOS headless only. Medium risk; parent owns independent critic, landing and completion.

