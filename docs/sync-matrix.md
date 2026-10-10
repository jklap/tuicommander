# Documentation Sync Matrix

Every code change that affects user-visible behavior, APIs, or configuration MUST update the corresponding documentation files. This matrix maps codebase areas to their docs.

**When a claim about the same mechanism appears in more than one doc** (e.g. the same feature described in both `docs/FEATURES.md` and a `docs/user-guide/*.md` page), grep for the distinguishing term across every sync-matrix-mapped doc before considering a fix complete — fixing one copy and missing a sibling copy leaves the docs contradicting each other. This happened for real: a dependency removal was fixed in `docs/FEATURES.md` but left stale in `docs/user-guide/terminals.md` and `CHANGELOG.md`, and survived two subsequent "fix stale claims" passes over the same doc before being caught.

## New Feature Checklist

- [ ] Feature works correctly
- [ ] Keyboard shortcut added (if applicable) — `keybindingDefaults.ts` + `actionRegistry.ts` ACTION_META
- [ ] `docs/FEATURES.md` updated with new feature entry
- [ ] `CHANGELOG.md` — entry in Unreleased section
- [ ] `SPEC.md` — feature status updated
- [ ] Domain-specific docs updated (see matrix below)
- [ ] Screenshot taken (if visual/CSS/layout change)
- [ ] `src/data/tips.ts` — add a Tip of the Day entry for discoverable features

## Sync Matrix by Area

### Plugin System
When modifying PluginHost API, capabilities, manifest schema, Tauri commands used by plugins, plugin panel rendering (base CSS, theme injection, iframe behavior), or plugin infrastructure (loader, registry, discovery):

| File | What to update |
|------|----------------|
| `src/plugins/types.ts` | PluginHost interface, PluginCapability union, snapshot types |
| `src/plugins/pluginRegistry.ts` | Implementation in `buildHost()` |
| `src/components/PluginPanel/pluginBaseStyles.ts` | Base CSS classes available to all plugin panels |
| `src-tauri/src/plugins.rs` | `KNOWN_CAPABILITIES` list (new capabilities); `set_plugin_output_watchers` sync |
| `src-tauri/crates/tuic-terminal/src/output_watchers.rs` | Rust-side OutputWatcher matching: `WatcherSpec`, `OutputWatcherRegistry::sync` (per-client sets; which patterns are rejected back to the frontend), `to_portable_pattern` (ECMAScript class escapes — Rust may over-match, never under-match), `clean_line` — a **port** of `src/utils/stripAnsi.ts` + the backtick strip — and `StreamLines`, the **only** line assembler. Changing `stripAnsi.ts` requires changing `clean_line`, or the two sides match on different text |
| `src-tauri/src/lib.rs` | Register new Tauri commands in `invoke_handler` |
| `docs/backend/command-threading.md` | Where a new command runs (`fn` = macOS main thread). Update the audit when a command changes placement |
| `docs/plugins.md` | Plugin developer guide (API reference, capabilities table, **Panel CSS Design Strategy** section, examples) |
| `src-tauri/src/mcp_http/plugin_docs.rs` | AI-optimized plugin reference (`PLUGIN_DOCS` const — **must stay in sync with `docs/plugins.md`**) |
| `docs/api/tauri-commands.md` | Tauri commands reference table |
| `docs/api/http-api.md` | HTTP API reference (if new HTTP endpoints) |
| `docs/backend/mcp-http.md` | MCP/HTTP server docs (if new routes) |
| `docs/FEATURES.md` | Section 17.1 capabilities list |
| `docs/user-guide/plugins.md` | User installation/management guide |

### Terminal & PTY
When modifying PTY behavior, output parsing, shell state, or terminal UI:

| File | What to update |
|------|----------------|
| `docs/backend/pty.md` | PTY session lifecycle, reader threads, output handling |
| `docs/backend/output-parser.md` | Rate limits, structured events, parsing rules |
| `docs/frontend/canvas-terminal-audit.md` | CanvasTerminal feature completeness audit |
| `docs/frontend/terminal-features.md` | Consolidated terminal behaviors, shortcuts, configurable features |
| `docs/FEATURES.md` | Section 1 (Terminal Management) |
| `docs/user-guide/terminals.md` | User-facing terminal features |
| `docs/api/tauri-commands.md` | PTY commands (create_pty, write_pty, resize_pty, etc.) |
| `docs/backend/alacritty-integration.md` | Alacritty patch inventory, upstream API usage, update procedure |

### Terminal Escape Sequences & Emulator Patches
When adding or changing a recognized escape sequence, terminal mode, mouse/key encoding, SGR
attribute, or emulator patch. (For OSC 7770 verbs specifically, see **Native hook
instrumentation** above — this section covers the sequence grammar, that one covers the hook
event map.)

| File | What to update |
|------|----------------|
| `tuic-escape-sequences.html` | Protocol reference (tracked) — recognized-sequence tables, "Explicitly not handled" lists, Deliberate Deviations |
| `src-tauri/patches/vte/src/ansi.rs` | `Performer::osc_dispatch` OSC arms, `PrivateMode::new` mode arms, CSI dispatch, `attrs_from_sgr_parameters` |
| `src-tauri/patches/alacritty_terminal/src/term/{mod,cell}.rs` | `Handler` impls (OSC 133, clipboard, keyboard modes), `Cell::erase_blank` |
| `src-tauri/src/terminal_grid.rs` | `TermEvent` mapping, `extract_semantic_zones`, emulator `Config` flags (e.g. `kitty_keyboard`) |
| `src-tauri/src/output_parser.rs` | Raw-stream regexes that bypass vte (`parse_osc94`, `parse_osc777_notifies`) |
| `src-tauri/src/state.rs` | `strip_kitty_sequences`, `KittyKeyboardState`; OSC 9;4 progress dispatch (`ProgressKind`/`ProgressInfo`, the `"progress"` event arm) |
| `src/stores/terminals.ts`, `src/components/Terminal/Terminal.tsx`, `src/components/TabBar/{TabViews.tsx,TabBar.module.css}`, `src/mobile/{useSessions.ts,components/SessionCard.tsx,screens/SessionDetailScreen.tsx}` (+ their `.module.css`) | OSC 9;4 progress bar rendering — kind (normal/error/indeterminate/warning) to color/animation mapping must stay identical on desktop and mobile |
| `src-tauri/src/input_line_buffer.rs` | Input-side key decoding and private-marker report filtering |
| `src/components/Terminal/canvasTerminalUtils.ts` | Mouse report encoding (`sgrMotionButton`, `motionReportButton`) |
| `src/components/Terminal/canvasTerminalWheel.ts` | Wheel notch quantization |
| `src/components/Terminal/canvasTerminalLinks.ts` | Link-gesture suppression of mouse reports |
| `docs/backend/alacritty-integration.md` | Patch inventory and the canonical OSC 7770 verb table |

### Keyboard Shortcuts & Actions
When adding or changing shortcuts:

| File | What to update |
|------|----------------|
| `src/keybindingDefaults.ts` | ACTION_NAMES + default key combo |
| `src/actions/actionRegistry.ts` | ACTION_META (label, category) — auto-populates Settings and Command Palette |
| `src-tauri/src/native_keys.rs` | macOS `NSEvent` monitor for keys WKWebView never forwards (Ctrl+Tab, F13–F20). **Keep it as ONE `KeyDown` monitor** — a second one doubles per-keystroke work on every key typed |
| `src/hooks/useNativeKeyCombo.ts` | Turns `native-key-down` back into a combo string identical to `keyEventToCombo`'s; used by every recorder |
| `docs/FEATURES.md` | Section 15 (Keyboard Shortcut Reference) |
| `docs/user-guide/keyboard-shortcuts.md` | User-facing shortcut table |
| `docs/frontend/hooks.md` | `useNativeKeyCombo` entry |

### Tauri Commands & IPC
When adding or changing Tauri commands:

| File | What to update |
|------|----------------|
| `src-tauri/src/lib.rs` | `invoke_handler!` macro registration |
| `src/transport.ts` / `src/transportExtended.ts` | `COMMAND_TABLE` entry (in `transport.ts` if any mobile-reachable code calls it, else `transportExtended.ts` — see `docs/frontend/transport.md`), or `INTENTIONALLY_UNMAPPED` if host-only |
| `src-tauri/src/mcp_http/command_table_paths.txt` | **Generated** — regenerate with `pnpm vitest run src/__tests__/transport.test.ts -u`. The Rust route probe reads it; never hand-edit |
| `docs/api/tauri-commands.md` | Command signature + description |
| `docs/api/http-api.md` | HTTP endpoint mapping (if browser/remote mode) |
| Domain backend doc | e.g. `docs/backend/pty.md`, `docs/backend/git.md` |

#### Tauri events emitted by backend
When adding a new `app.emit(event_name, payload)` call, document it here and listen in `useAppInit.ts`:

