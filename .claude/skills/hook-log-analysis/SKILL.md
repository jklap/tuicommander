---
name: hook-log-analysis
description: >
  Analyse Claude Code hook behavior from TUICommander's debug hook log
  (`.claude/hook-debug.log`, written by `.claude/hooks/debug-log.sh`) without ever
  loading the multi-GB file: mark a byte offset, reproduce the behavior, then read a
  timeline / payloads / env differences from that offset forward. Use when asked why a
  tab's busy/idle/awaiting state is wrong, what order hooks fire in (Stop,
  SubagentStart/Stop, Notification, teammates, background tasks), what a hook payload
  actually contains, whether a hook fired at all, or how a teammate's terminal relates
  to its lead. Use BEFORE theorising about hook ordering or payload shape.
keywords:
  - hook-debug.log
  - hooks
  - Stop
  - SubagentStop
  - SubagentStart
  - background_tasks
  - teammate
  - agent state
  - tuic-hook
  - debug-log.sh
---

# Hook Log Analysis

Ground truth for "what did Claude Code actually send, and in what order" is the raw hook
payload, not the docs and not the code. `.claude/hooks/debug-log.sh` appends every hook fire
(stdin JSON plus a filtered env dump) to **one file in the main checkout**, shared by every
worktree and every session. This skill reads it safely.

## Rules that prevent the usual mistakes

- **Never `cat`, `grep`, `Read` or `tail -f` the whole log.** It was 1.6 GB after a day. Always go
  through a byte offset: `scripts/hooklog.py mark` first, act, then `--since <offset>`.
- **The path is the main checkout's**, whichever worktree you are in (`debug-log.sh` writes one
  shared log on purpose). Both `debug-log.sh` and this skill's script resolve it via
  `git rev-parse --git-common-dir`; override both with `HOOK_DEBUG_LOG=/path`.
- **Entries from all sessions interleave.** Filter with `--session <TUIC_SESSION prefix>` (the
  terminal) or `--claude-session <payload session_id prefix>` (the Claude conversation). A
  **teammate is its own Claude session in its own TUIC terminal** — two different ids on both axes.
- **Timestamps are UTC**, taken when the hook script ran (not when Claude decided to fire it).
- **Do not log raw env.** `debug-log.sh` filters `TOKEN|KEY|SECRET|PASSWORD|CREDENTIAL` names;
  keep that filter if you touch it (the lead's env contains a messaging token).
- **Never delete or truncate the log yourself** (it may hold someone else's evidence); if size
  is a problem, ask the user to rotate it.

## Recipe

```bash
S=.claude/skills/hook-log-analysis/scripts/hooklog.py
OFF=$(python3 $S mark)                       # 1. remember where "now" is
# 2. reproduce: spawn the subagent / teammate / background task, end your turn, wait...
python3 $S timeline --since $OFF             # 3. who fired what, in order
python3 $S timeline --since $OFF --event Stop,SubagentStop,UserPromptSubmit
python3 $S show --since $OFF --event Stop --last     # 4. one full payload
python3 $S envdiff --since $OFF              # 5. hook-process env across terminals
```

`timeline` prints one line per entry: `time  tuic=<terminal>  claude=<session>  <Event>  key fields`
(tool, agent type/id, notification type, `background_tasks=[type:status,...]`, message text).
`show` pretty-prints the payload (long strings truncated). `envdiff` needs env blocks from 2+
terminals (entries written after env logging was added).

To observe a *teammate* you must let it finish and then end **your own turn** too, so the lead's
`Stop` fires while the teammate state is interesting. Read the log only after that.

## Facts already established (2026-10-01, verify against a fresh capture before relying)

- **`Stop` carries `background_tasks`**: `[{id,type,status,description,...}]` with `type` in
  `shell|subagent|teammate|monitor|workflow|...`. An **idle teammate stays `running` in every
  later `Stop`** until shut down.
- **A background subagent's completion** is announced by a synthetic `UserPromptSubmit`
  (`<agent-message ...>` / `<task-notification>`); **`SubagentStop` fires AFTER the parent's
  `Stop`** and still lists the finishing subagent as `running` in its own `background_tasks`.
- **`SubagentStop` with an empty `agent_type`**, repeating every ~30 s, is Claude's own internal
  agents (prompt suggestions, `/btw`), not your subagents. Match on `agent_id` to pair with
  `SubagentStart`.
- **Teammates fire NO `SubagentStart`/`SubagentStop`.** Their own `SessionStart`/`Stop` land on
  their own terminal; nothing fires in the lead when one finishes (an idle-notification wakes
  the lead with no hook, not even `UserPromptSubmit`).
- **A teammate's hook payload and env carry no parent reference** (only its own ids; env differs
  by `TUIC_SESSION`, `CLAUDE_CODE_SESSION_ID`, `CLAUDE_PID`, socket, tty,
  `TUIC_NONINTERACTIVE_HINT=1`, `CLAUDE_CODE_SESSION_ATTENDED=0`). The lead's `PostToolUse` for the
  `Agent` call has `tool_response` with `teammate_id`, `team_name`, `tmux_pane_id`; TUIC's tmux
  topology (`GET /tmux/topology?label=...`) maps pane -> terminal.
- Docs: trust the raw page (`curl -sL https://code.claude.com/docs/en/hooks.md`), not a WebFetch
  summary, which has fabricated fields and orderings for this page.

## Setup facts

- Hooks are registered in the **gitignored** `.claude/settings.local.json`; adding/removing an
  event there needs a Claude Code session restart. Editing `debug-log.sh` does not.
- Registered events (as of writing): SessionStart, UserPromptSubmit, PreToolUse, PermissionRequest,
  PermissionDenied, PostToolUse, PostToolUseFailure, PostToolBatch, Notification, Stop, StopFailure,
  ElicitationResult, SessionEnd, SubagentStart, SubagentStop. An event not registered produces no
  entries — "it never fired" is only a valid conclusion for a registered event.
- The production hook binary (`tuic-hook`, what actually drives the app's state) is separate from
  this debug logger: this log shows what Claude sent, not what TUIC derived. For TUIC's derived
  state use `GET /sessions/{id}/explain-state` (see root `AGENTS.md`, "Agent state detection").

## Script

`scripts/hooklog.py` — `mark | timeline | show | envdiff`; streams from an offset, resynchronises
on the first entry header after a mid-entry seek, tolerates unparseable payload lines, and reads
nothing before `--since`. Python 3 stdlib only.
