# TUICommander — Codebase Audit (2026-09-05)

Audit of the whole tree at `30776afa` by 13 parallel read-only passes, each on one
subsystem, then verified by hand on the top claims. Security was excluded on purpose:
this is a local tool for professionals and the user is the trust boundary. The axes
are **performance, usability, clarity, functionality, stability**.

Detailed per-module reports (findings with file:line evidence, ~50K words total) are
in `reviews/audit-2026-09-05/` (gitignored). This file is the synthesis: what is
wrong, what I would have done differently, and in which order to act.

Size at audit time: 192,948 lines Rust (160 files), 104,028 lines TS/TSX (504 files),
314 frontend test files, 82 docs. 411 commits in 60 days, 228 of them `fix`.

---

## 1. Verdict in ten lines

1. The hard parts are engineered well: binary grid frames, damage tracking pinned by
   a differential test, zero frontend import cycles, a gix port with parity tests,
   config writes protected by in-process mutex + `flock` + JSON delta merge, a 37-line
   mock surface in a 73K-line frontend test suite.
2. The codebase is **wide, not deep**: seven remote-access mechanisms, three "agent"
   subsystems, two dashboard mechanisms, two transports for every command, 61 stores,
   23 plugin capabilities of which 6 have no consumer, a 113-field `AppState`.
3. The single biggest risk is **agent state detection** in `pty.rs`: 14 fixes in 90
   days, nine set-signals crossed with five clear-paths, documented in a thousand
   words of prose and enforced by five fixture files.
4. The **HTTP transport has silently rotted**: 19 frontend command mappings hit no
   route, undetected for four months because the SPA catch-all answers every unknown
   path with `200 text/html`.
5. Two verified leaks that survive for the process lifetime: a panicking PTY reader
   thread leaves its 16 ms ticker and 1 Hz timer running; a WebView reload orphans
   plugin filesystem watchers until the per-plugin cap of 20 is hit.
6. A handful of one-line bugs produce wrong UI state today: conflict detection by
   substring (`UUID.md` shows a conflict), a settings toggle that is persisted and
   never read (`scrollback_reflow`), i18n that translates nothing, three panels whose
   open state is never saved.
7. Hot paths pay for work they discard: a PTY chunk is copied 10 to 12 times and the
   screen is scanned five times per chunk; the frontend polls `list_active_sessions`
   at 1 Hz forever; boot does ~25 IPC calls plus 5 per repo before the splash lifts.
8. Process is where effort is leaking: CI on `main` is red in 15 of 18 runs because the
   macOS/Windows matrix only runs after merge; `to-test.md` has 297 open items and 12
   closed; 60 pedantic clippy lints are individually allowed.
9. Docs drift is material in exactly the places AGENTS.md says to read first:
   `state-management.md` shows 6 of 113 fields, `github.md` omits the whole poller,
   `vt100-PWA.md` documents a removed crate, `output-parser.md` describes a
   test-only entry point.
10. Nothing is architecturally rotten. The recurring pattern is **features shipped to
    90% and left parallel to their neighbours**, plus a habit of adding a rule where
    removing a path would have closed the bug class.

---

## 2. Fix now — verified, small, user-visible

Ranked by impact over effort. Each was confirmed against the source (and the live
instance where applicable).

