# Windows CI round k: preserve the failing event and cache decision

## Observed failure

Windows CI run `40780966844`, source `405db73612`, reports 8,623 passed and one
failed. All three attempts fail
`monitoring_status_snapshot_keeps_unchanged_sibling_out_of_refresh` at the final
unchanged-sibling scan assertion: delta 1, expected 0. Both inventory-growth tests
and the focused directory-metadata regression pass. Evidence:
`~/Gits/.tmp/results/ci-40780966844-win.txt`, lines 7030–7033, 7256–7259 and
7288–7291.

The log still contains no raw event, generation change, signature mismatch or
snapshot-publication decision for the failed read. It does not establish that
another directory event caused the failure. A first scan can also fail to publish
because its generation changes during the scan, its signature is unavailable or
changes, or its backend result is not cacheable.

## Failure diagnostics

The three real-watcher monitoring tests enable an instance-local diagnostic
buffer under `cfg(test)`. Other caches retain no diagnostic history, and release
code contains neither the buffer nor its recording calls. No test emits this
history on success.

The buffer records:

- Raw `notify::Event` values, including kind, all paths and rescan flags.
- Watch registration, explicit invalidation and group watch revocation.
- The directory-metadata/access exclusions and the exact path causing watch loss.
- Each checkout's shared/branch/admin/working classification, prior generation
  and watching state for each event path.
- Snapshot hits and misses, including the captured/current watching and generation
  values, current signature, and stored signature/generation/age.
- Publication inputs and the actual store/reject decision.

The unchanged-sibling assertions freeze this history immediately after the read
whose computation delta they test. Later intentional tracked-file edits, index
updates and checkout removal cannot obscure that history. File/diff parity,
tracked-write scan-count and event-wait failures also include the diagnostic
history. Assertions and all 20-second event-wait budgets remain unchanged.

## Windows backend mapping: expected shapes, not a captured trace

Source: notify 8.2.0, revision
`a1d7c2d8f80786679d58ec6d5986a1d4278bc8cf`,
[`notify/src/windows.rs` subscription](https://github.com/notify-rs/notify/blob/a1d7c2d8f80786679d58ec6d5986a1d4278bc8cf/notify/src/windows.rs#L284-L316)
and
[action mapping](https://github.com/notify-rs/notify/blob/a1d7c2d8f80786679d58ec6d5986a1d4278bc8cf/notify/src/windows.rs#L395-L451).
The implementation subscribes to file/directory names, attributes, size,
last-write, creation and security changes. It prepends the watched directory to
each returned name. It does not subscribe to last-access changes.

| Test operation / Windows notification | Expected notify input | Current cache decision |
|---|---|---|
| Git creates and writes `index.lock` | `Create(Any)`, then `Modify(Any)` on that file | Owning admin only; main index activity does not map to a linked admin |
| Git replaces `index` by renaming its lock | `Modify(Name(From))` on `index.lock`, `Modify(Name(To))` on `index`; replacement may also report `Remove(Any)` | Owning admin invalidates; both rename paths are processed |
| Empty main commit updates its ref lock and `refs/heads/main` | Lock create/write plus name From/To or file Modify notifications | Only a checkout whose recorded branch is `main` invalidates; a sibling on `status-sibling` does not |
| Git updates main `HEAD`, `COMMIT_EDITMSG`, logs or objects | File Modify/Create or lock rename at the corresponding path | Main admin inputs can invalidate main; unrelated logs/objects do not invalidate sibling status |
| Parent Git-directory metadata changes | `Modify(Any)` on an existing Git directory | Round j skips this path; Create/Remove/Rename and rescan still revoke authority where required |
| Test overwrites sibling `target/tracked.txt` | `Modify(Any)` on the tracked file; directory metadata notifications are possible | The sibling owns this working-tree path and invalidates before ignore filtering |
| Test stages that tracked file | Lock/create/write/rename under the sibling admin directory | Sibling invalidates and must return fresh staged status |

These shapes follow from the supplied Windows action and the backend mapping.
They are not an assertion that every listed action occurs, nor that the OS emits
them in one fixed order for this fixture. Only a native trace can establish the
actual sequence responsible for CI's remaining failure.

## Production repair boundary

No additional production watch-trust exception is justified by the available
trace: there is no trace. In particular, ignoring checkout-root metadata merely
because the root currently exists could hide a replaced root. Ignoring linked
index or HEAD events could serve stale status. Changing the test to tolerate one
extra scan would remove the stated unchanged-sibling guarantee.

The safe next evidence is the failing read's raw-event and publication history.
The inline `DEFERRED (2026-10-29)` comment records that missing attribution at the
watcher fixture. This round must not be described as a proven repair of the
remaining Windows rescan.

The coordinator explicitly authorized diagnostics-only after this source audit
and directed that no guessed cache-trust exception be added (mail received
2026-10-29).

## Validation

One `build-slot.sh --remote` call ran:

```text
scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml -p tuicommander --lib -E 'test(monitoring_status::tests::)'
cargo clippy --manifest-path src-tauri/Cargo.toml -p tuicommander --all-targets -- -D warnings
cargo check --manifest-path src-tauri/Cargo.toml --all-targets --target x86_64-pc-windows-gnu
```

All ten monitoring tests passed in 0.125 seconds; test compilation took 2m 08s.
Clippy passed in 48.57 seconds. The Windows GNU all-target check passed in
41.34 seconds. The passing tests emitted no diagnostic history. This verifies
Linux execution and GNU cross-target compilation, not native Windows execution
or the unresolved failure's cause.

Log: `~/Gits/.tmp/results/tuic-2310k-validation.log`.
Remote result: `20261028T232329Z-73115`, `RB_BOX_DIRTY=no`.

No application restart is required for these test-only diagnostics. The
coordinator owns the next native Windows run and the original full-CI acceptance
criterion remains open.
