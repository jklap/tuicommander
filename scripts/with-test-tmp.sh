#!/usr/bin/env bash
# Run a test command with scratch files inside this checkout or its Gits root.
set -euo pipefail

root="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)"
case "${TMPDIR:-}" in
  "$root/"*|"$HOME/Gits/"*) test_tmp_base="${TMPDIR%/}" ;;
  *) test_tmp_base="$root/.tmp/tuic-tests" ;;
esac
mkdir -p "$test_tmp_base"
test_tmp="$(mktemp -d "$test_tmp_base/tuic-run.XXXXXX")"
trap 'rm -rf "$test_tmp"' EXIT
export TMPDIR="$test_tmp/"
export TMP="$test_tmp/"
export TEMP="$test_tmp/"
export TUIC_TEST_TMP_ROOT="$test_tmp"
"$@"
