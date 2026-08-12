# PTY Management

**Module:** `src-tauri/src/pty.rs`

Manages pseudo-terminal sessions for all terminal tabs in the application.

## Session Lifecycle

```
create_pty() / create_pty_with_worktree()
    │
    ├── Resolve shell (platform default or user override)
    ├── Build shell command via portable-pty CommandBuilder
    ├── Spawn PTY pair (master + child process)
    ├── Store PtySession in AppState.sessions (DashMap)
    ├── Create OutputRingBuffer for MCP access
    ├── Spawn reader thread (background, non-blocking)
    │
    ▼
Session Active: write_pty() / resize_pty() / pause_pty() / resume_pty()
    │
    ▼
close_pty(cleanup_worktree)
    ├── Remove session from DashMap
    ├── Kill child process
    ├── Remove output buffer
    └── Optionally remove associated git worktree
```

Each session may also carry an orchestrator-owned `pty_description`, separate
from the last user prompt captured by input-line bookkeeping. MCP spawn and
input actions update it through `pty-description-changed`; desktop and browser
clients render it together with the last prompt above the terminal.

## Tauri Commands

### Session Creation

| Command | Description |
|---------|-------------|
| `create_pty(config: PtyConfig)` | Spawn a new PTY session. Returns session ID. |
| `create_pty_with_worktree(pty_config, worktree_config)` | Create worktree + spawn PTY in it. Returns `WorktreeResult`. |

Production spawn sites share `pty::spawn_pty_pair_with_retry` and its async
wrapper. Only explicitly classified PTY allocation failures (for example OS
resource exhaustion or an interrupted/would-block allocation) receive the
bounded three-attempt, 100/200 ms backoff. Once a PTY pair exists, command spawn
runs exactly once: invalid binaries, cwd, and permission failures return
immediately. Async Tauri and HTTP entry points run allocation and backoff on
Tokio's blocking pool; synchronous internal callers retain the same bounded
policy. Each site still owns its justified command/env/dimension assembly.

### Session Control

| Command | Description |
|---------|-------------|
| `write_pty(session_id, data)` | Write data (user input) to the PTY. |
| `resize_pty(session_id, rows, cols)` | Resize the PTY terminal dimensions. |
| `pause_pty(session_id)` | Pause the reader thread (stops output emission). |
| `resume_pty(session_id)` | Resume the reader thread. |
| `close_pty(session_id, cleanup_worktree)` | Close PTY and optionally remove worktree. |
| `update_session_cwd(session_id, cwd)` | Update session's working directory (called from frontend on OSC 7). |

### Monitoring

| Command | Description |
|---------|-------------|
| `get_orchestrator_stats()` | Active/max/available session counts. |
| `get_session_metrics()` | Total spawned, failed, bytes emitted, pauses. |
| `can_spawn_session()` | Check if under MAX_CONCURRENT_SESSIONS (50). |
| `list_active_sessions()` | List all sessions with cwd and worktree info. |
| `list_worktrees()` | List all managed worktrees. |
| `get_process_stats()` | CPU% and RSS for TUIC + all child process trees (desktop Tauri command). |
| `collect_process_stats(state)` | Same logic, callable from HTTP routes and MCP tools. |

## Reader Thread

Each session spawns a dedicated reader thread that reads from the PTY master fd:

```rust
spawn_reader_thread(reader, paused, session_id, app, state)
```

**Processing pipeline per read:**

1. Read raw bytes from PTY master (up to 64KB buffer for natural burst batching)
2. Strip Kitty keyboard protocol sequences (non-printable noise for consumers)
3. Push through `Utf8ReadBuffer` — accumulates bytes until valid UTF-8 boundary, returns safe string
4. Push through `EscapeAwareBuffer` — holds incomplete ANSI escape sequences (CSI, OSC, etc.)
5. Feed into `VtLogBuffer` for VT100-aware changed-row parsing and primary-screen log extraction (mobile/MCP consumers)
6. Write to `OutputRingBuffer` (64KB circular buffer for MCP access)
7. Serialize parsed events once with `serde_json::to_value` — reused for both Tauri IPC and event bus (avoids double serialization)
8. Broadcast to WebSocket clients (if any connected)
9. Emit Tauri event `pty-output` with `{session_id, data}` — **throttled to ~10/s** (≥100ms between emits). The desktop canvas renders from grid frames and discards this text (it only drives the frontend activity dot / `lastDataAt`); emitting per-chunk flooded the WebView main thread under output storms (`yes`), starving keydown so Ctrl+C never reached `write_pty`. Dropping intermediate chunks is safe — only a periodic "output happened" pulse is needed.

