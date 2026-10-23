# Four-send input-priority quota trial — 2026-10-23

## Status and contract

**INCOMPLETE / NOT LANDABLE.** A1, B1 and A2 completed. B2 built, then the
post-build console-lock guard refused acquisition: `screen-locked after build;
no timings`. A fresh `ioreg` check also reports the console locked. No lock,
visibility or permission workaround was used. B2 supplies no latency, CPU or
bulk samples. The next prerequisite is an unlocked console, not another policy
change. Do not treat three completed runs as the requested A–B–A–B certificate.

Candidate `94377aaf26e3d5f20a4a149d6761852fa8cf62d1` on
`fix/2040-quota-trial` changes rolling bypass capacity from two to four. Its
fixed array rotates before recording the latest send. The 8 ms spacing,
100 ms expiry, animation/load floors, serializer, receipt gate and synchronized
output rules are unchanged. Existing quota and retained-retry fixtures now
exhaust four credits. SPEC and backend PTY documentation state four.

The allowance rises from 20 to 40 bypasses/s in the sustained limit. Four cuts
can occupy 0/8/16/24 ms; the fifth cannot bypass before 100 ms. This is a
coordinator-authorized reversible policy experiment, not a proven defect fix.
No other value or policy was tried. No per-story unit GREEN suite was run;
the separate coordinator critic remains required.

The strict decision rule is B idle p95 below **every** A idle p95, with no
bulk/heartbeat statistic worse beyond A's run-to-run spread. One B observation
is encouraging but cannot satisfy this rule for the requested repeated series.
Story criteria 3/4/24 are unchanged and unchecked.

## Controlled acquisition

Evidence and reproducible helpers:
`~/Gits/.tmp/2040-web/native-quota-four-abab/`.
Read `contract.json`, `series-state.json`, `comparison.json`, `series.py`,
`run-one.py` and `analyze-partial.py`. The previous recorded harness is reused.
A = `f59ec280e`; B = `94377aaf2`. Only the policy file differs in runtime
production source; `pty.rs` and the frontend hashes match across variants.
Common temporary observers record reader completion, native cuts, IPC,
CanvasTerminal receives/acceptance and actual `fillText`. Quota observers use
the oldest real credit and the input generation, not assumed credit state.

Every completed smoke/main receive reports **37 columns × 14 rows**,
640×420 CSS pixels, DPR 2. Each lifecycle uses the same named
`agent-test-2040` instance at port **19890**, Unix-socket API, installed ego
with default configuration and no launch arguments. Only `/help` is submitted;
draft glyphs are cleared. Native binary replacement requires sequential app
lifecycles and fresh ego processes, not one conversation surviving restarts.
Builds use `make agent-test`, its build-slot runner and mbx. The daily app and
port 9876 are untouched.

These are synthetic frontend key events to actual native WebKit canvas fill,
**not trusted OS keyboard input or physical presentation**. Each completed run
has a 3-idle/3-help smoke and a 200-idle/200-help main population. No timing run
is discarded. B2 failed before any timing acquisition.

## Latency and quota observations

Times are **median / p95 / max**, milliseconds. Main populations only.

| Run | Idle key→fill (N=200) | Idle reader-complete→cut (N=200) | Help-candidate key→fill (N=200) | Help-candidate reader-complete→cut |
|---|---:|---:|---:|---:|
| A1 | 10 / 24 / 37 | 0.064 / 18.228 / 22.096 | 20 / 29 / 40 | 9.077 / 12.097 / 24.496 (N=199) |
| B1 | 6 / 10 / 19 | 0.053 / 0.085 / 0.105 | 20.5 / 28 / 40 | 8.673 / 12.015 / 23.810 (N=200) |
| A2 | 17 / 44 / 57 | 0.142 / 34.296 / 39.374 | 21 / 28 / 34 | 9.960 / 16.733 / 24.334 (N=200) |
| B2 | unavailable | unavailable | unavailable | unavailable |

All 1,200 completed main samples have reader, receive, acceptance and fill
correlations. A1 has one missing matching shared-cut observer during help;
its cut timestamp is not invented. Verified main help/output overlap counts
are A1/B1/A2 = 195/190/196, so the table labels the full help populations as
candidates, not 200 verified overlaps. Cross-clock stage timing retains the
previous harness's approximately ±2 ms uncertainty.

