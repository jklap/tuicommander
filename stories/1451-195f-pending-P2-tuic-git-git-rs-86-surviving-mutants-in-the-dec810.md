---
id: 1451-195f
title: "tuic-git git.rs: 86 surviving mutants in the dec81053c..565171e36 diff"
status: pending
priority: P2
type: task
created: "2026-10-03T16:31:38.250Z"
updated: "2026-10-04T15:13:30.119Z"
dependencies: []
started_at: "2026-10-04T10:35:04.225Z"
---

# tuic-git git.rs: 86 surviving mutants in the dec81053c..565171e36 diff

## Problem Statement

Mutation gate (gate-tuic-2, pin 217efe8ea, in-diff dec81053c..565171e36, cargo-mutants via nextest, 3 shards of src-tauri/crates/tuic-git/src/git.rs): 355 mutants tested, 234 caught, 86 missed, 0 timeout, 35 unviable. Registry runs b6db69a5 c62a04f1 ba0b38f6 (imported, no metadata = historical-only). Each survivor is a missing assertion in tuic-git git.rs tests. Survivors (line:col mutation, lines in git.rs at the pin):
243:29: delete ! in get_repo_info_impl
311:5: replace rename_branch_impl -> Result<(), String> with Ok(())
322:45: replace || with && in rename_branch_impl
480:23: replace match guard !hash.is_empty() with true in diff_base_args
723:13: delete match arm Some(b'\\') in parse_diff_to_changes
707:26: replace += with *= in parse_diff_to_changes
711:30: replace > with < in parse_diff_to_changes
783:5: replace get_initials -> String with String::new()
783:5: replace get_initials -> String with "xyzzy".into()
797:5: replace check_is_main_branch -> bool with true
797:5: replace check_is_main_branch -> bool with false
852:13: replace || with && in detect_default_branch
857:13: replace || with && in detect_default_branch
899:5: replace packed_ref_exists -> bool with false
1008:28: replace < with > in get_branches_detail_impl
1016:38: replace || with && in get_branches_detail_impl
1100:5: replace apply_base_ahead_behind_and_sort with ()
1121:9: delete match arm (true, false) in apply_base_ahead_behind_and_sort
1122:9: delete match arm (false, true) in apply_base_ahead_behind_and_sort
1163:35: replace && with || in get_recent_branches_impl
1248:21: replace += with -= in status_counts_from_porcelain_v2
1248:21: replace += with *= in status_counts_from_porcelain_v2
1308:30: delete ! in get_git_panel_context_impl
1319:32: replace == with != in get_git_panel_context_impl
1343:28: replace == with != in get_git_panel_context_impl
1357:46: replace || with && in get_git_panel_context_impl
1474:9: delete match arm 'T' in status_char_to_code
1478:9: delete match arm 'C' in status_char_to_code
1492:21: replace < with > in parse_ordinary_entry
1529:21: replace < with > in parse_rename_entry
1565:21: replace < with > in parse_unmerged_entry
1579:5: replace parse_numstat -> HashMap<String, (u32, u32)> with HashMap::new()
1579:5: replace parse_numstat -> HashMap<String, (u32, u32)> with HashMap::from_iter([(String::new(), (0, 0))]
1579:5: replace parse_numstat -> HashMap<String, (u32, u32)> with HashMap::from_iter([(String::new(), (0, 1))]
1579:5: replace parse_numstat -> HashMap<String, (u32, u32)> with HashMap::from_iter([(String::new(), (1, 0))]
1579:5: replace parse_numstat -> HashMap<String, (u32, u32)> with HashMap::from_iter([(String::new(), (1, 1))]
1579:5: replace parse_numstat -> HashMap<String, (u32, u32)> with HashMap::from_iter([("xyzzy".into(), (0, 0))
1579:5: replace parse_numstat -> HashMap<String, (u32, u32)> with HashMap::from_iter([("xyzzy".into(), (0, 1))
1579:5: replace parse_numstat -> HashMap<String, (u32, u32)> with HashMap::from_iter([("xyzzy".into(), (1, 0))
1579:5: replace parse_numstat -> HashMap<String, (u32, u32)> with HashMap::from_iter([("xyzzy".into(), (1, 1))
1582:24: replace != with == in parse_numstat
1595:5: replace enrich_with_numstat with ()
1778:35: replace += with *= in validate_paths_within_repo
1787:35: replace -= with += in validate_paths_within_repo
1787:35: replace -= with /= in validate_paths_within_repo
1810:5: replace diagnose_git_failure -> String with String::new()
1810:5: replace diagnose_git_failure -> String with "xyzzy".into()
2005:9: replace || with && in validate_stash_ref
2107:13: replace && with || in parse_blame_porcelain
2106:13: replace && with || in parse_blame_porcelain
4877:5: replace get_remote_url_blocking -> Result<Option<String>, String> with Ok(None)
4877:5: replace get_remote_url_blocking -> Result<Option<String>, String> with Ok(Some(String::new()))
4877:5: replace get_remote_url_blocking -> Result<Option<String>, String> with Ok(Some("xyzzy".into()))
4884:5: replace get_branch_base_blocking -> Result<Option<String>, String> with Ok(None)
4884:5: replace get_branch_base_blocking -> Result<Option<String>, String> with Ok(Some(String::new()))
4884:5: replace get_branch_base_blocking -> Result<Option<String>, String> with Ok(Some("xyzzy".into()))
4891:5: replace get_recent_commits_blocking -> Result<Vec<RecentCommit>, String> with Ok(vec![])
4905:32: replace == with != in get_recent_commits_blocking
4922:5: replace get_git_diff_blocking -> Result<String, String> with Ok(String::new())
4922:5: replace get_git_diff_blocking -> Result<String, String> with Ok("xyzzy".into())
4946:5: replace get_changed_files_blocking -> Result<Vec<ChangedFile>, String> with Ok(vec![])
4973:28: replace >= with < in get_changed_files_blocking
4981:36: replace && with || in get_changed_files_blocking
4981:29: replace == with != in get_changed_files_blocking
4981:48: replace == with != in get_changed_files_blocking
4988:28: replace >= with < in get_changed_files_blocking
5037:52: replace += with -= in get_changed_files_blocking
5037:52: replace += with *= in get_changed_files_blocking
5074:32: replace && with || in get_file_diff_blocking
5107:56: delete - in get_file_diff_blocking
5108:25: replace > with < in get_file_diff_blocking
5148:5: replace get_git_branches_blocking -> Result<Vec<serde_json::Value>, String> with Ok(vec![])
5148:5: replace get_git_branches_blocking -> Result<Vec<serde_json::Value>, String> with Ok(vec![Default::defa
5159:28: delete ! in get_git_branches_blocking
5163:72: replace == with != in get_git_branches_blocking
5184:5: replace get_recent_branches_blocking -> Result<Vec<String>, String> with Ok(vec![])
5184:5: replace get_recent_branches_blocking -> Result<Vec<String>, String> with Ok(vec![String::new()])
5184:5: replace get_recent_branches_blocking -> Result<Vec<String>, String> with Ok(vec!["xyzzy".into()])
5252:50: delete ! in git_apply_reverse_patch_blocking
5339:5: replace get_stash_list_blocking -> Result<Vec<StashEntry>, String> with Ok(vec![])
5359:32: replace != with == in get_stash_list_blocking
5382:5: replace git_stash_apply_blocking -> Result<(), String> with Ok(())
5396:5: replace git_stash_pop_blocking -> Result<(), String> with Ok(())
5410:5: replace git_stash_drop_blocking -> Result<(), String> with Ok(())
5422:5: replace git_stash_show_blocking -> Result<String, String> with Ok(String::new())
5422:5: replace git_stash_show_blocking -> Result<String, String> with Ok("xyzzy".into())

## Acceptance Criteria

- [ ] RED: for each survivor, a test that fails with the mutation applied — catches: the mutation listed in the problem text
- [ ] GREEN: rerun cargo-mutants --in-diff on git.rs, survivors reduced to zero or each remaining one skipped with #[mutants::skip] and a reason

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

### 2026-10-04T10:35:05.066Z - Contract: blocking Git read adapters return actual repository commits, branch flags and working-tree diffs, including per-file line counts. Real disposable repositories supply Git output, not handwritten fixtures. New boundary regression exercises the consumer-facing adapters together; existing parser and safety tests remain relevant. Run targeted tests once at lane end.

### 2026-10-04T11:01:11.469Z - Wave 2 handoff: real blocking read adapter regression passed in targeted lane rerun 18/18 on 69ec40db6; first run exposed pre-existing changed-file counts defect, fixed under 1499-3a34. Historical 86-mutant inventory was not exhaustively proved. Remaining regression authorship and per-mutant/mutation acceptance stay open.

### 2026-10-04T15:13:28.256Z - Status reset to pending by the coordinator on 2026-10-04: no live agent; last state: Wave 2 handoff: real blocking read adapter regression passed in targeted lane rerun 18/18 on 69ec40db6; first run exposed pre-existing changed-file counts defect, fixed under 1499-3a34.

