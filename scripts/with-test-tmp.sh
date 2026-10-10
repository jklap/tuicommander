#!/usr/bin/env bash
# Run a test command with its scratch files in a fresh per-run directory under
# the caller's temp dir ($TMPDIR, else /tmp). Never reads or writes $HOME.
#
#   TUIC_TEST_HOST_TMPDIR  the caller's temp dir. Exported, so nested wrappers
#                          and test processes still see it after TMPDIR moves.
#   TUIC_TEST_TMP_BASE     opt-in parent of the per-run dirs (default
#                          <host>/tuic-tests); <checkout>/.tmp/tuic-tests
#                          restores the old in-checkout layout.
#   TUIC_TEST_TMP_ROOT     the per-run dir; TMPDIR/TMP/TEMP point at it too.
set -euo pipefail

root="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)"
# A linked worktree must not inherit the parent TUICommander's Cargo target.
# Otherwise its tests can reuse dependency artifacts compiled from another checkout.
if [ -f "$root/.git" ]; then
  unset CARGO_TARGET_DIR
fi

host="${TUIC_TEST_HOST_TMPDIR:-${TMPDIR:-/tmp}}"
host="${host%/}"
host="${host:-/}"
test_tmp_base="${TUIC_TEST_TMP_BASE:-$host/tuic-tests}"
test_tmp_base="${test_tmp_base%/}"
mkdir -p "$test_tmp_base"

# Housekeeping only: a prune that cannot delete must never fail the command.
warn_prune() {
  echo "with-test-tmp: could not prune stale scratch in $1; continuing" >&2
}
# Per-run dirs older than six days, in the current base and in the old
# in-checkout base this script used to create them in.
prune_stale_runs() {
  [[ -d "$1" ]] || return 0
  find "$1" -mindepth 1 -maxdepth 1 -type d \
    \( -name 'tuic-run.*' -o -name 'socket-*' \) -mtime +6 \
    -exec rm -rf -- {} + 2>/dev/null || warn_prune "$1"
}
# tuic-test-support's per-checkout socket dirs (tuic-s<16 hex>), ours only.
prune_stale_socket_dirs() {
  [[ -d "$1" ]] || return 0
  local stale
  while IFS= read -r -d '' stale; do
    [[ ${stale##*/} =~ ^tuic-s[0-9a-f]{16}$ ]] || continue
    [[ -O "$stale" && ! -L "$stale" ]] || continue
    rm -rf -- "$stale" 2>/dev/null || warn_prune "$stale"
  done < <(find "$1" -mindepth 1 -maxdepth 1 -type d \
    -name 'tuic-s????????????????' -mtime +6 -print0 2>/dev/null || true)
}
prune_stale_runs "$test_tmp_base"
prune_stale_runs "$root/.tmp/tuic-tests"
prune_stale_socket_dirs "$host"
for shared in /tmp /private/tmp; do
  [[ -d "$shared" ]] || continue
  [[ "$host" -ef "$shared" ]] && continue
  [[ "$shared" = /private/tmp && /tmp -ef /private/tmp ]] && continue
  prune_stale_socket_dirs "$shared"
done

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
export TUIC_TEST_HOST_TMPDIR="$host"
export TMPDIR="$test_tmp/"
export TMP="$test_tmp/"
export TEMP="$test_tmp/"
export TUIC_TEST_TMP_ROOT="$test_tmp"
"$@"
