# Design Mode v2 S0 (throwaway)

This is an executable browser-capability probe, not product code. The native host
**echoes length-framed JSON over stdio**; it never discovers or connects to
`mcp.sock`, and never changes TUIC state. The HTTP collector is spike telemetry,
not the proposed product HTTP fallback.

All browser profiles and process logs belong under `~/Gits/.tmp/dm2-s0/`.
Do not use an everyday browser profile. Do not install the host in a user-level
browser directory. `setup.py` refuses profiles outside the spike temporary root.

From this checkout, start the collector and Vite in separate terminals:

```sh
python3 spike/design-mode-v2-s0/collector.py
node node_modules/vite/bin/vite.js spike/design-mode-v2-s0/fixture --host 0.0.0.0 --port 19340 --strictPort
```

The collector binds only 127.0.0.1:19341. Vite binds all interfaces to permit both
localhost and 127.0.0.1 in this cross-origin fixture; use it only for this local
spike, then stop it. The fixture contains no real application data.

Register the host and launch the installed Edge directly:

```sh
python3 spike/design-mode-v2-s0/setup.py ~/Gits/.tmp/dm2-s0/edge-profile
"/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge" \
  --user-data-dir="$HOME/Gits/.tmp/dm2-s0/edge-profile" \
  --no-first-run --no-default-browser-check \
  --load-extension="$PWD/spike/design-mode-v2-s0/extension" \
  http://127.0.0.1:19340/
```

`evidence/launch.json` records the exact measured launch. No automation or
debugging flag is passed to Edge. The unpacked ID is path-derived for this spike;
rerun setup after moving the checkout. Production needs a fixed manifest key.

The worker connects to the host and requests one immediate echo and one delayed
reply after 75 seconds. It does not send periodic keepalive traffic. Evidence is
written to `evidence/events.jsonl`. Change `fixture/app.ts` after load to produce
a real Vite HMR update. `?s0pick=1` in the iframe enables a synthetic click probe
through the real one-shot picker handler; it does not claim a trusted gesture.

The copied `extension/extract.js` is byte-identical to the product extraction
function at the time of the spike. DOM extraction runs in ISOLATED; framework
expando hints and WebSocket observation run in MAIN. Page hints remain untrusted.
The source-map probe fetches enumerated script URLs without CDP.

For the outstanding GUI checks, use the extension action to open the panel,
click Pick once, and click the top-level or frame target. Verify ordinary
navigation and Escape cancellation. Repeat capture *after* granting activeTab
through the extension action. The initial automatic capture intentionally tests
the no-gesture permission boundary and is expected to fail. The spike crop path
handles top-level rects only: frame-to-top transforms belong to S1.

For install persistence, manually Load unpacked in the throwaway profile, quit
that browser, and relaunch with the same profile **without** `--load-extension`.
The flag-loaded restart measured here does not prove GUI installation persistence.
For Edge panel behavior, also switch tabs and return to the fixture.

Focused host validation:

```sh
python3 spike/design-mode-v2-s0/verify.py
```

The host accepts the browser-supplied origin argument, uses native-endian uint32
framing, handles fragmented input and rejects oversized/truncated frames. It is
not an instance-aware product bridge. No Rust build or product suite is needed.
