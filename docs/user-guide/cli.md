# tuic CLI

The `tuic` command line tool lets you control TUICommander from the terminal. It combines the best of VS Code's `code` CLI, Zed's editor integration, and tmux's session management into a single binary.

## Installation

**From the app:** Settings > General > Command Line Interface > Install tuic CLI

**First launch:** TUICommander offers to install the CLI on first run.

**From the CLI itself:** `tuic install-cli`

The binary is installed to:
- **macOS:** `/usr/local/bin/tuic` (requires admin password)
- **Linux:** `/usr/local/bin/tuic` (requires sudo)
- **Windows:** `%LOCALAPPDATA%\Microsoft\WindowsApps\tuic.exe` (no admin needed)

The CLI auto-updates silently when TUICommander starts — no manual update needed.

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

# Kill a session
tuic kill <id-or-name>

# Resize a session
tuic resize <id-or-name> 120x40

# Pause/resume output
tuic pause <id-or-name>
tuic resume <id-or-name>
```

Session targets accept full UUIDs, ID prefixes (the short ID `tuic ls` prints), exact names, or a name prefix — case-insensitive. An ambiguous target is rejected rather than guessed.

### Sending keys

Each argument is either a **key name** or **literal text** — matched whole, never as a substring, so `tuic send build "Enter the room"` types the sentence instead of pressing Return mid-word. Adjacent literals are joined with a single space.

Key names: `Enter`, `Space`, `Tab`, `Escape`, `BSpace`, `Up`, `Down`, `Left`, `Right`, `Home`, `End`, `PageUp`, `PageDown`, and any `C-<letter>` (`C-c`, `C-d`, `C-u`, …).

## Agent Orchestration

```bash
# Spawn an AI agent (the prompt is required — the agent starts on it)
tuic agent spawn claude "review the failing tests"
tuic agent spawn codex "add a changelog entry" --repo /path/to/repo

# List running agents
tuic agent ls

# Send a message to an agent
tuic agent send <id> "fix the tests"
```

## tmux Compatibility

`tuic` can act as a drop-in replacement for tmux. When invoked as `tmux` (via symlink), it translates tmux commands to TUICommander equivalents.

### Setting Up the Alias

```bash
# Create tmux -> tuic symlink
tuic alias

# Remove the alias (restores original tmux if installed)
tuic alias --remove
```

### Supported tmux Commands

When invoked as `tmux`, the following commands are supported:

| tmux Command | Behavior |
|---|---|
| `tmux` | Create new session in cwd |
| `tmux new-session -s name` | Create named session |
| `tmux list-sessions` | List sessions |
| `tmux kill-session -t target` | Kill session |
| `tmux kill-server` | Kill all sessions |
| `tmux send-keys -t target "cmd" Enter` | Send input |
| `tmux capture-pane -t target` | Capture output |
| `tmux resize-pane -t target -x 120 -y 40` | Resize |
| `tmux attach-session` | Focus TUICommander window |
| `tmux has-session -t target` | Check if session exists (exit code) |

Key names are translated: `Enter`, `Space`, `Tab`, `Escape`, `C-c`, `C-d`, `C-z`, etc.

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
- **macOS/Linux:** Unix domain socket at `~/.config/com.tuic.commander/mcp.sock`
- **Windows:** Named pipe at `\\.\pipe\tuicommander-mcp`

Override with `$TUIC_SOCKET` environment variable.

If TUICommander is not running, `tuic open` and `tuic new` will launch it automatically.
