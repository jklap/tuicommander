---
id: 1619-b84d
title: "Automations: Runs view and output navigation"
status: pending
priority: P2
type: feature
created: "2026-10-09T16:14:58.219Z"
updated: "2026-10-09T16:15:30.080Z"
dependencies: ["1618-685b"]
plan: plans/automations-scheduler.md
plan_step: Step 10
depends_on: ["stories/1618-685b-pending-P2-automations-automations-dialog.md"]
---

# Automations: Runs view and output navigation

## Problem Statement

Implement runs view and output navigation for the approved Automations scheduler. See the linked plan for the fixed contract and exclusions.

## Acceptance Criteria

- [ ] Render backend 24h/7d counts and paginated run history with needs_you/unknown/interrupted visible as distinct states.
- [ ] View run focuses the owning live session; unavailable sessions show bounded saved output and workspace details without silently starting another run.
- [ ] Test history retained after automation deletion, missing session/workspace and remote-owner navigation; update user guide and capture screenshots.

## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## Work Log

### 2026-10-10T16:39:28.352Z - GREEN: pnpm exec tsc --noEmit passed; scripts/with-test-tmp.sh pnpm vitest run six focused files passed 26 tests (1.61s). First validation exposed two i18n key collisions and missing summary in an existing test adapter; corrected both. Feature contract, no bug-fix RED required. Screenshot attempts with Chrome for Testing 149.0.7827.54 and .22 timed out at 20s, 30s and 60s; worktree Vite responded successfully but no screenshot files were produced. Visual criterion remains unmet. No desktop, Rust builds, full suites or external services used.

