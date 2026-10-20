# Input-priority baseline — 2026-10-20

Story `2040-16b2`, peer `desk-2040e`. Pre-fix HEAD `1c2d0bf8f351190adced10696a88270ec97bda68`. `git diff main HEAD -- src-tauri/src/pty.rs` is empty: the measured ticker is main's implementation. The temporary measurement patch is retained as `observer.patch` and removed from production source after acquisition.

## Proven layer

The requested visible baseline succeeded this time: native visibility true, document visible throughout, focus false. One unchanged `output.py` session, 15 untrusted JS keydowns, same isolated `at-2040c` / `19891` instance and Unix socket. The retained 15 × 38 grid is the compact native viewport. This is a synthetic kernel-echo workload, not a stable application composer.

**The rate floor is a material, measured contributor.** All 15 writes reach native PTY intake and VT application. Ten of the 14 glyphs that survive to a matching snapshot have one or two explicit `rate-floor` decisions between VT application and serialization. None has a gate deferral in that interval. VT application and native transport are much shorter than the observed waits. Thus the assignment's condition to stop because processing/gate/JS dominates is not met; a bounded priority correction is justified.

| Stage, nearest-rank | Count | p50 | p95 |
| --- | ---: | ---: | ---: |
| Input-write begin → VT application | 15 | 0.968 ms | 1.622 ms |
| VT processing (includes observer work) | 15 | 0.295 ms | 0.413 ms |
| VT application → matching serialized cut | 14 | 30.622 ms | 50.355 ms |
| First explicit floor deferral → matching cut | 10 | 17.923 ms | 36.312 ms |
| Cut → native submission | 14 | 1.063 ms | 1.597 ms |
| JS dispatch → matching frame arrival | 14 | 33 ms | 54 ms |
| Arrival → matching fillText | 14 | 9 ms | 20 ms |
| JS dispatch → matching fillText | 14 | 39 ms | 74 ms |

Example ②: write starts at native monotonic 2689097 us, VT applied at 2690719 us, and cut at 2741074 us. Two rate-floor decisions hold the already-applied glyph before that cut. It reaches a matching arrival at dispatch +54 ms, then fillText at +74 ms. Reader/VT and IPC do not explain this 50.355 ms VT-to-cut wait.

The results are censored: ① is echoed and applied but scrolls away before a matching snapshot, leaving 14/15 matching glyph calls. Do not report the matched p95 as a complete 15-key latency pass. The target is p95 <33 ms at 60 Hz for this measurable proxy; it does not certify compositor presentation or rewrite the original story criterion.

## Throughput and frontend scheduling baseline

- Backend echoes: 15/15; recorder drops: zero.
- Observed output growth: 19516.991 bytes/second, computed from first/last socket observations. This includes workload output and echo and is an observation-window metric, not an isolated generator benchmark.
- rAF gap p50/p95/max: 17/18/43 ms (201 callbacks).
- 100 ms timer gap p50/p95/max: 104/105/108 ms (32 samples).
- Whole-run ticker decisions: 180 rate-floor coalesces, 142 eligible cuts, two clean skips, two no-subscriber coalesces; zero gate coalesces.

## Correlation and limits

`native.log.gz` retains compact temporary events. Native stages use one process `Instant` anchor and monotonic elapsed microseconds; frontend events use `performance.now()`. The once-sampled native epoch anchor bridges clock domains for approximate send-to-arrival comparison only. Sub-millisecond negative estimates arise from the frontend's millisecond timestamp resolution and clock-domain uncertainty; they are not negative transport delays. Same-domain stage durations and input order are the primary evidence.

Native submissions retain payload length, FNV-1a32 and sentinel codepoints decoded from cell fields. Frontend arrivals retain the full binary payload. `analyze.py` matches length/hash and key order, and retains the exact stage events and intervening gate/floor decisions in `summary.json`. FNV is a correlation checksum, not a security digest. `native-events.json` retains the whole ordered native timeline. Logging is temporary and can affect scheduling; the A/B must use the same observer.

## Boundary RED

`pty::tests::grid_delivery_tests::input_echo_reaches_grid_before_animation_floor` catches an echo already applied by the real reader waiting behind the sustained/typing floor despite a caught-up subscriber. It warms through ten actual ticker publications, then stamps input and submits a controlled chunk through the reader; it decodes delivered cell codepoints rather than inspecting a pending flag. The behavior bound is one 16 ms tick plus 8 ms scheduling tolerance, below either 33/50 ms floor. Setup uses a separate ten-second liveness bound.

On unchanged production HEAD `1c2d0bf8f` plus this regression-only edit:

- Native targeted nextest: FAIL, echo waited **59.522917 ms**, bound 24 ms; exit 100. Log `~/Gits/.tmp/results/desk-2040e-priority-native-red.log`.
- rb targeted nextest: FAIL, echo waited **48.348081 ms**, bound 24 ms; exit 100, `RB_BOX_DIRTY=no`. Log `~/Gits/.tmp/results/desk-2040e-priority-rb-red-force.log`.
- Initial rb exit 78 required native evidence because the changed test path is macOS-gated by the runner. After native RED, replay used `RB_FORCE=1 build-slot.sh --remote cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(input_echo_reaches_grid_before_animation_floor)'`. An earlier attempt to pass `--force` directly to build-slot exited 2 at usage without running a test.

The controlled test chunk is not an invented external-agent capture. It exercises the reader, VT, real ticker and subscriber delivery boundary directly. It does not assert keyboard-to-WebView paint timing.

## Cleanup

The acquisition helper exits 0, deletes all sessions, stops the exact isolated instance successfully and records no remaining worktree process (`cleanup.json`). No separate Vite/browser, production restart, Boss desktop capture, credentials, TCP probe or installed executable replacement was used. No capture process remains running. The temporary observer is absent from production source before the regression commit.
