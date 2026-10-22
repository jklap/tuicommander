# Browser terminal input measurement — 2026-10-22

Story: `2040-16b2`. Measured source: `ec5327735b5994c02b1dc84c1913ffbf6df9c5ff`.
This is partial evidence, not story completion. No production code was changed.

## Scope and method

Boss authorized browser-mode evidence because the desktop WebView and browser
use the same terminal frontend: "The native UI is the same as the web browser,
it is Tauri." This run does not claim transport or engine equivalence.

The isolated `make agent-test AGENT_TEST_INSTANCE=agent-test-2040
AGENT_TEST_PORT=19884` served the embedded frontend. Backend setup and inspection
used only its instance Unix socket. Browser authentication used the supported
`/api/auth/session-token` endpoint on that socket; no credential store was read.
The worktree native build went through build-slot and the mbx cargo shim.

The named stealth-wrapper tab used Chromium with reported user agent
`Chrome/149.0.0.0`, a 1400×900 CSS viewport, DPR 1, and a visible document.
Actual engine build metadata was not independently captured. CDP
`Input.dispatchKeyEvent` drove the wrapper-owned tab, without JS event dispatch.
All measured keydown events reported `isTrusted=true` on the real terminal input.

A real Python PTY printed 12 source-like rows per 100 ms. Its raw-input reader
wrote each circled key in a persistent first-row echo, outside the scrolling
region. Each key also selected a distinct gray background for that echo. This
controlled workload prevents a key from scrolling away before observation; it is
not an ego transcript or an ego composer.

The observer recorded trusted event timestamps, matching production canvas
`fillText`, rAF gaps, a 100 ms event-loop sentinel, and CDP compositor PNG frames.
Offline analysis finds the first PNG with at least 20 pixels of the key's unique
gray echo background in its observed canvas cell crop. Neutral gray was used
because the earlier colored smoke exposed compositor color conversion. The
retained matching PNGs show the echoed glyph and its background.

The compositor number is event timestamp to the first matching screencast frame's
CDP timestamp, aligned through `performance.timeOrigin`. It is sampled browser
compositor observation, not physical display scanout or a native WebKit claim.
Capture overhead and frame sampling are included. A missed bound in this harness
does not, by itself, identify the production layer responsible for the delay.

## Results

Percentiles use nearest rank. The main run increased the input sample count after
the five-key smoke validated trusted input, the persistent echo and PNG matching.

| Observation | Smoke | Main |
| --- | ---: | ---: |
| Trusted inputs / matching fills / compositor matches | 5 / 5 / 5 | 20 / 20 / 20 |
| Event → matching fill p50 / p95 / max | 26.900 / 33.000 / 33.000 ms | 25.000 / 33.800 / 36.300 ms |
| Event → matching compositor PNG p50 / p95 / max | 40.186 / 62.131 / 62.131 ms | 29.557 / 42.789 / 44.755 ms |
| rAF gap p95 / max | 16.800 / 25.000 ms | 16.800 / 16.800 ms |
| 100 ms sentinel gap p95 / max | 101.000 / 101.000 ms | 101.300 / 101.400 ms |
| Compositor PNGs retained | 99 | 141 |

Main output grew from 5,696 to 72,738 bytes over 3,583.305 ms:
**18,709.541 bytes/s**. This is a current workload observation, not an A/B
throughput non-regression result. The sentinel is a browser event-loop heartbeat,
not the desktop backend's frontend-heartbeat receipt.

The run does not pass the original under-one-frame requirement or the later
below-33-ms target: main fill p95 is 33.8 ms and sampled compositor p95 is
42.789 ms. No speculative scheduling or rendering change was made from these
aggregate timings. There is no same-workload pre-fix A/B in this assignment.

## Incomplete criteria and prerequisites

- **Help versus plain PTY:** unavailable. The installed executable reported
  `ego 0.1.2`. The isolated plain nested-PTY probe printed its startup banner,
  but produced no prompt or `/help` response in the bounded observation. A
  second attempt submitted `/help` after the banner and still obtained no help
  bytes. This does not prove an ego defect; the probe or startup state needs
  diagnosis. No browser/plain help ratio or one-block-help claim is made.
- **WebKit:** `safaridriver` was installed and its status endpoint was ready.
  Session creation returned: `Could not create a session: You must enable
  'Allow remote automation' in the Developer section of Safari Settings to
  control Safari via WebDriver.` No setting was changed. An earlier driver
  attempt hit a port collision; the retained decisive retry used an ephemeral
  available port and confirmed driver readiness before requesting a session.
- **Initial Chromium attempt:** the console was locked and a compositor screenshot
  timed out despite `document.visibilityState=visible`. That attempt supplied no
  timing evidence. The later unlocked runs above supplied actual PNG frames.
- **Story mutation:** the main-store story was set `in_progress` through Wiz at
  assignment start. Later direct rewording was denied: `edit may not write ...:
  it is inside a read-only root`. No alternate write route was used after that
  denial. The coordinator must apply any browser-scope wording and worklog
  update in the main store. No criterion was dropped, rejected or checked here.

