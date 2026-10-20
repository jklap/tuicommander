# Native question parity audit — 2021-30cd

Audit date: 2026-10-19. Claude is the reference, not an assumed all-pass baseline. This report records retained live evidence, not subsequent fixes. No production code changed.

## Evidence key and limits

- **F** = `~/Gits/.tmp/results/parity-2021-final/`: permissions, mobile Chat, desktop DOM/store and network captures.
- **R** = `~/Gits/.tmp/results/live-restart/`: native multiple-choice round trips, mobile DOM/API and cleanup. Its `parity-matrix.md` identifies runtime `c281128313` and mobile viewport 390×844.
- **Q** = native multiple-choice question; **P** = enforced permission for harmless `printf`.
- **Unknown** is missing proof, not success. Store state is not a visual badge. Native output after a mobile answer does not prove desktop delivery. List navigation is not an answer surface.

These are local retained artifacts, not portable links or committed fixtures. Some JSON files encode another JSON string. `F/permission-states.json` is empty; use `F/permissions.json`. All agents use PTY, including ego CLI; this is not ACP AI Chat. No second desktop instance was used.

## Shared detection and native state

| Agent | Q detection and native options | P detection and native options | Evidence |
| --- | --- | --- | --- |
| Claude (reference) | Imperative `Pick a colour` is confidently awaiting but has empty question text and no exported choices (2074). Question-mark title later exports Red/Green/Blue and native text/chat alternatives. | Confident awaiting; native Yes/No correctly exported. | R/parity-all-states.json; R/claude-output.json; F/claude-choice-chat-dom.txt; F/permissions.json |
| Codex | Real /plan question; exported Red/Green/Blue/None of the above. | Native three-choice approval remains open but REST and desktop store say idle with no choices (2075). | R/codex-output.json; R/parity-all-states.json; F/permissions.json; F/desktop-dom-and-store.json |
| Grok | Real Ask; exported Red/Green/Blue/z custom. | Confident awaiting; all five native scope/approval choices exported. | R/grok-output.json; R/parity-all-states.json; F/permissions.json |
| ego | Native selector; exported Red/Green/Blue. | Confident awaiting; native y/a/n/e/d exported. | R/ego-question-state.json; F/permissions.json |

## Agent × surface × capability

Detection below uses the shared API state above. The badge column distinguishes DOM evidence from store-only evidence. Q and P answer claims remain separate.

