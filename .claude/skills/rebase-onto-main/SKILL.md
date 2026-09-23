---
name: rebase-onto-main
description: >
  Rebase a long-lived branch (typically `wip`, or a feature branch like
  `fix/command-block-diagnostics`) onto an updated `main`/`origin/main` safely.
  Covers pre-flight conflict reconnaissance, deciding rebase-vs-merge-commit
  and simple-vs-hardened strategy (including an empirical check for whether
  rerere training will actually pay off), ledger-file (CHANGELOG.md/to-test.md/
  todo.md) noise handling, safety nets, conflict-resolution judgment
  (including the "silent risk" set of files both sides touched that auto-merge
  with no marker), post-resolution compile-error triage, structural + semantic
  post-rebase verification, and landing it either as a commit-by-commit rebase
  or as a single merge commit. Use whenever asked to rebase a branch onto
  main, resolve a branch's divergence from main, or bring a long-running
  branch up to date before merging/PR-splitting it.
keywords:
  - rebase
  - rebase onto main
  - merge conflict
  - git rebase
  - wip branch
  - force-with-lease
  - rerere
  - fast-forward
---

# Rebase onto main

This repo has done this several times, at increasing scale, and all are
documented case studies worth re-reading before a new attempt:
`plans/please-review-the-new-elegant-widget.md` (small: `wip` 71 ahead / 9
behind, mostly ledger noise), `plans/polymorphic-floating-feather.md` (large:
41 commits, 21 real conflicts, genuine overlapping logic on both sides), and
`plans/bubbly-swimming-dijkstra.md` (very large: 176 commits vs. 255 on main,
several architectural refactors on both sides — this is where Steps 2, 4a, and
the merge-commit landing path below came from, after a real rebase attempt
proved far more expensive than the trained-rerere plan expected). `plans/
branch-review-2026-08-27.md` is a full post-rebase commit audit that found
real defects a rebase alone didn't catch. `plans/` is gitignored and **only
exists in the main checkout** — resolve these paths against
`/Users/jason.klapste/src/external/tuicommander/plans/`, not a worktree, even
when you're doing the actual rebase work in a worktree.

**Do all rebase work in a dedicated worktree, never the primary checkout.**
Use the exact worktree path for every Read/Edit/Write/Bash call once you're in
one — a stray `cd` back toward the main checkout silently propagates through
every later command and has corrupted a session before. Per this repo's
Branching rule, don't create a new branch/worktree autonomously — but the
safety-net backup ref below is a standard, expected part of *this* task, not
a separate autonomous branch creation; still, name the strategy and the
backup-ref name in your plan and get it approved before you start rewriting
history or pushing. **Only move the actual `<branch>` ref and push from the
primary checkout**, at the very end (Step 11) — everything before that stays
in the worktree.

## Step 0 — Understand the divergence

```bash
git fetch origin
git merge-base main <branch>
git log --oneline main..<branch> | wc -l     # commits only on <branch>
git log --oneline <branch>..main | wc -l     # commits only on main
git status                                    # note every untracked/modified path; don't touch anything you didn't create
```

Check for anything that needs special handling before you start:
- **Submodule pointer drift** (`plugins`). If `<branch>`'s tip has a different
  `plugins` gitlink than the pre-rebase state and you don't intend to resolve
  that as part of this rebase, `git stash push -- plugins` first and
  `git stash pop` after landing — don't let it become an accidental rebase
  conflict.
- **Untracked local scratch** (`.obsidian/`, `build.sh`, `.rsync-filter`,
  ideas/ notes, etc.) — rebase doesn't touch untracked files; leave them, but
  confirm `git status` shows only expected untracked paths before you start
  so you'd notice if something unexpected appears mid-rebase.
- **`submodule.recurse=true` set repo-wide with `plugins` uninitialized in
  this worktree** — pass `-c submodule.recurse=false` on every `rebase`,
  `reset`, and `checkout` in this session, or a partial submodule-touch
  failure can leave the worktree mid-operation.
