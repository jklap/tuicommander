---
status: in_progress
approved_at: "2026-10-09T16:14:58.202Z"
updated: "2026-10-09T16:15:32.401Z"
started_at: "2026-10-09T16:15:32.401Z"
---
# Plan: Automations scheduler

**Created:** 2026-10-09 | **Status:** Approved for phased implementation | **Effort:** XL | **Branch:** feat/automations-core

## Summary

- Add scheduled agent runs with friendly cadence controls, durable history and clear needs-you states.
- This is a development assignment. Implement phase 1, then phase 2. Phase 3 is listed but not started; this peer stops after Step 1.

## Architecture Context

- Definitions → zone-aware due occurrence → SQLite reservation → workspace/precheck → configured agent → task/event evidence → durable final state.
- `config.rs::ConfigFile<T>` provides atomic JSON writes and locked strict read-modify-write; use it for `automations.json`.
- `workflows/run/{store,runtime}.rs` provides the owner-lock, restart reconciliation and reserve-before-effect patterns, not an automation implementation.
- `mcp_http/mcp_transport.rs::launch_daemon_workflow_agent` requires a real workflow activation. Share its underlying agent launch assembly; preserve its guard.
- `create_daemon_workflow_worktree` delegates to the common worktree routes. An automation needs its configured base branch passed through that path.
- `lib.rs` starts WorkflowRuntime in desktop and remote boot. The scheduler is another in-process runtime with no new daemon or plugin host.
- `tasks.rs`, `event_bus` and progress supply evidence; idle is not universal proof. Final unknown is required when evidence is absent.
- Fixed decisions: no session reuse; run-config permissions; one IANA zone per definition default local; overlap skip; global configurable limit default 2; max duration.
- ADR: reserve durably before dispatch and interrupt all open runs at boot without retry — prevents duplicate agent execution after a crash — transparent automatic retry rejected.

## Research Findings

