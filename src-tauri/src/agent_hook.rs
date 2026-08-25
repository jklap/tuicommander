//! Native-agent hook command generation (emit side of hook-based agent state).
//!
//! Each supported agent (Claude, Gemini, …) drives its busy/idle/awaiting state
//! by running a small shell hook that invokes the `tuic-hook` sidecar, which
//! emits `OSC 7770;state=…` to its controlling tty — Claude also drives
//! `OSC 7770;toolfail=…` on failure-path hook events (`PostToolUseFailure`/
//! `StopFailure`) to flag the turn-level command block's exit code (see
//! `state.rs::turn_error_flags`), plus free-text metadata verbs
//! (`ccsession`/`cwd`/`transcript`/`tool`/`notify`) extracted from the hook's
//! stdin JSON. This module generates those hook commands and the per-agent
//! event→state maps the installer (see `agent_hook_installer`) writes into
//! the agent's settings file, and that `agent_hook_launch` writes into the
//! launch-scoped `agent-hooks/claude.json`.
//!
//! **The wire contract.** Every registration is a [`HookSpec`] carrying both
//! the argv passed to `tuic-hook` and the exact OSC 7770 sequence that fire
//! must produce (`wire`) — the state vocabulary the receiving side
//! (`pty.rs::handle_tuic_state`) understands: `prompt` on the submit-prompt
//! hooks (busy plus "the user submitted a prompt on this row", #1388 — a
//! tool call's `busy` must never be mistaken for it), `awaiting`/`busy` paired
//! for MCP elicitation, `idle` at turn end, `toolfail` before `state` on the
//! failure paths. `golden_wire_output::every_spec_emits_exactly_its_wire_contract`
//! runs the real binary for every spec and holds it to `wire`.
//!
//! For Claude, most of that behavior is not baked into argv at all:
//! `tuic-hook` derives what to emit from the hook payload's own
//! `hook_event_name` field (see `crates/tuic-hook/src/main.rs`'s
//! `DERIVATIONS` table), so a Claude spec only needs an explicit flag where
//! an entry's meaning diverges from the bare event — e.g. the narrow
//! `PreToolUse` matcher meaning "awaiting" rather than the event's default
//! "busy". Gemini/Grok/Codex still pass `--state` explicitly, since their
//! hooks haven't been verified to send `hook_event_name` in the same shape.
//!
//! **Builds without the sidecar.** Only desktop builds bundle `tuic-hook`
//! (`tauri.conf.json` `externalBin`, refreshed by `hook_binary`). The
//! headless `tuic-remote` binary ships alone but still spawns Claude with the
//! launch-scoped `--settings agent-hooks/claude.json`, so a non-desktop build
//! generates the equivalent self-contained shell one-liner from each spec's
//! `wire` instead (`ps`-resolved tty, plain `printf`) — the same bytes, minus
//! the metadata scrapes and `is_interrupt` suppression only the binary can do.
//!
//! Every generated command is inert outside TUIC (guarded on `TUIC_SESSION`
//! — in the binary flavour *twice*: once here, so the binary is never even
//! spawned outside a TUIC session, and again inside `tuic-hook` itself),
//! always exits 0, and ends in a trailing shell-comment sentinel so the
//! installer prunes only TUIC's entries and never touches user/wiz hooks.

/// Trailing shell comment marking a hook command as TUIC-owned. The installer
/// keys ownership off this — a valid comment in Claude/Gemini/Codex command
/// fields alike.
pub(crate) const SENTINEL: &str = "# tuic-managed-hook";

/// A single hook registration: `(event, matcher, command)`.
/// `matcher == ""` means "all" (no tool-name filter).
pub(crate) type HookEntry = (&'static str, &'static str, String);

/// One hook registration and its wire contract (see the module doc).
struct HookSpec {
    event: &'static str,
    /// `""` means "all" (no tool-name filter).
    matcher: &'static str,
    /// Flags passed to `tuic-hook`; empty means fully derived from the
    /// payload's `hook_event_name`.
    #[cfg_attr(not(feature = "desktop"), allow(dead_code))]
    args: &'static [&'static str],
    /// The exact `(verb, payload)` sequence this fire emits for a payload
    /// carrying only `hook_event_name` — `toolfail` always first.
    #[cfg_attr(all(feature = "desktop", not(test)), allow(dead_code))]
    wire: &'static [(&'static str, &'static str)],
}

const fn spec(
    event: &'static str,
    matcher: &'static str,
    args: &'static [&'static str],
    wire: &'static [(&'static str, &'static str)],
) -> HookSpec {
    HookSpec {
        event,
        matcher,
        args,
        wire,
    }
}

const BUSY: &[(&str, &str)] = &[("state", "busy")];
const PROMPT: &[(&str, &str)] = &[("state", "prompt")];
const AWAITING: &[(&str, &str)] = &[("state", "awaiting")];
const IDLE: &[(&str, &str)] = &[("state", "idle")];
const DERIVED: &[&str] = &[];

