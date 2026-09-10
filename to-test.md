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
4. [ ] **The plugin contract keeps returning a branch name, not an id.** Corrected:
   the original wording said to open "a plugin that renders the current branch",
   and no such plugin exists — `activeBranch` appears in exactly two places in the
   whole plugin surface, the `RepoSnapshot` declaration (`src/plugins/types.ts:166`)
   and the registry that fills it (`src/plugins/pluginRegistry.ts:366`, reading
   `workspace?.branchName ?? null`, not the map key). No shipped plugin consumes it
   today. Nothing to look at by eye; the obligation is that the FIRST plugin to read
   it gets a branch name once a COW workspace's id stops containing one, which is
   covered when `kind: "cow"` entries actually exist.

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
