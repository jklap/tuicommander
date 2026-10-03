# Telegram channel for TUICommander

**Story:** 1438-79b4. **Date:** 2026-10-03. **Phase:** approved design; implementation authorized for offline slices 1–2 only, no deployment.
**Branch:** feat/1438-telegram-adapter. **Source baseline:** 140e8950deae7e4bb7a2f4498c7a23c95eacca21.

## Consumer contract

- An allowlisted private chat sends text to the configured mint agent as structured TUIC mail. Busy agents retain it for a safe boundary; Telegram text never becomes a shell command or composer payload.
- Once the agent accepts that request, the phone shows Thinking, concise activity, then an explicit complete reply. Unrelated turns must not appear in that stream.
- Stop targets only that request's live turn. Buttons create durable structured decisions before an LLM reads the resulting mail.
- The adapter owns update recovery. Existing inbox delivery owns safe wake. Bot API assumptions come from the request and public documentation; phase 1 does not read the token, call the Bot API, use SSH, or access mint.
- Success in this phase means a reviewable design commit addressing all eight assigned subjects. Story acceptance criteria remain open until implementation and verification.

## 1. Placement, ownership and dependencies

Implement a Rust `telegram` module under `src-tauri/src/telegram/`, started explicitly by `run_remote`, on mint only. The daemon already constructs `AppState`, wires its event bus, starts background tasks and binds local bridge IPC (`src-tauri/src/lib.rs:2731`, `:2745`, `:2755`). `tuic-remote --instance` selects the isolated config directory before startup (`src-tauri/src/bin/tuic_remote.rs:130`). No desktop poller, frontend business logic, second approval bot, or mandatory desktop hub connection.

Use one adapter task with one sequential `getUpdates` loop. Its outbound worker handles drafts, replies, callbacks and notices through the same Bot API client. A process-held lock beside the token/config enforces one local owner across daemon instances; a 409 detects an owner on another host. Desktop configuration stays disabled and cannot silently take ownership. Never start a second poller to collect `/start`.

Reuse `ideas/mcp-channels.md:51`: authoritative mail plus one wake owner, optional Claude channel delivery, and a normal reply tool. The concept's claim that composer mail is a payload injection is historical: current `mcp_transport.rs:5560` keeps managed mail out of the composer. `AgentMessage.content` is a string (`src-tauri/src/state.rs:1474`), so structured Telegram bodies are JSON serialized into that existing field, not a parallel mailbox protocol. The sender is a daemon-local registered adapter identity with no PTY.

Extract the existing native send service only where needed to accept a caller identity and a stable message ID. Preserve recipient resolution, identity locking, waiter arbitration, inbox-first ordering, journal attribution and delivery receipts (`src-tauri/src/mcp_http/mcp_transport.rs:5241`). Do not bypass them with `push_agent_inbox` alone. Ordinary managed recipients get a payload-free `PEER_MAIL_WAKE`; busy delivery is deferred (`:5657`). Orchestrator recipients use the coalesced safe-wake gate (`src-tauri/src/pty.rs:9376`, `:9438`). Use normal urgency; receiving a Telegram message never interrupts a tool.

Dependency evidence is from these branch snapshots, not landed behavior:

| Branch snapshot | Evidence | Required behavior / boundary |
| --- | --- | --- |
| `fix/1419-remote-mcp-routing` at `b42daf54c9fe730e568005c5dd08c299702ad6e5` | `src-tauri/src/mcp_http/remote_peer.rs:503`, `:855`; `mcp_transport.rs:5037` | Authenticated host-qualified peer mail; local daemon mail still works without its hub. Reuse `local_peer_call_with_message_id` rather than add another remote-mail mechanism. |
| Same branch | `src-tauri/src/mcp_http/remote_mcp_sessions.rs:124` | Remote MCP currently supports output/submit here, not raw interrupt. Do not claim 1419 supplies remote Stop or a remote Telegram tool. Local mint Stop avoids that gap. |
| `fix/1420-remote-agent-detect` at `fad4fdfd7e72dfb4d1cd6e33c4254d4da7ff27a3` | `src-tauri/src/pty.rs:12311`; `mcp_http/session.rs:923`; `mcp_transport.rs:5718` | Shared foreground-agent discovery for headless consumers; identity does not imply readiness. Plain shells remain mail-only and get actionable detection diagnostics. |

