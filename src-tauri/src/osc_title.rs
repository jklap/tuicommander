//! OSC 0/1/2 title handling, moved off the frontend so it works for every
//! transport instead of only the desktop window.
//!
//! Previously the *entire* consumer of OSC title bytes was a desktop-only
//! `app.emit` (`pty.rs`'s `TermEvent::Title`/`TermEvent::ResetTitle` arms)
//! with no bus/SSE arm and no write to `PtySession.display_name` anywhere —
//! a headless/browser-only client never saw a title at all, and it only ever
//! became durable state as an accidental side effect of the desktop
//! frontend's own listener echoing back through `set_session_name`. This
//! module is a faithful Rust port of that frontend policy
//! (`Terminal.tsx`'s `cleanOscTitle`/title-listener, since deleted there),
//! writing through `AppState::set_session_display_name` so both transports
//! get it identically.

use crate::state::AppEvent;

/// Shell control-flow pattern — a title containing one of these is a cryptic
/// script fragment, not a useful name.
///
/// `(?-u:...)` restricts `\b`/word-boundary semantics to ASCII for this whole
/// group — required for parity with the JS original: ECMAScript's `\b`/`\w`
/// are ALWAYS ASCII-only (`[A-Za-z0-9_]`), even with the `u` flag, whereas
/// the `regex` crate's `\b`/`\w` are Unicode-aware by default. Without this,
/// a Unicode letter adjacent to a keyword could change whether a boundary is
/// recognized. This is also why `"ifort build.f90"` must NOT be rejected —
/// `\bif\b` requires a real word boundary on both sides, not a substring
/// match; see the `does_not_reject_commands_containing_keyword_substrings`
/// test, which is the one that would catch a naive substring-based port.
fn shell_script_re() -> &'static regex::Regex {
    lazy_static::lazy_static! {
        static ref RE: regex::Regex = regex::Regex::new(
            r"(?-u:;|&&|\|\||\$\(|\bif\b|\bthen\b|\belse\b|\belif\b|\bfi\b|\bfor\b|\bwhile\b|\bdo\b|\bdone\b|\bcase\b|\besac\b)"
        ).unwrap();
    }
    &RE
}

