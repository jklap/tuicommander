---
id: 1437-aad1
title: "tuic-git worktree.rs shard 0/4: 15 survivors (stale recovery, submodule admin, merged-PR proof)"
status: pending
priority: P2
type: fix
created: "2026-10-03T13:49:04.535Z"
updated: "2026-10-04T15:13:21.653Z"
dependencies: []
started_at: "2026-10-04T10:27:03.268Z"
---

# tuic-git worktree.rs shard 0/4: 15 survivors (stale recovery, submodule admin, merged-PR proof)

## Problem Statement

cargo-mutants --in-diff dec81053c..565171e36 on crates/tuic-git/src/worktree.rs, --shard 0/4 only (rb 2026-10-03, gate-tuic-2 B2b, head 217efe8ea): 113 mutants, 80 caught, 15 missed, 18 unviable. Shards 1-3 not run yet. Survivors: 309:24: delete ! in create_worktree_internal; 322:44: replace != with == in create_worktree_internal; 390:5: replace cleanup_stale_worktree_dir -> Result<(), String> with Ok(()); 436:23: replace match guard e.starts_with(STALE_DIR_PREFIX) with true in create_worktree_with_stale_recovery; 436:23: replace match guard e.starts_with(STALE_DIR_PREFIX) with false in create_worktree_with_stale_recovery; 568:18: replace > with >= in dirty_fingerprint_at; 713:21: replace || with && in submodule_admin_dir_at; 710:21: replace || with && in submodule_admin_dir_at; 762:53: replace || with && in verify_submodules_at; 762:25: replace || with && in verify_submodules_at; 762:45: delete ! in verify_submodules_at; 762:79: replace != with == in verify_submodules_at; 827:46: replace || with && in merged_pr_proof_from_pages; 830:78: replace > with >= in merged_pr_proof_from_pages; 845:21: replace || with && in merged_pr_proof_from_pages.

## Acceptance Criteria

- [ ] RED: create_worktree_internal tests for the existing-branch and already-registered paths — catches: delete ! at 309 and != to == at 322
- [ ] RED: stale-dir recovery: cleanup_stale_worktree_dir really removes the stale dir and only errors prefixed STALE_DIR_PREFIX trigger recovery — catches: Ok(()) at 390 and the guard true/false at 436
- [ ] RED: dirty_fingerprint_at at the exact size limit — catches: > to >= at 568
- [ ] RED: submodule_admin_dir_at (710, 713) and verify_submodules_at (762) tests with each condition false independently — catches: || to && and delete !
- [ ] RED: merged_pr_proof_from_pages at the page-limit boundary and with an empty page (827, 830, 845) — catches: || to && and > to >=

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

### 2026-10-04T10:27:02.954Z - Contract: worktree creation returns a real checkout or preserves the original Git error; stale cleanup removes only unregistered non-Git directories. Submodule safety comes from real Git-created checkouts and refs, and malformed declarations never escape their admin root. Existing recovery/idempotency and removal tests cover many historical survivors. The historical fingerprint byte-limit and PR page-limit no longer exist in current code; audit current conditions instead. No new external API fixture will be invented. Targeted lane tests run only after all changes.

### 2026-10-04T11:01:10.794Z - Wave 2 handoff: targeted lane rerun passed 18/18 on 69ec40db6. Added four real-behavior safety regressions; this is partial survivor coverage, not evidence that every historical mutant is killed. Literal RED and mutation criteria remain unchecked; no recorded external GitHub pagination fixture was available.

### 2026-10-04T15:13:21.419Z - Status reset to pending by the coordinator on 2026-10-04: no live agent; last state: Wave 2 handoff: targeted lane rerun passed 18/18 on 69ec40db6.

