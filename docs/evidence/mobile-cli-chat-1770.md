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


## Fresh browser evidence — 2026-10-18

The night-mobile audit captured the existing presentation through the mandated
`ab-stealth.sh` wrapper, in its own named tab against this worktree's Vite on
`127.0.0.1:15970`. The viewport was 390 × 844. No desktop instance or production
configuration was changed.

- [Mobile, Commander](mobile-cli-chat-1770/mobile-dark.png)
- [Mobile, light](mobile-cli-chat-1770/mobile-light.png)
- [Desktop presentation at phone width](mobile-cli-chat-1770/desktop-narrow-dark.png)
- [Browser command transcript](mobile-cli-chat-1770/browser-transcript.txt)

The production appearance fixture mounts the shared Transcript, real CLI/Chat
switch and real composer. DOM evidence confirms the header, both mode buttons,
user prompt, folded tool activity, answer, quote, list and code block were
present. Both measured presentations had viewport width 390 and document scroll
width 390: no document-level horizontal overflow. These are synthetic appearance
entries, not live-agent transport evidence or a phone benchmark. The screenshot
files were captured and their PNG dimensions verified; this agent's available
file reader returns raw image bytes, so pixel-level visual inspection still
needs an image-capable reviewer before the visual criterion is checked.

The first navigation timed out while Vite scanned dependencies. The bounded
retry limited dependency scanning to this fixture and waited for its header;
it exited successfully and captured all three images. The named tab, its
agent-browser daemon and the owned Vite process were stopped. Detailed failed
and successful harness logs remain under `~/Gits/.tmp/results/nw-mobile-visual*`.

The 2,000-entry first-open/resume phone measurement remains open. No application
code or tests changed, and no extra application suite was run for this audit.
