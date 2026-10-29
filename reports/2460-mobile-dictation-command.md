# Mobile dictation and slash-command transcript — 2460-5a96

## Reality and limits

Boss reported a plain dictated sentence displayed as a `/wiz:work` command in IMG_6329. The follow-up IMG_6330 shows the same blue slash key in CLI mode. The key does not implement a selected modifier.

The original iPhone input events and the terminal draft at that time were not captured. The reproduction below proves a concrete path to the symptom, not the historical origin of that draft. It does not establish whether a previous command selection, a desktop edit, or another writer left the command prefix.

## Defect A: hidden terminal draft survives the first mobile edit

- `src/mobile/components/TerminalKeybar.tsx:121-124,170-177`: slash invokes a callback only on a tap; its accent class is unconditional. There is no slash toggle state.
- `src/mobile/screens/SessionDetailScreen.tsx:642-663`: the key inserts through the shared composer; Chat and CLI use the same CommandInput and terminal input-line stream.
- `src/mobile/components/CommandInput.tsx:172-179`: unsolicited PTY drafts do not populate the visible composer. This protects against delayed voice/queue echoes.
- Before this fix, `syncedText` initialized to an empty string for every non-restored composer. `syncDelta` therefore appended the first local draft to the actual PTY even when `ptyInputLine` contained an existing command. The existing null-baseline reconciliation at `CommandInput.tsx:219-224` was only used for restored dictation.
- `src/mobile/useBrowserDictation.ts:222-227` appends recognized words to the visible draft. Its callback reaches `CommandInput.tsx:233-248`, the same edit path as native keyboard dictation. It does not insert or select slash commands.
- `CommandInput.tsx:283-296` selects a command only after an explicit menu selection. `SlashMenuOverlay.tsx:43-51` wires that selection to a row click. A highlighted item is not automatically selected by Send.
- `CommandInput.tsx:419-424` waits for the streamed draft then sends Enter; it does not prepend a command.
- The separate desktop dictation path (`src/hooks/useDictation.ts:154-158` and `src/hooks/dictationTarget.ts:120-145`) delivers captured text to its captured destination. It has no `/wiz:work` substitution. Its existence is not evidence that it ran during the reported phone interaction.

### History

Regression: `e2e75403724d4d495442f7c970f7fb7966d53c0d` intentionally stopped importing unsolicited PTY drafts and made `isSupersetEcho` reject an empty baseline. It retained the empty first-edit baseline. A command already in the PTY could thus become invisible while remaining the prefix of the submitted message. Earlier bidirectional synchronization imported nonempty snapshots into an empty composer. Later `1cc06c9409c` fixed reconciliation for restored dictated drafts only.

### Change and boundaries

Initialize the baseline as unknown for all newly mounted composers and reuse the existing first-edit reconciliation with `ptyInputLine`. Do not import that snapshot into the visible draft. Subsequent local deltas, explicit history/Tab responses, atomic replies and post-Send echo suppression retain their existing ownership rules.

For the reported sentence, an existing `/wiz:work ` draft has 10 characters and the sentence has 57 characters. With no common prefix, the existing delta algorithm emits 10 backspaces and 57 characters before the separate Enter. This adds no new synchronization path. It retains the existing end-of-line cursor and current-snapshot assumptions; an absent/stale snapshot or an unobserved concurrent terminal writer cannot be proven safe by this UI change.

## Defect B: command envelopes are rendered as ordinary prose

- `src-tauri/src/chat_view/claude.rs:290-332` accepts human-origin text and forwards it as user-message text. Legacy harness echoes are excluded separately.
- `src/components/AIChatPanel/Transcript.tsx:808-833` routes terminal user messages through TerminalUserText.
- Before this fix, `src/components/Terminal/chatPresentation.tsx:69-114` handled pasted output only; every other message was rendered as prose. No command-envelope display existed.

History: **never worked**. The original terminal Chat adapter (`2e2e5f8e92`) forwarded human text verbatim. Initial terminal user presentation (`9726b02ae3`) had no command-envelope branch. The legacy-echo exclusion in `77eb955e95` is not a readable command renderer.

The fix recognizes a complete, whole-message command envelope and renders the command name as a code chip followed by arguments. Quoted examples, incomplete markup and non-slash names remain literal. This is presentation only: backend messages, transport shapes and the Copy action retain the original text.

## Verification

Pre-fix HEAD: `3de2525b7c37ad839ddfac552b6df61f2699f341`.

RED command, through the local build slot and repository test-temp wrapper:

```sh
scripts/with-test-tmp.sh pnpm vitest run src/mobile/__tests__/CommandInput.dictation-command.test.tsx src/__tests__/components/Transcript.command.test.tsx
```

Result: 5 failed, 4 passed, 2.69 seconds. The dictated-draft assertion received `/wiz:work OMB è un assistente.` instead of `OMB è un assistente.`; the slash-only and ordinary-draft variants also failed. Transcript assertions saw escaped raw command XML and no command chip. Log: `~/Gits/.tmp/tuic-2460-red.log`.

The browser fixture uses production composer, keybar and transcript components with synthetic recognition callbacks and a disposable terminal-input sink. It is a browser interaction/layout check, not a microphone, iPhone, native Claude or end-to-end backend test. No desktop application is launched. A real-device replay remains in `to-test.md`.

Final targeted validation: 54 tests passed in eight files (5.85 seconds), followed by `pnpm exec tsc --noEmit` (exit 0). The existing history-ordering rejection test logs its expected write failure; no test failed. After adding routine-log capture to the new regression test and clearing only the browser fixture's saved draft, the invalidated four regression cases passed again (2.42 seconds), and typecheck passed again. Production source did not change between those runs.

Browser check: agent-browser through the stealth wrapper, worktree Vite on port 15460, viewport 390×844. Exact draft and submitted bubble both equal the reported sentence, without a slash prefix. The separate command bubble contains `/wiz:work` and its arguments, no raw envelope. DOM checks report no horizontal overflow. Screenshots:

- `~/Gits/.tmp/results/chat-dictation-2460/mobile-dictated-draft.png`
- `~/Gits/.tmp/results/chat-dictation-2460/mobile-command-and-dictated-message.png`

Transcript: `~/Gits/.tmp/results/chat-dictation-2460/browser-exact.txt`. The script asserts the exact text, not substring presence. Initial attempts exposed a test-query type error, an evidence-script quoting error, a retained closed-tab binding and a restored fixture draft; each is retained in the earlier attempt logs. These are not claims of a production browser failure. The final named tab/daemon and Vite process were closed. The daily application was not launched or changed.
