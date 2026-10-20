# Isolated post-landing checks — 2026-10-20

## Scope and isolation

Source: `chore/live-postland`, commit `a0bec09eea760f18ad113745d66cc8d8bbe166de`.
The native headless build passed through build-slot and mbx in 2m20s. `mbx doctor` reported zero failures. No Rust or frontend test suite ran.

The check used this worktree's `tuic-remote`, private port 22231, isolated HOME and temporary paths under `~/Gits/.tmp/results/live-postland/`, and worktree Vite on port 22232. The named browser session was `live-postland`. The Vite proxy used the isolated Unix socket for the full application router. Initial TCP-proxy captures are not evidence of complete desktop-router parity. No second desktop instance ran, and no request targeted production port 9876.

Local evidence directory: `~/Gits/.tmp/results/live-postland/`. These are retained local artifacts, not portable links. Screenshots were captured, but this agent did not perform subjective pixel approval.

## Results

| Story | Fresh result | Evidence and limit |
| --- | --- | --- |
| 2008-5317 | PASS: isolated restart recovery | `restart-exchanges.json` records 33 real exchanges, including suspension setup retries. The recorder suspended a real shell tab, stopped its own backend, restarted it, resumed the stable identity with a different PTY, and read the same pre-restart durable message. Project progress succeeded. Headerless inbox returned bounded missing-context metadata; missing-project progress was refused. Replacement migration and live-owner stranded-mail assertions passed. `restart.log` reports CAPTURE OK. Python exercised MCP, not ego's stdio consumer. |
| 2013-73f9 | BLOCKED: real ego hop measurement | Both native ego launch attempts exited before a usable turn. No before/after real-ego discovery count was measured. Server discovery exchanges in the restart recording do not substitute for that consumer measurement. |
| 2018-1885 | BLOCKED: live ego idle state | No usable ego composer was reached. An exited session is not an idle-session pass. |
| 2026-9178 | PASS in browser mode: Codex native question | Installed Codex reported 0.163.0, not the historical 0.162.1. `pre-proxy.json` and `last-states.json` export the clean question and Red/Green/Blue/None of the above. `codex-mobile.png` and the browser transcript show the 390×844 buttons. One Green tap produced the native answered question and retracted awaiting. A second question was retained for the focused 1400×900 desktop capture: `codex-pending-desktop.png`, `codex-pending-desktop-state.txt`, `codex-pending-native.json`, and `codex-grid.json`. Native Tauri delivery was not tested. |
| 2027-7ef3 | PASS in browser mode: Grok native question | `startup.json` and `after-startup.json` record the real 1.0.62 Ask and shared Red/Green/Blue/z options. `grok-desktop.png` captures the desktop terminal. `grok-mobile.png` and the browser transcript show 390×844 controls. One Green tap produced native `PARITY_Q_DONE Green` and awaiting retraction, retained in `current.json`. Native desktop Chat delivery was not tested. |
| 2028-6999 | BLOCKED: live ego selector title | No usable ego turn or selector was reached. |
| 2032-8dc3 | PARTIAL / BLOCKED: local Chat opens, transcript not proved | A real MCP-spawned local Claude completed `LIVE_POSTLAND_CHAT_OK`. The focused terminal enabled Chat and opened the transcript UI (`local-chat-open.png`). It showed no messages; `/chat-view` returned `not_bound: no Claude agent is bound to this terminal` (`local-chat-api-error.json`). The isolated backend and child use different HOME values, and the run did not establish the transcript-store binding. An explicit `CLAUDE_CONFIG_DIR` retry reached native login selection (`last-states.json`), so it was stopped without login. This is not a proven remote-classification regression or a transcript-content pass. |
| 2044-31f3 | PASS: caller tab placement; shell scope | A real shell tab declared this already-existing assigned worktree via caller-bound MCP. `declaration.json` records the backend receipt. `declaration-dom.txt`, the browser transcript and `declared-sidebar.png` show its Terminal tabs group immediately below `chore/live-postland`, with the main workspace empty. The actual shell cwd remained the root checkout. No branch or worktree was created or removed. This run proves shell-tab placement, not a fresh ego-driven worktree creation. |
| 1787-4778 | PARTIAL PASS: HTTP/MCP; IPC unavailable | After enabling settings access in the isolated app's normal config path, MCP changed notification volume to 0.25 and HTTP changed it to 0.35. Both transports returned identical complete snapshots after each write (`settings-parity.json`). Initial disabled-tool replies were setup observations, not parity defects. A real IPC check requires an authorized desktop instance; none was started. |
| 1789-796d | PARTIAL PASS: browser Settings sample | General and the owner-bound All settings `app.shell` inspector rendered on the real isolated backend. `settings-general-live.png`, `settings-inspector.png`, and their DOM captures retain control identities and Source: stored. This is not exhaustive catalogue parity, a native WebKit check, light/narrow/plugin coverage, or subjective visual approval. |

## Blocking prerequisite

`ego-startup-output.json` records the exact native refusal:

> read-only directory: cannot enforce a read-only directory beneath a writable root

The backend-owned attachment grant lies under the isolated HOME within `~/Gits`. No permission, sandbox, configuration, transport or attachment-grant override was used to bypass this refusal. The remaining ego checks need an approved layout that preserves the read-only grant and an authenticated provider available in that layout. No credential store was read or copied, and no login was started.

## Harness observations

The initial Vite proxy targeted the restricted TCP router. The final proxy targeted the full isolated Unix router; `/config/repositories` then returned 200. Settings MCP starts disabled and requires both its enable flag and removal of the denied capability. Those setup corrections did not change production code.

The first Codex prompt submission after `/plan` was refused as `agent_not_ready` despite the native empty composer. The retained receipt is `codex-prompt-exchanges.json`. The question was subsequently submitted through the frontend's existing `sendCommand` helper, not a raw text-plus-Enter write. This single observation does not establish a persistent submission defect.

## Completion

Each assigned completed story receives a fresh worklog entry with the scope above. No story is reopened and no missing live proof is claimed complete. All ten `story-close.sh --check` preflights passed without writes (`story-close-checks.json`). The existing completed criteria were not reinterpreted as fresh live proof. `fmt-changed.sh main...HEAD`, `git diff --check`, and instruction-link validation passed. `sessions-after-cleanup.json` is `[]`; the named browser tab and daemon closed; the recorded backend and final Vite process groups received SIGTERM, and `ps` found neither PID. The temporary Vite configuration was removed. No production implementation changed.