| Agent | Surface | Badge / state evidence | Options | Answer delivery | Artifacts / gap |
| --- | --- | --- | --- | --- | --- |
| Claude | Desktop terminal | P store awaiting; final imperative Q badge unknown | Native Q/P output captured; canvas pixels unknown | Desktop Q/P unknown | R/claude-output.json; F/permissions.json; F/desktop-dom-and-store.json |
| Claude | Desktop Chat | Q store awaiting with Chat active | Q named choices absent; only Copy/compose controls | Named-choice answer unavailable; compose answer not tested | F/desktop-claude-open-question-chat.json; 2079 |
| Claude | Mobile list | P question in awaiting list; final imperative Q badge not separately captured | N/A | N/A | F/list-dom.txt; R/parity-matrix.md |
| Claude | Mobile CLI | P Input; Q detection lacks exported title | P Yes/No match native; imperative Q colour controls absent | Q generic Enter selects Red; CLI-specific P delivery unknown | R/claude-mobile-dom.txt; R/parity-after-states.json; F/claude-permission-cli-dom.txt; 2074 |
| Claude | Mobile Chat | Q/P Input | Q colour controls and native alternatives; P Yes/No | Q Blue confirmed in native output and Chat with awaiting retraction. P command completes after approval sequence; multi-tab delay affects timing. | F/claude-choice-chat-dom.txt; F/claude-chat-blue-after.json; F/claude-chat-blue-after-dom.txt; F/claude-permission-chat-single-tab-dom.txt; F/permission-native-after.json; 2078 |
| Codex | Desktop terminal | Q store awaiting; P store idle despite native dialog | Native Q/P output captured; canvas pixels unknown | Desktop Q/P unknown | R/desktop-parity-questions.json; F/desktop-dom-and-store.json; 2075 |
| Codex | Desktop Chat | Final session disabled by exact transcript-binding gate | Unavailable in captured session | Unknown; binding gate is not itself a confirmed defect | F/desktop-dom-and-store.json |
| Codex | Mobile list | Q Input/title; P absent from awaiting list | N/A | N/A | R/mobile-list-dom.txt; F/list-dom.txt; 2075 |
| Codex | Mobile CLI | Q Input; P Idle | Q four choices; P no choice overlay, generic keyboard remains | Q Green and awaiting false. P generic Enter approves once; native command confirms. | R/codex-mobile-dom.txt; R/codex-after-mobile-answer.json; R/codex-after-output.json; F/codex-before-enter-dom.txt; F/codex-after-enter-dom.txt; 2075 |
| Codex | Mobile Chat | Surface excluded | No Chat toggle | Unavailable | F/codex-permission-cli-dom.txt plus source exclusion; 2076 |
| Grok | Desktop terminal | Q/P store awaiting; pixels unknown | Native Q/P captured | Desktop Q/P unknown | R/desktop-parity-questions.json; F/desktop-dom-and-store.json; F/permissions.json |
| Grok | Desktop Chat | Completed P transcript captured, not pending dialog | Pending Q/P controls unknown live; shared renderer lacks choices | Desktop Q/P unknown | F/desktop-dom-and-store.json; common boundary 2079 |
| Grok | Mobile list | Q Input/title; final open P list not captured | N/A | N/A | R/mobile-list-dom.txt; F/list-dom.txt predates final Grok session |
| Grok | Mobile CLI | Q/P question detected | Q four choices; P five native choices | Q Green and awaiting false. P approval sequence followed by PARITY_GROK_OK/DONE and awaiting false. | R/grok-mobile-dom.txt; R/grok-after-mobile-answer.json; R/grok-after-output.json; F/grok-permission-cli-dom.txt; F/grok-after-permission-dom.txt; F/post-approval-states.json |
| Grok | Mobile Chat | Surface excluded | No Chat toggle | Unavailable | F/grok-permission-cli-dom.txt plus source exclusion; 2077 |
| ego | Desktop terminal | Q/P store awaiting; pixels unknown | Native Q/P captured | Desktop Q/P unknown | R/desktop-parity-questions.json; F/desktop-dom-and-store.json; F/permissions.json |
| ego | Desktop Chat | Transcript after original command; later progress approval still pending | No native choices in captured button list | Desktop Q/P unknown | F/desktop-dom-and-store.json; F/post-approval-states.json; common boundary 2079 |
| ego | Mobile list | P awaiting question shown; final Q badge not separately captured | N/A | N/A | F/list-dom.txt; R/parity-matrix.md |
| ego | Mobile CLI | Q Input and Open ego question | Q three choices; P CLI-specific capture absent | Q Green and awaiting retraction; P CLI-specific delivery unknown | R/ego-mobile-dom.txt; R/ego-open-dom.txt; R/ego-after-mobile-answer.json; R/2029-ego-native-output.json |
| ego | Mobile Chat | P Input and expanded question action | P y/a/n/e/d; Q Chat not captured | P command eventually executes after Chat tap when connection pressure releases. Later progress-tool approval is a different question, not stale printf approval. Q Chat unknown. | F/ego-permission-chat-dom.txt; F/permission-native-after.json; F/post-approval-states.json; 2078 |

## Confirmed gaps and source boundaries

Line references were checked in the main checkout during report preparation. A missing boundary is not automatically an isolated root cause.

