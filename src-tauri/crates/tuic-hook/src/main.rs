//! `tuic-hook` — native replacement for the embedded shell one-liners
//! TUICommander installs into an agent's hook settings (Claude, Gemini, Grok,
//! Codex). Emits `OSC 7770;verb=payload` to the calling agent's controlling
//! tty; TUICommander reads it back off its own PTY byte stream.
//!
//! Behavior is derived by default from the hook payload's `hook_event_name`
//! (see `DERIVATIONS` below) — every flag is an override on top of
//! derivation, not the primary mechanism. Adding or changing what a *known*
//! Claude Code event emits is now a change to this binary alone; it no
//! longer also requires editing `agent_hook.rs`'s per-event argv and
//! re-installing every user's hooks. Non-Claude agents (Gemini/Grok/Codex),
//! and any Claude event this binary doesn't recognize, fall back to flags
//! exactly as before derivation existed.
//!
//! Invariants (every one of these is load-bearing — see `docs/FEATURES.md`
//! and `agent_hook.rs` for why):
//! - **Never blocks the agent.** Every path — success, malformed input, a
//!   missing tty, an internal panic — exits 0. A hook that could fail the
//!   agent's turn is worse than no hook at all.
//! - **Inert outside a TUIC session.** Checked here (`TUIC_SESSION` unset ⇒
//!   immediate no-op) *and* by the shell guard in the installed `command`
//!   field, which avoids even spawning this process outside TUIC.
//! - **`toolfail` always precedes `state` on the wire**, regardless of
//!   whether either came from derivation or an explicit flag, and regardless
//!   of argv order — `handle_tuic_state` (pty.rs) reads-and-clears the
//!   turn's failure flag at the exact moment it processes a `state=idle`
//!   transition.
//! - **Unrecognized flags, and unrecognized `hook_event_name` values, are
//!   ignored, not errors** — a stale copy of this binary (see
//!   `hook_binary::ensure_current`) must degrade gracefully against a future
//!   flag or event it doesn't understand, not fail the hook. The one
//!   direction this can't cover is the reverse: an *older* binary handed a
//!   *newer*, argv-free settings.json entry (post-migration to derivation)
//!   has no flags to fall back on and emits nothing. `hook_binary::ensure_current`
//!   *tries* to refresh the binary before `reinstall_outdated_hooks` rewrites
//!   settings (see `lib.rs`) — but that refresh is best-effort (logged, not
//!   propagated) and its caller never checks the outcome before proceeding to
//!   rewrite settings anyway, so this gap is a real possibility on a failed
//!   refresh (missing bundled sidecar, permission denied, disk full), not a
//!   fully closed one. The legacy `--emit-*`/`--toolfail-from-stdin` flags
//!   stay supported as aliases as the only actual mitigation.
//! - **stdin is now read unconditionally** (still bounded — see
//!   `read_stdin_json`). Previously a bare `--state` skipped stdin entirely;
//!   that fast path is gone because knowing `hook_event_name` requires
//!   reading it. This costs one small bounded read per fire, not a new
//!   dependency — Claude Code already sends a JSON payload with
//!   `hook_event_name` (plus session_id/cwd/transcript_path as documented
//!   common fields) on every hook event, including the ones that used to
//!   skip stdin. One real cost this reintroduces: unlike the old explicit
//!   flags, which each independently guaranteed their own verb regardless of
//!   whether *other* stdin fields parsed cleanly, a stdin read that fails
//!   (truncated past `MAX_STDIN_BYTES`, or simply malformed) now loses
//!   `hook_event_name` too — so derivation loses the *entire* fire (state
//!   included, not just `PostToolUseFailure`'s `toolfail`) rather than just
//!   the one field that happened to be oversized. `MAX_STDIN_BYTES` is sized
//!   generously (see below) specifically to make this rare in practice, not
//!   to eliminate it.

mod emit;
mod payload;
mod tty;

use emit::Emission;
use serde_json::Value;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // --version and --help bypass the TUIC_SESSION gate entirely: `--version`
    // is used by `hook_binary`'s startup drift check outside any agent
    // session, and `--help` is for a developer running this by hand, who has
    // no session either.
    if args.iter().any(|a| a == "--version") {
        println!("tuic-hook {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{}", help_text());
        return;
    }

    // Never let a panic escape as a non-zero exit or a crash report — a hook
    // must be a no-op-on-error citizen no matter what goes wrong internally.
    let _ = std::panic::catch_unwind(|| run(&args));
    std::process::exit(0);
}

