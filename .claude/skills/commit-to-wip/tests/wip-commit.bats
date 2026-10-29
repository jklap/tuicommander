#!/usr/bin/env bats
# wip-commit.sh against throwaway repos. Hermetic: git reads only a gitconfig in
# $BATS_TEST_TMPDIR, and every repo lives under it.
#
# Fixture: main = base; wip = main + feat(worktree) + feat(git) + docs(ledgers) + personal scripts.
# The main worktree has wip checked out; WIP_UPSTREAM=main via the repo's .claude/wip.conf.

SCRIPT="$BATS_TEST_DIRNAME/../scripts/wip-commit.sh"

setup() {
  unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE WIP_BRANCH WIP_UPSTREAM WIP_LEDGERS \
    WIP_PERSONAL_PATHS WIP_PERSONAL_RE WIP_LEDGERS_RE WIP_CONF WIP_SUGGEST_MAX
  export GIT_CONFIG_GLOBAL="$BATS_TEST_TMPDIR/gitconfig" GIT_CONFIG_NOSYSTEM=1
  export GIT_CEILING_DIRECTORIES="$BATS_TEST_TMPDIR"
  export WIP_CLASSIFY=''
  git config --global user.name Tester
  git config --global user.email tester@example.com
  git config --global init.defaultBranch main
  git config --global commit.gpgsign false

  R="$BATS_TEST_TMPDIR/repo"
  git init -q "$R"
  mkdir -p "$R/src" "$R/.claude"
  printf 'a1\na2\na3\n' >"$R/src/a.txt"
  printf 'b1\n' >"$R/src/b.txt"
  printf '# Changelog\n' >"$R/CHANGELOG.md"
  printf '# To test\n' >"$R/to-test.md"
  printf 'echo build\n' >"$R/build.sh"
  printf 'target/\n' >"$R/.gitignore"
  git -C "$R" add -A
  git -C "$R" commit -qm "chore: base"
  cat >"$R/.claude/wip.conf" <<'EOF'
WIP_BRANCH=${WIP_BRANCH:-wip}
WIP_UPSTREAM=${WIP_UPSTREAM:-main}
WIP_LEDGERS=${WIP_LEDGERS:-"CHANGELOG.md to-test.md .gitignore"}
WIP_PERSONAL_PATHS=${WIP_PERSONAL_PATHS:-"build.sh tools .claude/wip.conf"}
WIP_PERSONAL_RE=${WIP_PERSONAL_RE:-'^personal scripts$'}
EOF
  git -C "$R" switch -qc wip
  edit src/a.txt 1 A1;            commit "feat(worktree): change a" worktree
  printf 'b2\n' >>"$R/src/b.txt"; commit "feat(git): change b" git
  printf -- '- a\n' >>"$R/CHANGELOG.md"; printf -- '- a\n' >>"$R/to-test.md"
  commit "docs(ledgers): wip entries" ledgers
  printf 'echo more\n' >>"$R/build.sh"; printf '.obsidian/\n' >>"$R/.gitignore"
  commit "personal scripts" personal
}

# edit <file> <line> <text>: replace one line of a file in ${C:-$R}.
edit() {
  local f="${C:-$R}/$1" tmp="${C:-$R}/$1.tmp"
  awk -v n="$2" -v t="$3" 'NR == n { print t; next } { print }' "$f" >"$tmp" && mv "$tmp" "$f"
}

# commit <subject> [<group>] in ${C:-$R}, staging everything tracked plus new files.
commit() {
  local dir=${C:-$R}
  git -C "$dir" add -A -- . ':!.claude/wip.conf'
  if [[ -n ${2:-} ]]; then
    git -C "$dir" commit -qm "$1" -m "Squash-Group: $2"
  else
    git -C "$dir" commit -qm "$1"
  fi
}

