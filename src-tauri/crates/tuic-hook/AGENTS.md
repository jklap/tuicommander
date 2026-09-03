# tuic-hook — crate rules

Native sidecar TUICommander installs into an AI agent's own hook config (Claude
Code, Gemini, Grok, Codex) to emit `OSC 7770;verb=payload` on busy/idle/awaiting/
toolfail. See `docs/backend/tuic-hook.md` (human-facing CLI/derivation reference)
and `hook-lifecycle.html` (repo root, tracked — full event→verb→feature diagram)
for the "what"; this file is the "how to work on this crate" for an agent.

## Build before you test the generator

`src-tauri/src/agent_hook.rs`'s `golden_wire_output` tests execute the *real
compiled* `tuic-hook` binary end-to-end (not a mock) — they resolve it via
`tuic_cli::resolve_sidecar_path_from`, which expects `target/debug/tuic-hook` (or
`.exe`) to already exist. Run `cargo build --package tuic-hook` before
`cargo nextest run` if you've touched this crate, or `agent_hook.rs`'s tests panic
with an actionable message rather than failing silently.

## Testing this crate alone

`cargo nextest run -p tuic-hook` (56 tests as of this writing). `cargo build
--package tuic-hook && cargo nextest run -p tuicommander agent_hook::` for the
generator-side golden tests that depend on it.

## Env-var test hazard

`TUIC_HOOK_TTY` and `TUIC_PTY_TTY` are process-global env vars mutated by tests in
both `emit.rs` and `tty.rs`. Every test that sets/reads either MUST carry
`#[serial_test::serial]` (bare, no key — one shared group for the whole crate) or
it can interleave with another such test under a plain `cargo test` (nextest's
one-process-per-test default currently masks this, but don't rely on that).

## Adding or changing a Claude Code event's behavior

1. Edit the `DERIVATIONS` table in `src/main.rs` — this is the single source of
   truth for what a *known* `hook_event_name` emits; do not also add flags/argv
   in `agent_hook.rs`'s `claude_hook_map()` unless the event needs an override
   (see the `PreToolUse`→`awaiting` narrow-matcher exception there).
2. Add a `derives_*` unit test in `main.rs` for the new event — one per row should
   exist (the crate has a same-binary safety net for this,
   `help_text_lists_every_derivation_table_event_name`).
3. Update BOTH `docs/backend/tuic-hook.md`'s derivation table AND
   `hook-lifecycle.html`'s per-event card walkthrough — these are two
   independent hand-written sources of the same event list with nothing else
   keeping them in sync. `Elicitation`/`ElicitationResult` shipped in `main.rs`
   two commits before either doc caught up — don't repeat that.
4. If the event is Claude-specific, note in both docs whether Gemini/Grok/Codex
   could plausibly send the same `hook_event_name` — derivation matching is a
   bare string lookup with no per-agent scope (see `todo.md`'s "DERIVATIONS
   lookup is not scoped per agent" entry).

## Extending an existing event's scrape set (not just adding a new event)

When one `hook_event_name` needs its *behavior* to vary by a VALUE inside the
payload (not just by which event fired) — e.g. `Notification`'s 12 different
`notification_type` reasons, only some of which are a genuine block — keep this
binary "dumb": add the new field to `scrape_*` (its own `EventDerivation` bool +
`--emit-*` override flag, same shape as `scrape_message`) and emit it as its own
verb, unconditionally, alongside whatever the event already emits. Do NOT branch
`state` on the payload value here. All classification of what a scraped value
*means* belongs on the receiving end (`pty.rs`), which already owns this pattern
for `notify`'s message text (see `notification_awaiting_outcome`,
`agent-signal-architecture.html#osc-confusion`) — that keeps this crate a pure
"what did Claude Code say" extractor, with no policy logic to keep in sync across
two repos' worth of Claude Code documentation as its enums evolve.

## Startup ordering

`hook_binary::ensure_current()` MUST run before
`agent_hook_commands::reinstall_outdated_hooks()` at Tauri startup (see
`lib.rs`) — the latter's staleness check is only meaningful against a
just-refreshed stable copy.

## Non-negotiable invariants (see `main.rs`'s own module doc comment for the full list)

- Never blocks the agent and never exits non-zero — every path (success,
  malformed input, missing tty, internal panic) is exit 0.
- `toolfail` always precedes `state` on the wire, regardless of argv/derivation
  order — `handle_tuic_state` (`src-tauri/src/pty.rs`) reads-and-clears the
  turn's failure flag exactly when it sees `state=idle`.
- Unrecognized flags AND unrecognized `hook_event_name` values are ignored, not
  errors — a stale installed copy must degrade gracefully against a future
  binary's new vocabulary.
- Any Rust change here needs a `make dev` restart to take effect (this crate is
  `src-tauri/**`, which never hot-reloads) — add a `to-test.md` entry per the
  root `AGENTS.md`'s "Dev Hot Reload" section; do not open a story for it.
- When fixing a stale doc claim inside this binary's own `--help` text or doc
  comments, add the assertion that would have caught it (see
  `help_text_mentions_every_flag_and_env_var`) — this is the established,
  self-evident pattern in this crate's own tests.
