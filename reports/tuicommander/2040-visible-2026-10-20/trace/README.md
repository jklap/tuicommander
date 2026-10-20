# Sentinel omission boundary — 2026-10-20

Story: `2040-16b2`. Continuation peer: `desk-2040e`. Base: `fd17b424f83073ddd431d40a943c00af49166e21`, branch `chore/2040-visible-recheck`.

## Verdict

**The reproduced omission is benign viewport scrolling before serialization, not a glyph lost from a visible serialized cell.** The boundary is PTY chunk processing into the terminal grid, followed by a later ticker snapshot of the current viewport. `TerminalGrid::process` advances all bytes in a chunk (`src-tauri/crates/tuic-terminal/src/terminal_grid.rs:793`); the ticker serializes later under the VT lock (`src-tauri/src/pty.rs:4293`). `serialize_dirty_rows` encodes the current viewport, not every intermediate screen (`terminal_grid.rs:2628-2680`).

The inherited trace establishes this for ⑤, ⑨ and ⑪. The single authorized repeat establishes it for ⑦. In each case the glyph first exists on screen, then exists in scrollback **above the viewport before the first eligible serialization**. It is absent from that snapshot's screen and cell bytes, from all native submissions, and from recorded frontend arrivals. There is no visible cell for the delivery pipeline to lose at that point.

**No RED was written:** these observations do not reproduce a violated serializer or delivery contract. No production fix, throttle change, test, or story closure is justified by this omission.

This is a boundary diagnosis from two instrumented native runs, not a reconstruction of the earlier visible run's exact scheduling. Both instrumented runs report `document.visibilityState=hidden` despite native visibility being true. They do not certify visible typing latency, a stable composer, trusted keyboard input, or compositor presentation. The original native GREEN and REALITY criteria remain open.

## Decisive observations

All row coordinates below use the monotonic screen-origin coordinate system. The captured grid is **14 rows × 37 columns**, not the earlier visible run's 15 × 38. All four first appear at screen row 13, column 0.

| Glyph | Trace | First VT event / glyph absolute row | Last VT event before cut | First eligible cut / viewport top | Glyph above viewport by | In cut cells / native submits / frontend arrivals |
| --- | --- | --- | --- | --- | --- | --- |
| ⑤ | `../trace-inherited` | 258 / 458 | 263 | 265 / 491 | 33 rows | no / 0 / 0 |
| ⑦ | `trace` | 256 / 530 | 258 | 261 / 540 | 10 rows | no / 0 / 0 |
| ⑨ | `../trace-inherited` | 300 / 674 | 302 | 305 / 684 | 10 rows | no / 0 / 0 |
| ⑪ | `../trace-inherited` | 317 / 746 | 323 | 325 / 779 | 33 rows | no / 0 / 0 |

For example, repeat ⑦ is on screen in event 256 at epoch microseconds `1792493289124396`, with viewport top 517 and glyph row 530. Event 258 at `1792493289133348` retains it in scrollback while viewport top is 540. The first eligible cut, event 261 at `1792493289141994`, has no ⑦ in either its screen or encoded cells. The glyph has already moved ten rows above the viewport.

The four relevant cuts record `min_interval_ms=50`, `input_recent=true`, and `saturated=true`. There is no intervening gate/floor deferral event for these particular glyph-to-cut intervals. The floor was active, but these observations do **not** isolate its causal contribution from logging overhead or scheduling. No change to frame policy was made.

The repeat also supplies positive controls: ③ (event 220), ⑧ (270) and ⑪ (298) remain on screen at the first eligible cut. Each occurs in the serialized cell bytes, one native submission and one frontend arrival. No `desktop-decision:failed` event occurs in either trace. `desktop-submit` is logged immediately before the channel call, so it is an attempted submission, not independent proof of frontend receipt; the arrival recorder is the receipt evidence for these controls.

## Trace interpretation

