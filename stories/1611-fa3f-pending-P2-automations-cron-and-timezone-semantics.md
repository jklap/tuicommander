---
id: 1611-fa3f
title: "Automations: Cron and timezone semantics"
status: pending
priority: P2
type: feature
created: "2026-10-09T16:14:58.208Z"
updated: "2026-10-09T16:15:27.103Z"
dependencies: ["1610-ac85"]
plan: plans/automations-scheduler.md
plan_step: Step 2
depends_on: ["stories/1610-ac85-pending-P2-automations-definition-storage.md"]
---

# Automations: Cron and timezone semantics

## Problem Statement

Implement cron and timezone semantics for the approved Automations scheduler. See the linked plan for the fixed contract and exclusions.

## Acceptance Criteria

- [ ] Accept exactly five-field Vixie cron with day-of-month/day-of-week OR semantics; reject malformed and impossible schedules with bounded errors.
- [ ] Evaluate next and latest-due occurrences in the stored chrono-tz IANA zone, never the host zone; default omitted timezone to the local IANA zone at creation, fail visibly if unavailable.
- [ ] Test America/New_York versus UTC, spring gap and fall fold, leap dates, weekday/hourly/daily/weekly presets and manual custom cron; fixed wall-clock times skip gaps and fire once at the earlier fold instant.
- [ ] Return backend preview/preset results so the UI does not calculate schedules.

## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## Work Log

