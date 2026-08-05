# MCP & HTTP Server

**Module:** `src-tauri/src/mcp_http/mod.rs`

Optional HTTP/WebSocket server that exposes all Tauri commands as REST endpoints. Enables browser-mode operation and MCP (Model Context Protocol) integration for external AI tools.

## Activation

The server has two independent listeners:

- **IPC listener** (always started): On macOS/Linux, listens at `<config_dir>/mcp.sock` (Unix domain socket). On Windows, listens on `\\.\pipe\tuicommander-mcp` (named pipe). No authentication — used by the local `tuic-bridge` sidecar.
- **TCP listener** (opt-in): Only starts when remote access is enabled. Binds to `0.0.0.0:<port>` (port from `services.server`) with Basic Auth.

The `mcp_server_enabled` config flag controls whether the `/mcp` protocol route is active (MCP tool discovery and invocation), not whether the server itself starts. The HTTP API endpoints (sessions, git, config, etc.) are always available on the IPC listener.

The local IPC listener is independent from the **Remote Access** TCP toggle. Turning remote access on or off only starts or stops the authenticated TCP listener; it does not disable `mcp.sock` or the local MCP route. Lifecycle logs state whether a transition affects TCP or the always-on IPC listener.

Configuration via Settings > Services, or `config.json`:

```json
{
  "mcp_server_enabled": true,
  "mcp_port": 9876
}
```

On startup, the server:
1. Binds the IPC listener: Unix socket at `<config_dir>/mcp.sock` (macOS/Linux) or named pipe `\\.\pipe\tuicommander-mcp` (Windows)
2. If remote access is enabled, binds a TCP listener on the configured port
3. Starts Axum HTTP server on a background tokio thread
4. Enables CORS for browser mode
5. Spawns MCP session reaper (evicts stale sessions after 1h TTL)
6. Spawns upstream health checker for proxied MCP servers

## Unix Socket Lifecycle (macOS/Linux)

The socket at `<config_dir>/mcp.sock` is managed with two safety layers to survive crashes and rapid restarts:

| Layer | Mechanism | Purpose |
|-------|-----------|---------|
| **Retry bind** | 3 attempts × 100 ms, each removes stale file before trying | A crashed previous run leaves a dead socket file that blocks `bind(2)` — retrying clears it |
| **Real liveness check** | `UnixStream::connect()` in `get_mcp_status` | `file.exists()` returns `true` for stale sockets; only a real connect reveals whether the server is alive |

**Why this matters for AI tool integrations:** The `tuic-bridge` sidecar connects via the Unix socket to expose TUICommander tools to Claude Code. If the socket is stale (app crashed, Tauri force-quit), the bridge cannot connect and returns `tools: []`, silently disabling all MCP tools in the agent session. The retry bind ensures the socket is always valid on restart; the real liveness check ensures the UI accurately reports the server state.

## REST API Endpoints

### Session Management

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/sessions` | List active PTY sessions |
| `POST` | `/sessions` | Create new PTY session |
| `POST` | `/sessions/:id/write` | Write data to session |
| `POST` | `/sessions/:id/resize` | Resize session terminal |
| `GET` | `/sessions/:id/output` | Read session output (ring buffer) |
| `POST` | `/sessions/:id/pause` | Pause session output |
| `POST` | `/sessions/:id/resume` | Resume session output |
| `DELETE` | `/sessions/:id` | Close session |

### Monitoring

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check |
| `GET` | `/stats` | Orchestrator stats (active/max/available) |
| `GET` | `/metrics` | Session metrics (spawned, failed, bytes) |
| `GET` | `/process/stats` | CPU% and RSS memory for TUIC and all child process trees |
| `GET` | `/process/monitor` | Self-contained HTML dashboard for process metrics (for remote/PWA/mobile) |

### Git Operations

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/repo/info?path=` | Get repository info |
| `GET` | `/repo/diff?path=` | Get git diff |
| `GET` | `/repo/diff-stats?path=` | Get diff stats |
| `GET` | `/repo/changed-files?path=` | List changed files |
| `GET` | `/repo/branches?path=` | List git branches |
| `GET` | `/repo/github-status?path=` | Get GitHub status |
| `GET` | `/repo/pr-statuses?path=` | Get batch PR statuses |
| `GET` | `/repo/ci-checks?path=` | Get CI check details |
| `POST` | `/ai/review/pr` | AI review of a PR diff → line-level findings (Main slot) |
| `POST` | `/repo/create-pr` | Create a PR (gh wrapper, UI-gated) |
| `POST` | `/repo/create-issue` | Create an issue (gh wrapper, UI-gated) |
| `POST` | `/repo/post-pr-review` | Post a PR review with inline comments |
| `GET` | `/repo/merged-prs?path=&sinceTag=` | Merged PRs via GraphQL (changelog source) |
| `GET` | `/repo/changelog?path=&sinceTag=` | AI changelog `{markdown, json}` (Headless slot) |
| `POST` | `/repo/conflict-assist` | Worktree + rebase; reports verified/unverified clean or conflicts, base source, warning, and agent prompt |

### Configuration

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/config` | Get app config |
| `PUT` | `/config` | Save app config |
| `POST` | `/auth/hash-password` | Hash password for remote access |

`GET /config` and MCP `config action=get` redact remote-access secrets:
`services.auth.password_hash`, `services.auth.session_token`,
`services.relay.token`, and `services.push.vapid_private_key`. The config shape
exposes only the corresponding `*_exists` booleans for secret presence.

`PUT /config` and MCP `config action=save` both advertise "config fields to save"
and both accept a partial body: it is deep-merged onto the live config by
`merge_partial_app_config`, so an omitted field keeps its current value instead of
falling back to its serde default. Both also share `server_settings_changed` with
the IPC `save_config` and rebind the listener through
`restart_after_server_settings_change`, so no transport can leave the process
serving a configuration the disk disagrees with. See
[`config.md`](config.md#application-config-configjson) for the merge semantics.

### Agents

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/agents/detect` | Detect installed agents and IDEs |

