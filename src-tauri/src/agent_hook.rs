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
//! the agent's settings file.
//!
//! For Claude, most of that behavior is no longer baked into this module's
//! argv at all: `tuic-hook` derives what to emit from the hook payload's own
//! `hook_event_name` field (see `crates/tuic-hook/src/main.rs`'s
//! `DERIVATIONS` table), so `claude_hook_map()` below only needs an explicit
//! flag where an entry's meaning diverges from the bare event — e.g. the
//! narrow `PreToolUse` matcher meaning "awaiting" rather than the event's
//! default "busy". Gemini/Grok/Codex still pass `--state` explicitly, since
//! their hooks haven't been verified to send `hook_event_name` in the same
//! shape.
//!
//! Until this file's previous revision, the command was pure shell: it
//! resolved the controlling tty itself (`ps -o tty= -p $PPID`) and shelled
//! out to `jq` to read `PostToolUseFailure`'s stdin JSON — silently
//! no-op'ing the whole failure-flagging path on any machine without `jq` on
//! `PATH` (see `docs/FEATURES.md`'s red-tick section, or `git log` on this
//! file). `tuic-hook` removes both dependencies: native tty resolution
//! (validated end-to-end against a real PTY — no `ps`), and `serde_json` for
//! payload parsing. The command here is now just a guard plus one process
//! invocation; the binary itself, not this generator, still always exits 0.
//!
//! Every generated command is inert outside TUIC (guarded on `TUIC_SESSION`
//! *twice* — once here, so the binary is never even spawned outside a TUIC
//! session, and again inside `tuic-hook` itself, for direct-invocation
//! correctness), and ends in a trailing shell-comment sentinel so the
//! installer prunes only TUIC's entries and never touches user/wiz hooks.

/// Trailing shell comment marking a hook command as TUIC-owned. The installer
/// keys ownership off this — a valid comment in Claude/Gemini/Codex command
/// fields alike.
pub(crate) const SENTINEL: &str = "# tuic-managed-hook";

/// A single hook registration: `(event, matcher, command)`.
/// `matcher == ""` means "all" (no tool-name filter).
pub(crate) type HookEntry = (&'static str, &'static str, String);

/// Absolute path to the `tuic-hook` binary to embed in generated commands.
///
/// Desktop builds resolve the stable, install-location-independent copy
/// `hook_binary` maintains under the config dir. Non-desktop builds (the
/// headless `tuic-remote` binary, built with `--no-default-features`) never
/// install hooks — there is no local settings file or local tty to drive —
/// so this branch exists only so the module compiles uniformly across
/// feature sets; the value it returns is never invoked there.
#[cfg(feature = "desktop")]
fn tuic_hook_binary_path() -> String {
    crate::hook_binary::stable_path()
        .to_string_lossy()
        .to_string()
}

#[cfg(not(feature = "desktop"))]
fn tuic_hook_binary_path() -> String {
    "tuic-hook".to_string()
}

/// Build the guarded shell command: only if a TUIC session is active, assign
/// the binary path and invoke it with `args` when the binary is present and
/// executable — skipping the spawn entirely otherwise, rather than paying
/// for a "command not found" that `|| true` would just as validly swallow.
/// `args` are always fixed literals from the callers below (flag names,
/// fixed enum values), never arbitrary text, so they need no quoting; the
/// binary path is double-quoted because `<config_dir>` on macOS contains a
/// literal space (`Application Support`).
fn hook_binary_command(args: &[&str]) -> String {
    let path = tuic_hook_binary_path();
    let arg_str = args.join(" ");
    format!(
        r#"[ -n "${{TUIC_SESSION:-}}" ] && {{ B="{path}"; [ -x "$B" ] && "$B" {arg_str}; }} || true {SENTINEL}"#
    )
}

/// `OSC 7770;state=<state>` only — the common case (`UserPromptSubmit`,
/// `Stop`, `SessionEnd`, …).
pub(crate) fn hook_command(state: &str) -> String {
    hook_binary_command(&["--state", state])
}

