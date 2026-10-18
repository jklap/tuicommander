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

- [x] RED: lost_launch_reply_reuses_the_same_child - catches: retrying one idempotency key after partial creation spawning a second PTY or falling back to another checkout
- [x] One request selects an existing or new worktree, stable name, parent and hold-open policy; receipt returns child identity and a durable launch handle.
- [x] Repeating the same key after a lost reply returns the same child; conflicting key reuse and deterministic refusal are explicit and do not fall through to another checkout.
- [x] Receipt distinguishes created, prompt-delivered, ready and blocked states using authoritative state, not terminal movement alone; warm-artifact readiness remains explicit.
- [x] Fixture-test failure after worktree creation, after child creation and during prompt delivery with deterministic clocks and fault injection, without model sleeping or polling.
- [x] Keep model routing, account selection and local brief policy outside TUIC; preserve existing name and keep-open behavior.
- [x] RED: automation_and_mcp_launch_share_name_hold_and_retry_policy — catches: automation and MCP launch paths diverging on pinned name, keep-open policy and retry identity
- [x] GREEN: targeted launch transactions, automation/workflow consumers and route parity pass 23/23 on rb; local transport.test.ts passes 336/336
- [x] REALITY: effect=one keyed launch returns one child and stalled parentless initial delivery reports blocked without requeue ; state=SQLite launch journal, registered PTY, pending initial prompt and native Ready projection ; assumptions=the mute shell fixture models no composer readiness, no model acceptance is inferred ; radius=MCP HTTP IPC launch, automation and workflow run bindings
- [x] RED: launch_transaction_owned_prompt_timeout_reports_blocked_without_requeue — catches: parentless run-owned launch never arms the initial prompt watchdog (f781d0240, native failure created/queued/reason null after 31 seconds)
- [x] GREEN: native macOS aarch64 targeted launch transactions, overlapping requests, owned watchdog, lost launch reply and workflow launch pass 17/17; log ~/Gits/.tmp/2006-r2-native-green.log
## Related

- 1730-dd5a
- 1209-cc47
- 1240-8438

## Work Log

### 2026-10-17T16:13:33.718Z - Contract: the consumer sends one idempotency key, explicit parent/name/keep-open, worktree selection and spawn arguments; it observes a durable launch handle and the same child on retry. Real state comes from the launch journal, registered PTY, pending initial prompt and native agent readiness state, never output movement. Partial worktree/spawn outcomes fail closed rather than falling back. Policy for model/account/brief stays with the caller. Adjacent risks: conflicting keys, interrupted worktree allocation, interrupted PTY creation, queued prompt failure and restart with dead PTY. Targeted RED/GREEN only, as explicitly requested in this assignment.

### 2026-10-17T17:24:31.611Z - RED on pre-fix a529c83e39: lost_launch_reply_reuses_the_same_child failed with Unknown action launch, 0 passed / 1 failed on rb Linux aarch64. Log ~/Gits/.tmp/2006-red-fixed.log. Initial attempt had a test import compile error, corrected before this behavioral RED. Worst case: duplicate key delivery must reserve exactly 1 PTY identity; ambiguous interruption returns blocked and never creates replacement. Durable records persist request digest only, not prompt/env secrets.

### 2026-10-17T19:55:54.729Z - Review follow-up RED: automation_and_mcp_launch_share_name_hold_and_retry_policy failed with automation must use the same explicit hold policy as MCP. Log ~/Gits/.tmp/2006-automation-red-fixed.log; code before automation unification has keyed MCP launch but original managed_launch directly spawns. The initial fixture used an unsupported harness key and was corrected to a supported named run config before behavioral RED. Native automation/workflow run IDs now scope their launches through the same journal, without fabricating a peer parent.

### 2026-10-17T20:28:12.504Z - GREEN: final unified targeted rb Linux aarch64 nextest run passed 23/23 (7m36s build, 0.399s test execution), log ~/Gits/.tmp/2006-unified-green.log. Includes transaction faults, lost reply, concurrency CAS, native automation/workflow consumers, route parity, and existing named/unnamed spawn behavior. Local transport.test.ts passed 336/336 in 3.72s, route snapshot generated, log ~/Gits/.tmp/2006-js.log. Measurement: two legacy unkeyed spawn calls create two PTYs; two identical keyed launch calls create one PTY. Deferred timeout/retry retains one initial prompt in the queue; no duplicated delivery is claimed. Automation same-run retry returns one child, alongside one separately keyed MCP comparison child. No installed Mac runtime was replaced; no full suite. Intermediate compiler/fixture failures were diagnosed and corrected; all final targeted cases pass.

