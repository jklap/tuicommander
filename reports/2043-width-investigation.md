# Story 2043-4419: width investigation

## Status

The reported narrow agent layout is not reproduced. No production code changed.
The story must remain open. The new test passes on production revision
`96338d2199888df6f8126bd919a7b76c5e0d9705`; it is not a RED reproduction.

## Observations

Read-only inspection of the running desktop through `debug invoke_js` on
2026-10-18 confirmed the reported terminal identity:

- Terminal: `term-863`, Follow-ups and push, Claude.
- PTY: `546b8989-f4e8-4d68-8843-920d9aa5a619`.
- Retained canvas CSS width: 1774 pixels. The tab was hidden during inspection.
- `terminal_styled_rows` returned 221 grid columns.
- Composer rules at absolute rows 6728 and 6730 had nonblank cell index 220.
- Several retained answer rows had nonblank cells at indices 215 through 218.
- The raw PTY output contained a 221-column composer rule and explicit cursor
  addresses beyond column 200. Thus the agent itself had emitted wide rows.

This does not prove what the screenshot showed earlier, or that the hidden
canvas would paint those cells correctly when shown. No live user tab was
selected, resized, or sent input. No desktop process was restarted.

A separate throwaway session created through the running TUIC MCP session path
at 24 rows and 80 columns ran a foreground Python probe. It printed:

```text
INITIAL os.terminal_size(columns=80, lines=24)
WINCH os.terminal_size(columns=200, lines=40)
```

The second sample came from the probe's SIGWINCH handler after MCP session
resize. The throwaway session was closed after the check.

## Source path audited

- `Terminal.tsx`: `calcGridSize` and `initSession` compute initial dimensions and
  resize existing sessions before attaching them.
- `CanvasTerminal.tsx`: `measureNow` computes rows/columns from cell metrics;
  the visibility observer calls `remeasure` when showing a hidden terminal.
- `transport/http.ts`: `rpc` serializes/coalesces resize requests per session,
  including desktop IPC requests.
- `pty/commands.rs`: `create_pty` and `create_pty_with_worktree` assemble shell
  spawns; `resize_pty` uses the shared off-thread resize core.
- `agent.rs`: direct agent spawn passes the requested dimensions to the same
  PTY allocation helper.
- `mcp_http/session.rs`: direct and shell spawns register the grid using the
  requested geometry.
- `pty/lifecycle.rs`: `spawn_pty_pair_with_retry` supplies geometry to openpty;
  `resize_session_core` serializes grid and master resize under a session lock,
  updates the grid before signaling the child, and records successful geometry.
- `tuic-terminal/src/vt_log.rs`: `VtLogBuffer::resize` passes rows and columns to
  the alacritty grid, with reflow policy selected separately.

No fixed 80-column override was found in the audited resize path. This is a
scoped observation, not proof that every possible launch or observer is correct.

## Targeted test

`pty::tests::production_spawn_resize_reaches_child_winsize` uses the production
`spawn_pty_command` and `spawn_pty_session` entry points and
`resize_session_core`. Its foreground Python child reads its own kernel winsize
with `os.get_terminal_size(0)` in a SIGWINCH handler. The test checks grid
geometry against that independent child result.

Cases cover direct and shell spawns, a resize immediately after spawn without
waiting for child readiness, and normal 80-to-200, 200-to-60, 60-to-200 changes.
The Unix-only test requires `/usr/bin/python3`. Scratch data is under the
repository test temp root. An RAII guard closes every spawned session.

The first shell-based probes were unsuitable: an interrupted shell `read` could
exit on Linux, and an interactive shell could give its foreground `sleep` a
separate process group. Those failures are not evidence of the reported defect.
The final probe directly owns the foreground and passed on both hosts.

Final command inside the build-slot runner:

```sh
scripts/with-test-tmp.sh cargo nextest run \
  --manifest-path src-tauri/Cargo.toml --lib \
  -E 'test(production_spawn_resize_reaches_child_winsize)'
```

- Native macOS, through mbx/build-slot: 1 passed; test 1.828 seconds, build
  3 minutes 16 seconds.
- rb Linux, through build-slot `--remote` with `RB_FORCE=1` after native
  validation: 1 passed; test 1.254 seconds, build 4 minutes 7 seconds.
- Final log: `~/Gits/.tmp/fix-tuic-2043-python-probe.log`.
- rb result: `~/Gits/.tmp/rb/20261018T221250Z-55617`.

No full suite, clippy, mutation, or unrelated frontend test ran.

## Remaining gap

A fresh occurrence must correlate the visible pane and rendered cells with the
same moment's grid, raw agent output, kernel geometry, and resize history.
The current observations do not distinguish a historical startup/repaint race
from retained narrow hard-wrapped output or a renderer-only issue. No such cause
is claimed. Claude and ego narrow-to-wide visual acceptance and hidden-tab
return acceptance remain unverified. No production guard or forced resize was
added without a reproduced failure.
