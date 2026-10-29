# Windows CI round j: directory metadata and test import budgets

## New evidence supersedes the round-i completion inference

Native Windows CI `40773627108` on `fe13ec4179` receives the events required by
the three monitoring tests. It then fails their unchanged-sibling computation
assertions on every retry. The round-i path fix was necessary, but not sufficient:
it exposed a second problem in the production cache.

Windows log: `~/Gits/.tmp/results/ci-40773627108-123117080278.txt`.
Frontend log: `~/Gits/.tmp/results/ci-40773627108-123117080036.txt`.

## Production defect

The audited state machine is `StatusCache::set_watching` → `event` → `read_with`.
Its callers are the raw repository watcher callback, watch registration/sync,
and the monitoring read facade. The common Git directory, linked admin directory
and linked-root mapping are canonical in these fixtures. The Git text spelling
mismatch from round i is not a reason to add another path-normalization layer.

`StatusCache::event` previously treated every event naming the common `.git` or
an existing checkout admin directory as watch loss. This includes
`Modify(Any)` on a directory that still exists. Calling `stop(base)` disables
reuse for every mapped sibling until watches are re-established. An unchanged
sibling therefore recomputes status on subsequent refreshes.

The Windows backend in notify 8.2.0 subscribes to directory names, attributes and
last-write changes with recursive `ReadDirectoryChangesW`. Its
`FILE_ACTION_MODIFIED` arm emits `Modify(Any)` without distinguishing file
content from directory metadata. Source revision
`a1d7c2d8f80786679d58ec6d5986a1d4278bc8cf`,
[`notify/src/windows.rs`, subscription](https://github.com/notify-rs/notify/blob/a1d7c2d8f80786679d58ec6d5986a1d4278bc8cf/notify/src/windows.rs#L284-L316)
and
[event mapping](https://github.com/notify-rs/notify/blob/a1d7c2d8f80786679d58ec6d5986a1d4278bc8cf/notify/src/windows.rs#L425-L451).
TUIC already classifies this Windows directory-metadata shape in
`fs::is_directory_metadata_event`, used by the Codex and Usage file caches.

**Observation versus inference:** the current Windows failure log records the
extra computations, not the exact raw event that caused each one. The production
state-machine defect is decisive from source and has a direct regression below.
Attributing all three native failures to this metadata path remains a hypothesis
until the next native run; no new Windows event capture is claimed.

## Fix and boundaries

Before watch-loss checks, ignore generic metadata modifications only when the
path is an existing directory under the common `.git`. Reuse the existing shared
classifier. Child index/HEAD/ref/config events retain their current invalidation
scope. Missing directories, create/remove/rename, unknown paths, rescan and
watcher errors retain conservative recovery. Working-tree directory events are
not excluded by this fix.

No new state, cache, quota, lock, timeout, case folding or path-key rewrite is
introduced. One metadata event previously could revoke up to the cache's 256
checkout entries; the fix adds a directory-type check only for Git-path
`Modify(Any)` events. The five-minute reuse limit is unchanged.

`monitoring_status_git_directory_metadata_preserves_sibling_but_not_index_writes`
uses real linked checkouts and Git status as an independent oracle. It supplies
raw-event boundary inputs for the common Git directory, worktrees parent and
linked admin directory, then verifies reuse, fresh staged index state, and trust
revocation on topology events. These are API-boundary cases, not a fabricated
claim of captured OS output. Existing tests retain coverage of real watcher
inventory growth, main-entry eviction, sibling commits, file/index writes,
replacement, rescan, config changes and in-flight generation supersession.

The three existing native owner tests provide pre-fix Windows RED on
`fe13ec4179`. The new focused regression has not been run on pre-fix source;
that after-the-fact RED and independent critic remain coordinator work.

## Frontend fixture defect

`git blame` attributes the final-failure test to `08978a8ee2a`, story
`2335-9c03`. CI shard 2/4 times out the first Claude case at the module import;
the shard reports 37% of duration in imports. The test's behavior does not need
that cold module load inside its 5-second deadline.

Import `useAgentPolling` once in each isolated `beforeEach` fixture, after
`vi.resetModules` and loading the matching terminal store. All cases in the
`describe` use that preloaded module. This moves loading outside timed test
bodies without retaining a hook bound to an old store or removing isolation.
The import is once per reset fixture, not once per file. No timeout or assertion
changes, and no production frontend behavior change.

## Validation

One rb call ran:

```text
cargo check --manifest-path src-tauri/Cargo.toml --all-targets --target x86_64-pc-windows-gnu
cargo clippy --manifest-path src-tauri/Cargo.toml -p tuicommander --all-targets -- -D warnings
scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(monitoring_status::tests::)'
```

Results: Windows GNU check passed in 28.19s; Linux Clippy passed in 27.01s;
10 monitoring tests passed in 0.107s, test build 1m 26s. The test wrapper used the
required rb-local test temporary root. rb result `20261028T222059Z-11174`:
`WINDOWS_CHECK=0 CLIPPY=0 TARGETED_TESTS=0`, `RB_BOX_DIRTY=no`.

Then local `pnpm install --offline --frozen-lockfile` reused the current lockfile,
and build-slot ran:

```text
scripts/with-test-tmp.sh pnpm vitest run src/__tests__/hooks/useAgentPolling.test.ts
```

All 46 tests passed; duration 29.79s. The run emitted app debug logs and four
Solid warnings about computations outside a reactive root in the existing remote
session test. These are not a quiet-output pass; no logger suppression or
production lifecycle change was added to hide them.

Combined log: `~/Gits/.tmp/results/tuic-2310j-validation.log`.
Final shell status: `RUST_VALIDATION=0 VITEST=0`.

This is GNU cross-target compilation, native Linux Rust execution, and native
Mac Vitest evidence. Native Windows post-fix behavior and full CI remain owed.
The Rust production change requires an authorized application rebuild/restart;
`to-test.md` records the Windows check. Do not restart the daily instance for this
work. Risk is medium because this changes shared event-driven cache authority.
