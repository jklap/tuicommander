---
name: commit-to-wip
description: >
  Keep the long-lived `wip` branch in its re-organised shape (feature commits,
  one ledgers commit, one personal commit at the tip) while adding work to it.
  Use whenever committing on `wip` or on a worktree branch that will land on
  `wip`, when landing a worktree branch onto `wip` (rebase + fast-forward,
  never a merge commit), or when choosing which existing commit a follow-up
  should `--fixup`. Covers `wip-commit.sh check` (mixed ledger edits, personal
  paths, stray fixups, commits above the personal commit), `suggest-fixup`,
  `land` (with `--dry-run` and `--personal-last`), the TUIC worktree flow, and
  when to run `reorg-and-rebase-onto-main` instead.
keywords:
  - wip
  - commit to wip
  - land worktree
  - fast-forward
  - fixup commit
  - autosquash
  - Squash-Group
  - ledger commit
  - personal commit
  - wip-commit.sh
---

# Commit to wip

`wip` is re-organised by `reorg-and-rebase-onto-main` into ~124 feature commits (each with a
`Squash-Group: <group>` trailer and a subject like `feat(worktree): ... (part 3/9)`), then one
`docs(ledgers): ...` commit holding every ledger file, then one `personal scripts` commit at the
very tip. That shape is what makes rebasing onto `origin/main` cheap: ledger conflicts resolve
once, fixups land on the feature they fix. The last rebase hurt because ledger edits were mixed
into 135 code commits and fixups were scattered. This skill keeps new work from undoing it.

Tooling, in this skill's directory:

| Path | What it is |
|---|---|
| `scripts/wip-commit.sh` | `check`, `suggest-fixup`, `land`, `help`. Read-only except `land`. |
| `tests/` | bats suite, runner `tests/run.sh`. |
| `/Users/jklap/src/tuicommander/.claude/wip.conf` | Shared config with `reorg-and-rebase-onto-main`. In a linked worktree the script falls back to the main worktree's copy, since `wip.conf` is untracked. |

Config (each also an env var): `WIP_BRANCH`, `WIP_UPSTREAM`, `WIP_LEDGERS`, `WIP_PERSONAL_PATHS`,
`WIP_PERSONAL_RE` from `wip.conf`, plus `WIP_LEDGERS_RE` (ledgers subject, default
`^docs\(ledgers\)`), `WIP_CLASSIFY` (path to the sibling `classify.py`; empty skips the `misc`
check) and `WIP_SUGGEST_MAX` (commits listed per path, default 3). A commit's role comes from its
`Squash-Group:` trailer (`personal`/`ledgers`) or its subject, and a `fixup!`/`squash!`/`amend!`
takes its target's role, so `fixup! personal scripts` may touch personal paths.

## Ask first

Never commit, land or push without the user's explicit go-ahead for **that** action; an earlier
yes does not carry over. `check` and `suggest-fixup` are read-only and need no approval. Run
`land --dry-run` and show its output before asking to land. Never push `wip` as part of this
skill.

## Commit hygiene

1. **One feature per commit**, conventional subject `type(scope): summary`. Pick a scope that
   `classify.py`'s `SCOPE` table maps to the right group (`feat(worktree)`, `fix(git)`,
   `feat(remote)`, ...). `check` warns when a commit would land in `misc`.
2. **Ledger files never ride along with code** (`CHANGELOG.md`, `to-test.md`, `SPEC.md`,
   `todo.md`, `docs/FEATURES.md`, `.gitignore`). Commit them on their own
   (`docs(ledgers): ...`), or as `git commit --fixup=<ledgers commit>`. A feature and its
   CHANGELOG entry are therefore two commits:
   ```bash
   git add src/... && git commit -m "feat(worktree): ..."
   git add CHANGELOG.md to-test.md && git commit --fixup=<ledgers commit>
   ```
3. **Personal paths** (`WIP_PERSONAL_PATHS`: `build.sh`, `tuic`, `.claude/agents`, ...) only ever
   go into `git commit --fixup=<personal commit>`. A new personal file means a `wip.conf` edit
   first.
4. **Follow-ups to an existing feature are `--fixup`s**, not new commits. Stage the change, then:
   ```bash
   .claude/skills/commit-to-wip/scripts/wip-commit.sh suggest-fixup        # staged paths
   .claude/skills/commit-to-wip/scripts/wip-commit.sh suggest-fixup a b    # or named paths
   ```
   For each path it lists the last commits since `WIP_UPSTREAM` that touched it (on `wip` and on
   the current branch, marked `this branch only`), with their `Squash-Group`, resolves a
   `fixup!` to the commit it fixes, flags ledgers and personal paths, and ends with a ready
   `git commit --fixup=<hash>` (or one `git commit --fixup=<h> -- <paths>` per target when the
   staged paths belong to different commits; that form commits those paths' whole working-tree
   content, so restage by hand after `git add -p`). It is a suggestion: the last commit to touch a
   file is not always the feature the change belongs to; read the list.
5. **A genuinely new feature** whose group is ambiguous can pin it with a trailer:
   `git commit -m "feat(...): ..." -m "Squash-Group: worktree"`. The re-org honours it first.
6. No merge commits, no empty commits. `check` errors on both.

## Checking

```bash
.claude/skills/commit-to-wip/scripts/wip-commit.sh check            # what this branch adds
.claude/skills/commit-to-wip/scripts/wip-commit.sh check <base>..<tip>
```

Default range: `merge-base(wip, HEAD)..HEAD` on a feature branch, `merge-base(WIP_UPSTREAM,
HEAD)..HEAD` on `wip` itself. Exit 1 on any error, 0 with only warnings.

