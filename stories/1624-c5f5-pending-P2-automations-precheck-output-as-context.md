---
id: 1624-c5f5
title: "Automations: Precheck output as context"
status: pending
priority: P2
type: feature
created: "2026-10-09T16:14:58.225Z"
updated: "2026-10-09T16:15:31.766Z"
dependencies: ["1614-7e8f", "1615-57cf"]
plan: plans/automations-scheduler.md
plan_step: Step 15
depends_on: ["stories/1614-7e8f-pending-P2-automations-bounded-precheck-execution.md", "stories/1615-57cf-pending-P2-automations-dispatch-and-shared-runtime-boot.md"]
---

# Automations: Precheck output as context

## Problem Statement

Implement precheck output as context for the approved Automations scheduler. See the linked plan for the fixed contract and exclusions.

## Acceptance Criteria

- [ ] Phase 3 only: explicitly opt in to bounded precheck stdout as prompt context with clear delimiters and exact original prompt preservation; cover empty/truncated/invalid output.

## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## Work Log

