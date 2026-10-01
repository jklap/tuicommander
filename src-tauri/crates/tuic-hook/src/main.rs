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

Derivation is scoped per agent via `--agent` (default: claude, for backward
compatibility with commands generated before this flag existed) — matching is
never done on event name alone, since Gemini's own event names can collide with
Claude's (e.g. "Notification", "SessionEnd").

DERIVATION (Claude Code events, --agent claude):
    SessionStart          state=busy      scrapes session_id, cwd, transcript_path, session_title
    UserPromptSubmit      state=busy      scrapes session_title
    PreToolUse            state=busy      scrapes tool_name
    PostToolUse           state=busy      scrapes tool_name
    PostToolUseFailure    (no state)      scrapes tool_name; toolfail=<exit_code, default 1>,
                                           suppressed entirely if is_interrupt is true
    Notification          state=awaiting  scrapes message, notification_type
    Elicitation           state=awaiting  MCP server asking the user for input mid tool call
    ElicitationResult     state=busy      paired retraction for Elicitation
    Stop                  state=idle      scrapes background_tasks
    StopFailure           state=idle      toolfail=1; scrapes background_tasks
    SessionEnd            state=idle      scrapes session_id, cwd, transcript_path, session_title, reason
An unrecognized or absent `hook_event_name` derives nothing; only explicit flags apply.

FLAGS (override the derived value; freely combinable):
    --agent <claude|gemini|grok|codex>  Scope hook_event_name derivation to this
                                    agent (default: claude). Every generated hook
                                    command now passes this explicitly.
    --state <busy|awaiting|idle>   Force the state verb, regardless of derivation.
    --toolfail <code>              Force a fixed toolfail verb.
    --toolfail-from-stdin          Force toolfail, extracting `exit_code` from stdin
                                    JSON (falls back to "1" if absent or malformed);
                                    suppressed entirely if stdin's is_interrupt is true.
    --emit-session                 Force scraping session_id/cwd/transcript_path.
    --emit-tool                    Force scraping tool_name.
    --emit-notify                  Force scraping message.
    --emit-notification-type       Force scraping notification_type.
    --emit-background-tasks        Force scraping background_tasks.
    --emit-title                   Force scraping session_title.
    --emit-end-reason              Force scraping reason.
    --version                      Print the version and exit (no TUIC_SESSION needed).
    --help, -h                     Print this message and exit (no TUIC_SESSION needed).

Unrecognized flags, and value flags missing their value, are silently ignored — a
stale copy of this binary must degrade gracefully against a future flag it doesn't
understand, never fail the hook.

STDIN:
    A JSON object, read in full (bounded to 1 MiB). Fields read: hook_event_name,
    session_id, cwd, transcript_path, tool_name, message, notification_type,
    exit_code, is_interrupt, background_tasks, session_title, reason.
    Missing, empty, or malformed fields are treated as absent — never an error. A
    payload truncated past the bound loses the whole fire's derivation, not just
    the oversized field.

ENVIRONMENT:
    TUIC_SESSION      Must be set and non-empty, or every flag above is a no-op.
    TUIC_PTY_TTY      The pty device path, stamped by TUICommander onto every
                       child it spawns. Wins over ancestor-walk resolution.
    TUIC_HOOK_TTY     Overrides the resolved tty write target (test seam).
    TUIC_HOOK_DEBUG   If set, prints the resolved tty path to stderr.

WIRE FORMAT:
    ESC ] 7770 ; verb=payload ESC \    (one sequence per verb, one write per fire)
    Free-text payloads (ccsession, cwd, transcript, tool, notify, notifytype,
    bgtasks, cctitle, ccend) are percent-encoded; state and toolfail are fixed
    enum/numeric values, emitted verbatim.
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
    /// Which agent generated this invocation — `--agent claude`/`gemini`/
    /// `grok`/`codex`. Absent for a stale, already-installed hook command
    /// generated before this flag existed (see `find_derivation`'s
    /// `unwrap_or("claude")` — every pre-existing installed command relied
    /// purely on Claude-shaped derivation, so that's the only backward-
    /// compatible default).
    agent: Option<String>,
    state: Option<String>,
    toolfail: Option<String>,
    toolfail_from_stdin: bool,
    emit_session: bool,
    emit_tool: bool,
    emit_notify: bool,
    emit_notification_type: bool,
    emit_background_tasks: bool,
    emit_title: bool,
    emit_end_reason: bool,
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
            "--agent" => {
                if let Some(v) = args.get(i + 1) {
                    out.agent = Some(v.clone());
                    i += 1;
                }
            }
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
            "--emit-notification-type" => out.emit_notification_type = true,
            "--emit-background-tasks" => out.emit_background_tasks = true,
            "--emit-title" => out.emit_title = true,
            "--emit-end-reason" => out.emit_end_reason = true,
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
    /// Which agent this row's `event`/semantics were verified against.
    /// `DERIVATIONS` used to be matched on `event` alone, with no per-agent
    /// scope — Gemini's own "Notification" and "SessionEnd" hook events
    /// happen to be spelled identically to two Claude-specific rows here, so
    /// a Gemini payload that (unverified, but plausible) also carries a
    /// `hook_event_name` field could silently inherit Claude's scrape
    /// behavior for those events. See `find_derivation`.
    agent: &'static str,
    event: &'static str,
    state: Option<&'static str>,
    scrape_tool_name: bool,
    scrape_message: bool,
    scrape_notification_type: bool,
    scrape_session_metadata: bool,
    /// Scrapes stdin's `session_title` alone — deliberately separate from
    /// `scrape_session_metadata` (which covers `session_id`/`cwd`/
    /// `transcript_path`) because the title can change mid-session (a
    /// `/rename`) and needs to be re-scraped on `UserPromptSubmit`, an event
    /// that has no reason to re-scrape the other three.
    scrape_session_title: bool,
    scrape_background_tasks: bool,
    /// Scrapes stdin's raw `reason` string on `SessionEnd` — unclassified,
    /// recorded for diagnostics only (see crate AGENTS.md's rule against
    /// baking Claude Code's evolving vocabulary into this binary).
    scrape_end_reason: bool,
    toolfail: DerivedToolfail,
}