### Plugins

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/plugins/docs` | Plugin development guide (AI-optimized reference) |
| `GET` | `/api/plugins/:plugin_id/data/*path` | Read plugin data file (JSON or plain text) |

### Worktrees

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/worktrees` | Create worktree |
| `DELETE` | `/worktrees` | Remove worktree |
| `GET` | `/worktrees/paths?path=` | Get worktree paths for repo |

## Streaming

### WebSocket (`/sessions/:id/stream`)

Real-time PTY output streaming per session. Connects to the session's broadcast channel.

```
Client ──WebSocket──> /sessions/{session_id}/stream
                      Server pushes PTY output as text frames
```

### Session Lifecycle Events

When sessions are created or closed (via HTTP, MCP, or PTY exit), the server broadcasts events through the SSE event bus:

- **`session-created`** — Emitted when a new PTY session is created (both local and MCP-spawned). Carries `session_id`, `cwd`, `agent_type`, and the optional stable `display_name`. Frontend uses this to auto-add terminal tabs for remotely spawned agents and treats a supplied display name as custom so transient OSC/intent titles cannot replace it.
- **`term-alias-assigned`** — Emitted when a session receives its human-friendly alias. Carries `session_id` and `alias`. Frontend uses this to update tab tooltips.
- **`session-closed`** — Emitted when a session exits. Carries `session_id`. Frontend uses this for cleanup.

These events are available on the SSE `/events` stream used by the mobile PWA and any connected WebSocket clients.

### Streamable HTTP (`POST /mcp`)

MCP Streamable HTTP transport (spec 2025-03-26):

```
Client ──POST──> /mcp   (JSON-RPC request, response in body)
Client ──GET───> /mcp   (SSE stream for server notifications, requires Mcp-Session-Id header)
Client ──DELETE─> /mcp  (end session, pass Mcp-Session-Id header)
```

**`tools/call` does not require `Mcp-Session-Id`.** Identity is resolved per call
rather than demanded up front. A caller that sends the header gets its protocol
session refreshed — a stale id (app restart, or a long-lived client like Claude Code
that lost its session) auto-recovers instead of erroring. A caller that sends none is
served anyway: most tools need no caller identity at all, so a plain `curl` against
`POST /mcp` now works.

The identity-scoped actions (`agent action=register|send|inbox|wait`) still refuse
without a protocol session — but each refuses on its own, with the concrete next step
(`initialize`, or `agent action=register`) that the previous blanket `-32600` never
gave the caller. Making identity itself independent of the protocol session is a
separate step (Phase E of the dual-era plan); an `x-tuic-session` header binds a TUIC
identity **to** an existing `mcp-session-id`, it does not substitute for one.

`GET /mcp` and `DELETE /mcp` still require the header — both are bound to a specific
protocol session, and `GET` answers `401` without it.

The `GET /mcp` SSE stream emits `notifications/tools/list_changed` whenever the available tool set changes (e.g., native tools are enabled/disabled via config, or upstream MCP servers connect/disconnect). The bridge sidecar subscribes to this stream and forwards the notification to the AI agent.

The bridge uses the standard MCP `ping` request for its three-second liveness check. This keeps health traffic constant-size as terminal count grows; it does not rebuild or serialize the complete tool catalog. If IPC is reachable but `/mcp` is unavailable, the bridge reports that the MCP endpoint is unavailable instead of incorrectly claiming the desktop process is not running.

The bridge retains the downstream client's last `initialize` request and replays
it internally after reconnecting to a restarted TUIC process. This restores
session-scoped metadata such as Grok compatibility mode without sending a
second initialize response to the client.

On reconnect, a peer may reclaim its stable TUIC identity after the prior MCP protocol session has no live SSE subscriber and has missed the bridge activity grace period. The takeover retires the old forward and reverse routing entries atomically.

A currently subscribed or recently active owner is never replaced — but it can be *joined*. One PTY may hold more than one bridge (Codex opens two), and both inherit the same `$TUIC_SESSION`, so both assert the same `x-tuic-session`. Only a process that inherited that PTY's environment can assert it, so a second asserting bridge is a sibling, not a claimant: it is added to the identity's routing (`mcp_to_session` plus the `session_to_mcp` list) while the live owner keeps delivery ownership. Ownership stays put on purpose — two live siblings that traded it on every request would flip the delivery channel back and forth. The inbox is keyed by the PTY identity, so both bridges read the same mail. `agent action=register` from a joined sibling is a rename, not a takeover; a protocol session with neither the header nor an existing route is still refused with "already registered to another active MCP session" (throttled to one WARN per claimant pair). Ending one co-owner's protocol session drops only its own routes and promotes a survivor to delivery owner; the peer entry, inbox and orchestrator role are torn down only when the last co-owner goes.

**Blocking tool actions run off the runtime worker.** Dispatch is action-aware:
session create/input/close/kill/process-stats, agent spawn/detect/send, and config
writes use `run_blocking_handler` (`tokio::task::spawn_blocking`) because they may
sleep, spawn processes, write PTYs, or touch disk. Common read-only actions such
as session list/output/status and agent list-peers/inbox/stats/metrics execute
inline without cloning their JSON payload or scheduling blocking-pool work.
Async waits remain on their event-driven async handlers. A panicking blocking
handler becomes a tool error rather than killing the request task; `POST /mcp`
does not add another blanket wrapper.

### Lazy Tool Discovery (`collapse_tools`)

When `collapse_tools: true` in `config.json` (or via Settings > Services > TUIC Tools > "Collapse tools"), the server replaces the full tool list in `tools/list` with exactly three meta-tools (the Speakeasy pattern):

| Meta-tool | Purpose |
|-----------|---------|
| `search_tools` | BM25 search over the full native + upstream tool corpus; returns name/description pairs |
| `get_tool_schema` | Returns the full `{name, description, inputSchema}` for a specific tool |
| `call_tool` | Dispatches to the named tool — routes to the native handler or `proxy_tool_call` for `{upstream}__{tool}` names |

Rationale: a cold tool list of 100+ tools costs ~35k tokens in every agent turn; the 3 meta-tools cost ~500 fixed tokens and the agent fetches schemas on demand. Toggling `collapse_tools` fires `notifications/tools/list_changed` so connected clients refresh their tool cache.

TUIC also selects this three-tool surface automatically for an individual Grok
session when `initialize.clientInfo.name` starts with `grok-shell-`. Grok accepts
only one `__` namespace delimiter in a qualified MCP tool name, so it otherwise
discards proxied names such as `tuicommander__upstream__tool`. The meta-tools keep
the upstream identifier in the `call_tool` argument instead of the qualified MCP
tool id. This compatibility mode is session-local: it does not change
`collapse_tools` or the tool surface returned to other connected clients.

**Filter enforcement.** Both `search_tools` and `call_tool` re-apply the safety filters that the full listing would apply: `disabled_native_tools` is checked up-front in `handle_call_tool`, and upstream allow/deny filters are enforced at both enumeration time (`aggregated_tools`) and dispatch time (`proxy_tool_call`). This is critical under collapse mode: discovery no longer gates dispatch, so an agent that knows a filtered tool name cannot bypass the filter by calling `call_tool` directly. `search_tools` and `get_tool_schema` also reject meta-tool names, and `call_tool` refuses to recurse into itself.

The BM25 index lives in `AppState::tool_search_index` (`parking_lot::RwLock<ToolSearchIndex>`, backed by `src-tauri/src/tool_search.rs`). A background task subscribes to the `mcp_tools_changed` broadcast and rebuilds the index whenever the tool set changes (upstream connect/disconnect, `disabled_native_tools` edit, `collapse_tools` toggle).

The MCP instructions string returned by `initialize` (`build_mcp_instructions`) swaps to a "lazy discovery" guide when `collapse_tools: true` or the connecting Grok session requires compatibility mode, so agents know to call `search_tools` first rather than looking for a flat tool table.
The TUIC connection acknowledgment in those instructions is emitted exactly once per MCP connection or reconnect, never once per conversational turn. TUIC protocol context remains in initialize instructions and native core-tool descriptions only; upstream tool descriptions are preserved instead of receiving a repeated TUIC preamble.

### MCP Native Tools

Eight native tools, organized by domain. Two (`config`, `debug`) are hidden by default via `disabled_native_tools` — discoverable through `search_tools`/`get_tool_schema`/`call_tool` when `collapse_tools` is enabled.

| Tool | Actions | Default |
|------|---------|---------|
| `session` | list, create, input, output, status, wait, resize, close, kill, pause, resume, process_stats | Enabled |
| `agent` | spawn, wait, detect, stats, metrics, register, list_peers, send, inbox | Enabled |
| `task` | get, cancel | Enabled |
| `repo` | list, active, prs, status, worktree_list, worktree_create, worktree_remove | Enabled |
| `ui` | tab, toast, confirm | Enabled |
| `plugin_dev_guide` | *(no actions — returns guide text)* | Enabled |
| `config` | get, save | Disabled |
| `debug` | agent_detection, logs, sessions, invoke_js | Disabled |

The `disabled_native_tools` config key accepts an array of tool names to hide from `tools/list`. Default: `["config", "debug"]`.

Native responses omit optional values when they are unavailable. In particular,
`session action=output` includes `exit_code` only after an exit status is known, and
blocking wait timeouts return only the condition state without a repeated follow-up hint.
Native tool values are wrapped as compact JSON text in the MCP `content` envelope.
A proxied upstream value that is already a valid MCP `CallToolResult` object (an object
with a `content` array) instead becomes the JSON-RPC result directly, preserving its
`content`, `isError`, `structuredContent`, and any extension fields without mutation.
This applies both to direct `{upstream}__{tool}` calls and to the collapsed `call_tool`
meta-path. A malformed upstream value falls back to the native compact JSON text
envelope so the response remains protocol-valid and inspectable.

#### `task` tool — long-running orchestration past the 300s ceiling

`agent action=wait` and `session action=wait` are server-side blocking long-polls
clamped to `WAIT_MAX_MS` (300 000 ms). An orchestrator supervising a peer that works
longer than five minutes cannot hold one open, and a client that drops mid-wait loses
the outcome entirely.

`agent action=spawn` therefore also returns a **task handle**:

```json
{ "session_id": "…", "task_id": "…", "poll_interval_ms": 1000, "…": "…" }
```

The handle is polled with `task action=get` instead of holding a wait open. This is
purely additive — every field a pre-task client already read keeps its name and type,
and a client that ignores `task_id` behaves exactly as before.

**Lifecycle.** A task is created `working` once the PTY is live (a refused or failed
spawn leaves none). `mark_session_exited` (`pty.rs`) drives it to `completed` with
`result = {session_id, exit_code}`, or to `failed` with `error` when the exit code is
non-zero — so the outcome is recorded whether or not anyone was listening.

| Status | Meaning |
|--------|---------|
| `working` | The agent is running |
| `input_required` | Waiting on input; still live |
| `completed` / `failed` / `cancelled` | **Terminal and immutable** — never change again |

The vocabulary is the MCP `2026-07-28` Tasks vocabulary from day one, so the standard
`tasks/*` front door stays a serialization change rather than a semantic remap.

**`task action=get`** returns `{task_id, status, status_message?, result?, error_detail?,
poll_interval_ms}`, absent optional fields omitted. A failed task reports its reason in
`error_detail`, **not** `error` — a top-level `error` always means the call itself failed,
so reusing it would make a successful poll look like a broken one.

**`task action=cancel`** marks the task `cancelled` and does **not** kill the agent
(`session action=kill` does that). Cancelling an already-finished task is not an error:
it reports the state that stands with `cancelled: false`, so a cancel racing the agent's
exit never looks like a failure. Because terminal states are immutable, a cancel that
lands first is never overwritten by the later exit.

**Ownership.** A `task_id` is a capability over a spawned agent, so ownership is checked
before any state is returned or mutated: one agent cannot inspect or cancel another's
children. A caller that spawned before registering is stamped with its pending id and
still reaches the handle after it auto-binds a TUIC identity. `cancel` additionally
re-checks the same loopback guard as `agent spawn`; `get` is read-only monitoring and
stays open to authenticated remote clients.

**Retention.** Tasks live in memory for 24 h and are reaped by the existing
`mcp_sessions` reaper, not a separate timer. They are deliberately **not** persisted: the
case this exists for is a *client* restart, which the TUIC process outlives. Disk would
only cover a TUIC restart — and that tears down every PTY, so a recovered `working` task
would describe an agent that no longer exists.

#### `ui` tool — `tab` URL schemes

The `url` param of `action=tab` supports three schemes:

| Scheme | Behaviour |
|--------|-----------|
| `http(s)://` / `file://` | Loaded in a sandboxed iframe |
| `tuic://edit/<path>?line=N` | Opens a native code-editor tab at the given file and line. Absolute paths require a `//` prefix: `tuic://edit//Users/x/file.rs?line=42`. Relative paths resolve against the active repo root. |
| `tuic://open/<path>` | Opens a native markdown/preview tab |

Custom URL schemes (`vscode://`, `x-devonthink://`, etc.) do **not** work inside iframes and must not be used with `action=tab`.

### MCP Tools: `ai_terminal_*` (external agent surface)

Thirteen tools exposed to external MCP clients (e.g. Claude Code, Cursor) that let a
remote AI agent observe and interact with a TUICommander terminal, plus read/write/run
files in the session's sandboxed repo. All input and mutating
operations (`send_input`, `send_key`, `drive_agent`, `write_file`, `edit_file`, `run_command`) require user confirmation and are
rejected while an internal agent loop is active on the target session.

**Session aliases** — Every tool that accepts a `session_id` also accepts a human-friendly alias (e.g. `tc-1`). Aliases are auto-assigned from the repo directory name: first letter of each segment joined + per-repo counter. `list_sessions` includes the `alias` field. Aliases reset on app restart.

**Gated by `ai_terminal_mcp_enabled` config flag (default `false`).** When the flag is off, these tools are hidden from `tools/list` (via `filtered_native_tools`) and calls are rejected at dispatch time. Enable in `config.json` or Settings > Services. Note: no live-reload — a connected client may see a stale tools snapshot until it reconnects or `notifications/tools/list_changed` fires.

| Tool | Params | Description |
|------|--------|-------------|
| `ai_terminal_read_screen` | `session_id`, `lines?` (default 50, max 500), `since_cursor?` | Read terminal text. Returns `{screen, cursor, shell_state, awaiting_input, agent_intent?, agent_type?}` — `shell_state` is `busy` while the agent works (a spinner means busy, not idle), `idle` once stopped; `awaiting_input` is true when blocked on a question. Pass `since_cursor` for delta mode. Output passes through secret redaction. |
| `ai_terminal_send_input` | `session_id`, `text` | Send a text command to the session. Always prompts for confirmation. |
| `ai_terminal_send_key` | `session_id`, `key` (enter/tab/ctrl+c/escape/up/down/…) | Send a single special key. Always prompts for confirmation. |
| `ai_terminal_wait_for` | `session_id`, `pattern?`, `timeout_ms?` (10000), `stability_ms?` (500) | Wait for a regex match or for the screen to stabilise. |
| `ai_terminal_get_state` | `session_id` | Return structured `SessionState` (shell_state, cwd, terminal_mode, agent_type, …). |
| `ai_terminal_get_context` | `session_id` | Cheap orientation: `{shell_state, cwd, git_branch, last_exit_code, agent_type, terminal_mode}`. Git branch read from `.git/HEAD` (no subprocess, no index lock). |
| `ai_terminal_drive_agent` | `session_id`, `command?`, `timeout_ms?` (30000), `wait_pattern?`, `lines?` (80), `since_cursor?` | Atomic send→wait→read. Sends command, waits for idle/pattern, returns `{screen, cursor, shell_state, session_state}`. Pass `since_cursor` for delta mode. Requires user confirmation. |
| `ai_terminal_read_file` | `session_id`, `file_path`, `offset?`, `limit?` (default 200, max 2000) | Read a text file from the session's sandboxed repo. Paginated; binary files and files >10MB rejected. Secrets redacted. |
| `ai_terminal_write_file` | `session_id`, `file_path`, `content` | Create or overwrite a text file. Always prompts for confirmation. Atomic via tmp+rename. |
| `ai_terminal_edit_file` | `session_id`, `file_path`, `old_string`, `new_string`, `replace_all?` | Surgical search-and-replace on a file. Always prompts for confirmation. `old_string` must be unique unless `replace_all=true`. |
| `ai_terminal_list_files` | `session_id`, `pattern`, `path?` | List files matching a glob pattern inside the session's sandbox. Max 500 entries. |
| `ai_terminal_search_files` | `session_id`, `pattern`, `path?`, `glob?`, `context_lines?` | Regex search across files in the session's sandbox. Honors `.gitignore`. Max 50 matches with context lines. |
| `ai_terminal_run_command` | `session_id`, `command`, `timeout_ms?`, `cwd?` | Run a shell command and capture stdout/stderr. Always prompts for confirmation. Destructive commands blocked. Default timeout 2min, max 10min. |

### MCP Tool: `debug` — `invoke_js` and the Debug Registry

`invoke_js` executes JavaScript in the WebView (localhost-only). Results are logged with `source='eval_js'` and read via `debug(action='logs', source='eval_js', limit=1)`.

**`window.__TUIC__` bridge** — runtime introspection API:

| Method | Description |
|--------|-------------|
| `stores()` | List all registered store snapshot names |
| `store(name)` | Get a store snapshot by name |
| `plugins()` | All plugin states (legacy) |
| `plugin(id)` | Single plugin state with manifest (legacy) |
| `pluginLogs(id, limit?)` | Plugin log entries (legacy) |
| `terminals()` | All terminal states (legacy) |
| `terminal(id)` | Single terminal state (legacy) |
| `agentTypeForSession(sid)` | Agent type lookup (legacy) |
| `activity()` | Activity center sections/items (legacy) |
| `logs(limit?)` | App log entries (legacy) |

**Registered stores** (via debug registry): `github`, `globalWorkspace`, `keybindings`, `notes`, `paneLayout`, `repositories`, `settings`, `tasks`, `ui`. New stores self-register — see `src/stores/debugRegistry.ts`.

**Adding a new store snapshot** — 2 lines at the end of the store file:
```ts
import { registerDebugSnapshot } from "./debugRegistry";
registerDebugSnapshot("storeName", () => ({ /* fields to expose */ }));
```

### MCP Tool: `session` Output

The `session` tool's `action=output` strips ANSI escape codes by default, returning clean text suitable for AI consumption. Pass `format="raw"` to preserve escape sequences (e.g. for terminal rendering). For managed peers, task results travel through `agent action=send`; raw session output is only the anomaly fallback when a child failed to send its result. The `action=list` response includes process details per session: `child_pid`, `foreground_pgid`, `foreground_process`, `shell_state`, `agent_state`, `background_work`, and `is_caller`. `is_caller=true` identifies the managed PTY that owns the current MCP connection so an orchestrator does not close itself. Optional values such as alias, display name, cwd, worktree data, process identity, and agent state are omitted when absent rather than serialized as `null`; `status` follows the same omission rule.

`Global overview: session action=list` — one call; no per-session `status` fan-out.

`shell_state` is PTY activity (`busy` or `idle`); it is not task completion.
For detected agents, `agent_state` is `starting`, `working`, `awaiting_input`,
`idle`, or `completed`. `background_work=true` keeps `agent_state=working` while
a meaningful agent descendant is alive even when `shell_state=idle` and the
composer is ready; persistent integration helpers are excluded. Completion
requires the explicit end-of-task
`suggest: [ ... ]` protocol marker. A quiet ready prompt without that marker
remains `idle`. Spawned-agent lifecycle mail uses `completed` for the same
marker and reserves `idle` for an unclassified ready state.

| Param | Default | Description |
|-------|---------|-------------|
| `limit` | `8192` | Max bytes to read |
| `format` | (text) | `"raw"` preserves ANSI escape codes |
| `since_cursor` | (none) | Cursor from a previous response — returns only new scrollback lines since this position |

`session action=input` and HTTP `POST /sessions/:id/write` share the same PTY
bookkeeping: each write stamps `last_input_ms` and feeds the `InputLineBuffer`
so slash-mode tracking stays identical for MCP and remote web clients. When a
combined text + Enter request targets a prefill-only agent such as Codex or
OpenCode, MCP uses the canonical agent-submit sequence (Ctrl-U, bracketed paste
for multiline text, a flushed scheduling gap, then CR). Other text/key pairs,
including Claude's established input path, retain raw pair semantics.

**Delta reads:** The non-raw output path returns a `cursor` field (monotonic scrollback position). Pass `since_cursor` on subsequent calls to receive only new lines since that position, avoiding full re-reads. The `total_written` field is kept alongside `cursor` for backwards compatibility. When `since_cursor` is provided, screen rows are excluded — only scrollback log lines are returned.

### MCP Tool: `repo` — Worktree Create (Claude Code Agent Hint)

MCP `repo action=worktree_create` uses the same creation path as HTTP
`POST /worktrees`, including `base_repo` validation, stale-worktree recovery,
cache invalidation, `worktree-created` SSE/Tauri events, and setup-script result
reporting.

When the MCP client identifies as Claude Code (detected via `clientInfo.name` at initialize time), the `repo action=worktree_create` response includes an additional `cc_agent_hint` field:

```json
{
  "worktree_path": "/path/to/repo__wt/feature-branch",
  "branch": "feature-branch",
  "cc_agent_hint": {
    "worktree_path": "/path/to/repo__wt/feature-branch",
    "suggested_prompt": "Work in the worktree at `/path/...`. Use absolute paths for ALL file operations..."
  }
}
```

This works around Claude Code's inability to change its working directory mid-session. The hint tells CC to spawn a subagent that uses absolute paths for all file operations (Read, Edit, Glob, Grep) and `cd <path> && ...` for shell commands.

Non-Claude Code MCP clients do not receive this field.

### MCP Tool: `repo` — Worktree Remove

MCP `repo action=worktree_remove` returns `{ "ok": true }` on full success. When `delete_branch=true` and safe branch deletion fails after the worktree is removed, the action still succeeds with `branch_delete_warning` populated so clients can report that the worktree was removed but the branch was kept.

## Upstream MCP Proxy

TUICommander can proxy upstream MCP servers (stdio or HTTP) and aggregate their tools into its own `tools/list` response. Configuration lives in `mcp-servers.json`.

The desktop boot thread owns the always-on Unix-socket/named-pipe listener and
its one-time background tasks for the lifetime of the process. A configuration
save may stop and replace the TCP listener, but the boot runtime remains parked
after that shutdown so dropping it cannot silently kill local bridge IPC.

### Stdio transport

`StdioMcpClient` spawns a child process and communicates via newline-delimited JSON-RPC over stdin/stdout. The handshake is: `initialize` → `notifications/initialized` → `tools/list`.

**RPC id-matching.** The `rpc()` method matches responses by JSON-RPC `id`, skipping any server notifications (messages without an `id` field) that arrive between request and response. This prevents silent "0 tools" when a server emits `notifications/tools/list_changed` or log messages during the handshake.

**Tilde expansion.** All user-supplied paths (`command`, `args`, `cwd`) are expanded via `crate::cli::expand_tilde()` before being passed to `std::process::Command`. This applies globally across the codebase — PTY, agent spawn, headless prompts, worktree scripts, plugin exec, and file validation all expand `~` to `$HOME`.

### HTTP transport

`HttpMcpClient` communicates via Streamable HTTP (POST to the server URL, `mcp-session-id` header for session affinity).

**Bearer token caching.** The resolved bearer token (from OS keyring) is cached in memory after the first `resolve_bearer()` call. Subsequent calls (health checks every 60s, tool calls) use the cache. The cache is invalidated on 401 → `force_refresh()` and re-populated after a successful token refresh. This eliminates repeated macOS keychain permission prompts.

### Health checker

A background task runs every 60s (`HEALTH_CHECK_INTERVAL`) and calls `tools/list` on every `Ready` upstream. Failures feed a circuit breaker (3 consecutive failures → backoff starting at 1s, capped at 60s, max 5 retries before permanent `Failed`). Recovery from `CircuitOpen`, `Connecting`, or `Failed` is attempted on each tick.

### Diagnostics

Both transports log `warn!` when `tools/list` returns a response without `result.tools` — making "0 tools" diagnosable instead of silent.

## OAuth 2.1 Upstream Authentication

When an upstream MCP server requires OAuth instead of a static Bearer token, TUICommander runs a full RFC 9728 (Protected Resource Metadata) + RFC 8414 (Authorization Server Discovery) flow with PKCE S256.

### Configuration

`UpstreamMcpServer.auth` is an enum:

```rust
enum UpstreamAuth {
    Bearer  { token: String },
    OAuth2  {
        client_id: String,
        scopes: Vec<String>,
        authorization_endpoint: Option<String>,  // None → discover
        token_endpoint: Option<String>,          // None → discover
    },
}
```

Missing endpoints trigger metadata discovery: the proxy issues an unauthenticated probe, follows the `WWW-Authenticate: Bearer resource_metadata=<url>` challenge to fetch `ProtectedResourceMetadata`, then resolves the authorization server's `.well-known/oauth-authorization-server` (falling back to OIDC `.well-known/openid-configuration` when required).

### Error → flow transition

`src-tauri/src/mcp_proxy/http_client.rs` emits a typed error:

```rust
enum UpstreamError {
    NeedsOAuth { www_authenticate: String },
    AuthFailed,
    Other(String),
}
```

A `NeedsOAuth` on any request transitions the upstream registry to `needs_auth`. The Services tab in Settings surfaces an *Authorize* button that calls `start_mcp_upstream_oauth`. Auto-triggered OAuth is gated behind explicit user consent (the confirm dialog shows the AS origin so the user can refuse an Authorization Server mix-up attempt).

**Off-domain authorization servers are never blocked.** MCP gateways, corporate proxies and hosted IdP tenants routinely serve AS metadata whose `issuer` and endpoints point at a different registrable domain than the MCP server — RFC 8414 §3.3 says the issuer must match the discovery URL, but refusing on that basis makes legitimate servers unusable. Discovery logs a warning on an issuer mismatch and continues; `start_mcp_upstream_oauth` returns `cross_domain_as: true` when the AS is off-domain, and the consent dialog switches to a `warning` kind naming the origin. The decision belongs to the user, not to a hard-coded gate.

### Flow

1. **Start** — `start_mcp_upstream_oauth(name)` generates a PKCE verifier/challenge (S256), mints an opaque `state`, records the pending flow in a DashMap keyed by state, sets upstream status to `authenticating`, and returns the authorization URL + AS origin.
2. **Consent UI** — The frontend opens the URL via `tauri-plugin-opener` after user approval. The status bar and Services tab show "Awaiting authorization…".
3. **Callback** — The AS redirects to `tuic://oauth-callback?code=…&state=…`. The OS routes the deep link to the desktop app (`src-tauri/src/mcp_oauth/mod.rs` — `DEEP_LINK_SCHEME = "tuic://oauth-callback"`). The deep-link handler calls `mcp_oauth_callback(code, oauth_state)`.
4. **Exchange** — `TokenManager` posts code + PKCE verifier to the token endpoint, receives `{ access_token, refresh_token?, expires_in? }`, serializes into `OAuthTokenSet`, persists to the OS keyring (`mcp_upstream_credentials.rs` — structured JSON format with `"type": "oauth2"`), and transitions upstream to `connecting`.
5. **Refresh** — `TokenManager` is shared across every `HttpMcpClient` refresh path (unified per upstream); a semaphore serializes concurrent refresh attempts to defeat thundering-herd. `expires_at` uses a 60 s margin; `None` means "no known expiry — do not treat as expired".

