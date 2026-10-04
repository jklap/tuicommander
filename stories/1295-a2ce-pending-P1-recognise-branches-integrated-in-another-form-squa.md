---
id: 1295-a2ce
title: Recognise branches integrated in another form (squash, rebase, reimplementation)
status: pending
priority: P1
type: feature
created: "2026-09-30T07:20:07.079Z"
updated: "2026-10-04T15:13:16.350Z"
dependencies: []
started_at: "2026-09-30T17:03:36.552Z"
---

# Recognise branches integrated in another form (squash, rebase, reimplementation)

## Problem Statement

TUIC knows a branch is integrated only by ancestry or patch-id. Squash merges, rebases with edits and reimplementations leave 55 branches (tuicommander 36, mdkb 6, wiz 13) reported as unmerged although most are in main, so branch_delete refuses and every cleanup needs manual analysis. Case study and proposed rule set: branch-audit report in ~/Gits/.tmp/results/branch-audit-*.md.

## Acceptance Criteria

- [x] RED: squash_merged_branch_is_integrated - catches: a branch landed through a squash commit (subjects in its body) is reported unmerged
- [x] RED: noop_merge_branch_is_integrated - catches: a branch whose merge into main changes nothing is reported unmerged
- [x] RED: branch_with_unique_lines_is_not_integrated - catches: a partly landed branch with unique changes is reported integrated and deleted
- [x] Integration status reports the proof (ancestor, patch_equivalence, noop_merge, squash_message, content_superset) and branch_delete accepts every proof except content-based ones without an archive ref
- [x] The rule set and thresholds come from the branch-audit case study; every rule has a fixture from a real case
- [ ] MCP is the source of truth (Boss, 2026-09-30): repo MCP exposes the integration status and proof of every branch and worktree (list and single-branch query); branch_delete, the UI and orchestrator tools (tuic-land-cleanup.sh, wt-sweep.sh) use that answer instead of their own git checks
- [x] GREEN: targeted integration, branch deletion, UI consumer and GitHub proof tests pass (final.log: 39/39)
- [x] RED: diff_header_like_unique_lines_are_not_ignored_1295 - catches: a unique ++ line is mistaken for a diff header and reports false integration
- [x] RED: archive_hook_cannot_remove_content_proof_recovery_ref_1295 - catches: an archive hook invalidates the recovery ref but the content-based branch is deleted
## Proof

- [ ] [completeness] Completeness
- [x] [feature-availability] Feature availability (Native MCP handler query/list/delete fixture passes in final.log; advertised actions wired in repo dispatch. Installed process requires restart documented in to-test.md.)
- [x] [robustness] Robustness (Real Git attacks: unique lines, diff-like prefixes, duplicates, binary, deletion, missing newline, literal paths, stale archive, protected/current/worktree branches; final.log 39 passed.)
- [x] [resilience] Resilience (Archive-hook invalidation retains branch; PR error and stale-head fixtures pass; unknown classification never authorizes branch_delete.)
- [x] [security] Security (Validated branch names and existing MCP path guards; no shell interpolation; current/default/worktree checks and exact-tip archive requirement remain.)
- [x] [defense-in-depth] Defense in depth (Fresh backend proof at deletion plus current/default/worktree guard, content archive gate, post-hook archive recheck, and branch-tip compare-and-delete.)
- [x] [input-validation] Input validation (Existing MCP missing/nonexistent branch/path and branch deletion protected/current/worktree fixtures pass; query validates local branch names and requires branch.)
- [x] [thread-safety] Thread safety (Stateless query/classification on immutable captured tip; existing branch compare-and-delete and archive-hook advancement test passed in consumers.log （2/2）. Concurrent branch advancement retains ref. No new global mutable state.)
- [~] [configurability] Configurability (No configuration introduced: fixed proof rules and archive contract are shared backend behavior.)

## Work Log

### 2026-09-30T08:31:50.058Z - Reality contract: real Git histories replay audit squash/revised-twin/unique-WIP cases. Consumer sees lifecycle proof and ref deletion or preservation. Existing classify_branch_merge drives lifecycle/UI and deletion; unification must preserve dirty/worktree/CAS guards. Structural oracle: merge-tree tree equals default tree; subjects alone never prove integration. Content requires all added lines, no deletion/mode/binary change, exact-tip archive for deletion.

### 2026-09-30T08:31:51.697Z - Plan: unify classification in tuic-git; prove squash/noop/unique fixtures RED then GREEN; expose branch_integrations and branch_integration MCP; retain archive/CAS guards; attack content and misleading messages; sync docs; commit and handoff. Reality contract: observable lifecycle verdict plus real ref preservation/deletion with real Git fixture histories.

### 2026-09-30T10:26:36.145Z - Final assigned-scope validation: final.log, 39 targeted tests passed in 92.371s (no full-suite claim). RED1 squash/noop Unmerged; RED2 unknown MCP action/content Unmerged; RED3 UI merged list empty and ++unique line falsely integrated; RED4 GitHub bullets reported noop rather than squash; RED5 archive hook deleted recovery ref without retaining branch; RED6 exact same-subject twin after refactor unrecognised. GREEN covers all. Ordinary unique-line fixture already passed baseline; adversarial ++unique variant supplies the unsafe-content RED. Integration fixture provenance and conservative 100% threshold documented. Criterion 6 orchestrator consumers remain owned by parent; new MCP calls are ready. Rust deployment/restart check remains in to-test.md. Batch coverage/CRAP/mutation deferred to coordinator. Simplicity: classifier shared across query/lifecycle/UI/delete; adapter wrappers preserve GitHub authority; no dependency/config/cache added.