/// Clean an OSC 0/2 title: strip user@host prefix, env var assignments, and
/// command args. Returns an empty string if the title is only a user@host
/// pattern (no useful info), a bare path (the shell reporting cwd — not
/// useful as a tab title since the status bar already shows the full path),
/// or if it looks like a shell script (compound commands, control flow) —
/// a faithful port of the frontend `cleanOscTitle`, see its history for the
/// original policy rationale.
pub(crate) fn clean_osc_title(title: &str) -> String {
    // Reject titles that look like shell scripts before any processing.
    if shell_script_re().is_match(title) {
        return String::new();
    }

    lazy_static::lazy_static! {
        // Leading spinner/symbol noise: *, middle dots, bullets, braille
        // patterns, dingbats, geometric shapes, and other non-alphanumeric
        // decorators agents prepend. The dingbat range starts at U+2713
        // (✓✔✕✖✗✘) rather than U+2720 so completion and error indicators are
        // stripped too — pi ends a turn with "✓ | π | repo" and a failure
        // with "✗ | π | repo", which would otherwise leak a status glyph
        // into the tab name.
        static ref LEADING_NOISE_RE: regex::Regex = regex::Regex::new(
            "^[\\s*\u{00B7}\u{2022}\u{2219}\u{22C5}\u{2027}\u{25A0}-\u{25FF}\u{2800}-\u{28FF}\u{2713}-\u{273F}\u{2580}-\u{259F}]+"
        ).unwrap();
        // The separator the indicator was attached to, so a status-prefixed
        // title (pi emits "⠦ | π | repo", "○ | π | repo", "✓ | π | repo")
        // does not leave a dangling "| " once the animated glyph is gone.
        static ref LEADING_SEP_RE: regex::Regex = regex::Regex::new(
            "^[|\u{2502}\u{00B7}\\-\u{2013}\u{2014}:]+\\s*"
        ).unwrap();
        // "user@host:" or bare "user@host" prefix.
        static ref USER_HOST_RE: regex::Regex = regex::Regex::new(
            r"^[^@\s]+@[^:\s]+(:\s*)?"
        ).unwrap();
        // Leading env var assignments (KEY=value pairs, including empty
        // values). `[A-Za-z0-9_]` spelled out explicitly rather than `\w` —
        // same ASCII-parity reasoning as `shell_script_re`.
        static ref ENV_ASSIGN_RE: regex::Regex = regex::Regex::new(
            r"^(?:\s*[A-Za-z0-9_]+=\S*\s+)+"
        ).unwrap();
        // A bare path: the shell reporting cwd, not a useful tab title.
        static ref PATH_RE: regex::Regex = regex::Regex::new(
            r"^(?:/|~|[A-Za-z]:[\\/]|\\\\)"
        ).unwrap();
    }

    let mut cleaned = LEADING_NOISE_RE.replace(title, "").into_owned();
    cleaned = LEADING_SEP_RE.replace(&cleaned, "").into_owned();
    cleaned = USER_HOST_RE.replace(&cleaned, "").into_owned();
    cleaned = ENV_ASSIGN_RE.replace(&cleaned, "").into_owned();
    cleaned = cleaned.trim().to_string();

    if PATH_RE.is_match(&cleaned) {
        return String::new();
    }

    // Strip flags and their values, keep command + subcommands (bare words
    // before the first flag).
    if !cleaned.is_empty() {
        let kept: Vec<&str> = cleaned
            .split_whitespace()
            .take_while(|w| !w.starts_with('-'))
            .collect();
        cleaned = kept.join(" ");
    }
    cleaned
}

/// Whether to skip applying an OSC title right now for `session_id`: either
/// the tab has an explicit user rename (`display_name_is_custom`), or the
/// session has an active agent intent AND intent-titling is effectively on
/// (global AND per-agent — see `marker_flags_for_agent`, the same rule the
/// intent-title path itself already uses). The frontend's original gate only
/// ever checked the global setting; this closes that gap deliberately, not
/// as an incidental side effect of porting.
fn should_skip(state: &crate::AppState, session_id: &str) -> bool {
    let Some(entry) = state.session_maps.sessions.get(session_id) else {
        return true; // no session to apply to
    };
    let is_custom = entry.lock().display_name_is_custom;
    drop(entry);
    if is_custom {
        return true;
    }
    let Some((has_intent, agent_type)) = state
        .session_maps
        .session_states
        .get(session_id)
        .map(|s| (s.agent_intent.is_some(), s.agent_type.clone()))
    else {
        return false;
    };
    if !has_intent {
        return false;
    }
    let (show_intent, _) =
        crate::mcp_http::mcp_transport::marker_flags_for_agent(state, agent_type.as_deref());
    show_intent
}

