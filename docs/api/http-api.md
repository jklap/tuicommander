# HTTP API Reference

## Project Progress

`POST /progress/report?path=<absolute-project-path>` accepts `{ type, summary,
workstream? }`. `type` is `started`, `milestone`, `blocked`, or `done`. The
response is a bounded `{ status, revision, eventId? }` receipt. Recorded
events are published as `progress-recorded` on `/events`; duplicate and paused
reports emit nothing.

REST API served by the Axum HTTP server when MCP server is enabled. All Tauri commands are accessible as HTTP endpoints.

## Base URL

- **Local (Unix socket):** `<config_dir>/mcp.sock` — always started on macOS/Linux. No auth, MCP always enabled. Used by the local MCP bridge binary.
- **Remote (TCP):** `http://<host>:{remote_access_port}` — only started when remote access is enabled in settings. HTTP Basic Auth required.

## Authentication

- **MCP mode (localhost):** No authentication
- **Remote access mode:** HTTP Basic Auth with configured username/password, or
  the `?token=` session token the QR code embeds.

Both halves of the credential pair are required: `validate_basic_auth`
(`mcp_http/auth.rs`) reports `NotConfigured` when either the username or the
password hash is empty, and the server then answers **every** Basic Auth attempt
with `401` and the body `Scan the QR code or authenticate with Basic Auth` — the
same response a request with no `Authorization` header gets. A 401 carrying that
body while credentials were sent means the pair is incomplete, not wrong.

## Server Limits

Every route on both the desktop and remote routers is subject to two bounds
(`with_server_limits` in `mcp_http/mod.rs`):

- **`408 Request Timeout`** — a handler that has not produced a response within
  301 s is cut off. This includes slow COW creation with warm build artifacts
  while still bounding a wedged handler. 301 s is a deliberate 1 s margin above
  `ui action=confirm`'s own 300 s answer window, so that handler's own timeout
  body always wins the race instead of a bare 408 (`docs/backend/mcp-http.md` →
  "Server Limits"). SSE (`/events`) and WebSocket endpoints return their
  headers immediately and then stream for as long as they like, unaffected.
- **`413 Payload Too Large`** — a request body over 2 MB is refused rather than
  buffered.

See [`docs/backend/mcp-http.md`](../backend/mcp-http.md) → "Server Limits" for
the rationale and the tests that pin both.

## Unknown Paths

The desktop server also serves the frontend, so any path that matches no route
falls through to a catch-all. That catch-all splits on the first path segment:

- **API path** — the first segment is one of `agent`, `agents`, `ai`, `api`,
  `audio`, `claude`, `codex`, `config`, `debug`, `diagnostics`, `dictation`,
  `events`, `exec`, `fs`, `generators`, `github`, `health`, `logs`, `mcp`,
  `metrics`, `plugins`, `process`, `prompt`, `registry`, `repo`, `sessions`,
  `stats`, `system`, `terminal`, `tunnels`, `watchers`, `worktrees`. The
  response is `404` with `{ "error": "no such endpoint: /<path>" }`.
- **Anything else** — a deep link such as `/settings` or `/mobile/session/<id>`.
  The response is `200` with the SPA shell (`index.html`, or `mobile.html` under
  `/mobile`), so client-side routing takes over.

A registered path called with the wrong method still returns `405`, not `404`.
The catch-all is the router's fallback, so it answers on **every** method: an
unregistered path returns `404` whether it was reached with `GET`, `POST` or
anything else. It used to be a `GET`-only route, which made any non-`GET` call
to a path that does not exist answer `405` — a reply that claims the path is
real. The parity gate below depends on telling those two apart.

Without this split an unregistered API path answered `200` with HTML, which the
client read as success and then failed to parse as a command result — every
missing route looked like a malformed response. The prefix list is checked
against the registered routes by a test, so a new route family cannot silently
drop back to the HTML answer.

The `tuic-remote` daemon embeds no frontend and has no catch-all: unknown paths
there return a bare `404`.

## Route Parity Gate

Every `COMMAND_TABLE` entry in `src/transport.ts` must resolve to a registered
route. Two tests enforce it, one per language:

1. `src/__tests__/transport.test.ts` executes every mapper and snapshots the
   resulting paths to `src-tauri/src/mcp_http/command_table_paths.txt`.
2. `mcp_http::tests::command_table_paths_all_hit_a_registered_route` reads that
   file and `PATCH`-probes each path against `build_router`. `PATCH` is used
   because no route accepts it, so a registered path answers `405` without
   running its handler, while an unregistered one falls through to the
   catch-all. A `404` or an HTML body fails the test.

Adding a command therefore fails the Vitest half first (stale snapshot).
Regenerate with:

```bash
pnpm vitest run src/__tests__/transport.test.ts -u
```

If the route was never registered, the Rust half then fails. Both run in
`make check`. The gate proves the **path** exists, not that it accepts the
method the table declares — probing the declared method would execute the
handler, which is what the technique avoids.

`INTENTIONALLY_UNMAPPED` commands never enter the snapshot: they are not
`COMMAND_TABLE` entries, and the Vitest half asserts the two sets stay disjoint.

## Session Endpoints

### List Sessions

```
GET /sessions
```

Returns array of active session info (ID, cwd, worktree path, branch,
`display_name`, `display_name_is_custom`, `is_remote`, optional
`pty_description`, and nested state). The
origin fields let browser and desktop clients preserve manual-title protection
and remote-completion muting across reconnects. For detected agents,
`state.agent_state` distinguishes PTY
silence (`idle`) from explicit protocol completion (`completed`); the latter
requires a parsed `suggest: [ ... ]` marker. Other values are `starting`,
`working`, and `awaiting_input`. `state.background_work` is true when meaningful
non-helper descendants keep autonomous work alive despite an input-ready
terminal (`state.shell_state == "idle"`).

### Create Session

```
POST /sessions
Content-Type: application/json

{
  "rows": 24,
  "cols": 80,
  "shell": "/bin/zsh",    // optional
  "cwd": "/path/to/dir",  // optional
  "alias": "tu-1"         // optional — reclaim a persisted alias
}
```

Returns `{ "session_id": "..." }`.

`alias` lets a client restore the short address a tab had before a restart. The server
honours it only when it still has the `<prefix>-<number>` shape and no live session
holds it, and then raises the per-prefix counter past that number so the next
auto-assigned alias cannot collide. Anything else is ignored and the session receives a
freshly minted alias.

### Create Session with Worktree

```
POST /sessions/worktree
Content-Type: application/json

{ "pty_config": { ... }, "worktree_config": { ... } }
```

Creates a git worktree and a PTY session in one call.

### Spawn Agent Session

```
POST /sessions/agent
Content-Type: application/json

{ "pty_config": { ... }, "agent_config": { ... } }
```

Spawns an AI agent (Claude, etc.) in a PTY session.

### Write to Session

```
POST /sessions/:id/write
Content-Type: application/json

{ "data": "ls -la\n" }
```

### Write Several Inputs at Once

```
POST /sessions/:id/write-parts
Content-Type: application/json

{ "parts": ["/", "help", "\r"] }
```

One round trip and one PTY writer lock for the whole batch, but the parts stay
separate: the backend applies its post-write bookkeeping once per part. This is
**not** the same as joining them into `/write` — that bookkeeping reads each part
as one keystroke, so a lone `/` opens slash mode and an exact option key answers
a choice prompt, and a joined payload matches neither. The transport uses this
route when keystrokes arrive while an earlier write is still in flight; a
solitary keystroke keeps the plain `/write` route.

### Queue a Command for the Next Idle Window

```
POST /sessions/:id/queue
Content-Type: application/json

{ "text": "run the tests" }        -> { "typed": false, "queued": 2 }

GET /sessions/:id/queue            -> [ { "id": 7, "text": "run the tests" } ]

DELETE /sessions/:id/queue         -> 2   (commands dropped)

DELETE /sessions/:id/queue/:cmdId  -> true  (false when it already drained)
```

Hands the text to the same idle gate peer messages use instead of typing it now:
submitted immediately when the agent is idle (`typed: true`, `queued: 0`),
otherwise parked until the agent's next busy→idle transition, so a running turn
is never steered. User commands and peer messages share one typed FIFO and are
submitted one per idle window in backend acceptance order. `queued`,
`state.queued_commands`, and `DELETE` count or remove only user commands;
clearing Compose commands never deletes pending peer/orchestrator delivery.

Agent sessions only — `400` for a plain shell (`"Session is not running an
agent"`) or empty text, `404` when the PTY is gone. The current depth is also on
every session snapshot as `state.queued_commands` (omitted when zero).

### Resize Session

```
POST /sessions/:id/resize
Content-Type: application/json

{ "rows": 30, "cols": 120 }
```

### Read Output

```
GET /sessions/:id/output?limit=4096&format=text
```

Returns recent output. Format controls what is returned:

| `format` | Response shape | Description |
|----------|----------------|-------------|
| (omit) | `{ "data": "<string>", "data_length": N, "total_written": N }` | Raw PTY output as a lossy-UTF-8 string (not base64), read from the ring buffer |
| `text` | `{ "data": "<string>", "data_length": N, "total_written": N }` | One canonical terminal-grid snapshot, joined by `\n` (not from the ring buffer) |
| `log` | `{ "lines": [...], "total_lines": N, "screen": [...], "input_line"? }` | VT100-extracted clean lines (no ANSI, no TUI garbage) plus current screen rows and optional input line |

| Param | Default | Description |
|-------|---------|-------------|
| `limit` | raw: 8192 bytes; text/log: all | `raw`: max bytes; `text`/`log`: max lines to return |
| `offset` | (tail) | `text`/`log`: absolute start row/line offset. When omitted, returns the newest `limit` rows/lines. When provided, returns data starting from that offset |
| `format` | (raw) | See table above |

`format=log` reads from `VtLogBuffer` — a VT100-aware buffer that extracts only scrolled-off lines, suppressing alternate-screen TUI apps (vim, htop, claude). Ideal for mobile clients.

`format=text` is a point-in-time canonical grid view. It does not concatenate
the finalized log cursor with the visible screen, because growing a viewport can
move history rows back onto the screen and make that concatenation overlap.
`total_written` is the snapshot's total grid-row count for this format.

`total_lines` in the response is a monotonically increasing counter — it never decreases when old lines are evicted from the buffer. Use it as a stable cursor for paginated reads. The `offset` parameter operates in the same coordinate space.

### Kitty Protocol Flags

```
GET /sessions/:id/kitty-flags
```

Returns the current Kitty keyboard protocol flags (integer) for a session.

### Foreground Process

```
GET /sessions/:id/foreground
```

Returns the foreground process info for a session.

### PTY / Terminal Read State

