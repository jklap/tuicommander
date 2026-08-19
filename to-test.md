<!-- tweak-comments v1: inline review comments.
     Format: [tweak:begin:ID]highlighted text[tweak:end:ID @ISO-TIMESTAMP
     comment body (free text, may span multiple lines)
     ] — where [ ] are the HTML comment delimiters <!-- -->.
     The only escape is '-->' → '--&gt;' inside the comment body.
     Read each comment, apply the feedback to the highlighted text,
     then remove the tweak markers. -->

# To Test

Features to test when TUICommander is more usable.

## Render cadence of AI answers and the phone terminal (2026-08-17, frontend only) — story `603-c28f`

F10/F130/F136/F137 from the performance audit. The chat panel renders the live answer on
its own 200 ms cadence instead of once per token batch, and the mobile terminal reuses the
elements of screen rows that did not change. Vite reloads this without a restart.

- [ ] **[MANUAL]** Ask the AI chat a question with a long answer (a few thousand words) and confirm the panel stays responsive to the end — it used to slow down progressively as the answer grew. Watch that code fences and tables still render correctly as they stream in.
- [ ] **[MANUAL]** Open a session on a phone or at `:9877` in a narrow browser window, run something with a busy full-screen redraw (`htop`, a TUI agent), and confirm the output stays smooth and the search filter reacts instantly while output flows.

## OSC 133 and OSC 7 cwd in browser mode (2026-08-18, **Rust change — needs `make dev` restart**) — story `623-d369`

Both markers reached the desktop `AppHandle` only, so a browser/PWA client had no command
blocks, no gutter marks, no Cmd+Up/Down navigation and a cwd frozen at session start. They
are dual-emitted now and carried on the `?format=grid` WS the canvas already holds. Nothing
below works against the currently running binary.

- [ ] **After a `make dev` restart**: open a session at `:9877` in a browser, run a few commands, and confirm command blocks appear — gutter marks beside each prompt, and Cmd+Up/Down jumping between them.
- [ ] **After a `make dev` restart**: in the same browser session, `cd` into a subdirectory and confirm the context bar's path follows. It used to stay pinned to the launch directory forever.
- [ ] **After a `make dev` restart**: run a failing command (`false`) in browser mode and confirm its block is marked as failed — that is the `exit_code` path, and a field-name drift would silently render every command successful.
- [ ] **After a `make dev` restart**: confirm desktop still works unchanged — the desktop emit was kept, not replaced.
- [ ] **[HUMAN]** Compare the gutter marks side by side, browser vs desktop, on the same session. Canvas painting is not observable over HTTP, so only a visual check proves the marks land on the same rows.

## Desktop PTY activity pulse (2026-08-17, **Rust change — needs `make dev` restart**) — story `625-56b0`

`cda39f31` deleted the `pty-output` emit and left the listener, so desktop lost every
activity signal for a commit. A payload-free `pty-activity-{id}` pulse replaces it,
throttled to ~1/s and dual-emitted so browser and desktop read the same signal.
Nothing below works against the currently running binary.

- [ ] **After a `make dev` restart**: open the Activity Dashboard, run `for i in $(seq 1 20); do echo $i; sleep 1; done` in a terminal, and confirm the `lastDataAt` column keeps advancing while it runs — it froze completely before.
- [ ] **After a `make dev` restart**: with tab A focused, start long output in background tab B and confirm B raises its unread-activity dot *while output is still flowing*, not only when the command completes. This is the case grid frames cannot report, since the canvas stops acking them while hidden.
- [ ] **After a `make dev` restart**: open the same session in browser mode (`:9877`) and desktop side by side; both must light up their activity indicator on the same output, since both now read one backend signal.
- [ ] **After a `make dev` restart**: confirm the mobile session list's last-activity time still reflects notable events (command start/end) and does NOT tick continuously during a long `tail -f`.

## Duplicate and orphan event listeners (2026-08-17, **Rust change — needs `make dev` restart**) — story `600-d664`

F4/F5/F7/F11/F17 from the performance audit. Only `CanvasTerminal` listens for OSC 133 now;
content-search batches and errors carry a `search_id`; the dead `pty-vt-log-total` emit is gone;
the AI chat panel no longer subscribes to the producerless chat registry; improvement-scan
proposals are published only by the `proposals-ready` event.

- [ ] **After a `make dev` restart** — F4, which the audit could only verify by inspection: open a shell terminal with OSC 133 shell integration (not an agent — agent panes are alt-screen and emit none), run three or four commands, then check the block list. Each command must produce exactly one block with a command line and an exit code, and no empty block between them. Cmd+Up/Down block navigation and the scrollbar command marks must step through real commands only.
- [ ] **After a `make dev` restart**: start a content search in the File Browser on a large repo and, while it is still streaming, open the command palette and run a different `?` search. Neither panel may show the other's matches, neither spinner may stop early, and — the case a single global cancel token creates — the File Browser spinner must still stop, keeping the matches it found before it was superseded.
- [ ] **After a `make dev` restart**: detach the File Browser into its own window and search in both it and the main window. The two ids are minted at random per search now, not from a per-realm counter that both windows would start at 1, so neither window may see the other's matches.
- [ ] **After a `make dev` restart** — the OSC 133 subscription moved ahead of the canvas font load, so the very first prompt marker of a session is no longer racing it: open a brand-new shell tab and confirm the first command already has a block (the first prompt used to be the one at risk of being dropped once the second listener was gone).
- [ ] **After a `make dev` restart**: open a saved conversation from the AI chat history and confirm its messages stay on screen (they used to blank a moment after loading).
- [ ] **After a `make dev` restart**: run an improvement scan and confirm the proposals appear exactly once, and appear in a second window too.

## Content index freshness after a timestamp-preserving restore (2026-08-17, **Rust change — needs `make dev` restart**)

`ContentIndex::is_current` compared modification times only, so a restore that preserves them left
the index reporting itself current with stale content. The stat fingerprint is now mtime **and**
size, both already read by the same walk.

- [ ] **After a `make dev` restart**: `search_code` for a phrase, then `cp -p` a different file over an indexed one and confirm the next search reflects the new content rather than the old.

## Dictation truncation and model-snapshot staleness (2026-08-17, **Rust change — needs `make dev` restart**)