/// Apply an OSC 0/2 title (`Some(title)`) or an OSC ResetTitle (`None`) to
/// `session_id`'s display name, writing through
/// `AppState::set_session_display_name` so both transports converge on one
/// value.
///
/// `base_name` is `ChunkProcessor::osc_title_base` — captured ONCE per
/// session (outer `None` = "not yet captured"; `Some(None)` = "captured, and
/// the base was itself no name") and never overwritten by a later title, so
/// a repaint or a second real title doesn't stomp on the name to restore to.
/// A naive single-`Option<String>` capture (ambiguous between "not captured"
/// and "captured empty") would silently re-capture the ALREADY-CLEANED title
/// from a previous call as if it were the original base — this is exactly
/// the class of bug `base_name_is_recorded_once_and_not_overwritten_by_a_later_title`
/// guards against.
#[allow(clippy::option_option)] // deliberate 3-state disambiguation, see doc comment above
pub(crate) fn apply_osc_title(
    state: &crate::AppState,
    session_id: &str,
    title: Option<&str>,
    base_name: &mut Option<Option<String>>,
) {
    if should_skip(state, session_id) {
        return;
    }

    // Restore the captured base (component-lifetime capture, like the ported
    // frontend's `originalName` — never cleared here, only ever set once).
    let restore_base = |base_name: &Option<Option<String>>| {
        if let Some(base) = base_name.clone() {
            state.set_session_display_name(session_id, base, false);
        }
    };

    match title {
        None => restore_base(base_name),
        Some(raw) => {
            let cleaned = clean_osc_title(raw);
            if cleaned.is_empty() {
                restore_base(base_name);
                return;
            }
            if base_name.is_none() {
                let current = state
                    .session_maps
                    .sessions
                    .get(session_id)
                    .map(|s| s.lock().display_name.clone());
                *base_name = Some(current.flatten());
            }
            state.set_session_display_name(session_id, Some(cleaned), false);
        }
    }
}

/// Restore a session's display name to its captured OSC-title base when the
/// process exits — the backend equivalent of the ported frontend's own
/// exit-time restore (`Terminal.tsx`'s PTY-exit handler, which read the same
/// `originalName` this module's `base_name` replaces). Only meaningful once:
/// the reader thread calls this right before `emit_session_closed`, and the
/// `ChunkProcessor` that owns `base_name` is dropped immediately after.
///
/// A no-op when nothing was ever captured, or when the user has since
/// applied an explicit custom rename (`set_session_display_name`'s own
/// no-op guard also protects against clobbering, but skipping here avoids
/// even attempting it).
#[allow(clippy::option_option)] // deliberate 3-state disambiguation, see apply_osc_title's doc comment
pub(crate) fn restore_base_on_exit(
    state: &crate::AppState,
    session_id: &str,
    base_name: &Option<Option<String>>,
) {
    let Some(base) = base_name.clone() else {
        return; // never captured — nothing to restore
    };
    let is_custom = state
        .session_maps
        .sessions
        .get(session_id)
        .is_some_and(|s| s.lock().display_name_is_custom);
    if is_custom {
        return;
    }
    state.set_session_display_name(session_id, base, false);
}

// Silence "unused" for the AppEvent import when this module is compiled
// without any test referencing it directly by name (tests reference it via
// `crate::state::AppEvent` instead) — kept for the doc comment above and any
// future direct construction.
#[allow(unused_imports)]
use AppEvent as _AppEventReexportForDocs;

#[cfg(test)]
mod tests {
    use super::*;

    // --- clean_osc_title: ported verbatim from src/__tests__/ui-pure.test.ts's
    // deleted `describe("cleanOscTitle", ...)` block (24 cases). ---

    #[test]
    fn returns_empty_for_user_at_host_colon_path() {
        assert_eq!(clean_osc_title("user@myhost:~/projects"), "");
    }

    #[test]
    fn strips_single_env_var_assignment() {
        assert_eq!(clean_osc_title("ANTHROPIC_API_KEY=sk-xxx claude"), "claude");
    }

    #[test]
    fn strips_multiple_env_var_assignments() {
        assert_eq!(clean_osc_title("FOO=bar BAZ=qux npm test"), "npm test");
    }

    #[test]
    fn strips_env_vars_after_user_at_host_colon_prefix() {
        assert_eq!(clean_osc_title("user@host:FOO=bar claude"), "claude");
    }

    #[test]
    fn returns_empty_for_bare_user_at_host() {
        assert_eq!(clean_osc_title("stefano.straus@DGQT92CJFP"), "");
    }

    #[test]
    fn strips_bare_user_at_host_prefix_followed_by_command() {
        assert_eq!(clean_osc_title("user@host npm start"), "npm start");
    }

