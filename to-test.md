## Detached command peer isolation (2100-c249) — CLI update and backend restart required

- [ ] After installing the rebuilt `tuic` CLI, verify a detached reviewer cannot set the foreground composer to awaiting through its progress report, and its BG DONE still arrives. Install both the rebuilt CLI and backend, then manually restart the backend: the ancestry boundary cannot load through frontend hot reload. No daily app restart was performed by this peer.
## Sidebar session identifiers (2112-8a79)

- [x] Isolated `b6-session-menu` instance: a running Claude PTY menu exposes session ID, actual child PID, and session-list alias; browser reload preserves PID and alias. Evidence: `~/Gits/.tmp/results/b6-2112-visual-owned/` (DOM and screenshot, no session fixtures). Unknown-PID and absent-alias visibility is covered by the targeted sidebar component tests.

## Queued BG DONE diagnostics (2100-c249) — Rust restart required

- [ ] After Boss manually rebuilds and restarts the backend, leave a BG DONE wake queued behind a retained draft for more than 60 seconds. Check `/logs?source=bg-wake` on that instance for one warning with `partial_composer=true` and the bounded process tree. Clear the draft and check that the existing delivery path submits the wake. No daily app restart was performed by this peer.

## App Nap opt-out (1638-c94c) — Rust restart required

- [ ] After Boss manually restarts a rebuilt backend, check the log line `App Nap disabled for the process lifetime` (source `app-nap`). Cover the TUIC window with a full-screen app while no agent is busy, wait two minutes, and check that `/logs` contains no `Sleep-wake detected` or `Frontend unresponsive` line for that period and that remote clients stay live. This Rust change does not hot-reload into the daily app.
## Wrapped ego approval controls (2102-98ab) — Rust restart required

- [x] An isolated `at-2102` worktree build exported a wrapped printf approval with all five native keys. The mobile header question card sent `y` and the command exited 0 with `PARITY_P_OK`. Evidence: `~/Gits/.tmp/results/fix-2102-live/`. The daily backend still requires Boss's manual `make dev` restart or `make build` installation to load this Rust fix; no daily app restart was performed.
## Grok npm launcher Chat binding (2104-0523) — Rust restart required

- [ ] After Boss manually rebuilds and restarts the daily backend, verify a Grok terminal launched through npm opens its own Chat transcript. The isolated instance evidence does not load this Rust change into Boss's running app; no daily restart is performed by the peer.

## Bounded repository file reads (2098-98b2) — Rust restart required

- [ ] After a manual backend rebuild/restart, verify an external edit under an ignored directory containing a tracked file updates that checkout, unchanged siblings remain correct, and watcher restart returns fresh status. macOS/Windows monitoring snapshots require complete watches; Linux stays fresh. No daily app restart was performed.

- [ ] After a manual backend rebuild/restart, verify one file staged then edited and a directory with two untracked files show unchanged diff totals and correct dirty counts; monitoring now shares one status traversal. No daily app restart was performed.

- [ ] After Boss manually rebuilds and restarts the backend, commit twice in one linked worktree, fetch a remote, and pack refs. Confirm the changed row and integration badges update while sibling badges stay correct. This peer does not restart the daily app.

- [ ] After loading the rebuilt backend, verify Codex session and same-path date-directory replacement in an isolated CODEX_HOME and unchanged discovery without historical rollout scans; watched-index and cwd/age/claimed-ID regressions run on rb, but macOS watcher behavior still needs the isolated instance.

- [ ] After Boss manually restarts a rebuilt backend, commit in one disposable linked worktree and check current dirty/integration badges and unchanged sibling rows. This Rust change does not hot-reload into the daily app. The peer must not restart that app; isolated counter evidence is recorded in the story.
## Unchanged content-index events (2099-8c87) — Rust restart required

- [ ] After Boss manually restarts a rebuilt backend, confirm repeated unchanged repository events do not schedule content-index validation within 60 seconds of a completed validation. Verify a later content edit remains searchable after the cooldown. This Rust change does not hot-reload; this peer does not restart the daily app.
## Missing-checkout named submodule preservation (2096-1d25) — Rust restart required

- [ ] After Boss manually restarts a rebuilt backend, verify forced cleanup of a disposable missing worktree with a named submodule preserves its local-only commit in the initialized main module before pruning registration. No running desktop backend was restarted by this peer. Automated Git boundary evidence is recorded in the gate report.
## Managed Claude default-No trust dialog (2097-5739) — Rust restart required

- [ ] After Boss manually restarts a rebuilt `make dev` or installs `make build`, spawn a managed Claude child in a disposable untrusted folder. Check that the startup dialog selects Yes with Down then Enter once; an unmanaged terminal and trust opt-out must remain unanswered. The recorded 1987 capture is covered by automated replay; this Rust change does not hot-reload into the daily app. No daily app restart was performed.
## Covered agent-test presentation (2095-0106) — isolated native check verified

- [x] The recovered native observer passed a 300-second covered-window run: document/native visibility stayed true, rAF advanced with a 319 ms maximum gap, no focus or activation, and stable compact geometry. Evidence: `reports/tuicommander/2095-covered-observation/`. The dedicated agent-test binary must be rebuilt to load this Rust policy; the daily app is unchanged and needs no restart for this fix. This peer did not restart or replace Boss's running app.

## First narrow Codex /plan question (2094-da19) — Rust restart required

- [ ] After Boss manually restarts a rebuilt backend, open the first Codex /plan question in a narrow terminal. Check Input in the mobile list and named answer controls in CLI and Chat. Answer once and check native completion. This Rust parser change does not hot-reload into the daily app; isolated worktree verification is recorded in the story.
## Bounded input frame priority (2040-16b2) — Rust restart required

- [ ] After Boss manually restarts a rebuilt `make dev` or installs `make build`, type while a disposable terminal prints paced output. Check desktop and browser/PWA responsiveness, sustained output, and DEC-2026 complete redraws. This Rust change does not hot-reload into the running daily app. Isolated native trace timing is an untrusted-input-to-`fillText` proxy, not trusted keyboard or compositor proof; the original native REALITY criterion remains separate.

## Agent-test non-focusing visibility (2090-cf8a) — isolated native check verified

- [x] `scripts/agent-test-visible.py --instance at-2090 --port 19883` observes a visible document, positive terminal canvas and real `fillText` paint without native/document focus. The window is 640x420 logical pixels, 17 pixels from the primary work-area right edge and 16 from its bottom edge. Unrelated frontmost PID 761 stays unchanged before launch, at readiness and after paint (owned PID 42795). Removing only `native.orderFront(None)` produces hidden document/native window, no canvas and no text paint; restoring it passes. Both runs delete their sessions, stop through `make agent-test-stop`, and report no remaining worktree executable. Evidence: `~/Gits/.tmp/results/fix-2090-round2/{red2,green}/` (JSON, logs and GREEN canvas-only PNGs). This opt-in native observer requires desktop authorization; source/config contracts do not prove visibility. The change loads only in a newly built agent-test binary and requires no daily-app restart.

## Question state after agent exit (2091-ba03) — Rust restart required

- [ ] After a manual backend restart, exit a Claude question to the shell and run a command. Input and question controls must stay absent; verify an active question still shows its controls. Isolated automated live verification is recorded in the story. The daily app cannot load this Rust fix through hot reload.

## Repeated ego MCP approvals (2089-aafb) — Rust restart required

- [ ] After Boss manually restarts a rebuilt `make dev` or installs `make build`, run a disposable ego default-mode session that requires two or more MCP approvals. Each approval must show Input and native answer controls on mobile; answer once and confirm the controls clear before the next approval. Include a long approval title that wraps. This peer did not restart or replace the running desktop backend. The retained capture supplies the replay regression; seven targeted rb parity/corpus tests pass. Live verification belongs to integration.
## Isolated agent desktop target (2086-269b) — operator live proof pending

- [ ] After the coordinator reviews/builds this branch, checksum Boss's running binary and app bundle, launch `make agent-test` from the worktree, verify the daily port and agent-test loopback port both answer, and recheck identical checksums. Confirm the daily shortcut/microphone/Telegram ownership remains unchanged. Stop with `make agent-test-stop`, verify only its PID exits, and confirm the daily instance still responds. This peer is implementation-only: no desktop app was launched or restarted, and no live isolation/TCC claim is made. The launch-instruction and global CLI/hook refusal changes require a rebuilt backend and refreshed agent launch text; it does not hot-reload into Boss's running app.

## Paired native plugin refresh (2039-428c) — rebuild required

- [ ] After Boss manually restarts a rebuilt backend or installs a signed package, verify notification permission and delivery, then the updater check/download/install flow and deferred relaunch. Rust/npm notification 2.7.0 and updater 2.14.0 are paired; Linux default-feature compilation and targeted tests pass, but macOS/Windows packaging and real OS integration still need release validation. This peer did not restart or replace the running app.

## Audio dependency alignment (2036-f740) — Rust restart and hardware required

- [ ] [HUMAN] After Boss manually restarts `make dev` or installs `make build`, record and transcribe speech with the default and a named microphone; check the meter and stop/drain behavior. Include an integer-only microphone if available. CPAL 0.18 prefers 48 kHz and may rank high-precision formats above I16; capture must continue using a supported F32/I16 configuration.
- [ ] [HUMAN] Play all notification tones at low and high volume on the default and a named output; confirm no end click and missing named output falls back. Play an Edge MP3 reply, pause/resume it, interrupt it and play the next reply. This peer did not restart or use Boss's desktop or claim real microphone/speaker validation.
## Claude imperative question title (2074-b0dc) — Rust restart required

- [x] Worktree verification: real Claude `Pick a colour` without a question mark exports the title and five ordered controls to REST, mobile and desktop PTY Chat. One Green click reaches Claude, clears awaiting and leaves another session unchanged. Mobile evidence: `fix-2074-live-10`; desktop-width Vite component evidence after merging 2079: `fix-2074-desktop-3`, both under `~/Gits/.tmp/results/`. The desktop check mounts the production component and lifecycle consumer, not the full desktop shell.
- [ ] Boss must manually restart `make dev` or install `make build` to load the Rust change in the installed app. This peer did not restart or replace that backend.
## Dirs dependency consolidation (2035-096f) — rebuild required

- [ ] After Boss manually restarts a rebuilt `make dev` or installs `make build`, confirm the existing config, CLI/bridge connection and SSH aliases are unchanged. This dependency update does not load into the running backend; this peer did not restart or replace it. Targeted Linux tests cover config isolation, instance/socket paths, tilde expansion and SSH alias parsing.
## gix 0.89 migration (2038-b4c9) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, verify repository status counts, blame attribution and linked-worktree rows in an isolated repository. This dependency and Rust adapter change does not hot-reload; this peer did not restart or replace the running desktop.

## Codex enforced approval retention (2075-aa2f) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, open an enforced Codex command approval, leave it unanswered, and confirm desktop and mobile retain all three native choices through delayed idle notifications. Approve once, refuse and cancel separate harmless requests; controls must retire after each answer. The running desktop does not hot-reload this Rust change and was not restarted by this peer.
## HTTP and MCP Codex binding (2081-ac1f) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, spawn disposable Codex terminals through HTTP and MCP, submit a prompt and verify desktop/mobile Chat reads each terminal’s own transcript. Include Codex launched inside a generic HTTP shell terminal. Confirm HTTP/desktop and MCP session lists remain responsive after native binding. This peer did not restart or replace the running desktop; reader changes require a rebuilt backend.

## Compact MCP guidance (2069-7347) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, reconnect a cached MCP client and confirm session/agent/repo/ui/voice/telegram/automations advertise `help` where their owner is available. `action=help` returns the complete guide; disabled/unconfigured owners still refuse it. This peer did not restart or replace the running desktop. Automated tests cover catalogue lookup, complete guidance and owner gates.
## UI Design Mode actions (2067-e708) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, reconnect MCP and use `ui design_start` with the target terminal and page URL. Verify browser commands remain limited to the armed recipient/repository and extension host grants. The separate `design_mode` tool remains until story 2068. This peer did not restart the backend.

## Conditional MCP tools (2066-516c, 2067-e708) — Rust restart required

- [ ] After Boss manually restarts the backend with `make dev` or installs `make build`, verify a downstream MCP client refreshes after remote connect/disconnect and Voice/Telegram configuration changes. Clients that ignore `notifications/tools/list_changed` need a reconnect. The running desktop does not load these Rust changes; this peer did not restart it. Conversation binding, Telegram registration and remote digest/session-loss confirmation remain required.
## Joined intent title closure (2060-0ebe) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, check an intent followed immediately by ordinary prose on the same row. CONTEXT must contain only the intent description; desktop and mobile tint must stop at the title closure without hiding trailing prose. Include a title that wraps. This peer did not restart or replace the running desktop.
## Workflow MCP grouping (2065-dcc5) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, reconnect MCP and confirm the grouped `workflow` schema exposes story, run, proposal_create, report and launch. Legacy tools remain available; `tuic workflow` uses the new entry and `tuic automations` stays independent. The running backend was not restarted by this peer.

## Agent task operations (2064-6385) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, reconnect MCP and confirm `agent task_get/task_cancel` appear; cancelling an owned spawned task must leave its terminal running. The installed backend was not restarted by this peer.

## Ego PID index (2058-0d0e) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, confirm an ego terminal discovers its active conversation through the PID index. The live backend does not hot-reload Rust. This peer did not restart or change the installed desktop instance.

## Agent session discovery and frame receipts (2040-16b2) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, measure native keyboard-to-paint latency while a visible agent prints and other agent tabs perform session discovery. Confirm frame-receipt stalls no longer correlate with discovery file reads. The backend regression checks real PTY echo and an independent frame consumer under slow discovery I/O; it does not prove that every historical stall or native WebKit freeze is resolved. This peer did not restart or change the running desktop.

## Mobile ego slash filtering (2030-84bb) — Rust restart required

- [x] An isolated native headless worktree build and worktree Vite, through agent-browser at 1180×820, show only `/goal` for `/go` both idle and while a real ego turn runs. Exact `/goal` and unmatched `/gz` retract the menu. Claude's `/he` also replaces its broad menu with its own fuzzy matches. Evidence and DOM measurements: `src-tauri/src/fixtures/agent_prompts/ego-slash-filter-2030.md`.
- [ ] After Boss manually restarts `make dev` or installs `make build`, the installed app can load the fix; the running desktop was not restarted or used to verify changed Rust.
- [ ] Codex 0.162.1 draws `/status` and `/statusline` for `/sta`, but publishes no mobile dropup. Its menu sits above the composer/status line and uses `›`, unlike this parser's supported bottom block. This layout gap also exists in `main`; the shared Claude/Codex criterion remains unchecked. See the inline `DEFERRED (2026-10-19)` note at `parse_slash_menu`.
## ego Chat export diagnostics (2032-8dc3) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, verify an incompatible configured ego executable reports its path and failed export status. For the retained live failure, Settings points to `/Users/stefano.straus/bin/ego`, which rejects `--after`; select the transcript-capable `/Users/stefano.straus/.local/bin/ego`, then open Chat on a bound local ego session. Confirm the completed answer is shown and Chat stays open. This peer did not alter Settings, replace a binary, restart the app, or call desktop MCP tools.
## Agent session discovery (2057-2769) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, profile an isolated instance against about 1500 ego sessions and 10 polled ego terminals. Confirm discovery stays off the desktop main thread and repeated polls read only each bound lease. Check session ids after an ego restart and pid reuse. This peer did not restart or probe Boss's desktop; REALITY stays open for the coordinator.

## Canvas base-grid repaint reductions (2040-16b2) — load landed frontend

- [ ] With the landed frontend in native WebKit, verify typing stays responsive during scrolling agent output and record keyboard-to-paint latency. Check normal history growth, a stationary spinner, colored trailing blanks, wide glyphs, combining marks and marker-filtered output. Worktree browser replay proves lower drawing-operation counts and identical reference pixels, not native timing or the ego printer's independent row-emission delay. No Rust restart is needed for these frontend-only changes; do not restart a live desktop merely for this check.
## MCP worktree listing latency (2044-31f3) — Rust restart required

- [ ] After Boss manually restarts the Rust backend, repeat `tuic mcp repo` with `action=worktree_list` on the many-worktree repositories. Confirm cold listing makes no GitHub requests, dirty file/fingerprint fields still update after edits, and the sidebar remains unchanged. A GitHub-only merged proof can be absent until background monitoring populates the existing cache. Fresh Git/filesystem reads are not a hard latency guarantee. This peer did not restart the installed app; criterion 4 remains unchecked.

## Live spawned caller declaration (2044-31f3) — Rust restart and updated frontend required

- [ ] After Boss manually restarts the Rust backend and loads the updated frontend, spawn a native MCP agent, an HTTP agent and a generic `POST /sessions` terminal in an isolated instance. Declare an existing worktree from each caller and confirm only its live tab moves, with unchanged process cwd and stable placement after reconnect. Native session-created now carries the backend-bound identity; the installed backend was not restarted by this peer.

## Declaration into a newly discovered workspace (2044-31f3) — Rust restart required

- [ ] After Boss restarts the Rust backend, use an isolated instance and declare an existing linked worktree whose saved workspace row has no `savedTerminals`. Confirm declaration succeeds, the caller restore record moves without duplicating sibling records, and the caller tab appears under the declared workspace despite unchanged cwd. Repeat the declaration, cd into another repository or an unregistered directory, reload the frontend and send a cwd update; placement must persist without moving sibling tabs. The peer did not restart or mutate the installed instance; live sidebar verification is still pending.

## Automation profile validation and failure mail (2015-bdb3) — Rust restart required

- [ ] After Boss restarts `make dev` or installs `make build`, use an isolated instance to reject an unknown saved profile with valid-profile choices. Remove a profile after saving and confirm list exposes `run_config_error` with no next run. A failed agent-created run must send its creator one inbox message with run id and reason even before spawn; repeated publication must not resend it. The installed backend was not restarted by this peer.
## Mobile suggest dismissal (2029-994d) — Rust restart required

- [ ] After Boss rebuilds/restarts the Rust app, verify Claude publishes `suggest: [ Reply alpha | Reply beta ]` after Stop hooks settle. At 390×844, tap Reply alpha and confirm all old chips disappear and stay gone after the next poll and an ACK-only response. Final recorded-stream tests and isolated backend submission checks pass, but the Mac locked during the final rebuild: Chrome reports `visibilityState=hidden`, so final trusted-click/poll screenshots remain unverified. Earlier screenshots expose the pre-boundary history-repaint defect, not a final pass. Evidence: `~/Gits/.tmp/results/fix2029-live/`.
- [ ] Verify a genuinely later list, including identical labels after the backend clear, appears. Disconnect before choosing: exhausted retries must show the existing Send failed notice. Targeted component tests cover both cases; desktop behavior is unchanged.

## ego title and enforced approval parity (2028-6999) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, verify ego question titles exclude skill/provider diagnostics and working rows. Check a default-mode CLI-enforced command approval on desktop/mobile: native keys appear, tapping No opens the explanation prompt without submitting it, and a subsequent bare Enter refuses and clears the wait. Confirm an answered selector stays cleared while its retained rows scroll after protocol busy. Replay evidence does not verify updated live UI rendering. The exact historical provider-notice adjacency was not reproduced.

## Grok native question parity (2027-7ef3) — Rust restart required

- [ ] After Boss restarts `make dev` or installs `make build`, verify Grok Ask color/Yes-No choices and a default-mode CLI-enforced tool approval on desktop/mobile. Numeric answers must submit once without Enter; z must switch to a text-only wait, and refusal/settlement must retract the controls. Live native CLI forms were captured, but this worktree backend was not loaded into the desktop.

## Codex native question parity (2026-9178) — Rust restart required

- [ ] After Boss restarts the backend, trigger `/plan` `request_user_input` with “Pick a colour” and Red/Green/Blue. Verify the clean title and four choices (including None of the above) on desktop and authenticated mobile. The 2026-10-19 raw-byte parser regression does not prove live UI rendering; mobile verification requires Boss’s login.

- [ ] After Boss manually restarts `make dev` or installs `make build`, ask Codex 0.162.1 for a native color question. Verify the desktop/mobile waiting state, use the mobile Open Codex question action, and answer once. Confirm options retract after settlement. Also trigger a CLI-enforced command approval and verify numeric refusal without Enter. Live CLI Shift+Left and numeric answers were captured; updated desktop/mobile rendering has not been verified against this worktree backend.
## Automation validation parity (2007-59f6) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, submit an automation action with an unknown field through HTTP and MCP on an isolated instance. Confirm HTTP returns 400 and both payloads report `invalid_arguments`, bounded violations and `effect=not_started`. No live desktop or installed binary was restarted by this peer.

## Durable peer-mail subscriptions (2004-4760) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, create two named actionable readers for an isolated peer. Send idle lifecycle traffic, then RESULT and BLOCKED mail; confirm each reader receives the result independently, explicit ack prevents reconnect replay, and an interrupted unacknowledged batch remains available. Check the HTTP SSE stream and ACP resource notification path. No installed binary or live desktop instance was replaced by this peer.
## Local spawned-agent Chat (2032-8dc3) — frontend load after landing

- [ ] After the frontend loads the landed change, spawn a throwaway local agent with `tuic agent spawn` and confirm Chat opens once the transcript is bound. Confirm worker-tab cleanup and muted completion policy remain unchanged; configured remote sessions must still have Chat disabled. The live pre-fix desktop reproduced the disabled Chat tooltip with bound Claude session rt-2032-1; that PTY was killed. No desktop restart or second instance was used.

## Ego idle turn state (2018-1885) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, read `tuic agent ls` and the mobile session list for an ego session at its ready composer. It must not report Working solely because a background job remains pending. Confirm an active ego turn still reports Working and a question still reports Input. Keep background ownership metadata for suspend/standby safety. An idle ego with a current-turn pending descendant probe must refuse Suspend until ownership is resolved, even when `background_work` is false. No live desktop restart was performed by this peer.

## Generated launch modules — Rust restart required

- [ ] Inspect the launch module switches and additions preview after unlocking the Mac. Browser snapshot confirmed all labels and initial checkbox states; direct Chrome and wrapper screenshots could not complete while the macOS screen was locked.

- [ ] After Boss restarts the backend, launch a root and a spawned child. Inspect the delivered launch text: both get enabled modules and global then agent additions; only the child gets Children and omits suggest. Turn all off and verify a new launch has no TUIC playbook. Existing agent processes are not updated by hot reload.

## Launch instruction preview critic fixes — Rust restart required

- [ ] After Boss restarts `make dev` or installs `make build`, verify launch instruction previews on a daemon-backed machine. Switch from a loaded local machine to a delayed or unavailable remote: inherited text must not carry over and Customize must stay disabled until that machine's preview arrives. The running desktop backend was not restarted.
## Mobile Ideas dictation (2022-1b80)

- [ ] On an iPhone with the updated frontend, dictate with the caret inside an idea and with selected text. Confirm surrounding words remain, a second take works, and no idea or session command is sent until the user explicitly adds or sends it. Browser screenshots verify the real overlay at 390 px, not physical speech recognition.

## Mobile dictation recovery (2020-66aa)

- [ ] After the updated frontend loads on the phone, record a real take and inspect debug source `dictation` on the owner. Retain its session/take-correlated start, first-result, error/abort if present, end and ownership-release sequence for any recurrence. These records are bounded diagnostics, not an assertion that the original incident is fixed.

- [ ] On an iPhone after the updated frontend loads, reproduce a failed/empty recognition, then dictate again without a manual send. Confirm the next transcript inserts and “Dictation failed” stays visible beside the composer mic with the real keyboard open. The worktree browser probe used a focused textarea at 390×480 and a lifecycle driver; it did not record iOS callback order or display a real on-screen keyboard.

## Run-owned launch timeout (2006-f5a1) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, use an isolated run-owned child stalled before its composer is ready. Read its durable launch receipt after 30 seconds: it must report blocked with a delivery-timeout reason and retain one queued prompt and the same child identity. No live desktop restart was performed by this peer.
## Guided event configuration (2012-72ff)

- [ ] On a throwaway headless owner or browser-mode instance, send each Configure Smart Prompt to a test agent and verify the step-by-step interview stops on missing owner/profile/GitHub auth/skills and unsupported intake, without saving or launching work. Automated library tests verify the delivered prompt contract, not model compliance.
- [ ] After beta, with Boss-approved repository/test issue and authorization on the owner, run one labelled-issue-to-draft-PR pilot. Retain independent reviewer/check receipts on the published commit, the real draft PR URL and reconciliation evidence. No live GitHub publication or credentials were used by this change.

## Continuous answer gutter (1998-d826)

- [ ] [VISUAL] Capture the worktree CLI gutter for the real Claude answer fixture with blank rows, section headings and marked list items; confirm one continuous line and no line on trailing unmarked prose or tool output. Both headless Chrome capture attempts failed to produce a PNG; the bounded retry stopped its own Chrome/Vite processes after 35 seconds. The production-plan regression passes, but it is not screenshot evidence. No Rust change or backend restart is required.

## Agent exit backend cleanup (1990-66ac) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, exit Claude from a mobile-only terminal. Confirm shell output replaces the old agent screen in both the open view and a fresh page, and the session card drops the old intent/task. Repeat with an agent that restores its alternate screen normally. No live desktop was restarted by this peer.

## Desktop standby input recovery (1986-1236) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, let an isolated plain-shell terminal enter standby after its foreground child exits. Confirm desktop typing wakes it and executes the command without switching tabs again. Repeat by viewing the terminal and by sending from the phone. The running backend was not restarted; the real-PTY regression is not evidence that Boss's current binary loaded this fix.

## Mobile agent exit recovery (1985-45bd)

- [ ] [VISUAL] On a phone, exit a throwaway viewed ego session while its question is open, then type and send a shell command. Confirm agent/question controls disappear and shell output/input remain usable. Repeat with Claude Chat open and after reconnecting the output socket. Close the PTY and confirm Session ended offers Back. Component regressions do not prove phone touch behavior; no live production sessions were used.
## Live Settings credentials (1984-3472) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, use an isolated instance to approve a username or password change in Settings. Confirm the save succeeds and new Basic Auth/login requests use the new credentials without restarting the listener. Real headless listener port changes must still report persisted-but-unavailable application. No live desktop restart was performed by this peer.

## Mobile human replies (1980-47b7) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, answer an ego free-text question from the phone, both with its card open and with the ordinary composer. Type an answer before opening the card, then open it and Send; confirm the complete answer reaches the agent and it continues. In a fresh ego session, confirm the card shows only the question, not the startup banner or shortcut help. Use a recognized picker to select an option, then verify a typed prose reply to that picker explains the selection requirement and keeps the draft. No live desktop was restarted.
- [ ] [VISUAL] After the Mac unlocks, capture the mobile free-text question, picker selection and refused-answer toast at phone width. Supplied incident screenshots were read; no new screenshots were taken while the screen was locked. No CSS or layout change is part of this fix.
## Mobile voice handset (1979-89e1)

- [x] [VISUAL] Capture the idle handset in the phone More menu and tablet header in Commander and Paper. _(verified: screenshot-only captures at390×844 and834×844 in `.tmp/mobile-question-visual/after-1979/png/`; visible menu action230×44 and header action44×44; no live microphone or backend access.)_
- [ ] [VISUAL] Capture the armed handset and unchanged composer dictation microphone on a real voice session. The isolated presentation fixture only captures the idle handset; existing armed/unarmed component tests remain for batch integration.

## Readable mobile pushes (1978-9cfe) — Rust restart required

- [ ] After Boss restarts `make dev` or installs `make build`, receive an awaiting/progress push from a named session and an unnamed agent session. Confirm readable names and progress text, no UUID/tool JSON in the displayed body, and that tapping opens the correct session. No live desktop restart or phone push was performed by this peer.
## Soft-wrap text reconstruction (1977-063c) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, compare mobile CLI output and MCP session text with raw output for `unrestricted filesystem access` at 24 columns and a mid-word wrap at 64 columns. Confirm spaces, copied selection, ego question labels and explicit hard line breaks. At four columns, `abc界x` must copy and read as `abc界x`, without a space before `界`. The live desktop was not restarted by this peer.
## Mobile Claude pickers (1971-5834 / 1972-128e) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, open a throwaway Claude terminal in the mobile CLI view. Verify `/resume` shows its session rows and search results, then use key-bar Up/Down and Enter to resume one. The awaiting AskUserQuestion arrows must move selection without changing a local answer draft. The isolated headless regression and capture evidence do not load the Rust fix into the running desktop app.
- [ ] Capture mobile screenshots of the populated `/resume` list and a moved AskUserQuestion selection after macOS is unlocked. The coordinator authorized skipping these captures while the screen was locked; no screenshot was claimed.

## Settings restyle integration (1945-a78d)

- [ ] [VISUAL] After the Mac unlocks, capture isolated worktree Settings against the approved 2026-10-16 mock, not Boss's desktop. Capture Commander dark and Paper light at 1400×900 for all available pages across Application, Workspace, AI, Integrations, Plugin settings and Repositories. Include the selected machine/repository/plugin source, search results and a revealed expert field. Capture each theme at 780×900 and 390×844 with long repository/plugin names, paths, validation text and contextual help. Check stacked rows, compact page selection, wrapping, 44px touch targets, independent scrolling and stable shell size. Capture delayed plugin loading, empty search/catalogue, failed read and rejected-write recovery without fabricated live-service status. Screen locked during S10; no screenshots or live desktop mutation claimed.

## Settings transport confirmation (1787-4778) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, use an isolated instance. Replay a gated HTTP or native settings edit with its `approval` proposal ID and `confirm: true`; approve the existing dialog and verify persistence. Decline another edit and verify no write. Confirm that replacing tunnel forwards, including an empty list, now asks for approval. No live desktop was restarted by this peer.

## Mobile session Chat appearance (1974-e6ae)

- [ ] After the Mac unlocks, capture `src/__tests__/browser/sessionChatAppearance.html` on isolated worktree Vite: desktop at 1000×900 and `?mobile` at 360×800, 390×844 and 430×900; repeat with `theme=light` (Commander dark and Paper light). Compare baseline `ea8c46bed6` and the 1974 commit side by side. Copy only the layout fixture into the baseline checkout; keep its baseline production CSS. The fixture uses real session transcript/switch/input components and synthetic text, not an agent transport.
- [ ] Inspect the real mobile session detail Chat after integration: user prompt tint, folded tools, answer card, hint and existing composer in both themes. Verify send, scroll, CLI/Chat switching and answering remain usable on a phone; no touch or screenshot evidence is claimed by the static checks.

## Relay room expiration (1932-8398) — standalone relay rebuild required

- [x] Expired peers cannot forward into a recreated room, remove replacement peers, or refresh their activity; idle socket readers terminate. _(verified: Linux rb targeted relay tests, including different-owner and same-owner room reuse and stale channel identity.)_ Rebuild and restart any separately deployed `tuic-relay` binary to load this fix. This is not a desktop backend edit; no live TUIC restart is needed or performed.
## Managed injection recovery (1957-38c1) — Rust restart required

- [ ] Run a full fake-agent delayed-echo E2E in an isolated headless test instance with a real PTY. Delay the fake agent's composer echo after Enter to force uncertain injection, queue a later BG DONE wake, then render a fresh stable empty ego composer. Verify the later wake reaches the fake agent once without replaying the earlier uncertain payload, raw input returns a bounded uncertainty error while the reader is stalled, and close terminates the stalled session. Use the recorded ego composer fixtures for screen semantics; the existing recording-writer regression is not this process-level E2E. Do not use Boss's instance or sessions. This check remains unverified.
- [ ] After Boss manually restarts `make dev` or installs `make build`, verify a later urgent send is not permanently suppressed by an abandoned notice reservation. The 30-second reservation lease does not replay the earlier payload. Also verify raw input returns an uncertainty error rather than hanging on a stalled reader, close still terminates that session, and a fresh stable empty ego composer releases a later queued BG wake without replaying earlier input. No live desktop was restarted by this peer.
## AI Chat tidy layout and smart tables (1958-b67e / 1923-c6fb)

- [ ] Run the batch integration GREEN checks for AIChatPanel, Transcript, ContentRenderer, ChatTables, mobile Chat, panel adapters and the paste-ownership consumers. This peer ran only the required history-duration RED test.
- [ ] Capture native worktree-only Vite screenshots at 380/700 px for Tidy and 390/740 px for tables, dark and light, and compare them with the approved mocks. Chrome DOM measurements pass the compact bounds (35 px chrome, 1296 px conversation at 380 px). Native screenshot capture stalled; browser print-to-PDF raster PNG alternatives are under `~/Gits/.tmp/tuic-1958-png/` and are not native screenshots.
- [ ] Inspect the mobile read-only table document, return focus to Chat, and verify the existing mobile composer remains usable. Do not use production sessions for verification.

## MCP instruction byte budget (1947-3d17) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, reconnect an MCP client and confirm answer markers still support row starts and Markdown list-item text starts. The remote budget regression and all 563 MCP transport tests passed; no live app was restarted.

## Async AI Chat options (1918-2ea3) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, open a new AI Chat tab without sending a prompt. Verify Session settings appears with model and mode choices; select an offered model. Repeat with delayed offers and multiple tabs, then verify replacement offers and snapshot refresh/reconnect preserve session isolation. No live app was restarted by this peer.
## App setting choices (1915-d77f) — Rust restart required

- [ ] After Boss manually restarts, use an isolated instance. Check bundled font choices and a custom theme agree with Settings UI. Explicitly refresh installed IDE choices and check their labels/availability on macOS, Windows and Linux. Describe must not launch discovery. No desktop instance or real user config was used by the agent.

## Typed UI settings owners (1914-cdf6) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, use an isolated instance: set and disable a keyboard shortcut, reload, and confirm both persist. Split panes, switch focused panes and restore the layout; verify geometry and tab membership remain intact. The settings owner exposes only geometry, not runtime tab/focus identities. No running app was restarted.
## Persistent stall evidence (1638-c94c) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, retain the journal files after the next real desktop stall and build a metadata-derived replay input with `scripts/build-stall-replay.mjs`. Record independent backend-request timing separately. Targeted tests cover retention, rotation and numeric-only records; original real-stall RED/GREEN remain open. No managed peer restarts or stresses the live app.

## Urgent ego mail (1901-9b6b) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, send urgent mail to a busy managed ego child. Verify the inbox notice is queued without cancelling its tool, the body stays in the inbox, and a draft or question blocks injection. The targeted backend tests cover notice bytes, coalescing, drafts and confident dialogs; no live app was restarted for this change.

## GitHub Work B6/B7 bridge (1693-a816) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, reload the updated GitHub Work plugin. Verify a PR detail loads enriched review threads under the selected account and that the plugin host ports reach core action state. Do not perform merge/review/rerun writes without separate authorization. Verify cross-account rejection and desktop plugin loading; no managed peer launched a second desktop instance.

## GitLab provider plugin (1695-9f3a) — needs 1697-4cdc and a desktop restart

- [ ] After story 1697-4cdc lands (`host.httpFetch({account})` and manifest provider declarations) and Boss restarts the desktop app, install the `gitlab` plugin and add two GitLab accounts, one on a non-default port. In Tasks, confirm each account lists its own issues, merge requests and to-do items; paste an item URL of each instance and confirm only its own account resolves it. On a merge request, confirm merge/squash choices, readable blockers, approve, pipeline jobs and job retry; on a board, move a card and confirm the list label changes in GitLab. The plugin tests run on fixtures recorded from GitLab CE (`plugins/gitlab/fixtures/recorded.json`); no desktop instance was launched by this peer.
## Plugin hot reload re-reads sub-modules (1809-010d) — Rust restart required
## Settings owner prerequisites (1787-4778) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, verify isolated owner requests follow an Ego launch profile change and first configuration without recreating the registry. Verify removal of an on-connect SSH connection stops its ephemeral daemon after settings persistence. The integrated cutover closes the owner inventory gaps and replaces the config tool with settings; load the batch build before checking this path. Targeted owner regressions cover the saved-record callback, profile resolution and bindings-save failure. Do not restart the running app for this check.
## Orphan removal threading (1638-c94c) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, remove a throwaway orphan worktree through the desktop cleanup UI. Record desktop heartbeat continuity and independent backend-request timings during removal. The module-boundary regression covers async/blocking-pool placement, not a live heartbeat replay. No second desktop instance was launched.

## Design Mode shorthand tokens (1888-14a3) — backend rebuild required

- [ ] After Boss manually restarts `make dev` or installs `make build`, confirm a native Design Mode grab includes `--gap` for `padding: var(--gap)`, including an open shadow root with an adopted sheet. Real Chromium extraction and the Rust payload consumer passed; the embedded `include_str!` asset needs a backend rebuild to load. No desktop instance was launched by this peer.
## Suspend replacement identity (1806-ed18) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, suspend and resume a throwaway idle shell twice. The TUIC identity stays stable, each Resume has a fresh PTY id, and the second Suspend succeeds. On a remote repository, Resume and closing a suspended tab must act on that daemon. Automated targeted tests cover the lifecycle and routing; the running desktop requires a restart to load the Rust change.
## Shell wrapper argument guards — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, open a new shell. With OpenCode native scrollback enabled, verify `opencode mcp list` receives no `--mini`; a bare interactive launch still does. Verify Goose `run`/`session` with `--name=work`, `-nwork`, `--path f.jsonl`, `--path=f.jsonl`, and `run --no-session` receive no TUIC `--name`. Existing shells keep their sourced wrappers until reopened. Automated argv tests cover bash, zsh and fish.

## Scoped plugin settings (1785-c7a1) — Rust restart required

## AI Chat retry countdown (1536-1c9c)

- [x] Provider retries count down to “retrying now”, clear on recovery/error, and user-message Copy stays on one line. _(verified: 195 targeted Vitest tests and isolated full-shell actual-ego/local-503 screenshots in ~/Gits/.tmp/results/fix-1536cd/live/countdown-{0,6,11}s.png; copy-before/after.json show four text lines becoming one; final-recovery.png has no retry status.)_


- [ ] After Boss manually restarts `make dev` or installs `make build`, use an isolated instance with a configurable plugin. Describe its settings, patch a non-secret field using its revision, and verify its existing unload/load callback consumes the new value. A token reads as set/unset through settings; unloading removes its catalogue entries. Automated isolated owner/HTTP tests cover persistence, dispatch, zero limits, and refusal of linked settings paths; the current desktop needs a rebuild/restart to load this Rust change.
## Mobile keyboard correction (1878-3a3c)

- [ ] [HUMAN] On iPhone Safari or the installed PWA, check autocorrect suggestions, spellcheck and sentence capitalization in the terminal prompt with Settings → Input → Autocorrect and spellcheck on and off. Reload and confirm the choice persists. Native keyboard behavior requires the real phone; rendered attributes and persistence are covered by Vitest.

## Tasks MCP access (1854-4960) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, use an isolated instance with a loaded local test provider: `repo work_item` reads its worktree-linked item and comments, including when another worktree has an earlier link-store entry; `work_item_act` refuses writes by default. Enable `mcp_work_item_writes` through config save and verify one synthetic comment/transition, then disable it. Do not use a real tracker for automated writes. Targeted transport/provider tests cover the mechanics; the running desktop must be rebuilt to load the Rust bridge.

## One-time beta channel migration (1817-beab) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build` with a beta version, an old config with `update_channel=stable` and no `beta_channel_migrated` marker switches to Beta once. Selecting Stable afterwards survives restarts. Use a throwaway isolated instance; stable builds must preserve the saved channel. Targeted Rust tests cover the load/save migration; the running desktop needs a rebuild/restart to load it.

## Continuous answer regions (1872-1870)

- [x] Recorded Codex paragraphs form one CLI and Chat tint with no visible answer glyphs. _(verified: worktree CanvasTerminal and Transcript screenshots in ~/Gits/.tmp/results/1872/before-grid-chat.png and after-grid-chat.png; real Codex VT replayed with the Rust grid.)_
# To Test

- [ ] [HUMAN] Mobile composer safe area (#2116-0213): in the next guided iPhone session, check the installed PWA with keyboard closed/open/reopened. Controls must stay above the Home Indicator and the composer background must reach the bottom. If the separate shell-height band remains, capture a full-screen screenshot, device/iOS, orientation and keyboard state before reloading. The bottom-inset change is not proof that this band is fixed.

## Work-item page total (1845-66f6) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, `WorkItemPage.total` / `WorkItemGroupedPage.total` are accepted by the Rust DTOs (optional, defaulted). Targeted `plugin_docs` and `work_items` tests pass; the Tasks list shows "N of total" once a loaded GitLab plugin build that sends `x-total` is installed (plugin branch parked, see story worklog). _(triage 2026-10-14: WAITS — Needs the named integration/plugin release to land and load before the consumer check.)_
## GitHub settings owner adapter (1774-ce8f) — Rust restart required

- [ ] After transport wiring, use a throwaway instance to complete default/additional cloud device login and add an Enterprise PAT. Confirm immediate presence-only readback, real identity, bindings and account-removal cleanup. Boss performs provider authentication; do not use production credentials for automated checks. _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
## Ego settings adapter (1775-330c) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, model reads/writes in Settings > Providers use the configured Ego profile and AI Chat workspace. Existing ACP and PTY sessions keep their launch configuration. Internal settings adapters await the later transport-registration step; targeted fixture tests cover owner commands and readback without touching production credentials. _(triage 2026-10-14: WAITS — Needs the named integration/plugin release to land and load before the consumer check.)_



## Screen stays awake while dictating (1767-3071)

- [ ] [HUMAN] On the iPhone home-screen PWA (iOS 18.4 or later), dictate into the draft, and separately run hands-free, for longer than the Auto-Lock time. The screen must stay on and the recording must continue; after Stop it must lock as usual. Turn "Keep screen awake while dictating" off in mobile Settings and confirm the screen locks again. Before iOS 18.4 the API does not work in home-screen web apps ([WebKit bug 254545](https://bugs.webkit.org/show_bug.cgi?id=254545)), so a failure on 18.3 or earlier is expected. Vitest covers acquire, release, re-acquire and the setting; the device is unverified. _(audit 2026-10-13: HUMAN — iPhone.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Fn dictation through focus changes (1763-3361) — Rust restart required

- [ ] [HUMAN] After Boss manually restarts `make dev` or installs `make build`, dictate for over 60 seconds with short pauses while holding Fn. Let a toast or new tab appear, then move focus to another window while continuing to hold Fn. Recording must continue; release Fn in that window and confirm the microphone stops, the text arrives once, and both its opening and ending words are present. Check the file log for `Fn focus loss observed` with `physically_down=true`, followed by a real native key-up or `origin="fn-state-poll"`. Native adapter and AppKit timer tests cover retention and one release; real microphone/Fn hardware and the complete long transcript require this check. Do not restart Boss's running TUIC from an agent. _(audit 2026-10-13: HUMAN — Fn hardware.)_ _(triage 2026-10-14: WAITS — Needs real microphone/Fn hardware and a long transcript through native focus changes.)_
## Mobile keyboard, tab bar and session cards (1795-a710)

- [ ] [HUMAN] On the iPhone home-screen PWA, focus the command input of a session: it must sit directly above the keyboard without a manual scroll, and the view must not bounce. The iOS keyboard cannot be emulated; Vitest covers the shell variables from a fake visualViewport. _(audit 2026-10-13: HUMAN — iPhone.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
- [ ] [VISUAL] Sessions list: the tab bar is 56 px tall and the + button sits above it; cards end about 16 px sooner and the close button still answers a tap on its 44 px area. A band of about 62 px under the tabs remains on the standalone PWA: it is the 1746 band, present before and after cc21c0945 (Boss's screenshots 5 and 6, 2026-10-12), and it needs device measurement before a fix. _(audit 2026-10-13: HUMAN — iPhone.)_ _(triage 2026-10-14: WAITS — Needs the installed iPhone PWA to measure its reported bottom band.)_
## Mobile title and compact AI Chat composer (1746-aa9e)

- [ ] [VISUAL] After the TUIC restart, reload the iPhone PWA. Park/Restore/Swap and Stop should share the input row with Attach and Send; the bottom tabs should meet the screen edge _(NOTE: refuted on the device on 2026-10-12; see 1795-a710)_. Headless Chromium geometry and screenshots cover 360/390px with 62/34px safe-area overrides; the actual iPhone remains unverified. _(audit 2026-10-13: HUMAN — iPhone.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
- [ ] [VISUAL] Inspect the title on the iPhone with the menu closed and open. The reported screenshot has progressive blur over the title and connection dot, with a crisp subtitle. TopBar, shell and mobile global CSS contain no title/ancestor blur, mask, text-shadow or opacity; the app opts into a translucent iOS status bar in `mobile.html`. The likely cause is WebKit's native scroll-edge effect, which is outside page CSS ([WebKit scroll view](https://github.com/WebKit/WebKit/blob/main/Source/WebKit/UIProcess/ios/WKScrollView.mm) and [native edge wrapper](https://github.com/WebKit/WebKit/blob/main/Source/WebKit/UIProcess/ios/WKUIScrollEdgeEffect.mm)). This is an inference, not a confirmed device diagnosis. Do not add guessed spacing or filter overrides until device inspection identifies the native layer. _(audit 2026-10-13: HUMAN — iPhone.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Design Mode dev-server list collapsed (1727-0338)

- [ ] [VISUAL] In Boss's Edge profile, reload the Design Mode extension and open the side panel with many dev servers. The list is collapsed under "Dev servers (N)" and a new pick card shows without scrolling. Vitest and a real side panel in Chrome for Testing with a stub native host (DOM evidence, no screenshot) cover it. _(audit 2026-10-13: HUMAN — Boss's Edge profile.)_ _(triage 2026-10-14: WAITS — Needs the installed extension/profile operated by Boss; no profile reload performed.)_
## Design Mode picks while a comment has focus (1731-3be4)

- [ ] [VISUAL] In Boss's Edge profile, type in a pick card comment, then click more elements on the page. Each new pick appears in the side panel and the comment keeps its text and caret. Vitest and a real side panel in Chrome for Testing with a stub native host cover it. _(audit 2026-10-13: HUMAN — Boss's Edge profile.)_ _(triage 2026-10-14: WAITS — Needs the installed extension/profile operated by Boss; no profile reload performed.)_
## Design Mode recipient follows the starter (1734-9785)

- [ ] [VISUAL] In Boss's Edge profile, run Start Design Mode from a session (for example feat-1719). The side panel preselects that session labelled "started Design Mode" and lists other agents under "Other agents". A manual choice holds until the next Start. Vitest and a real side panel in Chrome for Testing with a stub native host cover it. _(audit 2026-10-13: HUMAN — Boss's Edge profile.)_ _(triage 2026-10-14: WAITS — Needs the installed extension/profile operated by Boss; no profile reload performed.)_
## Automations worktree cleanup render (1623-8b2a)

## Design Mode navigation restart (1724-fe7e) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, reload the Design Mode extension and request navigation to a local page. Confirm the first navigation reaches the reviewed tab, with and without an idle heartbeat before the request. Automated heartbeat boundary tests cover delivery and failed-update retry; no desktop instance was started by this peer. _(audit 2026-10-13: HUMAN — Needs Edge extension reload.)_ _(triage 2026-10-14: WAITS — Needs the installed extension/profile operated by Boss; no profile reload performed.)_
## Settings info badge tips stay in view (1741-36e5)

## GitHub token chain without gh (1714-1c1d) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, start TUIC with `GH_TOKEN` and `GITHUB_TOKEN` unset and no OAuth login, while `gh auth login` is still valid. Confirm Settings > Git & GitHub shows "Not connected" and that no `gh auth token` process starts (`ps` during boot and polling). Sign in with OAuth and confirm PR status, CI rings and PR writes work. Then set `GH_TOKEN` and confirm the tab shows "Environment variable". An account record saved earlier with `auth_source: gh_cli` must load as the OAuth github.com account. Targeted rb tests cover the chain order, the record load and the empty `gh` spawn allowlist; a live GitHub call is unverified. No desktop instance was started or restarted by this peer. _(audit 2026-10-13: HUMAN — OAuth sign-in/GH_TOKEN/Settings view. Partial: rb github_http_guard_tests::production_code_spawns_no_gh passed; no `gh` process in ps.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
## Hands-free listening / spoken replies split (1729-f7f1) — Rust restart required

- [ ] After Boss restarts `make dev`: (1) Settings → Voice, turn Hands-free listening off, Spoken replies on, Start conversation on a Claude terminal: no microphone indicator, the agent is told it may speak, push-to-talk dictation works, and the agent's `voice speak` is heard (with Dictation language Auto, after the first push-to-talk take). (2) Listening on, replies off: speech is captured and the agent answers in text. (3) Both off: Start conversation is refused with a clear message. (4) While recording, click the speaker button in the toast: it flips Settings → Spoken replies, and turning it off while a reply plays stops it. (5) Browser/phone tab: replies-only asks for no microphone permission. Layout of the toast button still needs Boss's eye in Design Mode. Vitest and Rust targeted tests cover the logic; real audio is unverified. _(audit 2026-10-13: HUMAN — Audio, mic, toast layout.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Design Mode dev servers (1723-4a21) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, reload the Design Mode extension and refresh servers. Confirm TUIC, non-HTML listeners and servers of other worktrees are absent, and only the bound session’s servers are listed. No desktop instance was started or restarted by this peer. _(audit 2026-10-13: HUMAN — Needs Edge extension.)_ _(triage 2026-10-14: WAITS — Needs the installed extension/profile operated by Boss; no profile reload performed.)_
## Commit failing checks (1712-b11c) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, select a failing external check in a throwaway PR and confirm auto-heal uses its exact head and detail link. Confirm a successful rerun removes the stale failure. The lookup uses the bound repository account over GraphQL and does not depend on the first 100 open PRs. Recorded GitHub fixtures cover CheckRuns, external StatusContexts and pagination on rb; live account requests remain unverified. No desktop instance was started or restarted by this peer. _(audit 2026-10-13: HUMAN — Needs throwaway PR/live GitHub write.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
## Merged GitHub PR proof (1711-67a7) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, check sidebar lifecycle and cleanup of a squash-merged throwaway worktree for a bound cloud account and an enterprise account. The proof now uses that repository's account over HTTP GraphQL and must not trust an incomplete response. Targeted stub tests cover pagination, retryability and the blocking-thread bridge; live account requests remain unverified. Rust does not hot-reload; no desktop instance was started or restarted by this peer. _(audit 2026-10-13: HUMAN — Live cloud+enterprise accounts.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
## GitHub REST headers (1709-e9f4) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, confirm issue close/reopen, PR approve/close/update and diff retrieval work for a bound cloud account and an enterprise account. Requests now share the account host/token and pinned API headers. Targeted request and error/breaker tests cover the contract; live GitHub operations remain unverified. Rust does not hot-reload; no desktop instance was started or restarted by this peer. _(audit 2026-10-13: HUMAN — Live GitHub writes (close/approve).)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
## Plugin account requests (1697-4cdc) — Rust restart required

- [ ] After Boss restarts `make dev` or installs `make build`: install a plugin whose manifest declares `"capabilities":["net:http"]` and a `providers` descriptor, confirm the provider appears in the account add flow, add a throwaway account, and call `host.httpFetch(<site>/<path>, { account })` from the plugin. The response arrives without the token; a URL outside the site and a plugin without that provider are refused. Uninstall the plugin and confirm the account remains listed. Local-server tests on rb cover injection, host rule, consent, redirects and redaction; a real Jira/GitLab/GitHub call is not verified. No desktop instance was launched by this peer. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Needs account add flow (token) + plugin install. Partial: rb plugin_http::account_tests (7) passed.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
## Work-item links and sidebar badge (1685-4b12) — Rust restart required

## Window resize background (1678-158c / GitHub #125) — Boss build required

- [ ] After Boss runs `make build` (or manually restarts `make dev`), rapidly enlarge and shrink the main window on macOS. Repeat with Commander, VS Code Light, and a theme with different terminal/app backgrounds. Confirm exposed resize areas use the app background; switch themes and reopen secondary/panel windows. Also cold-start with a saved light theme. Rust does not hot-reload. Regression tests cover native window/webview color requests for all bundled themes and saved-theme resolution; actual native paint remains unverified. A headless Chrome startup-document screenshot attempt timed out after 50 seconds without an image. No TUIC instance was launched, restarted or killed by this peer. _(audit 2026-10-13: HUMAN — Native paint, resize.)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: native resize paint and secondary windows require observation.)_
## Automation precheck context editor (1624-c5f5)

## Core provider accounts (1679-1419) — Rust restart required

- [ ] After landing and Boss's manual `make dev` restart or `make build`, list accounts through `POST /accounts/action`; confirm existing GitHub IDs and repo bindings, plus presence-only credential status. OAuth success replies must omit `access_token`. No app was launched, restarted or killed by this peer. _(audit 2026-10-13: HUMAN — List would probe credential presence (keychain risk); HTTP auth blocked; add flow needs a token. rb not run for accounts.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
- [ ] After A2/A3 integration, use the private native add flow for a registered Jira/GitLab token provider, Test it, then remove a throwaway account/site. A1 tests the generic descriptor protocol on rb; live provider auth and native secret-entry privacy are not claimed. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — List would probe credential presence (keychain risk); HTTP auth blocked; add flow needs a token. rb not run for accounts.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_

## iPhone hands-free Now Playing (1670-fdc8)

- [HUMAN] After landing and reloading the iPhone PWA, arm hands-free and lock the phone. Confirm the title is **TUICommander hands-free**; Pause holds the spoken reply and Play resumes at its position while the microphone stays active. Stop (if iOS exposes it) ends capture and drops queued replies. Also stop from the PWA and confirm controls no longer act on the ended session. Check a reply still uses media volume with the microphone open, then re-arm. Browser boundary tests cover registration, HTTP action routing and cleanup; physical iPhone audio/lock-screen behavior is unverified. No Rust change or backend restart is required. _(audit 2026-10-13: HUMAN — iPhone.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Saved AI Chat tabs (1628-4459)

## Terminal paste text precedence

- [ ] After loading the updated frontend, copy a text line by selection in terminal Chat and paste into Claude Code; confirm text appears rather than an image attachment. Capture the paste event's MIME types to identify the original WebView clipboard payload. Also check a screenshot-only paste and a Finder image copy still attach. Component tests cover the mixed text/image precedence; this peer did not access Boss's running WebView or clipboard. _(audit 2026-10-13: HUMAN — needs real WebView clipboard / Finder image copy)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
## Scrollbar history marks (1665-ce9b)

- [ ] After landing, compare a live terminal with recorded command/prompt metadata against `~/Gits/.tmp/results/scrollbar-marks-1665.png`. The screenshot uses the production marks renderer with representative metadata; it does not prove the affected live terminal has prompt metadata. No Rust restart is required. _(audit 2026-10-13: HUMAN — visual compare of live terminal (2 sub-items already [x]))_ _(triage 2026-10-14: WAITS — actual live-terminal visual/metadata comparison requires native screen capture permission from Boss. Production renderer/geometry fixtures passed; MacControl capture is denied, and the documented `__TUIC__.terminal` projection omits commandBlocks/userPromptLines.)_
## MCP upstream secret headers (1657-e1a4) — Rust restart required

## Mobile hands-free composer ownership (1662-c549)

- [ ] After loading the updated PWA frontend, dictate during a busy Claude turn, including after a dialog/draft hold and while interrupting speech. Confirm the PTY queues the phrase once and the mobile composer keeps only local typing. Toggle spoken-reply mute mid-phrase and confirm the same behavior. Real microphone, speaker, and Safari PWA timing need a device check; component tests cover unsolicited, delayed, and reconnect input echoes plus keyboard/keybar Tab and editable Up/Down history recall. No Rust production change or backend restart is needed for this fix. _(audit 2026-10-13: HUMAN — mic/speaker/Safari PWA device)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Once automation dispatch (1615-57cf) — Rust restart required

## Mobile Claude dot pulse (1654-a3d5)

- [ ] On an actual iPhone and iPad installed Safari PWA, run a long Claude Bash tool call; confirm grey ON/OFF pulses and green completion preserve text position and continuation indentation. Real OutputView browser geometry passes in Chromium at 390×844 and 1024×1366; device emulation does not prove Safari fonts or touch behavior. _(audit 2026-10-13: HUMAN — real iPhone/iPad Safari)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Tablet desktop switch (1658-fb34)

- [ ] [HUMAN] On the already-installed iPad PWA, confirm the updated manifest scope takes effect and both switches stay in the installed app window. Browser emulation proves routing and preference handling but cannot prove iOS updates an existing installation's manifest. _(audit 2026-10-13: HUMAN — first item already [x]; [HUMAN] iPad PWA manifest)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Crate diet stage B (1648-2c54) — Rust restart required

- [ ] After landing and Boss's planned `make dev` restart or `make build`, confirm relay traffic still reaches a paired browser and a Web Push reaches an existing subscription with the stored VAPID key. Fixed vectors cover relay/HKDF bytes and RFC8291 ciphertext; independent verifiers cover VAPID signatures. Rust does not hot-reload; this peer does not restart desktop. _(audit 2026-10-13: HUMAN — needs paired browser relay + Web Push subscription)_ _(triage 2026-10-14: WAITS — Needs a paired relay browser and existing real Web Push subscription.)_
## Crate diet stage A (1640-ecee) — Rust restart required

- [ ] After landing and Boss's planned `make dev` restart or `make build`, confirm CUID2 generation still returns 24 lowercase base36 characters starting with a letter, and an upstream OAuth sign-in completes with PKCE S256. Rust changes require a manual restart to load; this peer does not restart desktop. _(audit 2026-10-13: PARTIAL — CUID2 PASS: 8 live `generate_value{type:cuid2}` via Tauri invoke all matched `^[a-z][0-9a-z]{23}$` (r57bhcn7wlqzid68qdybl8w3, …). Upstream OAuth PKCE S256 sign-in = HUMAN (login))_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
## Automation prechecks (1614-7e8f) — Rust restart required

## Plugin host phase 0 — backend restart required

- [ ] After Boss's planned `make dev` restart or `make build`, verify browser Add account creates a named GitHub account while the default login stays unchanged. Also verify a plugin declaring `net:http` with no `allowedUrls` is rejected. The Rust HTTP handler and manifest validator are not hot-reloaded. No desktop instance was launched by the peer. _(audit 2026-10-13: PARTIAL — validator PASS: rb (RB_REF=ed776d323) `cargo nextest run --lib -E test(loading_http_manifest_without_allowed_urls_fails)` 1 passed (plugins.rs:338); check at plugins.rs:338-340…)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
## Dependency lane (1584–1589, 1587) — backend rebuild required

- [ ] Load the updated Rust dependencies with Boss's next planned `make dev` restart or `make build`; Rust does not hot-reload. After loading, check native folder dialogs, notifications and updater availability. Targeted git/OAuth2/push/relay tests, the desktop library check and frontend build passed in the worktree; no desktop instance was launched. WebRTC still needs coordinator-owned rb cross-platform verification, and crypto needs critic review before landing. _(audit 2026-10-13: HUMAN — native folder dialogs, notifications, updater availability need GUI; app boots and serves MCP on the new deps)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
## Remote terminal metadata (1581-8fd0) — Rust restart required

- [ ] After Boss chooses to rebuild/restart the backend and update the Mint daemon, run Claude in a remote shell: verify agent icon, idle/busy transitions and OSC tab title updates; custom names remain protected. Compare observed streaming with the loopback baseline (grid marker delay 4–16 ms; text/log batches about 200 ms). No Mint latency improvement is claimed by the loopback probe. _(audit 2026-10-13: HUMAN — needs Mint daemon update + Claude in a remote shell)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
## iPad terminal keyboard focus (1577-a9bf)

- [ ] [HUMAN] On the physical iPad Safari remote UI, tap the bottom Claude prompt and a higher row after dismissing the software keyboard; confirm it stays open and text/deletion work. Primary canvas presses now cancel the proven native blur/refocus cycle. The reported top-bar blur still has no proven cause; record whether it appears before keyboard opening, after opening, or after pinch zoom. _(audit 2026-10-13: HUMAN — physical iPad)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_

## PTY identity reattachment (1673-da49) — Rust restart required

- [ ] On a physical phone, confirm children nest under the coordinator in the mobile session list. _(verified 2026-10-14: real coordinator row publishes stable `tuic_session` 4f0ac9a9… and own `agent register` returns `terminal:true`; the recorded live parent/child UUIDs resolve correctly. The mobile reconcile/screen test nests all nine recorded children under COORDINATOR. Evidence: `~/Gits/.tmp/results/to-test-1014/phase2-identity-executable-proof.json`.)_ _(triage 2026-10-14: WAITS — Needs Boss’s physical phone for the remaining device layout check; src/mobile/AGENTS.md forbids treating desktop/component rendering as phone proof.)_

- [ ] #1669-bca7: On the affected phone, save any draft and use the mobile update action, then dictate to the coordinator with an empty mobile composer. Confirm the turn reaches the terminal once and leaves the mobile composer empty. Live logs showed an older mobile client (`c0cb1d193`) during the report; its exact device identity and cached/network version response could not be inspected. The current served bundle already includes #1662; no Rust change or app restart is required by this investigation. _(audit 2026-10-13: PARTIAL — `session list`: COORDINATOR row session_id f264213c… publishes tuic_session 4f0ac9a9…; `agent register` for totest-B returned `terminal:true`; list_peers shows peers with session_id/alias…)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Hands-free phrase boundaries (1656-eb5e) — Rust restart required

- [ ] After Boss's planned `make dev` restart or `make build`, hold a hands-free turn behind a draft or permission dialog, speak two phrases, then clear the hold. Confirm the phrases reach the agent on separate lines as one submission. The backend does not hot-reload; this peer does not restart desktop. _(audit 2026-10-13: HUMAN — needs voice/mic and a draft hold)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Terminal Chat presentation (1576-6320)

- [ ] After landing and loading the frontend, reopen Boss's Release Fixes and COORDINATOR conversations to compare the live transcript with the recorded screenshots. The standalone preview does not prove integration with the installed release candidate. _(NOT VERIFIED 2026-10-12: needs the web frontend served and driven in a browser; the rb build box has no browser or built frontend and the lane builds Rust only through rb, so no UI was rendered. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — visual compare of Boss's conversations; backend snapshot works (see 56))_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
## Claude Chat recorded transcripts (1568-7b55) — Rust restart required

- [ ] After Boss's next planned `make dev` restart or `make build`, open Chat on a bound Claude terminal: confirm older prompt-ID prompts appear, command echoes remain absent, media tool results show markers and model changes show a card. The real-record parser tests cover these projections; the live backend still requires a manual restart because Rust does not hot-reload. This lane does not launch or restart a desktop instance. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: PARTIAL — live `chat_view_snapshot` on this bound Claude session: 159 updates (user_message_chunk 1, agent_message_chunk, tool_call, tool_call_update), 0 malformed, 0 unknown rows…)_ _(triage 2026-10-14: WAITS — Boss planned native rebuild and bound Claude transcript visual check; recorded parser and live snapshot evidence remain partial.)_
## Mobile Markdown review (1569-1a1e) — Rust restart required

- [ ] [VISUAL] After the next `make dev` restart or rebuild, open a Markdown file with `- [ ]` items in the mobile PWA Files view: tap each checkbox (also just beside it) and confirm the file on disk toggles; tap a paragraph, press Comment, save, and confirm the desktop Markdown tab shows the highlight and comment. Edit the file on the desktop between opening and tapping on the phone and confirm the notice and reload appear with no overwrite. Take a mobile screenshot of the bottom comment bar. Rust does not hot-reload; no desktop instance was launched by this lane. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — phone/PWA + screenshot)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Paged MCP terminal output (1551-5fa4) — Rust restart required

## Progress blocked badge supersede (1537-6c4b) — Rust restart required

## AI Chat provider retry line (1536-1c9c, ego 288-f994)

- [ ] [VISUAL] After ego `fix/288-provider-retry` is merged and installed, cause an overloaded or 503 provider (or wait for a real one) in AI Chat and take a screenshot: one orange line `… — connection problem, retrying in Ns (attempt n/6)` that updates in place, disappears when the answer starts, and gives way to the turn error after 6/6, with the next prompt accepted. Frontend only (HMR); ego must be rebuilt. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — needs ego + 503 provider + screenshot)_ _(triage 2026-10-14: WAITS — Needs the installed ego retry build and a recorded overloaded provider response.)_
## Windows CI remaining regressions (1518-d3a7) — Rust restart required

- [ ] After Boss rebuilds/restarts the backend, confirm Windows archive hooks can run Git and short-name uploads publish or skip an existing destination without replacement. Native Windows CI owns the automated proof. No desktop instance was launched; Rust changes do not hot-reload. _(audit 2026-10-13: HUMAN — Windows only)_ _(triage 2026-10-14: WAITS — Needs the named native platform/CI runtime; this machine is macOS.)_
## Telegram channel adapter (1438-79b4) — headless rebuild required

- [ ] After Boss authorizes mint deployment and the bound agent is idle, rebuild/restart tuic-remote and verify registration/replacement/unregister, automatic retirement on agent exit with the shell still open, MCP session end and PTY close, and the allowlisted phone conversation: no-agent reply "Nessun agent registrato", Thinking/activity refresh, exact final reply, authored done/blocked notices and opaque-button callback mail. The Rust changes do not hot-reload. No live token/chat-ID reads, mint deployment or desktop restart were performed by this lane. Offline targeted tests use fake credentials and chats; deterministic publish receipts remain outside this slice. _(audit 2026-10-13: HUMAN — needs token/phone; note: peer `telegram-adapter` is registered in list_peers)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
## Optional dictation build graph (1394-ff2c) — Rust restart required

- [ ] After Boss restarts `make dev` or installs a rebuilt release, confirm push-to-talk, hands-free speech, notification output-device selection, and the global window hotkey still work. Tauri builds enable dictation explicitly; plain Cargo desktop builds omit it. Rust changes do not hot-reload. This lane does not restart the live app or launch another desktop instance. _(audit 2026-10-13: HUMAN — PTT/hands-free/device/hotkey hardware)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Windows Clippy cleanup (1501-e8cb) — rebuild required

- [ ] After the next Windows rebuild, confirm agent executable discovery still prefers `.exe` over `.cmd`, Chrome registry discovery works, and an upload completes. The Rust syntax cleanup does not hot-reload into the running desktop; Boss controls the next restart. _(audit 2026-10-13: HUMAN — Windows only)_ _(triage 2026-10-14: WAITS — Needs the named native platform/CI runtime; this machine is macOS.)_
## HTTP API origin boundary (1456-351c) — Rust restart required

- [ ] After Boss restarts `make dev` or installs a rebuilt release, confirm existing CLI/MCP Unix socket access, authenticated phone/PWA access and desktop remote peer access. Rust does not hot-reload; no desktop instance was launched by this lane. Foreign Origin/Host rejection and token-authenticated clients are covered by targeted router regressions. _(PARTIAL 2026-10-12: matrix on the TCP listener: cookie or `?token` without Origin 200; same-host Origin 200; foreign Origin 403 "Untrusted Origin"; foreign Host 403 "Untrusted Host"; no auth 401, no auth with foreign Origin 403. Unix socket: MCP session opens and `GET /sessions` answers 200 without a cookie. Desktop remote peer access was not run (the headless daemon has no remote client). Evidence `~/Gits/.tmp/run-1706/b1.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: PARTIAL — live :9876: foreign Origin no-auth 403 "Untrusted Origin"; foreign Host 403 "Untrusted Host"; no auth 401; Unix socket `tuic ls --json` 200 list…)_ _(triage 2026-10-14: WAITS — Boss authenticated phone/PWA and desktop remote-peer check after native rebuild; HTTP origin/auth matrix and Unix-socket CLI access already proven.)_
## Release-check lint cleanup (1447-a894) — Rust rebuild required

- [ ] After the next backend rebuild, verify capture/resampling, echo cleanup, loudness and Edge speech still work. These lint-only edits do not hot-reload; existing automated regressions need a final run after the managed background launcher is restored. Do not restart the live desktop from this lane. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — needs audio capture/Edge speech)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Remote hand-launched agent detection (1420-f3de) — Rust restart required

- [ ] After Boss approves and loads the rebuilt desktop and remote daemon, use the configured Mac-mint connection through desktop MCP only: create a disposable shell PTY, start Claude by hand, confirm `agent_state` appears, submit one task after its composer is ready, send mail with a payload-free wake and read the reply. Check that a plain shell rejects submit with a cause and a corrective action. Do not restart or redeploy Mac-mint while its live PTYs must be preserved. Coordinator harness: `scripts/test-remote-mcp.py` from story 1419. _(audit 2026-10-13: HUMAN — Mac-mint; list_peers shows connection error "daemon must support /mcp/peer" (remote daemon not redeployed))_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
## Suspend Tab (1358-d008) — desktop menu

## Global AI Chat (1157-1e54)

- [ ] After a `make dev` restart, open AI Chat and switch between three repositories: `ps` shows no new `ego acp` process and the tabs stay. Send a message: exactly one `ego acp -C ~/Gits` starts, and ego's answer knows which repository was on screen. Reload the webview and send again: still one process. Quit TUICommander: no `ego acp` survives. _(NOT VERIFIED 2026-09-29: Needs real ego binary (`ego acp`) and a live AI Chat agent conversation.)_ _(audit 2026-10-13: HUMAN — needs driving live AI Chat with real ego (ego 0.1.0 installed, no `ego acp` running now))_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
- [ ] After a Rust restart, open the same AI Chat peer simultaneously from desktop and a remote browser in an isolated instance; both views must attach to one `ego acp` process and show the same conversation. _(NOT VERIFIED 2026-09-29: Needs a real `ego acp` process (real ego binary/account); dual attach from desktop+browser cannot be done with a fake.)_ _(audit 2026-10-13: HUMAN — needs driving live AI Chat with real ego (ego 0.1.0 installed, no `ego acp` running now))_ _(triage 2026-10-14: WAITS — Needs Boss native desktop/browser dual-view check after rebuild; per-peer process reuse source and UI adoption tests verified. Evidence: phase2-acp-replay-capabilities-proof.json)_
## ego MCP over ACP (1156-1b61)

- [ ] After a `make dev` restart and an ego build that advertises `mcpCapabilities.acp`, open AI Chat in an isolated `TUIC_APP_INSTANCE=<id>` and ask ego to list terminals. `ps` shows no `tuic-bridge` child of ego, the app log shows no `MCP initialize` line per tool call, and the tool answers. _(NOT VERIFIED 2026-09-29: needs a real ego agent (ACP) session; not available headless)_ _(audit 2026-10-13: HUMAN — same)_ _(triage 2026-10-14: WAITS — Needs Boss live ego tool turn plus native process/log observation after rebuild; installed ego ACP capability handshake verified. Evidence: phase2-acp-replay-capabilities-proof.json)_
## AI Chat replay storm (1151-4243)

## Crate split restart

- [ ] Restart `make dev` after the `tuic-terminal`, `tuic-core`, `tuic-git` (including its GitHub domain), and `tuic-dictation` crate splits. Rust changes do not hot-reload in the running development instance. After the restart, use an isolated `TUIC_APP_INSTANCE=<id>` to check GitHub PR status and CI notifications, push-to-talk transcription, and one hands-free spoken reply with real audio; the current live backend still has the previous crate layout. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Restart-of-make-dev item; instance is a headless tuic-remote of HEAD bef15c20f. Backend crate-split behaviour exercised across r0 items (sessions, worktrees, git); UI part needs desktop build.)_ _(audit 2026-10-13: HUMAN — real audio + GitHub token; app up on the split build)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Ego notice cards (1075-05dc)

- [ ] After Boss rebuilds the desktop instance, confirm a real ego worker completion publishes an out-of-turn notice and **Open result** opens its file in a TUIC tab. _(verified 2026-10-14: production Transcript/reducer browser check shows three separate bordered cards, one action each, plain replies outside cards, exact result-path callback, and Answer/Approve scrolling to the open interaction; `notice-browser-proof.json`, `notice-cards.png`. Three narrow reducer tests pass in `notice-contracts2.log`; actual panel result-path resolution/openFile contract passed in `core-ui-tests.log`. Controlled metadata does not prove spontaneous publication by installed ego or native tab opening.)_ _(triage 2026-10-14: WAITS — Boss native rebuild and real ego notice publication; no desktop instance may be launched by this peer.)_

<!-- tweak-comments v1: inline review comments.
     Format: [tweak:begin:ID]highlighted text[tweak:end:ID @ISO-TIMESTAMP
     comment body (free text, may span multiple lines)
     ] — where [ ] are the HTML comment delimiters <!-- -->.
     The only escape is '-->' → '--&gt;' inside the comment body.
     Read each comment, apply the feedback to the highlighted text,
     then remove the tweak markers. -->
## Stable macOS dev executable (1510-03ae) — next Boss launch

- [ ] With ALF enabled, confirm LAN/tailnet HTTP requests reach the running desktop and its page works on the physical iPhone. _(verified 2026-10-14: PID 12511 maps the existing `~/Library/Application Support/com.tuic.commander/dev-bin/tuicommander`; adjacent `tuic`, `tuic-bridge` and `tuic-remote` exist. Stable copy runner and critic tests, including target deletion, plus the sibling-build ordering check passed. The printed startup path is source/test verified; ALF reports enabled. Evidence: `~/Gits/.tmp/results/to-test-1014/phase2-identity-executable-proof.json`.)_ _(triage 2026-10-14: WAITS — Needs a physical iPhone and off-host LAN/tailnet request to validate the firewall boundary; localhost and script tests cannot prove it.)_
## Remote file drops (1434-1719) — Rust restart required

- [ ] After Boss restarts the desktop and updates the remote daemon, drop a Mac file and a folder onto a registered remote repository in tree and flat views. Verify remote bytes, unchanged local sources, directory confirmation and conflict skipping. This native Finder-to-Tauri interaction requires the desktop rebuild; no second desktop instance was launched. _(audit 2026-10-13: HUMAN — Finder drag)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
## Safe orphan cleanup countdown (story 1257-a30b) — Rust restart required

## Queued Claude notice confirmation (story 1251-9ec8) — Rust restart required

## Shared agent mail identity (story 1246-46e3) — Rust restart required

## Managed child idle close (story 1209-cc47) — Rust restart required

## Native desktop notifications — Rust restart required

- [ ] [HUMAN] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, leave TUICommander unfocused and trigger an agent question and a Progress `done` entry. Confirm each appears once in macOS Notification Center, with the terminal or project name; clicking each brings TUICommander to the named terminal or Progress project. Confirm a focused window produces none. This requires real cross-app focus and Notification Center; the running Rust backend cannot load the native handler without restart. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: macOS Notification Center with real cross-app focus needs desktop app (headless tuic-remote has no native notification handler); not automatable here.)_ _(audit 2026-10-13: HUMAN — Notification Center; second item duplicates 89)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
## Mobile Files and editor (story 1225-60e7)

- [ ] On a real 360×800 phone PWA, verify Files long-press touch behavior, deep-path final folder and trailing slash visibility, 44px View/Edit header targets, and editor space above the bottom bar. Functional path dialog, folder ordering and nested search tests pass. _(verified 2026-10-14: phase2-mobile-functional-proof.json; mobile-contracts.log, 17 selected tests passed)_ _(triage 2026-10-14: WAITS — Real phone layout/touch and terminal row count require Boss; src/mobile/AGENTS.md forbids treating desktop/component evidence as phone proof.)_
## Mobile session header (story 1203-fe36)

- [ ] [HUMAN] After the frontend reloads, open Codex and Claude sessions on a 360×800 phone. Confirm each shows the correct 24 px logo and state dot, the display name remains readable, and the 56 px header leaves the terminal starting near y58 with no lost rows. Tap the name, Tasks, and overflow Progress to inspect the temporary sheets; check Files, Search, Ideas, Commands, and terminate remain reachable from overflow. Automated component tests cover the button actions and session binding; a real phone check remains. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — to-test.md:386; Real phone/PWA cross-device check explicitly requires iPhone; forbidden by brief.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## AI Chat prompt parking (story 1228-becb)

- [ ] On a real 360×800 phone PWA, verify parked drafts with an image preview and after a page reload, then compare the visible terminal row count after switching to Sessions. Mobile Park text and uploaded-document restoration tests pass. _(verified 2026-10-14: phase2-mobile-functional-proof.json; mobile-contracts.log, 17 selected tests passed)_ _(triage 2026-10-14: WAITS — Real phone layout/touch and terminal row count require Boss; src/mobile/AGENTS.md forbids treating desktop/component evidence as phone proof.)_
## Mobile global AI Chat (story 1208-b371)

- [ ] On the real phone PWA, compare titled global conversations and push-linked selection with desktop, then confirm the same visible terminal row count after switching to a session. Global-chat, no-repository-picker and push-linked navigation tests pass. _(verified 2026-10-14: phase2-mobile-functional-proof.json; mobile-contracts.log, 17 selected tests passed)_ _(triage 2026-10-14: WAITS — Real phone layout/touch and terminal row count require Boss; src/mobile/AGENTS.md forbids treating desktop/component evidence as phone proof.)_
## Mobile terminal states (story 1211-e1f4)

- [ ] On a real 360×800 phone PWA, verify blue working, green idle, orange awaiting-input and purple completed-unseen status colors; opening a completed session must show Idle without reducing visible terminal rows. Status label and completion-seen transition tests pass. _(verified 2026-10-14: phase2-mobile-functional-proof.json; mobile-contracts.log, 17 selected tests passed)_ _(triage 2026-10-14: WAITS — Real phone layout/touch and terminal row count require Boss; src/mobile/AGENTS.md forbids treating desktop/component evidence as phone proof.)_
## Dictation Metal release link (story 1198-535b) — Rust rebuild required

- [ ] After rebuilding `make dev`, verify macOS dictation starts with a downloaded Whisper model and still uses Metal. The build script change cannot affect Boss's running backend until a rebuild and restart; the targeted release test covers linking and loudness timing. _(NOT VERIFIED 2026-09-30: blocked — Needs downloaded Whisper model, microphone audio and Metal GPU use on desktop build (audio hardware blocked).)_ _(audit 2026-10-13: HUMAN — to-test.md:402; Real microphone / Fn / Metal GPU capture requires hardware.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Mobile slash commands (story 1199-7b8d)

- [ ] On the phone PWA, open disposable Claude Code and Codex sessions. Type a supported slash command (`/help` in Claude, `/status` in Codex) and press Send; confirm it opens once. In each session, type a slash prefix, choose that command from the suggested slash menu, then press Send; confirm it opens once. In Claude, submit `/model` with an argument and confirm the argument reaches the command. Confirm the Codex quick-command widget offers `/status` and that it opens Status. The current `make dev` frontend must reload the new bundle before this check. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — to-test.md:406; Real phone/PWA cross-device check explicitly requires iPhone; forbidden by brief.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Queued agent submission confirmation (story 1163-5bed) — Rust restart required

- [ ] After restarting `make dev` with an isolated `TUIC_APP_INSTANCE`, queue a short command for a disposable Codex session while it is busy. When it becomes ready, confirm that the command starts a turn. If the composer retains the text instead, confirm that TUICommander reports uncertain delivery with an error toast and `session status` shows `delivery_uncertain=true`. Repeat with the installed Claude, OpenCode, Goose, Grok, and pi binaries. The running backend does not load this Rust change until restart. _(FAILED 2026-09-30 story 1299-3ce1: Claude, Codex, grok, pi: busy-queue then delivered turn OK; Codex silent composer: 6.29s, delivery_uncertain=true, text retained. OpenCode (--mini): initial prompt and queued cmds never drained, state stuck, prompt_delivery_failed. Goose: text retained with uncertain=false, queue stuck. Toast unveri)_ _(audit 2026-10-13: HUMAN — Queue is HTTP `POST /sessions/{id}/queue` / frontend IPC only; port 9876 answers 401 (Basic Auth, no credentials by rule). MCP `session submit` never queues.)_ _(triage 2026-10-14: WAITS — Recorded Codex/Claude/OpenCode queue acceptance and uncertain-delivery tests passed earlier; fresh installed-vendor busy/ready conversation and toast remain unverified. Owned rb runtime is unavailable; no Boss composer used.)_ _(verified 2026-10-14: phase2-queued-input-proof.json; five targeted rb tests pass, including captured OpenCode drain and silent-agent uncertainty/error toast. Story 1299-3ce1 fixes OpenCode; 1338-7cf3 was merged into 1106-e81a, whose live Goose/Codex criteria were explicitly deferred to this tracker. New installed-vendor turn and native toast appearance remain unverified.)_
## Concurrent config saves (story 900-43dd) — Rust restart required

- [ ] With two independently hydrated settings windows, disable Gemini in A, then change only the IDE in B; both preferences must persist after reopening. _(FAILED 2026-10-14: actual production settingsStore/updateAppConfig in independent browser frames submits B base.disabled_agents=["gemini"] but config.disabled_agents=[] on an IDE-only edit. Backend delta application would explicitly re-enable Gemini. Evidence: ~/Gits/.tmp/results/to-test-1014/config-windows-proof.json and config-windows-browser2.log; second save captured only, no live config mutation. Follow-up story 1876-ceee (historical story 900-43dd).)_ _(triage 2026-10-14: WAITS — Reproduced failure and linked story retained; regression verification awaits an integrated fixed build.)_
## Current MCP tool results (story 1190-75eb) — Rust restart required

## ACP peer mail receipt (1176-e82e) — Rust restart required

## Push-to-talk sustained speech (story 1135-b600) — Rust restart required

- [ ] [HUMAN] After restarting `make dev` with an isolated `TUIC_APP_INSTANCE`, record a short noise burst, ordinary speech, and quiet genuine speech with push-to-talk. Confirm that only sustained speech reaches the prompt and that a rejected capture shows `no sustained speech` in the dictation ring. The synthetic command tests cover duration and threshold boundaries; the real microphone separation remains unverified and is tracked by story 1117. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: push-to-talk with real microphone speech (audio hardware blocked).)_ _(audit 2026-10-13: HUMAN — to-test.md:448; Real microphone / Fn / Metal GPU capture requires hardware.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Push-to-talk final skip reason (story 1140-28e3) — Rust restart required

- [ ] [HUMAN] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, use a microphone capture that triggers a final RMS or Whisper speech gate. Confirm the dictation status and ring show the specific gate reason. An empty successful transcript still shows `no speech detected`. The focused Rust command tests prove response mapping without a microphone; live audio remains unverified. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: needs microphone capture to trigger RMS/Whisper gates (audio hardware blocked).)_ _(audit 2026-10-13: HUMAN — to-test.md:452; Real microphone / Fn / Metal GPU capture requires hardware.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## AI Chat persistent approval (story 1147-33ec)

## AI Chat layout and composer (story 1166-ef2f)

- [ ] In an isolated AI Chat conversation, confirm the tool count and status remain on one line at the panel's normal width, raw shell commands appear only after expanding a call, and Copy has room in both message types. While at the bottom, stream an answer and confirm the typing dots stay visible; scroll up and confirm the view stays put. Paste over 200 words and an image, then confirm the compact marker expands to the full prompt on Send and the image preview is removable. Targeted component tests cover these behaviors; the mandated stealth browser wrapper timed out on screenshot and snapshot commands for this worktree fixture. _(NOT VERIFIED 2026-09-29: Needs a real ego (ACP) AI Chat session streaming an answer; stealth browser wrapper reportedly unreliable)_ _(audit 2026-10-13: VISUAL-OWED — Targeted panel tests passed layout anchors, bottom follow, typing indicator, >200 word compact paste and image removal. AIChatPanel.test.tsx:440,483,515,1829,2198…)_ _(triage 2026-10-14: WAITS — Reproduced failure and linked story retained; regression verification awaits an integrated fixed build.)_ _(FAILED 2026-10-14: user Copy wraps vertically and clips at500px panel; tool row, streaming follow/read position,201-word paste and image removal pass. See [1166-ef2f](stories/archive/1166-ef2f-complete-P2-ai-chat-polish-tool-call-row-layout-copy-button-sp.md); evidence: ~/Gits/.tmp/results/to-test-1014/phase2-chat-layout-failure.json.)_
## CLI sidecar replacement (story 1165-e905) — Rust restart required

- [ ] After a manual `make dev` restart when current sessions may be discarded, install or update `tuic` from Settings in an isolated `TUIC_APP_INSTANCE`. Confirm `tuic --version` runs and a previously running CLI process is unaffected. The live backend cannot load the Rust installer change until restart; fixture tests cover replacement through hard links and symlinks. _(NOTE 2026-09-29: partial evidence only — Installer replacement covered by fixture tests (hard links/symlinks) per item; live install would overwrite Boss's installed /usr/local/bin/tuic, so not done from an isolated instance.)_ _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Settings CLI install is UI; /usr/local/bin/tuic (Sep 29 build) runs and talks to the instance socket, but the sidecar replacement flow was not exercised.)_ _(audit 2026-10-13: HUMAN — Settings install overwrites Boss's /usr/local/bin/tuic.)_ _(triage 2026-10-14: WAITS — Boss native Settings install updates the shared /usr/local/bin/tuic; an isolated app instance does not isolate that installed executable. Existing replacement fixture tests do not prove this native update.)_
## AI Chat message Copy and trailing suggestions (story 1150-4042)

## AI Chat failed turns and choice buttons (story 1139-7310) — Rust restart required

## AI Chat shared ACP prompt queue (story 1079-fe88) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, open the same disposable ego conversation on desktop and phone. Start a long desktop turn, queue a phone prompt, and confirm both views show it. Cancel a queued item from desktop and confirm it disappears from phone without reaching ego; queue another, stop the running turn from phone, and confirm desktop shows cancellation and the queued prompt starts only after the ACP response. Pause a turn with a prompt queued; confirm it stays queued until Resume and remains cancellable from either view. The live backend cannot load this Rust change until restart; targeted Rust fixture and frontend tests cover the protocol and rendering paths. _(NOT VERIFIED 2026-09-30: blocked — Needs a real phone (PWA) plus ego ACP.)_ _(audit 2026-10-13: HUMAN — to-test.md:478; Real phone/PWA cross-device check explicitly requires iPhone; forbidden by brief.)_ _(triage 2026-10-14: WAITS — Needs an actual phone plus desktop ACP conversation; protocol/rendering evidence does not prove cross-device live interaction.)_
## ACP ego peer identity (story 1073-3431) — Rust restart required

- [ ] After the next planned native rebuild/restart, confirm AI Chat appears in `agent list_peers` as an ego peer without a terminal. Reconnect and restart, then send fresh mail to its restored UUID and read it with `agent wait`. _(NOTE 2026-10-14: the peer address persists; the in-memory inbox does not promise to preserve old mail across an app restart.)_ _(verified: phase2-acp-peer-identity-proof.json; UUID reload and nonterminal wait tests pass)_ _(triage 2026-10-14: WAITS — Boss native ego restore/list_peers/new-mail observation; this lane cannot launch or restart desktop.)_
- [ ] With phone push configured, submit blocked progress from an idle ego ACP conversation while away and verify the notification arrives on the real phone and opens that conversation. _(verified 2026-10-14: phase2-acp-peer-identity-proof.json; real child spawn has ACP peer parent_session_id and blocked progress event carries acpSessionId without PTY; three targeted tests pass)_ _(triage 2026-10-14: WAITS — Real phone Web Push delivery and native ego conversation require Boss; do not touch push credentials.)_
## AI Chat ACP session details (story 1072-6787)

## AI Chat ego profile (story 1074-9373) — Rust restart required

## AI Chat image paste (story 1085-fa65)

- [ ] [HUMAN] In an isolated desktop test instance with an image-capable ego connection, copy a PNG from another app and paste it into AI Chat. Confirm the thumbnail renders, can be removed, and an image-only submit reaches ego. Repeat with plain text paste. Targeted component/client tests prove the ACP block and guards; browser accessibility showed the thumbnail and controls, but Chrome's screenshot command timed out twice, so the visual result and real cross-app clipboard path remain unverified. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — to-test.md:499; Real cross-app clipboard path requires multi-app interaction.)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
## AI Chat conversation recovery (story 1071-46c9) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, open AI Chat with ego, send a turn, create a second conversation, then restart the app. Confirm the last conversation and its history return; select the older title in the newest-first picker and confirm its history appears once. The running Rust backend cannot load the new `AppConfig` field until restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: AI Chat conversation recovery is UI plus ego.)_ _(audit 2026-10-13: HUMAN — Full item explicitly requires app restart; hard limit forbids restarting Boss TUIC. Saved-load and picked replay tests 2468,2521 passed, not live restart recovery.)_ _(verified 2026-10-14: core-ui-tests.log covers saved tabs, newest-first title selection and one-time replay in a new document; phase2-acp-peer-identity-proof.json records scope)_ _(triage 2026-10-14: WAITS — Actual native ego conversation restoration after app restart requires Boss; this lane cannot launch/restart desktop.)_
## Windows Codex npm launcher — Rust restart required (story 987-c0ca)

- [ ] On a Windows build with npm's adjacent `codex` and `codex.cmd` shims, restart TUICommander and launch Codex from the agent menu. Confirm the help probe selects `codex.cmd`, reports `--no-alt-screen` support, and the new session stays on the primary screen. The targeted Rust test passed for the adjacent shims; native Windows execution remains to be checked. _(NOT VERIFIED 2026-09-30: blocked — Needs a Windows host with npm codex/codex.cmd shims (Windows desktop blocked).)_ _(audit 2026-10-13: HUMAN — to-test.md:507; Native Windows host and npm adjacent shims are unavailable on this macOS host.)_ _(triage 2026-10-14: WAITS — Needs the named native platform/CI runtime; this machine is macOS.)_
## Agent Enter gap (stories 974-254a, 975-1de1) — Rust restart required

- [ ] After restarting `make dev` in an isolated `TUIC_APP_INSTANCE`, send text plus `special_key=enter` to a disposable Claude MCP session and confirm it submits. Launch Codex through a wrapper that foreground detection does not recognize, then send a long prompt from a suggestion or dictation while the tab still has no agent type; confirm it submits and check app logs for one unknown-foreground warning. The current backend cannot load the Rust change until restart. _(NOT VERIFIED 2026-09-30: partial — Claude MCP session input text + special_key=enter submits (used in r0-cl4 run, turn started). Wrapper foreground-detection and log-warning not tested.)_ _(audit 2026-10-13: PARTIAL — (a) Claude MCP `input text + special_key=enter` in one call submitted (ENTEROK reply). (b) Codex launched through an unrecognised wrapper (sh script running codex as child): no agent_type…)_ _(verified 2026-10-14: three timing/framing contracts plus the null-agent foreground probe contract pass; the source guards the unknown-process warning once per session. Actual long Codex turn submission is still unproved; historical 0.159 evidence shows why delay tests alone are insufficient. Scope: ~/Gits/.tmp/results/to-test-1014/phase2-unknown-enter-proof.json.)_ _(triage 2026-10-14: WAITS — Needs a disposable unknown-wrapper Codex tab in Boss’s rebuilt native instance; this lane may not launch desktop TUIC. Verify actual submission and one emitted warning.)_
## Managed Codex wrapper trust (story 1047-c41c) — Rust restart required

## AI Chat pending ACP badge and notification (story 1070-38ce)

- [ ] After Boss's planned native rebuild, with AI Chat hidden and the desktop window unfocused, trigger a real ego permission request and confirm the AI Chat toggle badge and one native notification appear. Answer it and confirm the badge clears. _(NOTE 2026-10-14: an already-sent native notification cannot be withdrawn; `src/services/nativeNotifications.ts:12`. Settlement cancels only a notice still waiting for native permission, rather than closing an OS-delivered notice.)_ _(verified 2026-10-14: five targeted StatusBar/notifications contracts pass: hidden-panel pending count, visible-panel suppression, settlement clearing, native notifier despite WebView permission denial, once-per-interaction notification, cancellation during permission await, and focused-window suppression. Evidence `~/Gits/.tmp/results/to-test-1014/acp-attention-contracts.log`.)_ _(triage 2026-10-14: WAITS — real ego request plus native macOS notification appearance requires Boss's rebuilt desktop; this peer may not launch another desktop instance.)_
## AI Chat ACP pause settlement (story 1069-97bd) — Rust restart required

## Push-to-talk native release — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, hold Fn while dictating, then release it while the WebView is briefly busy or loses focus. The macOS microphone indicator must go off on release, the captured phrase must transcribe once, and the file log must show Fn down/up, native stop and IPC stop latency plus audio seconds and final/partial character counts. Repeat a recording longer than 30 seconds; its opening words must remain. The current live backend cannot load this Rust change without a restart. _(NOT VERIFIED 2026-09-30: blocked — Needs Fn key hold, macOS mic indicator and real microphone (audio hardware blocked).)_ _(audit 2026-10-13: HUMAN — to-test.md:557; Real microphone / Fn / Metal GPU capture requires hardware.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Claude awaiting badge — Rust restart required

## Queued agent command diagnostics — Rust restart required

## Mobile remote sessions, Progress, and Activity — Rust restart required

- [ ] [HUMAN] After Boss restarts `make dev` when current PTYs can be interrupted, verify `/api/version` identifies the integrated build, then open the Tailscale HTTPS `/mobile` PWA on a phone. _(NOT VERIFIED 2026-09-30: blocked — HUMAN item needing a real phone/Tailscale HTTPS PWA (real phone blocked). Only /api/version checked: {"version":"1.7.7","git_hash":"bef15c20f"} on rust0930 instance.)_ _(audit 2026-10-13: HUMAN — real phone + Tailscale HTTPS + second machine)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
- [ ] [HUMAN] A connected remote session should show live output through WebSocket; create and close only a throwaway remote session from mobile, then confirm it disappears on its owner. Disconnect that machine and confirm the stale session shows unavailable rather than a misleading local 404. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: needs a real phone (mobile PWA) and a second machine for the remote session; blocked (real phone / second physical machine).)_ _(audit 2026-10-13: HUMAN — real phone + Tailscale HTTPS + second machine)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
- [ ] [HUMAN] On the physical phone/PWA, open Progress with no desktop repository selected and confirm newest-project selection, project switching, saved done/blocked entries, and Activity showing active persisted events while dismissed entries stay hidden. _(verified 2026-10-14: relevant component/store contracts pass for newest-project selection and switching, Activity hydration, hidden dismissed entries and dismissal persistence. MobileApp.tsx:93 discovers projects independently of desktop selection. Evidence: ~/Gits/.tmp/results/to-test-1014/phase2-mobile-progress-files-proof.json.)_ _(triage 2026-10-14: WAITS — Physical phone/PWA layout and touch remain unverified; src/mobile/AGENTS.md forbids claiming device proof from component/browser tests.)_
## Progress toast dismissal (story 1061-7694) — after the fixed frontend loads

- [ ] [HUMAN] On the physical phone/PWA, tap a Progress toast body and confirm it closes without running its action. _(verified 2026-10-14: desktop Progress toast body dismissal preserves the active repository and terminal; Go to repo opens the reporting terminal in its worktree. The mobile component dismisses without navigating or running its dedicated action. Relevant tests pass; evidence: ~/Gits/.tmp/results/to-test-1014/phase2-mobile-progress-files-proof.json.)_ _(triage 2026-10-14: WAITS — Remaining phone touch behavior requires Boss’s physical device; desktop/component paths are verified.)_
## Embedded external links (story 989-63fa) — after Rust rebuild

- [ ] [HUMAN] After Boss’s planned native backend rebuild, click an HTTPS link in an HTML preview and inline plugin panel and confirm the system browser opens. From a cross-origin dashboard, attempt external navigation and confirm the blocked-link toast directs users to Open in Browser. _(verified 2026-10-14: seven scoped frontend tests pass for preview/SDK click interception, owning-frame host routing, URL-dashboard opener denial and fallback toast. Existing desktop-feature navigation predicate regression also passes on rb; source registers the tested guard and emits navigation-blocked. Evidence: ~/Gits/.tmp/results/to-test-1014/phase2-embedded-links-proof.json.)_ _(triage 2026-10-14: WAITS — Remaining native WebView-to-system-browser interaction needs Boss’s planned rebuild and cross-app observation; no desktop launch, system-browser open or credential access performed.)_
## Mobile repository files (story 1063-3fe1) — real phone

- [ ] [HUMAN] On a physical phone connected to an isolated test instance, browse a disposable repository, open Markdown View, edit and save with the phone keyboard, then confirm the content from desktop. Check plain text and large/binary refusal. _(verified 2026-10-14: component tests pass for repository/directory navigation, explicit text editing, Markdown View/Edit/Save/rendered View, and large/binary refusal. Storage is an in-memory transport fixture, so cross-device persistence is not claimed. Evidence: ~/Gits/.tmp/results/to-test-1014/phase2-mobile-progress-files-proof.json.)_ _(triage 2026-10-14: WAITS — Remaining physical-phone keyboard/touch and desktop readback require Boss’s device; src/mobile/AGENTS.md requires a real phone check.)_
## Mobile notification tags (story 1042-f5ca) — updated service worker

- [ ] [HUMAN] On a real subscribed phone after the updated service worker takes control, receive questions from two different sessions. Confirm both notifications stay visible and each opens its own session. Send another push for one session and confirm the other remains. The targeted service-worker test verifies tag replacement and both click deep links; the phone's notification UI requires real device verification. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — real subscribed phone)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Mobile completion push limit (story 1041-cdc7) — Rust, needs `make dev` restart

- [ ] [HUMAN] After restarting `make dev` when Boss is ready to end the current sessions, use a real subscribed phone while the desktop is away. Trigger a titled question followed immediately by session completion; confirm the phone displays one notification. Real phone delivery and display cannot be verified by the local HTTP push receiver. The targeted Rust test verifies accepted push requests, expiry after 30 seconds, and independent session limits. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: needs a real subscribed phone for push delivery (real phone blocked); instance has push.enabled=false.)_ _(audit 2026-10-13: HUMAN — real subscribed phone)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Managed agent workspace trust (2026-09-26) — Rust, needs `make dev` restart

## Remote repository browsing and terminal attach (stories 1025-8103, 1026-7633) — Rust restart and remote daemon update

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance and updating its test remote daemon, open the remote repository picker. Confirm it starts at the remote host's home directory, a denied directory shows a readable error while manual path entry and Up remain usable, and a terminal in a remote repository renders its prompt. Opening a terminal with a missing cwd must show a readable error instead of a blank pane; a failed grid stream must show an error toast. Use only disposable test connections and sessions; the live `make dev` backend cannot load the new home-directory route or cwd validation without a restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Remote repository picker is UI; needs remote daemon.)_ _(audit 2026-10-13: HUMAN — needs a remote daemon + saved connection (Boss's config))_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
## Claude question state after wrapped suggestions (story 1023-dcc9) — Rust, needs `make dev` restart

## Remote update and restart — Rust, needs `make dev` restart

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, use a disposable remote daemon to check the per-connection Auto-update option. Verify zero live sessions updates once through the daemon restart, live sessions show a manual offer with a count that refreshes while connected, and a failed update shows its error. During automatic transfer, confirm the manual update button is disabled; a stalled transfer eventually reports a timeout and resumes connection checks. Check the checkbox and status layout visually; the automated browser screenshot timed out. Do not use Boss's saved daemon. The running Rust backend cannot load this change until restart. _(NOT VERIFIED 2026-09-30: blocked — Needs a second physical machine (remote daemon update).)_ _(audit 2026-10-13: HUMAN — needs a second machine daemon (Direct + SSH))_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, connect a disposable Direct daemon and an SSH daemon, confirm the out-of-date badge and the exact live PTY count, then update each and verify reconnect with the new `/health.build.sha256`. The live backend cannot load this Rust change without a restart. Do not update Mac-mint or Boss's saved connections. _(NOT VERIFIED 2026-09-30: blocked — Needs a second physical machine (Direct and SSH daemons).)_ _(audit 2026-10-13: HUMAN — needs a second machine daemon (Direct + SSH))_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
## Squash-merged worktree removal (story 1022-8291) — Rust, needs `make dev` restart

## Claude usage per profile (story 1016-9cf8) — Rust, needs `make dev` restart

- [ ] On Boss's rebuilt macOS desktop, confirm two already-authenticated Claude profiles (default and a distinct `CLAUDE_CONFIG_DIR`) display their own actual account quota in the focused status badge and matching dashboard. _(verified 2026-10-14: five rb profile/environment/file-isolation/cache/backoff contracts pass; seven targeted frontend contracts prove focused-session polling and matching dashboard routing, unknown for unreadable/missing profile, stale quota clearing and late-response rejection. Evidence `~/Gits/.tmp/results/to-test-1014/claude-state-profile-contracts.log` and `claude-profile-focus.log`. Native macOS Keychain namespace tests are platform-gated on rb; no actual credentials read.)_ _(triage 2026-10-14: WAITS — real macOS account/Keychain quota display needs Boss; reading token stores or Keychain is forbidden to this peer.)_
## Hidden iframe unload (story 979-3d7f) — live instance required

- [ ] In a disposable test instance, open a URL tab pointing to a page that runs a 1 s busy loop every 5 s. Hide it by switching tabs, repositories, and pane groups, including a pinned tab and a split pane covered by an orphan tab. Verify `document.querySelectorAll('iframe[src="<test-url>"]').length === 0` while hidden and that showing the tab loads the same URL again. While hidden, run a read-only 30 s main-thread probe with 100 ms `setTimeout` ticks; require 0 gaps over 150 ms. Do not use Boss's live instance. _(Deferred: this requires the changed frontend in a running test instance; targeted Vitest proves DOM removal and remount.)_ _(NOT VERIFIED 2026-10-12: needs the web frontend served and driven in a browser; the rb build box has no browser or built frontend and the lane builds Rust only through rb, so no UI was rendered. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: PARTIAL — live app (Vite 1421): URL tab http://127.0.0.1:18761/busy.html (1s busy loop /5s): visible = 1 iframe; after opening another tab hidden = 0 `iframe[src=...]`…)_ _(triage 2026-10-14: WAITS — Synthetic busy-loop load is forbidden on this Mac; coordinator may verify on rb.)_

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
## Codex dictation auto-send (2026-09-25) — Rust, needs `make dev` restart

- [ ] [HUMAN] After restarting `make dev` when ready to end the current sessions, dictate a long phrase into a Codex tab with Auto-send enabled. Confirm it submits once rather than inserting a newline. Real microphone input and the Codex TUI are required for this final check. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: real microphone dictation plus Codex TUI (audio hardware blocked).)_ _(audit 2026-10-13: HUMAN — microphone + Codex TUI)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## File browser default (2026-09-25) — Rust, needs `make dev` restart

## Editor line wrapping (2026-09-25) — Vite hot reload

## Native WontFix dependency recovery (2026-09-25) — Rust, needs `make dev` restart

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, create a plan with a WontFix prerequisite and a Backlog dependent through the test instance. Confirm the dialog marks the prerequisite abandoned, offers Remove only on that direct cancelled edge, and reports the plan Active until its remaining stories are Done or WontFix. The current live backend cannot load this Rust change without a restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dialog for WontFix dependency is UI.)_ _(audit 2026-10-13: PARTIAL — `tuic story --project ~/Gits/.tmp/totest-E/proj`: A wont_fix -> plan_view B,C abandoned:true (direct+indirect), wontFixCount 1, state active, allCancelled false…)_ _(triage 2026-10-14: WAITS — Reproduced failure and linked story retained; regression verification awaits an integrated fixed build.)_ _(FAIL 2026-10-14: #1875-c3d9 (follow-up to #927-d482) — real backend status `wont_fix` mismatches UI `wontfix`: cancelled status label and Remove action are missing; #1875-c3d9 (follow-up to #937-69a4) — Active plan state is hidden. Actual dialog replay of captured isolated rb response; evidence `~/Gits/.tmp/results/to-test-1014/stories-indirect.png` and `stories-api-proof.json`. Archived-story CLI worklog rejected; coordinator recorded follow-up #1875-c3d9.)_
- [ ] After the native-stories backend rebuild, cancel a prerequisite with an indirect dependent and confirm the dialog shows both dependencies as abandoned, the Rust-supplied cancellation count, and the plan's Active state. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Cancellation dialog is UI.)_ _(audit 2026-10-13: PARTIAL — same run: B (direct) and C (indirect) abandoned:true, wontFixCount:1, state active. Dialog rendering not seen.)_ _(triage 2026-10-14: WAITS — Reproduced failure and linked story retained; regression verification awaits an integrated fixed build.)_ _(FAIL 2026-10-14: #1875-c3d9 (follow-up to #937-69a4) — real Rust count1 and both abandoned labels render, but Active plan state is not visible with stories present. Evidence `~/Gits/.tmp/results/to-test-1014/stories-indirect.png`, `stories-browser-outcomes.json`; archived-story CLI worklog rejected; coordinator recorded follow-up #1875-c3d9.)_
## Agent native scrollback (2026-09-25) — Rust, needs `make dev` restart

- [ ] On Windows, launch a throwaway agent whose `--help` child hangs and confirm no `cmd.exe` or `node.exe` descendant remains after the probe timeout. _(verified 2026-10-14: Linux rb executed the three deadline, once-per-binary-version/cache invalidation and descendant-reaping contracts; all passed. The production deadline is two seconds at agent.rs:999. Windows uses a suspended process assigned to a kill-on-close job at agent.rs:658-771, but this is source evidence only. Evidence: `~/Gits/.tmp/results/to-test-1014/phase2-screen-help-proof.json`.)_ _(triage 2026-10-14: WAITS — Needs an actual Windows runtime to observe cmd.exe/node.exe cleanup; this machine is macOS and rb is Linux.)_
- [ ] After a `make dev` restart, open a **new** TUIC shell and type `codex`, `grok`, and `opencode` in separate throwaway tabs. Confirm each supported installed CLI stays in native scrollback; repeat with its per-agent **Prevent alternate screen** setting off, then restore the setting. Existing shells retain the previous PTY environment and cannot verify this change. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Native scrollback in new TUIC shell tabs is UI.)_ _(audit 2026-10-13: PARTIAL — new session env: TUIC_CODEX_SCREEN_FLAG=--no-alt-screen, TUIC_GROK_SCREEN_FLAG=--no-alt-screen, TUIC_OPENCODE_SCREEN_FLAG=--mini; codex/grok/opencode are shell functions from tuic-integration.zsh…)_ _(triage 2026-10-14: WAITS — Installed help/argv and controlled launch evidence retained; real vendor startup/native scrollback needs isolated owned terminal runtime and desktop observation unavailable in this lane.)_
- [ ] After a `make dev` restart, launch Claude, Codex, Grok and OpenCode through the agent menu, PR Review where configured, and MCP `agent spawn` in an isolated `TUIC_APP_INSTANCE`; confirm terminal histories remain available after exit. Resume each session and check the same behavior. A CLI without the flag in `--help` should still launch. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Terminal history after agent launch is UI.)_ _(audit 2026-10-13: PARTIAL — `agent spawn` codex/grok/opencode with fake binary logged argv: codex `--no-alt-screen`, grok `--rules=... --no-alt-screen`, opencode `--mini` (~/Gits/.tmp/totest-E/fake2.log)…)_ _(triage 2026-10-14: WAITS — Launch/filter tests and controlled process evidence retained; complete real-agent menu/review/spawn scrollback flow needs an owned rebuilt runtime plus native desktop unavailable here.)_
## Markdown link navigation guard (2026-09-25) — Rust, needs `make dev` restart

- [ ] In the restarted instance, open an HTML preview, PDF preview, URL plugin panel on localhost, srcdoc plugin panel, and reveal.js deck. Their iframe content and in-frame links/slide navigation still load. Export a text download on Linux through a blob URL. _(Static coverage: `lib.rs` navigation guard allows the internal frame schemes and loopback origins; runtime platform behavior needs the restarted app.)_ _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Preview iframes are UI; download part also needs Windows/Linux.)_ _(audit 2026-10-13: PARTIAL — live: srcdoc (ui tab html) iframe rendered with sandbox "allow-scripts allow-same-origin"; URL tab on localhost loaded (GET /busy.html 200)…)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: native WebView navigation/download isolation needs a desktop window.)_
## Worktree warm status and safe removal (2026-09-25) — Rust, needs `make dev` restart

## Workflow definitions (2026-09-25) — Rust, needs `make dev` restart

- [ ] After a Rust restart in an isolated instance, launch a coordinator attempt into a registered throwaway worktree with `workflow_launch`, inspect its prompt and event timeline, create a story with `workflow_story_create` and retry its proposal key, submit `workflow_report` from that managed PTY, and verify a different PTY cannot report the attempt. Exit a second agent without reporting and confirm its attempt becomes interrupted and the run pauses. Do this only when losing the current dev sessions is acceptable. _(NOT VERIFIED 2026-10-12: the daemon starts the coordinator attempt itself, but without a claude/codex binary the attempt fails and the run pauses (events `attempt_started` then `paused`, commandId `daemon:failure:6`), so no worker or reviewer can report or integrate. The run is startable (see item 720) but this flow needs worker, reviewer or coordinator agent reports, a story worktree or a running published check. Evidence `~/Gits/.tmp/run-1706/b7.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: NOT RUN — each needs real claude/codex workers or reviewers driven by the engine in a story worktree (API cost, long-running); no isolated daemon can be reached without credentials (a debug tuic-remote…)_ _(triage 2026-10-14: WAITS — Reproduced failure and linked story retained; regression verification awaits an integrated fixed build.)_ _(verified 2026-10-14: production RunStore rejects wrong bound reporter, creates one story per proposal key only for bound coordinator, and clears interrupted coordinator ownership; six targeted tests in `~/Gits/.tmp/results/to-test-1014/workflow-report-contracts.log`. Actual launcher/process exit integration remains.)_ _(FAILED 2026-10-14: story 1883-eba0 — actual isolated workflow_launch rejects the registered branch worktree with “Cannot remove orphan worktree: checkout is not detached”; no child starts. Evidence: ~/Gits/.tmp/results/to-test-1014/workflow-launch-worktree-failure.json and workflow-launch-api-proof.json. Keep for regression verification.)_
- [ ] **[VISUAL]** In an isolated rebuilt instance, edit and publish a seeded workflow draft in the Plans and Stories Designer tab; verify backend graph validation errors and the saved revision. The component layout was visually checked with a temporary Vite harness at normal and narrow widths (`~/Gits/.tmp/workflow-designer-visual.png`, `~/Gits/.tmp/workflow-designer-narrow.png`). _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — VISUAL; the desktop window is Boss's live one, MacControl screenshots unavailable, headless instance needs Basic auth)_ _(triage 2026-10-14: WAITS — Reproduced failure and linked story retained; regression verification awaits an integrated fixed build.)_ _(FAILED 2026-10-14: story 1882-bc88 — actual isolated HTTP seed/edit/save succeeds, but Publish rejects the seeded Pause with “executable Pause needs resume_to”; Designer has no resume-target editor. Evidence: ~/Gits/.tmp/results/to-test-1014/workflow-designer-seed-failure.json and workflow-designer-api.log. Keep for regression verification.)_

- [ ] After restarting Boss's desktop build when sessions can be interrupted, open Plans and Stories from the project toolbar and inspect the layout and stale-revision error. The isolated browser build already completed the create → start manual → check criterion → submit review → approve flow on 2026-09-25; screen capture is pending because agent-browser returned white images even for a solid-red test page and MCPMacControl.app lacks Screen Recording permission. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — VISUAL; the desktop window is Boss's live one, MacControl screenshots unavailable, headless instance needs Basic auth)_ _(triage 2026-10-14: WAITS — Isolated create/edit/dependency API and dialog replay evidence retained; exact native layout/stale-revision rendering needs Boss rebuilt desktop. No restart or native capture available.)_

- [ ] After rebuilding the isolated Rust instance, submit a managed `workflow_report` with `needs_input` and `inputRequest`; verify Run history shows the pause, an operator `answer_input` command survives restart, and `resume` is refused until an answer exists. Submit a reviewer report with criterion-indexed findings and verify the story status does not change from the report alone. _(NOT VERIFIED 2026-10-12: the daemon starts the coordinator attempt itself, but without a claude/codex binary the attempt fails and the run pauses (events `attempt_started` then `paused`, commandId `daemon:failure:6`), so no worker or reviewer can report or integrate. The run is startable (see item 720) but this flow needs worker, reviewer or coordinator agent reports, a story worktree or a running published check. Evidence `~/Gits/.tmp/run-1706/b7.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: NOT RUN — each needs real claude/codex workers or reviewers driven by the engine in a story worktree (API cost, long-running); no isolated daemon can be reached without credentials (a debug tuic-remote…)_ _(triage 2026-10-14: WAITS — Unlanded fix 1883-eba0: workflow_launch rejects a normal registered branch worktree before spawning; integrated managed reports and parallel worker checks need that launch. Targeted store evidence remains partial.)_ _(verified 2026-10-14: actual RunStore input pause/refused early resume/durable answer/replay and reviewer criterion-index validation/advisory status pass in `~/Gits/.tmp/results/to-test-1014/workflow-report-contracts.log`; remaining check is Run history display plus explicit answer recovery after a daemon restart.)_
- [ ] After rebuilding an isolated Rust instance, launch two nonoverlapping story attempts from the active coordinator into two registered throwaway worktrees. Confirm distinct assignments appear in run history before spawn, a third attempt is held by the parallel limit, and overlapping or unknown scopes are serialized. Approve a prerequisite, run its pinned check in the assigned worktree, merge it into the canonical branch, then record integration and confirm the dependent releases. A failed post-merge check or later unrecorded ref movement must hold the dependent again. _(NOT VERIFIED 2026-10-12: the daemon starts the coordinator attempt itself, but without a claude/codex binary the attempt fails and the run pauses (events `attempt_started` then `paused`, commandId `daemon:failure:6`), so no worker or reviewer can report or integrate. The run is startable (see item 720) but this flow needs worker, reviewer or coordinator agent reports, a story worktree or a running published check. Evidence `~/Gits/.tmp/run-1706/b7.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: NOT RUN — each needs real claude/codex workers or reviewers driven by the engine in a story worktree (API cost, long-running); no isolated daemon can be reached without credentials (a debug tuic-remote…)_ _(triage 2026-10-14: WAITS — Unlanded fix 1883-eba0: workflow_launch rejects a normal registered branch worktree before spawning; integrated managed reports and parallel worker checks need that launch. Targeted store evidence remains partial.)_
- [ ] **[VISUAL]** In the isolated rebuilt instance, open **Run history** for a plan with real persisted events. Confirm the timeline advances after an IPC/SSE wake and remains readable at normal and narrow window widths. The component was visually checked with a temporary Vite harness and mocked IPC at both widths (`~/Gits/.tmp/workflow-timeline-visual.png`, `~/Gits/.tmp/workflow-timeline-narrow.png`); live backend integration awaits the Rust rebuild. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — VISUAL; the desktop window is Boss's live one, MacControl screenshots unavailable, headless instance needs Basic auth)_ _(verified 2026-10-14: real isolated daemon emits SSE and preserves persisted events after its own restart; production Run history replay consumes that captured hint and grows from four to five event rows with sequence 5 and retained event details. No horizontal DOM overflow at 1280px. Scope: ~/Gits/.tmp/results/to-test-1014/phase2-workflow-history-proof.json.)_ _(triage 2026-10-14: WAITS — Normal/narrow visual readability remains: MacControl recording denied and neither locked headed Chrome nor bundled headless Chrome produced a screenshot. Needs Boss’s visual check; recorded-response replay does not claim direct browser-to-rb integration.)_
## Native story API (2026-09-24) — Rust, needs `make dev` restart

## Remote Project Progress event (2026-09-24) — Rust, needs `make dev` restart

- [ ] With a connected remote repository after restart, report a new Progress entry on the remote agent. The local desktop Progress bell and dialog update, and `progress_list` reads that repository's journal from its owning machine. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs desktop build: remote connection + local Progress bell/dialog are desktop frontend/remote_mirror. Headless instance has no remote connection to configure. (A second local tuic-remote could serve as 'remote' for the phase-2 agent.))_ _(audit 2026-10-13: HUMAN — needs a connected remote repository)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
## Agent hook toggles store the default as absent (2026-09-24) — Rust, needs `make dev` restart

## Agent list on the `+` buttons (2026-09-25) — frontend, HMR

- [ ] [HUMAN] Decide in the morning whether the sidebar `+` right-click should also open the agent list (withheld pending approval, AGENTS.md "Sidebar clicks"). Today: tab bar `+` right-click and long press open the agent list; sidebar branch `+` long press opens it; sidebar `+` right-click opens the branch menu. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Boss decision (sidebar + right-click opens the agent list?), not a test; goes to the morning digest.)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
## Sub-agent tag icon in the sidebar (2026-09-25) — frontend, HMR

## Mobile terminal prose reflow (2026-09-24) — frontend, refresh PWA

- [ ] [VISUAL] On a phone-width PWA session with Claude output produced in a wider desktop terminal, read a long paragraph: words flow across the phone width without a short orphan line at the desktop row boundary. Lists and box-drawing tables keep their own rows and alignment. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — phone-width PWA)_ _(triage 2026-10-14: WAITS — Recorded paragraph/reflow and component evidence retained; live Claude paragraph/list/table appearance at phone width needs actual rendered capture. Locked Chrome cannot capture; real phone remains Boss observation.)_
## Voice library, voice files, Listen and loudness sliders (2026-09-24) — Rust + frontend, needs `make dev` restart

Settings > Voice > Spoken replies, with the Italian bundle and the runtime downloaded:

- [ ] Click "Add voice file…" and choose a valid Italian `.safetensors` voice. It appears under Yours and in the picker, and it speaks when selected. Choose a 24-layer (French) voice or a file that is not a voice: the reason shows under the button and nothing is added. The × on a Yours row deletes the file. Add a file with the same name as a voice under Yours: it is refused with "You already have a voice called … delete it first or choose another name", and the stored voice still speaks as before. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 on this headless instance. Add voice file UI + import route need desktop build (also needs a valid Italian .safetensors fixture); 'speaks' part is ear-only.)_ _(audit 2026-10-13: PARTIAL — IPC `import_speech_voice` it with garbage bytes -> `not a voice for Italian: not a safetensors voice file: header too large`, list unchanged…)_ _(triage 2026-10-14: WAITS — Seven targeted component checks passed in voice-library-contracts.log, including import/refusal and ready picker; valid Italian voice asset, audible synthesis and native import remain unverified.)_
- [ ] [HUMAN] With no hands-free conversation, click Listen and listen: the sample is audible, in the selected voice, at the Voice volume level, and the saved voice does not change. Start hands-free, let the agent speak a reply, and click Listen while it plays: the refusal shows inline. _(2026-09-24 isolated instance voice0924: `POST /dictation/speech/voices/preview` with hands-free not armed returned 200 for giovanni, alba and an imported voice, and the output device accepted the audio; UI Listen with alba showed no error. Nobody listened, so audibility is unverified.)_ _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): click Listen and hear the sample. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_ _(audit 2026-10-13: HUMAN — audible judgement)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] Move Voice volume and Levelling and release. The next reply is louder or quieter and more or less even, without a restart. At -12 dB with Strong there is no clipping. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): judging loudness/clipping by ear. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_ _(audit 2026-10-13: HUMAN — audible judgement)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_

HTTP import and delete (use the test instance on `:9877`):

- [ ] `POST /dictation/speech/voices/import` with `{"language":"it","name":"nonna","dataBase64":"<base64 of an Italian .safetensors>"}` succeeds. Voice choice then accepts `speech_voice` `"nonna"`, and a reply speaks in it. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 on this headless instance. POST /dictation/speech/voices/import needs desktop build and a valid Italian .safetensors fixture; spoken reply is ear-only.)_ _(audit 2026-10-13: PARTIAL — IPC `import_speech_voice` it with garbage bytes -> `not a voice for Italian: not a safetensors voice file: header too large`, list unchanged…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] Reinstall Italian from Settings > Voice while a downloaded voice (for example jean) and an imported voice are present. Both are still listed and still speak. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 on this headless instance. Needs desktop build with Italian bundle+jean+imported voice; 'still speak' is ear-only, listing part checkable via GET /dictation/speech/voices.)_ _(audit 2026-10-13: NOT RUN — live app has dictation routes only over IPC (HTTP needs auth): needs valid Italian .safetensors fixture and would alter Boss's installed voices)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] `GET /dictation/speech/voices?language=it` lists giovanni as `default`, then the downloaded and the imported voices. `?language=xx` returns an error that names `xx`. With the Italian bundle not downloaded it returns `[]`, and the voice picker offers only "Default for this language". _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 on this headless instance. GET /dictation/speech/voices?language= needs desktop build; picker UI too.)_ _(audit 2026-10-13: PARTIAL — live IPC `get_speech_voices`: it -> [{"id":"giovanni","source":"default"}] (no downloaded/imported voices at the time); xx -> `No speech bundle ships for language "xx"`…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- Note: an isolated `TUIC_APP_INSTANCE` debug instance binds no TCP port while remote access is off; reach it with `curl --unix-socket <tuic-mcp-*.sock>` (the path is in its log).
## Side panels follow an agent click to another repo (2026-09-24) — frontend, HMR

## Spoken replies at one level (2026-09-24) — Rust, needs `make dev` restart

- [ ] [HUMAN] After the restart, arm hands-free and have the agent speak two replies in two different voices. Both sound equally loud, with no clipping or pumping. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): two spoken replies judged by ear. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_ _(audit 2026-10-13: HUMAN — Audio output/device/ears required; no speaker playback or dictation config changed. commands.rs:2992-3006 preserves speaker for level-only changes…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] Set `speech_volume_db` to -24 in `dictation-config.json` through the settings save (`set_dictation_config`) while a reply is queued. The queued reply is not cut off, and the next reply is quieter. The same change is also available from the Voice volume slider in Settings > Voice > Spoken replies. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): audible comparison of reply loudness. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_ _(audit 2026-10-13: HUMAN — Audio output/device/ears required; no speaker playback or dictation config changed. commands.rs:2992-3006 preserves speaker for level-only changes…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## No duplicated agent rows after a WebView reload (2026-09-24) — frontend via HMR

- [ ] Open an agent tab that shows the Context bar. Reload the WebView (`POST localhost:9876/debug/reload_webview`). Scroll up: the last reply must appear once. Before the fix, each reload added 1–2 copies of its top rows. _(NOT VERIFIED 2026-09-29: Needs a real agent tab showing the Context bar (real Claude session); raw-ring check alone does not cover it)_ _(audit 2026-10-13: HUMAN — Explicit WebView reload is prohibited by assignment. No reload attempted; actual full repaint/reload behavior remains unverified.)_ _(triage 2026-10-14: WAITS — Needs an isolated desktop WebView reload; Boss reload is forbidden and managed peers may not launch another desktop. Prior browser reload fixture is not native evidence.)_
- [ ] Check without the eye: `GET /sessions/{id}/raw-ring`. Each Claude full repaint (`ESC[2K` run) after the reload must clear exactly the tab's row count, not 1–2 more. _(NOT VERIFIED 2026-09-29: Needs real Claude full repaint output (ESC[2K runs) after WebView reload.)_ _(audit 2026-10-13: HUMAN — Explicit WebView reload is prohibited by assignment. No reload attempted; actual full repaint/reload behavior remains unverified.)_ _(triage 2026-10-14: WAITS — Raw-ring checks need an owned backend and native full-repaint capture; rb SSH is unreachable and Boss WebView reload is forbidden.)_
## Hands-free turns reach a busy agent at once (2026-09-23) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, arm hands-free on a Claude Code tab, give it a long task, and speak while it works. The turn appears in the terminal within a second or two (Claude queues it or takes it mid-turn), not after the turn ends. `GET /logs?source=dictation` shows `Hands-free turn typed now` with the session. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Hands-free arm on a real Claude tab and spoken turn needs desktop; audio can be fed as `say`-synthesized PCM over WS /dictation/hands-free/audio instead of a mic.)_ _(audit 2026-10-13: HUMAN — Full live criterion needs microphone/Whisper and real Claude dialog/Compose behavior. continuous.rs:1931-1955 implements one log per hold and typed-now. No audio/device capture opened.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] While Claude shows a permission dialog, speak: nothing is typed into the dialog; the hands-free panel keeps showing the turn, and the log shows `Hands-free turn held` with `reason="confident question on screen"` once (not every 50 ms). Answer the dialog: the turn is typed. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free armed + permission dialog (reproducible with claude --permission-mode default, seen in item 529) + turn via audio WS; desktop only.)_ _(audit 2026-10-13: HUMAN — Full live criterion needs microphone/Whisper and real Claude dialog/Compose behavior. continuous.rs:1931-1955 implements one log per hold and typed-now. No audio/device capture opened.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] Queue a typed command in the Compose panel while the agent is busy, then speak: the spoken turn is typed now, and the Compose badge still shows the typed command, which goes out at the next idle as before. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free armed and Compose panel (frontend); desktop only.)_ _(audit 2026-10-13: HUMAN — Full live criterion needs microphone/Whisper and real Claude dialog/Compose behavior. continuous.rs:1931-1955 implements one log per hold and typed-now. No audio/device capture opened.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Opening AI Chat no longer aborts the app (2026-09-23) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, open the AI Chat panel (ego over ACP) and send a prompt. The app stays up and the reply streams in. Before the fix, `acp_subscribe` called `tokio::spawn` on the main thread, which panicked with `TryCurrentError` (SIGABRT, crash report `tuicommander-2026-09-23-164623.ips`) and killed every PTY session. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: AI Chat panel (ego over ACP) and acp_subscribe main-thread panic are Tauri desktop-only; headless tuic-remote has no main thread/UI. Needs desktop build with the frontend AI Chat panel.)_ _(audit 2026-10-13: HUMAN — Code acp_commands.rs:221-227 uses tauri::async_runtime::spawn outside current Tokio runtime; regression at :676 exists. Actual AI Chat prompt/subscription stream not exercised…)_ _(triage 2026-10-14: WAITS — Desktop regression gate attempted on rb but rustc was killed with SIGKILL before any test ran. Source acp_commands.rs:227/287 uses the global Tauri runtime; actual desktop stream remains unverified. Evidence: ~/Gits/.tmp/results/to-test-1014/phase2-subscription-markdown-proof.json.)_
## Readable Design Mode tab badge (2026-09-23) — frontend via HMR

## Hands-free from the Command Palette (2026-09-23) — frontend only, Vite HMR

- [ ] [HUMAN] With dictation enabled, open the Command Palette on an agent tab and run "Start hands-free conversation". Speak a sentence: it reaches that tab and the first earcon plays (priming happened inside the palette click). Switch tabs and reopen the palette: the entry reads "Stop hands-free conversation"; run it and the conversation stops. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real speech and first-earcon hearing require audio hardware/ears. No hardware activation attempted.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Reply language stated once per hands-free conversation (2026-09-23) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, with the dictation language on Auto-detect, arm hands-free on an idle agent tab and say two Italian sentences, then one English sentence. The terminal receives `<first> (reply in Italian)`, the second sentence with no suffix, and `<english> (reply in English)`. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free + Whisper Auto-detect; feed Italian/English `say` audio over WS /dictation/hands-free/audio on desktop build.)_ _(audit 2026-10-13: HUMAN — Language contract present continuous.rs:1104-1107,1281,1398-1403 with tests :3261,3281,3308,3393. End-to-end Whisper language recognition and terminal delivery need real audio; not run.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] After the same restart, set the dictation language to Italian and arm again: the start notice ends with `Reply in Italian.` and the first spoken turn carries no suffix. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free arm and dictation language setting; desktop only.)_ _(audit 2026-10-13: HUMAN — Language contract present continuous.rs:1104-1107,1281,1398-1403 with tests :3261,3281,3308,3393. End-to-end Whisper language recognition and terminal delivery need real audio; not run.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Custom hands-free start notice (2026-09-23) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, in Settings → Dictation → Hands-free, type a two-line start notice and arm hands-free on an idle agent tab: the agent receives your text as one line, not the built-in notice. Press **Reset to default**, disarm and arm again: the built-in notice is sent. `GET http://localhost:9876/dictation/hands-free/default-notice` returns the built-in text. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs Settings->Dictation UI plus arm on an agent-typed session; desktop only.)_ _(audit 2026-10-13: HUMAN — Code continuous.rs:1340-1356 folds custom notice whitespace; tests :3040,3058 cover custom/multiline notices. Live default-notice endpoint succeeds…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Activation phrase survives Whisper's spelling (2026-09-23) — Rust, needs `make dev` restart

- [ ] [HUMAN] After a `make dev` restart, with the activation phrase `senti mac`, arm hands-free and say three sentences that begin with "senti mac". Each one is queued, without the phrase. Then say "senti, ma che ore sono?" to someone else: it is dropped. For every dropped turn, `curl 'localhost:9876/logs?source=dictation'` shows the line `Hands-free turn dropped…` with a `heard=` field that holds the first four words Whisper wrote. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): spoken activation-phrase turns via microphone. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_ _(audit 2026-10-13: HUMAN — Real activation phrase transcription requires mic/Whisper acoustic evidence. No audio input opened.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Echo reference covers the whole reply (2026-09-23) — Rust, needs a `make dev` restart

- [ ] [HUMAN] After the restart, use the laptop speakers (no headphones). Arm hands-free with a voice and ask for a reply of at least 10 seconds. The reply plays to the end without cutting itself off, and no turn arrives that repeats its words. In `tuic.log`, no `speech: hushed` line appears during the reply, and there is no `echo: far-end reference full` warning. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): laptop speakers echo test. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_ _(audit 2026-10-13: HUMAN — Acoustic echo/barge-in judgment needs laptop speakers and microphone; no physical device test attempted.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] [HUMAN] Talk over a long reply after its first 3 seconds. It stops. The log shows `speech: hushed` with `speaking=true` and an `into_playback_ms` above 3000, then `Hands-free turn accepted heard=` with your words. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): talking over a reply. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_ _(audit 2026-10-13: HUMAN — Acoustic echo/barge-in judgment needs laptop speakers and microphone; no physical device test attempted.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Barge-in waits for sustained speech (2026-09-23) — Rust, needs a `make dev` restart

- [ ] [HUMAN] After the restart, use the laptop speakers (no headphones). Arm hands-free with a voice and ask a question. The spoken reply plays to the end unless you talk over it. Talk over a second reply: it stops within about a quarter of a second, and your first words are in the transcript. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): laptop speakers barge-in. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_ _(audit 2026-10-13: HUMAN — Real timing-sensitive speech pauses/barge-in/gain observation needs microphone/speakers/ears. No physical capture/playback attempted.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] [HUMAN] After the restart, in hands-free, say half a sentence, pause about a second, and finish it before the hold-back ends. One turn arrives with both halves; the first half is not sent alone. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): speaking half sentences into a mic. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_ _(audit 2026-10-13: HUMAN — Real timing-sensitive speech pauses/barge-in/gain observation needs microphone/speakers/ears. No physical capture/playback attempted.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] [HUMAN] After the restart, arm hands-free with the activation phrase on the desktop. Say a phrase, pause about four seconds, then continue without the phrase. The pending text gains the continuation and the agent receives one message after the final five-second window. Check the microphone level while TTS plays and whether the reply's own words appear as a new turn; these acoustic and gain observations require the real device. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): speaking with pauses, mic level with TTS. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_ _(audit 2026-10-13: HUMAN — Real timing-sensitive speech pauses/barge-in/gain observation needs microphone/speakers/ears. No physical capture/playback attempted.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Calm hands-free voice meter (2026-09-23) — frontend via HMR; the Rust level fallback needs a `make dev` restart

- [ ] [VISUAL] Arm hands-free. The toast shows one thin horizontal voice meter and the phase text — no pulsing dot and no moving dots after the text. Push-to-talk (dictation hotkey) still shows the bar meter, the dot and the dots. _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_ _(audit 2026-10-13: VISUAL-OWED — Pure rendering check; image access unavailable by assignment, no screenshot attempts.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] Stay silent with normal room noise (fan, typing): the voice meter stays flat. Speak: it fills at once and falls back smoothly over about 1.5 s after you stop, with no flicker between words. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): voice meter reaction to real speech/room noise. Also desktop-only (dictation routes 404 on headless).)_ _(audit 2026-10-13: HUMAN — Actual room noise/meter response requires microphone. No capture or image check attempted.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Unmerged worktree removal (2026-09-23) — Rust, needs `make dev` restart

## Sub-agent tags and branch count (2026-09-23) — Rust, needs `make dev` restart

- [ ] [VISUAL] After the restart, the in-window Activity Dashboard stays close to the detached window's width; a long terminal name remains readable beside the robot marker and project badge, and a narrow main window has no horizontal overflow. _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_ _(audit 2026-10-13: VISUAL-OWED — Pure rendering check; image access unavailable by assignment, no screenshot attempts.)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
- [ ] Pop out the Activity Dashboard: the detached window shows the same icon and parent tooltip. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Pop-out Activity Dashboard is a desktop window feature; needs desktop build. Backend parent_session on spawned child confirmed (see 608).)_ _(audit 2026-10-13: VISUAL-OWED — Detached-window icon and tooltip rendering need visual/window verification; unavailable image access. No popout opened.)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
## Voices from Kyutai's ungated repository (2026-09-23) — **Rust, needs a `make dev` restart**

- [ ] [HUMAN] A hands-free reply in Italian is spoken with the giovanni voice (`/dictation/speech/speak` refuses while hands-free is not armed). _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): hearing giovanni voice. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_ _(audit 2026-10-13: HUMAN — Live voice/mic playback and real binding unavailable without activating shared dictation hardware. Caller resolution exists mcp_transport_ancillary.rs:883,1331…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] **Rust, needs another `make dev` restart:** arm hands-free on a hand-opened Claude tab, then `voice action=status` from that tab reports `available: true` instead of "Speech is bound to another session" (caller now resolved to its live PTY, `resolve_mcp_origin_pty`). _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free armed on a real Claude tab, then MCP voice status; on headless voice status returns 'connection is not bound to a terminal'.)_ _(audit 2026-10-13: HUMAN — Live voice/mic playback and real binding unavailable without activating shared dictation hardware. Caller resolution exists mcp_transport_ancillary.rs:883,1331…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] Settings → Dictation → Spoken replies lists English, French, German, Italian, Portuguese and Spanish. Set the Whisper language to English, download English, and a hands-free reply is spoken in English with the alba voice. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Settings->Dictation list, ~390MB downloads; audible part is ear-only. Desktop only.)_ _(audit 2026-10-13: HUMAN — Live voice/mic playback and real binding unavailable without activating shared dictation hardware. Caller resolution exists mcp_transport_ancillary.rs:883,1331…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] French (24-layer, ~390 MB): download and speak one reply. The engine reads the layer count from `bundle.json`, but no 24-layer bundle has been run here before. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). French bundle download+speak; desktop only; audible verification is ear-only.)_ _(audit 2026-10-13: HUMAN — Live voice/mic playback and real binding unavailable without activating shared dictation hardware. Caller resolution exists mcp_transport_ancillary.rs:883,1331…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Alias survives a WebView reload (2026-09-23) — Rust, needs `make dev` restart

## Compose panel pin (2026-09-23) — frontend, live via HMR

## Hands-free earcons (2026-09-23) — **Rust, needs a `make dev` restart**

- [ ] Arm hands-free on the desktop, speak a turn: after the hold-back a short, quiet blip plays as it reaches the agent. Set an activation phrase and speak without it: a softer, lower blip plays and nothing is sent. Neither blip opens a turn or stops a spoken reply. Repeat from a browser tab at `:9876` — the tab beeps, the desktop stays silent. Also check the first blip after arming from the global hotkey is audible (WKWebView may keep an ungestured AudioContext suspended). Turn the Earcons setting off and repeat: no sound on the desktop, and none in a browser tab armed after the change. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): earcon blips heard through speakers. Also desktop-only (dictation routes 404 on headless).)_ _(audit 2026-10-13: HUMAN — Earcon hearing/direction and no acoustic feedback need real output/mic. No playback attempted.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_

- [ ] Earcon redesign (frontend, live via HMR): a delivered turn plays two short **rising** notes; a dropped turn plays two softer **falling** notes. Each is recognisable without hearing the other, and neither opens a turn, appears as captured speech, or stops a spoken reply. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): earcon note direction judged by ear. Also desktop-only (dictation routes 404 on headless).)_ _(audit 2026-10-13: HUMAN — Earcon hearing/direction and no acoustic feedback need real output/mic. No playback attempted.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Dictation auto-send on long text (2026-09-23) — frontend, live via HMR

- [ ] With Auto-send on, dictate 30+ seconds into a Claude tab: the text is submitted without pressing Enter, and no "Removed 1 invisible character" notice appears. _(NOT VERIFIED 2026-09-29: Needs 30+ s of real dictated audio into a Claude tab.)_ _(audit 2026-10-13: HUMAN — Real Claude/Codex prompt or dictation audio needed; no logged-in agent spawned. Current MCP catalogue explicitly says mail stays mail and is not typed into composer (mcp_transport_catalogue.rs:554)…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] Dictate a short phrase into Claude and into a Codex tab: both still submit. _(NOT VERIFIED 2026-09-29: Needs real dictation audio and real Claude and Codex tabs.)_ _(audit 2026-10-13: HUMAN — Real Claude/Codex prompt or dictation audio needed; no logged-in agent spawned. Current MCP catalogue explicitly says mail stays mail and is not typed into composer (mcp_transport_catalogue.rs:554)…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] **After a `make dev` restart (Rust):** `session action=submit` with a 600+ char input to a Claude tab returns `acknowledged:true` and the prompt runs; a peer `agent send` of a long message also submits. _(NOT VERIFIED 2026-09-29: Needs a real Claude tab to run the long prompt and peer agent send)_ _(audit 2026-10-13: HUMAN — Real Claude/Codex prompt or dictation audio needed; no logged-in agent spawned. Current MCP catalogue explicitly says mail stays mail and is not typed into composer (mcp_transport_catalogue.rs:554)…)_ _(triage 2026-10-14: WAITS — Captured queued submission tests passed; fresh long Claude prompt acknowledgement needs owned live vendor runtime, currently unavailable with rb SSH down. NOTE: agent send now delivers mail to inbox; it intentionally does not submit message text to composer.)_
## New-tab long press and settings button spacing (2026-09-23) — frontend, live via HMR

## Mobile Progress header (story 1214-2b95)

- [ ] [HUMAN] On a 360 px and a 390 px phone, open the Progress tab with a project available. Confirm the title, project and terminal selectors, **List | Flow**, and **Blocked only** are visible without horizontal scrolling; tap both view choices and the filter. Browser geometry at those widths is checked separately; this item covers real touch and device rendering. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone touch/device rendering explicitly HUMAN. No iPhone operation attempted; prior audio explanation in checklist is wrong.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Progress Flow view (2026-09-23) — **Rust, needs a `make dev` restart**

The List | Flow toggle is frontend and appears through HMR at once, but the running backend has no `progress_flow` until it restarts, so Flow shows an error line until then. The first open after the restart migrates `progress.sqlite3` (table rebuild for the new kinds).

## Markdown block review handoff (2026-09-23) — frontend, live via HMR

- [ ] With tweak comments in the file, choose a same-repository agent in the Markdown topbar and click **Send**. An idle agent receives the request immediately; a busy agent shows one queued command and receives it on its next idle transition. _(NOT VERIFIED 2026-09-29: Needs a real agent tab in the same repository with idle/busy transitions to observe immediate delivery vs queued delivery.)_ _(audit 2026-10-13: HUMAN — Markdown comment source-range/item contracts exist ContentRenderer.test.tsx:63 and tweakComments.test.ts:88,121; no browser hover/save real file operation. Targeted tests refused protected checkout…)_ _(triage 2026-10-14: WAITS — Same-repository selection, enqueue-once and queued toast component test passed in subscription-markdown-contracts.log; actual idle/busy vendor delivery needs owned backend unavailable while rb SSH is down.)_ _(verified partial 2026-10-14: production MarkdownTab test filters other repositories and enqueues once for selected session; prior captured backend ready/queue tests pass separately. Actual vendor idle/busy handoff remains. Evidence: ~/Gits/.tmp/results/to-test-1014/phase2-subscription-markdown-proof.json.)_
## Design Mode (2026-09-23) — **Rust, needs a `make dev` restart**

The existing `make dev` process does not hot-reload Rust. Restart it when the
current agent sessions can be closed, or use a separate debug instance with
`TUIC_APP_INSTANCE=<id>` to keep its configuration isolated. Targeted tests
cover the individual contracts; this check joins them in a real Chrome and
agent session.

An isolated `make dev` attempt on 2026-09-23 stopped before launch because
port 1421 was already serving a different checkout's Vite server. Do not stop
that checkout merely to run this check.

- [ ] On a page whose CSS uses custom properties (for example a Tailwind v4 _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Needs a Tailwind v4/shadcn fixture page as Dev Server)_ _(audit 2026-10-13: HUMAN — Design Mode needs host Chrome and bound real agent. extract.js:90-93 gathers shadow/adopted sheets; manager.rs:910-913 skips debugger pauses. No shared Chrome/repo config changed…)_ _(triage 2026-10-14: WAITS — FAILED in actual Chromium149: production extract.js loses var references in CSS shorthand declarations; normal padding and shadow adopted-sheet padding reproduce the same omission. Evidence: ~/Gits/.tmp/results/to-test-1014/design-tokens-failure.json and design-tokens-browser-proof.json; story 1888-14a3 records RED/GREEN on the fix branch; regression integration is pending in this audit checkout.)_
      or shadcn app), select a themed button. The grab carries a `tokens:` line
      with only the `--…` variables that button's rules reference, resolved to
      their values, and not the whole theme.
- [ ] On a page built from web components (open shadow roots with their own _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Needs a shadow-root web-component fixture page as Dev)_ _(audit 2026-10-13: HUMAN — Design Mode needs host Chrome and bound real agent. extract.js:90-93 gathers shadow/adopted sheets; manager.rs:910-913 skips debugger pauses. No shared Chrome/repo config changed…)_ _(triage 2026-10-14: WAITS — FAILED in actual Chromium149: production extract.js loses var references in CSS shorthand declarations; normal padding and shadow adopted-sheet padding reproduce the same omission. Evidence: ~/Gits/.tmp/results/to-test-1014/design-tokens-failure.json and design-tokens-browser-proof.json; story 1888-14a3 records RED/GREEN on the fix branch; regression integration is pending in this audit checkout.)_
      `<style>` or `adoptedStyleSheets`), select a button inside a component.
      The `tokens:` line lists the component's own `--…` variables. jsdom has no
      `ShadowRoot.styleSheets`, so no unit test covers this.
- [ ] Start Design Mode from another agent terminal in the same repository. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Needs two agent tabs in one repo.)_ _(audit 2026-10-13: HUMAN — Design Mode needs host Chrome and bound real agent. extract.js:90-93 gathers shadow/adopted sheets; manager.rs:910-913 skips debugger pauses. No shared Chrome/repo config changed…)_ _(triage 2026-10-14: WAITS — Current v2 launcher/source inspected; dedicated browser reuse, recipient rebinding and terminal/browser close lifecycle need native browser/agent integration, unavailable without an isolated desktop.)_
      Confirm the existing Chrome window is reused and subsequent grabs go to
      the newly bound terminal. Close that terminal, then Chrome: the status
      indicator must show Stopped and no further grab may be delivered.
- [ ] With a separate debug instance, check that quitting TUICommander closes _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Needs a separate debug desktop instance quit.)_ _(audit 2026-10-13: HUMAN — Design Mode needs host Chrome and bound real agent. extract.js:90-93 gathers shadow/adopted sheets; manager.rs:910-913 skips debugger pauses. No shared Chrome/repo config changed…)_ _(triage 2026-10-14: WAITS — Needs the named native platform/CI runtime; this machine is macOS.)_
      only the Chrome windows it owns. A browser/PWA start must explain that
      Chrome opens on the host machine.
- [ ] Review fixes (2026-09-23): a page running `debugger;` keeps responding; _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Needs a page running debugger; plus Vite dev page fix)_ _(audit 2026-10-13: HUMAN — Design Mode needs host Chrome and bound real agent. extract.js:90-93 gathers shadow/adopted sheets; manager.rs:910-913 skips debugger pauses. No shared Chrome/repo config changed…)_ _(triage 2026-10-14: WAITS — Retained legacy CDP manager skips all pauses at manager.rs:910-913, but current v2 launcher opens extension path. Native stop/restart and source-location/picking lifecycle remain unverified; no Boss browser touched.)_
      a Vite/webpack dev page resolves `source:` for most components, not only
      the first four modules; closing the bound terminal turns the hover
      highlight off, and a later start delivers no click made in between;
      closing Chrome and starting again works on the first click.
## Terminal Progress (2026-09-23) — Rust, needs a `make dev` restart

## Terminal scrollbar thumb minimum 48px (2026-09-23) — frontend, live via HMR

## Branch icon toggles agents (2026-09-22) — frontend, live via HMR

## Theme review applied (2026-09-23) — **Rust, needs a `make dev` restart**

Bundled JSONs updated (VS Code Dark now follows VS Code Dark 2026), Deep Black / Delicate One removed, "Clean"
shown as "Ink" and "VS Code Light" as "Paper", default and fallback moved to
Commander (`config.rs` default, `DEFAULT_THEME` in `settings.ts`). Before/after
reference: `docs/design/theme-gallery-2026-09-22/`. `seed_builtin_themes` is a
no-op once `<config>/themes` exists, so an existing install sees none of the
new colors or names until the JSONs are copied into that folder.

## DL and SU stop manufacturing scrollback (story `834-1878`, 2026-09-22) — **Rust, needs a `make dev` restart**

`delete_lines` and `scroll_up` used to push the rows they removed into history.
The fork tests and the retained ANSI captures cover the buffer; what they cannot
cover is what a real agent's repaint looks like on screen, and whether anything
a user relies on scrolled away with it.

- [ ] Select text in the scrollback, let the agent repaint, then copy. The _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend half verified on rust0930: seq 1..300, selection-text rows(150..152)=='150\n151\n152' identical before/after 20x CSI M/L/S/r repaint storms (historyBase=0). Frontend mouse selection+copy not observable headless.)_ _(audit 2026-10-13: HUMAN — Frontend mouse selection/copy requires verified clipboard interception and browser; no shared clipboard changed. Backend prior selection-text evidence retained.)_ _(triage 2026-10-14: WAITS — Nine frontend selection/copy tests and four rb Unicode/eviction tests pass; prior API grid evidence remains. Actual drag/output and rendering need an operable browser/native window with the verified clipboard guard: MacControl recording denied, locked Chrome pointer events/captures unavailable. Evidence: ~/Gits/.tmp/results/to-test-1014/phase2-selection-unicode-proof.json.)_
      selection must still yield the text it covered.
## Hands-free from a browser tab (story `832-e730`, 2026-09-22) — **Rust, needs a `make dev` restart**

A browser now has its own microphone and speaker for hands-free: the tab opens
`GET /dictation/hands-free/audio?owner=<id>` and streams capture up and replies
down on it. Rust refuses an owner with no socket rather than falling back to the
server's hardware, in either direction, and every part of that is covered by
tests — what no test can reach is a real microphone, a real speaker and a real
tab being closed.

- [ ] Press Start on a terminal running an agent. The browser must ask for _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Hands-free routes (/dictation/*, incl. /dictation/hands-free/audio WS) are #[cfg(feature=desktop)]: 404 on this headless rust0930 instance (unix socket and :9881). Needs the desktop build + browser (Chrome fake-device mic permission prompt).)_ _(audit 2026-10-13: HUMAN — Browser voice ownership/microphone requires isolated real audio socket/agent and capture. dictation_routes.rs:492-551 disconnects socket endpoint; no conversation armed to avoid shared voice state.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      microphone permission, and the phase must reach `waiting`.
- [ ] **[HUMAN]** Hold a complete turn: speak, see the transcript delivered to _(NOT VERIFIED 2026-09-30: blocked — audio hardware (mic/speakers, human listening); also dictation routes absent on headless build)_ _(audit 2026-10-13: HUMAN — Browser voice ownership/microphone requires isolated real audio socket/agent and capture. dictation_routes.rs:492-551 disconnects socket endpoint; no conversation armed to avoid shared voice state.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      the agent, and hear the reply **through the browser's speakers** — not
      through the machine running TUICommander. Check the other machine is
      silent.
- [ ] **[HUMAN]** Barge in mid-reply. The reply must stop where you are, not _(NOT VERIFIED 2026-09-30: blocked — audio hardware (mic/speakers, human barge-in); also dictation routes absent on headless build)_ _(audit 2026-10-13: HUMAN — Browser voice ownership/microphone requires isolated real audio socket/agent and capture. dictation_routes.rs:492-551 disconnects socket endpoint; no conversation armed to avoid shared voice state.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      merely stop being sent.
- [ ] **[HUMAN]** Close the tab mid-utterance. The conversation must disarm _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Socket-close disarm is testable (WS to /dictation/hands-free/audio?owner=X + arm on a real agent PTY, then GET /dictation/hands-free armed) but /dictation/* routes are desktop-gated: 404 on headless rust0930 (unix+:9881). Needs desktop build.)_ _(audit 2026-10-13: HUMAN — Browser voice ownership/microphone requires isolated real audio socket/agent and capture. dictation_routes.rs:492-551 disconnects socket endpoint; no conversation armed to avoid shared voice state.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      (`GET /dictation/hands-free` reports `armed: false`), nothing may stay
      queued, and nothing may be left speaking.
- [ ] Arm from the desktop app while a browser tab holds an audio socket. The _(NOT VERIFIED 2026-09-30: blocked — audio hardware: desktop microphone; also dictation routes absent on headless build)_ _(audit 2026-10-13: HUMAN — Browser voice ownership/microphone requires isolated real audio socket/agent and capture. dictation_routes.rs:492-551 disconnects socket endpoint; no conversation armed to avoid shared voice state.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      desktop must use its own microphone and ignore the browser entirely.
- [ ] Reload the browser tab while armed. The old conversation must disarm _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Reload-disarm testable via two WS connects with arm on a real agent PTY (real claude PTY now available), but /dictation/* routes are desktop-gated: 404 on headless rust0930. Needs desktop build.)_ _(audit 2026-10-13: HUMAN — Browser voice ownership/microphone requires isolated real audio socket/agent and capture. dictation_routes.rs:492-551 disconnects socket endpoint; no conversation armed to avoid shared voice state.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      rather than follow the new socket.
## ANSI edit-operation contract (2026-09-22) — **Rust, needs a `make dev` restart**

Four handler fixes in the Alacritty fork: IL/DL reset the cursor column,
IL/DL/ICH/DCH/ECH resolve a pending wrap, ED0 spares the cell behind one, and
DCH blanks only the cells it removed. Replay evidence and 200 fork tests cover
the parser; these items are the part a live terminal shows.

- [ ] **[VISUAL]** Select and copy text ending at the right margin after such a _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_ _(audit 2026-10-13: VISUAL-OWED — Pure rendering check; image access unavailable by assignment, no screenshot attempts.)_ _(triage 2026-10-14: WAITS — Nine frontend selection/copy tests and four rb Unicode/eviction tests pass; prior API grid evidence remains. Actual drag/output and rendering need an operable browser/native window with the verified clipboard guard: MacControl recording denied, locked Chrome pointer events/captures unavailable. Evidence: ~/Gits/.tmp/results/to-test-1014/phase2-selection-unicode-proof.json.)_
      redraw. The copied text must keep its last character.
## Selection during output (2026-09-21) — **Rust, needs a `make dev` restart**

- [ ] After loading the updated frontend, copy one block, then select a different _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Client-side selection persistence across a full frame is frontend-only (no server selection state); backend contract (historyBase snapshots) verified under 895. Needs desktop/browser build with clipboard guard.)_ _(audit 2026-10-13: HUMAN — Canvas selection/full-frame behavior requires browser with clipboard guard; no image capability and no shared clipboard writes…)_ _(triage 2026-10-14: WAITS — Nine frontend selection/copy tests and four rb Unicode/eviction tests pass; prior API grid evidence remains. Actual drag/output and rendering need an operable browser/native window with the verified clipboard guard: MacControl recording denied, locked Chrome pointer events/captures unavailable. Evidence: ~/Gits/.tmp/results/to-test-1014/phase2-selection-unicode-proof.json.)_
      multi-row block while output continues and keep the mouse held. The new
      selection must not disappear when another full frame arrives.
- [ ] After loading the rebuilt backend and frontend, park a terminal in history _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend half verified: selection-text with historyBase: snapshot at seq 1..300 rows(150-152) stays valid under 9000 rows, then 409-style error 'selection rows are no longer retained' after passing the 10000-line cap (never substitutes next row); a live snapshot re-read after +500 rows with historyBa)_ _(audit 2026-10-13: HUMAN — Canvas selection/full-frame behavior requires browser with clipboard guard; no image capability and no shared clipboard writes…)_ _(triage 2026-10-14: WAITS — Nine frontend selection/copy tests and four rb Unicode/eviction tests pass; prior API grid evidence remains. Actual drag/output and rendering need an operable browser/native window with the verified clipboard guard: MacControl recording denied, locked Chrome pointer events/captures unavailable. Evidence: ~/Gits/.tmp/results/to-test-1014/phase2-selection-unicode-proof.json.)_
      while output continues beyond the scrollback cap. Selection must stay on
      the same retained text, both during dragging and after release; copying
      must not substitute the next row when history advances. Automated browser
      checks require the verified clipboard guard in `tests/terminal-stress/`.
      Investigation and validation: `plans/terminal-selection-output-integrity.md`.
## Terminal Unicode integrity (2026-09-21) — **Rust, needs a `make dev` restart**

- [ ] After loading the rebuilt backend, print precomposed and decomposed accents, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend half verified: printf PRE U+00E9, e+U+0301, a+U+0300 and accent written after base in a second printf -> lines keep code points 0xe9,0x65 0x301,0x61 0x300,0x65 0x301; resize 60->14->10->60 reflow keeps all combining marks; OSC8 span after accented text cell-aligned; POST search-buffer matche)_ _(audit 2026-10-13: HUMAN — Unicode/cursor/selection rendering requires image access and verified clipboard guard; no real selection/copy attempted. Backend prior Unicode/search/reflow evidence retained.)_ _(triage 2026-10-14: WAITS — Nine frontend selection/copy tests and four rb Unicode/eviction tests pass; prior API grid evidence remains. Actual drag/output and rendering need an operable browser/native window with the verified clipboard guard: MacControl recording denied, locked Chrome pointer events/captures unavailable. Evidence: ~/Gits/.tmp/results/to-test-1014/phase2-selection-unicode-proof.json.)_
      including an accent written after its base letter. Verify rendering, block
      cursor, selection/copy, search and a link following accented text; scroll
      during output and resize. The installed instance still needs a restart
      even when the isolated verification build passes. See
      `plans/terminal-unicode-integrity.md` for automated evidence and limits.
## Dictation preserves speech before a pause (2026-09-21) — **Rust, needs a `make dev` restart**

- [ ] After restart, dictate a short phrase followed by a pause while holding F5. The live preview must receive the phrase; the final transcription must retain it. Verify silent recordings remain rejected by the configured speech gates. _(NOT VERIFIED 2026-09-30: blocked — audio hardware: microphone/F5 speech; dictation routes absent on headless build)_ _(audit 2026-10-13: HUMAN — Real microphone/F5 hardware dictation explicitly HUMAN; no hardware recording attempted.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Hooked dialogs restore the question badge (2026-09-21) — **Rust, needs a `make dev` restart**

- [ ] After restart, a Claude selection dialog with native hooks enabled must _(NOT VERIFIED 2026-09-30: partial — Real claude (sonnet) AskUserQuestion 2-question dialog, hooks emulated: agent_state awaiting_input/awaiting_input=true on dialog open; Tab Q1->Q2, resize x2 kept it; Enter-answer Q1 kept it; ONE 'awaiting_input set' log. On the Submit/Review tab (dialog still open) awaiting dropped (completed/idle o)_ _(audit 2026-10-13: HUMAN — Native-hooked real Claude multi-question dialog and final answer/cancel not driven. No logged-in agent spawned; prior partial failure not upgraded from code alone.)_ _(triage 2026-10-14: WAITS — Recorded Claude suggestion/question/permission boundary tests passed; actual native selection dialog plus submit-review interaction needs owned live Claude terminal and desktop, unavailable.)_
      show the question badge. It must survive switching sub-questions and
      redraws without duplicate notifications. After the final answer or cancel,
      normal protocol activity must clear question when the dialog closes.
## Protocol idle survives terminal animation (2026-09-21) — **Rust, needs a `make dev` restart**

- [ ] After a rebuild/restart, finish a Codex turn in brainstorming with its idle _(NOT VERIFIED 2026-09-30: partial — Real codex 0.159: after a turn idle held 40s and 2nd prompt -> busy ~1s after Enter (hook-idle Protocol). Codex pet animation (pet=codex in private CODEX_HOME) did not render/emit output in the TUIC PTY (last_activity static), so real animation NOT exercised. Emulated: script sends OSC 7770 idle the)_ _(audit 2026-10-13: HUMAN — Real Codex idle animation not exercised. No credentials/login and no existing Boss agent touched; prior emulated/partial evidence remains partial.)_ _(triage 2026-10-14: WAITS — Exact installed Codex pet animation/native idle behavior needs a fresh vendor turn and visible terminal; earlier capture did not show the animation. No fabricated parser/visual pass.)_
      animation enabled. The terminal and session API must remain idle while the
      animation runs. Submit another prompt: working must return immediately.
## Ego reaches this instance and the collapsed tool surface (#802-4c43, 2026-09-21) — **Rust, needs a `make dev` restart**

The ego MCP entry carried `TUIC_APP_INSTANCE`, which `tuic-bridge` never reads,
so with a named instance running beside the default one ego drove the DEFAULT
instance. It now carries `TUIC_SOCKET` set to the socket this process bound.
Separately, `tools/list` preferred the session flag over the per-request `_meta`
identity, so ego was handed the full catalogue after `server/discover` had
advertised the collapsed one.

- [ ] Start a named test instance (`TUIC_APP_INSTANCE=tuic-test make dev`) while _(NOT VERIFIED 2026-09-30: partial — AI Chat session/new fails on this host: ~/.ego/config.toml [mcp.servers.tuic] stdio bridge path missing (ENOENT) -> 'supplied MCP servers could not be admitted'; /acp/one-shot (no MCP) works. Not exercised: needs AI Chat session list + named instance)_ _(audit 2026-10-13: HUMAN — Named second desktop instance startup explicitly forbidden. Current peer has collapsed four-tool MCP surface but does not prove named AI Chat isolation. No second instance launched.)_ _(triage 2026-10-14: WAITS — In-process ACP/host identity tests passed in acp-transport-contracts.log; actual named-instance ego answer still needs isolated backend/model runtime. rb offline; no managed desktop launch.)_
      Boss's install is running. Open the AI Chat panel there and ask ego to list
      the sessions: it must report the test instance's sessions, not Boss's.
- [ ] In that same conversation ask ego which tools it has. It must name _(NOT VERIFIED 2026-09-30: partial — AI Chat session/new fails on this host: ~/.ego/config.toml [mcp.servers.tuic] stdio bridge path missing (ENOENT) -> 'supplied MCP servers could not be admitted'; /acp/one-shot (no MCP) works. Not exercised: ego tool listing needs a session with the tuicommander MCP)_ _(audit 2026-10-13: HUMAN — Named second desktop instance startup explicitly forbidden. Current peer has collapsed four-tool MCP surface but does not prove named AI Chat isolation. No second instance launched.)_ _(triage 2026-10-14: WAITS — ACP transport/host identity and collapsed-tool evidence retained; exact natural-language tool answer requires a real owned ego turn on a reachable instance, currently unavailable.)_
      `search_tools`, `get_tool_schema`, `call_tool` and `progress` — four, not
      the full catalogue.
## Mirrored remote events stay remote (#801-d34e, 2026-09-21) — **Rust, needs a `make dev` restart**

A mirrored event was indistinguishable from a local one, so a remote daemon's
`session-created`, `ui-tab`, `repo-changed` and worktree events ran the LOCAL
handlers. The window emit is now limited to `session-state-changed` and
`session-closed`, every mirrored payload carries `__tuic_origin`, and a frame
that already has one is dropped.

- [ ] Connect mac-mint, start a PTY **on mac-mint** (ssh in, `tuic session` there, _(NOT VERIFIED 2026-09-30: blocked — BLOCKED class: second physical machine (mac-mint/SSH daemon) required; no such host reachable from this instance)_ _(audit 2026-10-13: HUMAN — Actual mac-mint connection/remote session and repository UI not driven; no external credentials or remote repo mutation. Remote badge/event behavior remains unverified on a disposable remote session.)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
      or its own UI). This Mac must show it as a session-list row with the remote
      badge and **no new tab** — no `PTY: Session N`.
- [ ] Its busy/idle/question badge still moves from here while it works. That is _(NOT VERIFIED 2026-09-30: blocked — BLOCKED class: second physical machine (mac-mint/SSH daemon) required; no such host reachable from this instance)_ _(audit 2026-10-13: HUMAN — Actual mac-mint connection/remote session and repository UI not driven; no external credentials or remote repo mutation. Remote badge/event behavior remains unverified on a disposable remote session.)_ _(triage 2026-10-14: WAITS — Needs the Mac-mint session described by the preceding check.)_
      the one thing the window emit is still allowed to carry.
- [ ] Open a repo folder on mac-mint from its own UI: no repository appears in _(NOT VERIFIED 2026-09-30: blocked — BLOCKED class: second physical machine (mac-mint/SSH daemon) required; no such host reachable from this instance)_ _(audit 2026-10-13: HUMAN — Actual mac-mint connection/remote session and repository UI not driven; no external credentials or remote repo mutation. Remote badge/event behavior remains unverified on a disposable remote session.)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
      this Mac's sidebar, and no git work runs here for that path.
## Notification sound teardown (2026-09-20) — **Rust, needs a `make dev` restart**

- [ ] In Settings → Notifications, play each Test sound through the output device that previously crackled. The tone must end cleanly, with no relay-like click after its release. The source now reaches an exact zero sample and feeds 100 ms of silence before closing, but only the real CoreAudio device can verify the hardware-buffer teardown. _(NOT VERIFIED 2026-09-30: blocked — BLOCKED class: audio hardware (real CoreAudio output device and ears).)_ _(audit 2026-10-13: HUMAN — CoreAudio teardown crackle explicitly depends on affected real output device and ears; no playback attempted.)_ _(triage 2026-10-14: WAITS — Needs Boss to listen on the previously affected audio output device.)_
## Progress dialog and journal (2026-09-19)


- [ ] Open Progress (palette: "Open Project Progress"), move the pointer across three rows, then off the list. Every delete icon must be hidden again; before, WKWebView kept the icon of every row crossed. CSS only, live via HMR — no restart. Item created because the fix could not be reproduced programmatically. _(NOT VERIFIED 2026-09-29: WKWebView-specific hover-stuck icon; author states it could not be reproduced programmatically; needs desktop app and human.)_ _(audit 2026-10-13: VISUAL-OWED — WKWebView hover-stuck delete-icon check requires rendering observation. CSS ProgressDialog.module.css:38-43 removes opacity transition and uses hover-only reveal; no pointer live check.)_ _(triage 2026-10-14: WAITS — WKWebView-specific regression needs Boss native observation; MacControl Screen Recording is denied.)_ _(verified partial 2026-10-14: actual Chromium ProgressDialog hides all three delete icons after pointer leaves; cannot establish the WKWebView-specific regression. Evidence: ~/Gits/.tmp/results/to-test-1014/progress-hover-after-leave.json.)_
## ego reaches TUIC over the stdio bridge (story `796-7fa3`, 2026-09-20) — **Rust, needs a `make dev` restart**

The session's MCP entry moved from an HTTP URL to the `tuic-bridge` sidecar on
stdio. Every test here stops at the wire shape TUICommander writes; what none of
them reaches is ego actually spawning that binary and calling a tool through it.

- [ ] Launch a second instance with `TUIC_APP_INSTANCE=qa`, open AI Chat there, and ask ego which repositories it can see. It must see the `qa` instance's repositories, never the default instance's. _(NOT VERIFIED 2026-09-30: partial — AI Chat session/new fails on this host: ~/.ego/config.toml [mcp.servers.tuic] stdio bridge path missing (ENOENT) -> 'supplied MCP servers could not be admitted'; /acp/one-shot (no MCP) works. Not exercised: needs second instance + ego session)_ _(audit 2026-10-13: HUMAN — Remote Access on/off changes shared config, second desktop launch forbidden. Existing managed MCP reachability proves bridge availability for this peer, not AI Chat sessions under both settings…)_ _(triage 2026-10-14: WAITS — Current AI Chat uses in-process MCP-over-ACP, with transport identity tests passed; exact qa-versus-default repository answer needs a real isolated conversation. rb SSH unavailable; Boss app untouched.)_
## PR review, changelog and improvement scan run on ego (story `795-320b`, 2026-09-20) — **Rust, needs a `make dev` restart**

`pr_review.rs`, `changelog.rs`, `improvement_scan.rs`, two new `AppEvent`
variants and four new HTTP routes are all new Rust, and none of them load into a
running `make dev`. The parsers, the confidence gate and the stores are unit
tested; what no test reaches is a real ego process answering a real diff.

- [ ] Open a PR's detail popover and click **Run** under AI Review. It must produce findings (or "No findings" with a reviewed-file count and "by ego"), never a blank panel. Tick a finding with a line number and click **Post review**; the comment must appear on the PR in GitHub. _(NOT VERIFIED 2026-09-30: partial — POST /repo/pr-review {repoPath,prNumber:88} on real PR via dev ego: summary + files[{findings:[]}] returned (No findings, 1 file). Post-review/UI/line ticking not exercised.)_ _(audit 2026-10-13: HUMAN — Full PR/review/scan/changelog UI not exercised. Real Post review/Create issue writes explicitly forbidden; shared ego path not modified…)_ _(triage 2026-10-14: WAITS — Focused review/render tests and prior isolated error probe retained; live review/posting needs owned GitHub fixture and account plus real ego process. No external review posted or credentials read.)_
- [ ] Rename `ego_executable` in Settings → General to something that does not exist and run the review again. The popover must show ego's own sentence, not an empty finding list. Put the real path back. _(NOT VERIFIED 2026-09-30: partial — ego_executable=/nonexistent -> /repo/pr-review and /repo/improvement-scan return {error:'ego could not run this turn: invalid ego executable: No such file or directory'}; popover render not seen. Restored afterwards.)_ _(audit 2026-10-13: HUMAN — Full PR/review/scan/changelog UI not exercised. Real Post review/Create issue writes explicitly forbidden; shared ego path not modified…)_ _(triage 2026-10-14: WAITS — Await fixed test harness 1877-fb58 in this checkout before repeating the focused validation; no production defect inferred.)_ _(VALIDATION BLOCKED 2026-10-14: focused PrDetailContent error-render assertion passes, but unhandled logger error makes the run fail because mockRpc returns undefined; test-harness fix 1877-fb58. Evidence: ~/Gits/.tmp/results/to-test-1014/review-changelog-contracts.log. No production failure inferred and no live ego setting changed.)_
- [ ] Open the **Ops Dashboard** (the chart icon in the same header). Click each of `refactor`, `testing` and `perf`. Each scan must fill the Proposals column with at most five cards, and **Create issue** on one of them must file a real GitHub issue. _(NOT VERIFIED 2026-09-30: partial — POST /repo/improvement-scan focus refactor/testing/perf each 200 with proposals:[] on tiny fixture (<=5 not testable); cards/Create issue is UI.)_ _(audit 2026-10-13: HUMAN — Full PR/review/scan/changelog UI not exercised. Real Post review/Create issue writes explicitly forbidden; shared ego path not modified…)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
- [ ] While a review is running, the dashboard's Review findings column must show the PR as Working and then Done with a count — that is the `review-progress` event arriving over the bus. _(NOT VERIFIED 2026-09-30: partial — PR review ran to completion on real PR; review-progress event stream not captured; dashboard is UI.)_ _(audit 2026-10-13: HUMAN — Full PR/review/scan/changelog UI not exercised. Real Post review/Create issue writes explicitly forbidden; shared ego path not modified…)_ _(triage 2026-10-14: WAITS — Store event/render evidence retained; actual review-progress from an owned ego turn needs isolated backend/model runtime. Remote runner unavailable; no live GitHub/agent mutation.)_
- [ ] From a browser (not the Tauri app) at `localhost:9876`, run the same review and the same scan. Both must work: these routes are deliberately not desktop-gated. _(NOT VERIFIED 2026-09-30: partial — Unix-socket (local) router serves /repo/pr-review, /repo/changelog, /repo/improvement-scan OK. Remote router :9881 (Basic auth) returns 404 for all three (not in build_remote_router).)_ _(audit 2026-10-13: HUMAN — Full PR/review/scan/changelog UI not exercised. Real Post review/Create issue writes explicitly forbidden; shared ego path not modified…)_ _(triage 2026-10-14: WAITS — HTTP routes are present in non-desktop builds; actual review/scan turn requires owned backend and model runtime. rb unavailable, and Boss endpoints not used for writes.)_
## The terminal stream is compressed over a remote connection (story `794-832e`, 2026-09-20) — **Rust, needs a `make dev` restart**

`ws_stream`, `mcp_http::ws_compression` and the ssh `Compression=yes` are new
Rust. The codec is unit-tested on both sides and the framing end to end against a
mock socket; what no test reaches is a real browser inflating a real socket, and
a real ssh process.

- [ ] Resize the remote terminal, scroll it, and let the agent repaint a full screen. No stale rows, no torn frames — a mis-ordered inflate would show as rows from an older screen surviving under a newer one. _(NOT VERIFIED 2026-09-30: blocked — BLOCKED class: second physical machine. Daemon binds loopback only (LAN IP :9881 times out), so every peer is loopback and gets identity tags; no compressed frames obtainable here; real browser inflate also needed.)_ _(audit 2026-10-13: HUMAN — Actual remote browser/SSH tunnel profile lifecycle not driven; tunnel command.rs:48-52 adds Compression=yes/no, tests :180-195 exist but not run. No SSH profile changes or external login.)_ _(triage 2026-10-14: WAITS — Four production WsTransport compression/order tests passed in compression-order-contracts.log using actual CompressionStream/DecompressionStream and ordered delivery. Actual remote resize/scroll/repaint pixels still require an owned remote session and available capture; rb SSH is down.)_
- [ ] Settings → Services → SSH Tunnels → edit a profile: **Compress the channel (ssh -C)** is on. Save, start the tunnel, and confirm with `ps ax | grep "[s]sh -N"` that the command line holds `-o Compression=yes`. Untick it, restart the tunnel, confirm `Compression=no`. _(NOT VERIFIED 2026-09-30: partial — Via /tunnels API against a fake sshd listener: profile compression:true -> ps shows 'ssh -N ... -o Compression=yes'; false -> Compression=no. Settings checkbox UI not driven.)_ _(audit 2026-10-13: HUMAN — Actual remote browser/SSH tunnel profile lifecycle not driven; tunnel command.rs:48-52 adds Compression=yes/no, tests :180-195 exist but not run. No SSH profile changes or external login.)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
- [ ] Open a repository through an **SSH remote connection** and attach a terminal. The stream socket asks for `compress=deflate` (the client sees a remote connection) but the daemon answers identity tags because the peer is loopback — the terminal must still work, and the saving comes from the tunnel instead. _(NOT VERIFIED 2026-09-30: partial — WS /sessions/{id}/stream?compress=deflate over loopback TCP with subprotocol tuic.deflate: 101 selects tuic.deflate, 44 frames all tag 0x02 (identity), stream live. SSH remote connection through the app not done.)_ _(audit 2026-10-13: HUMAN — Actual remote browser/SSH tunnel profile lifecycle not driven; tunnel command.rs:48-52 adds Compression=yes/no, tests :180-195 exist but not run. No SSH profile changes or external login.)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
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

- [ ] The AI Chat panel's own connection is untouched: open the panel, send a turn, then run an `api` prompt. The panel's conversation must survive, and the prompt must not appear in it. _(NOT VERIFIED 2026-09-30: partial — one-shot uses its own launched ego (separate pid from the panel connection, which stayed alive); panel conversation itself not driven (session/new blocked).)_ _(audit 2026-10-13: HUMAN — Requires interaction with Boss-owned active app/repository or vendor/provider process; no ownership of that state. Contract test/source search performed…)_ _(triage 2026-10-14: WAITS — Separate one-shot/interactive connection tests retained; whole check explicitly requires two real ego turns. Owned runtime unavailable with rb SSH down.)_
## AI Chat runs on ego (story `785-58ca`, 2026-09-20) — frontend, Vite HMR picks it up, but it needs a real ego binary

Everything below is proven against a test double at the IPC boundary. What no
test can reach is the live process: these are the checks that need one.

- [ ] Set the path, open the panel on a repository, send a turn. The answer must stream in, reasoning must fold into a *Thinking* disclosure, and tool calls must stay one card each as their status changes. _(NOT VERIFIED 2026-09-29: Needs real ego streaming a turn with reasoning and tool calls.)_ _(audit 2026-10-13: HUMAN — Requires interaction with Boss-owned active app/repository or vendor/provider process; no ownership of that state. Contract test/source search performed…)_ _(triage 2026-10-14: WAITS — Production stream/thought/tool-card component checks passed in core-ui-tests.log; actual vendor stream requires owned ego/backend runtime, currently unavailable.)_
- [ ] A live prompt: check whether ego echoes the user message back as `user_message_chunk`. _(NOTE: the panel now renders server `promptSent` rather than a local optimistic message; if ego also sends a live user chunk, the two sources could duplicate it.)_ _(audit 2026-10-13: HUMAN — Requires interaction with Boss-owned active app/repository or vendor/provider process; no ownership of that state. Contract test/source search performed…)_ _(triage 2026-10-14: WAITS — Recorded ACP echo/de-duplication component behavior retained; requested fresh live echo observation needs real ego conversation on owned runtime. rb unavailable.)_
- [x] With **ego executable** empty, open the panel: it must say ACP is not configured, show no input box, and launch nothing. _(verified 2026-09-29: ego empty, status-bar AI Chat button: panel text 'ACP is not configured. Set the ego executable in Settings to start a conversation.'; 0 visible textarea/input in panel. No spawn attributable (only ego pid seen was Boss's cwd ~/Gits, not ours).)_
- [ ] Set the path, open the panel on a repository, send a turn. The answer must stream in, reasoning must fold into a *Thinking* disclosure, and tool calls must stay one card each as their status changes. _(NOT VERIFIED 2026-09-29: Needs real ego streaming a turn with reasoning and tool calls.)_
- [ ] Let ego ask for permission. The buttons must be the ones ego published, and answering must clear the card in every open window — not only the one that answered. _(NOT VERIFIED 2026-09-29: needs a real ego agent (ACP) session; not available headless)_
- [ ] Switch repository and back. Only one ego process per root (`ps ax | grep ego`), and the first conversation must still be there. _(NOT VERIFIED 2026-09-29: Needs real ego processes (ps ax | grep ego) per repo root.)_
- [ ] A live prompt: check whether ego echoes the user message back as `user_message_chunk`. _(NOTE: the panel now renders server `promptSent` rather than a local optimistic message; if ego also sends a live user chunk, the two sources could duplicate it.)_

## An idle Agent-Teams teammate no longer holds its lead "working" (2026-10-01)

Rust change, needs a `make dev` restart. Claude Code lists a teammate as `running` in every
`Stop` payload until it is shut down, so a lead with any teammate used to stay
`declared_background_work=true` forever (and the tab read "working", with completion/suggest/
standby held). The lead now counts as working only for non-teammate background work, or while a
teammate it owns (tmux pane `lead_session_id`, sent by `tuic-cli` as `origin_session_id`) has a
busy terminal. Needs the rebuilt `tuic-hook` sidecar (it now also emits `bgtasksummary`): run
`cargo build --package tuic-hook` and let the app re-install hooks.

- [ ] Spawn an Agent Teams swarm from a Claude Code lead, let every teammate finish and go idle.
  Once the lead's own turn ends, the lead's tab reads **idle** (not "working"); before this
  change it stayed working until the next prompt.
- [ ] While a teammate is actually working, the lead's tab reads **working**; the moment that
  teammate finishes it returns to idle without any input in the lead.
- [ ] A lead with a teammate idle AND a `run_in_background` shell task still running reads
  working; it goes idle when the shell task's completion ends the turn.
- [ ] `GET /sessions/{lead}/explain-state`: `epoch_flags.declared_background_work` shows
  `breakdown_source: "summary"`, `teammate_running`, `teammates_busy`, `counts_now`, `age_ms`,
  and a `swarm.teammates` list with each teammate's `shell_state`. On a teammate's session,
  `swarm.lead_session_id` names the lead.
- [ ] Explain-state modal (tab context menu) on a lead with teammates: a **Declared background work**
  section (counts_now, age, breakdown, teammates busy, unlinked) and a **Swarm** section (lead and each
  teammate's state) appear; on a teammate, the Swarm section names its lead. Leave the modal open while a
  teammate goes busy -> idle and re-open: values follow. With the modal open, make the tab's badge and the
  backend disagree (e.g. let the agent finish) and confirm the mismatch banner appears/clears without
  closing the modal.
- [ ] Close a teammate's terminal while the lead's last Stop still lists it: the lead keeps reading working
  until its next Stop (expected trade-off), and the explain payload shows it as unlinked, not as a listed
  teammate.
- [ ] Smart Prompt injection into a lead whose only remaining work is an idle teammate works (no
  "Agent is busy").
- [ ] Old `tuic` binary on PATH (no `origin_session_id`) or a plain `tuic alias` swarm: the lead
  still reads working while a teammate exists (fail-safe, the old behavior) and `explain-state`
  shows `unlinked_teammates > 0`. It must never read idle while a teammate is working.
- [ ] Older installed `tuic-hook` (no `bgtasksummary`): the lead still reads working while a
  teammate exists (old behavior, not worse). `explain-state` shows `breakdown_source:
  "statuses_only"`.

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
already running currently does nothing visible** — confirmed unrelated to
this fix and unrelated to frontend/Rust deep-link code (`open -a <exact
running app bundle path> 'tuic://...'` delivers and logs correctly instantly;
bare `open 'tuic://...'` — what `tuic open-here` actually calls — never
reaches the app at all). Root cause: **two copies of TUICommander.app are
registered under the same bundle id `com.tuic.commander`** on this machine —
`/Applications/TUICommander.app` (v1.7.4-nightly, Aug 13) and the running dev
build under `src-tauri/target/release/bundle/macos/` (v1.7.6-nightly) — so
macOS Launch Services' `tuic://` scheme resolution is ambiguous/stale. This
is an install-hygiene issue on Boss's machine, not a code bug — needs the
stale `/Applications` copy removed or replaced and Launch Services refreshed
before `tuic open-here` will work again. Not fixed as part of this session
(touching `/Applications` needs Boss's own OK).
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

## Customizable notification sounds — per-event preset/custom-file picker (2026-09-11, **Rust change — needs `make dev` restart**)
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

## OSC 1337 support — StealFocus/RequestAttention/OpenURL need a real window (2026-09-10, Rust-touching — needs a `make dev` restart)

Added recognition for the OSC 1337 proprietary commands: `CursorShape`,
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
- [ ] After a `make dev` restart with Codex hooks enabled, submit a Codex prompt and confirm AI Chat binds the exact Codex thread (`codex-session` now emitted by `tuic-hook --codex-session` instead of the inline `base64` shell hook).
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

- [ ] Reach a real `API Error: 500 Internal server error. This is a server-side _(NOT VERIFIED 2026-09-29: Needs a real Claude Code API 500 server-side error message)_ _(audit 2026-10-13: HUMAN — Requires genuine vendor server failure and live retry timing in logged-in Claude; no credential/provider sessions permitted…)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
  issue…` in a Claude tab. `GET http://localhost:9876/logs` shows
  `[ApiError] … pattern=claude-server-error-friendly kind=server` followed by
  `[AutoRetry] claude: attempt 1/3 in 5s`.
- [ ] The tab does NOT play the error sound and does NOT show the red awaiting _(NOT VERIFIED 2026-09-29: Needs real Claude prose 5xx error message and retry timing.)_ _(audit 2026-10-13: HUMAN — Requires genuine vendor server failure and live retry timing in logged-in Claude; no credential/provider sessions permitted…)_ _(triage 2026-10-14: WAITS — Depends on the preceding genuine Claude vendor 5xx failure.)_
  badge while a retry is pending — only after the 3rd attempt is exhausted.
- [ ] `continue` is injected after 5s and the turn resumes. _(NOT VERIFIED 2026-09-29: Needs real Claude Code emitting its prose 5xx error and auto-continue)_ _(audit 2026-10-13: HUMAN — Requires genuine vendor server failure and live retry timing in logged-in Claude; no credential/provider sessions permitted…)_ _(triage 2026-10-14: WAITS — Depends on the preceding genuine Claude vendor 5xx failure.)_
- [ ] With auto-retry disabled for Claude, the same error sets the red badge _(NOTE 2026-09-29: partial evidence only — Parser case for Claude's prose 5xx message at output_parser.rs:3843; disabled-retry branch is code inspection of auto-retry setting (verify in useAgentPolling/retry handler).)_ _(audit 2026-10-13: HUMAN — Requires genuine vendor server failure and live retry timing in logged-in Claude; no credential/provider sessions permitted…)_ _(triage 2026-10-14: WAITS — Depends on the preceding genuine Claude vendor 5xx failure.)_
  immediately and injects nothing.
- [ ] The message wraps across terminal rows (narrow the window before it _(NOTE 2026-09-29: partial evidence only — Detection covered by pty tests using API_ERROR fixture (pty/tests.rs:7310); wrapped-row case needs a real narrow agent tab if wanted.)_ _(audit 2026-10-13: HUMAN — Requires genuine vendor server failure and live retry timing in logged-in Claude; no credential/provider sessions permitted…)_ _(triage 2026-10-14: WAITS — Depends on the preceding genuine Claude vendor 5xx failure.)_
  fires): detection still happens — the pattern anchors on `API Error: 5xx`.
- [ ] A 429/overload (`API Error: 529` or "temporarily limiting requests") is _(NOTE 2026-09-29: partial evidence only — Detection in tuic-terminal/src/output_parser.rs:497-502 (rate_limit/overloaded/'limiting requests'), classify_error in tuic-core error_classification.rs:27; needs test cite or fixture replay.)_ _(audit 2026-10-13: HUMAN — Requires genuine vendor server failure and live retry timing in logged-in Claude; no credential/provider sessions permitted…)_ _(triage 2026-10-14: WAITS — Needs a genuine Claude overload/rate-limit response.)_
  still logged as a rate limit, not as a server error, and injects nothing.
## Usage ticker follows the agent in the terminal (Claude / Codex / Grok)

**The Rust restart is complete.** The active debug backend on :9876 was verified
on 2026-09-19 with the Codex App Server and Grok ACP integrations loaded.

**Precondition:** Settings → Agents → the Claude Usage toggle must stay enabled;
it now drives all supported usage providers. Codex and Grok must be logged in
through their own CLIs.

- [ ] Log Codex out through the Codex CLI and focus a Codex tab: the ticker _(NOT VERIFIED 2026-09-29: Needs real Codex CLI login/logout)_ _(audit 2026-10-13: VISUAL-OWED — Rendering/timing requires images or real pointer interaction; image viewing and MacControl unavailable. No screenshots attempted. Source: to-test.md:1759)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
  reports the authentication failure without displaying a cached reading.
### Codex Usage Dashboard

The active backend now includes `get_codex_usage_stats` and `GET /codex/stats`.

- [ ] Kill the network and open the dashboard: a cached snapshot is used only _(NOT VERIFIED 2026-09-29: Needs Codex account usage dashboard and network cut-off.)_ _(audit 2026-10-13: HUMAN — Requires interaction with Boss-owned active app/repository or vendor/provider process; no ownership of that state. Contract test/source search performed…)_ _(triage 2026-10-14: WAITS — Requires network-loss observation without disconnecting Boss or other agents.)_
  for up to 30 minutes; authentication failures are always surfaced.
### Grok Usage Dashboard

The active backend now includes `get_grok_usage_api` and `GET /grok/usage`.

- [ ] A logged-out Grok CLI surfaces an authentication error instead of a stale _(NOT VERIFIED 2026-09-29: Needs real Grok CLI logged out)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:1800)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
  cached percentage.
## `index.lock` owner probe now fails closed (#694-4fcc)

**Rust — needs a `make dev` restart to take effect.** Unit-tested (28 passed), but
the live behaviour changed, so it is worth one look on a real repo.

Policy, decided by Boss 2026-09-07: when the `lsof` owner probe cannot answer, the
lock is **kept**, not reclaimed. The escape hatch is age at
`UNADJUDICATED_LOCK_STALE_SECS` (1 h), so a lock nothing can adjudicate is still
cleared eventually.

- [ ] Watch for a lock kept longer than it used to be during ordinary work. The _(NOT VERIFIED 2026-09-29: Observational over ordinary real-world work (lock retention latency); not a discrete check.)_ _(audit 2026-10-13: HUMAN — Requires interaction with Boss-owned active app/repository or vendor/provider process; no ownership of that state. Contract test/source search performed…)_ _(triage 2026-10-14: WAITS — Exact ordinary-work lock duration/DeadlineExceeded soak remains unverified; no owned workload available while rb is down. Cannot infer long-term contention absence from unit tests or disturb Boss workloads.)_
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

### Upstream MCP OAuth — concurrent flows, expiry, late redirect

**Requires a `make dev` restart** — all of this is Rust (`mcp_oauth/`,
`mcp_proxy/registry.rs`). The running instance still has the old serialized
behaviour.

- [ ] Settings → Services → MCP: click **Authorize** on two different upstreams _(NOT VERIFIED 2026-09-29: Needs real upstream MCP OAuth servers and external browser consent)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:1939)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
  back to back. Both show the consent dialog and open a browser tab within a
  second. Previously the second click hung silently for 5 minutes: no browser,
  no dialog, no error, while the row already read "Awaiting authorization…".
- [ ] Click **Authorize**, then **Cancel** before completing consent: the row _(NOT VERIFIED 2026-09-29: Needs a real upstream MCP OAuth server to reach 'Awaiting authorization' and complete/cancel consent in a browser.)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:1943)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
  leaves "Awaiting authorization…" immediately and Authorize works again on the
  next click (no queue built up behind it).
- [ ] Click **Authorize** and then do nothing for >5 minutes. The row returns to _(NOT VERIFIED 2026-09-29: blocked — Needs a mock OAuth upstream MCP server plus >5 min wait; not set up within time-box.)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:1946)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
  **Authorize to connect** (`needs_auth`) on its own, and
  `GET http://localhost:9876/logs?source=mcp_oauth` shows
  `Cleaned up expired OAuth flows` naming the upstream. It must not stay stuck
  on "Awaiting authorization…".
- [ ] Click **Authorize**, wait out the full 5-minute timeout *in the browser*, _(NOT VERIFIED 2026-09-29: Needs real upstream MCP OAuth provider, browser consent and 5-minute timeout with an external account.)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:1951)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
  then complete consent. The browser shows the TUIC "Authentication failed" card
  reading "This authorization request expired or was cancelled…" plus "press
  Authorize again" — **not** the browser's own "can't connect to the server"
  page.
- [ ] A normal successful authorization still lands on the green _(NOT VERIFIED 2026-09-29: Needs a real upstream OAuth MCP server and browser authorization flow (external service))_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:1956)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
  "Authentication complete" card and the upstream goes `ready`.
### Terminal: no grid wipe on tab switch, resubscribe on reattach (#657-4345)

Frontend only (`Terminal.tsx`) — Vite HMR picks it up, no `make dev` restart
needed. Canvas painting is not observable over HTTP, so these need eyes.

- [ ] Switch back and forth between two busy terminal tabs. The returning tab _(NOT VERIFIED 2026-09-29: blocked — Trusted and DOM clicks on terminal tabs do not switch tabs in this web session (known limit); also 'busy tab' repaint flash not visible without screenshots.)_ _(audit 2026-10-13: VISUAL-OWED — Rendering/timing requires images or real pointer interaction; image viewing and MacControl unavailable. No screenshots attempted. Source: to-test.md:1964)_ _(triage 2026-10-14: WAITS — Targeted terminal rendering evidence retained; exact returning-tab paint under live output requires native rendering observation unavailable with Screen Recording denied and locked desktop.)_
  shows its content immediately with no blank flash. Previously every switch ran
  `resubscribe()` + `refresh()`, which cleared the grid and repainted it
  (paint → wipe → paint).
- [ ] Detach a tab into a floating window, then close that window to reattach. _(NOT VERIFIED 2026-09-29: blocked — Desktop-only: detach tab to floating window and reattach needs native window interaction on the validate instance (maccontrol targets orchestrator, not this instance).)_ _(audit 2026-10-13: VISUAL-OWED — Rendering/timing requires images or real pointer interaction; image viewing and MacControl unavailable. No screenshots attempted. Source: to-test.md:1968)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
  The reattached tab still paints live output and scrolls — the grid channel is
  resubscribed on this path, which is the only path that still resubscribes.
- [ ] Open a terminal in a split pane, collapse the pane to zero width, leave it _(NOT VERIFIED 2026-09-29: blocked — Split pane creation/collapse needs drag/keyboard interactions that do not work in this web session.)_ _(audit 2026-10-13: VISUAL-OWED — Real canvas/vendor rendering or subjective visual observation unavailable. Related tests run where applicable but do not prove rendered native surface; no screenshot attempts.)_ _(triage 2026-10-14: WAITS — Exact native zero-width/return lifecycle needs isolated desktop observation; earlier component checks do not prove the visible resumed paint.)_
  collapsed for a minute. `GET http://localhost:9876/logs?source=terminal` shows
  one `Container stayed zero-size for 120 frames` warning and CPU stays flat.
  Previously that container kept a `requestAnimationFrame` loop re-arming every
  frame for the lifetime of the page, one loop per terminal, surviving unmount.
## Settings search (story 684-35a8, frontend — Vite HMR picks it up)

- [ ] Open Settings. A "Search settings" box now sits at the top of the left nav. _(NOT VERIFIED 2026-09-29: partial — Only measured at default nav 180px: search input 155px wide, 12px font, padding 24/22px, placeholder 'Search settings' fits (scrollWidth<=clientWidth). Nav resize handle drag via agent-browser mouse did not change width, so 140/280px not tested; no screenshot.)_ _(audit 2026-10-13: VISUAL-OWED — Rendering/timing requires images or real pointer interaction; image viewing and MacControl unavailable. No screenshots attempted. Source: to-test.md:1994)_ _(triage 2026-10-14: WAITS — Reproduced failure and linked story retained; regression verification awaits an integrated fixed build.)_ _(FAILED 2026-10-14: Search settings placeholder clips at140px nav:94px text /67px available;280px passes. Worklog: [684-35a8](stories/archive/684-35a8-complete-P3-settings-search-across-22-tabs.md). Evidence: ~/Gits/.tmp/results/to-test-1014/phase2-settings-width-failure.json.)_
  Check it reads well at the narrowest (140 px) and widest (280 px) nav widths —
  the box shares the nav's resize handle area, and only the DOM is covered by
  tests, not the rendering.
## GitHub poller survives a dropped connection (story 648-051b, Rust — needs `make dev` restart)

The shared HTTP client had no timeout at all, so a dropped VPN wedged the poller
on a socket the peer never answers.

- [ ] With the network still down, disable GitHub polling in Settings. It must _(NOT VERIFIED 2026-09-30: partial — Not run: depends on the network outage of 1760 and a Settings toggle (frontend).)_ _(audit 2026-10-13: HUMAN — Requires interaction with Boss-owned active app/repository or vendor/provider process; no ownership of that state. Contract test/source search performed…)_ _(triage 2026-10-14: WAITS — Needs owned outage/poller execution; github_poller.rs has stop_aborts_a_poll_that_never_resolves and stop_poller_reaches_a_loop_parked_inside_a_request, but remote SSH is currently unreachable. No Boss network/settings changed.)_
  stop immediately, not after the in-flight request gives up.
## Git status and index.lock ownership (story 673-19fa, Rust — needs `make dev` restart)

The sidebar dirty badge now reads the gix porcelain-v2 counts, and the stale
`index.lock` sweep asks `lsof` who owns the lock before trusting the age rule.

## Weekly advisory scan (story 663-feea, CI — verify after merge)

`audit.yml` now installs a prebuilt `cargo-audit` and reads its ignore list from
`src-tauri/.cargo/audit.toml`. The workflow only runs on Mondays or on demand,
so nothing local can prove the install step resolves.

- [ ] Trigger `audit.yml` manually (`gh workflow run audit.yml`) and confirm the _(NOT VERIFIED 2026-09-29: Needs GitHub Actions run (gh workflow run audit.yml) on the remote; external service, not local.)_ _(audit 2026-10-13: HUMAN — Requires interaction with Boss-owned active app/repository or vendor/provider process; no ownership of that state. Contract test/source search performed…)_ _(triage 2026-10-14: WAITS — Needs coordinator authorization for a live GitHub Actions dispatch.)_
  `Install cargo-audit` step resolves `taiki-e/install-action@cargo-audit` and
  the scan runs to completion.
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

- [ ] Desktop, normal path: fetch/pull/push from the Git panel still work and _(NOT VERIFIED 2026-09-30: partial — POST /repo/run-git fetch: reachable local remote success:true; hanging remote (silent TCP listener) -> after 180s 'git timed out after 180.0s and was killed'. Side finding: orphaned 'git remote-http' helpers (ppid 1) survived until the peer closed. Git panel UI/pull/push not driven.)_ _(audit 2026-10-13: HUMAN — Requires interaction with Boss-owned active app/repository or vendor/provider process; no ownership of that state. Contract test/source search performed…)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
  still report failures the way they did before. No visible change expected.
- [ ] Browser mode (`http://localhost:9876/`): do a fetch on a repo whose _(NOT VERIFIED 2026-09-30: partial — Same API-level evidence as 1828 through the local router (unix socket): reachable fetch ok, hang -> 'git timed out after 180.0s' at 180s. Browser UI not driven.)_ _(audit 2026-10-13: HUMAN — Requires interaction with Boss-owned active app/repository or vendor/provider process; no ownership of that state. Contract test/source search performed…)_ _(triage 2026-10-14: WAITS — Earlier actual reachable fetch and bounded hanging-remote API evidence retained; full browser Git-panel flow needs owned backend/repo runtime unavailable with rb SSH down. No Boss repo fetch issued.)_
  remote is reachable. It should behave exactly as on desktop.
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

- [ ] Open the web UI (browser, not the desktop app) on a session with _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs a browser attached to a scrolling session (wheel/drag on the canvas); headless instance has no frontend. 29/09 note: tab switching in the web UI failed for MCP-created tabs; server scroll-to-offset returns ok but display_offset stays 0 with no subscribed viewer.)_ _(audit 2026-10-13: VISUAL-OWED — Rendering/timing requires images or real pointer interaction; image viewing and MacControl unavailable. No screenshots attempted. Source: to-test.md:2234)_ _(triage 2026-10-14: WAITS — Headless browser-scroll API and canvas/component evidence retained; exact visible wheel/drag behavior needs reachable owned backend and trusted browser input. rb down and locked desktop pointer unavailable.)_
  scrollback and scroll with the wheel and by dragging the scrollbar: the
  viewport must move, not just the thumb.
- [ ] With that browser attached, close the same terminal's tab in the desktop _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs a browser-attached scrolling session AND a desktop instance to close the same tab; both frontends; headless instance has neither.)_ _(audit 2026-10-13: VISUAL-OWED — Live disposable HTTP-created PTY: 504 lines, screen_lines 50; scroll-to-offset 120 -> scroll-info offset120. scroll-probe.json. Browser painting/desktop-close half is VISUAL-OWED.)_ _(triage 2026-10-14: WAITS — Cross-client native tab close plus browser live teardown needs isolated desktop and owned backend; peers cannot launch native app, rb unavailable, Boss tabs untouched.)_
  app. The browser must keep scrolling — the unsubscribe no longer drops the
  session's scroll target.
## Opening a 23 MB JSON no longer freezes the editor (2026-09-06, frontend — HMR; one Rust part needs `make dev` restart)

Above 500 KB the editor is plain text: no highlighting, no git gutter, no inline
blame, and the disk poll no longer re-reads the whole file 5 s after opening.
`get_gutter_changes` (Rust) returns nothing for an untracked file instead of
one "added" marker per line. Measured in Chrome only; WKWebView is the one
that blocked for over a minute.

- [ ] Desktop app: open `~/Gits/personal/ego/mutants.out/mutants.json` (23 MB, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Opening a 23 MB JSON in the editor is frontend/desktop behaviour; instance has no frontend. Real mutants.json is 5 KB now (needs a synthetic 24 MB file in a registered repo).)_ _(audit 2026-10-13: VISUAL-OWED — Rendering/timing requires images or real pointer interaction; image viewing and MacControl unavailable. No screenshots attempted. Source: to-test.md:2249)_ _(triage 2026-10-14: WAITS — Large-file WKWebView responsiveness requires native desktop observation; no managed desktop launch and no Boss file/UI interaction.)_
  gitignored). It must open in a few seconds at most, unhighlighted, with no
  gutter markers. With `window.__TUIC__.setPerfDebug(true)` first, any
  remaining `UI freeze` line on `/logs` names an `editor.*` breadcrumb.
- [ ] Desktop app (WKWebView), after the fix that installs the document with _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: WKWebView desktop smoothness with a 23 MB file; engine-specific, needs the desktop build.)_ _(audit 2026-10-13: VISUAL-OWED — Rendering/timing requires images or real pointer interaction; image viewing and MacControl unavailable. No screenshots attempted. Source: to-test.md:2256)_ _(triage 2026-10-14: WAITS — WKWebView large-document responsiveness cannot be established by component/Vite checks; isolated desktop unavailable.)_
  `EditorView.setState` instead of a whole-document dispatch: the same 23 MB
  file must scroll smoothly, and a small file must still highlight, show its
  git gutter and its inline blame, and keep undo working across an external
  reload (edit the file from a terminal while the tab is open).
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

- [ ] [HUMAN] Install any of `amp`, `cursor-agent`, `goose` or `droid` and capture an _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2302)_ _(triage 2026-10-14: WAITS — Needs an installed supported external agent and real captured session.)_
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

- [ ] [HUMAN] Reinstall the TUIC hooks from Settings → Agents, then confirm _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2323)_ _(triage 2026-10-14: WAITS — Boss controls reinstalling native hooks in his agent profiles.)_
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

- [ ] [HUMAN] In a release `.app`, hold `j`/`l`/`i` in vim: the cursor repeats and no _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2335)_ _(triage 2026-10-14: WAITS — Needs native release-app held-key repeat observation.)_
  accent picker appears. Option-key composition must still produce accented
  characters, and a user with `defaults write -g ApplePressAndHoldEnabled -bool true`
  must keep their override (the registration domain is lowest priority). Needs the
  release bundle domain — `press_and_hold.rs`, called from the `lib.rs` setup — and
  real key-repeat hardware. (#79)
- [ ] [HUMAN] Settings → Notifications → Attention → Test in a rebuilt app: the native _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2341)_ _(triage 2026-10-14: WAITS — Needs Boss to hear the native attention sound.)_
  engine matches the sample Boss approved on 2026-08-09 (triangular G4→G4→E5,
  75/75/140 ms, 50 ms gaps, gain 0.8) and stays identifiable from another room
  without being irritating. Audio, judged by ear.
- [ ] [HUMAN] Copy a long Claude message out of the terminal and paste it into Slack: _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2345)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
  no `▎` gutter and no gutter NBSPs, while lists, blank lines, indentation, `:wave:`
  and the body spacing survive unchanged. The text itself is asserted by nine Rust
  tests (`cargo nextest -E 'test(copied_selection)'`, `tuic-terminal/src/terminal_grid.rs:1687`); the
  paste is not. Tried twice from automation — `agent-browser clipboard read` fails
  with `Resource temporarily unavailable (os error 35)`.
- [ ] [HUMAN] Drag a file out of the file browser onto Finder, and drop a large folder _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2351)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
  from Finder into the app. The first is a real cross-application OS drag. The second
  confirms a **deliberate** gap, not a regression: `fs_transfer_paths` (`fs.rs:1624`)
  is still synchronous on the main thread because it is the drag-and-drop backend and
  D&D changes need Boss's approval.
- [ ] [HUMAN] Install `zed`, put a comment, a trailing comma and hand-tuned indentation _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Requires restart/reload/freeze or network interruption prohibited by audit brief; not attempted. Source: to-test.md:2356)_ _(triage 2026-10-14: WAITS — Needs Zed installed and its user configuration fixture.)_
  in `~/.config/zed/settings.json`, install the bridge from Settings → Agents, and
  confirm Zed still starts, still shows every setting, and lists the `tuicommander`
  context server — `diff` against `<config dir>/mcp-backups/zed-settings.json.orig`
  must show only the added member. Then press **Remove all MCP integrations** and
  confirm each client lost only its `tuicommander` entry and a relaunch does not put
  it back. Zed is not installed here, and a real client reading the file afterwards
  is the one thing the splice tests cannot cover. (issue #115)
- [ ] [HUMAN] Compare OSC 133 gutter marks side by side, browser at `:9876` against the _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: VISUAL-OWED — Real canvas/vendor rendering or subjective visual observation unavailable. Related tests run where applicable but do not prove rendered native surface; no screenshot attempts.)_ _(triage 2026-10-14: WAITS — Side-by-side native canvas rendering requires available desktop capture; API extraction and renderer tests cannot prove appearance.)_
  desktop app, on the same session: same rows, same size, neither client stealing the
  other's dirty rows. Canvas painting is not observable over HTTP, and both clients
  have to be visible at once. (Port corrected 2026-09-07 from `:9877` — only one
  instance runs, and it serves 9876; a browser pointed at 9877 gets nothing.)
- [ ] [HUMAN] Open `vim` or `htop`: the wheel still goes to the app, `Shift+wheel` _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2369)_ _(triage 2026-10-14: WAITS — Actual trusted wheel and Shift+wheel routing in the native terminal needs desktop interaction; locked browser pointer cannot exercise this path.)_
  scrolls TUIC history, and quitting restores the shell scrollback unchanged. The
  enter/exit half is covered by the `gh-run-watch.raw` replay test; mouse-reporting
  forwarding needs a real wheel. `lazygit` is not installed.
- [ ] [HUMAN] Print a fullwidth char and overwrite half of it — `printf '\e[1;5H中'` _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: VISUAL-OWED — Real canvas/vendor rendering or subjective visual observation unavailable. Related tests run where applicable but do not prove rendered native surface; no screenshot attempts.)_ _(triage 2026-10-14: WAITS — Fullwidth overwrite renderer behavior has automated coverage retained; the requested native glyph/cursor appearance still needs desktop capture.)_
  then `printf '\e[1;6HX'` — and confirm no ghost `中` survives beside the `X`. Scroll
  away and back to prove it is not just hidden by a later full-row reship. Canvas
  painting.
- [ ] [HUMAN] Raise an MCP `ui action=confirm` and answer it on a phone at _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2377)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
  `/mobile.html`: the desktop dialog must disappear by itself, and the reverse must
  work too. Then, with a push subscription registered and the PWA closed, confirm the
  push carries the title. Needs a real phone and a real subscription.
- [ ] [HUMAN] Boss's call: **Trim** the real `src-tauri/target` row in Build Cleaner. It _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2385)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
  is 58 GiB and a Trim forces a full rebuild of the running dev app, so no agent may
  run it. Trim against other repos' `target/` is covered.
- [ ] Rust change, needs a `make dev` restart (story #650-b0a0). With the app started _(NOT VERIFIED 2026-09-29: Needs a valid external relay server URL/token and killing the relay server; external service.)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2399)_ _(triage 2026-10-14: WAITS — Needs an owned externally reachable relay daemon/server pair; rb unavailable and no Boss relay/network teardown authorized.)_
  while **Cloud Relay is off**, turn it on in Settings → Services with a valid relay URL
  and token: the status dot must go green with no app restart, and `GET :9876/logs`
  must show `relay: connecting to …`. Turn it off: the dot goes grey and the log shows
  `relay: shutting down` then `relay: stopped`. Turn it on again — the supervisor must
  still be watching after a stop. Then kill the relay server (or pull the network) while
  connected: every reconnect log must read `reconnecting in 1s` for the first attempt
  after each *successful* connection, growing 1→2→4… only across consecutive failures.
- [ ] Rust change, needs a `make dev` restart (story #654-bfc1). Put a stub earlier on _(NOT VERIFIED 2026-09-29: partial — Stub cannot take priority: resolve_cli probes /usr/local/bin then /opt/homebrew/bin (real gh there; code comment says a PATH stub is never picked up) and I may not touch system dirs. Observed on headless tuic-remote (PATH stub first, sandbox HOME): HTTP bound 0.28s after start, deferred probe logged off boot path. Not checked: 10s 'did not answer' )_ _(audit 2026-10-13: HUMAN — Requires restart/reload/freeze or network interruption prohibited by audit brief; not attempted. Source: to-test.md:2417)_ _(triage 2026-10-14: WAITS — Needs isolated backend startup with deferred GitHub token probe; targeted the_deferred_probe_nudges_the_poller_once_the_token_lands remains unexecuted while rb SSH is unavailable. No credential store read.)_
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
- [ ] Rust change, needs a `make dev` restart (story #642-3741). `mod dictation_routes` _(NOT VERIFIED 2026-09-29: partial: /dictation/status, /models, /devices, /config, /system/relay-status and /system/check-update?channel=nightly answer JSON (unix-socket router); browser dictation record/inject and output device need a microphone/speaker)_ _(audit 2026-10-13: HUMAN — Partial live JSON route proof: dictation-status/models/devices/config-live.json and system-relay-status-live.json over supported UDS…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
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
- [ ] Rust change, needs a `make dev` restart (story #670-b9a2). Grid delivery got three _(NOT VERIFIED 2026-09-29: Needs the desktop WebView with tauri::ipc::Channel, blocking the app JS thread via devtools; not reachable from HTTP/MCP on a test instance.)_ _(audit 2026-10-13: VISUAL-OWED — Rendering/timing requires images or real pointer interaction; image viewing and MacControl unavailable. No screenshots attempted. Source: to-test.md:2441)_ _(triage 2026-10-14: WAITS — Exact blocked-WebView/grid-channel observation requires an owned desktop; peers cannot launch it or block Boss UI. Prior backend regression evidence retained.)_
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
- [ ] Rust change, needs a `make dev` restart (story #672-c1a3). Three always-on _(NOT VERIFIED 2026-09-29: partial — AI cron scheduler obsolete (no ai/scheduler in src). Knowledge persists: ai-sessions/*.json written. tuic.log.2026-09-29 grows. Own daemon SIGINT: 'Received shutdown signal' line flushed to log, process exited. UTC-midnight rotation not observed (23:19 UTC).)_ _(audit 2026-10-13: HUMAN — Requires interaction with Boss-owned active app/repository or vendor/provider process; no ownership of that state. Contract test/source search performed…)_ _(triage 2026-10-14: WAITS — Scheduler loop was removed; surviving knowledge/log flush checks queued in boot-cost-contracts.log never started because rb SSH timed out (exit69). Await restored remote runner; no pass inferred.)_
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
- [ ] Rust change, needs a `make dev` restart. Dictation speech gates: the transcriber _(NOT VERIFIED 2026-09-29: Needs real microphone silence/speech and Whisper gating.)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2469)_ _(triage 2026-10-14: WAITS — Needs the named native platform/CI runtime; this machine is macOS.)_
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
- [ ] Rust change, needs a `make dev` restart. Per-tab agent resume (issue #119): with _(NOT VERIFIED 2026-09-29: Needs several real Claude tabs in one folder to verify per-tab session discovery)_ _(audit 2026-10-13: HUMAN — Real per-tab resume across app restart and registry/config-dir checks not performed; restart forbidden and credential-store boundaries apply. No runtime pass claimed.)_ _(triage 2026-10-14: WAITS — Exact process-identity/resume evidence retained; whole multi-live-tab reconnect check needs owned vendor sessions and rebuilt lifecycle. rb unavailable, no Boss session restarted.)_
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
- [ ] **Rust change — needs a `make dev` restart** (or `make build`). Session state is _(NOT VERIFIED 2026-09-29: partial — /events SSE (unix socket) emits 'session-state-changed' {session_id,state{awaiting_input,shell_state,last_activity_ms}} only on real transitions (11 events for 2 shell commands: busy/idle). Browser resource entries: no /sessions or list_active_sessions polling in 15s (hidden tab); diagnostics HEALTH lines show no extra IPC field. Awaiting badge wit)_ _(audit 2026-10-13: VISUAL-OWED — Rendering/timing requires images or real pointer interaction; image viewing and MacControl unavailable. No screenshots attempted. Source: to-test.md:2513)_ _(triage 2026-10-14: WAITS — Needs owned actual state producer/SSE observation and frontend consumer; isolated rb daemon unavailable. Prior source/event test evidence retained without live-event claim.)_
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
- [ ] **Show scrollbar marks toggle** (719-36af) — frontend only, so Vite HMR _(NOT VERIFIED 2026-09-29: partial — Toggle 'Show scrollbar marks' present after Block folding (Expert on), searchable ('scrollbar' -> Terminal>Terminal), persists in config.json false/true and after page reload. Canvas tick gating/Cmd+F ticks not observable; restart not done. Item says General>Terminal; it's Terminal tab.)_ _(audit 2026-10-13: VISUAL-OWED — Rendering/timing requires images or real pointer interaction; image viewing and MacControl unavailable. No screenshots attempted. Source: to-test.md:2555)_ _(triage 2026-10-14: WAITS — Reproduced failure and linked story retained; regression verification awaits an integrated fixed build.)_ _(FAILED 2026-10-14: history ticks remain after OFF until the next frame; search ticks and Settings navigation/layout pass. See [723-6b02](stories/archive/723-6b02-complete-P3-show-scrollbar-marks-also-hides-search-match-ticks.md); evidence: ~/Gits/.tmp/results/to-test-1014/phase2-scrollbar-marks-failure.json.)_
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

- [ ] **Headless daemon serves Claude usage again** (678-9a75) — **Rust change, _(NOT VERIFIED 2026-09-29: partial — Ran built tuic-remote (--no-default-features build in instance target, --instance ag1remote, TUIC_PORT=9891, own TMPDIR): starts; unix-socket GET /claude/usage reaches handler (500 upstream 'OAuth access token has expired', env not code), /claude/timeline 400 (needs params), /claude/session-stats 400, /claude/projects 200 - not 404. TCP needs auth )_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2604)_ _(triage 2026-10-14: WAITS — Needs isolated headless daemon with a throwaway authenticated Claude profile; rb unavailable and credentials cannot be read. Prior profile-isolation checks retained.)_
  needs a rebuild.** `claude_usage_cache` carried `#[cfg(feature = "desktop")]`
  while `build_router` mounts `/claude/usage` and `/claude/usage/timeline`
  unconditionally, so `cargo build --bin tuic-remote --no-default-features` did
  not compile at all. The gate is gone. After a rebuild, start `tuic-remote` and
  check `curl http://127.0.0.1:<port>/claude/usage` answers instead of 404/500.
  Desktop behaviour must be unchanged — the same endpoint on :9876 still works.
## Linked worktrees start WARM (story `767-3968`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [ ] Time it. Expect ~38 s on this repo; if it feels worse than a cold build, _(NOT VERIFIED 2026-09-30: partial — Not run on the tuicommander repo (would create a worktree in Boss's real repo; forbidden). Analog: 200-file ignored dir fixture warmed in under 1s; 29/09 analog with 60k files ~13s. ~38s expectation on the real repo not measured.)_ _(audit 2026-10-13: HUMAN — Real main worktree creation exercised and pending->done observed, but elapsed warming not instrumented. Recycled clean archived checkout, so cannot claim cold 38s performance…)_ _(triage 2026-10-14: WAITS — Needs coordinator-owned benchmark/mutation scheduling; no expensive batch gate started by this peer.)_
      say so rather than living with it.
## Progress rewritten to one journal, one database and a dialog — needs a `make dev` restart

The whole Progress feature was re-implemented against the 2026-09-14 revision of
`plans/project-progress.md`: one append-only journal in a single database at
`<config dir>/progress.sqlite3`, two reportable kinds plus a host-written
`intent`, a dialog replacing the sidebar panel, eight `repo progress_*` actions
cut, and no Markdown export. Rust and frontend both changed, so the running
build has the old behaviour until restart.

- [ ] After the restart has proved the new store works, delete the stale _(NOT VERIFIED 2026-09-30: partial — Real deletion left to Boss (destructive on ~/Gits). Ran the two exact find commands on a simulated tree (a/.tuic with progress.sqlite3{,-wal,-shm}; b/.tuic with progress.sqlite3 + tunnels/t.json; c/x/.tuic): files deleted, empty .tuic dirs removed, b/.tuic/tunnels kept. Read-only count of real match)_ _(audit 2026-10-13: HUMAN — Destructive stale database cleanup across ~/Gits exceeds audit authority; no files deleted. Requested action remains Boss/coordinator-owned, not a test pass.)_ _(triage 2026-10-14: WAITS — Deleting Boss’s stale state is outside this documentation-only assignment.)_
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
- [ ] Run diff-scoped mutation testing once on the final HEAD of this batch: _(NOT VERIFIED 2026-09-30: partial — Not run: diff-scoped mutation testing (make mutants) is an overnight cargo job (~5 min/mutant) and cargo/build is forbidden for this pass. Orchestrator batch task.)_ _(audit 2026-10-13: HUMAN — Overnight diff-scoped mutation gate remains coordinator-owned; no final batch range supplied. No cargo/mutants run, no gate pass claimed.)_ _(triage 2026-10-14: WAITS — Needs coordinator-owned benchmark/mutation scheduling; no expensive batch gate started by this peer.)_
      `make mutants RANGE=<commit before the Progress rewrite>`. It is an
      overnight-class job (~5 min per viable mutant), so it is deliberately not
      run during the day — 780-e99a criterion 4.
## File pickers moved off `tauri-plugin-dialog` — needs a `make dev` restart

The app died on 2026-09-18 when `+[NSOpenPanel openPanel]` returned NULL after
the window-server connection was interrupted, panicking on the main thread. The
pickers now go through our own `pick_path` command, which owns the main-thread
closure and catches that unwind (`src-tauri/src/native_dialog.rs`). Automated
tests cover the wire contract and the wrapper's mapping; the panels themselves
need a window server, so these are by hand.

- [ ] **[HUMAN]** The crash path itself: let the Mac sleep with the display off, _(NOT VERIFIED 2026-09-30: partial — Not run: needs real Mac sleep with display off plus the desktop app picker; sleeping the shared host Mac would disturb other agents. No audio hardware involved.)_ _(audit 2026-10-13: HUMAN — Real standby/wake crash check forbidden by no sleep/restart/freeze live app constraint; no Mac sleep attempted.)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
      wake it, and immediately open a picker. It must either open, or show the
      "system file dialog is unavailable" error — the app must NOT exit. This
      needs real standby, which no automated check here can reach.
## Remote Machines authentication (#781-9652) — needs a `make dev` restart

The whole change is Rust plus the transport layer, so nothing here is live in
Boss's running session. Restart first. A daemon to test against is already up:
`mac-mint:9877`, user `stefano`, systemd user unit `tuic-remote`, running a
headless build of this tree.

- [ ] **[VISUAL]** The password field and the vault hint render inside the _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_ _(audit 2026-10-13: VISUAL-OWED — Password/vault form and remote-picker layout are purely visual; screenshot tools unavailable per brief, no screenshot attempts.)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
      add/edit form without breaking the Settings layout.
## The connection runtime moved to Rust (#790-ef85) — needs a `make dev` restart

The health probe, the token exchange, the 5s status poll and the SSH tunnel now
run in the backend; `remoteConnections.ts` only renders what the backend pushes.
Every item below must behave exactly as it did before the move — that is the
point of the story — plus the two things only the backend can do.

- [ ] Quit TUICommander with an SSH-transport connection live. No orphan `ssh` _(NOT VERIFIED 2026-09-30: blocked — Second physical machine (SSH transport to mac-mint) plus quitting the desktop app; neither available headless.)_ _(audit 2026-10-13: HUMAN — App quit with actual SSH connection requires Boss own window/restart and SSH credentials. Shared teardown audited/tested, but process-exit orphan sweep cannot be exercised under no-kill/no-restart…)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
      process and no `__remote_<id>` profile file is left behind.
## The daemon is a whole machine — needs a real `tuic-remote` run (#23a5)

Run on a *separate* box, or on this Mac with `--instance <id>` and after
checking the note below. `tuic-remote` now writes an MCP entry into the config
of every agent installed on the machine it runs on — including this one, whose
agent configs Boss uses. Running an unisolated daemon here rewrites them with
the path it resolves for `tuic-bridge`.

- [ ] `repo action=worktree_list` from that agent answers about the daemon's _(NOT VERIFIED 2026-09-29: partial — Own local tuic-remote daemon: MCP repo action=worktree_list via a bridge on the daemon socket answers from the daemon process (daemon 'repo list' = [] vs validate instance list [fx/repo]); same filesystem so cannot show 'daemon repos not the Mac's' and no real agent/bridge listing tools on a separate host.)_ _(audit 2026-10-13: HUMAN — Daemon-versus-Mac filesystem ownership needs real separate daemon/repo registration or isolated startup; no separate daemon launched…)_ _(triage 2026-10-14: WAITS — Actual separate-host ownership needs a reachable isolated rb daemon and bridge caller. Earlier daemon-local list evidence retained; rb SSH currently unavailable.)_
      repos, not the Mac's.
- [ ] On a daemon with no agents installed at all: no agent config file and no _(NOT VERIFIED 2026-09-29: partial — Own tuic-remote, empty HOME, no --instance: only codex/grok/opencode/VSCode configs created; no .claude/.cursor/.gemini etc. (skip-if-not-installed works). Cannot test zero agents: has_cli finds /opt/homebrew/bin/{codex,opencode,code}, ~/.grok/bin on this machine, so those count as installed.)_ _(audit 2026-10-13: HUMAN — Required separate daemon/startup/installed-agent config or one-hour soak not run; would require launching and possibly rewriting agent configs beyond disposable live-session scope…)_ _(triage 2026-10-14: WAITS — Needs isolated zero-installed-agent startup fixture; existing absolute installed-CLI probes prevent proving this by PATH alone. Remote runner unavailable; no user agent config rewritten.)_
      agent config directory is created.
## The embedded AI engine is gone — needs a `make dev` restart (#784-0aec)

~24k lines of Rust and ~44 frontend files were deleted. Nothing below is a new
feature: each item confirms that removing the engine did not take a *surviving*
feature with it. All of it needs the rebuilt backend, so run it after the
restart, not before.

- [ ] SSH Tunnels still opens — it shares that master toggle. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: UI check that SSH Tunnels still opens under the experimental master toggle; no frontend here.)_ _(audit 2026-10-13: VISUAL-OWED — Panel/popover/dashboard/detached-window/transcript runtime presentation not observed; no screenshot access, no live UI mutation…)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
- [ ] A terminal's command knowledge still records: run a failing command, then _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Headless tuic-remote deliberately does NOT run ai_agent::knowledge::spawn_persist_task (lib.rs:2627 comment); on both rust0930 and r3iso, failing+passing commands in a shell session produced no ai-sessions/*.json. Needs the desktop build (persist task) to check recording and restart survival.)_ _(audit 2026-10-13: HUMAN — Session knowledge restart-survival requires app restart, expressly forbidden. Disposable shell used only for redaction; no knowledge persistence pass claimed.)_ _(triage 2026-10-14: WAITS — Headless host deliberately omits desktop knowledge persistence task; whole check needs isolated desktop lifecycle. Related knowledge flush test never started due remote SSH timeout.)_
      a passing one, and confirm the session's knowledge survives a restart
      (this is the one part of `ai_agent/` that was kept).
- [ ] The AI Chat panel still detaches into its own window and the main window _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Detached AI Chat window + 'Bring back' placeholder are desktop-only (PanelWindowControls).)_ _(audit 2026-10-13: VISUAL-OWED — Panel/popover/dashboard/detached-window/transcript runtime presentation not observed; no screenshot access, no live UI mutation…)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
      shows the *Bring back* placeholder; closing the detached window restores
      the docked shell.
## Remote repo browser (2026-09-20) — frontend only, Vite HMR picks it up

`RemoteRepoPicker` replaces the "type the absolute path" prompt when adding a
repository from a connected machine. No Rust changed, so HMR is enough — but
nothing here is reachable until a remote connection reads **Connected**, which
needs the `make dev` restart that #781-9652 is waiting on.

- [ ] With **no** machine connected, the sidebar `+` must behave exactly as before: _(NOT VERIFIED 2026-09-29: partial — Web UI, machine disconnected (status []): sidebar 'Add Repository' opens a single popover with a path text input (Cancel/Add), no menu/picker. Browser mode cannot show the native dialog, so 'local native dialog' equivalence not checkable; picker absent as required.)_ _(audit 2026-10-13: VISUAL-OWED — nativeDialog.test.ts path mapping PASS; actual sidebar native dialog/no-menu flow needs native window observation unavailable. No click in Boss sidebar.)_ _(triage 2026-10-14: WAITS — Sidebar.tsx:148-157 calls onAddRepo directly with no connected remote; prior browser no-menu check and nativeDialog test retained. Actual native file dialog remains unobserved.)_
      straight to the local native dialog, no menu. The picker must not appear.
## A dropped request must not leak an ego process (#804-2ec8)

Rust change — needs a `make dev` restart (or `make build`) to load.

`/acp/one-shot` was awaited inline under the router's 301s timeout, which drops
the handler future. Everything after the drop was skipped, including the
`disconnect` that stops ego, and nothing else ever would: the supervisor is an
independent task and only settled connections are pruned. The turn now runs on
its own task, the launch has its own 60s budget and the turn 240s, so the whole
call fits inside the router's bound.

- [ ] A normal Smart Prompt still answers, and a long one that runs out of time _(NOT VERIFIED 2026-09-29: Needs real ego and a 240 s timeout turn (oneshot.rs:333).)_ _(audit 2026-10-13: HUMAN — Actual ego turn abandonment/240s timeout needs controlled provider process; not exercised on Boss ACP config. Code inspected acp/oneshot.rs:270-283 detached task, :313-342 bounded drive/disconnect…)_ _(triage 2026-10-14: WAITS — Whole check requires actual ego one-shot success/timeout and child cleanup in owned backend. Remote runner unavailable; no long live model turn or timeout pass claimed.)_
      reports "ego did not finish the turn within 240s" rather than a bare 408.
## Terminal stream compression is acknowledged, not assumed (#805-f52e)

Rust and frontend — the Rust half needs a `make dev` restart (or `make build`).

The browser used to decide every frame was tagged from its own request alone. A
daemon that predates `?compress=deflate` ignores it and sends untagged frames,
and the client then read the first byte of a grid row as a tag. The server now
selects the `tuic.deflate` subprotocol when it is going to tag, and the client
reads `ws.protocol` in `onopen` before the first frame.

- [ ] Open a terminal through an **SSH tunnel**. The frames must be tagged but _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Source canonical loopback ws_compression.rs:122 inspected; actual SSH compression/daemon CPU needs controlled remote SSH credentials, forbidden to type. No artificial CPU load or live tunnel changed.)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
      never deflated (`ssh -C` already compressed the channel), which is the
      `::ffff:127.0.0.1` case the canonical-address fix covers. Check CPU on the
      daemon stays flat while an agent repaints.
## The headless build announces repo-op progress too (#808-84e1)

Rust — needs a `make dev` restart, and the interesting half needs the headless
binary: `cargo build --bin tuic-remote --no-default-features`.

**Which binary:** `run_headless` (the main binary's headless mode), NOT
`tuic-remote`. The three routes are in `build_router`; `build_remote_router`
does not carry them, so `tuic-remote` answers 404 (#810-4986).

- [ ] Start the headless mode of a `--no-default-features` build, open the web _(NOT VERIFIED 2026-09-29: Needs PR review (ego/GitHub account) on a --no-default-features headless build.)_ _(audit 2026-10-13: VISUAL-OWED — Bus half PASS via three --no-default-features rb tests (rust.log/rust-contracts.log); actual finding/proposal/conflict panel rendering is owed…)_ _(triage 2026-10-14: WAITS — Dual-transport source and focused frontend event tests retained; actual headless review-progress delivery needs owned daemon plus real ego/GitHub fixture. rb unavailable.)_
      UI against it, and start a PR review on a repo. The Review findings column
      must move from "running" to a result on its own. Before this commit it
      stayed on "running" forever, because the `review-progress` event was
      dropped on that build.
- [ ] Same daemon, run an improvement scan. The proposals must appear in the _(NOT VERIFIED 2026-09-29: Improvement scan runs on ego against a real daemon; needs ego)_ _(audit 2026-10-13: VISUAL-OWED — Bus half PASS via three --no-default-features rb tests (rust.log/rust-contracts.log); actual finding/proposal/conflict panel rendering is owed…)_ _(triage 2026-10-14: WAITS — Actual proposals-progress daemon/web flow needs isolated backend/model/GitHub fixture; rb SSH unavailable. Existing event/render tests do not prove a live scan.)_
      panel when the scan finishes — the return value never populates it, only
      the `proposals-ready` event does.
- [ ] Same daemon, run conflict assist on a PR with conflicts. The status must _(NOT VERIFIED 2026-09-29: Needs a real GitHub PR with conflicts, conflict assist (ego) on a headless daemon.)_ _(audit 2026-10-13: VISUAL-OWED — Bus half PASS via three --no-default-features rb tests (rust.log/rust-contracts.log); actual finding/proposal/conflict panel rendering is owed…)_ _(triage 2026-10-14: WAITS — Actual conflict-assist event flow requires owned conflicting PR/backend/model fixture; remote runner unavailable. Prior store/source evidence retained without live operation claim.)_
      reach the panel rather than leaving it idle.
- [ ] Desktop build, same three operations: unchanged. The window emit still _(NOT VERIFIED 2026-09-29: blocked — Needs PR review, improvement scan and conflict assist (ego + real GitHub PR with conflicts) on a headless daemon plus desktop panels; ego/GitHub PR not available headless.)_ _(audit 2026-10-13: VISUAL-OWED — Bus half PASS via three --no-default-features rb tests (rust.log/rust-contracts.log); actual finding/proposal/conflict panel rendering is owed…)_ _(triage 2026-10-14: WAITS — Actual dual emit delivery for review/scan/conflict in native window requires desktop runtime observation; managed peers cannot launch another desktop or exercise Boss sessions.)_
      fires, so nothing about the desktop panels may look different.
## The website names warm copy-on-write worktrees

- [ ] `website/index.html`, "Git worktrees, fully managed": the second bullet _(NOT VERIFIED 2026-09-29: partial — Rendered website/index.html in a same-origin srcdoc iframe (no screenshot). 1200px: 'Warm worktrees' bullet names copy-on-write, li 544x69, no overflow. 390px: bullet wraps (92px tall, code chips intact) but its column right edge is 402 > 390 viewport and document scrollWidth is 508 (several .feature-content blocks are 361-484px wide, all sections )_ _(audit 2026-10-13: VISUAL-OWED — No screenshot attempts per brief. Progress rendering source: src/components/ProgressDialog/ProgressDialog.module.css:3,14,45,57 (width/embedded/wrap/muted italic), ProgressDialog.tsx:281 intent row…)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
      now names the copy-on-write warming that `docs/user-guide/worktrees.md`
      documents. Checked at 1200px; check it on a phone width too.
## A missing bridge says so (#809-724c)

Rust — needs a `make dev` restart.

## Pocket TTS speech synthesis (#823-c260)

Rust — needs a `make dev` restart. Nothing calls the port yet (playback is
story 816), so the only way to reach it today is the bundle-backed tests:

```
TUIC_POCKET_BUNDLE_DIR=<bundle> [TUIC_POCKET_VOICE=<voice>] \
  cargo nextest run --lib --run-ignored ignored-only -E 'test(/dictation::speech::pocket/)'
```

- [HUMAN] Listen to `.tmp/kokoro-eval/ONNX/frase{1,2,3}-rust-int8.wav`, rendered _(audit 2026-10-13: HUMAN — Listening judgment and Windows/Linux native ONNX loading require real audio/cross-platform hosts; no hardware/listening or cross-target runtime claim.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      by this adapter, against the `-torch-fp32`, `-onnx-fp32` and `-onnx-int8`
      sets from the evaluation. Two questions, one listening pass: does the Rust
      port sound like the Italian that was approved, and is int8 (125 MB per
      language) good enough against fp32 (400 MB)? The second answer decides what
      the downloader in story 813 offers.
- [ ] Windows and Linux: the adapter loads `onnxruntime.dll` / `libonnxruntime.so` _(NOT VERIFIED 2026-09-29: Requires Windows and Linux hosts for onnxruntime library loading.)_ _(audit 2026-10-13: HUMAN — Listening judgment and Windows/Linux native ONNX loading require real audio/cross-platform hosts; no hardware/listening or cross-target runtime claim.)_ _(triage 2026-10-14: WAITS — Needs the named native platform/CI runtime; this machine is macOS.)_
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

- [ ] Hands-free arm/disarm over HTTP, against a throwaway agent session: _(NOT VERIFIED 2026-09-29: partial — Shell session arm -> {error:'Session cannot accept hands-free input'} verified; agent-typed session (spawned fake agent_type=claude) passes that gate and fails 'Model not downloaded'. armed:true/sessionId echo not reached (needs whisper model+mic; not arming to avoid mic prompt). disarm returned wasArmed:false (armed state never entered); GET consi)_ _(audit 2026-10-13: HUMAN — Microphone arm/speech/device-switch/indicator/push-to-talk checks require hardware and may take global microphone; no arm call made…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      `curl -X POST localhost:9877/dictation/hands-free/arm -H 'content-type: application/json' -d '{"sessionId":"<id>","owner":"desktop"}'`
      must return `armed: true` with `sessionId` echoed back. Arming against a
      shell (non-agent) session must return `Session cannot accept hands-free
      input`. `GET /dictation/hands-free` must agree with what arm returned, and
      `POST /dictation/hands-free/disarm` must report `wasArmed: true` once and
      `wasArmed: false` on a second call.
- [ ] Arm, then speak one short Italian sentence and stop. Within about a second _(NOT VERIFIED 2026-09-29: Needs real microphone speech input (Italian sentence))_ _(audit 2026-10-13: HUMAN — Microphone arm/speech/device-switch/indicator/push-to-talk checks require hardware and may take global microphone; no arm call made…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      of the pause the text must appear as a `voice_command` entry in
      `GET /sessions/{id}/queue`, and reach the agent on its next idle window.
      `GET /dictation/hands-free` must walk `waiting` → `capturing` →
      `transcribing` → `holding_back` → `delivered` across the turn; a phase that
      never leaves `capturing` means end-of-speech was not detected.
- [ ] Arm, then close the bound session from the UI. The mode must disarm itself _(NOT VERIFIED 2026-09-29: Needs microphone hands-free arm and real bound session.)_ _(audit 2026-10-13: HUMAN — Microphone arm/speech/device-switch/indicator/push-to-talk checks require hardware and may take global microphone; no arm call made…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      with `TargetClosed` and release the microphone without a disarm call —
      check `GET /dictation/hands-free` reads `armed: false` and that the app log
      carries `Hands-free disarmed: TargetClosed`. This is the path that keeps a
      dead tab from holding the device open.
- [ ] Arm, then unplug or switch away the input device. After the silence _(NOT VERIFIED 2026-09-29: Needs physically unplugging/switching real input device)_ _(audit 2026-10-13: HUMAN — Microphone arm/speech/device-switch/indicator/push-to-talk checks require hardware and may take global microphone; no arm call made…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      timeout the mode must disarm with `DeviceFailed` and name the device in the
      message, rather than sitting armed and deaf.
- [ ] Push-to-talk must be unaffected. With hands-free armed, run a normal _(NOT VERIFIED 2026-09-29: Needs real audio capture for push-to-talk plus hands-free.)_ _(audit 2026-10-13: HUMAN — Microphone arm/speech/device-switch/indicator/push-to-talk checks require hardware and may take global microphone; no arm call made…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      push-to-talk recording: it must capture and transcribe as usual, and must
      neither disarm hands-free nor be disarmed by it. Then disarm hands-free and
      confirm push-to-talk still works. The two modes hold separate captures.
- [HUMAN] Confirm the microphone indicator (menu bar / camera-mic dot) turns on _(audit 2026-10-13: HUMAN — Microphone arm/speech/device-switch/indicator/push-to-talk checks require hardware and may take global microphone; no arm call made…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      at arm and off at disarm, for every disarm path above. Nothing in the test
      suite can see whether the OS actually released the device, and an armed
      mode that leaks the microphone after disarm is the failure that matters
      most here.
## Hands-free activation phrase (#815-7c76)

_(NOTE 2026-09-23: superseded — hands-free turns and notices are now typed straight into the terminal, busy or not, and never enter the Compose queue; there is no `voice_command` kind, no `queuedIds`, no `cancelled`/`alreadyDelivered`. Read "reaches the Compose queue" as "is typed into the terminal"; a dialog or draft holds the turn in the hands-free panel. See "Hands-free turns reach a busy agent at once" at the top.)_

Rust — needs a `make dev` restart. **There is no UI control for the phrase**;
`DictationSettings.tsx` has no field for it, so the only way to set one today is
the config surface. A Dictation control belongs to story 818.

- [ ] With `hands_free_activation_phrase` empty, arm and speak: every recognised _(NOT VERIFIED 2026-09-29: Needs real spoken utterances through recognizer into Compose queue.)_ _(audit 2026-10-13: HUMAN — Explicit speech-through-recognizer path needs real microphone input; not attempted. Deterministic gate tests inspected continuous.rs:2376,2446,2495,2572,2588,2612 but not run…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      utterance must reach the Compose queue exactly as it did before this
      story. An empty phrase must change nothing.
- [ ] Armed with that phrase, speak "che ore sono" alone: nothing must reach the _(NOT VERIFIED 2026-09-29: Needs spoken audio input.)_ _(audit 2026-10-13: HUMAN — Explicit speech-through-recognizer path needs real microphone input; not attempted. Deterministic gate tests inspected continuous.rs:2376,2446,2495,2572,2588,2612 but not run…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      queue. Then "attività tuic che ore sono": the queue must receive
      `che ore sono` with the phrase stripped. Then, within 15 seconds, speak a
      bare follow-up: it must go through without the phrase. Wait past 15 seconds
      and the phrase must be required again.
- [ ] Say "tuicommander che ore sono" with the phrase set to `tuic`: it must NOT _(NOT VERIFIED 2026-09-29: Needs real spoken audio input with activation phrase)_ _(audit 2026-10-13: HUMAN — Explicit speech-through-recognizer path needs real microphone input; not attempted. Deterministic gate tests inspected continuous.rs:2376,2446,2495,2572,2588,2612 but not run…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      activate. A longer word that merely starts with the phrase is a different
      word, not a prefix match.
- [ ] Disarm and re-arm while a window is open: the first utterance after the _(NOT VERIFIED 2026-09-29: Needs real speech through a microphone to test the activation phrase window.)_ _(audit 2026-10-13: HUMAN — Explicit speech-through-recognizer path needs real microphone input; not attempted. Deterministic gate tests inspected continuous.rs:2376,2446,2495,2572,2588,2612 but not run…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      fresh arm must need the phrase again. Every disarm closes the window.
## Resume banner names the work

- [ ] Leave an agent tab running with a declared `intent:` (or a typed prompt), _(NOT VERIFIED 2026-09-29: Needs real agent tab with declared intent and app quit/reopen resume banner.)_ _(audit 2026-10-13: VISUAL-OWED — Code path exists Terminal.tsx:1233-1238 intent-first/prompt fallback, :1320-1322 tooltip; restore src/hooks/git/createBranchSelectionCoordinator.ts:159-162…)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
      quit the app, reopen it and select that branch. The
      "Agent session was active — click to resume" banner must now carry
      `Intent: <...>` (or `Prompt: <...>` when no intent was declared), truncated
      with an ellipsis and with the full text in the tooltip. A tab that never
      had either must show the banner exactly as before.
## Echo canceller starts with the app

Needs a `make dev` restart — the Rust backend does not hot-reload.

- [ ] Startup must not be visibly slower. `DictationState::new()` now builds one _(NOT VERIFIED 2026-09-29: partial — desktop.log: first log 22:39:23.595, HTTP TCP+unix listening 22:39:24.617 (~1.0s incl. Tailscale detection 0.5s), key monitors 22:39:25.7. No AEC3-less baseline build to compare, so 'not slower' is not proven; nothing looks slow.)_ _(audit 2026-10-13: HUMAN — Startup comparison needs pre/post AEC baseline and restart; restart forbidden. Retained logs cannot prove not slower.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      AEC3 instance for the life of the app, before any dictation is used.
## Spoken replies and the `voice` MCP tool (story `817-f67c`, 2026-09-22) — **Rust, needs a `make dev` restart**

Everything below needs an installed Italian bundle, because arming without one
opens the conversation **without** a voice. That is no longer a blocker: the
`speech-voices-v1` release was cut on 2026-09-22, so install the bundle through
the download items above first, then work through these.

- [ ] Arm hands-free against a throwaway session on the restarted build, then _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/status need desktop build; WS audio client can arm without a mic (Italian bundle must be installed).)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      `curl 'localhost:9877/dictation/speech/status'`. `available` must be
      `true`, `sessionId` must be that session, and `voice` must name the
      installed Italian voice.
- [ ] `curl -X POST localhost:9877/dictation/speech/speak -H 'content-type: _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. POST speak -> state queued checkable there; 'audible in Italian' is speaker-hardware (blocked).)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      application/json' -d '{"text":"Ciao, sto parlando."}'` must answer
      `state: "queued"` with an `utteranceId` — never `"finished"`. Listen: the
      reply comes out of the speaker in Italian.
- [ ] Poll `GET /dictation/speech/status?utterance=<id>` while it plays. It must _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Status state walk checkable there; 'finished only after last word audible' needs ears.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      walk `queued` → `rendering` → `speaking` → `finished`, and only reach
      `finished` **after** the last word is audible.
- [ ] **Barge-in.** Queue a long reply, then start talking over it. The speaker _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Barge-in could be simulated by feeding speech audio on the WS; audible stop needs ears.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      must stop within a beat, the utterance must report `interrupted` rather
      than `finished`, and `turn` must have advanced. What you said must arrive
      in the terminal as a new turn — not appended behind the reply.
- [ ] `POST /dictation/speech/stop` while a reply plays: silence immediately, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. POST /dictation/speech/stop turn/queue checks are API-only once armed; audible silence needs ears.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      the queue empties, and the returned `turn` is higher than before.
- [ ] Disarm while a reply is playing. The audio must stop, and a `speak` after _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Disarm then speak refusal is API-only once armed via WS; audio stop needs ears.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      that must be refused with "Hands-free is not armed".
- [ ] From a **second** terminal's Claude Code, `voice action=status`: it must _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. voice tool via real claude reachable (see 3934); binding refusal needs an armed conversation on the desktop build.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      report the binding refusal, not the first conversation's queue. From the
      armed terminal's own Claude Code, `voice action=speak` must be heard.
## One language, end to end (story `822-7d7a`, 2026-09-22) — **Rust, needs a `make dev` restart**

The first four items need an installed Italian bundle, for the same reason as
the block above: without one there is no voice to listen to. The release that
serves it exists as of 2026-09-22, so install it first. The point of every one
of these items is the same — the
model must never answer in a language the user is not speaking.

- [ ] Set Dictation language to **Italian**, arm hands-free, and say something in _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs Italian language, armed conversation and real agent reply; drive via WS audio client on desktop build.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      Italian. The terminal entry must read `<what you said> (reply in Italian)`,
      on one line, and the agent must answer **in Italian**. Instruction
      delivery is not the proof: read the agent's reply.
- [ ] With the same setup, listen to the spoken reply. It must be the Italian _(NOT VERIFIED 2026-09-30: blocked — audio hardware: listening to the Italian TTS voice is speaker/ears; also headless build has no audio.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      voice reading Italian — not Italian text read by another language's voice,
      and not an English sentence.
- [ ] Still on Auto, say something in Italian. `status` must then report _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs spoken Italian/English input (WS audio) and real agent reply.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real armed audio and Italian/English speech with a model reply; prior unarmed status and language-contract evidence cannot prove this observation.)_
      `language: "it"`, and the entry must carry `(reply in Italian)`. Say the
      next turn in **English**: the entry must carry `(reply in English)` and the
      agent must switch with it.
- [ ] While a reply is being spoken, change the Dictation language in Settings. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs Settings UI language/RMS change during playback; audible cut needs ears.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      The speaker must stop mid-sentence and `status` must report the new
      language. Then change only the **RMS threshold** while another reply
      plays: that one must keep playing to the end.
- [ ] Turn the hands-free entry/exit hints **off** (story 821's setting, when it _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs hints-off setting in UI and armed entry.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      lands) and repeat the first item. The `(reply in …)` requirement must still
      be in the entry — it is not a hint.
- [ ] In the armed terminal's Claude Code, `voice action=status` must report _(NOT VERIFIED 2026-09-30: partial — Verified: voice tool inputSchema has only action,text,turn,utterance_id (no language/voice param). Not verified: status.language from an armed terminal (no dictation/arm in headless build).)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Schema already verified without language/voice inputs; armed desktop status needs an audio-enabled isolated build and real conversation.)_
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

- [ ] Arm hands-free on a Claude tab. The tab must receive a line saying voice _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      is on for this terminal, submitted as its own turn. Then say something
      ordinary and read the reply: the agent should either call the voice tool
      or explain why it cannot — **not** ignore the notice.
- [ ] Disarm. The tab must receive the "voice is off, reply as text" line, and _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      the next thing you type must be answered in text with no voice attempt.
- [ ] Arm, wait for the agent to read the start notice, then close the bound _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      tab. The runtime disarms itself; confirm the log shows the end notice was
      attempted and reports honestly that the target was gone.
- [ ] Turn the setting off, arm and disarm. Neither line may appear anywhere. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      Then, still with it off, arm and check that speech itself still works
      (`voice action=status` must report `available: true` once a language is
      known) — the setting must silence the notices and nothing else.
- [ ] Arm with the setting **on**, let the agent read the start notice, then _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      turn the setting off in Settings while still armed, then disarm. The stop
      notice must still be sent: the agent was already told voice was on.
- [ ] Hold the push-to-talk hotkey and dictate a sentence. No notice of either _(NOT VERIFIED 2026-09-30: blocked — audio hardware: push-to-talk hotkey hold + dictated sentence (Fn/hotkey and mic); no hotkey/mic path on headless build.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      kind may appear, the hands-free badge must stay off, and `voice
      action=status` must still report `available: false`.
## Voice conversation controls in the Dictation panel (story `818-2a29`, 2026-09-22) — **Rust, needs a `make dev` restart**

Settings > Dictation now carries two new sections below Voice tuning: **Spoken
replies** (the speech assets, the language replies are spoken in, and the voice)
and **Hands-free conversation** (terminal picker, Start/Stop, live phase,
activation phrase, hold-back). The dictation hotkey now also stops a running
conversation.

Everything mechanical is covered by tests; these items need real audio, a real
download, or Boss's eye. Run them against the worktree build, never Boss's live
sessions.

- [ ] Cancel a download halfway. The row must go back to Not Downloaded with no _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Cancel route and .staging cleanup need the desktop build (prior 29/09 run saw cancel not clearing 'downloading').)_ _(audit 2026-10-13: HUMAN — CODE+TEST partial: dictation.test.ts clears progress on failure/end; DictationSettings.test.tsx cancels through row control (vitest.log green)…)_ _(triage 2026-10-14: WAITS — Cancel control dispatch passed in voice-library-contracts.log; desktop download interruption and staging-file cleanup remain unverified, and headless build has no audio routes.)_
      progress bar left behind, and no partially installed files may remain.
- [ ] With Italian ready, the **Voice** control must appear and list `giovanni`. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Voice control list (giovanni) is UI; hearing the voice is speaker hardware.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Ready Italian picker including giovanni passed in voice-library-contracts.log; selecting it and hearing an armed reply still requires real speaker/audio.)_
      Pick it, then arm a conversation and hear a reply in that voice.
- [ ] Change the voice while a reply is being spoken. The reply must stop _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Voice change during reply needs UI; audible mid-sentence cut needs ears.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      mid-sentence rather than finish in the other voice.
- [ ] **Opening Settings > Dictation must not light the microphone indicator.** _(NOT VERIFIED 2026-09-30: blocked — audio hardware: observing the macOS microphone indicator (human eyes on menu bar) when opening Settings > Dictation.)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      Neither must starting the app. Nothing arms by itself.
- [ ] Start a conversation from the panel, then press the dictation hotkey. The _(NOT VERIFIED 2026-09-30: blocked — audio hardware: dictation global hotkey plus real capture/voice queue on the desktop app.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      conversation must stop — capture, queue and voice — and the status line
      must say how many spoken entries had already been typed.
- [ ] Press the hotkey with nothing armed. It must record as usual, not report a _(NOT VERIFIED 2026-09-30: blocked — audio hardware: global dictation hotkey recording with microphone.)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      stopped conversation.
- [ ] Let a conversation end by itself (close the bound terminal), then press the _(NOT VERIFIED 2026-09-30: blocked — audio hardware: global dictation hotkey recording with microphone after conversation ends.)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      hotkey. It must record — a stale armed flag must not eat the keypress.
- [ ] Set an activation phrase, then speak a sentence without it: nothing may be _(NOT VERIFIED 2026-09-30: blocked — audio hardware: real spoken sentences (mic) against the activation phrase; no synthetic STT path in headless build.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      sent. Speak one with it: the phrase itself must not reach the terminal.
## Barge-in over a real speaker (story `816-cbbf`, 2026-09-22) — **[HUMAN]**

Criterion 3 of the story asks for a real audio probe, and this is the half of it
no test can reach. Everything that can be measured offline already is —
`talking_over_the_reply_stops_it_without_losing_the_first_words` in
`continuous.rs` reports 50 ms stop latency and 0 false triggers against the real
AEC3 canceller, over a **modelled** room (40 ms delay, 0.35 gain, no
reverberation, no noise floor, no speaker distortion). See
`docs/backend/dictation.md` → "What barge-in measures".

Needs a real microphone and a real speaker, in a room, with no headphones.

- [ ] **[HUMAN]** Arm hands-free, ask something with a long answer, and let the _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      reply play **out of the laptop speaker**. Say nothing for the whole reply.
      The reply must finish. A reply that cuts itself off is the echo path
      failing on real reverberation, which the modelled room cannot produce.
- [ ] **[HUMAN]** Same again, and talk over it after a couple of seconds. The _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      reply must stop within about a quarter of a second, and the transcript
      that reaches the terminal must contain your **first** word — that is the
      pre-roll doing its job. A transcript that starts mid-sentence is the
      failure to report.
- [ ] **[HUMAN]** Repeat both at a high speaker volume, close to the _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
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

- [ ] **[HUMAN]** In an isolated instance (`TUIC_APP_INSTANCE=voice-check`) and _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs real voice hardware for the full conversation journey.)_
      against a throwaway terminal: download the speech assets, arm hands-free,
      speak a question, and let it run to the end — automatic end of turn, the
      agent answering out loud, and talking over the answer to interrupt it.
      Everything with the real assets, not the test doubles.
      _Unblocked 2026-09-22: the voice is published. Real Pocket TTS synthesis
      is already proven against a local bundle — `cargo nextest run --lib
      --run-ignored ignored-only -E 'test(/dictation::speech::pocket/)'` with
      `TUIC_POCKET_BUNDLE_DIR` and `ORT_DYLIB_PATH` set, 3/3 green. What is left
      here is the microphone, the speaker and the interruption._
- [ ] **[HUMAN]** Ask the **model** to speak through the `voice` MCP tool while _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      that same conversation is armed, and confirm it reaches the same speaker
      and the same queue as a reply the desktop asked for.
- [ ] **[HUMAN]** The same conversation from a browser tab against the same _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Depends on the preceding real voice conversation.)_
      instance: the microphone and the speaker must be the **browser's**, not
      the desktop's, and the desktop must behave identically.
      _Blocked: browser audio transport is story `832-e730`._
- [ ] **[HUMAN]** Record, with numbers: time from the end of speech to the _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      first audible word of the reply, and the process footprint before arming,
      while speaking and after disarming (`GET /diagnostics/memory`). A
      conversation that leaks per turn is the failure to look for.
- [ ] **[HUMAN]** Repeat the first item on Windows and on Linux from a release _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs the named native platform/CI runtime; this machine is macOS.)_
      build. Cross-platform evidence cannot come from this Mac.
## Speech and download progress on `/events` (story `833-6fd4`, 2026-09-22) — **Rust, needs a `make dev` restart**

The three pushes are dual-emitted: the desktop window gets an `emit`, the SSE
stream gets the identical body. Only a restart loads them.

- [ ] Arm hands-free, let a reply play, and watch the same stream: one _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs /events SSE watch on desktop build while a reply plays (speech-utterance frames).)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      `speech-utterance` frame per transition, in the order
      `queued → rendering → speaking → finished`, with no polling of
      `GET /dictation/speech/status`.
- [ ] Talk over a reply and confirm the last frame for that utterance is _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Talk-over needs armed conversation; audio can come via WS client, hearing cannot.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
      `interrupted` rather than `finished`, and that it arrives — the transition
      happens on the render thread after `speak` has long returned, which is the
      case a polling client used to miss entirely.
## Agent toast repository action (story `835-314c`, 2026-09-22) — **frontend visual**

The rendered DOM was exercised with the real Vite modules and contained two
toasts at once: the different-repository toast had `Go to repo`; the current-
repository toast did not. `agent-browser` navigation and DOM inspection worked,
but `Page.captureScreenshot` timed out repeatedly, and CUA could not bind the
headed test browser window.

## SSH local-forward readiness (story `1159-4e28`, 2026-09-28) — **Rust, needs a `make dev` restart**

- [ ] After restarting an isolated test instance, connect the Installed-service _(NOT VERIFIED 2026-09-30: blocked — second physical machine: aws-graviton (56481148) SSH remote; AWS box is DOWN per coordinator brief, and instance config must not be edited by hand.)_ _(audit 2026-10-13: HUMAN — Configured aws-graviton connection needs remote login/host readiness; no connection or credential changes attempted.)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
      `aws-graviton` remote (56481148) over SSH. It should progress from
      Connecting to Connected once the local forward listens and `/health`
      answers, without an intermediate "installed daemon not answering" error.
      Record the elapsed time and the tunnel status transitions. Use only the
      configured host and credentials; the targeted Rust tests cover delayed
      local ports and delayed health independently.
## Config defaults and expert-mode UI pref (story `863-03c1`, 2026-09-24) — **Rust, needs a `make dev` restart**

## Error messages name the reorganized Settings pages (story `861-977b`, 2026-09-24) — **Rust, needs a `make dev` restart**

- [ ] [VISUAL] Settings → General with Experimental Features **off**: an **ego** _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_ _(audit 2026-10-13: VISUAL-OWED — Pure visual requirement; screenshot/image inspection unavailable per brief; not attempted.)_ _(triage 2026-10-14: WAITS — GeneralTab.tsx:385/441/483/487 shows native Code Intelligence then ego, tooltip and Select/Clear outside the experimental gate; AiChatTab owns model/providers. Browser mode uses a text input instead of native file selection; prior GeneralTab DOM and AI Chat setup tests pass. Native visual arrangement remains: MacControl recording denied and locked Chrome cannot capture images. Evidence: ~/Gits/.tmp/results/to-test-1014/phase2-chat-setup-proof.json and ego-profile-row-proof.json.)_
      section sits directly after Code Intelligence, with a `?` tooltip, the
      "Configured at …" / "Not configured" line, **Select…** and **Clear**.
      Settings → AI Chat (Experimental Features on) shows only Default Model
      and Providers. With the ego path empty, AI Chat says to name the binary
      in Settings → General.
## Mobile Basic Auth recovery (2026-09-25) — **Rust, needs a `make dev` restart**

- [ ] **[HUMAN]** On a phone PWA with remote access enabled and a stale cached Basic _(NOT VERIFIED 2026-09-30: blocked — real phone: PWA with stale cached Basic credential and the browser's native Basic Auth challenge.)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
      credential, navigate until the browser shows its Basic Auth challenge. Enter the
      current password without reloading. The app must reconnect and resume the session.
## CircleCI failure logs — **Rust, needs a `make dev` restart**

- [ ] After restarting the worktree build, open a failed CircleCI check on a remote-only PR, including a PR with a failed GitHub Actions job. Its Log button shows only that CircleCI check's log and the end of a long failed step, with a truncation marker when the beginning was dropped; a stale or mismatched CircleCI build reports a revision mismatch. The running app cannot load this Rust change until restart. _(NOT VERIFIED 2026-09-30: blocked — not a hardware class, but needs a CircleCI API token and a real remote-only PR with a failed CircleCI check; GET /circleci/token -> configured:false. Reading the CircleCI code path for a fake server was denied by the sandbox classifier (credential), so not attempted. Coordinator decision.)_ _(audit 2026-10-13: HUMAN — Live /circleci/token configured=false source=none (totest-I/circleci.json); requires login/token and failed CircleCI PR.)_ _(triage 2026-10-14: WAITS — Five focused PrDetailContent CircleCI checks passed in circleci-ui-contracts.log: exact check URL/head SHA, failed-row gating, ANSI-clean log display, cached reopen and keyboard no-external-open. Actual remote account log download, truncation and revision mismatch need reachable isolated backend/account; rb unavailable.)_
## Vite watcher scope and native reload attribution — needs a `make dev` restart

- [ ] Restart `make dev` when live PTY sessions can be interrupted. In an isolated dev instance, create and delete a checkout with HTML files under repository `.tmp/`; verify document age continues increasing and no full reload occurs. Call `POST /debug/reload_webview` and verify the native log records caller address, trigger, action, and target URL while frontend startup records navigation type and document start. The Vite watch config and Rust backend require a restart to take effect. _(NOT VERIFIED 2026-09-30: partial — Verified: vite dev on :5199 with repo vite.config.ts: creating/deleting HTML under repo .tmp/ (nested too) logged no 'page reload'; controls public/*.html and root *.html logged 'page reload'. POST /debug/reload_webview on headless -> 'webview recovery requires the desktop feature'; native log lines)_ _(audit 2026-10-13: HUMAN — Required reload/restart prohibited by brief; prior Vite-watch partial evidence retained, native attribution not exercised.)_ _(triage 2026-10-14: WAITS — Exact document-age/no-full-reload check needs native dev WebView and owned Vite lifecycle; managed desktop launch forbidden. No Boss Vite or WebView restart.)_
## Ask Boss from the mobile PWA (2026-09-26) — Rust, needs a `make dev` restart

- [ ] **[HUMAN]** After this Rust parser change is landed and Boss restarts `make dev` when current PTYs can be interrupted, open the HTTPS mobile PWA on a real phone at 360×800. In a disposable Codex session, trigger `request_user_input` with two choices and `Other`. Confirm its waiting badge, tap the question control in the existing session header, read the title and all options, select an option once, and verify Codex receives exactly one answer and the overlay clears. Repeat with `Other` and type a note; verify it reaches the question rather than the main composer. Check the terminal has lost zero rows. The running backend cannot load the Rust parser change before restart. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_

- [ ] **[HUMAN]** After restarting the desktop app when its current PTY sessions can be interrupted, enable Remote Access and Tailscale HTTPS, then open the shown HTTPS `/mobile` URL on the phone. On iPhone, launch the installed Home Screen PWA. In mobile Settings, turn Push notifications off and on to replace the old subscription, grant permission, and confirm a test push appears on the phone. Do not change Tailscale/network configuration as part of this check. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
- [ ] **[HUMAN]** With the desktop window left focused but no Mac HID input for two minutes, have a managed agent report `progress type=blocked` with an identifiable question. Confirm one phone notification contains the question, opens that exact session, and one typed reply reaches it once. During a confident free-text question, leave an automated peer message queued: the phone answer must reach the question first and the peer message must remain parked until the question clears. Repeat with the desktop actively used: no duplicate push. The running app cannot load these Rust changes until restart. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs a subscribed phone and two minutes without Boss HID input.)_
- [ ] **[HUMAN]** After the separate question-state change is integrated, trigger a real Claude AskUserQuestion with a visible title. Confirm the phone push contains that title rather than the hook's empty awaiting signal or an Ink footer, then answer it from the opened session. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs real phone push delivery for the visible question title.)_
## Activity Dashboard window after Rust restart

- [ ] After restarting an isolated `make dev` build, restore Activity Dashboard with saved geometry larger than 550×650. The detached OS window opens at 550×650 while retaining its saved position; a smaller saved size remains unchanged. The running Rust backend cannot load this fix until restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs desktop build: detached Activity Dashboard OS window size after restore with saved geometry >550x650; not observable headless.)_ _(audit 2026-10-13: VISUAL-OWED — Desktop/window geometry/rendering requires visual verification unavailable here; no screenshot attempted.)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
## WebView reload resource cleanup — Rust, needs a `make dev` restart

- [ ] After restarting an isolated `make dev` instance, open two desktop terminal panes and register a plugin output watcher. Reload the main WebView, then inspect `/diagnostics/memory`: the old document's grid channels, gates and output watcher clients must be gone before the panes remount. Leave one pane unmounted; it must produce no desktop `grid frame gate stuck` warnings. Two minutes later, a 30-second app-log window must contain no `Couldn't find callback id` or `Output watcher clients exceeded 8` warnings. The running backend cannot load this Rust change until restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs desktop main WebView with two panes, plugin output watcher, reload and /diagnostics/memory before/after; headless has no WebView.)_ _(audit 2026-10-13: HUMAN — Reloading Boss WebView forbidden; no isolated desktop launch allowed; cleanup scenario cannot run.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
- [ ] Detach a terminal into a floating window, reload the main WebView, and confirm the floating terminal keeps painting. Close the floating window and confirm its grid subscription disappears while terminals in the main window keep painting. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs desktop floating terminal window and WebView reload.)_ _(audit 2026-10-13: HUMAN — Main WebView reload forbidden; floating-window persistence needs desktop multiwindow check.)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
## Mobile ego chat (story 1077-0c08) — real phone after `make dev`

- [ ] [HUMAN] After restarting the test instance with `make dev`, open its HTTPS `/mobile` URL on a real phone. In Chat, choose a disposable repository and send a prompt; confirm the answer and collapsed tool activity remain readable above the keyboard. Disconnect and reconnect the phone, then confirm the answer has no duplicate or missing lines. Start a permission request, tap one option twice, and confirm the desktop conversation records one answer. Start a second conversation, then use the titled picker to return to the first and confirm its history loads. This check requires real touch and mobile keyboard behavior; targeted Vitest covers the module behavior. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Detached Markdown document window

- [ ] After restarting an isolated test build, open a Markdown file tab and a `tuic://open` Markdown tab. Detach each from its context menu, resize the document window, edit each file on disk, and confirm the detached content updates. Clicking either tab should focus its window; closing the window should restore the document in the tab. Check an inline comment and a relative Markdown link in the detached view. _(NOT VERIFIED 2026-09-29: blocked — Needs observing detached panel windows' content/size/focus; maccontrol capture_window lacks Screen Recording permission, invoke_js reaches only the main webview, and two identical 'tuicommander' windows make clicking ambiguous with Boss's app. Not attempted.)_ _(audit 2026-10-13: VISUAL-OWED — Desktop/window geometry/rendering requires visual verification unavailable here; no screenshot attempted.)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
## ACP mobile interaction push (story 1078-05cf) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, subscribe a phone to push. With desktop unfocused, have ego ask permission or a form question and confirm one notification opens mobile Chat. Answer the next request on desktop before delivery and confirm no stale notification; ordinary activity must not notify. The running backend cannot load this Rust change until restart. _(NOT VERIFIED 2026-09-30: blocked — real phone: push subscription and mobile Chat notification with desktop unfocused.)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs a subscribed phone and native away-state notification observation.)_
## Push-to-talk Italian hallucination filter — Rust restart required

- [ ] After restarting `make dev`, use a disposable terminal to verify that a bare “Grazie a tutti.” recognition does not reach the composer, while a genuine instruction containing those words does. The current backend cannot load the Rust change until restart. The sustained-speech activity gate remains pending real quiet-speech recordings (story 1135-b600). _(NOT VERIFIED 2026-09-30: blocked — audio hardware: real speech recognition of 'Grazie a tutti.' (filter is unit-level HALLUCINATION_EXACT); dictation routes absent in headless build.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## MCP initialize storm — Rust restart required (#1148-c25f)

- [ ] After that Rust restart, let an isolated disposable MCP protocol session pass the one-hour TTL (or invoke the maintenance sweep in a test build). Confirm its protocol session, route, reverse route, and broadcast sender all disappear while an addressable PTY peer and any live sibling remain usable. This checks the reaper cleanup added for story 1148; the source of the incident's 27.3 GB malloc growth is still unknown. _(NOT VERIFIED 2026-09-30: partial — Isolated instance, sweep after 1h idle: mcp.sessions 3->1, log 'MCP session reaped (idle >=1h)' x2 (sibling kept, still usable); PTY peer r4pty kept addressable and agent send to it after reap delivered:true; ext peers evicted. Route/reverse-route/broadcast-sender maps not exposed in diagnostics: in)_ _(audit 2026-10-13: HUMAN — One-hour TTL/maintenance sweep not run on shared app. Prior note only proves session count, not route/reverse-route/sender cleanup.)_ _(triage 2026-10-14: WAITS — Needs owned expiry/reaper execution and live sibling observation; isolated daemon unavailable with rb SSH down. Existing source/test evidence retained without one-hour soak claim.)_
## Worktree removal preview — Rust restart and visual review (#1138-ed2e)

- [ ] After restarting an isolated `make dev` build, open removal confirmation for a branch with no own commits and a live agent in its worktree. Confirm the dialog names the agent and uncommitted/untracked counts, then take a screenshot of both the removal and post-merge cleanup dialogs. The current backend cannot hot reload the Rust preview, and this branch has not been rendered in a worktree build. _(NOT VERIFIED 2026-09-30: partial — Verified backend preview (GET /worktrees/lifecycle?repoPath&workspaceId=feature-r4): live_sessions names r4wt-agent (fake managed agent, not real CLI), dirty_files 3, untracked_files 2, warning 'nothing of its own'. Dialog rendering and screenshots need desktop UI.)_ _(audit 2026-10-13: VISUAL-OWED — Desktop/window geometry/rendering requires visual verification unavailable here; no screenshot attempted.)_ _(triage 2026-10-14: WAITS — Prior actual lifecycle API names and counts retained; requested native removal and cleanup screenshots unavailable with Screen Recording denied and no desktop launch.)_
# Mobile session search (story 1200-4dd4)

- [ ] [HUMAN] After `make dev`, check the magnifier position at the top right of the session list on a phone. Tap it, enter a filter, and confirm the field and matching cards fit without clipping. The component test covers matching and clearing; phone layout remains to be checked. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] mobile session list layout.)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
# Mobile AI Chat image prompts (story 1219-e2a1) — Rust restart required

- [ ] [HUMAN] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, open the HTTPS mobile PWA on an iPhone and paste a 4 MiB photo from the camera roll into AI Chat. Confirm ego receives it; check an image over 10 MiB is refused in the composer with its size and limit. The HTTP route and composer have targeted automated tests; iPhone Photos behavior requires the device. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Mobile attachments (story 1227-7838) — Rust restart and real phone

- [ ] [HUMAN] After restarting an isolated `make dev` instance, use an iPhone to pick a HEIC photo and a 4 MiB camera photo from the single paperclip picker. Check conversion/type handling, the displayed size limit, and that the selected file remains a draft until Send. The phone's Photos provider and touch layout require the device. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
- [ ] [HUMAN] On an Android Chrome installed PWA, share a photo from another app into TUICommander. Confirm it appears as an AI Chat draft, then send it. The OS share sheet requires a real device. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Mobile session output links (story 1202-dd5b)

- [ ] [HUMAN] On a phone, tap a Markdown path in a session's output, confirm Files renders it and Back preserves the session output and draft. Tap an HTTP(S) link and confirm it opens the system browser outside the PWA. A path outside registered repositories must show a toast naming that path. Targeted tests cover link detection, routing, and root refusal; the phone handoff and touch remain to be checked. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Mobile keybar and composer (story 1221-0380) — real phone

- [ ] **[HUMAN]** On a 360 px wide phone, open a disposable agent session and check that the terminal retains its previous visible row count, the keybar scrolls without a visible scrollbar, and `/`, Ctrl+C, input and Send are comfortable touch targets. Tap `/`: no character should reach the agent until a command is chosen. Close the menu and confirm an unsent draft is restored. End the disposable session and confirm the keybar and composer cannot send. Component tests cover PTY writes and disabled state; a 360×800 browser capture measured keybar 45 px, composer 53 px, and terminal 702 px, but cannot prove real touch behavior. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Mobile session list and new-session sheet (story 1223-f3bc) — real phone

- [ ] **[HUMAN]** On a 360 px phone, confirm a waiting session remains above idle agents and shells, exact mixed-case names display unchanged in the list, detail, and question banner, and the busy badge reads "Working". Tap the question counter and confirm it opens the first waiting session. Open `+`, check agent choice, repository search, and the close X, then create a disposable Codex session and confirm it opens. Component tests cover ordering, spawn payload, navigation callback, banner content, and counter click; real touch and visual layout remain to be checked. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Mobile session card actions (story 1217-4b0b) — real phone

- [ ] **[HUMAN]** On a 360 px phone, scroll the session list to its end. Confirm the `+` button never covers the last card's kill button, both `+` and kill are comfortable touch targets, tapping a card opens it, and tapping kill opens the confirmation without opening the session. Component tests cover independent click actions and accessible names; browser geometry at 360×800 measured 44×44 px kill, 52×52 px `+`, and an 87 px gap between the last kill and `+` after scrolling to the end. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Mobile five-tab navigation (story 1224-d21c) — real phone

- [ ] **[HUMAN]** Launch the mobile PWA on a phone and confirm Sessions opens first, Chat is the second bottom tab, and only five tabs remain. Tap the app-bar overflow, open Settings, then use a bottom tab to return. Check the overflow does not clip and the tap targets remain comfortable at 360 px. Component tests cover order, initial selection and Settings navigation; real touch and phone layout remain to be checked. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Mobile terminal hanging indent (story 1222-912d) — real phone

- [ ] [HUMAN] At 360 px, open a session with a long space-indented list item and a long tab-indented code line. Confirm every visual continuation starts under the first line's text, while an unindented line stays flush left and a box-drawing table still scrolls horizontally. Browser character-rectangle checks cover the same output shapes; this item checks real device rendering and touch scrolling. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Mobile session detail task text (story 1216-a482) — real phone

- [ ] On a 360 px phone, open idle, awaiting-input, and ended Claude sessions whose terminal status line has a decorative spinner verb. Confirm no task row repeats that verb and the terminal gains the freed row; a Codex session with a substantive task such as “Reading files” should still show it. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Mobile session-state targeted tests passed in mobile-contracts.log; exact real-phone freed-row layout and touch observation requires Boss device per mobile/AGENTS.md.)_
## Claude AskUserQuestion options on mobile (story 1212-3093) — Rust restart and real phone

- [ ] **[HUMAN]** Restart an isolated `make dev` instance so its Rust parser loads, then open a disposable Claude AskUserQuestion on a phone. Confirm the title and every option remain visible, the choice overlay replaces generic Yes/No, and tapping option 2 selects Green exactly once. The captured PTY replay and component tests cover the payload and key sequence; this check covers real touch and phone rendering. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Mobile Markdown images (1215-8e00)

- [ ] After restarting `make dev` to load the Rust HTTP route, open a nested Markdown file with a repository-relative image in the mobile Files tab and confirm the image loads. Check that a path escaping the repository does not load. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: VISUAL-OWED — Mobile rendered image-load check requires client view; no screenshot/view capability; no disposable registered repository fixture created.)_ _(triage 2026-10-14: WAITS — Mobile Files component/path-render evidence retained; actual repository-relative image HTTP containment/load needs owned daemon and phone/browser fixture. rb unavailable; no image route pass inferred.)_
## Mobile Progress, Activity, and Settings (1226-eb95)

- [ ] On a 360 px phone, confirm a long Progress message shows about four lines, More reveals it all, and Less collapses it again. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Mobile progress expansion/notification tests passed in mobile-progress-files-contracts.log; actual phone four-line geometry/touch needs Boss device.)_
- [ ] After restarting `make dev` to load the Rust `mobile_theme` preference, choose Light in mobile Settings, reload the PWA, and confirm the theme stays light. Confirm the desktop theme remains unchanged and app/server versions are visible. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Changing persisted mobile theme and reloading PWA not attempted on shared settings; code/test coverage alone does not prove persistence live.)_ _(triage 2026-10-14: WAITS — Four mobile appearance tests passed in mobile-appearance-contracts.log (Paper remount, versions, server-only preference and failed save); real PWA reload and backend persistence remain unverified while the isolated rb host is unreachable.)_
## AI Chat copy, links and parallel tabs (story 1142-f09c)

- [ ] [VISUAL] After `make dev`, in an AI Chat conversation select and copy a paragraph of transcript text and confirm it lands on the clipboard; click an http(s) link in a reply and confirm it opens in the external browser; click a local file path and confirm it opens through TUIC's file/editor opener. Open a second chat tab and confirm it runs an independent ACP session in parallel with the first. Component/DOM tests cover selection, copy, link routing and tab lifecycle; the live interaction remains to be checked. _(NOT VERIFIED 2026-09-29: Needs a real ego ACP conversation, external browser opener and system clipboard; two parallel live sessions.)_ _(audit 2026-10-13: VISUAL-OWED — Pure visual requirement; screenshot/image inspection unavailable per brief; not attempted.)_ _(triage 2026-10-14: WAITS — Prior component/link routing and parallel-session evidence retained; actual native clipboard, external browser and two live ACP conversations remain unverified. No Boss clipboard or external opener used.)_
## AI Chat session settings dialog (story 1145-3abd)

- [ ] [VISUAL] After `make dev`, open AI Chat and confirm the control bar shows a compact model/mode summary with one settings button. Open a new chat tab and confirm the summary is not blank. Open the settings dialog and confirm it lists one labeled row per ACP select config option (name, description, current value); change a value and confirm the summary updates from the agent's reply. Targeted Vitest covers the dialog and the late-options case; the live rendering remains to be checked. _(NOT VERIFIED 2026-09-29: Needs live ego ACP agent providing select config options.)_ _(audit 2026-10-13: VISUAL-OWED — Pure visual requirement; screenshot/image inspection unavailable per brief; not attempted.)_ _(triage 2026-10-14: WAITS — AIChatPanel full targeted module passed in core-ui-tests.log including late config options; exact live agent-supplied summary/dialog rendering remains without available native capture or owned model runtime.)_
## Codex questions on mobile PWA (story 1201-ae16)

- [ ] [VISUAL] On a real phone after `make dev`, open a Codex session that is waiting on a `request_user_input` question. Confirm the question text and its options are fully visible and each option can be tapped/selected without clipping at 360 px width. Confirm choosing an option submits exactly once and the question clears from both the session list badge and the session detail. Captured-fixture and 360 px DOM geometry tests already pass; the real-device check remains. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: VISUAL-OWED — Pure visual requirement; screenshot/image inspection unavailable per brief; not attempted.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Git panel Log tab layout (story 1263-7d7d)

## Branch integration proofs (story 1295-a2ce)

- [ ] After Boss restarts `make dev` or rebuilds the release, query `repo action=branch_integrations` and `repo action=branch_integration` on throwaway fixtures and confirm the new proof fields are available. Check that an integrated squash branch appears in the branch panel and sidebar, and a content-based proof requires an archive of the current tip before safe deletion. Automated real-Git lifecycle, MCP and deletion checks passed; the installed process has not been restarted to load this Rust change. _(PARTIAL 2026-10-11: branch_integrations and branch_integration returned proof fields (in_sync, ancestry, patch_equivalence for a squash branch); deletion with an archived tip succeeded (proof archived) and an unarchived unmerged branch was refused. A content_superset proof could not be reproduced and the branch panel/sidebar was not checked. Evidence `~/Gits/.tmp/totest-1011b/ev/git2.json`. Isolated headless daemon built from main d2f8f13df.)_ _(audit 2026-10-13: VISUAL-OWED — Prior 2026-10-11 partial proof fields retained; integrated squash branch panel/sidebar and content_superset archive not exercised.)_ _(triage 2026-10-14: WAITS — Prior actual proof-fields/deletion evidence retained; content_superset archive branch case and live branch-panel/sidebar remain incomplete. Owned runtime unavailable and native screenshot denied.)_
## Click on an underlined relative path (1336-7755)

- [ ] After a desktop rebuild, in a Claude session (and once in a plain shell with `echo docs/x.md`) click an underlined relative path, then read the app log for `link click:` lines (debug level). A click that opens the file logs `link click: opening`; a dead click logs which step ended it (`not opened` with claimed/detail/hasRange, `lookup went stale`, `nothing resolved`, or `does not cover`). Also click an OSC 8 link whose text is no path and hover it first: it opens (press claimed from the hover probe, #1336 fix). Browser mode could not reproduce the dead click; only the desktop can. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Desktop click/hover requires disposable visible terminal interaction; cannot view images, no owned UI interaction performed.)_ _(triage 2026-10-14: WAITS — Existing production file-link routing/real path resolver tests pass in file-links-contracts2.log and file-links-resolver.log; native click/debug log route requires desktop interaction unavailable here.)_
## Compose image paste and toolbar marker (1350-a1e6, 1351-69e0)

- [ ] After the next desktop rebuild, paste a screenshot (Cmd+Shift+Ctrl+4) and an image file copied from Finder into Compose: both attach an `[image: ...]` tag, in paste order. Paste plain text: it pastes as text. In auto density mode the toolbar button shows only the letter, no blue marker. _(audit 2026-10-13: HUMAN — PasteAndDensityCriticR2 + pastedImage + ComposePanelPasteCritic + ToolbarDensityCriticR2 passed (vitest.log / vitest-extra.log)…)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
## PR panel (1345-fb71 .. 1349-7546)

- [ ] After the next desktop rebuild, on a repo with open PRs: a PR transition (CI failed, ready, merged) gives one OS notification and its click opens the PR; Merge sends the head the panel showed; a PR with unresolved review threads shows the bot/human split and is not Ready; Update branch on a BEHIND PR, Close PR with confirmation, age marker and Copy reference work on their own row only. Then approve plan steps S1-S5. _(audit 2026-10-13: HUMAN — Full item includes GitHub PR merge/update/close/review/queue writes; prohibited external live writes. Read-only githubReviewTransport and githubPrActionsTransport passed…)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
## Edge TTS default engine (1357-7d37)

- [ ] After the next desktop rebuild, with network and a fresh config: Settings > Voice > Spoken replies shows Edge voices for the dictation language; Listen speaks the preview; the choice survives a restart. Run `cargo nextest run -p tuic-dictation --run-ignored only -E 'test(live_edge_service)'` once to confirm the live handshake (Sec-MS-GEC, Origin) still works. Start hands-free and get a reply spoken; say "hush" mid-reply and it stops. Turn the network off: the Voice section shows the "needs an internet connection" error and replies do not hang. Expert > Speech engine: Pocket and External still work; an install that had Pocket keeps Pocket. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:4758)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Remote desktop MCP routing (1419-ab18) — coordinated Rust rollout required

- [ ] Remote replay rollout (#1421-733e): deploy the updated daemon before the updated client. An older daemon without the explicit empty-replay marker can trigger a false 15-second stream error on a healthy idle session whose initial grid is unavailable. Coordinate deployment after live PTYs can be safely preserved or closed; do not restart mac-mint during this incident. _(audit 2026-10-13: HUMAN — Requires coordinated mint/Tailscale daemon rollout/restart or server lifecycle; only disposable local state authorized and real daemons must remain intact.)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
## Private secret forms (#1435-6e1d) — Rust restart required

- [ ] (#1520-46b1) After Boss restarts the rebuilt desktop and updates the CLI/bridge, request a throwaway form. Confirm a separate native window opens, wait over 10 seconds, then decline; caller must receive names and `declined`. Repeat with harmless entry and verify only names/`stored` return. If absent, read source `secrets` logs: no `Secret tool dispatched` means the request did not reach this handler; `Opening` without `Creating` means host/store setup failed (see `Secret tool failed`); `Creating` without success/error means native construction did not return; a creation failure reports its native error; `created` means investigate visibility/frontend bootstrap. No desktop was launched by the peer. _(audit 2026-10-13: VISUAL-OWED — Native separate-window visibility/10-second decline remains unobserved; secret dispatch tests passed but cannot prove native rendering. No credential entry attempted.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_

- [ ] After Boss restarts the desktop backend, request username/password/OTP _(NOT VERIFIED 2026-10-12: needs the native private-form window of the desktop host; the headless daemon refuses secret requests with "Secret requests require a desktop host to open the private form". Evidence `~/Gits/.tmp/run-1706/b9.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: VISUAL-OWED — Synthetic bootstrap/reduced-schema/decline paths source secrets/forms.rs and account_sink_tests.rs; native separate window rendering unavailable.)_ _(triage 2026-10-14: WAITS — Headless host explicitly refuses native secret forms; source/test reduced-schema evidence retained, but separate native window and timeout UI need Boss desktop. No credentials entered.)_
  fields with synthetic data. Verify the separate native window, exact reduced
  schema, main-window bootstrap rejection, decline, close and timeout cleanup.
- [ ] On a trusted existing HTTPS server address, submit a desktop-opened request _(NOT VERIFIED 2026-10-12: needs the native private-form window of the desktop host; the headless daemon refuses secret requests with "Secret requests require a desktop host to open the private form". Evidence `~/Gits/.tmp/run-1706/b9.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Trusted HTTPS one-time secret entry requires operator-controlled server/form interaction; no credential or external sink access authorized.)_ _(triage 2026-10-14: WAITS — Needs a trusted operator-controlled server and native private form; headless refusal already observed. No secret sink or credentials accessed.)_
  from the one-time entry path; verify one-time consumption and cleared input UI.
- [ ] After Boss restarts the desktop build and updates the remote daemon, drop a read-only directory from Finder onto a remote repository. Verify all files arrive, final directory permissions remain read-only, and the Mac source stays untouched. Rust does not hot-reload; this requires a manual restart when Boss is ready. _(Story 1434 round 3: Linux handler tests cover upload deadlines, staging cleanup and cancellation; native Finder/macOS publication awaits restart.)_ _(audit 2026-10-13: HUMAN — Native Finder/clipboard/drop interaction spans another app; excluded multi-app/hardware boundary. No Boss clipboard or files changed.)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
## Remote MCP toast mirror (1439-d84f) — Rust restart required

- [ ] After Boss restarts `make dev` or installs a `make build` release, connect a daemon, raise an MCP toast from a remote agent, and check the host-labelled Messages entry, requested sound and Open terminal navigation. Disconnect, raise a toast remotely, and reconnect: no stale entry should appear. The mirror backend cannot hot-reload; do not restart Boss's desktop from an agent. Automated Rust/frontend regressions cover the filter, payload, navigation and disconnected-frame behavior. _(NOT VERIFIED 2026-10-12: needs a desktop host mirroring a remote daemon plus a remote agent/GitHub/ACP event; a headless daemon connected to a second daemon works (item 4795) but no mirrored notice, toast or card UI exists to inspect. Evidence `~/Gits/.tmp/run-1706/b5.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: VISUAL-OWED — rust-final.log toast host label/origin and disconnect/reconnect tests passed; live native card sound/open-navigation rendering still owed; source remote_mirror.rs:932 + disconnected_toast)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_

Known limit for 1439-d84f: **Open terminal** is a harmless no-op when the remote PTY's cwd/repo is outside every registered remote repository. Navigation ownership currently comes from the repository registry. This is not fixed by the toast mirror change.
- [ ] #1418-48c7: After Boss restarts the backend, move a literal prompt-shaped row with RI/IL and copy it at its new position: keep the glyph. Replace the entire row with ECH/DCH/ICH, redraw a fresh composer, and copy: remove only composer chrome. Partial edits must keep unknown-origin content literal. Automated grid and selection regressions cover these paths; desktop clipboard awaits restart. _(NOT VERIFIED 2026-10-12: the check is a canvas selection plus clipboard copy in the desktop terminal; the HTTP terminal API returns rows only and has no selection or clipboard, so the copy result cannot be read headless (grid regressions are automated). Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — terminal.log 28 passed / rust-repaired.log 35 passed: copied_selection_ech_full_row_clears_loss_but_partial_keeps_literal; partial-line/full-screen-reset tests (RI/IL exact scenario not claimed)…)_ _(triage 2026-10-14: WAITS — Selection/Unicode and grid regressions retained; exact native RI/IL plus clipboard-copy observation needs desktop interaction. No global clipboard touched; source/API evidence cannot prove copied UI text.)_

- [ ] Speaker shutdown (#1452-f186, release gate #1447-a894): Rust fix is staged and requires Boss to restart `make dev` or rebuild the release when ready. After a spoken reply drains, close the voice conversation; the render worker must stop without hanging. A forced-interleaving regression covers shutdown during completion dispatch; the current desktop has not loaded this change. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Requires audible spoken reply and closing Boss voice conversation; hardware/audio judgement excluded.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Debug sidecars for `make dev` / `make test` (1465-3d95) — desktop restart required

- [ ] After a fresh `make dev` (or `make test`) on a checkout with an empty `src-tauri/target`, confirm `src-tauri/target/debug/tuic-bridge` and `tuic` exist and the app finds the bridge (`locate_bridge_binary`). After editing `crates/tuic-bridge/src`, restart: the bridge mtime must change. Do not launch a second desktop instance from an agent lane. _(audit 2026-10-13: HUMAN — Read-only exists check found both main src-tauri/target/debug/tuic and tuic-bridge. Empty-target cold build and edit/restart mtime clauses forbidden for live backend; Makefile:109 builds both.)_ _(triage 2026-10-14: WAITS — Makefile:74,103 builds both sidecars and prior read-only binaries exist; exact fresh-target rebuild/mtime/restart needs authorized coordinated rebuild. No cold per-agent build or native launch; rb unavailable.)_
- [ ] After the approved daemon update, launch Claude manually on the configured Mac-mint connection, exit back to the shell, and verify submit/mail no longer write there; a run-config preset must survive shell startup (#1420-f3de). _(audit 2026-10-13: HUMAN — Requires coordinated mint/Tailscale daemon rollout/restart or server lifecycle; only disposable local state authorized and real daemons must remain intact.)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_

- [ ] After the coordinated rebuild, verify shell-root return revokes a manually launched agent regardless of shell basename; a bash-script wrapper is observed, and a nested subshell under a directly spawned agent holds submit/mail until the agent regains foreground (#1420-f3de). Do not restart live PTYs for this check. _(NOT VERIFIED 2026-10-12: needs a real agent binary (claude/codex/ego) that does not exist on the rb build box or a desktop Settings/AI Chat UI. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — boundaries.log bash-script wrapper/shell-root revoke/exit-to-shell tests passed on Linux real processes. Full clause also needs nested subshell under directly spawned agent holding submit/mail…)_ _(triage 2026-10-14: WAITS — Prior Linux real-process wrapper/revoke tests retained; nested subshell foreground submit/mail clause needs owned running backend/process fixture unavailable with rb SSH down. Boss PTYs untouched.)_
## Windows core dependency (1478-ead1) — rebuild required

- [ ] Load this manifest fix in the next Windows build. The `tuic-core` MSVC cross-check passes; a separate WebRTC/Abseil C++ build failure is tracked in 1479-f956. Existing desktop processes do not hot-reload Rust; restart only when Boss is ready. _(audit 2026-10-13: HUMAN — Requires native Windows runtime/build or Windows Nightly artifacts; this lane is macOS plus Linux rb, cannot claim native Windows. No desktop/build deployment authorized.)_ _(triage 2026-10-14: WAITS — Needs the named native platform/CI runtime; this machine is macOS.)_
## Windows WebRTC compiler (1479-f956) — native Nightly required

- [ ] After landing and pushing, the Windows Nightly must report MSVC for both Meson C/C++ compilers, compile Abseil/WebRTC without MinGW header errors, and finish the Tauri NSIS build. This Rust build-script change requires rebuilding; no desktop instance was started for verification. _(audit 2026-10-13: HUMAN — Requires native Windows runtime/build or Windows Nightly artifacts; this lane is macOS plus Linux rb, cannot claim native Windows. No desktop/build deployment authorized.)_ _(triage 2026-10-14: WAITS — Needs the named native platform/CI runtime; this machine is macOS.)_
## Windows runtime CI fixes (1518-d3a7) — Rust restart required

- [ ] After Boss restarts the Rust build, verify Windows workflow worktree assignment and orphan cleanup with native and Git path spellings, archive hooks that invoke Git, and failed/cancelled remote-copy staging cleanup. Native Windows CI after landing remains required; the live desktop backend does not hot-reload these Rust edits. _(audit 2026-10-13: HUMAN — Requires native Windows runtime/build or Windows Nightly artifacts; this lane is macOS plus Linux rb, cannot claim native Windows. No desktop/build deployment authorized.)_ _(triage 2026-10-14: WAITS — Needs the named native platform/CI runtime; this machine is macOS.)_

- [ ] After Boss restarts the Rust build on Windows, verify Claude session discovery and the subagent view under a drive-letter checkout; the project slug must keep its drive letter and replace every non-ASCII-alphanumeric character (including the colon and profile spaces) with a dash. The running desktop backend does not hot-reload this fix. _(audit 2026-10-13: HUMAN — Requires native Windows runtime/build or Windows Nightly artifacts; this lane is macOS plus Linux rb, cannot claim native Windows. No desktop/build deployment authorized.)_ _(triage 2026-10-14: WAITS — Needs the named native platform/CI runtime; this machine is macOS.)_

- [ ] After Boss restarts the Windows Rust build, verify a captured `tuic bg` launcher returns while its command still runs and a worktree archive hook finds Git. Existing CI regressions exercise both contracts; the live desktop backend does not hot-reload these changes. _(audit 2026-10-13: HUMAN — Requires native Windows runtime/build or Windows Nightly artifacts; this lane is macOS plus Linux rb, cannot claim native Windows. No desktop/build deployment authorized.)_ _(triage 2026-10-14: WAITS — Needs the named native platform/CI runtime; this machine is macOS.)_

- [ ] After Boss restarts `make dev` or rebuilds, verify Windows worktree archive/setup hooks find Git with a long inherited PATH. Hook PATH now keeps Git first, deduplicates directories and stays within cmd.exe limits. _(audit 2026-10-13: HUMAN — Requires native Windows runtime/build or Windows Nightly artifacts; this lane is macOS plus Linux rb, cannot claim native Windows. No desktop/build deployment authorized.)_ _(triage 2026-10-14: WAITS — Needs the named native platform/CI runtime; this machine is macOS.)_
- [ ] After Boss restarts `make dev` (Rust does not hot-reload), exceed the scrollback cap and verify retained command navigation, gutter selection, green prompt ticks and answers-only associations stay on their original rows (#1370-0077). Restart must load backend and frontend together because stored OSC row coordinates changed. _(NOT VERIFIED 2026-10-12: needs the desktop command-navigation gutter and prompt ticks over OSC 133 rows; the headless API shows no gutter or association state. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: VISUAL-OWED — Retained command gutter/tick/answers-only visual row associations require canvas/native render; HTTP rows do not expose gutter state.)_ _(triage 2026-10-14: WAITS — Prior cap/selection/navigation regressions retained; exact live gutter/tick/answers-only row associations require native pointer/paint observation and owned runtime unavailable.)_
## Telegram minimal outbound (#1438-79b4) — Rust rebuild required

- [ ] After Boss rebuilds/restarts the headless daemon, verify drafts have no phone Stop, send/notifications use the single configured chat, and a button press or new message retires previous handles. Live mint verification remains coordinator-owned; no instance was launched here. Rust changes do not hot-reload. _(audit 2026-10-13: HUMAN — Full clause operates real Telegram phone chat; login/bot token and physical phone excluded. Native adapter is integrated, no production registration altered.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
- [ ] After Boss rebuilds/restarts the headless daemon, verify Telegram Stop on _(audit 2026-10-13: HUMAN — Telegram Stop real phone/bot interaction needs configured chat and live agent; no bot credentials/registration/read access authorized.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
  a throwaway phone request: one Escape reaches only the draft-bound current
  epoch, including a replacement Enter delivered before input bookkeeping;
  duplicate/stale Stop never revives a draft. Confirm the selected
  callback label stays disabled. The running Rust backend cannot load these
  changes until rebuilt/restarted; no restart was performed by this peer.

- [ ] Telegram setup (#1515-cb81): after rebuilding/restarting tuic-remote, use Settings on desktop and phone to replace/check a token, observe the MCP-registered agent read-only, enable, pair once within ten minutes or type a chat ID, remove a chat, and inspect safe status. Rust changes require a restart; no desktop instance was launched by the peer. Live Telegram authentication was not exercised. _(audit 2026-10-13: HUMAN — Token/PAT/account login/rotation or real Telegram registration required; credentials, keychain, login and production account mutation explicitly forbidden…)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
- [ ] After Boss restarts the Rust backend, confirm that remote MCP questions show the saved host name, answer only the owning daemon, and disappear when another client answers (#1440-3571). _(NOT VERIFIED 2026-10-12: needs a desktop host mirroring a remote daemon plus a remote agent/GitHub/ACP event; a headless daemon connected to a second daemon works (item 4795) but no mirrored notice, toast or card UI exists to inspect. Evidence `~/Gits/.tmp/run-1706/b5.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: VISUAL-OWED — Remote confirmation host/disconnect source remote_mirror.rs tests and remoteRepoRouting 46 passes; exact native question card owning-daemon settlement rendering not observed.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_

- [ ] After rebuilding Rust, verify remote GitHub review/proposal/conflict notices update the owning dashboard and leave same-path local repositories unchanged (#1443-e2fd). _(NOT VERIFIED 2026-10-12: needs a desktop host mirroring a remote daemon plus a remote agent/GitHub/ACP event; a headless daemon connected to a second daemon works (item 4795) but no mirrored notice, toast or card UI exists to inspect. Evidence `~/Gits/.tmp/run-1706/b5.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: VISUAL-OWED — Remote same-path machine routing source/tests inspected; owning dashboard mutation UI not observable without live remote event and native render.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_

- [ ] After rebuilding Rust, verify remote upstream MCP failures show their host with the popup closed and leave local upstream settings unchanged (#1444-95a4). _(NOT VERIFIED 2026-10-12: needs a desktop host mirroring a remote daemon plus a remote agent/GitHub/ACP event; a headless daemon connected to a second daemon works (item 4795) but no mirrored notice, toast or card UI exists to inspect. Evidence `~/Gits/.tmp/run-1706/b5.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: VISUAL-OWED — Remote upstream errors need native host badge/status popup rendering; remoteAcp/remoteConnections unit boundaries pass, not this exact visible outcome.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_

- [ ] After the Rust restart, check a connected daemon ACP permission/elicitation in AI Chat: host and ACP connection are shown, answer returns to that daemon, settlement/disconnect clears only its cards (#1441-695e). _(NOT VERIFIED 2026-10-12: needs a desktop host mirroring a remote daemon plus a remote agent/GitHub/ACP event; a headless daemon connected to a second daemon works (item 4795) but no mirrored notice, toast or card UI exists to inspect. Evidence `~/Gits/.tmp/run-1706/b5.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: VISUAL-OWED — remoteAcp.wave2-critic retains permission refresh during response passed; full live host/ACP cards plus settlement/disconnect shape not proved visually.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_

- [ ] After the Rust restart, check remote GitHub PR transition bell/native notices show the host once, open remote PR details and never touch a same-path local repo (#1442-2100). _(NOT VERIFIED 2026-10-12: needs a desktop host mirroring a remote daemon plus a remote agent/GitHub/ACP event; a headless daemon connected to a second daemon works (item 4795) but no mirrored notice, toast or card UI exists to inspect. Evidence `~/Gits/.tmp/run-1706/b5.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: VISUAL-OWED — Remote PR transition source tagging tested; full native host-labelled notice/dashboard navigation needs rendering, no real PR transition writes attempted.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_

- [ ] After the next backend restart, verify an ego card opens mobile Chat and shares the question push cooldown (#1078-05cf). Rust changes require a manual restart by Boss. _(NOT VERIFIED 2026-10-12: needs a real agent binary (claude/codex/ego) that does not exist on the rb build box or a desktop Settings/AI Chat UI. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Real mobile Chat/push cooldown observation needs phone and configured Push; excluded physical device/push account.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
- [ ] After rebuilding/restarting TUIC, verify sidebar dirty and merged badges with 11 writing worktrees, including initialized submodules; sample Git child spawns with the same before/after method (1491-2ae4). Rust backend changes require a manual restart. Include recovery after a PR proof lookup failure without moving refs, and `git rm --cached` leaving both a staged deletion and an untracked file (dirty count 2). _(PARTIAL 2026-10-11: only the `git rm --cached` clause was run: `worktree_list` reported `dirty_files: 2` for a staged deletion plus the untracked file. The 11-writer Git spawn-rate sample and the PR-proof-lookup-failure recovery were not run. Evidence `~/Gits/.tmp/totest-1011b/ev/git4.json`. Isolated headless daemon built from main d2f8f13df.)_ _(audit 2026-10-13: HUMAN — Requested sampling of Boss real seven/eleven writing worktrees and sidebar state cannot be reproduced on owned disposable sessions without changing Boss workload; synthetic CPU load forbidden.)_ _(triage 2026-10-14: WAITS — Needs the specified eleven active writing worktrees and coordinated Git-spawn measurement.)_
## Hands-free reply controls audit (#1377-16e1)

- [ ] [VISUAL] Capture the pill while speaking and while user-paused after Boss loads the desktop build. Check Pause/Stop and Play/Stop respectively. No test desktop may be launched by a managed peer. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: VISUAL-OWED — Screenshots unavailable and image viewing unsupported; rendered appearance not proved.)_ _(triage 2026-10-14: WAITS — Previous production DOM phase controls verified; speaking/paused native pill screenshots need real armed audio and available capture.)_
- [ ] [HUMAN] On a real phone, tap Pause, Play and Stop during a spoken reply. The reply resumes at its position; Stop drops queued replies; microphone capture stays active. Mouse command dispatch, accessible labels and 44px coarse-pointer targets are present in source; they do not prove real-phone touch/audio behavior. _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:4924)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_

- [ ] Workflow slice F: after Boss restarts the Rust backend, start a published Ready story in Plans and Stories; inspect decision/pause details and later event pages, answer input, explicitly resolve a graph pause, pause/cancel. Plan start must show unavailable until slice E is integrated. Browser screenshot/layout verification is owed: the screen was locked on 2026-10-06; no desktop instance was launched. _(NOT VERIFIED 2026-10-12: the Plans and Stories UI is needed; API half: `start_graph` and `cancel` worked and `start_plan` was refused while another run of the same plan was active, but the answer-input and graph-pause resolution steps need an attempt that asks for input (no agent binary). Evidence `~/Gits/.tmp/run-1706/b7.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: VISUAL-OWED — Plans/Stories graph and run-history UI rendering/navigation still owed; native serial graph/check tests pass but cannot prove visible controls.)_ _(triage 2026-10-14: WAITS — Workflow UI/RunStore tests and actual saved-history/SSE replay evidence retained. Full published Ready lifecycle plus screenshots needs integrated fix/runtime; workflow launch defect1883-eba0 and rb outage prevent completion.)_

- [ ] After Boss restarts the Rust build, start a published Resolve plan through the owning daemon API; verify the pinned coordinator and explicit integration waits. The dialog plan button remains visibly unavailable. _(PARTIAL 2026-10-12: `start_graph` for the published `Resolve plan` ran through the owning daemon; the run pins plan definition revision 2 and story definition revision 1, and the daemon started a `coordinate` attempt. Explicit integration waits could not be reached because that attempt failed without an agent binary. Evidence `~/Gits/.tmp/run-1706/b7.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Published Resolve plan runtime fixed-point tests pass but native dialog visibility and owning-daemon live pinned coordinator not exercised…)_ _(triage 2026-10-14: WAITS — Requires integrated workflow launch fix1883-eba0 and owned daemon for coordinator/integration waits; earlier contract tests retained. rb unavailable and no actual successful plan launch claimed.)_
- [ ] AI Chat ACP steering: after Boss restarts `make dev` or loads a rebuilt release, use an ego build advertising `_ego/steer`; send text during a turn from desktop/mobile and confirm it affects the next provider request with one echoed bubble. Attachments retain the queue; delayed rejected/not-busy steering stays ahead of later submissions. If ego leaves a steer unanswered, verify delivery uncertainty appears after 10 seconds and later messages can proceed without resending that text. _(Rust backend does not hot-reload; no desktop instance was launched.)_ _(NOT VERIFIED 2026-10-12: needs a real agent binary (claude/codex/ego) that does not exist on the rb build box or a desktop Settings/AI Chat UI. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Full live steer requires paid/provider conversation plus configured ego extension; no disposable live provider task was started. Existing extension regression not yet rerun; no PASS.)_ _(triage 2026-10-14: WAITS — Needs the installed ego build advertising _ego/steer and a live provider turn.)_
- [ ] After Boss restarts `make dev` (or rebuilds release), open Plans and Stories → Run history and refresh incidents for a recorded failed/input-required run. Confirm retained report and bound session/task evidence. Rust does not hot-reload; worktree UI preview alone does not verify the live backend. _(PARTIAL 2026-10-12: `workflow_run incidents` on the paused run returned an entry with source `run_state`, cause "Run is paused; no more specific cause is retained" and a next action; the Run history UI refresh and a recorded failed or input-required report with session and task evidence were not reachable (no agent). Evidence `~/Gits/.tmp/run-1706/b6.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: VISUAL-OWED — Plans/Stories graph and run-history UI rendering/navigation still owed; native serial graph/check tests pass but cannot prove visible controls.)_ _(triage 2026-10-14: WAITS — Actual isolated SSE/restart history and dialog replay evidence retained in workflow-history-api.log and workflow-history-browser5.log; exact live incident refresh/report/session/task integration still needs reachable rebuilt runtime.)_
- [ ] After Boss restarts `make dev` or rebuilds, create a coordinator chat with **New conversation with options**, send a turn, switch to a daily chat and reopen the coordinator after restart. Confirm its header and dedicated mail peer. Rust changes require a restart; no desktop instance was launched by the agent. _(NOT VERIFIED 2026-10-12: needs a real agent binary (claude/codex/ego) that does not exist on the rb build box or a desktop Settings/AI Chat UI. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Full clause reopens chat after restarting Boss backend; restart explicitly forbidden and cannot validate preserved live state by restarting host.)_ _(triage 2026-10-14: WAITS — Needs a configured ego provider conversation and app restart controlled by Boss.)_
- [ ] ego Perimeter (#1401-ab1c): after Boss restarts `make dev` or rebuilds the release, open Settings > AI Chat with an ego that supports `config ls --effective`; save roots/read-only access/allowlists, toggle network, and confirm the effective preview. With a profile that inherits user roots, confirm those roots appear before saving; explicitly declared `roots=[]` must stay empty. Rust does not hot-reload. Automated focused verification is recorded in the story; the worktree visual harness does not load the new IPC backend. _(NOT VERIFIED 2026-10-12: needs a real agent binary (claude/codex/ego) that does not exist on the rb build box or a desktop Settings/AI Chat UI. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — EgoPerimeterSection 9 passes readback/access/allowlists/network/refusal (vitest.log); editing Boss effective permissions or network config not authorized.)_ _(triage 2026-10-14: WAITS — Needs the installed ego config API and an isolated profile with recorded effective roots.)_

- [ ] After Boss restarts the Rust build, create a worktree through MCP without spawning: only the creator in the same repository moves; inactive selection and sibling tabs stay in place. Explicit spawn and foreign callers must not move the creator. _(PARTIAL 2026-10-11: worktree-created events carried the right creator_session and spawn_session flag for no-spawn, spawn and foreign-caller creates, and no create changed the declaredWorktrees record of another session. Tab movement in a UI was not observable headless. Evidence `~/Gits/.tmp/totest-1011b/ev/wt2.json`. Isolated headless daemon built from main d2f8f13df.)_ _(audit 2026-10-13: HUMAN — Worktree creation performed successfully, but caller/sibling native tab routing cannot be safely asserted by moving this active verification peer or Boss tabs…)_ _(triage 2026-10-14: WAITS — Prior isolated API creator/spawn/foreign metadata checks passed; remaining actual tab-routing observation requires isolated desktop unavailable to peers.)_

- [ ] After Boss restarts the Rust build, call `session action=declare_worktree worktree_path=<existing linked worktree>` from an agent. Only its tab moves; retry creates no duplicate. Reconnect/restart preserves placement and real shell cwd. Unknown, main-checkout, cross-repository and foreign-session targets are rejected. _(PARTIAL 2026-10-11: valid declare stored the placement once in repositories.json declaredWorktrees for the caller only; retry created no duplicate; unknown path, main checkout and cross-repository targets were rejected with no placement change. Reconnect/restart persistence and shell cwd were not checked. Evidence `~/Gits/.tmp/totest-1011b/ev/wt2.json`. Isolated headless daemon built from main d2f8f13df.)_ _(audit 2026-10-13: HUMAN — Full item requires reconnect/restart and real shell cwd/tab placement; host restart forbidden. Source declare_worktree caller validation inspected, no peer moved.)_ _(triage 2026-10-14: WAITS — Prior isolated valid/retry/rejection placement checks passed; reconnect/restart shell cwd and native tab placement remain unobserved. rb offline and no desktop launch.)_
- [ ] Tablet terminal input (#1565-562a): on iPad Safari and the installed PWA, tap near the bottom prompt of a terminal: confirm the keyboard stays open without a window jump. Repeat near the middle, type text, hold Backspace, and dismiss/reopen the keyboard. _(2026-10-07: 12 targeted tests pass; a real-component Chrome harness verifies stable touch/mouse focus and shared text/deletion handling. Screenshot: `~/Gits/.tmp/tuic-canvas-touch/canvas-touch-ipad.png`. Real iPad keyboard animation remains unverified.)_ _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:4959)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Linux scrollback arena retention (1567-843a) — daemon rebuild required

- [ ] [HUMAN] On an iPhone PWA, tap + on Sessions, then tap "Search repositories". The New Session sheet must stay visible above the keyboard (not the Sessions list), and typing a folder name or a parent folder must filter the list. _(Story 1572-6be7: sheet backdrop moved from fixed to absolute inside the visual-viewport shell; needs real iOS keyboard.)_ _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:4970)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## PWA voice, slice 1 (#1573-84f8) — client only, no Rust change

Needs an iPhone (iOS 16.4+, 18.4+ preferred), the desktop TUICommander running a build that contains `feat/pwa-voice-s1` (the phone loads the frontend of the instance it opens), and the instance reachable over HTTPS (Tailscale HTTPS URL). Headless `tuic-remote` has no voice route: the mic button must be absent there.

- [ ] **[HUMAN]** Open `https://<tailscale-host>/mobile` in Safari, open a Claude or Codex session. A microphone button must sit left of the tasks button in the header. Open the same page over plain `http://<ip>:9876/mobile`: tap the mic; a toast "Voice not started" must say the page needs https (not "Cannot read properties of undefined"). _(audit 2026-10-13: HUMAN — PWA voice sequence specifically requires real iPhone/Safari microphone/permission/silent-switch/lock. Hardware and physical device excluded; no phone access attempted.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] **[HUMAN]** Back on HTTPS: tap the mic. iOS asks for microphone permission once; allow. The button turns red and the subtitle shows `Voice waiting`. Check `GET /dictation/hands-free` on the desktop: `armed: true`, `owner` starting `browser-`, `sessionId` = the open session. _(audit 2026-10-13: HUMAN — PWA voice sequence specifically requires real iPhone/Safari microphone/permission/silent-switch/lock. Hardware and physical device excluded; no phone access attempted.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
- [ ] **[HUMAN]** Say the activation phrase and a short request. Subtitle moves `capturing` → `transcribing` → `holding_back` → `delivered`; the text arrives in the agent. If the agent calls `voice speak_reply`, the reply must play **from the phone speaker** and the desktop must stay silent. Silent reply with the mic light on = the suspended-AudioContext bug (#1573-84f8) is not fixed: note the iOS version and Safari vs installed app. _(audit 2026-10-13: HUMAN — PWA voice sequence specifically requires real iPhone/Safari microphone/permission/silent-switch/lock. Hardware and physical device excluded; no phone access attempted.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
- [ ] **[HUMAN]** Repeat the whole sequence from the installed Home Screen app (Add to Home Screen). Record whether the permission prompt reappears on each arm. _(audit 2026-10-13: HUMAN — PWA voice sequence specifically requires real iPhone/Safari microphone/permission/silent-switch/lock. Hardware and physical device excluded; no phone access attempted.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
- [ ] **[HUMAN]** Tap the red mic: it returns to normal, the Safari mic indicator goes off, `armed: false` on the desktop. _(audit 2026-10-13: HUMAN — PWA voice sequence specifically requires real iPhone/Safari microphone/permission/silent-switch/lock. Hardware and physical device excluded; no phone access attempted.)_ _(triage 2026-10-14: WAITS — Needs the physical iPhone microphone session from this section.)_
- [ ] **[HUMAN]** Arm, then press Back to the session list: the mic indicator must go off and `armed: false` (the screen disarms on leaving). _(audit 2026-10-13: HUMAN — PWA voice sequence specifically requires real iPhone/Safari microphone/permission/silent-switch/lock. Hardware and physical device excluded; no phone access attempted.)_ _(triage 2026-10-14: WAITS — Needs the physical iPhone microphone session from this section.)_
- [ ] **[HUMAN]** Deny the microphone permission once (Settings > Safari > Microphone): the toast must show the refusal and the button stay un-armed; `GET /dictation/hands-free` stays `armed: false` and no `browser-` audio socket remains. _(audit 2026-10-13: HUMAN — PWA voice sequence specifically requires real iPhone/Safari microphone/permission/silent-switch/lock. Hardware and physical device excluded; no phone access attempted.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] **[HUMAN]** Silent switch on, then off: record whether the reply is audible in each state (unverified, report 2026-10-07 section 8). _(audit 2026-10-13: HUMAN — PWA voice sequence specifically requires real iPhone/Safari microphone/permission/silent-switch/lock. Hardware and physical device excluded; no phone access attempted.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
- [ ] **[HUMAN]** With the desktop armed in its own conversation, tap the mic on the phone: record what happens (single `DictationState` conversation, untested). _(audit 2026-10-13: HUMAN — PWA voice sequence specifically requires real iPhone/Safari microphone/permission/silent-switch/lock. Hardware and physical device excluded; no phone access attempted.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
- [ ] Lock the screen mid-conversation: expected to stop capture (not supported by iOS, slice 2 handles it); record what the page shows on return. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — PWA voice sequence specifically requires real iPhone/Safari microphone/permission/silent-switch/lock. Hardware and physical device excluded; no phone access attempted.)_ _(triage 2026-10-14: WAITS — Needs iPhone lock-screen capture behavior.)_
## Terminal Chat view for Grok terminals (1791-b8be) — backend restart required

- [ ] After a manual `make dev` restart or `make build`, open a Grok terminal (`grok` started from TUIC): **CLI | Chat** is offered, Chat shows the prompt, thoughts, folded tool cards and replies, and a new reply appears within ~2 s. With two Grok tabs in one folder each shows its own conversation. _(Rust change; not verifiable by the agent: needs the desktop WebView and a restarted app. Unit tests cover the adapter on a recorded fixture and the exact-registry binding.)_ _(triage 2026-10-14: WAITS — Recorded Grok adapter/exact registry and Chat component evidence retained; actual installed Grok conversation and two-tab isolation need owned vendor runtime plus desktop visual check.)_
## CLI / Chat view for Claude terminals (1568-7b55) — backend restart required

- [ ] After `make dev` restart, open a Claude terminal with a bound session: **CLI | Chat** appears top-right, Chat shows your prompts as bubbles and replies as text with tool calls folded, and a new reply appears within ~2 s without switching back. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: VISUAL-OWED — chatView and follow tests pass for snapshot/replacement subscriptions; live switch top-right position/reply appearance needs native visual check.)_ _(triage 2026-10-14: WAITS — Recorded transcript subscription/follow checks retained; actual live bound Claude reply timing and native switch position need owned vendor runtime and visible desktop.)_
- [ ] Visual check of the switch position (top-right) against the grid scrollbar and the last-prompt bar. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: VISUAL-OWED — Screenshots unavailable and image viewing unsupported; rendered appearance not proved.)_ _(triage 2026-10-14: WAITS — Reproduced failure and linked story retained; regression verification awaits an integrated fixed build.)_ _(FAILED 2026-10-14: CLI/Chat switch overlays10visible Context preview glyphs; existing story #1568-7b55 worklog updated. Evidence: ~/Gits/.tmp/results/to-test-1014/phase2-terminal-toggle-overlap-proof.json.)_

- [ ] #1491: After Boss restarts the desktop with the combined fixes, sample direct Git children with seven active writers and initialized submodules; verify <= 2 Git spawns/s sustained and final sidebar badges update after the five-second trailing refresh. _(NOT VERIFIED 2026-10-12: needs seven live writers plus a Git child-spawn sampler over a desktop session; the 11-writer sample was not run on the rb box. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Requested sampling of Boss real seven/eleven writing worktrees and sidebar state cannot be reproduced on owned disposable sessions without changing Boss workload; synthetic CPU load forbidden.)_ _(triage 2026-10-14: WAITS — Needs coordinated seven-writer desktop workload and native sidebar observation; cannot change Boss workload or generate artificial CPU load.)_

- [ ] After Boss restarts `make dev` or rebuilds the release, confirm the scrollback row-allocation fixes from #1575 are loaded. Rust does not hot-reload. Automated native checks and isolated headless measurements are recorded in story `1575-2c7b`. _(NOT VERIFIED 2026-10-12: the item only asks to confirm the fixes are loaded in Boss's backend; the rb daemon runs the branch but the measurements belong to story 1575-2c7b. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Boss runtime restart confirmed by supplied ed776d323, but runtime memory/row-allocation-specific source behavior not probed…)_ _(triage 2026-10-14: WAITS — This check explicitly asks about loaded Boss backend and runtime allocations; recorded isolated measurements retained, no Boss restart or unsafe native probing.)_
## Managed Claude background output (1012-f12e) — backend rebuild required

- [ ] #1407-1ab2: After the next Rust restart, confirm resize keeps full terminal text and wrapped secrets stay redacted in session output; repeat the recorded Claude streaming/idle resize check. Automated coverage: `resize_preserves_complete_visible_logical_text_1407` and the two MCP resize/capture redaction regressions. _(PARTIAL 2026-10-11: resize to 30, 40, 100 and 120 columns kept the redaction marker intact and leaked no key text; the recorded Claude streaming/idle resize check was not repeated. Evidence `~/Gits/.tmp/totest-1011b/ev/resize-redact.json`. Isolated headless daemon built from main d2f8f13df.)_ _(audit 2026-10-13: HUMAN — terminal.log resize-content/captured Claude repaint tests passed; complete row also asks live wrapped-secret output redaction, not executed to avoid sensitive output/global clipboard…)_ _(triage 2026-10-14: WAITS — Prior real resize/redaction API and recorded Claude repaint evidence retained; whole check still requests fresh live streaming/idle resize. Owned remote runtime unavailable, no sensitive output introduced.)_

- [ ] iPad remote access after the next backend restart: open `/` without an authenticated cookie, sign in through the form, and confirm the touch interface loads. Recheck the originally reported connection failure on the physical iPad; no attributable transport error was available in server logs. Rust auth changes require a manual `make dev` restart by Boss. _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:5006)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
- [ ] Telegram Stop (#1521-52cd): after Boss restarts the rebuilt headless daemon, start a Telegram-bound reply, press Stop and verify the bound agent sees Escape. Send a replacement message and verify a repeated old Stop cannot interrupt it. Also delay draft begin until replacement bytes are written: it must not arm the retired turn. Backend byte ordering and captured Codex submission are covered by targeted tests; live Telegram client rendering remains unverified. _(audit 2026-10-13: HUMAN — Token/PAT/account login/rotation or real Telegram registration required; credentials, keychain, login and production account mutation explicitly forbidden…)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_

- [ ] #1596-df57: In macOS desktop, focus a terminal on a branch with an automatically opened PR detail popover. Wheel over the sidebar without clicking; it must scroll, and typing must still reach the terminal. Click outside the popover: it closes and that first click activates the underlying control. Repeat with no popover, after a pane resize, and after a tab drag. Browser hit-testing reproduces the old full-window overlay; native wheel and visuals need Boss’s next-build check. _(audit 2026-10-13: HUMAN — Exact check acts on Boss active native window/repository or named pre-existing worktree; no live interaction outside owned disposable sessions authorized.)_ _(triage 2026-10-14: WAITS — Prior browser hit-testing evidence retained; actual native wheel, typing focus and first-click dispatch require available desktop interaction.)_

- [ ] #1597-fab6: In macOS desktop, with each of the status-bar ticker popover (right-click a ticker), the status info balloon (click truncated status text), the sidebar GitHub panel (GitHub badge on a repo) and the Smart Prompts dropdown open, wheel over the sidebar without clicking; it must scroll. Click a sidebar control outside the popup: the popup closes and that first click activates the control. Clicking the GitHub badge or the info text again must close its popup (not re-open it). Browser hit-testing reproduces the old overlays; native wheel needs Boss’s next-build check. _(audit 2026-10-13: HUMAN — Exact check acts on Boss active native window/repository or named pre-existing worktree; no live interaction outside owned disposable sessions authorized.)_ _(triage 2026-10-14: WAITS — Prior browser overlay hit-testing evidence retained; native wheel and first-click activation remain unobserved, with desktop interaction unavailable.)_
## Terminal Chat Compose input (#1599-db8a)

- [ ] Visually confirm Compose sits below the conversation without overlapping it in a running app, and answer a permission prompt in CLI. The worktree Vite/stealth-browser screenshot attempt timed out on the shared browser; native rendering remains unverified. _(audit 2026-10-13: VISUAL-OWED — Rendered position/layout/native focus or touch observation remains unavailable; no screenshot attempts per task. Related source/tests consulted, no visual pass claimed.)_ _(triage 2026-10-14: WAITS — Production Compose layout/browser metrics and permission component evidence retained; requested native overlap/CLI permission response needs isolated desktop and live agent unavailable.)_

- [ ] After Boss restarts `make dev` (or rebuilds release), retry removal of the clean merged `feat/upload-cookie-1512` worktree. The Rust permission fix is not loaded by frontend HMR; the original worktree was left untouched during verification (#1607-0733). _(audit 2026-10-13: HUMAN — Exact check acts on Boss active native window/repository or named pre-existing worktree; no live interaction outside owned disposable sessions authorized.)_ _(triage 2026-10-14: WAITS — Exact requested worktree belongs to Boss; removal is outside disposable-fixture authority and requires Boss to choose the rebuilt runtime.)_
## Headless GitHub account login validation

- [ ] After landing and rebuilding `tuic-remote`, verify browser add-account device-flow login resolves and stores the named account. The shared helper now compiles without desktop. Rust changes require a manual restart to load; no desktop instance was launched. _(audit 2026-10-13: HUMAN — Token/PAT/account login/rotation or real Telegram registration required; credentials, keychain, login and production account mutation explicitly forbidden…)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_

- [ ] Mobile AI Chat on iPhone: confirm wrapped long suggestions/tool titles and table-local scrolling at 360–430px, session-style input auto-grow, attachment/Send icons and Park/Stop toolbar. Chromium component-layout evidence is recorded for #1625-c04f; phone touch and keyboard remain to check. _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:5028)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
- [ ] On iPhone PWA, check the update strip below the header in Commander and Paper; confirm that the native status-edge fade does not cover its text (#1626-9529). Chromium contrast is verified; iOS compositor behavior is not. _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:5029)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Mobile slash button parity (story 1609-9faa)

- [ ] In a disposable agent session on the phone PWA, compare typing `/` and tapping the keybar `/`: both must show the same live agent commands and navigation. Insert `/` within an unsent draft and confirm surrounding text survives without submission. _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:5032)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_

## Mermaid 12 (1590-7c6b)

- [ ] After Boss's next packaged `make build`, confirm the macOS bundle declares `LSMinimumSystemVersion` 12.0. Mermaid diagrams require system WebKit updated to Safari 17.4+; the OS version alone does not guarantee that update. No desktop instance was launched in this lane. _(audit 2026-10-13: HUMAN — tauri.conf.json:85 minimumSystemVersion=12.0; exact next packaged bundle/Safari update clause not observable from make dev and no release build authorized.)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_

- [ ] After Boss's next `make dev` restart or `make build`: copy and paste text in the desktop app with the arboard commands; confirm terminal copy still succeeds after an IPC await and macOS paste shows no system Paste pill. Rust changes require the restart to load (#1651-6cbe). _(audit 2026-10-13: HUMAN — Native Finder/clipboard/drop interaction spans another app; excluded multi-app/hardware boundary. No Boss clipboard or files changed.)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_
### Spoken replies toggle (#1659-f3cc)
- [ ] After Boss restarts make dev (Rust does not hot reload), arm hands-free on mobile, turn Spoken replies off, dictate, and confirm text answers with no reply audio. Turn it on again and confirm audio returns when the speech service is available. _(audit 2026-10-13: HUMAN — Real mobile microphone/TTS audio off/on judgement requires physical phone/audio hardware; excluded.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] After an Edge access rejection, confirm the UI reports the cool-down and dictation continues; wait five minutes before retrying speech. _(audit 2026-10-13: HUMAN — Requires live Edge TTS rejection plus audible reply recovery while dictating; cannot produce/observe real hardware cooldown without voice conversation.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] Mobile Ctrl menu (#1660-b7f6): on iPhone/iPad with the real keyboard open, open Ctrl, choose each key, dismiss outside, and check Ctrl+C/Ctrl+D danger colours. Ctrl+Enter submits with Claude CSI-u and ego LF; Codex/OpenCode/Goose/Grok/pi insert a newline. Other mappings and uncertainty are recorded in docs/evidence/ctrl-menu-1660/README.md. Desktop browser verification reserves keyboard space; real iOS keyboard remains to check. _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:5057)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_

- [ ] After Boss restarts `make dev` (or rebuilds release), use the rebuilt CLI to open a persistent directory and verify its sidebar row becomes active; repeat without losing tabs. Verify MCP `repo add` works and `ui tab` rejects `tuic://open-repo` with an actionable error (#1668-9b59). Rust and the installed CLI require rebuild/restart; the current running app does not load these edits. _(audit 2026-10-13: HUMAN — Exact check acts on Boss active native window/repository or named pre-existing worktree; no live interaction outside owned disposable sessions authorized.)_ _(triage 2026-10-14: WAITS — Needs owned rebuilt CLI/backend plus visible sidebar navigation; remote daemon currently unreachable and native app launch forbidden. No Boss repo registration or tab changes.)_
- [ ] After a manual Rust rebuild/restart: verify a failed short-name remote host names DNS resolution, suggests its advertised Tailscale FQDN, and shows a retry delay without a simultaneous Connecting/error state. Verify HTTP 403 Untrusted Host says the daemon rejected the name. Backend and renderer regressions cover these paths; desktop verification remains pending because this task forbids launching or accessing the live app. Two isolated headless Chrome screenshot attempts hung (the second stopped at its 45s outer limit). _(audit 2026-10-13: VISUAL-OWED — remoteConnections store passed clear failed-attempt details; native 403/DNS code inspected; actual visible simultaneous/error/retry state still unavailable.)_ _(triage 2026-10-14: WAITS — Three renderer/store tests and four rb DNS/403/retry tests pass; short-name advice reads advertised Tailscale FQDN at remote_runtime.rs:716. Native visual observation remains because MacControl recording is denied and Chrome cannot capture images while locked. Evidence: ~/Gits/.tmp/results/to-test-1014/phase2-remote-errors-proof.json.)_

- [ ] After rebuilding/restarting TUIC, open `https://tuicommander.com/blog/claude-cache-tiers.html` with MCP `ui action=tab`; confirm the page renders and localhost dashboards and asset previews still load. CSP config is embedded at build time; no desktop instance was launched for this change. _(audit 2026-10-13: VISUAL-OWED — Rendered position/layout/native focus or touch observation remains unavailable; no screenshot attempts per task. Related source/tests consulted, no visual pass claimed.)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: native CSP/WebView page rendering cannot be proved by Chromium.)_
- [ ] Telegram desktop owner: after a manual `make dev` restart or `make build`, enable a checked bot token with no tuic-remote running; confirm Connected (this app), link a private chat, and ask an agent to call Telegram MCP `register`. Start tuic-remote first in a separate check; confirm the desktop reports Connected (tuic-remote) and leaves its registration intact. Close the owner and confirm the standby takes over. Rust changes require restart; no desktop build was launched by the managed peer. _(audit 2026-10-13: HUMAN — Token/PAT/account login/rotation or real Telegram registration required; credentials, keychain, login and production account mutation explicitly forbidden…)_ _(triage 2026-10-14: WAITS — Needs Boss’s Telegram bot/private chat pairing; no credentials are entered.)_
- [ ] After rebuilding/restarting `tuic-remote`, start it before tailscaled, then verify its FQDN `/health` changes from 403 to 200 within 30.5 seconds plus scheduler delay after tailscaled becomes ready, provided detection completes within its 500 ms budget; verify stop/rename removes the former FQDN. Rust changes require a manual restart to load; coordinator owns mac-mint deployment. _(audit 2026-10-13: HUMAN — Requires coordinated mint/Tailscale daemon rollout/restart or server lifecycle; only disposable local state authorized and real daemons must remain intact.)_ _(triage 2026-10-14: WAITS — Needs coordinated daemon/tailscaled service startup; do not restart live services.)_
- [ ] After Boss restarts the Rust backend, run a Once automation that needs input and one that fails. Verify one bell/native alert per run/status, canonical saved output and owning-session navigation, configured Push and registered-agent Telegram delivery, and no replay after restart. Rust does not hot-reload (#1620-82ae). _(audit 2026-10-13: HUMAN — rust-extra.log 83 passed; live_channels_claim_once_and_restart_never_replays_ambiguous_sends; routing_claims_push_only_for_live_allowed_transitions; automationNotifications.test navigation…)_ _(triage 2026-10-14: WAITS — Needs native bell/push/Telegram delivery with Boss-owned subscribers.)_

- [ ] Design Mode v2 pilot: after Boss rebuilds/restarts TUIC, load the unpacked extension into its dedicated Edge profile; verify toolbar opening, activeTab capture and sidebar return after switching tabs. Rust does not hot-reload. CFT sidebar and trusted top/frame picks were verified in story 1672. _(audit 2026-10-13: HUMAN — Design Mode requires Boss dedicated Edge profile/extension browser lifecycle; explicitly excluded. No browser/profile launched or reloaded.)_ _(triage 2026-10-14: WAITS — Needs Boss’s dedicated Edge extension/profile reload.)_

- [ ] After the next Rust rebuild/restart, verify Design Mode sends a captured crop as a readable image and lets retained feedback reach a replacement after its original recipient exits (#1672-cd3d). Rust does not hot-reload. _(audit 2026-10-13: HUMAN — Design Mode requires Boss dedicated Edge profile/extension browser lifecycle; explicitly excluded. No browser/profile launched or reloaded.)_ _(triage 2026-10-14: WAITS — Needs Boss’s loaded Design Mode extension and a captured crop feedback session.)_

- [ ] After a manual `make dev` restart or `make build`, verify account removal rejects the github.com default and its site; use `clear_secret` to log out and confirm login keeps the default listed (#1679-1419). _(PARTIAL 2026-10-12: `remove github.com` was refused ("The github.com default cannot be removed; use clear_secret to log out", HTTP 500) and `remove_site` of its site was refused ("The github.com default site cannot be removed"); `clear_secret github.com` returned cleared:true and `list` still showed the default. The OAuth login step was not run (no GitHub). Evidence `~/Gits/.tmp/run-1706/b1.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Token/PAT/account login/rotation or real Telegram registration required; credentials, keychain, login and production account mutation explicitly forbidden…)_ _(triage 2026-10-14: WAITS — Needs throwaway GitHub account authentication and native secret entry by Boss.)_
### Jira provider B9 — after account HTTP integration

- [ ] After A1/A3c account HTTP wiring lands, restart the test build when ready and load the Jira plugin. Verify two site accounts register independently, unload removes both providers, a Jira browse URL resolves to the right account, and Start work opens the core draft flow. Current checkpoint has a tested injected-fetch activation seam; default plugin activation intentionally awaits the real host integration. _(triage 2026-10-14: WAITS — Injected-fetch provider evidence retained; full host-account wiring, two real Jira sites and browser URL/start-work integration require updated provider plus operator accounts. No login or credential entry.)_
- [ ] Once a Jira Cloud site exists, replace the unrecorded fixtures and perform the live checks in `plugins/jira/README.md`. Confirm search/comment pagination, creation metadata across issue types, per-card board transition IDs, ADF write/read, and Retry-After behavior. _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
- [ ] Work-item setting controls: after integration, verify the five controls in their consuming settings and Tasks forms; account Add requires the separate native account sink. The dev-only field preview covers the controls independently. _(triage 2026-10-14: WAITS — Needs the named integration/plugin release to land and load before the consumer check.)_
- [ ] Settings transaction foundation: after Boss's next manual restart/build and the later transport cutover, verify stale revisions conflict, a rejected batch writes nothing, and persisted listener changes report pending/recovery until applied. Internal service only at this stage; no new tool is exposed. _(triage 2026-10-14: WAITS — Needs the named integration/plugin release to land and load before the consumer check.)_
- [ ] After the next manual Rust restart, verify Tasks integration against the A4 contract when its UI/transport stream is installed. A4 itself adds a library contract and local provider only; no new visible surface. _(triage 2026-10-14: WAITS — Needs the named integration/plugin release to land and load before the consumer check.)_
## Tasks detail integration (#1687-27a3)

- [ ] After the Tasks shell is integrated and the next normal rebuild loads the updated plugin guide, open a work item from its list. Confirm the shell passes visibility, opens the selected provider/account/item, and receives authoritative detail updates. Component behavior is covered by the targeted detail tests and rb browser fixture; this item covers the separate shell integration. _(NOT VERIFIED 2026-10-12: needs the web frontend served and driven in a browser; the rb build box has no browser or built frontend and the lane builds Rust only through rb, so no UI was rendered. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: VISUAL-OWED — TasksShell.test.tsx 4 + TasksDetail.test.tsx 12 passes (tasks.log): shell/detail selection and provider boundary; recorded/demo provider, not external authenticated provider…)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
- [ ] Mobile session history (#1681-507e): after Boss manually restarts `make dev` or rebuilds release, verify a long-running Claude Code session keeps completed output above its live region while scrolling on iPhone. Rust does not hot-reload; this peer did not restart Boss's app. _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:5090)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
- [ ] [HUMAN] #1674-93d5: On iPhone Safari and separately the installed Home Screen PWA, tap the composer mic beside Send. Verify interim/final speech appends without opening the keyboard, stop works, Send remains explicit, and server hands-free cannot record simultaneously. Check permission refusal preserves the draft. While dictating, edit the draft: the mic returns to idle, the edit stays exact, and tapping the mic starts a new recording. Historical WebKit standalone restriction: https://bugs.webkit.org/show_bug.cgi?id=225298#c3. At 360×800 the controls remain in the existing row; Send/mic targets grow from 40 to 44 px, costing 4 vertical px rather than another terminal row. _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:5092)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_

- [ ] After a manual `make dev` restart (or `make build`), verify the line review APIs through the integrated Files UI: outdated/resolved threads, replies, resolution and rejection after the PR head moves. Rust changes require restart; automated checks use recorded GitHub fixtures/local stubs. _(NOT VERIFIED 2026-10-12: needs a GitHub PR and the Files UI; no GitHub token or network call is allowed in the isolated lane. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Full item includes GitHub PR merge/update/close/review/queue writes; prohibited external live writes. Read-only githubReviewTransport and githubPrActionsTransport passed…)_ _(triage 2026-10-14: WAITS — Recorded GitHub transport/Files component evidence retained; actual outdated/resolved thread replies/resolution and moved-head rejection require an owned PR/account. No external review/thread writes performed.)_

- [ ] [HUMAN] #1691-bdff: Open a PR with review threads from "View diff" on a real repository (desktop build). Check: threads sit on their lines and an outdated thread is labelled and listed above its file; Resolve/Unresolve and Reply show GitHub's answer; hover a line, click "+", add a comment, then Submit review as COMMENT on a throwaway PR; push a commit to the PR, then Submit review must say the PR has new commits and keep the pending comments. Check that long files no longer overlap the next file header. Automated: recorded fixtures and a local stub, Chromium + WebKit iPhone screenshots. _(audit 2026-10-13: HUMAN — Full item includes GitHub PR merge/update/close/review/queue writes; prohibited external live writes. Read-only githubReviewTransport and githubPrActionsTransport passed…)_ _(triage 2026-10-14: WAITS — Needs a real authenticated PR and native review UI; MacControl Screen Recording.)_

- [ ] After manual `make dev` restart (or `make build`), verify integrated PR actions: failed-job rerun, auto-merge enable/disable, enqueue/dequeue and queue state. A queue-required merge must enqueue; a moved head or unavailable branch policy must prevent writes. REQUEST_CHANGES with blank text must fail. Automated checks use recorded responses and local stubs only. _(NOT VERIFIED 2026-10-12: needs live GitHub PR actions (rerun, auto-merge, queue); no GitHub token or network call is allowed in the isolated lane. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Full item includes GitHub PR merge/update/close/review/queue writes; prohibited external live writes. Read-only githubReviewTransport and githubPrActionsTransport passed…)_ _(triage 2026-10-14: WAITS — Recorded/local-stub action evidence retained; actual rerun, auto-merge and queue writes need throwaway GitHub PR/account and branch policy. No live account mutation performed.)_
## Private account secret entry (#1696-c6d4)

- [ ] After a manual `make dev` restart (or `make build`), add an Enterprise _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Token/PAT/account login/rotation or real Telegram registration required; credentials, keychain, login and production account mutation explicitly forbidden…)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
  account: Settings shows only the host field; Add account opens the private
  native token window. Submit a valid PAT and confirm the account appears.
- [ ] Decline and close before submitting; confirm no account appears. Retry _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — cancelled_form_writes_nothing passed (rust-extra.log); full item also needs invalid PAT provider response, login/credential boundary explicitly excluded.)_ _(triage 2026-10-14: WAITS — Needs Boss’s native account secret form; never enter credentials.)_
  with an invalid PAT and confirm no partial account is created.
- [ ] No UI launches rotation yet. Send `POST /accounts/secret-form` with _(NOT VERIFIED 2026-10-12: `POST /accounts/secret-form` with `{"action":"rotate","id":"nope"}` on the headless daemon returned HTTP 500 "Secret requests require a desktop host to open the private form"; the item needs the native private form and a real account. Evidence `~/Gits/.tmp/run-1706/b9.log`. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Token/PAT/account login/rotation or real Telegram registration required; credentials, keychain, login and production account mutation explicitly forbidden…)_ _(triage 2026-10-14: WAITS — Needs a throwaway credential account and native secret form.)_
  `{"action":"rotate","id":"<account id>"}` to the test instance; confirm
  presence remains set and provider requests use the replacement credential.
- [ ] Confirm native entry has no mobile link or developer tools.

## GitHub Work desktop load (1693-a816) — runtime integration pending

- [ ] #1693-a816: After Boss starts an isolated desktop verification instance and completes GitHub device-flow login (`repo read:project`), run the one-command account/module probe in `docs/user-guide/plugins.md` → GitHub Work post-login verification (`node scripts/github-work-live-check.mjs ... --isolated-desktop`). Runtime/catalog/account HTTP and B6/B7 bindings already exist on main; do not reimplement them. The probe checks desktop loaded state and read-only installed-module/account ports, not Tasks rendering or actual registry unload. Separately confirm paged normal/review comments, draft start-work intent and registered-provider cleanup in Tasks. No desktop launch, account probe, credential-store inspection or GitHub mutation was performed by gh-1693.
- [ ] With `read:project`, record a successful Projects v2 read and verify board moves edit the Status field without closing the issue; verify roadmap date/number fields round-trip. The current credential returns `INSUFFICIENT_SCOPES`; this item has no success evidence.
- [ ] Confirm native entry has no mobile link or developer tools. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_

## Tasks create integration (#1688-518d)

- [ ] After the Tasks shell is integrated and a provider plugin that follows the `itemType` convention is installed, open the create form, choose a type, fill its required fields and create. Confirm the created item opens in Tasks detail. Component behavior is covered by the targeted create tests and the rb browser fixture; this item covers the shell and real-provider integration. _(NOT VERIFIED 2026-10-12: needs the web frontend served and driven in a browser; the rb build box has no browser or built frontend and the lane builds Rust only through rb, so no UI was rendered. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — TasksCreate.test.tsx 7 + TasksShell 4 passed (tasks.log), itemType required fields/created item opening. Real authenticated provider creation would write GitHub/GitLab/Jira and is forbidden.)_ _(triage 2026-10-14: WAITS — Targeted create/shell/component and prior browser evidence retained; actual provider itemType/create/detail flow needs an updated loaded provider and operator account. No external item created.)_
## Hands-free dictation keeps the whole sentence (1164-ee4b) — Rust restart required

- [HUMAN] After a manual `make dev` restart, arm hands-free on a Claude terminal and say "computer, <phrase one>, <short breath>, <phrase two>". The terminal receives one message holding both phrases, equal to the transcript box. Say the keyword plus a phrase, wait 4 s, then speak again without the keyword: the second phrase joins the same message. After 5 s of silence the message is sent. _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:5119)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [HUMAN] Let the agent speak a reply through the speakers with the microphone open. The reply is not taken as user speech (no new transcript from the TTS audio). Also watch the mic gain for a minute: it stays steady. _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:5120)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
  Covered by tests: `a_one_second_breath_keeps_both_phrases_in_one_utterance`, `keyword_turn_keeps_a_five_second_follow_up_in_the_same_terminal_message`, `speech_started_before_the_five_second_deadline_is_not_submitted_mid_phrase`; 91 continuous and 99 root dictation tests. Not verified: TTS not accepted as speech on the real speaker/microphone setup (the only open criterion); acoustic gain.
## Hands-free spoken reply does not interrupt itself (1376-f33e) — Rust restart required

- [HUMAN] After the backend restart, on Boss's Edge/speaker/microphone setup, arm hands-free and ask for a long spoken reply. It plays to the end without pausing or stopping itself. _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:5125)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [HUMAN] Speak over the reply without the wake word: the reply pauses, then resumes from the same point. Speak the wake word over the reply: it stops and the new turn starts. _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:5126)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
  Covered by tests: recorded Edge echo replay, `a_residual_echo_burst_does_not_stop_the_reply`, `scattered_residual_echo_frames_do_not_add_up_to_a_voice`, wake/no-wake reply tests (466 tuic-dictation, 149 root dictation, 2026-10-02). Not verified: real playback with the microphone open and real-voice barge-in (the two open criteria).
## Scrolled-up terminal keeps its lines during output (1264-89c8) — frontend only (HMR)

- [ ] [VISUAL] In the running app, start an agent that prints a lot of output. Scroll up with the wheel and with the scrollbar while it writes. The lines under the cursor stay put and no gap or repeated block appears. Scroll back down: every line is present, in order. Repeat once with a long session at the scrollback limit. _(audit 2026-10-13: VISUAL-OWED — Screenshots unavailable and image viewing unsupported; rendered appearance not proved.)_ _(triage 2026-10-14: WAITS — Canvas output/scroll regressions passed in core-terminal-tests.log and isolated browser canvas checks; trusted native wheel/drag and visual no-gap observation remain unavailable.)_
  Covered by tests: `CanvasTerminal-scroll-during-output` (rebase of the gesture offset, committed rows, backend history rewind), `canvasTerminalScroll` tests, `tuic-terminal` `history_*` capture tests. Not verified: the live scroll feel. Known open gap: under sustained output the 400 ms settle fallback can move the view forward by a few lines.
## Scrollbar shows one tick per user prompt (1388-a2e6) — Rust restart required

- [ ] After the restart, open a long real Claude Code session (many prompts, many tool calls). The scrollbar shows one green tick per user prompt at the right height, and no solid green band at the bottom right. Tool calls add no tick. _(audit 2026-10-13: VISUAL-OWED — Explicit screenshot/long-real-session prompt-tick appearance; no screenshot tools available.)_ _(triage 2026-10-14: WAITS — Earlier prompt-mark backend/renderer checks retained; exact long real-session green tick appearance and no band needs available screenshot/native observation. No Boss session used.)_
  Covered by tests: `tuic_state_busy_carries_no_prompt_row` (busy hooks carry row -1, prompt hooks the real row); headless run with 300 busy hooks and 3 prompts gave 3 ticks. Not verified: a screenshot of a long real session (earlier screenshots used simulated hooks).
## Answer highlight covers the whole answer (1389-3a47) — frontend only (HMR)

- [ ] Scroll to a quoted `💬` line in a long pasted Claude prompt whose start is more than 32 rows above the viewport. The quoted line has no answer gutter or hidden marker; the real answer does. Repeat after scrolling far away and back to exercise a cold row cache. Confirm a blank-separated indented genuine answer remains visible. _(round 2: automated planner regressions cover both warm and missing prompt-prefix caches; live rendering remains unverified.)_
- [ ] [VISUAL] In a live Claude session, get an answer with a 💬 line, wrapped rows and 3 or more continuation lines. Every row of the answer is tinted, up to the next bullet, prompt or marker. Nothing after the answer is tinted. The answers-only view shows the same extent. Take a screenshot. _(audit 2026-10-13: VISUAL-OWED — Screenshots unavailable and image viewing unsupported; rendered appearance not proved.)_ _(triage 2026-10-14: WAITS — Recorded answer-shape/presentation tests retained; exact live multirow tint and required screenshot cannot be captured while Chrome is locked and MacControl recording denied.)_
  Covered by tests: `answerExtent` (shared by overlay and answers-only view), suggestOverlay 32 and answersTurn 23 tests, critic tests for fenced code and indented prompts. Not verified: a live session screenshot.
## Mobile sub-agent rows nest under the coordinator (1698-46a8) — Rust restart required

- [HUMAN] After the restart, open the sessions list on the iPhone. The coordinator row's `tuic_session` equals the children's `parent_session`, and the sub-agent rows are nested under the coordinator (not a flat list). _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:5146)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
  Cause is story 1673 (a resumed coordinator PTY lost `tuic_session`), fixed by 4ca81ccaf and a5cd0e7fd; Boss's running app predates it. Covered by tests: the 1673 real-process regressions. Not verified: the story's own RED, the phone-payload nesting test and the iPhone screenshot are not written.
## Remote add-repository picker with the Verification server (1594-f297)

- [ ] Connect the Verification server and open Add repository. The picker lists the home directory. If the server answers with HTML (wrong version, base URL or route), the picker shows an error with the URL and content type, and the path field stays empty. _(audit 2026-10-13: HUMAN — RemoteRepoPicker 16 passed including HTML content-type failure/path empty; real Verification endpoint/authenticated path not supplied and login is excluded.)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
  Covered by tests: rpc test for a `text/html` 200 answer to `get_home_directory`; transport and picker tests (321), full run 8581. Not verified: the root cause on the real server (version, base URL, route) and the home-directory listing; both need the real Verification server URL and an authenticated path.

- [ ] After Boss restarts the Rust backend: verify Enterprise account creation through the core account manager and native secret form, and that existing accounts still list and resolve repo bindings. The legacy PAT add IPC command and POST /github/accounts route are removed. _(audit 2026-10-13: HUMAN — Token/PAT/account login/rotation or real Telegram registration required; credentials, keychain, login and production account mutation explicitly forbidden…)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
## GitHub HTTP writes (1710-124a) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, create a PR, create an issue, and submit a review with inline comments on a throwaway repository bound to the intended GitHub/GHE account. Confirm the bound identity is used without a `gh` executable and validation errors retain HTTP status/body. Targeted tests cover request coordinates, fields, review validation, and error propagation; no desktop instance was launched or restarted. _(audit 2026-10-13: HUMAN — Explicit PR/issue/review writes to real GitHub/GHE forbidden, even throwaway repository; transport regressions passed only.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_

- [ ] After a backend rebuild, hold mobile Send, select Queue while an agent is busy, and verify one submission at its next ready window with an empty input box. Queue errors must preserve local text and warn to inspect the session (#1717-efd3). _(audit 2026-10-13: VISUAL-OWED — Rendered position/layout/native focus or touch observation remains unavailable; no screenshot attempts per task. Related source/tests consulted, no visual pass claimed.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
## Launcher child reaping (1707-bd40) — Rust restart required

- [ ] After landing, Boss must manually restart `make dev` or install `make build` to load the Rust fix. Open a built-in editor, a custom tool and macOS microphone settings, then use the desktop normally for a day. Confirm its child processes contain no zombie older than one minute. Real-process regression tests cover immediate exit and retained wait ownership; the day-long production observation remains pending. No desktop instance was started or restarted by this peer. _(audit 2026-10-13: HUMAN — Day-long Boss desktop child observation and microphone Settings/editor interaction not safely owned; restart/launching apps forbidden.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_

- [ ] After a manual backend restart, right-click a localhost/HTTPS terminal link and select Start in Design Mode. Confirm the dedicated browser opens the exact path/query/fragment and its sidebar connects that terminal agent; start a second link while armed and confirm it navigates rather than stops. New backend code does not hot-reload. _(audit 2026-10-13: HUMAN — Design Mode requires Boss dedicated Edge profile/extension browser lifecycle; explicitly excluded. No browser/profile launched or reloaded.)_ _(triage 2026-10-14: WAITS — Exact native terminal context-menu to dedicated extension browser/rebind flow needs isolated desktop and owned agent; source/UI tests cannot prove native launch, and Boss browser not touched.)_
- [ ] S5 CI logs: after Boss restarts the backend, check auto-heal on github.com and a GHE-bound repo without `gh`; verify failure tails and selected external CI logs. The Rust change requires a manual `make dev` restart or `make build`. _(audit 2026-10-13: HUMAN — Auto-heal can cause real GitHub/GHE writes and needs configured account/external CI token. Read-only request path source github.rs:3434–3448 inspected; external writes/login excluded.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
- [ ] Mobile browser dictation (#1718-c32a): on iPhone, dictate and stop three times; edit a dictated draft, then dictate again. Confirm no repeated `aborted` toast and no lost or duplicated text. Browser error/end timing requires device replay. _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:5167)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_

- [ ] After a manual backend restart and extension reload (#1735-080b): start Design Mode on a static page under `.tmp`, run MCP `design_mode action=reload repo_path=<repo>` and confirm the tab reloads once; close the browser and confirm the D badge clears within about 60 s. New backend code does not hot-reload. _(audit 2026-10-13: HUMAN — Design Mode requires Boss dedicated Edge profile/extension browser lifecycle; explicitly excluded. No browser/profile launched or reloaded.)_ _(triage 2026-10-14: WAITS — Needs the installed extension/profile operated by Boss; no profile reload performed.)_
## PR action buttons (#1738-eb5e)

- [ ] [HUMAN] After a restart, on a PR of a throwaway repository: "Re-run failed jobs" (failed Actions check), "Enable/Disable auto-merge", "Add to / Remove from merge queue" (queue-protected branch only) and "Request changes" (blank body is blocked) reach GitHub and show the outcome. Unit tests stub the invoke boundary; no live write was made. _(audit 2026-10-13: HUMAN — Real GitHub rerun/auto-merge/queue/review writes forbidden; githubPrActionsTransport tests pass only.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_
## Tasks shell (#1737-29f7)

- [ ] [HUMAN] After a restart with a work-item provider plugin loaded (github-work, jira or gitlab with an account): Command Palette shows "Open Tasks"; the tab lists items, opens a detail pane, creates an item, shows the board and the roadmap. Without a provider the action is absent. _(Unit tests use the in-memory demo provider only.)_ _(audit 2026-10-13: HUMAN — TasksShell 4/component detail/create 19 pass with demo provider; real provider plugins are loaded per read-only __TUIC__.plugins(), but account-based creation is external write forbidden.)_ _(triage 2026-10-14: WAITS — Prior Tasks shell/create/board/palette evidence retained; full real-provider creation requires loaded plugin and account. NOTE: newer Tasks setup1841-bbac intentionally offers Open Tasks with no provider, superseding the old action-absent clause.)_
## Compact mobile attachment (1719-ff5d)

- [ ] On iPhone, tap the attachment icon left of the session composer, choose a photo/file, confirm upload status then its path in the draft, and confirm Queue stays disabled during upload. Smaller mic and Send controls remain inside the right input border. _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:5179)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_

- [ ] [VISUAL] #1264-89c8: after a manual Rust restart, scroll up gently during sustained agent output, stop the gesture, then use scrollbar drag and return to the bottom. Verify no forward jump or gap, including capped history. Rust does not hot-reload; restart only when ready to lose the current session. _(audit 2026-10-13: VISUAL-OWED — Screenshots unavailable and image viewing unsupported; rendered appearance not proved.)_ _(triage 2026-10-14: WAITS — Prior canvas scroll/browser and targeted test evidence retained; exact sustained-output trusted wheel/drag native visual check needs Boss desktop.)_
- [ ] Mobile: Claude multi-question AskUserQuestion (two questions + Submit) — answer each tab and the Submit review from the phone overlay; needs the TUIC restart for the Rust parser change (#1766-9311) _(audit 2026-10-13: HUMAN — Phone overlay interaction requires physical phone explicitly excluded; source pty/core_tests.rs:10985 covers Submit parser.)_ _(triage 2026-10-14: WAITS — Needs the specified real phone overlay flow.)_
## Automatic agent toast cards (#1760-bab7)

- [ ] After a manual rebuild/restart, create a new worktree after safe removal of a clean integrated one. Confirm that its ignored artifacts persist, its new branch appears, and the old sidebar row is gone. Rust does not hot-reload. _(audit 2026-10-13: VISUAL-OWED — Live repo worktree_create chore/totest-J-1013 reused archived clean worktree path, warm node_modules/dist present and status done…)_ _(triage 2026-10-14: WAITS — Prior actual clean archive/reuse preserved warm artifacts; requested visible old-row removal/new-branch and lifecycle still need owned backend plus desktop/browser routing. rb unavailable; Boss worktrees untouched.)_
- [ ] After Boss rebuilds/restarts the Rust backend, verify overlapping worktree creates both succeed, a rename-journal error leaves the original checkout usable, and fingerprint-confirmed forced deletion still works with a live session (#1705-67b3). _(audit 2026-10-13: HUMAN — Live safe archived rename/reuse succeeded on owned test branch and warm artifacts retained. Exact overlapping-create/error/fingerprint-forced-delete scenario not tested…)_ _(triage 2026-10-14: WAITS — Needs isolated concurrent-create/rename-failure/force-delete fixture on rebuilt backend; earlier safe reuse evidence retained. rb unavailable; Boss worktrees untouched.)_
## Mobile ego questions (1759-6047) — Rust restart required

- [ ] After rebuilding/restarting TUIC and updating ego, ask ego a real single-choice question from a terminal. On iPhone confirm **Input**, tap the header question icon, read the full question and both long options, and tap one option. Confirm one answer, continued output and no terminal-height loss. Browser rendering uses the recorded real ego question; real phone touch remains to check. _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:5194)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
- [ ] Desktop dictation in terminal Chat: select transcript text, hold Fn and dictate, then confirm Compose receives the text with its draft preserved. Move focus or switch tabs during the final pass; confirm delivery stays with the original target. Close the original tab; confirm the transcript stays in the error message and Messages bell (#1773-5b69). Automated target/Compose tests cover routing; native Fn and microphone observation remain for Boss's normal app session. _(audit 2026-10-13: HUMAN — Physical phone/tablet or audio/Fn hardware explicitly excluded; source mapping to-test.md:5200)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
- [ ] Beta update channel after backend rebuild: Settings → General → Expert Mode → Update Channel offers Beta and preserves it after restart. Before the first beta feed exists, Check for Updates reports no beta release (or offers a newer stable). After a beta tag workflow completes, beta.1 offers beta.2 from its immutable download page; after 2.0.0 publishes, it offers the final in-app. Stable channel stays on stable. Rust changes require Boss's manual restart to load. _(audit 2026-10-13: HUMAN — updater.test.ts 45 passes cover beta.2/final/stable/missing beta feed. Actual saved restart + release workflow publication requires permitted restart/release artifact beyond scope.)_ _(triage 2026-10-14: WAITS — Prior updater45-test evidence retained; full immutable beta.1→beta.2→final feed progression depends on published release artifacts and coordinated native updater restart, not executable in an audit lane.)_
## Shared form controls (1815-01c9)

- [x] Tasks create, Settings account/list fields, Automations, Git review, worktree creation, AI Chat elicitation, desktop filters and mobile New Session render with shared themed controls. _(verified: worktree Vite screenshots in `~/Gits/.tmp/results/form-controls/`, before/after per migration group; mobile 390×844 retains 44px fields.)_
- [x] Native select keyboard/form behavior, reactive values, refs, change targets, multi-selection, disabled state and semantic classes remain intact; the Tasks view switch supports arrows/Home/End. _(verified: `src/__tests__/components/FormControls.test.tsx` and existing migrated-module suites.)_
- [ ] #1753-f6b8 (Rust restart required): after restarting TUIC, open Inspect Launch Instructions on a session with 10+ MCP reconnects. Confirm one **MCP initialize instructions** row with a count and last-seen time, plus the Launch brief row; close the session and reopen: the error shows the backend reason.

- [ ] GitHub Work (#1693-a816): after a manual `make dev` restart or `make build`, sign in again with an existing GitHub OAuth account and confirm consent includes Projects read access (`read:project`). Existing tokens require reauthorization; no automatic upgrade occurs. Verify a Projects v2 board loads rather than reporting `INSUFFICIENT_SCOPES`.
## Goose reply completion (1812-022d) — Rust restart required

- [ ] After Boss restarts `make dev` or installs `make build`, send two prompts to Goose 1.49.0 with ollama. Each reply must return the tab to idle shortly after the ready composer appears; the second submit must be accepted without a five-minute wait or 409. The recorded first-turn replay covers the backend transition; this live check remains open until restart. _(triage 2026-10-14: WAITS — Needs Goose 1.49.0 and a running ollama model.)_

- [ ] #1792-7450: After Boss manually restarts `make dev` (or rebuilds release), confirm Claude Chat still loads and follows its transcript. After installing/updating and trusting TUIC native Codex hooks, open two Codex tabs in one cwd: each CLI / Chat view must show only its own completed conversation. Replacing a thread resets Chat; leaving Codex retracts availability. Without native hooks, a heuristic/resume ID must not enable Chat. Native hook capture was verified without a model call; automated boundary tests cover exact binding and revocation. No desktop instance was launched for this change. _(triage 2026-10-14: WAITS — Exact-binding hook capture/regression evidence retained; full two installed Codex tabs/Claude follow/replacement/revocation needs trusted native hooks and owned vendor sessions. No Boss hooks or sessions modified.)_
## Terminal Chat layout (#1832-b19a)

- [ ] After Boss restarts `make dev` or installs `make build`, load the repository settings owners and verify defaults/local read, patch and readback when settings transport cutover is wired (#1825-4566). Current transports remain unchanged; Rust edits require restart. _(triage 2026-10-14: WAITS — Needs the named integration/plugin release to land and load before the consumer check.)_
- [ ] GitHub Settings (#1829-54c2): after Boss restarts `make dev` or rebuilds release, verify one-account/one-remote repositories hide Repository Bindings, OAuth shows Logout, and environment Disconnect stays disconnected across reopening Settings. Then complete device-flow login; fork/upstream remotes must still offer a remote choice. Rust changes require restart; unit regressions use fake credentials. _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_

- [ ] Design Mode autoload (#1834-f6c4): after restarting `make dev` or rebuilding release, close the old Design Mode window, start from an agent terminal in each repository, verify the dev-server page, Pick element, and feedback delivery. Chrome is not installed locally; branded Chrome must report its unsupported-autoload error. _(triage 2026-10-14: WAITS — Current user guide and v2 launcher explain dedicated Edge extension autoload and branded Chrome137+ unsupported path; actual cold profile, Pick and feedback integration require isolated native desktop unavailable.)_
## Idle Claude placeholder composer clears the activity dot (1864-23d0) — Rust restart required

- [ ] After Boss restarts `make dev` / installs `make build` and the tuic-remote daemon, launch Claude Code from a shell tab and leave it at the `❯ Try "..."` placeholder: the blue activity dot clears in tuic-remote and in the desktop app, and returns while a turn runs. The replay test `captured_claude_launch_from_shell_returns_idle_at_its_ready_composer` covers the detector; the live dot is unverified. _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_
## Design Mode v2 S0 spike (1672-cd3d)

- [ ] [HUMAN] Once MacControl Screen Recording works (`capture_screen` returns an image), run S0 spike items (a), (b), (e) and (f) and record the evidence in `docs/design/design-mode-v2-s0-results.md`. Needs the Screen Recording grant of Boss; no restart of TUIC. _(Moved from the story by Boss's rule 2026-10-13.)_ _(triage 2026-10-14: WAITS — MacControl capture returned Screen Recording permission required on 2026-10-14.)_
## Mobile command input layout (1625-b873)

- [ ] [VISUAL] On the iPhone PWA after a reload, open a session: Send is outside the input box (36 px visible, 44 px hit area), the mic sits inside the box, and the text wraps at the same width as before. Chromium measured 286 px wrap at 390 px; the iOS Safari layout is unverified. No Rust restart. _(Moved from the story by Boss's rule 2026-10-13.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_
## Worktree recycling benchmark (1705-67b3) — Rust restart required

- [x] Native isolated headless API measurement completed on 2026-10-14: 15 recycled creations, 10.802 s total; Defender peak 284.4%, fseventsd peak 19.1%. See `docs/evidence/worktree-recycling-1705-native.md` for raw data and baseline comparison. The small serial fixture does not prove production speedup or Defender reduction. No desktop restart needed for this isolated check.

- [ ] After Boss rebuilds/restarts the Rust backend, create 15 worktrees in a row with recycling and compare the time and the Defender/fseventsd CPU with the 2026-10-10 benchmark. Rust does not hot-reload. _(Moved from the story by Boss's rule 2026-10-13.)_ _(triage 2026-10-14: WAITS — Needs coordinator-owned benchmark/mutation scheduling; no expensive batch gate started by this peer.)_
## Launch marker instructions (1810-c28c) — Rust restart required

- [ ] After Boss restarts `make dev`, start Claude, Codex, grok, goose, pi and ego from a TUIC shell (typed command, alias and resume) and one opencode tab; open **Inspect Launch Instructions** and confirm each agent got the marker text through its own flag (or `OPENCODE_CONFIG_CONTENT` for opencode), once, and that a launch by absolute path is the known miss. The flags were verified only against the installed `--help` of 2026-10-12. _(Moved from the story by Boss's rule 2026-10-13.)_ _(triage 2026-10-14: WAITS — Installed help/launch-flag and controlled argv evidence retained; full typed/alias/resume launch instructions for seven actual vendors needs owned rebuilt runtime and live sessions. rb unavailable; no Boss composer used.)_
## Terminal Chat file links (1835-a278)

## Truncated dictation incident (1763-3361) — Rust restart required

- [ ] When Boss meets a truncated dictation again, correlate its time with the file log (`focus-loss` stop lines, `~/Gits/.tmp/results/ui-1763-dictation-evidence.log` shows the method) and note whether the cause was the Fn focus-loss stop. The 11:49/11:51 focus-loss stops and the RED prove the native defect only; the link to all three reported truncations is unproven. _(Moved from the story by Boss's rule 2026-10-13.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_
## Recycled checkouts in `__archived` (1866-142a) — Rust restart required

- [ ] After restart, the orchestrator sidebar no longer lists `feat/agent-vm` and `feat/multi-coordinator` (neither as worktree nor as Merged branch row). Then `repo action=worktree_remove path=<orchestrator> branch=feat/agent-vm` removes the archived checkout and the branch (check `git worktree list` and `git branch` before and after). Run on the real repo only after Boss ok. _(REALITY criterion of 1866-142a.)_ _(triage 2026-10-14: WAITS — Needs Boss’s orchestrator repository sidebar; do not mutate its archived worktrees.)_
- [ ] Tasks setup (1841-bbac) — desktop only: with no work-item plugin installed, open Tasks (Open Tasks is offered) and install GitHub Work from the prompt; save a repository in the form and confirm the plugin registers its provider without a restart (`reloadPlugin` after writing `data/config.json`). Then open **Provider settings**, change the repository, save, and confirm the list reloads. _(The browser harness stubs `invoke` and the plugin loader; the real reload is not verified.)_ _(triage 2026-10-14: WAITS — Five targeted TasksSetup/TasksShell checks passed in tasks-setup-contracts.log for install offer, repository form/save error and provider settings. Actual desktop plugin installation/reload and registered provider remain unverified; tests mock invoke and reloadPlugin, and native plugin install is unavailable in this lane.)_

- [ ] After a manual `make dev` restart or release rebuild, verify GitLab planning dates still render in Roadmap and have no list filter/group control (`queryable` schema support requires the rebuilt Rust backend).

- [ ] After restarting `make dev` or installing a rebuilt app, confirm Tasks restores the source, view and list query for each repository/account. Confirm the settings catalogue also remains available. Rust `UIPrefsConfig` stores JSON-encoded `tasks_view_preferences`; the running desktop backend needs a restart to load this field (#1852-35a8).
- [ ] Tasks board (#1844-858d): on a configured provider with multiple pages, More retains prior cards and updates loaded lane counts; cards show assignees/labels and Move… offers legal destinations.

- [ ] #1873-bbc2: After a manual `make dev` restart, verify Claude/Codex suggest chips appear after the agent turn ends while a background child runs; completed remains gated and chips do not repeat when the child exits.
- [ ] After Boss rebuilds/restarts, verify Tasks Start work in a repository with `.tuic-start-work.md`: key/title/URL substitutions appear in the draft inside the untrusted wrapper; remove the file and confirm the original key/title/body brief. Rust does not hot-reload. (#1855-8ee0)
- [ ] #1874-1758: After restarting `make dev`, remove a scratch worktree whose tip is contained only in another remote branch with Delete branch on remove enabled; verify it succeeds. For unique local commits, verify the refusal offers “Remove worktree, keep branch” and preserves the branch.
- [ ] #1874-1758: After the Rust restart, remove a scratch worktree whose cleanup runs `git fetch --prune` after its sole containing remote branch was deleted. Verify the checkout is removed, the local branch remains, and the branch-deletion warning is shown. Automated backend regression: `removal_keeps_local_branch_when_cleanup_prunes_its_remote_proof`.
- [ ] #1874-1758 F3: After a manual Rust restart, verify a scratch worktree removal warns and keeps the local branch when a concurrent force-fetch conflicts with its remote proof. Automated backend regression: `removal_keeps_local_branch_when_fetch_replaces_proof_during_ref_deletion`.

- [ ] After Boss restarts `make dev` or rebuilds the release, attach a file to a new Claude/Gemini terminal: the path is under TUIC data, the repository and `.git/info/exclude` stay unchanged, and closing the terminal removes its uploads (#1833-aa60). Rust does not hot-reload. Ego ACP binary uploads show the unsupported read-root notice; inline images use ACP content blocks.
- [ ] Compose image attachments (#1758-1613): after loading the frontend, drop PNG/JPEG/WebP/GIF files from Finder onto the editor and confirm ordered image tags at the pointer; confirm no file tab opens and no path is written to the terminal. Verify the image-only multi-file picker and unchanged paste. Native Finder drag requires multi-app interaction.

## Rust close policy (#1802-e6b8) — after manual restart

- [ ] [to-test] After Boss restarts `make dev` or rebuilds release, close the main window with Ask: verify Background, Quit, Cancel and Remember; a cancelled dialog changes nothing.
- [ ] [to-test] With Background, close main and confirm PTYs remain reachable; Cmd/Ctrl+Q still confirms and quits, and a detached panel X closes only its panel. macOS Dock click shows main again.
- [ ] [to-test] With Quit, verify confirmation with live sessions, immediate quit without sessions or with confirmation disabled, and persistence of the remembered policy after restart.
- [ ] After a manual backend restart, verify an unknown `/tasks/no-such-endpoint` returns JSON 404 rather than the SPA shell (tasks API prefix fix).
- [ ] #1873-bbc2 revision: After a manual Rust restart/rebuild, verify Claude stays working/busy while blocking Stop hooks run after final choices; chips publish once when ready. Automated recorded Stop-hook regression and replay oracle cover the backend.


## Background hidden state (#1803-00d9)

- [ ] Workflow branch launch (#1883-eba0): after Boss restarts `make dev` or rebuilds release, launch a workflow coordinator in its registered branch worktree and confirm the attempt binds its session and prompt. Rust changes require restart; the peer did not restart the desktop.

- [ ] After Boss restarts `make dev` or rebuilds release, hide then Show an idle terminal around the standby timeout; confirm the restored active terminal remains responsive. The B1 visibility recheck requires a Rust restart to load.
- [ ] After Boss restarts `make dev` or rebuilds release, close to Background with a main window, floating window and open panel. Verify all hide and Show restores only those previously open. The Rust change does not hot-reload.
- [ ] After restart on macOS, verify Dock icon disappears with `background.hide_dock_icon=true`, remains with false, and returns on Show. Verify Dock Reopen (when icon retained), second app launch, a deep link and a native notification click restore the window and active terminal.
- [ ] On Windows after rebuild, verify close/Show applies Low/Normal WebView memory target without losing terminal output. On Linux verify panel restore. Native tray Show is wired by the following tray story to `background_mode::desktop::leave_background`.
- [ ] #1833-aa60 round 1: after Boss restarts `make dev` or runs `make build`, confirm shared-data instances preserve live uploads, disabled Claude instructions retain attachment access, and custom wrappers launch normally. Rust changes require restart; no desktop was launched or reloaded by the peer.

- [ ] #1879-05ad — After Boss restarts `make dev` or rebuilds the release, open a
  repository Markdown tab on desktop and inspect the session overflow Markdown
  section on iPhone. Confirm title/path, tap to view, and live open/close updates.
  The backend metadata publication and routes need the rebuilt Rust process.
## Background tray and menu-bar icon (#1804-d44b)

- [ ] After Boss restarts `make dev` or builds release: verify the template icon on light/dark macOS menu bars, Windows overflow area and KDE. Check Always/background-only/Never, Show via click/menu, Quit confirmation, one first-entry native notice, live working/awaiting counts and attention clearing on Show. The current Rust process does not hot-reload.
- [ ] On stock GNOME without a StatusNotifier host, close configured as Background must minimize with the main window reachable; Ask must omit Background. Native Linux packaging requires the appindicator dependency.

- [ ] #1806-ed18: after a manual `make dev` restart (or `make build` for release), destroy/unload the main WebView, suspend an idle shell and a resumable agent through MCP, then Show/recreate the window. Confirm suspended notices and pane placement, one replacement PTY per Resume, and refusal for busy/question/queued sessions. Restart while unloaded and confirm durable tabs remain. Automated rb PTY/storage and frontend DOM tests cover the backend and render boundaries; native unload/recreate integration requires the coordinator restart.
## Settings policy foundation (#1786-9cbd)

- [ ] After Boss manually restarts `make dev` or installs `make build`, queued approved writes must honor disconnect and expiry before owner preparation. Automated owner-boundary regressions cover both; settings transport publication remains a separate step.

- [ ] After Boss's manual Rust restart and the separate settings-transport cutover, request a protected remote/auth/TLS/relay/executable edit: verify the desktop/phone dialog uses the backend diff, refusal writes nothing, acceptance commits once, and replay is refused. The policy foundation is covered through isolated owner files and the existing human-response channel; settings transport publication is still separate. Do not restart a live desktop session solely for this check.
- [ ] Workflow Pause fix (#1882-bc88): after Boss restarts `make dev` or rebuilds release, edit a seeded story, publish it, and use Pause → Resume to → Save draft to repair an existing draft. Rust seed/API changes require restart.
- [ ] Mobile composer + menu (#1880-d7ff): on iPhone, check both modes at 390px, native camera/file pickers, browser mic, Chat Park/Restore and busy Queue/Stop, terminal Queue and long-press Send. Confirm keyboard and safe-area positioning.
- [ ] Tasks capability schema: restart the Rust build, then verify GitHub, GitLab and Jira list selectors offer only declared fields; a rejected selection restores the previous value and shows its reason.
- [ ] Jira create: choose Reporter and Assignee by search, enter new comma-separated Labels, fill required Parent for sub-tasks, and verify Project/Type do not require duplicate entry (#1859-c01f).
## Jira JQL errors (#1861-8a4c)

- [x] `jql: project =` shows the recorded Jira parse failure and a correction hint; the typed query remains intact and previous rows are marked stale. _(verified: real TasksShell and Jira provider in the read-only Vite harness, screenshot and UI request trace in `~/Gits/.tmp/fix-1861/`; no text fallback request.)_
- [x] Explicit text, key lookup, valid JQL, saved-filter predicates and board query scoping retain their meanings. _(verified: `plugins/jira/jql-errors.test.js`, 31 Jira tests pass; 11 Tasks consumer tests pass.)_

- [ ] Tasks/plugins batch 1013: after Boss restarts `make dev` (or rebuilds release), verify review-bot findings in Tasks and PR review threads. The merged Rust changes require a backend restart to load.

- [ ] Tasks small-list mode (#1851-7ade): after Boss restarts the Rust backend, check a GitHub small scope shows local queries and title sorting, while unknown-total Jira stays server mode. New project_work_items IPC/HTTP requires rebuild; never reload the desktop during agent work.

- [ ] After a manual backend restart, verify small Tasks lists filter Jira Status and GitHub Type correctly. Rust projection changes do not hot-reload.

### Background WebView unload (#1805-decc)

- [ ] [VISUAL] After Boss rebuilds/restarts desktop TUIC, set unload to one minute, hide with live and suspended tabs plus split panes and detached panels: all WebViews disappear; backend sessions/remote streams continue; Show recreates main and restores tabs/layout and native focus. Check Dock Reopen, tray Show, second launch, deep link and notification entry points.
- [ ] [VISUAL] On the rebuilt desktop, zero disables unload; sleeping does not consume the timer; dictation and close dialogs defer it; Quit while unloaded uses a native confirmation, Cancel keeps sessions, Confirm quits. Verify geometry and updater check after Show. Windows/Linux native unload behavior and memory savings remain unmeasured. No desktop instance was launched by this peer.
- [ ] #1724-fe7e: After Boss rebuilds/restarts TUIC and reinstalls the Design Mode extension, navigate an armed worktree page, get a matching visible-tab PNG, read captured errors, grant/revoke host eval, and verify the sidebar action history. Rust is not hot-reloaded. Verify Claude Code, Codex and ego tool-list refresh on Arm/Stop; cached calls must refuse unarmed sessions.
- [ ] #1885-0352 After Boss restarts `make dev` or rebuilds release, use an isolated fresh project in Workflow Designer: publish Resolve plan as revision 2 with its repository-integrity check; clearing all final checks must reject publication.
## Worktree rename restore records (1705-67b3) — Rust restart required

- [ ] After Boss restarts the backend, recycle a finished linked checkout and confirm the old workspace and declared agent placements are absent from repositories.json and the sidebar; sibling workspaces stay intact. Automated persisted-state and frontend regression tests cover the change; the live app does not load Rust edits through HMR.
- [ ] After a manual `make dev` restart or release rebuild, verify GitLab planning dates still render in Roadmap and have no list filter/group control (`queryable` schema support requires the rebuilt Rust backend). _(triage 2026-10-14: WAITS — Three Tasks date/list tests passed in tasks-date-contracts.log; actual GitLab provider integration needs its planning-date update and an account. Read-only main plugin checkout 4150e41 model.js:32-33 currently exposes milestone text and due date, with no planning-date fields/queryable schema.)_


## Mobile Claude CLI / Chat (#1770-3e28)
- [ ] [HUMAN] On a real iPhone, measure first Chat open and resume after more than 20 seconds hidden at 2000 entries; verify gentle touch detaches during streaming and CLI returns to latest output. Browser timings are recorded separately and do not prove phone performance.
- [ ] After a manual backend restart, open a suspended agent tab and send it input/mail; confirm one replacement PTY and retained peer identity (#1807-4f1f). Rust changes do not hot-reload.
- [ ] After the next manual Rust rebuild/restart, suspend a tab in a disposable linked checkout. Verify cleanup names the suspended tab and refuses; recycling must skip its checkout. Verify removal of a different checkout retires its Worktrees bell action (#1705-67b3).
- [ ] After a manual restart/rebuild, verify that a detached worktree whose non-default branch still points at HEAD does not trigger orphan cleanup. Restore a non-default branch at HEAD after orphan discovery; automatic removal must refuse the stale candidate and preserve its checkout. Fresh in-sync branches must report no integration proof (#1894-7683). Rust changes require restart; do not restart live sessions for this check.

- [ ] After Boss restarts `make dev` or builds release, verify Settings → Agents idle hibernation (60 minutes/background-only by default): an eligible tab frees its process and wakes on open, input or peer mail. Also verify a keep-open peer stays live after its durable identity is rebound to another PTY. Rust backend changes do not hot-reload (#1808-389f).
## Tasks notifications (#1856-1d20)

- [ ] After Boss restarts `make dev` or installs a rebuilt release, enable Settings → Notifications → Tasks, follow an item, and refresh after a new provider comment while the window is in the background. Check the item-labelled OS notice and its provider URL. Confirm a linked GitHub PR failure names the Tasks item without a second notice. Rust comparison changes require the restart; they do not hot-reload.

- [ ] After Boss restarts the Rust backend, verify a followed task notifies for a distinct comment ID at the latest baseline timestamp, while repeated IDs and edits remain silent. The current live backend does not load this comparator fix through HMR.

- [ ] After a manual Rust rebuild/restart, reload cache-keepalive with a question open, then type a draft near its keepalive deadline. No dialog answer or draft clearing must occur. Guarded input requires the rebuilt backend; Rust does not hot-reload (#1742-13a4).

- [ ] After installing the Tasks provider data update and moving the existing GitHub Work `data/config.json` to `data/work-items.json`, confirm GitHub Work loads and Tasks setup saves/reloads its repository. Repeat setup with Jira and GitLab. Backend code is unchanged; the Rust edit adds a regression test only.
- [ ] #1904-9eef: After Boss restarts `make dev` (or rebuilds release), verify new launch/MCP marker instructions permit 💬 at the start of list-item text. In a Claude session, emit `- 💬 **Answer.**` with an indented continuation and nested item, followed by an unmarked sibling; check the green rule and hidden marker in CLI and Chat, then Answers-only scope. Frontend behavior does not require a Rust restart; updated launch instructions do.

## Settings cutover (#1787-4778)

- [ ] After Boss restarts make dev or rebuilds release, enable settings in trusted MCP UI. Confirm an old config-enabled installation does not auto-enable settings.
- [ ] After Boss loads the integrated settings build, use a throwaway named instance to compare prompt create/get/edit/remove over MCP, HTTP and IPC; stale revisions must conflict and disabled meta calls must fail. Isolated MCP/HTTP/native-facade readback passed in batch integration; live desktop IPC remains unverified.
- [ ] Start GitHub device login, log out before completion, then finish browser authorization. The old flow must expire without restoring credentials. Repeat with disconnect and an additional account.
- [ ] Confirm an exposure patch displays a redacted backend diff on desktop and phone; approve once, replay exact request, then verify approval cannot be reused.
## Settings choice discovery (#1908-c327)

- [ ] After Boss restarts `make dev` or installs a rebuilt release, open Notifications and explicitly refresh output devices. Verify real device names/default markers and selection persistence. Disconnect or deny the audio backend and verify failure is not shown as a supported empty device list. No hardware success is claimed by the isolated owner tests.

- [ ] After the next scheduled backend rebuild, refresh Claude run-config model choices, save a model and confirm existing sessions remain unchanged (#1910-451e). No live instance was started during implementation.
- [ ] After the next planned rebuild, verify Telegram token replacement with a real bot, presence-only readback, clear, and add/remove of an isolated private-chat ID. Verify the existing poller wakes without starting a second owner. Do not use production credentials in automated tests. _(Story #1916, read-only desktop check 2026-10-15: `telegram_settings` returned `enabled:true`, `token_set:true`, no token-value property, zero chats, `connected:false`, and `polling_owner:null`. Presence-only readback is verified; live token validation and single-poller wake still need an isolated real bot/private chat. No setup mutation or restart was performed.)_

- [ ] After planned settings cutover/rebuild, set and clear credentials on an isolated saved connection; verify presence-only replies, teardown/reconnect, and that remote-target requests do not mutate local credentials. Automated tests use the in-memory vault, not a real account or Keychain. _(integration 2026-10-17, #1913: the shared settings registry now supplies the production runtime callback. Isolated dispatch tests passed for addressed-runtime reconciliation, disconnected-owner preservation and foreign-target refusal. Real credential storage and live reconnect remain unverified; no credentials or live connection state were accessed.)_
- [ ] After Boss restarts the Rust backend, start a direct Gemini session in a trusted workspace, upload a PNG and ask it to read the image. There must be no trust prompt for the session attachment directory. Repeat with ego in ro, workspace and adaptive sandbox modes; the read must succeed without approval. Check a resumed and a typed-by-name launch. ACP extra-directory grants remain unsupported. (#1833-aa60)
- [ ] After Boss restarts the Rust backend, verify the internal Ego settings owner with an isolated EGO_HOME: exec_paths/profile edits, base-only auth presence, and stale-revision rejection. No live config or credential commands are required (#1907-0f9f).
- [ ] #1438-79b4 (closure handoff 2026-10-15): After Boss confirms readiness and pe-3 is idle, deploy on mint; register a disposable agent with Telegram, send phone text while idle and busy, observe draft/final replies and blocked/done notices, press a callback button, unregister and confirm the no-agent reply.

- [ ] #1547-2f5c (closure handoff 2026-10-15): After Boss manually rebuilds/restarts the backend, open a newly managed agent Prompt Inspector; expand captured launch/MCP sections and verify source, original UTF-8 bytes, redaction/truncation and historical/unobservable labels. Do not launch a second desktop.

- [ ] #1573-84f8 (closure handoff 2026-10-15): On a real iPhone, open a session, tap push-to-talk, allow microphone access, speak and release; verify one audible reply, then leave during arming and confirm microphone capture stops.

- [ ] #1625-b873 (closure handoff 2026-10-15): On iPhone open a terminal composer, enter a multiline draft, verify Send remains outside the text box with a separate mic target and no overlap; open + and dismiss without sending.

- [ ] #1707-bd40 (closure handoff 2026-10-15): After loading the rebuilt desktop, use editor/custom/settings launchers and Design Mode normally for a day; inspect direct children of that desktop PID and confirm no zombie remains older than one minute.

- [ ] #1804-d44b (closure handoff 2026-10-15): After manual rebuild/restart, verify background tray icon and attention dot in light/dark macOS menu bars, Windows overflow and KDE; hide the window, trigger awaiting, use Show and confirm attention clears and window remains reachable.

- [ ] #1834-f6c4 (closure handoff 2026-10-15): Close only the old TUIC-owned Design Mode browser profile window, then in orchestrator and tuicommander use the tab-bar button or terminal link; confirm the page opens without extension setup, Pick element works and the pick reaches the intended agent.

- [ ] #1878-3a3c (closure handoff 2026-10-15): On iPhone mobile Settings toggle keyboard correction ON, type a misspelled sentence in the terminal composer and verify native suggestions; switch OFF and repeat, then reload and confirm preference persists.

- [ ] #1879-05ad (closure handoff 2026-10-15): After loading the rebuilt backend, open Markdown tabs in a desktop repository; on iPhone open its session overflow menu and a Markdown entry, verify the viewer opens, then open/close a desktop Markdown tab and confirm the phone list refreshes without reload and excludes other repositories.

- [ ] #1880-d7ff (closure handoff 2026-10-15): On iPhone check terminal and Chat composers: + / text-with-mic / Send order, + menu outside the textarea, outside-tap dismissal, Attach file and Take photo pickers, Chat Park/Restore, terminal Queue/long-press Send and microphone permission/cancel.
## Voice settings discovery (#1909-0d24)

- [ ] After Boss manually restarts the Rust app, verify Voice input device refresh, missing-backend errors, Whisper model installed/downloadable state and per-language installed/Edge voices. Edge needs an internet connection only on explicit refresh. Exercise download/cancel/delete/import through the existing UI owners with disposable assets; verify subsequent choice refresh and runtime invalidation. No hardware, network or real download success is claimed by fixture tests.

## Settings owner gaps (#1906-5bf8, #1911-6214, #1912-5c77)
- [ ] After Boss restarts the Rust build, use an isolated test owner: complete GitHub cloud device login; verify credential presence only, then enterprise token rotation/removal and binding cleanup. Do not use production accounts for destructive checks.
- [ ] After restart, use an isolated upstream: start OAuth consent, rotate to bearer, rotate/clear a custom header, and confirm only that upstream reconnects; ensure no secret appears in replies/events/errors.
- [ ] After restart, use an isolated registration file: edit a repository label and group name while workspace/tab state changes; verify owner readback, unchanged workspace identities, and no checkout/worktree deletion.

## #1917 — AI Chat conversation deletion

- [ ] Open AI Chat with installed ego; expand Conversations, capture the trash controls and named confirmation, then cancel. Delete a throwaway inactive conversation and confirm only its tab closes. Delete the selected last conversation, reload, and confirm it does not return. While a throwaway conversation is running, attempt deletion and confirm ego's refusal is shown without removing it. Repeat on browser/mobile Chat. No application restart is required for this frontend-only change.

- [ ] #1928-e282: In MCP Settings, hover the long design_mode help for at least three status polls. Check keyboard focus, changed descriptions, viewport edges, scrolling, and resizing; the tip must remain beside its badge without an unpositioned frame.

- [ ] #1929-b1d9: After Boss restarts make dev (Rust does not hot-reload), connect to a Direct short Tailscale host whose short DNS lookup fails. Check automatic FQDN recovery and the explicit Save resolved address action. Do not change production connections for automated verification.
- [ ] After a manual backend restart, verify that a stalled desktop pauses after three abandoned frames, sends no hidden grid frames, still delivers background bells, and restores full content on show; browser watchers must continue independently (story 1934-3c67).

- [ ] After the next CLI rebuild, run `tuic bg` with a long multiline command: verify a single-line completion label of at most 120 characters and the full command before output in the log (#1950-43b7). The installed CLI is unchanged until rebuilt; no desktop restart is needed for this CLI-only change.

- [ ] After a manual backend restart, verify Pocket Italian replies with a missing catalogue voice use giovanni; Settings must retain the selected missing voice and offer Download. Download the selected voice while hands-free stays armed and verify the next reply uses it. Rust changes do not hot-reload (#1954-bab7).
## 1902-7010 — ego terminal provider recovery (requires Boss restart)

- [ ] After Boss restarts `make dev` (Rust does not hot-reload), run a managed
  terminal ego against a provider double that fails then recovers. Check the tab
  says recovering, the status bar shows phase/attempt/next-time/countdown, and
  the parent gets recovery entry and exit notices. Click Force retry once and
  verify `/retry` reaches only that tab. Repeat via MCP `session force_retry`.
  Verify explicit clear, busy/idle and shell OSC 133 A remove the recovery display.
  Check the visual layout at narrow and normal window widths. No desktop instance
  was launched by the implementation agent.

- [ ] 1902 critic revision: after backend restart, type an ego composer draft during
  provider recovery and click Force retry. The draft must remain unchanged and the
  status bar must display the refusal. Clear the draft and verify one retry submits.

- [ ] Tab hints (#1951-8fac): hold Cmd alone (Ctrl on Windows/Linux) for 450 ms and check the first nine terminal-tab badges; release, another key, mouse button, or scrolling must hide them. Automated hook/TabBar coverage and worktree dark/light screenshots passed.

- [ ] Hands-free default shortcut (#1953-3da5): with dictation enabled and a live active terminal, Cmd+Alt+V (Ctrl+Alt+V on Windows/Linux) starts the conversation and a second press stops it. Disabled dictation must stay silent. Dispatcher/store tests pass; microphone/audio confirmation needs real hardware.

- [ ] After a manual `make dev` restart or release rebuild, verify a hands-free Italian reply uses Edge speech on the migrated endpoint with v1 GEC without HTTP 403 (#1952-bf91). The old Rust process does not hot-reload.

- [ ] After the next manual Rust restart (`make dev` or release rebuild), verify Telegram linking confirms with `Chat linked.`, `/start` explains Link chat, invalid/expired codes explain retry, and the consumed Settings hint clears (#1955-2f43). Automated offline poll-loop and Settings regressions cover these paths; no live Telegram API was called during implementation.

- [ ] After the next manual Rust restart, check Settings > Telegram Recent messages for inbound/outbound outcomes. The log is process-local, last 20, redacted and cleared on restart; a standby cannot show another process's log (#1956-c325). Automated offline boundary tests cover retention, redaction, failure categories, callbacks and status readback.

- [ ] #1955-2f43 round 1: after Boss manually restarts `make dev` or installs `make build`, confirm Telegram linking still works. A failed link confirmation must leave the chat linked without delivering the pairing code as agent mail; Recent messages shows the safe failed reply. Offline poll-loop tests cover replay and same-batch order. No live Telegram API or desktop instance was used.

- [ ] Plugin Settings: confirm Plugin identity, source IDs and the opaque Settings shell while switching pages with slow reads, in dark and light themes (#1927-9649).

- [ ] #1900-5ff6: After loading the updated mobile frontend, use the iPhone PWA normally. If the grey screen returns, do not reload until the visible URL and screenshot are recorded. On the next boot, inspect backend warnings with source `app` for `Mobile previous document lifecycle trail`, and compare root count, shell geometry, visual viewport, CSS app height/top and stream phases. The trail is automatic and per-tab; storage eviction can remove it. The original grey-screen reproduction/fix and three physical app-switch cycles remain unverified. No Rust restart is needed for this frontend-only diagnostic change.
## Telegram streaming replies (#1959-162a)

- [ ] [HUMAN] After Boss restarts `make dev` (or rebuilds release), verify the native draft and reaction rendering on iOS and Telegram Desktop for a PTY agent and AI Chat. Send allowlisted text, confirm 👀 then 👨‍💻 after Read/new turn, Italian status, live AI Chat assistant text, 🤔 on a permission wait, one final with 👍, and 👎 plus one notice on failure. Verify queued and no-agent cases, queued Stop versus working Stop, multiple phone messages, and expiration after app exit. This requires the real authorized Telegram client; automated tests use only the recorded public example and offline loopback responses, never the Bot API or bot token. Rust does not hot-reload; restart only when Boss is ready to lose live sessions.

## Plugin review follow-up (#1756)

- [ ] After the existing provider configuration migration, reload GitHub Work, GitLab and Jira twice. Verify one registration per account, and verify that rejected startup reports its error. The read-only live snapshot on 2026-10-15 reported GitLab/Jira loaded, but GitHub Work failed with `Configurable plugin requires a settings schema`; no reload was attempted.
- [ ] Set and use a project ID through the approved plugin settings UI. _(Correction 2026-10-20: current main and plugin pin `56659df3` include schema-backed Jira/GitLab defaults; the earlier missing-form observation is historical, not an outstanding implementation gap.)_
- [ ] Resolve a Jira URL and cancel its in-flight request. _(Correction 2026-10-20: current main and plugin pin `56659df3` propagate cancellation through resolver, provider, client and host HTTP; the remaining gap is live loaded-plugin evidence.)_
- [ ] Install an incompatible plugin in an isolated verification environment and check its refusal message. Catalogue/provider boundary checks passed 38/38 and loader minimum-version checks passed 3/3 (25 filtered) through build-slot on 2026-10-15; these are not native install E2E evidence.
- [ ] Once GitHub Work loads with an authorized test account, invoke a read-only review action through core and verify it reaches the account-bound provider. No vendor writes are authorized.
- [ ] Check same-version stale-copy detection in a supported isolated loaded-plugin runtime. _(Correction 2026-10-20: current main includes catalogue payload identity and installed hashing; version-only comparison is historical. The `tuic-remote` plus Vite path still skips user-plugin loading in `loadUserPlugins`, so it cannot supply this story's required loaded-plugin proof.)_
- [ ] After landing the restored #1756 branch and its plugin commit, set and use the Jira default project ID and GitLab default project through their schema-driven plugin settings pages. Verify an account-specific Tasks project overrides the default. Module tests cover schema registration and persistence; live UI set/use remains unverified.
- [ ] After landing the restored #1756 branch and restarting the backend, resolve a Jira URL and cancel its in-flight request. The restored provider/client/account adapter forwards the signal and the host cancels headers/body reads; confirm this through the loaded plugin, not only the module tests.
- [ ] Install an incompatible plugin in an isolated verification environment and check its refusal message. Catalogue/provider boundary checks passed 38/38 and loader minimum-version checks passed 3/3 (25 filtered) through build-slot on 2026-10-15; these are not native install E2E evidence.
- [ ] Once GitHub Work loads with an authorized test account, invoke a read-only review action through core and verify it reaches the account-bound provider. No vendor writes are authorized.
- [ ] Verify same-version stale-copy detection with an isolated installed plugin and a catalogue payload digest. The host and catalogue now compare payload identities; mutable `data/` files are excluded. Module/package tests are not live installation E2E evidence.

- [ ] #1756-4d42: after Boss restarts `make dev` (Rust does not hot-reload), resolve a Jira URL and cancel while headers/body are pending; confirm no late result and active requests stop. No live restart was performed by w-1756.

- [ ] Telegram ACP replies (#1959-162a): after Boss manually restarts `make dev` or rebuilds release, send two requests and explicitly finish the first before a streaming tick. The first ACP turn must not also answer the second request. The pre-fix loopback regression recorded duplicate finals; post-fix integration and live phone checks remain coordinator-owned. Rust changes do not hot-reload.

- [ ] After Boss rebuilds/restarts TUIC, use an ego executable with local transcript v1 support and verify an exact-bound ego terminal switches to Chat on desktop and mobile, follows completed messages/tool cards, shows goal continuation notices, and resets after compaction (#1969-ac9b). Rust changes do not hot-reload. Prerequisite: land ego #532 (`0821caee`, integrated in `batch/ego-1016-8`) and configure that rebuilt executable in Settings. Deployment still requires the installed ego to support `--format transcript`. Isolated native verification now proves live PID/lease binding, incremental snapshots and SSE wakes through the real 532 binary; desktop/mobile dark/light PDF rasters use that live terminal, not a replay. See `docs/backend/ego-chat-verification.md`. This remaining item is the installed-app restart/deployment check, not a missing consumer integration.
- [ ] Settings typed-owner cutover (#1787-4778): after Boss restarts make dev or installs a rebuilt release, enable settings through trusted UI and verify MCP/HTTP/IPC readback, owner choice refresh, typed credential presence, one-connection reconnect, and rejected remote/non-owning targets in an isolated instance. No live app restart was performed by the agent.
- [ ] After Boss manually restarts `make dev` or rebuilds release, check a disposable no-remote repo whose main checkout uses `feat/macos-mvp`: integrated linked worktrees show Merged, not Unknown. A detached main checkout without main/master or a valid configured base shows No base branch with a reason. Rust does not hot-reload (#1961-bab6).
## Standby job control and remote activity (1962-4182) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, use an isolated disposable PTY: an agent launched as an interactive shell foreground job must remain running when standby would otherwise apply, with no suspended-job line. An eligible directly launched group must wake to a usable prompt. Remote HTTP output reads, typing and observer disconnects must postpone standby for the full configured timeout. The committed real-PTY regressions cover these paths; the current desktop does not load Rust changes until restart.
- [ ] After Boss restarts `make dev` (or installs a rebuilt release), print two ego intents with the second wrapping across the old composer rows. Confirm Context and the non-custom tab title show the latest intent (#1964-b32e). The Rust change does not hot-reload. Automated pre-fix replay evidence is recorded in the story; GREEN is owned by the batch integrator.

- [ ] Mobile CLI and Chat: scroll back to show the key-bar down-arrow above Send, tap to resume latest output, and manually return to bottom to hide it (#1975-4a38).
## Ego mobile question forms (1973-3d1b) — backend restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, open ego single-choice, multiple-choice, and free-text questions from a narrow mobile terminal. Confirm **Input** and the header question action for each. Multiple-choice taps must toggle without submitting until **Submit answers**; free-text must show its full question above the reply composer. The current running backend cannot load this Rust change through hot reload.
- [x] Capture all three recorded ego question cards in Commander and Paper at390×844 and834×844. _(verified: `.tmp/mobile-question-visual/after-1973/png/`; actual SessionDetailScreen with existing Rust replay state; cards end at746px above the first key at747px, Paper contrast11.386:1. Presentation only, not a live PTY or physical-phone check.)_
- [ ] Capture the originally requested360×800 geometry if that exact smaller viewport remains required; this follow-up captured390/834 widths, not360.

## Telegram AI Chat fallback (#1983-1b47)

- [ ] After Boss restarts the Rust backend (`make dev` or a release rebuild), send an allowed Telegram message with AI Chat connected and no explicit Telegram registration. Check receipt, delivery to ego and its final reply on the phone. Connect two chats: the latest-connected chat is the fallback, not the selected tab. Connect the second chat while the first is answering phone mail: the first reply must still arrive, and later mail must reach the second chat. Register another agent: it must retain ownership. With neither registration nor a connected AI Chat, check the no-agent instruction. This needs Boss's restart; Rust does not hot-reload.
- [ ] [VISUAL] #1981-0caf: Capture the session header at 390 px, 430 px, and 834 px in Commander and Paper. Confirm COORDINATOR is readable, CLI/Chat stays direct, phone voice/tasks are in More, and tablet controls stay direct. The local headless capture timed out before producing images; the Mac screen was locked.

- [ ] [VISUAL] #1982-ee68: After the backend rebuild, capture the same tab-separated Claude Edit in Chat at 390 px in Commander and Paper. Expand tool activity and its edit; confirm full-width old/new backgrounds, aligned columns and horizontal scrolling. From phone CLI tap the header Chat toggle, then CLI; verify terminal history and draft remain intact, with CLI following the latest output. Includes Rust projection changes, so Boss must restart make dev or rebuild release when ready; no Rust hot reload.

## Settings MCP live refresh (#1788-2163)

- [ ] After a Boss-controlled `make dev` restart or release rebuild, patch settings through an isolated instance's published settings API while Settings is open. Verify desktop and browser update without a save echo.
- [ ] Keep an unsaved field while MCP changes an untouched field, then the same field. Verify the draft survives and the conflict notice appears. Reconnect the browser and verify a fresh snapshot after the process generation resets.
- [ ] In a remote machine's Agents/Upstreams editor, change settings on that daemon and verify only that machine reloads; local settings must not change. No Boss sessions or live credentials were used by automated verification.

## Settings catalogue UI (#1789-796d)

- [ ] After a Boss-controlled backend rebuild/restart, check plugin shortcut registration, unload and reconnect choice refresh; publication must not write keybindings.json.
- [ ] In an isolated instance, use All settings to edit a scalar, nullable override and string list; verify source/readback, stale-revision conflict, dirty-event preservation, refusal/expiry and persisted-apply-failure recovery. Do not use production credentials.
- [ ] Verify General Resource Limits and All settings at desktop/narrow widths and light/dark themes. Isolated General and All settings screenshots are captured under ~/Gits/.tmp/results/tuic-settings-mcp-1789-*.png. The light inspector DOM check showed its input inside the viewport with no document overflow; image review and narrow-screen checks remain pending.
- [ ] Coordinator batch: source catalogue parity, allSettingsTab renderer/options/owner-write regressions, pluginSettingsTab, settings resource/background merge and keybindingActionPublication tests; Rust frontend_plugin_actions_reach_keybinding_owner_choices and catalogue generation consistency. No per-story GREEN run.
- [ ] #1788-2163 critic fixes: after Boss rebuilds/restarts Rust, interrupt only a remote daemon event stream while health stays connected, then change settings and reconnect. Verify that owner refreshes after the generation resets, without reloading local or other remote settings. Keep a launch-instruction draft and issue two rapid repository-default edits during a settings refresh; verify the draft survives and both nonconflicting edits persist. No desktop restart was performed by the agent.

## Settings generated IPC parity (#1787-4778)

- [ ] After a Boss-controlled backend restart/rebuild, use an isolated instance to patch notification volume through MCP, read it through HTTP and desktop `settings_action({request: ...})`, then edit through desktop IPC and read through MCP/HTTP. The opt-in native mock-runtime harness covers generated-handler argument extraction and durable readback without launching a desktop app; it does not verify the installed desktop handler/ACL state. Do not restart Boss's live sessions or launch a second desktop instance for this check.

## Settings startup contract (#1790-1011)

- [x] Coordinator batch: exercise the published GitHub/Ego settings facade with isolated owners, covering refusal/confirmation expiry, device-flow refusal/expiry, stale revisions and persisted apply/readback failure; never use production credentials. _(verified: targeted Linux runs on f5c637467d passed startup_flow_tests, startup_uses_only_published_schema and provider_authorization_deadline_cannot_authenticate_or_persist in both configurations; the headless run also passed persisted_apply_failure_publishes_unapplied_generation)_
- [ ] After Boss restarts the Rust backend, discover GitHub list_accounts and Ego begin_login/login_status through Settings. Complete a human Ego login in the returned terminal; verify closed/expired is never success and provider refresh shows owner credential state. Use only an isolated test account/instance. Automated isolated recovery tests passed; live authentication remains unverified. See docs/backend/settings-startup.md for prerequisites and the redacted evidence checklist.
- [ ] After the next backend restart, check Claude trust, external-import and Bash permission dialogs in mobile CLI: all options and the footer must remain visible (#1987-606c). Rust changes require a manual restart to load.

- [ ] After a manual Rust rebuild/restart, answer an ego selector and submit an ordinary prompt: no orange awaiting dot or question sound from the answered selector; a real new question still alerts (#1997-eb30).
- [ ] After Boss restarts the Rust backend, verify CircleCI Settings discovery (`circleci.token`) and presence-only readback through the shared Settings action. Isolated tests cover set/clear and failures; no production credential test is required (#1991-bd88).

- [ ] After Boss restarts `make dev` or installs a new release, ask a bound agent to schedule a recheck in one hour using name, prompt, in_secs=3600 and the actual saved run_config. Confirm one create returns caller repository, explicit profile, local fire time and next_run_ms; an identical retry must return the same id and occurrence. Omitting run_config must report caller_profile_unresolved. Rust changes do not hot-reload (#2001-5c1f).
- [ ] After the next manual `make dev` restart or release rebuild, open Settings → Agents. Check read-only global and per-agent launch instruction previews, Customize, Use global, empty overrides, and marker-toggle refresh. The Rust preview response is not loaded by hot reload.
- [ ] After Boss restarts the Rust backend, exercise `agent action=launch` in an isolated throwaway checkout: repeat a key, confirm the same child/name/parent/hold policy, and read its durable `launch_id` receipt. Rust does not hot-reload. Automated targeted transaction tests cover partial-stage retries; this item checks the installed build.
- [ ] After Boss rebuilds/restarts TUIC (#2005-191f), read a large inbox and terminal with `byte_budget=4096`, concatenate chunk strings and parse JSON; verify final-page cursor advancement, scoped compact inventory and explicit expired-token errors. Rust does not hot-reload; no desktop verification instance was started.
- [ ] After Boss manually rebuilds/restarts, explicitly enable perfDebug and frameWorkload; capture a real long paint/stall, confirm the saved path via eval_js logs, and replay in isolated native WebKit with heartbeat/input and independent backend timings. Synthetic smoke is not the real-stall RED (#1638-c94c). Rust changes do not hot-reload; do not start a second desktop instance.

- [ ] #1993: After Boss restarts the Rust build, set the global PR visibility filters and repository Show/Hide/Inherit overrides. Restart and verify they persist, and Inherit tracks the current global value.
- [ ] After a Boss-controlled rebuild/restart, use an isolated instance to create, edit and remove a `corrections` Settings record; verify Voice and the runtime dictionary adopt it and the new-entry draft remains intact (#1992-d4fa). Seed a legacy `constructor` correction through the owner and verify ordinary sibling edits preserve it without `unknown_path`. No real microphone is needed; the running Rust process does not hot-reload.

- [ ] After a Boss-controlled Rust rebuild/restart, verify isolated legacy upstream credential edits, OAuth completion, connection password set/clear, deployment pairing-token creation and deployment-mode changes refresh another open Settings reader; vault rejection must not emit a successful persistence hint (#1989-22fe).
## MCP inline schemas (2013-73f9) — Rust restart required

- [ ] After Boss loads the rebuilt backend, measure a real ego session before/after: search top_schema admission must remove one discovery hop before its first tool call. Confirm a tools/list_changed notification invalidates admission. Requires the ego client counterpart; no desktop restart was performed by this peer.

## MCP argument contracts (2007-59f6) — Rust restart required

- [ ] After the backend rebuild and ego counterpart are available, validate the action-input-contract-2007 fixture in ego locally; static bad arguments must produce no server call. This peer did not restart the desktop backend.

## MCP runtime outcomes (2010-30a7) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, capture real ego/client correlation for permission denial, application refusal, rendering success and lost reply in an isolated instance. Inspect `/mcp/audit`: bodies must be absent; a lost reply must not imply no effect. Supply UUID call/parent metadata to distinguish recipient batches. Server-only tests do not verify Codex custom calls that never reach MCP. No desktop restart was performed by this peer.

## Managed stateless reconnect (2008-5317) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, reconnect an isolated real ego managed session using its live host-issued identity with no legacy MCP session header. Verify retained mail and authoritative project recover; missing host context must stop unchanged repair loops. Verify external initialize/register and explicit replacement migration/stranded receipts. No desktop instance was restarted; server contract tests do not prove the ego retry policy.

- [ ] After the normal backend rebuild/restart, verify description-only MCP input updates/clears the context bar without sending terminal input (#2007).

- [ ] After the normal backend rebuild/restart, verify an ACP wake skipped while ego is busy can wake once after it becomes idle and later mail arrives (#2004).
- [ ] After Boss rebuilds/restarts the desktop or deploys the new `tuic-remote`, verify authenticated remote Settings edits refresh open readers; headless upstream OAuth remains unavailable (#2014-9d06). Rust changes do not hot-reload.
- [ ] After a Boss-controlled rebuild/restart, save and delete a remote connection and tunnel profile from desktop Settings and confirm another connected client refreshes without reopening Settings (#1788-2163 review repair). Rust changes do not hot-reload.
- [ ] After Boss manually restarts `make dev` or rebuilds release, verify a disposable repository with local main/master and a different remote HEAD uses main first, then master when main is absent. Keep the no-remote non-standard-base check above (#1961-bab6). Rust does not hot-reload; do not restart live sessions for this check.

- [ ] After the next manual Rust restart/rebuild, verify a disposable repo with a stale `main` and an explicit `branch.<main-checkout-branch>.tuicommander-base=develop` classifies branches integrated into `develop` as Merged. Explicit verified bases take priority; unconfigured repos still prefer main/master before remote HEAD (#1961-bab6).

- [ ] After the next backend restart, check a Claude `intent:` line that wraps on a phone: its first row and continuation must both stay visible with the warning tint. The Rust log serializer change requires a manual `make dev` restart or release rebuild. Do not restart live sessions for this check (#2019-7c5d).
- [ ] On a real iPhone, confirm the session menu Markdown list stays still during session updates; on an iPad, open a listed file from the top-bar Markdown dropdown and return to the session (#2024-9a86, #2025-82cb). Browser evidence covers 390 px and 1024 px, not physical touch behavior.

- [ ] After the next planned desktop rebuild, confirm desktop startup and browser/mobile static pages still load. Story 2041-04c4 moves Tauri context construction into the binary; no restart was performed by the agent.
- [x] Mobile Markdown review send (#2042-a71b): 360×800 agent-browser fixture check ticks a checkbox, saves a block comment, sends one path-and-count prompt, and displays transient queued feedback with amber commented-block highlighting; screenshots are in the orchestrator report. The fixture uses real components with an isolated transport, not a live backend round trip.

- [ ] Mobile CLI on a real phone: answer paragraphs show a continuous green gutter instead of protocol speech bubbles; verify native text selection and touch scrolling. Browser evidence at 390×844 is retained under `~/Gits/.tmp/2070/` (#2070-773d).

## Plugin review integration (#1756-4d42)

- [ ] After the Rust restart and plugin catalogue publication, install a plugin, edit its installed `main.js` without changing its version, rediscover it and refresh Browse: both Installed and Browse offer Update. Restore from the catalogue and confirm the badge clears; editing only `data/config.json` must not offer an update.
- [ ] Reload GitHub/GitLab/Jira twice: one registration each; rejected async startup reports its error. Set and use Jira/GitLab default projects through plugin Settings (account-specific projects take precedence); capture the settings page. Resolve a Jira URL and cancel an in-flight detail request.
- [ ] Remaining original live checks: switch repos during scans, change composer during retry/wakeup reads, open same-name CSV/DOCX/XLSX files in different repos, refuse an incompatible real plugin, and route an account-authorized GitHub review/action through the core ports. Module tests do not satisfy these live E2Es.
- [ ] #2072-adf9: After a manual `make dev` restart or release rebuild, reconnect Claude Code and confirm ToolSearch finds agent, session, repo, and ui; send one agent message. Rust schemas do not hot-reload.

## Mobile Codex Chat (#2076-8190)

- [ ] On a real phone, open a Codex session with an exact native-hook transcript binding. Switch CLI → Chat → CLI and confirm the existing header, composer and question controls remain touch-usable. Browser verification does not prove phone touch behavior.

## Mobile Grok Chat (#2077-b132)

- [ ] On a real phone, open a registry-bound Grok session and switch CLI → Chat → CLI. Confirm the existing question controls and composer remain touch-usable. Worktree browser evidence does not prove phone touch behavior.

- [ ] After Boss restarts `make dev` or installs a rebuilt release, verify MCP capability switches, migrated denial diagnostics and client reconnect to the alias-free catalogue (#2068-63b9). Rust changes do not hot-reload.
## ZIP 9 dependency migration (2037-6c5a) — backend rebuild required

- [ ] After Boss rebuilds/restarts the app, install a disposable plugin ZIP with a root manifest and another with a single top-level directory; confirm their files load. On Windows, install the speech runtime from its ZIP and confirm the selected library loads. Automated extraction/name tests run on rb; this item records that the currently running backend cannot load the ZIP 9 migration without a rebuild. Do not restart the running app for this check.


## 1638-c94c — generated workload WKWebView replay

- [ ] After story 2086's isolated `make agent-test` harness lands, run the single-command `scripts/stall-workload/desktop-replay.mjs` procedure from its README against the generated recording in `~/Gits/.tmp/stall-1638/font-workload/frame-workload.json`. Verify named-instance/executable guards, collect native heartbeat-command round trips and independent backend timings, and retain the result. This is paint-only replay of generated output, not the original captured-real-stall RED. No Boss-app restart or second ordinary desktop instance is needed or authorized.
- [x] Mobile AI Chat tab availability (#2087-07bf): isolated worktree Vite + headless backend at 390×844 shows four tabs without ego and five with ego; host config saves update the tab over SSE without page reload. _(verified: ~/Gits/.tmp/feat-2087-visual/transcript.txt and with-ego.png / without-ego.png)_

- [ ] After Boss rebuilds/restarts the Rust backend, ask ego for a native selector immediately after a TUIC connection/intent or successful tool receipt. Desktop and mobile must show only the native question title, with the same choices (#2088-98ea). The running app does not hot-reload this parser change; do not restart it during agent work.

- [ ] After the next normal Rust rebuild, check a narrow Claude AskUserQuestion: previous/next/select controls replace generic Yes/No, and widening restores all native choices (#2093-0be6).

- [ ] #2107-bbda: After the next Rust rebuild/restart, right-click shell and agent rows on main and linked branches; confirm exact session/PID clipboard values and copy toasts, and disabled PID for a suspended tab. The new child_pid session metadata does not hot-reload.

- [ ] #2108-465b: After rebuilding/restarting the Rust backend, reconnect Claude Code and Codex and confirm served instructions distinguish the TUIC agent tool from native Agent. Client qualification is recorded in tool-names-2108.json.

- [ ] Mobile CLI answer gutter (#2110-734c): on a phone, verify the green bar stays left of the first ego glyph, Claude keeps its existing margin, and blank paragraphs, wrapped links and tables keep their bars.

- [ ] After rebuilding, verify `agent send urgency=interrupt` with an isolated Claude or ego child: the running tool stops and the full message starts a new turn; normal/urgent remain non-interrupting (#2113-c419). Rust changes require a manual `make dev` restart or release rebuild to reach the daily app.
- [ ] After a manual Rust restart/rebuild, check abandoned-shell markers on desktop/mobile after one idle hour; optional close hours default to zero. Running foreground/background work must remain open. Check remote MCP close against the owning daemon (#2111-131f).

## Library cleanup (#2115-1333)

- [ ] After the next manual backend restart, confirm shell activity, inline-TUI classification and plugin output watchers still work. The write-only command knowledge recorder no longer writes history files. Existing files remain untouched. Do not restart the daily app for this check.

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
  Prompt" path.) _(NOTE (2026-10-17 replay): the append path was NOT carried onto
  main — main's own fix (`textRequest`, #1599-db8a) REPLACES the open draft with the
  prompt text; verify replace, or decide whether to restore append.)_
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

## SSH daemon provisioning, confirmed (wip 2ba33a3f5 replay) — Rust restart required

- [ ] After Boss's next planned backend restart and a matching `tuic-remote` release for this version, against a disposable SSH host in an isolated `TUIC_APP_INSTANCE`: a Never-deploy "Remote Server — SSH" with "Offer to start the remote daemon if it is not running" and no daemon running shows **Start remote daemon…**; the dialog lists the exact commands; Cancel runs nothing; Accept starts it (with Instance ID: `tuic-remote --instance <id>`, `tuic-remote-<id>.pid`) and connects; Disconnect stops it (unless "Leave it running" is checked). Against a daemon with no password, **Set remote password…** sets the saved one, a second attempt reports "already has credentials; nothing was changed", and the daemon needs a restart to use it. No desktop instance or SSH host was used by this lane; Rust does not hot-reload.

## Nested SSH model migration (wip afc10a2c9 replay) — Rust restart required

- [ ] On Boss's next planned backend restart, check `<config_dir>/connections.json` and `<config_dir>/tunnels/*.toml`: each SSH entry now has a nested `ssh` block, and a `*.pre-nested-ssh-<timestamp>.bak` beside each rewritten file holds the original bytes. Then Connect an existing SSH remote machine and start an existing tunnel — both must behave exactly as before (same host, port, identity, compression). A second restart must create no new `.bak`. Rust does not hot-reload; this lane did not restart the live app.

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
  reports a different id there, and that `terminalsStore.exactAgentSessionId` (via
  browser devtools / a temporary log) holds each tab's own id once its own hook has fired.

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
