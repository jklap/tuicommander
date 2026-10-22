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


## Follow-up: accepted frame → actual rAF callback → paint

A second coordinator assignment requested callback-entry timestamps to distinguish
normal frame scheduling from an unnecessary second rAF. It used the same isolated
instance, 20 trusted keys and controlled output. No production fix was present.

The temporary frontend observer records `performance.now()` after decoded rows
are installed and the frame becomes current (`accepted`), at `scheduleRepaint`,
when a new rAF is queued, at the actual callback entry, and before/after
`paintFrame`. Each event includes the accepted-frame number, queued-rAF number
and circled key decoded from the authoritative first row. A separate every-frame
rAF sentinel records both its browser frame timestamp and actual callback-entry
time. This removes the earlier ambiguity between rAF timestamp and execution.
All temporary source additions were restored byte-for-byte after the run.

### Result: no extra rAF in this workload

**All 20 accepted key frames were painted in the first subsequent rAF opportunity
observed by the sentinel, in that callback itself.** There was exactly one
sentinel callback between each acceptance and the matching production callback,
with no intervening production callback. No key required a second queued rAF.
Nineteen acceptances queued a new callback immediately; one reused an already
pending callback and painted in it. No key was held behind a separate non-urgent
paint queue.

| Boundary | Median | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Browser message → accepted decoded frame | 0.100 ms | 0.200 ms | 0.300 ms |
| Accepted frame → schedule call | 0.000 ms | 0.100 ms | 0.100 ms |
| Accepted frame → actual rAF entry | 15.950 ms | 16.300 ms | 16.400 ms |
| Queued rAF → actual entry | 16.000 ms | 16.400 ms | 16.600 ms |
| rAF frame timestamp → actual entry | 0.100 ms | 0.200 ms | 0.200 ms |
| Actual rAF entry → matching fill completion | 0.200 ms | 0.300 ms | 12.500 ms |
| Whole paint call | 0.300 ms | 0.500 ms | 12.900 ms |
| Trusted event → browser message | 11.150 ms | 19.000 ms | 24.100 ms |
| Trusted event → matching fill | 27.450 ms | 40.700 ms | 47.500 ms |

The key-bearing frames arrived just after the previous sentinel callback:
acceptance minus previous callback-entry median/p95 was 0.700/1.300 ms, with a
9.000 ms maximum. This explains why the next legitimate rAF cost nearly a full
frame in most samples. It is not an application double-rAF. The source's one-rAF
coalescing mechanism behaves as written under this capture. The trace cannot
attribute why browser messages are phase-aligned this way; browser delivery,
backend scheduling and capture effects are not separated by this frontend probe.

The first key had a 12.900 ms paint and 12.500 ms entry-to-matching-fill interval;
the other 19 whole paints were at most 0.500 ms. That first paint is recorded, not
silently excluded. Its cause is not isolated: the observer does not identify a
font-cache, canvas, compositor or OS cause. It is not evidence of an extra queued
paint opportunity.

For a concrete first-key timeline (milliseconds from browser time origin):
wire arrival 5160.200; accepted 5160.400; queued 5160.500; actual callback entry
5176.200 (browser rAF timestamp 5176.000); matching fill completion 5188.700;
paint end 5189.100. The long interval occurs inside this same paint, not in a
second callback. Key 15 demonstrates the pending-callback path: queued at
7309.600, accepted at 7318.500, callback at 7326.200, fill at 7326.400.

There were 20 trusted inputs, 20 fills and 20 matching compositor PNGs. Sampled
compositor p95/max was 52.242/55.231 ms; rAF gap p95/max was 16.7/16.8 ms; the
100 ms sentinel p95/max was 101.7/106.0 ms. Observed output was 18,691.179 bytes/s.
These are another instrumented observation, not an optimization before/after or
non-regression comparison. In particular, fill p95 40.7 ms does not improve on the
earlier 34.5 ms and does not meet the target.

### Decision and evidence

No avoidable extra scheduling turn was found. No production fix or RED test is
justified for a double-rAF or deferred-queue bug that this trace does not show.
Removing rAF batching would be a design trade-off (paint outside the browser's
frame callback), not a supported repair of an extra wait. The earlier three-pair
`seq` ratio remains the measured comparison; it was not rerun as a claimed
post-fix result because there is no fix. Story criteria 3/4/24 still require the
coordinator's scope decision; none was marked met.

