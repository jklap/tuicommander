# Design Mode v2 S0 results

Date: 2026-10-08. Plan: `docs/design/design-mode-v2.md` (§4, §5, §9, §13 S0, §14).
Native macOS evidence only; Windows/Linux and branded Google Chrome were not tested.

**Gate recommendation: retain X-A with native messaging, provisionally.** Edge
loaded the extension, resolved the custom-profile host, and kept the worker's
native port alive. There is no measured channel failure justifying the HTTP
fallback. Source-map discovery worked on this Vite fixture without CDP, so there
is no measured reason to select X-B. **S0 is not fully passed:** GUI persistence,
panel gestures/tab switching, trusted picking, and successful screenshot cropping
remain blocked. Keep the product CDP backend until those checks pass; this spike
authorises no deletion or product rollout.

The native host is an **echo-only stdio adapter**, permitted by the spike brief.
It does not relay to `mcp.sock`; instance handshake and real read-only socket relay
remain unproven. The spike telemetry collector is unrelated to §5.1's product
HTTP fallback.

The raw event log redacts the ephemeral fixture Vite WebSocket token; timestamps,
nonces and event content are preserved. The final MAIN probe omits URL query
strings to avoid recording that token in subsequent runs.

Evidence paths below are relative to `spike/design-mode-v2-s0/`. The exact Edge
argv and server commands are in [launch.json](../../spike/design-mode-v2-s0/evidence/launch.json).

| Item | Browser and exact version | Result | Command / step | Evidence and limits |
|---|---|---|---|---|
| (a) Flag load and Load unpacked persistence | Microsoft Edge **154.0.4258.62**; GUI attempt: Google Chrome for Testing **149.0.7827.54** | **PASS flag load; BLOCKED GUI persistence** | Edge launch from `evidence/launch.json`, with `--load-extension=<extension>` and dedicated `--user-data-dir`. Via wrapper: `chrome://extensions`, Developer mode, Load unpacked. | `events.jsonl`: Edge `worker_boot`, content injection, native reply. Restart with the same flag also loaded it. Native folder chooser is outside agent-browser's CDP tree; MacControl lacks Accessibility permission. No directory selected and no no-flag restart of a GUI-installed extension: persistence is unproven. |
| (b) Side panel, gesture, tab switch | Microsoft Edge **154.0.4258.62** | **PASS API presence; BLOCKED interaction** | Worker reads `typeof chrome.sidePanel.open`, calls `setPanelBehavior({openPanelOnActionClick:true})`; extension includes panel + Pick button. | `worker_boot.sidePanel="function"`, no `panel_error` observed. API availability is not proof of opening. Native Edge gesture and tab return could not be driven with available permissions. CFT has no loaded extension because its folder chooser is blocked. |
| (c) Custom-profile host registration | Microsoft Edge **154.0.4258.62** | **PASS (echo)** | `python3 spike/design-mode-v2-s0/setup.py ~/Gits/.tmp/dm2-s0/edge-profile`; worker `connectNative('dev.tuic.dm2_s0')`. | `native_reply` echoes nonce and origin `chrome-extension://jnonmjddknoldgkhmfialhdimebopeop/`. The only spike host manifest is `<user-data-dir>/NativeMessagingHosts/dev.tuic.dm2_s0.json`; no global/user browser registration written. Real TUIC relay is untested. |
| (d) Native port keeps worker alive | Microsoft Edge **154.0.4258.62** | **PASS for measured 75 s interval** | Host replies immediately, then sleeps 75 s before another reply. No Edge CDP/worker inspector attached; no periodic extension timer. | Same worker nonce `a12a134f-b8dd-4f37-bddb-da277705b684` receives echo at `1791462407001` and delayed reply at `1791462482000` (74.999 s). Last intervening HMR message at `1791462432495`, leaving 49.505 s without extension event traffic before delayed reply. Does not prove crash recovery or indefinite lifetime. |
| (e) Cross-origin iframe picker | Microsoft Edge **154.0.4258.62** | **PASS synthetic protocol probe; BLOCKED trusted/visual pick** | Top page `http://127.0.0.1:19340/`, iframe `http://localhost:19340/frame.html?s0pick=1`; ISOLATED fixture probe arms and dispatches one click through the picker handler. | `pick` from frameId **2**, URL on localhost, selector `#target`, text `Frame target`; following `picker_stopped.armed=false`. Different hostnames make different origins. The background frame returned a zero rect: no coordinate/crop success is claimed. Real pointer hit testing, overlay rendering, Escape and frame transforms remain unverified. |
| (f) captureVisibleTab + crop | Microsoft Edge **154.0.4258.62** | **FAIL without gesture (expected); BLOCKED successful crop** | Worker attempts capture at page DOMContentLoaded with localhost host permissions and declared activeTab, before any extension action gesture. | `capture_error`: `Either the '<all_urls>' or 'activeTab' permission is required.` Declaring activeTab or granting localhost host access is insufficient here; the action must grant activeTab. No successful capture or PNG crop exists. Do not broaden permissions merely to bypass this check. |
| (g) extract.js as plain function | Microsoft Edge **154.0.4258.62** | **PASS DOM extraction; framework hints separate** | Byte-copy product `extract.js`; content script calls `tuicExtract.call(target)`. | `plain_extract` records selector, paths, snippet, styles and top-level rect `{x:32,y:102.015625,width:180,height:60}`. Source is empty in ISOLATED; MAIN `main_hint.source="fixture/app.ts"` separately finds the page-world Vue-shaped expando. The expando is an explicit probe fixture, not evidence of a full Vue application. |
| (h) Vite HMR probe | Microsoft Edge **154.0.4258.62** | **PASS** | MAIN wraps WebSocket at document_start. Append a comment to `fixture/app.ts` after load. | `hmr` records `connected` and actual `js-update` for `/app.ts` in top frame and iframe at `1791462432495`. This proves observation, not successful application of a product UI change or reload policy. |

