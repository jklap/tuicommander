# TUICommander — Project Rules

This file is repo-wide: it applies no matter which part of the tree you're touching.
Subsystem-specific rules live next to the code they govern, each with its own
`AGENTS.md` (and a `CLAUDE.md` that just imports it, so Claude Code picks it up
automatically when you're working in that directory):

| Area | File |
|---|---|
| Rust/Tauri backend (PTY, agent-state detection, worktrees, MCP HTTP server, alacritty patches, build/test mechanics) | [`src-tauri/AGENTS.md`](src-tauri/AGENTS.md) |
| `tuic-cli` crate | [`src-tauri/crates/tuic-cli/AGENTS.md`](src-tauri/crates/tuic-cli/AGENTS.md) |
| `tuic-hook` crate | [`src-tauri/crates/tuic-hook/AGENTS.md`](src-tauri/crates/tuic-hook/AGENTS.md) |
| `tuic-streamdock` crate | [`src-tauri/crates/tuic-streamdock/AGENTS.md`](src-tauri/crates/tuic-streamdock/AGENTS.md) |
| SolidJS frontend (reactivity, focus/modals, panel refresh) | [`src/AGENTS.md`](src/AGENTS.md) |
| Settings panel (tri-state settings, search index, export/import) | [`src/components/SettingsPanel/AGENTS.md`](src/components/SettingsPanel/AGENTS.md) |
| Canvas terminal component (selection, frame protocol, command blocks) | [`src/components/Terminal/AGENTS.md`](src/components/Terminal/AGENTS.md) |

If a change spans both a nested area and this file's scope, read both — they cross-reference each other rather than duplicating content.

## Doc Sync

Read [`docs/sync-matrix.md`](docs/sync-matrix.md) before any feature/API/config change — it maps code areas to docs that MUST be updated.


## Tests

- Before declaring a change complete, run `./scripts/check-gate.sh` (or `make check-gate`) —
  not a scoped test filter, and not a bare `make check 2>&1 | tee log` (that pipeline's exit
  code is `tee`'s, not `make`'s, so a real failure can go unnoticed). Repo-wide consistency
  tests — e.g. the IPC/HTTP Parity mapping test — only run as part of the full suite and
  will not show up in a feature-scoped filter.
- Tests are the spec. When a test fails after a code change, investigate BOTH sides before deciding which to fix.
- **Finding a story partially implemented does NOT mean it's done.** When you pick up a story and discover the feature already exists, verify EVERY part of the story is honored — each acceptance criterion, edge case, and requirement — before marking it complete. Never assume the whole story is satisfied just because one part is implemented. Check each criterion against the code and prove it, or the story isn't done.
- `to-test.md` tracks features awaiting manual testing — add items there for minor features.
- **CI has never executed on this repository.** `.github/workflows/ci.yml` triggers on push and exists, but `gh run list` returns zero runs for this repo on any branch. "CI is green" is never evidence of anything — nothing in this repo's test suite has ever been machine-validated by CI. Treat any CI-shaped claim in a commit message or PR description as unverified until someone actually watches a run. `CONTRIBUTING.md` describes CI as an active gate; it isn't yet.
- **Fixed 2026-09-25 (`15406b2b5`):** `make check`'s "plugin tests" step used to be expected to fail (stale submodule pin `497bb1f` had zero test files, and that same pin also failed `cargo check` outright for unrelated `include_str!` references — see `src-tauri/AGENTS.md`'s expanded note). The pin now points to `6dc4b8047`, which has real test files and everything `SEEDED_PLUGINS` references; a red "plugin tests" step is a real failure again, not known drift — investigate it. The submodule can still show up as *uninitialized* in a fresh worktree (a separate, still-expected drift mode — see `src-tauri/AGENTS.md`'s Fresh Worktree Setup section), which is different from a stale pin.
- **`/code-review` and `/security-review` default to diffing the whole branch/working tree, not "the change you just made."** Confirmed repeatedly (7+ times across sessions) that passing a scope-override instruction in the invocation's own prompt/args is NOT reliable — the skill can still compute its own git-status-based diff and hand that to the reviewing agent instead. The fix that actually works: save the exact diff you want reviewed to a file first (`git diff > /tmp/whatever.txt`, including untracked new files via a temporary `git add -N` + `git diff` + `git reset` round-trip if needed), then launch the review agent with an explicit instruction to read that file and NOT run its own `git diff`/`git status`. A same-turn correction or a post-hoc message to an already-running review agent is too late — the scope has to be right in the agent's first message.
- **Before writing what you believe is a "new" test file, verify it doesn't already exist.** `Write`'s "must Read first" safeguard only fires for paths *outside* the current working directory, so it will not stop you from silently overwriting an existing in-repo test file with no warning. A 2026-08-28 session did exactly this to `CreateWorktreeDialog.test.tsx` — a subagent's exploration summary claimed "zero tests exist for this component," which was wrong (a 919-line file already existed), and `Write` clobbered it. Caught only by a routine final `git status --porcelain`/`git diff --stat` sweep before wrapping up, which is why that sweep is not optional on multi-file work. Run `ls`/`git status <path>` yourself before `Write`-ing a file whose non-existence you're only inferring from someone else's report.
- **`[HUMAN]` is a last resort.** Before marking a to-test item `[HUMAN]`, you MUST attempt verification through this escalation ladder:
  1. **Code inspection** — read the source, confirm the logic exists at file:line
  2. **Test execution** — `cargo nextest run` (doctests: `cargo test --doc`), `vitest run` with relevant filter
  3. **CLI probing** — `curl` HTTP endpoints, `grep` for patterns
  4. **MCP maccontrol** — take screenshots, click UI elements, verify visual state
  5. **MCP invoke/JS** — call Tauri commands, inspect store state, trigger actions programmatically
  Only use `[HUMAN]` when the item genuinely requires real hardware (audio, IME, touch), multi-app interaction (drag to Finder, global hotkey from another app), or timing-sensitive observation that none of the above can capture. When code-verifying, change `[HUMAN]` to `[x]` with a `_(verified: file:line explanation)_` annotation. When code reveals the description is wrong, change to `[ ]` with a `_(NOTE: ...)_` correction.

Rust test-suite mechanics (mutation testing, `cargo nextest --workspace` gotchas for the
vendored `patches/` crates, which 15 tests are skipped on purpose, timing-assertion
pitfalls): `src-tauri/AGENTS.md`. Nothing frontend-specific is split out separately —
`vitest`/Testing-Library conventions live alongside the component code they test.

## Test instance vs orchestrator instance — READ BEFORE TESTING

There are TWO running TUICommander instances; do not confuse them:

- **Orchestrator instance** — the one this agent is embedded in. The `tuicommander` MCP tools and `debug invoke_js` target THIS instance (Mission Control on `:14319`, app logs on `:9876`). It does **NOT** run your worktree build, so testing it proves nothing about your changes.
- **Test instance** — the worktree dev build you start with `make dev`. Test your changes against it **only via its HTTP API on `http://127.0.0.1:9877`**. MCP/`invoke_js` cannot reach it.

9877 endpoints (see `src-tauri/src/mcp_http/mod.rs`): `GET/POST /sessions`, `DELETE /sessions/{id}`, `POST /sessions/{id}/write`, `GET /sessions/{id}/output`, and terminal grid ops (all session-scoped) `POST /sessions/{id}/terminal/scroll {delta}`, `POST /sessions/{id}/terminal/scroll-to {line}` (absolute; `line`=top row, 0=oldest), `POST /sessions/{id}/terminal/scroll-to-offset {offset}` (coalesced display-offset jump; powers wheel + scrollbar-drag in browser mode), `GET /sessions/{id}/terminal/scroll-info`, `GET /sessions/{id}/terminal/lines?start&end`, `GET /sessions/{id}/terminal/row-text?row`, `POST /sessions/{id}/terminal/search-buffer {query}`. Create a throwaway session, exercise it, then `DELETE` it — never test against Boss's live sessions.

Canvas rendering (selection highlight, smooth-scroll visuals, cursor) is **not observable over HTTP** — those still need a visual check with Boss.

## Web-UI testing with agent-browser (browser mode, not Tauri)

You can exercise features through the **web UI** instead of the Tauri desktop app — every instance serves the full frontend at `http://localhost:<port>/` (`static_files.rs` `FRONTEND_DIST`). Loading it in a real browser is **browser mode** (`isTauri() === false`), the exact web/PWA path a remote client sees.

- **Port:** the primary/only instance serves on **:9876**; if that's free (no conflict), point the browser there. If the orchestrator already holds 9876, bring up a **second debug instance** — it auto-retries to **:9877** (the single-instance lock is `#[cfg(not(debug_assertions))]`, so a *debug* build can run a 2nd copy alongside the orchestrator). Note: `TUIC_PORT` is honored ONLY by the headless `tuic-remote` binary — the desktop `make dev` build **ignores** it and relies on the `9876→9877→9878` retry. So: use :9876 when unconflicted, else :9877.
- **Drive it with `agent-browser`**, always via the stealth wrapper (see global rules). `@ref` CDP clicks are trusted and work (open modals/panels/tabs); **JS-dispatched keydown is `isTrusted:false`** so app keyboard shortcuts (Cmd+P, etc.) are ignored — click UI, never synthesize keys. Use a persistent `--session <name>`, restart the browser periodically (snapshots/clicks degrade after many calls), and wrap each call in a `perl alarm` timeout (macOS has no `timeout`/`gtimeout`).
- **Isolation caveat:** a 2nd debug instance has isolated backend/sessions (its own PTY/agent state), but by default there is no config-dir split — debug and release builds read and write the exact same config directory and the same `config.json`/`repositories.json` (see `docs/backend/config.md`), protected by a cross-process file lock so concurrent saves don't clobber each other. A toggle flipped in the dev build IS visible to the installed app and vice versa. It still **shares the filesystem** with the orchestrator — never run repo-mutating tests against Boss's repos, and expect possible SQLite contention on `tunnel_audit.db` (same shared dir, not covered by the config lock). **Launch a verification/test instance with `TUIC_APP_INSTANCE=<id>`** (any lowercase DNS label) to get a real, code-enforced isolated config directory instead — `instances/<id>/`, entirely separate from Boss's production files (#763-d219; see `docs/backend/config.md`). Prefer this over the shared default whenever a test is going to add or remove repositories.
- **The Command Palette DOES render in web mode.** `App.tsx` mounts it unconditionally as `<CommandPalette actions={actionEntries()} browserMode={!isTauri()} />`; `browserMode` filters the action list down to the HTTP-safe subset in `BROWSER_ACTION_IDS`/`BROWSER_ACTION_PREFIXES` (`CommandPalette.tsx`), which **includes** `search-files` and `search-file-contents`. Only the *opening shortcut* is unavailable to `agent-browser`: a JS-dispatched Cmd+P is `isTrusted:false`, so click the UI instead — that is a test-harness limit, not a missing feature.
- **Desktop-only features do NOT render in web mode** — Command Palette is a partial exception (see above); the rest — IdeLauncher, Dictation, Global Hotkey, detach-panel windows, updater, native file drop, user-plugin install, MCP/hooks config — are fully gated. Built-in plugins DO load. See mdkb `web-mode-verification-2026-07-02` for the verified inventory before reporting a feature "missing".
- **If `agent-browser` isn't available in your environment** (confirmed missing in a 2026-08-27 session — no wrapper script found, and the `claude-in-chrome` skill requires an extension the user may decline), fall back to the **Playwright CLI**: `which playwright` resolves a global npm install (e.g. `/opt/homebrew/lib/node_modules/playwright` on macOS) even when the repo has no `playwright` devDependency. `require()` it by its full install path — `require('/opt/homebrew/lib/node_modules/playwright')` — from a throwaway Node script (scratchpad dir), since it won't resolve via bare `require('playwright')` outside that directory. Real CDP-dispatched `page.keyboard.press(...)` IS trusted (unlike JS-synthesized keydown), so app shortcuts like Cmd+, work fine for navigation. Pass `ignoreHTTPSErrors: true` on `newPage()` — the self-signed-HTTPS fallback 301-redirects plain `http://` to `https://` even on `127.0.0.1`, and Chromium otherwise refuses the untrusted cert before any page loads.
- **When `agent-browser`/`chromium-cli` are unavailable and raw Playwright is the fallback, apply the same click discipline `@ref` gives you for free.** `@ref` resolves against a live accessibility snapshot, so it can't land on the wrong element; a blind CSS/text-label selector against the whole page can, and has, twice: a bare text-match on a branch name landed on the **double-click-to-rename** hit area and opened a real "Rename Branch" dialog on a live worktree, and a bare `input:not([type])` selector matched a **real terminal's hidden keyboard-capture input** instead of a dialog's own field, typing an unrelated string into a live shell prompt (nothing was executed in either case — no Enter was sent — but both required cleanup). Before any fallback-Playwright click: take a snapshot/screenshot first, and scope every locator to a specific dialog/container element (e.g. its heading's ancestor), never a bare `input`, `button`, or text-match against the whole page.

