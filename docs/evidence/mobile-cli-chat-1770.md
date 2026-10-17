# Mobile Claude CLI/Chat audit — 2026-10-17

Story: #1770-3e28 (raised to P1 in the main story store).

## Existing implementation

The requested feature is already on main. Do not add a second switch or transcript renderer.

- `src/mobile/screens/SessionDetailScreen.tsx:269` mounts the CLI/Chat switch in the session header.
- `src/mobile/components/useSessionChatView.tsx:22` enables it for an existing Claude session. The backend owns transcript availability; a frontend agent-session ID is not required.
- `src/mobile/components/useSessionChatView.tsx:57` mounts the shared `TerminalChatTranscript` with the mobile host hint and existing wake subscription.
- `src/mobile/useSessions.ts:195` handles `chat-view-changed` in the existing session event subscription.
- `src/mobile/screens/SessionDetailScreen.tsx:488` keeps terminal output mounted and hides it while Chat is active.
- `src/mobile/mobile.css:1` and `src/global.css:1` import the same Chat presentation tokens. This is the landed #1974-e6ae change (merge `546ee325f9`).

This lane synchronizes the existing implementation from main. It changes no application behavior or test assertion.

## Validation and limits

- Instruction-link validation passed: two instruction pairs.
- Changed-file formatting check against `main...HEAD` passed with no files listed.
- No RED or GREEN tests ran in this audit. No production defect or new feature implementation required a test. The batch integrator owns GREEN validation under the managed build policy.
- Attempted the existing rendering-only fixture at `/src/__tests__/browser/sessionChatAppearance.html?mobile`, dark and light, with a 390×844 headless Chrome window and an 8-second virtual-time budget. Both attempts timed out at the 18-second outer bound. No screenshot was produced. Logs: `~/Gits/.tmp/tuic-1770-visual/`.
- The fixture uses synthetic transcript entries. It does not prove live-agent transport behavior or phone performance.
- Process-group cleanup returned `PermissionError: [Errno 1] Operation not permitted`. A subsequent read-only process listing found no matching Vite or fixture Chrome processes. No alternate termination mechanism was used.
- Story-close preflight refused the unmeasured phone first-open/resume criterion and historical test-change metadata. The new 390×844 visual criterion also remains unchecked. Do not close the story based on this audit.

## Remaining evidence

Capture and inspect the mobile session Chat at 390×844. Measure first open and background/resume with 2,000 entries on a real phone, including device, browser, network and elapsed times. Prior synthetic Chromium timings are not phone measurements.
