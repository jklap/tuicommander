# TUICommander Specification

**Version:** 1.7.6
**Last Updated:** 2026-08-27

## Overview

TUICommander is a multi-agent terminal orchestrator designed to manage supported AI coding agents, including Claude Code, Gemini CLI, OpenCode, Aider, and Codex, in parallel. It provides per-pane zoom, git worktree isolation, and GitHub integration.

## Goals

1. **Parallel Agent Orchestration** - Run 50+ coding agents simultaneously
2. **Git Worktree Isolation** - Each task gets its own isolated workspace
3. **Per-Pane Font Control** - Independent zoom levels for each terminal
4. **Rate Limit Resilience** - Detection and countdown display when agents hit rate limits
5. **Productivity Features** - Prompt library, keyboard shortcuts, IDE integration

## Architecture

### Technology Stack

| Layer | Technology | Purpose |
|-------|------------|---------|
| Frontend | SolidJS | Reactive UI with fine-grained reactivity |
| Terminal | alacritty_terminal | Native VT engine with canvas rendering |
| Backend | Rust + Tauri | Native PTY management, file system access |
| Build | Vite | Fast HMR development, optimized production builds with automatic code splitting and deferred loading |

### Backend Execution Model

All Tauri commands that perform I/O (git subprocesses, network, bcrypt) are `async` and run inside `tokio::task::spawn_blocking` to avoid blocking Tokio worker threads. Git data is cached with a 60s TTL, invalidated immediately by `repo_watcher` on file system changes. PTY output is serialized once and reused for both Tauri IPC and event bus broadcast. Frontend coalesces paint triggers via `requestAnimationFrame` (~60 repaints/sec) to reduce canvas render passes during burst output.

### Why SolidJS?

- Fine-grained reactivity without virtual DOM
- Direct DOM manipulation for terminal performance
- Compile-time optimizations
- Smaller bundle size than React/Vue
- Familiar JSX syntax

### Frontend Refactoring Workstream

The SolidJS frontend is being refactored incrementally to improve module ownership,
test isolation, and deferred loading without changing the framework or product
behavior. The measured architecture map, dependency constraints, sequencing, and
validation contract are maintained in
[`docs/frontend/solid-refactoring-plan.md`](docs/frontend/solid-refactoring-plan.md).

The work preserves browser/Tauri transport parity and keeps terminal frame and
paint scheduling imperative. Structural changes must remain independently tested
and revertible; line-count reduction alone is not a goal.

### Component Architecture

```
App
├── Sidebar
│   ├── Repository List
│   └── Terminal List
├── TabBar
│   └── Terminal Tabs
├── Terminal Container
│   ├── Terminal (CanvasTerminal)
│   ├── MarkdownPanel
│   └── IdeasPanel
├── GitPanel (side panel)
├── StatusBar
├── PromptOverlay
└── PromptDrawer
```

## State Management

### Stores (SolidJS createStore)

#### terminalsStore
Manages terminal instances and their state.

```typescript
interface TerminalState {
  terminals: Record<string, TerminalData>;
  activeId: string | null;
  nextId: number;
}

interface TerminalData {
  id: string;
  sessionId: string | null;
  fontSize: number;
  name: string;
  awaitingInput: AwaitingInputType;
}

type AwaitingInputType = "question" | "error" | null;
```

### Authoritative agent wait state

Agent lifecycle state is owned by one per-session backend state machine. Every
input turn has a monotonically increasing `turn_epoch`; asynchronous parser and
timer events are applied only to the epoch that produced them. A wait carries
its source and confidence, and every SET has an explicit CLEAR path (submitted
input, choice resolution/disappearance, protocol busy/idle, interruption, or
PTY exit). Terminal scrollback is evidence only for the current chat turn: a
question retained above a later response or completion must never re-arm
`awaiting_input`.

