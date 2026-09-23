---
name: live-verify-debug-instance
description: >
  Live-verify a Rust-touching or UI change against a real running TUICommander
  debug instance (make dev) — safely, without disturbing Boss's orchestrator
  instance or real repos/sessions. Use whenever a change needs "does this
  actually work end-to-end" proof beyond unit tests: a new config field, a new
  HTTP/IPC command, a UI toggle, a git-state detector, anything that only a
  running app can confirm. Covers starting/stopping the instance by exact PID,
  preferring HTTP-API checks over UI clicks, and the click discipline needed
  when a UI check is unavoidable.
keywords:
  - make dev
  - debug instance
  - live verification
  - playwright
  - agent-browser
  - port 9877
  - curl HTTP API
---

# Live-Verify Against a Debug Instance

Unit tests prove the logic is correct in isolation. This skill is for the separate question —
does it actually work when a real `make dev` build runs it — without risking Boss's live
orchestrator instance, real repos, or real terminal sessions. Two incidents motivated writing
this down: a blind Playwright text-selector opened a real "Rename Branch" dialog on a live
worktree, and a blind `input` selector typed a scratch path into a real terminal's shell prompt.
Neither caused lasting damage, but both were avoidable — see AGENTS.md's "Web-UI testing with
agent-browser" section for the click-discipline rule this skill's UI-checking step follows.

## 1. Start the debug instance

The orchestrator (your own embedded instance) holds port 9876. Never touch it. Bring up a
**second, debug** instance instead:

```bash
lsof -iTCP:9876 -sTCP:LISTEN -Pn   # confirm the orchestrator holds 9876
lsof -iTCP:9877 -sTCP:LISTEN -Pn   # confirm 9877 is free

nohup make dev > /tmp/tuic-dev-verify.log 2>&1 &
disown
```

Poll for the port instead of a fixed sleep:

```bash
for i in $(seq 1 60); do
  lsof -iTCP:9877 -sTCP:LISTEN -Pn >/dev/null 2>&1 && { echo "up after ${i}s"; break; }
  sleep 2
done
```

Immediately record the exact PIDs you'll need to kill later — `make dev` spawns four processes
(`target/debug/tuicommander`, `tauri.js dev --no-watch`, `vite.js`, `tsc --noWatch`):

```bash
ps aux | grep -E "target/debug/tuicommander|tauri.js dev|vite.js|tsc --noEmit" | grep -v grep
```

**Never use a broad `pkill` pattern** (e.g. `pkill -f tuicommander`) — it can match the
orchestrator's release binary too and disconnect your own MCP session. Kill only the exact PIDs
you recorded.

## 2. Prefer the HTTP API over clicking the UI

Most things worth verifying — a config field round-trip, a new command's response shape, a
detection function's output against real git state — can be checked with `curl` against the
debug instance's own HTTP API (`https://127.0.0.1:9877/...`, self-signed cert: `curl -sk`), with
zero UI-interaction risk. Check `docs/api/http-api.md` / `docs/api/tauri-commands.md` for the
route. Two examples from real use:

```bash
# Config round-trip
curl -sk https://127.0.0.1:9877/config | python3 -m json.tool

# A git-state detector, against a scratch repo you created (never a real one)
curl -sk --get "https://127.0.0.1:9877/repo/structure" --data-urlencode "path=$SCRATCH_REPO"
```

Build the scratch state with plain `git` commands in a throwaway directory (e.g. under your
scratchpad) — never in one of Boss's real repos. This is strong evidence: it proves the exact
backend logic runs correctly against a real running instance, not just a test harness.

If the change only manifests in the frontend (a toggle's visual effect, a re-render), HTTP alone
can't confirm it — move to step 3.

**A fresh `TUIC_APP_INSTANCE=<id>` debug instance's HTTP server defaults to disabled.** Launching
via a named instance ID (for real config isolation — see AGENTS.md's isolation caveat) gives that
instance its own `config.json` under `instances/<id>/`, and `services.server.enabled` starts
`false` in it. `curl localhost:9877` (or whatever port it retries to) gets nothing until you edit
that instance's own `config.json` to set `services.server.enabled: true` (and pin an explicit
`port` to avoid 9876→9877→9878 retry ambiguity), then restart the instance. This is easy to
mistake for the instance failing to start at all — check that config file before assuming a
startup failure. (Confirmed twice in real use, 2026-09-21.)

## 3. If you must drive the UI: scope every click

Prefer `agent-browser` (stealth wrapper, `@ref` CDP clicks resolved against a live accessibility
snapshot — see AGENTS.md). If it's unavailable and raw Playwright is the fallback, apply the same
discipline `@ref` gives you for free:

- **Screenshot or snapshot before every click.** Don't click blind.
- **Scope every locator to a specific container**, never a bare tag or text-match against the
  whole page. A page-wide `input:not([type])` can match a *terminal's* hidden keyboard-capture
  input just as easily as a dialog's field. A page-wide text-match on a branch name can land on
  its double-click-to-rename hit area instead of whatever you meant to click nearby.
  ```js
  // Bad: matches the first input anywhere on the page
  const input = page.locator('input[type="text"]').first();

  // Good: scoped to the dialog you just opened
  const dialog = page.getByText("Add Repository", { exact: true }).last().locator("xpath=../..");
  const input = dialog.locator("input");
  ```
- **Never click directly on a branch/repo name label** in the sidebar — it's a double-click-to-
  rename target. Click a badge, icon, or the row background instead if you need to interact near
  it at all.
- **If something goes wrong** (text typed into the wrong place, an unintended dialog opens): stop,
  do not guess a fix by clicking more. Use the HTTP API to inspect and correct real state — e.g.
  `POST /sessions/{id}/write` with `{"data":""}` (Ctrl+U) to clear a terminal's input line,
  then confirm via `GET /sessions/{id}/terminal/lines?start=0&end=10` that the prompt is clean.
  Report the incident plainly rather than silently working around it.

Save screenshots under `.screenshots/<feature-name>/` in the worktree (gitignored) — see
AGENTS.md's Visual section.

## 4. Clean up before stopping

- **Config**: if you changed any setting for testing, restore it via the HTTP API before
  stopping — debug and release builds share the same `config.json`/`repositories.json`
  (AGENTS.md's isolation caveat). `PUT /config` with the original values, then `GET /config` to
  confirm.
- **Repos**: don't leave a scratch repo registered in the app (`GET /config/repositories`);
  don't touch or add terminals in Boss's real repos.
- **Terminals**: if you accidentally wrote to a real session (see step 3's recovery), verify it's
  clean via `/terminal/lines` before moving on.

## 5. Stop the instance

Kill the exact PIDs recorded in step 1, never a pattern match:

```bash
kill <tuicommander_pid> <tsc_pid> <vite_pid> <tauri_pid>
sleep 2
ps aux | grep -E "<pids>" | grep -v grep     # confirm gone
ps aux | grep -i tuicommander | grep -v grep # confirm the orchestrator is still there, untouched
```
