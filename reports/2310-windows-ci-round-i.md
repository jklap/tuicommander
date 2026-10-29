# Windows CI round i: watcher fixture path identity

## Follow-up

Run `40773627108` confirms that the path-oracle repair made the required events
arrive, but exposes a second failure in production sibling-cache reuse. The
classification below applies to the old event-wait timeouts only, not the later
computation-count failures. See `2310-windows-ci-round-j.md`.

## Classification

The three remaining failures in native Windows CI run `40762628254` on
`4c015082be7c68ac616e334327a4ef368d2f4486` are test-oracle defects, not evidence
that production fails to receive worktree changes.

All three tests belong to story `2098-98b2`:

| Test | Introduction and relevant change |
| --- | --- |
| `monitoring_status_snapshot_keeps_unchanged_sibling_out_of_refresh` | `7bd2043e5e` introduced it; `5479e7004b` added the index-event wait that fails |
| `monitoring_status_inventory_add_keeps_unchanged_sibling_reusable` | `b3420d9c9c` introduced the shared inventory helper and test |
| `monitoring_status_inventory_add_after_main_entry_eviction_keeps_sibling_reusable` | `b3420d9c9c` introduced the eviction case |

These attributions come from `git blame` and the commit subjects. The later
production repair `59e161ba6b` preserves sibling watches during inventory growth.

## Cause and backend evidence

The tests canonicalize the repository and linked checkout before registering
recursive watches. On Windows, those roots have the verbatim `\\?\D:\...`
spelling. In contrast, `tuic-git/src/git.rs::resolve_git_dir` reads a linked
checkout's `.git` file and returns its absolute `gitdir:` value without
canonicalizing it. Git writes that value as `D:/...`.

Two expected paths were derived from that raw Git text:

- The snapshot test's linked `index` path.
- The inventory helper's new admin directory path, shared by both inventory tests.

The tests compared these values directly with `notify::Event.paths`. Windows
`Path` equality does not equate a normal drive prefix with a verbatim drive
prefix. Accepting both slash styles on normal Windows paths does not remove that
prefix difference.

The installed `notify` 8.2.0 source identifies upstream commit
`a1d7c2d8f80786679d58ec6d5986a1d4278bc8cf` in `.cargo_vcs_info.json`.
Its Windows backend proves the event spelling and event kinds:

- [`watch_inner`, lines 530–543](https://github.com/notify-rs/notify/blob/a1d7c2d8f80786679d58ec6d5986a1d4278bc8cf/notify/src/windows.rs#L530-L543)
  preserves an absolute watch path; it does not strip its verbatim prefix.
- [`start_read`, lines 284–316](https://github.com/notify-rs/notify/blob/a1d7c2d8f80786679d58ec6d5986a1d4278bc8cf/notify/src/windows.rs#L284-L316)
  requests directory-name and file-name notifications, among other changes, and
  passes recursive monitoring to `ReadDirectoryChangesW`.
- [`handle_event`, lines 394–398](https://github.com/notify-rs/notify/blob/a1d7c2d8f80786679d58ec6d5986a1d4278bc8cf/notify/src/windows.rs#L394-L398)
  joins the registered root with the native UTF-16 relative notification name.
  It therefore emits the canonical watched-root prefix plus native separators,
  not the Git text spelling used by the failing oracles.
- The same handler maps `FILE_ACTION_ADDED` to `Create(Any)`, modified to
  `Modify(Any)`, and new rename names to `Modify(Name(To))`. These are all
  accepted by the tests' existing non-access predicate. A new admin directory
  does not need a Linux-only `Create(Folder)` event in these tests.

No case folding is required or added: the fixtures create these exact lowercase
admin/index names. The backend preserves native relative names; the fix does not
broaden an event match to case-insensitive strings or unrelated paths.

The snapshot test reached its index wait only after receiving the real marker,
main-ref, and tracked linked-file events. Its timeout is at line 380. Both
inventory tests timed out expecting the raw `D:/.../.git/worktrees/added` value
at line 705. All three repeat across three attempts in the supplied native log:
`~/Gits/.tmp/results/ci-win-40762628254.log.txt`.

## Production path and fix

`StatusCache::set_watching` already canonicalizes each admin directory before
checking its common-dir parent and backlink. The production watcher registers
canonical linked roots through `linked_worktree_roots`; its raw callback feeds
`StatusCache::event` before event filtering. The fixture callbacks do the same.
For these canonical watched roots, the received paths and production admin
mapping agree. The failing values exist only in the test wait oracles.

Canonicalize the two expected Git-derived paths after Git has created them.
This preserves exact path equality, all required real events, every status and
computation assertion, and all 20-second wait bounds. No production code,
watch registration, event-kind filter, platform gate, or cache contract changes.

Adjacent cases retained and checked:

1. Main-root marker and ref events retain their existing canonical root spelling.
2. Linked tracked-file writes still force a fresh status computation matching Git.
3. Linked index writes still require their own real mutating event.
4. New admin creation still must arrive before inventory sync arms the new root.
5. Main-cache eviction still must preserve the old sibling's computation count.

The worst-case wait budget is unchanged. The only added work is one path
resolution at each of the two test fixture boundaries. No new test is needed:
the existing three native failures protect these exact contracts.

## Validation

One rb call ran these commands serially:

```text
cargo check --manifest-path src-tauri/Cargo.toml --all-targets --target x86_64-pc-windows-gnu
cargo clippy --manifest-path src-tauri/Cargo.toml -p tuicommander --all-targets -- -D warnings
scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(monitoring_status::tests::)'
```

The test command used rb-local `TMPDIR` under `~/Gits/.tmp/tuic-tests/`.

- Windows GNU all-target check: passed in 15.85s.
- Linux all-target Clippy: passed in 24.57s.
- Monitoring status module: 9 passed, 0 failed in 0.129s; test build 1m 23s.
- `WINDOWS_CHECK=0 CLIPPY=0 TARGETED_TESTS=0`, `RB_BOX_DIRTY=no`.

Log: `~/Gits/.tmp/results/tuic-2310i-final-rb.log`.
Result: `~/Gits/.tmp/rb/20261028T213353Z-78429/result.txt`.

Native Windows post-fix execution is still required. These results do not claim
that it passed, or that all possible Windows watcher behavior has been tested.
The classification above follows the failing source expressions and verified
backend semantics, not a new native event capture. No daily-app restart is needed
for this test-only repair.
