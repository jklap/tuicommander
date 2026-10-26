# Mobile AI Chat session binding — 2348-8927

## Result and boundary

The TUIC attachment defect is fixed. The targeted frontend checks pass. The isolated browser recovered the stale tab to a new attached ego session. The full live criterion is **blocked by an independent ego perimeter error**, so this story is not complete.

No ego source, daily TUIC process, daily configuration, GitHub state, or installed binary was changed. The named `agent-test-2348` instance used port 19878. Its ego wrapper used a separate EGO_HOME under `~/Gits/.tmp/2348-ego/home`.

## Evidence and causal path

- The daily log `~/Library/Application Support/com.tuic.commander/logs/tuic.log.2026-10-26:26058` records at `2026-10-26T06:51:02.542027Z`: connection `01a176f9-f5fe-7796-b88f-421e5e856d9e`, method `session/load`, session `0b466823-0736-5dcc-ad6e-fb87430c21ce`.
- The host configuration's saved workspace session is a different id. The reported id has no directory in the default `~/.ego/sessions` store. Ego's `store_root` and `JsonlStore::session_dir` establish that directory layout. This does not prove when or why the old session disappeared.
- `createAcpChat` restores the selected tab from browser-local `tuic-ai-chat-tabs`. On remount, its effect combined that selection with the module's existing connection. `pair()` checked only the two ids. With no root restored on the new hook, the background replay did not run. A send therefore reached the backend with no attachment and produced `AcpClientError::not_attached`.
- A second route also committed `binding.sessionId` before `session/load` succeeded. After a refused load, the next send trusted that binding and could prompt the unattached session.
- Explicit recovery attempted `session/load` for the same id. In the isolated instance, loading that missing id through the real TUIC manager and installed ego 0.2.0 produced the exact recorded fixture `src/__tests__/fixtures/ego-missing-session-2348.json`: `agent_error`, operation `load`, message `Resource not found: "session not found"`, retryable false.

The code paths and regression tests prove both invalid-pair routes. The daily log proves which id mobile requested for load. **The initial source of that id is not captured**: stale mobile localStorage is consistent with the evidence, but a session query parameter can also select it. Desktop AI Chat state remains unknown. The supplied PNG files could not be inspected as images with this agent's text-only file reader; no visual claim about those originals is made.

## Change

- Restore the workspace root with the module binding on remount.
- Require a backend attachment before an action can use a connection/session pair.
- Do not commit a saved session as the binding until its load succeeds.
- On the exact recorded missing-session load refusal, create a replacement on the same connection, remove only the stale local tab, and persist the successful replacement. Other admission, trust and I/O refusals do not create a replacement or trigger an automatic retry loop.
- Reuse that recovery for initial send, explicit selection and recovery replay.
- Keep test client fakes faithful to `acpClient.newSession`: successful creation refreshes the full attachment snapshot. The pending-load tests now explicitly model dropped attachments. Existing assertions remain unchanged.

## RED

Pre-fix commit: `1153c5b43d15ad38496f245535e6d9ba5259e0e2`.

Through `build-slot.sh`, after offline frozen-lockfile installation:

```text
scripts/with-test-tmp.sh pnpm exec vitest run src/__tests__/components/AIChatPanel.test.tsx -t "never sends to a saved conversation after its attachment was refused|attaches the selected saved tab after remount before sending"
```

Both tests failed in 3.63 seconds. The second send called prompt once on the unattached id. The remount called load zero times. Full output: `~/Gits/.tmp/2348-red.log`.

## GREEN and static checks

The final targeted run through `build-slot.sh`:

```text
scripts/with-test-tmp.sh pnpm exec vitest run src/__tests__/components/AIChatPanel.test.tsx src/mobile/__tests__/MobileChatScreen.test.tsx
scripts/with-test-tmp.sh pnpm exec tsc --noEmit
pnpm exec vite build
```

216 tests passed in 9.82 seconds. TypeScript and the frontend build passed. Output: `~/Gits/.tmp/2348-validated.log`. Two preceding targeted attempts exposed test-boundary inconsistencies; their outputs are retained in `2348-green.log` and `2348-green-final.log`. They were diagnosed and corrected, not hidden.

Biome, `git diff --check`, and the instruction-link check passed. No Rust source changed; no Rust test or cross-target test result is claimed. The native agent-test build used the repository's mbx/build-slot path.

## Isolated live check and blocker

The browser procedure used the stealth wrapper with named session `ego-2348`, a separate orphan-scan directory, and the named agent-test backend. A seeded browser-local stale tab used the reported id. Sending a harmless request produced:

- a new backend-confirmed attachment `2936b192-92b0-53e8-99ec-49aa3b780e62` on connection `01a17705-f09c-748b-9ada-a8a98617986e`;
- removal of the old id from browser-local tabs;
- no not-attached or missing-session error in the recovered view;
- a separate ego failure before a successful turn:

```text
Internal error: "process backend failed: perimeter-probe-temp-overlap: TMPDIR /Users/stefano.straus/Gits/.tmp/tuic-at/736b4a02bd577529 is inside granted root /Users/stefano.straus/Gits; the negative filesystem probe cannot be proven"
```

Model and Mode remained pending; the backend attachment had an empty configOptions array. Per the assignment, ego was not changed and its perimeter check was not bypassed. A successful prompt and model/mode publication remain unverified.

Evidence: `~/Gits/.tmp/2348-browser/backend-after-open.json`, `live-after.json`, and `recovered-ego-blocker.png`. Browser prerequisites passed with evidence in `~/Gits/.tmp/ego-browser-2348-smoke/`. The browser tab and named browser daemon were closed, and agent-test stop was requested.

## Risk and remaining work

Risk: **medium** — shared default binding, asynchronous replay and desktop/mobile consumers. The coordinator must run the separate critic on the committed branch. The missing-session classifier deliberately accepts only the recorded ego load refusal; a future wire-message change fails closed rather than replacing a conversation on an unrelated error.

Complete the live criterion after the ego/agent-test perimeter prerequisite is resolved by its owner. Do not mark this story complete from the targeted tests alone.