```
GET  /sessions/:id/shell-state                         -> { "state": "busy"|"idle"|null }
GET  /sessions/:id/shell-family                        -> "posix"|"windows-native"|"unknown"|null
GET  /sessions/:id/last-prompt                         -> { "prompt": string|null }
GET  /sessions/:id/input-buffer                        -> { "content": string }
GET  /sessions/:id/leaf-pid                            -> { "pid": number|null }
GET  /sessions/:id/has-foreground                      -> { "process": string|null }
POST /sessions/:id/visible              { "visible": bool }   -> { "ok": true }
GET  /sessions/:id/terminal/selection-text?startRow=&startCol=&endRow=&endCol=  -> { "text": string }
GET  /sessions/:id/terminal/logical-line?row=N         -> [logicalStartRow, text]
GET  /sessions/:id/terminal/hyperlink-span?row=R&col=C -> [startCol, endCol, url] | null
GET  /sessions/:id/terminal/styled-rows?start=N&count=N -> application/octet-stream (packed rows)
GET  /process/stats                                    -> ProcessStats[]
```

`terminal/styled-rows` fills the CanvasTerminal client-side row cache and answers
**binary**, not JSON: a 64-row chunk is ~141 KB of packed cells, which as a JSON
number array becomes ~350 KB of decimal text for the client to parse back into the
bytes it started as. The desktop `terminal_styled_rows` command returns the same
payload raw (`tauri::ipc::Response`), and `rpcImpl` decides between
`arrayBuffer()` and `json()` on the content-type alone. An empty body means "no
such session or range" — a valid empty chunk, not an error.

`shell-family` classifies the session's shell so the client picks the right control
sequences (Ctrl-U is line-kill under POSIX readline, a literal character on
`cmd.exe`/PowerShell). It answers the bare value, **not** a `{field}` wrapper,
because `get_session_shell_family` returns `Option<ShellFamily>` bare over IPC and
`src/utils/sendCommand.ts` reads both transports with the same code. An unknown
session is `null`, which is the same "fall back to the host default" answer.

Read-only PTY/terminal state mirroring the desktop Tauri commands (story 062). The
`{field}`-wrapped responses are unwrapped by the frontend transport to match the
command's bare return (e.g. `Option<String>` → `null`). The desktop-only commands
themselves are absent from the remote binary, so these handlers read `AppState`
directly.

`terminal/selection-text` reads absolute scrollback coordinates, rejoins
soft-wrapped rows, trims terminal padding, and removes only coherent multi-line
Claude `NBSP NBSP ▎` visual gutter runs. Desktop IPC returns the same string.

### Pause/Resume

```
POST /sessions/:id/pause
POST /sessions/:id/resume
```

### Rename Session

```
PUT /sessions/:id/name
Content-Type: application/json

{ "name": "my-session", "isCustom": true }
```

Sets a display name and its origin. `isCustom: true` protects an explicit user
rename from subsequent OSC/intent titles; spawn-assigned and dynamic titles use
`false`. Omitting the field preserves the legacy custom-rename behavior.

### Close Session

```
DELETE /sessions/:id?cleanup_worktree=false
```

## Streaming Endpoints

### WebSocket PTY Stream

```
WS /sessions/:id/stream
```

Receives real-time PTY output as text frames. One WebSocket per session.

### WebSocket JSON Framing (Mobile/Browser)

WebSocket connections to `/sessions/:id/stream` receive JSON-framed messages:

```json
{"type": "output", "data": "raw terminal output text"}
{"type": "parsed", "event": {"type": "question", "text": "Allow?"}}
{"type": "watcher-lines", "session_id": "abc", "lines": [{"text": "clean line", "matched_ids": ["<client_id>/w0"]}]}
{"type": "exit"}
{"type": "closed"}
```

Frame types:
- `output` — Raw PTY output (ANSI-stripped when `?format=text`)
- `log` — VT100-extracted clean lines batch (when `?format=log`): `{"type":"log","lines":[...],"offset":N}`
- `parsed` — Structured events (questions, rate limits, errors) from the output parser
- `watcher-lines` — A batch of assembled PTY lines for the plugin OutputWatchers registered through `POST /api/plugins/output-watchers`. Sent on the raw stream and on `?format=grid`; `?format=log|text` does not carry it. Each entry is `{text, matched_ids}`: `text` is the cleaned line Rust matched on, so the client can run its own `RegExp` on it and obtain the capture groups; `matched_ids` are the qualified ids (`<client_id>/<watcher_id>`) of the watchers Rust matched. A client ignores the ids of other clients. While every registered pattern compiles, only the matched lines are sent; while one does not, every line is sent
- `exit` — Session process exited
- `closed` — Session was closed

#### WebSocket format=log

```
WS /sessions/:id/stream?format=log
```

When `?format=log` is specified, the connection streams VT100-extracted log lines instead of raw PTY chunks:
- On connect: sends all accumulated lines as a single catch-up frame
- While running: polls every 200ms and sends new lines batched by offset
- PTY input passthrough is still available (write text/binary frames to send to PTY)

#### WebSocket format=grid

```
WS /sessions/:id/stream?format=grid
```

The transport behind `CanvasTerminal` in browser/PWA mode. Binary frames carry the
serialised terminal grid; JSON text frames carry the side-channel events the canvas
needs, each one the desktop Tauri payload plus a `type` key:

```json
{"type": "osc133", "marker": "D", "line": 42, "exit_code": 0}
{"type": "cwd", "cwd": "/Users/me/project"}
{"type": "watcher-lines", "session_id": "abc", "lines": [{"text": "clean line", "matched_ids": ["<client_id>/w0"]}]}
```

- `osc133` — Shell-integration marker (`A` prompt, `B` command start, `C` output start,
  `D` command end). Drives command blocks, gutter marks and Cmd+Up/Down navigation.
  `exit_code` is `null` for every marker except `D`, and `null` on a `D` without one.
  The field is `exit_code`, not `exitCode` — it is serialised from the same Rust struct
  as the desktop `pty-osc133-{id}` event, and the two must not drift
- `cwd` — OSC 7 working-directory change. Same `{ cwd }` object as the desktop event
- `watcher-lines` — See the frame list above; carried here as well as on the raw stream

Frames the server has no grid consumer for are dropped rather than forwarded, so this
socket does not carry `output`, `parsed` or the activity pulse.

**Dropped-frame recovery.** Binary frames are deltas, and the `watch` channel behind
this socket keeps only the newest value — a client that cannot keep up skips frames
and would apply a delta onto a row map missing rows. Each published frame therefore
carries a Rust-internal sequence number; when the reader sees a gap it re-serialises
the full grid and sends that instead. The sequence never reaches the wire, so the
binary frame format is unchanged.

### Server-Sent Events (SSE)

```
GET /events?types=repo-changed,pty-parsed&stream_id=<uuid>
```

Broadcasts server-side events to all browser/mobile clients. Supports optional `?types=` query parameter for comma-separated event name filtering. Omitting `types` asks for every event; sending it empty is an empty allowlist and delivers none. Uses monotonic event IDs and 15-second keep-alive pings.

`stream_id` is a client-chosen id for the connection. With one, `types` is only the
*initial* filter and the client can widen it later on the same connection:

```
POST /events/types
Content-Type: application/json

{ "stream_id": "<uuid>", "types": ["repo-changed", "dir-changed"] }
```

`types` is the full set the client wants from now on, not a delta. `204` applies it to
the live stream; `404` means the server is not tracking that stream (it ended, or more
than 64 streams are already tracked), and the client must fall back to reconnecting with
a wider `?types=`.

A panel that mounts late needs an event type the stream was not opened with, and
reconnecting to get it loses every event published between the close and the new
subscription — the server subscribes to the event bus at connect time and replays
nothing. That is what this route exists to avoid. A client that lets `EventSource`
auto-reconnect must re-post its set on every `onopen`: the reconnect replays the URL, so
the server is back to the filter the connection was opened with.

| Event | Payload | Description |
|-------|---------|-------------|
| `session-created` | `{session_id, cwd, agent_type, display_name}` | New session started; `display_name` is the optional stable assigned name |
| `pty-description-changed` | `{session_id, description}` | Orchestrator updates the short task description shown above a PTY |
| `session-closed` | `{session_id}` | Session ended |
| `repo-changed` | `{repo_path, kind}` | Repository changed. `kind` is `"git-state"` (`.git/` was written — a commit, ref or index change) or `"working-tree"` (files changed and `.git` did not). A git-state emit cancels the pending working-tree one, so `"git-state"` does **not** mean "only `.git` changed" — a client that needs working-tree news must react to both kinds. |
| `head-changed` | `{repo_path, branch}` | Git HEAD changed (branch switch) |
| `pty-parsed` | `{session_id, parsed}` | Structured output event from PTY parser |
| `pty-exit` | `{session_id}` | PTY process exited |
| `pty-activity` | `{session_id}` | Bytes are flowing from the PTY. Payload-free pulse, at most one per second — a dropped pulse loses nothing |
| `pty-osc133` | `{session_id, marker, line, exit_code}` | Shell-integration marker (OSC 133). `exit_code` is `null` except on a `D` marker that reported one |
| `pty-cwd` | `{session_id, cwd}` | Working directory changed (OSC 7) |
| `plugin-watcher-lines` | `{session_id, lines}` | A batch of assembled PTY lines for the plugin OutputWatchers; each line is `{text, matched_ids}`, where `text` is the cleaned text the match was made on |
| `plugin-changed` | `{plugin_ids}` | Plugin(s) installed/removed/updated |
| `upstream-status-changed` | `{name, status}` | MCP upstream server status change |
| `mcp-toast` | `{title, message, level, sound, origin_repo_path?, origin_session_id?}` | Toast notification from MCP layer, including the caller repository/cwd and the caller's TUIC session when known. Clients use the session id to focus the terminal that raised the toast |
| `triage-progress` | `{repo_path, summary, files, phase, done, llm_used, llm_model}` | Diff-triage classification progress (browser parity for the desktop window event) |
| `session-state-changed` | `{session_id, state}` — `state` is the same object `GET /sessions` returns per session (`shell_state`, `agent_state`, `awaiting_input`, `question_confident`, `background_work`, `queued_commands`, …), snake_case, with the fields serde skips at their zero value omitted | A session's derived lifecycle state moved. Published by the session-state accumulator (`state.rs publish_session_state_change`) once per real transition, deduped by `SessionState`'s `PartialEq` — a repaint that changes only `last_activity_ms` publishes nothing. Dual-emitted on the Tauri window under the same name and with the same payload, so `useAgentPolling.ts` consumes both transports with one handler instead of polling `list_active_sessions`. Absence of a field means its zero value, not "unknown" |
| `lagged` | `{missed}` | Client fell behind; N events were dropped. Dropped events are never resent, so a client that derives state from the stream must re-read it — `subscribeEvents`' `onResync("lagged")` callback exists for that. It also fires with `"reconnect"` on any EventSource re-open after the first, because a drop loses the same way silently. Both are SSE-only: Tauri `listen()` is in-process and cannot drop |

### MCP Streamable HTTP

```
POST /mcp
Content-Type: application/json

{ JSON-RPC message }
```

Single endpoint for all MCP JSON-RPC requests (initialize, tools/list, tools/call). Returns JSON-RPC responses directly in the HTTP response body. Session ID returned via `Mcp-Session-Id` header on initialize.

