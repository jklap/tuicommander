---
status: approved
approved_at: "2026-10-12T12:07:42.616Z"
updated: "2026-10-12T12:07:42.617Z"
---
# Plan: Background mode (close to tray, unload the WebView, hibernate idle agents)

**Created:** 2026-10-12 | **Status:** Decisions recorded 2026-10-12; revision for D5 wake and Automations | **Effort:** XL (9 stories) | **Branch:** none (plan only)

## Summary

When the user clicks the window X, TUIC can keep running in the background: the Rust core keeps PTYs, the HTTP/MCP server (mobile, remote, agents), Automations and Telegram. The UI shrinks to a tray or menu-bar icon. After a delay the WebView is destroyed to free its memory, and idle agent tabs can hibernate (PTY ended, tab kept, resume on demand). Every automatic behavior has a setting.

## Architecture Context

- Rust owns everything that must survive: PTYs (`AppState.session_maps.sessions`, `state.rs:1436`), HTTP server (`mcp_http::start_server`, own thread and runtime), Automations (`automations/runtime.rs`: "independent of the WebView lifecycle"), workflows, `idle_close`, standby, push. `tuic-remote` proves it: the same background tasks run with no WebView (`spawn_daemon_background_tasks`, `lib.rs:2725`, with a parity test).
- Close handling is today **only in the WebView**: `getCurrentWindow().onCloseRequested` (`useAppBootstrap.ts:154`) → quit dialog when `confirm_before_quit` (`config.rs:963`) and sessions exist → `forceQuit` (`App.tsx:490`) closes the PTYs and destroys the window, and Tauri exits.
- Rust window hooks: `WindowEvent::Destroyed` → `release_webview_document_resources` (`lib.rs:1537`, `:1830`, tested); `Focused` → `desktop_window_focused` (`lib.rs:1904`), which opens the mobile push gate when false (`state.rs:4518`); `RunEvent::Reopen` shows "main" (`lib.rs:2575`); single instance (release only) unminimizes and focuses "main" but does not `show()` (`lib.rs:1848`).
- A WebView reload already keeps every PTY: the frontend re-adopts the surviving sessions, including unknown ones (`useAppInit.ts:963-1040`). Recreating the window reuses this path.
- WebView-only duties: the tab and pane model with a 30 s snapshot (`useAppInit.ts:~331`), JS plugins (`plugins/cache-keepalive`, `claude-wakeup` and `at-capacity-retry` write to agent PTYs), completion sounds, the updater check (`App.tsx`), `session action=suspend` (frontend-mediated, `suspendTerminal.ts`, needs `ui_attached()`), and `ui` MCP screenshot, tab and toast.
- Managed sub-agent unload **already exists**: `idle_close.rs` closes a child with a `session_parent` after `agents.<type>.idle_close_minutes` (default 15, `config.rs:1920`). It sends `{"type":"state_change","state":"closed","reason":"idle_timeout"}` to the parent and wakes it. It never closes on unread mail, `background_work`, a working or starting agent, `keep_open`, a BLOCKED hold with a live parent, a live `__bg-runner`, or a failed bg wake.
- Standby (`pty/lifecycle.rs:3786`) SIGSTOPs idle sessions that are **not visible**. Visibility comes only from the frontend (`set_session_visible`, `terminals.ts:633`), so the active tab of a hidden window never parks.
- ADR: the close decision moves to Rust — the WebView may not exist when close or quit is requested — keeping the JS `onCloseRequested` interceptor rejected (two paths for one transition).
- ADR: no PTY hand-over to `tuic-remote` — PTYs are children of the desktop process and no hand-over mechanism exists (no fd passing, VT state or agent state transfer) — the "daemon always owns the PTYs" model rejected for this plan (it is the `ideas/remote-daemon.md` client and daemon architecture, an XL rewrite).

## Research Findings

- **Measured memory (2026-10-11, `~/Gits/.tmp/results/inv-mem-1791704357.md`)**, with 8 to 20 sessions: main process 476-529 MB, WebContent 1620-1842 MB (WebKit Malloc 864, graphics 417, WebAssembly 224 MB), GPU 276-323 MB, Networking 12 MB. Each agent CLI uses 120-350 MB RSS. A reload did **not** shrink WebContent (1842 → 1620 MB): the process survives a reload.
- Conclusion: with many idle agents, **agent processes cost more than the WebView** (20 × 120-350 MB = 2.4-7 GB). Hibernation is as important as the WebView unload.
- **Options:**