## Retained evidence and cleanup

Evidence root: `~/Gits/.tmp/2040-web/`.

- `main/{probe,frames,summary,throughput,output,setup}.json` and `match-*.png`:
  main raw timings, actual compositor frames and per-key matches.
- `smoke/`: successful five-key neutral-gray smoke.
- `smoke-initial/`, `smoke-color-profile/`, `diagnose.json`: rejected observer
  calibration attempts; not mixed into the main distributions.
- `browser.mjs`, `measure.mjs`, `rows.py`: retained acquisition harness.
- `safari-preflight.json`, `safaridriver.log`: actual WebKit prerequisite refusal.
- `plain-help-*.json`, `plain-help-live.bin`, `help_probe.py`: incomplete help
  probe; no successful help timing.
- `build.log`: native exploratory build; mbx doctor reported zero failures.
- `cleanup.json`: zero sessions before shutdown, successful owned tab and named
  daemon closure, and exact instance stop exit 0.

All measurement sessions were deleted. The instance supervisor exited after its
requested stop (the make wrapper reports the expected child termination). Shared
Chrome and the daily TUIC instance were not stopped. No test suite, rb test,
mutation run, production restart or installed-binary replacement was performed.


## Follow-up: layered Chrome trace

The coordinator narrowed the target to Chrome browser mode, authorized the same
controlled workload, and replaced the ego-help comparison with a plain-shell
`seq` comparison. Safari automation is not a prerequisite for this follow-up.
The production source was unchanged from the source identified above.

### Observation contract and trace boundaries

The consumer observes the glyph produced in response to a trusted terminal key,
not an input acknowledgement. Real state comes from the browser event, HTTP PTY
writer, real PTY reader/writer, terminal grid, WebSocket arrival, canvas operation,
and compositor PNG. The controlled Python echo is the external fixture. It does
not represent an ego composer. The radius of the observation is the isolated
`agent-test-2040` instance on port 19884 and its owned browser tab.

Temporary, environment-gated Rust observers recorded epoch microseconds at HTTP
write begin/end, PTY read, VT application, frame serialization and frame submit.
The Python fixture recorded epoch nanoseconds at input read and echo write.
Browser observers recorded fetch, binary grid-message arrival and the existing
fill/PNG boundaries. Browser clocks were aligned through `performance.timeOrigin`.
The native observers were removed byte-for-byte after acquisition; their patch is
retained as evidence, not shipped code. Logging and PNG capture can perturb the
instrumented run.

All 20 trusted inputs have a writer, PTY echo, VT, frame, browser arrival, fill and
PNG observation. All native trace records identify the one measurement session
`89008dcf-1aae-463d-b747-d2ec21e05b78`. Frame correlation scans for the unique echoed
codepoint, rather than decoding and asserting row geometry. Thus arrival is the
first key-bearing wire message, not a separately verified accepted-row timestamp.
Submillisecond cross-clock values are not precise enough for causal attribution:
one submit-to-arrival sample was -0.012 ms.

### Layer timings

The following table uses the statistical median and nearest-rank p95. Percentiles
of different stages must not be added together.

| Boundary | Median | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Trusted event → browser fetch | 0.200 ms | 0.400 ms | 10.200 ms |
| Fetch → HTTP writer begin | 1.064 ms | 2.223 ms | 2.251 ms |
| HTTP writer begin → done | 0.044 ms | 0.094 ms | 0.114 ms |
| Writer done → Python input read | 0.021 ms | 0.049 ms | 0.122 ms |
| Python input read → echo write done | 0.026 ms | 0.048 ms | 0.096 ms |
| Echo done → PTY reader | 0.005 ms | 0.024 ms | 0.046 ms |
| PTY reader → VT applied | 0.076 ms | 0.169 ms | 0.229 ms |
| VT applied → frame cut | 0.047 ms | 4.456 ms | 8.074 ms |
| Frame cut → submit | 0.021 ms | 0.065 ms | 0.067 ms |
| Frame submit → browser message | 4.839 ms | 14.753 ms | 15.382 ms |
| Browser message → matching fill | 16.250 ms | 17.100 ms | 23.400 ms |
| Matching fill → sampled compositor | 10.853 ms | 19.082 ms | 27.563 ms |
| Trusted event → matching fill | 26.000 ms | 34.500 ms | 36.200 ms |
| Trusted event → sampled compositor | 38.028 ms | 55.063 ms | 55.260 ms |

Cumulative event → frame-submit median/p95/max was 1.725/9.712/11.733 ms;
event → browser-message was 9.900/16.200/18.200 ms. The frontend fill/compositor
boundaries, not the PTY input write or VT parsing, therefore account for most of
the measured end-to-end delay. Submit-to-arrival also contributes up to almost
one frame; that interval includes backend WebSocket scheduling, network delivery
and browser message dispatch, not just network transit.

