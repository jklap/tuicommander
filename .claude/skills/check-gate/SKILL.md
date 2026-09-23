---
name: check-gate
description: >
  Run TUICommander's full make-check gate (tsc, biome, architecture cycles,
  rustfmt, clippy, the full Rust test suite, vitest, plugin tests, pnpm/cargo
  audit) reliably before declaring any code change complete. Use before
  telling the user "all tests pass" or a feature is done — a scoped test
  filter or a partial run is not sufficient. Also use if asked to "run all
  checks", "run the check gate", or "verify everything passes".
keywords:
  - make check
  - check gate
  - pre-commit
  - full test suite
  - clippy
  - rustfmt
  - biome
  - ci
---

# Check Gate

TUICommander's `make check` is the authoritative "is this actually done" gate: tsc, biome,
architecture-cycle check, plugin-docs-sync, `cargo fmt --check`, `cargo clippy --release -D
warnings`, `cargo nextest run --workspace` + doctests, the full `vitest run`, plugin tests, and
`pnpm audit` / `cargo audit`.

**Do not declare a feature complete, or tell the user "all tests pass," based only on a
feature-scoped test filter or a bare `cargo test`/`vitest run` with no path filter.** Repo-wide
consistency tests — e.g. `src/__tests__/transport.test.ts`'s assertion that every registered
Tauri command is either HTTP-mapped or in `INTENTIONALLY_UNMAPPED` (see `AGENTS.md` > IPC / HTTP
Parity) — only run as part of the full suite. A scoped filter by component or feature name never
touches them, so a real, currently-failing violation can hide behind an all-green scoped report.
This exact thing happened in the session that created this skill: a scoped `vitest run` came back
green, the feature was reported done, and only running the real full check surfaced that two new
`#[tauri::command]`s had never been added to `INTENTIONALLY_UNMAPPED`.

## How to run it

Use the wrapper script, not a bare `make check`:

```bash
./scripts/check-gate.sh
# or
make check-gate
```

Do **not** run `make check 2>&1 | tee somelog.log` yourself and trust the reported exit code —
that pipeline's exit status is `tee`'s (almost always 0), not `make`'s, so a real failure
partway through the `&&`-chained steps goes unnoticed. `check-gate.sh` fixes this (captures
`make`'s own exit code correctly) and also defensively rebuilds `tuic-hook` first, since that
binary can transiently read as 0 bytes even right after a successful build — a stale copy fails
`agent_hook.rs`'s `golden_wire_output` tests for reasons unrelated to your change. See
`AGENTS.md` > Fresh Worktree Setup for background on both gotchas.

## Interpreting the result

- Exit 0: every step passed. Report the change as fully checked.
- Non-zero, with the script's note that the `plugins/` submodule isn't initialized, AND that is
  the *only* reported failure: this is known, pre-existing environment drift (see `AGENTS.md` >
  Fresh Worktree Setup / Tests), not a regression — say so explicitly rather than treating it as
  a blocker, but don't silently omit it either.
- Non-zero with the script's "failure looks like it's in the vitest step" note: run
  `pnpm exec vitest run` directly and check for `Tests  N passed (N)` with 0 real failures. A
  known, pre-existing flaky async leak in `ChangelogModal.test.tsx` (unrelated to that file's own
  logic — an uncleaned timer/effect) marks that test FILE "failed" and fails the whole vitest
  step even when every individual test passes. If that's what you see, this failure predates and
  is unrelated to your change — say so explicitly, don't treat it as a blocker, and don't spend
  time chasing it as part of an unrelated change.
- Non-zero for any other reason: a real failure. Read the failing step's own output (the script
  prints which steps reported `✓`/`✗` and keeps the full log) and fix it before reporting done.

This takes several minutes (`cargo clippy --release` + a full `cargo nextest run --workspace`
are the slow parts) — run it once near the end of a change, not after every small edit.