The native `session` tool includes `action=submit` for a managed-agent command
and bounded terminal-movement receipt in that same JSON-RPC response. It is
loopback-only, never queues, and rejects a busy/dialog/partial composer before
writing. `action=input` and `POST /sessions/:id/write` remain raw write-only
surfaces; neither returns submission acknowledgement. See
[MCP & HTTP Server](../backend/mcp-http.md#mcp-tool-session-atomic-submission).

```
GET /mcp          → 405 Method Not Allowed
DELETE /mcp       → Ends MCP session (pass Mcp-Session-Id header)
```

## Git Endpoints

### Repository Info

```
GET /repo/info?path=/path/to/repo
```

Returns `RepoInfo` (name, branch, status, initials).

### Git Diff

```
GET /repo/diff?path=/path/to/repo
```

Returns unified diff string.

### Diff Stats

```
GET /repo/diff-stats?path=/path/to/repo
```

Returns `{ "additions": N, "deletions": N }`.

### Changed Files

```
GET /repo/files?path=/path/to/repo
```

Returns array of `ChangedFile` (path, status, additions, deletions).

### Single File Diff

```
GET /repo/file-diff?path=/path/to/repo&file=src/main.rs
```

Returns diff for a single file.

### Read File

```
GET /repo/file?path=/path/to/repo&file=src/main.rs
```

Returns file contents as text.

### Branches

```
GET /repo/branches?path=/path/to/repo
```

Returns sorted branch list.

### Repo Summary

```
GET /repo/summary?path=/path/to/repo
```

Aggregate snapshot: worktree paths, merged branches, and per-path diff stats in one round-trip. Replaces 3+ separate IPC calls.

### Repo Structure (Progressive Phase 1)

```
GET /repo/structure?path=/path/to/repo
```

Returns `{ "worktree_paths": { "branch": "/path", ... }, "merged_branches": ["branch", ...] }`. Fast path — no diff stats computation.

### Repo Diff Stats (Progressive Phase 2)

```
GET /repo/diff-stats/batch?path=/path/to/repo
```

Returns `{ "diff_stats": { "/path": { "additions": N, "deletions": N }, ... }, "last_commit_ts": { "branch": N, ... }, "workspace_statuses": { "workspace-id": { "dirty": true, "commit_status": "unpublished", "unpublished_commits": 3, "removal_safety": "requires_force" } } }`. Slow path — computes per-worktree diff stats, timestamps, and lifecycle verdicts. Lifecycle entries are keyed by opaque workspace id, never branch name.

### Local Branches

```
GET /repo/local-branches?path=/path/to/repo
```

Returns local branch list.

### Checkout Remote Branch

```
POST /repo/checkout-remote
Content-Type: application/json

{ "repoPath": "/path/to/repo", "branchName": "feat-remote" }
```

Creates a local tracking branch from `origin/<branchName>`.

### Rename Branch

```
POST /repo/branch/rename
Content-Type: application/json

{ "path": "/path/to/repo", "old_name": "old", "new_name": "new" }
```

### Check Main Branch

```
GET /repo/is-main-branch?branch=main
```

Returns `true` if the branch is main/master/develop.

### Initials

```
GET /repo/initials?name=my-repo
```

Returns 2-char repo initials.

### Markdown Files

```
GET /repo/markdown-files?path=/path/to/repo
```

Returns list of `.md` files in a directory.

### Recent Commits

```
GET /repo/recent-commits?path=/path/to/repo
```

Returns recent git commits.

### GitHub Status

```
GET /repo/github?path=/path/to/repo
```

Returns PR status, CI status, ahead/behind for current branch.

### PR Statuses (Batch)

```
GET /repo/prs?path=/path/to/repo
```

Returns `BranchPrStatus[]` for all branches with open PRs.

### PR Statuses (Multi-Repo Batch)

```
POST /repo/prs/batch
Content-Type: application/json

{ "paths": ["/repo1", "/repo2"], "include_merged": false }
```

Returns aggregated PR statuses across multiple repositories.

### Issues

```
GET /repo/issues?path=/path/to/repo
```

Returns `GitHubIssue[]` for the repo, filtered by the user's configured issue filter.

### Close Issue

```
POST /repo/issues/close
Content-Type: application/json

{ "repo_path": "/path/to/repo", "issue_number": 42 }
```

Closes the specified issue via GitHub GraphQL API.

### Reopen Issue

```
POST /repo/issues/reopen
Content-Type: application/json

{ "repo_path": "/path/to/repo", "issue_number": 42 }
```

Reopens a closed issue via GitHub GraphQL API.

### GitHub Auth & Diagnostics

Browser/PWA parity for the GitHub settings panel. Registered on the loopback
router only (the headless `tuic-remote` daemon does not expose GitHub).

```
GET  /github/viewer-login                       -> string (login)
GET  /repo/ci-failure-logs?repoPath=&branch=    -> string (logs)
POST /github/pr-hide-drafts   { hide }          -> null
POST /github/auth/start                         -> DeviceCodeResponse
POST /github/auth/poll        { deviceCode }    -> PollResult
POST /github/auth/logout                        -> null
POST /github/auth/disconnect                    -> null
GET  /github/auth/status                        -> AuthStatus
GET  /github/diagnostics                        -> GitHubDiagnostics
```

Auth commands share the desktop `*_impl` (device-code flow + OS-keyring token via
`crate::credentials`). `get_all_issues` is intentionally unmapped — it has no frontend
`invoke()` caller (the `/repo/issues` route already serves browser issue lists).

### Merged Branches

```
GET /repo/branches/merged?path=/path/to/repo
```

Returns list of branch names merged into the default branch.

### Orphan Worktrees

```
GET /repo/orphan-worktrees?repoPath=/path/to/repo
```

Returns list of worktree directory paths that are in detached HEAD state (their branch was deleted).

### Remove Orphan Worktree

```
POST /repo/remove-orphan
Content-Type: application/json

{ "repoPath": "/path/to/repo", "worktreePath": "/path/to/worktree" }
```

Removes an orphan worktree by filesystem path. The worktree path is validated against the repo's actual worktree list.

### Merge PR via GitHub

```
POST /repo/merge-pr
Content-Type: application/json

{ "repoPath": "/path/to/repo", "prNumber": 42, "mergeMethod": "squash" }
```

Merges a PR via the GitHub API. `mergeMethod` must be `"merge"`, `"squash"`, or `"rebase"`. Returns `{"sha": "..."}` on success.

### Approve PR

```
POST /repo/approve-pr
Content-Type: application/json

{ "repoPath": "/path/to/repo", "prNumber": 42 }
```

Submits an approving review on a PR via the GitHub API.

### CI Checks

```
GET /repo/ci?path=/path/to/repo
```

Returns detailed CI check list.

### PR Diff

```
GET /repo/pr-diff?path=/path/to/repo
```

Returns diff for the current branch's open PR.

### AI Review / Changelog / Conflict Assist

```
POST /ai/review/pr           { repoPath, prNumber }        -> PrReviewResult
GET  /repo/merged-prs?path=&sinceTag=                      -> MergedPr[]
GET  /repo/changelog?path=&sinceTag=                       -> { markdown, json }
POST /repo/conflict-assist   { repoPath, prNumber }        -> ConflictAssistResult
```

`/ai/review/pr` runs the multi-turn review engine (Main slot) over a PR diff and
returns line-level findings. `/repo/changelog` summarizes merged PRs (Headless
slot) into markdown + a structured JSON breakdown; `sinceTag` filters to PRs
merged at/after that tag's date. `/repo/conflict-assist` creates a worktree on
the PR head and rebases it onto the base. `status` is `clean` only when the base
was refreshed from origin, `clean_unverified` when a conflict-free result used
an existing tracking ref or local fallback, and `conflicts` when manual
resolution is needed. The response includes `base_source`, an optional
`base_warning`, the conflicted-file list, and an agent prompt; it never pushes
or merges.

### Remote URL

```
GET /repo/remote-url?path=/path/to/repo
```

Returns the remote origin URL.

## Git Panel Endpoints

### Working Tree Status

```
GET /repo/working-tree-status?path=/path/to/repo
```

Returns porcelain v2 working tree status.

### Panel Context

```
GET /repo/panel-context?path=/path/to/repo
```

Returns aggregated context for the Git Panel (status, branch, merge state).

### Stage Files

```
POST /repo/stage
Content-Type: application/json

{ "repoPath": "/path/to/repo", "files": ["src/main.rs"] }
```

### Unstage Files

```
POST /repo/unstage
Content-Type: application/json

{ "repoPath": "/path/to/repo", "files": ["src/main.rs"] }
```

### Discard Files

```
POST /repo/discard
Content-Type: application/json

{ "repoPath": "/path/to/repo", "files": ["src/main.rs"] }
```

### Commit

```
POST /repo/commit
Content-Type: application/json

{ "repoPath": "/path/to/repo", "message": "feat: add feature" }
```

### Run Git Command

```
POST /repo/run-git
Content-Type: application/json

{ "repoPath": "/path/to/repo", "args": ["log", "--oneline", "-5"] }
```

Runs an arbitrary git command in the repo directory.

### Commit Log

```
GET /repo/commit-log?path=/path/to/repo
```

Returns commit log entries.

### File History

```
GET /repo/file-history?path=/path/to/repo&file=src/main.rs
```

Returns git log for a specific file.

### File Blame

```
GET /repo/file-blame?path=/path/to/repo&file=src/main.rs
```

Returns line-by-line blame annotations.

### Git Panel (Branches / Graph / Gutter)

```
GET  /repo/gutter-changes?path=&file=&scope=      -> GutterChange[]
GET  /repo/branches-detail?path=                  -> BranchDetail[] (cached)
GET  /repo/recent-branches?path=&limit=           -> string[]
GET  /repo/branch-base?path=&branchName=          -> string | null
GET  /repo/worktree-dirty?repoPath=&workspaceId=  -> bool
GET  /repo/base-ref-options?repoPath=             -> BaseRefOption[]
GET  /repo/commit-graph?path=&count=              -> GraphNode[]
POST /repo/clone-branch-name   { sourceBranch, existingNames }   -> string
POST /repo/create-branch       { path, name, startPoint?, checkout }       -> { ok: true }
POST /repo/delete-branch       { path, name, force }                        -> DeleteBranchResult
POST /repo/delete-local-branch { repoPath, branchName, workspaceId, keepWorktree? } -> { ok: true }
POST /repo/update-from-base    { path, branchName, strategy? }              -> string
POST /repo/switch-branch       { repoPath, branchName, force, stash }       -> SwitchBranchResult
POST /repo/merge-archive-worktree { repoPath, branchName, workspaceId, targetBranch, afterMerge, force? } -> MergeArchiveResult
```

Powers the Git panel's Branches tab, commit graph, and editor gutter in
browser/PWA/remote. Mutations call the shared `*_impl` + `invalidate_repo_caches`.
`run_diff_triage` (event-emitting, LLM progress) is not yet mapped — it belongs with
the agent/chat/watcher event-bridge work; see `todo.md`.

## Stash Endpoints

### List Stashes

```
GET /repo/stash?path=/path/to/repo
```

Returns stash list.

### Apply Stash

```
POST /repo/stash/apply
Content-Type: application/json

{ "repoPath": "/path/to/repo", "index": 0 }
```

### Pop Stash

```
POST /repo/stash/pop
Content-Type: application/json

{ "repoPath": "/path/to/repo", "index": 0 }
```

### Drop Stash

```
POST /repo/stash/drop
Content-Type: application/json

{ "repoPath": "/path/to/repo", "index": 0 }
```

### Show Stash

```
GET /repo/stash/show?path=/path/to/repo&index=0
```

Returns diff of a stash entry.

## Log Endpoints

### Get Logs

```
GET /logs?limit=50&level=error&source=terminal
```

Retrieve log entries from the ring buffer (1000 entries max). All query params optional:
- `limit` — max entries to return (0 = all, default: 0)
- `level` — filter by level: `debug`, `info`, `warn`, `error`
- `source` — filter by source: `app`, `plugin`, `git`, `network`, `terminal`, `github`, `dictation`, `store`, `config`

`level`/`source` filters are applied first, then `limit` takes the most recent N of the *filtered* results — not the last N of the whole buffer. `?level=error&limit=50` returns up to 50 error entries, even if none of the errors are among the newest lines overall.

### Push Log

```
POST /logs
{ "level": "warn", "source": "git", "message": "...", "data_json": "{...}" }
```

### Clear Logs

```
DELETE /logs
```

### Capture Raw PTY Streams

Start capture before reproducing an agent-state detection failure:

```text
POST /diagnostics/capture
Content-Type: application/json

{ "enabled": true, "session_id": "<session-id>" }
```

Omit `session_id` to capture every session. Starting capture creates a fresh set
of files rather than appending to an earlier run. `GET /diagnostics/capture`
returns `enabled`, the optional `session_filter`, the capture `dir`, and each
recorded session's byte count. Stop with:

```text
POST /diagnostics/capture
Content-Type: application/json

{ "enabled": false }
```

Files are written as framed PTY timelines to
`<app config dir>/captures/<session-id>.tcap`, capped at 512 KiB per session.
Each record preserves input/output direction, original chunk boundaries and a
monotonic timestamp. Legacy `.raw` fixtures remain readable as one output record.
Copy the relevant file into `src-tauri/src/fixtures/agent_prompts/` and replay it
through the production parser composition. Do not acquire state-detection
fixtures from `GET /sessions/:id/output`: its ring is bounded, may already have
overwritten the one-shot signal, and its string response is lossy UTF-8 rather
than a byte-preserving fixture.

### Execute JS in WebView (debug)

```
POST /debug/invoke_js
{ "script": "return window.__TUIC__.terminals().length;" }
```

Executes JavaScript in the main WebView. **Loopback-only** (rejected with 403 from
non-localhost peers) — this is an RCE surface and is exposed on the local router only,
never the remote router. Fire-and-forget: the return value (`return expr`) and any
captured `console.log/warn/error/info` output are pushed to the ring buffer with
`source="eval_js"`. Read the result back via `GET /logs?source=eval_js&limit=1`.

The only injected global is `window.__TUIC__` (stores, terminals, plugins, …). Mirrors
the MCP `debug action=invoke_js` tool — both share `log_routes::eval_debug_script`. The
HTTP route is what makes the `tauri dev` build (which has no MCP stdio transport)
scriptable for diagnostics.

### Reload the WebView (debug)

```
POST /debug/reload_webview
```

Sends the main WebView back to the last URL it was healthy at, from the **native**
side (`WebviewWindow::navigate`), so it works precisely when the JavaScript side
does not. **Loopback-only**, local router only. Returns
`{"ok":true,"action":"navigate","url":…}`, or `{"error":…}` when the window is not
available.

**Navigate, not reload.** It used to call `WebviewWindow::reload`, and on
2026-09-08 that answered `{"ok":true}` while the window stayed white for an hour:
the main frame was on `about:srcdoc`, and there is no URL behind a blank document
to reload. Changing this back to a reload re-breaks the one case the endpoint
exists for.

This is the recovery path for a white UI: a WebView whose main thread is blocked
(or whose document is gone) cannot run `/debug/invoke_js`, because that route
needs the very thread that is stuck. Before this endpoint existed the only remedy
was restarting the app, which takes every PTY session with it — sessions live in
the backend, so this costs nothing but a repaint.

The backend names both conditions on its own and recovers the second by itself:
the diagnostics thread logs `Frontend unresponsive: no heartbeat for Ns` when the
main thread is blocked, and the `webview-recovery` thread logs
`Main WebView lost its document` and re-navigates when the frame is on `about:`
(see AGENTS.md → Diagnostics). Note that `grid frame gate stuck` is **not** either
signal — hidden terminals never ack, so it fires in normal operation.

### Memory report (diagnostics)

```
GET /diagnostics/memory
```

Names which structure holds the process's memory. Returns `phys_footprint_bytes`
(resident **plus compressed** — `ps` RSS read 0.52 GB while the process held
40 GB), `accounted_bytes`, and `maps`: every `AppState` structure that grows with
sessions, clients or repos, with entry counts, measured bytes for the four that
hold payloads, sorted biggest first.

```json
{"phys_footprint_bytes":123456789,"accounted_bytes":98765432,
 "maps":[{"name":"grid.vt_log_buffers","entries":15,"bytes":94371840}, …]}
```

`accounted_bytes` far below the footprint is itself the finding: the growth is
outside `AppState`. Available on every instance, `tuic-remote` included.

## Configuration Endpoints

### App Config

```
GET /config
PUT /config
```

Load/save `AppConfig`.

`PUT /config` **merges** its body onto the live config rather than replacing it, so
a caller may send only the fields it wants changed. Objects merge key by key;
arrays and scalars replace wholesale (an empty array still clears a list, `""`
still blanks a string). A wrongly-typed field is a `400`, never a silent default.
When the body moves `services.server.{enabled,port,ipv6_enabled}` or
`services.auth.{username,password_hash}`, the HTTP listener is rebound just as the
IPC `save_config` does, so the running process cannot keep serving a configuration
the disk no longer agrees with.

`GET /config` redacts remote-access secrets (`services.auth.password_hash`,
`services.auth.session_token`, `services.relay.token`, and
`services.push.vapid_private_key`). Secret presence is exposed only through
`session_token_exists`, `token_exists`, and `vapid_private_key_exists`.

### Config / themes / notes / misc parity (story 066)

Browser/PWA parity for assorted stateless commands. Loopback router only.
Mutating/action routes carry the `require_local_or_auth` guard; reads do not.

```
GET  /config/ai-prompts                      -> AiPromptsConfig
PUT  /config/ai-prompts        (AiPromptsConfig)            -> { ok }
POST /config/repo-local-config { repoPath }                -> { ok }   (GET = read)
POST /config/branch-label      { repoPath, branchName, label? } -> { ok }
POST /config/note-image        { noteId, dataBase64, extension } -> string (path)
POST /config/note-assets/delete       { noteId }           -> { ok }
POST /config/note-assets/delete-batch { noteIds }          -> { ok }
GET  /config/themes                          -> ThemeEntry[]
POST /config/project-mcp-upstreams { repoPath, upstreamNames? } -> { ok }
POST /exec/shell-script        { scriptContent, timeoutMs, repoPath } -> string  [guarded]
GET  /audio/output-devices                   -> AudioOutputDevice[] (empty on remote)
POST /agent/discover-session   { agentType, cwd, claimedIds, agentPid?, envOverrides } -> { sessionId, launchCommand }|null
POST /agent/claude-project-dir { cwd, claudeConfigDir? }   -> string
POST /agent/open-in-custom     { executable, args, ctx }   -> { ok }   [guarded]
POST /generators/generate      { request }                 -> GeneratorResult  [guarded]
GET  /registry/plugins                       -> RegistryEntry[]
```

Intentionally NOT mapped (no frontend `invoke()` caller — YAGNI): `load_app_config`,
`save_app_config`, `get_note_images_dir`, `process_prompt_content_shell_safe`,
`detect_claude_binary`, `mdkb_code_find`. Skipped as integration/stateful (separate
follow-up): `set_ansi_colors` (PTY ring-buffer state), the `mdkb_*` daemon commands,
`install_agent_mcp`/`remove_agent_mcp` (config-file writes, also no caller).

### Provider keyring + slot/ollama checks (story 072)

Browser/PWA parity for provider API-key storage (the OS keyring is proxied through
the server so remote clients never touch it directly) plus slot/Ollama connectivity
checks. Loopback router only; mutating routes carry the `require_local_or_auth` guard.

```
GET    /config/provider-key/exists?providerId=<id>   -> bool
POST   /config/provider-key    { providerId, key }   -> { ok }    [guarded]
DELETE /config/provider-key    { providerId }        -> { ok }    [guarded]
POST   /config/slot-test       { slot }              -> string    (connection test result)
POST   /config/ollama-models   { providerId }        -> OllamaStatus
```

`OllamaStatus` is `{ available: bool, models: [{ name, size }], detail: string|null }`.
`detail` carries the backend's reason the endpoint is unusable — connection refused,
no answer within the 4s probe timeout, or the HTTP status it answered with — and is
`null` when the provider is reachable. Clients render it; they never compose their own
wording, so a new failure mode is a change in `detect_ollama` and nowhere else.

The OAuth upstream flow (`start_mcp_upstream_oauth` / `cancel_mcp_upstream_oauth`) is
**not** mapped: `start` binds a loopback callback server and opens the OS browser, so
the redirect can't return to a remote/PWA client. Desktop drives it over IPC; browser
clients get a clean host-only error until the redirect UX is redesigned.

### Hash Password

```
POST /config/hash-password
Content-Type: application/json

{ "password": "..." }
```

Returns bcrypt hash string.

### Notification Config

```
GET /config/notifications
PUT /config/notifications
```

Load/save `NotificationConfig`.

### UI Preferences

```
GET /config/ui-prefs
PUT /config/ui-prefs
```

Load/save `UIPrefsConfig`.

### Repository Settings

```
GET /config/repo-settings
PUT /config/repo-settings
```

Load/save per-repository settings.

### Repository Defaults

```
GET /config/repo-defaults
PUT /config/repo-defaults
```

Load/save default settings applied to new repositories.

### Check Custom Settings

```
GET /config/repo-settings/has-custom?path=/path/to/repo
```

Returns `true` if the repo has non-default settings.

### Repositories

```
GET /config/repositories
PUT /config/repositories
```

`GET` loads the repositories document. Every `PUT` must send the same versioned
`mutationVersion: 1` delta as desktop `save_repositories`: keyed
`repos`/`groups` entries carry `{id,before,after}`, while `repoOrder`,
`activeRepoPath`, and `groupOrder` optionally carry `{before,after}`. The
backend applies the delta to the latest document under the cross-process lock.
Different repository/group IDs and independent order membership changes
compose; incompatible changes to the same record return `409 Conflict` and a
malformed or unversioned delta returns `400 Bad Request`.

A `PUT` that actually moves the document broadcasts a payload-free
`repositories-changed` SSE event (subscribe with `GET /events?types=repositories-changed`)
so the other clients re-read the document instead of saving over it from a stale
baseline. A delta that was already applied changes nothing on disk and is not
announced. See `docs/backend/config.md` for what a receiving client is allowed to
adopt.

### Stale-temp Repository Repair (#763-d219)

```
GET  /config/repositories/stale-temp
POST /config/repositories/stale-temp
```

`GET` returns `StaleTempCandidate[]` (`{path, displayName}`) — a read-only
preview, re-classified against the live document on every call, of rows that
match ALL of: local path absent, path under a recognized OS/project temp root,
`isGitRepo: false`, exactly one shell-only workspace with no terminals/saved
terminals/parent/commit state, and no user metadata (see `docs/backend/config.md`
for the exact classifier). Never mutates.

`POST` body `{"paths": ["/tmp/…", …]}` — the user-explicit repair. Re-validates
every named path against the classifier on the document as it stands on disk
*right now*; if even one no longer matches, the whole request is rejected with
`400 Bad Request` and nothing is written (no implicit or partial deletion). On
success it writes an exact pre-repair backup (`repositories.repair-backup-<UTC
timestamp>.json`) in the config directory, removes the validated rows from
`repos`/`repoOrder`/every group's `repoOrder` (and clears `activeRepoPath` if it
pointed at a removed row) in one write, returns `StaleTempRepairSummary`
(`{removed, backupPath}`), and broadcasts `repositories-changed` like `PUT
/config/repositories`. Requires local-or-authenticated like every other
mutating config route.