## Branching

NEVER create branches autonomously — Boss works with multiple windows.


## Commits

When a commit resolves a **GitHub issue**, use a closing keyword so GitHub auto-closes it: `Fixes #N` / `Closes #N` / `Resolves #N` (anywhere in the message — `fix(scope): desc (closes #N)` in the subject is fine). A bare `(#N)` only *links* the issue, it does NOT close it. This repo pushes directly to `main` (the default branch), where closing keywords take effect on push — no PR merge required.

- Use the GitHub-issue keyword only for the commit that actually fixes it; reference-only commits keep `(#N)`.
- This is distinct from **mdkb story ids** (7-char hex like `#abc1234`): those follow the wiz convention — `(#abc1234)` for traceability, `(closes #abc1234)` on story completion — and are unrelated to GitHub issue auto-close.
- **Enforced by the `pre-push` hook** (`scripts/hooks/pre-push`, installed by `make hooks` / `make dev`): a push to `main` is blocked if a pushed commit references an **open** issue with a bare `#N` and no closing keyword. Reference-only pushes bypass with `git push --no-verify` (or `TUIC_SKIP_ISSUE_CHECK=1`). The hook skips silently when `gh` is missing/unauthenticated/offline — it never blocks on a verification failure.


## Architecture

All business logic in Rust. Frontend only renders and handles interaction — no data reshaping, computation, or process orchestration.


