# Mobile dictation error surfaces — 2026-10-27

Story: `2399-e2ae`. Base: `bd3b0a34fcf14d588a649b011b0b6d92b6ee5178`.

## Findings

### Browser microphone capture: cause not yet attributed

The composer uses `SpeechRecognition` or `webkitSpeechRecognition` directly.
It does not acquire a `getUserMedia` stream or invoke desktop speech recognition.
The shared `Button` forwards its click handler to one native button; focus does
not start recognition. `useBrowserDictation` claims the recognizer before
`start()` and retains ownership until `end`, including during error shutdown.
The separate hands-free control has its own recording/arming exclusion.

The text “No microphone found, or it is in use” is the application's generic
mapping for `audio-capture`, not the result of device enumeration. The browser
error does not identify a missing device, a permission denial or another client
holding the microphone. No `not-allowed` error is present in the retained events.

Read-only evidence came from TUIC `debug action=logs source=dictation limit=100`.
The matching viewed session is `b104b7f1-78c7-4ecf-ad6a-da6d252db246`.
The table preserves the source's millisecond timestamps and per-take order;
server arrival order is not callback order.

| Take ID suffix | Event | Timestamp (ms) | Source record |
|---|---|---|---|
| `bf477e6d-d1dc-4877-bb57-9be79adec367` | start | 1793128660078 | 3412 |
| same | first-result | 1793128665218 | 3415 |
| same | end and ownership-release | 1793128666613 | 3418, 3417 |
| `32252410-854c-484d-93fe-9cdf1edb8688` | start | 1793128671878 | 3419 |
| same | error: audio-capture | 1793128677714 | 3420 |
| same | abort-requested | 1793128677802 | 3422 |
| same | end and ownership-release | 1793128677939 | 3424, 3423 |
| `a3cf414a-3574-4603-acea-31e9c5e76905` | start | 1793128748078 | 3506 |
| same | error: aborted | 1793128749027 | 3508 |
| same | end and ownership-release | 1793128749027 | 3509, 3510 |
| `59010694-899d-4e1b-be49-a66f036065a2` | start | 1793128750713 | 3517 |
| same | error: aborted | 1793128751460 | 3522 |
| same | end and ownership-release | 1793128751452 | 3520, 3521 |

Each failure has its own take ID. Each previous take released ownership before
the next start. The last take's error has sequence 2 and end has sequence 4,
although their upload timestamps arrive in the opposite order. This is not
evidence of two starts from one tap or two failure callbacks toasted in one take.
The browser logs do not identify the physical client, so they are not a recording
of the iPad's OS microphone state.

A first-result in the same deployment also prevents an honest “never worked”
conclusion. **The physical capture failure's root cause and regression commit
remain unknown.** No speculative microphone reroute, Safari mode change, retry,
new timeout or desktop adapter change is included. Boss must retry on the iPad
and retain the loaded client identity plus a fresh callback sequence if it fails.

### Multiple toasts: separate retained failures, not a proven duplicate

`toastsStore` makes error cards sticky until dismissal. Distinct failures can
therefore remain on screen together, even when they occurred in separate takes.
The capture error and later aborted errors in these logs are separate failures.
The existing hook suppresses an abort follow-up error while discarding a take.
The focused pre-fix control test confirms one toast for audio-capture followed
by aborted within that take. No duplicate-toast code change is justified by this
evidence. This test remains as a guard against a future regression.

### Persistent composer label: reproduced and fixed

`f8610e9a0086f213ac515a31266e57435bbbda62` introduced a `failed` signal and a
second error label which stayed until the next take. The toast already reported
the failure. `a46e7ddd72b4a7e88ece50e99d7ecdbdce96aa5b` moved that label out of
the button and placed it above the composer, explaining the reported location.

The fix removes the duplicate label, its failure-only colour, and the unused
`failed` signal. Recording retains its existing red state. Failure detail and
lifecycle diagnostics remain unchanged. The Ideas input shares this control,
so its test now requires the toast rather than the rejected duplicate label.

## Regression evidence

Only the requested pre-fix RED run was executed, locally through the scheduler:

```sh
build-slot.sh scripts/with-test-tmp.sh pnpm vitest run \
  src/mobile/__tests__/BrowserDictation.errors.test.tsx \
  -t 'starts once when|reports one failure when|keeps a failure in the toast'
```

Base `bd3b0a34fc`, exit 1, duration 3.60 s:

- `starts once when the mic receives focus and one activation`: passed.
- `reports one failure when audio capture fails and shutdown reports aborted`: passed.
- `keeps a failure in the toast without retaining a composer failure label`:
  failed with an actual `span role=status` containing `Dictation failed`, where
  the contract requires no duplicate label.

See [RED output](red.txt). The first two tests are controls, not reproduced bugs.
The existing lifecycle driver simulates the Web Speech boundary; it is not an
acoustic or physical WebKit recording. Expected console output is captured in
the final test source. No GREEN run was performed, per managed batch policy.
The integration run must cover the changed `BrowserDictation.errors.test.tsx`
and `IdeasOverlay.dictation.test.tsx`; existing repeat/voice-control tests cover
unchanged lifecycle consumers.

## Browser rendering probe and limits

A temporary Vite page rendered the real `CommandInput` and `ToastList` at
768 × 1024 using the existing style sheets. A synthetic recognition driver
emitted audio-capture, then end on abort. No real microphone or backend was used.
[Captured DOM state](visual-state.json) reports zero failure status labels,
one toast, `aria-pressed=false`, and no `data-failed` marker after shutdown.

**Screenshot verification is incomplete.** The bounded headless Chrome CLI
capture timed out after 35 seconds. A second CDP probe obtained the DOM state,
but `Page.captureScreenshot` did not return before its 30-second bound.
No screenshot was produced. All owned browser/server processes were terminated.
This DOM result does not replace a visual check or Boss's physical iPad retest.