## Round 2 permission recheck (2026-10-08)

**BLOCKED before browser launch.** Boss reported that MacControl already has its
permissions. The new live checks still refuse capture. The exact help and error
responses are preserved in
[maccontrol-round2.txt](../../spike/design-mode-v2-s0/evidence/maccontrol-round2.txt).

- Screen state was checked first: `ioreg -n Root -d1` reports
  `"IOConsoleLocked" = No`, with an active, logged-in console session.
- `maccontrol.help({})` succeeds and returns its normal usage instructions.
- `maccontrol.capture_screen({})` and `maccontrol.list_windows({})` both return
  `isError: true` with the exact text:
  `Screen Recording permission required. Grant access in System Settings > Privacy & Security > Screen Recording`.
- `maccontrol.processes({name:"mcpmaccontrol"})` identifies two running processes
  of `/Applications/MCPMacControl.app/Contents/MacOS/mcpmaccontrol`: PID **51489**
  (server; `--server` in `ps`) and PID **34336** (no server flag). `codesign`
  identifies the app as **com.sstraus.mcpmaccontrol**, with an ad-hoc signature.
  This is the installed app to inspect in Privacy & Security > Screen Recording.
  **Neither the error nor the available TCC logs identifies the denied process:**
  a targeted 10-minute TCC log query returned no matching attribution record.
  The exact responsible process and why the existing grant is ineffective remain
  unknown; no permission reset or process restart was attempted.

Items **(a), (b), (f)**, and the optional trusted cross-origin **(e)** remain
blocked with their round-1 results unchanged. A current Screen Recording refusal
is established; a current Accessibility refusal is **not** established, because
round 2 did not attempt input after the capture prerequisite failed. No Edge
profile, everyday browser data, credentials, or permission settings were touched.
This does not change the provisional X-A recommendation or establish an extension
capability failure. Windows/Linux remain untested.

## Additional gate checks

- **Strict CSP:** top page sets `script-src 'self'`, with no unsafe-inline/eval,
  and self-only connections plus its explicit Vite WebSocket. ISOLATED extraction
  and MAIN expando/HMR reports both arrived. This covers the supplied strict-CSP
  fixture, not every CSP/framework combination.
- **Source maps without CDP:** worker enumerates `document.scripts` and fetches
  `/app.ts` and `/@vite/client`; both return inline base64 source-map comments,
  recorded as `source_map.found=true`. This establishes discovery on Vite only,
  not complete source-to-element resolution, dynamically imported modules,
  header-only maps, or all frameworks. Keep those S1/S6 limitations explicit.