/// Claude hooks (tool-level). Array order matters: the broad `PreToolUse` busy
/// entry precedes the `AskUserQuestion|ExitPlanMode` awaiting entry so awaiting
/// wins for those tools.
///
/// `UserPromptSubmit` emits `prompt`, not `busy` (#1388): the scrollbar's
/// user-prompt tick is recorded from `state=prompt` alone, so sharing the
/// tool call's `busy` would tick every tool call too.
///
/// `Elicitation` covers MCP `elicitation/create` — an MCP server asking the user
/// for input mid tool call. It is NOT a tool call, so no `PreToolUse` matcher can
/// reach it, and its dialog matches none of the screen heuristics either: the
/// options render horizontally (`Accept  Decline`) instead of numbered, and the
/// footer is `Esc to cancel · ↑/↓ to navigate · …`, not `Enter to select`. Without
/// this entry the tab stays "busy" while the agent is blocked on the user.
/// `ElicitationResult` fires once the user answers and is the paired retraction —
/// awaiting is sticky, so a set with no clear latches the badge forever.
///
/// `PostToolUseFailure` derives no state — `Stop`/`StopFailure` handle the
/// transition. Deriving it (rather than the old explicit
/// `--toolfail-from-stdin`) means an unparseable payload can't be identified
/// as this event at all, so nothing is emitted; accepted because that JSON is
/// Claude Code's own generated payload, not user input. `StopFailure` emits
/// `toolfail` before `idle`, because the idle transition reads-and-clears the
/// turn's failure flag (`handle_tuic_state`).
const CLAUDE_HOOKS: &[HookSpec] = &[
    spec("SessionStart", "", DERIVED, BUSY),
    spec("UserPromptSubmit", "", DERIVED, PROMPT),
    spec("PreToolUse", "", DERIVED, BUSY),
    // The bare `PreToolUse` event derives "busy" (the broad entry above);
    // "awaiting" is what this matcher means specifically — a matcher policy
    // that belongs here, not baked into `tuic-hook`. Its `tool_name` scrape
    // is still derived.
    spec(
        "PreToolUse",
        "AskUserQuestion|ExitPlanMode",
        &["--state", "awaiting"],
        AWAITING,
    ),
    spec("PostToolUse", "AskUserQuestion|ExitPlanMode", DERIVED, BUSY),
    spec("Elicitation", "", DERIVED, AWAITING),
    spec("ElicitationResult", "", DERIVED, BUSY),
    spec("PostToolUseFailure", "", DERIVED, &[("toolfail", "1")]),
    spec("Notification", "", DERIVED, AWAITING),
    spec("Stop", "", DERIVED, IDLE),
    spec(
        "StopFailure",
        "",
        DERIVED,
        &[("toolfail", "1"), ("state", "idle")],
    ),
    spec("SessionEnd", "", DERIVED, IDLE),
];

/// Gemini hooks (same shell-hook shape, different event names; v0.26+).
const GEMINI_HOOKS: &[HookSpec] = &[
    spec("BeforeAgent", "", &["--state", "prompt"], PROMPT),
    spec("BeforeTool", "", &["--state", "busy"], BUSY),
    spec("AfterAgent", "", &["--state", "idle"], IDLE),
    spec("Notification", "", &["--state", "awaiting"], AWAITING),
    spec("SessionEnd", "", &["--state", "idle"], IDLE),
];

/// Grok hooks (Claude-compatible JSON schema, written to our OWN file
/// `~/.grok/hooks/tuic.json`). Event names verified against the in-app hooks doc
/// (`~/.grok/docs/user-guide/10-hooks.md`). Lifecycle events (UserPromptSubmit,
/// Stop, SessionEnd) reject a matcher, so all entries use an empty matcher (the
/// own-file writer omits it). Grok has no clean "awaiting" event — approval
/// prompts are covered by the existing OSC-0 title heuristic, which is not
/// suppressed under instrumentation.
const GROK_HOOKS: &[HookSpec] = &[
    spec("UserPromptSubmit", "", &["--state", "prompt"], PROMPT),
    spec("PreToolUse", "", &["--state", "busy"], BUSY),
    spec("Stop", "", &["--state", "idle"], IDLE),
    spec("SessionEnd", "", &["--state", "idle"], IDLE),
];

/// Codex hooks (Claude-compatible JSON schema, merged into `~/.codex/hooks.json`,
/// gated by a `[features] hooks = true` flag in `config.toml`). Turn-level only:
/// Codex doesn't expose PreToolUse/PostToolUse usefully (Bash-only) and has no
/// SessionEnd — the badge clears via the idle/Stop event. SessionStart fires on
/// the first turn (not session open), so busy appears once the user submits.
const CODEX_HOOKS: &[HookSpec] = &[
    spec("SessionStart", "", &["--state", "busy"], BUSY),
    spec("UserPromptSubmit", "", &["--state", "prompt"], PROMPT),
    spec("Stop", "", &["--state", "idle"], IDLE),
];

/// Absolute path to the `tuic-hook` binary to embed in generated commands:
/// the stable, install-location-independent copy `hook_binary` maintains
/// under the config dir.
#[cfg(feature = "desktop")]
fn tuic_hook_binary_path() -> String {
    crate::hook_binary::stable_path()
        .to_string_lossy()
        .to_string()
}

/// Build the guarded shell command: only if a TUIC session is active, assign
/// the binary path and invoke it with `args` when the binary is present and
/// executable — skipping the spawn entirely otherwise, rather than paying
/// for a "command not found" that `|| true` would just as validly swallow.
/// `args` are always fixed literals from the specs above (flag names,
/// fixed enum values), never arbitrary text, so they need no quoting; the
/// binary path is double-quoted because `<config_dir>` on macOS contains a
/// literal space (`Application Support`).
#[cfg(feature = "desktop")]
fn hook_binary_command(args: &[&str]) -> String {
    let path = tuic_hook_binary_path();
    let arg_str = args.join(" ");
    format!(
        r#"[ -n "${{TUIC_SESSION:-}}" ] && {{ B="{path}"; [ -x "$B" ] && "$B" {arg_str}; }} || true {SENTINEL}"#
    )
}

/// Resolve the controlling tty into `$__t`, even when the caller's stdout is
/// captured (hooks have no controlling tty of their own — read the parent's).
#[cfg(any(test, not(feature = "desktop")))]
fn tty_resolve() -> &'static str {
    r#"__t=$(ps -o tty= -p "$PPID" 2>/dev/null|tr -d '[:space:]');case "$__t" in *[0-9]*)__t="/dev/${__t#/dev/}";;*)__t="/dev/tty";;esac"#
}

