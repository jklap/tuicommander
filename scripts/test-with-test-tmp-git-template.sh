#!/usr/bin/env bash
# Both test harnesses (scripts/with-test-tmp.sh and nextest's setup script) must
# hand test processes an empty GIT_TEMPLATE_DIR, so a fixture `git init` copies
# none of git's hooks/*.sample files into its scratch repo.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
# This test itself may run under with-test-tmp.sh: prove each harness sets the
# variable rather than inheriting it.
unset GIT_TEMPLATE_DIR

"$root/scripts/with-test-tmp.sh" bash -c '
  set -euo pipefail
  [ -n "${GIT_TEMPLATE_DIR:-}" ] || { echo "with-test-tmp.sh did not export GIT_TEMPLATE_DIR" >&2; exit 1; }
  git init -q "$TMPDIR/repo"
  if [ -n "$(ls -A "$TMPDIR/repo/.git/hooks" 2>/dev/null)" ]; then
    echo "with-test-tmp.sh: git init still copied template hooks" >&2
    exit 1
  fi
'

scratch="$(mktemp -d "${TUIC_TEST_TMP_ROOT:-$root/.tmp/tuic-tests}/git-template.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/root"
(cd "$root/src-tauri" && TUIC_TEST_TMP_ROOT="$scratch/root" NEXTEST_ENV="$scratch/env" sh scripts/nextest-test-tmp.sh)
template="$(sed -n 's/^GIT_TEMPLATE_DIR=//p' "$scratch/env")"
[ -n "$template" ] || { echo "nextest-test-tmp.sh did not write GIT_TEMPLATE_DIR" >&2; exit 1; }
[ -d "$template" ] && [ -z "$(ls -A "$template")" ] || { echo "GIT_TEMPLATE_DIR is not an empty dir: $template" >&2; exit 1; }
GIT_TEMPLATE_DIR="$template" git init -q "$scratch/repo"
if [ -n "$(ls -A "$scratch/repo/.git/hooks" 2>/dev/null)" ]; then
  echo "nextest-test-tmp.sh: git init still copied template hooks" >&2
  exit 1
fi
