#!/usr/bin/env python3
"""Assign each working-branch commit to a feature group.

Library for plan.py, plus a preview CLI:

    classify.py <repo> <base> <tip> [main-subjects-file]

prints one `hash<TAB>group<TAB>subject` row per commit, so a new mapping can be checked before
a re-org. Rows in group `misc` matched nothing below and need a mapping (or a `Squash-Group:`
trailer on the commit).

Precedence, first match wins:
  1. a `Squash-Group: <group>` trailer (re-orgs write one, and a commit may set it by hand)
  2. the subject also exists on upstream -> "upstream" (kept as its own commit)
  3. `fixup! <subject>` -> the group of the commit it fixes
  4. SUBJECT_OVERRIDES regexes
  5. the conventional-commit scope, via SCOPE (a compound "a,b" scope falls back to "a")
  6. scope-less test:/ci:/style: commits -> "test-infra"
  7. PATHS prefixes over the files the commit touches
  8. "misc"

The tables are this repo's data, not logic: edit them freely.
"""
import re
import subprocess
import sys
from collections import OrderedDict

# group -> (squashed subject, order). Order is informational; plan.py keeps original order
# and only moves a commit back into an earlier block of its own group.
GROUPS = OrderedDict([
    ("upstream",       ("(kept individually: already on upstream, verbatim or modified)", 0)),
    ("test-infra",     ("test(infra): hermetic test scratch, git fences, socket roots and the tmp/HOME guards", 1)),
    ("process-info",   ("feat(tuic-core): native process inspection; no ps/pgrep/pkill exec", 2)),
    ("terminal-input", ("fix(terminal): input, mouse, wheel, cursor and VT fidelity fixes", 3)),
    ("command-blocks", ("feat(terminal): command blocks, scrollbar marks and the Terminal settings tab", 4)),
    ("smart-select",   ("feat(terminal): configurable links, word boundaries and smart selection", 5)),
    ("inline-images",  ("feat(terminal): iTerm2/Kitty inline images, OSC 1337 and the color-tools CLI", 6)),
    ("worktree",       ("feat(worktree): safe removal, Setup/Archive scripts, TUIC_* context and warming", 7)),
    ("git",            ("fix(git): default branch, mergeOptions and branch classification for non-origin remotes", 8)),
    ("events-parity",  ("feat(events): desktop/HTTP event parity, per-viewer visibility and session lifecycle", 9)),
    ("agent-state",    ("feat(agent-state): teammate-aware state, declared background work and explain-state", 10)),
    ("hooks",          ("feat(hooks): tuic-hook binary and the agent-state fixture gate", 11)),
    ("tmux",           ("feat(tmux): full swarm support in the tuic-as-tmux shim", 12)),
    ("session-review", ("feat(session-review): Session Diff Review, diff options and live diff views", 13)),
    ("remote",         ("feat(remote): Remote Servers, SSH provisioning, Local/Direct transports and HTTPS", 14)),
    ("tunnels",        ("fix(tunnels): reap ssh process groups and close shutdown races", 15)),
    ("mcp",            ("feat(mcp): per-tool instruction gating, spawn readiness and config ownership", 16)),
    ("prompts",        ("feat(smart-prompts): import/export, variable registry and safe injection", 17)),
    ("settings",       ("feat(settings): settings reorganisation, tri-state controls and search", 18)),
    ("ui",             ("fix(ui): panes, sidebar, tabs, palette and dialog fixes", 19)),
    ("features-misc",  ("feat: notifications, StreamDock, Finder service, plugins and restore", 20)),
    ("build",          ("build: bundle budget, lazy loading, Makefile and dependency fixes", 21)),
    ("docs",           ("docs: AGENTS.md split, skills and repo guidance", 22)),
    ("misc",           ("chore: unclassified commits", 23)),
    ("ledgers",        ("docs(ledgers): wip entries for CHANGELOG, to-test, SPEC, todo, FEATURES and .gitignore", 98)),
    ("personal",       ("personal scripts", 99)),
])


