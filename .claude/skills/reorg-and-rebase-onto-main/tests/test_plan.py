"""plan.py on small synthetic repos: Git.merge, build_blocks and plan() end to end."""
import os
import subprocess

import pytest

import plan as P

LEDGERS = ["CHANGELOG.md", "to-test.md", ".gitignore"]
PERSONAL = ["build.sh"]
PERSONAL_RE = r"^personal scripts$"
LINES = "".join(f"line {i}\n" for i in range(10))


class Repo:
    def __init__(self, path):
        self.path, self.n = path, 0
        path.mkdir()
        self.git("init", "-q")

    def git(self, *args, inp=None, env=None):
        return subprocess.run(["git", "-C", str(self.path), *args], check=True, capture_output=True,
                              text=True, input=inp, env={**os.environ, **(env or {})}).stdout.strip()

    def commit(self, msg, files=None, rm=()):
        """Write files, remove rm, commit everything with a distinct author date; the new hash."""
        for name, body in (files or {}).items():
            (self.path / name).parent.mkdir(parents=True, exist_ok=True)
            (self.path / name).write_text(body)
        for name in rm:
            (self.path / name).unlink()
        self.n += 1
        date = f"2026-01-01T00:00:{self.n:02d}Z"
        subprocess.run(["git", "-C", str(self.path), "add", "-A"], check=True)
        subprocess.run(["git", "-C", str(self.path), "commit", "-q", "--allow-empty", "-m", msg,
                        f"--date={date}"], check=True, env={**os.environ,
                                                           "GIT_COMMITTER_DATE": date})
        return self.git("rev-parse", "HEAD")

    def edit(self, name, lineno, text):
        lines = (self.path / name).read_text().splitlines(keepends=True)
        lines[lineno] = text + "\n"
        return {name: "".join(lines)}

    def strip(self, base, orig, paths):
        """What squash-wip.sh step 1 does: reset paths to base in base..orig, prune empties."""
        self.git("update-ref", "refs/squash/stripped", orig)
        p = " ".join(paths)
        self.git("filter-branch", "-f", "--prune-empty", "--index-filter",
                 f"git rm -r -q --cached --ignore-unmatch -- {p}\n"
                 f"git ls-tree -r {base} -- {p} | git update-index --index-info",
                 "--", f"{base}..refs/squash/stripped", env={"FILTER_BRANCH_SQUELCH_WARNING": "1"})
        return self.git("rev-parse", "refs/squash/stripped")

    def subjects(self, base, tip):
        return self.git("log", "--reverse", "--format=%s", f"{base}..{tip}").splitlines()

    def body(self, rev):
        return self.git("log", "-1", "--format=%B", rev)


@pytest.fixture
def repo(tmp_path):
    return Repo(tmp_path / "repo")


@pytest.fixture
def git(repo, tmp_path):
    (tmp_path / "scratch").mkdir()
    return P.Git(str(repo.path), str(tmp_path / "scratch"))


# --- Git.merge -------------------------------------------------------------------------------

def test_merge_clean(repo, git):
    base = repo.commit("base", {"a": LINES})
    ours = repo.commit("ours", repo.edit("a", 1, "ours"))
    repo.git("checkout", "-q", "-b", "side", base)
    theirs = repo.commit("theirs", repo.edit("a", 8, "theirs"))
    t = git.merge(git.tree(base), git.tree(ours), git.tree(theirs))
    content = repo.git("show", f"{t}:a").splitlines()
    assert content[1] == "ours" and content[8] == "theirs"


def test_merge_conflict_is_none(repo, git):
    base = repo.commit("base", {"a": LINES})
    ours = repo.commit("ours", repo.edit("a", 4, "ours"))
    repo.git("checkout", "-q", "-b", "side", base)
    theirs = repo.commit("theirs", repo.edit("a", 4, "theirs"))
    assert git.merge(git.tree(base), git.tree(ours), git.tree(theirs)) is None


def test_merge_shortcuts_skip_git():
    git = P.Git("/nonexistent", "/nonexistent")
    assert git.merge("B", "O", "B") == "O"
    assert git.merge("B", "B", "T") == "T"


