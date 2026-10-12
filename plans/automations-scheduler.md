---
status: in_progress
approved_at: "2026-10-09T16:14:58.202Z"
updated: "2026-10-09T16:15:32.401Z"
started_at: "2026-10-09T16:15:32.401Z"
---
# Plan: Automations scheduler

**Created:** 2026-10-09 | **Status:** Steps 1-15 complete (all linked stories `complete`); revision 2026-10-12 waits for Boss answers | **Effort:** XL + L (revision) | **Branch:** none (plan only)

## Summary

- Add scheduled agent runs with friendly cadence controls, durable history and clear needs-you states.
- Steps 1-15 landed (phase 3 Steps 13-14 too: 1622-be11, 1623-8b2a complete). Cron is stored and previewed but does not dispatch (see Revision).
- Revision 2026-10-12 (Boss, background-mode D5): routines and the scheduler are one Automations feature; a hook channel triggers a routine; a run can wake a hibernated agent.

## Architecture Context

- Definitions → zone-aware due occurrence → SQLite reservation → workspace/precheck → configured agent → task/event evidence → durable final state.
- `config.rs::ConfigFile<T>` provides atomic JSON writes and locked strict read-modify-write; use it for `automations.json`.
- `workflows/run/{store,runtime}.rs` provides the owner-lock, restart reconciliation and reserve-before-effect patterns, not an automation implementation.
- `mcp_http/mcp_transport.rs::launch_daemon_workflow_agent` requires a real workflow activation. Share its underlying agent launch assembly; preserve its guard.
- `create_daemon_workflow_worktree` delegates to the common worktree routes. An automation needs its configured base branch passed through that path.
- `lib.rs` starts WorkflowRuntime in desktop and remote boot. The scheduler is another in-process runtime with no new daemon or plugin host.
- `tasks.rs`, `event_bus` and progress supply evidence; idle is not universal proof. Final unknown is required when evidence is absent.
- Fixed decisions: no session reuse in phase 1 (memory `automations-scheduler-assignment-20261009`: "no phase-one session reuse"; the revision below proposes it, QA1); run-config permissions; one IANA zone per definition default local; overlap skip; global configurable limit default 2; max duration.
- ADR: reserve durably before dispatch and interrupt all open runs at boot without retry — prevents duplicate agent execution after a crash — transparent automatic retry rejected.

## Research Findings

- Prior-art scheduler read at source level (types, cron occurrence, dispatch); see [source analysis](../ideas/automations-scheduler.md).

