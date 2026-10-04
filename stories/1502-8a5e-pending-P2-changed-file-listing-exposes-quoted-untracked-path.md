---
id: 1502-8a5e
title: Changed-file listing exposes quoted untracked paths instead of literal filenames
status: pending
priority: P2
type: fix
created: "2026-10-04T11:23:53.432Z"
updated: "2026-10-04T15:13:52.911Z"
dependencies: []
started_at: "2026-10-04T11:27:12.069Z"
---

# Changed-file listing exposes quoted untracked paths instead of literal filenames

## Problem Statement

Critic real-Git regression changed_files_return_literal_untracked_control_path_critic_1499 fails on Linux rb at fix/wave2-git 3a0a73076, crates/tuic-git/src/git.rs:2366: returned path is the C-quoted display spelling instead of the literal tab/newline filename. get_changed_files_blocking uses ls-files --others --exclude-standard without -z, then lines/trim; opening that returned path targets a nonexistent file and yields zero counts. Defect predates 1499; no production changes made by critic. Evidence: ~/Gits/.tmp/critic-w2-git/verify-retry.log lines 69-72. Parent owns the fix; smallest remedy is NUL-delimited ls-files and literal record handling, no new mechanism.

## Acceptance Criteria

- [x] Public changed-file listing returns the exact literal path and two additions for a real untracked file containing tab/newline on Unix
- [x] Untracked paths with leading or trailing spaces are preserved and remain usable by file-diff consumers
- [x] Ordinary untracked inclusion and staged/committed scopes remain intact

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

### 2026-10-04T11:27:11.864Z - Contract: Changed-file consumers receive exact untracked filenames and real line counts from get_changed_files_blocking. State comes from live git ls-files and filesystem contents; Git NUL records preserve whitespace/control characters. Existing critic regression covers tab/newline; add real-Git boundary-space and scope regression. No String encoding contract change. Pre-fix 3a0a730768f10438312ad679c925b55e2a41099e; critic already reproduced control-path failure on rb.

### 2026-10-04T11:29:42.304Z - RESULT: fix/wave2-git HEAD 6cdb8a945cb7a9adc075f26efa456e3b2a443247, clean. Untracked ls-files uses -z and nonempty literal records without trim. Critic test unchanged; added changed_files_preserve_untracked_boundary_spaces_and_scopes_1502 (Unix boundary spaces, ordinary inclusion, actual file diff, staged/committed isolation). Risk medium: Git output parser consumed by IPC/HTTP. Pre-fix 3a0a730768f10438312ad679c925b55e2a41099e; critic already observed changed_files_return_literal_untracked_control_path_critic_1499 failure on rb. One native Mac run: scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml -p tuic-git -p tuicommander --no-default-features --lib -E 'test(/_1437|_1450|_1451|_1488|_1499|_1502|critic_1499|branch_delete|tip_archived_at_the_suffixed_ref/)' through mbx/build-slot/tuic-bg. 21 passed, 0 failed, 5079 excluded by selection; build 20.47s, tests 2.253s; log ~/Gits/.tmp/w2-git-fix/targeted.log. Includes MCP branch_delete_reports_the_suffixed_archive_holding_the_tip_1489 PASS. No full suite, Windows or desktop claim. fmt-changed main...HEAD, diff-check, instruction links passed. Rust restart tracked in to-test.md; parent owns landing and story completion.

### 2026-10-04T15:13:50.402Z - Status reset to pending by the coordinator on 2026-10-04: no live agent; last state: RESULT: fix/wave2-git HEAD 6cdb8a945cb7a9adc075f26efa456e3b2a443247, clean.

