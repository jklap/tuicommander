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

TUICommander can manage persistent SSH tunnels with automatic reconnection, port forwarding, and audit logging. A tunnel **profile** is a standalone, reusable SSH forward you set up yourself; it's a separate concept from the throwaway tunnel the [Remote Servers](#remote-servers) feature creates behind the scenes for its own "Remote Server — SSH" kind (see [Adding an SSH Remote Server](#adding-an-ssh-remote-server) below).

Tunnel profiles are no longer behind the experimental-features flag — they're a fully graduated feature.

### Where to create and edit tunnels

**Settings (`Cmd+,`) → Remote Servers.** This tab owns all tunnel-profile create/edit/delete, through the same merged connection editor used for remote-server connections (see [Remote Servers](#remote-servers) below) — pick **Kind: SSH Tunnel**. There is also a lightweight **Tunnels Panel** overlay (Command Palette → "tunnels", or the sidebar shield icon) for live status/start/stop/audit-log only; its header has an **Edit in Settings** link that jumps straight to the Remote Servers tab instead of opening its own editor.

### Creating a Tunnel Profile

1. Open **Settings → Remote Servers** and click **Add Connection**
2. Set **Kind** to **SSH Tunnel**
3. Configure:
   - **Name** — A descriptive label (e.g., "prod-db-tunnel")
   - **Host** — Remote SSH host; the field autocompletes from host aliases found in your `~/.ssh/config`
   - **Port** — SSH port (default 22)
   - **User** — SSH username
   - **Identity / Authentication** — Optional path to an SSH private key (use the **Browse…** button, which opens a file picker rooted at `~/.ssh`); leave empty to use your SSH agent instead. The detected agent and its loaded keys — including each key's fingerprint — are shown live underneath this field. See [SSH Agent Detection](#ssh-agent-detection)
   - **Port Forwards** — click **+ Add** per rule, then choose **Local** or **Remote** and fill in the bind port and paired host:port. Local forwards save `remote_host`/`remote_port`; Remote forwards save `local_host`/`local_port`. A new Local forward's remote host defaults to the tunnel's own Host field
   - **ServerAliveInterval** (default 15s), **ServerAliveCountMax** (default 3 — now has its own field, previously fixed with no UI), and **StrictHostKeyChecking** (`Yes` or `AcceptNew`) are all editable
   - **Connect automatically on startup** checkbox (persisted as `auto_connect`)
4. Optionally click **Test Connection** — runs a one-shot, no-forwards SSH connectivity check before you save, reporting reachable / auth failed / unreachable
5. Save

This same "SSH Tunnel" section — the SSH fields, Port Forwards editor, and auto-connect checkbox — is shared with the merged editor's "Remote Server — SSH" kind, so both present identical SSH capability; they only differ in what happens after (a tunnel profile vs. a remote-server connection).

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

- In the **Tunnels Panel** (or the same list embedded in Settings → Remote Servers), click **Start** next to a profile to launch the SSH tunnel
- The status badge next to it shows the current state: Starting, Connected, Reconnecting, Stopped, or Error
- **Edit** (Settings only — the standalone Tunnels Panel overlay no longer has its own per-row Edit button, only "Edit in Settings" in its header) reopens the merged editor for that profile. **Log** expands an inline audit-event timeline (last 20 events) for it. **Del** removes the profile
- Click **Stop** to gracefully terminate the SSH process (SIGTERM with 5s grace period, then SIGKILL)
- On app exit, all active tunnels are automatically stopped — no orphaned SSH processes

### SSH Agent Detection

Built and working — it's inline text inside the shared SSH fields section of the merged connection editor (both the "SSH Tunnel" and "Remote Server — SSH" kinds). TUICommander detects your SSH agent from `SSH_AUTH_SOCK` and shows the agent type and loaded keys as you fill in the form. Supported agents:

- **1Password** — Detected via the 1Password SSH agent socket
- **Secretive** — Detected via the Secretive agent socket
- **GPG Agent** — Detected via gpg-agent socket
- **Generic SSH Agent** — Any other `SSH_AUTH_SOCK` value

The key listing shows each loaded key's comment, key type (e.g. `ed25519`), and fingerprint, all fetched via `ssh-add -l`.

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

## Remote Servers

Remote connections let you manage `tuic-remote` daemons (or another TUICommander instance's HTTP API) running on other machines. TUICommander routes API calls to the correct host based on which repo/session is active.

### Where to find it

Open **Settings** (`Cmd+,`) → **Remote Servers**. This is a dedicated global Settings tab (it moved out of "Services & MCP" — that tab no longer has a "Remote Machines" section at all) that owns both SSH tunnel profiles and remote-server connections through one merged editor. Click **Add Connection** to open it — a single, consistently labeled primary action, replacing the old unlabeled "+" icon.

### The merged connection editor

Every connection — tunnel or remote server — is created and edited through the same form: **Name**, then a **Kind** dropdown with exactly four options:

- **SSH Tunnel** — a port-forwarding profile (see [SSH Tunnel Management](#ssh-tunnel-management) above)
- **Remote Server — SSH** — connects to a `tuic-remote` daemon (or another desktop instance) over an SSH-forwarded port
- **Remote Server — Direct** — connects directly to a URL (e.g. `http://10.0.0.5:9877`)
- **Remote Server — Local** — connects to another named/isolated TUICommander instance running on the **same machine** (`tuic-remote --instance <id>` or `TUIC_APP_INSTANCE=<id>`), by instance ID (the port is resolved automatically by reading that instance's own config off disk) or by a manually-entered port (for an unnamed instance, e.g. a second `make dev` debug build)

Kind is fixed once you're editing an existing item (switching an existing tunnel to a remote-server kind, or vice versa, doesn't map onto one Save since they're different backing stores) — only "Add Connection" lets you pick Kind freely.

Editing an existing connection now always shows its **Name** field, so renaming no longer requires delete-and-recreate.

### Adding an SSH Remote Server

1. Open Settings → Remote Servers → **Add Connection**
2. Set **Kind** to **Remote Server — SSH**
3. Configure **Host**, **Port** (default 22), **User**, and an optional **Identity file** path — the same shared SSH fields as an SSH Tunnel, including agent detection and `~/.ssh/config` autocomplete
4. Set the **Remote daemon port** — defaults to **9877**, matching what a freshly-started `tuic-remote` binary actually listens on by default
5. Optionally set an **Instance ID** — passed as `--instance <id>` only when *this connection* launches or configures the remote daemon itself (see [Remote Daemon Provisioning](#remote-daemon-provisioning-ssh) below). It is never used to discover an existing port — that auto-discovery only exists for the Local kind, where the port is read directly off disk on the same machine
6. Optionally check **Start remote daemon if not running** — if the tunnel fails to connect, TUICommander offers to install and start `tuic-remote` on the remote host itself, always with an explicit confirmation first
7. When that's checked, optionally also check **Leave daemon running on disconnect** — otherwise Disconnect stops the daemon again, but only if this session actually started it
8. Optionally enter an **Auth username** and **Auth password** — both are now optional (previously username was required but never actually used to authenticate anything). A password you enter is sent to the OS keyring, never written to `connections.json`
9. Optionally click **Test Connection** to verify reachability (and, if you supplied credentials, that they're accepted) before saving
10. Save

Saving only writes the connection record — **no SSH tunnel is created at Save time.** The tunnel is created lazily the first time you click **Connect** on that connection: TUICommander creates a throwaway tunnel profile (named `__remote_<connection-id>`, not shown under a friendly name in the Tunnels Panel) with one Local forward from a random local port to `127.0.0.1:<remote_daemon_port>` on the far side, starts it, and waits up to 30s for it to connect. **Disconnect** stops that tunnel and deletes the throwaway profile again — nothing persists between connect/disconnect cycles.

### Remote Daemon Provisioning (SSH)

When **Start remote daemon if not running** is checked and a Connect attempt's tunnel fails (the remote end refuses the forwarded port, rather than an auth/host-key problem on an otherwise-live daemon), TUICommander walks through getting the daemon running itself — **every step that changes something on the remote host asks for confirmation first; nothing happens silently**:

1. **Probe.** Checks over the same SSH connection whether `tuic-remote` is running, installed but not running, or missing entirely.
2. **Install, if missing.** Confirms with you, then detects the remote's OS/architecture (`uname`), downloads the matching `tuic-remote` release artifact, and streams it to the remote host over the same SSH connection (no separate `scp`/network access needed on your end beyond the one download).
3. **Start.** Confirms with you, then launches `tuic-remote` on the remote host (using your configured **Remote daemon port** and, if set, **Instance ID**), and retries the tunnel connection once it comes up.
4. **Configure a password, if unconfigured.** If the daemon comes up but has never had a password set (distinguished from a *wrong* password — a genuinely unconfigured daemon never asks for the wrong-password case), and you've entered an Auth username/password on this connection, TUICommander confirms with you, then sets that password on the remote daemon over the same SSH connection.

If you decline any confirmation, TUICommander stops there and reports the original connection error — it never falls back to a different action on your behalf.

On Disconnect, if this session is the one that started the daemon (and **Leave daemon running on disconnect** isn't checked), TUICommander stops it the same way local tunnel cleanup does: verifying the remote process before signaling it, never a blind kill.

### Version Checking

After a successful Connect (SSH, Direct, or Local), TUICommander compares the remote's reported version against its own. A mismatch shows an inline warning next to that connection in the Remote Servers list. For an SSH connection, an **Update** button appears alongside the warning — it repeats the same download/replace step used for provisioning a missing binary, then restarts the remote daemon. Direct and Local connections don't get an Update button (there's no channel to act on the remote host without a real SSH connection to it); the warning is informational only for those.

### Adding a Direct Remote Server

1. Open Settings → Remote Servers → **Add Connection**
2. Set **Kind** to **Remote Server — Direct**
3. Enter the URL of the remote daemon (e.g., `http://10.0.0.5:9877` or `https://10.0.0.5:9877`)
4. Optionally enter an **Auth username**/**Auth password** (see below)
5. Optionally click **Test Connection**
6. Save, then click **Connect** — health polling and the live-event bridge only start once you connect, not on save

If the URL is `https://` and the certificate isn't signed by a CA your system already trusts (a self-signed cert on the remote daemon — the common case for a `tuic-remote` instance with no manually-configured TLS), TUICommander shows the same kind of one-time fingerprint-verification prompt used for its own self-signed HTTPS (see [HTTPS](#https) above): compare the shown fingerprint against the one on the remote machine, then accept to pin it. Once pinned, TUICommander refuses to connect automatically if the certificate ever changes — it never silently re-trusts a new one. Behind the scenes, this routes the connection through a small local loopback proxy that terminates the pinned TLS connection and also attaches your configured Basic Auth credentials, so plain `http://` and already-CA-trusted `https://` Direct connections are unaffected and keep talking to the remote URL directly.

### Adding a Local Remote Server

1. Open Settings → Remote Servers → **Add Connection**
2. Set **Kind** to **Remote Server — Local**
3. Choose **Named instance** (enter the instance ID — the real port is read off that instance's own on-disk config at connect time, so it can never go stale) or **Manual port** (for an unnamed instance, e.g. a second `make dev` debug build) — only one of the two is used, never both
4. Optionally enter auth credentials and **Test Connection**
5. Save

### Remote Repositories and Terminals

Once a connection shows **Connected** in Remote Servers:

- **Add remote repo** — right-click "Add repo" in the sidebar; connected remote servers appear in that context menu. The repo is added with the connection's ID attached and shows a static **"remote"** badge in the sidebar
- **Open terminal** — terminals on a remote repo route their calls through that connection's resolved base URL (the local tunnel port for SSH, the configured URL for Direct, or `127.0.0.1:<resolved-port>` for Local). I/O works the same as local terminals
- **Health monitoring** — polled every 5 seconds against `<baseUrl>/health` while a connection is active. **Caveat:** the sidebar's "remote" badge is static — it's shown whenever a repo has a `connectionId` at all, and does not turn into a warning or change appearance when that connection drops. Only the status dot in the Remote Servers list (grey/yellow/green/red) reflects live connected/connecting/error/disconnected state today

Connections are stored in `<config_dir>/connections.json` with SSH, Direct, and Local transport types. Passwords are never stored there — only in the OS keyring, keyed by connection ID.

### Authentication

**Auth username** and **Auth password** are both optional on every remote-server kind, and — unlike the previous "Remote Machines" panel, where the username field was required but never actually used — a supplied password is now genuinely sent as HTTP Basic Auth to the remote daemon (for SSH and Local, over the already-encrypted forwarded/loopback connection; for Direct, directly to the URL). Use **Test Connection** to verify a password is accepted before relying on it. Leaving both fields blank is fine for a daemon with no auth configured.

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
