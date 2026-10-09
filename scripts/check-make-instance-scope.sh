#!/usr/bin/env bash
# Guard: every Makefile launch of the desktop dev build (`make dev`, `make
# test`) runs on its OWN per-checkout config instance (`instances/<id>/`), and
# only an explicit EMPTY `TUIC_APP_INSTANCE=` lands on the shared default config
# directory — with a loud warning.
#
# History: main once kept `make dev` on the shared default config and only
# defaulted `make test` to `tuic-test`. A bare `TUIC_APP_INSTANCE?=tuic-test` is
# a *global* make variable, so `make dev` silently expanded it too (twice: an
# "every repository vanished" scare on 2026-09-14 and 2026-09-17). The other
# direction burned too: a worktree's `make dev` on the SHARED config repaired
# the global ~/.claude.json `tuicommander` entry to point at that worktree's own
# tuic-bridge, which went ENOENT when the worktree was removed (2026-09-29).
# The user decision of 2026-10-08 is per-checkout isolation for BOTH targets
# (derived from the checkout directory name), so concurrent worktrees never
# share one repositories.json and nothing started here touches the shared one
# by accident. A Makefile edit can break that without looking different, hence
# this check, which asks make what it actually expands.
#
# Usage: check-make-instance-scope.sh [<dir containing the Makefile>]
# (default: this checkout). The directory argument exists for
# scripts/test-check-make-instance-scope.sh, which runs it against fixtures.
set -euo pipefail

if [ "$#" -ge 1 ]; then
	cd "$1"
else
	cd "$(dirname "$0")/.."
fi

# Absence of the token is NOT a usable answer on its own: `make -n` erroring
# out, or a recipe reshaped so the literal `TUIC_APP_INSTANCE=` token stops
# appearing, both print nothing — and an empty value is a real answer for the
# opt-out cases. So the two "I saw nothing" cases report a sentinel no `expect`
# can match, and dump make's diagnostics. `set -euo pipefail` does not cover
# this: a command substitution in argument position never fires errexit.
#
# Diagnostics go to stderr; stdout is the value the caller compares.
parsed_instance() { # <make-output> <make-rc>
	local out="$1" rc="$2" match
	if [ "$rc" -ne 0 ]; then
		printf '      make -n exited %d:\n' "$rc" >&2
		printf '%s\n' "$out" | sed 's/^/      /' >&2
		echo '<make-failed>'
		return
	fi
	# Anchored left so a future MY_TUIC_APP_INSTANCE cannot pass for this one.
	match=$(printf '%s\n' "$out" |
		grep -oE '(^|[[:space:]])TUIC_APP_INSTANCE=[^[:space:]]*' | head -1) || true
	if [ -z "$match" ]; then
		printf '      make -n printed no TUIC_APP_INSTANCE= token at all\n' >&2
		echo '<no-token>'
		return
	fi
	printf '%s\n' "${match#*=}"
}

# `make -n` output for a run. No inherited opinion on the variable: a stray
# `export TUIC_APP_INSTANCE` in the developer's shell would otherwise mask the
# very defaults under test. `env:<value>` as the first argument sets it in the
# ENVIRONMENT instead (a different path through make's precedence model than a
# command-line assignment).
make_dry_run() {
	if [ "${1:-}" != "${1#env:}" ]; then
		local value="${1#env:}"
		shift
		TUIC_APP_INSTANCE="$value" MAKEFLAGS= make -n "$@" 2>&1
	else
		env -u TUIC_APP_INSTANCE MAKEFLAGS= make -n "$@" 2>&1
	fi
}

instance_for() {
	local out rc
	out=$(make_dry_run "$@") && rc=0 || rc=$?
	parsed_instance "$out" "$rc"
}

warns_for() { # prints yes/no: does this run print the shared-config warning?
	local out
	out=$(make_dry_run "$@") || true
	if printf '%s\n' "$out" | grep -q 'WARNING: TUIC_APP_INSTANCE is empty'; then
		echo yes
	else
		echo no
	fi
}

fail=0
expect() {
	local label="$1" want="$2" got="$3"
	if [ "$got" != "$want" ]; then
		echo "  ✗ $label: expected '$want', got '$got'"
		fail=1
	fi
}

# The per-checkout default: identical for both targets, a valid instance id
# (lowercase DNS label, at most 63 characters; docs/backend/config.md), and
# never the shared directory.
dev_default=$(instance_for dev)
expect "make test (same instance as make dev)" "$dev_default" "$(instance_for test)"
if ! printf '%s' "$dev_default" | grep -qE '^tuic-([a-z0-9]([a-z0-9-]{0,56}[a-z0-9])?)$'; then
	echo "  ✗ make dev: default TUIC_APP_INSTANCE='$dev_default' is not a per-checkout 'tuic-<label>' instance id"
	fail=1
fi
for target in dev test; do
	expect "make $target warns by default" no "$(warns_for "$target")"
	# An explicit id wins, from the command line and from the environment.
	expect "make $target TUIC_APP_INSTANCE=scope-check" "scope-check" \
		"$(instance_for "$target" TUIC_APP_INSTANCE=scope-check)"
	expect "TUIC_APP_INSTANCE=scope-check make $target" "scope-check" \
		"$(instance_for env:scope-check "$target")"
	expect "make $target TUIC_APP_INSTANCE=scope-check warns" no \
		"$(warns_for "$target" TUIC_APP_INSTANCE=scope-check)"
	# An EMPTY value is the one deliberate way back to the shared config, and
	# it must say so loudly, both ways.
	expect "make $target TUIC_APP_INSTANCE=" "" "$(instance_for "$target" TUIC_APP_INSTANCE=)"
	expect "make $target TUIC_APP_INSTANCE= warns" yes "$(warns_for "$target" TUIC_APP_INSTANCE=)"
	expect "TUIC_APP_INSTANCE= make $target" "" "$(instance_for env: "$target")"
	expect "TUIC_APP_INSTANCE= make $target warns" yes "$(warns_for env: "$target")"
done

# The expectations above name their targets, so a NEW target that launches the
# dev build without the instance passes until someone hand-adds a line for it.
# This catches the shape instead: every recipe line that runs `tauri dev` must
# hand it `TUIC_APP_INSTANCE=$(TUIC_APP_INSTANCE)`, and no target may carry its
# own `target: TUIC_APP_INSTANCE…` assignment that splits it off the shared
# per-checkout default again.
unscoped_launch=$(grep -nE '^	.*tauri dev' Makefile | grep -vF 'TUIC_APP_INSTANCE=$(TUIC_APP_INSTANCE)' || true)
if [ -n "$unscoped_launch" ]; then
	echo "  ✗ Makefile: a 'tauri dev' launch without TUIC_APP_INSTANCE=\$(TUIC_APP_INSTANCE):"
	printf '%s\n' "$unscoped_launch" | sed 's/^/      /'
	fail=1
fi
target_specific=$(grep -nE '^[^[:space:]#=]+:[[:space:]]*(export[[:space:]]+)?TUIC_APP_INSTANCE[[:space:]]*[?:+]?=' Makefile || true)
if [ -n "$target_specific" ]; then
	echo "  ✗ Makefile: target-specific TUIC_APP_INSTANCE assignment (keep the one global per-checkout default):"
	printf '%s\n' "$target_specific" | sed 's/^/      /'
	fail=1
fi

exit $fail
