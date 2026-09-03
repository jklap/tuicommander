# tuic-cli — crate rules

The `tuic` CLI: editor opener, session multiplexer, agent orchestrator, and (when
invoked as `tmux` via `tuic alias`) a tmux compatibility shim. See
`docs/user-guide/cli.md` for the user-facing command reference and
`tmux-swarm-shim.md` (repo root) for the investigation the tmux shim was built
from. The tmux shim's own pane/window/session *topology* lives app-side
(`src-tauri/src/mcp_http/tmux_routes.rs`) — this crate only does argv parsing,
`#{...}` format rendering, target resolution, and exit codes (all pure, in
`src/tmux/{args,format,target}.rs`); `src/tmux/exec.rs` is the only place that
talks to the running instance, via the `TuicBackend` trait (`IpcBackend` in
production, an in-memory `FakeBackend` in `src/tmux/mod.rs`'s tests).

## Testing this crate alone

`cargo nextest run -p tuic-cli` (111 tests as of this writing: pure parser/
format/target tests, an in-memory-backend integration suite in `tmux/mod.rs`
that drives the whole parse→execute pipeline, `ipc.rs`'s mock-socket tests, and
`cmd_alias`'s tempdir-based tests).

## `$TUIC_SOCKET` mock-server tests: join the client thread, don't detach it

`ipc.rs`'s tests point `$TUIC_SOCKET` (a process-global env var) at a scratch
Unix socket and need something to `accept()` the connection `get()`/`post()`
opens. **Do not spawn a detached background thread to run the mock server and
let the test's own `get()` call race it.** An earlier version did exactly
that — bind the listener, spawn a thread with `listener.accept()` inside it,
return immediately, then call `get()` from the test's own body — and it was
genuinely flaky under plain `cargo test` (which runs the whole binary's ~100
tests as OS threads in one process): under heavy scheduling contention, the
real 3-second read timeout in `request_with_headers` could fire before the
detached server thread got scheduled to accept and write, producing a `502`-
shaped `WouldBlock` failure with no logic bug behind it. `#[serial_test::serial]`
alone does not fix this — it only keeps two `ipc.rs` tests from running at the
same time as *each other*; it says nothing about scheduling latency against
unrelated tests hammering the same core count.

The fix (see `round_trip` in `ipc.rs`'s test module): spawn the **client**
(`get("/health")`) on a background thread, and do the **server** side
(`accept()` + read + write + flush) synchronously in the test's own body, then
`.join()` the client thread before asserting. This makes exactly one
`connect()` pair with exactly one `accept()`, with no independent thread
scheduling to race — confirmed via `cargo nextest run` (which runs one test per
process; matches the real gate) passing cleanly on every run, and 20 back-to-
back `cargo test` runs with zero flakes after the fix, versus roughly 1-in-5
before it.

## `new-session` has two genuinely different code paths — most tests want `-P`

`TmuxOp::NewSession`'s executor (`exec.rs`) branches on the `-P` flag, not just
on its output format:

- **Without `-P`** (`tuic alias` general-purpose users, byte-identical to
  pre-shim behavior): goes through `backend.dispatch_legacy(Command::New{..})`
  — the *legacy* `tuic new-session` path, which never touches tmux topology at
  all.
- **With `-P`** (the swarm path — Claude Code always passes it): goes through
  `backend.create_tmux_session(...)`, which actually creates the session in
  topology.

A test using `FakeBackend` that calls `new-session -d -s name` **without**
`-P` therefore creates nothing a later `has-session`/`display-message`/
`list-panes`/etc. in the same test can find — `FakeBackend::dispatch_legacy`
is a no-op stub, so the fake topology map stays empty. This bit real tests
during this crate's initial build-out: three tests set up a session via a
plain `new-session -n <window>` and then asserted against topology that had
never been created, failing with empty resolved names rather than a crash.
**Any `FakeBackend`-based test that needs a session to exist for a later
lookup must create it via `new-session -P -F '<something>'`**, not the bare
form — even if the test doesn't care what gets printed.

## `cwd` must fall back to `std::env::current_dir()`, never stay `None`

`new-session`/`new-window`/`split-window`'s `cwd` field only ever gets a value from an
explicit `-c` flag (`args.rs`'s `p.value('c')`). Real Claude Code swarm calls **never** pass
`-c` — confirmed empirically, both live and via the synthetic wording harness
(`plans/agent-teams-wording-harness/` in the main checkout). Without `exec.rs`'s
`resolve_cwd()` fallback, that leaves `cwd: None` all the way down to `spawn_pty_session`
(`session.rs`), which only calls `cmd.cwd(dir)` when a `cwd` is actually given — so the
spawned pane's PTY silently inherits whatever directory the **TUICommander app process
itself** happens to be running in, not the repo the swarm was actually spawned from. This
produced a real, live, user-visible bug (2026-09-04): a 4-teammate swarm's panes all landed
under an unrelated repo's tab group in the sidebar, because the frontend's
`assignSessionToRepoBranch` correctly parks a session with an unowned cwd in "whichever repo
is currently active" — which is exactly and only wrong because the cwd it received was wrong.

`resolve_cwd()` fixes this by falling back, in priority order, to (1) an existing pane's
already-resolved cwd elsewhere in the same session/window (`topology_cwd_for_session`/
`topology_cwd_for_window`), then (2) `std::env::current_dir()` — the tmux CLI subprocess's
own cwd, inherited from whatever process spawned it (normally Claude Code, itself running
inside the lead session's real repo). `new-session` has nothing to inherit from (it's the
first pane), so it only ever hits (2). The topology-inheritance step (a 2026-09-04 code-review
finding, not part of the original fix) exists because each `tmux` subcommand is its own OS
subprocess — a `split-window`/`new-window` call's own `current_dir()` read is genuinely
independent of whatever `new-session` saw earlier, so relying on it alone would only
guarantee a *correct* cwd, not one that's *identical* across every pane in one swarm if the
calling process's own cwd ever changed between separate subprocess invocations. Inheriting
from topology closes that off entirely, and also matches real tmux's own actual semantics
more closely (real `split-window`/`new-window` without `-c` default to the pane/session being
split from, not to the invoking client — only `new-session` falls back to the client's own
cwd). Any new tmux subcommand that creates a pane must go through this same fallback — check
`exec.rs`'s three existing call sites (`NewSession`, `NewWindow`, `SplitWindow`) for the
pattern before adding a fourth.

**Two related findings from that same review, accepted as-is rather than fixed:**
- `std::env::current_dir()` resolves symlinks (`getcwd(3)` semantics); if a registered repo's
  path was typed through a symlinked ancestor, the frontend's exact-prefix repo-owner matching
  (`src/utils/repoOwnership.ts`) could still fail to match the canonicalized form. This is a
  pre-existing, general path-canonicalization gap spanning anywhere TUICommander resolves a
  session's cwd, not something this fix introduces or worsens — before this fix, split/new
  panes had `cwd: None` and matched *no* repo 100% of the time, so this is strictly an
  improvement, just not a complete one for symlink-affected setups.
- This crate now has multiple independent inline `current_dir().ok().map(|p|
  p.to_string_lossy()...)` implementations (`main.rs`'s `cmd_open` and others, plus this
  module's `resolve_cwd`) instead of one shared helper. Real duplication, but consolidating it
  means touching `main.rs` call sites unrelated to this fix — left as a separate, future
  cleanup rather than scope-creeping this change.

## `DisplayMessage`/`ListPanes` session-name resolution

Both need to report `#{session_name}` for the session that actually *owns*
the resolved window/pane — not `topology.sessions[0]`. The latter shortcut
was the original implementation and is wrong the instant one label ever
holds more than one session (harmless under the normal one-session-per-`-L`
swarm flow, which is exactly why it shipped unnoticed at first and was only
caught by a test that deliberately builds a two-session topology under one
label — `display_message_and_list_panes_report_the_targeted_sessions_own_name`
in `tmux/mod.rs`). Resolve the session via `target::resolve_session` with the
same parsed target used for the window/pane, not via `.first()`.