Before those branches land: Bot API client, journal, scheduler, schema, splitting and fake-server tests can be built against narrow ports. Their final native-mail integration must use 1419's stable-ID seam, and the hand-launched marketeer PTY wake test depends on 1420. Do not cherry-pick unfinished branch code. Re-read both final landed diffs before integration. Cross-host send by a desktop agent through the Telegram tool is outside this first slice; agents on mint use its local MCP bridge.

Binding config stores the stable target `tuic_session`, for a NEW managed Claude marketeer agent in `~/Gits/personal/marketeer` on mint. The coordinator spawns it with a stable UUID at deployment; it is not pe-3. The new agent reads marketeer/AGENTS.md, x/AGENTS.md and HANDOFF.md after pe-3 hands over. An ambiguous alias or missing peer retains pending mail and reports unavailability; it never selects a different agent. Rebinding requires an explicit config change. The coordinator supplies the new peer UUID at deployment.

## 2. Streaming source and correlation

| Source | What current code proves | Decision |
| --- | --- | --- |
| Transcript tailing | Claude discovery honors its config dir but can fall back to newest-unclaimed files (`src-tauri/src/agent_session.rs:270`, `:338`). Incremental tailing exists for Claude subagent maps (`src-tauri/src/subagent_map.rs:388`), not a portable final-answer stream. | Reject as primary: ambiguous ownership on older agents, provider formats, private tool arguments and no Telegram request correlation. |
| Raw TUIC events | Per-PTY semantic event channels coexist with the global bus (`src-tauri/src/state.rs:1930`). `ParsedEvent` has lifecycle/status/intent and tool errors, not generic tool-start details (`src-tauri/crates/tuic-terminal/src/output_parser.rs:19`). | Use as transport and lifecycle guard. Do not forward PTY bytes or infer replies from spinner redraws. |
| Intent and progress markers | Intent is journaled and emitted with `_turn_epoch` (`src-tauri/src/pty.rs:5517`, `:5560`). `ProgressRecorded` is emitted headlessly (`mcp_transport.rs:6931`). Progress text is capped at 500 characters (`src-tauri/src/progress/model.rs:3`). | Choose semantic intent/progress plus explicit correlated activity/reply calls. Progress is an outcome notice, not the full reply. |

The adapter subscribes to semantic events for the bound PTY. Deduplicate intents seen both as `PtyParsed` and journal entries. Use parsed intent for the active draft; use stored progress IDs for blocked/done notifications and replay. On subscriber lag, rebuild latest intent/outcome from state/journal; never replay raw terminal history.

A busy event alone cannot prove that the agent is working on a Telegram request. Inbound mail includes `request_id`. The agent calls `telegram begin` when it starts handling that mail; TUIC verifies caller identity, pending request and current PTY turn, then binds `(request_id, peer_uuid, pty_id, turn_epoch, draft_id)`. Send Thinking immediately after acceptance, within scheduler availability. Queued requests do not replace another request's active draft. Support one active request per target/chat in this first release; reject a second begin until finish/stop. Other incoming messages still queue as mail.

Activity uses the current intent as a concise line; explicit `telegram activity` reports a safe tool-step description under the same request. Existing hooks only emit busy/awaiting/idle (`src-tauri/src/agent_hook.rs:41`), so they cannot supply truthful tool names. Do not claim automatic tool-step coverage. The bound agent instructions must require activity updates at material tool boundaries and `finish` with the complete reply. No tool arguments, hidden reasoning or command output are streamed automatically.

Lifecycle idle/completed pauses activity but does not invent a reply. If the agent ends a turn without finish, record an incomplete response and stop the draft; keep the request recoverable and alert TUIC. A done progress event still sends its authored notice. Notifications from the bound peer outside a Telegram request are supported, unchanged in language. `finish` and progress are different messages; do not suppress arbitrary outcomes by comparing prose. Journal notification IDs prevent replay duplicates within the adapter.

## 3. Draft, rate and Stop lifecycle

Proposed states: `queued -> active -> finalizing -> finished`, with `stopped`, `incomplete` and `delivery_uncertain` terminal/recovery states. Persist transitions before observable side effects.