- **No automation banner mechanism on Edge:** exact argv has neither
  `--enable-automation` nor a debugging flag. Edge reports `webdriver:false`.
  Visual absence of the banner was not captured; it is not marked visually passed.
- **GUI evidence:** [CFT extensions screenshot](../../spike/design-mode-v2-s0/evidence/cft-extensions.png)
  and [attempt log](../../spike/design-mode-v2-s0/evidence/gui-attempts.txt).
  MacControl reports both Screen Recording and Accessibility permissions missing.
  `ioreg` returned no locked=true entry and the CFT screenshot succeeded; these
  failures are permission failures, not evidence of a locked display.
- **CDP disclosure:** used only through the mandated stealth wrapper for the
  extensions-page GUI attempt because MacControl was unavailable. Edge's capability
  measurements used no CDP. Wrapper CFT UA Chrome/149 matches its 149 binary and
  its required webdriver check passed. Everyday profiles and authentication stores
  were never accessed.

## Validation and remaining work

`python3 spike/design-mode-v2-s0/verify.py` passed fragmented, multiple Unicode
frames, and rejection of oversized/truncated frames; output is in
`evidence/framing.txt`. `node --check` passed worker, picker and MAIN scripts.
`git diff --check` passed. Biome matched zero files because spike paths are
outside its configured scope; this is not claimed as a formatting pass.
The instruction-link validator failed on the pre-existing CLAUDE.md violation.
Owned Edge/CFT/Vite/collector processes were stopped and spike native-host
registrations removed; dedicated profiles/logs remain under ~/Gits/.tmp/dm2-s0/.
No product compilation or application suite was run: all code is throwaway and
outside product imports. No desktop TUIC was launched.

To close S0, Boss or a permitted GUI runner must select the unpacked directory in
the **throwaway profile**, verify a restart without the load flag, open the panel
with its action, switch tabs and return, perform trusted top/frame picks, and
capture/crop after activeTab is granted. The manual sequence is in the spike
README. No missing GUI permission proves native messaging unreliable; keep HTTP
fallback and X-B contingent on a reproducible capability failure.

Out of scope: pre-existing `AGENTS.md` modification and `CLAUDE.md` instruction-link
violation were preserved. No unrelated story was changed or closed.

## Round 3: CFT without MacControl (2026-10-10)

CFT **149.0.7827.54**, dedicated profile under `~/Gits/.tmp/dm2-s0/1672-profile`,
loaded the unpacked extension with its flag. CDP was used only as the verification
harness; product native messaging and extraction do not use it. The named stealth
wrapper attached to this owned browser and verified `navigator.webdriver` is
undefined. No desktop TUIC was built or launched.

- **PASS side panel rendering:** a trusted CDP pointer click on a temporary
  extension-document button opened the actual browser sidebar. Its document
  measured 360 × 1231; see `evidence/cft-panel-0.png`. This proves CFT's gesture
  path, not Edge's toolbar or tab-return behaviour.
- **PASS trusted top and cross-origin frame picks:** CDP pointer events reached
  the one-shot picker, producing top-frame and frame-7 payloads with nonzero
  rectangles. See `evidence/round3-events.jsonl` and
  `evidence/cft-trusted-pick.png`. The earlier synthetic zero-rectangle limit no
  longer blocks implementation of the isolated picker.
- **Not passed:** capture still requires the browser action's activeTab grant;
  opening a panel from an extension document does not grant it. No successful
  crop is claimed. Product must report capture errors and retain useful DOM
  evidence, without broadening host permissions.
- **Native Edge checks remain:** GUI Load unpacked persistence without the flag,
  toolbar activeTab capture, and sidebar return after tab switching. These need
  native browser UI input; they do not block building S1-S4 on the measured CFT
  capability. The existing CDP backend stays intact.

Decision: proceed with X-A and native messaging. S2 must prove the real socket
relay and explicit-instance handshake; the spike echo is not that proof.

## Round 4 prerequisite audit (2026-10-14)

The managed `design-mode` peer tried to recheck the capture prerequisite through
TUIC's discovered `maccontrol__capture_screen` tool. Schema retrieval failed
before the capture call:

