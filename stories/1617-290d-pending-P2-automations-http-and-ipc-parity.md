---
id: 1617-290d
title: "Automations: HTTP and IPC parity"
status: pending
priority: P2
type: feature
created: "2026-10-09T16:14:58.217Z"
updated: "2026-10-09T16:15:29.664Z"
dependencies: ["1611-fa3f", "1613-67f3", "1615-57cf", "1616-1882"]
plan: plans/automations-scheduler.md
plan_step: Step 8
depends_on: ["stories/1611-fa3f-pending-P2-automations-cron-and-timezone-semantics.md", "stories/1613-67f3-pending-P2-automations-scheduler-admission-and-catch-up.md", "stories/1615-57cf-pending-P2-automations-dispatch-and-shared-runtime-boot.md", "stories/1616-1882-pending-P2-automations-completion-and-maximum-duration.md"]
---

# Automations: HTTP and IPC parity

## Problem Statement

Implement http and ipc parity for the approved Automations scheduler. See the linked plan for the fixed contract and exclusions.

## Acceptance Criteria

- [ ] Expose list/get/create/update/delete/run_now/list_runs, pause via update, schedule preview/presets and run aggregates through identical HTTP and Tauri shapes.
- [ ] Transport handlers call shared Rust business logic; preserve per-id definition edits and immutable run snapshots; validation errors are consistent.
- [ ] Add COMMAND_TABLE mappings and mapping assertions; regenerate command_table_paths.txt with Vitest and pass registered-route PATCH probes.
- [ ] Document HTTP, IPC, backend configuration, specification and feature availability; add targeted API error and happy-path tests.

## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## Work Log