### Cancel

`cancel_mcp_upstream_oauth(name)` drops the pending flow entry and resets the upstream status to whatever it was before the attempt (`disconnected` / `failed` / `ready`).

### Deep-link scheme

| Scheme | Purpose |
|--------|---------|
| `tuic://oauth-callback?code=…&state=…` | OAuth 2.1 authorization code return path for upstream MCP servers |

Registered at boot via Tauri's single-instance + deep-link plugins. The frontend listener routes callbacks to `mcp_oauth_callback` without exposing the code to the WebView console.

### Threat model

OAuth callbacks arrive exclusively through the OS-level `tuic://` deep link — not over the network. There is no adversary position from which a remote attacker can probe the pending-flow map, so state comparison uses a direct DashMap lookup (no constant-time compare). The localhost dev callback server (used only in development) binds `127.0.0.1` with a random port; it is never exposed in production builds.

## Inter-Agent Messaging

The `agent` tool's messaging actions (`register`, `list_peers`, `send`, `inbox`) enable coordination between multiple AI agents connected to TUICommander.
There is no separate `swarm` action; orchestration composes the `agent` and `session` primitives.

For `agent action=spawn`, `prompt` is always delivered. Caller-supplied `args`
that contain `{prompt}` remain authoritative and receive direct substitution.
Flags-only `args` keep their order; normal CLIs receive the prompt as the final
positional argument, while prefill-only interactive TUIs receive it through the
deferred PTY-injection path after their ready prompt appears.
Configured run-config argv retains its established authoritative behavior:
`{prompt}` is substituted where authored, otherwise the prompt is appended as
the final positional argument rather than converted to deferred PTY delivery.
Structured `model` is composed with `args`; direct Codex commands include the approval-bypass default. Outside authoritative run-config argv, direct executable identity also selects Codex prompt deferral and parser state, even when `agent_type` is omitted or disagrees. That bypass-default step leaves canonical Codex wrapper run-config argv untouched and adds `launch_warning` because TUIC cannot validate the wrapper's internal Codex flags. Structured parameters retain their established composition independently, including appending a caller-supplied `model`.

