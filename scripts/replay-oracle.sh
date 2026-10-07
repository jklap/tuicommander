#!/usr/bin/env bash
# Explicitly regenerate or verify the recorded production replay oracles.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
case "${1:-verify}" in
  regenerate) export TUIC_REGENERATE_ORACLES=1 ;;
  verify) unset TUIC_REGENERATE_ORACLES ;;
  *) echo "usage: scripts/replay-oracle.sh [verify|regenerate]" >&2; exit 2 ;;
esac
runner=()
if [[ $(uname -s) == Darwin ]]; then
  export PATH="$HOME/Library/Application Support/mbx/bin:$PATH"
  which cargo
  mbx doctor
  if [[ -f "$HOME/Gits/.tmp/BUILD_FREEZE" ]] && ! grep -q "${TUIC_ORACLE_BUILD_OWNER:-tuic-1342}" "$HOME/Gits/.tmp/BUILD_FREEZE"; then
    echo 'BUILD_FREEZE does not authorize this oracle run' >&2
    exit 1
  fi
  runner=("$HOME/Gits/personal/orchestrator/tools/build/build-slot.sh")
fi
cd "$root/src-tauri"
exec "$root/scripts/with-test-tmp.sh" "${runner[@]}" cargo nextest run --lib -E 'test(replay_oracle)'