- **`rr-cache` lives in the shared common git dir, not per-worktree.** If you
  go looking for recorded resolutions to confirm rerere trained correctly, the
  real path is `$(git rev-parse --git-common-dir)/rr-cache` — **not**
  `.git/worktrees/<name>/rr-cache`, which doesn't exist for this purpose and
  will read as empty even after a real training pass genuinely recorded
  dozens of entries. Checking the wrong path here reads as "rerere recorded
  nothing," which is alarming and wrong — `git rerere status`/`git rerere
  diff` (run from inside the worktree; they resolve the shared cache
  correctly) are the reliable way to check, not `ls`-ing a guessed path.

## Step 1 — Conflict reconnaissance (before touching history)

Never start a real rebase blind. Build a picture of the integration first —
this is what turns "45 conflicts, mostly noise" into a plan instead of a
slog, and what surfaces the commits that need combining rather than picking.

1. **Whole-branch preview.** In the worktree (or a disposable scratch clone),
   do a *throwaway* trial merge — never on the primary checkout:
   ```bash
   git merge main --no-commit --no-ff   # or: git merge-tree <merge-base> <branch> main
   ```
   This enumerates every file that actually **conflicts** (needs a human
   decision) — usually a short list even when the commit counts are large.
   Abort it (`git merge --abort`) once you've recorded the list; it's
   reconnaissance, not the real merge.
2. **The silent-risk set.** Separately, find every file **both sides
   changed** whether or not it conflicted:
   ```bash
   comm -12 <(git diff --name-only <merge-base> <branch> | sort) \
            <(git diff --name-only <merge-base> main | sort)
   ```
   Files in this list minus the ones that actually conflicted are the
   dangerous ones: they'll auto-merge with **no marker at all**, yet both
   sides may have rewritten the same function/store/handler body in
   incompatible ways that still type-check and still pass each side's own
   tests (because those tests mock the seam, not the integration). Read each
   one against both parents by hand — `git show main:<path>` vs your branch's
   version — before trusting an auto-merge. `polymorphic-floating-feather.md`
   Phase 0 Gap D (`src/stores/repoSettings.ts`) and the security review's
   `aa632a59`/`c64f9620` findings in `branch-review-2026-08-27.md` are exactly
   this failure mode: a clean, no-conflict merge that was semantically wrong
   or that silently dropped a hunk.
3. **Per-commit grep recon (for a large divergence).** For each notable
   upstream (main-only) commit, grep your branch's diff for the
   function/symbol names it touches. A zero-hit grep means that commit is
   genuinely orthogonal to your branch and should auto-merge cleanly with no
   surprises; a hit means read both sides' actual diffs (not just commit
   subjects) before deciding how they combine. `plans/origin-review.md` is a
   worked example of this pass — reuse its structure (group by subsystem,
   name the specific functions checked, state the grep command and its
   result) rather than reasoning from commit messages alone.
4. **Classify every conflicting/risky file** into a short table before you
   start rewriting anything: *pure ledger noise* (append-only doc files where
   both sides only ever add lines), *mechanical* (single-hunk, obvious
   resolution, e.g. "keep both imports"), or *needs combining* (both sides
   changed the same logic for different reasons — these need judgment, not a
   pick-a-side).
5. **Per-file touch counts on each side, for the heaviest files.** For every
   file in the "needs combining" set, count how many commits on `<branch>`
   touch it (`git log --oneline main..<branch> -- <file> | wc -l`) and check
   *how* main changed it — a plain edit, or a restructure (split into
   multiple files, a purge/reorg of an append-only ledger, a data-model
   rekey)? A file with a double-digit commit count on `<branch>` **and** a
   restructure on main (not just an edit) is the strongest available signal
   that a commit-by-commit rebase will be expensive for that file — see the
   rerere caveat in Step 2's large-divergence case before assuming the
   hardened protocol will absorb it cheaply.

## Step 2 — Choose a strategy based on what Step 1 found

There are three shapes, not two — the third (land as a single merge commit)
is easy to miss because it looks like giving up on "doing it properly," but
it is sometimes strictly the right call, and knowing that *before* sinking
hours into a rebase attempt is the whole point of this step.

**Ledger-noise-dominated, few real conflicts** (the `please-review-the-new-elegant-widget.md`
shape — e.g. 6 conflicting files, ~45 more that are pure repeated noise from
append-only files getting replayed commit-by-commit):

- Add a **local-only, uncommitted** union-merge driver for the append-only
  files so they stop conflicting on every single replay:
  ```bash
  printf 'CHANGELOG.md merge=union\nto-test.md merge=union\ntodo.md merge=union\n' > .git/info/attributes
  ```
  (Never commit `.git/info/attributes`. If it needs to be worktree-private
  rather than shared with the primary checkout — e.g. the primary checkout
  has its own `core.attributesFile` you must not disturb — set
  `extensions.worktreeConfig=true` and `git config --worktree
  core.attributesFile <worktree-private-path>` instead of writing directly
  into the shared `.git/info/attributes`.) Union merge is line-additive — it
  never conflicts, but it silently interleaves both sides' lines, including
  near-duplicates, and needs a real editorial pass afterward (Step 5).
- Disable rerere for the rebase itself: `git -c rerere.enabled=false rebase main`.
  With only a handful of real conflicts left after the union driver, rerere
  buys nothing and can replay a stale/wrong cached resolution from an earlier
  session — a recorded gotcha, not a hypothetical.
- Resolve the small number of real conflicts one at a time, diffing before
  staging (Step 4).

**Large divergence with genuine overlapping logic changes on both sides, where
individual commits are still reasonably atomic** (the
`polymorphic-floating-feather.md` shape — tens of commits, dozens of
both-touched files, several conflicts that are "combine, don't pick"):

- **Phase 0 — write tripwire tests before touching history.** For every seam
  Step 1 flagged as "needs combining" (both sides independently rewrote the
  same function/component/store), write a test — on the current branch tip,
  before the rebase — that pins the invariant the *merged* code must
  preserve. These are test-only commits (or one test+fix commit if the audit
  surfaces a real live bug along the way, per AGENTS.md's red-green
  convention) that rebase trivially since they touch no production code. The
  payoff: the rebase gets a tripwire instead of relying on a code review
  after the fact to catch a dropped invariant.
- **Phase 1 — derive a known-good target tree via a throwaway merge.** This
  is the single highest-leverage trick for a rebase in this shape:
  1. `git merge main --no-ff` (add `--no-ff` explicitly if `merge.ff` is set
     to `only` anywhere in your git config — a real gotcha, not
     hypothetical: it makes a plain `git merge main` refuse with "Not
     possible to fast-forward" on a repo that's otherwise a normal
     divergence) on a scratch branch/worktree, and resolve **all** conflicts
     once, carefully, using the judgment notes in Step 4.
  2. **Commit the merge.** This step is easy to skip by accident if the
     conflict-resolution work leaves you sitting at "all conflicts fixed but
     you are still merging" — `git commit` (with a real message; it may
     become the permanent landing commit, see the merge-commit path below)
     is required before the next step, not optional busywork.
  3. Run the full suite until green (Step 4a covers getting from
     "conflicts resolved" to "actually compiles" first, which is usually its
     own multi-pass effort for a rebase this size — don't skip straight to
     the test suite while the crate still has compile errors).
  4. Record the tree and tag it: `git tag rebase-target` — **but only after
     step 2's commit**. `git tag <name>` with no commit argument tags
     *current HEAD*, and if you run it while still mid-merge (conflicts
     resolved and staged, but no commit made), HEAD is still the *old,
     pre-merge* commit — the tag silently points at the wrong tree with no
     error. Verify immediately: `git rev-parse rebase-target^{tree}` must
     equal `git write-tree` run right after the commit, not before it. Also
     write down `git rev-parse HEAD^{tree}` somewhere durable — don't rely on
     the tag alone surviving, see Step 3.
  5. Set `git config rerere.autoupdate true`, then reset the scratch
     branch/worktree back to the pre-rebase tip. Don't keep the merge commit
     itself — its job was to train rerere with correct postimages for every
     conflicting hunk and to give you something to diff the real rebase
     against.
- **Phase 2 — the real rebase**, on the actual branch, with rerere now
  enabled and pre-trained: `git rebase main`.
  **Caveat, confirmed the hard way on a 176-vs-255-commit rebase: rerere
  training from one accumulated merge does NOT reliably cover a
  commit-by-commit replay's conflicts, and the gap is worst on exactly the
  files Step 1.5 flagged as heaviest.** rerere's cache is keyed on the exact
  conflict *hunk text* — the pre-image both sides produce at that exact spot.
  A single accumulated merge produces **one** conflict per file, spanning the
  *whole* accumulated diff; a commit-by-commit rebase produces a **new,
  differently-shaped** conflict at every commit that touches the file, each
  one a different slice of that same territory. These essentially never
  match the cache recorded from the single merge. On the file where this bit
  hardest (an append-only ledger main had purged/restructured while the
  branch kept accumulating on the old shape), the **very first replayed
  commit** produced a 2,400-line conflict block rerere had nothing cached
  for — and every other commit touching that file was expected to be the
  same size. Do not assume "rerere is pre-trained" means the replay will be
  mostly automatic for a file with a double-digit touch count on one side and
  a restructure on the other; expect to manually resolve it at *every*
  touching commit, same effort as Phase 1, repeated N times.
  **Given that, spot-check before committing to the full replay:** let the
  rebase run and look at the size/frequency of the first few real stops. If
  they're small and rerere is genuinely absorbing most of them, continue. If
  the first stop alone reproduces a mega-conflict like the one above, stop,
  `git rebase --abort` (nothing is lost — `rebase-target` and the backup ref
  from Step 3 are untouched), and take the finding back as a decision point:
  is preserving this branch's individual commits actually worth N more
  rounds of that cost? A branch that's mostly fix-up-on-fix-up rather than
  atomic, independently-reviewable commits (check: has it already been
  squashed/reordered once before, or does `git log` show mostly "fix again"/
  "actually fix" messages?) usually isn't — see the merge-commit landing path
  below.
- **Phase 3 — the structural check that makes this worth doing, if you
  finish the real rebase:**
  ```bash
  git diff rebase-target HEAD
  ```
  Empty diff means every intermediate replay resolution was consistent with
  the once-carefully-resolved target. Any output here is a mis-resolution
  somewhere in the commit-by-commit replay that would otherwise be invisible
  — investigate every line before proceeding to verification.

**Land as a single merge commit, not a rebase replay** — appropriate when
either Step 1.5's touch-count/restructure signal is strong, or Phase 2's
empirical spot-check above shows the per-commit conflict cost is much higher
than expected, or the branch's own commits were never atomic/independently
reviewable units to begin with (so replaying them one-by-one buys nothing a
single accumulated resolution doesn't already give you):

- Do Phase 0 and Phase 1 above exactly as written (tripwire tests, then the
  throwaway merge, resolved once and committed) — **except it isn't
  throwaway**. Run Step 4a (compile-error triage) and the full verification
  in Steps 6-9 against this commit directly, and once it's fully green, this
  commit *is* the result — there's no Phase 2/3 replay to run.
- Rewrite the commit message before landing (`git commit --amend -m ...`) to
  describe the merge for real (what was reconciled, why a merge commit
  instead of a replay, what was verified) rather than leaving whatever
  placeholder message described it as a training/throwaway pass. Confirm the
  amend didn't change the tree (`git rev-parse HEAD^{tree}` before and after
  must match) and re-point any `rebase-target`-style tag at the amended
  commit (`git tag -f rebase-target`).
- Landing mechanics differ from the rebase path — see Step 11.

If you're not sure which shape you're in, run Step 1's reconnaissance (and
1.5's touch-count check) first — it tells you directly.

## Step 3 — Safety net (before rewriting anything)

```bash
git branch <branch>-prerebase-<YYYYMMDD> <branch>     # e.g. wip-prerebase-20260828
```

Record the **raw commit hash** too, in scrollback or a scratch file — don't
trust the branch/tag ref alone to survive the session. A backup tag has
silently vanished mid-rebase before with no `rebase --abort` and no explicit
deletion involved (cause unconfirmed); the hash in a shell variable or a
written note is what actually held up as the real safety net that time. Every
tree-equality check in Step 6/7 should reference that literal hash, not just
the ref name.

## Step 4 — Resolving conflicts: judgment notes

At every stop, `git diff` before `git add` — never stage blind.

- **"Keep both" is correct far more often than it looks.** Two disjoint
  additions to the same import block, the same doc section, the same enum —
  don't default to picking one side just because the diff shows a clean
  3-way marker; check whether the real resolution is literally both hunks
  concatenated.
- **Never take one side's function/struct body wholesale without diffing the
  full field set it returns or touches.** A struct rewrite that drops a field
  can compile cleanly and pass tests if the field is only exercised by an
  end-to-end path neither side's unit tests cover (mocked in the caller's own
  tests). When a function was *restructured* on one side (e.g. split into two
  composable halves, or rebuilt around a new resolver table) while the other
  side *extended the old monolith*, the right move is usually: adopt the new
  structure, then **port your side's specific additions into the
  corresponding new location** — not keep both bodies, and not blindly take
  the "bigger" side. Read the other side's commit message for *why* it
  restructured; it usually tells you exactly where your lines belong now.
- **A "new helpers collided" conflict may actually be a stale-function
  trap** — read past the diff markers to check whether one side already
  extracted/renamed what the conflict hunk is fighting over further down the
  same file, and a later commit on that side deleted the old monolith
  entirely. Losing your side's addition here is easy to miss because
  `DashMap`/`HashMap`-style `.remove()` calls on an absent key are silent
  no-ops — nothing errors, it just quietly stops doing anything. Lean on
  Phase 0's tripwire tests (or write one now) to catch it.
- **A conflict resolved by keeping "both sides" of a duplicate
  function/route/registration can leave a second, silent copy elsewhere that
  the language's own compiler never catches.** Two independent security
  fixes landed a duplicate `run_setup_script_http` handler once — Rust's
  duplicate-definition error (E0428) caught *that* immediately. But the merge
  had *also* carried over both sides' `.route("/worktrees/run-script", ...)`
  registrations in the axum router-builder file, which is not a
  compile-time duplicate (each call to `.route()` is syntactically fine on
  its own) — it only surfaced as a runtime panic ("Overlapping method
  route") the first time a test actually built the router. When you resolve
  a duplicate-definition conflict by deleting one side's copy, grep for
  every *other* place that symbol/route/table entry is referenced or
  registered, not just its own definition site — the compiler will not find
  a leftover twin for you unless the twin happens to be a second definition
  in the same namespace.
- **Append-only ledger files (`CHANGELOG.md`, `to-test.md`, `todo.md`):** a
  conflict where "ours" is empty at a given location does **not** always mean
  "this is a duplicate that the other side will re-add" — it can just as
  easily mean "this content genuinely doesn't exist yet and must be kept."
  The only safe check is to search the **already-resolved portion of the
  file so far**, plus the original commit's own diff (`git show <hash> --
  <file>`), for the exact heading text before deciding duplicate-to-drop vs.
  unique-to-keep. Getting this backwards has silently dropped a real section
  once and, in the opposite direction, kept a stale duplicate that corrupted
  two downstream commits into literal `<<<<<<< HEAD` text baked into history.
- **rerere can replay a stale, wrong resolution** once an earlier commit in
  the chain has been amended to fix a mistake mid-rebase — it matches on
  textual similarity, not semantics, and can silently apply an old cached
  resolution to a conflict that only looks the same. If you have to `rebase
  -i --edit` and amend something mid-flight to fix a discovered corruption,
  temporarily `git config rerere.enabled false` for that corrective pass and
  re-enable afterward — don't trust it blindly through a fixup.
- **A deliberate-choice conflict is not a mechanical one.** When both sides
  added semantically-equivalent-but-differently-styled code (e.g. one side
  added two explicit special cases where your side had already generalized
  the same two cases into a shared helper), the right resolution is usually
  to extend *your* generalization to cover the new cases, not to keep the
  other side's one-off special-casing. Check whether either side has its own
  cross-check test (a table/enum consistency test) that will need updating
  either way.
- **Transient `git commit`/`rebase --continue` failures** ("you have staged
  changes", a `MERGE_RR.lock` error) can come from an unrelated concurrent
  process (Sourcetree, a background `git fetch`) touching the same repo.
  Harmless — retry `git rebase --continue` (or `git commit --no-edit` then
  `--continue`).
- If a conflict shows up in a file Step 1's whole-branch preview said was
  clean, **stop and inspect** — it means something changed since recon (a
  concurrent push, a stale preview) rather than "recon was wrong"; re-run the
  preview against the current tips before proceeding.
- **Delegating a seam to a subagent produces a hypothesis, not a verified
  fact — especially across a wrapper/delegation layer.** An audit subagent
  once flagged a ref method as "missing" by checking the component that
  *would* implement it directly, without checking whether a wrapper one
  layer up (the object actually registered as the live ref) already
  delegated to a differently-named real method. The finding was plausible,
  specific, and wrong. Passing an inherited finding straight into a second
  subagent's fix instructions compounds the mistake — it reads as confirmed
  because you're the one relaying it. Before accepting (or forwarding) a
  "this is broken" claim about a symbol with more than one layer between its
  declaration and its use, check the *actual* call site's real, live
  resolution yourself (or say explicitly "verify this against the real
  consumer before touching anything").

## Step 4a — Getting from "conflicts resolved" to "it actually compiles"

For anything beyond the smallest rebase, resolving every `<<<<<<<` marker
does not mean the tree builds — expect a real, possibly multi-hour, separate
pass of iterating `cargo check`/`tsc` (or your stack's equivalent) to zero
before the full test suite is worth running at all. This phase has its own
failure shapes, distinct from Step 4's conflict-marker judgment:

- **A single dropped syntax token near the end of a heavily-edited file can
  cascade into 50-100+ unrelated-looking errors across the whole crate.** A
  file split (e.g. one monolith's test module extracted to `mod tests;` in a
  sibling file) that lost its trailing `mod tests;` declaration during
  conflict resolution — a dangling `#[cfg(test)]` attribute with nothing
  following it — poisoned the entire compilation unit. The resulting error
  count (~165 of ~220 total) looked like a huge, unrelated mess spread across
  a dozen files; the actual defect was one missing line. **When a post-merge
  error count is implausibly large and scattered across files you didn't
  expect to be affected, check for exactly this before triaging errors one at
  a time**: `git show main:<file> | tail -5` (or the equivalent for whichever
  side did the split) to compare the real, correct file ending against what's
  currently there. Fixing this class of defect can collapse the apparent
  problem size by 3-4x in a single edit.