**Cursor-up clamping** — The `clamp_cursor_up()` function limits `ESC[nA` (cursor up) and `ESC[nF` (cursor previous line) sequences to prevent them from moving the cursor beyond the visible viewport. This replaced the previous DiffRenderer approach for simpler escape sequence handling.

**ANSI anomaly detection** — The `detect_anomalous_sequences()` function scans PTY output for unusual escape sequences (screen clears, cursor home, alt-screen toggles, scrollback clears) and logs them at warn level. This is a diagnostic tool for investigating scroll-jump issues.

**Pause behavior:** When `paused` flag is set (`AtomicBool`), the reader thread sleeps for 50ms instead of reading. This prevents output flooding during background operations.

**Exit detection:** When the read returns 0 bytes or an error, the thread:
1. Flushes remaining buffered data
2. Emits `pty-exit` event with exit code
3. Removes session from `AppState.sessions`
4. Updates metrics (decrement `active_sessions`)

### Frame Emission Pipeline

Frame emission is decoupled from PTY reading via a per-session **frame ticker** thread (same approach as iTerm2's Metal display-link renderer):

1. **Reader thread**: processes PTY data into the alacritty VT grid, sets a `grid_frame_dirty` AtomicBool flag
2. **Ticker thread**: every 16ms, checks the dirty flag → if set, serializes dirty rows via `serialize_dirty_rows()` → sends frame via `send_grid_frame()` (respects `grid_frame_in_flight` backpressure)
3. **Frontend**: coalesces paint triggers via `requestAnimationFrame` (~60fps)
4. **Ack handler**: clears only the in-flight flag; the ticker sends any dirty rows accumulated while the prior frame was in flight

This coalesces rapid writes (e.g. spinner CR+erase+rewrite within 16ms) into a single frame, eliminating flicker from intermediate erase states. The ticker exits when the reader's `running` flag clears, with a final flush to avoid losing the last frame.

**Synchronized output (DEC mode 2026).** TUIC advertises `Sy` in the spoofed `TERM_FEATURES`, so agents — Codex in particular — wrap each repaint in `ESC[?2026h` … `ESC[?2026l`. The vendored VTE buffers those bytes and applies them atomically on ESU, but its 150ms `SYNC_UPDATE_TIMEOUT` is **passive**: it records a deadline and never fires. The embedder must enforce it, and the ticker is the only wakeup that can — by definition no further PTY bytes are coming.

The ticker therefore checks the deadline **before** the non-dirty early return, via `flush_sync_timeout_if_needed()` on `VtLogBuffer`. Three details are load-bearing:

- A per-session `sync_update_active` AtomicBool, published by the reader after each `process()`, keeps idle sessions from taking the vt lock every 16ms. It mirrors real parser state, so a nested BSU (which re-arms the deadline rather than closing the update) keeps it set.
- A timeout flush **bypasses** `grid_send_min_interval_ms()`. A protocol deadline is not animation; throttling it back a tick defeats the purpose.
- Teardown calls `force_stop_sync_if_buffered()` before the final serialize — session exit is the other "no more bytes arrive" case, and without it buffered output dies with the session.

Without this enforcement a single BSU whose ESU is delayed or lost freezes the tab **indefinitely**: content buffers invisibly and only a later ESU releases it. That was the cause of Codex streaming appearing to eat text and then dump it all at once, and it made any binary containing the BSU bytes a permanent tab wedge.

### Headless Reader Thread

`spawn_headless_reader_thread()` — used for HTTP-created sessions (no Tauri app handle). Same pipeline but skips Tauri event emission; only writes to ring buffer and WebSocket. Includes `extract_question_line()` for silence-based question detection, session lifecycle events (`session-created`, `session-closed`), and full output parser integration.

Named agent sessions propagate their stable `display_name` through the
`session-created` event. That launch label is a replaceable base title: OSC and
structured intent titles may update it. An independent
`display_name_is_custom` flag protects only an explicit user rename and survives
frontend reconnection. Session snapshots also carry `is_remote`, so reconnecting
an HTTP/MCP-created PTY does not lose orchestration-only notification muting.

## Shell Resolution

```rust
pub(crate) fn resolve_shell(override_shell: Option<String>) -> String
```

Priority:
1. User override from settings (`override_shell`)
2. Platform default via `default_shell()`

Platform defaults:
- macOS: `/bin/zsh`
- Linux: `$SHELL` environment variable, fallback `/bin/bash`
- Windows: `powershell.exe`

## Buffer Types

### Utf8ReadBuffer

Handles the case where a multi-byte UTF-8 character (e.g., emoji, CJK) is split across two reads:

```rust
impl Utf8ReadBuffer {
    fn push(&mut self, new_bytes: &[u8]) -> String  // Returns valid UTF-8, keeps remainder
    fn flush(&mut self) -> String                     // Force-flush (lossy conversion)
}
```

### EscapeAwareBuffer

Prevents ANSI escape sequences from being split between two emissions. Detects incomplete CSI (`\x1b[...`), OSC (`\x1b]...`), and other escape sequences:

```rust
impl EscapeAwareBuffer {
    fn push(&mut self, input: &str) -> String  // Returns safe-to-emit portion
    fn flush(&mut self) -> String              // Force-flush buffered escapes
}
```

### OutputRingBuffer

Fixed-capacity circular buffer (64KB) that stores recent output for MCP access:

```rust
impl OutputRingBuffer {
    fn write(&mut self, data: &[u8])                    // Append data
    fn read_last(&self, limit: usize) -> (Vec<u8>, u64) // Read last N bytes
}
```

### VtLogBuffer

**Module:** `src-tauri/src/state.rs`

VT100-aware extractor that captures clean log lines from PTY output. Designed for mobile/browser clients that need readable text without ANSI noise or TUI screen garbage.

```rust
impl VtLogBuffer {
    fn new(rows: u16, cols: u16, capacity: usize) -> Self  // Create with terminal size
    fn process(&mut self, data: &[u8]) -> Vec<ChangedRow>   // Feed raw PTY bytes, return changed rows
    fn resize(&mut self, rows: u16, cols: u16)              // Update terminal dimensions
    fn screen_rows(&self) -> Vec<String>                    // Current VT100 screen content (for slash menu detection)
    fn screen_log_lines(&self) -> Vec<LogLine>              // Styled screen rows for mobile/REST (structural tokens stripped)
    fn lines_since_owned(&self, offset: usize, limit: usize) -> (Vec<LogLine>, usize) // Paginated reads (absolute offset, structural tokens stripped, chrome lines skipped)
    fn total_lines(&self) -> usize                          // Monotonic counter (never decreases on eviction)
    fn oldest_offset(&self) -> usize                        // Absolute offset of oldest retained line
}
```

**`ChangedRow`** — describes a row that changed between two `process()` calls:

```rust
struct ChangedRow {
    row_index: usize,   // 0-based row in the VT100 screen
    text: String,        // Clean text content (no ANSI)
}
```

**How it works:**

1. Maintains a `vt100::Parser` — a full VT100 screen emulator (24 rows × 220 cols default)
2. On each `process()` call, compares current screen rows against previous snapshot
3. Lines that have scrolled off the top are emitted to the log (diff-based detection)
4. **Separate alternate-screen contracts:** changed rows are still returned while a TUI app owns the alternate screen, so status/intent/question parsers keep working. Durable log extraction reads only primary-screen history, so fullscreen repaint noise never reaches mobile/MCP logs
5. Bounded by `VT_LOG_BUFFER_CAPACITY` (10,000 lines); oldest lines are dropped when full
6. **Monotonic cursor:** `total_lines()` returns a monotonically increasing count of all lines ever pushed (not the current buffer length). Clients use this as a stable cursor for paginated reads via `lines_since_owned(offset, limit)`. If a client's saved offset falls in the evicted range, it is clamped to `oldest_offset()`

**Resize:** When the PTY is resized, `VtLogBuffer.resize()` keeps the parser in sync and clears the previous-row snapshot (avoids false scroll detection after resize). If an alternate-screen app is active, the durable-log cursor is synchronized against the inactive primary grid, not the unrelated alternate history; normal shell capture therefore resumes on the first line after exit.

Each session gets its own `VtLogBuffer` stored in `AppState.vt_log_buffers: DashMap<String, Mutex<VtLogBuffer>>`.

## OSC 7 CWD Tracking

Shells that emit OSC 7 (`\x1b]7;file://hostname/path\x07`) report the current working directory after each command. TUICommander uses this to keep the Rust-side `PtySession.cwd` in sync:

1. **Frontend handler:** `terminal.parser.registerOscHandler(7, ...)` in `Terminal.tsx` parses the `file://` URL via `parseOsc7Url()`.
2. **Store update:** The parsed path is written to `terminalsStore` so the UI reflects the current directory.
3. **IPC persist:** The frontend calls `update_session_cwd(sessionId, cwd)` to update `PtySession.cwd` on the Rust side.
4. **Restart recovery:** The persisted cwd is used during session restore so reopened terminals start in the correct directory.
5. **Worktree reassignment:** When the cwd changes to a path inside a different worktree, the terminal tab is reassigned to the corresponding branch in the sidebar.

## Shell Environment Variables

`build_shell_command()` sets these environment variables for spawned PTY sessions:

| Variable | Value | Purpose |
|----------|-------|---------|
| `COLORTERM` | `truecolor` | Advertise 24-bit color support |
| `KITTY_WINDOW_ID` | `1` | Signal kitty keyboard protocol support for heuristic detection by Ink-based agents |
| `TERM_PROGRAM` | `ghostty` | Satisfy Claude Code's terminal allow-list for kitty protocol; also prevents macOS `/etc/zshrc` from sourcing `zshrc_Apple_Terminal` |
| `TERM_PROGRAM_VERSION` | `3.0.0` | Passes Claude Code's version gate (rejects `^[0-2]\.`) |

Additionally, `CLAUDECODE` is removed from the environment (`env_remove`) to prevent nested-session detection when TUICommander itself runs inside a Claude Code session. `NO_COLOR` is also removed from every PTY command immediately after construction because it may belong to a Codex parent that launched TUICommander, not to the independent child session. This does not force application color or override explicit command flags; a deliberate per-agent environment may restore `NO_COLOR` after sanitization.

## Child Process Priority

Each spawned shell is given a lower scheduling priority right after spawn
(`lower_pty_child_priority()`), so heavy workloads run inside a pane (`cargo
build`, bundlers, test runners) yield CPU to TUIC's own render loop and the rest
of the system. A child inherits the parent's priority **at fork time**, so every
process the shell later spawns is deprioritized too. The effect only bites under
contention — an idle machine still runs the build at full speed.

| Platform | Mechanism | Default |
|----------|-----------|---------|
| macOS / Linux | `setpriority(PRIO_PROCESS, …)` | nice **+10**, override via `TUIC_PTY_NICE` |
| Windows | `SetPriorityClass(BELOW_NORMAL_PRIORITY_CLASS)` | fixed |

Validated on an M4 Max under 14-core saturation: TUIC's UI goes from frozen
(nice 0) to responsive (nice +10). `BELOW_NORMAL` (not `IDLE_PRIORITY_CLASS`) is
the Windows analog — `IDLE` only runs when the whole system is idle, the
equivalent of macOS QoS-background, which would make builds crawl.

### macOS Thread QoS Elevation

On macOS, the PTY **reader thread**, the **frame ticker**, and the **keystroke-write thread** are all raised to `QOS_CLASS_USER_INTERACTIVE` via `pthread_set_qos_class_self_np` (`raise_thread_for_interactive_io()` in `src-tauri/src/pty.rs`, `thread_qos` module). This is complementary to the child-process renice: on Apple Silicon the scheduler is QoS-band driven — `nice` only reorders threads within a band. Without this elevation, TUIC's interactive-path threads ran in the default QoS band alongside compiler worker threads, causing input latency under heavy builds. Raising to `USER_INTERACTIVE` puts the interactive path in a higher scheduler band. macOS-only; a no-op on Linux/Windows.

## Session Conflict Flag File

When an agent reports a session conflict (session already in use or not found), TUICommander handles it via a flag-file mechanism instead of writing directly to the PTY.

**Flow:**

1. The output parser detects a session conflict message (`ParsedEvent::AgentSessionConflict`)
2. `ChunkProcessor` calls `mark_session_conflict()`, which creates a flag file named `no-session-inject.<TUIC_SESSION>` in the app config directory
3. Shell wrapper functions (zsh, bash, fish) check for this flag file before injecting `--session-id $TUIC_SESSION`
4. If the flag file exists, the wrapper skips session-id injection, allowing the agent to start a fresh session

This replaced the previous `maybe_reset_tuic_session` approach, which wrote `export TUIC_SESSION=...` directly to the PTY. Direct PTY writes could corrupt TUI output (e.g., Ink-based agents in raw mode). The flag-file approach is safe because it uses the filesystem as a side-channel — no bytes are injected into the terminal stream.

A debounce (`last_session_conflict_mark`) prevents creating multiple flag files within a short window for the same session.

## Ctrl-U Prefix Handling

Single-key PTY writes that should clear the current input line prepend `\x15` (Ctrl-U) on POSIX shells. The selection is **shell-family aware**, not host-platform aware: the detected shell (`bash`/`zsh`/`fish` → POSIX, `powershell`/`cmd` → Windows) drives the choice. Mixing PowerShell on macOS or a POSIX shell via WSL/MSYS now behaves correctly. Native Windows shells skip the prefix entirely to avoid inserting a literal `^U`.

Frontend input helpers route through `src/utils/sendCommand.ts`:
- `sendCommand(fn, text)` — full command: `Ctrl-U` (family-gated) + text + `\r`. Handles Ink raw-mode split writes.
- `sendPtyKey(fn, key)` — pass-through single key/escape sequence. No prefix, no trailing CR. Use for `ChoicePrompt` option keys, TUI app navigation, and any raw-stdin interaction.

Never write `text + "\r"` directly to a PTY — see `AGENTS.md`.

## OSC 133 Semantic Prompts

When the shell emits OSC 133 markers (modern bash/zsh/fish with the integration enabled), the reader records clean command lifecycles into the per-session knowledge store:

| Marker | Meaning |
|--------|---------|
| `OSC 133;A` | Prompt start — delimits a new prompt line |
| `OSC 133;B` | Command start — the user has pressed Enter, command is about to run |
| `OSC 133;C` | Command output start |
| `OSC 133;D[;exit_code]` | Command completed with the given exit code |

`ChunkProcessor.record_osc133_outcomes` consumes the markers and writes a `CommandOutcome { command, cwd, exit_code, classification, duration_ms, output_snippet }` into the session knowledge store. Classification is one of `Success`, `Error { error_type }`, `TuiLaunched { app_name }`, `Timeout`, `UserCancelled`, `Inferred`. `error_type` is inferred from the output snippet (e.g. `rust-error-borrow`, `npm-missing-module`, `python-traceback`).

**Fallback:** when OSC 133 is absent (plain shells, remote sessions), the silence timer still records an `Inferred` outcome so the AI agent loop has *something* to learn from. The `has_osc133_integration` flag on `AppState` tracks per-session whether real markers have been seen.

Persistence lives at `<config_dir>/agent-knowledge/<session_id>.json`. A 2 s debounced background task (`spawn_persist_task`) flushes `knowledge_dirty` sessions to disk. `load_all` rehydrates stores on app start.

## TUI Application Detection

`src-tauri/src/ai_agent/tui_detect.rs` tracks alternate-screen enter (`ESC[?1049h`) and leave (`ESC[?1049l`) to classify the terminal as:

```rust
enum TerminalMode {
    Shell,
    FullscreenTui { app_hint: Option<String>, depth: u8 },
}
```

`depth` is a counter for nested alt-screen pushes (e.g. `less` invoked from inside `vim`). Known app hints — matched heuristically from nearby screen rows — include `vim`, `nvim`, `htop`, `btop`, `lazygit`, `less`, `tmux`, `claude`, and others. The mode is surfaced on `SessionState.terminal_mode` and used by:
- `ai_terminal_get_context` — tells the model it's in a TUI so it prefers `send_key` + `wait_for` over line-oriented `send_input`.
- `SessionKnowledgeBar` — renders a `TUI` badge and accumulates `tui_apps_seen`.
- The agent safety layer — blocks Ctrl-U prefix injection while a TUI app is in the foreground.

## Silence-Based Question Detection

The reader thread tracks output silence to detect unanswered agent prompts. When the terminal stops producing output for 10 seconds after a line ending with `?` is detected, the session is treated as waiting for input. This complements the instant pattern-based detection in the output parser and catches generic questions that would cause too many false positives if detected immediately (e.g., streaming fragments like "ad?", "swap?").

**Question extraction:** `extract_question_line()` scans changed rows for a candidate, but a visible input-box anchor makes chat order authoritative: only the latest chat content above the current prompt may become a question. The changed-row fallback is used only when no prompt anchor is available. This prevents scroll/repaint from resurrecting a question retained above a later answer or completion. Question events carry the input `turn_epoch`, and the state accumulator rejects an event produced by an older turn.

**Echo suppression:** When the user submits a line — including bare Enter — the shared desktop/HTTP bookkeeping advances the turn, clears the current wait, and activates a 500ms suppression window (`suppress_user_input`). During this window, matching PTY echo is ignored for question detection.

**Single threshold:** All silence-based questions use a uniform 10-second timeout regardless of whether new output has arrived since the question was detected.

## Shell State (Busy/Idle) Detection

The backend combines explicit lifecycle markers, agent-specific screen evidence,
real output, and silence to emit `ShellState` events (`busy`/`idle`). Rust is the
single source of truth — the frontend does not derive activity from raw PTY data.
Before the first lifecycle observation, shell state is absent and detected-agent
state is `starting`; the internal null sentinel is never serialized as `idle`.

PTY lifecycle events update the authoritative `SessionState` through a lossless
single-consumer lane. The global broadcast bus remains the live SSE/WS transport,
where slow consumers may reconnect after lag, but a dropped broadcast copy cannot
strand the sticky awaiting/idle state.

**Transitions:**
- **Explicit markers:** OSC 133 shell markers and OSC 7770 agent hooks transition immediately. Output silence cannot override an observed hook `busy`; it ends on hook `idle`, a confirmed interruption, process exit, or a stable ready composer after the submitted turn produced real activity. The last path recovers safely when an idle hook is missed without letting the previous turn's composer cancel a fresh submission.
- **→ busy:** A submitted agent prompt, real output, an animated spinner, or an agent-specific `Working` screen transitions via atomic CAS (`try_shell_transition`). Positive screen evidence is evaluated even while the stored state is idle, so false-idle is self-healing.
- **→ idle:** The 1s silence timer is the sole heuristic idle path. Plain shells use 500ms; agents use 2.5s and must have no active sub-tasks. Agents with ready-screen adapters require the ready prompt to remain stable for 1.5s.
- **Interrupts:** Ctrl-C and bare Escape record `interrupt pending` but never force idle. Idle follows only after an interrupted/ready screen, explicit Stop, or process exit.

**Movement is the default busy signal (#446-596f):** "if the text above the input area moves, the agent is active." Post-cutoff `changed_rows` are text-equality diffed (`TerminalGrid::process`), so a byte-identical repaint produces no ChangedRow. Static completed summaries, hints, HUD bars, and banner art are inert. Spinner rows among the changed rows additionally refresh `last_output_ms` while they animate. Claude and Codex have narrowly scoped semantic presence exceptions described below because current versions can freeze a valid active status while a child or blocking hook runs.

**Agent screen adapters:** Gemini and Aider remain prompt-based (`Ready` or `Unknown`). Gemini and Codex accept composers only in the current bottom chrome zone (or the final three rows when no input box can be identified), so a historical submitted prompt or markdown quote cannot report `Ready`. Codex detects `Working`/`Ready`/`Interrupted` from its semantic status near that current composer; both `›` and the newer `»` composer glyph are accepted. Claude treats only a spinner-prefixed phase containing an ellipsis and parenthesized progress as `Working`; this outranks the empty `❯` composer that current Claude versions leave visible during long tools. Completed summaries such as `✻ Sautéed for 1m 25s` remain `Ready`. If Claude emits a premature Stop/suggest before a blocking Stop hook, a live phase marker reopens that turn and clears the stale completion suggestions. Grok similarly keeps its `❯` composer visible during a turn: a leading Braille spinner in its bottom status row is `Working`, and the stable composer becomes `Ready` only after that row disappears. This repairs the shell's long-lived OSC 133 busy marker even when native Grok hooks are disabled.

**Signal precedence and confirmation:** Explicit hook busy > current Claude/Codex/Grok semantic Working marker > movement (real output / animated spinner) > silence. A ready prompt visible from the previous turn cannot cancel a newly submitted prompt until real activity has been observed; after activity, a stable ready composer can repair a missed hook idle. A current-turn completion marker prevents a stale static Codex Working row from relatching BUSY; movement of that exact semantic row can reopen a Codex internal continuation that starts without PTY input. Claude's current live phase marker can supersede a premature completion from a blocking Stop hook. A pending process probe or confirmed meaningful descendant still owns the task lifecycle. Hook-based question suppression activates only after an OSC 7770 state marker is actually received.

**OSC 777 notification classification:** OSC 777 `notify` is a desktop-notification transport, not an awaiting-state protocol. Raw-stream parsing promotes only response-required wording (`needs your permission`, `approval required`, or `is waiting for your input`) to a confident question. This preserves plan/skill picker detection for hook-instrumented Claude sessions while ignoring the observed generic `Claude Code needs your attention` notification, which can announce completion and otherwise latches awaiting indefinitely.

**State-regression capture:** Enable `POST /diagnostics/capture` before reproducing (`{"enabled":true,"session_id":"<id>"}`), stop it afterward, and copy the exact `<config dir>/captures/<id>.tcap` file into `src-tauri/src/fixtures/agent_prompts/`. `GET /diagnostics/capture` reports the directory and bytes written. Framed records preserve input/output ordering, original chunk boundaries, and monotonic timestamps; legacy `.raw` fixtures remain output-only. Do not build fixtures from `/sessions/:id/output`: the bounded ring can overwrite the signal and its JSON string is lossy UTF-8.

**Safety consumers:** For agents with a verified screen adapter, peer-message injection and Unix auto-standby require confirmed idle (explicit Stop/OSC or stable ready screen). A silence-only idle can update the cosmetic state but cannot type into or `SIGSTOP` a potentially working agent. Agents without an adapter retain the legacy heuristic behavior until their UI is characterized.

**Task lifecycle is separate from shell activity:** `shell_state=idle` means the
PTY is quiet; it does not prove that the assigned task finished. An agent's
`suggest: [ ... ]` marker explicitly closes the current task epoch and produces
`agent_state=completed` plus a `state_change: completed` parent notification.
Likewise, a visible ready composer may coexist with an autonomous background
command. While a meaningful descendant of the agent is alive, session state
reports `background_work=true` and keeps `agent_state=working`; `shell_state`
remains `idle` because terminal input readiness is a separate fact. Persistent
integration helpers (`mdkb`, `tuic-bridge`, and `node_repl`) and Claude's
standalone timed `caffeinate -i -t <seconds>` assertion do not count as work;
Unix classification checks both `comm` and the authoritative argv path from
unlimited-width `ps` output. A `caffeinate` invocation that wraps a command
remains meaningful background work. Parent `idle` lifecycle mail is
deferred until the real descendant exits, while confirmed-ready message
delivery keeps using the terminal-readiness gate. The first confirmed-ready
observation and every explicit agent IDLE marker arm a generation boundary:
idle/completed lifecycle output waits until a process snapshot newer than that
observation or marker has been reconciled. Fresh working evidence starts a new
readiness episode even within the same task epoch: it invalidates only the
satisfied or pending snapshot boundary, so the next ready observation must
reconcile a newer snapshot while preserving tracked background work and the
snapshot generation. One app-wide process snapshot is collected at most once
per second on Tokio's
blocking pool and shared by every session. The refresher runs only while a
ready probe or tracked background process needs it, skips missed interval ticks,
and stops scanning stable idle sessions. Enumeration or parse failures preserve
the prior `background_work` value. On Windows, where Toolhelp does not provide
command lines, generic `node.exe` processes are kept as meaningful work rather
than guessed to be `node_repl` helpers.
Submitting new user or PTY-injected peer input starts a new task epoch immediately,
clearing the prior completion marker and its stale suggested actions before new output arrives.
Claude channel and inbox delivery do not claim a submitted turn; the channel is used only
inside an already working turn. Idle or completed managed composers take the PTY submission
path, and lifecycle changes only after that input or normal activity evidence. Idle
CAS and parent lifecycle notification share the same per-session lifecycle lock;
submitted epoch mutation and its IDLE-to-BUSY transition hold that lock as one
critical section, so a new turn cannot inherit a stale idle notification. The
authoritative parent inbox enqueue occurs under the child lock; parent terminal
wake/dispatch runs only after release, avoiding cross-session lock ordering. A
queued BUSY-to-IDLE transition also carries the task epoch observed before it
waited for the lock and is discarded if a new submitted turn won first.
Without a fresh marker the new task epoch returns to `idle`, not `completed`.

**Transactional peer injection:** Reserving an idle composer creates an ownership token before the PTY write. A failure proven to occur before any byte was written rolls the synthetic BUSY state back to the prior confirmed IDLE state and keeps the message queued. Once any byte may have escaped, failure is `delivery_uncertain`: the session remains conservatively BUSY, the authoritative inbox remains readable, and TUIC does not automatically retry into the terminal. Real output, a Working screen, or an explicit state marker invalidates rollback ownership so a late error cannot erase genuine activity. `session status` exposes the additive `delivery_uncertain` flag.

**User-composed commands share that gate:** the Compose panel's enqueue action
(`enqueue_agent_command` / `POST /sessions/:id/queue`) appends to the same
typed `pending_injections` FIFO as peer delivery rather than writing to the PTY,
so a command typed by the user cannot steer a turn in progress. It is appended
and then flushed, never handed straight to `deliver_message_to_pty`: injecting
ahead of any accepted peer message or user command would reorder delivery. Each
flush pops one entry and leaves the session BUSY, so the queue drains one item per
idle window in global acceptance order. `state.queued_commands`,
`list_queued_agent_commands`, `remove_queued_agent_command` and
`clear_queued_agent_commands` select only user-command entries; clear retains all
peer/orchestrator entries in their original relative order. Each user command
carries a process-unique id so the Compose panel can delete a single entry —
a queue position would shift under the caller as the FIFO drains.

**Status line ticks:** Animated spinner repaint evidence refreshes both shell activity and `SilenceState`, preventing low-confidence question/tool-error events from contradicting a busy tab. Static mode/footer rows remain chrome only and do not prove activity.

**Status line dedup is per turn:** `ChunkProcessor.last_status_task` keys its dedup on `(turn_epoch, task_name)`, so a spinner rotation inside one turn stays suppressed while the first status line of a *new* turn always re-emits. The epoch must stay in the key because an agent may name every turn identically — Codex always reports `Working`. A session-lifetime dedup swallowed every turn after the first, and since the `status-line` event is the only thing that clears the previous turn's `suggested_actions` (which `session_state_with_shell` reads as a completion marker), the session reported a busy agent as `completed`/`idle` permanently.

**Agent detection:** `detectAgentForTerminal()` fires on shell-state transitions (immediate on idle, 500ms debounce on busy). A 30s fallback poll catches cold starts. This replaces the previous 3s polling interval, reducing syscalls ~30x.

## Amber Tab Styling

Sessions created via HTTP/MCP (remote sessions) are flagged with `isRemote`. The tab bar applies an amber gradient background and amber bottom border (`rgba(251, 191, 36, ...)`) to visually distinguish remote-created sessions from locally spawned ones.

## Concurrency

- Sessions stored in `DashMap<String, Mutex<PtySession>>` for lock-free concurrent access
- Each session's writer has its own shared `Mutex`, independent from the
  `PtySession` metadata lock. User input, HTTP/WebSocket input, agent injection,
  and terminal-generated protocol replies all serialize through that writer.
  A reader may therefore wait for an in-flight write without blocking PTY
  draining, and mandatory device/kitty query replies are never dropped merely
  because session metadata is contended.
- Reader thread holds `Arc<AtomicBool>` for pause signaling
- Metrics use `AtomicUsize` for zero-overhead counting
