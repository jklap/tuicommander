#!/usr/bin/env python3
"""Build a feature-squashed history in which every step is a clean 3-way merge.

    plan.py --repo R --base B --stripped S --orig O --out DIR
            [--ledgers "a b"] [--personal-paths "x y"] [--personal-re RE]
            [--main-subjects FILE] [--window N]

S is O with the ledger and personal paths reset to B's version in every commit (see
squash-wip.sh). Walking S oldest first, each commit is placed:
  * at the end of the last block when that block is the same group, else
  * back into the nearest earlier block of its group, only if the commit AND every block
    after that point still apply cleanly (`git merge-tree` on trees, no worktree), else
  * as a new block.
A second pass folds whole blocks back into an earlier same-group block under the same rule.
"upstream" commits stay single, in place. Then one ledgers commit restores O's ledger files
(minus the .gitignore lines personal commits added) and one personal commit restores O's
personal paths and its .gitignore. Commits are written with `git commit-tree`, so nothing can
stop on a conflict, and nothing is written unless the final tree equals O's.

Prints the new tip on stdout and writes DIR/blocks.md, the reviewable todo.
"""
import argparse
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import classify as C  # noqa: E402


class PlanError(Exception):
    pass


class Git:
    def __init__(self, repo, scratch):
        self.repo, self.scratch = repo, scratch

    def run(self, *args, ok=(0,), env=None, inp=None):
        p = subprocess.run(["git", "-C", self.repo, *args], capture_output=True, text=True,
                           env={**os.environ, **(env or {})}, input=inp)
        if p.returncode not in ok:
            raise PlanError(f"git {' '.join(args)} failed: {p.stderr.strip()}")
        return p

    def out(self, *args, **kw):
        return self.run(*args, **kw).stdout

    def tree(self, rev):
        return self.out("rev-parse", f"{rev}^{{tree}}").strip()

    def merge(self, base, ours, theirs):
        """Apply the change base->theirs onto ours. The merged tree, or None on any conflict."""
        if base == theirs:
            return ours
        if base == ours:
            return theirs
        p = self.run("merge-tree", "--write-tree", "--no-messages", f"--merge-base={base}", ours,
                     theirs, ok=(0, 1))
        return p.stdout.split("\n", 1)[0] if p.returncode == 0 else None

    def with_paths(self, tree, paths, source=None, extra=""):
        """`tree` with `paths` removed, then source's entries for them (and `extra`) added."""
        env = {"GIT_INDEX_FILE": os.path.join(self.scratch, "plan.index")}
        self.run("read-tree", tree, env=env)
        if paths:
            self.run("rm", "-r", "-q", "-f", "--cached", "--ignore-unmatch", "--", *paths, env=env)
        entries = (self.out("ls-tree", "-r", source, "--", *paths) if source and paths else "") + extra
        if entries:
            self.run("update-index", "--index-info", env=env, inp=entries)
        t = self.out("write-tree", env=env).strip()
        os.remove(env["GIT_INDEX_FILE"])
        return t


class Block:
    def __init__(self, group, base, result, members):
        self.group, self.base, self.result, self.members = group, base, result, members


def reapply(git, blocks, prev):
    """Re-apply blocks in order onto prev. List of (base, result), or None if one conflicts."""
    out = []
    for b in blocks:
        r = git.merge(b.base, prev, b.result)
        if r is None:
            return None
        out.append((prev, r))
        prev = r
    return out


def fold_into(git, blocks, j, base, result):
    """New (base, result) for blocks[j:] after folding base->result into blocks[j], or None."""
    t = git.merge(base, blocks[j].result, result)
    if t is None:
        return None
    rest = reapply(git, blocks[j + 1:], t)
    return None if rest is None else [(blocks[j].base, t)] + rest


def _set(blocks, start, trees):
    for b, (nb, nr) in zip(blocks[start:], trees):
        b.base, b.result = nb, nr