/// The self-contained shell flavour of a spec (builds without the sidecar):
/// emits each `wire` pair with `printf` to the controlling tty, resolved once.
/// `toolfail` is always emitted first regardless of input order — the
/// `state=idle` it may accompany reads-and-clears the turn's failure flag.
#[cfg(any(test, not(feature = "desktop")))]
fn shell_hook_command(wire: &[(&str, &str)]) -> String {
    let (toolfail, rest): (Vec<_>, Vec<_>) = wire.iter().partition(|(verb, _)| *verb == "toolfail");
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

fn spec_command(spec: &HookSpec) -> String {
    #[cfg(feature = "desktop")]
    {
        hook_binary_command(spec.args)
    }
    #[cfg(not(feature = "desktop"))]
    {
        shell_hook_command(spec.wire)
    }
}

fn map_of(specs: &[HookSpec]) -> Vec<HookEntry> {
    specs
        .iter()
        .map(|s| (s.event, s.matcher, spec_command(s)))
        .collect()
}

/// Claude's hook map — see [`CLAUDE_HOOKS`].
pub(crate) fn claude_hook_map() -> Vec<HookEntry> {
    map_of(CLAUDE_HOOKS)
}

/// Gemini's hook map — see [`GEMINI_HOOKS`].
pub(crate) fn gemini_hook_map() -> Vec<HookEntry> {
    map_of(GEMINI_HOOKS)
}

/// Grok's hook map — see [`GROK_HOOKS`].
pub(crate) fn grok_hook_map() -> Vec<HookEntry> {
    map_of(GROK_HOOKS)
}

/// Codex's hook map — see [`CODEX_HOOKS`].
pub(crate) fn codex_hook_map() -> Vec<HookEntry> {
    map_of(CODEX_HOOKS)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_SPECS: &[(&str, &[HookSpec])] = &[
        ("claude", CLAUDE_HOOKS),
        ("gemini", GEMINI_HOOKS),
        ("grok", GROK_HOOKS),
        ("codex", CODEX_HOOKS),
    ];

    fn all_maps() -> Vec<(&'static str, Vec<HookEntry>)> {
        vec![
            ("claude", claude_hook_map()),
            ("gemini", gemini_hook_map()),
            ("grok", grok_hook_map()),
            ("codex", codex_hook_map()),
        ]
    }

    fn wire_of(
        specs: &[HookSpec],
        event: &str,
        matcher: &str,
    ) -> Vec<(&'static str, &'static str)> {
        specs
            .iter()
            .find(|s| s.event == event && s.matcher == matcher)
            .unwrap_or_else(|| panic!("no spec for ({event}, {matcher:?})"))
            .wire
            .to_vec()
    }

    /// The whole event→wire table in one literal, so a reviewer can check it
    /// against the receiving side in one place. Every entry that existed in
    /// the shell-hook era must emit exactly what its one-liner printed; the
    /// two Claude additions (`SessionStart`, `Notification`) are marked.
    #[test]
    fn wire_contract_is_the_event_table() {
        let expected: &[(&str, &str, &str, &[(&str, &str)])] = &[
            ("claude", "SessionStart", "", &[("state", "busy")]), // added with tuic-hook
            ("claude", "UserPromptSubmit", "", &[("state", "prompt")]),
            ("claude", "PreToolUse", "", &[("state", "busy")]),
            (
                "claude",
                "PreToolUse",
                "AskUserQuestion|ExitPlanMode",
                &[("state", "awaiting")],
            ),
            (
                "claude",
                "PostToolUse",
                "AskUserQuestion|ExitPlanMode",
                &[("state", "busy")],
            ),
            ("claude", "Elicitation", "", &[("state", "awaiting")]),
            ("claude", "ElicitationResult", "", &[("state", "busy")]),
            ("claude", "PostToolUseFailure", "", &[("toolfail", "1")]),
            ("claude", "Notification", "", &[("state", "awaiting")]), // added with tuic-hook
            ("claude", "Stop", "", &[("state", "idle")]),
            (
                "claude",
                "StopFailure",
                "",
                &[("toolfail", "1"), ("state", "idle")],
            ),
            ("claude", "SessionEnd", "", &[("state", "idle")]),
            ("gemini", "BeforeAgent", "", &[("state", "prompt")]),
            ("gemini", "BeforeTool", "", &[("state", "busy")]),
            ("gemini", "AfterAgent", "", &[("state", "idle")]),
            ("gemini", "Notification", "", &[("state", "awaiting")]),
            ("gemini", "SessionEnd", "", &[("state", "idle")]),
            ("grok", "UserPromptSubmit", "", &[("state", "prompt")]),
            ("grok", "PreToolUse", "", &[("state", "busy")]),
            ("grok", "Stop", "", &[("state", "idle")]),
            ("grok", "SessionEnd", "", &[("state", "idle")]),
            ("codex", "SessionStart", "", &[("state", "busy")]),
            ("codex", "UserPromptSubmit", "", &[("state", "prompt")]),
            ("codex", "Stop", "", &[("state", "idle")]),
        ];
        let actual: Vec<(&str, &str, &str, Vec<(&str, &str)>)> = ALL_SPECS
            .iter()
            .flat_map(|(agent, specs)| {
                specs
                    .iter()
                    .map(move |s| (*agent, s.event, s.matcher, s.wire.to_vec()))
            })
            .collect();
        let expected: Vec<(&str, &str, &str, Vec<(&str, &str)>)> = expected
            .iter()
            .map(|(a, e, m, w)| (*a, *e, *m, w.to_vec()))
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn hook_command_starts_with_tuic_session_guard() {
        for (agent, map) in all_maps() {
            for (event, _, cmd) in map {
                assert!(
                    cmd.starts_with(r#"[ -n "${TUIC_SESSION"#),
                    "{agent} {event}: must guard on TUIC_SESSION first: {cmd}"
                );
            }
        }
    }

    #[test]
    fn every_map_entry_ends_with_sentinel() {
        // The installer's whole prune-only-TUIC's-own-entries scheme depends
        // on every generated command carrying it, regardless of which helper
        // built it.
        for (map_name, map) in all_maps() {
            for (event, matcher, command) in map {
                assert!(
                    command.trim_end().ends_with(SENTINEL),
                    "{map_name} entry ({event}, {matcher:?}) must end with the ownership sentinel: {command}"
                );
            }
        }
    }

    #[test]
    fn hook_command_always_exits_zero() {
        for (agent, map) in all_maps() {
            for (event, _, cmd) in map {
                assert!(
                    cmd.contains("|| true"),
                    "{agent} {event}: must never block the agent (exit 0)"
                );
            }
        }
    }

    #[test]
    fn claude_map_has_awaiting_for_askuserquestion_and_stop_idle() {
        assert_eq!(
            wire_of(CLAUDE_HOOKS, "PreToolUse", "AskUserQuestion|ExitPlanMode"),
            [("state", "awaiting")]
        );
        assert_eq!(wire_of(CLAUDE_HOOKS, "Stop", ""), [("state", "idle")]);
        assert_eq!(
            wire_of(CLAUDE_HOOKS, "PreToolUse", ""),
            [("state", "busy")],
            "broad PreToolUse must drive busy"
        );
        let broad = CLAUDE_HOOKS
            .iter()
            .position(|s| s.event == "PreToolUse" && s.matcher.is_empty());
        let narrow = CLAUDE_HOOKS
            .iter()
            .position(|s| s.event == "PreToolUse" && !s.matcher.is_empty());
        assert!(
            broad < narrow,
            "broad busy entry must precede the awaiting one"
        );
    }

    /// Catches #1388: if UserPromptSubmit shared PreToolUse's `state=busy`, the
    /// scrollbar could not tell a prompt from a tool call and ticked both.
    /// (The binary actually emitting it is pinned by
    /// `golden_wire_output::every_spec_emits_exactly_its_wire_contract`.)
    #[test]
    fn submit_prompt_hooks_emit_prompt_not_busy() {
        for (specs, event) in [
            (CLAUDE_HOOKS, "UserPromptSubmit"),
            (GEMINI_HOOKS, "BeforeAgent"),
            (GROK_HOOKS, "UserPromptSubmit"),
            (CODEX_HOOKS, "UserPromptSubmit"),
        ] {
            assert_eq!(wire_of(specs, event, ""), [("state", "prompt")], "{event}");
        }
    }

    /// MCP elicitation blocks the agent on the user but is not a tool call, so
    /// the awaiting signal can only come from the dedicated `Elicitation` event —
    /// and it must be retracted by `ElicitationResult`, or the badge latches.
    #[test]
    fn claude_map_pairs_elicitation_awaiting_with_a_retraction() {
        assert_eq!(
            wire_of(CLAUDE_HOOKS, "Elicitation", ""),
            [("state", "awaiting")],
            "MCP elicitation must set awaiting"
        );
        assert_eq!(
            wire_of(CLAUDE_HOOKS, "ElicitationResult", ""),
            [("state", "busy")],
            "answered elicitation must clear awaiting"
        );
    }

    #[test]
    fn claude_map_has_stop_failure_driving_idle_and_toolfail() {
        // PostToolUse/PostToolUseFailure and Stop/StopFailure are
        // mutually-exclusive success/failure branches of the same lifecycle
        // point, not sequential hooks — a turn ending via StopFailure would
        // never reach idle without this.
        assert_eq!(
            wire_of(CLAUDE_HOOKS, "StopFailure", ""),
            [("toolfail", "1"), ("state", "idle")]
        );
    }

    #[test]
    fn gemini_map_has_notification_awaiting_and_afteragent_idle() {
        assert_eq!(
            wire_of(GEMINI_HOOKS, "Notification", ""),
            [("state", "awaiting")]
        );
        assert_eq!(wire_of(GEMINI_HOOKS, "AfterAgent", ""), [("state", "idle")]);
        assert_eq!(wire_of(GEMINI_HOOKS, "BeforeTool", ""), [("state", "busy")]);
    }

    #[test]
    fn grok_map_uses_empty_matchers_only() {
        assert!(
            GROK_HOOKS.iter().all(|s| s.matcher.is_empty()),
            "grok lifecycle events reject a matcher — every entry must be empty"
        );
    }

    #[test]
    fn codex_map_has_no_session_end() {
        assert!(
            !CODEX_HOOKS.iter().any(|s| s.event == "SessionEnd"),
            "Codex has no SessionEnd — the badge clears via Stop/idle instead"
        );
    }

    /// Non-Claude agents aren't known to send `hook_event_name`, so their
    /// argv must carry the whole state on its own.
    #[test]
    fn non_claude_specs_pass_their_state_explicitly() {
        for (agent, specs) in &ALL_SPECS[1..] {
            for s in *specs {
                let state = s
                    .wire
                    .iter()
                    .find(|(v, _)| *v == "state")
                    .map(|(_, p)| *p)
                    .unwrap();
                assert_eq!(s.args, ["--state", state], "{agent} {}", s.event);
            }
        }
    }

    // -- shell flavour (builds without the tuic-hook sidecar) ---------------

    #[test]
    fn shell_command_resolves_the_tty_and_prints_every_wire_pair() {
        for (agent, specs) in ALL_SPECS {
            for s in *specs {
                let cmd = shell_hook_command(s.wire);
                assert!(cmd.starts_with(r#"[ -n "${TUIC_SESSION"#), "{cmd}");
                assert!(cmd.contains(r#"ps -o tty= -p "$PPID""#), "{cmd}");
                assert!(cmd.contains("|| true"), "{cmd}");
                assert!(cmd.trim_end().ends_with(SENTINEL), "{cmd}");
                for (verb, payload) in s.wire {
                    assert!(
                        cmd.contains(&format!(r"\033]7770;{verb}={payload}\033")),
                        "{agent} {}: {cmd}",
                        s.event
                    );
                }
            }
        }
    }

    #[test]
    fn shell_command_always_orders_toolfail_before_state_regardless_of_input_order() {
        for wire in [
            &[("state", "idle"), ("toolfail", "1")][..],
            &[("toolfail", "1"), ("state", "idle")][..],
        ] {
            let cmd = shell_hook_command(wire);
            let toolfail = cmd.find("toolfail=1").unwrap();
            let state = cmd.find("state=idle").unwrap();
            assert!(toolfail < state, "toolfail must be printed first: {cmd}");
            assert_eq!(
                cmd.matches("ps -o tty=").count(),
                1,
                "tty resolved once: {cmd}"
            );
        }
    }

    /// Every command in every map — and every shell-flavour command — must be
    /// syntactically valid POSIX shell. Catches the quoting regressions the
    /// nested `format!` templates invite before they reach a settings file.
    #[test]
    #[cfg(unix)]
    fn every_generated_command_is_syntactically_valid_shell() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut all_commands: Vec<String> = all_maps()
            .into_iter()
            .flat_map(|(_, map)| map.into_iter().map(|(_, _, cmd)| cmd))
            .collect();
        all_commands.extend(
            ALL_SPECS
                .iter()
                .flat_map(|(_, specs)| specs.iter().map(|s| shell_hook_command(s.wire))),
        );
        assert!(!all_commands.is_empty());

        for cmd in all_commands {
            let mut child = Command::new("sh")
                .arg("-n")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn sh -n");
            child
                .stdin
                .take()
                .expect("stdin")
                .write_all(cmd.as_bytes())
                .expect("write script to sh -n");
            let out = child.wait_with_output().expect("wait for sh -n");
            assert!(
                out.status.success(),
                "not valid POSIX shell: {cmd}\nstderr: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    // -- binary flavour argv (desktop builds) -------------------------------

    #[cfg(feature = "desktop")]
    mod binary_argv {
        use super::*;

        fn cmd_of(map: &[HookEntry], event: &str, matcher: &str) -> String {
            map.iter()
                .find(|(e, m, _)| *e == event && *m == matcher)
                .map(|(_, _, c)| c.clone())
                .unwrap_or_else(|| panic!("no entry for ({event}, {matcher:?})"))
        }

        #[test]
        fn every_command_invokes_the_tuic_hook_binary_only_if_executable() {
            for (agent, map) in all_maps() {
                for (event, _, cmd) in map {
                    assert!(
                        cmd.contains(r#"[ -x "$B" ]"#),
                        "{agent} {event}: must not attempt to invoke a missing/non-executable binary: {cmd}"
                    );
                    assert!(
                        !cmd.contains("ps -o tty=") && !cmd.contains("jq"),
                        "{agent} {event}: tty resolution and JSON parsing live in the binary: {cmd}"
                    );
                }
            }
        }

        #[test]
        fn non_claude_commands_carry_their_state_flag() {
            assert!(cmd_of(&gemini_hook_map(), "BeforeAgent", "").contains("--state prompt"));
            assert!(cmd_of(&grok_hook_map(), "UserPromptSubmit", "").contains("--state prompt"));
            assert!(cmd_of(&codex_hook_map(), "UserPromptSubmit", "").contains("--state prompt"));
            assert!(cmd_of(&codex_hook_map(), "SessionStart", "").contains("--state busy"));
        }

        #[test]
        fn claude_narrow_pretooluse_is_the_only_explicit_override() {
            let map = claude_hook_map();
            for (event, matcher, cmd) in &map {
                let explicit = cmd.contains("--state") || cmd.contains("--toolfail");
                let is_narrow_pre = *event == "PreToolUse" && !matcher.is_empty();
                assert_eq!(
                    explicit, is_narrow_pre,
                    "({event}, {matcher:?}) must derive from hook_event_name unless it is the narrow PreToolUse override: {cmd}"
                );
            }
            assert!(
                cmd_of(&map, "PreToolUse", "AskUserQuestion|ExitPlanMode")
                    .contains("--state awaiting")
            );
        }
    }

    // -----------------------------------------------------------------------
    // 0b: golden wire-output tests. These execute the *actual generated
    // command* via `sh -c` against a *real compiled `tuic-hook` binary*
    // (installed into a per-test fake config dir — see `install_binary`
    // below), with the tty-write target redirected via `TUIC_HOOK_TTY`, and
    // assert on the literal bytes that reach the tty, and on the process
    // exit status. Originally written against the pure-shell implementation;
    // after the rewrite to a compiled binary, the same assertions hold
    // against the new command — that is what proves wire compatibility for a
    // user whose settings file still holds an old-format command.
    // -----------------------------------------------------------------------
    #[cfg(all(unix, feature = "desktop"))]
    mod golden_wire_output {
        use super::*;
        use std::io::{Read, Write};
        use std::process::{Command, Stdio};
        use tempfile::{NamedTempFile, TempDir};

        /// Locate the `tuic-hook` binary Cargo already built for this
        /// workspace (`cargo build --package tuic-hook`, or `pnpm
        /// build:sidecar`, or CI's dedicated build step — see
        /// `.github/workflows/ci.yml`). Reuses `tuic_cli`'s own dev-fallback
        /// resolution rather than duplicating it; `current_exe` is
        /// deliberately `None` here since the exe-sibling branch is for a
        /// packaged app, not the test harness binary.
        ///
        /// Panics with an actionable message rather than skipping silently:
        /// these are the regression-net tests for the whole conversion, so a
        /// missing binary must fail loudly, not pass by doing nothing.
        fn find_real_binary() -> std::path::PathBuf {
            let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let name = if cfg!(windows) {
                "tuic-hook.exe"
            } else {
                "tuic-hook"
            };
            let path = crate::tuic_cli::resolve_sidecar_path_from(None, manifest, name)
                .unwrap_or_else(|e| {
                    panic!(
                        "{e} (golden wire-output tests need a real tuic-hook build — \
                         run `cargo build --package tuic-hook` first)"
                    )
                });
            std::path::PathBuf::from(path)
        }

        /// Install the real, already-compiled `tuic-hook` binary into a fake
        /// config dir, and override `config::config_dir()` to point at it
        /// for the duration of the returned guard. This is exactly what
        /// `hook_binary::ensure_current` does in production, just skipping
        /// the version-drift check since source and destination are always
        /// in sync here.
        fn install_binary() -> (TempDir, impl Drop) {
            let dir = TempDir::new().expect("temp config dir");
            let bin_dir = dir.path().join("bin");
            std::fs::create_dir_all(&bin_dir).unwrap();
            let dest = bin_dir.join(if cfg!(windows) {
                "tuic-hook.exe"
            } else {
                "tuic-hook"
            });
            std::fs::copy(find_real_binary(), &dest).expect("copy tuic-hook binary");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            let guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
            (dir, guard)
        }

        /// Run `cmd` under `/bin/sh -c`, with the tty-write target redirected
        /// to a temp file via `TUIC_HOOK_TTY` (see `tty::resolve`'s doc
        /// comment in the `tuic-hook` crate). Returns (exit_code,
        /// bytes_written_to_the_tty).
        ///
        /// `session` toggles `TUIC_SESSION` (every hook command is a no-op
        /// without it). `path_override`, when set, replaces `PATH` for the
        /// child. `stdin` is piped verbatim, mirroring how Claude Code feeds
        /// hook payloads.
        fn run(
            cmd: &str,
            session: bool,
            path_override: Option<&str>,
            stdin: Option<&[u8]>,
        ) -> (i32, Vec<u8>) {
            let tty_file = NamedTempFile::new().expect("temp tty file");
            // Absolute path: `path_override` below can replace PATH entirely,
            // which must not also break locating the shell binary itself.
            let mut command = Command::new("/bin/sh");
            command
                .arg("-c")
                .arg(cmd)
                .env("TUIC_HOOK_TTY", tty_file.path())
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            if session {
                command.env("TUIC_SESSION", "test-session");
            } else {
                command.env_remove("TUIC_SESSION");
            }
            if let Some(path) = path_override {
                command.env("PATH", path);
            }
            let mut child = command.spawn().expect("spawn sh -c");
            {
                let mut child_stdin = child.stdin.take().expect("stdin");
                // A payload larger than the binary's read cap can make the
                // child finish (and exit) before this write completes,
                // closing its end of the pipe — a `BrokenPipe` here is that
                // legitimate race, not a bug in the write itself, so it's
                // tolerated like a real caller would need to; any other
                // error is still a genuine test-harness failure.
                match child_stdin.write_all(stdin.unwrap_or(b"")) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {}
                    Err(e) => panic!("write stdin: {e}"),
                }
            }
            let out = child.wait_with_output().expect("wait for sh -c");
            assert!(
                out.status.success(),
                "hook command must always exit 0 (never block the agent): {cmd}\nstderr: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let mut written = Vec::new();
            std::fs::File::open(tty_file.path())
                .expect("reopen tty file")
                .read_to_end(&mut written)
                .expect("read tty file");
            (out.status.code().unwrap_or(-1), written)
        }

        fn osc(verb: &str, payload: &str) -> Vec<u8> {
            format!("\u{1b}]7770;{verb}={payload}\u{1b}\\").into_bytes()
        }

        /// The wire contract (see the module doc), held against the real
        /// binary: every spec of every agent, fired with a payload carrying
        /// only its own `hook_event_name` (what production stdin looks like
        /// minus the scrape fields), must put exactly `spec.wire` on the tty —
        /// no more, no less, `toolfail` first. Gemini/Grok/Codex get the same
        /// payload: their explicit `--state` must win over any derivation an
        /// event-name collision with Claude's table would otherwise apply.
        #[test]
        fn every_spec_emits_exactly_its_wire_contract() {
            let _binary = install_binary();
            for (agent, specs) in ALL_SPECS {
                let map = match *agent {
                    "claude" => claude_hook_map(),
                    "gemini" => gemini_hook_map(),
                    "grok" => grok_hook_map(),
                    "codex" => codex_hook_map(),
                    other => panic!("unhandled agent {other}"),
                };
                assert_eq!(map.len(), specs.len());
                for (spec, (event, matcher, cmd)) in specs.iter().zip(map) {
                    assert_eq!((spec.event, spec.matcher), (event, matcher));
                    let stdin = format!(r#"{{"hook_event_name":"{event}"}}"#);
                    let (_, written) = run(&cmd, true, None, Some(stdin.as_bytes()));
                    let expected: Vec<u8> = spec
                        .wire
                        .iter()
                        .flat_map(|(verb, payload)| osc(verb, payload))
                        .collect();
                    assert_eq!(
                        String::from_utf8_lossy(&written),
                        String::from_utf8_lossy(&expected),
                        "{agent} ({event}, {matcher:?}) did not emit its wire contract"
                    );
                }
            }
        }

        #[test]
        fn every_map_entry_exits_zero_without_tuic_session() {
            let _binary = install_binary();
            for (_, _, cmd) in claude_hook_map()
                .into_iter()
                .chain(gemini_hook_map())
                .chain(grok_hook_map())
                .chain(codex_hook_map())
            {
                let (code, written) = run(&cmd, false, None, None);
                assert_eq!(code, 0, "must exit 0 even without TUIC_SESSION: {cmd}");
                assert!(
                    written.is_empty(),
                    "must write nothing without TUIC_SESSION: {cmd}"
                );
            }
        }

        #[test]
        fn every_map_entry_exits_zero_when_the_binary_is_missing() {
            // No install_binary() call — tuic_hook_binary_path() resolves to
            // a stable path with nothing at it. The `[ -x "$B" ]` guard must
            // skip the spawn cleanly rather than surface a "command not
            // found" from the agent's hook runner.
            let dir = TempDir::new().unwrap();
            let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
            for (_, _, cmd) in claude_hook_map() {
                let (code, written) = run(&cmd, true, None, None);
                assert_eq!(
                    code, 0,
                    "must exit 0 even if tuic-hook isn't installed yet: {cmd}"
                );
                assert!(written.is_empty());
            }
        }

        #[test]
        fn stop_failure_writes_toolfail_before_state_on_the_actual_wire() {
            let _binary = install_binary();
            let map = claude_hook_map();
            let (_, _, cmd) = map
                .iter()
                .find(|(e, _, _)| *e == "StopFailure")
                .expect("StopFailure entry present");
            let stdin = br#"{"hook_event_name":"StopFailure"}"#;
            let (code, written) = run(cmd, true, None, Some(stdin));
            assert_eq!(code, 0);
            let expected = [osc("toolfail", "1"), osc("state", "idle")].concat();
            assert_eq!(
                written, expected,
                "toolfail must be the first bytes on the wire, before state=idle"
            );
        }

        #[test]
        fn post_tool_use_failure_extracts_exit_code_from_stdin_json_natively() {
            let _binary = install_binary();
            let map = claude_hook_map();
            let (_, _, cmd) = map
                .iter()
                .find(|(e, _, _)| *e == "PostToolUseFailure")
                .expect("PostToolUseFailure entry present");

            // Claude Code's real PostToolUseFailure schema (v2.1.245) is
            // {tool_name, tool_input, tool_use_id, error, is_interrupt?,
            // duration_ms?} — there is no `exit_code` field. This is the
            // honest shape a real fire sends, not a synthetic one no real
            // build ever produces.
            let (_, written) = run(
                cmd,
                true,
                None,
                Some(br#"{"hook_event_name":"PostToolUseFailure","tool_name":"Bash","error":"command failed"}"#),
            );
            assert_eq!(
                written,
                [osc("toolfail", "1"), osc("tool", "Bash")].concat(),
                "no exit_code in the real schema — falls back to the sentinel exit code 1; \
                 tool_name still scrapes independently"
            );

            let (_, written) = run(
                cmd,
                true,
                None,
                Some(br#"{"hook_event_name":"PostToolUseFailure"}"#),
            );
            assert_eq!(
                written,
                osc("toolfail", "1"),
                "missing exit_code must fall back to the sentinel exit code 1"
            );

            // Malformed/empty/absent stdin means `hook_event_name` itself
            // can't be read, so this fire can't even be identified as
            // PostToolUseFailure — nothing is derived, matching every other
            // event's behavior on unreadable stdin (see the trade-off note
            // on this map entry in `claude_hook_map`).
            let (_, written) = run(cmd, true, None, Some(b"not json"));
            assert!(written.is_empty(), "malformed stdin must derive nothing");

            let (_, written) = run(cmd, true, None, Some(b""));
            assert!(written.is_empty(), "empty stdin must derive nothing");

            let (_, written) = run(cmd, true, None, None);
            assert!(written.is_empty(), "absent stdin must derive nothing");
        }

        #[test]
        fn post_tool_use_failure_with_is_interrupt_writes_no_toolfail_on_the_real_wire() {
            // A user pressing Esc during a tool call fires PostToolUseFailure
            // with is_interrupt: true — a cancelled call, not a real failure.
            // End-to-end through the real compiled binary + real shell.
            let _binary = install_binary();
            let map = claude_hook_map();
            let (_, _, cmd) = map
                .iter()
                .find(|(e, _, _)| *e == "PostToolUseFailure")
                .expect("PostToolUseFailure entry present");

            let (code, written) = run(
                cmd,
                true,
                None,
                Some(br#"{"hook_event_name":"PostToolUseFailure","tool_name":"Bash","error":"interrupted","is_interrupt":true}"#),
            );
            assert_eq!(code, 0);
            let written_str = String::from_utf8_lossy(&written);
            assert!(
                !written_str.contains("toolfail="),
                "is_interrupt: true must write no toolfail bytes at all, got: {written_str:?}"
            );
            // The tool-name scrape is unaffected — still useful metadata.
            assert!(
                written.ends_with(&osc("tool", "Bash")),
                "got: {written_str:?}"
            );
        }

        #[test]
        fn session_start_extracts_session_metadata_from_stdin() {
            let _binary = install_binary();
            let map = claude_hook_map();
            let (_, _, cmd) = map
                .iter()
                .find(|(e, _, _)| *e == "SessionStart")
                .expect("SessionStart entry present");
            let stdin = br#"{"hook_event_name":"SessionStart","session_id":"abc123","cwd":"/tmp/proj","transcript_path":"/tmp/t.jsonl"}"#;
            let (code, written) = run(cmd, true, None, Some(stdin));
            assert_eq!(code, 0);
            // Metadata verbs land before `state` on the wire — build_emissions
            // (tuic-hook) only hoists `toolfail` ahead of `state`; every other
            // verb keeps insertion order, and order carries no meaning here
            // since pty.rs treats each AgentMetadata verb independently.
            let expected = [
                osc("ccsession", "abc123"),
                osc("cwd", "%2Ftmp%2Fproj"),
                osc("transcript", "%2Ftmp%2Ft.jsonl"),
                osc("state", "busy"),
            ]
            .concat();
            assert_eq!(written, expected);
        }

        #[test]
        fn pre_tool_use_extracts_tool_name_from_stdin() {
            let _binary = install_binary();
            let map = claude_hook_map();
            let (_, _, cmd) = map
                .iter()
                .find(|(e, m, _)| *e == "PreToolUse" && m.contains("AskUserQuestion"))
                .expect("PreToolUse entry present");
            let stdin = br#"{"hook_event_name":"PreToolUse","tool_name":"AskUserQuestion"}"#;
            let (code, written) = run(cmd, true, None, Some(stdin));
            assert_eq!(code, 0);
            let expected = [osc("tool", "AskUserQuestion"), osc("state", "awaiting")].concat();
            assert_eq!(written, expected);
        }

        #[test]
        fn post_tool_use_extracts_tool_name_from_stdin_via_derivation() {
            let _binary = install_binary();
            let map = claude_hook_map();
            let (_, _, cmd) = map
                .iter()
                .find(|(e, _, _)| *e == "PostToolUse")
                .expect("PostToolUse entry present");
            let stdin = br#"{"hook_event_name":"PostToolUse","tool_name":"Bash"}"#;
            let (code, written) = run(cmd, true, None, Some(stdin));
            assert_eq!(code, 0);
            let expected = [osc("tool", "Bash"), osc("state", "busy")].concat();
            assert_eq!(written, expected);
        }

        #[test]
        fn notification_extracts_message_from_stdin_and_percent_encodes_it() {
            let _binary = install_binary();
            let map = claude_hook_map();
            let (_, _, cmd) = map
                .iter()
                .find(|(e, _, _)| *e == "Notification")
                .expect("Notification entry present");
            let stdin = br#"{"hook_event_name":"Notification","message":"needs input; now"}"#;
            let (code, written) = run(cmd, true, None, Some(stdin));
            assert_eq!(code, 0);
            let expected = [
                osc("notify", "needs%20input%3B%20now"),
                osc("state", "awaiting"),
            ]
            .concat();
            assert_eq!(written, expected);
        }

        #[test]
        fn help_flag_prints_something_and_exits_zero_without_tuic_session() {
            let _binary = install_binary();
            let bin = find_real_binary();
            let out = Command::new(bin)
                .arg("--help")
                .env_remove("TUIC_SESSION")
                .output()
                .expect("run tuic-hook --help");
            assert!(out.status.success());
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(!text.is_empty());
            assert!(text.contains("--state"));
        }

        /// A failed tool call's `tool_response` can legitimately carry a
        /// large stdout/stderr capture ahead of `exit_code` in the JSON —
        /// this is the realistic version of "PostToolUseFailure's payload is
        /// larger than the old 64 KiB cap", which would previously have lost
        /// the entire fire's derivation (not just `toolfail`, see the
        /// module-doc trade-off note). Confirms the raised
        /// `MAX_STDIN_BYTES` (1 MiB) actually covers a realistic large
        /// payload rather than only a synthetic small one.
        #[test]
        fn post_tool_use_failure_still_derives_correctly_with_a_large_but_under_cap_payload() {
            let _binary = install_binary();
            let map = claude_hook_map();
            let (_, _, cmd) = map
                .iter()
                .find(|(e, _, _)| *e == "PostToolUseFailure")
                .expect("PostToolUseFailure entry present");
            let filler = "x".repeat(500 * 1024);
            let stdin = format!(
                r#"{{"hook_event_name":"PostToolUseFailure","tool_response":{{"stdout":"","stderr":"{filler}"}},"exit_code":42}}"#
            );
            let (code, written) = run(cmd, true, None, Some(stdin.as_bytes()));
            assert_eq!(code, 0);
            assert_eq!(written, osc("toolfail", "42"));
        }

        /// The other side of the same trade-off: a payload that genuinely
        /// exceeds `MAX_STDIN_BYTES` must still degrade gracefully — no
        /// panic, no hang, exit 0, and (since `hook_event_name` itself is
        /// pushed past the truncation point here) nothing derived, exactly
        /// like any other unparseable stdin.
        #[test]
        fn oversized_stdin_past_the_cap_degrades_to_no_derivation_not_a_crash() {
            let _binary = install_binary();
            let map = claude_hook_map();
            let (_, _, cmd) = map
                .iter()
                .find(|(e, _, _)| *e == "Stop")
                .expect("Stop entry present");
            // Filler before hook_event_name guarantees truncation cuts the
            // JSON off before that field is ever read.
            let filler = "x".repeat(2 * 1024 * 1024);
            let stdin = format!(r#"{{"padding":"{filler}","hook_event_name":"Stop"}}"#);
            let (code, written) = run(cmd, true, None, Some(stdin.as_bytes()));
            assert_eq!(code, 0, "must still exit 0 on a too-large payload");
            assert!(
                written.is_empty(),
                "truncated-past-the-cap stdin must derive nothing, not panic or hang"
            );
        }

        /// Gemini/Grok/Codex's hook-invocation stdin-closing behavior has
        /// never been verified — before derivation, their entries never
        /// called `read_stdin_json()` at all (bare `--state`, no
        /// `--emit-*`/`--toolfail-from-stdin`), so this path was previously
        /// unreachable for them. Now every fire reads stdin unconditionally.
        /// If a caller spawns the hook with stdin inherited from an open tty
        /// rather than piped-then-closed, this proves the read still can't
        /// hang the process past its bounded timeout (`STDIN_READ_TIMEOUT`
        /// in `crates/tuic-hook/src/main.rs`) — the never-block invariant
        /// this whole binary exists to guarantee.
        #[test]
        fn stdin_read_has_a_bounded_timeout_when_the_caller_never_closes_it() {
            let _binary = install_binary();
            let bin = find_real_binary();
            let tty_file = NamedTempFile::new().expect("temp tty file");
            let mut child = Command::new(bin)
                .arg("--state")
                .arg("busy")
                .env("TUIC_SESSION", "test-session")
                .env("TUIC_HOOK_TTY", tty_file.path())
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn tuic-hook");
            // Deliberately keep the write end of the stdin pipe open — never
            // written to, never dropped — simulating a caller whose hook
            // invocation never sends EOF.
            let _stdin_handle = child.stdin.take().expect("stdin");

            let start = std::time::Instant::now();
            let mut exited = false;
            for _ in 0..40 {
                if child.try_wait().expect("try_wait").is_some() {
                    exited = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            let elapsed = start.elapsed();
            if !exited {
                let _ = child.kill();
            }
            assert!(exited, "tuic-hook must exit even if stdin is never closed");
            assert!(
                elapsed < std::time::Duration::from_secs(2),
                "must not block on stdin beyond its read timeout: took {elapsed:?}"
            );
        }

        /// KNOWN GAP, locked down rather than silently left untested: `tuic-hook`'s
        /// `DERIVATIONS` table is matched purely on the `hook_event_name` string,
        /// with no per-agent scoping. Gemini's own event names "Notification" and
        /// "SessionEnd" happen to be spelled identically to two Claude entries in
        /// that table. Gemini's map carries an explicit `--state`, so the *state*
        /// transition stays correct either way — but if a future Gemini payload
        /// shape turns out to include a `hook_event_name` field (its hooks
        /// "haven't been verified" not to, per this module's doc comment),
        /// Notification would ALSO start emitting a `notify` scrape Gemini's map
        /// never asked for, contradicting this module's doc comment that
        /// non-Claude agents "fall back to flags exactly as before derivation
        /// existed." This test pins the current, real behavior (not the intended
        /// one) so a fix — or a decision to accept the risk — is a deliberate,
        /// visible change to this test, not a silent one.
        #[test]
        fn gemini_notification_name_collision_with_claude_derivations_currently_leaks_a_scrape() {
            let _binary = install_binary();
            let map = gemini_hook_map();

            let (_, _, notification_cmd) = map
                .iter()
                .find(|(e, _, _)| *e == "Notification")
                .expect("gemini map must have a Notification entry");
            let stdin = br#"{"hook_event_name":"Notification","message":"unexpected but present"}"#;
            let (_, written) = run(notification_cmd, true, None, Some(stdin));
            assert_eq!(
                written,
                [
                    osc("notify", "unexpected%20but%20present"),
                    osc("state", "awaiting"),
                ]
                .concat(),
                "documents the current leak — Claude's Notification derivation scrapes \
                 `message` for ANY caller whose payload names itself \"Notification\", \
                 including Gemini's, since matching isn't scoped per agent"
            );

            // SessionEnd has no scrape field in DERIVATIONS, so its collision is
            // currently harmless — state stays the only output.
            let (_, _, session_end_cmd) = map
                .iter()
                .find(|(e, _, _)| *e == "SessionEnd")
                .expect("gemini map must have a SessionEnd entry");
            let stdin = br#"{"hook_event_name":"SessionEnd"}"#;
            let (_, written) = run(session_end_cmd, true, None, Some(stdin));
            assert_eq!(written, osc("state", "idle"));
        }
    }
}
