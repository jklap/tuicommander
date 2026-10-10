#!/usr/bin/env bash
# Run a test command with its scratch files in a per-run directory under the
# caller's temp dir (roots: scripts/test-tmp-lib.sh). Never reads or writes $HOME.
#
#   TUIC_TEST_HOST_TMPDIR  the caller's temp dir. Exported, so nested wrappers
#                          and test processes still see it after TMPDIR moves.
#   TUIC_TEST_TMP_BASE     opt-in parent of the per-run dirs (default
#                          <host>/tuic-tests). Pointing it into the checkout
#                          is unsafe where writes to the checkout's .git are
#                          denied (sandboxes) and is opt-in only.
#   TUIC_TEST_TMP_ROOT     inherited: the caller's root, used as is and never
#                          deleted (only its stale tuic-run.* children are
#                          pruned); else a fresh <base>/tuic-run.XXXXXX,
#                          removed on exit. TMPDIR/TMP/TEMP point at it too.
set -euo pipefail

root="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)"
# A linked worktree must not inherit the parent TUICommander's Cargo target.
# Otherwise its tests can reuse dependency artifacts compiled from another checkout.
if [ -f "$root/.git" ]; then
  unset CARGO_TARGET_DIR
fi

# shellcheck source=test-tmp-lib.sh
. "$(dirname "$0")/test-tmp-lib.sh"
host="$(tuic_test_host_tmpdir)"
test_tmp_base="$(tuic_test_tmp_base)"
mkdir -p "$test_tmp_base"

# Housekeeping only: a prune that cannot delete must never fail the command.
warn_prune() {
  echo "with-test-tmp: could not prune stale scratch in $1; continuing" >&2
}
# Per-run and per-checkout dirs (current and older names) untouched for six
# days, in the current base, the old in-checkout base and an inherited root.
prune_stale_runs() {
  [[ -d "$1" ]] || return 0
  find "$1" -mindepth 1 -maxdepth 1 -type d \
    \( -name 'tuic-run.*' -o -name 'socket-*' -o -name 'tuic-co-*' \
    -o -name 'tuic-proc-*' -o -name 'tuic-nextest-*' \) -mtime +6 \
    -exec rm -rf -- {} + 2>/dev/null || warn_prune "$1"
}
# tuic-test-support's socket roots in the host temp dir, ours only: a private
# t.XXXXXX holding its .tuic-socket-root marker (each test process removes its
# own at exit; this catches the ones a killed process left), and the older
# per-checkout tuic-s<16 hex> dirs. Never anything in /tmp.
prune_stale_socket_dirs() {
  [[ -d "$1" ]] || return 0
  local stale name
  while IFS= read -r -d '' stale; do
    name=${stale##*/}
    [[ -O "$stale" && ! -L "$stale" ]] || continue
    if [[ $name =~ ^t\.[a-z0-9]{6}$ ]]; then
      [[ -f "$stale/.tuic-socket-root" ]] || continue
    elif [[ ! $name =~ ^tuic-s[0-9a-f]{16}$ ]]; then
      continue
    fi
    rm -rf -- "$stale" 2>/dev/null || warn_prune "$stale"
  done < <(find "$1" -mindepth 1 -maxdepth 1 -type d \
    \( -name 't.??????' -mtime +1 -o -name 'tuic-s????????????????' -mtime +6 \) \
    -print0 2>/dev/null || true)
}
prune_stale_runs "$test_tmp_base"
prune_stale_runs "$root/.tmp/tuic-tests"
prune_stale_socket_dirs "$host"

if [[ -n "${TUIC_TEST_TMP_ROOT:-}" ]]; then
  # The caller's root: use it as is, never delete it.
  test_tmp="${TUIC_TEST_TMP_ROOT%/}"
  mkdir -p "$test_tmp"
  prune_stale_runs "$test_tmp"
else
  test_tmp="$(mktemp -d "$test_tmp_base/tuic-run.XXXXXX")"
  # Real permission tests may leave unreadable directories or read-only files.
  # This root is disposable; physical traversal never changes symlink targets.
  cleanup_test_tmp() {
    local command_status=$?
    chmod -R u+rwX "$test_tmp"
    rm -rf "$test_tmp"
    return "$command_status"
  }
  trap cleanup_test_tmp EXIT
fi
export TUIC_TEST_HOST_TMPDIR="$host"
export TMPDIR="$test_tmp/"
export TMP="$test_tmp/"
export TEMP="$test_tmp/"
export TUIC_TEST_TMP_ROOT="$test_tmp"
"$@"
