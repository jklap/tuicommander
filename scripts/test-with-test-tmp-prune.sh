#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
shared="$HOME/Gits/.tmp/tuic-tests"
mkdir -p "$shared"
old_socket="$shared/socket-prune-test-$$"
old_run="$shared/tuic-run.prune-test-$$"
recent_socket="$shared/socket-keep-test-$$"
trap 'rm -rf "$old_socket" "$old_run" "$recent_socket"' EXIT
mkdir "$old_socket" "$old_run" "$recent_socket"
touch -t 202001010000 "$old_socket" "$old_run"

"$root/scripts/with-test-tmp.sh" true

test ! -e "$old_socket" || { echo "stale socket scratch was not pruned" >&2; exit 1; }
test ! -e "$old_run" || { echo "stale test run was not pruned" >&2; exit 1; }
test -d "$recent_socket" || { echo "recent socket scratch was removed" >&2; exit 1; }
