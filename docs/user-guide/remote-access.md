# Remote Access

Access TUICommander from a browser on another device on your network.

## Setup

1. Open **Settings** (`Cmd+,`) → **Services** → **Remote Access**
2. Configure:
   - **Port** — Default `9876` (range 1024–65535)
   - **Username** — Basic Auth username
   - **Password** — Basic Auth password (stored as a bcrypt hash, never in plaintext)
3. Enable remote access

Once enabled, the settings panel shows the access URL: `http://<your-ip>:<port>`

## Connecting from Another Device

1. Open a browser on any device on the same network
2. Navigate to the URL shown in settings (e.g., `http://192.168.1.42:9876`)
3. Enter the username and password you configured
4. TUICommander loads in the browser with full terminal access

### QR Code

The settings panel shows a QR code for the access URL — scan it from a phone or tablet to connect quickly. The QR code uses your actual local IP address.

## What Works Remotely

The browser client provides the same UI as the desktop app:

- Terminal sessions (via WebSocket streaming)
- Sidebar with repositories and branches
- Diff, Markdown, and File Browser panels
- Keyboard shortcuts
- Compose commands queued through the same agent idle gate as the desktop app;
  clearing the Compose queue leaves pending peer messages intact
- Notification sounds, including the distinct G4→G4→E5 Attention callback,
  through the browser audio fallback

**The terminal stream is compressed over a remote link, and only over one.** A
remote browser asks the daemon to deflate the WebSocket frames carrying the
terminal, which takes a repainting agent's traffic to a few percent of its size —
measured at 5.8% over a real session. A browser on the same machine asks for
nothing and pays nothing, and a client reaching the daemon through an SSH tunnel
is compressed by the tunnel instead (see **Compression** under SSH Tunnels below),
never twice. Nothing to configure: the client decides from whether the session
belongs to a remote connection. An older browser that cannot inflate simply reads
the uncompressed stream, and so does a new browser talking to a daemon too old to
compress — the daemon has to accept the request on the handshake before the
browser uses it, so the two can never disagree about what is on the wire. The
exact framing is in
[`docs/api/http-api.md`](../api/http-api.md).

## Security

- **Authentication** — Basic Auth with bcrypt-hashed passwords
- **Secret storage** — Session tokens, relay bearer tokens, and push VAPID
  private keys are stored in the OS keyring-backed credential vault; config
  files and `/config` responses expose only non-secret settings and existence
  flags
