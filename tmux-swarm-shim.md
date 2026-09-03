# Claude Code tmux/iTerm2 teammate-pane shim — investigation + plan

Status: §5.1–5.6 implemented (see the "Implementation status" note right after this paragraph).
§5.0 (confirming the real pane-backed-teammate trigger) remains deliberately deferred — the
invocation logging added in §5.6 is how a future session answers it, by reading
`GET /logs?source=tmux-shim` / `<config dir>/logs/tmux-shim.log` against a live Claude Code run.

> **Implementation status (this session):** §1.5/§4's tmux-subcommand disposition table and §2's
> test-coverage numbers are **stale as written below** — left in place as investigation history,
> corrected here rather than rewritten in place:
>
> - **§1.5 correction, verified against the installed v2.1.258 binary** (the doc below was written
>   against v2.1.252): every swarm-path call is prefixed with the global option
>   `-L claude-swarm-<pid>`; the session Claude Code creates is the **fixed name `claude-swarm`**,
>   not `claude-swarm-<id>`; panes are created running the placeholder `cat`, and the real command
>   arrives later via `respawn-pane -k -t %N -- <command>` — **`respawn-pane` is REAL** (the
>   command-delivery mechanism), not the cosmetic `set-option` variant §4 filed it as.
>   `send-keys` is not used by the swarm path at all.
> - **§2's "exactly 2 tests" claim is wrong even at the time it was written** — `main.rs` had 22
>   tests, not 2 (only `tmux_compat()`/`find_flag()`/`cmd_alias()`/`ipc.rs` were genuinely
>   untested, which is what actually mattered).
> - Implemented: `-V`, `new-session`, `new-window`, `split-window`, `respawn-pane`, `select-pane
>   -T` (now emits a `session-renamed` event so the tab title actually updates — it previously
>   didn't), `list-panes`, `list-windows`, `kill-pane`, `display-message -p`, explicit noops for
>   `select-layout`/`set-option`/`set-window-option`/`switch-client`/`rename-window`, and dual-sink
>   invocation logging (`<config dir>/logs/tmux-shim.log` + `POST /logs`, source `tmux-shim`).
>   Pane topology lives app-side (`src-tauri/src/mcp_http/tmux_routes.rs`), partitioned by the
>   `-L`/`-S` label, in-memory only (a swarm can't outlive its lead process). See
>   `docs/user-guide/cli.md`'s "tmux Compatibility" section for the user-facing surface and
>   `docs/api/http-api.md`'s "tmux Compatibility Shim Endpoints" for the HTTP contract.
> - Several bugs found during the coverage pass were fixed rather than just characterized:
>   `resolve_session_id`'s `len()>=32 && contains('-')` UUID heuristic (a live footgun — any long
>   hyphenated target passed through unvalidated), a missing `-t` silently prefix-matching every
>   session, `argv0 == "tmux"` never matching the `tmux.exe` copy `cmd_alias` itself creates on
>   Windows, and `resize-pane -Z` alone silently shrinking a pane to 80x24.
Related but independent work already staged (uncommitted) in this working
tree: `prefer_tuic_messaging`/`prefer_tuic_spawning` settings and the
`--dangerously-load-development-channels` regression fix in
`docs/user-guide/agent-teams.md` / `docs/FEATURES.md` / `docs/backend/mcp-http.md`
/ `to-test.md`. That work already corrected the doc's false
`TeamCreate`/`TaskCreate` claim — this plan builds on top of it and does not
re-litigate that part. Leave those changes in place.

## 1. What we verified, and how

All of this was verified empirically against the installed Claude Code
binary (`v2.1.252`, currently at `/opt/homebrew/Caskroom/claude-code@latest/2.1.252/claude`)
via `strings` + reading de-minified source fragments recovered from the
binary (it's a bundled Bun build; large spans of original source survive as
plain text between `// @bun @bytecode` chunk headers). Nothing below is
speculative — each claim has either a direct code quote or a live
reproduction. Findings are non-obvious and easy to re-break by future
assumption, so read this before touching anything in the "what to build"
section.

### 1.1 `TeamCreate`/`TaskCreate` don't exist as tools

Confirmed independently by two mechanisms: our own `ToolSearch("TeamCreate")` /
`ToolSearch("TaskCreate")` returned nothing (tested both from this
orchestrator session and from a freshly spawned Claude Code lead), and a
spawned lead explicitly self-reported "There's no dedicated 'TeamCreate'
tool" after searching for it. `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` is
genuinely injected (`pty.rs:112`, unconditional, confirmed via `echo` inside
a live PTY) but in this Claude Code version it only affects `SendMessage`
availability — team membership is implicit, not created via a dedicated
tool. This matches the already-corrected text in `docs/user-guide/agent-teams.md`
(see the uncommitted diff noted above) — no further doc work needed for
this specific claim.

### 1.2 The `Agent` tool (generic async subagent) is a *different, unrelated*
mechanism from the tmux/iTerm2 pane system

This is the finding that unblocked the whole investigation. Asking Claude
Code to "use the Agent tool to spawn a teammate" — which is what every one
of our early tests did — **never calls the pane-backend detection code at
all**, regardless of `teammateMode` or env vars. Proven by running with
`--debug-file <path>` and grepping the full log: zero occurrences of
`BackendRegistry`, `swarm`, or `pane`, across a run that did spawn and
complete a background `Agent` task successfully. The `Agent` tool runs
fully in-process ("Backgrounded agent" in the transcript) no matter what.

**Open question, not yet resolved:** we do not know what *does* trigger the
pane-backed teammate system now that `TeamCreate` doesn't exist as a tool.
It's presumably wired into whatever implicit team-membership mechanism
`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` enables, but we never found the
trigger — every test we ran that produced a visible pane came from a
separately-run agent whose invocation details we couldn't fully audit
(non-`$TMUX`, non-iTerm2 environment, so per section 1.3/1.4 it should not
have been possible through the code paths we found — this is flagged as an
open question, not a contradiction we resolved). **Before writing any shim
code, find and confirm the actual trigger** (see section 4, task 0) — otherwise
we risk building a shim for a code path nothing in practice calls.

### 1.3 Backend selection logic (`BackendRegistry.detect()`, function `Hit()`)

Recovered verbatim (de-minified) from the binary:

```js
async function Hit(e = Cz) {
  await gPt(e);                                 // registers TmuxBackend/ITermBackend classes
  if (e.cachedDetectionResult) return e.cachedDetectionResult;

  if (DHe() === "iterm2") {                      // DHe() = current teammateMode setting
    if (!_M(e)) throw Error('teammateMode is "iterm2" but not running inside iTerm2...');
    if (!await iOe(e)) throw Error('teammateMode is "iterm2" but the it2 CLI is not reachable...');
    // log: "Selected: iterm2 (explicit teammateMode)"
    return iterm2 backend;
  }

  let insideTmux = await I2();                    // = !!process.env.TMUX  (see 1.4)
  let inITerm2   = _M(e);                          // $TERM_PROGRAM==="iTerm.app" || $ITERM_SESSION_ID set

  if (insideTmux) {
    // log: "Selected: tmux (running inside tmux session)"
    return tmux backend;                           // <-- never calls `tmux -V`
  }

  if (inITerm2) {
    if (!preferTmuxOverIterm2 && await iOe(e))      // it2 CLI actually works (`it2 session list`)
      return iterm2 backend;                        // "Selected: iterm2 (native iTerm2 with it2 CLI)"
    if (await tmuxVersionCheck())                    // `tmux -V` exit code 0
      return tmux backend;                           // "Selected: tmux (fallback in iTerm2, ...)"
    throw Error("iTerm2 detected but no it2 CLI and no tmux");
  }

  // not in tmux, not in iTerm2
  if (await tmuxVersionCheck())                       // `tmux -V` exit code 0
    return tmux backend;                               // "Selected: tmux (external session mode)"
  throw Error("No pane backend available");
}
```

Non-interactive (`-p`) sessions are hard-forced to in-process regardless of
all of the above (`"[BackendRegistry] isInProcessEnabled: true (non-interactive
session)"`), and any thrown error from `Hit()` also falls back to in-process
(`"...isInProcessEnabled: true (fallback after pane backend unavailable)"`).

**Critically: there is no explicit `teammateMode === "tmux"` branch.**
`DHe()` (the `teammateMode` getter) is only special-cased for `"iterm2"`.
Setting `teammateMode: "tmux"` (or `--teammate-mode tmux`) does not force
tmux selection by itself in this function — it rides the same
auto-detection cascade as the unset case. (The CLI flag is real —
`--teammate-mode <mode>` is a genuine, just undocumented-in-`--help`,
option; confirmed by comparing its silent acceptance against a deliberately
bogus flag, which does error. `[TeammateModeSnapshot] Captured from CLI
override: tmux` also shows up in `--debug-file` output, confirming the value
is read — it just isn't consulted anywhere in `Hit()` except the `"iterm2"`
branch.) **Do not assume `--teammate-mode tmux`/`teammateMode: "tmux"` does
anything for backend selection until this is re-verified against whatever
mechanism turns out to be the real teammate-spawn trigger (see 1.2's open
question) — this snippet only covers what `Hit()` does, and we don't yet
know for certain that `Hit()` is even reached by the real trigger.**

### 1.4 `insideTmux` is env-var-based, not PATH-based

```js
var d = a.TMUX, T = a.TMUX_PANE;             // a = process.env
function Dgt() { return !!d }                 // insideTmux
function p6t() { return T || null }           // current pane id, from $TMUX_PANE
async function one() {                        // "is tmux available at all" probe
  return (await $e(B6, ["-V"])).code === 0;   // B6 = "tmux"; literally `tmux -V`
}
```

`$TMUX`/`$TMUX_PANE` gate the "already inside a real tmux client" branch
(1.3's first tmux branch). `tmux -V` gates every other tmux branch
("external session mode" and the iTerm2-fallback branch). These are two
independent gates with different consequences for what we'd need to build
(see section 3).

iTerm2 detection: `$TERM_PROGRAM === "iTerm.app"` or `$ITERM_SESSION_ID` set
(or `$TERM === "iTerm.app"`, a third variant seen in the same function).
`it2` CLI is verified with `command -v it2` via login shell, then confirmed
actually working with `it2 session list` (not just installed) — a
subprocess check, not an env check. Not this repo's problem to shim (we're
not simulating iTerm2), noted only for completeness.

### 1.5 tmux subcommands the pane backend actually issues

Recovered as string constants adjacent to `TmuxBackend` methods
(`createTeammatePaneInSwarmView`, `sendCommandToPane`, `setPaneBorderColor`,
`setPaneTitle`, `getCurrentPaneId`, `getCurrentWindowTarget`,
`getCurrentWindowPaneCount`, `createExternalSwarmSession`,
`createTeammatePaneWithLeader`, `rebalancePanesWithLeader`,
`rebalancePanesTiled`):

`has-session`, `new-session` (creates a detached session named
`claude-swarm-<id>`), `new-window` (window named `swarm-view`),
`split-window`, `select-layout` (`main-vertical` / `tiled`), `resize-pane`,
`select-pane` (`-T <title>`), `send-keys`, `set-option` (multiple:
`pane-border-style`, `pane-active-border-style`, `window-style`,
`remain-on-exit`, `respawn-pane`, `pane-border-format`,
`pane-border-status`), `display-message -p '#{pane_id}'` /
`'#{window_id}'`, `list-panes`, `list-windows`, `kill-pane`, plus the
capability probe `tmux -V`. There's also an env var,
`CLAUDE_CODE_TEAMMATE_COMMAND`, that appears to override the command
launched inside each teammate's pane — not yet investigated further, note
for whoever picks this up.

### 1.6 Our existing shim (`src-tauri/crates/tuic-cli/src/main.rs`, `tmux_compat()`)

Currently handles: `new-session`/`new` (bare `tmux` too), `list-sessions`/`ls`,
`kill-session`, `kill-server`, `send-keys`, `capture-pane`, `resize-pane`,
`attach-session`/`attach`/`a` (focuses the TUIC window via deep link),
`has-session`, `display-message` (stub — always returns `Command::Status`,
does not honor `-p`/`-F` format strings). Unknown subcommands fall through
to an error + exit 1. Installed via `tuic alias` (symlinks `tmux -> tuic`,
target `/usr/local/bin/tmux` on Unix, elevated via `osascript`/`sudo` if
needed; `argv[0]` sniffing in `main()` routes to `tmux_compat()`).

**Live-verified right now, this session:** `tmux -V` through the alias
fails — `"tmux (tuic compat): unknown command '-V'"`, exit 1 — and there is
no real tmux anywhere on this machine (`brew list tmux` → not installed, no
`/opt/homebrew/bin/tmux`, no `/usr/local/bin/tmux`; the only `tmux` on
`$PATH` is our alias, first in `$PATH`). This is a hard, reproducible fact,
not an inference from source reading.

## 2. Coverage gap: what exists in tests today (near-zero)

Checked directly, not assumed:

- `src-tauri/crates/tuic-cli/src/main.rs`: exactly 2 tests exist
  (`agent_send_separates_framed_payload_from_enter`,
  `agent_send_bracket_pastes_multiline_before_separate_enter`), both about
  bracketed-paste framing for `agent send`. **Zero tests for `cmd_alias()`,
  `tmux_compat()`, or `argv[0]` dispatch.**
- `src-tauri/crates/tuic-cli/src/ipc.rs`: zero tests.
- `src-tauri/src/tuic_cli.rs` (the *app-side* installer commands — CLI sidecar
  install/update/uninstall, a different file from the CLI crate itself):
  1 test (`sidecar_name_has_no_target_triple`). Nothing about aliasing.
- No TypeScript/frontend code references `swarm`, `TmuxBackend`, `pane_id`,
  `teammateMode`, or `TMUX_PANE` anywhere in `src/` — this feature has zero
  frontend awareness today (unsurprising, since none of it currently works
  end-to-end).
- Grep for existing callers/related code confirmed `swarm` elsewhere in this
  codebase (`state.rs`, `mcp_transport.rs`, `useAppInit.ts`) refers to
  TUICommander's *own*, unrelated MCP multi-agent-orchestration
  terminology (the `agent`/`session` MCP tools) — a naming coincidence with
  Claude Code's internal "swarm" terminology, not shared code. Do not
  conflate the two when searching or grepping during implementation.

**Gap to close before writing shim code** (per user instruction — review
coverage of code we're about to touch, and of related callers, before
starting):

- Add unit tests for `cmd_alias()` (both create and `--remove` paths,
  including the "exists but points elsewhere — refuse to remove" guard) —
  currently completely unexercised.
- Add unit tests for `tmux_compat()`'s existing arms (`new-session`,
  `list-sessions`, `kill-session`, `kill-server`, `send-keys`,
  `capture-pane`, `resize-pane`, `attach-session`, `has-session`,
  `display-message`) before modifying any of them, so regressions in
  currently-working behavior are caught. `find_flag()` (the `-t`/`-s`/`-x`/`-y`
  arg parser used throughout `tmux_compat()`) has no direct tests either —
  add them; every new subcommand will depend on it working correctly.
