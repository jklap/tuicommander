---
id: 1488-073d
title: "worktree.rs: 53 surviving mutants in the dec81053c..565171e36 diff"
status: pending
priority: P3
type: task
created: "2026-10-04T08:03:44.594Z"
updated: "2026-10-04T15:13:36.145Z"
dependencies: []
started_at: "2026-10-04T10:34:57.319Z"
---

# worktree.rs: 53 surviving mutants in the dec81053c..565171e36 diff

## Problem Statement

Mutation gate (gate-tuic-2, pin 217efe8ea, in-diff dec81053c..565171e36, cargo-mutants lib, -p tuicommander --no-default-features, src-tauri/src/worktree.rs, 4 shards): 118 mutants tested, 46 caught, 53 missed, 0 timeout, 19 unviable. worktree.rs has no cfg(windows)/cfg(feature) gating. Registry runs 42a2418d, 98f41721, d16d061d (shards 0-2) and the final shard 3/3 run (historical-only, no metadata). Registry run 16f24b0e is a preempted partial of shard 3 (baseline only) and is not a result.
Survivors:
src/worktree.rs:18:5: replace merged_github_pr_proves_tip -> bool with false
src/worktree.rs:54:8: delete ! in merged_github_pr_proves_tip
src/worktree.rs:167:70: replace > with >= in inspect_worktree_removal
src/worktree.rs:342:5: replace create_worktree -> Result<serde_json::Value, String> with Ok(Default::default())
src/worktree.rs:375:5: replace get_worktrees_dir -> String with String::new()
src/worktree.rs:375:5: replace get_worktrees_dir -> String with "xyzzy".into()
src/worktree.rs:398:5: replace remove_worktree -> Result<RemoveWorktreeOutcome, String> with Ok(Default::default())
src/worktree.rs:400:48: replace && with || in remove_worktree
src/worktree.rs:400:14: replace && with || in remove_worktree
src/worktree.rs:400:51: delete ! in remove_worktree
src/worktree.rs:405:49: delete ! in remove_worktree
src/worktree.rs:507:5: replace get_worktree_paths_cached -> HashMap<String, WorkspaceWorktree> with HashMap::new()
src/worktree.rs:567:5: replace remove_orphan_worktree -> Result<(), String> with Ok(())
src/worktree.rs:614:24: replace == with != in switch_branch_impl
src/worktree.rs:624:15: replace && with || in switch_branch_impl
src/worktree.rs:624:8: delete ! in switch_branch_impl
src/worktree.rs:624:18: delete ! in switch_branch_impl
src/worktree.rs:629:12: delete ! in switch_branch_impl
src/worktree.rs:646:9: delete ! in switch_branch_impl
src/worktree.rs:704:5: replace checkout_remote_branch -> Result<(), String> with Ok(())
src/worktree.rs:784:64: replace != with == in finalize_merged_worktree_impl_with_confirmation
src/worktree.rs:884:5: replace finalize_merged_worktree -> Result<MergeArchiveResult, String> with Ok(Default::default())
src/worktree.rs:884:31: replace && with || in finalize_merged_worktree
src/worktree.rs:1087:5: replace merge_and_archive_worktree -> Result<MergeArchiveResult, String> with Ok(Default::default())
src/worktree.rs:1087:31: replace && with || in merge_and_archive_worktree
src/worktree.rs:1112:5: replace check_worktree_dirty -> Result<bool, String> with Ok(true)
src/worktree.rs:1112:5: replace check_worktree_dirty -> Result<bool, String> with Ok(false)
src/worktree.rs:1122:5: replace get_workspace_lifecycle -> Result<WorktreeRemovalPreview, String> with Ok(Default::default())
src/worktree.rs:1139:5: replace get_worktree_paths -> Result<HashMap<String, WorkspaceWorktree>, String> with Ok(HashMap::new())
src/worktree.rs:1139:5: replace get_worktree_paths -> Result<HashMap<String, WorkspaceWorktree>, String> with Ok(HashMap::from_iter([(String::new(), Default::default())]))
src/worktree.rs:1139:5: replace get_worktree_paths -> Result<HashMap<String, WorkspaceWorktree>, String> with Ok(HashMap::from_iter([("xyzzy".into(), Default::default())]))
src/worktree.rs:1144:5: replace detect_orphan_worktrees -> Result<Vec<String>, String> with Ok(vec![])
src/worktree.rs:1144:5: replace detect_orphan_worktrees -> Result<Vec<String>, String> with Ok(vec![String::new()])
src/worktree.rs:1144:5: replace detect_orphan_worktrees -> Result<Vec<String>, String> with Ok(vec!["xyzzy".into()])
src/worktree.rs:1260:5: replace assess_orphan_cleanup -> Result<Vec<OrphanCleanupReview>, String> with Ok(vec![])
src/worktree.rs:1260:5: replace assess_orphan_cleanup -> Result<Vec<OrphanCleanupReview>, String> with Ok(vec![Default::default()])
src/worktree.rs:1279:9: replace || with && in begin_orphan_cleanup_internal
src/worktree.rs:1317:33: replace || with && in answer_orphan_cleanup_internal
src/worktree.rs:1331:5: replace begin_orphan_cleanup -> Result<(), String> with Ok(())
src/worktree.rs:1343:5: replace pending_orphan_cleanup_answer -> Option<bool> with None
src/worktree.rs:1343:5: replace pending_orphan_cleanup_answer -> Option<bool> with Some(true)
src/worktree.rs:1343:5: replace pending_orphan_cleanup_answer -> Option<bool> with Some(false)
src/worktree.rs:1364:5: replace clear_orphan_cleanup with ()
src/worktree.rs:1370:5: replace generate_worktree_name_cmd -> String with String::new()
src/worktree.rs:1370:5: replace generate_worktree_name_cmd -> String with "xyzzy".into()
src/worktree.rs:1379:5: replace generate_clone_branch_name_cmd -> String with String::new()
src/worktree.rs:1379:5: replace generate_clone_branch_name_cmd -> String with "xyzzy".into()
src/worktree.rs:1385:5: replace list_local_branches -> Result<Vec<String>, String> with Ok(vec![])
src/worktree.rs:1385:5: replace list_local_branches -> Result<Vec<String>, String> with Ok(vec![String::new()])
src/worktree.rs:1385:5: replace list_local_branches -> Result<Vec<String>, String> with Ok(vec!["xyzzy".into()])
src/worktree.rs:1391:5: replace list_base_ref_options -> Result<Vec<BaseRefOption>, String> with Ok(vec![])
src/worktree.rs:1391:5: replace list_base_ref_options -> Result<Vec<BaseRefOption>, String> with Ok(vec![Default::default()])
src/worktree.rs:1397:5: replace run_setup_script -> Result<serde_json::Value, String> with Ok(Default::default())

