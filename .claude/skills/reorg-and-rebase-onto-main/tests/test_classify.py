"""classify.py: precedence, compound scopes, fixup resolution and scope_of."""
import pytest

import classify as C


def cls(subject, files=(), upstream=(), trailer=None):
    return C.classify(subject, list(files), set(upstream), trailer)


def test_trailer_beats_everything():
    assert cls("personal scripts", upstream={"personal scripts"}, trailer="ui") == "ui"
    assert cls("fixup! feat(ui): x", trailer="git") == "git"


def test_upstream_subject_beats_fixup_override_and_scope():
    for s in ("fixup! feat(ui): x", "personal scripts", "feat(ui): x"):
        assert cls(s, upstream={s}) == "upstream"


def test_fixup_returns_none():
    assert cls("fixup! feat(ui): x") is None


def test_subject_override_beats_scope():
    assert cls("feat(git): command blocks in the log") == "command-blocks"
    assert cls("personal scripts") == "personal"
    assert cls("ignore .obsidian") == "personal"
    assert cls("fix(ui): honour TMPDIR in tests") == "test-infra"


@pytest.mark.parametrize("subject,group", [
    ("feat(ui): x", "ui"),
    ("fix(tunnels): x", "tunnels"),
    ("fix(tunnels,remote): x", "remote"),        # explicit compound entry
    ("fix(worktree,prompt): x", "worktree"),     # first-part fallback
    ("fix(hooks,state-explain): x", "agent-state"),
    ("feat(ui)!: breaking", "ui"),
])
def test_scope(subject, group):
    assert cls(subject) == group


def test_scope_beats_paths():
    assert cls("feat(ui): x", files=["docs/a.md"]) == "ui"


@pytest.mark.parametrize("kind", ["test", "ci", "style"])
def test_scopeless_test_ci_style_is_test_infra(kind):
    assert cls(f"{kind}: x", files=["docs/a.md"]) == "test-infra"


def test_scoped_test_uses_scope_or_falls_through():
    assert cls("test(ui): x") == "ui"
    assert cls("test(nosuchscope): x") == "misc"
    assert cls("test(nosuchscope): x", files=["docs/a.md"]) == "docs"


def test_paths_first_prefix_in_table_order_wins():
    assert cls("feat: x", files=["docs/a.md", "scripts/b.sh"]) == "test-infra"
    assert cls("feat(zzz): x", files=[".claude/x.md"]) == "docs"
    assert cls("feat: x", files=["src-tauri/crates/tuic-hook/src/main.rs"]) == "hooks"


def test_misc_fallback():
    assert cls("feat: x", files=["src/a.ts"]) == "misc"
    assert cls("Update stuff") == "misc"


@pytest.mark.parametrize("subject,scope", [
    ("feat(ui): x", "ui"),
    ("feat(tunnels,remote): x", "tunnels,remote"),
    ("feat(ui)!: x", "ui"),
    ("feat: x", None),
    ("Feat(ui): x", None),
    ("feat(ui) x", None),
    ("Merge branch 'x'", None),
])
def test_scope_of(subject, scope):
    assert C.scope_of(subject) == scope


def commit(s, files=(), tg=None):
    return dict(s=s, files=list(files), trailer_group=tg)


def test_assign_groups_resolves_fixup_to_target_group():
    cs = C.assign_groups([commit("fixup! feat(git): y"), commit("feat(git): y"), commit("feat(ui): x"),
                          commit("fixup! feat(ui): x")], set())
    assert [c["g"] for c in cs] == ["git", "git", "ui", "ui"]


def test_assign_groups_orphan_fixup_is_misc():
    assert C.assign_groups([commit("fixup! feat(ui): gone")], set())[0]["g"] == "misc"


def test_assign_groups_fixup_of_upstream_target():
    cs = C.assign_groups([commit("feat(ui): x"), commit("fixup! feat(ui): x")], {"feat(ui): x"})
    assert [c["g"] for c in cs] == ["upstream", "upstream"]


def test_assign_groups_trailer():
    assert C.assign_groups([commit("feat(ui): x", tg="git")], set())[0]["g"] == "git"


def test_load_subjects(tmp_path):
    assert C.load_subjects(None) == set()
    p = tmp_path / "s.txt"
    p.write_text("a\n\nb c\n")
    assert C.load_subjects(str(p)) == {"a", "b c"}
