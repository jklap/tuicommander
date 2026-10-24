# Mobile dictation verification — 2026-10-20

Story: `2020-66aa`. Risk: low (test and evidence only; no production change).

## Keyboard-visible failure

- Worktree Vite: `http://127.0.0.1:15470/mobile.html`.
- Agent-browser through `ab-stealth.sh`, named session `desk-2020-visual`.
- Browser viewport: 390 × 844 CSS pixels. Focused element: `TEXTAREA`.
- The test overrides `visualViewport.height` to 500 and dispatches its resize event. The real MobileApp handler sets `--app-height: 500px`.
- A labelled test-only overlay reserves y=500 through y=844 for the emulated keyboard area. This is desktop Chrome, **not an iPhone keyboard screenshot**.
- A page-local SpeechRecognition stub emits `no-speech`; abort completes through a queued end callback. Session/config HTTP responses are page-local stubs. No backend, microphone, live terminal or desktop application is used.
- The existing `Dictation failed` status bounds are x=233.6953125, y=469, width=85.3046875, height=14. Its bottom is 483, above the keyboard boundary at 500.
- The mic has `aria-pressed="false"` and `data-failed="true"`. The unchanged status text is visible in the accessibility tree. No application wording or styles were added.
- A second mic click and result insert `retry succeeded`; the status disappears. No Send click or Enter is used, and the recorded request list has no write/reply request.

Evidence:

- [Failure at 390 × 844](failure-390x844.png)
- [Recovered at 390 × 844](recovered-390x844.png)
- [Wrapper commands, DOM bounds and accessibility snapshots](browser-transcript.txt)

The Vite process, named tab and its agent-browser daemon were stopped. No headless backend was started.

## Transcript-oracle regression

The existing `finishes a failed %s take before inserting the next transcript` test now asserts the observable second transcript rather than failing first on an abort spy. The synthetic service completes shutdown after abort. The test clicks the actual mic control even when stale ownership leaves it labelled Stop, then delivers the next transcript. It does not send manually.

Historical production source was exported with `git archive a4c3722f src public scripts package.json pnpm-lock.yaml pnpm-workspace.yaml vitest.config.ts tsconfig.json` to a disposable directory, and only the updated regression test was copied into it. Dependencies were installed offline with the historical frozen lockfile. No branch was switched and no production source in this worktree was replaced.

Both commands ran locally through `/Users/stefano.straus/Gits/personal/orchestrator/tools/build/build-slot.sh`:

```sh
scripts/with-test-tmp.sh pnpm vitest run src/mobile/__tests__/BrowserDictation.repeat.test.tsx -t 'finishes a failed'
scripts/with-test-tmp.sh pnpm vitest run src/mobile/__tests__/BrowserDictation.repeat.test.tsx
```

- **RED:** pre-fix `a4c3722f47eaf74ac488a70ba7c28a2b8c7a2c3c`, three failures (aborted, network, no-speech). Each fails at the composer oracle: expected `retry succeeded`, received an empty string. Exit 1; duration 1.14 s. [Log](red.txt).
- **GREEN:** current main-derived worktree `65c4062d71`, with the test change. Production code is unchanged and contains landed `f8610e9a` and `1d982198`. All 12 tests in the repeat file pass, including empty end, draft cancellation, repeated takes, synchronous start failure and wake-lock boundaries. Exit 0; duration 1.36 s. [Log](green.txt). Existing lifecycle debug output is retained in the log.
- This is not a full frontend suite, Rust check, real speech-service test or physical iOS test.

## Cause evidence and remaining limit

At pre-fix `a4c3722f`, `useBrowserDictation.ts:162-164` sets `discarding=true` on error without aborting. `cancel` at lines 112-117 then returns for a discarding recognizer, and `onend` at lines 166-168 is the release path. If the service does not end on its own, recognition ownership survives and the next insertion is ignored.

Current `src/mobile/useBrowserDictation.ts:230-237` calls cancel on error. Cancel at lines 150-160 requests abort but retains ownership until end; lines 239-242 release it. Toggle at lines 168-177 refuses overlapping starts. This protects the synthetic lifecycle in the regression, but it cannot prove that WebKit emits end after abort on the affected phone.

**The manual-send causal claim remains unproved.** `src/mobile/components/CommandInput.tsx:333-339` calls that same cancel function on send. Cancel returns while discarding, so send cannot directly release the already-discarding recognizer. Empty recognition followed by end already releases normally. Do not close criterion 1 from this evidence.

The next real iPhone failure must supply the deployed client identity and bounded lifecycle records grouped by `takeId`, ordered by `sequence`: start, first-result, error, abort-requested, end, ownership-release. It must show which event/state is missing and what changes around the manual send. The screenshot satisfies the assignment's explicitly permitted visualViewport emulation only; it does not prove physical keyboard or WebKit behavior.
