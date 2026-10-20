# Mobile live audit — 2026-10-20

## Scope and isolation

Stories: 1985-45bd and 1900-5ff6. Source revision: `e6ab3b7fcd41546348c4cd52d6f0893c3cb43359`, branch `chore/live-mobile`.

The native macOS headless backend was built in this worktree through `build-slot.sh cargo build --manifest-path src-tauri/Cargo.toml --bin tuic-remote --no-default-features`, with the mbx shim. Build passed in 1m30s; mbx doctor reported zero failures. This is a native build, not an rb test result. No test suite ran.

Backend port: 22185. Worktree Vite port: 22186, strict port. Backend HOME, instance configuration and temporary files were below `~/Gits/.tmp/results/live-mobile/`. No desktop instance was started. The production application, port 9876 and its configuration were not used.

The mandated browser wrapper used named session `live-mobile`, with its orphan scan scoped to the evidence directory. The viewport was 390×844. One disposable shell and one real Claude CLI were created through the isolated backend. Claude reached its idle composer and its local `/permissions` dialog; no model question or paid inference was submitted.

## Evidence

Retained local evidence is at `~/Gits/.tmp/results/live-mobile/`:

- `build.log`, `mbx-doctor.txt`: native build and cache prerequisites.
- `resume.mjs`, `resume.json`: browser probe and complete snapshots.
- `current.png`, `list-dom.txt`, `permissions-dom.txt`: browser screenshot and DOM captures.
- `before-exit-sessions.json`, `claude-before.json`, `permissions-output.json`, `model-output.json`: actual backend state and native CLI output.
- `network.mjs`, `network.json`, `app-logs.json`: request and diagnostic evidence.
- `cleanup.json`, `sessions-after-cleanup.json`, `pids.json`: owned resources and cleanup.

These are local evidence paths, not committed fixtures.

## 1900-5ff6: resume

The real mobile detail screen retained one root child and a shell rectangle at `(0,0,390,844)` in all recorded snapshots:

1. Baseline, with real Claude terminal output.
2. Three synthetic hidden/pagehide/pageshow/visible cycles, `persisted=false`.
3. Three synthetic cycles, `persisted=true`.
4. Native Chromium CDP `frozen` then `active`.

No `Runtime.exceptionThrown` event was captured by this probe. Native document visibility remained hidden. Synthetic visibility and persisted values are not native iOS evidence, and synthetic persisted events do not prove bfcache. Vite later reloaded the page, so this run cannot prove retained-document recovery after freezing.

The isolated backend received the existing `Mobile previous document lifecycle trail` warning. Its payload contains URL without query credentials, root child count, shell rectangle, viewport and lifecycle events. The Vite harness did not proxy the configuration endpoints: theme and notes requests returned HTML and produced expected-JSON warnings. Those are harness limitations, not the original grey-screen failure.

The empty grey-screen symptom was not reproduced. No original fix criterion was checked. Physical standalone iPhone behavior remains unknown.

## 1985-45bd: agent exit

The browser displayed the real Claude detail header, CLI/Chat controls, terminal output and editable composer. The local permissions dialog was captured in native output, but the API did not report `awaiting_input=true`; it is not an awaiting-state pass. No busy-agent exit, awaiting-agent exit, recovered shell submission or ended-PTY Back round trip was proved.

Later browser actions were not reliable enough to complete those checks. Requests to the isolated backend were pending in `network.json`, while a direct browser fetch with the explicit audit token returned both real sessions. A later wrapper navigation timed out with `waiting for another entity to finish attaching`. The exact cause of the pending application requests was not isolated. Shared-host cookie contention is a hypothesis only.

An attempt to use a separate `live-mobile.localhost` hostname received HTTP 403 `Untrusted Host`. The request boundary was not weakened or bypassed. That attempt supplies no application pass. The existing phone-only visual criterion remains unchecked.

## Harness failures and limits

- The first CDP probe accidentally selected the service worker target and timed out. The corrected probe pinned the exact owned page target and produced `resume.json`.
- Early programmatic `sendCommand` calls used an incorrect argument order and failed before command delivery. The corrected call opened only the local permissions dialog. These errors are not product regressions.
- Browser automation after freeze/reload was incomplete; unavailable round trips are not passes.
- Chromium emulation does not establish physical iPhone touch, standalone process reclamation, WebKit paint or app-switch behavior.

## Cleanup and remaining work

Both owned PTYs were deleted. `sessions-after-cleanup.json` is `[]`. Backend PID 49994 and Vite PID 49996 received SIGTERM; a subsequent `ps` found neither. The owned browser tab closed, then `agent-browser --session live-mobile close` closed its named daemon. Shared Chrome was not stopped. The temporary Vite configuration was removed.

No production code, tests or capture corpus changed. No story was closed and no acceptance criterion was newly checked.

Remaining emulated checks: obtain a stable authenticated browser session, then capture actual busy and confidently-awaiting agent exits, shell input/output recovery, PTY closure and Back. Remaining physical checks for Boss: perform the 1985 exit flows on a phone; switch the installed standalone PWA away and back three times for 1900. If the grey screen recurs, retain its automatic lifecycle report and capture URL, DOM/root/shell geometry, viewport and console before reloading. Do not infer the missing failure cause from this successful shell-retention probe.