- **Local network only** — The server binds to your machine's IP; it's not exposed to the internet unless you configure port forwarding (don't do this without a VPN)
- **CORS** — When remote access is enabled, any origin is allowed (necessary for browser access from different IPs)

## MCP HTTP Server

Separate from remote access, TUICommander runs an **HTTP API server** for AI tool integration:

- The server always listens on an IPC listener: Unix domain socket at `<config_dir>/mcp.sock` on macOS/Linux, or named pipe `\\.\pipe\tuicommander-mcp` on Windows
- AI agents connect via the `tuic-bridge` sidecar binary, which translates MCP stdio transport to the IPC listener
- Bridge configs are auto-installed on first launch for supported agents (Claude Code, Cursor, Windsurf, VS Code, Zed, Amp, Gemini, Codex, Grok, opencode, Droid, goose, pi) — and only for the ones present on the machine, so TUICommander never creates a config directory for a tool you do not have. On every subsequent launch, the bridge path is verified and updated if stale (from reinstalls, updates, or moves)
- The `mcp_server_enabled` toggle in **Settings** → **Services** controls whether MCP protocol tools are exposed, not the server itself
- Shows server status and active session count in settings
- Local MCP callers submit one managed-agent command with `session action=submit`; the same response reports child terminal movement or a precise timeout, so callers must not split text/Enter or poll afterward. Raw `session action=input` remains write-only. Mutating session actions, including `submit`, are not exposed to non-loopback MCP clients

The Unix socket is accessible only to the current user (filesystem permissions) and requires no authentication — it's designed for local tool integration, not remote access.

## Mobile Companion

TUICommander includes a phone-optimized interface for monitoring agents from your phone.

### Accessing the Mobile UI

1. Enable remote access (see Setup above)
2. Navigate to `http://<your-ip>:<port>/mobile` from your phone
3. Log in with your credentials

### Add to Home Screen

The mobile UI supports PWA (Progressive Web App) installation:

- **iOS Safari**: Tap Share → "Add to Home Screen"
- **Android Chrome**: Tap the three-dot menu → "Add to Home screen"

The app launches in standalone mode (no browser chrome) for a native-like experience.

### Mobile Features

- **Sessions list** — See all running agents with status (idle, busy, question, rate-limited, error)
- **Session detail** — Live output streaming, quick-reply chips (Yes/No/Enter/Ctrl-C), text input
- **Question banner** — Instant notification when any agent needs input, with quick-reply buttons
- **Activity feed** — Chronological event feed grouped by time
- **Notification sounds** — Audio alerts for questions, errors, completions, and rate limits

### Tips

- Pull down on the sessions list to refresh
- The question banner appears on all screens — you don't need to be on the sessions tab to respond
- Sound notifications can be toggled in the mobile Settings tab

## SSH Tunnel Management

TUICommander can manage persistent SSH tunnels with automatic reconnection, port forwarding, and audit logging.

### Creating a Tunnel Profile

1. Open **Settings** (`Cmd+,`) → **Services** → **SSH Tunnels**
2. Click **Add Tunnel** to open the editor
3. Configure:
   - **Name** — A descriptive label (e.g., "prod-db-tunnel")
   - **Host** — Remote SSH host
   - **Port** — SSH port (default 22)
   - **User** — SSH username
   - **Identity File** — Optional path to SSH private key (use the Browse button to select)
   - **Port Forwards** — Local or remote port forwarding rules (e.g., local 8080 → remote 80). Local forwards target `remote_host`/`remote_port`; Remote forwards target `local_host`/`local_port`. The remote host is pre-populated from the tunnel host when adding a Local forward
   - **Options** — ServerAliveInterval (default 15s), ServerAliveCountMax (default 3), StrictHostKeyChecking (Yes or AcceptNew), Compression (default on)
4. Save the profile

**Compression** is `ssh -C` on the tunnel channel, and it is on by default because
a tunnel usually carries a terminal stream, which deflates to a few percent of
itself. Turn it off for a tunnel to a machine on the same LAN, where the link is
fast and the CPU is the scarcer thing. Profiles written before this option existed
keep compressing.

Tunnel profiles are stored as TOML files. **Global profiles** live in `<config_dir>/tunnels/` and are available across all repos. **Per-repo profiles** are stored in `<repo>/.tuic/tunnels/` and override global profiles with the same ID.

### Auto-Connect

Enable **Auto-Connect** on a tunnel profile to have it start automatically when TUICommander launches. Useful for tunnels you always need (database access, internal services).

Toggle auto-connect in the tunnel editor — profiles marked with auto-connect are started during app hydration before you interact with the UI.

### Statusbar Indicator

The status bar shows a shield icon for SSH tunnels:

- **Grey shield** — You have tunnel profiles configured but none are currently connected
- **Green shield with badge** — Shows the number of active tunnel connections

Click the shield to open the Tunnels Panel.

### Command Palette

Open the command palette (`Cmd+P` / `Ctrl+P`) and type "tunnels" to toggle the Tunnels Panel without navigating to Settings.

### Starting and Stopping Tunnels

- In the **Tunnels Panel**, click the **Start** button next to a profile to launch the SSH tunnel
- The **TunnelStatusBadge** shows the current state: Starting, Connected, Reconnecting, Stopped, or Error
- Click **Stop** to gracefully terminate the SSH process (SIGTERM with 5s grace period, then SIGKILL)
- On app exit, all active tunnels are automatically stopped — no orphaned SSH processes

### SSH Agent Detection

TUICommander automatically detects your SSH agent and shows the agent type and loaded keys in the tunnel editor. Supported agents:

- **1Password** — Detected via the 1Password SSH agent socket
- **Secretive** — Detected via the Secretive agent socket
- **GPG Agent** — Detected via gpg-agent socket
- **Generic SSH Agent** — Any other `SSH_AUTH_SOCK` value

The key listing shows fingerprint, comment, and key type for each loaded key, helping you verify that the correct identity is available before connecting.

### Automatic Reconnection

When a tunnel disconnects due to a network issue or timeout, the supervisor automatically reconnects with exponential backoff:

- Base delay: 1 second, doubling each attempt
- Maximum delay: 30 seconds
- Jitter: +/-25% to prevent thundering herd
- Maximum retries: 10 before giving up
- Backoff resets on successful connection

Non-retryable failures (authentication errors, host key mismatches) stop immediately without retry.

### Audit Log

All tunnel events (start, connect, disconnect, error, retry, stop) are recorded in a SQLite database with WAL mode for performance. The audit log supports:

- Querying events by tunnel ID
- Querying events by time range
- Automatic rotation of old events (configurable retention period)

### Exit Classification

The supervisor classifies SSH process exits to determine whether retry is appropriate:

| Exit Reason | Retryable | Description |
|-------------|-----------|-------------|
| AuthFailed | No | Permission denied or authentication failure |
| HostKeyMismatch | No | Remote host key changed |
| PortInUse | No | Local forwarding port already bound |
| ConnectionRefused | Yes | Remote host rejected the connection |
| NetworkDown | Yes | Network unreachable |
| Timeout | Yes | Connection timed out |
| UserKilled | No | Process terminated by user signal |

## Remote Connection Manager

Remote connections let you manage `tuic-remote` daemons running on other machines. TUICommander routes API calls to the correct host based on which repo/session is active.

### Adding an SSH Connection

1. Open **Settings** → **Connections** → **Add Connection**
2. Select **SSH** transport
3. Configure host, port (default 22), user, and optional identity file
4. Set the remote daemon port (default 9877)
5. Set the auth username and password the daemon was configured with
6. Save — an SSH tunnel is automatically created to forward the daemon port

### Adding a Direct Connection

1. Open **Settings** → **Connections** → **Add Connection**
2. Select **Direct** transport
3. Enter the URL of the remote daemon (e.g., `http://10.0.0.5:9877`)
4. Set the auth username and password
5. Save — health polling begins immediately

A URL that points back at the TUICommander you are configuring is refused with
"this very TUICommander instance — a machine cannot mirror itself". The check
compares the `instance_id` in `GET /health` against this process's own, so a
second daemon on the same machine (a different port) is still a valid peer.

### Authentication

`tuic-remote` authenticates **every** TCP request. The headless build has no
loopback bypass and `run_remote` forces `lan_auth_bypass` off, so an SSH tunnel
does not make the daemon local: `GET /health` is the only unauthenticated route.
A connection without credentials therefore reaches `/health` and nothing else.

The password is kept in the OS credential vault, keyed by the connection's UUID.
`connections.json` holds the username only, and neither the vault entry nor the
daemon's token ever appears in `GET /config`. Deleting a connection deletes its
vault entry with it.

On connect, TUICommander trades the password for the daemon's session token over
`GET /api/auth/session-token` (Basic Auth), then puts that token in the query
string of every call — HTTP, the terminal WebSocket and the `/events` SSE
stream alike. It has to be the query string: a WebSocket upgrade cannot carry an
`Authorization` header, and the daemon answers `Access-Control-Allow-Origin: *`,
which rules out credentialed cookies. The token lives in the daemon's memory and
is never written to disk, so the client re-fetches it on every connect and after
a daemon restart.

Status tells the two failures apart:

| Status | What it means |
|---|---|
| **Connected** | A route behind the auth middleware answered 200 |
| **Not authenticated** | The daemon is reachable and `/health` answers, but the credentials were rejected. Fix the username or password — not the network |
| **Error** | The daemon is unreachable, or the tunnel failed |

An unauthenticated connection routes no traffic at all: no terminals, no repo
calls, no event bridge. `/health` passing is not evidence that anything else
will work, which is why it is not what the status is read from.

### Where the connection runs

The connection itself — the health probe, the token exchange, the five-second
status poll, the SSH tunnel — runs in the TUICommander backend, not in the
window. Three consequences you can observe:

- **Every client sees the same status.** Connecting from the desktop app shows
  up in a browser tab pointed at the same TUICommander, and the other way round.
  The status is pushed as a `remote-connection-status` event.
- **A daemon restart is recovered with the window closed.** The poll notices the
  token it holds is no longer the daemon's, re-authenticates once, and the
  connection stays Connected without anyone looking at it.
- **Nothing is left behind.** The SSH tunnel a connection needs is built in
  memory; it never appears as a saved profile in the Tunnels panel, and quitting
  the app cannot orphan one.

### A remote tab behaves like a local one

While a connection is up, the backend follows the remote daemon's own event
stream and repeats every event locally under the same name. A session on the
other machine therefore raises the same idle / busy dot, the same question
badge, the same notification and the same queued-command gate as a session on
this one — there is no separate remote code path to fall behind.

The remote sessions are listed beside the local ones, each tagged with the
connection that owns it. Losing the connection announces them closed and drops
them, so a badge cannot freeze on the last state the machine was in.

### Remote Repositories and Terminals

Once a remote connection is configured:

- **Add remote repo** — When adding a repository, select a connection. The repo appears in the sidebar with a remote badge
- **Open terminal** — Terminals on remote repos connect via WebSocket to the remote daemon. I/O works identically to local terminals
- **Health monitoring** — Connection health is polled periodically. Disconnected connections show a warning badge in the sidebar

Connections are stored in `<config_dir>/connections.json` with SSH and Direct transport types.

#### What runs on which machine

A call is routed by its own arguments, not by which panel made it. TUICommander asks
the repository registry which machine owns the path (or, for a session, which machine
owns the repo its tab was opened in) and sends the call there. A path that no registered
repository owns is local, and so is every repository with no connection.

| Operation | Runs on |
|---|---|
| Git — status, diff, log, stage, commit, push, stashes, worktrees | the machine that holds the repo |
| Filesystem — browse, read, write, search, watch | the machine that holds the repo |
| Terminals — create, write, resize, close, and the output stream | the machine that holds the repo |
| Repo change events (`repo-changed`) | the machine that holds the repo, over its own SSE stream |
| Agent run configs (`agents.json`), the agent hook toggles, upstream MCP servers | the machine that holds the repo — they describe a machine, not this app |
| Settings, keybindings, themes, pane layout, notification config | always local — they describe this app, not a repo |
| The repository registry itself (add, remove, reorder, group) | always local — it is this machine's list of machines |
| Native file pickers, window management, global hotkey, CLI install, dictation, plugin install | always local — they carry no repository path, so nothing routes them |
| mdkb code intelligence — outline, go to definition, references | **local, and that is a gap** (see below) |
| Plugin filesystem watches (`plugin_watch_path`) | **local, and that is a gap** (see below) |

Those last two have no HTTP route at all, so they cannot be sent anywhere. Called with
a path inside a remote repository they run against the local machine — which does not
have that path — and log `"<command>" has no remote route and ran on the local machine`
once per command. Check it with `GET http://localhost:9876/logs?source=network`. The
mismatch is reported rather than silent, but the feature genuinely does not work on a
remote repository yet.

**Open in app** is routed: the IDE is launched on the machine that holds the file, which
is right for a second desktop and useless for a headless `tuic-remote` — nothing there
has a display to open it on.

#### Which config follows the repo, and which stays here

Two families, and the line between them is what the config describes.

**Follows the repo — a machine owns it.** `claude`, `grok` and `codex` are installed on
the box that runs them, with their own licences, their own paths and their own config
directories, so a run config naming a Mac path cannot describe a Linux daemon. A tab
opened on a remote repository launches with **that machine's** `agents.json`: the command,
its arguments, its environment and the `CC_ENV_FLAGS` that reach the PTY. The same rule
covers the agent hook toggles (`hook_instrumentation`, `native_status_signals`) and the
upstream MCP servers, which are dialled by the backend that holds them.

The frontend keeps one copy per machine, read the first time that machine is needed and
dropped whenever its connection changes state — a daemon that went away and came back may
have been reconfigured, or be a different box behind the same name. A local repository
costs no round trip at all: its config is read once at boot.

**Stays here — this app owns it.** Theme, keybindings, pane layout, notification config
and the repository registry describe this window on this desktop. None of them is ever
sent to a daemon, including when every registered repository is remote.

**Settings edits the machine you pick.** The Agents tab and the upstream-MCP panel carry a
machine selector; it defaults to the machine of the repository the settings nav is standing
on, and an explicit pick overrides it, so a remote machine is editable from a local repo.

Three things stay local even while a remote machine is selected, because they cannot work
anywhere else: installing or removing the TUIC MCP bridge (`install_agent_mcp`), opening an
agent's config file in the editor, and the upstream OAuth flow — that one opens a browser
and listens on a loopback redirect, and a headless daemon has neither. Their controls are
hidden rather than shown pointing at the wrong machine.

A call on a repository whose machine is not connected fails with
`Remote connection <id> not connected`. It is never quietly answered by the local
backend — an answer about the wrong disk is worse than an error.

## tuic-remote (Beta)

A standalone headless daemon for running TUICommander on a Linux server without a desktop environment. It exposes the same HTTP/WebSocket API as the desktop app's remote access feature, but runs as an independent binary — no Tauri, no GUI.

### Installation

Download the `tuic-remote` binary **and** the `tuic-bridge` binary for your
platform from the [GitHub Releases](https://github.com/sstraus/tuicommander/releases)
page. Both are published for every platform below.

| Platform | Daemon | MCP bridge |
|----------|--------|------------|
| Linux x64 | `tuic-remote-x86_64-unknown-linux-gnu` | `tuic-bridge-x86_64-unknown-linux-gnu` |
| Linux ARM64 | `tuic-remote-aarch64-unknown-linux-gnu` | `tuic-bridge-aarch64-unknown-linux-gnu` |
| macOS ARM (Apple Silicon) | `tuic-remote-aarch64-apple-darwin` | `tuic-bridge-aarch64-apple-darwin` |
| Windows x64 | `tuic-remote-x86_64-pc-windows-msvc.exe` | `tuic-bridge-x86_64-pc-windows-msvc.exe` |

```bash
# Example: Linux x64
BASE=https://github.com/sstraus/tuicommander/releases/latest/download
curl -fsSL -o tuic-remote "$BASE/tuic-remote-x86_64-unknown-linux-gnu"
curl -fsSL -o tuic-bridge "$BASE/tuic-bridge-x86_64-unknown-linux-gnu"
chmod +x tuic-remote tuic-bridge
```

**Keep the two in the same directory, and keep those names.** At startup the
daemon writes an MCP entry into the config of every agent installed on its
machine, and that entry names the bridge by the path it finds next to itself. A
daemon without `tuic-bridge` beside it configures agents to run a binary that is
not there; an agent then starts with no `tuicommander` tools at all — no
`session`, no `repo`, no `progress`, no peer mail.

The same miss silently strips ego's tools: an AI Chat session granted by a
process that found no bridge gets an empty `mcpServers`, and the only symptom is
ego answering that it cannot see terminals or repositories. Since #809-724c the
process says so instead — one `warn` at startup naming every path it checked:

```
No tuic-bridge binary found, so ego sessions start with no MCP server and ego
cannot see terminals or repositories. Checked: /opt/tuic/tuic-bridge, tuic-bridge
```

Read it with `curl 'http://127.0.0.1:9876/logs?level=warn'`.

### Setup

Set a password before first use. Omit `--instance` to use the existing default
TUICommander configuration and credential vault:

```bash
./tuic-remote --set-password
```

For an isolated daemon, pass the same named instance to password setup and every
subsequent launch:

```bash
./tuic-remote --instance build-host --set-password
./tuic-remote --instance build-host
```

An instance ID is one lowercase ASCII DNS label: 1–63 characters, letters or
digits at both ends, with hyphens allowed internally. `default` is reserved;
omit the option to select the default instance. Invalid IDs fail before any
configuration or credentials are accessed.

The default instance keeps the platform paths listed in
[Configuration](../backend/config.md#config-directory) and the existing OS
keyring entry. A named instance instead stores files in the corresponding
`instances/<id>/` subdirectory and uses keyring service
`tuicommander-instance-<id>`, user `vault`. It starts empty: it never falls back
to, imports, changes, or deletes default or legacy files and credentials. In a
release build the OS keyring is mandatory; if a named instance's vault cannot be
opened, the daemon exits before binding its network socket.

Automation that depends on this isolation contract must pin and verify the
digest of the release artifact it launches. Artifact verification belongs to
the consuming harness; `tuic-remote` does not expose a separate capability or
version endpoint for it.

### Running

```bash
# Default port 9877
./tuic-remote

# Custom port
TUIC_PORT=8080 ./tuic-remote

# Named instance, with the same port override
TUIC_PORT=8080 ./tuic-remote --instance build-host
```

The daemon binds to `0.0.0.0:<port>` and serves:
- The HTTP API (sessions, terminals, git, filesystem, agents)
- WebSocket terminal streaming
- MCP tool integration (for AI agents)

It does **not** serve the web UI: `FRONTEND_DIST`, `serve_index` and
`serve_static` are all `#[cfg(feature = "desktop")]` (`mcp_http/static_files.rs`)
and `tuic-remote` is built `--no-default-features`, so `/` has no route.
Point a desktop TUICommander at the daemon; a browser has nothing to load.

Every TCP request is authenticated — the headless build has no loopback bypass
and `run_remote` forces `lan_auth_bypass` off — so an SSH tunnel does not make
the daemon local. `GET /health` is the only unauthenticated route.

The daemon also opens a local IPC endpoint that is **not** on the network: a
Unix socket at `<config dir>/mcp.sock`, or the named pipe
`\\.\pipe\tuicommander-mcp` on Windows. That is how `tuic-bridge` reaches it —
an agent running on this machine starts the bridge as a child process and the
bridge speaks HTTP over that socket. It carries no authentication because the OS
user is the boundary; nothing outside the machine can open it.

#### What the daemon runs, and what it deliberately does not

The daemon is a whole machine, not a session server. It runs almost everything
the desktop runs in the background:

| Runs on the daemon | Why it has to |
|---|---|
| Session state accumulator, ACP notice pump, tombstone sweeper | the sessions live here |
| Process snapshot refresher | agent session discovery reads argv and env off the processes on this machine; without it a tab cannot resume after a restart |
| Standby checker (Unix) | a remote client reports tab visibility, so an idle hidden session parks here exactly as it would on the desktop |
| Content index updater and boot pre-warm | see below |
| Tool search index updater | the daemon now serves MCP |
| Upstream MCP auto-connect and health checker | upstreams are per-machine configuration, so the machine that holds them connects them |
| CPU watchdog | the daemon is precisely where nobody is watching |
| Maintenance sweep (idle MCP sessions, expired auth rate limits, expired tasks) | the daemon authenticates every TCP request, so its rate-limit map grows with every port scanner that finds it |
| MCP config install for the agents on this machine | so an agent launched here has the `tuicommander` tools |

It deliberately does not run:

- **The WebView recovery watchdog** — there is no WebView, so there is no blank
  document to navigate back.
- **The embedded assistant**: knowledge persistence, the job scheduler and the
  watcher engine. Those are configured and read from the desktop UI, and the
  daemon's router serves none of their routes, so a scheduler ticking here would
  run jobs nobody on this machine can create, inspect or stop.

**Content search needs an index, and an index needs a warm.** A cross-repo
search (`/fs/search-content-all`) skips any repo whose index is not resident and
starts no build of its own — the warm strategy owns scheduling. So the daemon
watches every registered repo and pre-warms indices at boot, following the same
`index_strategy` setting as the desktop: `active_and_switch` (the default) and
`active_only` warm the active repo, `all_sequential` warms every repo with the
active one first. Repos that are not yet indexed are reported back as pending
and indexing counts, not as an empty result — a single-repo search
(`/fs/search-content`) falls back to a full scan while its index builds, so it
answers correctly either way.

### TLS

Configure TLS in the instance's `config.json` under `services.tls`:

```json
{
  "services": {
    "tls": {
      "mode": "manual",
      "cert_path": "/path/to/cert.pem",
      "key_path": "/path/to/key.pem"
    }
  }
}
```

### Differences from Desktop Remote Access

| | Desktop Remote Access | tuic-remote |
|---|---|---|
| Requires desktop app | Yes | No |
| Runs headless | No | Yes |
| Tauri dependency | Yes | No |
| Default port | 9876 | 9877 |
| LAN auth bypass | Configurable | Always disabled |
| Signal handling | N/A | Graceful SIGINT/SIGTERM |
| MCP bridge for local agents | Bundled sidecar | Downloaded next to the daemon |
| Embedded assistant (watchers, scheduler) | Yes | No |

### Status

**Beta** — the core HTTP/WebSocket API is stable, but the standalone daemon is new and may have rough edges. Report issues on GitHub.

## Troubleshooting

| Problem | Fix |
|---------|-----|
| Can't connect from another device | Check that both devices are on the same network. Try pinging the host IP. |
| Connection refused | Verify the port isn't blocked by a firewall. The settings panel includes a reachability check. |
| Authentication fails | Re-enter the password in settings — the stored bcrypt hash may be from a different password. |
| Terminals not responding | WebSocket connection may have dropped. Refresh the browser page. |
