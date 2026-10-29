#!/usr/bin/env bats
# End to end: squash-wip.sh on a synthetic repo with a fake origin/main.

SCRIPT="$BATS_TEST_DIRNAME/../scripts/squash-wip.sh"

setup() {
  export GIT_CONFIG_GLOBAL=$BATS_TEST_TMPDIR/gitconfig GIT_CONFIG_NOSYSTEM=1
  unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE
  git config --file "$GIT_CONFIG_GLOBAL" user.name Test
  git config --file "$GIT_CONFIG_GLOBAL" user.email test@example.com
  git config --file "$GIT_CONFIG_GLOBAL" init.defaultBranch main
  git config --file "$GIT_CONFIG_GLOBAL" commit.gpgsign false
  REPO=$BATS_TEST_TMPDIR/repo OUT=$BATS_TEST_TMPDIR/out N=0
  build_repo
}

# Commit everything with a distinct date: cm <subject> [file content]...
cm() {
  local msg=$1; shift
  while (($#)); do mkdir -p "$(dirname "$REPO/$1")"; printf '%b' "$2" >"$REPO/$1"; shift 2; done
  N=$((N + 1))
  local d; d=$(printf '2026-01-01T00:00:%02dZ' "$N")
  git -C "$REPO" add -A
  GIT_COMMITTER_DATE=$d git -C "$REPO" commit -q --allow-empty --date="$d" -m "$msg"
}

lines() { local i; for i in 0 1 2 3 4 5 6 7 8 9; do printf '%s\\n' "${1:-}$i"; done; }

build_repo() {
  git init -q "$REPO"
  # shellcheck disable=SC2016 # wip.conf is written literally
  cm base .gitignore 'target/\n' CHANGELOG.md '# log\n' to-test.md '# to-test\n' \
    ui "$(lines ui)" git "$(lines git)" shared "$(lines shared)" \
    .claude/wip.conf 'WIP_LEDGERS=${WIP_LEDGERS:-"CHANGELOG.md to-test.md .gitignore"}\nWIP_PERSONAL_PATHS=${WIP_PERSONAL_PATHS:-"build.sh"}\n'
  git -C "$REPO" checkout -q -b up
  cm 'fix(git): shared fix' shared "$(lines shared | sed 's/shared3/fixed/')"
  git -C "$REPO" update-ref refs/remotes/origin/main up
  git -C "$REPO" checkout -q -b wip main
  git -C "$REPO" branch -q -D up
  cm 'feat(ui): pane' ui "$(lines ui | sed 's/ui1/pane/')" CHANGELOG.md '# log\n- ui\n'
  cm 'feat(git): branch' git "$(lines git | sed 's/git1/branch/')" CHANGELOG.md '# log\n- ui\n- git\n'
  cm 'fix(ui): pane follow-up' ui "$(lines ui | sed 's/ui1/pane/; s/ui8/follow/')" CHANGELOG.md '# log\n- ui\n- git\n- ui2\n'
  cm 'docs: to-test entry' to-test.md '# to-test\n- [ ] ui\n'
  cm 'fix(git): shared fix' shared "$(lines shared | sed 's/shared3/fixed/')"
  cm 'fixup! feat(git): branch' git "$(lines git | sed 's/git1/branch/; s/git8/fixed/')"
  cm 'personal scripts' build.sh '#!/bin/sh\n' .gitignore 'target/\nlocal-secret/\n'
  cm 'fix(ui): late' ui "$(lines ui | sed 's/ui1/pane/; s/ui8/follow/; s/ui5/late/')"
}

run_squash() { TMPDIR=$BATS_TEST_TMPDIR run "$SCRIPT" --out "$OUT" "$@"; }

snapshot() {
  git -C "$1" for-each-ref --format='%(refname) %(objectname)'
  git -C "$1" symbolic-ref HEAD
  git -C "$1" status --porcelain=v1 --untracked-files=all
  git -C "$1" diff HEAD
}

@test "dry run writes nothing into the repo" {
  before=$(snapshot "$REPO")
  run_squash --dry-run "$REPO"
  [ "$status" -eq 0 ]
  [[ $output == *"trees identical: OK"* ]]
  [[ $output == *"dry run: nothing written"* ]]
  [ "$(snapshot "$REPO")" = "$before" ]
  [ -s "$OUT/blocks.md" ]
}

@test "real run creates wip-squashed with the same tree and leaves wip alone" {
  before=$(snapshot "$REPO")
  wip=$(git -C "$REPO" rev-parse wip)
  run_squash "$REPO"
  [ "$status" -eq 0 ]
  [ "$(git -C "$REPO" rev-parse wip)" = "$wip" ]
  [ "$(snapshot "$REPO" | grep -v '^refs/heads/wip-squashed ')" = "$before" ]
  git -C "$REPO" diff --quiet wip wip-squashed
  [ -z "$(git -C "$REPO" for-each-ref refs/squash refs/original)" ]

  mapfile -t subjects <<<"$(git -C "$REPO" log --reverse --format=%s main..wip-squashed)"
  [ "${#subjects[@]}" -eq 5 ]
  [[ ${subjects[0]} == "fix(ui): "* ]]
  [[ ${subjects[1]} == "fix(git): "* ]]
  [ "${subjects[2]}" = "fix(git): shared fix" ]
  [[ ${subjects[3]} == "docs(ledgers): "* ]]
  [ "${subjects[4]}" = "personal scripts" ]

  # The upstream singleton keeps its original message; the fixup went into the git block.
  [ "$(git -C "$REPO" log -1 --format=%B wip-squashed~2)" = "fix(git): shared fix" ]
  [[ $(git -C "$REPO" log -1 --format=%B wip-squashed~3) == *"- fixup! feat(git): branch ("* ]]
  [[ $(git -C "$REPO" log -1 --format=%B wip-squashed~4) == *"Squashed from 3 commits:"* ]]
  # Only the personal commit adds the personal .gitignore line and file.
  [ "$(git -C "$REPO" show wip-squashed~1:.gitignore)" = "target/" ]
  [ "$(git -C "$REPO" diff --name-only wip-squashed~1 wip-squashed | tr '\n' ' ')" = ".gitignore build.sh " ]
  # Feature commits carry no ledger edits.
  [ -z "$(git -C "$REPO" diff --name-only main wip-squashed~2 -- CHANGELOG.md to-test.md .gitignore)" ]
}

@test "refuses when wip-squashed already exists" {
  git -C "$REPO" branch wip-squashed main
  run_squash "$REPO"
  [ "$status" -eq 1 ]
  [[ $output == *"refs/heads/wip-squashed already exists"* ]]
  [ "$(git -C "$REPO" rev-parse wip-squashed)" = "$(git -C "$REPO" rev-parse main)" ]
}

@test "works with a dirty working tree and keeps the changes" {
  printf 'dirty\n' >>"$REPO/ui"
  printf 'staged\n' >>"$REPO/git"
  git -C "$REPO" add git
  printf 'new\n' >"$REPO/untracked"
  before=$(snapshot "$REPO")
  run_squash "$REPO"
  [ "$status" -eq 0 ]
  [ "$(snapshot "$REPO" | grep -v '^refs/heads/wip-squashed ')" = "$before" ]
  git -C "$REPO" diff --quiet wip wip-squashed
}

@test "runs from the current directory without a repo argument" {
  cd "$REPO"
  run_squash --dry-run
  [ "$status" -eq 0 ]
  [[ $output == *"trees identical: OK"* ]]
}

@test "works from a linked worktree" {
  WT=$BATS_TEST_TMPDIR/wt
  git -C "$REPO" worktree add -q --detach "$WT" wip
  run_squash "$WT"
  [ "$status" -eq 0 ]
  git -C "$REPO" diff --quiet wip wip-squashed
  [ "$(git -C "$WT" rev-parse HEAD)" = "$(git -C "$REPO" rev-parse wip)" ]
  [ "$(git -C "$REPO" log -1 --format=%s wip-squashed)" = "personal scripts" ]
}

@test "WIP_UPSTREAM may name a local branch" {
  git -C "$REPO" branch upstream-local refs/remotes/origin/main
  git -C "$REPO" update-ref -d refs/remotes/origin/main
  WIP_UPSTREAM=upstream-local run_squash "$REPO"
  [ "$status" -eq 0 ]
  [ "$(git -C "$REPO" log --format=%s main..wip-squashed | grep -c '^fix(git): shared fix$')" -eq 1 ]
}

@test "submodule.recurse=true does not make the result fetch touch submodule remotes" {
  local sub=$BATS_TEST_TMPDIR/sub
  git init -q "$sub" && git -C "$sub" commit -q --allow-empty -m sub
  git -C "$REPO" -c protocol.file.allow=always submodule -q add "$sub" plugins
  cm 'chore(plugins): add the plugins submodule'
  # an unreachable submodule remote, as with a locked SSH agent
  git -C "$REPO/plugins" remote set-url origin "$BATS_TEST_TMPDIR/gone"
  git config --file "$GIT_CONFIG_GLOBAL" submodule.recurse true
  run_squash "$REPO"
  [ "$status" -eq 0 ]
  git -C "$REPO" diff --quiet wip wip-squashed
}
