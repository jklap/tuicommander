//! Native-agent hook command generation (emit side of hook-based agent state).
//!
//! Each supported agent (Claude, Gemini, …) drives its busy/idle/awaiting state
//! by running a small shell hook that emits `OSC 7770;state=…` to its controlling
//! tty — Claude also emits `OSC 7770;toolfail=…` on failure-path hook events
//! (`PostToolUseFailure`/`StopFailure`) to flag the turn-level command block's
//! exit code (see `state.rs::turn_error_flags`). This module generates those
//! hook commands and the per-agent event→state maps the installer (see
//! `agent_hook_installer`) writes into the agent's settings file.
//!
//! The command is inert outside TUIC (guarded on `TUIC_SESSION`), resolves the
//! controlling tty from a context where stdout is captured by the agent
//! (`ps -o tty= -p $PPID`, validated end-to-end by the injection spike, story
//! 042), and always exits 0 so a hook can never block the agent. Ownership is a
//! trailing shell-comment sentinel so the installer prunes only TUIC's entries
//! and never touches user/wiz hooks.

/// Trailing shell comment marking a hook command as TUIC-owned. The installer
/// keys ownership off this — a valid comment in Claude/Gemini/Codex command
/// fields alike.
pub(crate) const SENTINEL: &str = "# tuic-managed-hook";

/// A single hook registration: `(event, matcher, command)`.
/// `matcher == ""` means "all" (no tool-name filter).
pub(crate) type HookEntry = (&'static str, &'static str, String);

/// Resolve the controlling tty into `$__t`, even when the caller's stdout is
/// captured (hooks have no controlling tty of their own — read the parent's).
fn tty_resolve() -> &'static str {
    r#"__t=$(ps -o tty= -p "$PPID" 2>/dev/null|tr -d '[:space:]');case "$__t" in *[0-9]*)__t="/dev/${__t#/dev/}";;*)__t="/dev/tty";;esac"#
}

/// Generate the guarded, self-contained shell command that emits
/// `OSC 7770;state=<state>` to the controlling tty. Inert outside TUIC, always
/// exits 0, ends with the ownership sentinel.
pub(crate) fn hook_command(state: &str) -> String {
    hook_command_multi(&[("state", state)])
}