### Prompt Library

```
GET /config/prompt-library
PUT /config/prompt-library
```

Load/save prompt entries.

### Notes

```
GET /config/notes
PUT /config/notes
```

Load/save notes (opaque JSON, shape defined by frontend).

### MCP Status

```
GET /mcp/status
```

Returns MCP server status (enabled, port, connected clients).

### MCP Upstream Status

```
PUT /mcp/upstreams
Content-Type: application/json

{
  "base": { "servers": [...] },
  "config": { "servers": [...] }
}
```

`base` is the configuration previously loaded by the caller and `config` is its
desired result. The backend derives an ID-keyed three-way delta, then applies it
to the latest `mcp-upstreams.json` under the cross-process file lock. Removing a
server from `config` explicitly deletes that ID; removing an optional `auth`
field explicitly clears it. Fields and servers unchanged from `base` preserve
concurrent updates, including OAuth/DCR auth written by another process. After
the atomic write, the live registry hot-reloads the exact locked pre/post
configurations. Returns `200` with an empty body, `400` for invalid config or
duplicate IDs, and `500` for persistence or conflicting-add failures.

```
GET /mcp/upstream-status
```

Returns status and metrics for all upstream MCP servers (connecting, ready, circuit_open, disabled, failed).

### MCP Instructions