| Finding | Level | Fix |
|---|---|---|
| ledger files mixed with other files | error | split: `git reset HEAD~` the commit and recommit as two, or `git rebase -i` + `edit` |
| personal paths outside the personal commit | error | move them into a `--fixup` of the personal commit |
| ledgers/personal commit also changing other paths | error | split |
| empty or merge commit | error | drop it; rebase instead of merging |
| `fixup!`/`squash!`/`amend!` whose target is in the range | warning | `land` squashes it; on `wip`, see below |
| fixup whose target exists nowhere | warning | reword it to the target's current subject |
| non-conventional subject, or `classify.py` says `misc` | warning | reword, or add a `Squash-Group:` trailer |
| on `wip`: commits above the personal commit | warning | `git rebase -i <personal>^` and move its pick line last, or leave it for the next re-org |

A fixup of a `wip` commit seen from a feature branch is only a `note:`: it is the intended
pattern, it lands as-is and squashes at the next re-org.

## Worktree flow (the usual one)

1. Create the worktree with TUIC: `repo action=worktree_create` with `base_ref=wip`, never
   `git worktree add`. Work and commit there following the hygiene rules.
2. `wip-commit.sh check` in the worktree. Fix errors.
3. Ask the user, then from inside the worktree:
   ```bash
   .claude/skills/commit-to-wip/scripts/wip-commit.sh land --dry-run   # show this first
   .claude/skills/commit-to-wip/scripts/wip-commit.sh land
   ```
   `land` (default: the current branch; a named branch must be checked out in some worktree)
   refuses on `check` errors, on uncommitted tracked changes, and while any worktree is
   rebasing `wip`. It then runs **one** `git rebase -i --autosquash --onto wip wip` with a no-op
   sequence editor, so the branch's own fixups fold into their in-branch targets and the branch
   moves onto the current `wip` in a single step; on a conflict it aborts that rebase (the
   branch is back exactly where it was) and tells you to resolve by hand. Finally it
   fast-forwards `wip`: `git -C <wip worktree> merge --ff-only <branch>` when `wip` is checked
   out somewhere (usually the main checkout, whose files update), else
   `git fetch . <branch>:wip`, which is fast-forward-only by nature. It never creates a merge
   commit and never runs `git switch`/`checkout` in your worktree. If the `wip` checkout has
   local changes in the files being updated, git refuses the fast-forward; the branch stays
   rebased and `wip` unchanged, so clean up and re-run.
4. Remove the worktree with TUIC: `repo action=worktree_remove` (path + branch), never
   `git worktree remove`. After `land` the branch tip is on `wip`, so TUIC's branch-safety proof
   for deleting it holds.

### `--personal-last`: the trade-off

A plain fast-forward puts the new commits **above** the personal commit. That is harmless: the
next re-org moves the personal commit back to the tip, and `land` prints a note saying so. It is
the default because it is a pure fast-forward: no existing `wip` commit changes.

`land --personal-last --yes` instead rebases onto the commit below the personal tip (the whole
run of personal commits at the tip, if there are several), re-creates those personal commits on
top inside the same rebase, and moves `wip` there (`reset --keep` in the `wip` worktree, or a
compare-and-swap `update-ref`). `wip` stays in shape, but its personal commit gets a new hash,
so it is **not** a fast-forward. Hence `--yes`; it prints the old and new hashes. Use it when the
user wants `wip` tidy now (e.g. before pushing it somewhere); otherwise prefer the default. Other
worktree branches forked from the old `wip` still rebase fine afterwards: the old personal commit
has the same patch as the new one, so `git rebase` drops it as already applied.

## Working directly on wip

Commit following the same rules, then `wip-commit.sh check`. Fixups of `wip` commits produce
"unsquashed" warnings; that is expected. Either:

- **leave them** for the next re-org, which folds every `fixup!` into its target's block; or
- squash them now, after asking: `GIT_SEQUENCE_EDITOR=true git rebase -i --autosquash
  $(git merge-base origin/main wip)`. This rewrites `wip` from the first fixed commit up, so do
  it only when no other worktree branch depends on the old hashes (or rebase those afterwards
  with `git rebase --onto wip <old-wip>`).

New commits on `wip` sit above the personal commit; `check` warns, and the fix is the same
`git rebase -i <personal>^` move, or nothing until the next re-org.

Never `git switch`/`git checkout` another branch inside a TUIC session's worktree (it closes the
session); work on `wip` only where `wip` is already checked out.

## When to re-org instead

Run `reorg-and-rebase-onto-main` (not this skill) when `check` on `wip` shows the shape has
drifted enough that hand fixes are no longer worth it: many mixed-ledger errors, dozens of
fixups or commits above the personal commit, a pile of `misc`, or before rebasing `wip` onto
`origin/main`. After a re-org, `wip` is rewritten: rebase every open worktree branch with
`git rebase --onto wip <old wip tip>` before landing it.

## Running the tests

```bash
cd /Users/jklap/src/tuicommander
scripts/with-test-tmp.sh bats .claude/skills/commit-to-wip/tests   # or tests/run.sh
shellcheck .claude/skills/commit-to-wip/scripts/wip-commit.sh .claude/skills/commit-to-wip/tests/run.sh
```

The bats suite builds throwaway repos under `$BATS_TEST_TMPDIR` with git reading only a
gitconfig there (`GIT_CONFIG_GLOBAL`, `GIT_CONFIG_NOSYSTEM=1`); plain `git worktree add` is fine
inside them.