/// Like `hook_command`, but emits multiple OSC 7770 `verb=payload` pairs from
/// one guarded shell snippet (tty resolved once, reused for every printf).
/// Needed where a single CC hook event must drive two independent signals —
/// e.g. `StopFailure` must both transition the session to idle (like `Stop`)
/// *and* flag the turn as failed (like `PostToolUseFailure`), which a bare
/// `state=idle` can't distinguish from an ordinary `Stop`: both events would
/// otherwise emit an identical wire payload.
fn hook_command_multi(pairs: &[(&str, &str)]) -> String {
    // Defensive ordering, not just caller discipline: `handle_tuic_state`
    // (pty.rs) reads-and-clears `turn_error_flags` at the exact moment it
    // processes a `state=idle` event, so any `toolfail` pair in the same
    // call MUST be emitted first — regardless of what order the caller
    // listed the pairs in. Without this, a future edit that lists
    // `("state", "idle")` before `("toolfail", ...)` would compile, pass a
    // casual review, and silently lose the failure flag for that hook.
    let (toolfail, rest): (Vec<_>, Vec<_>) =
        pairs.iter().partition(|(verb, _)| *verb == "toolfail");
    let printfs: String = toolfail
        .into_iter()
        .chain(rest)
        .map(|(verb, payload)| format!(r#"printf '\033]7770;{verb}={payload}\033\\' > "$__t";"#))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        r#"[ -n "${{TUIC_SESSION:-}}" ] && {{ {tty}; {printfs} }} >/dev/null 2>&1 || true {SENTINEL}"#,
        tty = tty_resolve(),
    )
}

/// Generate the hook command for `PostToolUseFailure`: on tool failure only,
/// CC pipes a JSON payload (containing `exit_code`, `stderr`, `stdout`) on
/// stdin — unlike `PostToolUse`, which fires only on success and carries no
/// error field at all (the two are mutually-exclusive branches of one
/// lifecycle point, not sequential hooks). Requires `jq` to extract
/// `exit_code`; if `jq` is absent the whole guard short-circuits and the hook
/// silently no-ops (same `|| true` never-block-the-agent philosophy as every
/// other hook here) — TUICommander falls back to the `ToolError`/`ApiError`
/// text-pattern tier for that session. The extracted value itself is never
/// parsed on the Rust side; presence of the `toolfail` event is the whole
/// signal (see `state.rs::turn_error_flags`).
fn post_tool_use_failure_hook_command() -> String {
    format!(
        r#"[ -n "${{TUIC_SESSION:-}}" ] && command -v jq >/dev/null 2>&1 && {{ {tty}; code=$(jq -r '.exit_code // 1' 2>/dev/null); printf '\033]7770;toolfail=%s\033\\' "${{code:-1}}" > "$__t"; }} >/dev/null 2>&1 || true {SENTINEL}"#,
        tty = tty_resolve(),
    )
}

/// Claude hooks (tool-level). Array order matters: the broad `PreToolUse` busy
/// entry precedes the `AskUserQuestion|ExitPlanMode` awaiting entry so awaiting
/// wins for those tools.
///
/// `Elicitation` covers MCP `elicitation/create` — an MCP server asking the user
/// for input mid tool call. It is NOT a tool call, so no `PreToolUse` matcher can
/// reach it, and its dialog matches none of the screen heuristics either: the
/// options render horizontally (`Accept  Decline`) instead of numbered, and the
/// footer is `Esc to cancel · ↑/↓ to navigate · …`, not `Enter to select`. Without
/// this entry the tab stays "busy" while the agent is blocked on the user.
/// `ElicitationResult` fires once the user answers and is the paired retraction —
/// awaiting is sticky, so a set with no clear latches the badge forever.
pub(crate) fn claude_hook_map() -> Vec<HookEntry> {
    vec![
        ("UserPromptSubmit", "", hook_command("busy")),
        (
            "PreToolUse",
            "AskUserQuestion|ExitPlanMode",
            hook_command("awaiting"),
        ),
        (
            "PostToolUse",
            "AskUserQuestion|ExitPlanMode",
            hook_command("busy"),
        ),
        ("Elicitation", "", hook_command("awaiting")),
        ("ElicitationResult", "", hook_command("busy")),
        (
            "PostToolUseFailure",
            "",
            post_tool_use_failure_hook_command(),
        ),
        ("Stop", "", hook_command("idle")),
        (
            "StopFailure",
            "",
            // Input order doesn't matter — hook_command_multi always emits
            // toolfail before state, since the idle transition it drives
            // reads-and-clears turn_error_flags at the exact moment it's
            // processed (handle_tuic_state).
            hook_command_multi(&[("state", "idle"), ("toolfail", "1")]),
        ),
        ("SessionEnd", "", hook_command("idle")),
    ]
}

/// Gemini hooks (same shell-hook shape, different event names; v0.26+).
pub(crate) fn gemini_hook_map() -> Vec<HookEntry> {
    vec![
        ("BeforeAgent", "", hook_command("busy")),
        ("BeforeTool", "", hook_command("busy")),
        ("AfterAgent", "", hook_command("idle")),
        ("Notification", "", hook_command("awaiting")),
        ("SessionEnd", "", hook_command("idle")),
    ]
}

/// Grok hooks (Claude-compatible JSON schema, written to our OWN file
/// `~/.grok/hooks/tuic.json`). Event names verified against the in-app hooks doc
/// (`~/.grok/docs/user-guide/10-hooks.md`). Lifecycle events (UserPromptSubmit,
/// Stop, SessionEnd) reject a matcher, so all entries use an empty matcher (the
/// own-file writer omits it). Grok has no clean "awaiting" event — approval
/// prompts are covered by the existing OSC-0 title heuristic, which is not
/// suppressed under instrumentation.
pub(crate) fn grok_hook_map() -> Vec<HookEntry> {
    vec![
        ("UserPromptSubmit", "", hook_command("busy")),
        ("PreToolUse", "", hook_command("busy")),
        ("Stop", "", hook_command("idle")),
        ("SessionEnd", "", hook_command("idle")),
    ]
}

/// Codex hooks (Claude-compatible JSON schema, merged into `~/.codex/hooks.json`,
/// gated by a `[features] hooks = true` flag in `config.toml`). Turn-level only:
/// Codex doesn't expose PreToolUse/PostToolUse usefully (Bash-only) and has no
/// SessionEnd — the badge clears via the idle/Stop event. SessionStart fires on
/// the first turn (not session open), so busy appears once the user submits.
pub(crate) fn codex_hook_map() -> Vec<HookEntry> {
    vec![
        ("SessionStart", "", hook_command("busy")),
        ("UserPromptSubmit", "", hook_command("busy")),
        ("Stop", "", hook_command("idle")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_command_starts_with_tuic_session_guard() {
        let cmd = hook_command("busy");
        assert!(
            cmd.starts_with(r#"[ -n "${TUIC_SESSION"#),
            "must guard on TUIC_SESSION first: {cmd}"
        );
    }

    #[test]
    fn hook_command_contains_tty_resolve() {
        let cmd = hook_command("busy");
        assert!(
            cmd.contains(r#"ps -o tty= -p "$PPID""#),
            "must resolve the controlling tty: {cmd}"
        );
    }

    #[test]
    fn hook_command_emits_state_osc() {
        assert!(hook_command("busy").contains(r"\033]7770;state=busy\033"));
        assert!(hook_command("awaiting").contains(r"\033]7770;state=awaiting\033"));
        assert!(hook_command("idle").contains(r"\033]7770;state=idle\033"));
    }

    #[test]
    fn hook_command_ends_with_sentinel() {
        assert!(
            hook_command("idle").trim_end().ends_with(SENTINEL),
            "must end with the ownership sentinel"
        );
    }

    #[test]
    fn hook_command_always_exits_zero() {
        assert!(
            hook_command("busy").contains("|| true"),
            "must never block the agent (exit 0)"
        );
    }

    #[test]
    fn claude_map_has_awaiting_for_askuserquestion_and_stop_idle() {
        let map = claude_hook_map();
        let awaiting = map
            .iter()
            .find(|(e, m, _)| *e == "PreToolUse" && m.contains("AskUserQuestion"));
        let (_, _, cmd) =
            awaiting.expect("claude map must have a PreToolUse AskUserQuestion awaiting entry");
        assert!(cmd.contains("state=awaiting"));
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "Stop" && c.contains("state=idle")),
            "Stop must drive idle"
        );
    }

    #[test]
    fn claude_map_has_no_broad_pretooluse_busy_entry() {
        // Removed deliberately: redundant with UserPromptSubmit's single busy
        // call for the busy/idle atomic (traced through note_explicit_state,
        // note_ready_screen's hook_busy/turn_activity_seen guard,
        // stamp_last_output_now, invalidate_background_probe_boundary_locked —
        // none of it needs re-affirming per tool call), and it was the root
        // cause of the green-tick scrollbar marker firing once per tool call
        // instead of once per prompt.
        let map = claude_hook_map();
        assert!(
            !map.iter()
                .any(|(e, m, _)| *e == "PreToolUse" && m.is_empty()),
            "broad PreToolUse busy entry must not be reintroduced"
        );
    }

    /// MCP elicitation blocks the agent on the user but is not a tool call, so
    /// the awaiting signal can only come from the dedicated `Elicitation` event —
    /// and it must be retracted by `ElicitationResult`, or the badge latches.
    #[test]
    fn claude_map_pairs_elicitation_awaiting_with_a_retraction() {
        let map = claude_hook_map();
        let find = |event: &str| {
            map.iter()
                .find(|(e, m, _)| *e == event && m.is_empty())
                .map(|(_, _, c)| c.clone())
        };
        let set = find("Elicitation").expect("MCP elicitation must set awaiting");
        assert!(set.contains("state=awaiting"), "{set}");
        let clear = find("ElicitationResult").expect("answered elicitation must clear awaiting");
        assert!(clear.contains("state=busy"), "{clear}");
    }

    #[test]
    fn claude_map_has_stop_failure_driving_idle_and_toolfail() {
        // PostToolUse/PostToolUseFailure and Stop/StopFailure are
        // mutually-exclusive success/failure branches of the same lifecycle
        // point, not sequential hooks — a turn ending via StopFailure would
        // never reach idle without this (only Stop drove `state=idle` before).
        let map = claude_hook_map();
        let (_, _, cmd) = map
            .iter()
            .find(|(e, _, _)| *e == "StopFailure")
            .expect("claude map must have a StopFailure entry");
        assert!(
            cmd.contains("state=idle"),
            "StopFailure must still reach idle: {cmd}"
        );
        assert!(
            cmd.contains("toolfail=1"),
            "StopFailure must flag the turn failed: {cmd}"
        );
        // Order matters: handle_tuic_state reads-and-clears turn_error_flags
        // at the exact moment it processes the idle transition, so toolfail
        // must already be set by then — emitting it after state=idle would
        // silently lose the failure flag for every StopFailure turn.
        let toolfail_pos = cmd.find("toolfail=1").expect("toolfail present");
        let state_idle_pos = cmd.find("state=idle").expect("state=idle present");
        assert!(
            toolfail_pos < state_idle_pos,
            "toolfail must be emitted before state=idle: {cmd}"
        );
    }

    #[test]
    fn claude_map_has_post_tool_use_failure_entry() {
        let map = claude_hook_map();
        let (_, matcher, cmd) = map
            .iter()
            .find(|(e, _, _)| *e == "PostToolUseFailure")
            .expect("claude map must have a PostToolUseFailure entry");
        assert!(matcher.is_empty(), "must match every tool, not a subset");
        assert!(cmd.contains("jq"), "must gate on jq availability: {cmd}");
        assert!(
            cmd.contains("toolfail="),
            "must emit the toolfail verb: {cmd}"
        );
    }

    #[test]
    fn post_tool_use_failure_hook_command_reads_stdin_exit_code_via_jq() {
        let cmd = post_tool_use_failure_hook_command();
        assert!(
            cmd.contains("command -v jq"),
            "must check jq is on PATH: {cmd}"
        );
        assert!(
            cmd.contains(".exit_code"),
            "must extract exit_code from stdin JSON: {cmd}"
        );
        assert!(cmd.contains("|| true"), "must never block the agent: {cmd}");
        assert!(cmd.trim_end().ends_with(SENTINEL));
    }

    #[test]
    fn hook_command_multi_emits_every_pair_via_one_tty_resolve() {
        let cmd = hook_command_multi(&[("state", "idle"), ("toolfail", "1")]);
        assert!(cmd.contains(r"\033]7770;state=idle\033"));
        assert!(cmd.contains(r"\033]7770;toolfail=1\033"));
        assert_eq!(
            cmd.matches(r#"ps -o tty= -p "$PPID""#).count(),
            1,
            "tty must be resolved once and reused, not per pair: {cmd}"
        );
    }

    #[test]
    fn hook_command_multi_always_orders_toolfail_before_state_regardless_of_input_order() {
        // Defensive, not just a caller convention: turn_error_flags is
        // read-and-cleared at the exact moment state=idle is processed, so
        // toolfail must be on the wire first no matter how the caller listed
        // the pairs.
        let listed_state_first = hook_command_multi(&[("state", "idle"), ("toolfail", "1")]);
        let listed_toolfail_first = hook_command_multi(&[("toolfail", "1"), ("state", "idle")]);
        for cmd in [&listed_state_first, &listed_toolfail_first] {
            let toolfail_pos = cmd.find("toolfail=1").expect("toolfail present");
            let state_idle_pos = cmd.find("state=idle").expect("state=idle present");
            assert!(
                toolfail_pos < state_idle_pos,
                "toolfail must always precede state=idle, regardless of input order: {cmd}"
            );
        }
    }

    #[test]
    fn gemini_map_has_notification_awaiting_and_afteragent_idle() {
        let map = gemini_hook_map();
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "Notification" && c.contains("state=awaiting"))
        );
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "AfterAgent" && c.contains("state=idle"))
        );
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "BeforeTool" && c.contains("state=busy"))
        );
    }
}