The 300 s recording cap now reports what it dropped: `streaming_loop` counts the trimmed samples,
`stop()` returns them, and `TranscribeResponse.truncated_s` carries them to `useDictation`, which
says so instead of `Ready`. The model snapshot behind the 75 ms microphone meter expires after a
second so a change made by the other build is noticed.

- [ ] **After a `make dev` restart**: dictate normally and confirm the status still returns to `Ready`, then change the dictation model in a second window (or delete the model file) and confirm Settings > Dictation reflects it within about a second without a restart.

## Resumed session knowledge (2026-08-17, **Rust change — needs `make dev` restart**)

The startup load of session knowledge is capped at the 40 newest files, so a resumed older session
had no record in memory. Recording an outcome for it started a blank one, and the next flush wrote
that blank over the file. Both writers now read the file first.

- [ ] **After a `make dev` restart**: with more than 40 session-knowledge files present, reopen a session older than the newest 40, run a command in it, and confirm its earlier history survives in `<config dir>/ai-sessions/<id>.json`.

## Wave-1 perf runtime re-measure (2026-08-17, **Rust change — needs `make dev` restart**) — story `620-3281`

The unit tests cannot measure what these changes were made for. **Baseline, captured before the
`605-f104` fix:** the last 4000 log lines of the running instance held **360** `Emit repo-changed
(working-tree)` against **3** `(git-state)`. That ratio is what F40/F41/F42 has to move.

- [ ] **After a `make dev` restart**: re-measure `Emit repo-changed (working-tree)` vs `(git-state)` over ~4000 lines of `GET http://localhost:9876/logs` and compare against 360 vs 3.
- [ ] **After a `make dev` restart**: sidebar diff badges and branch stats still update while an agent writes in a worktree — the emit reduction must not cost responsiveness.
- [ ] **After a `make dev` restart**: Build Cleaner and the File Browser still behave after the `plugin_fs.rs` changes, and the app boots with no new warnings in `GET http://localhost:9876/logs`.

## MCP stdio backlog and SSE stream ownership (2026-08-17, **Rust change — needs `make dev` restart**)

The stdio upstream reader now uses a bounded, drop-oldest queue (256 lines / 8 MiB, 16 MiB per
line) instead of an unbounded channel, and one RPC waits on a single deadline instead of a
64-message budget. A `GET /mcp` SSE stream takes a process-wide generation on its session and its
teardown releases the session only while it still holds it.

- [ ] **After a `make dev` restart**: with a real stdio upstream configured (mdkb, context7), connect it, call one of its tools with a large argument, and confirm the tool list and the call still work — the request now crosses a writer thread instead of going straight down the pipe. Then restart the bridge/agent so its `GET /mcp` reconnects, and confirm `notifications/tools/list_changed` still reaches the agent afterwards. Finally close the agent (`DELETE /mcp`) and reconnect it, and confirm notifications still arrive.

## Working-tree read freshness and artifact trim accounting (2026-08-17, **Rust change — needs `make dev` restart**)

`get_working_tree_status` single-flight is now keyed by repository **plus** a generation counter that
every mutating git command bumps, so a coalesced read can no longer answer with a snapshot taken
before the mutation. A forced artifact rescan no longer joins a scan that started earlier, and
`trim_build_artifact` measures each target before removing it and returns the reclaimed bytes.

- [ ] **After a `make dev` restart**: stage a file in the Git panel and confirm the Changes list updates on the first refresh. Then open the Build Cleaner, trim a `target/` directory and confirm the reported size drops by what was removed, not by the old estimate.

## Repo watcher ignore rules (2026-08-17, **Rust change — needs `make dev` restart**)

`build` and `out` no longer count as build-output directory names; `.gitignore` decides, parents
included. `.git/info/exclude` and the root `.gitignore` are separate layers in git's own
precedence, and editing `info/exclude` now rebuilds the matcher.

- [ ] **After a `make dev` restart**: in a repo with a tracked `build/` or `out/` directory, edit a file there and confirm the git panel and file browser refresh. Then run a full `cargo build` and confirm the panels stay quiet — `target/` is still pruned.

## Rust-side plugin OutputWatcher matching (2026-08-17, **Rust change — needs `make dev` restart**) — story `599-6e94`

The `pty-output` throttle dropped every chunk inside its 100 ms window, which corrupted the plugin
OutputWatcher line reassembly in the WebView. Rust is now the only line assembler: the reader thread
reassembles, cleans and matches the lines, and pushes `pty-watcher-lines-{session}` batches (100 ms
window, ordered, and lossless through the batcher — delivery itself stays live-only, like every
other event on this bus). The raw `pty-output` event, its coalescer and the frontend `LineBuffer`
are gone. Watcher sets are per frontend, so a browser tab and the desktop window no longer overwrite
each other, and browser clients receive watcher matches for the first time.

- [ ] **After a `make dev` restart**: `claude-wakeup` still fires on a real `/done` line, and `at-capacity-retry` still fires on a real capacity line — both now matched in Rust.
- [ ] **After a `make dev` restart**: run `yes` in a terminal, then Ctrl+C. It must still interrupt promptly (this is the flood the throttle exists for), and the terminal must stay responsive.
- [ ] **After a `make dev` restart**: a rare line printed once, with the PTY then completely quiet, still reaches a watcher within ~100 ms (the ticker drains the tail; it no longer waits for more output).
- [ ] **After a `make dev` restart**: open the web UI alongside the desktop app and confirm a watcher fires in the browser tab, and that neither client blinds the other.
- [ ] **After a `make dev` restart**: reload the browser tab ten times (each reload leaves a client id behind, and the bound is 8), then confirm a watcher still fires in the desktop window within 30 s — the heartbeat has to re-install the set eviction dropped.

## Terminal copy gutter normalization (2026-08-17, **Rust change — needs `make dev` restart**) — story `622-6c69`

Terminal selection extraction now removes Claude's repeated `NBSP NBSP ▎` visual
gutter only from coherent multi-line runs. It continues to join soft-wrapped rows
and preserves literal block characters, indentation, bullets, numbering, emoji
shortcodes, and non-breaking spaces inside the message.