| # | Where | What | Effect today |
|---|---|---|---|
| 1 | `mcp_http/mod.rs:1509` | SPA catch-all `/{*path}` serves `index.html` with 200 for any unregistered API path | `rpc()` sees `ok`, fails JSON parse, hands a 12 KB HTML string typed as the command's return value. Masks #2 and #3. |
| 2 | `mcp_http/mod.rs:1-21` | `mod dictation_routes;` was never added. File exists, 157 lines, 12 handlers, edited twice, **never compiled** (`git log -S` confirms) | 12 dictation commands broken in browser mode with no build warning. |
| 3 | `src/transport.ts` | 19 `COMMAND_TABLE` paths have no route: 12 `/dictation/*`, `/agents/detect-all`, `/agents/open-in-app`, `/system/notification-sound`, `/system/relay-status`, `/system/check-update`, `/sessions/{id}/shell-family`, `/worktrees/run-script` | Silent HTML-as-data in browser/PWA. The parity test asserts the table, never the router. `mod.rs:2251` already has a PATCH-probe technique that would catch all 19 in 30 lines. |
| 4 | `pty.rs:7976` | `running.store(false)` sits **inside** the `catch_unwind` closure; the `Err` arm at 8051 never sets it | A reader-thread panic leaves one OS thread at 62 wakeups/s and one 1 Hz tokio task alive until app exit, plus `grid_frame_dirty`/`sync_update_active` entries that are removed only after the ticker loop. |
| 5 | `pty.rs:5701` + `5955` | Resize grace re-arms while `!vt_log_grew`; `vt_log_grew` compares `history_size()+screen_lines()` against a `.max()` high-water mark. In the alternate screen `history_size()` is 0, so it is permanently false | Resize a tab running an alt-screen agent and low-confidence questions, rate-limit and API-error events are suppressed and the BUSY transition is disabled until output pauses for a full second. |
| 6 | `git.rs:305` | `stdout.contains("UU") \|\| contains("AA") \|\| contains("DD")` over the whole porcelain output | `?? UUID.md` marks the repo conflicted for the 60 s cache TTL. `git_cli.rs:290` already parses the two status columns correctly. |
| 7 | `state.rs:2535` | `http_client: reqwest::Client::new()` — no timeout; `github_poller` awaits it inline in a `select!` | One half-open socket after a VPN drop wedges the GitHub poller and its `Stop` command permanently. Every other client in the crate sets a timeout. |
| 8 | `output_parser.rs:450` | Dedup reset on `UserInput` is unreachable: no parser in `parse_clean_lines` emits `UserInput` | After one API error the identical error is suppressed for the session; `session_conflict_fired` latches so a second session-id conflict never auto-resets. |
| 9 | `plugins.rs:463` | `register_loaded_plugin_impl` never calls `dispose_plugin_runtime_state` (only `uninstall` does, line 1000) | WebView reload / HMR full reload orphans `RecommendedWatcher` + thread. wiz-kanban watches 3 dirs; ~7 reloads exhaust `MAX_WATCHERS_PER_PLUGIN=20`, board stops updating until restart. |
| 10 | `relay_client.rs:195` + `:254` | `let mut backoff` outside the loop, doubled every iteration, comment at `:295` claims a reset that cannot happen | After a few sleep/wake cycles the relay waits the full 60 s cap forever. |
| 11 | `tunnels/supervisor.rs:159` | ssh stderr piped, `read_stderr` called only after exit | Chatty ssh fills the 64 KB pipe and blocks; tunnel stalls with status still `Connected`. |
| 12 | `lib.rs:1249-1270` | Relay task spawned only if enabled at boot; shutdown sender stored and never used | The Settings toggle does nothing until restart, in both directions. |
| 13 | `src/hooks/useAgentPolling.ts:311` | `list_active_sessions` every 1000 ms while any terminal exists, no visibility gate. The justification (Activity Dashboard convergence) refers to a component that is unmounted when closed | The always-on IPC class the Diagnostics doc warns about, running in the background. Also `:295`: the effect re-runs on every tab add/remove and restarts both timers, so the 30 s session-discovery poll starves during tab churn. |
| 14 | `src/stores/ui.ts:142-157` | Save serializes 5 of 8 exclusive panels; `outline`, `references`, `aiTriage` are dropped | Leave Outline open, restart, no panel restored (or a stale one). |
| 15 | `config.rs:626` → `terminal_grid.rs:523` | `scrollback_reflow` is declared, defaulted, mapped in TS both ways, exposed in Settings — and `reflow_history` is initialised `true` and never assigned | A visible toggle with zero effect. |
| 16 | `src/i18n/t.ts:7` | `t(_key, fallback)` ignores the key; `en.json` is `{}`; 46 components import it; `setLocale` is wired from config | The Language setting is a dead control. Either implement or delete the key parameter. |
| 17 | `mcp_transport.rs:1907` | Unknown-session guard runs before `session_wait_met`, but `pty.rs` removes the session right after inserting its exit code | `session wait until=exited` returns `Unknown session` for the exact case it exists for. |
| 18 | `mcp_transport.rs:3634` | `agent spawn` hardcodes `VtLogBuffer::new(24, 220, …)` while the PTY uses caller `rows`/`cols` | Screen-row parsing against the wrong geometry for any non-24-row spawn. `session.rs:594` does it right. |
| 19 | `Terminal.tsx:902-911` | Zero-size retry reschedules a rAF every frame with no cap, no `disposed` check, no captured handle | A collapsed or transitioning pane spins one rAF per terminal for the page lifetime, surviving unmount. |
| 20 | `Terminal.tsx:1087-1101` | Visibility effect fires on **every** tab switch (comment says reattach only), does `resubscribe()` + `refresh()`, which clears the grid | Paint → wipe → paint on each switch, plus a channel teardown. No `.catch()` on the promise. |
| 21 | `mcp_http/session.rs:1777` | `pending_scroll` entry is created only by the desktop-only `subscribe_terminal_grid` command | Wheel and scrollbar drag in pure browser mode return `ok:true` and do nothing. Closing the desktop terminal disables scrolling for an attached browser. Contradicts AGENTS.md. |
| 22 | `lib.rs:1232` → `github_auth.rs:594` | `resolve_token_without_keychain` eagerly runs `gh auth token` (no timeout) on the synchronous boot path, even when `GH_TOKEN` is set and would win | A `gh` blocked on a credential helper means no window at all. |
| 23 | `log_routes.rs:34-45` | `/logs` slices by `limit` then filters by level/source | `?level=error&limit=50` returns errors within the last 50 lines, usually none. AGENTS.md tells every agent to use this exact query. |
| 24 | `mcp_http/ai_stream.rs:120` | `bridge_conversation` does not `select!` on the socket; its twin `bridge_chat` does, with a comment explaining why | A quiet conversation holds its subscription until the next event, which may never come. |

---

## 3. What I would have done differently (architecture)

### 3.1 One transport, not two clients

253 `#[tauri::command]`, 298 routes, 335 `COMMAND_TABLE` entries, 58
`INTENTIONALLY_UNMAPPED`, 26 hand-written `transform` unwraps, a 398-line
`generate_handler!` block and a 2,671-line parity test. Roughly **5,800 lines of glue**
whose only job is keeping two call paths in sync, maintained by "HTTP parity for N
commands" sweeps (48 commits mention parity).

The second **transport** is right: browser, PWA and remote clients are real products.
The second **client** is not. The desktop app could talk HTTP for request/response
too, keeping IPC only for the grid channel, `write_pty`, and the ~58 genuinely
host-only commands. That deletes the table, the allowlist, most of the parity test,
the transforms, and the whole drift class (findings 1-3 above). The one place IPC is
"faster", grid push, is also the place that produced the 240 IPC/s flush loop and had
to be moved off the IPC thread.

