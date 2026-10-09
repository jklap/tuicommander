---
id: 1615-57cf
title: "Automations: Dispatch and shared runtime boot"
status: pending
priority: P2
type: feature
created: "2026-10-09T16:14:58.213Z"
updated: "2026-10-09T16:15:28.603Z"
dependencies: ["1612-70d1", "1613-67f3", "1614-7e8f"]
plan: plans/automations-scheduler.md
plan_step: Step 6
depends_on: ["stories/1612-70d1-pending-P2-automations-durable-run-ledger.md", "stories/1613-67f3-pending-P2-automations-scheduler-admission-and-catch-up.md", "stories/1614-7e8f-pending-P2-automations-bounded-precheck-execution.md"]
---

# Automations: Dispatch and shared runtime boot

## Problem Statement

Implement dispatch and shared runtime boot for the approved Automations scheduler. See the linked plan for the fixed contract and exclusions.

## Acceptance Criteria

- [ ] Create a worktree per run or use the existing repository workspace; reuse create_daemon_workflow_worktree and common launch assembly behind launch_daemon_workflow_agent.
- [ ] Extract a narrow shared agent launch seam; keep workflow owner/reached-activation checks in its existing wrapper, without inventing workflow rows or bypass flags for automations.
- [ ] Recheck the definition and destination before launch; use the configured agent run config and exact literal prompt, no session reuse or separate unattended permission flag.
- [ ] Persist worktree/task/session ids after dispatch and dispatch failure after errors; boot the scheduler next to WorkflowRuntime::spawn in both desktop and tuic-remote.
- [ ] Cover reservation-before-spawn, disabled/deleted-between-reservation-and-dispatch, worktree failure, spawn failure and task binding with bounded integration fixtures.

## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## Work Log