| Story | Confirmed gap | Detection / rendering boundary |
| --- | --- | --- |
| 2074-b0dc | Claude imperative AskUserQuestion title yields no native title/options payload. | `src-tauri/crates/tuic-terminal/src/output_parser.rs:2554-2561` requires a title ending in `?`; called at `src-tauri/src/pty.rs:2724-2726`. Mobile controls require a choice payload at `src/mobile/components/CommandInput.tsx:455,528-529`. |
| 2075-aa2f | Codex native enforced approval becomes idle while still open. | `src-tauri/src/pty.rs:2723-2735` chooses the parser; `src-tauri/crates/tuic-terminal/src/output_parser.rs:2225-2229,2248-2259,2316-2321` handles footer/options/title. Root cause remains unisolated; do not assert a wrapped-label or transport cause. |
| 2076-8190 | Mobile excludes Codex Chat despite shared desktop support. | `src/mobile/components/useSessionChatView.tsx:24,36-45` accepts only Claude/ego; compare `src/stores/chatView.ts:61-72`. Preserve exact transcript-binding gates. |
| 2077-b132 | Mobile excludes Grok Chat despite shared desktop support. | Same mobile exclusion at `src/mobile/components/useSessionChatView.tsx:24,36-45`; shared support at `src/stores/chatView.ts:61-72`. |
| 2078-464b | Two same-origin mobile tabs exhaust observed HTTP/1.1 connections and delay ordinary state/answer requests. | `src/transport/ipc.ts:357-375` creates one EventSource per subscription. `F/network-requests.json` records six open streams across the two tabs, connection IDs 7673/7678/7649/7763 and 7762/7761, with later ordinary requests lacking responses at capture end. Tab closure releases queued work. This is not proof of faulty native key semantics. |
| 2079-78fa | Desktop terminal Chat omits native question controls that mobile Chat shows. | `src/components/Terminal/Terminal.tsx:1450-1458` passes terminal/session identity; `src/components/Terminal/TerminalChatView.tsx:43-49` mounts the transcript without question state. Compare mobile `src/mobile/components/CommandInput.tsx:528-529`. Same-session pending Claude desktop/mobile DOM confirms the gap; Grok/ego pending-dialog parity is source-shared, not independently live-proven. |

All six existing fix stories include authentic-evidence requirements, named RED regressions and batch GREEN criteria. They are follow-up fixes; this audit does not implement them. The earlier mobile session-binding fix belongs to 2071-174b, not this audit.

## Acceptance-criterion disposition

1. **Supported:** real Q sessions exist for all four in R; real native P dialogs for all four exist in F/permissions.json. Startup failures are not counted as successful sessions.
2. **Not fully supported:** the matrix above records the available evidence, but desktop answer delivery, several pending Chat cells, and some final list badges have no retained per-cell proof. Confirmed unavailable controls are gaps, not passes. A mobile round trip cannot satisfy a desktop round trip. Do not check this criterion or close the story on the current evidence.
3. **Supported by this report:** all agents/surfaces/capabilities are listed; unknown cells remain explicit; six confirmed gaps have source boundaries and story IDs.
4. **Supported:** existing stories 2074-b0dc through 2079-78fa contain RED-ready follow-up criteria. No fix was made here.
5. **Already checked:** the coordinator reports all four final parity sessions closed. R/parity-cleanup.json documents the earlier multiple-choice cleanup. This report starts no session and touches no Boss session.

One additional throwaway session cannot establish missing agent-specific desktop delivery for all four agents. Completion requires the coordinator to authorize and collect the missing evidence, or explicitly revise the audit's acceptance scope. No unavailable cell is silently waived.

## Isolated follow-up attempt (21:18–21:26 local time)

The coordinator authorized fresh disposable sessions to fill unknown cells, using a private headless backend and same-source Vite. The main-checkout build was refused by build-slot's protected-checkout gate. The coordinator then supplied the TUIC-created read-only build source `tuicommander__wt/chore-live-check-main`, revision `445fd7d9475638479f234a42f8d82e07acd8f44a`. Its native headless build succeeded through build-slot/mbx (2m55s build, preceded by 100s admission wait). No installed binary was replaced.