- [ ] **After a `make dev` restart**: copy the original long Claude message and paste it into Slack; no `▎` gutter or gutter NBSPs remain, while lists, blank lines, indentation, `:wave:`, `:pray:`, and the body spacing in `QA  Engineering` are unchanged.
- [ ] **After a `make dev` restart**: copy a lone `▎` and an ASCII-indented `  ▎` code/table line; both paste unchanged.

## Build Cleaner: Trim vs Clean (2026-08-16, **Rust change — needs `make dev` restart**) — story `598-e7fe`

Artifact rows gained a second action. **Trim** removes only regenerable intermediates
(Rust `<profile>/{deps,build,incremental,.fingerprint}`, Swift `index-build` + `ModuleCache`/`index`/`*.build`,
Maven `classes`/`generated-*`/`*-reports`, Gradle `classes`/`tmp`/`intermediates`/…) and leaves the built
executables on disk; **Clean** is the old full `remove_dir_all`. Measured on 5 real Rust repos:
113.9 GB of `target/`, 112.9 GB trimmable (98.2–99.8%), 1.04 GB of actual output.

- [ ] **After a `make dev` restart**: click **Trim** on the real `src-tauri/target` in the dashboard — not done on purpose, the row is 58 GiB and a Trim forces a full rebuild of the running dev app. Boss's call.
- [ ] **After a `make dev` restart**: a running `cargo build` in another window is not broken by a Trim of a *different* repo's `target` (the hot-window badge should mark the active one "recent").
- [ ] Visual: the two-tier button styling reads correctly in light and dark themes — `.safe` on accent, `.danger` on error — and the armed states ("Trim?" vs "Delete all?") are distinguishable at a glance.
- [ ] Cross-platform: on Windows and Linux, confirm a Trim of a Rust `target/` reclaims the same four dirs and leaves `*.exe`/the binary in place. The pattern matcher is separator-agnostic by construction and clippy compiles on Windows CI, but **Windows CI does not run the Rust tests** (`if: matrix.platform != 'windows-latest'`), so this is unverified at runtime.

## Authoritative agent state (2026-08-12, **Rust change — needs `make dev` restart**) — story `592-acde`

Question/choice lifecycle, causal capture replay, non-blocking MCP UI confirmation, lossless
session-state reducer, OSC 777 batching, shell-starting semantics, tall-HUD chrome cutoff, and
strengthened MCP `intent:` instructions. All Rust-backed, so none of it is live until a restart.

- [ ] **After a `make dev` restart**: a stale historical question does not re-arm after submitting a later turn.
- [ ] **After a `make dev` restart**: a bare Enter clears an active question or choice, on desktop *and* over HTTP/PWA input.
- [ ] **After a `make dev` restart**: leaving an MCP `ui action=confirm` dialog unanswered does not block requests from other agents.
- [ ] **After a `make dev` restart**: a non-empty Codex composer above a status HUD taller than 15 rows does not leak HUD text into question parsing.
- [ ] **After a `make dev` restart**: a newly spawned detected agent reports `starting` until real busy or idle evidence arrives.
- [ ] **After a `make dev` restart**: a newly initialized MCP agent emits `intent:` at task start and on material phase changes.

## Prompt-derived descriptions for orchestrated PTYs (2026-08-12, **Rust change — needs `make dev` restart**) — story `597-e4dc`

Codex collaboration exposes `task_name`/`message` but no `pty_description`; the backend now derives
display-only metadata from the original spawn prompt.

- [ ] **After a `make dev` restart**: a Codex collaboration subagent shows its task description above the terminal.

## Agent worktree creation prompt (2026-08-05)

- [ ] Create a worktree through MCP while an agent terminal is active, then choose **Open Worktree**: the worktree's terminal must open while the agent terminal remains attached to its original branch and working directory, with no injected stop/switch message.

## Perf pass + light-theme fix — visual checks (2026-06-09)
### 022-dc94 — Scrollbar track-height cache
- [ ] Smooth-scroll gesture: scrollbar thumb position/size stays visually correct (no jump/drift) _(controller logic covered by `src/components/Terminal/__tests__/canvasTerminalScroll.test.ts` — 4/4 passed on 2026-08-05; the scrollbar thumb still needs visual validation)_

### 027-deb3 — rowCache lagging-frame guard
- [ ] Fast scroll gesture: no flicker or wrong overscan content _(cache-generation and lagging-frame controller coverage is in `src/components/Terminal/__tests__/canvasTerminalScroll.test.ts` — 4/4 passed on 2026-08-05; flicker/overscan rendering still needs visual validation)_

## #79 — vim & repeating key (macOS press-and-hold) (2026-06-06)
- [ ] [HUMAN] In a release `.app`, open vim and hold `j`/`l`/`i` → cursor repeats, NO accent picker popup _(needs release build: dev build lacks proper bundle domain; fix registers `ApplePressAndHoldEnabled=NO` in `press_and_hold.rs`, called from `lib.rs` setup)_
- [ ] [HUMAN] Typing accented chars still works where intended (Option-key composition path unaffected — only the hold-for-accent picker is suppressed)
- [ ] [HUMAN] A user with explicit global `defaults write -g ApplePressAndHoldEnabled -bool true` still sees their override (registration domain is lowest priority)

## Content Index Strategy (2026-05-24)
- [ ] Set "Active repo only": switch repos → content search only works for the repo that was active at boot _(NOTE: boot pre-warm selects only the persisted active repo (`lib.rs:1419-1442`), but `content_index.rs:438-440` rebuilds any repo receiving `RepoChanged` whenever the strategy is not `disabled`; the claimed post-switch exclusivity is therefore not proven and may not match current behavior.)_
- [ ] "Active + on switch": warm_content_index fires on repo switch (check logs for index build for the new repo) _(NOTE: current source registers `warm_content_index` as an IPC/HTTP command (`fs.rs:472-477`, `mcp_http/fs_routes.rs:274`) but has no frontend caller on repository switch; the repo-change updater rebuilds enabled indices asynchronously via `content_index.rs:438-440`. This specific `warm_content_index` event contract is not proven and may be a story-worthy documentation/implementation mismatch.)_