### 2026-10-17T20:29:27.039Z - Committed 1eca9c5f53350dd37f6f685fff81676fa7640614 on feat/2006-f5a1-mcp; worktree clean. fmt-changed.sh main...HEAD passed (Biome 2 files, Rust format checks), git diff --check and instruction-link validation passed. All eight acceptance criteria checked from the evidence; coordinator owns landing and completion. Orchestrator wrapper remains unchanged by request: tuic-spawn.sh should persist an attempt key and call agent launch once with parent/name/keep_open/workspace/spawn, then retain/read launch_id instead of composing worktree_create, spawn, rename, keep_open or replaying creation after EAGAIN. Rust changes require Boss to rebuild/restart when ready; installed runtime was not replaced.

### 2026-10-18T06:20:19.056Z - Round-2 GREEN: explicitly authorized native macOS aarch64 build-slot/mbx run passed 17/17 in 0.881s (3m05s build), including owned watchdog, overlapping requests, lost_launch_reply_reuses_the_same_child, workflow_launch_accepts_registered_branch_worktree and automation shared policy. Command: scripts/with-test-tmp.sh cargo nextest run -p tuicommander --no-default-features --lib -E test(launch_transaction)|test(lost_launch_reply_reuses_the_same_child)|test(workflow_launch_accepts_registered_branch_worktree), with short TMPDIR. Log ~/Gits/.tmp/2006-r2-native-green.log. All-targets Clippy is deferred by explicit coordinator authority until rb returns or the coordinator batch gate; no Clippy success claimed. Runtime requires manual rebuild/restart by Boss.

### 2026-10-18T06:20:41.846Z - Round-2 pre-fix RED evidence: committed regression at f2f5a9f4f, watchdog remained unfixed at f781d0240. Native launch_transaction_owned_prompt_timeout_reports_blocked_without_requeue failed at f781d0240 with created/queued/reason null after advancing paused Tokio clock 31 seconds (expected blocked); 0 passed / 1 failed, 0.443s test, 21m55s build, log ~/Gits/.tmp/2006-r2-native-red-hex.log. Fix commit 19e951eb2 moves only the existing watchdog outside optional parent linkage. Formatting and story-close --check MAIN 2006-f5a1 WORKTREE passed; no Clippy or rb GREEN claim. Initial rb failures and local prerequisite/compile failures remain recorded in the peer result.


### 2026-10-17T21:03:08.149Z - Round-2 contract: a run-owned parentless child with a mute startup must report blocked after the existing 30-second prompt budget, retaining exactly one queued prompt and one child. Authority is pending initial prompt state and the public owned-launch receipt. Adjacent cases: parent-linked watchdog, delivered prompt, identical retry, and overlapping keyed public requests. Pre-fix HEAD 3f8759c80.

### 2026-10-17T21:55:08.454Z - Round-2 RED blocked by rb transport, not a behavioral result. First attempt exit 74: SSH broken pipe, artifact listing failed; retry exit 69: ssh 34.253.255.16 port 22 Operation timed out, remote command not started. Logs ~/Gits/.tmp/2006-r2-red.log and 2006-r2-red-retry.log. F1 watchdog move is not applied because the required pre-fix RED cannot run. F2 grouping/hash and N1 overlapping public request regression committed; main c4d495462f merged preserving bounded-response and launch contracts. No GREEN or Clippy execution claimed.

### 2026-10-18T05:52:45.063Z - Round-2 RED on f781d0240: launch_transaction_owned_prompt_timeout_reports_blocked_without_requeue failed for the named bug after 31 seconds of paused Tokio time: status was created, prompt_delivery queued, reason null, expected blocked. Native macOS aarch64 through build-slot and mbx, explicitly authorized after rb outage; 0 passed / 1 failed, test 0.443s, build 21m55s. Log ~/Gits/.tmp/2006-r2-native-red-hex.log. Earlier local attempts failed before execution on an overlong mbx socket path (fixed by the mandated short TMPDIR) and unsupported digest LowerHex (fixed with existing hex::encode).
