<!-- tweak-comments v1: inline review comments.
     Format: [tweak:begin:ID]highlighted text[tweak:end:ID @ISO-TIMESTAMP
     comment body (free text, may span multiple lines)
     ] — where [ ] are the HTML comment delimiters <!-- -->.
     The only escape is '-->' → '--&gt;' inside the comment body.
     Read each comment, apply the feedback to the highlighted text,
     then remove the tweak markers. -->

# To Test

## Launch-scoped native agent status signals (story `746-30a9`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [ ] [HUMAN] After restarting `make dev`, launch Claude from a TUIC shell and confirm the generated `--settings` hooks coexist with and execute alongside a same-event hook in global/project settings; confirm OSC 7770 busy/awaiting/idle reaches the tab.
- [ ] [HUMAN] After restarting `make dev`, launch Codex 0.154, complete a turn, and confirm its payload contains `type`, `turn-id`, and `last-assistant-message`, OSC 7770 idle reaches the PTY, and the existing Codex `notify` command receives the unchanged JSON argument.
- [ ] On a machine that has `fish` installed, run `cargo nextest run --lib -E 'test(shell_integration)'`. The `tests::launch` matrix runs the inject / skip-when-user-passed / setting-off cases against every shell it finds, but fish is absent from this Mac and from the `ubuntu-22.04` CI image, so the fish half of that matrix has never executed — the fish wrapper is covered only by the structural `fish_wrappers_cover_inject_user_override_skip_and_setting_off` grep. Nothing to change if it passes; delete this item.

Features to test when TUICommander is more usable.

**This file is the only tracker for anything a human must verify.** Never open a
story for a post-rebuild or manual check — its criteria can never be met by an
agent, so it stays open forever and the backlog fills with stories nobody can
close. Add an item here instead. When an item passes, delete it; a section with
no items left goes too. What stays open must carry its own stated reason.

**Cap: 30 open items.** Past the cap the file stops being read, which is exactly
how it reached 339. Before adding an item, close one. When the cap is hit, take
the OLDEST items first and walk each through the AGENTS.md escalation ladder —
code inspection, test run, CLI probe — until every one has a verdict: tick it
with the `file:line` that proves it, correct the description if the code says
otherwise, or delete it with a stated reason. Anything left that is real
unfinished work is **a story, not a check** — open it and drop the item. An item
must never age past a rebuild without a verdict: an unread backlog costs more
than a missed check.