**Correction after review with Boss.** The WebView (WKWebView, WebView2, WebKitGTK)
cannot `fetch()` a Unix socket or a Windows named pipe; those serve native processes
(`tuic-bridge`, `tuic-cli`, mdkb). Desktop-over-HTTP therefore means **loopback TCP**
on all three platforms, and today the TCP listener binds only when
`services.server.enabled` is on (`mod.rs:1883`). Real costs: an always-on loopback
listener, one IPC at boot to hand the WebView the actual port (the 9876→9878 retry
makes it non-fixed), and on Windows a small support surface (AV/proxy products that
intercept loopback, more frequent port collisions). No evidence in git history that
IPC was chosen for performance: it is the Tauri default from the first commit, and
HTTP parity was bolted on afterwards. But the migration has not been benchmarked
either. Treat it as a code-cleanliness decision, to be taken after measuring 3-4
typical commands IPC vs loopback on Boss's machine, not as a correctness fix.

The correctness fix is small and should happen regardless: make unregistered API
paths return 404 (serve the SPA only for paths with no known API prefix) and drive
the existing PATCH probe over every `COMMAND_TABLE` path. That closes the drift class
at near-zero cost.

Related: HTTP has a 30 s client abort, IPC has none, so `run_pr_review`,
`create_worktree`, `download_whisper_model`, `scan_build_artifacts` and a dozen others
succeed on desktop and abort in the browser. Error strings differ across transports
too, and 19 call sites match on error substrings.

### 3.2 `pty.rs` is four modules and a regression sink

22,791 lines, 10,969 production. 122 commits in 90 days, 78 of them `fix`, 14 on
agent working/idle/awaiting alone. The file has clean seams already marked by
banners: lifecycle/`SilenceState` (~3,100 lines), per-agent screen adapters and
process-tree probing (~1,000), injection and peer delivery (~900), the ~50 one-line
`#[tauri::command]` wrappers (~2,900). What remains, `ChunkProcessor` + reader +
ticker, is ~2,500 lines and readable. The 11.8K-line test module should move with its
subjects, otherwise splitting makes both halves worse.

The deeper point is that the state machine has **no closed specification**. Nine
signals set `awaiting_input`, five clear it, working/idle is decided by a further
ten inputs, and the priority between them is documented in prose across three files
and nowhere as one table. `docs/architecture/terminal-state-machine.md` is 828 lines
and not executable. Every fix adds a rule. The `.tcap` capture format, the
`/diagnostics/capture` endpoint and the shared replay harness were all built to make
"a reproduced failure becomes a fixture, always" cheap — and the corpus has **two**
`.tcap` files. I would make the fixture a hard gate on any commit touching
`chrome::`, `suppress_heuristic_question` or the awaiting signals, and derive the
transition tests from the table rather than the other way round.

Concrete heuristic defects the parser audit verified by simulation:
`COPILOT_STATUS_RE` claims `●`, the glyph Claude uses for assistant output, so a
Claude message line becomes a status line that clears low-confidence awaiting and
rewrites `current_task`; `AIDER_SPINNER_RE` matches `#   comment` and `##  heading`;
`is_working_status_row` is an unanchored `contains("esc to interrupt)")`, so an agent
quoting that string pins itself busy; `is_chrome_row` matches `·` anywhere in a line;
hook-instrumented Codex has every `Question` suppressed but its hook map emits no
`awaiting` and it sends no OSC 777, so approval dialogs read as "working". The chrome
cutoff's own deferral note measures it failing open on 71-81% of ticks on two
captures. The newest code (`is_ink_dialog_footer_row`, `is_spinner_row`,
`rearm_awaiting_for_open_dialog`) applies column-0 anchoring and presence-vs-edge
correctly; the older code next to it does not. Retrofit the old rules to the new
discipline before adding another.

### 3.3 `AppState` and `AppConfig`

`AppState` has **113 fields**, 63 of them `DashMap`/`DashSet`, no `Default`, and the
full literal is hand-written in four places (~425 duplicated lines; `state.rs` even
has two different functions named `make_test_app_state`). Group into `GitHubState`,
`McpState`, `AiAgentState`, `SessionMaps`, `GridState` with `Default` each.

`AppConfig` has 48 top-level / 176 leaf fields, mirrored by hand in `settings.ts`
(42 save writes, 87 load reads). Every new field is five coordinated edits, which is
how `scrollback_reflow` reached every layer except a consumer, and how
`mcp_config_installed` ended up serialized into every user's `config.json` with zero
readers. Generate the TS type from the Rust struct (`ts-rs`/`specta`) or keep
snake_case keys and drop the mapping. Every toggle also rewrites the whole file via
load-modify-save; a per-key patch command would remove `applyOwnedFields` and the
"other surface owns that field" class of bug.

### 3.4 An internal coding agent inside an agent orchestrator

`ai_agent/` is 15.5K lines (23K with chat/registry/routes, 12% of Rust), 30 tools,
~700 tests, four UI entry points, all reachable, well built. But **a third of the tool
layer** (`read_file`, `write_file`, `edit_file`, `list_files`, `search_files`,
`search_code`, `run_command`, ~840 lines) re-implements what Claude Code and Codex
already do inside the panes TUIC exists to orchestrate, and `safety.rs` (1,134 lines,
14 regexes) guards a threat model AGENTS.md explicitly disclaims — while being
trivially bypassable (`find . -delete`, `curl | sh`). The differentiated value is the
terminal-aware tools: `read_screen`, `send_key`, `drive_agent`, `wait_for`,
`search_scrollback`, `get_semantic_zones`, plus the watcher system. I would narrow the
internal agent to orchestration, delegate file work to the agent already in the pane,
and either scope `safety.rs` to the unattended cron path or shrink it to advisory.

Real bugs in that subsystem: the watcher event loop is one task that `await`s a 20 s
LLM classification inline, so one slow provider stalls every watcher on every session
and drops events past the 256-slot bus; `fire_rule` does synchronous `save_config`
(with the cross-process flock) on a tokio worker up to three times per fire,
violating the contract written at `config.rs:1888`; idle classification is paid before
the cooldown check so a rule in cooldown burns an LLM call per idle event; no
`max_tokens` on any LLM call; streaming has no per-chunk timeout; `mdkb_client::call`
has no timeout and holds a global mutex; provider defaults are hardcoded twice.