- `src-tauri/src/tuic_cli.rs` (app-side installer): review whether
  `install_path_writable`, `check_version_match`, and the elevation paths
  (`copy_with_elevation`/`remove_with_elevation`) have adequate coverage
  given the shim's `new-session`/`split-window` handlers will need to talk
  to the running TUICommander instance over the same IPC socket
  (`ipc.rs`) — confirm `ipc.rs`'s `post`/`get`/`delete` helpers (used
  throughout `tmux_compat()` today) have at least minimal coverage before
  layering more calls through them; today they have none.
- No caller anywhere (Rust or TS) currently depends on `tmux_compat()`
  output format, so there's no risk of breaking a hidden consumer by
  changing it — but confirm this stays true as new arms are added (e.g. if
  `display-message -p '#{pane_id}'` starts returning a real value, check
  nothing internal parses the old stub `Command::Status` output expecting
  its current shape).

## 3. Should we inject `$TMUX`/`$TMUX_PANE`, or fix `tmux -V`? — recommendation

The user's working assumption was that TUICommander needs to inject
`$TMUX`/`$TMUX_PANE` into every PTY to "fool" Claude Code into the
`insideTmux` branch. **Recommendation: don't, at least not first — fix
`tmux -V` and the missing subcommands instead, and only add env injection
later if the "external session mode" path turns out to be insufficient.**
Reasoning:

