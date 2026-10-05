---
id: 1491-2ae4
title: Desktop TUIC spawns ~26 git processes per second while agents work in 11 worktrees
status: pending
priority: P1
type: fix
created: "2026-10-04T09:36:42.995Z"
updated: "2026-10-04T15:13:42.328Z"
dependencies: []
started_at: "2026-10-04T10:26:30.489Z"
---

# Desktop TUIC spawns ~26 git processes per second while agents work in 11 worktrees

## Problem Statement

Measured 2026-10-04 11:40 (load 32, 7 agents in tuicommander worktrees): sampling new processes for 25 s attributed 652 git spawns to the tuicommander desktop process (~26/s). Most frequent command lines in a 20 s sample: git cherry SHA SHA (55), git status --porcelain --untracked-files=all --ignore-submodules=none (48), git submodule status --recursive (24), git diff --shortstat (11), git merge-tree --write-tree (9), rev-list --merges (8), worktree list (6). Every exec is scanned by Microsoft Defender (wdavdaemon ~290% CPU at the time), and the terminal stream timed out waiting for its first frame (15 s). Unknown: which watcher/poller triggers the recomputation (repo_watcher.rs, lifecycle status), and whether file changes in worktrees re-run the full lifecycle set for every worktree.

## Acceptance Criteria

- [x] Identify the trigger and rate of each git command family with a reproducible measurement (worktrees with active writers)
- [ ] With 11 worktrees and agents writing files, desktop git spawns stay under an agreed bound (target: <= 2/s sustained) measured the same way
- [x] Merged/integrated detection (cherry, merge-tree, rev-list) runs only when a ref moves, not on working-tree file changes
- [x] status --untracked-files=all is not re-run for a worktree while a previous run for it is in flight (coalesced)
- [x] No regression in sidebar dirty/merged badges (Critic rb tests at bced18208 reproduce stale PR merged badge without local ref movement and porcelain dirty count regression after git rm --cached. See critic worklog.)

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

### 2026-10-04T10:26:30.252Z - Contract: sidebar dirty counts follow each checkout index and files; merged classification follows repository refs, not file saves. Removal preflight must keep fresh status, fingerprint and submodule checks. Working-tree repo-changed currently invalidates all caches and get_repo_diff_stats_impl calls full lifecycle per workspace (two porcelain reads plus submodule and integration processes). Use existing gix read adapter for monitoring and ref-keyed single-flight classification; preserve operational inspection. Verify repeated writes across 11 real linked worktrees with process-family counts, ref movement, dirty badges and concurrent reads. No tests run before final changes.

### 2026-10-04T10:38:48.630Z - Verified cause: working-tree invalidation runs full lifecycle preflight per checkout and uncached integration classification per branch. Monitoring now uses distinct gix dirty paths, ref/config/reflog-keyed integration classification and existing Moka single-flight for repository stats. Operational lifecycle/removal remains fresh. Final targeted native tests launched; initialized-submodule process bound remains to verify.

### 2026-10-04T11:01:01.142Z - Native measurement: 11 real linked worktrees, 3 active-write refreshes each; old lifecycle emitted branch=33 cherry=66 diff=33 log=33 ls-tree=66 merge-base=99 merge-tree=33 reflog=33 rev-list=66 rev-parse=297 show=66 status=66 submodule=33 symbolic-ref=33 worktree=33 (1089 total); new warmed monitoring plus diff and merged reads emitted zero. Library filter monitoring_gitpoll OR get_merged_branches: 8 passed in 27.954s. Application filter monitoring_gitpoll OR repo_structure_and_diff_stats_share_one_worktree_paths_read: 2 passed in 0.285s after correcting two test field-name compilation errors. Existing frontend per-repository in-flight coordinator was already present. Criterion 2 remains open: desktop sampling with initialized submodules is unavailable to this peer and CLI status/diff fallbacks can still exceed the target. No desktop launched. Coordinator critic required (medium risk: shared caches/concurrency).