## AI Chat (Level 1)
- [ ] Ollama selected + running: green dot, model list populated from /api/tags _(browser verified: the live Providers UI populated `Available: qwen2.5-coder:3b-instruct-q4_K_M, qwen2.5-coder-3b-mlx, qwen3.5:4b-mlx-bf16`; the required green availability dot is not rendered.)_
- [ ] Ollama selected + not running: red dot with "Not detected" message _(NOTE: `detect_ollama` returns `{available:false, models:[]}`, but `ProvidersTab.tsx` does not render a red dot or `Not detected` message.)_
- [ ] Context lines slider: 50-500, persists across restart _(NOTE: no context_lines slider exists in the codebase. AI Chat tab only has temperature slider and scheduled tasks. Feature not implemented.)_
- [ ] Status bar: chat bubble icon toggles panel, highlighted when active _(NOTE: toggle works (StatusBar.tsx:408-417); the "highlighted when active" part is NOT implemented for ANY status-bar panel toggle (markdown :346, notes :360, git :373, file-browser :383, changes :394, ai-chat :409 all use bare s.toggleBtn; no .toggleBtnActive class exists). Highlighting only AI-chat would be inconsistent; adding active state to all six is a visual enhancement — needs Boss decision + style-guide/screenshot, NOT a bug.)_

## AI Chat — Detachable Panel (1388-9bda)
- [ ] Detached window receives streaming chunks from active conversation _(NOTE: `AIChatPanel.tsx:39-42` explicitly documents that detached-window stores are separate and streaming/controls are not fully synchronized; generic panel projection sync is not registered for the AI Chat adapter in `App.tsx:124-132`.)_
- [ ] Closing detached window emits `ai-chat-window-closed` event _(NOTE: the generic bridge listens for `panel-window-closed`, but no AI-specific `ai-chat-window-closed` emission was found; current contract is `panel-window-closed` in `useDetachedPanelBridge.ts:12-16`.)_
- [ ] Send message from main window → stream visible in detached window _(NOTE: detached AI Chat has separate stores and the adapter has no `serialize`/`syncIntervalMs` projection; source comment at `AIChatPanel.tsx:39-42` identifies this as unresolved.)_
- [ ] Close detached window mid-stream → main panel resumes with partial text _(NOTE: no cross-window AI conversation projection is registered; the generic panel close bridge restores UI state but does not transfer streaming text.)_
- [ ] Switch terminals in main window while detached → subscription updates chatId _(NOTE: the AI Chat adapter passes the initial `chatId` only; no AI-specific panel-action handler or projection sync was found in `App.tsx:124-132`.)_

## AI Agent — Level 2 Loop (1299/1300/1301/1302)
- [ ] Rejoining session after reload: agent state recovered from store; tool-call history preserved (schema v2) _(PARTIALLY CONFIRMED: chat messages reload via initFromDisk at conversationStore.ts:443-494 (schema v1). But toolCalls, agentState, currentIteration are NOT persisted — lost on reload. "schema v2" is for session knowledge files in knowledge.rs:51, not conversation store.)_

## Smart Prompts Drawer (Cmd+Shift+K)
- [ ] Open Cmd+Shift+K → drawer shows compact prompt list with badges (inject/headless, built-in, placement) _(browser reproduced 2026-08-05: the drawer shows categories/descriptions and `Manage Smart Prompts…`, but no editor/badges; activating Manage leaves Settings on existing tabs such as General/Providers because `SmartPromptsTab` is not registered; tracked in story `548-aedb`)_
- [ ] Click Edit on a prompt → modal shows variable dropdown under Content textarea _(blocked and browser-confirmed by the missing Smart Prompts settings navigation; tracked in story `548-aedb`)_
- [ ] Click a variable in dropdown → inserts `{variable}` at cursor in textarea _(blocked and browser-confirmed by the missing Smart Prompts settings navigation; tracked in story `548-aedb`)_
- [ ] Execution Mode and Auto-execute appear side by side _(blocked and browser-confirmed by the missing Smart Prompts settings navigation; tracked in story `548-aedb`)_
- [ ] Auto-execute ON → prompt sends Enter automatically after injection _(requires the missing settings editor and an agent session; tracked in story `548-aedb`)_
- [ ] Auto-execute OFF → prompt text pasted without Enter, user can edit before sending _(requires the missing settings editor and an agent session; tracked in story `548-aedb`)_

## Plan Panel (515-660c / 516-41a5 / 517-74c2)
> **OBSOLETE (panel removed 2026-04-02, commit `123f7a2c` "refactor(plan): remove HTML panel"; sidebar panel also dropped, `1634e0b1`; stale doc refs cleaned in `331bd649`).** The plan feature is now plugin-only (`planPlugin.ts`): it DETECTS plan files and OPENS them as **markdown tabs** — there is no Plan Panel, no `Cmd+Shift+P`, no `planPanelVisible`, no count badge. Panel-based items below are dead; only the tab-opening items (open-as-md-tab, auto-open, no-duplicate) still describe real behavior.
- [ ] Switching repos rescans plans for the new active repo (no panel — affects which plans auto-open as tabs) _(NOTE: `planPlugin.ts:103-106` scans only during plugin load, and exported `scanPlans()` at `:264-266` has no caller elsewhere in `src`; an active-repo switch rescan is not currently demonstrated)_

## Voice Dictation (Stories 117-123)
### Model Management
- [ ] Model status shows "Ready" after download completes _(NOTE: `DictationSettings.tsx:29-35` renders `Downloaded`/`Active`; no `Ready` status is currently implemented.)_

## Native System Menu Bar (Stories 192 + 193)
- [ ] HelpPanel shows note about system menu bar _(NOTE: HelpPanel.tsx has no mention of the system menu bar. Panel shows About, Keyboard Shortcuts, UI Legend, and resource links only; tracked in story `550-fb23`.)_

## File Browser Content Search (807-e295)
- [ ] Results stream in progressively, grouped by file with match count _(browser regression reproduced 2026-08-05: searching `AGENTS` in the live web UI remained on `Searching…` after 3 seconds with no rows; tracked in story `547-9bef`)_
- [ ] Each result row shows file path, line number, and highlighted match context _(blocked by the live frontend search remaining on `Searching…`; backend response contains path, line number, and highlighted match offsets)_
- [ ] Click a result opens the file in code editor at the matched line _(blocked because the live browser rendered no result rows during the reproduction)_