# A linked worktree on branch <name>, forked from wip; sets W.
feature() {
  W="$BATS_TEST_TMPDIR/wt-$1"
  git -C "$R" worktree add -q -b "$1" "$W" wip
}

wc_() { (cd "${C:-$R}" && "$SCRIPT" "$@"); }

@test "help prints usage" {
  run "$SCRIPT" help
  [ "$status" -eq 0 ]
  [[ $output == *"suggest-fixup"* && $output == *"--personal-last"* ]]
}

@test "check: a clean wip passes" {
  run wc_ check
  [ "$status" -eq 0 ]
  [[ $output == *"4 commit(s)"* && $output == *"0 error(s), 0 warning(s)"* ]]
}

@test "check: a commit mixing ledger and code files is an error" {
  feature f1
  C=$W edit src/b.txt 1 B1
  printf -- '- f1\n' >>"$W/CHANGELOG.md"
  C=$W commit "feat(git): mixed"
  C=$W run wc_ check
  [ "$status" -eq 1 ]
  [[ $output == *'error:'*'"feat(git): mixed": mixes ledger files (CHANGELOG.md)'* ]]
  [[ $output == *"1 commit(s)"* ]]
}

@test "check: personal paths outside the personal commit are an error" {
  feature f1
  mkdir -p "$W/tools" && printf 'x\n' >"$W/tools/x.sh"
  printf 'c\n' >"$W/src/c.txt"
  C=$W commit "feat(worktree): add c and a tool"
  C=$W run wc_ check
  [ "$status" -eq 1 ]
  [[ $output == *"touches personal paths (tools/x.sh)"* ]]
}

@test "check: a personal commit leaking project files is an error" {
  printf 'x\n' >"$R/src/leak.txt"; printf 'echo 3\n' >>"$R/build.sh"
  commit "personal scripts"
  run wc_ check
  [ "$status" -eq 1 ]
  [[ $output == *"personal commit also changes src/leak.txt"* ]]
}

@test "check: fixups are warnings for in-range targets and notes for wip targets" {
  feature f1
  printf 'c\n' >"$W/src/c.txt"; C=$W commit "feat(worktree): add c"
  printf 'c2\n' >>"$W/src/c.txt"; C=$W commit "fixup! feat(worktree): add c"
  printf 'b3\n' >>"$W/src/b.txt"; C=$W commit "fixup! feat(git): change b"
  printf 'b4\n' >>"$W/src/b.txt"; C=$W commit "fixup! feat(git): no such commit"
  C=$W run wc_ check
  [ "$status" -eq 0 ]
  [[ $output == *'"fixup! feat(worktree): add c": unsquashed; its target'* ]]
  [[ $output == *'note:'*'"fixup! feat(git): change b": targets a wip commit'* ]]
  [[ $output == *'"fixup! feat(git): no such commit": no commit with subject'* ]]
  [[ $output == *"0 error(s), 2 warning(s)"* ]]
}

@test "check: empty commits are errors, non-conventional subjects warnings" {
  feature f1
  git -C "$W" commit -q --allow-empty -m "feat(git): nothing"
  printf 'c\n' >"$W/src/c.txt"; C=$W commit "Add c"
  C=$W run wc_ check
  [ "$status" -eq 1 ]
  [[ $output == *'"feat(git): nothing": empty commit'* ]]
  [[ $output == *'"Add c": not a conventional subject'* ]]
}

@test "check: on wip, commits above the personal commit are a warning with the fix" {
  printf 'b3\n' >>"$R/src/b.txt"; commit "feat(git): later"
  run wc_ check
  [ "$status" -eq 0 ]
  [[ $output == *'warning:'*'"personal scripts": 1 commit(s) sit above this personal commit'* ]]
  [[ $output == *"git rebase -i "*"^ and move its pick line last"* ]]
}

@test "check: an explicit range is honoured" {
  run wc_ check main..wip~2
  [ "$status" -eq 0 ]
  [[ $output == *"2 commit(s)"* ]]
}