fn help_text() -> String {
    format!(
        r#"tuic-hook {version} — emits OSC 7770 agent-state escapes to the calling agent's tty.

Installed by TUICommander into Claude Code (and Gemini/Grok/Codex) hook commands.
Never blocks: exits 0 on success, malformed input, a missing tty, or an internal
panic. Inert unless TUIC_SESSION is set in the environment.

USAGE:
    tuic-hook [FLAGS]

By default, behavior is DERIVED from the hook payload's `hook_event_name` (read
from stdin, which Claude Code populates on every hook event). Flags OVERRIDE their
derived counterpart rather than replacing derivation outright — a plain Claude Code
hook needs no flags at all.

DERIVATION (Claude Code events):
    SessionStart          state=busy      scrapes session_id, cwd, transcript_path
    UserPromptSubmit      state=busy
    PreToolUse            state=busy      scrapes tool_name
    PostToolUse           state=busy      scrapes tool_name
    PostToolUseFailure    (no state)      scrapes tool_name; toolfail=<exit_code, default 1>
    Notification          state=awaiting  scrapes message
    Elicitation           state=awaiting  MCP server asking the user for input mid tool call
    ElicitationResult     state=busy      paired retraction for Elicitation
    Stop                  state=idle
    StopFailure           state=idle      toolfail=1
    SessionEnd            state=idle
An unrecognized or absent `hook_event_name` derives nothing; only explicit flags apply.

FLAGS (override the derived value; freely combinable):
    --state <busy|awaiting|idle>   Force the state verb, regardless of derivation.
    --toolfail <code>              Force a fixed toolfail verb.
    --toolfail-from-stdin          Force toolfail, extracting `exit_code` from stdin
                                    JSON (falls back to "1" if absent or malformed).
    --emit-session                 Force scraping session_id/cwd/transcript_path.
    --emit-tool                    Force scraping tool_name.
    --emit-notify                  Force scraping message.
    --version                      Print the version and exit (no TUIC_SESSION needed).
    --help, -h                     Print this message and exit (no TUIC_SESSION needed).

Unrecognized flags, and value flags missing their value, are silently ignored — a
stale copy of this binary must degrade gracefully against a future flag it doesn't
understand, never fail the hook.

STDIN:
    A JSON object, read in full (bounded to 1 MiB). Fields read: hook_event_name,
    session_id, cwd, transcript_path, tool_name, message, exit_code. Missing, empty,
    or malformed fields are treated as absent — never an error. A payload truncated
    past the bound loses the whole fire's derivation, not just the oversized field.

ENVIRONMENT:
    TUIC_SESSION      Must be set and non-empty, or every flag above is a no-op.
    TUIC_HOOK_TTY     Overrides the resolved tty write target (test seam).
    TUIC_HOOK_DEBUG   If set, prints the resolved tty path to stderr.

WIRE FORMAT:
    ESC ] 7770 ; verb=payload ESC \    (one sequence per verb, one write per fire)
    Free-text payloads (ccsession, cwd, transcript, tool, notify) are percent-encoded;
    state and toolfail are fixed enum/numeric values, emitted verbatim.
"#,
        version = env!("CARGO_PKG_VERSION")
    )
}

fn run(args: &[String]) {
    if !session_active() {
        return;
    }
    let parsed = parse_args(args);
    let stdin_json = read_stdin_json();
    let pairs = build_emissions(&parsed, &stdin_json);
    emit::emit(&pairs);
}

fn session_active() -> bool {
    std::env::var("TUIC_SESSION").is_ok_and(|v| !v.is_empty())
}

#[derive(Default, Debug, PartialEq)]
struct ParsedArgs {
    state: Option<String>,
    toolfail: Option<String>,
    toolfail_from_stdin: bool,
    emit_session: bool,
    emit_tool: bool,
    emit_notify: bool,
}

/// Hand-rolled, not clap: this is most of the per-fire cost a compiled
/// binary was meant to remove versus reusing the `tuic` CLI's full command
/// tree (see the plan's benchmark). Unknown flags and flags missing their
/// value are silently skipped — never an error, per the never-block
/// invariant above.
fn parse_args(args: &[String]) -> ParsedArgs {
    let mut out = ParsedArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--state" => {
                if let Some(v) = args.get(i + 1) {
                    out.state = Some(v.clone());
                    i += 1;
                }
            }
            "--toolfail" => {
                if let Some(v) = args.get(i + 1) {
                    out.toolfail = Some(v.clone());
                    i += 1;
                }
            }
            "--toolfail-from-stdin" => out.toolfail_from_stdin = true,
            "--emit-session" => out.emit_session = true,
            "--emit-tool" => out.emit_tool = true,
            "--emit-notify" => out.emit_notify = true,
            _ => {} // unrecognized — ignore, don't error
        }
        i += 1;
    }
    out
}

