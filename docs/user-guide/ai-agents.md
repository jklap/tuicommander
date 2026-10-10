# AI Agents

TUICommander detects, monitors, and manages AI coding agents running in your terminals.

## Supported Agents

| Agent | Binary | Resume Command | Session Binding |
|-------|--------|----------------|-----------------|
| Claude Code | `claude` | `claude --continue` | `claude --resume $TUIC_SESSION` |
| Codex CLI | `codex` | `codex resume --last` | `codex resume $TUIC_SESSION` |
| Aider | `aider` | `aider --restore-chat-history` | — |
| Gemini CLI | `gemini` | `gemini --resume` | `gemini --resume $TUIC_SESSION` |
| OpenCode | `opencode` | `opencode -c` | — |
| Amp | `amp` | `amp threads continue` | — |
| Cursor Agent | `cursor-agent` | `cursor-agent resume` | — |
| Droid (Factory) | `droid` | — | — |
| Goose | `goose` | `goose session --resume` | `goose session --resume --name $TUIC_SESSION` |
| Grok | `grok` | `grok --continue` | `grok --resume <discovered id>` |
| pi | `pi` | `pi --continue` | — |

### Native scrollback on launch

**Prevent alternate screen** in Settings → AI → Agents is enabled by default for every agent. TUIC applies Claude Code's `CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN=1` and `CLAUDE_CODE_DISABLE_AGENT_VIEW=1` environment controls when enabled. A terminal that enters the alternate screen within its first seconds raises a warning toast naming the session and the fix. Codex and Grok receive `--no-alt-screen` when their installed CLI advertises it in `--help`; OpenCode receives `--mini` when available. On Windows, the probe selects an executable `.exe` or `.cmd` launcher ahead of an extensionless npm shell shim. TUICommander checks the executable with a two-second deadline and shares concurrent checks. A timed-out check is not retried until that binary changes; a quick inconclusive exit is retried after a short cooldown. Older versions still start without an unsupported option. Existing flags are not duplicated. Rust applies the same policy to structured IPC, HTTP and MCP launches and exports it to new TUIC shells, where zsh, bash and fish wrappers apply it to commands typed manually. `command <agent>` bypasses those wrappers. Reopen an existing shell after changing the setting.

Gemini currently defaults to the primary screen (`ui.useAlternateBuffer: false`), Cursor enables fullscreen only when requested, and pi defaults to regular TUI mode. Other agents without a documented alternate-screen control keep their own defaults. If an agent enters the alternate screen despite these defaults, TUICommander records one warning for that session with the agent name and detected version.

To allow alternate screen, turn off **Prevent alternate screen** for that agent. The preference applies to new TUIC agent launches and new shells. An explicit `CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN` value in a Claude command's environment retains precedence.

### Workspace trust for managed spawns

**Accept workspace trust for managed spawns** is on by default for Claude Code and Codex in Settings → AI → Agents. When an agent starts another agent through TUICommander's `agent spawn` action in a new folder, the child starts without waiting for a workspace trust answer. Codex receives a trust setting for that one launch and folder, including when a custom launcher forwards its arguments to Codex. TUICommander answers Claude Code's initial trust picker only when it shows the expected question with **No, exit** selected. Neither path edits the CLI's saved trust configuration.

Turn this setting off for an agent if you want to answer its normal trust question during managed spawns. Terminals that you open yourself always use the agent's normal trust behavior.

## Agent Detection

A supported agent started by hand in a terminal is detected from its foreground process on macOS, Linux and Windows. The backend records that identity in both desktop and headless builds, without requiring an open UI. HTTP and desktop foreground queries use the same detection logic. `TUIC_SESSION` identifies the terminal; it does not turn a plain shell into an agent. An agent stops being eligible when its foreground returns to a shell. Configured launch presets remain available during startup until a non-shell foreground is first observed, including a custom wrapper; startup helpers can intentionally disarm the preset early to prevent unattended shell input; after that they are also revoked on return to a shell. Transient tool subprocesses preserve agent identity. Submission and mail wake still require a ready composer with no partial user input or approval prompt.