@test "suggest-fixup: names the last commit, its group and a ready fixup line" {
  local h l p
  h=$(git -C "$R" rev-parse --short=10 wip~3)
  l=$(git -C "$R" rev-parse --short=10 wip~1)
  p=$(git -C "$R" rev-parse --short=10 wip)
  edit src/a.txt 2 A2 && git -C "$R" add src/a.txt
  run wc_ suggest-fixup
  [ "$status" -eq 0 ]
  [[ $output == *"$h [worktree] feat(worktree): change a"* ]]
  [[ $output == *"ready: git commit --fixup=$h"* ]]

  run wc_ suggest-fixup CHANGELOG.md build.sh src/new.txt
  [ "$status" -eq 0 ]
  [[ $output == *"ledger: commit it alone, or fold it into the ledgers commit $l"* ]]
  [[ $output == *"personal: never mix it"*"$p"* ]]
  [[ $output == *"src/new.txt"*"no commit since main touched it"* ]]
  [[ $output == *"git commit --fixup=$l -- CHANGELOG.md"* ]]
}

@test "suggest-fixup: on a feature branch, sees the branch's own commits and resolves fixups" {
  feature f1
  printf 'c\n' >"$W/src/c.txt"; C=$W commit "feat(worktree): add c"
  local h
  h=$(git -C "$W" rev-parse --short=10 HEAD)
  printf 'c2\n' >>"$W/src/c.txt"; C=$W commit "fixup! feat(worktree): add c"
  C=$W run wc_ suggest-fixup src/c.txt
  [ "$status" -eq 0 ]
  [[ $output == *"(this branch only)"*"(fixes $h)"* ]]
  [[ $output == *"git commit --fixup=$h -- src/c.txt"* ]]
}

@test "land: from a linked worktree fast-forwards the checked-out wip and autosquashes" {
  feature f1
  printf 'c\n' >"$W/src/c.txt"; C=$W commit "feat(worktree): add c"
  printf 'c2\n' >>"$W/src/c.txt"; C=$W commit "fixup! feat(worktree): add c"
  printf 'b3\n' >>"$W/src/b.txt"; C=$W commit "fixup! feat(git): change b"
  # wip moves on meanwhile, so land has to rebase
  edit src/a.txt 3 A3; commit "feat(worktree): change a3"
  local old
  old=$(git -C "$R" rev-parse wip)
  C=$W run wc_ land
  [ "$status" -eq 0 ]
  [[ $output == *"wip fast-forwarded"* ]]
  [ "$(git -C "$R" rev-parse wip)" = "$(git -C "$W" rev-parse HEAD)" ]
  git -C "$R" merge-base --is-ancestor "$old" wip
  [ -z "$(git -C "$R" rev-list --merges main..wip)" ]
  run git -C "$R" log --format=%s "$old..wip"
  [ "${lines[0]}" = "fixup! feat(git): change b" ]
  [ "${lines[1]}" = "feat(worktree): add c" ]
  [ "${#lines[@]}" -eq 2 ]
  [ "$(cat "$R/src/c.txt")" = "$(printf 'c\nc2')" ]
  [ -z "$(git -C "$R" status --porcelain --untracked-files=no)" ]
}

@test "land: updates wip by fetch when it is not checked out anywhere" {
  feature f1
  git -C "$R" switch -q main
  printf 'c\n' >"$W/src/c.txt"; C=$W commit "feat(worktree): add c"
  C=$W run wc_ land
  [ "$status" -eq 0 ]
  [ "$(git -C "$R" rev-parse wip)" = "$(git -C "$W" rev-parse HEAD)" ]
  [ "$(git -C "$R" symbolic-ref --short HEAD)" = main ]
}