- **A "flatten struct into subsystem-structs" refactor (or the reverse) makes
  the compiler's own suggestions do most of the mechanical work — but only
  the *singular* ones.** Rust's "help: one of the expressions' fields has a
  field of the same name" (singular "one") names an unambiguous rename and is
  safe to batch-apply — write a small script that parses every such
  diagnostic out of a `cargo check` log and applies the suggested insertion
  directly; this can fix dozens of call sites in one pass. **"help: *some*
  of the expressions' fields have a field of the same name" (plural) is not
  safe to auto-apply** — it means two *different* fields with the same short
  name exist in different subsystems (e.g. an MCP-protocol `sessions` map and
  a PTY `sessions` map, both reachable as `state.<x>.sessions`), and picking
  the wrong one compiles fine while silently reading/writing the wrong
  table. Read the field's real type and the call site's actual intent before
  choosing, every time you see the plural form.
- **This is also where genuinely dropped invariants and architectural gaps
  surface, not just renames** — a function whose helper still referenced an
  old flat field name instead of the new nested path is a real bug the
  compiler *will* catch (unlike the silent, no-marker cases Step 1's
  silent-risk list exists for); don't wave every compile error through as
  "just a mechanical rename" without reading what the line actually does.
- Re-run the scoped check (`cargo check --lib -p <crate>`, not
  `--workspace --all-targets`, which can produce spurious cross-workspace
  feature-unification errors that look real but aren't) after each batch of
  fixes to track real progress, and don't declare this phase done until both
  the library and its test target (`--lib --tests`) compile clean — a test
  module can have its own, separate set of stale signatures even after the
  library itself is fully fixed.
- For large post-merge cleanup at this stage, delegating to parallel
  subagents works well **as long as their file sets are disjoint from each
  other and from anything you're concurrently building/testing** — but a
  single "make X fully pass" task (get `tsc`/the full test suite green
  across a hundred-plus errors) can be big enough to exhaust a subagent's own
  turn budget before finishing. Don't treat "no final report yet" as "still
  working correctly" — after a long delegation returns, verify the *actual*
  current state yourself (rerun the check directly) rather than trusting the
  subagent's last message, which may describe a plan ("now let's check X")
  rather than a completed result.

## Step 5 — Post-rebase ledger reconciliation

If you used a union-merge driver (Step 2) or plain "keep both" resolutions
repeatedly on append-only files:

```bash
rm .git/info/attributes   # (or: git config --worktree --unset core.attributesFile,
                          #  if you used the worktree-private variant in Step 2)
```

Union/keep-both merging is purely additive — it can duplicate near-identical
lines or leave a section split across two now-adjacent headings (literally:
multiple `### Added`/`### Changed`/`### Fixed` sub-headings under one `##
[Unreleased]`, one per side that touched the file — consolidate back to
exactly one of each, in Keep-a-Changelog order, concatenating each side's
bullets under it in their original relative order; don't re-sort by topic).
Do one editorial commit per ledger file, diffing against **both** parents:

```bash
diff <(git show main:CHANGELOG.md) CHANGELOG.md
```

For `CHANGELOG.md` specifically: if main released a version during the
divergence window, everything that was under the base's `[Unreleased]` and
that main already shipped must end up **only** under main's version heading —
`[Unreleased]` should hold only entries unique to your branch. Verify no
entry appears in both sections. Commit as something like `chore(changelog):
reconcile Unreleased against the vX.Y.Z release`.

For `to-test.md` specifically, check whether main **purged** stale entries
during the divergence window (a curated cleanup, not just additions) — if
so, a plain union/keep-both merge will silently resurrect everything main
deliberately removed. Take main's post-purge structure as the base and
replay only your branch's entries dated after the purge (or whose subject
doesn't match one of the removed/refiled items); this needs an actual
read-through per entry, not a mechanical merge tool.

## Step 6 — Full verification recipe

Every one of these has independently caught a real problem in a past rebase
on this repo — skipping any one has missed something before:

1. **Commit count** roughly matches expectation (a few fewer than naive
   arithmetic is normal — a fix-up whose entire delta is already satisfied by
   an earlier resolution gets auto-dropped as empty by git; that's expected,
   not a loss). **Not applicable if you landed as a single merge commit
   (Step 2) — skip to item 2.**
2. `git diff --name-only <backup-hash> HEAD` lists only the files you
   expected to change.
3. Per-ledger-file `diff <(sort <old>) <(sort <new>)` is empty (same lines,
   different order/section — no loss, no duplication).
4. Heading-count sanity: `grep -c '^## ' <file>` before/after, and
   `grep '^## ' <file> | sort | uniq -c` shows no entry `>1`. For a
   consolidated `[Unreleased]` section specifically, also check
   `### `-level sub-headings the same way — exactly one `### Added`, one
   `### Changed`, one `### Fixed` (Step 5).
5. `git log -p <range> | grep -nE '^\+<<<<<<<|^\+>>>>>>>'` — literally
   committed stray conflict markers, baked into a real commit. This has
   happened (a stale rerere replay), and it's otherwise invisible. **Not
   applicable to a single merge commit** (there's no per-commit range to
   grep) — instead just re-read the final diff for any leftover marker text.
6. **If you completed a full rebase replay (Step 2, Phase 2/3):** `git diff
   rebase-target HEAD` is empty.
7. A commit whose message still carries a leftover `# Conflicts: <file>`
   trailer and claims to be `test-only`/`fix-up` is worth a second look — that
   trailer is the tell that a real production hunk may have been dropped
   during conflict resolution, with only a test hunk surviving. If the
   underlying bug happens to already be fixed elsewhere, this is a
   near-miss, not a live bug — but audit the diff, don't just trust the
   commit subject.
8. **A test failure that looks like a rebase regression may be caused by
   *your own machine's* git config, not the code.** Before spending time on
   a git-behavior-dependent test failure (a synthetic-merge-conflict fixture
   that unexpectedly stays "clean", a raw-URL-vs-`git remote get-url`
   mismatch), reproduce the *exact* git operation the test performs in a
   throwaway scratch repo, outside the test harness entirely. If it fails
   the identical way there, the cause is a personal global git setting
   (`merge.ff=only`, a `url.*.insteadOf` rewrite, etc.), not a defect from
   the rebase — confirmed twice as the actual explanation in one session.
   Don't touch global git config to work around this; just document the
   finding and move on.
9. **`cargo audit`/`pnpm audit` (part of the real check gate, Step 8) can
   surface a dependency CVE that's unrelated to the rebase but still blocks a
   clean gate.** If a `cargo update -p <crate>` bump doesn't fully clear the
   advisory, check whether it only advanced to an intermediate patch version
   (other transitive constraints can cap a bare `cargo update -p <crate>`
   short of the actually-fixed release) — `cargo update -p <crate> --precise
   <fixed-version>` forces the specific version, after which re-run the
   audit to confirm it's actually clean, not just "a version changed."

## Step 7 — Post-rebase semantic audit

Text merging cleanly does not mean it merged *correctly*. Go back to Step 1's
silent-risk list and, for each file, verify the specific invariant that made
it risky — not just that it compiles:

- Any shared registry/enum/table that both sides added entries to (an IPC
  command table, a hook-derivation map, a status-label set, a mode-resolution
  table) — confirm **both** sides' new entries are present and the
  cross-check test for that table (if one exists) is updated, not just
  passing by coincidence.
- Any symbol one side deleted/renamed while the other side kept using the old
  name — grep for the old name across the whole tree, not just the
  conflicting files.
- Any tri-state/optional/nullable field resolution order — confirm `override
  ?? default` semantics (never the reverse) survived the merge if both sides
  touched settings resolution.
- Version strings (`package.json`, `Cargo.toml`, `Cargo.lock`, doc files) — if
  main bumped a version during the divergence, confirm every file that
  states a version agrees after the rebase.
- Anything Rust-touching that needs a real running app to observe (this repo's
  `make dev` never hot-reloads Rust) — don't claim it's verified; add
  `to-test.md` entries instead, and check whether main already shipped its
  own entries for the same area that need to survive ledger reconciliation.
- This is a good phase to fan out to a few parallel read-only audit
  subagents, one per risk cluster (e.g. one per architectural seam Step 1
  identified) — explicitly tell each one to verify claims against
  `git show main:<file>`/`git show <backup-hash>:<file>` directly rather than
  trusting anything a plan document or an earlier session already asserted;
  plan documents can themselves be stale or wrong by the time you're auditing
  against the real, current tree.

## Step 8 — Run the real check gate

Use the `check-gate` skill (`./scripts/check-gate.sh` / `make check-gate`) —
**never** a scoped test filter, and never pipe its output through `tee`,
`tail`, `grep`, or anything else and trust the reported exit code (that
reports the piped-to command's exit status, not the underlying command's).
Redirect to a file and read that file's own recorded exit code, or use the
wrapper script, which handles this correctly. **This applies to every
individual verification command in this whole workflow, not just the
top-level gate script** — piping a single tool's own output through `tail`
to see the interesting part (e.g. `cargo audit -q | tail -40; echo
$?`) captures `tail`'s exit code, not the tool's, and a real failure (a live
RUSTSEC advisory, not just informational warnings) can read as a clean pass.
Redirect to a file, then check the real exit code of the command itself in
its own statement, separately from anything you do to inspect its output.

A large rebase is exactly the situation where a repo-wide consistency test
(IPC/HTTP command-table parity, a `DERIVATIONS`↔`help_text()` cross-check, a
coverage-ratchet threshold) is most likely to have silently drifted — these
only run in the full suite.

Distinguish real regressions from this repo's known pre-existing flakes
(`ChangelogModal.test.tsx`'s async-leak, the uninitialized `plugins`
submodule reporting zero tests) — `check-gate.sh` calls both out itself. A
gate step that runs commit-by-commit history checks (item 1/5/6 of Step 6)
doesn't apply if you landed as a single merge commit — see Step 2.

## Step 9 (optional, for a large/long-lived rebase) — Full post-rebase branch audit

For a branch that's been diverging and accumulating fix-ups over weeks (not
a quick catch-up rebase), consider a full commit-by-commit review pass after
the rebase lands, modeled on `plans/branch-review-2026-08-27.md`: split the
commit range by theme across parallel review subagents, plus one dedicated
pass sweeping session transcripts for issues that were found and explicitly
deferred but never filed anywhere durable. This is what catches things no
single-file diff review will: a commit mislabeled as a pure formatting
fix-up that's actually a substantive fix with a false diagnosis in its own
message, a doc claim that's been stale across two later "fix stale docs"
commits, a safety-dialog default that doesn't match its own stated goal, or
a feature with zero `to-test.md` coverage despite needing a restart to
verify. Fold confirmed fixes into their own `fix-up <hash>` commits rather
than rewriting the original commits again. **Not meaningful if you landed as
a single merge commit** — there's no per-commit range to review; rely on
Step 7's file-by-file semantic audit instead.

## Step 10 — Get explicit confirmation on how to land it

Before Step 11, confirm with the user which shape actually happened, even if
Step 2 already named a plan going in — a large rebase can (and did, once)
discover mid-flight that the cheap path costs far more than expected, which
changes the landing shape from "commit-by-commit rebase, preserving
history" to "single merge commit, history preserved only on the backup ref."
This is exactly the kind of decision only the person requesting the rebase
should make, informed by the *real*, empirically-observed cost (Step 2's
Phase 2 spot-check), not the a priori estimate from Step 1.

## Step 11 — Land it

**If you completed a full commit-by-commit rebase:**

```bash
git branch -f <branch> <new-tip>                  # from the primary checkout, if <branch> isn't checked out there
git stash pop                                      # restore anything stashed in Step 0 (e.g. a submodule pointer bump)
git push --force-with-lease origin <branch>        # NEVER a bare --force
```

**If you landed as a single merge commit instead:** from the primary
checkout, where `<branch>` is checked out — `git branch -f` refuses to force
a branch onto a new tip while it's the checked-out branch, so use
`git reset --hard <new-tip>` there instead (safe: it only touches tracked
files, never the untracked scratch paths Step 0 told you to leave alone).
Confirm no live dev process is running against that checkout first (`ps aux
| grep -E "tauri dev|vite"` or your stack's equivalent) before resetting it
out from under anything that might be watching the filesystem.

```bash
git reset --hard <new-tip>                         # from the primary checkout
git push --force-with-lease origin <branch>         # NEVER a bare --force
```

Either way:

- **Fetch immediately before pushing** (`git fetch origin <branch>`) and
  confirm `origin/<branch>` still matches what you expect, so
  `--force-with-lease` is actually checking against current reality, not a
  fetch from hours ago.
- Get explicit confirmation before this push — it rewrites shared history for
  everyone tracking `<branch>`, and `--force-with-lease` only protects against
  someone else having pushed since your last fetch, not against a mistake in
  what you're about to publish.
- **If the push fails on SSH auth** (`sign_and_send_pubkey: signing failed
  ...`, `Permission denied (publickey)`), check `ssh-add -l` — "The agent has
  no identities" means the user's key isn't loaded, which only they can fix
  (their own passphrase/Touch ID prompt). Don't attempt a workaround (no
  `GIT_SSH_COMMAND` overrides, no switching remotes to HTTPS unasked); ask
  them to run `ssh-add <key>` themselves (suggest the `! <command>` prefix so
  it runs in their own interactive session) and retry once they confirm.
  Verify nothing was left in a bad state either way:
  `git rev-parse origin/<branch>` should still show the *old* tip after a
  failed push — a failed push never partially applies.

**Sibling branches/worktrees that are strict ancestors of `<branch>` with no
unique commits of their own are untouched by the rebase** — their refs keep
the pre-rebase commits alive, and each needs its own separate
`git rebase --onto <branch> <old-tip> <sibling-branch>` when its owner is
ready. That's a distinct, separate decision — don't do it as part of this
task unless explicitly asked.

## Rollback

```bash
git rebase --abort                                       # mid-rebase, any time before it finishes
git reset --hard <branch>-prerebase-<YYYYMMDD>            # after a completed but wrong rebase/merge, before push
```

Keep the backup ref until the force-push is verified good on the remote —
delete it only after that.
