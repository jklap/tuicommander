# TUICommander — Terminal Component (`src/components/Terminal/`)

Repo-wide and general-frontend rules live in the root `AGENTS.md` and
`src/AGENTS.md` — read those first. This file covers what's specific to the
canvas-based terminal renderer, its selection/input handling, and its
consumption of the PTY/agent-signal backend documented in `src-tauri/AGENTS.md`.

## Smart Selection Drag Anchor

A double/quad-click "smart" match (a path, URL, etc. — see Smart Selection in
`docs/frontend/terminal-features.md`) must never reuse `"word"` mode's drag
anchor (`wordAnchor`/`extendSelectionDrag`'s `"word"` branch,
`canvasTerminalSelection.ts`). That branch re-derives its live drag edge from
the *plain* word-boundary resolver (`wordBoundsAt`/`getWordBoundaryResolver`)
at the current mouse position — which is narrower than a multi-segment smart
match at every point except its exact extent, since punctuation like `/`,
`.`, `:` splits it. Any `mousemove` before `mouseup` — including ordinary
pointer jitter during the double-click itself, well within the double-click
window, with zero real displacement — recomputed that narrower boundary and
collapsed the whole match down to just the sub-word under the cursor, before
the user ever got to drag anywhere. This shipped and went unnoticed because
the only existing test for a URL double-click (`canvasTerminalGestures.pin
.test.ts`) fired `mousedown`×2 → `mouseup` with no intervening `mousemove` —
exactly the one sequence real double-clicking never manages in practice.

Fixed by giving smart matches their own `"smart"` `SelectionMode` and
`smartAnchor` (full `{start, end}` coordinate pair) in `DragAnchor`, with a
dedicated `extendSelectionDrag` branch: while the live drag position stays
within `[start, end]` (inclusive both ends — a drag landing exactly on either
edge must not shrink either), the full match is kept untouched; only once the
drag genuinely moves past an edge does it extend outward, by whole word at
the new position (not the raw point) — this last part matters because the
built-in smart-selection rule set includes a low-precision catch-all `\S+`
rule (`smartSelectionDefaults.ts`), so *every* plain double-clicked word is
also technically a "smart match" under the hood, and dragging one across
several words must keep pulling in whole words the way plain `"word"`-mode
dragging always did — an early version of this fix used the raw drag point
for that extension and broke that exact case (`canvasTerminalGestures.pin
.test.ts`'s pre-existing "dragging after a double-click" tests caught it).

If you add a fourth drag-extension mode, check whether its live boundary can
ever be narrower than what was selected at mousedown — that mismatch, not
row-span (a single-row smart match hit this identically to a multi-row one),
is the actual failure condition.


## Smart Selection Built-in Rules: The Content Hash Is A Persisted Contract

Saving any smart-selection rule writes the whole shipped set into `config.json`, so a rule a
release changes never reaches a user who customized anything. `smartSelectionReconcile.ts`
fixes that at startup: each saved copy of a built-in carries `built_in_hash` (the hash of the
shipped definition it was last reconciled against); an **unedited** copy (hash of its current
content equals its baseline) is replaced with the new shipped version, an **edited** one is left
alone and the new version is added after it as a disabled "<name> (updated)" rule, and one toast
links to Settings → Smart Selection. See `docs/frontend/terminal-features.md` "Built-in rule updates".

The thing that is easy to break: **`ruleContentKey`'s field list and `ruleContentHash`'s algorithm
are written into every user's config.** (The per-action options, `sendReturn` and `runIn`, were
added without moving anyone's baseline exactly the way this paragraph prescribes: they are appended
to an action's key only when SET, never as a placeholder, and a default is never stored, so a rule
that does not use them hashes as it always did; `smartSelectionReconcile.test.ts` pins both.) Add a
field to the key, reorder it, or change the hash,
and every saved rule's current hash stops matching its stored baseline, so EVERY built-in looks
user-edited and the next release adds a spurious "(updated)" copy of each one. A test pins the hash
of a fixed rule (`smartSelectionReconcile.test.ts`) — if that test fails, you are changing the
contract, not fixing a test. If a rule genuinely gains a new editable field, give old copies a way
to hash as before (e.g. hash without the field when it is at its default) rather than moving the
baseline of everyone's config. `enabled` is deliberately outside the key: switching a rule on or
off is a preference, not an edit.