- The `insideTmux` (`$TMUX` set) branch is Claude Code's *"I'm already
  inside a live tmux client, use my current pane"* path. To honor that
  correctly, our shim would additionally need to fake **the lead's own
  current-pane identity** — `display-message -p '#{pane_id}'` /
  `'#{window_id}'` would need to return a consistent, pre-existing pane id
  representing the *lead's own TUIC tab* (not a newly created one), and
  subsequent `split-window` calls would need to create new panes *relative
  to* that fake existing pane. That's strictly more state to fake
  correctly than the alternative below.
- The `tmux -V`-gated "external session mode" branch (`insideTmux` false,
  not in iTerm2) is *"no live tmux session exists — create a brand new
  detached one for the team."* That maps cleanly onto what TUICommander
  already does well: `new-session` there is 1:1 with "create a new TUIC
  PTY session," with no need to pretend an existing pane already exists.
  This is a materially smaller, more natural shim surface, and it's also
  the path that governs the *first* teammate — every subsequent teammate
  in the same team goes through `split-window` regardless of which branch
  got selected initially, so fixing `-V` unlocks both the initial-session
  creation and all subsequent teammates in one fix.
- Fixing `-V` is also strictly required either way: even if we later decide
  to inject `$TMUX`, the very first thing `one()` does elsewhere (the
  iTerm2-fallback branch, and anything else that calls `tuic --version`-style
  capability probes) still needs a working answer.