## TUIC Protocol Markers (ack / intent: / suggest:)

Top-level sessions only. Subagents (Task tool) and in-process teammates must NEVER emit `ack`, `intent:`, or `suggest:` — `suggest:` is the end-of-task signal, so a subagent emitting it flips the *parent* session to `completed` mid-work. See `docs/user-guide/ai-agents.md#tuic-protocol--output-markers` for the full protocol and `src-tauri/src/mcp_http/mcp_transport.rs`'s `build_mcp_instructions_for_mode` / `cc_agent_hint.suggested_prompt` for where this is enforced today.


## IPC / HTTP Parity

**Every Tauri IPC surface MUST have an HTTP/WS equivalent, and the two MUST stay consistent.** The desktop app talks over Tauri IPC; browser/PWA/remote clients talk over HTTP+SSE+WS. They are two transports for the *same* backend — never let them drift.

- A new `#[tauri::command]` (request/response) → add the matching axum route + a `COMMAND_TABLE` entry in `src/transport.ts`, with a mapping assertion in `src/__tests__/transport.test.ts`. If a command is deliberately desktop-only, add it to `INTENTIONALLY_UNMAPPED` (don't silently leave it unmapped).
- **A `COMMAND_TABLE` entry with no route is now a test failure, not a runtime 404.** The table is TypeScript and the router is Rust, so the gate is split: the Vitest half executes every mapper and snapshots the paths to `src-tauri/src/mcp_http/command_table_paths.txt`; the Rust half (`command_table_paths_all_hit_a_registered_route`) `PATCH`-probes each one against `build_router`. Adding an entry fails Vitest first — regenerate with `pnpm vitest run src/__tests__/transport.test.ts -u`, then the Rust half tells you whether the route exists. Do not hand-edit the generated file. Full mechanics in `docs/api/http-api.md` → "Route Parity Gate".
- A new push (`AppHandle.emit`, `Channel<T>`, or per-stream broadcast) → bridge it: low-frequency lifecycle/progress events go on `event_bus` → `/events` SSE (add arms to `event_wire.rs`'s `event_type_name`/`event_payload`); high-frequency token streams get a dedicated per-id WS (mirrors the PTY log-mode WS). Keep the desktop `emit` AND the bus/WS path in sync from ONE payload by calling `AppState::emit_dual(event)` — never build the two payloads separately, and never call `emit_pty_event`/`event_bus.send` directly for an event that has a desktop half. There is **no general** bus→window forwarder — `emit_dual` is a producer-side dual-emit, not a receiver-side bridge — except a narrow, explicit allowlist in `spawn_desktop_event_bridge` (`state.rs`) for the rare event whose only producer is the bus (`UpstreamStatusChanged`, `ScheduledJobCompleted`); do not widen that allowlist to a catch-all, since most events are already dual-emitted at their own call site and forwarding those too would double-deliver, and an imperative event (e.g. `watcher-fire`) must never be forwarded at all — a desktop window and an open browser tab receiving it simultaneously would both execute it.
- Request/response shapes (field names, casing, payload structure) MUST be identical across IPC and HTTP so the same frontend store code works unchanged on both transports.

**`SessionCreated` must be the first event a subscriber sees for a brand-new `session_id` — ordering, not just presence, matters.** Found 2026-09-21 while landing the desktop↔HTTP-transport-parity plan: all four session-creation call sites (`create_pty`, `create_pty_with_worktree`, `spawn_session_for_agent`, `register_pty_session`) called `assign_term_alias` BEFORE emitting `SessionCreated`. This was harmless while `assign_term_alias`/`record_term_alias` had no bus arm at all, but once it gained one (`AppEvent::TermAliasAssigned`, dual-emitted via `emit_dual`), a subscriber could see `term-alias-assigned` arrive before `session-created` for a session it had never heard of yet — three `mcp_transport.rs` tests that assumed `SessionCreated` is always first caught this the moment the full workspace suite could reach them. Fixed by moving `assign_term_alias` to run AFTER the `emit_session_created`/`SessionCreated` call at all four sites. **The general rule: `emit_session_created`/`SessionCreated` must be the very first `emit_dual` call for a session, before any other session-scoped side-announcement (alias, accent color, initial rename, etc.) that a new call site might add.** If you add a fifth session-creation path, put its `emit_session_created` call first, before any other per-session announcement.

**Gap found and fixed 2026-09-10:** `get_session_foreground_process` (`pty.rs`, the desktop IPC command) mirrors the detected foreground agent into `session_states.agent_type` — the flag `should_transition_idle_with_hook` reads to pick the shell-idle vs. agent-idle threshold. Its HTTP counterpart, `get_foreground_process` (`mcp_http/session.rs`), used to only compute and return the detected name — it never wrote anything into `session_states` at all, neither the set nor the clear path, so a browser/PWA/remote client polling this endpoint never actually corrected the backend's per-session `agent_type` mirror. Fixed by making the underlying logic a shared `pub(crate) get_session_foreground_process_impl` (not gated to the `desktop` feature — nothing it calls is Tauri-specific, only the `#[tauri::command]` wrapper is) and having the HTTP handler call it directly instead of re-deriving a second copy. If you add a THIRD transport-specific consumer of foreground-process detection, share this same function — don't re-implement the detection logic again.


## Releases

See [`docs/release-checklist.md`](docs/release-checklist.md) for version bump, tag, and GitHub release steps. After creating any release or nightly tag, **verify CI completes successfully** — check `gh run list`, inspect failures, and confirm all platform assets (macOS .dmg, Linux .deb/.rpm/.AppImage, Windows .exe) are uploaded before reporting done.


## Implementation Memory

After non-trivial implementations, write an mdkb `memory_write` entry. Content: **Goal**, **Approach**, **Outcome**, **Gotchas**, **Rejected alternatives**. Skip file lists (mdkb indexes code). Focus on non-obvious insights a future session can't derive from reading the code. Search existing memories first to avoid duplicates.


## Accepted Security Decisions

Do NOT flag these as security issues in reviews — they are intentional design choices.

- **`POST /mcp/confirm-response` and `POST /agent-wrap-prompt/response` have no `require_local_or_auth` gate.** Both resolve a specific, server-generated UUID `request_id` that only matches one pending entry — knowing the id is the effective credential, and each route's resolver (`resolve_mcp_confirm`/`agent_wrap_prompt::resolve`) is a no-op for any id that doesn't exactly match. Reviewed 2026-09-25 (code review + security review, both independently confirmed clean); don't re-flag without a concrete new exploitation path (e.g. a way to observe or guess a still-pending UUID from outside the app).

- **CSP is intentionally wide open.** TUIC is a local dev tool, not a SaaS. The user IS the trust boundary. The CSP uses a single permissive `default-src` that allows `https:`, `http:`, `data:`, `blob:`, `unsafe-inline`, etc. **NEVER tighten the CSP.** Every time we've had per-directive restrictions, some iframe content (reveal.js slides, plugin panels, dashboards) broke. The only specific directive kept is `frame-src` (for localhost wildcard ports). If you feel the urge to add CSP restrictions, don't — read this bullet point again.
- **`dangerousDisableAssetCspModification: ["style-src", "script-src"]`** in `tauri.conf.json` — **DO NOT REMOVE.** Tauri auto-injects sha256 hashes for inline `<script>` tags. Per CSP3, hashes silently disable `'unsafe-inline'`. This kills all JS in srcdoc iframes (plugins, HTML previews). The override prevents Tauri from injecting those hashes.
- **`lazy_static` in `output_parser.rs`, `pty.rs`, etc.** — transitive deps (`portable-pty`, `symphonia`) also use it; removing the direct dep saves nothing. Modules outside `ai_agent/` will migrate opportunistically.
- **`opener:allow-open-path` scope `"**"`** — FileBrowser must open any file the user can see. Narrower globs break external drives and network mounts.
- **Iframe sandbox = `allow-scripts allow-same-origin`** — ALL iframes MUST use this. NEVER use bare `sandbox=""` — it kills JavaScript.
- **Plugin capabilities do not isolate plugins from each other.** `plugin_id` is caller-supplied and plugins load into the same JS realm as the host, so any plugin can pass another plugin's id and inherit its grants. This is known, documented at the capability check in `plugins.rs`, at the `import()` in `pluginLoader.ts`, and in `docs/plugins.md`. A per-plugin token was considered and rejected — same-realm JS can read or proxy it, so it would be security theatre. Real isolation needs Worker/iframe + a host-created MessagePort; it is deferred, not overlooked. Do NOT propose the token.
- **The self-signed-HTTPS HTTP→HTTPS redirect trusts `X-Forwarded-Host` over `Host` with no allow-list** (`axum-server-dual-protocol`'s `UpgradeHttp`, activated via `upgrade_http`/`.set_upgrade()` in `mcp_http::start_server` whenever the self-signed cert — not Tailscale — is the active TLS source). This is a textbook open-redirect *pattern*, but doesn't clear a real bar for reporting it: there's no reverse proxy in front of this app, exploitation needs LAN access plus a non-simple cross-origin header no normal browser navigation ever sends, and no credentials leak on redirect (Basic Auth isn't auto-forwarded cross-origin). Known and accepted; do not re-flag it without a concrete new exploitation path.
- **`worktree.rs`'s `run_shell_capture` (Setup/Archive Script execution, incl. via `POST /worktrees/run-script`) has unbounded `stdout`/`stderr` capture** — a script that writes gigabytes grows process memory by that much before its timeout can fire. Accepted: the script is always either user-authored in Settings or reachable only through the same `require_local_or_auth`-gated caller as the rest of the MCP HTTP surface, so there's no untrusted party who can supply a script without already being able to run arbitrary code locally. See the fuller rationale in `run_shell_capture`'s doc comment. Do not add a truncation cap without a real OOM report.
- **The HTTP `read-external`/`read-editor-external` allow-list (`additional_readable_dirs`) is user-configurable and ships with `~/.claude/plans` enabled by default** — this deliberately widens the HTTP read-only surface beyond registered repository roots (a browser/remote/PWA client could previously only read a file inside a registered repo). It was added specifically so a Claude Code plan-file link an agent printed opens over HTTP with no setup. It is intentionally READ-only: `expand_readable_root`/`additional_readable_roots` (`mcp_http/fs_routes.rs`) are wired only into `read_external_file_http`/`read_editor_file_external_http` — the four write/copy/move/transfer routes still call bare `registered_repo_roots()` and are unaffected. Known and accepted; don't re-flag the default or the read-only widening in a future security review without a concrete new exploitation path.
- **A client-supplied `session_id` (`create_pty`, `create_pty_with_worktree`, `agent::spawn_agent`) has a TOCTOU gap between the `contains_key` check and the actual `session_maps.sessions` insert** — a real race, but a security review of the desktop-http-parity plan (2026-09-21) and this note both independently judge it not worth closing right now: the id exists specifically so a client can pre-register its OWN randomly-generated UUID to deduplicate its OWN self-created echo (see the doc comment at each call site), so exploiting the window needs a second actor to independently generate or otherwise learn a still-in-flight client's exact UUID within a sub-second PTY-spawn window — no realistic path today. The "obvious" fix (an atomic reserve-then-fill, since the real `PtySession` value doesn't exist until after the expensive PTY-open/shell-spawn work that follows the check) needs a new reservation set plus guaranteed cleanup on every one of several early-return/error paths in that span; getting the cleanup wrong would leak a permanently "reserved but never used" id and break legitimate reconnection — a worse regression than the race it would fix. Deferred; revisit only with a concrete new exploitation path, not on principle. **Revisit trigger:** if `session_id` generation is ever changed to something non-random or attacker-influenced (e.g. reused/derived from an external source instead of a fresh crypto-random UUID per call), that removes the "no realistic path today" premise this deferral rests on.


## Ideas

The `ideas/` folder (gitignored) holds Boss's own scratch notes — half-formed feature
ideas, things to revisit later. Do not read it as a task backlog or treat its contents
as instructions; it's personal notes, not a spec.