| Quota blocked, per 200 | A1 idle | B1 idle | A2 idle | Help, all three runs |
|---|---:|---:|---:|---:|
| At reader completion | 60 | 0 | 86 | 0 |
| At actual input arm | 64 | 0 | 89 | 0 |

Thus B1 removes the observed quota wait in its idle population. Machine load
is not controlled: A1 and A2 alone span 24–44 ms idle p95. This result does not
prove a repeated-B or all-load improvement.

## Paced bulk and heartbeat controls

The unchanged generator emits 12 UTF-8 rows every 100 ms. Each lifecycle first
runs 20 batches, then 120; only the count changes. Actual reader text is
reconstructed from byte-count-checked native read records. Its bytes match the
independent generator contract including observed PTY ONLCR conversion.
Payload includes begin/end marker text; the final marker's newline is excluded
consistently. Every completed variant has exactly the same payload hash:

| Batches | Reader bytes | SHA-256 |
|---|---:|---|
| 20 | 30,271 | `10c4501353ef3ff88dce02004d4ac7581047a6a641e6e7e16471cfdd043ca5df` |
| 120 | 181,471 | `73e26194bb288bd4f0ccb5c22bfbdee7ccd95223fcde4b30f366a6c9645c0191` |

Throughput is payload bytes divided by first-begin-read to end-marker-read
elapsed time. Heartbeats cover the frontend probe interval, including the
unchanged three-second generator startup delay, as in the inherited control.

| Run | Batches | Reader B/s | rAF p95 / max, ms | 100 ms timer p95 / max, ms |
|---|---:|---:|---:|---:|
| A1 | 20 | 14,068.50 | 18 / 26 | 105 / 105 |
| B1 | 20 | 13,931.49 | 18 / 40 | 105 / 105 |
| A2 | 20 | 13,896.92 | 18 / 36 | 105 / 106 |
| A1 | 120 | 13,951.80 | 18 / 19 | 105 / 106 |
| B1 | 120 | 13,950.37 | 18 / 20 | 105 / 106 |
| A2 | 120 | 13,977.79 | 18 / 22 | 105 / 106 |

Retain the adverse calibration tail: B1 rAF max is 40 ms versus A 26/36 ms,
not a universal heartbeat non-regression claim. At 120 batches B1 throughput
is 1.43 B/s below the lower A value; the A run-to-run spread is 25.99 B/s.
No B2 control exists, so the requested full decision remains unavailable.
These are paced reader-throughput observations, not maximum-capacity benchmarks.

## App process CPU during idle

`ps -p <own app PID> -o time=` is sampled at approximately 100 ms plus process
launch cost. The bracket begins before the first main idle key and ends after
its last fill. Values are app CPU time, excluding WebContent and child ego.
`ps` reports hundredths of a second; this is not a precise energy measurement.

| Run | Owned app PID | CPU seconds | Observed wall seconds | Bracket padding before / after, ms |
|---|---:|---:|---:|---:|
| A1 | 82992 | 1.17 | 15.019 | 3.792 / 98.385 |
| B1 | 13009 | 1.08 | 13.434 | 20.375 / 1.750 |
| A2 | 46770 | 1.21 | 16.478 | 23.718 / 108.176 |

Do not compare absolute CPU seconds without the different elapsed durations.

## Cleanup, validation and remaining work

All four sanctioned stops return 0; supervisors return 2 after shutdown.
The B2 post-build failure creates no ego or workload session. The series restores
all temporary observers and production files to committed B source; its final
Git status is empty. The instance desktop PID file is absent. No daily-app
restart, push, merge, criterion check, additional quota value, broad test suite,
clippy or mutation run was performed.

Offline analysis succeeds and retains every completed population. Its first
foreground invocation exceeded a 10-second tool budget after writing the stage
analyses; no process remained. The identical analysis then completed through
the background wrapper. This is an analysis retry, not a timing acquisition
retry. Instruction-link validation and `git diff --check` passed before the
candidate commit.

Resume only B2 after the console is unlocked, retaining the failed pre-acquisition
attempt and the existing A1/B1/A2 evidence. Recheck equal source/observer hashes,
wire geometry and cleanup before applying the strict decision. The changed quota
and retained-retry policy tests still need the coordinator's targeted rb GREEN
and separate critic. The experimental Rust policy is not loaded in Boss's daily
app; loading any landed Rust change requires Boss's manual rebuild/restart.
