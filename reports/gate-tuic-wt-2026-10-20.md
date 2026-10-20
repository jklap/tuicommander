# Worktree mutation gate: six backlog units

Date: 2026-10-20. Source: `a778245a871c15e65b83061d88a4ff8bfdf60cfc`, branch `chore/gate-tuic-wt`.

## Result

Completed on rb (Linux aarch64): 120 mutants, 84 caught, 28 missed, 8 unviable, 0 timeouts. Mutation kill rate: 84 / (84 + 28) = 75.0%. No equivalent-mutant exclusions were applied. Exit 2 means survivors remain. No numeric tier threshold was supplied in this assignment; this is not a converged gate.

Baseline: 254 worktree tests passed in tuic-git; 359 tests were excluded by the runtime filter. This does not claim a full workspace pass or HTTP/IPC consumer coverage. Baseline build 17.42 s, test 6.81 s. cargo-mutants reports 20 minutes for the mutation run.

## Per-unit function supersets

Functions have changed since the original story. The scopes below include complete current functions, not just original changed lines. Shared functions count in more than one row, so do not sum the rows. A survivor in a shared function is not evidence that each listed story introduced a defect.

| Unit | Tested | Caught | Missed | Unviable | Timeout | Kill rate |
|---|---:|---:|---:|---:|---:|---:|
| 1001-fc30 | 54 | 36 | 11 | 7 | 0 | 76.6% |
| 992-d46d | 13 | 8 | 4 | 1 | 0 | 66.7% |
| 994-69fe | 34 | 22 | 11 | 1 | 0 | 66.7% |
| 995-d43e | 28 | 20 | 8 | 0 | 0 | 71.4% |
| 996-f44b | 20 | 12 | 8 | 0 | 0 | 60.0% |
| 997-672c | 17 | 13 | 4 | 0 | 0 | 76.5% |

## Function and survivor mapping

### 1001-fc30

Scope: `resolve_missing_registered_workspace`, `remove_worktree_internal_with_lock`, `remove_worktree_by_workspace_id_with_missing_confirmation_and_pr`.

- `crates/tuic-git/src/worktree.rs:2402:12: delete ! in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2408:20: replace != with == in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2444:16: replace match guard stderr.contains("not a working tree") || stderr.contains("No such file") with false in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2453:16: replace match guard stderr.contains("locked working tree") || stderr.contains("cannot remove a locked") with true in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2466:16: replace match guard stderr.contains("is a main working tree") with true in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2466:16: replace match guard stderr.contains("is a main working tree") with false in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2444:54: replace || with && in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2454:17: replace || with && in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2482:17: replace && with || in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2482:20: delete ! in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2781:51: replace != with == in remove_worktree_by_workspace_id_with_missing_confirmation_and_pr`

### 992-d46d

Scope: `archive_worktree`, `repair_archived_submodules`, `move_registered_checkout`.

- `crates/tuic-git/src/worktree.rs:4211:23: replace match guard error.kind() == std::io::ErrorKind::NotFound with true in archive_worktree`
- `crates/tuic-git/src/worktree.rs:4225:35: replace match guard error.kind() == std::io::ErrorKind::NotFound with true in archive_worktree`
- `crates/tuic-git/src/worktree.rs:4225:35: replace match guard error.kind() == std::io::ErrorKind::NotFound with false in archive_worktree`
- `crates/tuic-git/src/worktree.rs:4225:48: replace == with != in archive_worktree`

### 994-69fe

Scope: `remove_worktree_internal_with_lock`, `validate_worktree_path`, `orphan_cleanup_safety`.

- `crates/tuic-git/src/worktree.rs:2402:12: delete ! in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2408:20: replace != with == in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2444:16: replace match guard stderr.contains("not a working tree") || stderr.contains("No such file") with false in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2453:16: replace match guard stderr.contains("locked working tree") || stderr.contains("cannot remove a locked") with true in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2466:16: replace match guard stderr.contains("is a main working tree") with true in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2466:16: replace match guard stderr.contains("is a main working tree") with false in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2444:54: replace || with && in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2454:17: replace || with && in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2482:17: replace && with || in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:2482:20: delete ! in remove_worktree_internal_with_lock`
- `crates/tuic-git/src/worktree.rs:3661:24: replace || with && in validate_worktree_path`

### 995-d43e

Scope: `initialized_main_submodule`, `preserve_submodule_refs`, `preserve_missing_worktree_modules`, `preserve_missing_worktree_modules::visit`.

