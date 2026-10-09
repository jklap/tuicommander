#!/usr/bin/env python3
"""Read TUICommander's Claude Code hook debug log without ever loading it whole.

`.claude/hooks/debug-log.sh` appends one entry per hook fire to
`.claude/hook-debug.log` in the MAIN checkout (resolved from the git common dir,
or `$HOOK_DEBUG_LOG`, so every worktree writes to the same file). The log grows without bound (1.6 GB observed),
so every command here seeks to a byte offset and streams forward.

Entry format::

    ===== <utc-ts> | <HookEvent> | <project dir> | tuic_session=<id> =====
    {single-line JSON payload Claude Code sent on stdin}
    --- env ---                      (newer entries only)
    KEY=value ...                    (CLAUDE_* / TUIC_* / TMUX*, secrets filtered)

Typical use::

    hooklog.py mark                      # -> prints the current size; remember it
    ... reproduce the behavior ...
    hooklog.py timeline --since 1594574497
    hooklog.py show --since 1594574497 --event Stop --last
    hooklog.py envdiff --since 1594574497
"""

import argparse
import json
import os
import re
import subprocess
import sys
from typing import Iterator, Optional

HEADER_RE = re.compile(
    r"^===== (?P<ts>\S+) \| (?P<event>\S+) \| (?P<proj>.*?) \| tuic_session=(?P<tuic>\S+) =====$"
)
ENV_MARKER = "--- env ---"
MAX_STR = 160


