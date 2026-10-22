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