- `native.log.gz` is a lossless gzip archive of the captured native stdout/stderr. It includes every temporary `TUIC2040` event and the native build/launch output. `observer.patch` records the exact temporary source diff. The patch is evidence only and is **not applied** in the final tree.
- `timeline.json` retains native log sequence and epoch timestamp, each chunk's sentinels, VT-lock screen/scrollback sentinel locations, geometry, tick decisions, encoded payload sentinel cells and payload hashes.
- `summary.json` gives each sampled key's intake, last VT state before cut, first eligible cut, serialized-cell presence, native submission count and frontend arrival count. These are independent runs, not pooled latency samples.
- `frame-workload.json` retains frontend arrivals. Recorder drops are zero in both runs. Native submission counts include frames outside the recorder window; unmatched whole-run hashes must not be read as transport loss. The repeat records 94 submissions versus 50 arrivals in the narrower recorder window; inherited records 98 versus 54.
- Some dispatched keys do not appear even in the instrumented PTY intake (repeat includes no ⑤ or ⑨; inherited includes no ⑦). Their absence is **not** classified as a serialization omission. It is outside the proven boundary here. All four requested glyphs do have an observed scrolling example across the two traces.
- The inherited patch uses the cached `screen_rows()` snapshot. The repeat patch instead reads the live physical screen through `read_rows_in_range(history, history + rows - 1)` under the same lock. This avoids relying on cache freshness for the repeat's decisive observation.
- `read_rows_in_range` uses retained-history-relative coordinates, whereas `grid_screen_origin` is monotonic across resize/reflow. In these traces history is two rows greater than origin. The analyzer converts a tail coordinate with `tail_start + index + origin - history`; it does not compare unconverted history-relative rows to monotonic origin. A screen row uses `origin + row`.
- Native event times are epoch microseconds, not monotonic timestamps. The analyzer retains log sequence as well; the decisive VT application and serialization snapshots run under the same VT lock. No cross-run clock comparison or sub-millisecond latency claim is made.

## Execution and cleanup

1. Read the inherited trace first and ran its analyzer. It proved scrolling for three requested glyphs but had no intake for ⑦.
2. Preserved that run under `../trace-inherited/`, including its observer patch and cleanup record.
3. Ran one repeat via `tuic-bg.sh .../desk-2040e-repeat.log -- python3 .../run_trace.py`. The helper calls `make agent-test AGENT_TEST_INSTANCE=at-2040c AGENT_TEST_PORT=19891`, whose build uses build-slot/mbx. This is authorized native Mac execution, not an rb test result. All application probes used the instance Unix socket, never the production API or TCP authentication.
4. The repeat completed measurement and copied the frame recording, then exited 1 at its explicit visibility guard: `Trace run was not visible; do not claim paint timing`. This failure is retained and is not reported as a visible test pass.
5. Its `finally` cleanup deleted all sessions, ran exact-instance `make agent-test-stop` successfully, waited for its owned supervisor and found no worktree app processes. See `cleanup.json`: zero sessions, stop exit 0, no processes. The supervisor exits 2 because make reports the intentional child termination.
6. A final exact-instance stop returned `no running agent-test supervisor at this instance` (make exit 2); this confirms the already-stopped lifecycle, not a second cleanup success claim. No separate Vite or browser session was started.
7. Removed the temporary Rust observer. `git diff --exit-code -- src-tauri/src/pty.rs` passes. Only report artifacts are committed.
8. Static validation: instruction-links passes two pairs; `fmt-changed.sh <worktree> main...HEAD` checks zero applicable code files and passes; Python AST and retained JSON parsing, analyzer regeneration and `git diff --check` pass. No Rust/JS test suite, RED/GREEN run, mutation, or release build was performed for this evidence-only result.

## Reproduce the analysis

From the checkout root, run:

```sh
python3 reports/tuicommander/2040-visible-2026-10-20/trace/analyze_trace.py
python3 reports/tuicommander/2040-visible-2026-10-20/trace-inherited/analyze_trace.py
```

These commands decompress the retained logs and regenerate their local `summary.json` and `timeline.json`. The observer sources are retained to explain acquisition, not as a request to launch another native instance. A future acquisition requires explicit authorization and applying the matching temporary observer patch; `run_trace.py` itself does not apply it. No new `.tcap` fixture was produced, so no replay-oracle regeneration is required.
