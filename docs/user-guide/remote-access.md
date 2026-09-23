# Remote Access

Access TUICommander from a browser on another device on your network.

## Setup

1. Open **Settings** (`Cmd+,`) → **Remote Access**
2. Configure:
   - **Port** — Default `9876` (range 1024–65535)
   - **Username** — Basic Auth username
   - **Password** — Basic Auth password (stored as a bcrypt hash, never in plaintext)
3. Enable remote access

Once enabled, the settings panel shows the access URL: `https://<your-ip>:<port>` (see [HTTPS](#https) below for where that certificate comes from).

## Connecting from Another Device

1. Open a browser on any device on the same network
2. Navigate to the URL shown in settings (e.g., `https://192.168.1.42:9876`)
3. If your browser shows a certificate warning, see [HTTPS](#https) below before proceeding
4. Enter the username and password you configured
5. TUICommander loads in the browser with full terminal access

### QR Code

The settings panel shows a QR code for the access URL — scan it from a phone or tablet to connect quickly. The QR code uses your actual local IP address.

### Connecting by name instead of IP (macOS)

On macOS, the network picker (in the QR dialog and Settings → Remote Access) also lists an **mDNS** entry — your Mac's existing Bonjour hostname (e.g. `MyMac.local`), the same name AirDrop and file sharing already use. Select it to get a URL like `https://MyMac.local:9876` instead of a raw IP; it keeps working even if your IP changes after a network switch. This isn't available on Windows/Linux, since those platforms don't guarantee an mDNS responder is running for the hostname out of the box.

## HTTPS

Browsers only expose some features — notably the Clipboard API used for copy/paste — to a "secure context": `https://` or `http://localhost`. A plain `http://<lan-ip>` URL doesn't qualify, so TUICommander serves HTTPS for remote/LAN access using one of two sources, in this priority order:

1. **Tailscale HTTPS** — if the Tailscale daemon is running and HTTPS Certificates are enabled in your tailnet's admin console, TUICommander provisions a real Let's Encrypt-backed certificate for your tailnet hostname. No browser warning, ever.
2. **Self-signed certificate (fallback)** — if Tailscale HTTPS isn't available, TUICommander generates and serves a self-signed certificate covering `localhost` and all of the machine's current LAN IPs. This is the standard approach for LAN-only HTTPS (the same thing Plex, Home Assistant, and Portainer do), but it means your browser doesn't recognize the certificate authority and shows a security warning — **once per device**, the first time that device connects. Click through it (usually "Advanced" → "Proceed") to continue; the browser remembers your choice for that device.

Since remote access defaults to serving HTTPS, plain `http://` requests to the same port are automatically redirected (301) to `https://` when the self-signed fallback is active, so old bookmarks and QR codes keep working.

### Verifying the self-signed certificate

A LAN threat model is low-risk, but it's still worth a quick check the first time: **Settings → Remote Access → Self-Signed HTTPS** shows the certificate's SHA-256 fingerprint. Before clicking through your browser's warning, you can compare that fingerprint against the one your browser shows in its "view certificate" details (usually reachable from the padlock/warning icon in the address bar) to confirm you're accepting the certificate TUICommander actually generated on your machine — not one substituted by something else on the network.

If you ever suspect the cert was compromised, or you just want a fresh one, use the **Regenerate** button in that same settings section.

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

## Security

- **Authentication** — Basic Auth with bcrypt-hashed passwords
- **Secret storage** — Session tokens, relay bearer tokens, and push VAPID
  private keys are stored in the OS keyring-backed credential vault; config
  files and `/config` responses expose only non-secret settings and existence
  flags
- **Local network only** — The server binds to your machine's IP; it's not exposed to the internet unless you configure port forwarding (don't do this without a VPN)
- **CORS** — When remote access is enabled, any origin is allowed (necessary for browser access from different IPs)

### Reading a file outside your registered repositories

The desktop app can open any file you can see, but a browser/remote/PWA client
is a narrower-trust caller: opening an absolute file path (a Markdown/plan-file
link, or a file opened directly in the code editor) is gated to your
**registered repositories**, plus any directory you've explicitly added under
**Settings → Services & MCP → File Access → Additional Readable Directories**.
`~/.claude/plans` is included by default, so a Claude Code plan-file link an
agent printed opens with no extra setup. If you click a link elsewhere and see
a friendly "outside your registered repositories and allowed directories"
message instead of the file, add that file's folder to the same setting. This
only affects **reading** files over HTTP — it never widens what a remote
client can write, copy, or move; those stay confined to registered repository
roots regardless of this setting.

## MCP HTTP Server

Separate from remote access, TUICommander runs an **HTTP API server** for AI tool integration:

- The server always listens on an IPC listener: Unix domain socket at `<config_dir>/mcp.sock` on macOS/Linux, or named pipe `\\.\pipe\tuicommander-mcp` on Windows
- AI agents connect via the `tuic-bridge` sidecar binary, which translates MCP stdio transport to the IPC listener
- Bridge configs are auto-installed on first launch for supported agents (Claude Code, Cursor, Windsurf, VS Code, Zed, Amp, Gemini, Codex, Grok, opencode, Droid, goose, pi) — and only for the ones present on the machine, so TUICommander never creates a config directory for a tool you do not have. On every subsequent launch, the bridge path is verified and updated if stale (from reinstalls, updates, or moves)
- The `mcp_server_enabled` toggle in **Settings** → **Remote Access** controls whether MCP protocol tools are exposed, not the server itself
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

TUICommander can manage persistent SSH tunnels with automatic reconnection, port forwarding, and audit logging. A tunnel **profile** is a standalone, reusable SSH forward you set up yourself; it's a separate concept from the throwaway tunnel the [Remote Connection Manager](#remote-connection-manager) creates behind the scenes for its own SSH transport (see [Adding an SSH Connection](#adding-an-ssh-connection) below).

### Opening the Tunnels Panel

There is currently no "SSH Tunnels" entry under Settings. Open the panel one of two ways:

- **Command Palette** (`Cmd+P` / `Ctrl+P`) → type "tunnels" → **SSH Tunnels**
- The **sidebar shield icon** (see [Sidebar Shield Indicator](#sidebar-shield-indicator) below) — but it only appears once at least one tunnel profile already exists, so the Command Palette is the only way in the first time

### Creating a Tunnel Profile

1. Open the Tunnels Panel (see above)
2. Click **+ New Tunnel** to open the editor
3. Configure:
   - **Name** — A descriptive label (e.g., "prod-db-tunnel")
   - **Host** — Remote SSH host; the field autocompletes from host aliases found in your `~/.ssh/config`
   - **Port** — SSH port (default 22)
   - **User** — SSH username
   - **Identity / Authentication** — Optional path to an SSH private key (use the **Browse…** button, which opens a file picker rooted at `~/.ssh`); leave empty to use your SSH agent instead. The detected agent and its loaded keys are shown live underneath this field — see [SSH Agent Detection](#ssh-agent-detection)
   - **Port Forwards** — click **+ Add** per rule, then choose **Local** or **Remote** and fill in the bind port and paired host:port. Local forwards save `remote_host`/`remote_port`; Remote forwards save `local_host`/`local_port`. A new Local forward's remote host defaults to the tunnel's own Host field
   - **Options** — only **ServerAliveInterval** (default 15s) and **StrictHostKeyChecking** (`Yes` or `AcceptNew`) are editable here. `ServerAliveCountMax` exists in the saved profile with a fixed default of 3, but the editor has no field for it
   - **Connect automatically on startup** checkbox (persisted as `auto_connect`)
4. Save the profile

Tunnel profiles are stored as TOML files. **Global profiles** live in `<config_dir>/tunnels/` and are available across all repos. **Per-repo profiles** are stored in `<repo>/.tuic/tunnels/` and override global profiles with the same ID.

### Auto-Connect

Enable **Connect automatically on startup** on a tunnel profile to have it start automatically when TUICommander launches. Useful for tunnels you always need (database access, internal services).

Profiles marked with auto-connect are started during app hydration before you interact with the UI.

### Sidebar Shield Indicator

This lives in the **Sidebar footer** (next to the Help button) — it is not part of the app's separate top-level Status Bar. It's hidden entirely until you have at least one tunnel profile:

- **Muted icon, no badge** — you have tunnel profiles configured but none are currently connected
- **Accent-colored icon with a green count badge** — shows the number of currently connected tunnels

Click it to open the Tunnels Panel.

### Command Palette

Open the command palette (`Cmd+P` / `Ctrl+P`) and type "tunnels" to toggle the Tunnels Panel without navigating to Settings.

### Starting and Stopping Tunnels

- In the **Tunnels Panel**, click **Start** next to a profile to launch the SSH tunnel
- The status badge next to it shows the current state: Starting, Connected, Reconnecting, Stopped, or Error
- **Edit** reopens the same editor for that profile. **Log** expands an inline audit-event timeline (last 20 events) for it. **Del** removes the profile
- Click **Stop** to gracefully terminate the SSH process (SIGTERM with 5s grace period, then SIGKILL)
- On app exit, all active tunnels are automatically stopped — no orphaned SSH processes

### SSH Agent Detection

Built and working — but it's not a separate panel, it's inline text inside the tunnel editor's Identity/Authentication field. TUICommander detects your SSH agent from `SSH_AUTH_SOCK` and shows the agent type and loaded keys as you fill in the form. Supported agents:

- **1Password** — Detected via the 1Password SSH agent socket
- **Secretive** — Detected via the Secretive agent socket
- **GPG Agent** — Detected via gpg-agent socket
- **Generic SSH Agent** — Any other `SSH_AUTH_SOCK` value

The key listing shows each loaded key's comment and key type (e.g. `ed25519`), fetched via `ssh-add -l`. The backend also returns each key's fingerprint, but the editor does not currently display it.

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

Remote connections let you manage `tuic-remote` daemons (or another TUICommander instance's HTTP API) running on other machines. TUICommander routes API calls to the correct host based on which repo/session is active.

### Where to find it

Open **Settings** (`Cmd+,`) → **Services & MCP** → scroll to the bottom → **Remote Machines**. There is no separate "Connections" tab or section anywhere in the app — this is the only place. Click the **+** icon in the section header (tooltip: "Add remote machine") to reveal the add form; there's no button literally labeled "Add Connection".

### Adding an SSH Connection

1. Open Settings → Services & MCP → **Remote Machines** → **+**
2. Enter a **Name**
3. Leave the transport dropdown on **SSH** (the default)
4. Configure **Host**, **Port** (default 22), **User**, and an optional **Identity file** path
5. Set the **Remote daemon port** — the form defaults this to **9876**. (Note: the Rust-side `RemoteConnection::new_ssh` helper used in tests defaults the same field to **9877** — the frontend and backend disagree on the default; worth reconciling as a small bug independent of this doc.)
6. Enter an **Auth username** (required — the form won't save without it)
7. Save

Saving only writes the connection record — **no SSH tunnel is created at Save time.** The tunnel is created lazily the first time you click **Connect** on that connection: TUICommander creates a throwaway tunnel profile (named `__remote_<connection-id>`, not shown under a friendly name in the Tunnels Panel) with one Local forward from a random local port to `127.0.0.1:<remote_daemon_port>` on the far side, starts it, and waits up to 30s for it to connect. **Disconnect** stops that tunnel and deletes the throwaway profile again — nothing persists between connect/disconnect cycles.

### Adding a Direct Connection

1. Open Settings → Services & MCP → **Remote Machines** → **+**
2. Select **Direct** transport
3. Enter the URL of the remote daemon (e.g., `http://10.0.0.5:9876`)
4. Enter an **Auth username** (see the authentication caveat below)
5. Save, then click **Connect** — health polling and the live-event bridge only start once you connect, not on save

### Remote Repositories and Terminals

Once a connection shows **Connected** in Remote Machines:

- **Add remote repo** — right-click "Add repo" in the sidebar; connected remote machines appear in that context menu. The repo is added with the connection's ID attached and shows a static **"remote"** badge in the sidebar
- **Open terminal** — terminals on a remote repo route their calls through that connection's resolved base URL (the local tunnel port for SSH, or the configured URL for Direct). I/O works the same as local terminals
- **Health monitoring** — polled every 5 seconds against `<baseUrl>/health` while a connection is active. **Caveat:** the sidebar's "remote" badge is static — it's shown whenever a repo has a `connectionId` at all, and does not turn into a warning or change appearance when that connection drops. Only the status dot in the Remote Machines settings panel (grey/yellow/green/red) reflects live connected/connecting/error/disconnected state today

Connections are stored in `<config_dir>/connections.json` with SSH and Direct transport types.

### Authentication caveat

The **Auth username** field is required by validation on both transport types, but as of this writing it is **not used anywhere** to actually authenticate the connection — nothing in the frontend or backend builds an `Authorization` header or otherwise sends credentials to the remote daemon using it. If the remote daemon has Basic Auth enabled (the same auth described earlier in this doc's [Security](#security) section for the desktop app's own remote-access server), a Remote Connection as currently implemented has no way to supply the password. Treat this as either a real gap to close (wire the field to an actual credential and send it) or flag the field as not-yet-functional until it is.

## tuic-remote (Beta)

A standalone headless daemon for running TUICommander on a Linux server without a desktop environment. It exposes the same HTTP/WebSocket API as the desktop app's remote access feature, but runs as an independent binary — no Tauri, no GUI.

### Installation

Download the `tuic-remote` binary for your platform from the [GitHub Releases](https://github.com/sstraus/tuicommander/releases) page.

| Platform | Artifact |
|----------|----------|
| Linux x64 | `tuic-remote-x86_64-unknown-linux-gnu` |
| Linux ARM64 | `tuic-remote-aarch64-unknown-linux-gnu` |
| macOS ARM (Apple Silicon) | `tuic-remote-aarch64-apple-darwin` |
| Windows x64 | `tuic-remote-x86_64-pc-windows-msvc.exe` |

```bash
# Example: Linux x64
curl -fsSL -o tuic-remote https://github.com/sstraus/tuicommander/releases/latest/download/tuic-remote-x86_64-unknown-linux-gnu
chmod +x tuic-remote
```

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
- The TUICommander web UI (PWA-capable)
- WebSocket terminal streaming
- MCP tool integration (for AI agents)

### TLS

`tuic-remote` doesn't generate a self-signed cert on its own — that fallback is desktop-only. Configure a cert manually in the instance's `config.json` under `services.tls`:

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

Both HTTP and HTTPS are served on the same port (no forced redirect) once a manual cert is configured.

### Differences from Desktop Remote Access

| | Desktop Remote Access | tuic-remote |
|---|---|---|
| Requires desktop app | Yes | No |
| Runs headless | No | Yes |
| Tauri dependency | Yes | No |
| Default port | 9876 | 9877 |
| LAN auth bypass | Configurable | Always disabled |
| Signal handling | N/A | Graceful SIGINT/SIGTERM |

### Status

**Beta** — the core HTTP/WebSocket API is stable, but the standalone daemon is new and may have rough edges. Report issues on GitHub.

## Troubleshooting

| Problem | Fix |
|---------|-----|
| Can't connect from another device | Check that both devices are on the same network. Try pinging the host IP. |
| Connection refused | Verify the port isn't blocked by a firewall. The settings panel includes a reachability check. |
| Authentication fails | Re-enter the password in settings — the stored bcrypt hash may be from a different password. |
| Terminals not responding | WebSocket connection may have dropped. Refresh the browser page. |