- `crates/tuic-git/src/worktree.rs:2092:51: replace && with || in preserve_missing_worktree_modules::visit`
- `crates/tuic-git/src/worktree.rs:2208:17: replace || with && in preserve_missing_worktree_modules::visit`
- `crates/tuic-git/src/worktree.rs:2207:17: replace || with && in preserve_missing_worktree_modules::visit`
- `crates/tuic-git/src/worktree.rs:2206:17: replace || with && in preserve_missing_worktree_modules::visit`
- `crates/tuic-git/src/worktree.rs:2213:48: replace == with != in preserve_missing_worktree_modules::visit`
- `crates/tuic-git/src/worktree.rs:2213:77: replace || with && in preserve_missing_worktree_modules::visit`
- `crates/tuic-git/src/worktree.rs:2222:29: replace || with && in preserve_missing_worktree_modules::visit`
- `crates/tuic-git/src/worktree.rs:2224:16: delete ! in preserve_missing_worktree_modules::visit`

### 996-f44b

Scope: `preserve_submodule_refs`, `preserve_missing_worktree_modules`, `preserve_missing_worktree_modules::visit`.

- `crates/tuic-git/src/worktree.rs:2092:51: replace && with || in preserve_missing_worktree_modules::visit`
- `crates/tuic-git/src/worktree.rs:2208:17: replace || with && in preserve_missing_worktree_modules::visit`
- `crates/tuic-git/src/worktree.rs:2207:17: replace || with && in preserve_missing_worktree_modules::visit`
- `crates/tuic-git/src/worktree.rs:2206:17: replace || with && in preserve_missing_worktree_modules::visit`
- `crates/tuic-git/src/worktree.rs:2213:48: replace == with != in preserve_missing_worktree_modules::visit`
- `crates/tuic-git/src/worktree.rs:2213:77: replace || with && in preserve_missing_worktree_modules::visit`
- `crates/tuic-git/src/worktree.rs:2222:29: replace || with && in preserve_missing_worktree_modules::visit`
- `crates/tuic-git/src/worktree.rs:2224:16: delete ! in preserve_missing_worktree_modules::visit`

### 997-672c

Scope: `submodule_admin_dir_at`, `verify_submodules_at`.

- `crates/tuic-git/src/worktree.rs:762:53: replace || with && in verify_submodules_at`
- `crates/tuic-git/src/worktree.rs:762:25: replace || with && in verify_submodules_at`
- `crates/tuic-git/src/worktree.rs:762:45: delete ! in verify_submodules_at`
- `crates/tuic-git/src/worktree.rs:762:79: replace != with == in verify_submodules_at`

## Interpretation and limits

- 995: all eight initialized-main-submodule mutants were caught. Its mapped survivors are in the shared missing-module traversal, not that destination guard.
- 997: all five submodule-admin-name resolver mutants were caught; four survivors are in verify_submodules_at at line 762.
- 996: preserve_submodule_refs and the outer preserve_missing_worktree_modules replacements were caught. Eight survivors remain in the nested missing-module traversal. Mutation operators do not prove that every literal Git argument, including --reflog, was mutated.
- 992: archive repair function replacements were caught. Four allocation-marker error/rollback mutations survived.
- 994: orphan_cleanup_safety replacements were caught. One attached/detached predicate mutation survived; ten other survivors are in the shared removal function, outside the original detached-head guard.
- No tests were written and no production code changed. Surviving mutations need review before they can be called defects or equivalents.

## Execution and evidence

Command (from the clean worktree, through tuic-bg.sh):

```sh
TMPDIR="$HOME/Gits/.tmp/tuic-tests/" BUILD_SLOT_AGENT=gate-tuic-wt \
  /Users/stefano.straus/Gits/personal/orchestrator/tools/build/build-slot.sh --remote --gate \
  cargo mutants --manifest-path src-tauri/Cargo.toml --workspace --in-place \
  -f 'crates/tuic-git/src/worktree.rs' \
  --re '(resolve_missing_registered_workspace|remove_worktree_internal_with_lock|remove_worktree_by_workspace_id_with_missing_confirmation_and_pr|archive_worktree|repair_archived_submodules|move_registered_checkout|validate_worktree_path|orphan_cleanup_safety|initialized_main_submodule|preserve_submodule_refs|preserve_missing_worktree_modules|submodule_admin_dir_at|verify_submodules_at| in visit)' \
  --test-tool nextest --timeout 180 -- -E 'test(worktree)'
```

The repository config supplies --lib. The tool selected tuic-git as the mutated package and ran its worktree-filtered tests.