So: implement `-V` and the "external session mode" subcommand set first (no
env injection). Revisit `$TMUX` injection only if we specifically want
Claude Code to treat an *existing* TUIC tab as a tmux pane it can split
against, which is a different, harder feature than "let a fresh teammate
team get its own new TUIC tabs."

## 4. What to build — per-subcommand disposition

Legend: **REAL** = must do the real, correct thing or Claude Code will
believe something exists that doesn't (silent hang / lost teammate output —
worse than today's honest in-process fallback). **NOOP** = safe to
accept-and-succeed with no effect, since it's purely cosmetic in tmux and
TUIC tabs aren't panes. **HAVE** = already implemented in `tmux_compat()`.
**MISSING** = not implemented; falls through to the unknown-command error
today.

| Subcommand | Status today | Disposition | Notes |
|---|---|---|---|
| `-V` | MISSING | **REAL** (trivial) | Gates the entire "external session mode" fallback (§1.3/§1.4). Return a fake but plausible version string (e.g. `tmux 3.4`), exit 0. Highest-priority single fix — unblocks everything else even before other arms exist, since `Hit()` will otherwise throw before ever trying `new-session`. |
| `has-session` | HAVE | REAL, keep | Already checks TUIC session list. |
| `new-session` | HAVE (generic) | **REAL, extend** | Needs `claude-swarm-<id>` naming awareness and `-d` (detached) semantics matching "external session mode" — confirm current `find_flag(rest, "-s")` handling is sufficient, or extend. |
| `new-window` | MISSING | **REAL** | Creates the `swarm-view` window inside the swarm session. TUIC's tab model doesn't have a distinct "window" concept above "session" — needs design: likely just a naming/grouping marker, not a new TUIC entity of its own. |
| `split-window` | MISSING | **REAL — core of the whole feature** | This is what actually creates each teammate's pane. Map to creating a new TUIC PTY session; must support `-P -F '#{pane_id}'` (return format) so Claude Code can capture the created pane's id, and `-c <cwd>`. Requires a pane_id ↔ TUIC session_id mapping maintained by the shim (see below). |
| `select-layout` | MISSING | NOOP | Cosmetic (`main-vertical`/`tiled`); TUIC tabs aren't laid out as panes. |
| `resize-pane` | HAVE | Keep as NOOP-compatible | Already exists for generic tmux compat; confirm it degrades gracefully (success, no-op) when given a fake pane_id rather than a real TUIC session_id, since layout doesn't apply to tabs. |
| `select-pane -T <title>` | MISSING | **REAL, low-risk** | Maps naturally to renaming the TUIC tab's `display_name`. Cheap to implement, meaningfully improves UX (teammates get sensible tab names instead of generic ones), no correctness risk if skipped. |
| `send-keys` | HAVE | **REAL, extend** | Already implemented for named/UUID targets; needs extension to resolve fake pane_ids (from `split-window`) back to real TUIC session_ids via the mapping above. |
| `set-option` (6 variants: `pane-border-style`, `pane-active-border-style`, `window-style`, `remain-on-exit`, `respawn-pane`, `pane-border-format`, `pane-border-status`) | MISSING | NOOP (all) | Pure tmux visual/behavioral cosmetics with no TUIC-tab equivalent. Accept and succeed. |
| `display-message -p '#{pane_id}'` / `'#{window_id}'` | HAVE (stub only — ignores `-p`/`-F`, always returns generic status) | **REAL, must fix** | Must actually parse the `-F`/format string and return the real fake pane_id/window_id the shim generated, not a generic status blob — Claude Code depends on this to learn what it just created. |
| `list-panes` | MISSING | **REAL, minimal** | Needs to report a pane count for a given window/session — used for layout math (`rebalancePanesTiled`/`rebalancePanesWithLeader`). Minimal correct implementation: count of tracked panes in that group. |
| `list-windows` | MISSING | **REAL, minimal** | At minimum needs to report `#{window_name}` (`swarm-view`) correctly when queried. |
| `kill-pane` | MISSING | **REAL** | Maps to closing/killing the corresponding TUIC session. Must actually remove it from the pane_id mapping too, or later lookups will resolve to a dead session. |
| `attach-session` | HAVE | Keep | Focuses the TUIC window via deep link (`tuic://focus`) — already correct for this feature's needs (not clear the swarm backend even calls this one; verify — see "built but maybe unused" below). |

