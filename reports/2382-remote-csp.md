# Remote terminal CSP fix — 2382-97aa / issue #147

## Contract and scope

A macOS desktop terminal on a connected remote repository must receive remote shell output and submit input. The SSH runtime chooses `http://127.0.0.1:<dynamic port>`; Direct retains the configured HTTP(S) host and port. Both terminal transports replace HTTP(S) with WS(S).

The fix adds only `ws:` and `wss:` to the existing permissive `default-src`. There is no new `connect-src`: it inherits `default-src`. A loopback-only source would exclude supported Direct HTTP connections. A fixed port would exclude dynamic SSH tunnels. Existing iframe sources and the Tauri script/style hash override stay unchanged.

Local desktop grid streams use Tauri channels, and local desktop PTY subscriptions use Tauri events. Browser/mobile streams use their HTTP(S) page origin; their static HTML does not receive the desktop Tauri CSP from `tauri.conf.json`.

## Focused RED and GREEN

Pre-fix commit: `b36673fbe55af0033b7de7ccdb1333203aeeb595`.

- RED: `build-slot.sh bash scripts/with-test-tmp.sh pnpm exec vitest run src/__tests__/csp-policy.test.ts -t remote_terminal_websocket_schemes_are_explicitly_allowed` failed because the shipped fallback source list lacked `ws:` and `wss:`.
- GREEN: the same command without the test-name filter passed both tests in `src/__tests__/csp-policy.test.ts` (532 ms). This includes the existing no-per-directive-restrictions and no-script/style-hash-injection guard.
- No Rust source or Rust tests changed. No Rust unit suite, full frontend suite, mutation run, or Direct TLS live check was run.

## Native SSH evidence — 2026-10-27

- Native worktree build: `make agent-test AGENT_TEST_INSTANCE=agent-test-2382 AGENT_TEST_PORT=19982`, after its dry run. Native Cargo ran through the harness build-slot/mbx path. The WebView used `tauri://localhost`, not Vite or a browser page.
- Disposable rb daemon: published `v2.0.0-beta.3`, Linux aarch64, SHA-256 `66dc80f45a9fd8e290c55b11285d30e633add20b4d82d9848b635b6eb395aa12`, bound to `127.0.0.1:19983`. Its HOME, temp files and throwaway repository were under `/home/rb/Gits/.tmp/tuic-2382`.
- The production SSH connection manager selected local tunnel port `58060`. The native UI opened a terminal on `/home/rb/Gits/.tmp/tuic-2382/repo`.
- Runtime observation of the actual WebSocket constructor recorded both the raw PTY socket and `?format=grid&compress=deflate`. Both opened, received messages and recorded zero socket error events. Tokens were removed before evidence serialization.
- Input was entered in the real Compose editor and submitted with its Send button, which uses the production command path. `printf 'CSP2382_SSH_%s\n' VERIFIED; uname -s; hostname` returned `CSP2382_SSH_VERIFIED`, `Linux`, and `lnx-arm64`.
- Observation of the real canvas `fillText` calls recorded those returned strings. The matching remote output API independently retained them. No synthetic terminal output was injected.
- Window-only screenshot: owned native PID `32039`, window ID `27351`, 2800 × 1800 pixels. A second window-only MCP capture confirmed the same 1400 × 900 logical-point test window. No whole-screen capture was taken.
- The window was initially minimized; unminimizing only the test window allowed its terminal to initialize. No show/setFocus override or daily-app interaction was used.

Retained evidence: `~/Gits/.tmp/results/tuic-2382-live/`, including RED/GREEN logs, sanitized socket/canvas observations, `remote-output.json`, and `2382-native-ssh-verified.png`.

Both throwaway PTYs were deleted, the SSH connection disconnected, the native supervisor stopped through `make agent-test-stop`, and owned rb daemon PID `2726735` terminated. The daily app and its saved connections were not used. This compiled CSP change requires a rebuilt/restarted desktop; it does not change the already running app.

## Limits and unrelated observations

Direct HTTPS/WSS is covered by the explicit scheme contract but was not exercised against a TLS daemon. Mobile phone, disconnect UI, compression negotiation and daemon update checks remain outside this evidence. The isolated desktop logged missing notification permission and unavailable `get_dictation_config` at startup; neither error prevented the SSH terminal check. Risk is medium because the compiled desktop CSP is shared by all remote transports.

## Draft issue reply — publish only after the fix ships

Thank you for the detailed report and for tracing the failure to the CSP. Your diagnosis was correct: the macOS desktop policy did not explicitly allow WebSocket connections, so remote terminals could fail even though HTTP requests and PTY creation succeeded.

The fix adds `ws:` and `wss:` to the existing policy, covering SSH tunnel streams and Direct HTTP/HTTPS connections. We verified a native macOS desktop terminal against a separate Linux daemon over SSH, including command input and rendered shell output, and added a regression test for the shipped policy.

This fix will be included in the next release. Please update the desktop app when that release is available and restart it. Thank you again for the careful diagnosis.