> **Where the restart gate sits (measured 2026-09-07).** The backend serving
> `:9876` is `src-tauri/target/debug/tuicommander`, PID 28512, started
> **2026-09-06 15:43:13**. The newest commit it can contain is `7d232d3e`
> (09-06 15:33, `perf(boot): gate the AI scheduler and knowledge flush`).
> **Every Rust change up to and including that commit is live** — which is the
> entire 08-xx backlog *and* the whole 09-04/09-05 wave, the ACP work included.
> Only these six are **not** loaded: `7d5f0f8a`, `631bad31`, `59e183b2`,
> `68429c0f`, `ecbda408`, `9473819c` — plus anything still uncommitted.
>
> The previous note recorded PID 12931 / 09-03 22:01:49 / `e4d7efd6`, and was
> inherited unchecked for three days after the app had already been restarted.
> That is the failure this paragraph exists to prevent, so it happened to the
> paragraph itself. **Re-measure it, never inherit it** — and re-measure it
> *first*, before triaging anything, because the gate decides which items are
> even answerable:
>
> ```sh
> ps -o lstart= -p "$(pgrep -x tuicommander | head -1)"   # -x, not -f
> git log --until='<that time>' -1
> ```
>
> `pgrep -f target/debug/tuicommander` is what the old recipe said and it does
> not work: the pattern also matches sibling `tuic-bridge` processes under the
> same path, so it returns several PIDs and `ps -p` chokes on the list. The
> binary's own mtime is useless either way — an agent may rebuild the file on
> disk without the running process restarting, which is the case right now
> (file built 09-07 00:22, process started 09-06 15:43).
>
> **A commit inside the gate is not the same as a fix that is live.** The gate
> answers "which commits does the running process contain". It says nothing
> about work that was never committed, and this tree currently carries **101
> uncommitted files** — 90 modified plus 11 untracked. A fix sitting in the
> working tree is in no build at all, whatever the gate says. The OSC 10/11/12
> section below was marked LIVE off a gate check alone and was wrong: Boss saw
> the exact garbage it claims to fix. **Check both** —
> `git merge-base --is-ancestor <commit> <gate>` for the commit, and
> `git diff HEAD --stat -- <file>` for whether it is committed at all.
>
> **This paragraph overrides every per-section "needs a `make dev` restart"
> label below.** There are 29 of them and they are all frozen at the moment
> someone typed them, so re-labelling each one just re-creates this problem at
> the next restart — the gate lives here, in one re-measured place, on purpose.
> As of 2026-09-07, subject to the uncommitted-work caveat above, only these are
> still gated by a *commit* boundary:
>
> | Still needs a restart | Why |
> |---|---|
> | "Opening a 23 MB JSON no longer freezes the editor" | `59e183b2`, `68429c0f`, `ecbda408` all land after the gate |
> | the per-tab agent resume item (issue #119) under "Still needs a human" | `9473819c`, after the gate |
> | the `scrollback_reflow` toggle and the headless `/claude/usage` check | still uncommitted |
>
> **Everything else labelled "needs a restart" is already live** — including the
> whole 09-04/09-05 wave. Test it now; do not wait for a rebuild.
> **Gate status as of the `wip`→`main` rebase (2026-09-04).** This branch was
> just rebased onto `origin/main`'s `1.7.6` tip, so every prior "gate satisfied"
> or "stale as of" note above is talking about a running binary that no longer
> corresponds to what's on disk. Re-verify anything Rust-touching against a
> fresh `make dev` restart before trusting a prior `[x]` — don't assume an
> older gate note still holds.
>
> **The frontend has no such gate.** In a debug build the HTTP server reads
> `dist/` from disk on every request (`static_files.rs:65-90`), not the
> `include_dir!` copy, so a browser client at `:9876` picks up any frontend change
> after a plain `pnpm build` + reload — no Rust rebuild, no restart. The desktop
> WebView gets the same change over Vite HMR. If a browser check of a frontend fix
> shows nothing, check `dist/index.html`'s mtime before blaming the code.

## The 2026-09-06 reset (story `664-94db`)

339 open items, 65 commits, 90 days, 12 things ever closed. Nearly all of it was
"after a `make dev` restart" checks whose restart had already happened, unrecorded
— so they were never re-run and never deleted. The 249 items predating 09-04 were
worked through the ladder and closed out. What is left is the 09-04/09-05 wave,
which the running binary genuinely does not contain, and a short tail of checks
that need a human body.

Evidence that closed the bulk, all measured against the running 09-03 binary:

- `GET :9876/sessions`, 15 live sessions: 14 classify (`claude` ×13, `codex` ×1),
  11 idle / 3 working, and **zero** reporting `shell_state: idle` while
  `agent_state: working`. That count was 11 of 14 before the `started_with_agent`
  window (`pty.rs:2321`) — it is the whole "agent sessions reach idle" section,
  measured rather than argued.
- `GET :9876/logs?limit=3000`: **zero** `config write refused` lines, and
  `save_checked` with its stamp guard is gone from the tree.
- `repositories.json`: 37 repos, 3 groups, 37 `repoOrder` entries, `P42` and `ego`
  both present, plain shape with no `{id, before, after}` envelope. The 08-21
  restore held.
- `npx vitest run`: 381 files, 5738 tests, all green.
- ~~`cargo nextest` could NOT run: an in-flight edit elsewhere in the tree leaves
  `pty.rs:23285` calling `OutputRingBuffer::snapshot`, which does not exist.~~
  **Resolved 2026-09-07 — this note is discharged, do not act on it.** The tree
  compiles: no caller of `OutputRingBuffer::snapshot` remains in `pty.rs`, and the
  full suite ran green — `cargo nextest run --lib` **4934 passed / 0 failed /
  14 skipped**, `vitest run` **5919 passed / 0 failed** across 392 files, plus
  `clippy --all-targets -- -D warnings` clean and
  `cargo build --bin tuic-remote --no-default-features` building. So the Rust
  claims below are no longer read-only inferences; the suite backs them.

## Idle watchers stop stalling the event loop (2026-09-05, **Rust change — needs `make dev` restart**) — story `674-78a8`

The idle classifier now runs in its own task, gated by the rule cooldown and a
4-permit lane. Covered by unit tests against a hanging local provider; what the
tests cannot show is behaviour under a real slow provider with several watchers
armed at once.

- [ ] Arm an Idle watcher on two busy agent sessions. While one is waiting on the
  classifier, the other session's Busy/Question/Error watchers must still fire —
  no lag, no `Watcher lagged N events` line in `GET :9876/logs`.
- [ ] Let a watcher fire, then trigger it again inside its cooldown. The logs must
  show `Watcher skipped — cooldown` and NO classifier call for that event.
- [ ] Fire a watcher until `max_fires`, restart the app, and confirm the rule comes
  back as `exhausted` in the Watcher Manager — the deferred write must land.

## A backend-created worktree offers itself as a toast, not a modal (2026-08-30, frontend only — HMR)

The "Switch to new worktree?" confirm was a blocking modal with a ten-second
auto-cancel, raised only by MCP/HTTP worktree creation — the one case with nobody
at the keyboard. It is a toast with a **Switch** button now, mirrored into the
bell so an unattended run leaves the offers waiting instead of discarding them.
Behaviour is covered by `worktreeSwitchPrompt.test.ts`; what tests cannot see is
how it renders and whether it interrupts anything.

- [ ] Have an MCP client call `repo worktree_create`. A toast appears with the
  repo badge, `Worktree "<branch>" created`, the `repo__wt/branch` subtitle and a
  **Switch** button. Nothing blocks, no dialog, no countdown, and typing in the
  focused terminal is uninterrupted.
- [ ] Ignore the toast until it fades, then open the bell: the
  `Worktree: <branch>` row under WORKTREES is clickable and switches to it.
- [ ] Create a worktree while a plain shell is the active tab, then click
  **Switch**: the tab moves to the new branch and `cd`s into the worktree.
- [ ] Repeat with a *running agent* as the active tab: the worktree opens in its
  own terminal and the agent's tab stays on its branch and CWD.
- [ ] **Rust change — needs `make dev` restart** (#728-bc76). `create_worktree`
  now returns `workspace_id`, and the frontend keys the new sidebar row by it.
  Against an unrestarted backend that field is `undefined`, so the row lands
  under the key `"undefined"`. After a restart: create a worktree from the "+"
  button and from `repo worktree_create`, and check the row appears under the
  branch, opens a terminal, and removes cleanly.
- [ ] **Rust change — needs `make dev` restart** (#727-2085). Both worktree
  events now carry `workspace_id` *and* `branch`, and creation goes through the
  new `notify_worktree_created`. On an unrestarted backend the frontend reads
  `workspace_id: undefined`, so the sidebar row lands under the key `"undefined"`
  and the prune drops nothing — the failure is silent. After a restart: MCP
  `repo worktree_create` returns a `workspace_id`, the row appears under the
  branch, and `repo worktree_remove` with that id removes both the directory and
  the row.

## An agent quoting a menu footer stops flagging itself as awaiting (2026-08-30, **Rust change — needs `make dev` restart**)

Observed live on Boss's own `tuicommander/main` tab, twice in one turn: the agent
read another session's screen, pasted it into its answer, and the menu footer
came back out inside its own indented output. `parse_question` matched it,
emitted `Question { confident: true }`, and no clear path retracts a confident
question — the tab read "awaiting" while the agent worked, until Boss typed. The
anchor is now matched at column 0 of the rendered row instead of the trimmed
text. Covered by `pty::tests::quoted_ink_footer_in_agent_output_raises_no_question`
(fixture `claude-quoted-ink-footer.tcap`, verified RED without the fix), but a
live agent-frame check cannot be replayed.

- [ ] After restarting `make dev`, ask an agent in a throwaway session to print a
  captured menu screen — footer row included — inside a fenced code block. Its tab
  must stay "working": no `?` in the sidebar, `awaiting_input` false in
  `GET /sessions`.
- [ ] In the same session, open a real interactive menu (any agent prompt that
  draws the selection footer) and confirm the `?` still appears. The regression to
  fear is the opposite one: an over-tight anchor that silences real menus for
  agents whose frame indents them.

## Atomic MCP managed-agent submission (2026-08-27, **Rust change — needs `make dev` restart**)

The running backend cannot expose the new `session action=submit` schema or
handler until it is rebuilt. Validation belongs to the Luna delivery pass; this
manual item covers only the rebuilt live Codex integration.

- [ ] After restarting `make dev`, create a throwaway managed Codex session,
  wait for a confirmed idle composer, and issue one MCP
  `session action=submit session_id=<id> input=/clear` call. The same call must
  return either `status=acknowledged` with terminal-movement evidence or a
  precise non-retryable timeout; it must not require a `status`/`output` poll,
  leave `/clear` in the composer, or run it twice. Close only the throwaway
  session after observing the result.
## DECCKM app-cursor keys, DECSCUSR cursor shape, and wide-glyph cursor width (2026-08-20)

- [ ] **[MANUAL]** In a real `zsh` prompt with `bindkey -v` (vi mode) and a non-empty prompt line, press Home/End and arrow keys: cursor moves without dropping into vi normal mode (visible via the block cursor NOT appearing after Home/End).
- [ ] **[MANUAL]** oh-my-zsh vi-mode plugin: switching insert/normal mode visibly changes the terminal's own rendered cursor between beam and block.
- [ ] **[MANUAL]** Run a full-width character (e.g. `echo 界` or a Nerd Font icon) under the cursor in a real pane: the block/underline cursor visibly covers both columns instead of only the leading half.
- [ ] **[MANUAL]** `\x1b[2 q` (steady block) in a live shell: cursor stops blinking and stays solid; `\x1b[1 q` (blink block) restores blinking.

## Background-color-erase reverse-video fix (2026-08-21/27, **Rust change — needs `make dev` restart**)

- [ ] **[VISUAL]** In a real terminal, run something that enters standout mode and prints a highlighted status line then scrolls (e.g. `tput smso; echo "status"; tput sgr0` in a loop that pushes it up through several more lines of plain output). Confirm the reverse bar stays scoped to the original highlighted line and does NOT reappear on later blank rows scrolled into view.
- [ ] **[VISUAL]** Separately, confirm `tput smso; tput el` (explicit erase-to-end-of-line under a reverse pen) STILL paints a highlighted bar to the edge — the original, still-intended behavior for explicit erases, which the row-recycling fix must not have regressed.

## Mouse-motion button reporting + local-selection hardening (2026-08-21)

- [ ] Manual click-through in a real Claude Code / Ink-based agent pane (the originally reported symptom): click-and-drag over agent output actually extends the app's own text selection instead of doing nothing, and double-click-drag / triple-click-drag extend by word/line respectively.
- [ ] Manual: Shift+drag (TUIC's local-selection fallback) started while an app has mouse tracking on, then the app toggles mouse tracking off/on mid-drag — selection must not flicker into forwarded-to-app mode or leave stray autoscroll/copy state.

## Scrollbar block/prompt marks respect their own settings independently (2026-08-22/25)

- [ ] **[MANUAL]** With a real terminal open and some command blocks/prompts recorded, flip "Show block marks" and "Show prompt marks" off/on in Settings > Terminal > Blocks with the timestamp overlay (Ctrl+Cmd) NOT held — ticks must appear/disappear immediately, not only while Ctrl+Cmd is held or on the next unrelated repaint.
- [ ] **[MANUAL]** Open a brand-new shell tab, run one short command (never scroll past one screen), confirm a block tick is visible on the scrollbar track immediately — the track must not be entirely absent as it was before this fix.

## Native drag out of the file browser survives a missing icon (2026-08-25, **Rust change — needs `make dev` restart**)

`drag::Image` has no "no image" variant, so an unresolvable `icons/drag-file.png`
was `ImageNotFound` and killed the whole drag session — a cosmetic asset was
load-bearing. The icon is now compiled in (`include_bytes!`) and used whenever the
resolved path is not a real file.

- [ ] Drag a file from the file browser onto a terminal tab, onto the editor pane,
  and out to Finder. All three must start a real OS drag.
- [ ] Confirm `GET http://localhost:9876/logs` shows no `Native drag failed` /
  `drag image not found` warning afterwards.
- [ ] Drag a file onto another folder **inside** the tree — the internal move must
  still work (it never went through the icon path, so this is a no-regression check).

## Config saves stop being refused, and a broken wide char stops ghosting (2026-08-25, **Rust change — needs `make dev` restart**)

Three backend fixes from the 5-day regression review. All three are invisible until
the running binary is replaced.

- [ ] `save_checked` and its stamp guard are gone: every whole-document save
  (`save_activity`, `save_ui_prefs`, `save_notes`, …) is now plain last-writer-wins.
  Exercise the app normally for an hour and confirm `GET http://localhost:9876/logs`
  shows **no** `config write refused` / stamp-mismatch lines — before the fix these
  dropped ~30 activity items in 2.5h while protecting nothing.
- [ ] Print a fullwidth char and overwrite half of it (`printf '\e[1;5H中'` then
  `printf '\e[1;6HX'` in a terminal tab). The leading half must not survive as a
  ghost `中` beside the `X` — the damaged span now covers both cells of the pair.
  Scroll away and back to confirm it is not just hidden by a later full-row reship.
- [ ] Answer the FIRST sub-question of a multi-question `AskUserQuestion` on a
  non-hook agent (grok/codex) and confirm the tab's hover text shows the real
  question, not the `⊠ … ✓ Submit` footer row.

## "Capture Session" in the tab context menu (2026-08-22, **Rust change — needs `make dev` restart**)

New `get_pty_capture` / `set_pty_capture` commands plus a **Capture Session** item in the
terminal tab context menu, gated on `isPerfDebug()`. Until the restart the frontend item is
live but the commands are missing from the running binary, so it will just toast an error.

- [ ] Right-click a terminal tab → **Capture Session**, reproduce something, right-click →
  **Stop Capture Session**, and confirm the toast reports a plausible KB count and the path.
- [ ] Confirm the file appears in `~/Library/Application Support/com.tuic.commander/captures/`
  named after that session, and that starting a second time truncates rather than appends.
- [ ] Flip the tap with `curl` and confirm the menu label follows on the next right-click
  (the store re-reads on open — it must not trust the last value this window wrote).
- [ ] Confirm the item is absent in a release build until `window.__TUIC__.setPerfDebug(true)`.

## grok 1.0.x detected as an agent again (2026-08-22, **Rust change — needs `make dev` restart**)

grok 1.0.5 installs `~/.grok/bin/grok` as a symlink to `grok-1.0.5`; `proc_pidpath` resolves
the link, so the foreground process reads `grok-1.0.5`, `classify_agent` returned `None`, and
the session got no `agent_type`. With no ready-screen adapter the OSC 133 busy bit set once by
the long-lived `grok` command was never cleared, so the tab stayed working for the whole
process. Verified live before the fix: `GET /sessions/<id>/foreground` → `{"agent":null}` while
`GET /sessions/<id>/has-foreground` → `{"process":"grok-1.0.5"}`. `classify_agent` now falls
back to the basename with a trailing `-<digits…>` suffix removed.

- [ ] Run `grok --minimal`, wait for a turn to finish, and confirm the tab goes idle
  (dot stops pulsing) instead of staying busy for the whole process.
- [ ] Confirm `GET http://localhost:9876/sessions` reports `agent_type: "grok"` for that session.
- [ ] Confirm boxed (non-minimal) grok still goes idle, and that a `cursor-agent` session is
  still classified as `cursor` (the suffix strip must not eat a hyphenated tool name).

## Peer identity survives the 1h MCP reaper (2026-08-24, **Rust change — needs `make dev` restart**)

`last_activity` only moves on an MCP request, so an agent on a turn longer than an hour that
calls no TUIC tool had its protocol session reaped — and the reaper deleted the peer identity
with it, while the PTY was still running. Children's handoffs then failed with
`Recipient '<uuid>' is not registered`; for a headerless parent that is permanent, because
re-registering mints a fresh UUID. Observed live on 2026-08-24 in the veritas `Blocker audit`
session ("Il parent TUICommander indicato non è più registrato"), with four
`MCP session reaped (idle ≥1h)` entries in the retained log window. The reaper now keeps an
identity that owns a live PTY or is still named as a live session's parent.

- [ ] Leave an agent on a long turn (>1h) with no TUIC tool call, then have a child `send` to
  it — the handoff must be accepted, not refused as unregistered.
- [ ] Check `GET http://localhost:9876/logs` for `kept addressable` after a reap, and confirm
  the retained ids still appear in `agent action=list_peers`.
- [ ] Confirm an identity with no PTY and no child still disappears after its hour (the
  retention must stay bounded) — `list_peers` should shrink over a long idle session.

## Agent sessions reach idle again (2026-08-23, **Rust change — needs `make dev` restart**)

`has_meaningful_descendant` called any descendant outside a three-name allowlist
(`mdkb | tuic-bridge | node_repl`) background work, and `background_work` outranks both
`completion_declared` and an idle shell in the agent-state ladder. Every agent has a daemon
outside that list — Codex 0.149.0 ships `codex-code-mode-host`, and an MCP server started via
`npm exec` reports as `npm`, a name that cannot be allowlisted without hiding real work.
Measured live before the fix: all 14 agent sessions reported `working`, including 11 with an
idle shell and no work on screen. A descendant that starts within 60s of its agent is now
plumbing regardless of name; simulating that rule over the same snapshot returned 11 sessions
to idle and kept the 3 genuinely busy ones working.

- [ ] Let a codex turn finish and confirm the tab goes idle instead of pulsing forever.
- [ ] Same for a claude session (the `npm exec @upstash/context7-mcp` case).
- [ ] Start a long command from an agent (`cargo build`), confirm the tab reads working while
  it runs and returns to idle when it exits — the window must not hide real work.
- [ ] Check `GET http://localhost:9876/sessions`: sessions with `shell_state: "idle"` must no
  longer report `agent_state: "working"` unless something real is running.
- [ ] Known residual (see the `DEFERRED (2026-08-23)` note at `started_with_agent`): a daemon
  that crashes and respawns mid-session escapes the window and pins that one session to
  working until it restarts. Note it if you see it; do not widen the window.

## Repository list restored after the version-skew wipe (2026-08-21, **Rust change — needs `make dev` restart**) — story `637-c311`

The live backend (started 08-20 16:23) predates the `mutationVersion 1` delta contract, so
the hot-reloaded frontend sent delta envelopes that the stale backend wrote to disk as the
whole `repositories.json`. The repo list went to `repos: []`. A replacement was rebuilt
from the pre-migration copy (`~/Library/Application Support/tuicommander/repositories.json`,
08-08, 35 repos) plus `ego` recovered out of the delta's own `after` payload and a minimal
`P42` entry: 37 repos, 3 groups, `repoOrder` complete, active repo `tuicommander`. Both
files live in the config dir as `repositories.restore-2026-08-21.json` and
`repositories.broken-2026-08-21.json`.

**Writing the restore while the app runs does not hold** — it was installed at 22:02 and
the stale backend had overwritten it with an envelope again by 22:03. The restore must go
in while the process is down:

```
# quit TUICommander first, then:
cd ~/Library/Application\ Support/com.tuic.commander
cp repositories.restore-2026-08-21.json repositories.json
# then: make dev
```


- [ ] After the restart, confirm the sidebar lists all 37 repos with the 3 groups
  (`Progetti`, `IOS`, third) in their old order, and that `ego` still shows its 3 branches.
- [ ] Confirm repos added between 08-08 and today — other than `ego` and `P42`, which were
  recovered — are genuinely absent, and re-add by hand whatever is missing. That gap is not
  recoverable from any file on disk.
- [ ] Add, rename, group and remove a repo, then confirm `repositories.json` still holds
  the plain `repos`/`repoOrder` shape and **never** an `{id, before, after}` envelope.
- [ ] Confirm `P42` (rebuilt by hand, not from a backup) shows the right branch, display
  name and terminals once opened.

## Repository mutation persistence under the lock (2026-08-21, **Rust change — needs `make dev` restart**) — was story `642-eabd`

The delta-under-lock implementation in `config.rs` / `mcp_http/config_routes.rs` cannot be
exercised until the backend is rebuilt. Post-restart checks only, so it lives here and not
in a story.

- [ ] Confirm the restarted backend accepts a `mutationVersion 1` repository delta over
  both the IPC and the HTTP surfaces.
- [ ] Confirm a stale same-repository mutation is reported as a visible conflict and does
  not overwrite the newer value.

## UTF-16-safe content search offsets (2026-08-21, **Rust change — needs `make dev` restart**) — was story `643-26ac`

`fs.rs` now returns UTF-16 offsets for content-search matches, and
`FileBrowserPanel.tsx` highlights with them. Post-restart checks only.

- [ ] Confirm desktop content-search batches highlight the match after an em dash, an
  accented character and an emoji.
- [ ] Confirm the HTTP content-search response returns the same UTF-16 offsets and that
  ASCII highlighting is unchanged.

## MCP bridge writes into third-party configs (2026-08-21, **Rust change — needs `make dev` restart**) — issue #115

JSON MCP configs are now edited member-by-member through a syntax tree instead of being
reserialized, Zed/Amp/Gemini wait for an explicit install, and Settings → Agents gained
**Remove all MCP integrations**. The unit tests cover the splice, the refusals and the
gate; what they cannot cover is a real client reading the file afterwards.

- [ ] **[MANUAL]** Put a comment, a trailing comma and hand-tuned indentation in a real `~/.config/zed/settings.json`, install the bridge from Settings → Agents, then confirm Zed still starts, still shows every setting, and lists the `tuicommander` context server. `diff` the file against `<config dir>/mcp-backups/zed-settings.json.orig` — the only change must be the added member.
- [ ] **[MANUAL]** Launch TUICommander with Zed installed but no bridge entry, and confirm `~/.config/zed/settings.json` is **not** touched (mtime unchanged) and the panel reads "Not installed automatically".
- [ ] **[MANUAL]** Press **Remove all MCP integrations**, then confirm every listed client's config lost only the `tuicommander` entry and that a relaunch does not put them back.

## Render cadence of AI answers and the phone terminal (2026-08-17, frontend only) — story `603-c28f`

F10/F130/F136/F137 from the performance audit. The chat panel renders the live answer on
its own 200 ms cadence instead of once per token batch, and the mobile terminal reuses the
elements of screen rows that did not change. Vite reloads this without a restart.

- [ ] **[MANUAL]** Open a session on a phone or at `:9877` in a narrow browser window, run something with a busy full-screen redraw (`htop`, a TUI agent), and confirm the output stays smooth and the search filter reacts instantly while output flows.

## OSC 133 and OSC 7 cwd in browser mode (2026-08-18, **Rust change — needs `make dev` restart**) — story `623-d369`

Both markers reached the desktop `AppHandle` only, so a browser/PWA client had no command
blocks, no gutter marks, no Cmd+Up/Down navigation and a cwd frozen at session start. They
are dual-emitted now and carried on the `?format=grid` WS the canvas already holds. Nothing
below works against the currently running binary.

- [ ] **After a `make dev` restart**: confirm desktop still works unchanged — the desktop emit was kept, not replaced.
- [ ] **[HUMAN]** Compare the gutter marks side by side, browser vs desktop, on the same session. Canvas painting is not observable over HTTP, so only a visual check proves the marks land on the same rows.

> **Why the browser-mode block checks above are still open (2026-08-20).** An
> automated browser can drive the UI, but macOS/Chrome suspends
> `requestAnimationFrame` while the window is occluded — `visibilityState` still
> reads `visible` and `hasFocus()` still reads `true`, so nothing announces it.
> Command blocks flush through `_scheduleOsc133Flush` (rAF, `terminals.ts:226`)
> and the editor's jump-to-line runs inside a rAF (`CodeEditorTab.tsx:274`), so
> both silently do nothing. Any "it did not render" conclusion from an occluded
> automated window is worthless — raise the window first, then judge.

## Desktop PTY activity pulse (2026-08-17, **Rust change — needs `make dev` restart**) — story `625-56b0`

`cda39f31` deleted the `pty-output` emit and left the listener, so desktop lost every
activity signal for a commit. A payload-free `pty-activity-{id}` pulse replaces it,
throttled to ~1/s and dual-emitted so browser and desktop read the same signal.
Nothing below works against the currently running binary.

- [ ] **After a `make dev` restart**: open the Activity Dashboard, run `for i in $(seq 1 20); do echo $i; sleep 1; done` in a terminal, and confirm the `lastDataAt` column keeps advancing while it runs — it froze completely before. _(2026-08-20, signal proven at the source: with that exact loop running in a throwaway session, `GET /events` carried **exactly 10 `pty-activity` frames for that session id in 10 s** — continuous during the command, at the ~1/s throttle. The `lastDataAt` column is a render of this signal; the column itself was not read because the Activity Dashboard has no reachable trigger in browser mode)_
- [ ] **After a `make dev` restart**: with tab A focused, start long output in background tab B and confirm B raises its unread-activity dot *while output is still flowing*, not only when the command completes. This is the case grid frames cannot report, since the canvas stops acking them while hidden.
- [ ] **After a `make dev` restart**: open the same session in browser mode (`:9877`) and desktop side by side; both must light up their activity indicator on the same output, since both now read one backend signal. _(2026-08-20: the shared signal is proven — `pty-activity` rides `/events` for browser clients (`sse_routes.rs:258`) alongside the desktop `pty-activity-{id}` emit (`pty.rs:369`), and the SSE frames were observed live. What is left is the two indicators lighting up side by side, which needs both UIs visible)_

## Duplicate and orphan event listeners (2026-08-17, **Rust change — needs `make dev` restart**) — story `600-d664`

F4/F5/F7/F11/F17 from the performance audit. Only `CanvasTerminal` listens for OSC 133 now;
content-search batches and errors carry a `search_id`; the dead `pty-vt-log-total` emit is gone;
the AI chat panel no longer subscribes to the producerless chat registry; improvement-scan
proposals are published only by the `proposals-ready` event.

- [ ] **Residual after the browser Command Palette defect is fixed**: run concurrent File Browser and command-palette searches in separate windows and confirm search-id isolation, spinner completion, and detached-window isolation. HTTP content-search transport and independent request surfaces are verified.
- [ ] **After a `make dev` restart**: detach the File Browser into its own window and search in both it and the main window. The two ids are minted at random per search now, not from a per-realm counter that both windows would start at 1, so neither window may see the other's matches.
- [ ] **After a `make dev` restart** — the OSC 133 subscription moved ahead of the canvas font load, so the very first prompt marker of a session is no longer racing it: open a brand-new shell tab and confirm the first command already has a block (the first prompt used to be the one at risk of being dropped once the second listener was gone). _(2026-08-20, attempted, inconclusive — **not a failure, an isolation problem.** Created a fresh session, confirmed its shell emits OSC 133 (4 `133;` markers in the raw output for the first command) and that `pty-cwd` resolved its cwd to `/private/tmp`. But the browser client logged no `[OSC133] … flushed N blocks` line (`terminals.ts:251-254`, `appLogger.debug`, which does reach the browser console) for the new session **or** for an established one during the same window — the flush only runs for a session whose `CanvasTerminal` is mounted (`CanvasTerminal.tsx:2068-2076`), and I could not get the new tab mounted: clicking its tab element did not move the active terminal, and no active-tab signal is exposed on `window.__TUIC__`. Earlier in this same sweep the same browser client did log per-command flushes and painted 13 marks for 13 commands, so the subscription works in general; what stayed untested is specifically the **first** command of a **fresh** tab. Needs a way to activate a tab from automation, or a human click)_
- [ ] **After a `make dev` restart**: open a saved conversation from the AI chat history and confirm its messages stay on screen (they used to blank a moment after loading). _(2026-08-20: **not reachable from a browser — see defect 6 below.** `conversationStore.listAllConversations` returns `[]` behind `if (!isTauri())` (`conversationStore.ts:996`), so the history view opens empty however many conversations exist; `loadConversation` (`:1007`) bails the same way. Confirmed live: the panel's history button leaves the panel text unchanged at 182 chars with no list and no error, while `curl localhost:9876/ai/chat/conversations` returns 200 with real saved conversations. Needs the desktop app, or defect 6 fixed)_
- [ ] **After a `make dev` restart**: run an improvement scan and confirm the proposals appear exactly once, and appear in a second window too.

## Content index freshness after a timestamp-preserving restore (2026-08-17, **Rust change — needs `make dev` restart**)

`ContentIndex::is_current` compared modification times only, so a restore that preserves them left
the index reporting itself current with stale content. The stat fingerprint is now mtime **and**
size, both already read by the same walk.

- [ ] **Confirmed runtime residual**: after warming a content index and waiting past its rebuild cooldown, `cp -p` a different-sized file over an indexed one must rebuild the live index and make the replacement phrase searchable. The disposable runtime probe returned neither unique phrase and logged no rebuild; the preserved-mtime unit test passes.

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

- [ ] **After a `make dev` restart**: `claude-wakeup` still fires on a real `/done` line, and `at-capacity-retry` still fires on a real capacity line — both now matched in Rust. _(2026-08-20: **`claude-wakeup` is not installed** — `/plugins/list` returns `rtk-dashboard`, `wiz-kanban`, `csv-preview`, `build-cleaner`, `mdkb-dashboard`, `tuic-vscode-icons`, `at-capacity-retry`. So half this item has nothing to fire; its watcher (`plugins/claude-wakeup/main.js:385-397`, pattern `/done/i`) is also gated on a recent wake having been sent, so it would not fire on a bare `/done` line anyway. `at-capacity-retry` is installed but loads only on the desktop client, which `agent-browser` cannot reach)_
- [ ] **After a `make dev` restart**: a rare line printed once, with the PTY then completely quiet, still reaches a watcher within ~100 ms (the ticker drains the tail; it no longer waits for more output). _(2026-08-20, attempted and blocked — **no OutputWatcher exists on a browser client to receive the line.** Probe: held `/events` open, echoed a line containing `done` into a quiet throwaway session, and got 18 `pty-activity`, 29 `pty-parsed`, 3 `pty-osc133` and 1 `pty-cwd` in 10 s but **zero** `plugin-watcher-lines`. That is correct behaviour, not a transport failure: `emit_watcher_lines` (`pty.rs:406-412`) returns early when no line matched a registered watcher, and the SSE bridge itself is present (`sse_routes.rs:261,346`). See the item below for why nothing is registered)_
- [ ] **After a `make dev` restart**: open the web UI alongside the desktop app and confirm a watcher fires in the browser tab, and that neither client blinds the other. _(2026-08-20: **a browser client registers no OutputWatcher at all**, so this is not reachable through the UI. `window.__TUIC__.plugins()` on the live browser client returned exactly two loaded plugins — `plan` and `stories-ticker` — and neither registers one; outside the host itself (`pluginRegistry.ts:293`) and the type declaration, no `registerOutputWatcher` call exists anywhere in `src/`. The watchers this section is about ship as **user** plugins, which do not load in browser mode: `/plugins/list` reports 7 installed (`rtk-dashboard`, `wiz-kanban`, `csv-preview`, `build-cleaner`, `mdkb-dashboard`, `tuic-vscode-icons`, `at-capacity-retry`) and none of them appeared in the browser client. Testing this needs a user plugin loading in web mode)_
- [ ] **After a `make dev` restart**: reload the browser tab ten times (each reload leaves a client id behind, and the bound is 8), then confirm a watcher still fires in the desktop window within 30 s — the heartbeat has to re-install the set eviction dropped. _(2026-08-20: the reload half is drivable from a browser, but the assertion is about the **desktop** window firing a watcher, and `agent-browser` cannot reach the Tauri webview. Also blocked upstream by the two items above — the browser client registers no watcher set, so ten reloads leave no watcher-set client ids to evict)_

## Terminal copy gutter normalization (2026-08-17, **Rust change — needs `make dev` restart**) — story `622-6c69`

Terminal selection extraction now removes Claude's repeated `NBSP NBSP ▎` visual
gutter only from coherent multi-line runs. It continues to join soft-wrapped rows
and preserves literal block characters, indentation, bullets, numbering, emoji
shortcodes, and non-breaking spaces inside the message.

- [ ] **After a `make dev` restart**: copy the original long Claude message and paste it into Slack; no `▎` gutter or gutter NBSPs remain, while lists, blank lines, indentation, `:wave:`, `:pray:`, and the body spacing in `QA  Engineering` are unchanged. _(2026-08-21 reattempt: the fixture was staged successfully in throwaway session `audit-luna-20260821-copy` and rendered in the persistent browser; trusted `mouse move/down/up` drag plus `Cmd+C` completed, but `agent-browser clipboard read` again hung with `Resource temporarily unavailable (os error 35)`, and `pbpaste` still contained unrelated pre-existing clipboard text. The paste payload therefore remains unproven; needs a human drag/clipboard target or a backend endpoint returning current selection text)_
- [ ] **After a `make dev` restart**: copy a lone `▎` and an ASCII-indented `  ▎` code/table line; both paste unchanged. _(2026-08-21 reattempt: the lone-bar and indented-bar fixture rows were rendered in throwaway session `audit-luna-20260821-copy`; trusted drag/Cmd+C reached the canvas, but clipboard read remained unavailable (`Resource temporarily unavailable`, with unrelated `pbpaste` content), so unchanged paste is still unproven)_

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

## Scroll acceleration cap + SGR-report input corruption (2026-08-20)

- [x] `gestureAccelFactor` stays within `[0.5, 2.0]` and reaches exactly `1.0` at two screens of cumulative travel _(verified: `src/components/Terminal/__tests__/canvasTerminalScroll.test.ts` "gestureAccelFactor" suite; `cargo`/`vitest` n/a here, this is the frontend suite — 141/141 pass)_
- [x] A direction reversal restarts the acceleration ramp from `|dy|` instead of continuing to accelerate off distance traveled the other way _(verified: `canvasTerminalScroll.test.ts` "accumulates same-signed gesture distance and restarts the ramp on reversal", and the `snap()`/`cancel()` sign-reset case)_
- [x] An SGR mouse report (`ESC [ < Cb ; Cx ; Cy M`, wheel notch or mouse-move motion report) fed into `InputLineBuffer` no longer splices its digits into the reconstructed line _(verified: `src-tauri/src/input_line_buffer.rs` `test_sgr_mouse_report_does_not_leak_into_content` — feeds real notch/motion-report bytes mid-line and asserts the reconstructed content is unaffected; 122/122 `input_line_buffer` tests, 472/472 `pty`, 12/12 `mcp_http::session` tests pass; `cargo clippy`/`cargo fmt --check` clean)_
- [ ] **Needs a `make build`/dev-restart to observe live** (Rust doesn't hot-reload): after rebuilding, run a long session in Claude Code with mouse tracking on (default) and use the wheel/mouse heavily, including while a subprocess pager (`less`, `top`) is displayed; confirm `last_prompt`/submitted text for that session contains no `NN;NN;NNM`/`m` fragments (previously visible via `GET /sessions` on the debug HTTP port).
- [ ] A long momentum fling over deep scrollback (`seq 1 100000 | less` main-buffer scrollback, or a Claude Code pane without mouse tracking) no longer visibly outruns a shorter fling — the acceleration ceiling should be reachable but not exceeded, felt as "fast, but not runaway."

## Mouse wheel quantization in mouse-aware apps (2026-08-20)

- [ ] Flick the wheel over a Claude Code pane: moves a few lines, not a page; scrolling back to a specific earlier message lands on it accurately.
- [ ] Slow drag over the same pane: nothing moves until a full line of travel, then exactly one line, with no 1px jitter; a mid-scroll reversal responds within one notch.
- [ ] Discrete mouse (USB wheel or Magic Mouse) click ≈ 3 lines.
- [ ] Plain shell pane scrolling feels unchanged from before this fix (it was already on the scrollback path).
- [ ] Shift+wheel, held for the whole gesture, scrolls TUIC's own scrollback (not the app) in Claude Code, `vim`, `lazygit`, and `htop` — this previously did nothing on macOS until Shift was released.
- [ ] `vim` (`:set mouse=a`) still scrolls the file on wheel without the cursor jumping; `lazygit` scrolls only the hovered panel; `htop` scrolls the process list — no regression from the alt-screen forwarding path.
- [ ] `grok --no-alt-screen` still scrolls its own conversation on wheel (the `e92200f0` inline-mouse-mode behavior the guard must preserve).
- [ ] Ctrl+wheel in an app that binds it: the modifier bit survives the coalesced multi-notch write.
- [ ] Scrollbar thumb drag, then wheel immediately after releasing: no smooth-scroll re-entry or repaint freeze.
- [ ] Blur the pane mid-flick, refocus, then wheel once: no stray leftover notch fires.

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
- [ ] Ollama selected + running: ~~model list populated from `/api/tags`~~ **residual:** render the required green availability dot. _(Browser runtime populated the local model list; no green dot was rendered.)_
- [ ] Ollama selected + not running: red dot with "Not detected" message _(NOTE: `detect_ollama` returns `{available:false, models:[]}`, but `ProvidersTab.tsx` does not render a red dot or `Not detected` message.)_
- [ ] Context lines slider: 50-500, persists across restart _(NOTE: no context_lines slider exists in the codebase. AI Chat tab only has temperature slider and scheduled tasks. Feature not implemented.)_
- [ ] Status bar: ~~chat bubble icon toggles the AI Chat panel~~ **residual:** decide and visually validate an active highlight for status-bar panel toggles. _(Toggle behavior was verified live; no active-highlight class exists and applying it consistently needs a style decision.)_

## AI Chat — Detachable Panel (1388-9bda)
- [ ] Detached window receives streaming chunks from active conversation _(NOTE: `AIChatPanel.tsx:39-42` explicitly documents that detached-window stores are separate and streaming/controls are not fully synchronized; generic panel projection sync is not registered for the AI Chat adapter in `App.tsx:124-132`.)_
- [ ] Closing detached window emits `ai-chat-window-closed` event _(NOTE: the generic bridge listens for `panel-window-closed`, but no AI-specific `ai-chat-window-closed` emission was found; current contract is `panel-window-closed` in `useDetachedPanelBridge.ts:12-16`.)_
- [ ] Send message from main window → stream visible in detached window _(NOTE: detached AI Chat has separate stores and the adapter has no `serialize`/`syncIntervalMs` projection; source comment at `AIChatPanel.tsx:39-42` identifies this as unresolved.)_
- [ ] Close detached window mid-stream → main panel resumes with partial text _(NOTE: no cross-window AI conversation projection is registered; the generic panel close bridge restores UI state but does not transfer streaming text.)_
- [ ] Switch terminals in main window while detached → subscription updates chatId _(NOTE: the AI Chat adapter passes the initial `chatId` only; no AI-specific panel-action handler or projection sync was found in `App.tsx:124-132`.)_

## AI Agent — Level 2 Loop (1299/1300/1301/1302)
- [ ] Rejoining session after reload: ~~chat message text reloads from the conversation store~~ **residual:** persist and recover tool-call history, `agentState`, and `currentIteration`; the conversation store is schema v1, while schema v2 belongs to session knowledge.

## Smart Prompts Drawer (Cmd+Shift+K)
- [ ] Auto-execute ON → prompt sends Enter automatically after injection _(2026-08-20: **the flag is unreachable for any prompt a user can create — see defect 4 below.** Only `useSmartPrompts.executeInject` reads it (`useSmartPrompts.ts:240`) and only when `injectTarget === "terminal"`; the drawer's own `doInject` (`PromptDrawer.tsx:158-177`) never reads it at all. Live half of the test: created a temp prompt, made a throwaway shell session active, opened the toolbar dropdown — the row rendered `itemDisabled` with title "No agent detected in terminal", 14 of 15 rows likewise. Finishing this needs an agent session, and the only agent sessions running are Boss's live ones, which must not be injected into. The temp prompt was deleted; the library is back to its original 31 entries)_
- [ ] Auto-execute OFF → prompt text pasted without Enter, user can edit before sending _(2026-08-20: same blocker as the item above. The OFF branch is `pty.write` without Enter at `useSmartPrompts.ts:247-248`, and is also what a `compose` target falls back to when no compose panel exists)_

## Plan Panel (515-660c / 516-41a5 / 517-74c2)
> **OBSOLETE (panel removed 2026-04-02, commit `123f7a2c` "refactor(plan): remove HTML panel"; sidebar panel also dropped, `1634e0b1`; stale doc refs cleaned in `331bd649`).** The plan feature is now plugin-only (`planPlugin.ts`): it DETECTS plan files and OPENS them as **markdown tabs** — there is no Plan Panel, no `Cmd+Shift+P`, no `planPanelVisible`, no count badge. Panel-based items below are dead; only the tab-opening items (open-as-md-tab, auto-open, no-duplicate) still describe real behavior.
- [ ] Switching repos rescans plans for the new active repo (no panel — affects which plans auto-open as tabs) _(NOTE: `planPlugin.ts:103-106` scans only during plugin load, and exported `scanPlans()` at `:264-266` has no caller elsewhere in `src`; an active-repo switch rescan is not currently demonstrated)_

## Voice Dictation (Stories 117-123)
### Model Management
- [ ] Model status shows "Ready" after download completes _(NOTE: `DictationSettings.tsx:29-35` renders `Downloaded`/`Active`; no `Ready` status is currently implemented.)_

## Smart Prompts API Mode
- [ ] Select provider (OpenAI/Anthropic/etc.) → model placeholder updates _(browser verified: the Add Model form keeps the static placeholder `e.g. claude-sonnet-4-5-20241022`; current `ProvidersTab.tsx` has no dynamic provider-specific placeholder.)_
- [ ] No API key configured → canExecute returns error with Settings link _(NOTE: `useSmartPrompts.ts:109-125` returns the plain reason `Headless provider not configured — add a provider and assign the Headless slot in Settings → Providers`; no clickable Settings link is produced.)_
- [ ] PWA/browser → API mode shows "requires desktop app" message _(NOTE: browser transport maps `execute_api_prompt` through HTTP, and no `requires desktop app` guard/message exists in the API execution path.)_
- [ ] [HUMAN] Wrong API key → toast shows "Authentication failed" with Settings hint _(2026-08-21 Luna audit: code and `error_mapping_auth_keyword` verify the 401 mapping at `src-tauri/src/llm_api.rs:141-143,231-236`, and `src/utils/promptContext.ts:109-115` routes a failed result to a toast. The safe HTTP probe of `/prompt/execute-api` returned 400 for missing content; no bad-key request was sent because it could use live credentials/external API. The rendered toast and Settings hint remain unverified.)_

## ChoicePrompt (story 1296-ce3e)
- [ ] Agent resumes work (status-line emits) → `choice_prompt` cleared, overlay disappears _(NOTE: current test `test_session_state_status_line_keeps_choice_prompt` explicitly preserves the prompt during status-line repaint; the checklist expectation does not match current behavior.)_
- [ ] Codex numbered-choice dialog (if/when encountered) captured by parser — add fixture if not _(NOTE: `output_parser.rs:93-95,1754-1782` documents and implements the shared numbered-choice shape for Codex, but no Codex screen capture/fixture exists in the current corpus; do not invent one.)_
- [ ] Aider confirmation dialog — add fixture if layout differs _(NOTE: the parser documents the same cross-agent layout, but no Aider capture/fixture exists in the current corpus; a live Aider prompt is required before adding evidence.)_

## Command Block System (2026-05-20)
- [ ] [HUMAN] Cmd+F with block-scoped toggle ON → only matches within current block shown _(filter logic and targeted tests pass; residual is a trusted browser SearchBar/CodeMirror interaction on a mounted editor.)_
- [x] Settings > Terminal > Blocks → toggle timestamps and folding on/off _(verified: TerminalTab.tsx:395-429 renders all four toggles (show_block_timestamps, show_block_marks, show_prompt_marks, block_folding_enabled) wired to settingsStore setters, exercised by src/__tests__/components/SettingsPanel/TerminalTab.test.tsx. The stale NOTE above predated commit 7582d468, which added this UI — the runtime modifier/shortcut paths now live at CanvasTerminal.tsx:3295-3338, not the old 2106-2110/2165-2183 citation.)_
- [ ] [HUMAN] Run 500+ commands → ~~no crash~~ **residual:** prove frontend oldest-block eviction, bounded count, and no memory growth. _(HTTP generated 510 OSC 133 blocks and the browser rendered output through `cmd-510` without a crash; store tests pass.)_
- [ ] [HUMAN] Claude Code session: tool calls show as blocks without OSC 7770 (heuristic detection) _(2026-08-21 Luna audit: the targeted Rust parser tests passed; detection is implemented at `src-tauri/src/pty.rs:3341-3368,4993-5012`. HTTP rendered a Claude-like `⏺ Read(foo.txt)` screen in `luna-legacy-20260821-heuristic`, but its proven `agent_type` was `null` (plain shell), not Claude, and no live agent session was used.)_

## Process Monitor
- [ ] Panel: changing refresh interval to Manual stops auto-polling _(NOTE: live `/process/monitor` HTML has no refresh-interval selector or Manual mode; it only auto-refreshes on the fixed 3-second timer.)_
- [ ] Panel: Refresh button triggers immediate data fetch _(NOTE: live `/process/monitor` HTML has no Refresh button; only the fixed timer is implemented.)_

## Search/UI consistency: unified SearchBar + scrollbar overview + file-browser tracking (2026-06-11)
- [ ] [VISUAL] Live (needs rebuild): open Cmd+F in the code editor → compact SearchBar pill (counter inside input); typing shows orange full-width ticks covering the scrollbar and hides the green git ticks; closing search brings the git overview back. Replace row expands via the chevron. _(2026-08-21 Luna audit: `SearchBar.test.tsx`, `editorSearchEngine.test.ts`, and `searchOverview.ts:59-128` passed/inspected; the active throwaway browser panel had no CodeMirror or SearchBar, so no trusted editor interaction was attempted.)_
- [ ] [VISUAL] Live (needs rebuild): open a brand-new (untracked) file → the git-change overview shows a SINGLE tick at the top, not a solid green bar. _(2026-08-21 Luna audit: `gitGutterRuns.test.ts` passed and `src/components/CodeEditorPanel/gitGutter.ts:48-71` collapses contiguous additions; creating an untracked file or mutating the dirty worktree was out of scope, and no editor panel was active.)_
- [ ] [VISUAL] Live (needs rebuild): search inside a diff tab → orange match ticks appear on the diff scrollbar and track scroll. _(2026-08-21 Luna audit: shared SearchBar/search-overview code and targeted SearchBar/editor tests passed; no diff editor was active in the browser DOM, so trusted query/scroll observation was skipped.)_
- [ ] [VISUAL] Live (needs rebuild): editor scrollbar visually matches the terminal's (14px track, rounded inset thumb). _(2026-08-21 Luna audit: source inspection confirms the shared scrollbar rule in `src/components/CodeEditorPanel/theme.ts:22-23` and 14px search ruler in `searchOverview.ts:103-123`; no editor canvas was active for a visual comparison.)_
- [ ] [VISUAL] Live (needs rebuild): open a file deep in a subtree → the file browser (tree view) auto-expands its parents and scrolls it into view, highlighted with the accent bar. Switching the active editor tab moves the highlight. _(2026-08-21 Luna audit: `src/components/FileBrowserPanel/FileBrowserPanel.tsx:260-289` implements ancestor expansion/scroll and targeted FileBrowser tests passed; the browser had no safe throwaway repo/editor target, so no click was made.)_
- [ ] [VISUAL] Live (needs rebuild, story 443-ea2b): install/enable the `docx-preview` plugin, open a `.docx` from File Browser, and confirm it opens the Mammoth HTML preview panel with conversion notes/raw-text toggle; Edit opens the same file in CodeMirror. Requires backend restart because `host.readFileBase64()` is Rust-backed. _(2026-08-21 Luna audit: `plugins/docx-preview/main.js:8-64` and plugin-host tests were inspected; `GET /plugins/list` did not list `docx-preview`, and installing/opening a DOCX would require config/repo mutation plus the Rust restart explicitly called out here.)_
- [ ] DEFERRED (story 041-cd15): HTML Preview search → shared SearchBar (in-iframe search needs a postMessage bridge); shared sidebar-filter component for Error Log / Knowledge History / Branch Switcher etc. ("consistency of a different kind" for narrow sidebars).
- [ ] [VISUAL] Live (story 040-29e1): open a tracked file in the code editor → a dim italic annotation "Author · relative time · summary" appears at the end of the active line and follows the cursor (no flicker, no fetch per keystroke). Edit a line → it shows "You · Uncommitted changes". Toggle `settingsStore.setInlineBlameEnabled(false)` → annotation disappears. External (absolute-path) files show no annotation. _(2026-08-21 Luna audit: `inlineBlame.test.ts` passed and `CodeEditorTab.tsx:453-488` confirms reactive enable/fetch behavior; no tracked editor was activated, and editing the dirty worktree was intentionally skipped.)_

### Legacy-tag Luna pass — 2026-08-21

All 11 raw legacy-tag lines in this section were audited through code inspection, targeted tests, safe HTTP probes, and a new stealth browser session. None is complete: visual/editor behavior was not observable on the active throwaway terminal canvas, and no live session received input. The macOS capture ladder step was attempted read-only but showed an existing live session, so further macOS interaction was skipped. Throwaway IDs `luna-legacy-20260821-blocks` and `luna-legacy-20260821-heuristic` were the only sessions written, and cleanup was completed after this pass.

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

- [ ] Command palette, `?OPENROUTER` with **Search all repos** on: matches appear from repos you have NOT opened this session. _(2026-08-20, measured against the shipped backend: they do NOT. `/fs/search-content-all?query=OPENROUTER` returned matches from `tuicommander` only, with `repos_searched: 1` and `repos_pending: 40`, unchanged across four polls over 80 s. This is the documented design, not a regression — `fs.rs:613-616` says outright that "the configured warm strategy owns build scheduling: one cross-repo query must not enqueue every registered repo behind the single global build semaphore". With the default `active_and_switch` strategy an unopened repo has no index, so it stays unsearchable. The criterion as written is only satisfiable with the `all` strategy)_
- [ ] Repeat the search a few seconds later: the pending count drops and matches appear, proving `ensure_index` was kicked off by the first search rather than the repo staying invisible forever. _(2026-08-20: **the pending count does not drop.** Four searches over 80 s all reported `repos_pending: 40`, and no index build appeared in the logs. `search_content_all_impl` (`fs.rs:656-671`) counts a missing index and moves on — it never calls `ensure_index`. The criterion describes an intent the implementation deliberately rejected)_

**Consequence worth a decision (2026-08-20):** the two points above make the
`N still indexing, retry shortly` wording misleading. Nothing is indexing, and
retrying never helps under the default strategy. Either the search must warm the
missing indices (what this checklist assumed) or the message must stop promising
progress that will not happen.

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

## Smart Prompts settings tab + HelpPanel system-menu note (2026-08-05, frontend only — Vite HMR is enough)

`SmartPromptsTab` existed but was never registered in `SettingsPanel`, so the drawer's
"Manage Smart Prompts..." landed on General. Now it is a nav entry (`smart-prompts`) and the
drawer opens it directly. Separately, HelpPanel gained the missing note pointing at the native
system menu bar (desktop only — browser mode has no native menu).

- [ ] Visual pass: ~~Smart Prompts category groups/counts and mode/placement/built-in badges~~ **residual:** inspect the expanded editor and desktop HelpPanel note spacing under Quick Actions. _(Browser screenshot `/tmp/luna-smart-prompts.png` proves the groups/counts/badges; browser Help has no native system-menu note.)_

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

- [ ] **[HUMAN]** Browser/PWA transport and tracked cwd are verified; **residual:** prove canvas OSC 133 gutter painting and trusted Cmd+Up/Down navigation on a visible browser client. _(HTTP captured OSC133 A/C/D, including exit codes 1/0, and cwd changes.)
- [ ] **[HUMAN]** Desktop still gets both events after the payload change: `pty-cwd` now carries `{cwd}` instead of a bare string. _(code confirms the desktop emits at `pty.rs:4777-4787` and `:4806-4811` with bus emits following; no desktop runtime observation is possible from a second instance)_
- [ ] **[HUMAN]** Frame bytes/s before and after on a busy session, confirming the ~3x drop the binary format predicts. _(post-fix side measured: 596 720 B over 16 frames. The "before" is unobtainable — no pre-fix binary exists any more, and a post-fix process cannot yield an honest baseline. Either accept the post-fix number alone or rebuild a pre-fix commit deliberately)_
- [ ] **[HUMAN]** A background tab still rings its bell, and switching back repaints immediately with no stale rows. _(needs audible delivery and row freshness by eye; hidden frames are decoded and ring at `CanvasTerminal.tsx:1358-1393`, show requests a full repaint at `:2121-2158`)_
- [ ] **[HUMAN]** A browser client and the desktop app streaming the same session both keep painting — neither steals the other's dirty rows. _(needs the desktop WebView alongside a browser; per-client gate rationale at `grid_gate.rs:1-21`, WS recovery at `mcp_http/session.rs:1481-1492`)_
- [ ] **[HUMAN]** A plain file save logs `Emit repo-changed (working-tree)` and the Git panel's Log/History/Stashes do NOT re-run their git processes; a `git commit` logs `(git-state)` and they DO refresh. _(requires mutating a real repo. Tabs depend on `getGitRevision` — `LogTab.tsx:207-224`, `HistoryTab.tsx:88-99`, `StashesTab.tsx:57-69`; `bumpGitRevision` moves both counters at `repositories.ts:779-795`)_
- [ ] **[HUMAN]** FileBrowser backend create/rename/copy/delete and read/write are verified over HTTP; **residual:** open and save a file through the FileBrowser/editor UI. _(Disposable repo HTTP operations passed; no live repo was mutated.)_
- [ ] **[HUMAN]** Dropping a large folder still freezes the UI. **This confirms a deliberate gap, it is not a regression** — `fs_transfer_paths` was intentionally left synchronous because it is the drag-drop backend and D&D needs Boss's approval. _(the gap is intact and documented at `fs.rs:1615-1629`; the native Finder→Tauri drop cannot be driven from browser mode)_
- [ ] **[HUMAN]** AI agent `read_file` / `write_file` / `edit_file` / `list_files` / `search_files` / `search_code` still work on the blocking pool. _(all six mapped in `ai_agent/tools.rs:2460-2477`, dispatched through `spawn_blocking` at `:2525-2534`; write/edit need the desktop confirmation UI)_
- [ ] **[HUMAN]** Terminal search and scrolled-back history backend reads are verified over HTTP; **residual:** trusted Cmd+F next/previous, file links, OSC 8 hover, and selection copy. _(Grid reads/search/scroll endpoints passed; UI bindings and clipboard remain unproven.)_
- [ ] **[HUMAN]** With a session producing heavy output, dragging a selection or typing in the search box no longer stalls the WebView. _(this is the freeze the finding is about; offload mechanism at `pty.rs:10167-10227`. Needs a visible live canvas and timed main-thread observation)_
- [ ] **[HUMAN]** HTTP `scroll-to`/offset behavior is verified; **residual:** trusted wheel, scrollbar drag, Cmd+Up/Down, and Home/End input routes. _(HTTP `terminal/scroll-to` returned 200 with `display_offset=120`; UI routes remain unobserved.)_

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

### Story `632-8d67` — clicking an MCP toast jumps to its terminal

Backend change, so it needs a rebuilt binary: `make dev` does not hot-reload
`src-tauri/**`. Everything else in the story is verified by test.

- [ ] **[HUMAN]** From a rebuilt desktop app, have an agent call `ui action=toast` and click the toast — it must switch to that agent's tab. _(verified by test at the seams: the Rust bus event carries `origin_session_id` and the click focuses the matching terminal, `src/__tests__/components/ToastContainer.test.tsx`. What no test covers is the real IPC round trip through a live `AppHandle`, plus that the tab is genuinely the one on screen afterwards)_

### Story `631-e618` — browser mode renders (closed as an artefact)

Browser mode was verified working: a real headed browser at `:9877` sized every
canvas to its container and painted a live PTY. One comparison was not possible.

- [ ] **[VISUAL]** In browser mode, with a shell that has OSC 133 integration, check the gutter marks render at the same size as the desktop app. _(the probe shell had no shell integration, so there were no gutter marks on screen to compare. Rows, cursor, status bar and cwd all render — screenshot `/tmp/631-browser-mode.png`)_

### Story `627-4571` — restart checks the HTTP surface cannot reach

The 2026-08-18 Rust sweep is committed and the app has been restarted on it (binary
built 14:05, commits 10:35). Everything reachable over HTTP was verified and checked
off in the story: the `repo-changed` kind split, the binary frame format, the async
fs commands, the async grid reads (`scroll-info`, `lines`, `row-text`,
`search-buffer`, and a deleted session answering 404 rather than 500), and every
scroll mutation landing exactly where asked — `scroll` delta, absolute `scroll-to`,
and the coalesced `scroll-to-offset` (offset 100/200/0 → `riga-278`/`riga-178`/
`riga-378`, `display_offset` tracking each one). Read that offset back after a
beat: the endpoint coalesces, so an immediate read still reports the old position
and looks like a no-op.

What is left is canvas painting and input handling, which no endpoint exposes.

- [ ] **[VISUAL]** Frame bytes/s on a busy session, against the ~3x drop the binary format predicts. _(the pre-change baseline is gone with the old binary, so this is now a sanity check on the absolute number, not a before/after)_
- [ ] **[VISUAL]** A background tab still rings its bell, and switching back to it repaints immediately with no stale rows.
- [ ] **[VISUAL]** A browser client at `:9877` and the desktop app streaming the same session both keep painting — neither steals the other's dirty rows.
- [ ] **[MANUAL]** FileBrowser backend operations are verified over HTTP; **residual:** editor open/save and the visual FileBrowser interaction. _(All disposable HTTP create/read/write/search/rename/copy/delete operations passed.)_
- [ ] **[MANUAL]** AI agent `read_file` / `write_file` / `edit_file` / `list_files` / `search_files` / `search_code` still work — they now run on the blocking pool.
- [ ] **[VISUAL]** Terminal backend search/grid reads are verified asynchronously over HTTP; **residual:** Cmd+F next/previous, file-link opening, OSC 8 hover, and selection copy UI.
- [ ] **[VISUAL]** With a session producing heavy output, dragging a text selection or typing in the search box no longer stalls the WebView — the freeze F95 is about.
- [ ] **[MANUAL]** Dropping a large folder still freezes the UI. _(`fs_transfer_paths` was deliberately NOT converted — this confirms the known gap, it is not a regression. Same residual as F120 above)_
- [ ] **[MANUAL]** Desktop still gets both events after the payload change: `pty-cwd` now carries `{cwd}` instead of a bare string.

## Defects found during the 2026-08-20 browser sweep

### 1. A `null` answer is an error on the HTTP transport (browser/PWA only)

`transport.ts:2233-2235` throws `RPC <cmd>: empty response body` whenever the
decoded body is `null`. But `null` is a legitimate answer: `/agent/discover-session`
returns HTTP 200 with the body `null` when no session file matches, mirroring the
Tauri command's `Option<String>` → `None`.

Live evidence: the browser console on Boss's running instance carries
`[AgentDetect] term-2/3/4/6/10/15 discover_agent_session failed —
RPC discover_agent_session: empty response body`, once per terminal per poll.
Confirmed by hand: `POST /agent/discover-session` → `200`, `content-length: 4`, body `null`.

Consequence: **agent session discovery never succeeds for a browser/PWA client**,
so resume-after-restart cannot work there, and every 30 s poll logs a failure.
Every other command whose valid answer is `null` has the same fate.

- [x] Fix: distinguish "no body" from "body is `null`" in the transport, then confirm
      the AgentDetect errors stop and a browser client discovers a Claude session id. _(verified: focused transport tests and isolated :9877 POST `/agent/discover-session` returned literal `null`; a matching Codex fixture was discovered by both curl and browser fetch.)_

### 2. Two clients fight over `repositories.json`

With the desktop app and a browser client both attached, the browser's persistence
fails: `RPC save_repositories failed: 500 {"error":"config file changed on disk since
it was last read"}`. The guard is doing its job — the whole-object save would
otherwise clobber the desktop's write — but the frontend swallows the rejection as a
`debug` log, so the user's change is silently dropped rather than retried or merged.

- [ ] Decide: retry-on-conflict (reload, re-apply, save) or surface the failure. Silent
      loss is the one option that is certainly wrong. _(NOTE: Luna proved the delta protocol, queueing, visible error path, and Rust conflict behavior in tests, but the rebuilt-runtime proof is blocked: the current shared `repositories.json` was previously written as a delta by the stale Rust backend, and Sol's compatibility repair is awaiting root authorization. No shared config was repaired.)_

### 3. Content-search highlight offsets are byte offsets fed to a UTF-16 slice

`fs.rs:1007-1011` takes the match offsets from `matcher.find(line.as_bytes())`, which
are **byte** offsets, and `FileBrowserPanel.tsx:1359-1363` feeds them to
`line_text.slice()`, which counts **UTF-16 code units**. Every multi-byte character
before the match shifts the highlight by the difference.

Reproduced live: `src-tauri/src/llm_api.rs:21` contains an em-dash before the match,
and the rendered highlight was `enrouter, ` instead of `openrouter` — exactly the
2-unit drift a 3-byte `—` produces. Line 15 of the same file, pure ASCII, highlighted
correctly.

- [x] Fix: return char/UTF-16 offsets from the Rust side (or slice by bytes in the frontend), then re-check a line with an em-dash, an accented word and an emoji before the match. _(verified: Rust offset tests, FileBrowser desktop/browser rendering tests, and isolated :9877 HTTP results for ASCII/accent/em-dash/emoji returned UTF-16 ranges.)_

### 4. `autoExecute` is unreachable for every user-created prompt

The drawer's prompt editor renders an **Auto-execute** checkbox and persists it
(`PromptDrawer.tsx:447,652,759`), but no path a user can reach ever reads it:

| Link in the chain | Where | What breaks |
|---|---|---|
| The drawer's own injection ignores the flag | `PromptDrawer.tsx:158-177` | `doInject` keys off `executeImmediately` (double-click / "Insert & Run"), never `prompt.autoExecute` |
| The one reader requires `injectTarget === "terminal"` | `useSmartPrompts.ts:236-249` | default is `"compose"`, so the flag is skipped |
| `injectTarget` is editable only in Settings → Smart Prompts | `SmartPromptsTab.tsx:394-398` | — |
| …which lists only prompts tagged `smart` | `SmartPromptsTab.tsx:141` | the drawer's editor never sets `tags`, so drawer-created prompts are invisible there |

Verified live on `:9876`: the Settings → Smart Prompts tab listed only the built-in
groups (Git 4, Review 6, Pr 3, Merge 3, Ci 5, Investigation 5, Code 3 — 29 built-ins).
A prompt created through the drawer did not appear, and neither do Boss's own
`X - Hook` and `superGoal`. So a user can tick Auto-execute, save it, and it can
never take effect.

- [x] Fix: pick one owner for the flag. Either have `PromptDrawer.doInject` honour
      `autoExecute` the way `executeInject` does, or tag drawer-created prompts
      `smart` so `injectTarget` becomes editable. Leaving a checkbox that does
      nothing is the one option that is certainly wrong. _(verified: focused PromptDrawer/useSmartPrompts/usePty/sendCommand tests cover enabled/disabled user-created prompts, precedence, fallback, and exactly-once submission.)_

### 5. The Command Palette button exists only where the palette cannot open

`Toolbar.tsx:763-778` renders a "Command palette (⌘P)" button **as the browser-mode
fallback** — its own comment says it is there because "browser-desktop has no native
menu and keyboard shortcuts may be swallowed by the browser". But `App.tsx:986-989`
mounts `<CommandPalette>` behind `<Show when={isTauri()}>`, so in browser mode the
component never exists. The button therefore appears in exactly the one mode where
clicking it can do nothing: `commandPaletteStore.toggle()` flips state that nothing
renders.

Verified live on `:9876`: the button is present in the browser toolbar; clicking it
leaves zero palette elements in the DOM.

This is also the precise reason the cross-repo `?OPENROUTER` palette items in the
`#483-7b93` section cannot be exercised from a browser.

- [x] Fix: either mount the palette in browser mode (dropping the Tauri-only actions
      from the list) or drop the fallback button. Shipping a dead control in the mode
      that was supposed to need it most is the one option that is certainly wrong. _(verified: browser component/Toolbar tests, trusted toolbar click on :9877 with focused dialog, HTTP filename/content searches, foreign-batch rejection, and `/tmp/validate-six-command-palette-current.png`.)_

### 6. AI chat persistence is switched off in browser mode, though its HTTP routes exist

`conversationStore.ts` guards its whole persistence layer with `if (!isTauri()) return`
and reaches the backend by importing `@tauri-apps/api/core` directly, bypassing the
`transport.ts` wrapper that would map the call to HTTP. Six functions are affected:

| Function | Line | Effect in a browser |
|---|---|---|
| `deleteConversation` | 377 | old conversation never deleted |
| `schedulePersist` | 394 | no debounced autosave |
| `persistNow` | 405 | no save at all |
| `initFromDisk` | 451 | nothing restored on load |
| `listAllConversations` | 996 | returns `[]` — history list always empty |
| `loadConversation` | 1007 | a history entry can never be opened |

The backend is not the problem — `mod.rs:1045-1057` serves the full family
(`/ai/chat/conversations` GET, `/ai/chat/conversation` GET+POST,
`/ai/chat/conversation/delete` POST, `/ai/chat/new-id` POST). Verified live:
`curl localhost:9876/ai/chat/conversations` returns 200 with real saved
conversations, while the browser panel's history button opens an empty view
(panel text stays 182 chars, no list, no error logged — the guard returns before
any call is made).

This is the IPC/HTTP parity rule in AGENTS.md being broken in the frontend rather
than the backend: a browser/PWA user's AI chat keeps no history whatsoever.

- [x] Fix: route these six through `invoke` from `transport.ts` and drop the
      `isTauri()` guards, so the existing routes are actually used. _(verified: browser persistence suite covers all six operations, stale/error behavior, and no direct Tauri invoke; :9877 browser-created fixture survived reload, opened from history, and was deleted via the UI, with a subsequent 500 not-found response.)_

## Luna validation audit — 2026-08-21

All unchecked checklist entries were reviewed and reconciled against the strongest
available evidence. Items remain open when their full stated behavior still needs a
desktop WebView, a rebuilt/release app, real hardware, an external service, a real
agent, a destructive or repository-mutating action, or a visual/manual observation.
Existing evidence notes and defect candidates above remain authoritative; no source
fixes were made during this audit.

Validation evidence:

- `rtk cargo nextest run --manifest-path src-tauri/Cargo.toml --no-fail-fast`: 4,641 tests passed.
- `rtk cargo test --manifest-path src-tauri/Cargo.toml --doc`: 0 passed, 1 ignored.
- `rtk pnpm vitest run`: 355 files and 5,386 tests passed; the run still reports one async timer leak in `ToastContainer.test.tsx` via `activityStore.ts:30`.
- The targeted Luna queue tests passed: `enqueue_never_overtakes_a_command_already_waiting` and `clear_queued_commands_preserves_peer_deliveries`.
- Live `:9876` HTTP probes used uniquely named throwaway sessions, covering PTY output/activity, SSE events, terminal lines/row text, search-buffer, scroll/scroll-to/scroll-to-offset, and deleted-session `404` behavior. The final `audit-luna-20260821-copy` fixture session was deleted and verified absent.
- Persistent stealth-browser session `tuic-test` exercised Smart Prompts navigation/settings, Help, AI Chat history, and the Command Palette toolbar button. Smart Prompts evidence is `/tmp/luna-smart-prompts.png`; the browser copy-selection retry is recorded in the copy items above.
- The deployed docs site loaded through the same persistent browser; Pagefind query `terminal` returned 61 results. At 375×667, the responsive layout still clipped the content/search hero, so that item remains open with screenshots recorded above.

The remaining `[HUMAN]`, `[VISUAL]`, and `[MANUAL]` entries are therefore intentional gaps, not unattempted assumptions. In particular, the six live defects documented above remain open until their fixes are implemented and revalidated.

### Tagged-item second pass — 2026-08-21

This second pass supersedes the stale mobile/docs wording in the preceding audit
paragraph. It covers every tagged item still open at the start of this pass; the
three items promoted above are the only tagged items closed by this run.

- **32 — promoted.** A headed browser run against `:9876` streamed an answer of
  about 17k DOM characters and rendered one fenced code block plus one table;
  the AI panel and terminal remained responsive. Evidence:
  `/tmp/luna-ai-chat-long.png`, `/tmp/luna-ai-chat-long-final.png`.
- **33 — remains open.** At 375×667 the terminal canvas sized to 342×580 and a
  live `FLOW-` search updated while output streamed; this does not prove the
  requested full-screen redraw (`htop`/TUI agent) smoothness.
- **46, 471, 514 — remain open.** Browser-side terminal rendering and HTTP
  transport evidence exist, but no safe desktop-WebView side-by-side visual
  comparison proved gutter mark placement and size.
- **215–217, 455 — remain open.** Release-app key-repeat/Option composition,
  explicit macOS defaults precedence, and native attention audio require the
  rebuilt release app and/or real hardware/hearing; no such interaction was
  claimed.
- **310–311 — remain open.** `tweakComments` and `tweakDomHighlight` targeted
  tests passed (54 tests), but the repeated-occurrence anchor and overlap UI
  still need a visual browser/editor check.
- **380–383 — remain open.** `amp`, `cursor-agent`, `goose`, and `droid` are
  not installed (`command -v` returned no path), so their idle/mid-turn adapter
  screens cannot be captured.
- **394 — remains open.** Vim 9.1 was opened visually in a throwaway session,
  but the wheel/Shift+wheel forwarding and quit/scrollback sequence was not
  completed: the browser wheel automation hung and was terminated safely.
  `lazygit` is not installed; no live session was reused.
- **402–403 — remain open.** `tuic --version`/`tuic --help` worked (1.7.4),
  but the two commands were not run because they would create a real TUIC
  session or dev process outside the disposable HTTP-session scope.
- **413–414 — promoted.** The deployed docs site returned 61 Pagefind results
  for `terminal`; at 375×667 the hero search and menu-bar search panel were
  usable after closing the contents drawer, with no horizontal page overflow.
  Evidence: `/tmp/luna-docs-desktop.png`,
  `/tmp/luna-docs-mobile-closed-audit.png`,
  `/tmp/luna-docs-mobile-search-panel.png`.
- **472 — remains open.** `/events` delivered live `pty-cwd` payloads while a
  throwaway session changed from `/private/tmp` to `/`; the desktop AppHandle
  counterpart was not observable from this browser instance.
- **473 — remains open.** The post-fix stream was measurable, but no honest
  pre-fix binary baseline remains for the requested before/after bytes-per-
  second comparison.
- **474–475, 507, 532–533, 539 — remain open.** Background-tab bell/repaint,
  browser-plus-desktop painting, native toast focus, and desktop event delivery
  require trusted desktop WebView/audio observation.
- **476 — remains open.** A disposable Git repo was mutated only through the
  HTTP FS API; no Git-panel revision split was exercised against a UI repo.
- **477, 534 — remain open.** HTTP create/read/write/search/rename/copy/delete
  operations passed against a disposable repo, but FileBrowser UI and editor
  open/save were not promoted from backend evidence.
- **478, 499, 538 — remain open.** Native Finder folder drag/drop was not
  attempted; the documented synchronous residual therefore remains neither
  reproduced nor disproved.
- **479, 535 — remain open.** No real AI-agent tool call was run; source and
  blocking-pool routing are evidence only.
- **480, 482, 536 — remain open.** Browser search, scrolled history, and
  Cmd+Up/Down were exercised; Home/End did not move history in this shell, and
  file-link/OSC8 hover plus clipboard proof remain incomplete. The trusted
  selection-copy probe selected `COPY_OK`, but paste into both a data-page and a
  local HTTP textarea remained blank; no clipboard-read API was used.
- **481, 537 — remain open.** A streaming-output search remained visually
  responsive, but no sufficiently heavy-output timed WebView-freeze measurement
  was obtained.
- **490–492 — remain open.** The normalization Rust tests passed and selection
  highlight was visible, but the trusted paste target stayed empty, so the
  end-to-end clipboard contract is not promoted.
- **500 — remains open.** No real repository churn was generated; the idle
  suppression reading cannot stand in for the requested storm measurement.
- **531 — remains open.** Same missing pre-fix bytes-per-second baseline as 473.

Audit incident: after a browser re-render, one Vim close attempt targeted the
live `Filter design` session instead of the throwaway tab. HTTP inspection found
an empty input buffer, no shell command or repository mutation, and the session
was left running; all further app UI automation was stopped. This is recorded as
an automation-targeting incident, not a product result.

## Rust changes staged 2026-08-21 — require a `make dev` restart

Neither is live in the running app; the Rust backend does not hot-reload.

### Awaiting badge through a multi-question `AskUserQuestion`

Observed on the live `Wire format` Claude tab: a multi-question dialog was on
screen waiting on Boss and the tab read "working". Sub-question 1 badges the
tab, answering it clears the badge, and sub-question 2 repaints its title and
options while the `Enter to select` footer stays byte-identical — so the
changed-rows parser never fires again. Fixed by `rearm_awaiting_for_open_dialog`
(`pty.rs`), which reads the footer off the full screen as a presence level.

- [ ] Open a multi-question AskUserQuestion (several sub-questions + Submit) and
  answer them one at a time. The tab must stay on the awaiting badge for EVERY
  sub-question, and drop it only once the dialog closes.
- [ ] Confirm no notification storm: one awaiting notification per dialog, not
  one per repaint or per arrow keypress.

### MCP elicitation drives awaiting (needs the hooks re-enabled)

`claude_hook_map()` gained `Elicitation` → awaiting and `ElicitationResult` →
busy, so an MCP server's `elicitation/create` dialog ("MCP server X requests your
input", Accept/Decline) badges the tab. Unverified against the running Claude
binary — the doc is the only source that these events fire.

- [ ] Re-enable Claude hooks in Settings → Agents (the map changed, so the badge
  reads "Hooks: re-enable"). Note: `~/.claude/settings.json` currently carries NO
  TUIC hooks at all, so nothing is instrumented today.
- [ ] Trigger the Context7 sign-in elicitation. Tab must go awaiting, and clear
  on Accept/Decline.
- [ ] Record it with `/diagnostics/capture` so the `.tcap` becomes a fixture.

### Every MCP `initialize` is logged

`initialize_session_id` now reports how a client arrived — `fresh`, `resumed` or
`reconnected` — and the handshake is logged at info with `source=mcp_initialize`.
A `reconnected` record carries the stale id in `presented_session`.

- [ ] After the restart, `curl 'http://localhost:9876/logs?source=mcp_initialize'`
  must show one record per agent handshake.
- [ ] Kill and relaunch an agent tab: its re-handshake must log `reconnected`
  with the previous session id, not `fresh`.

### grok is detectable again (`grok-1.0.5` symlink)

grok 1.0.5 installs `~/.grok/bin/grok` as a symlink to `grok-1.0.5`, and
`proc_pidpath` resolves the link, so the foreground process reads `grok-1.0.5`.
The running binary (built 22 Aug) matches agent names exactly, so every grok tab
gets `agent_type = None`. Measured live on 24 Aug: both an installed grok tab and
a fresh probe reported no `agent_state`, and
`get_session_foreground_process` returned `null`.

Consequence while undetected: `session_is_agent` is false, so nothing can be
typed into grok's composer — no peer message, no orchestrator mail wake — and
the OSC 133 busy bit set once by the long-lived `grok` command is never cleared,
so the tab reads working forever. The screen adapter that would fix both is
already shipped; classification never reaches it.

`strip_version_suffix` (uncommitted) closes it. Needs a `make dev` restart.

- [ ] Run `grok` in a tab. `session action=list` must show `agent_type`-derived
  `agent_state`, not an absent field.
- [ ] Finish a turn: the tab must leave working and reach `idle`, then
  `completed` once grok emits its `suggest:` marker.
- [ ] `agent action=send` to that grok tab while it is idle: `delivery_path`
  must be a terminal/wake route, not `inbox_only`, and the line must appear in
  grok's composer.
- [ ] Confirm `cursor-agent` and other hyphenated binaries still classify
  correctly (covered by unit test, but re-check one live tab).

## Extended-thinking gate covers Opus/Sonnet 4.6 (needs `make dev` restart)

`supports_extended_thinking` (`src-tauri/src/ai_agent/conversation_engine.rs`)
matched only `opus-4-7` / `opus-4-8` / `opus-4-9` — the last of which is not a
real model. Opus 4.6 and Sonnet 4.6 are in genai 0.6.5's own
`SUPPORT_EFFORT_MODELS` + `SUPPORT_ADAPTIVE_THINK_MODELS`, so they were being
denied reasoning for no reason. The Claude 5 family stays gated OFF on purpose:
genai's `claude-opus-(\d+)-(\d+)` regex needs a minor-version suffix, so a bare
`claude-opus-5` misses every table and would get the legacy `budget_tokens`
payload that Claude 5 rejects with a 400.

- [ ] Point an AI-chat provider at `claude-sonnet-4-6` with reasoning on and
  confirm reasoning chunks stream (no 400).
- [ ] Same with `claude-opus-4-6`.
- [ ] Point one at `claude-opus-5` and confirm it still answers normally — no
  `thinking` block, and crucially no 400 from the API.

## Dead commands removed: `update_session_cwd`, `get_global_hotkey` (needs `make dev` restart)

Both were registered Tauri commands with zero callers, carrying stale
DEFERRED notes claiming the feature was unwired. Both premises were false:
Rust already handles OSC 7 in-stream (`pty.rs`, `TermEvent::Osc7`), and the
hotkey already reaches the settings UI via the config payload. Removed the
commands, their `lib.rs` registrations, and their `INTENTIONALLY_UNMAPPED`
entries in `transport.ts`.

- [ ] `cd` around in a terminal, restart TUIC, confirm the restored session
  reopens in the last cwd (not the launch-time one).
- [ ] Open Settings → Keyboard Shortcuts and confirm the global hotkey still
  displays its current value on load, and that setting a new one still works.

## `VtLogBuffer::resize` lost its dead `shell_state` param (needs `make dev` restart)

The param was `_shell_state` (ignored) and its only caller computed a
`shell_states` lookup purely to feed it. Renamed `resize_with_shell_state` →
`resize`, dropped the param and the now-pointless lookup in `pty.rs`.

- [ ] Resize the window and a split pane over a full screen of content: the
  viewport must repaint fully, no blank area until a scroll.
- [ ] Resize while a fullscreen TUI (vim/htop) is running — alt screen must not
  reflow.

## File Browser: new file/folder now appears in tree view (Vite HMR, no restart)

`TreeNode` fetched its children only inside the click that expanded it, so
nothing could re-read an already-expanded folder. Every invalidation path was
dead: creating a file, deleting, renaming, and the `dir-changed` watcher all
drop a cache key and expected a reload that never happened — the row simply
never appeared, or the node rendered empty forever. Moved the fetch into a
`createEffect` keyed on expanded-state + cache presence.

- [ ] Tree view, expanded folder → right-click → New File → `.env`: the row
  must appear immediately (dimmed, because gitignored).
- [ ] Same for New Folder, Delete and Rename on an already-expanded folder.
- [ ] Create a file from an external editor inside an expanded folder: the
  `dir-changed` watcher must make it appear without collapsing/re-expanding.

## Markdown preview: checkboxes inside table cells

`[x]` / `[ ]` / `[~]` in a whole table cell now render as a real checkbox and
toggle the source. GFM task lists are list-item-only, so marked never did this.

- [ ] Open a markdown doc with a status table (e.g. `ego/docs/18-pi-hermes-…`),
  click a cell checkbox: it must toggle and the file must save with the mark
  changed at that exact cell, not another one on the same line.
- [ ] A table with the checkbox column in a different position must still map
  1:1 (two tables in the same doc with different column indexes).
- [ ] `[x]` inside a code fence must stay literal text.

## Voice plugin (`tuic-voice`)

Speaks an agent's prose while it streams, via the WebView's `speechSynthesis`.
Already loaded live (JS hot-reload, no rebuild needed) — the log confirms 68
voices in the WKWebView. What a human still has to judge is whether it picks the
right text, because every filter stage is a heuristic and the fixtures are
synthetic: no real Claude/Codex turn was ever replayed through it.

Right-click a terminal for **Voice: settings / toggle / stop speaking**.

- [ ] Run a Claude turn that mixes prose with tool calls: only the prose is read.
  A spoken `Bash`, `cargo`, a file path or a diff line is a filter bug — turn on
  "Log every dropped line" in the settings panel and check `GET :9876/logs` for
  the rule that let it through (or wrongly dropped a sentence).
- [ ] The status line / HUD below the input box is never read. This is the
  bottom-zone rule; if any of it is spoken, `chromeCutoff` found no `❯` anchor.
- [ ] Speech starts on the first complete sentence, NOT at the end of the turn.
- [ ] No sentence is repeated as the TUI repaints.
- [ ] The last sentence of a turn is spoken even without a full stop.
- [ ] "Voice: stop speaking" cuts off mid-sentence, immediately.
- [ ] An Italian reply is read with an Italian voice, not an English one
  mangling it — the plugin sets no `lang` by default, so this may need a voice
  picked by hand in the settings panel.
- [ ] Codex / Grok / Gemini: the drop rules were written against Claude Code's
  glyphs (`⏺`, `⎿`). Check what each of the others does to the filter.

## Voice plugin freeze (fixed)

Boss had to disable the plugin: the app locked up completely as soon as it
started speaking. Cause was an infinite loop in `drain()` — the buffer was cut on
`[.!?…:]` but the "is this worth saying" test excluded the colon, so a short
colon-terminated chunk was consumed, rejected and put back unchanged, and the
next pass cut it at exactly the same place. Frontend-only, so a reload is enough.

- [ ] Re-enable `tuic-voice` and run a turn containing a short clause before a
  colon ("Ecco il piano: ..."). The UI must stay responsive throughout.
- [ ] A turn ending on a colon with nothing after it: nothing is spoken until
  the turn ends, then the tail flushes. No freeze while waiting.
- [ ] An abbreviation ("e.g.", "v1.2") mid-sentence does not stop the real
  sentence after it from being spoken.

## Cross-repo tab misfiling (fixed)

Every "which repo owns this?" question now goes through one resolver
(`utils/repoOwnership.ts`), which has no parameter through which the focused
repo could reach it. Frontend-only — a browser reload picks it up, no rebuild.

Four of these were driven through the web UI on `:9876` with `agent-browser`.
Note for whoever repeats it: in dev, `:9876` serves the **built** `dist/`, not
Vite (which sits on `:1421`). A source edit is invisible there until
`pnpm build` — the desktop WebView gets it over HMR, the browser does not.

- [x] Launch a PTY / agent in repo A while looking at repo B: the tab appears
  under A, not B. _(verified: focused on `tuicommander`, MCP-spawned a session
  with `cwd=…/ego`; `ego:master` went 3→4 terminals, `tuicommander:main` stayed
  at 2.)_
- [ ] A plan file written by a session in repo A opens as a tab under A while
  you are on B. Previously the event was dropped outright — check the app log
  for `[plan] event:` with the right `ownerRepo=`.
- [x] Open the same relative path (e.g. `README.md`) in two different repos:
  two separate tabs, each visible only under its own repo. _(verified: opened
  `README.md` from `tuicommander` and from `ego`; each repo's tab bar shows
  exactly one, and the content differs.)_
- [x] Reopen a file belonging to repo A while focused on repo B: the existing
  tab is re-activated and stays under A — it must NOT migrate to B. _(verified:
  reopened tuicommander's `README.md` while focused on `ego`; tuicommander still
  has exactly one README tab, ego still has its own.)_
- [ ] "Open With TUICommander" on a file from a repo that is not the focused
  one: the tab is scoped to that file's repo, not shown under every repo.
  _(NOTE: desktop-only — the file association does not exist in web mode, so
  this one cannot be driven from the browser.)_
- [ ] Click a file path printed by an agent running in another repo: the tab
  lands under that repo.
- [ ] Register a repo AFTER sessions are already running inside it (this was
  `gate-os` in Boss's log): its parked tabs move to it automatically. The log
  shows `[Reconcile] <id> ... → <repo>:<branch>`. _(NOTE: not driven from the
  browser — unregistering and re-registering one of Boss's live repos writes to
  the shared `repositories.json`. Needs a scratch repo.)_
- [x] `cd` a terminal from one repo into another (OSC 7): the tab does NOT move.
  It stays under the repo it was opened in, keeps its place in the tab strip, and
  the sidebar counts do not change. _(REVERSED on 2026-08-29. This item once asked
  for the opposite and was ticked when the tab followed the `cd` — that was the
  regression Boss reported as "the app changes repo on its own". **Driven live in
  the web UI on 2026-08-29 and it FAILED first: `reclaimParkedTerminal` was only
  half the fix.** A throwaway session in `veritas` that cd'd to `mdkb` moved
  anyway — veritas 3→2, mdkb 2→3 — and the sidebar followed it, because
  `CanvasTerminal` also feeds the cwd to `onCwdChange` →
  `performCwdReassignment`, a second re-homing path that additionally calls
  `repositoriesStore.setActive` when the tab is the active one. Log evidence:
  `[CwdChange] term-24 → …/mdkb:main`, and the same line for the desktop
  client's own id `term-472`. Guarded now (`createTerminalWorktreeCoordinator.ts`)
  so a cwd may only re-place a tab inside its owning repo. Re-driven after the
  fix: cwd moved to mdkb, brainstorming stayed at 2, mdkb stayed at 2,
  `activeRepoPath` unchanged, no `[CwdChange]` line at all.)_
- [x] `cd` a PARKED terminal (one whose cwd matched no registered repo) into a
  repo that is registered: that one does move, and the log shows
  `[Reconcile] <id> ... → <repo>:<branch>`. This is the only case a `cd` settles.
  _(verified live 2026-08-29 in the web UI: a session opened on `/tmp` parked in
  the active repo — `cwd "/tmp" is owned by no registered repo` — then a `cd` into
  `veritas` produced `[Reconcile] term-25 …/mdkb:main → …/veritas:main` and moved
  the counts mdkb 4→3, veritas 2→3. The active repo did NOT change, which is the
  difference between settling a parked tab and re-homing an owned one.)_

## Background file tab must not steal the pane

Found while testing the item above. `tuic://open/<path>` with `focus: false`
deliberately does NOT switch repo — but it still called `mdTabsStore.add`, which
activates. The result was the exact ghost the focused branch avoids: the file's
content filling the pane while its own tab button is filtered out of the bar,
under a repo it does not belong to.

- [x] Open a file from repo A with `focus: false` while looking at repo B:
  nothing appears in B — no tab button, no content. _(verified in the web UI:
  zero occurrences of the filename anywhere in B's view.)_
- [x] The same tab IS present under repo A, unactivated. _(verified: switching
  to A shows it in the tab bar.)_
- [ ] `tuic://edit` with `focus: false` behaves the same way (same fix, via the
  new `background` option on `editorTabsStore.add`) — not exercised live.

## Poisoned `repositories.json` guard

Frontend-only (reload is enough). Reproduces the 2026-08-21 loss on purpose, so
**back the real file up first** and do it with the app stopped.

```bash
D=~/Library/Application\ Support/com.tuic.commander
cp "$D/repositories.json" "$D/repositories.mine.json"
echo '{"mutationVersion":1,"repos":[],"groups":[]}' > "$D/repositories.json"
```

- [ ] Start the app: the repo list is empty AND the Errors badge carries
  "repositories.json holds a mutation delta". No stack trace about
  `Object.values`.
- [ ] With the app still running, restore the backup over the poisoned file. The
  restored file must still be intact a minute later — saves are blocked, so
  nothing clobbers it. This is the step that failed during the incident.
- [ ] Restart: the 38 repos are back and a normal mutation (add a repo, reorder)
  saves again.
- [ ] A genuinely empty file (`{}`) still starts a fresh install: empty list, no
  error, and adding a repo persists.

## mdkb code intelligence — after `make dev` restart AND a fresh mdkb install

Rust change: needs a TUIC restart. It ALSO needs an mdkb newer than 3.7.17 —
`code_graph` only carries the machine-readable `symbols` field from the commit
added alongside this fix. Build and install mdkb first (`cargo build --release`
in the mdkb repo, then put the binary in a trusted dir), otherwise find-references
fails with "code_graph response has no 'symbols'".

- [ ] **Find references** (Shift+F12 on a symbol in the code editor) lists the
  callers. Before this fix it silently returned an empty list, always — mdkb
  answers `code_graph` with prose and TUIC was parsing it as JSON.
- [ ] Clicking a reference opens the caller **on the right line**, not one line
  above it. mdkb ranges are 0-based; TUIC now shifts them.
- [ ] **Outline panel**: clicking a symbol lands on its own line, not the line
  before. Check a symbol on line 1 of a file too — it used to clamp to line 1
  either way, hiding the off-by-one.
- [ ] **Cmd+Click go-to-definition** in the editor lands on the right line.
- [ ] Outline nesting: methods inside a type are indented one level, top-level
  functions are not. (Previously everything with any scope got the same indent.)
- [ ] With the OLD mdkb still installed, find-references shows no results and
  logs `mdkb_references failed: ... no 'symbols'` — it must NOT look like a
  symbol with zero callers. Check `GET http://localhost:9876/logs`.

## MCP `ui action=confirm` on every client — after `make dev` restart

Rust change: needs a TUIC restart. The native OS dialog is gone; the request now
goes to the desktop WebView, browser tabs and the mobile PWA at once, plus a
mobile push, and the first answer wins.

- [ ] Ask an agent for a confirmation (`ui action=confirm`). The dialog appears
  **in-app** (dark themed), not as a light macOS system sheet.
- [ ] The same dialog appears at the same time on `http://localhost:9876/mobile.html`
  in a browser or on the phone. This is the whole point — before, a remote human
  could not answer and the agent blocked until someone reached the machine.
- [ ] On a phone-width screen the dialog is readable and both buttons are
  tappable — it reuses the desktop `shared/dialog.module.css`, which had never
  been rendered at that width before. (Needs a live backend to raise a real
  confirm, so it could not be screenshot-checked at implementation time.)
- [ ] A confirm with a **long** message scrolls inside the dialog and still shows
  its buttons. `.popover` clips overflow, so `.body` now caps at 60vh and
  scrolls — check a couple of ordinary desktop dialogs (rename branch, create
  worktree, unsaved changes) still look unchanged, since that CSS is shared.
- [ ] Answering on **mobile** unblocks the agent, and the desktop dialog
  disappears by itself. Then the reverse: answer on desktop, mobile dismisses.
- [ ] With a mobile push subscription registered, a confirm raised while the PWA
  is closed sends a push carrying the title.
- [ ] Escape / clicking the overlay answers **cancel**, and Enter also takes
  Cancel — the agent asks before destructive ops, so Enter must not approve.
- [ ] Leave a confirm unanswered for 5 minutes: the agent receives
  `{confirmed: false, reason: "no answer within 300s"}` and the dialog closes on
  every client. It must NOT read as an approval.

## Nested shell prompt no longer reads as working

Requires a `make dev` restart — the change is in `src-tauri/src/pty.rs`.

- [ ] In a shell tab run `sh` (or `sudo su`). Within ~4s of the inner prompt
  appearing the tab badge goes **idle**. Before, OSC 133 latched it `busy` for
  as long as the inner shell lived — observed stuck for 33 minutes.
- [ ] Run a real command in that inner shell (`sleep 20`, a build): the tab goes
  back to working while it prints, and returns to idle at the prompt.
- [ ] `sudo dd if=… of=…` with no output for a minute must stay **working** —
  the wrapper has real work under it. This is the regression the probe must not
  cause.
- [ ] An agent tab (Claude, Codex) is unaffected: its badge still follows the
  ready-screen adapter, not this probe.


## Repository saves survive a concurrent diffstat change

Requires a `make dev` restart — the change is in `src-tauri/src/config.rs`.

- [ ] With two windows open on the same config, work in a repo so its diff counts
  keep moving (an agent committing is enough). Rename another repo, reorder the
  sidebar, add a repo. Each must persist. Before, `GET /logs` showed a stream of
  `Repository changes were not saved` / `repository configuration conflict`, and
  nothing was written.
- [x] `GET http://localhost:9876/logs?level=error` shows no
  `Repository changes were not saved` entry over a working session.
  _(verified 2026-09-07: live instance PID 28512, 10h45m uptime with Boss's
  agents committing throughout — `?level=error` returns **0 entries**, and
  neither `Repository changes were not saved` nor `repository configuration
  conflict` appears anywhere in the 1000-entry buffer at any level.)_
- [ ] Rename the same repo in two windows without reloading either: this must
  STILL conflict. The exemption covers counts, not intent.
- [ ] The sidebar diff counts keep updating — the exemption must not make them
  unwritable.

## A parked tab names the repo to register

Frontend only; Vite HMR picks it up.

- [ ] Have an agent spawn a child via MCP in a worktree of a repo that is NOT
  registered (`<repo>__wt/<branch>`). A toast appears: *Tab parked in the wrong
  repo — nothing claims "<repo root>"*. The log warning names the same path.
- [ ] Reconnecting many sessions from that one repo raises ONE toast, not one
  per session.
- [ ] Register that repo: the parked tab moves to it by itself, and the active
  repo does NOT change under you while the tab moves.

## An exited tab says so instead of going black

Frontend only; Vite HMR picks it up. Already verified live in the running dev
build: `term-100` ("GitHub state", exited agent in a deleted worktree) renders
one `terminal-exited-notice`, the other 15 tabs render none. What is left is the
visual check.

- [ ] Click an exited tab (grey dot). The panel shows a centred, muted *Session
  ended* / *The process exited and its output was released. Close this tab to
  remove it.* — not a black void.
- [ ] Open a brand-new terminal: the notice must NOT flash before the PTY
  spawns. A new tab also has a null sessionId; only `shellState === "exited"`
  may show the notice.
- [ ] Let an agent finish in a background tab: the tab keeps its grey dot and
  its name, and the panel shows the notice when you switch to it.

## Repository saves converge across windows

**Rust change — a `make dev` restart (or `make build`) is required.** The
frontend half is HMR-only, but `repositories-changed` is emitted by the backend,
so nothing happens until the Rust process is rebuilt.

- [ ] Open the desktop app and a browser at `http://localhost:9876/`. Rename a
  repo in the browser. The desktop sidebar shows the new name without a reload,
  and vice versa.
- [ ] Add a repo in one client: it appears in the other, in the right sidebar
  position.
- [ ] Remove a repo with no terminals open in one client: it disappears from the
  other.
- [ ] Remove a repo that has open terminals in the other client: that client
  KEEPS the repo and its tabs (they must not be orphaned), and the repo is still
  visible in the sidebar — not just present in memory.
- [ ] Remove a worktree/branch in one client while the other has a terminal open
  on that exact branch: the branch row and its tabs stay in the other client.
- [ ] Rename a repo in one client while the other has that same repo open and
  actively changing (edit a file so the diffstat moves): the rename still lands.
- [ ] Group a repo in one client, then delete the group there: the other client
  loses the group and shows the repo ungrouped, with no empty accordion left.
- [ ] Switch the active repo in one client: the other client's focus does NOT
  move.
- [ ] After any of the above, rename a *different* repo in the client that
  received the change. `GET http://localhost:9876/logs?level=error` shows no
  `Repository changes were not saved`, and the first client's change is still
  there — the receiver must not have reverted it.
- [ ] Toggle something that writes no change (re-save the same value): the other
  client must not re-read. `GET /logs` shows no burst of `load_repositories`.

## Auto-retry on Claude Code's prose 5xx message

**Rust change — a `make dev` restart (or `make build`) is required.** The parser
runs in the backend, so nothing changes until the Rust process is rebuilt.

**Precondition:** Settings → Agents → Claude → enable auto-retry. It is
`auto_retry_on_error`, default `false`, and it is currently unset in
`config.json`, so with it off you only get the red error badge and no retry.

- [ ] Reach a real `API Error: 500 Internal server error. This is a server-side
  issue…` in a Claude tab. `GET http://localhost:9876/logs` shows
  `[ApiError] … pattern=claude-server-error-friendly kind=server` followed by
  `[AutoRetry] claude: attempt 1/3 in 5s`.
- [ ] The tab does NOT play the error sound and does NOT show the red awaiting
  badge while a retry is pending — only after the 3rd attempt is exhausted.
- [ ] `continue` is injected after 5s and the turn resumes.
- [ ] With auto-retry disabled for Claude, the same error sets the red badge
  immediately and injects nothing.
- [ ] The message wraps across terminal rows (narrow the window before it
  fires): detection still happens — the pattern anchors on `API Error: 5xx`.
- [ ] A 429/overload (`API Error: 529` or "temporarily limiting requests") is
  still logged as a rate limit, not as a server error, and injects nothing.

## Usage ticker follows the agent in the terminal (Claude / Codex)

**Rust change — a `make dev` restart (or `make build`) is required.** The new
`get_codex_usage_api` command and the `GET /codex/usage` route live in the
backend, so the ticker shows `offline` until the Rust process is rebuilt.

**Precondition:** Settings → Agents → the Claude Usage toggle must stay enabled;
it now drives both agents. A Codex login must exist (`~/.codex/auth.json`).

- [ ] Focus a tab running Claude: the status bar ticker is labelled `Claude` and
  shows the `5h` / `7d` numbers as before. Clicking it still opens the Claude
  Usage dashboard tab.
- [ ] Focus a tab running Codex: the label becomes `Codex` and the text shows
  the Codex windows (e.g. `7d: 100% -1d`). The switch happens on tab focus,
  without waiting for the 5-minute poll.
- [ ] Clicking the Codex ticker opens a **Codex Usage Dashboard** tab (a
  singleton — clicking again focuses the existing tab, it does not duplicate).
- [ ] Switch to a plain shell tab: the ticker keeps showing the last agent
  rather than blanking or reverting to Claude.
- [ ] Switch Claude → Codex → Claude quickly. No stale value from the previous
  agent lands on the ticker (the seq guard should drop late responses).
- [x] `curl http://localhost:9876/codex/usage` returns the JSON payload and
  contains **no** `email`, `user_id` or `account_id` field.
  _(verified 2026-09-07, live PID 28512: HTTP 200, 791 bytes. Full recursive key
  set is rate-limit/quota data only — `plan_type`, `credits`, `model_usage`,
  `primary_window`, `used_percent`, … — and none of `email`, `user_id`,
  `account_id` appears at any depth. Port was written as 9877; only 9876 runs.)_
- [ ] Rename `~/.codex/auth.json` away and focus a Codex tab: the ticker shows
  `no token`, and `GET /logs` has no warn line for it (missing token is not an
  error worth logging).
- [ ] With the Claude Usage toggle off, no ticker appears for either agent.

### Codex Usage Dashboard

Same `make dev` restart precondition — the `get_codex_usage_stats` command and
`GET /codex/stats` are new Rust.

- [ ] **Rate Limits** section shows the account windows first with plain `5h` /
  `7d` names, then the per-model windows prefixed with the model name. A window
  at 100% is red, ≥70% amber, below that normal.
- [ ] **Tokens per Day** renders one bar per day; hovering a bar shows the date
  and the token count. The tallest bar is the busiest day, and a near-zero day
  is still visible as a sliver rather than invisible.
- [ ] **Insights** tiles are populated (lifetime tokens, peak day, threads,
  streak, longest turn, fast mode, skills, reasoning effort) — no `NaN`, and
  absent values read `--`.
- [ ] Kill the network (or rename `~/.codex/auth.json`) and open the dashboard:
  each section shows its own error hint independently — one failing endpoint
  must not blank the other section.
- [x] `curl http://localhost:9876/codex/stats` contains **no** `profile` object
  (no username, display name or avatar URL).
  _(verified 2026-09-07, live PID 28512: HTTP 200, 1618 bytes, single top-level
  key `stats`. None of `profile`, `username`, `display_name`, `avatar_url`,
  `email` appears at any depth. Port corrected from 9877.)_

## `index.lock` owner probe now fails closed (#694-4fcc)

**Rust — needs a `make dev` restart to take effect.** Unit-tested (28 passed), but
the live behaviour changed, so it is worth one look on a real repo.

Policy, decided by Boss 2026-09-07: when the `lsof` owner probe cannot answer, the
lock is **kept**, not reclaimed. The escape hatch is age at
`UNADJUDICATED_LOCK_STALE_SECS` (1 h), so a lock nothing can adjudicate is still
cleared eventually.

- [ ] Normal case unchanged: a genuinely orphaned `index.lock` (kill a `git add`
  mid-write, wait 30 s) is still reclaimed and git works again.
- [ ] With the probe unavailable, the lock survives: temporarily shadow `lsof`
  with a non-executable stub on `PATH`, create a 30 s-old lock, run a git command
  through TUIC, and confirm the lock is **still there** and `GET /logs` carries
  `Keeping index.lock … ownership could not be determined`.
- [ ] The log names *which* failure it was — `could not run` vs `outlived its 2s
  deadline`. The two are not interchangeable and the message must say which.
- [ ] Watch for a lock kept longer than it used to be during ordinary work. The
  measured `lsof` latency here is 0.32–3.7 s against a 2 s deadline, so
  `DeadlineExceeded` is routine, not exotic — if that turns out to be noisy in
  practice, the deadline is the knob, not the policy.

## Terminal answers OSC 10/11/12 colour queries

**LIVE since the 2026-09-07 07:42 `make dev` — but NOT because it was committed.**
The answering code is still uncommitted: `git show HEAD:src-tauri/src/terminal_grid.rs`
has `Event::ColorRequest(..)` in the **ignore list** and no `palette_color_for_index`
at all. `make dev` builds the *working tree*, so the rebuilt binary contains it.

> **Three states, not two — and the restart gate only distinguishes two of them.**
> The gate answers "which commits does the running process contain". A fix can be:
> (a) committed and inside the gate → live; (b) committed and after the gate →
> needs a restart; (c) **uncommitted** → live if a `make dev` rebuilt since it was
> written, in no binary at all otherwise. The gate is blind to (c), and this tree
> carries 101 uncommitted files.
>
> This section was wrong twice in one day, once in each direction: first marked
> LIVE off a gate check while the code was uncommitted and unbuilt, then marked
> NOT LIVE right after a `make dev` had in fact built it. **Neither check is the
> answer — probe the running process instead**, which for this fix is one command:
>
> ```sh
> # in a throwaway session: printf '\033]11;?\033\\' as a COMMAND, not piped
> # answered  -> output contains 11;rgb:xxxx/xxxx/xxxx
> # unanswered-> nothing comes back
> ```
>
> Measured 2026-09-07 on PID 37840: `11;rgb:2525/2525/2626`. Answered.

Fixes the `^[[?6c` garbage and the 1.2 s probe loop: Claude Code asks for the
background with `OSC 11 ; ? ST` + `ESC[c`, and TUIC used to drop the colour query
while answering the fence.

The code below is **working-tree code**, reviewed by inspection (ladder rungs
1–2). It describes what will run once this is committed and rebuilt — not what
runs now:

- the reply is built at `terminal_grid.rs:196-202` and pushed as
  `TermEvent::PtyWrite`, drained unconditionally on the chunk path at
  `pty.rs:5106-5119` — so it does NOT depend on a frontend being attached;
- `palette_color_for_index` (`terminal_grid.rs:94-103`) resolves foreground,
  background and cursor off a global `PALETTE` that always has a value, so the
  `None` branch cannot swallow a 10/11/12 query;
- reply content is asserted by `terminal_grid.rs:2360-2415`.

**A live CLI probe of the reply was attempted and is NOT a usable check — do not
retry it the obvious ways.** Two traps, both hit on 2026-09-07:

1. `printf '…' | cat -v` (what this item used to say) **cannot work**: the pipe
   sends the query to `cat`, not to the terminal, so `cat -v` just prints the
   query back and the emulator never sees it. The old recipe was proving nothing.
2. `POST /sessions/{id}/write` **also cannot work**: it feeds the shell's *stdin*,
   while an OSC query has to arrive on the emulator's *output* parse path. The
   shell just echoes `11;?` as typed text.

Running `printf '\033]11;?\033\\'` as a command does reach the parser, but then
reading the reply needs raw-mode `stty` juggling inside the PTY, and that harness
returned a single truncated `ESC` byte — a harness artefact, not an app result.
What is left genuinely needs eyes on a real agent:

- [x] The `ESC]11;?` / `ESC[c` pair fires **once or twice at startup, not every
  ~1.2 s**. _(verified 2026-09-07 on PID 37840: the colour query is answered —
  `11;rgb:2525/2525/2626` — so the probe concludes and stops re-arming. The DA
  query also gets exactly one reply, not a repeat. This is the loop half of the
  fix and it works.)_
- [ ] **`^[[?6c` no longer appears at startup — NEEDS A `make dev` RESTART.**
  Diagnosed from capture `f2bddfb0` on 2026-09-07, which settled it. The
  ordering, verbatim from the frames:

  ```
  [24] OUT ESC[>0q     claude asks XTVERSION
  [25] OUT ESC[c       claude asks DA1                       t=152.165s
  [26] OUT ^[[?6c      ← our reply, ECHOED BACK as literal text
  [33] OUT ESC[>0q     claude asks again...
  [34] OUT ESC[c       ...because it never received the answer
  [35] OUT (status)    the second reply lands silently — ECHO is off by now
  ```

  We answered *before* claude switched the tty out of cooked mode. `ICANON`
  was still set, so the reply was never delivered (a canonical read blocks for
  a newline a terminal reply never contains), and `ECHO`+`ECHOCTL` painted it
  as `^[[?6c`. Claude re-queried 100 ms later and got a clean answer. So the
  reply was never lost — only the first one was garbage on screen. That is also
  why a clean throwaway session did not reproduce it: the race needs claude to
  be slower to reach raw mode than we are to answer.

  Fix (uncommitted, in the working tree): `tty_would_swallow_reply` in `pty.rs`
  reads the master's termios and `write_terminal_reply` withholds a reply while
  `ICANON` is set.

  > **The gate keys on `ICANON`, not `ECHO`, and the first draft got this
  > wrong.** `ECHO` only decides whether the bytes are *also* painted; `ICANON`
  > decides whether they are *delivered*. In cbreak (`ICANON` off, `ECHO` on)
  > the querier reads the reply immediately, so an `ECHO` gate would withhold a
  > reply nothing will resend — trading Boss's cosmetic `^[[?6c` for a hung
  > agent. Both directions are pinned:
  > `terminal_reply_is_withheld_while_the_tty_is_canonical` and
  > `terminal_reply_is_delivered_in_cbreak_even_though_the_tty_echoes`.
  **Rust — will not hot-reload.** Verify after the next `make dev`: launch
  `c2` in a real repo tab, no `^[[?6c` above the banner, and the DA/colour
  queries still get answered (`ESC]11;?` still reports `11;rgb:…`).
- [ ] Switch to a light theme, then repeat the query: the reported colour
  follows the theme (the frontend republishes on remeasure).
- [ ] Only one publish per real theme change — `GET /logs` shows no burst of
  palette traffic when resizing the window with several tabs open.
- [ ] `curl -X POST http://localhost:9876/terminal/theme-colors -H 'content-type: application/json' -d '{"foreground":[255,0,0],"background":[0,255,0],"cursor":[0,0,255]}'`
  returns `{"ok":true}` and changes what the query above reports. (Port corrected
  from 9877: there is no second instance running; the live app serves 9876.)

### Upstream MCP OAuth — concurrent flows, expiry, late redirect

**Requires a `make dev` restart** — all of this is Rust (`mcp_oauth/`,
`mcp_proxy/registry.rs`). The running instance still has the old serialized
behaviour.

- [ ] Settings → Services → MCP: click **Authorize** on two different upstreams
  back to back. Both show the consent dialog and open a browser tab within a
  second. Previously the second click hung silently for 5 minutes: no browser,
  no dialog, no error, while the row already read "Awaiting authorization…".
- [ ] Click **Authorize**, then **Cancel** before completing consent: the row
  leaves "Awaiting authorization…" immediately and Authorize works again on the
  next click (no queue built up behind it).
- [ ] Click **Authorize** and then do nothing for >5 minutes. The row returns to
  **Authorize to connect** (`needs_auth`) on its own, and
  `GET http://localhost:9876/logs?source=mcp_oauth` shows
  `Cleaned up expired OAuth flows` naming the upstream. It must not stay stuck
  on "Awaiting authorization…".
- [ ] Click **Authorize**, wait out the full 5-minute timeout *in the browser*,
  then complete consent. The browser shows the TUIC "Authentication failed" card
  reading "This authorization request expired or was cancelled…" plus "press
  Authorize again" — **not** the browser's own "can't connect to the server"
  page.
- [ ] A normal successful authorization still lands on the green
  "Authentication complete" card and the upstream goes `ready`.

### Terminal: no grid wipe on tab switch, resubscribe on reattach (#657-4345)

Frontend only (`Terminal.tsx`) — Vite HMR picks it up, no `make dev` restart
needed. Canvas painting is not observable over HTTP, so these need eyes.

- [ ] Switch back and forth between two busy terminal tabs. The returning tab
  shows its content immediately with no blank flash. Previously every switch ran
  `resubscribe()` + `refresh()`, which cleared the grid and repainted it
  (paint → wipe → paint).
- [ ] Detach a tab into a floating window, then close that window to reattach.
  The reattached tab still paints live output and scrolls — the grid channel is
  resubscribed on this path, which is the only path that still resubscribes.
- [ ] Open a terminal in a split pane, collapse the pane to zero width, leave it
  collapsed for a minute. `GET http://localhost:9876/logs?source=terminal` shows
  one `Container stayed zero-size for 120 frames` warning and CPU stays flat.
  Previously that container kept a `requestAnimationFrame` loop re-arming every
  frame for the lifetime of the page, one loop per terminal, surviving unmount.

- [ ] (story 644-2cf4, Rust — needs a `make dev` restart) A reader-thread panic no
  longer leaks its ticker. The panic path now clears the `running` flag, so the
  16 ms frame ticker and the 1 Hz silence timer both stop. Hard to force by hand;
  the observable if it ever happens is that a session logging
  `READER THREAD PANICKED` leaves no residual CPU and its tab stops repainting.
  Enable diagnostics and watch `thread count` stay flat after such a log line.

- [ ] (story 645-9bfb, Rust — needs a `make dev` restart) Resize an alternate-screen
  agent (grok) while it is streaming, then let it ask a low-confidence question.
  The tab must badge within about a second of the resize. Before the fix the resize
  grace re-armed on every chunk, so questions, rate-limit and API-error events and
  the busy badge stayed suppressed until the agent went quiet for a full second.
  Also confirm a resize during a normal-screen Claude re-render still does NOT
  flip an idle tab to busy — that is the behaviour the grace extension protects.

## Settings search (story 684-35a8, frontend — Vite HMR picks it up)

- [ ] Open Settings. A "Search settings" box now sits at the top of the left nav.
  Check it reads well at the narrowest (140 px) and widest (280 px) nav widths —
  the box shares the nav's resize handle area, and only the DOM is covered by
  tests, not the rendering.
- [ ] Type `relay`. The tab body is replaced by a result list; each row shows the
  setting on top and a `Tab › Section` trail underneath. Confirm the trail is
  legible against the panel background in both light and dark themes.
- [ ] Click the "Relay Server URL" result. Services & MCP opens and the view
  scrolls to that field. The smooth-scroll animation itself is not observable
  over the DOM — confirm it lands on the field, not at the top of the tab.
- [ ] Search a Dictation setting (e.g. `whisper`) in the desktop app: it appears.
  In browser mode (`http://localhost:9876/`) the Dictation tab is absent, so the
  same query must return "No settings match your search."

## Cross-kind tab drag reorder (story 682-b8d2, frontend — Vite HMR picks it up)

Free-mode and terminals-first drag reorder across tab kinds never worked: the
cross-kind order list had no writer, so the reorder call always returned early.
The DOM order is covered by tests; a real pointer drag in the WebView is not.

- [ ] Settings → Appearance → Tab Ordering → **Free**. Open a terminal, a diff and
  a markdown tab. Drag the diff tab onto the left half of the terminal tab: it must
  land before the terminal and stay there. Repeat dragging the terminal to the right
  half of the markdown tab.
- [ ] Still in Free mode, open a new terminal after a drag. It must appear at the
  end without disturbing the order you dragged.
- [ ] Switch to **Terminals First**. Terminals stay leftmost. Drag the markdown tab
  onto the diff tab — the two non-terminal tabs must swap, and the terminals must
  not move.
- [ ] Switch to **Grouped by Type** (the default). Ordering must be unchanged from
  before this story: kinds stay grouped, and dragging only reorders within a kind.
- [ ] Close a tab you dragged, then reopen one. No ghost position: the reopened tab
  appears at the end, not at the closed tab's old slot.

## Corrupt `config.json` is preserved, state-lane depth is reported (story `712-e1d2`, Rust — needs `make dev` restart)

An unparseable `config.json` used to be silently replaced by defaults, and startup
then wrote those defaults straight over it (`lib.rs:1228` fills the empty session
token and VAPID key, so `config_dirty` is always set on that path). It is now moved
aside as `config.corrupt-<uuid>` before defaults are returned, matching what every
other config file already did. Covered by
`config::tests::corrupt_app_config_survives_the_first_run_save_that_follows_it` and
`config::tests::two_corrupt_app_config_loads_keep_two_distinct_backups`; the items
below are the live confirmations only.

- [ ] With the app stopped, truncate `config.json` mid-document, then start it. The
  app must come up on defaults, and the config dir must hold a
  `config.corrupt-<uuid>` file with the original bytes. Repeat once more: the second
  run must add a SECOND backup, not overwrite the first.
- [x] `curl -X POST localhost:9876/diagnostics -d '{"enabled":true}' -H 'content-type: application/json'`,
  wait 30s, then `curl 'localhost:9876/logs?source=diagnostics'` — the `HEALTH` line
  must carry a `state_lane=<n>` field, normally `0`.
  _(verified 2026-09-07 on live PID 28512: two consecutive HEALTH snapshots both
  carry `state_lane=0`, e.g. `HEALTH cpu=5.8% children_cpu=0.0% threads=120
  fds=85 sessions=10 … head_emits_suppressed=0 state_lane=0`. Diagnostics was
  off before the check and was restored to off after. The `CPU SPIKE` variant
  cannot be forced on demand and is left unverified.)_

## API-error dedup reopens on user input (story 646-1a9f, Rust — needs `make dev` restart)

The reset lived in `parse_clean_lines` keyed on a `UserInput` event no output
parser emits, so after the first API error of a session the identical error was
never reported again. The input path now parks the reset on `SilenceState` and
the reader drains it. Covered by `pty::tests::user_submission_rearms_the_api_error_dedup`;
the item below is only the live confirmation that the notification really fires.

- [ ] Provoke or wait for an `API Error: 5xx` in an agent tab — the error toast/sound
  must fire. Submit a prompt, provoke the same error again: it must notify a
  SECOND time instead of staying silent for the rest of the session.

## MCP handshake and repo issue actions (story 676-89c2, Rust — needs `make dev` restart)

`initialize` used to answer a fixed `2025-11-25` whatever the client asked for,
and the `repo` tool never dispatched its GitHub issue actions.

- [x] Reconnect an MCP client that speaks an older **supported** revision. The
  `initialize` result must echo the version the client offered, not `2025-11-25`.
  _(verified 2026-09-07, live PID 28512, `POST /mcp`: offered `2025-03-26` →
  answered `2025-03-26`; `2026-07-28` → `2026-07-28`; `2025-11-25` →
  `2025-11-25`; unsupported `1999-01-01` → `2025-11-25`. **Wording corrected —
  "an older revision" is not enough and misled this check once.** The supported
  set is `["2026-07-28","2025-11-25","2025-03-26"]`
  (`mcp_transport.rs:5443`); offering `2024-11-05` or `2025-06-18` correctly
  falls back to `2025-11-25`, because echoing a revision the server does not
  implement would be a promise it cannot keep — see the doc comment on
  `negotiate_protocol_version`, `mcp_transport.rs:5450-5463`. A fallback answer
  is NOT the bug this item was written about.)_
- [ ] `repo action=issues`, `action=close_issue` and `action=reopen_issue` all
  reach GitHub instead of answering `Unknown action 'issues' for tool 'repo'`.
- [ ] Register the same UI tab id repeatedly from one session: it dedupes, and the
  per-session count stops at the cap instead of growing.

## GitHub poller survives a dropped connection (story 648-051b, Rust — needs `make dev` restart)

The shared HTTP client had no timeout at all, so a dropped VPN wedged the poller
on a socket the peer never answers.

- [ ] Start the GitHub poller, then drop the network (turn off Wi-Fi or the VPN).
  Within ~30 s the request must fail and the poller must log the error and carry
  on, not sit silent forever.
- [ ] With the network still down, disable GitHub polling in Settings. It must
  stop immediately, not after the in-flight request gives up.

## Git status and index.lock ownership (story 673-19fa, Rust — needs `make dev` restart)

The sidebar dirty badge now reads the gix porcelain-v2 counts, and the stale
`index.lock` sweep asks `lsof` who owns the lock before trusting the age rule.

- [ ] The sidebar repo badge still shows clean / dirty / conflict correctly:
  edit a file, stage it, create a merge conflict, then clean up. Each state must
  match what `git status` reports.
- [ ] Start a long `git add` or `git stash` in a large repo from a TUIC terminal
  and leave it running past 30 s. TUIC must NOT delete that repo's
  `.git/index.lock` while the command still holds it.

## Weekly advisory scan (story 663-feea, CI — verify after merge)

`audit.yml` now installs a prebuilt `cargo-audit` and reads its ignore list from
`src-tauri/.cargo/audit.toml`. The workflow only runs on Mondays or on demand,
so nothing local can prove the install step resolves.

- [ ] Trigger `audit.yml` manually (`gh workflow run audit.yml`) and confirm the
  `Install cargo-audit` step resolves `taiki-e/install-action@cargo-audit` and
  the scan runs to completion.

## Process manager after the shared `ps` walk (story 669-e059, needs a Rust restart)

The stats refresh now queries the process table ONCE per refresh and walks each
session's subtree out of that shared map, instead of forking `ps` per session.
Rust does not hot-reload, so this needs a `make dev` restart to load.

- [ ] With several sessions open (at least one running a nested command such as
  `cargo test` or a `sh -c 'sleep 30'`), open the process manager and confirm
  each session still lists its child AND its descendants, with non-zero RSS.

## Smart Prompts dropdown: missing-provider hint is now clickable (story 706-8d98, frontend only — Vite HMR, visual)

Only the toolbar "Smart Prompts Library" dropdown (the sparkle icon) got the
fix. The compact split-button strip (git changes tab, PR popover, etc.) still
shows the same reason as a plain hover tooltip — see the DEFERRED comment at
`SmartButtonStrip.tsx` for why that one was left alone.

- [ ] In Settings → Providers, make sure no model is assigned to the "Headless"
  slot (or temporarily unassign it).
- [ ] Create or edit a Smart Prompt with Execution Mode = "API (LLM direct)"
  (or "Headless" with the agent set to "API"), and give it `placement: toolbar`.
- [ ] Open the toolbar's Smart Prompts dropdown (sparkle icon). The prompt
  should appear dimmed/disabled, and *underneath its name* (not just as a
  hover tooltip) you should see the full reason text — "Headless provider not
  configured — add a provider and assign the Headless slot in Settings →
  Providers" — rendered as an underlined, clickable control.
- [ ] Click that reason text. Confirm it opens the Settings panel directly on
  the **Providers** tab (not the default "Smart Prompts" tab the footer
  "Manage Smart Prompts..." link opens), and confirm nothing was sent/run in
  the terminal.
- [ ] Confirm a prompt disabled for an unrelated reason (e.g. no active
  terminal) still shows only the old hover tooltip — no clickable text was
  added there.

## HTTP git commands are now bounded (story 697-d6ea, Rust — needs a `make dev` restart)

Rust does not hot-reload, so this needs a restart to load. The HTTP error
path also changed shape: a git spawn failure used to return HTTP 500 and now
returns HTTP 200 with `{ success: false, exit_code: -1, stderr: ... }`, the
same shape the Tauri command has always returned. That is deliberate — the
frontend documents `run_git_command never throws; inspect success explicitly`
(`BranchesTab.tsx:18`), so the old 500 made a browser client behave
differently from the desktop.

**The shape half is verified** (2026-09-07, live PID 28512, no restart needed —
it is inside the gate): `POST /repo/run-git {"path":"…/tuicommander",
"args":["rev-parse","--verify","no-such-ref-xyz123"]}` returns **HTTP 200** with
`{"success":false,"exit_code":128,"stderr":"fatal: Needed a single revision"}` —
not a 500. Note the payload field is `path`, not `repoPath`, and there is a
subcommand allowlist (`git_routes.rs:251-267`): `reset` comes back **HTTP 400**
`Git subcommand "reset" is not allowed via HTTP`. The items below are the
remaining behavioural checks.

- [ ] Desktop, normal path: fetch/pull/push from the Git panel still work and
  still report failures the way they did before. No visible change expected.
- [ ] Browser mode (`http://localhost:9876/`): do a fetch on a repo whose
  remote is reachable. It should behave exactly as on desktop.
- [ ] Slow/dead remote: point a throwaway repo at an unroutable remote and
  fetch. It must give up after ~180s with a `git timed out` message, not hang
  forever. This is the whole point of the story — do it on a throwaway repo,
  never on a real one.

## Language picker in Settings → General (story 689-52d8, visual)

The General tab now renders a Language select above Shell, listing every locale
that ships a message catalog. Only `en.json` exists today, so the list has one
entry ("English"). Frontend-only change, so Vite HMR loads it, but the rendering
cannot be checked from a test.

- [ ] Open Settings → General and confirm the Language select sits directly under
  the "General" heading, above Shell, with the same field styling as the IDE and
  update-channel selects (label, control width, hint line).
- [ ] Confirm the option reads "English" and the hint reads "Language of the
  TUICommander interface".
- [ ] Type "language" in the Settings search box and confirm the result reads
  `General › General` and scrolls to the field when selected.
- [ ] The single option is by design: only locales that ship a catalog are
  offered, and listing others would show English under a foreign name. The
  control stays visible so the docs that already promise it stay true. Say if
  you would rather it were hidden until a second catalogue lands.

## PTY chunk-path refactor (story `668-59be`, **Rust — needs `make dev` restart**)

Behaviour must be IDENTICAL to before; five characterization tests assert that,
so these checks are looking for what a test cannot see on a live agent.

- [ ] On a live Claude tab and a live grok tab: the state badge still moves
  working → idle → awaiting as it did. The chunk path was reordered around the
  chrome cutoff and the SilenceState locks; the tests cover the events, not the
  feel.
- [ ] A slash menu (`/` in Claude Code) still opens and is detected. This is the
  case that killed the proposed optimisation — the menu renders BELOW the input
  box, so it is the first thing to break if the cutoff order is ever touched
  again (`DEFERRED (2026-09-06)` at `pty.rs:4911`).
- [ ] A choice dialog and an Ink question footer still badge the tab as awaiting,
  and the badge still CLEARS afterwards.
- [ ] **Observability trade — check this deliberately.** The DECRST-leak
  `error!` and the "Anomalous ANSI sequence" `warn!` no longer appear unless
  Diagnostics is on. Run `curl -X POST localhost:9876/diagnostics -d
  '{"enabled":true}' -H 'content-type: application/json'`, then confirm they
  reappear in `GET /logs`. If either turns out to be load-bearing for an open
  bug while OFF, revert the three `&& crate::cpu_watchdog::diagnostic_mode()`
  guards at `pty.rs:5025`, `pty.rs:8014`, `pty.rs:8042` — they are isolated.
- [ ] Resize a tab mid-turn on an agent that was busy: the resize grace still
  suppresses the false idle. `on_resize` and the `is_resize_grace` read now run
  BEFORE `stamp_last_output_now` rather than after (`pty.rs:5741-5770`). Both
  touch only `SilenceState.last_resize_at`, but that machinery has a long
  fix/revert history, which is why it is here and not left to the suite.

## Browser-mode scroll (story `658-3ce1`, **Rust — needs `make dev` restart**)

`pending_scroll` is now created by `spawn_reader_thread` instead of the
desktop-only `subscribe_terminal_grid`, so a session no desktop terminal ever
rendered can still be scrolled. Proven live against a headless `tuic-remote`
built from this tree — the same POST answered `{"ok":true}` and left
`display_offset` at 0 before the fix, and moved it to the requested offset after
— so what is left needs a real canvas, which no endpoint renders.

**Re-proven 2026-09-07 on the running desktop build** (PID 28512, inside the
gate — no restart needed), on a session created purely over HTTP that no desktop
terminal ever rendered: `seq 1 500` → `scroll-info` `{"display_offset":0,
"total_lines":502,"screen_lines":24}`; `POST terminal/scroll-to-offset
{"offset":120}` → `{"ok":true}`; `scroll-info` then reads
`"display_offset":120`. The viewport genuinely moved rather than just the
counter: `row-text?row=0` returns `"358"`, and 502 − 24 − 120 = 358 exactly.
That is the whole mechanism the two items below sit on; only the wheel/scrollbar
*rendering* still needs eyes.

- [ ] Open the web UI (browser, not the desktop app) on a session with
  scrollback and scroll with the wheel and by dragging the scrollbar: the
  viewport must move, not just the thumb.
- [ ] With that browser attached, close the same terminal's tab in the desktop
  app. The browser must keep scrolling — the unsubscribe no longer drops the
  session's scroll target.

## Opening a 23 MB JSON no longer freezes the editor (2026-09-06, frontend — HMR; one Rust part needs `make dev` restart)

Above 500 KB the editor is plain text: no highlighting, no git gutter, no inline
blame, and the disk poll no longer re-reads the whole file 5 s after opening.
`get_gutter_changes` (Rust) returns nothing for an untracked file instead of
one "added" marker per line. Measured in Chrome only; WKWebView is the one
that blocked for over a minute.

- [ ] Desktop app: open `~/Gits/personal/ego/mutants.out/mutants.json` (23 MB,
  gitignored). It must open in a few seconds at most, unhighlighted, with no
  gutter markers. With `window.__TUIC__.setPerfDebug(true)` first, any
  remaining `UI freeze` line on `/logs` names an `editor.*` breadcrumb.
- [ ] After the `make dev` restart: a small **untracked** file opens with an
  empty gutter; a tracked file with an unsaved-vs-HEAD edit still shows its
  markers; the diff viewer still shows the untracked file as all added.
- [ ] Desktop app (WKWebView), after the fix that installs the document with
  `EditorView.setState` instead of a whole-document dispatch: the same 23 MB
  file must scroll smoothly, and a small file must still highlight, show its
  git gutter and its inline blame, and keep undo working across an external
  reload (edit the file from a terminal while the tab is open).

## goose tabs now reach idle after a turn (story `699-c6e0`, **Rust — needs `make dev` restart**)

`goose session` is one long-lived foreground command, so OSC 133 marks the tab
busy once and nothing ever cleared it. `detect_goose_screen_activity` now reads
the composer footer (`Enter to send` → Ready) and the interrupt hint
(`Ctrl+C to interrupt` → Working), with the hint checked first so a working
screen is never downgraded. Captured live off goose 1.49.0.

- [ ] Open a goose tab, let it sit at the composer: the badge must read idle,
  not "working". This is the whole bug — before the adapter it latched busy
  from the moment the process started.
- [ ] Send it a prompt: the badge must go to working for the whole turn (the
  spinner message is whimsical and changes every second — the badge must not
  flicker with it) and back to idle when the composer returns.
- [ ] Interrupt a turn with Ctrl+C: the badge must return to idle, not stay
  working.
- [ ] amp, cursor and droid are still **not** adapted (see the DEFERRED note on
  `has_ready_screen_adapter`). If you run one of those, expect the old
  latched-busy behaviour — that is known, not a regression from this change.

**Config note:** to capture the fixtures I pointed `~/.config/goose/config.yaml`
at the local ollama (`gemma4:12b-mlx`), since goose refused to start without a
provider and `goose configure` has no non-interactive flags. Your original file
is at `~/.config/goose/config.yaml.bak-tuic` — restore it if you had goose set
up against a real provider.

## Still needs a human

Every item here failed the ladder for a stated reason — real hardware, a second
application, a canvas no endpoint renders, or a judgement made by eye or ear.
None of them is here because nobody looked.

**Embedded factual claims re-probed 2026-09-07 — all still true**, so nobody
needs to re-run this: `command -v` still finds none of `amp`, `cursor`,
`cursor-agent`, `goose`, `droid`, `zed`, `lazygit`; none of `~/.amp`,
`~/.cursor`, `~/.goose`, `~/.droid`, `~/.factory`, `~/.config/zed` exists; and
`~/.claude/settings.json` still has 5 hook events (`PostToolUse`, `PreToolUse`,
`SessionStart`, `Stop`, `UserPromptSubmit`) and **0** TUIC references. The
`699-c6e0` premise and the hook-reinstall item are both unchanged.

- [ ] [HUMAN] Install any of `amp`, `cursor-agent`, `goose` or `droid` and capture an
  idle and a mid-turn screen for it (story `699-c6e0`). All four are offered as
  launchable agents (`src/agents.ts:188-275`) but none has a ready-screen adapter
  (`has_ready_screen_adapter`, `pty.rs:3245`), so a tab running one latches busy for
  the life of the process: OSC 133 marks the command busy once and nothing clears it.
  Measured 2026-09-06 — `command -v` finds none of the four binaries, none of their
  config dirs exists, and `src-tauri/src/fixtures/agent_prompts/` holds captures for
  claude and grok only. The adapter cannot be written from documentation: grok's first
  fixtures passed green while the real UI stayed stuck BUSY for 132s (story
  `523-1df4`). Per agent — install it, open a tab, `POST /diagnostics/capture` with
  `{"enabled":true,"session_id":"<id>"}`, sit at the idle composer, send one short
  prompt, let it finish, then `{"enabled":false}`; one `.tcap` spanning both states is
  enough. Hand the files over — writing the adapter is code work and stays on
  `699-c6e0`, not a check.

- [ ] [HUMAN] Under `make dev`, edit any file in `src/` to force a Vite full reload
  (story #716-031e). Every terminal pane must come back filling its pane, with no
  window resize: no small canvas in the top-left corner with black around it, and
  scrolling must show every row. Split a pane and reload again — both halves. Canvas
  geometry is not observable over HTTP, which is why this is by eye.

- [ ] [HUMAN] Reinstall the TUIC hooks from Settings → Agents, then confirm
  `~/.claude/settings.json` gains TUIC references (measured 2026-09-06: 5 hook
  events present, **0** TUIC references). Then trigger a real elicitation — a
  Context7 sign-in prompt will do — and check the tab badges as awaiting and
  clears on Accept or Decline. This is the only path that exercises the
  `Elicitation` → awaiting / `ElicitationResult` → busy map at
  `agent_hook.rs:45-68`, which has never run against a real Claude binary.
  Needs a human because the hook install and the elicitation are both user
  actions no endpoint can drive. **When it fires, capture it** —
  `POST /diagnostics/capture` — and hand the `.tcap` over; the fixture is code
  work and becomes a story, not a check.

- [ ] [HUMAN] In a release `.app`, hold `j`/`l`/`i` in vim: the cursor repeats and no
  accent picker appears. Option-key composition must still produce accented
  characters, and a user with `defaults write -g ApplePressAndHoldEnabled -bool true`
  must keep their override (the registration domain is lowest priority). Needs the
  release bundle domain — `press_and_hold.rs`, called from the `lib.rs` setup — and
  real key-repeat hardware. (#79)
- [ ] [HUMAN] Settings → Notifications → Attention → Test in a rebuilt app: the native
  engine matches the sample Boss approved on 2026-08-09 (triangular G4→G4→E5,
  75/75/140 ms, 50 ms gaps, gain 0.8) and stays identifiable from another room
  without being irritating. Audio, judged by ear.
- [ ] [HUMAN] Copy a long Claude message out of the terminal and paste it into Slack:
  no `▎` gutter and no gutter NBSPs, while lists, blank lines, indentation, `:wave:`
  and the body spacing survive unchanged. The text itself is asserted by nine Rust
  tests (`cargo nextest -E 'test(copied_selection)'`, `terminal_grid.rs:1687`); the
  paste is not. Tried twice from automation — `agent-browser clipboard read` fails
  with `Resource temporarily unavailable (os error 35)`.
- [ ] [HUMAN] Drag a file out of the file browser onto Finder, and drop a large folder
  from Finder into the app. The first is a real cross-application OS drag. The second
  confirms a **deliberate** gap, not a regression: `fs_transfer_paths` (`fs.rs:1624`)
  is still synchronous on the main thread because it is the drag-and-drop backend and
  D&D changes need Boss's approval.
- [ ] [HUMAN] Install `zed`, put a comment, a trailing comma and hand-tuned indentation
  in `~/.config/zed/settings.json`, install the bridge from Settings → Agents, and
  confirm Zed still starts, still shows every setting, and lists the `tuicommander`
  context server — `diff` against `<config dir>/mcp-backups/zed-settings.json.orig`
  must show only the added member. Then press **Remove all MCP integrations** and
  confirm each client lost only its `tuicommander` entry and a relaunch does not put
  it back. Zed is not installed here, and a real client reading the file afterwards
  is the one thing the splice tests cannot cover. (issue #115)
- [ ] [HUMAN] Compare OSC 133 gutter marks side by side, browser at `:9876` against the
  desktop app, on the same session: same rows, same size, neither client stealing the
  other's dirty rows. Canvas painting is not observable over HTTP, and both clients
  have to be visible at once. (Port corrected 2026-09-07 from `:9877` — only one
  instance runs, and it serves 9876; a browser pointed at 9877 gets nothing.)
- [ ] [HUMAN] Open `vim` or `htop`: the wheel still goes to the app, `Shift+wheel`
  scrolls TUIC history, and quitting restores the shell scrollback unchanged. The
  enter/exit half is covered by the `gh-run-watch.raw` replay test; mouse-reporting
  forwarding needs a real wheel. `lazygit` is not installed.
- [ ] [HUMAN] Print a fullwidth char and overwrite half of it — `printf '\e[1;5H中'`
  then `printf '\e[1;6HX'` — and confirm no ghost `中` survives beside the `X`. Scroll
  away and back to prove it is not just hidden by a later full-row reship. Canvas
  painting.
- [ ] [HUMAN] Raise an MCP `ui action=confirm` and answer it on a phone at
  `/mobile.html`: the desktop dialog must disappear by itself, and the reverse must
  work too. Then, with a push subscription registered and the PWA closed, confirm the
  push carries the title. Needs a real phone and a real subscription.
- [ ] [HUMAN] Comment a word that repeats many times in a markdown preview ("reason"
  ×18) and confirm the highlight lands on the occurrence you selected, and that
  selecting across an existing highlight hides "Add comment". The offsets are asserted
  in `tweakComments.test.ts`; where the highlight is *drawn* is not.
- [ ] [HUMAN] Run a real Claude turn with `tuic-voice` enabled: only prose is spoken —
  never a `Bash`, a path, a diff line, or anything at or below the input box — and the
  first sentence starts before the turn ends. Every filter stage is a heuristic and no
  real agent turn has ever been replayed through it. Turn on "Log every dropped line"
  and read `GET :9876/logs` for whichever rule misfired.
- [ ] [HUMAN] Boss's call: **Trim** the real `src-tauri/target` row in Build Cleaner. It
  is 58 GiB and a Trim forces a full rebuild of the running dev app, so no agent may
  run it. Trim against other repos' `target/` is covered.
- [ ] Rust change, needs a `make dev` restart (stories #5525 / #8c80). Switch to a repo
  that has never been indexed this session with `index_strategy` at its default
  `active_and_switch`, then check `GET :9876/logs` for `content index warm on repo
  switch` followed by `content index built` for that repo — until now nothing warmed a
  repo on switch, so the "and switch" half of the strategy did nothing. Then set the
  strategy to `active_only` in Settings → General and switch again: NO `content index
  warm`/`content index built` pair may appear for that repo (the skip itself is logged
  at debug level, so absence of the build is the check). Finally, with several repos registered
  and unindexed, run a cross-repo content search (`?` in the command palette, all-repos
  on) for a string that is not in the active repo: the empty state must read
  "N not indexed" and must NOT promise "retry shortly" for repos nothing is building.
- [ ] Rust change, needs a `make dev` restart (story #650-b0a0). With the app started
  while **Cloud Relay is off**, turn it on in Settings → Services with a valid relay URL
  and token: the status dot must go green with no app restart, and `GET :9876/logs`
  must show `relay: connecting to …`. Turn it off: the dot goes grey and the log shows
  `relay: shutting down` then `relay: stopped`. Turn it on again — the supervisor must
  still be watching after a stop. Then kill the relay server (or pull the network) while
  connected: every reconnect log must read `reconnecting in 1s` for the first attempt
  after each *successful* connection, growing 1→2→4… only across consecutive failures.
- [ ] Rust change, needs a `make dev` restart (story #656-2b63). Spawn an agent via MCP
  `agent action=spawn` with explicit `rows`/`cols` (e.g. 50x140), then confirm the tab
  renders the full screen: before this, the VT screen was built at a hardcoded 24x220
  while the child was handed the caller's geometry, so anything below row 24 (an agent's
  input box, a dialog footer) never reached the parsers. Then let that child exit and
  call `session action=wait session_id=<id> until=exited` **after** it has died: it must
  answer `{met:true, exit_code:N}` instead of `{"error":"Unknown session …"}`. A wait on
  an id that never existed must still fail fast with `Unknown session`. The same
  registration path now also backs `POST /agents` and browser/remote `POST /sessions`,
  so a browser-created terminal and an HTTP-spawned agent both need a smoke check.
- [ ] Rust change, needs a `make dev` restart (story #654-bfc1). Put a stub earlier on
  the resolved `gh` path (`/opt/homebrew/bin/gh` or `/usr/local/bin/gh`, whichever
  `resolve_cli` finds first) containing `#!/bin/sh` + `sleep 600`, unset `GH_TOKEN` and
  `GITHUB_TOKEN`, then launch the app: the window must appear at the usual speed instead
  of waiting on the stub. `GET :9876/logs?source=github` must then show
  "`gh auth token` did not answer in time" about 10s later (5s for the `gh_token` crate's
  own spawn, 5s for ours) and the app must stay usable with no GitHub token. Restore the
  real `gh`, relaunch, and confirm PRs/issues populate within a second or two without
  touching Settings — the deferred probe, not boot, is what fills them now, and it has to
  nudge the poller (`ForceResync`) to re-run the cycle it missed; a sidebar that stays
  empty for a full minute means that nudge did not land. Same stub check against
  `tuic-remote` (headless): the HTTP server must bind immediately instead of waiting on
  `gh`, and `GET /repo/issues` must answer once the probe lands.
- [ ] Rust change, needs a `make dev` restart (story #642-3741). `mod dictation_routes`
  was never declared, so 12 handlers never compiled, and 7 more COMMAND_TABLE paths hit
  no route at all. Against the restarted build: `curl :9876/dictation/status` and
  `/dictation/models`, `/dictation/devices`, `/dictation/config`, `/system/relay-status`
  must answer JSON (not a 404 or the SPA shell), and
  `curl ':9876/system/check-update?channel=nightly'` must return an `UpdateCheckResult`.
  Then open the app in a browser at `:9876` and use dictation end to end: record, stop,
  and confirm the transcript is injected — browser mode reads `inject_text` as a bare
  string now, not `{text}`. Last, set a non-default audio output in notification
  settings and trigger a notification from the browser tab: it must play on the chosen
  device, which is what the added `device` field in the HTTP body carries.
- [ ] Rust change, needs a `make dev` restart (story #670-b9a2). Grid delivery got three
  changes that only show up in a live WebView. (1) **Frame ordering:** zoom/resize a busy
  session repeatedly (the resize path cuts a FULL frame off-thread while the ticker cuts
  deltas) — no blank or half-stale screen may survive the zoom, and any frame that loses
  the race must come back as a full repaint on the next tick rather than vanishing.
  (2) **Browser not starved by a stalled desktop:** open the same session in the app and
  in a browser tab at `:9876`, block the app's JS thread (devtools breakpoint, or a heavy
  panel), and confirm the browser tab keeps painting at the normal rate instead of
  freezing with it. (3) **Desktop repair:** release that breakpoint — the app window must
  repaint the whole screen in one go, with no rows left stale from the frames it missed.
  Tests cover the Rust side of all three; what they cannot reach is the frame actually
  crossing `tauri::ipc::Channel` into the WebView.
- [ ] Rust change, needs a `make dev` restart (story #672-c1a3). Three always-on
  background costs from the boot audit: (1) the AI cron scheduler's 30s tick loop
  (`ai_agent::scheduler`) now only spawns when `ai-cron.json` has at least one enabled
  job, and stops when the last one is disabled/removed via `save_scheduler_config` —
  with zero jobs configured (the default), confirm no "Scheduler stopped"/tick log lines
  ever appear; add one enabled job via the Scheduler UI (or `PUT /ai/scheduler/config`)
  and confirm it fires on schedule; then delete/disable it and confirm the loop actually
  stops (no further tick activity) rather than continuing to poll. (2) Knowledge persist
  (`ai_agent::knowledge`) now skips the `spawn_blocking` dispatch on a 2s tick when no
  session has dirty knowledge — run a normal terminal session (commands recorded via
  knowledge tracking), confirm history still persists to `ai-sessions/` correctly (no
  regression from the skip). (3) `app_logger::init_tracing`'s file appender is now
  wrapped in `tracing_appender::non_blocking` instead of writing to `logs/tuic.log.*`
  synchronously on the calling thread (including from async tokio tasks) — confirm the
  daily-rotated log file still receives entries during normal use and on graceful
  shutdown (no lines silently dropped by the leaked `WorkerGuard`).
- [ ] Rust change, needs a `make dev` restart. Dictation speech gates: the transcriber
  now rejects a window in three steps — the RMS floor, Whisper's own
  `no_speech_probability`, and the phrase filter — and the first two read their
  thresholds from `dictation-config.json` (`rms_threshold`, `no_speech_threshold`)
  rather than constants. (1) **The reported leak:** with the headset a metre away,
  start dictation, say nothing, stop. No "Grazie"/"Thank you" may reach the terminal —
  before this change the repeated form ("Grazie. Grazie.") produced by the final
  full-buffer pass slipped past the filter, while the short streaming windows caught
  the single form. (2) **No over-rejection:** dictate a normal command and confirm it
  still lands, and that a sentence that merely starts with thanks ("Grazie, ora
  committa") is not eaten. (3) **Settings > Dictation > Voice tuning:** the meter must
  move with your voice, the marker must sit where the level gate is, "Start test
  recording" must show the transcript in the panel and never type it into the terminal
  behind it, and a rejected recording must print its reason ("Rejected: no speech
  detected (no_speech 0.91 > 0.60)"). (4) **Both sliders persist** across an app
  restart, and a `dictation-config.json` written before this change keeps the defaults
  (0.001 / 0.60) instead of reading 0.
- [ ] Rust change, needs a `make dev` restart. Per-tab agent resume (issue #119): with
  several Claude tabs open in the SAME folder, each tab must now hold its own session.
  Before this change discovery took "the newest unclaimed transcript in the project
  dir", so tabs stole each other's session or got none — measured live on 6 Claude
  tabs: 3 had `agentSessionId: null` and one held a different tab's id, which is why
  every tab resumed with `claude --continue` into the same conversation. (1) Open three
  Claude tabs in one repo, give each a distinct conversation, then check
  `curl -X POST localhost:9876/debug/invoke_js -d '{"script":"return
  JSON.stringify(window.__TUIC__.terminals())"}'` — every claude tab must show a
  DISTINCT non-null `agentSessionId`, and each must equal the `sessionId` in that
  tab's own `$CLAUDE_CONFIG_DIR/sessions/<pid>.json` (pid from
  `GET /sessions/<id>/leaf-pid`). (2) Quit TUIC (Cmd+Q), relaunch, click each resume
  banner: each tab must reopen ITS conversation, and `ps -ax -o args=` must show
  `claude --resume <uuid>` with three different uuids — not `claude --continue`.
  (3) Same check for a grok tab (binding comes from `~/.grok/active_sessions.json`).
  (4) No regression for Codex/Gemini, which have no pid registry and keep the old
  heuristic: a single Codex tab must still resume its own session.
- [ ] Frontend only, Vite HMR picks it up — no restart. Detached AI Chat window,
  story `700-4d5d`. Detach-panel windows are desktop-only, so none of this renders
  in browser mode and no agent can check it. (1) **The panel comes back:** open the
  AI Chat panel, click detach, then close the detached window. The docked panel must
  reappear. Before this it did not — `onDetach` never hid it, so the toggle on the
  way home flipped it off. (2) **The stream reaches the detached window:** with the
  chat detached, make the MAIN window start a conversation for that same terminal (a
  file watcher rule firing, an automation goal, or right-click the terminal →
  *Explain this error* — all three run on the main window's store, which renders
  nowhere while detached). The reply must now stream *in the detached window*, and
  must still be on screen after it finishes. (3) **The local stream wins:** type a
  message in the detached window and, while it is streaming, trigger a main-window
  conversation as in (2). What you asked for in the detached window must not be
  painted over. (4) **A live stream survives the homecoming:** repeat (2) and close
  the detached window while the reply is still arriving. The docked panel must come
  back showing the answer mid-stream; before this it came back blank and stayed
  blank until the stream finished. (5) **No cross-talk:** detach from terminal A,
  focus terminal B in the main window, and start a conversation on B. Nothing may
  appear in the detached window.
- [ ] **Rust change — needs a `make dev` restart** (or `make build`). Agent runs now
  persist with the conversation (story `705-57fa`). The backend stamps
  `schema_version: 3` and migrates older files on read. (1) **Existing conversations
  survive:** with saved chats already on disk from an older build, open a terminal
  that had one — the history must load as before, and
  `<config_dir>/ai-chat-conversations/<id>.json` must come back rewritten with
  `"schema_version": 3` after it is read once. Nothing may be lost or dropped.
  (2) **A run survives a reload mid-iteration:** start an autonomous agent goal in
  the AI Chat panel, wait until a tool card or two has appeared and the banner reads
  `Agent running — iter N`, then reload the window (browser mode: refresh; desktop:
  reopen the tab). The tool cards and the banner must come back as they were.
  Before this only the prose came back. (3) **A plain L1 chat file gains no `agent`
  block:** send a normal (assisted) message, then inspect the saved JSON — there
  must be no `"agent"` key.
- [ ] **Rust change — needs a `make dev` restart** (or `make build`). Session state is
  pushed, not polled (story `687-be9d`). The desktop no longer calls
  `list_active_sessions` on a 1 Hz timer; the backend emits `session-state-changed`
  once per real transition, on the Tauri window and on `/events` SSE. (1) **Badges
  still move:** with a claude tab open, send it a prompt — the tab must go busy, then
  show the awaiting badge on a question, then clear when answered, all as fast as
  before. Same for the Activity Dashboard. (2) **The poll is gone:** with the app
  idle and the window VISIBLE, `curl -X POST http://localhost:9876/diagnostics -d
  '{"enabled":true}' -H 'content-type: application/json'`, wait a minute, then
  `curl 'http://localhost:9876/logs?source=diagnostics'` — no periodic IPC at idle.
  Before this it polled once a second forever whenever the window was on screen.
  (3) **A reload still converges:** with a long-idle, silent agent tab, reload the
  window (desktop: Cmd+R / reopen). The badge must be correct immediately — that is
  the one mount-time `list_active_sessions` catch-up, the only call left.
  (4) **Browser parity:** open `http://localhost:9876/` in a browser and repeat (1);
  the SSE arm carries the same payload.
- [ ] **Rust change — needs a `make dev` restart** (or `make build`). Provider
  availability is now rendered (story `701-b6ac`). `detect_ollama` returns a
  `detail` string saying *why* the endpoint is unusable, and the Providers tab
  shows it. Visual confirmation is what is needed here — the states are covered
  by tests, the rendering is not. (1) **Reachable:** with Ollama running, open
  `Settings > Providers` with an Ollama provider configured — its row must show a
  green check icon and `Reachable`, and no reason line underneath. (2)
  **Unreachable:** stop Ollama (`pkill ollama`), reopen the tab — the row must
  show a yellow `(!)` icon and `Not detected`, with
  `Cannot reach http://localhost:11434 — is Ollama running?` underneath.
  (3) **Wrong port:** point the provider's base URL at a port serving something
  else and confirm the reason names the HTTP status instead. (4) The icons must
  be SVG glyphs, not emoji, and must recolour with the theme (check light theme).
- [ ] **Rust change — needs a `make dev` restart** (or `make build`). Block-display
  settings now persist (story `702-327a`). `show_block_timestamps`,
  `show_scrollbar_marks` and `block_folding_enabled` were absent from the Rust
  `AppConfig`, so serde silently dropped them from every `save_config` payload —
  the frontend wrote them and the next `load_config` returned nothing, and
  `?? true` restored the default. All three are now real fields. Verify:
  (1) **The toggles exist:** `Settings > General > Terminal` shows **Show block
  timestamps** and **Block folding**, both on. (2) **They persist across a
  restart** — this is the part the old build could NOT do: turn both off, quit,
  relaunch, reopen Settings; both must still be off. Cross-check
  `config.json` — it must now carry `"show_block_timestamps": false` and
  `"block_folding_enabled": false` (before this change those keys never appeared
  in the file at all). (3) **Timestamps obey the toggle:** with it on, hold
  Ctrl+Cmd over a terminal with several command blocks — a relative-time label
  appears at the right edge of each block's prompt row; with it off, nothing
  appears. (4) **Folding obeys the toggle:** with it off, Cmd+Shift+. and the
  `Toggle block fold` palette entry must both do nothing; with it on, both fold
  the block nearest the viewport centre. (5) **Nothing else regressed:** flip an
  unrelated setting (e.g. Copy on select), restart, confirm it also survived —
  the new fields must not have disturbed the config merge.

- [ ] **Show scrollbar marks toggle** (719-36af) — frontend only, so Vite HMR
  picks it up; no `make dev` restart needed. I could not screenshot it: the
  orchestrator instance on :9876 does not run this build, and no worktree dev
  instance was up. (1) **It appears:** `Settings > General > Terminal` now shows
  a third toggle, **Show scrollbar marks**, below **Block folding**, on by
  default — check it lines up with the other two and the hint wraps sanely.
  (2) **It is searchable:** type "scrollbar" in the settings search box; the
  entry must appear and jump to the Terminal section. (3) **It actually gates
  the marks:** in a terminal with several command blocks, turn it off — the
  blue/red block ticks **and** the green user-prompt ticks disappear, and they
  must go on the flip itself, not on the next scroll. (An early return used to
  skip the repaint that erases them, so they stayed painted forever; 723-6b02
  fixed that, and this is the check for it.) (4) **Search ticks must SURVIVE**
  — with the toggle OFF, run a terminal search (Cmd+F): the orange match ticks
  must still be drawn. A search that silently marks nothing is the failure
  723-6b02 exists to prevent; the flag covers command history only.
  (5) **It persists:** turn it off, restart, confirm `config.json` carries
  `"show_scrollbar_marks": false` and the toggle is still off.

- [ ] **Agent tool-log bound, measured on a real run** (718-aebf) — frontend
  only, so Vite HMR picks it up. This is the half of the story code could not
  close: criterion 1 asked for the file size and rewrite rate of a *real* long
  agent run, and no such run exists yet — the tool log itself landed this
  session (705-57fa) and the Rust half needs a `make dev` restart, so every
  conversation on disk predates it (8 files, largest 3.3 KB, newest Jun 3).
  After the restart, run one long autonomous agent session — the longer and the
  more tool-heavy the better — then:
  (1) **Size:** `ls -laS "$HOME/Library/Application Support/com.tuic.commander/ai-chat-conversations"`.
  The active conversation's `.json` must stay under ~560 KB. Above that, the
  512 KB ceiling in `conversationStore.ts` is not biting where it should.
  (2) **Rewrite rate:** watch the same file's mtime during the run (`stat -f %m`
  in a loop). It should move at most twice a second, and each write should now
  be a fraction of a megabyte instead of up to 4 MB.
  (3) **The log is still useful:** open the AI panel's tool cards on that
  conversation after a reload. The MOST RECENT tool calls must be there — the
  bound drops from the oldest end, so a run that trimmed shows a truncated
  history, never a stale one.
  (4) **Ordinary runs are untouched:** a normal short session should keep every
  tool card it produced; the cap was sized so only megabyte-scale output trims.

- [ ] **Scrollback reflow honours its Settings toggle** (660-d087) — **Rust
  change, needs a `make dev` restart.** Until now the grid reflowed scrollback
  unconditionally and `scrollback_reflow` had no consumer at either end, so
  this change adds the missing Settings control AND the backend wiring.
  (1) **Default is unchanged behaviour:** open Settings > General > Terminal.
  "Reflow scrollback on resize" must be ON for an existing install — the config
  key defaulted `false` before it had a consumer, so it was flipped to `true`
  (`#[serde(default = "default_true")]`) precisely so an upgrade does not
  silently change what the terminal does. Scroll back through old output after
  opening a side panel: lines should re-wrap, exactly as before this change.
  (2) **Off truncates:** turn the toggle OFF, then narrow the terminal (open a
  side panel or drag the split). Scrollback lines written at the old width must
  now be cut at the new width instead of wrapping onto extra lines. The visible
  screen must look the same either way — a cursor-addressed TUI (htop, vim)
  redraws itself and is never reflowed.
  (3) **It reaches sessions already open:** with several tabs running, flip the
  toggle and resize a tab that was created BEFORE the flip. It must follow the
  new setting without being recreated — `commit_config_change` pushes it to
  every live grid, and a change that only affected the next session is the bug
  this story was opened for.
  (4) **It persists:** flip it off, restart, confirm `config.json` carries
  `"scrollback_reflow": false` and the toggle is still off.

- [ ] **Headless daemon serves Claude usage again** (678-9a75) — **Rust change,
  needs a rebuild.** `claude_usage_cache` carried `#[cfg(feature = "desktop")]`
  while `build_router` mounts `/claude/usage` and `/claude/usage/timeline`
  unconditionally, so `cargo build --bin tuic-remote --no-default-features` did
  not compile at all. The gate is gone. After a rebuild, start `tuic-remote` and
  check `curl http://127.0.0.1:<port>/claude/usage` answers instead of 404/500.
  Desktop behaviour must be unchanged — the same endpoint on :9876 still works.

- [ ] **Frontend liveness watchdog + WebView reload escape hatch** — **Rust +
  frontend change, needs a `make dev` restart.**
  (1) **Quiet when healthy:** after the restart, `curl
  'localhost:9876/logs?source=diagnostics'` must NOT contain `Frontend
  unresponsive`. The beat runs every 5s, so a healthy app is silent.
  (2) **It fires:** block the main thread from devtools/invoke_js with
  `const t=Date.now(); while(Date.now()-t<40000){}` — within ~35s the log must
  carry `Frontend unresponsive: no heartbeat for 30s`, exactly ONE line, and a
  `Frontend responsive again` line once the loop ends.
  (3) **Sleep does not false-positive:** close the lid for a few minutes, reopen.
  `Sleep/wake detected` must appear WITHOUT a `Frontend unresponsive` next to it.
  (4) **The reload works and keeps sessions:** with several PTY tabs running,
  `curl -X POST localhost:9876/debug/reload_webview` → `{"ok":true}`, the UI
  repaints, and every session is still there with its scrollback.
  (5) **Browser mode is unaffected:** open `localhost:9876` in a browser; it must
  not beat (command is `INTENTIONALLY_UNMAPPED`) and must not produce errors in
  the console or 404s in the log.

- [ ] **Resume finds the session the alias hid** — **Rust + frontend change,
  needs a `make dev` restart.** Fixes `c2 --resume <id>` → `No conversation
  found with session ID` when the session belongs to the *other* config dir.
  (1) **Discovery captures the real command:** open a tab, launch Claude with
  `c2` (alias for `CLAUDE_CONFIG_DIR=~/.claude-private claude
  --dangerously-skip-permissions`), let it go busy→idle once, then check the tab
  carries the rebuilt string, not `c2`:
  `curl -s localhost:9877/... ` is not enough — read it from the store via
  devtools/`invoke_js`: `window.__TUIC__` terminal dump must show
  `agentLaunchCommand: "CLAUDE_CONFIG_DIR=/Users/stefano.straus/.claude-private
  claude --dangerously-skip-permissions"`.
  (2) **The resume works across dirs:** with that tab, switch branch away and
  back (or restart) so the resume command is offered. It must read
  `CLAUDE_CONFIG_DIR=… claude --resume <uuid> --dangerously-skip-permissions`,
  and running it must land in the SAME conversation — not `No conversation
  found`, not a fresh session.
  (3) **The `c` case still works:** repeat with the `c` alias (default
  `~/.claude`). The rebuilt command must have NO `CLAUDE_CONFIG_DIR=` prefix and
  must resume its own conversation, not the private-dir one.
  (4) **No regression without an alias:** a tab launched from the TUIC agent
  menu (run config, no alias) resumes exactly as before.
  (5) **Worktree seed caveat:** a tab auto-seeded with an inline prompt
  (auto-fix / conflict-assist) rebuilds with that prompt still in the command,
  so its resume re-sends it. Known and documented (`DEFERRED` in
  `rebuild_launch_command`) — confirm it is only cosmetic-annoying, and report
  if it is worse than that.

## WebView lost-document recovery + memory report (2026-09-08)

Needs a `make dev` restart — these are Rust changes and `make dev` runs
`--no-watch`.

1. **The reload endpoint navigates, not reloads.**
   `curl -X POST localhost:9876/debug/reload_webview` must answer
   `{"ok":true,"action":"navigate","url":"http://127.0.0.1:1421/"}` — not a bare
   `{"ok":true}`. The window must repaint and every PTY session must survive.
2. **The poller heals a lost frame by itself.** Force the failure the incident
   produced, from devtools on the main frame:
   `document.open(); document.write(""); document.close();` — or navigate the
   top frame to `about:blank`. Within ~15 s the log must carry
   `Main WebView lost its document` followed by `WebView recovery attempted`,
   and the app must come back with its sessions. Confirm it does NOT loop: a
   single recovery pair, then `Main WebView is back on the app`.
3. **A healthy app is never re-navigated.** Leave the app running for a few
   minutes and confirm the log has no `lost its document` line and the UI does
   not flicker/reload — an over-broad check would reload every 15 s.
4. **In-app routes survive.** Navigate around the app (settings, tabs, hash
   routes) and confirm no recovery fires.
5. **`GET /diagnostics/memory` names the structures.**
   `curl -s localhost:9876/diagnostics/memory | python3 -m json.tool` — the
   `maps` list must be sorted biggest-first, `grid.vt_log_buffers` must carry a
   plausible byte count for the open sessions, and `phys_footprint_bytes` must
   match `footprint -p <pid>` (NOT `ps` RSS, which reads far lower).
6. **The leak is still unattributed.** Leave the instance running through a
   normal working day, then compare `/diagnostics/memory` against the footprint.
   If `accounted_bytes` tracks the footprint, the named structure is the leak.
   If the footprint climbs far above `accounted_bytes`, the growth is outside
   `AppState` and the next suspect is the wry event-loop message queue.

## Workspace identity migration (725-b343) — needs a `make dev` restart

The repositories store is now keyed `workspaces: Record<WorkspaceId, WorkspaceState>`
instead of `branches: Record<string, BranchState>`, and `activeBranch` is now
`activeWorkspaceId`. Migration is an identity function (`workspaceId = branchName`),
so no persisted key moves — but it runs against Boss's real `repositories.json` on
first start, and `config.rs` changed, so **none of this is live until the Rust
backend restarts**.

**Restarted 2026-09-09 16:59. Items 1-3 verified against the live
`~/Library/Application Support/com.tuic.commander/repositories.json`; item 4 was
unverifiable as written and is corrected below.**

1. [x] **Nothing is lost on first start.** _(37 repos migrated, 0 integrity
   problems: every `activeWorkspaceId` indexes its own map — no dangling pointer —
   and for every entry `workspaceId == branchName == key`, which is what an
   identity migration must produce. 36 workspaces still carry their
   `savedTerminals` / `runCommand` / `ciAutoHeal`. Branch names containing a slash
   survived as keys unaltered (`feat/ai-fingerprint-coverage`,
   `POC-0001/fingerprint-native-12`) — sanitization applies only to newly minted
   ids, never to a migrated key.)_
2. [x] **The migrated record persists.** _(All 37 repos carry `workspaces` and
   `activeWorkspaceId`; `branches` and `activeBranch` appear on none of them.)_
3. [x] **No conflict storm.** _(`GET /logs?limit=2000` since the restart: zero
   `repository configuration conflict` and zero `Repository changes were not
   saved` at any level. The only repo-related warning is an unrelated GitHub
   404 cooldown. Re-check after a longer multi-window session — this is a
   fresh-boot buffer, not a full day's evidence.)_

## Content-index memory bound, incremental update and snapshots (2026-09-10) — **Rust, needs a `make dev` restart**

Background: since `412dc849` (2026-09-06) every repo switch warmed an index and
nothing ever released one, so the backend reached 40.7 GB across seven indices.
Three changes ship together — a memory bound with LRU eviction, an incremental
update that touches only the files that moved, and an on-disk snapshot so an
evicted repo reloads instead of rebuilding.

1. [ ] **The bound actually bounds.** With `index_memory_budget_mb` at its default
   `1024`, switch across ten or more registered repos, then read `GET :9876/logs`
   for `content index evicted to stay within the memory budget`. The backend's RSS
   must settle near the budget instead of climbing with every repo visited — check
   it in Activity Monitor or the in-app memory report, not by eye on the log alone.
2. [ ] **Eviction is invisible to a search.** Right after an eviction line names a
   repo, run a cross-repo content search for a string only in that repo. The result
   must arrive (the repo is rebuilt or restored on demand) and must never be a stale
   hit from before the eviction.
3. [ ] **An edit costs a file, not a corpus.** In a large repo already indexed, edit
   one file and wait past the 60-second rebuild cooldown. The log must show
   `content index updated incrementally` with a small `files=` count — not
   `content index rebuilt`. Then search for a word only in the edit: it must be
   found. This is the whole point of the change; a `content index rebuilt` here
   means the incremental path declined and the reason is worth reading.
4. [ ] **A big change still rebuilds.** Switch branches in a large repo (a checkout
   rewrites far more than a quarter of the corpus). The log must show
   `content index rebuilt`, and a search for a string introduced by the new branch
   must find it. Falling back here is correct, not a regression: the embedder's
   average document length is refitted only by a full build.
5. [ ] **Coming back to an evicted repo is cheap.** After a repo is evicted, switch
   back to it and read the log: `content index restored from snapshot`, and the
   restore must be visibly faster than the original `content index built` for the
   same repo. Check `<data_dir>/content-index/` holds one `.idx` per evicted repo
   and that the directory does not grow without bound across a long session.
6. [ ] [HUMAN] **A snapshot never serves stale content.** Evict a repo, then modify
   and delete files in it from outside the app, then switch back. The restored index
   must reflect the current working tree — the deleted file must not appear in a
   search and the modified file's new text must be findable. The snapshot is always
   validated against disk before use, and this is the check that it is.

## Unowned PTY tabs park in the Global Workspace (2026-09-10)

A session whose cwd belongs to no registered repo used to borrow a slot from the
ACTIVE repo, so its home depended on where you were standing: the two gate-os
worktree sessions landed under `brainstorming` and `tuicommander` respectively.
They now go to the Global Workspace instead, and leave it the moment a repo claims
the cwd. Frontend only — Vite HMR picks up the code, but the placement decision
runs during session adoption, so **reload the WebView** to see it applied to the
sessions already running.

1. [ ] **An unowned session lands in the Global Workspace, not the visible repo.**
   With `gate-os` still unregistered, reload the WebView while a gate-os session is
   alive. The tab must NOT appear in the tab strip of whatever repo is focused, and
   the "Global Workspace" entry must appear in the sidebar with a count that
   includes it. Click it: the terminal renders and is still attached to its PTY.
2. [ ] **Standing somewhere else changes nothing.** Switch to a different repo and
   reload again. The tab must land in the Global Workspace both times — the two
   gate-os sessions must end up TOGETHER, which is the whole bug.
3. [ ] **Register walks it home.** Click Register on the "Tab parked outside your
   repos" toast. The tab must move out of the Global Workspace and into `gate-os`
   under its worktree's branch, and the Global Workspace count must drop.
4. [ ] **One toast, not one per repo you visit.** The toast previously carried the
   active repo as its scope, which defeated the dedup: walking to another repo
   raised the same warning again there. With several unowned sessions from one repo,
   exactly one toast must be present, and moving between repos must not raise more.
5. [ ] [HUMAN] **A hand-promoted tab is not evicted.** Promote a normal, properly
   owned terminal to the Global Workspace by hand, then trigger a reconcile (add or
   remove a repo, or `cd` the terminal). It must STAY promoted — the unpromote is
   keyed on "was parked", not on "is promoted", and this is the check that it is.
6. [ ] **The active branch gets its own terminal now.** Where a borrowed tab used to
   satisfy "this branch has a terminal" and suppress it, an empty active branch now
   opens one of its own. Confirm this is the behaviour you want and not one extra
   terminal per launch that annoys you.

## Rust worktree API keys on workspace_id (story `726-5ac7`, 2026-09-10) — **Rust + IPC shape, needs a `make dev` restart**

The whole removal/dirtiness/archive path now resolves by opaque `workspace_id`
instead of branch name, and `get_worktree_paths` changed shape from
`{branch: path}` to `{workspace_id: {branch, path}}`. Under the identity
migration a git worktree's id **is** its branch, so nothing visible should
change — which is exactly why it needs eyes: a silent mismatch between the new
payload and the sidebar would look like nothing happening.

1. [ ] **Sidebar still lists every worktree.** After the restart, each repo's
   branch rows appear with their diff badges and merged marks intact. An empty
   sidebar with a live repo means the frontend failed to read the new
   `{branch, path}` value shape.
2. [ ] **Remove a worktree from the sidebar.** The row disappears immediately
   (the `worktree-removed` event now carries `workspace_id`, not `branch` — if
   the payload key were still misread the row would linger until a refresh).
3. [ ] **Merge & archive, then merge & delete a worktree.** Both must complete
   and the archived directory must still contain any uncommitted file, since
   `archive_worktree` now derives the archive folder name from the resolved
   record's branch rather than the caller's string.
4. [ ] **The dirty-worktree guard still asks.** Leave an uncommitted file in a
   worktree, then archive it. It must come back as a confirmation prompt, not a
   silent destroy — `worktree_dirtiness` is now id-addressed and this is the
   path that gates the irreversible part.
5. [ ] **Post-merge cleanup dialog with "keep worktree" unchecked.** The branch
   goes, the directory stays, HEAD detaches. `delete_local_branch` now takes a
   branch *and* a workspace id and refuses when they disagree.
6. [ ] **MCP `repo action=worktree_remove` now requires `workspace_id`.** A call
   passing only `branch` must be refused with a message naming `workspace_id`.
   Get an id from `repo action=worktree_list` first.

## Long dictation keeps its window tails (story `738-1e31`, 2026-09-11) — **Rust, needs a `make dev` restart**

The final whole-recording whisper pass used to run with the streaming flags
(`single_segment`, `no_timestamps`) at any length. Above one 30 s whisper window
those flags make `seek` advance a full window regardless of how much the decoder
reached, so every window silently lost its tail. The flags are now chosen by
audio length, and the no-speech gate filters per segment instead of discarding
the whole transcript on its worst segment.

1. [ ] **Dictate for more than 60 s.** The final text must not be shorter than
   the streaming partials that appeared while speaking. Check
   `GET http://localhost:9876/logs?source=dictation`: the `[accuracy]` line now
   reports `ratio=` instead of `match=`, and a ratio under 90% also logs a
   warning naming both character counts. A ratio at or above 100% is the normal
   case — streaming skips VAD-silent windows.
2. [ ] **A pause mid-dictation no longer eats the transcript.** Dictate, stay
   silent for several seconds, then keep dictating. Both halves must arrive; the
   silent stretch is now dropped as one segment rather than rejecting everything.
3. [ ] **Short dictation is unchanged.** A few seconds of speech still
   transcribes, and dictating into a silent room still produces nothing rather
   than invented subtitle boilerplate — that suppression relies on the flags the
   short path still sets.

## Session alias as the universal agent address (story `737-2150`, 2026-09-11) — **Rust + frontend, needs a `make dev` restart**

Every action that takes a `session_id`, and `agent action=send`'s `to`, now accept
three names for one terminal: the PTY id, the `tuic_session`, and the alias. The
alias also persists across a restart, and the session/peer list payloads dropped the
fields that answered a question nobody asked.

1. [ ] **Address a session by alias.** Take an alias from `session action=list`
   (e.g. `tu-1`) and call `session action=output session_id=tu-1`. The same call with
   that session's `tuic_session` must return the same terminal.
2. [ ] **`agent action=send to=<alias>`** reaches the peer that owns that terminal,
   with the same `delivery_path` as sending to its `tuic_session`.
3. [ ] **The alias survives a restart.** Note a tab's alias, quit the app, start it
   again, and check `session action=list`: the restored tab must hold the same alias,
   and a new session in that repo must get the next free number rather than reusing it.
4. [x] **Tab context menu copies the alias.** Right-click a terminal tab that has an
   alias. The menu shows `Alias: tu-1` under a separator; clicking it puts the alias
   on the clipboard. A tab with no alias shows no such item. _(verified: story
   `761-c847` retains event-before-binding aliases in `terminalsStore`; the focused
   store, listener, and TabBar tests pass 214/214)_
5. [ ] **`is_caller` marks the right tab.** From an agent running in a TUIC tab, call
   `session action=list`: exactly the caller's own session carries `is_caller: true`.
6. [ ] **Reading a dead session still works.** Let a session's process exit, then call
   `session action=output` on it. It must return the buffered output, not
   `Unknown session` — resolution falls through for a reference it cannot resolve.

## Named `tuic-remote` application instances (story `736-0afd`, 2026-09-11) — **Rust, needs a `make dev` restart**

`tuic-remote --instance <id>` now picks a separate config directory and OS-keyring
vault before it touches any state. The default path through `config_dir()` and the
credential vault moved behind the same seam, so the desktop app must be checked for
regressions even though it has no `--instance` flag.

1. [ ] **The desktop app still reads its own config.** After the restart, settings,
   repositories, GitHub token and MCP upstream credentials are all still there —
   `config_dir()` now goes through `app_instance`, and the default branch must
   resolve to the same platform directory as before.
2. [ ] **A named daemon starts empty and stays isolated.** Build the headless binary
   (`cargo build --bin tuic-remote --no-default-features`), run
   `./tuic-remote --instance build-host --set-password`, and check that
   `<platform config>/com.tuic.commander/instances/build-host/config.json` appears
   while the default `config.json` is untouched. A macOS Keychain entry must be
   created for service `tuicommander-instance-build-host`, not `tuicommander`.
3. [ ] **The password is per instance.** The password set for `build-host` must not
   log you into the default daemon, and vice versa.
4. [ ] **An invalid id fails loudly.** `./tuic-remote --instance Work-Laptop` and
   `--instance default` both exit 1 with `Invalid application instance …` and never
   bind the port.

## Mobile PWA lazy screens + IDE-icon split (2026-09-12) — frontend only, Vite HMR picks it up

`pnpm build` was failing on `main`: `dist/mobile.html` weighed 117 378 gzip bytes
against the 100 KB budget in `scripts/report-frontend-bundles.mjs`. Two causes, both
desktop weight leaking into the mobile initial load graph:

- `MobileApp.tsx` imported `ActivityScreen` and `SettingsScreen` eagerly although the
  app always opens on the `sessions` tab. Both are `lazy()` now.
- `IDE_ICON_PATHS` (33 inlined IDE/terminal SVGs) lived in `stores/settings.ts`, which
  mobile reaches through `ToastContainer` -> `stores/terminals` -> `stores/settings`.
  Moved to `stores/ideIcons.ts`; only `IdeLauncher` reads it.

Result: 101 941 gzip bytes, **459 bytes under budget**. The margin is thin on purpose
— see the note below.

1. [ ] **Mobile PWA still boots and the bottom tabs work.** Open `/mobile.html`, tap
   **Activity** and **Settings**: both must render (they now arrive over a second
   request). A blank tab means the `lazy()` chunk failed to load.
2. [ ] **The IDE launcher still shows its icons.** Desktop, Settings -> the IDE picker:
   all 33 entries must show their logo, not a broken-image glyph.
3. [ ] **Deep link into a session still works** (`/mobile/session/<id>`) —
   `SessionDetailScreen` is deliberately still eager.

**Known, not fixed:** mobile still pulls `stores/settings.ts` and with it the whole
`i18n/en.json` string table (13 KB gzip) although no mobile component calls `t()`.
The chain is `ToastContainer` / `utils/activitySnapshot` / `stores/toasts` ->
`stores/notifications` -> `stores/terminals` -> `stores/settings`. Cutting the
`terminals -> settings` edge would take ~26 KB gzip off mobile and give the budget
real headroom, but it is a core-store refactor and needs Boss's approval first.

## Orchestrator inbox wake after background probe (Rust — needs `make dev` restart)

1. [ ] Start a managed parent with a child, leave the parent shell visibly ready,
   and have the child send `RESULT` while the parent still has a pending
   background-process probe. When the probe settles, the parent must receive the
   payload-free `agent action=inbox` notice without closing the child or waiting
   for another lifecycle event. Reading the inbox must return the original
   `RESULT` payload exactly once.

## Codex usage in the active agent badge (frontend — Vite HMR)

1. [ ] With the Usage Dashboard feature enabled, focus a Codex terminal and
   confirm its `5h`/`7d` utilization appears beside the Codex icon in the bottom
   status bar, not as a second standalone ticker. Clicking the badge must open
   the Codex Usage dashboard. Switch directly from a Claude tab and confirm the
   old Claude percentages never appear under the Codex icon while the Codex poll
   is in flight.

## Plan and Stories external-plugin migration (Rust — needs `make dev` restart)

1. [ ] After restarting, Settings → Plugins → Installed lists Plan Tracker and
   Stories Ticker as ordinary external plugins, with no Built-in badge.
2. [ ] In a repository with `plans/` and `stories/`, a newly created Markdown
   plan opens in a pinned background tab and the status ticker shows the open
   story count.
3. [ ] Uninstall Plan Tracker, restart again, and confirm it is not recreated.
   Reinstall it from Browse after the `plan.zip` release asset is published.

## Project Progress corrupt-store recovery (Rust — needs `make dev` restart)

1. [ ] After the Project Progress reporting surface is wired, use a throwaway
   registered Git repository with invalid bytes at `.tuic/progress.sqlite3`.
   The first report must return `progress_store_recovered`, name a preserved
   `.corrupt-<uuid>` database artifact with the original bytes, and require a
   retry. Repeat with existing `-wal` and `-shm` sidecars and confirm all three
   named artifacts retain their exact original bytes under one unique recovery
   suffix. The retry must persist into a validated schema-v1 replacement and read
   the new event after another restart; it must not claim the empty replacement
   is the original history. Two simultaneous first reports must perform exactly
   one recovery: one reports recovery, the other succeeds against the replacement,
   and every later startup succeeds.

## Post-merge cleanup runs without freezing the window (2026-09-12) — **Rust, needs a `make dev` restart.**

`switch_branch`, `delete_local_branch`, `finalize_merged_worktree` and `close_pty`
were plain `fn` Tauri commands, so they ran inline on the macOS main thread. All
four now run on the blocking pool. Their HTTP twins were already correct, so this
is only observable in the desktop app.

1. [ ] **The window stays alive during Execute.** Open the post-merge cleanup
   dialog on a branch that has at least two terminals, check every step, press
   Execute: the spinner must animate, the sidebar must stay scrollable and the
   window must keep redrawing for the whole run. Before this change the whole
   WebView was frozen — cursor included — until the last step returned.
2. [ ] **The steps still report in order.** Each row must go running → done one
   at a time, with the same success/error wording as before; a failing step must
   still stop the ones after it.
3. [ ] **Closing a tab is still immediate and complete.** Close a terminal
   normally: the tab disappears, the process dies (no orphan `claude`/shell in
   `ps`), and a worktree-cleanup close still removes the directory.
## Remote-access credentials cannot be half configured (2026-09-12) — frontend only, Vite HMR picks it up

Boss's config held a password hash with an empty username, which made the server
answer every Basic Auth attempt from the phone with 401. The live config is
already repaired (`username: "admin"`); these checks cover the UI that produced it.

1. [ ] In Settings → Services → Remote Access, clear the Username field, type a
   password and press Tab. The Username field must fill in with `admin` by itself
   and `GET http://localhost:9876/config` must report that username — not `""`.
2. [ ] With a password set, clear the Username field and press Tab: it must snap
   back to `admin` instead of saving an empty username.
3. [ ] With no password set, an empty Username field must stay empty (it is only
   forced when a credential pair exists).
4. [ ] From the phone, open the LAN URL and log in with the saved username and
   password: the Basic Auth prompt must accept them and not reappear.

## Server request timeout no longer ties the confirm answer window (story `760-c29f`, 2026-09-13) — **Rust, needs `make dev` restart**

`REQUEST_TIMEOUT` (the outer HTTP layer every route runs behind, `mcp_http/mod.rs`)
and `CONFIRM_TIMEOUT` (`ui action=confirm`'s own answer window, `mcp_transport.rs`)
were both 300 seconds — an exact tie the outer layer could win, dropping the
handler's future before its cleanup ran. `REQUEST_TIMEOUT` is now 301 seconds,
a deliberate 1-second margin above the confirm window. Covered by an in-process
test that drives an unanswered confirm through the real `build_router` stack
(`mcp_http::tests::confirm_left_unanswered_resolves_clean_through_the_real_server_stack`),
but the actual dialog-dismissal behavior across every connected client can only
be observed against a rebuilt binary:

- [ ] Trigger `ui action=confirm` from an MCP client and let it sit unanswered
  past 300 seconds without touching any client. Confirm the requesting call
  receives `{confirmed:false, reason:"no answer within 300s"}` — not a bare
  HTTP 408 — and that the confirm dialog disappears on its own from every
  connected surface (desktop WebView, a browser tab, the mobile PWA) rather
  than staying stuck on screen.

## `tuic <dir>` refuses a temporary directory (story `763-d219`, 2026-09-13) — **Rust CLI, needs a `tuic` rebuild + reinstall**

The check lives in the `tuic-cli` binary, so neither Vite HMR nor a `make dev`
restart picks it up — the installed `tuic` on `$PATH` must be rebuilt (`make
build`, or `tuic install-cli` after a `cargo build -p tuic-cli`).

1. [ ] `tuic "$TMPDIR/scratch"` (after `mkdir -p "$TMPDIR/scratch"`) exits
   non-zero and prints the refusal naming the path, plus the `tuic new <path>`
   suggestion. Nothing new appears in the sidebar.
2. [ ] With `TMPDIR=$HOME/Gits/.tmp` exported — this repo's Rust-suite
   convention — `tuic "$HOME/Gits/.tmp/scratch"` is refused too. This is the
   case that proves the check reads the *caller's* `TMPDIR` and not the app's;
   it is the one of the fifteen observed ghost rows that an app-side check
   would have missed.
3. [ ] `tuic new "$TMPDIR/scratch"` still opens a shell there. Only
   registration is refused, never a session.
4. [ ] A real repository still registers: `tuic ~/Gits/personal/tuicommander`
   lands in the sidebar and activates as before.
5. [ ] A directory whose name merely *starts* with a temp root's name is not
   refused — e.g. `mkdir -p /tmpfoo && tuic /tmpfoo` on Linux, or any
   `~/Gits/.tmpfiles/repo`. Covered by
   `tuic-cli::tests::a_real_repository_is_not_disposable`, but worth one real
   run because that test cannot exercise canonicalization against the live
   filesystem.

## Stale-temp repository classifier + repair, and `TUIC_APP_INSTANCE` (story `763-d219`, 2026-09-13) — **Rust, needs a `make dev` restart**

Both live in `src-tauri/src/config.rs` / `lib.rs` / `app_instance.rs`, so
neither is loaded by Vite HMR — `make dev` must be restarted (or `make build`
for release) before any of this is observable.

**Items 1–3 are pre-staged; do not build the fixture by hand.** An isolated
instance is already seeded at
`<config dir>/instances/story763verify/repositories.json` with four rows: the
real `tuicommander` repo, one legitimate-but-offline git repo that must
survive, and two temp-root ghosts (`tuic-763-ghost-alpha`, `tuic-763-ghost-beta`)
shaped exactly like the classifier's own `stale_temp_repo_json` fixture. Run
`make test TUIC_APP_INSTANCE=story763verify` and check items 1–3 against it.
A desktop-feature build is required: `src-tauri/target/debug/tuicommander` was
last built without it and refuses to start the GUI, and `tuic-remote` serves no
frontend at all (`src-tauri/src/mcp_http/static_files.rs:10-11`). A debug build
reads `dist/` from disk first (`static_files.rs:14-17`), so the frontend itself
needs no recompile.

0. [x] A repository row is classified as missing only when filesystem metadata
   returns `NotFound`; permission, invalid-data, and other I/O errors preserve
   the row. _(verified: `config::tests::only_not_found_metadata_errors_prove_the_path_is_missing`
   exercises all four error kinds)_

1. [ ] With one or more genuinely stale-temp rows in `repositories.json` (path
   gone, under a temp root, `isGitRepo:false`, one empty shell workspace, no
   user metadata), the sidebar footer shows a red flagged-repo icon with a
   count badge; those rows do not appear as ordinary sidebar entries.
2. [ ] Clicking the badge opens the popover listing each candidate's display
   name; clicking "Repair N stale repositories" removes them, the badge
   disappears, and `repositories.repair-backup-<timestamp>.json` exists in the
   config directory holding the pre-repair document.
3. [ ] A legitimate repository whose path is temporarily offline/unmounted (not
   under a temp root, or not git, or holding any terminal/commit/metadata)
   never appears in that popover and renders normally in the sidebar.
4. [ ] `TUIC_APP_INSTANCE=story-763-verify make dev` (or an equivalent env-var
   launch) creates and uses `<config dir>/instances/story-763-verify/` —
   confirm via `GET /config/repositories` on that instance's port and by
   checking the directory on disk — and never touches the default instance's
   `repositories.json`. An invalid id (e.g. `TUIC_APP_INSTANCE=Default`)
   fails the process at startup with a clear error instead of silently
   falling back to the default instance.

## Rust dependency tree refresh (story `757-9ee7`) — **Rust, needs a `make dev` restart**

After rebuilding with `make dev`, confirm the running backend uses the refreshed
Cargo dependency tree; no frontend HMR reload can load these Rust changes.

## Project Progress reporting (story `750-d656`) — **Rust, needs a `make dev` restart**

After restarting an isolated `make dev` instance, report one milestone through
the MCP `progress` tool and confirm one `progress-recorded` SSE event appears and
the event remains available after reconnect. Repeat the exact report within 60
seconds and confirm the duplicate receipt produces no second event.

## Linked worktrees start WARM (story `767-3968`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [ ] Create a linked worktree of this repo through the dialog or
      `repo action=worktree_create` without `mode` or `dirty` fields.
      It must contain `node_modules/` and `src-tauri/target/` straight away, and
      the MCP/HTTP `instructions` payload must report
      `warm_artifacts.warmed_directories` > 0.
- [ ] `git -C <worktree> status` must still work after creation, and the
      worktree's `.git` must still be a FILE, not a directory.
- [ ] The ignored top-level FILES must NOT have been copied: no `.env`,
      `.mcp.json`, `CLAUDE.md` newly appearing in the worktree beyond what the
      branch tracks.
- [ ] `plugins/` (a submodule) and `src-tauri/plugins/claude-wakeup/` must not
      have been double-copied or left half-populated.
- [ ] Time it. Expect ~38 s on this repo; if it feels worse than a cold build,
      say so rather than living with it.
- [ ] Switch Settings → worktree storage to "inside repo" (`.worktrees/`),
      create a worktree, and confirm creation does not hang or recurse — the
      destination's own ignored ancestor must be skipped.
- [ ] Confirm Settings and settings search contain no copy-on-write workspace
      toggle, the create dialog has no mechanism or parent-changes picker, and
      the Worktree Manager has no clone badge or Publish action.

- [ ] After restarting `make dev`, verify Project Progress HTTP controls on the
      isolated test instance: pause rejects reports, resume accepts only new reports,
      and clear leaves an existing `progress.md` untouched. _(Rust backend change;
      requires restart to load.)_

## Project Progress panel (story `752-8492`, 2026-09-13)

Screenshots captured on an isolated instance (`TUIC_APP_INSTANCE=story752`,
HTTP `:9877`) in browser mode live in `~/Gits/.tmp/story752/shots/`.

- [x] Open Progress from the command palette in desktop-width browser mode.
      _(verified: **Open Project Progress** opens `#progress-panel`; shot
      `10-wide-populated-history.png`.)_
- [x] Populated list at desktop width, with a long summary that wraps inside the
      event body. _(verified: shot `10-wide-populated-history.png`, 395-character
      summary over six lines, kind badge right-aligned, `Source`/`Correct`/`Move`
      on the meta row.)_
- [x] Empty state with projects present. _(verified: shot
      `11-wide-empty-blockers.png` — **No progress matches this view.** under a
      live state card, with `Delete (0)` and `Merge` correctly disabled.)_
- [x] Paused state. _(verified: shot `12-wide-paused.png` — **Paused** in the
      state card, **Pause** became **Resume**, history still listed.)_
- [x] Unavailable project shown as an error beside working projects, not as a
      project without progress. _(verified: shot `13-wide-error-unavailable.png`
      — the project root was deleted underneath a running instance.)_
- [x] Narrow viewport with data. _(verified: shot `14-narrow-populated.png` at
      700x950 — scope row wraps, the tab strip scrolls, the error card and the
      long summary stay readable.)_
- [x] The bell exposes ONE aggregate Progress row that opens the panel.
      _(verified: shot `15-bell-aggregate-row.png` — "Project Progress / 9 unread
      changes across projects".)_
- [x] A live report shows exactly one toast that names project and workstream and
      carries an **Open Progress** action. _(verified: shot `03-live-toast.png`.)_
- [x] An unavailable project shows a red card with its name and the reason, above
      the list, instead of looking like a project without progress. _(verified:
      shots `00-error-cards-ownership-bug-prefix.png`, `02-narrow-error-state.png` —
      both captured before the two backend fixes, kept because they show what a
      total backend outage looks like in this panel.)_
- [x] The mobile Progress tab hosts the same panel full-bleed. _(verified: shot
      `04-mobile-progress-tab.png`. NOTE: the mobile shell never calls
      `repositoriesStore.hydrate()`, so the tab has no projects to scope and can
      only show the empty state — the layout is proven, the data path is not.)_
- [ ] Provenance, pagination (**Load older …**), and the destructive confirmation
      text, which name the scope and the count and state that existing
      `progress.md` exports are not deleted. Not captured: the seeded set was
      below one page and the confirmations are native `window.confirm` dialogs,
      which a screenshot of the page cannot show.
- [ ] While the panel is open, report another event and verify the displayed
      watermark stays frozen, the later event remains unread, and exactly one
      toast appears without a duplicate MESSAGES row.

## Project Progress ownership self-edge (story `752-8492`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [ ] Register any repository and open Progress. Every project must list its
      events. Before the fix, `resolve_owning_project_in` read the repository's
      own main workspace (`worktreePath` == repo root, no `parentRepoPath`) as an
      ownership cycle, so `progress_status` and `progress_list` failed for every
      registered project with
      `project_unavailable: managed workspace ownership cycle` and the panel
      showed nothing but red cards.
- [ ] A genuine two-project ownership cycle must still fail closed.

## Project Progress export (story `753-9998`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [ ] After restarting an isolated `TUIC_APP_INSTANCE`, select a project in the
  Progress panel, preview `progress.md`, and export it. Verify the preview remains
  usable at desktop and narrow/mobile widths and the file appears at the owning
  project root rather than the active worker workspace.
- [ ] Preview an existing `progress.md`, edit it externally, then choose Replace.
  Verify the stale write is refused and the external edit remains unchanged;
  preview again and confirm explicit replacement succeeds.

## Project Progress end-to-end journey (story `755-35c8`, 2026-09-13) — **Rust, needs a `make dev` restart**

The backend half of this journey is **done**, not pending. It ran live on a
rebuilt debug instance on `:9877` against throwaway projects `/tmp/pe-a` and
`/tmp/pe-b` (never Boss's repos); the evidence is in the 755-35c8 worklog. The
header this section used to carry — "the `/progress/*` routes are absent from
the backend that is running now" — described the state before `edd69ea7` moved
all ten routes into `shared_routes()`, and is kept here only so a reader who
remembers it knows it was retired rather than lost.

What is left is the half HTTP cannot observe: the **panel**. A projection that
is right over the wire and wrong on screen is a real failure mode, and no
assertion below can be promoted from the backend evidence.

- [x] In an isolated `TUIC_APP_INSTANCE`, register two projects. Report events
      into two workstreams in each, including one `blocked`.
      _(verified: two projects stayed separate with their own revision and
      unread count; "Shadow AI" projected `progressing` with 0 active blockers
      and "Windows Packaging" `blocked` with 1, from started/milestone/blocked
      reports.)_
- [x] Restart the instance. Verify the history, the workstream states, the
      active blockers, and the unread count all survive the restart.
      _(verified: the writing process 71345 was gone and pid 85275 read back 5
      events in order, both workstream states, revision 8, and `readCursor` 0 /
      `unreadCount` 5 — the unread cursor survived too.)_
- [x] Rename one workstream, then report again with the OLD workstream name and
      verify the event lands in the renamed workstream.
      _(verified behaviourally, and again through the post-restart process: a
      report using the pre-rename name landed in workstream `749fd0ff` and
      created no second workstream, so the aliases are durable.)_
- [x] Pause one project, report into it, and verify the receipt says `paused`
      and no event is recorded. Resume and verify the next report is recorded
      with no backfill.
      _(verified: paused receipt, no event, no revision bump; resume recorded
      the next report and did not backfill the paused one.)_
- [x] Clear one project. Verify its history, workstreams, and read state go
      away, that collection is paused, and that an existing `progress.md` at
      that project root is unchanged.
      _(verified: revision moved 1→2 rather than resetting, `collectionEnabled`
      went false in the same transaction, the exported `progress.md` hashed
      identically before and after, and repeating the clear with the stale
      `expectedRevision` was refused with `progress_revision_conflict`.)_
- [x] Preview and export `progress.md` and confirm the Markdown matches the
      status projection.
      _(verified: the preview was deterministic, its Markdown matched the status
      projection including the renamed workstream on historical events, and the
      write landed 780 bytes at the owning project root.)_

Still owed, and only these — all of them are about what is drawn:

- [ ] Open the Progress panel and confirm the **global scope** shows both
      projects with the right per-project state, and that switching to each
      project scope shows that project's workstreams and its blocked one.
- [ ] Report into a **paused** project while the panel is open: the receipt is
      already proven to say `paused`, but confirm no toast appears either.
- [ ] Correct one event through the panel's correction control (edit a summary)
      and confirm the panel and a fresh export both show the corrected text.
      This leg was never exercised: the live run covered the workstream rename,
      not an event correction.

## Protocol-ranked agent state (story `745-8ff1`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [ ] After restarting `make dev`, run an instrumented agent turn for longer
      than the ordinary silence threshold. A stale Ready repaint must not turn
      the tab idle before the agent's protocol completion signal arrives.
- [ ] Disable native/global status instrumentation for one agent and confirm
      its existing Ready-screen fallback still returns the tab to idle.

## Vendored fxhash in the bm25 fork (story `758-ff0d`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [ ] Before restarting, note a repo you have searched recently — its content-index
      snapshot on disk was written by the pre-vendoring binary. After the restart,
      run a content search in that repo (`?` in the command palette) for a word you
      know is in it. Results must appear immediately, with `GET :9876/logs` showing
      the snapshot being restored and NOT `content index rebuilt` for that repo. An
      empty result set with a successful restore is the exact failure the vendoring
      had to avoid: the persisted `token.index` values are fxhash32 hashes, so a
      drifted algorithm still decodes the file and then matches nothing.

## Progress Markdown export (story `753-9998`, 2026-09-13) — **Rust, needs a `make dev` restart**

Automated verification already covers the backend contract end to end (unit
tests plus a live HTTP run against a rebuilt debug instance on `:9877`:
preview → write → `progress_export_exists` → `progress_export_content_changed`
with the human edit preserved). What is left is what HTTP cannot observe.

- [ ] Open the Progress panel in the desktop app, pick one project, and check
      the export card against `docs/frontend/STYLE_GUIDE.md`: the source-metadata
      checkbox, the preview button, the revision line, and the scrolling
      Markdown preview block.
- [ ] Toggle "Include source metadata" while a preview is shown. The preview and
      its export button must disappear, because that snapshot can no longer be
      written.
- [ ] Export once, then export again. The second run must ask for confirmation
      before replacing the file, and the button must read `Replace progress.md`.
- [ ] Edit `progress.md` by hand between the preview and the write, then write.
      The panel must show `progress_export_content_changed` and your edit must
      still be in the file.
- [ ] After an export, run `git status` in that project: only `progress.md` may
      appear. Nothing under `.tuic/` may be listed.

## MCP instruction de-duplication (#754-affa) — needs a `make dev` restart

Rust-only change to `mcp_transport.rs`. Boss's live instance still serves the old
strings until the backend is restarted; nothing below can be checked before that.

- [ ] After restart, `curl -s localhost:9876/mcp/instructions | jq -r .instructions`.
      The `## Tools` section must hold three lines (the delegation sentence, the
      Worktrees rule, the Submit rule) and **no** per-tool bullet list; there must
      be no `## Workflow` section and no `**UI feedback:**` line. `## Multi-Agent
      Work` keeps the peer count and the isolated-branches bullet only.
- [ ] `ack` / `intent:` / `suggest:` markers must be byte-identical to before —
      they are protocol, and a reworded marker breaks the tab title and the
      suggestion bar. Compare against a capture of the old output if in doubt.
- [ ] In a connected agent, ask for the `repo` tool schema: its description must
      now document all nine `progress_*` actions, which it never did before.
- [ ] Watch one agent session for a turn. It must still emit `ack` exactly once
      per connection and `intent:` at each phase change — the markers moved not
      at all, but this is the cheapest way to notice if they did.

## Progress reachable from `tuic-remote` (#755-35c8 finding) — needs a `make dev` restart

Rust-only routing change in `mcp_http/mod.rs`: the ten `/progress/*` routes moved
from `build_router` into `shared_routes()`. Before this, a remote/PWA client
talking to a `tuic-remote` daemon got **404 on the whole Progress feature** — no
route at all, which looked like an auth failure. Tests cover route existence;
these check the live surface.

- [ ] Start a headless daemon: `TUIC_APP_INSTANCE=remote-check tuic-remote`, then
      `curl -u <user>:<pass> 'http://127.0.0.1:<port>/progress/status?path=<repo>'`.
      It must answer with a status body, not 404.
- [ ] Same call with **no** credentials from a non-loopback address must still be
      rejected by the auth middleware — the move must not have widened access.
- [ ] On the desktop instance, the Progress panel must behave exactly as before:
      the routes are merged into `build_router` through `shared_routes()` now, so
      a regression here shows up as the panel 404ing on every call.

## A turn closed by the foreground probe logs `activity_source=process` — needs a `make dev` restart

`foreground_probe` never constructed `ForegroundProbe::Quiet`, so every close
that the process table actually answered was logged as `agent-ready-screen`,
indistinguishable from a screen-only guess (#771-4733). Rust-only — the running
app keeps the old logging until restart.

- [ ] After restart, let an agent tab finish a turn with nothing running under
      it, then `curl 'http://localhost:9876/logs' | grep 'Shell state'`: the
      close must read `activity_source=process rank=Process`, not
      `agent-ready-screen`.
- [ ] A tab whose agent still has a `cargo`/`npm` child running when the ready
      screen appears must still close as `agent-ready-screen` — the probe must
      not claim an observation it did not make.
- [ ] After such a close, typing into that tab (or the agent resuming on its
      own) must turn it BUSY again. A tab stuck IDLE while the agent works is
      the regression this rank change could cause.

## A wake that could not start is retried at the next idle edge — needs a `make dev` restart

A `NotStarted` wake attempt burns the orchestrator wake budget for the whole
group, and only an inbox read restored it. So one draft in the composer, one
open question or one unconfirmed idle at the moment mail arrived silenced the
"you have mail" notice for the rest of the session: the mail sat in the inbox
and the master terminal was never told. A new BUSY→IDLE edge now re-arms the
budget before chasing the notice. Rust-only — the running app keeps the old
behaviour until restart.

- [ ] Type a draft into the orchestrator's composer (do not submit), have a peer
      `agent action=send` to it, then clear the draft and let the turn settle.
      Within a few seconds the orchestrator must be handed the
      `agent action=inbox` line. Before the fix nothing ever arrived.
- [ ] The payload must never appear on the orchestrator's screen — only the
      pointer to the inbox.
- [ ] A notice already being typed must not be duplicated by a concurrent idle
      edge: one wake per group, not two.