### Built, but not confirmed to be called by Claude Code's pane backend

These exist in `tmux_compat()` today for TUICommander's own general
`tuic`-as-tmux-replacement CLI story (documented in `docs/user-guide/cli.md`
independent of the teammate/swarm feature), not because we found evidence
Claude Code's swarm code calls them. Keep them (real users of the general
`tuic alias` tmux-replacement feature may depend on them), but don't assume
they need swarm-specific extension:

- `list-sessions`/`ls`, `kill-session`, `kill-server` — general session
  management, not seen anywhere in the swarm-specific string constants.
- `capture-pane` — not seen in the swarm string constants either; may be
  used by some other tmux-integration path in Claude Code we haven't
  identified, or may be purely for TUICommander's own general tmux-compat
  users.
- `attach-session` — plausible the swarm backend never calls this (a
  fresh detached "external session mode" session has no reason to need
  attaching from the caller's perspective — the point is the *panes* show
  up as TUIC tabs, not that the user attaches to a tmux client). Verify
  before assuming it's part of the swarm flow.

## 5. Sequencing

0. **Resolve the open question from §1.2 first**: find and confirm the
   actual current trigger for pane-backed teammate spawning, now that
   `TeamCreate` doesn't exist. Don't build the rest of this plan against an
   unconfirmed trigger — re-run the `--debug-file` + `BackendRegistry` grep
   technique from this investigation against whatever that trigger turns
   out to be, and confirm it really does reach `Hit()`.