- Initial shared-main attempt: exit 78, modified local macOS sidecar. No --force and no native build were used.
- First clean-worktree attempt: duplicate --lib in nextest arguments. Compilation passed; test invocation failed before any mutant. Removed the redundant forwarded --lib.
- Next attempt: baseline passed; rb gate preempted, exit 75. Retried once.
- Final run: /Users/stefano.straus/Gits/.tmp/rb/20261020T115430Z-79314.
- Log: /Users/stefano.straus/Gits/.tmp/gate-tuic-wt/run3.log.
- Registry import: run 1a50aa94cc7fe2c1150cb521ddd645c24f7a2bda629894d76dacb80dde34429a, complete, 121 outcomes including baseline.
- rb reported only mutants.out.old/lock.json as a changed remote artifact and cleaned two leftover job processes. Local source stayed clean.
- TUIC archived the new clean branch while the first run was active; subsequent runs used its actual __archived checkout path.
- No GUI, browser, native Mac build, or frontend gate was run.

## Recording gaps

The six requested story IDs are absent from the current main story store. Each worklog write and each story-close.sh --check was attempted and refused (story not found). Do not recreate or complete those stories from this report. All six completed mutation results are recorded with tested.sh. The gate queue is not edited; the proposed diff below is for the coordinator.

## Proposed gate-queue.md diff

