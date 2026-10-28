#!/usr/bin/env bash
# Rewrite the working branch into feature-squashed commits on its CURRENT base, final tree unchanged.
#
#   squash-wip.sh [--dry-run] [--out DIR] [<repo>]   (repo: current checkout; DIR: $TMPDIR/squash-wip-out)
#
#   1. strip   in a throwaway bare clone, reset the ledger and personal paths to the base's
#              version in every commit (a tree-level rewrite: no patches are applied, so it
#              cannot conflict); commits left empty are pruned
#   2. plan    plan.py groups commits by feature, moves one only when it and everything after
#              it still merge cleanly, then writes the new commits with git commit-tree
#   3. verify  the new tip's tree must equal the working branch's, byte for byte
#
# Config: <repo>/.claude/wip.conf (WIP_BRANCH, WIP_UPSTREAM, WIP_LEDGERS, WIP_PERSONAL_PATHS,
# WIP_PERSONAL_RE), each overridable from the environment.
#
# Without --dry-run the ONLY change to <repo> is a non-forced fetch of refs/heads/<branch>-squashed,
# which fails rather than overwrite an existing one. The branch itself, its remote and the
# working tree are never touched: inspect <branch>-squashed, then move the branch yourself.
set -euo pipefail

DRY=0 OUT='' REPO=''
while (($#)); do
  case $1 in
    --dry-run) DRY=1 ;;
    --out) OUT=$2; shift ;;
    -h|--help) sed -n '2,19p' "$0"; exit 0 ;;
    *) REPO=$1 ;;
  esac
  shift
done
REPO=$(git -C "${REPO:-.}" rev-parse --show-toplevel)
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=/dev/null
[[ -f $REPO/.claude/wip.conf ]] && source "$REPO/.claude/wip.conf"
: "${WIP_BRANCH:=wip}" "${WIP_UPSTREAM:=origin/main}" "${WIP_LEDGERS:=}" "${WIP_PERSONAL_PATHS:=}"
: "${WIP_PERSONAL_RE:=^personal scripts\$}"
read -r -a LEDGERS <<<"$WIP_LEDGERS"
read -r -a PERSONAL <<<"$WIP_PERSONAL_PATHS"
STRIP_PATHS=("${LEDGERS[@]}" "${PERSONAL[@]}")
RESULT=$WIP_BRANCH-squashed

WORK=$(mktemp -d "${TMPDIR:?TMPDIR must be set}/squash-wip.XXXXXX")
trap 'rm -rf "$WORK"' EXIT
OUT=${OUT:-$TMPDIR/squash-wip-out}   # kept after exit: blocks.md is the reviewable todo
mkdir -p "$OUT"

if (( !DRY )) && git -C "$REPO" show-ref --verify --quiet "refs/heads/$RESULT"; then
  echo "squash-wip: refs/heads/$RESULT already exists in $REPO; delete it or rename it first" >&2
  exit 1
fi

# A bare clone keeps every intermediate ref out of <repo>, and filter-branch refuses a
# checkout with uncommitted changes anyway.
R=$WORK/clone.git
git clone -q --bare --no-local "$REPO" "$R"
up_ref=$(git -C "$REPO" rev-parse --symbolic-full-name "$WIP_UPSTREAM" 2>/dev/null || true)
[[ -n $up_ref ]] || up_ref=refs/heads/$WIP_UPSTREAM
git -C "$R" fetch -q --no-recurse-submodules "$REPO" "+$up_ref:refs/squash/upstream" "+refs/heads/$WIP_BRANCH:refs/squash/orig"
cd "$R"

TIP=$(git rev-parse refs/squash/orig)
BASE=$(git merge-base refs/squash/orig refs/squash/upstream)
git log --format=%s "$BASE..refs/squash/upstream" > "$OUT/upstream-subjects.txt"

echo "== 1. strip ${STRIP_PATHS[*]:-nothing} from $(git rev-list --count "$BASE..$TIP") commits"
git update-ref refs/squash/stripped "$TIP"
if ((${#STRIP_PATHS[@]})); then
  # ls-tree output is valid --index-info input: this resets each path to the base's blobs.
  FILTER_BRANCH_SQUELCH_WARNING=1 git filter-branch -f --prune-empty --index-filter "
    git rm -r -q --cached --ignore-unmatch -- ${STRIP_PATHS[*]}
    git ls-tree -r $BASE -- ${STRIP_PATHS[*]} | git update-index --index-info
  " -- "$BASE..refs/squash/stripped" >/dev/null 2>&1
fi
echo "   $(git rev-list --count "$BASE..refs/squash/stripped") commits remain"

echo "== 2. plan and write commits"
NEW=$(python3 "$HERE/plan.py" --repo "$R" --base "$BASE" --stripped refs/squash/stripped --orig "$TIP" \
  --out "$OUT" --ledgers "${LEDGERS[*]}" --personal-paths "${PERSONAL[*]}" \
  --personal-re "$WIP_PERSONAL_RE" --main-subjects "$OUT/upstream-subjects.txt")

echo "== 3. verify"
echo "   $WIP_BRANCH: $(git rev-list --count "$BASE..$TIP") commits -> $(git rev-list --count "$BASE..$NEW")"
if ! git diff --quiet "$TIP" "$NEW"; then echo "   TREES DIFFER" >&2; exit 1; fi
echo "   trees identical: OK"
echo "   todo: $OUT/blocks.md"

if (( DRY )); then
  echo "   dry run: nothing written to $REPO"
else
  git update-ref "refs/heads/$RESULT" "$NEW"
  # --no-recurse-submodules: with submodule.recurse=true this fetch would otherwise reach out
  # to every submodule's remote (and its SSH agent) just to copy one local ref.
  git -C "$REPO" fetch -q --no-recurse-submodules "$R" "refs/heads/$RESULT:refs/heads/$RESULT"
  echo "   created refs/heads/$RESULT in $REPO"
fi