```text
invalid MCP candidate: invalid collapsed tool schema: unknown field `annotations`, expected one of `name`, `description`, `inputSchema`
```

This establishes a tool-schema discovery failure, **not** a current Screen
Recording refusal or a browser capability failure. No capture, browser launch,
profile change, desktop restart, or permission change was performed. Items (a),
(b), (e), and (f) retain their earlier evidence and limits; no new S0 pass is
claimed.

The source audit found the product autoload and agent browser command work
already integrated on main at `15e04143b4`. Automatic cold-launch extension
loading replaces manual product onboarding, but does not prove the spike's
historical no-flag GUI-install persistence check. Keep that distinction in the
final test evidence.

For the coordinated live test, use the throwaway profile and the spike README:

1. Recheck capture through a working MacControl schema, then record its actual
   response. Do not infer permission status from this discovery error.
2. Load unpacked in that profile, close its browser, and restart without the load
   flag. Record whether the extension remains enabled: item (a).
3. Open the sidebar through the native extension action, switch tabs, and return:
   item (b).
4. Make trusted top-level and cross-origin frame picks, and verify the picker
   stops after one pick and Escape cancels it: item (e).
5. After the extension action grants activeTab, capture the top-level target and
   retain the successful PNG crop with its rectangle: item (f).

Separately, product checks for stories 1834-f6c4 and 1724-fe7e still need Boss's
entry-point/pick delivery, visible-tab screenshot and sidebar history, and
Claude Code/Codex/ego tool-list refresh on Arm/Stop. Source and isolated protocol
tests are not evidence that these installed-browser checks passed.

### Product-pilot continuation (2026-10-10)

After the S2 host install, the product sidebar completed a real native `hello`
against the selected `mcp.sock` and pinned instance. CFT disabled the unpacked
extension on runtime reload while Developer mode was off
(`unsupportedDeveloperExtension=true`). A trusted click on Developer mode in
`chrome://extensions` enabled it immediately. The pilot setup instructions
already require this switch. The native host's framing/manifest/version tests and
repository-recipient test passed (4/4).

A live session observation also established that `is_remote=true` describes
sessions created through the API, including local PTYs. It is not a remote-daemon
ownership signal. Recipient isolation uses `connection_id` instead. The running
pre-v2 TUIC returns HTTP 404 for `/design-mode/extension`; the product sidebar
prints the exact endpoint and restart requirement while retaining cards. Native
Edge checks remain outstanding.

## Round 5 recheck (2026-10-17)

- `ioreg -n Root -d1`: `IOConsoleLocked = No` (13:01 CEST).
- `screencapture -x` from the managed shell succeeded (6.3 MB PNG, exit 0): no Screen Recording denial for the shell.
- The MacControl tool is not exposed to this session (ToolSearch "maccontrol": no match), so the MacControl-specific prompt could not be re-read.
- Trusted GUI input is unavailable: `osascript` System Events fails with error -1743 (not authorized to send Apple events). `cliclick` exists but Accessibility was not probed, to avoid a permission dialog on the Boss screen.
- Result: S0 items (a), (b), (e), (f) stay BLOCKED on trusted input. No new S0 pass is claimed.


## Round 6: isolated web and MCP checks (2026-10-22)

**MacControl is not a prerequisite for these checks.** Boss's correction applies:
TUIC's browser frontend can exercise its shared Tauri/HTTP backend, and an owned
Chrome test instance may use CDP as a harness. The product launcher must still
omit CDP and automation flags. No daily app restart or daily browser profile was
used. These results do not close S0 or the screenshot/client-refresh criteria.

### Setup and evidence

- Worktree: `fix/design-mode-web-checks`, starting at `0d02ef11470f`.
- Built and started with `make agent-test AGENT_TEST_INSTANCE=agent-test-design
  AGENT_TEST_PORT=19885`, after its matching dry run. This target runs its native
  build through build-slot/mbx. The isolated backend reported instance
  `ffd92cb1-5bdd-4de0-bf17-c33f918a184d`; the explicit socket is under
  `~/Gits/.tmp/tuic-at/8244dd0e0d93010a/`.
