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


## Release follow-up — 2026-10-27

The failure remains **not reproducible** on production revision `a5232c9650`.
The original guard is retained on `fix/2351-stale-branch-row`; no production
behavior has changed. This is regression coverage, not a demonstrated fix.

### Watcher and backend cache

The Rust guard now creates B before watching, at the same commit as A, then
switches A → B → A → B. Each switch waits for the real watcher event and reads
the production cached structure. The first event primes the main-checkout
fingerprint. Later switches must bypass that unchanged fingerprint through
the linked admin HEAD event; creating a new ref cannot mask a missing event.
The retained A ref, A-named directory, four missing detached registrations,
current-row removal and preservation of A remain covered.

The targeted rb run passed: **1 test**, 1.715 s test time, 1m 48s compilation.
Command: `build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run
--manifest-path src-tauri/Cargo.toml --lib -E
'test(sidebar_2351_switch_with_retained_ref_and_prunable_checkouts)'`.
This is Linux watcher evidence, not native macOS FSEvents evidence.

### Refresh coordinator and sidebar store

`useGitOperations.test.ts` adds `2351 real Git replaces retained A with B`,
parameterized for active and background rows. Each case creates an actual Git
repository and linked checkout; Git, not a mock, supplies the branch inventory.
The test adapts porcelain into the frontend port shape. It does not execute
Rust through the mocked IPC transport.

Each case warms the production refresh coordinator and repository store on A,
attaches a terminal at that checkout, then delays an A inventory response.
Git switches to the already-existing B while A remains. A queued refresh reads
B after the stale response completes. Assertions require A to disappear, B to
carry the correct branch and path, the terminal to survive without closing,
and active selection to follow only when the replaced row was active.

The initial targeted local Vitest run passed **2 tests** in 3.30 s. Its Git
commands capture stdout and stderr. The test adapter normalizes Git's path
separators for the host; Windows execution has not been performed.
After this test-only path normalization, the final targeted run passed both
cases in 6.21 s; Biome also passed for the changed TypeScript file.
Command: `build-slot.sh scripts/with-test-tmp.sh pnpm vitest run
src/__tests__/hooks/useGitOperations.test.ts -t '2351 real Git'`.

Combined first-run log: `~/Gits/.tmp/tuic-2351-close/targeted.log`.
Final frontend log: `~/Gits/.tmp/tuic-2351-close/frontend-final.log`.
Dependency setup used `pnpm install --offline --frozen-lockfile`; no dependency
or lockfile changed.

### Limits and release disposition

The retained-ref scenario passes at the Git inventory/removal, real Linux
watcher/cache, frontend refresh-coordinator and sidebar-store boundaries.
The two test layers are not a live end-to-end desktop test. No agent-test
instance or daily application was launched. Native macOS event delivery,
Roberto's running build, persisted workspace paths/keys and failing-instance
refresh errors remain unverified. The original RED acceptance criterion must
remain unchecked: no test failed for the reported defect. Do not present these
passing guards as proof that Roberto's symptom has been fixed.


## Full-topology native replay — 2026-10-27

This section supersedes the earlier scope and native-verification limits.
The story was reopened because the earlier checks did not replay all the Git
registrations. The full topology has now been replayed in backend tests,
frontend tests, and the isolated native macOS application. The reported stale
A row and `No workspace found for id` error still did not occur. This evidence
does not prove that the failure on Roberto's machine is fixed.

### Source and fixture fidelity

Source: the verbatim porcelain, ref listing and admin-directory listing in
`~/Gits/.tmp/coordinator/ans-2351-raw.md`. The recorded fixture is
`src-tauri/src/fixtures/worktrees/2351-roberto-topology.json`. It contains all
19 registrations (the main checkout and 18 linked checkouts), not just the
checkout being switched. It preserves:

- `poc-00001-intercept-apps` and the nested `__wt` registration
  `poc-00001-intercept-apps__wt/tmp-review236-r1`, on `tmp/review236-r1`.
- `poc-0001-endpoint-risk-wip-20261019`, its exact branch name, and the lock
  reason `User requested endpoint-risk WIP preservation and later rework`.
