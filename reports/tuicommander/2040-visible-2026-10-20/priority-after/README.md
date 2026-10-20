# Bounded input-priority result — 2026-10-20

## Disposition

Production change: `6eb6bbefad894315fc1c8f5a75f44b7835468485`, on
`chore/2040-visible-recheck`. The earlier baseline and regression-only commit is
`cd4552b857`; pre-fix production is `1c2d0bf8f`, whose `pty.rs` matched main.

The same real reader/ticker regression that failed before the fix passes on
native macOS and rb. The matched-glyph native proxy p95 falls from **74ms to
22ms**, with **28ms** in the repeat. These are censored distributions, not a
15/15 keyboard-to-presentation pass. Original native GREEN/REALITY stay open.
The combined new GREEN criterion also stays open: native heartbeat behavior and
all-tail liveness non-regression are not established by these short runs.

## Implementation and invariants

- Interactive desktop and HTTP/PWA writes arm priority before the PTY write.
  Post-write bookkeeping updates the input timestamp without replacing the
  generation of an echo that the reader already applied.
- The reader snapshots the input generation after `read()` and wakes only after
  applying that chunk. An already-processing pre-input chunk cannot consume the
  new generation. Generic PTY output does not identify an application echo;
  two credits allow later output after an unrelated first post-input chunk.
- Only the existing ticker serializes. Its 16ms maintenance tick remains for
  watcher tails, DEC-2026 timeout and gate recovery. Priority does not create a
  second serializer or send a cursor-row snapshot from input.
- Priority expires 100ms after input. At most two early/floor-bypass cuts are
  allowed in any rolling 100ms, without replenishment from repeated input.
  Priority coalesces against an 8ms interval from the previous ticker cut.
- Subscriber checks, the desktop receipt gate, synchronized-output buffering,
  ordered sends and shared-damage repair stay on the existing path. Credit is
  consumed only after serialization, not by a failed gate/subscriber check.
- The lifecycle removes the hint beside the dirty/sync maps, including reader
  panic. The bounded mutex holds only scheduler metadata, never the VT lock.

## Targeted RED/GREEN

`input_echo_reaches_grid_before_animation_floor` submits a controlled `⑤` chunk
through the real reader immediately after ten actual ticker publications. It
asserts delivery in less than 24ms, with a separate 10s setup/liveness budget.
The source is controlled test data, not a fabricated agent capture.

| Revision and host | Result |
| --- | --- |
| Pre-fix `1c2d0bf8f`, native | FAIL, 59.522917ms versus 24ms; exit 100 |
| Pre-fix `1c2d0bf8f`, rb | FAIL, 48.348081ms versus 24ms; exit 100 |
| Fixed `6eb6bbefa`, native | 20/20 selected tests PASS, 1.299s; exit 0 |
| Fixed `6eb6bbefa`, rb | 20/20 selected tests PASS, 1.299s; exit 0; `RB_BOX_DIRTY=no` |

The filter covers the new expiry, generation, nonrenewable quota and damage
notification tests; real reader echo; pre-write desktop/HTTP arming; gate and
DEC-2026 ESU preservation; existing ordering, paused/hidden desktop, independent
browser, full-repair, panic teardown, multipart-write and bare-Enter behavior.
It excludes the unrelated slow-discovery test. No broad suite, clippy, coverage
or mutation run was performed.

Command, through `build-slot.sh` and `scripts/with-test-tmp.sh` locally:

```sh
cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E \
  '(test(grid_delivery_tests) - test(slow_session_discovery)) | test(frame_priority) | test(grid_send_min_interval_policy) | test(a_reader_panic_stops) | test(n_input_parts_share_one_writer) | test(bare_enter_uses_the_same_submission)'
```

The rb run used `RB_FORCE=1 build-slot.sh --remote` after the native prerequisite.
Every run checked BUILD_FREEZE and used the repository-local TMPDIR. Native mbx
reported zero doctor failures/warnings. Native compilation took 1m46s; rb took
9m21s. Native linking warned that `__eh_frame` exceeded the compact-unwind table
limit; tests still completed. Full log:
`~/Gits/.tmp/results/desk-2040e-priority-green.log`. rb receipt:
`~/Gits/.tmp/rb/20261020T113458Z-58642`.

## Same-workload native A/B

All runs use the unchanged `output.py`, `layered.js`, 15 JS keydowns at requested
150ms spacing, one PTY, 15x38 native grid and 604x560 backing canvas at DPR 2.
The isolated instance is `at-2040c`/19891, accessed only over its Unix socket.
All documents/native windows are visible and unfocused. A second after run was
made because the first had one absent PTY echo and lower observed throughput;
both outcomes remain retained, not selected away. The repeat also saves the
pre-transform raw ring after measurement, before session deletion.

Nearest-rank durations in milliseconds:

| Observation | Before | After | After repeat |
| --- | ---: | ---: | ---: |
| Successful native input writes | 15 | 15 | 15 |
| PTY/VT echoes | 15 | 14 | 15 |
| Matching frame arrivals | 14 | 13 | 14 |
| Matching `fillText` calls | 14 | 13 | 13 |
| Dispatch → `fillText` p50 / p95 | 39 / 74 | 15 / 22 | 16 / 28 |
| Write begin → VT applied p95 | 1.622 | 1.958 | 1.952 |
| VT applied → matching cut p95 | 50.355 | 9.103 | 5.738 |
| Cut → native submit p95 | 1.597 | 0.976 | 1.663 |
| Dispatch → matching arrival p95 | 54 | 13 | 22 |
| Arrival → matching `fillText` p95 | 20 | 14 | 14 |
| Observed output bytes/s | 19516.991 | 18639.243 | 19580.906 |
| rAF gap p50 / p95 / max | 17 / 18 / 43 | 17 / 18 / 49 | 17 / 18 / 88 |
| 100ms timer gap p50 / p95 / max | 104 / 105 / 108 | 104 / 106 / 115 | 104 / 105 / 109 |
| Recorder drops | 0 | 0 | 0 |
| Priority bypass cuts | n/a | 28 | 28 |
| Maximum bypass cuts in rolling 100ms | n/a | 2 | 2 |

The before trace has floor deferrals for 10/14 matched glyphs and no gate waits
in those glyph intervals. Only one matched glyph in the first after run has a
floor deferral (0.839ms from that decision to cut); none in the repeat does.
The trace confirms the intended layer moved: VT-to-cut p95, not PTY processing
or a second send path. The 28 priority cuts include output without a sentinel,
not 28 input echoes. `tick` now counts serializer attempts, including early
wakes, rather than only fixed maintenance ticks.

### Censoring and unresolved observations

- Before: `①` scrolls out before a snapshot. After: `⑤` is at absolute row 747
  when applied, above the next viewport origin 805. Repeat: `⑥` is at row 891,
  above the next origin 900. They are above the viewport by the first eligible
  cut, not on-screen serialized cells that disappeared in transport.
- First after run: `⑧` has a successful native write but is absent from all
  retained raw-read compact key scans, VT applications and final output. The
  compact observer does not retain its original raw bytes. Its cause is unknown;
  this is not relabelled as viewport scrolling or a successful echo. The repeat
  has all 15 keys exactly once in its 116463-byte valid-UTF-8 raw ring. That
  repeat does not explain the earlier absent echo.
- Repeat: `①` arrives at frontend time 4392ms; a newer frame arrives at 4403ms
  before the next paint at 4404ms, whose viewport top is 446. It has no matching
  `fillText` call. The timing distribution remains 13/15, not 15/15.
- First after throughput is 4.50% below baseline; the repeat is 0.33% above it.
  These finite paced-generator observations do not prove statistical throughput
  equivalence or a throughput regression. Timer/rAF p95 are stable in the repeat,
  but its maximum rAF gap rises from 43 to 88ms during the measurement. Do not
  call this all-tail liveness non-regression.
- Native `frontend_heartbeat` receipts were not instrumented in the baseline.
  Timer/rAF callbacks are main-thread liveness proxies, not that command's
  receipts. No native heartbeat non-regression claim is made.
- Input is untrusted JS. `fillText` completion is canvas submission, not trusted
  native keyboard timing or compositor presentation. Native monotonic durations
  and frontend monotonic durations are primary. Their epoch bridge has sampled
  clock/resolution uncertainty; small negative cross-domain estimates are not
  negative transport latency.

## Reproduction and cleanup

`priority-before/analyze.py [RUN_DIRECTORY]` is the shared compact analyzer;
without an argument it still reproduces the original baseline. The after
`observer.patch` applies to `6eb6bbefa`. It retains the same observer sites and
format as before, with additional priority-bypass/periodic metadata at cuts and
a distinct `priority-spacing` deferral. `../compare_priority.py` derives
`../priority-comparison.json` without discarding absent keys. Raw events,
frontend binary payloads, observation logs and compressed native logs are
retained in this directory and `../priority-after-repeat/`.

Both acquisition helpers exit 0 after visibility checks and cleanup. Each
cleanup records zero sessions, exact-instance stop exit 0 and zero worktree
app processes. Make's supervisor exits 2 because its owned app is terminated
with signal 143 by the stop target; this also occurs in the baseline. A final
explicit stop reports no running supervisor (make exit 2), not a cleanup leak.
All temporary Rust observer edits were restored to the committed production
source. No daily app restart, installed-binary replacement, credential access,
production interaction or separate browser/Vite server was used.

The Rust fix requires a manual rebuild/restart to affect Boss's running app.
`to-test.md` keeps that check. A separate coordinator critic still owns review;
no story closure, landing or push is claimed.
