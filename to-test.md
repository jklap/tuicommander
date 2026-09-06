<!-- tweak-comments v1: inline review comments.
     Format: [tweak:begin:ID]highlighted text[tweak:end:ID @ISO-TIMESTAMP
     comment body (free text, may span multiple lines)
     ] — where [ ] are the HTML comment delimiters <!-- -->.
     The only escape is '-->' → '--&gt;' inside the comment body.
     Read each comment, apply the feedback to the highlighted text,
     then remove the tweak markers. -->

# To Test

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

> **Where the restart gate sits (measured 2026-09-06).** The backend serving
> `:9876` is `src-tauri/target/debug/tuicommander`, PID 12931, started
> **2026-09-03 22:01:49**. The newest commit it can contain is `e4d7efd6`
> (09-03 17:37, `chore(release): 1.7.6`). **Every Rust change up to and including
> the 09-03 batch is live** — the entire 08-xx backlog of "needs a `make dev`
> restart" lost its gate here, months after some of those items were written.
> Everything from `9f4d9318` (09-04 16:49) onward — the ACP work and the whole
> 09-05 batch — is **not** loaded.
>
> Re-measure it, never inherit it:
> `ps -o lstart= -p $(pgrep -f target/debug/tuicommander)`, then
> `git log --until='<that time>' -1`. The binary's own mtime is useless — another
> agent may have rebuilt the file on disk without the running process restarting,
> which is the case right now (file built 09-05 23:34, process started 09-03).
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
- `cargo nextest` could NOT run: an in-flight edit elsewhere in the tree leaves
  `pty.rs:23285` calling `OutputRingBuffer::snapshot`, which does not exist. Every
  Rust claim below was settled by reading the code, not by running it. Re-run the
  Rust suite once the tree compiles.

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

## Per-repo settings survive a restart (2026-08-30, frontend only — HMR, but needs an app restart to prove)

Every per-repo override was being dropped on save: the store sent camelCase keys
to a snake_case Rust struct that has `#[serde(default)]` on every field, so serde
discarded them without a word. Only `path` and `color` — the two names that spell
the same in both conventions — ever reached disk. Overrides looked correct until
the next launch.

- [ ] Settings → a repository → set a per-repo override (base branch, or the
  worktree "Prompt for branch name during creation" toggle). Confirm the new
  value in `repo-settings.json` under the app config dir is written with the
  snake_case key and the value you chose.
- [ ] Restart the app. The override is still set in the UI and still applies.
- [ ] A repo you never customised still shows "(Global Default)" — the fix must
  not turn absent overrides into explicit values.

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

## Repository saves survive a concurrent diffstat change

Requires a `make dev` restart — the change is in `src-tauri/src/config.rs`.

- [ ] With two windows open on the same config, work in a repo so its diff counts
  keep moving (an agent committing is enough). Rename another repo, reorder the
  sidebar, add a repo. Each must persist. Before, `GET /logs` showed a stream of
  `Repository changes were not saved` / `repository configuration conflict`, and
  nothing was written.
- [ ] `GET http://localhost:9876/logs?level=error` shows no
  `Repository changes were not saved` entry over a working session.
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
- [ ] `curl http://localhost:9877/codex/usage` returns the JSON payload and
  contains **no** `email`, `user_id` or `account_id` field.
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
- [ ] `curl http://localhost:9877/codex/stats` contains **no** `profile` object
  (no username, display name or avatar URL).

## Terminal answers OSC 10/11/12 colour queries

**Rust change — needs a `make dev` restart.** Fixes the `^[[?6c` garbage and the
1.2 s probe loop: Claude Code asks for the background with `OSC 11 ; ? ST` +
`ESC[c`, and TUIC used to drop the colour query while answering the fence.

- [ ] With capture on (`POST /diagnostics/capture {"enabled":true}`), start
  `claude` in a tab and let it sit for a minute. The `.tcap` must show the
  `ESC]11;?` / `ESC[c` pair **once or twice at startup — not repeating every
  ~1.2 s**. This is the whole point of the fix.
- [ ] No `^[[?6c` text appears on screen at startup, and no stray `c2` / `6c`
  residue is left glued to the shell prompt or prepended to the next command.
- [ ] `printf '\033]11;?\033\\' | cat -v` in a shell tab prints an
  `ESC]11;rgb:....` reply whose colour matches the current terminal background.
- [ ] Switch to a light theme, then repeat the query: the reported colour
  follows the theme (the frontend republishes on remeasure).
- [ ] Only one publish per real theme change — `GET /logs` shows no burst of
  palette traffic when resizing the window with several tabs open.
- [ ] `curl -X POST http://localhost:9877/terminal/theme-colors -H 'content-type: application/json' -d '{"foreground":[255,0,0],"background":[0,255,0],"cursor":[0,0,255]}'`
  returns `{"ok":true}` and changes what the query above reports.

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
  `GET http://localhost:9877/logs?source=mcp_oauth` shows
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
- [ ] `curl -X POST localhost:9876/diagnostics -d '{"enabled":true}' -H 'content-type: application/json'`,
  wait 30s, then `curl 'localhost:9876/logs?source=diagnostics'` — the `HEALTH` line
  must carry a `state_lane=<n>` field, normally `0`. It must also appear on a
  `CPU SPIKE` line if one fires.

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

- [ ] Reconnect an MCP client that speaks an older revision. The `initialize`
  result must echo the version the client offered, not `2025-11-25`.
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

## Still needs a human

Every item here failed the ladder for a stated reason — real hardware, a second
application, a canvas no endpoint renders, or a judgement made by eye or ear.
None of them is here because nobody looked.

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
- [ ] [HUMAN] Compare OSC 133 gutter marks side by side, browser at `:9877` against the
  desktop app, on the same session: same rows, same size, neither client stealing the
  other's dirty rows. Canvas painting is not observable over HTTP, and both clients
  have to be visible at once.
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
