#!/usr/bin/env bash
# Keep the working branch re-org friendly while committing to it.
#
#   wip-commit.sh check [<range>]          lint commits (default: what this branch adds)
#   wip-commit.sh suggest-fixup [<path>...] which commit to `--fixup` (default: staged paths)
#   wip-commit.sh land [--dry-run] [--personal-last [--yes]] [<branch>]
#   wip-commit.sh help
#
# check     range: <base>..<tip>, or <rev> meaning <rev>..HEAD. Default: merge-base(WIP_BRANCH,
#           HEAD)..HEAD on a feature branch, merge-base(WIP_UPSTREAM, HEAD)..HEAD on WIP_BRANCH.
#           Errors (exit 1): ledger files mixed with other files, personal paths outside the
#           personal commit, ledger/personal commits leaking other paths, empty and merge commits.
#           Warnings (exit 0): unsquashed fixup!/squash!/amend! whose target is in the range,
#           fixups whose target is nowhere, non-conventional subjects, subjects classify.py files
#           under `misc`, and on WIP_BRANCH commits above the personal commit.
# land      from the feature worktree: check, rebase -i --autosquash onto WIP_BRANCH (one
#           rebase, aborted on conflict so nothing changes), then fast-forward WIP_BRANCH, with
#           `merge --ff-only` in the worktree that has it checked out, else `fetch . b:wip`.
#           --personal-last rebases onto the commit below the personal tip and re-creates the
#           personal commit on top: WIP_BRANCH is rewritten by one commit, so it needs --yes.
#
# Config: <toplevel>/.claude/wip.conf, else the main worktree's, else $WIP_CONF (WIP_BRANCH,
# WIP_UPSTREAM, WIP_LEDGERS, WIP_PERSONAL_PATHS, WIP_PERSONAL_RE), each overridable from the
# environment, plus WIP_LEDGERS_RE (ledgers commit subject), WIP_CLASSIFY (classify.py path, empty
# to skip the misc check) and WIP_SUGGEST_MAX (commits listed per path).
set -euo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
SELF=$HERE/$(basename "${BASH_SOURCE[0]}")
US=$'\x1f' RS=$'\x1e'
MARK_RE='^(fixup|squash|amend)! (.*)$'
CC_RE='^[a-z]+(\([^)]+\))?!?: [^ ]'
HASH_RE='^[0-9a-f]{7,40}$'

die() { echo "wip-commit: $*" >&2; exit 1; }

load_config() {
  TOP=$(git rev-parse --show-toplevel 2>/dev/null) || die "not inside a git work tree"
  local conf=$TOP/.claude/wip.conf common
  if [[ ! -f $conf ]]; then
    common=$(git rev-parse --path-format=absolute --git-common-dir)
    conf=$(dirname "$common")/.claude/wip.conf
  fi
  [[ -f $conf ]] || conf=${WIP_CONF:-}
  CONF_FOUND=0
  if [[ -n $conf && -f $conf ]]; then
    # shellcheck source=/dev/null
    source "$conf"
    CONF_FOUND=1
  fi
  : "${WIP_BRANCH:=wip}" "${WIP_UPSTREAM:=origin/main}" "${WIP_LEDGERS:=}" "${WIP_PERSONAL_PATHS:=}"
  : "${WIP_PERSONAL_RE:=^personal scripts\$}" "${WIP_LEDGERS_RE:=^docs\\(ledgers\\)}"
  : "${WIP_CLASSIFY=$HERE/../../reorg-and-rebase-onto-main/scripts/classify.py}"
  : "${WIP_SUGGEST_MAX:=3}"
  read -r -a LEDGERS <<<"$WIP_LEDGERS"
  read -r -a PERSONAL <<<"$WIP_PERSONAL_PATHS"
}

resolve() { git rev-parse -q --verify "$1^{commit}" 2>/dev/null || die "cannot resolve $1"; }

is_ledger() {
  local l
  for l in ${LEDGERS[@]+"${LEDGERS[@]}"}; do [[ $1 == "$l" ]] && return 0; done
  return 1
}

