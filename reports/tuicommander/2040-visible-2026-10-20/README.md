# Visible native terminal recheck — 2026-10-20

## Scope and disposition

Story: `2040-16b2`. Peer: `desk-2040d`. Tested source: `c7c69e1a49eaa2ff01e45855b00fa550b9e7c7c5` on `chore/2040-visible-recheck`. `git merge --no-edit main` reported `Already up to date` before the build.

This is native macOS WebKit evidence from the isolated `at-2040c` instance. All application API requests used its Unix socket from `launch.json`. No production app restart, production desktop capture, credential read, TCP API access, native OS key event, or browser session was used.

**The visible-window repeat found no lost backend echoes: 15/15 in both visible runs. `/help` met the 2x output-time comparison. The under-one-frame input-to-paint condition did not pass.** This report does not close the story or certify compositor presentation.

## Contract and method

- Consumer observation: a JS-dispatched character reaches the PTY echo and, when still present in the viewport, a matching canvas `fillText` call while paced rows print.
- Real state: production terminal key handler, native Tauri IPC, kernel PTY echo, grid delivery callback and production canvas renderer.
- Controlled external input: unchanged `scripts/stall-workload/output.py`, one session, 15 circled-number keys at requested 150 ms intervals. This is synthetic output, not a recorded agent failure.
- Input events have `isTrusted=false`. `fillText` completion is a canvas-submission proxy. Double rAF is a scheduling proxy, not proof of display presentation.
- Socket echo time is an **observation upper bound**, including polling interval, curl startup and socket request time. It is not the PTY-read timestamp.
- The main layered run captured actual outgoing `ipc://localhost/write_pty` fetches without inspecting auth headers, their response times, frame-arrival bytes, decoded paint state and matching glyph calls. Tauri's internal `invoke` is non-writable; the first run's attempted wrapper did not install. Its empty `ipc` array is not evidence of no IPC. The follow-up used a verified fetch wrapper instead.
- Both visible runs report `document.visibilityState=visible`, native visibility true, document focus false and native focus false. The window is 640x420 logical pixels at DPR 2.

## Results

All percentiles below use nearest-rank p95; raw samples are retained in `summary.json`.

| Observation | Initial visible run | Layered visible run |
| --- | ---: | ---: |
| Dispatched JS keys | 15 | 15 |
| Backend output echoes | 15 | 15 |
| Missing backend echoes | 0 | 0 |
| Dispatch → socket echo observation p50 / p95 | 17.981 / 30.160 ms | 17.449 / 53.262 ms |
| Dispatch → IPC response p50 / p95 | not instrumented | 1 / 2 ms |
| Keys with matching glyph calls | 11 | 11 |
| Dispatch → matching `fillText` p50 / p95 | 44 / 78 ms | 25 / 73 ms |
| Dispatch → frame arrival p50 / p95, matched keys | not instrumented | 18 / 52 ms |
| Frame arrival → matching `fillText` p50 / p95 | not instrumented | 10 / 21 ms |
| rAF gap p50 / p95 / max | 17 / 18 / 18 ms | 17 / 18 / 47 ms |
| 100 ms timer gap p50 / p95 / max | 104 / 104 / 109 ms | 103.5 / 105 / 109 ms |

The matched-glyph distributions are censored: they cover 11/15, not all 15 keys. They must not be reported as a complete latency pass.

### Layer localization

The layered visible run has 15 outgoing character writes, all successful HTTP-200 native IPC responses, and all 15 characters in backend output. Frontend event dispatch returned within 1 ms. This rules out lost dispatch, a missing IPC write, and a missing PTY echo in this run.

For key `①`, dispatch → IPC response is 2 ms, the socket observer already sees its echo by 22.399 ms, the first frame containing it arrives at 52 ms, and `fillText` completes at 73 ms. Thus at least 29.601 ms of this sample lies after backend output was observable and before the frontend frame callback; another 21 ms lies between arrival and glyph submission. It is not a 73 ms keyboard-handler or IPC write stall.

Keys `⑤⑦⑨⑪` occur in backend output but in **none of the 74 retained frame-arrival payloads**, and in none of the 103 retained paint snapshots. Recorder drops are zero. These four absent glyph calls are upstream of frontend decode/paint, not a renderer dropping cells it received. `analyze.py` decodes serialized row/cell fields, rather than searching arbitrary binary substrings.

This does **not** prove data loss: the output generator writes 12 long rows per 100 ms into a small terminal. Its 220-column source rows wrap many times in this compact viewport, so a complete burst can scroll a transient kernel echo out of view before a grid snapshot. All four characters remain in backend output. A stable application composer is a different contract from this raw kernel-echo workload.

Source inspection identifies deliberate frame-rate floors in `src-tauri/src/pty.rs::grid_send_min_interval_ms`: 33 ms for sustained dirtiness and 50 ms for typing under CPU saturation, in addition to ticker/ack/scheduling work. The captured data does not record which floor was active, so it does not attribute the observed delay to a specific guard. It localizes the remaining latency to grid delivery plus frontend scheduling, not the write path. No production correction or RED was made on an unproved root-cause hypothesis.

### `/help` against plain PTY