### 3.5 Seven ways to reach the machine

LAN TCP, Tailscale TLS, SSH tunnels (3,780 lines), cloud relay (704), Web Push (546),
PWA, `tuic-remote` headless, Remote Connection Manager (397). Only Tailscale layers on
the others. No shared abstraction, no shared reachability test, no decision guide;
the user docs cover all of them on one page with no "if you want X use Y" table. The
relay is one-way (inbound frames are decrypted and dropped at a `TODO`), so push
wakes the user on `awaiting_input` and the phone cannot answer. Tunnel `auto_connect`
is implemented in the frontend only, so `tuic-remote` never auto-starts anything.
`kill_ssh_on_port` SIGTERMs any listening `ssh` on the port, including the user's own.
Consolidating mechanisms would also consolidate the five crypto crates that each
serve exactly one of them.

### 3.6 ACP: a second agent transport with no integration point

`src-tauri/src/acp/` is 845 lines + 339 test lines, pinned to `agent-client-protocol
= "=2.0.0"` with three unstable features, referenced from exactly one line
(`lib.rs:6 pub mod acp;`). No command, no route, no frontend, no doc. Snapshot types
are declared and never constructed; an 18-operation capability matrix answers
questions no code can ask. The last five commits are all ACP, so it is active work —
but the project has no stated position on which agents go through PTY and which
through ACP. Decide that before more surface accrues, and until a vertical slice
lands put it behind a Cargo feature.

### 3.7 Plugin host sized for plugins that do not exist

~12,170 lines of host (8,100 Rust + 4,000 TS) for 11 plugins totalling 6,363 lines,
3 of them tested. 6 of 23 capabilities and 26 of ~52 host methods have zero consumers.
`plugin_http.rs` (682) and `plugin_credentials.rs` (236) are careful, tested, and
unused. `plugin_fs.rs` is 3,502 lines because ~980 are a 15-toolchain build-artifact
scanner for one plugin, filed under "filesystem". The iframe SDK (`tuicSdk.ts` +
327 doc lines) is injected into every panel and called by nobody. Two dashboard
mechanisms exist (`dashboardRegistry` vs hardcoded tab types in `MdTabContent.tsx`)
with no rule for which to use; the uncommitted Codex dashboard picked the hardcoded
one, never refreshes, redefines its response types locally, and re-implements the
Claude cache skeleton as a cleaner `TtlCache<T>` that Claude should now consume.
`PLUGIN_DOCS` tells agents the current app version is `0.3.x`.

### 3.8 Frontend shell: 61 stores, 13 settings tabs, one virtualized list

61 store files / 16,464 lines. Four are a single boolean plus toggle, the shape
`ui.ts` already implements eight times with exclusivity logic. Overlap clusters:
preferences in 3 layers, notifications on 4 surfaces (with **two live sound
implementations**, Web Audio in `toasts.ts` and rodio via `notifications.ts`),
plugins in 3 stores, GitHub in 2, prompts in 3, tab ordering in 2 (`tabManager._order`
and `tabOrdering.ts` with byte-identical splice logic). `terminals.ts` is the one tab
kind outside `createTabManager`, so `TabBar` merges four id lists then applies a fifth
ordering. Settings: 13 tabs, 7,879 lines, 48 keys, no search — while
`sections.ts` already assigns deep-link ids. Exactly one list in the app uses the
virtualizer (`BlameTab`); FileBrowser, ChangesTab, ErrorLog, CommandPalette do not.
`App.tsx` (1,109 lines, 40 hooks, 24 dialog booleans, `Sidebar` with 21 props) and
`useAppInit.ts` (855 lines, ten concerns including a full `tuic://` router) are the
map of the app and currently unreadable as one.

### 3.9 MCP transport: careful code under a legacy dispatch layer

`mcp_transport.rs` is 15,987 lines, 6,450 production; three functions
(`handle_messaging`, `handle_session`, `handle_agent_with_parent_cwd`) are 1,680 of
them. The file's own section banners mark a five-way split (transport, tool catalogue,
peer identity + messaging, session/agent handlers, ancillary handlers); the peer
identity subsystem — twelve maps under one process-global bind lock, with invariants
that were clearly learned the hard way — is coherent but should own its state behind
one struct in its own module.

What I would remove: the `LEGACY_*` action constants and `remap_action`, which
clone the whole args `Value` per dispatch (twice on blocking paths) so an inner
handler can re-match an action the outer one already validated — the two-sources-
of-truth this creates is how `issues`/`close_issue`/`reopen_issue` in
`handle_github` became unreachable while still advertised. `agent spawn` duplicates
~80 lines of `session::spawn_pty_session` and has drifted (hardcoded VT geometry,
missing `shell_integration::inject`, different event ordering); one
`register_pty_session` helper makes that class of bug unrepresentable.

Protocol: `initialize` always answers `2025-11-25` and never reads the client's
requested version, so the three-entry `SUPPORTED_PROTOCOL_VERSIONS` array is dead
and the `tasks/*` migration `tasks.rs` was designed for can never be announced;
`tools.listChanged` is emitted but never declared as a capability, which is the
root of the 3 s blocking settle hack inside `tools/list`. `session submit` polls at
10 ms for up to 10 s where `session wait` subscribes to the same signal. `session
list` (the call agents are told to prefer) does a `proc_pidpath`/`/proc` read per
session on the async runtime; `repo` reads and parses `repositories.json` from disk
per call; `resolve_allowed_upstreams` re-parses `repo-settings.json` on every
`tools/list` and every proxied call. `session_html_tabs` has no dedup and no cap,
and tabs from unbound callers are never registered so they orphan.

### 3.10 Dependencies and lints