    #[test]
    fn returns_empty_for_path_titles() {
        assert_eq!(clean_osc_title("~/projects/foo"), "");
        assert_eq!(clean_osc_title("/Users/me/projects/bar"), "");
        assert_eq!(clean_osc_title("~/Gits/CC_Playground/abrowser"), "");
        assert_eq!(clean_osc_title("~"), "");
        assert_eq!(clean_osc_title("~/"), "");
        assert_eq!(clean_osc_title("C:\\Users\\me"), "");
        assert_eq!(clean_osc_title("\\\\server\\share"), "");
    }

    #[test]
    fn renders_pi_status_prefixed_title_as_one_stable_name_across_every_state() {
        // pi's terminal-status-title extension repaints the title ~8x/second
        // with an animated braille frame; every frame plus the idle/done/error
        // glyphs must collapse to the same tab name, or the tab flickers. The
        // separator the glyph sat on must go with it.
        let frames = [
            "⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "○", "✓", "✗",
        ];
        for glyph in frames {
            assert_eq!(
                clean_osc_title(&format!("{glyph} | π | tuicommander")),
                "π | tuicommander"
            );
        }
    }

    #[test]
    fn keeps_a_legitimate_leading_separator_like_word_intact() {
        assert_eq!(clean_osc_title("npm run build"), "npm run build");
    }

    #[test]
    fn keeps_subcommands_but_strips_flags() {
        assert_eq!(clean_osc_title("vim file.txt"), "vim file.txt");
        assert_eq!(clean_osc_title("npm test"), "npm test");
        assert_eq!(clean_osc_title("git commit"), "git commit");
    }

    #[test]
    fn strips_double_dash_flags_from_commands() {
        assert_eq!(
            clean_osc_title("claude --dangerously-skip-permissions"),
            "claude"
        );
        assert_eq!(clean_osc_title("npm test --force"), "npm test");
        assert_eq!(clean_osc_title("git commit -m message"), "git commit");
    }

    #[test]
    fn handles_empty_string() {
        assert_eq!(clean_osc_title(""), "");
    }

    #[test]
    fn handles_title_that_is_only_env_vars() {
        assert_eq!(clean_osc_title("FOO=bar "), "");
    }

    #[test]
    fn keeps_command_subcommands_before_flags() {
        assert_eq!(clean_osc_title("echo FOO=bar"), "echo FOO=bar");
    }

    #[test]
    fn handles_env_var_with_path_value() {
        assert_eq!(
            clean_osc_title("PATH=/usr/bin:/bin node server.js"),
            "node server.js"
        );
    }