/// No flags at all: `hook_event_name`, which `tuic-hook` reads from the
/// hook's own stdin JSON, fully determines what gets emitted (see
/// `crates/tuic-hook/src/main.rs`'s `DERIVATIONS` table) — state, any
/// free-text scrape, and `toolfail` alike. Used for every Claude entry whose
/// meaning matches the bare event; only `PreToolUse`'s narrow matcher needs
/// an explicit override (see `claude_hook_map` below).
fn derived_hook_command() -> String {
    hook_binary_command(&[])
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
        ("SessionStart", "", derived_hook_command()),
        ("UserPromptSubmit", "", derived_hook_command()),
        (
            "PreToolUse",
            "AskUserQuestion|ExitPlanMode",
            // Narrow matcher only (see claude_map_has_no_broad_pretooluse_busy_entry
            // below). The bare `PreToolUse` event derives "busy" (correct for
            // Grok's broad, unmatched `PreToolUse`); this override to
            // "awaiting" is what the AskUserQuestion|ExitPlanMode matcher
            // means specifically on this already-scoped entry — a matcher
            // policy that belongs here, not baked into `tuic-hook`. Its
            // `tool_name` scrape is still derived, not an explicit flag.
            hook_binary_command(&["--state", "awaiting"]),
        ),
        (
            "PostToolUse",
            "AskUserQuestion|ExitPlanMode",
            derived_hook_command(),
        ),
        ("Elicitation", "", derived_hook_command()),
        ("ElicitationResult", "", derived_hook_command()),
        // Trade-off vs. the old explicit `--toolfail-from-stdin`: that flag
        // guaranteed a `toolfail=1` fallback even against malformed/absent
        // stdin. Deriving instead means an unparseable payload can't be
        // identified as this event at all, so nothing is emitted — a red
        // tick could be missed on this one hook if Claude Code ever sent it
        // unparseable JSON. Accepted because that JSON is Claude Code's own
        // generated payload (not user input), so it is not expected to be
        // malformed in practice.
        ("PostToolUseFailure", "", derived_hook_command()),
        ("Notification", "", derived_hook_command()),
        ("Stop", "", derived_hook_command()),
        ("StopFailure", "", derived_hook_command()),
        ("SessionEnd", "", derived_hook_command()),
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
    fn hook_command_invokes_the_tuic_hook_binary_with_state_flag() {
        // Native tty resolution moved into the compiled binary — the
        // generator's job is now just "build the right argv", not resolve a
        // tty itself. See the golden_wire_output module below for proof the
        // *actual bytes on the wire* are unchanged.
        assert!(hook_command("busy").contains("--state busy"));
        assert!(hook_command("awaiting").contains("--state awaiting"));
        assert!(hook_command("idle").contains("--state idle"));
    }

    #[test]
    fn hook_command_ends_with_sentinel() {
        assert!(
            hook_command("idle").trim_end().ends_with(SENTINEL),
            "must end with the ownership sentinel"
        );
    }

    #[test]
    fn every_map_entry_ends_with_sentinel() {
        // hook_command_ends_with_sentinel only ever exercises hook_command("idle") —
        // one explicit-state call. derived_hook_command() routes through the same
        // hook_binary_command() and so carries SENTINEL too, but nothing asserted
        // that across the actual maps: most Claude entries, and every Gemini/Grok/
        // Codex entry, are derived_hook_command() calls this never touched. The
        // installer's whole prune-only-TUIC's-own-entries scheme depends on every
        // generated command carrying it, regardless of which helper built it.
        for (map_name, map) in [
            ("claude", claude_hook_map()),
            ("gemini", gemini_hook_map()),
            ("grok", grok_hook_map()),
            ("codex", codex_hook_map()),
        ] {
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
        assert!(
            hook_command("busy").contains("|| true"),
            "must never block the agent (exit 0)"
        );
    }

    #[test]
    fn hook_command_checks_the_binary_is_executable_before_invoking_it() {
        let cmd = hook_command("busy");
        assert!(
            cmd.contains(r#"[ -x "$B" ]"#),
            "must not attempt to invoke a missing/non-executable binary: {cmd}"
        );
    }

    #[test]
    fn claude_map_has_awaiting_override_for_askuserquestion_and_derived_stop() {
        let map = claude_hook_map();
        let awaiting = map
            .iter()
            .find(|(e, m, _)| *e == "PreToolUse" && m.contains("AskUserQuestion"));
        let (_, _, cmd) =
            awaiting.expect("claude map must have a PreToolUse AskUserQuestion awaiting entry");
        assert!(cmd.contains("--state awaiting"));
        let (_, _, stop_cmd) = map
            .iter()
            .find(|(e, _, _)| *e == "Stop")
            .expect("claude map must have a Stop entry");
        assert!(
            !stop_cmd.contains("--state"),
            "Stop's idle transition is derived from hook_event_name, not an explicit flag: {stop_cmd}"
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
        assert!(
            !set.contains("--state"),
            "Elicitation's awaiting is derived, not baked into argv: {set}"
        );
        let clear = find("ElicitationResult").expect("answered elicitation must clear awaiting");
        assert!(
            !clear.contains("--state"),
            "ElicitationResult's busy is derived, not baked into argv: {clear}"
        );
    }

    #[test]
    fn claude_map_has_stop_failure_entry_deriving_idle_and_toolfail() {
        // PostToolUse/PostToolUseFailure and Stop/StopFailure are
        // mutually-exclusive success/failure branches of the same lifecycle
        // point, not sequential hooks — a turn ending via StopFailure would
        // never reach idle without this. Both `idle` and `toolfail=1` are now
        // derived by `tuic-hook` itself from `hook_event_name` (see
        // `crates/tuic-hook/src/main.rs`'s `DERIVATIONS` table) rather than
        // baked into this argv — proven against the real binary by
        // `golden_wire_output::stop_failure_writes_toolfail_before_state_on_the_actual_wire`
        // below, including that `toolfail` reaches the wire before `state`.
        let map = claude_hook_map();
        let (_, matcher, cmd) = map
            .iter()
            .find(|(e, _, _)| *e == "StopFailure")
            .expect("claude map must have a StopFailure entry");
        assert!(matcher.is_empty());
        assert!(
            !cmd.contains("--state") && !cmd.contains("--toolfail"),
            "StopFailure's idle+toolfail are derived, not baked into argv: {cmd}"
        );
    }

    #[test]
    fn claude_map_has_post_tool_use_failure_entry_with_no_explicit_argv() {
        let map = claude_hook_map();
        let (_, matcher, cmd) = map
            .iter()
            .find(|(e, _, _)| *e == "PostToolUseFailure")
            .expect("claude map must have a PostToolUseFailure entry");
        assert!(matcher.is_empty(), "must match every tool, not a subset");
        assert!(
            !cmd.contains("jq"),
            "must not depend on jq — that dependency is gone for good: {cmd}"
        );
        assert!(
            !cmd.contains("--toolfail"),
            "toolfail is now derived from hook_event_name, not baked into argv: {cmd}"
        );
    }

    #[test]
    fn claude_map_has_session_start_entry_with_no_explicit_argv() {
        let map = claude_hook_map();
        let (_, matcher, cmd) = map
            .iter()
            .find(|(e, _, _)| *e == "SessionStart")
            .expect("claude map must have a SessionStart entry");
        assert!(matcher.is_empty());
        assert!(
            !cmd.contains("--state") && !cmd.contains("--emit-session"),
            "SessionStart's busy state and session metadata scrape are derived, not baked into argv: {cmd}"
        );
    }

    #[test]
    fn claude_map_has_notification_entry_with_no_explicit_argv() {
        let map = claude_hook_map();
        let (_, matcher, cmd) = map
            .iter()
            .find(|(e, _, _)| *e == "Notification")
            .expect("claude map must have a Notification entry");
        assert!(matcher.is_empty());
        assert!(
            !cmd.contains("--state") && !cmd.contains("--emit-notify"),
            "Notification's awaiting state and message scrape are derived, not baked into argv: {cmd}"
        );
    }

    #[test]
    fn claude_map_pre_and_post_tool_use_ride_on_the_existing_narrow_matcher() {
        // Must ride on the existing AskUserQuestion|ExitPlanMode matcher, not
        // a new broad one — see claude_map_has_no_broad_pretooluse_busy_entry.
        // tool_name scraping is derived from hook_event_name for both now;
        // only PreToolUse still carries an explicit --state override.
        let map = claude_hook_map();
        for event in ["PreToolUse", "PostToolUse"] {
            let (_, matcher, _) = map
                .iter()
                .find(|(e, _, _)| *e == event)
                .unwrap_or_else(|| panic!("claude map must have a {event} entry"));
            assert_eq!(matcher, &"AskUserQuestion|ExitPlanMode");
        }
        let (_, _, pre_cmd) = map.iter().find(|(e, _, _)| *e == "PreToolUse").unwrap();
        assert!(pre_cmd.contains("--state awaiting"));
        let (_, _, post_cmd) = map.iter().find(|(e, _, _)| *e == "PostToolUse").unwrap();
        assert!(
            !post_cmd.contains("--emit-tool"),
            "tool_name scrape for PostToolUse is now derived, not an explicit flag: {post_cmd}"
        );
    }

    #[test]
    fn gemini_map_has_notification_awaiting_and_afteragent_idle() {
        let map = gemini_hook_map();
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "Notification" && c.contains("--state awaiting"))
        );
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "AfterAgent" && c.contains("--state idle"))
        );
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "BeforeTool" && c.contains("--state busy"))
        );
    }

    // -----------------------------------------------------------------------
    // 0c: map-shape coverage for grok/codex (previously only exercised
    // indirectly via agent_hook_commands.rs).
    // -----------------------------------------------------------------------

    #[test]
    fn grok_map_has_busy_and_idle_with_empty_matchers_only() {
        let map = grok_hook_map();
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "UserPromptSubmit" && c.contains("--state busy"))
        );
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "PreToolUse" && c.contains("--state busy"))
        );
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "Stop" && c.contains("--state idle"))
        );
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "SessionEnd" && c.contains("--state idle"))
        );
        assert!(
            map.iter().all(|(_, m, _)| m.is_empty()),
            "grok lifecycle events reject a matcher — every entry must be empty"
        );
    }

    #[test]
    fn codex_map_has_sessionstart_and_userpromptsubmit_driving_busy_and_stop_idle() {
        let map = codex_hook_map();
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "SessionStart" && c.contains("--state busy"))
        );
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "UserPromptSubmit" && c.contains("--state busy"))
        );
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "Stop" && c.contains("--state idle"))
        );
        assert!(
            !map.iter().any(|(e, _, _)| *e == "SessionEnd"),
            "Codex has no SessionEnd — the badge clears via Stop/idle instead"
        );
    }

    /// Every command in every map must be syntactically valid POSIX shell.
    /// Cheap, and catches exactly the quoting regressions the nested
    /// `format!` templates invite — it would have caught an unbalanced quote
    /// or brace before it ever reached a user's settings file.
    #[test]
    #[cfg(unix)]
    fn every_generated_command_is_syntactically_valid_shell() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let all_commands: Vec<String> = claude_hook_map()
            .into_iter()
            .chain(gemini_hook_map())
            .chain(grok_hook_map())
            .chain(codex_hook_map())
            .map(|(_, _, cmd)| cmd)
            .collect();
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
    #[cfg(unix)]
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

        #[test]
        fn every_map_entry_writes_the_exact_expected_osc_bytes() {
            let _binary = install_binary();

            // Claude's map is now mostly argv-free — `hook_event_name`
            // (which Claude Code populates on every real hook fire) is what
            // drives derivation, so each entry below runs with a payload
            // naming its own event, matching what production stdin looks
            // like for that event. Gemini/Grok/Codex still pass `--state`
            // explicitly and need no stdin at all.
            for (event, _, cmd) in claude_hook_map() {
                let stdin: Vec<u8> = format!(r#"{{"hook_event_name":"{event}"}}"#).into_bytes();
                let (_, written) = run(&cmd, true, None, Some(&stdin));
                let expected: Vec<u8> = match event {
                    "SessionStart" | "UserPromptSubmit" | "PostToolUse" | "ElicitationResult" => {
                        osc("state", "busy")
                    }
                    "PreToolUse" | "Notification" | "Elicitation" => osc("state", "awaiting"),
                    "Stop" | "SessionEnd" => osc("state", "idle"),
                    "StopFailure" => [osc("toolfail", "1"), osc("state", "idle")].concat(),
                    // PostToolUseFailure derives no state at all — its
                    // toolfail (needing a real exit_code) is covered
                    // separately below.
                    "PostToolUseFailure" => continue,
                    other => panic!("unhandled claude event in this test: {other}"),
                };
                assert_eq!(
                    written, expected,
                    "event {event} did not emit the expected wire bytes"
                );
            }

            for (event, _, cmd) in gemini_hook_map() {
                let (_, written) = run(&cmd, true, None, None);
                let expected = match event {
                    "BeforeAgent" | "BeforeTool" => osc("state", "busy"),
                    "AfterAgent" | "SessionEnd" => osc("state", "idle"),
                    "Notification" => osc("state", "awaiting"),
                    other => panic!("unhandled gemini event in this test: {other}"),
                };
                assert_eq!(written, expected, "event {event} wire mismatch");
            }

            for (event, _, cmd) in grok_hook_map() {
                let (_, written) = run(&cmd, true, None, None);
                let expected = match event {
                    "UserPromptSubmit" | "PreToolUse" => osc("state", "busy"),
                    "Stop" | "SessionEnd" => osc("state", "idle"),
                    other => panic!("unhandled grok event in this test: {other}"),
                };
                assert_eq!(written, expected, "event {event} wire mismatch");
            }

            for (event, _, cmd) in codex_hook_map() {
                let (_, written) = run(&cmd, true, None, None);
                let expected = match event {
                    "SessionStart" | "UserPromptSubmit" => osc("state", "busy"),
                    "Stop" => osc("state", "idle"),
                    other => panic!("unhandled codex event in this test: {other}"),
                };
                assert_eq!(written, expected, "event {event} wire mismatch");
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
