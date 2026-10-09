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

- [ ] Create versioned automation_runs.sqlite3 using workflows/run owner-lock, WAL/busy-timeout and transaction patterns; fail closed on unsupported schemas.
- [ ] Reserve a scheduled occurrence uniquely by automation id and UTC occurrence before external effects; manual requests have unique run ids.
- [ ] Persist definition snapshot, trigger, status, timestamps, task/session/workspace pointers, bounded output, precheck result and reason; retain history after deletion.
- [ ] At boot mark every open run interrupted transactionally, never retry; distinguish completed, failed, needs_you (active), unknown, timed_out, interrupted and skip reasons; terminal transitions are idempotent.
- [ ] Provide paginated history and UTC 24h/7d aggregates; age retention prunes only final rows.

## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## Work Log

### 2026-10-09T16:19:10.674Z - OpenClaw reference addition: preserve notification attempted/confirmed/unknown separately from execution status; canonical saved output remains the result report. See plan OpenClaw reference; no new delivery framework.