def build_blocks(git, commits, base_tree, window=60):
    """Pass 1 + pass 2. commits: dicts with h, g; already stripped and grouped."""
    blocks, tip = [], base_tree
    for c in commits:
        cb, ct = git.tree(f"{c['h']}^"), git.tree(c["h"])
        g, placed = c["g"], False
        if blocks and blocks[-1].group == g and g != "upstream":
            t = git.merge(cb, tip, ct)
            if t is not None:
                blocks[-1].result, tip, placed = t, t, True
                blocks[-1].members.append(c)
        if not placed and g != "upstream":
            j = next((j for j in range(len(blocks) - 2, max(-1, len(blocks) - 1 - window), -1)
                      if blocks[j].group == g), None)
            if j is not None:
                trees = fold_into(git, blocks, j, cb, ct)
                if trees is not None:
                    _set(blocks, j, trees)
                    blocks[j].members.append(c)
                    tip, placed = blocks[-1].result, True
        if not placed:
            t = git.merge(cb, tip, ct)
            if t is None:
                raise PlanError(f"{c['h'][:12]} {c['s']!r} does not apply even in its original place")
            blocks.append(Block(g, tip, t, [c]))
            tip = t

    changed = True
    while changed:
        changed = False
        for k in range(len(blocks) - 1, 0, -1):
            g = blocks[k].group
            if g == "upstream":
                continue
            j = next((j for j in range(k - 1, max(-1, k - 1 - window), -1) if blocks[j].group == g), None)
            if j is None:
                continue
            redo = reapply(git, blocks[k + 1:], blocks[k].base)
            if redo is None:
                continue
            trial = [Block(b.group, b.base, b.result, list(b.members)) for b in blocks[:k] + blocks[k + 1:]]
            _set(trial, k, redo)
            trees = fold_into(git, trial, j, blocks[k].base, blocks[k].result)
            if trees is None:
                continue
            _set(trial, j, trees)
            trial[j].members += blocks[k].members
            blocks, changed = trial, True
            break
    return blocks


def personal_gitignore_lines(git, base, orig, personal_re):
    lines = []
    for row in filter(None, git.out("log", "--format=%H%x1f%s", f"{base}..{orig}").split("\n")):
        h, s = row.split("\x1f")
        if re.search(personal_re, s):
            for d in git.out("show", "--format=", h, "--", ".gitignore").splitlines():
                if d.startswith("+") and not d.startswith("+++") and d[1:].strip():
                    lines.append(d[1:])
    return lines


def ledger_and_personal(git, tip, orig, ledgers, personal_paths, gi_lines):
    """(ledgers tree, personal tree) on top of tip."""
    gi_ledger = ".gitignore" in ledgers
    plain = [p for p in ledgers if p != ".gitignore"]
    extra = ""
    if gi_ledger:
        lines = git.out("show", f"{orig}:.gitignore").split("\n")
        for pl in gi_lines:
            if pl in lines:
                lines.remove(pl)
        blob = git.out("hash-object", "-w", "--stdin", inp="\n".join(lines)).strip()
        extra = f"100644 blob {blob}\t.gitignore\n"
        plain_tree = git.with_paths(tip, plain + [".gitignore"], orig, extra)
    else:
        plain_tree = git.with_paths(tip, plain, orig)
    personal = list(personal_paths) + ([".gitignore"] if gi_ledger else [])
    return plain_tree, git.with_paths(plain_tree, personal, orig)


SQUASHED_RE = re.compile(r"^Squashed from \d+ commits?:$")


def member_lines(git, c):
    """The `- subject (hash)` lines a member contributes; a squashed member contributes its list."""
    body = git.out("log", "-1", "--format=%B", c["h"]).splitlines()
    for i, line in enumerate(body):
        if SQUASHED_RE.match(line):
            return [x for x in body[i + 1:] if x.startswith("- ")]
    return [f"- {c['s']} ({c['orig'][:9]})"]


def message(git, b, title):
    if b.group == "upstream":
        return git.out("log", "-1", "--format=%B", b.members[0]["h"])
    lines, trailers = [], []
    for c in b.members:
        lines += member_lines(git, c)
        for t in git.out("log", "-1", "--format=%(trailers:only,unfold)", c["h"]).splitlines():
            t = t.strip()
            if t and not t.startswith("Squash-Group:") and t not in trailers:
                trailers.append(t)
    msg = title + "\n"
    if lines:
        msg += f"\nSquashed from {len(lines)} commits:\n\n" + "\n".join(lines) + "\n"
    return msg + "\n" + "\n".join(trailers + [f"Squash-Group: {b.group}"]) + "\n"


