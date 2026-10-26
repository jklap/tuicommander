# Story transition history — 2355-bb93

## Contract and implementation

The history API returns persisted command variants, revisions and actors, but no timestamp. The UI translates every current command kind, displays criterion numbers from 1 and appends the current criterion text when present. Unknown command names are humanized without dumping their payload. Live actors use the terminal name; closed actors use eight ID characters with the full ID in a native title tooltip. Human and Local API labels are unchanged.

## Regression evidence

Pre-fix commit: `922ee6c9d7`.

RED command, through the build-slot wrapper:

```sh
scripts/with-test-tmp.sh pnpm vitest run src/__tests__/components/StoriesDialog.metadata.test.tsx -t 'transition history renders human labels'
```

One failure: `Criterion 1 checked: Works` was absent; the DOM contained `{"check_criterion":0}` and the full closed-session UUID. Log: `~/Gits/.tmp/tuic-2355-red.log`.

Final targeted command, through the build-slot wrapper:

```sh
scripts/with-test-tmp.sh pnpm vitest run src/__tests__/components/StoriesDialog.metadata.test.tsx src/__tests__/i18nKeyCollisions.test.ts
```

Result: 2 files, 9 tests passed in 3.42 seconds, after 220 seconds queued. Log: `~/Gits/.tmp/tuic-2355-green-final.log`.

The first final run exposed a test-query error: Human and Local API share a span with the separator. The assertions now include that existing separator; production actor behavior was correct. The existing lowercase `block` assertion was changed to the readable `Blocked` label.

## Isolated desktop evidence

Both builds used `make agent-test AGENT_TEST_INSTANCE=agent-test-2355 AGENT_TEST_PORT=19885` from this linked worktree, after a dry run. Native builds used the target's build-slot and mbx integration. Requests used only the isolated Unix socket. No daily application or Vite server was used. The instance was stopped after capture.

A throwaway project at `~/Gits/.tmp/tuic-2355-demo` held a real plan and story. Nine transitions were committed through the real history service. The two captures show the same persisted story at revision 10:

- `before.png`: window 24587, 2800 × 1800 pixels.
- `after.png`: window 24655, 2800 × 1800 pixels.
- `before-dom.json` and `after-dom.json`: debug replies from the isolated WebView.

Comparison: revision 3 changed from raw `{"check_criterion":0}` to `Criterion 1 checked: Show readable history labels`; revision 9 uses criterion 2 and its text; the other command names changed to readable past-tense labels. All nine after rows fit within the 1400 × 900 logical viewport, from y=335 to y=534, with no horizontal overflow (each scrollWidth equals clientWidth).

Limitation: this agent's tool connection returns screenshot metadata, not image pixels to the model. The PNG files were captured, and rendered text and bounds were compared, but the PNGs were not visually inspected. A coordinator with image viewing must inspect them before claiming the VISUAL criterion fully complete.

The first pre-fix open logged `Frontend unresponsive: no heartbeat for 34s`. One isolated WebView reload recovered it; the subsequent open displayed all nine transitions. No unrelated stall fix is included.

## Scope and risk

Low risk: presentation-only change; no database, protocol, timestamp or lifecycle mutation. All 12 current backend command kinds were inspected. Tests cover both manual-criterion values, missing criterion text, future object/string variants, closed/live managed actors, Human and Local API. Existing stale-response and retry coverage remains.