| Option | Memory freed | Keeps working | Restart risk | Platforms |
|---|---|---|---|---|
| (a) hide only | ~0 MB: wry `hide()` is `setHidden` (wry `wkwebview/mod.rs:1052`). Rendering stops (inference). Windows `MemoryUsageLevel::Low` via `with_webview` (wry `lib.rs:2368`) is not measured | everything, including JS plugins | none | all |
| (b) hide, then destroy the WebView | macOS spike: at least 1,090 MB freed with a 1 GiB page allocation; WebContent exits, GPU and Networking remain (Step 0 results below). TUIC workload and other platforms are unmeasured | Rust core; JS plugins, sounds and the updater stop | low: the reload/re-adopt path is used daily. Re-open cost: 9 paint stalls of 223-857 ms in 274 s after a reload with 20 terminals (measured) | macOS WKWebView measured; Windows WebView2 and Linux WebKitGTK unmeasured |
| (c) hand over to `tuic-remote` | ~2.4 GB (the whole desktop process) | the daemon set only | high: no PTY hand-over exists, so every PTY is lost or needs fd passing | Windows ConPTY hand-over is harder |

- **Prior art** (one existing desktop app, source read 2026-10-08):
  - Close-to-tray: Windows only, default off, and only `hide()`s the window. One notice on the first hide: "still running in the system tray".
  - macOS: a menu-bar item (default on); closing the last window never quits. Linux: no tray (the tray code is a no-op there).
  - Tray menu: Open, Settings, Check for Updates (macOS), Quit. An amber attention dot appears when an agent finishes or a bell rings while the window is hidden; it clears on show.
  - PTYs live in a detached daemon that survives quit and crash.
  - **Agent hibernation**: an experimental setting (default off) with an idle threshold (default 30 min, range 1 min to 24 h). It kills the PTY of a done, untouched, resumable agent that is not in a foreground worktree, not driven by mobile, with no orchestration dispatch pending and no live sub-agent. Resume uses `claude --resume <id>` and similar when the worktree is opened. Non-resumable agents never hibernate. No notification was found.
  - **Copy:** the tray attention dot, the one-time notice, hibernation eligibility and resume, and per-agent resumability. **Do not copy:** Windows-only close-to-tray, a renderer that stays alive, or no setting for Linux.