The backend used private port 22021; Vite used 22022. Backend HOME and temporary paths were under `~/Gits/.tmp/results/close-2021-live/`, not Boss's config. Backend and Vite HTTP probes returned 200. The mandated shared-browser wrapper initially created owned tab `59F685D750AB3C921392660F1334877A`, but navigation/eval then timed out. A full-budget recovery produced: `agent-browser --cdp 57162 --pin-tab --json open ... timed out after 60 seconds`. See `~/Gits/.tmp/results/close-2021-browser-recovery.log` and `close-2021-live/browser-transcript.txt`. No alternative browser transport was used to claim DOM evidence.

The four HTTP spawn receipts and `startup.json` are retained under `close-2021-live/`. Grok reached a native colour question. Claude stopped at its external-instruction import decision, Codex at an update notice, and ego exited with `read-only directory: cannot enforce a read-only directory beneath a writable root`. Startup gates are not question-parity passes. No permission or sandbox override was used to evade ego's rejection.

No new matrix cell is checked from this attempt. Three still-live owned PTYs were deleted; ego was already absent. `sessions-after-cleanup.json` is an empty list. The exact owned browser target was closed through its CDP cleanup endpoint; the shared Chrome process was left intact. The first named agent-browser daemon close timed out (`process-cleanup.json`); the final retry after exact target closure succeeded (`daemon-final-close.txt`). Backend and Vite received SIGTERM and were confirmed absent with `ps`. Criterion 2 remains open pending working browser automation and an approved ego launch configuration.


## Current-main retry by live-2021b (23:37–23:42 local time)

The retry fast-forwarded the assigned branch to current main `112ace3f25e67efe02ae8ff2d609e804ea3c5a5a`. A native headless build through build-slot/mbx passed in 2m03s; mbx doctor reported zero failures. The private backend used port 22121, the worktree Vite used port 22122, and backend HOME was `~/Gits/.tmp/results/live-2021b/home`. No installed binary was replaced. The mandated ab-stealth wrapper opened the private page and returned snapshot, eval and screenshot evidence successfully. This retry does not reproduce the earlier browser timeout.

Fresh ego PTY `f13269e2-a47f-4df5-a993-6700b5893c69` exited before the question with the exact error: `read-only directory: cannot enforce a read-only directory beneath a writable root`. TUIC supplies ego's attachment grant with `--read-only-dir` (`src-tauri/src/agent_hook_launch.rs:273,296`). In this retry, the backend-owned grant is beneath the isolated HOME inside `~/Gits`, which is a default writable root for ego. No policy override or alternate launch transport was used to bypass the rejection. An approved ego launch/configuration layout that preserves the read-only grant is required before the complete matrix can run under this isolated-HOME constraint.

`~/Gits/.tmp/results/live-2021b/startup.json` retains API state and native output for all four spawn receipts. Claude was at external-import consent, Codex at its composer, and Grok had started the audit prompt. These are startup observations, not passing question cells. The browser transcript and `startup.png` record the private page, not Boss's desktop app. No criterion was newly checked. Criterion 2 remains open; `story-close.sh --check` reported it unchecked and otherwise passed its format preflight. That preflight is not authorization to close an incomplete story.

Cleanup: the three live owned PTYs were deleted; ego was already absent. `sessions-after-cleanup.json` is `[]`. The owned browser tab and named daemon closed successfully; shared Chrome was left running. Backend PID 77320 and Vite PID 77321 received SIGTERM and a subsequent `ps` found neither. The retry's temporary Vite config was removed. No production code or capture corpus changed, and no Rust or frontend test suite ran.

### Approved root-narrowing retry (23:43–23:45 local time)

The coordinator authorized an isolated ego user config with its sole writable root set to the parity cwd. `EGO_HOME=~/Gits/.tmp/results/live-2021b/ego-home ego config set -- 'roots=[{path="/Users/stefano.straus/Gits/.tmp/results/live-2021b/scratch/ego",access="read-write"}]'` succeeded. `ego-effective.json` records the effective configuration. The fresh native ego launch accepted the read-only attachment directory, resolving the first launch rejection, but then exited with `no stored subscription credential: run ego auth login openai-codex`. Evidence: `r2-startup.json`. No credential was read, copied or linked from Boss's store.