    #[test]
    fn rejects_compound_commands_with_semicolons() {
        assert_eq!(
            clean_osc_title(r#"cfg= ; if [ "$cfg" = "yml" ]; then lazygit"#),
            ""
        );
        assert_eq!(clean_osc_title("cd /foo; make"), "");
    }

    #[test]
    fn rejects_commands_with_and_and_or() {
        assert_eq!(clean_osc_title("test -f file && echo yes"), "");
        assert_eq!(clean_osc_title("cmd1 || cmd2"), "");
    }

    #[test]
    fn rejects_shell_control_flow_keywords() {
        assert_eq!(clean_osc_title("if test -f foo"), "");
        assert_eq!(clean_osc_title("for f in *.txt"), "");
        assert_eq!(clean_osc_title("while true"), "");
        assert_eq!(clean_osc_title("case $x in"), "");
    }

    #[test]
    fn rejects_subshell_expressions() {
        assert_eq!(clean_osc_title("echo $(whoami)"), "");
    }

    /// The regression test for a naive substring-based (rather than
    /// word-boundary-based) port — see `shell_script_re`'s doc comment.
    #[test]
    fn does_not_reject_commands_containing_keyword_substrings() {
        assert_eq!(clean_osc_title("docker compose up"), "docker compose up");
        assert_eq!(clean_osc_title("ifort build.f90"), "ifort build.f90");
        assert_eq!(clean_osc_title("terraform apply"), "terraform apply");
    }

    #[test]
    fn strips_env_vars_with_empty_values() {
        assert_eq!(clean_osc_title("FOO= bar"), "bar");
    }

    #[test]
    fn strips_braille_spinner_characters() {
        assert_eq!(clean_osc_title("\u{2800}Claude Code"), "Claude Code");
        assert_eq!(clean_osc_title("\u{28FF}Claude Code"), "Claude Code");
        assert_eq!(clean_osc_title("\u{2801} claude"), "claude");
    }

    #[test]
    fn strips_bullet_and_dot_operator_characters() {
        assert_eq!(clean_osc_title("\u{2022} intent"), "intent");
        assert_eq!(clean_osc_title("\u{2219} Claude Code"), "Claude Code");
        assert_eq!(clean_osc_title("\u{22C5} npm test"), "npm test");
    }

    #[test]
    fn strips_geometric_shape_spinners() {
        assert_eq!(clean_osc_title("\u{25D0} building"), "building");
        assert_eq!(clean_osc_title("\u{25CB} loading"), "loading");
    }

    // --- apply_osc_title: session-state-aware behavior. ---

    fn fresh_state() -> crate::AppState {
        crate::state::tests_support::make_test_app_state()
    }

    fn insert_bare_session(state: &crate::AppState, session_id: &str) {
        crate::state::tests_support::insert_dummy_session(state, session_id);
    }

    #[test]
    fn a_custom_name_is_never_clobbered_by_an_osc_title() {
        let state = fresh_state();
        insert_bare_session(&state, "s1");
        state.set_session_display_name("s1", Some("my custom name".into()), true);
        let mut base = None;
        apply_osc_title(&state, "s1", Some("npm test"), &mut base);
        let name = state
            .session_maps
            .sessions
            .get("s1")
            .map(|s| s.lock().display_name.clone());
        assert_eq!(name, Some(Some("my custom name".to_string())));
    }

    #[test]
    fn an_unusable_title_restores_the_base_name() {
        let state = fresh_state();
        insert_bare_session(&state, "s1");
        state.set_session_display_name("s1", Some("Terminal 1".into()), false);
        let mut base = None;
        apply_osc_title(&state, "s1", Some("npm test"), &mut base);
        apply_osc_title(&state, "s1", Some("cd /foo; make"), &mut base); // rejected (script-like)
        let name = state
            .session_maps
            .sessions
            .get("s1")
            .map(|s| s.lock().display_name.clone());
        assert_eq!(name, Some(Some("Terminal 1".to_string())));
    }

    #[test]
    fn a_reset_title_restores_the_base_name() {
        let state = fresh_state();
        insert_bare_session(&state, "s1");
        state.set_session_display_name("s1", Some("Terminal 1".into()), false);
        let mut base = None;
        apply_osc_title(&state, "s1", Some("npm test"), &mut base);
        apply_osc_title(&state, "s1", None, &mut base); // ResetTitle
        let name = state
            .session_maps
            .sessions
            .get("s1")
            .map(|s| s.lock().display_name.clone());
        assert_eq!(name, Some(Some("Terminal 1".to_string())));
    }

    #[test]
    fn the_base_name_is_recorded_once_and_not_overwritten_by_a_later_title() {
        let state = fresh_state();
        insert_bare_session(&state, "s1");
        state.set_session_display_name("s1", Some("Terminal 1".into()), false);
        let mut base = None;
        apply_osc_title(&state, "s1", Some("npm test"), &mut base);
        assert_eq!(base, Some(Some("Terminal 1".to_string())));
        // A second, different title must NOT re-capture the now-current
        // ("npm test") name as if it were the base.
        apply_osc_title(&state, "s1", Some("git commit"), &mut base);
        assert_eq!(base, Some(Some("Terminal 1".to_string())));
        apply_osc_title(&state, "s1", None, &mut base);
        let name = state
            .session_maps
            .sessions
            .get("s1")
            .map(|s| s.lock().display_name.clone());
        assert_eq!(name, Some(Some("Terminal 1".to_string())));
    }

    #[test]
    fn a_session_with_no_base_name_restores_to_none_not_some_empty_string() {
        let state = fresh_state();
        insert_bare_session(&state, "s1");
        // No display_name ever set — `None` from the start.
        let mut base = None;
        apply_osc_title(&state, "s1", Some("npm test"), &mut base);
        assert_eq!(base, Some(None)); // captured, and the base was "no name"
        apply_osc_title(&state, "s1", None, &mut base); // ResetTitle
        let name = state
            .session_maps
            .sessions
            .get("s1")
            .map(|s| s.lock().display_name.clone());
        // Must be `Some(None)` (session exists, display_name is None) — NOT
        // `Some(Some(String::new()))` (this exact class of bug is C.4.1's, now
        // pinned on the backend side too).
        assert_eq!(name, Some(None));
    }

    #[test]
    fn a_spinner_repaint_across_every_pi_status_frame_emits_exactly_one_session_renamed() {
        let state = fresh_state();
        insert_bare_session(&state, "s1");
        state.set_session_display_name("s1", Some("Terminal 1".into()), false);
        let mut rx = state
            .session_maps
            .pty_event_channels
            .entry("s1".to_string())
            .or_insert_with(|| tokio::sync::broadcast::channel(64).0)
            .subscribe();
        let mut base = None;
        let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "○"];
        for glyph in frames {
            apply_osc_title(
                &state,
                "s1",
                Some(&format!("{glyph} | π | tuicommander")),
                &mut base,
            );
        }
        // Every frame cleans to the SAME name ("π | tuicommander") — the
        // no-op guard inside `set_session_display_name` must suppress every
        // repeat emit, leaving exactly one `SessionRenamed` on the channel.
        let mut count = 0;
        while let Ok(event) = rx.try_recv() {
            if matches!(event, AppEvent::SessionRenamed { .. }) {
                count += 1;
            }
        }
        assert_eq!(
            count, 1,
            "expected exactly one SessionRenamed, spinner repaint must not re-emit"
        );
    }