/// Bounded read: Claude Code hook payloads are usually small JSON objects,
/// but a failed tool call's `tool_response` can legitimately carry a large
/// stdout/stderr capture — this must never block on or exhaust memory over
/// that (or a misbehaving/adversarial pipe) while still tolerating a normal
/// large payload. Malformed, empty, absent, or oversized-and-truncated
/// stdin all fall back to `Value::Null` — but note that failure now costs
/// the *whole* fire's derivation (see the module doc comment's stdin
/// invariant), not just one field, which is why `MAX_STDIN_BYTES` is sized
/// generously rather than kept tight.
///
/// Bounded in *time*, not just size: stdin is now read on every fire,
/// including for Gemini/Grok/Codex entries that never touched it before
/// derivation existed (they only ever passed `--state`). Unlike Claude Code,
/// those agents' hook-invocation stdin-closing behavior has never been
/// verified — if one of them spawns the hook with stdin inherited from an
/// open interactive tty rather than piped-then-closed, a plain
/// `read_to_end()` would block forever, violating the one invariant this
/// whole binary exists to guarantee (see the module doc comment). The read
/// runs on a background thread so a hang there can never hang `main` — the
/// thread is abandoned (and killed with the process) on timeout.
fn read_stdin_json() -> Value {
    serde_json::from_slice(&read_stdin_bounded()).unwrap_or(Value::Null)
}

const MAX_STDIN_BYTES: u64 = 1024 * 1024;
const STDIN_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

fn read_stdin_bounded() -> Vec<u8> {
    use std::io::Read;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::stdin().take(MAX_STDIN_BYTES).read_to_end(&mut buf);
        // The receiver may already be gone (timed out) — a dropped channel
        // is not an error worth handling, just a thread that finished late.
        let _ = tx.send(buf);
        // Only after handing off what we'll actually use: drain and discard
        // anything past the cap. A caller writing a payload larger than
        // MAX_STDIN_BYTES would otherwise block on a full pipe forever,
        // since nothing else here ever reads past the cap — this keeps that
        // block on the *caller's* side from ever happening, without
        // delaying the result above (the send already happened). If the
        // caller never closes stdin at all, this drain just blocks
        // harmlessly here until the process exits.
        let mut stdin = std::io::stdin();
        let mut discard = [0u8; 8192];
        while stdin.read(&mut discard).is_ok_and(|n| n > 0) {}
    });
    rx.recv_timeout(STDIN_READ_TIMEOUT).unwrap_or_default()
}

/// The sentinel emitted when a real exit code can't be determined — same
/// fallback value the original `jq`-based hook used, and its meaning is
/// unchanged: presence of the `toolfail` verb at all is the signal
/// `state.rs::turn_error_flags` on the receiving end acts on, not this value.
const TOOLFAIL_FALLBACK: &str = "1";

/// What a recognized `hook_event_name` derives on its own, absent any
/// overriding flag. This table is the single place Claude Code event policy
/// lives — see the module doc comment above for why that matters. Verified
/// against the official hooks reference (https://code.claude.com/docs/en/hooks):
/// `PostToolUseFailure` and `StopFailure` are real, distinct event names
/// Claude Code fires (not `PostToolUse`/`Stop` plus a secondary flag), each
/// reporting its own literal name as `hook_event_name`.
struct EventDerivation {
    event: &'static str,
    state: Option<&'static str>,
    scrape_tool_name: bool,
    scrape_message: bool,
    scrape_session_metadata: bool,
    toolfail: DerivedToolfail,
}

