# Terminal Stress Regression Suite

This directory contains repeatable end-to-end regressions for terminal data
integrity and agent lifecycle detection. It complements the in-process Rust
tests in `src-tauri/src/terminal_grid.rs` and `src-tauri/src/pty.rs` by driving a
real PTY through TUICommander's HTTP transport.

## Safety

The runner creates a throwaway shell session, verifies only that session, and
deletes it in a `finally` block. It refuses port `9876` by default because that
is normally the orchestrator instance containing live user sessions.

Start a worktree build with `make dev`; it normally binds to `9877` when the
orchestrator already owns `9876`. Then run:

```bash
python3 tests/terminal-stress/run.py --base-url http://127.0.0.1:9877
```

### Against a headless instance (no window, fully isolated)

`make dev` opens a window and shares the config dir with the orchestrator. To
verify a HEAD build without either, run the headless binary under its own `HOME`
so it gets a private config dir, credential store and session set:

```bash
cargo build --no-default-features --bin tuic-remote
export STRESS_HOME=/tmp/tuic-stress-home && mkdir -p "$STRESS_HOME"
printf 'stress\nstresspass\n' | HOME=$STRESS_HOME ./target/debug/tuic-remote --set-password
HOME=$STRESS_HOME TUIC_PORT=9879 ./target/debug/tuic-remote &
python3 tests/terminal-stress/run.py --base-url http://127.0.0.1:9879 \
  --auth stress:stresspass --scenario all --count 2000
```

`--auth` is required here and only here: the headless binary has no desktop
loopback bypass, so it demands Basic Auth even on 127.0.0.1. Setting the password
without the `HOME` override would write credentials into the REAL config.

To deliberately test a primary instance:

```bash
python3 tests/terminal-stress/run.py \
  --base-url http://127.0.0.1:9876 \
  --allow-primary
```

## Scenarios

- `atomic`: 2,000 DEC 2026 synchronized updates. Each record is first written
  partially, erased, then written completely. Escape sequences and payloads
  are split at deterministic irregular byte boundaries.
- `progressive`: writes the partial and replacement row in separate synchronized
  frames, modeling visible token-by-token growth while scroll and resize requests
  race with the producer.
- `timeout`: the same workload with periodic pauses longer than the synchronized
  update timeout before the erase-and-complete phase.
- `reflow`: commits a PARTIAL row to history with its own newline, then prints
  the complete extension as a separate row, while the runner resizes the
  viewport underneath. Models the shape reported in story #498-7e3d. Its
  verifier is deliberately different: it does NOT require the partial row to
  disappear (the producer really printed both, and discarding one would be the
  grid silently dropping history) — it requires that the grid does not
  MANUFACTURE a copy of the complete row under reflow.
- `slash-pressure`: enters TUIC's slash-command mode through a no-echo producer
  handshake, then emits the atomic workload. This reproduces the per-chunk
  slash-menu parser pressure that once generated thousands of application-log
  records during sustained agent output.
- During both scenarios the controller repeatedly scrolls and resizes the
  terminal while output is still arriving.

For every scenario the verifier requires every expected `REC-NNNN` record to
exist exactly once and byte-complete in the canonical backend grid. Missing,
truncated, or duplicate records fail with the relevant record IDs.

## Capturing a live anomaly

`capture.py` snapshots the three things a grid anomaly can only be diagnosed
from together — the raw ring, the dimensions, and the canonical rows — in one
shot, because two of them are volatile and the ring rotates:

```bash
python3 tests/terminal-stress/capture.py --session <id> -o capture-dir
```

It also flags adjacent rows where the next row EXTENDS the previous one, which
is the reported #498-7e3d shape.

The suite does not erase legitimate terminal history. If an application scrolls
a partial row into history and only later prints an extended version, both rows
are valid terminal output and cannot be deduplicated safely without an
application-specific semantic signal. Preserve a raw-ring capture when such an
episode occurs so a new deterministic producer sequence can be added here.

The screen snapshots in `fixtures/` are sanitized captures of real false-idle
layouts. Rust lifecycle tests load these fixtures so changes to agent chrome can
be reviewed independently from the assertions.

## Extending the suite

Add producer behavior as a named scenario in `producer.py`, keep all schedules
deterministic, and document the exact invariant here. A regression fixture must
contain no repository secrets, credentials, or full user prompts.