- The authenticated TUIC browser frontend loaded at `http://127.0.0.1:19885/`
  through the named stealth-wrapper/agent-browser session `design-web-ui`.
  `tuic-web-snapshot.txt` contains its real sidebar and terminal controls.
- Chrome for Testing **149.0.7827.54**, real product extension **2.0.0**, fixed ID
  `eefbgjmmpaimpeaflioojmdipmfchefo`; dedicated profile
  `~/Gits/.tmp/design-web/profile`. The freshly built native host installed an
  explicit socket/instance binding. Its sidebar reported that exact socket and
  the isolated Claude PTY recipient `3b946888-932f-434e-afb0-c595de039544`.
- The product Arm path launched Edge in the isolated agent-test instance only.
  Its own Edge processes were stopped before the harness used CFT. Browser cache
  and native-host files for CFT stayed under the Gits test profile. Product launch
  and agent-test state also used the explicitly selected isolated instance's
  application-support directory, never the daily configuration directory. The debug
  credential-store initialization also selected
  `~/.tuicommander-dev/instances/agent-test-design/` (startup log); no credential
  store was read or copied by the harness.
- The fixture was served from this worktree's `.tmp/design-web-page` on **19886**.
  Its top origin was `127.0.0.1`; its iframe origin was `localhost`. Native host,
  socket, DOM picks and MCP replies were real, not mock browser state.
- Raw evidence and harnesses: `~/Gits/.tmp/design-web/`. Build/start logs:
  `~/Gits/.tmp/results/design-web-{start,instance}.log`. No application suites,
  Clippy, mutation runs or rb tests were needed for this documentation/evidence
  change. The native build is not claimed as a test-suite pass.

### Per-criterion results

| Story / criterion | Non-native verification path | Measured result and remaining scope |
|---|---|---|
| 1724 AC4: sidebar who/what/when | Actual extension side panel attached to the isolated backend; MCP `ui design_screenshot`; expand its **Agent actions** details | **PASS DOM observation.** `history-dom-expanded.txt` shows `10:21:43 AM · 3b946888-932f-434e-afb0-c595de039544 · screenshot · failed`. `history-after.json` retains the same real action record. A failed browser action is correctly visible, not hidden. This is not a visual screenshot pass. |
| 1724 AC5: routing tests plus matching screenshot reality | Existing recorded routing/handler GREEN evidence, plus a real MCP navigation to the worktree fixture and an extension screenshot request | Navigation returned the requested URL (`design_navigate-headed.json`). Screenshot returned Chrome's `Either the '<all_urls>' or 'activeTab' permission is required.` No matching PNG was produced; the mixed criterion stays open. A later navigation during harness reconnect returned the production ten-second `command_timeout`; it is retained, not counted as successful delivery. |
| 1724 AC6: session-scoped discovery and Claude/Codex/ego refresh | MCP initialize, persistent SSE, tools/list before and after Arm/Stop, cached call after Stop, and separate installed-client probes | **Protocol PASS, client refresh unproved.** Arm and Stop each emitted `notifications/tools/list_changed`. Stop succeeded; the cached `ui design_screenshot` call then refused `Design Mode not armed for this session`. `tools-armed.json` and `tools-stopped.json` are identical under the later accepted #2067 grouping: `ui` always lists `design_*` actions and there is no standalone `design_mode` tool. The old tool-add/remove wording needs reconciliation with #2067, not another implementation of the removed tool. |
| 1724 AC19: visual action history | Screenshot of the real extension panel, not a static mock or the orchestrator WebView | **OPEN.** DOM history is proved above, but `Page.captureScreenshot` timed out at the harness's eight-second bound (`history-screenshot-error.txt`). No PNG is claimed. |
| 1724 AC20: visual screenshot correspondence | Compare the MCP screenshot with a harness screenshot of the same reviewed fixture | **OPEN.** The extension capture lacks `activeTab`; the independent harness capture also timed out. Neither half of the comparison is replaced with a fixture image. |
| 1724 AC21: three-client Arm/Stop refresh | Actual installed clients with invocation-scoped MCP configuration and isolated client homes; no model request or credentials copied | **OPEN.** Claude Code **2.1.306** reached its initial theme/onboarding screen; Codex **0.163.0** reached its sign-in screen; ego **0.1.2** reached model selection with isolated providers not signed in. These bounded probes did not establish a connected, ready MCP client, so they prove neither refresh support nor lack of support. The server-side cached-call fallback is verified independently, not attributed to any client. Transcripts: `{claude,codex,ego}-client.txt`; Claude startup log: `claude-debug.log`. |
| 1672 AC1 / S0(a): load and persistence | Test-profile flag load; `Extensions.loadUnpacked`; restart the same profile without `--load-extension`; `Extensions.getExtensions` | Flag/API loading worked. After each no-load-flag restart the test API returned `extensions: []` (`persistence-result.json`). This is **not** a successful persistent GUI installation, and does not prove that Chrome's actual **Load unpacked** chooser installation fails. That separate install/persistence step remains open. |
| 1672 AC1 / S0(b): side panel and tab return | `Target.getTargets` with `type: tab`, then `Extensions.triggerAction` using that **tab** target (not the page target) | The actual `panel.html` target opened and its DOM contained the real native connection and recipient. No OS toolbar click was needed to open it. Tab-return restoration and rendered screenshots were not proved. A page target is rejected with `Action can only be triggered on a tab target`; that is a harness-addressing distinction, not a product failure. |
| 1672 AC1 / S0(e): trusted top/frame picks and cancellation | Reload owned fixture pages after extension installation, then real `Input.dispatchMouseEvent` in top/frame targets; panel's real arm/stop path | **Partial PASS.** Persisted real picks contain `#target` at `{x:44,y:216.3125,width:278.828125,height:76}` and cross-origin `#frame-target` at `{x:8,y:8,width:291.265625,height:78}` (`picks-result.json`). No hand-written pick storage was injected. Escape and Stop inputs were exercised, but one-shot/cancellation ordering was not independently asserted; do not close that part. Reload after extension installation is required to inject content scripts into already open pages. |
| 1672 AC1 / S0(f): captureVisibleTab and crop | Invoke the browser action through the test API, then the real capture and crop path, without broadening permissions | **OPEN.** `Extensions.triggerAction` opened the panel but did not grant usable `activeTab` in this run, even on the selected fixture tab. Direct extension capture and the top pick both returned the precise Chrome permission error. Cross-origin picks explicitly retained DOM evidence and omitted their image. No successful crop exists. |