- All six external temporary-path registrations, including the live detached
  `agent2-reactive-backoff-20261022/old` and
  `windows-location-qa-20261023/replay`, plus the four missing registrations.
- Directory/branch mismatches for `poc-0001-striker`,
  `poc-00001-mcp-optimize`, and `poc-00001-slack-flap`.
- All detached rows, including the three `8c104ea` names with one shared HEAD,
  and the two missing story-review registrations with one shared HEAD.
- The ordinary old A ref, at a different synthetic commit from B, after the
  target checkout switches to `POC-00003/endpoint-capability-audit`.

The live comparison of every registration against the source fixture passed:
`topology-comparison.json` records `all_19_entries_match: true`. The comparison
checks paths, branch names, HEAD groups, lock reason and prunable flags.
Git admin names are generated by Git from the original directory basenames.

Only the absolute roots and object contents are synthesized. `work/agent2`
represents Roberto's repository, `work/agent2__wt` its sibling storage, and
`private/tmp` the external checkouts. All three are below the disposable
`~/Gits/.tmp/tuic-2351-topology/live` root, outside the repository as applicable;
no files were created in the real `/private/tmp` or Roberto's home directory.
Original SHAs are mapped to distinct empty synthetic commits. Equal source
SHAs map to equal synthetic SHAs; actual source commit contents and ancestry
are not supplied by the porcelain and are not recreated. Consequently the
synthetic branches are clean/integrated under TUIC's tree-equivalence rules.

### Backend and frontend checks

The existing Rust guard now constructs the full fixture with real Git before
warming discovery and watching A → B → A → B. It requires all eight named
workspace paths, seven live detached rows and four diagnostics, then removes
B and verifies the old A ref survives. Targeted rb Nextest: **1 passed**, 2.079 s
test time, 5m 12s compilation.

Both frontend cases now use `build2351Topology` from
`src/__tests__/helpers/worktree2351.ts`. Real porcelain supplies all branch,
detached and missing rows to the production coordinator/store. Active and
background variants retain the delayed-A/queued-B race. They require every
source branch/path, seven detached rows, four diagnostics, and terminal/active
selection preservation. Targeted local Vitest: **2 passed**, 4.68 s total.
The local build-slot queue waited 440 s; this was queue time, not a test hang.
Commands use the same targeted filters as above. Log:
`~/Gits/.tmp/tuic-2351-topology/tests.log`.

### Native application check

The native build used `make agent-test AGENT_TEST_INSTANCE=agent-test-2351
AGENT_TEST_PORT=19891` from the linked worktree after a dry run. It ran the
current worktree production code, not the daily/orchestrator application.
Health identified `2.0.0-beta.3`, `aarch64-apple-darwin`, binary SHA-256
`dcba829d1f10aa0dbf14a2196de455e2097efd3454b913f941e7e091a4d4e9dd`.
All requests used this instance's Unix socket. No daily configuration,
credentials, global shortcuts or microphone were used.

1. Create the live fixture through the same TypeScript helper (Node's native
   TypeScript runner), register the scratch repository through its own MCP
   endpoint, and reload its WebView to run normal persisted-repository startup.
2. Confirm native DOM rows and backend structure while the target is on A:
   eight named rows, seven detached rows, four repair diagnostics, all exact
   named branches and the nested registration. Capture `warm-native.png`.
3. Arm a DOM mutation observer, then run real `git switch` in the target
   checkout. With no manual refresh or post-switch reload, the native watcher,
   cache and frontend replace A with B. The observer records success after
   1075 ms from arming (not a performance bound). Capture `switched-native.png`.
4. Click that row's actual `Remove worktree` control through native WebView
   DOM interaction. Default integrated-worktree policy asks `Archive workspace?`
   for B. Confirm `Archive`. The row disappears and the status reads exactly
   `Archived POC-00003/endpoint-capability-audit worktree for reuse; branch was kept`.
   Backend logs record `remove_worktree SUCCESS` addressed by B and the
   original `poc-00001-slack-flap` checkout path.