# --- build_blocks ----------------------------------------------------------------------------

def groups_of(blocks):
    return [(b.group, [c["s"] for c in b.members]) for b in blocks]


def stack(repo, specs):
    """Commit (subject, group, files) in order; commit dicts for build_blocks."""
    return [dict(h=repo.commit(s, f), s=s, g=g) for s, g, f in specs]


def test_independent_fix_moves_back_into_its_block(repo, git):
    base = repo.commit("base", {"a": LINES, "b": LINES})
    cs = stack(repo, [("a1", "A", repo.edit("a", 1, "a1")), ("b1", "B", repo.edit("b", 1, "b1"))])
    cs += stack(repo, [("a2", "A", repo.edit("a", 8, "a2"))])
    blocks = P.build_blocks(git, cs, git.tree(base))
    assert groups_of(blocks) == [("A", ["a1", "a2"]), ("B", ["b1"])]
    assert blocks[-1].result == git.tree("HEAD")
    assert blocks[1].base == blocks[0].result


def test_commit_conflicting_with_intervening_block_starts_new_part(repo, git):
    base = repo.commit("base", {"a": LINES, "s": LINES})
    cs = stack(repo, [("a1", "A", repo.edit("a", 1, "a1")), ("b1", "B", repo.edit("s", 4, "b1"))])
    cs += stack(repo, [("a2", "A", repo.edit("s", 4, "a2"))])
    blocks = P.build_blocks(git, cs, git.tree(base))
    assert groups_of(blocks) == [("A", ["a1"]), ("B", ["b1"]), ("A", ["a2"])]
    assert blocks[-1].result == git.tree("HEAD")


def test_upstream_singletons_never_merge(repo, git):
    base = repo.commit("base", {"a": LINES, "u": LINES})
    cs = stack(repo, [("a1", "A", repo.edit("a", 1, "a1")), ("u1", "upstream", repo.edit("u", 1, "u1")),
                      ("u2", "upstream", repo.edit("u", 8, "u2")), ("a2", "A", repo.edit("a", 8, "a2"))])
    blocks = P.build_blocks(git, cs, git.tree(base))
    assert groups_of(blocks) == [("A", ["a1", "a2"]), ("upstream", ["u1"]), ("upstream", ["u2"])]
    assert blocks[-1].result == git.tree("HEAD")


def test_window_limits_how_far_back_a_commit_moves(repo, git):
    base = repo.commit("base", {"a": LINES, "b": LINES, "c": LINES})
    cs = stack(repo, [("a1", "A", repo.edit("a", 1, "a1")), ("b1", "B", repo.edit("b", 1, "b1")),
                      ("c1", "C", repo.edit("c", 1, "c1")), ("a2", "A", repo.edit("a", 8, "a2"))])
    blocks = P.build_blocks(git, cs, git.tree(base), window=1)
    assert [b.group for b in blocks] == ["A", "B", "C", "A"]


# --- plan() ----------------------------------------------------------------------------------

def make_wip(repo):
    """base + a wip history with two features, ledger edits and a personal commit; (base, tip)."""
    base = repo.commit("base", {"ui": LINES, "git": LINES, "CHANGELOG.md": "# log\n",
                                "to-test.md": "# to-test\n", ".gitignore": "target/\n"})
    repo.commit("feat(ui): pane", {**repo.edit("ui", 1, "ui1"), "CHANGELOG.md": "# log\n- ui\n"})
    repo.commit("feat(git): branch", {**repo.edit("git", 1, "git1"), "CHANGELOG.md": "# log\n- ui\n- git\n"})
    repo.commit("docs: to-test entry", {"to-test.md": "# to-test\n- [ ] ui\n"})
    repo.commit("personal scripts", {"build.sh": "#!/bin/sh\n", ".gitignore": "target/\nsecret/\n"})
    repo.commit("fix(ui): pane follow-up", {**repo.edit("ui", 8, "ui2"), ".gitignore": "target/\nsecret/\nnode_modules/\n"})
    return base, repo.git("rev-parse", "HEAD")