### 2026-10-04T11:05:58.998Z - Critic contract: sidebar must recover a merged PR badge after an upstream proof lookup failure without a local ref change; porcelain dirty counts remain unchanged when staged removal leaves a file untracked; operational removal must reject newly dirty initialized submodules after a cached clean monitoring read. State comes from real Git repositories; PR callback failure/recovery is exercised at the existing proof-provider boundary, not with invented GitHub response fixtures. No production edits; one remote crate-suite and clippy run after all tests.

### 2026-10-04T12:05:37.032Z - Critic RESULT: bced18208 adds three tests only. rb Linux nextest -p tuic-git -p tuicommander --no-default-features attempted full suites: 393/5179 run, 391 pass, 2 fail, 16 ignored; 4786 not run due fail-fast. PR-provider recovery fails worktree.rs:8067 Unmerged vs Merged; cached index removal fails :8096 Some(1) vs Some(2). Fresh submodule dirty detection/removal rejection passes. Clippy both packages --no-default-features --all-targets -D warnings fails git_cli.rs:18 missing_const_for_thread_local in implementer counter. Log ~/Gits/.tmp/critic-w2-gitpoll/remote-suite.log. Criterion 5 rejected; criterion 2 still owed desktop measurement. No production edit, no native Mac run, no push/merge.

### 2026-10-04T12:10:11.528Z - Repair contract: cache only ref-dependent local integration and its tip; reapply current PR proof at the existing repository stats refresh boundary without rerunning local integration on file saves. Preserve porcelain record counts: staged/unstaged tracked paths deduplicate, untracked rows remain separate. Use the unchanged critic recovery, cached-removal and submodule tests; strengthen the existing app invalidation test against empty workspace maps. No tests before final changes. One rb workspace nextest no-fail-fast and clippy run; desktop process-rate criterion stays with coordinator.

### 2026-10-04T12:30:11.834Z - Fix 0719f0e6b: current PR proof overlays ref-keyed local classification; separate untracked rows preserve cached-removal count; stateless dirty read and const command counter; app invalidation asserts fixture cardinality. rb workspace nextest --no-default-features --no-fail-fast: 7101 passed, 25 skipped, 243.100s test phase; all critic cases green. Clippy workspace excluding bm25 failed solely type_complexity on new cache value; named the value type in follow-up commit, no behavior change, retry only clippy. Log ~/Gits/.tmp/w2-gitpoll-fix/rb-validation-v3.log; rb collection ~/Gits/.tmp/rb/20261004T121415Z-11130. Two earlier launch failures ran no Cargo (rsync PATH; duplicated rb wrapper).

### 2026-10-04T12:33:14.286Z - Final HEAD ec83b9842: rb workspace Clippy --exclude bm25 --no-default-features --all-targets -- -D warnings PASSED (exit 0; 1m50s), log ~/Gits/.tmp/w2-gitpoll-fix/rb-clippy-retry.log. Prior 0719f0e6b workspace nextest remains valid: follow-up only aliases the exact cache value type, no behavior/config/dependency change. 7101 passed, 25 skipped, no failing tests; tuic-git included and all three unchanged critic regressions plus app invalidation passed. Criteria 1/3/4/5 checked; criterion 2 remains open for desktop measurement. Clean branch fix/wave2-gitpoll, no push/merge/desktop launch. Final fmt-changed main...HEAD and diff check passed. Coordinator owns final combined-tree overlap review and story completion.

### 2026-10-04T12:33:56.643Z - Timing correction: final Clippy reported 1m18s (not 1m50s). Final rb collection ~/Gits/.tmp/rb/20261004T123023Z-23658. No test failures.

### 2026-10-04T15:13:41.113Z - Status reset to pending by the coordinator on 2026-10-04: no live agent; last state: Timing correction: final Clippy reported 1m18s (not 1m50s).

