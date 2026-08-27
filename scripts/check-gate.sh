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
# Two more known, pre-existing conditions this script warns about instead of
# letting fail silently confuse an otherwise-clean change (neither is
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

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

echo "==> Defensive rebuild: tuic-hook (works around a transient 0-byte binary issue)"
(cd src-tauri && cargo build --package tuic-hook)

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
