#!/bin/sh
set -eu

root=${TUIC_TEST_TMP_ROOT:-"$(cd .. && pwd)/.tmp/tuic-tests"}
mkdir -p "$root"
root=$(cd "$root" && pwd)
# An empty template dir keeps `git init`/`git clone` in test fixtures from
# copying git's own hooks/*.sample files into every scratch repo: slower,
# machine-dependent, and EPERM where `.git/hooks` writes are forbidden. Test
# processes only; production git calls never see this variable.
git_template="$root/git-template-empty"
mkdir -p "$git_template"
{
    printf 'TUIC_TEST_TMP_ROOT=%s\n' "$root"
    printf 'TMPDIR=%s/\n' "$root"
    printf 'TMP=%s/\n' "$root"
    printf 'TEMP=%s/\n' "$root"
    printf 'GIT_TEMPLATE_DIR=%s\n' "$git_template"
} >> "$NEXTEST_ENV"
