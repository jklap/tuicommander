---
id: "2006-f5a1"
title: "Create managed children through one idempotent launch transaction"
status: pending
priority: P1
type: feature
created: 2026-10-17T16:09:52.942Z
updated: 2026-10-17T16:09:52.942Z
dependencies: []
---

# Create managed children through one idempotent launch transaction

## Problem Statement

Candidate 3. Source: ~/Gits/personal/orchestrator/reports/tuicommander/2026-10-17-mcp-usage-audit.md, section 5. Measured repository evidence: tuic-spawn has "79 spawn-script commits" in the inspected 30 days, which is maintenance churn, not 79 MCP defects. Commit 342883a4 stops deterministic worktree/spawn errors; the wrapper checks for successful creation after EAGAIN, pins names, attaches parents, holds children open and checks prompt delivery. Candidate evidence also names 450b97d6 and 7a31194f. Existing stable-name and idle-close stories cover individual primitives, not an idempotent composed launch.

## Acceptance Criteria

- [ ] RED: lost_launch_reply_reuses_the_same_child - catches: retrying one idempotency key after partial creation spawning a second PTY or falling back to another checkout
- [ ] One request selects an existing or new worktree, stable name, parent and hold-open policy; receipt returns child identity and a durable launch handle.
- [ ] Repeating the same key after a lost reply returns the same child; conflicting key reuse and deterministic refusal are explicit and do not fall through to another checkout.
- [ ] Receipt distinguishes created, prompt-delivered, ready and blocked states using authoritative state, not terminal movement alone; warm-artifact readiness remains explicit.
- [ ] Fixture-test failure after worktree creation, after child creation and during prompt delivery with deterministic clocks and fault injection, without model sleeping or polling.
- [ ] Keep model routing, account selection and local brief policy outside TUIC; preserve existing name and keep-open behavior.

## Related

- 1730-dd5a
- 1209-cc47
- 1240-8438

## Work Log
