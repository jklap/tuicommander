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