def write(git, blocks, base, orig, out, ledgers):
    counts, seen, md = {}, {}, []
    for b in blocks:
        counts[b.group] = counts.get(b.group, 0) + 1
    parent = git.out("rev-parse", base).strip()
    for b in blocks:
        seen[b.group] = seen.get(b.group, 0) + 1
        title = C.GROUPS.get(b.group, (f"chore({b.group}): squashed", 0))[0]
        if counts[b.group] > 1 and b.group != "upstream":
            title += f" (part {seen[b.group]}/{counts[b.group]})"
        msg = message(git, b, title)
        env = {}
        if b.members:
            name, email, date = git.out("log", "-1", "--format=%an%x1f%ae%x1f%aI",
                                        b.members[-1]["h"]).strip().split("\x1f")
            env = {"GIT_AUTHOR_NAME": name, "GIT_AUTHOR_EMAIL": email, "GIT_AUTHOR_DATE": date}
        parent = git.out("commit-tree", b.result, "-p", parent, "-F", "-", env=env, inp=msg).strip()
        md.append(f"\n# {msg.split(chr(10), 1)[0]}  [{len(b.members)} commit(s)]")
        for i, c in enumerate(b.members):
            md.append(f"{'pick ' if i == 0 or b.group == 'upstream' else 'fixup'} {c['orig'][:9]} {c['s']}")
        if b.group == "ledgers":
            md.append(f"exec git checkout {orig[:9]} -- {' '.join(ledgers)}  # minus personal .gitignore lines")
    with open(os.path.join(out, "blocks.md"), "w") as f:
        f.write("\n".join(md).lstrip("\n") + "\n")
    return parent


def plan(repo, base, stripped, orig, out, ledgers, personal_paths, personal_re,
         main_subjects=None, window=60, log=sys.stderr):
    os.makedirs(out, exist_ok=True)
    git = Git(repo, out)
    commits = C.assign_groups(C.read_commits(repo, base, stripped), C.load_subjects(main_subjects))

    # DEFERRED (2026-10-27): commits are mapped back to their original hash by (subject, author
    # date). Two commits with the same subject in the same second show the wrong original hash in
    # blocks.md and "Squashed from"; trees are unaffected. filter-branch's commit map would be exact.
    origs = {}
    for row in filter(None, git.out("log", "--format=%H%x1f%s%x1f%aI", f"{base}..{orig}").split("\n")):
        h, s, d = row.split("\x1f")
        origs.setdefault((s, d), h)
    for c in commits:
        d = git.out("log", "-1", "--format=%aI", c["h"]).strip()
        c["orig"] = origs.get((c["s"], d), c["h"])

    leftover = [c for c in commits if c["g"] in ("personal", "ledgers")]
    if leftover:
        raise PlanError("personal/ledger commits still change other paths after stripping; add those "
                        "paths to WIP_PERSONAL_PATHS or WIP_LEDGERS:\n" +
                        "\n".join(f"  {c['orig'][:12]} {c['s']}: {' '.join(c['files'][:6])}" for c in leftover))

    blocks = build_blocks(git, commits, git.tree(base), window)
    tip = blocks[-1].result if blocks else git.tree(base)

    gi_lines = personal_gitignore_lines(git, base, orig, personal_re)
    lt, pt = ledger_and_personal(git, tip, orig, ledgers, personal_paths, gi_lines)
    personal_members = []
    for row in filter(None, git.out("log", "--reverse", "--format=%H%x1f%s%x1f%(trailers:key=Squash-Group,valueonly,separator=)",
                                    f"{base}..{orig}").split("\n")):
        h, s, tg = row.split("\x1f")
        if re.search(personal_re, s) or tg.strip() == "personal":
            personal_members.append(dict(h=h, s=s, orig=h))
    if lt != tip:
        blocks.append(Block("ledgers", tip, lt, []))
    if pt != lt:
        blocks.append(Block("personal", lt, pt, personal_members))

    if pt != git.tree(orig):
        raise PlanError("the planned history does not end on the original tree; nothing written")
    new_tip = write(git, blocks, base, orig, out, ledgers)
    split = {g: n for g, n in ((g, sum(b.group == g for b in blocks)) for g in {b.group for b in blocks})
             if n > 1 and g != "upstream"}
    print(f"   {len(commits)} commits -> {len(blocks)} blocks; split groups: {split}", file=log)
    return new_tip


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    for a in ("repo", "base", "stripped", "orig", "out"):
        ap.add_argument(f"--{a}", required=True)
    ap.add_argument("--ledgers", default="")
    ap.add_argument("--personal-paths", default="")
    ap.add_argument("--personal-re", default=r"^personal scripts$")
    ap.add_argument("--main-subjects")
    ap.add_argument("--window", type=int, default=60)
    a = ap.parse_args(argv)
    try:
        print(plan(a.repo, a.base, a.stripped, a.orig, a.out, a.ledgers.split(), a.personal_paths.split(),
                   a.personal_re, a.main_subjects, a.window))
    except PlanError as e:
        sys.exit(f"plan.py: {e}")


if __name__ == "__main__":
    main()
