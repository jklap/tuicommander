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
