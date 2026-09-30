## SSH daemon provisioning, confirmed (wip 2ba33a3f5 replay) — Rust restart required

- [ ] After Boss's next planned backend restart and a matching `tuic-remote` release for this version, against a disposable SSH host in an isolated `TUIC_APP_INSTANCE`: a Never-deploy "Remote Server — SSH" with "Offer to start the remote daemon if it is not running" and no daemon running shows **Start remote daemon…**; the dialog lists the exact commands; Cancel runs nothing; Accept starts it (with Instance ID: `tuic-remote --instance <id>`, `tuic-remote-<id>.pid`) and connects; Disconnect stops it (unless "Leave it running" is checked). Against a daemon with no password, **Set remote password…** sets the saved one, a second attempt reports "already has credentials; nothing was changed", and the daemon needs a restart to use it. No desktop instance or SSH host was used by this lane; Rust does not hot-reload.

## Nested SSH model migration (wip afc10a2c9 replay) — Rust restart required

- [ ] On Boss's next planned backend restart, check `<config_dir>/connections.json` and `<config_dir>/tunnels/*.toml`: each SSH entry now has a nested `ssh` block, and a `*.pre-nested-ssh-<timestamp>.bak` beside each rewritten file holds the original bytes. Then Connect an existing SSH remote machine and start an existing tunnel — both must behave exactly as before (same host, port, identity, compression). A second restart must create no new `.bak`. Rust does not hot-reload; this lane did not restart the live app.

## Progress blocked badge supersede (1537-6c4b) — Rust restart required

- [ ] After Boss restarts `make dev` (or installs a rebuilt release), have an agent call `progress type=blocked` and confirm its tab shows the orange waiting dot; then have the same agent call `progress type=done` and confirm the dot clears while the agent keeps working. A real open dialog (for example a Claude AskUserQuestion) must stay orange after a `progress done`. Rust changes do not hot-reload; no desktop instance was launched by this lane.

## AI Chat provider retry line (1536-1c9c, ego 288-f994)

- [ ] [VISUAL] After ego `fix/288-provider-retry` is merged and installed, cause an overloaded or 503 provider (or wait for a real one) in AI Chat and take a screenshot: one orange line `… — connection problem, retrying in Ns (attempt n/6)` that updates in place, disappears when the answer starts, and gives way to the turn error after 6/6, with the next prompt accepted. Frontend only (HMR); ego must be rebuilt.

## Workflow daemon executor (1446-ff21 slice B) — Rust restart required

- [ ] On Boss's next planned backend restart, load the owner-locked workflow runtime and confirm existing run history remains available. The daemon tests cover graph position, control fences and idle deadlines; Agent delivery and start controls remain disabled. Rust does not hot-reload. This lane does not restart the live app or launch a desktop instance.

## Windows CI remaining regressions (1518-d3a7) — Rust restart required

- [ ] After Boss rebuilds/restarts the backend, confirm Windows archive hooks can run Git and short-name uploads publish or skip an existing destination without replacement. Native Windows CI owns the automated proof. No desktop instance was launched; Rust changes do not hot-reload.
## Main build integration — Rust rebuild required

- [ ] Load the cfg and GitHub lint fixes on Boss's next planned backend rebuild/restart. Rust does not hot-reload; this lane does not restart the live desktop. The fixes preserve config recovery and GitHub emission behavior.

## Telegram channel adapter (1438-79b4) — headless rebuild required

- [ ] After Boss authorizes mint deployment and the bound agent is idle, rebuild/restart tuic-remote and verify registration/replacement/unregister, automatic retirement on agent exit with the shell still open, MCP session end and PTY close, and the allowlisted phone conversation: no-agent reply "Nessun agent registrato", Thinking/activity refresh, exact final reply, authored done/blocked notices and opaque-button callback mail. The Rust changes do not hot-reload. No live token/chat-ID reads, mint deployment or desktop restart were performed by this lane. Offline targeted tests use fake credentials and chats; deterministic publish receipts remain outside this slice.
## Optional dictation build graph (1394-ff2c) — Rust restart required

- [ ] After Boss restarts `make dev` or installs a rebuilt release, confirm push-to-talk, hands-free speech, notification output-device selection, and the global window hotkey still work. Tauri builds enable dictation explicitly; plain Cargo desktop builds omit it. Rust changes do not hot-reload. This lane does not restart the live app or launch another desktop instance.
## Remote update cookie migration (1490-e3ac) — Rust rebuild required

- [ ] After Boss rebuilds/restarts the daemon, confirm a Direct remote update works with the current client. This release accepts both `tui-session` and the legacy query token; the client switches next release. Rust backend changes do not hot-reload. No desktop instance was launched by this lane.

## Windows Clippy cleanup (1501-e8cb) — rebuild required

- [ ] After the next Windows rebuild, confirm agent executable discovery still prefers `.exe` over `.cmd`, Chrome registry discovery works, and an upload completes. The Rust syntax cleanup does not hot-reload into the running desktop; Boss controls the next restart.

## HTTP API origin boundary (1456-351c) — Rust restart required

- [ ] After Boss restarts `make dev` or installs a rebuilt release, confirm existing CLI/MCP Unix socket access, authenticated phone/PWA access and desktop remote peer access. Rust does not hot-reload; no desktop instance was launched by this lane. Foreign Origin/Host rejection and token-authenticated clients are covered by targeted router regressions.
## Release-check lint cleanup (1447-a894) — Rust rebuild required

- [ ] After the next backend rebuild, verify capture/resampling, echo cleanup, loudness and Edge speech still work. These lint-only edits do not hot-reload; existing automated regressions need a final run after the managed background launcher is restored. Do not restart the live desktop from this lane.

## Bodyless IPC replies (1416-8ad4) — rebuilt clients required

- [ ] Rebuild/reinstall the CLI and bridge before using this decoder fix. The running clients do not hot-reload Rust changes. Automated shared-decoder regression covers 204/304, protocol-switch, bounded headers/interims and final response boundaries; no desktop restart was performed by this lane.

## Shared IPC instance routing (1390-cd42) — Rust rebuild required

- [ ] After Boss restarts the rebuilt backend and replaces the bridge/CLI binaries, use a disposable named headless instance. Run `tuic --instance <id> ls --json` and `TUIC_APP_INSTANCE=<id> tuic-bridge`: both must reach that instance. An explicit `TUIC_SOCKET` must still win. Do not launch a second desktop instance. The existing live Rust process does not hot-reload these changes.
## Stable MCP bridge (1415-ef32) — Rust restart required

- [ ] After Boss restarts `make dev` or installs a rebuilt release, confirm the primary instance migrates Claude and private Claude MCP commands to `mcp-bridge/<sha256>/tuic-bridge` under its config directory. Start a disposable Claude session and confirm MCP initialize succeeds. Existing desktop Rust code does not hot-reload. Target cleanup and executable lifetime are covered by the targeted regression tests; real Claude startup after the desktop restart remains to check.
## Remote hand-launched agent detection (1420-f3de) — Rust restart required

- [ ] After Boss approves and loads the rebuilt desktop and remote daemon, use the configured Mac-mint connection through desktop MCP only: create a disposable shell PTY, start Claude by hand, confirm `agent_state` appears, submit one task after its composer is ready, send mail with a payload-free wake and read the reply. Check that a plain shell rejects submit with a cause and a corrective action. Do not restart or redeploy Mac-mint while its live PTYs must be preserved. Coordinator harness: `scripts/test-remote-mcp.py` from story 1419.

## Suspend Tab (1358-d008) — desktop menu

- [ ] In the desktop app, right-click an idle agent tab: **Suspend Tab** is enabled; while the agent works or asks a question it is greyed out. Click it: the tab keeps its place and shows `zz`, the tab body shows "Suspended" with a Resume button, `ps` shows no process for it. Right-click it: **Resume Tab** is offered; click: a new shell opens in the same folder and the agent resumes its conversation. Suspend, quit and restart TUICommander: the tab is restored still suspended and not auto-resumed. Suspend a plain idle shell tab and repeat. _(Browser-mode part checked by tuic-1358-suspend; the Tauri window menu itself is not.)_

## Markdown Live mode (1278-33f5)

- [ ] Open a `.md` file with headings, `**bold**`, `*italic*`, `` `code` ``, a link, a fenced ```js block and at least one tweak comment (add one from the viewer). Click **Live**: marks disappear off the cursor line and reappear on it; the tweak highlight shows amber with the comment on hover; the caret jumps over hidden tweak markers; Backspace/Delete beside a highlight never leaves a stray `<!--tweak:...` when you press **Live** off and read the raw file. Select text, **Comment**, type, Enter: the same `<!--tweak:begin/end-->` format as the viewer writes. Cmd+F searches the text. Edit, click **Save** (or Cmd+S), `git diff` shows only the lines you touched. Make no edit on a CRLF file and save an edit: line endings stay CRLF. _(PARTIAL 2026-09-30: verified marks hidden off cursor line/reappear on it, amber tweak highlight, Backspace/Delete beside highlight leave markers intact, Cmd+S saves and git diff shows only the touched lines, comment format `@ts\nbody-->` as tweakComments.ts. NOT verified: caret jump, Cmd+F, CRLF. FAILED: after clicking Comment the input is not focused (activeElement stays cm-content), so typing replaces the selection; after Enter a stray newline follows the end marker (CDP Enter may cause it). Fixture ~/Gits/.tmp/to-test-0930/live-fx)_

## MCP reaper refresh race (1259-62e3) — Rust restart required

- [x] After restarting `make dev` in an isolated `TUIC_APP_INSTANCE=<id>`, keep a disposable MCP bridge with a stable `x-tuic-session` active across the one-hour idle boundary and send a ping near a maintenance sweep. Confirm no reap log for the refreshed protocol session and that `agent action=inbox` still resolves its identity. The deterministic race and expiry cases are covered by Rust tests; the running backend does not hot-reload this fix. _(verified 2026-09-29: by code/test inspection, tests not executed here: Idle refresh vs reap covered by test: mcp_http/mod.rs:8186 'refreshed session must no longer meet the idle deadline'. A live 1h run adds nothing.)_

## Worktree removal recovery (1258-e9ba) — Rust restart required

- [x] After restarting `make dev` in an isolated `TUIC_APP_INSTANCE=<id>`, remove a disposable clean landed worktree with ignored build artifacts through `repo worktree_remove`. Confirm the directory is gone before the branch disappears. The running backend does not hot-reload this Rust change. _(verified 2026-09-29: fixture repo ~/Gits/.tmp/tuic-validate/fx/repo, MCP/HTTP on tuic-remote --instance validate: clean in_sync worktree with warmed target/ removed via repo worktree_remove: {ok, removal_rule:in_sync}; directory gone, branch gone, git worktree list clean (order of dir vs branch removal not observable); t_wt.py)_

## Named debug vault test (1181-2f80)

- [x] Rebuild the headless test binary and confirm a named instance writes its seeded session token to its own credentials file without changing the default file. _(verified: `app_instance_cli` whole-module run passed 11/11 after rebuilding; `named_debug_vault_ignores_and_does_not_mutate_default_legacy_entries` asserts both files.)_

## Global AI Chat (1157-1e54)

- [ ] After a `make dev` restart, open AI Chat and switch between three repositories: `ps` shows no new `ego acp` process and the tabs stay. Send a message: exactly one `ego acp -C ~/Gits` starts, and ego's answer knows which repository was on screen. Reload the webview and send again: still one process. Quit TUICommander: no `ego acp` survives. _(NOT VERIFIED 2026-09-29: Needs real ego binary (`ego acp`) and a live AI Chat agent conversation.)_
- [ ] After a Rust restart, open the same AI Chat peer simultaneously from desktop and a remote browser in an isolated instance; both views must attach to one `ego acp` process and show the same conversation. _(NOT VERIFIED 2026-09-29: Needs a real `ego acp` process (real ego binary/account); dual attach from desktop+browser cannot be done with a fake.)_

## ego MCP over ACP (1156-1b61)

- [ ] After a `make dev` restart and an ego build that advertises `mcpCapabilities.acp`, open AI Chat in an isolated `TUIC_APP_INSTANCE=<id>` and ask ego to list terminals. `ps` shows no `tuic-bridge` child of ego, the app log shows no `MCP initialize` line per tool call, and the tool answers. _(NOT VERIFIED 2026-09-29: needs a real ego agent (ACP) session; not available headless)_

## AI Chat replay storm (1151-4243)

- [ ] After a `make dev` restart, open AI Chat on a repository with two saved tabs in an isolated `TUIC_APP_INSTANCE=<id>`. The app log shows one `ACP attach` line with `method=session/load` per tab, not a repeating stream, and a refused load shows the error with Retry instead of re-sending. _(NOT VERIFIED 2026-09-29: Needs ego executable and real ACP session/load; log check of 'ACP attach' only meaningful with real ego.)_

## Crate split restart

- [ ] Restart `make dev` after the `tuic-terminal`, `tuic-core`, `tuic-git` (including its GitHub domain), and `tuic-dictation` crate splits. Rust changes do not hot-reload in the running development instance. After the restart, use an isolated `TUIC_APP_INSTANCE=<id>` to check GitHub PR status and CI notifications, push-to-talk transcription, and one hands-free spoken reply with real audio; the current live backend still has the previous crate layout. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Restart-of-make-dev item; instance is a headless tuic-remote of HEAD bef15c20f. Backend crate-split behaviour exercised across r0 items (sessions, worktrees, git); UI part needs desktop build.)_

## Ego notice cards (1075-05dc)

- [ ] After the next desktop rebuild, with an ego build that publishes notices (story 171-cb51), open AI Chat on an idle session and let a worker finish so ego publishes a notice between turns. A bordered card appears in the transcript with a title, the notice text and one button, apart from the agent's last reply. Click **Open result**: the result file opens in a TUIC tab. A normal reply without `_meta` still renders as plain assistant text. For `answer` and `approve` cards the button only scrolls to the open question or permission below. Check that look and that behaviour. _(Component and store tests pass; no live ego session was available.)_

<!-- tweak-comments v1: inline review comments.
     Format: [tweak:begin:ID]highlighted text[tweak:end:ID @ISO-TIMESTAMP
     comment body (free text, may span multiple lines)
     ] — where [ ] are the HTML comment delimiters <!-- -->.
     The only escape is '-->' → '--&gt;' inside the comment body.
     Read each comment, apply the feedback to the highlighted text,
     then remove the tweak markers. -->

# To Test

## Stable macOS dev executable (1510-03ae) — next Boss launch

- [ ] On Boss's next manual `make dev` restart, confirm the printed executable path is `~/Library/Application Support/com.tuic.commander/dev-bin/tuicommander`, the live process maps that existing file, and LAN/tailnet HTTP requests and the iPhone page work with ALF enabled. Confirm bridge startup and local remote-update fallback still find their adjacent binaries. Script tests cover target deletion without launching the desktop; the live firewall and phone checks remain for Boss. No desktop restart was performed by the peer.

## Remote file drops (1434-1719) — Rust restart required

- [ ] After Boss restarts the desktop and updates the remote daemon, drop a Mac file and a folder onto a registered remote repository in tree and flat views. Verify remote bytes, unchanged local sources, directory confirmation and conflict skipping. This native Finder-to-Tauri interaction requires the desktop rebuild; no second desktop instance was launched.

## Safe orphan cleanup countdown (story 1257-a30b) — Rust restart required

- [ ] In an isolated `TUIC_APP_INSTANCE=<id>` after `make dev` restart, create a disposable detached linked worktree whose HEAD is on a branch and has no tracked or untracked changes. In Ask mode verify the dialog counts down from the configured number and removes it; repeat with Keep and Escape and verify it remains. Add an untracked file and verify the dialog names the reason and never counts down. While a clean dialog is open, answer `repo action=orphan_cleanup_answer path=<repo> decision=keep` through MCP and verify it closes without removal. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: No pending orphan cleanup exists headless; orphan_cleanup_answer exists but dialog and countdown are UI.)_

## Queued Claude notice confirmation (story 1251-9ec8) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated instance, queue a notice to a disposable Claude session with the UserPromptSubmit hooks enabled. When Claude accepts it after more than one second, confirm the notice appears without an "Agent input was not confirmed" toast. A notice left in the composer must still show the uncertainty toast after the six-second bound. The running backend does not hot-reload this Rust change. _(NOT VERIFIED 2026-09-30: partial — Real Claude with own --settings hooks: 3s SIGSTOP freeze wake delivered, uncertain=false, msg in transcript. Not observed: >1s confirm notice in UI, left-in-composer case (9s freeze still delivered). Headless lacks agent-hooks assets so hooks were my fixture.)_
## Shared agent mail identity (story 1246-46e3) — Rust restart required

- [ ] After rebuilding `make dev` in an isolated `TUIC_APP_INSTANCE=<id>`, use a disposable managed PTY with two MCP bridges asserting its durable tab UUID and PTY UUID. Send distinct messages to each UUID and to the PTY display name, once through `tuic mcp` and once through the MCP client. Confirm both bridges read every message in `agent action=inbox`, `list_peers` shows one recipient for the PTY, and reconnecting one bridge leaves the inbox readable. Boss's live Rust backend does not hot-reload this change. _(NOT VERIFIED 2026-09-30: partial — Headless PTY S: 2 MCP bridges (x-tuic-session=S) each read all 6 messages sent to UUID, display name and alias, 3 via MCP client and 3 via 'tuic mcp'; list_peers 1 entry for S; reconnecting a bridge keeps inbox readable (7 msgs). NOT tested: distinct durable tab UUID vs PTY UUID (HTTP session create)_

## Managed child idle close (story 1209-cc47) — Rust restart required

- [ ] After restarting `make dev` with an isolated `TUIC_APP_INSTANCE=<id>`, spawn a disposable managed agent child and set its per-agent idle-close delay to 1 minute. Let it become idle and confirm the parent receives an `idle_timeout` notice before the child terminal closes. Confirm its worktree remains. Send a follow-up before a second child's delay ends and confirm the timer restarts; confirm a user-created terminal and a managed child marked keep-open stay open. Run a disposable `tuic bg` job with an unreachable queue and mail path; its failed wake marker must keep the child open. The current live backend and installed CLI need a rebuild to load this change. _(NOT VERIFIED 2026-09-30: partial — Managed grok/pi children exited on their own after long idle (exit_code null); idle_timeout inbox notice, 1-minute per-agent delay, keep_open and worktree-stay not verified; no notice seen in parent inbox.)_
## Native desktop notifications — Rust restart required

- [ ] [HUMAN] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, leave TUICommander unfocused and trigger an agent question and a Progress `done` entry. Confirm each appears once in macOS Notification Center, with the terminal or project name; clicking each brings TUICommander to the named terminal or Progress project. Confirm a focused window produces none. This requires real cross-app focus and Notification Center; the running Rust backend cannot load the native handler without restart. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: macOS Notification Center with real cross-app focus needs desktop app (headless tuic-remote has no native notification handler); not automatable here.)_
- [ ] After restarting `make dev` with an isolated `TUIC_APP_INSTANCE=<id>`, spawn a disposable managed agent child and set its per-agent idle-close delay to 1 minute. Let it become idle and confirm the parent receives an `idle_timeout` notice before the child terminal closes. Confirm its worktree remains. Send a follow-up before a second child's delay ends and confirm the timer restarts; confirm a user-created terminal and a managed child marked keep-open stay open. Run a disposable `tuic bg` job with an unreachable queue and mail path; its failed wake marker must keep the child open. The current live backend and installed CLI need a rebuild to load this change. _(NOT VERIFIED 2026-09-30: partial — Same as 57: idle-close of managed children seen (grok/pi exited idle) but idle_timeout notice to parent not observed; desktop notification part is UI.)_

## Background wake retry (story 1233-4738) — rebuild the Rust CLI

- [x] After rebuilding and reinstalling `tuic`, run a disposable `tuic bg` command against an isolated instance. If the instance temporarily stops answering, check that `<log>.wake` shows `retrying` with `tuic_session` and `attempts`, then `queued` or `mailed` after recovery. The installed CLI cannot load the Rust change until rebuilt. _(verified 2026-09-30: Python unix-socket proxy started 3s late as TUIC_SOCKET: /usr/local/bin/tuic bg .wake went {status:retrying,tuic_session,attempts:3,error:'queue: Cannot connect...'} then {status:queued,attempts:5}. (kit tuic lacks bg; used /usr/local/bin/tuic, same crate source).)_

## CLI MCP worktree timeout (story 1240-8438) — rebuild the Rust CLI

- [x] After rebuilding and reinstalling `tuic`, use an isolated test instance to create and remove a throwaway worktree through `tuic mcp repo`. Confirm both commands report the server result after a request longer than three seconds. A CLI socket read timeout must warn that the server may still complete the action. The installed CLI cannot load this Rust change until rebuilt; restart a live `make dev` process only when ready to end its current sessions. _(verified 2026-09-30: tuic mcp repo worktree_create on 60k-file repo returned server result after 86.7s, worktree_remove after 35.8s ({ok:true,removal_rule:in_sync}). Delaying replies 5s via unix-socket proxy gave 'tuic: TUICommander reply timed out; the server may still complete the action...' at 3.006s.)_

## Mobile Files and editor (story 1225-60e7)

- [ ] On a 360×800 phone PWA, open Files and long-press a repository path: the full path should appear without opening the repository. Open a deep folder and confirm the header keeps its final folder name visible, while paths ending in `src/.claude/` keep the slash on the right. Check that ordinary folders precede hidden folders; search for a file in a nested folder and open it. In View and Edit, Back, file name, and actions should share one row with touch-sized buttons; the editor should fill the space above the bottom tabs and wrap long lines. Return to a session with an unsent draft and confirm Browse Files did not submit or change the draft. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Claude dismissed question (story 1213-82e1) — Rust restart required

- [x] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE=<id>`, open a disposable Claude session and trigger `AskUserQuestion`. Dismiss it with Esc, wait for Claude's ready composer, and confirm the awaiting badge disappears and `session action=submit` accepts a command. The running backend does not hot-reload this Rust change; the recorded PTY capture test covers the state transition and submit write after idle settlement. _(verified 2026-09-29: by code/test inspection, tests not executed here: Replay test with recorded capture claude-askuser-esc-20260929.tcap; clear path pty.rs:7155-7192 and pty/tests.rs:15518/15538 (protocol-question-cleared). Item says so itself.)_

## Mobile session header (story 1203-fe36)

- [ ] [HUMAN] After the frontend reloads, open Codex and Claude sessions on a 360×800 phone. Confirm each shows the correct 24 px logo and state dot, the display name remains readable, and the 56 px header leaves the terminal starting near y58 with no lost rows. Tap the name, Tasks, and overflow Progress to inspect the temporary sheets; check Files, Search, Ideas, Commands, and terminate remain reachable from overflow. Automated component tests cover the button actions and session binding; a real phone check remains. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## AI Chat prompt parking (story 1228-becb)

- [ ] On a 360×800 phone PWA, type a draft in AI Chat and tap Park. Send a different prompt and confirm the draft returns; repeat with an image preview and after a page reload. Check that switching to Sessions retains the same visible terminal row count. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Mobile global AI Chat (story 1208-b371)

- [ ] On the phone PWA after `make dev` serves this frontend, tap Chat with multiple repositories registered. Confirm there is no repository picker, the same titled conversations as desktop appear, and a push link opens its conversation without changing the chat root. Switch to a session and confirm the terminal keeps the same visible row count as before this change. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Mobile terminal states (story 1211-e1f4)

- [ ] On a 360×800 phone PWA, check a working, idle, awaiting-input and completed-unseen terminal in the session list. Verify the corresponding blue, green, orange and purple status colors. Open the completed session: the header should show Idle and the terminal should retain the same visible row count as before this change. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Dictation Metal release link (story 1198-535b) — Rust rebuild required

- [ ] After rebuilding `make dev`, verify macOS dictation starts with a downloaded Whisper model and still uses Metal. The build script change cannot affect Boss's running backend until a rebuild and restart; the targeted release test covers linking and loudness timing. _(NOT VERIFIED 2026-09-30: blocked — Needs downloaded Whisper model, microphone audio and Metal GPU use on desktop build (audio hardware blocked).)_

## Mobile slash commands (story 1199-7b8d)

- [ ] On the phone PWA, open disposable Claude Code and Codex sessions. Type a supported slash command (`/help` in Claude, `/status` in Codex) and press Send; confirm it opens once. In each session, type a slash prefix, choose that command from the suggested slash menu, then press Send; confirm it opens once. In Claude, submit `/model` with an argument and confirm the argument reaches the command. Confirm the Codex quick-command widget offers `/status` and that it opens Status. The current `make dev` frontend must reload the new bundle before this check. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## PTY close reason logging (story 1194-31e8) — Rust restart required

- [x] After restarting `make dev` with an isolated `TUIC_APP_INSTANCE=<id>`, close a disposable shell session and confirm the app log records `reason=close_requested` with its session ID. Kill a second disposable shell session through MCP and confirm `reason=kill_requested`. The running backend does not load this Rust change until restart. _(verified 2026-09-29: MCP session action=close on disposable shell -> /logs source=session shows reason=close_requested with session id; MCP action=kill -> reason=kill_requested with id. (HTTP DELETE /sessions uses a different path, logs only 'explicit close'.))_

## Queued agent submission confirmation (story 1163-5bed) — Rust restart required

- [ ] After restarting `make dev` with an isolated `TUIC_APP_INSTANCE`, queue a message for a disposable Codex session while a stop hook delays the next Working screen by about four seconds. Confirm the message reaches the transcript without an uncertain-delivery toast. A silent composer must still report uncertainty after the bounded wait. The running backend does not load story 1239-dca9 until restart. _(NOT VERIFIED 2026-09-30: partial — Codex, 4s SIGSTOP freeze: enqueue typed:true after 4.1s, uncertain=false, message reached transcript. Toast not observable headless.)_
- [ ] After restarting `make dev` with an isolated `TUIC_APP_INSTANCE`, queue a short command for a disposable Codex session while it is busy. When it becomes ready, confirm that the command starts a turn. If the composer retains the text instead, confirm that TUICommander reports uncertain delivery with an error toast and `session status` shows `delivery_uncertain=true`. Repeat with the installed Claude, OpenCode, Goose, Grok, and pi binaries. The running backend does not load this Rust change until restart. _(FAILED 2026-09-30 story 1299-3ce1: Claude, Codex, grok, pi: busy-queue then delivered turn OK; Codex silent composer: 6.29s, delivery_uncertain=true, text retained. OpenCode (--mini): initial prompt and queued cmds never drained, state stuck, prompt_delivery_failed. Goose: text retained with uncertain=false, queue stuck. Toast unveri)_
- [x] In that disposable session, leave one uncertain queued command in the composer and queue a second. Confirm the second waits. Press Enter once for the retained command, wait for the next ready prompt, and confirm the second is delivered once. Check that its toast says to inspect the transcript and composer before acting. Amp, Cursor, and Droid still use the legacy PTY-write result until live screen captures establish a confirmation signal. _(verified 2026-09-30: Codex silent composer (SIGSTOP): first cmd retained uncertain, second queued:1 waited; one Enter ran the retained cmd, second delivered once after next ready prompt.)_

## Concurrent config saves (story 900-43dd) — Rust restart required

- [ ] After a manual `make dev` restart with an isolated `TUIC_APP_INSTANCE=<id>`, open two windows, change different agent and UI preferences from the same loaded state, and confirm both persist after reopening. The live Rust backend does not hot-reload; targeted Rust and frontend tests cover the merge and request shapes. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Two-window UI preference persistence; config save API not modified by hand per rules.)_

## Current MCP tool results (story 1190-75eb) — Rust restart required

- [ ] After a manual `make dev` restart and sidecar rebuild in an isolated `TUIC_APP_INSTANCE`, open a disposable ego PTY session and call `search_tools`. Confirm it lists TUIC tool names without a protocol error. Disconnect the test MCP endpoint and confirm a current-protocol `tools/call` reports the unavailable error without a result-shape error. Targeted HTTP and bridge tests cover the wire fields; the running backend and installed sidecar cannot load this Rust change until restart or rebuild. _(NOT VERIFIED 2026-09-30: partial — search_tools via MCP works on this instance (needs non-empty query, returns tool listing, no protocol error). No ego configured (ego_executable empty), so ego PTY session and disconnect part not testable.)_

## Worktree removal with sealed build output (story 1179-50a8) — Rust restart required

- [x] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, remove a disposable worktree whose ignored target contains a read-only nested directory. Confirm the checkout and Git registration both disappear, while a symlink target outside the worktree keeps its contents and permissions. Targeted Rust tests cover this behavior; the running backend cannot load the Rust change until restart. _(verified 2026-09-29: fixture repo ~/Gits/.tmp/tuic-validate/fx/repo, MCP/HTTP on tuic-remote --instance validate: target/ilink -> outside dir plus target/ro/deep with mode 555: worktree_remove ok; checkout and registration gone; outside dir kept keep.txt and mode 750; t_wt2.py)_

## CLI install lint (story 1185-790d) — rebuild required

- [x] After rebuilding `tuic`, the macOS elevated install path still uses the target's parent directory; the Linux path compiles without an unused binding. _(verified: `src-tauri/crates/tuic-cli/src/main.rs:1179` gates only the parent binding with `target_os = "macos"`; macOS and Linux-target Clippy both pass with `-D warnings`.)_

## Remote manual update guard (story 1183-7154) — Rust restart required

- [x] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, start an automatic update for a disposable remote daemon and try a manual update through IPC, HTTP, or MCP. Confirm the backend reports "remote update already in progress". Repeat with a manual update running first: another manual request is rejected and automatic update is skipped. Targeted Rust tests cover these races; the running backend cannot load this Rust change until restart. _(verified 2026-09-29: by code/test inspection, tests not executed here: Guard exists at remote_runtime.rs:451 and test asserts the error at remote_runtime.rs:2781; item itself says targeted Rust tests cover the races.)_

## ACP peer mail receipt (1176-e82e) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE=<id>`, open a disposable ego conversation and subscribe it to `tuic://inbox`. Send ordinary and urgent peer mail to ego, including after `agent register orchestrator=true`, and confirm `agent send` returns `delivered:true` with `delivery_path:acp_inbox_resource` (and `urgent_delivered:true` for urgent mail); confirm ego receives the inbox updates. Disconnect ego and verify a separately registered offline peer still reports `inbox_only`. The live backend cannot load this Rust change until restart; targeted MCP tests cover the subscription and notification path. _(NOT VERIFIED 2026-09-30: partial — No ego configured (ego_executable empty) so ACP ego inbox subscription not testable; peer mail send/urgent paths verified in other items with real agents.)_

## Already unregistered worktree cleanup (1175-71fc) — Rust restart required

- [ ] After a manual `make dev` restart, use an isolated `TUIC_APP_INSTANCE=<id>` and a disposable repository to remove a worktree while its build-input warming is pending, after Git has already unregistered the checkout. Confirm the pending status clears even if a leftover directory cannot be removed. The running backend cannot load this Rust change until restart; the targeted Rust test covers the cleanup failure path. _(NOT VERIFIED 2026-09-30: partial — Could not reproduce the internal state via API: with warm pending, git worktree remove --force (leftover dir undeletable) drops the entry from GET /worktrees/paths and worktree_remove returns 'No workspace found'; the internal warm token is not observable, so the clear-on-unregistered path (remove_w)_

## Push-to-talk sustained speech (story 1135-b600) — Rust restart required

- [ ] [HUMAN] After restarting `make dev` with an isolated `TUIC_APP_INSTANCE`, record a short noise burst, ordinary speech, and quiet genuine speech with push-to-talk. Confirm that only sustained speech reaches the prompt and that a rejected capture shows `no sustained speech` in the dictation ring. The synthetic command tests cover duration and threshold boundaries; the real microphone separation remains unverified and is tracked by story 1117. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: push-to-talk with real microphone speech (audio hardware blocked).)_

## Push-to-talk final skip reason (story 1140-28e3) — Rust restart required

- [ ] [HUMAN] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, use a microphone capture that triggers a final RMS or Whisper speech gate. Confirm the dictation status and ring show the specific gate reason. An empty successful transcript still shows `no speech detected`. The focused Rust command tests prove response mapping without a microphone; live audio remains unverified. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: needs microphone capture to trigger RMS/Whisper gates (audio hardware blocked).)_

## AI Chat persistent approval (story 1147-33ec)

- [ ] **[HUMAN]** In an isolated AI Chat conversation, trigger a permission request offering both Allow once and Allow always. Confirm Allow always appears enabled and visually distinct from a disabled control, then click it and confirm the persistent option is selected. The component test verifies the click and success-color token; automated screenshot attempts timed out in agent-browser, and macOS denied Screen Recording to both capture tools. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## AI Chat layout and composer (story 1166-ef2f)

- [ ] In an isolated AI Chat conversation, confirm the tool count and status remain on one line at the panel's normal width, raw shell commands appear only after expanding a call, and Copy has room in both message types. While at the bottom, stream an answer and confirm the typing dots stay visible; scroll up and confirm the view stays put. Paste over 200 words and an image, then confirm the compact marker expands to the full prompt on Send and the image preview is removable. Targeted component tests cover these behaviors; the mandated stealth browser wrapper timed out on screenshot and snapshot commands for this worktree fixture. _(NOT VERIFIED 2026-09-29: Needs a real ego (ACP) AI Chat session streaming an answer; stealth browser wrapper reportedly unreliable)_

## CLI sidecar replacement (story 1165-e905) — Rust restart required

- [ ] After a manual `make dev` restart when current sessions may be discarded, install or update `tuic` from Settings in an isolated `TUIC_APP_INSTANCE`. Confirm `tuic --version` runs and a previously running CLI process is unaffected. The live backend cannot load the Rust installer change until restart; fixture tests cover replacement through hard links and symlinks. _(NOTE 2026-09-29: partial evidence only — Installer replacement covered by fixture tests (hard links/symlinks) per item; live install would overwrite Boss's installed /usr/local/bin/tuic, so not done from an isolated instance.)_ _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Settings CLI install is UI; /usr/local/bin/tuic (Sep 29 build) runs and talks to the instance socket, but the sidecar replacement flow was not exercised.)_

## AI Chat message Copy and trailing suggestions (story 1150-4042)

- [ ] In an isolated AI Chat conversation, confirm a message shows Copy on hover and keyboard focus, and a reply ending with `suggest: [ Retry | Show status | Diagnose ]` displays three buttons without the raw token. Targeted component tests cover the parser and keyboard reachability; a browser CSS fixture confirms visibility on hover and focus. _(NOT VERIFIED 2026-09-29: Live AI Chat reply from ego needed to render suggest buttons; parser/keyboard already covered by component tests.)_
- [ ] In the same conversation, confirm Pause, Resume, Compact and New remain on one row as icon buttons at the panel's normal width, have tooltips, and the model summary shows only the name after the final `/`. _(NOT VERIFIED 2026-09-29: Needs a real ego AI Chat conversation (same conversation with Pause/Compact/Copy); ego not available headlessly.)_
- [ ] Send a typed AI Chat message and choose a suggested reply; confirm each user bubble shows the text once after ego replies. Targeted reducer and panel tests cover both paths. _(NOT VERIFIED 2026-09-29: needs a real ego agent (ACP) session; not available headless)_

## AI Chat failed turns and choice buttons (story 1139-7310) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, open a disposable ego chat and send a prompt that fails before producing a reply. Confirm its diagnostic appears in the transcript and the composer returns to Send. Answer a two-choice trust elicitation using its direct button and confirm the turn then shows a reply or a failure. The live backend cannot load the ACP failure event until restart; targeted Rust and frontend tests cover the wire and rendering behavior. _(NOT VERIFIED 2026-09-30: partial — No ego/ACP agent configured; failed-turn diagnostic and choice buttons need ego and UI.)_

## AI Chat shared ACP prompt queue (story 1079-fe88) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, open the same disposable ego conversation on desktop and phone. Start a long desktop turn, queue a phone prompt, and confirm both views show it. Cancel a queued item from desktop and confirm it disappears from phone without reaching ego; queue another, stop the running turn from phone, and confirm desktop shows cancellation and the queued prompt starts only after the ACP response. Pause a turn with a prompt queued; confirm it stays queued until Resume and remains cancellable from either view. The live backend cannot load this Rust change until restart; targeted Rust fixture and frontend tests cover the protocol and rendering paths. _(NOT VERIFIED 2026-09-30: blocked — Needs a real phone (PWA) plus ego ACP.)_

## ACP ego peer identity (story 1073-3431) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, open AI Chat with ego configured and verify its MCP bridge appears in `agent list_peers` as an `ego` peer without a terminal; send mail, reconnect, restart, and verify the same peer UUID can read it with `agent wait`. _(NOT VERIFIED 2026-09-30: partial — No ego configured (ego_executable empty): ego peer in list_peers not testable; register/list_peers verified for MCP peers only.)_
- [ ] Spawn a child from that ACP bridge and verify `parent_session_id` equals the AI Chat peer UUID. Submit blocked progress and verify the desktop progress event carries the ACP conversation ID and the away-state mobile push is emitted when push is configured. _(NOT VERIFIED 2026-09-30: blocked — Needs real phone push for away-state; also requires ego ACP bridge.)_

## Codex approval cancellation (story 1125-f4ea) — Rust restart required

- [ ] After restarting `make dev` with an isolated `TUIC_APP_INSTANCE`, create a disposable Codex session and trigger a shell approval. Confirm its tab reports awaiting input; press Esc and confirm the badge clears when the idle composer returns. Trigger another approval and confirm the badge appears again. The running backend cannot load this Rust change until restart. _(NOT VERIFIED 2026-09-30 story 1302-83ae: partial — Not tested with Codex approval; awaiting input via Claude Ink picker verified (awaiting_input=true, source=question). After Esc cancel of AskUserQuestion Claude session stayed busy/working 156s until new input, badge not cleared: possible fault.)_

## AI Chat ACP session details (story 1072-6787)

- [ ] In an isolated test instance running this frontend, open a disposable ego conversation and confirm its updated title fits the panel header and picker. After a usage update, confirm the context percentage and optional cost remain readable above the panel edge. The targeted component tests cover the values; no instance running this worktree was available for a screenshot. _(NOT VERIFIED 2026-09-29: Needs live ego ACP conversation (title, usage update).)_

## AI Chat ego profile (story 1074-9373) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, set **ego profile** to a profile in ego's user configuration and open AI Chat. Confirm ego uses that profile for the new connection. Clear the setting and reconnect; ego must use its normal profile selection. Capture the Settings row to verify its layout. The running Rust backend cannot load the new `AppConfig` field or ACP launch arguments until restart; the browser wrapper timed out twice while opening the worktree's Vite page, and maccontrol returned circuit open. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Ego profile setting is UI plus ego configuration; no ego installed.)_

## AI Chat image paste (story 1085-fa65)

- [ ] [HUMAN] In an isolated desktop test instance with an image-capable ego connection, copy a PNG from another app and paste it into AI Chat. Confirm the thumbnail renders, can be removed, and an image-only submit reaches ego. Repeat with plain text paste. Targeted component/client tests prove the ACP block and guards; browser accessibility showed the thumbnail and controls, but Chrome's screenshot command timed out twice, so the visual result and real cross-app clipboard path remain unverified. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## AI Chat conversation recovery (story 1071-46c9) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, open AI Chat with ego, send a turn, create a second conversation, then restart the app. Confirm the last conversation and its history return; select the older title in the newest-first picker and confirm its history appears once. The running Rust backend cannot load the new `AppConfig` field until restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: AI Chat conversation recovery is UI plus ego.)_

## Windows Codex npm launcher — Rust restart required (story 987-c0ca)

- [ ] On a Windows build with npm's adjacent `codex` and `codex.cmd` shims, restart TUICommander and launch Codex from the agent menu. Confirm the help probe selects `codex.cmd`, reports `--no-alt-screen` support, and the new session stays on the primary screen. The targeted Rust test passed for the adjacent shims; native Windows execution remains to be checked. _(NOT VERIFIED 2026-09-30: blocked — Needs a Windows host with npm codex/codex.cmd shims (Windows desktop blocked).)_

## Agent Enter gap (stories 974-254a, 975-1de1) — Rust restart required

- [ ] After restarting `make dev` in an isolated `TUIC_APP_INSTANCE`, send text plus `special_key=enter` to a disposable Claude MCP session and confirm it submits. Launch Codex through a wrapper that foreground detection does not recognize, then send a long prompt from a suggestion or dictation while the tab still has no agent type; confirm it submits and check app logs for one unknown-foreground warning. The current backend cannot load the Rust change until restart. _(NOT VERIFIED 2026-09-30: partial — Claude MCP session input text + special_key=enter submits (used in r0-cl4 run, turn started). Wrapper foreground-detection and log-warning not tested.)_

## Managed Codex wrapper trust (story 1047-c41c) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, configure a Codex run config whose launcher forwards `"$@"` to Codex. Spawn a throwaway managed peer in a new directory and confirm it reaches Ready and receives its initial task without a trust answer. Turn off **Accept workspace trust for managed spawns** and repeat in another new directory; the ordinary Codex trust question must remain. The current live backend cannot load this Rust change until restart. _(NOT VERIFIED 2026-09-30: partial — Codex spawn with own -c args reaches prompt; wrapper forwarding and skip_trust_dialog opt-out via agents config not exercised.)_

## Missing registered worktree cleanup — Rust restart required

- [x] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, remove the checkout directory of a throwaway linked worktree. Ask for its lifecycle by workspace id, confirm `missing_checkout=true` and no dirty fingerprint, then confirm removal in the desktop dialog or HTTP with `confirmMissingCheckout=true`. Confirm the Git registration is pruned and the branch remains when branch deletion is disabled. Repeat with a locked registration: cleanup must stop until a separate lock override is confirmed. The running Rust backend cannot load this change until restart. _(verified 2026-09-29: fixture repo ~/Gits/.tmp/tuic-validate/fx/repo, MCP/HTTP on tuic-remote --instance validate (HTTP path only, desktop dialog not tried): rm -rf checkout -> worktree_lifecycle missing_checkout=true, no dirty_fingerprint; MCP remove refuses, HTTP DELETE force without confirm refused, with confirmMissingCheckout=true&deleteBranch=false ok, registration pruned, branch kept; locked variant refused with worktree_locked until overrideLock=true; t_wt3.py t_wt4.py)_

## AI Chat pending ACP badge and notification (story 1070-38ce)

- [ ] In an isolated desktop test instance, open AI Chat, start a request that asks for permission, then hide the panel. Confirm the status-bar AI Chat toggle shows one pending item and one desktop notification. Answer the request and confirm the badge disappears and the notification closes. Targeted component/store tests cover the state changes; the visual screenshot attempt timed out in the browser wrapper after its accessibility snapshot showed the badge. _(NOT VERIFIED 2026-09-29: Needs a real ego ACP request that asks permission plus a real desktop notification; no ego/paid provider in headless run.)_

## Child lifecycle inbox coalescing — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, let a throwaway managed child ask a confident question, answer it, then have it ask another. Confirm the parent inbox contains both question notices while ordinary state updates still coalesce. The current live backend cannot load this Rust change until restart. _(NOT VERIFIED 2026-09-30: partial — Question notices seen via awaiting_input state_change (confident, source=question); two-notice coalescing in parent inbox not exercised.)_

- [x] After restarting `make dev` in an isolated `TUIC_APP_INSTANCE`, send a peer RESULT to a throwaway parent, then leave its inbox unread while three throwaway children each send repeated state notices. Confirm the latest notice for each child and the complete RESULT remain available in `agent action=inbox`, with no missed count from the replacements. The running backend cannot load this Rust change until restart. _(verified 2026-09-29: Peers ag2-parent/ag2-kid via MCP; kid sent 'RESULT: ...' to parent; parent spawned 3 fake amp children cycling busy/idle for 40s (~6 cycles each) without reading. agent inbox: count 4 = RESULT + one state_change idle notice per child, no missed_count field, has_more false.)_

## Agent inbox FIFO and paging — Rust restart required

- [x] After restarting `make dev` in an isolated `TUIC_APP_INSTANCE`, send 101 messages to a throwaway recipient without reading. Confirm every send succeeds, the inbox reports `missed_count=1`, and the oldest message is absent. Read with `limit=2` and repeat while `has_more=true`; each page must start after the prior `next_since`. The live Rust backend cannot load this change until restart. _(verified 2026-09-29: isolated tuic-remote --instance validate over MCP unix socket: 101 sends by peer UUID all succeeded; first inbox limit=2 returned m1,m2 with missed_count=1 (m0 absent); paging via since=next_since covered m1..m100 in order, no dupes; script ~/Gits/.tmp/tuic-validate/t_inbox.py,t2.py))_

## AI Chat ACP pause settlement (story 1069-97bd) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, pause a disposable ego conversation while its turn streams. When ego stops at the pause boundary, Resume must remain visible and a new prompt must work after Resume. The running backend cannot load this Rust change until restart; the targeted ACP fixture test covers the state transition and the resumed prompt. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: ACP pause and Resume are UI and need ego.)_

## Urgent agent mail — Rust restart required

- [ ] After a `make dev` restart in an isolated `TUIC_APP_INSTANCE`, start throwaway Claude Code and Codex sessions and send `agent action=send urgency=urgent` while each is busy. Check that the notice appears after the current tool call and before the agent's next planned step, the peer body remains in the inbox, and the sender receives `urgent_delivered=true`. Repeat with a draft and a confident dialog; each must return `urgent_delivered=false` with a fallback reason and must preserve the composer. Boss's current backend cannot load this Rust change without a manual restart. _(NOT VERIFIED 2026-09-30: partial — Urgent mail to busy Claude/Codex not exercised end-to-end; only normal queue delivery verified (see 110).)_

## Progress journal paging — Rust restart required

- [x] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, call `repo action=progress_list` on a journal with more than 10 entries. The first page returns 10 entries, `total`, and `nextCursor`; follow the cursor to the end without duplicates. Open Progress and confirm the dialog still shows the complete journal. The current live backend cannot load this Rust change until restart. _(verified 2026-09-29: 13-entry journal: first page 10 entries, total 13, nextCursor 4; following input.cursor gave the remaining 3, 13 unique ids, then null; Progress dialog (browser mode) lists all 13 entries)_

## Headless MCP voice binding (story 1006-2729) — remote daemon rebuild required

- [x] After replacing a disposable `tuic-remote` daemon with this build, call MCP `voice action=status` from a connection without a live terminal and from one bound to a live terminal. The first must be refused as unbound; the second must report that this build has no audio support. The running daemon cannot load the Rust change until it restarts. Targeted headless and desktop tests cover both binding paths. _(verified 2026-09-29: Disposable tuic-remote (--instance ag3d, unix socket): MCP voice action=status from unbound connection -> isError {error:'This connection is not bound to a terminal...'}; from peer named by live PTY id -> {available:false, unavailable_reason:'This TUICommander build has no audio support'}.)_

## MCP agent spawn environment and model — Rust restart required

- [x] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, set an Agents run config model and environment value, then spawn a throwaway MCP child with overriding `model` and `env` values. Confirm the child sees the caller environment, the override model reaches its argv, and `TUIC_SESSION` and `TUIC_PARENT` still identify the peer. The running backend cannot load this Rust change without a restart. _(verified 2026-09-29: PUT /config/agents run config (model sonnet, env LAYER/ONLY_RUN/TUIC_* spoofs) + MCP spawn with model=opus env{LAYER=caller,TUIC_*=spoof}: child printed --model|opus|<own sid>|<parent P>|caller|present; without overrides --model|sonnet|..|run|present. TUIC_SESSION/PARENT protected. Config restored.)_

## Push-to-talk native release — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, hold Fn while dictating, then release it while the WebView is briefly busy or loses focus. The macOS microphone indicator must go off on release, the captured phrase must transcribe once, and the file log must show Fn down/up, native stop and IPC stop latency plus audio seconds and final/partial character counts. Repeat a recording longer than 30 seconds; its opening words must remain. The current live backend cannot load this Rust change without a restart. _(NOT VERIFIED 2026-09-30: blocked — Needs Fn key hold, macOS mic indicator and real microphone (audio hardware blocked).)_

## Managed Claude mail wake — Rust restart required

- [x] After restarting `make dev` in an isolated `TUIC_APP_INSTANCE`, send mail to a throwaway managed Claude peer while its turn is busy and its MCP SSE stream is connected. Let it become idle without reading the inbox during the turn. Confirm one payload-free `[TUIC] message available` notice starts a new turn and `agent action=inbox` returns the mail. Repeat with an inbox read before idle and confirm no stale notice is submitted. The current live backend cannot load this Rust change until restart. _(verified 2026-09-30: r0-cl4: real Claude (--settings hooks, --mcp-config bridge, TUIC_SOCKET env). Mail sent while busy; after idle one '[TUIC] message available - read it with: agent action=inbox' notice started a new turn, Claude called inbox and quoted the mail. Second run: inbox read before idle -> no notice submitt)_

## Claude awaiting badge — Rust restart required

- [ ] After a `make dev` restart, let a Claude tab finish an ordinary prose reply with the Activity Dashboard open. The generic desktop notification must not flash Waiting input; an Ink picker or explicit permission request must still show it. The running backend cannot load this Rust change without a restart. _(NOT VERIFIED 2026-09-30: partial — Claude prose reply never set awaiting; AskUserQuestion picker set awaiting_input=true; startup Ink import picker did not. Dashboard and notification flash are UI.)_

## Detached CLI wake status — rebuild the Rust CLI

- [x] After rebuilding and reinstalling `tuic`, run `tuic bg` from a throwaway managed agent and inspect `<log>.exit` and `<log>.wake`. Confirm the command exit code is preserved and wake status is `queued` when the queue takes the request. For an unbound caller, confirm MCP mail surfaces `BG DONE` with the queue error and `.wake` says `mailed`; when both channels fail, `.wake` says `failed` with both reasons. The installed CLI cannot load this Rust change until rebuilt. _(verified 2026-09-30: Used /usr/local/bin/tuic (kit tuic lacks bg/mcp). Managed pi caller: exit 7 kept, wake {status:queued}. Unbound MCP identity + active agent wait: inbox got 'BG DONE exit=4 ... queue wake failed: Expected one live session', wake status mailed. Both fail: status failed 'queue: ...; mail: ...inbox_only)_

## Queued agent command diagnostics — Rust restart required

- [x] After a `make dev` restart, enqueue a throwaway command for a test Codex session and inspect app logs for one `queue delivery attempt` record with session id, agent and shell states, queue counts, typed/submitted result, and separate Enter status. This Rust instrumentation is absent from Boss's current backend until restart; do not interrupt live sessions for it. _(verified 2026-09-29: Fake amp-type agent (adapter-less so idle is confirmed) via spawn binary_path; POST /sessions/{id}/queue {text} -> typed:true. /logs shows exactly one 'queue delivery attempt' with session_id, agent_state/shell_state=idle, queued_before 1/queued_after 0, typed yes, submitted true, enter_separate sent. Not a real Codex session.)_
- [ ] After loading the story 1106 Rust build in an isolated `TUIC_APP_INSTANCE`, _(NOT VERIFIED 2026-09-30: partial — Item text truncated (only NOT VERIFIED note). Codex queue while busy verified delivered (see 110); idle_unconfirmed diagnostic not observed.)_
      queue a command for a throwaway Codex session while it is busy. Confirm it
      runs once when Codex reaches Ready and the queue reaches zero. If the shell
      first becomes idle without confirmed readiness, logs must name
      `defer_reason=idle_unconfirmed`, then `Ready confirmed after shell became
      idle` before the successful flush. The current backend cannot load this
      fix without a restart.

## Consumed MCP agent inbox mail (story 1105-966b) — Rust restart required

- [x] After a `make dev` restart in an isolated `TUIC_APP_INSTANCE`, fill a throwaway peer inbox, read it, and send one more message. Confirm the new send succeeds and the next inbox call returns it without `missed_count`. The current live backend still has the old Rust code; targeted unit tests cover pagination, capacity, and delivery leases. _(verified 2026-09-29: same instance: after full read, one more send returned only 'extra' with no missed_count)_

## MCP tab caller repository (story 1102-0945)

- [x] An MCP caller in repository A opens an unpinned external Markdown tab while repository B is visible: the focused tab switches to A and remains in A's tab bar; `focus=false` leaves B visible and the tab appears on return to A. Inline HTML/URL tabs use the same caller scope. _(verified: targeted `useAppInit` and `mdTabs` Vitest tests cover focused/background external files and caller-scoped HTML.)_

## Sequential Markdown comments (story 1103-d5ae)

- [x] Add block comments to several different numbered Markdown items in one open file. Each marker stays beside its item, the convention header appears once, and each highlight reopens its own comment. _(verified: `MarkdownTab.test.tsx` exercises four sequential saves through the tab, source positions, parsed comments, highlights, and reopen; `ContentRenderer.test.tsx` verifies source-only updates refresh block metadata.)_

## Mobile remote sessions, Progress, and Activity — Rust restart required

- [ ] [HUMAN] After Boss restarts `make dev` when current PTYs can be interrupted, verify `/api/version` identifies the integrated build, then open the Tailscale HTTPS `/mobile` PWA on a phone. _(NOT VERIFIED 2026-09-30: blocked — HUMAN item needing a real phone/Tailscale HTTPS PWA (real phone blocked). Only /api/version checked: {"version":"1.7.7","git_hash":"bef15c20f"} on rust0930 instance.)_
- [ ] [HUMAN] A connected remote session should show live output through WebSocket; create and close only a throwaway remote session from mobile, then confirm it disappears on its owner. Disconnect that machine and confirm the stale session shows unavailable rather than a misleading local 404. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: needs a real phone (mobile PWA) and a second machine for the remote session; blocked (real phone / second physical machine).)_
- [ ] [HUMAN] Open mobile Progress with no desktop repository selected. It should select the newest journal project, show saved done/blocked entries, and allow switching projects. Open Activity and confirm persisted active events appear while dismissed events stay hidden. The backend routes and store shape have targeted automated tests; the real phone remains to be checked. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: needs a real phone PWA for mobile Progress/Activity views (real phone blocked).)_

## Progress toast dismissal (story 1061-7694) — after the fixed frontend loads

- [ ] Tap a Progress toast body on desktop while another repository is active; it should close without changing the repository or terminal. On a second toast, use **Go to repo** and confirm it opens the reporting workspace. On mobile, tapping the toast body should close it without opening its action. Targeted component tests verify these paths; check the loaded UI after this branch is integrated. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Embedded external links (story 989-63fa) — after Rust rebuild

- [ ] After the next `make dev` restart, use an isolated `TUIC_APP_INSTANCE` test instance to click an HTTPS link in an HTML preview and an inline plugin panel: each should open outside the app. Open a cross-origin dashboard URL in a tab, attempt an external navigation, and confirm the blocked-link toast directs users to the tab menu's Open in Browser action. The live Boss backend has not restarted for the Rust navigation event. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Embedded link opening in HTML preview and plugin panel is UI.)_

## Mobile repository files (story 1063-3fe1) — real phone

- [ ] [HUMAN] On a phone connected to an isolated TUICommander test instance, open Files, select a disposable repository, browse into a directory, open a `.md` file in rendered View, switch to Edit, change its source and save, then confirm the updated rendered View and saved content from the desktop. Check another text file stays plain text and that a file over 1 MB and a binary file show a refusal. Targeted Vitest covers these flows; a responsive desktop-browser screenshot does not verify touch and mobile keyboard behavior. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Editor links and external Markdown tabs (2026-09-27)

- [x] MCP file tabs remain visible after selecting a terminal in a repository with no active workspace; distinct MCP ids coexist and a repeated id updates its native file tab. A terminal link to an existing external Markdown path under `~/Gits/.tmp/` opens in the Markdown viewer. _(verified: targeted TabBar, useAppInit, and terminal file opening Vitest tests.)_
- [x] Unpinned MCP native file and HTML/URL tabs hide in another repository and return when their opening repository is selected again; pinned MCP tabs stay visible across repositories. _(verified: targeted `useAppInit`, `mdTabs`, `tabManager`, and `TabBar` Vitest cases exercise scope, visibility, pinning, and retention.)_
- [x] Cmd/Ctrl+click opens editor web links in the system browser, local paths in the matching TUICommander view, and missing paths with a toast. MCP `tuic://open` opens external Markdown in a Markdown tab. _(verified: targeted editor and MCP tab Vitest cases exercise these routes; browser and native window appearance require a visual check after integration.)_
## Native MCP action cleanup (story 1091-1d5d) — rebuild Rust server and CLI

- [x] After rebuilding, use an isolated test instance to confirm `agent register/list_peers` accepts `path`, `repo worktree_lifecycle/worktree_remove` accepts `branch`, and removed actions return errors naming their HTTP routes. Reinstall the CLI before checking `tuic agent list-peers --path` and `tuic agent stats --json`. Restart a live `make dev` session only when ready to end its current PTYs. _(verified 2026-09-30: agent register path=bigrepo + list_peers path filter returned the peer with path; tuic agent list-peers --path and tuic agent stats --json ({active_sessions:13,...}) work; repo worktree_lifecycle/worktree_remove accept branch; agent stats/detect, repo prs/close_issue, session process_stats return 'w)_

## Detached CLI commands (story 1100-96bd) — rebuild the Rust CLI

- [x] After rebuilding and reinstalling `tuic`, run a disposable `tuic bg <log> -- <cmd>` from an isolated managed session. Confirm the launcher returns before the command, `<log>.exit` records its code, and a busy caller receives the completion wake only after becoming idle. The installed CLI cannot load this Rust change until rebuilt; restart `make dev` only when ready to end its live sessions. Windows behavior is covered by CI-only tests and remains unverified on this Mac. _(verified 2026-09-30: /usr/local/bin/tuic bg from pi session: launcher returned in 0.019s, bgE.log.exit=9, wake queued; while pi was working the BG DONE text sat in /sessions/{id}/queue (id 29) and drained only after idle; pi then reported exit=9. Windows unverified.)_

## Generic MCP CLI (story 1099-79b8) — rebuild the Rust CLI

- [x] After rebuilding and reinstalling `tuic`, use an isolated test instance to compare `tuic mcp session '{"action":"list"}' | jq length` with the instance's MCP session count. Run `tuic mcp agent '{"action":"wait","timeout_ms":8000}'` and confirm it waits for the server reply without a three-second socket failure. The installed CLI cannot load the Rust change until rebuilt; restart `make dev` only when ready to end its live sessions. _(verified 2026-09-30: /usr/local/bin/tuic mcp session list | jq length=16 == MCP session list 16 == GET /sessions 16. 'tuic mcp agent {wait,timeout_ms:8000}' returned {timed_out:true} after 8.012s, no 3s socket failure.)_

## CLI blocking waits (story 1060-df82) — rebuild the Rust CLI

- [x] After rebuilding and reinstalling `tuic`, run `tuic agent wait --timeout-ms 8000 --json` and `tuic session wait <busy-session> --until exited --timeout-ms 8000 --json` against an isolated test instance. Confirm each returns after the server's response rather than failing after three seconds. The running app and installed CLI do not load this Rust change until rebuilt; restart `make dev` only when ready to end its live sessions. _(verified 2026-09-30: tuic agent wait --timeout-ms 8000 --json -> {timed_out:true} after 8.012s; tuic session wait r0-cl2 --until exited --timeout-ms 8000 --json -> {met:false,timed_out:true,until:exited} after 8.012s. (r0-cl2 was idle claude, not busy.))_

## Mobile notification tags (story 1042-f5ca) — updated service worker

- [ ] [HUMAN] On a real subscribed phone after the updated service worker takes control, receive questions from two different sessions. Confirm both notifications stay visible and each opens its own session. Send another push for one session and confirm the other remains. The targeted service-worker test verifies tag replacement and both click deep links; the phone's notification UI requires real device verification. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Rust test temp root (story 980-420e) — Rust, needs `make dev` restart

- [x] Bare `cargo test` test binaries and Nextest setup scripts route Rust scratch directories through the repository test root. _(verified: `src-tauri/crates/tuic-test-support/src/lib.rs` initializes the libtest environment; `src-tauri/.config/nextest.toml` runs platform setup scripts; targeted subprocess test confirms `tempfile` and `std::env::temp_dir` stay under `test_temp_root()`.)_ No live-app behavior changes; the current backend cannot load the Rust test-support code until a restart.

## Mobile completion push limit (story 1041-cdc7) — Rust, needs `make dev` restart

- [ ] [HUMAN] After restarting `make dev` when Boss is ready to end the current sessions, use a real subscribed phone while the desktop is away. Trigger a titled question followed immediately by session completion; confirm the phone displays one notification. Real phone delivery and display cannot be verified by the local HTTP push receiver. The targeted Rust test verifies accepted push requests, expiry after 30 seconds, and independent session limits. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: needs a real subscribed phone for push delivery (real phone blocked); instance has push.enabled=false.)_

## Managed agent workspace trust (2026-09-26) — Rust, needs `make dev` restart

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, use `agent action=spawn` to start Claude and direct Codex in never-trusted folders under `~/Gits/.tmp/`. Confirm each reaches the agent prompt and receives the task without a manual trust keypress. Turn **Accept workspace trust for managed spawns** off for each agent and confirm its normal trust question remains. User-opened agent terminals must retain normal trust behavior. The current running backend cannot load this Rust change until restart. _(FAILED 2026-09-30 story 1300-f7f4: Codex 0.159.0 spawned with -c projects."<cwd>".trust_level="trusted" in a new dir still showed Trust this folder? and waited 48s+ with no auto-answer. Claude new-dir and opt-out not tested.)_

## MCP config ownership (story 988-d1a1) — Rust, needs `make dev` restart

- [x] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, confirm startup leaves a sandboxed agent MCP config unchanged. The current live backend cannot load the ownership guard without a restart. Do not restart Boss's running instance or use his real agent configs for this check. _(verified 2026-09-29: Sandbox HOME with ~/.claude.json holding stale tuicommander bridge entry; tuic-remote --instance ag3d: log 'Skipping agent MCP config updates from a secondary instance', file byte-identical (diff). Control with TUIC_MCP_CONFIG_OWNER=1 rewrote entry. Desktop validate instance log shows same skip line. Headless daemon used; same fn.)_

## Plan picker (2026-09-26) — Rust, needs `make dev` restart

- [x] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, open Plans and Stories in a repository with `plans/*.md`. Confirm the document choices are visible, the selected plan title matches its heading or front matter, and **Add from path or link** stays collapsed until opened. The running backend cannot serve `list_plan_sources` or `add_plan_source` until restart. Targeted Rust and Vitest tests cover discovery and dialog behavior; the visual layout needs the rebuilt app. _(verified 2026-09-29: fx/repo with two md plans (one with front matter title differing from its # heading): 'Plans and Stories' > New plan lists 'Front Matter Title' and 'Plain Heading Plan' with their paths; 'Add from path or link' details open=false. Layout judged from DOM text only, no screenshot.)_

## Remote repository browsing and terminal attach (stories 1025-8103, 1026-7633) — Rust restart and remote daemon update

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance and updating its test remote daemon, open the remote repository picker. Confirm it starts at the remote host's home directory, a denied directory shows a readable error while manual path entry and Up remain usable, and a terminal in a remote repository renders its prompt. Opening a terminal with a missing cwd must show a readable error instead of a blank pane; a failed grid stream must show an error toast. Use only disposable test connections and sessions; the live `make dev` backend cannot load the new home-directory route or cwd validation without a restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Remote repository picker is UI; needs remote daemon.)_

## Claude question state after wrapped suggestions (story 1023-dcc9) — Rust, needs `make dev` restart

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, run a Claude turn that ends in a wrapped `suggest: [ … ]` item containing `?`. Confirm the idle tab does not show a question badge. A real AskUserQuestion must still show one. The existing live backend cannot load this Rust change without a restart. _(NOT VERIFIED 2026-09-30: partial — Claude wrapped suggest with '?': idle, awaiting=false. Real AskUserQuestion: awaiting_input=true. Badge itself is UI.)_

## Remote update and restart — Rust, needs `make dev` restart

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, use a disposable remote daemon to check the per-connection Auto-update option. Verify zero live sessions updates once through the daemon restart, live sessions show a manual offer with a count that refreshes while connected, and a failed update shows its error. During automatic transfer, confirm the manual update button is disabled; a stalled transfer eventually reports a timeout and resumes connection checks. Check the checkbox and status layout visually; the automated browser screenshot timed out. Do not use Boss's saved daemon. The running Rust backend cannot load this change until restart. _(NOT VERIFIED 2026-09-30: blocked — Needs a second physical machine (remote daemon update).)_
- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, connect a disposable Direct daemon and an SSH daemon, confirm the out-of-date badge and the exact live PTY count, then update each and verify reconnect with the new `/health.build.sha256`. The live backend cannot load this Rust change without a restart. Do not update Mac-mint or Boss's saved connections. _(NOT VERIFIED 2026-09-30: blocked — Needs a second physical machine (Direct and SSH daemons).)_

## Squash-merged worktree removal (story 1022-8291) — Rust, needs `make dev` restart

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, inspect a clean squash-merged worktree whose local tip is contained in its merged GitHub PR head. Confirm the lifecycle badge says Merged and removal with branch deletion succeeds. Check that a branch contained in the main checkout's current integration branch also removes when the remote default branch is behind. A read-only ignored build tree should arrive writable only in the new worktree and should not block removal. The current live backend cannot load this Rust change without a restart. _(NOT VERIFIED 2026-09-30: partial — Verified: branch contained in checkout's current branch 'integ' (origin/main behind) removes without force: removal_rule integration_ancestry, branch deleted; ignored read-only target/ in main arrived writable (drwxr-xr-x/-rw-r--r--) in worktree, main stays r-x. NOT verified: GitHub-PR-head 'Merged')_

## MCP local branch deletion (story 1033-1796) — Rust, needs `make dev` restart

- [x] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, use MCP `repo action=branch_delete` on an integrated local branch with no worktree. Confirm only the local ref disappears; a checked-out or unmerged branch must be refused. The current live backend cannot load this Rust action without a restart. _(verified 2026-09-29: fixture repo ~/Gits/.tmp/tuic-validate/fx/repo, MCP/HTTP on tuic-remote --instance validate: branch_delete int1 ok (proof in_sync, only local ref gone); main refused (current integration branch); un1 refused (unmerged commits); co1 refused (checked out in a worktree); t_wt2.py)_

## Claude usage per profile (story 1016-9cf8) — Rust, needs `make dev` restart

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, focus Claude sessions launched with the default config and a separate `CLAUDE_CONFIG_DIR`. Confirm the status badge changes to each account's quota and its dashboard shows the same account. A profile without credentials must show unknown. The live `make dev` backend cannot load this Rust change without a restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Status badge quota per CLAUDE_CONFIG_DIR is UI.)_

## Hidden iframe unload (story 979-3d7f) — live instance required

- [ ] In a disposable test instance, open a URL tab pointing to a page that runs a 1 s busy loop every 5 s. Hide it by switching tabs, repositories, and pane groups, including a pinned tab and a split pane covered by an orphan tab. Verify `document.querySelectorAll('iframe[src="<test-url>"]').length === 0` while hidden and that showing the tab loads the same URL again. While hidden, run a read-only 30 s main-thread probe with 100 ms `setTimeout` ticks; require 0 gaps over 150 ms. Do not use Boss's live instance. _(Deferred: this requires the changed frontend in a running test instance; targeted Vitest proves DOM removal and remount.)_

  Run this in the test instance's frontend console while the tab is hidden (a gap is measured delay beyond the intended 100 ms):

  ```js
  const gaps = [];
  let last = performance.now();
  const end = last + 30000;
  const tick = () => {
    const now = performance.now();
    if (now - last > 250) gaps.push(Math.round(now - last - 100));
    last = now;
    if (now < end) setTimeout(tick, 100);
    else console.log({ gapsOver150ms: gaps.length, gaps });
  };
  setTimeout(tick, 100);
  ```

## Linked-worktree warming excludes MDKB (2026-09-26) — Rust, needs `make dev` restart

- [x] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, create a disposable worktree from a repository that ignores and contains `.mdkb/`. Confirm the new worktree has no `.mdkb` while an ignored build directory still arrives warm. The live backend cannot load this Rust change without a restart. _(verified 2026-09-29: fixture repo ~/Gits/.tmp/tuic-validate/fx/repo, MCP/HTTP on tuic-remote --instance validate: worktree_create from a repo containing ignored .mdkb/ and target/: new worktree has no .mdkb, target/debug/x present (warm done), .env not copied; t_wt.py)_

## Native scrollback capture fixtures (2026-09-25) — after mcp-config-guard lands

- [x] In an isolated `tuic-remote` instance with `TUIC_CAPTURE_DIR` under `~/Gits/.tmp/`, captured Codex 0.157.1 with `--no-alt-screen` (approval prompt, resize, idle footer) and OpenCode 1.18.30 with `--mini` (idle resize). Both `.tcap` fixtures are in `src-tauri/src/fixtures/agent_prompts/`. Targeted Rust replay tests verify primary-screen mode, the Codex chrome-cutoff anchor, and no BUSY edge from the OpenCode resize repaint (story 939-475b). _(verified: `pty::tests::live_native_scrollback_captures_never_enter_alternate_screen`, `codex_native_scrollback_capture_keeps_approval_and_idle_composer_visible`, `opencode_mini_resize_repaint_does_not_reopen_an_idle_turn`; 3/3 passed)_

## Codex dictation auto-send (2026-09-25) — Rust, needs `make dev` restart

- [ ] [HUMAN] After restarting `make dev` when ready to end the current sessions, dictate a long phrase into a Codex tab with Auto-send enabled. Confirm it submits once rather than inserting a newline. Real microphone input and the Codex TUI are required for this final check. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: real microphone dictation plus Codex TUI (audio hardware blocked).)_

## File browser default (2026-09-25) — Rust, needs `make dev` restart

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` build with no saved `file_browser_view_mode`, open the file browser and confirm tree view is selected. Switch to flat list, restart, and confirm that choice remains selected. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: File browser view mode is UI.)_

## Editor line wrapping (2026-09-25) — Vite hot reload

- [ ] After Vite reloads the frontend, open a `.txt` file with a long line and confirm it wraps without horizontal scrolling; open a `.rs` file and confirm it does not. Toggle either with the header button or `Alt+Z`, then reopen the file and restart the app to confirm each file kind keeps its own setting. Check the active button style and that cursor, selection, search, git gutter, and inline blame still work while wrapped. _(NOT VERIFIED 2026-09-29: partial — Long-line agb_long.txt: .cm-lineWrapping, scrollWidth==clientWidth (822), Wrap button aria-pressed true + active class. agb_long.rs: no wrap, scrollWidth 16108>822. Header Wrap button toggled .rs to wrapped (sw==cw); localStorage tui-commander-editor-wrap {text,code} separate per kind, survives reload. Not checked: Alt+Z effect, cursor/selection/se)_

## Native WontFix dependency recovery (2026-09-25) — Rust, needs `make dev` restart

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, create a plan with a WontFix prerequisite and a Backlog dependent through the test instance. Confirm the dialog marks the prerequisite abandoned, offers Remove only on that direct cancelled edge, and reports the plan Active until its remaining stories are Done or WontFix. The current live backend cannot load this Rust change without a restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dialog for WontFix dependency is UI.)_
- [x] After the native-stories backend rebuild, add a dependency to a story, reload Plans and Stories, and confirm the story list and derived plan state agree. This checks the reused SQLite connection and status aggregation in the rebuilt app. _(verified 2026-09-29: POST /stories/action: created plan+A,B in fx/repo, add_dependency B<-A; list_stories: A ready, B backlog deps=[A]; plan_view state=active with same story statuses/deps; plan_state=active; list_plans lists plan. Consistent.)_
- [ ] After restarting an isolated dev instance, open Plans and Stories to verify the capability probe succeeds; also confirm an actual story action error shows its own message rather than a restart instruction. _(NOT VERIFIED 2026-09-30: partial — GET /stories/capabilities returns true. Action error message display in UI not observed.)_
- [ ] After the native-stories backend rebuild, cancel a prerequisite with an indirect dependent and confirm the dialog shows both dependencies as abandoned, the Rust-supplied cancellation count, and the plan's Active state. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Cancellation dialog is UI.)_
- [ ] After the native-stories backend rebuild, cancelling an already cancelled story must show an error and leave its revision unchanged; removing a cancelled dependency remains limited to a Backlog story. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Error dialog is UI; revision check not exercised.)_
## Agent native scrollback (2026-09-25) — Rust, needs `make dev` restart

- [ ] After a safe `make dev` restart in an isolated `TUIC_APP_INSTANCE`, launch a throwaway agent whose `--help` child hangs. Confirm the first launch waits at most the two-second probe deadline, later launches of the same binary version do not wait again, and replacing the binary permits a new probe. On Windows, confirm no `cmd.exe` or `node.exe` child remains after timeout. _(NOT VERIFIED 2026-09-30: partial — Not exercised with a hanging --help fixture.)_
- [ ] After a `make dev` restart, open a **new** TUIC shell and type `codex`, `grok`, and `opencode` in separate throwaway tabs. Confirm each supported installed CLI stays in native scrollback; repeat with its per-agent **Prevent alternate screen** setting off, then restore the setting. Existing shells retain the previous PTY environment and cannot verify this change. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Native scrollback in new TUIC shell tabs is UI.)_
- [ ] After a `make dev` restart, launch Claude, Codex, Grok and OpenCode through the agent menu, PR Review where configured, and MCP `agent spawn` in an isolated `TUIC_APP_INSTANCE`; confirm terminal histories remain available after exit. Resume each session and check the same behavior. A CLI without the flag in `--help` should still launch. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Terminal history after agent launch is UI.)_
- [x] Start an isolated test instance with an absolute `TUIC_CAPTURE_DIR` under a disposable directory. Enable `POST /diagnostics/capture` for a throwaway session; `GET /diagnostics/capture` must report that directory and its `.tcap` must appear there. A relative override must return `TUIC_CAPTURE_DIR must be absolute` and leave capture disabled. _(verified 2026-09-29: tuic-remote (own instance ag2a) with absolute TUIC_CAPTURE_DIR: POST /diagnostics/capture -> dir echoed, .tcap (114B) appeared there, GET reports dir+bytes. Relative 'relcap' -> {enabled:false,error:'TUIC_CAPTURE_DIR must be absolute'}, GET enabled:false, no dir created.)_
## Markdown link navigation guard (2026-09-25) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, click a relative source link in a Markdown file: the code editor opens and no localhost page opens in the system browser. A link to an external web origin is blocked by the WebView navigation guard. Restart only when current live sessions can be interrupted. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Markdown link navigation is WebView UI.)_
- [ ] In the restarted instance, open an HTML preview, PDF preview, URL plugin panel on localhost, srcdoc plugin panel, and reveal.js deck. Their iframe content and in-frame links/slide navigation still load. Export a text download on Linux through a blob URL. _(Static coverage: `lib.rs` navigation guard allows the internal frame schemes and loopback origins; runtime platform behavior needs the restarted app.)_ _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Preview iframes are UI; download part also needs Windows/Linux.)_
## Streaming Progress intents (2026-09-25) — Rust, needs `make dev` restart

- [x] After restarting an isolated `make dev` instance, stream an `intent:` line in a narrow Codex or Ink terminal. The Progress journal gets the complete text and title once, including an indented hard-wrap row; closing a tab with a title-less open intent preserves one final entry. Check that a token in a done/blocked report is redacted in the journal. _(verified 2026-09-29: 40-col agent-typed PTY: word-hard-wrapped 4-row intent -> one journal entry full text, tab title set (Narrow Zed). Close with open titleless intent: exactly one entry (id 48) kept. progress done with ghp_ token+Bearer -> text '[REDACTED]'. Shell fake agent, not real Codex.)_
## MCP bridge config guard (2026-09-25) — Rust, needs `make dev` restart

- [x] After a `make dev` restart, verify the dev app resolves its adjacent `tuic-bridge` but leaves existing working absolute agent MCP commands unchanged. Confirm a missing command is repaired to the dev sidecar only in a disposable agent config. The targeted Rust child-process test covers launch without a sidecar and checks that a disposable HOME stays byte-identical. _(verified 2026-09-29: Ran a tuic-remote copy (same ensure_mcp_configs as desktop, not the desktop app itself) with TUIC_MCP_CONFIG_OWNER=1, disposable HOME, tuic-bridge beside it: entry installed with adjacent bridge path; existing working abs command (other dir) left unchanged; nonexistent abs command repaired to adjacent bridge. Real HOME untouched.)_
## Worktree warm status and safe removal (2026-09-25) — Rust, needs `make dev` restart

- [x] After restarting an isolated `make dev` instance, deinitialize a disposable worktree submodule with a local-only commit and a Git module name different from its checkout path; repeat with a nested named submodule. Removal must refuse and leave each module Git store and commit recoverable. _(verified 2026-09-30: sm3 fixture: submodule name 'modname'!=path vendor/lib, nested named 'nestmod'; local-only commits in both; git submodule deinit -f --all in the worktree. worktree_remove -> 'Cannot remove worktree: uninitialized submodule vendor/lib still has Git state'; worktree stays; lib and nest commits still r)_
- [x] After restarting an isolated `make dev` instance, attempt to remove a disposable worktree with a local-only submodule commit while the main checkout's copy of that submodule is uninitialized. Removal must refuse and leave the source worktree and commit intact; after initializing the main copy, removal should preserve the commit in the module repository. _(verified 2026-09-30: sm6 fixture (clone, main submodule uninit): worktree with committed pointer to local-only vendor/lib commit; worktree_remove delete_branch=false -> 'main checkout has no repository for submodule vendor/lib', worktree kept. After 'git submodule update --init' in main: ok kept_branch, worktree gone, c)_
- [x] After restarting an isolated `make dev` instance, remove a disposable worktree whose submodule has two stash entries and a reflog-only commit. Confirm all three OIDs remain reachable in the main checkout module after removal, including when a separate missing checkout is force-pruned. _(verified 2026-09-29: repo4 w/ submodule libs; worktree s1: 2 stashes + reflog-only commit in its private module. Before: OIDs absent in main .git/modules/libs (cat-file fails). After MCP worktree_remove: all 3 OIDs are commits there, kept as refs/tuic/preserved/... Also s2 (missing checkout, force-pruned): reflog commit + stash OID reachable afterwards.)_
- [ ] After a `make dev` restart in an isolated `TUIC_APP_INSTANCE`, archive a disposable linked worktree with an initialized submodule. Confirm the archive remains a usable Git checkout, its submodule `git status` and refs work, and it disappears from the active sidebar. A locked disposable worktree must remain at its original path during an automatic archive sweep. _(NOT VERIFIED 2026-09-30: partial — POST /worktrees/finalize action=archive on merged worktree w445a (initialized submodule with local commit): archived to __archived/w445a; git status clean, submodule status shows same local commit 2ab282c, submodule refs/log work; worktree_list no longer lists it. Locked w445lock: 'worktree_locked:w)_
- [x] After a `make dev` restart in an isolated `TUIC_APP_INSTANCE`, confirm Worktree Manager Prune refuses a detached checkout during a Git operation and one whose latest commit exists only at detached HEAD. A detached checkout whose HEAD is reachable from a branch or tag should prune cleanly. _(verified 2026-09-29: POST /repo/remove-orphan (what Prune calls) on disposable repo: real conflicting rebase in progress -> 'Cannot remove orphan worktree: a Git operation is in progress'; detached HEAD with commit only there -> 'detached HEAD commit has no durable ref' (both kept); detached at main-reachable commit -> ok; detached at tag-only commit -> ok. Manager UI )_

- [x] After an isolated `TUIC_APP_INSTANCE=<id>` Rust restart, confirm a refused dirty non-force removal leaves the worktree's pending warm state visible. Remove a different worktree while another registered checkout directory is missing; the missing checkout's submodule Git state must remain available for later safe removal. _(verified 2026-09-29: Own repo r440: create worktree w/ 60k-file ignored dir -> warm_artifacts pending; dirty non-force remove refused ('has uncommitted changes'), status still pending, later done. Submodule repo: w2,w3 with modules; rm w3 dir; removed w2 ok; .git/worktrees/w3/modules/sub intact + w3 prunable; w3 later removed via confirmMissingCheckout.)_
- [x] After a `make dev` restart, verify a missing registered worktree refuses removal without force, and a separately confirmed lock override is needed if that registration is locked. After force removal, its submodule-only refs must remain in the main checkout's module repository. _(verified 2026-09-29: Missing+locked registration (rm -rf dir; git worktree lock). DELETE /worktrees/s2 no force -> 'Worktree presence changed since confirmation' (MCP: 'uncommitted changes'), refused. force+confirmMissingCheckout -> 'worktree_locked:missing worktree is locked'. +overrideLock -> ok removal_rule=force; submodule stash+reflog OIDs then present in main mod)_
- [ ] After a `make dev` restart, create a worktree through HTTP/MCP in an isolated `TUIC_APP_INSTANCE`. Its response says warming is pending; `GET /worktrees/paths?path=<repo>` moves to `done` or `failed`, and a configured setup script finishes before copying begins. Remove the worktree and check its warm status is no longer retained. A clean squash-merged branch removes without force and the response names `patch_equivalence`. _(NOT VERIFIED 2026-09-30: partial — Verified: worktree_create response warm_artifacts.status=pending; GET /worktrees/paths shows done within 1s; after worktree_remove the entry is gone; squash-merged branch removed without force -> removal_rule:patch_equivalence. NOT verified: setup script ordering (script fields cannot be set via API)_
- [x] After that restart, create a worktree through desktop IPC and confirm its instructions report `pending` until warming completes. Confirm non-force removal preserves a dirty worktree with `delete_branch` both on and off; when an archive script adds a commit, the branch remains and the response includes `branch_delete_warning`. _(verified 2026-09-30: MCP repo worktree_create: warm_artifacts.status=pending, later done. worktree_remove on dirty wt, delete_branch true and false: both refused 'uncommitted changes', wt+branch kept. With archive_script (empty commit) set via PUT /config/repo-settings: branch kept, branch_delete_warning present. Used M)_

## Night integration 2026-09-25 — Rust, needs `make dev` restart

- [x] After a `make dev` restart, run `tuic agent spawn …` outside TUICommander (no `TUIC_SESSION`): stderr shows one `registering an external MCP caller` notice and the spawn succeeds. `tuic session status <unique name or short id>` resolves; an ambiguous name returns an error. _(verified 2026-09-30: tuic (/usr/local/bin/tuic; kit tuic binary is stale, lacks session/story/mcp) with TUIC_SOCKET, env -u TUIC_SESSION: agent spawn claude -> one 'registering an external MCP caller' notice, rc=0, session created. session status <name> and <8-char id> resolve; two sessions named r1-dup -> 'ambiguous; m)_
- [x] Orchestrator inbox under load: while children finish, a parent that does not read its inbox retains the newest 100 messages in FIFO order. The 101st send succeeds and the next inbox read reports the unread eviction in `missed_count`. _(verified 2026-09-29: same instance: unread inbox kept newest 100 in FIFO order, 101st send succeeded, missed_count=1 (tested peer-to-peer, not with finishing children))_
- [ ] Put a malformed value in one field of `dictation-config.json` (for example `"speech_volume_db": "loud"`) in a `TUIC_APP_INSTANCE=<id>` instance: Settings → Voice keeps the other values, and a "Dictation settings recovered" warn toast appears once. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dictation routes/config are desktop-only (cfg feature desktop): GET /dictation/config and /dictation/hands-free return 404 on this headless tuic-remote. Settings->Voice toast needs desktop build + frontend.)_

## Workflow definitions (2026-09-25) — Rust, needs `make dev` restart

- [ ] In an isolated rebuilt instance, run two independent story workers. Let one request input so the run pauses, then let the other submit a current-generation report. Confirm its report is retained and an identical retry returns the same receipt while the run remains paused.
- [ ] In an isolated rebuilt instance, integrate and accept a story, add an unrelated canonical commit, and confirm its dependent is held until `recertify_canonical` runs the pinned checks at the new clean tip. Confirm a later ref or tree change invalidates that recertification.

- [ ] After rebuilding an isolated instance, save a direct-executable check with `update_checks`, publish the workflow, and confirm the published revision keeps its check set after the draft changes. A shell command or stale draft revision must be rejected.
- [ ] After rebuilding an isolated instance, read an existing workflow definition and confirm its closure is `human`. Try publishing an `automatic` draft and confirm the API rejects it; switch back to `human` and publish at the current draft revision.
- [ ] After rebuilding an isolated instance, transition a throwaway story through review and approve it through desktop IPC. Read `transition_history` through HTTP and confirm the approval records a human actor. Claim a second story with one managed session, submit it for review, and confirm that session's approval fails with `a story cannot be approved by its implementer` without a new history row; a different managed reviewer can approve and records its session ID. Approve a third story through sessionless HTTP and confirm it records `local_api`.
- [ ] After a Rust restart in an isolated instance, launch a coordinator attempt into a registered throwaway worktree with `workflow_launch`, inspect its prompt and event timeline, create a story with `workflow_story_create` and retry its proposal key, submit `workflow_report` from that managed PTY, and verify a different PTY cannot report the attempt. Exit a second agent without reporting and confirm its attempt becomes interrupted and the run pauses. Do this only when losing the current dev sessions is acceptable.
- [ ] **[VISUAL]** In an isolated rebuilt instance, edit and publish a seeded workflow draft in the Plans and Stories Designer tab; verify backend graph validation errors and the saved revision. The component layout was visually checked with a temporary Vite harness at normal and narrow widths (`~/Gits/.tmp/workflow-designer-visual.png`, `~/Gits/.tmp/workflow-designer-narrow.png`).

- [ ] After restarting Boss's desktop build when sessions can be interrupted, open Plans and Stories from the project toolbar and inspect the layout and stale-revision error. The isolated browser build already completed the create → start manual → check criterion → submit review → approve flow on 2026-09-25; screen capture is pending because agent-browser returned white images even for a solid-red test page and MCPMacControl.app lacks Screen Recording permission.

- [ ] After a restart in an isolated instance, start a run for a published `Resolve plan` definition through the workflow run API, discover it through `list_plan_runs`, page events from sequence zero, pause and resume, and confirm the run survives another restart. The runtime currently requires explicit commands; the scheduler is not yet connected.
- [ ] After rebuilding the isolated Rust instance, submit a managed `workflow_report` with `needs_input` and `inputRequest`; verify Run history shows the pause, an operator `answer_input` command survives restart, and `resume` is refused until an answer exists. Submit a reviewer report with criterion-indexed findings and verify the story status does not change from the report alone.
- [ ] After rebuilding the isolated Rust instance, report a story attempt while the plan coordinator is live. Confirm its inbox receives one `workflow_event` cursor and that replay from the cursor contains the committed report. A duplicate or late report must not produce another wake.
- [ ] After rebuilding an isolated Rust instance, launch two nonoverlapping story attempts from the active coordinator into two registered throwaway worktrees. Confirm distinct assignments appear in run history before spawn, a third attempt is held by the parallel limit, and overlapping or unknown scopes are serialized. Approve a prerequisite, run its pinned check in the assigned worktree, merge it into the canonical branch, then record integration and confirm the dependent releases. A failed post-merge check or later unrecorded ref movement must hold the dependent again.
- [ ] **[VISUAL]** In the isolated rebuilt instance, open **Run history** for a plan with real persisted events. Confirm the timeline advances after an IPC/SSE wake and remains readable at normal and narrow window widths. The component was visually checked with a temporary Vite harness and mocked IPC at both widths (`~/Gits/.tmp/workflow-timeline-visual.png`, `~/Gits/.tmp/workflow-timeline-narrow.png`); live backend integration awaits the Rust rebuild.

- [ ] In an isolated instance after restart, list workflow definitions for a project, edit a draft, publish revision 2, and confirm a run or reader pinned to revision 1 still sees revision 1. Confirm a malformed graph reports a validation error.

## Native story API (2026-09-24) — Rust, needs `make dev` restart

- [ ] After rebuilding an isolated instance, approve a prerequisite in a manual plan and confirm its dependent becomes Ready immediately. Start a workflow run for a separate plan, approve its prerequisite, and confirm the dependent stays Backlog until integration is recorded. Confirm desktop and headless startup remain responsive before opening a workflow; the first workflow access should recover prior active runs.

- [ ] After rebuilding an isolated instance, create a plan and story using `tuic story`, read them from the browser `/stories/action` route, and confirm another project's path cannot read their IDs. Claim from a live tab and verify stale revisions are rejected.
- [x] After rebuilding an isolated instance, create a plan and story using `tuic story`, read them from the browser `/stories/action` route, and confirm another project's path cannot read their IDs. Claim from a live tab and verify stale revisions are rejected. _(verified 2026-09-30: tuic story create_plan+create_story (repo fx/r1); POST :9881/stories/action?path= lists them; other project path (repoB): 'plan does not belong to project' for list/get_story/get_plan. Claim with live claude PTY -> in_progress rev2; stale expected_revision 0 and reused 1 -> 'stale story revision'. R)_

## Remote Project Progress event (2026-09-24) — Rust, needs `make dev` restart

- [ ] With a connected remote repository after restart, report a new Progress entry on the remote agent. The local desktop Progress bell and dialog update, and `progress_list` reads that repository's journal from its owning machine. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs desktop build: remote connection + local Progress bell/dialog are desktop frontend/remote_mirror. Headless instance has no remote connection to configure. (A second local tuic-remote could serve as 'remote' for the phase-2 agent.))_

## Voice auto-send is on by default (2026-09-24) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, with an instance whose `dictation-config.json` has no `auto_send` key (use `TUIC_APP_INSTANCE=<id>`, fresh config), Settings → Voice shows Auto-send on and a dictated phrase is sent with Enter. In basic mode the Auto-send row is hidden; switching it off makes it visible and the stored `false` survives an app restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Settings->Voice Auto-send default, basic/expert visibility and fresh dictation-config need the desktop frontend (+ mic for dictated send).)_

## "Add another GitHub account" is an expert entry point (2026-09-24) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, `curl localhost:9876/config/defaults` has `"github_accounts":{"accounts":[]}`. In Settings → Git & GitHub with no additional account, basic mode does not show the "Add another GitHub account" button, and Expert mode shows it. With one additional account configured (or a repository that needs an account), the "Additional GitHub Accounts" block with "Add another github.com account" and "Add Enterprise account" shows in basic mode. Before the restart the old backend has no `github_accounts` domain, so the button stays visible in basic mode. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Settings->Git&GitHub basic/expert rows and /config/defaults on desktop; headless GET /config/defaults is 404.)_

## Agent hook toggles store the default as absent (2026-09-24) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, in Settings → Agents, turn Claude's "Native status signals" off and on again, and Gemini's "Install hooks globally" on and off again. `agents.json` then has no `native_status_signals` / `hook_instrumentation` key for them. After a Settings reopen in basic mode, both rows are hidden. Signals and hooks still behave as enabled/disabled respectively. (Existing `agents.json` files that already hold `true`/`false` keep them until the toggle is used again.) _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Settings->Agents toggles and agents.json writes need the frontend (headless has no UI).)_

## Old config.json keeps `config`/`debug` MCP tools disabled (2026-09-24) — Rust, needs `make dev` restart

- [x] After a `make dev` restart, with a `config.json` that has no `disabled_native_tools` key (use `TUIC_APP_INSTANCE=<id>` and remove the key from that instance's config), the `config` and `debug` MCP tools are absent from `tools/list` and show as disabled in Settings. _(verified 2026-09-29: instance config.json has no disabled_native_tools key; tools/list omits config and debug (Settings display not checked here))_
## Agent list on the `+` buttons (2026-09-25) — frontend, HMR

- [ ] [HUMAN] Decide in the morning whether the sidebar `+` right-click should also open the agent list (withheld pending approval, AGENTS.md "Sidebar clicks"). Today: tab bar `+` right-click and long press open the agent list; sidebar branch `+` long press opens it; sidebar `+` right-click opens the branch menu. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Sub-agent tag icon in the sidebar (2026-09-25) — frontend, HMR

- [ ] [VISUAL] Spawn a sub-agent from an agent tab. Its sidebar row shows a small monochrome agent icon, aligned with the row text, instead of "↳ Parent". Hovering the icon shows "Spawned by <parent>". Take a screenshot (MCP maccontrol has no Screen Recording permission on 2026-09-25). _(NOT VERIFIED 2026-09-29: blocked — Needs agent action=spawn from an agent-typed tab (real agent CLI; all agents NOT FOUND; a fake binary is not detected as agent, arm test showed session not agent-typed).)_

## Mobile terminal prose reflow (2026-09-24) — frontend, refresh PWA

- [ ] [VISUAL] On a phone-width PWA session with Claude output produced in a wider desktop terminal, read a long paragraph: words flow across the phone width without a short orphan line at the desktop row boundary. Lists and box-drawing tables keep their own rows and alignment. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Voice library, voice files, Listen and loudness sliders (2026-09-24) — Rust + frontend, needs `make dev` restart

Settings > Voice > Spoken replies, with the Italian bundle and the runtime downloaded:

- [ ] [VISUAL] Downloadable starts collapsed as "DOWNLOADABLE (n)" with a disclosure marker; Tab focuses it, Enter or Space opens it, and the Voice volume and Levelling sliders are visible without scrolling past the catalogue. _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
- [ ] [VISUAL] Take a screenshot of the Spoken replies section. The group titles (Installed, Downloadable, Yours), the voice rows, the Listen button beside the voice picker and the two slider labels follow `docs/frontend/STYLE_GUIDE.md`. _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
- [x] Download one catalogue voice from Downloadable. The progress bar moves; when it ends, the row moves to Installed and the voice appears in the voice picker. _(verified 2026-09-29: Italian, Downloadable>alba(6MB) Download: row showed progress bar 0%->12%.. then left Downloadable; Installed group 'alba Downloaded'; voice select options [Default,giovanni,alba]; assets voice-italian-alba ready. Then deleted it (absent) and restored language auto.)_
- [ ] Click "Add voice file…" and choose a valid Italian `.safetensors` voice. It appears under Yours and in the picker, and it speaks when selected. Choose a 24-layer (French) voice or a file that is not a voice: the reason shows under the button and nothing is added. The × on a Yours row deletes the file. Add a file with the same name as a voice under Yours: it is refused with "You already have a voice called … delete it first or choose another name", and the stored voice still speaks as before. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 on this headless instance. Add voice file UI + import route need desktop build (also needs a valid Italian .safetensors fixture); 'speaks' part is ear-only.)_
- [ ] [HUMAN] With no hands-free conversation, click Listen and listen: the sample is audible, in the selected voice, at the Voice volume level, and the saved voice does not change. Start hands-free, let the agent speak a reply, and click Listen while it plays: the refusal shows inline. _(2026-09-24 isolated instance voice0924: `POST /dictation/speech/voices/preview` with hands-free not armed returned 200 for giovanni, alba and an imported voice, and the output device accepted the audio; UI Listen with alba showed no error. Nobody listened, so audibility is unverified.)_ _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): click Listen and hear the sample. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_
- [ ] Move Voice volume and Levelling and release. The next reply is louder or quieter and more or less even, without a restart. At -12 dB with Strong there is no clipping. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): judging loudness/clipping by ear. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_

HTTP import and delete (use the test instance on `:9877`):

- [ ] `POST /dictation/speech/voices/import` with `{"language":"it","name":"nonna","dataBase64":"<base64 of an Italian .safetensors>"}` succeeds. Voice choice then accepts `speech_voice` `"nonna"`, and a reply speaks in it. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 on this headless instance. POST /dictation/speech/voices/import needs desktop build and a valid Italian .safetensors fixture; spoken reply is ear-only.)_
- [x] The same with the French `8843db76` estelle file into language `"fr"` is refused with the "self_attn/pad … different model" reason, and nothing appears under `<speech>/user-voices/french/`. _(verified 2026-09-29: Installed French bundle via POST /dictation/speech/assets/download {asset:french} (removed afterward), POST /dictation/speech/voices/import {language:fr, 8843db76 french_24l/estelle.safetensors} -> error 'not a voice for French: ... transformer.layers.0.self_attn/pad ... made for a different model'; no user-voices dir created.)_
- [x] `POST /dictation/speech/voices/delete` with `{"language":"it","name":"nonna"}` removes the file. A reply with `speech_voice` `"nonna"` then reports that the voice is missing. _(verified 2026-09-29: Placed dummy models/speech/user-voices/italian/nonna.safetensors; POST voices/delete {it,nonna} -> "Deleted the Italian voice nonna", file gone. Missing voice reported via /voices/preview (same choose_voice): 'Italian has no voice called "nonna"; it offers giovanni')_
- [ ] Reinstall Italian from Settings > Voice while a downloaded voice (for example jean) and an imported voice are present. Both are still listed and still speak. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 on this headless instance. Needs desktop build with Italian bundle+jean+imported voice; 'still speak' is ear-only, listing part checkable via GET /dictation/speech/voices.)_
- [ ] `GET /dictation/speech/voices?language=it` lists giovanni as `default`, then the downloaded and the imported voices. `?language=xx` returns an error that names `xx`. With the Italian bundle not downloaded it returns `[]`, and the voice picker offers only "Default for this language". _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 on this headless instance. GET /dictation/speech/voices?language= needs desktop build; picker UI too.)_
- Note: an isolated `TUIC_APP_INSTANCE` debug instance binds no TCP port while remote access is off; reach it with `curl --unix-socket <tuic-mcp-*.sock>` (the path is in its log).

## Side panels follow an agent click to another repo (2026-09-24) — frontend, HMR

- [ ] Open the Notes, Git and Files panels on repo A. In the sidebar, click an agent row under repo B. All three panels now show repo B, as they do after a click on B's branch row. _(NOT VERIFIED 2026-09-29: blocked — Needs a second registered repo; web 'Add Repository' shows no path prompt here and MCP worktree_create does not register unregistered repos, so only fx/repo is registered.)_

## Spoken replies at one level (2026-09-24) — Rust, needs `make dev` restart

- [ ] [HUMAN] After the restart, arm hands-free and have the agent speak two replies in two different voices. Both sound equally loud, with no clipping or pumping. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): two spoken replies judged by ear. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_
- [ ] Set `speech_volume_db` to -24 in `dictation-config.json` through the settings save (`set_dictation_config`) while a reply is queued. The queued reply is not cut off, and the next reply is quieter. The same change is also available from the Voice volume slider in Settings > Voice > Spoken replies. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): audible comparison of reply loudness. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_
- [x] An existing `dictation-config.json` with neither field loads with -18 dB and 0.67 levelling (`GET /dictation/config` or `get_dictation_config`). _(verified 2026-09-29: wrote dictation-config.json {enabled,hotkey} only in the instance dir, GET /dictation/config -> speech_volume_db -18.0, speech_levelling 0.67 (file removed after))_

## MCP `session action=rename` and leaner output/spawn responses (2026-09-24) — Rust, needs `make dev` restart

- [ ] After a restart, call `session action=rename session_id=<id> name="Foo"` via MCP: the tab's display name in the sidebar/tab bar changes to "Foo" immediately. _(NOT VERIFIED 2026-09-30: partial — Backend only: MCP session action=rename name=Foo on live claude PTY -> GET /sessions display_name=Foo, display_name_is_custom=true, session list shows Foo at once. Sidebar/tab-bar rendering not observable on headless instance (needs desktop/browser build).)_
- [x] Rename again with `is_custom=false`: an agent's own OSC/intent title can then overwrite it, unlike a default (sticky) rename. _(verified 2026-09-29: session rename is_custom=false -> display_name_is_custom False; then shell printed OSC 2 'OSCTITLE' -> GET /sessions display_name became OSCTITLE. Rename default (sticky, is_custom True) then same OSC -> stayed 'Sticky' True.)_
- [x] `session action=rename` with no `name` or a blank one returns `{"error": ...}` and leaves the existing tab name untouched. _(verified 2026-09-29: session rename with missing/blank name -> {'error':'name (non-empty string) is required for action=rename'}, GET /sessions display_name stays 'Foo')_
- [x] `session action=output` on an idle Claude tab: the data ends at the agent's last output line, with no `❯`, separators or status-line/HUD rows. On a tab showing a permission dialog, the dialog and all its options are still there. _(verified 2026-09-30: Real claude (spawned via agent spawn): idle tab output data ends at '✻ Cooked for 3s · done' with no ❯ box, separators or status/HUD rows. Second claude (--permission-mode default) showing Bash permission dialog: output contains dialog title, command, question and options 1-4 (Yes/always/auto/No).)_
- [x] `agent action=spawn` returns no `*_with` fields; a registered orchestrator still gets `parent_session_id`. _(verified 2026-09-29: agent spawn (fake claude binary_path, agent_type=claude) returned only session_id,task_id,poll_interval_ms,name,server_ts,(communication_warning); no *_with keys. After agent register (ag3-orch), spawn also returned parent_session_id=<orch tuic_session>.)_

## No duplicated agent rows after a WebView reload (2026-09-24) — frontend via HMR

- [ ] Open an agent tab that shows the Context bar. Reload the WebView (`POST localhost:9876/debug/reload_webview`). Scroll up: the last reply must appear once. Before the fix, each reload added 1–2 copies of its top rows. _(NOT VERIFIED 2026-09-29: Needs a real agent tab showing the Context bar (real Claude session); raw-ring check alone does not cover it)_
- [ ] Check without the eye: `GET /sessions/{id}/raw-ring`. Each Claude full repaint (`ESC[2K` run) after the reload must clear exactly the tab's row count, not 1–2 more. _(NOT VERIFIED 2026-09-29: Needs real Claude full repaint output (ESC[2K runs) after WebView reload.)_

## Hands-free turns reach a busy agent at once (2026-09-23) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, arm hands-free on a Claude Code tab, give it a long task, and speak while it works. The turn appears in the terminal within a second or two (Claude queues it or takes it mid-turn), not after the turn ends. `GET /logs?source=dictation` shows `Hands-free turn typed now` with the session. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Hands-free arm on a real Claude tab and spoken turn needs desktop; audio can be fed as `say`-synthesized PCM over WS /dictation/hands-free/audio instead of a mic.)_
- [ ] While Claude shows a permission dialog, speak: nothing is typed into the dialog; the hands-free panel keeps showing the turn, and the log shows `Hands-free turn held` with `reason="confident question on screen"` once (not every 50 ms). Answer the dialog: the turn is typed. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free armed + permission dialog (reproducible with claude --permission-mode default, seen in item 529) + turn via audio WS; desktop only.)_
- [ ] Queue a typed command in the Compose panel while the agent is busy, then speak: the spoken turn is typed now, and the Compose badge still shows the typed command, which goes out at the next idle as before. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free armed and Compose panel (frontend); desktop only.)_
- [x] Stop hands-free: the status bar reads "Hands-free: stopped" (no "entries had already been typed" count any more). _(verified 2026-09-29: by code/test inspection, tests not executed here: useDictation.ts:68 sets 'Hands-free: stopped'; asserted at src/__tests__/hooks/useDictation.test.ts:120; old count text no longer exists (rg 'already been typed' = no matches).)_

## Opening AI Chat no longer aborts the app (2026-09-23) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, open the AI Chat panel (ego over ACP) and send a prompt. The app stays up and the reply streams in. Before the fix, `acp_subscribe` called `tokio::spawn` on the main thread, which panicked with `TryCurrentError` (SIGABRT, crash report `tuicommander-2026-09-23-164623.ips`) and killed every PTY session. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: AI Chat panel (ego over ACP) and acp_subscribe main-thread panic are Tauri desktop-only; headless tuic-remote has no main thread/UI. Needs desktop build with the frontend AI Chat panel.)_

## Voiceless dictation languages are marked (2026-09-23) — frontend via HMR

- [x] [VISUAL] Settings → Dictation → Language: languages without a speech bundle (e.g. Japanese) read "Japanese — no spoken replies"; Italian/English and Auto-detect carry no marker. Choosing Japanese shows a hint under the select that replies will not be spoken and suggests Auto-detect; choosing Italian or Auto-detect hides it. _(verified 2026-09-29: web UI Settings > Voice > Language: Dutch/Japanese/Chinese/Korean/Russian read '— no spoken replies', Auto-detect/English/Italian carry none; choosing Japanese shows 'Replies in this language will not be spoken… choose it or Auto-detect', Italian and Auto-detect hide it)_

## Readable Design Mode tab badge (2026-09-23) — frontend via HMR

- [ ] [VISUAL] Arm Design Mode on an agent tab, then stop it. The boxed `D·` badge in the tab is readable on the dark tab bar: letter and border in `--fg-primary`, 11px text. The armed `D` stays green. _(NOT VERIFIED 2026-09-29: blocked — Design Mode arm/stop needs an agent tab (D badge) and Chrome; only shell tabs exist (no agent CLI).)_

## Hands-free from the Command Palette (2026-09-23) — frontend only, Vite HMR

- [ ] [HUMAN] With dictation enabled, open the Command Palette on an agent tab and run "Start hands-free conversation". Speak a sentence: it reaches that tab and the first earcon plays (priming happened inside the palette click). Switch tabs and reopen the palette: the entry reads "Stop hands-free conversation"; run it and the conversation stops. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Reply language stated once per hands-free conversation (2026-09-23) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, with the dictation language on Auto-detect, arm hands-free on an idle agent tab and say two Italian sentences, then one English sentence. The terminal receives `<first> (reply in Italian)`, the second sentence with no suffix, and `<english> (reply in English)`. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free + Whisper Auto-detect; feed Italian/English `say` audio over WS /dictation/hands-free/audio on desktop build.)_
- [ ] After the same restart, set the dictation language to Italian and arm again: the start notice ends with `Reply in Italian.` and the first spoken turn carries no suffix. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free arm and dictation language setting; desktop only.)_

## Custom hands-free start notice (2026-09-23) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, in Settings → Dictation → Hands-free, type a two-line start notice and arm hands-free on an idle agent tab: the agent receives your text as one line, not the built-in notice. Press **Reset to default**, disarm and arm again: the built-in notice is sent. `GET http://localhost:9876/dictation/hands-free/default-notice` returns the built-in text. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs Settings->Dictation UI plus arm on an agent-typed session; desktop only.)_
- [ ] After the same restart, the **Start notice** textarea in Settings → Dictation shows the built-in text in grey as its placeholder. Before the restart it is empty and the log reads `Failed to load the default hands-free start notice`, because the old backend has no such endpoint. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs Settings->Dictation textarea placeholder; desktop only.)_

## Activation phrase survives Whisper's spelling (2026-09-23) — Rust, needs `make dev` restart

- [ ] [HUMAN] After a `make dev` restart, with the activation phrase `senti mac`, arm hands-free and say three sentences that begin with "senti mac". Each one is queued, without the phrase. Then say "senti, ma che ore sono?" to someone else: it is dropped. For every dropped turn, `curl 'localhost:9876/logs?source=dictation'` shows the line `Hands-free turn dropped…` with a `heard=` field that holds the first four words Whisper wrote. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): spoken activation-phrase turns via microphone. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_

## Parked voice turns leave together (2026-09-23) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, arm hands-free on a Claude tab, give it a long task, and speak three short turns while it works. At the next idle the three turns arrive together as one message, in the order spoken, one `(reply in Italian)` line each — not one message per agent turn. A command typed into Compose between the turns still arrives as its own message. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free arm on real Claude tab and 3 turns via audio WS; desktop only.)_

## Settings consistency: Language, ego, Spoken replies (2026-09-23) — Rust part needs `make dev` restart

- [x] [VISUAL] Settings → General: no Language picker (only English ships). The AI Chat section (Experimental Features on) looks like TUIC CLI: `?` tooltip, green "Configured at …", **Select…** opens a file picker and saves the pick, **Clear** empties it. _(verified 2026-09-23 by screenshot after restart: no Language row; AI Chat shows the `?` badge, the not-configured hint and Select…, matching TUIC CLI. Select/Clear not clicked — `ego_executable` is empty on disk)_
- [x] [VISUAL] Settings → Dictation → Spoken replies: rows look like the Whisper list. Only the language replies are spoken in is highlighted with "Active"; a running download shows its bar and a × to cancel. _(verified 2026-09-23 by screenshot: Downloaded badges and × on every ready bundle, highlight and Active on Italian only, as on Whisper Large V3 Turbo. The in-progress bar was not observed on screen)_
- [x] After the restart, start a speech download with `curl -X POST localhost:9876/dictation/speech/assets/download -H 'content-type: application/json' -d '{"asset":"german"}'` while Settings → Dictation is open. The bar climbs, then the row turns to Downloaded by itself instead of staying at 100% with a cancel control. _(verified 2026-09-23 on the restarted instance: German download over HTTP emitted 8000+ progress events on `/events`, then `{"asset":"german","done":true}`; catalogue reports `ready`. The store clearing the bar on `done` is covered by `dictation.test.ts` "clears a finished download this client never started")_

## Echo reference covers the whole reply (2026-09-23) — Rust, needs a `make dev` restart

- [ ] [HUMAN] After the restart, use the laptop speakers (no headphones). Arm hands-free with a voice and ask for a reply of at least 10 seconds. The reply plays to the end without cutting itself off, and no turn arrives that repeats its words. In `tuic.log`, no `speech: hushed` line appears during the reply, and there is no `echo: far-end reference full` warning. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): laptop speakers echo test. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_
- [ ] [HUMAN] Talk over a long reply after its first 3 seconds. It stops. The log shows `speech: hushed` with `speaking=true` and an `into_playback_ms` above 3000, then `Hands-free turn accepted heard=` with your words. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): talking over a reply. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_

## Barge-in waits for sustained speech (2026-09-23) — Rust, needs a `make dev` restart

- [ ] [HUMAN] After the restart, use the laptop speakers (no headphones). Arm hands-free with a voice and ask a question. The spoken reply plays to the end unless you talk over it. Talk over a second reply: it stops within about a quarter of a second, and your first words are in the transcript. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): laptop speakers barge-in. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_
- [ ] [HUMAN] After the restart, in hands-free, say half a sentence, pause about a second, and finish it before the hold-back ends. One turn arrives with both halves; the first half is not sent alone. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): speaking half sentences into a mic. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_
- [ ] [HUMAN] After the restart, arm hands-free with the activation phrase on the desktop. Say a phrase, pause about four seconds, then continue without the phrase. The pending text gains the continuation and the agent receives one message after the final five-second window. Check the microphone level while TTS plays and whether the reply's own words appear as a new turn; these acoustic and gain observations require the real device. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): speaking with pauses, mic level with TTS. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_

## Calm hands-free voice meter (2026-09-23) — frontend via HMR; the Rust level fallback needs a `make dev` restart

- [ ] [VISUAL] Arm hands-free. The toast shows one thin horizontal voice meter and the phase text — no pulsing dot and no moving dots after the text. Push-to-talk (dictation hotkey) still shows the bar meter, the dot and the dots. _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
- [ ] Stay silent with normal room noise (fan, typing): the voice meter stays flat. Speak: it fills at once and falls back smoothly over about 1.5 s after you stop, with no flicker between words. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): voice meter reaction to real speech/room noise. Also desktop-only (dictation routes 404 on headless).)_

## Unmerged worktree removal (2026-09-23) — Rust, needs `make dev` restart

- [ ] After the restart, a clean worktree with commits not in the default branch shows `Unmerged` in the sidebar even with no diff or dirty badge. Choosing Delete Worktree while **Delete branch on remove** is on keeps the worktree and its terminals and explains that the branch has unmerged commits. With that setting off, removal keeps the local branch. _(NOT VERIFIED 2026-09-30: partial — Backend via MCP: clean worktree with 1 extra commit -> commit_status=unmerged, dirty_files=0. worktree_remove delete_branch=true refused 'branch has unmerged commits', dir+branch kept; delete_branch=false removes worktree, removal_rule kept_branch, branch kept. Sidebar 'Unmerged' label and its termi)_

## Sub-agent tags and branch count (2026-09-23) — Rust, needs `make dev` restart

- [ ] [VISUAL] After the restart, the in-window Activity Dashboard stays close to the detached window's width; a long terminal name remains readable beside the robot marker and project badge, and a narrow main window has no horizontal overflow. _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
- [x] [VISUAL] Sidebar: a branch whose agent list is expanded shows no number on its icon; collapse it and the number comes back. _(verified 2026-09-29: Nested Terminal Tabs enabled via Settings>Appearance. Branch icon toggle expanded: .branchAgentCount absent (null); after collapse count '11' shown (DOM). No screenshot (timeouts).)_
- [ ] After the restart, spawn an agent with `agent action=spawn` from an agent tab. In the Activity Dashboard (`Cmd+Shift+A`) the child row shows a robot-head icon with tooltip `Spawned by <parent tab name>`, and the parent row shows no icon. Rename the parent tab: the tooltip follows. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend seen: GET /sessions child has parent_session=<parent id> (spawn from real claude tab via x-tuic-session=parent). Robot icon, tooltip 'Spawned by <parent>', rename-follow are Activity Dashboard UI (desktop/browser build).)_
- [ ] [VISUAL] Sidebar nested agent rows: the spawned child shows the same robot-head icon; with a long parent name, the tab title stays readable. _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
- [ ] Pop out the Activity Dashboard: the detached window shows the same icon and parent tooltip. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Pop-out Activity Dashboard is a desktop window feature; needs desktop build. Backend parent_session on spawned child confirmed (see 608).)_
- [ ] After the restart, with a child marked as a subagent, run `curl -X POST http://localhost:9876/debug/reload_webview`: the icon and resolved parent name remain. Open a browser-mode agent tab with no spawn name, let Claude set its OSC title, reload: the tab keeps following later OSC titles. A named `agent action=spawn` tab still ignores Claude's OSC title after the reload. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs POST /debug/reload_webview on desktop WebView and frontend tab-title logic (OSC titles); headless has no WebView. Real claude sets OSC titles fine when spawned with env CLAUDE_CODE_CHILD_SESSION=0.)_

## Voices from Kyutai's ungated repository (2026-09-23) — **Rust, needs a `make dev` restart**

- [x] Download reaches Ready with no hash error _(verified 2026-09-23: `POST /dictation/speech/assets/download` for onnxruntime, italian, english, french; all `ready`, sha256 checked during install)_
- [ ] [HUMAN] A hands-free reply in Italian is spoken with the giovanni voice (`/dictation/speech/speak` refuses while hands-free is not armed). _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): hearing giovanni voice. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_
- [ ] **Rust, needs another `make dev` restart:** arm hands-free on a hand-opened Claude tab, then `voice action=status` from that tab reports `available: true` instead of "Speech is bound to another session" (caller now resolved to its live PTY, `resolve_mcp_origin_pty`). _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free armed on a real Claude tab, then MCP voice status; on headless voice status returns 'connection is not bound to a terminal'.)_
- [ ] Settings → Dictation → Spoken replies lists English, French, German, Italian, Portuguese and Spanish. Set the Whisper language to English, download English, and a hands-free reply is spoken in English with the alba voice. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Settings->Dictation list, ~390MB downloads; audible part is ear-only. Desktop only.)_
- [ ] French (24-layer, ~390 MB): download and speak one reply. The engine reads the layer count from `bundle.json`, but no 24-layer bundle has been run here before. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). French bundle download+speak; desktop only; audible verification is ear-only.)_

## Alias survives a WebView reload (2026-09-23) — Rust, needs `make dev` restart

- [ ] After the restart, spawn an agent with `agent action=spawn`, then reload the WebView (`curl -X POST localhost:9876/debug/reload_webview`). The tab context menu still shows "Alias: …" and the tooltip shows the alias that `session list` returns for that session. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs /debug/reload_webview and tab context menu 'Alias:' UI (desktop). Backend: alias present in session list (re-N) after agent spawn via MCP.)_
- [x] Browser mode (`http://localhost:9876/`): with the page open, spawn an agent. Its new tab shows the alias without a page reload (`term-alias-assigned` now arrives over `/events`). _(verified 2026-09-23: live `/events` SSE on :9876 delivered `event: term-alias-assigned` `{"session_id":…,"alias":"tt-1"}` for a throwaway `POST /sessions`; listener in `useAppInit.ts:596`; tests `assign_term_alias_publishes_the_alias_on_the_event_bus` + `term_alias_assigned_has_matching_sse_name_and_payload` pass. Browser tab rendering itself not observed.)_
- [ ] Spawn a Claude agent with `name=call-map`. When Claude prints its session title, the tab still reads `call-map` (an `intent:` title may still replace it, a manual rename too). Reload the WebView: the name is still protected from the OSC title. _(frontend half is live via HMR; the reload check needs the restart)_ _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend seen: spawned claude tab keeps display_name (from_spawn=true, e.g. r1-tx) through Claude's turns. Frontend OSC-title protection and WebView reload need the desktop/browser build.)_

## Compose panel pin (2026-09-23) — frontend, live via HMR

- [ ] [VISUAL] `Cmd+I` in an agent tab: the panel is one row shorter than before. Click the pin: the terminal shrinks above the panel, the agent redraws at the new height, and no row hides behind the panel. Unpin: the panel overlays the terminal again and the terminal regains its full height. _(NOT VERIFIED 2026-09-29: partial — Pin button toggles (title 'Pin to the terminal bottom' <-> 'Unpin from the terminal bottom', aria-pressed). Terminal geometry not measurable: canvas height stayed 150px pinned vs unpinned, PTY screen_lines unchanged (38/24/13/24); panel-one-row-shorter and redraw at new height unverified; no screenshot.)_
- [ ] Pinned: `Ctrl+Enter` sends, the editor empties and keeps the caret; the panel stays open. `Shift+Ctrl+Enter` does the same through the queue. _(NOT VERIFIED 2026-09-29: partial — Pinned: typed text + Send button: session got text, editor emptied, panel stayed open (verified). Ctrl+Enter / Shift+Ctrl+Enter via agent-browser press never reached the page (document keydown listener saw 0 events), so key bindings and caret retention untested; shell tab not agent.)_
- [ ] Pinned: click in the terminal and type — the caret stays in the terminal (not pulled back into the panel). `Cmd+I` and `Esc` move the caret between the panel and the terminal. _(NOT VERIFIED 2026-09-29: partial — Pinned compose panel open, trusted mouse click on terminal area: document.activeElement is the terminal's INPUT (not inside the panel), panel stays open (caret stays in terminal). Cmd+I/Esc caret moves and typed-in-terminal echo not testable: agent-browser key presses do not reach the page.)_
- [ ] Pinned in one tab only: another tab's compose panel still closes after send. _(NOT VERIFIED 2026-09-29: blocked — Needs a second terminal tab active: trusted clicks on the 'Foo' tab and its sidebar row did not switch away from Terminal 4 in this web session, so cross-tab pinned/unpinned behaviour could not be compared.)_
- [x] The ✕ at the right of the status bar closes the panel, pinned or not; reopening with `Cmd+I` shows it unpinned. _(verified 2026-09-29: Compose panel (opened via 'Compose' hint on Terminal 4 shell tab; browser Cmd+I not delivered): pinned (aria-pressed true) -> x 'Close compose panel' at status bar right (1107,839) closed it; reopened -> pin aria-pressed false. Unpinned x also closes.)_

## Hands-free earcons (2026-09-23) — **Rust, needs a `make dev` restart**

- [ ] Arm hands-free on the desktop, speak a turn: after the hold-back a short, quiet blip plays as it reaches the agent. Set an activation phrase and speak without it: a softer, lower blip plays and nothing is sent. Neither blip opens a turn or stops a spoken reply. Repeat from a browser tab at `:9876` — the tab beeps, the desktop stays silent. Also check the first blip after arming from the global hotkey is audible (WKWebView may keep an ungestured AudioContext suspended). Turn the Earcons setting off and repeat: no sound on the desktop, and none in a browser tab armed after the change. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): earcon blips heard through speakers. Also desktop-only (dictation routes 404 on headless).)_

- [ ] Earcon redesign (frontend, live via HMR): a delivered turn plays two short **rising** notes; a dropped turn plays two softer **falling** notes. Each is recognisable without hearing the other, and neither opens a turn, appears as captured speech, or stops a spoken reply. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): earcon note direction judged by ear. Also desktop-only (dictation routes 404 on headless).)_

## Dictation auto-send on long text (2026-09-23) — frontend, live via HMR

- [ ] With Auto-send on, dictate 30+ seconds into a Claude tab: the text is submitted without pressing Enter, and no "Removed 1 invisible character" notice appears. _(NOT VERIFIED 2026-09-29: Needs 30+ s of real dictated audio into a Claude tab.)_
- [ ] Dictate a short phrase into Claude and into a Codex tab: both still submit. _(NOT VERIFIED 2026-09-29: Needs real dictation audio and real Claude and Codex tabs.)_
- [ ] **After a `make dev` restart (Rust):** `session action=submit` with a 600+ char input to a Claude tab returns `acknowledged:true` and the prompt runs; a peer `agent send` of a long message also submits. _(NOT VERIFIED 2026-09-29: Needs a real Claude tab to run the long prompt and peer agent send)_

## New-tab long press and settings button spacing (2026-09-23) — frontend, live via HMR

- [ ] Hold the `+` in the tab bar for half a second: a menu lists the enabled agents (a submenu per agent with 2+ run configs). Picking one opens a new tab in the active branch that starts the agent. Releasing does not also open a plain tab. _(NOT VERIFIED 2026-09-29: partial — Web UI: mouse down on tab-bar '+' held 0.8s (agent-browser mouse down/up): no menu appeared and a plain terminal opened (tabs 10->11). Instance lists all agents NOT FOUND, so getNewAgentMenuItems is empty; agent menu/submenu/new-agent-tab not verifiable.)_
- [ ] A quick click on `+` still opens a plain terminal; right-click still shows New Tab / Split. _(NOT VERIFIED 2026-09-29: partial — Quick click on tab-bar '+' opened a plain terminal (close buttons 9->10). Trusted right-click (mouse down/up right at 1426,47) showed no menu at all; createAgentLaunchMenu default rightClick=true opens only the agent list (empty: all agents NOT FOUND) — no 'New Tab / Split' menu observed; claim unconfirmed.)_
- [ ] [VISUAL] Settings → Dictation → Voice tuning: "Level gate" no longer touches the "Start test recording" button. Also check the Import/Export row and the Notifications "Reset Defaults" footer. _(NOT VERIFIED 2026-09-29: partial — capture_window blocked (no Screen Recording perm) so no screenshot. DOM geometry via invoke_js in validate instance: Start-test button bottom 1172 vs Level gate label top 1190 (18px gap); Import/Export buttons 8px apart, 8px below prev row; Notifications Reset Defaults footer 20px below prev block. Human eyeball still advised.)_

## Mobile Progress header (story 1214-2b95)

- [ ] [HUMAN] On a 360 px and a 390 px phone, open the Progress tab with a project available. Confirm the title, project and terminal selectors, **List | Flow**, and **Blocked only** are visible without horizontal scrolling; tap both view choices and the filter. Browser geometry at those widths is checked separately; this item covers real touch and device rendering. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Progress Flow view (2026-09-23) — **Rust, needs a `make dev` restart**

The List | Flow toggle is frontend and appears through HMR at once, but the running backend has no `progress_flow` until it restarts, so Flow shows an error line until then. The first open after the restart migrates `progress.sqlite3` (table rebuild for the new kinds).

- [ ] After the restart, open Progress: the List still shows every older entry, and deleting one still works. _(2026-09-23 after restart: `POST /progress/list` returns 200 with 349 entries, ids 388–1618, so older entries are back; delete not exercised on live data.)_ _(OLD NOTE 2026-09-23: FAILED on :9876. `POST /progress/list` and `POST /progress/flow` both answer `progress_store_unavailable: cannot read the progress id high-water mark: no such table: sqlite_sequence`. The live `progress.sqlite3` (1614 entries) was created with a bare `id INTEGER PRIMARY KEY` (no AUTOINCREMENT), so `sqlite_sequence` does not exist and `rebuild_for_hand_offs` (`progress/store.rs:146-153`) errors in every `ProgressStore::open()`. No entry has been written since the restart (newest `created_at_ms` 12:04). The migration test only covers an AUTOINCREMENT journal.)_ _(NOT VERIFIED 2026-09-30: partial — API: POST /progress/list -> entries+total+ptyIds; POST /progress/delete {ids:[3]} -> deleted 1, list shrinks 8->7. Store opens fine on this fresh journal. NOT observed: legacy non-AUTOINCREMENT progress.sqlite3 migration (would need hand-made DB in instance dir) and the Progress dialog UI.)_
- [ ] From a Claude terminal in a registered repo, `agent action=spawn` a peer with a prompt. List shows `delegated to <child>`; Flow shows a blue arrow from the parent's column to the child's, labelled with the prompt. _(NOTE 2026-09-23: blocked — the progress store fails to open on :9876, see the first item of this section.)_ _(NOT VERIFIED 2026-09-30: partial — Real claude parent, agent action=spawn peer with prompt (x-tuic-session=parent PTY id): /progress/list has type=delegated, targetName=<child>, text=prompt; /progress/flow has delegated event parent->child, summary=prompt. Needs repo registered (POST /watchers/repo). Blue arrow/'delegated to' label n)_
- [ ] Have the child `agent action=send` to the parent and report `done`. Flow shows a grey message arrow child → parent, then a green return arrow child → parent. No toast for the delegation or the message; one silent toast for `done`. _(NOTE 2026-09-23: blocked — the progress store fails to open on :9876, see the first item of this section.)_ _(NOT VERIFIED 2026-09-30: partial — Child PTY id as x-tuic-session: agent send to parent -> journal type=message; progress type=done -> type=done. /progress/flow events: message then done, both child->parent. Toast behaviour and grey/green arrow colours not observed (UI).)_
- [x] Spawn the child into a managed worktree (`repo action=worktree_create spawn_session`). Its `intent:` now appears on its column (it was dropped before). _(NOTE 2026-09-23: blocked — the progress store fails to open on :9876, see the first item of this section.)_ _(verified 2026-09-30: Worktree from repo action=worktree_create, real claude spawned with cwd=worktree printing 'intent: ...': dropped while workspace unregistered, recorded (entry filed under parent project, flow participant.intent set) once repos.<r>.workspaces registered via PUT /config/repositories (what the frontend)_
- [ ] In a Claude terminal, run 2 subagents (one nested). Each gets a column under its terminal with a tool count; the dashed arrow carries its task and, once done, a green arrow carries its report. Clicking a long label fetches the full text, with any token shown as `[REDACTED]`. _(NOTE 2026-09-23: blocked — the progress store fails to open on :9876, see the first item of this section.)_ _(FAILED 2026-09-30 story 1298-c8d8: Real claude (env CLAUDE_CODE_CHILD_SESSION=0 so transcripts persist): alpha, beta+nested gamma, delta subagents. Columns, toolCalls, nested parent, [REDACTED], beta return arrow OK. alpha/gamma/delta stay 'running', no return: jsonl ends in SubagentHandback tool_result, no end_turn; subagent_map.rs )_
- [ ] Select one terminal in the selector: Flow keeps that terminal, its parent and its children only. _(NOTE 2026-09-23: blocked — the progress store fails to open on :9876, see the first item of this section.)_ _(NOT VERIFIED 2026-09-30: partial — POST /progress/flow with ptyId=<wtchild> -> participants [parent Foo, wtchild], 2 events; ptyId=<parent> -> parent + its 4 children only. Selector UI not observed.)_
- [ ] [VISUAL] With 6+ columns: the header row stays pinned while scrolling, every arrow ends on a lifeline, and the dialog scrolls sideways rather than squashing columns. _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
- [x] **Store fix, needs another `make dev` restart:** after the restart, one `progress` call (or `POST /progress/list`) succeeds, and the live journal's schema (`SELECT sql FROM sqlite_master WHERE name='entries'` on `<config dir>/progress.sqlite3`) contains `'delegated'`. The legacy no-AUTOINCREMENT journal is migrated with every id kept. _(verified 2026-09-23 after restart: `progress` returned id 1617; schema now `INTEGER PRIMARY KEY AUTOINCREMENT` with `'delegated'` in the CHECK; `sqlite_sequence` = 1617 = MAX(id); `POST /progress/list` and `/progress/flow` answer 200, list ids 388–1618.)_
- [ ] After the same restart: in a Claude session with 65+ subagents, the newest and every running one still get a Flow column; expand a Flow row, let a new entry arrive, and the same row stays expanded. _(NOT VERIFIED 2026-09-30: partial — Real claude launched 70 haiku subagents (74 in session). /progress/flow: 73 subagent columns, missing only oldest finished 'beta'; truncated=false. All 73 report 'running' though finished (SubagentHandback, see 665), so cap on finished ones not exercised cleanly. Row-stays-expanded is UI, not observ)_
- [x] `curl -s -o /dev/null -w '%{http_code}' localhost:9876/agents/map` answers `404`: the map page is removed. _(verified 2026-09-23: returned `404` on :9876.)_

## Markdown block review handoff (2026-09-23) — frontend, live via HMR

- [x] An MCP-opened absolute Markdown file outside the active repository writes tweak comments through the external file route; a rejected write shows an error and leaves the draft available to retry. _(verified: `MarkdownTab.test.tsx` exercises external save and rejected-then-successful retry.)_
- [x] A terminal file path with `:line` opens the built-in editor at that line; a numbered Markdown path does the same while an unnumbered Markdown path opens the viewer. _(verified: `terminalFileOpen.test.ts` covers the numbered routes and unnumbered viewer route.)_
- [x] Clicking a later `:line` or `:line:col` terminal link for a file already open in the editor moves the cursor there and retains unsaved edits. _(verified: `editorOpenLinks.test.tsx` checks repeat navigation, oversized column clamping, and retained edits.)_

- [ ] In a Markdown file with plain, task, and nested bullets, hover each bullet's gutter and save a comment. Confirm each highlight stays on its chosen bullet and the file contains an indented `tweak:item` marker directly below that bullet's own content. _(Automated: `MarkdownTab.test.tsx` writes the selected task marker; `ContentRenderer.test.tsx` checks nested source targets; `tweakComments.test.ts` checks nested anchors and list rendering.)_
- [ ] [VISUAL] Hover beside a Markdown block: its rule stays in the gutter with clear space before the text, and the comment button does not cover the block. _(NOT VERIFIED 2026-09-29: blocked — Markdown tab opened (agb.md) but rendered blocks carry no data-comment-source-start attributes and hovering the gutter (mouse move 340,241) produced no comment button/rule; feature not reachable in this web session; no visual check possible.)_
- [ ] Click a task-list checkbox inside a commented block: it cycles state without opening the comment popover; task rows after a comment containing `- [ ]` still update the correct source line. _(NOT VERIFIED 2026-09-29: blocked — Same as 671: no comment overlay hooks in DOM (data-comment-source-start count 0) for the file opened from the file browser; task checkbox cycle with comments not testable. Task checkboxes render (2 unchecked inputs).)_
- [ ] With tweak comments in the file, choose a same-repository agent in the Markdown topbar and click **Send**. An idle agent receives the request immediately; a busy agent shows one queued command and receives it on its next idle transition. _(NOT VERIFIED 2026-09-29: Needs a real agent tab in the same repository with idle/busy transitions to observe immediate delivery vs queued delivery.)_

## Design Mode (2026-09-23) — **Rust, needs a `make dev` restart**

The existing `make dev` process does not hot-reload Rust. Restart it when the
current agent sessions can be closed, or use a separate debug instance with
`TUIC_APP_INSTANCE=<id>` to keep its configuration isolated. Targeted tests
cover the individual contracts; this check joins them in a real Chrome and
agent session.

An isolated `make dev` attempt on 2026-09-23 stopped before launch because
port 1421 was already serving a different checkout's Vite server. Do not stop
that checkout merely to run this check.

- [ ] In the restarted instance, set a repository's **Dev Server URL** to a _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. )_
      local page with a click handler. Start Design Mode from an agent tab: a
      dedicated Chrome window opens the configured URL, hovering highlights an
      element, and clicking selects it without firing the page handler.
- [ ] Begin typing a note in the bound agent's composer, select two elements, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Also needs composer prefill (pty prefill_agent_input))_
      and confirm both grab blocks appear alongside the untouched note without
      submitting. Check selector, path, style subset, rectangle, HTML snippet,
      nearby text, source location when the dev build supplies one, and a valid
      `[image: …]` PNG path.
- [ ] On a page whose CSS uses custom properties (for example a Tailwind v4 _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Needs a Tailwind v4/shadcn fixture page as Dev Server)_
      or shadcn app), select a themed button. The grab carries a `tokens:` line
      with only the `--…` variables that button's rules reference, resolved to
      their values, and not the whole theme.
- [ ] On a page built from web components (open shadow roots with their own _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Needs a shadow-root web-component fixture page as Dev)_
      `<style>` or `adoptedStyleSheets`), select a button inside a component.
      The `tokens:` line lists the component's own `--…` variables. jsdom has no
      `ShadowRoot.styleSheets`, so no unit test covers this.
- [ ] Start Design Mode from another agent terminal in the same repository. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Needs two agent tabs in one repo.)_
      Confirm the existing Chrome window is reused and subsequent grabs go to
      the newly bound terminal. Close that terminal, then Chrome: the status
      indicator must show Stopped and no further grab may be delivered.
- [ ] With a separate debug instance, check that quitting TUICommander closes _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Needs a separate debug desktop instance quit.)_
      only the Chrome windows it owns. A browser/PWA start must explain that
      Chrome opens on the host machine.
- [ ] Review fixes (2026-09-23): a page running `debugger;` keeps responding; _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Needs a page running debugger; plus Vite dev page fix)_
      a Vite/webpack dev page resolves `source:` for most components, not only
      the first four modules; closing the bound terminal turns the hover
      highlight off, and a later start delivers no click made in between;
      closing Chrome and starting again works on the first click.

## Terminal Progress (2026-09-23) — Rust, needs a `make dev` restart

- [ ] After restarting an isolated dev instance, open two agent PTYs in one repo and record different `intent:`/`progress` entries. Progress should open on the active PTY, allow switching to the other PTY, and show both in **All repo**. _(NOTE 2026-09-23: blocked — every `ProgressStore::open()` fails on :9876 (`no such table: sqlite_sequence`), see Progress Flow view, first item.)_ _(NOT VERIFIED 2026-09-30: partial — Backend: /progress/list {ptyId:A} returns only A's entries; unfiltered returns all PTYs' entries; ptyIds lists every PTY with history. Progress dialog default-to-active-PTY and switching are UI (not observed).)_
- [ ] In **All repo**, existing entries recorded before this change should remain visible as **Terminal unknown**. Closing a PTY should leave its saved history selectable. _(NOTE 2026-09-23: blocked — progress store fails to open on :9876, see Progress Flow view, first item.)_ _(NOT VERIFIED 2026-09-30: partial — Backend: POST /progress/report without pty -> entry ptyId=null in unfiltered list ('Terminal unknown' source). Closed PTYs (DELETE /sessions) stay in ptyIds, history stays selectable. Label and dialog selection are UI, not observed. Pre-change legacy rows not tested.)_
- [ ] Open PTY A, switch to PTY B, then **All repo** and close Progress. Reopen each view and confirm each has its own last-visit divider; viewing A alone must not mark B or **All repo** as seen. _(NOTE 2026-09-23: blocked — progress store fails to open on :9876, see Progress Flow view, first item.)_ _(NOT VERIFIED 2026-09-30: partial — API: POST /progress/viewed?ptyId=A sets lastViewedMs for A only; list for B and for All (no ptyId) still null; POST /progress/viewed (All) sets only All, B stays null. Per-view state isolation confirmed. Divider rendering and dialog open/close flow not observed (UI).)_
- [x] Start a **new** Claude Code session after the restart (a running one keeps its old tool list). Its tool list must show `mcp__tuicommander__progress` with a full schema, not as a deferred name behind ToolSearch; at the end of a task it must report `done` without being asked. Before this change only Codex terminals wrote done/blocked entries (`anthropic/alwaysLoad` on the `progress` tool, `mcp_transport.rs`). _(NOTE 2026-09-23: first half passes — a Claude Code subagent session started at 13:41 against :9876 received `mcp__tuicommander__progress` with its full schema, not deferred; `progress_is_the_only_tool_claude_code_must_not_defer` passes. Second half fails: the `done` write cannot land while the progress store fails to open (see Progress Flow view, first item).)_ _(verified 2026-09-30: Spawned real claude (sonnet) against rust0930 via tuic-bridge; it reported mcp__tuicommander__progress schema loaded, other 9 tools deferred. Then a plain file-write task: agent called progress unprompted; /repo progress_list has entry type=done 'Created hello730.txt...' ptyId=session. Note: hooks f)_

## Terminal scrollbar thumb minimum 48px (2026-09-23) — frontend, live via HMR

- [ ] [VISUAL] A terminal with a very long scrollback (e.g. `seq 100000`): the thumb stays 48px tall and is easy to grab; dragging it scrolls the whole history, top to bottom. _(NOT VERIFIED 2026-09-29: partial — seq 100000 in Terminal 4 session: scroll-info total_lines 10024, screen_lines 24 (long scrollback reached). Thumb is drawn on the canvas (no DOM element; canvas 300x150), screenshot times out, so 48px height/drag not measured. Code: scrollbarThumb.ts MIN_THUMB_PX=48, height=min(trackH,max(48,trackH*ratio)) (read only).)_
- [ ] [VISUAL] A short scrollback still gets a proportional (larger) thumb; a very short split pane never shows a thumb taller than its track. _(NOT VERIFIED 2026-09-29: partial — Code read only: scrollbarThumb.ts:41 height=min(trackH,max(MIN_THUMB_PX,trackH*ratio)) caps thumb at track height; canvas-drawn thumb not measurable in this web session (no DOM, screenshot timeouts).)_

## Branch icon toggles agents (2026-09-22) — frontend, live via HMR

- [ ] [VISUAL] Hover the icon of a branch with terminals: it swaps to a chevron (pointing down when expanded) in the same box; the row does not shift. _(NOT VERIFIED 2026-09-29: partial — Hover over branch icon (mouse move): chevron opacity 0->1, icon opacity 1->0, toggle box identical [17,68,14,12], row height 26 unchanged; chevron pointing-down not judged (transform none, no screenshot).)_
- [x] Click the icon: agents expand/collapse, the branch does NOT open. Click the row: the branch opens, agents do NOT expand/collapse. _(verified 2026-09-29: With co1 active: clicking main's icon toggle flipped aria-expanded false->true and active stayed co1; clicking main row made main active and aria-expanded unchanged.)_
- [x] Tab to the icon + Enter/Space toggles; focus ring shows the chevron. _(verified 2026-09-29: focus() on icon toggle (role=button tabIndex 0); trusted Enter toggled aria-expanded true->false, Space back to true; :focus-visible true with chevron opacity 1 and icon opacity 0.)_
- [ ] Every branch with terminals starts expanded after the reload; a branch collapsed via its icon stays collapsed after a restart. _(NOT VERIFIED 2026-09-29: partial — Collapsed via icon, reloaded page: aria-expanded stayed false (count shown), so collapse persists across reload. Not done: 'starts expanded after reload' from a fresh state, and instance restart.)_
- [ ] [VISUAL] Repo header: GitHub badge sits right next to the repo chevron; on hover ⋯ and + appear to its left, nothing shifts. _(NOT VERIFIED 2026-09-29: partial — Repo header hover (mouse move): repoActions (⋯ and +) opacity 0->1, repoName x=10 and chevron x=277 unchanged (no shift). No GitHub badge in fx/repo (no GitHub remote) so badge placement not verifiable; no screenshot.)_

## Theme review applied (2026-09-23) — **Rust, needs a `make dev` restart**

Bundled JSONs updated (VS Code Dark now follows VS Code Dark 2026), Deep Black / Delicate One removed, "Clean"
shown as "Ink" and "VS Code Light" as "Paper", default and fallback moved to
Commander (`config.rs` default, `DEFAULT_THEME` in `settings.ts`). Before/after
reference: `docs/design/theme-gallery-2026-09-22/`. `seed_builtin_themes` is a
no-op once `<config>/themes` exists, so an existing install sees none of the
new colors or names until the JSONs are copied into that folder.

- [x] In a **restarted** instance with an empty `<config>/themes` (or
      `TUIC_APP_INSTANCE=<id>`), Settings > Appearance lists 13 themes, with Ink,
      Paper and VS Code Dark and without Deep Black or Delicate One.
      _(verified 2026-09-23: `themes.rs:306` `BUILTIN_THEMES` holds 13 files; their
      `name`s include Ink (`clean.json`), Paper (`vscode-light.json`) and VS Code
      Dark, none is Deep Black or Delicate One; `seed_creates_dir_and_files_when_missing`
      and `builtin_themes_parse_successfully` pass. Not run in an isolated instance.)_
- [x] Same instance, `config.json` with `"theme": "does-not-exist"`: the app
      opens in Commander and logs `falling back to commander`.
      _(verified 2026-09-23: `themes.ts:313` warns `Unknown theme "<key>", falling back
      to commander` and `getAppTheme` falls back to `DEFAULT_THEME = "commander"`
      (`settings.ts:85`); `src/__tests__/themes.test.ts` "falls back to commander for
      unknown theme" and "warns when applying unknown theme" pass, 24/24.)_
- [ ] [VISUAL] On Paper: the toolbar wordmark is a clean grey with no dark _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
      smear, a colored repo name is readable, and the active tab row in the
      sidebar is visible.

## Clean theme bundled (2026-09-22) — **Rust, needs a `make dev` restart**

`clean.json` is in `BUILTIN_THEMES`, but `seed_builtin_themes` is a no-op once
`<config>/themes` exists, so no existing install receives it from the bundle.
Verified live on 2026-09-22 by copying the file into
`~/Library/Application Support/com.tuic.commander/themes/` (the watcher picked
it up): pure black chrome, white accent, neutral tab-type tints on
`html[data-theme="clean"]`, toast clamp at four lines. Font smoothing
(`antialiased`, 0.01em tracking) is global on `html`, verified live on both
Clean and VS Code Dark.

- [x] In a **restarted** instance with an empty `<config>/themes` (or
      `TUIC_APP_INSTANCE=<id>`), Settings > Appearance must list "Ink" (key `clean`) without
      any manual copy.
      _(verified 2026-09-23: `clean.json` (name "Ink") is in `BUILTIN_THEMES`
      (`themes.rs:332`); `seed_creates_dir_and_files_when_missing` passes. Code and
      test only, not an isolated instance.)_
- [x] Selecting "Ink" (then named "Clean") applies the black chrome and antialiased text at once.
      _(verified: live screenshot + `document.documentElement.dataset.theme ===
      "clean"`, computed `-webkit-font-smoothing: antialiased`, 2026-09-22)_

## DL and SU stop manufacturing scrollback (story `834-1878`, 2026-09-22) — **Rust, needs a `make dev` restart**

`delete_lines` and `scroll_up` used to push the rows they removed into history.
The fork tests and the retained ANSI captures cover the buffer; what they cannot
cover is what a real agent's repaint looks like on screen, and whether anything
a user relies on scrolled away with it.

- [x] In a **restarted** instance, run an agent that repaints with DL — Claude
      Code or any Ink TUI redrawing its box is enough. Scroll back afterwards:
      the history must hold what the agent printed, with no duplicated frames of
      the repainting box. Before the fix each repaint left its removed rows
      behind.
      _(verified 2026-09-23 with a synthetic repaint, not an Ink agent: throwaway
      `POST /sessions` on :9876, `seq 1 100`, then 20× `CSI 1;1H CSI 10 M`, 20× `CSI 5;1H
      CSI 10 M`, 20× `CSI 5 S` and 20× `CSI 2;20r CSI 5 S`: `scroll-info.total_lines`
      grew by exactly 1 per command (its own echo line) instead of up to 200.)_
- [x] Scroll back far enough to be off the live screen, then let the agent
      repaint. The viewport must stay where you put it — a control scroll no
      longer shifts a scrolled-back view, so the rows under your eyes must not
      move.
      _(verified 2026-09-23, synthetic: scrolled 50 lines back, sent 20× `CSI 10 M` at
      the top row; `row-text?row=0` read `228` before and after, `display_offset`
      moved 50→51 only for the one real linefeed of the command echo.)_
- [ ] Select text in the scrollback, let the agent repaint, then copy. The _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend half verified on rust0930: seq 1..300, selection-text rows(150..152)=='150\n151\n152' identical before/after 20x CSI M/L/S/r repaint storms (historyBase=0). Frontend mouse selection+copy not observable headless.)_
      selection must still yield the text it covered.
- [x] Run `less` or `man` on a long file and quit. Everything printed before it
      must still be in the scrollback — this is the linefeed path, which must be
      unchanged.
      _(verified 2026-09-23: throwaway session, `seq 2001 2060`, `less` on a 201-line
      file, `q`: `/terminal/lines` still held 2001 and 2060, no `less` content leaked
      into history, `total_lines` 84→85.)_

## Hands-free from a browser tab (story `832-e730`, 2026-09-22) — **Rust, needs a `make dev` restart**

A browser now has its own microphone and speaker for hands-free: the tab opens
`GET /dictation/hands-free/audio?owner=<id>` and streams capture up and replies
down on it. Rust refuses an owner with no socket rather than falling back to the
server's hardware, in either direction, and every part of that is covered by
tests — what no test can reach is a real microphone, a real speaker and a real
tab being closed.

- [x] Open the web UI of the **restarted** instance in a real browser — port _(verified 2026-09-29: web UI (browser mode) Settings has a Voice tab with a Dictation section (renamed from 'Dictation'); no global hotkey field and no microphone-device list in the section text)_
      9876 if it took it, else 9877 — and go to **Settings > Dictation**. Check
      the port first: an instance started before this commit serves the old
      frontend and has no audio route, so testing it proves nothing. The tab
      must be **present** — it used to be hidden outside Tauri. The global
      hotkey and the microphone-device list must be **absent**.
- [ ] Press Start on a terminal running an agent. The browser must ask for _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Hands-free routes (/dictation/*, incl. /dictation/hands-free/audio WS) are #[cfg(feature=desktop)]: 404 on this headless rust0930 instance (unix socket and :9881). Needs the desktop build + browser (Chrome fake-device mic permission prompt).)_
      microphone permission, and the phase must reach `waiting`.
- [ ] **[HUMAN]** Hold a complete turn: speak, see the transcript delivered to _(NOT VERIFIED 2026-09-30: blocked — audio hardware (mic/speakers, human listening); also dictation routes absent on headless build)_
      the agent, and hear the reply **through the browser's speakers** — not
      through the machine running TUICommander. Check the other machine is
      silent.
- [ ] **[HUMAN]** Barge in mid-reply. The reply must stop where you are, not _(NOT VERIFIED 2026-09-30: blocked — audio hardware (mic/speakers, human barge-in); also dictation routes absent on headless build)_
      merely stop being sent.
- [ ] **[HUMAN]** Close the tab mid-utterance. The conversation must disarm _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Socket-close disarm is testable (WS to /dictation/hands-free/audio?owner=X + arm on a real agent PTY, then GET /dictation/hands-free armed) but /dictation/* routes are desktop-gated: 404 on headless rust0930 (unix+:9881). Needs desktop build.)_
      (`GET /dictation/hands-free` reports `armed: false`), nothing may stay
      queued, and nothing may be left speaking.
- [ ] Arm from the desktop app while a browser tab holds an audio socket. The _(NOT VERIFIED 2026-09-30: blocked — audio hardware: desktop microphone; also dictation routes absent on headless build)_
      desktop must use its own microphone and ignore the browser entirely.
- [ ] Reload the browser tab while armed. The old conversation must disarm _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Reload-disarm testable via two WS connects with arm on a real agent PTY (real claude PTY now available), but /dictation/* routes are desktop-gated: 404 on headless rust0930. Needs desktop build.)_
      rather than follow the new socket.

## ANSI edit-operation contract (2026-09-22) — **Rust, needs a `make dev` restart**

Four handler fixes in the Alacritty fork: IL/DL reset the cursor column,
IL/DL/ICH/DCH/ECH resolve a pending wrap, ED0 spares the cell behind one, and
DCH blanks only the cells it removed. Replay evidence and 200 fork tests cover
the parser; these items are the part a live terminal shows.

- [x] Run a full-screen TUI that edits lines in place (`htop`, `lazygit`, `vim`
      with a long line at the right margin). No row may paint at the wrong
      column after a redraw, and no character may disappear from the last column.
      _(verified 2026-09-23 with synthetic CSI on a live :9876 throwaway session, not
      htop/vim: `CSI 3;10H CSI 1 M` + `X` gave `Xddd` and `CSI 4;10H CSI 1 L` + `Y` gave
      `Y` at column 0; at a pending wrap in the last column (220 cols), ECH/DCH/ICH then
      `X` put `X` in the last column of the same row, EL0/ED0 kept the last `F` and `X`
      wrapped to the next row — the documented contract.)_
- [x] In a Claude/Codex tab, let an agent stream a tall frame that repaints.
      Scrollback must not gain rows the agent did not print.
      _(verified 2026-09-23, synthetic, same probe as the DL/SU section: 80 DL/SU
      repaints added no history rows. Not observed with a real agent.)_
- [ ] **[VISUAL]** Select and copy text ending at the right margin after such a _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
      redraw. The copied text must keep its last character.

## Recover captured terminal context (2026-09-22) — frontend

- [x] Captured context reappears on the idle Grok tab after frontend reload.
      _(verified: live DOM after automatic HMR, 2026-09-22: the same PTY session
      shows `Intent: locking out hashtags` and its campaign prompt without
      additional input; hook regressions cover delayed session attachment.
      The existing nine-word follow-up retains the previous substantial prompt.)_

## Selection during output (2026-09-21) — **Rust, needs a `make dev` restart**

- [x] Isolated browser verification passed: held and released multi-row
      selections survive 50 output rows at the history cap, with stable base
      canvas hashes. Snapshot copy returns the original line; expired endpoints
      return HTTP 409. Clipboard writes were intercepted before app load.
      Evidence: `tests/terminal-stress/SELECTION_FINDINGS.md`.
- [ ] After loading the updated frontend, copy one block, then select a different _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Client-side selection persistence across a full frame is frontend-only (no server selection state); backend contract (historyBase snapshots) verified under 895. Needs desktop/browser build with clipboard guard.)_
      multi-row block while output continues and keep the mouse held. The new
      selection must not disappear when another full frame arrives.
- [ ] After loading the rebuilt backend and frontend, park a terminal in history _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend half verified: selection-text with historyBase: snapshot at seq 1..300 rows(150-152) stays valid under 9000 rows, then 409-style error 'selection rows are no longer retained' after passing the 10000-line cap (never substitutes next row); a live snapshot re-read after +500 rows with historyBa)_
      while output continues beyond the scrollback cap. Selection must stay on
      the same retained text, both during dragging and after release; copying
      must not substitute the next row when history advances. Automated browser
      checks require the verified clipboard guard in `tests/terminal-stress/`.
      Investigation and validation: `plans/terminal-selection-output-integrity.md`.

## Terminal Unicode integrity (2026-09-21) — **Rust, needs a `make dev` restart**

- [x] Resize/scroll visibility recovery verified in the isolated browser on
      2026-09-21: 1200×1223 → 1000×700 → scroll-to-top stayed painted, with
      scrollbar movement and valid frames, without forced hide/show. Evidence:
      `.tmp/terminal-integrity/post-fix-20260921-1819-frontend/`.
- [ ] Finish the block-cursor-on-decomposed-glyph visual check using a browser _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Block cursor on decomposed glyph is canvas rendering (no DOM). Backend: 'e'+U+0301 stays 2 code points in 1 cell (hyperlink span after decomposed text is cell-aligned [14,19]); needs desktop/browser canvas build with clipboard interception.)_
      with verified clipboard interception. The prior run was interrupted after
      terminal auto-copy overwrote the host clipboard; do not repeat real
      selection/copy/paste against the shared clipboard.
- [ ] After loading the rebuilt backend, print precomposed and decomposed accents, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend half verified: printf PRE U+00E9, e+U+0301, a+U+0300 and accent written after base in a second printf -> lines keep code points 0xe9,0x65 0x301,0x61 0x300,0x65 0x301; resize 60->14->10->60 reflow keeps all combining marks; OSC8 span after accented text cell-aligned; POST search-buffer matche)_
      including an accent written after its base letter. Verify rendering, block
      cursor, selection/copy, search and a link following accented text; scroll
      during output and resize. The installed instance still needs a restart
      even when the isolated verification build passes. See
      `plans/terminal-unicode-integrity.md` for automated evidence and limits.

## Dictation preserves speech before a pause (2026-09-21) — **Rust, needs a `make dev` restart**

- [ ] After restart, dictate a short phrase followed by a pause while holding F5. The live preview must receive the phrase; the final transcription must retain it. Verify silent recordings remain rejected by the configured speech gates. _(NOT VERIFIED 2026-09-30: blocked — audio hardware: microphone/F5 speech; dictation routes absent on headless build)_

## Hooked dialogs restore the question badge (2026-09-21) — **Rust, needs a `make dev` restart**

- [ ] After restart, a Claude selection dialog with native hooks enabled must _(NOT VERIFIED 2026-09-30: partial — Real claude (sonnet) AskUserQuestion 2-question dialog, hooks emulated: agent_state awaiting_input/awaiting_input=true on dialog open; Tab Q1->Q2, resize x2 kept it; Enter-answer Q1 kept it; ONE 'awaiting_input set' log. On the Submit/Review tab (dialog still open) awaiting dropped (completed/idle o)_
      show the question badge. It must survive switching sub-questions and
      redraws without duplicate notifications. After the final answer or cancel,
      normal protocol activity must clear question when the dialog closes.

## Protocol idle survives terminal animation (2026-09-21) — **Rust, needs a `make dev` restart**

- [ ] After a rebuild/restart, finish a Codex turn in brainstorming with its idle _(NOT VERIFIED 2026-09-30: partial — Real codex 0.159: after a turn idle held 40s and 2nd prompt -> busy ~1s after Enter (hook-idle Protocol). Codex pet animation (pet=codex in private CODEX_HOME) did not render/emit output in the TUIC PTY (last_activity static), so real animation NOT exercised. Emulated: script sends OSC 7770 idle the)_
      animation enabled. The terminal and session API must remain idle while the
      animation runs. Submit another prompt: working must return immediately.
- [x] Finish a Claude turn: idle must survive ordinary redraws. A real active _(verified 2026-09-30: Real claude (sonnet) in rust0930 with launch hooks emulated + project Stop hook that blocks once (exit 2, run sleep 12): agent_state stayed working 0.7s..22.2s across the block (Stop hook fired 08:03:24, again 08:03:41), then idle. After idle: 4 resizes, ctrl+l x2, typing/backspace over 30s: sample )_
      phase from a blocking Stop hook must still report working.

## Ego reaches this instance and the collapsed tool surface (#802-4c43, 2026-09-21) — **Rust, needs a `make dev` restart**

The ego MCP entry carried `TUIC_APP_INSTANCE`, which `tuic-bridge` never reads,
so with a named instance running beside the default one ego drove the DEFAULT
instance. It now carries `TUIC_SOCKET` set to the socket this process bound.
Separately, `tools/list` preferred the session flag over the per-request `_meta`
identity, so ego was handed the full catalogue after `server/discover` had
advertised the collapsed one.

- [ ] Start a named test instance (`TUIC_APP_INSTANCE=tuic-test make dev`) while _(NOT VERIFIED 2026-09-30: partial — AI Chat session/new fails on this host: ~/.ego/config.toml [mcp.servers.tuic] stdio bridge path missing (ENOENT) -> 'supplied MCP servers could not be admitted'; /acp/one-shot (no MCP) works. Not exercised: needs AI Chat session list + named instance)_
      Boss's install is running. Open the AI Chat panel there and ask ego to list
      the sessions: it must report the test instance's sessions, not Boss's.
- [ ] In that same conversation ask ego which tools it has. It must name _(NOT VERIFIED 2026-09-30: partial — AI Chat session/new fails on this host: ~/.ego/config.toml [mcp.servers.tuic] stdio bridge path missing (ENOENT) -> 'supplied MCP servers could not be admitted'; /acp/one-shot (no MCP) works. Not exercised: ego tool listing needs a session with the tuicommander MCP)_
      `search_tools`, `get_tool_schema`, `call_tool` and `progress` — four, not
      the full catalogue.
- [ ] Kill nothing and start a second copy so the primary socket is already held: _(NOT VERIFIED 2026-09-30: partial — AI Chat session/new fails on this host: ~/.ego/config.toml [mcp.servers.tuic] stdio bridge path missing (ENOENT) -> 'supplied MCP servers could not be admitted'; /acp/one-shot (no MCP) works. Not exercised: needs second instance socket + ego session)_
      ego in the second copy still reaches the second copy (it binds a `-<pid>`
      socket, and the entry carries that path).

## Mirrored remote events stay remote (#801-d34e, 2026-09-21) — **Rust, needs a `make dev` restart**

A mirrored event was indistinguishable from a local one, so a remote daemon's
`session-created`, `ui-tab`, `repo-changed` and worktree events ran the LOCAL
handlers. The window emit is now limited to `session-state-changed` and
`session-closed`, every mirrored payload carries `__tuic_origin`, and a frame
that already has one is dropped.

- [ ] Connect mac-mint, start a PTY **on mac-mint** (ssh in, `tuic session` there, _(NOT VERIFIED 2026-09-30: blocked — BLOCKED class: second physical machine (mac-mint/SSH daemon) required; no such host reachable from this instance)_
      or its own UI). This Mac must show it as a session-list row with the remote
      badge and **no new tab** — no `PTY: Session N`.
- [ ] Its busy/idle/question badge still moves from here while it works. That is _(NOT VERIFIED 2026-09-30: blocked — BLOCKED class: second physical machine (mac-mint/SSH daemon) required; no such host reachable from this instance)_
      the one thing the window emit is still allowed to carry.
- [ ] Open a repo folder on mac-mint from its own UI: no repository appears in _(NOT VERIFIED 2026-09-30: blocked — BLOCKED class: second physical machine (mac-mint/SSH daemon) required; no such host reachable from this instance)_
      this Mac's sidebar, and no git work runs here for that path.
- [x] Add a **Direct** connection whose URL is this machine's own daemon
      (`http://127.0.0.1:9876`). Connect must fail with "this very TUICommander
      instance — a machine cannot mirror itself", and the row must read Error.
      _(verified 2026-09-23 by code + test, no connection added to the live config:
      `remote_runtime.rs:769-773` refuses when `/health.instance_id` equals
      `instance_identity()`; `connecting_to_this_very_process_is_refused_by_identity`
      passes and asserts the message, `RemoteStatus::Error`, no token, no further probe.)_
- [x] `curl -s localhost:9876/health | jq .instance_id` returns a UUID, and it
      changes after a restart.
      _(verified 2026-09-23: :9876 returned `a53f203e-7e81-4d47-a0a8-6c3ec9a4d8ca`;
      `crates/tuic-core/src/app_instance.rs:152-155` mints it with `Uuid::new_v4()` in a process-local
      `OnceLock`, never persisted, so each process gets a new one.)_

## Workspace badge: file count instead of "Dirty", `in_sync` instead of "Merged" (2026-09-20) — **Rust, needs a `make dev` restart**

The sidebar called a worktree "Merged" while it held 24 uncommitted files. The
backend verdict now separates `in_sync` (HEAD is the default branch's tip, so
nothing was ever merged) from `merged`, and reports `dirty_files` — the count a
removal discards — instead of a `dirty` flag. **Until the restart the frontend
reads `dirty_files` off an old backend that does not send it, so every count is
0 and the old `merged` verdict still shows.**

- [x] After the restart, the `feat/sqlite-viewer-plugin` row must read `24 dirty` (or whatever `git status --porcelain -uall | wc -l` says in that worktree), not `Merged`. _(verified 2026-09-23 on substitutes — that worktree no longer exists: `GET /worktrees/lifecycle` on :9876 for all 7 linked worktrees of `LS/agent2` and `engineering-blog` returned `dirty_files` equal to `git status --porcelain -uall | wc -l` (7, 0, 0, 0, 2, 3 …); `RepoSection.tsx:590-597` puts the count before `Merged`. The chip shows only when no stats/PR chip is on the row, else the count is in that chip's tooltip.)_
- [x] No `main` row anywhere in the sidebar carries a lifecycle badge, however dirty. Main is not removable from that list, so the badge has nothing to warn about. _(verified 2026-09-23 by code: the badge is gated on `!props.branch.isMain` (`RepoSection.tsx:579`); `LS/agent2` main has 2388 dirty files and would otherwise show one.)_
- [x] A worktree with commits of its own, all merged into the default branch, and a clean tree still reads `Merged`. _(verified 2026-09-23 by test + code, no such worktree exists live: `a_workspace_behind_the_default_tip_is_merged`, `a_workspace_on_the_default_tip_is_in_sync_not_merged` and `a_workspace_with_its_own_commit_is_unmerged` pass; `RepoSection.tsx:597` renders `Merged` for `merged` only.)_
- [x] Removing a worktree with uncommitted files: the confirm dialog must name the count ("N uncommitted files will be discarded"), not the word dirty. _(verified 2026-09-23 by code: `useConfirmDialog.ts:110-111` builds `${lost} uncommitted file(s) will be discarded` from `dirtyFiles`.)_

## Notification sound teardown (2026-09-20) — **Rust, needs a `make dev` restart**

- [ ] In Settings → Notifications, play each Test sound through the output device that previously crackled. The tone must end cleanly, with no relay-like click after its release. The source now reaches an exact zero sample and feeds 100 ms of silence before closing, but only the real CoreAudio device can verify the hardware-buffer teardown. _(NOT VERIFIED 2026-09-30: blocked — BLOCKED class: audio hardware (real CoreAudio output device and ears).)_

## Progress dialog and journal (2026-09-19)

- [ ] **Rust, needs a `make dev` restart.** In an isolated agent session, print an `intent:` that soft-wraps over at least 12 rows in a 40-column terminal. Confirm the complete title appears once and Progress records a capped entry. Scroll a title-less intent under capped history and confirm it stays open until the next prose line. Composer chrome must not extend the intent, and an unfinished `(` title fragment must be removed when the intent closes. _(NOT VERIFIED 2026-09-29: partial — Fake agent, 40-col grid via POST /resize: 560-char soft-wrapped intent (14 rows) -> title 'Soft Wrapped Title' set once, ONE journal entry capped at 500 chars ending '…'. Unfinished '(' fragment removed on close. Title-less intent closes at next prose line. NOT tested: capped-history scroll, composer chrome not extending intent.)_
- [x] **Rust, needs a `make dev` restart.** In an isolated dev instance, print a title-less `intent:` followed by indented prose in a wide agent terminal. Progress must keep only the intent text; the prose must not become a tab title. In a narrow terminal, print a title after three hard-wrap rows, then another intent: both entries and both titles must appear. A long intent must keep its full tab-title event while its journal text ends at 500 characters. _(verified 2026-09-29: Wide 200col: titleless intent + indented prose -> journal only intent text, display_name unchanged. 40col: title after 4 hard-wrap rows then 2nd intent: both entries + titles (Narrow Zed, Second Zed). 750-char intent: journal 500 chars with ellipsis, full Long Title event.)_

- [ ] **Rust, needs a `make dev` restart.** In an isolated dev instance, have Codex stream a long `intent:` in a narrow terminal while it redraws and moves the cursor to its composer. Progress should receive one full entry with its title; a later identical repaint must add none. Restart when current PTY sessions may be lost. _(NOT VERIFIED 2026-09-29: Needs real Codex streaming a long intent in a narrow terminal.)_

- [ ] Open Progress (palette: "Open Project Progress"), move the pointer across three rows, then off the list. Every delete icon must be hidden again; before, WKWebView kept the icon of every row crossed. CSS only, live via HMR — no restart. Item created because the fix could not be reproduced programmatically. _(NOT VERIFIED 2026-09-29: WKWebView-specific hover-stuck icon; author states it could not be reproduced programmatically; needs desktop app and human.)_
- [ ] **Rust, needs a `make dev` restart.** With an agent tab open, let it print `intent: …` and let the screen repaint (spinner running). `sqlite3 "<config dir>/progress.sqlite3" "select count(*) from entries where kind='intent' and created_at_ms > <restart ms>"` must grow by one per distinct intent, not per repaint. Then call the `progress` tool twice with the same `done` text and confirm both rows land. _(NOTE 2026-09-23: FAILS on :9876 — no row at all has landed since the restart (newest `created_at_ms` is 12:04, restart 13:08) because every `ProgressStore::open()` fails with `no such table: sqlite_sequence`; see Progress Flow view, first item.)_ _(RE-CHECK 2026-09-30, make dev from main 8c407689b: store now works — `repo action=progress_list` returns rows landing after restart; two identical `done` calls landed as separate rows 6422 and 6423. NOT VERIFIED: one-intent-per-distinct-intent under spinner repaint; recent intent rows from other agents (6413, 6414, 6421) appear once each, no repaint duplicates seen.)_

## ego reaches TUIC over the stdio bridge (story `796-7fa3`, 2026-09-20) — **Rust, needs a `make dev` restart**

The session's MCP entry moved from an HTTP URL to the `tuic-bridge` sidecar on
stdio. Every test here stops at the wire shape TUICommander writes; what none of
them reaches is ego actually spawning that binary and calling a tool through it.

- [ ] With **Remote Access off** (Settings → Remote Access), open the AI Chat panel and ask ego to list the open terminal sessions. It must answer with them. Before this change the same question got a refusal or an empty answer, because the session carried no MCP server at all — that is the whole bug. _(NOT VERIFIED 2026-09-30: partial — AI Chat session/new fails on this host: ~/.ego/config.toml [mcp.servers.tuic] stdio bridge path missing (ENOENT) -> 'supplied MCP servers could not be admitted'; /acp/one-shot (no MCP) works. Not exercised: needs ego session with bridge)_
- [ ] Turn Remote Access **on**, start a new chat session, ask again. Same answer. The switch must no longer change what ego can reach. _(NOT VERIFIED 2026-09-30: partial — AI Chat session/new fails on this host: ~/.ego/config.toml [mcp.servers.tuic] stdio bridge path missing (ENOENT) -> 'supplied MCP servers could not be admitted'; /acp/one-shot (no MCP) works. Not exercised: needs ego session + Remote Access toggle)_
- [x] Check the tool surface is still the collapsed one. _(verified: covered on both sides instead — `tuic-bridge` `the_downstream_client_name_is_forwarded_and_not_replaced_by_the_bridges_own` asserts the proxied `initialize` still carries `clientInfo.name = ego` and that the bridge's own session opens under its own name, and `mcp_transport::tests::the_collapsed_surface_is_decided_by_the_name_the_bridge_forwarded` asserts `tuic-bridge` does NOT earn the collapsed surface by itself. Falsified by mutation: renaming the forwarded client turns the bridge test red.)_
- [ ] Launch a second instance with `TUIC_APP_INSTANCE=qa`, open AI Chat there, and ask ego which repositories it can see. It must see the `qa` instance's repositories, never the default instance's. _(NOT VERIFIED 2026-09-30: partial — AI Chat session/new fails on this host: ~/.ego/config.toml [mcp.servers.tuic] stdio bridge path missing (ENOENT) -> 'supplied MCP servers could not be admitted'; /acp/one-shot (no MCP) works. Not exercised: needs second instance + ego session)_

## PR review, changelog and improvement scan run on ego (story `795-320b`, 2026-09-20) — **Rust, needs a `make dev` restart**

`pr_review.rs`, `changelog.rs`, `improvement_scan.rs`, two new `AppEvent`
variants and four new HTTP routes are all new Rust, and none of them load into a
running `make dev`. The parsers, the confidence gate and the stores are unit
tested; what no test reaches is a real ego process answering a real diff.

- [ ] Open a PR's detail popover and click **Run** under AI Review. It must produce findings (or "No findings" with a reviewed-file count and "by ego"), never a blank panel. Tick a finding with a line number and click **Post review**; the comment must appear on the PR in GitHub. _(NOT VERIFIED 2026-09-30: partial — POST /repo/pr-review {repoPath,prNumber:88} on real PR via dev ego: summary + files[{findings:[]}] returned (No findings, 1 file). Post-review/UI/line ticking not exercised.)_
- [ ] A finding with **no** line number must be listed but its checkbox disabled — GitHub refuses an inline comment without a line. _(NOT VERIFIED 2026-09-30: partial — Review returned files[].findings schema for PR 88 but zero findings, so no line-less finding to check disabled checkbox (UI anyway).)_
- [ ] Rename `ego_executable` in Settings → General to something that does not exist and run the review again. The popover must show ego's own sentence, not an empty finding list. Put the real path back. _(NOT VERIFIED 2026-09-30: partial — ego_executable=/nonexistent -> /repo/pr-review and /repo/improvement-scan return {error:'ego could not run this turn: invalid ego executable: No such file or directory'}; popover render not seen. Restored afterwards.)_
- [ ] GitHub panel header → the document icon opens the Changelog modal. It must produce markdown for the merged PRs since the last tag, and **Copy** and **Save** must both work on the result. _(NOT VERIFIED 2026-09-30: partial — GET /repo/changelog?path=fx repo returns markdown (## Features/## Fixes with PR refs). Copy/Save buttons are UI.)_
- [ ] Open the **Ops Dashboard** (the chart icon in the same header). Click each of `refactor`, `testing` and `perf`. Each scan must fill the Proposals column with at most five cards, and **Create issue** on one of them must file a real GitHub issue. _(NOT VERIFIED 2026-09-30: partial — POST /repo/improvement-scan focus refactor/testing/perf each 200 with proposals:[] on tiny fixture (<=5 not testable); cards/Create issue is UI.)_
- [ ] While a review is running, the dashboard's Review findings column must show the PR as Working and then Done with a count — that is the `review-progress` event arriving over the bus. _(NOT VERIFIED 2026-09-30: partial — PR review ran to completion on real PR; review-progress event stream not captured; dashboard is UI.)_
- [ ] From a browser (not the Tauri app) at `localhost:9876`, run the same review and the same scan. Both must work: these routes are deliberately not desktop-gated. _(NOT VERIFIED 2026-09-30: partial — Unix-socket (local) router serves /repo/pr-review, /repo/changelog, /repo/improvement-scan OK. Remote router :9881 (Basic auth) returns 404 for all three (not in build_remote_router).)_

## The terminal stream is compressed over a remote connection (story `794-832e`, 2026-09-20) — **Rust, needs a `make dev` restart**

`ws_stream`, `mcp_http::ws_compression` and the ssh `Compression=yes` are new
Rust. The codec is unit-tested on both sides and the framing end to end against a
mock socket; what no test reaches is a real browser inflating a real socket, and
a real ssh process.

- [x] From a **second machine** (or a phone on the LAN), open the web UI against this daemon's IP and attach a terminal. In devtools → Network → the `stream` socket, the URL must carry `compress=deflate`, every message must be **Binary**, and the terminal must paint and stay live. Run something noisy (`yes | head -100000`) and watch the socket's byte counter — it should be a small fraction of what the same run costs locally.
      _(verified 2026-09-20, server side, against a headless `tuic-remote` on mac-mint
      reached from this Mac — a genuine non-loopback peer. A raw RFC6455 client
      (`scripts/ws-stream-probe.mjs`) asked for `?compress=deflate` and read the opcodes: every
      payload arrived as a **Binary** WS frame carrying the tag byte, 2 of 9 tagged
      `TextDeflate`, and **1849 bytes on the wire inflated to 4832** with 0 decode
      errors. The same workload with no `compress=` cost 4859 bytes for 4859 bytes of
      content, so the saving is 62% and the untagged path is untouched. **Still
      uncovered: a real browser running the app's own decoder** — this proves the
      server's framing, not the frontend's inflate.)_
- [ ] Same devtools panel, from a browser on **this** machine at `localhost:9876`: the URL must have **no** `compress=` at all and the messages must still be a mix of Binary and Text. This is the local path, and it must be untouched. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Client-side choice of compress= param is frontend (canvasTerminalTransport); server side: socket without compress= gets untagged frames. Needs a browser at the local URL.)_
      _(NOTE 2026-09-20: the server half is proven — a socket that sends no `compress=`
      gets genuine untagged Text frames, byte for byte the old framing. What is still
      open here is the **frontend's** choice not to ask on a local connection, which no
      HTTP probe can see.)_
- [ ] Resize the remote terminal, scroll it, and let the agent repaint a full screen. No stale rows, no torn frames — a mis-ordered inflate would show as rows from an older screen surviving under a newer one. _(NOT VERIFIED 2026-09-30: blocked — BLOCKED class: second physical machine. Daemon binds loopback only (LAN IP :9881 times out), so every peer is loopback and gets identity tags; no compressed frames obtainable here; real browser inflate also needed.)_
- [x] `curl` the daemon's log after a remote session: no `WsTransport could not decode a compressed frame` lines. One would mean the tags disagree.
      _(verified 2026-09-20: zero occurrences in the daemon log after the remote,
      loopback and untagged runs above, and the client decoded every deflated frame it
      received — `decode_errors: 0` on all three.)_
- [ ] Settings → Services → SSH Tunnels → edit a profile: **Compress the channel (ssh -C)** is on. Save, start the tunnel, and confirm with `ps ax | grep "[s]sh -N"` that the command line holds `-o Compression=yes`. Untick it, restart the tunnel, confirm `Compression=no`. _(NOT VERIFIED 2026-09-30: partial — Via /tunnels API against a fake sshd listener: profile compression:true -> ps shows 'ssh -N ... -o Compression=yes'; false -> Compression=no. Settings checkbox UI not driven.)_
- [ ] Open a repository through an **SSH remote connection** and attach a terminal. The stream socket asks for `compress=deflate` (the client sees a remote connection) but the daemon answers identity tags because the peer is loopback — the terminal must still work, and the saving comes from the tunnel instead. _(NOT VERIFIED 2026-09-30: partial — WS /sessions/{id}/stream?compress=deflate over loopback TCP with subprotocol tuic.deflate: 101 selects tuic.deflate, 44 frames all tag 0x02 (identity), stream live. SSH remote connection through the app not done.)_
      _(NOTE 2026-09-20: the **daemon half is proven** — the same probe run on mac-mint
      against `127.0.0.1:9879` with `?compress=deflate` got 10 tagged frames and **not
      one** `TextDeflate`, where a remote peer on the identical workload got 2. The
      wire cost was 4905 bytes for 4895 bytes of content: exactly the 10 tag bytes and
      nothing else. What is left is the tunnel wiring that puts a real client on the
      loopback side of it.)_

## Smart Prompts `api` mode runs on ego (story `787-ee50`, 2026-09-20) — **Rust, needs a `make dev` restart**

`acp_one_shot_prompt`, `POST /acp/one-shot` and `acp/oneshot.rs` are new Rust, so
none of this is live in a running `make dev`. The collector is unit-tested
against event sequences and the frontend against a double; what no test reaches
is a real ego process, which is every item below.

- [x] With **ego executable** empty, a prompt saved with `executionMode: "api"` is listed but disabled, and hovering it says ego is not configured and names *General* then *AI Providers*. _(verified 2026-09-29: Created prompt via Settings>Smart Prompts, Execution Mode=api, toolbar placement; ego empty: Smart Prompts dropdown item has class itemDisabled, opacity 0.5, title 'ego is not configured - name the binary in Settings > General, then pick a model in Settings > AI Chat' (says AI Chat, not AI Providers).)_
- [ ] With ego configured but no repository or terminal open, the same prompt is disabled and says ego needs a working directory. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: disabled-with-reason state is frontend; not exercisable headless.)_
- [ ] With a real ego and a repository open, run an `api` prompt whose output target is the clipboard. The clipboard must hold ego's final text, trimmed, with no reasoning in it. _(NOT VERIFIED 2026-09-30: partial — POST /acp/one-shot in fixture repo returns {text:'ok',stopReason:end_turn,declined:0}, plain final text; clipboard target is UI.)_
- [x] While it runs: `ps ax | grep ego` shows exactly one extra process, and it is gone within a second of the answer arriving. Run the prompt three times — no ego process accumulates. _(verified 2026-09-30: ps during /acp/one-shot: exactly one extra 'ego acp -C repo' process, gone <=1s after answer; 3 sequential runs left no ego process accumulating.)_
- [ ] The AI Chat panel's own connection is untouched: open the panel, send a turn, then run an `api` prompt. The panel's conversation must survive, and the prompt must not appear in it. _(NOT VERIFIED 2026-09-30: partial — one-shot uses its own launched ego (separate pid from the panel connection, which stayed alive); panel conversation itself not driven (session/new blocked).)_
- [ ] Ask an `api` prompt to do something that needs a tool ("list the files in this directory"). It must come back refused, saying how many permissions were declined — not as an empty answer, and never hanging. _(NOT VERIFIED 2026-09-30: partial — likely fixture (Boss ego config yolo/sandbox off); one-shot 'Run shell command touch r2-made.txt' -> executed (file created), declined:0, no refusal. Cause: ~/.ego/config.toml mode=yolo sandbox=off; unattended one-shot does not force no-tools/refuse. File removed.)_
- [ ] Run one with the output target set to *commit message*: the Git panel's commit box must fill. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: commit box fill is frontend; not exercisable headless.)_

## AI Providers tab over ego's configuration (story `786-4a6d`, 2026-09-20) — **Rust, needs a `make dev` restart**

The backend (`ego_cli.rs`, `/ego/*`) is new Rust, so none of this is live in a
running `make dev`. Every check below also needs a real ego binary: the tests
prove the join and the failure shapes against a double, never against ego.

- [x] With **Experimental Features** off, the Settings nav has no *AI Providers* entry and searching for "default model" finds nothing. Turn it on: the tab appears. _(verified 2026-09-29: Experimental off: nav lacks AI tab, search 'default model' -> 'No settings match your search.'. Enabled via General checkbox: nav gains 'AI Chat' (item says 'AI Providers': renamed) and search 'default model' -> 'AI Chat > Default Model'. Web UI :9880.)_
- [ ] With **ego executable** empty, open the tab. It must name the field to fill (General → AI Chat), not render an empty list, and launch nothing. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend half verified on rust0930 with ego_executable empty: GET /ego/providers and POST /ego/providers/model -> HTTP 409 {code:notConfigured,message:'no ego executable is configured; set it in Settings ...'}, no ego process spawned. The tab wording (names General -> AI Chat, no raw HttpRpcError) is)_
- [ ] Point the setting at a path that does not exist. The tab must say the executable could not be *started* — a different message from "not configured" — and show what the OS reported. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend half verified: PUT /config ego_executable=/nonexistent/ego-r2 -> GET /ego/providers HTTP 424 {code:launchFailed,message:'could not run the configured ego executable: No such file or directory (os error 2)',command:'ego-r2 config ls --json'} - distinct from notConfigured. Tab text is UI. (con)_
- [ ] With a real ego: the provider rows must match `ego doctor --json`, the model list `ego models --json`, and the picker's selection `ego config ls --json`'s `model`. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend half verified: GET /ego/providers vs real ego CLI: credential rows == 'ego doctor --json' (only openai-codex stored/ok, others missing); 11 model slugs and availability == 'ego models --json'; defaultModel == 'ego config ls --json' model (openai-codex/gpt-6-sol). Tab rendering not observed.)_
- [ ] Change the default model, then `ego config ls --json` in a terminal: the new value must be there, quoted. Restart TUICommander — the tab must still show it. (This is the criterion no test can reach: ego holds it, TUIC does not.) _(NOT VERIFIED 2026-09-30: partial — Real ego via instance API: POST /ego/providers/model ollama/gemma4:e4b-mlx -> `ego config get model` prints "ollama/gemma4:e4b-mlx" (quoted), config ls shows it, GET /ego/providers defaultModel reflects it (TUIC re-runs ego each call, no cache); restored, ~/.ego/config.toml byte-identical to backup.)_
- [ ] Press **Refresh from providers** and watch the network (or ego's own logs): the refresh must be the only outbound call, and opening the tab must make none. _(NOT VERIFIED 2026-09-30: partial — Watched processes during GET /ego/providers: default open runs no --refresh; ?refresh=true runs `ego models --refresh --json` (seen in ps). Outbound sockets not caught (0.15s run, lsof sampling empty) so 'only outbound call' not measured; UI button not observed.)_
- [ ] Break one provider (revoke a token, or point ego at an unreachable base URL) and press Refresh. The failure must be shown in ego's own words with the command and exit code, not swallowed into an empty list. _(NOT VERIFIED 2026-09-30: partial — Used shim ego (fails only `models --refresh` with stderr + exit 7, else real ego): GET /ego/providers?refresh=true -> {code:commandFailed,command:'ego-fail.sh models --refresh --json',stderr:<ego words>,exitCode:7}, not an empty list; open still works. Real broken provider and tab text not exercised)_
- [ ] Log out of one provider (`ego auth logout <provider>`) and confirm its row reads "no credential" while the others stay "stored" — a missing credential must not blank out the whole tab. _(NOT VERIFIED 2026-09-30: partial — API shows credential rows independent: openai-codex stored, anthropic/gemini/kimi-coding/ollama/openai 'missing' and model lists intact for all (no blanking). Actual `ego auth logout` NOT run: openai-codex is Boss's only stored credential (would delete it). Tab wording ('no credential') is UI.)_

## AI Chat runs on ego (story `785-58ca`, 2026-09-20) — frontend, Vite HMR picks it up, but it needs a real ego binary

Everything below is proven against a test double at the IPC boundary. What no
test can reach is the live process: these are the checks that need one.

- [x] With **ego executable** empty, open the panel: it must say ACP is not configured, show no input box, and launch nothing. _(verified 2026-09-29: ego empty, status-bar AI Chat button: panel text 'ACP is not configured. Set the ego executable in Settings to start a conversation.'; 0 visible textarea/input in panel. No spawn attributable (only ego pid seen was Boss's cwd ~/Gits, not ours).)_
- [ ] Set the path, open the panel on a repository, send a turn. The answer must stream in, reasoning must fold into a *Thinking* disclosure, and tool calls must stay one card each as their status changes. _(NOT VERIFIED 2026-09-29: Needs real ego streaming a turn with reasoning and tool calls.)_
- [ ] Let ego ask for permission. The buttons must be the ones ego published, and answering must clear the card in every open window — not only the one that answered. _(NOT VERIFIED 2026-09-29: needs a real ego agent (ACP) session; not available headless)_
- [ ] Switch repository and back. Only one ego process per root (`ps ax | grep ego`), and the first conversation must still be there. _(NOT VERIFIED 2026-09-29: Needs real ego processes (ps ax | grep ego) per repo root.)_
- [ ] A live prompt: check whether ego echoes the user message back as `user_message_chunk`. _(NOTE: the panel now renders server `promptSent` rather than a local optimistic message; if ego also sends a live user chunk, the two sources could duplicate it.)_

## `is_remote` now means "created by an agent", not "created via HTTP/MCP" (2026-09-30)

Every `POST /sessions`/`/sessions/worktree` call from our own HTTP client (the browser UI's
`usePty.ts`) and mobile's `NewSessionSheet.tsx` agent launch (`POST /sessions/agent`) now send
`user_initiated: true`, which registers
`is_remote: false` — the goal being that a terminal a human starts from the browser and later
continues in the desktop app no longer shows the `PTY · ` badge/remote styling, and survives a
browser close exactly like an app-created tab (closing the browser must never close it — verify
this specifically). A raw `curl` caller or an MCP/tmux-shim spawn needs no changes and stays
`is_remote: true`.

- [ ] Create a terminal from the browser UI ("+" button). Confirm in the desktop app: no `PTY`
  badge, `remoteTab` styling absent, tab does NOT auto-close when its shell exits (behaves like a
  normal app-created tab).
- [ ] With that browser-created terminal still open, close the browser tab/window. Confirm the
  terminal is still visible and usable in the desktop app afterward.
- [ ] Create a session via a raw HTTP call with no `user_initiated` field (e.g. `curl -X POST
  .../sessions -d '{}'`). Confirm it IS badged `PTY · ` (once it also has an `agent_type`) and gets
  the auto-close countdown on exit — unchanged from today's behavior.
- [ ] Have an agent (MCP `agent action=spawn`, or a swarm/tmux-shim pane) spawn a session. Confirm
  unchanged remote/auto-close/notification-muting behavior end to end.

## `debug action=explain_state` now surfaces `agent.agent_session_id` (2026-09-30) — **Rust, needs a `make dev` restart**

Added while investigating why a "why is this session busy" report took a pid-registry detour
to find the right hook transcript: `AgentExplain` (`pty/explain.rs`) gained an
`agent_session_id: Option<String>` field, mirrored straight from
`SessionState.agent_session_id` (already tracked for hook-instrumented Claude sessions, per
the exit-time resume banner feature — just never surfaced in this diagnostic before). A
pasted `explain_state`/`GET /sessions/{id}/explain-state` snapshot can now be joined directly
against `hook-debug.log` by this id, with no need to resolve `child_pid` →
`~/.claude/sessions/<pid>.json` first. Unit-tested
(`explain_session_state_surfaces_the_hook_reported_agent_session_id`); needs a rebuild to
confirm the live MCP `debug action=explain_state` / HTTP response actually includes the new
field for a real hook-instrumented session (`[HUMAN]` not attempted — the orchestrator
instance this would be checked against is a separately-launched release build per this
repo's own "Test instance vs orchestrator instance" rule, so it can't observe this source
change until a real rebuilt binary replaces it).

## Three desktop PTY-creation commands sized their VT screen at a hardcoded 24x220 instead of the real pane (2026-09-30) — **Rust, needs a `make dev` restart**

Found while investigating a user report ("pane size and not showing the bottom of the
screen" — previously fixed in HTTP client mode, starting to recur in the desktop app).
`register_pty_session` (`mcp_http/session.rs`) already builds the VT screen at the PTY's real
geometry for the shared spawn paths (`spawn_pty_session`, MCP `agent spawn`, `POST /agents`),
but the three desktop commands that build their own `VtLogBuffer` —
`pty::commands::create_pty`/`create_pty_with_worktree` (behind **every** plain new terminal
tab, worktree tab included) and `agent::spawn_agent` (desktop "New Agent" launch) — opened the
real PTY at the caller's `rows`/`cols` but built the screen model (what agent-state detection,
choice-prompt parsing and the chrome cutoff read) at a hardcoded `24x220`. Any pane taller than
24 rows had its bottom rows (input box, dialog footer, Enter-to-select line) invisible to
detection.

Fixed by building the VT screen at exactly the PTY's `rows`/`cols` at all three sites — the
same contract as `register_pty_session`, which on main has no 220-column floor any more (#1413-7dcc
removed it; wip's version of this fix still floored the width through a `vt_screen_size_for`
helper, which was not carried over). Tests in `mcp_http/session.rs`:
`registration_sizes_the_vt_screen_to_the_real_pty_height` (40x300 PTY => 40-row, 300-column
screen) beside main's `headless_registration_and_same_size_resize_preserve_requested_width`,
and `desktop_spawn_commands_size_the_vt_screen_to_the_real_pty` (a source guard: the three
desktop commands are `#[tauri::command]`s with no `tauri::test` scaffolding, so no hardcoded
`new_vt_log_buffer(24, 220` may remain in `pty/commands.rs` or `agent.rs`).

- [ ] [HUMAN] After restarting `make dev`, open a **plain new terminal tab** (not an agent
  launch) in a tall pane (more than 24 rows — e.g. a maximized window), run something that
  fills the bottom rows (e.g. `htop`, or any TUI with a footer), and confirm the full screen —
  including the bottom rows — is captured correctly via `ai_terminal_read_screen`/`debug
  explain_state` or an HTTP screen read for that session. This is the most commonly-hit of the
  three fixed sites.
- [ ] [HUMAN] Repeat the same check for a **worktree tab** created via "Create Worktree" (exercises
  `create_pty_with_worktree` specifically).
- [ ] [HUMAN] Spawn a new agent from the desktop app in a tall pane with a task that triggers a
  choice prompt (`AskUserQuestion`) or a plan-approval dialog. Confirm the awaiting/badge
  detection fires correctly and the dialog's footer/options are recognized (`agent::spawn_agent`).
- [ ] [HUMAN] Open a narrow pane (fewer than 220 cols) and a very wide one (more than 220 cols)
  with any of the three commands and confirm no truncation or misdetection at the right edge —
  the screen now starts at the PTY's real width with no 220-column floor, and grows/shrinks with
  each resize.

## `agent action=spawn` defers a Claude prompt until MCP identity binds (2026-09-29) — **Rust, needs a `make dev` restart**

**REGRESSION FOUND live-testing this against the orchestrator instance (v1.7.7-nightly.20260930.b32b2990f) on 2026-09-30 — the prompt is never delivered at all, not just raced.** 4/4 deferred-path spawns (`agent_type: "claude"`, no `print_mode`) reproduced the same hang: `spawn` returned immediately with `prompt_delivery: "queued — withheld..."` as documented, and `wait_for_mcp_identity_bound` bound in ~400-1200ms (well inside the 5s fail-open window, confirmed via `debug logs` `mcp_initialize` entries matching the session's `tuic_session`) — but the composer stayed permanently empty (just the rotating placeholder hint) for 3-6+ minutes, `session status` showed `shell_state: "busy"`/`agent_state: "working"` the whole time with `busy_duration_ms` climbing unbounded, and `debug explain_state` showed `queued_commands: 1` that never drained, with the busy evidence pinned at `rank: "protocol", source: "hook-busy"` from the instant of spawn and never cleared. A 5th spawn using explicit `args: ["--verbose", "{prompt}"]` (the bypass path, item 16 below) worked perfectly — delivered via argv, replied and completed in ~5s — confirming the bypass path is fine and the bug is isolated to the deferred-delivery mechanism itself.

Root cause (from reading `mcp_transport.rs`'s `spawn_deferred_prompt_delivery` and `pty.rs`'s `deliver_notice_to_pty`/`deliver_notice_to_managed_pty`): delivery doesn't type the prompt directly — it files the prompt as an `AgentMessage` in the recipient's inbox, then delivers `PEER_MAIL_WAKE` via the same queued-injection mechanism `agent action=send` uses for any managed peer, which only flushes into the composer on that session's own BUSY→IDLE transition (`flush_pending_injections_blocking`/`should_inject_now`). A freshly spawned `claude` process launched with **no prompt in argv at all** (the new case this feature introduces — every prior spawn path always had a real task immediately) reaches its real interactive welcome screen and is genuinely idle for input, but `tuic-hook`'s own busy/idle signal for that session appears to have gone busy at spawn (`hook-busy`, protocol rank) and never sent whatever event would clear it — plausibly because Claude Code's hook lifecycle for a "launched with nothing to do" session never fires the turn-boundary event the busy→idle clear depends on. Since the injection queue only flushes on that transition, the wake notice (and therefore the real prompt) never reaches the terminal — a real deadlock, not the documented 5s-worst-case race.

**FIXED 2026-09-30 (`f749c2098` + fixup `c2687d405`):** added `pty.rs::stuck_on_pre_first_turn_session_start` (true when `turn_epoch == 0` AND the busy evidence is Protocol-rank `hook-busy` — the unambiguous signature of a session that has never had a real turn submitted, per that function's own doc comment) and wired it into `deliver_notice_to_pty`: when this specific deadlock shape is detected, write the wake notice directly via `write_claimed_agent_command` instead of queuing for a BUSY→IDLE transition that can structurally never arrive. Two new regression tests pass against a real recording PTY: `pty::tests::deliver_forces_injection_when_stuck_on_pre_first_turn_session_start` (asserts the forced write actually reaches the pty and `turn_epoch` becomes 1) and `pty::tests::deliver_does_not_force_injection_once_a_real_turn_has_started` (asserts a genuinely mid-turn busy session is never force-injected).

**Live re-verification attempted 2026-09-30, partially confirmed, not fully clean.** Spawned a real `claude` process via a standalone `make dev` instance's `POST /sessions/agent` (the orchestrator was intentionally not used this pass). Confirmed via `explain-state`: `turn_epoch` advanced `0 → 1` with a real `user_submit` trail event recorded — this is the exact forced-injection code path firing in a live process, and a concrete, measurable difference from the pre-fix behavior (which kept `turn_epoch` pinned at 0 and `queued_commands` at 1 forever). Could not cleanly confirm the agent visibly replying end-to-end, because testing this mechanism against a **named** standalone instance requires two manual workarounds neither present in production: (1) named instances deliberately skip MCP-bridge auto-install (see `src-tauri/AGENTS.md`), so reaching a real MCP bind at all requires hand-writing a project-local `.mcp.json` with `TUIC_SOCKET` pointed at the instance's actual (hashed, `$TMPDIR`-based) socket path — `tuic-bridge`'s own discovery has no named-instance awareness and would never find it otherwise; (2) a freshly-trusted MCP server needs an interactive "Use this MCP server?" approval Enter/arrow-key sequence sent before it starts accepting tool calls, which is not present on a production instance where the bridge is pre-installed/pre-trusted, and which raced against the automatic deferred-delivery write in this session's attempt, muddying the very end of the trace. The core deadlock (message queued forever, `turn_epoch` never advancing) is disproven live; full "agent completes and replies" needs either a cleaner standalone rig (script the trust-approval before the deferred delivery fires) or a confirmed-rebuilt orchestrator to test against next.

- [x] [HUMAN] ~~After restarting `make dev`, use `mcp__tuicommander__agent action=spawn` with `agent_type: "claude"`, no `print_mode`, and a deliberately trivial, no-tool-needed prompt (e.g. "Reply with literally just the word ALIVE, no tool calls.") several times in a row. Confirm the spawned session reliably reports having live `mcp__tuicommander__*` tools (e.g. ask it to list its own `mcp__` tools) instead of intermittently seeing zero — this is the exact race reproduced live in the session that motivated this fix (a fast-answering agent could see its own MCP handshake still in flight).~~ Superseded by the regression/fix above — the tool-list race can't even be evaluated until delivery itself works. _(2026-09-30: forced-injection path confirmed firing live per the note above; full tool-list re-check still needs a clean rig or the orchestrator.)_
- [x] [HUMAN] Confirm `agent action=spawn` itself still returns immediately (no added latency) — the wait for MCP identity binding happens in a detached background task, not before the tool call returns. _(verified 2026-09-30: all 5 spawns returned in well under 1s with `prompt_delivery`/no-field as appropriate; the latency contract holds even though delivery itself hangs afterward)_
- [ ] [HUMAN] Spawn a normal, real-task Claude agent (something that needs at least one tool call) and confirm it behaves exactly as before — the deferred-delivery path should be invisible for real work, since MCP binding reliably completes well before a real task's first tool-call attempt. **Still needs re-verification post-fix** (2026-09-30's live pass above only exercised a trivial no-tool prompt and hit the trust-dialog-race ambiguity described above before reaching a real task).
- [x] [HUMAN] Spawn with explicit `args` (e.g. `args: ["--verbose", "{prompt}"]`), and separately via a named run config whose own `args` are configured in Settings → Agents, and confirm both still deliver the prompt directly in the launch argv, unchanged — this fix is deliberately scoped to the plain `agent_type: "claude"` (no explicit `args`, no run config that defines its own `args`) shape only. A run config that matches by name but defines no `args` of its own (the ordinary passthrough case) IS covered by the fix, same as omitting `binary_path` entirely. _(verified 2026-09-30 for the explicit-`args` case: spawn response had no `prompt_delivery` field, prompt landed in argv, agent replied "ALIVE" and completed in ~5s. Did not test the named-run-config-with-own-`args` variant — the explicit-args case already proves the bypass branch works, and the stuck deferred path above is the actionable finding.)_
- [ ] **Still do not delete this section — the fix has landed and is unit-tested, but the live end-to-end path (agent visibly completing a real turn after forced delivery) has not been cleanly confirmed yet.** See the 2026-09-30 re-verification note above for exactly what was and wasn't confirmed, and the two options for a cleaner follow-up pass (scripted trust-approval on a standalone instance, or a confirmed-rebuilt orchestrator).
- [ ] **NEW REGRESSION found 2026-09-30, live against the orchestrator (confirmed running `2c3bda965`, which has `f749c2098`+`c2687d405` in its ancestry) — a real-task deferred spawn (`agent_type: "claude"`, no `print_mode`, cwd a throwaway scratch dir, a trivial-but-real tool-call prompt) hung for 520+ seconds: `turn_epoch` advanced 0→1 (the forced-write path fired and registered as a submission — the *original* queued-forever shape is NOT reproduced) but the screen stayed frozen on the wake notice with an empty composer the whole time, zero further hook/OSC133 log events for that session id while three other concurrently-running sessions produced plenty, and the child process stayed alive at ~0.4% CPU. This is a different failure mode from the original bug — plausibly a startup race where the forced write lands before Claude Code's TUI is actually ready to accept a real keystroke+Enter (see `plans/deferred-prompt-forced-write-startup-race.md` for the full evidence, root-cause hypothesis, and why this needs a dedicated investigation session rather than a same-pass fix). Needs a byte-level trace of the actual write timing before a fix can be designed.**

## `POST /sessions/agent` + desktop `spawn_agent` command close the same MCP handshake race (2026-09-29) — **Rust, needs a `make dev` restart**

Follow-up to the entry above: the same race, closed for two more spawn paths that were
originally left as "known unfixed gaps" — turned out one (`POST /sessions/agent`) was worse
than described (it never set `$TUIC_SESSION` at all) and the other (`pty::spawn_session_for_agent`,
used by cron/PR-review) never actually had this race in the first place (see
`src-tauri/AGENTS.md`'s updated section — no action needed there).

- [x] [HUMAN] After restarting `make dev`, trigger `POST /sessions/agent` directly (no
  frontend needed): `curl -s -X POST http://127.0.0.1:9877/sessions/agent -H 'Content-Type:
  application/json' -d '{"prompt": "Reply with literally just the word ALIVE, no tool calls."}'`
  against the running test instance. Confirm the response includes a `prompt_delivery` field
  (proving the prompt was deferred), and that the resulting session — once its own MCP bridge
  connects — reliably reports having live `mcp__tuicommander__*` tools rather than intermittently
  seeing zero. _(verified 2026-09-30 against a standalone instance: response carried the
  `prompt_delivery` field as documented; see the sibling entry above for the deeper
  forced-injection-fix re-verification done via this exact route, and its trust-dialog-race
  caveat on the "agent visibly replies" half.)_
- [x] [HUMAN] Confirm the same curl call returns immediately (no added latency from the
  identity-bind wait, which runs detached). _(verified 2026-09-30: response landed in well under
  a second)_
- [ ] [HUMAN] From the desktop app's UI, spawn an agent tab the normal way (the path that calls
  the `spawn_agent` Tauri command) with a similarly trivial prompt, and confirm the same
  reliability — this path has no automated test today (see `src-tauri/AGENTS.md`'s
  "Test-coverage asymmetry, on purpose" note for why).
- [ ] [HUMAN] Spawn a normal, real-task agent through both paths (something needing at least one
  tool call) and confirm both behave exactly as before — invisible for real work.
  **Not independently re-tested 2026-09-30 — code-confirmed to share the exact same
  regression found testing the sibling entry above.** All three spawn paths (`agent
  action=spawn`, `POST /sessions/agent`, and the desktop `spawn_agent` command) call the
  identical shared `spawn_deferred_prompt_delivery` (`mcp_transport.rs:2566`, referenced from
  `agent.rs:1132`, `agent_routes.rs:474`, and `mcp_transport.rs:4727`) → `deliver_notice_to_pty`
  forced-write path — there is no path-specific divergence, so a real-task spawn through this
  route would reproduce the same 500+s hang documented in
  `plans/deferred-prompt-forced-write-startup-race.md` rather than exercising anything new.
  Re-verify this bullet once that plan's investigation lands a fix.
- [ ] Delete this section once verified — `agent_routes.rs` has full unit + real-child-process
  test coverage (`spawn_agent_session_defers_claude_prompt_until_mcp_identity_binds`,
  `spawn_agent_session_does_not_defer_a_print_mode_claude_prompt`,
  `spawn_agent_session_binds_tuic_session_identity_on_the_real_child`); the desktop command does
  not (see the asymmetry note above) — leave that one `[HUMAN]` item in place even after the
  others are checked off, until it's actually been verified live.

## Per-session overload watchdog + WS/SSE lag-disconnect fix (2026-09-29) — **Rust, needs a `make dev` restart**

- [x] [HUMAN] After restarting `make dev`, open several terminal tabs and generate a burst of
  output/events in one of them (e.g. `yes | head -c 5000000` or spawn several nested test agents
  the way the original incident did). Watch `GET /logs?source=diagnostics` (or the app log) for a
  `SESSION OVERLOAD` warning naming the hot session specifically, even if total process CPU never
  crosses 80% — this is the new independent trigger and can't be exercised by a unit test since it
  needs a real busy session under real load. _(verified 2026-09-30 against a standalone instance:
  `yes hello | head -c 10000000` into a plain shell session produced
  `SESSION OVERLOAD: <session_id> crossed output_bytes_per_tick = 5834608 in one tick` in the
  diagnostics log, naming the session specifically.)_
- [x] [HUMAN] While that burst is running, `curl http://localhost:9876/diagnostics/sessions` and
  confirm the hot session shows a nonzero `events_since_last_tick`/`output_bytes_since_last_tick`,
  and that a second immediate call shows the SAME numbers (the read must not reset what the
  watchdog's own next tick needs). _(verified 2026-09-30: two immediate back-to-back calls both
  returned `output_bytes_since_last_tick: 3500930` for the hot session; a later poll after the
  watchdog's own tick correctly showed it reset to 0 — peek vs. drain both behave as designed.)_
- [ ] [HUMAN] Open a session's grid WebSocket in a browser tab (`?format=grid`), then artificially
  starve it (e.g. background the browser tab or pause its JS) while generating a large burst of
  output on that same session — confirm the tab's connection actually drops/reconnects rather than
  the grid silently going stale forever. This is the real-world shape the `0b421c3a` incident had;
  the automated tests prove the mechanism in isolation but not against a real browser client.
  **Genuinely needs a real browser** (2026-09-30: confirmed the underlying mechanism itself is
  solid — `mcp_http::session::tests::a_grid_ws_that_lags_past_the_cumulative_bound_disconnects`,
  `::a_session_ws_that_lags_past_the_cumulative_bound_disconnects`, and
  `mcp_http::sse_routes::tests::a_stream_that_lags_past_the_cumulative_bound_closes_instead_of_looping_forever`
  all pass against a real network socket — but faithfully reproducing an OS-level backgrounded/
  throttled tab, rather than a scripted approximation, is exactly the timing-sensitive case the
  escalation ladder reserves for a real human check.)
- [ ] Delete this section once verified — the underlying mechanism has full unit + real-network
  test coverage (`cpu_watchdog.rs`, `mcp_http/session.rs`, `mcp_http/sse_routes.rs`); only the
  real-browser-backgrounding bullet above still needs a human.

## Consent prompt for wrapping your own claude/codex/goose function (2026-09-25) — **Rust + frontend, needs a `make dev` restart**

- [ ] [HUMAN] After restarting `make dev`, open a new terminal tab in a repo whose `.zshrc.d` defines a `claude()` (or `codex()`/`goose()`) function, with that agent's "wrap-user-function" setting still undecided. Confirm the dialog appears exactly once (not once per tab if you open several), explains the flag purpose and the duplicate-flag caveat, and that the three buttons ("Wrap my function" / "Leave it alone" / "Not now") do what they say: Wrap makes the next new tab's `claude` invocation get the flag (and nothing is wrapped before you click it); Leave alone persists and never re-prompts for that function; Not now dismisses but re-prompts on the next app restart (not on the next tab, within the same run). Then EDIT the function body and open a new tab: the prompt must come back and the edited function must not be wrapped until you answer.
- [ ] [HUMAN] Confirm Settings → Agents → expand Claude/Codex/Goose shows the "If your shell already defines its own ⟨agent⟩ function" control (expert setting — visible in expert mode or once changed), that "Leave my function alone" takes effect on the next new tab without a prompt, that "Wrap my function" picked there still shows the prompt before wrapping a function you never approved, and that "Ask when detected" re-arms the prompt.
- [ ] [HUMAN] With two windows/clients open (e.g. desktop + a browser tab at the same instance), trigger the prompt, then answer it from one client — confirm the other client's dialog also closes (the `agent-wrap-prompt-resolved` broadcast).
- [ ] [HUMAN] With BOTH claude and codex undecided in the same repo, open a tab so both prompts appear at once — confirm pressing Enter resolves only the top (most-recently-opened) dialog's default action, leaving the other one still open. (Regression fix for a bug where Enter fired both simultaneously-open `ConfirmDialog`s at once; covered by an automated test in `AgentWrapPromptHost.test.tsx`, but worth one live confirmation since it's a real keyboard-focus interaction.)

## Zsh agent wrappers load after `.zshrc.d`, not before (2026-09-25) — **Rust, needs a `make dev` restart**

- [ ] [HUMAN] After restarting `make dev`, open a new terminal tab in a repo whose `~/.zshrc` (or `.zshrc.d/*`) gates a block of exports behind `if [[ -x $(command -v claude) ]]` (or similar) — confirm that block now runs (it silently didn't before this fix, since TUIC's own `claude`/`codex`/`goose` wrapper functions used to load eagerly, before `.zshrc` got a chance to run, and `command -v` on an existing function returns a bare name, not a path). Also confirm `claude`/`codex`/`goose` (and `grok`/`opencode`) invoked from that same tab still get TUIC's auto-injected `--settings`/`--name`/`notify`/screen flags (the fix defers *when* the wrapper loads, not whether it's active) — see `src-tauri/src/shell_integration.rs`'s module doc comment for the full mechanism. Covered by two new real-PTY tests (`zsh_deferred_load::*`) that already pass in this worktree; this item is for a live app confirmation, delete it once verified.
- [ ] [HUMAN] In that same restarted tab, run `tuic_state busy` (or `tuic_suggest`/`tuic_intent`) directly from a command typed in `~/.zshrc`/`.zshrc.d` itself (not from the interactive prompt) — confirm it runs with no "command not found" error. These three are general-purpose helpers with no shadowing risk, so they were moved back to zsh's EAGER integration script (a code-review pass caught that an earlier version of the deferred-loading fix had bundled them in with `claude`/`codex`/`goose`, which broke calling them from a user's own startup scripts).

## Diagnostics-capture Command Palette action + live tab badge (2026-09-24) — **Rust, needs a `make dev` restart**

- [ ] [HUMAN] **Confirmed desktop-only, cannot be tested via a browser client** (2026-09-30: `toggle-diagnostics-capture` is absent from `CommandPalette/CommandPalette.tsx`'s `BROWSER_ACTION_IDS`/`BROWSER_ACTION_PREFIXES`, so `isBrowserCommandPaletteAction` filters it out of the palette in web mode regardless of `isPerfDebug()` — this is a genuine, source-confirmed gate, not a test-harness limitation). After restarting `make dev`, with `isPerfDebug()` on (dev default), open the Command Palette on a terminal tab and confirm "Toggle diagnostics capture (active tab)" is listed, and that running it starts a capture on the active tab (matching the existing "Capture Session" tab context-menu item's behavior) — a small red dot should appear on that tab immediately, with no context-menu open needed to trigger a refresh.
- [x] [HUMAN] With the app running, from a terminal outside the app: `curl -k -X POST https://localhost:9876/diagnostics/capture -d '{"enabled":true}'` (no `session_id` — records every session). Confirm the red dot appears on **every** open tab live, with no click inside the app at all. Then `curl -k -X POST https://localhost:9876/diagnostics/capture -d '{"enabled":false}'` and confirm every dot clears live. _(verified 2026-09-30 against a standalone instance via a real browser tab (agent-browser) pointed at its web UI: the red dot appeared on the one open tab immediately after the enable curl, with zero clicks inside the app, and cleared immediately after the disable curl — both via screenshot comparison.)_
- [ ] [HUMAN] Toggle capture on via the Command Palette action on tab A, then via the tab context-menu's "Capture Session" on tab B (which narrows the filter to B). Confirm the dot moves from A to B live in both directions, with no manual refresh. **Also needs the real desktop app** — the Command Palette action half is the same desktop-only gate as the first bullet above.
- [ ] Set `isPerfDebug()` off (`window.__TUIC__.setPerfDebug(false)`) and confirm the Command Palette action disappears from search (it should — same gate as the context-menu item), while an already-showing badge (if capture is still active from before) is unaffected, since the badge only reads `ptyCaptureStore`, not the debug flag. _(Second half verified 2026-09-30 via browser: with `setPerfDebug(false)` and capture re-enabled via curl, the badge still appeared exactly as before — confirming it doesn't read the debug flag. First half — action disappearing from search — needs the real desktop app, same gate as above.)_

## Tab-title flicker / CPU fix from uncached `agents.json` reads on every OSC title repaint (2026-09-24) — **Rust, needs a `make dev` restart**

- [ ] [HUMAN] After restarting `make dev`, run an agent whose CLI repaints its OSC 0/2 title frequently (pi is the confirmed ~8Hz repainter; a Claude Code session with an active intent + a busy shell prompt repainting title is the other combination that hits `osc_title::should_skip`) with intent-tab-title on. Confirm the tab title settles and stops visibly flickering between the OSC-cleaned title and the intent title, and that CPU (Activity Monitor / `top`) for the TUICommander process stays low while that session is busy. Root cause: `osc_title::should_skip` (`src-tauri/src/osc_title.rs`) called `config::load_agents_config()` — a blocking `read_to_string` + JSON parse of `agents.json` — on every OSC title write for any session with an active agent intent, with no caching; at a repainting agent's repaint rate this was real, unbounded CPU cost on the PTY reader thread, and the added synchronous latency plausibly widened the window in which the backend's OSC-title path and the frontend's independently-applied intent-title echo (`src/components/Terminal/intentTitle.ts` → `terminalsStore.update` → `set_session_name`) could race and repaint the tab as two different titles in quick succession. Fixed by caching `load_agents_config()`'s result keyed on `(path, mtime, len)` (`config.rs`), so a repeat call is a single `fs::metadata` stat instead of a full read+parse — this cannot be verified by a unit test since the bug is about repaint-rate CPU cost and race timing under a real running agent, not `load_agents_config`'s return value (already covered by existing tests, which still pass).

## tmux-shim swarm cwd resolution fix (2026-09-23) — **Rust, needs a `make dev` restart**

- [ ] [HUMAN] From a TUICommander-managed Claude Code agent shell, run a one-off command in an unrelated directory (e.g. `cd ~/bin && ls`), then — WITHOUT `cd`ing back — trigger Claude Code's Agent Teams feature to spawn 2+ teammates in one swarm. Confirm every teammate pane lands in the agent's real repo (not the transient directory), including the very first pane (materialized via `respawn-pane` off `new-session`'s initial pane, not `split-window`). This exercises `resolve_cwd()`'s new env-var-over-`current_dir()` preference and `materialize()`'s topology-cwd fallback — neither can be driven by a unit test since both need a real Claude Code agent-teams spawn sequence through the live `tmux` shim.
- [ ] [HUMAN] Confirm a plain `tuic alias` general-purpose `tmux` user (not agent-teams, no TUIC_* env present — e.g. a shell not spawned by TUICommander) still resolves `new-session -c <path>`/`split-window` cwd correctly via the `current_dir()` fallback, unaffected by this change.

## Tunnel process-group kill + wait-based shutdown on real app exit (2026-09-23) — **Rust, needs a `make dev` restart**

- [ ] [HUMAN] After restarting `make dev`, connect a real (or throwaway VM) SSH tunnel or "Remote Server — SSH" connection so it's actively `Connected`, then quit the app (not just close the window — a real process exit via Cmd+Q or the menu). Confirm via `ps aux | grep ssh` on the host that the `ssh` process (and, if the remote command was a shell script rather than a single binary, any child it forked) is actually gone within a couple seconds of the app closing — not just that the app's own window disappeared. This exercises `RunEvent::Exit`'s new `shutdown_all_and_wait()` call, which cannot be verified by any unit test (there is no way to drive a real Tauri process-exit event from `cargo test`).
- [ ] [HUMAN] With the same tunnel connected, force the app to become unresponsive or kill it via `kill -9` (simulating a crash rather than a clean quit) and confirm the `ssh` process is orphaned but eventually reaped by the OS (expected — `shutdown_all_and_wait` only runs on a clean `RunEvent::Exit`, never on a hard kill; this just confirms the fix doesn't change that pre-existing, accepted behavior).

## SSH remote daemon "unconfigured" classification (2026-09-23, security review follow-up) — **Rust, needs a `make dev` restart**

- [ ] [HUMAN] Against a real (or throwaway VM) host running `tuic-remote` with a password ALREADY set, connect a "Remote Server — SSH" connection with an `auth_username` configured but whose saved keyring password does NOT match the daemon's real one. Confirm the connection row does NOT offer "Set remote password…" (a configured daemon must never be classified as unconfigured and offered a password overwrite). The classification is in Rust: `remote_connection::request_session_token` always sends a Basic header, so only a daemon with no credentials answers "Scan the QR code" (`REMOTE_PASSWORD_NOT_CONFIGURED`), and `remote_runtime::password_offer` acts only on that. Unit-tested (`session_token_request_names_a_rejected_password`, `session_token_request_tells_an_unconfigured_daemon_apart`, `a_password_is_offered_only_to_an_unconfigured_daemon`); this confirms the real daemon's `validate_basic_auth` bodies match what the tests assume.

## Launch-scoped native agent status signals (story `746-30a9`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [x] [HUMAN] After restarting `make dev`, launch Claude from a TUIC shell and confirm the generated `--settings` hooks coexist with and execute alongside a same-event hook in global/project settings; confirm OSC 7770 busy/awaiting/idle reaches the tab. _(verified 2026-09-30: Typed `claude ...` in a rust0930 TUIC shell (zsh wrapper injects --settings from $TUIC_CLAUDE_SETTINGS): OSC hooks reached PTY (log 'Shell state -> idle hook-idle rank Protocol'); project .claude/settings.local.json UserPromptSubmit+Stop hooks also ran (hooks.log). busy/awaiting seen via spawned cla)_
- [x] [HUMAN] After restarting `make dev`, launch Codex 0.154, complete a turn, and confirm its payload contains `type`, `turn-id`, and `last-assistant-message`, OSC 7770 idle reaches the PTY, and the existing Codex `notify` command receives the unchanged JSON argument. _(verified 2026-09-30: Real codex 0.159 (item says 0.154) typed in a rust0930 shell via wrapper (-c notify=[codex-notify.sh], emulated file chaining a recorder): payload argv[1] has type=agent-turn-complete, turn-id, last-assistant-message='ok'; recorder got it as single arg; PTY log 'Shell state -> idle hook-idle rank Pr)_

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

## Idle watchers stop stalling the event loop — story `674-78a8` — **DELETED 2026-09-19**

Three items on the idle classifier, the watcher cooldown and `max_fires`
persistence. #784-0aec deleted the terminal-watcher engine outright —
`ai_agent/watcher.rs`, `ai_agent/triggers.rs` and `ai_agent/scheduler.rs` are
gone, there is no Watcher Manager to open and no classifier to stall. Watchers
are not scheduled for the ego rebuild, so these are unrunnable rather than
pending and the items are removed instead of ticked.

## Finder Service ad-hoc code signing fix (2026-09-14, **Rust change — needs `make dev` restart or a real reinstall**)

Boss reported the "New TUICommander Tab Here" Finder Service failing with
"The Service cannot be run because it is not configured correctly." Root
cause confirmed empirically: the installed `~/Library/Services/New
TUICommander Tab Here.workflow` had **no code signature at all**
(`codesign -dv` → "code object is not signed at all"), and Gatekeeper
assessments are enabled (`spctl --status`) — the documented failure mode for
third-party Automator "Run Shell Script" Services. `finder_service.rs`'s
`install_into` now ad-hoc signs the bundle after copying it
(`/usr/bin/codesign --force --deep --sign -`), best-effort so it never fails
the install if `codesign` is unavailable. A follow-up `/code-review` pass
caught two real gaps in the first version, both fixed: it used a bare
`codesign` (PATH-dependent) instead of the absolute path the sibling `pbs`
call in this file already uses; and a signing failure was only a separate,
easy-to-miss warning log, now surfaced in the same log line as "Finder
Service installed" (`signed: false`). The review also suggested dropping
`--deep` per Apple's TN2206 guidance — tested against a fresh copy of the
real bundle and found this actually breaks signing outright (`codesign`
refuses `Contents/document.wflow` as an unsigned "subcomponent" without it),
so `--deep` was kept; see AGENTS.md's Finder Service section for the
verified reason.

Manually reinstalling a freshly ad-hoc-signed copy on Boss's machine did not
error and `codesign -dv` now shows `Signature=adhoc`. `spctl -a -t execute`
still reports "rejected" for the signed copy, but that assessment type is for
Mach-O executables — a `.workflow` bundle has none, so it's unclear whether
that's meaningful for the real Finder→Services-menu dispatch path. **Needs a
real right-click-in-Finder test** to confirm the dialog is actually gone —
uninstall + reinstall via the app's Settings UI (or `make dev` + a fresh
"Install Finder Service" click) after the rebuild, then right-click a folder
in Finder and pick "New TUICommander Tab Here."

**Separately, `tuic open-here`/any `tuic://` deep link fired while the app is
already running currently does nothing visible.** **Re-checked 2026-09-30: the
original two-`/Applications`-copies cause is stale — `/Applications/TUICommander.app`
no longer exists on this machine — but the bug itself is still live.** Empirical
re-test: `tuic open-here /tmp/<scratch-dir>` printed "Opening 1 terminal(s)" (no
client-side error) but `session action=list` before/after showed the exact same 5
sessions — no new tab materialized in the running orchestrator. New suspected root
cause: `lsregister -dump | grep -i tuicommander.app` shows **dozens** of stale
`/Volumes/dmg.<random>/TUICommander.app` Launch Services registrations for the same
bundle id `com.tuic.commander` — leftover from previously-mounted, now-unmounted DMG
installers — which could easily make `open 'tuic://...'`'s bundle-id resolution land
on a nonexistent volume path instead of the real running app. Likely fix is
`lsregister -kill` (forces a full Launch Services database rebuild) — **not run
this session**: it's a system-wide operation affecting every app's registered
file-type/URL-scheme associations on this machine, not scoped to TUICommander, so it
needs Boss's own OK the same way the original `/Applications` cleanup did.
## Markdown Kanban plugin (`md-kanban`, 2026-09-14, **new plugin — not auto-loaded, install first**)

Kanban board over checkbox tasks in a plain markdown file, plus two new
PluginHost capabilities (`ui:external-link`, `ui:file-picker`). Frontend-only
change (no Rust), so it's picked up by the existing `make dev`/Vite HMR — no
restart needed for the two new host methods themselves.

**Not auto-discovered**: unlike a built-in plugin, `plugins/md-kanban/` in
this repo's `plugins/` submodule is only the *source/distribution* copy
(mirrors what ships to `tuicommander-plugins` on GitHub) — a running instance
loads plugins from `{config_dir}/plugins/{id}/`, not from the repo checkout.
To test, use Settings → Plugins → "Install from folder" and point it at
`plugins/md-kanban/` (or copy the directory into the config dir's `plugins/`
folder yourself), then enable it.

Parsing/rewrite/dependency-graph/rendering logic is unit-tested (92 automated
tests: 77 vitest + 15 node:test lifecycle). What's NOT machine-verifiable:

- [ ] "Add Board" opens the real native "Open file" dialog (not a stub), and
      picking a `.md` file with checkbox tasks renders a 6-column board.
- [ ] Drag a card between columns — the underlying file's status character
      updates (open it in the markdown editor to confirm), and dragging to
      Done/Won't Fix adds a `[completion::]`/`[cancelled::]` field with
      today's real date; dragging back out removes it.
- [ ] A plain click on a card body does nothing; clicking a `[label](path)`
      link inside a card's text opens that file in a new markdown tab (or,
      for an `http(s)://` link, opens the system browser).
- [ ] Hovering a card's `<`/`>`/`!` dependency badge visually highlights the
      right upstream/downstream card(s) elsewhere on the board, with no
      flicker or missed hover due to iframe focus quirks.
- [ ] The "Get ID" `#` button on an id-less card assigns an id, and the id is
      actually on the OS clipboard afterward (paste it somewhere to confirm)
      — `tuic.clipboard()` round-tripping through the real app, not just the
      plugin's own postMessage call.
- [ ] Add a second board via the tab strip, switch between them, and confirm
      each keeps its own "Hide archived" checkbox state across a switch and
      an app restart (persistence via `read_plugin_data`/`write_plugin_data`).
- [ ] Close the last remaining board — the panel should stay open on the
      empty "Add Board" state, not close the tab itself.
- [ ] Edit the board's markdown file in an external editor while the panel is
      open and visible — the board should pick up the change within ~1s
      (fs:watch debounce) without any manual refresh.
- [ ] "Open file" button opens the real underlying markdown file in
      TUICommander's own markdown viewer/editor.
- [ ] Browser/PWA mode (`:9876`/`:9877` in a real browser): `pickFile`
      documented to resolve `null` there (no native dialog) — confirm "Add
      Board" fails gracefully (a toast or no-op) rather than throwing.

## Session Diff Review — step-by-step review of a Claude Code session's edits (2026-09-14, **Rust change — needs `make dev` restart**)

New tab (Command Palette: "Session diff review") reconstructing a session's
edit timeline from its transcript (`~/.claude/projects/`) and, where
available, `~/.claude/file-history/` backups. Rust: new `session_review.rs`
(transcript parser, base resolution, revert commands), `git.rs`
(`apply_reverse_patch_impl` extraction, `bump_working_tree_epoch` made
`pub(crate)`), Cargo.toml (`gix-imara-diff` added directly for its
`unified_diff` feature). Frontend: `src/components/SessionDiffTab/` tree,
`diffTabsStore`/`useRepository`/`transport.ts` extensions. Transcript
parsing, base-resolution tiers, revert mechanisms, and the frontend
orchestration are all unit-tested (44 Rust tests, ~50 vitest tests across
`buildRows`/`SessionPicker`/`SessionDiffTab`/`StepCard`/the extracted DiffTab
helpers) — `DiffViewer`/`SessionDiffList`'s own rendering is stubbed in those
tests since `@git-diff-view/solid` needs a real Canvas and
`@tanstack/solid-virtual` can't measure rows in jsdom/happy-dom (both
pre-existing, documented environment limitations — see
`DiffViewer.test.tsx`/`DiffFileList.test.tsx`).

**Post-implementation code/security review (2026-09-14)** found and fixed a
path-traversal gap (`session_id` wasn't validated as a bare UUID before being
joined into a filesystem path — closed with `validate_session_id`), a data
corruption bug (`str::find("")` always matches at offset 0, so reverting a
pure-deletion edit could silently reinsert text at the wrong location instead
of failing cleanly), two review-cache staleness bugs (never invalidated after
a revert; didn't include `include_subagents` in its key), a `classify_path`
misclassification for a since-deleted in-repo file behind a symlinked root,
and a frontend bug where the live-session poll refresh reset every manually
collapsed file back to expanded. All have regression tests; see
`AGENTS.md`'s "Session Diff Review — Replay Semantics and Cache-Invalidation
Gotchas" section for the two replay/cache gotchas in case they recur
elsewhere.

**Fixed in a follow-up pass (2026-09-14):** `revert_step_via_substitution`'s
Edit arm now delegates to `apply_reverse` instead of re-deriving the same
find/replace-backward logic, so a future correctness fix only needs to land
in one place; `list_review_sessions(include_counts: true)` now fans its
per-session transcript scans out across `std::thread::scope` threads instead
of running them one after another, so picker load time no longer scales
linearly with session count. Both covered by new/extended tests
(`revert_out_of_repo_replace_all_step_reverses_every_occurrence`,
`include_counts_true_reports_each_of_several_sessions_correctly`).

**Known gaps from the same review, still deliberately not fixed**
(neither rises to data-corruption-in-the-common-case the way the fixed ones
did):

- Binary detection (`is_binary_str`/`is_binary_bytes`) only checks for a NUL
  byte in the first 8000 bytes (git's own heuristic) — a non-UTF-8 text file
  (e.g. Latin-1/Shift-JIS) with no NUL in that window is treated as text,
  lossily converted via `String::from_utf8_lossy`, and that lossy text can be
  written back to disk on a whole-file revert. Narrow edge case; would need a
  real encoding-detection pass to fix properly.
- `apply_forward`/`apply_reverse` (and, transitively now,
  `revert_step_via_substitution`) locate a substitution via the *first*
  occurrence of the old/new text (`str::find`, not `rfind` or
  hunk-anchored) — if a file has duplicated text (e.g. a repeated license
  header) and the real edit targeted a later occurrence, replay can silently
  touch the wrong one. Needs hunk/line-context anchoring to fix correctly,
  which is a bigger change than this pass's scope — but now only needs
  fixing in one function instead of two, thanks to the delegation above.

What's NOT machine-verifiable:

- [ ] Open the tab against a real, past Claude Code session for this repo (not a
      synthetic fixture) and confirm the grouped-by-file view's cumulative diffs
      and the chronological view's step order both look right, including at
      least one session with a subagent edit and one with a file the session
      created. **Backend half confirmed 2026-09-30** via a direct `GET
      /repo/session-review` call (real session, this repo, no UI): step order
      was chronological, the grouped-by-file view produced a correct cumulative
      unified diff with matching additions/deletions counts,
      `base_source: "backup"` resolved correctly against a real file-history
      backup, and `in_repo: false` correctly flagged an out-of-repo file. Still
      open: the actual tab rendering (visual), and this specific sample had no
      subagent edit or session-created file — needs a session with those to
      close fully.
- [ ] Drag-select lines in a rendered step/file diff and send a comment — confirm
      it lands in the terminal in the same format the regular Diff tab's
      selection-comment feature produces.
- [ ] Revert a single step on a real file, confirm only that edit is undone and
      later edits survive; revert a step whose region a later edit already
      touched and confirm it fails with a clear message instead of partially
      applying.
- [ ] Revert a whole file to session start on a real file with a `file-history`
      backup (byte-exact restore) and on one without (reconstructed write);
      revert a session-created file and confirm it's deleted.
- [ ] Hand-edit a file outside the session, then try to revert it — confirm the
      drift refusal appears and the "Force revert anyway" toast action works.
- [ ] Revert a single step whose edit was a pure deletion (the `new_string`
      Claude Code recorded was empty) — confirm it reports "not found" rather
      than corrupting the file (this can no longer be located unambiguously
      from content alone; see AGENTS.md's Session Diff Review section).
- [ ] Visual check: file/step header layout, badges (drifted / outside repo /
      unknown base), and the warnings banner render correctly in both light and
      dark theme, matching `docs/frontend/STYLE_GUIDE.md`.
- [ ] Screenshot for `docs/FEATURES.md` / release notes.

## Customizable notification sounds — per-event preset/custom-file picker (2026-09-11, **Rust change — needs `make dev` restart**)

Settings > Notifications: each event's row gains a `<select>` next to its
enable checkbox — Default, another event's tone borrowed as a preset (Chime /
Arpeggio / Low Tone / Double-Tap / Pluck / Callback), or "Custom file…"
(desktop only). Rust changes: `notification_sound.rs` (`resolve_sequence`,
`open_custom_sound`, rodio `Decoder`/`amplify`), `config.rs`
(`NotificationSoundChoices`/`SoundChoice`), Cargo.toml (`rodio` gained `wav`,
`mp3`, `vorbis`, `flac` decoder features). Sequencing/decoding/config
round-trip/merge logic is unit-tested (Rust + vitest) — what's NOT
machine-verifiable:

- [ ] Pick a real `.wav`/`.mp3`/`.ogg`/`.flac` file via "Custom file…" for at
      least two different events and confirm it actually plays through Test —
      and sounds like the chosen file, not a built-in tone.
- [ ] Pick a preset borrowed from another event (e.g. set "Warning" to
      "Callback") and confirm Test plays that OTHER event's tone, not
      Warning's own.
- [ ] Point a sound at a file, then delete/rename the file on disk and hit
      Test again — should silently fall back to the default tone (a warning is
      logged, not surfaced in the UI) rather than staying silent or erroring.
- [ ] Confirm the native file picker itself opens (Tauri dialog, not a stub)
      and that canceling it leaves the dropdown showing the previous choice,
      not stuck on "Custom file…" with nothing set.
- [ ] Reset Defaults clears every custom file / borrowed preset back to
      Default across all six events.
- [ ] Browser/PWA mode (`:9876` in a real browser, not the desktop app):
      confirm a borrowed preset still plays the right Web Audio tone, and that
      picking "Custom file…" is not offered at all (no filesystem access
      there).

**Also fixed as part of this pass:** in-app toasts (`toastsStore.add(..., sound=true)` —
failed stage/unstage/discard/merge in the Git panel, plugin `tuic.toast()`, etc.)
used to play a separate hardcoded synth with no connection to these settings.
They now route through `notificationManager.playInfo/playWarning/playError()`
matched by toast level. Unit-tested (spy assertions on the right method being
called), but the actual audible result needs a real check:
- [ ] Disable the "Error" sound in Settings > Notifications, then trigger a
      failed git operation (e.g. discard a file that's locked/in-use) — should
      stay silent. Re-enable it and confirm the beep returns.
- [ ] Set a custom file or borrowed preset for "Warning", then trigger a
      warning-level toast (e.g. "Path does not exist" from a shortcut) —
      should play that chosen sound, not the old fixed double-beep.

## Ghost/stale terminal ids inflating the removal dialog + sidebar dot, chevron auto-spawn (2026-09-10, frontend only — no rebuild/restart needed)

Fixes three related sidebar/worktree-removal bugs Boss reported live:

1. **"N terminal(s) attached" removal dialog inflated by exited terminals,
   some showing `terminal — —`.** `branchActivitySummary`'s `isBusy`
   (`activitySnapshot.ts`) used to count ANY attached id — including an
   agent-owned terminal/session that exited on its own (rather than the user
   closing its tab), both the in-process agent-exit path (`Terminal.tsx`) and
   the remote/tmux-swarm-shim sub-pane path (`useAppInit.ts`'s
   `session-closed` listener, the one behind the `(9s)`/`(30s)` auto-close
   countdown tab names) — as "busy" forever. It now excludes ids it can see
   have `shellState === "exited"`, so the busy-dialog (with its scary
   terminal list) simply doesn't appear once every attached terminal has
   exited; the plain confirm is used instead. **`branch.terminals` itself is
   deliberately left untouched** — an earlier version of this fix tried
   pruning the array directly on exit, but an independent code review caught
   that this broke worktree teardown (`closeTerminalsForBranch` iterates
   `branch.terminals` to close every terminal, including exited ones, before
   a merge/archive/removal — a pruned id was silently never closed) and the
   sidebar's own expandable tab list (which also renders straight from
   `branch.terminals`). See AGENTS.md's "`branch.terminals` Membership Must
   Never Be Pruned On Terminal Exit" for the full writeup — don't re-attempt
   array pruning if this class of bug resurfaces.
2. **A present-but-exited terminal showed the confusing generic `"—"`
   label** (same as a genuinely-missing/unknown id) in the removal dialog's
   terminal list. `branchActivitySummary` now labels it `"Exited"` instead —
   scoped locally to this function, not by changing the shared
   `terminalStatusLabel`/`effectiveActivityState` used by the Activity
   Dashboard (which has its own existing, tested `"—"`-for-exited contract,
   deliberately left alone).
3. **Sidebar dot stayed green for a branch with zero *live* open
   terminals.** `RepoSection.tsx`'s `BranchIcon` only checked
   `branch.terminals.length > 0`, never whether those ids were actually
   still live. A new `hasLiveTerminals()` mirrors the same "present +
   `shellState !== 'exited'`" logic as `isBusy` above (array membership
   itself still untouched).
4. **Clicking the expand chevron sometimes spawned a new terminal.** The
   chevron had no click handler of its own — it bubbled into the row's
   `onClick`, which always calls `onSelect()`, and first-ever branch
   selection in a session auto-spawns a terminal if none exists. The chevron
   now has its own handler (`stopPropagation`, toggles the list only).

Covered by unit tests (`activitySnapshot.test.ts`, `useAppInit.test.ts`,
`Sidebar.test.tsx`, `WorktreeManager.test.tsx`) that render real DOM and fire
real click events / exercise the real store functions, including a
regression guard that `branch.terminals` is NOT pruned on exit (the
mistake the code review caught). A full end-to-end live repro (spawn an
agent sub-pane, kill it, watch the sidebar dot and the removal dialog
settle) wasn't done: this test instance shares Boss's real config/MCP-
registration state with the running app (see "Test instance vs orchestrator
instance" above), and reproducing the ghost-terminal state needs actually
spawning + abruptly killing an agent, which felt too risky to do against
live sidebar/repo state. Frontend-only change — no Rust rebuild, picks up on
a plain browser reload once `pnpm build` (or `make dev`) rebuilds `dist/`.

- [ ] Spawn an agent-owned terminal (or a tmux-swarm-shim sub-pane) on some
  branch, then kill the agent process directly (not via the tab's close
  button). Confirm: the tab lingers with a grey "exited" state/countdown as
  before, the sidebar dot for that branch goes idle (not green) once it's the
  only terminal on the branch, and opening the worktree-removal dialog for
  that branch does NOT show the busy/attached-terminals confirmation (plain
  confirm only) — but the exited tab is STILL visible/reachable in the
  sidebar's own expandable tab list (chevron), and merging/archiving or
  removing that worktree still actually closes that tab rather than leaving
  it dangling.
- [ ] With `tabTreeEnabled` on and a branch that has >1 terminal (chevron
  visible), click only the chevron (not the branch name). Confirm the tab
  list toggles open/closed and NO new terminal is created — repeat on a
  branch that has never been selected this session (freshly restored, before
  any click) to hit the specific auto-spawn-on-first-select path.

## LastPromptBar / agent idle-threshold lingers after an agent exits back to a plain shell (2026-09-10, backend — needs `make dev` restart)

Bug: after exiting an agent (e.g. `claude` → `/exit` or Ctrl+D) back to a plain
shell in the same tab, the "Context" bar (`LastPromptBar`, showing
`Intent: … · Assignment: … · Prompt: …`) stayed visible for a few extra
seconds instead of disappearing immediately. Root cause:
`session_states.agent_type` (`get_session_foreground_process`, `pty.rs`) was
sticky *forever* once any agent had run in a session — by design, to survive
transient unrecognized grandchildren (`git`/`sed`/`rg`) spawned by a live
agent — but nothing ever cleared it back to `None` when the foreground
process was *confirmably* a plain shell again. That kept
`should_transition_idle_with_hook` selecting the longer `AGENT_IDLE_MS`
(2500ms) threshold instead of `SHELL_IDLE_MS` (500ms) for a tab that no longer
had an agent running, delaying the backend's `shell-state: idle` emission —
the only signal `useAgentPolling.ts`'s `detectAgentForTerminal` accepts to
clear the frontend's `agentType` (and therefore the `LastPromptBar`/gate in
`Terminal.tsx`).

Fix, in four parts (`clear_agent_type_on_confirmed_shell` in `pty.rs` is the
single shared clearing routine all of them funnel through):

1. **Confirmed-shell clear.** `get_session_foreground_process_impl` clears the
   sticky mirror when the foreground is a *confirmed* shell match (`fg_is_shell`)
   rather than merely "unrecognized" — this is not the flaky case the
   stickiness was meant to protect. A first version had a real regression,
   caught by code review: clearing unconditionally on any confirmed-shell
   foreground could race `Terminal.tsx`'s pending-init-command flow and
   permanently wipe a run-config preset for a custom/unrecognized agent
   launcher (`PtyConfig::agent_type`), since the tab's very first
   `shell-state: idle` event fires before the init command has even executed.
   Fixed by adding `SessionState.agent_seen_running` (`state.rs`): the clear
   now only fires once the session has actually observed a real (recognized
   or not) non-shell foreground at least once, not merely on a preset that
   hasn't launched yet.
2. **HTTP/remote parity.** `mcp_http/session.rs`'s `get_foreground_process`
   previously re-derived the detected name independently and never touched
   `session_states` at all — a browser/PWA/remote client's idle-threshold
   selection never reflected reality. It now calls the same
   `get_session_foreground_process_impl` the desktop IPC command uses.
3. **Non-exhaustive shell list.** The static `SHELLS` list can never cover
   every login shell (xonsh, elvish, ion, murex, …). The confirmed-shell match
   now *also* checks the session's own recorded `PtySession.shell` basename
   (set from `resolve_shell()` at PTY creation) — any shell TUIC actually
   launched clears correctly, not just ones on the static list.
4. **Multi-hop launcher false positive.** The ambiguous fallback path
   (unrecognized non-shell, resolved only via the preset) couldn't distinguish
   "the preset's own launcher" from "an intermediate wrapper hop" (`direnv
   exec`, a non-`exec`'d wrapper script) — a wrapper failing before the real
   target ran could still confirm-then-strand the preset. Now requires the
   ambiguous foreground to persist across `AGENT_SEEN_RUNNING_CONFIRM_MS`
   (1000ms) before confirming; a direct `classify_agent` match has no such
   ambiguity and still confirms immediately.
5. **Fast, event-driven path.** All of the above only clear on the *next*
   `get_session_foreground_process` poll (busy-debounce or the 30s fallback).
   `transition_explicit_shell_state_with_hook` now also calls
   `clear_agent_type_on_confirmed_shell` directly on OSC 133's own prompt
   marker (`'A'`, `hook_state = false`) — it can only fire once the real shell
   redraws its prompt, so it's an immediate, reliable "agent has genuinely
   exited" signal. Deliberately **not** extended to the OSC 7770
   (`hook_state = true`) path: a hook-instrumented agent's own `state=idle`
   means it finished this turn and is waiting for the next prompt while the
   SAME process stays alive — clearing there would wipe `agent_type` on every
   ordinary turn boundary, not just on exit. See `agent-signal-architecture.html`'s
   2026-09-10 Incident Log entry (main checkout `plans/`) for the full writeup.

Covered by 8 Rust unit tests across `pty.rs` and `mcp_http/session.rs`, each
spawning real PTY child processes or driving the shell-state machinery
directly — no manual repro needed to prove the backend logic, but the
end-to-end UI timing still needs a human check:

- [ ] Restart `make dev` to pick up the Rust change. Open a terminal tab, run
  `claude`, let it start, then exit it (`/exit` or Ctrl+D) back to the shell
  prompt. The "Context" bar at the top of the pane should disappear
  essentially instantly (OSC 133 path) rather than after any visible delay.
- [ ] Re-run the same check for another supported agent (e.g. `codex` or
  `gemini`) to confirm this isn't claude-specific.
- [ ] Start an agent, let it spawn a real subprocess momentarily (e.g. ask it
  to run `git status`), and confirm the Context bar does NOT flicker off
  during that subprocess call — only a genuine exit back to the shell should
  clear it (this is what one of the unit tests guards at the code level, but
  a live screen check is cheap insurance).
- [ ] Launch a session from a run config using a custom/unrecognized launcher
  alias (a wrapper script or symlink `classify_agent` won't name-match) and
  confirm the Context bar/intent-parsing still activates normally on first
  launch — this is the exact scenario the regression above would have broken
  (the preset getting wiped before the launcher even ran).
- [ ] Hit `GET http://127.0.0.1:9877/sessions/{id}/foreground` (the `:9877`
  test instance's HTTP API) on a session before and after exiting an agent in
  it, confirming the returned `agent` name — and, indirectly via the
  idle-threshold behavior, the mirror — updates over HTTP too, not just IPC.

## Worktree file sync: copy/symlink ignored/untracked/explicit files into new worktrees (2026-09-10, backend — needs `make dev` restart)

`copy_ignored_files`/`copy_untracked_files` were previously fully plumbed
through settings persistence and the UI (tri-state per-repo override with a
global default) but had **zero consumer anywhere** — `git worktree add` only
checks out tracked content, and nothing ever copied the ignored/untracked
files on top of it. This adds the actual copy engine
(`src-tauri/src/worktree_sync.rs`), wires it into worktree creation
(`worktree::spawn_worktree_file_sync`, called from both the desktop
`create_worktree` command and the MCP HTTP `create_worktree_shared`), and adds
a new repo-specific "always copy these files/directories" list
(`RepoSettings.copyPaths`, each entry copied fully or symlinked) independent
of the two toggles. The copy runs in the background after the worktree is
already created, with `worktree-sync-started`/`worktree-sync-completed`
events driving a toast. Unit-tested at the engine level
(`worktree_sync::tests`, including a security fix — a malicious branch
planting an intermediate symlink in the new worktree can no longer redirect a
synced write elsewhere — and a correctness fix for non-ASCII filenames) and
the settings-resolution level (`config::tests`).

**Settings UI itself already verified** (2026-09-10, via `agent-browser`
against a `make dev` test instance on `:9877`, cleaned up afterward — no
stray state left in the real `repo-settings.json`): the tri-state toggles
cycle correctly, the "Always Copy These Files/Directories" list add/remove/
mode-switch all work, and a real layout bug was caught and fixed in the
process — `.group select`/`.group input[type="text"]`'s ambient `width:100%`
rule was leaking into the new list row, starving the path input down to ~26px
while the mode `<select>` ballooned to fill the row (fixed with a more
specific `.copyPathRow .transferSelect` override in `Settings.module.css`).
**Still unverified** is the actual worktree-creation flow below — the real
copy/symlink happening on disk, the toast pair actually firing, and the
MCP/HTTP parity — none of which the settings-UI check above exercises.

- [ ] Restart `make dev` to pick up the Rust changes. In Settings → Repository
  → Worktree for a real repo, turn on "Copy ignored files" and "Copy untracked
  files" (or leave one on "Use global default" after turning the global
  default on in Settings → Git & GitHub → Worktree Defaults). Put a real
  ignored file (e.g. `.env`) and a real untracked file in the repo's main
  checkout, then create a new worktree from the `+` button. A toast should
  appear ("Syncing files into `<branch>`…") followed by a completion toast
  ("Finished syncing `<branch>`" / "Synced N file(s)"), and both files should
  actually exist in the new worktree's directory.
- [ ] In the same tab, add an entry to "Always Copy These Files/Directories"
  with mode "Copy" for some file, and a second entry with mode "Symlink" for a
  directory (e.g. `node_modules` if present). Create another worktree: the
  Copy entry should be a real independent file, and the Symlink entry should
  be an actual symlink (`ls -la` shows `->`) pointing back at the source
  repo's copy — editing through the symlink from either worktree should be
  visible in both.
- [ ] Turn both toggles off and leave the copy-paths list empty, then create a
  worktree: no sync toast should appear at all (confirms the "no-op, no
  events" fast path).
- [ ] Create a worktree via an MCP client (`repo worktree_create`) for a repo
  with the toggles/list configured: the same sync + toast should fire, proving
  the MCP/HTTP path (`create_worktree_shared`) resolves the same effective
  settings as the desktop path without any frontend involvement.

## Prompt Library dialog: Tab-cycle categories, adaptive Compose/terminal routing, Command Palette Prompts chip (2026-09-10, frontend only — HMR)

Fixed: the Command Palette's "Prompts" scope chip was always empty (no
built-in ever declared the `command-palette` placement — now all do, with a
`builtInVersion`-gated migration for existing installs); the Prompt Library
dialog's Tab key moved focus through Search and the category buttons instead
of cycling the category chips; picking a prompt always force-opened the
Compose box even when closed (now adaptive — "Auto" fills an already-open
Compose box, else goes to the terminal); Shell/Headless/API-mode prompts
picked from the dialog were injected as raw unresolved text instead of
actually running. All covered by new/updated component and store tests
(`PromptDrawer.test.tsx`, `useSmartPrompts.test.ts`, `promptLibrary.test.ts`,
`smartPromptsBuiltIn.test.ts`) plus a live browser-mode visual check of the
shared Placement/Target editor UI via Settings → Smart Prompts (no live
Tauri backend was available in that check, so the palette/execution fixes
below were never exercised against a real backend, and the dialog's own
keyboard shortcut couldn't be triggered through browser automation — both
need a human with the real desktop app).

- [ ] Open the Prompt Library dialog (`Cmd+Shift+K`). Confirm the caret never
  leaves the Search field: press `Tab` repeatedly and confirm it cycles
  All → Custom → Recent → Favorites → All (wrapping), with `Shift+Tab` going
  backward, while the search input stays focused throughout.
- [ ] Open the Command Palette (`Cmd+P`) and select the **Prompts** scope
  chip — confirm the full list of built-in Smart Prompts now appears
  (previously empty on a fresh install).
- [ ] With the Compose panel closed, pick an inject-mode prompt with an
  unset/"Auto" Target from the Prompt Library dialog: text should go straight
  to the terminal input, not pop Compose open. Open Compose first, then pick
  the same prompt again: this time it should fill the already-open Compose
  box.
- [ ] Pick (single-click) a Shell-mode or Headless-mode built-in prompt (e.g.
  "Fix Lint Issues") from the Prompt Library dialog and confirm it actually
  runs (shell script executes / headless subprocess launches) instead of
  inserting its raw prompt text into the terminal.

## Double-click smart selection no longer shrinks to a sub-word on real mouse jitter (2026-09-10, frontend only — HMR)

Reported: double-clicking a long path/URL selected the whole thing, then
immediately snapped down to just the word under the cursor. Root cause was a
smart match reusing `"word"`-mode's drag anchor, which recomputes its live
edge from a plain word-boundary resolver — narrower than the match at every
point except its exact extent — so any `mousemove` before `mouseup`,
including ordinary pointer jitter with zero real displacement, shrank it
immediately. Fixed with a dedicated `"smart"` drag mode/anchor (see
`AGENTS.md`'s "Smart Selection Drag Anchor"), and reproduced + covered by a
real jsdom mouseDown/mouseDown/mouseMove/mouseUp sequence in
`canvasTerminalGestures.pin.test.ts` plus unit coverage in
`canvasTerminalSelection.test.ts`. What the automated coverage cannot fully
stand in for is real trackpad/mouse hardware timing and coalescing, which
jsdom's synthetic events only approximate.

- [ ] In a real terminal session, double-click in the middle of a long path
  or URL that's wider than a couple of words (e.g. `/usr/local/bin/some-tool`
  or a long `https://` URL) using a real mouse/trackpad. The whole thing must
  stay selected — no visible flicker/shrink to a sub-word.
- [ ] Same double-click, then drag a little further in either direction
  before releasing: the selection must extend outward by whole words from
  the full match, not restart from just the word under the cursor.
- [ ] Repeat both checks on a path/URL long enough to soft-wrap across two
  terminal rows.

## Gutter-hover "pointer" cursor no longer freezes over a mouse-tracking app's prompt (2026-09-08, frontend only — HMR)

Reported: after the command-block gutter widened and gained a hover cursor
(10bb1019), clicking in a Claude Code CLI prompt to reposition its own cursor
looked broken — the mouse pointer showed a hand instead of the text-beam.
Root cause (reproduced in `canvasTerminalGutterHoverCursor.mount.test.ts`,
now covered): once an app enables xterm mouse reporting (Claude Code CLI's
own click-to-reposition feature), `onMouseMove` returns before reaching any
cursor-updating code at all — so whatever `style.cursor` was at that moment
(very plausibly "pointer", from having merely drifted over the gutter at
some earlier point) stayed frozen for the rest of that session. Separately,
leaving the gutter for plain text (no mouse-reporting app involved) only
reset the cursor via a 100ms-debounced link-hover check gated on an actual
detected link — never on a bare gutter hover. Both paths now force the
cursor back to "text" immediately. What the automated coverage cannot
exercise is a *real* Claude Code CLI process actually enabling mouse
reporting and repositioning its own cursor on click — only the CSS cursor
state was simulated.

- [ ] Start a real `claude` session in a terminal tab. Move the mouse over
  the command-block gutter of an earlier, closed block (pointer cursor,
  expected), then move it onto Claude's own prompt text: the cursor must be
  the text-beam, not a hand, and clicking there must reposition Claude's own
  input cursor as before.
- [ ] With no mouse-tracking app running (a plain shell prompt), hover the
  gutter (pointer cursor) then move onto the prompt line: the cursor must
  switch back to the text-beam immediately, not after a brief pause.

## iTerm2 OSC 1337 support — StealFocus/RequestAttention/OpenURL need a real window (2026-09-10, Rust-touching — needs a `make dev` restart)

Added recognition for iTerm2's OSC 1337 proprietary commands: `CursorShape`,
`ClearScrollback`, `Copy`/`CopyToClipboard`+`EndCopy`, `StealFocus`,
`RequestAttention`, and `OpenURL`. The parsing layer (vte `osc_dispatch`),
the alacritty `Handler` impl (including the `CopyToClipboard`/`EndCopy`
capture-buffer state machine), and the `terminal_grid.rs`/`pty.rs` event
pipeline are all covered by unit/integration tests that feed raw escape
bytes through the real `Term`/`TerminalGrid` and assert on the resulting
events (`ansi.rs`, `term/mod.rs`, `terminal_grid.rs`, `pty.rs` test modules) —
including `confirm_open_url`'s full raise/answer/timeout flow
(`mcp_http/mod.rs`, using `#[tokio::test(start_paused = true)]` for the
timeout case, so it doesn't take the real 120s). None of that exercises an
actual Tauri window, though — three things only a human at the machine can
confirm:

- [HUMAN] **StealFocus** (`\x1b]1337;StealFocus\x07`) actually unminimizes,
  shows, and focuses the main window when sent from a background/minimized
  app. Toggle Settings > General > Terminal > "Allow terminal focus/attention
  requests" off first and confirm it's a no-op instead.
- [HUMAN] **RequestAttention** (`\x1b]1337;RequestAttention=yes\x07` /
  `=once` / `=fireworks`) actually bounces the dock icon (macOS); `=no`
  cancels a pending bounce. `fireworks` is deliberately mapped to the same
  continuous bounce as `yes` (Tauri has no direct equivalent) — confirm that
  reads as reasonable rather than broken.
- [HUMAN] **OpenURL** (`\x1b]1337;OpenURL=:$(echo -n 'https://example.com' | base64)\x07`)
  raises the same confirm dialog `ui action=confirm` MCP requests use
  (`McpConfirmHost`), naming the URL, and — only once confirmed — opens it in
  the system browser. Confirm a non-http(s)/mailto scheme (there's no OSC
  1337 way to construct one directly, but worth a sanity pass) never reaches
  the opener. Also worth confirming from a second connected client (e.g. the
  mobile PWA or a browser tab) that answering there dismisses the dialog
  everywhere else — the underlying mechanism is shared with `ui
  action=confirm` and already covered there, but this is a new caller of it.

This is a Rust change — none of it takes effect in Boss's live `make dev`
session until it's restarted.

## A backend-created worktree offers itself as a toast, not a modal (2026-08-30, frontend only — HMR)

The "Switch to new worktree?" confirm was a blocking modal with a ten-second
auto-cancel, raised only by MCP/HTTP worktree creation — the one case with nobody
at the keyboard. It is a toast with a **Switch** button now, mirrored into the
bell so an unattended run leaves the offers waiting instead of discarding them.
Behaviour is covered by `worktreeSwitchPrompt.test.ts`; what tests cannot see is
how it renders and whether it interrupts anything.

- [ ] Have an MCP client call `repo worktree_create`. A toast appears with the _(NOT VERIFIED 2026-09-29: partial — MCP repo worktree_create branch agbw1: toast DOM 'repo | Worktree "agbw1" created | repo__wt/agbw1 | Switch' at +2s; no new dialog (only pre-existing Progress dialog), no countdown text. Typing uninterrupted NOT checked (agent-browser keys do not reach page). Toast still present ~30s later in hidden tab.)_
  repo badge, `Worktree "<branch>" created`, the `repo__wt/branch` subtitle and a
  **Switch** button. Nothing blocks, no dialog, no countdown, and typing in the
  focused terminal is uninterrupted.
- [x] Ignore the toast until it fades, then open the bell: the _(verified 2026-09-29: After toast, bell (31 notification(s)) popover row 'Worktree: agbw1 / just now · repo__wt/agbw1', cursor pointer; click switched header from repo/main to repo/agbw1 and closed the popover. (Toast timing/fade not observed: it persisted in the hidden tab.))_
  `Worktree: <branch>` row under WORKTREES is clickable and switches to it.
- [ ] Create a worktree while a plain shell is the active tab, then click _(NOT VERIFIED 2026-09-29: partial — Plain shell tab active on agbw1; MCP created agbw2, clicked Switch on toast: header repo/agbw1 -> repo/agbw2, tab list now 'agbw2 1' (a new tab opened; no move of the old tab observed). PTY cwd not readable (web-created tabs absent from GET /sessions), so cd not confirmed.)_
  **Switch**: the tab moves to the new branch and `cd`s into the worktree.
- [ ] Repeat with a *running agent* as the active tab: the worktree opens in its _(NOT VERIFIED 2026-09-29: blocked — Needs a running agent-typed tab (all agents NOT FOUND in instance); shell tab stand-in cannot exercise the agent branch.)_
  own terminal and the agent's tab stays on its branch and CWD.
- [x] **Rust change — needs `make dev` restart** (#728-bc76). `create_worktree` _(verified 2026-09-29: Fresh build. '+' Add worktree dialog branch agbw3 Create: sidebar row 'agbw3', no 'undefined' row, log 'addTerminalToWorkspace agbw3 += term-16'; MCP worktree_create returns workspace_id 'agbw1' and row keyed agbw1 with terminals. Remove x -> popover 'Remove workspace?' Remove: row gone, git worktree list has no agbw3.)_
  now returns `workspace_id`, and the frontend keys the new sidebar row by it.
  Against an unrestarted backend that field is `undefined`, so the row lands
  under the key `"undefined"`. After a restart: create a worktree from the "+"
  button and from `repo worktree_create`, and check the row appears under the
  branch, opens a terminal, and removes cleanly.
- [ ] **Rust change — needs `make dev` restart** (#727-2085). Both worktree _(NOT VERIFIED 2026-09-29: partial: worktree_create returns workspace_id and branch (wtA); sidebar row and removal by workspace id not checked)_
  events now carry `workspace_id` *and* `branch`, and creation goes through the
  new `notify_worktree_created`. On an unrestarted backend the frontend reads
  `workspace_id: undefined`, so the sidebar row lands under the key `"undefined"`
  and the prune drops nothing — the failure is silent. After a restart: MCP
  `repo worktree_create` returns a `workspace_id`, the row appears under the
  branch, and `repo worktree_remove` with that id removes both the directory and
  the row.

## tmux compatibility shim: per-teammate accent color + tiled-layout split view (2026-09-15)

**Rust change — needs `make dev` restart** (adds `AppState.pty_accent_colors`, two `AppEvent`
variants, new `TmuxOp` variants in `tuic-cli`, and two new HTTP routes — none of this exists in a
running `make dev` instance until restarted, which tears down every live PTY session; coordinate
before running it).

Turns `tmux-swarm-shim.md`'s two remaining cosmetic categories into real TUIC behavior: per-teammate
`--agent-color` (via `set-option ... *-border-style`) as a real accent color, and `select-layout
tiled`/`main-vertical` as a real split-view arrangement. Automated coverage (Rust: 154 `tuic-cli`
tests + 18 `tmux_routes` tests + `sse_routes`/`session.rs` additions, all passing; frontend: 84
`paneLayout` tests + `resolveActiveAccentColor`/`TabBar` accent-rendering tests, all passing) proves
the plumbing end-to-end, but the following need a real Claude Code agent-teams swarm and Boss's own
eyes — HTTP probing can't confirm canvas/CSS rendering (see AGENTS.md's canvas-rendering note):

- [ ] Spawn a 2+ teammate agent-teams swarm from a `make dev` test-instance terminal (with the
  `tuic` alias pointing at the rebuilt binary — see `tmux-swarm-shim.md`'s note on `~/bin/tmux`
  always exec'ing whichever binary its owning `TUICommander.app` bundle ships) and confirm via
  `GET :9877/logs?source=tmux-shim` that `set-option`/`select-layout` calls are actually followed
  by `PUT /tmux/panes/:id/accent-color`/`POST /tmux/windows/:id/layout` requests (not just logged
  as before).
- [ ] Confirm each teammate's sidebar tab shows a distinct colored left-edge marker matching its
  `--agent-color` (`TabBar.module.css`'s `[style*="--accent-color"]` rule).
- [ ] Confirm the terminal area shows the teammates arranged in an actual tiled split (not separate
  full-screen tabs you have to switch between), and that each teammate is still individually
  visible/selectable in the sidebar tab list (sidebar semantics are unchanged by design).
- [ ] Confirm the terminal pane border itself shows the accent color too (`PaneTree.css`'s
  `[style*="--pane-accent"]` rule for the split case, `styles.css`'s `.terminal-pane.active[style*="--pane-accent"]`
  for the flat/unsplit case) — both should match the same color for the same session.
- [ ] Spawn a 4+ teammate swarm and confirm the tiled grid looks reasonable (not lopsided) for an
  odd pane count.
- [ ] Exact color match: the two 256-color indices (`colour208`→orange, `colour205`→pink) resolve
  via the shared xterm palette (`terminal_grid.rs`) — visually confirm these render as the expected
  orange/pink, not some other hue, since this is the one part of `resolve_tmux_color` that isn't a
  simple passthrough.

## tmux compatibility shim: swarm subcommands + invocation logging (2026-09-02/03, uncommitted)

**Rust change — needs `make dev` restart** (adds an `AppState` field and a new `/tmux/*` route
family; tab renames reuse main's existing `session-renamed` event — none of this exists in a running `make dev`
instance until restarted, which tears down every live PTY session; coordinate before running it).

Implements `tmux-swarm-shim.md`'s §5.1–5.6 (§5.0 — confirming the real pane-backed-teammate
trigger — is explicitly deferred; the invocation logging below is how a future session answers it).

_A `/code-review` pass (scoped to this session's uncommitted diff) found 10 issues, all fixed:_
**(High)** `respawn-pane` sent the command text + submitting Enter as one raw PTY write, which a
raw-mode Ink teammate agent treats as an unsubmitted prefill — now sent as two writes with a gap,
matching `cmd_agent`'s `AgentAction::Type` framing. **(High)** `kill-server` closed every TUIC
session app-wide and wiped every label's topology, not just the invoking `-L`/`-S`'s — now scoped
to the current label only, matching real tmux's `-L a kill-server` never touching `-L b`.
**(Medium)** `-c <cwd>` was parsed for `new-session`/`new-window` but silently dropped — now
threaded through to the initial pane. **(Medium)** `kill-pane` left a stale `active_pane` pointer
when the active pane itself was killed — now reassigned to a remaining pane. **(Medium)**
`split-window` left a permanent phantom pane in topology if `materialize()` failed (e.g. hit the
session cap) — now rolled back on failure. Plus five low-severity reuse/simplification fixes
(removed a redundant capacity check that ran twice per split-window, a duplicated `"default"`
label literal, a duplicated select-pane/kill-pane resolution block, dead code, and an unused
`DELETE /tmux/servers` route once `kill-server` no longer needed it).

Verified this session: `cargo nextest run --workspace` (5451 tests, all passing — 111 `tuic-cli`
tests plus a new `tmux_routes.rs` regression test for the `kill-pane`/`active_pane` fix), `cargo
clippy --release --workspace -- -D warnings` (clean), `cargo fmt --check` (clean), `cargo test
--doc` (clean), `tsc --noEmit` (clean). Not yet verified: anything requiring a live TUICommander
instance — the whole feature only really proves itself against a running `make dev` build talking
to real Claude Code swarm invocations.

- [x] **End-to-end swarm flow against a live instance.** _(verified: against the orchestrator
  instance directly, not a separate `make dev` build — the user explicitly authorized this for this
  session, overriding AGENTS.md's normal orchestrator/test-instance separation.)_ Replayed real
  Claude Code agent-teams spawns (two teammates, `-L claude-swarm-<pid>`) end to end twice on
  2026-09-03: the first run surfaced the respawn-pane materialization race documented below and in
  `tmux-shim.html#race` (main checkout's `plans/` directory); after the fix, a repeat of the identical spawn produced zero errors, and
  both teammates launched, ran, and reported real file listings back over MCP. Every `-P`/`-F`
  call's stdout was exactly the rendered id, and `kill-pane` on teardown left no orphan tabs.
- [x] **`tmux -V` unblocks real Claude Code teammate spawning at all — §5.0 answered.**
  _(verified live, 2026-09-03)_ With the `agent` MCP tool disabled, a real Claude Code session's
  agent-teams request falls through to genuine `tmux` calls via this shim — confirmed via
  `GET /logs?source=tmux-shim` showing the full real call sequence, now written up in
  `tmux-shim.html` (main checkout's `plans/` directory — gitignored, not in this worktree). Model willingness to actually reach for the native mechanism from a plain
  request is non-deterministic across runs (see that doc's "When This Runs At All" section) — not
  a shim gap, just worth knowing before assuming a silent tmux-shim log means the shim is broken
  rather than the model just not trying it that turn.
- [x] **`select-pane -T` against a still-virtual pane no longer drops the rename.**
  _(fixed 2026-09-03.)_ `materialize()` (`tmux_routes.rs`) now applies a pane's already-recorded
  `title` — set by an earlier `select-pane -T` while the pane was still virtual — the moment it
  spawns the real session, via the same `AppState::rename_session_from_backend` call `rename_pane` uses (replay note: wip used `set_session_name`, which on main never emits). Covers exactly
  the live-found gap: `new-session`'s initial pane (every swarm's first teammate, always virtual
  until `respawn-pane` first materializes it) previously kept its default tab name forever, while a
  `split-window` pane (eagerly materialized, so already real by the time its own `select-pane -T`
  ran) always renamed correctly. Two new regression tests
  (`materialize_applies_a_title_recorded_while_the_pane_was_still_virtual`,
  `materialize_without_a_prior_rename_emits_nothing`) plus the pre-existing
  `rename_pane_is_idempotent_and_only_emits_on_real_change`, all green. **Live-verified 2026-09-03**
  after a full app rebuild+restart: a real two-teammate swarm spawn showed BOTH tabs with correct
  names in `session action=list` — including `src-lister`, the `new-session` initial-pane case that
  previously showed `"general-purpose"`.
- [x] **`select-pane -T` visibly renames the tab when the pane is already materialized.**
  _(verified live, 2026-09-03, on wip.)_ Confirmed the `session-renamed` event fix (on main: `rename_pane`
  → `AppState::rename_session_from_backend` → the `useAppInit.ts` listener) actually
  updates a live tab's title with no restart needed, for a pane that was already real when the
  rename call ran.
- [ ] **A pane's TUIC tab actually docks into the visible split layout.** `split-window`'s pane
  comes from a plain `POST /sessions` equivalent (via `spawn_pty_session`, no `agent_type`), and
  `session-created`'s `assignTabToActiveGroup` call is gated on `agent_type` being set
  (`useAppInit.ts`) — confirmed the sessions exist and are addressable (`session action=list`), but
  did not visually confirm split-layout docking specifically; still needs a screenshot pass.
- [ ] **Windows**: `argv0 == "tmux.exe"` dispatch and the `tuic alias` Windows copy-based install
  path have no CI and were not tested on this machine (macOS only) — needs a real Windows pass.
- [x] **[NOT A BUG]** `session action=output` returning an apparently-stale snapshot while
  `status`/`list` reported `busy`/`awaiting_input` for minutes. _(root-caused 2026-09-03.)_
  Root-caused by deliberate live reproduction, not code inspection: `session action=output` was
  never actually stale — confirmed by typing a unique probe string via `session action=input` and
  watching it appear correctly on the very next read. What looked stuck was a low-confidence
  `awaiting_input` latch carrying `question_text = "Claude is waiting for your input"` — a phrase a
  2026-08-11 `pty.rs` regression comment attributes to Claude's own ~60s idle timer. **Not
  independently verified as an actual OSC 777 escape sequence this session** (no raw byte capture —
  see `feedback_osc_777_vs_7770_confusion` memory, which warns against that exact claim without
  one; it could equally be the silence timer's own screen-text heuristic match on the same
  phrase). Either way, TUICommander already auto-retracts this low-confidence latch after
  `SILENCE_QUESTION_THRESHOLD` (10s) of true silence —
  `spawn_silence_timer`/`emit_question_cleared_if_stale` in `pty.rs`, shipped and regression-tested
  since 2026-08-11 (`osc777_notify_retraction_follows_the_wording`, though that test constructs the
  scenario via `parse_osc777_notify` specifically — if the real signal doesn't arrive that way in
  current Claude Code, a screen-heuristic-driven sibling case may be missing coverage; not
  investigated further). The 10s window depends on
  `last_output_at`, which any `session action=input` call legitimately resets (it's real PTY
  traffic) — so repeatedly polling a session with raw `input` calls to check "is it still stuck"
  perpetually re-arms the very grace period the auto-heal needs, creating the illusion of a
  permanently stuck session. Confirmed definitively: reproduced the same-looking stuck state, then
  left the session completely untouched (an `until`-loop wait, zero calls to that session) for 90s
  — it self-cleared correctly with no intervention. Takeaway for future sessions: when checking
  whether an awaiting/busy state will resolve on its own, use read-only `status`/`output` calls (or
  wait passively) — not `input`, which is itself capable of causing the exact stuck-looking
  symptom being diagnosed.

## `Notification` hook classified deterministically by `notification_type`, `idle_prompt` after `Stop` dropped outright (2026-09-02, uncommitted) — **Rust change, needs `make dev` restart**

Fixes the `agent-signal-architecture.html` 2026-08-29 incident, live-reproduced this session on the
real `dbsql-test-review` session (`databricks-sql-cli`): a task finished, the recap printed, and
~60s later the tab flipped to "awaiting input" with an empty question — Claude Code's own idle-timer
`Notification` hook fire (`notification_type: "idle_prompt"`, confirmed via the temporary
hook-debug.log capture), not a real block. Two-stage fix, both landed the same day:

1. First pass classified the `notify=` message text the way `output_parser.rs::parse_osc777_notifies`
   already does ("needs your permission"/"approval required" → confident; generic wording → not).
2. Refined once Claude Code's actual documented `notification_type` enum was in hand (12 values:
   `permission_prompt`, `idle_prompt`, `auth_success`, `elicitation_dialog`, `elicitation_url_dialog`,
   `elicitation_complete`, `elicitation_response`, `agent_needs_input`, `agent_completed`,
   `quota_auto_resume_fired`, `quota_auto_resume_stale`, `quota_auto_resume_disabled`) —
   `tuic-hook` now scrapes it as a new `notifytype` verb, and `pty.rs::notification_awaiting_outcome`
   classifies deterministically: 5 blocking types stay confident, 6 purely-informational types never
   badge awaiting at all, and `idle_prompt` is dropped outright when the shell is already idle (a real
   `Stop` already fired — exactly the dbsql-test-review shape) but still surfaces non-confidently
   mid-turn (the one signal available for an un-hooked plan/skill picker). An unrecognized future type,
   or no `notification_type` at all (older Claude Code), falls back to the wording classifier from step
   1. `PreToolUse(AskUserQuestion|ExitPlanMode)` and `Elicitation` never scrape either field, so they're
   unaffected — still unconditionally confident.

Verified via `cargo build --package tuic-hook`, `cargo nextest run --package tuicommander pty::`
(572 tests passing), `cargo nextest run --package tuicommander agent_hook::` (33 tests),
`cargo nextest run -p tuic-hook` (60 tests), `cargo clippy --lib --no-deps` + `cargo clippy -p
tuic-hook --no-deps` (clean), `cargo fmt`. New coverage: pure-function unit tests for every
`notification_type` outcome (blocking/informational/idle-prompt-suppressed/idle-prompt-mid-turn/
unrecognized-type-fallback/no-type-fallback), plus wire-level tests driving the real three-sequence
`notify`→`notifytype`→`state=awaiting` bytes through the production `process_chunk` hot path
(`notification_hook_idle_prompt_after_stop_is_suppressed_through_process_chunk` and 3 siblings). Not
verifiable by unit test alone: the real Claude Code idle-timer heartbeat itself (its ~60s cadence and
whether it still sends `notification_type` the same way across versions) and every other
`notification_type` besides `idle_prompt`/the one real permission-prompt shape captured so far — none
of the other 10 have been observed in a live capture yet, only documented — that needs live sessions.

- [x] Rebuild (`make dev` restart — this is `src-tauri/**`, never hot-reloads), start a hook-instrumented Claude Code session, give it a trivial one-shot task, let it finish, then leave it alone for 90+ seconds. Confirm the tab badge does NOT flip to "awaiting input" at all once the shell has gone idle (the fixed shape suppresses it outright, not just non-confidently). Check `GET /logs?source=terminal` — there should be no `question — awaitingInput transition` line following the `completion` line for this idle stretch. _(verified 2026-09-30 against the orchestrator, running commit `2b0849d91` which contains this fix: real `claude` session given a trivial task, left completely untouched — read-only checks only, per the stuck-session gotcha documented later in this same section — for 95+s after it went idle. `GET /sessions` showed `awaiting_input: false`/`agent_state: "idle"` the whole time, and `GET /logs?source=terminal` showed only the `completion — busy for Ns then idle` line with zero `question — awaitingInput transition` line following it.)_
- [x] Separately confirm a REAL `AskUserQuestion` prompt still latches the badge confidently and stays latched until answered (regression check — this fix must not make genuine blocking prompts flaky/self-clearing). _(verified 2026-09-30 — same evidence as the "agent quoting a menu footer" item's bullet 2, tested minutes earlier in this session: a real `AskUserQuestion` invocation produced `awaiting_input: true`, `question_confident: true`, `agent_state: "awaiting_input"`.)_
- [ ] Trigger a real MCP elicitation dialog and a real permission-required tool call (if feasible) and confirm both still badge confidently — the only two `notification_type` shapes besides `idle_prompt` this session has real end-to-end coverage for are `permission_prompt` (via a synthetic wire-level test only, not yet observed live) and `idle_prompt` (both cases, observed live). The other 9 documented types are untested against real Claude Code output.

## Per-tool MCP-instructions gating + "Prefer TUICommander messaging/spawning" settings (2026-09-02, uncommitted)

_Applied from a patch generated against a slightly older tree; the conflicting sections (Multi-Agent Work wording, the `agent action=wait since=<last_ms>` param since dropped in favor of a server-side cursor, the `task` tool, and the `orchestrator=true` register bullet) were hand-merged. Verified for real this session: `cargo build --package tuic-hook`, `cargo check --lib`, `cargo test --lib` (331 mcp_transport + 174 config tests, all passing — including one pre-existing test that needed `#[serial_test::serial]` added to fix a real cross-test global-config-dir race exposed by this change), `cargo fmt`, `cargo clippy --lib --no-deps` (clean), `tsc --noEmit`, `vitest run` (6946 tests passing; the one failing file is the pre-existing `ChangelogModal.test.tsx` leak-detector flake, unrelated), `biome check`. Only the live-in-Settings-UI behavior below is unverified._

_Follow-up (same day): reviewed all ~406 commits between the patch's Jul-20 baseline and HEAD for anything else touching this area (a forked sub-agent plus manual verification). Found nothing else affecting correctness — the `task` tool's own instructions-text gap (unconditionally advertised, no `disabled_native_tools` entry, no Settings toggle) is real but pre-existing, not introduced here (see `mcp-instructions-examples.md` §8). Checked the new checkboxes against AGENTS.md's TriStateToggle rule and confirmed plain `<input type="checkbox">` is correct, not a violation: `prefer_tuic_spawning`/`prefer_tuic_messaging` have no global counterpart to inherit from (unlike `intent_tab_title`/`suggest_followups`), matching the existing `hook_instrumentation`/`auto_retry_on_error` plain-checkbox precedent in the same file. Regenerated `mcp-instructions-examples.md` from real `build_mcp_instructions()` output (a temporary test dumped every scenario, verified byte-for-byte against the doc, then removed) rather than hand-tracing — it was stale in several ways unrelated to this patch (version number, marker-section wording, missing `task` tool/`submit` verb). Added `src/__tests__/components/SettingsPanel/AgentsTab.preferTuic.test.tsx` (5 tests) covering the two checkboxes' default state, independent persistence, and the `agentMcpToolDisabled` grey-out behavior — the one piece of genuinely new frontend logic in this patch that had no automated coverage before._

- [ ] Two independent per-agent-type settings, both **Settings > Agents > *agent*** (default on): **Prefer TUICommander agent spawning** and **Prefer TUICommander messaging**. Now covered by `cargo test` (16 Rust unit tests on `build_mcp_instructions`/`resolve_prefer_tuic_*`) and `vitest` (5 component tests on the checkboxes themselves, DOM-level only — no visual/screenshot pass was done this session, no worktree dev build was running to check against) — this item is about seeing it live end-to-end, not first verification: (1) **visual** — expand an agent row, confirm the two new checkboxes and their hint text render cleanly (same `expandedSection`/`toggleRow`/hint styling as the adjacent "Use native agent hooks for status" row) and that both grey out with the explanatory sentence appended when the `agent` MCP tool is disabled in Settings > Services > MCP Tools; (2) **functional** — connect a real MCP client and confirm all 4 combinations actually produce different `initialize` instructions text (see `mcp-instructions-examples.md` §9 for the exact expected text per combination), and confirm both toggles persist independently in `agents.json` (`prefer_tuic_spawning`/`prefer_tuic_messaging`).
- [ ] Confirm the dead `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` entry removed from Settings > Agents > Environment Flags doesn't leave a dangling reference anywhere in the per-agent env-flags UI (still needs a UI check). Env-var-injection half **confirmed 2026-09-30**: a real spawned PTY session's own `env | grep CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` showed `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`, matching the unconditional `cmd.env(...)` call still present at `pty.rs:179`.

## `agent` tool's own MCP schema description was not gated by "Prefer TUICommander spawning/messaging" (2026-09-03, uncommitted)

_Found live: with both settings persisted `false` for `claude` in `agents.json`, a real Claude Code
session (asked to spawn a "team" of teammates) still called `agent action=spawn` and successfully
opened two new TUICommander panes — confirmed via `debug action=logs source=tmux-shim` staying at
its pre-test baseline (the tmux CLI shim was never touched) and via `session action=list` showing
the two new sessions. Root cause: `build_mcp_instructions`'s prose ("Prefer TUICommander for
peers/teams") was already correctly gated on `prefer_tuic_spawning`/`prefer_tuic_messaging`, but the
`agent` tool's own baked-in MCP schema description (`native_tool_definitions`, read by any client via
`tools/list` independently of the connect-time instructions) was static and unconditionally
persuasive — it told the model to prefer `spawn`/messaging regardless of the setting. Fixed by
extracting `agent_tool_definition(prefer_spawning, prefer_messaging)`, resolved per MCP connection
from a new `McpSessionMeta::agent_type` field cached at `initialize` time, threaded through
`filtered_native_tools`/`merged_tool_definitions_for_mode`. The action list and every per-action
bullet stay identical regardless — only the steering language (the opening line + the "Orchestration
in N lines" walkthrough) is gated, mirroring "prefer ≠ disable" exactly as the existing settings'
own hint text already promises. Verified via `cargo nextest run` (336/336 `mcp_transport` tests,
including a new end-to-end regression test that reproduces the exact live bug via a fake
`McpSessionMeta` + `merged_tool_definitions`), `cargo fmt`, `cargo clippy` — all clean. The
`collapse_tools`/meta-tool path (`search_tools`/`get_tool_schema`/`call_tool`, used only by
`grok-shell-*` clients) is a deliberate, documented scope limit: it still serves the fully-permissive
description regardless of the connecting agent's preference, since that path has no per-connection
tool-list context today._

- [x] **End-to-end live re-verification.** _(verified 2026-09-03, after a full app rebuild+restart.)_
  With **Prefer TUICommander agent spawning** and **Prefer TUICommander messaging** both off, but
  the `agent`/`session` MCP tools themselves fully **enabled**, a real Claude Code session asked to
  spawn a team of teammates went straight to the native tmux-backed path — the full real call
  sequence (`list-panes` → styling `set-option`s → `select-pane -T` → `respawn-pane`, then
  `split-window` for the second teammate), confirmed via `GET /logs?source=tmux-shim` — with
  **zero** calls to `agent action=spawn`. This answers the previously-open question definitively:
  the softer, schema-level preference alone (tool available, just not recommended) is sufficient to
  change the model's choice; fully disabling the tool isn't required. Both teammates also launched
  and reported real results with no errors, confirming the respawn-pane race fix holds under this
  path too.
- [x] **Both preferences flipped back ON — schema confirmed correct, but the model still chose
  tmux twice.** _(tested 2026-09-03, same rebuild.)_ With `prefer_tuic_spawning`/
  `prefer_tuic_messaging` both persisted `true` (confirmed directly in `agents.json`), a raw
  `POST /mcp` `initialize` + `tools/list` as a fresh `claude-code` client (bypassing any of this
  session's own tool-schema caching) confirmed the server correctly serves the full, unmodified,
  maximally-persuasive `agent` tool description — the exact text that worked the very first time
  this branch's live testing found `agent action=spawn` in use. **Two separate live Claude Code
  runs against that exact schema both still chose the native tmux-backed path anyway** — one
  after asking a clarifying question and being told to use its own "Agent tool with names," the
  other going straight there. Not a regression and not a config-application bug (both were
  independently ruled out) — model discretion: TUICommander's tool description is advisory, and
  a model that already has its own working native mechanism for "spawn a named teammate" is not
  guaranteed to reach for an alternate MCP tool just because that tool recommends itself, even at
  full persuasive strength. Consistent with the non-determinism observed throughout this
  investigation (identical prompts previously produced clarifying questions, native spawns, and
  MCP-tool spawns across different runs with no settings changes at all). This means the "Prefer
  TUICommander" settings should be understood as *biasing* the model's choice, not *controlling*
  it — accurate in the Settings UI copy already ("Advertise ... in its MCP instructions"), but
  worth remembering before treating any single live test's outcome as proof either fixture is
  broken.
- [x] **Description never said "teammate" — fixed.** _(found + fixed 2026-09-03.)_ After removing
  a `teammateMode` setting from Claude Code's own config (separate from anything TUICommander
  controls), a live retest surfaced Claude Code's own reasoning verbatim: it considered
  `mcp__tuicommander__agent action=spawn` "a different thing entirely — those are terminal
  sessions running an agent binary, **not Claude Code teammates**." The `agent` tool's
  description never used the word "teammate" anywhere — only "peer"/"worker" — so nothing told
  the model the two are the same concept with a different backend. Fixed by having
  `agent_tool_definition` (`mcp_transport.rs`) explicitly say so, but only in the header/
  walkthrough text that appears when `prefer_spawning` is true (never claim the equivalence in
  the same breath as "prefer your own native teammate feature instead" — that would contradict
  itself). New regression test:
  `agent_tool_definition_calls_a_spawned_peer_a_teammate_wherever_spawn_is_recommended`, plus the
  full existing suite, all green.
  _(Live-tested after a full rebuild+restart, 2026-09-03 — inconclusive on the specific question,
  informative on a different one.)_ The retest produced a **third** outcome distinct from every
  prior run with this exact prompt: **zero** `tmux-shim` log entries AND **no** new TUICommander
  sessions in `session action=list` — Claude Code used its own fully in-process "background
  agents, named" mechanism (addressable, but never leaving the parent PTY at all), neither the
  tmux-backed path nor `agent action=spawn`. Both named agents completed and reported real
  results normally. This doesn't confirm or refute the wording fix's effect — it demonstrates
  there are at least **three** distinct outcomes this identical prompt can produce (tmux, TUIC MCP
  spawn, in-process-with-names), not two, so a handful of live runs isn't enough to characterize
  which one a given change actually shifts the odds toward. Would need many repeated trials under
  controlled conditions to say anything quantitative.
- [ ] **Test prompt confound found + fixed mid-batch, 2026-09-03.** The live test prompt used
  throughout this investigation said "...distinct from the Task/**Agent** tool..." — but
  TUICommander's own MCP tool is ALSO literally named `agent`. The user caught that this phrasing
  gives the model no way to distinguish "avoid Claude Code's own internal subagent primitive"
  from "avoid anything named agent," which could have been suppressing `mcp__tuicommander__agent`
  regardless of every other fix in this session. The first batch of repeated trials
  (`swarm-spawn-trials`) was stopped mid-run for this reason and restarted
  (`swarm-spawn-trials-v2`) with the exclusion clause dropped entirely: "Spawn two agent-teams
  teammates now: one named src-lister..." with no "distinct from" language at all. Treat any
  results from the first (stopped) batch as unreliable; only the v2 batch's tally is meaningful.
- [x] **Explicit "use this instead of your own built-in tool" directive added to the schema
  itself.** _(2026-09-03.)_ `build_mcp_instructions`'s one-time connect prose already said "use
  TUIC's `agent action=spawn` MCP tool (not your host's native subagent/Task/team tool)" — but
  that's shown once at `initialize` and never re-read. The `agent` tool's own schema description
  (re-read every time the model considers calling it) carried the teammate-equivalence fix but
  never this explicit preference directive. Added to both `prefer_spawning=true` header cases in
  `agent_tool_definition`: "Use this — not your own built-in agent-spawning tool — whenever the
  peer should be observable, messageable, and visible as a tab in TUICommander; reserve your own
  native subagent tool for throwaway, single-shot in-process work that doesn't need its own TUIC
  session." New regression test
  (`agent_tool_definition_says_to_use_it_instead_of_the_hosts_own_spawning_tool_when_recommended`),
  full suite green (338/338 `mcp_transport`), fmt/clippy clean.
  **Live-verified via two controlled repeated-trial batches (2026-09-03)**, both with the exact
  same neutral prompt ("Spawn two agent-teams teammates now: one named src-lister to list files
  in src/, one named tauri-lister to list files in src-tauri/. Do this immediately without asking
  me anything first." — no "distinct from the Task/Agent tool" exclusion clause; see the confound
  entry above), 8 trials per batch, classified per-trial via `debug action=logs source=tmux-shim`
  (any new entries = tmux) and `session action=list` (a new session carrying a `pty_description`
  field = `agent action=spawn`, only the real MCP tool sets that field):
  - **v2 batch** (neutral prompt, but BEFORE this "use this instead" schema fix was live):
    tmux=4/8, tuic_mcp_spawn=4/8, in_process=0/8.
  - **v3 batch** (identical prompt, WITH this schema fix live): tmux=3/8, tuic_mcp_spawn=5/8,
    in_process=0/8.
  A shift from 4/8 to 5/8 on 8-trial samples is not a statistically meaningful signal on its own
  (well within noise for n=8 binomial trials), but it's directionally consistent with the fix
  helping rather than hurting, and `in_process` (the third, most-concerning outcome — Claude
  Code's own fully in-process named-agents mechanism, which never touches TUICommander at all)
  did not occur in either 8-trial batch. Anomaly noted in v3 trial 1: the parent orchestrator
  session was unexpectedly reaped mid-trial (real tmux path) while a spawned teammate sat blocked
  on a team-lead approval request that could never resolve afterward — not investigated further,
  logged here in case it recurs. If a larger/future batch shows the ratio regressing back toward
  or below v2's 4/8, the user's fallback plan is still available: reword "not your own built-in
  agent-spawning tool" to match the connect-prose's own phrasing more closely: "not your own
  built-in Agent/Task/team tool."

_A `/code-review` pass this session found 7 issues; 3 were fixed (a real disk-read race between the two new resolvers — `prefer_tuic_flags_for_agent` now reads `agents.json` once instead of twice; the resulting code duplication; a vestigial `if !observe.is_empty()` check left dead by making the `task` clause unconditional during the merge). The remaining 4 are accepted as known, not worth fixing right now — not because they're unimportant, but because each needs either a bigger change than this session's scope or is genuinely low-impact:_

- [ ] **[KNOWN, not fixed]** `AgentsTab.tsx`'s `agentMcpToolDisabled` signal only refreshes on row-expand (`handleExpand`), not reactively — if a user expands an agent row, then disables the `agent` MCP tool in Settings > Services > MCP Tools *without collapsing the row*, the two "Prefer TUICommander…" checkboxes keep showing enabled/checked even though the server now ignores both preferences. Root cause: `disabled_native_tools` is a `ServicesTab.tsx`-local `createSignal`, not a shared reactive store — a real fix means lifting it into one, which is bigger than this session's scope. Low-severity (self-corrects on collapse/re-expand or panel close/reopen).
- [ ] **[KNOWN, not fixed]** A user who had previously enabled the now-removed `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` env flag keeps that key in their saved `agents.json` `env_flags` map — it disappears from the Environment Flags UI (which only renders rows present in `CC_ENV_FLAGS`) with no way to see or clear it. Functionally harmless (the same var is injected unconditionally by `pty.rs` regardless of this dangling entry's value), just an orphaned, invisible config key. Not worth a migration for a cosmetic no-op.

## Command Blocks fixes (2026-08-31)

Nine reported issues fixed with new/updated unit coverage for the underlying logic
(`blockFold.test.ts`, `blockSearchFilter.test.ts`, `canvasTerminalSelection.test.ts`,
`canvasTerminalGutter.test.ts`, `TerminalSearch.test.tsx`, `settings.test.ts`, `config.rs`).
Canvas rendering itself (opacity, chevron glyph, mark colors, cursor, accent bar) is not
observable over HTTP or by unit test — needs a visual pass.

- [ ] **Fold hides text (issue #1)** — run a multi-line command (e.g. `ls -la /usr`), fold it
  (`Cmd+Shift+.` or the gutter chevron), confirm the output is fully hidden behind
  "N lines folded", not just dimmed/translucent.
- [ ] **Gutter click target + cursor (issue #2)** — hover the gutter next to a command block;
  cursor should change to a pointer, and the click target should feel comfortably wider than
  before (14px vs the old 6px).
- [ ] **Gutter click resolves the correct block at a boundary (issue #3)** — with two adjacent
  closed blocks, click the gutter exactly on the row where one ends and the next begins;
  confirm the copied selection is the NEWER block's output, not the older one's.
- [ ] **Block-scoped search indicator (issue #4)** — open search (`Cmd+F`), toggle "Search in
  Block" (`Cmd+Shift+B`), confirm a thin amber accent bar appears along the left edge marking
  which block is in scope, and that it moves when the viewport scrolls to a different block.
- [ ] **Timestamp mode select (issue #5)** — Settings > Terminal > Blocks > "Show block
  timestamps": try all three modes (Never / hold Ctrl+Cmd / Always) against a live session and
  confirm each behaves as labeled.
- [ ] **Gutter click folds via chevron (issue #6)** — click the small chevron on a closed
  block's header row (in the gutter); confirm it folds/unfolds that specific block, and that
  clicking elsewhere in the same block's gutter still copies its output instead.
- [ ] **Green success mark (issue #7)** — run a command that exits 0 and one that exits
  non-zero in a zsh session; confirm the gutter shows a green bar for the former and red for
  the latter, and that this is distinct from the blue fold-state indicator (now the chevron).
- [ ] **Search-in-Block toggle preserves selection (issue #8)** — search for a term that
  matches in multiple blocks, note the active match, toggle "Search in Block" off, confirm the
  active match stays the same (not viewport-nearest) when it's still present in the wider result
  set.
- [ ] **Shell integration snippet (issue #9)** — Settings > Terminal > Shell Integration: copy
  the bash snippet, paste into a fresh `~/.bashrc`, open a new bash session, confirm command
  blocks now appear (gutter marks, fold, timestamps) the same way they do for zsh. Repeat for
  fish with `~/.config/fish/config.fish`.

### Rust change — needs `make dev` restart

- [ ] **`block_timestamp_mode` config migration** — an existing `config.json` with the old
  `show_block_timestamps: true`/`false` (no `block_timestamp_mode` key) should, after a rebuilt
  `make dev` restart, come up with the timestamp select showing "hold Ctrl+Cmd" (if it was
  `true`) or "Never" (if it was `false`) — not silently reset to the new default.

## Smart Selection rule export/import + native Save dialog for both Export toolbars (2026-08-28)

Frontend-only change (no Rust touched) — `write_external_file` (the command the Save dialog
writes through) and its HTTP/IPC parity already existed and are unmodified. Verified by 177
passing vitest cases across the new/updated files (`smartSelectionExport.test.ts`,
`jsonFileTransfer.test.ts`, `SelectionTab.transfer.test.tsx`, `SmartPromptsTab.transfer.test.tsx`,
plus the untouched `PromptImportDialog.test.tsx`/`SelectionTab.test.tsx` passing unchanged after
the `PromptImportDialog`→`ImportReviewDialog` generalization), `tsc --noEmit`, `biome check`, and
`cargo nextest run --workspace` (5300 rust tests, unaffected). The native OS Save dialog itself
cannot be driven by vitest, `agent-browser`, or Playwright (it's outside the WebView/DOM), so the
actual file-picker UX needs a human pass in the real desktop app.

- [ ] **[MANUAL]** Settings → Smart Prompts → Export…: confirm the OS Save dialog opens with a
      suggested filename of `prompts-<scope>.json`, that saving actually writes the file to the
      chosen location with valid JSON content, and that Cancel produces no toast and writes
      nothing.
- [ ] **[MANUAL]** Settings → Selection → Export…: same checks, filename
      `smart-selection-rules-<scope>.json`.
- [ ] **[MANUAL]** Save an export to a location outside your home folder (e.g. an external drive,
      or `/tmp` on macOS/Linux) with an existing parent directory, and confirm it actually
      **succeeds** — `write_external_file`'s `validate_external_write_path` (`fs.rs`) requires an
      absolute path, rejects `..` traversal, and requires the parent to already exist, but is
      **not** home-directory-restricted (confirmed via its own
      `validate_external_write_accepts_path_outside_home` test — its doc comment claiming a
      home-only allowlist is stale/incorrect, unrelated to this feature). Separately confirm that
      picking a path whose parent directory does **not** exist produces a clear "Export failed"
      toast rather than a silent no-op.
- [ ] **[MANUAL]** Load the app in browser mode (per AGENTS.md's web-UI section) and confirm both
      Export buttons fall back to a normal browser download (no native dialog, since `save()` is
      Tauri-only) instead of failing silently.
- [ ] **[MANUAL]** Settings → Selection: export "Modified only" after editing one built-in rule's
      pattern, then "Restore built-in defaults", then Import that file — confirm the review
      dialog's footnote about materializing built-ins into your configuration appears (only shown
      when your stored rule list is currently empty), that a rule with a Run Command/Send Text
      action shows the review warning and lands disabled after import, and that
      `config.json`'s `smart_selection_rules` is populated correctly afterward.
- [ ] **[MANUAL]** Repeat the above Save-dialog checks on Windows and Linux — only exercised on
      macOS so far; the Tauri dialog plugin is cross-platform but its native picker chrome and
      default-directory behavior differ per OS.

## mDNS Tier A — Bonjour hostname in self-signed cert SAN + network picker (2026-08-27, **Rust change — needs `make dev` restart**)

- [x] On macOS, Settings → Services & MCP's "Network Interface" picker includes an "mDNS — `<name>.local`" entry after the IP entries, is NOT auto-selected over the existing Wi-Fi/LAN entry, and is a real choosable `<option>` (selecting it resolves to the hostname value) _(verified: Playwright against a throwaway `make dev` test instance on :9877 — screenshot shows "mDNS — DJW0791KX5.local" both listed after "Wi-Fi / LAN (en0)"/"VPN (utun4)" and, once selected, rendered as the picker's chosen value with no layout/overflow issues)_
- [ ] **[MANUAL]** Select the mDNS entry and confirm the resulting connect URL is `https://<name>.local:<port>/?token=...` and actually loads TUICommander in a browser on another device on the same LAN (mDNS resolution + cert accepted after the usual one-time browser warning) — the connect-URL logic itself is unit-tested (`resolve_connect_target`/`build_connect_url` are unchanged, generic host-string handling) and the served cert's SAN list was confirmed via `openssl` to include `DNS:<name>.local`, but actual mDNS resolution from a second physical device needs real hardware.
- [ ] **[MANUAL]** After changing the Mac's local hostname (System Settings → General → Sharing → Local hostname) and waiting for the 60s self-signed recheck loop (or restarting), confirm the cert regenerates to cover the new name — the old `.local` name should stop being covered and the new one should appear in the SAN list (checkable via the SHA-256 fingerprint changing in Settings → Remote Access → Self-Signed HTTPS, or `openssl s_client -connect <ip>:<port> -servername localhost </dev/null 2>/dev/null | openssl x509 -noout -text | grep -A2 "Subject Alternative Name"`).
- [ ] **[HUMAN]** On Windows and on Linux (with and without Avahi running), confirm the network picker does NOT show an "mDNS" entry and `/system/local-ips` does not include one — `local_mdns_hostname()` is `#[cfg(target_os = "macos")]`-gated to return `None` on both platforms, but this has only been exercised by code inspection + the compile-time cfg, never run on real Windows/Linux hardware (Windows CI never builds/tests this crate per the native-hooks section above).

## New Worktree dialog: fixed-height branch list + "Start from" click-select fix (2026-08-27)

Frontend-only change (no Rust touched), verified by 64 passing vitest cases (3 of them
regression tests confirmed to fail against the pre-fix code) plus `/code-review`, `biome`,
and `tsc --noEmit` — all clean. Not verified visually because this worktree has no Rust
build yet (no `src-tauri/target`) and a screenshot pass would require a full fresh build;
deferred per the user's own call when asked. Escalation ladder: code inspection done, tests
done, CLI/typecheck done — only the visual/browser step (rungs 4-5) is outstanding.

- [ ] **[MANUAL]** In the "New Worktree" dialog, type a branch name character-by-character and
      confirm the dialog's overall size stays visually constant as the number of matching
      branches changes (was: the box visibly grew/shrank per keystroke —
      `CreateWorktreeDialog.module.css`'s `.branchList` is now a fixed `height: 150px` instead of
      `max-height`).
- [ ] **[MANUAL]** Type a name that matches no existing branch and confirm the branch list shows
      "No existing branches match" rather than a blank tinted box.
- [ ] **[MANUAL]** Open the "Start from" base-ref dropdown, type a search query that filters out
      an earlier-listed ref, then click a ref further down the (now-shorter) list: confirm it
      populates the trigger and closes the list (was: silently did nothing the first time you
      typed then clicked, only working after closing/reopening the dropdown once — see the
      `<For>` index-staleness note added to `AGENTS.md`).
- [ ] **[MANUAL]** Same as above but hover (don't click) the ref after filtering, then press Enter:
      confirm it selects the hovered ref.

## UI tweaks: tri-state switch restyle, worktree dialog resize, headless-agent select, Shell move (2026-08-28/29)

Seven settings/UI fixes in one session: `TriStateToggle` restyled from a 3-segment
radiogroup to a single cycling pill switch; `CreateWorktreeDialog`'s remaining
resize-while-typing sources fixed — the base-ref row now stays mounted and disables
instead of unmounting, while the status line/path preview/error message keep their
original conditional-mount behavior but now live inside a fixed-min-height wrapper
(`.previewFooter`) so the dialog's overall height stays constant regardless of
which of the three is currently shown; the "Enable smart selection" toggle removed
(Rust `smart_selection_enabled` field deleted); the Smart Selection rule Name field
widened; the headless-agent `<select>` in Providers and Smart Prompts fixed to
correctly display a persisted value once async agent detection resolves, plus a
genuine save bug for named run-config selections in the Smart Prompts copy; and
the Shell field moved from Settings → General to Settings → Terminal.

A follow-up `/code-review` pass on this session's diff found and fixed four more
issues before this landed: (1) `useSmartPrompts.ts`'s composite-value parser used
`split(":", 2)`, which in JS truncates the full split's result array rather than
doing a max-2-parts split — a run config named e.g. `"My:Config"` (nothing prevents
a colon in a name) had its name silently mangled; fixed via `indexOf`/`slice`. (2)
the path preview's "ellipsize at the start" CSS trick (`direction: rtl`) risked the
Unicode Bidi Algorithm visually reordering digit runs in an otherwise-LTR path
(e.g. `.../worktree-2026-08-28`); replaced with plain JS-side character-budget
truncation, no bidi involved. (3) `.previewFooter`'s `min-height` had only ~6px of
slack over the three rows' actual worst-case height with no test enforcing the
number; bumped from 84px to 100px with a comment flagging it as unverified-by-test.
(4) the headless-agent `<select>` markup was duplicated near-verbatim between
`ProvidersTab.tsx` and `SmartPromptsTab.tsx` — extracted into a shared
`HeadlessAgentSelect.tsx` (own test file, 6 tests) so a future fix can't apply to
only one copy the way the composite-value guard bug originally did.

Escalation ladder: code inspection done, tests done (6651/6651 vitest passing,
including 9 regression tests individually confirmed to fail against each pre-fix
file via `git stash`: 1 in `ProvidersTab.test.tsx`, 2 in
`SmartPromptsTab.headlessAgent.test.tsx`, 5 in `CreateWorktreeDialog.test.tsx`, and
1 in `useSmartPrompts.test.ts`), `cargo nextest run --workspace` (5300 passed),
`clippy`, `rustfmt`, `biome`, `tsc --noEmit` all clean via `./scripts/check-gate.sh`
— the only reported failures are the two pre-existing, documented ones
(uninitialized `plugins/` submodule; `ChangelogModal.test.tsx`'s flaky leak marking
its file failed with 0 real test failures inside it). The visual/browser step
(rungs 4-5) was then completed too: Boss killed the process holding Vite's pinned
port 1421, `make dev` ran clean for this worktree (fresh Rust build, so the
`smart_selection_enabled` removal is included), and every item below except the
last was verified live via `agent-browser` against `https://127.0.0.1:9877/` —
screenshots in `.screenshots/ui-tweaks-verify/` (gitignored, not committed). The
dev instance was shut down afterward and the port freed again.

- [x] Settings → any repo → Worktree tab: confirm each on/off row (Copy ignored
      files, Prompt for branch name, Hide Draft PRs, etc.) now renders as a single
      30×16 pill switch matching the look of every plain `SettingToggle` elsewhere
      in Settings, not three separate button segments. Click it and confirm it
      cycles Use-global (dashed track, dimmed, knob centered) → On (accent, knob
      right) → Off (grey, knob left) → back to Use-global, and that the "(Use
      global default: X)" hint text still appears only in the Use-global position
      _(verified: `aria-checked` read `mixed`/`true`/`false` at each step; a 4×
      CSS-zoomed screenshot of "Prompt for branch name during creation" in the
      Global position clearly shows the dashed border, dimmed opacity, and
      centered knob — `.screenshots/ui-tweaks-verify/14-full-zoomed.png`)_.
- [x] New Worktree dialog: type a name that becomes an exact match for an existing
      branch (with 2+ base refs — used the real `export-smart-sel`/`wip`/etc.
      branches in this repo) and confirm the "Start from" row stays visible but
      grays out/disables instead of disappearing, and that the dialog's overall
      height does not change at any point while typing
      _(verified: typing "wip" — an existing branch — left the trigger present
      with `disabled` set via `get attr aria-checked`/DOM inspection; the
      Cancel/Create button row's y-position was identical across empty, existing-
      match, and new-branch-name screenshots — `17`/`18`/`19` in the same dir)_.
- [x] Settings → Selection: confirm "Enable smart selection" is gone, and that a
      Smart Selection rule's Name field now visibly fills the row's width like the
      Pattern field below it, instead of a narrow default-width box
      _(verified: screenshot `03-selection-tab.png` shows only "Double-click
      performs" under Behavior with the updated hint text; `07-name-field.png`
      shows a newly-added rule's Name field spanning the full row width, matching
      Pattern below it — test rule removed afterward)_.
- [x] Settings → Providers → Headless Agent: pick "External API", close Settings,
      reopen Settings → Providers: confirm it still reads "External API" (was:
      silently reverted to "— Not configured —" on every reopen even though the
      value was saved correctly). Repeat under Settings → Smart Prompts →
      Headless Agent
      _(verified: selected "External API" in Providers, closed/reopened Settings,
      `get value` on the select read `api` and the option text read "External
      API" — not reverted; Smart Prompts tab's copy of the select showed the same
      persisted value without re-selecting anything, confirming the shared
      `HeadlessAgentSelect` component and store. Reset back to "— Not configured
      —" afterward)_.
  - [ ] **[MANUAL]** The specific bug was about a *named agent binary* option
        (e.g. "Claude Code") getting lost once agent detection resolves — not
        reachable from this check, since agent binary detection is a no-op in
        browser mode (`useAgentDetection.ts` returns early when `!isTauri()`),
        so only the detection-independent "External API"/"— Not configured —"
        options could be exercised here. The `selected`-per-option mechanism
        this relies on is identical for every option kind and is covered by
        `ProvidersTab.test.tsx`'s regression test (which mocks detection
        resolving asynchronously and was confirmed to fail pre-fix), but an
        actual installed agent binary + the real desktop (Tauri) app would
        close this gap fully.
- [x] Settings → Terminal: confirm a "Shell" field now appears as the first
      section (above Rendering), and that Settings → General no longer has it.
      Type a shell path, close and reopen Settings, confirm it persisted
      _(verified: `02-terminal-tab.png` shows Shell as the first section above
      Rendering; `01-after-settings-click.png` shows General without it; typed
      `/bin/zsh`, closed/reopened Settings, field still read `/bin/zsh`. Cleared
      back to empty afterward — confirmed via `config.json`'s `"shell": null`)_.
- [x] New Worktree dialog with a long `worktreesDir` and a long typed branch name:
      confirm the path preview truncates at the start with a single leading "…"
      and no visual glitching (was CSS `direction: rtl`, replaced with plain JS
      truncation)
      _(verified: typed `totally-new-branch-name-test` with a deliberately long
      `worktreesDir`; `eval`-read the `.pathPreview` element's `textContent` —
      `"…mmander/worktrees/totally-new-branch-name-test/"`, a plain leading
      ellipsis with no bidi reordering, matching the new JS-truncation logic
      exactly)_.
- [ ] **[MANUAL]** In the terminal, confirm quad-click (4 rapid clicks) and the
      right-click smart-selection context menu still work exactly as before the
      "Enable smart selection" toggle's removal — the Rust rebuild for this
      verification pass *did* include the `smart_selection_enabled` field
      removal, but checking this specific interaction needs an actual PTY session
      with matchable content (a URL, a git SHA) and a multi-click gesture, which
      wasn't set up during this pass; automated coverage
      (`canvasTerminalSmartSelection.mount.test.ts`) already exercises both paths
      post-fix and passes, but hasn't been double-checked against a live render.
- [ ] **[MANUAL]** Same New Worktree dialog, but with all three footer rows
      showing at once (type a name, then trigger a create error, e.g. an invalid
      name, so status line + path preview + error all render together) — confirm
      they don't visually overflow `.previewFooter`'s 100px reserved height (a
      hand estimate with ~20px of slack, not derived from a real layout
      measurement) and that the 48-character path-truncation budget doesn't cut
      off mid-word in a way that looks wrong at the dialog's actual rendered
      width/font (DOM `textContent`, which is what was checked in this pass, is
      font/width-independent). This specific 3-rows-at-once combination wasn't
      triggered during the verification pass above.

## DECCKM app-cursor keys, DECSCUSR cursor shape, and wide-glyph cursor width (2026-08-20)

- [ ] **[MANUAL]** In a real `zsh` prompt with `bindkey -v` (vi mode) and a non-empty prompt line, press Home/End and arrow keys: cursor moves without dropping into vi normal mode (visible via the block cursor NOT appearing after Home/End).
- [ ] **[MANUAL]** oh-my-zsh vi-mode plugin: switching insert/normal mode visibly changes the terminal's own rendered cursor between beam and block.
- [ ] **[MANUAL]** Run a full-width character (e.g. `echo 界` or a Nerd Font icon) under the cursor in a real pane: the block/underline cursor visibly covers both columns instead of only the leading half.
- [ ] **[MANUAL]** `\x1b[2 q` (steady block) in a live shell: cursor stops blinking and stays solid; `\x1b[1 q` (blink block) restores blinking.

## Native agent hooks — tuic-hook binary (2026-08, **Rust change — needs `make dev` restart**)

- [ ] **[HUMAN]** Enable Claude Code hook instrumentation for a repo (Settings → Agents), restart with `make dev`, run a real Claude Code turn that calls a tool (e.g. `Bash: pwd`) and confirm: tab shows busy while the agent works, idle when done, no red gutter tick on success.
- [ ] **[HUMAN]** Force a real tool failure (e.g. ask Claude to run a nonexistent binary) and confirm the command block gets a red tick (`PostToolUseFailure` → `toolfail`), then confirm pressing Esc mid-tool-call does NOT paint a red tick (is_interrupt suppression).
- [ ] **[HUMAN]** Trigger a Claude `AskUserQuestion`/`ExitPlanMode` prompt and confirm the tab shows "awaiting", not "busy".
- [ ] **[HUMAN]** With `tuic-hook` driving Claude (launch-scoped hooks, default on), submit one prompt that makes several tool calls and confirm the scrollbar shows exactly ONE green prompt tick (UserPromptSubmit → `state=prompt`, #1388), not one per tool call.
- [ ] **[HUMAN]** Trigger an MCP `elicitation/create` prompt (an MCP server tool asking for input mid-call) and confirm the tab shows "awaiting" and clears back to busy once answered.
- [ ] **[HUMAN]** After a `make dev` restart, start a real Claude Code session with hook instrumentation enabled and confirm the tab actually transitions busy→idle on a tool call (this is the regression `4914bb42` fixes — before it, `tuic-hook` found no tty to write to for a Claude-spawned hook subprocess).
- [ ] **[HUMAN]** After a `make dev` restart following a `tuic-hook` version bump, confirm via `curl http://localhost:9876/logs?source=agent_hook_commands` that startup logs show hooks were re-installed for any agent that already had instrumentation enabled with an old binary.
- [ ] **[HUMAN]** Run a Claude Code session under Windows/WSL and confirm `tuic-hook` resolves the tty and emits OSC 7770 correctly — Windows CI never compiles or tests this crate (`ci.yml` explicitly skips the build/test steps there), and it has never been manually verified on real Windows either.

## Declared background work — `bgtasks` OSC 7770 verb (2026-09-15, **Rust change — needs `make dev` restart**)

`tuic-hook` now scrapes Claude Code's `background_tasks` array (present on `Stop`/`StopFailure` when a `run_in_background` tool call is still outstanding) into a new `bgtasks` OSC 7770 verb; `pty.rs` classifies it into `SilenceState::declared_background_work` (epoch-stamped, self-expiring, shaped like `completion_declared`) and exposes it as `SessionState.declared_background_work`, separate from the existing OS-heuristic `background_work`. The frontend's `effectiveActivityState` renders this as "Working" unconditionally, bypassing the idle-preserving carve-out `background_work` alone is subject to (the one protecting a Codex-left-running dev server). Machine-verified: `cargo nextest run -p tuic-hook` (new derivation/emission tests), `cargo nextest run -p tuicommander` (new OSC-dispatch/epoch-expiry/ladder tests, plus the full pre-existing `background_work`/`standby` suite unchanged), `pnpm exec tsc --noEmit` and `pnpm exec vitest run activitySnapshot` (new frontend cases). NOT machine-verified: the actual live badge rendering in a real `make dev` session.

- [ ] **[HUMAN]** After a `make dev` restart, in a hook-instrumented Claude Code session, ask Claude to run a long-lived command in the background (e.g. "run `sleep 60` in the background and then tell me when it's done"), and once it ends its turn while that's still running, confirm via `curl -k https://127.0.0.1:9876/sessions` that the session's `agent_state` is `"working"` and `declared_background_work` is `true`.
- [ ] **[HUMAN]** Confirm the sidebar/terminal-list badge for that session visually reads "Working," not "Idle," during that window — this is the actual user-facing fix; nothing above proves the real UI renders it correctly, only that the underlying data is correct.
- [ ] **[HUMAN]** Confirm the badge returns to normal (Idle, or a fresh Working for a new turn) once the backgrounded command finishes and Claude's next hook fire reports it, or once you submit a new prompt.
- [ ] **[HUMAN]** Confirm a session where Codex (or another agent) leaves an actual long-lived background process running (e.g. a dev server) still shows "Idle" once its own composer is ready — this fix must not have regressed that existing carve-out, since it only reads a NEW field (`declaredBackgroundWork`) that non-Claude agents never populate.

### Fix: `declared_background_work` survives a poll-driven reopen, incl. Agent-Teams teammates (2026-09-16, **Rust change — needs `make dev` restart**)

Found live: an "ai-usage" Claude Code session dispatched three Agent-Teams teammates
(`background_tasks[].type == "teammate"`, confirmed in the real `Stop` hook payload
via `.claude/hook-debug.log`) and ended its own turn. `declared_background_work` was
correctly set `true` by the `bgtasks` OSC verb, but read back `false` moments later
with no new user input submitted. Root cause: `reset_suggest_memory()` cleared
`declared_background_work` alongside `completion_declared`, but that method is also
called from `apply_working_evidence`'s "reopen a stale idle/completed turn on
renewed screen evidence" path — which fires every time the orchestrator's own screen
shows a spinner again (e.g. it waking up to poll its teammates), not just on a real
new turn. Fixed by splitting the clear into its own `SilenceState::reset_declared_background_work()`,
called only from genuine new-turn-submission sites. Machine-verified: new
`cargo nextest run -p tuicommander` unit tests (`reset_suggest_memory_no_longer_touches_declared_background_work`,
`reset_declared_background_work_clears_the_declaration`,
`test_claude_reopening_a_premature_stop_hook_preserves_declared_background_work`,
`a_genuine_new_turn_clears_declared_background_work`,
`submitted_input_with_no_detected_agent_type_still_clears_declared_background_work`
— covers `note_submitted_input_with_hook`'s other branch, and
`end_to_end_stop_hook_bgtasks_survives_a_subsequent_screen_poll` — a full-pipeline
replay through `ChunkProcessor::process_chunk` of the real OSC byte sequence
(`bgtasks=running` + `state=idle`, in the order the installed `tuic-hook` binary
actually emits them) followed by a real spinner-row repaint, not a direct call into
`apply_working_evidence`). Independently reviewed via `/code-review` (scoped to this
diff only) with no findings. Full `./scripts/check-gate.sh` and
`cargo nextest run --workspace --no-fail-fast` both clean except two confirmed
pre-existing, unrelated failures (`pty::tests::non_repo_cwd_gets_no_worktree_vars`,
`worktree::tests::run_setup_script_does_not_set_unknown_vars` — both reproduce
identically with this fix's changes fully `git stash`-ed out; see AGENTS.md's
"Known pre-existing test-environment leak" note).

- [ ] **[HUMAN]** After a `make dev` restart, reproduce the original scenario: ask Claude to dispatch a couple of Agent-Teams teammates on a task that takes at least a minute, wait for Claude's own turn to end (idle prompt), then trigger a poll (either wait for its own scheduled check-in, or nudge it to check status) without submitting a literal new prompt yourself. Confirm via `curl http://localhost:9876/sessions` that `declared_background_work` stays `true` (and the sidebar badge stays "Working") across that poll, not just immediately after the `Stop` hook fires.
- [ ] **[HUMAN]** Confirm the badge still correctly returns to "Idle" once you submit a genuine new prompt while `declared_background_work` was `true` — the fix must not have made the flag permanently sticky.

## MCP marker/peer-message framing rewrite (2026-08-26, **Rust change — needs `make dev` restart**)

- [ ] **[MANUAL/MCP]** After restart, spawn a real Task-tool subagent from a worktree-creation flow and confirm it does NOT emit `intent:`/`suggest:`/`ack` markers — the delegated hint tells it not to, but no automated check can observe a live model's actual behavior. This is the closest reachable proxy for "agents stop flagging markers as injection"; a full live-verification run needs a human or an MCP-driven agent session to actually converse with a fresh Claude Code / Codex / Gemini session.

## Background-color-erase reverse-video fix (2026-08-21/27, **Rust change — needs `make dev` restart**)

- [ ] **[VISUAL]** In a real terminal, run something that enters standout mode and prints a highlighted status line then scrolls (e.g. `tput smso; echo "status"; tput sgr0` in a loop that pushes it up through several more lines of plain output). Confirm the reverse bar stays scoped to the original highlighted line and does NOT reappear on later blank rows scrolled into view.
- [ ] **[VISUAL]** Separately, confirm `tput smso; tput el` (explicit erase-to-end-of-line under a reverse pen) STILL paints a highlighted bar to the edge — the original, still-intended behavior for explicit erases, which the row-recycling fix must not have regressed.

## Mouse-motion button reporting + local-selection hardening (2026-08-21)

- [ ] Manual click-through in a real Claude Code / Ink-based agent pane (the originally reported symptom): click-and-drag over agent output actually extends the app's own text selection instead of doing nothing, and double-click-drag / triple-click-drag extend by word/line respectively.
- [ ] Manual: Shift+drag (TUIC's local-selection fallback) started while an app has mouse tracking on, then the app toggles mouse tracking off/on mid-drag — selection must not flicker into forwarded-to-app mode or leave stray autoscroll/copy state.

## Scrollbar block/prompt marks respect their own settings independently (2026-08-22/25)

- [ ] **[MANUAL]** With a real terminal open and some command blocks/prompts recorded, flip "Show block marks" and "Show prompt marks" off/on in Settings > Terminal > Blocks with the timestamp overlay (Ctrl+Cmd) NOT held — ticks must appear/disappear immediately, not only while Ctrl+Cmd is held or on the next unrelated repaint.
- [ ] **[MANUAL]** Open a brand-new shell tab, run one short command (never scroll past one screen), confirm a block tick is visible on the scrollbar track immediately — the track must not be entirely absent as it was before this fix.

## Link click activation (2026-08-24)

- [x] `linkVisuals`/`shouldOpenOnClick`/`shouldSkipMouseReportForLink`/`shouldResolveLinkHoverOnMove`/`linkModifierEffectDecision` are pure functions with exhaustive per-mode/per-modifier unit coverage _(verified: src/components/Terminal/__tests__/canvasTerminalLinks.test.ts)_
- [x] `linkModifierHeld` store tracks Cmd (macOS) / Ctrl (Windows/Linux), resets on blur/visibilitychange _(verified: src/__tests__/stores/linkModifier.test.ts)_
- [ ] Manual: Settings > Terminal > "Open links on" = Click (default) — click a URL/file path in a live terminal, confirm it opens exactly as before this change.
- [ ] Manual: set to "⌘Click"/"Ctrl+Click" — confirm the link shows NO underline/pointer cursor until Cmd (macOS)/Ctrl (Win/Linux) is held, then reveals both and opens on modifier+click; releasing the modifier mid-hover removes the underline without needing to move the mouse.
- [ ] Manual: set to "Never" — confirm plain click never opens a link but the dashed underline stays visible and right-click's Open/Copy-link menu still works.
- [ ] Manual: with mouse-reporting on (e.g. inside `vim`/`htop`) and mode = "⌘Click", confirm modifier+left-click on a link opens it without the app receiving a stray click, and confirm a right-click on a link still reaches the Open/Copy-link menu (not swallowed as a mouse report) — regression guard for #57.

## Activity Dashboard keyboard navigation (2026-08-26, frontend only — Vite HMR is enough)

- [x] Arrow-key cursor movement, Return-to-activate, digit 1-9 jump-and-activate, capture-phase interception ahead of useKeyboardRedirect's PTY writer _(verified: src/__tests__/components/ActivityDashboard.test.tsx, "Keyboard navigation" suite)_
- [x] Idle rows sort by idleSince descending without disturbing the working-group anti-reshuffle spine _(verified: src/__tests__/utils/activitySnapshot.test.ts, `reconcileActivityOrder` suite)_
- [ ] Manual: open the Activity Dashboard with more rows than fit on screen, arrow down past the visible area, and confirm the list actually scrolls the selected row into view and the selected-row highlight is visually distinguishable — not observable in jsdom.
- [ ] Manual: repeat the arrow/Return/digit navigation in the **detached** Activity Dashboard panel window (`Cmd+Shift+A` panel detach) — the detached-window code path (separate mount, `props.embedded`) isn't exercised by any existing test.

## Main-worktree-on-non-main-branch icon color (2026-08-26, frontend only)

- [ ] **[MANUAL]** Switch the main checkout to a non-main branch (e.g. `git checkout -b tmp` in the main repo directory, not a linked worktree). Confirm the sidebar's branch icon turns the new accent/blue color, distinct from the main branch's yellow star and a separate linked worktree's green fork icon. Screenshot in both light and dark themes.
- [ ] **[MANUAL]** With that same off-main main-worktree row visible, select/focus it (and, separately, a different row) and confirm the new accent-blue icon color doesn't get visually confused with the sidebar's existing accent-colored active/selected-row indicator — i.e. an unselected row showing this color shouldn't read as "currently selected."

## Self-signed HTTPS fallback for remote/LAN access (2026-08-27, **Rust change — needs `make dev` restart**)

Remote access now defaults to HTTPS even without Tailscale: `provision_tls_config` falls back to
a self-signed cert (`src-tauri/src/selfsigned.rs`) covering `localhost` + current LAN IPs when
Tailscale HTTPS isn't active, plain `http://` on the same port 301-redirects to `https://` while
that fallback is serving, and Settings > Remote Access > Self-Signed HTTPS shows status/fingerprint/a
Regenerate action. Covered by 23 Rust unit tests (cert generation/caching/expiry/IP-coverage,
concurrent-regeneration serialization, the connect-URL scheme decision, `get_self_signed_cert_status`/
`regenerate_self_signed_cert` command wiring) and `cargo nextest run --workspace` (5269 tests),
but none of that exercises a real TLS handshake, a real browser's certificate-warning UX, or the
Settings panel's actual rendering — needs a live `make dev` restart to check:

- [ ] **[MANUAL]** With remote access enabled and no Tailscale, open the connect URL from Settings
  (or the QR code) on a phone/second device that has never connected before — confirm the browser
  shows a certificate warning exactly once, that clicking through (Advanced → Proceed) works, and
  that copy/paste (Clipboard API) works afterward, since that's the whole point of this feature.
- [ ] **[MANUAL]** With Tailscale HTTPS active, confirm the connect URL still uses
  `https://<fqdn>` (zero-warning) — the self-signed fallback must not activate when Tailscale is
  already serving HTTPS.
- [ ] **[MANUAL]** Navigate to the plain `http://<lan-ip>:<port>` URL directly (not the QR code)
  while the self-signed fallback is active — confirm it 301-redirects to `https://` rather than
  serving plain HTTP or erroring.
- [ ] **[MANUAL]** Open Settings > Remote Access > Self-Signed HTTPS — confirm the status line, expiry
  date, and SHA-256 fingerprint render correctly, and that the fingerprint shown matches what the
  browser's own certificate-details view reports for the same cert.
- [ ] **[MANUAL]** Click "Regenerate" — confirm status updates to a new fingerprint/expiry after
  the server restarts, and that a device that already trusted the old cert now sees a fresh
  warning (expected — it's a genuinely different cert).
- [ ] **[MANUAL]** With remote access + self-signed active, change networks (e.g. join a
  different Wi-Fi network) without restarting the app — within ~60s the background re-check loop
  should regenerate/hot-reload the cert to cover the new IP; confirm connecting from the new
  network doesn't hit a hostname-mismatch error. (Hard to force reliably — best-effort check.)

## Indicator customization — icon + animation pickers (2026-08-27, frontend only — Vite HMR is enough)

Phase 3 of the indicator customization work: expanded `src/indicators/icons.ts` from 5 to 18
curated shapes, added `IconPickerDialog`/`AnimationPickerDialog`, and wired `setIndicatorIcon`/
`setIndicatorAnimation` into the editable UI Legend. No Rust/config-schema change (rides the same
`indicator_overrides` field from Phase 2) — a running `make dev` picks this up via Vite HMR alone.

- [x] Opened a `make dev` debug instance, drove it live with Playwright: opened the icon picker
  on the "Busy" row (all 18 shapes rendered correctly, live, in a grid), picked "ring" — the row's
  icon button updated from a filled dot to a ring outline immediately. Opened the animation picker
  on the same row (6 options for a dot-shaped indicator: None/Pulse/Pulse (slow)/Blink/Breathe/Glow
  — correctly excludes Spin), picked "Breathe" — `getComputedStyle` confirmed
  `--ind-anim-terminal-busy` resolved to `tuic-breathe 3s ease-in-out infinite` with no reload.
  Confirmed the PR Conflict row's animation picker is narrower (4 options: None/Pulse/Pulse
  (slow)/Breathe — no Blink/Glow/Spin), and that the reset "×" appears for an icon+animation-only
  override with no color set. Cleaned up the test overrides via the HTTP config API afterward.
  _(verified live, 2026-08-27; screenshots in `.screenshots/phase3-icon-animation-pickers/`.)_
- [ ] **[MANUAL]** Set the "spin" animation on an icon that visually reads well rotating (e.g. the
  `arc` shape, meant to pair with it) and confirm it actually spins smoothly rather than jittering
  — not exercised in the automated pass above (only Breathe was checked end-to-end).
- [ ] **[MANUAL]** Confirm the icon picker's grid is keyboard-navigable / doesn't trap focus
  awkwardly, and that both new dialogs close on Escape like the existing `ColorPickerDialog`
  (they share `registerModal`, which should cover this, but wasn't explicitly re-checked).

## Indicator customization — persisted color overrides (2026-08-27, **Rust change — needs `make dev` restart**)

`AppConfig.indicator_overrides` (`src-tauri/src/config.rs`) is new — a `Vec<IndicatorOverride>`
holding per-indicator color/icon/animation overrides for the terminal status dots, tab types,
sidebar symbols, PR badges, and diff stats documented in Settings → Appearance → UI Legend, which
is now the editor for it (a swatch button + reset `×` per color-capable row, "Reset all
indicators" at the bottom). Covered by Rust round-trip/backward-compat/doc-parity tests and
frontend tests for the store actions, the sanitize-on-hydrate path, `apply.ts`'s DOM writes, and
the legend's editable-mode interactions (all passing against a fresh `cargo build`), but none of
that proves the *running* app picks up the new config field or applies overrides to the live UI —
needs a `make dev` restart to check:

- [x] Opened a `make dev` debug instance of this worktree, drove it live with Playwright (real
  browser, real clicks — the same class of check the MCP maccontrol escalation step calls for),
  clicked the "Busy" row's color swatch in Settings → Appearance → UI Legend, picked the "Red"
  preset, and read `getComputedStyle(document.documentElement).getPropertyValue("--ind-terminal-busy")`
  immediately after — resolved to `#FF6B6B` with no reload needed, and the row's own preview +
  reset "×" updated live in the same pass. _(verified live, 2026-08-27; screenshots in
  `.screenshots/phase2-indicator-color-editor/`.)_ **Not** separately caught: an actual busy
  terminal's tab dot rendering red mid-session — the test terminal had gone idle by the time of
  the check, so this confirms the CSS var updates live (the only thing the dot's color rule
  reads), not a red pixel in a screenshot. `sidebar branch icon` / nested-tab-dot sharing of the
  same override — not re-checked live; still open below.
- [ ] **[MANUAL]** Confirm an override is reflected in the sidebar branch icon for a busy
  branch (terminalStatus colors are deliberately shared between the tab dot and the branch icon —
  see the registry.ts comment on why) and in the branch's nested terminal-tab dot when Nested
  Terminal Tabs is on.
- [x] Restarted is stronger than tested: after setting the "Busy" override above, read
  `~/Library/Application Support/com.tuic.commander/config.json` directly off disk (not just a
  frontend reload) and found `"indicator_overrides": [{"id": "terminal.busy", "color":
  "#FF6B6B"}]` written there — proves it reaches disk, which is what any future launch reads.
  _(verified live, 2026-08-27.)_ Cleaned up immediately after via `PUT /config
  {"indicator_overrides":[]}` against the same running debug instance, and confirmed the file
  reverted to `"indicator_overrides": []` — this worktree's testing must never leave a stray
  override in Boss's real, shared config.json.
- [ ] **[MANUAL]** Click the reset "×" on a row with an active override *in the running app* and
  confirm the color reverts to the theme default live (the click-through interaction itself is
  unit-tested against the real store in `UiLegend.test.tsx`, but not yet exercised in a live
  browser session — the cleanup above used the HTTP config API directly, not the button).
- [ ] **[MANUAL]** Switch terminal themes while an override is active — confirm the override
  survives the theme switch (it re-applies from `themes.ts`'s `applyAppTheme` tail) rather than
  reverting to the new theme's unoverridden default.
- [ ] **[MANUAL]** Hand-edit `config.json`'s `indicator_overrides` array to add an invalid entry
  (e.g. `{"id": "not.real", "color": "#fff"}` or `{"id": "terminal.busy", "color":
  "javascript:alert(1)"}`), restart, and confirm the app doesn't crash and silently drops the bad
  entry rather than applying it — `sanitizeIndicatorOverrides` should catch it on hydrate.

## Git repo status indicators — rebasing/merging/conflicts (2026-08-27, **Rust change — needs `make dev` restart**)

Phase 5 of the indicator customization work: `worktree::operation_in_progress()` (new) resolves
a worktree's admin dir for **both** the main checkout (`.git` as a directory) and a linked
worktree (`.git` as a `gitdir:` file) — the plain `has_operation_in_progress` this replaces
only ever worked for linked worktrees, so an in-progress rebase/merge/etc in the main checkout
was silently invisible before this. `RepoStructure.in_progress_ops: Vec<{path, kind}>` (was
`in_progress_worktrees: Vec<String>`) now names *which* operation — `GitOpKind::Rebase | Merge |
CherryPick | Revert | Bisect` — instead of just flagging "busy". `RepoSection.tsx` renders a
distinct colored badge per kind (dropped the old `!isMain` gate, so the main checkout's row can
show one too), gated by Phase 4's `showGitState` toggle. Separately, `ChangesTab.tsx` now reads
`WorkingTreeStatus.conflicted` (a field the Rust side always returned but the frontend silently
dropped) and renders a conflicts banner + file list when non-empty — no new backend work needed
for that part. New `gitState` registry entries (`rebasing`/`merging`/`cherryPicking`/`reverting`/
`bisecting`/`conflicts`) back both. Deliberately **not** added: `detached`/`ahead`/`behind`/
`diverged`/`stashes` — nothing in the running app renders those yet (their would-be home,
`GitPanel/SyncRow.tsx`, is dead code per the plan's own "Known follow-up" note), and adding
inert legend rows for them would repeat the exact mistake Phase 1 fixed when it deleted the old
"Panels" section. Covered by 5 new Rust tests (marker→kind mapping, main-worktree detection,
`get_repo_structure_impl` regression) and Rust/frontend suites all green (`cargo nextest`: 5274
passed; vitest: 6606 passed — the plugins-submodule failure is pre-existing environment drift,
not this change).

- [x] Started a `make dev` debug instance (port 9877), created a scratch git repo (not one of
  Boss's real repos) with a diverged `main`/`feature` history, ran `git rebase main` from
  `feature` to produce a real content conflict, and queried the running debug instance directly:
  `GET /repo/structure?path=<scratch>` returned `in_progress_ops: [{"kind":"rebase","path":
  "<scratch>"}]` — the **main worktree** (no linked worktrees existed for this repo), proving
  the exact bug this phase fixes, live, against real git state. `GET
  /repo/working-tree-status?path=<scratch>` returned `conflicted: [{"path":"file.txt",
  "status":"UU",...}]`, proving the ChangesTab wiring's data source end-to-end.
  _(verified live, 2026-08-27.)_
- [ ] **[MANUAL]** The actual sidebar badge (`.gitOpBadge`/`.gitOpRebase` etc.) and the
  `ChangesTab.tsx` conflicts banner were **not** visually confirmed in a running browser this
  pass — see the incident note below. Confirm live: add the scratch repo above (or any repo mid-
  rebase-with-conflict) via Settings → "Add Repository", and check (a) the sidebar row shows a
  colored "Rebasing" pill, including on the **main** branch row (previously suppressed by the
  dropped `!isMain` gate), (b) the Changes tab shows the conflicts banner + `file.txt (UU)` row,
  (c) toggling "Show git repo status indicators" off in Settings → Appearance hides both.
- [ ] **[MANUAL]** Repeat live-verification for merge/cherry-pick/revert/bisect (only rebase was
  exercised above) — confirm each shows its own distinct color/label, not all defaulting to
  "Rebasing".

**Incident during this pass, for awareness:** driving the "Add Repository" dialog with
Playwright, a blind `input:not([type])` selector matched a **real terminal's hidden keyboard-
capture input** instead of the dialog's path field (the dialog's own input never received the
text, which is why "Add" stayed disabled) — the scratch repo's absolute path got typed into two
of Boss's real terminal sessions' shell prompts. Nothing was executed (no Enter was sent in
either case), and both were cleaned by sending Ctrl+U over the session `/write` HTTP endpoint and
confirming via `/terminal/lines` that each prompt returned to a bare `❯` with no residual text.
Given two near-misses now (this one, and Phase 4's accidental "Rename Branch" dialog open), any
future live UI verification pass should scope Playwright selectors much more narrowly — target
elements inside a specific dialog/modal container, never a bare `input`/text-label selector
against the whole page.

## Indicator customization — visibility toggles (2026-08-27, **Rust change — needs `make dev` restart**)

Phase 4 of the indicator customization work: four new `AppConfig` bools
(`show_diff_stats`, `show_pr_badges`, `show_git_state`, `tab_type_highlighting`, all
`#[serde(default = "default_true")]`), each rendered as a group-header toggle in the editable UI
Legend (Settings → Appearance). `show_diff_stats`/`show_pr_badges` gate `RepoSection.tsx`'s
sidebar badges; `tab_type_highlighting` sets `document.documentElement.dataset.tabTypeTint`,
neutralized by new `:global(:root[data-tab-type-tint="off"])` blocks in `TabBar.module.css` and
`PaneTree.css`; `show_git_state` ships inert (gates Phase 5's not-yet-built indicators). Covered
by Rust round-trip/backward-compat/doc-parity tests and frontend tests for the four setters, the
sidebar gating (`Sidebar.test.tsx`), the `data-tab-type-tint` effect (`useAppearanceSync.test.ts`),
and the legend's four group toggles (presence/absence by `editable`, correct setter wired, empty
`gitState` section) — full suite green (6593 passed; `ChangelogModal.test.tsx`'s known
pre-existing async-leak flake is the only "failed" file) and `./scripts/check-gate.sh` fully green
(rustfmt/clippy/5271 rust tests/tsc/biome all ✓). Live-verified against a `make dev` debug
instance on port 9877:

- [x] Opened Settings → Appearance and confirmed all four group-header toggles render in the
  right place — "Show tab type highlighting" under Tab Types, "Show PR status badges" under PR
  Status Badges, "Show git repo status indicators" under a new "Git Repo Status" section (heading
  + hint text, zero rows beneath it — correct, since Phase 5 hasn't populated it), "Show diff
  stats" under Diff Stats. _(verified live, 2026-08-27; screenshots in
  `.screenshots/phase4-visibility-toggles/`.)_
- [x] Toggled "Show PR status badges" and "Show diff stats" off together in the running app and
  confirmed the sidebar's diff-stat badges (`+N -N` next to `unity`, `main`, `icon-customization`,
  etc.) disappeared immediately with no reload, then toggled back on and confirmed they
  reappeared. _(verified live, 2026-08-27.)_ No branch in this sidebar currently has an open PR,
  so the PR-badge-specific disappearance wasn't independently visible in this pass — covered
  instead by `Sidebar.test.tsx`'s two new unit tests (`hides StatsBadge when showDiffStats is
  off...` / `hides the PR badge when showPrBadges is off...`), which do construct PR data.
- [x] Confirmed via the config HTTP API (`GET /config`) that `show_git_state` and the resolved
  state of all four toggles read back correctly after live UI interaction, and restored all four
  to `true` (plus `indicator_overrides: []`) before stopping the debug instance — this worktree's
  testing must never leave a toggle flipped in Boss's real, shared config.json.
- [ ] **[MANUAL]** The tab-type-tint neutralization itself (`TabBar.module.css`/`PaneTree.css`'s
  `data-tab-type-tint="off"` blocks removing the per-type background gradient and border-bottom,
  restoring the accent bar on the active tab) was not conclusively confirmed by screenshot in this
  pass — the only type-colored tab readily reachable without touching Boss's real repos more than
  necessary was a git-changes panel tab, where the tint is a subtle gradient that didn't show up
  clearly at screenshot resolution. The underlying mechanism (`data-tab-type-tint` attribute
  toggling) is unit-tested in `useAppearanceSync.test.ts`, and the CSS rules were manually
  re-reviewed line-by-line (catching and fixing a real bug — see the plan's Phase 4 notes on the
  `PaneTree.css` active-border-color mistake — before this pass), but a live pixel check on an
  actual `diffTab`/`editTab`/`mdTab` (not a panel tab) in both light and dark themes is still
  outstanding.
- [ ] **[MANUAL]** Confirm the active tab of a type-tinted tab still shows *some* visual
  indicator (the restored default accent bar) when tint is off, rather than looking identical to
  an inactive tab — this was a deliberate proactive fix (`TabBar.module.css`'s
  `.active::before { display: block; }` override) but not live-verified.

**Also noted during this verification pass**: driving the sidebar's branch rows with Playwright,
clicking directly on a branch name label is a double-click-to-rename target — it opened a real
"Rename Branch" dialog on Boss's actual `wip` worktree. No rename was confirmed (the dialog was
closed without clicking "Rename"), and the branch name was confirmed unchanged afterward, but
future live verification against the sidebar should click the diff-stat badge or row background
instead of the branch label text.

## Window geometry restore, shell tab restore, and scrollback restore (2026-08-26, **Rust change — needs `make dev` restart**)

Three related features. `main` is now denylisted from `tauri-plugin-window-state` and owns its own size/position/maximized/fullscreen persistence (`window-geometry.json`, `src-tauri/src/window_geometry.rs`) with a measure-and-correct restore step working around the plugin's `set_size`/`outer_size` inner/outer drift under `titleBarStyle: Overlay`. `createBranchSelectionCoordinator.ts` now restores plain shell tabs (not just agent tabs) on branch select, behind Settings → Terminal → "Restore open terminals on launch" (default on). A new opt-in "Save terminal scrollback" setting (default off) persists each terminal's recent output (`scrollback_store.rs`) and replays it above a fresh prompt on restore.

- [ ] **[MANUAL]** `make dev`, resize and move the window, quit, relaunch. Repeat **three
  times** in a row and confirm the size is byte-identical every launch — this is the exact
  regression the `SIZE` flag was excluded from the plugin to avoid (progressive shrink), so
  a single restart is not enough evidence either way.
- [ ] **[MANUAL]** Maximize the window, quit, relaunch — comes back maximized. Un-maximize
  it — reveals the pre-maximize size, not the screen size.
- [ ] **[MANUAL]** Move the window to a second display, quit, disconnect that display,
  relaunch — the window recenters onto a live monitor instead of restoring off-screen.
- [ ] **[MANUAL]** Toggle "Restore window size and position on launch" off in Settings →
  General, resize the window, restart — comes back at the 1200×800 default instead.
- [ ] **[MANUAL]** Open two plain shells and one agent tab in a repo branch, quit, relaunch,
  reselect that branch — all three tabs return; the shells show a live prompt in their saved
  cwd; the agent tab shows its resume banner. Toggle "Restore open terminals on launch" off,
  repeat — only the agent tab comes back.
- [ ] **[MANUAL]** Enable "Save terminal scrollback", run a colorful command (e.g. `ls -la
  --color`) and a longer one that scrolls, restart, reselect the branch — the prior output
  appears above a dim "restored from previous session" separator with colors/bold intact,
  is scrollable and searchable, and the live prompt below it still works normally.
- [ ] **[MANUAL]** With scrollback saving on, set "Scrollback lines to save" low (e.g. 100),
  generate more output than that in a terminal, restart — confirm only the cap's worth comes
  back. Click "Clear saved scrollback" and confirm a subsequent restart shows no restored
  history for any tab.
- [ ] **[MANUAL]** Confirm `<config dir>/scrollback/*.json` files are not world-readable
  (owner-only permissions) and that the directory doesn't exist at all when scrollback
  saving has never been turned on.

**Follow-up fix (2026-08-27, `size-restore` branch, Rust change — needs `make dev`
restart):** the "resize/restart three times" item above is exactly what should have
caught a real bug that shipped with the original feature — a live install's
`window-geometry.json` was found corrupted to `width: 4944, height: 2368` on a
`3456x2234` physical display (window far wider than the screen, off the edge). Root
cause: `apply_window_geometry`'s measure-and-correct step (`corrected_size`) could act
on a stale pre-resize `outer_size()` read (`wait_for_geometry_to_settle` returning
early on a compositor that hadn't started applying the resize yet), computing a wildly
wrong "correction" that then got persisted and compounded larger on each subsequent
restart. Fixed with a plausibility bound (`is_frame_offset_plausible`, skips the
correction if the observed/requested gap exceeds 256px) and a missing safety-net check
(`window_geometry_fix` now also resets geometry wider/taller than every monitor
combined, not just too-small or off-screen-by-center — the corrupted window's *center*
was still on-screen, which is why the old check never caught it).

- [ ] **[MANUAL]** After restart on this fix: resize the window noticeably (e.g. drag
  much wider than default), quit, relaunch — confirm the restored size matches. Repeat
  several times in a row, including at least once right after a fresh `make dev` start
  (cold start is when the original race was most likely to hit) — this is the scenario
  that produced the real corrupted value above.
- [ ] **[MANUAL]** With TUICommander closed, manually edit `window-geometry.json` in the
  app's config dir and set `width`/`height` larger than your actual display (e.g. 2x),
  then launch — confirm the app resets to the ~1200×800 fallback centered on-screen
  instead of opening oversized/off-screen.

## Create Worktree dialog: searchable base-ref picker + keyboard nav (2026-08-26, frontend only)

The "Start from" base-ref dropdown gained a search box, `↑`/`↓`/`Enter` navigation, and
Local/Remote section headers that arrows cross transparently. The existing-branch list
below the name input gained the same `↑`/`↓`/`Enter` navigation plus match highlighting.
The last base ref chosen in the dialog is now remembered per repo for the session (not
persisted — forgotten on restart) and preselected next time that repo's dialog opens.
Vite reloads this without a restart.

- [ ] **[MANUAL]** Open Create Worktree on a repo with several local + remote branches.
  Type into the "Start from" search box — confirm it narrows both groups, `↑`/`↓` moves a
  visible highlight across the Local/Remote boundary without getting stuck, and `Enter`
  picks the highlighted ref and closes the list. Confirm one `Escape` closes just the
  dropdown and a second closes the whole dialog — and that Escape never reaches the
  terminal underneath (types `ESC` into the active session) either time.
- [ ] **[MANUAL]** With the dropdown closed, confirm `Enter` (or `Space`) while the
  "Start from" trigger button has focus opens the list and does **not** submit the dialog.
- [ ] **[MANUAL]** In the branch list, confirm `↑`/`↓` skips rows tagged "(has worktree)",
  and that typing a fragment highlights the matching substring in each row. Confirm `Enter`
  with no cursor still creates using the typed text (unchanged behavior), and `Enter` with
  a highlighted row populates the input instead of submitting (a second `Enter` submits).
- [ ] **[MANUAL]** Create a worktree off a non-default branch, then reopen the dialog for
  the same repo — the "Start from" trigger should show that branch preselected. Reopen it
  for a *different* repo — it should show that other repo's own default, not the first
  repo's remembered choice. Restart the app and confirm the memory is gone (session-only).
- [ ] **[MANUAL]** Screenshot the dropdown's search box and the branch list's highlight
  styling in both light and dark themes — check the sticky search row, the keyboard-cursor
  color versus the "currently chosen" color (they're deliberately different tokens), and
  that highlighted/`<mark>`-wrapped text stays legible in both themes.

## Worktree removal "in use" confirmation (2026-08-26, **Rust change — needs `make dev` restart** for the log persistence)

Ported onto main's removal model (lifecycle confirmation + fingerprint; no backend
live-session refusal, no session-lifetime git locks — see rebase-log). Verify against a
throwaway repo on the worktree build (`:9877`, never Boss's real repos):

- With a terminal open in a worktree, click Delete (sidebar `×` or Worktree Manager).
  After the usual removal confirmation, the "in use" confirmation should appear naming
  the attached terminal(s), BEFORE the terminal closes, with Enter = Cancel — cancel it
  and confirm nothing closed. Confirm it and verify the terminal closes, then the
  worktree is removed as before.
- The "Worktree is locked by an agent" Force Remove prompt: Enter cancels.
- Select several worktrees in the Worktree Manager, including one with an attached
  terminal, and batch-delete: the unused ones go first, the busy one last with its own
  confirmation.
- `curl :9877/logs` (or the ErrorLogPanel) after clicking through a removal, then check
  the day's `tuic.log.*` in the instance's logs dir for the same `git`-sourced entry —
  this used to live only in the 1000-entry ring buffer.

## Tri-state (On / Use global / Off) settings + repo-settings persistence fix (2026-08-25)

- [x] `TriStateToggle` renders a three-segment `role="radiogroup"`/`role="radio"` control (Off / Global / On), marks the segment matching `value` as `aria-checked`, clicking a segment calls `onChange` with that segment's value, Left/Right (and Up/Down) arrows move-and-select with clamping at the ends, the "Use global default (…)" hint shows only while `value === null`, and it supports custom on/off labels _(verified: `src/__tests__/components/shared/TriStateToggle.test.tsx`, 8/8 pass)_
- [x] **Review fix** — arrow-key navigation now moves DOM focus to the newly-selected segment, not just the logical selection. Without this, the `:focus-visible` outline stayed on the segment you started on while the highlighted/checked segment jumped elsewhere — a real desync between what looked focused and what was selected for keyboard users _(verified: `TriStateToggle.test.tsx` "moves DOM focus to the newly selected segment on arrow key")_
- [x] **Review fix, dead code removed** — `SettingTriToggle` (a `SettingFields.tsx` wrapper) had zero real call sites; every actual tri-state row ended up using `TriStateToggle` directly instead (wrapped in whichever group class its own tab already used). Deleted it rather than leaving a second, unused way to render the control that a future developer could mistake for the wired-up one.
- [x] **Review fix** — `AgentsTab.tsx`'s per-agent "Show suggested follow-ups" tri-state override persisted a value nothing read: `Terminal.tsx`'s `"suggest"` case gated only on the global `settingsStore.state.suggestFollowups`, never `agentConfigsStore.getSuggestFollowups(agentType)` — unlike its sibling `intent_tab_title`, which was already correctly AND-combined via `perAgentEnabled` at `Terminal.tsx:459`. Fixed by resolving `perAgentOverride ?? globalValue` before gating. Also removed a redundant outer `<Show when={settingsStore.state.suggestFollowups}>` around `<SuggestOverlayContainer />` in `TerminalArea.tsx` — it double-gated the same decision one level up and specifically blocked the "per-agent on while global off" direction, since the container already renders nothing when there's nothing to show. `Terminal.tsx`/`TerminalArea.tsx` are excluded from the coverage floor by existing project convention (`vitest.config.ts`: "Untestable without runtime: Tauri APIs, xterm.js, complex Tauri IPC") — same as the untested `intent_tab_title` sibling logic it now matches.
- [x] **Review coverage gap, closed** — `mcpUpstreams` (a per-repo MCP-upstream-server allowlist; `null` = no restriction) round-trips through `toWire`/`fromWire` on both save and hydrate. It's the one `RepoSettings` field with real security consequences — a silent persistence failure here fails open to "no restriction" — and had no dedicated assertion despite being covered generically by the same fix _(verified: `repoSettings.test.ts`, 2 new cases)_
- [x] `RepoWorktreeTab`'s nine inheritable booleans (copy ignored/untracked files, prompt on create, delete branch on remove, auto-archive merged, the three PR-visibility filters, and — macOS only — Cmd+1-9 terminal hotkeys) resolve against the right global default, selecting On/Off overrides it, and selecting "Global" writes `null` back _(verified: `src/__tests__/components/SettingsPanel/RepoWorktreeTab.test.tsx`, 14/14 pass)_
- [x] `AgentsTab`'s per-agent `intent_tab_title`/`suggest_followups` overrides bridge the tri-state's `null` to the store's `undefined` inherit sentinel correctly in both directions _(verified: `src/__tests__/components/SettingsPanel/AgentsTab.perAgentOverrides.test.tsx`, 3/3 pass)_
- [x] **Persistence bug fix** — `repoSettings.ts` now converts explicitly to/from the Rust `RepoSettingsEntry`'s snake_case wire shape instead of posting the camelCase store object verbatim (which `#[serde(default)]` silently dropped in full — confirmed against the live `repo-settings.json`, every field but `path` was empty). Save/update/reset/hydrate/localStorage-migration wire payloads are asserted directly against `mockInvoke` calls _(verified: `src/__tests__/stores/repoSettings.test.ts`, 33/33 pass)_; the new `RepoSettingsEntry` fields (`pr_hide_drafts`, `pr_hide_conflicting`, `pr_hide_ci_failing`, `terminal_meta_hotkeys`) and a full frontend-shaped JSON payload parse correctly on the Rust side _(verified: `src-tauri/src/config.rs` `repo_settings_*` tests, 6/6 pass)_. The global `pr_hide_drafts`/`pr_hide_conflicting`/`pr_hide_ci_failing` toggles had no backing `AppConfig` field at all and are now real fields, round-tripped in `app_config_round_trip` and defaulted in `app_config_serde_default_for_new_fields`.
- [x] **Made the "silently dropped key" failure mode observable** — `RepoSettingsEntry` now has a flattened `extra: HashMap<String, serde_json::Value>` catch-all (never re-serialized) that captures any JSON key not matching a known field instead of `#[serde(default)]` dropping it with zero trace; `load_repo_settings()` logs `tracing::warn!(repo_path, unknown_keys, ...)` whenever `extra` is non-empty. A leftover-camelCase payload lands in `extra` rather than vanishing, and `extra` itself never gets written back into `repo-settings.json` _(verified: `src-tauri/src/config.rs` `repo_settings_entry_captures_unrecognized_keys_instead_of_silently_dropping_them` + `repo_settings_entry_extra_is_never_serialized_back`)_
- [x] `agentConfigsStore.setIntentTabTitle`/`setSuggestFollowups` set/persist/reset-to-`undefined` correctly, and `isAutoRetryEnabled`/`syncHookInstrumentation` (the latter mirrors state without saving to disk) behave as documented — none of this had test coverage before _(verified: `src/__tests__/stores/agentConfigs.test.ts`, 27/27 pass)_
- [ ] **Rust restart required** — `pr_hide_drafts`/`pr_hide_conflicting`/`pr_hide_ci_failing` are new `AppConfig` fields and `RepoSettingsEntry` gained four fields; per `AGENTS.md`, `make dev`'s `--no-watch` backend won't pick these up without a manual restart.
- [ ] Manual, after the restart above: in a running app, open Settings → a repo → set "Hide Draft PRs" to On, "Copy ignored files" to Off, leave "Auto-archive merged" on "Use global". Read `<config dir>/repo-settings.json` directly and confirm `pr_hide_drafts: true`, `copy_ignored_files: false`, `auto_archive_merged: null`, and a non-empty `display_name` — restart the app and confirm the three segments come back in the same positions. Use a throwaway repo entry, not a real one — debug and release builds share this file (`AGENTS.md`).
- [ ] Manual: with "Hide Draft PRs" On for one repo and the global setting Off, confirm drafts disappear from that repo's PR list only, and reappear when the row is set back to "Use global".
- [ ] Manual: screenshot the tri-state control in both light and dark themes; keyboard-only pass (Tab into the group, Left/Right to move, screen reader announces role + checked state).
- [ ] Manual: hand-edit one repo's entry in `<config dir>/repo-settings.json` to add a stray key (e.g. `"displayName": "leftover"` alongside the real `display_name`), restart the app, and check `GET http://localhost:9876/logs` for a warning naming that repo path and `displayName` as an unrecognized key. Confirm the stray key does not reappear in the file after the app saves that repo's settings again.

## Smart Selection (2026-08-24)

- [x] `findSmartMatch` scores every enabled rule's matches spanning the click offset by `precision × matchLength`, highest wins; a match not spanning the offset is rejected; an invalid rule regex is skipped, not thrown _(verified: `src/components/Terminal/__tests__/smartSelection.test.ts`, 17/17 pass)_
- [x] Every shipped default rule (iTerm2's built-in set + git SHA/`file:line:col`/semver/IPv4/IPv6/UUID/issue-key/`#NNN` extras) matches a representative line, has no duplicate ids, and at most one default action per rule _(verified: `src/components/Terminal/__tests__/smartSelectionDefaults.test.ts`, 25/25 pass)_
- [x] `createWordBoundaryResolver`'s "characters" mode with the default separator string is behaviorally identical to the pre-existing `wordBoundsAt`; "regex" mode's `https://` alternate example (the original feature request) joins the scheme onto an adjacent word run _(verified: `src/components/Terminal/__tests__/canvasTerminalSelection.test.ts`, 40/40 pass)_
- [x] `runSmartSelectionAction` dispatches each of the 7 action kinds to the right injected dep with the substituted `\0`-`\9`/`\d`/`\u`/`\h` parameter; `open_url` refuses a non-http/https/mailto scheme _(verified: `src/components/Terminal/__tests__/smartSelectionActions.test.ts`, 10/10 pass)_
- [x] End-to-end through a real mount: double-click (smart vs. word mode), quad-click (always smart, falls back to whole-line when nothing matches), Alt+double-click running a matched rule's default action (and doing nothing when Alt isn't held, or the rule has no default action), and the right-click menu surfacing a rule's actions (deduplicated against the link-detection Open/Copy-link pair when both apply to the same span) _(verified: `src/components/Terminal/__tests__/canvasTerminalGestures.pin.test.ts` + `canvasTerminalSmartSelection.mount.test.ts`, 34/34 pass combined)_
- [x] Settings > Selection tab: every control persists via `settingsStore`, the rule editor add/remove/edit round-trips through `resolveSmartSelectionRules`, marking one action default clears any other in the same rule _(verified: `src/__tests__/components/SettingsPanel/SelectionTab.test.tsx`, 15/15 pass)_
- [ ] Manual click-through in a real terminal: double-click a URL like `https://github.com/foo/bar.git` selects the whole thing (not just `https`); double-click a git log's short SHA selects the whole SHA; Option/Alt+double-click a git SHA runs "Show commit" (`git show <sha>`) in the terminal; quad-click still selects the whole line when nothing matches.
- [ ] Manual: Settings > Selection > switch "Word boundaries" to "Regular expression", add `https://` as the pattern, confirm a double-click on a bare word elsewhere still works (falls back to the plain alnum/underscore word class) and a double-click on a URL's host now includes the scheme.
- [ ] Manual: right-click a matched rule (e.g. a UUID or semver in real output) and confirm the Copy action actually puts the right text on the system clipboard (`Cmd+V` into another app) — the mount-harness tests assert the Tauri invoke call shape, not a real OS clipboard round-trip.
- [ ] Manual: `run_command_new_terminal` action — trigger it via a custom rule, confirm a new terminal tab appears, is focused, and the command types + submits once the shell is idle.
- [ ] Manual: `ask_ai` action opens the AI Chat panel focused on the right session with the substituted text as the outgoing message.
- [ ] **Rust restart required** — `smart_selection_enabled`/`double_click_action`/`word_selection_mode`/`word_separators`/`word_selection_regex`/`smart_selection_rules` are new `AppConfig` fields; per `AGENTS.md`, `make dev`'s `--no-watch` backend won't pick up `config.rs` changes without a manual restart.

## Smart Prompts import & export (2026-08-24)

- [x] Export scope selection (`selectForExport`): "all" returns every prompt, "custom" returns only non-built-in prompts, "modified" returns changed built-ins plus every custom prompt, and comparison ignores placement order and volatile fields (`createdAt`/`updatedAt`/`lastUsed`/`builtInVersion`) _(verified: `src/__tests__/utils/promptExport.test.ts`, 22/22 pass)_
- [x] Import parsing rejects non-JSON, a mismatched `kind`, and a newer `schemaVersion`; sanitizes an invalid `executionMode`/`injectTarget`/`preferredAgent` instead of rejecting the whole file; drops entries missing `id`/`name`/`content` with a warning _(verified: `src/__tests__/utils/promptExport.test.ts`, `src/__tests__/utils/promptSanitize.test.ts`)_
- [x] `importPrompts()` batch-upserts in a single debounced save, forces `enabled: false` on imported `shell`/`api` prompts and reports their names, preserves the existing `createdAt` on a conflicting overwrite, and clears a forged `builtIn: true` on an id that isn't an actual built-in _(verified: `src/__tests__/stores/promptLibrary.test.ts` "importPrompts()" suite)_
- [x] `hydrate()` behavior is unchanged after extracting its validation into `sanitizePrompt` — same warnings, same `tab-context` migration _(verified: existing `hydrate()` tests in `src/__tests__/stores/promptLibrary.test.ts` still pass unmodified)_
- [x] `PromptImportDialog` renders one row per candidate with NEW/CONFLICT badges, flags shell/api rows needing review, defaults to everything selected, and the All/None/New-only bulk controls and Import/Cancel buttons behave correctly _(verified: `src/__tests__/components/PromptImportDialog.test.tsx`, 8/8 pass)_
- [x] `downloadPromptExport`/`pickPromptImportFile` build the right Blob/anchor and file input, and resolve the picked file's text (or `null` when nothing was selected) _(verified: `src/__tests__/utils/promptTransfer.test.ts`)_
- [ ] Manual click-through in **Settings > Smart Prompts**: Export "Everything" downloads a `.json` with all 27+ built-ins and any customs, no `lastUsed` keys; edit one built-in and disable a second, then Export "Modified only" includes exactly those two plus all customs; Export "Custom only" has no `builtIn: true` entries.
- [ ] Re-import the "Everything" file: every row shows CONFLICT; uncheck some rows and confirm only the checked ones are overwritten. Import into a profile with no `prompt-library.json`: every row shows NEW.
- [ ] Hand-craft a file with a `shell`-mode prompt and import it: it lands disabled, and the toast/warning names it. Confirm a modified built-in still offers "Reset to default" after import.
- [ ] Remote/browser mode (`http://127.0.0.1:9877` per `AGENTS.md`'s test-instance rule): Export downloads via the browser's normal download flow and Import opens the native OS file picker, with the same behavior as desktop.

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

## Command Block System (2026-05-20)

- [ ] Press `Cmd+Shift+.` once with the viewport centered on a block: confirm only ONE block toggles fold (not two, not the wrong one) — this is the double-firing bug `e36c1ae4` fixed by removing `CanvasTerminal`'s duplicate inline handler.
- [ ] Windows/Linux: `Ctrl+Shift+.` folds a block instead of typing a literal `.` into the shell prompt (regression guard for the raw-keystroke passthrough exclusion `CHANGELOG.md`'s `Cmd+Shift+.` entry describes).
- [ ] `Cmd+Shift+B` toggles block-scoped search through the SAME toolbar-button state (`TerminalSearchRef.toggleBlockScope`) — opening via toolbar then pressing `Cmd+Shift+B` should turn scoping off, not open a second/duplicate bar.
- [ ] Settings > Terminal > Blocks → turn OFF "Enable block folding", then press `Cmd+Shift+.`: nothing happens (verifies `toggleBlockFoldAtViewport`'s `settingsStore.state.blockFoldingEnabled` gate at `CanvasTerminal.tsx:3320`).
- [ ] **[MANUAL]** With a live Claude Code session (hook-instrumented), run a turn that makes 3+ tool calls: confirm exactly ONE command block spans the whole turn (not one block per tool call) and its label shows the submitted prompt text when the prompt is 10+ words.
- [ ] **[MANUAL]** End a turn via a genuine `StopFailure` (not just `PostToolUseFailure`): confirm the tab still goes idle (not stuck busy) AND the block is red-ticked.

## Repository saves survive a concurrent diffstat change

Requires a `make dev` restart — the change is in `src-tauri/src/config.rs`.

- [ ] With two windows open on the same config, work in a repo so its diff counts _(NOT VERIFIED 2026-09-29: partial — Emulated second window through HTTP PUT deltas: rename, add repo, group, active-repo persisted; UI rename persisted (repo-uirn) while repo diffstat was changing; only 2 'Failed to fetch' error logs, no 'repository configuration conflict'. Sidebar reorder not exercised.)_
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
- [ ] Rename the same repo in two windows without reloading either: this must _(NOT VERIFIED 2026-09-29: blocked — Needs a stale second window: backend broadcast converges the client within ~3s, so a stale-baseline same-repo rename race could not be produced in one tab.)_
  STILL conflict. The exemption covers counts, not intent.
- [x] The sidebar diff counts keep updating — the exemption must not make them _(verified 2026-09-29: Added file with git add -N in fx/repo: sidebar main row showed '+1 -0 Tracked line changes' within 5s and disk additions=1.)_
  unwritable.

## A parked tab names the repo to register

Frontend only; Vite HMR picks it up.

- [x] Have an agent spawn a child via MCP in a worktree of a repo that is NOT _(verified 2026-09-29: MCP session create cwd=fx/agb/ur__wt/b1 (unregistered repo ur): toast 'Tab parked outside your repos | Nothing claims "<...>/fx/agb"... register the repo' + Register button (wording differs from item). GET /logs warn names 'register ".../fx/agb/ur"'. Plain session, not agent spawn.)_
  registered (`<repo>__wt/<branch>`). A toast appears: *Tab parked in the wrong
  repo — nothing claims "<repo root>"*. The log warning names the same path.
- [x] Reconnecting many sessions from that one repo raises ONE toast, not one _(verified 2026-09-29: 4 MCP sessions in ur__wt/b1 (8 warn log lines) -> exactly 1 toast element in DOM; after page reload (sessions re-adopted) again 1 toast. Caveat: sessions were later auto-closed by the client reload.)_
  per session.
- [ ] Register that repo: the parked tab moves to it by itself, and the active _(NOT VERIFIED 2026-09-29: partial — Clicked toast Register: repo 'ur' appeared in sidebar (rows main,b1), toast gone. Header switched repo/agbw2 -> ur/main (maybe Register's own activation, cannot separate). Parked tab moving not observed: the parked sessions had been closed after reload.)_
  repo does NOT change under you while the tab moves.

## An exited tab says so instead of going black

Frontend only; Vite HMR picks it up. Already verified live in the running dev
build: `term-100` ("GitHub state", exited agent in a deleted worktree) renders
one `terminal-exited-notice`, the other 15 tabs render none. What is left is the
visual check.

- [ ] Click an exited tab (grey dot). The panel shows a centred, muted *Session _(NOT VERIFIED 2026-09-29: blocked — Web-created tabs never got a PTY (GET /sessions unchanged: 3; tabs 'main N'), and MCP-created remote sessions are removed from the UI on exit; no exited (grey dot) tab could be produced.)_
  ended* / *The process exited and its output was released. Close this tab to
  remove it.* — not a black void.
- [ ] Open a brand-new terminal: the notice must NOT flash before the PTY _(NOT VERIFIED 2026-09-29: partial — Clicked New Tab '+' and sampled DOM every 50ms for 5s (97 samples): .exitedTitle count 0, no 'Session ended' text. The exited-tab half (notice appears) not observed.)_
  spawns. A new tab also has a null sessionId; only `shellState === "exited"`
  may show the notice.
- [ ] Let an agent finish in a background tab: the tab keeps its grey dot and _(NOT VERIFIED 2026-09-29: blocked — No agent CLI; exited tabs cannot be produced in this web session (remote-session tabs are removed on exit).)_
  its name, and the panel shows the notice when you switch to it.

## Repository saves converge across windows

**Rust change — a `make dev` restart (or `make build`) is required.** The
frontend half is HMR-only, but `repositories-changed` is emitted by the backend,
so nothing happens until the Rust process is rebuilt.

- [ ] Open the desktop app and a browser at `http://localhost:9876/`. Rename a _(NOT VERIFIED 2026-09-29: partial — Second client emulated with HTTP PUT /config/repositories delta (rename repo ur -> ur-renamed): browser tab sidebar showed the new name within 3s with no reload. Reverse direction (UI rename -> other client) only checked as disk write (config displayName 'repo-uirn'). No desktop client.)_
  repo in the browser. The desktop sidebar shows the new name without a reload,
  and vice versa.
- [x] Add a repo in one client: it appears in the other, in the right sidebar _(verified 2026-09-29: HTTP PUT delta adding repo ur2 (repos + repoOrder append): browser sidebar chip UR2 appeared after 3s at the end, matching repoOrder position. Emulated other client via API; no second browser tab.)_
  position.
- [x] Remove a repo with no terminals open in one client: it disappears from the _(verified 2026-09-29: HTTP PUT delta removing repo ur2 (no terminals): sidebar chip UR2 disappeared within 3s; config no longer lists it.)_
  other.
- [x] Remove a repo that has open terminals in the other client: that client _(verified 2026-09-29: Client had tabs main 1..3 on repo ur; HTTP PUT removed ur from disk: client kept header ur-renamed/main, the sidebar chip/rows (main, b1) and all 3 tabs. Other client emulated via API.)_
  KEEPS the repo and its tabs (they must not be orphaned), and the repo is still
  visible in the sidebar — not just present in memory.
- [ ] Remove a worktree/branch in one client while the other has a terminal open _(NOT VERIFIED 2026-09-29: partial — DELETE /worktrees/agbw2 (deleteBranch) while browser had tab 'agbw2 1': log 'Worktree removed - pruned sidebar row agbw2'; row AND tab both disappeared (item expects both to stay). Disk-only delta removing ur main workspace: row+3 tabs stayed. Unclear if agbw2 tab had a live PTY.)_
  on that exact branch: the branch row and its tabs stay in the other client.
- [x] Rename a repo in one client while the other has that same repo open and _(verified 2026-09-29: While a script appended lines to fx/repo (git add -N, diffstat moving), HTTP PUT delta renamed repo -> repo-api: sidebar shows REPO-API and disk displayName repo-api with additions 6 written by the client; no error log. Other client emulated via API.)_
  actively changing (edit a file so the diffstat moves): the rename still lands.
- [x] Group a repo in one client, then delete the group there: the other client _(verified 2026-09-29: HTTP PUT group grpagb (AGBGRP) holding ur: sidebar groupSection AGBGRP appeared; then deleting group + moving ur to repoOrder: groupSection gone, ur listed ungrouped, no empty accordion. Emulated other client.)_
  loses the group and shows the repo ungrouped, with no empty accordion left.
- [x] Switch the active repo in one client: the other client's focus does NOT _(verified 2026-09-29: HTTP PUT changed disk activeRepoPath ur -> fx/repo -> ur2 (emulated other client). The browser's store (__TUIC__.store('repositories').activeRepoPath) stayed on '.../fx/agb/ur' while disk said ur2, and sidebar Fetch acted on the client's own active repo. Focus did not move.)_
  move.
- [x] After any of the above, rename a *different* repo in the client that _(verified 2026-09-29: After API changes (rename/add/remove ur, ur2), renamed repo in the receiving UI via Repo Settings 'Custom name...': disk displayName repo-uirn, ur removal NOT reverted (disk has only fx/repo then), no new 'Repository changes were not saved' with conflict (only 2 older 'Failed to fetch' entries).)_
  received the change. `GET http://localhost:9876/logs?level=error` shows no
  `Repository changes were not saved`, and the first client's change is still
  there — the receiver must not have reverted it.
- [ ] Toggle something that writes no change (re-save the same value): the other _(NOT VERIFIED 2026-09-29: partial — 3 no-op PUT deltas (before==after): /logs grew by 1 entry ('pty Tombstone reaped'), no repositories/load_repositories entries. No log line names load_repositories anyway, so client re-read is not directly observable.)_
  client must not re-read. `GET /logs` shows no burst of `load_repositories`.

## Auto-retry on Claude Code's prose 5xx message

**Rust change — a `make dev` restart (or `make build`) is required.** The parser
runs in the backend, so nothing changes until the Rust process is rebuilt.

**Precondition:** Settings → Agents → Claude → enable auto-retry. It is
`auto_retry_on_error`, default `false`, and it is currently unset in
`config.json`, so with it off you only get the red error badge and no retry.

- [ ] Reach a real `API Error: 500 Internal server error. This is a server-side _(NOT VERIFIED 2026-09-29: Needs a real Claude Code API 500 server-side error message)_
  issue…` in a Claude tab. `GET http://localhost:9876/logs` shows
  `[ApiError] … pattern=claude-server-error-friendly kind=server` followed by
  `[AutoRetry] claude: attempt 1/3 in 5s`.
- [ ] The tab does NOT play the error sound and does NOT show the red awaiting _(NOT VERIFIED 2026-09-29: Needs real Claude prose 5xx error message and retry timing.)_
  badge while a retry is pending — only after the 3rd attempt is exhausted.
- [ ] `continue` is injected after 5s and the turn resumes. _(NOT VERIFIED 2026-09-29: Needs real Claude Code emitting its prose 5xx error and auto-continue)_
- [ ] With auto-retry disabled for Claude, the same error sets the red badge _(NOTE 2026-09-29: partial evidence only — Parser case for Claude's prose 5xx message at output_parser.rs:3843; disabled-retry branch is code inspection of auto-retry setting (verify in useAgentPolling/retry handler).)_
  immediately and injects nothing.
- [ ] The message wraps across terminal rows (narrow the window before it _(NOTE 2026-09-29: partial evidence only — Detection covered by pty tests using API_ERROR fixture (pty/tests.rs:7310); wrapped-row case needs a real narrow agent tab if wanted.)_
  fires): detection still happens — the pattern anchors on `API Error: 5xx`.
- [ ] A 429/overload (`API Error: 529` or "temporarily limiting requests") is _(NOTE 2026-09-29: partial evidence only — Detection in tuic-terminal/src/output_parser.rs:497-502 (rate_limit/overloaded/'limiting requests'), classify_error in tuic-core error_classification.rs:27; needs test cite or fixture replay.)_
  still logged as a rate limit, not as a server error, and injects nothing.

## Usage ticker follows the agent in the terminal (Claude / Codex / Grok)

**The Rust restart is complete.** The active debug backend on :9876 was verified
on 2026-09-19 with the Codex App Server and Grok ACP integrations loaded.

**Precondition:** Settings → Agents → the Claude Usage toggle must stay enabled;
it now drives all supported usage providers. Codex and Grok must be logged in
through their own CLIs.

- [ ] Focus a tab running Claude: the status bar ticker is labelled `Claude` and _(NOT VERIFIED 2026-09-29: Needs real Claude/Codex/Grok CLI accounts with usage data)_
  shows the `5h` / `7d` numbers as before. Clicking it still opens the Claude
  Usage dashboard tab.
- [ ] Focus a tab running Codex: the label becomes `Codex` and the text shows _(NOT VERIFIED 2026-09-29: Needs real Codex/Claude usage data (accounts).)_
  the Codex windows (e.g. `7d: 100% -1d`). The switch happens on tab focus,
  without waiting for the 5-minute poll.
- [ ] Clicking the Codex ticker opens a **Codex Usage Dashboard** tab (a _(NOT VERIFIED 2026-09-29: Needs real Codex/Claude/Grok agent in terminal to drive usage ticker)_
  singleton — clicking again focuses the existing tab, it does not duplicate).
- [ ] Focus a tab running Grok: the ticker is labelled `Grok`, shows the _(NOT VERIFIED 2026-09-29: Needs a real Grok CLI tab and provider billing data.)_
  provider's billing period and percentage, and opens a singleton **Grok Usage
  Dashboard** with tier and billing amounts.
- [ ] Switch to a plain shell tab: the ticker keeps showing the last agent _(NOT VERIFIED 2026-09-29: Needs real Claude/Codex/Grok sessions to drive usage ticker.)_
  rather than blanking or reverting to Claude.
- [ ] Switch Claude → Codex → Claude quickly. No stale value from the previous _(NOT VERIFIED 2026-09-29: Needs real Claude and Codex agents with usage tickers.)_
  agent lands on the ticker (the seq guard should drop late responses).
- [x] `curl http://localhost:9877/codex/usage` returns the JSON payload and
  contains **no** `email`, `user_id` or `account_id` field.
  _(verified 2026-09-19 against an isolated `tuic-remote` on :9877: the official
  App Server replacement returned HTTP 200 with plan and rate-limit data and no
  identity fields)_
- [ ] Log Codex out through the Codex CLI and focus a Codex tab: the ticker _(NOT VERIFIED 2026-09-29: Needs real Codex CLI login/logout)_
  reports the authentication failure without displaying a cached reading.
- [ ] With the Claude Usage toggle off, no usage ticker appears for Claude, _(NOT VERIFIED 2026-09-29: Needs real Claude/Codex/Grok agent tabs and usage accounts.)_
  Codex, or Grok.

### Codex Usage Dashboard

The active backend now includes `get_codex_usage_stats` and `GET /codex/stats`.

- [ ] **Rate Limits** section shows the account windows first with plain `5h` / _(NOT VERIFIED 2026-09-29: Needs real Codex account rate-limit data)_
  `7d` names, then the per-model windows prefixed with the model name. A window
  at 100% is red, ≥70% amber, below that normal.
- [ ] **Tokens per Day** renders one bar per day; hovering a bar shows the date _(NOT VERIFIED 2026-09-29: partial — Dashboard reachable only via the status-bar Codex ticker of a codex agent tab (no agent CLI) so not rendered. GET /codex/stats has 49 daily buckets, min 3,937,882 vs peak 2,988,540,050 tokens; code barHeightPercent = max(2, round(t/peak*100)) gives a 2% sliver; bar title '<date>: <n> tokens' (CodexUsageDashboard.tsx:222-227).)_
  and the token count. The tallest bar is the busiest day, and a near-zero day
  is still visible as a sliver rather than invisible.
- [ ] **Insights** shows the fields the official App Server supplies (lifetime _(NOT VERIFIED 2026-09-29: Needs real Codex account/App Server for Insights fields.)_
  tokens, peak day, streak, longest turn). Fields absent from that API are not
  invented and do not render as misleading zeroes.
- [ ] Kill the network and open the dashboard: a cached snapshot is used only _(NOT VERIFIED 2026-09-29: Needs Codex account usage dashboard and network cut-off.)_
  for up to 30 minutes; authentication failures are always surfaced.
- [x] `curl http://localhost:9877/codex/stats` contains **no** `profile` object
  (no username, display name or avatar URL).
  _(verified 2026-09-19 against an isolated `tuic-remote` on :9877: the official
  App Server response returned token summary/daily buckets under `stats` and no
  profile or identity fields)_

### Grok Usage Dashboard

The active backend now includes `get_grok_usage_api` and `GET /grok/usage`.

- [x] `curl http://localhost:9877/grok/usage` returns `credit_usage_percent`,
  `current_period`, and the subscription tier without exposing credentials.
  _(verified 2026-09-19 against an isolated `tuic-remote` on :9877: 26% weekly,
  subscription tier and billing fields present; the public payload uses
  `current_period.period_type` and contains no legacy `type` field)_
- [x] The usage bar, billing period end, on-demand used/cap, and prepaid balance
  match Grok's own billing view. Missing values render as unavailable, not zero.
  _(verified 2026-09-19 in browser mode against the isolated :9877 backend: the
  live 26% weekly period, tier, end date and billing cards rendered correctly;
  `GrokUsageDashboard.test.tsx` separately proves null renders as `--` while a
  real zero renders as `0.00`)_
- [ ] A logged-out Grok CLI surfaces an authentication error instead of a stale _(NOT VERIFIED 2026-09-29: Needs real Grok CLI logged out)_
  cached percentage.

## `index.lock` owner probe now fails closed (#694-4fcc)

**Rust — needs a `make dev` restart to take effect.** Unit-tested (28 passed), but
the live behaviour changed, so it is worth one look on a real repo.

Policy, decided by Boss 2026-09-07: when the `lsof` owner probe cannot answer, the
lock is **kept**, not reclaimed. The escape hatch is age at
`UNADJUDICATED_LOCK_STALE_SECS` (1 h), so a lock nothing can adjudicate is still
cleared eventually.

- [x] Normal case unchanged: a genuinely orphaned `index.lock` (kill a `git add` _(verified 2026-09-29: Disposable repo: killed a real 'git add many' (60k files) with SIGKILL when .git/index.lock appeared -> 0-byte orphan lock; after 31s GET /repo/files?path= answered normally, lock gone, plain git add then works.)_
  mid-write, wait 30 s) is still reclaimed and git works again.
- [ ] With the probe unavailable, the lock survives: temporarily shadow `lsof` _(NOT VERIFIED 2026-09-29: blocked — probe_index_lock_owner does Command::new("lsof") using the running instance's own PATH; shadowing lsof needs the instance restarted with a modified PATH (forbidden). Not attempted; existing unit tests inject the probe (reclaim_stale_index_lock).)_
  with a non-executable stub on `PATH`, create a 30 s-old lock, run a git command
  through TUIC, and confirm the lock is **still there** and `GET /logs` carries
  `Keeping index.lock … ownership could not be determined`.
- [x] The log names *which* failure it was — `could not run` vs `outlived its 2s _(verified 2026-09-29: by code/test inspection, tests not executed here: Owner-probe failure messages tested at git_cli.rs:1101 ('could not run') and git_cli.rs:1129 ('outlived its 2s deadline'); messages at git_cli.rs:406-408.)_
  deadline`. The two are not interchangeable and the message must say which.
- [ ] Watch for a lock kept longer than it used to be during ordinary work. The _(NOT VERIFIED 2026-09-29: Observational over ordinary real-world work (lock retention latency); not a discrete check.)_
  measured `lsof` latency here is 0.32–3.7 s against a 2 s deadline, so
  `DeadlineExceeded` is routine, not exotic — if that turns out to be noisy in
  practice, the deadline is the knob, not the policy.

## Terminal answers OSC 10/11/12 colour queries

**LIVE since the 2026-09-07 07:42 `make dev` — but NOT because it was committed.**
The answering code is still uncommitted: `git show HEAD:src-tauri/crates/tuic-terminal/src/terminal_grid.rs`
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

- the reply is built at `tuic-terminal/src/terminal_grid.rs:196-202` and pushed as
  `TermEvent::PtyWrite`, drained unconditionally on the chunk path at
  `pty.rs:5106-5119` — so it does NOT depend on a frontend being attached;
- `palette_color_for_index` (`tuic-terminal/src/terminal_grid.rs:94-103`) resolves foreground,
  background and cursor off a global `PALETTE` that always has a value, so the
  `None` branch cannot swallow a 10/11/12 query;
- reply content is asserted by `tuic-terminal/src/terminal_grid.rs:2360-2415`.

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
- [ ] **`^[[?6c` no longer appears at startup — NEEDS A `make dev` RESTART.** _(NOT VERIFIED 2026-09-29: partial — Own shell session on validate instance: fresh startup output and a script emitting DA1 query (ESC[c) -> no '?6c' or '^[[' in /output (1388 bytes), DONE echoed clean. Original capture ordering/frontend xterm DA1 reply path not reproduced (no webview attached).)_
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
- [ ] Switch to a light theme, then repeat the query: the reported colour _(NOT VERIFIED 2026-09-29: blocked — No theme picker in web Settings (checked previous batch) and theme cannot be switched without editing config; OSC 10/11 colour reply after theme change not testable.)_
  follows the theme (the frontend republishes on remeasure).
- [ ] Only one publish per real theme change — `GET /logs` shows no burst of _(NOT VERIFIED 2026-09-29: partial — Patched window.fetch to log '/theme-colors' and resized the viewport 4 times (1200x700, 900x800, 1100x850, 1440x900) with ~30 tabs: 0 theme-colors requests and 0 new /logs entries. The web client may not publish at all, so a real 'one publish per change' was not observed.)_
  palette traffic when resizing the window with several tabs open.
- [x] `curl -X POST http://localhost:9876/terminal/theme-colors -H 'content-type: application/json' -d '{"foreground":[255,0,0],"background":[0,255,0],"cursor":[0,0,255]}'` _(verified 2026-09-29: POST /terminal/theme-colors returned {ok:true}; an OSC 10/11/12 query in a session shell (osc.py) changed from cccccc/1e1e1e/cccccc to ff0000/00ff00/0000ff)_
  returns `{"ok":true}` and changes what the query above reports. (Port corrected
  from 9877: there is no second instance running; the live app serves 9876.)

### Upstream MCP OAuth — concurrent flows, expiry, late redirect

**Requires a `make dev` restart** — all of this is Rust (`mcp_oauth/`,
`mcp_proxy/registry.rs`). The running instance still has the old serialized
behaviour.

- [ ] Settings → Services → MCP: click **Authorize** on two different upstreams _(NOT VERIFIED 2026-09-29: Needs real upstream MCP OAuth servers and external browser consent)_
  back to back. Both show the consent dialog and open a browser tab within a
  second. Previously the second click hung silently for 5 minutes: no browser,
  no dialog, no error, while the row already read "Awaiting authorization…".
- [ ] Click **Authorize**, then **Cancel** before completing consent: the row _(NOT VERIFIED 2026-09-29: Needs a real upstream MCP OAuth server to reach 'Awaiting authorization' and complete/cancel consent in a browser.)_
  leaves "Awaiting authorization…" immediately and Authorize works again on the
  next click (no queue built up behind it).
- [ ] Click **Authorize** and then do nothing for >5 minutes. The row returns to _(NOT VERIFIED 2026-09-29: blocked — Needs a mock OAuth upstream MCP server plus >5 min wait; not set up within time-box.)_
  **Authorize to connect** (`needs_auth`) on its own, and
  `GET http://localhost:9876/logs?source=mcp_oauth` shows
  `Cleaned up expired OAuth flows` naming the upstream. It must not stay stuck
  on "Awaiting authorization…".
- [ ] Click **Authorize**, wait out the full 5-minute timeout *in the browser*, _(NOT VERIFIED 2026-09-29: Needs real upstream MCP OAuth provider, browser consent and 5-minute timeout with an external account.)_
  then complete consent. The browser shows the TUIC "Authentication failed" card
  reading "This authorization request expired or was cancelled…" plus "press
  Authorize again" — **not** the browser's own "can't connect to the server"
  page.
- [ ] A normal successful authorization still lands on the green _(NOT VERIFIED 2026-09-29: Needs a real upstream OAuth MCP server and browser authorization flow (external service))_
  "Authentication complete" card and the upstream goes `ready`.

### Terminal: no grid wipe on tab switch, resubscribe on reattach (#657-4345)

Frontend only (`Terminal.tsx`) — Vite HMR picks it up, no `make dev` restart
needed. Canvas painting is not observable over HTTP, so these need eyes.

- [ ] Switch back and forth between two busy terminal tabs. The returning tab _(NOT VERIFIED 2026-09-29: blocked — Trusted and DOM clicks on terminal tabs do not switch tabs in this web session (known limit); also 'busy tab' repaint flash not visible without screenshots.)_
  shows its content immediately with no blank flash. Previously every switch ran
  `resubscribe()` + `refresh()`, which cleared the grid and repainted it
  (paint → wipe → paint).
- [ ] Detach a tab into a floating window, then close that window to reattach. _(NOT VERIFIED 2026-09-29: blocked — Desktop-only: detach tab to floating window and reattach needs native window interaction on the validate instance (maccontrol targets orchestrator, not this instance).)_
  The reattached tab still paints live output and scrolls — the grid channel is
  resubscribed on this path, which is the only path that still resubscribes.
- [ ] Open a terminal in a split pane, collapse the pane to zero width, leave it _(NOT VERIFIED 2026-09-29: blocked — Split pane creation/collapse needs drag/keyboard interactions that do not work in this web session.)_
  collapsed for a minute. `GET http://localhost:9876/logs?source=terminal` shows
  one `Container stayed zero-size for 120 frames` warning and CPU stays flat.
  Previously that container kept a `requestAnimationFrame` loop re-arming every
  frame for the lifetime of the page, one loop per terminal, surviving unmount.

- [ ] (story 644-2cf4, Rust — needs a `make dev` restart) A reader-thread panic no _(NOTE 2026-09-29: partial evidence only — Panic path hard to force; code inspection at pty.rs:10929 (READER THREAD PANICKED) clears running flag.)_
  longer leaks its ticker. The panic path now clears the `running` flag, so the
  16 ms frame ticker and the 1 Hz silence timer both stop. Hard to force by hand;
  the observable if it ever happens is that a session logging
  `READER THREAD PANICKED` leaves no residual CPU and its tab stops repainting.
  Enable diagnostics and watch `thread count` stay flat after such a log line.

- [ ] (story 645-9bfb, Rust — needs a `make dev` restart) Resize an alternate-screen _(NOT VERIFIED 2026-09-29: Needs real grok agent streaming in alternate screen.)_
  agent (grok) while it is streaming, then let it ask a low-confidence question.
  The tab must badge within about a second of the resize. Before the fix the resize
  grace re-armed on every chunk, so questions, rate-limit and API-error events and
  the busy badge stayed suppressed until the agent went quiet for a full second.
  Also confirm a resize during a normal-screen Claude re-render still does NOT
  flip an idle tab to busy — that is the behaviour the grace extension protects.

## Settings search (story 684-35a8, frontend — Vite HMR picks it up)

- [ ] Open Settings. A "Search settings" box now sits at the top of the left nav. _(NOT VERIFIED 2026-09-29: partial — Only measured at default nav 180px: search input 155px wide, 12px font, padding 24/22px, placeholder 'Search settings' fits (scrollWidth<=clientWidth). Nav resize handle drag via agent-browser mouse did not change width, so 140/280px not tested; no screenshot.)_
  Check it reads well at the narrowest (140 px) and widest (280 px) nav widths —
  the box shares the nav's resize handle area, and only the DOM is covered by
  tests, not the rendering.
- [ ] Type `relay`. The tab body is replaced by a result list; each row shows the _(NOT VERIFIED 2026-09-29: partial — Dark theme only (no theme picker in web UI): search 'relay' rows have label + trail 'Remote Access > Cloud Relay'; trail color rgb(115,115,115) on bg rgb(30,30,30) = ~3.5:1 contrast. Light theme not checked; no screenshot.)_
  setting on top and a `Tab › Section` trail underneath. Confirm the trail is
  legible against the panel background in both light and dark themes.
- [x] Click the "Relay Server URL" result. Services & MCP opens and the view _(verified 2026-09-29: Settings search 'relay', clicked 'Relay Server URL' result (Remote Access > Cloud Relay): active nav 'Remote Access', label at y=722 of 900 viewport, height 14 (field in view). Scroll animation not observed.)_
  scrolls to that field. The smooth-scroll animation itself is not observable
  over the DOM — confirm it lands on the field, not at the top of the tab.
- [ ] Search a Dictation setting (e.g. `whisper`) in the desktop app: it appears. _(NOT VERIFIED 2026-09-29: partial — Browser :9880: search 'whisper' returns 'Whisper Model > Voice > Speech recognition' (NOT 'No settings match'): the Voice tab is present in web mode now, so item premise is outdated. Desktop half not checked.)_
  In browser mode (`http://localhost:9876/`) the Dictation tab is absent, so the
  same query must return "No settings match your search."

## Cross-kind tab drag reorder (story 682-b8d2, frontend — Vite HMR picks it up)

Free-mode and terminals-first drag reorder across tab kinds never worked: the
cross-kind order list had no writer, so the reorder call always returned early.
The DOM order is covered by tests; a real pointer drag in the WebView is not.

- [ ] Settings → Appearance → Tab Ordering → **Free**. Open a terminal, a diff and _(NOT VERIFIED 2026-09-29: blocked — Free-mode drag of diff/terminal/markdown tabs needs HTML5 drag-and-drop; D&D is Boss-approval territory and agent-browser mouse drag has not worked in this session (sidebar/nav handle drags did nothing). Setting 'Tab Ordering' exists with options Grouped by Type/Terminals First/Free (default grouped).)_
  a markdown tab. Drag the diff tab onto the left half of the terminal tab: it must
  land before the terminal and stay there. Repeat dragging the terminal to the right
  half of the markdown tab.
- [ ] Still in Free mode, open a new terminal after a drag. It must appear at the _(NOT VERIFIED 2026-09-29: blocked — Needs a prior drag in Free mode; D&D not drivable here.)_
  end without disturbing the order you dragged.
- [ ] Switch to **Terminals First**. Terminals stay leftmost. Drag the markdown tab _(NOT VERIFIED 2026-09-29: blocked — Needs a drag of the markdown tab onto the diff tab (D&D not drivable here).)_
  onto the diff tab — the two non-terminal tabs must swap, and the terminals must
  not move.
- [ ] Switch to **Grouped by Type** (the default). Ordering must be unchanged from _(NOT VERIFIED 2026-09-29: blocked — Drag within kinds not drivable; only confirmed default tab_ordering is Grouped by Type (Appearance select value grouped-by-type).)_
  before this story: kinds stay grouped, and dragging only reorders within a kind.
- [ ] Close a tab you dragged, then reopen one. No ghost position: the reopened tab _(NOT VERIFIED 2026-09-29: partial — Set Free (config tab_ordering_mode=free), closed tab 'Foo', opened New Tab: new tab 'main 4' at the end of the list. No dragged tab was involved (D&D not drivable), so 'no ghost position' after a drag is untested. Mode restored to grouped-by-type.)_
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

- [ ] With the app stopped, truncate `config.json` mid-document, then start it. The _(NOT VERIFIED 2026-09-30: partial — Not re-run: desktop stop/start impossible here and I must not restart the instance or start a second daemon; prior 29/09 tuic-remote evidence not repeated.)_
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

- [ ] Provoke or wait for an `API Error: 5xx` in an agent tab — the error toast/sound _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Toast/sound is frontend; API Error 5xx not provoked (would need fake Anthropic endpoint and a UI to observe the notification).)_
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
- [ ] `repo action=issues`, `action=close_issue` and `action=reopen_issue` all _(NOT VERIFIED 2026-09-30: partial — MCP repo action=issues -> error 'was removed; use GET /repo/issues' (item stale). GET /repo/issues returns [] and POST /repo/issues/close on #2000000000 returns GitHub 'Failed to close issue (404)': HTTP path reaches GitHub.)_
  reach GitHub instead of answering `Unknown action 'issues' for tool 'repo'`.
- [x] Register the same UI tab id repeatedly from one session: it dedupes, and the _(verified 2026-09-29: MCP ui action=tab from a peer named as a live PTY id: 5x id 'same' then session DELETE -> close-html-tabs tab_ids as one entry (see next); 75 distinct opens -> close list 64 entries (cap SESSION_HTML_TAB_LIMIT), oldest (same,t1-t6) evicted, t70 kept.)_
  per-session count stops at the cap instead of growing.

## GitHub poller survives a dropped connection (story 648-051b, Rust — needs `make dev` restart)

The shared HTTP client had no timeout at all, so a dropped VPN wedged the poller
on a socket the peer never answers.

- [ ] Start the GitHub poller, then drop the network (turn off Wi-Fi or the VPN). _(NOT VERIFIED 2026-09-30: partial — Not run: dropping host Wi-Fi/VPN would disrupt other agents on this shared Mac; no override for the GitHub base URL found.)_
  Within ~30 s the request must fail and the poller must log the error and carry
  on, not sit silent forever.
- [ ] With the network still down, disable GitHub polling in Settings. It must _(NOT VERIFIED 2026-09-30: partial — Not run: depends on the network outage of 1760 and a Settings toggle (frontend).)_
  stop immediately, not after the in-flight request gives up.

## Git status and index.lock ownership (story 673-19fa, Rust — needs `make dev` restart)

The sidebar dirty badge now reads the gix porcelain-v2 counts, and the stale
`index.lock` sweep asks `lsof` who owns the lock before trusting the age rule.

- [ ] The sidebar repo badge still shows clean / dirty / conflict correctly: _(NOT VERIFIED 2026-09-30: partial — Fixture repo, /repo/info status vs git status: clean/clean, ' M'->dirty, 'M '->dirty, UU->conflict, abort->clean, all match, but each change appears only after the 60s repo_info cache TTL on this headless instance (no watcher invalidation). Sidebar badge UI not seen.)_
  edit a file, stage it, create a merge conflict, then clean up. Each state must
  match what `git status` reports.
- [x] Start a long `git add` or `git stash` in a large repo from a TUIC terminal _(verified 2026-09-30: Real 'git add' held .git/index.lock ~40s (clean filter sleep 50) in a TUIC terminal; lock present at 11/20/31/41s while /repo/info, working-tree-status, branches were called; removed only when git ended (50s).)_
  and leave it running past 30 s. TUIC must NOT delete that repo's
  `.git/index.lock` while the command still holds it.

## Weekly advisory scan (story 663-feea, CI — verify after merge)

`audit.yml` now installs a prebuilt `cargo-audit` and reads its ignore list from
`src-tauri/.cargo/audit.toml`. The workflow only runs on Mondays or on demand,
so nothing local can prove the install step resolves.

- [ ] Trigger `audit.yml` manually (`gh workflow run audit.yml`) and confirm the _(NOT VERIFIED 2026-09-29: Needs GitHub Actions run (gh workflow run audit.yml) on the remote; external service, not local.)_
  `Install cargo-audit` step resolves `taiki-e/install-action@cargo-audit` and
  the scan runs to completion.

## Process manager after the shared `ps` walk (story 669-e059, needs a Rust restart)

The stats refresh now queries the process table ONCE per refresh and walks each
session's subtree out of that shared map, instead of forking `ps` per session.
Rust does not hot-reload, so this needs a `make dev` restart to load.

- [x] With several sessions open (at least one running a nested command such as _(verified 2026-09-29: 3 MCP sessions in ur2, one running sh -c 'sleep 40; sleep 41'. GET /process/stats and the Process Manager modal (palette > Process Manager) list shell 706d0311 pid 80056 (2.4 MB) plus child 'sleep' pid 80058 with 1.1 MB RSS; every session listed with non-zero RSS. Depth only 2 levels.)_
  `cargo test` or a `sh -c 'sleep 30'`), open the process manager and confirm
  each session still lists its child AND its descendants, with non-zero RSS.

## Smart Prompts dropdown: missing-provider hint is now clickable (story 706-8d98) — **DELETED 2026-09-19**

Five items on a dimmed `api`-mode prompt whose reason text was clickable through
to `Settings → Providers`. #784-0aec deleted every precondition: there is no
Providers tab and no Headless slot to unassign, and an `api` prompt is now
refused with a reason rather than dimmed behind a provider hint. The tab comes
back as 786-4a6d and `api` execution as 787-ee50, each with its own checks —
these are unrunnable rather than pending and the items are removed instead of
ticked. The DEFERRED comment in `SmartButtonStrip.tsx` still records why the
compact split-button strip kept a plain hover tooltip.

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

- [ ] Desktop, normal path: fetch/pull/push from the Git panel still work and _(NOT VERIFIED 2026-09-30: partial — POST /repo/run-git fetch: reachable local remote success:true; hanging remote (silent TCP listener) -> after 180s 'git timed out after 180.0s and was killed'. Side finding: orphaned 'git remote-http' helpers (ppid 1) survived until the peer closed. Git panel UI/pull/push not driven.)_
  still report failures the way they did before. No visible change expected.
- [ ] Browser mode (`http://localhost:9876/`): do a fetch on a repo whose _(NOT VERIFIED 2026-09-30: partial — Same API-level evidence as 1828 through the local router (unix socket): reachable fetch ok, hang -> 'git timed out after 180.0s' at 180s. Browser UI not driven.)_
  remote is reachable. It should behave exactly as on desktop.
- [x] Slow/dead remote: point a throwaway repo at an unroutable remote and _(verified 2026-09-29: POST /repo/run-git {fetch origin} on throwaway repo: remote http://127.0.0.1:9899 (mute TCP listener) -> after 180s 'Failed to execute git: git timed out after 180.0s and was killed', success=false. Unroutable 10.255.255.1 failed earlier at 75s by OS connect timeout ('Couldn't connect'), so it does not reach the deadline on this net.)_
  fetch. It must give up after ~180s with a `git timed out` message, not hang
  forever. This is the whole point of the story — do it on a throwaway repo,
  never on a real one.

## Language picker in Settings → General (story 689-52d8, visual)

The General tab now renders a Language select above Shell, listing every locale
that ships a message catalog. Only `en.json` exists today, so the list has one
entry ("English"). Frontend-only change, so Vite HMR loads it, but the rendering
cannot be checked from a test.

- [ ] Open Settings → General and confirm the Language select sits directly under _(NOTE 2026-09-29: superseded — GeneralTab.tsx:143 hides the Language select when AVAILABLE_LOCALES.length<=1 (only 'en' catalog); rewrite the expectation)_
  the "General" heading, above Shell, with the same field styling as the IDE and
  update-channel selects (label, control width, hint line).
- [ ] Confirm the option reads "English" and the hint reads "Language of the _(NOTE 2026-09-29: partial evidence only — Language hint present at en.json:299 and GeneralTab.tsx:149 ('Language of the TUICommander interface'); option label check by inspecting GeneralTab.)_
  TUICommander interface".
- [ ] Type "language" in the Settings search box and confirm the result reads _(NOT VERIFIED 2026-09-29: partial — Search 'language' lists 'Language > General > General' (text matches) and click succeeds, but the General tab renders no Language field (hidden, 1 locale), so nothing to scroll to: stale search entry for a hidden control.)_
  `General › General` and scrolls to the field when selected.
- [ ] The single option is by design: only locales that ship a catalog are _(NOTE 2026-09-29: moot — the picker is already hidden with one locale (GeneralTab.tsx:143); the item says it stays visible)_
  offered, and listing others would show English under a foreign name. The
  control stays visible so the docs that already promise it stay true. Say if
  you would rather it were hidden until a second catalogue lands.

## PTY chunk-path refactor (story `668-59be`, **Rust — needs `make dev` restart**)

Behaviour must be IDENTICAL to before; five characterization tests assert that,
so these checks are looking for what a test cannot see on a live agent.

- [x] On a live Claude tab and a live grok tab: the state badge still moves _(verified 2026-09-30: Real claude: starting->working(busy)->idle over a tool turn; permission dialog -> awaiting_input; real grok: working->completed/idle twice. 'Feel' is subjective; transitions ordered correctly.)_
  working → idle → awaiting as it did. The chunk path was reordered around the
  chrome cutoff and the SilenceState locks; the tests cover the events, not the
  feel.
- [x] A slash menu (`/` in Claude Code) still opens and is detected. This is the _(verified 2026-09-30: Real claude, typed '/': session state slash_menu_items populated (/wiz:handoff highlighted ...), agent_state stays idle; cleared after backspace.)_
  case that killed the proposed optimisation — the menu renders BELOW the input
  box, so it is the first thing to break if the cutoff order is ever touched
  again (`DEFERRED (2026-09-06)` at `pty.rs:4911`).
- [x] A choice dialog and an Ink question footer still badge the tab as awaiting, _(verified 2026-09-30: Real claude --permission-mode default: Bash dialog -> agent_state awaiting_input/awaiting true, after Enter -> working -> idle, awaiting false. Ink AskUserQuestion footer: awaiting true, false after Esc.)_
  and the badge still CLEARS afterwards.
- [x] **Observability trade — check this deliberately.** The DECRST-leak _(verified 2026-09-29: Shell session prints ESC[2J/ESC[3J and bare '1049l'. Diagnostics OFF: 0 'DECRST leak'/'Anomalous ANSI' in GET /logs (count unchanged 2/2 on a second OFF run). POST /diagnostics enabled:true then same output: error 'DECRST leak: kitty_clean has bare 1049l...' + warn 'Anomalous ANSI sequence: ESC[2J/3J' appear. Guards at pty.rs:6344/10758/10787. Disa)_
  `error!` and the "Anomalous ANSI sequence" `warn!` no longer appear unless
  Diagnostics is on. Run `curl -X POST localhost:9876/diagnostics -d
  '{"enabled":true}' -H 'content-type: application/json'`, then confirm they
  reappear in `GET /logs`. If either turns out to be load-bearing for an open
  bug while OFF, revert the three `&& crate::cpu_watchdog::diagnostic_mode()`
  guards at `pty.rs:5025`, `pty.rs:8014`, `pty.rs:8042` — they are isolated.
- [x] Resize a tab mid-turn on an agent that was busy: the resize grace still _(verified 2026-09-30: Real claude in a silent 25s 'sleep' tool call: 4 POST /sessions/{id}/resize calls mid-turn; agent_state stayed working/busy throughout, idle only at turn end (25.2s).)_
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

- [ ] Open the web UI (browser, not the desktop app) on a session with _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs a browser attached to a scrolling session (wheel/drag on the canvas); headless instance has no frontend. 29/09 note: tab switching in the web UI failed for MCP-created tabs; server scroll-to-offset returns ok but display_offset stays 0 with no subscribed viewer.)_
  scrollback and scroll with the wheel and by dragging the scrollbar: the
  viewport must move, not just the thumb.
- [ ] With that browser attached, close the same terminal's tab in the desktop _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs a browser-attached scrolling session AND a desktop instance to close the same tab; both frontends; headless instance has neither.)_
  app. The browser must keep scrolling — the unsubscribe no longer drops the
  session's scroll target.

## Opening a 23 MB JSON no longer freezes the editor (2026-09-06, frontend — HMR; one Rust part needs `make dev` restart)

Above 500 KB the editor is plain text: no highlighting, no git gutter, no inline
blame, and the disk poll no longer re-reads the whole file 5 s after opening.
`get_gutter_changes` (Rust) returns nothing for an untracked file instead of
one "added" marker per line. Measured in Chrome only; WKWebView is the one
that blocked for over a minute.

- [ ] Desktop app: open `~/Gits/personal/ego/mutants.out/mutants.json` (23 MB, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Opening a 23 MB JSON in the editor is frontend/desktop behaviour; instance has no frontend. Real mutants.json is 5 KB now (needs a synthetic 24 MB file in a registered repo).)_
  gitignored). It must open in a few seconds at most, unhighlighted, with no
  gutter markers. With `window.__TUIC__.setPerfDebug(true)` first, any
  remaining `UI freeze` line on `/logs` names an `editor.*` breadcrumb.
- [x] After the `make dev` restart: a small **untracked** file opens with an _(verified 2026-09-29: Web UI local repo (fx/agb2/lr): untracked small.txt opens with no git markers (changeGutter only active-line element); tracked f.txt edited vs HEAD shows 2 cm-gitMarker elements; Git panel diff of untracked small.txt shows '+a +b +c' (all added). Instance is the running debug build (not restarted after this change, current build).)_
  empty gutter; a tracked file with an unsaved-vs-HEAD edit still shows its
  markers; the diff viewer still shows the untracked file as all added.
- [ ] Desktop app (WKWebView), after the fix that installs the document with _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: WKWebView desktop smoothness with a 23 MB file; engine-specific, needs the desktop build.)_
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

- [x] Open a goose tab, let it sit at the composer: the badge must read idle, _(verified 2026-09-30: POST /sessions/agent {agent_type:goose,binary_path:goose,args:[],no prompt} (goose 1.49 at composer): session status agent_state=idle shell_state=idle in 6 samples over 24s. Note: plain 'goose' typed in a shell PTY (no agent_type seeded) stayed shell_state busy 67s+.)_
  not "working". This is the whole bug — before the adapter it latched busy
  from the moment the process started.
- [x] Send it a prompt: the badge must go to working for the whole turn (the _(verified 2026-09-30: Sent 'reply with the word ok' + Enter to goose session: session status agent_state=working in 46 consecutive 1s samples (spinner text changing) then idle in all following samples once composer returned; no flicker. Turn 58s.)_
  spinner message is whimsical and changes every second — the badge must not
  flicker with it) and back to idle when the composer returns.
- [ ] Interrupt a turn with Ctrl+C: the badge must return to idle, not stay _(FAILED 2026-09-30 story 1301-87fd: Goose turn then Ctrl+C (\u0003 via /sessions/{id}/write): screen shows composer placeholder 'Interrupted, what should goose work on instead?' (no 'Enter to send' hint); agent_state stays working/shell_state busy for 40+s. detect_goose_screen_activity (pty.rs:3895) needs Enter to send -> Unknown.)_ _(fix landed e8043223a 2026-10-01: retest on the next build)_
  working.
- [x] amp, cursor and droid are still **not** adapted (see the DEFERRED note on _(verified 2026-09-29: by code/test inspection, tests not executed here: Informational note: amp/cursor/droid unadapted, see has_ready_screen_adapter pty.rs:3991 (no adapter listed in pty/tests.rs:512/2418/2530).)_
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

- [ ] [HUMAN] Install any of `amp`, `cursor-agent`, `goose` or `droid` and capture an _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
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

- [ ] [HUMAN] Under `make dev`, edit any file in `src/` to force a Vite full reload _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  (story #716-031e). Every terminal pane must come back filling its pane, with no
  window resize: no small canvas in the top-left corner with black around it, and
  scrolling must show every row. Split a pane and reload again — both halves. Canvas
  geometry is not observable over HTTP, which is why this is by eye.

- [ ] [HUMAN] Reinstall the TUIC hooks from Settings → Agents, then confirm _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
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

- [ ] [HUMAN] In a release `.app`, hold `j`/`l`/`i` in vim: the cursor repeats and no _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  accent picker appears. Option-key composition must still produce accented
  characters, and a user with `defaults write -g ApplePressAndHoldEnabled -bool true`
  must keep their override (the registration domain is lowest priority). Needs the
  release bundle domain — `press_and_hold.rs`, called from the `lib.rs` setup — and
  real key-repeat hardware. (#79)
- [ ] [HUMAN] Settings → Notifications → Attention → Test in a rebuilt app: the native _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  engine matches the sample Boss approved on 2026-08-09 (triangular G4→G4→E5,
  75/75/140 ms, 50 ms gaps, gain 0.8) and stays identifiable from another room
  without being irritating. Audio, judged by ear.
- [ ] [HUMAN] Copy a long Claude message out of the terminal and paste it into Slack: _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  no `▎` gutter and no gutter NBSPs, while lists, blank lines, indentation, `:wave:`
  and the body spacing survive unchanged. The text itself is asserted by nine Rust
  tests (`cargo nextest -E 'test(copied_selection)'`, `tuic-terminal/src/terminal_grid.rs:1687`); the
  paste is not. Tried twice from automation — `agent-browser clipboard read` fails
  with `Resource temporarily unavailable (os error 35)`.
- [ ] [HUMAN] Drag a file out of the file browser onto Finder, and drop a large folder _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  from Finder into the app. The first is a real cross-application OS drag. The second
  confirms a **deliberate** gap, not a regression: `fs_transfer_paths` (`fs.rs:1624`)
  is still synchronous on the main thread because it is the drag-and-drop backend and
  D&D changes need Boss's approval.
- [ ] [HUMAN] Install `zed`, put a comment, a trailing comma and hand-tuned indentation _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  in `~/.config/zed/settings.json`, install the bridge from Settings → Agents, and
  confirm Zed still starts, still shows every setting, and lists the `tuicommander`
  context server — `diff` against `<config dir>/mcp-backups/zed-settings.json.orig`
  must show only the added member. Then press **Remove all MCP integrations** and
  confirm each client lost only its `tuicommander` entry and a relaunch does not put
  it back. Zed is not installed here, and a real client reading the file afterwards
  is the one thing the splice tests cannot cover. (issue #115)
- [ ] [HUMAN] Compare OSC 133 gutter marks side by side, browser at `:9876` against the _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  desktop app, on the same session: same rows, same size, neither client stealing the
  other's dirty rows. Canvas painting is not observable over HTTP, and both clients
  have to be visible at once. (Port corrected 2026-09-07 from `:9877` — only one
  instance runs, and it serves 9876; a browser pointed at 9877 gets nothing.)
- [ ] [HUMAN] Open `vim` or `htop`: the wheel still goes to the app, `Shift+wheel` _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  scrolls TUIC history, and quitting restores the shell scrollback unchanged. The
  enter/exit half is covered by the `gh-run-watch.raw` replay test; mouse-reporting
  forwarding needs a real wheel. `lazygit` is not installed.
- [ ] [HUMAN] Print a fullwidth char and overwrite half of it — `printf '\e[1;5H中'` _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  then `printf '\e[1;6HX'` — and confirm no ghost `中` survives beside the `X`. Scroll
  away and back to prove it is not just hidden by a later full-row reship. Canvas
  painting.
- [ ] [HUMAN] Raise an MCP `ui action=confirm` and answer it on a phone at _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  `/mobile.html`: the desktop dialog must disappear by itself, and the reverse must
  work too. Then, with a push subscription registered and the PWA closed, confirm the
  push carries the title. Needs a real phone and a real subscription.
- [ ] [HUMAN] Comment a word that repeats many times in a markdown preview ("reason" _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  ×18) and confirm the highlight lands on the occurrence you selected, and that
  selecting across an existing highlight hides "Add comment". The offsets are asserted
  in `tweakComments.test.ts`; where the highlight is *drawn* is not.
- [ ] [HUMAN] Boss's call: **Trim** the real `src-tauri/target` row in Build Cleaner. It _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  is 58 GiB and a Trim forces a full rebuild of the running dev app, so no agent may
  run it. Trim against other repos' `target/` is covered.
- [x] Rust change, needs a `make dev` restart (stories #5525 / #8c80). Switch to a repo _(verified 2026-09-29: Isolated instance (web UI + /logs): default active_and_switch: adding/switching to never-indexed repo lr logs info 'content index warm on repo switch' then 'content index built'. Set active_only in Settings>General (config index_strategy=active_only), added+switched to lr2: only debug 'content index warm skipped by strategy', no warm/built pair. Cr)_
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
- [ ] Rust change, needs a `make dev` restart (story #650-b0a0). With the app started _(NOT VERIFIED 2026-09-29: Needs a valid external relay server URL/token and killing the relay server; external service.)_
  while **Cloud Relay is off**, turn it on in Settings → Services with a valid relay URL
  and token: the status dot must go green with no app restart, and `GET :9876/logs`
  must show `relay: connecting to …`. Turn it off: the dot goes grey and the log shows
  `relay: shutting down` then `relay: stopped`. Turn it on again — the supervisor must
  still be watching after a stop. Then kill the relay server (or pull the network) while
  connected: every reconnect log must read `reconnecting in 1s` for the first attempt
  after each *successful* connection, growing 1→2→4… only across consecutive failures.
- [x] Rust change, needs a `make dev` restart (story #656-2b63). Spawn an agent via MCP _(verified 2026-09-29: MCP agent spawn rows=50 cols=140 (fake agent via binary_path): scroll-info screen_lines=50, ROW45 at row 45, child stty 50 140. After exit session wait until=exited -> {met:true,exit_code:7}; unknown ids -> 'Unknown session'. POST /sessions/agent and POST /sessions rows/cols 50x140 -> screen_lines 50.)_
  `agent action=spawn` with explicit `rows`/`cols` (e.g. 50x140), then confirm the tab
  renders the full screen: before this, the VT screen was built at a hardcoded 24x220
  while the child was handed the caller's geometry, so anything below row 24 (an agent's
  input box, a dialog footer) never reached the parsers. Then let that child exit and
  call `session action=wait session_id=<id> until=exited` **after** it has died: it must
  answer `{met:true, exit_code:N}` instead of `{"error":"Unknown session …"}`. A wait on
  an id that never existed must still fail fast with `Unknown session`. The same
  registration path now also backs `POST /agents` and browser/remote `POST /sessions`,
  so a browser-created terminal and an HTTP-spawned agent both need a smoke check.
- [ ] Rust change, needs a `make dev` restart (story #654-bfc1). Put a stub earlier on _(NOT VERIFIED 2026-09-29: partial — Stub cannot take priority: resolve_cli probes /usr/local/bin then /opt/homebrew/bin (real gh there; code comment says a PATH stub is never picked up) and I may not touch system dirs. Observed on headless tuic-remote (PATH stub first, sandbox HOME): HTTP bound 0.28s after start, deferred probe logged off boot path. Not checked: 10s 'did not answer' )_
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
- [ ] Rust change, needs a `make dev` restart (story #642-3741). `mod dictation_routes` _(NOT VERIFIED 2026-09-29: partial: /dictation/status, /models, /devices, /config, /system/relay-status and /system/check-update?channel=nightly answer JSON (unix-socket router); browser dictation record/inject and output device need a microphone/speaker)_
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
- [ ] Rust change, needs a `make dev` restart (story #670-b9a2). Grid delivery got three _(NOT VERIFIED 2026-09-29: Needs the desktop WebView with tauri::ipc::Channel, blocking the app JS thread via devtools; not reachable from HTTP/MCP on a test instance.)_
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
- [ ] Rust change, needs a `make dev` restart (story #672-c1a3). Three always-on _(NOT VERIFIED 2026-09-29: partial — AI cron scheduler obsolete (no ai/scheduler in src). Knowledge persists: ai-sessions/*.json written. tuic.log.2026-09-29 grows. Own daemon SIGINT: 'Received shutdown signal' line flushed to log, process exited. UTC-midnight rotation not observed (23:19 UTC).)_
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
- [ ] Rust change, needs a `make dev` restart. Dictation speech gates: the transcriber _(NOT VERIFIED 2026-09-29: Needs real microphone silence/speech and Whisper gating.)_
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
- [ ] Rust change, needs a `make dev` restart. Per-tab agent resume (issue #119): with _(NOT VERIFIED 2026-09-29: Needs several real Claude tabs in one folder to verify per-tab session discovery)_
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
- **DELETED 2026-09-19 — two items, both unrunnable.** **Detached AI Chat
  window** (`700-4d5d`) asked for a stream to follow the panel into its own window,
  and **agent runs persist with the conversation** (`705-57fa`) asked for a
  `schema_version: 3` migration and tool cards surviving a reload. #784-0aec deleted
  the engine under both: no conversation store, no
  `<config_dir>/ai-chat-conversations/`, no autonomous agent goal, no tool card, no
  *Explain this error* context-menu entry. The one surviving check — the panel
  detaches and comes home — is listed under *The embedded AI engine is gone
  (#784-0aec)* at the end of this file. The streaming half returns with 785-58ca and
  will be written against ego rather than restored from here.
- [ ] **Rust change — needs a `make dev` restart** (or `make build`). Session state is _(NOT VERIFIED 2026-09-29: partial — /events SSE (unix socket) emits 'session-state-changed' {session_id,state{awaiting_input,shell_state,last_activity_ms}} only on real transitions (11 events for 2 shell commands: busy/idle). Browser resource entries: no /sessions or list_active_sessions polling in 15s (hidden tab); diagnostics HEALTH lines show no extra IPC field. Awaiting badge wit)_
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
- **DELETED 2026-09-19 — unrunnable.** **Provider availability is now rendered**
  (`701-b6ac`) asked for a visual check of the reachable / not-detected / wrong-port
  rows in `Settings > Providers`. #784-0aec deleted `detect_ollama` with the rest of
  the provider registry and removed the tab itself — no row, no icon, no reason line.
  Provider configuration moves into ego as 786-4a6d, which brings its own checks.
- [ ] **Rust change — needs a `make dev` restart** (or `make build`). Block-display _(NOT VERIFIED 2026-09-29: partial — Toggles Show block timestamps/Block folding/scrollbar marks exist (Expert on for last two), off writes config.json show_block_timestamps/block_folding_enabled=false, survive page reload+GET /config. Restart, Ctrl+Cmd label, Cmd+Shift+. fold not drivable. Default-hidden without Expert.)_
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

- [ ] **Show scrollbar marks toggle** (719-36af) — frontend only, so Vite HMR _(NOT VERIFIED 2026-09-29: partial — Toggle 'Show scrollbar marks' present after Block folding (Expert on), searchable ('scrollbar' -> Terminal>Terminal), persists in config.json false/true and after page reload. Canvas tick gating/Cmd+F ticks not observable; restart not done. Item says General>Terminal; it's Terminal tab.)_
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

- **DELETED 2026-09-19 — unrunnable.** **Agent tool-log bound, measured on a real
  run** (`718-aebf`) asked for the file size and rewrite rate of a long autonomous
  agent session. #784-0aec deleted `conversationStore.ts`, the 512 KB tool-log
  ceiling it enforced, and the agent run that filled it; there is no conversation
  file left to measure and no tool card to reload. ego keeps its own transcript, so
  bounding it is ego's problem, not TUICommander's.

- [ ] **Scrollback reflow honours its Settings toggle** (660-d087) — **Rust _(NOT VERIFIED 2026-09-29: partial — Web UI Terminal (Expert on) 'Reflow scrollback on resize' ON by default (config true). OFF: 100->30 col resize truncated lines (22 rows, no extra). Flipped ON after session existed: resize 30->80 rejoined wrapped lines, config persisted. Visual re-wrap/htop and restart not checked.)_
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

- [ ] **Headless daemon serves Claude usage again** (678-9a75) — **Rust change, _(NOT VERIFIED 2026-09-29: partial — Ran built tuic-remote (--no-default-features build in instance target, --instance ag1remote, TUIC_PORT=9891, own TMPDIR): starts; unix-socket GET /claude/usage reaches handler (500 upstream 'OAuth access token has expired', env not code), /claude/timeline 400 (needs params), /claude/session-stats 400, /claude/projects 200 - not 404. TCP needs auth )_
  needs a rebuild.** `claude_usage_cache` carried `#[cfg(feature = "desktop")]`
  while `build_router` mounts `/claude/usage` and `/claude/usage/timeline`
  unconditionally, so `cargo build --bin tuic-remote --no-default-features` did
  not compile at all. The gate is gone. After a rebuild, start `tuic-remote` and
  check `curl http://127.0.0.1:<port>/claude/usage` answers instead of 404/500.
  Desktop behaviour must be unchanged — the same endpoint on :9876 still works.

- [ ] **Frontend liveness watchdog + WebView reload escape hatch** — **Rust + _(NOT VERIFIED 2026-09-29: partial: /logs?source=diagnostics has no 'Frontend unresponsive' on a healthy start; POST /debug/reload_webview -> {ok:true, action:navigate} and both PTY sessions remain. Note: it navigated the desktop window to http://127.0.0.1:1421/ (Vite dev URL), so repaint, the 40 s block and sleep cases were not observed)_
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

- [ ] **Resume finds the session the alias hid** — **Rust + frontend change, _(NOT VERIFIED 2026-09-29: Needs real Claude with c/c2 aliases and two config dirs, resume into real conversation.)_
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
7. **Only app URLs become recovery targets.** After restarting `make dev`,
   navigate among in-app routes, then verify that a blocked navigation to a
   different localhost port or external host does not replace the URL returned
   by `POST /debug/reload_webview`. The Rust origin guard is covered by
   `webview_recovery::tests::recovery_keeps_the_last_app_url_when_other_documents_are_observed`;
   this checks the native WebView path after rebuild.

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

- [x] Trigger `ui action=confirm` from an MCP client and let it sit unanswered _(verified 2026-09-29: MCP ui action=confirm left unanswered: call returned after 300s (03:09:47 -> ~03:14:47) with {confirmed:false, reason:"no answer within 300s"} (not HTTP 408). Dialog text was in the :9880 browser DOM at +5s and gone after. Desktop WebView and mobile PWA surfaces not observed.)_
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

Both live in `src-tauri/src/config.rs` / `lib.rs` / `crates/tuic-core/src/app_instance.rs`, so
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

- [ ] Create a linked worktree of this repo through the dialog or _(NOT VERIFIED 2026-09-30: partial — Own daemon r3iso, fixture repo (ignored node_modules/ 200 files, src-tauri/target/): repo worktree_create (no mode/dirty) -> worktree_list warm_artifacts.status pending->done within ~1s; worktree has node_modules/ (200 files) and src-tauri/target/, no .env, .git is a file, git status clean. warmed_d)_
      `repo action=worktree_create` without `mode` or `dirty` fields.
      It must contain `node_modules/` and `src-tauri/target/` straight away, and
      the MCP/HTTP `instructions` payload must report
      `warm_artifacts.warmed_directories` > 0.
- [x] `git -C <worktree> status` must still work after creation, and the _(verified 2026-09-29: fixture worktree via MCP repo worktree_create: git -C status OK, .git is a file)_
      worktree's `.git` must still be a FILE, not a directory.
- [x] The ignored top-level FILES must NOT have been copied: no `.env`, _(verified 2026-09-29: fixture repo with ignored .env/.mcp.json/CLAUDE.md: none appear in the new worktree (node_modules/ and target/ do))_
      `.mcp.json`, `CLAUDE.md` newly appearing in the worktree beyond what the
      branch tracks.
- [ ] `plugins/` (a submodule) and `src-tauri/plugins/claude-wakeup/` must not _(NOT VERIFIED 2026-09-30: partial — Fixture analog (not real repo) on r3iso: repo with submodule 'plugins' + tracked src-tauri/plugins/claude-wakeup with ignored node_modules; linked worktree via repo worktree_create: warm done; plugins/ has .git file + p.js, git submodule status clean (initialized), plugins/node_modules (ignored) not)_
      have been double-copied or left half-populated.
- [ ] Time it. Expect ~38 s on this repo; if it feels worse than a cold build, _(NOT VERIFIED 2026-09-30: partial — Not run on the tuicommander repo (would create a worktree in Boss's real repo; forbidden). Analog: 200-file ignored dir fixture warmed in under 1s; 29/09 analog with 60k files ~13s. ~38s expectation on the real repo not measured.)_
      say so rather than living with it.
- [x] Switch Settings → worktree storage to "inside repo" (`.worktrees/`), _(verified 2026-09-29: Disposable repo2 (.gitignore has .worktrees/ cache/), per-repo settings PUT /config/repo-settings worktree_storage=inside-repo + copy_ignored_files=true: repo worktree_create returned in 0.06s, worktree at repo2/.worktrees/in1, warm_artifacts status done, cache/c.bin copied, no nested .worktrees/recursion (find depth 4).)_
      create a worktree, and confirm creation does not hang or recurse — the
      destination's own ignored ancestor must be skipped.
- [x] Confirm Settings and settings search contain no copy-on-write workspace _(verified 2026-09-29: by code/test inspection, tests not executed here: cow.rs:3 says COW workspace clones are gone; rg for copy-on-write/cowMode in src/ finds no Settings or dialog UI. Verified by absence in src/.)_
      toggle, the create dialog has no mechanism or parent-changes picker, and
      the Worktree Manager has no clone badge or Publish action.

## Worktree warming: opt-out, bounded parallelism, sidebar badge — **Rust + frontend, needs a `make dev` restart**

Warming already ran in the background on main; this adds a per-repo/global opt-out,
copies up to four ignored directories at once, reports progress, and gives
`POST /sessions/worktree` the same warm step as the other creation paths.

- [ ] Create a worktree of a repo with a large `node_modules`/`target`. While it
      warms, `repo action=worktree_list` (or `GET /worktrees/paths?path=<repo>`)
      shows that workspace's `warm_artifacts` as `pending` with `phase: "warming"`
      and `copied` climbing toward `total`, then `phase:
      "file_sync_and_setup_script"`, then `done`.
- [ ] The sidebar row shows a **Warming…** badge whose tooltip's copied/total
      moves as directories finish; it clears when the copy finishes (not when the
      Setup Script finishes).
- [ ] Turn off "Warm ignored build directories" for the repo (Settings →
      Repository → Worktree; global default under Settings → Git & GitHub →
      Repository Defaults, expert mode), create a worktree: no badge, no
      `node_modules`/`target`, and the final `warm_artifacts` is `done` with a
      `skipped` reason. Per-repo On/Use global/Off beats the global default.
- [ ] Same opt-out through `POST /sessions/worktree` (HTTP) and MCP
      `repo worktree_create`: identical behaviour to the desktop dialog.
- [ ] Remove a worktree while its chain is still running (big warm or a slow
      Setup Script): the creation flow of a NEW worktree on the same branch is
      not released early or frozen, and the new one's setup status is not wiped.

- [x] After restarting `make dev`, verify Project Progress HTTP controls on the _(obsolete, verified 2026-09-29: Progress pause/resume/clear removed: transport.test.ts:395 lists progress_pause/clear/export as gone; no /progress/pause route in mcp_http/mod.rs:884-890.)_
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
- [x] Provenance, pagination (**Load older …**), and the destructive confirmation _(obsolete, verified 2026-09-29: Feature gone: rg 'Load older|loadOlder' src src-tauri/src only hits src/mobile/components/OutputView.tsx:167 (unrelated); ProgressDialog has no pagination/provenance/confirm.)_
      text, which name the scope and the count and state that existing
      `progress.md` exports are not deleted. Not captured: the seeded set was
      below one page and the confirmations are native `window.confirm` dialogs,
      which a screenshot of the page cannot show.
- [x] While the panel is open, report another event and verify the displayed _(obsolete, verified 2026-09-29: ProgressPanel replaced by ProgressDialog; rg -i 'watermark|unread' src/components/ProgressDialog src-tauri/src/progress -> no matches)_
      watermark stays frozen, the later event remains unread, and exactly one
      toast appears without a duplicate MESSAGES row.

## Project Progress ownership self-edge (story `752-8492`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [x] Register any repository and open Progress. Every project must list its _(verified 2026-09-29: registered fx/repo (its own main workspace): repo progress_list and POST /progress/list answer entries, no project_unavailable; dialog lists them)_
      events. Before the fix, `resolve_owning_project_in` read the repository's
      own main workspace (`worktreePath` == repo root, no `parentRepoPath`) as an
      ownership cycle, so `progress_status` and `progress_list` failed for every
      registered project with
      `project_unavailable: managed workspace ownership cycle` and the panel
      showed nothing but red cards.
- [x] A genuine two-project ownership cycle must still fail closed. _(verified 2026-09-29: by code/test inspection, tests not executed here: Ownership cycle fails closed: src-tauri/src/progress/ownership.rs:71 (project_unavailable ownership cycle) with test assertion at :318)_

## Project Progress export (story `753-9998`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [x] After restarting an isolated `TUIC_APP_INSTANCE`, select a project in the _(obsolete, verified 2026-09-29: Progress export feature removed: transport.test.ts:395 asserts progress_export is gone; rg 'progress_export|ProgressExport' finds only that test line; progress.md only a comment at progress/store.rs:273.)_
  Progress panel, preview `progress.md`, and export it. Verify the preview remains
  usable at desktop and narrow/mobile widths and the file appears at the owning
  project root rather than the active worker workspace.
- [x] Preview an existing `progress.md`, edit it externally, then choose Replace. _(obsolete, verified 2026-09-29: Project Progress export (progress.md Replace) removed: transport.test.ts:395 lists progress_export as gone; no export route in mcp_http/mod.rs:884-890.)_
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

- [x] Open the Progress panel and confirm the **global scope** shows both _(obsolete, verified 2026-09-29: Global scope/workstreams UI absent: rg -i 'global|workstream' src/components/ProgressDialog/ProgressDialog.tsx = no matches (dialog rewritten to one journal, see line 3079).)_
      projects with the right per-project state, and that switching to each
      project scope shows that project's workstreams and its blocked one.
- [x] Report into a **paused** project while the panel is open: the receipt is _(obsolete, verified 2026-09-29: Progress pause/correction removed: rg -i 'pause' src-tauri/src/progress -> no matches; no correction control in src/components/ProgressDialog; transport.test.ts:395 lists progress_pause gone)_
      already proven to say `paused`, but confirm no toast appears either.
- [ ] Correct one event through the panel's correction control (edit a summary) _(NOT VERIFIED 2026-09-30: partial — Obsolete item: no correction/edit-summary control exists (rg -i 'correct|edit summary' in ProgressDialog components and src-tauri/src/progress found no matches); journal API exposes list/delete only. Cannot exercise.)_
      and confirm the panel and a fresh export both show the corrected text.
      This leg was never exercised: the live run covered the workstream rename,
      not an event correction.

## Protocol-ranked agent state (story `745-8ff1`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [x] After restarting `make dev`, run an instrumented agent turn for longer _(verified 2026-09-30: Fake claude binary (agent_type=claude, hook_instrumented) via POST /sessions/agent: emitted OSC 7770 busy, then a bare '❯' Ready repaint with no spinner, silent 20s, then OSC 7770 idle. status stayed busy/working ~22s through the stale Ready, idle only at hook-idle rank=Protocol (log). Fake stands i)_
      than the ordinary silence threshold. A stale Ready repaint must not turn
      the tab idle before the agent's protocol completion signal arrives.
- [ ] Disable native/global status instrumentation for one agent and confirm _(NOT VERIFIED 2026-09-30: partial — Fake codex (no hook OSC; 'Working (Ns • esc to interrupt)' then '› Ask Codex' prompt) with PUT /config/agents/codex/native-status-signals enabled=false: status busy then idle ~13s after work ended. Log attributes the idle close to activity_source=process rank=Process, not ready-screen. Same with sig)_
      its existing Ready-screen fallback still returns the tab to idle.

## Vendored fxhash in the bm25 fork (story `758-ff0d`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [ ] Before restarting, note a repo you have searched recently — its content-index _(NOT VERIFIED 2026-09-30: partial — Cannot recreate a pre-vendoring snapshot (no old binary). Own daemon r3iso: /fs/search-content on fixture repo -> 'content index built', results returned; after kill+restart of the daemon the same search logged 'content index built' again (no snapshot written/restored on abrupt stop), so the restore)_
      snapshot on disk was written by the pre-vendoring binary. After the restart,
      run a content search in that repo (`?` in the command palette) for a word you
      know is in it. Results must appear immediately, with `GET :9876/logs` showing
      the snapshot being restored and NOT `content index rebuilt` for that repo. An
      empty result set with a successful restore is the exact failure the vendoring
      had to avoid: the persisted `token.index` values are fxhash32 hashes, so a
      drifted algorithm still decodes the file and then matches nothing.

## Progress Markdown export (story `753-9998`, 2026-09-13) — **OBSOLETE, do not run**

_(NOTE 2026-09-18: the feature every item below tests no longer exists. `6b925e04`
deleted `src-tauri/src/progress/export.rs` with its routes and `EXPORT_LOCK`, and
replaced the Progress **panel** with `ProgressDialog` — which has no export card,
no source-metadata checkbox and no preview. `src/components/` holds only
`ProgressDialog`, and no `/progress/export*` route survives in `mcp_http/mod.rs`.
Kept for history; the five items are unrunnable, not pending.)_

Automated verification already covers the backend contract end to end (unit
tests plus a live HTTP run against a rebuilt debug instance on `:9877`:
preview → write → `progress_export_exists` → `progress_export_content_changed`
with the human edit preserved). What is left is what HTTP cannot observe.

- [x] Open the Progress panel in the desktop app, pick one project, and check _(obsolete, verified 2026-09-29: Export card removed: rg 'Include source metadata|Replace progress' src src-tauri/src = no matches; section header itself says OBSOLETE.)_
      the export card against `docs/frontend/STYLE_GUIDE.md`: the source-metadata
      checkbox, the preview button, the revision line, and the scrolling
      Markdown preview block.
- [x] Toggle "Include source metadata" while a preview is shown. The preview and _(obsolete, verified 2026-09-29: Section marked OBSOLETE; rg 'Include source metadata|Replace progress' src -> no matches; progress_export gone (transport.test.ts:395))_
      its export button must disappear, because that snapshot can no longer be
      written.
- [x] Export once, then export again. The second run must ask for confirmation _(obsolete, verified 2026-09-29: Section header at to-test.md:2940 says OBSOLETE; `rg 'Replace progress' src src-tauri/src` returns no matches (only a comment in progress/store.rs:273).)_
      before replacing the file, and the button must read `Replace progress.md`.
- [x] Edit `progress.md` by hand between the preview and the write, then write. _(obsolete, verified 2026-09-29: Section marked OBSOLETE; `rg progress_export_content_changed src src-tauri/src` -> no matches (progress.md export gone))_
      The panel must show `progress_export_content_changed` and your edit must
      still be in the file.
- [x] After an export, run `git status` in that project: only `progress.md` may _(obsolete, verified 2026-09-29: Progress Markdown export removed (heading says OBSOLETE); rg 'progress_export|ProgressExport' matches only transport.test.ts:395 asserting it is gone.)_
      appear. Nothing under `.tuic/` may be listed.

## MCP instruction de-duplication (#754-affa) — needs a `make dev` restart

Rust-only change to `mcp_transport.rs`. Boss's live instance still serves the old
strings until the backend is restarted; nothing below can be checked before that.

- [x] After restart, `curl -s localhost:9876/mcp/instructions | jq -r .instructions`. _(verified 2026-09-29: GET /mcp/instructions (unix socket): ## Tools holds the delegation line, Worktrees rule and Submit rule, no per-tool bullets, no ## Workflow, no UI feedback line. NOTE: ## Multi-Agent Work also keeps a Mail bullet besides the peer count and isolated-branches bullet)_
      The `## Tools` section must hold three lines (the delegation sentence, the
      Worktrees rule, the Submit rule) and **no** per-tool bullet list; there must
      be no `## Workflow` section and no `**UI feedback:**` line. `## Multi-Agent
      Work` keeps the peer count and the isolated-branches bullet only.
- [x] `ack` / `intent:` / `suggest:` markers must be byte-identical to before — _(verified 2026-09-29: ack, intent and suggest marker lines present verbatim in /mcp/instructions (comparison with the old capture not possible))_
      they are protocol, and a reworded marker breaks the tab title and the
      suggestion bar. Compare against a capture of the old output if in doubt.
- [x] In a connected agent, ask for the `repo` tool schema: its description must _(obsolete, verified 2026-09-29: repo tool now has only progress_list: REPO_ACTIONS at mcp_http/mcp_transport.rs:1148 lists a single progress_* action, not nine; text obsolete.)_
      now document all nine `progress_*` actions, which it never did before.
- [x] Watch one agent session for a turn. It must still emit `ack` exactly once _(verified 2026-09-30: Real claude -p (stream-json, --mcp-config stdio bridge to instance MCP socket) at r3 registered repo: first assistant text 'TUICommander v1.7.7 is connected.' then 'intent: Listing the current directory (List directory)' (ack once, intent at the phase), then progress done, suggest: [A|B|C]. One turn)_
      per connection and `intent:` at each phase change — the markers moved not
      at all, but this is the cheapest way to notice if they did.

## Progress reachable from `tuic-remote` (#755-35c8 finding) — needs a `make dev` restart

Rust-only routing change in `mcp_http/mod.rs`: the ten `/progress/*` routes moved
from `build_router` into `shared_routes()`. Before this, a remote/PWA client
talking to a `tuic-remote` daemon got **404 on the whole Progress feature** — no
route at all, which looked like an auth failure. Tests cover route existence;
these check the live surface.

- [x] Start a headless daemon: `TUIC_APP_INSTANCE=remote-check tuic-remote`, then
      `curl -u <user>:<pass> -X POST 'http://127.0.0.1:<port>/progress/list'`.
      It must answer with a list body, not 404.
      _(NOTE 2026-09-18: `/progress/status` was deleted by `6b925e04` — probing it
      returns 404 for that reason, not a routing regression. The ten routes are now
      four: `/progress/{report,list,delete,viewed}`, all POST, and all still inside
      `shared_routes()` at `mcp_http/mod.rs:708-723`, so the property this item
      exists to protect is intact. Use `/progress/list`.)_
      _(verified 2026-09-20 against a headless `tuic-remote` on mac-mint:
      `POST /progress/list?path=/home/stefano` with credentials answers **200**
      `{"project":"/home/stefano","entries":[]}`. The control that makes this mean
      something: `POST /progress/nonexistent` with the same credentials answers **404**,
      so the 200 is a registered route and not a catch-all.)_
- [x] Same call with **no** credentials from a non-loopback address must still be
      rejected by the auth middleware — the move must not have widened access.
      _(verified 2026-09-20: the same POST from this Mac to mac-mint — a genuine
      non-loopback peer — answers **401** with no credentials and **401** with a wrong
      password, while `GET /health` answers 200 unauthenticated, which is the one route
      documented as open. The headless build has no loopback bypass, so this also holds
      from the daemon's own localhost.)_
- [x] On the desktop instance, the Progress **dialog** must behave exactly as _(verified 2026-09-29: Web UI :9880: Progress dialog opens from bell popover, lists entries, updates live when POST /progress/report adds one (entry id 58 appeared while open), list/report/viewed calls succeed; no 404 (dialog renders entries, logs show no progress errors).)_
      before: the routes are merged into `build_router` through `shared_routes()`
      now, so a regression here shows up as the dialog 404ing on every call.
      _(NOTE 2026-09-18: "panel" — `ProgressPanel` was replaced by `ProgressDialog`
      in `6b925e04`. Same check, different surface.)_

## A turn closed by the foreground probe logs `activity_source=process` — needs a `make dev` restart

`foreground_probe` never constructed `ForegroundProbe::Quiet`, so every close
that the process table actually answered was logged as `agent-ready-screen`,
indistinguishable from a screen-only guess (#771-4733). Rust-only — the running
app keeps the old logging until restart.

- [x] After restart, let an agent tab finish a turn with nothing running under _(verified 2026-09-30: Goose turn (agent_type=goose, spawned via POST /sessions/agent) after 'reply with the word ok': /logs 'Shell state → idle' data activity_source=process rank=Some(Process) for that session at turn end (also at startup), not agent-ready-screen. Codex/claude not used (trust dialogs).)_
      it, then `curl 'http://localhost:9876/logs' | grep 'Shell state'`: the
      close must read `activity_source=process rank=Process`, not
      `agent-ready-screen`.
- [x] A tab whose agent still has a `cargo`/`npm` child running when the ready _(verified 2026-09-29: by code/test inspection, tests not executed here: Covered by pty/tests.rs:4617-4686 asserting source stays 'agent-ready-screen' with a child process running)_
      screen appears must still close as `agent-ready-screen` — the probe must
      not claim an observation it did not make.
- [ ] After such a close, typing into that tab (or the agent resuming on its _(NOTE 2026-09-29: partial evidence only — activity_source=process test: pty/tests.rs:4626 (must name the probe). Busy-on-typing recovery not confirmed by a named test; verify or add.)_ _(NOT VERIFIED 2026-09-30: partial — Same goose session: after the process-ranked idle close, typing a prompt+Enter logged 'Shell state → busy' activity_source=user-submit rank=Protocol (idle->busy recovered; status busy/working 19 samples). NOT tested: agent resuming on its own.)_
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

- [x] Type a draft into the orchestrator's composer (do not submit), have a peer _(verified 2026-09-30: Fake claude orchestrator (agent_type=claude, OSC 7770 hooks, register orchestrator=true). Typed 'draftx' (no Enter), peer agent send -> delivery_path=inbox_only, nothing typed. Cleared draft + submitted 'go': after busy->idle edge the fake received '[TUIC] message available — read it with: agent act)_
      `agent action=send` to it, then clear the draft and let the turn settle.
      Within a few seconds the orchestrator must be handed the
      `agent action=inbox` line. Before the fix nothing ever arrived.
- [x] The payload must never appear on the orchestrator's screen — only the _(verified 2026-09-30: Same run: composer log and screen output never contained the payload 'PAYLOAD-SECRET-r3-1' (grep -c = 0); only the '[TUIC] message available — read it with: agent action=inbox' pointer was typed. Payload only via agent inbox.)_
      pointer to the inbox.
- [ ] A notice already being typed must not be duplicated by a concurrent idle _(NOT VERIFIED 2026-09-30: partial — Draft typed, two peers (r3-b, r3-c) sent two messages (both inbox_only), draft cleared, one idle edge: exactly one wake line typed (not two). Exact race of notice being typed at the same instant as an idle edge not constructed.)_
      edge: one wake per group, not two.

## Progress rewritten to one journal, one database and a dialog — needs a `make dev` restart

The whole Progress feature was re-implemented against the 2026-09-14 revision of
`plans/project-progress.md`: one append-only journal in a single database at
`<config dir>/progress.sqlite3`, two reportable kinds plus a host-written
`intent`, a dialog replacing the sidebar panel, eight `repo progress_*` actions
cut, and no Markdown export. Rust and frontend both changed, so the running
build has the old behaviour until restart.

- [x] After restart, `progress.sqlite3` must exist in the config directory, and _(verified 2026-09-29: progress.sqlite3 exists in instances/validate; ls -a fx/repo shows no .tuic dir after progress writes)_
      no *new* `.tuic/` directory may appear in any repository. The 42 existing
      ones are stale leftovers of the old design — see the cleanup item below.
- [ ] Ask an agent to report: the entry must appear in the dialog with its agent _(NOT VERIFIED 2026-09-30: partial — Real claude report (progress type=done via MCP) -> repo progress_list entry type=done agentName=rep-1 ptyId set; intent entries type=intent exist alongside (fake agent marker 'Checking the marker ONTAG'). Dialog rendering (agent name, muted intent styling) not seen: needs_browser.)_
      name, and an `intent:` marker from any agent tab must appear as a muted
      `intent` entry in the same list.
- [x] An agent calling `progress` with `type=intent` must be refused, naming _(verified 2026-09-29: by code/test inspection, tests not executed here: Test: progress/model.rs:309 parse_reportable('intent') is an error; MCP path test mcp_transport.rs:16890 sends type=intent.)_
      `done` or `blocked`.
- [x] Open the dialog on a project with history, note the divider, let a new _(verified 2026-09-29: Web UI Progress dialog (All repo): seeded via POST /progress/report. Open with new A: 'Seen before' divider below A. B reported while open: rows B,A,divider - divider stayed below A. Close+reopen: no divider (nothing new; code draws none for index 0). Report C closed, reopen: divider between C and B (B,A now read).)_
      entry arrive: the divider must NOT move while the dialog is open. Close
      and reopen: it must now sit above the entries just read.
- [x] Settings → Agents → **Collect project progress** off: the `progress` tool _(verified 2026-09-30: On own daemon (r3iso): PUT /config progress_tracking=false -> fresh MCP peer tools/list lost 'progress' (10->9); fake claude PTY intent line not journaled (on: journaled). Per-agent agents.claude.progress_tracking=false: tool still listed, progress done -> 'progress_tracking_disabled: Progress colle)_
      must disappear from a newly-connected agent's tool list, and `intent:`
      markers must stop being recorded. Per-agent **Collect progress** off must
      instead answer `progress_tracking_disabled` on a report.
- [x] `repo action=progress_list` must still work; `progress_status`, _(verified 2026-09-29: by code/test inspection, tests not executed here: src/__tests__/transport.test.ts:395 asserts progress_status/pause/clear/export are gone; progress_list still in REPO_ACTIONS (mcp_transport.rs:1148))_
      `progress_pause`, `progress_clear` and `progress_export` must be gone.
- [ ] The mobile PWA's Progress tab must render the same list full-bleed. _(NOT VERIFIED 2026-09-30: blocked — real phone (mobile PWA Progress tab full-bleed rendering))_
- [ ] **[HUMAN]** Screenshot check against `docs/frontend/STYLE_GUIDE.md`: _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: [HUMAN] screenshot comparison of the Progress dialog against docs/frontend/STYLE_GUIDE.md: needs a rendered UI (no frontend here); no audio hardware involved.)_
      blocked entries red, `intent` muted and italic, the divider legible, the
      delete button appearing on row hover.
- [ ] **[HUMAN]** Narrow the window to ~480px with a long entry on screen: the _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs a rendered UI narrowed to ~480px with a long Progress entry; headless instance has no frontend.)_
      dialog must stay readable — it is `min(680px, 100vw - 48px)` wide and the
      text wraps with `overflow-wrap: anywhere` — and the header must keep the
      blocked-only toggle and the close button on one row (778-a9a6 criterion 8).
- [ ] After the restart has proved the new store works, delete the stale _(NOT VERIFIED 2026-09-30: partial — Real deletion left to Boss (destructive on ~/Gits). Ran the two exact find commands on a simulated tree (a/.tuic with progress.sqlite3{,-wal,-shm}; b/.tuic with progress.sqlite3 + tunnels/t.json; c/x/.tuic): files deleted, empty .tuic dirs removed, b/.tuic/tunnels kept. Read-only count of real match)_
      per-repo databases. They are not migrated by design. Delete the **files**,
      not the directory:
      `find ~/Gits -maxdepth 5 -path '*/.tuic/progress.sqlite3*' -delete`
      then drop the directories that this leaves empty:
      `find ~/Gits -maxdepth 4 -type d -name .tuic -empty -delete`
      _(NOTE 2026-09-18: the previous `rm -rf` on the whole `.tuic/` directory is
      wrong even though it happens to be harmless today. `.tuic/` is a live
      namespace — `tunnels/storage.rs:33,59,69` writes `<repo>/.tuic/tunnels/` —
      so the blanket delete destroys tunnel storage for any repo that has one.
      Verified 2026-09-18: 43 `.tuic` directories (not 42; `agent2__wt/`
      `analysis-ai-risk-score-20260918` is new), 0 contain `tunnels/`, and every
      file in all 43 matches `progress.sqlite3*`.)_
- [ ] Run diff-scoped mutation testing once on the final HEAD of this batch: _(NOT VERIFIED 2026-09-30: partial — Not run: diff-scoped mutation testing (make mutants) is an overnight cargo job (~5 min/mutant) and cargo/build is forbidden for this pass. Orchestrator batch task.)_
      `make mutants RANGE=<commit before the Progress rewrite>`. It is an
      overnight-class job (~5 min per viable mutant), so it is deliberately not
      run during the day — 780-e99a criterion 4.
- [x] Bring the worktree build up on `:9877` and exercise Progress through its _(verified 2026-09-29: On isolated validate instance (unix socket = its own HTTP router, not :9876): POST /sessions in fx/repo, MCP progress type=done -> {id:29}; POST /progress/report?path= -> {id:30}; POST /progress/list?path= and repo progress_list returned both entries (ptyId, agentName); DELETE /sessions ok.)_
      own HTTP instance — creating a throwaway session, reporting, listing and
      deleting — rather than against the orchestrator on `:9876`.

## TypeScript mutation tooling (story `944-15f3`, 2026-09-25)

- [x] Run the narrow canary from `docs/guides/development-setup.md` and confirm
      Stryker reports the `pathBasename` condition mutant as `Killed`, with at
      least one Vitest test executed against it. _(verified: 2026-09-25;
      `scripts/ts-mutants.mjs` on `pathUtils.ts:65-65` reported 6 Killed, 0
      Survived, and 1.00 tests per mutant; JSON saved under
      `~/Gits/.tmp/results/ts-mutation-gate/mutation.json`.)_
- [x] On the next changed TypeScript source/test pair, run the same scoped _(verified 2026-09-29: Ran node scripts/ts-mutants.mjs 'src/utils/panelSync.ts:30-40' -- src/__tests__/utils/panelSync.test.ts (changed pair from 8b3e740d2) with TMPDIR set: completes in 1m26s, 20 mutants (8 killed, 2 timeout, 8 survived), score 55.56, 0.89 tests/mutant, mutation.json written. Removed reports/mutation after.)_
      command before using its mutation score as a story gate.
- [x] After the StoriesDialog dependency-removal change is present in this _(verified 2026-09-29: ts-mutants.mjs on StoriesDialog.tsx:532 (story row onClick) with StoriesDialog.test.tsx: 1 mutant, Killed, 100% score, 1.00 tests/mutant. Only that one click handler was mutated; the original false-survivor identity not confirmed.)_
      checkout, run its targeted test through `scripts/ts-mutants.mjs` and
      verify the previously false-surviving click-handler mutant is `Killed`
      (story `944-15f3`).

## File pickers moved off `tauri-plugin-dialog` — needs a `make dev` restart

The app died on 2026-09-18 when `+[NSOpenPanel openPanel]` returned NULL after
the window-server connection was interrupted, panicking on the main thread. The
pickers now go through our own `pick_path` command, which owns the main-thread
closure and catches that unwind (`src-tauri/src/native_dialog.rs`). Automated
tests cover the wire contract and the wrapper's mapping; the panels themselves
need a window server, so these are by hand.

- [ ] Sidebar → add a repository: the folder picker opens, a pick registers the _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Native OS folder picker (rfd) in the desktop app; no window server/frontend on the headless instance. Needs desktop build + macOS UI automation.)_
      repo, and Cancel leaves the sidebar unchanged.
- [ ] Cmd+O (open file) and the open-folder action: both return a path and the _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Native Cmd+O / open-folder dialog: desktop build only, not drivable over HTTP/MCP.)_
      chosen file opens in an editor tab.
- [ ] New File (save panel): the suggested name is pre-filled and the file is _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Native New File save panel (suggested name pre-filled): desktop build only.)_
      created at the chosen location.
- [ ] Settings → Plugins → Install from ZIP: the type filter still restricts the _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Native Install-from-ZIP file dialog and its .zip type filter: desktop build only.)_
      selection to `.zip` — that filter is the one option most likely to have
      been dropped in the move.
- [ ] Settings → Plugins → Install from Folder, and Tunnels → the SSH identity _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Native Install-from-Folder and SSH identity file pickers: desktop build only.)_
      file browse button: both still pick.
- [ ] **[HUMAN]** The crash path itself: let the Mac sleep with the display off, _(NOT VERIFIED 2026-09-30: partial — Not run: needs real Mac sleep with display off plus the desktop app picker; sleeping the shared host Mac would disturb other agents. No audio hardware involved.)_
      wake it, and immediately open a picker. It must either open, or show the
      "system file dialog is unavailable" error — the app must NOT exit. This
      needs real standby, which no automated check here can reach.

## Remote Machines authentication (#781-9652) — needs a `make dev` restart

The whole change is Rust plus the transport layer, so nothing here is live in
Boss's running session. Restart first. A daemon to test against is already up:
`mac-mint:9877`, user `stefano`, systemd user unit `tuic-remote`, running a
headless build of this tree.

- [ ] Settings → Services → Remote Machines → add a Direct connection to _(NOT VERIFIED 2026-09-30: partial — Stand-in local daemon http://127.0.0.1:9893 as Direct conn with correct user/password: status connected (not unauthenticated), protocol_version 1. Form/UI flow (Settings > Remote Machines) and mac-mint:9877 not exercised.)_
      `http://mac-mint:9877` with the username and password. Connect: the status
      goes **Connected**, not "Not authenticated".
- [ ] Same connection with a wrong password: the status reads **Not _(NOT VERIFIED 2026-09-30: partial — API: wrong password -> status 'unauthenticated' + 'Authentication rejected by the remote daemon'. UI wording 'rejected these credentials' / amber badge / no calls routed need the frontend (needs_browser).)_
      authenticated** with "rejected these credentials", stays amber rather than
      red, and no terminal or repo call goes through.
- [x] Edit an existing connection: the password field shows the "stored — leave _(verified 2026-09-29: Web UI edit of Direct conn: password input placeholder 'Password (stored — leave blank to keep it)'. Saved with blank password (status drops to [] until Connect), pressed Connect -> status connected, no error: vault password kept.)_
      blank to keep it" placeholder, and saving with it blank keeps the
      connection working.
- [ ] Restart `tuic-remote` on mac-mint under a live connection. The daemon mints _(NOT VERIFIED 2026-09-30: partial — Stand-in: local second tuic-remote (port 9893) instead of mac-mint. Killed+restarted under live connection: error(Unreachable) then connected within ~2s (<5s poll), new token, no manual action. mac-mint itself not available (second machine).)_
      a new token; within one poll (5s) the connection re-authenticates by itself
      and stays Connected.
- [ ] **[VISUAL]** The password field and the vault hint render inside the _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
      add/edit form without breaking the Settings layout.

## Remote repos run on the remote machine (#782-3d05) — needs a `make dev` restart

Frontend-only, but it changes where every repo-scoped call goes, so it needs the
same restart as the item above and the same daemon (`mac-mint:9877`).

- [ ] With the connection Connected, add a remote repo from it. The sidebar shows _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Frontend flow (add remote repo, sidebar badge, git status compare). Backend stand-in exists: second local tuic-remote (Direct conn) worked for connect/mirror; comparing against 'ssh mac-mint git status' needs the second machine.)_
      the repo with its remote badge and the git status, branch and file tree are
      the **remote machine's** — compare against `ssh mac-mint git -C <path> status`.
- [ ] Open a terminal on that repo. It spawns on mac-mint: `hostname` and `pwd` _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Frontend flow: terminal tab on a remote repo (hostname/pwd, resize, close ends remote session). Backend mirror of remote sessions verified with a local second daemon (session-created/state events); the tab needs the UI. mac-mint itself not available.)_
      answer for the remote box, typing and resizing work, and closing the tab
      ends the session there (`ssh mac-mint` + check the daemon's `/sessions`).
- [ ] Edit a file on mac-mint by hand while the repo is open locally. The git _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Frontend refresh of git panel/file tree from remote watcher SSE. Backend half verified: second daemon repo-changed reaches the local /events bus with __tuic_origin; panel refresh needs the UI.)_
      panel and file tree refresh by themselves — the remote watcher and its SSE
      stream are doing it.
- [ ] Commit and stage from the git panel on the remote repo. The commit lands on _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Commit/stage from the git panel on a remote repo: frontend action; needs the UI and a remote host (local second daemon can stand in).)_
      mac-mint, not on any local repo.
- [ ] A local repo behaves exactly as before — no extra latency, no remote call. _(NOT VERIFIED 2026-09-30: partial — Local repo: GET /logs?source=network on the instance is [] after local repo/session activity (no remote call logged). Latency/'exactly as before' comparison and UI not measured.)_
      Confirm with `GET http://localhost:9876/logs?source=network`.
- [ ] Stop `tuic-remote` on mac-mint with the repo still open. Repo operations _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Frontend: repo operations with the daemon stopped must say 'Remote connection … not connected'. Backend: killing the daemon flips status to error(Unreachable) within a poll; UI message not seen.)_
      report "Remote connection … not connected" rather than showing local data.
- [x] Open a file from the remote repo in the editor and use Go to definition. _(verified 2026-09-29: Local tuic-remote as Direct machine; remote repo added via picker (fx/agb2/rr), opened a.rs from File Browser in editor: /logs?source=network has warn '"mdkb_outline" has no remote route and ran on the local machine' (once). Go to definition itself not invoked (keys do not reach page); the log line is triggered by opening the file.)_
      mdkb has no remote route, so it runs locally against a path this machine
      does not have and logs `has no remote route and ran on the local machine`
      once — check `GET http://localhost:9876/logs?source=network`. The log line
      is the thing under test; the feature itself is a known gap.

## The connection runtime moved to Rust (#790-ef85) — needs a `make dev` restart

The health probe, the token exchange, the 5s status poll and the SSH tunnel now
run in the backend; `remoteConnections.ts` only renders what the backend pushes.
Every item below must behave exactly as it did before the move — that is the
point of the story — plus the two things only the backend can do.

- [ ] Connect a remote machine from Settings → Remote Machines. The status goes _(NOT VERIFIED 2026-09-30: partial — Direct conn to local second daemon: connect -> status connected with protocol_version=1 (+build). Intermediate 'connecting' state not sampled; Settings panel rendering not seen (headless).)_
      Connecting → Connected and the panel shows the protocol version.
- [ ] Connect the same machine from a second client (browser at _(NOT VERIFIED 2026-09-30: partial — Two concurrent /events SSE clients (unix socket) both received the same 6 'remote-connection-status' events during daemon restart (error -> connected). No desktop window/browser panels compared side by side.)_
      `http://localhost:9876/`) while the desktop app is open. **Both** panels
      show Connected: the status is pushed to every client, not owned by the one
      that clicked.
- [ ] A wrong password reports `unauthenticated` (not "connection error") and no _(NOT VERIFIED 2026-09-30: partial — Wrong password: POST /connect -> {'error':'Authentication rejected by the remote daemon'}; GET /config/remote-connections/status = status 'unauthenticated' (stable 6 samples), no base_url/token. NOT tested: that no repo/terminal call is routed while unauthenticated.)_
      repo or terminal call is routed to that machine while it is in that state.
- [x] Restart `tuic-remote` under a live connection: within one poll the _(verified 2026-09-30: Own tuic-remote (--instance r3iso, TUIC_PORT=9893, bcrypt pw) as Direct connection via PUT /config/remote-connections + /password + POST /connect: status connected with protocol_version=1. Killed+restarted the daemon: status error(Unreachable) then connected again ~2s after start, no user action; in)_
      connection re-authenticates by itself and stays Connected. This is now a
      Rust task, so it keeps working with the TUICommander window closed to the
      tray or the WebView asleep — the case the WebView implementation lost.
- [ ] Disconnect: the tab's remote sessions stop being routed, the SSH tunnel is _(NOT VERIFIED 2026-09-30: partial — Direct transport: DELETE /config/remote-connections/{id}/connect -> ok, status list empties. SSH tunnel/ps ssh/__remote_* Tunnels profile parts need an SSH transport to a second machine: blocked (second physical machine).)_
      gone (`ps aux | grep ssh` on this machine), and the Tunnels panel shows no
      leftover `__remote_*` profile — the tunnel is built in memory now.
- [ ] Quit TUICommander with an SSH-transport connection live. No orphan `ssh` _(NOT VERIFIED 2026-09-30: blocked — Second physical machine (SSH transport to mac-mint) plus quitting the desktop app; neither available headless.)_
      process and no `__remote_<id>` profile file is left behind.

## Remote sessions report idle / busy / question (#791-055e) — needs a `make dev` restart

`remote_mirror.rs` now follows the remote daemon's own `/events` and repeats
every frame on the local bus under the daemon's own name, and the WebView's
`remoteEventBridge.ts` is gone with it. Nothing on the frontend subscribes to a
remote machine any more.

Verified 2026-09-19 against a **real second daemon** — `tuic-remote --instance
mirrortest` on `:9899` with its own password, reached over a Direct connection
from the running desktop on `:9876`. Not a mock: a separate process, real auth,
real SSE. Torn down afterwards (connection deleted, vault entry cleared, instance
dir removed).

- [x] The dot goes busy while a remote session runs and idle when it stops.
      _(verified: a session created on the `:9899` daemon appeared in the
      desktop's `GET /sessions` as `3b19ac21 | connection_id=1111…5555 | shell=idle`;
      writing a 5-second loop to it produced, on the **desktop's own**
      `/events?types=session-state-changed`, the sequence idle → busy → idle —
      11 frames, under the ordinary event name and matched by the ordinary type
      filter, which is exactly what `applySessionStateEvent` consumes.)_
- [x] A daemon restart under a live connection re-seeds instead of leaving stale
      rows. _(verified: two mirrored sessions before; killed and restarted the
      daemon; the connection re-authenticated by itself with a new token and the
      list came back holding only the one session the restarted daemon actually
      has.)_
- [x] Disconnecting clears the badges and drops the sessions.
      _(verified: `DELETE …/connect` published
      `session-closed {"session_id":"3b19ac21…","reason":"remote-disconnected"}`
      on the local `/events`, and `GET /sessions` then held no row with a
      `connection_id`.)_
- [ ] Make a remote **agent** ask a question. The question badge and the _(NOT VERIFIED 2026-09-30: partial — Second daemon (:9893) Direct-connected; fake claude agent there (OSC awaiting + 'Do you want to proceed?') -> local /events (unix socket) carried session-state-changed agent_state=awaiting_input awaiting_input=true question_text='Do you want to proceed?' with __tuic_origin; after answering on remote)_
      notification are the same ones a local agent raises. Answer it: the badge
      clears. _(Not covered above: the probe drove a plain shell, so
      `awaiting_input` never moved. Needs a real agent on the other machine.)_
- [ ] While the remote agent is mid-turn, queue a command from the Compose _(NOT VERIFIED 2026-09-30: partial — Mirror side only: remote fake agent mid-turn -> local /events session-state-changed shell_state=busy agent_state=working, idle at end; GET /sessions on local lists the mirrored row with connection_id. Compose-panel 'N queued' badge and queue gate are frontend: not exercised.)_
      panel. The `N queued` badge moves and the command is delivered at the
      agent's next idle window — the queue gate reads the mirrored state.
- [ ] Commit something on the remote repo from a shell there. The local panels _(NOT VERIFIED 2026-09-30: partial — Registered repo watcher on second daemon (POST /watchers/repo), committed from git in that repo: local /events?types=repo-changed received {repo_path,kind:git-state,__tuic_origin.connection}. Local panel refresh is frontend: not exercised.)_
      for that repo still refresh (this used to come from `remoteEventBridge.ts`;
      it now arrives on the mirrored `repo-changed`, through the same coalescer a
      local change uses). Needs a repo registered on the remote machine.
- [x] With no remote connection configured at all, nothing changes.
      _(verified 2026-09-19 on the restarted build: no `connections.json` exists,
      `GET /sessions` returns the 4 local rows with their state and **no**
      `connection_id` field on any of them — the mirror adds nothing when there
      is nothing to mirror.)_

## Ideas panel: queue instead of typing, and a shorter Compose panel

- [ ] Open an agent tab and the Ideas panel. Each idea shows a queue button _(NOT VERIFIED 2026-09-29: partial — Web UI Ideas panel on a plain shell tab: each idea shows only pencil (Edit idea), ▶ (Send to terminal), ✕; no queue button. Agent-type tab (queue button present) not testable: no real agent CLI tab.)_
      (stacked lines) left of the ▶ send button. On a plain shell tab the queue
      button is absent and only ▶ remains.
- [ ] Click queue while the agent is mid-turn: nothing is typed into the prompt, _(NOT VERIFIED 2026-09-29: Needs a real agent mid-turn to observe queue-on-busy and idle delivery)_
      the Compose `N queued` badge goes up by one, and the idea gets its used
      timestamp. The idea is delivered at the agent's next idle window.
- [ ] Detach the Ideas panel to its own window. The queue button is always shown _(NOT VERIFIED 2026-09-29: blocked — Needs desktop detached Ideas window plus maccontrol click/focus observation; detached windows and focus are not observable via HTTP/MCP; browser and maccontrol not permitted for this run.)_
      there; clicking it with a plain shell active raises the "not running an
      agent" toast in the main window instead of queueing, and does NOT steal
      focus back to the main window (unlike ▶, which does).
- [ ] The Compose panel is visibly shorter (160px, was 200px) and still fits the _(NOT VERIFIED 2026-09-29: partial — Compose panel not openable in web UI on a shell tab (palette 'Toggle compose panel' rendered nothing; needs agent session). Code disagrees with item: ComposePanel.module.css .panel height 142px (item says 160px) and .queueList max-height 78px (item says 96px) - item text may be stale; no runtime measurement, queue with several commands not producib)_
      editor, the status bar and the buttons. Open the queue list with several
      queued commands: the list caps at 96px and the editor keeps usable rows.

## Updater — symlinked binary path (2026-09-19)

- [ ] Settings -> General -> Updates -> Check Now, on a build whose binary sits _(NOT VERIFIED 2026-09-29: Tauri updater Check Now UI is desktop-only (not in web mode) and needs a symlinked build; maccontrol lacks screen access.)_
      under a symlinked path (a `make dev` build: `src-tauri/target` is an mbx
      target view). It must print a muted "In-app updates are unavailable…"
      hint and NOT the red "Update failed" dialog nor the red hint.
- [ ] The same build on a release install with no symlink in the path still _(NOT VERIFIED 2026-09-29: Needs a release install (symlink-free path) updater check)_
      reports "You are on the latest version" or the available version.
- [ ] After the Notes→Ideas rename: existing ideas still load. The store reads _(NOT VERIFIED 2026-09-29: partial — Web UI Ideas panel: added 2 ideas -> instance notes.json 'notes' array written (same file/store); edit (blur commit) and delete work and persist to notes.json ([] after delete). Instance had no pre-existing notes.json so 'existing ideas load/count' not checked; reassign, image paste (note-images), detach/re-dock not done.)_
      the same `notes.json` through the same `load_notes`/`save_notes` commands,
      so nothing should have moved — but this is the one failure that would be
      silent and lossy, so open the panel and count the ideas before trusting it.
      Add, edit, reassign and delete one; paste an image (assets still land in
      `note-images/<id>/`); detach the panel and re-dock it.

## MCP 2026-07-28 stateless lifecycle — needs a `make dev` restart (#843d)

Rust-only change: it is NOT live in the running session until the backend is
rebuilt.

- [x] `curl -s localhost:9876/mcp -H 'content-type: application/json' -d @src-tauri/src/mcp_http/fixtures/ego_server_discover.json` _(verified 2026-09-29: tuic-remote --instance validate, MCP unix socket: discover fixture returned resultType=complete, supportedVersions [2026-07-28,2025-11-25,2025-03-26], capabilities.tools.listChanged, instructions, no mcp-session-id header; script t3.py)_
      returns a `result` with `resultType: "complete"`, `supportedVersions`,
      `capabilities.tools.listChanged`, `instructions`, and NO `mcp-session-id`
      response header.
- [x] Claude Code (the legacy `initialize` path) still connects and still lists _(verified 2026-09-29: legacy initialize with mcp-session-id then tools/list returned the full surface (session, agent, task, remote, repo, story, progress, ui, plugin_dev_guide, voice); t4.py)_
      the full tool surface — the two lifecycles share one endpoint.
- [x] A `tools/list` carrying `params._meta."io.modelcontextprotocol/clientInfo"` _(verified 2026-09-29: tools/list with _meta clientInfo name=ego returned search_tools, get_tool_schema, call_tool, progress only; t3.py)_
      with `name: "ego"` returns the three meta-tools plus `progress`, and no
      native or upstream definitions.

## One MCP tool family — needs a `make dev` restart (#f6ed)

- [ ] `tools/list` on `:9877` returns exactly `session, agent, task, repo, _(NOT VERIFIED 2026-09-30: partial — item text stale: tools/list now returns session,agent,task,remote,repo,story,progress,ui,plugin_dev_guide,voice; tools/list (x-tuic-session peer, unix socket) returns session, agent, task, remote, repo, story, progress, ui, plugin_dev_guide, voice: differs from i)_
      progress, ui, plugin_dev_guide, config, debug` and no `ai_terminal_*`.
- [x] `call_tool`/`tools/call` with `ai_terminal_read_screen` answers _(verified 2026-09-29: unknown-tool error for ai_terminal_read_screen lists Available: session, agent, task, remote, repo, story, progress, ui, plugin_dev_guide, config, debug, voice, search_tools, get_tool_schema, call_tool; no ai_terminal_*)_
      "Unknown tool", and the message does not advertise `ai_terminal_*`.
- [x] Echo a fake token into a terminal (`echo GITHUB_TOKEN=ghp_…`), then read _(verified 2026-09-30: 80-col session (cols=80), sh with short prompt: 'echo aaaa… GITHUB_TOKEN=ghp_<42 chars>' (101 chars, wraps mid-token). session output default and format=raw both show GITHUB_TOKEN=[REDACTED]; longest leaked substring of the token >=4 chars = 0. Also OK in zsh default prompt. Earlier 29/09 leak not r)_
      it back with `session action=output`: the value must come back
      `[REDACTED]`, in both the default format and `format=raw`.
- [x] A `config.json` still carrying `ai_terminal_mcp_enabled` loads without _(verified 2026-09-29: by code/test inspection, tests not executed here: AppConfig (config.rs:764) derives Deserialize without deny_unknown_fields, so legacy ai_terminal_mcp_enabled is ignored; rg finds no field)_
      error — the field is simply ignored now.

## Bridged `log` records are diagnostic — needs a `make dev` restart

- [ ] With an upstream whose TLS fails (or any dependency logging through the _(NOTE 2026-09-29: partial evidence only — Bridged log records classified at app_logger.rs:97 bridged_log_classification (diagnostic); confirm via that function's tests rather than a live TLS failure.)_ _(NOT VERIFIED 2026-09-30: partial — Same real TLS failure: entries have audience=diagnostic; GET /logs?audience=user has 0 rustls entries (996 user rows), ?audience=diagnostic has them. Error-panel User tab/unseen badge are frontend: not observed.)_
      `log` facade at error level), the error log panel's default **User** tab
      stays clean and the unseen-error badge does not move.
- [ ] The same entries are present under the **Diagnostic** tab, with `source` _(NOT VERIFIED 2026-09-30: partial — API side verified: the TLS-failure entries are audience=diagnostic with source rustls_platform_verifier::verification::apple (not 'log'). Diagnostic tab rendering needs the frontend (needs_browser).)_
      showing the real module (e.g. `rustls_platform_verifier::verification::apple`)
      instead of `log`.
- [x] `GET /logs?source=rustls_platform_verifier::verification::apple` returns _(verified 2026-09-30: Real TLS failure: Direct connection to python https server with self-signed cert (https://localhost:9894), POST /connect -> Unreachable; GET /logs?source=rustls_platform_verifier::verification::apple returns 2+ entries (level error, audience diagnostic, 'localhost certificate is not trusted'); GET /)_
      them; `GET /logs?source=log` returns none.

## The daemon is a whole machine — needs a real `tuic-remote` run (#23a5)

Run on a *separate* box, or on this Mac with `--instance <id>` and after
checking the note below. `tuic-remote` now writes an MCP entry into the config
of every agent installed on the machine it runs on — including this one, whose
agent configs Boss uses. Running an unisolated daemon here rewrites them with
the path it resolves for `tuic-bridge`.

- [ ] Start the daemon and confirm `<config dir>/mcp.sock` exists (Windows: _(NOT VERIFIED 2026-09-29: needs a Windows or Linux host — not reproducible in the isolated headless/browser instance)_
      the `tuicommander-mcp` named pipe) while it runs.
- [ ] Put `tuic-bridge` next to `tuic-remote`, launch an agent in a tab bound to _(NOT VERIFIED 2026-09-29: Needs a real tuic-remote daemon on another host and a real agent listing tools)_
      a repo on that machine, and confirm it lists the `tuicommander` tools —
      `session`, `repo`, `progress`, `agent` — not an empty tool list.
- [ ] `repo action=worktree_list` from that agent answers about the daemon's _(NOT VERIFIED 2026-09-29: partial — Own local tuic-remote daemon: MCP repo action=worktree_list via a bridge on the daemon socket answers from the daemon process (daemon 'repo list' = [] vs validate instance list [fx/repo]); same filesystem so cannot show 'daemon repos not the Mac's' and no real agent/bridge listing tools on a separate host.)_
      repos, not the Mac's.
- [x] Remove `tuic-bridge` from beside the daemon, restart, and confirm the _(verified 2026-09-29: Ran daemon with HOME=fakehome, TUIC_MCP_CONFIG_OWNER=1: with bin/tuic-bridge present ~/.claude.json got command=<bin>/tuic-bridge. Removed bridge, restarted: log 'Skipping agent MCP config updates: no bridge beside this executable', .claude.json stayed {} (no dead path written). Bridge restored.)_
      written config names a path that does not exist (the failure this story
      exists to remove) — then put it back.
- [ ] On a daemon with no agents installed at all: no agent config file and no _(NOT VERIFIED 2026-09-29: partial — Own tuic-remote, empty HOME, no --instance: only codex/grok/opencode/VSCode configs created; no .claude/.cursor/.gemini etc. (skip-if-not-installed works). Cannot test zero agents: has_cli finds /opt/homebrew/bin/{codex,opencode,code}, ~/.grok/bin on this machine, so those count as installed.)_
      agent config directory is created.
- [x] Cross-repo content search (Cmd+P → search file contents) against the _(verified 2026-09-29: Daemon tuic-remote --instance ag3g with repositories.json (1 repo, active): log 'content index pre-warm complete'; GET /fs/search-content-all?query=zebraquokka -> 1 match with repo_path, repos_pending:0, repos_searched:1. Control daemon with no repos: 0. HTTP route, not Cmd+P UI.)_
      daemon returns results for the pre-warmed repo instead of reporting every
      repo pending forever.
- [ ] Leave the daemon running for an hour and confirm the maintenance sweep _(NOT VERIFIED 2026-09-29: Requires an hour-long real tuic-remote soak.)_
      logs reaped MCP sessions rather than growing without bound.

## `tools/list` gained a 2026-07-28 cache envelope — needs a `make dev` restart (#3c1b)

The three new fields are withheld from the legacy revision, so the risk is not
that ego breaks — it is that Claude Code does. Verified in tests; confirm on a
live instance.

- [x] Claude Code connects to the running instance and lists TUIC's tools as _(verified 2026-09-29: legacy-initialized session tools/list result has only key 'tools')_
      before (its `initialize` names 2025-11-25, so its `tools/list` result must
      still carry `tools` and nothing else).
- [x] `curl -s -X POST localhost:9876/mcp -H 'mcp-protocol-version: 2026-07-28' _(verified 2026-09-29: tools/list with mcp-protocol-version: 2026-07-28 returns resultType=complete, ttlMs=0, cacheScope=private beside tools)_
      -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'`
      answers with `resultType`, `ttlMs` and `cacheScope` beside `tools`.
- [x] The same call without the header answers with `tools` alone. _(verified 2026-09-29: same call without the header returns tools only)_

## The embedded AI engine is gone — needs a `make dev` restart (#784-0aec)

~24k lines of Rust and ~44 frontend files were deleted. Nothing below is a new
feature: each item confirms that removing the engine did not take a *surviving*
feature with it. All of it needs the rebuilt backend, so run it after the
restart, not before.

- [x] The app starts with the existing `config.json` and no config backup _(verified 2026-09-30: Headless tuic-remote --instance r3iso started with config.json carrying ai_chat_enabled, ai_triage_enabled, ai_watchers_enabled, ai_terminal_mcp_enabled: starts, /health ok, no config backup file in the instance dir (only config.json, config.json.lock, ai-sessions, logs, worktrees). Desktop app itse)_
      appears beside it (`ai_chat_enabled`, `ai_triage_enabled` and
      `ai_watchers_enabled` are still in Boss's file and must be ignored).
- [x] Settings → General → Experimental Features shows the master toggle alone; _(verified 2026-09-29: Web UI Settings>General>Experimental Features: only 'Enable experimental features' toggle (Expert off and on, experimental on and off); no AI Triage/AI Watchers/AI Chat sub-toggles. Nav gains 'AI Chat' tab when enabled.)_
      the AI Chat, AI Triage and AI Watchers sub-toggles are gone.
- [ ] With the master toggle ON, the AI Chat panel opens and shows the _(NOTE 2026-09-29: rg -i 'moving to' src finds no 'moving to ego' shell string; AI Chat panel is real now; re-write the expectation)_ _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: UI check of the AI Chat panel with the master toggle; 29/09 note: expectation text stale (no 'moving to ego' shell string; panel is real now).)_
      "moving to ego" shell with the focused terminal's name in its header; with
      it OFF the panel, its shortcut and its command-palette entry are absent.
- [ ] SSH Tunnels still opens — it shares that master toggle. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: UI check that SSH Tunnels still opens under the experimental master toggle; no frontend here.)_
- [ ] Settings has no Providers tab and no AI Chat tab, and its search returns _(NOTE 2026-09-29: description stale — an AI Chat settings tab exists (SettingsPanel.tsx:78, settingsSearchIndex.ts:625); re-write the expectation before testing)_ _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Settings UI/search check; 29/09 note: expectation stale (an AI Chat settings tab now exists, SettingsPanel.tsx:78).)_
      nothing for "provider", "triage" or "watcher".
- [x] The toolbar has no watcher eye next to the notification bell. _(verified 2026-09-29: Web UI toolbar: buttons next to the notification bell are 'Smart Prompts Library', a session-finished chip and the bell; only 'watcher'-named element is the Command palette button (class watcherBtn, title 'Command palette (⌘P)'). No watcher eye.)_
- [ ] A PR detail popover opens and shows checks, files and comments with no AI _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: PR detail popover needs the frontend and a real GitHub PR; not exercisable headless.)_
      review section and no error in its place.
- [ ] The GitHub Ops dashboard renders three columns — auto-fix sessions, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: GitHub Ops dashboard rendering; 29/09 note: dashboard now has five columns, item text says three (stale). Needs UI + GitHub-remote repo.)_
      conflict assists, CI/merge readiness — and conflict assist still populates
      its column when a conflicting PR is opened.
- [ ] Smart Prompts still run in shell, inject and headless modes. _(NOT VERIFIED 2026-09-30: partial — Headless mode verified on own daemon: POST /prompt/execute-headless {command: claude, args:[-p,--strict-mcp-config,--settings <hooks>], stdinContent:'reply with the word ok', repoPath} -> "ok" in 9.5s. Shell and inject modes are frontend-driven: not exercised.)_
- [ ] A terminal's command knowledge still records: run a failing command, then _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Headless tuic-remote deliberately does NOT run ai_agent::knowledge::spawn_persist_task (lib.rs:2627 comment); on both rust0930 and r3iso, failing+passing commands in a shell session produced no ai-sessions/*.json. Needs the desktop build (persist task) to check recording and restart survival.)_
      a passing one, and confirm the session's knowledge survives a restart
      (this is the one part of `ai_agent/` that was kept).
- [x] Nothing in the app opens a knowledge-history overlay any more. Its only _(verified 2026-09-29: by code/test inspection, tests not executed here: No knowledge-history overlay opener remains: rg 'KnowledgeHistory|knowledge-history' finds no matches; only an empty comment at App.tsx:1017. ai-sessions still written at ai_agent/knowledge.rs:309.)_
      opener was the chat panel's knowledge footer, so the overlay and its two
      backend commands went with it — `<config_dir>/ai-sessions/*.json` keeps
      filling up with no reader.
- [ ] The AI Chat panel still detaches into its own window and the main window _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Detached AI Chat window + 'Bring back' placeholder are desktop-only (PanelWindowControls).)_
      shows the *Bring back* placeholder; closing the detached window restores
      the docked shell.
- [ ] **Needs a `make dev` restart (Rust).** Load an ego conversation whose _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Ego conversation transcript rendering (no clipped ack) is frontend; ego exists at /usr/local/bin/ego but the transcript UI is needed.)_
      first answer starts with `TUICommander v1.7.7 is connected.` followed by
      `intent:`. The status appears and no clipped acknowledgement such as
      `.7.7 is connected.` remains in the transcript.
- [x] **Needs a `make dev` restart (Rust).** In a tab *you* opened by hand (not _(verified 2026-09-29: Hand-opened shell PTY (POST /sessions cwd=registered repo) e2b29bb4 used as MCP peer: progress type=done -> {id:50} (no project_required); repo progress_list shows entry in that project with ptyId; webview DOM contains toast element (class *_toast > *_message) with the text.)_
      one an orchestrator spawned), an agent calling `progress type=done` no
      longer answers `project_required`: the entry lands in that project's
      journal and a toast appears. `resolve_mcp_origin_repo_path` used to read
      the PTY map under the peer's `$TUIC_SESSION`, which only matches for a
      spawned child. The same fix also gives `ui action=tab` and `ui
      action=toast` the right repo badge in those tabs.
- [x] **Needs a `make dev` restart (Rust).** An agent that writes the ack and its _(verified 2026-09-29: Fake claude-type agent (spawn binary_path, grid resized to run width): 'TUICommander v1.7.7 is connected. intent: ... (Ack Run)' -> display_name 'Ack Run' + journal entry; 3-row and 2-row agent-hard-wrapped variants put (Title) on later row -> title set. 'Ready when you are. intent: x (Prose Reject)' -> no title, no journal row.)_
      first `intent:` as one sentence run (`TUICommander v1.7.7 is connected.
      intent: … (Title)`) now sets the tab title and the Progress journal row
      instead of being dropped entirely; the same for an intent long enough that
      the agent's own wrapping pushes the `(Title)` onto the next row. Prose is
      still rejected — `Ready when you are. intent: x` must NOT set a title.
- [x] **Needs a `make dev` restart (Rust).** In a 120-column agent tab, a long _(verified 2026-09-29: 120col resize: 539-char intent soft-wrapped ~5 rows sets title 'Five Row', one journal row (500 chars, truncated). Titleless 'intent: ... ending in (' then different intent: both entries kept (ids 46,47). Note: two intents in one chunk yield only last (fixture: separate chunks).)_
      `intent:` soft-wrapped across five rows still sets its final `(Title)` and
      writes one truncated journal row. A following different intent must not
      silently erase a previous titleless line ending in an unfinished `(`.

## Remote repo browser (2026-09-20) — frontend only, Vite HMR picks it up

`RemoteRepoPicker` replaces the "type the absolute path" prompt when adding a
repository from a connected machine. No Rust changed, so HMR is enough — but
nothing here is reachable until a remote connection reads **Connected**, which
needs the `make dev` restart that #781-9652 is waiting on.

- [ ] With **no** machine connected, the sidebar `+` must behave exactly as before: _(NOT VERIFIED 2026-09-29: partial — Web UI, machine disconnected (status []): sidebar 'Add Repository' opens a single popover with a path text input (Cancel/Add), no menu/picker. Browser mode cannot show the native dialog, so 'local native dialog' equivalence not checkable; picker absent as required.)_
      straight to the local native dialog, no menu. The picker must not appear.
- [ ] With mac-mint connected, `+` opens the menu; picking it opens the browser _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      showing `/` on **mac-mint**, not this Mac. Compare against
      `ssh mac-mint ls /`.
- [ ] Walk to `/home/stefano/Gits`, press **Add This Folder** on a real repo. It _(NOT VERIFIED 2026-09-29: Needs remote machine with /home/stefano/Gits (second machine).)_
      lands in the sidebar with the remote badge, and its git status is the remote
      machine's.
- [x] Only folders are listed — no files. _(verified 2026-09-29: Web UI: connected local tuic-remote (:9892) as Direct machine, sidebar Add Repository menu (Local Repository | agb2wrong) -> 'Browsing agb2wrong' picker at remote home: 82 rows, all directories (checked vs os.path.isdir), 0 of 149 home files listed; '..' entry and 'Add This Folder'/Cancel present.)_
- [ ] Type a path that does not exist on mac-mint into the field and press Enter: _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      the daemon's own message must show, not an empty folder.
- [ ] Close the picker and reopen it for the same machine: it must resume where it _(NOT VERIFIED 2026-09-29: Remote repo picker needs a real remote machine)_
      was left, not at `/`.
- [ ] **[VISUAL]** The list scrolls inside the dialog without breaking its layout _(NOT VERIFIED 2026-09-29: partial — Remote picker at /usr/lib (17 folders) and home (82): dialog top 193 bottom 707 in 900px viewport; entries list is a scroll container (client 318 / scroll 413, overflow auto) so the dialog does not grow; footer buttons stay. No screenshot (times out), judged by geometry.)_
      on a directory with many entries (`/usr/lib` is a good one).

## Orchestrator RESULT wake with background work (#797-8549) — needs a `make dev` restart

- [x] After restarting `make dev`, leave an orchestrator at its confirmed-ready,
      empty composer while one background descendant is still running, then have
      a child send `RESULT`. The send must report
      `delivery_path=wake_notification_and_inbox`; the parent must receive only
      the generic `agent action=inbox` notice, and the inbox must contain one
      untouched RESULT.
      _(verified 2026-09-21: a delayed `tuic agent send` ran as the live Codex
      process's background descendant. After this turn yielded, TUIC injected only
      `[TUIC] message available`, the sender received
      `wake_notification_and_inbox`, and `agent action=inbox` returned exactly one
      untouched `RESULT live-wake-797-8549`.)_
- [x] Repeat with text partially typed in the parent composer: the route must be _(verified 2026-09-30: Fake claude orchestrator with a real background descendant (sleep 240 started by a turn >60s after start; status background_work=true). Draft 'partialdraft' typed + RESULT send -> inbox_only, draft still on screen, nothing submitted. Control with empty composer -> wake_notification_and_inbox, one wa)_
      `inbox_only` and the draft must remain unchanged. The focused Rust regression
      covers this mechanically; this item retains the live composer check.

## `tuic agent send` accepts current delivery reports (#800-18c6) — needs a sidecar rebuild

- [x] After the next `make dev` or sidecar rebuild, run the installed _(verified 2026-09-29: /usr/local/bin/tuic agent send <peer-uuid> msg (TUIC_SOCKET to validate instance, TUIC_SESSION set) against a registered PTY peer running 'sleep 60': exit 0, 'Buffered for <id> (inbox_only) — unread until the recipient polls its inbox'; message present in peer inbox. Caveat: peer is a shell (shell_state reported idle), not a real busy agent.)_
      `/usr/local/bin/tuic agent send` against a busy registered peer. It must exit
      0 and print `Buffered … (inbox_only)`, not `Registry did not accept the
      message`. The freshly built `target/debug/tuic` already passed this exact
      live check; this item verifies that the installed sidecar has caught up.

## SQLite Viewer plugin host primitives — needs a `make dev` restart (#797-a4bd)

- [x] Open a `.db` fixture from the File Browser and confirm the themed object
      list, row page, index inspector, and query console render without a stale
      dirty badge. _(verified in an isolated `sqlite-viewer-920` instance;
      screenshot: `.screenshots/sqlite-viewer-plugin.png`)_
- [ ] Exercise the per-column filter and visual query-plan buttons through the UI, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: SQLite Viewer registry plugin UI (per-column filter, visual query plan): needs frontend and plugin install.)_
      then enable editing on a primary-key table, change a scalar cell, save,
      reopen the file, and confirm the persisted value. _(The exact SQL filter,
      explain, edit, export, and reopen path is runtime-tested; UI automation was
      stopped after MacControl switched same-name windows/coordinate spaces.)_
- [x] Close and reopen the SQLite tab and confirm a fresh iframe/database viewer
      is created. _(verified in the isolated instance; pending-load cleanup is
      also covered by `main.test.js`.)_

## Remote machine reachability is visible outside Settings

Boss added a repo from mac-mint, the daemon went unreachable, and the only place
that said so was Settings -> Remote Machines. Frontend-only, so Vite HMR already
has it: no `make dev` restart needed.

- [ ] With a remote machine in `error`/`disconnected` and at least one repo _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      registered on it, the status bar shows `Offline: <machine>` in red, and the
      tooltip points at Settings -> Remote Machines.
- [x] The sidebar badge on that repo reads `offline` in red instead of `remote`, _(verified 2026-09-29: Web UI: remote repo 'RR' badge 'remote' (grey, tooltip 'On agb2wrong.'); after killing the local tuic-remote daemon: badge 'offline', color rgb(241,76,76) red, tooltip 'agb2wrong is not answering. Reconnect in Settings → Remote Machines.')_
      and its tooltip names the machine and its state.
- [ ] Reconnect the machine: the status-bar pill disappears and the badge goes _(NOT VERIFIED 2026-09-29: Needs a remote machine to disconnect/reconnect)_
      back to a muted `remote` without a reload.
- [ ] A remote machine with NO registered repo must NOT appear in the status bar _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      while disconnected — that is its normal resting state.

## Adding a remote repo must open ONE tab, not two (`usePty` pre-registration)

Observed: adding `/home/stefano/omi-local-stack` from mac-mint opened `shell 1`
plus a phantom `PTY: Session 17`. Both were the same remote PTY — the desktop
create is routed over HTTP to the daemon, whose `session-created` echo was not
deduped because the guard keyed on `isTauri()` instead of "is this call routed
to a remote connection". Covered by two new tests in `usePty.test.ts`.

- [ ] Add a repo from a connected remote machine. Exactly one shell tab appears. _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
- [ ] Adding a LOCAL repo still opens one tab and the backend still mints the id. _(NOT VERIFIED 2026-09-29: partial — Web UI: Add Repository > Local Repository > path 'fx/agb2/lr' > Add: LR appears in sidebar, tab bar shows exactly one tab 'main 1' (no duplicate), repositories.json entry for lr has no connectionId (rr has). Backend-minted id not inspected.)_

### Root cause found while testing the above

`remoteConnectionsStore.hydrate()` was called from exactly one place —
`RemoteMachinesPanel.tsx`. Until the user opened Settings -> Remote Machines the
store held nothing, so the status bar, the sidebar badge and `Sidebar.tsx`'s own
`getConnections()` read an empty map and could not tell a live machine from a
dead one. Now hydrated once at startup in `useAppInit`; `hydrate()` is
idempotent, so the panel still calls it.

- [ ] Start the app WITHOUT opening Settings. A down remote machine holding a _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      repo must already show `Offline: <name>` in the status bar.

## Deleting a remote machine must take its connection with it (#803-f875)

Rust change — needs a `make dev` restart (or `make build`) to load; the running
session will NOT have it.

Deleting a connected machine used to leave the SSH tunnel, the status poll and
the mirror task running against a machine that no longer existed in the config.
There is now one teardown path (`remote_runtime::teardown`) and both the IPC and
the HTTP delete route call it.

- [ ] Connect an SSH-transport machine, confirm `ssh` is running _(NOT VERIFIED 2026-09-29: Needs SSH-transport remote machine (second machine mac-mint) and ssh process)_
      (`pgrep -fl ssh`), then delete the machine from Settings -> Remote
      Machines. The `ssh` process must be gone within a second, and the app must
      not show a status push for the deleted id afterwards.
- [x] Do the same over HTTP against the dev instance: _(verified 2026-09-29: Connected conn via API, then DELETE /config/remote-connections/{id} -> {ok}; status [] at once, log 'Disconnected'. Killed+restarted daemon and waited 20s: no further Connecting/Connected log, status stays []. Supervisor stopped.)_
      `curl -X DELETE http://127.0.0.1:9877/config/remote-connections/<id>` —
      same result. Before this change the HTTP route stopped nothing.
- [ ] Any sessions that machine had mirrored disappear from the session list on _(NOT VERIFIED 2026-09-29: Needs a real connected remote machine with mirrored sessions.)_
      delete, and no `session-state-changed` for them arrives after it.
- [x] Delete a machine that was never connected: no error, nothing logged as a _(verified 2026-09-29: Validate instance: PUT /config/remote-connections (enabled:false, never connected) then DELETE /config/remote-connections/{id} -> {ok:true} 200; list no longer has it; /logs since the call has no warn/error and no remote entries.)_
      failure.

## An errored remote machine recovers on its own (#803-f875)

Same restart caveat — Rust.

- [ ] Connect a machine, then stop `tuic-remote` on it. The badge goes to _(NOT VERIFIED 2026-09-29: Needs a real tuic-remote daemon to stop and start)_
      `error` and its mirrored sessions retire from the list.
- [ ] Start the daemon again and WAIT — do not press Connect. Within one poll _(NOT VERIFIED 2026-09-29: partial — Daemon killed (badge 'offline', status error Unreachable), restarted daemon with no Connect press: status 'connected' within 8s (polled every 8s), sidebar badge back to grey 'remote'. Reappearing sessions not checked (daemon had none).)_
      interval the badge must return to `connected` by itself and the machine's
      sessions must reappear.
- [ ] Wrong password: the badge reads `unauthenticated`, and for an SSH-transport _(NOT VERIFIED 2026-09-29: Needs SSH-transport remote machine with wrong password (second machine))_
      machine no `ssh` process is left behind (`pgrep -fl ssh`).

## A dropped request must not leak an ego process (#804-2ec8)

Rust change — needs a `make dev` restart (or `make build`) to load.

`/acp/one-shot` was awaited inline under the router's 301s timeout, which drops
the handler future. Everything after the drop was skipped, including the
`disconnect` that stops ego, and nothing else ever would: the supervisor is an
independent task and only settled connections are pruned. The turn now runs on
its own task, the launch has its own 60s budget and the turn 240s, so the whole
call fits inside the router's bound.

- [ ] Run a Smart Prompt in `api` mode, then close the tab / kill the request _(NOT VERIFIED 2026-09-29: Needs a real ego process running a live turn (POST /acp/one-shot, kill mid-turn, pgrep ego); no ego/provider in headless run.)_
      mid-turn (`curl ... & sleep 2; kill %1` against
      `POST http://127.0.0.1:9877/acp/one-shot`). Within a few seconds
      `pgrep -fl ego` must show no leftover process.
- [ ] `GET /acp/connections` must not list a connection for the abandoned turn. _(NOT VERIFIED 2026-09-29: Needs ego ACP turn abandoned by dropped request.)_
- [ ] A normal Smart Prompt still answers, and a long one that runs out of time _(NOT VERIFIED 2026-09-29: Needs real ego and a 240 s timeout turn (oneshot.rs:333).)_
      reports "ego did not finish the turn within 240s" rather than a bare 408.
- [x] Point `ego_executable` at something that starts and never speaks (e.g. a _(verified 2026-09-30: browser mode on isolated instance, mute `sleep 3600` script: no AbortError at +30 s; UI shows the 502 "the agent did not answer initialize within 60s" between +47 s and +83 s, Retry button; no leftover child)_
      `sleep 600` wrapper) and press Connect in AI Chat: it must fail within a
      minute with "the agent did not answer initialize within 60s" instead of
      spinning forever, and leave no child behind.
- [ ] Press Connect on a remote machine and navigate away immediately. The _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      machine must still reach `connected` (or `error`) — never stay stuck on
      `connecting`, which used to make every later Connect a silent no-op.

## Terminal stream compression is acknowledged, not assumed (#805-f52e)

Rust and frontend — the Rust half needs a `make dev` restart (or `make build`).

The browser used to decide every frame was tagged from its own request alone. A
daemon that predates `?compress=deflate` ignores it and sends untagged frames,
and the client then read the first byte of a grid row as a tag. The server now
selects the `tuic.deflate` subprotocol when it is going to tag, and the client
reads `ws.protocol` in `onopen` before the first frame.

- [ ] Open a terminal on a remote machine over a **direct** (non-tunnel) link. _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      It renders normally, and DevTools shows the stream socket with
      `Sec-WebSocket-Protocol: tuic.deflate` on the 101.
- [ ] Open a terminal on the same machine (local session). The socket asks for _(NOT VERIFIED 2026-09-29: blocked — Browser WebSocket URL/subprotocol not observable: no resource-timing entry for WS and no fresh PTY tab can be created to patch WebSocket before connect (agent-browser eval only, page reload drops patch). Code: canvasTerminalTransport.ts:221 (per agentb0 1048).)_
      nothing: no `compress=deflate` in the URL and no subprotocol on the 101.
- [ ] Point a current build at an **older** `tuic-remote` (one without this _(NOT VERIFIED 2026-09-29: Needs an older tuic-remote binary from an earlier commit)_
      commit). The terminal must render correctly — untagged framing — and the
      app log must carry "asked for compression and the server did not take it"
      rather than "could not decode a compressed frame" once per frame.
- [ ] Open a terminal through an **SSH tunnel**. The frames must be tagged but _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      never deflated (`ssh -C` already compressed the channel), which is the
      `::ffff:127.0.0.1` case the canonical-address fix covers. Check CPU on the
      daemon stays flat while an agent repaints.

## The AI Chat panel keeps its connections apart (#806-4335)

Frontend only — Vite HMR picks this up, no `make dev` restart needed.

- [ ] Open AI Chat on two different repo roots so two ego connections are live. _(NOT VERIFIED 2026-09-29: Needs two live ego connections on two repo roots.)_
      Stop ego on the first (or disconnect it). The second panel must keep
      streaming — no "Not receiving updates" banner on the root that did not end.
- [ ] Press Recover after a gap. The connection list must show ONE connection _(NOT VERIFIED 2026-09-29: Needs live AI Chat/ego connections and a gap recovery.)_
      afterwards, not the dead one plus the fresh one, and the fresh panel must
      keep receiving updates rather than freezing a second later.
- [ ] Send a prompt while ego is wedged or the session is not accepting prompts. _(NOT VERIFIED 2026-09-29: Needs a wedged/non-accepting real ego session in AI Chat.)_
      The message must disappear from the transcript rather than sitting there as
      a turn that was never received.
- [ ] Attach to a session id ego does not have. The transcript that was on screen _(NOT VERIFIED 2026-09-29: needs a real ego agent (ACP) session; not available headless)_
      must come back rather than being left blank under a live session.

## The headless build announces repo-op progress too (#808-84e1)

Rust — needs a `make dev` restart, and the interesting half needs the headless
binary: `cargo build --bin tuic-remote --no-default-features`.

**Which binary:** `run_headless` (the main binary's headless mode), NOT
`tuic-remote`. The three routes are in `build_router`; `build_remote_router`
does not carry them, so `tuic-remote` answers 404 (#810-4986).

- [ ] Start the headless mode of a `--no-default-features` build, open the web _(NOT VERIFIED 2026-09-29: Needs PR review (ego/GitHub account) on a --no-default-features headless build.)_
      UI against it, and start a PR review on a repo. The Review findings column
      must move from "running" to a result on its own. Before this commit it
      stayed on "running" forever, because the `review-progress` event was
      dropped on that build.
- [ ] Same daemon, run an improvement scan. The proposals must appear in the _(NOT VERIFIED 2026-09-29: Improvement scan runs on ego against a real daemon; needs ego)_
      panel when the scan finishes — the return value never populates it, only
      the `proposals-ready` event does.
- [ ] Same daemon, run conflict assist on a PR with conflicts. The status must _(NOT VERIFIED 2026-09-29: Needs a real GitHub PR with conflicts, conflict assist (ego) on a headless daemon.)_
      reach the panel rather than leaving it idle.
- [ ] `curl -N http://127.0.0.1:<port>/events` against that daemon while each of _(NOT VERIFIED 2026-09-29: blocked — review-progress/proposals-ready/conflict-assist-status can only be produced by ego (ACP; POST /repo/improvement-scan -> 'no ego executable is configured') or a real GitHub PR (pr-review/conflict-assist -> 'No GitHub remote URL found'). Not triggerable in isolated daemon.)_
      the three runs. The `review-progress`, `proposals-ready` and
      `conflict-assist-status` frames must appear on the stream.
- [ ] Desktop build, same three operations: unchanged. The window emit still _(NOT VERIFIED 2026-09-29: blocked — Needs PR review, improvement scan and conflict assist (ego + real GitHub PR with conflicts) on a headless daemon plus desktop panels; ego/GitHub PR not available headless.)_
      fires, so nothing about the desktop panels may look different.

## A registered remote machine comes up by itself and stays up

Rust — needs a `make dev` restart.

One task per connection now owns its whole lifecycle: bring it up, keep it up,
retry while it is down. It replaces the heartbeat that was spawned only from the
SUCCESS branch of a connect, which is why a machine whose first attempt failed
sat in `error` until somebody pressed Connect — measured on mac-mint, answering
200 throughout while the app showed it unreachable for forty minutes.

- [ ] Start the app with a remote machine registered and REACHABLE, without _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      touching Settings. It must reach `connected` on its own, and the sidebar
      badge must read `remote` rather than `offline`.
- [ ] Start the app with the remote machine OFF. It must show `offline`, and the _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      backend must keep retrying — the wait doubles from 2s to a 60s ceiling.
      `GET http://localhost:9876/logs?source=remote` shows one `Connecting` line
      per attempt, spaced by a growing gap.
- [ ] With the app running and the machine offline, turn the machine ON. It must _(NOT VERIFIED 2026-09-29: Needs a remote machine to switch on/off)_
      go `connected` by itself within one backoff window. No click.
- [x] Press Connect on a machine that is off. The button must report the failure _(verified 2026-09-29: Daemon stopped, machine 'Error'. Pressed Connect in Settings>Remote Machines: row shows 'Error / Unreachable: error sending request for url (http://127.0.0.1:9892/health)'; /logs show repeating 'Connecting' / 'Remote connection failed' afterwards, i.e. retry continues. (Cannot tell attempt-own vs shared error text.))_
      (that attempt's own error), AND the retry must continue afterwards.
- [x] Press Disconnect on a connected machine. It must stay disconnected — _(verified 2026-09-29: Direct conn to local tuic-remote (:9892) connected; pressed Disconnect in Settings>Remote Machines; watched 95s (daemon reachable throughout): status [] via GET /config/remote-connections/status, no new 'Connecting'/'Connected' remote log lines, UI shows no Connected.)_
      watch it for longer than 60s. A retry that resurrects it is the bug the
      generation counter exists to stop.
- [x] Disconnect, then Connect again immediately. The machine must come up, and _(verified 2026-09-29: DELETE /connect then POST /connect immediately: status connected (same token). Then killed daemon: status error Unreachable, restarted daemon: back to connected in 3s with new token, so the new supervisor still retries after the retired one was taken down.)_
      it must still retry if it later drops — the new supervisor must not have
      been taken down with the retired one.
- [x] Give a machine the WRONG password and connect. It must land in _(verified 2026-09-29: Own tuic-remote w/ password (--set-password) on :9894; connection with WRONGPW + POST /connect -> 502 'Authentication rejected', status unauthenticated; over 60s /logs shows one 'Connecting'+one 'Remote connection failed' for it, no retries (other conn kept retrying). Correct PW + POST connect -> status connected.)_
      `unauthenticated` and STOP: no repeated attempts in the logs. Fix the
      password, press Connect, and it must start again.
- [ ] Delete a remote machine while it is retrying. Nothing may keep probing it, _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      and no entry for it may remain in the status bar.

## The website names warm copy-on-write worktrees

- [ ] `website/index.html`, "Git worktrees, fully managed": the second bullet _(NOT VERIFIED 2026-09-29: partial — Rendered website/index.html in a same-origin srcdoc iframe (no screenshot). 1200px: 'Warm worktrees' bullet names copy-on-write, li 544x69, no overflow. 390px: bullet wraps (92px tall, code chips intact) but its column right edge is 402 > 390 viewport and document scrollWidth is 508 (several .feature-content blocks are 361-484px wide, all sections )_
      now names the copy-on-write warming that `docs/user-guide/worktrees.md`
      documents. Checked at 1200px; check it on a phone width too.

## A missing bridge says so (#809-724c)

Rust — needs a `make dev` restart.

- [x] Move or rename `tuic-bridge` so it is neither beside the executable nor on _(verified 2026-09-29: tuic-remote copied to dir without tuic-bridge (owner env, sandbox HOME): /logs?level=warn holds 'Skipping agent MCP config updates: no bridge beside this executable' with searched_paths [<exedir>/tuic-bridge, 'tuic-bridge']. (Plus an extra warn 'temporary or mounted app' since binary sat under TMPDIR.) Control with bridge beside logged 'Ensuring br)_
      the resolved path, then start the app. `curl 'http://localhost:9876/logs?level=warn'`
      must carry one line naming both checked paths and the symptom. Before this
      commit there was nothing in the log at all.
- [ ] Put it back and restart. That warning must NOT appear, and AI Chat must be _(NOT VERIFIED 2026-09-29: partial — Headless tuic-remote copy (TUIC_MCP_CONFIG_OWNER=1, disposable HOME): without tuic-bridge beside it, /logs warn = 'Skipping agent MCP config updates: no bridge beside this executable' with searched_paths [<dir>/tuic-bridge, tuic-bridge]; with bridge beside it: 0 warns, 'Ensuring bridge configs'. Desktop app restart and AI Chat list-terminals (ego) )_
      able to list terminals again.

## Hands-free voice entries in the Compose queue (#814-6d13)

_(NOTE 2026-09-23: superseded — hands-free turns and notices are now typed straight into the terminal, busy or not, and never enter the Compose queue; there is no `voice_command` kind, no `queuedIds`, no `cancelled`/`alreadyDelivered`. Read "reaches the Compose queue" as "is typed into the terminal"; a dialog or draft holds the turn in the hands-free panel. See "Hands-free turns reach a busy agent at once" at the top.)_

Rust — needs a `make dev` restart. The hands-free mode has no UI control yet
(Step 8), so this checks the queue half through the existing HTTP surface.

- [x] With an agent tab busy, `POST /sessions/{id}/queue` a command, then check _(verified 2026-09-29: Fake agent child (agent_type claude, idle-not-ready) : POST /sessions/{id}/queue twice -> queued 1,2; GET queue -> every entry has kind=user_command. No voice_command kind exists (state.rs kind() only notice/initial_prompt/user_command; kind removed in 560709f9e).)_
      `GET /sessions/{id}/queue`: every entry still lists a `kind`, and an
      ordinary Compose command still reads `user_command`. The new
      `voice_command` kind must not appear for anything typed by hand.
- [x] Enqueue two commands on a busy agent and let them drain on the next idle _(verified 2026-09-29: Fake amp-type agent held busy 9s; POST /queue ALPHA then BRAVO (queue list ids 6,7 in order); after idle, agent output: ALPHA, GOT: ALPHA, BRAVO, GOT: BRAVO; queue empty. Order preserved.)_
      window. They must still arrive in order — `enqueue_user_command` now
      appends through a shared helper, and a reordering would show up here.

## Pocket TTS speech synthesis (#823-c260)

Rust — needs a `make dev` restart. Nothing calls the port yet (playback is
story 816), so the only way to reach it today is the bundle-backed tests:

```
TUIC_POCKET_BUNDLE_DIR=<bundle> [TUIC_POCKET_VOICE=<voice>] \
  cargo nextest run --lib --run-ignored ignored-only -E 'test(/dictation::speech::pocket/)'
```

- [HUMAN] Listen to `.tmp/kokoro-eval/ONNX/frase{1,2,3}-rust-int8.wav`, rendered
      by this adapter, against the `-torch-fp32`, `-onnx-fp32` and `-onnx-int8`
      sets from the evaluation. Two questions, one listening pass: does the Rust
      port sound like the Italian that was approved, and is int8 (125 MB per
      language) good enough against fp32 (400 MB)? The second answer decides what
      the downloader in story 813 offers.
- [ ] Windows and Linux: the adapter loads `onnxruntime.dll` / `libonnxruntime.so` _(NOT VERIFIED 2026-09-29: Requires Windows and Linux hosts for onnxruntime library loading.)_
      from beside the models by an explicit path. Only macOS/arm64 has been run,
      and a missing library must still report `ModelUnavailable` rather than
      taking the process down inside `ort`.

## Hands-free arm and disarm (#814-6d13)

_(NOTE 2026-09-23: superseded — hands-free turns and notices are now typed straight into the terminal, busy or not, and never enter the Compose queue; there is no `voice_command` kind, no `queuedIds`, no `cancelled`/`alreadyDelivered`. Read "reaches the Compose queue" as "is typed into the terminal"; a dialog or draft holds the turn in the hands-free panel. See "Hands-free turns reach a busy agent at once" at the top.)_

Rust — needs a `make dev` restart. There is still no UI control, so the HTTP
surface is the only way to reach it.

**Arming opens the microphone and starts capturing.** `open_endpoint` runs
before the bind (`dictation/commands.rs:959`) and a successful arm spawns a
driver thread that polls every 50 ms, transcribes each closed utterance through
Whisper, and injects the text into the bound session's Compose queue. Speak near
the machine while armed and the words reach the agent. Use a throwaway session,
and disarm before walking away.

- [ ] Hands-free arm/disarm over HTTP, against a throwaway agent session: _(NOT VERIFIED 2026-09-29: partial — Shell session arm -> {error:'Session cannot accept hands-free input'} verified; agent-typed session (spawned fake agent_type=claude) passes that gate and fails 'Model not downloaded'. armed:true/sessionId echo not reached (needs whisper model+mic; not arming to avoid mic prompt). disarm returned wasArmed:false (armed state never entered); GET consi)_
      `curl -X POST localhost:9877/dictation/hands-free/arm -H 'content-type: application/json' -d '{"sessionId":"<id>","owner":"desktop"}'`
      must return `armed: true` with `sessionId` echoed back. Arming against a
      shell (non-agent) session must return `Session cannot accept hands-free
      input`. `GET /dictation/hands-free` must agree with what arm returned, and
      `POST /dictation/hands-free/disarm` must report `wasArmed: true` once and
      `wasArmed: false` on a second call.
- [ ] Arm, then speak one short Italian sentence and stop. Within about a second _(NOT VERIFIED 2026-09-29: Needs real microphone speech input (Italian sentence))_
      of the pause the text must appear as a `voice_command` entry in
      `GET /sessions/{id}/queue`, and reach the agent on its next idle window.
      `GET /dictation/hands-free` must walk `waiting` → `capturing` →
      `transcribing` → `holding_back` → `delivered` across the turn; a phase that
      never leaves `capturing` means end-of-speech was not detected.
- [ ] Arm, then close the bound session from the UI. The mode must disarm itself _(NOT VERIFIED 2026-09-29: Needs microphone hands-free arm and real bound session.)_
      with `TargetClosed` and release the microphone without a disarm call —
      check `GET /dictation/hands-free` reads `armed: false` and that the app log
      carries `Hands-free disarmed: TargetClosed`. This is the path that keeps a
      dead tab from holding the device open.
- [ ] Arm, then unplug or switch away the input device. After the silence _(NOT VERIFIED 2026-09-29: Needs physically unplugging/switching real input device)_
      timeout the mode must disarm with `DeviceFailed` and name the device in the
      message, rather than sitting armed and deaf.
- [ ] `owner` is now checked against the one adapter that exists. Any value other _(NOTE 2026-09-29: description stale — a browser audio owner now exists (continuous.rs:321); the refusal text was not found by rg 'Audio endpoint' in Rust)_
      than `desktop` must be refused with `Audio endpoint '<owner>' is not
      available on this build` and must leave the mode unarmed — the browser
      endpoint is story 818. This is a behaviour change: arming from a remote
      client used to bind and now fails at the endpoint.
- [ ] Push-to-talk must be unaffected. With hands-free armed, run a normal _(NOT VERIFIED 2026-09-29: Needs real audio capture for push-to-talk plus hands-free.)_
      push-to-talk recording: it must capture and transcribe as usual, and must
      neither disarm hands-free nor be disarmed by it. Then disarm hands-free and
      confirm push-to-talk still works. The two modes hold separate captures.
- [HUMAN] Confirm the microphone indicator (menu bar / camera-mic dot) turns on
      at arm and off at disarm, for every disarm path above. Nothing in the test
      suite can see whether the OS actually released the device, and an armed
      mode that leaks the microphone after disarm is the failure that matters
      most here.

## Hands-free activation phrase (#815-7c76)

_(NOTE 2026-09-23: superseded — hands-free turns and notices are now typed straight into the terminal, busy or not, and never enter the Compose queue; there is no `voice_command` kind, no `queuedIds`, no `cancelled`/`alreadyDelivered`. Read "reaches the Compose queue" as "is typed into the terminal"; a dialog or draft holds the turn in the hands-free panel. See "Hands-free turns reach a busy agent at once" at the top.)_

Rust — needs a `make dev` restart. **There is no UI control for the phrase**;
`DictationSettings.tsx` has no field for it, so the only way to set one today is
the config surface. A Dictation control belongs to story 818.

- [ ] With `hands_free_activation_phrase` empty, arm and speak: every recognised _(NOT VERIFIED 2026-09-29: Needs real spoken utterances through recognizer into Compose queue.)_
      utterance must reach the Compose queue exactly as it did before this
      story. An empty phrase must change nothing.
- [x] Set the phrase to `attività tuic`, then save something unrelated from the _(verified 2026-09-29: Web UI Settings>Voice: set Activation phrase 'attività tuic' and Hold-back 2250ms (Expert), then changed Language auto->Italian from the UI; GET /dictation/config after each: phrase 'attività tuic', hands_free_hold_back_ms 2250 and language 'it' all persisted (no reset to defaults). Hotkey/device saves not separately exercised.)_
      Dictation settings UI — a hotkey, the language, the device. Re-read
      `GET /dictation/config`: the phrase and `hands_free_hold_back_ms` must
      **still be there**. This is the defect the store fix closes; before it,
      every save from the UI silently reset both fields to their defaults.
- [ ] Armed with that phrase, speak "che ore sono" alone: nothing must reach the _(NOT VERIFIED 2026-09-29: Needs spoken audio input.)_
      queue. Then "attività tuic che ore sono": the queue must receive
      `che ore sono` with the phrase stripped. Then, within 15 seconds, speak a
      bare follow-up: it must go through without the phrase. Wait past 15 seconds
      and the phrase must be required again.
- [ ] Say "tuicommander che ore sono" with the phrase set to `tuic`: it must NOT _(NOT VERIFIED 2026-09-29: Needs real spoken audio input with activation phrase)_
      activate. A longer word that merely starts with the phrase is a different
      word, not a prefix match.
- [ ] Disarm and re-arm while a window is open: the first utterance after the _(NOT VERIFIED 2026-09-29: Needs real speech through a microphone to test the activation phrase window.)_
      fresh arm must need the phrase again. Every disarm closes the window.

## Resume banner names the work

- [ ] Leave an agent tab running with a declared `intent:` (or a typed prompt), _(NOT VERIFIED 2026-09-29: Needs real agent tab with declared intent and app quit/reopen resume banner.)_
      quit the app, reopen it and select that branch. The
      "Agent session was active — click to resume" banner must now carry
      `Intent: <...>` (or `Prompt: <...>` when no intent was declared), truncated
      with an ellipsis and with the full text in the tooltip. A tab that never
      had either must show the banner exactly as before.
- [ ] The Context bar (`Show last prompt` setting) on the restored tab must show _(NOT VERIFIED 2026-09-29: Needs a real resumed agent tab to declare intent and restore context bar values.)_
      the same restored values, and must be replaced by the live ones as soon as
      the resumed agent declares a new intent or the user sends a prompt.

## Echo canceller starts with the app

Needs a `make dev` restart — the Rust backend does not hot-reload.

- [x] After the restart, `GET http://localhost:9876/logs?source=dictation` must _(verified 2026-09-29: Validate instance (this build, started fresh; dictation state installs echo canceller at startup mod.rs:170): GET /logs?source=dictation has 0 'no echo cancellation'; full startup log desktop.log and instance tuic.log also have none. Caveat: the warn in echo.rs:305 has no source=dictation field, so that filter would not match it anyway; grep the wh)_
      NOT contain `no echo cancellation`. That line means `WebRtc::new()` failed
      and hands-free fell back to `PassThrough`, which cannot hear the user over
      the speaker. It is logged at warn level on purpose; a quiet fallback would
      look exactly like a working canceller until someone tried to interrupt.
- [ ] Startup must not be visibly slower. `DictationState::new()` now builds one _(NOT VERIFIED 2026-09-29: partial — desktop.log: first log 22:39:23.595, HTTP TCP+unix listening 22:39:24.617 (~1.0s incl. Tailscale detection 0.5s), key monitors 22:39:25.7. No AEC3-less baseline build to compare, so 'not slower' is not proven; nothing looks slow.)_
      AEC3 instance for the life of the app, before any dictation is used.

## Speech assets download and install (#813-e84b)

Needs a `make dev` restart — the Rust backend does not hot-reload. There is no
Dictation UI for these yet (#818-2a29), so drive them over HTTP.

**The voices are published.** `speech-voices-v1` was cut on 2026-09-22 and serves
`italian-giovanni.safetensors` at the sha256 pinned in `assets.rs`, verified by
downloading it back from the public URL. The Italian download is therefore
expected to complete rather than 404 on its last file.

- [ ] `curl localhost:9877/dictation/speech/assets` lists two assets, _(NOT VERIFIED 2026-09-29: partial — GET /dictation/speech/assets on clean instance: onnxruntime state=absent, italian state=absent, but list has 157 entries (onnxruntime, 6 languages, 150 voices), not two. Item text stale.)_
      `onnxruntime` and `italian`, both `"state": "absent"` on a clean machine.
- [x] `curl -X POST localhost:9877/dictation/speech/assets/download -H _(verified 2026-09-29: POST speech/assets/download {asset:onnxruntime} -> 'Installed to .../models/speech/onnxruntime' (5.9s, download_bytes 42631433); only libonnxruntime.dylib present, 74612032 bytes; asset list state=ready.)_
      'content-type: application/json' -d '{"asset":"onnxruntime"}'` downloads
      42 MB, extracts one library, and answers `Installed to <path>`. The file
      at `<config>/models/speech/onnxruntime/libonnxruntime.dylib` must be
      about 74 MB — that is the library, not the 330-byte pkgconfig file beside
      it in the archive. Re-query the list: `"state": "ready"`.
- [x] While that download runs, the same list must report _(verified 2026-09-29: During POST /dictation/speech/assets/download {asset:german}: GET /dictation/speech/assets showed german state=downloading (3 polls, italian ready); second identical POST refused: 'could not write the download: German is already downloading'. (First download later failed on a HuggingFace 'error decoding response body' network error, state absent.))_
      `"state": "downloading"` for it, and a second download of the same asset
      must be refused rather than started.
- [x] `POST /dictation/speech/assets/cancel {"asset":"onnxruntime"}` mid-download _(verified 2026-09-29: Deleted onnxruntime, started POST assets/download, POST assets/cancel at 0.05/0.15/0.4s: download returned {error:'download cancelled'}, GET assets state=absent (not incomplete), models/speech/.staging empty each time. 3 runs; asset removed afterwards.)_
      must stop it, and the list must go back to `absent` — not `incomplete`.
      Nothing may be left under `<config>/models/speech/.staging/`.
- [x] `POST /dictation/speech/assets/delete {"asset":"onnxruntime"}` removes the _(verified 2026-09-29: Downloaded onnxruntime (state ready, libonnxruntime.dylib), POST assets/delete {asset:onnxruntime} -> 'Deleted ONNX Runtime', dir gone; second delete -> same success string. Also on already-absent asset.)_
      directory, and a second delete answers success rather than an error.
- [x] `{"asset":"italian"}` downloads about 125 MB into _(verified 2026-09-29: POST {asset:italian} -> 'Installed to .../speech/italian' (~133MB on disk, 130082025 B download); contains voices/giovanni.safetensors; list shows italian state=ready voices [giovanni].)_
      `<config>/models/speech/italian/` and the list then reports `ready`,
      including `voices/giovanni.safetensors`.
- [ ] The failure path still needs checking, and no longer happens by itself: _(NOT VERIFIED 2026-09-29: Needs network interruption or bad URL fault injection during a real download)_
      interrupt the network mid-download (or point one `Fetch` at a bad URL) and
      confirm the failure leaves `state` at `absent` with no `.staging`
      directory behind. That behaviour was previously proven for free by the
      missing voice, so it is now unobserved rather than known-good.
- [x] Corrupting one installed file afterwards (`truncate -s 100 _(verified 2026-09-29: Downloaded French via API (ready), then truncate -s 100 <models/speech/french/bundle.json> (used French, not shared Italian): asset list shows french state=incomplete missing=['bundle.json'] (was ready, []). Deleted french afterwards.)_
      <config>/models/speech/italian/bundle.json`) must move the asset to
      `"state": "incomplete"` with that file named in `missing` — never `ready`.

## Spoken replies and the `voice` MCP tool (story `817-f67c`, 2026-09-22) — **Rust, needs a `make dev` restart**

Everything below needs an installed Italian bundle, because arming without one
opens the conversation **without** a voice. That is no longer a blocker: the
`speech-voices-v1` release was cut on 2026-09-22, so install the bundle through
the download items above first, then work through these.

- [ ] Arm hands-free against a throwaway session on the restarted build, then _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/status need desktop build; WS audio client can arm without a mic (Italian bundle must be installed).)_
      `curl 'localhost:9877/dictation/speech/status'`. `available` must be
      `true`, `sessionId` must be that session, and `voice` must name the
      installed Italian voice.
- [ ] `curl -X POST localhost:9877/dictation/speech/speak -H 'content-type: _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. POST speak -> state queued checkable there; 'audible in Italian' is speaker-hardware (blocked).)_
      application/json' -d '{"text":"Ciao, sto parlando."}'` must answer
      `state: "queued"` with an `utteranceId` — never `"finished"`. Listen: the
      reply comes out of the speaker in Italian.
- [ ] Poll `GET /dictation/speech/status?utterance=<id>` while it plays. It must _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Status state walk checkable there; 'finished only after last word audible' needs ears.)_
      walk `queued` → `rendering` → `speaking` → `finished`, and only reach
      `finished` **after** the last word is audible.
- [ ] **Barge-in.** Queue a long reply, then start talking over it. The speaker _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Barge-in could be simulated by feeding speech audio on the WS; audible stop needs ears.)_
      must stop within a beat, the utterance must report `interrupted` rather
      than `finished`, and `turn` must have advanced. What you said must arrive
      in the terminal as a new turn — not appended behind the reply.
- [ ] Re-send the same reply quoting the **old** `turn`. It must be refused with _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Stale-turn refusal is pure API once armed via WS audio client.)_
      a message naming both turns, not spoken.
- [ ] `POST /dictation/speech/stop` while a reply plays: silence immediately, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. POST /dictation/speech/stop turn/queue checks are API-only once armed; audible silence needs ears.)_
      the queue empties, and the returned `turn` is higher than before.
- [ ] Disarm while a reply is playing. The audio must stop, and a `speak` after _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Disarm then speak refusal is API-only once armed via WS; audio stop needs ears.)_
      that must be refused with "Hands-free is not armed".
- [ ] From a **second** terminal's Claude Code, `voice action=status`: it must _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. voice tool via real claude reachable (see 3934); binding refusal needs an armed conversation on the desktop build.)_
      report the binding refusal, not the first conversation's queue. From the
      armed terminal's own Claude Code, `voice action=speak` must be heard.
- [x] With nothing armed, `GET /dictation/speech/status` must answer _(verified 2026-09-29: GET /dictation/speech/status with nothing armed -> {available:false,unavailableReason:"Hands-free is not armed",...} HTTP 200 JSON, no error)_
      `available: false`, `unavailableReason: "Hands-free is not armed"` — never
      an error.
- [x] In Claude Code connected to this build, `voice` must appear in the tool _(verified 2026-09-30: Real claude -p (tuic-bridge stdio, TUIC_SOCKET+TUIC_SESSION=shell PTY id) saw mcp__tuic__voice on a fresh connection; status answered {available:false,unavailable_reason:'This TUICommander build has no audio support'}, no error. raw tools/list on fresh MCP session lists voice.)_
      list on a fresh connection without any list-change notification, and
      `action=status` must answer rather than erroring.

## One language, end to end (story `822-7d7a`, 2026-09-22) — **Rust, needs a `make dev` restart**

The first four items need an installed Italian bundle, for the same reason as
the block above: without one there is no voice to listen to. The release that
serves it exists as of 2026-09-22, so install it first. The point of every one
of these items is the same — the
model must never answer in a language the user is not speaking.

- [ ] Set Dictation language to **Italian**, arm hands-free, and say something in _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs Italian language, armed conversation and real agent reply; drive via WS audio client on desktop build.)_
      Italian. The terminal entry must read `<what you said> (reply in Italian)`,
      on one line, and the agent must answer **in Italian**. Instruction
      delivery is not the proof: read the agent's reply.
- [ ] With the same setup, listen to the spoken reply. It must be the Italian _(NOT VERIFIED 2026-09-30: blocked — audio hardware: listening to the Italian TTS voice is speaker/ears; also headless build has no audio.)_
      voice reading Italian — not Italian text read by another language's voice,
      and not an English sentence.
- [ ] Set the language to **Auto** and disarm/re-arm. Before you say anything, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Auto-language status (available:false, language:'') is an HTTP check after re-arm.)_
      `curl 'localhost:9877/dictation/speech/status'` must answer
      `available: false`, `language: ""` and a reason naming Auto. Nothing may be
      spoken in this state.
- [ ] Still on Auto, say something in Italian. `status` must then report _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs spoken Italian/English input (WS audio) and real agent reply.)_
      `language: "it"`, and the entry must carry `(reply in Italian)`. Say the
      next turn in **English**: the entry must carry `(reply in English)` and the
      agent must switch with it.
- [ ] Set the language to **Korean** (transcribed, no voice bundle) and arm. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Korean status refusal is an HTTP check after arm.)_
      `status` must answer `available: false` with
      `No speech bundle ships for language "ko"`. It must **not** fall back to
      the Italian voice.
- [ ] While a reply is being spoken, change the Dictation language in Settings. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs Settings UI language/RMS change during playback; audible cut needs ears.)_
      The speaker must stop mid-sentence and `status` must report the new
      language. Then change only the **RMS threshold** while another reply
      plays: that one must keep playing to the end.
- [ ] Turn the hands-free entry/exit hints **off** (story 821's setting, when it _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs hints-off setting in UI and armed entry.)_
      lands) and repeat the first item. The `(reply in …)` requirement must still
      be in the entry — it is not a hint.
- [ ] In the armed terminal's Claude Code, `voice action=status` must report _(NOT VERIFIED 2026-09-30: partial — Verified: voice tool inputSchema has only action,text,turn,utterance_id (no language/voice param). Not verified: status.language from an armed terminal (no dictation/arm in headless build).)_
      `language`, and the tool schema must offer no way to pass a language or a
      voice.

## The model is told when hands-free starts and stops (story `821-842a`, 2026-09-22) — **Rust, needs a `make dev` restart**

_(NOTE 2026-09-23: superseded — hands-free turns and notices are now typed straight into the terminal, busy or not, and never enter the Compose queue; there is no `voice_command` kind, no `queuedIds`, no `cancelled`/`alreadyDelivered`. Read "reaches the Compose queue" as "is typed into the terminal"; a dialog or draft holds the turn in the hands-free panel. See "Hands-free turns reach a busy agent at once" at the top.)_

Automated coverage is in place for the mechanism: the notice reaches the Compose
FIFO of the bound session, a notice the agent never read is withdrawn instead of
contradicted, both disarm paths send the end notice, the setting turns both off,
and push-to-talk sends neither. What no test here can check is whether a real
model **acts** on them — that is the whole point of the feature, and it needs a
live agent and Boss's judgement.

Run every item against a throwaway tab in the worktree build, never Boss's live
sessions. Settings > Dictation now carries **Notify model when hands-free
changes** (on by default).

- [ ] Arm hands-free on a Claude tab. The tab must receive a line saying voice _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_
      is on for this terminal, submitted as its own turn. Then say something
      ordinary and read the reply: the agent should either call the voice tool
      or explain why it cannot — **not** ignore the notice.
- [ ] Disarm. The tab must receive the "voice is off, reply as text" line, and _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_
      the next thing you type must be answered in text with no voice attempt.
- [ ] **Rapid arm/disarm while the agent is busy.** Arm and disarm again within _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_
      a second or two while the agent is mid-task. The start notice must
      disappear from the Compose queue and **no** stop notice may appear — the
      agent must end up with neither line, not with a lone "voice is off".
- [ ] Arm, wait for the agent to read the start notice, then close the bound _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_
      tab. The runtime disarms itself; confirm the log shows the end notice was
      attempted and reports honestly that the target was gone.
- [ ] Turn the setting off, arm and disarm. Neither line may appear anywhere. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_
      Then, still with it off, arm and check that speech itself still works
      (`voice action=status` must report `available: true` once a language is
      known) — the setting must silence the notices and nothing else.
- [ ] Arm with the setting **on**, let the agent read the start notice, then _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_
      turn the setting off in Settings while still armed, then disarm. The stop
      notice must still be sent: the agent was already told voice was on.
- [ ] Hold the push-to-talk hotkey and dictate a sentence. No notice of either _(NOT VERIFIED 2026-09-30: blocked — audio hardware: push-to-talk hotkey hold + dictated sentence (Fn/hotkey and mic); no hotkey/mic path on headless build.)_
      kind may appear, the hands-free badge must stay off, and `voice
      action=status` must still report `available: false`.
- [ ] **[VISUAL]** Settings > Dictation: the new toggle must sit with the other _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
      dictation toggles and its hint must read clearly at the panel's width.

## Voice conversation controls in the Dictation panel (story `818-2a29`, 2026-09-22) — **Rust, needs a `make dev` restart**

Settings > Dictation now carries two new sections below Voice tuning: **Spoken
replies** (the speech assets, the language replies are spoken in, and the voice)
and **Hands-free conversation** (terminal picker, Start/Stop, live phase,
activation phrase, hold-back). The dictation hotkey now also stops a running
conversation.

Everything mechanical is covered by tests; these items need real audio, a real
download, or Boss's eye. Run them against the worktree build, never Boss's live
sessions.

- [ ] Download **ONNX Runtime** and **Italian** from Spoken replies. The percent _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Asset download routes /dictation/speech/assets* are desktop-only; needs Settings rows to check percent per row.)_
      must climb on each row independently — starting both at once must not show
      one row the other's progress — and each row must end at Downloaded.
- [ ] Cancel a download halfway. The row must go back to Not Downloaded with no _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Cancel route and .staging cleanup need the desktop build (prior 29/09 run saw cancel not clearing 'downloading').)_
      progress bar left behind, and no partially installed files may remain.
- [ ] With Italian ready, the **Voice** control must appear and list `giovanni`. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Voice control list (giovanni) is UI; hearing the voice is speaker hardware.)_
      Pick it, then arm a conversation and hear a reply in that voice.
- [ ] Change the voice while a reply is being spoken. The reply must stop _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Voice change during reply needs UI; audible mid-sentence cut needs ears.)_
      mid-sentence rather than finish in the other voice.
- [ ] **Opening Settings > Dictation must not light the microphone indicator.** _(NOT VERIFIED 2026-09-30: blocked — audio hardware: observing the macOS microphone indicator (human eyes on menu bar) when opening Settings > Dictation.)_
      Neither must starting the app. Nothing arms by itself.
- [ ] Start a conversation from the panel, then press the dictation hotkey. The _(NOT VERIFIED 2026-09-30: blocked — audio hardware: dictation global hotkey plus real capture/voice queue on the desktop app.)_
      conversation must stop — capture, queue and voice — and the status line
      must say how many spoken entries had already been typed.
- [ ] Press the hotkey with nothing armed. It must record as usual, not report a _(NOT VERIFIED 2026-09-30: blocked — audio hardware: global dictation hotkey recording with microphone.)_
      stopped conversation.
- [ ] Let a conversation end by itself (close the bound terminal), then press the _(NOT VERIFIED 2026-09-30: blocked — audio hardware: global dictation hotkey recording with microphone after conversation ends.)_
      hotkey. It must record — a stale armed flag must not eat the keypress.
- [ ] Set an activation phrase, then speak a sentence without it: nothing may be _(NOT VERIFIED 2026-09-30: blocked — audio hardware: real spoken sentences (mic) against the activation phrase; no synthetic STT path in headless build.)_
      sent. Speak one with it: the phrase itself must not reach the terminal.
- [ ] **[VISUAL]** Both new sections at the panel's width: the asset rows must _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
      line up with the Whisper model rows above them, and the phase line must
      stay readable while it changes.
- [ ] Open the app in a browser tab (`http://localhost:9877/`) and open _(NOTE 2026-09-29: stale — browser mode now has a Voice tab (renamed Dictation) with the full Dictation/Hands-free content)_ _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Instance has no frontend (headless); item is a browser-tab Settings check (Dictation tab absent? previous note says browser mode now shows Voice tab, item text stale).)_
      Settings. The **Dictation** tab must be absent entirely, and searching
      settings for "Hands-free" must report no match rather than opening an
      empty panel. The browser microphone and speaker are story `832-e730`.

## Barge-in over a real speaker (story `816-cbbf`, 2026-09-22) — **[HUMAN]**

Criterion 3 of the story asks for a real audio probe, and this is the half of it
no test can reach. Everything that can be measured offline already is —
`talking_over_the_reply_stops_it_without_losing_the_first_words` in
`continuous.rs` reports 50 ms stop latency and 0 false triggers against the real
AEC3 canceller, over a **modelled** room (40 ms delay, 0.35 gain, no
reverberation, no noise floor, no speaker distortion). See
`docs/backend/dictation.md` → "What barge-in measures".

Needs a real microphone and a real speaker, in a room, with no headphones.

- [ ] **[HUMAN]** Arm hands-free, ask something with a long answer, and let the _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
      reply play **out of the laptop speaker**. Say nothing for the whole reply.
      The reply must finish. A reply that cuts itself off is the echo path
      failing on real reverberation, which the modelled room cannot produce.
- [ ] **[HUMAN]** Same again, and talk over it after a couple of seconds. The _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
      reply must stop within about a quarter of a second, and the transcript
      that reaches the terminal must contain your **first** word — that is the
      pre-roll doing its job. A transcript that starts mid-sentence is the
      failure to report.
- [ ] **[HUMAN]** Repeat both at a high speaker volume, close to the _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
      microphone. This is the case the linear room model is least like: a
      driven speaker clips, and AEC3 cannot subtract what the amplifier added.
      Report whether false interruptions appear and at roughly what volume.

## A whole voice conversation, on real hardware (story `820-21a5`, 2026-09-22) — **[HUMAN]**

The eight states a conversation has to survive are held by tests and indexed in
`docs/backend/dictation.md` → "The eight states the conversation has to
survive". What is left here is what no test can reach: real Whisper and real
Kokoro inference, a real microphone and speaker, and the browser endpoint.

**One of these is still blocked.** The Italian voice is no longer: the
`speech-voices-v1` release on `sstraus/tuicommander` was cut on 2026-09-22 and
serves `italian-giovanni.safetensors` at the pinned sha256, so a first run can
download a voice. Browser capture/playback is story `832-e730`, which is not
built. Do not mark that item from a mock — record it as blocked.

- [ ] **[HUMAN]** In an isolated instance (`TUIC_APP_INSTANCE=voice-check`) and _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
      against a throwaway terminal: download the speech assets, arm hands-free,
      speak a question, and let it run to the end — automatic end of turn, the
      agent answering out loud, and talking over the answer to interrupt it.
      Everything with the real assets, not the test doubles.
      _Unblocked 2026-09-22: the voice is published. Real Pocket TTS synthesis
      is already proven against a local bundle — `cargo nextest run --lib
      --run-ignored ignored-only -E 'test(/dictation::speech::pocket/)'` with
      `TUIC_POCKET_BUNDLE_DIR` and `ORT_DYLIB_PATH` set, 3/3 green. What is left
      here is the microphone, the speaker and the interruption._
- [ ] **[HUMAN]** Ask the **model** to speak through the `voice` MCP tool while _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
      that same conversation is armed, and confirm it reaches the same speaker
      and the same queue as a reply the desktop asked for.
- [ ] **[HUMAN]** The same conversation from a browser tab against the same _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
      instance: the microphone and the speaker must be the **browser's**, not
      the desktop's, and the desktop must behave identically.
      _Blocked: browser audio transport is story `832-e730`._
- [ ] **[HUMAN]** Record, with numbers: time from the end of speech to the _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
      first audible word of the reply, and the process footprint before arming,
      while speaking and after disarming (`GET /diagnostics/memory`). A
      conversation that leaks per turn is the failure to look for.
- [ ] **[HUMAN]** Repeat the first item on Windows and on Linux from a release _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
      build. Cross-platform evidence cannot come from this Mac.

## Speech and download progress on `/events` (story `833-6fd4`, 2026-09-22) — **Rust, needs a `make dev` restart**

The three pushes are dual-emitted: the desktop window gets an `emit`, the SSE
stream gets the identical body. Only a restart loads them.

- [x] Open the web UI (`http://localhost:9877/`, browser mode) and start a _(verified 2026-09-29: Web UI :9880 browser mode, Voice>Spoken replies: clicked Download on English; row went 0% -> 12% -> 25% -> 81% in the browser tab (polled DOM every 3s), then Downloaded. (Also found: whisper-model Download in browser sends {} -> 422 'missing field model': transport.ts:136 reads args.model_name but store passes modelName.))_
      speech-asset download from the Dictation panel. The progress bar must move
      in the **browser** tab, not only on the desktop — before 833 a browser
      client saw the download start and finish with nothing in between.
- [x] `curl -N http://localhost:9877/events` while that download runs: frames _(verified 2026-09-29: SSE /events (unix socket) during italian speech download: 'event: speech-download-progress' data {downloaded,total,percent,asset:'italian'} + final {asset,done:true}. Concurrent Whisper small download (POST /dictation/models/download {model:small}): 'event: dictation-download-progress' data {downloaded,total,percent} with no asset key (29961 frames)_
      named `speech-download-progress` carrying `asset`, `downloaded`, `total`
      and `percent`. A Whisper-model download on the same stream must be named
      `dictation-download-progress` and must **not** carry `asset`.
- [ ] Arm hands-free, let a reply play, and watch the same stream: one _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs /events SSE watch on desktop build while a reply plays (speech-utterance frames).)_
      `speech-utterance` frame per transition, in the order
      `queued → rendering → speaking → finished`, with no polling of
      `GET /dictation/speech/status`.
- [ ] Talk over a reply and confirm the last frame for that utterance is _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Talk-over needs armed conversation; audio can come via WS client, hearing cannot.)_
      `interrupted` rather than `finished`, and that it arrives — the transition
      happens on the render thread after `speak` has long returned, which is the
      case a polling client used to miss entirely.

## Agent toast repository action (story `835-314c`, 2026-09-22) — **frontend visual**

The rendered DOM was exercised with the real Vite modules and contained two
toasts at once: the different-repository toast had `Go to repo`; the current-
repository toast did not. `agent-browser` navigation and DOM inspection worked,
but `Page.captureScreenshot` timed out repeatedly, and CUA could not bind the
headed test browser window.

- [ ] **[VISUAL]** Capture the two toast states together after the screenshot _(NOT VERIFIED 2026-09-29: partial — Toast VISUAL: screenshots time out in this browser so no contrast/wrapping judgement possible; toast trigger states not reproduced in web UI. Not verified.)_
      backend is available. Confirm the secondary button spacing, contrast and
      wrapping at the normal window width and at a narrow width.

## SSH-managed `tuic-remote` deploy and install (stories `836-c262`–`847-ad3d`, 2026-09-22) — **Rust, needs a `make dev` restart**

The Rust backend does not hot-reload. Restart the test instance before these
checks; use `TUIC_APP_INSTANCE=remote-deploy-check` so no production connection
or credential is touched.

- [x] Against a throwaway Linux or Apple Silicon macOS SSH host with no daemon,
      save **Deploy on connect** and click Connect. It must progress through
      `Deploying: <step>` to Connected, bind only `127.0.0.1`, and leave the
      host's agent configuration files unchanged.
      _(verified 2026-09-22 through the HTTP parity surface against an isolated
      Ubuntu systemd container on mac-mint: Connected, loopback listener only,
      pairing token absent from argv, and no `~/.claude.json` created)_
- [x] Disconnect, wait less than the configured survive time, and reconnect.
      Existing remote sessions must still be present. After disconnecting for
      longer than the survive time, the daemon and its pid file must disappear.
      _(verified 2026-09-22: a three-second client between lifetime polls reset
      the deadline; after the new idle window both process and pid file vanished)_
- [x] Connect again with the same desktop version. The remote binary hash must
      match, no second SCP should occur, and the vault pairing token must still
      work after restarting the desktop test instance.
      _(verified 2026-09-22: inode/mtime stayed unchanged and the isolated
      credential-file digest survived a full `make dev` restart)_
- [x] Click Install. On Linux verify the systemd user unit and mode-0600 env
      file; on macOS verify the mode-0600 launchd plist. Reboot or log out/in and
      confirm Connect no longer deploys. Then click Uninstall and confirm the
      service files and ephemeral pid are gone.
      _(verified 2026-09-22 on isolated Ubuntu/systemd via HTTP parity: unit and
      protected env installed, linger enabled, service and loopback listener
      returned after a container reboot before any SSH login, then Uninstall
      removed the unit, env, pid and listener. launchd rendering/mode/lifecycle
      are covered by the targeted Rust service tests.)_
- [x] The Remote Machines form, deployment picker, survive-minutes field, SSH
      host picker and monochrome icons were rendered in an isolated browser on
      port 9877. _(verified 2026-09-22 from the worktree build; proof in
      `.tmp/visual-proof/remote-machines-fields.png`)_

## SSH local-forward readiness (story `1159-4e28`, 2026-09-28) — **Rust, needs a `make dev` restart**

- [ ] After restarting an isolated test instance, connect the Installed-service _(NOT VERIFIED 2026-09-30: blocked — second physical machine: aws-graviton (56481148) SSH remote; AWS box is DOWN per coordinator brief, and instance config must not be edited by hand.)_
      `aws-graviton` remote (56481148) over SSH. It should progress from
      Connecting to Connected once the local forward listens and `/health`
      answers, without an intermediate "installed daemon not answering" error.
      Record the elapsed time and the tunnel status transitions. Use only the
      configured host and credentials; the targeted Rust tests cover delayed
      local ports and delayed health independently.

## Config defaults and expert-mode UI pref (story `863-03c1`, 2026-09-24) — **Rust, needs a `make dev` restart**

- [x] After restarting the desktop dev build, `GET http://127.0.0.1:9876/config/defaults` _(verified 2026-09-29: GET /config/defaults on validate desktop instance: keys app, notifications, agent_settings, repo_defaults, agents, github_accounts, dictation (superset of item's 4; matches docs table). app keys == GET /config keys; live notifications and /dictation/config equal defaults; app differs only in instance services (port 9880, auth, vapid). dictation pre)_
      (or `:9877` for a worktree build) returns `{ app, notifications,
      agent_settings, dictation }` — each nested object matching the shape of
      its own `load_config`/`load_notification_config`/`load_agents_config`/
      `get_dictation_config` response, and every field holding that domain's
      documented default value (see `docs/backend/config.md` → "Config
      Defaults"). Confirm `dictation` is present on the desktop build.
- [ ] Toggle `settingsExpertMode` (via `ui.ts`'s `setSettingsExpertMode`, once a _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: settings_expert_mode is set by frontend ui.ts (writes ui-prefs.json per 29/09 note, item path config.json is wrong); headless instance has no frontend; full restart not allowed.)_
      caller wires it in — this story only persists the pref, no UI control
      yet) and confirm `settings_expert_mode` round-trips through
      `~/Library/Application Support/tuicommander/config.json` (or platform
      equivalent) and survives a full `make dev` restart.
- [ ] After the restart, `GET /config/defaults` also returns `repo_defaults` _(NOT VERIFIED 2026-09-30: partial — Verified: GET /config/defaults returns repo_defaults (16 keys) and agents ({agents:{}}); remote.log has 0 'unknown configKey' lines. Not verified (needs UI): Expert-off hides 8 Git&GitHub rows and Smart Prompts 'Headless Agent'.)_
      and `agents` (story `864-a5c9`). In Settings with Expert off, Git &
      GitHub hides the eight repository-default rows while they hold their
      defaults, and Smart Prompts hides "Headless Agent" while it is not
      configured. The app log shows no `settingsExpert` "unknown configKey"
      warning for `repo_defaults.*` or `agents.*`.

## Error messages name the reorganized Settings pages (story `861-977b`, 2026-09-24) — **Rust, needs a `make dev` restart**

- [x] With a configured microphone unplugged, starting dictation reports _(verified 2026-09-29: by code/test inspection, tests not executed here: Message exists at src-tauri/src/dictation/commands.rs:1513 'check Settings > Voice > Input device'; old label gone)_
      "check Settings > Voice > Input device" (was "Settings > Dictation >
      Microphone"; neither the page nor the label exists any more).
- [x] With progress collection off for an agent, the `progress` MCP tool _(verified 2026-09-29: by code/test inspection, tests not executed here: progress/service.rs:38 'progress_tracking_disabled: ... (Settings → Agents)'; mcp_transport.rs:1523 comment. Message names Agents page.)_
      answers `progress_tracking_disabled … (Settings → Agents)`. The toggle
      lives on the Agents page; "Settings → Progress" never existed.
- [ ] [VISUAL] Settings → General with Experimental Features **off**: an **ego** _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
      section sits directly after Code Intelligence, with a `?` tooltip, the
      "Configured at …" / "Not configured" line, **Select…** and **Clear**.
      Settings → AI Chat (Experimental Features on) shows only Default Model
      and Providers. With the ego path empty, AI Chat says to name the binary
      in Settings → General.
## Mobile Basic Auth recovery (2026-09-25) — **Rust, needs a `make dev` restart**

- [ ] **[HUMAN]** On a phone PWA with remote access enabled and a stale cached Basic _(NOT VERIFIED 2026-09-30: blocked — real phone: PWA with stale cached Basic credential and the browser's native Basic Auth challenge.)_
      credential, navigate until the browser shows its Basic Auth challenge. Enter the
      current password without reloading. The app must reconnect and resume the session.

## CircleCI failure logs — **Rust, needs a `make dev` restart**

- [ ] After restarting the worktree build, open a failed CircleCI check on a remote-only PR, including a PR with a failed GitHub Actions job. Its Log button shows only that CircleCI check's log and the end of a long failed step, with a truncation marker when the beginning was dropped; a stale or mismatched CircleCI build reports a revision mismatch. The running app cannot load this Rust change until restart. _(NOT VERIFIED 2026-09-30: blocked — not a hardware class, but needs a CircleCI API token and a real remote-only PR with a failed CircleCI check; GET /circleci/token -> configured:false. Reading the CircleCI code path for a fake server was denied by the sandbox classifier (credential), so not attempted. Coordinator decision.)_

## Safe linked-worktree removal — **Rust, needs a `make dev` restart**

- [x] After restarting an isolated worktree build, remove a clean linked worktree with a populated submodule. It succeeds without a dirty-file confirmation. A submodule with a local commit stays intact on a non-force request. A forced removal of a branch with unmerged commits keeps the branch and reports why. The running app cannot load this Rust change until restart. _(verified 2026-09-29: Own repo+submodule: clean wt3 removed via worktree_remove, no confirmation. wt4 (submodule local commit): non-force refused 'uncommitted changes', dir intact; forced w/ fingerprint: commit 4ad6c724 still in main libsub. wt5 unmerged: non-force refused; force ok + warning 'unmerged commits', branch wt5 kept.)_
- [x] After the same restart, confirm a forced removal with a dirty submodule, then change its HEAD before the request completes. Removal must stop with a changed-state message; retry after a fresh review. A clean merged submodule commit must remain accessible from the main checkout after removal. _(verified 2026-09-29: Disposable repo w/ submodule via MCP repo tool: dirty submodule (requires_force), fingerprint from worktree_lifecycle, then committed in submodule (HEAD change) -> worktree_remove force with old fp: 'Worktree state changed since confirmation; review it before removal', worktree kept. Fresh lifecycle fp -> ok. Clean merged sub commit 7cf89d3b (branc)_

## Vite watcher scope and native reload attribution — needs a `make dev` restart

- [ ] Restart `make dev` when live PTY sessions can be interrupted. In an isolated dev instance, create and delete a checkout with HTML files under repository `.tmp/`; verify document age continues increasing and no full reload occurs. Call `POST /debug/reload_webview` and verify the native log records caller address, trigger, action, and target URL while frontend startup records navigation type and document start. The Vite watch config and Rust backend require a restart to take effect. _(NOT VERIFIED 2026-09-30: partial — Verified: vite dev on :5199 with repo vite.config.ts: creating/deleting HTML under repo .tmp/ (nested too) logged no 'page reload'; controls public/*.html and root *.html logged 'page reload'. POST /debug/reload_webview on headless -> 'webview recovery requires the desktop feature'; native log lines)_
## Desktop Progress entry (2026-09-26)

- [x] With zero unread Progress updates, open the toolbar bell and select Terminal Progress for the active repository. The bell badge remains absent; a new update restores the count. The command palette and `Cmd/Ctrl+Shift+P` open the same dialog. _(verified: targeted Toolbar, keyboard shortcut, and action registry tests; rendered bell screenshot at `~/Gits/.tmp/tuic-progress-entry/progress-bell.png`.)_

## Ask Boss from the mobile PWA (2026-09-26) — Rust, needs a `make dev` restart

- [ ] **[HUMAN]** After this Rust parser change is landed and Boss restarts `make dev` when current PTYs can be interrupted, open the HTTPS mobile PWA on a real phone at 360×800. In a disposable Codex session, trigger `request_user_input` with two choices and `Other`. Confirm its waiting badge, tap the question control in the existing session header, read the title and all options, select an option once, and verify Codex receives exactly one answer and the overlay clears. Repeat with `Other` and type a note; verify it reaches the question rather than the main composer. Check the terminal has lost zero rows. The running backend cannot load the Rust parser change before restart. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_

- [ ] **[HUMAN]** After restarting the desktop app when its current PTY sessions can be interrupted, enable Remote Access and Tailscale HTTPS, then open the shown HTTPS `/mobile` URL on the phone. On iPhone, launch the installed Home Screen PWA. In mobile Settings, turn Push notifications off and on to replace the old subscription, grant permission, and confirm a test push appears on the phone. Do not change Tailscale/network configuration as part of this check. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_
- [ ] **[HUMAN]** With the desktop window left focused but no Mac HID input for two minutes, have a managed agent report `progress type=blocked` with an identifiable question. Confirm one phone notification contains the question, opens that exact session, and one typed reply reaches it once. During a confident free-text question, leave an automated peer message queued: the phone answer must reach the question first and the peer message must remain parked until the question clears. Repeat with the desktop actively used: no duplicate push. The running app cannot load these Rust changes until restart. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_
- [ ] **[HUMAN]** After the separate question-state change is integrated, trigger a real Claude AskUserQuestion with a visible title. Confirm the phone push contains that title rather than the hook's empty awaiting signal or an Ink footer, then answer it from the opened session. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_

## PTY build environment — Rust, needs a `make dev` restart

- [x] After restarting an isolated TUIC build, open a shell PTY in a different Rust repository and check that `CARGO_TARGET_DIR`, `CARGO_MANIFEST_DIR`, and `OUT_DIR` are unset while `CARGO_HOME` and an ordinary user environment variable remain available. Spawn a managed agent in the same repository and confirm the same. The running TUIC backend cannot load this Rust change until restart. _(verified 2026-09-30: Isolated tuic-remote (--instance r4iso, own TMPDIR) started with CARGO_TARGET_DIR/CARGO_MANIFEST_DIR/OUT_DIR=/poison/* and MYUSERVAR=hello. Shell PTY env: only CARGO_HOME, MYUSERVAR, USER; poison vars absent. MCP-spawned managed agent (fake binary): same. Fixture cwd was not a Rust repo.)_
- [x] After restarting an isolated `make dev` build, open a new terminal and run `env | grep -E 'CARGO_INCREMENTAL|RUSTC_WRAPPER|^MBX_'`; expect no matches. In a managed agent PTY, check that `HOST_CC` and `HOST_CXX` are also unset. Confirm a configured per-agent `CARGO_INCREMENTAL=1` still reaches its PTY. The current Rust backend requires a restart before this can be checked. _(verified 2026-09-29: Own tuic-remote started with CARGO_INCREMENTAL=0 RUSTC_WRAPPER MBX_* HOST_CC HOST_CXX in its env (confirmed via ps): new shell PTY env|grep -> no matches; spawned agent PTY (env in spawn) -> HOST_CC/CXX unset, none of the vars; spawn env CARGO_INCREMENTAL=1 -> reaches agent PTY.)_

## Peer mail wake after Rust restart

- [x] After restarting an isolated `make dev` build, send a 10 KiB message to a disposable external Claude client subscribed to MCP SSE. Confirm the channel shows the sender UUID, message ID, size, and first-line preview without the body; `agent action=inbox` returns the complete message once. The running Rust backend cannot load this change until restart. _(verified 2026-09-29: Fake external client (initialize clientInfo claude-code, GET /mcp SSE) got notifications/claude/channel: '[TUIC] message available...\nfrom <sender uuid> id <msgid> 10240 bytes: Big report title', meta from_tuic_session+message_id, no body 'zzz'. agent inbox returned 1 message with 10240-char content; second inbox call 0. Via validate instance sock)_
- [x] After restarting `make dev`, spawn one disposable agent through MCP _(verified 2026-09-29: MCP agent action=spawn and POST /sessions/agent (stub /bin/sh -c 'sleep 30', no MCP call by the child): GET /sessions rows show tuic_session == session_id for both (96caece0.., fe777b72..). Both deleted.)_
      `agent action=spawn` and one through `POST /sessions/agent`. Confirm each
      `GET /sessions` row reports `tuic_session` equal to its `session_id` before
      the agent calls MCP, then close both sessions. The current backend cannot
      load this Rust binding change until restart.
- [x] Restart the isolated `make dev` test instance and run _(verified 2026-09-30: Instance has no hook config (--no-agent-configs), so spawn's --settings file is missing and claude died. Ran a copy of canary-peer-mail-wake.py with binary_path wrapper (strips --settings; cwd=worktree; TCP relay to the UDS): 'PASS claude: peer mail wake appeared in PTY within 20 s'. Deviations note)_
      `TUIC_CANARY_URL=http://127.0.0.1:9877 python3 scripts/canary-peer-mail-wake.py claude`.
      Confirm the disposable Claude PTY shows `PEER_MAIL_WAKE` within 20 seconds.
      See `docs/guides/development-setup.md` for the instance setup; the Rust
      change does not hot reload into the current process.
- [x] On that rebuilt isolated instance, run _(verified 2026-09-30: Same modified canary with --capacity: 'PASS: inbox accepted and returned mail after 100 read messages'.)_
      `TUIC_CANARY_URL=http://127.0.0.1:9877 python3 scripts/canary-peer-mail-wake.py claude --capacity`.
      Confirm mail 101 is accepted and returned after the first 100 were read.

## Activity Dashboard window after Rust restart

- [ ] After restarting an isolated `make dev` build, restore Activity Dashboard with saved geometry larger than 550×650. The detached OS window opens at 550×650 while retaining its saved position; a smaller saved size remains unchanged. The running Rust backend cannot load this fix until restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs desktop build: detached Activity Dashboard OS window size after restore with saved geometry >550x650; not observable headless.)_

## Rust dead-code warning cleanup after restart

- [x] On the next `make dev` Rust rebuild, confirm no dead-code warning names Design Mode `status`, `to_prompt`, or `on_script_parsed`, Progress `mark_viewed`, AppState `resolve_session_ref` or `resolve_peer_ref`, or StoryStore `transition`. The targeted test build has already compiled without these warnings; the current backend cannot hot reload the source change. _(verified 2026-09-29: build-desktop.log (tuicommander lib compiled from this worktree): 'generated 5 warnings'; grep finds none naming Design Mode status/to_prompt/on_script_parsed, Progress mark_viewed, resolve_session_ref/resolve_peer_ref or StoryStore transition (also 0 in headless build.log). Remaining warnings are unrelated symbols.)_

## CLI build after Rust rebuild

- [ ] After rebuilding `tuic`, verify `tuic repo worktree-list`, `worktree-create`, and `worktree-remove` still accept their existing names and `tuic agent spawn` accepts its positional prompt and launcher flags. The current binary does not hot reload the Rust CLI change. _(NOTE 2026-09-29: partial evidence only — tuic-cli main.rs:321 defines name 'worktree-list' and parse test at main.rs:1803; Spawn args at main.rs:192; verify create/remove/spawn args by inspection or clap tests.)_ _(NOT VERIFIED 2026-09-30: partial — my -p tuic-cli build output lacks session/repo/story/bg/mcp subcommands (help lists only open..resume) though source main.rs has them: needs investigation of the tuic-cli build; /usr/local/bin/tuic has them)_

## Claude transcript activity after restart

- [x] After restarting `make dev` with the story 1121 Rust build, put a throwaway Claude session in detailed transcript view after a hook idle, then deliver a peer mail wake. If Claude does not begin a turn, confirm the session returns to idle after the five-minute stale submission window and the app log contains both shell transitions. The current running backend cannot load this Rust change without a restart. _(verified 2026-09-30: Real claude via wrapper with my --settings hooks (Stop/UserPromptSubmit OSC 7770 same as hook_command). hook-idle 06:11:05, Ctrl+O sent (screen shows transcript-style output), mail wake -> busy 06:11:15, no turn began, idle via 'submission-stale' at 06:16:15 (+300s). Both transitions in log. Transcr)_

## WebView reload resource cleanup — Rust, needs a `make dev` restart

- [ ] After restarting an isolated `make dev` instance, open two desktop terminal panes and register a plugin output watcher. Reload the main WebView, then inspect `/diagnostics/memory`: the old document's grid channels, gates and output watcher clients must be gone before the panes remount. Leave one pane unmounted; it must produce no desktop `grid frame gate stuck` warnings. Two minutes later, a 30-second app-log window must contain no `Couldn't find callback id` or `Output watcher clients exceeded 8` warnings. The running backend cannot load this Rust change until restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs desktop main WebView with two panes, plugin output watcher, reload and /diagnostics/memory before/after; headless has no WebView.)_
- [ ] Detach a terminal into a floating window, reload the main WebView, and confirm the floating terminal keeps painting. Close the floating window and confirm its grid subscription disappears while terminals in the main window keep painting. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs desktop floating terminal window and WebView reload.)_

## Mobile ego chat (story 1077-0c08) — real phone after `make dev`

- [ ] [HUMAN] After restarting the test instance with `make dev`, open its HTTPS `/mobile` URL on a real phone. In Chat, choose a disposable repository and send a prompt; confirm the answer and collapsed tool activity remain readable above the keyboard. Disconnect and reconnect the phone, then confirm the answer has no duplicate or missing lines. Start a permission request, tap one option twice, and confirm the desktop conversation records one answer. Start a second conversation, then use the titled picker to return to the first and confirm its history loads. This check requires real touch and mobile keyboard behavior; targeted Vitest covers the module behavior. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Detached Markdown document window

- [ ] After restarting an isolated test build, open a Markdown file tab and a `tuic://open` Markdown tab. Detach each from its context menu, resize the document window, edit each file on disk, and confirm the detached content updates. Clicking either tab should focus its window; closing the window should restore the document in the tab. Check an inline comment and a relative Markdown link in the detached view. _(NOT VERIFIED 2026-09-29: blocked — Needs observing detached panel windows' content/size/focus; maccontrol capture_window lacks Screen Recording permission, invoke_js reaches only the main webview, and two identical 'tuicommander' windows make clicking ambiguous with Boss's app. Not attempted.)_

## ACP mobile interaction push (story 1078-05cf) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, subscribe a phone to push. With desktop unfocused, have ego ask permission or a form question and confirm one notification opens mobile Chat. Answer the next request on desktop before delivery and confirm no stale notification; ordinary activity must not notify. The running backend cannot load this Rust change until restart. _(NOT VERIFIED 2026-09-30: blocked — real phone: push subscription and mobile Chat notification with desktop unfocused.)_

## Push-to-talk Italian hallucination filter — Rust restart required

- [ ] After restarting `make dev`, use a disposable terminal to verify that a bare “Grazie a tutti.” recognition does not reach the composer, while a genuine instruction containing those words does. The current backend cannot load the Rust change until restart. The sustained-speech activity gate remains pending real quiet-speech recordings (story 1135-b600). _(NOT VERIFIED 2026-09-30: blocked — audio hardware: real speech recognition of 'Grazie a tutti.' (filter is unit-level HALLUCINATION_EXACT); dictation routes absent in headless build.)_

## MCP initialize storm — Rust restart required (#1148-c25f)

- [x] After restarting an isolated `make dev` instance, run short-lived MCP stdio bridge clients under one disposable `TUIC_SESSION` identity and inspect `/diagnostics/memory`: normal exits should release their protocol sessions immediately, while abrupt exits should be reaped after the next initialize once the six-second activity grace has elapsed. Stop the MCP endpoint briefly and verify one surviving bridge spaces its retries. The running backend and bridge binary cannot load these Rust changes until rebuilt. _(verified 2026-09-30: Isolated instance r4iso + mbx tuic-bridge, one TUIC_SESSION: mcp.sessions 0 -> 4 normal exits 0 -> 4 live 4 -> kill -9 x4 stays 4 -> >6s later one initialize 1 -> exit 0. Endpoint stopped: bridge retry connect times after 'connection lost' at t=5.5,7.5,11.6,19.6,27.6,35.6.. (spacing 2,4,8,8..) despi)_
- [ ] After that Rust restart, let an isolated disposable MCP protocol session pass the one-hour TTL (or invoke the maintenance sweep in a test build). Confirm its protocol session, route, reverse route, and broadcast sender all disappear while an addressable PTY peer and any live sibling remain usable. This checks the reaper cleanup added for story 1148; the source of the incident's 27.3 GB malloc growth is still unknown. _(NOT VERIFIED 2026-09-30: partial — Isolated instance, sweep after 1h idle: mcp.sessions 3->1, log 'MCP session reaped (idle >=1h)' x2 (sibling kept, still usable); PTY peer r4pty kept addressable and agent send to it after reap delivered:true; ext peers evicted. Route/reverse-route/broadcast-sender maps not exposed in diagnostics: in)_

## Worktree removal preview — Rust restart and visual review (#1138-ed2e)

- [ ] After restarting an isolated `make dev` build, open removal confirmation for a branch with no own commits and a live agent in its worktree. Confirm the dialog names the agent and uncommitted/untracked counts, then take a screenshot of both the removal and post-merge cleanup dialogs. The current backend cannot hot reload the Rust preview, and this branch has not been rendered in a worktree build. _(NOT VERIFIED 2026-09-30: partial — Verified backend preview (GET /worktrees/lifecycle?repoPath&workspaceId=feature-r4): live_sessions names r4wt-agent (fake managed agent, not real CLI), dirty_files 3, untracked_files 2, warning 'nothing of its own'. Dialog rendering and screenshots need desktop UI.)_
# Mobile session search (story 1200-4dd4)

- [ ] [HUMAN] After `make dev`, check the magnifier position at the top right of the session list on a phone. Tap it, enter a filter, and confirm the field and matching cards fit without clipping. The component test covers matching and clearing; phone layout remains to be checked. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] mobile session list layout.)_

# Mobile AI Chat image prompts (story 1219-e2a1) — Rust restart required

- [ ] [HUMAN] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, open the HTTPS mobile PWA on an iPhone and paste a 4 MiB photo from the camera roll into AI Chat. Confirm ego receives it; check an image over 10 MiB is refused in the composer with its size and limit. The HTTP route and composer have targeted automated tests; iPhone Photos behavior requires the device. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_

## Mobile attachments (story 1227-7838) — Rust restart and real phone

- [ ] [HUMAN] After restarting an isolated `make dev` instance, use an iPhone to pick a HEIC photo and a 4 MiB camera photo from the single paperclip picker. Check conversion/type handling, the displayed size limit, and that the selected file remains a draft until Send. The phone's Photos provider and touch layout require the device. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_
- [ ] [HUMAN] On an Android Chrome installed PWA, share a photo from another app into TUICommander. Confirm it appears as an AI Chat draft, then send it. The OS share sheet requires a real device. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_

## Mobile session output links (story 1202-dd5b)

- [ ] [HUMAN] On a phone, tap a Markdown path in a session's output, confirm Files renders it and Back preserves the session output and draft. Tap an HTTP(S) link and confirm it opens the system browser outside the PWA. A path outside registered repositories must show a toast naming that path. Targeted tests cover link detection, routing, and root refusal; the phone handoff and touch remain to be checked. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Mobile keybar and composer (story 1221-0380) — real phone

- [ ] **[HUMAN]** On a 360 px wide phone, open a disposable agent session and check that the terminal retains its previous visible row count, the keybar scrolls without a visible scrollbar, and `/`, Ctrl+C, input and Send are comfortable touch targets. Tap `/`: no character should reach the agent until a command is chosen. Close the menu and confirm an unsent draft is restored. End the disposable session and confirm the keybar and composer cannot send. Component tests cover PTY writes and disabled state; a 360×800 browser capture measured keybar 45 px, composer 53 px, and terminal 702 px, but cannot prove real touch behavior. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Mobile session list and new-session sheet (story 1223-f3bc) — real phone

- [ ] **[HUMAN]** On a 360 px phone, confirm a waiting session remains above idle agents and shells, exact mixed-case names display unchanged in the list, detail, and question banner, and the busy badge reads "Working". Tap the question counter and confirm it opens the first waiting session. Open `+`, check agent choice, repository search, and the close X, then create a disposable Codex session and confirm it opens. Component tests cover ordering, spawn payload, navigation callback, banner content, and counter click; real touch and visual layout remain to be checked. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Mobile session card actions (story 1217-4b0b) — real phone

- [ ] **[HUMAN]** On a 360 px phone, scroll the session list to its end. Confirm the `+` button never covers the last card's kill button, both `+` and kill are comfortable touch targets, tapping a card opens it, and tapping kill opens the confirmation without opening the session. Component tests cover independent click actions and accessible names; browser geometry at 360×800 measured 44×44 px kill, 52×52 px `+`, and an 87 px gap between the last kill and `+` after scrolling to the end. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Mobile five-tab navigation (story 1224-d21c) — real phone

- [ ] **[HUMAN]** Launch the mobile PWA on a phone and confirm Sessions opens first, Chat is the second bottom tab, and only five tabs remain. Tap the app-bar overflow, open Settings, then use a bottom tab to return. Check the overflow does not clip and the tap targets remain comfortable at 360 px. Component tests cover order, initial selection and Settings navigation; real touch and phone layout remain to be checked. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Mobile terminal hanging indent (story 1222-912d) — real phone

- [ ] [HUMAN] At 360 px, open a session with a long space-indented list item and a long tab-indented code line. Confirm every visual continuation starts under the first line's text, while an unindented line stays flush left and a box-drawing table still scrolls horizontally. Browser character-rectangle checks cover the same output shapes; this item checks real device rendering and touch scrolling. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Mobile session detail task text (story 1216-a482) — real phone

- [ ] On a 360 px phone, open idle, awaiting-input, and ended Claude sessions whose terminal status line has a decorative spinner verb. Confirm no task row repeats that verb and the terminal gains the freed row; a Codex session with a substantive task such as “Reading files” should still show it. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Claude AskUserQuestion options on mobile (story 1212-3093) — Rust restart and real phone

- [ ] **[HUMAN]** Restart an isolated `make dev` instance so its Rust parser loads, then open a disposable Claude AskUserQuestion on a phone. Confirm the title and every option remain visible, the choice overlay replaces generic Yes/No, and tapping option 2 selects Green exactly once. The captured PTY replay and component tests cover the payload and key sequence; this check covers real touch and phone rendering. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_

## Mobile Markdown images (1215-8e00)

- [ ] After restarting `make dev` to load the Rust HTTP route, open a nested Markdown file with a repository-relative image in the mobile Files tab and confirm the image loads. Check that a path escaping the repository does not load. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Mobile Progress, Activity, and Settings (1226-eb95)

- [ ] On a 360 px phone, confirm a long Progress message shows about four lines, More reveals it all, and Less collapses it again. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_
- [ ] Confirm Activity shows local 24-hour times, a completed 2343-second run as 39 min, and a single block as `1 block`. _(NOT VERIFIED 2026-09-29: partial — Code only: mobile/components/ActivityItem.tsx formatTime uses toLocaleTimeString hour12:false; readableSubtitle maps 'ran for Ns' >=60 to floor(N/60) min (2343s -> 39 min) and '1 blocks' -> '1 block'. /config/activity has 'ran for 329s' items (would show 5 min). Mobile /mobile in a 390px same-origin iframe loaded Sessions but Activity tab rendered )_
- [ ] After restarting `make dev` to load the Rust `mobile_theme` preference, choose Light in mobile Settings, reload the PWA, and confirm the theme stays light. Confirm the desktop theme remains unchanged and app/server versions are visible. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## AI Chat copy, links and parallel tabs (story 1142-f09c)

- [ ] [VISUAL] After `make dev`, in an AI Chat conversation select and copy a paragraph of transcript text and confirm it lands on the clipboard; click an http(s) link in a reply and confirm it opens in the external browser; click a local file path and confirm it opens through TUIC's file/editor opener. Open a second chat tab and confirm it runs an independent ACP session in parallel with the first. Component/DOM tests cover selection, copy, link routing and tab lifecycle; the live interaction remains to be checked. _(NOT VERIFIED 2026-09-29: Needs a real ego ACP conversation, external browser opener and system clipboard; two parallel live sessions.)_

## AI Chat session settings dialog (story 1145-3abd)

- [ ] [VISUAL] After `make dev`, open AI Chat and confirm the control bar shows a compact model/mode summary with one settings button. Open a new chat tab and confirm the summary is not blank. Open the settings dialog and confirm it lists one labeled row per ACP select config option (name, description, current value); change a value and confirm the summary updates from the agent's reply. Targeted Vitest covers the dialog and the late-options case; the live rendering remains to be checked. _(NOT VERIFIED 2026-09-29: Needs live ego ACP agent providing select config options.)_

## Codex questions on mobile PWA (story 1201-ae16)

- [ ] [VISUAL] On a real phone after `make dev`, open a Codex session that is waiting on a `request_user_input` question. Confirm the question text and its options are fully visible and each option can be tapped/selected without clipping at 360 px width. Confirm choosing an option submits exactly once and the question clears from both the session list badge and the session detail. Captured-fixture and 360 px DOM geometry tests already pass; the real-device check remains. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Git panel Log tab layout (story 1263-7d7d)

- [ ] [VISUAL] After `make dev`, open the Git panel Log tab on tuicommander main and compare with `~/Gits/.tmp/boss/log-tab/`: rows next to a narrow graph section have no wide empty gap before the subject; expanding a commit shows the full message without a hover tooltip, ref badges clip with an ellipsis instead of covering the subject, and rows below the expanded one move down at once with their graph dots. _(NOT VERIFIED 2026-09-29: partial — Web UI Git panel Log tab on fx/repo (not tuicommander main; no boss screenshots compared): subject starts 20px from row left (no wide gap); expanding 'ignore more' grows row 48->107px, rows below shift down 201->260 and 249->308 in one step, full message shown in commitBody, row has no title tooltip; refBadge computed text-overflow: ellipsis. Graph)_

## Branch integration proofs (story 1295-a2ce)

- [ ] After Boss restarts `make dev` or rebuilds the release, query `repo action=branch_integrations` and `repo action=branch_integration` on throwaway fixtures and confirm the new proof fields are available. Check that an integrated squash branch appears in the branch panel and sidebar, and a content-based proof requires an archive of the current tip before safe deletion. Automated real-Git lifecycle, MCP and deletion checks passed; the installed process has not been restarted to load this Rust change.
- [ ] After a manual `make dev` restart, call `repo action=progress_list` with no input on the live tuicommander journal (2400+ entries). The first page has 8 entries and is under 16384 B (was 17545 B with 10 entries, 2026-09-30). Story 1088-4783 GREEN criterion.

## Click on an underlined relative path (1336-7755)

- [ ] After a desktop rebuild, in a Claude session (and once in a plain shell with `echo docs/x.md`) click an underlined relative path, then read the app log for `link click:` lines (debug level). A click that opens the file logs `link click: opening`; a dead click logs which step ended it (`not opened` with claimed/detail/hasRange, `lookup went stale`, `nothing resolved`, or `does not cover`). Also click an OSC 8 link whose text is no path and hover it first: it opens (press claimed from the hover probe, #1336 fix). Browser mode could not reproduce the dead click; only the desktop can.

## Compose image paste and toolbar marker (1350-a1e6, 1351-69e0)

- [ ] After the next desktop rebuild, paste a screenshot (Cmd+Shift+Ctrl+4) and an image file copied from Finder into Compose: both attach an `[image: ...]` tag, in paste order. Paste plain text: it pastes as text. In auto density mode the toolbar button shows only the letter, no blue marker.

## Question reminder (1354-00ed)

- [ ] After the next desktop rebuild, leave an agent question unanswered for 2 minutes in a background tab: one sound and one OS notification. With the question tab active and the window focused: sound only. With notifications disabled: nothing.

## PR panel (1345-fb71 .. 1349-7546)

- [ ] After the next desktop rebuild, on a repo with open PRs: a PR transition (CI failed, ready, merged) gives one OS notification and its click opens the PR; Merge sends the head the panel showed; a PR with unresolved review threads shows the bot/human split and is not Ready; Update branch on a BEHIND PR, Close PR with confirmation, age marker and Copy reference work on their own row only. Then approve plan steps S1-S5.

## Edge TTS default engine (1357-7d37)

- [ ] After the next desktop rebuild, with network and a fresh config: Settings > Voice > Spoken replies shows Edge voices for the dictation language; Listen speaks the preview; the choice survives a restart. Run `cargo nextest run -p tuic-dictation --run-ignored only -E 'test(live_edge_service)'` once to confirm the live handshake (Sec-MS-GEC, Origin) still works. Start hands-free and get a reply spoken; say "hush" mid-reply and it stops. Turn the network off: the Voice section shows the "needs an internet connection" error and replies do not hang. Expert > Speech engine: Pocket and External still work; an install that had Pocket keeps Pocket.

- [x] Sidebar: separate status and agents chevron hit areas, collapsed count, and Enter/Space (#1410-e201). _(verified: production components in agent-browser; expanded/collapsed screenshots, trusted keyboard input, and 13 filtered component tests)_

- [x] Voice settings: accent Start, distinct Stop, and Running/Stopped indicator (#1409-4ec3). _(verified: accent colour in agent-browser screenshot; Running/Stopped component regression tests)_

## Remote desktop MCP routing (1419-ab18) — coordinated Rust rollout required


- [ ] After Boss schedules the desktop restart and coordinator deploys the reviewed daemon build, run `scripts/test-remote-mcp.py --connection <Mac-mint-id> --session <disposable-agent> --exercise`. Confirm remote output/submit, destination wake and reply to the Mac. Run with `--second-connection` and `--second-session` for a disposable second daemon peer. The live desktop does not load Rust changes without a restart; never restart it or a daemon holding live PTYs just to perform this check.


- [ ] After Boss schedules the Rust restart, verify a forwarded notice retry does not enqueue again while its ID is within the recipient's last 100 forwarded IDs, including after inbox reads. A retry after 100 newer IDs may deliver again; unregister must clear only that recipient's ring (#1419-ab18).
- [ ] Remote empty-grid replay control message (#1421-733e): staged Rust WS change requires a rebuilt backend; targeted loopback WS test and isolated headless fixture cover it before deployment. Keep the running desktop and mac-mint daemon intact until the coordinator schedules deployment.

- [ ] Remote replay rollout (#1421-733e): deploy the updated daemon before the updated client. An older daemon without the explicit empty-replay marker can trigger a false 15-second stream error on a healthy idle session whose initial grid is unavailable. Coordinate deployment after live PTYs can be safely preserved or closed; do not restart mac-mint during this incident.


- [ ] Workflow graph slice A (1446-ff21): after Boss rebuilds/restarts, inspect and cancel a pre-contract run; it must refuse Resume without changing its history. Graph runtime remains disabled. Targeted native store/replay tests cover the backend; the running desktop has not loaded these Rust changes.

- [ ] After Boss restarts the desktop or rebuilds release: verify the native workflow graph runtime uses serial predecessor history and refuses pause resolution while effects are uncertain or input is pending (1446 slice A). Internal graph transitions remain unavailable on the public transport.
## Private secret forms (#1435-6e1d) — Rust restart required

- [ ] (#1520-46b1) After Boss restarts the rebuilt desktop and updates the CLI/bridge, request a throwaway form. Confirm a separate native window opens, wait over 10 seconds, then decline; caller must receive names and `declined`. Repeat with harmless entry and verify only names/`stored` return. If absent, read source `secrets` logs: no `Secret tool dispatched` means the request did not reach this handler; `Opening` without `Creating` means host/store setup failed (see `Secret tool failed`); `Creating` without success/error means native construction did not return; a creation failure reports its native error; `created` means investigate visibility/frontend bootstrap. No desktop was launched by the peer.

- [ ] After restart, with a private form open, verify direct upstream MCP
  `tools/call` rejects inspection, matching native and `call_tool` entry points.

- [ ] After Boss restarts the desktop backend, request username/password/OTP
  fields with synthetic data. Verify the separate native window, exact reduced
  schema, main-window bootstrap rejection, decline, close and timeout cleanup.
- [ ] On a trusted existing HTTPS server address, submit a desktop-opened request
  from the one-time entry path; verify one-time consumption and cleared input UI.
- [ ] Verify exact argv/name/directory approval and template consent. Run a
  trusted test executable that prints synthetic encoded/wrapped values; confirm
  masked output and no terminal/tcap entries.

These desktop checks wait for Boss's restart; no second desktop instance is
launched by the implementer. Security critic and cross-platform validation are
required before treating the feature as complete.
- [ ] After Boss restarts the desktop build and updates the remote daemon, drop a read-only directory from Finder onto a remote repository. Verify all files arrive, final directory permissions remain read-only, and the Mac source stays untouched. Rust does not hot-reload; this requires a manual restart when Boss is ready. _(Story 1434 round 3: Linux handler tests cover upload deadlines, staging cleanup and cancellation; native Finder/macOS publication awaits restart.)_
## Remote MCP toast mirror (1439-d84f) — Rust restart required

- [ ] After Boss restarts `make dev` or installs a `make build` release, connect a daemon, raise an MCP toast from a remote agent, and check the host-labelled Messages entry, requested sound and Open terminal navigation. Disconnect, raise a toast remotely, and reconnect: no stale entry should appear. The mirror backend cannot hot-reload; do not restart Boss's desktop from an agent. Automated Rust/frontend regressions cover the filter, payload, navigation and disconnected-frame behavior.

Known limit for 1439-d84f: **Open terminal** is a harmless no-op when the remote PTY's cwd/repo is outside every registered remote repository. Navigation ownership currently comes from the repository registry. This is not fixed by the toast mirror change.
- [ ] #1411-097c: After a backend restart, copy a wrapped Claude prompt. Confirm only the outer `❯ ` and two-column margin disappear, width wraps join, and typed newlines remain. Select the pasted second glyph from column 2, content after an ASCII/wide prefix, and a VT continuation starting with `❯ `: literal glyphs and indentation must remain (#1414-4366). Rust changes require Boss to restart `make dev` or rebuild the release.

- [ ] #1418-48c7: After Boss restarts the backend, copy a literal prompt-shaped VT continuation whose predecessor was evicted from scrollback; the glyph must remain. Targeted grid regression verifies the extraction path; desktop clipboard check awaits restart.

- [ ] #1418-48c7: After the backend restart, clear history, fully erase the top row and redraw a real composer there, including with zero scrollback. Copy must remove the composer marker; purging history without erasing literal prompt-shaped content must preserve it. Automated regression coverage exercises full and partial line erasure.

- [ ] #1418-48c7: After Boss restarts the backend, move a literal prompt-shaped row with RI/IL and copy it at its new position: keep the glyph. Replace the entire row with ECH/DCH/ICH, redraw a fresh composer, and copy: remove only composer chrome. Partial edits must keep unknown-origin content literal. Automated grid and selection regressions cover these paths; desktop clipboard awaits restart.

- [ ] #1418-48c7: After Boss restarts the backend, issue ED1 (`CSI 1 J`) with the cursor on the second row: the first row must be blank. Redraw a fresh composer there and copy it: remove only composer chrome. First-row and last-row erase boundaries and the cursor-row suffix have automated grid regression coverage; desktop verification awaits restart.

- [ ] #1418-48c7: After Boss restarts the backend, fill every row with scrollback set to zero, clear the screen with ED2 (`CSI 2 J`), then redraw and copy a fresh composer on row zero: remove only composer chrome. Automated regressions cover two/three-row screens, the origin flag, and retained history with nonzero scrollback. ED3 must still preserve literal live content.

- [ ] #1418-48c7: After the backend restart, a composer on a blank row whose nonblank predecessor was evicted or purged may retain its `❯ ` when copied until a full row erase. This conservative limitation is accepted by Boss (2026-10-03, option a); the implementation uses one origin flag and no resize promotion.
- [ ] Telegram offline adapter boundaries (#1438-79b4): after a future rebuild, native startup remains disabled until stable-ID mail integration lands; no Telegram polling or secret reads are wired by slices 1–2.


- [ ] Telegram slices 1–2 polling recovery: after the next Rust rebuild and later native integration, verify visible in-memory 403/404 stops, fixed ten-update batches, bounded retries and alert-driven cursor reset. Offline adapter tests cover the cursor/network/mail-port boundary; daemon startup and operator UI remain deferred. Rust changes require Boss's manual restart to load.
- [ ] Speaker shutdown (#1452-f186, release gate #1447-a894): Rust fix is staged and requires Boss to restart `make dev` or rebuild the release when ready. After a spoken reply drains, close the voice conversation; the render worker must stop without hanging. A forced-interleaving regression covers shutdown during completion dispatch; the current desktop has not loaded this change.

## Debug sidecars for `make dev` / `make test` (1465-3d95) — desktop restart required

- [ ] After a fresh `make dev` (or `make test`) on a checkout with an empty `src-tauri/target`, confirm `src-tauri/target/debug/tuic-bridge` and `tuic` exist and the app finds the bridge (`locate_bridge_binary`). After editing `crates/tuic-bridge/src`, restart: the bridge mtime must change. Do not launch a second desktop instance from an agent lane.
- [ ] After the approved daemon update, launch Claude manually on the configured Mac-mint connection, exit back to the shell, and verify submit/mail no longer write there; a run-config preset must survive shell startup (#1420-f3de).

- [ ] After the coordinated rebuild, verify shell-root return revokes a manually launched agent regardless of shell basename; a bash-script wrapper is observed, and a nested subshell under a directly spawned agent holds submit/mail until the agent regains foreground (#1420-f3de). Do not restart live PTYs for this check.
## Concurrent workflow checks (story 953-feed) — Rust restart required

- [ ] After Boss restarts the Rust backend, run independent published checks on disposable workflow runs. Confirm both subprocesses can progress concurrently and a notification event during a check does not discard its receipt. Confirm a changed worktree or cancelled run cannot acquire a receipt. The current backend cannot load this Rust change without a manual restart.

## Workflow merge-tree verification (story 957-dc59) — Rust restart required

- [ ] After Boss restarts the backend, use disposable repositories to verify a clean checked merge receives a receipt, an extra integration-time file does not, and a manually resolved conflict requests separate review. The running Rust backend cannot load the change without a manual restart.

## Workflow recovery boundaries (story 960-8670) — Rust restart required

- [ ] After Boss restarts the backend, verify restart recovery marks old attempts interrupted. On disposable active runs, a runtime reconciliation must preserve healthy attempts and intended effects; one corrupt run must not prevent a healthy run from recovering. After a dependency-refresh failure during recovery, resume the run and start a worker; reopening must preserve that live worker. Failed recovery is not retried by later opens or runtime reconciliation. Startup Git-probe latency remains pending story 959-c69c.
## Windows core dependency (1478-ead1) — rebuild required

- [ ] Load this manifest fix in the next Windows build. The `tuic-core` MSVC cross-check passes; a separate WebRTC/Abseil C++ build failure is tracked in 1479-f956. Existing desktop processes do not hot-reload Rust; restart only when Boss is ready.

## Security Group C — rebuild required

- [ ] After the next Rust restart, verify run-git rejects unsupported options and GitPanel fetch/push/merge still work (#1460-9ed8).

- [ ] After the next Rust restart, verify force branch deletion retains its tip at refs/archive and refuses archive collisions (#1462-e0e9).

- [ ] After the next Rust restart, verify plugin CLI output over a pipe buffer completes and overflow returns an explicit error (#1461-8d36).

- [ ] After the next Rust restart, verify creating feat-x cannot remove an existing worktree for feat/x (#1458-e1f6).

- [ ] After the next Rust restart, verify remote connection failures contain no token in logs or status and session/SSE mirroring authenticates with the existing cookie (#1457-91e0).
## Windows WebRTC compiler (1479-f956) — native Nightly required

- [ ] After landing and pushing, the Windows Nightly must report MSVC for both Meson C/C++ compilers, compile Abseil/WebRTC without MinGW header errors, and finish the Tauri NSIS build. This Rust build-script change requires rebuilding; no desktop instance was started for verification.

## Codex notify publication (1483-7a6e) — Rust restart required

- [ ] After Boss loads the rebuilt backend, confirm a disposable Codex session still reports turn completion. The script is now published with owner execute permission already set; the existing concurrent-publication regression covers the race. Rust does not hot-reload; no desktop instance was launched by this lane.

## Windows runtime CI fixes (1518-d3a7) — Rust restart required

- [ ] After Boss restarts the Rust build, verify Windows workflow worktree assignment and orphan cleanup with native and Git path spellings, archive hooks that invoke Git, and failed/cancelled remote-copy staging cleanup. Native Windows CI after landing remains required; the live desktop backend does not hot-reload these Rust edits.

- [ ] After Boss restarts the Rust build on Windows, verify Claude session discovery and the subagent view under a drive-letter checkout; the project slug must keep its drive letter and replace every non-ASCII-alphanumeric character (including the colon and profile spaces) with a dash. The running desktop backend does not hot-reload this fix.

- [ ] After Boss restarts the Windows Rust build, verify a captured `tuic bg` launcher returns while its command still runs and a worktree archive hook finds Git. Existing CI regressions exercise both contracts; the live desktop backend does not hot-reload these changes.

- [ ] After Boss restarts `make dev` or rebuilds, verify Windows worktree archive/setup hooks find Git with a long inherited PATH. Hook PATH now keeps Git first, deduplicates directories and stays within cmd.exe limits.
- [ ] After the next backend restart, cancel a workflow while a published check is running and confirm its workers stop (#954-4f33). Automated regression covers process-tree teardown; the running backend must be restarted to load this change.

- [ ] After backend restart, verify CLI/local and authenticated browser workflow actions succeed and story history records local_api or human provenance (#956-9745, #1497-4f55). Actor identity is tracking only. Rust changes require a manual make dev restart (or make build); automated route and provenance regressions cover the backend contract.

- [ ] After backend restart, verify workflow plan Done becomes Active after canonical branch movement and Done after recertification (#962-0888); manual approval completion is preserved. Targeted Git integration tests cover both projections.
- [ ] After Boss restarts `make dev` (Rust does not hot-reload), exceed the scrollback cap and verify retained command navigation, gutter selection, green prompt ticks and answers-only associations stay on their original rows (#1370-0077). Restart must load backend and frontend together because stored OSC row coordinates changed.
- [ ] Queue retry idempotency (1106): after Boss rebuilds/restarts the backend, send the same `idempotencyKey` twice to an isolated agent queue, then retry after it drains; verify one wake and `accepted: true` without requeue. Automated HTTP/PTY tests cover bytes and queue state; this check loads the Rust change into the running app. Live per-CLI turn acceptance and composer/reconnect convergence remain separate open criteria.
## Remote transfer cancellation fixture (1528-cee8) — Rust rebuild

- [x] The cancellation regression must retain worker staging and its upload permit after handler abort, publish both fixture files, clean staging, and release both slots. Production still passes the unchanged extractor. _(verified by source inspection: `remote_transfer.rs` receive/worker ownership and channel-gated regression in `remote_transfer_tests.rs`; targeted execution is recorded in the story worklog. Rust changes require Boss to restart `make dev` or rebuild release before loading; this refactor adds no new runtime behavior.)_
## Telegram minimal outbound (#1438-79b4) — Rust rebuild required

- [ ] After Boss rebuilds/restarts the headless daemon, a directly observed Claude-to-Codex replacement in the same terminal must require fresh Telegram registration, even without a shell observation. Offline regression: `observed_agent_type_change_does_not_transfer_registration` (#1526-22f3).

- [ ] After Boss rebuilds/restarts the headless daemon, register an agent, let it exit to its shell, then restart an agent in that terminal. Phone mail must receive `Nessun agent registrato` until the replacement explicitly registers. Offline native foreground/inbox coverage: `observed_agent_exit_does_not_transfer_registration_to_restarted_agent` (#1524-dcc1). The running backend needs a restart to load this Rust change.
- [ ] After Boss rebuilds/restarts the headless daemon, verify drafts have no phone Stop, send/notifications use the single configured chat, and a button press or new message retires previous handles. Live mint verification remains coordinator-owned; no instance was launched here. Rust changes do not hot-reload.
- [ ] After Boss rebuilds/restarts the headless daemon, verify Telegram Stop on
  a throwaway phone request: one Escape reaches only the draft-bound current
  epoch, including a replacement Enter delivered before input bookkeeping;
  duplicate/stale Stop never revives a draft. Confirm the selected
  callback label stays disabled. The running Rust backend cannot load these
  changes until rebuilt/restarted; no restart was performed by this peer.

- [ ] Telegram setup (#1515-cb81): after rebuilding/restarting tuic-remote, use Settings on desktop and phone to replace/check a token, observe the MCP-registered agent read-only, enable, pair once within ten minutes or type a chat ID, remove a chat, and inspect safe status. Rust changes require a restart; no desktop instance was launched by the peer. Live Telegram authentication was not exercised.
- [ ] After Boss rebuilds/restarts TUIC: Settings > Agents > Codex shows the migrated bypass argument and warning; remove it and confirm terminal and managed launches preserve the removal. The durable migration stamp and wrapper task routing require a rebuilt backend; also confirm a default wrapper receives its positional task. Rust migration requires restart. Visual screenshot attempt could not render the isolated harness while the macOS screen was locked. (#1399-17bd)

- [ ] After Boss rebuilds/restarts the backend, confirm an interactive Codex default with `--profile review` (or `exec`/`e`) receives and submits its managed task. Actual subcommands after root options must retain positional tasks. The public spawn regression covers argv and queued delivery; the running Rust backend requires restart. (#1513-701d)
- [ ] After Boss restarts `make dev`, verify `claude remote-control --resume main` and `claude auth status` pass through without TUIC settings and normal Claude launches retain status hooks, including when cached help is empty or lacks usable command rows (#1405-a5e4).

- [ ] After Boss restarts the Rust backend, verify a new headless terminal at 148 columns retains that width after a same-size resize (#1413-7dcc).
- [ ] After Boss restarts the Rust backend, confirm that remote MCP questions show the saved host name, answer only the owning daemon, and disappear when another client answers (#1440-3571).

- [ ] After rebuilding Rust, verify remote GitHub review/proposal/conflict notices update the owning dashboard and leave same-path local repositories unchanged (#1443-e2fd).

- [ ] After rebuilding Rust, verify remote upstream MCP failures show their host with the popup closed and leave local upstream settings unchanged (#1444-95a4).

- [ ] After the Rust restart, check a connected daemon ACP permission/elicitation in AI Chat: host and ACP connection are shown, answer returns to that daemon, settlement/disconnect clears only its cards (#1441-695e).

- [ ] After the Rust restart, check remote GitHub PR transition bell/native notices show the host once, open remote PR details and never touch a same-path local repo (#1442-2100).

- [ ] After the next backend restart, verify an ego card opens mobile Chat and shares the question push cooldown (#1078-05cf). Rust changes require a manual restart by Boss.
- [ ] After Boss restarts the Rust backend, verify MCP `branch_delete` reports the tip-suffixed archive ref when the primary archive holds older work (#1489-1a14). Targeted regression covers the backend; the running desktop still requires restart.

- [ ] After Boss restarts the Rust backend, confirm the Git diff file list displays tracked-file additions/deletions (#1499-3a34); targeted backend regression covers scopes and renamed paths.

- [ ] After Boss restarts the Rust backend, confirm untracked files with tabs/newlines or boundary spaces appear with their literal names and correct line counts (#1502-8a5e). Backend regressions cover listing and file-diff consumers; Rust does not hot-reload.
- [ ] After rebuilding/restarting TUIC, verify sidebar dirty and merged badges with 11 writing worktrees, including initialized submodules; sample Git child spawns with the same before/after method (1491-2ae4). Rust backend changes require a manual restart. Include recovery after a PR proof lookup failure without moving refs, and `git rm --cached` leaving both a staged deletion and an untracked file (dirty count 2).
## Hands-free reply controls audit (#1377-16e1)

- [x] Pause and resume preserve playback ownership without opening a user turn. _(verified: src-tauri/crates/tuic-dictation/src/speaker.rs:590 pause_by_user/resume_by_user only change playback and hold flags; generation changes in hush, not pause. src-tauri/src/dictation/commands.rs:1285 does not change capture state.)_
- [x] Browser resume continues from the saved sample position without suspending the microphone context. _(verified: src/utils/browserVoice.ts:186 saves the elapsed offset and restarts playback from it; capture remains connected.)_
- [ ] [VISUAL] Capture the pill while speaking and while user-paused after Boss loads the desktop build. Check Pause/Stop and Play/Stop respectively. No test desktop may be launched by a managed peer.
- [ ] [HUMAN] On a real phone, tap Pause, Play and Stop during a spoken reply. The reply resumes at its position; Stop drops queued replies; microphone capture stays active. Mouse command dispatch, accessible labels and 44px coarse-pointer targets are present in source; they do not prove real-phone touch/audio behavior.
- [ ] Native MCP registry Settings (#1522): after Boss rebuilds/restarts the backend, open Settings → MCP → Native tools; confirm all registry tools have switches and description badges, disabled tools can be re-enabled, and Telegram appears when its backend registration lands. Rust does not hot-reload.

- [ ] AI Chat setup (#1406-06f2): with an empty ego executable, confirm the inactive explanation and Configure ego button in inline and detached panels. The button opens Settings → General at the ego controls; selecting the executable shows the composer without restart. Return to the detached window after saving to refresh its settings. _(Automated behavior tests cover routing and activation; agent-browser screenshot attempt was blocked by the locked macOS screen.)_

- [ ] AI Chat message fork: after rebuilding/restarting Rust, fork the second of four ego replies and check the child cutoff while the parent retains all replies. The backend capability and metadata changes require a manual restart.

## Worktree removal with a submodule checked out — after `make dev` restart

Rust (`tuic-git` `remove_worktree_internal_with_lock`): needs a TUIC restart.

Reported bug (wip): clicking the "X" in the sidebar to delete a worktree whose
checkout has an initialized git submodule (e.g. `plugins/`) failed with
`fatal: working trees containing submodules cannot be moved or removed`. On main
this is handled by tuic-git's removal (cleanliness + submodule check, then one
`--force`); wip's own Safe-mode retry was not carried over — verify main's path.

- [ ] Create a worktree from a repo that has the `plugins/` submodule
  initialized inside it (or any repo with a submodule checked out in the
  worktree), then delete it via the sidebar "X". It should succeed and the
  row should disappear — no more "cannot be moved or removed" error in the
  logs.
- [ ] Same, but first add/modify a file in the worktree (uncommitted change)
  before deleting — the app should show the normal "discard uncommitted
  changes?" confirmation instead of silently deleting.

## DSR/CPR reply latency — `leaf` and other cursor-position-querying tools (2026-08-29, **Rust change — needs `make dev` restart**)

Reported bug: running `leaf <file.md>` (a terminal markdown pager) in a
TUICommander tab printed `^[[3;1RError: The cursor position could not be read
within a normal duration` instead of rendering the file. Root cause: TUIC's
cursor-position (DSR/CPR, `ESC[6n`) reply was only flushed to the child's
stdin *after* an expensive per-chunk screen-diff, which raced against `leaf`'s
own short internal deadline on a session's first paint. Fixed in
`pty.rs::process_chunk` (flush immediately after draining VT events) and in
the frame ticker's stalled-sync-update flush path (`terminal_grid.rs`'s new
`drain_pty_write_events`), which had the same class of bug for a rarer trigger
(a BSU/DEC-2026 block that never sees its ESU within 150ms).

- [ ] After restart, run `leaf <any-markdown-file>` in a TUIC tab. It should
  render the file normally — no raw `ESC[...R` text printed before/instead of
  the rendered output.
- [ ] Same, but on a large file (so the pager's first paint is a big,
  multi-row repaint) — this is the specific case that raced before the fix.
- [ ] Any other tool that queries cursor position on startup (e.g. some
  readline-based REPLs, `tput`-driven scripts) still gets a correct reply and
  behaves normally — this fix changes reply *timing*, not content, so nothing
  should look different, only faster/more reliable.

## OSC 9;4 progress bar — error/warning/indeterminate states

All four states (normal/error/indeterminate/warning), with and without a
percentage, were verified via synthetic `printf` injection on both desktop
(tab bar) and mobile (SessionCard + SessionDetailScreen header) — screenshots
in `.screenshots/progress-bar-error-warning-indeterminate/` in the main
checkout. Two things weren't covered by that synthetic testing:

- [ ] Watch for a real long-running tool (a build, a package manager, an AI
  agent) that emits OSC 9;4 state 2 (error), 3 (indeterminate), or 4 (warning)
  during normal daily use, and confirm it renders as expected end-to-end (not
  just via injected test sequences). Most real emitters observed so far
  (cargo, nextest) only use state 1 (normal).
- [ ] Enable "Reduce motion" (macOS System Settings → Accessibility, or the
  OS-level `prefers-reduced-motion` toggle) and confirm the indeterminate
  sweep animation actually stops rather than continuing to animate — the
  `@media (prefers-reduced-motion: reduce)` rule in `global.css` is a blanket
  `*, *::before, *::after { animation-duration: 0.01ms !important }`, which
  should cover it, but this wasn't visually confirmed with the OS setting
  actually toggled on (only reasoned about from the CSS cascade).
- [ ] Mobile session detail (main's layout has no header progress bar): open the
  overflow menu and confirm the Progress entry shows `NN%`, or the kind name
  (e.g. `indeterminate`) when no percentage was sent.

## UI fixes batch (2026-08-31, frontend only — no `make dev` restart needed)

Nine UI/UX fixes plus three follow-on fixes for issues the same session's own
code review and audit surfaced (a Space-key double-toggle, a Tab accessibility
regression, an uncancelled animation frame, plus three more `autofocus`-on-
dynamic-insertion sites, a keystroke-drop race window, and the New Worktree
dropdown's fixed-direction clipping). Most of the original nine were live-
verified against a real built app via `agent-browser` with real keystrokes
(scope chips + Tab-cycling, the Menu-label focus/scroll fix, the Compose-vs-
Edit-Prompt focus trap, dialog width/autofocus, both renames — see
`.screenshots/ui-fixes-batch/` in the main checkout for the captured evidence)
and are not repeated here. What's left needs a human pass:

- [ ] **New Worktree dialog — Tab into "Start from", then type** — open the
  dialog, Tab from the name field until the "Start from" trigger is focused
  (verify with a visible focus ring), type a letter. It should open the
  dropdown and filter by that letter in the search box — nothing should be
  typed into the terminal behind the dialog. Also type a second letter
  immediately (as fast as you can) right after the first — both should land in
  the search box, not just the first.
- [ ] **New Worktree dialog — "Start from" popup direction** — the popup now
  picks upward or downward based on the trigger's actual on-screen position
  (recomputed every open), rather than always opening upward. Needs a repo
  with 2+ branches (the one repo checked live during this session's own
  verification pass had only one, so the dropdown never appeared). Resize the
  window short and confirm the popup opens toward whichever side has more room
  and is never clipped, in both a tall and a short window.
- [ ] **Compose append vs. overwrite** — with a terminal's Compose panel open
  and some draft text typed in, trigger a Smart Prompt with "compose" as its
  inject target (e.g. from the Prompt Library, with auto-execute off). The
  prompt's content should be appended below the existing draft (blank line
  separator), not silently dropped and not overwriting the draft. (Live
  verification this session covered the adjacent focus-trap fix by typing
  directly into the Content field, not this exact "trigger a real Smart
  Prompt" path.)
- [ ] **Settings → Smart Selection — first-edit-on-a-fresh-config case** — on
  a config with no prior Smart Selection customization (so the rule list is
  showing built-in defaults, unmaterialized), expand a rule, type into "Menu
  label", and confirm focus and scroll position both survive that first edit.
  (Live verification this session used a config that already had
  materialized rules, so it didn't exercise the identity-flip-on-first-edit
  path specifically, only the steady-state per-keystroke case.)
- [ ] **Three more `autofocus`-on-dynamic-insertion fixes** — confirm each
  actually focuses when shown: Prompt Library drawer's search box on open
  (`Cmd/Ctrl+Shift+K`), the Knowledge history overlay's search box on open,
  and a repository group's rename field (Settings → Appearance → Repository
  Groups → double-click a group name).
- [ ] **Tab-name-flapping fix (rustc requires `make dev` restart)** — fixed an
  infinite `session-renamed` echo loop between the frontend's `update()` (in
  `terminals.ts`) and the backend's `set_session_name` (`pty.rs`'s Tauri
  command and `session.rs`'s HTTP twin): both now no-op an unchanged
  name/is_custom pair instead of unconditionally re-emitting, and the
  frontend now also skips its echo-back RPC when the value hasn't changed.
  Covered by automated tests (`set_session_name_skips_emit_when_unchanged`,
  `rename_pane_is_idempotent_and_only_emits_on_real_change`,
  `terminals.renameEchoGuard.test.ts`), all verified to fail without the fix.
  What automated coverage can't reach: the actual live symptom — a tab's
  title visibly flickering/relabeling while an agent works, or during a tmux
  swarm session with `select-pane -T` — and the sustained CPU spike this was
  causing (~125-130% observed in this session's own orchestrator instance
  logs before the fix). After restarting `make dev`, open a tab running an
  active agent for a couple of minutes and confirm the tab name stays stable
  (only changes when the agent's title/status genuinely changes), and check
  `GET /diagnostics` (enable diagnostic mode first) for CPU staying idle
  between real activity instead of pinned high.
- [ ] **Swarm teammate panes landed under the wrong repo's tab group (needs
  `make dev` restart, then live rebuild + reinstall to real `~/bin/tmux` per
  AGENTS.md).** _(found + fixed 2026-09-04, live report from Boss.)_ A
  4-teammate `agent-teams` swarm spawned from an `ai-usage` session rooted
  in `commerce-journal` had all 4 teammate panes appear under an unrelated
  repo, `databricks-sql-cli` — the repo Boss happened to be actively working
  in at that moment. Root-caused: Claude Code's real swarm calls never pass
  `-c <cwd>` on `new-session`/`split-window` (confirmed empirically, both in
  this session's earlier live captures and the synthetic wording harness);
  `tuic-cli`'s arg parser only ever reads `-c` and left `cwd: None`
  otherwise, so `spawn_pty_session` (`session.rs`) never called `cmd.cwd()`
  and the child PTY just inherited whatever directory the TUICommander app
  process itself happened to be running in — matching no registered repo.
  The frontend's `assignSessionToRepoBranch` (`useAppInit.ts`) correctly
  falls back to "park it in the sidebar's currently *active* repo" when a
  session's cwd owns no registered repo — which is exactly, and only,
  because the cwd it received was wrong in the first place. Fixed with a
  `resolve_cwd()` helper in `tuic-cli/src/tmux/exec.rs`, applied at all
  three creation call sites (`new-session`, `new-window`, `split-window`):
  falls back, in priority order, to (1) an existing pane's already-resolved
  cwd elsewhere in the same session/window, then (2)
  `std::env::current_dir()` (the tmux CLI subprocess's own cwd, inherited
  from Claude Code's real process) whenever `-c` is absent — matching real
  tmux's own default behavior, not TUICommander's own fabricated default.
  A `/code-review` pass (scoped to just this fix) found the topology-inheritance
  step was missing initially — each `tmux` subcommand is its own OS
  subprocess, so relying on a fresh `current_dir()` read alone for every
  pane could in principle disagree with an earlier pane's if the calling
  process's own cwd ever changed between separate invocations; closed off
  by preferring an already-resolved sibling pane's cwd first. That same
  review flagged two more issues, accepted as-is (documented in
  `tuic-cli/AGENTS.md`): `current_dir()`'s symlink-resolving semantics
  could still mismatch a repo registered via a symlinked path (pre-existing,
  general gap, not introduced or worsened by this fix), and this crate now
  has multiple independent inline `current_dir()`-stringify implementations
  instead of one shared helper (real duplication, deferred as a separate
  cleanup rather than scope-creeping this fix). New regression tests
  (`new_session_without_dash_c_falls_back_to_the_real_cwd_not_none`,
  `split_window_without_dash_c_falls_back_to_the_real_cwd_not_none`,
  `new_window_without_dash_c_falls_back_to_the_real_cwd_not_none`,
  `new_session_with_dash_c_still_honors_the_explicit_value`,
  `legacy_new_session_without_dash_c_never_gets_resolve_cwds_fallback`,
  plus a `resolve_cwd`/topology-helper unit test group), all 125 `tuic-cli`
  tests green, full workspace (`cargo nextest run --workspace`, 5477 tests)
  green, fmt/clippy clean. **Not yet live-verified** — this is a `tuic-cli` crate
  change; per AGENTS.md's "Fresh Worktree Setup" point 4 and "Dev Hot
  Reload," the real `~/bin/tmux` alias execs whichever `tuic` binary its
  owning `TUICommander.app` bundle ships, not `target/debug/tuic` — testing
  this live needs a real rebuild (`make build` or `pnpm build:sidecar
  --force`) that actually replaces that bundle's binary, not just `cargo
  build`. After that: reproduce the exact repro (spawn a swarm from a
  session in one repo while a different repo's tab is focused/active) and
  confirm the new teammate panes land under the *originating* repo, not the
  focused one.
- [ ] **Reworded the "Prefer TUICommander for peers/teams" connect-time bullet
  (needs `make dev` restart to take effect).** _(changed 2026-09-04, based on
  an n=10-confirmed A/B harness result — see
  `plans/docs/agent-teams-wording-harness/README.md`.)_ The old hedged phrasing
  ("use TUIC's `agent action=spawn` MCP tool (not your host's native
  subagent/Task/team tool) whenever spawning an AI peer that should be
  observable, messageable, and visible as a tab in TUICommander...") scored
  0/10 against a real Claude Code session running as Opus, choosing between
  this tool and tmux on an identical spawn-two-teammates scenario. A blunt,
  unhedged directive scored 10/10 on the same scenario. `build_mcp_instructions`
  (`mcp_transport.rs`) now reads: "every agent-teams teammate is created with
  TUIC's `agent action=spawn` MCP tool — do not use your own built-in
  agent-spawning tool for this, at every level..." — same bold label
  (`**Prefer TUICommander for peers/teams:**`, so existing tests/greps still
  match), same trailing carve-out/scope clauses (untested against removal,
  so left in for safety), only the opening directive reworded to match what
  was actually measured to work. All 339 `mcp_transport` tests green,
  fmt/clippy clean. `mcp-instructions-examples.md` (main checkout's `plans/`)
  updated to match, plus a new §11 documenting the separate `agent` tool
  schema (`tools/list`) wording, which the same harness found has NO
  measurable effect (unlike this connect-time bullet). **Not yet
  live-verified** — after a `make dev` restart, a fresh Claude Code
  agent-teams spawn (ideally running as Opus, the case this specifically
  targets) should now reach for `agent action=spawn` rather than falling
  through to tmux noticeably more often than before. Given the harness
  showed near-100% vs near-0% rates, a single live test should already be
  fairly telling, though the harness's earlier finding that model choice
  dominates everything else means don't read too much into one Sonnet-backed
  trial (Sonnet already picked TUIC's tool 100% of the time regardless of
  this wording).
- [ ] **Tab-name-flapping fix (rustc requires `make dev` restart)** — fixed an
  infinite `session-renamed` echo loop between the frontend's `update()` (in
  `terminals.ts`) and the backend's `set_session_name` (`pty.rs`'s Tauri
  command and `session.rs`'s HTTP twin): both now no-op an unchanged
  name/is_custom pair instead of unconditionally re-emitting, and the
  frontend now also skips its echo-back RPC when the value hasn't changed.
  Covered by automated tests (`set_session_name_skips_emit_when_unchanged`,
  `rename_pane_is_idempotent_and_only_emits_on_real_change`,
  `terminals.renameEchoGuard.test.ts`), all verified to fail without the fix.
  What automated coverage can't reach: the actual live symptom — a tab's
  title visibly flickering/relabeling while an agent works, or during a tmux
  swarm session with `select-pane -T` — and the sustained CPU spike this was
  causing (~125-130% observed in this session's own orchestrator instance
  logs before the fix). After restarting `make dev`, open a tab running an
  active agent for a couple of minutes and confirm the tab name stays stable
  (only changes when the agent's title/status genuinely changes), and check
  `GET /diagnostics` (enable diagnostic mode first) for CPU staying idle
  between real activity instead of pinned high.

## Second `AskUserQuestion` in one session never showed "awaiting" (2026-08-30, **Rust change — needs `make dev` restart**)

Reported bug: a hook-instrumented Claude tab correctly showed "awaiting" for its FIRST
`AskUserQuestion` in a session, but a second, later `AskUserQuestion` in the same session showed
"working" — genuinely blocked on the user with no badge to show it. Confirmed via a live capture
with Claude Code's own hooks fully instrumented (`.claude/hook-debug.log`): Claude Code's
`PreToolUse(AskUserQuestion)` hook fired correctly both times, and the raw OSC 7770 stream carried
`state=awaiting` for both questions — the break was entirely inside `pty.rs`'s own dedup logic.
Root cause: `tuic_state_awaiting_event()`'s hook-based mapping always builds
`ParsedEvent::Question{prompt_text: String::new(), ..}` (there's no real question text on the
bare protocol marker), and the question-dedup guard a few chunks later keyed off that same
(always-identical, always-empty) `prompt_text` with no regard for source. The screen-absence reset
meant to retire the dedup key can never fire for an empty string (`anything.contains("")` is
always true), so the first `AskUserQuestion` permanently poisoned the dedup for the rest of the
session. Fixed by only applying that dedup when `prompt_text` is non-empty. Regression test
`claude_double_askuserquestion_both_survive_dedup` (`pty.rs`) replays a real captured two-question
session through the actual production `process_chunk` hot path and asserts all four raw
`state=awaiting` markers survive — confirmed it fails (count stuck at 1) against the pre-fix code
and passes (count 4) with the fix.

- [ ] After restart, in a hook-instrumented Claude session, trigger `AskUserQuestion` twice in a
  row (answer the first, let it lead into a second, separate `AskUserQuestion` call) — the tab
  should show "awaiting" both times, not just the first.
- [ ] Same, but with a real amount of work between the two questions (not back-to-back) — confirms
  the fix holds once other busy/idle churn has happened in between.
- [ ] A single multi-sub-question `AskUserQuestion` (the "Pick 3 options, Submit" wizard shape)
  still re-arms correctly between sub-questions — this fix is unrelated to that mechanism
  (`rearm_awaiting_for_open_dialog`) but should be spot-checked for a regression anyway since both
  paths touch `awaiting_input`.

## Mid-turn `AskUserQuestion` busy re-affirmation silently dropped the awaiting clear (2026-09-01, **Rust change — needs `make dev` restart**)

Live-reproduced on two real, unattended/auto-approve Claude sessions (`commerce-journal`'s
`publish` tab, `agent-tooling-analysis`'s `tool-scan` tab): the awaiting badge stuck `true` through
full task completion. Root cause: `PreToolUse(AskUserQuestion)`'s `awaiting` override never touches
the shell busy/idle bit, so shell state was already `SHELL_BUSY` before and after a mid-turn
`AskUserQuestion` — the ordinary case. `PostToolUse(AskUserQuestion|ExitPlanMode)`'s busy
re-affirmation, kept specifically to clear the badge, landed on that already-busy state
(`busy_transitioned == false`) and its clear was silently dropped — nothing else is allowed to
retract a confident question besides real user input, which never arrives when the agent answers
its own question unattended. Fixed: `tuic_state_awaiting_event` now clears on
`busy_transitioned || currently_awaiting`. _(NOTE, rebase onto main: main already clears awaiting
on EVERY hook `busy` — no edge gate, prompt row moved to the `prompt` verb (#1388) — so this half is
carried by main's code; the `ChoicePrompt` suppression below was NOT carried over, because main
deliberately screen-scrapes Claude dialogs in hooked sessions too (AskUserQuestion choices for
mobile, #1212); the `test_chunk_processor_choice_prompt_suppressed_when_hook_instrumented` test
was dropped.)_ Separately, the `ChoicePrompt` screen-scrape detector
had no hook-instrumented suppression at all (only the generic `Question` heuristic did, via
`suppress_heuristic_question`) — fixed to skip detection outright for a hook-instrumented session.
Both covered by regression tests in `pty.rs`
(`claude_askuserquestion_midturn_busy_reaffirmation_clears_stuck_awaiting`,
`test_chunk_processor_choice_prompt_suppressed_when_hook_instrumented`), each individually
confirmed to fail against the pre-fix code. Full incident write-up, a related-but-still-OPEN
`Notification`-hook confidence gap, and an investigation playbook for the next report in this
class: `agent-signal-architecture.html#incidents`.

Debug logging (all at `tracing::debug!` — the app's default level is `info`; needs
`RUST_LOG=info,tuicommander_lib::pty=debug,tuicommander_lib::state=debug` to actually see them) was
added throughout the awaiting-input state-transition path specifically so a future report doesn't
need this same archaeology — see the playbook link above for exactly what's logged where. A
temporary global Claude Code hook (tagged `# tuic-debug-hook-2026-09-01 (temporary, research)` in
`~/.claude/settings.json`) logs every raw hook payload to `.claude/hook-debug.log` per-repo while
`TUIC_SESSION` is set — intentionally left in place "for the time being"; remove it (grep the
sentinel) once this research window closes.

- [ ] After restart, reproduce the original report if possible: an unattended/auto-approve Claude
  session that calls `AskUserQuestion` (or hits a Bash permission prompt while hooks are
  installed) mid-turn — confirm the tab correctly shows "awaiting" and then correctly clears back
  to busy/idle once the turn continues, instead of sticking.
- [ ] With `RUST_LOG` elevated per above, confirm the new debug log lines actually appear in
  `GET /logs` during a real session — spot-check at least the `state.rs` generic awaiting-diff
  line and the `pty.rs` shell-state-edge lines.

## macOS Finder Service — "New TUICommander Tab Here" (2026-09-10, **Rust change — needs `make dev` restart or a packaged build**)

New feature: right-click a folder (or file) in Finder → a terminal pane opens there, filed under
the right repo group via a 3-rung placement ladder (owning repo → active repo → ask the user).
Ships as a hand-authored Automator `.workflow` bundle (`src-tauri/services/`), verified functionally
via `automator -i <path> "services/New TUICommander Tab Here.workflow"` for a single item, a
multi-item selection, and a plain file — but **never through a real Finder right-click**, which
needs a packaged/installed build for Launch Services to pick up the bundle from
`~/Library/Services/`. See `docs/user-guide/finder-integration.md` and `FEATURES.md` §17.4.2 for
the intended behavior.

- [ ] Install from Settings → General → Finder Integration on a packaged build (or accept the
  first-run prompt), then right-click a folder in Finder — confirm "New TUICommander Tab Here"
  appears in the menu and opens a pane with that folder as cwd.
- [ ] Right-click a **file** — confirm the pane opens at the file's parent directory, not the file
  itself.
- [ ] Right-click a folder inside a repo you've already registered (repo root, and separately a
  linked worktree) — confirm the pane is filed under that repo/branch in the sidebar, with cwd
  equal to the exact folder clicked (not the repo root) when clicking a nested subfolder.
- [ ] Right-click a folder outside every registered repo while a repo is active — confirm the pane
  is filed under the active repo.
- [ ] Right-click a folder outside every registered repo with **no** active repo — confirm the
  "Open terminal in which repo?" picker appears, and each of its four outcomes works: choosing an
  existing repo, "Add this folder as a repository", "Open unattached terminal", and Cancel/Escape.
- [ ] Select 3 folders and invoke the service — confirm 3 panes open. Select more than 5 — confirm
  only the first 5 open.
- [ ] Quit TUICommander entirely, then invoke the service from Finder — confirm it launches the
  app and still opens the pane once ready.
- [ ] Remove the integration from Settings → General → Finder Integration → Remove — confirm the
  Finder menu item disappears (may need a moment for `pbs -flush` to take effect).
- [ ] Confirm the first-run prompt never reappears after being dismissed (accept or decline), and
  that Settings always reflects the true installed/not-installed state.
- [ ] **[VISUAL]** No screenshots were taken during implementation — both new visual pieces
  (`RepoPickerDialog`, and the Settings → General → Finder Integration section) are gated behind
  `isTauri()`/a real deep link, which browser-mode can't reach without either risking the URL
  scheme resolving to Boss's live orchestrator instance instead of a test build, or standing up a
  second debug instance unnecessarily for what's fundamentally a CSS check. Both reuse existing,
  already-shipped CSS (the shared `dialog.module.css` shell; GeneralTab's existing CLI-section
  classes) with only new page-specific styling in `RepoPickerDialog.module.css` genuinely
  unverified visually. While doing the real Finder round-trip above, screenshot the picker dialog
  and the Settings section and save them to `.screenshots/finder-service/` in the main checkout.
## Branch From: real branch list + stale-setting warning (2026-09-10, frontend only — no `make dev` restart needed)

Frontend-only change (no Rust touched): `RepoWorktreeTab` now lists the repo's real local/remote
branches (via the existing `list_base_ref_options` command) instead of a hardcoded
main/master/develop list, and the Create Worktree dialog now consults the repo's "Branch From"
setting when preselecting a base ref, with a non-blocking warning when the configured branch has
since been deleted. Covered by 40+ new/updated vitest cases (coordinator, `RepoWorktreeTab`,
`CreateWorktreeDialog`, `repoSettings`), but **no live/visual verification was done** — this
worktree had no prior build (`dist/`, sidecar binaries), and standing up a full `make dev` instance
purely to eyeball CSS on a frontend-only change wasn't judged worth the build time. Needs:

- [ ] Settings → *a repo* → Worktree Configuration → **Branch From**: confirm the dropdown shows
  `Automatic` plus `<optgroup>`s "Local"/"Remote" populated with the repo's actual branches (not a
  static main/master/develop list), and that a repo without `develop`/`master` doesn't offer them.
- [ ] Set Branch From to a real branch, then delete that branch (`git branch -D <name>`) outside
  the app and let the repo refresh — confirm the dropdown keeps showing the stale value with a
  "(no longer exists)" option label, plus a warning line below the dropdown.
- [ ] With that stale setting still configured, open **Create Worktree** (`+`) for that repo —
  confirm the "Start from" control shows a "Select a branch…" placeholder (nothing preselected),
  a warning row with a tooltip icon appears above the preview footer, and **Create still works**
  (the warning must never disable the Create button).
- [ ] Repeat with a repo where Branch From names a branch that still exists — confirm it *is*
  preselected as "Start from" when no session-remembered ref exists yet for that repo.
- [ ] With the stale-setting dialog open (warning showing, nothing preselected), pick any branch
  from the "Start from" dropdown — confirm the warning row disappears immediately (a code-review
  fix: it used to stay visible, telling the user to "pick one below" even after they'd already
  picked one).
- [ ] Repeat the stale-setting scenario on a repo with **only one branch total** — confirm the
  dialog does NOT show the warning row (no dropdown exists to pick from), and confirm the created
  worktree is actually based on that one real branch, not on `HEAD`/whatever the main repo's
  current checkout happens to be (a code-review fix: it used to leave nothing selected here,
  silently falling back to HEAD instead of the one real branch).
- [ ] **[VISUAL]** Screenshot the Branch From dropdown (both the normal grouped state and the
  stale-value + warning state) and the Create Worktree dialog's warning row, save to
  `.screenshots/branch-from/` in the main checkout.

## `get_git_branches`/`get_branches_detail` remote-classification + phantom-entry fixes (2026-09-10, Rust change — needs `make dev` restart)

Fixed three bugs found while closing test-coverage gaps in git branch enumeration:

1. **`get_git_branches`'s `is_remote`** was a naive `name.starts_with("origin/")` check
   on the short ref name, so a remote added under any name other than `origin` (e.g.
   `upstream`) was never detected as remote, and a local branch literally named
   `origin/foo` was misreported as remote. Backend for `BranchSwitcher.tsx`
   (`Cmd+B`-style branch switcher dialog).
2. **Detached-HEAD garbage entry** — in a detached-HEAD repo state (checked out a
   specific commit/tag, mid-rebase, mid-bisect), `git branch -a`'s synthetic
   `(HEAD detached at abc1234)` pseudo-entry was parsed into a garbage branch named
   `(HEAD`. Also `get_git_branches`.
3. **Remote-HEAD-symref phantom branch, in BOTH `get_git_branches` AND the already-shipped
   `get_branches_detail_impl`** (backend for `GitPanel`'s **Branches** tab, not just the
   switcher) — found by code review, not in the original plan. Every normally **cloned**
   repo has a `refs/remotes/<remote>/HEAD` symref (e.g. `refs/remotes/origin/HEAD`)
   whose *short* name collapses to just the remote's own name (`"origin"`, not
   `"origin/HEAD"`). Both functions' old filtering logic checked the short name for a
   `/HEAD` suffix, which this ref never matches — so a phantom branch literally named
   `"origin"` (or whatever the remote is called) has been leaking into both the branch
   switcher AND the Git Panel's Branches tab for any cloned repo, likely for a long time
   (this bug predates this session entirely for `get_branches_detail_impl`). Now both
   check the *full* refname for a `refs/remotes/.../HEAD` shape instead.

All three are covered by new unit tests (`git.rs`), but the actual UI surfaces (branch
switcher dialog, Git Panel Branches tab) have not been visually checked against a real
cloned repo.

- [ ] Restart `make dev` to pick up the Rust change. Open **any normally-cloned repo**
  (not one created via `git init`) in the branch switcher (`BranchSwitcher.tsx`) —
  confirm there is no phantom branch entry named exactly `origin` (or whatever the
  remote is called) in the list. This is the highest-value check: it's a
  long-standing, previously-undetected bug affecting ordinary repos, not just an edge
  case.
- [ ] Open the same repo's Git Panel → **Branches** tab (`GitPanel/BranchesTab.tsx`,
  backed by `get_branches_detail`) — confirm the same phantom `origin` entry does not
  appear there either.
- [ ] Open a repo with a remote added under a non-`origin` name
  (`git remote add upstream <url> && git fetch upstream`), open the branch switcher,
  and confirm the `upstream/*` branches render in the "remote" section/style, not
  mixed in with locals, and that there's no phantom `upstream` entry either.
- [ ] In that same repo, check out a specific commit (`git checkout <sha>`) to detach
  HEAD, then open the branch switcher again — confirm there is no phantom
  `(HEAD detached at ...`-style entry in the list.

## Inline images (color-tools plan, Phases 1-9 — all phases implemented)

Rust changes here **require a `make dev` restart** to take effect (no hot-reload) — this whole
section needs a human running the rebuilt app, not just re-running tests.

**Post-implementation code review + security review (2026-09-11)**, scoped to the whole
inline-images feature diff (`93d00740..d11e3f4b`), found and fixed 4 real issues before this
feature could be considered done — see commit history for the fix commit:
1. **[Critical, fixed]** `reserve_image_footprint` had no upper bound on requested rows — a
   crafted `r=`/`height=` (e.g. `r=4000000000`) could attempt up to `u32::MAX` `linefeed()`
   calls under the session's `vt_log` lock, hanging that session (and every other consumer of
   its grid) indefinitely. Now clamped to a hard `MAX_FOOTPRINT_ROWS = 10_000` cap. Regression
   test: `terminal_grid::tests::kitty_footprint_row_count_is_clamped_to_a_sane_maximum`.
2. **[High, fixed]** Kitty's `m=1` chunked-transmission accumulation had no cap of its own —
   each individual chunk was already bounded by vte's `MAX_OSC_RAW_STD` (2 MiB), but a client
   could stream an unbounded *number* of chunks, growing the pending buffer past any reasonable
   size before the app-level `MAX_SESSION_IMAGE_BYTES` cap ever ran. Now capped at
   `kitty::MAX_CHUNKED_B64_BYTES` (96 MiB, mirroring iTerm2's own equivalent guard), failing
   closed (aborts the whole transfer). Regression test:
   `terminal_grid::tests::kitty_chunked_transmission_over_cap_aborts_the_whole_transfer`.
3. **[High, fixed]** `o=z` zlib decompression had no bound on inflated output size — a classic
   decompression bomb (a small compressed all-zero payload expanding to gigabytes) would
   allocate the full inflated buffer before the byte cap ever checked it. Now bounded via
   `Read::take` at 64 MiB, rejecting before the oversized buffer is ever fully built.
   Regression test: `terminal_grid::tests::kitty_o_equals_z_decompression_bomb_is_rejected`.
4. **[Should-fix, fixed]** `unicode_placeholder_z` (the U=1 z_index map) had no cap of its own —
   only cleared by explicit `a=d` or session end — so a client registering many placements
   without ever deleting them would grow it for the session's lifetime. Now capped at 100,000
   entries as a refusal (falls back to the default z_index=0 past the cap, a benign
   degradation). **No dedicated regression test was written for this one** — the guard is a
   one-line length check, and a test proving the exact 100,000-entry boundary would need that
   many real escape-sequence parses, which wasn't judged worth the runtime cost for a
   defense-in-depth-only fix; reviewed by inspection instead.

The review's remaining findings: (a) **Fixed 2026-09-11**: Kitty transmit decode
(base64/zlib/file-read/shm-copy) now runs off the `vt_log` lock — see `docs/backend/pty.md`'s
Kitty Graphics Protocol section for the design (`PendingKittyDecodeJob`, `ImageData`'s
`OnceLock`-backed `bytes`, the new `image-decoded` frontend retry signal). Cell reservation
stays synchronous/in-order; a transmission's eventual decode failure no longer prevents its
footprint from being reserved (disclosed behavior change, 3 existing tests updated).
**Live-verified** against a real `make dev` instance: a real Kitty raw-`f=24` image decoded and
displayed correctly end-to-end through the actual `pty.rs::process_chunk` path (not just the
test harness), and the reserve-before-decode-completes behavior confirmed live too. A follow-up
code review + security review of that fix (commit `484d269b`) found and fixed two more real
issues: an unbounded `ImageStore` entry-count growth path (a permanently-failed decode never
evicted its placeholder — now fixed) and a functional regression (the frame ticker's
stalled-sync-update flush and session-teardown flush could each queue a Kitty decode job without
ever resolving it — now fixed, and flagged in `AGENTS.md` as a "check every such queue, not just
`PtyWrite`" lesson). **Still needs a real-tool pass** beyond the unit/integration coverage and
the one hand-crafted live test already done: run `mpv --vo=kitty`/`timg` against a live
`make dev` session and confirm no visible stutter in other panes/HTTP polling during *sustained*
playback — the fix's *correctness* is tested and spot-verified live, but the *latency benefit*
under real sustained load (this was the whole point) has not been benchmarked, only reasoned
about from the code. (b) `ImageLayer.verifyOverlapping()`'s fail-open behavior on a `terminal_image_ref_at`
error could in principle mask a *persistent* backend error (a genuinely-overwritten placement
never getting cleared) rather than just a transient one — low severity, since `a=d`/alt-screen
cleanup independently cover the common cases; left open. (c) **Fixed 2026-09-11**: added
dedicated `mapCommandToHttp` shape tests for all 4 image commands (method/path/transform,
including the null-passthrough cases) plus two `rpc()`-level integration tests — one proving
`terminal_image_bytes` specifically goes through the octet-stream/`ArrayBuffer` path (not just a
sibling command sharing the mechanism), one proving `terminal_image_meta` goes through the JSON
path and passes a real `null` through for an unknown image id — in `transport.test.ts`.

- [ ] After a rebuild, run the real `imgcat`/`imgls`/`divider` scripts, a hand-written OSC 1337
  sequence, or a real Kitty-protocol tool (`chafa -f kitty`, `mpv --vo=kitty`) against a live
  `make dev` session and confirm an image visibly displays. **Both protocols' backends are fully
  implemented and unit/integration-tested end to end** (parsing, decode, footprint sizing incl.
  `auto`/aspect-ratio, cell reservation, `CellExtra` attachment, transmit/place/delete/query,
  chunking, quiet levels), **and a frontend renderer now exists** (Phase 5: `imageLayer.ts`, a
  dedicated canvas between the glyph canvas and the cursor/selection overlay, fed by a new
  `image-placement`/`image-placements-cleared` WS/Tauri event pair and a
  `terminal_image_placements` hydration query for reconnect/new-client attach) — this item now
  needs a real visual confirmation against a live rebuild, not just the unit-level formula/paint
  tests in `imageLayer.test.ts` and the backend's own placement-event tests in `terminal_grid.rs`.
  **Full z-order compositing, raw pixel format decoding, and overwrite detection are now
  implemented too** (previously-documented gaps, since closed): (1) Kitty `z<0` ("paint below
  text") placements now render on a dedicated `belowTextImageCanvasRef` sandwiched between a
  background-only canvas and a transparent glyph-only canvas — `CanvasTerminal.tsx` switches a
  session from the single fused `canvasRef` paint to this 5-layer stack the first time
  `ImageLayer.hasNegativeZ()` goes true, and never switches back, so the common case (no
  negative-z images) keeps the original fast path. Live-verified against a real `make dev`
  session: a `z=-1` placement painted through a blue square's text (visible on top) while an
  otherwise-identical `z=0` placement stayed hidden beneath it, exactly as intended.
  (2) Kitty's raw `f=24`/`f=32` pixel formats (no container — used by mpv/blackcat) now decode via
  a new `terminal_image_meta` command (mime + intrinsic dimensions) plus frontend-side manual
  `ImageData` reconstruction (RGB expanded to RGBA) in `imageLayer.ts`'s `decodeRawPixels`.
  (3) A placement's cells being overwritten by ordinary text (not an explicit `a=d`/alt-screen
  switch) is now detected heuristically: `ImageLayer.verifyOverlapping()` re-checks a placement's
  top-left cell via `terminal_image_ref_at` whenever a dirty-row update touches one of its rows,
  and drops the placement if the ref no longer matches — narrower than a full-rectangle check
  (a partial overwrite that spares the top-left cell isn't caught) but covers the common case.
  All three still need a real-tool visual pass beyond the z-order live-verification already done:
  raw `f=24`/`f=32` against mpv/blackcat, and overwrite-clearing against a real TUI that redraws
  over a placement's origin cell (yazi, image.nvim scrolling). iTerm2's `File=` and Kitty's default
  `f=100` PNG (everything `imgcat`/`imgls`/`divider` and most real Kitty clients actually send)
  both render fine and were live-verified earlier in this same session.
- [ ] Diagnostics-ring elision (color-tools plan, Phase 8, `image_payload_elision.rs`) is now
  implemented: an OSC 1337/Kitty APC payload is replaced with a short placeholder before it reaches
  `pty_raw_rings` or a `.tcap` capture. Covered by unit tests (including one against the same real
  captured `tuic divider` bytes `terminal_grid.rs`'s end-to-end parser test uses), but the PTY
  reader thread's hot loop (AGENTS.md flags this as one of the most regression-prone spots in the
  codebase) deserves its own live-capture pass, not just unit tests: run a real image tool (`tuic
  imgcat`/`chafa -f kitty`) against a `make dev` session, then enable capture mode
  (`POST /diagnostics/capture`) and confirm the resulting `.tcap` shows `<image N bytes elided>`
  in place of the base64 payload while the image still displays correctly on screen (proving the
  real parser saw the unelided bytes). Also worth confirming: a payload that straddles a PTY
  `read()` boundary (a large image) is passed through unelided rather than partially elided —
  by design, not a bug, but worth seeing once with a real large image.
- [ ] `CSI 14 t`/`CSI 16 t`/XTVERSION replies: confirm against a real client. `timg -pk` and
  `broot` are the easiest first targets — `broot`'s env-based detection already matches our
  `TERM_PROGRAM=ghostty` with zero further changes needed.
- [ ] Kitty capability probe (`a=q` + immediate `i=` echo): confirm against yazi or blackcat, both
  of which do a live runtime probe rather than an env-var allowlist. yazi in particular keys its
  `kgp`/`kgp_shm` flags off the echoed `i=` matching what it sent — worth checking specifically.
- [ ] Kitty `t=f`/`t=t`/`t=s` transmission mediums and `o=z` compression are now implemented
  (color-tools plan, Phase 6) and covered by real end-to-end tests in `terminal_grid.rs` (a real
  temp file read+delete, a real POSIX shared-memory segment round-tripped through `shm_open`, a
  real `flate2`-compressed zlib payload) and `terminal_image_transmission.rs` (file/shm reader unit
  tests, including the temp-dir-delete-guard and the size-cap check). Still needs a real-client
  pass: blackcat (`t=s`, `o=z`), ranger's kitty backend (`t=f`), and mpv (raw `f=24`/`t=s`) against
  a live `make dev` session.
  **The Windows `t=s` path (`read_shm_medium` in `terminal_image_transmission.rs`, `#[cfg(windows)]`
  branch) is written against documented Win32 `OpenFileMappingW`/`MapViewOfFile`/`VirtualQuery`
  semantics but has never been compiled, let alone run, on Windows** — this repo is developed on
  macOS, and cross-compiling the whole app for `x86_64-pc-windows-gnu` here hits an unrelated
  missing-mingw-toolchain wall before reaching this code at all. Needs a real Windows build and a
  real `t=s` client to verify.
- [ ] Unicode virtual placeholders (`U=1`, color-tools plan, Phase 7) are now implemented:
  `U+10EEEE` + diacritics + foreground/underline color is recognized during ordinary text output
  and attached as a real image tile, using the authoritative `rowcolumn-diacritics.txt` table
  fetched directly from Kitty's own docs (not guessed) and verified end-to-end against the
  protocol page's own worked examples (2x2 grid, most-significant-byte extension, diacritic-
  omission inheritance) in `terminal_grid.rs`. Still needs a real-tool pass against a live
  `make dev` session: image.nvim and snacks.nvim (neovim plugins) and yazi's modern `Kgp` driver
  are the three target tools that specifically rely on this path rather than ordinary Kitty
  placements. Also worth checking live: a real client's exact diacritic-omission behavior (does
  it always send all 2-3 diacritics per cell, or actually rely on the inheritance optimization?),
  and whether any of the three tools sets colors via a code path (e.g. a terminal-capability
  fallback) this implementation doesn't yet convert into an id — only `Color::Indexed`/`Color::Spec`
  (256-color and true-color SGR) are handled; `Color::Named` yields no image id at all.
- [ ] `tuic imgcat`/`imgls`/`divider` (color-tools plan, Phase 4, `crates/tuic-cli/src/imgcat.rs`):
  unit-tested (sequence construction, tmux passthrough wrapping) and proven end-to-end against our
  own OSC 1337 parser (`real_tuic_divider_cli_output_displays_through_our_own_parser` in
  `terminal_grid.rs`, using bytes actually captured from the compiled binary), but never run against
  a real terminal/human eyeball. After a `make dev` rebuild (these are plain binary changes, not
  requiring the Rust-hot-reload workaround since they're a separate sidecar binary, but still worth
  confirming against a fresh build), verify: `imgcat` against a real PNG/JPEG, `imgls` against a
  directory with several images, `divider` producing a visible full-width bar, and the tmux
  passthrough path (`TERM=tmux-256color tuic imgcat ...` inside a real tmux pane).
- [ ] `imgcat`/`imgls`/`divider` are now wired onto each spawned PTY's `PATH` as one-line shim
  scripts (color-tools plan, Phase 9: `image_cli_shims.rs`, written to `<config
  dir>/image-cli-shims/` and prepended in `inject_unix_terminal_env`), each `exec`-ing the resolved
  `tuic` sidecar with the right subcommand. **Requires a `make dev` restart** to take effect (Rust
  change, no hot-reload) — after restarting, open a NEW terminal tab (existing PTYs were spawned
  before the restart and won't have the updated `PATH`) and confirm bare `imgcat photo.png`,
  `imgls`, and `divider ...` all work with no `tuic` prefix. Unix only, matching
  `inject_unix_terminal_env`'s own scope — Windows PATH shims are out of scope for this pass.
  Silently absent (no error, no shim) if the `tuic` sidecar can't be resolved at all (e.g. a
  from-source checkout with no built sidecar) — confirm that failure mode doesn't also break
  anything else about the PTY spawn.
- [ ] Worktree automation scripts (`TUIC_*` env injection) and PTY `TUIC_*` context
  (`plans/worktree-automation-tuic-env-injection.md`): Setup/Archive script injection and
  timeouts are unit-tested end-to-end against real subprocesses/worktrees, but the PTY-side
  injection (`pty.rs::inject_worktree_env`, called from every `bind_pty_identity` site) needs a
  **`make dev` restart** to take effect (Rust change, no hot-reload) — after restarting, open a
  NEW terminal tab in a worktree (existing PTYs were spawned before the restart) and confirm
  `env | grep '^TUIC_'` shows `TUIC_MAIN_REPO_PATH`, `TUIC_BRANCH`, `TUIC_WORKTREE_NAME`,
  `TUIC_WORKTREES_DIR`, `TUIC_IS_WORKTREE=true`, and (if a base ref/branch is configured)
  `TUIC_BASE_REF`/`TUIC_BASE_BRANCH`. Also verify a Run Script (Settings → repo → Automation
  Scripts) actually sees these vars when typed into the fresh tab. Windows is out of scope for
  this pass (no process-group kill on script timeout, and the PTY/PATH env behavior there is
  unverified) — confirm at least that nothing regresses there (`cmd /C`, `%TUIC_MAIN_REPO_PATH%`).
  A real `npm ci`/`npm install` setup script should also be checked in the *packaged* app (`make
  build`), where the desktop-launch `PATH` is genuinely impoverished — the new
  `PATH=enriched_path()` on setup/archive scripts is meant to fix exactly that.
- [ ] Worktree setup-script ordering fix + new event (`plans/worktree-automation-tuic-env-injection.md`,
  Phase 7): create a worktree in a repo with `copy_ignored_files`/`copy_untracked_files`/`copy_paths`
  AND a Setup Script configured (e.g. `ls -la > setup-saw-these-files.txt`) — confirm the synced
  files are visible to the script (previously could race and not be there yet), and that
  `GET /worktrees/paths?path=<repo>` keeps `warm_artifacts.status: "pending"` until the script has
  finished (order is warm -> sync -> setup). Since the setup script's outcome is no longer returned
  synchronously, confirm the status bar reports a failing script (e.g. `exit 1`) as "Setup script
  failed (exit 1)", and that the script ran exactly once (browser mode used to run it twice). Needs a `make dev` restart (Rust change). Test on all three creation
  paths if practical: desktop "+" button, MCP `repo worktree create` (`agent` tool or a manual
  HTTP `POST /worktrees`), and MCP HTTP session-with-worktree creation.
- [ ] Setup-script/Run-Script ordering fix, frontend half (`createWorktreeCreationCoordinator.ts`'s
  `armSetupScriptWaiter`, follow-up to the item above): configure a Setup Script that takes
  a few real seconds (e.g. `sleep 5 && echo done > setup-ran.txt`) plus a Run Script (e.g.
  `echo run-script-typed`). Create a new worktree from the desktop "+" button and confirm the Run
  Script is NOT typed into the new tab until the Setup Script has actually finished (watch for
  `setup-ran.txt` to appear before the Run Script's command shows up in the terminal) — previously
  the Run Script could type immediately, racing the Setup Script. This is a frontend-only change
  (hot-reloads under `make dev`, no restart needed) but is easiest to verify alongside the Rust-side
  item above during the same restart.
- [ ] `TUIC_*` vars from a subdirectory cwd (`git::find_repo_root`, `script_env.rs`): needs a
  `make dev` restart (Rust change). Use the macOS Finder Service ("New TUICommander Tab Here") on a
  subfolder *inside* a worktree (not the worktree root itself) and confirm the new tab's
  `env | grep '^TUIC_'` still shows `TUIC_WORKTREE_PATH`/`TUIC_MAIN_REPO_PATH`/`TUIC_BRANCH` etc.,
  describing the worktree root — not empty, and not describing the subfolder.
- [ ] Smart Prompts active-repo-vs-worktree-cwd fix (`plans/worktree-automation-tuic-env-injection.md`,
  Phase 8b): with a worktree terminal tab focused (not the main checkout), run a Smart Prompt that
  uses `{branch}`/`{diff}` (e.g. Smart Commit) and confirm the generated message reflects the
  *worktree's* branch/diff, not the main checkout's. Then focus a plain shell tab in an
  *unregistered* directory and confirm the same prompt still falls back to resolving against the
  active repo rather than failing with `unresolved_variables`.
- [ ] `POST /worktrees/run-script` was verified via unit tests calling the real handler function
  directly (including a real axum-router routing test), but never against an actual running
  `make dev` instance over the wire on `:9877`. Low priority (the handler and router-wiring are
  both already exercised) but worth a real `curl -u <token> https://127.0.0.1:9877/worktrees/run-script
  -d '{"script":"echo hi","cwd":"/tmp"}'` if a spare moment allows — confirm `{exit_code, stdout,
  stderr}` and that an unauthenticated LAN-origin request gets 403.
- [ ] GitPanel/ChangesTab.tsx's "Generate commit message" cross-repo fix (executeSmartPrompt's new
  `targetPath` param, `useSmartPrompts.ts`): register two repos, focus a terminal tab in repo A, then
  open repo B's Git panel (Changes tab) and click "Generate commit message" with some staged/unstaged
  changes in repo B. Confirm the generated message reflects repo B's diff (the repo the panel is
  showing), not repo A's (the focused terminal's repo) — previously it would have used repo A's,
  since `executeSmartPrompt` derived context from the active terminal, independent of which repo's
  Git panel triggered it. Covered by unit tests at the `useSmartPrompts.ts` level (the exact mechanism
  `ChangesTab` now relies on), but not by a full component-mount test — this is the live end-to-end
  check for that gap. Frontend-only change, hot-reloads under `make dev`, no restart needed.
- [ ] `SmartButtonStrip`'s same cross-repo fix, across its non-ChangesTab callers (register two repos,
  focus a terminal in repo A, then trigger the strip from repo B's context): the Branches tab's
  "Create PR" button (`BranchesTab.tsx`), and an Issue/PR popover's smart button (`GitHubPanel.tsx`
  right-click on an issue, or `PrSection.tsx`/`PrDetailPopover.tsx` on a PR) for repo B — confirm each
  resolves variables (and, for a headless prompt, runs) against repo B, not repo A. Covered by a new
  component-mount test (`SmartButtonStrip.test.tsx`) asserting the prop is forwarded, but not by a live
  check against these five real call sites. Frontend-only change, hot-reloads under `make dev`.
- [ ] `repo action=worktree_setup_status` MCP tool action + `GET /worktrees/setup-status` HTTP route
  (closes the "MCP client can't observe setup script completion" gap): unit-tested via direct handler
  calls, but not against a live `make dev` instance. Create a worktree with a Setup Script configured
  via an MCP client (or `curl -X POST /worktrees` then poll), then poll
  `curl "https://127.0.0.1:9877/worktrees/setup-status?repoPath=<repo>&branch=<branch>"` (or
  `repo action=worktree_setup_status path=<repo> branch=<branch>` via the `agent`/MCP tool surface) and
  confirm it transitions `running` → `completed` (or `not_configured` with no script), matching what the
  `worktree-setup-script-completed` event reports for the same worktree.
## Command Blocks fullscreen-mode fix (2026-09-15)

Root cause (verified live via a throwaway session and `terminal_grid.rs` unit tests, not just
theorized): Claude Code's default fullscreen renderer draws in the alternate screen buffer, which
never grows real scrollback for a fully-repainted TUI, so `line` values OSC 133/7770 compute
during that time are transient on-screen cursor rows, not valid anchors. Fixed by tagging every
`AgentBlock`/`Osc133Event` with `on_alt_screen`/`onAltScreen`, and filtering row-anchored consumers
(gutter, scrollbar, fold, jump-nav, block-scoped search, "Copy Block Output") through the new
`rowAnchoredBlocks()` helper (`terminals.ts`) — `CommandOverview` deliberately does not filter.
This is a **Rust change** — it will not take effect in any already-running build (including the
orchestrator instance this fix was developed inside) until rebuilt (`make build` or a `make dev`
restart); do not restart the shared orchestrator to test this, per AGENTS.md's Dev Hot Reload
section. All of the below needs a rebuilt build to check.

- [ ] **Core fix** — open a fresh terminal tab, run `claude` (fullscreen renderer, the default),
  send a couple of turns, then drop back to the shell (`/exit` or Ctrl+D). Confirm: no bogus
  gutter marks, scrollbar ticks, or `Cmd+Shift+Up/Down` jump targets appear either during the
  fullscreen session or once back at the shell prompt; `CommandOverview`'s row for that tab still
  shows live prompt text/duration while the agent is fullscreen; running a real shell command
  after exiting produces a normal, correctly-anchored block.
- [ ] **Alt-screen re-entry** — with a plain alt-screen TUI (`vim`, `htop`, `less`) run inside a
  hook-instrumented agent's session (or just inside a plain shell), confirm the same: no bogus
  marks appear from whatever happens to print while that TUI has the alt screen, and shell blocks
  before/after it stay correct.
- [ ] **§5 transcript-dump reconstruction** — inside a fullscreen Claude Code session, press
  `Ctrl+O` (transcript mode) then `[` (write to native scrollback). Confirm: gutter marks,
  scrollbar ticks, fold, and `Cmd+Shift+Up/Down` jump-nav now populate against the dumped text,
  anchored to the real prompt (`❯`) / turn-completion (`✻ ... · done`) lines; press `Esc` to
  return to fullscreen and confirm nothing breaks. This only applies to a hook-instrumented Claude
  Code session specifically (`agent_type == "claude"` — verified via the session's own state, not
  guessed).
- [ ] **§5/#11 transcript-dump de-duplication (2026-09-15 fix)** — repeat the `Ctrl+O`→`[` gesture
  a second time in the same session (`Esc` back to the agent, `Ctrl+O`, `[` again). Confirm the
  re-dumped copy does NOT leave a second, overlapping set of blocks: `Cmd+Shift+Up/Down` jump-nav
  should only ever land on turns from the LATEST dump plus any real shell blocks from before the
  first dump, never a duplicate of an earlier dump's turn. Previously flagged as a known, accepted
  limitation ("note, do not fix"); fixed via `new_dump_generation`/`fromTranscriptDump` pruning
  (`terminals.ts`'s `handleOsc133`, gated on a real alternate-screen visit between dumps).
- [ ] **#6 scrollback-ring eviction fix (2026-09-15)** — needs a session with >10,000 lines of
  real scrollback (`GRID_SCROLLBACK`, `state.rs`) to actually saturate the ring, so a short manual
  session won't exercise it; the unit-level regression coverage
  (`osc133_line_stays_eviction_stable_and_never_aliases_past_scrollback_saturation`,
  `terminal_grid.rs`) is the practical verification for this one. If a long-running session with
  heavy output is available, confirm gutter marks/scrollbar ticks/jump-nav for an OLD block don't
  suddenly jump to a wrong row, and a NEW block recorded after heavy scrollback growth still
  anchors correctly, once real eviction has occurred.
- [ ] **§5 glyph-collision residual risk (code-review finding, assessed not fixed)** — the
  transcript-dump detector opens a phantom block if an ordinary typed command at a bare shell
  prompt happens to start with `❯ ` (some zsh themes, e.g. Pure/Spaceship, use this glyph) while
  `agent_type` is still stuck at `"claude"` from a just-exited session. For the default zsh
  auto-injected shell integration this window is roughly one prompt-redraw beat (the shell's own
  OSC133 `A` marker clears `agent_type` almost immediately via `clear_agent_type_on_confirmed_shell`)
  — with a `❯`-glyph zsh theme, exit Claude Code and immediately type an ordinary command at the
  next prompt; confirm no phantom `AgentBlock`/`CommandOverview` entry appears for it. If one
  does appear reproducibly (not just as a rare race), this needs a real fix, not just monitoring.

- [ ] **Additional Readable Directories (HTTP read allow-list) — Rust change, needs a `make dev`
  restart to take effect.** Unit- and integration-tested at the handler/router level (`fs_routes.rs`,
  `mcp_http/mod.rs`), but not against a real running instance:
  - Start a debug instance with Remote Access enabled. `curl
    'http://127.0.0.1:9877/fs/read-external?path='"$HOME"'/.claude/plans/<some-file>.md'` with no repo
    registered for that path → expect `200` (the default `~/.claude/plans` entry).
  - `curl -X POST http://127.0.0.1:9877/fs/write-external -d '{"path":"'"$HOME"'/.claude/plans/x.md","content":"x"}'`
    into the same directory → expect `403` (proves the write path was NOT widened).
  - In Settings → Remote Access → File Access, remove the default entry, re-request the same file via
    curl → confirm `403` with the friendly-sounding message body.
  - In browser mode, click a `~/.claude/plans/...` link rendered from an agent's output (a plan-file
    reference) and confirm it opens with no `error`-level log line in `GET /logs`.
  - In the Settings UI, try adding a relative path (e.g. `notes`) and confirm the new inline validation
    error appears and the entry is NOT added — then add a real `~/...` or absolute path and confirm it
    IS added and persists across a settings reload.
- [ ] StreamDock M18 macropad integration (`src-tauri/crates/tuic-streamdock/`,
  `src-tauri/src/streamdock/`, Settings > StreamDock tab): device connection, live session-tile
  rendering, gesture dispatch, config lifecycle, and the ambient LED ring, code-complete through
  Phase 7 and gated by `check-gate.sh` (all Rust/frontend tests pass), but the in-process wiring
  (`AppStateSource`/`AppStateSink`, `StreamDockManager::apply_config` reconcile, the new
  `SessionFocusRequested`/`UiActionRequested` `AppEvent`s, `Coordinator::ambient_led_update`) has
  never run against a live `make dev` process — **requires a restart to load** (Rust change, no
  hot-reload). After restarting with the real VSD M18 attached and `Settings > StreamDock`
  enabled: confirm the device connects (status strip shows `running` with the right serial),
  starting/finishing a real session moves the corresponding key within ~300ms, closing a session
  blanks its slot without reindexing neighbors, a tap on a session tile focuses that tab (and
  answers a pending choice prompt if one is showing), a hold sends interrupt (`\x03`), the
  bottom-row verb keys (`Approve`/`Reject`/`Interrupt`/`JumpWaiting`/`Overflow`) and the 3 plain
  buttons work, and both brightness sliders move the backlight live with no restart. Also verify
  hot-plug: unplug and replug the device and confirm it reattaches within ~2s. **New in Phase 7:**
  confirm the 24-LED ring lights up at all on this unit (Boss's VSD firmware string
  `"V3.VSDM18_HBOE.02.01"` should negotiate `FeatureSet::rgb = true` per `device::model`'s
  firmware-prefix match — this has only been unit-tested against that exact string, never
  observed against the real ring) and that it actually changes color: green with all sessions
  quiet, peach the moment any session goes into "awaiting input," red when a session is
  rate-limited/errored, and that it settles back to green once the triggering session clears
  (not stuck on the last color). If no hardware is available, the focus/action event path itself
  can be proven without a device via `curl -X POST :9877/sessions/{id}/focus` and
  `curl -X POST :9877/ui/action` against the worktree build.
  **Visual verification of `Settings > StreamDock` itself has NOT been done** — the tab is gated
  behind `isTauri()` (same as Dictation), so it's invisible in plain browser-mode testing and can
  only be screenshotted from the real Tauri desktop app, which needs the same restart called out
  above. After restarting, open Settings > StreamDock and check: the status strip's colored dot
  and label render correctly for each state (disabled/waiting/connected/error), the device
  dropdown and Rescan button look right with 0 vs 1+ devices listed, both brightness sliders
  render and drag smoothly, and the pinned-sessions list renders correctly with 0 vs several live
  sessions. Follow `docs/frontend/STYLE_GUIDE.md` and take a screenshot per AGENTS.md's "Visual"
  rule — none exist yet for this tab.

- [ ] **Settings search extras port (BM25 ranking, hints, palette actions, flash) — frontend-only,
  Vite HMR picks it up** — visual checks that jsdom can't prove: (1) open Settings, type a query —
  results are ranked sensibly (the setting named by the query first), each row shows its hint text
  under the `Tab › Section` trail without overflowing the row (hints are ellipsized); (2) pick a
  result — the target control scrolls into view and briefly flashes with the accent-tinted
  highlight (`Settings.module.css` `settingsSearchHighlight`), and re-picking the same result
  restarts the flash; (3) `Cmd+P` — "Settings" category actions exist (e.g. "Shell (Terminal
  settings)"); selecting one opens Settings on the right tab, scrolled to and flashing that
  control, including while Settings is already open on another tab; (4) with AI Chat disabled the
  palette offers no `AI Chat settings` entries, and in browser mode (`:9876`) no StreamDock ones
  and no desktop-only controls (e.g. Global Hotkey), while other setting actions still work there;
  (5) with Expert mode off, a palette action for an expert control (e.g. "Shell (Terminal
  settings)") reveals that control and lands on it.

- [ ] **Browser-client terminal copy over a real network (not just localhost)** —
  `CanvasTerminal.tsx`'s `copySelection()` and `useTerminalContextMenus.ts`'s "Copy Block Output"
  both used to await an HTTP round-trip (`terminal_get_selection_text` / `getBufferLines`) before
  writing to the clipboard; over real network latency (Tailscale/remote access, not localhost,
  where the round-trip is near-instant) this can outlast the browser's user-activation window and
  silently no-op both `navigator.clipboard.writeText` and the `execCommand('copy')` fallback —
  reproduced locally via a real headed-Chromium select+Cmd+C against `:9876` landing on the wrong
  pane/timing. Fixed via a new `writeClipboardAsync()` (`utils/clipboard.ts`) that calls
  `navigator.clipboard.write()` *synchronously* (satisfying the activation requirement immediately)
  with a `ClipboardItem` whose data is the still-pending round-trip promise — a spec-sanctioned
  pattern supported by Chrome/Firefox/Safari — so the Rust-side text (wrap-unwrapped, Claude
  quote-gutters stripped) is preserved with no quality tradeoff on modern browsers. Falls back to
  the old (activation-risking) synchronous path only when `ClipboardItem`/`navigator.clipboard.write`
  aren't available. **Verify on a real remote connection** (Tailscale, not `127.0.0.1`) with actual
  round-trip latency: drag-select terminal text (including a multi-line Claude quote with the `▎`
  gutter, and a soft-wrapped long line) and use "Copy Block Output" from the context menu; press
  Cmd/Ctrl+C or click the menu item, and confirm in both cases (a) the OS clipboard actually
  updates and (b) the pasted text is fully clean (gutters stripped, wraps joined) — not degraded
  quality, matching desktop/Tauri behavior exactly.
  **Separately, found during this investigation and since fixed (2026-09-18):** the "Copy on
  Select" setting (Settings > Terminal, `copyOnSelect` in `settingsStore`) was
  fully unwired to the actual trigger — `CanvasTerminal.tsx`'s `onMouseUp` called `copySelection()`
  unconditionally on any non-empty selection, with no `settingsStore.state.copyOnSelect` check
  anywhere in the file, so disabling the toggle had no effect. Fixed by gating only the
  auto-copy-on-drag branch on the setting — the selection itself is still always made, and Cmd/Ctrl+C
  still always copies manually regardless of the setting, matching the documented intent
  (`docs/user-guide/terminals.md`'s "Copy on Select" section). Covered by a new test in
  `canvasTerminalClipboardFailure.test.ts`. **Manual check still worth doing:** in Settings, turn
  "Copy on Select" off, drag-select terminal text, confirm nothing is copied and no "Copied to
  clipboard" status appears, then press Cmd/Ctrl+C and confirm that DOES copy — in both the
  desktop app and the browser/HTTP client.

## Session State Explain (2026-09-21, **Rust change — needs `make dev` restart**)
- New read-only troubleshooting dump for "why is this session's status badge what it is" —
  ranked evidence per axis, `decide_now`, the `agent_state` ladder rung, screen/silence
  bookkeeping, the last notification classification, and an always-on decision trail (including
  rejected evidence attempts and what outranked them). Full design in `docs/backend/pty.md`'s
  "Session State Explain" section.
- Surfaces: desktop `explain_session_state` command, `GET /sessions/{id}/explain-state`, MCP
  `debug action=explain_state`; frontend modal reachable from the Activity Dashboard row's new
  icon button and the terminal tab context menu's "Explain State…" item.
- **Verify against a real running session** (needs a rebuild first — this is Rust-side): open
  the Activity Dashboard, click the small "?" button on a working session's row, confirm the
  modal loads and every section renders (Visible/Evidence & decision/Agent & screen/Silence
  timer/Decision trail); try it again on an idle session and on one currently awaiting input.
  Right-click a terminal tab and confirm "Explain State…" opens the same modal for that
  session, and is disabled/absent for a tab with no PTY session or an exited one.
- **Verify Copy as JSON in the real desktop app, not just browser mode** — this is exactly the
  surface where a prior WKWebView clipboard bug shipped once (issue #101,
  `navigator.clipboard.writeText` silently failing inside a modal); `writeClipboard` is used
  here specifically to avoid repeating it, but only a real Tauri build proves it.
- **Try to catch a genuine mis-detection with it**: next time a session's badge looks wrong,
  pull its explain-state dump before doing anything else and check whether `trail` shows a
  rejected attempt, or `decide_now` disagrees with the displayed `shell_state` — either would
  confirm the feature is actually useful for real troubleshooting, not just self-consistent in
  tests.

## Custom PTY env vars + tmux pane shell-readiness gate (2026-09-24, **Rust change — needs `make dev` restart**)
- Fixes the p10k-wizard-hijack pane-spawn race (`plans/p10k-wizard-hijack-agent-pane-spawn-race.md`):
  `tmux_routes.rs::materialize` now blocks until the freshly spawned shell reaches `SHELL_IDLE`
  (5s bound, fail-open) before returning — a `respawn-pane`-delivered launch command can no
  longer arrive while the shell is still sourcing `.zshrc`. See `docs/backend/pty.md`'s "Pane
  Shell-Readiness Gate" section.
- New global setting `AppConfig::custom_pty_env` — user-authored `KEY=value` pairs injected into
  every spawned PTY (`pty::apply_custom_pty_env`, applied last, overrides everything except the
  internal `TUIC_PTY_TTY`). Settings UI: Settings → Terminal → "Custom Environment Variables"
  (`TerminalTab.tsx`, add/remove `KEY = value` rows, key-format + duplicate-key validation).
  See `docs/backend/pty.md`'s "Custom PTY environment variables" section for the full
  precedence order.
- **Verify after rebuild (Rust) + reload (frontend):** open Settings → Terminal → "Custom
  Environment Variables", add `TUIC_MANUAL_TEST` = `hello`, open a new terminal tab, run
  `echo $TUIC_MANUAL_TEST` — expect `hello`. Then add
  `POWERLEVEL9K_DISABLE_CONFIGURATION_WIZARD` = `true` on a machine with Powerlevel10k's
  instant-prompt in verbose mode and confirm a burst of several simultaneous agent-teams pane
  spawns no longer triggers the config wizard. Also confirm the setting is findable via the
  Settings search box (typing "Environment Variables" should scroll to it).

## Ghost pane-tab / Global Workspace stale-layout fix (2026-09-25, **frontend-only — needs app restart or reload**)
- Fixes a live-reported bug: a new tab kept landing inside a 6-way split left over from an
  Agent Teams swarm, and "Reset Panel Sizes" only worked until the next new terminal. Root
  cause + fix details: `src/AGENTS.md`'s "`paneLayoutStore` Ghost Tabs Can Permanently Wedge A
  Split..." section. Unit-tested (`useTerminalLifecycle.test.ts`, `useSplitPanes.test.ts`), but
  the actual live scenario (a real tmux-shim-materialized pane whose PTY died outside the
  normal close path) can't be reproduced from a unit test.
- **Verify after restart/reload:** with the Global Workspace active and holding a split (any
  auto-consolidated repo with 2+ terminals promoted), run "Reset Panel Sizes" from the Command
  Palette, then open a brand-new terminal in that same repo — it should land as its own flat
  tab, NOT get pulled back into a resurrected split. Separately, manually verify a pane whose
  only tab is a session TUICommander no longer tracks (hardest to force manually — closest
  approximation: kill a teammate pane's PTY process directly from a shell, e.g. `kill -9`, in a
  way that bypasses the app's own close path) can still be closed via the pane's close-pane
  action.

## Global Workspace ambient-scope fix (2026-09-29, frontend-only)
- Fixes the ambient-`scope` confusion documented in `src/AGENTS.md`'s "`globalWorkspaceStore`'s
  Ambient `scope` Pointer..." section: sidebar badge inflated by dead ids, globe icon
  disagreeing with the Activity Dashboard, removing a terminal via the Activity Dashboard not
  sticking, and the sidebar pill's click having no visible effect. Extensively unit-tested
  (new `GlobalWorkspaceEntry.test.tsx`, `PaneTree.test.tsx`, plus additions across
  `globalWorkspaceScopes.test.ts`, `useWorktreeConsolidation.test.ts`,
  `createBranchSelectionCoordinator.test.ts`, `TabBar.test.tsx`, `ActivityDashboard.test.tsx`,
  `useAppShortcutHandlers.test.ts`) — the items below are the parts a unit test can't fully
  prove (real click sequencing, real visual rendering).
- **Sidebar pill is now one-way** — click "Global Workspace" with at least one terminal
  manually promoted; confirm it shows the merged view. Click it again while already showing —
  confirm nothing changes (no flicker, no layout reset). Click any ordinary terminal or branch
  in the sidebar tree — confirm it exits Global Workspace and shows that terminal's own repo
  (its normal view, or its own auto-consolidated worktree view if that repo has the setting on).
- **Badge count** — promote 2-3 terminals across different repos via the Activity Dashboard's
  globe button; confirm the sidebar badge matches exactly. Repeat with a repo that has
  "consolidate worktrees" on and has its own worktree terminals — confirm the badge only counts
  your manual promotions, not that repo's auto-consolidated ones.
- **Globe icon parity** — promote a terminal, confirm its tab (both in the normal TabBar strip
  and, if split, in a PaneTree pane) shows the globe icon, and the Activity Dashboard's promote
  button shows the same "promoted" state. Click the tab's own globe icon to unpromote directly
  from the tab; confirm the Activity Dashboard updates to match.
- **Repo-name hover overlay** — while Global Workspace is showing (multiple repos' terminals
  merged), hover a tab and confirm the repo-name overlay appears. Switch to a repo that has
  "consolidate worktrees" on (its own automatic merged view, not Global Workspace) and hover a
  tab there — confirm the overlay does NOT appear, since you're already in that one repo.
- **Tab close still does a real close** — while Global Workspace is showing, click a tab's `×`.
  Confirm the terminal is actually gone (not just removed from the Global Workspace view) —
  check it no longer appears anywhere, including that terminal's own repo in the sidebar.

## Resume banner on Claude Code exit, with session title (**Rust change — needs `make dev` restart**)
- `tuic-hook` now scrapes `session_title` on SessionStart/UserPromptSubmit/SessionEnd and the
  raw `reason` string on SessionEnd (new `cctitle`/`ccend` OSC 7770 verbs); `pty.rs` writes the
  live session id/title/cwd into `SessionState` and takes a `resumable_session` snapshot the
  moment the shell reclaims the foreground from Claude (`snapshot_resumable_session_on_agent_exit`). See
  `src-tauri/AGENTS.md`'s "Agent Session Management" section, "Exit-time resume banner".
- **Verify after rebuild + restart:** open a throwaway terminal, run real `claude`, use
  `/rename foo`, send one prompt, then `/exit`. Expect a banner reading something like
  `Resume "foo" — click to resume` to appear in that pane (it appears once the next
  foreground observation sees the shell back — the next PTY chunk, e.g. the prompt redraw). Confirm typing at the shell prompt passes through normally with the banner
  still visible (it's click-only, unlike the restore-time banner's Space/Enter behavior).
  Click the banner and confirm it runs `claude --resume <id>` and the resumed session shows the
  same conversation. Then click the × on a fresh occurrence and confirm it dismisses without
  resuming. Separately, restore a branch/workspace with a saved agent tab and confirm that
  banner still shows its title too.
- **Also verify issue #119's hardening:** open 2+ Claude tabs in the SAME repo folder (matching
  the original bug report's shape), let each reach an idle prompt, then check each tab resumes
  its OWN conversation (not all landing in the same one). `SessionState.agent_session_id` is now
  visible on `GET /sessions` (or `debug action=explain_state`) per-session — confirm each tab
  reports a different id there, and that `terminalsStore.agentSessionIdIsAuthoritative` (via
  browser devtools / a temporary log) is `true` for each once its own hook has fired.

## Session Diff Review / Branch Diff Scroll overhaul (2026-09-29, **Rust change — needs `make dev` restart**)
Full rewrite of both diff views: fixed the reported bugs (stale virtualized rows after a mode/
session switch, dead collapse carets, sticky headers, wrong chronological line numbers, the
global "scroll" mode hijacking every open per-file tab), then added turns, agent display names,
jump-to-tab, live updates, navigation, diff-comparison settings, and an unseen tab icon. Plan:
`plans/enchanted-puzzling-teacup.md`. Everything below is code-inspected and/or unit-tested
(on the pre-rebase `wip` branch: 547 frontend files / 8600+ tests, 7100+ Rust tests, all green;
replayed onto main 2026-10, see the last item) — this list is only the parts
that need a real running instance because they depend on live PTY/transcript/visual state a unit
test can't produce.

- [ ] **Session/mode switching no longer bleeds content.** Open Session Diff Review on a repo
  with 2+ Claude sessions. Switch sessions rapidly, and switch By File ↔ Chronological rapidly —
  confirm no stale file headers or step cards ever linger from the previous view.
  _(The collapse-caret half of this item is its own item below.)_
- [ ] **Collapse caret toggles on a click anywhere in the file header row, not just the chevron**
  _(verified on the pre-rebase `wip` branch only, at `651f46b558`: live browser testing against
  Branch Diff Scroll's real DOM — happy-dom has no real flex layout, so no unit test could catch
  this — found the header was STILL partially broken: `.filePath`'s `flex: 1` stretched its
  invisible DOM box across nearly the whole row, so a click anywhere except the chevron/stats
  badge hit the path span's own "open file" handler instead of toggling collapse. Fixed in the
  shared `diffFileList.module.css` `.filePath` class, which `SessionFileHeader.tsx` also imports
  (replayed byte-identical as `6eca144ec`). Re-verify on the rebased tree — the diff views were
  merged with main's own collapse code during the replay — in Branch Diff Scroll AND Session Diff
  Review, which was never live-clicked for lack of a real transcript)_.
- [ ] **Sticky file headers sit at the true top** of the list (no longer offset by a summary-bar
  height meant for a different consumer) _(verified on the pre-rebase `wip` branch only: Branch
  Diff Scroll's "All Changes" summary header confirmed flush at the top with a real scroll,
  screenshot in `.screenshots/session-diff-overhaul/`. Session Diff Review's own list uses the
  same shared CSS default — `--diff-header-height: 0px` when no `headerHeight` prop is passed —
  but wasn't independently screenshotted. Re-check both on the rebased tree)_.
- [ ] **Diff Scroll ("All files") no longer hijacks other tabs.** Open a per-file diff tab, then
  open Diff Scroll from a different tab/shortcut — the per-file tab must stay showing its own
  single-file diff, not flip into scroll mode too.
- [ ] **Live updates (needs a real live Claude session):** open Session Diff Review on the
  session you're driving from a terminal tab in the SAME window. Have that agent make an edit.
  - Chronological mode: with "Follow" checked, the view should auto-scroll to the new step. With
    Follow unchecked, an "N new changes" pill should appear instead, and clicking it should jump
    to the first unseen step.
  - By File mode: a new file should append at the bottom with a "New content below" pill if
    you're scrolled away from the bottom. An edit to a file currently on screen should apply in
    place with a brief flash/fade. An edit to a file scrolled off screen should hold behind a
    "Refresh (N)" pill until clicked.
  - Confirm the session dropdown itself refreshes (a brand-new session appearing in the list)
    without you clicking the manual refresh button.
- [ ] **Turn picker.** In chronological mode, open the turn dropdown — each entry should show a
  time, a +/- size, and the file(s) touched; hovering should show a prompt-preview tooltip;
  selecting one should jump the list to that turn's first change.
- [ ] **`^`/`v` same-file jump buttons** on a step's header in chronological mode — jump to the
  previous/next change touching that same file, disabled at the first/last occurrence.
- [ ] **`<`/`>` navigation** in both Session Diff modes and in Branch Diff Scroll — step
  change-to-change (chronological) or file-to-file (By File / Branch Diff Scroll), disabled at
  each end. _(Branch Diff Scroll's file-to-file stepping was verified live on the pre-rebase
  `wip` branch only — disabled correctly at the first file, `>` scrolled to the next file,
  screenshots in `.screenshots/session-diff-overhaul/`; re-check on the rebased tree. Session
  Diff's own chronological/By File `<`/`>` has never been live-verified.)_
- [ ] **Agent display names + jump-to-tab.** In a session with at least one subagent
  (`Task`/`Agent` tool call), confirm the step badge shows a real name (from its `meta.json`),
  not the raw hex id, and clicking it jumps to the PARENT session's terminal tab (subagents have
  no PTY of their own — verify it does NOT try to jump to a nonexistent subagent tab). Verify a
  session with no live matching terminal renders the name as plain, non-clickable text.
- [ ] **Auto-open.** In Settings → General → Diffs (an expert setting — turn Expert on), set
  "Auto-open Session Diff Review" to `Ask`, make
  an edit in a tracked repo from a live agent session with no Session Diff tab open for it yet —
  expect a notification-bell notice (not a toast) with an "Open Session Diff" action. Set it to `Auto` — expect the tab to open in
  the background (not stealing focus). Set it to `off` — expect nothing. In all three cases,
  confirm it does NOT refire while a tab for that exact session is already open.
- [ ] **Unseen tab icon.** With a Session Diff tab open but NOT the active tab, trigger a live
  edit in that session — the tab's icon should change to an unseen indicator, and clear the
  moment you click back into the tab.
- [ ] **Diff-comparison settings** (Settings → General → Diffs: ignore leading/trailing whitespace, ignore
  whitespace amount, ignore case, soft wrap, truncate long changes; all but soft-wrap are expert): toggle each against a real diff
  with a whitespace-only or case-only change and confirm it disappears/reappears as expected in
  BOTH Session Diff and Branch Diff Scroll. Confirm the shared options popover in each view's own
  toolbar reads/writes the same settings (flip one in Session Diff's toolbar, reopen Branch Diff
  Scroll, confirm it's already applied there too). _(Partly verified on the pre-rebase `wip`
  branch only: the `DiffOptionsMenu` popover opened from the per-file `DiffTab` toolbar with all
  5 controls, and "Ignore trailing whitespace" turned a whitespace-only diff into "No changes".
  Still needs: the other 3 whitespace/case options individually, the shared-settings check
  across Session Diff's and Branch Diff Scroll's own toolbar instances, soft-wrap and
  truncate-lines — and the verified part again on the rebased tree.)_
- [ ] **Truncation.** Set "Truncate long changes" to something small (e.g. 20) and open a
  large real diff — confirm it's cut at a hunk boundary with a "Show all N more lines" button
  that reveals the rest on click.
- [ ] **Background-subagent turn attribution** (narrow, hard to force live — code-inspected +
  unit-tested via a synthetic transcript fixture, not yet observed against a real background
  Task call): if you can reproduce a genuinely `background`-shaped subagent call whose result
  reports back several turns later, confirm its edits show up under the turn it was actually
  SPAWNED in, not the later turn that merely observed its completion.
- [ ] **Canvas-only visual checks** (not observable over HTTP, per this repo's own testing
  convention): the flash/fade animation's actual look, and the turn-picker tooltip's
  rendering/positioning. _(The sticky-header CSS check that used to be listed here is its own
  item, above.)_
- [ ] **Watcher bounds** (replay addition): with Session Diff tabs open on the same session in
  two windows, closing one keeps the other live-updating; a watch the backend refuses (64
  distinct sessions / 32 subscribers per session) refreshes only via the slow fallback (10 s after a working-tree change),
  and closing that tab never stops the other tab's updates.
- [ ] Run the single aggregate `./scripts/check-gate.sh` once on the rebased tree. On the
  pre-rebase `wip` branch the gate first got blocked by the permission classifier (at
  `c29a9018b`) and then passed in full at that branch's tip of the time, `5c40f3de5` (exit 0,
  7108 Rust tests passed / 16 skipped, vitest, plugin tests, both audits). That result does NOT
  carry over: after the replay onto main it has not been run against this series at all —
  per-pick replay checks were targeted only.

## Tab-name / accent-color rename-echo ping-pong (2026-09-30, frontend-only)

`session-renamed` and `session-accent-color-changed` are now applied through
`terminalsStore.applyBackendRename`/`applyBackendAccentColor` (`{ echo: false }`) instead of a
plain `update()`, so a backend-pushed name or color is never echoed back to
`set_session_name`/`set_session_accent_color`. On this tree the accent color could loop
(`set_session_accent_color` re-emits every change); `set_session_name` never emits, so a name
echo was only a redundant round trip. Unit-tested (`terminals.renameEchoGuard.test.ts`,
`useAppInit.test.ts` listener tests, `osc_title::a_real_title_immediately_followed_by_a_reset_emits_exactly_two_renames_then_settles`).

- [ ] **The frontend's echo-suppression, live against a real running debug instance.** Start a
  debug instance with an explicit `TUIC_APP_INSTANCE=<id>` (on this tree `make dev` does not
  derive one per checkout), enable its HTTP server, create a throwaway session with its tab open
  in the WebView, then drive two tmux-shim `set-option ... pane-border-style` calls with
  different colors in quick succession while sampling `/events`: expect exactly two
  `session-accent-color-changed` events and no further ones (an echoing frontend would make
  `set_session_accent_color` re-emit). (A name echo is not observable this way:
  `set_session_name` never emits on this tree.)
  _(Verified on the pre-rebase `wip` branch only, against its own rename-emitting backend:
  exactly 2 `session-renamed` events for 2 direct calls, zero phantom echoes. Re-verify on the
  rebased tree.)_
- [ ] **Reproducing the trigger from a REAL OSC-emitting process** (a live shell/agent whose
  title changes, not a direct `PUT /name` call) still needs a human check: open a real terminal
  tab, run a real agent (or `printf '\033]0;test title\007'` at a plain shell prompt) so its
  title changes and then reverts, and confirm the tab title/accent border settle immediately
  with no flicker.
