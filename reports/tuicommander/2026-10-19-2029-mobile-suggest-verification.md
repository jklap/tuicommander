# Mobile suggestion dismissal: verification follow-up

Story: #2029-994d. Date: 2026-10-19.

## Implementation status

The fix is already in this checkout's ancestry through merge `3aabcfdb2f`.
No production or test change is needed for the reported cause.

At pre-fix `fc9b3ccdcd`, `src/mobile/components/SuggestChips.tsx:18-31`
only sends the command and logs a failure. Lines 34-43 always render the
incoming items. A tap never changes the rendered list.

The current component sets `picked` before the first asynchronous write
(lines 24-33), gates the list on that state (line 48), and resets it only
when the session or item content changes (lines 19-23). General activity
timestamps cannot restore a consumed list. Failed writes show the existing
mobile error toast (lines 40-43). Desktop remains unchanged:
`src/components/TerminalArea.tsx:54` dismisses before sending at line 61.

The implementation and relevant mobile/desktop suggest tests have no diff
between reviewed commit `36f48554b7` and this follow-up's starting HEAD
`9fce6e2de5`.

## Existing test evidence

These runs are retained evidence from the original implementation and critic,
not new test runs in this follow-up.

- RED on `fc9b3ccdcd`: `build-slot.sh scripts/with-test-tmp.sh pnpm vitest run src/mobile/__tests__/SuggestChips.test.tsx -t "consumes a picked list"`.
  The assertion expected zero buttons but got two. The initial failing test
  also reported five async leaks; the final test drains the pending send in
  `finally`. Log: `/Users/stefano.straus/Gits/.tmp/fix-2029-red.log`.
- GREEN on reviewed `36f48554b7`: `build-slot.sh scripts/with-test-tmp.sh pnpm vitest run src/mobile/__tests__/SuggestChips.test.tsx src/mobile/__tests__/SessionHeader.test.tsx src/__tests__/components/SuggestOverlay.test.tsx`.
  Result: 25 tests passed in 3 files, 3.86 seconds, exit 0. Two desktop store
  debug lines are present. Log: `/Users/stefano.straus/Gits/.tmp/critic-2029b-vitest.log`.

No new Vitest, standalone tsc, Rust, or full-suite run was required for this
report-only change. The worktree Vite checker reported `No errors` during the
browser probe; this is not a standalone tsc result.

## Real-backend probe and remaining limit

The named agent-browser session `fix2029-oct19` loaded this worktree's
`mobile.html` through Vite on port 1529. API paths proxied to the real backend
on port 9876. The DOM measured **390 by 844 CSS pixels**. The bottom navigation
occupied y=789 through y=844, with five 78-pixel-wide buttons. The DOM showed
Sessions, Chat, Files, Progress, and Activity, but no session or suggestion.

The browser's `/sessions` fetch did not settle within an eight-second bound.
A direct unauthenticated `GET http://127.0.0.1:9876/sessions` returned HTTP 401.
No credentials or token store were read. No live session was changed or
created. This probe does **not** prove tap dismissal or next-poll persistence.
The real-backend SYMPTOM criterion remains unchecked.

Evidence directory: `/Users/stefano.straus/Gits/.tmp/results/2029-994d/`:

- `browser.txt`: commands, DOM measurements, request bound, and cleanup.
- `mobile.png`: session-list shell capture, not a before/after symptom capture.
- `vite.txt`: worktree server identity and checker output.
- `run.log`: background probe completion.

The coordinator must review pixels; this agent used DOM measurements only.
The owned Vite process, named browser tab, and named agent-browser daemon
were stopped. The shared Chrome host and other agents' tabs were not stopped.

## Required next step

Use an authenticated browser session against the real backend, or supply an
approved isolated headless backend with a disposable session. Emit real
suggestions, tap one, and inspect the list immediately and after the next
poll. Do not close the story until that criterion has evidence.
