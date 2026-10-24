# Bounded terminal input priority — 2026-10-20

Story: `2040-16b2`. Implementation: `6eb6bbefad894315fc1c8f5a75f44b7835468485`.
Pre-fix production: `1c2d0bf8f`; regression and baseline: `cd4552b857`.

## Method and result

A visible, unfocused native WebKit test instance (`at-2040c`, port 19891) ran
one controlled `scripts/stall-workload/output.py` PTY. The observer dispatched
15 untrusted JS keys at requested 150ms intervals and correlated native monotonic
write → VT apply → ticker cut → channel submission with matching frontend
binary arrivals and `fillText` calls. Geometry was 15×38 cells with a 604×560
backing canvas at DPR 2. No daily app restart or production interaction occurred.

The baseline proved material frame-floor delays: 10/14 matched echoes had
explicit floor deferrals, with no relevant gate waits. The fix wakes the same
serializer only after reader-applied post-input output. It allows two bypass
cuts per rolling 100ms, expires after 100ms, and coalesces 8ms from the previous
cut. Desktop gates, DEC-2026 buffering, ordering and shared damage stay intact.

| Observation | Before | After | After repeat |
| --- | ---: | ---: | ---: |
| Input writes | 15 | 15 | 15 |
| PTY/VT echoes | 15 | 14 | 15 |
| Matching arrivals / glyph calls | 14 / 14 | 13 / 13 | 14 / 13 |
| Dispatch → matching `fillText` p95 | **74ms** | **22ms** | 28ms |
| VT apply → matching cut p95 | 50.355ms | 9.103ms | 5.738ms |
| Observed output bytes/s | 19516.991 | 18639.243 | 19580.906 |
| rAF gap p95 / maximum | 18 / 43ms | 18 / 49ms | 18 / 88ms |
| 100ms timer gap p95 / maximum | 105 / 108ms | 106 / 115ms | 105 / 109ms |
| Recorder drops | 0 | 0 | 0 |

Both after traces observed at most two priority bypasses in any rolling 100ms.
The ticker-boundary regression failed before the fix: native 59.522917ms and
rb 48.348081ms versus a 24ms bound. Twenty targeted tests then passed on native
macOS and rb, each in 1.299s; rb reported `RB_BOX_DIRTY=no`.

## Commands and retained evidence

Targeted filter:

```sh
FILTER='(test(grid_delivery_tests) - test(slow_session_discovery)) | test(frame_priority) | test(grid_send_min_interval_policy) | test(a_reader_panic_stops) | test(n_input_parts_share_one_writer) | test(bare_enter_uses_the_same_submission)'
```

After checking `~/Gits/.tmp/BUILD_FREEZE`, native validation used the mbx cargo
shim and `TMPDIR="$HOME/Gits/.tmp/tuic-tests/"`:

```sh
export PATH="$HOME/Library/Application Support/mbx/bin:/opt/homebrew/bin:$PATH"
SLOT="$HOME/Gits/personal/orchestrator/tools/build/build-slot.sh"
"$SLOT" scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E "$FILTER"
RB_FORCE=1 "$SLOT" --remote cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E "$FILTER"
```

Native acquisition used `make agent-test AGENT_TEST_INSTANCE=at-2040c
AGENT_TEST_PORT=19891`, through the archived `run_trace.py` lifecycle helper,
and only the instance Unix socket. Each helper deleted its sessions and ran
`make agent-test-stop` with those same variables. Cleanup recorded stop exit 0
and no remaining sessions or owned app processes. The temporary Rust observer
was removed before the production commit. No new tests ran for evidence cleanup.

Raw runs, observer scripts/patches, full binary arrivals, JSON and native logs:

`~/Gits/.tmp/results/desk-2040e/2040-visible-2026-10-20/`

The archive includes `priority-before/`, `priority-after/`,
`priority-after-repeat/` and the earlier `trace*`/visible investigations.
All 156 archived files were SHA-256 checked before repository removal; the
manifest is `~/Gits/.tmp/results/desk-2040e/2040-visible-2026-10-20.sha256.json`.
The archived scripts retain their original checkout-relative path bindings;
they are evidence, not a relocated executable harness.
Targeted test log: `~/Gits/.tmp/results/desk-2040e-priority-green.log`.

## Limits and disposition

These distributions are censored, not a 15/15 latency pass. Transient echoes
can scroll above the viewport before a cut. In the repeat, one received glyph
was superseded by a newer frame before the next paint. The first after run
also has a successful write for `⑧` but no retained echo before VT application;
its cause is unknown. The repeat raw ring contains all 15 keys exactly once.

Untrusted JS and `fillText` completion do not prove trusted keyboard timing or
compositor presentation. Native frontend-heartbeat receipts were not captured
in the baseline. Throughput varies −4.50% / +0.33%, and the repeat's maximum rAF
gap rises to 88ms; statistical throughput equivalence and all-tail liveness
non-regression are not certified. Original native GREEN/REALITY and the combined
heartbeat/non-regression criterion remain unchecked. `story-close --check`
still refuses original criterion 3; it was not weakened to obtain closure.

The coordinator reports critic round 2 PASS on code, with 6054/6054 rb crate
tests and clean clippy. That is separate critic evidence, not a new cleanup run.
The Rust fix requires Boss's manual rebuild/restart to load in the daily app.
