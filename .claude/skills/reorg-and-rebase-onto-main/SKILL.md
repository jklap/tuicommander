---
name: reorg-and-rebase-onto-main
description: >
  Re-organise the long-lived `wip` branch into feature-squashed commits on its
  CURRENT base (final tree byte-identical, zero conflicts by construction),
  then rebase the much shorter result onto `main` with the `rebase-onto-main`
  skill. Covers config review (`.claude/wip.conf`), a `classify.py` grouping
  preview with `misc` triage, the dry run and its `blocks.md`, the real run
  (which only ever creates `wip-squashed`), tree/range-diff verification,
  moving `wip` with a guarded `update-ref`, publishing under a new name, and
  the Phase B resolutions for the single ledgers commit, the already-upstream
  singletons, the personal commit and the regen commit. Use when asked to
  re-organise or squash the wip branch by feature, clean up wip history before
  a rebase, cut down the number of rebase stops, or "re-org and rebase onto
  main".
keywords:
  - squash wip
  - reorg
  - re-organise wip
  - squash by feature
  - squash-wip.sh
  - wip-squashed
  - ledger commit
  - rebase onto main
  - merge-tree
  - filter-branch
---

# Re-org wip, then rebase onto main

`wip` accumulates hundreds of small commits, and most of the cost of rebasing it is not
its code: it is the ledger files (`CHANGELOG.md`, `to-test.md`, `docs/FEATURES.md`,
`.gitignore`, ...) that almost every commit touches and that `main` edits constantly, plus
leftovers of earlier rebases (empty "recording" commits, reconcile/de-duplicate commits,
lockfile and fmt passes). This skill removes that cost **before** the rebase, in two phases:

- **Phase A (this skill): re-org on the current base.** `squash-wip.sh` rewrites `wip` into
  one commit (or a few `part i/n` commits) per feature, one **ledgers** commit and one
  **personal** commit at the tip. The final tree is byte-identical to `wip`, nothing can stop
  on a conflict, and the only thing it writes to the repo is a new `wip-squashed` branch.
- **Phase B (`rebase-onto-main`): rebase the short result onto `main`**, with the specific
  resolutions below for the ledgers, upstream-singleton, personal and regen commits.

Tooling, all in this skill's directory:

| Path | What it is |
|---|---|
| `scripts/squash-wip.sh` | Driver: bare clone, strip, plan, verify, then (non-dry) create `<branch>-squashed`. `-h` prints usage. |
| `scripts/plan.py` | Planner and writer: places each commit with `git merge-tree` proofs, writes commits with `git commit-tree`, writes `blocks.md`. |
| `scripts/classify.py` | Group mapping tables (`GROUPS`, `SCOPE`, `SUBJECT_OVERRIDES`, `PATHS`) and a preview CLI. |
| `tests/` | pytest + bats suite, runner `tests/run.sh` (see "Running the tests"). |
| `/Users/jklap/src/tuicommander/.claude/wip.conf` | Shared config (also read by `commit-to-wip`). |

## When to use this vs plain `rebase-onto-main`

Use the re-org first when any of these hold (measure with Step 0/1 of `rebase-onto-main`):

- the branch has **hundreds** of commits over its merge base (`git rev-list --count <base>..wip`);
- **ledger noise dominates**: a large share of commits touch a ledger file
  (`git log --oneline <base>..wip -- CHANGELOG.md to-test.md docs/FEATURES.md | wc -l`);
- many commits touch a file `main` also changed (each one is a possible stop);
- the history is mostly fix-on-fix, recording commits or leftovers of earlier rebases, so
  replaying it one commit at a time preserves nothing worth the cost.

Use plain `rebase-onto-main` when the branch is short, its commits are already atomic, or the
user wants every original commit preserved (Strategy D there). The re-org is a history
rewrite: get the user's go-ahead for it, not just for the rebase.

## Safety rules (non-negotiable)

- **Ask before every branch creation, ref move and push**, separately. A previous yes does not
  carry over (repo Branching rule; global git rules). That includes the backup branch, the real
  run (it creates `wip-squashed`), the `update-ref` of `wip`, and each push.