```
GET /mcp/instructions
```

Returns dynamic server instructions for the MCP bridge binary as `{"instructions": "..."}`.

## Filesystem Endpoints

```
GET  /fs/list?repoPath=/path/to/repo&subdir=src
GET  /fs/search?repoPath=/path/to/repo&query=main&limit=50
GET  /fs/search-content?repoPath=/path/to/repo&query=foo&caseSensitive=false&useRegex=false&wholeWord=false&limit=200
GET  /fs/search-content-all?query=foo&caseSensitive=false&limit=200   -> cross-repo BM25 over every ready index
GET  /fs/read?repoPath=/path/to/repo&file=src/main.rs
GET  /fs/read-external?path=/absolute/path/to/file
POST /fs/write         { "repoPath": "...", "file": "...", "content": "..." }
POST /fs/mkdir         { "repoPath": "...", "dir": "..." }
POST /fs/delete        { "repoPath": "...", "path": "..." }
POST /fs/rename        { "repoPath": "...", "from": "...", "to": "..." }
POST /fs/copy          { "repoPath": "...", "from": "...", "to": "..." }
POST /fs/gitignore     { "repoPath": "...", "pattern": "..." }
GET  /fs/resolve-terminal-path?cwd=/repo&candidate=src/x.ts   -> ResolvedFilePath | null
POST /fs/resolve-terminal-paths { "cwd": "/repo", "candidates": [...] } -> (ResolvedFilePath | null)[]
GET  /fs/stat?path=/absolute/path                              -> PathStat (exists/is_dir/size/modified_at)
POST /fs/warm-index    { "repoPath": "..." }                   -> { "ok": true } (fire-and-forget BM25 build; strategy-gated, see below)
POST /fs/write-external { "path": "/abs", "content": "..." }   -> { "ok": true }
POST /fs/copy-abs      { "from": "/abs", "to": "/abs" }        -> { "ok": true }
POST /fs/move-abs      { "from": "/abs", "to": "/abs" }        -> { "ok": true }
POST /fs/transfer      { "destDir": "/abs", "paths": [...], "mode": "move"|"copy", "allowRecursive": bool } -> TransferResult
```

`/fs/warm-index` returns `{ "ok": true }` whether or not it scheduled anything.
Under the `disabled` or `active_only` index strategies it deliberately builds
nothing, so a repo switch cannot index behind a setting that asked it not to.
The `warm_content_index` IPC command applies the identical gate — the two
transports share one implementation so they cannot drift.

`/fs/search-content-all` reports why a repo produced nothing, so the UI can tell
"no match" apart from "not searched yet". Its result and every
`content-search-batch` carry `repos_searched`, `repos_pending`, and
`repos_indexing` — the last being the subset of `repos_pending` with a build
actually in flight. Only that subset justifies telling the user to retry; the
rest are waiting on a scheduling event that may never come.

Content-search results expose `match_start` and `match_end` as zero-based,
end-exclusive UTF-16 code-unit offsets within `line_text`. They can be passed
directly to JavaScript `String.slice`, including when text before the match
contains multibyte characters or non-BMP emoji.

Sandboxed filesystem operations for the file manager panel. `/fs/read-external` reads an arbitrary absolute path (not sandboxed to a repo).

## Agent Usage Endpoints

```
GET /claude/usage                              -> UsageApiResponse (rate-limit usage, 5-min cached)
GET /claude/projects                           -> ProjectEntry[]
GET /claude/timeline?scope=all&days=7          -> TimelinePoint[] (hourly token aggregation)
GET /claude/session-stats?scope=current        -> SessionStats
GET /codex/usage                               -> CodexUsageApiResponse (rate-limit usage, 5-min cached)
GET /codex/stats                               -> CodexStatsResponse (token history + lifetime stats)
```

### Terminal theme

```
POST /terminal/theme-colors   {foreground:[r,g,b], background:[r,g,b], cursor:[r,g,b]}
```

Publishes the resolved terminal theme so the emulator can answer OSC 10 / 11 /
12 colour queries. Not session-scoped: one window, one palette.

This is not cosmetic. An app asks for the background with `OSC 11 ; ? ST`
followed by `ESC[c` as a fence — DA is universally supported, so a DA reply
arriving with no colour reply before it is meant to mean "this terminal cannot
answer". A terminal that stays silent therefore never reads as "no", and the
probe simply retries: Claude Code re-sent the pair every ~1.2 s for the whole
life of a session, and every DA reply it triggered was written into the PTY as
if typed. Until the frontend posts here, the emulator answers from a dark
default — a plausible wrong colour ends the probe, silence does not.

Powers the Claude Usage dashboard in browser/PWA/remote. `scope` is `"all"`,
`"current"`, or a project slug. `timeline`/`session-stats` are desktop-only Tauri
commands; the handlers call non-gated `*_impl` siblings so they also serve the
remote daemon.

Both Codex routes read the OAuth token from `~/.codex/auth.json`; TUIC never
refreshes it, so an expired token surfaces as a 401 rather than a silent retry.

`/codex/usage` calls the endpoint the Codex CLI itself polls. It returns
rate-limit windows, the per-model limits under `additional_rate_limits`, credits
and `model_usage`.

`/codex/stats` calls `/wham/profiles/me` upstream — the daily token history does
**not** live under any `/usage` path. It returns `stats` with
`daily_usage_buckets` (~30 days of `{start_date, tokens}`) plus lifetime totals,
streaks, thread count, fast-mode share and reasoning-effort mix.

Identity fields the upstream sends are deliberately dropped before either
response leaves `codex_usage.rs`: `user_id`, `email` and `account_id` from the
usage payload, and the whole `profile` object (username, display name, avatar
URL) from the stats payload.

**Absolute-path write boundary.** `/fs/write-external`, `/fs/copy-abs`, and `/fs/move-abs` are gated to **registered repository roots** for the HTTP boundary (a 403 otherwise), mirroring `/fs/read-external`. The gate rejects traversal syntax (`..`), NUL bytes, and relative paths *before* the containment check: containment is `Path::starts_with`, which is purely lexical, so `/repo/../../etc/passwd` is "inside" `/repo` by components while the OS resolves it far outside. Paths are deliberately **not** canonicalized — a symlink inside a registered repo that points outside it is an accepted design decision in this project. `/fs/transfer` gates only its `destDir` — sources are commonly external (a file dragged in from the desktop). `/fs/stat` and `/fs/resolve-terminal-path` return only metadata (no content) so they are not repo-gated; both also refuse macOS TCC-protected directories. `/fs/resolve-terminal-path` returns JSON `null` on a miss (`Option<ResolvedFilePath>`). `/fs/resolve-terminal-paths` is its batched sibling and is a POST for one reason: a whole terminal screen's candidates do not fit a query string, and being able to send many of them is the point. It answers **positionally** — the array it returns has one entry per input candidate, in order, `null` where that candidate resolved to nothing — so a caller may index the response by the index of the request.

## Monitoring Endpoints

### Health Check

```
GET /health
```

Returns `{ "status": "ok" }`.

### Orchestrator Stats

```
GET /stats
```

Returns `{ "active_sessions": N, "max_sessions": 50, "available_slots": N }`.

### Session Metrics

```
GET /metrics
```

Returns `{ "total_spawned": N, "failed_spawns": N, "bytes_emitted": N, "pauses_triggered": N }`.

### Local IPs

```
GET /system/local-ips
```

Returns list of local network interfaces and addresses.

### Local IP (Primary)

```
GET /system/local-ip
```

Returns the preferred local IP address (single value).

## Watcher Endpoints

### Head Watcher

```
POST   /watchers/head?path=/path/to/repo
DELETE /watchers/head?path=/path/to/repo
```

Start/stop watching `.git/HEAD` for branch changes. Browser-only mode.

### Repo Watcher

```
POST   /watchers/repo?path=/path/to/repo
DELETE /watchers/repo?path=/path/to/repo
```

Start/stop watching `.git/` for repository state changes. Browser-only mode.

### Directory Watcher

```
POST   /watchers/dir?path=/path/to/directory
DELETE /watchers/dir?path=/path/to/directory
```

Start/stop watching a directory (non-recursive) for file changes (create/delete/rename). Emits `dir-changed` SSE event. Used by File Browser panel for auto-refresh.

### Hot Repos

```
PUT /watchers/hot-repos
```

Body: `{"paths": ["/path/to/repo", ...]}`

Updates the set of "hot" repository paths (repos with active terminals). Cold repos (not in this set) get throttled watcher debounce (15s vs 1.5s) and reduced GitHub polling frequency (~10min vs ~1min). HTTP equivalent of the `set_hot_repos` Tauri command, served by both the desktop server and `tuic-remote` — a browser client of the desktop app needs it just as much as a remote one.

### AI Watchers (agent rules — story 070)