TUICommander auto-detects which agent is running in each terminal by matching output patterns. Detection uses agent-specific status line markers:

- **Claude Code**: Middle dot `·` (U+00B7), dingbat asterisks `✢` `✳` `✶` `✻` `✽` (U+2720–273F), or ASCII `*`
- **Copilot CLI**: Therefore sign `∴` (U+2234), filled circle `●` (U+25CF), empty circle `○` (U+25CB)
- **Aider**: Knight Rider scanner blocks `░█`
- **Gemini CLI / Amazon Q / Cline**: Braille spinners `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`
- **Codex CLI**: Bullets `•` `◦`

When detected:

- The **status bar** shows the agent's brand logo and name
- The **tab indicator** updates to reflect agent state
- Rate limit and question detection activate for that provider's patterns

Binary detection uses `resolve_cli()` — Rust probes well-known directories so agents are found even in release builds where the user's shell PATH isn't available.

## Rate Limit Detection

When an agent hits a rate limit, TUICommander detects it from terminal output:

- **Status bar warning** — Shows a badge with the number of rate-limited sessions and a countdown timer
- **Per-session tracking** — Each session's rate limit is tracked independently with automatic cleanup when expired
- **Provider-specific patterns** — Custom regex for Claude ("overloaded", "rate limit"), Gemini ("429", "quota exceeded"), OpenAI ("too many requests"), and generic patterns

## Question Detection

When an agent asks an interactive question (Y/N, multiple choice, numbered options), TUICommander:

1. Changes the **tab indicator** to a `?` icon
2. Shows a **prompt overlay** with keyboard navigation:
   - `↑/↓` to navigate options
   - `Enter` to select
   - Number keys `1-9` for numbered options
   - `Escape` to dismiss
3. Plays a **notification sound** (if enabled in Settings → Notifications)

For unrecognized agents, silence-based detection kicks in — if the terminal stops producing output for 10 seconds after a line ending with `?`, it's treated as a potential prompt. User-typed lines ending with `?` are suppressed from question detection for 500ms (echo window) to avoid false positives from PTY echo.

## Native Hook Instrumentation

Claude and Codex status signals are enabled by default and scoped to each TUIC launch. Claude receives an additional `--settings <config-dir>/agent-hooks/claude.json`; Codex receives `-c notify=["<config-dir>/agent-hooks/codex-notify.sh"]`. Existing explicit overrides win. The Codex adapter emits idle on `agent-turn-complete` (to the PTY TUIC stamped in `$TUIC_PTY_TTY`, so it works inside a sandboxed agent that cannot run `ps`) and then chains the user's configured `notify` command with the original JSON payload. No global agent configuration is written by this default path.

After a completion hook, decorative terminal animation and redraws keep the
session idle. New input and recognized active work can start another turn.

An open interactive selection dialog restores the question badge even when
hooks are enabled and another hook cleared it before you answered.

Disable this per agent with **Settings → Agents → Native status signals** to restore screen-only heuristics. Gemini, Grok, and OpenCode retain a separate, explicit **Install hooks globally** toggle.

