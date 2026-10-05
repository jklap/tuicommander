---
id: 1499-3a34
title: Changed-file adapter loses additions and deletions by combining Git format flags
status: pending
priority: P2
type: fix
created: "2026-10-04T10:50:01.038Z"
updated: "2026-10-04T15:13:49.287Z"
dependencies: []
started_at: "2026-10-04T10:50:30.306Z"
---

# Changed-file adapter loses additions and deletions by combining Git format flags

## Problem Statement

Decisive real-Git reproduction: git diff --name-status --numstat returns only M TAB tracked.txt; --numstat alone returns 2 TAB 1 TAB tracked.txt. get_changed_files_blocking assumes both blocks exist, so tracked files always receive zero line counts. New boundary regression blocking_reads_expose_real_commits_branches_and_diff_counts_1451 fails at (0,0) vs (2,1), targeted.log on fix/wave2-git 70f7fbda3. Proven layer: tuic-git adapter, not parser or frontend. No external service involved.

## Acceptance Criteria

- [x] Changed-file adapter returns real additions/deletions and status for a modified tracked file
- [x] Existing untracked inclusion and staged/commit scope behavior remain intact

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

### 2026-10-04T10:50:30.749Z - Contract: consumer sees exact tracked-file status and line counts from actual Git diff output; untracked, staged and commit scopes retain their existing semantics. Real Git emitted only name-status for the old combined flags; regression already failed on the pre-fix tree 70f7fbda3. Use raw plus numstat in one existing subprocess, preserving the hot-path process count. No externally authored fixtures. Targeted rerun will cover lane and scope boundaries.

### 2026-10-04T10:58:28.916Z - Native macOS headless GREEN: targeted-retry.log at 69ec40db6, 18/18 passed in 5.837s; compile succeeded. Regression blocking_reads_expose_real_commits_branches_and_diff_counts_1451 failed on pre-fix 70f7fbda3 in targeted.log (16 pass, 1 fail), decisive CLI reproduction confirmed format suppression. changed_file_counts_preserve_scope_and_rename_paths_1499 proves staged, working-tree, committed and rename behavior. One Git subprocess retained. fmt-changed and diff-check pass; no broad-suite or cross-platform claim.

### 2026-10-04T11:04:56.605Z - Critic contract: consumers receive literal file paths, Git statuses and counts across staged, working and committed scopes. Fixtures are actual local Git repositories; fixed bytes provide independent count oracles. Live Git probe confirms untracked newline paths are C-quoted without -z. Audit only; no production edits. Add boundary tests for binary/deletion/tracked control characters and untracked control characters; classify the latter as pre-existing if it fails.

### 2026-10-04T15:13:45.403Z - Status reset to pending by the coordinator on 2026-10-04: no live agent; last state: Critic contract: consumers receive literal file paths, Git statuses and counts across staged, working and committed scopes.

