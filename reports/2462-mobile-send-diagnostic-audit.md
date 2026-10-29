# Mobile Send diagnostic-only landing audit — #2462-5839

Date: 2026-10-29

## Decision

LANDABLE as temporary diagnostics only. The reported missed Send tap remains
unproven. This audit does not claim a RED test, a fix, regression attribution,
or completion of the original story. It audits diagnostic commit `de4d813aad`
after main merge `99d215beb1`. No production code changed during this audit.

## Recorder and behavior audit

- `src/mobile/components/mobileSendTrace.ts` constructs its own metadata object.
  It does not serialize events, DOM elements, draft text, DOM text, session ids,
  session names, or repository ids. The message contains only a timestamp and a
  process-local press counter, not an application identity.
- The recorder retains at most 24 events per press. It preserves the initial down
  and newest events, counts evictions, and finishes once after a 400 ms settling
  window, a new press, unmount, or a five-second deadline.
- The `DEFERRED (2026-10-29)` comment requires removal after a failing iOS trace
  is obtained and the story closes. The existing `isPerfDebug()` master gate
  controls collection and host mirroring; release defaults remain off.
- Compared with the parent of `de4d813aad`, `CommandInput.tsx` retains the same
  held flag writes, 500 ms Queue timer, cancel handlers, click suppression, and
  Send/Queue submission paths. Added callbacks record metadata only; they do not
  call preventDefault, stop propagation, capture pointers, or mutate those paths.
- `appLogger.ts` adds only a source-and-message-prefix debug mirror allowlist,
  guarded by `isPerfDebug()`. Other debug sources remain local.
- `CommandInput.trace.test.tsx` verifies privacy, short Send, cancelled movement
  followed by a fresh tap, held-click suppression, bounded retention, abandoned
  presses, and dormant diagnostics. Existing Queue tests cover queue delivery.

## Validation

Local Mac commands, each through `tools/build/build-slot.sh`:

```sh
scripts/with-test-tmp.sh pnpm exec vitest run src/mobile/__tests__/CommandInput src/__tests__/stores/appLogger.test.ts
pnpm exec tsc --noEmit
```

Result: 15 files and 135 tests passed in 8.65 seconds; TypeScript exited zero.
The new trace tests and logger mirror tests passed. The run also reports four
async promise leaks in `CommandInput.history-ordering.test.tsx`, including the
pending history write at `CommandInput.tsx:334`; exit status remains zero.
The existing file uses pending-write mocks; its failure test consumes a one-shot
rejection while the prior mock implementation can still return a pending write.
The four reports point to that history path, not the new recorder. These warnings
are not a missed-Send reproduction or a clean leak check. No test or production code was weakened to hide them.
Existing logger ring tests print their fixture messages; the failed-history-write
test prints its expected error. Full output is retained at
`~/Gits/.tmp/results/tuic-2462-audit/checks.log`.

Instruction-link validation passed (two pairs); `git diff --check` passed.
No Rust build, full suite, desktop launch, or new touch reproduction was run.

## Last iOS tooling attempt

Full command output: `~/Gits/.tmp/results/tuic-2462-audit/ios-tooling.txt`.

`xcrun --find simctl` returns:

```text
/Applications/Xcode.app/Contents/Developer/usr/bin/simctl
```

`xcrun simctl help io` lists enumerate, poll, recordVideo, screenshot and
screenConfig, but no tap or touch injection. `xcrun simctl help ui` lists only
appearance, increase_contrast and content_size. General help has no HID/tap
command. PATH checks return:

```text
idb: not found on PATH
idb_companion: not found on PATH
applesimutils: not found on PATH
/opt/homebrew/bin/cliclick
maestro: not found on PATH
ios-deploy: not found on PATH
tidevice: not found on PATH
```

`cliclick -h` documents macOS screen-coordinate mouse/keyboard actions, not a
simulator HID transport. It needs a Simulator window for UI interaction.
`open -Ra Simulator` returns exactly:

```text
Unable to find application named 'Simulator'
```

The selected developer directory is
`/Applications/Xcode.app/Contents/Developer`; its `Applications/Simulator.app`
is absent. `xcrun simctl list devices booted` lists no booted devices. No usable
installed route to a real iOS Send event sequence was found in these checks.
No software was installed, no simulator was booted, and no global clicks were
injected into Boss's desktop.

## Collect the next real-phone trace

After the diagnostic build reaches the phone, enable collection on that phone's
page in Web Inspector, then reload:

```js
localStorage.setItem("tuic.perfDebug", "true")
```

The desktop setting does not enable the phone. Use the terminal composer
normally. After a Send tap, retrieve the host diagnostic ring:

```sh
curl -sS 'http://localhost:9876/logs?level=debug&source=mobile-send&audience=diagnostic&limit=100'
```

Look for `Mobile Send press: <timestamp>/<counter>`. Preserve the complete
entry, including `data_json`, not just the message. Compare `events`, `held`,
`pressTimer`, `sendCalled`, `down`, `click`, `reason`, and `droppedEvents` for a
missed tap and a successful tap. `sendCalled` means the function was entered,
not that backend delivery succeeded. A deadline, truncation, or the end of the
400 ms settling window cannot prove that no later click happened. A tap that
never reaches Send pointerdown cannot create a trace.

Set `tuic.perfDebug` to `"false"` and reload to disable collection. Keep original
REALITY, RED and behavior-fix criteria open until real failing evidence exists.
