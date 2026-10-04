---
id: 1520-46b1
title: Fix secret-form client deadlines and diagnose missing native window
status: in_progress
priority: P1
type: fix
created: "2026-10-04T15:00:23.086Z"
updated: "2026-10-04T15:16:41.752Z"
dependencies: []
started_at: "2026-10-04T15:16:41.700Z"
---

# Fix secret-form client deadlines and diagnose missing native window

## Problem Statement

Coordinator reproduced twice 2026-10-04 around 17:15 on desktop started 14:59 including 568aa1424..6e8f736e2: Claude MCP secret request timed out; CLI secret request timed out after 3 seconds; only main native window was listed and no secret logs existed. Source proves forms::ask waits 300 seconds; CLI defaults 3 seconds and bridge 10 seconds. run can wait another 120 seconds for the child. Missing native window remains unconfirmed; desktop restart and keychain access are excluded.

## Acceptance Criteria

- [ ] CLI secret request/run wait beyond backend deadlines; remove retains ordinary deadline.
- [ ] Bridge direct and call_tool-wrapped secret request/run wait beyond backend deadlines; unrelated calls retain ordinary deadline.
- [ ] Safe stage logs distinguish dispatch, open attempt, native success/error and completion without logging values or capabilities.
- [ ] After Boss restarts desktop, confirm native form opens and returns names/status on submit/decline; establish missing-window cause.

## Proof

- [ ] [completeness] Completeness
- [ ] [feature-availability] Feature availability
- [ ] [robustness] Robustness
- [ ] [resilience] Resilience
- [ ] [security] Security
- [ ] [defense-in-depth] Defense in depth
- [ ] [input-validation] Input validation
- [ ] [thread-safety] Thread safety
- [ ] [configurability] Configurability

## Work Log

### 2026-10-04T15:01:00.727Z - Contract: secret request remains connected until the desktop form submits, declines, closes or reaches its 300-second deadline. secret run additionally permits the existing 120-second child execution. SecretStore owns pending state and values; neither client receives values or the nonce. Both CLI and bridge (including collapsed call_tool) must allow the backend deadline plus transport margin. No external system fixture is assumed: tests use the TUIC-owned IPC protocol. Missing-window cause is unknown; stage-only tracing must expose whether dispatch, host lookup, open or native construction failed. Native desktop verification is deferred to Boss restart, with no second app or keychain access. Existing pending/store tests protect nonce, lifetime and inspection; new tests name premature transport expiry. Risk: medium, shared IPC timing and security-sensitive form lifecycle.

### 2026-10-04T15:05:02.305Z - Implementation preserves blocking submit/decline contract. Shared tuic-ipc deadlines now drive both clients and backend form/child bounds; request=305s, run=425s. Added CLI socket regression secret_request_accepts_user_entry_after_four_seconds and CLI/bridge secret_calls_do_not_expire_before_form_and_child_deadlines, including collapsed calls and short remove. Added safe stage logs and restart observations. Native-window absence is still unproven: no speculative window threading change. Instruction links pass (2 pairs); no tests run yet. Final validation will target these client tests, existing transport deadline test and secrets tests only, plus touched-crate Clippy.

### 2026-10-04T15:13:59.776Z - Status reset to pending by the coordinator on 2026-10-04: no live agent; last state: Implementation preserves blocking submit/decline contract.

### 2026-10-04T15:16:40.186Z - Coordinator: status restored to in_progress; tuic-stale-status reset it by mistake while tuic-secret-window owns it.