```
GET  /ai/watchers                                            -> WatcherRule[]
POST /ai/watchers          { name, sessionId?, trigger, instructions?, promptId?, repoPath?, maxFires?, cooldownSecs? } -> id
POST /ai/watchers/update   { id, name?, trigger?, instructions?, promptId?, repoPath?, maxFires?, cooldownSecs? } -> { ok }
POST /ai/watchers/delete   { id }                            -> { ok }
POST /ai/watchers/toggle   { id, enabled }                   -> { ok }
POST /ai/watchers/attach   { templateId, sessionId }         -> id
POST /ai/watchers/detach   { id }                            -> { ok }
```

CRUD for the agent watcher rules (WatcherManager). Watcher *fires* surface as the
existing `session-created` SSE event (a fired watcher spawns an agent session), so no
dedicated watcher-fire stream is needed. Config mutations are client-initiated → the UI
refetches `GET /ai/watchers`; no push event for state changes. The mutation logic is the
shared `ai_agent::watcher::*_rule` core; `watcher_create`/`watcher_update` reuse the
extracted `*_impl`.

### AI Chat (config + conversation CRUD — story 069 RPC slice)

```
GET  /ai/chat/config                         -> AiChatConfig
PUT  /ai/chat/config          (AiChatConfig)  -> { ok }
GET  /ai/chat/conversations                  -> ConversationMeta[]
GET  /ai/chat/conversation?id=               -> Conversation
POST /ai/chat/conversation    (Conversation)  -> { ok }   (save)
POST /ai/chat/conversation/delete  { id }     -> { ok }
POST /ai/chat/new-id                         -> string (new conversation id)
```

File-backed conversation persistence + chat config.

```
GET (WS) /ai/chat/{chat_id}/stream
```

Chat registry live stream (event-bridge plan Step 4). WebSocket upgrade: the first
frame is a `ChatEvent::Snapshot` (`{"kind":"snapshot",...}`), then live `ChatEvent`
frames (`chunk`/`error`/`cleared`/`snapshot`) as they are fanned out. Closing the
socket unsubscribes (no explicit `chat_unsubscribe` call). Browser parity for the
desktop `chat_subscribe` Tauri Channel. Dedicated per-chat WS, NOT the global
`/events` bus (high-frequency token stream).

**No producer, and no client.** Nothing in the backend calls `fan_out` or any
`ConversationState` setter, so the only frame this stream ever sends is the empty
default snapshot. The frontend consumer was removed in story `600-d664`: applying
that snapshot ran `setMessages([])` and wiped the history `loadConversation` had
just read from disk. The route stays, unused, until something produces the events.

### AI Agent Loop control + knowledge + scheduler (story 068 RPC slice)

```
POST /ai/conversation/cancel   { sessionId }            -> string
POST /ai/conversation/pause    { sessionId }            -> string
POST /ai/conversation/resume   { sessionId }            -> string
POST /ai/conversation/approve  { sessionId, approved }  -> { ok }
GET  /ai/session-knowledge?sessionId=                   -> SessionKnowledgeSummary
POST /ai/suggestions/toggle    { sessionId }            -> bool (new state)
POST /ai/knowledge/sessions    { filter?, limit? }      -> SessionListEntry[]
GET  /ai/knowledge/session?sessionId=                   -> SessionDetail | null
GET  /ai/scheduler/config                               -> SchedulerConfig
PUT  /ai/scheduler/config      (SchedulerConfig)        -> { ok }
POST /ai/triage/run            { repoPath, refresh? }   -> TriageResult   (desktop only)
POST /ai/improvements/scan     { repoPath, focus }      -> ImprovementScanResult (desktop only)
POST /repo/create-issue-from-proposal { repoPath, proposal } -> CreatedIssue (desktop only)
GET (WS) /ai/conversation/{session_id}/stream
```

Agent-loop *control* (cancel/pause/resume/approve), session-knowledge reads, and the
scheduler config. State-taking commands reuse extracted `*_impl`s
(`get_session_knowledge_impl`, `toggle_ai_suggestions_impl`,
`get_knowledge_session_detail_impl`, `save_scheduler_config_impl`).

`PUT /ai/scheduler/config` also reconciles the cron tick loop: it starts the
30s-tick background task if the saved config has at least one enabled job and
it wasn't already running, and stops it if the config has none (#672-c1a3) —
previously the loop ticked (and re-read `ai-cron.json` from disk) forever from
boot regardless of whether any job existed.

**Conversation token stream** (event-bridge plan Step 3): the WebSocket
`/ai/conversation/{session_id}/stream` is the browser parity for the desktop
`start_conversation` Tauri Channel. The client sends the start params as the first
text frame — `{ message, autonomy?, maxSteps?, temperature?, modelOverride?,
bypassedTools?, reasoningEffort? }` — then receives `ConversationEvent` frames
(`{"type":"text_chunk",...}` etc.) with the same 50ms batching as desktop. Dedicated
per-session WS, NOT the global `/events` bus (high-frequency token stream). A client
disconnect stops forwarding but leaves the conversation running — cancel explicitly
via `/ai/conversation/cancel`. The bridge watches the socket alongside the event
stream (same `forward_until_closed` helper as the chat bridge), so a disconnect on a
*quiet* conversation — one blocked on tool approval, say — is noticed at once instead
of waiting for a send failure on the next event, which may never arrive.

**Diff triage** (`POST /ai/triage/run`, event-bridge plan Step 2): triggers
`run_diff_triage`; progress frames stream over the global `/events` SSE bus as
`triage-progress` (low-frequency, safe on the bus). Desktop-only — the triage LLM
pipeline needs the desktop providers, so the remote daemon does not serve it.

**Improvement proposals** (`POST /ai/improvements/scan`) run a one-shot Headless-slot
LLM pass over deterministic local repo context (working-tree status + recent commits)
and emit `proposals-ready` on the same GitHub Ops event shape. The scan never creates
GitHub issues. A user action calls `POST /repo/create-issue-from-proposal`, which
wraps the existing `create_issue_impl` path and returns `{ number, url, title }`.

## Agent Endpoints

### Detect All Agents

```
GET /agents
```

Returns detected agent binaries and installed IDEs.

### Detect Specific Agent

```
GET /agents/detect?binary=claude
```

Returns detection result for a specific agent binary.

### Detect Installed IDEs

```
GET /agents/ides
```

Returns list of installed IDEs.

### Batch Detect Agent Binaries

```
POST /agents/detect-all
Content-Type: application/json

{ "binaries": ["claude", "codex"] }
```

Returns `{ "<binary>": { "path": string|null, "version": string|null }, ... }`.
Detection runs in parallel and skips version lookup for speed; use
`GET /agents/detect` when the version matters. Blank names are dropped, so a name
that was sent may be absent from the map.

### Open Path in Application

```
POST /agents/open-in-app
Content-Type: application/json

{ "path": "/repo/src/main.rs", "app": "vscode", "line": 42, "col": 7 }
```

Opens the path in an IDE, terminal or file manager. `line` and `col` are optional
and only used by editors that support them. An unknown `app` is rejected before
anything is spawned. Returns `null` on success, matching the `open_in_app` command.

## Dictation Endpoints

Desktop-only: audio capture and the whisper model live behind the `desktop`
feature, so the remote daemon serves none of these. Every response is exactly what
the matching `dictation::commands` Tauri command resolves to — a bare string for
`inject`, `null` for the commands that return nothing — because
`src/stores/dictation.ts` reads both transports with the same code. Before the app
handle is up, every state-touching route answers `503`.

```
GET  /dictation/status                              -> DictationStatus
GET  /dictation/models                              -> ModelInfo[]
POST /dictation/models/download  { "model": "..." } -> "Downloaded to <path>"
POST /dictation/models/delete    { "model": "..." } -> "<deletion message>"
POST /dictation/start                               -> null
POST /dictation/stop                                -> TranscribeResponse
GET  /dictation/corrections                         -> { "<from>": "<to>", ... }
PUT  /dictation/corrections      { "map": { ... } } -> null
GET  /dictation/devices                             -> AudioDevice[]
POST /dictation/inject           { "text": "..." }  -> "<corrected text>"
GET  /dictation/config                              -> DictationConfig
PUT  /dictation/config           DictationConfig    -> null
```

`POST /dictation/stop` stops the recording and transcribes it, returning
`{ text, skip_reason?, duration_s }`. `PUT /dictation/config` takes the config
object as the whole body, not wrapped in a field.

## Desktop Integration Endpoints

Desktop-only, like the three commands they mirror: the relay client, the audio
output and the updater are all gated on the `desktop` feature.

### Relay Status

```
GET /system/relay-status
```

Returns `{ "enabled": bool, "connected": bool, "url": string, "session_id": string }`.
Shares one body with the `get_relay_status` command, so the two transports cannot
drift.

### Play Notification Sound

```
POST /system/notification-sound
Content-Type: application/json

{ "sound": "question", "volume": 0.5, "device": "Studio Display Speakers" }
```

`sound` is one of `question`, `completion`, `error`, `warning`, `info`,
`attention`. `device` is the chosen audio output; `null` or omitted uses the
system default. Returns `null`, matching the command.

### Check Update Channel

```
GET /system/check-update?channel=nightly
```

Returns `{ "available": bool, "version": string|null, "notes": string|null, "release_page": string|null, "not_found": bool }`.
Channel URLs are hardcoded — no user-supplied URL is accepted. An unpublished
channel is `not_found: true`, not an error; an unknown channel name is a 500.

## Prompt Endpoints

### Process Prompt

```
POST /prompt/process
Content-Type: application/json

{ "content": "...", "variables": { ... } }
```

Substitutes `{{var}}` placeholders in prompt text.

### Extract Variables

```
POST /prompt/extract-variables
Content-Type: application/json

{ "content": "..." }
```

Returns list of `{{var}}` placeholder names found in content.

## Plugin Endpoints

### List Plugins

```
GET /plugins/list
```

Returns array of valid plugin manifests.

### Plugin Development Guide

```
GET /plugins/docs
```

Returns the complete plugin development reference as `{"content": "..."}`. AI-optimized documentation covering manifest format, PluginHost API, structured event types, and example plugins.

### Plugin Data

```
GET /api/plugins/:plugin_id/data/*path
```

Reads a plugin's stored data file. Returns `application/json` if content starts with `{` or `[`, otherwise `text/plain`. Returns 404 if the file doesn't exist. Goes through the same auth middleware as all other routes.

**Note:** `write_plugin_data` maps to `POST /api/plugins/:plugin_id/data/*path`; `delete_plugin_data` has no HTTP route (no frontend caller). Data is sandboxed to `~/.config/tuicommander/plugins/{plugin_id}/data/`.

### Plugin RPC (host capabilities, story 071)

Browser/PWA parity for the plugin host RPC surface. Every route is `:plugin_id`-scoped
and reuses the same per-plugin sandboxing as the Tauri commands (`plugin_fs.rs` path
jail, `plugin_http.rs` allowed-URL check, `plugin_exec.rs` binary whitelist).