Desktop IPC, HTTP/PWA, WebSocket, MCP, and orchestrator injection use the same
post-input bookkeeping. Frontends render the backend snapshot; parsed terminal
events may trigger one-shot effects but are not a second state authority.
PTY lifecycle mutations reach that snapshot through a lossless ordered lane;
the broadcast event bus is reserved for reconnectable live consumers and cannot
be the sole carrier of sticky SET/CLEAR state. Before any lifecycle evidence,
the shell state is absent and a detected agent remains `starting`.

MCP managed-agent commands use one `session action=submit` request. It claims a
confirmed-idle empty composer, never queues, serializes the complete raw-mode
payload through Enter, advances this same input FSM and epoch, and waits
internally for bounded child terminal movement. The receipt distinguishes
complete, not-started, and uncertain writes; only a provably not-started write
is retry-safe. Terminal movement is acknowledgement evidence, not semantic
application acceptance. `session action=input` remains raw and write-only.

#### repositoriesStore
Manages the list of git repositories.

```typescript
interface Repository {
  path: string;
  displayName: string;
}
```

#### settingsStore
User preferences with localStorage persistence.

```typescript
interface SettingsState {
  ide: IdeType;
  fontFamily: FontType;
  defaultFontSize: number;
}
```

#### promptLibraryStore
Saved prompts with variable substitution.

```typescript
interface SavedPrompt {
  id: string;
  name: string;
  content: string;
  description?: string;
  shortcut?: string;
  category: PromptCategory;
  isFavorite: boolean;
  variables?: PromptVariable[];
  lastUsed?: number;
  useCount: number;
}

interface PromptVariable {
  name: string;
  description?: string;
  defaultValue?: string;
}
```

#### rateLimitStore
Tracks rate limit status per session.

```typescript
interface RateLimitInfo {
  sessionId: string;
  agentType: AgentType;
  detectedAt: number;
  retryAfterMs: number | null;
}
```

## Hooks

Key hooks in `src/hooks/`:

- **usePty** — PTY session lifecycle (spawn, write, resize, close, subscribe to data/exit events)
- **useRepository** — Git operations (getInfo, getDiff, getDiffStats, openInApp with line/col, renameBranch)
- **useGitHub** — Reactive wrapper over `githubStore`; returns `{ status, loading, error, refresh, startPolling, stopPolling }`
- **useKeyboardRedirect** — Redirects keyboard input from non-terminal areas to active terminal
- **useFileDrop** — External file drag & drop handling

See `src/hooks/` for full signatures — the above is a representative summary.

## Agent Types

```typescript
type AgentType = "claude" | "gemini" | "opencode" | "aider" | "codex" | "amp" | "cursor" | "goose" | "grok" | "droid" | "pi" | "git" | "api";
```

Full agent configuration (binary, resume command, session discovery, detection patterns) lives in `src/agents.ts`.

### PTY versus ACP routing

Two transports carry an assistant, and a session belongs to exactly one of them.
There is no hybrid route and no fallback between them.

- **PTY.** Every member of `AgentType` above. TUICommander allocates a terminal,
  runs the CLI executable, and infers state by parsing the rendered rows into
  `ParsedEvent`. Session state is recovered from the agent's own session files
  on disk (see AGENTS.md, "Agent Session Management").
- **ACP.** `ego` only, through the Agent Client Protocol v1 client in
  `src-tauri/src/acp/`. TUICommander launches `ego acp -C <root>` directly and
  owns its stdio JSON-RPC connection. No terminal is allocated, no shell is
  invoked, and no output is scraped.

`ego` is deliberately **not** an `AgentType`. `AgentType` describes a CLI
executable, its launch arguments and its parser behaviour; it carries no
negotiated protocol version, connection lifetime, capability snapshot, reverse
request, or durable ACP session ID. Adding `ego` to it would make the ACP
process look like a terminal and would recreate exactly the hybrid this rule
forbids. ACP data is already structured and must never pass through the terminal
parser or be projected into `ParsedEvent`.

When the ACP connection fails, it settles as a failure. It does not degrade to a
PTY session, to reading ego's files, or to terminal input. There is no runtime
feature flag selecting between the two paths.