## Smart Prompts API Mode
- [ ] Select provider (OpenAI/Anthropic/etc.) → model placeholder updates _(browser verified: the Add Model form keeps the static placeholder `e.g. claude-sonnet-4-5-20241022`; current `ProvidersTab.tsx` has no dynamic provider-specific placeholder.)_
- [ ] No API key configured → canExecute returns error with Settings link _(NOTE: `useSmartPrompts.ts:109-125` returns the plain reason `Headless provider not configured — add a provider and assign the Headless slot in Settings → Providers`; no clickable Settings link is produced.)_
- [ ] PWA/browser → API mode shows "requires desktop app" message _(NOTE: browser transport maps `execute_api_prompt` through HTTP, and no `requires desktop app` guard/message exists in the API execution path.)_
- [HUMAN] Wrong API key → toast shows "Authentication failed" with Settings hint

## ChoicePrompt (story 1296-ce3e)
- [ ] Agent resumes work (status-line emits) → `choice_prompt` cleared, overlay disappears _(NOTE: current test `test_session_state_status_line_keeps_choice_prompt` explicitly preserves the prompt during status-line repaint; the checklist expectation does not match current behavior.)_
- [ ] Codex numbered-choice dialog (if/when encountered) captured by parser — add fixture if not _(NOTE: `output_parser.rs:93-95,1754-1782` documents and implements the shared numbered-choice shape for Codex, but no Codex screen capture/fixture exists in the current corpus; do not invent one.)_
- [ ] Aider confirmation dialog — add fixture if layout differs _(NOTE: the parser documents the same cross-agent layout, but no Aider capture/fixture exists in the current corpus; a live Aider prompt is required before adding evidence.)_

## Command Block System (2026-05-20)
- [HUMAN] Cmd+F with block-scoped toggle ON → only matches within current block shown
- [ ] Settings > Terminal > Blocks → toggle timestamps and folding on/off _(NOTE: the current settings state persists `show_block_timestamps` and `block_folding_enabled`, but no matching Settings-panel controls were found; the only current controls are the runtime modifier/shortcut paths in `CanvasTerminal.tsx:2106-2110,2165-2183`.)_
- [HUMAN] Run 500+ commands → oldest blocks evicted, no crash or memory growth
- [HUMAN] Claude Code session: tool calls show as blocks without OSC 7770 (heuristic detection)

## Process Monitor
- [ ] Panel: changing refresh interval to Manual stops auto-polling _(NOTE: live `/process/monitor` HTML has no refresh-interval selector or Manual mode; it only auto-refreshes on the fixed 3-second timer.)_
- [ ] Panel: Refresh button triggers immediate data fetch _(NOTE: live `/process/monitor` HTML has no Refresh button; only the fixed timer is implemented.)_

## Search/UI consistency: unified SearchBar + scrollbar overview + file-browser tracking (2026-06-11)
- [VISUAL] Live (needs rebuild): open Cmd+F in the code editor → compact SearchBar pill (counter inside input); typing shows orange full-width ticks covering the scrollbar and hides the green git ticks; closing search brings the git overview back. Replace row expands via the chevron.
- [VISUAL] Live (needs rebuild): open a brand-new (untracked) file → the git-change overview shows a SINGLE tick at the top, not a solid green bar.
- [VISUAL] Live (needs rebuild): search inside a diff tab → orange match ticks appear on the diff scrollbar and track scroll.
- [VISUAL] Live (needs rebuild): editor scrollbar visually matches the terminal's (14px track, rounded inset thumb).
- [VISUAL] Live (needs rebuild): open a file deep in a subtree → the file browser (tree view) auto-expands its parents and scrolls it into view, highlighted with the accent bar. Switching the active editor tab moves the highlight.
- [VISUAL] Live (needs rebuild, story 443-ea2b): install/enable the `docx-preview` plugin, open a `.docx` from File Browser, and confirm it opens the Mammoth HTML preview panel with conversion notes/raw-text toggle; Edit opens the same file in CodeMirror. Requires backend restart because `host.readFileBase64()` is Rust-backed.
- [ ] DEFERRED (story 041-cd15): HTML Preview search → shared SearchBar (in-iframe search needs a postMessage bridge); shared sidebar-filter component for Error Log / Knowledge History / Branch Switcher etc. ("consistency of a different kind" for narrow sidebars).
- [VISUAL] Live (story 040-29e1): open a tracked file in the code editor → a dim italic annotation "Author · relative time · summary" appears at the end of the active line and follows the cursor (no flicker, no fetch per keystroke). Edit a line → it shows "You · Uncommitted changes". Toggle `settingsStore.setInlineBlameEnabled(false)` → annotation disappears. External (absolute-path) files show no annotation.

## Markdown preview: inline comments anchor + highlight correctly (2026-07-15)

- [ ] [VISUAL] Commenting a word that repeats many times in the doc (e.g. "reason" ×18) highlights the ACTUAL selected occurrence, not the first one. _(root cause: `findSourceMatch` used first-occurrence `indexOf`; fixed with DOM occurrence-ordinal → Nth source occurrence. Logic verified in `tweakComments.test.ts` incl. real-file offsets; visual anchor position needs an eye.)_
- [ ] [VISUAL] Selecting text overlapping an existing highlight hides the "Add comment" button; keyboard-selecting over one and saving shows "That text already has a comment" instead of silently nesting/vanishing. _(logic verified: overlap-rejection + OverlappingCommentError; DOM pre-filter `rangeIntersectsHighlight` needs a visual check.)_

## Native key monitor: F13-F20 + Ctrl+Tab (#495-ec28, 2026-07-28, Rust — needs `make dev` restart)

- [ ] Help > Keyboard Shortcuts > pencil on any action, press **F13** (or F16-F20): the combo is recorded and persists. Repeat with a modifier (Cmd+F13) — the recorded string must match what a normal key produces. Same for the Global Hotkey field at the top of the tab.
- [ ] If **F14/F15** record nothing: check `curl 'http://localhost:9876/logs?source=native-keys'`. A "extended function key observed" line means AppKit delivered it and the gap is downstream; no line means macOS consumed the key system-wide for keyboard illumination — remap it in System Settings, not a bug here.

## Claude/Codex stay busy with a LIVE agent (#497-4e67, 2026-07-29, Rust — needs `make dev` restart)