def default_log_path() -> str:
    """Resolves the log path.

    Returns:
        `$HOOK_DEBUG_LOG` if set, else `<main checkout>/.claude/hook-debug.log`
        derived from the git common dir, so it also works from a worktree.
    """
    env = os.environ.get("HOOK_DEBUG_LOG")
    if env:
        return env
    try:
        common = subprocess.check_output(
            ["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
            text=True,
            stderr=subprocess.DEVNULL,
        ).strip()
        return os.path.join(os.path.dirname(common), ".claude", "hook-debug.log")
    except (subprocess.CalledProcessError, FileNotFoundError):
        return os.path.join(".claude", "hook-debug.log")


def iter_entries(path: str, since: int) -> Iterator[dict]:
    """Streams entries starting at byte offset `since`.

    A partial entry at the seek point is skipped (we resynchronise on the first
    header line). Each yielded dict has `ts`, `event`, `proj`, `tuic`, `payload`
    (parsed JSON, or `None` if it did not parse) and `env` (dict).

    Args:
        path: Log file path.
        since: Byte offset to start reading from.

    Yields:
        One dict per complete-looking entry.
    """
    current: Optional[dict] = None
    in_env = False
    with open(path, "rb") as fh:
        fh.seek(since)
        for raw in fh:
            line = raw.decode("utf-8", errors="replace").rstrip("\n")
            match = HEADER_RE.match(line)
            if match:
                if current is not None:
                    yield current
                current = {**match.groupdict(), "payload": None, "env": {}, "_raw": None}
                in_env = False
                continue
            if current is None:
                continue
            if line == ENV_MARKER:
                in_env = True
            elif in_env:
                if "=" in line:
                    key, _, value = line.partition("=")
                    current["env"][key] = value
            elif current["_raw"] is None and line.strip():
                current["_raw"] = line
                try:
                    current["payload"] = json.loads(line)
                except json.JSONDecodeError:
                    current["payload"] = None
    if current is not None:
        yield current


def shorten(value: object, limit: int = MAX_STR) -> object:
    """Truncates long strings (recursively) for display."""
    if isinstance(value, str):
        return value if len(value) <= limit else value[:limit] + f"...[+{len(value) - limit}]"
    if isinstance(value, list):
        return [shorten(v, limit) for v in value]
    if isinstance(value, dict):
        return {k: shorten(v, limit) for k, v in value.items()}
    return value


def summarize(payload: Optional[dict]) -> str:
    """Builds a one-line summary of the fields that usually matter."""
    if not payload:
        return "(unparseable payload)"
    parts = []
    for key in ("tool_name", "agent_type", "agent_id", "notification_type", "source", "reason"):
        if payload.get(key):
            parts.append(f"{key}={payload[key]}")
    tasks = payload.get("background_tasks")
    if isinstance(tasks, list):
        compact = ",".join(f"{t.get('type', '?')}:{t.get('status', '?')}" for t in tasks)
        parts.append(f"background_tasks=[{compact}]")
    for key in ("last_assistant_message", "prompt", "message"):
        if payload.get(key):
            parts.append(f"{key}={str(payload[key])[:70]!r}")
            break
    return " ".join(parts)


def select(entries: Iterator[dict], args: argparse.Namespace) -> Iterator[dict]:
    """Applies the --session / --claude-session / --event filters."""
    events = {e for e in (args.event or "").split(",") if e}
    for entry in entries:
        if args.session and not entry["tuic"].startswith(args.session):
            continue
        sid = (entry["payload"] or {}).get("session_id", "")
        if args.claude_session and not sid.startswith(args.claude_session):
            continue
        if events and entry["event"] not in events:
            continue
        yield entry


def cmd_mark(args: argparse.Namespace) -> int:
    """Prints the log's current size: the offset to pass to --since later."""
    print(os.path.getsize(args.log))
    return 0


def cmd_timeline(args: argparse.Namespace) -> int:
    """One line per entry: time, TUIC terminal, Claude session, event, key fields."""
    for entry in select(iter_entries(args.log, args.since), args):
        sid = (entry["payload"] or {}).get("session_id", "")
        print(
            f"{entry['ts']}  tuic={entry['tuic'][:8]}  claude={sid[:8]:<8}  "
            f"{entry['event']:<18} {summarize(entry['payload'])}"
        )
    return 0


def cmd_show(args: argparse.Namespace) -> int:
    """Pretty-prints payloads (long strings truncated) of matching entries."""
    matches = list(select(iter_entries(args.log, args.since), args))
    if args.last:
        matches = matches[-1:]
    for entry in matches:
        print(f"== {entry['ts']} {entry['event']} tuic={entry['tuic']}")
        print(json.dumps(shorten(entry["payload"]), indent=2))
    if not matches:
        print("no matching entries", file=sys.stderr)
        return 1
    return 0


def cmd_envdiff(args: argparse.Namespace) -> int:
    """Compares hook-process env across TUIC terminals: only differing keys."""
    envs: dict = {}
    for entry in select(iter_entries(args.log, args.since), args):
        if entry["env"]:
            envs.setdefault(entry["tuic"], entry["env"])
    if len(envs) < 2:
        print(f"need env blocks from 2+ terminals, found {len(envs)}", file=sys.stderr)
        return 1
    ids = list(envs)
    print("terminals: " + "  ".join(i[:8] for i in ids))
    for key in sorted({k for env in envs.values() for k in env}):
        values = [envs[i].get(key, "<unset>") for i in ids]
        if len(set(values)) > 1:
            print(f"{key}: " + " | ".join(v[:50] for v in values))
    return 0


def build_parser() -> argparse.ArgumentParser:
    """Builds the CLI parser."""
    parser = argparse.ArgumentParser(description="Analyse .claude/hook-debug.log safely.")
    parser.add_argument("--log", default=default_log_path(), help="log path")
    sub = parser.add_subparsers(dest="cmd", required=True)
    mark = sub.add_parser("mark", help="print the current size (an offset for --since)")
    mark.set_defaults(func=cmd_mark)
    for name, func, helptext in (
        ("timeline", cmd_timeline, "one line per entry"),
        ("show", cmd_show, "pretty-print payloads"),
        ("envdiff", cmd_envdiff, "env keys that differ between terminals"),
    ):
        p = sub.add_parser(name, help=helptext)
        p.add_argument("--since", type=int, required=True, help="byte offset from `mark`")
        p.add_argument("--session", help="TUIC_SESSION prefix filter")
        p.add_argument("--claude-session", help="payload session_id prefix filter")
        p.add_argument("--event", help="comma-separated hook events, e.g. Stop,SubagentStop")
        if name == "show":
            p.add_argument("--last", action="store_true", help="only the last match")
        p.set_defaults(func=func)
    return parser


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    if not os.path.exists(args.log):
        print(f"log not found: {args.log}", file=sys.stderr)
        return 2
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