Evidence is under `~/Gits/.tmp/2040-web/fine/`: `observer.patch`,
`CanvasTerminal.tsx.original`, `measure-fine.mjs`, `run.py`,
`run/{probe,summary,frames,throughput}.json`, actual `run/match-*.png`,
`analyze.py`, `timeline.json` and `analysis.json`. The 444 frontend trace events
identify only session `21c6d582-39ba-4eb9-85ba-88653c78a740`. The per-key timeline
retains queued/accepted frame IDs, all intermediate callbacks and same-callback
fill assertions. `lifecycle.json` records zero remaining PTY sessions, owned tab
and named daemon closed, exact instance stop exit 0, and expected supervisor
termination. No daily instance, shared Chrome process or installed binary was
stopped or replaced.


## Follow-up: server send, browser ACK and randomized input phases

The coordinator asked whether an rAF-timed client acknowledgement holds the next
echo frame, and requested randomized phases if no such coupling exists. The new
trace includes native frame cut, watch submission, WebSocket send begin and send
completion, browser WebSocket sends, HTTP request paths, decoded acceptance and
actual rAF entry. It runs 20 trusted keys per arm with seeded pseudo-random delays
of 103–265 ms between input pairs, rather than a fixed 150 ms. A second arm uses
the same randomized schedule with compositor screencast disabled, to test whether
continuous PNG capture alone creates the phase alignment. The controlled PTY
output remains unchanged. No production fix was present.

### Source and observed client traffic exclude an ACK gate

- Browser `WsTransport.ackFrame` is a no-op by design
  (`canvasTerminalTransport.ts:466`). It does not send credit from paint or rAF.
- Desktop `TauriTransport.ackFrame` invokes `ack_terminal_frame`, but it is not
  the browser transport used here. The desktop gate cannot stop the ticker when
  browser watch subscribers exist (`pty.rs`, ticker gate/watcher branches), and
  `send_grid_frame` publishes to the watch before consulting the desktop channel
  gate.
- The grid WebSocket sender waits on `frame_rx.changed()` and sends the latest
  frame. It repairs skipped sequence numbers with a full frame. Its receive loop
  ignores text control messages and does not require an ACK (`mcp_http/session.rs`,
  grid stream sender/receiver). These are bounded newest-frame semantics, not a
  client-paint credit protocol.
- The observer recorded **zero browser WebSocket sends in both 20-key arms**.
  HTTP paths during measurement contain input writes, occasional full-frame
  reconciliation requests, foreground observation and unrelated config reads;
  there is no ACK, visibility or resize request per echo. All request paths and
  times are retained in `server-analysis.json`.

### The server sends promptly; randomized phases do not remove alignment

Each table cell is median / p95 / maximum milliseconds, with 20 keys per arm.

| Boundary | Randomized + PNG capture | Randomized, no screencast |
| --- | ---: | ---: |
| Event → frame watch submission | 1.590 / 2.487 / 4.439 | 2.611 / 6.636 / 7.673 |
| Frame cut → watch submission | 0.011 / 0.066 / 0.067 | 0.015 / 0.056 / 0.064 |
| Watch submission → WS send begin | 0.020 / 0.080 / 0.129 | 0.017 / 0.065 / 0.066 |
| WS send begin → send completion | 0.031 / 0.084 / 0.093 | 0.024 / 0.076 / 0.089 |
| WS send completion → browser message | 6.434 / 16.535 / 16.752 | 7.796 / 15.253 / 16.734 |
| Accepted frame → actual rAF entry | 15.850 / 16.400 / 16.400 | 15.650 / 16.300 / 17.100 |
| Event → matching fill | 24.350 / 34.500 / 34.800 | 25.800 / 34.700 / 37.700 |

Send completion is completion of the awaited WebSocket send operation, not a
packet-capture timestamp. Its interval to JS message handling still includes
socket/network delivery, browser ingress and browser task dispatch. Native and
browser timestamps are epoch-aligned; do not read submillisecond cross-clock
values as precise hardware timings.