The source contract is `plans/ego-acp-client.md`, which is final rather than
exploratory. It supersedes `plans/acp-transport-integration.md` and
`ideas/acp-integration.md`; those describe an early proof of concept whose PTY
fallback and feature gate were rejected, and they do not define the
implementation.

## Rate Limit Detection

Provider-specific patterns detect rate limits in terminal output:

### Claude Code
- `rate limit`
- `API rate limit`
- `overloaded`
- `try again later`
- Retry-after extraction from error messages

### Gemini CLI
- `429`
- `quota exceeded`
- `rate limit exceeded`
- `resource exhausted`

### Generic Patterns
- `too many requests`
- `rate limited`
- `slow down`
- `retry after`

## Output Parser

JSONL event parser for structured agent output:

```typescript
type OutputEventType = "result" | "assistant" | "error" | "tool" | "system" | "unknown";

interface OutputEvent {
  type: OutputEventType;
  content: string;
  timestamp: number;
  raw?: unknown;
}
```

Features:
- Streaming parser with line buffering
- 100KB buffer limit to prevent memory bloat
- Handles partial lines across chunks

## Keyboard Shortcuts

### Global
| Shortcut | Action |
|----------|--------|
| Cmd+T | New terminal |
| Cmd+W | Close terminal |
| Cmd+K | Open prompt library |
| Cmd+Shift+D | Toggle Git Panel |
| Cmd+G | Git Panel — Branches tab |
| Cmd+M | Toggle markdown panel |
| Cmd+Alt+N | Toggle Ideas panel |
| Cmd+O | Open file… |
| Cmd+N | New file… |
| Cmd+1-9 | Switch to tab N |
| Cmd++/- | Zoom in/out |
| Cmd+0 | Reset zoom |
| Cmd+F | Find in terminal |
| Cmd+E | Toggle file browser |
| Cmd+[ | Toggle sidebar |
| Cmd+? | Toggle help panel |
| Cmd+Shift+[ | Previous tab |
| Cmd+Shift+] | Next tab |
| Cmd+Shift+T | Reopen closed tab |
| Cmd+P | Command palette |
| Cmd+Shift+A | Activity dashboard |
| Cmd+, | Settings |

### Prompt Library
| Shortcut | Action |
|----------|--------|
| ↑/↓ | Navigate prompts |
| Enter | Insert prompt |
| Ctrl+N | New prompt |
| Ctrl+E | Edit selected |
| Ctrl+F | Toggle favorite |
| Esc | Close drawer |

## Workspaces: linked worktree versus copy-on-write clone

A workspace is a linked worktree or a copy-on-write clone of the whole
repository directory. Both are created by one entry point
(`worktree::create_workspace`) and land at the same destination, derived from
the storage strategy and the sanitized task name; the caller asks for a
workspace, not for a mechanism.

**Mode.** `auto` (default) takes a clone when every guard and the capability
probe pass and a linked worktree otherwise, carrying the reason it degraded —
"you got a worktree" without one is indistinguishable from "you asked for a
worktree". `cow` fails naming the check that refused, because a caller asking
for a clone wants the isolation and a silent worktree has different semantics.
`worktree` forces the previous behaviour and reports no degradation, since that
is a choice rather than a degradation. The clone guards run for `auto` and
`cow` only: a linked worktree never had them.

**Capability is measured, never inferred.** Support is decided by a real
copy-on-write copy of `.git/HEAD`, with a same-volume device-id comparison as a
cheap pre-filter. A filesystem name says nothing about a specific mount or a
specific pair of paths. The probe and the clone try the same mechanisms from one
shared list, because a probe that accepts a mechanism the clone cannot issue
turns a degrade into an error raised after the decision was already made — and
every mechanism on that list must fail rather than degrade to a byte copy.

**Guards refuse; they never repair.** Destination inside the source, a linked
worktree as source, a bare repo, an in-progress operation marker, or a lock
held by a live writer each refuse with the reason. Deleting an inherited lock
or finishing someone else's rebase would turn a torn copy into a
plausible-looking corrupt one. A *stale* lock is the one exception and is
dropped in the copy, by the caller, after the copy exists — never in the
source.

**There is no full-copy fallback.** A `cp -c -R` that fails after the probe
succeeded means something changed underneath; falling back to a recursive copy
would silently turn a 19 MB clone into a 12 GB one. A clone whose fixups fail
is removed rather than handed back looking usable.

**Dirty policy** applies to clones only: `inherit` (default, free),
`clean_untracked` (`clean -fd`, never `-fdx` — the ignored build output is the
point of the clone), `clean` (`reset --hard --recurse-submodules` then the same
clean, the only policy that costs real disk).

**Identity.** A linked worktree's `workspace_id` **is** its branch. A clone's id
is minted, because two workspaces may sit on the same branch and a branch can
therefore not name one. Every workspace API is keyed on the id; `kind` is
persisted on the record so lifecycle code never infers the mechanism from the
path.

**Publish** exists for clones only — a linked worktree shares its refs with the
parent. It stages the tip under `refs/tuic/published/<id>` in the parent, then
fast-forwards `refs/heads/<branch>` with a compare-and-swap against the ref it
read. Fast-forward only: with two workspaces on one branch a divergent parent
branch is the normal case, and forcing would orphan whichever side published
second. A parent branch that is checked out is refused; publish is not an
implicit checkout. The parent update and the origin push fail and are reported
independently.

**Removal of a clone is gated, not warned.** Deleting it deletes a repository,
and unpublished commits — reachable from HEAD and from no remote and no
mirrored parent ref — exist nowhere else. The gate lives in the shared
id-addressed removal path, so no transport bypasses it, and `force` defaults to
false on all three. A parent mirror that fails to refresh makes the count err
high: refusing a removal that might have been safe is the correct direction.

## Persistence

Repository state is persisted by the Rust backend in `repositories.json`, in
the single platform config directory shared by debug and release builds,
written through the locked/atomic `ConfigFile` path.

At bootstrap, `tuic-remote` may select one immutable named application instance
with `--instance <id>`. Omitting it preserves the existing platform config path,
keyring tuple (`tuicommander`/`vault`), and migrations. A named ID is a lowercase
ASCII DNS label of 1–63 characters with alphanumeric ends and internal hyphens;
`default` is reserved. Its files live below
`<platform-app-config>/instances/<id>/` and its vault uses
`tuicommander-instance-<id>`/`vault`. Named state never falls back to or migrates
default/legacy state. Selection occurs before `--set-password` or persistence,
and an invalid ID or unavailable named release vault terminates before network
bind. Runtime switching is unsupported.

Production black-box consumers pin and verify the digest of the exact
`tuic-remote` artifact they launch. That consumer-side artifact identity is the
capability proof for this contract; the daemon exposes no additional capability
or version endpoint.

Ordinary `config.json` and `mcp-upstreams.json` mutations use delta-under-lock
semantics: after taking the cross-process file lock, the backend reloads the
latest document and applies only the caller's changed fields or server-ID
operations. Independent saves from concurrent debug and release instances do
not overwrite one another's unrelated changes.

Some frontend-only stores persist to localStorage:

| Key | Store | Content |
|-----|-------|---------|
| `tui-commander-settings` | settingsStore | IDE, font, preferences |
| `tui-commander-prompt-library` | promptLibraryStore | Saved prompts |

## Feature Status

### Completed (P1)
- [x] ACP client for ego — backend complete (connections, sessions, turns, the
      agent's questions back, ego's pause/resume/compact), reachable identically
      from Tauri IPC and HTTP; no frontend surface yet
- [x] Multi-agent support through the canonical `AgentType` registry
- [x] Git worktree management per task
- [x] Copy-on-write workspaces: mode/dirty on all three transports, publish
      into parent and origin, removal gated on unpublished commits
- [x] Agent spawning integration
- [x] SolidJS migration

### Completed (P2)
- [x] Split pane layout
- [x] Multi-repository sidebar
- [x] Git diff panel
- [x] Interactive agent prompts UI
- [x] IDE launcher dropdown
- [x] GitHub integration
- [x] Sidebar PR badges retain `#number` while showing lifecycle, conflict, CI, and review state
- [x] Parallel agent orchestration
- [x] Orchestrated PTY task descriptions with prompt-derived fallback metadata
- [x] One-call MCP managed-agent submission with bounded terminal-movement receipt
- [x] Expandable terminal Context bar for agent intent, orchestrator assignment, and last user prompt
- [x] Font selection setting
- [x] Tab bar with keyboard navigation
- [x] Density modes for readability
- [x] Terminal selection copy unwraps soft-wrapped rows and removes coherent Claude visual gutters without altering literal block characters
- [x] Status bar with branch and PR info
- [x] Rate limit detection
- [x] JSONL output parsing
- [x] Prompt library with variables
- [x] Keyboard redirect to terminal
- [x] Ideas panel (formerly Notes) with send-to-terminal and delete actions
- [x] Terminal session persistence across app restarts
- [x] GitHub GraphQL API (replaces gh CLI for PR/CI data)
- [x] Multi-account GitHub: multiple github.com logins + GitHub Enterprise Server (PAT), per-repo account bindings with ambiguity chooser, isolated per-account polling/rate-limits/circuit-breaker (see FEATURES.md 8.14)
- [x] Auto-update via tauri-plugin-updater with progress badge
- [x] Prevent system sleep while agents are working (keepawake)
- [x] Usage limit detection for Claude Code (weekly/session) with status bar badge
- [x] Repository groups with accordion UI (named, colored, collapsible, drag-and-drop)
- [x] HEAD file watcher for branch change detection
- [x] Git status via .git file reads (no subprocess)
- [x] Lazy terminal restore (sessions materialize on branch click, not app startup)
- [x] Windows compatibility (shell escaping, process detection, resolve_cli, IDE detection)
- [x] Repo watcher for automatic panel refresh on `.git/` changes
- [x] Git Panel (4 tabs: Changes with History/Blame sub-panels, Log with canvas commit graph, Stashes, Branches) — replaces Git Operations Panel and DiffPanel
- [x] Branch Panel (4th tab in Git Panel): checkout, create, delete, rename, merge, rebase, push, pull, fetch, prefix folding, inline search, context menu, stale/merged indicators. `Cmd+G` opens directly on Branches tab
- [x] Context menu submenus and "New Group..." via PromptDialog
- [x] File Browser panel (`Cmd+E`) with content search (`Cmd+Shift+F`, case/regex/whole-word, streaming results)
- [x] CodeMirror code editor
- [x] Find in terminal (`Cmd+F`)
- [x] Configurable keybindings system
- [x] Command palette (`Cmd+P`)
- [x] Activity dashboard (`Cmd+Shift+A`)
- [x] Park repos feature
- [x] Plugin system (see FEATURES.md section 17)
- [x] Remote access / HTTP server
- [x] Mobile Companion PWA (sessions, live output, question reply, activity feed)
- [x] MCP Proxy Hub (aggregate upstream MCP servers via HTTP and stdio, tool namespace prefixing, circuit breaker, hot-reload, OS keyring credentials, tool filtering, session-local Grok compatibility through lazy meta-tools)
- [x] Copy Path in Markdown panel
- [x] Claude Usage Dashboard (native SolidJS component with API polling, session analytics, usage timeline)
- [x] ConfirmDialog component (in-app dark-themed replacement for native OS dialogs)
- [x] Status bar unified agent badge with priority cascade (rate limit > usage API > PTY usage > name)
- [x] Movement-based PTY agent activity detection ("text above the input area moves = active") with explicit-hook precedence, prompt-based Ready screens, Codex presence-based Working policy, Grok activity/composer disambiguation, interrupt confirmation, and confirmed-idle safety gates
- [x] PR lifecycle filtering (CLOSED hidden, MERGED hidden after 5min user activity)
- [x] Notes/Ideas: mark as used, badge count in status bar
- [x] Notes/Ideas: image paste support (Ctrl+V), thumbnails, send absolute paths to terminal
- [x] Inter-Agent Messaging (`messaging` MCP tool: register, list_peers, send, inbox with channel push + polling fallback)
- [x] Smart Prompts (29 built-in AI prompts with context variable resolution, inject/headless/API execution, toolbar dropdown, SmartButtonStrip, Command Palette integration, direct LLM API mode via genai crate)
- [x] AI Chat panel (`Cmd+Alt+A`) — streaming conversational AI with terminal context injection, multi-provider (Ollama/Anthropic/OpenAI/OpenRouter), conversation persistence, OS-keyring API keys
- [x] AI Agent loop (ReAct) — terminal observe/act, filesystem, search, drive_agent, and reactive watch tools; pause/resume, destructive-command approval gate, tool-call cards
- [x] Session knowledge store — per-session command outcomes, error→fix pairs, CWD history, TUI apps seen; fed by OSC 133 with silence-timer fallback; persisted with 2s debounce
- [x] TUI app detection — alternate-screen tracking classifies terminal as Shell or FullscreenTui with app hint (vim/htop/lazygit/…)
- [x] `ai_terminal_*` MCP tools — external agent surface (Claude Code, Cursor) driving TUICommander terminals with user-confirmation gates
- [x] ChoicePrompt parser variant — numbered confirmation menu detection with destructive-label flagging, PWA overlay, `sendPtyKey()` helper
- [x] MCP OAuth 2.1 — RFC 9728 + RFC 8414 PKCE flow for upstream MCP servers, `tuic://oauth-callback` deep link, shared `TokenManager` with thundering-herd-safe refresh
- [x] GitHub Ops dashboard — live review/conflict/autofix/changelog state plus Headless-slot improvement proposals with explicit issue creation

### Completed (Voice Dictation)
- [x] Local Whisper inference via whisper-rs (Metal GPU acceleration)
- [x] Audio capture (cpal, 16kHz mono resampling)
- [x] Text correction map (longest-match-first dictionary)
- [x] Model download from HuggingFace (large-v3-turbo)
- [x] Push-to-talk mic button in StatusBar (blue pulsing animation)
- [x] Configurable push-to-talk hotkey (keydown/keyup)
- [x] Transcribed text injection into active terminal via PTY
- [x] Settings > Dictation tab (model, hotkey, language, corrections)
- [x] Shell integration inject_text stub (prepared for external triggers)
- [x] Streaming transcription with adaptive sliding windows (1.5s→3s)
- [x] VAD energy gate (ported from whisper.cpp vad_simple)
- [x] Floating toast for partial transcription results
- [x] Prompt token carry-forward across windows

### Completed (P2)
- [x] Alternate-screen scrollback — isolated bounded history for fullscreen apps, primary-only durable logs, and atomic renderer-generation transitions
- [x] Task completion detection
- [x] Audio notification when agent awaits input
- [x] Intent tab titles override spawn labels while explicit user renames remain protected across reconnects
- [x] Remote completion muting survives reconnects and deduplicates idle/exit signals per busy cycle
- [x] IDE launcher with app icons

### Pending (P2)
- [ ] Error handling strategy config

### Agent Configuration (Done)
- [x] Settings > Agents tab with per-agent run configurations
- [x] MCP bridge install/remove for every MCP-capable agent in the canonical registry
- [x] Terminal context menu > Agents submenu with run configs
- [x] Agent binary detection and version display
- [x] agents.json persistence for run configurations

### Completed (P3)
- [x] Markdown rendering (MarkdownPanel, not inline terminal)
- [x] Task queue UI
- [x] Advanced keyboard shortcuts

### Pending (P3)
- [ ] Agent stats display
- [ ] Config file support
- [ ] TypeScript PTY wrapper

## Future Considerations

### WebSocket Backend
For web deployment without Tauri:
- Go backend with PTY multiplexing
- WebSocket protocol for PTY I/O
- Session management

## References

- [SolidJS Documentation](https://www.solidjs.com/docs/latest)
- [alacritty_terminal crate](https://crates.io/crates/alacritty_terminal)
- [Tauri Documentation](https://tauri.app/v1/guides/)
