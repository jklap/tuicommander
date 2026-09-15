# tuic CLI

The `tuic` command line tool lets you control TUICommander from the terminal. It combines the best of VS Code's `code` CLI, Zed's editor integration, and tmux's session management into a single binary.

## Installation

**From the app:** Settings > General > TUIC CLI > Install TUIC CLI

**First launch:** TUICommander offers to install the CLI on first run.

**From the CLI itself:** `tuic install-cli`

The binary is installed to:
- **macOS:** `/usr/local/bin/tuic` (requires admin password)
- **Linux:** `/usr/local/bin/tuic` (requires sudo)
- **Windows:** `%LOCALAPPDATA%\Microsoft\WindowsApps\tuic.exe` (no admin needed)

The CLI auto-updates silently when TUICommander starts — no manual update needed.
Install and update stage the new executable beside the installed path, then rename it into place. A running `tuic` process keeps its original executable; installing over a symlink replaces the link without changing its target.

## Opening Files and Repos

```bash
# Open a file (launches TUICommander if not running)
tuic file.rs

# Open at specific line and column
tuic file.rs:42
tuic file.rs:42:10
tuic open --goto file.rs:42

# Open the current directory as a repo (adds it to the sidebar and activates it)
tuic .
tuic /path/to/project

# Open with --wait (for use as $EDITOR)
tuic open --wait file.rs

# Diff two files
tuic diff old.rs new.rs
```

A directory is treated as a **repo**, not as a terminal: it lands in the sidebar and becomes the active repo. A folder TUICommander does not know yet is confirmed once in the app before it is added — after that, `tuic .` activates it silently. Use `tuic new` when what you want is a shell.

### Temporary directories are refused

`tuic <dir>` registers the directory **permanently** — the sidebar keeps it across restarts. A temporary directory therefore becomes a row that points at something the operating system later deletes, and nothing records who added it. `tuic` refuses such a path instead:

```
$ tuic "$TMPDIR/scratch"
Refusing to register "/var/folders/.../T/scratch" as a repository: it is inside a
temporary directory, which the OS deletes while the sidebar entry stays behind forever.
For a shell there instead: tuic new /var/folders/.../T/scratch
```

A path is refused when it is at or below the temp root of **the shell you run `tuic` from** (`TMPDIR` on macOS and Linux, `TEMP`/`TMP` on Windows) or below `/tmp`. Running the check in the CLI rather than in the app is what makes a custom `TMPDIR` — for example the `~/Gits/.tmp` this repository's Rust suites use — count as temporary; the app process has a different one and cannot see yours.

Only registration is refused. `tuic new <dir>` still opens a shell in a temporary directory, because a session is disposable in the same way the directory is.

### Using as $EDITOR

```bash
export EDITOR="tuic open --wait"
git commit  # opens commit message in TUICommander
```

## Session Management

These commands mirror tmux semantics:

```bash
# List all sessions (short IDs; --json for scripts)
tuic ls
tuic ls --json

# Create a new session
tuic new
tuic new -n "my-session"
tuic new -n "build" /path/to/repo

# Create a session and run something in it
tuic run pnpm dev
tuic run -n "tests" cargo nextest run

# Send input to a session
tuic send <id-or-name> "make test" Enter

# Capture session output
tuic capture <id-or-name>
tuic capture <id-or-name> -n 50          # last 50 lines
tuic capture <id-or-name> --format raw

`capture` supports `raw` and `text`; `--format log` is not supported by the
CLI and returns an error.

# Kill a session
tuic kill <id-or-name>

# Resize a session
tuic resize <id-or-name> 120x40

# Pause/resume output
tuic pause <id-or-name>
tuic resume <id-or-name>
```

Session targets are resolved by the server. Use the PTY ID, its stable
`tuic_session`, the terminal alias (for example `tu-33`), a unique short PTY-ID
prefix, or a unique display name; an ambiguous target is rejected rather than
guessed.

### Sending keys

Each argument is either a **key name** or **literal text** — matched whole, never as a substring, so `tuic send build "Enter the room"` types the sentence instead of pressing Return mid-word. Adjacent literals are joined with a single space.

Key names: `Enter`, `Space`, `Tab`, `Escape`, `BSpace`, `Up`, `Down`, `Left`, `Right`, `Home`, `End`, `PageUp`, `PageDown`, and any `C-<letter>` (`C-c`, `C-d`, `C-u`, …).

## Agent Orchestration

### Generic MCP calls

Use `tuic mcp <tool> [<json>|-]` to call a native MCP tool over the local
socket. Omit the JSON argument for `{}`, or pass `-` to read a JSON object from
stdin. The tool's text payload is printed unchanged with a trailing newline,
so it can be piped to `jq`:

```bash
tuic mcp session '{"action":"list"}' | jq length
tuic mcp agent '{"action":"wait","timeout_ms":8000}'
cat <<'JSON' | tuic mcp agent -
{"action":"send","to":"peer-id","message":"it's ready"}
JSON
```

Tool and protocol errors go to stderr with exit code 1. Invalid JSON or CLI
arguments exit 2 before connecting. Managed callers send `$TUIC_SESSION` for
peer binding; callers outside TUICommander register an external identity for
that invocation.

### Detached commands

Run `tuic bg <log> -- <cmd> [args...]` from a managed terminal to return at
once while the command runs in a separate process group. The command's stdout
and stderr append to `<log>`, and its exit code is written to `<log>.exit`.
When it finishes, `tuic` requests a `BG DONE exit=<code> log=<log> cmd=…`
wake for the originating session. If that session is busy, the wake is queued
until it becomes idle. `<log>.wake` records the request outcome as JSON:
`queued`, `mailed`, `retrying`, or `failed`, with `tuic_session` and the number
of queue attempts. A queue lookup or request failure falls
back to MCP agent mail addressed to the same `TUIC_SESSION`; that mail includes
the `BG DONE` text and the queue error. `queued` means the queue took the
request, not that the agent later submitted it. `mailed` means the mail was
surfaced to the caller; inbox-only mail remains a failure. If both paths fail
after a transient socket error or server error, the runner retries up to six
queue-then-mail attempts with exponential delays of 0.2–3.2 seconds (6.2
seconds of total retry waiting). A permanent queue rejection ends the retry
immediately after mail fails. `retrying` records an attempt still in progress;
`failed` records the last queue and mail errors. Each IPC read has its own
three-second deadline, so a slow or unavailable server can extend the elapsed
time beyond the retry waiting period. The command works
on macOS, Linux, and Windows.

```bash
tuic bg "$HOME/Gits/.tmp/build.log" -- make check
```

On Windows, the detached runner does not inherit the launcher's input or output pipes. Capturing the launcher output therefore returns before the background command completes.

Each background runner generates one completion key and reuses it on queue
retries. The backend remembers the last 128 accepted keys per live terminal,
so a lost reply does not enqueue the same completion again within that window.
A recognized acceptance remains `queued` in the wake file even after the queue
has drained. If every queue receipt is lost and mail also fails, the wake file
reports `uncertain`, not a proven delivery failure; inspect backend state before
retrying manually. `uncertain` also prevents automatic child closure.

`TUIC_SESSION` is required; without it, `tuic bg` exits 2 before starting a
command. The launcher prints the wake-status path and removes stale `.exit`
and `.wake` files before detaching. If no `BG DONE` arrives, inspect `.exit`
for command completion and `.wake` for both queue and mail failures. Both
errors are also appended to the log. Neither status file can start a new agent
turn while TUICommander is unavailable.

The instance config directory also holds `bg-wakes/<TUIC_SESSION>.json`. It is
marked `retrying` while a background command is active or its wake is being
retried, then records the final result. Automatic managed-child idle closure
reads this record and keeps a child open after a failed wake.

```bash
# Spawn an AI agent (the prompt is required — the agent starts on it)
tuic agent spawn claude "review the failing tests"
tuic agent spawn codex "add a changelog entry" --repo /path/to/repo
tuic agent spawn codex "review" --name reviewer --model gpt-5 --args exec --args --full-auto \
  --cwd /path/to/repo --rows 40 --cols 120 --json

# List running agents
tuic agent ls

# Deliver a message to a registered peer's INBOX (peer registry)
tuic agent send <peer-uuid> "fix the tests"

# Type a prompt into an agent's TERMINAL and submit it (no peer routing)
tuic agent type <id-or-name> "fix the tests"

# Wait for mail or inspect peers without polling
tuic agent wait --timeout-ms 60000 --json
tuic agent inbox --json
tuic agent list-peers --path /path/to/repo --json
tuic agent stats --json

# Server-owned session state and output
tuic session status <id-or-name> --json
tuic session wait <id-or-name> --until idle --timeout-ms 60000 --json
tuic session output <id-or-name> --limit 50 --json

# Server-owned worktree lifecycle
tuic repo worktree-list /path/to/repo --json
tuic repo worktree-create /path/to/repo --branch feature/task --spawn-session --json
tuic repo worktree-remove /path/to/repo <branch> --json
```

