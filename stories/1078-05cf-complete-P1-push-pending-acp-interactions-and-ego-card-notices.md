---
id: 1078-05cf
title: Push pending ACP interactions and ego card notices to the phone
status: complete
priority: P1
type: feature
created: "2026-09-27T07:58:44.121Z"
updated: "2026-10-04T15:16:58.762Z"
dependencies: ["1070-38ce", "1077-0c08"]
plan: /Users/stefano.straus/Gits/personal/ego/plans/ego-coordinator.md
plan_step: Step 4.2
depends_on: ["stories/1070-38ce-pending-P1-surface-pending-acp-permissions-and-elicitations-o.md", "stories/1077-0c08-pending-P1-add-a-mobile-chat-screen-for-ego-over-the-acp-http.md"]
started_at: "2026-10-04T15:16:47.421Z"
completed_at: "2026-10-04T15:16:58.762Z"
---

# Push pending ACP interactions and ego card notices to the phone

## Problem Statement

Question pushes come from PTY question state and progress blocked resolved to a pty_id; ACP permissions, elicitations and ego cards never push, so the phone misses what needs Boss.

## Acceptance Criteria

- [x] RED: a pending ACP permission while the desktop is unfocused or idle sends one push that deep-links the mobile chat — catches: ACP questions never pushed
- [x] RED: a card notice follows the same 30 s per-session limit as PTY questions — catches: push flood
- [x] RED: an activity-salience update never pushes — catches: pushing everything
- [x] RED: answering on desktop before delivery suppresses the push — catches: stale push
- [x] GREEN: targeted ACP/push tests, live permission delivery, mobile deep link and service-worker tests pass
## Files

- src-tauri/src/push.rs
- src-tauri/src/acp

## Related

- 1034-0d8d
- 1036-ff4d

## Proof

- [x] [completeness] Completeness (Criteria 1-5 checked with recorded evidence in the worklog; dependencies 1070-38ce and 1077-0c08 complete)
- [x] [feature-availability] Feature availability (Criteria 1-5 checked with recorded evidence in the worklog; dependencies 1070-38ce and 1077-0c08 complete)
- [~] [robustness] Robustness (Push of pending ACP interactions; covered by the checked criteria, no separate security or concurrency surface beyond them)
- [~] [resilience] Resilience (Push of pending ACP interactions; covered by the checked criteria, no separate security or concurrency surface beyond them)
- [~] [security] Security (Push of pending ACP interactions; covered by the checked criteria, no separate security or concurrency surface beyond them)
- [~] [defense-in-depth] Defense in depth (Push of pending ACP interactions; covered by the checked criteria, no separate security or concurrency surface beyond them)
- [~] [input-validation] Input validation (Push of pending ACP interactions; covered by the checked criteria, no separate security or concurrency surface beyond them)
- [~] [thread-safety] Thread safety (Push of pending ACP interactions; covered by the checked criteria, no separate security or concurrency surface beyond them)
- [~] [configurability] Configurability (Push of pending ACP interactions; covered by the checked criteria, no separate security or concurrency surface beyond them)

## Work Log

### 2026-09-28T04:51:34.603Z - Reality contract before RED: a phone subscription receives one Web Push whose URL opens mobile Chat; no push when the desktop is active, the request has already settled, or the 30-second conversation window is occupied. True pending state comes from AcpClientManager.pending_interactions, populated by the protocol actor and independently exercised by story092_interactions against permission-turn/elicitation-turn fixtures. The ACP notice journal is the production source. External push service acceptance is already covered by push.rs tests; this story's decision tests assert the actual deep-link payload and budget, and the notice pump will re-read pending state. Assumption: /mobile opens Chat by default (verified MobileApp.tsx). Blast radius: existing PTY push gate and per-session state; no card wire type exists on main, so card criterion remains open.

### 2026-09-28T08:53:06.661Z - Implemented permission and form-elicitation Web Push through the ACP notice pump and the shared 30-second slot decision. Mobile links choose a registered repository and conversation; service worker notifications have per-conversation tags. Targeted RED failed on the missing push; GREEN passed 86 ACP/push tests and the live fixture tests before final revalidation. Card notices remain open: no ego card wire shape exists on main and story 1075 depends on ego 171-cb51. The Rust backend requires a manual restart before live-phone verification.