- **Backup first**: `git branch wip-backup-<YYYYMMDD> wip` (on the user's say-so), and write the
  raw hash down too (`rebase-onto-main` Step 3: a ref alone has vanished before).
- **Never `git switch`/`checkout`/`reset` the primary checkout.** The orchestrating session's own
  worktree is usually on `wip`; switching branches there closes the session. Move refs with a
  guarded `git update-ref` (Step A7).
- **Publishing: the user's choice on 2026-10-27 was to push the result under a NEW branch name
  and keep the old `origin/wip`** untouched. Default to that again; replacing `origin/wip` needs
  `--force-with-lease=wip:<old-hash>` and its own explicit confirmation.
- The scripts never touch `wip`, its remote, the index or the working tree. Keep it that way:
  don't "fix" a failed run by editing refs by hand inside the real repo.

## Phase A — re-org on the current base

### A1. Review the config

`.claude/wip.conf` is sourced by bash; every value can be overridden from the environment.

| Variable | Meaning | 2026-10-27 value |
|---|---|---|
| `WIP_BRANCH` | branch to re-org | `wip` |
| `WIP_UPSTREAM` | the base is `merge-base(WIP_BRANCH, WIP_UPSTREAM)`; its subjects mark "upstream" commits | `origin/main` |
| `WIP_LEDGERS` | append-only files moved into the one ledgers commit | `CHANGELOG.md to-test.md SPEC.md todo.md docs/FEATURES.md .gitignore` |
| `WIP_PERSONAL_PATHS` | files that never go upstream, moved into the one personal commit (directories match recursively) | `build.sh force_perms.sh .rsync-filter tuic tuic-script.md tuic.bats .claude/agents` |
| `WIP_PERSONAL_RE` | subjects of personal commits; the `.gitignore` lines they add go to the personal commit, not the ledgers one | `^personal scripts$\|^ignore \.obsidian$` |

Check before each run: has a new personal file appeared (`git status`, recent "personal
scripts" commits)? Does a new append-only file deserve ledger treatment? Keep the ledger set
narrow: `docs/sync-matrix.md`, `AGENTS.md` and `src-tauri/AGENTS.md` were considered and left
out, so docs keep travelling with their feature.

Then fetch and confirm the base is real (`rebase-onto-main` Step 0: `git ls-remote origin main`
vs `git rev-parse origin/main`). The re-org runs on the **merge base**, so a stale
`origin/main` only skews the "upstream" subject list, but Phase B needs the live head.

### A2. Preview the grouping, triage `misc`

```bash
S=.claude/skills/reorg-and-rebase-onto-main/scripts
BASE=$(git merge-base wip origin/main)
git log --format=%s "$BASE..origin/main" > "$TMPDIR/upstream-subjects.txt"
python3 $S/classify.py . "$BASE" wip "$TMPDIR/upstream-subjects.txt" > "$TMPDIR/groups.tsv"
cut -f2 "$TMPDIR/groups.tsv" | sort | uniq -c | sort -rn
awk -F'\t' '$2 == "misc"' "$TMPDIR/groups.tsv"
```

Each row is `hash<TAB>group<TAB>subject`. Precedence, first match wins (`classify.py` docstring):
a `Squash-Group:` trailer; subject also on upstream (`upstream`); `fixup! <subject>` takes its
target's group; `SUBJECT_OVERRIDES`; the conventional-commit scope via `SCOPE` (a compound
`a,b` scope falls back to `a`, unless listed explicitly); scope-less `test:`/`ci:`/`style:` go
to `test-infra`; `PATHS` prefixes; else `misc`.

This preview runs on the **unstripped** branch, so commits that only touch ledgers show up
under whatever group their scope says; they are pruned in the real run. What matters is
`misc`: every `misc` row needs a `SCOPE` entry, a `SUBJECT_OVERRIDES` regex or a `PATHS`
prefix (see "Maintaining the mapping tables"), or a deliberate decision to leave it there.
Also eyeball the large groups for obvious misfiles. Grouping only decides *which* commits may
be merged together; a wrong group never breaks the tree, it just produces a misleading title.

The `upstream` rows are matched **by subject only**. The tool does not tell byte-identical
twins from modified ones; both are kept as single commits with their original message and are
sorted out in Phase B.

### A3. Dry run

```bash
.claude/skills/reorg-and-rebase-onto-main/scripts/squash-wip.sh --dry-run .
```

About two minutes for ~500 commits. It:

1. clones the repo **bare** into `$TMPDIR/squash-wip.XXXXXX` (removed on exit) and fetches
   `wip` and the upstream ref into it. Everything below happens in that clone: no scratch ref
   ever lands in your repo, and `git filter-branch` would refuse your dirty checkout anyway;
2. **strips**: `filter-branch --index-filter --prune-empty` resets every ledger and personal
   path to the base's blob in every commit. A tree-level rewrite applies no patches, so it
   cannot conflict; commits left empty (ledger-only, personal-only, already-empty) are pruned;
3. **plans** (`plan.py`, below) and writes the new commits with `git commit-tree`;
4. **verifies** that the new tip's tree equals `wip`'s, else exits non-zero with `TREES DIFFER`.

Outputs, kept after exit, in `$TMPDIR/squash-wip-out/` (override with `--out DIR`):
`blocks.md` (the reviewable todo) and `upstream-subjects.txt`.

How `plan.py` places each stripped commit, oldest first:

- appended to the last block if it is the same group and still merges cleanly;
- else moved back into the **nearest** earlier block of its group (within a window of 60
  blocks, `plan.py --window`), only if `git merge-tree --write-tree` proves that the commit
  **and every block after that point** still apply cleanly;
- else it starts a new block. A second pass folds whole blocks into an earlier same-group
  block under the same rule. `upstream` commits never move or merge.

Then a **ledgers** commit restores `wip`'s ledger files (minus the `.gitignore` lines personal
commits added) and a **personal** commit restores the personal paths and the full `.gitignore`.
`fixup!` commits are just members of their target's group, so they are squashed under the same
merge-tree proof as everything else; one that can't move stays where it is.