```
GET  /api/plugins/:plugin_id/fs/read?path=<p>                     -> string        (plugin_read_file)
GET  /api/plugins/:plugin_id/fs/read-base64?path=<p>              -> string        (plugin_read_file_base64)
GET  /api/plugins/:plugin_id/fs/tail?path=<p>&maxBytes=<n>        -> string        (plugin_read_file_tail)
GET  /api/plugins/:plugin_id/fs/list?path=<p>&pattern=&sortBy=    -> string[]      (plugin_list_directory)
POST /api/plugins/:plugin_id/fs/write    { path, content }        -> { ok }        (plugin_write_file)
POST /api/plugins/:plugin_id/fs/rename   { from, to }             -> { ok }        (plugin_rename_path)
POST /api/plugins/:plugin_id/build-artifacts/scan   { repoPaths, forceRefresh? } -> BuildArtifact[]
POST /api/plugins/:plugin_id/build-artifacts/delete { path, repoPaths } -> { ok }
POST /api/plugins/:plugin_id/build-artifacts/trim   { path, repoPaths } -> { ok }   (intermediates only; keeps executables)
POST /api/plugins/:plugin_id/exec        { binary, args, cwd? }   -> string        (plugin_exec_cli)
POST /api/plugins/:plugin_id/http        { url, method?, headers?, body?, allowedUrls } -> HttpResponse
GET  /api/plugins/:plugin_id/pty/output?sessionId=<id>&maxLines=  -> string        (plugin_read_session_output)
POST /api/plugins/:plugin_id/register    { capabilities }         -> { ok }
POST /api/plugins/:plugin_id/unregister                           -> { ok }
GET  /api/plugins/:plugin_id/readme                               -> string | null
```

Build-artifact scans normalize the root set, share an in-flight scan across callers,
and reuse completed results for 30 seconds. Set `forceRefresh: true` to bypass a
completed cached result; a scan already running for the same roots remains shared.

Intentionally **not** mapped (native/host-only, stay Tauri-only): `plugin_watch_path` /
`plugin_unwatch` (change events need AppHandle/WS delivery), `plugin_read_credential`
(OS keychain), and user-plugin install/uninstall (`install_plugin_from_*`,
`uninstall_plugin` — local-FS install + AppHandle emit). `delete_plugin_data` is unmapped
for lack of a frontend caller (YAGNI).

### Plugin Output Watchers

```
POST /api/plugins/output-watchers
Content-Type: application/json

{
  "client_id": "b7d1…",
  "seq": 3,
  "watchers": [{ "id": "w0", "pattern": "model is at capacity", "flags": "i" }]
}
```

Replaces the compiled OutputWatcher set the PTY reader thread matches lines against
(`set_plugin_output_watchers`). `pattern` and `flags` are the source and flags of a JS
`RegExp`; `i`, `m` and `s` are applied. The route is not `:plugin_id`-scoped: one
frontend owns one set, holding the watchers of all its plugins, and pushes all of it on
every add or remove.

`client_id` identifies the frontend. Sets are per client (at most 8; the least recently
synced is evicted, and an empty `watchers` array leaves a parked record so a delayed
older sync cannot resurrect the disposed set), so a desktop window and a browser tab
cannot overwrite each other. A client is expected to re-post its set every 30 s while it
holds any watcher: nothing signals a disconnect, so that heartbeat is both what keeps it
from being evicted as dead and how it recovers if it was. It must not contain `/`, which qualifies the
watcher ids reported back. `seq` is a monotonic per-client counter that orders the
mutations: a sync whose `seq` is not above the stored one is stale and changes nothing.

Returns `{ "applied": bool, "rejected": [id] }`. `rejected` lists the ids the Rust
`regex` crate cannot compile (lookaround, backreferences, a negated class escape inside
a character class). A rejected id is not an error — the frontend keeps matching that
watcher itself, and Rust ships every assembled line for as long as one is registered.
When `applied` is `false` the sync was stale: the client must ignore `rejected`, because
it describes a set the backend does not hold.

Assembled lines are pushed back as the `watcher-lines` WebSocket frame (on
`/sessions/:id/stream` in both `?format=grid` and raw mode; `?format=log|text` does not
carry it) and the `plugin-watcher-lines` SSE event.

## Worktree Endpoints

### List Worktrees

```
GET /worktrees
```

Returns list of managed worktrees.

### Create Worktree

```
POST /worktrees
Content-Type: application/json

{ "base_repo": "/path", "branch_name": "feature-x", "mode": "auto", "dirty": "inherit" }
```

`base_repo` must be an absolute, normalized path. The route rejects invalid paths
before invoking git, matching MCP `repo action=worktree_create` validation.

`mode` (`auto` | `cow` | `worktree`, default `auto`) chooses the mechanism.
`auto` takes a copy-on-write clone of the whole repo directory where the
filesystem and the repo shape allow it — an independent repository where
`node_modules`/`target` arrive warm — and a
linked worktree otherwise, reporting why. `cow` fails when COW is unavailable,
naming the check. `worktree` forces the old behaviour.

`dirty` (`inherit` | `clean_untracked` | `clean`, default `inherit`) decides what
happens to the parent's uncommitted work in a clone. `inherit` writes zero
blocks. `clean_untracked` removes untracked files but KEEPS ignored build
output. `clean` resets everything and costs real disk — measured 15 MB → 113 MB.

The first managed checkout of a branch fixes its mechanism. Creation refuses a
parallel COW when a linked worktree already owns the branch, and refuses a
parallel linked worktree when a persisted COW owns it. If both sources claim the
branch, the response reports a reconciliation conflict instead of guessing.

`201` returns:

```json
{
  "name": "feature-x",
  "path": "/path__wt/feature-x",
  "workspace_id": "feature-x",
  "branch": "feature-x",
  "base_repo": "/path"
}
```

`instructions` is the model-facing payload: the dirty policy applied and how
many paths carried over, the warm artifact directories with their sizes (plus
`warmed_directories`, how many ignored build directories were copied in — always
`0` for a COW clone, which copied the whole tree in one operation) and an
explicit instruction not to run an install or a full build, and the isolation
semantics of the mechanism this workspace actually got. It is byte-identical to
what MCP `repo action=worktree_create` returns — one value, two carriers — and
it is the ONLY instruction channel: there is no enforcement layer behind it.

`workspace_id` is how every later call addresses this workspace — `DELETE
/worktrees/:workspaceId`, `POST /worktrees/finalize`,
`GET /repo/worktree-dirty`, MCP `action=worktree_remove`. Read it from the
response; do not assume it equals `branch`. It does for a linked worktree (the
identity migration) and will not for a COW clone. MCP
`repo action=worktree_create` returns the same two fields.

Creation announces itself on both transports as `worktree-created`
(`{ repo_path, workspace_id, branch, worktree_path, kind }`, `kind` being
`"cow"` or `"worktree"`) and removal as
`worktree-removed` (`{ repo_path, workspace_id, branch }`) — the desktop Tauri
event and the `/events` SSE frame serialize the same struct, so the field names
are identical by construction. Payload table: `docs/sync-matrix.md`.

A COW clone is durably registered in `repositories.json` before this returns
`201` — not left for the frontend's own save, which used to be the only writer
and left a crash-sized window where a real clone existed with nothing durable
naming it (#756-cf6d). If registration itself fails, the clone is **not**
deleted — the response is `500` naming the registration failure explicitly, and
the clone stays on disk for a retry or the recovery pass below to pick up.
Removal is the mirror image: the row is dropped only once the directory is
confirmed gone, and a failure to drop it is its own explicit `500` rather than
a silently stale row.

### Worktrees Base Directory

```
GET /worktrees/dir
```

Returns the base directory where worktrees are created.

### Get Worktree Paths

```
GET /worktrees/paths?path=/path/to/repo
```

Returns `{ "<workspace-id>": { "branch": "feature-x", "path": "/worktree/path", "kind": "worktree" | "cow" }, ... }`.

Keyed by opaque workspace id, never by branch — two workspaces may sit on one
branch, so a branch-keyed map collapses them and a client asking for one gets the
other's directory (#726-5ac7). For a git worktree the id **is** the branch (the
identity migration), so nothing persisted moves; only a COW clone carries a
minted id. Persisted COW rows are merged with Git's linked-worktree list because
Git cannot report independent clones. Nothing may parse the id back into a
branch: read the `branch` field.

Before merging, this route self-heals: any immediate child of the repo's
worktree directory that carries every marker a COW clone's own git config is
given at creation (its minted workspace id, its canonical parent path, and the
existing no-push parent-remote evidence) and is not already registered gets
registered here, so a row lost to the crash window above — or restored from an
old backup — reappears without a separate recovery call. A directory missing
even one marker is never adopted, including a COW clone made before this
recovery existed; there is no safe way to tell those apart from an arbitrary
directory, so they stay invisible rather than being guessed at.

Removal (`DELETE /worktrees/:workspaceId` on a COW workspace) re-runs this same
validation against the directory the row names immediately before deleting
anything, unconditionally — a hand-edited, stale, or otherwise forged
`kind: "cow"` row is refused even with `force=true`, which only ever waives the
dirty-workspace/unpublished-commit prompts, never provenance. The same check
also requires a real (non-symlink) `.git` directory and that the row's
workspace id matches the one recorded on disk.

### Adopt COW Workspace

```
POST /worktrees/adopt
Content-Type: application/json

{ "repoPath": "/path", "candidatePath": "/path__wt/legacy-clone", "workspaceId": null }
```

Explicitly registers a markerless COW clone this backend lost track of (or
never marked in the first place, if made before the marker-based recovery
above existed) — the one case the self-heal above will never adopt on its own.
The request itself is the confirmation: there is no separate `force` flag,
because adoption never deletes anything, and the only change it makes to the
clone is writing its two provenance markers into local git config — HEAD, the
index, the working tree, untracked files, refs and remotes are untouched.

Validates, in order: `candidatePath` canonicalizes to an immediate,
non-symlink child of the repo's configured worktree base; it has a real
(non-symlink) `.git` directory; its `parent` remote's `pushurl` is the same
no-push sentinel every COW clone carries and that remote's `url` resolves to
`repoPath`; it is on a branch (not detached); and it collides with no
already-known workspace id or path (linked worktrees and existing COW rows
alike). Only once every check passes does it write anything, and what it
writes is exactly the two provenance markers creation itself would have
written — nothing to the index, the working tree, refs, HEAD, untracked files,
or any remote.

`workspaceId` is optional: omitted, a fresh id is minted the same way creation
mints one; supplied, that id is used instead (refused if already taken) —
primarily for retrying a call whose registry write failed after the markers
were already written, which is idempotent.

`200` returns:

```json
{ "workspaceId": "legacy-clone~a1b2c3d4", "branch": "legacy-clone", "path": "/path__wt/legacy-clone" }
```

Identical shape and identical implementation
(`worktree::adopt_cow_workspace_impl`) behind the HTTP route, the desktop
`adopt_cow_workspace` Tauri command, and MCP `repo action=worktree_adopt`.

### Publish Workspace

```
POST /worktrees/publish
Content-Type: application/json

{ "repoPath": "/path", "workspaceId": "feature-x~a1b2c3d4" }
```

Gets a workspace's commits into the parent repo and out to origin. Identical
shape and identical response to the `publish_workspace` Tauri command and MCP
`repo action=worktree_publish` — one implementation
(`worktree::publish_workspace_impl`) behind all three.

```json
{
  "parent_updated": true,
  "parent_error": null,
  "published_commit": "9f2a...",
  "origin_pushed": false,
  "origin_error": "the workspace has no 'origin' remote",
  "no_op_reason": null
}
```