```diff
--- a/gate-queue.md
+++ b/gate-queue.md
@@ -36,15 +36,15 @@
 | ego | close-ego story 150-28ea (Seatbelt probe timeout 2 s → 30 s) | master 8a27bdf..ad248ed; ego-tools/src/exec/probe.rs T2 | T2 | paused (Boss 2026-09-27: ego changes under the coordinator plan) | workspace suite and clippy green at ad248ed; mutate the deadline and the timeout-to-refusal mapping |
 | mdkb | close-mdkb story 159-31ea (cosine rank of recall neighbors before the cap) | main d238ad9..e417ac1; src/mcp/dispatch.rs T2 | T2 | running gate-mdkb | gate-mdkb2 snapshot predates it: next chunk; mutate the rank order, the zero-vector None and the collection identity |
 | Fleet | fl-1 story 005-2d28 (Nu save/mkdir/rm through host-fs-write, in-memory overlay) | no Git: component SHA-256 537c7184…; host grants, overlay and effect bounds | T2 | deferred | Boss 2026-09-27: after his direct Fleet task is finished and validated; gate-fleet snapshot predates it; guard mutants need a per-mutant component rebuild (see nu-blade note) |
-| tuicommander | tuic-backlog 1001-fc30 (force for missing checkout removal) | fix/1001-missing-worktree-force 0c948e386..89a299c1e; tuic-git/src/worktree.rs T2 | T2 | pending | follow-up defect 1051-a82a |
-| tuicommander | tuic-backlog 992-d46d (archive refuses locked checkouts, repairs worktree and submodule gitfiles) | fix/992-archive-worktree-state 089757453..e36a9bc57; tuic-git/src/worktree.rs T2; src/worktree.rs tests only | T2 | pending | mutate lock refusal, submodule core.worktree repair and rollback |
+| tuicommander | tuic-backlog 1001-fc30 (force for missing checkout removal) | fix/1001-missing-worktree-force 0c948e386..89a299c1e; tuic-git/src/worktree.rs T2 | T2 | partial (2026-10-20 gate-tuic-wt) | follow-up defect 1051-a82a; a778245a8: 54 tested, 36 caught/11 missed/7 unviable, 0 timeout (76.6%); overlapping function superset; story worklog missing; reports/gate-tuic-wt-2026-10-20.md |
+| tuicommander | tuic-backlog 992-d46d (archive refuses locked checkouts, repairs worktree and submodule gitfiles) | fix/992-archive-worktree-state 089757453..e36a9bc57; tuic-git/src/worktree.rs T2; src/worktree.rs tests only | T2 | partial (2026-10-20 gate-tuic-wt) | mutate lock refusal, submodule core.worktree repair and rollback; a778245a8: 13 tested, 8 caught/4 missed/1 unviable, 0 timeout (66.7%); overlapping function superset; story worklog missing; reports/gate-tuic-wt-2026-10-20.md |
 | mdkb | close-mdkb story 166-f183 (hook-generated prompts not mined as corrections) | main e417ac1..02a023bfa; T2 domain/prior_detect.rs, domain/prior_distill.rs, mcp/dispatch.rs | T2 | pending | with the 159 row; mutate the synthetic-prompt filter and the distill matcher rejection |
-| tuicommander | tuic-backlog 994-69fe (orphan removal protects detached commits) | fix/994-orphan-detached-commits 4974e660f..4a6517e21; tuic-git/src/worktree.rs T2 | T2 | pending | destructive path: mutate the attached, in-progress and unreachable-HEAD refusals |
-| tuicommander | tuic-backlog 995-d43e (submodule preservation verifies its destination module) | fix/995-submodule-preservation-destination 9d7aba9d8..9ebcd9325; tuic-git/src/worktree.rs T2 | T2 | pending | mutate the initialized-root and distinct-gitdir refusals |
+| tuicommander | tuic-backlog 994-69fe (orphan removal protects detached commits) | fix/994-orphan-detached-commits 4974e660f..4a6517e21; tuic-git/src/worktree.rs T2 | T2 | partial (2026-10-20 gate-tuic-wt) | destructive path: mutate the attached, in-progress and unreachable-HEAD refusals; a778245a8: 34 tested, 22 caught/11 missed/1 unviable, 0 timeout (66.7%); overlapping function superset; story worklog missing; reports/gate-tuic-wt-2026-10-20.md |
+| tuicommander | tuic-backlog 995-d43e (submodule preservation verifies its destination module) | fix/995-submodule-preservation-destination 9d7aba9d8..9ebcd9325; tuic-git/src/worktree.rs T2 | T2 | partial (2026-10-20 gate-tuic-wt) | mutate the initialized-root and distinct-gitdir refusals; a778245a8: 28 tested, 20 caught/8 missed/0 unviable, 0 timeout (71.4%); overlapping function superset; story worklog missing; reports/gate-tuic-wt-2026-10-20.md |
 | Fleet | fl-1 story 006-c4b6 (dry run served from the real Nu component via nu-lab-host --json) | no Git: component SHA-256 c1320ae7…; Fleet server dry-run endpoint and nu-lab-host JSON output | T2 | deferred | Boss 2026-09-27: after his direct Fleet task is validated; mutate the 503-on-missing-artifact path (no partial-evaluator fallback) and the trace/effect bounds |
 | mdkb | close-mdkb story 167-c322 (error-triggered priors via PostToolUseFailure) | main 02a023bfa..56726ae17; T2 domain/prior_distill.rs, cli/setup.rs; T1 store/priors.rs, mcp/dispatch.rs | T2 | pending | mutate error_contains matching, blank/pre-tool selector rejection and the hook registration |
-| tuicommander | tuic-backlog 996-f44b (removal preserves submodule stash history and reflog-only commits) | fix/996-submodule-reflog-preservation 1c4fcfe0d..382ef4eb2; tuic-git/src/worktree.rs T2 | T2 | pending | mutate the reflog-tip fetch and the bundle reflog inclusion |
-| tuicommander | tuic-backlog 997-672c (deinitialized submodule admin found by module name) | fix/997-submodule-admin-name 83618af68..95b9d67a9; tuic-git/src/worktree.rs T2 | T2 | pending | mutate the .gitmodules name-to-path lookup and the malformed-component refusal |
+| tuicommander | tuic-backlog 996-f44b (removal preserves submodule stash history and reflog-only commits) | fix/996-submodule-reflog-preservation 1c4fcfe0d..382ef4eb2; tuic-git/src/worktree.rs T2 | T2 | partial (2026-10-20 gate-tuic-wt) | mutate the reflog-tip fetch and the bundle reflog inclusion; a778245a8: 20 tested, 12 caught/8 missed/0 unviable, 0 timeout (60.0%); overlapping function superset; story worklog missing; reports/gate-tuic-wt-2026-10-20.md |
+| tuicommander | tuic-backlog 997-672c (deinitialized submodule admin found by module name) | fix/997-submodule-admin-name 83618af68..95b9d67a9; tuic-git/src/worktree.rs T2 | T2 | partial (2026-10-20 gate-tuic-wt) | mutate the .gitmodules name-to-path lookup and the malformed-component refusal; a778245a8: 17 tested, 13 caught/4 missed/0 unviable, 0 timeout (76.5%); overlapping function superset; story worklog missing; reports/gate-tuic-wt-2026-10-20.md |
 | wiz-agents | 018 1286f1d8 (test runner per-file completion), 042 a6153f0a (OTLP 413 drain), 051 7d953aa5 (local-date token buckets) | fix/hud-test-reliability 6809e762..7d953aa5 | T2 | pending | after the 048/028/050 row; not yet on the integration branch |
 | tuicommander | tuic-backlog 993-d404 (long soft-wrapped intents) | fix/993-long-soft-wrapped-intents 8479cbe9f..0919b48ba; T2 src/pty.rs; T1 tuic-terminal/terminal_grid.rs | T2 | pending | mutate the 2048-char soft-wrap budget, chrome cutoff and capped-scroll anchor; commit used TUIC_SKIP_FIXTURE_GATE=1 |
 | tuicommander | tuic-backlog 986-4a6b (timed-out --help probes cached unsupported; probe owns its process group / Job Object) | fix/986-screen-probe-timeout 84a8339aa..f2ca02b21; T2 src/agent.rs; T1 Cargo.toml | T2 | pending | mutate the timeout-vs-inconclusive cache split and the group kill; Windows compile/runtime unverified (to-test.md) |
```
