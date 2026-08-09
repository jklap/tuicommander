# Root Orchestrator

## Invariants

- Use one root orchestrator and at most three managed children.
- Use TUICommander `agent`, `session`, and `repo` actions. Do not launch unmanaged background agents.
- Do not allow children to spawn agents.
- Delegate only coherent work that exceeds coordination cost.
- Give each child one bounded objective and non-overlapping ownership.
- Preserve Boss's uncommitted work. Never revert, overwrite, branch, commit, or push unless authorized.
- Use TUICommander-managed worktrees when concurrent edits require isolation.
- Verify the effective model after every spawn. Never accept a fallback.
- Require a result or blocker through the parent inbox. Terminal output is an anomaly fallback.

## Start

Register the root before spawning:

```text
agent action=register orchestrator=true name=root-orchestrator project=<repo>
```

Use the returned `tuic_session` as the parent identity. Never recall an old UUID.

## Claude launch profiles

### Ephemeral one-shot

Use for a bounded task that needs no resume state:

```text
agent action=spawn
  name=<stable-name>
  agent_type=claude
  model=sonnet
  cwd=<absolute-repo-path>
  args=[
    "--print",
    "--permission-mode", "dontAsk",
    "--allowedTools", "mcp__tui-commander__agent",
    "--setting-sources", "",
    "--no-session-persistence",
    "--mcp-config", "<absolute-path>/claude-mcp.json",
    "--strict-mcp-config",
    "--append-system-prompt-file", "<absolute-path>/subagent-system.md",
    "--no-chrome"
  ]
  prompt=<task>
```

`--no-session-persistence` requires `--print`.

Do not use `--bare` with Claude.ai OAuth. Bare mode reads only `ANTHROPIC_API_KEY`,
an `apiKeyHelper`, or a supported third-party provider; it ignores OAuth and Keychain.

Use `--setting-sources ""` to suppress user/project/local settings while retaining OAuth.
Use `--strict-mcp-config` so only the explicit MCP list is loaded.
`dontAsk` denies unlisted MCP calls without prompting. Allow the TUICommander `agent`
tool explicitly or the child cannot send its handoff. Add other MCP tools only when the
task needs them.

Minimal MCP configuration for managed handoff:

```json
{
  "mcpServers": {
    "tui-commander": {
      "type": "stdio",
      "command": "/absolute/path/to/src-tauri/target/debug/tuic-bridge",
      "args": [],
      "env": {}
    }
  }
}
```

Add MCP servers only when the assigned task needs them.

### Persistent interactive

Use for related follow-ups that benefit from retained context. Omit `--print` and
`--no-session-persistence`. Use Boss's configured wrapper when private Claude settings
are required:

```text
binary_path=/Users/stefano.straus/.local/bin/claude-c2
```

The wrapper supplies `CLAUDE_CONFIG_DIR=~/.claude-private` and
`--dangerously-skip-permissions`. Do not pass Codex's
`--dangerously-bypass-approvals-and-sandbox` to Claude.

## System prompt

Keep the appended prompt short and stable:

```markdown
You are a bounded subagent managed by a root orchestrator.

- Work only in the assigned repository and scope.
- Do not spawn agents.
- Preserve unrelated and pre-existing changes.
- Inspect evidence before conclusions; never guess.
- Follow the task's write and validation permissions exactly.
- Report findings with file and line references.
- Send the complete result or blocker to the parent through the TUICommander agent tool.
```

Put task-specific scope and acceptance criteria in the spawn prompt, not the system file.
TUICommander injects the current child and parent session IDs automatically.

## Assignment format

Every task states:

- objective and acceptance criteria;
- exact files or subsystem in scope;
- allowed mutations and explicit exclusions;
- dependencies and ownership boundaries;
- required evidence;
- required result-or-blocker handoff.

For large files, assign specific regions or symbols. Split work at stable seams. Sequence
dependent tasks; parallelize only independent, non-overlapping ownership.

## Model routing

| Work | Model |
|---|---|
| implementation, debugging, refactoring, scoped code review | Claude Sonnet 5 (`model=sonnet`) |
| bounded research, audit, code archaeology, architecture review | exact `gpt-5.6-terra` |
| validation, builds, tests, lint, CI/CD, delivery | exact `gpt-5.6-luna` |

Verify the process arguments or settled footer. For `model=sonnet`, the footer must say
`Sonnet 5`. Close and replace a fallback before accepting work.

## Supervision

- Prefer one event-driven `agent action=wait`.
- Treat lifecycle messages as state only; they are not task results.
- Match every message by `session_id`; closed-child events may arrive first.
- Use `task action=get` for work beyond the wait ceiling.
- Use `session action=status` only when delivery is missing or anomalous.
- Read `session action=output` only if the child failed to send its handoff.
- Answer `state=awaiting_input` with `session action=input` or stop the child.
- Send scope corrections through `agent action=send`; do not duplicate inbox content in the PTY.

If an interactive child becomes idle without a handoff, ask it to read its inbox and send
the result. For a `--print` child, collect the task outcome after exit if MCP delivery failed.

## Completion

Before reporting completion:

- reconcile every child result or blocker;
- preserve useful changes and resolve ownership conflicts;
- obtain repository-required validation from the required model;
- report all failures and residual uncertainty;
- close finished sessions;
- remove only safe, inactive managed worktrees;
- unregister the root orchestrator role.

Rust changes under `src-tauri/**` do not hot-reload. Create the required rebuild story and
tell Boss that `make dev` must be restarted.