`name` optionally assigns a non-empty peer and PTY display name at spawn time.
The parent-assigned name is stored before prompt delivery, returned in the spawn
response, exposed as `name` by `agent action=list_peers` and as `display_name`
by `session action=list`, and preserved when the child later auto-binds its MCP
connection. This avoids making identity depend on the child successfully
executing a registration instruction in its initial prompt.
The session list's `alias` remains a separate repo-derived short address and is
not replaced by the display name.

Every managed child is registered server-side and receives an inbox immediately,
even when the caller has no bound peer identity. A registered parent additionally
creates the bidirectional relationship: the child prompt receives its parent ID
and send instruction, while the spawn response returns `communication_ready`,
`send_to`, and `parent_session_id`. An unregistered caller receives
`communication_ready=false` plus a warning instead of a false two-way guarantee.
The spawn still records the caller's MCP session as a pending parent: a later
`register` call links existing children to the stable parent UUID and migrates
any lifecycle notifications emitted before registration.

Deferred initial prompts use a one-shot internal watchdog. Successful PTY
submission removes the marker silently. A prompt still pending after 30 seconds
emits one `prompt_delivery_failed` message to the parent; there is no success
event, delivery polling, or public delivery-state machine.

Ordinary managed-agent PTY injection is allowed only when the recipient is idle,
its composer buffer is empty, and no confident question or approval is active.
Busy workers and recipients with partially typed input keep the message queued.
Clearing or submitting the composer rechecks the queue.

