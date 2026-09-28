#!/usr/bin/env bash
# Run a test command with scratch files inside this checkout or its Gits root.
set -euo pipefail

root="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)"
# A linked worktree must not inherit the parent TUICommander's Cargo target.
# Otherwise its tests can reuse dependency artifacts compiled from another checkout.
if [ -f "$root/.git" ]; then
  unset CARGO_TARGET_DIR
fi
case "${TMPDIR:-}" in
  "$root/"*|"$HOME/Gits/"*) test_tmp_base="${TMPDIR%/}" ;;
  *) test_tmp_base="$root/.tmp/tuic-tests" ;;
esac
mkdir -p "$test_tmp_base"
for stale_root in "$root/.tmp/tuic-tests" "$HOME/Gits/.tmp/tuic-tests"; do
  if [[ -d "$stale_root" ]]; then
    find "$stale_root" -mindepth 1 -maxdepth 1 -type d \
      \( -name 'tuic-run.*' -o -name 'socket-*' \) -mtime +6 -exec rm -rf -- {} +
  fi
done
test_tmp="$(mktemp -d "$test_tmp_base/tuic-run.XXXXXX")"
trap 'rm -rf "$test_tmp"' EXIT
export TMPDIR="$test_tmp/"
export TMP="$test_tmp/"
export TEMP="$test_tmp/"
export TUIC_TEST_TMP_ROOT="$test_tmp"
"$@"
