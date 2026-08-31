#!/usr/bin/env bash
# check-gate.sh — Run this repo's full `make check` gate reliably.
#
# Usage:
#   ./scripts/check-gate.sh
#   make check-gate
#
# Exists because `make check` itself is easy to run in a way that silently
# hides a real failure, and because a fresh/lazily-materialized worktree can
# fail one specific test for a reason that has nothing to do with your change:
#
#   1. `make check 2>&1 | tee somelog` reports the exit code of `tee` (almost
#      always 0), not `make`'s — a real failure partway through the chain
#      (`make check`'s steps are joined with `&&`) goes unnoticed if you only
#      look at the pipeline's own exit status. This script uses process
#      substitution (`> >(tee ...)`) instead of a trailing pipe, so `$?` here
#      is `make`'s own exit code.
#   2. `src-tauri/target/debug/tuic-hook` — which `agent_hook.rs`'s
#      `golden_wire_output` tests execute directly and assert on the real
#      output of — can transiently read as 0 bytes even right after a
#      successful build reports "Finished"/nothing-to-rebuild. Rebuilding it
#      defensively before the real run avoids a false failure report.
#
# Three known, pre-existing conditions this script warns about instead of
# letting fail silently confuse an otherwise-clean change (none of these are
# something to "fix" as part of an unrelated change — see AGENTS.md > Fresh
# Worktree Setup):
#   - The `plugins/` git submodule is frequently uninitialized in worktrees,
#     which fails the `Plugin tests` step.
#   - `src/__tests__/components/ChangelogModal.test.tsx` has a flaky async
#     leak (an uncleaned timer/effect from `onMount`, unrelated to this
#     file's own logic) that vitest's leak detector marks as a failed test
#     FILE even when every individual test in the run passes — this makes
#     the `vitest` step, and therefore all of `make check`, fail regardless
#     of what else changed. Confirmed pre-existing (untouched by recent
#     commits) — track/fix it separately, don't chase it as a regression.
#   - There is no `rust-toolchain.toml` and CI installs `dtolnay/rust-toolchain
#     @stable` (a floating, unpinned "whatever is current" version) — and CI
#     has never actually executed on this repo (see AGENTS.md > Tests), so
#     nothing enforces that a contributor's local stable toolchain is new
#     enough for lints a recent commit assumed exist. A commit can land clippy
#     fixes for a lint name that only exists in a newer stable than your local
#     `rustc`/`cargo clippy`; with `-D warnings` promoting `unknown-lints` to a
#     hard error, that reads as "1 errors" naming an innocuous-looking
#     `#[allow(clippy::...)]` in a file you never touched. `rustup update` is
#     the fix, not editing the offending file. Confirmed via `git log`/`git
#     blame`: the 2026-08-29 session that found this traced the failing
#     `#[allow(clippy::unused_async_trait_impl)]` in
#     `src-tauri/src/mcp_proxy/registry.rs` to commit `7db734f9` ("fix(build):
#     clear the clippy errors Rust 1.98 turned into build failures"), already
#     an ancestor of the branch it was investigating — and `rustup check`
#     confirmed the installed toolchain (1.97.1) was exactly one stable
#     release behind (1.98.0) at the time.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

echo "==> Defensive rebuild: tuic-hook (works around a transient 0-byte binary issue)"
(cd src-tauri && cargo build --package tuic-hook)

if command -v rustup >/dev/null 2>&1; then
	rustup_status="$(rustup check 2>/dev/null || true)"
	if echo "$rustup_status" | grep -qi "update available"; then
		echo
		echo "NOTE: a newer stable Rust toolchain is available:"
		echo "$rustup_status" | grep -i "update available" | sed 's/^/      /'
		echo "      This repo has no rust-toolchain.toml pin and CI floats on"
		echo "      'stable', so a commit can start assuming a lint/feature only"
		echo "      a newer stable knows about (clippy then fails with 'unknown"
		echo "      lint' on an unrelated file, not a real regression in your"
		echo "      change). If the clippy step below fails with 'unknown lint',"
		echo "      run 'rustup update' and re-run this script before assuming"
		echo "      your change broke something."
	fi
fi

if git submodule status plugins 2>/dev/null | grep -q '^-'; then
	echo
	echo "NOTE: the plugins/ submodule is not initialized in this worktree."
	echo "      make check's 'Plugin tests' step will fail with exit 1 as a"
	echo "      result of this — known, pre-existing environment drift, not a"
	echo "      regression from your change. If that is the ONLY failure"
	echo "      reported below, the gate is effectively green."
fi

LOG="$(mktemp -t tuic-check-gate)"

echo
echo "==> Running make check (several minutes: clippy --release + full nextest run)"
# `if make check ...; then` would NOT work here: bash's `if` returns exit
# status 0 when its condition is false and there is no `else` — it does NOT
# pass through the tested command's real status. Capture `$?` directly
# instead, with `set -e` off around just this one command since a nonzero
# status here is expected and handled explicitly below, not a script bug.
set +e
make check > >(tee "$LOG") 2>&1
status=$?
set -e

if [ "$status" -eq 0 ]; then
	echo
	echo "All checks passed."
	rm -f "$LOG"
	exit 0
fi

echo
echo "make check exited $status. Steps that reported a result:"
grep -E "✓|✗|error:|FAILED|Found [0-9]+ error" "$LOG" || true

if grep -q "unknown lint" "$LOG"; then
	echo
	echo "NOTE: 'unknown lint' means your local clippy is older than a lint"
	echo "      name a commit's #[allow(clippy::...)] assumes exists — a"
	echo "      stale-toolchain problem (see the header of this script), not"
	echo "      a real regression in your change. Run 'rustup update' and"
	echo "      re-run this script before investigating the named file further."
fi

if grep -q "rust tests ✓" "$LOG" && ! grep -q "vitest ✓" "$LOG"; then
	echo
	echo "NOTE: the failure looks like it's in the vitest step. Before treating"
	echo "      this as a regression, run 'pnpm exec vitest run' directly and"
	echo "      check for 'Tests  N passed (N)' with 0 real failures — a known,"
	echo "      pre-existing flaky async leak in ChangelogModal.test.tsx marks"
	echo "      that file 'failed' (and fails this whole step) even when every"
	echo "      individual test passes. If that's what you see, this failure is"
	echo "      unrelated to your change."
fi

echo
echo "Full log kept at: $LOG"
exit "$status"