A launch-scoped `-c roots=[{path="/Users/stefano.straus/Gits/.tmp/results/live-2021b/scratch/ego",access="read-write"}]` attempt, without changing Boss's saved configuration, failed validation before startup: `the command line: invalid config in flags layer: roots in project/local layers may only restrict read access` (`r3-startup.json`). Thus the tested installed CLI does not permit this alternative writable-root override. The available `ego auth login [--device] openai-codex` command requires a new login to the isolated store. No login was started. The remaining prerequisite is isolated-store authentication or an approved way to select a separate user configuration while using the existing credential provider; neither is a parity pass.

All retry sessions were deleted (`r2-sessions-after-cleanup.json` is `[]`), the owned `live-2021b-r2` browser tab and daemon closed, and backend PID 26557 plus Vite PID 26559 were stopped. No production code changed and criterion 2 remains unchecked.


## desk-2021 isolated desktop retry (2026-10-20)

**Criterion 2 remains unchecked.** This run resolves the ego login prerequisite and adds real round trips. It does not establish the complete question/permission matrix. A failed binding, a startup gate, an absent approval, or an API payload alone is not a surface pass.

### Runtime and evidence

Evidence root **D**: `~/Gits/.tmp/results/desk-2021/`. `browser-transcript.txt` retains the ordered browser commands and results; `snapshot-01.txt` through `snapshot-34.txt` split its DOM snapshots. JSON captures retain the authoritative `/sessions` collection in `sessions` and native output in `agents.<agent>.output`. The unused `agents.<agent>.session` field is empty because the attempted per-session GET is not a state endpoint; do not use that field as evidence.

- Source: `chore/desk-2021-cells`, pre-documentation HEAD `ded040a205b427d9963c466916a5ff5cea8b70a1`; the requested merge of `chore/live-2021-parity` reported already up to date.
- Authorized `make agent-test AGENT_TEST_INSTANCE=at-2021 AGENT_TEST_PORT=19921` built and launched the isolated native macOS app. `mbx doctor` reported zero failures. API calls used its printed Unix socket. Browser checks used its embedded frontend at `127.0.0.1:19921`, not Boss's app, not port 9876, and not a Vite server. These desktop-shell browser checks do not prove the native IPC path or native window pixels.
- The first launch exited with status 0 before browser capture; the initial browser navigation and subsequent socket/TCP probes failed with connection refused. Cause unknown. See `runtime-ended.json` and `agent-test.log`. A fresh retry ran until explicit cleanup.
- The mandated wrapper repeatedly reported **macOS screen is locked**. DOM snapshots and several clicks worked, but browser commands also timed out and the backend logged sleep/wake gaps. No screenshot is claimed. Canvas content and visual badge colour remain unverified. Desktop tab DOM classes `awaitingInput`/`awaitingQuestion` were retained in the transcript; a status bar reading Ready is not substituted for question state.
- `ego auth status` reported an existing openai-codex OAuth login. Ego used its default home and scratch cwd under D; no credential store was read or copied and no saved ego configuration changed. The agent-test upload grant is outside Gits and native ego accepted it. The isolated TUIC instance initially had no `ego_executable`; only its own config was set through `/config` to the installed absolute executable. After that, ego Chat opened successfully.
- Q receipts: Claude `4db5e9d2`, Codex `8be58af6`, Grok `e7fffde7`, ego `415d06bf`. P receipts are in `permission-receipts.json`. These are disposable PTY agents, not ACP conversations.

### Added per-cell evidence

Q = native colour question; P = enforced harmless printf approval. This table records this retry only; historical R/F evidence above remains separate. “Unknown” is deliberately not a pass. List rows are navigation surfaces, not answer controls.