Data integrity and the slash-menu log flood are already verified automatically —
`tests/terminal-stress/run.py` passes all four scenarios at 2000 records against a
HEAD build, with zero `slash_menu` log records. What is left needs REAL agents:

- [ ] Give Claude a long tool call (something taking minutes) while its empty `❯` composer stays visible: the tab must stay busy for the whole call, not flip idle. Then let it finish — the completed `✻ …ed for 1m 25s` summary must go idle normally.
- [ ] Claude with a **blocking Stop hook**: the tab must NOT settle to completed while the hook is still doing visible work, and the follow-up suggestions from the premature Stop must be discarded rather than left on screen.
- [ ] Codex v0.145+ with a background terminal: the `»` composer row (not the historical `›`) must anchor the Working marker, and the tab stays busy.

## Cross-repo content search covers every registered repo (#483-7b93, 2026-07-29, Rust — needs `make dev` restart)

- [ ] Command palette, `?OPENROUTER` with **Search all repos** on: matches appear from repos you have NOT opened this session. Before the fix only repos with a warm index were searched (with the default `active_and_switch` strategy that is the active repo alone), so 33 of 34 registered repos were skipped.
- [ ] Immediately after a cold start the same search must report `N still indexing, retry shortly` rather than a bare **No results** — the empty result reading as a confident miss is the actual bug being fixed.
- [ ] Repeat the search a few seconds later: the pending count drops and matches appear, proving `ensure_index` was kicked off by the first search rather than the repo staying invisible forever.

## Codex busy while a background terminal runs (#482-33ec, 2026-07-29, Rust — needs `make dev` restart)

- [ ] Start a background terminal from Codex (something long, e.g. `sleep 120 &` via its own runner) so its status row reads `• Waiting for background terminal (Ns • esc to interrupt)`. The tab dot must stay **busy (non-green)** for the whole wait. Before the fix the verb swap flipped it idle within seconds and nothing could re-enter busy until the next user submission.
- [ ] While it is waiting, confirm the session is NOT put into standby (auto-standby SIGSTOPs an idle session — a false idle here would suspend Codex mid-work).
- [ ] After it finishes, the transcript line `• Waited for background terminal · <cmd>` (past tense, no `esc to interrupt`) must NOT hold the tab busy — it should go idle normally.

## Off-domain OAuth authorization servers no longer blocked (2026-08-01, Rust — needs `make dev` restart)

- [ ] Authorize an MCP upstream served through a gateway whose AS metadata carries a different `issuer` (e.g. a `*.mcp-s.com` tenant): the flow must reach the consent dialog instead of failing with "Issuer mismatch … mix-up attack".
- [ ] That dialog must be the **warning** variant and say the authorization server is on a different domain, naming the AS origin — and Cancel must still abort cleanly (upstream back to `needs_auth`).
- [ ] Authorize an upstream whose AS is on the same registrable domain: dialog stays the plain `info` variant with no cross-domain sentence.
- [ ] `curl 'http://localhost:9876/logs?source=mcp_oauth'` after the gateway case shows the `AS metadata issuer differs from the discovery URL` warning (warn, not error).
- [ ] Regression: an upstream with an explicit `authorization_endpoint`/`token_endpoint` override still skips discovery and gets the plain dialog.

## Worktree mid-rebase stays alive (2026-08-01, Rust — needs `make dev` restart)

- [ ] Finish the rebase (`--continue` through the conflicts): the row must survive the whole way and settle back on its branch.
- [ ] `git rebase --abort` from the worktree: row still there, branch restored, no prompt.
- [ ] Regression: delete a worktree's branch with `git branch -D` while the worktree exists and no operation is running — the archive/delete prompt MUST still appear.

## Grok returns to idle after a long turn (2026-08-01, Rust — needs `make dev` restart)

- [ ] Watch the tab dot during the turn: it must stay busy while the `⠋ Waiting for response…` row is animating, and only then go green.
- [ ] Regression: Aider — its Knight Rider `█░` spinner must still hold the tab busy (the fix only excludes a row trimmed to a *single* block glyph).
- [ ] Regression: a short Grok turn whose output fits the viewport (no scrollbar column) still goes idle as before.

## OpenCode returns to idle after a finished turn (2026-08-02, Rust — needs `make dev` restart)

- [ ] Run an OpenCode turn: the tab dot must go green within seconds of the composer coming back, instead of sitting busy for the whole process.
- [ ] Watch the dot DURING the turn (including a tool phase such as a long `bash` call): it must stay busy while the footer shows `⬝⬝⬝■■■  esc interrupt`.
- [ ] Auto-standby must not SIGSTOP an OpenCode session mid-turn.
- [ ] Regression: with OpenCode exited and a plain shell on screen, the session must not report Ready off the leftover frame (the adapter requires both the `┃`/`╹▀▀▀` frame and the `ctrl+p commands` status bar).

## Screen adapters still missing for amp / cursor / goose / droid (2026-08-02, audit — blocked on installs)

