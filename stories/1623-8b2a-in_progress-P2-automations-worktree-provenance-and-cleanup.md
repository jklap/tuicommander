---
id: 1623-8b2a
title: "Automations: Worktree provenance and cleanup"
status: in_progress
priority: P2
type: feature
created: "2026-10-09T16:14:58.224Z"
updated: "2026-10-11T08:10:35.004Z"
dependencies: ["1615-57cf", "1619-b84d"]
plan: plans/automations-scheduler.md
plan_step: Step 14
depends_on: ["stories/1615-57cf-pending-P2-automations-dispatch-and-shared-runtime-boot.md", "stories/1619-b84d-pending-P2-automations-runs-view-and-output-navigation.md"]
started_at: "2026-10-11T08:10:35.004Z"
---

# Automations: Worktree provenance and cleanup

## Problem Statement

Implement worktree provenance and cleanup for the approved Automations scheduler. See the linked plan for the fixed contract and exclusions.

## Acceptance Criteria

- [ ] Phase 3 only: persist automation/run provenance and offer cleanup through existing integration/safety proof, preserving dirty/unmerged/unarchived worktrees and live sessions.
- [ ] TEST-CHANGE: src-tauri/src/automations/mcp_tests.rs::mcp_automation_tool_is_discoverable_with_all_definition_actions — old expectation wrong because the approved phase 3 adds worktree_preview and cleanup_worktree to the advertised shared API
## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## Work Log

### 2026-10-11T07:51:12.000Z - First rb run: frontend and TypeScript passed; nextest 14/15 passed including all 4 new cleanup tests. Existing MCP discovery action inventory failed because it omitted the new two actions; expectation updated to the approved public action contract. Clippy did not run due to the failing earlier gate.

### 2026-10-11T08:10:35.230Z - Canonical contract, checked acceptance criteria and final evidence are in MAIN story store. Implemented safe saved-run cleanup and retained provenance. rb targeted nextest 15/15, frontend 16/16, TypeScript and clippy passed. Visual verification blocked by locked Mac and no browser on rb; coordinator critic remains required. Do not complete or land.

