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

- **Authentication** — QR URL token, session cookie or Basic Auth with bcrypt-hashed passwords. HTTP requires credentials even from this computer or the LAN; the old LAN authentication bypass no longer applies. Local CLI/MCP IPC remains available without HTTP credentials.
- **Secret storage** — Session tokens, relay bearer tokens, and push VAPID
  private keys are stored in the OS keyring-backed credential vault; config
  files and `/config` responses expose only non-secret settings and existence
  flags
- **Local network only** — The server binds to your machine's IP; it's not exposed to the internet unless you configure port forwarding (don't do this without a VPN)
- **Browser request protection** — The HTTP listener rejects foreign Origin headers and unknown Host names before authentication. Use its literal IP or detected Tailscale FQDN; browser/PWA requests from that same server and bundled/development WebViews remain supported. CORS does not allow arbitrary websites.

### Reading a file outside your registered repositories

The desktop app can open any file you can see, but a browser/remote/PWA client
is a narrower-trust caller: opening an absolute file path (a Markdown/plan-file
link, or a file opened directly in the code editor) is gated to your
**registered repositories**, plus any directory you've explicitly added under
**Settings → Remote Access → File Access → Additional Readable Directories**.
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
- The `mcp_server_enabled` config key controls whether MCP protocol tools are exposed, not the server itself
- **Settings** → **MCP** → **HTTP API Server** shows server status and active session count
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

In a mobile terminal, tap the paperclip to choose a photo or file. The upload
inserts its `@path` into the draft; review the draft and tap Send when ready.
In mobile AI Chat, images join the image draft and other files appear as file
chips until Send. Android Chrome can also share a file into the installed PWA;
the shared file opens as an AI Chat draft. iOS Safari currently requires the
paperclip because it does not support Web Share Target.
See [Chrome's Share Target guide](https://developer.chrome.com/docs/capabilities/web-apis/web-share-target)
and [WebKit's open support issue](https://bugs.webkit.org/show_bug.cgi?id=194593).

### Receive and answer an agent's question away from the desk

1. On the Mac, enable **Remote Access** and **Tailscale HTTPS** in TUICommander
   Settings. Connect both the Mac and phone to the same tailnet. Use the HTTPS
   Tailscale URL shown in Settings; a plain LAN `http://` URL cannot register a
   phone service worker for push. Do not expose the port to the public internet.
2. Open `<Tailscale HTTPS URL>/mobile` on the phone and sign in. On iPhone, add
   it to the Home Screen, then launch that installed PWA. In mobile **Settings**,
   turn **Push notifications** on and grant notification permission. If it says
   **Enabled** for an old subscription, turn it off and back on to re-subscribe.
3. On the Mac, run an authenticated `POST /api/push/test` against that HTTPS
   server. `sent` counts accepted push-service requests, not notification
   display. A 404 means no subscription; `stale_removed` after HTTP 410 means
   the phone must re-subscribe. A 503 means push is disabled or the VAPID key is
   unavailable. Confirm the notification actually appears on the phone.
4. Have a managed agent report `progress type=blocked` with its question.
   After the desktop is unfocused, or the Mac has had no HID input for two
   minutes, the phone notification contains the question and opens that
   session. Type one answer and tap Send. The PWA waits for the same submission
   receipt as `session action=submit`; if the session closed or is busy, it
   keeps the draft for review instead of blindly retrying.

The question is encrypted to the phone's Web Push subscription. It does not
pass through TUICommander's content-blind cloud relay. TUICommander sends at
most one question or completion push per session every 30 seconds.
Notifications from different sessions remain separate. A newer notification
for one session can replace its earlier one; tapping either remaining session
notification opens that session.

### Mobile Features

- **Sessions list** — See all running agents with status (idle, busy, question, rate-limited, error)
- **Session detail** — Live output streaming, quick-reply chips (Yes/No/Enter/Ctrl-C), text input
- **Files** — Browse a configured repository, view `.md` files as rendered Markdown or other text as plain text, and tap Edit to change and save source files up to 1 MB
- **Question banner** — Instant notification when any agent needs input, with quick-reply buttons
- **Activity feed** — Chronological event feed grouped by time
- **Progress** — Select a project with journal entries to read its recent reports
- **Remote sessions** — Open live output and close a session on its connected owning machine from the same phone page
- **Notification sounds** — Audio alerts for questions, errors, completions, and rate limits

### Tips

- Pull down on the sessions list to refresh
- The question banner appears on all screens — you don't need to be on the sessions tab to respond
- Sound notifications can be toggled in the mobile Settings tab

## SSH Tunnel Management

TUICommander can manage persistent SSH tunnels with automatic reconnection, port forwarding, and audit logging. A tunnel **profile** is a standalone, reusable SSH forward you set up yourself; it is a separate thing from the tunnel a [remote connection](#remote-connection-manager) opens for itself on Connect. Tunnels are no longer behind the Experimental Features flag.

### Where to create and edit tunnels

**Settings (`Cmd+,`) → Remote Servers.** That page owns tunnel create, edit and delete through the same merged editor remote connections use — pick **Kind: SSH Tunnel**. Its **SSH Port-Forwarding Tunnels** section lists every profile with Start/Stop, Edit, Log and Del. The **Tunnels Panel** overlay (Command Palette → "tunnels", or the sidebar shield icon) is for live status, start/stop and the audit log only; its **Edit in Settings** link closes it and opens the Remote Servers page.

### Creating a Tunnel Profile

1. Open **Settings → Remote Servers** and click **Add Connection**
2. Leave **Kind** on **SSH Tunnel** (the default)
3. Configure:
   - **Name** — A descriptive label (e.g., "prod-db-tunnel")
   - **Host** — Remote SSH host; autocompletes from the host aliases in your `~/.ssh/config`
   - **Port** — SSH port (default 22)
   - **User** — SSH username
   - **Identity / Authentication** — Optional path to an SSH private key; leave empty to use your SSH agent. On the desktop app **Browse…** opens a file picker at `~/.ssh`. The detected agent and its loaded keys, with each key's fingerprint, show underneath (see [SSH Agent Detection](#ssh-agent-detection))
   - **Port Forwards** — click **+ Add** per rule, choose **Local** or **Remote**, and fill in the bind port and the paired host:port. Local forwards target `remote_host`/`remote_port`; Remote forwards target `local_host`/`local_port`. A new Local forward's remote host defaults to the tunnel's Host
   - **ServerAliveInterval** (default 15s), **ServerAliveCountMax** (default 3), **StrictHostKeyChecking** (`Yes` or `AcceptNew`) and **Compress the channel (ssh -C)** (default on)
   - **Connect automatically on startup** (persisted as `auto_connect`)
4. Optionally click **Test Connection** — a one-shot, no-forwards SSH check that reports reachable, authentication failed or unreachable before you save
5. Save

**Compression** is `ssh -C` on the tunnel channel, and it is on by default because
a tunnel usually carries a terminal stream, which deflates to a few percent of
itself. Turn it off for a tunnel to a machine on the same LAN, where the link is
fast and the CPU is the scarcer thing. Profiles written before this option existed
keep compressing.

Tunnel profiles are stored as TOML files. **Global profiles** live in `<config_dir>/tunnels/` and are available across all repos. **Per-repo profiles** are stored in `<repo>/.tuic/tunnels/` and override global profiles with the same ID.

A tunnel shows **Connected** only after SSH survives its initial 500 ms. With local forwards, every local port must also accept a TCP connection. If SSH exits first or a port does not start listening within 30 seconds, the tunnel reports the failure instead.

### Auto-Connect

Enable **Connect automatically on startup** on a tunnel profile to have it start automatically when TUICommander launches. Useful for tunnels you always need (database access, internal services). Profiles marked with auto-connect are started during app hydration before you interact with the UI.

### Sidebar Shield Indicator

The sidebar footer (next to the Help button) shows a shield icon once at least one tunnel profile exists:

- **Muted icon, no badge** — you have tunnel profiles configured but none are currently connected
- **Accent-colored icon with a count badge** — the number of connected tunnels

Click it to open the Tunnels Panel.

### Command Palette

Open the command palette (`Cmd+P` / `Ctrl+P`) and type "tunnels" to toggle the Tunnels Panel.

### Starting and Stopping Tunnels

- In the **Tunnels Panel** (or the SSH Port-Forwarding Tunnels list on Settings → Remote Servers), click **Start** next to a profile to launch the SSH tunnel
- The status badge next to it shows the current state: Starting, Connected, Reconnecting, Stopped, or Error
- **Edit** (Settings only) reopens the merged editor on that profile; **Log** expands the last 20 audit events; **Del** removes the profile
- Click **Stop** to gracefully terminate the SSH process (SIGTERM with 5s grace period, then SIGKILL)
- On app exit, all active tunnels are automatically stopped — no orphaned SSH processes

### SSH Agent Detection

TUICommander detects your SSH agent from `SSH_AUTH_SOCK` and shows the agent type and loaded keys inside the shared SSH fields of the connection editor (both the SSH Tunnel and Remote Server — SSH kinds). Supported agents:

- **1Password** — Detected via the 1Password SSH agent socket
- **Secretive** — Detected via the Secretive agent socket
- **GPG Agent** — Detected via gpg-agent socket
- **Generic SSH Agent** — Any other `SSH_AUTH_SOCK` value

The key listing shows each loaded key's comment, key type and fingerprint (from `ssh-add -l`), so you can check the right identity is available before connecting.

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

They live on **Settings → Remote Servers** (the separate Remote Machines page merged into it; an old `remote-machines` deep link opens it). **Add Connection** opens the merged editor: a **Name** and a **Kind** — **SSH Tunnel** (a tunnel profile, see above), **Remote Server — SSH**, **Remote Server — Direct** or **Remote Server — Local**. Editing an existing item fixes its Kind; its Name stays editable. The page's **Remote Machines** section lists the connections, the SSH hosts discovered from your SSH config and `known_hosts` (click one to open the editor prefilled with it, or **Probe** it), and each row's Connect, Update, Install, Edit and Remove actions.

### Adding an SSH Connection

1. Open **Settings** → **Remote Servers** and click **Add Connection** (or click a
   discovered host under **Remote Machines** — **Probe config hosts** shows which
   accept a shell login)
2. Set **Kind** to **Remote Server — SSH**
3. Configure the host, port (default 22), user and optional identity file — the
   same SSH fields as a tunnel. **StrictHostKeyChecking** is fixed to
   `AcceptNew` here: the tunnel TUICommander opens on your behalf always accepts a
   new host's key on first contact and refuses a changed one
4. Set the remote daemon port (default 9877)
5. Choose **Never deploy**, **Deploy on connect**, or **Installed service**, and
   set how many minutes an ephemeral daemon should survive with no client
6. Optionally set an **Instance ID** — passed as `--instance <id>` whenever
   TUICommander starts the remote daemon or sets its password (see
   [Start a daemon or set its password, with confirmation](#start-a-daemon-or-set-its-password-with-confirmation)).
   It is never used to discover a port; it must be a lowercase DNS label
7. With **Never deploy**, optionally check **Offer to start the remote daemon if
   it is not running**, and then **Leave it running on disconnect** if a daemon
   TUICommander starts should outlive the connection
8. Set the auth username and password the daemon was configured with
9. Optionally click **Test Connection**, then Save, then click **Connect**

For an installed service, Connect waits for the SSH forwarding port and retries the daemon health check during startup before reporting it unavailable.

### Adding a Direct Connection

1. Open **Settings** → **Remote Servers** and click **Add Connection**
2. Set **Kind** to **Remote Server — Direct**
3. Enter the URL of the remote daemon (e.g., `http://10.0.0.5:9877`)
4. Set the auth username and password
5. Optionally click **Test Connection**, then Save, then click **Connect**

**Self-signed HTTPS.** An `https://` daemon whose certificate no authority your
system trusts (typically its own self-signed one, see **Self-Signed HTTPS**
above) is checked when you press **Connect**: a **Verify certificate** dialog
shows its SHA-256 fingerprint. Compare it with the fingerprint the remote
machine shows under **Settings → Remote Access → Self-Signed HTTPS**, then
**Accept and connect** to pin it (trust on first use). From then on TUICommander
talks to that daemon only through a local relay that accepts exactly the pinned
certificate; if the certificate ever changes, Connect fails with
"Certificate changed" instead of trusting the new one. After a legitimate
certificate change, open the connection's editor and click **Forget pinned
certificate** (changing the URL forgets it too); the next Connect asks again.
Automatic reconnects never pin anything: an unpinned self-signed daemon stays in
error until you confirm it. Test Connection judges the certificate the same way
before it sends anything: a pinned certificate that still matches is tested
through the same kind of relay; an unpinned self-signed certificate reports "not
trusted" with its fingerprint, and a changed one reports "certificate changed"
— in both cases nothing (no password either) is sent to the server.

A URL that points back at the TUICommander you are configuring is refused with
"this very TUICommander instance — a machine cannot mirror itself". The check
compares the `instance_id` in `GET /health` against this process's own, so a
second daemon on the same machine (a different port) is still a valid peer.

### Adding a Local Connection

**Remote Server — Local** points at another TUICommander instance on the same
machine (`tuic-remote --instance <id>` or `TUIC_APP_INSTANCE=<id>`): either a
**Named instance** (its port is read from that instance's own config when used,
never cached) or a **Manual port** for an unnamed one. **Connect** reaches it
at `http://127.0.0.1:<port>` and authenticates exactly like any other
connection: set the auth username and password the other instance expects —
being on the same machine is not treated as a credential, and a rejected or
missing password leaves it **unauthenticated**. A named instance that cannot be
found (never started, typo) fails Connect without contacting anything. Pointing
it at the instance you are configuring is refused like a Direct self-connection.
Before any password is sent (Connect and Test Connection), the process on that
port must prove it is a TUICommander instance of your user — and, for a Named
instance, that instance: its `/health` must name an IPC socket in the place that
instance keeps one, owned by you, and that socket must report the same instance
id. A port taken over by another program (a stale port after a restart, or a
squatter) fails with "could not be verified … nothing was sent to it". (Not
checked on Windows; a local relay that forwards the real instance's `/health`
verbatim is not detected.)
**Update & restart** is not offered for a Local connection's binary: update
that install directly. When the other instance runs a different version than this
app, the row shows **Remote out of date** with a "Version mismatch" notice naming
both versions.

### Connect or Install

**Connect** with **Deploy on connect** is the zero-setup path. If no compatible
daemon answers, TUICommander downloads the release asset matching the host,
caches it locally, copies it over the same SSH identity as the tunnel and starts
it on `127.0.0.1`. The daemon keeps existing sessions alive for the configured
survive time after the last client leaves, then exits. A later Connect reuses a
matching binary already on the host and the pairing token in the local vault.

**Install** uses the same binary but registers a persistent user service. Linux
gets `~/.config/systemd/user/tuic-remote.service` plus the mode-0600
`~/.config/tuic/remote.env`; macOS gets the mode-0600
`~/Library/LaunchAgents/dev.tuicommander.remote.plist`. The service starts at
login and restarts on failure, so later Connect operations only open the SSH
tunnel. **Uninstall** stops and removes the service files but leaves the cached
binary available for a future Connect.

Both paths place the executable and log under `~/.cache/tuic/`. The ephemeral
path also uses `tuic-remote.pid`; deleting an on-connect machine sends a
best-effort stop, while deleting an installed connection deliberately leaves its
service alone. The daemon is launched with `--no-agent-configs`, so deploying it
does not rewrite the host's Claude, Codex, or other agent configuration.

### Start a daemon or set its password, with confirmation

Two situations get an **offer** on the connection's row instead of a bare error.
Nothing on the remote host changes until you accept the exact plan.

- **Start remote daemon…** — a **Never deploy** connection with **Offer to start
  the remote daemon if it is not running** found nothing answering on its daemon
  port.
- **Set remote password…** — the daemon answered, but it has **no password
  configured at all** (never a wrong one: a wrong password stays a plain
  "rejected credentials" error), and this connection has an auth username and a
  saved password.

Either button opens a confirmation listing the destination (`user@host:port`)
and every command that will run there, in order, exactly as it will be sent.
**Accept and run** runs it; Cancel, Escape or a click outside runs nothing. If
the connection is edited between the dialog and the accept, the plan is refused
and must be reviewed again.

Starting uses the same path as **Deploy on connect**: the release asset pinned to
this app's version, checked against its published SHA-256 (512 MiB limit, 300 s
timeout; Windows hosts are refused), copied only when the installed binary's
hash differs, and launched on `127.0.0.1` with a pairing token read from stdin.
With an **Instance ID** the daemon is launched as `--instance <id>` and keeps its
own `~/.cache/tuic/tuic-remote-<id>.pid` and `.log`. After starting, the
connection connects.

Setting the password runs `tuic-remote [--instance <id>] --set-password-if-unset`
over the same SSH connection, with the saved username and password on stdin —
never in a command line, a log or a response. The daemon itself refuses when it
already has credentials, so an existing password is never overwritten; a saved
password with a line break or leading/trailing whitespace is refused rather than
altered. A running daemon reads its password at startup: restart it afterwards.
A daemon too old to know `--set-password-if-unset` refuses too, and nothing
changes.

**Disconnect** stops a daemon TUICommander started this way in this run, unless
**Leave it running on disconnect** is checked — or the daemon still has live
sessions (terminals or agents on that machine): those are never killed by a
Disconnect; the daemon keeps running and exits by itself once its last client has
been gone for the survive time. A later Disconnect with no sessions left still
stops it. The stop is PID-file verified: the
file must name a process that `ps` reports as `tuic-remote`, otherwise nothing is
signalled — a stale PID is never killed and nothing is stopped by name. A daemon
somebody else started is never touched.

### Update and restart a remote machine

When a connected daemon reports a different binary SHA-256 from the binary the
desktop would deploy, **Remote out of date** appears beside the connection.
Each connection has an **Auto-update remote daemons** option in Settings. It is
off by default. When enabled, a connection to a newer available build updates
automatically only if the daemon reports zero live PTY sessions. A daemon with
live sessions stays on its current build; Settings shows the session count and
offers the manual update. No update is queued. An automatic failure is shown
once for that selected build, without a retry on each reconnect.
The manual update button is disabled while an automatic update is running. The
backend rejects concurrent manual requests from IPC, HTTP, or MCP, and skips an
automatic update while a manual one is in progress. A stalled transfer reports
a timeout, after which connection checks resume.
Select **Update & restart remote** for either Direct or SSH transport. The
preview reports the remote target and build, selected desktop build and source,
and the number of live PTY sessions. Confirming ends those sessions. The
desktop uses the matching release asset first; if that asset does not exist, a
locally built `tuic-remote` beside the desktop executable is used only when its
target triple matches the remote. A target mismatch names both targets; a
missing local binary reports its expected path.
For a development build, build the headless binary with
`cargo build --bin tuic-remote --no-default-features` from `src-tauri` when the
desktop-feature sibling is a stub. The preview names that cause and command.

Direct updates stream the binary over the authenticated connection. The daemon
accepts the session cookie and the legacy URL token for binary and file uploads.
This release keeps the URL form for older clients; the next release will migrate
the update client to the cookie and remove the legacy query form. The daemon
verifies its target, size (512 MiB maximum), SHA-256 and confirmed session
count, stages it in its own install directory, then starts the new build. SSH
updates use the existing SCP deployment path. TUICommander waits for `/health`
to report the selected build after restart; the connection's status polling
re-authenticates when the daemon mints a new token. A changed session count or
binary between preview and confirmation cancels the update. In-process update
on Windows is unavailable: a running `.exe` cannot be overwritten, and the
daemon answers 501.

An older daemon without `/health.build` is shown as out of date. Automatic
updates require a build identity, so this daemon needs one manual installation.
SSH can bootstrap it because the desktop probes the host target with `uname`; an older
Direct daemon has no update endpoint or reported target, so it needs one manual
installation of a compatible daemon before this action can update it.

### Authentication

`tuic-remote` authenticates **every** TCP request. The headless build has no
loopback bypass and `run_remote` forces `lan_auth_bypass` off, so an SSH tunnel
does not make the daemon local: `GET /health` is the only unauthenticated route.
A connection without credentials therefore reaches `/health` and nothing else.

The password is kept in the OS credential vault, keyed by the connection's UUID.
`connections.json` holds the username only, and neither the vault entry nor the
daemon's token ever appears in `GET /config`. Deleting a connection deletes its
vault entries with it — the password and, for a deployed daemon, the pairing
token — over the desktop app and over HTTP alike.

The username is optional: a LAN desktop with `lan_auth_bypass`, or a daemon
TUICommander deployed (pairing token), needs none. If you store a password but
leave the username blank, the daemon refuses the login — it never signs you in
as "any user".

For a manually managed daemon, TUICommander trades the password for the daemon's
session token over `GET /api/auth/session-token` (Basic Auth). For Connect and
Install, the vault pairing token is the daemon session token itself and is sent
to the launch process over SSH stdin, never in its command line. TUICommander
then puts the session token in the query
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

Project Progress for a remote repository is stored on its owning machine.
New entries also reach the connected desktop window, so its Progress bell and
dialog update without moving the journal to the local machine.

### Remote Repositories and Terminals

Once a remote connection is configured:

- **Add remote repo** — Select a connection and browse from that machine's home directory. If a directory cannot be read, the picker explains the error and still allows entering a path or moving to its parent. The repo appears in the sidebar with a remote badge
- **Open terminal** — Terminals on remote repos connect via WebSocket to the remote daemon. A failed launch displays its error in the terminal pane; a failed stream connection, a missing initial frame after 15 seconds, or an unreadable compressed frame shows a persistent error toast. The client retries the stream and replays the current viewport on reconnect; the toast remains until dismissed. An idle terminal whose viewport or explicit empty replay has arrived is not treated as stalled
- **Health monitoring** — Connection health is polled periodically. Disconnected connections show a warning badge in the sidebar

Connections are stored in `<config_dir>/connections.json` with SSH, Direct and
Local transport types. An SSH connection keeps its SSH settings (host, port,
user, identity file, keepalive, host-key policy, compression) in the same nested
`ssh` block a tunnel profile uses. A `connections.json` or tunnel profile written
by an older build is upgraded once at startup; the original is kept beside it as
`<file>.pre-nested-ssh-<timestamp>.bak` (restore it if you go back to an older
build, which cannot read the new shape).

> **Downgrading:** an older build cannot read the new `connections.json`. The
> first time it saves a connection (add, edit or delete) it moves the file aside
> as `connections.corrupt-<uuid>` and starts an empty one, so your connections
> seem to vanish in that build — nothing is lost: the pre-migration backup
> `connections.json.pre-nested-ssh-<timestamp>.bak` is next to it. Before
> running the older build, copy that backup back over `connections.json`. A
> tunnel profile fails loudly in an older build instead and is not overwritten
> (restore its `.bak` the same way). **Local** — another TUICommander
instance on this same machine, by port or by instance id — connects over
loopback HTTP with the usual authentication.

#### Remote agent notices

An agent's MCP toast on a connected machine appears in the desktop notification
bell under **Messages**, with `[connection name]` before its title. It keeps the
requested level and sound. **Open terminal** switches to the originating remote
tab when it is still open; an unknown or closed session leaves focus unchanged.
Notices from a disconnected machine are not queued or replayed on reconnect.
Connections with the same display name retain separate notices. Malformed remote
notice text is discarded.

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
If a remote config cannot be read, the Agents tab shows the connection error and retries
on the next load. It does not save a fallback empty config over the remote file. A 404
for `/config/agents` means the remote daemon needs an update that includes this route.

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

`--set-password-if-unset` reads the same two lines but refuses — before reading
anything, and again before writing — when the instance already has a username or
password. It is the mode TUICommander's **Set remote password…** uses.

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

# Loopback-only ephemeral daemon with a 30-minute idle lifetime
TUIC_PAIRING_TOKEN=... ./tuic-remote --bind 127.0.0.1 \
  --survive-secs 1800 --no-agent-configs
```

By default the daemon binds to `0.0.0.0:<port>` and runs until signalled. The
desktop-managed form binds to loopback, skips agent config installation, writes
`tuic-remote.pid` in its config directory, and exits after the survive time with
no SSE or WebSocket clients. SIGINT, SIGTERM and SIGHUP all shut it down cleanly
and remove that pid file.

The daemon serves:
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
| LAN auth bypass | Disabled | Disabled |
| Signal handling | N/A | Graceful SIGINT/SIGTERM/SIGHUP |
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

## Private secret entry on a phone

A desktop secret request shows a one-time entry path in its private window.
Open that path on your trusted TUICommander server address and enter only the
requested fields. Entry uses the existing server authentication and transport;
use HTTPS to protect values in transit. No separate private listener is created. The headless daemon cannot originate requests in this slice.
See [Private secret forms](secrets.md).

## Address remote terminals and peers from desktop MCP

Use `session action=list` to discover connected remote terminals. Their
`connection_id` identifies the saved remote connection. Pass that field with
`session_id`, or use the returned `address`, for output and semantic submit.
Use `agent action=list_peers` to find remote peer addresses, then send mail to
`<connection_id>/<peer-id>`. Replies to `local/<peer-id>` return through the
desktop hub. Daemons can also address peers on another configured connection.
All traffic follows the configured daemon connection and credentials.

Both desktop and daemon need a version supporting remote peer mail. Local daemon
mail remains usable if the desktop disconnects. A cross-host error is explicit;
a timeout with uncertain delivery is not permission to resend the body.

The read-only reproduction harness is `scripts/test-remote-mcp.py`. Run
`python3 scripts/test-remote-mcp.py --connection <id> --session <alias-or-id>`
against the desktop MCP. `--exercise` submits and sends mail and must target a
disposable idle agent; `--second-connection` and `--second-session` check a
remote-to-remote reply through the same hub.

For a local protocol fixture, build a test-support headless binary and run
`python3 scripts/test-remote-mcp.py --fixture-bin <binary> --fixture-launcher scripts/run-remote-fixture.sh`.
This launches three actual isolated daemons, exercises the hub MCP, remote-to-remote
mail and replies, semantic shell rejection, then verifies intrahost mail with the hub
stopped. It does not claim to test a real agent composer; use `--exercise` for that.

Connected daemon notices carry their host identity. MCP confirmation responses and ACP permission/elicitation answers return to that daemon; disconnected questions disappear without changing local connections. MCP confirmation dialogs appear on the desktop and close when another client answers. AI Chat shows remote questions separately, with their ACP connection identity. Completing an earlier answer preserves questions announced by newer notices. Remote GitHub transitions fetch PR data from the repository owner, notify once, and do not run local repository automation. GitHub polling also works on the headless daemon. MCP upstream health refreshes use a separate host snapshot rather than this machine’s editable configuration.