An orchestrator role is declared explicitly with
`agent action=register orchestrator=true` and removed with `orchestrator=false`; spawning a child never
infers or permanently grants the role. The declaration is returned by `register`
and `list_peers`. Its routing is intentionally stricter: every peer and
child-lifecycle message remains in the authoritative inbox, and peer payloads
never enter its channel, active turn, pending-injection queue, or composer. An
active `agent wait` owns delivery and suppresses terminal wake. Without a waiter,
only canonical `idle` or `completed` lifecycle may submit the payload-free notification
`[TUIC] message available — read it with: agent action=inbox`. Working,
awaiting-input, starting, missing, and unknown state fail closed to inbox-only.
Mail received while working remains eligible and is re-evaluated at the next
authoritative idle/completed transition. One pending wake covers later unread
mail through its logical inbox cursor. Inbox and successful wait observations
acknowledge the same cursor atomically with their snapshot, including an empty
snapshot, so a read cannot race a delayed wake assignment. A generic notice never
hides the underlying payload from a later wait. PTY I/O happens outside the
delivery gate. An ambiguous payload-free notice expires after five seconds and
may be retried once after the managed lifecycle reconfirms readiness. A second
uncertain result exhausts that unread-mail group's wake budget: later expiry or
idle/completed reevaluations, including newly coalesced mail, remain inbox-only
until a successful inbox or wait observation acknowledges the group. An attempt
that writes no bytes remains `NotStarted` and does not enter an automatic retry
loop.

