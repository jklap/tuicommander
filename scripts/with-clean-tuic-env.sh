#!/usr/bin/env bash
#
# with-clean-tuic-env.sh — run a command without the ambient TUIC_* variables.
#
# A terminal opened by TUICommander exports TUIC_SESSION, TUIC_CONFIG_DIR,
# TUIC_WORKTREE_PATH, ... into every child. They leak into tests (a test then
# reads the real app's config dir or session) and, for crates whose build script
# tracks them, make a build differ from the one the gate later does.
#
# TUIC_TEST_* is kept: those are the deliberate test-harness opt-ins
# (TUIC_TEST_TMP_BASE, TUIC_TEST_FAKE_HOME, ...), not ambient state.
#
# Usage: scripts/with-clean-tuic-env.sh <command> [args...]
set -euo pipefail

if [ "$#" -eq 0 ]; then
	echo "usage: with-clean-tuic-env.sh <command> [args...]" >&2
	exit 2
fi

unset_args=()
while IFS= read -r name; do
	case "${name}" in
	TUIC_TEST_*) continue ;;
	esac
	unset_args+=(-u "${name}")
done < <(env | grep -oE '^TUIC_[A-Za-z0-9_]+' || true)

# ${arr[@]+...}: an empty array is an unbound variable under `set -u` on bash 3.2.
exec env ${unset_args[@]+"${unset_args[@]}"} "$@"
