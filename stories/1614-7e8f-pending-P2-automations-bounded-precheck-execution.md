---
id: 1614-7e8f
title: "Automations: Bounded precheck execution"
status: pending
priority: P2
type: feature
created: "2026-10-09T16:14:58.212Z"
updated: "2026-10-09T16:15:27.950Z"
dependencies: ["1610-ac85"]
plan: plans/automations-scheduler.md
plan_step: Step 5
depends_on: ["stories/1610-ac85-pending-P2-automations-definition-storage.md"]
---

# Automations: Bounded precheck execution

## Problem Statement

Implement bounded precheck execution for the approved Automations scheduler. See the linked plan for the fixed contract and exclusions.

## Acceptance Criteria

- [ ] Execute optional precheck in the resolved workspace with the existing clean shell/environment and bounded subprocess pattern; exit 0 proceeds, nonzero/timeout records skipped_precheck.
- [ ] Capture stdout/stderr with truncation and duration, preserve spawn errors distinctly, terminate owned subprocesses on timeout and cover both host shell families.
- [ ] Manual Run Now bypasses precheck and records that choice; do not insert stdout into the prompt in phase 1.

## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## Work Log