The two steps report independently: the commits reaching the parent is worth
knowing even when origin is unreachable, and a failure there never rolls back
what already landed. The parent's branch is moved **fast-forward only** — with
two workspaces on one branch a divergent parent branch is normal, and forcing
would orphan whichever side published second. A branch the parent has checked
out is refused too, with the objects already staged under
`refs/tuic/published/<workspace-id>` so a retry costs no transfer.

`no_op_reason` is set for a linked worktree, whose refs are shared with the
parent already.

### Count Unpublished Commits

```
GET /worktrees/unpublished?repoPath=/path&workspaceId=feature-x~a1b2c3d4
```

Answers with the number of commits that exist **only** in that workspace —
reachable from HEAD and from no remote and no mirrored parent ref. Always `0`
for a linked worktree, whose objects live in the parent and outlive the
directory, so the caller pays nothing to ask about one.

The parent mirror is refreshed before counting, so a commit published a moment
ago does not still read as unpublished; that costs a fetch per call, which is
why the UI counts once per panel open rather than on every repository refresh.
A refresh that fails returns an error. The caller must treat that as unknown
and refuse removal: stale refs can under-count as well as over-count.

Same implementation as the `count_unpublished_commits` Tauri command and MCP
`repo action=worktree_unpublished`, and the same number the removal gate and
the confirmation dialog use.

### Workspace Lifecycle Preflight

```
GET /worktrees/lifecycle?repoPath=/path&workspaceId=feature-x~a1b2c3d4
```

Returns a fresh `{ dirty, commit_status, unpublished_commits,
removal_safety, error? }` verdict for one exact workspace. `commit_status` is
`unmerged`, `unpublished`, `published`, `merged`, or `unknown`; published is
recoverable but not necessarily integrated into the default branch.
`removal_safety` is `safe`, `requires_force`, or `unknown`. An inspection
failure is returned as an `unknown` verdict and must never be treated as zero or
safe. This is the HTTP twin of `get_workspace_lifecycle`.

### Generate Worktree Name

```
POST /worktrees/generate-name
Content-Type: application/json

{ "existing_names": ["name1", "name2"] }
```

Returns a unique worktree name.

### Finalize Merged Worktree

```
POST /worktrees/finalize
Content-Type: application/json

{ "repoPath": "/path/to/repo", "workspaceId": "feature-x", "action": "archive", "force": false }
```

Finalizes a merged worktree, addressed by workspace id. The merge already
happened, so no branch is needed here — only which checkout to dispose of. `action` must be `"archive"` (moves to archive directory) or `"delete"` (removes worktree and branch).
For `action: "delete"`, the response includes `branch_delete_warning` when the worktree was removed but safe branch deletion failed, for example because the branch has unmerged commits.

`force` (optional, default `false`) skips the dirty-worktree gate. Both actions end in `git worktree remove --force`, so a worktree that is **not known to be clean** comes back as `{ "action": "needs_confirmation", "merged": true }` without touching anything — ask the user, then re-send with `"force": true`. A dirty check that fails to run blocks the same way (`worktree_dirty` stays `false`, because git never reported "dirty"). This route shares `finalize_merged_worktree_impl` with the Tauri command, so both transports pass the identical gate.

### Run Setup Script

```
POST /worktrees/run-script
Content-Type: application/json

{ "script": "pnpm install", "cwd": "/path/to/worktree" }
```

Runs the script through `sh -c` (Unix) or `cmd /C` (Windows) in `cwd` and returns
exit code plus captured output. `cwd` accepts `~`. A `cwd` that does not exist is a
500 with `{ "error": ... }`. Same body as the `run_setup_script` command.

### Remove Worktree

```
DELETE /worktrees/:workspaceId?repoPath=/path&deleteBranch=true
```

Query parameters:
- `repoPath` (required) -- base repository path
- `deleteBranch` (optional, default `true`) -- when `true`, also deletes the local git branch
- `force` (optional, default `false`) -- when `true`, permits discarding dirty or unpublished COW state, or uses forced linked-worktree removal and branch deletion

The path segment is the opaque workspace id from `GET /worktrees/paths`, not a branch name.

Returns `{ "ok": true, "branch_delete_warning": null }` on full success. A
non-forced COW removal refuses staged, unstaged, or untracked changes before it
checks for unpublished commits; deleting an independent clone must not silently
discard either class of work. When `deleteBranch=true` and `git branch -d`
refuses to delete the branch after a linked worktree is removed, the request
still succeeds with `branch_delete_warning` set so clients can report the
partial outcome.

## Push Notification Endpoints

### Get VAPID Public Key

```
GET /api/push/vapid-key
```

Returns the VAPID public key for `PushManager.subscribe()`. No authentication required.

**Response:** `{ "publicKey": "<base64url>" }`

Returns 404 if push is not enabled.

### Subscribe

```
POST /api/push/subscribe
Content-Type: application/json

{ "endpoint": "https://...", "keys": { "p256dh": "...", "auth": "..." } }
```

Register a push subscription. Idempotent (same endpoint updates keys).

Push delivery is gated by desktop window focus: notifications for `question` and session completion events are sent whenever the desktop window is **not** focused (including when the app is minimized or the user is on another workspace). This avoids duplicate alerts while the user is actively at the desktop, and still wakes the PWA service worker when the phone is locked.

### Unsubscribe

```
DELETE /api/push/subscribe
Content-Type: application/json

{ "endpoint": "https://..." }
```

Remove a push subscription by endpoint.

## ACP (ego)

Every `acp_*` Tauri command has a route here, with the same request field
names, the same response body, and the same error body — an `AcpClientError`
serialized whole (`code`, `message`, `connectionId`, `sessionId`, `operation`,
`retryable`). The HTTP status is a translation of `code`, never a second
opinion about it:

| `code` | Status |
|--------|--------|
| `invalid_input` | 400 |
| `not_found` | 404 |
| `capability_unavailable` | 501 — the agent never advertised it; the caller did nothing wrong |
| `transport_closed`, `stream_gap` | 410 — what the URL names is genuinely gone |
| `unsupported_protocol`, `initialization_failed`, `agent_error` | 502 |
| `protocol_violation` | 502 — the agent denied a method it advertised; the connection is now failed |

```
POST   /acp/connections                                          {root}                    -> AcpConnectionSnapshot
GET    /acp/connections/{connection_id}                                                    -> AcpConnectionSnapshot
DELETE /acp/connections/{connection_id}                                                    -> AcpConnectionSettlement
POST   /acp/connections/{connection_id}/kill                                               -> AcpConnectionSettlement
POST   /acp/connections/{connection_id}/reconnect                {root}                    -> AcpConnectionSnapshot
GET    /acp/connections/{connection_id}/sessions?cwd=&cursor=                              -> ListSessionsResponse
POST   /acp/connections/{connection_id}/sessions                 {authority}               -> AcpAttachmentSnapshot
POST   /acp/connections/{cid}/sessions/{session_id}/load         {authority}               -> AcpAttachmentSnapshot
POST   /acp/connections/{cid}/sessions/{session_id}/resume       {authority}               -> AcpAttachmentSnapshot
POST   /acp/connections/{cid}/sessions/{session_id}/fork         {authority}               -> AcpAttachmentSnapshot
DELETE /acp/connections/{cid}/sessions/{session_id}                                        -> null
POST   /acp/connections/{cid}/sessions/{session_id}/close                                  -> null
POST   /acp/connections/{cid}/sessions/{session_id}/prompt       {prompt:[ContentBlock]}   -> AcpTurnId
POST   /acp/connections/{cid}/sessions/{session_id}/cancel                                 -> null
POST   /acp/connections/{cid}/sessions/{session_id}/config       {configId, value}         -> [SessionConfigOption]
POST   /acp/connections/{cid}/sessions/{session_id}/pause        {requestId}               -> EgoHoldResponse
POST   /acp/connections/{cid}/sessions/{session_id}/resume-turn  {requestId}               -> EgoHoldResponse
POST   /acp/connections/{cid}/sessions/{session_id}/compact      {requestId}               -> EgoCompactResponse
GET    /acp/connections/{connection_id}/interactions                                       -> [AcpPendingInteraction]
POST   /acp/connections/{cid}/permissions/{request_id}/response  {outcome}                 -> AcpInteractionSettlement
POST   /acp/connections/{cid}/elicitations/{request_id}/response {action}                  -> AcpInteractionSettlement
```

`POST /acp/connections` and `.../reconnect` are the only two that launch a
process, and they are the only two that take the loopback-or-authenticated
guard. The executable is never in the body: it comes from the `ego_executable`
setting.

### Stream (WebSocket)

```
GET /acp/connections/{connection_id}/stream?after={sequence}     (WebSocket)
```

The browser half of `acp_subscribe`. Frames are `{"kind":"event",...}`,
`{"kind":"gap",...}` or `{"kind":"end"}` — byte-identical to what the desktop
Channel carries. `after` is the first sequence wanted; `0` means everything the
journal still holds. A cursor the journal has dropped is refused **before** the
upgrade, with a 404 and the usual error body, rather than by opening a socket
and closing it.

A gap is terminal: the missing frames exist nowhere in this client, and a
stream that silently resumed past them would render a turn with a hole in it.
Recovery is a fresh connection and `session/load`.

This is deliberately not on `/events`: one turn emits more frames per second
than the 256-entry SSE broadcast can carry without lagging every other
subscriber. `/events` carries only the low-frequency `acp-notice` wake signal
(`ready`, `settled`, `interaction_pending`, `interaction_settled`), whose
payload names the connection, generation, sequence and — when it has one — the
session and request it is about.

## Tauri-Only Commands (No HTTP Route)

The following commands are accessible only via the Tauri `invoke()` bridge in the desktop app. They have no HTTP endpoint.

| Command | Module | Description |
|---------|--------|-------------|
| `get_claude_usage_api` | `claude_usage.rs` | Fetch rate-limit usage from Anthropic OAuth API |
| `get_claude_usage_timeline` | `claude_usage.rs` | Get hourly token usage timeline from session transcripts |
| `get_claude_session_stats` | `claude_usage.rs` | Scan session transcripts for aggregated token/session stats |
| `get_claude_project_list` | `claude_usage.rs` | List Claude project slugs with session counts |
| `plugin_watch_path` | `plugin_fs.rs` | Start watching path for changes (change events need AppHandle/WS) |
| `plugin_unwatch` | `plugin_fs.rs` | Stop watching a path |
| `plugin_read_credential` | `plugin_credentials.rs` | Read credential from system store |
| `fetch_plugin_registry` | `registry.rs` | Fetch remote plugin registry index |
| `install_plugin_from_zip` | `plugins.rs` | Install plugin from local ZIP file |
| `install_plugin_from_url` | `plugins.rs` | Install plugin from HTTPS URL |
| `uninstall_plugin` | `plugins.rs` | Remove a plugin and all its files |
| `get_agent_mcp_status` | `agent_mcp.rs` | Check MCP config status for an agent |
| `install_agent_mcp` | `agent_mcp.rs` | Install TUICommander MCP entry in agent config |
| `remove_agent_mcp` | `agent_mcp.rs` | Remove TUICommander MCP entry from agent config |