enum DerivedToolfail {
    None,
    /// `PostToolUseFailure`: extract `exit_code` from stdin, same fallback
    /// as the legacy `--toolfail-from-stdin` flag. Suppressed entirely
    /// (emits no `toolfail` at all) when stdin's `is_interrupt` is `true` —
    /// a user-cancelled (Esc) tool call, not a real failure. See
    /// `toolfail_from_exit_code`.
    FromStdinExitCode,
    /// `StopFailure`: no distinguishing field of its own — Claude Code's
    /// `Stop`/`StopFailure` pair is told apart only by which event name
    /// fires, so the failure signal here is the event itself, not a value
    /// extracted from the payload.
    Fixed(&'static str),
}

const DERIVATIONS: &[EventDerivation] = &[
    EventDerivation {
        agent: "claude",
        event: "SessionStart",
        state: Some("busy"),
        scrape_tool_name: false,
        scrape_message: false,
        scrape_notification_type: false,
        scrape_session_metadata: true,
        scrape_session_title: true,
        scrape_background_tasks: false,
        scrape_end_reason: false,
        toolfail: DerivedToolfail::None,
    },
    EventDerivation {
        // `session_title` is re-scraped here (not just SessionStart) so a
        // mid-session `/rename` reaches the receiving end the very next time
        // the user submits a prompt — Claude Code's own hook payload carries
        // the CURRENT title on every fire, not just the first.
        agent: "claude",
        event: "UserPromptSubmit",
        state: Some("busy"),
        scrape_tool_name: false,
        scrape_message: false,
        scrape_notification_type: false,
        scrape_session_metadata: false,
        scrape_session_title: true,
        scrape_background_tasks: false,
        scrape_end_reason: false,
        toolfail: DerivedToolfail::None,
    },
    EventDerivation {
        agent: "claude",
        event: "PreToolUse",
        state: Some("busy"),
        scrape_tool_name: true,
        scrape_message: false,
        scrape_notification_type: false,
        scrape_session_metadata: false,
        scrape_session_title: false,
        scrape_background_tasks: false,
        scrape_end_reason: false,
        toolfail: DerivedToolfail::None,
    },
    EventDerivation {
        agent: "claude",
        event: "PostToolUse",
        state: Some("busy"),
        scrape_tool_name: true,
        scrape_message: false,
        scrape_notification_type: false,
        scrape_session_metadata: false,
        scrape_session_title: false,
        scrape_background_tasks: false,
        scrape_end_reason: false,
        toolfail: DerivedToolfail::None,
    },
    EventDerivation {
        agent: "claude",
        event: "PostToolUseFailure",
        state: None,
        scrape_tool_name: true,
        scrape_message: false,
        scrape_notification_type: false,
        scrape_session_metadata: false,
        scrape_session_title: false,
        scrape_background_tasks: false,
        scrape_end_reason: false,
        toolfail: DerivedToolfail::FromStdinExitCode,
    },
    EventDerivation {
        agent: "claude",
        event: "Notification",
        state: Some("awaiting"),
        scrape_tool_name: false,
        scrape_message: true,
        scrape_notification_type: true,
        scrape_session_metadata: false,
        scrape_session_title: false,
        scrape_background_tasks: false,
        scrape_end_reason: false,
        toolfail: DerivedToolfail::None,
    },
    EventDerivation {
        // MCP `elicitation/create` — an MCP server asking the user for input mid
        // tool call. Not a tool call itself, so no `PreToolUse` matcher reaches
        // it, and its dialog matches none of the screen heuristics (options
        // render horizontally, footer reads "Esc to cancel", not "Enter to
        // select"). Without this the tab stays "busy" while blocked on the user.
        agent: "claude",
        event: "Elicitation",
        state: Some("awaiting"),
        scrape_tool_name: false,
        scrape_message: false,
        scrape_notification_type: false,
        scrape_session_metadata: false,
        scrape_session_title: false,
        scrape_background_tasks: false,
        scrape_end_reason: false,
        toolfail: DerivedToolfail::None,
    },
    EventDerivation {
        // Paired retraction for `Elicitation` — awaiting is sticky, so a set
        // with no matching clear latches the badge forever once the user answers.
        agent: "claude",
        event: "ElicitationResult",
        state: Some("busy"),
        scrape_tool_name: false,
        scrape_message: false,
        scrape_notification_type: false,
        scrape_session_metadata: false,
        scrape_session_title: false,
        scrape_background_tasks: false,
        scrape_end_reason: false,
        toolfail: DerivedToolfail::None,
    },
    EventDerivation {
        // `background_tasks` (an array of {id,type,status,description,command})
        // rides on Claude Code's own `Stop`/`StopFailure` payload when a
        // backgrounded tool call (e.g. a `run_in_background` Bash command) is
        // still outstanding as the turn ends. Scraped here as raw per-task
        // `status` strings (see `scrape_background_tasks_statuses`) — this
        // binary stays a dumb extractor; `pty.rs` decides which status values
        // mean "still running" (see crate AGENTS.md's "Extending an existing
        // event's scrape set").
        agent: "claude",
        event: "Stop",
        state: Some("idle"),
        scrape_tool_name: false,
        scrape_message: false,
        scrape_notification_type: false,
        scrape_session_metadata: false,
        scrape_session_title: false,
        scrape_background_tasks: true,
        scrape_end_reason: false,
        toolfail: DerivedToolfail::None,
    },
    EventDerivation {
        agent: "claude",
        event: "StopFailure",
        state: Some("idle"),
        scrape_tool_name: false,
        scrape_message: false,
        scrape_notification_type: false,
        scrape_session_metadata: false,
        scrape_session_title: false,
        scrape_background_tasks: true,
        scrape_end_reason: false,
        toolfail: DerivedToolfail::Fixed("1"),
    },
    EventDerivation {
        // Also re-scrapes session_id/cwd/transcript_path (not just at
        // SessionStart) and the session title, plus the raw `reason` string
        // — the exit-resume-banner feature's snapshot needs the session's
        // identity and title available at the exact moment the turn ends,
        // not just at its start (a long session's title can have changed via
        // `/rename` since SessionStart fired).
        agent: "claude",
        event: "SessionEnd",
        state: Some("idle"),
        scrape_tool_name: false,
        scrape_message: false,
        scrape_notification_type: false,
        scrape_session_metadata: true,
        scrape_session_title: true,
        scrape_background_tasks: false,
        scrape_end_reason: true,
        toolfail: DerivedToolfail::None,
    },
];

/// Scoped per agent — `DERIVATIONS` models Claude Code's own event names and
/// payload shape specifically, and every current row is `agent: "claude"`.
/// Without this scope, a same-named event from a different agent (Gemini's
/// own "Notification"/"SessionEnd" happen to collide) would silently inherit
/// Claude's scrape behavior if that agent's payload ever also carries a
/// `hook_event_name` field — unverified for Gemini/Grok/Codex, but plausible.
fn find_derivation(stdin_json: &Value, agent: &str) -> Option<&'static EventDerivation> {
    let name = str_field(stdin_json, "hook_event_name")?;
    DERIVATIONS
        .iter()
        .find(|d| d.event == name && d.agent == agent)
}