def _scopes(names, group):
    return dict.fromkeys(names.split(), group)


SCOPE = {
    **_scopes("test test-support support infra guard check fixtures toolchain ci gate bridge chat_view "
              "install script-env make tests headless vt coverage replay helpers branch-restore injection",
              "test-infra"),
    **_scopes("tuic-core process process-manager agent-hooks", "process-info"),
    **_scopes("terminal clipboard crypto ids", "terminal-input"),
    **_scopes("shortcuts config", "command-blocks"),
    **_scopes("smart-selection", "smart-select"),
    **_scopes("cli", "inline-images"),
    **_scopes("worktree worktrees worktree-dialog workflows tuic-git attachments", "worktree"),
    **_scopes("git git-panel log", "git"),
    **_scopes("events naming init repositories session security visibility standby terminals transport "
              "daemon review store ipc remote-tabs pty", "events-parity"),
    **_scopes("agent-state tuic-hook explain-state diagnostics command-palette activity activity-dashboard "
              "agent", "agent-state"),
    **_scopes("hooks", "hooks"),
    **_scopes("tmux tmux-shim", "tmux"),
    **_scopes("backend session-review session-diff diff diff-tab diff-viewer diff-views confirm-dialog",
              "session-review"),
    **_scopes("remote ssh connections local direct remote-access tuic-remote ssh-tunnels http", "remote"),
    **_scopes("tunnels", "tunnels"),
    **_scopes("mcp mcp-http agent-wrap shell-integration", "mcp"),
    **_scopes("smart-prompts smart-prompt prompts compose tuic-cli", "prompts"),
    **_scopes("settings indicators", "settings"),
    **_scopes("panes pane-layout pane-tree sidebar global-workspace workspace tabbar ui close panels "
              "dialogs markdown-tab", "ui"),
    **_scopes("notifications streamdock finder plugins window resume-banner claude-usage wiz-kanban features",
              "features-misc"),
    **_scopes("build bundle deps notices lint rust clippy mobile frontend", "build"),
    **_scopes("agents AGENTS docs skill skills to-test-cycle comments gitignore changelog to-test ledgers rebase",
              "docs"),
    # compound scopes whose first part would pick the wrong group
    "tunnels,remote": "remote",
    "hooks,state-explain": "agent-state",
    "pty,state": "agent-state",
}

# Subject regex -> group, checked before the scope.
SUBJECT_OVERRIDES = [
    (r"^personal scripts$|^ignore \.obsidian$", "personal"),
    (r"^added \.claude/$", "docs"),
    (r"command blocks|block display|Blocks group|block navigation|scrollbar (block )?marks|OSC 133|osc133|"
     r"133;B|fold-at-viewport|Command Blocks|gutter|synthesizes_block", "command-blocks"),
    (r"inline[- ]image|Kitty|OSC 1337|color-tools|imgcat|pixel size|cell pixel|image payloads|"
     r"pixel-metrics", "inline-images"),
    (r"[Ss]mart [Ss]election|word boundar|link click", "smart-select"),
    (r"TUIC_\* (worktree )?context|TUIC_\* env", "worktree"),
    (r"teammate|declared[ -]?(background )?work|declaredBackgroundWork|bgtasks|awaiting|explain|"
     r"background tasks", "agent-state"),
    (r"is_remote|origin-tracking|origin \(agent vs user\)", "events-parity"),
    (r"[Ss]ession [Dd]iff|diff-comparison|diff-options|Branch Diff", "session-review"),
    (r"Remote Access tab|Remote Servers|SSH Tunnels|Tailscale|Remote Access and MCP", "remote"),
    (r"native(ly)?\b.*(ps|pgrep|process)|ps fallback|exec'ing ps|ps/pgrep|process-exec guard|logged ps",
     "process-info"),
    (r"/tmp|TMPDIR|\$HOME|HOME-relative|~/Gits|hermetic|scratch|GIT_CEILING|template directory|"
     r"merge\.ff=only|url\.insteadOf|toplevel", "test-infra"),
    (r"shell readiness|readiness gate|p10k|materialize race|alt-screen swap|pending_scroll|VT screen|"
     r"custom_pty_env|Ctrl-U|explicit close kills", "events-parity"),
    (r"split layout|Add Terminal|ghost tabs", "ui"),
    (r"bundle|lazy-load|chunk|Cargo\.lock|THIRD_PARTY|biome|cargo fmt|rustfmt", "build"),
]

