# `tuic-hook`

`tuic-hook` is the native sidecar TUICommander installs into an AI coding agent's own
hook configuration (Claude Code, Gemini, Grok, Codex) to drive that agent's
busy/idle/awaiting state and turn-failure flagging inside TUICommander. It emits
`OSC 7770;verb=payload` escapes to the pty TUICommander created for the calling
agent's session — via `TUIC_PTY_TTY`, stamped onto every child TUICommander spawns
(`src-tauri/src/pty.rs::spawn_pty_pair_with_retry`), with an ancestor-walk fallback
for agents TUICommander didn't spawn. TUICommander reads the emissions back off its
own PTY byte stream (see
[Alacritty Integration → OSC 7770](./alacritty-integration.md#osc-7770--tuic-protocol)
for the wire-level verb table and consumer side).

Source: `src-tauri/crates/tuic-hook/src/main.rs`. Generator side (which builds the
installed hook commands, and the per-agent event→command maps):
`src-tauri/src/agent_hook.rs`. See also `hook-lifecycle.html` (repo root) for a fuller,
visual walk through the whole event→wire→feature pipeline.

## Derivation vs. flags

By default, **`tuic-hook` derives what to emit from the hook payload's own
`hook_event_name` field**, read from stdin (which Claude Code populates on every hook
fire). Every flag below *overrides* its derived counterpart — a plain Claude Code hook
needs no flags at all. This is why most entries in `agent_hook.rs`'s
`claude_hook_map()` carry no argv: adding or changing what a *known* Claude Code event
emits is now a change to this binary alone, not also to the per-agent maps and every
installed `settings.json`.

Gemini, Grok, and Codex still pass `--state` explicitly — their hooks haven't been
verified to send `hook_event_name` in the same shape, so derivation is a no-op for
them (an unrecognized `hook_event_name` derives nothing) and their maps fall back to
flags exactly as they did before derivation existed.

## Derivation table

| `hook_event_name` | derived `state` | derived scrape | derived `toolfail` |
|---|---|---|---|
| `SessionStart` | `busy` | `session_id`, `cwd`, `transcript_path` | — |
| `UserPromptSubmit` | `busy` | — | — |
| `PreToolUse` | `busy` | `tool_name` | — |
| `PostToolUse` | `busy` | `tool_name` | — |
| `PostToolUseFailure` | *(none)* | `tool_name` | from `exit_code` if present, else sentinel `1` — suppressed entirely if `is_interrupt` is `true` |
| `Notification` | `awaiting` | `message` | — |
| `Stop` | `idle` | — | — |
| `StopFailure` | `idle` | — | fixed `1` |
| `SessionEnd` | `idle` | — | — |
| `Elicitation` | `awaiting` | — | — |
| `ElicitationResult` | `busy` | — | — |

An unrecognized or absent `hook_event_name` derives nothing at all — only explicit
flags apply in that case, same as before derivation existed.

**One override survives in `agent_hook.rs`:** Claude's `PreToolUse` entry is scoped to
the `AskUserQuestion|ExitPlanMode` matcher, which means *awaiting*, not the bare
event's derived *busy* (Grok's broad, unmatched `PreToolUse` legitimately derives
busy). That entry carries an explicit `--state awaiting` to express the matcher's
policy; its `tool_name` scrape is still derived, not a flag.

**Known trade-off — `PostToolUseFailure`:** the old explicit `--toolfail-from-stdin`
flag guaranteed a `toolfail=1` fallback even against malformed or absent stdin.
Derivation needs to read `hook_event_name` to know this fire *is*
`PostToolUseFailure` in the first place, so unparseable stdin means the fire can't be
identified at all, and nothing is emitted. Accepted because this JSON is Claude Code's
own generated payload, not user input, and is not expected to be malformed in
practice.

## CLI flags