1. Close the test-coverage gaps in §2 for existing `tmux_compat()`/`cmd_alias()`/
   `find_flag()`/`ipc.rs` behavior before changing any of it.
2. Add `-V` (trivial, unblocks detection entirely) and add regression tests
   pinning `one()`-equivalent behavior (`tmux -V` exit 0 with a plausible
   version string) via the CLI's own test harness (spawn the built binary
   as `tmux` and assert on stdout/exit code, mirroring how `tmux_compat()`
   is exercised today, if at all — per §2, add this pattern since it
   doesn't exist yet).
3. Design and implement the pane_id ↔ TUIC-session_id mapping (needed by
   `split-window`, `send-keys`, `display-message`, `list-panes`, `kill-pane`
   — five of the REAL arms all depend on this one piece of shared state).
4. Implement `split-window`/`new-window`/`display-message` (the minimum
   set to get one real teammate pane end-to-end), verify manually against
   a live `make dev` instance per the project's `live-verify-debug-instance`
   skill (never against Boss's orchestrator instance — see `AGENTS.md`).
5. Implement `select-pane -T`, `kill-pane`, `list-panes`, `list-windows`.
6. NOOP the `set-option`/`select-layout`/`resize-pane` cosmetic arms
   explicitly (don't leave them falling through to the unknown-command
   error — an explicit noop is different from "unimplemented").
7. Re-run the full doc audit against verified behavior (`docs/user-guide/agent-teams.md`'s
   "How It Works" and "Navigating Teammates" sections) once the above is
   real and tested — not before, and not from assumption.
