# Isolated parked-shell browser check — 1986-1236

Date: 2026-10-20. Source: `chore/live-1986`, runtime commit `3c2cdfd007` (empty branch-start commit on main `09b2ad81ff`). No production code changed.

## Scope and isolation

The native macOS headless backend was built in this worktree through build-slot and mbx:

```sh
cd "<worktree>"
export PATH="<mbx-shim-directory>:$PATH"
build-slot.sh cargo build --manifest-path src-tauri/Cargo.toml --bin tuic-remote --no-default-features
```

The confirmed worktree build passed in 50.71 seconds with 614 cache hits. Its dependency receipt names this worktree. `mbx doctor` reported zero failures and warnings. An earlier background invocation resolved to another worktree; its binary was not used.

Backend port: 21986. Worktree Vite port: 21987, with `--strictPort`. The backend used `--instance live-1986 --bind 127.0.0.1 --no-agent-configs`, an isolated HOME, and temporary paths beneath `~/Gits/.tmp/results/live-1986-run/`. Its config directory was beneath that HOME, not Boss's application config directory. The temporary Vite proxy attached this instance's test token to API requests. The first page lacked API authentication and stalled at browser Basic Auth; the authenticated proxy and a fresh owned tab resolved that harness issue.

All browser interaction used the mandated `ab-stealth.sh` wrapper, with named session `live-1986-r2`. No desktop TUICommander instance was launched, restarted or captured. No installed binary was replaced. No request targeted port 9876.

## Real standby and wake observations

Two owned interactive `/bin/bash -l` PTYs ran a foreground `/bin/sh` child which printed `FOREGROUND_CHILD_EXITED` and exited. Setup used the production `sendCommand` helper. This is a real foreground-child exit, **not** a paid agent conversation or independently verified agent-exit transition.

| Lane | Session | Shell PID / PGID |
| --- | --- | --- |
| Desktop browser | `090acc15-84c3-47bf-8195-404eabbabea0` | 78330 |
| Mobile browser | `b47effe3-9e3d-4e74-96b3-308d29369310` | 78348 |

The isolated standby setting was one minute. The browser navigated to `about:blank` and both sessions were marked hidden. No signal was sent manually to manufacture standby.

- **Desktop cycle:** backend logs record both groups entering standby at 05:57:54 UTC. `park-confirmed.json`, sampled at 05:58:09 UTC, records both as `Ts+`. Opening the desktop page at 1400×900 produced backend `SIGCONT` logs at 05:58:11 UTC for the same groups. Raw browser terminal text plus trusted Enter executed `printf 'DESK_%s\n' OK1986`, yielding `DESK_OK1986` without a resend.
- **Separate mobile cycle:** the browser left the page again. `mobile-park-observation.json` records both groups as `Ts+`; the backend records standby at 06:02:54 UTC. Opening only `mobile.html` at 390×844 produced `SIGCONT` at 06:03:02 UTC. Opening the mobile shell and submitting `printf 'MOB2_%s\n' OK1986` through its composer yielded `MOB2_OK1986` without a resend. No desktop page was open during this wake cycle.

The first observer had a 125-second outer bound and ended before standby occurred. It is **not** a passing observation. A later direct OS sample and the backend logs establish the first cycle; the separate mobile observer completed successfully at 127.205 seconds. Watch-release and recent-observation grace can outlast the configured idle interval, so the one-minute setting is not an exact one-minute stop deadline.

## Input latency measurements

These are single local samples after attachment had resumed the shell, not benchmarks or wake-inclusive latency bounds.

| Surface | Start | End | Measured interval |
| --- | --- | --- | --- |
| Desktop browser, 1400×900 | Captured trusted raw Enter `keydown` | Browser fetch observes the new execution result `DESK2_OK1986` in backend terminal output | **21.3 ms** |
| Mobile browser, 390×844, separate standby cycle | Captured trusted Send click | MutationObserver observes `MOB2_OK1986` in the rendered terminal DOM | **370.2 ms** |

The desktop observer used `performance.now()` and an 8 ms retry interval. Its end includes HTTP observation overhead; it does not measure canvas paint. The mobile measurement includes command submission and terminal DOM rendering, but does not measure display presentation or a physical keyboard. The command source does not contain the contiguous result marker, so these markers prove shell execution rather than input echo alone.

Retained `desktop-enter-latency.json` and `mobile-second-latency.json` contain timestamps, trusted-event evidence and output. The earlier `desktop-latency.json` and `desktop-latency-key.json` are invalid timing attempts: the first did not observe text key events, and the second compared against an old baseline. Do not cite their apparent 7.7 ms value. An early desktop retry clicked before tab activation settled and typed into the other owned shell; its expired timing attempt is not evidence. The final desktop measurement selected and confirmed the correct terminal before submission.

## Evidence and acceptance disposition

Evidence root: `~/Gits/.tmp/results/live-1986-run/`.

- Build: `build-own-cwd.log`, its exit receipt, and the binary dependency receipt.
- Standby: `park-confirmed.json`, `mobile-park-observation.json`, `backend.log`.
- Input/output: `desktop-final-output.json`, `mobile-final-output.json`, `browser-transcript.txt`.
- Timing: `desktop-enter-latency.json`, `mobile-second-latency.json`.
- Screenshots: `desktop-after-input.png`, `desktop-final.png`, `mobile-second-after-input.png`.
- Cleanup: `cleanup.json`, `sessions-after-cleanup.json`, `process-cleanup.json`, `browser-cleanup.txt`, `cleanup-ps.txt`.

**AC4 remains unchecked.** This run proves isolated desktop-browser and emulated-mobile recovery after real standby. Both browser surfaces use HTTP, and attachment/visibility resumed the shell before input. It does not directly execute the changed native desktop `write_pty_parts_blocking` boundary, nor does it prove raw input itself woke an already parked shell. The existing real-PTY regression tests remain the evidence for that boundary.

Boss still needs the native desktop check after his chosen backend restart, including actual agent exit, and a check on his physical phone. The emulated mobile result is not silently substituted for either requirement. No story closure is justified by this run.

## Cleanup

Both owned PTYs were deleted; the final session list was empty. The owned browser tab and named daemon were closed; shared Chrome was left running. Backend PID 65820 and Vite PID 65821 received SIGTERM, and the final `ps` check found neither process nor either owned shell. The temporary Vite configuration was removed. No capture corpus, dependencies, production code or application test suites changed or ran.
