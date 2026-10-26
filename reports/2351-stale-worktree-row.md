# Stale worktree row investigation — 2351-1a0b

## Status

The reported failure is not reproduced on the assigned branch. No production fix
is proposed. The new real-Git regression passes before any production change.
Do not close the story or describe this as RED/GREEN evidence.

## Field evidence

The coordinator supplied Roberto's raw `git worktree list --porcelain` output.
The checkout `agent2__wt/poc-00001-slack-flap` holds
`POC-00003/endpoint-capability-audit`; the ordinary local ref
`POC-00001-slack-flap` remains at `9f22c71fdcd9f17d5ab0d19260c17cc5828e57a5`.
It is not a symbolic ref, as already confirmed in the 2334 report.

The raw output has **four**, not five, prunable entries. All four are detached.
Other live detached checkouts and a locked, unrelated branch also appear. The
nested checkout `poc-00001-intercept-apps__wt/tmp-review236-r1` is registered under
the same common repository. None of this supplies the stale row's persisted
`workspaceId`, `branchName`, or `worktreePath`, or Roberto's running TUIC version.

## Source boundary

- `GitReads.worktree_inventory` reads each checkout's current admin HEAD, not
  the basename of its checkout directory or the list of local branch refs.
- `get_repo_structure_impl` serves the cached inventory.
- `repo_watcher` marks linked admin HEAD changes as shared Git changes, bypasses
  the main-checkout fingerprint deduplication, and invalidates that inventory.
- `createRepositoryRefreshCoordinator` replaces an old workspace with the new
  inventory workspace at the same path, transfers its terminals, and drops the
  old key. Diagnostics are separate from selectable workspaces.
- `resolve_removable_workspace` uses fresh inventory. An old branch ref with no
  checkout is correctly not a removable workspace; it must not be retargeted to
  the checkout now on another branch.

Fix `2d29b4712` is an ancestor of the assigned pre-change commit
`3da5747e14`. It fixed a distinct symbolic-ref inconsistency, not the confirmed
ordinary branch ref in Roberto's report.

## New real-Git check

`sidebar_2351_switch_with_retained_ref_and_prunable_checkouts`:

1. Creates branch A in a directory named for A and four detached checkouts.
2. Deletes the four detached directories, retaining their Git registrations.
3. Confirms Git itself reports four prunable registrations.
4. Warms the production cached structure while the live checkout is on A.
5. Starts the real repository watcher, switches the checkout to B, and keeps A.
6. Waits for the production event bus and reads the production cached structure.
7. Asserts B replaces A, all four missing registrations remain diagnostics,
   and fresh removal lookup refuses A.
8. Removes B through `remove_worktree_by_workspace_id`, keeping branch refs,
   and confirms the checkout is gone while A remains.

This passes on unchanged production code. It rules out the retained ref plus
missing-detached-registration combination in this rb environment. It does not
prove Roberto's persisted state or native macOS event delivery.

## Commands and evidence

- First targeted rb attempt: compilation failed because a crate-local
  `#[cfg(test)]` helper is unavailable to the root test binary. No tests ran.
  Replaced that import with the existing `git_cmd` adapter.
  Log: `~/Gits/.tmp/tuic-2351-evidence/repro-rust.log`.
- Targeted rb command: `build-slot.sh --remote scripts/with-test-tmp.sh cargo
  nextest run --manifest-path src-tauri/Cargo.toml --lib -E
  'test(sidebar_2351_switch_with_retained_ref_and_prunable_checkouts)'`.
  Result: **1 passed**, test 0.776 s; build 2m 57s.
  Log: `~/Gits/.tmp/tuic-2351-evidence/repro-rust-2.log`.
- Local prerequisite: `pnpm install --offline --frozen-lockfile` passed without
  dependency changes.
- Targeted existing Vitest checks cover background branch replacement,
  main-checkout replacement with and without terminals, stale-row pruning, and
  diagnostic publication/retraction. **5 passed** in 2.33 s.
  Log: `~/Gits/.tmp/tuic-2351-evidence/frontend.log`.
  These use mocked transport data, not a real desktop or real Rust backend.

No full suite, native desktop, Windows test, mutation run, or daily-instance
access was performed. No source change outside the added test is required.
Risk of this test-only change is low; a proposed lifecycle/removal fix would need
independent review because it can affect live sessions and destructive Git work.

## Required next evidence

From the failing instance, collect its version/build identity and the repository's
saved workspace records for A and B, including their map keys and paths. Also
collect `get_repo_structure` for this repository and the refresh error log around
the failure. Exclude saved terminal prompts and other unrelated user data.

Those observations distinguish stale frontend state from backend discovery
failure or an older build. Do not infer one from the Git listing alone.

**Workaround to try (unverified):** Reload the TUICommander window to request a
fresh sidebar discovery; do not delete branch A or remove B's checkout merely
to clear the stale label. No verified safe Git command that clears this UI row
has been established.