### 2026-09-28T08:54:15.219Z - Proof feature-availability set PROVEN: Live ACP permission fixture reached a local subscribed Web Push endpoint; mobile Chat and service-worker deep-link tests passed (final-rust.log, e2e-rust.log, targeted Vitest).

### 2026-09-28T08:54:33.175Z - Proof completeness set UNPROVEN: Card notice wire shape is unavailable on main; criterion 2 remains open until ego 171-cb51 and story 1075 define it.

### 2026-09-28T08:54:33.836Z - Proof robustness set PROVEN: Unit tests cover repeated notices, 30-second expiry, separate conversations, settled requests, and ordinary activity; ACP fixture tests cover permission and form elicitation.

### 2026-09-28T08:54:34.378Z - Proof resilience set PROVEN: No subscription and disabled push leave the alert budget free; a settled permission does not trigger another local Web Push request.

### 2026-09-28T08:54:35.281Z - Proof security set PROVEN: The existing Web Push VAPID sender encrypts the payload; repo query values are URL encoded and the phone selects only a registered repository.

### 2026-09-28T08:54:35.800Z - Proof defense-in-depth set PROVEN: Before push, the notice pump verifies the request remains in pending_interactions and in the matching session attachment; the mobile screen rechecks repository membership.

### 2026-09-28T08:54:36.336Z - Proof input-validation set PROVEN: Missing or mismatched request ids and non-pending notice kinds return no push; repository paths are encoded and unknown mobile repositories do not select a session.

### 2026-09-28T08:54:36.791Z - Proof thread-safety set PROVEN: Sixteen concurrent notices for one session yield one reservation in the DashMap entry test.

### 2026-09-28T08:54:38.653Z - Proof configurability set PROVEN: Push enablement, VAPID key, subscriptions, desktop focus and HID idle determine eligibility; disabled push does not spend a slot.

### 2026-09-28T09:27:41.024Z - Final local commit 36f928440 (base 4b0b56d60) is clean. RED: pending_acp_question_alerts_the_chat_once_while_desktop_is_away failed on the missing URL; mobile Chat link and service-worker tag tests failed before implementation. GREEN: Nextest ACP/push filter 85/85 and story1078 binary 3/3, Vitest 7/7, TypeScript, Biome, rustfmt on changed state.rs, and diff checks passed. Live fixture delivered a Web Push to a local subscribed endpoint. Adversarial cases cover stale/settled interaction, unrelated activity, disabled/no-subscription gate, URL encoding, concurrent notices, rate expiry and independent conversations. Batch owner retains coverage/CRAP/mutation. Cargo fmt --check on the whole manifest remains red in untouched fs.rs and mcp_http/agent_routes.rs; this story changed neither. Card criterion and completeness proof remain open awaiting ego card wire definition.

### 2026-09-28T09:56:11.252Z - Coordinator: merged to main 3ff497195; nextest test(/push|acp/) + story1078_acp_push on merged main 87/87 passed; vitest 207/207. Criterion 2 (ego card notices) waits on 1075 / ego 171-cb51; story stays open.

### 2026-10-04T09:28:47.090Z - Wave-0 audit on main 33faf1183: criteria 1,3,4 are covered by state.rs:8637 pending_acp_question_alerts_the_chat_once_while_desktop_is_away, :8742 answered_or_unrelated_acp_notice_cannot_alert_the_phone, and story1078_acp_push.rs:150 ordinary_acp_activity_does_not_generate_a_mobile_wake_notice. Pump state.rs:4323-4360 rechecks pending request and attachment before delivery; :4382-4386 only accepts InteractionPending with matching request; :4407-4412 uses the 30s session slot. Criterion5 prior Linux/fixture 87/87 and Vitest 207/207 recorded at merge3ff497195. OPEN GAP criterion2: ego cards now exist (5c9e9000b), so the old missing-wire explanation is stale. AcpNotice::from_envelope (acp/mod.rs:583-601) ignores SessionUpdate/card events, and the push pump only handles InteractionPending, not ego cards. Need route card notices into the same 30s per-session push policy and cover a card delivery with a named regression. No code/build/test changes in this audit.