enum DerivedToolfail {
    None,
    /// `PostToolUseFailure`: extract `exit_code` from stdin, same fallback
    /// as the legacy `--toolfail-from-stdin` flag.
    FromStdinExitCode,
    /// `StopFailure`: no distinguishing field of its own — Claude Code's
    /// `Stop`/`StopFailure` pair is told apart only by which event name
    /// fires, so the failure signal here is the event itself, not a value
    /// extracted from the payload.
    Fixed(&'static str),
}

const DERIVATIONS: &[EventDerivation] = &[
    EventDerivation {
        event: "SessionStart",
        state: Some("busy"),
        scrape_tool_name: false,
        scrape_message: false,
        scrape_session_metadata: true,
        toolfail: DerivedToolfail::None,
    },
    EventDerivation {
        event: "UserPromptSubmit",
        state: Some("busy"),
        scrape_tool_name: false,
        scrape_message: false,
        scrape_session_metadata: false,
        toolfail: DerivedToolfail::None,
    },
    EventDerivation {
        event: "PreToolUse",
        state: Some("busy"),
        scrape_tool_name: true,
        scrape_message: false,
        scrape_session_metadata: false,
        toolfail: DerivedToolfail::None,
    },
    EventDerivation {
        event: "PostToolUse",
        state: Some("busy"),
        scrape_tool_name: true,
        scrape_message: false,
        scrape_session_metadata: false,
        toolfail: DerivedToolfail::None,
    },
    EventDerivation {
        event: "PostToolUseFailure",
        state: None,
        scrape_tool_name: true,
        scrape_message: false,
        scrape_session_metadata: false,
        toolfail: DerivedToolfail::FromStdinExitCode,
    },
    EventDerivation {
        event: "Notification",
        state: Some("awaiting"),
        scrape_tool_name: false,
        scrape_message: true,
        scrape_session_metadata: false,
        toolfail: DerivedToolfail::None,
    },
    EventDerivation {
        // MCP `elicitation/create` — an MCP server asking the user for input mid
        // tool call. Not a tool call itself, so no `PreToolUse` matcher reaches
        // it, and its dialog matches none of the screen heuristics (options
        // render horizontally, footer reads "Esc to cancel", not "Enter to
        // select"). Without this the tab stays "busy" while blocked on the user.
        event: "Elicitation",
        state: Some("awaiting"),
        scrape_tool_name: false,
        scrape_message: false,
        scrape_session_metadata: false,
        toolfail: DerivedToolfail::None,
    },
    EventDerivation {
        // Paired retraction for `Elicitation` — awaiting is sticky, so a set
        // with no matching clear latches the badge forever once the user answers.
        event: "ElicitationResult",
        state: Some("busy"),
        scrape_tool_name: false,
        scrape_message: false,
        scrape_session_metadata: false,
        toolfail: DerivedToolfail::None,
    },
    EventDerivation {
        event: "Stop",
        state: Some("idle"),
        scrape_tool_name: false,
        scrape_message: false,
        scrape_session_metadata: false,
        toolfail: DerivedToolfail::None,
    },
    EventDerivation {
        event: "StopFailure",
        state: Some("idle"),
        scrape_tool_name: false,
        scrape_message: false,
        scrape_session_metadata: false,
        toolfail: DerivedToolfail::Fixed("1"),
    },
    EventDerivation {
        event: "SessionEnd",
        state: Some("idle"),
        scrape_tool_name: false,
        scrape_message: false,
        scrape_session_metadata: false,
        toolfail: DerivedToolfail::None,
    },
];

fn find_derivation(stdin_json: &Value) -> Option<&'static EventDerivation> {
    let name = str_field(stdin_json, "hook_event_name")?;
    DERIVATIONS.iter().find(|d| d.event == name)
}

fn build_emissions(parsed: &ParsedArgs, stdin_json: &Value) -> Vec<Emission> {
    let derivation = find_derivation(stdin_json);
    let mut pairs = Vec::new();

    let scrape_session_metadata =
        parsed.emit_session || derivation.is_some_and(|d| d.scrape_session_metadata);
    let scrape_tool_name = parsed.emit_tool || derivation.is_some_and(|d| d.scrape_tool_name);
    let scrape_message = parsed.emit_notify || derivation.is_some_and(|d| d.scrape_message);

    if scrape_session_metadata {
        if let Some(v) = str_field(stdin_json, "session_id") {
            pairs.push(Emission::encoded("ccsession", v));
        }
        if let Some(v) = str_field(stdin_json, "cwd") {
            pairs.push(Emission::encoded("cwd", v));
        }
        if let Some(v) = str_field(stdin_json, "transcript_path") {
            pairs.push(Emission::encoded("transcript", v));
        }
    }
    if scrape_tool_name && let Some(v) = str_field(stdin_json, "tool_name") {
        pairs.push(Emission::encoded("tool", v));
    }
    if scrape_message && let Some(v) = str_field(stdin_json, "message") {
        pairs.push(Emission::encoded("notify", v));
    }

    // toolfail: an explicit fixed value or `--toolfail-from-stdin` always
    // overrides whatever the event would otherwise derive — the two flags
    // are never passed together by any generated hook command, but if they
    // were, a caller-supplied fixed value is the more explicit request.
    let toolfail_code = if let Some(code) = &parsed.toolfail {
        Some(code.clone())
    } else if parsed.toolfail_from_stdin {
        toolfail_from_exit_code(stdin_json)
    } else {
        match derivation.map(|d| &d.toolfail) {
            Some(DerivedToolfail::Fixed(v)) => Some(v.to_string()),
            Some(DerivedToolfail::FromStdinExitCode) => toolfail_from_exit_code(stdin_json),
            _ => None,
        }
    };
    if let Some(code) = toolfail_code {
        pairs.push(Emission::verbatim("toolfail", code));
    }

    // state: an explicit `--state` always overrides the derived state —
    // needed for Claude's narrow `PreToolUse` entry, whose matcher
    // (AskUserQuestion|ExitPlanMode) means "awaiting" while the bare event
    // derives "busy" (Grok's broad, unmatched `PreToolUse` legitimately
    // means busy — the distinction is the agent's matcher policy, not
    // something this binary should hardcode).
    let state = parsed
        .state
        .clone()
        .or_else(|| derivation.and_then(|d| d.state).map(str::to_string));
    if let Some(state) = state {
        pairs.push(Emission::verbatim("state", state));
    }

    // Defensive ordering, ported from the shell generator's
    // `hook_command_multi`: `toolfail` must reach the wire before `state`
    // regardless of the order these were pushed above, because
    // `handle_tuic_state` reads-and-clears the turn's failure flag at the
    // exact moment it processes `state=idle`.
    let (toolfail, rest): (Vec<_>, Vec<_>) = pairs.into_iter().partition(|p| p.verb == "toolfail");
    toolfail.into_iter().chain(rest).collect()
}