`register` and `list_peers` also return `mail_wake`. Its only current non-`none`
value is `managed_pty_lifecycle`, derived from a live TUIC-managed PTY rather than
claimed by the caller. Headerless/external orchestrators have a mailbox and MCP/SSE
transport but no authoritative model lifecycle or host wake adapter capable of
starting a turn, so they honestly report `mail_wake: "none"` and must use
`agent wait`/`inbox`. MCP activity and SSE presence are not treated as idle proof.

The final injection decision atomically claims `idle -> busy`, closing the race
between observing a ready screen and writing to the PTY. Idle is published before
the queued-message flush, and each idle transition submits at most one queued
message; remaining messages wait for later turns. This keeps backend state and UI
events ordered and prevents lifecycle reports from overwriting an active composer.

The stdio bridge reads each IPC HTTP response through its declared
`Content-Length` rather than waiting for connection EOF. A single transport error
does not discard the current MCP identity; subsequent authenticated calls refresh
the session-to-terminal binding. Ordinary calls keep a ten-second read deadline;
direct and collapsed wait calls derive it from the clamped requested timeout plus
a five-second transport margin.

Focused `ui action=tab` requests using `tuic://open` or `tuic://edit` switch to
the registered repository that owns an absolute target path before activating
the native file tab. This keeps repo-scoped tabs visible in the tab bar instead
of rendering their content under an unrelated active repository. Background
requests (`focus=false`) do not change repository context.