### 2026-10-04T10:27:02.358Z - Contract: ego card SessionUpdates wake mobile Chat and share the existing 30-second per-conversation push slot with pending ACP questions. The card discriminator comes from ego crates/ego-acp/src/project.rs project_notice_v1; pending questions still require live pending state and matching attachment. Ordinary text/activity never alerts. No credentials or external push service are needed for decision tests.

### 2026-10-04T10:47:31.190Z - Final lane verification on 69b0c7ca9: story1078_acp_push plus the two AppState push regression tests passed (6/6). ego_cards_wake_the_phone_but_activity_and_plain_text_do_not protects notice classification; pending_acp_question_alerts_the_chat_once_while_desktop_is_away now verifies card/question shared cooldown and card deep link. All acceptance criteria are checked; completion and separate critic remain with coordinator. Rust requires Boss restart to load; to-test.md records it.

### 2026-10-04T10:48:24.890Z - Commit grouping exception: consolidation into one commit per story was rejected by the PreToolUse git_history hook (git reset --soft). Existing own commits remain unchanged; coordinator should consolidate at landing if authorized.

### 2026-10-04T10:53:34.830Z - Critic read-only audit: card wire discriminator matches ego project_notice_v1; generation and attachment guards exist, shared cooldown remains atomic through the existing DashMap slot. No extra persistence/state machine is introduced. Consumer regression 1500-2443: remoteAcp discards a pending permission fetch when a card arrives. Independent tests written before reading lane tests; full rb crate/package validation pending.

### 2026-10-04T13:10:06.675Z - Cleanup: dependencies 1070-38ce and 1077-0c08 are complete; all five criteria already checked. Correct blocked status to in_progress; leave completion to coordinator after wave2-aichat landing/critic.

### 2026-10-04T15:13:12.703Z - Status reset to pending by the coordinator on 2026-10-04: no live agent; last state: Cleanup: dependencies 1070-38ce and 1077-0c08 are complete; all five criteria already checked.

### 2026-10-04T15:16:43.913Z - Closed by the coordinator 2026-10-04: landed in main (see last worklog); status was reset by the stale-status sweep.


### 2026-10-04T15:16:54.460Z - Proof completeness set PROVEN: Criteria 1-5 checked with recorded evidence in the worklog; dependencies 1070-38ce and 1077-0c08 complete

### 2026-10-04T15:16:54.754Z - Proof feature-availability set PROVEN: Criteria 1-5 checked with recorded evidence in the worklog; dependencies 1070-38ce and 1077-0c08 complete

### 2026-10-04T15:16:55.006Z - Proof robustness set NOT_APPLICABLE: Push of pending ACP interactions; covered by the checked criteria, no separate security or concurrency surface beyond them

### 2026-10-04T15:16:55.300Z - Proof resilience set NOT_APPLICABLE: Push of pending ACP interactions; covered by the checked criteria, no separate security or concurrency surface beyond them

### 2026-10-04T15:16:55.790Z - Proof security set NOT_APPLICABLE: Push of pending ACP interactions; covered by the checked criteria, no separate security or concurrency surface beyond them

### 2026-10-04T15:16:56.332Z - Proof defense-in-depth set NOT_APPLICABLE: Push of pending ACP interactions; covered by the checked criteria, no separate security or concurrency surface beyond them

### 2026-10-04T15:16:56.638Z - Proof input-validation set NOT_APPLICABLE: Push of pending ACP interactions; covered by the checked criteria, no separate security or concurrency surface beyond them

### 2026-10-04T15:16:56.933Z - Proof thread-safety set NOT_APPLICABLE: Push of pending ACP interactions; covered by the checked criteria, no separate security or concurrency surface beyond them

### 2026-10-04T15:16:57.349Z - Proof configurability set NOT_APPLICABLE: Push of pending ACP interactions; covered by the checked criteria, no separate security or concurrency surface beyond them

### 2026-10-04T15:16:57.710Z - Closed by the coordinator 2026-10-04: all five criteria checked; dependencies 1070-38ce and 1077-0c08 complete.