`ulid` and `cuid2` each have one call site (`generators.rs:51-52`) in a utility
panel; UUIDv7 is already present. `gix` with seven features serves one file while 58
sites shell out to `git` — the split is principled (reads via gix, writes via CLI,
parity-tested) but the `Backend::Cli` router arms are dead dispatch with no exit
criterion. `cargo tree`: 634 crate-versions, **62 at multiple versions** (reqwest
0.12+0.13, tokio-tungstenite 0.29+0.30, schemars 0.8+1.2, thiserror 1+2, syn 2+3,
the whole RustCrypto stack doubled). `lazy_static` half-migrated to `LazyLock` (4
files left). `jsdom` exists for one test file. Clippy `pedantic` with **60 allows**
is fatigue written as config; opt into the ~20 that fire usefully instead.

---

## 4. Performance: where the CPU goes

### Backend PTY path (per 64 KB read, `pty.rs:7877`)

A chunk is **copied 10-12 times**: raw ring, UTF-8 reassembly, escape reassembly,
kitty strip, `vt.process`, changed-row strings, full `screen_rows()` clone,
output ring, one clone per WS client, `Some(data.to_owned())` return, and up to two
more in `transform_xterm`. The visible screen is **scanned five times per chunk**
(chrome cutoff, activity classify, `parse_choice_prompt`, question-gone,
`ink_dialog_footer`), with the clone gated on `any_row_changed` computed **before**
the chrome filter, so a 1 Hz status line pays the whole cost and produces nothing.
`SilenceState` is locked **seven times** per chunk, once with two regexes per row
inside the lock. Eight status regexes run per changed row, and the fast-path bail
(`!contains(0xE2)`) never fires on agent output because every box-drawing glyph
starts with 0xE2. The silence timer clones the whole screen up to four times per
session per second. Always-on diagnostic byte scans run on every chunk and every
keystroke for the sake of a `tracing::warn!`. `ps` is forked once per second when
demanded and **once per session** for stats (16 forks for 14 sessions, 14 parsing the
same table). A 50 ms `std::thread::sleep` is held under the PTY writer mutex on a path
reachable from the reader thread and from every keystroke that empties the composer.

What I would do: `Cow` through the chunk path; compute the chrome cutoff first and
split "changed above" from "changed below"; one screen snapshot per tick handed
down; one `SilenceState` critical section with regex work outside it; gate the
status-regex table on `agent_type`; hang the diagnostic scans on the existing
Diagnostics toggle; one `ps` per stats refresh.

### Grid delivery

Frames are serialized under the vt lock and **sent after releasing it** from five
producers, so delta ordering is not guaranteed; the resize path can land a stale full
frame last, which is the "blank after zoom" the code exists to prevent. A slow WS
client triggers `full_frame_for_single_client`, which re-marks the grid fully damaged
and re-arms the ticker, so one slow browser pins the session into 108 KB frames that
the desktop also decodes. The desktop `GridGate` is checked before serializing, so a
stalled WebView starves every browser viewer. The alternate grid gets the same 10,000
scrollback as primary (upstream gives it 0), ~96 MB worst case per session.

### Frontend

Tab switch: paint → wipe → paint, two full frames, a channel resubscribe. Resize
paints an empty grid before the new frame arrives (100 ms debounce, flashes on
drag). `resize_pty` newest-wins coalescing is dead on desktop because the terminal
bypasses `rpc()`. Mouse movement probes links in **every visible pane** (no rect
test; the sibling branch has one) at up to 3 IPC each. Cursor blink clears and
repaints the whole seven-pass overlay every 700 ms, iterating every command block.
Four `document` listeners per mounted terminal and terminals are never unmounted (30
tabs → 120 handlers per mouse move). No glyph atlas: one `fillText` + one string
allocation per non-blank cell, three column scans per row. `isIdempotentRpc` runs the
full HTTP table lookup on every desktop RPC and constructs-and-catches an `Error` for
unmapped commands. `await import("@tauri-apps/api/core")` per call. Link verification
awaits one IPC per URL row serially, ~6.6×/s under agent output, next to a batched
single-row path that fixed the same problem.

### Boot

Synchronous before the window: rustls install, log dir cleanup, config load with
possible secret migration, first-run VAPID keygen + save, **`gh auth token`
subprocess with no timeout**, `PushStore::load`, MCP config rewrite across **13
agents' config files**, window-state sanitize. Then per repo: `.tuic.json` load + 4
separate git probes, concurrency 4. 15 repos ≈ 100 round trips before the UI settles.
Background loops started unconditionally that need not be: AI scheduler reads
`scheduler.json` from disk every 30 s forever even with AI off; knowledge persist
dispatches a `spawn_blocking` every 2 s regardless of dirt (~43K/day); process
snapshot refresher dispatches a `spawn_blocking` every 1 s with the demand check
inside it (~86K/day); MCP upstream health checker runs with zero upstreams; the Fn-key
monitor installs with dictation off. `tracing_appender` file writer is used without
`non_blocking`, so every log line is a synchronous file write on the emitting thread.

### Git

One file save costs 5-6 git invocations per repo: three different `git status` flag
combinations (fingerprint, `repo_info`, working-tree) plus two `numstat`.
`branches_detail` is O(branches × history): two full revwalks per branch per
`ahead_behind`, called twice per branch. `index.lock` is deleted on every git call if
older than 30 s, without checking whether a live process owns it — an index
corruption path on large monorepos, which is exactly the target audience. `GitCmd`
has no timeout facility; `git fetch` and setup scripts can hang forever.

---

## 5. Usability

- **Settings**: 13 tabs, no search, four tabs over 800 lines; a Language setting and
  a scrollback-reflow toggle that do nothing; relay toggle that needs a restart with
  no hint.