| Agent | Surface | Detection / badge and options | Native answer evidence / remaining gap | D artifacts |
| --- | --- | --- | --- | --- |
| Claude | Desktop terminal | Q API awaiting; terminal canvas mounted; tab awaiting classes captured. Canvas options not pixel-verified. | Native Enter cleared awaiting, but retained output was still in PostToolUse hooks and does not prove a selected colour. P delivery unknown. | `final-questions.json`, `snapshot-34.txt`, `desktop-terminal-claude-after.json` |
| Claude | Desktop Chat | Q Red/Green/Blue and native text/chat alternatives; P Yes/No rendered in Native terminal question region. | Green Q confirmed in native output; P printf and PARITY_P_DONE confirmed after Yes tap. | `round2-api.json`, `snapshot-22.txt`, `snapshot-33.txt`, `desktop-chat-round2.json`, `permission-final.json` |
| Claude | Mobile list | Q Input shown. Initial narrow native viewport exported no title/options; explicit resize restored them. | Navigation only; no answer controls expected. P list not separately retained. | `questions-before.json`, `snapshot-06.txt`, `snapshot-12.txt`, `claude-mobile-chat-before.json` |
| Claude | Mobile CLI | Q Input; narrow native dialog showed generic Yes/No instead of all colour choices. | No new CLI-specific Q/P answer round trip. Earlier R/F evidence is not relabelled as this retry. | `snapshot-13.txt`, `questions-before.json` |
| Claude | Mobile Chat | After native resize, Q title and all five native choices rendered with Input header. | Blue tap confirmed by native User answered and PARITY_Q_DONE Blue. P not repeated here. | `snapshot-14.txt`, `claude-mobile-chat-before.json`, `desktop-before.json` |
| Codex | Desktop terminal | P API awaiting and terminal canvas mounted. Q native output retained. Canvas options not pixel-verified. | No terminal-specific answer round trip. | `desktop-before.json`, `permission-before.json`, `snapshot-31.txt` |
| Codex | Desktop Chat | Q four choices with separate hints; P three native approval options rendered. | Green Q confirmed in native output; P Yes tap executes printf and produces PARITY_P_DONE. | `snapshot-20.txt`, `snapshot-32.txt`, `desktop-chat-round2.json`, `permission-final.json` |
| Codex | Mobile list | First real /plan Q was visibly open in native output, but API/list reported Idle. Later repeated Q was correctly exported. | This first-question mismatch is not an all-pass detection result. P list not separately retained. | `questions-before.json`, `snapshot-06.txt`, `snapshot-09.txt`, `desktop-before.json` |
| Codex | Mobile CLI | First Q title was in native output but no choice controls rendered; generic keyboard remained. | No CLI-specific answer in this retry. | `snapshot-10.txt`, `questions-before.json` |
| Codex | Mobile Chat | Chat opens with exact transcript binding, but first Q had no choices or Input badge because API was idle. | Generic Enter from Chat selected Red; native Questions answered and PARITY_Q_DONE Red confirm delivery. P Chat not repeated. | `snapshot-11.txt`, `mobile-after.json` |
| Grok | Desktop terminal | Q API exports choices; canvas mounted and awaiting tab classes retained. | Trusted browser key 2 reached the native selector; Green confirmed. No fresh P approval: default-mode printf ran without a dialog. | `round2-api.json`, `snapshot-23.txt`, `grok-desktop-terminal-after.json`, `permission-before.json` |
| Grok | Desktop Chat | Native Q still open, but Chat closes with no Grok session is registered for this terminal. | Blocked by exact transcript binding; no Chat answer pass. P Chat unproven. | `snapshot-21.txt`, `snapshot-23.txt`, `desktop-chat-round2.json` |
| Grok | Mobile list | Q Input, title and awaiting list entry rendered. | Navigation only. Fresh printf produced no P dialog; not a permission pass. | `snapshot-06.txt`, `snapshot-09.txt`, `questions-before.json`, `permission-before.json` |
| Grok | Mobile CLI | Q Red/Green/Blue/z rendered with Input header. | Green tap confirmed in native output and awaiting retracted. | `snapshot-07.txt`, `grok-mobile-cli-after.json`, `mobile-after.json` |
| Grok | Mobile Chat | Toggle exists, but Chat closes with the same exact-binding error and returns to CLI. | Unavailable in this session; the subsequent Green tap is a CLI answer, not Chat delivery. | `snapshot-08.txt`, `questions-before.json`, `grok-mobile-cli-after.json` |
| ego | Desktop terminal | Q native selector/API payload retained; terminal canvas mounted. | No terminal-specific Q/P answer round trip. Later Q repeat failed with HTTP response body failed; no recovery/pass inferred. | `snapshot-16.txt`, `desktop-before.json`, `final-questions.json` |
| ego | Desktop Chat | After isolated TUIC executable configuration, Q transcript and Red/Green/Blue controls rendered. | Blue tap confirmed by native PARITY_Q_DONE Blue. P desktop Chat not exercised. | `snapshot-19.txt`, `desktop-before.json`, `desktop-chat-after.json` |
| ego | Mobile list | Q Input/title and P Input/Allow run printf question rendered. | Navigation only. | `snapshot-02.txt`, `snapshot-24.txt`, `snapshot-27.txt`, `questions-before.json`, `permission-startup.json` |
| ego | Mobile CLI | Q and P header actions expand the correct native options. | Green Q and one-time y approval both reach ego; printf exits 0 and prints marker. Later progress-tool approval is distinct, not stale printf approval. | `snapshot-04.txt`, `snapshot-29.txt`, `ego-mobile-cli-after.json`, `permission-final.json` |
| ego | Mobile Chat | Transcript and expanded Q Red/Green/Blue controls render after isolated TUIC executable configuration. | Red tap confirmed in native PARITY_Q_DONE Red despite a transient provider retry. P Chat is historical F evidence only. | `snapshot-26.txt`, `grok-desktop-terminal-after.json`, `ego-mobile-chat-after.json` |

