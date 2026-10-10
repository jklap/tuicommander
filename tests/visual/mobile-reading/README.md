# Mobile history cap regression (1681-507e)

The phone session route mounts the actual `OutputView`, which appends finalized
log rows and replaces the current-screen region. This harness intercepts only
its HTTP/WS transport. It uses snapshots exported by the real `VtLogBuffer`.

`coordinator.raw` is a 200,001-byte, read-only ANSI tail captured from the running
COORDINATOR Claude Code PTY on 2026-10-10 at approximately 21:37 Europe/Madrid,
raw offsets 32147596..32347597. It includes spinner and live-region redraws.
It begins midstream. The original geometry and byte timing were not captured:
27 rows by 71 columns and 2,048-byte chunks are replay assumptions, not device
measurements. The cap is reached with plain seed lines before replay. Identical
recorded bytes below and above the cap must produce identical finalized text.

Run on rb with `bash tests/visual/mobile-reading/run.sh`. Requires Playwright
core and WebKit under `/home/rb/Gits/.tmp/pw`, plus WebKit's runtime libraries.
For missing host libraries, `bash tests/visual/mobile-reading/webkit-libs.sh`
downloads Ubuntu packages and extracts them under the rb workspace without
installing system packages. `webkit.sh` preserves the bundled WPE environment
and adds that local library path, without modifying the shared browser. The
system-ldconfig validation is skipped for this local extraction; actual WebKit
startup and rendering still have to succeed.

The browser uses iPhone 13 emulation; this does not prove physical iOS behavior.
PNG artifacts are written under `reports/visual/mobile-reading/` and collected
by rb `pull_artifacts`. Rust output and exported frames are under `junit.xml/`.
For before-fix browser proof, temporarily copy the RED run's exported frames to
`tests/visual/mobile-reading/red-frames.json`; this is a derived local artifact,
not another producer fixture. For browser-only retries, also copy the GREEN
export to `green-frames.json` and use `run.sh --browser-only`; neither derived
JSON file belongs in the commit.

The replay asserts a visible selected row more than 200 CSS pixels above the
bottom before continuing output. It must fail on already displayed text
vanishing, rather than on a browser launch or harness error.