### 2026-09-30T10:26:40.657Z - Proof feature-availability set PROVEN: Native MCP handler query/list/delete fixture passes in final.log; advertised actions wired in repo dispatch. Installed process requires restart documented in to-test.md.

### 2026-09-30T10:26:40.960Z - Proof robustness set PROVEN: Real Git attacks: unique lines, diff-like prefixes, duplicates, binary, deletion, missing newline, literal paths, stale archive, protected/current/worktree branches; final.log 39 passed.

### 2026-09-30T10:26:41.331Z - Proof resilience set PROVEN: Archive-hook invalidation retains branch; PR error and stale-head fixtures pass; unknown classification never authorizes branch_delete.

### 2026-09-30T10:26:41.644Z - Proof security set PROVEN: Validated branch names and existing MCP path guards; no shell interpolation; current/default/worktree checks and exact-tip archive requirement remain.

### 2026-09-30T10:26:41.956Z - Proof defense-in-depth set PROVEN: Fresh backend proof at deletion plus current/default/worktree guard, content archive gate, post-hook archive recheck, and branch-tip compare-and-delete.

### 2026-09-30T10:26:43.347Z - Proof input-validation set PROVEN: Existing MCP missing/nonexistent branch/path and branch deletion protected/current/worktree fixtures pass; query validates local branch names and requires branch.

### 2026-09-30T10:26:46.667Z - Proof configurability set NOT_APPLICABLE: No configuration is introduced; strict proof rules are shared backend contracts.

### 2026-09-30T10:28:45.331Z - Proof thread-safety set PROVEN: Stateless query/classification on immutable captured tip; existing branch compare-and-delete and archive-hook advancement test passed in consumers.log (2/2). Concurrent branch advancement retains ref. No new global mutable state.

### 2026-09-30T10:28:45.539Z - Proof configurability set NOT_APPLICABLE: No configuration introduced: fixed proof rules and archive contract are shared backend behavior.

### 2026-09-30T10:28:45.770Z - Additional direct consumers: consumers.log 2/2 passed in 10.410s: shootout_branches (gix/CLI parity), archive_that_advances_branch_keeps_its_new_commit (CAS retains advanced branch). Completeness remains UNPROVEN until parent updates orchestrator cleanup tools for criterion 6 and batch gates run. Native macOS evidence only; Windows/Linux not executed.

### 2026-09-30T10:30:02.973Z - Correction to previous worklog duration: consumers.log reports 3.506s, not 10.410s. The two named consumer tests both passed; final.log remains 39 tests in 92.371s.

### 2026-09-30T10:31:20.985Z - Committed assigned scope as 45695bead7f559c553e9db09eb25738bb9be8830 on feat/1295-integration-proofs. fmt-changed.sh worktree main...HEAD and git diff --check pass; working tree clean. Non-closing reference because parent-owned orchestrator consumer updates and batch gates remain. No push.

### 2026-09-30T13:16:42.142Z - Live 2026-09-30: repo action=branch_integration denied by auto-mode classifier; branch_integrations timed out (IPC read timeout) on the tuicommander repo. Criterion 6 unverified; no orchestrator diff.

### 2026-09-30T17:03:36.785Z - Contract: consumer (MCP repo branch_integrations) gets every branch's verdict within the client timeout; live 2026-09-30 the list timed out on tuicommander (40 local branches). Measured: one gh GraphQL lookup 0.76s, called sequentially per unmerged branch, plus per-branch git work ~5s/40 branches for merge-tree+cherry+log alone. Proven layer: branch_integrations_with_pr classifies sequentially. Fix: 6 scoped worker threads, listing order preserved. Assumption: gh/GitHub tolerate 6 concurrent lookups. Classification logic unchanged. Criterion 6 orchestrator tools (tuic-land-cleanup.sh, wt-sweep.sh) outside this repo: parent-owned.

### 2026-10-03T09:58:16.360Z - SKIPPED lane c-workflows: last open criterion 6 explicitly requires orchestrator consumer migration (tuic-land-cleanup.sh and wt-sweep.sh) outside the assigned worktree, already recorded as parent-owned. Backend proof/classifier work is present; no authorization to mutate the external orchestrator repository. Criterion 6 stays open; no source change or test run.

### 2026-10-04T10:38:36.042Z - Wave 2 audit: existing domain classification, query/list MCP and safe branch deletion are present; no implementation or extra Git subprocess on file-change events was added. Last criterion 6 still requires parent-owned external orchestrator tools tuic-land-cleanup.sh and wt-sweep.sh; managed-child contract limits writes to assigned worktree. That remaining consumer migration is skipped here for missing external ownership, as in the prior lane handoff. 1489 separately corrects the deletion response archive ref.

### 2026-10-04T15:13:15.986Z - Status reset to pending by the coordinator on 2026-10-04: no live agent; last state: Wave 2 audit: existing domain classification, query/list MCP and safe branch deletion are present; no implementation or extra Git subprocess on file-change events was added.

