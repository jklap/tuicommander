---
id: 1489-1a14
title: branch_delete response names the primary archive ref when the tip sits at the suffixed ref
status: pending
priority: P3
type: fix
created: "2026-10-04T08:26:43.251Z"
updated: "2026-10-04T15:13:39.609Z"
dependencies: []
started_at: "2026-10-04T10:35:49.547Z"
---

# branch_delete response names the primary archive ref when the tip sits at the suffixed ref

## Problem Statement

After 1462, a force delete can archive the tip at refs/archive/<b>-<sha7>. mcp_transport.rs:4205 still reports the primary refs/archive/<b> in the branch_delete response, because delete_integrated_local_branch returns only the proof string (tuic-sec-fix2 RESULT, ~/Gits/.tmp/results/tuic-sec-fix2-1791102363.md). A user restoring the branch from the reported ref gets the wrong commit.

## Acceptance Criteria

- [ ] RED: branch_delete test with an occupied primary archive ref — catches: the response naming refs/archive/<b> while the tip is at refs/archive/<b>-<sha7>
- [x] GREEN: the response names the ref that holds the deleted tip

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

### 2026-10-04T10:35:49.907Z - Contract: after archived branch deletion, MCP returns the exact durable ref holding the deleted tip, including a suffix when the primary holds older work. The deletion operation resolves this ref before deleting the branch; transport only renders its result. Real Git-created refs are the test fixture. Return a typed deletion outcome through the existing adapter, preserving the public JSON shape. Risk medium: destructive CAS deletion and shared refs; no new watcher or background Git work.

### 2026-10-04T10:58:28.667Z - GREEN native macOS headless evidence: targeted-retry.log at 69ec40db6, 18/18 passed including branch_delete_reports_the_suffixed_archive_holding_the_tip_1489 and archived/stale/protected/integration consumers. Regression pre-fix source commit fbc553366; no RED execution per BUILD POLICY. Exact archive captured before deletion, primary preserved. Coordinator critic remains required (medium risk). Rust restart item is in to-test.md.

### 2026-10-04T15:13:39.006Z - Status reset to pending by the coordinator on 2026-10-04: no live agent; last state: GREEN native macOS headless evidence: targeted-retry.log at 69ec40db6, 18/18 passed including branch_delete_reports_the_suffixed_archive_holding_the_tip_1489 and archived/stale/protected/integration consumers.