- **First run**: a modal asking to install a CLI to `/usr/local/bin` fires during
  boot before the user has seen the app. TipOfTheDay is not dismissible and reappears
  whenever all tabs close. WhatsNew has no re-open path.
- **Notifications**: 63 toast sites, 14 sound sites, 7 activity-feed sites, 4
  surfaces; one event can appear in three places; errors never auto-dismiss; dedup is
  opt-in per call site; two independent sound mixers with different volume/mute.
- **Selection auto-copies to the clipboard on every mouse-up**, no setting, standard on
  X11 and surprising elsewhere. One IPC per accidental drag.
- **Terminal swallows app shortcuts**: the catch-all VT branch calls
  `stopPropagation()` at target phase, so any user binding not special-cased in the
  ~17 preceding branches is unreachable while a terminal has focus; branches that
  only `preventDefault` double-fire (PTY gets the byte and the app runs the action).
- **Browser mode**: wheel/scrollbar scrolling is a no-op; WS gives up after 10
  reconnects (~1 minute) with no UI signal and no retry; reconnect never re-requests a
  frame so the terminal shows stale content; backend-down has no indicator anywhere;
  SSE `lagged` is unhandled on the main browser path so a client that falls behind
  silently loses events with no resync.
- **Sidebar staleness**: cold repos (no open terminal) debounce working-tree changes
  by **15 s**; PR badges on dormant repos are up to 10-50 minutes old by design;
  `head-changed` invalidates the backend cache only if the frontend calls back, and
  the frontend early-returns when it already knows the branch. Worth knowing before
  treating "sidebar is wrong" as a bug.
- **Plugins**: load failures are visible only in Settings → Plugins; no toast.
- **MCP for agents**: `tools/list` returns **155 KB (~39K tokens)** per connection on
  this install with upstream proxying on, 17.8 KB of which is TUIC's own;
  `collapse_tools` exists and defaults off, all-or-nothing. `plugin_dev_guide` returns
  28 KB unconditionally. `ui confirm` has a 300 s timeout with no client-deadline
  margin, the exact round number the `wait` path added a margin to avoid.
- **bash and fish get no OSC 133 integration** (only zsh via ZDOTDIR and WSL bash);
  `inject_bash`/`inject_fish` set an env var nobody sources, with nothing telling the
  user the deterministic path is unavailable.
- **`/logs` filter-after-limit** makes the documented debugging query return nothing.

---

## 6. Stability

