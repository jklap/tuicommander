# AI Chat session errors — 2383-f1a1

## Cause and scope

The missing-session refusal comes from the ACP agent. `acp/connection.rs::send` preserves its JSON-RPC error as `agent_error` and stamps the operation. TUIC applied that refusal to the wrong consumer in two frontend paths:

- Daily log entries 104077, 104081 and 104139 report `listing conversations failed`, operation `list`, session id null. Optional catalogue discovery used the same fatal guard as attachment and prompt failures. A pending list also held startup on **Connecting conversation**. The recorded error fixture and its provenance are under `src/__tests__/fixtures/ego-missing-catalogue-2383.*`.
- Background replay of a different saved tab wrote its refusal into the selected conversation's error signal. The isolated instance reproduced this with real missing-session load errors (first-run log ids 40 and 60).

The listing path was introduced in `dacc30f176` (durable conversation restoration). The background-tab error path was introduced in `94d8b8b0b3` (parallel chat tabs). These are code-history introductions, not a claim that the exact missing record was present at those commits. The original missing durable record's provenance is unknown. Story 2348 fixed selected stale-session recovery but did not isolate catalogue or background-tab failures.

## Change

- Load the optional catalogue after attachment, without holding startup or prompting.
- Keep catalogue failure separate from fatal conversation error, with a visible warning and a list-only retry on desktop and mobile.
- Reject stale catalogue results after a newer request or connection takes ownership.
- Show a replay refusal only for the selected connection/session. Keep background refusals cached for later selection.

No ego source, installed binary, daily application configuration, or daily session was changed. No durable conversation was deleted to hide an error.

## RED and targeted validation

Pre-fix commit: `606ad696b6996cb40a55963619c2db80b298372e`.

The first RED command was run through `build-slot.sh` and `scripts/with-test-tmp.sh`, with this Vitest filter in `AIChatPanel.test.tsx`:

```text
conversation usable when ego catalogue|before its catalogue request settles
```

All three cases failed: new and restored chats held the recorded missing-session error; a pending catalogue left Connecting visible. Duration: 6.93 s. Log: `~/Gits/.tmp/2383-red.log`.

The separate background-tab regression, `keeps the selected conversation live when a background saved tab is missing`, failed against the original replay logic with the catalogue-only patch in place. Duration: 3.82 s. Log: `~/Gits/.tmp/2383-background-red.log`. The original replay code is unchanged from the pre-fix commit at that point.

Final validation on the committed production implementation (`55989c87e`):

```text
build-slot.sh bash -lc 'scripts/with-test-tmp.sh pnpm exec vitest run src/__tests__/components/AIChatPanel.test.tsx src/mobile/__tests__/MobileChatScreen.test.tsx src/mobile/__tests__/MobileChatLinks.test.tsx src/mobile/__tests__/MobileUpdateBanner.test.tsx && scripts/with-test-tmp.sh pnpm exec tsc --noEmit'
```

Result: 229 tests passed in 9.37 s; TypeScript passed. Log: `~/Gits/.tmp/2383-final-live.log`. The earlier catalogue-only run passed 228 tests. No existing assertions were weakened or removed. Formatting and instruction-link checks passed. There are no Rust production changes and no cross-target test claim.

## Native evidence

The final code ran through `make agent-test AGENT_TEST_INSTANCE=agent-test-2383 AGENT_TEST_PORT=19883`, after its dry run. Native compilation used the target's mbx/build-slot path and passed in 49.42 s. Ego used a separate home and a narrowed scratch workspace under `~/Gits/.tmp/2383-live/`. The authorized OpenRouter key was loaded into the process environment only, never written or printed.

- New session `471c0555-5161-52cf-a34b-f455de003b14` answered the harmless prompt with `FINAL_RESTORED_OK`. The prompt's label does not make it a restore: this is the new-session case.
- Earlier session `58dfa8bf-bd2a-5159-9972-768ee557d518` first answered `NEW_OK`. After the native backend restart, the final connection loaded it and answered `RESTORED_OK`. Its durable event log proves that the reply belongs to that same restored session, not a replacement.
- The final snapshot records both attachments idle with settled turns. Background missing-tab refusals did not leave a fatal banner on the selected chat.

Evidence in `~/Gits/.tmp/2383-live/`:

- `turn-evidence.json`: filtered user prompts and assistant replies from the two task-owned durable logs.
- `final-connection.json`, `final-acp-log.json`: attachment state and the real load request.
- `final-new-ui.json`, `final-restored-ui.json`: rendered native text and measured bounds.
- `ai-chat-restored.png`: 1000×1608 AI Chat-only crop. It came only from window 27648, owned by the recorded final test PID 80552. No whole-display capture was used.

`~/Gits/.tmp/2383-ocr.log` confirms the image contains NEW_OK, RESTORED_OK, and the model/mode summary. The named native instance was stopped after capture. Initial-run stop exit 143 was the requested supervisor stop, not a validation failure.

## Risk and remaining ownership

Risk: medium. Desktop and mobile share asynchronous attachment and catalogue state. The coordinator owns the separate critic and integration gate.

Out of scope: the native conversation select's DOM value reported another attached session while the transcript and actual prompt correctly used the restored session. `final-restored-ui.json`, `final-connection.json`, and `turn-evidence.json` retain the mismatch. The controls owner, tuic-2385, was informed. This did not change the proven prompt destination and was not silently patched in that owner's files.