The browser's rAF gap p95/max was 16.7/16.8 ms. The 100 ms sentinel p95/max was
108.6/113.6 ms. Output grew by 65,158 bytes over 3,545.654 ms, or 18,376.863 bytes/s.
These are instrumented current-run observations, not throughput or heartbeat
non-regression evidence.

### Attribution and implementation decision

The measured dominant boundary is **browser key-bearing message arrival to
canvas fill**, followed by compositor observation. This is not evidence of an
input-writer stall. Source inspection confirms the browser WebSocket handler
calls the terminal handler synchronously (`canvasTerminalTransport.ts`,
`deliveredFrame`), while `CanvasTerminal.tsx`, `scheduleRepaint`, coalesces work into
one `requestAnimationFrame`. The backend grid WebSocket waits on the frame watch
channel and sends the latest frame, with a full-frame repair for sequence gaps
(`mcp_http/session.rs`, grid WebSocket send task).

No production defect is isolated by this trace. In particular, the earlier
suspicion that the first key frame was dropped is not established: for the first
key the matching fill precedes the next key-bearing wire frame. rAF timestamps
are frame timestamps, not callback-entry timestamps, so their difference from a
fill cannot be called paint execution time. The trace does not split accepted
frame handling, rAF queueing, callback entry, renderer execution and compositor
scheduling finely enough to prove an unnecessary extra turn.

**No fix was made and no regression RED was fabricated.** Painting synchronously
on every arrival or bypassing the serializer would trade away existing batching,
damage and ordering contracts without evidence of a specific defect. The current
numbers still fail the under-one-frame requirement and the below-33-ms fill p95
target. This report narrows the investigation boundary; it does not claim that
all latency inside that boundary is avoidable or that the story can close.

### Plain-shell A/B

The successful probe uses the same `/bin/sh -c` workload on a real 40×120 PTY:
print READY, wait for `go`, run `seq 1 1000`, print END, then wait to keep the final
screen alive. The plain arm uses `subprocess.Popen` with PTY descriptors; the TUIC
arm uses the isolated backend session and browser. Startup is outside the timed
interval. Both output streams include the expected final marker. The browser
observer sees the production canvas draw that marker.

| Pair | Plain input → final PTY bytes | TUIC HTTP submission → polled output | TUIC submission → final canvas fill | Fill / plain |
| --- | ---: | ---: | ---: | ---: |
| 1 | 9.785 ms | 19 ms | 23.300 ms | 2.381× |
| 2 | 8.607 ms | 35 ms | 41.200 ms | 4.787× |
| 3 | 8.458 ms | 16 ms | 13.700 ms | 1.620× |

The median fill/plain ratio is 2.381×, not a pass of a 2× target. There are only
three pairs. The TUIC output endpoint is polled, so its result is an upper bound
with polling/API overhead; a fill can precede that observation. Plain PTY-read
completion and canvas-fill completion are deliberately different endpoints.
These numbers are a same-workload processing/display comparison, not equivalent
physical presentation clocks, an ego `/help` comparison, or a compositor A/B.

The original manual nested-fork probe, including a `pty.fork` variant, hung in
child cleanup (`waitpid`, recorded by faulthandler). No ego or shell defect is
inferred. The replacement avoids Python fork-child setup and successfully captures
both arms. A first replacement attempt exited the shell immediately after END:
backend bytes arrived but the canvas observer saw only `go`. Keeping the shell
alive after END resolved that harness lifetime error. A stale wrapper tab also
required a new, uniquely named browser session. These rejected attempts are not
mixed into the three successful pairs.

### Evidence, checks and remaining scope

Additional evidence under `~/Gits/.tmp/2040-web/layer/`:

- `run/{probe,frames,summary,throughput}.json`, `run/match-*.png`: trusted trace and
  matching compositor frames.
- `native.log`, `echo.jsonl`, `observer.patch`: native and Python observation.
- `analyze.py`, `timeline.json`, `stages.json`, `trace-events.json`: all per-key
  correlations and aggregate stage timings.
- `seq-pairs.json`, `seq-output-*.json`, `seq-last.json`: successful shell A/B.
- `seq-burst.mjs`, `run-seq.py`, `seq-lifecycle.json`: acquisition and cleanup.
- `burst-stack.log` and rejected probe logs: diagnosed harness failures.

The final lifecycle reports zero remaining measurement sessions, owned tab and
named daemon closure, and exact instance stop exit 0. The make supervisor exit 2
wraps its requested child termination. All temporary production observers were
restored, and no installed application binary was replaced. No application suite,
GREEN run, mutation, cross-target test or Safari verification was claimed.

The main-story read-only-root denial remains binding. No alternate tool was used
to modify its criteria. Original help, one-frame and combined non-regression
criteria remain unsupported; the parent must reconcile scope and worklog wording,
not mark those requirements passed from these measurements.
