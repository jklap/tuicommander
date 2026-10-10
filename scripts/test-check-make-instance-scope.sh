#!/usr/bin/env bash
# Tests for scripts/check-make-instance-scope.sh and the Makefile's per-checkout
# `TUIC_APP_INSTANCE` derivation. Runs the guard against copies of the real
# Makefile placed in fixture directories with chosen names, plus mutated copies
# that reintroduce each shape the guard exists to catch.
set -euo pipefail

project_root="$(git rev-parse --show-toplevel)"
test_tmp="${TUIC_TEST_TMP_ROOT:-${TMPDIR:-/tmp}}"
mkdir -p "$test_tmp"
fixture_root="$(mktemp -d "$test_tmp/make-instance-scope.XXXXXX")"
trap 'rm -rf "$fixture_root"' EXIT

check="$project_root/scripts/check-make-instance-scope.sh"
failures=0

# A fixture checkout: <dir name> -> a directory holding a copy of the Makefile.
fixture() {
	local dir="$fixture_root/$1"
	mkdir -p "$dir"
	cp "$project_root/Makefile" "$dir/Makefile"
	printf '%s\n' "$dir"
}

dev_instance() { # <dir>
	env -u TUIC_APP_INSTANCE MAKEFLAGS= make -C "$1" -n dev 2>&1 |
		grep -oE '(^|[[:space:]])TUIC_APP_INSTANCE=[^[:space:]]*' | head -1 | sed 's/.*=//'
}

expect_eq() { # <label> <want> <got>
	if [ "$2" != "$3" ]; then
		echo "FAIL: $1: expected '$2', got '$3'" >&2
		failures=$((failures + 1))
	fi
}

expect_pass() { # <label> <dir>
	if ! out=$(bash "$check" "$2" 2>&1); then
		echo "FAIL: $1: guard rejected a correct Makefile:" >&2
		printf '%s\n' "$out" | sed 's/^/    /' >&2
		failures=$((failures + 1))
	fi
}

expect_fail() { # <label> <dir> <expected message fragment>
	local out
	if out=$(bash "$check" "$2" 2>&1); then
		echo "FAIL: $1: guard accepted a broken Makefile" >&2
		failures=$((failures + 1))
	elif ! printf '%s\n' "$out" | grep -qF -- "$3"; then
		echo "FAIL: $1: guard failed for the wrong reason (wanted '$3'):" >&2
		printf '%s\n' "$out" | sed 's/^/    /' >&2
		failures=$((failures + 1))
	fi
}

# 1. The real Makefile passes, and the id follows the checkout directory name.
dir=$(fixture "My Checkout.Name__X")
expect_pass "real Makefile" "$dir"
expect_eq "derived id" "tuic-my-checkout-name-x" "$(dev_instance "$dir")"

# 2. Two checkouts get two different instances.
other=$(fixture "another-worktree")
expect_eq "second checkout's id" "tuic-another-worktree" "$(dev_instance "$other")"

# 3. A long name is cut to a valid id: <= 63 characters, no trailing hyphen
#    even when the cut lands right after one.
long_name="$(printf 'a%.0s' $(seq 1 57))-bcdefghijklmnop"
dir=$(fixture "$long_name")
id=$(dev_instance "$dir")
expect_eq "long id" "tuic-$(printf 'a%.0s' $(seq 1 57))" "$id"
expect_pass "real Makefile, long directory name" "$dir"

# 4. A name with no usable character still yields a valid id.
dir=$(fixture "___")
expect_eq "fallback id" "tuic-checkout" "$(dev_instance "$dir")"

# 5. main's old shape: only `make test` isolated, `make dev` on the shared config.
dir=$(fixture "old-shape")
sed -i.bak -E 's/^TUIC_APP_INSTANCE\?=.*/test: TUIC_APP_INSTANCE?=tuic-test/' "$dir/Makefile"
expect_fail "test-only default" "$dir" "target-specific TUIC_APP_INSTANCE assignment"

# 6. The default removed altogether: both targets on the shared config, silently.
dir=$(fixture "no-default")
sed -i.bak -E '/^TUIC_APP_INSTANCE\?=/d' "$dir/Makefile"
expect_fail "no default" "$dir" "is not a per-checkout 'tuic-<label>' instance id"

# 7. The loud warning dropped from a target.
dir=$(fixture "no-warning")
awk 'BEGIN{seen=0} /^\t\$\(WARN_SHARED_INSTANCE\)$/ && !seen {seen=1; next} {print}' \
	"$project_root/Makefile" > "$dir/Makefile"
expect_fail "missing warning" "$dir" "TUIC_APP_INSTANCE= warns: expected 'yes', got 'no'"

# 8. A new launch target that forgets the instance.
dir=$(fixture "new-target")
printf '\nrun-dev-again:\n\tpnpm tauri dev --no-watch\n' >> "$dir/Makefile"
expect_fail "unscoped launch" "$dir" "a 'tauri dev' launch without TUIC_APP_INSTANCE"

if [ "$failures" -ne 0 ]; then
	echo "$failures check-make-instance-scope test(s) failed" >&2
	exit 1
fi
echo "check-make-instance-scope: all tests passed"