- **Tauri 2.12.1** (Cargo.lock): `tray-icon` is not enabled (`Cargo.toml:179`).
  - `ExitRequested{code: None}` fires when the last window is destroyed and needs `prevent_exit()`. `Some(code)` is a programmatic exit and must pass (`tauri app.rs:225-232`).
  - `set_activation_policy(Accessory)` hides the Dock icon.
  - Linux tray: needs `libayatana-appindicator` or `ksni`, has no click events (tray-icon #104), and stock GNOME shows no icon.

## Security Considerations

- No new network surface: the tray only calls in-process commands.
- In background the HTTP/MCP server keeps its current auth. Background mode does not change the bind or the remote exposure.
- The forced snapshot and the hibernation records go in the config dir with the same permissions as `repositories.json`. A resume command holds argv and env names, not secrets: keep the existing `agentLaunchCommand` rules.

## Performance Considerations

- The tray status line reads the `SessionState` counts on change events, not with a timer. No polling.
- Recreating the WebView pays the reload cost (above). Pre-create nothing: a hidden spare WebView would defeat the purpose.
- Hibernate and unload timers must use time that does not count sleep, or a wake from a night of sleep hibernates everything at once. `cpu_watchdog` already detects sleep and wake; reuse its signal.
- Standby in background: all sessions become not visible, so idle shells SIGSTOP after `standby_timeout_minutes` (default 5). This saves CPU, not RSS.

## Boss decisions (answered 2026-10-12, all as recommended)

Source: `~/Gits/personal/orchestrator/reports/tuicommander/2026-10-12-background-mode-decisions.md`. The separate PTY daemon is abandoned.

- [x] **D1.** `close_behavior` default `ask` the first time, with "remember".
- [x] **D2.** Keep `confirm_before_quit`, only on the real quit path (tray Quit, Cmd+Q, `close_behavior=quit`).
- [x] **D3.** Hide at once; destroy the WebView after `background.webview_unload_minutes` (default 10, 0 = never). Step 0 passes on macOS: at least 1,090 MB freed in each of two runs. D3 stands; option (a) is not selected. This does not establish savings on Windows or Linux.
- [x] **D4.** Accept that `cache-keepalive`, `claude-wakeup` and `at-capacity-retry` stop while the WebView is unloaded; say so in the setting help.
- [x] **D5.** `agents.hibernate_idle_minutes` default 60, scope `background_only`.
  - Boss addition: the Automations scheduler must be able to wake a hibernated agent; a hook channel triggers a routine; routine and scheduler are one feature under Automations. See "Shared wake path" and `plans/automations-scheduler.md` → "Revision 2026-10-12".
- [x] **D6.** Managed sub-agents keep `idle_close` (`idle_close.rs`, 15 min, parent told `idle_timeout`).
- [x] **D7.** Input to a hibernated session resumes it, waits until ready, then delivers.
- [x] **D8.** Session suspend moves from the frontend (`src/utils/suspendTerminal.ts:49-110`) to Rust.
- [x] **D9.** No scrollback for a hibernated agent; the resumed agent redraws its conversation.
- [x] **D10.** Linux without a tray: `background` falls back to minimize, and Settings says why.
- [x] **1771-c6fb.** The update relaunch waits until agents are idle (can be forced).
- [x] **1772-25c5.** After the update restart, every agent tab resumes at once.

### Revision decisions (Boss, 2026-10-12)

Source: `~/Gits/personal/orchestrator/reports/tuicommander/2026-10-12-bg-automations-revision.md`.

- [x] **QB1.** Wake tabs on demand: a hibernated tab wakes when it is opened or when input, mail or a run targets it. The window show does not wake every tab.
- [x] **QA1.** An Automation may target an existing session (`Target::Session`, possibly hibernated). This lifts the phase-one rule "every run spawns a new agent".
- [x] **QA3.** The trigger action is on the existing API (MCP, HTTP, IPC, `tuic` CLI) **plus** internal event triggers (for example, "a branch landed" starts a routine). No public webhook.
- [x] **QA6.** When the target session is busy at dispatch, the run is queued after the current turn (not skipped). This replaces the recommended `skipped_target_busy`.

Coordinator decisions, already in the report:

- [x] **QB2.** One resume path: 1772-25c5 uses `ensure_live`, after B6.
- [x] **QA2.** Recurring cron dispatch is enabled now, as story A1 (a defect: `dispatcher.rs:90` filters cron out).
- [x] **QA4.** A hook-triggered run behaves like a scheduled run: the definition must be enabled and its precheck runs.
- [x] **QA5.** When a run on a session target times out, record `timed_out` and leave the tab alone.

## Shared wake path (one transition, one code path)

Today a session can be "asleep" in two ways, and each has its own wake:

| State | Set by | Wake today | Where |
|---|---|---|---|
| standby (SIGSTOP) | `pty/lifecycle.rs:3786` | `wake_session` sends SIGCONT | `pty/lifecycle.rs:3955`; callers `pty/commands.rs:513,1219`, `pty/lifecycle.rs:4006`, `pty/injection.rs:1429` (orchestrator mail), `mcp_http/session.rs:1093`, `mcp_http/mcp_transport_peer.rs:1888` |
| suspended (PTY ended, tab kept) | `suspendTerminal` (frontend) | `resumeTerminal` sets `pendingInitCommand`; the Terminal component spawns a PTY and types the command at first shell idle | `src/utils/suspendTerminal.ts:101-110`, `src/components/Terminal/Terminal.tsx:1222-1245`, `src/components/TabBar/TabBar.tsx:330` |
| restored after restart | snapshot restore | `verifyAndBuildResumeCommand` → `pendingResumeCommand`, lazily, when the branch is selected | `src/hooks/git/createBranchSelectionCoordinator.ts:319-340`, `src/utils/agentSession.ts:219-262` |

The resume half lives only in the WebView. With the WebView unloaded (D3) nothing can resume, so D7 and the scheduler wake cannot work on it.

**Revision:** one Rust entry point owns "make this session live": `ensure_live(state, tuic_session, reason) -> Result<String /* pty session id */, WakeError>`.
- Live and in standby → today's `wake_session` (unchanged; it becomes the first arm).
- Hibernated or suspended → spawn a PTY in the stored cwd with the stored resume command and the **same** `TUIC_SESSION` (`bind_pty_identity`, `pty/commands.rs:52-57`). `live_pty_by_tuic_session` (`state.rs:1993`, read by `live_pty_for_peer`, `state.rs:3281`) then resolves the peer again with no re-registration. Resume verification moves from `agentSession.ts:219` to Rust; it already calls the Rust `verify_agent_session` (`agent_session.rs:687`).
- Single flight per `tuic_session`: two concurrent callers get the same new PTY, never two.
- Input waits on the existing idle gate: the caller enqueues on the returned PTY with `enqueue_user_command` (`pty/injection.rs:2184`, `pending_injections`), which flushes at the first BUSY→IDLE. No second queue.

Callers (each replaces its own resume code, none adds one):

| Caller | Today | After |
|---|---|---|
| `session submit` / Compose queue | `Session not found` for a dead PTY (`pty/injection.rs:2198-2199`) | `ensure_live`, then enqueue; reply `{status:"resuming"}` |
| peer mail | inbox only while no live PTY (`pty/injection.rs:1463-1490`) | `ensure_live`, then the existing wake |
| user opens the tab (desktop, mobile) | `resumeTerminal` (TS) | IPC/HTTP `resume_session` → `ensure_live` |
| Automations run targeting a session | not possible (new session per run) | `ensure_live`, then enqueue the prompt (automations plan, A4) |
| hook-triggered routine | not possible | through the same Automations dispatch (automations plan, A3) |
| 1772 eager resume after update | lazy branch-select path | `ensure_live` for every tab with a verified id |

## Steps

### Step 0: Spike — does destroying the WebView free memory? (no repository code)
- **Test:** a throwaway Tauri 2.12.1 app under `~/Gits/.tmp/bgmode-spike/` loads a page that allocates 1 GiB in touched typed arrays. Not TUIC, so no dictation or hotkey conflict.
- **Measure:** `footprint` of the WebContent, GPU and main pids in 4 states: visible, `hide()`, `destroy()` + `prevent_exit`, recreated. Also check whether the WebContent pid exits.
- **Windows:** the same app with `set_memory_usage_level(Low)` on hide, then destroy (manual or a CI runner).
- **Code:**
```rust
.on_window_event(|w, e| if let WindowEvent::CloseRequested { api, .. } = e { api.prevent_close(); let _ = w.hide(); })
.run(|app, e| if let RunEvent::ExitRequested { code: None, api, .. } = e { api.prevent_exit(); })
```
- **Acceptance:** a table of the numbers in this plan. If destroy frees less than 500 MB on macOS, D3 falls back to (a).

#### Step 0 results — 2026-10-14 (story 1801-3689)

**Result:** destroy frees more than 500 MB on macOS. D3 stands. The conservative reduction from hidden to destroyed is **1,090.013 MB in run 1** and **1,090.160 MB in run 2** (minimum hidden total minus maximum destroyed total within each run).

Native environment: macOS 27.0.1 (26A434), arm64; Tauri and tauri-runtime-wry 2.12.1, wry 0.57.0. The app uses the default persistent WKWebsiteDataStore. `CFFIXED_USER_HOME` redirects its native home under the spike directory; a Foundation `NSHomeDirectory()` probe verifies that path. The page retains sixteen 64 MiB `Uint8Array`s, filled with random bytes through `crypto.getRandomValues` to touch every page and resist compression. Its readiness response reports exactly 1,073,741,824 allocated bytes on initial load and recreation. This is a synthetic allocation, not a measurement of TUIC terminals or canvases.

Two independent app launches each run visible → `hide()` → `destroy()` → recreated. Each state has two samples (four samples per row), after a three-second settling interval, with two seconds between samples. `footprint -f bytes <main-pid> <associated-WebKit-pids...>` measures physical footprint; MB below means 1,000,000 bytes. Process attribution uses before/after process inventories and requires exactly one new WebContent PID at initial readiness. Totals use footprint's combined-process summary; column extrema need not occur in the same sample.

| State | Main MB | WebContent MB | GPU MB | Networking MB | Total MB | Original WebContent |
|---|---:|---:|---:|---:|---:|---|
| Visible | 33.244–33.736 | 1,089.816–1,090.095 | 15.336–15.910 | 8.700–8.864 | 1,147.474–1,147.670 | Alive |
| Hidden | 33.195–33.605 | 1,088.997–1,089.341 | 12.174–12.780 | 8.569–8.651 | 1,143.148–1,143.607 | Alive |
| Destroyed | 32.900–33.244 | 0 (exited) | 11.699–12.371 | 8.324–8.422 | 52.988–53.430 | Exited |
| Recreated | 34.719–34.948 | 1,088.817–1,088.931 | 14.894–15.451 | 8.274–8.356 | 1,146.458–1,147.392 | Replaced by new PID |

Run 1: main PID 74242 survives throughout; WebContent 74375 exits after destroy, and recreation starts 79616. Run 2: main PID 13163 survives throughout; WebContent 13250 exits, and recreation starts 18229. Both app logs record `EXIT_REQUESTED_NONE prevented`. GPU and Networking survive destruction in both runs: unload must not promise that every WebKit process exits. Hide alone retains the 1 GiB allocation.

Raw evidence: `~/Gits/.tmp/results/1801/run{1,2}-{visible,hidden,destroyed,recreated}-{1,2}-footprint.txt`, matching `-processes.json` files, `run{1,2}-process-identification.json`, `run{1,2}-app.log`, `measure-persistent.log`, `measurements.json`, `summary.json`, `platform.txt` and `native-home.txt`. Source and runner remain under `~/Gits/.tmp/bgmode-spike/`; the build-output link is removed after measurement. Earlier nonpersistent-store control runs are retained separately under `~/Gits/.tmp/results/1801/nonpersistent/` and are not used for this table. Only the spike's own processes are stopped; TUIC is untouched.

- **[MANUAL] Windows:** no Windows machine is available. `set_memory_usage_level(Low)` and destroy savings remain unmeasured. Do not promise measured Windows memory savings.
- **Limit:** Linux, long-lived process-pool behavior and actual TUIC document teardown are unmeasured. Step 4 must verify its integration separately.

### Step 1: Rust owns close and quit (`close_behavior`)
- **Test:** `src-tauri/src/background_mode.rs` with a pure transition table.
  - RED: `close_with_background_hides_and_never_exits` — catches: X still quitting the app.
  - RED: `cmd_q_and_tray_quit_always_quit_even_in_background` — catches: quit swallowed by background.
  - RED: `exit_requested_none_is_prevented_only_in_background` — catches: an app that cannot quit, or one that dies when the last window is destroyed.
  - RED: `panel_window_close_closes_only_the_panel` — catches: a detached panel X sending the app to background.
- **Implement:** `on_window_event` `CloseRequested` for "main" calls `prevent_close()`, then `decide(close_behavior, has_sessions, confirm_before_quit)`. `Ask` and `ConfirmQuit` emit `close-dialog` (dual-emit on `event_bus`). The frontend answers with `resolve_close {choice, remember}`. Delete the JS `onCloseRequested` interceptor (single path). The `ask` dialog offers Background, Quit and Cancel, plus "Remember".
- **Settings:** `app.close_behavior` (`ask|background|quit`) with catalogue, i18n and a GeneralTab control; template `standby_timeout_minutes`.
- **Code:**
```rust
pub(crate) enum CloseAction { Hide, Quit, Ask, ConfirmQuit }
pub(crate) fn decide(b: CloseBehavior, sessions: usize, confirm: bool) -> CloseAction {
    match b { CloseBehavior::Background => CloseAction::Hide, CloseBehavior::Ask => CloseAction::Ask,
              CloseBehavior::Quit if confirm && sessions > 0 => CloseAction::ConfirmQuit,
              CloseBehavior::Quit => CloseAction::Quit }
}
```
- **Constraint:** IPC/HTTP parity: add `resolve_close` to `COMMAND_TABLE` or to `INTENTIONALLY_UNMAPPED` (desktop-only).
- **Validation:** `cargo nextest run -E 'test(background_mode)'` on rb; `pnpm vitest run src/__tests__/closeDialog`.

### Step 2: Hidden state and every way back
- **Test:**
  - RED: `entering_background_marks_sessions_hidden_and_desktop_unfocused` — catches: standby never parking the active tab, and no push to the phone.
  - RED: `show_restores_previous_visibility_and_wakes_visible_session` — catches: a SIGSTOPped tab after show.
  - RED: `single_instance_second_launch_shows_hidden_window` — catches: today's `unminimize` without `show()`.
- **Implement:** `enter_background()` does the following:
  - Force a terminal snapshot (emit `snapshot-now` and wait for the ack, 2 s timeout; log a timeout, never block the hide).
  - Hide "main", floating and panel windows, and record which panels were open.
  - On macOS, `set_activation_policy(Accessory)` (setting `background.hide_dock_icon`, default true).
  - Set `desktop_window_focused=false`, and save then clear `session_visibility`.
  - On Windows, call `MemoryUsageLevel::Low` through `with_webview`.

  `leave_background()` reverses it. Wire it to tray Show, `RunEvent::Reopen`, single instance, deep link and `native-notification-click`.
- **Code:**
```rust
let saved: Vec<(String, bool)> = state.session_maps.session_visibility.iter().map(|e| (e.key().clone(), *e.value())).collect();
state.background.saved_visibility.lock().replace(saved);
for mut e in state.session_maps.session_visibility.iter_mut() { *e.value_mut() = false; }
```
- **Constraint:** a session that an SSE or WS client watches stays visible (the mobile PWA must not freeze on SIGSTOP). Reuse `ws_clients`.
- **Depends on:** Step 1.

### Step 3: Tray and menu-bar icon
- **Test:**
  - RED: `tray_status_counts_working_and_awaiting_from_session_state` — catches: counts from a stale cache.
  - RED: `tray_build_failure_disables_background_and_close_falls_back_to_minimize` — catches: the user locked out on GNOME (D10).
  - RED: `attention_flag_sets_on_awaiting_while_hidden_and_clears_on_show` — catches: a dot that never clears.
- **Implement:** add `tray-icon` and `image-png` to the tauri features; Linux needs the `ksni` or appindicator backend and adds `libayatana-appindicator3` to the deb and rpm dependencies. Build `TrayIconBuilder` with a template icon on macOS.
  - Menu: Show TUICommander, a disabled status line ("3 working · 1 awaiting"), Quit.
  - Left-click shows the window on macOS and Windows; Linux uses the menu only.
  - The attention variant of the icon shows while hidden and an agent is awaiting.
  - One native notice on the first background entry: "TUICommander is still running".
- **Settings:** `background.tray_icon` (`always|background_only|never`, default `background_only`; `never` disables the `background` choice).
- **Code:**
```rust
TrayIconBuilder::with_id("main").icon(icon).icon_as_template(true).menu(&menu)
    .show_menu_on_left_click(cfg!(target_os = "linux"))
    .on_menu_event(|app, e| match e.id().as_ref() { "show" => leave_background(app), "quit" => request_quit(app), _ => {} })
    .build(app)?;
```
- **Visual — requires human verification:** template icon in light and dark menu bars; Windows overflow area; KDE.
- **Depends on:** Step 2.

### Step 4: Unload and recreate the WebView (gated by Step 0 and D3)
- **Test:**
  - RED: `unload_timer_destroys_windows_after_delay_and_zero_disables` — catches: an unload that is not configurable.
  - RED: `unload_deferred_while_dictation_or_close_dialog_active` — catches: killing a live interaction.
  - RED: `show_after_unload_recreates_main_from_config` — catches: `Reopen` calling `get_webview_window("main")` and getting None.
  - RED (Vitest): `reinit_after_recreate_restores_tabs_layout_and_suspended_tabs` — catches: lost layout or lost sessions spawned while unloaded.
- **Implement:**
  - After `background.webview_unload_minutes` (sleep-aware) in Hidden: snapshot, then `destroy()` every window. `release_webview_document_resources` already runs on `Destroyed`.
  - `RunEvent::ExitRequested{code: None}` while in background calls `prevent_exit()`.
  - Show calls `WebviewWindowBuilder::from_config`; window-state restores the geometry.
  - While unloaded:
    - `ui` screenshot, tab and toast return `{"error":"webview_unloaded"}` at once (no 15 s timeout).
    - `session suspend` follows Step 5.
    - The tray Quit confirmation uses `native_dialog.rs`, not the WebView.
  - Emit `background-mode-changed {state}` on `event_bus` so the mobile PWA and the MCP clients can show "desktop in background".
- **Code:**
```rust
match e { RunEvent::ExitRequested { code: None, api, .. } if bg.is_hidden_or_unloaded() => api.prevent_exit(), _ => {} }
```
- **Constraint:** the updater check runs on show (D4 lists what stops). The docs must not promise freed memory on platforms Step 0 did not measure.
- **Depends on:** Steps 0 and 2.

### Step 5: Rust-owned suspend record (needs D8)
- **Test:**
  - RED: `suspend_works_with_no_ui_attached` — catches: today's `"no UI is attached"` refusal.
  - RED: `suspended_record_survives_restart_and_frontend_renders_it` — catches: a tab lost on quit while unloaded.
  - RED: `refusal_rules_unchanged` — reuse the `suspend_refusal` cases (`mcp_transport_session_agent.rs:1608`).
- **Implement:** move `verifyAndBuildResumeCommand` (`agentSession.ts:219`) and the suspended flag into Rust: a `suspended_sessions` store keyed by `tuic_session` (agent type, cwd, worktree, agent_session_id, launch command, alias, pane slot, reason `user|idle_hibernate`) persisted in the config dir. The frontend only renders it and asks Rust to resume. `session action=suspend` drops the `ui_attached()` requirement.
- **Depends on:** none (parallel with Steps 1-4).

### Step 6: Shared wake path `ensure_live` (D7; see "Shared wake path")
- **Test:**
  - RED: `submit_to_hibernated_session_resumes_then_delivers_once` — catches: dropped or duplicated input.
  - RED: `mail_to_hibernated_peer_resumes_it` — catches: an orchestrator waiting forever.
  - RED: `concurrent_wakes_spawn_one_pty` — catches: two PTYs for one `tuic_session` (mail and a scheduled run at the same instant).
  - RED: `resumed_pty_keeps_tuic_session_and_peer_resolves` — catches: a peer address that points at nothing after resume.
  - RED: `resume_failure_reports_reason_and_keeps_record` — catches: a silently lost tab (missing id, gone cwd, binary not on PATH).
  - RED: `standby_session_wakes_through_the_same_entry_point` — catches: a second wake path left behind.
  - RED (Vitest): `opening_suspended_tab_calls_resume_session` — catches: the old TS resume still spawning its own PTY.
- **Implement:** `session_wake.rs` with `ensure_live` as above. Route every caller in the "Shared wake path" table through it. Delete `resumeTerminal`'s spawn logic and `pendingInitCommand` for resume (the frontend only calls `resume_session`). The MCP `session list/status` exposes `state: "hibernated"`.
- **Constraint:** IPC/HTTP parity for `resume_session` (`COMMAND_TABLE` + route + snapshot).
- **Depends on:** Step 5.

### Step 7: Automatic hibernation of idle agent tabs (D5)
- **Test:**
  - RED: `hibernates_resumable_idle_agent_after_threshold_in_background_only` — catches: hibernating in foreground against the scope setting.
  - RED: `never_hibernates_with_unread_mail_background_work_awaiting_input_or_watching_client` — catches: killing an agent mid-tool or one Boss is reading on the phone.
  - RED: `never_hibernates_a_session_bound_to_an_open_automation_run` — catches: a run losing its agent mid-turn (automation sessions have no `session_parent`, so D6 does not cover them).
  - RED: `never_hibernates_non_resumable_agent_or_plain_shell` — catches: losing a shell's state.
  - RED: `sleep_time_does_not_count_as_idle` — catches: a mass hibernation after wake.
- **Implement:** a sweep that reuses `idle_close`'s `Observation` and `IdleCloseTracker`: same exclusions plus `suspend_refusal` (`mcp_transport_session_agent.rs:1608`), an open automation run (`RunStore::open_runs`), and a verified resume command. Sessions with a `session_parent` stay with `idle_close` (D6). Hibernate is the Step 5 suspend with `reason: "idle_hibernate"`, logged at info.
- **Settings:** `agents.hibernate_idle_minutes` (0 = off, default 60) and `agents.hibernate_scope` (`background_only|always`, default `background_only`).
- **Order:** ships after Step 6 — hibernating before mail, submit and the scheduler can wake the session strands them.
- **Depends on:** Step 6.

### Step 8 (story 1772-25c5): eager resume after an update restart
- **Implement:** at boot after an update relaunch, call `ensure_live` for every restored agent tab with a verified id; tabs without one keep the lazy path. No change to `createBranchSelectionCoordinator.ts` resume logic beyond deleting its command build.
- **Depends on:** Step 6. 1771-c6fb (defer relaunch until idle) is independent.

## Open questions for Boss (background side)

Answered 2026-10-12: QB1 = (a) on demand; QB2 = (a) after Step 6. See "Revision decisions".

## Story breakdown (no stories created; 1776-0f46 creates them after the open questions)

| Id | Step | Size | Depends on |
|---|---|---|---|
| B0 | Step 0 spike | S | 1776-0f46 |
| B1 | Step 1 close and quit in Rust | M | 1776-0f46 |
| B2 | Step 2 hidden state and ways back | M | B1 |
| B3 | Step 3 tray | M | B2 |
| B4 | Step 4 WebView unload | L | B0, B2 |
| B5 | Step 5 Rust suspend record (D8) | L | 1776-0f46 |
| B6 | Step 6 `ensure_live` shared wake path (D7) | L | B5 |
| B7 | Step 7 auto hibernation (D5) | M | B6 |
| 1772-25c5 | Step 8 eager resume after update | S | B6 (QB2a) |
| 1771-c6fb | defer relaunch until idle | S | none |

Automations stories A1-A5 are in `plans/automations-scheduler.md`; A4 depends on B6.

## Acceptance Criteria

- [ ] X with `close_behavior=background` hides every window, and the tray or menu bar shows status and Quit.
- [ ] While hidden or unloaded:
  - [ ] the mobile PWA lists sessions, streams output and sends input;
  - [ ] Automations fire;
  - [ ] MCP `agent send/wait` works;
  - [ ] push reaches the phone.
- [ ] After the unload delay, the WebContent process exits on measured macOS configurations (GPU and Networking may remain), and Show restores tabs, layout and live sessions. Other platforms require their own measurements.
- [ ] Idle managed sub-agents still close with `idle_timeout` mail to the parent; idle agent tabs hibernate per setting and resume on open, submit, mail, a scheduled run or a hook-triggered run — all through `ensure_live`.
- [ ] Every new automatic behavior has a setting with a catalogue entry, i18n and a Settings control: `close_behavior`, `background.hide_dock_icon`, `background.tray_icon`, `background.webview_unload_minutes`, `agents.hibernate_idle_minutes`, `agents.hibernate_scope`.
- [ ] All tests pass; `cargo build --bin tuic-remote --no-default-features` stays green (the tray code is `#[cfg(feature = "desktop")]`).

## Risks

- Step 0 passes D3 on macOS for the synthetic allocation. A long-lived TUIC process pool or a different platform may behave differently; Step 4 must verify actual document teardown and restoration.
- macOS Accessory policy removes the app from Cmd+Tab and can block native permission prompts (microphone): show the window before any permission request.
- Quit, OS logout or an update while unloaded: there is no WebView to snapshot. Steps 1 and 5 make Rust the owner of the quit path and the suspended records; the snapshot taken before hide is the last tab layout.
- A crash in background loses the PTYs, as today. A watchdog or LaunchAgent is out of scope.
- Unattended agents driven by the JS plugins stop (D4).
- Linux tray coverage varies by desktop (D10).

## Checklist (non-TDD cleanup)

- [ ] `docs/sync-matrix.md` rows: `docs/FEATURES.md`, `docs/backend/config.md`, `docs/api/http-api.md` (`background-mode-changed`, `webview_unloaded`, `hibernated`), user docs for settings.
- [ ] `to-test.md` items for the Rust changes (they need a `make dev` restart) and the Visual checks of Step 3.
- [ ] Packaging: Linux deb and rpm depend on `libayatana-appindicator3`; CI builds the tray on all three OSes.
- [ ] Mark `ideas/terminal-virtualization.md` as stale (it describes xterm.js; TUIC now uses CanvasTerminal).
