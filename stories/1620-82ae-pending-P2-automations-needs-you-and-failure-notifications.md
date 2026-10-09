---
id: 1620-82ae
title: "Automations: Needs-you and failure notifications"
status: pending
priority: P2
type: feature
created: "2026-10-09T16:14:58.220Z"
updated: "2026-10-09T16:15:30.506Z"
dependencies: ["1616-1882", "1618-685b"]
plan: plans/automations-scheduler.md
plan_step: Step 11
depends_on: ["stories/1616-1882-pending-P2-automations-completion-and-maximum-duration.md", "stories/1618-685b-pending-P2-automations-automations-dialog.md"]
---

# Automations: Needs-you and failure notifications

## Problem Statement

Implement needs-you and failure notifications for the approved Automations scheduler. See the linked plan for the fixed contract and exclusions.

## Acceptance Criteria

- [ ] Notify once per run transition to needs_you or failed using existing channels/preferences; link the run and its owning session.
- [ ] Persist deduplication across reconnect/restart, avoid storms on repeated awaiting signals, and keep unknown/interrupted states visible without claiming success.
- [ ] Test backend notification routing and frontend bell navigation; document preferences and limitations.

## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## Work Log

### 2026-10-09T16:19:10.890Z - OpenClaw reference addition: notices link to canonical run/session, remain quiet for precheck skips, never alter successful execution on delivery failure, deduplicate per run/transition/channel and never replay ambiguous sends or historical notices on boot. No heartbeat/model polling or new routing framework.