**zsh only:** if your shell already defines its own `claude`/`codex`/`goose` function (a common pattern for picking a model or profile), TUICommander leaves it alone, so the launch-flag injection above does not happen for that shell. When this is detected, TUIC asks whether to wrap your function (calling it with the flag added) or leave it alone. **Nothing is wrapped without that explicit yes**, and the answer applies to that exact function: if you edit it, TUIC asks again. "Not now" asks again the next time the app starts. The choice can be revisited in **Settings → Agents → *(agent)* → "If your shell already defines its own \<agent\> function"** (an expert setting: Ask when detected / Wrap my function / Leave my function alone; choosing "Wrap" there still waits for the prompt's yes for your actual function). Wrapping only works if your function passes its arguments through (`"$@"`); if your function already sets its own version of the same flag, the CLI keeps only one of them — the prompt calls this out before you opt in. Applies to new terminals only. TUIC's own zsh wrappers are defined after your `.zshrc` runs, so a `command -v claude` check in your startup files sees the real binary; your own `grok`/`opencode` functions are always left alone.

Instead of inferring busy/idle/waiting from terminal output, TUICommander can drive an agent's status directly from the agent's **own hook system**.

When enabled, TUIC writes a small guarded shell command into the agent's settings file for each lifecycle event; the command invokes the bundled `tuic-hook` binary, which emits `OSC 7770;state=…` (`prompt` when you submit a prompt, busy on tool start, `awaiting` on an approval/question prompt, idle on stop) — plus, for Claude Code, free-text metadata (session id, working directory, transcript path, tool name, notification message) extracted natively from the hook's own JSON payload. The session state then follows the hooks precisely, and the heuristic question-detection above is suppressed for that agent. Screen and ordinary silence cannot override a protocol-held turn. If its completion signal is lost, a stable Ready screen plus five minutes without PTY output recovers the session and writes a `protocol-stale` warning.

For Claude, the status stays "Working" while a backgrounded tool call (e.g. a
`run_in_background` Bash command) is still outstanding, even after Claude's own
turn ends and its composer is ready — Claude's `Stop` hook payload names any such
outstanding task, so this doesn't rely on inference. This is a separate, more
authoritative signal from the general "background process detected" heuristic
(which deliberately shows "Idle" once the composer is ready, since Codex agents
in particular may intentionally leave a long-lived dev server running).

For Claude, `awaiting` also covers **MCP elicitation** — the dialog an MCP server raises to ask you for input (`MCP server "…" requests your input`, with Accept/Decline). It arrives through Claude's `Elicitation` event and is retracted by `ElicitationResult`; no screen scraping is involved, because that dialog matches none of the question heuristics.

**Ownership is safe and reversible.** Each managed hook carries a `# tuic-managed-hook` sentinel; enabling installs only TUIC's entries and disabling removes only them — your own (and wiz/mdkb) hooks in the same file are never touched. The toggle is the source of truth; the effect applies on the agent's **next launch** (hooks are read at startup).

| Agent | Hooks | Status |
|-------|-------|--------|
| Claude | `~/.claude/settings.json` | Supported |
| Gemini | `~/.gemini/settings.json` | Supported |
| Codex | `~/.codex/hooks.json` + `~/.codex/config.toml` (`[features] hooks = true`) | Supported |
| Grok | `~/.grok/hooks/tuic.json` (own file) | Supported |
| OpenCode | `~/.config/opencode/plugin/tuic.ts` (Bun/TS plugin) | Supported |
| Others (Aider, Amp, Cursor, Goose, Droid, pi) | — | No TUIC-managed hook system — stays heuristic |

> **Platform note:** Hook instrumentation is currently offered on **macOS and Linux**. `tuic-hook` resolves the controlling tty natively rather than shelling out to `ps`, which removes the previous architectural blocker on Windows, but that path isn't validated there yet — on Windows the toggle stays hidden and agents keep heuristic detection (no regression).

## Usage Limit Tracking

For Claude Code, TUICommander detects weekly and session usage limit messages from terminal output:

- **Unified agent badge** — When Claude is the active agent, the status bar shows a single badge combining the agent icon with usage data. The badge displays rate limit countdowns (when rate-limited), Claude Usage API data (5h/7d utilization percentages), or terminal-detected usage limits, in that priority order.
  - Blue: < 70% utilization
  - Yellow: 70–89%
  - Red (pulsing): >= 90%
- Clicking the badge opens the Claude Usage Dashboard.

The API quota follows the focused Claude session's `CLAUDE_CONFIG_DIR`. Switching between Claude sessions using different accounts updates the badge. If the session's profile or its credentials cannot be read, the badge shows unknown rather than the default account's quota.

This helps you pace your usage across the week.

## Claude Usage Dashboard

A native feature (not a plugin) that provides detailed analytics for your Claude Code usage. Enable it in **Settings** > **Agents** > expand **Claude Code** > **Features** > **Usage Dashboard**.

When enabled, TUICommander polls the Claude API every 5 minutes and shows:

- **Rate limits** — 5-hour and 7-day utilization bars with reset countdowns. Color-coded: green (OK), yellow (70%+), red (90%+). Enterprise/spend-based plans don't populate these named buckets — the badge and dashboard fall back to that plan's own usage figure instead of showing "no data."
- **Usage Over Time** — 7-day token usage chart (input vs. output tokens) with hover tooltips.
- **Insights** — Session count, message counts, input/output/cache token totals.
- **Activity heatmap** — 52-week GitHub-style heatmap of daily message counts with per-project drill-down on hover.
- **Model usage** — Breakdown by model (messages, input, output, cache created, cache read).
- **Per-project breakdown** — All projects ranked by token usage. Click a project to filter the dashboard to that project.

The dashboard opens as a tab in the Activity Center. You can also reach it by clicking the Claude usage badge in the status bar.
When opened from the badge, its rate-limit data uses that session's credential profile. The transcript-based charts and project statistics currently scan the default Claude projects directory.

## Agent Teams

Agent Teams lets Claude Code spawn teammate agents as TUIC terminal tabs. No configuration needed — it's enabled by default for all Claude Code sessions launched from TUICommander (see [Agent Teams](agent-teams.md) for the full picture, including how spawned-PTY teammates differ from in-process teammates).

PTY sessions receive the `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` environment variable, which unlocks Claude Code's `TeamCreate`, `TaskCreate`, and `SendMessage` tools. Agent spawning uses direct MCP tool calls (`agent spawn`) — the earlier it2 shim approach (iTerm2 CLI emulation) is deprecated.

Spawned sessions automatically emit lifecycle events (`session-created`, `session-closed`) so they appear as tabs and clean up on exit.

By default, TUICommander steers spawned Claude Code agents toward TUIC's own `agent` tool for both spawning (`agent action=spawn`, instead of Claude Code's native subagent/Task tool) and messaging (register/send/inbox/wait, instead of Claude Code's native cross-agent messaging) — via both the MCP `initialize` instructions' prose and the `agent` tool's own MCP schema description (read via `tools/list` independently of that prose). These are two **independent** settings — **Settings** > **Agents** > **Claude Code** > **Prefer TUICommander agent spawning** and **Prefer TUICommander messaging** — so you can turn either off without affecting the other, in any combination: TUIC for both, TUIC spawning with native messaging, native spawning with TUIC messaging, or native for both while leaving the `agent` MCP tool itself enabled. Turning a preference off softens both surfaces — the connect-time instructions and the tool's own description stop recommending that half — but it doesn't remove the corresponding actions from the schema or disable the underlying tool. Both preferences stop mattering (and the UI greys them out) if the `agent` MCP tool itself is disabled.

## TUIC Protocol — Output Markers

TUICommander asks the top-level agent in each session to emit three wire markers so the UI can
reflect what the agent is doing: an `ack` (which version connected), `intent:` (current work,
shown as the tab title), and `suggest:` (follow-up actions, shown as a chip bar). These arrive
over MCP in the `initialize` response's `instructions` field — the same channel that carries
this app's tool descriptions — with their own provenance and scope statement, not as a bare
directive. See [`docs/backend/output-parser.md`](../backend/output-parser.md#intent) for the
exact grammar and a copy-paste stanza for setups that don't go through MCP.

**Scope:** markers belong to the top-level session only. Delegated subagents (Claude Code's Task
tool) are explicitly told not to emit them — `suggest:` is the end-of-task marker, so a subagent
emitting it would flip the *parent* session to `completed` mid-work. In-process teammates share
the lead's MCP connection and must stay quiet on markers for the same reason; spawned-PTY
teammates get their own `initialize` call (and therefore their own markers) — see
[Agent Teams](agent-teams.md).

**Toggles:**
- Global: **Settings** > **Agents** > "Show agent intent as tab title" and "Show suggested
  follow-up actions" — these gate `intent:`/`suggest:` for every agent.
- Per-agent override: **Settings** > **Agents** > expand an agent ("Track agent intent", "Show
  suggested follow-ups", expert settings) — overrides the global toggle for just that agent. A marker shows only when **both** the global toggle and the per-agent
  override (if set) allow it; leaving the per-agent override unset just follows the global
  toggle.
- The `ack` marker has no toggle — it's the one-line "which version connected" courtesy message.

Check `GET /diagnostics/markers` to see, per session, whether markers are enabled and how many
have actually been observed — useful for confirming a session is emitting rather than refusing.

## TUICommander Agents vs Codex Internal Subagents

This comparison primarily concerns Codex internal subagents. For substantial or
long-running work, prefer agents spawned through TUICommander. Codex coordinates an
internal subagent inside the parent conversation and runtime. A TUICommander agent
runs in its own managed process and session, with a visible terminal and a stable
address that the parent or user can use later.

| Concern | Codex internal subagent | TUICommander agent |
|---------|-------------------------|---------------------|
| **Control** | Controlled through the parent conversation | Visible session with explicit status, input, interrupt, close, and kill controls |
| **Parent interruption** | Its lifecycle remains coupled to the parent runtime | Continues independently after spawn; its terminal and task handle remain available |
| **Context** | May inherit parent conversation history | Receives only the assigned brief and files it reads |
| **Worker output** | Final output normally returns directly to the parent context | Terminal output stays outside the parent context until explicitly requested |
| **Handoff** | Returned through Codex's internal collaboration mechanism | Worker sends a concise result or blocker through the TUICommander inbox |
| **Recovery** | Depends on Codex's internal subagent lifecycle | Parent can reconnect, inspect task state, read the inbox, or resume supported agent sessions |
| **User visibility** | Usually summarized by the parent | Full terminal and lifecycle state remain observable and controllable |

Each worker performs its own model inference and consumes tokens in its own context.
TUICommander does not reduce total model-token usage: MCP calls, briefs, messages,
and handoffs also consume coordination tokens. The gain is operational efficiency.
The parent can assign a focused context, let the worker continue when the parent turn
or window stops, supervise it directly, and retrieve only the result needed for
coordination. Unread terminal output does not enter the parent context, but this does
not make the worker's inference cheaper.

The terminal display itself has no model-token cost; it is rendered locally. A
provider-wide quota or outage can still affect every agent using that provider.
Recovery after a process or application restart depends on the selected agent's
documented session-resume support.

Codex internal subagents remain useful for short, tightly coupled work whose result
is needed immediately in the current turn. TUICommander agents are more operationally
efficient for parallel work that benefits from independent context, persistent
supervision, direct human control, and selective result collection. Claude Code's
native Agent Teams use a separate lifecycle described in [Agent Teams](agent-teams.md).

## Session Binding (TUIC_SESSION)

Every terminal tab has a stable UUID that persists across app restarts. This UUID is injected into the PTY shell as the `TUIC_SESSION` environment variable.

### How It Works

1. When a terminal tab is created, a UUID is generated via `crypto.randomUUID()`
2. The UUID is saved with the tab and restored when the app restarts
3. On PTY creation, the UUID is injected as `TUIC_SESSION=<uuid>` in the shell environment
4. Agents can use `$TUIC_SESSION` for session-specific operations

### Use Cases

**Automatic session binding (Claude Code):**

Shell integration automatically injects `--session-id $TUIC_SESSION` into every `claude` invocation via a shell function wrapper. You don't need to pass it manually — just type `claude` and the session is bound to this tab. The wrapper is bypassed when you explicitly pass `--session-id`, `--resume`, or `--continue`.

```bash
# These are equivalent — the wrapper handles it transparently:
claude                              # wrapper adds --session-id $TUIC_SESSION
claude --session-id $TUIC_SESSION   # explicit, wrapper bypassed
```

Claude Code stores the session locally. When you restart TUICommander and switch to this branch, the session resumes automatically via `claude --resume <uuid>`.

**Automatic session binding (Goose):**

Shell integration injects `--name $TUIC_SESSION` into `goose session` and `goose run` subcommands. The wrapper is bypassed when you explicitly pass `--name`, `-n`, `--resume`, or `-r`.

```bash
# These are equivalent:
goose session "fix the bug"                               # wrapper adds --name $TUIC_SESSION
goose session --name $TUIC_SESSION "fix the bug"          # explicit, wrapper bypassed
```

**Gemini CLI session binding (manual):**

```bash
gemini --resume $TUIC_SESSION
```

**Custom scripts that persist state per-tab:**

```bash
# Use TUIC_SESSION as a stable key for any tab-specific state
echo "Last run: $(date)" > "/tmp/tuic-$TUIC_SESSION.log"
```

### Automatic Resume

When TUICommander restores saved terminals after a restart, only tabs that had an active agent session (`agentType` set) are restored. Plain shell tabs are discarded and a fresh terminal is spawned instead. For agent tabs, TUICommander checks whether the session file exists on disk before deciding the resume strategy:

1. **Verified session** — If the terminal's saved agent session ID maps to an existing session file (e.g. `~/.claude/projects/…/<uuid>.jsonl`), the agent resumes with that agent's ID-specific command. On Windows, Claude's project slug replaces the drive colon with a dash: `C:\Users\foo\bar` becomes `C--Users-foo-bar`.
2. **No saved session ID** — Falls back to the agent's default resume behavior (e.g. `claude --continue` for the last session)
3. **Saved ID no longer verifies** — Refuses automatic resume instead of opening an unrelated last session

The resume command honours the agent's **default run config**: TUICommander swaps the binary in the resume command (`claude`) for the run config's `command` (e.g. `c2`) and appends the run config's args after the resume flag. So a user with the default run config `c2 --model claude-opus-4-6` will resume with `c2 --resume <uuid> --model claude-opus-4-6`, not `claude --resume <uuid>`.

### Resume Banner on Exit

For a hook-instrumented Claude Code session, exiting (`/exit`, Ctrl-D) shows a resume banner in that same pane, labeled with the session's title (e.g. `Resume "file-locations" — click to resume`) — the title comes from Claude Code's own session name, whether auto-generated or set via `/rename`. Clicking it resumes the exact session that just exited; the × dismisses it without resuming.

This banner is **click-only and stays until dismissed** — unlike the restore-time banner (above), which accepts Space/Enter and dismisses on any other keystroke, typing at the shell prompt after an exit passes straight through, and the banner remains until you click it, click ×, or start another agent session in that pane.

### UI Agent Spawn

When you spawn an agent via the context menu or command palette, TUICommander automatically uses the tab's `TUIC_SESSION` as the `--session-id`. This ensures the spawned session is bound to the tab and will resume correctly on restart.

Sidebar agent launches wait for shell readiness and submit once, even if the first idle event arrives before the command is prepared or the terminal subscribes. Remote repositories use the owning daemon's run configurations, environment and launch-scoped hook paths; desktop profile paths are not copied to that machine.

The active-terminal and sidebar agent menus apply the selected run configuration's environment to that agent launch. The values do not persist in the tab's shell for later commands. `TUIC_SESSION` and `TUIC_PARENT` remain controlled by TUICommander, including differently cased names on Windows. Windows menu launches preserve Unicode characters in environment values and commands.

When the run config's command is a custom alias, symlink, or wrapper (e.g. `c2`, `c`), the foreground-process name no longer matches `"claude"` in `classify_agent`. TUICommander compensates by pre-seeding the session's `agent_type` from the run config at PTY creation time, so intent/suggest parsing and tab-title binding work from the first output line. The foreground-process detector also falls back to the pre-seeded type whenever it sees a non-shell process it doesn't recognise, which covers aliases and wrapper scripts without requiring every name to be hardcoded.

## The embedded agent loop is gone

Unsafe mode, the per-conversation cost footer, per-phase model overrides and the
cron scheduler all belonged to TUICommander's own ReAct loop, which was deleted
in #784-0aec. `ego` runs its own loop over ACP and reaches terminals from
outside, through the `session` MCP tool family — so none of those controls has a
TUICommander-side equivalent any more. Nothing on this page below is affected:
it describes PTY agents (Claude Code, Codex, Gemini, …), which are unchanged.

## Sleep Prevention

When agents are actively working, TUICommander can keep your machine awake:

- Enable in **Settings** → **General** → **Power Management** → **Prevent sleep when busy**
- Uses the `keepawake` system integration
- Automatically releases when all agents are idle

## Environment Flags

Per-agent environment variables can be injected into every new terminal session. Configure in Settings > Agents > expand an agent > Environment Flags.

This is useful for enabling feature flags (e.g., `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`) without manually running `export` commands. Flags are organized by category with toggle, enum, and number types.

**`TUIC_NONINTERACTIVE_HINT=1`** is set in a pane the `tuic`-as-`tmux` shim creates for an automated Claude Code Agent Teams swarm (tmux label `claude-swarm-*`), whose launch command TUICommander types into it — no human is at that prompt. Nothing inside TUICommander reads it; it is for your shell startup files, e.g. to skip an interactive first-run wizard: `[[ -n "$TUIC_NONINTERACTIVE_HINT" ]] && return`. A human's own `tuic`-as-`tmux` session (the default label) never gets it.

## Worktree Context (TUIC_*)

Every terminal spawned in a worktree (and every Setup/Archive/Run script — see the Settings guide's Scripts Tab, and Smart Prompt shell/headless scripts) gets a `TUIC_*` context describing the repo/branch it's in, in addition to `TUIC_SESSION` above:

| Variable | Value |
|---|---|
| `TUIC_SCRIPT_KIND` | `setup` \| `archive` \| `run` \| `prompt` |
| `TUIC_APP_VERSION` | TUICommander's version |
| `TUIC_CONFIG_DIR` | The app's config directory |
| `TUIC_WORKTREE_PATH` | The directory the script/terminal runs in |
| `TUIC_WORKTREE_NAME` | Its basename |
| `TUIC_WORKTREES_DIR` | Its parent directory |
| `TUIC_MAIN_REPO_PATH` | The main checkout (not this worktree) |
| `TUIC_REPO_NAME` | The main checkout's basename |
| `TUIC_IS_WORKTREE` | `"true"`/`"false"` |
| `TUIC_BRANCH` | The current branch — **absent** (not empty) on detached HEAD |
| `TUIC_BASE_REF` | The persisted base ref this branch was cut from, if any |
| `TUIC_BASE_BRANCH` | The repo's configured/detected base branch |

A value that can't be resolved is **omitted from the environment entirely**, never set to an empty string — a `set -u` script fails loudly rather than silently running against an empty branch name.

**Always quote these values.** A branch name may legally contain `&`, `;`, `$`, `(` or backticks: write `"$TUIC_BRANCH"`, never a bare `$TUIC_BRANCH`, and never pass one through `eval`. On Windows the scripts run under `cmd /C`, which expands `%TUIC_BRANCH%` *before* parsing the line, so a bare `%TUIC_BRANCH%` in a branch named `x&calc` runs a second command; quote it (`"%TUIC_BRANCH%"`) or read it from PowerShell as `$env:TUIC_BRANCH`.

**A PTY's env is fixed at spawn time.** If you `cd` to a different worktree in the same tab, these vars keep describing the tab's original spawn directory, not wherever the shell currently is — the same property `TUIC_SESSION` already has. There is no way to keep them live off `cd`; a running process's environment can't be mutated from outside it.

## Tips

- **Multiple agents on the same repo** — Use split panes (`Cmd+\`) to run two agents side by side on the same branch
- **Different agents per branch** — Each worktree is independent, so you can run Claude on one branch and Aider on another
- **Monitor all at once** — Use the Activity Dashboard (`Cmd+Shift+A`) to see every terminal's agent status in one view