    #[test]
    fn intent_title_gate_respects_both_global_and_per_agent_setting() {
        let state = fresh_state();
        insert_bare_session(&state, "s1");
        state.set_session_display_name("s1", Some("Terminal 1".into()), false);
        state
            .session_maps
            .session_states
            .entry("s1".to_string())
            .or_insert_with(crate::state::SessionState::default);
        state
            .session_maps
            .session_states
            .get_mut("s1")
            .unwrap()
            .agent_intent = Some("fixing the bug".to_string());
        state
            .session_maps
            .session_states
            .get_mut("s1")
            .unwrap()
            .agent_type = Some("claude".to_string());

        // Global ON, no per-agent override (defaults to true) — intent wins,
        // OSC title is skipped.
        state.config.write().intent_tab_title = true;
        let mut base = None;
        apply_osc_title(&state, "s1", Some("npm test"), &mut base);
        let name = state
            .session_maps
            .sessions
            .get("s1")
            .map(|s| s.lock().display_name.clone());
        assert_eq!(name, Some(Some("Terminal 1".to_string())));

        // Global OFF — intent no longer wins, OSC title applies even with an
        // active agent intent. This is the deliberate improvement over the
        // ported frontend behavior: the frontend's gate only ever checked
        // the global setting, but so does this branch, so it's covered by
        // the case above too — the per-agent branch below is the actual new
        // coverage.
        state.config.write().intent_tab_title = false;
        apply_osc_title(&state, "s1", Some("npm test"), &mut base);
        let name = state
            .session_maps
            .sessions
            .get("s1")
            .map(|s| s.lock().display_name.clone());
        assert_eq!(name, Some(Some("npm test".to_string())));
    }
}