Installed ego reports `0.1.2`. `help_probe.py` runs the same nested 40x120 PTY on both paths with separate temporary EGO_HOME directories under `~/Gits/.tmp`, `--bare --offline`, and the same model selection. Plain mode answers CPR immediately; TUIC mode forwards bytes and answers through the real outer TUIC terminal. Timing ends when the final Ctrl+C help row appears in nested PTY output, using the historical probe's marker. It does not include compositor presentation or certify that all help rows appeared in a single painted block.

| Path | Submit → final help marker |
| --- | ---: |
| Plain PTY | 117.901 ms |
| Visible TUIC | 209.921 ms |
| TUIC / plain | 1.7805x — within 2x |

`help-window.json` confirms the test window was visible when the help tab was selected. Help submission itself is scripted nested-PTY input; the typing measurement above retains the requested JS-keydown path.

## Invalid observations and harness failures

- The first layered attempt found the test window **hidden** between runs. It produced no rAF/glyph calls. Its `layered-*.json` files are retained as invalid-for-visible-timing evidence, not mixed into the visible summary. The cause of the intervening hide was not established. The isolated instance was stopped, then restarted for an immediately executed visible layered run.
- Initial plain-PTY observer attempts hung during `waitpid` after killing the child while the master PTY remained open. The retained final helper closes the master before waiting; the subsequent baseline completed. This was an observer cleanup failure, not a measured `/help` slowdown.
- The final lifecycle observer returned exit 1 only when it tried to DELETE the help session after the probe had already exited naturally (404). All measurements and final cleanup had completed. `cleanup.json` records zero sessions, successful exact-instance stop and no remaining worktree app process. The retained helper now checks whether that session is still active before DELETE. No timing rerun was made for this cleanup-only correction.
- The first `exec` launch used an incorrect 1-second background timeout and was terminated during setup. The successful launch used no execution timeout; startup was observed through the socket. Neither aborted setup is counted as a measurement.

## Validation, cleanup and limits

Commands executed:

1. `python3 ~/Gits/.agent-tools/check-instruction-links.py` — passes, two pairs.
2. `make agent-test AGENT_TEST_INSTANCE=at-2040c AGENT_TEST_PORT=19891 AGENT_TEST_DRY_RUN=1` — retained `launch.json`.
3. Native `make agent-test` — Vite and native debug binary build through the target's build-slot/mbx integration; mbx doctor reported zero failures. No installed executable was replaced. This is native execution evidence, not rb cross-target evidence.
4. `python3 .../run_measure.py`, `python3 .../run_layered.py`, `python3 .../help_probe.py plain`, and lifecycle `run_visible.py` — observations above, including the stated observer failures.
5. `python3 .../analyze.py` — derives `summary.json` from retained observations and serialized cell data.
6. `make agent-test-stop AGENT_TEST_INSTANCE=at-2040c AGENT_TEST_PORT=19891` — run before this handoff. Cleanup records no sessions or worktree app process. No Vite server/browser was launched separately.
7. `story-close.sh --check <main checkout> 2040-16b2 <this worktree>` — refuses original criterion 3: `/help ... within 2x ... and keystroke echo stays under one frame`. The combined criterion remains unchecked: the first half passes this output-time comparison; the under-frame condition does not.

No production code changed. No Rust/JS unit suite, RED/GREEN cycle, mutation, release build, native trusted keyboard timing, or compositor capture is claimed. The original native GREEN/REALITY criteria remain open. The native-input decision report is not changed.

## Sentinel boundary follow-up

The continuation trace resolves the reproduced omission as viewport scrolling before serialization: ⑤, ⑨ and ⑪ in the inherited trace, and ⑦ in the single authorized repeat, are already above the viewport before their first eligible grid snapshot. Still-visible control glyphs are serialized and received. No serializer/delivery defect or RED was established. Both trace runs were hidden and do not add visible timing evidence or satisfy the original native GREEN/REALITY criteria. See [`trace/README.md`](trace/README.md) for the coordinates, event sequences, retained raw logs, observer patches, limitations and successful exact-instance cleanup. The temporary Rust observer is not part of the final source tree.

## Evidence index

- `summary.json`, `analyze.py`: computed distributions and per-key stage times.
- `visible-loaded.json`, `backend.json`, `output.json`: initial valid visible run.
- `visible-layered-loaded.json`, `visible-layered-backend.json`, `visible-layered-output.json`, `visible-layered-frame-workload.json`: valid layered run, complete retained frame window, zero recorder drops.
- `layered-loaded.json`, `layered-backend.json`, `layered-output.json`, `layered-frame-workload.json`: hidden attempt, explicitly invalid for visible comparison.
- `plain-help.json`, `tuic-help.json`, corresponding `*-help-events.json`, `help-window.json`: help comparison and window precondition.
- `launch.json`, `health.json`, `repeat-launch.txt`, `observer-diagnostics.txt`, `cleanup.json`, `story-close.txt`: owned native lifecycle.
- `measure.js`, `layered.js`, `socket_tools.py`, `run_measure.py`, `run_layered.py`, `run_visible.py`, `help_probe.py`: observer sources. They are evidence helpers, not unattended application tests.