`agent wait` and `session wait` size their IPC read timeout from `--timeout-ms`
(60 seconds by default) with a five-second transport margin. MCP worktree
creation and removal allow 305 seconds on Unix, four seconds beyond the server's
301-second request limit. Other Unix CLI requests retain a three-second read
timeout, so a stalled app fails promptly. A timed-out MCP action may still
complete on the server; inspect its state before retrying.

The orchestration commands above call the same MCP tools as an agent. They use
the local `mcp.sock` transport and send `$TUIC_SESSION` as `x-tuic-session`, so
a child spawned from a managed terminal records that terminal as its parent.
Outside TUICommander, `tuic` prints one notice and registers a headerless
external MCP caller. That caller is the parent of a child it spawns, but its
identity lasts only for that `tuic` invocation, so a later `tuic` call cannot
read the mail the child sends back.
Use `--json` for the unmodified server payload. A server error is printed to
stderr and makes `tuic` exit non-zero.

### Two delivery channels, chosen explicitly

`tuic agent send` and `tuic agent type` are not interchangeable, and neither
guesses which one you meant.

| Command | Route | Target | Use when |
|---|---|---|---|
| `tuic agent send` | peer registry → recipient inbox | a **registered peer's** `tuic_session` UUID | the recipient is an orchestrator or any peer, including one with no terminal of its own |
| `tuic agent type` | PTY write | a session **ID or name** | you want the text to appear in a terminal and be submitted |
| `tuic send` | PTY write | a session ID or name | raw keys, no agent framing (see *Sending keys*) |

`tuic agent send` is the CLI counterpart of the MCP `agent action=send` tool and
uses the same delivery path, so both report the same `delivery_path` and both
land the payload exactly once. It exits non-zero — with the registry's own
message — when the recipient is not registered or the message is empty.

Acceptance is not delivery, and the output says which one you got:

```
Delivered to <peer> (sse_channel_and_inbox)
```

means something surfaced the message — a waiter, the SSE channel, or the
recipient's terminal. Whereas:

```
Buffered for <peer> (inbox_only) — unread until the recipient polls its inbox
warning: Recipient has NO terminal and no active wait: nothing will wake it. …
```

means the registry took the message but nothing will wake the recipient: it sits
unread until that peer calls `agent action=wait`/`inbox`. Both exit 0, because
the registry accepted the message in both cases — do not block on an answer
after a `Buffered` line. The CLI validates this current report contract through
`message_id`, `delivered`, and `delivery_path`; it does not require the removed
`accepted` compatibility field.

`tuic agent type` keeps the agent-safe framing: the text and the Enter are sent
as **separate** PTY writes, because a raw-mode Ink TUI treats a combined
`text\r` as a prefill and leaves it unsent. `tuic send` does not do this.

`tuic agent send` uses `$TUIC_SESSION` when it runs inside a TUICommander
session. Outside one, it registers a headerless external caller for the MCP
connection and prints a notice on stderr. It has no terminal, automatic wake-up,
or managed parent relationship; a spawned child cannot reply to it.

## tmux Compatibility

`tuic` can act as a drop-in replacement for tmux. When invoked as `tmux` (via symlink), it
translates tmux commands to TUICommander equivalents — including the pane/window/session graph
Claude Code's teammate-pane backend drives when its `teammateMode` selects tmux (see
`tmux-swarm-shim.md` at the repo root for the investigation this was built from).

### Setting Up the Alias

```bash
# Create tmux -> tuic symlink
tuic alias

# Remove the alias (restores original tmux if installed)
tuic alias --remove
```

### Supported tmux Commands

When invoked as `tmux`, the following commands are supported. Global options (`-L <name>`,
`-S <path>`) may precede any subcommand, exactly as with real tmux — `-L`/`-S` partition state so
two concurrent tmux-driving processes (e.g. two Claude Code instances) never see each other's
panes.

