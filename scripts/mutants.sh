#!/usr/bin/env bash
# Mutation-test the Rust changes of a git range with cargo-mutants.
#
#   scripts/mutants.sh [RANGE]      default RANGE=HEAD~1 (that commit vs HEAD)
#
# Tests the committed tree at HEAD: commit first. Only mutants overlapping the
# diff run (`--in-diff`); a full-tree run is N mutants x one incremental build
# of the lib crate and is deliberately not offered.
#
# Runs `--in-place` in a disposable worktree under .tmp/ instead of the copy
# cargo-mutants makes by default: `tauri::generate_context!` needs `../dist`
# next to src-tauri, which a copy of the workspace root does not have, and the
# copy would land in $TMPDIR where every freshly built test binary is scanned
# on exec. `--in-place` implies one job, which is also the budget.
set -euo pipefail

RANGE="${1:-HEAD~1}"
ROOT="$(git rev-parse --show-toplevel)"
WT="$ROOT/.tmp/wt-mutants"
DIFF="$ROOT/.tmp/mutants.diff"

# Every cargo call goes through the mbx shim when it is installed (shared
# artifact cache + machine-wide compile scheduler); CI has no mbx and runs
# plain cargo.
MBX_BIN="$HOME/Library/Application Support/mbx/bin"
[ -d "$MBX_BIN" ] && export PATH="$MBX_BIN:$PATH"

command -v cargo-mutants >/dev/null || { echo "cargo-mutants missing: cargo install --locked cargo-mutants"; exit 1; }
[ -d "$ROOT/dist" ] || { echo "$ROOT/dist missing — run 'pnpm exec vite build' first"; exit 1; }
ls "$ROOT/src-tauri/binaries"/tuic-bridge-* >/dev/null 2>&1 || { echo "sidecar missing — run 'pnpm build:sidecar' first"; exit 1; }

mkdir -p "$ROOT/.tmp"
# Paths relative to src-tauri, with the b/ prefix --in-diff expects.
git -C "$ROOT" diff --relative=src-tauri "$RANGE" HEAD -- src-tauri > "$DIFF"
if ! grep -q '^+++ b/.*\.rs$' "$DIFF"; then
  echo "no Rust changes in $RANGE..HEAD"; exit 0
fi

if [ -d "$WT" ]; then
  git -C "$WT" checkout -q -- . && git -C "$WT" checkout -q --detach "$(git -C "$ROOT" rev-parse HEAD)"
else
  git -C "$ROOT" worktree add -q --detach "$WT" HEAD
fi
# generate_context! embeds ../dist; tauri-build checks the gitignored sidecar.
mkdir -p "$WT/dist" && cp -R "$ROOT/dist/." "$WT/dist/"
cp -R "$ROOT/src-tauri/binaries/." "$WT/src-tauri/binaries/"
# cargo-mutants restores each mutant itself; this covers an interrupted run.
trap 'git -C "$WT" checkout -q -- . 2>/dev/null || true' EXIT

cd "$WT/src-tauri"
ulimit -n 10240
cargo mutants --in-place --in-diff "$DIFF" "${@:2}"
STATUS=$?
echo "--- missed (a surviving mutant is a missing test):"
cat mutants.out/missed.txt 2>/dev/null || true
echo "logs: $WT/src-tauri/mutants.out"
exit $STATUS