- Allocate a nonzero draft ID and persist its request mapping. Initial draft has empty text, `can_stop=true`, `keep_on_stop=false`. Update the same ID with a bounded activity tail.
- Refresh unchanged activity every 20 seconds while active, measured from the last successful send. Coalesce new activity to at most one update per 2 seconds. These are local scheduling choices, not claimed Telegram draft quotas.
- Use one per-chat outbound budget of at most one message operation per second, and a conservative bot-wide budget of 25 per second. Prioritize callback acknowledgements, final replies and due refreshes over intermediate activity. Respect `retry_after` on 429; do not let a retry block incoming polling. A prolonged network outage or throttle can expire a draft: report degraded delivery, preserve final text, and never claim the 30-second TTL can be guaranteed without connectivity. [Telegram rate guidance](https://core.telegram.org/bots/faq#my-bot-is-hitting-limits-how-do-i-avoid-this)
- Serialize draft refresh and finalization per request. Cancel future refreshes before sending the final reply. Store successful message IDs and chunk receipts. With plain text and no parse mode, split without losing bytes or splitting a Unicode scalar, preferably at a newline; use a conservative 4096 UTF-16-unit budget so astral characters cannot overflow. Reassemble chunks exactly; do not trim whitespace. Keyboard attaches only to the final chunk. An approval record covers all preceding exact-text chunks; bind their returned message IDs, and activate buttons only after every chunk succeeds. An uncertain or incomplete chunk delivery leaves approval buttons inactive.
- `stopped_message_generation` supplies chat and draft ID. Match both against the active persisted mapping and verify the live peer and current turn epoch. A stale Stop must never interrupt a newer turn. Retire the refresh task before interrupting. Duplicate update IDs do not send a second Esc.
- Stop calls `src-tauri/src/mcp_http/session.rs:235` `write_pty_input(state, pty_id, "\u001b")`, which invokes capture and input bookkeeping (`:245`, `:358`). This is the bare Esc mapping used by terminal input (`src/components/Terminal/terminalInput.ts:97`). Do not kill the process or substitute Ctrl+C. Log request/peer/draft IDs and write result, without token or private text; a successful write is an interrupt request, not proof that an agent has stopped.
- Keep stopped partial activity ephemeral; send a small persistent stopped notice after the interrupt attempt. If the turn already ended, record stale Stop without writing. Recovery must not replay an uncertain Esc into a later turn.

Draft TTL, empty-text placeholder and final `sendMessage` behavior are confirmed by the public [draft API](https://core.telegram.org/bots/api#sendmessagedraft); Stop fields by [MessageGenerationStopped](https://core.telegram.org/bots/api#messagegenerationstopped). No live Bot API behavior was exercised in phase 1.

## 4. Inline callbacks and exact-text approvals

Generic tool buttons are rows of `{label, data}`. Preserve `data` as an opaque string. Bot wire payload is `tc1:<random_handle>`; persisted mapping contains payload, issuing peer, chat ID, returned message ID, expiry and selection status. All callbacks use this mapping, including approvals. [Telegram limits callback data to 1–64 bytes](https://core.telegram.org/bots/api#inlinekeyboardbutton): `approve:<id>:<64 hex chars>` cannot fit. Never truncate a hash.

Inbound callback validation checks private allowlisted chat, sender identity for that private chat, known handle, matching message ID, expiry and issuing peer. Missing/inaccessible message metadata fails closed. Never route using a callback's supplied peer or raw action. In one transaction record the first choice and enqueue the following mail body; then call `answerCallbackQuery` and edit the message with the selected label and remove its keyboard. Ack/edit failures retry independently without re-recording the decision or re-mailing it. Later presses receive an already-selected response. Unknown/unauthorized callbacks have no mail or decision side effect; unauthorized chats are silently dropped.

```json
{"channel":"telegram","kind":"callback","request_id":"tg:bot-alias:12345","chat_id":"<allowed-id>","message_id":77,"callback_id":"<query-id>","data":"approve:<artifact-id>:<sha256>","decision_id":"<durable-id>"}
```

Text mail uses the same envelope with `kind:"text"`, `text`, originating `message_id` and `request_id` derived from the Bot API update ID. Existing `AgentMessage` retains the adapter sender identity and serializes this JSON into `content`; no user-controlled role or shell flags are accepted.

For publish approvals, add an explicit approval input `{artifact_id, exact_text}` to send. TUIC computes SHA-256 over the exact UTF-8 bytes before chunking, escaping or display annotation; no trimming, newline conversion or Unicode normalization. Require approval exact_text to equal the submitted message text byte for byte; reject a different preview. Store an immutable version `(artifact_id, digest, exact_text, issuer, chat, expiry)`. Buttons reference that version; edited text creates a new version and invalidates old handles. On click, Rust writes a decision row with that immutable digest before sending mail. Generic opaque `approve:*` strings alone do not confer publish authority.

The publisher must obtain the decision through a deterministic receipt lookup and recompute the digest of the bytes it will publish. The LLM cannot mint, alter or reinterpret a decision, and free-text "yes" never creates an approval. Proposed `telegram decision` returns the stored receipt to the issuing peer; the marketeer's publishing code must check it, not trust an agent's paraphrase. TUIC cannot prove an external publisher consumes this record: Boss selected the xkit publish path: TUIC owns the receipts, and xkit checks them before every publication, with no local approve path. This is application publish consent, separate from Claude MCP permission relay discussed in `ideas/mcp-channels.md:64`.

### Publish previews cannot carry approval

Coordinator constraint from br-3: content that might be published must never appear in the stream as already approved. The following is a required presentation and state contract, not an instruction for the LLM to decide whether approval exists.

- Every nonempty draft is framed by adapter-owned preview wording. For marketeer: "Anteprima — non approvata. Per approvare usa il pulsante legato al testo esatto nel messaggio definitivo." The empty Thinking placeholder carries no content or approval claim. If publishable text appears later, show it as a clearly delimited preview below that wording. The label is display metadata and is excluded from the exact-text digest.
- Draft state contains only request/turn/draft identifiers, preview text and activity scheduling. It has no approved flag, approval result, decision ID or publishing permission. `begin`, `activity`, `finish`, intent, progress and successful sends cannot create or change consent. For publication previews, activity uses neutral host templates such as preparing/reviewing text; agent-authored claims of approval are never used as status labels. Proposed publication bytes remain quoted preview content, even if those bytes contain the word "approved".
- Stream finalization means message delivery finished, not publication approved. The persistent exact-text message with digest-bound buttons is still a proposal awaiting a choice. Finalization, retries, restarts and Stop do not promote the proposal to approved.
- Only a validated approve-button callback creates a positive approval record, atomically bound to the immutable SHA-256 version described above. Reject records refusal. Free text, draft text and agent tool parameters cannot insert that record. Only after the callback transaction commits may the persistent decision message show the chosen approval; drafts are never edited to show an approval state.
- The publisher requires that callback-created record and a byte-identical digest. A complete preview, a sent message or an agent assertion is insufficient. Edited publication text invalidates the old buttons and requires a new callback.

This guarantees separation of preview, transport completion and approval authority. It does not attempt to infer truth from an LLM's prose: the approval record and publisher gate are deterministic Rust/application checks.

## 5. Config, secrets and restart recovery

Confirmed by the coordinator via br-3: the sole allowlist source is mint `~/.config/tuic-telegram/allowed_chat_ids`, mode 0600, one decimal chat ID per line, beside `bot.token`. Boss's private chat is already authorized there. Read this file for authorization; never copy its real IDs into the repository, fixtures, tests, logs or memory. Tests use fake IDs. Missing, empty, unreadable or malformed content fails closed; refresh authorization before accepting inbound updates or sending outbound work, so removal also revokes queued sends. Never learn authorization from `/start`, usernames or first contact.

Proposed `config.json` remains opt-in daemon configuration for `enabled`, a non-secret stable `bot_alias` and `target_tuic_session`; it does not duplicate the allowlist. For the initial single target, route authorized chats to that peer and use only allowlisted destinations. Multiple authorized destinations require an explicit outbound chat selector; never broadcast by accident. Represent chat IDs as decimal strings at JSON/MCP boundaries and validate them as signed integers in Rust. No live allowlist or token is read in phase 1.

The existing token path is `~/.config/tuic-telegram/bot.token`. Read it for each API request, keep it only for that request's duration, then discard it. Never put it in `AppState`, config serialization, environment, memory, logs, error strings or persisted retry work. Validate a regular owner-readable file and enforce owner-only access on mint. No token reads occur in this phase. A shared lock and state directory belong to this adapter config, so isolated TUIC instances cannot become duplicate owners of the same file. Test instances explicitly redirect all three paths under the checkout temp root.

Bot API URLs contain the secret: disable request URL tracing and map transport errors to typed safe errors without formatting a raw reqwest error. Persist method/body and non-secret identifiers, never a constructed URL. Use production HTTPS with the fixed API host; an injected loopback base exists only in tests.

- Network loss/5xx, nonpermanent rejections, malformed responses, capacity and journal failures: one bounded exponential backoff with jitter, 1–60 seconds; reuse the persisted cursor. Long poll for 25 seconds with a separately larger HTTP timeout. Cancellation terminates the outstanding request before another loop starts.
- 401/403/404: latch `unauthorized`, stop polling and outbound sends, persist a TUIC-visible alert, and require an explicit restart/re-enable after token rotation. No autonomous retry loop.
- 409: latch ownership conflict, stop polling and sends, alert that another poller or webhook conflicts. Do not remove a webhook or try to steal ownership. Operator resolves it and explicitly resumes.
- 429: honor response retry delay, clamped to 1 second–1 hour; log the safe category when the upper cap applies. Malformed update/API responses never advance a cursor blindly. Log safe error categories, not response bodies or chat content.

Oversized polling responses use an adaptive request limit: start at 100, halve down to 1 after each response above the 1 MiB batch cap, through the same backoff schedule. A successfully committed batch restores the limit to 100. At limit 1, allow an 8 MiB single-update response budget. This is an explicit finite allocation policy, not a documented universal Telegram byte bound. A response beyond that budget latches `oversize_update` durably, stops all further requests and leaves the cursor unchanged; explicit operator recovery is required. Never silently acknowledge or drop an oversize update.

Allowlist revalidation purges mail belonging to removed chats from the journal before another poll or offer, including already offered entries. The committed cursor stays unchanged so restoring authorization cannot resurrect purged mail. This purge covers adapter retention; a previously offered native inbox copy requires its own integration policy.

The source token and formatted request string use zeroizing buffers. Copies inside reqwest's parsed `Url` are **not zeroized**; this is a remaining memory limitation. Safe errors and logging still never expose request URLs or tokens.

Use a small SQLite journal (the repository already uses SQLite for progress) under the adapter state directory. Transactionally insert unique `(bot_alias, update_id)` records and pending mail/Stop/callback effects, then advance `next_offset` to `update_id + 1`. Only the committed offset may be sent in the next poll; failed journal writes leave it unchanged. Store only validated allowlisted payloads; rejected/irrelevant updates advance the cursor without retaining their text. Telegram acknowledgement is defined by the next higher-offset poll, per [getUpdates](https://core.telegram.org/bots/api#getupdates).

**First-start backlog policy (authorized by the coordinator):** br-3 reports two unacknowledged updates, `/start` and "Fatto". The adapter may discard the pending backlog on first initialization; this is not a restart policy and tests must not pin that count.

1. Under the sole-owner lock, create and commit journal state `bootstrap_started` before the first poll. Startup with an existing `initialized` row always uses its persisted nonnegative offset; it never enters discard mode again.
2. For the fresh initialization only, call `getUpdates(offset=-1, limit=1, timeout=0)` without a restrictive update filter. The documented negative offset reads the queue tail and forgets earlier updates. Do not dispatch or retain the returned old payload. Persist `initialized=true` and `next_offset=last_update_id+1` in one transaction; an empty successful result persists `initialized=true, next_offset=0`.
3. The next ordinary poll uses that committed nonnegative offset, acknowledging the last discarded update. Updates arriving after the sampled tail remain eligible for normal delivery. No second negative-offset poll runs during normal operation or token rotation.
4. If the call outcome or the subsequent local commit is uncertain, retain `bootstrap_started` and stop with a durable initialization alert. On restart, that state requires an explicit operator recovery decision; do not repeat `offset=-1` automatically, because a repeated discard could erase new phone messages. Journal corruption or disappearance after prior use also requires explicit reinitialization authorization, not an automatic "first start" reset. A fresh install is the only automatic discard path.

This intentionally changes Telegram queue state only in the later authorized implementation bootstrap. No polling occurs during design. [Negative-offset semantics](https://core.telegram.org/bots/api#getupdates)

**Critical gap:** the TUIC mailbox is a bounded in-memory `DashMap<VecDeque<AgentMessage>>` (`src-tauri/src/state.rs:2124`, capacity at `:1654`). Stable-ID deduplication in 1419 is also insufficient to claim disk durability. The adapter journal therefore retains channel mail until consumption, rehydrates it when the same peer re-registers after restart, and integrates stable-ID insertion plus inbox consumption receipts with the shared service. Journal mail IDs remain immutable; rehydration checks the live inbox under its identity/delivery lock. An inbox read records the durable consumed cursor before acknowledging removal. Capacity eviction must leave unconsumed channel mail pending, not delete it from the journal. Bound pending count/bytes and surface saturation; never silently evict accepted phone messages. Retention compacts only consumed/completed rows while preserving callback expiry and replay tombstones.

Do not equate this with exactly-once model execution. A crash after a read response is sent but before the model acts cannot be resolved by an offset. Likewise `sendMessage` has no idempotency field in this design: an accepted POST followed by a lost response is uncertain. Persist `delivery_uncertain`, do not blindly resend successful-looking final replies or approval posts, and surface recovery to TUIC. Boss accepted durable deduplication and explicit delivery uncertainty without blind resend; no stronger model acknowledgement is required in this scope. Do not promise lossless, duplicate-free semantic effects across that boundary.

## 6. Agent MCP surface

One native tool, `telegram`, registered in the existing tool registry/dispatch (`src-tauri/src/mcp_http/mcp_transport.rs:1207`). Authenticate via the existing MCP-bound peer; authorize only the configured target peer for its chat. Chat defaults to that peer's binding; a supplied chat ID must match an authorized binding. No token, arbitrary Bot API method, remote URL or target peer parameters.

```json
{"action":"send","text":"Exact authored text","buttons":[[{"label":"Approve","data":"opaque application payload"}]],"approval":{"artifact_id":"post-17","exact_text":"Exact authored text"},"idempotency_key":"post-17-v1"}
```

`buttons` and `approval` are optional. For approval buttons use explicit approve/reject choices bound by Rust, not arbitrary `data` as authority. Unknown fields are rejected. Keep text/body size bounded to the existing mail-sized envelope, then split messages in Rust. A durable idempotency key is required for send/finish; reuse with different content returns conflict.

| Action | Input / result |
| --- | --- |
| `begin` | `request_id`; accepts one queued request owned by caller; returns request/draft IDs and bound epoch. |
| `activity` | `request_id`, `text` (one safe line, at most 160 characters); replaces pending activity. |
| `finish` | `request_id`, `text`, `idempotency_key`; persists full reply and queues finalization; returns operation ID and `queued`, not fictitious sent success. |
| `send` | `text`, optional buttons/approval, `idempotency_key`; proactive persistent message, including publish previews. |
| `status` | `operation_id`; reports queued/sent/uncertain/failed and known message IDs without secret details. |
| `decision` | `decision_id`; returns immutable exact-text approval receipt to issuer, for deterministic publisher validation. |

Use the same Rust service through MCP HTTP and local bridge. No new Tauri command is necessary for an agent-only tool. If phase 2 adds a frontend status/config command, add its HTTP equivalent, command table mapping and route parity tests. Alerts must use an existing headless-observable journal/event path, not a desktop-only toast.

## 7. Tests and evidence

Use an in-process fake Bot API HTTP server, injected clock and fault injection at journal/service boundaries. Test the real Rust scheduler, persistence and native mail path; no Telegram, SSH or mint in automated tests. Telegram response/update fixtures must be sanitized captures supplied from a real Bot API session or recorded public reference examples. Do not manufacture fixtures and call them recorded: the required draft/Stop/error captures are currently unavailable and must precede protocol-fixture tests.

| Proposed test name | Plausible bug it catches / observable assertion |
| --- | --- |
| `telegram_unknown_chat_cannot_mail_or_send` | Non-allowlisted text, callbacks or Stop produce mail/PTY/API side effects; assert none and no retained private body. |
| `telegram_idle_mail_wakes_without_typing_payload` | Adapter bypasses native send arbitration; real inbox has JSON mail and PTY has only the generic wake. |
| `telegram_busy_mail_waits_for_safe_idle` | Message text or Enter reaches a running tool; no write until safe idle, then one wake. Also cover partial composer and active waiter. |
| `telegram_restart_restores_unconsumed_mail_once` | Offset persists but inbox vanishes; restart at insertion/dispatch/consumption boundaries and assert one stable mail record and no replay of consumed entries. |
| `telegram_first_start_discards_backlog_once` | A restart repeats negative-offset polling and drops new text; fake-server cases cover nonempty/empty tails, new arrivals and persisted bootstrap uncertainty, with fake chat IDs. |
| `telegram_allowlist_file_revokes_pending_sends` | A stale JSON allowlist or cached authorization leaks outbound messages after file removal; fake 0600 file changes revoke queued work. |
| `telegram_disk_failure_does_not_ack_update` | Cursor advances before durable commit; next poll keeps the old offset. |
| `telegram_401_latches_without_retrying` | Revoked credentials trigger an infinite loop; one failed request, durable alert, zero later calls. |
| `telegram_409_never_starts_a_second_owner` | Conflict or second instance races another poller; stopped owner and no automatic takeover. |
| `telegram_rotation_and_errors_never_retain_token` | Request URL/error or retry state leaks a credential; rotate synthetic secret file between calls and inspect logs/storage/errors. |
| `telegram_draft_survives_quiet_work_and_obeys_429` | No intent changes let the preview expire, or throttling is ignored; fake time proves refresh cadence and retry delay. |
| `telegram_finish_cancels_refresh_before_persistent_reply` | In-flight refresh revives a finished draft; server records ordered final chunks and no later refresh. |
| `telegram_split_preserves_unicode_and_whitespace` | Byte slicing corrupts emoji or trims the approved text; boundary cases reassemble exact input within budget. |
| `telegram_old_stop_cannot_interrupt_a_new_turn` | Reused draft/PTY mapping sends Esc to another task; matching Stop writes one Esc with bookkeeping, stale/replayed Stop writes none. |
| `telegram_callbacks_bind_chat_message_and_issuer` | Forged handle or message moves a choice to another agent; only valid callback produces structured mail, ack and selected-message edit. |
| `telegram_publish_preview_cannot_create_or_display_approval_state` | Forged agent approval prose or a finished draft masquerades as consent; preview framing remains explicit, draft schema rejects approval fields, no decision exists before a valid digest-bound callback, and the publisher refuses preview-only evidence. |
| `telegram_approval_records_exact_bytes_before_mail` | LLM or formatting approves different text; immutable digest/decision precedes mail, and one-byte/newline changes invalidate publishing. |
| `telegram_repeated_button_does_not_record_two_decisions` | Replayed callback, restart or ack/edit retry repeats publish consent; one decision and stable mail ID. |
| `telegram_unrelated_turn_cannot_replace_active_stream` | Generic busy/done events are attributed to the wrong request; begin/epoch binding filters them and missing finish stays incomplete. |
| `telegram_unknown_post_result_is_not_blindly_retried` | Lost send response duplicates final messages/approval keyboards; persisted uncertainty blocks resend. |

Reuse existing mail regressions, including `busy_claude_with_channel_gets_unread_mail_wake_on_idle` (`mcp_transport.rs:16701`) and `mcp_send_does_not_type_into_partial_claude_composer_with_background_work` (`:15850`). Add adapter boundary tests only for new integration behavior. Medium/high protocol and approval risk requires a separate coordinator critic before landing.

At the end of each implementation story, run only the relevant `telegram::tests::<filter>` plus changed native-mail consumers once, through `scripts/with-test-tmp.sh`, mbx and build-slot/tuic-bg per managed-peer rules. No compilation or test run in this design phase. Coordinator owns later suites/mutations on rb. The one live mint test remains gated by Boss's readiness and pe-3 being idle; do not deploy or restart mint from this design task.

## 8. Ordered landable slices

1. **Owner/config/Bot API boundary:** opt-in daemon startup, shared owner lock, strict allowlist, per-request secrets, typed safe failures and fake server. No live enablement.
2. **Durable inbound delivery:** journal transactions/offset, stable-ID native mail service integration, read receipts, rehydration and backpressure. Integrate landed 1419/1420; phone text remains mail-only.
3. **Correlated drafts and final replies:** MCP begin/activity/finish/send/status, epoch binding, refresh scheduler, plain-text splitting and uncertain-send recovery. Wire bound-agent instructions.
4. **Stop:** draft mapping, Esc/bookkeeping integration, stale-turn rejection and durable safe audit.
5. **Progress notices:** bound-peer blocked/done subscription, persisted progress replay cursor and language-preserving notifications.
6. **Buttons and approvals:** opaque handles, durable decisions, callback acknowledgement/edit retry, exact-text versioning and publisher receipt contract. Publisher integration must be identified before enabling approvals.
7. **Operator docs and coordinated live verification:** apply sync matrix MCP/remote/progress sections (`docs/sync-matrix.md:112`, `:149`, `:318`, `:409`): API/backend/user guides, FEATURES, SPEC, CHANGELOG and restart checklist as each behavior lands. Use the confirmed allowlist file, obtain the stable peer binding, authentic sanitized fixtures and Boss readiness; coordinator schedules mint deployment while pe-3 is idle.

## Approved decisions (Boss via br-3, 2026-10-03)

- Recipient: new coordinator-spawned managed Claude marketeer on mint with a stable UUID; never pe-3. Allowlist stays in allowed_chat_ids.
- Streaming: explicit begin/finish plus intents and authored safe activity accepted.
- Recovery: durable deduplication and delivery_uncertain without blind resend accepted.
- Consent: TUIC owns immutable callback-created SHA-256 receipts. The xkit publish path checks them before every publish; no local approval path.
- Phase 2: slices 1–2 offline only. 1419/1420 are not landed: implement narrow ports and leave native stable-ID mail integration explicitly deferred, with no cherry-picks. No Bot API, mint, token or SSH access. Live captures and deployment occur later with Boss present.

### Receipt lookup contract for xkit through tuic-bridge

The existing native `telegram` tool gains `action:"receipt_lookup"`. Input is `{receipt_id, subject:{kind:"item", artifact_id, sha256}}` or `{receipt_id, subject:{kind:"queue", queue_id, item_sha256s:[...]}}`. Unknown fields, noncanonical 64-lowercase-hex digests, empty queues and invalid IDs fail validation. No caller-supplied peer identity is accepted. The MCP connection bound by tuic-bridge to the managed marketeer TUIC_SESSION is the issuing peer; headerless/unregistered calls fail authentication. The same identity must have issued the proposal that owns the receipt. Cross-peer lookups fail authorization. Peer identity is authorization, not permission to manufacture a receipt.

Success is `{valid:true, receipt:{receipt_id, issuing_peer, chat_id, callback_id, approved_at_ms, subject, binding_sha256}}`. Only an existing callback-created positive record returns success, and subject/digest must match exactly. Return safe typed errors `unauthenticated`, `forbidden`, `invalid_input`, `receipt_not_found`, `not_approved`, `subject_mismatch`, `superseded`, `expired`, or `store_unavailable`; no error is an approval. Do not return proposal text or token. xkit invokes this through the local mint bridge for every publish, recomputing the exact item bytes first. It fails closed on any error, timeout, digest mismatch or uncertain result; there is no `./x approve` fallback. Lookup does not consume the immutable receipt. Revocation/supersession status is checked on every lookup.

For Mac Keyboard, Boss approves the queue once as a process. Store a queue proposal with its exact ordered item-digest list. Compute `binding_sha256 = SHA256(b"tuic-telegram-queue-v1\0" || count_u64_be || digest_1_raw_32_bytes || ... || digest_N_raw_32_bytes)`. Count and binary fixed-length encoding prevent concatenation ambiguity; order and duplicates are significant. The validated button callback creates that queue receipt. Adding, removing, changing or reordering an item changes the binding and requires a new proposal and button press. The publisher supplies the whole original ordered digest list for lookup and verifies the current item's exact bytes and queue position against it before each publish; remaining items are not represented as a shortened queue. A completed item cannot authorize newly appended work. Persist execution progress separately from approval. Receipt lookup uses the original issuing marketeer peer via its bridge; a Mac Keyboard runner must use that authorized publisher path, never impersonate or assert an issuing peer in tool arguments. Dedicated runner identity/delegation would need separate approval and is outside slices 1–2.

### Open operational inputs (not unresolved architecture)

- Coordinator supplies the new managed peer UUID at deployment.
- Authentic sanitized Bot API captures are acquired later on mint with Boss present. No live capture is required or claimed by offline slices 1–2.
- Live end-to-end and pe-3 handover remain coordinator-owned and require Boss readiness. Approval/publisher tooling is a later slice, not implemented by the current offline ports.

## Offline implementation status

Slice 1 adds strict opt-in file configuration, a process-held OS owner lock, per-request zeroized credentials and a safe typed HTTP boundary. The module is deliberately not started by the daemon until native integration lands; synthetic-secret loopback tests exercise it. The recorded public text-update fixture documents its source; no live API equivalence is claimed.

Slice 2 implements a SQLite inbound journal, first-start discard state, atomic offset/mail commit, bounded pending mail, stop latches and retry scheduling. `MailPort` and consumption receipts are narrow explicit ports; they do not yet insert into the native inbox or wake any real peer. Stable-ID native insertion, atomic native inbox consumption, daemon startup and operator recovery remain DEFERRED until integration/later slices. Offline tests verify only the persistence/port contract, not idle/busy daemon wake.
