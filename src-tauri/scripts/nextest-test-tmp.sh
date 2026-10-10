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
# Fail closed: git in a test process must never discover a repository above
# the root (a fixture whose `git init` failed once fell through to the real
# checkout and renamed its branches). GIT_CEILING_DIRECTORIES = the root's
# parent, lexical and physical spelling (macOS /var vs /private/var), each
# appended once to any value an outer wrapper already set.
git_ceiling=${GIT_CEILING_DIRECTORIES:-}
for dir in "$(dirname "$root")" "$(cd "$(dirname "$root")" && pwd -P)"; do
    case ":$git_ceiling:" in
        *":$dir:"*) ;;
        *) git_ceiling=${git_ceiling:+$git_ceiling:}$dir ;;
    esac
done
{
    printf 'TUIC_TEST_TMP_ROOT=%s\n' "$root"
    printf 'TMPDIR=%s/\n' "$root"
    printf 'TMP=%s/\n' "$root"
    printf 'TEMP=%s/\n' "$root"
    printf 'GIT_TEMPLATE_DIR=%s\n' "$git_template"
    printf 'GIT_CEILING_DIRECTORIES=%s\n' "$git_ceiling"
} >> "$NEXTEST_ENV"