## Acceptance Criteria

- [ ] RED: for each survivor, a test that fails with the mutation applied — catches: the listed mutation
- [ ] GREEN: rerun cargo-mutants --in-diff on worktree.rs, survivors zero or skipped with #[mutants::skip] and a reason

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

### 2026-10-04T10:35:03.756Z - Contract: cleanup dialogs come only from registered orphan checkouts; an existing answer is immutable until the dialog is settled. Real Git is the source of checkout membership. Existing removal and confirmation tests cover shared-state guards; new tests target empty/unknown requests and repeated answers. Historical desktop command survivors are now cfg desktop and cannot be proved by the historical headless mutation run. Final targeted tests only; mutation audit remains coordinator-owned.

### 2026-10-04T11:01:11.249Z - Wave 2 handoff: empty/unknown cleanup input and answer overwrite regressions passed in targeted lane rerun 18/18 on 69ec40db6. Historical survivor inventory includes desktop-only wrappers absent from the headless build. Partial coverage delivered; remaining survivor coverage and mutation acceptance stay open.

### 2026-10-04T15:13:34.551Z - Status reset to pending by the coordinator on 2026-10-04: no live agent; last state: Wave 2 handoff: empty/unknown cleanup input and answer overwrite regressions passed in targeted lane rerun 18/18 on 69ec40db6.