def run_plan(repo, tmp_path, base, tip, name="out", bare=True):
    """Strip and plan in a bare clone, as squash-wip.sh does; the new tip, fetched back into repo."""
    stripped = repo.strip(base, tip, LEDGERS + PERSONAL)
    target = repo.path
    if bare:
        target = tmp_path / f"{name}.git"
        subprocess.run(["git", "clone", "-q", "--bare", "--no-local", str(repo.path), str(target)], check=True)
        subprocess.run(["git", "-C", str(target), "fetch", "-q", str(repo.path),
                        "+refs/squash/stripped:refs/squash/stripped"], check=True)
    new = P.plan(str(target), base, stripped, tip, str(tmp_path / name), LEDGERS, PERSONAL,
                 PERSONAL_RE, log=open(os.devnull, "w"))
    if bare:
        repo.git("fetch", "-q", str(target), new)
    return new


def test_plan_end_to_end(repo, tmp_path):
    base, tip = make_wip(repo)
    new = run_plan(repo, tmp_path, base, tip)
    assert repo.git("rev-parse", f"{new}^{{tree}}") == repo.git("rev-parse", f"{tip}^{{tree}}")
    subjects = repo.subjects(base, new)
    assert subjects[0].startswith("fix(ui):") and subjects[1].startswith("fix(git):")
    assert subjects[-2].startswith("docs(ledgers):")
    assert subjects[-1] == "personal scripts"
    assert len(subjects) == 4
    groups = repo.git("log", "--reverse", "--format=%(trailers:key=Squash-Group,valueonly)",
                      f"{base}..{new}").split()
    assert groups == ["ui", "git", "ledgers", "personal"]
    ui = repo.body(f"{new}~3")
    assert "Squashed from 2 commits:" in ui
    assert "- feat(ui): pane (" in ui and "- fix(ui): pane follow-up (" in ui
    # The ledgers commit carries the non-personal .gitignore line; personal adds the rest.
    assert repo.git("show", f"{new}~1:.gitignore") == "target/\nnode_modules/"
    assert repo.git("show", f"{new}~1:CHANGELOG.md") == "# log\n- ui\n- git"
    assert repo.git("diff", "--name-only", f"{new}~1", new).split() == [".gitignore", "build.sh"]
    assert (tmp_path / "out" / "blocks.md").read_text().startswith("# fix(ui):")


def test_plan_second_run_flattens_squashed_lists(repo, tmp_path):
    base, tip = make_wip(repo)
    first = run_plan(repo, tmp_path, base, tip)
    repo.git("reset", "-q", "--hard", first)
    repo.commit("fix(ui): late", {**repo.edit("ui", 5, "ui3"), "CHANGELOG.md": "# log\n- ui\n- git\n- late\n"})
    second = run_plan(repo, tmp_path, base, repo.git("rev-parse", "HEAD"), "out2")
    assert repo.git("rev-parse", f"{second}^{{tree}}") == repo.git("rev-parse", "HEAD^{tree}")
    assert repo.subjects(base, second)[-1] == "personal scripts"
    ui = repo.body(f"{second}~3")
    assert ui.count("Squashed from") == 1
    assert "Squashed from 3 commits:" in ui
    assert [line for line in ui.splitlines() if line.startswith("- ")][2].startswith("- fix(ui): late (")
    assert ui.count("Squash-Group:") == 1
    assert "Squashed from 1 commit" in repo.body(second)


def test_plan_leftover_personal_commit_raises(repo, tmp_path):
    base = repo.commit("base", {"ui": LINES, ".gitignore": "target/\n"})
    tip = repo.commit("personal scripts", {"build.sh": "#!/bin/sh\n", **repo.edit("ui", 1, "oops")})
    with pytest.raises(P.PlanError, match="personal/ledger commits still change other paths"):
        run_plan(repo, tmp_path, base, tip)


def test_plan_in_non_bare_repo(repo, tmp_path):
    base, tip = make_wip(repo)
    run_plan(repo, tmp_path, base, tip, bare=False)
