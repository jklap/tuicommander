#!/usr/bin/env bash
set -euo pipefail

project_root="$(git rev-parse --show-toplevel)"
test_tmp="${TUIC_TEST_TMP_ROOT:-$project_root/.tmp/tuic-tests}"
mkdir -p "$test_tmp"
scratch="$(mktemp -d "$test_tmp/fixture-gate.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

git -C "$scratch" init -q repo
repo="$scratch/repo"
mkdir -p "$repo/src-tauri/src"
printf 'fn awaiting_input() { false }\n' > "$repo/src-tauri/src/pty.rs"
git -C "$repo" add src-tauri/src/pty.rs
git -C "$repo" -c core.hooksPath=/dev/null -c user.name=Test -c user.email=test@example.com commit -qm initial

printf 'fn awaiting_input() { true }\n' > "$repo/src-tauri/src/pty.rs"
git -C "$repo" add src-tauri/src/pty.rs
if (cd "$repo" && bash "$project_root/scripts/hooks/pre-commit") > "$scratch/blocked" 2>&1; then
  echo 'detection change unexpectedly passed without a fixture' >&2
  exit 1
fi
grep -Fq 'agent-state detection changed with no capture' "$scratch/blocked"
grep -Fq 'The output ring holds 2 MB' "$scratch/blocked"
grep -Fq '8192-byte output page limit' "$scratch/blocked"
if grep -Fq 'ring holds 8 KB' "$scratch/blocked"; then
  echo 'hook still reports the output page size as ring retention' >&2
  exit 1
fi

mkdir -p "$repo/src-tauri/src/fixtures/agent_prompts"
printf 'recorded fixture\n' > "$repo/src-tauri/src/fixtures/agent_prompts/recorded.raw"
git -C "$repo" add src-tauri/src/fixtures/agent_prompts/recorded.raw
(cd "$repo" && bash "$project_root/scripts/hooks/pre-commit")