### Protocol

0. **Auto-identity** *(no call needed)*: TUICommander's Codex MCP entry explicitly whitelists
   `TUIC_SESSION` through `env_vars`; other supported clients inherit it from the agent PTY.
   `tuic-bridge` sends the value as the `x-tuic-session` header on the initialize `POST /mcp`. The server
   validates the UUID and binds the MCP session to that tuic session (`apply_initialize_identity`
   → the shared locked live-owner policy), auto-registering the peer. `agent action=register`
   becomes an optional rename. The same MCP session may refresh its binding, and a fresh session
   may reclaim a stale owner; a subscribed or recently active owner is not replaced but is joined,
   so a second bridge in the same PTY becomes routable instead of being locked out. An existing
   peer's display name is preserved. The bridge's eager initialize and the downstream client's
   proxied initialize reuse the same existing `mcp-session-id`; this prevents the bridge's own live
   SSE stream from being mistaken for a competing identity owner. External bridges without
   `$TUIC_SESSION` are not auto-bound at initialize.
1. **Register**: optional rename/project/role update for an auto-bound peer. Pass
   `orchestrator=true` to declare the role or `false` to remove it; omission
   preserves the current declaration. A headerless external
   caller may omit `tuic_session`; the server generates an MCP-scoped UUID that remains stable for
   that connection and does not create a PTY. Supplying an explicit UUID preserves identity across
   reconnects and retains the live-owner takeover guard, which now refuses only callers that hold
   no route to the identity — a bridge already joined to it is renaming, not taking over.
   When the announced UUID differs from the one already bound to the MCP session, the two are
   ranked by whether they resolve to a live PTY rather than merely compared: an identity backed by
   a terminal outranks one that is not. A caller that registered an invented UUID may therefore
   repair itself by announcing its real `$TUIC_SESSION`, while the reverse — wandering off a
   terminal-backed identity onto a fabricated one — is refused with an error naming the identity to
   use. Two identities that both lack a terminal keep the original "already bound to a different
   peer identity" rejection. A repaired identity carries over any mail buffered under the abandoned
   one and retires it, so `list_peers` stops advertising an address that can never be reached. The
   retire and the recipient check inside `send` share one identity lock, so a message aimed at the
   abandoned identity either arrives before the retire and is carried over with the rest of the
   inbox, or arrives after it and is refused with "is not registered" — it is never buffered under
   an address that is deleted a moment later.
2. **Discover**: `agent action=list_peers` returns all registered peers (filterable by project).
3. **Send**: `agent action=send to=<tuic_session> message="..."` buffers to the recipient's inbox.
   `accepted=true` and `buffered_in_inbox=true` acknowledge success;
   `delivered_via_channel` describes only the optional SSE path, while
   `delivery_path` distinguishes SSE, terminal-or-queued, waiter, generic/coalesced
   orchestrator wake, and inbox-only delivery. When the recipient is a
   real managed PTY, `recipient_state` contains only its current `shell_state` and `agent_state`;
   external generated peers omit `recipient_state`.
4. **Receive** — three layers, most-immediate first:
   - **Channel push**: real-time `notifications/claude/channel` only when an ordinary managed Claude Code recipient already has a working turn and holds an SSE stream (CC + channels flag). A managed non-Claude worker, or an idle/completed Claude worker, uses PTY delivery even if its MCP bridge has an SSE stream. Registered orchestrators never receive peer payloads through this channel.
   - **PTY injection**: for an ordinary idle or completed managed agent, the message is *typed into its terminal* (framed single line; split write, Ink-safe) so it submits a real next turn without polling. A busy ordinary recipient without active Claude channel support gets the message on its next BUSY→IDLE transition. Oversized (>2 KB) bodies inject a pointer to `agent action=inbox` instead. An idle/completed orchestrator receives only the generic inbox wake described above; a busy orchestrator is never queued or steered.
   - **Inbox poll**: `agent action=inbox since=<ms>` — always the authoritative store.
5. **Wait** *(prefer over polling)*: `agent action=wait since=<ms>` blocks until new mail;
   `session action=wait session_id=<id> until=idle|exited` blocks on a peer's lifecycle. The default
   is 60 seconds and the server cap is 300000 ms. Agent-wait success preserves
   `{met,timed_out,new_messages}` and directly includes every retained fresh message (up to the
   100-message inbox capacity) plus `next_since`, in chronological order. Per-recipient logical
   unix-millisecond cursors make equal-clock-millisecond bursts safe. Wait never consumes the
   authoritative inbox; actual FIFO eviction is still reported by `missed_count` on inbox reads.
   Both wait actions subscribe before their initial state check and then sleep on inbox or
   per-session lifecycle events; they do not run an internal polling loop.
   `session action=wait` validates `session_id` against the live session registry first and
   returns `{"error": "Unknown session …"}` immediately for an id that is not a real session —
   subscribing creates the per-session broadcast channel for whatever id it is given, and
   teardown only reaps ids that were real sessions.

Low-risk response compaction also omits an absent peer `project` from `list_peers` and an absent
`parent_session_id` from standalone spawn responses. Proxied upstream tool payloads are unchanged.