Two more invariants: `built_in_hash` is never exported (`buildRulesExportFile`) and never read on
import (`parseRulesExportFile` builds rules from explicit fields), so an imported rule cannot claim
a baseline; and the reconcile runs only in the main window (`useAppBootstrap`'s `hydrate`), because
panel windows and the mobile app also hydrate settings and would race it and toast twice.

## Smart Selection "Run Command" Executes Untrusted Text: Matched Text Is Data, Never Command Text

Two kinds look alike and are not. `send_text` types into the terminal you clicked (its **Press
Enter** option is `never` or `when_safe`, never "always"); `run_background` (labelled **Run
Command**) runs a command either in the background (`background_command.rs`) or, per its **Run**
option, in a new terminal. The id `run_background` predates the second mode. The older `run_command`
kind ("Send Command") was folded into `send_text` + `when_safe`: it stays valid on disk and is read
through `canonicalAction` everywhere its meaning matters (dispatcher, Settings editor, hash key,
export comparison) — never re-spelled on load, only when the user edits that rule, so an untouched
config keeps working in a build that does not know the option. The older `run_command_new_terminal`
kind still works, unquoted, exactly as before.

The matched text comes from terminal output, so it is attacker-influenced (a file name in `ls`
output, a line in a fetched page). For typed text that is bounded by a human reading it and by the
`isInertShellText` gate; for Run Command nothing reviews it, and a new terminal runs its command the
moment its shell is ready. **The rule is that matched text is never put into the command line.**
`buildRunCommandScript` rewrites each placeholder into a `"${TUIC_Mn}"`-style reference (choosing
the form that matches the quoting around it) and returns the values separately; they travel as
environment variables, which a shell expands as data and never parses again. That makes injection
independent of how the template author quoted the placeholder — an earlier design quoted values
inline with POSIX single quotes, and a security review showed that is unsound twice over: it is
broken by a user who writes `imark "\0"` (the value then sits inside THEIR double quotes), and it is
broken by fish, whose single quotes treat `\'` as an escape. Do not go back to inserting values into
the line, do not add a second caller of `run_background_command` that builds the line itself, and a
new placeholder code must be added to `PLACEHOLDER_VARIABLES` *and* to the backend's
`is_allowed_env_name` or it is refused. The backend only accepts those fixed names (so `env` cannot
set `PATH`/`LD_PRELOAD`), and always runs the line with `/bin/sh` — the user's login shell is used
only to build the environment, through a constant wrapper, and only for zsh/bash/fish/ksh.

**One mode still relies on quoting: "In a new terminal" TYPES a line into the user's own shell.** It
types `TUIC_M0='…' /bin/sh -c '…'` (`typedRunLine`), POSIX single-quoted, which is sound only in
sh/dash/bash/zsh/ksh. So the dispatcher asks the backend first (`run_command_shell_support`) and
refuses — opening and typing nothing — for any other shell, and for any missing or malformed answer.
It also refuses a value holding a control byte (`hasControlBytes`: the line editor reads `^C`/`^U`
before the shell parses quotes, and `\d` is percent-decoded raw from OSC 7) and a line over
`MAX_TYPED_RUN_LINE_BYTES` (1000; a shell short of its line editor truncates at `MAX_CANON`, and this
path is not covered by the HTTP-only long-write guard).
Do not widen the supported list without testing the new shell with hostile values (the way
`smartSelection.test.ts` does for the supported ones, and the way fish was found to break). A
placeholder inside `$(( ))`, `[[ … -eq … ]]`, `let`, `declare -i`, `eval` or `sh -c` re-reads a value as code on purpose; that is
documented, not prevented. A rule's Run Command action imports disabled (`RISKY_ACTION_KINDS`).

**A background command must have nothing connected to TUIC, and that means `/dev/null`, not a pipe
and not a file.** Measured: give the child a pipe for stdout/stderr, let the parent exit, and the
child's next write kills it with `SIGPIPE`; give it a file and it survives. So a pipe makes a
long-lived tool die on quit if it logs, and a file grows without bound for a chatty one — only
`/dev/null` for all three streams makes "quitting TUIC does not affect it" true, which is why there
is no quit warning. The cost is accepted: a failure reports only the exit status (127 reads "not
found"), and the output is read by running in a new terminal.
`nothing_is_connected_to_tuic_so_quitting_cannot_kill_it_with_sigpipe` pins it; do not add output
capture back without solving that.

**Three things a security review of this feature proved, each reproduced against real shells.** (1) The
"press Enter only when safe" gate (`shouldAutoSubmitSuggestion` in `utils/sendCommand.ts`) is an
ALLOWlist (`isInertShellText`), never a denylist: the original denylist of ``; & | ` $ < >`` approved
text that zsh (`/(e:'cmd':)`) and fish (`x(cmd)`) execute. Do not "relax" it by adding characters;
each shell has its own syntax and the safe rule is to name what is inert. It is shared with the OSC
7770 suggestion chips. (2) POSIX single-quoting is not a universal quoting: it is sound in sh/dash/bash/zsh/ksh (tested in 12
zsh option sets), **not** in fish (`\'` inside single quotes is an escape, so a value like `\' ; cmd #`
breaks out), and unverified in tcsh/csh/nu/elvish/xonsh/pwsh — which is why Run Command passes values
as environment variables instead and restricts its one typed mode to the shells above. (3) A plain click must never type into a running agent
(`pickLinkClickRule`'s `agentRunning`): with an agent in the foreground Enter is always pressed.

## Canvas-Terminal Frame Protocol Bit Layout

The bit layout is defined **once**, in `crates/tuic-terminal`'s `terminal_grid.rs`
`serialize_dirty_rows` doc comment (backend). Any bit added there MUST be mirrored in
`canvasTerminalUtils.ts::decodeBinaryFrame` (this directory) AND in
`docs/frontend/canvas-terminal-audit.md`'s bit-layout paragraph, not just the two code
sites — three separate bugs (the DECCKM `app_cursor` bit, the DECSCUSR `cursor_steady`
bit, and a stale scrollbar-visibility condition next to them) have traced back to that
three-way split staying out of sync. Full backend-side rationale:
`src-tauri/AGENTS.md`.

## Command Blocks — Frontend Consumer Rules

Full mechanism (why a block's `line` can be a meaningless alt-screen cursor row, or
alias past scrollback-ring eviction): `src-tauri/AGENTS.md`'s "Command Blocks" section.
Frontend-side rules that follow from it:

- **Every row-anchored block consumer must read blocks through `rowAnchoredBlocks()`**
  (`src/stores/terminals.ts`), which drops `onAltScreen: true` entries, rather than
  rendering a mark at a meaningless row.
- **`CommandOverview.tsx`'s `getCommandText` falls back to slicing the grid**
  (`ref.getBufferLines(commandLine, executionLine)`) whenever `promptText` is null — a
  real shell OSC133 block with no hook-driven prompt text. That fallback DOES depend on
  row validity, so it must check `block.onAltScreen` too and return `""` rather than
  read a row that no longer means what it once did.
- A `CommandBlock`'s stored `line` is eviction-stable (counted from the grid's
  `screen_origin`), not a live viewport/buffer-line row — subtract the terminal's
  `historyBase` (`TerminalData.historyBase`, the latest frame's evicted-row count) before
  using it to index into the live grid; a negative result has been evicted. See the
  `CommandBlock` doc comment in `terminals.ts`.

## A Visible Terminal Must Notice A Dead Grid Channel (Frame-Starvation Watchdog)

Found 2026-10-06 from a live report: the `main 1` tab showed only its cursor bar
and the command-block gutter marks, no text, and stayed that way through tab
switches and window resizes. The backend was fine (full scrollback via
`/sessions/{id}/output`). The component was alive too — its cursor blinked and a
resize re-measured and re-sized its canvases — but it received zero grid frames
and sent zero `ack_terminal_frame`. Rust still held a grid `Channel` whose JS
callback was gone from the webview (Tauri logged `Couldn't find callback id N`,
same id, every ~540 ms while idle), so the ticker sent into the void, the gate
timed out after 500 ms and was abandoned, and the cycle repeated. A webview reload
(sessions live in Rust and survive it) healed it at once.

**Rust cannot detect this** — `Channel::send` does not fail when the callback is
missing — so the receiver has to. `createFrameStarvationWatchdog`
(`canvasTerminalUtils.ts`) is armed by `CanvasTerminal` whenever a *visible*
terminal requests a frame (the show path and the initial subscribe) and
resubscribes via the shared `resubscribeGrid()` if `framesReceived` has not
advanced within `FRAME_STARVATION_MS` (2 s). Bounded to
`FRAME_STARVATION_MAX_ATTEMPTS` (3), then one logged error and no more timers;
cancelled on hide and unmount.

Rules that follow, each one a way an earlier draft of this idea goes wrong:

- **Never arm it for a hidden terminal.** A background tab acks late on purpose and
  legitimately receives nothing for long stretches.
- **Re-read the counter AFTER `onStarved`.** Resubscribing resets `framesReceived`
  to zero synchronously; a baseline captured before the call is a number the fresh
  subscription may never reach, which reads a working channel as starved.
  `frameStarvationWatchdog.test.ts` pins this.
- **The timeout must clear the backend's own recovery** (500 ms abandon + the 1 s
  `STUCK_PAUSE_MS`), or a busy-but-healthy WebView gets resubscribed. A false
  positive is harmless (resubscribe is idempotent and installs a fresh gate), but
  it is churn.
- **Do not add a second `resubscribe` implementation.** `onRef.resubscribe` (the
  reattach path) and the watchdog share `resubscribeGrid()`.

**The root cause of the callback being lost is NOT known** — this is a self-heal,
not a cause fix. If the `Couldn't find callback id` warning is ever seen again
with the watchdog in place, look at what unregistered the Channel's callback
(`@tauri-apps/api` `Channel`, `transformCallback`) rather than at the gate.

Diagnosing a blank tab: `grid frame gate stuck` alone is routine noise for hidden
tabs (`frontend_liveness.rs`); it only means something for a visible one. The
discriminating signals are (a) the idle `callback id` warning loop and (b) no
`ack_terminal_frame` from the visible session — wrap
`window.__TAURI_INTERNALS__.invoke` from `debug invoke_js` to watch for it. Run an
**idle** control (no requests) before dismissing a recurring warning as a probe
artifact — an earlier pass here did exactly that and was wrong.

## Terminal Keydown vs. Global Shortcuts

`keyToSequence()` (`terminalInput.ts`) excludes `metaKey` but not `ctrlKey` from its
printable-character PTY-forwarding fallback — so a global shortcut whose Windows/Linux
form uses Ctrl+&lt;printable&gt; (macOS form: Cmd+&lt;printable&gt;) will be silently
swallowed and typed into the terminal instead of bubbling to the document-level
shortcut listener, unless the terminal's keydown handler explicitly bails out first
(see `isGlobalShortcutPassthrough`, same file). When adding or rebinding a global
shortcut that uses Ctrl/Cmd + a printable key, verify the Windows/Linux (Ctrl) form is
special-cased the same way.

## Clipboard Writes That Need An Async Round-Trip First (browser mode's user-activation window)

Any browser-mode clipboard write that needs to resolve its text via an HTTP round-trip
first (`terminal_get_selection_text`, `getBufferLines`) is at risk of the browser's
user-activation window expiring before the write actually happens — `document
.execCommand('copy')` and `navigator.clipboard.writeText` both need to run within that
window, and a slow/remote connection (Tailscale, not localhost, where the round-trip is
near-instant) can make an awaited fetch outlast it, silently no-oping the copy with no
error surfaced anywhere. Tauri desktop mode never hits this: its native
`clipboard-manager` plugin has no gesture requirement (see `utils/clipboard.ts`'s
`writeClipboard` doc comment).

Found 2026-09-18 in both `copySelection()` (`CanvasTerminal.tsx`) and "Copy Block Output"
(`useTerminalContextMenus.ts`) — both awaited their round-trip before calling into the
Clipboard API. **The fix is not "resolve the text locally and skip the round-trip"** — an
early version of this fix did exactly that and was caught mid-review: the Rust-side path
doesn't just fix line-wrapping, it also strips Claude's `▎` quote-gutter markers
(`docs/backend/pty.md`'s `get_selection_text` section) so a multi-line Claude message
pastes clean — a documented, marketed feature (`docs/user-guide/terminals.md`'s "Copy &
Paste" section). Skipping the round-trip in browser mode would have silently degraded
every multi-line browser-client copy, not just the rare slow-network case it was meant
to fix.

The actual fix, `writeClipboardAsync()` (`utils/clipboard.ts`): call
`navigator.clipboard.write()` **synchronously** (satisfying the activation requirement
immediately, still inside the same task as the triggering gesture) with a
`ClipboardItem` whose data is the still-*pending* round-trip promise — per spec, a
`ClipboardItem`'s data may be a `Promise` that resolves later, which is the standard
pattern for exactly this "the real data isn't ready yet" situation and is supported by
Chrome, Firefox, and Safari. This gets both the fixed timing *and* the full Rust-side
text quality, with no tradeoff, on any browser that supports `ClipboardItem`+`write`.
Only falls back to the old (activation-risking) synchronous path when that API isn't
available at all. Two consequences of committing to the write before the text is known:
a selection that expired out of scrollback rejects the text promise, so the write fails
and nothing is copied (`commitSelectionCopy` still reports "expired"); and an empty result
writes an empty string on the `ClipboardItem` path — the awaited paths (Tauri, the
fallback) skip an empty write as before.

If you add a third caller with this shape (resolve text via IPC/HTTP, then write to the
clipboard), route it through `writeClipboardAsync(textPromise)` rather than
`await writeClipboard(await textPromise)` — the latter reintroduces this exact bug in
browser mode. And if you're tempted to "fix" a slow-clipboard-write bug by resolving
text locally/synchronously instead, check whether the async path you're skipping does
more than just fetch data (wrap-unwrapping, gutter-stripping, or similar cleanup) before
assuming that's a safe simplification.

**A related, separate bug found during this same investigation, fixed 2026-09-18:** the
"Copy on Select" setting (`settingsStore.state.copyOnSelect`) was fully unwired — this
`CanvasTerminal.tsx`'s `onMouseUp` called `copySelection()` unconditionally on any non-empty selection,
with no `copyOnSelect` check anywhere, so disabling the toggle in Settings did nothing.
Fixed by gating only the auto-copy-on-drag branch in `onMouseUp` on the setting — the
selection is still always made, and the Cmd/Ctrl+C keydown path (a separate call site,
same function) still always copies manually regardless of the setting. If you add a
third call site of `copySelection()`, decide deliberately whether it's a "passive
auto-copy" trigger (gate it on `copyOnSelect`, like `onMouseUp`) or an "explicit copy
action" trigger (never gate it, like Cmd+C) — don't assume one shape covers both.

## Opt-in frame workload evidence

`window.__TUIC__.frameWorkload.enable(true)` requires perfDebug and retains sensitive terminal text in bounded memory; default off. `debug action=frame_workload_dump` saves rotated instance-local captures after frontend JS responds. See `docs/backend/stall-journal.md` for limits, privacy and replay. The synthetic smoke fixture does not satisfy the real-stall RED.
