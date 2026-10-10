#!/usr/bin/env bash
# Both test harnesses (scripts/with-test-tmp.sh and nextest's setup script) must
# fence git discovery at the test root: a repo-less scratch dir under the root
# must never resolve to an enclosing repository (a fixture whose `git init`
# failed once fell through to the real checkout and renamed its branches).
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
test_tmp="${TUIC_TEST_TMP_ROOT:-$root/.tmp/tuic-tests}"
mkdir -p "$test_tmp"
scratch="$(mktemp -d "$test_tmp/git-ceiling.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
# This test's own fixtures must not escape either.
export GIT_CEILING_DIRECTORIES="${GIT_CEILING_DIRECTORIES:+$GIT_CEILING_DIRECTORIES:}$(cd "$test_tmp" && pwd):$(cd "$test_tmp" && pwd -P)"

fail() { echo "$*" >&2; exit 1; }

# An outer repository standing in for the real checkout.
outer="$scratch/outer"
git init -q --template= "$outer"
[ "$(cd "$outer" && pwd -P)" = "$(git -C "$outer" rev-parse --show-toplevel)" ] || fail "fixture init failed: $outer"

# 1. with-test-tmp.sh, run from a checkout copy inside the outer repo: its
#    default root is <checkout>/.tmp/tuic-tests, i.e. inside a repository.
mkdir -p "$outer/scripts"
cp "$root/scripts/with-test-tmp.sh" "$outer/scripts/with-test-tmp.sh"
probe='mkdir -p "$TMPDIR/x"; git -C "$TMPDIR/x" rev-parse --show-toplevel'
if found="$(env -u TMPDIR -u GIT_CEILING_DIRECTORIES "$outer/scripts/with-test-tmp.sh" sh -c "$probe" 2>&1)"; then
  fail "with-test-tmp.sh: git under the test root found $found"
fi
# A stricter value set by an outer wrapper survives, and nothing is listed twice.
value="$(env -u TMPDIR GIT_CEILING_DIRECTORIES=/pre/set "$outer/scripts/with-test-tmp.sh" sh -c 'printf %s "$GIT_CEILING_DIRECTORIES"')"
case "$value" in /pre/set:*) ;; *) fail "with-test-tmp.sh dropped the inherited ceiling: $value" ;; esac
case ":$value:" in *":$outer/.tmp/tuic-tests:"*) ;; *) fail "with-test-tmp.sh: ceiling lacks the root's parent: $value" ;; esac
[ "$(printf %s "$value" | tr ':' '\n' | sort | uniq -d)" = "" ] || fail "with-test-tmp.sh: duplicate ceiling entries: $value"

# 2. nextest's setup script, with its root inside the outer repo.
ntroot="$outer/sub/root"
mkdir -p "$ntroot/x"
git -C "$ntroot/x" rev-parse --show-toplevel >/dev/null 2>&1 || fail "control: the outer repo is not discoverable"
(cd "$root/src-tauri" && GIT_CEILING_DIRECTORIES=/pre/set TUIC_TEST_TMP_ROOT="$ntroot" NEXTEST_ENV="$scratch/env" sh scripts/nextest-test-tmp.sh)
ceiling="$(sed -n 's/^GIT_CEILING_DIRECTORIES=//p' "$scratch/env")"
case "$ceiling" in /pre/set:*) ;; *) fail "nextest-test-tmp.sh dropped the inherited ceiling: $ceiling" ;; esac
case ":$ceiling:" in *":$outer/sub:"*) ;; *) fail "nextest-test-tmp.sh: ceiling lacks the root's parent: $ceiling" ;; esac
if found="$(GIT_CEILING_DIRECTORIES="$ceiling" git -C "$ntroot/x" rev-parse --show-toplevel 2>&1)"; then
  fail "nextest-test-tmp.sh: git under the test root found $found"
fi