- No Orca implementation code is incorporated. RRULE, multi-host ownership, external managers and estimated token spend are out of scope.
- [croner](https://docs.rs/croner/latest/croner/) offers next/previous zone-aware occurrence APIs; [chrono-tz](https://docs.rs/chrono-tz/latest/chrono_tz/) supplies IANA TimeZone implementations. Verify pinned APIs in Step 2.
- Strict ConfigFile loading preserves malformed JSON by moving it aside. Semantic/schema failures must abort writes and leave the original available for recovery.

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
| 2 | 1611-fa3f | 1610-ac85 |
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

## Steps


### Step 1: Definition storage

- **Phase:** Phase 1 — Rust core
- **Files:** `automations/{mod,model,definitions}.rs (new)`
- **Depends on:** None
- **Test contract:**
  - Persist typed automation definitions in automations.json through ConfigFile<T>, with per-id locked mutations that preserve unrelated definitions and global settings.
- **Constraint:** Strict storage preserves corruption; per-id mutations avoid array replacement races. Definitions contain literal prompts; Smart Prompt variable expansion is not available headless.
- **Validation:** test(automations::definitions::tests)

```rust
let file = ConfigFile::<AutomationsConfig>::new("automations.json");
file.update_with_strict(|latest| { latest.definitions.insert(id, definition); Ok(((), true)) })
```

### Step 2: Cron and timezone semantics

- **Phase:** Phase 1 — Rust core
- **Files:** `automations/schedule.rs (new), src-tauri/Cargo.toml, Cargo.lock`
- **Depends on:** Step 1
- **Test contract:**
  - Accept exactly five-field Vixie cron with day-of-month/day-of-week OR semantics; reject malformed and impossible schedules with bounded errors.
- **Constraint:** Use croner with chrono support plus chrono-tz; verify exact pinned API and DST policy against its source before adding it. No custom cron parser or RRULE in phase 1.
- **Validation:** test(automations::schedule::tests)

```rust
let zone: chrono_tz::Tz = timezone.parse().map_err(|_| "Invalid IANA timezone")?;
let zoned_now = now.with_timezone(&zone);
let next = cron.find_next_occurrence(&zoned_now, false)?;
```

### Step 3: Durable run ledger

- **Phase:** Phase 1 — Rust core
- **Files:** `automations/{run,store}.rs (new)`
- **Depends on:** Step 1
- **Test contract:**
  - Create versioned automation_runs.sqlite3 using workflows/run owner-lock, WAL/busy-timeout and transaction patterns; fail closed on unsupported schemas.
- **Constraint:** Record before dispatch. A database owner lock prevents desktop and remote sharing one config directory from dispatching twice; follow existing WorkflowRuntime ownership.
- **Validation:** test(automations::store::tests)

```rust
let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
tx.execute("INSERT INTO automation_runs (...) VALUES (...)", params![run_id, automation_id, occurrence])?;
tx.commit()?;
```

### Step 4: Scheduler admission and catch-up

- **Phase:** Phase 1 — Rust core
- **Files:** `automations/scheduler.rs (new)`
- **Depends on:** Step 2, Step 3
- **Test contract:**
  - Implement a deterministic tick against the real ledger: latest due occurrence only, grace-inclusive run-once catch-up, older skipped_missed and no replay backlog.
- **Constraint:** One 30s tick, no minute-by-minute wake replay. Do not launch a runtime with an unimplemented dispatcher; Step 6 wires boot after real dispatch exists.
- **Validation:** test(automations::scheduler::tests)

```rust
for due in latest_due(now, &definitions)? {
    store.reserve_if_admitted(due, definitions.max_concurrent_runs)?;
}
```

### Step 5: Bounded precheck execution

- **Phase:** Phase 1 — Rust core
- **Files:** `automations/precheck.rs (new), src-tauri/src/smart_prompt.rs`
- **Depends on:** Step 1
- **Test contract:**
  - Execute optional precheck in the resolved workspace with the existing clean shell/environment and bounded subprocess pattern; exit 0 proceeds, nonzero/timeout records skipped_precheck.
- **Constraint:** Use test_support host shell helpers; no freshly executed per-test script. Precheck context injection is phase 3.
- **Validation:** test(automations::precheck::tests)

```rust
let result = run_precheck(&workspace, &precheck.command, precheck.timeout_secs).await?;
if result.exit_code != Some(0) { store.skip_precheck(run_id, result)?; }
```

### Step 6: Dispatch and shared runtime boot

- **Phase:** Phase 1 — Rust core
- **Files:** `automations/{dispatcher,runtime}.rs (new), src-tauri/src/mcp_http/mcp_transport.rs, src-tauri/src/lib.rs`
- **Depends on:** Step 3, Step 4, Step 5
- **Test contract:**
  - Create a worktree per run or use the existing repository workspace; reuse create_daemon_workflow_worktree and common launch assembly behind launch_daemon_workflow_agent.
- **Constraint:** No desktop launch in agents. This helper currently creates from HEAD; pass the explicit configured base branch through the shared worktree API, preserving existing workflow behavior.
- **Validation:** test(automations::dispatcher::tests) | test(automations::runtime::tests)

```rust
let worktree = create_daemon_workflow_worktree(state, &definition.repository, &branch).await?;
let launch = launch_agent_effect(state, &definition.run_config, &worktree, &definition.prompt)?;
store.bind_launch(run_id, launch)?;
```

### Step 7: Completion and maximum duration

- **Phase:** Phase 1 — Rust core
- **Files:** `automations/completion.rs (new), automations/runtime.rs`
- **Depends on:** Step 6
- **Test contract:**
  - Consume task/PTy/progress signals for completed/failed/needs_you; idle alone is insufficient evidence of success. Handle broadcast lag by reconciling task/session state.
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

- **Phase:** Phase 1 — Rust core
- **Files:** `automations/api.rs (new), src-tauri/src/mcp_http/mod.rs, src-tauri/src/lib.rs, src/transport.ts, src/__tests__/transport.test.ts`
- **Depends on:** Step 2, Step 4, Step 6, Step 7
- **Test contract:**
  - Expose list/get/create/update/delete/run_now/list_runs, pause via update, schedule preview/presets and run aggregates through identical HTTP and Tauri shapes.
- **Constraint:** All runtime decisions remain Rust. Low-frequency changes use SSE and desktop emit; remote ownership must be explicit, never dispatch onto the wrong machine.
- **Validation:** targeted automations API nextest + transport.test.ts + pnpm exec tsc --noEmit

```rust
#[tauri::command]
async fn automation_action(input: AutomationAction, state: State<AppState>) -> Result<AutomationReply, String> {
    automations::api::execute(input, &state).await
}
```

### Step 9: Automations dialog

- **Phase:** Phase 2 — UI
- **Files:** `src/components/AutomationsDialog/ (new), src/stores/automations.ts (new), src/actions/actionRegistry.ts, src/components/CommandPalette/CommandPalette.tsx`
- **Depends on:** Step 8
- **Test contract:**
  - Add list, search, editor, cadence presets, cron/timezone preview, detail history, Run Now, pause/resume and delete with clear errors and empty states.
- **Constraint:** Visual verification required. No schedule computations or process orchestration in TypeScript. Browser/PWA use the same commands.
- **Validation:** targeted dialog/store Vitest + pnpm exec tsc --noEmit + screenshot after every visual change

```rust
const preview = await invoke("automation_action", { input: { action: "preview", cron, timezone } });
```

### Step 10: Runs view and output navigation

- **Phase:** Phase 2 — UI
- **Files:** `src/components/AutomationsDialog/RunsView.tsx (new), src/stores/automations.ts`
- **Depends on:** Step 9
- **Test contract:**
  - Render backend 24h/7d counts and paginated run history with needs_you/unknown/interrupted visible as distinct states.
- **Constraint:** Visual verification required. UTC windows are elapsed durations, and the backend owns aggregates.
- **Validation:** targeted runs-view Vitest + pnpm exec tsc --noEmit + screenshot

```rust
const summary = await invoke("automation_action", { input: { action: "summary", window: "7d" } });
```

### Step 11: Needs-you and failure notifications

- **Phase:** Phase 2 — UI
- **Files:** `automations/notifications.rs (new), existing native/web-push/Telegram adapters, AutomationsDialog`
- **Depends on:** Step 7, Step 9
- **Test contract:**
  - Notify once per run transition to needs_you or failed using existing channels/preferences; link the run and its owning session.
- **Constraint:** Visual verification required for bell changes. No vendor traffic fixtures invented; reuse internal envelope fault models and record limits.
- **Validation:** targeted notification nextest/Vitest + pnpm exec tsc --noEmit if frontend touched

```rust
if store.claim_notification(run_id, transition)? { notifications.publish(run_notice).await?; }
```

### Step 12: MCP automation tool

- **Phase:** Phase 3 — not started
- **Files:** `src-tauri/src/mcp_http/mcp_transport.rs, CLI HTTP adapter`
- **Depends on:** Step 8
- **Test contract:**
  - Phase 3 only: expose automation actions through MCP and CLI using the Step 8 core and identical validated shapes; add docs and protocol tests.
- **Constraint:** Not started. Requires explicit phase-3 authorization.
- **Validation:** targeted MCP/CLI parity tests

```rust
automation_api::execute(input, state).await
```

### Step 13: Built-in templates

- **Phase:** Phase 3 — not started
- **Files:** `automations/templates.rs (new), AutomationsDialog`
- **Depends on:** Step 9
- **Test contract:**
  - Phase 3 only: backend templates populate drafts with user-selected repo/run config and local zone, disabled until saved; no hidden automatic execution.
- **Constraint:** Not started. Requires explicit phase-3 authorization.
- **Validation:** targeted template tests + frontend tsc

```rust
let draft = templates::daily_review(repository, run_config, local_zone)?;
```

### Step 14: Worktree provenance and cleanup

- **Phase:** Phase 3 — not started
- **Files:** `automations/worktrees.rs (new), shared worktree lifecycle adapter`
- **Depends on:** Step 6, Step 10
- **Test contract:**
  - Phase 3 only: persist automation/run provenance and offer cleanup through existing integration/safety proof, preserving dirty/unmerged/unarchived worktrees and live sessions.
- **Constraint:** Not started. Requires explicit phase-3 authorization; never raw recursive deletion.
- **Validation:** targeted provenance/lifecycle tests

```rust
let safety = worktrees.lifecycle(&run.workspace)?;
if safety.removal_safe { worktrees.remove(&run.workspace)?; }
```

### Step 15: Precheck output as context

- **Phase:** Phase 3 — not started
- **Files:** `automations/precheck.rs, dispatcher.rs`
- **Depends on:** Step 5, Step 6
- **Test contract:**
  - Phase 3 only: explicitly opt in to bounded precheck stdout as prompt context with clear delimiters and exact original prompt preservation; cover empty/truncated/invalid output.
- **Constraint:** Not started. Requires explicit phase-3 authorization. Session reuse remains outside this plan.
- **Validation:** targeted precheck-context tests

```rust
let prompt = attach_precheck_context(&definition.prompt, &result.stdout, result.truncated);
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
