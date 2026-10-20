# Mobile live audit — 2026-10-20

## Scope

Source: `b64dfbfe8b0fb3bc28d2b9ca33b440893679cdd8`, branch `chore/live-mobile2`.
Stories: 2029-994d and 2030-84bb. Both stories were already complete before this run. This report adds fresh evidence; it does not replace earlier RED or GREEN results.

The native macOS headless backend was built in this worktree through `build-slot.sh cargo build --manifest-path src-tauri/Cargo.toml --bin tuic-remote --no-default-features`, with the mbx shim. Build passed in 2m12s. `mbx doctor` reported zero failures and warnings. No Rust or frontend test suite ran. No rb result is claimed.

Backend: `127.0.0.1:22285`, instance `live-mobile2`, isolated HOME below `~/Gits/.tmp/results/live-mobile2/`. Frontend: this worktree's Vite at `http://127.0.0.1:22286/mobile.html`. The production port 9876, installed binary and desktop application were not used or restarted.

## Harness corrections

The mandated ab-stealth wrapper opened one named browser tab, `live-mobile2`. All application traffic used the same hostname and Vite origin. The temporary Vite proxy covered every backend API prefix, derived from `API_PREFIXES` in `mcp_http/mod.rs`, including root paths with query strings and WebSocket upgrades.

The proxy attached the newly generated instance cookie on upstream requests, set the upstream Origin to match its Host, and removed upstream Set-Cookie responses. This kept the isolated backend's authentication out of the shared browser cookie jar. No application authentication or request guard was disabled. A Bearer-header attempt returned 401; a mismatched proxy Origin returned 403. The final proxy used the existing cookie authentication contract and matching upstream origin. No second hostname was used.

The browser helper reapplied 390×844 emulation for each capture. Before the final touch run it also brought only the owned browser tab to the foreground. These checks mattered: the first touch run changed to 1400×828 before its second poll, and a later hidden-tab attempt timed out in touch dispatch. Those attempts remain local evidence, not the final mobile pass.

With the corrected harness, real browser `/sessions`, `/sessions/agent`, session writes, session output and session deletion requests completed. The final query-bearing `/logs?level=warn&limit=10` also returned HTTP 200. Native terminal streams and application polling remained usable across all three agents.

The headless backend returned JSON 404 for `/config/ui-prefs` and `/config/themes`. These were no longer Vite HTML fallbacks. Theme loading is therefore a remaining headless route limitation, not a fully passing configuration audit.

## 2029-994d: suggestion consumption — passed

A real Claude Code 2.1.286 / Sonnet 5.5 session published `Reply alpha` and `Reply beta`. The final run recorded:

| UTC time | Observation |
| --- | --- |
| 06:17:42.223 | Both suggestion buttons present; visible document, 390×844, one touch point |
| 06:17:42.261 | CDP dispatched a native touch to Reply alpha |
| 06:17:42.439 | Both suggestion buttons absent |
| 06:17:43.919 | First real `/sessions` response was HTTP 200; buttons remained absent |
| 06:17:46.919 | Second real `/sessions` response was HTTP 200; buttons remained absent |

All four DOM snapshots were visible at 390×844. Captured touchstart, touchend and click events were trusted; click pointerType was `touch`. Both poll payloads lacked Claude suggested_actions. The settled session also lacked suggested_actions. The real agent answered ACK without publishing replacement choices.

Evidence: [structured observations](2026-10-20-mobile-live-evidence.json), [before touch](/Users/stefano.straus/Gits/.tmp/results/live-mobile2/before-touch.png), [after two polls](/Users/stefano.straus/Gits/.tmp/results/live-mobile2/after-two-polls.png).

## 2030-84bb: slash menu checks

All screenshots listed below were captured with a visible document at 390×844. Text was entered through the real mobile textarea with CDP Input.insertText; commands were not filtered by the audit code.

- **Ego 0.1.2, idle: passed.** `/` produced the broad backend and mobile command list. Extending it to `/go` replaced the dropup with exactly `/goal`; the backend published exactly one slash_menu_item. See [broad menu](/Users/stefano.straus/Gits/.tmp/results/live-mobile2/ego-mobile-slash.png) and [filtered menu](/Users/stefano.straus/Gits/.tmp/results/live-mobile2/ego-mobile-go-idle.png).
- **Claude Code 2.1.286: passed.** `/` and `/he` produced different native command sets. The mobile dropup reflected Claude's fuzzy `/he` matches, with `/help` first. This is agent filtering, not strict prefix matching. See [broad menu](/Users/stefano.straus/Gits/.tmp/results/live-mobile2/claude-mobile-slash.png) and [filtered menu](/Users/stefano.straus/Gits/.tmp/results/live-mobile2/claude-mobile-he.png).
- **Codex 0.163.0: native filtering passed; mobile dropup unavailable.** `/` painted the broad native command block; `/sta` reduced it to `/status` and `/statusline` in the mobile terminal output. Neither state published slash_menu_items or rendered a Commands dropup. This matches the compatibility gap already recorded in the 2030 worklog for Codex 0.162.1. It is not proof that the mobile command-button overlay works for current Codex. See [broad native block](/Users/stefano.straus/Gits/.tmp/results/live-mobile2/codex-mobile-slash.png) and [filtered native block](/Users/stefano.straus/Gits/.tmp/results/live-mobile2/codex-mobile-sta.png).

The fresh **busy ego criterion is not certified**. A 390×844 snapshot showed a native `Working 12s` row and a one-item `/goal` dropup, but the corresponding backend state already said completed/idle. Earlier attempts also crossed turn completion or included `/go` in the submitted prompt because the harness typed before Send completed. These observations do not establish a synchronized working-state pass. The earlier checked busy evidence in the story remains historical evidence only.

Ego's first structured launch failed because TUIC's read-only attachment grant was below the inherited writable Gits root. A disposable EGO_HOME with an empty extra-root list avoided that launch conflict without editing the user's configuration. An isolated subscription launch had no stored credential. The usable ego session used the same GPT-6.1 Sol model through OpenRouter, with the authorized key passed only to the launched process; no key is in the report or retained captures. No provider fixture or terminal capture corpus was changed.

## Evidence, cleanup and limits

Raw DOM, native output, state, request, build and failed-harness records are retained below `~/Gits/.tmp/results/live-mobile2/`. The committed JSON contains the relevant viewport, touch, polling, command and backend state observations, not credentials or complete HTML. Screenshots stay in the local evidence directory; no binary screenshots are committed.

All three active disposable PTYs were deleted; the final session list was empty. The owned browser tab and named agent-browser daemon closed, preserving shared Chrome. Backend PID 66775 and final Vite PID 58991 received SIGTERM and were confirmed absent. Earlier owned Vite instances were stopped when the proxy was corrected. The temporary Vite configuration, generated pairing token, isolated HOME, isolated ego configuration and Vite cache were removed.

No production code changed, no new regression test was needed, and no story RED/GREEN result was rerun. This is Chromium mobile emulation, not a physical iPhone/WebKit, keyboard, standalone PWA or touch-hardware certification. Screenshot pixels remain available for coordinator review.