| Flag | Effect |
|---|---|
| `--state <busy\|awaiting\|idle>` | Overrides the derived state, if any. |
| `--toolfail <code>` | Overrides the derived `toolfail`, with a fixed value. |
| `--toolfail-from-stdin` | Overrides the derived `toolfail`, extracting `exit_code` from stdin itself (legacy alias for `PostToolUseFailure`'s derivation — kept for a binary/settings version mismatch during rollout; see [Migration notes](#migration-notes)). |
| `--emit-session` | Forces the `session_id`/`cwd`/`transcript_path` scrape (legacy alias for `SessionStart`'s derivation). |
| `--emit-tool` | Forces the `tool_name` scrape (legacy alias). |
| `--emit-notify` | Forces the `message` scrape (legacy alias). |
| `--version` | Prints `tuic-hook <version>` and exits. Bypasses the `TUIC_SESSION` gate — used by `hook_binary`'s startup drift check outside any agent session. |
| `--help`, `-h` | Prints a usage summary (this table, condensed) and exits. Also bypasses the `TUIC_SESSION` gate. |

Unrecognized flags, and a value flag missing its value (e.g. a bare trailing
`--state`), are silently ignored — never an error. A stale copy of this binary must
degrade gracefully against a future flag it doesn't understand, per the never-block
invariant below.

## Stdin

A JSON object, read in full on every fire (bounded to 1 MiB — see
`read_stdin_json`/`read_stdin_bounded`). Fields read:

| Field | Used by |
|---|---|
| `hook_event_name` | Derivation lookup (see table above). |
| `session_id`, `cwd`, `transcript_path` | `SessionStart`'s scrape (or `--emit-session`). |
| `tool_name` | `PreToolUse`/`PostToolUse`/`PostToolUseFailure`'s scrape (or `--emit-tool`). |
| `message` | `Notification`'s scrape (or `--emit-notify`). |
| `exit_code` | `PostToolUseFailure`'s `toolfail` derivation (or `--toolfail-from-stdin`). Accepts a JSON number or a numeric string. Claude Code's real `PostToolUseFailure` payload doesn't send this field at all (its schema is `tool_name`, `tool_input`, `tool_use_id`, `error`, `is_interrupt?`, `duration_ms?`) — in practice this always falls back to the sentinel `1`, unless `is_interrupt` is `true` (see below). |
| `is_interrupt` | `PostToolUseFailure`: if `true` (a tool call cancelled via Esc, not a real failure), suppresses the `toolfail` emission entirely rather than falling back to the sentinel. |

Missing, empty-string, or malformed fields are all treated as absent — "omit this
verb" for free-text fields, "fall back to the sentinel `1`" for `exit_code`. Malformed
or empty stdin as a whole falls back to a null JSON value, from which every field
extraction above already degrades the same way — **but note this now costs the
entire fire's derivation, not just one field**: unlike the old explicit flags, which
each independently guaranteed their own verb, `hook_event_name` itself lives in the
same payload, so a truncated-past-the-cap or otherwise unparseable payload loses
state/scrape/toolfail together. `MAX_STDIN_BYTES` (1 MiB) is sized generously
specifically to make this rare — a failed tool call's captured stdout/stderr can
legitimately be large — not to eliminate the possibility.

Two further robustness properties, both load-bearing for the never-block invariant:

- **Bounded in time, not just size.** The read runs on a background thread with a
  500&nbsp;ms (`STDIN_READ_TIMEOUT`) budget; if the caller never closes stdin (its
  hook-invocation stdin-closing behavior is unverified for Gemini/Grok/Codex, which
  never touched stdin at all before derivation existed), `main` proceeds with an
  empty buffer rather than blocking forever. The abandoned thread dies with the
  process on exit.
- **Drains past the cap.** After capturing what it will use, the same background
  thread keeps reading (and discarding) anything the caller writes beyond
  `MAX_STDIN_BYTES`, so a caller that writes a larger payload than we keep never
  blocks on a full pipe waiting for us to keep consuming it.

Reading stdin unconditionally (rather than only when a flag needed it, as before
derivation) costs roughly half a millisecond per fire in practice, dominated by
process-spawn overhead rather than the read itself — not a new dependency, since
Claude Code already sends this JSON payload (including `hook_event_name`) on every
hook event, including ones that used to skip stdin entirely.

## Environment variables

| Variable | Purpose |
|---|---|
| `TUIC_SESSION` | Must be set and non-empty, or every flag is a no-op (checked here, and redundantly in the installed shell command's guard — see `agent_hook.rs`). |
| `TUIC_PTY_TTY` | The pty device path TUICommander stamps onto every child it spawns (`src-tauri/src/pty.rs::spawn_pty_pair_with_retry`). Primary resolution mechanism — TUICommander already knows the device, so there's nothing to infer. Wins over the ancestor walk below. |
| `TUIC_HOOK_TTY` | Overrides the resolved tty write target. Test seam only; never set in production. |
| `TUIC_HOOK_DEBUG` | If set to a non-empty value, prints the resolved tty path to stderr. |

Without `TUIC_PTY_TTY` (an agent TUICommander didn't spawn, or an old binary), `tuic-hook`
falls back to walking process ancestors for the first one with a controlling tty
(`src-tauri/crates/tuic-hook/src/tty.rs`), then to `/dev/tty`.

## Wire format

```
ESC ] 7770 ; verb=payload ESC \
```

One sequence per verb, all verbs for one fire concatenated into a single buffer and
delivered in one `write_all`. `state` and `toolfail` are emitted verbatim (fixed
enum/numeric values); `ccsession`/`cwd`/`transcript`/`tool`/`notify` are
percent-encoded (RFC 3986 unreserved set) since they carry free text that could
otherwise contain the OSC `;` delimiter or control bytes. `toolfail` is always
partitioned ahead of every other verb on the wire, regardless of derivation/argv
order — `handle_tuic_state` (`pty.rs`) reads-and-clears the turn's failure flag at the
exact moment it processes a `state=idle` transition, so `toolfail` must already be
recorded by then.

## Exit-code / never-block contract

Every path — success, malformed input, a missing tty, an internal panic
(`catch_unwind`-wrapped) — exits `0`. A hook that could fail the agent's turn is worse
than no hook at all. `--version` and `--help` are the only paths that print to stdout;
every other invocation is silent on stdout/stderr (unless `TUIC_HOOK_DEBUG` is set) and
communicates purely via the OSC write to the resolved tty.

## Migration notes

`hook_binary::ensure_current()` refreshes the installed binary copy, and
`agent_hook_commands::reinstall_outdated_hooks()` rewrites any enabled agent's
settings file whose installed commands don't match the current map — both run at
startup, in that order (`lib.rs`). This bounds the "old binary, new argv-free settings
entry" gap to a single restart. The reverse direction — a *newer* binary receiving an
*older*, flag-laden settings entry left over from before this derivation model — is
covered by keeping the four legacy `--emit-*`/`--toolfail-from-stdin` flags working as
aliases for the scrape/toolfail they used to request explicitly.