is_personal_path() {
  local p
  for p in ${PERSONAL[@]+"${PERSONAL[@]}"}; do [[ $1 == "$p" || $1 == "$p"/* ]] && return 0; done
  return 1
}

# REPLY = the subject a fixup!/squash!/amend! chain points at (the subject itself otherwise).
target_subject() {
  REPLY=$1
  while [[ $REPLY =~ $MARK_RE ]]; do REPLY=${BASH_REMATCH[2]}; done
}

# REPLY = personal | ledgers | feature, from the Squash-Group trailer or the (target) subject.
role_of() {
  target_subject "$1"
  if [[ $2 == personal || $REPLY =~ $WIP_PERSONAL_RE ]]; then REPLY=personal
  elif [[ $2 == ledgers || $REPLY =~ $WIP_LEDGERS_RE ]]; then REPLY=ledgers
  else REPLY=feature
  fi
}

# Fill H/PAR/S/TR/FILES (oldest first) for <range>.
load_commits() {
  H=() PAR=() S=() TR=() FILES=()
  local log rec hdr rest files
  log=$(git log --reverse --no-renames --name-only \
    --format="${RS}%H${US}%P${US}%s${US}%(trailers:key=Squash-Group,valueonly,separator=)" "$@")
  while IFS= read -r -d "$RS" rec || [[ -n $rec ]]; do
    [[ -n ${rec//$'\n'/} ]] || continue
    hdr=${rec%%$'\n'*}
    files=''
    [[ $rec == *$'\n'* ]] && files=${rec#*$'\n'}
    H+=("${hdr%%"$US"*}"); rest=${hdr#*"$US"}
    PAR+=("${rest%%"$US"*}"); rest=${rest#*"$US"}
    S+=("${rest%%"$US"*}")
    rest=${rest#*"$US"}; rest=${rest//[[:space:]]/}
    TR+=("$rest")
    FILES+=("$files")
  done <<<"$log"
}

# Split <range> into RBASE/RTIP (commit ids) and RANGE.
parse_range() {
  local r=$1
  if [[ $r == *...* ]]; then die "use <base>..<tip>, not a symmetric range"; fi
  if [[ $r == *..* ]]; then
    RBASE=$(resolve "${r%%..*}"); RTIP=$(resolve "${r#*..}")
  else
    RBASE=$(resolve "$r"); RTIP=$(resolve HEAD)
  fi
  RANGE=$RBASE..$RTIP
}

current_branch() { git symbolic-ref -q --short HEAD || true; }

say() { local kind=$1 i=$2; shift 2; printf '%s: %s "%s": %s\n' "$kind" "${H[i]:0:10}" "${S[i]}" "$*"; }
err() { say error "$@"; CHK_ERR=$((CHK_ERR + 1)); }
warn() { say warning "$@"; CHK_WARN=$((CHK_WARN + 1)); }

# run_check <range> <on_wip:0|1>; sets CHK_ERR, CHK_WARN, CHK_FIXUPS (fixups with in-range targets).
run_check() {
  parse_range "$1"
  local on_wip=$2 i j n f t role ledg other pers nonpers up_subjects upbase target last_p=-1
  CHK_ERR=0 CHK_WARN=0 CHK_FIXUPS=0
  load_commits "$RANGE"
  n=${#H[@]}
  echo "check: $n commit(s) in ${RBASE:0:10}..${RTIP:0:10}"
  ((CONF_FOUND)) || echo "note: no .claude/wip.conf found; ledger/personal lists come from the environment only"
  up_subjects=''
  if upbase=$(git merge-base "$WIP_UPSTREAM" "$WIP_BRANCH" 2>/dev/null); then
    up_subjects=$(git log --format=%s "$upbase..$WIP_BRANCH")
  fi

  for ((i = 0; i < n; i++)); do
    role_of "${S[i]}" "${TR[i]}"; role=$REPLY
    if [[ ${PAR[i]} == *" "* ]]; then err "$i" "merge commit; land with rebase + fast-forward only"; continue; fi
    if [[ -z ${FILES[i]//$'\n'/} ]]; then err "$i" "empty commit; drop it"; continue; fi
    ledg=() other=() pers=() nonpers=()
    while IFS= read -r f; do
      [[ -n $f ]] || continue
      if is_ledger "$f"; then ledg+=("$f"); else other+=("$f"); fi
      if is_personal_path "$f"; then pers+=("$f"); elif [[ $f != .gitignore ]]; then nonpers+=("$f"); fi
    done <<<"${FILES[i]}"
    case $role in
      personal)
        ((${#nonpers[@]} == 0)) || err "$i" "personal commit also changes ${nonpers[*]}; commit those separately"
        ;;
      ledgers)
        ((${#other[@]} == 0)) || err "$i" "ledgers commit also changes ${other[*]}; commit those separately"
        ;;
      *)
        if ((${#ledg[@]} && ${#other[@]})); then
          err "$i" "mixes ledger files (${ledg[*]}) with other files; split the ledger edits into their own commit or a --fixup of the ledgers commit"
        fi
        ((${#pers[@]} == 0)) || err "$i" "touches personal paths (${pers[*]}); those belong only in the personal commit"
        ;;
    esac
    [[ $role == personal ]] && ! [[ ${S[i]} =~ $MARK_RE ]] && last_p=$i

    if [[ ${S[i]} =~ $MARK_RE ]]; then
      target_subject "${S[i]}"; t=$REPLY target=''
      for ((j = 0; j < i; j++)); do
        if [[ ${S[j]} == "$t" ]] || { [[ $t =~ $HASH_RE ]] && [[ ${H[j]} == "$t"* ]]; }; then target=${H[j]}; break; fi
      done
      if [[ -n $target ]]; then
        CHK_FIXUPS=$((CHK_FIXUPS + 1))
        warn "$i" "unsquashed; its target ${target:0:10} is in this range (land squashes it, or: git rebase -i --autosquash ${RBASE:0:10})"
      elif grep -qxF -- "$t" <<<"$up_subjects"; then
        echo "note: ${H[i]:0:10} \"${S[i]}\": targets a $WIP_BRANCH commit; it squashes at the next re-org"
      elif [[ $t =~ $HASH_RE ]] && git merge-base --is-ancestor "$t" "$WIP_BRANCH" 2>/dev/null; then
        echo "note: ${H[i]:0:10} \"${S[i]}\": targets $WIP_BRANCH commit ${t:0:10}; it squashes at the next re-org"
      else
        warn "$i" "no commit with subject \"$t\" on $WIP_BRANCH or in this range; autosquash will ignore it"
      fi
    elif [[ $role == feature ]] && ! [[ ${S[i]} =~ $CC_RE ]]; then
      warn "$i" "not a conventional subject (type(scope): summary); classify.py groups by its scope"
    fi
  done

  if ((on_wip && last_p >= 0)); then
    local above=()
    for ((i = last_p + 1; i < n; i++)); do
      role_of "${S[i]}" "${TR[i]}"
      [[ $REPLY == personal ]] || above+=("${H[i]:0:10}")
    done
    if ((${#above[@]})); then
      warn "$last_p" "${#above[@]} commit(s) sit above this personal commit (${above[*]}); fix: git rebase -i ${H[last_p]:0:10}^ and move its pick line last, or leave it for the next re-org"
    fi
  fi

  if [[ -n $WIP_CLASSIFY && -f $WIP_CLASSIFY ]] && ((n)) && command -v python3 >/dev/null; then
    local h g s
    while IFS=$'\t' read -r h g s; do
      [[ $g == misc ]] || continue
      [[ $s =~ $MARK_RE ]] && continue
      for ((i = 0; i < n; i++)); do [[ ${H[i]} == "$h"* ]] && break; done
      warn "$i" "classify.py files it under misc; use a scope it maps or add a Squash-Group: trailer"
    done <<<"$(python3 "$WIP_CLASSIFY" "$TOP" "$RBASE" "$RTIP" 2>/dev/null || true)"
  fi
  echo "check: $CHK_ERR error(s), $CHK_WARN warning(s)"
}

default_range() {
  local cur
  cur=$(current_branch)
  if [[ $cur == "$WIP_BRANCH" ]]; then
    REPLY="$(git merge-base "$WIP_UPSTREAM" HEAD 2>/dev/null || die "no merge-base between $WIP_UPSTREAM and HEAD")..HEAD"
  else
    REPLY="$(git merge-base "$WIP_BRANCH" HEAD 2>/dev/null || die "no merge-base between $WIP_BRANCH and HEAD")..HEAD"
  fi
}

cmd_check() {
  local range=${1:-} on_wip=0
  [[ $(current_branch) == "$WIP_BRANCH" ]] && on_wip=1
  if [[ -z $range ]]; then default_range; range=$REPLY; fi
  run_check "$range" "$on_wip"
  ((CHK_ERR == 0))
}

cmd_suggest_fixup() {
  local paths=() p upbase revs=() i k h line kind targets=() tpaths=() lc='' pc=''
  if (($#)); then paths=("$@"); else
    while IFS= read -r p; do [[ -n $p ]] && paths+=("$p"); done <<<"$(git diff --cached --name-only --no-renames)"
  fi
  ((${#paths[@]})) || die "nothing staged; stage the follow-up or name the paths"
  upbase=$(git merge-base "$WIP_UPSTREAM" "$WIP_BRANCH" 2>/dev/null) || die "no merge-base between $WIP_UPSTREAM and $WIP_BRANCH"
  revs=("$WIP_BRANCH")
  [[ $(git rev-parse HEAD) != "$(git rev-parse "$WIP_BRANCH")" ]] && revs+=(HEAD)
  load_commits --topo-order "${revs[@]}" --not "$upbase"
  # load_commits reversed the topo order: walk from the end to get the newest first.
  for ((i = ${#H[@]} - 1; i >= 0; i--)); do
    [[ ${S[i]} =~ $MARK_RE ]] && continue
    role_of "${S[i]}" "${TR[i]}"
    [[ $REPLY == ledgers && -z $lc ]] && lc=${H[i]}
    [[ $REPLY == personal && -z $pc ]] && pc=${H[i]}
  done

  add_target() {
    local x
    for ((x = 0; x < ${#targets[@]}; x++)); do
      if [[ ${targets[x]} == "$1" ]]; then tpaths[x]+=" $2"; return; fi
    done
    targets+=("$1"); tpaths+=("$2")
  }
  find_idx() { for ((k = 0; k < ${#H[@]}; k++)); do [[ ${H[k]} == "$1" ]] && return 0; done; return 1; }

  for p in "${paths[@]}"; do
    echo "$p"
    if is_ledger "$p"; then
      if [[ -n $lc ]]; then
        echo "  ledger: commit it alone, or fold it into the ledgers commit ${lc:0:10}"
        echo "  -> git commit --fixup=${lc:0:10}"; add_target "${lc:0:10}" "$p"
      else
        echo "  ledger: commit it alone as docs(ledgers): ... (no ledgers commit on $WIP_BRANCH yet)"
      fi
      continue
    fi
    if is_personal_path "$p"; then
      if [[ -n $pc ]]; then
        echo "  personal: never mix it with project files; fold it into the personal commit ${pc:0:10}"
        echo "  -> git commit --fixup=${pc:0:10}"; add_target "${pc:0:10}" "$p"
      else
        echo "  personal: commit it alone with a subject matching WIP_PERSONAL_RE"
      fi
      continue
    fi
    local first=''
    while IFS= read -r h; do
      [[ -n $h ]] || continue
      find_idx "$h" || continue
      kind=''
      git merge-base --is-ancestor "$h" "$WIP_BRANCH" || kind='  (this branch only)'
      line="  ${h:0:10} [${TR[k]:--}] ${S[k]}$kind"
      if [[ ${S[k]} =~ $MARK_RE ]]; then
        target_subject "${S[k]}"
        local t=$REPLY m
        for ((m = ${#H[@]} - 1; m >= 0; m--)); do
          if [[ ${S[m]} == "$t" ]]; then line+="  (fixes ${H[m]:0:10})"; [[ -n $first ]] || first=${H[m]}; break; fi
        done
      else
        [[ -n $first ]] || first=$h
      fi
      echo "$line"
    done <<<"$(git log --topo-order --format=%H -n "$WIP_SUGGEST_MAX" "${revs[@]}" --not "$upbase" -- "$p")"
    if [[ -n $first ]]; then
      echo "  -> git commit --fixup=${first:0:10}"; add_target "${first:0:10}" "$p"
    else
      echo "  no commit since $WIP_UPSTREAM touched it: a new feature commit (scope from classify.py, or a Squash-Group: trailer)"
    fi
  done

  if ((${#targets[@]} == 1)) && (($# == 0)); then
    echo "ready: git commit --fixup=${targets[0]}"
  elif ((${#targets[@]})); then
    echo "ready (each commits the named paths' full working-tree content):"
    for ((i = 0; i < ${#targets[@]}; i++)); do echo "  git commit --fixup=${targets[i]} --${tpaths[i]/#/ }"; done
  fi
}

# Path of the worktree that has refs/heads/<branch> checked out, or empty.
worktree_of() {
  local line wt='' want="branch refs/heads/$1"
  while IFS= read -r line; do
    case $line in
      "worktree "*) wt=${line#worktree } ;;
      "$want") echo "$wt"; return ;;
    esac
  done <<<"$(git worktree list --porcelain)"
}

# Fails when any worktree is mid-rebase of <branch>, or <path> has an operation in progress.
assert_quiet() {
  local line p hn
  while IFS= read -r line; do
    [[ $line == "worktree "* ]] || continue
    p=${line#worktree }
    for hn in rebase-merge/head-name rebase-apply/head-name; do
      hn=$(git -C "$p" rev-parse --path-format=absolute --git-path "$hn" 2>/dev/null) || continue
      [[ -f $hn && $(cat "$hn") == "refs/heads/$1" ]] && die "$p is rebasing $1; finish that first"
    done
  done <<<"$(git worktree list --porcelain)"
  if [[ -n ${2:-} ]]; then
    for hn in rebase-merge rebase-apply MERGE_HEAD CHERRY_PICK_HEAD REVERT_HEAD; do
      [[ -e $(git -C "$2" rev-parse --path-format=absolute --git-path "$hn") ]] && die "$2 has a $hn in progress"
    done
  fi
  return 0
}

run() {
  printf '+'; printf ' %q' "$@"; echo
  ((DRY)) || "$@"
}

# Sequence editor for land: append one pick per hash in WIP_APPEND_PICKS to the todo.
cmd_seq_append() {
  local h
  for h in ${WIP_APPEND_PICKS:-}; do echo "pick $h" >>"$1"; done
}

cmd_land() {
  local branch='' plast=0 yes=0 wt wipwt w orig base tail=() tailbase onto picks='' n i
  DRY=0
  while (($#)); do
    case $1 in
      --dry-run) DRY=1 ;;
      --personal-last) plast=1 ;;
      --yes) yes=1 ;;
      -*) die "land: unknown option $1" ;;
      *) branch=$1 ;;
    esac
    shift
  done
  [[ -n $branch ]] || branch=$(current_branch)
  [[ -n $branch ]] || die "land: HEAD is detached; name the branch"
  [[ $branch != "$WIP_BRANCH" ]] || die "land: run it on the feature branch, not $WIP_BRANCH"
  git show-ref --verify -q "refs/heads/$branch" || die "land: '$branch' is not a local branch"
  w=$(resolve "refs/heads/$WIP_BRANCH")
  orig=$(resolve "refs/heads/$branch")
  wt=$(worktree_of "$branch")
  [[ -n $wt ]] || die "land: $branch is not checked out in any worktree (land rebases it in place)"
  wipwt=$(worktree_of "$WIP_BRANCH")
  assert_quiet "$WIP_BRANCH" "$wt"
  assert_quiet "$branch" "$wipwt"
  [[ -z $(git -C "$wt" status --porcelain --untracked-files=no) ]] || die "land: $wt has uncommitted changes"

  base=$(git merge-base "$w" "$orig") || die "land: $branch shares no history with $WIP_BRANCH"
  run_check "$base..$orig" 0
  ((CHK_ERR == 0)) || die "land: refusing, check found $CHK_ERR error(s)"
  n=$(git rev-list --count "$w..$orig")
  ((n)) || { echo "land: $branch has nothing $WIP_BRANCH lacks"; return 0; }

  onto=$w
  if ((plast)); then
    load_commits -n 64 "$w"
    for ((i = ${#H[@]} - 1; i >= 0; i--)); do
      role_of "${S[i]}" "${TR[i]}"
      [[ $REPLY == personal ]] || break
      tail=("${H[i]}" ${tail[@]+"${tail[@]}"})
    done
    if ((${#tail[@]} == 0)); then
      echo "land: $WIP_BRANCH's tip is not a personal commit; --personal-last falls back to a plain fast-forward"
      plast=0
    else
      tailbase=$(resolve "${tail[0]}^")
      onto=$tailbase picks="${tail[*]}"
      ((yes || DRY)) || die "land: --personal-last re-creates ${#tail[@]} personal commit(s) (${tail[0]:0:10}) on top, rewriting $WIP_BRANCH (not a fast-forward); re-run with --yes"
    fi
  fi

  local seq=true
  [[ -n $picks ]] && seq="$(printf %q "$SELF") _seq-append"
  if [[ $onto != "$base" || $CHK_FIXUPS -gt 0 || -n $picks ]]; then
    if ! run env GIT_SEQUENCE_EDITOR="$seq" WIP_APPEND_PICKS="$picks" GIT_EDITOR=true \
      git -C "$wt" rebase -q -i --autosquash --onto "$onto" "$w"; then
      local conflicts
      conflicts=$(git -C "$wt" diff --name-only --diff-filter=U | tr '\n' ' ')
      git -C "$wt" rebase --abort 2>/dev/null || true
      die "land: rebasing $branch onto $WIP_BRANCH conflicts (${conflicts:-see git status}); aborted, nothing changed. Resolve by hand in $wt: git rebase -i --autosquash $WIP_BRANCH, then land again"
    fi
  else
    echo "land: $branch already sits on $WIP_BRANCH with no fixups of its own; no rebase needed"
  fi
  ((DRY)) && { echo "land: dry run, nothing changed"; return 0; }

  local new
  new=$(resolve "refs/heads/$branch")
  if ((plast)); then
    if [[ -n $wipwt ]]; then
      [[ $(git -C "$wipwt" rev-parse HEAD) == "$w" ]] || die "land: $WIP_BRANCH moved during land; nothing landed"
      run git -C "$wipwt" reset -q --keep "$new" || die "land: $wipwt refused the update (local changes?); $branch is rebased, $WIP_BRANCH unchanged"
    else
      run git update-ref -m "wip-commit land --personal-last $branch" "refs/heads/$WIP_BRANCH" "$new" "$w"
    fi
    echo "land: re-created personal $(printf '%.10s ' "${tail[@]}")as $(git rev-list --reverse "$new~${#tail[@]}..$new" | cut -c1-10 | tr '\n' ' ')"
    echo "land: $WIP_BRANCH ${w:0:10} -> ${new:0:10} (personal re-created on top; not a fast-forward)"
  else
    git merge-base --is-ancestor "$w" "$new" || die "land: $branch does not contain $WIP_BRANCH after the rebase; refusing"
    if [[ -n $wipwt ]]; then
      run git -C "$wipwt" merge -q --ff-only "$new" || die "land: $wipwt refused the fast-forward (local changes?); $branch is rebased, $WIP_BRANCH unchanged"
    else
      run git -C "$wt" fetch -q . "refs/heads/$branch:refs/heads/$WIP_BRANCH"
    fi
    echo "land: $WIP_BRANCH fast-forwarded ${w:0:10} -> ${new:0:10} ($(git rev-list --count "$w..$new") commit(s))"
    load_commits -n 1 "$w"
    role_of "${S[0]}" "${TR[0]}"
    [[ $REPLY == personal ]] && echo "note: the new commits sit above the personal commit; the next re-org moves it back (or land with --personal-last --yes)"
  fi
  return 0
}

main() {
  local sub=${1:-help}
  (($#)) && shift
  case $sub in
    help|-h|--help) sed -n '2,24p' "$SELF" | sed 's/^# \{0,1\}//'; return 0 ;;
    _seq-append) cmd_seq_append "$@"; return ;;
  esac
  load_config
  case $sub in
    check) cmd_check "$@" ;;
    suggest-fixup) cmd_suggest_fixup "$@" ;;
    land) cmd_land "$@" ;;
    *) die "unknown subcommand '$sub' (try help)" ;;
  esac
}

main "$@"
