---
id: 1450-c637
title: "tuic-git worktree.rs shards 1-3: 53 survivors (worktree removal, orphan, submodule and integratio..."
status: pending
priority: P2
type: fix
created: "2026-10-03T15:11:21.858Z"
updated: "2026-10-04T15:13:26.633Z"
dependencies: []
started_at: "2026-10-04T10:29:00.187Z"
---

# tuic-git worktree.rs shards 1-3: 53 survivors (worktree removal, orphan, submodule and integratio...

## Problem Statement

cargo-mutants --in-diff dec81053c..565171e36 on crates/tuic-git/src/worktree.rs, --shard 1/4, 2/4, 3/4 (rb 2026-10-03, gate-tuic-2 B3, head 217efe8ea): shard1 113 mutants 94 caught/13 missed/6 unviable; shard2 113: 76/26/0/11; shard3 113: 82/12/2 timeout/17. 53 survivors (missed+timeout) by function: remove_worktree_internal_with_lock (10); preserve_missing_worktree_modules::visit (8); classify_branch_merge (4); context_free_patch (4); generate_worktree_name (4); delete_integrated_local_branch_with_pr (3); branch_integration_with_pr (2); generate_worktree_name_cmd (2); generate_clone_branch_name_cmd (2); get_remote_default_branch (2); fetch_if_remote (2); free_archive_dest (2); content_superset (1); registered_worktree_admin_dir (1); path_entry_exists (1); remove_worktree_by_workspace_id_with_missing_confirmation_and_pr (1); parse_worktree_entries (1); validate_worktree_path (1); worktree_dirtiness (1); run_script_in_dir (1). Full list: ~/Gits/.tmp/gate-tuic-2/w/B3.log; registry runs 0ee60a1a, 32821b9d, 5a3355e6 (canonical tuicommander registry). Many are destructive paths (orphan removal, submodule preservation): treat as higher priority than the other P2 survivors. Shard 0 is story 1437-aad1.

## Acceptance Criteria

- [ ] RED: for each function in the list, a test that fails on each listed mutant (see B3.log for the exact mutants), grouped by function
- [ ] GREEN: cargo mutants -p tuic-git -f src/worktree.rs --in-diff on the same range reports no missed mutant for these functions

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

### 2026-10-04T10:28:58.985Z - Contract: removal preserves checkout, registration and module commits on every unsafe/error state; archive collisions never overwrite prior archives; public generated names are valid nonempty collision-free names; remote fetch refreshes the requested ref using a real local bare origin. Existing real-Git tests already exercise destructive paths; new tests close specific error and wrapper gaps. No mutation runs or RED runs in this lane.

### 2026-10-04T11:01:11.007Z - Wave 2 handoff: public name collision and requested remote fetch regressions passed in targeted lane rerun 18/18 on 69ec40db6. Existing removal/orphan/submodule/integration tests were audited. Coverage is partial; no per-mutant RED or zero-survivor claim, criteria remain open for further authoring and coordinator batch validation.

### 2026-10-04T15:13:23.834Z - Status reset to pending by the coordinator on 2026-10-04: no live agent; last state: Wave 2 handoff: public name collision and requested remote fetch regressions passed in targeted lane rerun 18/18 on 69ec40db6.