### What still needs a browser-chrome gesture, and what does not

The exact unproved gesture is invoking the extension's real browser action so
Chrome grants `activeTab` to the reviewed tab. Opening the panel from an
extension document, or calling the tested `Extensions.triggerAction`, did not
establish that grant. Likewise, the original GUI **Load unpacked** persistence
step has not been replaced by a successful persistent test-API install. These
are browser-chrome installation/permission checks, not reasons to block MCP
notifications, sidebar DOM checks or ordinary TUIC web controls on MacControl.
No claim is made that every possible browser API alternative has been exhausted.
Toolbar pinning itself is not a product acceptance criterion.

During the capture attempts, `ioreg` reported `IOConsoleLocked = Yes`. Both the
headed browser and the tested `--headless=new` configuration timed out on
`Page.captureScreenshot`; `fromSurface:false` and a later software-rendering
configuration also failed to produce a PNG. The concurrent lock is recorded as
context, not proved as the sole cause. No Accessibility/Screen Recording request,
permission reset, unlock attempt or permission broadening was performed.

The next verification pass needs a rendering-capable owned browser session, a
working browser-action grant, and ready MCP clients configured for the isolated
socket. It does **not** require restarting Boss's daily TUIC, installing into his
everyday browser, or waiting for MacControl before trying the web/MCP checks.


### Cleanup and validation

Design Mode was explicitly stopped, the owned PTY was deleted, all three remaining
named agent-browser daemons were closed, and the recorded Chrome/fixture processes
were terminated. `make agent-test-stop AGENT_TEST_INSTANCE=agent-test-design
AGENT_TEST_PORT=19885` stopped the isolated desktop. The final recorded-PID check
found none running and the isolated desktop PID file was absent. Raw test-profile
state and transcripts remain as evidence; no daily process was restarted.
Instruction-link validation and `git diff --check` passed. The formatting helper
was run against the branch change range; this report changes Markdown only.