# Path prefix -> group, for commits nothing above matched.
PATHS = [
    ("src-tauri/crates/tuic-streamdock", "features-misc"),
    ("src-tauri/crates/tuic-hook", "hooks"),
    ("src-tauri/src/session_review", "session-review"),
    ("scripts/", "test-infra"),
    ("src-tauri/crates/tuic-test-support", "test-infra"),
    ("docs/", "docs"),
    ("AGENTS.md", "docs"),
    (".claude/", "docs"),
]

_CC = re.compile(r"^[a-z]+(?:\(([^)]*)\))?!?: ")


def scope_of(subject):
    m = _CC.match(subject)
    return m.group(1) if m else None


def classify(subject, files, upstream, trailer_group=None):
    """Return the group for one commit, or None for a `fixup!` (see assign_groups)."""
    if trailer_group:
        return trailer_group
    if subject in upstream:
        return "upstream"
    if subject.startswith("fixup! "):
        return None
    for pat, grp in SUBJECT_OVERRIDES:
        if re.search(pat, subject):
            return grp
    scope = scope_of(subject)
    if scope:
        for candidate in (scope, scope.split(",")[0]):
            if candidate in SCOPE:
                return SCOPE[candidate]
    kind = subject.split(":")[0].split("(")[0]
    if not scope and kind in ("test", "ci", "style"):
        return "test-infra"
    for prefix, grp in PATHS:
        if any(f.startswith(prefix) for f in files):
            return grp
    return "misc"


def assign_groups(commits, upstream):
    """Set c["g"] on each dict with keys s (subject), files, and optional trailer_group."""
    for c in commits:
        c["g"] = classify(c["s"], c["files"], upstream, c.get("trailer_group"))
    by_subject = {c["s"]: c for c in commits}
    for c in commits:
        if c["g"] is None:
            target = by_subject.get(c["s"][len("fixup! "):])
            c["g"] = target["g"] if target and target["g"] else "misc"
    return commits


def read_commits(repo, base, tip):
    """Commits in base..tip, oldest first: h, s, files, trailer_group."""
    def git(*args):
        return subprocess.run(["git", "-C", repo, *args], check=True, capture_output=True,
                              text=True).stdout

    out = []
    log = git("log", "--reverse",
              "--format=%H%x1f%s%x1f%(trailers:key=Squash-Group,valueonly,separator=)%x1e",
              f"{base}..{tip}")
    for rec in log.split("\x1e"):
        rec = rec.strip("\n")
        if not rec:
            continue
        h, s, tg = rec.split("\x1f")
        files = [f for f in git("diff-tree", "--no-commit-id", "--name-only", "-r", h).split("\n") if f]
        out.append(dict(h=h, s=s, files=files, trailer_group=tg.strip() or None))
    return out


def load_subjects(path):
    if not path:
        return set()
    with open(path) as f:
        return {line.rstrip("\n") for line in f if line.strip()}


if __name__ == "__main__":
    if len(sys.argv) < 4:
        sys.exit(__doc__)
    repo, base, tip = sys.argv[1:4]
    upstream = load_subjects(sys.argv[4] if len(sys.argv) > 4 else None)
    for c in assign_groups(read_commits(repo, base, tip), upstream):
        print(f"{c['h'][:12]}\t{c['g']}\t{c['s']}")