/// `None` suppresses the toolfail emission entirely — used for
/// `is_interrupt: true` (a user-cancelled tool call via Esc, not a real
/// failure; PostToolUseFailure's real schema carries this field, unlike
/// `exit_code`, which it never sends — see `toolfail_from_exit_code`'s doc
/// comment above its call sites).
fn toolfail_from_exit_code(stdin_json: &Value) -> Option<String> {
    if stdin_json.get("is_interrupt").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    Some(
        stdin_json
            .get("exit_code")
            .and_then(exit_code_as_string)
            .unwrap_or_else(|| TOOLFAIL_FALLBACK.to_string()),
    )
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// Accept a JSON number (`42`) or numeric string (`"42"`) for `exit_code` —
/// Claude Code's own schema is a number, but this tolerates a stringified one
/// too rather than silently falling back when a future version changes shape.
fn exit_code_as_string(v: &Value) -> Option<String> {
    if let Some(n) = v.as_i64() {
        return Some(n.to_string());
    }
    v.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_state_flag() {
        let a = parse_args(&["--state".into(), "busy".into()]);
        assert_eq!(a.state.as_deref(), Some("busy"));
    }

    #[test]
    fn parses_toolfail_flag_with_value() {
        let a = parse_args(&["--toolfail".into(), "1".into()]);
        assert_eq!(a.toolfail.as_deref(), Some("1"));
    }

    #[test]
    fn parses_boolean_flags() {
        let a = parse_args(&[
            "--toolfail-from-stdin".into(),
            "--emit-session".into(),
            "--emit-tool".into(),
            "--emit-notify".into(),
        ]);
        assert!(a.toolfail_from_stdin);
        assert!(a.emit_session);
        assert!(a.emit_tool);
        assert!(a.emit_notify);
    }

    #[test]
    fn unrecognized_flags_are_ignored_not_fatal() {
        let a = parse_args(&["--nonsense".into(), "--state".into(), "idle".into()]);
        assert_eq!(a.state.as_deref(), Some("idle"));
    }

    #[test]
    fn flag_missing_its_value_is_ignored() {
        // `--state` with nothing after it — must not panic or consume the
        // next flag as its value.
        let a = parse_args(&["--state".into()]);
        assert_eq!(a.state, None);
    }

    #[test]
    fn combined_state_and_toolfail_puts_toolfail_first() {
        let parsed = ParsedArgs {
            state: Some("idle".into()),
            toolfail: Some("1".into()),
            ..Default::default()
        };
        let pairs = build_emissions(&parsed, &Value::Null);
        assert_eq!(pairs[0].verb, "toolfail");
        assert_eq!(pairs[1].verb, "state");
    }

    #[test]
    fn toolfail_from_stdin_extracts_exit_code() {
        let parsed = ParsedArgs {
            toolfail_from_stdin: true,
            ..Default::default()
        };
        let json = serde_json::json!({"exit_code": 42});
        let pairs = build_emissions(&parsed, &json);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].payload, "42");
    }

    #[test]
    fn toolfail_from_stdin_falls_back_to_sentinel_on_missing_field() {
        let parsed = ParsedArgs {
            toolfail_from_stdin: true,
            ..Default::default()
        };
        let pairs = build_emissions(&parsed, &Value::Null);
        assert_eq!(pairs[0].payload, TOOLFAIL_FALLBACK);
    }

    #[test]
    fn toolfail_from_stdin_falls_back_on_wrong_type() {
        let parsed = ParsedArgs {
            toolfail_from_stdin: true,
            ..Default::default()
        };
        let json = serde_json::json!({"exit_code": {"nested": true}});
        let pairs = build_emissions(&parsed, &json);
        assert_eq!(pairs[0].payload, TOOLFAIL_FALLBACK);
    }

    #[test]
    fn toolfail_from_stdin_accepts_stringified_exit_code() {
        let parsed = ParsedArgs {
            toolfail_from_stdin: true,
            ..Default::default()
        };
        let json = serde_json::json!({"exit_code": "7"});
        let pairs = build_emissions(&parsed, &json);
        assert_eq!(pairs[0].payload, "7");
    }

    #[test]
    fn explicit_toolfail_wins_over_toolfail_from_stdin() {
        let parsed = ParsedArgs {
            toolfail: Some("1".into()),
            toolfail_from_stdin: true,
            ..Default::default()
        };
        let json = serde_json::json!({"exit_code": 99});
        let pairs = build_emissions(&parsed, &json);
        assert_eq!(pairs.len(), 1, "must not double-emit toolfail");
        assert_eq!(pairs[0].payload, "1");
    }

    #[test]
    fn emit_session_extracts_all_three_fields() {
        let parsed = ParsedArgs {
            emit_session: true,
            ..Default::default()
        };
        let json = serde_json::json!({
            "session_id": "abc123",
            "cwd": "/Users/me/project",
            "transcript_path": "/tmp/t.jsonl",
        });
        let pairs = build_emissions(&parsed, &json);
        let verbs: Vec<&str> = pairs.iter().map(|p| p.verb).collect();
        assert_eq!(verbs, ["ccsession", "cwd", "transcript"]);
    }

    #[test]
    fn emit_session_omits_missing_fields_rather_than_emitting_empty() {
        let parsed = ParsedArgs {
            emit_session: true,
            ..Default::default()
        };
        let json = serde_json::json!({"session_id": "abc123"});
        let pairs = build_emissions(&parsed, &json);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].verb, "ccsession");
    }

    #[test]
    fn emit_session_omits_empty_string_fields() {
        let parsed = ParsedArgs {
            emit_session: true,
            ..Default::default()
        };
        let json = serde_json::json!({"session_id": ""});
        let pairs = build_emissions(&parsed, &json);
        assert!(pairs.is_empty());
    }

    #[test]
    fn emit_tool_extracts_tool_name() {
        let parsed = ParsedArgs {
            emit_tool: true,
            ..Default::default()
        };
        let json = serde_json::json!({"tool_name": "Bash", "tool_input": {"command": "ls"}});
        let pairs = build_emissions(&parsed, &json);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].verb, "tool");
        assert_eq!(pairs[0].payload, "Bash");
    }

    #[test]
    fn emit_notify_extracts_message_and_encodes_it() {
        let parsed = ParsedArgs {
            emit_notify: true,
            ..Default::default()
        };
        let json = serde_json::json!({"message": "needs your input; now"});
        let pairs = build_emissions(&parsed, &json);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].verb, "notify");
        assert!(!pairs[0].payload.contains(';'), "must be percent-encoded");
    }

    #[test]
    fn state_and_no_stdin_flags_never_touches_stdin_json() {
        // Sanity: state-only emission shouldn't depend on stdin content at all.
        let parsed = ParsedArgs {
            state: Some("busy".into()),
            ..Default::default()
        };
        let pairs = build_emissions(&parsed, &Value::Null);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].verb, "state");
        assert_eq!(pairs[0].payload, "busy");
    }

    // -- hook_event_name derivation ----------------------------------------

    #[test]
    fn derives_busy_and_session_metadata_for_session_start() {
        let json = serde_json::json!({
            "hook_event_name": "SessionStart",
            "session_id": "abc123",
            "cwd": "/tmp/proj",
            "transcript_path": "/tmp/t.jsonl",
        });
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        let verbs: Vec<&str> = pairs.iter().map(|p| p.verb).collect();
        assert_eq!(verbs, ["ccsession", "cwd", "transcript", "state"]);
        assert_eq!(pairs.last().unwrap().payload, "busy");
    }

    #[test]
    fn derives_busy_for_user_prompt_submit_with_no_scrape() {
        let json = serde_json::json!({"hook_event_name": "UserPromptSubmit"});
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].verb, "state");
        assert_eq!(pairs[0].payload, "busy");
    }

    #[test]
    fn derives_busy_and_tool_name_for_pre_and_post_tool_use() {
        for event in ["PreToolUse", "PostToolUse"] {
            let json = serde_json::json!({"hook_event_name": event, "tool_name": "Bash"});
            let pairs = build_emissions(&ParsedArgs::default(), &json);
            assert_eq!(pairs.len(), 2, "event {event}");
            assert_eq!(pairs[0].verb, "tool");
            assert_eq!(pairs[0].payload, "Bash");
            assert_eq!(pairs[1].verb, "state");
            assert_eq!(pairs[1].payload, "busy");
        }
    }

    #[test]
    fn explicit_state_overrides_the_derived_state() {
        // Claude's narrow PreToolUse entry (AskUserQuestion|ExitPlanMode)
        // means "awaiting", overriding the bare event's derived "busy".
        let json =
            serde_json::json!({"hook_event_name": "PreToolUse", "tool_name": "AskUserQuestion"});
        let parsed = ParsedArgs {
            state: Some("awaiting".into()),
            ..Default::default()
        };
        let pairs = build_emissions(&parsed, &json);
        assert_eq!(
            pairs.iter().find(|p| p.verb == "state").unwrap().payload,
            "awaiting"
        );
    }

    #[test]
    fn derives_toolfail_and_tool_name_for_post_tool_use_failure_with_no_state() {
        // Claude Code's real PostToolUseFailure schema (v2.1.245) is
        // {tool_name, tool_input, tool_use_id, error, is_interrupt?,
        // duration_ms?} — there is no `exit_code` field at all. This is the
        // honest shape a real hook fire sends; the resulting fallback to
        // TOOLFAIL_FALLBACK is the actual behavior in production, not the
        // `exit_code: 42` a real Claude Code build never sends.
        let json = serde_json::json!({
            "hook_event_name": "PostToolUseFailure",
            "tool_name": "Bash",
            "tool_use_id": "toolu_1",
            "error": "command failed",
        });
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        assert!(
            !pairs.iter().any(|p| p.verb == "state"),
            "PostToolUseFailure derives no state — Stop/StopFailure handle the transition"
        );
        assert_eq!(
            pairs.iter().find(|p| p.verb == "toolfail").unwrap().payload,
            TOOLFAIL_FALLBACK,
            "no exit_code in the real schema — falls back to the sentinel"
        );
        assert_eq!(
            pairs.iter().find(|p| p.verb == "tool").unwrap().payload,
            "Bash"
        );
    }

    #[test]
    fn post_tool_use_failure_with_is_interrupt_emits_no_toolfail() {
        // A user pressing Esc during a tool call fires PostToolUseFailure
        // with is_interrupt: true — that's a user-cancelled call, not a real
        // failure, and must not paint a red gutter tick on the turn.
        let json = serde_json::json!({
            "hook_event_name": "PostToolUseFailure",
            "tool_name": "Bash",
            "error": "interrupted",
            "is_interrupt": true,
        });
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        assert!(
            !pairs.iter().any(|p| p.verb == "toolfail"),
            "is_interrupt: true must suppress the toolfail emission entirely, got: {pairs:?}"
        );
        // The tool-name scrape is unaffected — still useful metadata.
        assert_eq!(
            pairs.iter().find(|p| p.verb == "tool").unwrap().payload,
            "Bash"
        );
    }

    #[test]
    fn is_interrupt_false_or_absent_still_emits_toolfail() {
        for json in [
            serde_json::json!({"hook_event_name": "PostToolUseFailure", "is_interrupt": false}),
            serde_json::json!({"hook_event_name": "PostToolUseFailure"}),
        ] {
            let pairs = build_emissions(&ParsedArgs::default(), &json);
            assert!(
                pairs.iter().any(|p| p.verb == "toolfail"),
                "expected a toolfail emission for {json:?}, got: {pairs:?}"
            );
        }
    }

    #[test]
    fn legacy_toolfail_from_stdin_flag_also_respects_is_interrupt() {
        // --toolfail-from-stdin is the pre-derivation flag path, calling the
        // same underlying extraction — still installed for real today (the
        // user's live settings.json predates the derivation migration), so
        // this path must get the same is_interrupt guard, not just the new
        // DerivedToolfail::FromStdinExitCode path.
        let json = serde_json::json!({"is_interrupt": true});
        let parsed = ParsedArgs {
            toolfail_from_stdin: true,
            ..Default::default()
        };
        let pairs = build_emissions(&parsed, &json);
        assert!(
            !pairs.iter().any(|p| p.verb == "toolfail"),
            "got: {pairs:?}"
        );
    }

    #[test]
    fn derives_awaiting_and_message_for_notification() {
        let json = serde_json::json!({"hook_event_name": "Notification", "message": "needs input"});
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        assert_eq!(pairs[0].verb, "notify");
        assert_eq!(pairs[1].verb, "state");
        assert_eq!(pairs[1].payload, "awaiting");
    }

    #[test]
    fn derives_idle_for_stop_and_session_end() {
        for event in ["Stop", "SessionEnd"] {
            let json = serde_json::json!({"hook_event_name": event});
            let pairs = build_emissions(&ParsedArgs::default(), &json);
            assert_eq!(pairs.len(), 1, "event {event}");
            assert_eq!(pairs[0].verb, "state");
            assert_eq!(pairs[0].payload, "idle");
        }
    }

    #[test]
    fn derives_toolfail_before_idle_for_stop_failure() {
        let json = serde_json::json!({"hook_event_name": "StopFailure"});
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        assert_eq!(pairs[0].verb, "toolfail");
        assert_eq!(pairs[0].payload, "1");
        assert_eq!(pairs[1].verb, "state");
        assert_eq!(pairs[1].payload, "idle");
    }

    #[test]
    fn unrecognized_hook_event_name_derives_nothing() {
        let json = serde_json::json!({"hook_event_name": "SomeFutureEvent", "tool_name": "Bash"});
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        assert!(
            pairs.is_empty(),
            "an unknown event must derive nothing and never panic"
        );
    }

    #[test]
    fn explicit_toolfail_overrides_a_derived_toolfail() {
        let json = serde_json::json!({"hook_event_name": "StopFailure"});
        let parsed = ParsedArgs {
            toolfail: Some("7".into()),
            ..Default::default()
        };
        let pairs = build_emissions(&parsed, &json);
        let toolfails: Vec<_> = pairs.iter().filter(|p| p.verb == "toolfail").collect();
        assert_eq!(toolfails.len(), 1, "must not double-emit toolfail");
        assert_eq!(toolfails[0].payload, "7");
    }

    #[test]
    fn legacy_emit_tool_flag_still_works_without_a_recognized_hook_event_name() {
        let json = serde_json::json!({"tool_name": "Bash"});
        let parsed = ParsedArgs {
            emit_tool: true,
            ..Default::default()
        };
        let pairs = build_emissions(&parsed, &json);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].verb, "tool");
    }

    #[test]
    fn legacy_toolfail_from_stdin_flag_still_works_without_a_recognized_hook_event_name() {
        let json = serde_json::json!({"exit_code": 5});
        let parsed = ParsedArgs {
            toolfail_from_stdin: true,
            ..Default::default()
        };
        let pairs = build_emissions(&parsed, &json);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].payload, "5");
    }

    #[test]
    fn no_hook_event_name_and_no_flags_emits_nothing() {
        let pairs = build_emissions(&ParsedArgs::default(), &serde_json::json!({}));
        assert!(pairs.is_empty());
    }

    #[test]
    fn empty_string_hook_event_name_derives_nothing() {
        // str_field() already treats an empty string as absent for every
        // free-text field; must hold for hook_event_name too, not just
        // silently match a derivation with an empty `event` (there is none,
        // but a future table edit should not need to remember this).
        let json = serde_json::json!({"hook_event_name": "", "tool_name": "Bash"});
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        assert!(
            pairs.is_empty(),
            "an empty hook_event_name must derive nothing, same as an absent one"
        );
    }

    #[test]
    fn non_string_hook_event_name_derives_nothing_and_does_not_panic() {
        let json = serde_json::json!({"hook_event_name": 12345, "tool_name": "Bash"});
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        assert!(pairs.is_empty());
    }

    #[test]
    fn short_help_flag_is_recognized_by_main_dispatch() {
        // main() checks `a == "--help" || a == "-h"` before ever calling
        // run() — this can't be exercised through build_emissions/run(), so
        // assert the exact condition main() uses instead, to catch a future
        // edit that silently drops the short alias.
        let args = ["-h".to_string()];
        assert!(args.iter().any(|a| a == "--help" || a == "-h"));
    }

    #[test]
    fn help_text_mentions_every_flag_and_env_var() {
        let text = help_text();
        for needle in [
            "--state",
            "--toolfail",
            "--toolfail-from-stdin",
            "--emit-session",
            "--emit-tool",
            "--emit-notify",
            "--version",
            "--help",
            "TUIC_SESSION",
            "TUIC_HOOK_TTY",
            "TUIC_HOOK_DEBUG",
            "PostToolUseFailure",
            "StopFailure",
        ] {
            assert!(text.contains(needle), "help text missing {needle}");
        }
    }

    #[test]
    fn help_text_lists_every_derivation_table_event_name() {
        // The DERIVATIONS table and help_text()'s printed table are two
        // independent hand-written sources of the same 9 events — nothing
        // else keeps them in sync. This at least catches an event added to
        // one and forgotten in the other.
        let text = help_text();
        for d in DERIVATIONS {
            assert!(
                text.contains(d.event),
                "help text is missing DERIVATIONS entry {}",
                d.event
            );
        }
    }
}