- No external implementation code is incorporated. RRULE, multi-host ownership, external managers and estimated token spend are out of scope.
- [croner](https://docs.rs/croner/latest/croner/) offers next/previous zone-aware occurrence APIs; [chrono-tz](https://docs.rs/chrono-tz/latest/chrono_tz/) supplies IANA TimeZone implementations. Verify pinned APIs in Step 2.
- Strict ConfigFile loading preserves malformed JSON by moving it aside. Semantic/schema failures must abort writes and leave the original available for recovery.

## Delivery and wake design

- Checked 2026-10-09 against a prior-art scheduler (cron schedules, heartbeat/wake, result delivery); no code copied.
- Better single-user fit (product judgment): explicit result destinations/inspection links, separate execution and delivery outcomes, quiet no-change reports and bounded wakes.
- **Adopt, Steps 3/10/11:** saved output is the canonical report; notices link the run/session. Persist notification attempted/confirmed/unknown separately; deduplicate by run/transition/channel. Notification failures cannot change execution success. Do not replay ambiguous sends or historical notices on boot.
- **Adopt, Steps 4/7:** bounded wake reconciliation, reserve before execution, finite duration and completion provenance. Keep TUIC's simple 30s tick.
- **Adopt, Step 11:** precheck skips remain quiet; notify failure/needs-you through existing native/push/Telegram preferences. Completed output remains in history.
- **Reject:** model heartbeats (cost/context coupling); session reuse/result injection into replacement conversations (phase-1 exclusion; QA1 proposes an explicit `Target::Session`, not implicit reuse); stream watchers, interval/pacing and webhook/channel routing (YAGNI); automatic execution retries/restart dispatch (fixed no-retry decision); implicit last-conversation recipient (use run/session ownership).

## Security Considerations

- Automations use the professional user's own repo, prompt, shell and run config; do not invent per-automation unattended permissions.
- Reject invalid definitions before execution; preserve malformed/unsupported files, validate destination again before spawning, and terminate only the run's owned child/session.
- Run output may contain private project text. Follow existing config-directory ownership and authenticated HTTP transport; never log prompts or secrets routinely.

## Performance Considerations

- One 30s scheduler tick; query indexed recent/open runs, not entire history. Sleep/wake evaluates at most one latest occurrence per definition.
- Configurable concurrency default 2, overlap skip, finite max duration; no queued backlog. Capacity refusal is recorded as skipped_concurrency.
- Defaults: grace 12h; max duration 1h; both user-configurable. Output/precheck buffers bounded to 256 KiB per stream; history age retention default 90d, final runs only.
- Routine tests are targeted and use test temp isolation, mbx and build-slot. Coordinator owns full suites/mutations and headless cross-feature builds.

## Execution contract

- All `automations/` paths below are relative to `src-tauri/src/`; full acceptance criteria live in each linked story.
- DST: skip nonexistent fixed wall times; run fixed wall times once at the earlier fold instant. Validate impossible cron patterns with bounded search.
- Run Now works when paused, bypasses precheck, obeys overlap/cap and leaves scheduled cursors unchanged. Full capacity records skipped_concurrency, never queues.
- Open needs_you runs count toward overlap/cap and max duration. Boot interrupts every open state. Final transitions are immutable/idempotent; lost evidence becomes unknown.
- History survives deletion; final output/precheck streams truncate at 256 KiB. Age retention defaults to 90d and never deletes open runs.
- Every command shares one Rust implementation. API work includes preview/presets and aggregates, regenerated COMMAND_TABLE snapshot and registered-route probes.

## Story order

| Step | Story | Depends on |
|---|---|---|
| 1 | 1610-ac85 | None |
| 2 | 1611-fa3f, 1629-2262 (Once) | 1610-ac85 |
| 3 | 1612-70d1 | 1610-ac85 |
| 4 | 1613-67f3 | 1611-fa3f, 1612-70d1 |
| 5 | 1614-7e8f | 1610-ac85 |
| 6 | 1615-57cf | 1612-70d1, 1613-67f3, 1614-7e8f |
| 7 | 1616-1882 | 1615-57cf |
| 8 | 1617-290d | 1611-fa3f, 1613-67f3, 1615-57cf, 1616-1882 |
| 9 | 1618-685b | 1617-290d |
| 10 | 1619-b84d | 1618-685b |
| 11 | 1620-82ae | 1616-1882, 1618-685b |
| 12 | 1621-5dce | 1617-290d |
| 13 | 1622-be11 | 1618-685b |
| 14 | 1623-8b2a | 1615-57cf, 1619-b84d |
| 15 | 1624-c5f5 | 1614-7e8f, 1615-57cf |

- Acceptance criteria and concrete test cases for each step are recorded in the linked story files.

## Revision 2026-10-12: one Automations feature — routines, triggers, session targets

Boss on background-mode D5: "the scheduler must be able to revive them. We also need a hook channel to trigger a routine. The routine and the scheduler live together under Automations."

### One plan or two

Keep two plans, linked. The wake transition is session lifecycle and belongs to `plans/background-mode.md` → "Shared wake path" (`ensure_live`, Step 6). Automations only calls it. One merged plan would put tray, WebView unload and Linux packaging next to cron and hook semantics with no shared code. The single-path rule holds across the two plans: Automations adds no resume code.

### Today (source, 2026-10-12)

- **"Routine" does not exist as a concept.** An `AutomationDefinition` (`automations/model.rs:24-47`) already is the routine: prompt, run config, repository, workspace (`Existing | NewPerRun`) — plus exactly one schedule (`cron` or `once_local`). The revision names it a routine in user text only; no rename in code.
- **Recurring cron never dispatches.** `Dispatcher::reserve_due` drops every non-Once definition (`automations/dispatcher.rs:90`), and `require_once` refuses dispatch and Run Now for cron (`dispatcher.rs:68,204,222-227`: "Only Once automations dispatch in phase 1"). The user guide says so (`docs/user-guide/automations.md:7`, `docs/FEATURES.md:2800`). "The scheduler wakes a hibernated agent" therefore needs cron dispatch first.
- **Triggers:** `RunTrigger::{Scheduled{occurrence_ms}, Manual}` (`automations/run.rs:11-14`). Both use one admission with overlap and capacity (`scheduler.rs:14-65`, `store/admission.rs:12`).
- **Every run spawns a new agent.** `NativeEffects::launch` → `managed_launch::launch_named` → `agent action=spawn` (`dispatcher/native.rs:45-72`, `mcp_http/managed_launch.rs:29-60`). Completion reads the bound task, progress and exit (`completion.rs:12-21`). Maximum duration **kills** the owned session (`completion.rs:39-45`).
- **One transport-neutral API already exists:** `automations::api::execute` (`automations/api.rs:94-125`) behind MCP `automations` (`automations/mcp.rs:17`, 18 actions incl. `run_now`), HTTP `POST /automations/action` (`mcp_http/mod.rs:1010`, `mcp_http/automation_routes.rs:9-14`), IPC, and the CLI `tuic automations <json|->` (`crates/tuic-cli/src/main.rs:54-55`) over the local socket or named pipe (`crates/tuic-cli/src/ipc.rs:16-40`).
- **Untrusted text into a prompt** already has one escaper: `context.rs:7-60` (16 KiB cap, control characters stripped, `<precheck-output untrusted="true">`).
- The runtime is in Rust and independent of the WebView (`automations/runtime.rs:1`), booted on desktop and headless (`lib.rs:2651,2727`). It keeps working in background mode.

### Hook channel: options compared

| Option | What fires it | New surface | Fit |
|---|---|---|---|
| **(a) `trigger` action on the shared API** | anything that can run `tuic automations '{"action":"trigger",...}'`, call MCP or `POST /automations/action`: Claude Code / Codex hooks in the user's settings, git hooks, launchd, CI-local scripts, another agent | none: same API, same auth (local socket = OS user; remote HTTP = Basic Auth, `mcp_http/auth.rs`) | **recommended** |
| (b) internal event triggers | `event_bus` events (`state.rs:167-215`: `HeadChanged`, `RepoChanged`, `SessionClosed`, `PtyExit`) matched by a filter in the definition | a filter language and a second admission source | later, only if (a) shows a need; TUIC's own agent hooks (`agent_hook_installer.rs`) emit OSC 7770 for agent state, not routine events |
| (c) public webhook (GitHub, CI) | an internet caller | a public endpoint, per-hook secrets, replay protection | rejected (YAGNI, security); a GitHub Action can reach (a) through the existing remote access if Boss wants it |
| (d) file watcher | a path changes | a watcher per definition | rejected |

### Revised model

```rust
pub struct AutomationDefinition {        // existing fields kept
    // cron / once_local become optional together: none = trigger or Run Now only
    pub hook: Option<HookTrigger>,       // None = trigger action refused
    pub target: Target,                  // default NewSession = today
}
pub struct HookTrigger { pub inject_payload: bool }
pub enum Target { NewSession, Session { tuic_session: String } }
pub enum RunTrigger { Scheduled { occurrence_ms: i64 }, Manual, Hook { source: String, key: Option<String> } }
```

- A trigger is admitted like a scheduled occurrence: enabled required, precheck runs, overlap and capacity apply, refusals are recorded (`skipped_overlap`, `skipped_concurrency`), never queued (QA4).
- `key` deduplicates a retrying caller: one run per `(automation_id, key)`, kept with the run row.
- The payload (if `inject_payload`) goes through the `context.rs` escaper as `<hook-payload untrusted="true">`, same 16 KiB cap; the ledger records bytes, not text (QA3).
- `Target::Session`: dispatch calls `ensure_live(tuic_session, Automation)` (background-mode Step 6), then `enqueue_user_command` with the prompt. The run binds the session but does **not own** it: maximum duration records `timed_out` and leaves the session alone (QA5). Completion is evidence from that session after admission: `progress done|blocked` (already read, `completion.rs:19`) or the Protocol-rank turn end of the submitted turn; otherwise `unknown`.
- A target that is working or awaiting a human at dispatch records `skipped_target_busy`; hibernated or idle targets proceed (QA6).
- Validation at save: the target must be an agent tab with a resumable session; at dispatch a missing or non-resumable target fails the run with the `ensure_live` reason.

### Revision steps (story-sized)

| Id | Step | Size | Depends on |
|---|---|---|---|
| A1 | Recurring cron dispatch: remove `require_once` and the `reserve_due` filter; Run Now for cron. RED: `cron_occurrence_dispatches_once_per_occurrence` — catches: the phase-1 gate left on; `run_now_accepts_cron_definition` — catches: Run Now refused for cron | S | none (QA2) |
| A2 | Optional schedule + `Target` and `hook` fields, `deny_unknown_fields` kept; version stays 1 with `#[serde(default)]`. RED: `trigger_only_definition_has_no_next_run` — catches: a hook-only routine refused or scheduled | M | A1 |
| A3 | Hook channel (a): `trigger` action in `api.rs` + MCP schema + `tuic automations` docs; `RunTrigger::Hook`; payload injection; key dedupe. RED: `trigger_respects_pause_overlap_and_capacity` — catches: a hook bypassing admission; `same_key_admits_one_run` — catches: duplicate runs from a retrying hook; `payload_cannot_close_untrusted_block` — catches: prompt injection through the payload | M | A2 |
| A4 | `Target::Session` through `ensure_live`. RED: `scheduled_run_wakes_hibernated_target_and_delivers_prompt_once`; `timeout_never_kills_borrowed_session`; `busy_target_records_skipped_target_busy` | L | A2, background-mode B6 |
| A5 | Dialog: trigger section (schedule / hook / Run Now), target picker, copyable `tuic automations` hook snippet; Visual check | M | A3, A4 |

Docs per `docs/sync-matrix.md`: user guide, `docs/api/http-api.md` (route snapshot unchanged — same route), FEATURES, CHANGELOG.

### Open questions for Boss (Automations side)

- **QA1. Session target** (lifts "no phase-one session reuse")
  - (a) add `Target::Session`, run delivered into an existing — possibly hibernated — agent tab — **recommended** (without it "the scheduler wakes a hibernated agent" has nothing to wake: every run spawns a new agent)
  - (b) new session per run only; "wake" means only that runs keep firing in background
- **QA2. Enable recurring cron dispatch now (A1)**
  - (a) yes, as the first revision story — **recommended** (phases 1-2 are complete; cron is stored and previewed but never runs)
  - (b) keep Once-only
- **QA3. Hook channel**
  - (a) `trigger` action on the shared API (MCP, HTTP, IPC, `tuic` CLI) — **recommended**
  - (b) (a) plus internal `event_bus` triggers
  - (c) a public webhook endpoint
- **QA4. A hook-triggered run behaves like**
  - (a) a scheduled run: needs enabled, runs the precheck — **recommended** (a hook fires unattended)
  - (b) Run Now: ignores pause and precheck
- **QA5. Maximum duration on a session target**
  - (a) record `timed_out`, leave the session running — **recommended** (it is Boss's tab)
  - (b) interrupt the turn (Esc)
  - (c) kill the session as for owned runs
- **QA6. Session target busy at dispatch**
  - (a) record `skipped_target_busy` — **recommended** (matches overlap skip, no backlog)
  - (b) queue the prompt behind the current turn (idle gate)

## Steps


### Phase 1 — Rust core

### Step 1: Definition storage

- **Files:** `automations/{mod,model,definitions}.rs (new)`
- **Constraint:** Strict storage preserves corruption; per-id mutations avoid array replacement races. Definitions contain literal prompts; Smart Prompt variable expansion is not available headless.
- **Validation:** test(automations::tests)

```rust
let file = ConfigFile::<AutomationsConfig>::new("automations.json");
file.update_with_strict(|latest| { latest.definitions.push(definition); Ok(((), true)) })
```

### Step 2: Cron, Once and timezone semantics

- **Files:** `automations/schedule.rs (new), src-tauri/Cargo.toml, Cargo.lock`
- **Constraint:** Use croner with chrono support plus chrono-tz; verify exact pinned API and DST policy against its source before adding it. No custom cron parser or RRULE in phase 1. Once stores one local date-time with the same IANA zone; reject past instants at creation and spring gaps, resolve folds to the earlier instant. Keep the definition after its one scheduled occurrence is consumed, and derive completed state from the durable cursor.
- **Validation:** test(automations::schedule::tests)

```rust
let zone: chrono_tz::Tz = timezone.parse().map_err(|_| "Invalid IANA timezone")?;
let zoned_now = now.with_timezone(&zone);
let next = cron.find_next_occurrence(&zoned_now, false)?;
```

### Step 3: Durable run ledger

- **Files:** `automations/{run,store}.rs (new)`
- **Constraint:** Record before dispatch. A database owner lock prevents desktop and remote sharing one config directory from dispatching twice; follow existing WorkflowRuntime ownership.
- **Validation:** test(automations::store::tests)

```rust
let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
tx.execute("INSERT INTO automation_runs (...) VALUES (...)", params![run_id, automation_id, occurrence])?;
tx.commit()?;
```

### Step 4: Scheduler admission and catch-up

- **Files:** `automations/scheduler.rs (new)`
- **Constraint:** One 30s tick, no minute-by-minute wake replay. Do not launch a runtime with an unimplemented dispatcher; Step 6 wires boot after real dispatch exists.
- **Validation:** test(automations::scheduler::tests)

```rust
for due in latest_due(now, &definitions)? {
    store.reserve_if_admitted(due, definitions.max_concurrent_runs)?;
}
```

### Step 5: Bounded precheck execution

- **Files:** `automations/precheck.rs (new), src-tauri/src/smart_prompt.rs`
- **Constraint:** Use test_support host shell helpers; no freshly executed per-test script. Precheck context injection is phase 3.
- **Validation:** test(automations::precheck::tests)

```rust
let result = run_precheck(&workspace, &precheck.command, precheck.timeout_secs).await?;
if result.exit_code != Some(0) { store.skip_precheck(run_id, result)?; }
```

### Step 6: Dispatch and shared runtime boot

- **Files:** `automations/{dispatcher,runtime}.rs (new), src-tauri/src/mcp_http/mcp_transport.rs, src-tauri/src/lib.rs`
- **Constraint:** No desktop launch in agents. This helper currently creates from HEAD; pass the explicit configured base branch through the shared worktree API, preserving existing workflow behavior.
- **Validation:** test(automations::dispatcher::tests) | test(automations::runtime::tests)

```rust
let worktree = create_daemon_workflow_worktree(state, &definition.repository, &branch).await?;
let launch = launch_agent_effect(state, &definition.run_config, &worktree, &definition.prompt)?;
store.bind_launch(run_id, launch)?;
```

### Step 7: Completion and maximum duration

- **Files:** `automations/completion.rs (new), automations/runtime.rs`
- **Constraint:** Do not equate agent idle with completed; unsupported detection must remain honest. Maximum-duration cancellation cannot target another task.
- **Validation:** test(automations::completion::tests)

```rust
match evidence {
    CompletionEvidence::ConfirmedDone => store.finish(id, RunStatus::Completed)?,
    CompletionEvidence::Lost => store.finish(id, RunStatus::Unknown)?,
    _ => {}
}
```

### Step 8: HTTP and IPC parity

- **Files:** `automations/api.rs (new), src-tauri/src/mcp_http/mod.rs, src-tauri/src/lib.rs, src/transport.ts, src/__tests__/transport.test.ts`
- **Constraint:** All runtime decisions remain Rust. Low-frequency changes use SSE and desktop emit; remote ownership must be explicit, never dispatch onto the wrong machine.
- **Validation:** targeted automations API nextest + transport.test.ts + pnpm exec tsc --noEmit

```rust
#[tauri::command]
async fn automation_action(input: AutomationAction, state: State<AppState>) -> Result<AutomationReply, String> {
    automations::api::execute(input, &state).await
}
```

### Phase 2 — UI

### Step 9: Automations dialog

- **Files:** `src/components/AutomationsDialog/ (new), src/stores/automations.ts (new), src/actions/actionRegistry.ts, src/components/CommandPalette/CommandPalette.tsx`
- **Constraint:** Visual verification required. No schedule computations or process orchestration in TypeScript. Browser/PWA use the same commands.
- **Validation:** dialog/store Vitest; tsc; screenshots

```rust
const preview = await invoke("automation_action", { input: { action: "preview", cron, timezone } });
```

### Step 10: Runs view and output navigation

- **Files:** `src/components/AutomationsDialog/RunsView.tsx (new), src/stores/automations.ts`
- **Constraint:** Visual verification required. UTC windows are elapsed durations, and the backend owns aggregates.
- **Validation:** runs-view Vitest; tsc; screenshots

```rust
const summary = await invoke("automation_action", { input: { action: "summary", window: "7d" } });
```

### Step 11: Needs-you and failure notifications

- **Files:** `automations/notifications.rs (new), existing native/web-push/Telegram adapters, AutomationsDialog`
- **Constraint:** Visual verification required for bell changes. No vendor traffic fixtures invented; reuse internal envelope fault models and record limits.
- **Validation:** notification nextest/Vitest; tsc for frontend

```rust
if store.claim_notification(run_id, transition)? { notifications.publish(run_notice).await?; }
```

### Step 12: MCP automation tool

- **Files:** `src-tauri/src/mcp_http/mcp_transport.rs, CLI HTTP adapter`
- **Constraint:** Phase 2 (Boss, 2026-10-09). Share the backend API with HTTP/IPC; no separate scheduler logic.
- **Validation:** targeted MCP/CLI parity tests

```rust
automation_api::execute(input, state).await
```

### Step 15: Precheck output as context

- **Files:** `automations/precheck.rs, dispatcher.rs`
- **Constraint:** Phase 2 (Boss, 2026-10-09). Session reuse remains outside this plan.
- **Validation:** targeted precheck-context tests

```rust
let prompt = attach_precheck_context(&definition.prompt, &result.stdout, result.truncated);
```

### Phase 3 — not started

### Step 13: Built-in templates

- **Files:** `automations/templates.rs (new), AutomationsDialog`
- **Constraint:** Not started. Requires explicit phase-3 authorization.
- **Validation:** targeted template tests + frontend tsc

```rust
let draft = templates::daily_review(repository, run_config, local_zone)?;
```

### Step 14: Worktree provenance and cleanup

- **Files:** `automations/worktrees.rs (new), shared worktree lifecycle adapter`
- **Constraint:** Not started. Requires explicit phase-3 authorization; never raw recursive deletion.
- **Validation:** targeted provenance/lifecycle tests

```rust
let safety = worktrees.lifecycle(&run.workspace)?;
if safety.removal_safe { worktrees.remove(&run.workspace)?; }
```

## Acceptance Criteria

- [ ] Phase 1 reserves before dispatch, evaluates stored zones correctly, recovers without retries and exposes identical IPC/HTTP behavior.
- [ ] Phase 2 supports cadence editing, pause, Run Now, history, 24h/7d summaries and needs-you navigation with verified visuals.
- [ ] Only targeted evidence is claimed per story; coordinator verifies integration before landing.

## Checklist

- [ ] Backend/config/API docs and route snapshots follow docs/sync-matrix.md.
- [ ] UI docs, translations and screenshots follow STYLE_GUIDE.md.
- [ ] Every Rust story adds to-test.md restart checks; no agent launches desktop or builds in main.
- [ ] Phase 3 remains pending until authorized. Session reuse, RRULE and token spend are excluded.
