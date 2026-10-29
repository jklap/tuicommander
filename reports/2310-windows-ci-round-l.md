# Windows monitoring cache: worktree directory metadata (2310-6e76)

## Native evidence

Windows-x64 CI run `40787712783`, source `0028d4edd5`, failed the unchanged-sibling assertion on all three attempts. The first attempt records the following sequence verbatim. CI prefixes are output timestamps, not per-event acquisition timestamps.

```text
2026-10-29T00:00:19.4675714Z     read checkout="\\\\?\\D:\\a\\tuicommander\\tuicommander\\.tmp\\tuic-tests\\tuic-run.Nc9dDI\\.tmp7kPThm\\linked" decision=compute watching=true/true generation=0/0 signature=Some(9185725942862994016) snapshot=None
2026-10-29T00:00:19.4679713Z     raw event base="\\\\?\\D:\\a\\tuicommander\\tuicommander\\.tmp\\tuic-tests\\tuic-run.Nc9dDI\\.tmprI8OYj": Event { kind: Modify(Any), paths: ["\\\\?\\D:\\a\\tuicommander\\tuicommander\\.tmp\\tuic-tests\\tuic-run.Nc9dDI\\.tmp7kPThm\\linked\\target"], attr:tracker: None, attr:flag: None, attr:info: None, attr:source: None }
2026-10-29T00:00:19.4684664Z     event decision checkout="\\\\?\\D:\\a\\tuicommander\\tuicommander\\.tmp\\tuic-tests\\tuic-run.Nc9dDI\\.tmp7kPThm\\linked" path="\\\\?\\D:\\a\\tuicommander\\tuicommander\\.tmp\\tuic-tests\\tuic-run.Nc9dDI\\.tmp7kPThm\\linked\\target" shared=false branch=false admin=false working=true generation=0 watching=true event=Event { kind: Modify(Any), paths: ["\\\\?\\D:\\a\\tuicommander\\tuicommander\\.tmp\\tuic-tests\\tuic-run.Nc9dDI\\.tmp7kPThm\\linked\\target"], attr:tracker: None, attr:flag: None, attr:info: None, attr:source: None }
2026-10-29T00:00:19.4689885Z     event decision checkout="\\\\?\\D:\\a\\tuicommander\\tuicommander\\.tmp\\tuic-tests\\tuic-run.Nc9dDI\\.tmprI8OYj" path="\\\\?\\D:\\a\\tuicommander\\tuicommander\\.tmp\\tuic-tests\\tuic-run.Nc9dDI\\.tmp7kPThm\\linked\\target" shared=false branch=false admin=false working=false generation=2 watching=true event=Event { kind: Modify(Any), paths: ["\\\\?\\D:\\a\\tuicommander\\tuicommander\\.tmp\\tuic-tests\\tuic-run.Nc9dDI\\.tmp7kPThm\\linked\\target"], attr:tracker: None, attr:flag: None, attr:info: None, attr:source: None }
2026-10-29T00:00:19.4693772Z     publish checkout="\\\\?\\D:\\a\\tuicommander\\tuicommander\\.tmp\\tuic-tests\\tuic-run.Nc9dDI\\.tmp7kPThm\\linked" complete=true dirty_ok=true watching=true generation=0/1 signature=Some(9185725942862994016)/Some(9185725942862994016) branch=Some("status-sibling")
2026-10-29T00:00:19.4695655Z     publish checkout="\\\\?\\D:\\a\\tuicommander\\tuicommander\\.tmp\\tuic-tests\\tuic-run.Nc9dDI\\.tmp7kPThm\\linked" decision=reject
2026-10-29T00:00:19.4697760Z     raw event base="\\\\?\\D:\\a\\tuicommander\\tuicommander\\.tmp\\tuic-tests\\tuic-run.Nc9dDI\\.tmprI8OYj": Event { kind: Create(Any), paths: ["\\\\?\\D:\\a\\tuicommander\\tuicommander\\.tmp\\tuic-tests\\tuic-run.Nc9dDI\\.tmprI8OYj\\.git\\index.lock"], attr:tracker: None, attr:flag: None, attr:info: None, attr:source: None }
```

The callback invalidates the linked checkout on an existing `target` directory's `Modify(Any)`. The generation changes during the initial status computation, so publication is rejected despite an unchanged signature. The next refresh repeats the computation. There is no preceding child-file write in that trace.

## Attribution limits and decision

The log proves the event and the cache decision, not the process or operation that changed directory metadata. The fixture's `git worktree add` creates the directory before watcher registration. The locked gix 0.89.0 status path uses non-mutating attribute/ignore stacks. notify 8.2.0 on Windows requests file/directory names, attributes, size, last-write, creation and security, but not last-access. Ordinary last-access notification is therefore not the explanation. Delayed setup metadata remains possible, not proven.

Microsoft documents that last-write notifications can wait for sufficient cache flushing: <https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesw>. Its backend maps `FILE_ACTION_MODIFIED` to `Modify(Any)` without further attribution.

The coordinator explicitly approved applying the existing `fs::is_directory_metadata_event` rule to mapped worktree directories as well as common Git directories. This fixes the inconsistent event classification, not a named writer. It does not ignore `target` by name, add a cache, or retry status scans. Checkout roots and unmapped paths retain conservative watch-identity handling. Child events remain authoritative, including tracked files under ignored-looking directories.

## Contract and worst case

A directory metadata notification during or after a status load must not cause an otherwise unchanged checkout to scan again. Any number of these metadata events leaves the snapshot generation unchanged; a single real child write must still invalidate it. The entry cap is unchanged at 256, the snapshot TTL at 300 seconds, and the watcher test deadlines at 20 seconds. A silently missing child event remains an existing complete-recursive-watch assumption, not something this change can prove away.

## Validation

Pre-fix source: `0028d4edd5d7446d48b38391f0988fe9d6342169`, with only new tests added. On rb, `scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml -p tuicommander --lib -E 'test(monitoring_status_directory_metadata_during_load_preserves_snapshot)'` failed as intended: `target: publication rejected`, actual computations 2 versus expected 1, exit 100. One test ran; its assertion failed in 0.052 seconds. Log: `~/Gits/.tmp/results/tuic-2310l-red.log`.

Controls cover directory create/remove/rename, missing and non-directory `Modify(Any)`, and an unmapped existing directory. Existing tests retain real tracked writes under `target`, linked index writes, root replacement, rescan, TTL and superseded-generation rejection. The raw-event status-input test also checks a real file deletion. Existing assertions and deadlines are unchanged.

Final rb validation: `scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml -p tuicommander --lib -E 'test(monitoring_status::tests::)'` passed all 12 selected tests in 0.110 seconds after a 1 minute 11 second incremental build. `cargo clippy --manifest-path src-tauri/Cargo.toml -p tuicommander --all-targets -- -D warnings` passed in 26.11 seconds. `TARGETED_TESTS=0 CLIPPY=0`, `RB_BOX_DIRTY=no`. Log: `~/Gits/.tmp/results/tuic-2310l-validation.log`; rb result `20261029T001833Z-12244`. Changed-file rustfmt, instruction links and `git diff --check` also passed.

This is Linux execution of the recorded native event boundary, not a new Windows runtime observation. No full suite, mutation run, new Windows cross-target compile or native Windows execution was performed in this round. Full native Windows CI remains required before story closure. The Rust change requires a manual rebuild/restart to load; no daily app restart was performed. Risk is medium: shared monitoring-cache freshness depends on correctly distinguishing directory metadata from file/topology events.