5. Move the archived scratch checkout back with real Git. Set this scratch
   repository's `delete_branch_on_remove=false` through real load/modify/save
   settings IPC, preserving its baseline. Click the native row control again.
   It asks `Remove workspace?`, with `The local branch will be kept.` Confirm
   `Remove`. The status reads exactly `Removed POC-00003/endpoint-capability-audit`.
6. Verify the original checkout directory is gone, both A and B refs remain,
   the locked and nested checkouts remain, main remains, all four missing
   registrations remain diagnostics, and neither A nor B is a sidebar row.
   Native logs show `git worktree remove: OK` and `remove_worktree command: SUCCESS`.
7. Stop only this instance with `make agent-test-stop` and verify its supervisor
   PID file is gone. The child exits 143 after the requested SIGTERM; Make
   reports that requested stop as exit 2, not a startup or verification failure.

**Error text:** the reported `No workspace found for id` did not occur.
Both native row operations completed successfully. Dialogs and status text are
retained in the JSON evidence, not inferred from backend-only calls. Window-only
screenshots target agent-test window 26188; no whole-screen capture was taken.

Harness corrections: an inspection script initially omitted `return`; parsing
one captured result initially failed because normal console output precedes the
JSON after `---`; the first direct settings call omitted the required entry
`path` and was rejected before persistence; an attempted `Remove` confirmation
then correctly found only `Archive`. These were automation errors, not a
reproduction of the workspace identity failure. The settings call was corrected,
its saved value was read back, and the literal Remove flow then passed.

Evidence directory: `~/Gits/.tmp/tuic-2351-topology/`. Key artifacts:
`topology-comparison.json`, `switched-porcelain.txt`, `warm-structure.json`,
`switched-structure.json`, `switch-ui.json`, `removal-dialog.json`,
`removal-outcome.json`, `literal-remove-confirmed.json`, `final-ui.json`,
`final-git-checks.json`, `final-git-logs.json`, `final-porcelain.txt`,
`native.log`, and the window-only PNGs.

### What still differs from Roberto's machine

- His running TUIC version and build identity remain unknown; this replay uses
  the current worktree build identified above.
- His persisted workspace map keys/paths, terminal placement, settings and
  refresh errors remain unknown. This instance starts clean, warms on A and
  then switches; it does not import his saved application state.
- Absolute path roots are relocated under Gits; this does not test `/private/tmp`
  storage-specific permissions, mounts, symlinks, cleanup timing or FSEvents
  behavior on that original path.
- Only the Git topology and HEAD equality groups are known. Source commit
  contents/ancestry, real working-tree dirtiness, submodule state and temporal
  event history are not established by the supplied listing.

### Final evidence validation

After adding the helper's existing-checkout safety check, the final command was:

```sh
build-slot.sh bash -lc 'pnpm exec biome check src/__tests__/helpers/worktree2351.ts src/__tests__/hooks/useGitOperations.test.ts; git diff --check; scripts/with-test-tmp.sh pnpm vitest run src/__tests__/hooks/useGitOperations.test.ts -t "2351 real Git"'
```

The actual shell used `set -e`. Biome and diff checks passed. Both selected tests
passed in 4.64 s total; other tests were excluded by the filter. Log:
`~/Gits/.tmp/tuic-2351-topology/frontend-final-3.log`. Normal application debug
messages were present on stdout; no error output was reported. The Rust test
source and fixture did not change after the successful rb run above.

Two earlier ad hoc standalone TypeScript checks failed before reaching Vitest:
TS5112 required `--ignoreConfig` for explicit input files, then TS2688 could not
resolve the explicitly requested Node type library. These were not successful
type checks. No dependency was added, and no full frontend type-check pass is
claimed. Logs: `frontend-final.log` and `frontend-final-2.log` in the same evidence
directory. Instruction-link validation also passed for this worktree.

The story-close check refused completion with:
`fix story 2351-1a0b has no checked RED: criterion`.
This is an unmet defect criterion, not a reason to relabel passing evidence as RED.

No production patch is justified by the passing replay. Keep the unresolved
reported defect distinct from these passing regression guards; do not close it
as fixed or claim a RED test that did not fail.
