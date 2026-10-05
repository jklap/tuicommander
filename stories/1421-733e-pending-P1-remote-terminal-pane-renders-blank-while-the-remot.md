---
id: 1421-733e
title: Remote terminal pane renders blank while the remote PTY has output, with no error toast
status: pending
priority: P1
type: fix
created: "2026-10-03T10:50:29.600Z"
updated: "2026-10-04T15:13:21.011Z"
dependencies: []
started_at: "2026-10-03T10:55:10.440Z"
---

# Remote terminal pane renders blank while the remote PTY has output, with no error toast

## Problem Statement

Desktop sidebar showed PERSONAL MINT (remote) > shell > Terminal 24 with Claude icon, green dot, but the pane was empty while the remote PTY (inferred pe-3) held 1150 scrollback lines and the process was alive. Docs promise the output stream over WebSocket and a persistent error toast on a failed stream. Silent failure: Boss concluded the terminal was dead. Evidence: ~/Gits/.tmp/coordinator/urgent-remote-incident.md.

## Acceptance Criteria

- [ ] Root cause proven in the attach/replay/scrollback path over the remote transport
- [ ] RED: regression test that attaching to a remote PTY with existing scrollback renders that scrollback — catches: replay skipped for remote sessions or stream subscribed after the initial output
- [x] A failed or stalled remote output stream shows the persistent error toast promised by docs/user-guide/remote-access.md
- [x] Verified through the Mac to mac-mint reproduction harness or a local tuic-remote fixture with agent-browser; no redeploy on mac-mint without Boss approval

## Proof

- [ ] [completeness] Completeness
- [ ] [feature-availability] Feature availability
- [ ] [robustness] Robustness
- [ ] [resilience] Resilience
- [ ] [security] Security
- [ ] [defense-in-depth] Defense in depth
- [ ] [input-validation] Input validation
- [ ] [thread-safety] Thread safety
- [ ] [configurability] Configurability

## Work Log

### 2026-10-03T10:55:11.475Z - Contract: an existing remote PTY paints its current viewport and scrollback through its owning connection, without requiring new output. Production state comes from terminalsStore session ownership and the daemon grid buffer; server sends a full grid on WS attach. Failure, stalled initial replay, and decode errors must produce a persistent terminal-scoped toast. No idle-output deadline after healthy replay. Inspect live desktop ownership and daemon read-only responses before selecting the regression. Shared boundary: grid WS transport, no server wire changes assumed.

### 2026-10-03T11:02:29.217Z - Verified before edits: configured mac-mint pe-3 compressed grid replay decodes 40 rows, historySize=1150, needsFullFrame=false. Current Terminal 24 is pe-4, not incident-inferred pe-3. Recorded pe-4 idle full grid as remote-idle-grid-1421.bin. Proven client defect: subscribe resolves at onopen; no initial-frame deadline, post-open close only reconnects/logs, decode errors only warn. Regression: stalled attach reports persistent stream failure then reconnects and delivers recorded existing viewport without new PTY output. Tests observe consumer frame text and stream-error event; no server or external CLI behavior invented.

### 2026-10-03T11:05:24.665Z - RED observed before production edits on b3e6fe19e: remoteTerminalReplay.test.ts 3 tests failed (expected one stream-error, received zero). Initial fixture loader URL required a setup correction; the subsequent RED failed for the named missing behavior. Log: ~/Gits/.tmp/tuic-1421/red.log. Fix scope: first-frame watchdog, failure events, renderer persistent toast, replay-based reconnect health.

### 2026-10-03T11:17:20.766Z - Validation: initial 3-file targeted Vitest run passed 46 tests in 19.48s. Added component consumer coverage: post-attach error stays rendered after 60s; 2 component tests pass in 11.61s with no async leaks after correcting fake-timer setup. TypeScript noEmit clean. Separate production and test-support fixture builds queued via build-slot; parent authorized fixture-only test-support build and one shared launcher. Historical blank-pane cause remains uncertain: pe-3 replay healthy live, current term-24 maps pe-4. No desktop or mac-mint restart, no remote input sent.

### 2026-10-03T11:20:59.713Z - Committed ce55288df on fix/1421-remote-pane-blank, clean tree, no push. Final impacted transport/replay run passed 42 tests in 4.75s; rendered persistent-toast run passed 2 tests in 11.61s. fmt-changed.sh worktree main...HEAD exit 0, instruction links pass. Criterion 3 checked; criterion 1 remains open because the historical screenshot-to-PTY mapping and exact blank cause are not proven. Build/visual fixture checks await the background build event; no polling or live app restart.

