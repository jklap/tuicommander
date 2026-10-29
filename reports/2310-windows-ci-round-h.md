# Windows CI round h: portable worktree test setup

## Scope and cause

Native Windows CI run `40753012786` on `9a50eaaf60` failed before test execution.
The compiler reported E0425 in
`mcp_http::worktree_routes::survivor_tests::permanent_delete_removes_checkout_and_branch_instead_of_archiving`:
its shared `warm_tests::setup_repo` helper had `#[cfg(unix)]`.

Local `main` at `cc0059c462` was merged into `fix/ci-win-2310g` as
`c54987bd5601b555046a4db95c2b0523b0b975db` before this repair.

The helper only creates a directory, writes a README, and runs Git through
`git_cmd`. None of its operations requires Unix. Remove the helper's Unix gate
instead of excluding the portable deletion regression from Windows. All test
assertions, time bounds, and test gates remain unchanged.

## Audit and adjacent cases

- The permanent-deletion regression remains enabled on Windows and still checks
  checkout removal, branch deletion, and the absence of an archived registration.
- Unix setup-script tests still use the same helper. Their shell polling and
  `libc::kill` helpers retain their Unix gates.
- The existing dirty/live-session and orphan-decision tests already have Unix
  gates; this repair does not add gates or remove coverage.
- A Windows GNU all-target compiler pass checks the other enabled test bodies
  together, rather than relying on a sequence of native CI failures.

The change affects one helper's compilation visibility. It adds no filesystem
operations, processes, state, or runtime work to that helper.

## Evidence

### Pre-fix compile regression

On rb, the Windows GNU target standard library was already installed.

```text
cargo check --manifest-path src-tauri/Cargo.toml --all-targets --target x86_64-pc-windows-gnu
```

The pre-fix snapshot of merged HEAD `c54987bd5601b555046a4db95c2b0523b0b975db`
failed with the same E0425 at `worktree_routes.rs:1150`. It reported one compiler
error. Log: `~/Gits/.tmp/results/tuic-2310h-windows-check.log`.

That snapshot was taken before the local helper edit; rb marked the result stale
because the Mac source changed after sync. Tauri also regenerated the Windows
schema in that disposable rb replica. Neither fact changes the recorded pre-fix
compiler diagnostic. No generated schema was copied into this commit.

### Final single rb validation call

`build-slot.sh --remote` ran these commands serially:

```text
cargo check --manifest-path src-tauri/Cargo.toml --all-targets --target x86_64-pc-windows-gnu
cargo clippy --manifest-path src-tauri/Cargo.toml -p tuicommander --all-targets -- -D warnings
scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(mcp_http::worktree_routes::)'
```

The test command used the required rb-local `TMPDIR` under `~/Gits/.tmp/tuic-tests/`.

- Windows GNU all-target check: passed, 1m 50s.
- Linux all-target Clippy: passed, 2m 20s.
- Both worktree-route test modules: 12 passed, 0 failed, test execution 2.071s;
  test build 4m 42s.
- Final status: `WINDOWS_CHECK=0 CLIPPY=0 TARGETED_TESTS=0`, `RB_BOX_DIRTY=no`.

Log: `~/Gits/.tmp/results/tuic-2310h-final-rb.log`.
Result: `~/Gits/.tmp/rb/20261028T201054Z-73013/result.txt`.

This is Windows GNU compile evidence and native Linux test evidence. It is not
native Windows MSVC linker or runtime evidence. The original full Windows CI
acceptance criterion remains open for the coordinator's next authorized push.
There is no production Rust change to load in the daily application.
