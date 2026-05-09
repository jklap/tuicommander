# SSH Tunnel Management

## Architecture

```
TunnelManager
  └── TunnelSupervisor (per profile)
        ├── SSH child process (tokio::process::Command)
        ├── BackoffCalculator (reconnect timing)
        ├── ExitClassifier (stderr → ExitReason)
        └── AuditLog (SQLite WAL)
```

The **TunnelManager** orchestrates multiple **TunnelSupervisor** instances, one per active tunnel profile. Each supervisor owns an SSH child process and runs a supervision loop:

1. Validate the profile (fields, port ranges, duplicate bind ports)
2. Check local port availability for all `-L` forwards
3. Spawn `ssh` with constructed arguments (including agent forwarding if `SSH_AUTH_SOCK` is found)
4. Health check: if the process dies within 500ms, classify the exit immediately
5. If the process survives 500ms, mark as **Connected** and reset the backoff counter
6. On process exit, classify the exit reason from stderr patterns and exit code
7. If retryable, wait the backoff delay and loop; otherwise, stop

### Shutdown

`TunnelSupervisor::stop()` sends a signal via a `oneshot` channel. The supervision loop catches this at any `tokio::select!` point and performs graceful shutdown:

- Unix: SIGTERM to the SSH process, wait up to 5 seconds, then SIGKILL
- Windows: `child.kill()` immediately

## Profile Configuration

Profiles are TOML files with this structure:

```toml
id = "550e8400-e29b-41d4-a716-446655440000"
name = "prod-db-tunnel"
host = "bastion.example.com"
port = 2222
user = "deploy"
identity_file = "/home/deploy/.ssh/id_ed25519"

[[forwards]]
type = "Local"
bind_port = 5432
remote_host = "db.internal"
remote_port = 5432

[[forwards]]
type = "Remote"
bind_port = 9090
local_host = "127.0.0.1"
local_port = 9090

[options]
server_alive_interval = 15
server_alive_count_max = 3
strict_host_key_checking = "Yes"
```

### Storage Scopes

| Scope | Path | Precedence |
|-------|------|------------|
| Global | `<config_dir>/tunnels/*.toml` | Base |
| Per-repo | `<repo>/.tuic/tunnels/*.toml` | Overrides global (same ID) |

`ProfileStore::load_all()` merges both scopes, with per-repo profiles taking precedence.

## Tunnel States

```
Starting ──────► Connected
    │                 │
    │                 ▼
    │           Reconnecting ──► Connected (backoff reset)
    │                 │
    │                 ▼
    ▼           Stopped (max retries)
Error
    │
    ▼
Stopped
```

| State | Meaning |
|-------|---------|
| Starting | SSH process is being spawned |
| Connected | SSH process survived health check; tunnel is operational |
| Reconnecting { attempt, reason } | Process exited with retryable reason; waiting backoff before retry |
| Stopped { reason } | Terminal state: user requested stop, max retries exceeded, or non-retryable exit |
| Error { message } | Validation failure or spawn error; no process was created |

## Exponential Backoff

`BackoffCalculator` computes retry delays:

- **Base**: 1000ms
- **Formula**: `min(base * 2^attempt, 30000)` + jitter
- **Jitter**: +/-25% of computed delay (uniform random)
- **Floor**: 100ms minimum delay
- **Max retries**: 10 (returns `None` after exhaustion)
- **Reset**: called on successful connection (attempt counter returns to 0)

Example sequence (base values, before jitter):
1s, 2s, 4s, 8s, 16s, 30s, 30s, 30s, 30s, 30s

## Exit Classification

`classify_exit()` inspects SSH stderr output first, then falls back to exit code:

| Pattern | ExitReason | Retryable |
|---------|-----------|-----------|
| "Permission denied" / "Authentication failed" | AuthFailed | No |
| "Host key verification failed" / "REMOTE HOST IDENTIFICATION HAS CHANGED" | HostKeyMismatch | No |
| "Address already in use" / "Could not request local forwarding" | PortInUse | No |
| "Connection refused" | ConnectionRefused | Yes |
| "Network is unreachable" / "No route to host" | NetworkDown | Yes |
| "Connection timed out" | Timeout | Yes |
| Exit code 130 (SIGINT) / 137 (SIGKILL) | UserKilled | No |

## Audit Logging

`AuditLog` uses SQLite with WAL journal mode for safe concurrent access.

### Schema

```sql
CREATE TABLE tunnel_events (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    tunnel_id TEXT    NOT NULL,
    kind      TEXT    NOT NULL,
    detail    TEXT    NOT NULL DEFAULT '{}'
);
```

Indexed on `tunnel_id` and `timestamp`.

### Event Kinds

`Started`, `Connected`, `Disconnected`, `Error`, `Retry`, `Stopped`

### Operations

- `insert(tunnel_id, kind, detail)` — Record an event
- `query_by_tunnel(tunnel_id, limit)` — Most recent N events for a tunnel
- `query_by_time_range(from, to)` — Events within a time window
- `rotate(max_age_days)` — Delete events older than N days

## UI Components

### TunnelsPanel

Main panel listing all tunnel profiles with:
- Profile name and host
- TunnelStatusBadge showing current state
- Start/Stop toggle button
- Edit button opening TunnelEditorModal

### TunnelEditorModal

Form for creating and editing profiles:
- Name, host, port, user, identity file fields
- Port forwards list with add/remove
- Options section (keepalive, host key checking)
- Validation errors shown inline

### TunnelStatusBadge

Color-coded status indicator:
- Green: Connected
- Blue (pulsing): Starting
- Orange: Reconnecting (shows attempt number)
- Red: Error
- Grey: Stopped

## Integration with Remote Connection Manager

When creating an SSH remote connection (`RemoteConnection` with `RemoteTransport::Ssh`), a tunnel profile is automatically created to forward the daemon port. The tunnel supervisor manages the SSH connection, and the remote connection routes API calls through the forwarded port.

## Module Map

| Module | Responsibility |
|--------|---------------|
| `tunnels/profile.rs` | Data model: TunnelProfile, ForwardSpec, ProfileOptions |
| `tunnels/command.rs` | Build SSH command-line arguments from a profile |
| `tunnels/classifier.rs` | Classify SSH exit reasons from stderr/exit code |
| `tunnels/agent.rs` | Discover SSH_AUTH_SOCK for agent forwarding |
| `tunnels/port.rs` | Check if a local TCP port is available |
| `tunnels/backoff.rs` | Exponential backoff with jitter |
| `tunnels/audit.rs` | SQLite audit log (WAL mode) |
| `tunnels/supervisor.rs` | Per-tunnel supervision loop |
| `tunnels/storage.rs` | TOML profile persistence (global + per-repo) |
| `tunnels/manager.rs` | Orchestrate multiple supervisors |
| `tunnels/commands.rs` | Tauri command handlers |