The run aborts with `personal/ledger commits still change other paths after stripping` when a
commit matched `WIP_PERSONAL_RE` (or has a `personal`/`ledgers` trailer) but touches a path
outside the strip set: add that path to `WIP_PERSONAL_PATHS`/`WIP_LEDGERS` or fix the subject
mapping, then re-run.

### A4. Read `blocks.md`

One heading per new commit, `# <title>  [N commit(s)]`, then its members in order as
`pick`/`fixup <orig-hash> <subject>` (upstream singletons are one `pick` each), and for the
ledgers block an `exec git checkout <orig> -- <ledgers>` line. It looks like a rebase todo but
is documentation: the commits are already written. Check:

- the `chore: unclassified commits` blocks (`misc`): ideally none;
- groups split into many parts (`(part i/n)`): a split means feature A's commit edits lines
  feature B changed in between, so they cannot commute. That is real dependency structure,
  not a planner limit (see Gotchas); re-grouping won't remove it;
- the upstream singletons: count them and note the hashes for Phase B;
- the personal block lists the personal commits and nothing project-shaped.

Commit messages: group title (`(part i/n)` when split), `Squashed from N commits:` with every
original `- subject (hash)` (a member that is itself a squashed commit contributes its own list,
flattened, so re-runs don't nest), the members' de-duplicated trailers (Co-Authored-By,
Fixes/Closes, ...), and a `Squash-Group: <group>` trailer. Author and date come from the newest
member. Upstream singletons keep their original message. The ledgers commit has no members, so
its author/date are whoever runs the tool, now.

### A5. Real run

With the user's go-ahead for the backup branch and for creating `wip-squashed`:

```bash
git branch wip-backup-<YYYYMMDD> wip          # user-approved
OLD=$(git rev-parse wip); echo "$OLD"          # write it down
.claude/skills/reorg-and-rebase-onto-main/scripts/squash-wip.sh .
```

It refuses to start if `refs/heads/wip-squashed` already exists (delete or rename it first,
with the user's OK). Its only change to the repo is a **non-forced** fetch of
`refs/heads/wip-squashed` from the scratch clone.

### A6. Verify

```bash
git diff --quiet wip wip-squashed && echo TREES-IDENTICAL       # must print
git rev-list --count "$BASE..wip-squashed"
git log --oneline "$BASE..wip-squashed"
git range-diff "$BASE..wip" "$BASE..wip-squashed" | less        # optional deep review
git log --format=%B "$BASE..wip-squashed" | grep -c '^Squash-Group:'
```

`range-diff` shows each squashed commit against the originals it absorbed; most of the output
is "deleted"/"added" pairs, which is expected for squashes. Spot-check two or three split
features and the ledgers commit.

### A7. Move `wip` (guarded, no checkout)

With the user's go-ahead:

```bash
git update-ref refs/heads/wip wip-squashed "$OLD"
```

The third argument makes it a compare-and-swap: it fails if `wip` moved since you recorded
`$OLD` (someone committed in between). Why not `git switch`/`reset --hard`: the session's own
worktree is on `wip`, and switching there closes the session; `reset --hard` would also
touch the user's uncommitted edits. Because the trees are identical, the index and working
tree already match the new `wip`: `git status` afterwards shows exactly what it showed before
(uncommitted edits such as `build.sh` and untracked files stay put).

### A8. Publish (ask first)

The user's 2026-10-27 choice: keep `origin/wip`, publish the result under a new name.

```bash
git push origin wip-squashed                 # a new branch: no force needed
```

Only on a separate, explicit instruction replace `origin/wip`:
`git fetch origin wip && git push --force-with-lease=wip:<old-hash> origin wip`.

## Phase B — rebase onto `main`

Hand off to the `rebase-onto-main` skill and follow it as written (dedicated worktree, Step 0
live-head check, backup ref, `-c submodule.recurse=false -c rerere.enabled=false`). The
union-merge driver (its Step 2) is not needed: there is only one ledgers commit. What is
different:

- **Far fewer stops**: on 2026-10-27, ~123 commits that touch a file `main` changed, against
  464 before, and no ledger stops except the ledgers commit itself.
- **The ledgers commit** (`docs(ledgers): wip entries for ...`): its own diff is exactly `wip`'s
  net ledger delta against the old base. Resolve each file by taking `main`'s version and
  re-adding `wip`'s entries from `git show <ledgers-commit> -- <file>`; `--theirs`/`--ours`
  each lose one side. For `.gitignore` take the union. For `to-test.md` and `CHANGELOG.md`
  apply `rebase-onto-main` Step 5 (purged entries, released versions, one `### Added` etc.).
- **Upstream singletons**: the byte-identical ones are dropped by `git rebase` automatically
  (patch already upstream). Each modified one stops once: take `main`'s side, keep only `wip`'s
  genuine delta, and drop the commit if nothing is left (`git rebase --skip` on an empty
  result). Log each one: hash, what survived, what didn't.
- **The personal commit stays last** and never goes upstream. Its only realistic conflict is
  `.gitignore` (union). If `main` ever adds a file at a personal path, stop and ask.
- **The regen commit.** Don't resolve generated files inside feature commits: at every stop on
  `src-tauri/Cargo.lock`, `THIRD_PARTY_NOTICES.md`, the vitest coverage thresholds or
  `src-tauri/src/mcp_http/command_table_paths.txt`, take `main`'s side and move on. After the
  rebase regenerate all four in one `chore(regen): ...` commit, just below the personal commit:
  - `Cargo.lock`: `cargo metadata --format-version 1 >/dev/null` in `src-tauri/` (keeps existing
    versions), then `cargo check --locked`; precedent `f8182d77e`.
  - `THIRD_PARTY_NOTICES.md`: `pnpm notices`, then `pnpm notices:check`; precedent `183283bbc`.
  - Coverage floors (`vitest.config.ts` `thresholds`): `scripts/with-test-tmp.sh pnpm
    test:coverage`, set each floor to the measured value minus one point, rounded down;
    precedent `3e52170e3`.
  - `command_table_paths.txt`: `scripts/with-test-tmp.sh pnpm vitest run
    src/__tests__/transport.test.ts -u` (never hand-edit), then the Rust half
    `command_table_paths_all_hit_a_registered_route`.
  Commit it on top, then swap it below the personal commit (`git rebase -i HEAD~2` with a
  `GIT_SEQUENCE_EDITOR` that swaps the two lines; disjoint files, so it can't conflict).
- Finish with `./scripts/check-gate.sh` (`check-gate` skill) and `rebase-onto-main` Steps 6-11.

## First run, 2026-10-27: numbers and gotchas

| | `wip` before | after |
|---|---|---|
| commits over base `9110058f5` | 524 | ~124 (104 feature parts + 20 upstream singletons) |
| commits touching a file `origin/main` changed | 464 | 123 |
| commits touching a ledger | 195 | 2 (ledgers + personal) |
| personal scripts | 7 commits, scattered | 1 commit, last |
| conflicts while squashing | n/a | 0, by construction |

Stripping pruned 30 ledger-only or empty commits (524 to 494). `origin/main` was 3,762 commits
ahead of the base. These numbers are from the scratchpad prototype run, before
`.claude/agents` joined `WIP_PERSONAL_PATHS`; a re-run can differ by a part or two.

Gotchas:

- **A naive feature reorder conflicts everywhere.** Reordering every commit into its group and
  squashing with `git rebase -i` hit **365 conflicting replay steps** (`state.rs`, `lib.rs`,
  `mcp_http/mod.rs`, `commandTable.ts`, the AGENTS docs are touched by dozens of features).
  Hence the merge-tree-proven moves: a commit only moves where git proves it commutes.
- **`added .claude/` (`f5255df9f`) could not move to the end** with the personal scripts: later
  project commits edit the skills it adds, and moving it broke the dry run. The fix was
  tree-level, not a move: `.claude/agents` went into `WIP_PERSONAL_PATHS`, so the agents are
  stripped from every commit and reappear in the personal commit, while the skills stay in the
  `docs` group with their later edits.
- **Blank lines in `.gitignore` diffs.** Personal `.gitignore` lines are removed from the
  ledgers commit's `.gitignore` one matching line at a time (first occurrence). A blank `+`
  line in a personal commit's diff would remove the file's *first* blank line, an unrelated
  one, so the ledgers commit drops it and the personal commit puts it back: a stray whitespace
  hunk in both, and one more place for Phase B to conflict. `personal_gitignore_lines` therefore
  ignores blank added lines (they stay in the ledgers commit). Tree equality is unaffected
  either way, since the personal commit restores `wip`'s `.gitignore` wholesale.
- **`git filter-branch` refuses a dirty checkout**, and the primary checkout always has
  uncommitted personal edits. Hence the throwaway bare clone, which also keeps every scratch
  ref out of the real repo.
- **A wider search gives no gain.** Letting each commit search every earlier same-group block
  instead of the nearest one gave the same 124. In the current code that knob is
  `plan.py --window N` (default 60 blocks; `squash-wip.sh` doesn't pass it). Getting under
  ~100 commits needs hand-resolved conflicts, which is what this avoids.
- **Upstream detection is subject-only**, so a `wip` commit that happens to share a subject with
  an unrelated `main` commit becomes a singleton. Harmless (it just isn't squashed); check the
  singleton list in `blocks.md`.

## Re-running later

The tool is idempotent on its own output: every squashed commit carries `Squash-Group:`, which
wins over every other rule, so it re-classifies into the same group; its `Squashed from` list is
flattened into the new commit's list; and the personal commit is recognised by its trailer as
well as by `WIP_PERSONAL_RE`. New commits since the last re-org classify by the usual rules.
Check `blocks.md` for `misc` again. A commit can also pin its group by hand with a
`Squash-Group: <group>` trailer.

## Maintaining the mapping tables

The tables at the top of `scripts/classify.py` are this repo's data, not logic:

- `GROUPS`: group id -> (squashed commit title, informational order). A new group needs a
  title here, or it gets `chore(<group>): squashed`. `upstream`, `ledgers` and `personal` are
  structural; don't rename them.
- `SCOPE`: conventional-commit scope -> group (built with `_scopes("a b c", "group")`). The
  usual fix for a `misc` row is a new scope here. Compound scopes (`tunnels,remote`) can be
  mapped explicitly when their first part would pick the wrong group.
- `SUBJECT_OVERRIDES`: regex -> group, checked before the scope. Use sparingly, for
  cross-cutting themes whose commits carry assorted scopes; order matters (first match wins).
- `PATHS`: path prefix -> group, the last resort for scope-less commits.

After editing, re-run the A2 preview and the tests. A grouping change only affects titles and
which commits may merge; tree equality is checked regardless.

## Running the tests

```bash
.claude/skills/reorg-and-rebase-onto-main/tests/run.sh
```

pytest for `classify.py`/`plan.py` and bats for `squash-wip.sh`, against throwaway repos under
`$TMPDIR`. Run them after any change to the scripts or the mapping tables.

## Keeping it clean afterwards

Use the sibling `commit-to-wip` skill (`.claude/skills/commit-to-wip/`, script
`wip-commit.sh`: `check`, `suggest-fixup`, `land`) for new work: it reads the same
`.claude/wip.conf`, keeps ledger edits out of code commits and suggests `--fixup=<feature
commit>` targets, so the next re-org (or a plain `git rebase -i --autosquash`) stays cheap.
Re-run this skill when the branch has grown back to hundreds of commits.