### 2026-10-03T11:42:30.901Z - Production headless build and fixture-only tuic-core/test-support build both passed (validation.log). Visual check used worktree Vite :1422 + isolated tuic-remote :19877, config/socket under ~/Gits/.tmp/tuic-remote-fixture/tuic-1421-visual. Prepared output before mounting renderer: HTTP counted 1153 total lines, screenshot replay-visible.png visibly shows REMOTE REPLAY 1082..1150 and READY FROM EXISTING SCROLLBACK. Deleted only disposable session and stopped own daemon: stream-error-visible.png shows persistent Terminal stream failed / Terminal stream disconnected toast. Stealth webdriver undefined; UA Chrome149 matches CDP Chrome149.0.7827.54. Own Vite and daemon stopped; mac-mint untouched. Shared-profile browser lock delayed initial commands; isolated wrapper profile under Gits succeeded. Criterion1 historical root and criterion2 exact claimed replay-skipping RED remain unproven; observed RED covers silent stream failures and replay recovery, not server skipped replay.

### 2026-10-03T11:54:00.365Z - Critic follow-up contract: only WS transports publish local stream failure notifications, without a Tauri PTY listener. After a replay timeout, detach and schedule reconnect directly even if close never completes. A live server grid watch with no available frame explicitly sends grid-replay-empty; client treats only this marker or an accepted binary frame as replay success, never unrelated text events. Add real WS endpoint regression for idle missing buffer and consumer regressions for plain/negotiated empty replay; retain critic half-open test. No RED runs per coordinator; final full Vitest once on rb and narrow Rust endpoint nextest on Mac.

### 2026-10-03T12:10:34.884Z - Final HEAD707360d9280d3d5fd8b46d10498d77fe27ae4b0a validated once per coordinator: rb full Vitest571 files/8145 tests passed; Mac targeted nextest grid_ws_empty_replay_does_not_leave_healthy_idle_session_silent passed in0.069s; production headless build39.51s and fixture test-support build41.98s passed. Log ~/Gits/.tmp/tuic-1421/critic-validation.log. Repeated visual test on rebuilt isolated daemon and Vite:1153 total lines existed before mount, replay-final.png shows1082..1150/READY; stream-error-final.png shows persistent disconnect toast after deleting only disposable PTY/stopping own fixture. Own Vite/daemon/Chrome stopped. fmt-changed final pass. No mac-mint or desktop restart. Criteria1 and2 remain open: exact historical blank cause and replay-skipping RED unproven; no story completion claim.

### 2026-10-03T12:11:42.011Z - Correction to final formatting note: critic tests c85cb3ec4 arrived during verification, briefly causing fmt-changed to report their negotiated socket test unformatted. Current HEAD9981194838654f5bf938c3edc683bc61197f3d6d includes the critic formatting commit; final fmt-changed passes and tree is clean. Validation totals8145 remain scoped707360d92 (production unchanged); new critic tests await separate critic results.

### 2026-10-03T12:16:12.737Z - Final requested two-file run on3abccd34f7ba8fbaee5f3b287681ee17c0d38293:9 tests passed in3.76s, final-two-tests.log. Mock rejects unsupported PTY stream-error registration by ignoring it; malformed-frame regression now requires reconnect and healthy idle recovery. Daemon-first rollout documented. Final fmt pass, tree clean. Exactly one terminal RESULT sent to parent and saved ~/Gits/.tmp/results/tuic-1421-1791029740.md; criteria1/2 gaps remain explicit.

### 2026-10-03T12:17:42.752Z - Coordinator authorized merge-main follow-up: HEAD2f8a97bef172f8989ef62ef434969eed318eb7f9 merges main into fix/1421-remote-pane-blank. Sole conflict docs/frontend/canvas-terminal-audit.md resolved preserving both remote stream-health and answers-only-history paragraphs. No code conflicts; no test rerun per coordinator. Final fmt passes, clean tree, no push. Parent informed new head.

### 2026-10-04T15:13:20.732Z - Status reset to pending by the coordinator on 2026-10-04: no live agent; last state: Coordinator authorized merge-main follow-up: HEAD2f8a97bef172f8989ef62ef434969eed318eb7f9 merges main into fix/1421-remote-pane-blank.