- Reader-thread panic leak (§2 #4), resize-grace latch (§2 #5), plugin watcher orphan
  (§2 #9).
- The lossless PTY state lane is an **unbounded** mpsc consumed by one task that also
  clones the whole `AppConfig` on some paths; the bus next to it is bounded at 256 and
  every consumer handles `Lagged`. No depth metric.
- `std::sync::Mutex` with `.lock().unwrap()` inside the VTE event path
  (`terminal_grid.rs:39`): one panic poisons it for the session. Same pattern in
  `plugin_exec.rs:45`. Rest of the file uses `parking_lot`.
- `serialize_styled_range` trusts a client-supplied `count` for its loop bound under
  the vt read lock, and truncates `rows.len()` to u16 silently.
- Corrupt `config.json` is silently replaced by defaults and the **next save overwrites
  the file**, destroying hand-repairable content. `update_with_strict` exists for this
  and is not used on the read path. Same for a failed `repositories.json` hydrate:
  status string for 30 s, empty sidebar, one save from clobbering.
- `restart_server` builds a whole runtime with `.expect()` on a detached thread and
  drops every SSE/WS on config save.
- No `timeout-minutes` in CI (one run: 1,020 minutes). No `TimeoutLayer` or body-limit
  on the axum server. OAuth HTTP clients, whisper model download, `mdkb_client`,
  `registry.rs` fetch: no timeouts. Whisper downloads are unverified; any file >1 MB
  passes `model_exists`.
- `chunks(channels)` on the cpal audio callback with `channels` unvalidated from the
  device: `chunks(0)` panics on an OS-owned thread.
- `InputLineBuffer` has no bound on `csi_params` or `chars`; the test named
  `test_buffer_cap_safety` asserts there is *no* cap.
- `enrich_with_numstat` swallows failure into `0/0` with no log; one `eprintln!` in
  `git.rs:1865` bypasses tracing so it never reaches `/logs`.
- Built-in themes are seeded once (`if dir.exists() return`); updates never reach
  existing users.
- Hook commands are POSIX-only; on Windows a hook-instrumented session gets full
  `Question` suppression with no replacement signal.

---

## 7. Process and tooling

- **CI is red by default**: 15 of 18 concluded runs failed. `rust-cross`
  (macOS-ARM, Windows) has `if: github.event_name == 'push'`, so platform breakage is
  found after merge. Windows never runs tests (`if: matrix.platform !=
  'windows-latest'`), only clippy. Two commits exist solely to restore green.
- **`to-test.md`**: 297 `[ ]`, 12 `[x]`, 34 `[HUMAN]`, 1,445 lines, 65 commits in 90
  days. Effort goes in, nothing comes out. Cap it at ~30 items, triage the rest into
  stories or delete.
- **Fixture corpus**: 5 files in `fixtures/agent_prompts/` against 14 agent-state
  fixes and an "always a fixture" rule.
- `cargo audit` ignore list lives in the Makefile, not in `audit.toml`, so the weekly
  `audit.yml` re-opens issues for four accepted advisories forever. `audit.yml`
  compiles `cargo-audit` from source weekly.
- The committed Makefile hard-depends on `rtk` (14 sites), a personal proxy
  undocumented in the repo; `CONTRIBUTING.md` documents different raw commands.
  `RTK ?= $(shell command -v rtk)` fixes it with zero local change.
- Root clutter: `perf-scan/` (13 chunk files, 310 KB, tracked), `performance_scan.md`,
  `cyclomatic.md`, `ai-evolution.md` tracked at root; `DESIGN.md`/`PRODUCT.md` frozen
  since June while `SPEC.md` moves; `TODO.md` untracked but not gitignored; test
  fixtures (`test-data.csv`, `test-width.txt`) at root; `tests/` is a Python stress
  harness, not the test suite.
- Thin-wrapper hook tests assert the mock's return value (`useRepository.test.ts:15`,
  `usePty.test.ts:16` — whose only real logic, the `catch → false`, is untested).
  Bounded problem; the suite is otherwise real.
- 72 of 298 routes undocumented in `http-api.md`; the doc uses `:id` params while the
  router uses `{session_id}`, defeating mechanical checks. `sync-matrix.md` is 295
  hand-maintained rows with one 6-line shell check in the Makefile.
- Docs that actively mislead: `pty.md` documents `clamp_cursor_up` (does not exist)
  and a 50 ms reader sleep (it is 10 ms); `state-management.md` shows a 64 KB ring (it
  is 2 MB) and 6 of 113 fields; `overview.md` claims ~830 tests (there are ~9,800);
  `github.md` names three functions that do not exist and omits `github_poller.rs`
  entirely; `vt100-PWA.md` documents the removed `vt100` crate; `alacritty-integration.md`
  lists 10 patch rows against a ~1,000-line semantic delta across 23 files (plus ~1,700
  lines of self-inflicted rustfmt noise that will make a 0.27 rebase conflict on
  whitespace everywhere); two comments claim a render worker that does not exist
  (`gridRenderer.ts:3`, `perfTrace.ts:13`), aiming freeze investigations at the wrong
  thread; AGENTS.md says Command Palette is desktop-only (it renders in browser mode
  with a filtered action set).

---

## 8. What is right and should not be "simplified" away

Recorded so a later pass does not undo them.

- Binary grid frames with `ROW_PARTIAL_FLAG`, justified by a measured 2.44× overship;
  `process_damage_matches_full_diff` differential test; `GridGate` counting design.
- `ConfigFile<T>`: in-process mutex + cross-process flock + JSON delta merge, with
  two-process race tests that spawn real OS processes. The clobber bug is fixed.
- gix/CLI split in `git_reads.rs`: principled, documented, parity-tested. Panic
  hygiene in git/github: 3 unwraps in ~25K production lines.
- GitHub poller: adaptive cadence, four rate-limit signals classified, budget-aware,
  single batched loop. No finding.
- `shared_routes()` + PATCH-probe surface lock; `repo-changed` cross-transport payload
  parity test with a guard-the-guard.
- `output_watchers.rs`, `agent_hook.rs` (every awaiting SET paired with a retraction
  and tested), `rearm_awaiting_for_open_dialog`, `is_ink_dialog_footer_row`.
- `tuic-bridge`: reconnects, offline responses, per-request timeouts derived from the
  requested wait. Best-engineered file in the periphery.
- Zero frontend import cycles, with a self-testing checker. `createTabManager`'s
  `paneDeactivators`. Per-repo debounce + in-flight coalescing + frame-coalesced
  revision bumps. Mutation-delta persistence with conflict rebase in `repositories.ts`.
- `report-frontend-bundles.mjs` failing the build if mermaid/katex/cytoscape re-enter
  the eager graph.
- `setup.ts` inert socket stubs with a nine-line comment on the exact flake they fix.
  16 Rust `#[ignore]`s, every one with a reason string. Zero silently skipped Vitest
  cases.
- `appLogger` is a `tracing` Layer, not a second logging system. `dashmap`/`moka`/
  `parking_lot` each have a distinct justified job. Claude usage scanning is
  incremental (offset-based, persisted cache), not a rescan.
- Every `DEFERRED (date)` and `REJECTED (date)` note found was specific and honest.
  That discipline is why the remaining holes are findable.

---

## 9. Suggested order of work

**Wave 1 — one-liners with user-visible effect (a day).**
§2 items 1-2 (404 for unknown API paths, `mod dictation_routes`), 6 (conflict
substring), 7 (HTTP client timeout), 8 (dead `UserInput` reset), 9 (dispose on
register), 10 (relay backoff), 13 (kill the 1 Hz poll, fix the effect restart),
14 (persist three panel flags), 22 (move `gh auth token` off the boot path), 23
(`/logs` filter order). Then §2 #4 and #5 with a `.tcap` fixture each.

**Wave 2 — structural guards (a week).**
Drive the PATCH probe over `COMMAND_TABLE`. `RTK ?=` in the Makefile.
`timeout-minutes` on every CI job; run `rust-cross` on PRs. `audit.toml`. Reset
`to-test.md`. Make the fixture a gate on agent-state commits. Fix the six docs that
actively mislead. Delete `perf-scan/`, `cyclomatic.md`, `performance_scan.md`; merge
`DESIGN.md`/`PRODUCT.md` into `SPEC.md`.

**Wave 3 — hot paths (two weeks).**
PTY chunk path: `Cow`, cutoff-first, one snapshot per tick, one `SilenceState`
section, agent-gated regex table, diagnostics behind the toggle. Grid: send under the
lock or stamp a sequence on the desktop channel; per-subscriber resync without
global re-damage; create `pending_scroll` at session creation. Frontend: drop
`resubscribe` from the visibility edge, skip the empty paint on resize, rect-test the
link probe, early-return `rpc()` on desktop, batch the four per-repo boot probes into
one command. Gate the five always-on background loops.

**Wave 4 — decisions Boss has to make.**
Desktop over loopback HTTP (kill the second client) — only after the IPC-vs-loopback
benchmark in §3.1. Split `pty.rs` along its banners,
tests first. Group `AppState`. Generate the TS config type. Narrow the internal AI
agent to terminal orchestration. Feature-gate ACP until a vertical slice lands, and
write down PTY-vs-ACP routing. Pick a dashboard mechanism. Cut the unused plugin
tiers. Fold the boolean stores into `ui.ts`, unify sounds, unify tab ordering.
Settings search. Implement or delete i18n. Drop `ulid`/`cuid2`; decide gix vs CLI;
replace `pedantic` with a list.

---

## 10. Report index

| Report | Scope | Findings |
|---|---|---|
| `audit-pty.md` | `pty.rs`, grid gate, capture, shell integration | 26 + output-path walkthrough + split plan |
| `audit-parser.md` | `output_parser.rs`, `chrome.rs`, hooks, agent tables | 24 + signal inventory |
| `audit-grid.md` | `terminal_grid.rs`, alacritty/vte fork, grid WS | 18 + wire format + fork inventory |
| `audit-http.md` | axum server, routes, SSE, transport parity | 24 + dual-transport verdict |
| `audit-core.md` | `state.rs`, `config.rs`, `lib.rs`, boot | 21 + boot loop inventory + cleared hypotheses |
| `audit-git.md` | git, GitHub, worktree, watcher, fs, index | 18 + gix/CLI table |
| `audit-ai.md` | `ai_agent/`, chat, providers, mdkb | 25 + scope verdict |
| `audit-plugins.md` | plugin host, built-in dashboards, usage trackers | 19 + capability usage table |
| `audit-infra.md` | tunnels, relay, push, tailscale, MCP proxy, OAuth, ACP, dictation | 23 + mechanism inventory |
| `audit-fe-term.md` | CanvasTerminal, gridRenderer, transport.ts, panes | 25 + latency path |
| `audit-fe-shell.md` | App, hooks, 61 stores, settings, CSS, i18n | 25 + boot/polling/store tables |
| `audit-tooling.md` | Makefile, CI, deps, tests, docs, hygiene, churn | 25 + dependency/hygiene tables |
| `audit-mcp.md` | `mcp_transport.rs`, agent MCP tools, tasks, CLI | 25 + live-measured tool surface (bytes/tokens per handshake) |

All in `reviews/audit-2026-09-05/`.

---

## 11. Actionables (stories, 2026-09-05)

45 stories tagged `audit-sept` + `wave1|wave2|wave3|wave4`; `wiz-run stories-cli.js list` to browse.

| Wave | Stories | Chain |
|---|---|---|
| 1 | 641-660 | 641 (404) -> 642 (routes) -> 643 (probe, wave 2) -> 666 (docs) |
| 2 | 643, 661-667 | 665 (fixture gate) after 644, 645 |
| 3 | 668-676 | 668 (chunk path) after 644/645/646 -> 669, 670 - 671 after 657 - 672 after 654 - 673 after 647 - 676 after 656 |
| 4 | 677-685 | 677 (pty commands) after 668 - 678 (AppState) after 648/668/670/672/675 |

## 12. Corrections to this audit (2026-09-05, verified)

The Wave 4 review checked each §3 claim against source. Ten were wrong or
overstated. Recorded so a later pass does not act on them.

| Claim | Source says |
|---|---|
| §3.1 desktop-over-loopback is unbenchmarked | Benchmarked in the live WebView: loopback HTTP costs a constant +0.3-0.5 ms per call, flat across payloads from 149 B to 34.6 KB. Not a performance argument either way. Decision taken: keep two clients, close the drift class with 641 + 643 only. |
| §3.2 `pty.rs` has clean seams "already marked by banners" | One banner exists (`ChunkProcessor`, line 4228). The other seams are real but implicit. |
| §3.3 `AppConfig` mapping is a case conversion; "keep snake_case and drop the mapping" | TS already uses snake_case (`font_family`). What remains is semantic renaming (`config.font_family = state.font`), 96 sites. Codegen cannot remove it. The per-key patch command is the fix that bites. |
| §3.6 the project has no stated PTY-vs-ACP position | `plans/ego-acp-client.md` is a final contract: line 90 "no old-path fallback", line 704 rejects the hybrid route. The gap is placement, not decision. |
| §3.7 two competing dashboard mechanisms; Codex re-implements a `TtlCache<T>` | Hardcoded switch = built-in dashboards, `dashboardRegistry` = plugin dashboards. Legitimate split. No `TtlCache` exists in `src`. The uncommitted `features/agentUsage.ts` is already the shared layer. Only real defect: the Codex dashboard never refreshes. |
| §3.7 `plugin_credentials` is unused | Called from `claude_usage.rs:620`. Also: the plugin host is a published, versioned (`TUIC_SDK_VERSION = "1.0"`) API with three install paths and an external registry (`sstraus/tuicommander-plugins`). Unused-in-repo does not mean unused. Nothing here is safe to delete. |
| §3.8 two live sound implementations | Only `toasts.ts` matches `AudioContext`/`createOscillator`; no `rodio`/`play_sound` in `notifications.rs`. Unverified. |
| §3.10 drop `ulid`/`cuid2`; `Backend::Cli` is dead dispatch | ULID and CUID2 are distinct formats the generator panel offers on purpose; dropping them removes a feature. `Backend::Cli` is a documented one-line rollback marked `#[allow(dead_code)]` (`git_reads.rs:837`) — it needs an expiry date, not deletion. |
| §5 `sections.ts` already assigns deep-link ids | `sections.ts` is 6 lines with one constant. A settings search index must be built, not exposed. |
| §2 #16 i18n is a dead control to implement or delete | 801 `t()` call sites across 51 components are already key-tagged. Only the lookup is missing (~10 lines). Deleting would throw away the extraction work. |

Not actioned by decision: the internal AI agent (§3.4) is left untouched; the
plugin host (§3.7) has no safe deletions.