| Event | Payload | Emitted from | Frontend listener |
|-------|---------|-------------|-------------------|
| `native-notification-click` | `{ kind: "terminal", id }`, `{ kind: "progress", project, ptyId? }`, `{ kind: "aichat", id }` or `{ kind: "pr", url }` | `native_notification.rs` when a macOS Notification Center alert is clicked; desktop window only | `useAppInit.ts` → `navigateFromNativeNotice` → terminal, Progress dialog, AI Chat or the PR in the browser |
| `session-standby` | `{ session_id: string, standby: bool }` | `pty.rs emit_standby_event()`, via `AppState::emit_dual` (D.3 of the desktop↔HTTP-parity plan — used to be desktop-only with no bus arm, so a browser/PWA client never saw a session get SIGSTOP-parked). Also on `event_bus` for `/events` SSE | `useAppInit.ts` → `terminalsStore.update(termId, { standby })` |
| `pty-capture-changed` | `{ enabled: bool, session_filter: string \| null }` | `pty_capture.rs`'s `set_enabled_in_config_dir()` — the single app-side mutation point the Tauri command, the HTTP route, and (transitively) a raw curl POST all funnel through — via `AppState::emit_dual`. Global (no `pty_session_id`), so bus-only + desktop `emit`, no per-session lane | `useAppInit.ts` → `ptyCaptureStore.applyStatus(payload)`, read per-tab by `TabViews.tsx`'s capture badge via `isRecording(sessionId)` |
| `session-closed` | `{ session_id: string, reason: string, agent_type: string \| null }` | `pty.rs`'s `emit_session_closed()` — the single funnel for every close path (`close_pty_core`, `kill_pty_core`, `cleanup_session`, reader-thread EOF), via `AppState::emit_dual`. `agent_type` is read from `session_states` BEFORE the emit, since the session-state accumulator's own arm removes that entry on this same event — reading after would race itself. Also on `event_bus` for `/events` SSE | `useAppInit.ts`'s listener is the sole owner of a **remote** tab's exit teardown (Terminal.tsx's own pty-exit handler explicitly skips its mutations for one — see that file's `isRemoteTab` guard): clears `shellState`/`sessionId`/`awaitingInput` unconditionally, plus `agentType`/resume-banner fields when there was an agent, fires the `agent-stopped` plugin notification in that same case, and picks the short (`AGENT_TAB_AUTOCLOSE_MS`) vs. long (`REMOTE_TAB_AUTOCLOSE_MS`) auto-close timer based on whether `agent_type` parsed. A non-remote tab's exit is owned entirely by Terminal.tsx instead. |
| `themes-changed` | `{}` — payload-free | `themes.rs start_theme_watcher()`'s 500ms-debounced directory watcher, via `AppState::emit_dual` (D.6 — used to be desktop-only). Also on `event_bus` for `/events` SSE | `src/themes.ts` re-fetches `GET /config/themes` |
| `pty-clipboard-store` | `{ session_id: string, text: string }` | `pty.rs`'s `TermEvent::ClipboardStore` (OSC 52) handler, via `AppState::emit_dual` (D.7 — converted from a suffixed desktop-only `pty-clipboard-store-{id}` event with no bus arm to the unsuffixed form, since a suffixed name can't ride SSE). Session-scoped; **never relayed off the host** — explicitly excluded in `relay_client.rs`'s `is_relayable`. Also on `event_bus` for `/events` SSE | `Terminal.tsx` → filters by `session_id`, then `handleOsc52ClipboardStore()` (gated on the `osc52Clipboard` setting) |
| `term-alias-assigned` | `{ session_id: string, alias: string }` | `state.rs record_term_alias()` — the single funnel for every alias-assigning creation path (`create_pty`, `create_pty_with_worktree`, `register_pty_session`; always after `session-created`), via `AppState::emit_dual` (C.3 — used to be desktop-only with no bus arm; `alias` was also missing from `GET /sessions`, both now fixed). Also on `event_bus` for `/events` SSE | `useAppInit.ts` → `terminalsStore.applyAlias` → tab hover tooltip / context menu |
| `design-mode-changed` | `{ repo_path: string, session_id: string, status: "armed" \| "stopped" }` | `design_mode` lifecycle, dual-emitted to the desktop window and `event_bus` → `/events` SSE when inspection starts, rebinds or stops | Design Mode status listener → bound agent tab indicator; reads the repository and session from the event, not the currently focused tab |
| `worktree-created` | `state.rs WorktreeCreatedPayload` — `{ repo_path: string, workspace_id: string, branch: string, worktree_path: string, kind: "worktree" }`. `workspace_id` identifies the linked-worktree row; `branch` remains display and Git-ref data. | `state.rs notify_worktree_created()` — the only producer; called by `session.rs` (`/sessions/worktree`) and `worktree_routes.rs create_worktree_shared` (HTTP route + MCP `repo worktree_create`). Dual-emits: desktop `emit` plus `event_bus` → `/events` SSE, both serializing the same struct. | `useWorktreeSwitchPrompt.ts` → `setWorkspace(repo, workspace_id, { branchName, worktreePath, kind, parentRepoPath })` + Worktrees bell item with an explicit switch action; no toast |
| `worktree-removed` | `state.rs WorktreeRemovedPayload` — `{ repo_path: string, workspace_id: string, branch: string }`. Addressed by the stable workspace id; `branch` is display data captured before the checkout goes away. | `state.rs notify_worktree_removed()` — called by every removal path: `worktree.rs` (`remove_worktree`, `finalize_merged_worktree`, `merge_and_archive_worktree`, `delete_local_branch` — all addressed by `workspace_id`), `worktree_routes.rs` (`remove_worktree_http`, `finalize_merged_worktree_http`), `mcp_transport.rs` (`repo worktree_remove`) | `useWorktreeSwitchPrompt.ts` → `pruneRemovedWorktree()` closes the workspace's terminals and drops the sidebar row |
| `session-accent-color-changed` | `{ session_id: string, color: string \| null }` | `state.rs AppState::set_pty_accent_color()` — the tmux compatibility shim's `set-option ... window-style\|pane-border-style\|pane-active-border-style` dispatch (`tmux_routes.rs`, carrying Claude Code's per-teammate `--agent-color`) is the only writer today. Suppressed when the value hasn't changed, same as `session-renamed`. Also on `event_bus` for `/events` SSE | `useAppInit.ts` → `terminalsStore.applyBackendAccentColor(session_id, color)` — applied with `{ echo: false }` like `session-renamed` (an echo to `set_session_accent_color` would re-emit and ping-pong); rendered as a sidebar tab marker (`TabViews.tsx`) and a terminal pane border (`PaneTree.tsx`, `TerminalArea.tsx`) |
| `tmux-window-layout-requested` | `{ session_ids: string[], layout: string }` | `mcp_http/tmux_routes.rs request_window_layout()` — the tmux compatibility shim's `select-layout tiled`/`main-vertical` dispatch. `session_ids` is the window's **materialized** panes only, in pane order (a still-virtual pane is omitted, not a gap). Also on `event_bus` for `/events` SSE | `useAppInit.ts` → `arrangeSwarmLayout(termIds, layout)` (`utils/arrangeTmuxLayout.ts`) — arranges those sessions into a real split view in the layout of the repo+branch that **owns** them: the live `paneLayoutStore` when that branch is on screen (or the owner can't be resolved, or the manual Global Workspace is showing), otherwise that branch's entry in `savedPaneLayouts`, which `resolvePaneLayoutForBranch` restores when the user opens it. Each session stays its own independent sidebar tab |
| `repositories-changed` | `{}` — payload-free on purpose. One backend serves the desktop WebView, the browser and the PWA, and each keeps its own compare-and-swap baseline; the receiver only needs "disk moved, re-read it". Shipping the document would copy the whole repository set to every client on every save, including the one that just wrote it. | `state.rs notify_repositories_changed()` — called by both save paths, `config.rs save_repositories` (IPC) and `config_routes.rs put_repositories` (HTTP), and **only when `save_repositories_request` returns `Ok(true)`**, i.e. the delta actually moved the document. Also on `event_bus` for `/events` SSE | `repositories.ts` `startRemoteSync()` (registered by `hydrate`) → re-reads `load_repositories` → `adoptRemoteRepositories()`, which moves the store **and** the persisted baseline together for every key this client has no unsaved intent for. `activeRepoPath` is never adopted — focus is per-window |
| `repo-changed` (git-state) | `{ repo_path: string, kind: "git-state" }` | `repo_watcher.rs` — **only when the git-state fingerprint changed** (index size + resolved HEAD + porcelain status + the sorted `.git/worktrees/*` set; skips no-op `.git` touches). The worktree set is an input because add/remove touches nothing else, so worktree-only changes used to be swallowed and left ghost sidebar rows. Last fingerprint in `AppState.repo_git_fingerprints`. | `useAppInit.ts` → coalesced one bump/repo/frame via `revisionCoalescer` → `repositoriesStore.bumpGitRevision`, which bumps **both** the general and the git revision |
| `repo-changed` (working-tree) | `{ repo_path: string, kind: "working-tree" }` | `repo_watcher.rs` — non-`.git`, non-ignored file changes, debounced 1.5s when the repo is hot (has ≥1 open terminal, `set_hot_repos`) and 15s when cold. Ignore coverage is the **full git set**: the global `core.excludesFile`, the root `.gitignore` plus `.git/info/exclude`, and nested `.gitignore` files. `ALWAYS_EXCLUDED_DIRS` matches on **any path component**, so a nested git repo's `.git/` is treated as noise rather than a working-tree change. No fingerprint guard, but a firing git-state emit **cancels the pending working-tree emit** as a duplicate. Covers the main checkout **and every linked worktree** (`sync_worktree_watches`), which is what keeps a branch's sidebar diff badge live while an agent works in its worktree; the payload always names the parent repo. | `useAppInit.ts` → `revisionCoalescer` → `bumpRevision` (general revision **only**) + debounced `refreshAllBranchStats` |
| `head-changed` | `{ repo_path: string, branch: string }` | `repo_watcher.rs` — **only when the resolved HEAD target changed** (`resolve_head_target`); skips the Linux inotify storm where `.git/HEAD` events recur without HEAD moving (issue #82). Last target in `AppState.repo_head_targets`; suppressed-emit count in `AppState.repo_head_emits_suppressed`. | `useAppInit.ts` → branch rename/activate (also dedupes on `activeBranch === branch`) |
| `worktree-warm-started` | `state::worktree_warm_started_payload` — `{ repoPath: string, branch: string, worktreePath: string, total: number }` | `worktree.rs warm_with_events()`, the CoW-warm step of `spawn_worktree_setup_chain()` (first in the chain, on all three creation paths). **Fires only when something is copied**: never when `warm_ignored_directories` resolves `false` (`config::resolve_effective_warm_setting`, read inside the chain) or every candidate was skipped. `total` is the count after skip rules. Desktop emit and the `/events` SSE arm share the builder. | `useAppInit.ts` → `setWorkspace(repo, <row whose worktreePath matches>, { warmState: { status: "warming", copied: 0, total } })` — an existing row only, matched by path, never by branch; `RepoSection.tsx` renders the "Warming…" badge. Until the first such event for a path in this page session, `createRepositoryRefreshCoordinator.ts` seeds/clears `warmState` from the worktree list's `warm_artifacts` (`hooks/git/warmStateSeed.ts`; `updateWarmState` calls `noteWarmEvent`) — what shows the badge after a reload |
| `worktree-warm-progress` | `state::worktree_warm_progress_payload` — `{ repoPath, branch, worktreePath, copied: number, total: number, current: string \| null }` | Same producer, throttled ~150 ms (the final tick always sent). Copies run up to `tuic_git::cow::WARM_COPY_CONCURRENCY` (4) at a time, so `current` is the directory that finished most recently, not candidate order. | `useAppInit.ts` → same `warmState` (`copied`/`total`/`current`), shown in the badge tooltip |
| `worktree-warm-completed` | `state::worktree_warm_completed_payload` — `{ repoPath, branch, worktreePath, warmed: number, warnings: string[] }` | Same producer, once every copy finished (failures are non-fatal `warnings`). Only after a `worktree-warm-started`. `warm_artifacts.status` still reads `pending` until the chain's last step. | `useAppInit.ts` → `{ warmState: null }` clears the badge; `RepoSection.tsx` also clears it after a 900 s safety timeout without events |
| `worktree-sync-started` | `{ repoPath: string, branch: string }` | `worktree.rs run_worktree_file_sync()`, awaited by `worktree.rs spawn_worktree_setup_chain()` strictly between the chain's CoW warm (it never starts until the warm finished writing) and the setup script — the chain is spawned right after a worktree is actually created on **all three** creation paths: desktop `create_worktree`, `worktree_routes.rs create_worktree_shared` (HTTP `POST /worktrees` and MCP `repo worktree_create`), and `mcp_http/session.rs create_session_with_worktree` (previously a gap — this path never called the sync at all). Resolves the repo's effective copy settings itself via `config::resolve_effective_copy_settings` (three-tier: per-repo > `.tuic.json` > global default for the two toggles; `copy_paths` is repo-specific only) — **fires only when there is something to copy**; a repo with both toggles off and an empty `copy_paths` never emits any of the three `worktree-sync-*` events. | `useAppInit.ts` → toast "Syncing files into {branch}…" |
| `worktree-sync-progress` | `{ repoPath: string, branch: string, copied: number, total: number }` | Same producer, throttled to ~once every 150ms (plus always on the final entry) by `worktree.rs`'s `run_worktree_file_sync` — a large ignored tree (e.g. `node_modules`) can be thousands of entries. | Not consumed by the frontend today (no progress-bar UI yet) — emitted for future use and exercised by `sse_routes.rs`'s parity test. |
| `worktree-sync-completed` | `{ repoPath: string, branch: string, copied: number, total: number, errors: string[] }` | Same producer, once the background copy (`worktree_sync.rs::sync_paths`) finishes. `errors` is non-fatal per-path detail (e.g. an optional file that doesn't exist), not a failure signal on its own. | `useAppInit.ts` → toast "Finished syncing {branch}" naming how many were skipped, if any |
| `worktree-setup-script-completed` | `{ repoPath: string, branch: string, worktreePath: string, outcome: "completed" \| "not_configured" \| "stopped", exitCode: number \| null, error: string \| null }` | `worktree.rs spawn_worktree_setup_chain()`, once the setup script (if configured) finishes — always *after* the chain's CoW warm (on paths that warm) and the file sync above, on all three creation paths. Desktop emit and the `/events` SSE arm share one builder, `state::worktree_setup_script_completed_payload`. `exitCode` is `null` only for a spawn/task-panic failure (see `error` in that case); a non-zero exit still reports via `exitCode`, not `error`. **Fires exactly once per chain, whatever its end** (`outcome`): `completed` after a configured script ran, `not_configured` when none is configured, `stopped` when a removal or an abort ended the chain first (`error` says which) — unless a newer chain for the same `(repoPath, branch)` replaced it. None of the three creation paths return `setup_script`/`setup_script_error` synchronously in their response anymore; this event is the only way the outcome is reported (an MCP client has no way to observe it today — see `worktree.rs`'s doc comment on `spawn_worktree_setup_chain`). | `useAppInit.ts` → `handleWorktreeSetupScriptCompleted` status "Setup script failed (exit N)" / "Setup script failed: {error}" on a failed `completed`, silent on success, `not_configured` and `stopped`; `createWorktreeCreationCoordinator.ts` `armSetupScriptWaiter` (subscribed before the create request) holds back the new tab's Run Script until it arrives |
| `conflict-assist-status` | `{ repo_path: string, payload: { pr_number, status, conflicted_files } }` | `conflict_assist.rs` `emit_conflict_assist_status()` lifecycle; also sent on `event_bus` for `/events` SSE. Only the window emit is gated on `feature = "desktop"` — the bus send runs on every build, because `POST /repo/conflict-assist` is mounted unconditionally (#808-84e1) | `githubOpsStore` listener updates conflict-assist state; remote window forwarding accepts only the registered repository owner (#1443-e2fd) |
| `review-progress` | `{ repo_path: string, payload: { pr_number, done, findings_count?, error? } }` | `pr_review.rs` `emit_review_progress()` — once with `done: false` when the turn starts, once with `done: true` carrying either `findings_count` or ego's `error`; also sent on `event_bus` for `/events` SSE, on every build — only the window emit is desktop-gated (#808-84e1) | `githubOpsStore` listener → Review findings column; remote notices are gated by repository owner. `pr_number` must be a JSON **number**: `Number(null)` is `0`, so a missing one would file as PR #0 |
| `proposals-ready` | `{ repo_path: string, payload: { focus, proposals[] } }` | `improvement_scan.rs` `emit_proposals_ready()` just before the command returns; also sent on `event_bus` for `/events` SSE, on every build — only the window emit is desktop-gated (#808-84e1). It, not the return value, is the path that reaches every window and every transport | `githubOpsStore` listener accepts the repository owner and replaces the repo's proposals and clears the running flag |
| `ctrl-tab` | `"next"` \| `"prev"` | `native_keys.rs` — macOS only; the `NSEvent` is swallowed so AppKit cannot also cycle tabs | `useNativeMenuBridge.ts` → tab switch |
| `native-key-down` | `{ key: "F13".."F20", cmd, ctrl, alt, shift }` | `native_keys.rs` — macOS only, scoped to the `main` window; the event is passed through (nothing native to suppress) | `useNativeKeyCombo.ts`, attached only while a shortcut recorder is open |
| `mcp-toast` | `{ title, message, level, sound, origin_repo_path?, origin_session_id?, __tuic_origin?: { connection, name } }` | `mcp_transport.rs` — `ui action=toast`; derives origin from the calling MCP session rather than accepting caller-supplied scope. `origin_session_id` is the caller's TUIC session, absent for an unbound caller | `useAppInit.ts` → repository-scoped Messages bell item only, even when transient mirroring is disabled; consecutive identical notices from the same origin and connection id within five seconds deduplicate both the item and requested sound; mirrored titles/messages must be string/string-or-null or the notice is discarded with a content-free debug log. The explicit Open terminal action resolves `origin_session_id` at click time. `remote_mirror.rs` forwards connected remote toasts on both transports, adds the locally saved connection name to the origin marker, and resolves the remote peer to its PTY UUID. Mirrored notices are labelled with that name and navigate only within the owning connection; disconnected frames are dropped |
| `progress-recorded` | `{ repo_path, payload: { entry: { id, project, ptyId?, type, text, ... } } }` | `mcp_transport.rs emit_progress_entry()` after the SQLite transaction commits; dual-emitted to the desktop window and `event_bus` → `/events` SSE | `progressStore` refreshes the selected PTY or aggregate and routes the toast to that source |
| `workflow-run-changed` | `{ repo_path, payload: { runId, sequence } }` | `workflows/run/api.rs emit_run_changed()` after operator mutations (also wakes the daemon actor), daemon transitions and deadline expiry, MCP agent launch/report, and bound PTY exit; dual-emitted to the desktop window and `event_bus` → `/events` SSE. It is a cursor wake hint, not a replacement for event replay | `useAppInit.ts` → `workflowRunSignals` keeps the highest sequence per project/run; reconnect increments a resync revision so run views can replay from their cursor |
| `mcp-confirm` | `{ request_id, title, message, origin_repo_path?, origin_session_id? }` | `mcp_transport.rs` — `ui action=confirm`, dual-emitted, plus a mobile push. It used to be a native OS dialog, which no remote human could answer; every client now gets the request and the first answer wins. Also raised by `mcp_http/mod.rs`'s `confirm_open_url` for an OSC 1337 `OpenURL` request (no mobile push; same `confirm_responses` map and `mcp_confirm_response` answer path) | `McpConfirmHost.tsx` (mounted by BOTH `ApplicationOverlays.tsx` and `MobileApp.tsx`) → `stores/mcpConfirm.ts` queue → `ConfirmDialog`; remote confirmations retain the origin connection, show its locally saved host name, route answers to that daemon, and clear on disconnect. Resolution is scoped by connection and request id |
| `mcp-confirm-resolved` | `{ request_id, confirmed }` | `mcp_http/mod.rs` `resolve_mcp_confirm()` on an answer, and `mcp_transport.rs` when the 300 s wait expires | `McpConfirmHost.tsx` → drops that request from the queue, so the clients that lost the race take the dialog down |
| `agent-wrap-prompt` | `{ request_id: string, agent_type: "claude" \| "codex" \| "goose" }` | `agent_wrap_prompt.rs request()` — raised by `pty.rs`'s OSC 7770 `userwrap` arm (only at the shell prompt, payload bound to the PTY's own session); one pending prompt per agent, 10 min TTL; `AppState::emit_dual` | `stores/agentWrapPrompt.ts` → `AgentWrapPromptHost` (desktop `ApplicationOverlays`, mobile `MobileApp`) |
| `agent-wrap-prompt-resolved` | `{ request_id: string, agent_type: string, decision: boolean \| null }` | `agent_wrap_prompt.rs resolve()` after an answer (IPC `agent_wrap_prompt_response` or `POST /agent-wrap-prompt/response`); `AppState::emit_dual` | `stores/agentWrapPrompt.ts` → drops that agent's dialog on every client |
| `pty-description-changed` | `{ session_id: string, description: string | null }` | `state.rs` — MCP `agent spawn` / `session submit` / `session input` updates the orchestrator-owned PTY description | `useAppInit.ts` → `terminalsStore.ptyDescription` → Context bar |
| `session-renamed` | `{ session_id: string, name: string \| null, is_custom: boolean }` | `state.rs` `rename_session_from_backend` (MCP `session rename`, the tmux shim's `select-pane -T`), `clear_session_name_from_backend` (`select-pane -T ""`, `name: null`), `rename_session_from_frontend` (IPC/HTTP `set_session_name`, once per real change, so other windows see a rename live; `name: null` = cleared) and `osc_title.rs` OSC 0/2 title sync, all via `emit_dual` | `useAppInit.ts` → `terminalsStore.applyBackendRename(session_id, name, is_custom)` (null name keeps the label, applies `is_custom` only) → tab bar and sidebar — applied with `{ echo: false }`, never a plain `update()`, which would echo the name back to `set_session_name` |
| `session-suspend-requested` | `{ session_id: string, request_id: string }` | `state.rs` `request_session_suspend` — MCP `session action=suspend` after the busy check | `useAppInit.ts` → `suspendTerminal` → closes the PTY, tab stays with `suspended`; answers via `session_suspend_response` |
| `pty-activity-{session_id}` | `{ session_id: string }` | `pty.rs emit_pty_activity()` via `ActivityPulse`, throttled to one pulse per `ACTIVITY_PULSE_WINDOW` (1 s). Payload-free and idempotent, so dropping pulses inside the window loses nothing — do NOT convert this throttle into a coalescer. Dual-emitted on `event_bus` as `PtyActivity` (`activity` WS frame on `/sessions/:id/stream`, plus `pty-activity` SSE); deliberately NOT forwarded on the `?format=grid` WS, which has no activity consumer. Ignored by `apply_event_to_session_state` — it must not restamp `SessionState.last_activity_ms`, which answers a different question | `Terminal.tsx` → `subscribePty(…, { onActivity })` → `terminalsStore.touchLastDataAt` + background-tab `activity` flag. **Not** in `useAppInit.ts` — per-session |
| `pty-osc133-{session_id}` | `{ marker: string, line: number, exit_code: number \| null, on_alt_screen: bool }` | `pty.rs` OSC 133 handler — serialised from `terminal_grid.rs Osc133Event`, so the field name is `exit_code`, NOT `exitCode` (same for `on_alt_screen`/`onAltScreen`). `on_alt_screen` is true when `line` was computed while the alternate screen buffer was active (a fullscreen TUI, e.g. Claude Code's default renderer, never grows real alt-screen history, so `line` there is a transient on-screen cursor row, not a valid scrollback anchor — see `CommandBlock.onAltScreen`, `terminals.ts`). Dual-emitted on `event_bus` as `PtyOsc133` (`osc133` frame on the `?format=grid` WS via `grid_ws_frame()`, plus `pty-osc133` SSE); the grid WS is the right lane because `CanvasTerminal` is the only consumer and it already holds that socket. Ignored by `apply_event_to_session_state` | `CanvasTerminal.tsx` → `transport.onEvent("osc133", …)` → `terminalsStore.handleOsc133()` → command blocks, gutter marks, Cmd+Up/Down — every row-anchored consumer must read blocks through `rowAnchoredBlocks()` (`terminals.ts`), which drops `onAltScreen: true` entries. **Not** in `useAppInit.ts` — per-session |
| `pty-cwd-{session_id}` | `{ cwd: string }` | `pty.rs` OSC 7 handler. The desktop payload is the `{ cwd }` object, not a bare string — both transports carry the same shape so the handler needs no branch. Dual-emitted on `event_bus` as `PtyCwd` (`cwd` frame on the `?format=grid` WS, plus `pty-cwd` SSE). Ignored by `apply_event_to_session_state` | `CanvasTerminal.tsx` → `transport.onEvent("cwd", …)` → `terminalsStore.update({ cwd })` + `onCwdChange`. **Not** in `useAppInit.ts` — per-session |
| `pty-open-url` | `{ session_id: string, url: string }` | `pty.rs confirm_and_notify_open_url()` — fires only after a human confirms the `mcp-confirm` request raised for an OSC 1337 `OpenURL` sequence; a declined or timed-out request never reaches here. Global (not session-scoped) — dual-emitted on `event_bus` as `PtyOpenUrl`, plus the desktop window event of the same name | `stores/ptyOpenUrl.ts` (mounted globally via `PtyOpenUrlHost`, in BOTH `ApplicationOverlays.tsx` and `MobileApp.tsx`) → `utils/openUrl.ts handleOpenUrl()` (allowlists http/https/mailto) |
| `pty-watcher-lines-{session_id}` | `{ session_id: string, lines: [{ text: string, matched_ids: string[] }] }` | `pty.rs emit_watcher_lines()` — one emit per 100 ms batch of assembled lines; `text` is the CLEANED text Rust matched on, `matched_ids` are qualified `client_id/watcher_id`. Rust ships every line only while a registered pattern could not be compiled, otherwise the matched ones alone. Dual-emitted on `event_bus` as `PluginWatcherLines` (`watcher-lines` WS frame on `/sessions/:id/stream` in both `?format=grid` and raw mode — **not** `?format=log|text`, which returns before the event loop — plus `plugin-watcher-lines` SSE) | `CanvasTerminal.tsx` → `transport.onEvent("watcher-lines", …)` → `pluginRegistry.handleWatcherLines()`, which re-runs the JS `RegExp` on each line. The listener is installed BEFORE the grid subscription — a line that lands while it is being attached is lost. **Not** in `useAppInit.ts` — the listener is per-session |
| `session-state-changed` | `{ session_id: string, state: SessionState }` — `state` is exactly the object `list_active_sessions` returns per session, snake_case, with serde skipping the zero-valued fields. Both transports build it from `state.rs session_state_payload()`, one function on purpose | `state.rs publish_session_state_change()`, called only from the session-state accumulator — the sole writer of `session_states` and therefore the only place that can see a transition. Deduped by `SessionState`'s `PartialEq` (which excludes `last_activity_ms`), so a repaint that changes nothing a client renders emits nothing. Dual-emitted on `event_bus` as `SessionStateChanged` (`session-state-changed` SSE on `/events`). Ignored by `apply_event_to_session_state` — it is the accumulator's output, never its input | `useAgentPolling.ts` → `subscribeEvents({"session-state-changed"})` → `applySessionState()` → tab awaiting/busy badges + Activity Dashboard. Replaced a 1 Hz `list_active_sessions` poll; the only remaining reads are a single mount-time catch-up, for sessions already idle and silent, and the same catch-up re-run from `subscribeEvents`' `onResync` when the SSE stream reconnects or reports `lagged` (browser only — Tauri `listen()` cannot drop) |
| `acp-notice` | `{ connectionId, generation, sessionId?, requestId?, sequence, kind }` — `kind` is `ready` \| `settled` \| `interaction_pending` \| `interaction_settled` | `card`. camelCase, unlike the PTY rows above: it is the same object the `/acp` routes and the `acp_*` commands return, and a second spelling would be a second thing to keep in sync | `state.rs spawn_acp_notice_pump()`, fed by `acp/events.rs` `AcpEventJournal::append` — the one place every ACP event is stamped, so a notice cannot be forgotten by a new producer. Dual-emitted on `event_bus` as `AcpNotice` (`acp-notice` SSE on `/events`) | TBD — no ACP frontend yet. It is a wake signal: react by reading `acp_connection_snapshot`, `acp_pending_interactions`, or the stream from the `sequence` it names. The ordered turn frames stay on `acp_subscribe` / the `/acp/connections/:id/stream` WebSocket and never ride this bus |
| `remote-connection-status` | `{ id, status, base_url?, token?, protocol_version?, build?, out_of_date?, live_sessions?, update_notice?, update_in_progress?, error?, step?, provision_offer? }` — `status` is `disconnected` \| `connecting` \| `connected` \| `unauthenticated` \| `error`. snake_case: it is the `RemoteConnectionStatus` struct as `/config/remote-connections/status` returns it, and the push is the whole client view rather than a delta, so a missed event cannot leave a client holding a route the backend has retracted. `base_url`, `token`, `live_sessions`, and `update_notice` are present only while connected | `remote_runtime.rs publish()` — dual-emitted on every real change (dedup is on the snapshot, so a poll that keeps answering 200 emits nothing) | `stores/remoteConnections.ts` `applyStatus()` → store state + the SSE bridge, which exists exactly while the connection is connected |
| `session-focus-requested` | `{ session_id: string }` | `mcp_http/session.rs focus_session_impl()` via `POST /sessions/{id}/focus` or its `focus_session` IPC twin — did not exist before the StreamDock integration; `INTENTIONALLY_UNMAPPED` in `transport.ts` used to list window/panel focus as host-only because there was nothing to call. Dual-emitted on `event_bus` as `SessionFocusRequested` (`session-focus-requested` SSE) | `useAppInit.ts` → `terminalsStore.getTerminalForSession()` then **both** `terminalsStore.setActive` and `conversationStore.setActiveTerminal` (see `watcherFire.ts`'s own `setActiveSession` for why both) |
| `ui-action-requested` | `{ name: string }` | `mcp_http/session.rs run_ui_action_impl()` via `POST /ui/action` or its IPC twin — gated by `session::UI_ACTION_ALLOWLIST` in Rust (currently `jump-waiting-terminal`, `activity-dashboard`); the event itself carries no restriction, so any future producer must apply its own allowlist. Dual-emitted on `event_bus` as `UiActionRequested` (`ui-action-requested` SSE) | `useShortcutRegistration.ts` → `dispatchAction()` (exported from `useKeyboardShortcuts.ts`) — the same dispatch table real keydown events use, so this makes every registry action controller-bindable without a second handler list |

### HTTP & MCP Server
When adding routes or changing server behavior:

| File | What to update |
|------|----------------|
| `docs/api/http-api.md` | REST endpoint reference |
| `docs/backend/mcp-http.md` | Server architecture, routing, lazy tool discovery (`collapse_tools` / meta-tools) |
| `docs/user-guide/remote-access.md` | User setup guide |
| `src-tauri/src/mcp_http/plugin_docs.rs` | PLUGIN_DOCS (if plugin-facing) |
| `src-tauri/src/mcp_http/fixtures/` | A client's exact captured handshake, when the change touches a lifecycle. `/mcp` serves two — legacy `initialize` (2025-11-25) and stateless `server/discover` (2026-07-28) — and a client that speaks only one is proven by its own request, not by a hand-written approximation |

### CLI Companion (`tuic` / `tuic`-as-`tmux`)
When changing `src-tauri/crates/tuic-cli/**` (subcommands, tmux-compat arms, IPC client) or the
app-side tmux topology it talks to (`src-tauri/src/mcp_http/tmux_routes.rs`):

| File | What to update |
|------|----------------|
| `docs/user-guide/cli.md` | Command reference — session management, tmux compatibility table, `TUIC_TMUX_LOG` |
| `docs/api/http-api.md` | The `/tmux/*` endpoints, if the topology HTTP surface changes |
| `docs/backend/mcp-http.md` | REST table, if routes change |
| `tmux-swarm-shim.md` (repo root) | Investigation notes — keep verified-against-binary claims current if Claude Code's own tmux usage is re-audited |

Previously this repo had no row for the CLI at all — `docs/user-guide/cli.md` is reachable only
through `docs/SUMMARY.md`, which is exactly how its tmux-compatibility table drifted from the
actual `tmux_compat()` implementation unnoticed for a full feature cycle.

### Self-Signed HTTPS & Network Reachability (mDNS/Bonjour, custom hostname)
When modifying `selfsigned.rs`'s SAN/cache-coverage logic, `get_local_ips`'s entry list, or
`get_connect_url`/`resolve_connect_target`'s scheme/host resolution:

| File | What to update |
|------|----------------|
| `src-tauri/src/selfsigned.rs` | SAN computation (`generate_and_cache`), cache-coverage check (`load_cached`) — every new SAN source needs a matching coverage check or the cache goes stale silently |
| `src-tauri/src/lib.rs` | `get_local_ips_with_config`/`append_mdns_entry` (network-picker entries), `current_lan_ips`, the 3 `ensure_self_signed_cert` call sites |
| `docs/FEATURES.md` | Section 14.11 (Self-Signed HTTPS Fallback) |
| `docs/user-guide/remote-access.md` | HTTPS section — sources of SAN coverage, how to connect by each entry type |
| `docs/api/tauri-commands.md` | `get_local_ips` entry shape/description |
| `docs/api/http-api.md` | `GET /system/local-ips` description |
| `to-test.md` | Any new SAN source needs a manual cert-regeneration/connectivity check — this class of change can't be exercised by `cargo test` alone (real network/OS state) |
| `plans/network-aware-remote-access.md` (main checkout only, gitignored) | Living plan for trusted-network gating / mDNS Tier B / custom hostname — update phase status as each ships |

### Diagnostics
When modifying `cpu_watchdog.rs` or the `/diagnostics` HTTP endpoint:

| File | What to update |
|------|----------------|
| `src-tauri/src/cpu_watchdog.rs` | Watchdog logic, thresholds, snapshot fields |
| `src-tauri/src/mcp_http/log_routes.rs` | `/diagnostics` GET/POST handlers |
| `src-tauri/AGENTS.md` | Diagnostics section (usage, known failure patterns) |
| `docs/FEATURES.md` | Section 20.11 (Runtime Diagnostics) |

### Agent state detection (working / idle / awaiting)
When changing an awaiting/idle/busy signal — a parser, the hook suppression, or the raw-stream composition:

| File | What to update |
|------|----------------|
| `src-tauri/crates/tuic-terminal/src/output_parser.rs` | The parser itself (`parse_question`, `parse_osc777_notify`, …) |
| `src-tauri/crates/tuic-terminal/src/chrome.rs` | Bottom-zone cutoff — anything at or below the input box must stay unparsed |
| `src-tauri/src/pty.rs` | `raw_stream_events` composition + `suppress_heuristic_question` gating |
| `src-tauri/src/state.rs` | `apply_event_to_session_state` — the arms that SET and CLEAR `awaiting_input`. A signal nothing retracts latches the badge |
| `src/components/Terminal/Terminal.tsx` | The frontend twin of those arms (`terminalsStore` awaiting flags) |
| `src-tauri/src/fixtures/agent_prompts/` | A framed `.tcap` capture of the failure, recorded via `/diagnostics/capture` (`.raw` remains legacy-readable) |
| `src-tauri/src/pty/tests.rs` | A case in the `Awaiting-signal fixtures` block replaying that capture |
| `src-tauri/src/pty.rs` tests | A case in the `Awaiting RETRACTION` block when the failure is a state that never clears — fixtures assert emitted events and cannot express a MISSING one |
| `scripts/hooks/pre-commit` | The fixture gate's symbol/file lists — a new detection symbol or a new detection-carrying file must be added, or the gate silently stops covering it |

| `src-tauri/AGENTS.md` | "Agent state detection" section (signal table, capture workflow, retraction) |
| `agent-signal-architecture.html` | The three-origin pipeline diagram, OSC 777-vs-7770 status, and the per-agent signal matrix — update if a signal's origin, confidence, or an agent's coverage changes |

The fixture gate compares the indexed and committed production scopes, excluding
`#[cfg(test)]` modules and `#[test]`/`#[tokio::test]` functions. A function or
module that is a test item in the index and appears attribute-less but otherwise
byte-identical in HEAD (a test that was missing its `#[test]`) is excluded from
both views, so adding the attribute is test-only; editing its body in the same
commit, or keeping the attribute-less copy, exempts nothing. A formatting-only
change to production code (whitespace, line breaks, a trailing comma rustfmt adds
or removes; never a 1-tuple's comma, never whitespace inside a literal) is not a
detection change. Test-only
changes may reuse existing captures. Production detection changes require a staged,
framed TCAP v2 capture with nonzero geometry, complete records and a matching
provenance Markdown file (capture source and SHA-256). New raw/text external
fixtures are rejected; legacy captures remain readable. Framing and provenance
checks do not authenticate where bytes came from: the author must record them
from the real system.


### Native hook instrumentation (OSC 7770, `tuic-hook`)
When changing an agent's hook event map, adding/removing an OSC 7770 verb, or touching the
`tuic-hook` binary that emits them:

| File | What to update |
|------|----------------|
| `src-tauri/src/agent_hook.rs` | Per-agent `HookSpec` tables (`CLAUDE_HOOKS`, `GEMINI_HOOKS`, …: argv + the `wire` contract, pinned by `wire_contract_is_the_event_table` and the golden `every_spec_emits_exactly_its_wire_contract`), the `hook_binary_command` generator, and the non-desktop `shell_hook_command` flavour |
| `src-tauri/crates/tuic-hook/src/main.rs` | argv parsing, stdin JSON extraction, the `DERIVATIONS` event→behavior table, the `toolfail`-before-`state` ordering guarantee |
| `src-tauri/crates/tuic-hook/src/tty.rs` | Native controlling-tty resolution (Linux `/proc` fd readlink, macOS `proc_pidinfo`) |
| `src-tauri/crates/tuic-hook/src/payload.rs` | Percent-encoding scheme for free-text verb payloads |
| `src-tauri/src/agent_hook_installer.rs` | Settings-file merge/prune — sentinel-keyed, agnostic to command content; should rarely need changes |
| `src-tauri/src/agent_hook_launch.rs` | Launch-scoped `agent-hooks/claude.json` (built from `claude_hook_map`, regenerated at every boot) — no change usually needed, but its tests pin the submit-prompt command |
| `src-tauri/src/agent_hook_commands.rs` | Per-agent settings paths, `apply_at`/`state_at`, `reinstall_outdated_hooks` startup migration |
| `src-tauri/src/hook_binary.rs` | Stable-copy location and version-drift refresh (`ensure_current`) |
| `src-tauri/src/pty.rs` | The `TermEvent::Tuic` verb switch — decode a new verb here, mirroring `percent_decode_osc_payload` for free text |
| `src-tauri/crates/tuic-terminal/src/output_parser.rs` | `ParsedEvent::AgentMetadata` (or a new variant, for a verb that isn't free-text metadata) |
| `docs/backend/alacritty-integration.md` | OSC 7770 verb table |
| `docs/backend/tuic-hook.md` | CLI reference, derivation table, stdin fields, env vars, exit-code contract |
| `docs/user-guide/ai-agents.md` | "Native Hook Instrumentation" user-facing section |
| `docs/FEATURES.md` | Section 1.19 (red-tick tier description) |
| `plans/docs/hook-lifecycle.html` (main checkout only, gitignored — NOT repo-root/tracked despite this table's other rows; confirmed absent from `git ls-files` 2026-09-15) | Full event→verb→feature lifecycle diagram, CLI surface, and rationale |
| `agent-signal-architecture.html` (repo root, tracked) | Per-agent signal matrix and the command-block/scrollbar-tick section — update if a hook map, block producer, or per-agent classifier changes |

### Agent wrap consent prompt (zsh `userwrap`)
When changing how TUIC asks to wrap a user's own `claude`/`codex`/`goose` zsh function:

| File | What to update |
|------|----------------|
| `src-tauri/src/shell_integration.rs` | The zsh emitter (`userwrap=<agent>:<fingerprint>:<$TUIC_SESSION>`, `cksum` fingerprint) and its quoting (`zsh_single_quote`) |
| `src-tauri/src/pty.rs` | The OSC 7770 `userwrap` arm: honoured only between OSC 133 D/A and C (at the prompt), via `agent_wrap_prompt::bound_userwrap_payload` |
| `src-tauri/src/agent_wrap_prompt.rs` | Payload validation + session binding, pending map (one per agent, `PENDING_WRAP_PROMPT_TTL`), `request`/`resolve`, persisted decision |
| `src-tauri/src/event_wire.rs` | `agent-wrap-prompt` / `agent-wrap-prompt-resolved` arms |
| `src-tauri/src/lib.rs` + `src-tauri/src/mcp_http/mod.rs` | IPC `agent_wrap_prompt_response` and its shared route `POST /agent-wrap-prompt/response` (same body, 400 for an unknown agent) |
| `src/transport.ts` + `src/__tests__/transport.test.ts` | Core `COMMAND_TABLE` entry (mobile answers it) + mapping assertion |
| `src/stores/agentWrapPrompt.ts`, `src/components/AgentWrapPromptHost/` | Queue, dialog, first-answer-wins dismissal |
| `docs/user-guide/ai-agents.md`, `docs/api/http-api.md`, `docs/backend/alacritty-integration.md` | User-facing consent text, route contract, OSC 7770 verb table |
| `AGENTS.md` | Accepted Security Decisions entry for the route |

### Session state explain (troubleshooting dump for agent state detection)
Cross-cutting: `pty.rs`'s ranked-evidence arbiter + `state.rs`'s `agent_state` ladder, all 3
transports, and the frontend modal. When changing the `SessionStateExplain` payload shape, the
decision trail, or the notification-classification record:

| File | What to update |
|------|----------------|
| `src-tauri/src/pty.rs` | `TrailKind`/`TrailEntry`/`DecisionTrail` (the always-on rejection-visible ring, a sibling field of `SilenceState::evidence` — NOT inside `TurnEvidence`, which is `Clone`d on the PTY reader's hot path); `NotificationClassification`; the `SilenceState` wrapper methods (`record_busy`/`record_idle`/`force_idle`/`clear_idle`/`clear_busy_evidence`) every evidence-recorder call site must route through so the trail can't silently miss one |
| `src-tauri/src/pty/explain.rs` | `SessionStateExplain` and its section structs (`AgentExplain`/`VisibleExplain`/`EvidenceExplain`/`ScreenExplain`/`SilenceExplain`/`NotificationExplain`/`TrailEntryExplain`), `explain_session_state_impl` — the one assembler both transports share. Never call `get_session_foreground_process_impl`/`detect_agent_screen_activity` from here (side effects); never re-derive the ladder (call `session_state_with_shell_detailed` instead) |
| `src-tauri/src/state.rs` | `resolve_agent_state` (the extracted `agent_state` ladder — the single copy `session_state_with_shell_detailed` and the explain assembler both consult) and `session_state_with_shell_detailed`'s tuple shape |
| `src-tauri/src/pty/commands.rs` + `src-tauri/src/lib.rs` | `explain_session_state` Tauri command + `invoke_handler!` registration |
| `src-tauri/src/mcp_http/session.rs` + `src-tauri/src/mcp_http/mod.rs` | `explain_state` HTTP handler + the `/sessions/{id}/explain-state` route |
| `src/transport.ts` + `src-tauri/src/mcp_http/command_table_paths.txt` | `explain_session_state` `COMMAND_TABLE` entry; regenerate the snapshot with `pnpm vitest run src/__tests__/transport.test.ts -u`, never hand-edit |
| `src-tauri/src/mcp_http/mcp_transport.rs` | `DEBUG_ACTIONS`, the tool schema description, `handle_debug`'s `"explain_state"` arm, **and** `handle_debug_unified`'s dispatch/help map — both, or the unified path 404s an action `handle_debug` alone supports |
| `src/components/StateExplainModal/` | `StateExplainModal.tsx` (payload rendering, the frontend-badge-disagreement banner via `effectiveActivityState`, copy-as-JSON via `writeClipboard` — never `navigator.clipboard` directly, see issue #101), `StateExplainHost.tsx` (mounted once in `ApplicationOverlays.tsx`, same shape as `PtyOpenUrlHost`/`McpConfirmHost`) |
| `src/stores/stateExplain.ts` | The shared open/close signal both triggers (Activity Dashboard row button, `TabBar.tsx`'s tab context menu) call into |
| `src/i18n/en.json` | Any new `tabBar.*`/UI label — `i18nKeyCollisions.test.ts` fails the build otherwise |
| `docs/backend/pty.md` | "Session state explain" section — the four-layer chain (evidence → `SilenceState` bookkeeping → the ladder rung → the frontend badge) and the full payload shape |
| `docs/api/tauri-commands.md` / `docs/api/http-api.md` | `explain_session_state` entry, beside `debug_agent_detection`/`get_session_foreground_process` |
| `docs/backend/mcp-http.md` | Debug-actions table row (`explain_state`) |
| `src-tauri/AGENTS.md` | "Agent state detection" section — how to read a dump, alongside the existing signal table/capture workflow |
| `agent-signal-architecture.html` | Its "Investigation Playbook" — the new surface belongs there |

### MCP Tool Surface (native tools, upstream proxy, meta-tools)
When changing the tool list, tool handlers, `disabled_native_tools`, upstream allow/deny filters, or the Speakeasy meta-tools:

| File | What to update |
|------|----------------|
| `src-tauri/src/mcp_http/mcp_transport.rs` | Tool definitions, `merged_tool_definitions`, `searchable_tool_definitions`, meta-tool handlers (`search_tools`, `get_tool_schema`, `call_tool`), `build_mcp_instructions` |
| `src-tauri/src/mcp_proxy/registry.rs` | `aggregated_tools`, `proxy_tool_call` (filter is enforced on BOTH — discovery no longer gates dispatch under `collapse_tools`) |
| `src-tauri/src/tool_search.rs` | BM25 `ToolSearchIndex` backing `search_tools` / `get_tool_schema` |
| `docs/backend/mcp-http.md` | Lazy Tool Discovery section, meta-tool table, filter-enforcement note |
| `src-tauri/src/mcp_http/plugin_docs.rs` | AI plugin guide references to MCP worktree inputs |
| `src-tauri/crates/tuic-cli/` | CLI callers of renamed or removed native actions |
| `docs/user-guide/cli.md`, `CHANGELOG.md` | Public CLI examples and BREAKING migration instructions |
| `docs/backend/config.md` | `collapse_tools` field in `AppConfig` table |
| `docs/user-guide/settings.md` | MCP page → TUIC MCP Server — "Collapse tools" checkbox description |

#### Session tool actions added (swarm Layer 3–4)
- `session action=submit` — submits one command to a confirmed-idle managed agent and returns a bounded terminal-movement receipt in the same response. It never queues or overwrites a partial composer; `session action=input` remains raw and write-only.
- `session action=status` — returns `{shell_state, idle_since_ms, busy_duration_ms, exit_code, agent_type}`. Useful for polling agent progress without streaming output.
- `session action=list` response now includes `shell_state` per entry, plus `tuic_session` (the identity the tab persists) and `alias`. It no longer includes `child_pid` or `foreground_pgid` — no action accepts a raw pid.
- `session_id` on every action accepts the PTY id, `tuic_session`, alias, unique short PTY-id prefix, or unique display name; `to` on `agent action=send` uses the same resolver and also accepts a peer's registered `name` (the one `list_peers` prints); two peers sharing a name are refused with the candidates. Ambiguous prefixes or names return an error. Resolution lives in `AppState::resolve_session_ref_checked` / `resolve_peer_ref_checked`; an unresolvable reference falls through unchanged so a tombstoned session can still be read.
- `session action=create` (and `POST /sessions`) accept an optional `alias`, which the frontend replays from persisted tab state so an alias survives a restart.

#### Agent tool actions added (swarm inbox)
- `agent action=inbox` response now includes `missed_count` — number of unread messages evicted at capacity since last read. Same-child, same-kind lifecycle notice replacement does not increase it. Non-zero means the orchestrator missed messages and should increase polling frequency.
- Reading `agent action=inbox` settles delivery for the returned messages, including queued terminal wakes. A queued generic wake is removed once no pending terminal-owned mail remains, so read mail can be reclaimed at the 100-message inbox limit.
- `agent action=send` response includes **`delivered`** (bool) plus, when false, `warning` and `recipient_has_terminal`. `delivered` is false exactly when `delivery_path == "inbox_only"`: no waiter, channel, direct terminal delivery, or already-pending coalesced orchestrator wake will surface it, so it stays unread until the recipient polls. Registered orchestrators add `wake_notification_and_inbox`, `coalesced_wake_and_inbox` and `lifecycle_summary_and_inbox`; none of them exposes a peer payload — the last one is reachable only for a window made entirely of server-authored `tuic-auto-*` lifecycle notifications, which it prints inline and acknowledges itself. Canonical idle/completed state may claim the wake; derived `working` may also claim it only through the stricter composer gate (idle shell, confirmed readiness, no question, no partial input), so background work cannot strand a result while active work remains protected. Ordinary Claude recipients use that same gate before selecting their SSE channel: a ready composer held `working` by background work receives a PTY wake instead. A payload-free wake gets at most one retry after an uncertain PTY write per unread-mail group; coalesced mail does not reset that budget, and inbox/wait observation does. `delivered` plus `delivery_path` are the whole verdict: `ok`, `accepted`, `buffered_in_inbox` and `recipient_has_terminal` were removed because they answered "buffered" to a question about delivery. Keep these distinct in every client and in the tool descriptions — reporting `inbox_only` as success is how a reply to an agent with no PTY silently vanished.
- `agent action=register` response includes **`terminal`** (bool): false means the identity resolves to no live PTY (`live_pty_for_peer` → `None`), so it can never be typed into or woken, and the peer must consume its own inbox via `wait`/`inbox`. Identities without a PTY arise from a bridge that sent no `x-tuic-session` header (agent launched outside a TUIC PTY) — the server then mints an MCP-scoped UUID.
- `agent action=register` accepts **`orchestrator`** (bool) as the only role declaration seam; omission preserves the current role and child spawn never infers it. The `register` response surfaces `orchestrator` plus **`mail_wake`** (`managed_pty_lifecycle` or `none`); `list_peers` reports `orchestrator` but not `mail_wake`, which is a property of the caller's own identity rather than of every peer in a listing. External/headerless orchestrators are inbox/wait-only because MCP/SSE activity is not an authoritative idle or wake surface.

### Smart Prompts `api` mode (one unattended ego turn)
The same ego, a different shape of use (#787-ee50): no panel, no stream, no
second turn, and **no MCP server in the session** — a turn nobody watches cannot
be handed the tools that drive terminals and repositories. Changing any of the
three rules in `acp/oneshot.rs` changes what the mode is allowed to do.

| File | What to update |
|------|----------------|
| `src-tauri/src/acp/oneshot.rs` | The turn: what counts as the answer, which questions are refused, how long the turn may take. `detached` is why a dropped caller cannot skip the shutdown, and `TURN_TIMEOUT` must leave room for `INITIALIZE_TIMEOUT` inside `REQUEST_TIMEOUT` |
| `src-tauri/src/acp/manager.rs` | `new_unattended_session` / `unattended` — the emptied authority. `granted` is its attended counterpart. `INITIALIZE_TIMEOUT` bounds the launch, enforced inside the supervisor because only there can expiry stop the child |
| `src-tauri/src/acp_commands.rs`, `src-tauri/src/mcp_http/acp_routes.rs` | `acp_one_shot_prompt` and `POST /acp/one-shot`. The route takes the spawn guard: it launches a process |
| `src/transport.ts`, `src-tauri/src/mcp_http/command_table_paths.txt` | The parity entry and its regenerated snapshot |
| `src/hooks/useSmartPrompts.ts` | `canExecuteApi`, `apiRoot`, `executeApi` — and the `api` branch of `executeHeadless`, which is the same one path |
| `src/types/acp.ts` | `EgoTurn`, the mirror of the Rust type |
| `docs/api/tauri-commands.md`, `docs/api/http-api.md`, `docs/FEATURES.md`, `docs/user-guide/settings.md` | The command, the route, the feature and what a user is told when ego is not configured |

### PR review, changelog and improvement scan (one unattended ego turn each)
The three GitHub features #784-0aec deleted, back on ego (#795-320b). Each is
**one** `acp::oneshot` turn with the whole input inline — there is no multi-turn
engine, no `read_file` tool use and no per-file phase, because an unattended turn
is refused every tool it asks for. TUIC stores no API key and makes no provider
HTTP call for any of the three; which model runs is ego's configuration and this
side is never told it. When ego is not reachable each one fails with ego's own
sentence, never with an empty result.

| File | What to update |
|------|----------------|
| `src-tauri/src/acp/oneshot.rs` | `ask`, `answer_text`, `extract_json`, `unparseable` — the seam all three share. A change here changes all three |
| `src-tauri/src/pr_review.rs` | The review: diff split, prompt, envelope, and `filter_findings_by_confidence` — the gate runs before anything leaves the module. `TUIC_REVIEW_CONFIDENCE_THRESHOLD` overrides it |
| `src-tauri/src/changelog.rs` | The changelog: prompt over merged PRs, plus `split_changelog_output` — lenient about format (markdown with a null json half is a valid answer), strict about reachability |
| `src-tauri/src/improvement_scan.rs` | The scan: focus, prompt, `parse_improvement_output`, the five-proposal cap, and `create_issue_from_proposal` |
| `src-tauri/src/state.rs`, `src-tauri/src/mcp_http/sse_routes.rs` | `AppEvent::ReviewProgress` / `ProposalsReady` and their `/events` SSE arms. Producers dual-emit |
| `src-tauri/src/mcp_http/{mod,github_routes,types}.rs` | `GET /repo/changelog`, `POST /repo/pr-review`, `POST /repo/improvement-scan`, `POST /repo/create-issue-from-proposal`. Not desktop-gated: ego is reached over ACP, so `tuic-remote` serves them too |
| `src/transport.ts`, `src-tauri/src/mcp_http/command_table_paths.txt` | The four parity entries and the regenerated snapshot |
| `src/types/index.ts` | `PrReviewResult`, `ReviewedFile`, `ReviewFinding`, `FindingSeverity`, `ChangelogResult`, `ImprovementProposal`, `ImprovementScanResult`, `ImprovementFocus`, `CreatedIssue` |
| `src/stores/prReview.ts` | Per-PR review state, the flatten/id scheme and GitHub's needs-a-line posting rule |
| `src/stores/githubOps.ts` | The ops event accumulator: `review-progress` and `proposals-ready` |
| `src/components/PrDetailPopover/PrDetailContent.tsx`, `src/components/shared/SeverityIcon.tsx` | Where findings are shown, selected and posted |
| `src/components/ChangelogModal/`, `src/components/Sidebar/GitHubPanel.tsx` | The modal and the button that opens it |
| `src/components/GithubOpsDashboard/GithubOpsDashboard.tsx` | The Review findings and Proposals columns, the focus buttons and create-issue |
| `docs/api/tauri-commands.md`, `docs/api/http-api.md`, `docs/FEATURES.md`, `docs/backend/github.md` | The four commands, the four routes and the three features |

### AI Chat panel (ego over ACP)
The panel is a control plane over an agent that lives outside it (#785-58ca). It
is **one chat for the app** (#1157-1e54), never bound to a terminal or a
repository: sessions run in `~/Gits`, and the repository on screen is a hint sent
with each prompt. Nothing here holds a
provider, an API key or a tool loop.

The session's one MCP server is **`tuicommander` on the ACP transport**
(#1156-1b61): `{"type":"acp","name":"tuicommander","serverId":"tuicommander"}`,
served on the ACP connection itself through `mcp/connect`, `mcp/message` and
`mcp/disconnect`. It replaced the stdio `tuic-bridge` entry (#796-7fa3), which
opened a fresh HTTP MCP session for every ego tool operation. Every request goes
through the same `mcp_post` handler as HTTP `/mcp`, bound to the connection's
peer identity; the server-side synthesis in `granted` still keeps a caller from
naming a server — see plan §4.5.

| File | What to update |
|------|----------------|
| `src-tauri/src/acp/mod.rs` | `tuicommander_acp_mcp_server` — the entry's name and `serverId`, and `mcp_acp`/`mcp_stdio` in `capability_snapshot` |
| `src-tauri/src/acp/mcp_host.rs` | The `McpOverAcpHost` contract and `McpChannel`: which `serverId` is served, unknown connection ids refused, every connection released when the ACP connection ends |
| `src-tauri/src/acp/manager.rs` | `granted` replaces the caller's list; the connection's peer ID reaches ego's environment and every MCP connection; `set_mcp_host`; `mcp/message` runs under `$/cancel_request` |
| `src-tauri/src/mcp_http/acp_mcp.rs` | `AcpMcpHost` — one protocol session per MCP connection, the GET `/mcp` stream forwarded as notifications, `end_mcp_session` on disconnect; `install` at both startup sites |
| `src-tauri/src/acp_commands.rs`, `src-tauri/src/config.rs` | Persist a root's ACP peer UUID beside its selected conversation and reuse it after reconnect or restart |
| `src-tauri/src/mcp_http/mcp_transport.rs` | Bind bridge mail, child parentage and blocked progress to the ACP peer without a PTY |
| `src/components/AIChatPanel/AIChatPanel.tsx` | The panel frame plus the banners: gap, refusal, "not receiving updates" |
| `src/components/AIChatPanel/useAcpChat.ts` | One connection for the app, rooted at `~/Gits` and started by the first message or "+"; global tabs; the viewed repo sent per prompt; every action it offers |
| `src-tauri/src/acp/manager.rs` (`prompt_with_context`, `shutdown_all`, peer adoption) | `_meta.tuicommander/viewedRepo` on `session/prompt`; one live connection per peer id; every ego ended on app exit |
| `src/components/AIChatPanel/Transcript.tsx` | How each transcript entry is drawn — message, thought, tool call, plan, a turn that ended without answering, the provider-retry status line |
| `src/components/AIChatPanel/Interactions.tsx` | Permission options and elicitation forms; a single choice field with up to three values has direct buttons. `form` is the only mode drawn |
| `src/components/AIChatPanel/SessionControls.tsx` | The options the session publishes, pause/resume/compact, and readable labels for untitled conversations |
| `src/components/AIChatPanel/Composer.tsx`, `draft.ts` | Where a turn is written; the draft is module-scoped so the context menu can seed it |
| `src/components/AIChatPanel/contextMenuActions.ts` | "Explain with AI" / "Fix this error" on terminal right-click, registered through `contextMenuActionsStore` |
| `src/services/acpClient.ts`, `src/services/acpStream.ts` | Every command the panel sends and the frame stream behind it. `adopt` commits to the store only after the pending fetch and the open succeed, and `reconnect` lets go of the id the backend replaced — the new one is a *different* connection |
| `src/stores/acp.ts` | Protocol state per connection: snapshot, journal cursor, pending questions, gap. `applyFrame` takes the reading stream's id because `end` names nobody on the wire |
| `src/stores/acpTranscript.ts` | The render projection, keyed by session. Everything shown before the backend answered must be reversible: `noteUserMessage`/`dropEntry`, `clear`/`restore` |
| `src/panelAdapters/aiChat.tsx` | Registry entry: what makes `Cmd+Alt+A`, the status-bar button, the palette entry and detach work |
| `src/components/PanelOrchestrator.tsx` | Switches between AIChatPanel and DetachedPlaceholder |
| `src/components/DetachedPlaceholder.tsx` | Placeholder shown in the main window when the panel is detached |
| `src/panelRouter.tsx` | Panel adapter registry + routing for detached panel windows |
| `src/utils/panelSync.ts` | PanelSyncProvider + PanelSyncReceiver for main<->detached communication |
| `src/hooks/initPanelWindow.ts` | Bootstrap for detached panel windows (theme, font, settings) |
| `src/stores/ui.ts` | `aiChatPanelVisible` + `detachedPanels` map |
| `src/stores/settings.ts` | `isAiChatEnabled()` — whether the panel is offered at all (reads `experimental_features_enabled`) — and `isAcpConfigured()`/`egoExecutable`, the one binary the host may launch |
| `src/keybindingDefaults.ts` | `toggle-ai-chat` + `detach-activity-dashboard` hotkeys |
| `docs/user-guide/ai-chat.md` | User-facing AI Chat guide |
| `docs/FEATURES.md` | AI Chat feature section |

### AI Chat settings page (ego's own configuration)
Settings → AI Chat reads and writes **ego's** configuration through ego's
command line (#786-4a6d). It replaces the provider registry #784-0aec deleted,
and the replacement is not a registry: TUIC stores no API key, keeps nothing in
its keyring, and makes no provider HTTP call. The one network call anywhere in
this area is `ego models --refresh`, which ego makes, when a person presses
Refresh.

| File | What to update |
|------|----------------|
| `src-tauri/src/ego_cli.rs` | Which ego commands are run, how their three answers are joined, and what an ego failure carries. `model` is the only writable key, spelled as its own operation so no caller can reach `sandbox` or `permissions.judge` |
| `src-tauri/src/mcp_http/ego_routes.rs` | `GET /ego/providers`, `POST /ego/providers/model`. Both are behind `require_local_or_auth` — spawning a process is not a read |
| `src/transport.ts`, `src/__tests__/transport.test.ts` | The `ego_providers` / `ego_set_default_model` mappings and the generated path snapshot |
| `src/types/ego.ts` | The TS mirror of the Rust projection. A mirror, not a second opinion — nothing reshapes it |
| `src/services/egoCli.ts` | The two calls the tab makes |
| `src/components/SettingsPanel/tabs/AiChatTab.tsx` | What the page draws: the default-model picker, the per-provider credential badge, and each of the four failure states |
| `src/components/SettingsPanel/tabs/GeneralTab.tsx` | The **ego** section: the ego executable field, shown also while Experimental Features is off |
| `src/components/SettingsPanel/SettingsPanel.tsx` | The nav entry (hidden while `isAiChatEnabled()` is false) and the tab body |
| `src/components/SettingsPanel/settingsSearchIndex.ts` | Its rows in the settings search index — the drift test re-derives them from the JSX |
| `docs/user-guide/settings.md` | The AI Chat page section |
| `docs/api/tauri-commands.md`, `docs/api/http-api.md` | Both transports for the two commands |
| `docs/FEATURES.md` | The AI Chat page entry under Settings |

### Session knowledge store and TUI detection
Neither module has anything to do with an LLM, which is why both outlived the
engine. `pty.rs` reads them, so they kept the `ai_agent/` module path:

| File | What to update |
|------|----------------|
| `src-tauri/src/ai_agent/knowledge.rs` | CommandOutcome, SessionKnowledge, OSC 133 scanner, persist/load/spawn_persist_task |
| `src-tauri/src/ai_agent/tui_detect.rs` | TerminalMode heuristics (Shell vs FullscreenTui) |
| `src-tauri/src/pty.rs` | ChunkProcessor.record_osc133_outcomes + Inferred fallback in the silence timer |
| `src-tauri/src/state.rs` | session_knowledge DashMap, knowledge_dirty set, has_osc133_integration, record_outcome helper |
| `src-tauri/src/lib.rs` | spawn_persist_task at boot |
| `src-tauri/crates/tuic-core/src/redaction.rs` | `redact_secrets` — lives OUTSIDE `ai_agent/` on purpose, because `session action=output` applies it |
| `src-tauri/crates/tuic-terminal/src/terminal_grid.rs` | Grid reader methods in `tuic-terminal`: `search_buffer`, `enumerate_visible_hyperlinks`, `extract_semantic_zones`; `VtLogBuffer` delegates in `vt_log.rs` |

### What #784-0aec removed, and where it comes back
These areas had sections here and no longer have any code to sync. They are
listed so a search for the feature lands on the reason rather than on nothing.
Do not re-add a section for one of them until the story that restores it lands —
the replacement lives in ego and will have a different file list.

| Removed area | Went with it | Comes back as |
|------|----------------|----------------|
| AI Prompts (diff-triage prompt) | `AiPromptsConfig`, `diff_triage.rs`, `aiPrompts.ts`, the `list/load/save_ai_prompt` MCP config actions | nothing. #795-320b restored the review itself but not the editable prompt: an unattended turn sends one prompt this side owns, so there is nothing for a user to tune |
| AI Agent ReAct loop | `ai_agent/{engine,tools,safety,sandbox,context,commands,triggers,scheduler}.rs`, `SessionKnowledgeBar.tsx` | nothing. ego runs its own tool loop; TUIC is the environment it drives, per `plans/ego-integration/archive/plan.md` §1 |
| Terminal Watcher | `ai_agent/watcher.rs`, the WatcherEngine spawn, the `watcher_*` commands, `WatcherManager/`, `ai-watchers.json` | not scheduled |
| PR review, changelog, improvement scan | `diff_triage.rs`, `improvement_scan.rs`, `changelog.rs`, `aiTriageStore.ts`, `prReview.ts`, `AiTriagePanel/`, `ChangelogModal/` | **landed as 795-320b** — see "PR review, changelog and improvement scan" above. `diff_triage.rs` came back as `pr_review.rs`, because it no longer triages a diff; `aiTriageStore.ts` and `AiTriagePanel/` did not come back at all |
| Smart Prompts `api` execution mode | `llm_api.rs`, `execute_api_prompt` | **landed as 787-ee50** — see "Smart Prompts `api` mode" above. The provider registry did not come back; the mode is one unattended ego turn |

### Remote Daemon (`tuic-remote`)
When modifying the remote daemon binary or standalone server behavior:

| File | What to update |
|------|----------------|
| `src-tauri/src/bin/tuic_remote.rs` | Binary entry point |
| `src-tauri/src/lib.rs` | `run_remote()` — the daemon |
| `src-tauri/src/lib.rs` | `spawn_daemon_background_tasks()` — every task is started or refused with a reason; the guard test is `the_daemon_decides_on_every_desktop_background_task` |
| `src-tauri/src/mcp_http/mod.rs` | `spawn_ipc_listener()` / `spawn_maintenance_sweep()` — shared by the desktop and the daemon |
| `src-tauri/src/remote_lifetime.rs` | Optional idle lifetime driven by SSE and WebSocket client counts; `/health.survive_secs` must stay in sync |
| `src-tauri/src/remote_deploy/{mod,assets,service}.rs` | SSH asset resolution/cache, ephemeral launch, and systemd/launchd install lifecycle; keep Sections 22/24 and the remote-access guide aligned |
| `docs/user-guide/remote-access.md` | `tuic-remote (Beta)` section, incl. "What the daemon runs" |
| `docs/FEATURES.md` | Section 22 (Remote Daemon) |
| `.github/workflows/release.yml` | Release artifact build job — publishes **both** `tuic-remote` and `tuic-bridge` per target |

### SSH Tunnel Management
When modifying tunnel profiles, supervisor, audit logging, backoff, or tunnel UI:

| File | What to update |
|------|----------------|
| `src-tauri/src/tunnels/profile.rs` | TunnelProfile, ForwardSpec; the legacy flat-shape reader and `migrate_legacy_toml` |
| `src-tauri/src/ssh_connection.rs` | `SshConnectionParams` — the SSH settings shared with `RemoteTransport::Ssh` — and the boot-time flat→nested migration (`legacy`); update `docs/backend/config.md`'s migration table and both HTTP shape descriptions in `docs/api/http-api.md` |
| `src-tauri/src/tunnels/command.rs` | SSH command-line argument building |
| `src-tauri/src/tunnels/classifier.rs` | ExitReason enum and stderr classification |
| `src-tauri/src/tunnels/agent.rs` | SSH agent socket discovery |
| `src-tauri/src/tunnels/port.rs` | Local port availability check |
| `src-tauri/src/tunnels/backoff.rs` | BackoffCalculator (delays, jitter, max retries) |
| `src-tauri/src/tunnels/audit.rs` | AuditLog SQLite schema, insert/query/rotate |
| `src-tauri/src/tunnels/supervisor.rs` | TunnelSupervisor lifecycle and reconnect loop |
| `src-tauri/src/tunnels/storage.rs` | ProfileStore: TOML load/save (global + per-repo) |
| `src-tauri/src/tunnels/manager.rs` | TunnelManager: orchestrates supervisors |
| `src-tauri/src/tunnels/commands.rs` | Tauri commands for tunnel CRUD and control |
| `src/stores/tunnels.ts` | Frontend tunnel state (profiles, statuses) |
| `src/stores/tunnelPanel.ts` | Tunnel panel UI state |
| `src/components/TunnelsPanel/TunnelsPanel.tsx` | Tunnel list with start/stop controls |
| `src/components/SettingsPanel/tabs/services/RemoteConnectionEditor.tsx` | Profile create/edit form (Kind "SSH Tunnel"; shares `SshConnectionFields` + `PortForwardsEditor`) |
| `src/components/TunnelsPanel/TunnelStatusBadge.tsx` | Color-coded status indicator |
| `docs/features/ssh-tunnels.md` | Feature architecture doc |
| `docs/FEATURES.md` | Section 23 (SSH Tunnel Manager) |
| `docs/user-guide/remote-access.md` | SSH Tunnel Management section |

### StreamDock M18 Macropad
When modifying the StreamDock device model, render pipeline, gesture resolution, slot-assignment
policy, or the in-app supervisor/config/Settings UI:

| File | What to update |
|------|----------------|
| `src-tauri/crates/tuic-streamdock/src/device/model.rs` | Device table (VID/PID, geometry, hw/write_key maps) — **transcribe hw/write_key changes only from confirmed hardware testing**, never the vendor SDK comment alone (see the module's own doc comment for why) |
| `src-tauri/crates/tuic-streamdock/src/device/actor.rs` | The serializing write actor, heartbeat, shutdown ordering |
| `src-tauri/crates/tuic-streamdock/src/device/reader.rs` | Button-state polling → `InputEvent` |
| `src-tauri/crates/tuic-streamdock/src/device/hotplug.rs` | Device discovery + hot-plug watching |
| `src-tauri/crates/tuic-streamdock/src/dispatch.rs` | Tap/DoubleTap/Hold gesture resolution — see its doc comment for the confirmed press/release timing model before changing thresholds |
| `src-tauri/crates/tuic-streamdock/src/render/` | KeyFace → JPEG rendering (palette, font, cache) |
| `src-tauri/crates/tuic-streamdock/src/policy/` | SlotPlanner (sticky assignment, eviction) and priority ranking |
| `src-tauri/crates/tuic-streamdock/src/port.rs` | `StateSource`/`ActionSink` — the entire coupling surface to any host |
| `src-tauri/crates/tuic-streamdock/src/leds.rs` | Ambient LED ring color (aggregate green/peach/red), deduped per state change |
| `src-tauri/crates/tuic-streamdock/src/coordinator.rs` | The 250ms tick loop tying render+policy+device together |
| `src-tauri/src/streamdock/mod.rs` | `StreamDockManager` supervisor (`apply_config`, start/stop, hot-plug reconnect loop) |
| `src-tauri/src/streamdock/source.rs` | `StateSource` impl reading `AppState` directly |
| `src-tauri/src/streamdock/sink.rs` | `ActionSink` impl — calls the same shared functions the HTTP/IPC surfaces use |
| `src-tauri/src/streamdock/commands.rs` / `tauri_commands.rs` | HTTP routes / IPC twins for status + device listing |
| `src-tauri/src/config.rs` | `StreamDockConfig`, `ConfigSaveEffects::streamdock_changed` |
| `src-tauri/src/state.rs` | `AppEvent::SessionFocusRequested` / `UiActionRequested`, `AppState.streamdock` field |
| `src-tauri/src/mcp_http/session.rs` | `focus_session_impl` / `run_ui_action_impl` / `UI_ACTION_ALLOWLIST` |
| `src/components/SettingsPanel/tabs/StreamDockTab.tsx` | Settings UI (enable, device picker, brightness, pinned sessions) |
| `src/hooks/useAppInit.ts` | `session-focus-requested` listener |
| `src/hooks/useShortcutRegistration.ts` | `ui-action-requested` listener → `dispatchAction` |
| `docs/backend/config.md` | `StreamDockConfig` schema row |
| `docs/api/tauri-commands.md` | `focus_session`, `run_ui_action`, `streamdock_status`, `streamdock_list_devices` |
| `docs/api/http-api.md` | `/sessions/{id}/focus`, `/ui/action`, `/streamdock/status`, `/streamdock/devices` |
| `docs/FEATURES.md` | StreamDock section |

### Remote Connection Manager
When modifying remote connection config, storage, or transport routing:

| File | What to update |
|------|----------------|
| `src-tauri/src/remote_connection.rs` | RemoteConnection, RemoteTransport (`Ssh` nests `SshConnectionParams`; `Local` must fail closed in every runtime arm until its connect flow lands), RemoteConnectionStore, `migrate_legacy_connections_file`, `delete_remote_connection_impl` (the one delete path IPC and HTTP share), the password/token commands |
| `src-tauri/src/connection_test.rs` | Test Connection (`test_connection` IPC / `POST /config/remote-connections/test`): `ConnectionTestResult`, the request-only password, no-redirect health check (Direct pin honoured through `direct_proxy`), SSH via `tunnels::exec::ssh_check_with_binary` + the `-W` daemon check (`ssh_daemon_request_with_binary`); keep `docs/api/http-api.md` and `docs/api/tauri-commands.md` in step |
| `src-tauri/src/direct_proxy.rs` | Direct `https://` certificate pinning: `probe_direct_tls_connection` IPC / `POST /config/remote-connections/probe-direct-tls`, `DirectProxies` pinned loopback relay started only by `remote_runtime::resolve_direct_base_url`; keep `docs/api/http-api.md`, `docs/api/tauri-commands.md`, `docs/user-guide/remote-access.md` in step |
| `src/components/shared/DirectCertConfirmDialog.tsx` | Fingerprint confirmation before pinning a self-signed Direct certificate (driven by `remoteConnectionsStore.connect`) |
| `src-tauri/src/ssh_provision.rs` | Confirmed SSH daemon provisioning: `ProvisionPlan` (built from the same `remote_deploy` command builders it shows), digest-checked `start_ssh_daemon` / `configure_ssh_daemon_password` / `stop_ssh_daemon`, stop-on-disconnect. Stored ids only; keep `docs/api/http-api.md` (`/config/ssh-daemon/*`, `configure-ssh-password`), `docs/api/tauri-commands.md` and `docs/user-guide/remote-access.md` in step. `tuic-remote --set-password-if-unset` (`bin/tuic_remote.rs`, `lib.rs`) is its remote half |
| `src/components/shared/ProvisionConfirmDialog.tsx` | Shows a provisioning plan's commands verbatim; runs only on explicit accept (driven by `remoteConnectionsStore.provision`) |
| `src-tauri/src/remote_runtime.rs` | The live half: status, base URL, session token, the status poll and the SSH tunnel. Every status change is dual-emitted as `remote-connection-status`. **`teardown()` is the one way a connection goes down** — poll, mirror task, mirrored rows, tunnel and token, in that order, idempotent and safe on an id nothing knows. Disconnect and delete both call it, over IPC and over HTTP alike; a second stop path is how the tunnel came to outlive the delete that removed its profile |
| `src-tauri/src/remote_deploy/{mod,assets,service}.rs` | On-connect deployment, cached release assets, and systemd/launchd install/uninstall |
| `src-tauri/src/remote_lifetime.rs` | Ephemeral daemon idle expiry and `/health.survive_secs` |
| `src-tauri/crates/tuic-core/src/credentials.rs` | `Credential::RemoteConnection` — the password, keyed by the connection's UUID |
| `src/stores/remoteConnections.ts` | Frontend remote connections store — a renderer of the backend status, plus the token it holds in memory for the transport |
| `src/transportRuntime.ts` | `withRemoteToken` — the one place a credential is put on a URL; `resolveOwningConnection` — the one place a call's machine is decided |
| `src/stores/repositories.ts` / `src/stores/terminals.ts` | the registered path→connection and session→connection lookups |
| `src-tauri/src/remote_mirror.rs` | Mirrors a connected daemon's sessions and events onto the local bus. The `/events` read carries an idle deadline (3× the daemon's 15 s keep-alive): a silent stream is a dead stream, and it re-seeds rather than waiting forever |
| `src/transport.ts` | `owningConnectionFor` + connectionId-based routing in COMMAND_TABLE |
| `src/invoke.ts` | the desktop IPC path's diversion to `rpc()` for a remotely-owned call |
| `src/components/Terminal/canvasTerminalTransport.ts` | connectionId support for the remote WebSocket |
| `docs/FEATURES.md` | Section 24 (Remote Connection Manager) |
| `docs/user-guide/remote-access.md` | Remote Connection Manager section |

### Terminal stream compression

When changing what a remote WebSocket puts on the wire, or which peers pay for it:

| File | What to update |
|------|----------------|
| `src-tauri/src/mcp_http/ws_compression.rs` | The negotiation, `DEFLATE_SUBPROTOCOL`, the frame tags, the level, and the measurement the level and the stateless choice were made on |
| `src-tauri/src/mcp_http/session.rs` | `ws_stream` reads `?compress=` and the peer address, and selects the subprotocol when the mode is tagged; all three handlers send through `WsFrameSender` |
| `src-tauri/src/mcp_http/types.rs` | `OutputQuery::compress` |
| `src/components/Terminal/wsFrameCodec.ts` | The client half — the tag values must match `FrameTag` literal for literal, and `DEFLATE_SUBPROTOCOL` must match its Rust twin |
| `src/components/Terminal/canvasTerminalTransport.ts` | Who asks for the encoding, who reads the acceptance off `ws.protocol` in `onopen`, and the chain keeping inflated deltas in order |
| `src-tauri/src/ssh_connection.rs` / `tunnels/command.rs` | `SshConnectionParams::compression` → `ssh -C`; the WebSocket's loopback refusal assumes this is on |
| `docs/api/http-api.md` | "WebSocket compress=deflate" — the table of what is compressed by what |
| `docs/user-guide/remote-access.md` | What a user is told about it |

### Machine-owned configuration

When changing which machine a config family belongs to, or adding a config file
that describes a machine rather than this app:

| File | What to update |
|------|----------------|
| `src/stores/agentConfigs.ts` | The per-machine registry: `agentConfigsFor`, `ensureAgentConfigs`, `invalidateAgentConfigs`, and the remote IO that names the connection |
| `src/stores/remoteConnections.ts` | `applyStatus` — the connection edge that drops and refills a machine's cached config |
| `src/components/SettingsPanel/MachineSelector.tsx` | The selector every tab in the machine family carries |
| `src/components/SettingsPanel/tabs/AgentsTab.tsx` | Run configs, the hook toggles, and which controls stay local because they have no remote route |
| `src/components/SettingsPanel/tabs/services/UpstreamMcpPanel.tsx` | Upstream MCP servers, and the OAuth flow that cannot follow a machine |
| `src/__tests__/remoteRepoRouting.test.ts` | Both halves: what follows the repo, and what must never leave this machine |
| `docs/backend/config.md` | Agents Config section — the ownership rule and the local families |
| `docs/user-guide/remote-access.md` | "Which config follows the repo, and which stays here" |

### Project Progress (storage)

When modifying the project-owned Progress store, its ownership resolution, or
its Git-exclude/watcher interaction:

| File | What to update |
|------|----------------|
| `src-tauri/src/progress/store.rs` | Schema, migrations, recovery, and SQLite locking behavior |
| `src-tauri/src/progress/ownership.rs` | Registered and nested workspace ownership resolution |
| `src-tauri/src/progress/model.rs` | Entry kinds, which of them are reportable, and validation limits |
| `src-tauri/src/progress/service.rs` | The `progress_tracking` gate and the shared record/list/delete core |
| `src-tauri/src/progress/flow.rs` | The Flow sequence: participants, arrow joining, scope, redaction |
| `src-tauri/src/pty.rs` | `intent:` capture — the host's half of the journal |
| `src-tauri/src/mcp_http/mcp_transport.rs` | `journal_hand_off` at `agent action=spawn` / `send` — the `delegated` and `message` entries |
| `src-tauri/src/repo_watcher.rs` | The journal must stay outside every repository; the test asserts the tree is byte-identical after a write |
| `docs/backend/config.md` | Progress Storage section |
| `docs/api/http-api.md` | Project Progress section |
| `docs/api/tauri-commands.md` | Progress command table rows |
| `docs/user-guide/project-progress.md` | The whole document |

### Project Progress (dialog)

When modifying the Progress dialog, its store, or the surfaces that open it:

| File | What to update |
|------|----------------|
| `src/components/ProgressDialog/` | The list, the frozen divider, the blocked filter, `embedded` mobile mode, the Flow view (`ProgressFlow.tsx`) |
| `src/stores/progress.ts` | The one-project query, the frozen divider, live presentation |
| `src/actions/actionRegistry.ts` | The `progress` action, and the browser allowlist in `CommandPalette.tsx` |
| `src/components/Toolbar/Toolbar.tsx` | The aggregate unread bell row |
| `src/mobile/MobileApp.tsx` | The lazy Progress tab |
| `docs/frontend/components.md` | ProgressDialog entry |
| `docs/frontend/stores.md` | progressStore entry |
| `docs/user-guide/command-palette.md` | Project Progress action |
| `docs/user-guide/project-progress.md` | The whole guide — it describes the dialog |
| `docs/user-guide/settings.md` | The two `progress_tracking` toggles in the Agents table |
| `docs/FEATURES.md` | Project history row |

### Native Stories (manual plans and stories)

When modifying the native story store, its actions, or any of the four surfaces
that reach them:

| File | What to update |
|------|----------------|
| `src-tauri/src/stories/store.rs`, `store/records.rs`, `store/transitions.rs` | Schema version, claims, revisions, transitions, and the read-only claim probe on session close |
| `src-tauri/src/stories/model.rs`, `api.rs` | The `StoryAction` / `StoryReply` wire shapes and project scoping — one shape for every transport |
| `src-tauri/src/pty.rs` | Claim release on session teardown (`remove_live_session_state`) |
| `src-tauri/src/lib.rs` | `story_action_command` (IPC) |
| `src-tauri/src/mcp_http/mod.rs` | `POST /stories/action` and its auth guard |
| `src-tauri/src/mcp_http/mcp_transport.rs` | The MCP `story` tool (`handle_story`, bound caller only) |
| `src-tauri/crates/tuic-cli/src/main.rs` | `tuic story` |
| `src/transport.ts` + `src-tauri/src/mcp_http/command_table_paths.txt` | The `story_action_command` mapping; regenerate the paths file with `pnpm vitest run src/__tests__/transport.test.ts -u` |
| `src/components/StoriesDialog/`, `src/stores/storiesUi.ts`, `src/components/Toolbar/Toolbar.tsx` | The dialog, its visibility store, and the toolbar entry point |
| `docs/api/tauri-commands.md`, `docs/api/http-api.md`, `docs/backend/mcp-http.md` | The command, the route, and the MCP tool |
| `docs/user-guide/native-stories.md`, `docs/FEATURES.md`, `SPEC.md` | The guide, the feature row, and the specification |

### Git & Worktree Integration
When modifying git operations, worktree logic, or GitHub API:

| File | What to update |
|------|----------------|
| `docs/backend/git.md` | Git command lifecycle, diff parsing, **GitReads port (gix vs CLI op split)**, moka cache |
| `src-tauri/crates/tuic-git/src/git_reads.rs` | **GitReads port**: flipping an op to gix requires a green byte-parity shootout test first |
| `src-tauri/crates/tuic-git/src/{github,github_account,github_auth,github_poller,circleci,pr_review,changelog,github_debug}.rs` | Pure GitHub models, parsing, and decisions; keep network, credentials, scheduling, events, and commands in the root adapters |
| `docs/backend/github.md` | PR fetching, CI checks, GraphQL |
| `docs/user-guide/worktrees.md` | Worktree workflow, configuration |
| `docs/user-guide/github-integration.md` | PR monitoring, CI rings |
| `docs/FEATURES.md` | Sections 7 (Git) and 8 (GitHub) |
| `docs/api/tauri-commands.md` | Git/worktree commands |

### Voice dictation, spoken replies and hands-free
When modifying `src-tauri/src/dictation/**`, `src-tauri/crates/tuic-dictation/**`, or the Voice settings page (`DictationSettings.tsx`):

| File | What to update |
|------|----------------|
| `docs/backend/dictation.md` | Commands, hands-free runtime, the speech port and its adapters, the speech-asset catalogue, and the one-language rule |
| `docs/api/tauri-commands.md` | The Voice Dictation command table — including which config fields drop the voice built for the previous one |
| `docs/api/http-api.md` | The `/dictation/**` routes and the wire shapes. **`SpeechAssetInfo` is snake_case**; `SpeechStatus`, `HandsFreeStatus`, `SpokenReply` and `HandsFreeDisarmed` are camelCase |
| `docs/frontend/stores.md` | `dictationStore` — the speech assets, the polled hands-free/speech status, and the fields `saveConfig` reads off the stored config rather than store state |
| `docs/frontend/components.md` | `DictationSettings` — including which groups stay behind `isTauri()` now that the tab itself renders in browser mode |
| `src/utils/browserVoice.ts` | The browser half of the hands-free audio socket. Transport only: any decision added here becomes a second segmenter that exists only in browsers |
| `src/components/SettingsPanel/settingsSearchIndex.ts` | One row per new label, **in source order**: `extractSettings` reads the file, not the render tree |
| `src/i18n/en.json` | Every `t("key", "double-quoted fallback")`, verbatim — `i18nKeyCollisions.test.ts` is the gate |
| `docs/FEATURES.md` | Section 9 (Voice Dictation) |
| `to-test.md` | Anything needing real audio, a real download or Boss's eye — a Rust change here never reaches his live `make dev` session |

### Session Diff Review
When modifying transcript parsing, base resolution, revert mechanisms, or the review UI:

| File | What to update |
|------|----------------|
| `src-tauri/src/session_review.rs` | Transcript parser, base-resolution tiers, revert commands |
| `docs/backend/session-review.md` | Transcript format reference, base-resolution table, the two revert mechanisms, cache invariant |
| `docs/api/tauri-commands.md` | The four `session_review.rs` commands |
| `docs/api/http-api.md` | The four `/repo/session-review/*` routes |
| `docs/backend/mcp-http.md` | Route table |
| `src/components/SessionDiffTab/` | Frontend tab (picker, list, file/step rows, row-model builder) |
| `src/types/sessionDiff.ts` | TS mirror of the Rust wire types — keep field-for-field in sync |
| `src/hooks/useRepository.ts` | `listReviewSessions`/`getSessionReview`/`revertSessionStep`/`revertFileToSessionStart` |
| `src/stores/diffTabs.ts` | `SESSION_SCOPE`/`isSessionReviewTab`/`addSessionReview`/`setSessionId` |
| `docs/frontend/components.md` | Component tree |
| `docs/FEATURES.md` | Section 7.5 (Diff) |
| `docs/user-guide/branches.md` | User-facing walkthrough |

### Worktree Automation Scripts (Setup/Archive/Run) & `TUIC_*` Context
When touching Setup/Archive/Run script execution, their `TUIC_*` environment, their timeout, or the post-create chain order:

| File | What to update |
|------|----------------|
| `src-tauri/src/script_env.rs` | `ScriptContext::pairs()` — the single source of truth for every `TUIC_*` name; a name here must follow the `TUIC_<UPPERCASE>` convention (a frontend parity test checks this against the Smart Prompts variable registry, see the Smart Prompts Context Variables section) |
| `src-tauri/src/worktree.rs` | `run_setup_script`/archive wrappers (pass `ScriptContext` pairs into tuic-git's `UserScript`), `spawn_worktree_setup_chain`/`run_worktree_setup_chain` (the warm → sync → script background ordering + deferred warm status), `resolve_archive_script` |
| `src-tauri/crates/tuic-git/src/worktree.rs` | `run_shell_script` (default `SCRIPT_TIMEOUT`, overridable per call via `UserScript::timeout` / `run_setup_script_with_env`'s `timeout`; PATH enrichment), `run_background_warm_blocking` (warm without publishing its status) |
| `src-tauri/src/smart_prompt.rs` | `apply_clean_env`'s `ctx` slot — Smart Prompt shell/headless children get the same `TUIC_*` context |
| `src-tauri/src/pty.rs` | `inject_worktree_env`, called at every `bind_pty_identity` site — this is how the Run Script (typed into the PTY, not a real child process) sees the context |
| `src-tauri/src/config.rs` | `resolve_effective_setup_script`, `resolve_effective_base_branch`, `setup_script_timeout`/`archive_script_timeout` (config-file-only `RepoDefaultsConfig` fields — keep `docs/backend/config.md`'s Repo Defaults table and `docs/user-guide/settings.md`'s Scripts timeout sentence in step; `src/stores/repoDefaults.ts` must carry them through saves) |
| `src-tauri/src/mcp_http/worktree_routes.rs` | `run_setup_script_http` (`POST /worktrees/run-script`) — keep its response shape (`{exit_code, stdout, stderr}`) identical to the Tauri command, no `transform` needed in `transport.ts`; `get_worktree_setup_status_http` (`GET /worktrees/setup-status`) |
| `src-tauri/src/state.rs` | `WorktreeSetupStatus`/`AppState::worktree_setup_status` — the pollable snapshot `spawn_worktree_setup_chain`/`run_worktree_setup_chain` write at each transition |
| `src-tauri/src/mcp_http/mcp_transport.rs` | `handle_worktree`'s `"worktree_setup_status"` arm, `handle_repo`'s dispatch list, `REPO_ACTIONS`/the `repo` tool's schema description |
| `src/transport.ts` + `src/__tests__/transport.test.ts` | The `run_setup_script` → `/worktrees/run-script` mapping and its parity assertions |
| `docs/user-guide/settings.md` | The `TUIC_*` table (Scripts Tab section) — the canonical user-facing list |
| `docs/user-guide/ai-agents.md` | "Worktree Context (TUIC_\*)" section (grouped with `TUIC_SESSION` above it) — the fixed-at-spawn caveat |
| `docs/api/tauri-commands.md` | `run_setup_script`'s signature/response |
| `docs/api/http-api.md` | `GET /worktrees/setup-status` |
| `docs/backend/mcp-http.md` | REST-table row for `GET /worktrees/setup-status` |

**Ordering:** `spawn_worktree_setup_chain` awaits the CoW warm (on paths that warm) and then the file sync (`worktree-sync-*` events, above) before resolving/running the setup script, on all three worktree-creation paths (desktop, HTTP `create_worktree_shared` incl. MCP `repo worktree_create`, HTTP `create_session_with_worktree`); the warm status stays `pending` until the script is done. All steps run in the background — worktree creation itself has already returned by the time either happens — so the setup script's outcome is reported via the dual-emitted `worktree-setup-script-completed` event, not a synchronous response field. See `worktree.rs`'s doc comment on `spawn_worktree_setup_chain` for the full rationale, including why the MCP tool response no longer carries `setup_script`/`setup_script_error`. An MCP client, which has no event stream, instead polls `repo action=worktree_setup_status` / `GET /worktrees/setup-status` — see `state.rs`'s `WorktreeSetupStatus` doc comment. The warm skips its copy when `warm_ignored_directories` is off and reports progress via `worktree-warm-*` (above) and the pending `warm_artifacts` detail (`phase`, `copied`, `total`) that `worktree_list`/`GET /worktrees/paths` return. Status writes and the terminal event are gated on a per-chain generation (`worktree.rs SETUP_CHAIN_GENERATIONS`), so a stale chain for a removed-and-recreated `(repo, branch)` cannot touch its successor.

### Settings & Configuration
When adding config fields or settings UI:

| File | What to update |
|------|----------------|
| `docs/backend/config.md` | Config files, schema, platform directories |
| `docs/user-guide/settings.md` | Settings page breakdown, by navigation group |
| `docs/FEATURES.md` | Section 11 (Settings) |
| `src/components/SettingsPanel/settingsTabs.ts` | `GLOBAL_TAB_GROUPS` — which group and page a new page belongs to (`hiddenTabs` for availability) |
| `src/components/SettingsPanel/settingsSearchIndex.ts` | Search index rows for every new label; the drift test re-derives them from the JSX |
| `src/components/SettingsPanel/ExpertSetting.tsx`, `src/stores/settingsExpert.ts` | Wrap a control in `ExpertSetting` only with a `configKey` that `get_config_defaults` can resolve; update the Expert Mode section of `docs/user-guide/settings.md` if the visibility rule changes |

### Indicator Registry (customizable colors/icons/animations)
When adding, removing, or changing a customizable visual indicator (terminal status dots, tab
types, sidebar symbols, PR badges, git repo status, diff stats):

| File | What to update |
|------|----------------|
| `src/indicators/registry.ts` | `INDICATORS` — the single source of truth. Add/edit an entry here, never a hardcoded color/label elsewhere |
| `src/global.css` | `:root` default for every `colorVar`/`animVar` the entry names (always `var()`-of-a-token, never a raw hex) |
| Consuming CSS (`TabBar.module.css`, `Sidebar.module.css`, `PaneTree.css`, `ChangesTab.module.css`) | Read `var(--ind-*)`, never the underlying palette token directly |
| `src/components/HelpPanel/UiLegend.tsx` | Swatch groups (terminal dots, tab types, git repo status, diff stats) render from the registry automatically. Sidebar-symbol and PR rows render the real components (`SIDEBAR_SYMBOL_LEGEND`/`PR_BADGE_LEGEND`, each row's `indicatorId` names the entry it edits); a new PR/sidebar entry needs a row there or it is listed after them as a plain swatch. A new **group** needs a `GROUP_LABELS`/`GROUP_HINTS` entry and, if it should be independently hideable, a `groupToggleBinding` case + a `show*` bool in `AppConfig`/`SettingsStoreState` |
| `src/components/Sidebar/PrStateBadge.tsx` + `Sidebar.module.css` | A new PR state needs a `PR_BADGE_CLASSES` pill class AND a `PR_MARK_CLASSES` compact-marker class reading its `--ind-pr-*` var (`markColorVar` when the marker's default differs) |
| `src/indicators/IndicatorEditorDialog.tsx` | The combined color/icon/animation editor for one indicator, opened by clicking that row's own preview icon in `UiLegend.tsx` (editable mode). Sections gate on `entry.capabilities` automatically — a new entry needs no changes here |
| `src/__tests__/indicators/registryParity.test.ts` | Source-text-parity test enforcing the above — will fail the build on a missed step, not just remind you |
| `docs/frontend/STYLE_GUIDE.md` | "Indicator Customization" section — describes the `--ind-*`/`tuic-*` layer conceptually; do not hand-copy a color table here (it will drift) |
| `docs/user-guide/settings.md` | Appearance Tab section — the UI Legend prose + the visibility-toggle table, if a new group toggle was added |
| `docs/backend/config.md` | `indicator_overrides` row's group list, and any new `show_*` bool |

### Agent Detection

Launch-scoped Claude/Codex status changes must keep `agent_hook_launch.rs`, `shell_integration.rs`, direct/MCP spawn assembly, agent config types, Settings, HTTP/Tauri transport parity, the route snapshot, AI-agent/config/settings docs, FEATURES and CHANGELOG synchronized.
When adding agents or changing detection logic:

| File | What to update |
|------|----------------|
| `docs/user-guide/ai-agents.md` | Agent support, detection method |
| `docs/backend/output-parser.md` | Agent-specific parsing rules |
| `docs/FEATURES.md` | Section 6 (AI Agent Support) |
| `src-tauri/src/mcp_http/plugin_docs.rs` | agentTypes valid values in PLUGIN_DOCS |

### UI Components & Panels
When adding or modifying panels, status bar, toolbar, sidebar:

| File | What to update |
|------|----------------|
| `docs/FEATURES.md` | Relevant section (2-5: Sidebar, Panels, Toolbar, Status Bar) |
| `docs/frontend/STYLE_GUIDE.md` | If changing visual patterns |
| `docs/frontend/components.md` | Component tree, panel descriptions |
| Domain user guide | e.g. `docs/user-guide/sidebar.md`, `docs/user-guide/file-browser.md` |

### Markdown Inline Review Comments (tweaks) & Highlight Rendering
When modifying the tweak-comment format, the selection/popover UI, or the DOM highlight wrapping:

| File | What to update |
|------|----------------|
| `src/utils/tweakComments.ts` | Marker format, parse/insert/remove/update, sentinels, convention header |
| `src/utils/tweakDomHighlight.ts` | DOM-side sentinel→`.tweak-highlight` span wrapping |
| `src/components/MarkdownTab/CommentOverlay.tsx` | Floating Comment button + inline popover + hover tooltip |
| `src/components/MarkdownTab/MarkdownTab.tsx` | Save/delete wiring, write-back to disk |
| `src/components/ui/ContentRenderer.tsx` | Sentinel injection + `applyTweakDomHighlights` on render (shared with the AI Chat panel) |
| `docs/FEATURES.md` | Section 3.3 (Markdown Panel) — Inline review comments |

### TUIC SDK & iframe Integration
When modifying the TUIC SDK, iframe postMessage protocol, path resolution, or tab injection:

| File | What to update |
|------|----------------|
| `src/components/PluginPanel/tuicSdk.ts` | Inline SDK script for plugin iframes |
| `src/components/PluginPanel/resolveTuicPath.ts` | Path resolution (relative/absolute, traversal guard) |
| `src/components/PluginPanel/PluginPanel.tsx` | Host-side message handlers, SDK injection |
| `docs/tuic-sdk.md` | SDK reference — API methods, path resolution, testing |
| `docs/examples/sdk-test.html` | Interactive test page (update when adding SDK methods) |
| `docs/plugins.md` | Plugin developer guide (if plugin-facing API changes) |

### Deep Links
When adding or changing `tuic://` schemes:

| File | What to update |
|------|----------------|
| `docs/FEATURES.md` | Section 17.4 (Deep Links) |
| `docs/plugins.md` | If affecting plugin contentUri format |

### macOS Finder Service ("New TUICommander Tab Here")
When changing the Finder Service bundle, the `tuic open-here` CLI subcommand, the
`tuic://open-terminal` deep link, or the repo-placement ladder it resolves through:

| File | What to update |
|------|----------------|
| `src-tauri/services/New TUICommander Tab Here.workflow/` | The hand-authored Automator bundle — re-verify with `automator -i <path> "services/New TUICommander Tab Here.workflow"` after any change |
| `src-tauri/src/finder_service.rs` | Install/uninstall/status commands |
| `src-tauri/crates/tuic-cli/src/main.rs` | `cmd_open_here`, `resolve_open_here_paths`, `build_open_terminal_url` |
| `src/deep-link-handler.ts` | `open-terminal` case, `openTerminalAtPath` |
| `src/stores/terminalPlacement.ts` | `resolvePlacementForCwd` — the ladder itself |
| `src/components/RepoPickerDialog/`, `src/hooks/useRepoPickerDialog.ts` | The "ask the user" rung |
| `docs/FEATURES.md` | Section 17.4 (Deep Links) + Finder integration entry |
| `docs/api/tauri-commands.md` | The four `finder_service` commands |
| `docs/user-guide/finder-integration.md` | User-facing behavior of the placement ladder |
| `docs/user-guide/cli.md` | `tuic open-here` |
| `docs/user-guide/settings.md` | Settings → General install/uninstall control |

### Documentation Site (mdBook + Pagefind)
### Mutation testing
When changing how `make mutants` selects, builds or runs mutants:

| File | What to update |
|------|----------------|
| `src-tauri/.cargo/mutants.toml` | Test tool, cargo test args, excluded globs |
| `scripts/mutants.sh` | local `git clone --local` copy + `--in-place` + `--in-diff` mechanics, the `dist` copy `generate_context!` needs |
| `Makefile` | The `mutants` target and its `RANGE` default |
| `src-tauri/AGENTS.md` | "Tests" section bullet (per-change rule, who runs it, surviving-mutant policy) |
| `CONTRIBUTING.md` | The install line and the one-paragraph mention |

### Test scratch roots
When changing where tests, the wrapper or fixtures put scratch files or Unix sockets (`TUIC_TEST_HOST_TMPDIR`, `TUIC_TEST_TMP_BASE`, `TUIC_TEST_TMP_ROOT`, `TUIC_TEST_SOCKET_ROOT`, prune rules, the socket budget):

| File | What to update |
|------|----------------|
| `scripts/with-test-tmp.sh` | Resolution order and prune rules; keep `scripts/test-with-test-tmp-prune.sh` / `-readonly-home.sh` in step |
| `src-tauri/crates/tuic-test-support/src/lib.rs` | `host_temp_dir`/`test_base`/`test_temp_root`, socket candidates and `MAX_SOCKET_ROOT_LEN`; `tests/socket_root.rs`, `tests/temp_roots.rs` |
| `src-tauri/scripts/nextest-test-tmp.{sh,ps1}` | Same default as the wrapper when no root was chosen |
| `scripts/test-tmp-root.mjs` | Node-side resolver for `vitest.config.ts` and `scripts/check-frontend-cycles.mjs` |
| `scripts/check-no-home-gits.mjs` | Scan roots, patterns and the allow-list |
| `scripts/run-remote-fixture.sh`, `scripts/test-remote-mcp.py`, `tools/ipad-repro/run.sh` | Fixture scratch and the printed fixture `TMPDIR` (`docs/user-guide/remote-access.md`) |
| `docs/guides/development-setup.md` | "Testing" table and prune description |
| `AGENTS.md`, `CONTRIBUTING.md` | The one-line test temp root descriptions |
| `src-tauri/AGENTS.md` | "Tests (Rust)" exec-scan bullet and the in-checkout `find_repo_root` trap |

### Config-instance scoping (`TUIC_APP_INSTANCE`)
When changing which make target launches against which configuration directory:

| File | What to update |
|------|----------------|
| `Makefile` | The `dev` and `test` recipes and the ONE global per-checkout `TUIC_APP_INSTANCE` default (`tuic-<checkout dir>`). Every `tauri dev` recipe passes `TUIC_APP_INSTANCE=$(TUIC_APP_INSTANCE)` and `$(WARN_SHARED_INSTANCE)`; no target-specific `target: TUIC_APP_INSTANCE?=…` split |
| `scripts/check-make-instance-scope.sh` | The expected value per target (per-checkout id, overrides, the warning only for an empty value), and the structural checks. It asks `make -n` what it expands rather than reading the line |
| `scripts/test-check-make-instance-scope.sh` | Fixture checkouts for the id derivation and one mutated Makefile per shape the guard must reject |
| `scripts/hooks/pre-commit` | Gate 1 runs that script when `Makefile` is staged. Gate 2 is the agent-state fixture gate; `TUIC_SKIP_FIXTURE_GATE=1` deliberately skips only gate 2 |
| `docs/backend/config.md` | The override forms and what each target defaults to |
| `CHANGELOG.md` | Only when the behaviour a user sees changes |

When adding, renaming or moving a docs page:

| File | What to update |
|------|----------------|
| `docs/SUMMARY.md` | **Required** — mdBook only renders, and Pagefind only indexes, chapters listed here. A file that is not in `SUMMARY.md` is invisible to readers and to search |
| `docs/index.md` | "Popular articles" cards and "Browse by section" list, if the page belongs there |
| `scripts/build-docs.sh` | Only when the pipeline changes (excluded pages, Pagefind flags, HTML rewrites) — CI and `make docs` both run this one script |
| `docs/guides/development-setup.md` | "Documentation Site" section, if the build steps change |

## Documentation File Index

| Path | Purpose |
|------|---------|
| **Root** | |
| `agent-signal-architecture.html` | Big-picture reference — how hooks (OSC 7770), the OSC 777 notification format, and screen-scraping all converge into one `SessionState` and one scrollbar-tick system; per-agent signal matrix; start here before the two below |
| `tuic-escape-sequences.html` | Terminal protocol reference (OSC/CSI/SGR/modes, both directions) |
| `SPEC.md` | Feature specification, architecture, version |
| `CHANGELOG.md` | Release history (Keep a Changelog format) |
| `AGENTS.md` | Project rules, compact reference |
| `CONTRIBUTING.md` | Contributor guide (test requirements, PR quality gates) |
| `to-test.md` | Manual testing tracker |
| **plans/ (main checkout only, gitignored)** | |
| `plans/docs/hook-lifecycle.html` | Native hook instrumentation reference — event→wire→feature pipeline, CLI surface, per-agent event tables, rationale. Despite the name matching `agent-signal-architecture.html`'s style, this one is NOT tracked at repo root — confirmed absent from `git ls-files` (2026-09-15); it only exists as a `plans/docs/`-directory reference doc in the main checkout, same as `plans/network-aware-remote-access.md` below |
| **docs/** | |
| `docs/FEATURES.md` | Canonical feature inventory (single source of truth) |
| `docs/plugins.md` | Plugin developer authoring guide |
| `docs/tuic-sdk.md` | TUIC SDK reference (inline + URL tab postMessage protocol) |
| `docs/api/tauri-commands.md` | All Tauri IPC commands |
| `docs/api/http-api.md` | REST/HTTP endpoint reference |
| `docs/architecture/overview.md` | High-level architecture |
| `docs/architecture/data-flow.md` | IPC and data flow |
| `docs/architecture/state-management.md` | Store patterns |
| `docs/backend/pty.md` | PTY session lifecycle |
| `docs/backend/output-parser.md` | Output parsing and structured events |
| `docs/backend/git.md` | Git operations |
| `docs/backend/github.md` | GitHub API integration |
| `docs/backend/config.md` | Configuration file management |
| `docs/backend/mcp-http.md` | MCP/HTTP server, lazy tool discovery, meta-tools |
| `docs/backend/dictation.md` | Whisper voice dictation |
| `docs/backend/error-classification.md` | Error types and backoff |
| `docs/frontend/STYLE_GUIDE.md` | Visual design rules |
| `docs/frontend/components.md` | Component tree reference |
| `docs/frontend/hooks.md` | Custom hooks |
| `docs/frontend/stores.md` | SolidJS stores |
| `docs/frontend/transport.md` | Tauri/HTTP dual-mode transport |
| `docs/frontend/utilities.md` | Utility function reference |
| `docs/frontend/terminal-features.md` | Consolidated terminal behaviors, shortcuts, configurable features |
| `docs/features/ssh-tunnels.md` | SSH tunnel architecture and module map |
| `docs/user-guide/*.md` | User-facing guides (20 files) |
| **Code-embedded docs** | |
| `src-tauri/src/mcp_http/plugin_docs.rs` | AI-optimized plugin reference (`PLUGIN_DOCS` const) |
| `src/actions/actionRegistry.ts` | ACTION_META → auto-populates HelpPanel + Command Palette |
| `examples/plugins/` | Reference plugin implementations (7 examples) |

Remote upstream health (#1444-95a4): remote_mirror forwards upstream-status-changed with the locally saved host label; mcpPopupStore refreshes the originating daemon through explicit RPC into remoteStatus. Remote events never refresh or save local server configuration. App-lifetime notices remain visible while the popup is closed; disconnect invalidates in-flight snapshots.

Remote ACP notices wake a separate daemon-scoped interaction queue in AI Chat. Pending questions and responses use the existing ACP HTTP routes with an explicit remote owner; settlement/disconnection remove only that daemon’s questions. Local ACP connections are not attached or changed.

GitHub pollers now cold-start without a desktop AppHandle. Remote PR/issue snapshots and transitions share the notice forwarding path; repository ownership gates desktop consumers, daemon-owned URLs are fetched explicitly, and remote transitions never invoke local automation. Bell identity includes daemon ownership.