@test "land: refuses a conflicting rebase and changes nothing" {
  feature f1
  C=$W edit src/a.txt 1 F1
  C=$W commit "feat(worktree): f1 edits a"
  edit src/a.txt 1 WIP1; commit "feat(worktree): wip edits a"
  local w b
  w=$(git -C "$R" rev-parse wip) b=$(git -C "$W" rev-parse HEAD)
  C=$W run wc_ land
  [ "$status" -eq 1 ]
  [[ $output == *"conflicts (src/a.txt"*"aborted, nothing changed"* ]]
  [ "$(git -C "$R" rev-parse wip)" = "$w" ]
  [ "$(git -C "$W" rev-parse HEAD)" = "$b" ]
  [ ! -e "$(git -C "$W" rev-parse --git-path rebase-merge)" ]
}

@test "land: refuses when check finds errors" {
  feature f1
  printf 'c\n' >"$W/src/c.txt"; printf -- '- c\n' >>"$W/to-test.md"
  C=$W commit "feat(worktree): add c"
  local w
  w=$(git -C "$R" rev-parse wip)
  C=$W run wc_ land
  [ "$status" -eq 1 ]
  [[ $output == *"refusing, check found 1 error(s)"* ]]
  [ "$(git -C "$R" rev-parse wip)" = "$w" ]
}

@test "land --personal-last: needs --yes, then re-creates the personal commit on top" {
  feature f1
  printf 'c\n' >"$W/src/c.txt"; C=$W commit "feat(worktree): add c"
  local w b
  w=$(git -C "$R" rev-parse wip) b=$(git -C "$W" rev-parse HEAD)
  C=$W run wc_ land --personal-last
  [ "$status" -eq 1 ]
  [[ $output == *"re-run with --yes"* ]]
  [ "$(git -C "$R" rev-parse wip)" = "$w" ] && [ "$(git -C "$W" rev-parse HEAD)" = "$b" ]

  C=$W run wc_ land --personal-last --yes
  [ "$status" -eq 0 ]
  [[ $output == *"re-created personal ${w:0:10}"* && $output == *"not a fast-forward"* ]]
  run git -C "$R" log --format=%s -3 wip
  [ "${lines[0]}" = "personal scripts" ]
  [ "${lines[1]}" = "feat(worktree): add c" ]
  [ "${lines[2]}" = "docs(ledgers): wip entries" ]
  [ "$(git -C "$R" rev-parse wip)" != "$w" ]
  [ "$(git -C "$R" rev-parse wip~2)" = "$(git -C "$R" rev-parse "$w~1")" ]
  [ -z "$(git -C "$R" diff "$b" wip)" ]
  [ -z "$(git -C "$R" status --porcelain --untracked-files=no)" ]
}

@test "land --dry-run prints the steps and changes nothing" {
  feature f1
  printf 'c\n' >"$W/src/c.txt"; C=$W commit "feat(worktree): add c"
  printf 'c2\n' >>"$W/src/c.txt"; C=$W commit "fixup! feat(worktree): add c"
  local w b
  w=$(git -C "$R" rev-parse wip) b=$(git -C "$W" rev-parse HEAD)
  for args in "--dry-run" "--dry-run --personal-last"; do
    # shellcheck disable=SC2086
    C=$W run wc_ land $args
    [ "$status" -eq 0 ]
    [[ $output == *"+ env GIT_SEQUENCE_EDITOR="*"rebase -q -i --autosquash"* ]]
    [[ $output == *"dry run, nothing changed"* ]]
    [ "$(git -C "$R" rev-parse wip)" = "$w" ]
    [ "$(git -C "$W" rev-parse HEAD)" = "$b" ]
  done
}

@test "land: refuses on wip itself and on a dirty worktree" {
  run wc_ land
  [ "$status" -eq 1 ]
  [[ $output == *"not wip"* ]]
  feature f1
  printf 'dirty\n' >>"$W/src/a.txt"
  C=$W run wc_ land
  [ "$status" -eq 1 ]
  [[ $output == *"uncommitted changes"* ]]
}