### Proven limits and follow-up boundaries

1. **Grok Chat cannot be audited in these sessions.** Both surfaces return the same binding error while native Q/API detection works. `src-tauri/src/chat_view/mod.rs:312-315` requires `grok_transcript`; absence fails closed. This is the proven layer, not proof that the registry, process lookup, or producer is defective. No guessed binding or fallback was installed.
2. **Initial detection is not uniformly successful.** The first Codex /plan dialog had API Idle and no choice payload; a repeat exported the complete payload. The initial narrow Claude viewport omitted choices; an explicit 41×128 resize restored them. The earlier failed states remain evidence, not waived startup noise. Root causes are not established by this audit.
3. **Pixel checks and remaining independent round trips are incomplete.** The locked screen prevented reliable canvas evidence; later ego stream failure and repeated browser/API timeouts limit continuation. Desktop terminal answers for Codex/ego, several independent permission answers, and Grok Chat are still not proven. No original acceptance scope was revised, so criterion 2 must not be checked from this run.
4. **No new enforced Grok permission was produced.** Explicit default permission mode still ran printf without approval. This does not verify approval detection or answer delivery; the historical F enforced-approval evidence remains the only such Grok evidence.

### Cleanup and validation

All eight retry sessions were explicitly deleted; `cleanup.json` retains responses and `sessions-after-cleanup.json` is `[]`. The first-launch receipts were already absent in `retry-startup.json`. `make agent-test-stop` requested shutdown; the supervisor removed its PID file and stopped (the supervising make reports Error 143 / exit 2 after the requested SIGTERM). No test instance remains.

The owned wrapper tab initially refused close with Cannot close the last tab. The named `ab-shared-desk-2021-81136` daemon then closed successfully; the exact owned target `B698ED61B198B1E387870653C3293E27` was closed through its CDP cleanup endpoint. Shared Chrome PID 81136 was left running. No Vite or headless backend was started. No Boss session, installed executable, production source or capture corpus changed. No Rust/Vitest/TypeScript suite ran; this is live audit evidence plus documentation checks, not a suite pass.