Blocking waits and terminal wake-up use a per-recipient delivery lease. Each
message is atomically assigned to exactly one wake-up owner: an active waiter,
or SSE/PTY delivery. The deadline path performs its final inbox check while
releasing the lease, and cancellation hands unobserved waiter-owned messages
back to terminal delivery. This removes both duplicate inbox+terminal turns and
the missed-wake race at the wait timeout boundary; inbox visibility itself is
unchanged and remains backward compatible.

The server never infers orchestrator role from child spawn, peer name, prompt, MCP
activity, or SSE presence. Registration is the sole declaration seam. Wake
capability remains server-derived: without a live managed PTY and its canonical
lifecycle, an explicitly declared external orchestrator is inbox/wait-only.

Spawned peers additionally auto-post a `state_change` (`idle` / `completed` / `exited`) to the
parent's inbox. They use the same waiter-or-generic-wake orchestrator routing and never inject the
state payload into the parent composer. These notifications carry state only, never task
output. Each child must send its result or blocker with `agent action=send`; `session action=output`
is reserved for diagnosing the anomaly where that result message never arrived.

### Channel Push Delivery

When an already working ordinary Claude Code worker has an active SSE stream (`GET /mcp`), messages are pushed into that turn as `notifications/claude/channel` JSON-RPC notifications. Idle or completed ordinary managed recipients use PTY submission instead. Registered orchestrators never use this payload-bearing route:

A channel notification is transport delivery into an existing turn, not proof that the recipient submitted a new one. It does not mutate the recipient's task epoch or lifecycle. Managed Codex and other non-Claude agents never receive this extension; an idle or completed Claude composer also takes the PTY split-write payload plus Enter path so delivery owns a real submitted turn.

```json
{
    "jsonrpc": "2.0",
    "method": "notifications/claude/channel",
    "params": {
        "content": "Message from worker-1: done with auth module",
        "meta": { "from_tuic_session": "abc-123", "from_name": "worker-1", "message_id": "msg-uuid" }
    }
}
```

This requires the client to be launched with `--dangerously-load-development-channels server:tuicommander`. The server declares `experimental.claude/channel` in its capabilities. Spawned Claude Code agents get this flag automatically.

### Limits

- Max message size: 64 KB
- Inbox capacity: 100 messages per agent (FIFO eviction)
- Peer registrations cleaned up on MCP session delete and TTL reap

## Authentication

When remote access is enabled:
- Basic Auth with username/password
- Password stored as bcrypt hash in config
- Session token, relay token, and VAPID private key stored in the OS keyring-backed credential vault
- Applied to all endpoints

When MCP-only (localhost):
- No authentication required
- Localhost binding only

## Security Model

- **Default:** Localhost-only, no authentication, opt-in
- **Remote access:** Configurable port, Basic Auth required
- **CORS:** Enabled for all origins (browser mode support)
- **Compression:** Gzip and Brotli via `CompressionLayer` (responses >860 bytes, auto-negotiated). SSE and WebSocket excluded by `DefaultPredicate`
- **No TLS:** Intended for local network use; use SSH tunnel for remote
- **Loopback-only session actions:** `session create`, `input`, `kill`, `close`, `pause`, and `resume` are restricted to loopback connections — a non-loopback (remote/LAN) MCP client cannot pause/resume sessions, write to PTYs, or spawn/destroy sessions (those remain read-only: `list`, `output`, `status`)
- **Remote `/fs/read-editor*` cap:** Remote clients receive the standard 10 MB file-read cap on `/fs/read-editor` and `/fs/read-editor-external`, not the 250 MB local cap (`MAX_EDITOR_LARGE_FILE_SIZE`). The local (loopback) router routes these paths to the large-cap handler; the remote router routes them to the standard-cap handler to avoid OOM/latency over metered links (see `build_remote_router` in `src-tauri/src/mcp_http/mod.rs`)
- **Traversal gate on absolute-path fs routes:** `/fs/read-external`, `/fs/read-editor-external`, `/fs/write-external`, `/fs/copy-abs`, `/fs/move-abs` and `/fs/transfer` (its `destDir`) share `deny_unless_in_roots`, which rejects `..`, NUL and relative paths before the lexical `Path::starts_with` containment check. Without that first layer, `/repo/../../etc/passwd` passes containment by components while the OS resolves it outside the repo. These routes are in `shared_routes()`, so they are reachable from the remote router too. Paths are intentionally not canonicalized — symlinks placed inside a registered repo are an accepted design decision
- **Anti-hijack guard on `agent register`:** A non-loopback caller cannot register as an existing live TUIC session — the `register` action (along with `list_peers`, `send`, `inbox`) is restricted to loopback connections, preventing a remote client from injecting messages into another agent's context (see `mcp_transport.rs`)

## Browser Mode Integration

The frontend's `transport.ts` maps all Tauri commands to HTTP endpoints:

```typescript
// In browser mode:
invoke("create_pty", { config }) → POST /sessions { config }
invoke("get_repo_info", { path }) → GET /repo/info?path=...
```

PTY output in browser mode uses WebSocket instead of Tauri events.

### GitHub Ops AI Routes

Desktop/browser mode exposes the GitHub Ops AI helpers over HTTP; the remote
daemon does not serve them because they depend on desktop provider credentials.

| Endpoint | Body | Response | Notes |
|----------|------|----------|-------|
| `POST /ai/improvements/scan` | `{ repoPath, focus }` | `ImprovementScanResult` | One-shot Headless-slot LLM scan over local repo context (`focus`: `refactor`, `testing`, `perf`). Dual-emits `proposals-ready` to the window and `/events` SSE. |
| `POST /repo/create-issue-from-proposal` | `{ repoPath, proposal }` | `CreatedIssue` | Explicit user-gated issue creation from one proposal; scan itself never creates issues. |

## Mobile Transport

The mobile companion UI (`/mobile`) uses the same HTTP/WebSocket infrastructure as the desktop browser mode:

- **Session polling**: `GET /sessions` every 3s, enriched with `SessionState` (question, rate-limit, busy, agent type)
- **Real-time events**: SSE via `GET /events` for session create/close notifications
- **Live output**: WebSocket to `/sessions/{id}/stream` with JSON framing (`output`, `parsed`, `exit`)
- **Input**: `POST /sessions/{id}/write` sends text to PTY (used by quick-reply chips and command input)
- **History**: `GET /sessions/{id}/output?format=text` fetches initial ANSI-stripped output buffer

The mobile entry point shares `transport.ts` and `invoke.ts` with the desktop — no mobile-specific transport code.