| tmux Command | Behavior |
|---|---|
| `tmux -V` | Print a plausible version string, exit 0 — a pure capability probe, answered without contacting TUICommander |
| `tmux` | Create new session in cwd |
| `tmux new-session -s name [-n window] [-P -F fmt]` | Create a named session (and its initial window/pane); `-P -F` prints the rendered format instead of the legacy `name: id` line |
| `tmux new-window -t target [-n name] [-P -F fmt]` | Add a window to an existing session |
| `tmux split-window -t target [-c cwd] [-P -F fmt]` | Add a pane to an existing window — the actual mechanism behind a new teammate |
| `tmux respawn-pane -k -t target -- cmd` | Deliver a real command into a pane that was created running a placeholder |
| `tmux list-sessions` | List sessions |
| `tmux list-windows [-t target] [-F fmt]` | List a session's windows |
| `tmux list-panes [-t target] [-F fmt]` | List a window's panes |
| `tmux display-message -p fmt` | Render a `#{...}` format string to stdout |
| `tmux select-pane -t target -T title` | Rename a pane's tab |
| `tmux kill-pane -t target` | Close one pane |
| `tmux kill-session -t target` | Kill session |
| `tmux kill-server` | Kill all sessions (and all tracked tmux topology) |
| `tmux send-keys -t target "cmd" Enter` | Send input |
| `tmux capture-pane -t target` | Capture output |
| `tmux resize-pane -t target -x 120 -y 40` | Resize |
| `tmux attach-session` | Focus TUICommander window |
| `tmux has-session -t target` | Check if session exists (exit code) |
| `tmux set-option -p -t target window-style\|pane-border-style\|pane-active-border-style fg=<color>` | Sets the pane's real accent color (Claude Code's per-teammate `--agent-color`) — rendered as a sidebar tab marker and a terminal pane border. `colourN`/`color#rrggbb` and the 8 standard ANSI names are all understood; other `set-option` names remain cosmetic no-ops (see below) |
| `tmux select-layout -t target tiled\|main-vertical` | Arranges that window's materialized panes into an actual TUICommander split view (the same mechanism as manually pressing Cmd+\\) — each pane remains its own independent sidebar tab; only the terminal area's current split arrangement changes. `main-vertical` is unreachable from TUIC's environment today (see `tmux-swarm-shim.md`) but supported for forward-compat |
| `tmux select-layout` (any other layout name), `set-option`/`set-window-option` (any other option name), `switch-client`, `rename-window` | Accepted and succeed with no effect — cosmetic in real tmux, no TUIC-tab equivalent |

Key names are translated: `Enter`, `Space`, `Tab`, `Escape`, `C-c`, `C-d`, `C-z`, etc.

`-t` targets accept the usual tmux forms: a pane id (`%3`), window id (`@1`), session id (`$0`),
a bare session name, or a compound `session:window.pane` path. A target that doesn't resolve
against tracked tmux topology falls back to the same session name/uuid/prefix matching `tuic send`
and friends already use outside tmux mode.

Panes created by `new-session`/`new-window` don't spawn a terminal tab until something actually
writes to them (`send-keys`, `respawn-pane`, a rename) — this keeps a freshly-created, unused
session from cluttering the sidebar. `split-window` is the exception: it materialises immediately,
since a split is always about to be used. Topology (the pane/window/session graph itself, as
opposed to the TUIC tabs it points at) lives only in memory and does not survive an app restart —
a swarm can't outlive its lead process anyway, so this is by design, not a limitation to work
around.

### Invocation logging

Every `tmux ...` call is logged — including ones this compatibility layer doesn't recognize, which
is the main way to discover what a caller like Claude Code's teammate-pane backend actually
invokes. Logged to `<config dir>/logs/tmux-shim.log` (size-rotated at 1 MiB, one previous
generation kept) and, best-effort, to the running instance's own logs via `POST /logs` (source
`tmux-shim`, readable via `GET /logs?source=tmux-shim`) — the file sink is the one that still
works when TUICommander isn't running. Disable with `TUIC_TMUX_LOG=0`; set `TUIC_TMUX_LOG=stderr`
to also mirror every line to stderr.

## System Commands

```bash
# Check TUICommander status — version, session/agent counts, and which
# sessions are waiting on you right now
tuic status

# Install CLI to system PATH
tuic install-cli
tuic install-cli --path /custom/path

# Create/remove tmux alias
tuic alias
tuic alias --remove
```

## IPC Architecture

The CLI communicates with TUICommander via IPC:
- **macOS/Linux:** Unix domain socket at `<platform config dir>/com.tuic.commander/mcp.sock` for the default instance.
- **Windows:** Named pipe at `\\.\pipe\tuicommander-mcp`

Use `tuic --instance <id> <command>` or `TUIC_APP_INSTANCE=<id> tuic <command>`
to select the same namespace as `tuic-remote --instance <id>`. An explicit
`--instance` takes precedence over the environment. Invalid ids fail before any
connection. Named Unix sockets use the server's short SHA-256-based name in the OS
temp directory. The bridge accepts `tuic-bridge --instance <id>` and the same
environment variable; it searches only that instance's alternative sockets.

On Unix, `TUIC_SOCKET` overrides the selected socket path. Windows retains the
existing named-pipe endpoint. The clients and server share instance validation,
config paths and IPC names through `tuic-ipc`; CLI and bridge also share HTTP
request and response framing.

If TUICommander is not running, `tuic open` and `tuic new` will launch it automatically.