The actual key event phases varied across the frame period: phase since previous
sentinel callback ranged 1.700–15.700 ms with capture and 1.700–16.500 ms without
capture (19 keys each; the first precedes the sentinel's first callback). Native
send completion phases also span the period: 0.083–16.369 ms and 0.150–16.613 ms,
respectively. The server is not waiting to send just after client rAF.

But browser message phases remain tightly after the previous callback:
median/p95/range 0.650/1.500/0.300–1.600 ms with capture and
1.000/2.100/0.300–2.500 ms without capture. Consequently, the accepted-to-rAF wait
**does not become uniform with an approximately 8 ms median**. Randomizing input
and removing screencast did not eliminate it. All 40 keys still paint in the first
subsequent sentinel-observed rAF opportunity, in that callback.

This rules out the proposed application ACK/credit coupling and also contradicts
the alternative claim that fixed test cadence alone explains the alignment. The
remaining observed phase concentration arises after server send completion and
before JS message delivery, followed by the legitimate next rAF. This trace does
not distinguish socket delivery from Chromium ingress/task scheduling, and does
not prove a repairable TUIC code defect inside that interval. It would be wrong
to force the evidence into either of the proposed explanations or claim the
random-phase control passed a uniformity result that it did not produce.

No speculative ACK bypass, paint bypass or RED was added. There is no post-fix
`seq` comparison because there is no fix. The unresolved transport/browser
interval and unmet timing bounds remain explicit for the coordinator's next
scope decision. The earlier three successful shell pairs remain the available
shell comparison, not a new result.

### Evidence and cleanup

Evidence is under `~/Gits/.tmp/2040-web/phase/`: `native.log`, `observer.patch`,
`measure-fine.mjs`, `run.py`, `analyze-fine.py`, `analyze-server.py` and
`lifecycle.json`. Each of `capture/` and `no-capture/` holds `probe.json`,
`summary.json`, `timeline.json`, `analysis.json`, `server-timeline.json` and
`server-analysis.json`. The capture arm also retains actual compositor PNGs;
the no-capture arm deliberately has no presentation samples. Session IDs are
`104aea93-3631-473c-ac78-78babd4c4554` and
`778ce40a-529e-4fa5-8be0-0ea79fac2abf`, respectively.

All temporary Rust and frontend observers were restored byte-for-byte. The final
lifecycle reports zero measurement sessions, owned tab and named daemon closure,
and exact isolated instance stop exit 0. No shared Chrome or daily app process
was stopped. No source fix, suite, GREEN, mutation or cross-target claim is made.


## Follow-up: TCP_NODELAY and standalone browser controls

The coordinator requested the server socket policy plus a standalone WebSocket
baseline in the same Chrome. The result distinguishes unsolicited push from the
trusted-input/request/echo path; treating those as equivalent would give a false
TUIC attribution.

### TCP_NODELAY is not selected by this HTTP listener

The actual run logs identify the HTTP-only listener. Its source constructs
`axum_server::from_tcp(listener)` at `src-tauri/src/mcp_http/mod.rs:2990`, then
calls `server.serve(svc)` at line 2997, without selecting an acceptor. The lockfile
pins `axum-server` 0.8.0 (`src-tauri/Cargo.lock:560`). Its installed source is under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/axum-server-0.8.0/`:

- `src/server.rs:70`: `Server::from_listener(TcpListener::from_std(listener)?)`.
- `src/server.rs:165–166`: `from_listener` chooses `DefaultAcceptor::new()`.
- `src/accept.rs:42`: that acceptor returns `Ok((stream, service))`, unchanged.
- `src/accept.rs:63`: the separate opt-in `NoDelayAcceptor` calls
  `stream.set_nodelay(true)`; the measured HTTP branch did not select it.

The temporary experiment selected
`server.acceptor(axum_server::accept::NoDelayAcceptor::new())` on that HTTP-only
branch and reran the same 20-key randomized, no-screencast arm. This change and all
observers were restored after acquisition. The TLS branch was not measured and
no TLS-wide claim is made.

| Observation (median / p95 / maximum) | Original HTTP arm | Temporary NODELAY arm |
| --- | ---: | ---: |
| Server WS send completion → browser message | 7.796 / 15.253 / 16.734 ms | 9.817 / 16.098 / 16.262 ms |
| Browser arrival phase after previous rAF | 1.000 / 2.100 / 2.500 ms | 0.900 / 2.100 / 2.200 ms |
| Accepted frame → actual rAF entry | 15.650 / 16.300 / 17.100 ms | 15.600 / 16.200 / 16.300 ms |
| Trusted event → matching fill | 25.800 / 34.700 / 37.700 ms | 29.100 / 34.200 / 34.300 ms |

NODELAY did not remove the phase concentration or meet the fill-p95 target. These
small sequential samples do not establish an overall performance regression or
improvement. They do reject NODELAY as an explanation of the observed phase lock.
No NODELAY production change was retained.

### Standalone controls outside TUIC

A dependency-free Node HTTP/WebSocket server and tiny page live only under
`~/Gits/.tmp/2040-web/baseline/`. The server performs the WebSocket handshake,
writes server frames, explicitly sets `socket.setNoDelay(true)`, and records send
begin/completion. The page records message arrival and actual rAF callback entry.
It loads no TUIC JavaScript. Each arm uses the same shared Chrome engine, its own
wrapper-owned tab, visible document and 1400×900 CSS viewport. No screencast runs
while these samples are collected. There are 20 messages in each arm.

Three controls avoid a misleading text-only or inactive-input comparison:

1. **Unsolicited text:** the minimal server sends JSON at seeded randomized
   intervals, with no user input or application code.
2. **Unsolicited binary under traffic:** ArrayBuffer messages, 1,510-byte key-like
   payloads and 57,239-byte background frames every 100 ms. The page decodes JSON
   and ignores background frames. This controls binary framing and approximate
   grid wire traffic without TUIC rendering.
3. **Trusted input → HTTP → binary echo:** the same tiny binary page adds a plain
   input and a keydown handler that POSTs the key to `/input`; the Node server
   immediately writes it to the WebSocket. CDP supplies 20 trusted keydown events
   at the same randomized intervals. There is no terminal, Rust, Solid store,
   grid decoder, frame gate or TUIC code. Background traffic is unchanged.

| Standalone control | Arrival phase median / p95 / range | Message → next rAF median / p95 / range |
| --- | ---: | ---: |
| Unsolicited text | 8.050 / 15.700 / 1.500–15.800 ms | 8.650 / 14.600 / 0.800–15.200 ms |
| Unsolicited binary + background | 9.850 / 16.100 / 0.600–16.300 ms | 6.850 / 15.400 / 0.300–16.200 ms |
| Trusted-key HTTP/binary echo | 1.100 / 2.100 / 0.200–2.100 ms | 15.150 / 16.300 / 5.300–16.400 ms |

The first two unsolicited controls are spread across the frame period. The third
**reproduces the tight post-rAF arrival concentration without TUIC**. All 20
input events in that control report `isTrusted=true`. Its server-send → browser
arrival median/p95/max is 1.590/14.748/15.857 ms, versus 0.196/0.246/0.279 ms for
unsolicited text and 0.418/0.767/0.823 ms for unsolicited binary. The input/echo
control therefore reproduces both delayed delivery and the nearly-full-period
next-rAF wait outside the application.

This is decisive against attributing the alignment to TUIC merely because the
unsolicited baseline is uniform. A key-driven round trip is not an unsolicited
push. The observed residual is reproducible in the standalone Chrome input/HTTP/
WebSocket path. The controls do not isolate Chromium's precise internal scheduling
policy or separate the trusted-input effect from the input-triggered HTTP request;
no undocumented browser mechanism is asserted as fact. They do show that TUIC
business logic, its grid queue, its frame acknowledgements and its socket NODELAY
policy are not required to produce this phase concentration.

No repairable TUIC scheduling defect was isolated, so no production fix or RED was
fabricated. Painting outside rAF would remain a product scheduling trade-off,
not repair of a TUIC-specific extra queue proven by this evidence. The original
one-frame/help/non-regression criteria remain unmet; no post-fix `seq` ratio is
claimed. The coordinator owns the scope decision.

### Evidence and cleanup

`~/Gits/.tmp/2040-web/nodelay/` retains the exact observer patch, original source
backups, native log, acquisition and analyzers; `no-capture/` holds all per-key
frontend/server timelines and distributions. Its session was
`535a280b-09b1-4883-858f-d0044e33705d`.

`baseline/`, `baseline/binary/` and `baseline/echo/` each retain standalone server
and runner source, raw `probe.json`, screenshot, owned browser identity, server
PID/port record and `lifecycle.json`. `baseline/analysis.json` compares all three.
The first baseline runner attempt blocked its own Node HTTP event loop while
waiting synchronously for browser navigation. It timed out; its owned tab/daemon
were closed. Moving the server to a recorded child process removed that harness
deadlock. That attempt supplied no timing samples.

All successful controls closed their owned tabs/daemons and stopped their recorded
server child. The NODELAY lifecycle records zero remaining terminal sessions and
exact isolated instance stop exit 0. All temporary production source was restored
byte-for-byte, and no daily app/shared Chrome process was stopped or replaced.