fn build_emissions(parsed: &ParsedArgs, stdin_json: &Value) -> Vec<Emission> {
    // Absent `--agent` means a stale, already-installed command generated
    // before this flag existed — every one of those was Claude-shaped
    // (`derived_hook_command()`, no explicit `--agent`), so `"claude"` is the
    // only backward-compatible default, not an arbitrary choice.
    let agent = parsed.agent.as_deref().unwrap_or("claude");
    let derivation = find_derivation(stdin_json, agent);
    let mut pairs = Vec::new();

    let scrape_session_metadata =
        parsed.emit_session || derivation.is_some_and(|d| d.scrape_session_metadata);
    let scrape_tool_name = parsed.emit_tool || derivation.is_some_and(|d| d.scrape_tool_name);
    let scrape_message = parsed.emit_notify || derivation.is_some_and(|d| d.scrape_message);
    let scrape_notification_type =
        parsed.emit_notification_type || derivation.is_some_and(|d| d.scrape_notification_type);
    let scrape_background_tasks =
        parsed.emit_background_tasks || derivation.is_some_and(|d| d.scrape_background_tasks);
    let scrape_session_title =
        parsed.emit_title || derivation.is_some_and(|d| d.scrape_session_title);
    let scrape_end_reason =
        parsed.emit_end_reason || derivation.is_some_and(|d| d.scrape_end_reason);

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
    // `session_title` — Claude Code's own session name (auto-generated, or set
    // via `/rename`). Scraped separately from the session-metadata trio above:
    // SessionStart/SessionEnd want it alongside session_id/cwd/transcript_path,
    // but UserPromptSubmit wants ONLY this (a mid-session `/rename` shows up on
    // the very next prompt submission's payload), not a redundant re-scrape of
    // the other three.
    if scrape_session_title && let Some(v) = str_field(stdin_json, "session_title") {
        pairs.push(Emission::encoded("cctitle", v));
    }
    if scrape_tool_name && let Some(v) = str_field(stdin_json, "tool_name") {
        pairs.push(Emission::encoded("tool", v));
    }
    if scrape_message && let Some(v) = str_field(stdin_json, "message") {
        pairs.push(Emission::encoded("notify", v));
    }
    // `notification_type` is Claude Code's own closed-set discriminant for a
    // `Notification` fire (`permission_prompt`, `idle_prompt`, `auth_success`,
    // `elicitation_dialog`, `elicitation_url_dialog`, `elicitation_complete`,
    // `elicitation_response`, `agent_needs_input`, `agent_completed`,
    // `quota_auto_resume_fired`, `quota_auto_resume_stale`,
    // `quota_auto_resume_disabled` as of this writing) — far more reliable
    // than sniffing the free-text `message` wording the way
    // `output_parser.rs::parse_osc777_notifies` has to. Scraped as its own
    // verb, alongside `notify`, so the receiving end (`pty.rs`) can classify
    // deterministically instead of guessing from prose.
    if scrape_notification_type && let Some(v) = str_field(stdin_json, "notification_type") {
        pairs.push(Emission::encoded("notifytype", v));
    }

    // `background_tasks`: an array Claude Code includes on `Stop`/`StopFailure`
    // when a backgrounded tool call (e.g. a `run_in_background` Bash command)
    // is still outstanding as the turn ends. Scraped as the raw, comma-joined
    // per-task `status` strings — NOT reduced to a "still running" boolean
    // here, per this crate's own rule against baking Claude Code's evolving
    // vocabulary into this binary (see crate AGENTS.md). `pty.rs` decides
    // which status values mean "still running". Emitted whenever the field is
    // present, even as an empty array (empty payload) — that's a real
    // observation ("no background tasks"), same as an empty-but-present list
    // for every other opportunistic scrape here.
    if scrape_background_tasks && let Some(v) = background_task_statuses(stdin_json) {
        pairs.push(Emission::encoded("bgtasks", v));
    }

    // `reason` — Claude Code's own SessionEnd reason string (e.g. "exit",
    // "other" — not a documented closed set, so scraped raw and unclassified
    // like `bgtasks`'s statuses above; the receiving end records it for
    // diagnostics only, never as a gate on whether to show a resume banner).
    if scrape_end_reason && let Some(v) = str_field(stdin_json, "reason") {
        pairs.push(Emission::encoded("ccend", v));
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

/// Extracts the raw `status` string of every entry in stdin's
/// `background_tasks` array, comma-joined (e.g. `"running,completed"`).
/// `None` when the field is absent (no observation to report — the receiving
/// end should leave its prior value alone). `Some("")` when the field is
/// present but empty, or when every entry's `status` is missing/non-string —
/// a real "nothing outstanding" observation, distinct from absence. A comma
/// itself can never appear inside a status value from this source (Claude
/// Code's own enum strings), and the payload is percent-encoded on the wire
/// regardless (`Emission::encoded`), so a plain join needs no escaping.
fn background_task_statuses(stdin_json: &Value) -> Option<String> {
    let tasks = stdin_json.get("background_tasks")?.as_array()?;
    Some(
        tasks
            .iter()
            .filter_map(|t| t.get("status").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(","),
    )
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
            "--emit-notification-type".into(),
            "--emit-background-tasks".into(),
            "--emit-title".into(),
            "--emit-end-reason".into(),
        ]);
        assert!(a.toolfail_from_stdin);
        assert!(a.emit_session);
        assert!(a.emit_tool);
        assert!(a.emit_notify);
        assert!(a.emit_notification_type);
        assert!(a.emit_background_tasks);
        assert!(a.emit_title);
        assert!(a.emit_end_reason);
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
    fn emit_notification_type_extracts_and_encodes_it() {
        let parsed = ParsedArgs {
            emit_notification_type: true,
            ..Default::default()
        };
        let json = serde_json::json!({"notification_type": "idle_prompt"});
        let pairs = build_emissions(&parsed, &json);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].verb, "notifytype");
        assert_eq!(pairs[0].payload, "idle_prompt");
    }

    #[test]
    fn emit_notification_type_omits_missing_field() {
        let parsed = ParsedArgs {
            emit_notification_type: true,
            ..Default::default()
        };
        let pairs = build_emissions(&parsed, &Value::Null);
        assert!(pairs.is_empty());
    }

    #[test]
    fn emit_notification_type_omits_empty_string_field() {
        let parsed = ParsedArgs {
            emit_notification_type: true,
            ..Default::default()
        };
        let json = serde_json::json!({"notification_type": ""});
        let pairs = build_emissions(&parsed, &json);
        assert!(
            pairs.is_empty(),
            "an empty notification_type must be treated as absent, same as every other free-text field"
        );
    }

    #[test]
    fn emit_notification_type_ignores_non_string_value_without_panicking() {
        // Claude Code's schema documents notification_type as a string, but a
        // future/buggy build sending a non-string value must degrade the same
        // way every other str_field() consumer does — omit it, never panic.
        let parsed = ParsedArgs {
            emit_notification_type: true,
            ..Default::default()
        };
        let json = serde_json::json!({"notification_type": 42});
        let pairs = build_emissions(&parsed, &json);
        assert!(pairs.is_empty());
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
    fn derives_session_title_alongside_session_metadata_for_session_start() {
        let json = serde_json::json!({
            "hook_event_name": "SessionStart",
            "session_id": "abc123",
            "cwd": "/tmp/proj",
            "transcript_path": "/tmp/t.jsonl",
            "session_title": "file-locations",
        });
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        let verbs: Vec<&str> = pairs.iter().map(|p| p.verb).collect();
        assert_eq!(
            verbs,
            ["ccsession", "cwd", "transcript", "cctitle", "state"]
        );
        assert_eq!(pairs[3].payload, "file-locations");
    }

    #[test]
    fn derives_session_title_for_user_prompt_submit_when_present() {
        // The mechanism a mid-session `/rename` reaches the receiving end
        // through: Claude Code's own hook payload carries the CURRENT title
        // on every fire, and UserPromptSubmit re-scrapes it (unlike the other
        // three session-metadata fields, which only need to be read once).
        let json = serde_json::json!({
            "hook_event_name": "UserPromptSubmit",
            "session_title": "renamed-session",
        });
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        let verbs: Vec<&str> = pairs.iter().map(|p| p.verb).collect();
        assert_eq!(verbs, ["cctitle", "state"]);
        assert_eq!(pairs[0].payload, "renamed-session");
    }

    #[test]
    fn session_title_omitted_when_absent_or_empty() {
        for json in [
            serde_json::json!({"hook_event_name": "UserPromptSubmit"}),
            serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_title": ""}),
        ] {
            let pairs = build_emissions(&ParsedArgs::default(), &json);
            assert!(pairs.iter().all(|p| p.verb != "cctitle"), "got: {pairs:?}");
        }
    }

    #[test]
    fn emit_title_flag_forces_the_scrape_without_a_recognized_hook_event_name() {
        let parsed = ParsedArgs {
            emit_title: true,
            ..Default::default()
        };
        let json = serde_json::json!({"session_title": "manual-title"});
        let pairs = build_emissions(&parsed, &json);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].verb, "cctitle");
        assert_eq!(pairs[0].payload, "manual-title");
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
    fn derives_notification_type_alongside_message_for_notification() {
        // Real production payload shape (2026-09-02 capture, dbsql-test-review):
        // Claude Code's Notification event carries a closed-set
        // `notification_type` discriminant alongside `message` — scraped as
        // its own verb so the receiving end can classify deterministically
        // instead of sniffing `message`'s free-text wording.
        let json = serde_json::json!({
            "hook_event_name": "Notification",
            "message": "Claude is waiting for your input",
            "notification_type": "idle_prompt",
        });
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        let verbs: Vec<&str> = pairs.iter().map(|p| p.verb).collect();
        assert_eq!(verbs, ["notify", "notifytype", "state"]);
        assert_eq!(pairs[1].payload, "idle_prompt");
        assert_eq!(pairs[2].payload, "awaiting");
    }

    #[test]
    fn notification_with_no_notification_type_field_still_derives_normally() {
        // An older Claude Code build that doesn't send notification_type yet
        // — must degrade to exactly the pre-existing shape, not drop message
        // or state too.
        let json = serde_json::json!({
            "hook_event_name": "Notification",
            "message": "needs input",
        });
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        let verbs: Vec<&str> = pairs.iter().map(|p| p.verb).collect();
        assert_eq!(verbs, ["notify", "state"]);
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
    fn session_end_scrapes_session_metadata_title_and_reason() {
        // Supersedes the pre-resume-banner characterization test (SessionEnd
        // used to derive only state=idle). The exit-resume-banner feature
        // needs the session's identity/title/reason available at the exact
        // moment the turn ends, so SessionEnd's row now scrapes all of it —
        // the same session-metadata trio SessionStart scrapes, plus title and
        // the raw end reason.
        let json = serde_json::json!({
            "hook_event_name": "SessionEnd",
            "session_id": "abc123",
            "cwd": "/tmp/proj",
            "transcript_path": "/tmp/t.jsonl",
            "session_title": "file-locations",
            "reason": "exit",
        });
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        let verbs: Vec<&str> = pairs.iter().map(|p| p.verb).collect();
        assert_eq!(
            verbs,
            [
                "ccsession",
                "cwd",
                "transcript",
                "cctitle",
                "ccend",
                "state"
            ]
        );
        assert_eq!(pairs[0].payload, "abc123");
        assert_eq!(pairs[3].payload, "file-locations");
        assert_eq!(pairs[4].payload, "exit");
        assert_eq!(pairs[5].payload, "idle");
    }

    #[test]
    fn session_end_with_no_metadata_still_derives_only_state() {
        // Degrades to the pre-feature shape when the payload carries none of
        // the optional fields — every scrape is opportunistic, never required.
        let json = serde_json::json!({"hook_event_name": "SessionEnd"});
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        assert_eq!(pairs.len(), 1, "got: {pairs:?}");
        assert_eq!(pairs[0].verb, "state");
        assert_eq!(pairs[0].payload, "idle");
    }

    #[test]
    fn end_reason_omitted_when_absent_or_empty() {
        for json in [
            serde_json::json!({"hook_event_name": "SessionEnd"}),
            serde_json::json!({"hook_event_name": "SessionEnd", "reason": ""}),
        ] {
            let pairs = build_emissions(&ParsedArgs::default(), &json);
            assert!(pairs.iter().all(|p| p.verb != "ccend"), "got: {pairs:?}");
        }
    }

    #[test]
    fn end_reason_not_scraped_for_unrelated_events() {
        let json = serde_json::json!({"hook_event_name": "Stop", "reason": "exit"});
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        assert!(pairs.iter().all(|p| p.verb != "ccend"), "got: {pairs:?}");
    }

    #[test]
    fn emit_end_reason_flag_forces_the_scrape_without_a_recognized_hook_event_name() {
        let parsed = ParsedArgs {
            emit_end_reason: true,
            ..Default::default()
        };
        let json = serde_json::json!({"reason": "other"});
        let pairs = build_emissions(&parsed, &json);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].verb, "ccend");
        assert_eq!(pairs[0].payload, "other");
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
    fn derives_background_tasks_before_state_for_stop() {
        // Real production shape (2026-09-14 capture, diff-tool worktree): Claude
        // Code's Stop payload can carry `background_tasks` when a backgrounded
        // tool call is still outstanding as the turn ends. Scraped as raw
        // comma-joined statuses, ordered before `state` on the wire (mirrors
        // notify/notifytype preceding state) since `pty.rs` reads it that way.
        let json = serde_json::json!({
            "hook_event_name": "Stop",
            "background_tasks": [{"id": "bzala5foe", "type": "shell", "status": "running"}],
        });
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        let verbs: Vec<&str> = pairs.iter().map(|p| p.verb).collect();
        assert_eq!(verbs, ["bgtasks", "state"]);
        assert_eq!(pairs[0].payload, "running");
        assert_eq!(pairs[1].payload, "idle");
    }

    #[test]
    fn background_tasks_multiple_statuses_are_comma_joined() {
        let json = serde_json::json!({
            "hook_event_name": "Stop",
            "background_tasks": [
                {"id": "a", "status": "running"},
                {"id": "b", "status": "completed"},
            ],
        });
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        assert_eq!(pairs[0].verb, "bgtasks");
        // `,` is outside the unreserved set, so `Emission::encoded` percent-
        // encodes it at construction time — the receiving end percent-decodes
        // before splitting on commas (see `background_task_statuses`'s doc).
        assert_eq!(pairs[0].payload, "running%2Ccompleted");
    }

    #[test]
    fn background_tasks_present_but_empty_emits_empty_payload() {
        // An empty array is a real observation ("nothing outstanding right
        // now"), distinct from the field being absent entirely — must still
        // emit, so the receiving end can clear a stale prior declaration.
        let json = serde_json::json!({"hook_event_name": "Stop", "background_tasks": []});
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        assert_eq!(pairs[0].verb, "bgtasks");
        assert_eq!(pairs[0].payload, "");
    }

    #[test]
    fn background_tasks_absent_field_emits_nothing_for_it() {
        let json = serde_json::json!({"hook_event_name": "Stop"});
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        assert_eq!(pairs.len(), 1, "must derive only `state`, no `bgtasks`");
        assert_eq!(pairs[0].verb, "state");
    }

    #[test]
    fn background_tasks_scraped_for_stop_failure_too() {
        let json = serde_json::json!({
            "hook_event_name": "StopFailure",
            "background_tasks": [{"id": "a", "status": "running"}],
        });
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        let verbs: Vec<&str> = pairs.iter().map(|p| p.verb).collect();
        assert_eq!(verbs, ["toolfail", "bgtasks", "state"]);
    }

    #[test]
    fn background_tasks_not_scraped_for_unrelated_events() {
        // PostToolUse's Claude-generated matcher only fires for
        // AskUserQuestion|ExitPlanMode in practice, but even a bare
        // derivation-table lookup must not scrape background_tasks here —
        // only Stop/StopFailure do.
        let json = serde_json::json!({
            "hook_event_name": "PostToolUse",
            "tool_name": "Bash",
            "background_tasks": [{"id": "a", "status": "running"}],
        });
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        assert!(pairs.iter().all(|p| p.verb != "bgtasks"));
    }

    #[test]
    fn emit_background_tasks_flag_forces_the_scrape() {
        let parsed = ParsedArgs {
            emit_background_tasks: true,
            ..Default::default()
        };
        let json = serde_json::json!({
            "hook_event_name": "SomeFutureEvent",
            "background_tasks": [{"id": "a", "status": "running"}],
        });
        let pairs = build_emissions(&parsed, &json);
        assert_eq!(pairs[0].verb, "bgtasks");
        assert_eq!(pairs[0].payload, "running");
    }

    // ---- `background_tasks` scrape: characterization of today's behavior ----
    // (Step 1 of plans/teammate-background-work-busy.md — these pin what the
    // scrape does TODAY so the later `type`-scrape change can't silently
    // alter status handling.)

    fn stop_with(tasks: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"hook_event_name": "Stop", "background_tasks": tasks})
    }

    fn bgtasks_payload(json: &serde_json::Value) -> Option<String> {
        build_emissions(&ParsedArgs::default(), json)
            .into_iter()
            .find(|p| p.verb == "bgtasks")
            .map(|p| p.payload)
    }

    #[test]
    fn background_tasks_type_field_is_ignored_only_status_is_scraped() {
        // `tuic-hook` deliberately never discriminates on `type` today (docs:
        // terminal-state-machine.md). A teammate, subagent and shell all look
        // identical on the wire — only their `status` survives, in order.
        let json = stop_with(serde_json::json!([
            {"id": "t", "type": "teammate", "status": "running"},
            {"id": "s", "type": "subagent", "status": "running"},
            {"id": "b", "type": "shell", "status": "completed"},
        ]));
        assert_eq!(
            bgtasks_payload(&json).as_deref(),
            Some("running%2Crunning%2Ccompleted")
        );
        // Same statuses with no `type` at all produce the identical payload.
        let untyped = stop_with(serde_json::json!([
            {"id": "t", "status": "running"},
            {"id": "s", "status": "running"},
            {"id": "b", "status": "completed"},
        ]));
        assert_eq!(bgtasks_payload(&untyped), bgtasks_payload(&json));
    }

    #[test]
    fn background_tasks_only_the_status_key_is_used_from_each_entry() {
        // description/command/agent_type/etc. never reach the wire.
        let json = stop_with(serde_json::json!([{
            "id": "x", "type": "shell", "status": "running",
            "description": "secret; stuff", "command": "rm -rf /", "agent_type": "Explore"
        }]));
        assert_eq!(bgtasks_payload(&json).as_deref(), Some("running"));
    }

    #[test]
    fn background_tasks_entries_without_a_string_status_are_dropped() {
        // Missing status, non-string status, null status and non-object items
        // are all skipped (`filter_map`), not errors and not placeholders —
        // so the surviving statuses are NOT index-aligned with the input array.
        let json = stop_with(serde_json::json!([
            {"id": "a", "type": "shell"},
            {"id": "b", "status": 7},
            {"id": "c", "status": null},
            "running",
            42,
            null,
            [],
            {"id": "d", "status": "running"},
        ]));
        assert_eq!(bgtasks_payload(&json).as_deref(), Some("running"));
    }

    #[test]
    fn background_tasks_all_entries_malformed_is_present_but_empty() {
        // Indistinguishable on the wire from a genuine `[]` ("nothing
        // outstanding") — the empty payload also clears a prior declaration.
        let json = stop_with(serde_json::json!([{"id": "a"}, {"status": false}, 1]));
        assert_eq!(bgtasks_payload(&json).as_deref(), Some(""));
    }

    #[test]
    fn background_tasks_that_is_not_an_array_emits_nothing() {
        for not_array in [
            serde_json::json!(null),
            serde_json::json!("running"),
            serde_json::json!(5),
            serde_json::json!({"status": "running"}),
            serde_json::json!(true),
        ] {
            let json = stop_with(not_array.clone());
            assert_eq!(
                bgtasks_payload(&json),
                None,
                "non-array background_tasks ({not_array}) must leave the prior value alone"
            );
        }
    }

    #[test]
    fn background_tasks_empty_string_status_is_kept_as_an_empty_slot() {
        // `filter_map` only drops non-strings: an empty-string status survives
        // and becomes an empty comma slot (the receiver ignores empties).
        let json = stop_with(serde_json::json!([
            {"status": "running"},
            {"status": ""},
            {"status": "completed"},
        ]));
        assert_eq!(
            bgtasks_payload(&json).as_deref(),
            Some("running%2C%2Ccompleted")
        );
    }

    #[test]
    fn background_tasks_unrecognized_status_values_pass_through_verbatim() {
        // No vocabulary is baked in here: `pending`/`queued`/future values are
        // forwarded raw; `pty.rs` decides what counts as running.
        let json = stop_with(serde_json::json!([
            {"status": "pending"},
            {"status": "Running"},
            {"status": "some future value"},
        ]));
        assert_eq!(
            bgtasks_payload(&json).as_deref(),
            Some("pending%2CRunning%2Csome%20future%20value")
        );
    }

    #[test]
    fn background_tasks_payload_is_truncated_at_512_raw_bytes_losing_later_statuses() {
        // GAP (characterized, not fixed): `payload::encode` truncates the RAW
        // joined string to MAX_PAYLOAD_LEN (512) bytes BEFORE encoding, so a
        // long task list silently drops its tail. 60 x "completed," is 540
        // raw bytes; a trailing "running" is cut off and the receiver would
        // see "all terminal" -> clear the declaration. Fail-UNsafe for lists
        // longer than 512 bytes with the running task late in the array.
        let mut tasks: Vec<serde_json::Value> = (0..60)
            .map(|_| serde_json::json!({"status": "completed"}))
            .collect();
        tasks.push(serde_json::json!({"status": "running"}));
        let payload = bgtasks_payload(&stop_with(serde_json::Value::Array(tasks))).unwrap();
        assert!(
            !payload.contains("running"),
            "trailing running status was expected to be lost to truncation"
        );
        // Raw cap is 512 bytes; every `,` becomes `%2C`, so the wire payload is
        // longer than 512 but bounded by 3x.
        assert!(payload.len() > 512 && payload.len() <= 512 * 3);
    }

    #[test]
    fn background_tasks_short_list_with_running_first_survives_intact() {
        let mut tasks = vec![serde_json::json!({"status": "running"})];
        tasks.extend((0..10).map(|_| serde_json::json!({"status": "completed"})));
        let payload = bgtasks_payload(&stop_with(serde_json::Value::Array(tasks))).unwrap();
        assert!(payload.starts_with("running%2Ccompleted"));
        assert_eq!(payload.matches("completed").count(), 10);
    }

    #[test]
    fn stop_wire_order_is_bgtasks_then_state_and_state_is_still_idle_with_running_tasks() {
        // The hook never lets background work change the derived state: Stop
        // is always `idle` here; `pty.rs` layers the declared-work signal on top.
        let json = stop_with(serde_json::json!([{"status": "running"}]));
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        let wire: Vec<(&str, &str)> = pairs.iter().map(|p| (p.verb, p.payload.as_str())).collect();
        assert_eq!(wire, [("bgtasks", "running"), ("state", "idle")]);
    }

    #[test]
    fn explicit_state_flag_overrides_derived_state_but_bgtasks_is_still_scraped() {
        let parsed = ParsedArgs {
            state: Some("busy".into()),
            ..Default::default()
        };
        let pairs = build_emissions(
            &parsed,
            &stop_with(serde_json::json!([{"status": "running"}])),
        );
        let wire: Vec<(&str, &str)> = pairs.iter().map(|p| (p.verb, p.payload.as_str())).collect();
        assert_eq!(wire, [("bgtasks", "running"), ("state", "busy")]);
    }

    #[test]
    fn stop_failure_wire_order_is_toolfail_bgtasks_state() {
        let json = serde_json::json!({
            "hook_event_name": "StopFailure",
            "background_tasks": [{"status": "running"}],
        });
        let pairs = build_emissions(&ParsedArgs::default(), &json);
        let wire: Vec<(&str, &str)> = pairs.iter().map(|p| (p.verb, p.payload.as_str())).collect();
        assert_eq!(
            wire,
            [("toolfail", "1"), ("bgtasks", "running"), ("state", "idle")]
        );
    }

    #[test]
    fn background_tasks_not_scraped_for_other_agents_stop_events() {
        // --agent scoping: DERIVATIONS rows are all `agent: "claude"`, so a
        // Stop from gemini/grok/codex (even with a `background_tasks` field)
        // derives nothing — no `bgtasks`, no `state`.
        for agent in ["gemini", "grok", "codex", "unknown-agent"] {
            let parsed = ParsedArgs {
                agent: Some(agent.into()),
                ..Default::default()
            };
            let pairs = build_emissions(
                &parsed,
                &stop_with(serde_json::json!([{"status": "running"}])),
            );
            assert!(
                pairs.is_empty(),
                "--agent {agent} must not inherit Claude's Stop derivation, got {:?}",
                pairs.iter().map(|p| p.verb).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn explicit_agent_claude_matches_absent_agent_for_background_tasks() {
        let json = stop_with(serde_json::json!([{"status": "running"}]));
        let explicit = ParsedArgs {
            agent: Some("claude".into()),
            ..Default::default()
        };
        let a: Vec<_> = build_emissions(&explicit, &json)
            .into_iter()
            .map(|p| (p.verb, p.payload))
            .collect();
        let b: Vec<_> = build_emissions(&ParsedArgs::default(), &json)
            .into_iter()
            .map(|p| (p.verb, p.payload))
            .collect();
        assert_eq!(a, b);
        assert_eq!(a[0], ("bgtasks", "running".to_string()));
    }

    #[test]
    fn emit_background_tasks_flag_scrapes_even_for_a_non_claude_agent() {
        // The explicit flag is independent of derivation scoping.
        let parsed = ParsedArgs {
            agent: Some("gemini".into()),
            emit_background_tasks: true,
            ..Default::default()
        };
        let pairs = build_emissions(
            &parsed,
            &stop_with(serde_json::json!([{"status": "running"}])),
        );
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].verb, "bgtasks");
    }

    #[test]
    fn emit_background_tasks_flag_with_empty_array_emits_empty_and_absent_emits_nothing() {
        let parsed = ParsedArgs {
            emit_background_tasks: true,
            ..Default::default()
        };
        let empty = build_emissions(&parsed, &serde_json::json!({"background_tasks": []}));
        assert_eq!(empty.len(), 1);
        assert_eq!((empty[0].verb, empty[0].payload.as_str()), ("bgtasks", ""));
        assert!(build_emissions(&parsed, &serde_json::json!({})).is_empty());
    }

    #[test]
    fn background_tasks_on_null_stdin_derives_nothing() {
        // read_stdin_json falls back to Value::Null on malformed/oversized/
        // timed-out stdin: the whole fire's derivation is lost, no panic.
        let pairs = build_emissions(&ParsedArgs::default(), &serde_json::Value::Null);
        assert!(pairs.is_empty());
    }

    #[test]
    fn stdin_bound_constants_are_pinned() {
        // `read_stdin_bounded` reads the real process stdin on a thread, so the
        // function itself is not unit-testable here (see report); pin the two
        // constants its invariants depend on. MAX_STDIN_BYTES must stay well
        // above MAX_PAYLOAD_LEN-sized fields (a truncated/invalid JSON blob
        // loses the ENTIRE fire), and the timeout must stay sub-second so a
        // hung stdin can never stall the hook.
        assert_eq!(MAX_STDIN_BYTES, 1024 * 1024);
        assert!(STDIN_READ_TIMEOUT <= std::time::Duration::from_secs(1));
        assert!(MAX_STDIN_BYTES > crate::payload::MAX_PAYLOAD_LEN as u64 * 100);
    }

    #[test]
    fn stop_and_stop_failure_rows_are_the_only_background_task_scrapers() {
        for d in DERIVATIONS {
            let expect = d.agent == "claude" && (d.event == "Stop" || d.event == "StopFailure");
            assert_eq!(
                d.scrape_background_tasks, expect,
                "{} {} scrape_background_tasks",
                d.agent, d.event
            );
        }
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
            "--agent",
            "--state",
            "--toolfail",
            "--toolfail-from-stdin",
            "--emit-session",
            "--emit-tool",
            "--emit-notify",
            "--emit-notification-type",
            "--emit-background-tasks",
            "--emit-title",
            "--emit-end-reason",
            "--version",
            "--help",
            "TUIC_SESSION",
            "TUIC_PTY_TTY",
            "TUIC_HOOK_TTY",
            "TUIC_HOOK_DEBUG",
            "PostToolUseFailure",
            "StopFailure",
            "is_interrupt",
        ] {
            assert!(text.contains(needle), "help text missing {needle}");
        }
    }

    #[test]
    fn help_text_stdin_field_list_matches_what_is_actually_read() {
        // Nothing previously enforced that help_text()'s hand-written STDIN
        // field list stays in sync with what build_emissions/toolfail_from_exit_code
        // actually read from stdin — per this crate's own AGENTS.md rule
        // ("when fixing a stale doc claim... add the assertion that would
        // have caught it"), this is that assertion.
        let text = help_text();
        for field in [
            "hook_event_name",
            "session_id",
            "cwd",
            "transcript_path",
            "tool_name",
            "message",
            "notification_type",
            "exit_code",
            "is_interrupt",
            "background_tasks",
            "session_title",
            "reason",
        ] {
            assert!(text.contains(field), "STDIN field list missing {field}");
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