Audited while fixing OpenCode (#535-d4f5): these four have no ready-screen adapter, so if
their foreground command is long-lived they hit the same OSC 133 "busy forever" failure.
None of the binaries is installed on this machine, and writing an adapter from documentation
rather than a live capture is exactly how grok shipped green tests over a stuck UI.

- [ ] [HUMAN] Install `amp` and capture its idle + mid-turn screens, then decide whether it needs an adapter.
- [ ] [HUMAN] Same for `cursor-agent`.
- [ ] [HUMAN] Same for `goose`.
- [ ] [HUMAN] Same for `droid`.

## Alternate-screen scrollback (2026-08-03, Rust — needs `make dev` restart / `make build`)

Reported by Boss: `gh run watch <id>` renders but has no scrollbar. Root cause: the alternate
screen had scrollback capacity 0, so `historySize` was always 0 and the scrollbar hid itself.
Now the separate alt grid keeps the lines that scroll off, with the same user-visible result as
iTerm2's save-to-scrollback option but a different internal architecture. Backend is covered by
tests replaying a real `gh run watch` PTY capture
(`src-tauri/src/fixtures/alt_screen/gh-run-watch.raw`); canvas rendering is not observable over HTTP.

- [ ] [VISUAL] Open `vim`/`htop`/`lazygit`: wheel still goes to the app; `Shift+wheel` scrolls TUIC history; quitting restores the shell scrollback unchanged. _(only the mouse-reporting forwarding is left unverified — the enter/exit half is covered above.)_

## tuic CLI: repo opening + command ergonomics (2026-08-03)

The `tuic` sidecar was rebuilt (`node src-tauri/build-sidecar.mjs`), so the CLI half is live
immediately. The app half — the `open-repo` deep link adding an unknown folder — is frontend
code and needs the WebView to have reloaded.

- [ ] [HUMAN] `tuic <dir>` on a folder NOT in the sidebar: one confirmation appears, then the repo is added and activated exactly like the "Add Repository" button (branch selected, terminal opened, watcher started).
- [ ] [HUMAN] `tuic run pnpm dev` in a repo: a session appears and the command is running in it.

## Docs site: Pagefind search + coverage (2026-08-05, docs only — no app rebuild needed)

mdBook's elasticlunr search (a 4.2 MB index behind a magnifier icon nobody found) is replaced by
Pagefind: same magnifier button in the menu bar, plus a big hero search box on the docs landing page.
`SUMMARY.md` went from 41 to 71 published chapters — the backend/, frontend/, sync-matrix,
release-checklist and audit docs were never rendered, so they were never searchable either.
Build with `make docs-serve` (needs `mdbook` + `npx`); CI runs the same `scripts/build-docs.sh`.

- [ ] [HUMAN] Deployed site (tuicommander.com/docs) after the next `main` push: search works over GitHub Pages, and the first query feels fast.
- [ ] [HUMAN] Mobile/narrow viewport: the menu-bar search panel and the hero box stay usable.

## Smart Prompts settings tab + HelpPanel system-menu note (2026-08-05, frontend only — Vite HMR is enough)

`SmartPromptsTab` existed but was never registered in `SettingsPanel`, so the drawer's
"Manage Smart Prompts..." landed on General. Now it is a nav entry (`smart-prompts`) and the
drawer opens it directly. Separately, HelpPanel gained the missing note pointing at the native
system menu bar (desktop only — browser mode has no native menu).

- [ ] Visual pass: the Smart Prompts tab layout (category groups, expanded editor) and the HelpPanel note spacing under Quick Actions. No dev instance was running (9876/1420 refused), so nothing was screenshotted.

## File Browser content search in browser mode (2026-08-05, frontend only — Vite HMR is enough)

Desktop `search_content` returns void and streams matches as `content-search-batch` events;
the HTTP route returns the whole result in the response body and pushes nothing, so a browser
client stayed on "Searching…" forever. `startContentSearch()` now republishes the HTTP body as
one final batch, locally — not over the `/events` SSE bus, which is global and would leak one
client's hits into every other client's panel.

- [ ] End-to-end in a real browser against a running instance: type a query in File Browser content search and confirm rows appear and a click opens the editor at the line. No instance was running during this work.

## GitHub panel keyboard UX + persisted collapse (2026-08-05, **Rust change — needs `make dev` restart**)

Arrow/Enter navigation across the three GitHub panel sections, plus collapse state persisted through
`save_ui_prefs`. PrSection is now fully controlled by GitHubPanel (collapse, expansion, dismissed
PRs lifted) so the navigable row list and the rendered rows cannot drift apart.

- [ ] **After a `make dev` restart**: collapse a section, quit, relaunch — the section is still collapsed. Without the restart the Rust `UIPrefsConfig` field is absent and serde silently drops it, so it only persists in-session.
- [ ] Visual: the `.ghItemRowActive` highlight (inset accent bar) reads clearly in light and dark themes, and the active row scrolls into view on long lists.

## Compose enqueue + MCP attention callback (2026-08-08, **Rust change — needs `make dev` restart**)

Compose panel gained a second submit that queues instead of steering, and the `ui` MCP tool
can now raise a named notification sound (new `attention` callback). Both are Rust-backed, so
nothing below works against the currently running binary.

- [ ] Luna: prove a user command never overtakes an earlier peer message and that one idle window submits only the shared FIFO head (`enqueue_never_overtakes_a_command_already_waiting`).
- [ ] Luna: prove Compose count/clear select only user commands and preserve every peer entry (`clear_queued_commands_preserves_peer_deliveries`).
- [ ] **After a `make dev` restart**: with an agent mid-turn, `Shift+Ctrl+Enter` a prompt, watch the badge show `1 queued`, and confirm the prompt is typed the moment the agent finishes — not before.
- [ ] **After a `make dev` restart**: queue two prompts, confirm they arrive in order across two idle windows, and that clicking the badge discards them.
- [ ] **After a `make dev` restart**: confirm a generic OSC 777 `Claude Code needs your attention` completion notice does not leave the tab awaiting, while plan/skill pickers still do.
- [ ] **[HUMAN] Listen to the selected `attention` sound in the rebuilt app** — Settings > Notifications > Attention > Test. Boss selected sample C on 2026-08-09: triangular G4→G4→E5, 75/75/140 ms, 50 ms gaps, gain 0.8. Confirm the native engine matches the approved direct-synthesis sample and remains identifiable from another room without being irritating.
- [ ] **After a `make dev` restart**: `ui action=toast sound="attention"` from an MCP client shows the toast **on the desktop app** (it never did before — the event was bus-only) and plays the callback; muting Attention in Settings silences it while the toast still appears.

## Detached AI Chat window (2026-08-18, frontend only) — story `624-a6c3`

The detached chat now loads the conversation it was detached from and can send to the
terminal it was detached from. Vite reloads this without a restart.

- [ ] Detach the chat, type a message in the separate window, confirm it sends and the reply streams there (it was read-only before — the textarea said "Focus a terminal first").
- [ ] Close the detached window, reopen the panel in the main window, confirm the exchange is there.
- [ ] Detach from terminal A, switch the main window to terminal B, send from the detached window, close it, then switch back to A and confirm A shows the new messages.
- [ ] Detach with a non-terminal tab focused and confirm the window is read-only, with the "No terminal focused" banner rather than a broken send.

## Post-fix verification leftovers (2026-08-19) — stories `627-4571`, `628-7148`, `616-71e1`
### `627-4571` — needs the desktop app

- [ ] **[HUMAN]** A browser/PWA client shows command blocks, OSC 133 gutter marks, Cmd+Up/Down navigation and a tracked cwd. _(transport and cwd proved on :9877 — OSC133 A/C/D frames captured including `D exit_code=1` and `D exit_code=0`, cwd `/private/tmp` then `/` — but the gutter never rendered because no canvas ever sized. That specific question is now story `631-e618`; this item is the human confirmation once it is settled)_
- [ ] **[HUMAN]** Desktop still gets both events after the payload change: `pty-cwd` now carries `{cwd}` instead of a bare string. _(code confirms the desktop emits at `pty.rs:4777-4787` and `:4806-4811` with bus emits following; no desktop runtime observation is possible from a second instance)_
- [ ] **[HUMAN]** Frame bytes/s before and after on a busy session, confirming the ~3x drop the binary format predicts. _(post-fix side measured: 596 720 B over 16 frames. The "before" is unobtainable — no pre-fix binary exists any more, and a post-fix process cannot yield an honest baseline. Either accept the post-fix number alone or rebuild a pre-fix commit deliberately)_
- [ ] **[HUMAN]** A background tab still rings its bell, and switching back repaints immediately with no stale rows. _(needs audible delivery and row freshness by eye; hidden frames are decoded and ring at `CanvasTerminal.tsx:1358-1393`, show requests a full repaint at `:2121-2158`)_
- [ ] **[HUMAN]** A browser client and the desktop app streaming the same session both keep painting — neither steals the other's dirty rows. _(needs the desktop WebView alongside a browser; per-client gate rationale at `grid_gate.rs:1-21`, WS recovery at `mcp_http/session.rs:1481-1492`)_
- [ ] **[HUMAN]** A plain file save logs `Emit repo-changed (working-tree)` and the Git panel's Log/History/Stashes do NOT re-run their git processes; a `git commit` logs `(git-state)` and they DO refresh. _(requires mutating a real repo. Tabs depend on `getGitRevision` — `LogTab.tsx:207-224`, `HistoryTab.tsx:88-99`, `StashesTab.tsx:57-69`; `bumpGitRevision` moves both counters at `repositories.ts:779-795`)_
- [ ] **[HUMAN]** FileBrowser still creates, renames, copies, moves and deletes files, and the editor still opens and saves. _(every one of those commands changed sync → async; entry points at `fs.rs:370, 504, 1379, 1396, 1434, 1451, 1468, 1505, 1530`. Exercising it necessarily mutates a repo)_
- [ ] **[HUMAN]** Dropping a large folder still freezes the UI. **This confirms a deliberate gap, it is not a regression** — `fs_transfer_paths` was intentionally left synchronous because it is the drag-drop backend and D&D needs Boss's approval. _(the gap is intact and documented at `fs.rs:1615-1629`; the native Finder→Tauri drop cannot be driven from browser mode)_
- [ ] **[HUMAN]** AI agent `read_file` / `write_file` / `edit_file` / `list_files` / `search_files` / `search_code` still work on the blocking pool. _(all six mapped in `ai_agent/tools.rs:2460-2477`, dispatched through `spawn_blocking` at `:2525-2534`; write/edit need the desktop confirmation UI)_
- [ ] **[HUMAN]** Terminal search, Cmd+F next/previous, click-to-open file links, OSC 8 hyperlink hover, selection copy and scrolled-back history all still work. _(every one of those grid reads changed sync → async and now returns a `Result`; reads use `vt_read`/`vt_try_read` on the blocking pool at `pty.rs:10167-10227`, search/selection/lines/hyperlinks async at `:10318-10442`. Needs trusted keys, hover and clipboard inspection)_
- [ ] **[HUMAN]** With a session producing heavy output, dragging a selection or typing in the search box no longer stalls the WebView. _(this is the freeze the finding is about; offload mechanism at `pty.rs:10167-10227`. Needs a visible live canvas and timed main-thread observation)_
- [ ] **[HUMAN]** Wheel, scrollbar drag, Cmd+Up/Down and Home/End still land where expected. **The four scroll/frame mutations were deliberately NOT converted**, so this confirms they still behave. _(`terminal/scroll-to` probed clean over HTTP — 200, `display_offset=120` — but that proves the endpoint, not the four UI input routes. Rationale at `pty.rs:10143-10164` and `:10271-10281`, wheel/drag coalescing at `:10229-10245`)_

### `628-7148` — copy normalisation, content proven, rendering not

The text the copy produces is fully asserted by 9 green Rust tests
(`cargo nextest -E 'test(copied_selection)'`). What is left is only how a paste
target renders it.

- [ ] **[HUMAN]** Copy a Claude blockquote, paste into Slack, confirm no gutter bars. _(content proven by `copied_selection_strips_repeated_claude_gutters`, `_strips_space_indented_gutters`, `_accepts_nbsp_separator_and_preserves_body_nbsp`)_
- [ ] **[HUMAN]** The pasted paragraph has no mid-sentence line breaks; bullets and blank lines keep their own line. _(proven by `copied_selection_rejoins_rows_claude_wrapped_for_width`, `_rejoins_bullet_continuations_but_not_the_next_bullet`, `_stops_rejoining_after_the_wrapped_paragraph_ends`, `_normalizes_after_unwrapping_soft_wrapped_rows`)_
- [ ] **[HUMAN]** Copying a short hand-written quote or a code block is unchanged. _(the over-reach guards: `copied_selection_keeps_lone_or_non_claude_gutters`, `_keeps_deliberate_breaks_in_a_short_quote`)_

### `616-71e1` — measurement gaps left open on purpose

The story is complete; these are the two things its own document records as not
obtainable, kept here so nobody re-derives them from scratch.

- [ ] **[HUMAN]** Reproduce the F120 drag-drop freeze by dropping a large folder from Finder, and time it. _(the residual is real and narrowed to one command, `fs_transfer_paths` at `fs.rs:1624`, sync on the macOS main thread while every sibling moved to `spawn_blocking_fs`. Thread evidence was measured — `ps -M` 86 threads, `sample` ties WebKit IPC to `com.apple.main-thread` — but the freeze itself was not reproduced)_
- [ ] **[HUMAN]** Re-measure watcher emit suppression under real repo churn. _(`head_emits_suppressed` measured 0/min, but only on an idle instance with no repo mutation in the window. That is a floor: it proves quiescence, it does not re-measure the `repeat_count: 12` storm behind issue #82)_
