#[test]
fn pty_identity_defaults_claude_to_native_scrollback() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut cmd = CommandBuilder::new("claude");
    bind_pty_identity(&state, &mut cmd, "screen-default", None);
    assert_eq!(
        cmd.get_env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN"),
        Some(std::ffi::OsStr::new("1"))
    );
    cmd.env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN", "0");
    assert_eq!(
        cmd.get_env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN"),
        Some(std::ffi::OsStr::new("0"))
    );

    let mut ipc = CommandBuilder::new("claude");
    bind_pty_identity(&state, &mut ipc, "screen-ipc", None);
    let mut http = CommandBuilder::new("claude");
    let env = std::collections::HashMap::new();
    apply_agent_screen_env(&mut ipc, &env);
    apply_agent_screen_env(&mut http, &env);
    assert_eq!(
        ipc.get_env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN"),
        http.get_env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN")
    );
    assert_eq!(
        ipc.get_env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN"),
        Some(std::ffi::OsStr::new("1"))
    );
}

#[test]
fn spawned_claude_disables_the_alt_screen_agent_view() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut cmd = CommandBuilder::new("claude");
    bind_pty_identity(&state, &mut cmd, "agent-view-default", None);
    assert_eq!(
        cmd.get_env("CLAUDE_CODE_DISABLE_AGENT_VIEW"),
        Some(std::ffi::OsStr::new("1"))
    );
    // A caller's explicit value, applied afterwards, keeps precedence.
    cmd.env("CLAUDE_CODE_DISABLE_AGENT_VIEW", "0");
    assert_eq!(
        cmd.get_env("CLAUDE_CODE_DISABLE_AGENT_VIEW"),
        Some(std::ffi::OsStr::new("0"))
    );
}

#[test]
fn claude_screen_setting_off_preserves_explicit_environment() {
    let dir = tempfile::TempDir::new().unwrap();
    let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
    let mut config = crate::config::AgentsConfig::default();
    config.agents.insert(
        "claude".into(),
        crate::config::AgentSettings {
            prevent_alt_screen: Some(false),
            ..Default::default()
        },
    );
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), config).unwrap();
    let state = crate::state::tests_support::make_test_app_state();
    let mut cmd = CommandBuilder::new("claude");
    cmd.env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN", "custom");
    bind_pty_identity(&state, &mut cmd, "screen-setting-off", None);
    apply_agent_screen_env(&mut cmd, &std::collections::HashMap::new());
    assert_eq!(
        cmd.get_env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN"),
        Some(std::ffi::OsStr::new("custom"))
    );
}

#[cfg(unix)]
#[test]
fn prefill_agent_input_preserves_partial_input_and_accumulates_grabs() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let bytes = insert_recording_session(&state, "agent-prefill");
    agent_session(&state, "agent-prefill", SHELL_IDLE);
    crate::mcp_http::session::write_pty_input(&state, "agent-prefill", "my note ").unwrap();
    prefill_agent_input(&state, "agent-prefill", "<first>\n").unwrap();
    prefill_agent_input(&state, "agent-prefill", "<second>\n").unwrap();
    assert_eq!(
        *bytes.lock().unwrap(),
        b"my note \x1b[200~<first>\n\x1b[201~\x1b[200~<second>\n\x1b[201~"
    );
}

#[test]
fn test_parse_signal_number_killed() {
    assert_eq!(parse_signal_number("Killed: 9"), 9);
}

#[test]
fn test_parse_signal_number_interrupt() {
    assert_eq!(parse_signal_number("Interrupt: 2"), 2);
}

#[test]
fn test_parse_signal_number_format_variant() {
    assert_eq!(parse_signal_number("Signal 15"), 15);
}

#[test]
fn test_parse_signal_number_unknown() {
    assert_eq!(parse_signal_number("unknown signal"), 0);
}

/// Catches: `not_managed_agent` for an idle terminal ego (story 1080-a62f).
#[cfg(unix)]
#[test]
fn test_submit_to_idle_terminal_ego_is_accepted() {
    let state = crate::state::tests_support::make_test_app_state();
    insert_recording_session(&state, "ego-tab");
    agent_session(&state, "ego-tab", SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut("ego-tab")
        .unwrap()
        .agent_type = Some("ego".into());
    assert_eq!(agent_submission_rejection(&state, "ego-tab", false), None);
}

// --- classify_shell tests (story 1274-2e38) ---

#[test]
fn classify_shell_bare_posix_basenames() {
    for s in [
        "sh", "bash", "zsh", "fish", "dash", "ksh", "ash", "tcsh", "csh", "mksh",
    ] {
        assert_eq!(classify_shell(s), ShellFamily::Posix, "{s}");
    }
}

#[test]
fn classify_shell_absolute_posix_paths() {
    for s in [
        "/bin/bash",
        "/usr/bin/zsh",
        "/opt/homebrew/bin/fish",
        "/usr/local/bin/sh",
    ] {
        assert_eq!(classify_shell(s), ShellFamily::Posix, "{s}");
    }
}

#[test]
fn classify_shell_windows_native() {
    for s in [
        "cmd",
        "cmd.exe",
        "C:\\Windows\\System32\\cmd.exe",
        "powershell",
        "powershell.exe",
        "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
        "pwsh",
        "pwsh.exe",
    ] {
        assert_eq!(classify_shell(s), ShellFamily::WindowsNative, "{s}");
    }
}

/// Critical regression case for story 1274-2e38: Git Bash / Cygwin / MSYS
/// ship `bash.exe` on Windows and DO support Ctrl-U. Classifying by host
/// OS would wrongly skip the prefix here; classifying by shell basename
/// correctly keeps them in the Posix family.
#[test]
fn classify_shell_git_bash_on_windows_is_posix() {
    for s in [
        "bash.exe",
        "C:\\Program Files\\Git\\bin\\bash.exe",
        "C:\\Program Files\\Git\\usr\\bin\\bash.exe",
        "C:/Program Files/Git/bin/bash.exe",
        "C:\\cygwin64\\bin\\bash.exe",
        "C:\\msys64\\usr\\bin\\bash.exe",
    ] {
        assert_eq!(classify_shell(s), ShellFamily::Posix, "{s}");
    }
}

#[test]
fn classify_shell_wsl_is_posix() {
    for s in [
        "wsl",
        "wsl.exe",
        "wsl.exe -d Ubuntu",
        "C:\\Windows\\System32\\wsl.exe",
    ] {
        assert_eq!(classify_shell(s), ShellFamily::Posix, "{s}");
    }
}

#[test]
fn classify_shell_case_insensitive() {
    assert_eq!(classify_shell("BASH.EXE"), ShellFamily::Posix);
    assert_eq!(classify_shell("Cmd.Exe"), ShellFamily::WindowsNative);
    assert_eq!(classify_shell("PowerShell.exe"), ShellFamily::WindowsNative);
}

#[test]
fn classify_shell_ignores_trailing_arguments() {
    // Arguments after the first whitespace must not affect classification.
    assert_eq!(classify_shell("bash --login"), ShellFamily::Posix);
    assert_eq!(
        classify_shell("powershell.exe -NoProfile"),
        ShellFamily::WindowsNative
    );
}

#[test]
fn classify_shell_unknown_for_other_binaries() {
    // Intentionally unknown — callers should fall back to a safe default.
    for s in ["python", "node", "/usr/bin/env", "", "   "] {
        assert_eq!(classify_shell(s), ShellFamily::Unknown, "{s:?}");
    }
}

#[test]
fn test_tool_error_different_line_fires_after_first() {
    let mut s = SilenceState::new();
    s.mark_tool_error_candidate("Error: Exit code 1".to_string());
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    let _ = s.check_tool_error();
    s.clear_tool_error_on_recovery();

    // A different error appears in a later turn — must still fire.
    s.mark_tool_error_candidate("Error: Exit code 128".to_string());
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_tool_error(),
        Some("Error: Exit code 128".to_string()),
        "distinct error text must not be suppressed by prior surface"
    );
}

#[test]
fn test_api_retry_blocks_ready_screen_confirm() {
    // Claude Code keeps its `❯` prompt visible while auto-retrying, so a stable
    // ready screen would otherwise confirm idle after AGENT_READY_CONFIRM. The
    // retry hold must refuse that confirmation.
    let mut s = SilenceState::new();
    s.mark_api_retry();
    // Force the ready prompt to look long-stable.
    s.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM * 2);
    assert!(
        !s.note_ready_screen(),
        "ready screen must not confirm idle while an API retry is in flight"
    );

    // Once the hold expires, the same stable ready prompt confirms idle.
    s.api_retry_hold_until = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));
    s.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM * 2);
    assert!(
        s.note_ready_screen(),
        "ready screen confirms idle after the retry hold expires"
    );
}

#[test]
fn test_tool_error_mark_is_idempotent_while_pending() {
    let mut s = SilenceState::new();
    s.mark_tool_error_candidate("Error: Exit code 1".to_string());
    // Second mark for the same line while still pending → no-op.
    s.mark_tool_error_candidate("Error: Exit code 1".to_string());
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(s.check_tool_error(), Some("Error: Exit code 1".to_string()));
}

// --- Suggest backend-gating tests ---

#[test]
fn test_suggest_drain_returns_parked_items() {
    let mut s = SilenceState::new();
    s.mark_suggest_candidate(vec!["alpha".to_string(), "beta".to_string()], 0);
    assert_eq!(
        s.drain_pending_suggest(),
        Some(vec!["alpha".to_string(), "beta".to_string()])
    );
}

#[test]
fn test_suggest_newer_items_overwrite_older() {
    let mut s = SilenceState::new();
    s.mark_suggest_candidate(vec!["old".to_string()], 0);
    s.mark_suggest_candidate(vec!["new1".to_string(), "new2".to_string()], 0);
    assert_eq!(
        s.drain_pending_suggest(),
        Some(vec!["new1".to_string(), "new2".to_string()]),
        "latest parked set must win (agent updated suggestions mid-turn)"
    );
}

#[test]
fn test_suggest_empty_items_ignored() {
    let mut s = SilenceState::new();
    s.mark_suggest_candidate(vec![], 0);
    assert!(
        s.pending_suggest_items.is_none(),
        "empty items must not park"
    );
    assert!(s.drain_pending_suggest().is_none());
}

#[test]
fn test_tool_error_suppressed_while_spinner_active() {
    let mut s = SilenceState::new();
    s.mark_tool_error_candidate("Error: Exit code 2".to_string());
    s.last_status_line_at = Some(std::time::Instant::now());
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(
        s.check_tool_error().is_none(),
        "spinner active means agent still working — no notification"
    );
}

#[test]
fn test_silence_state_pending_after_threshold() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    // Simulate time passing by backdating last_output_at
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(s.check_silence(), Some("Continue?".to_string()));
}

#[test]
fn test_silence_state_regex_clears_prior_pending() {
    let mut s = SilenceState::new();
    // Silence detector has a pending question from an earlier chunk
    s.on_chunk(
        false,
        Some("Earlier question?".to_string()),
        false,
        false,
        false,
    );
    assert!(s.pending_question_line.is_some());
    // Regex fires on a different event — no question line in this chunk
    s.on_chunk(true, None, false, false, false);
    assert!(
        s.pending_question_line.is_none(),
        "prior pending should be cleared when regex fires"
    );
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(s.check_silence().is_none());
}

#[test]
fn test_silence_state_non_question_output_preserves_pending() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    // Non-`?` output (spinners, prompts, decorations) must NOT clear pending.
    s.on_chunk(false, None, false, false, false);
    s.on_chunk(false, None, false, false, false);
    s.on_chunk(false, None, false, false, false);
    // Standard 10s threshold fires normally
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(s.check_silence(), Some("Continue?".to_string()));
}

#[test]
fn test_silence_state_new_question_replaces_old() {
    let mut s = SilenceState::new();
    s.on_chunk(
        false,
        Some("First question?".to_string()),
        false,
        false,
        false,
    );
    s.on_chunk(
        false,
        Some("Second question?".to_string()),
        false,
        false,
        false,
    );
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(s.check_silence(), Some("Second question?".to_string()));
}

#[test]
fn test_silence_state_suppress_echo_expires() {
    let mut s = SilenceState::new();
    s.suppress_user_input();
    // Expire the echo suppress window with a past deadline (not None,
    // which means "never suppressed" — a different code path).
    s.suppress_echo_until = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));
    // Agent asks a genuine question after the window expires
    s.on_chunk(
        false,
        Some("Would you like to proceed?".to_string()),
        false,
        false,
        false,
    );
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Should fire — this is a real agent question
    assert_eq!(
        s.check_silence(),
        Some("Would you like to proceed?".to_string())
    );
}

#[test]
fn test_silence_state_spinner_suppresses_question() {
    let mut s = SilenceState::new();
    // Agent prints a `?`-line alongside a status-line/spinner in the same chunk
    s.on_chunk(
        false,
        Some("Want me to proceed?".to_string()),
        true,
        false,
        false,
    );
    // Simulate 10s+ of silence
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Should NOT emit question — spinner was recently active
    assert_eq!(s.check_silence(), None, "spinner active → no question");
}

#[test]
fn test_silence_state_spinner_expired_allows_question() {
    let mut s = SilenceState::new();
    s.on_chunk(
        false,
        Some("Want me to proceed?".to_string()),
        true,
        false,
        false,
    );
    // Spinner was active but long ago (>10s, matching SILENCE_QUESTION_THRESHOLD)
    s.last_status_line_at = Some(std::time::Instant::now() - std::time::Duration::from_secs(12));
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Spinner expired, question should fire
    assert_eq!(s.check_silence(), Some("Want me to proceed?".to_string()));
}

#[test]
fn test_silence_state_spinner_within_10s_suppresses() {
    let mut s = SilenceState::new();
    s.on_chunk(
        false,
        Some("Want me to proceed?".to_string()),
        true,
        false,
        false,
    );
    // Spinner was 8s ago — still within the 10s window
    s.last_status_line_at = Some(std::time::Instant::now() - std::time::Duration::from_secs(8));
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_silence(),
        None,
        "spinner within 10s should suppress question"
    );
}

// --- Status-line-only chunk tests ---

#[test]
fn test_silence_state_status_line_only_does_not_reset_silence() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    // Backdate last_output_at to simulate 10s of silence
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Mode-line timer tick: status_line_only = true, should NOT reset last_output_at
    s.on_chunk(false, None, true, true, false);
    // The silence threshold should still be met
    assert_eq!(
        s.check_silence(),
        Some("Continue?".to_string()),
        "status_line_only chunks must not reset the silence timer"
    );
}

#[test]
fn test_silence_state_mode_line_ticks_do_not_suppress_question() {
    // Reproduces the bug: Claude Code asks a question, then the mode line
    // keeps updating every ~1s while waiting for input. Status-line-only chunks
    // were keeping `is_spinner_active()` true forever, preventing question
    // detection even after 10s of silence.
    let mut s = SilenceState::new();
    // Agent outputs question + status line in same chunk (not status-line-only)
    s.on_chunk(
        false,
        Some("Vuoi fare un commit?".to_string()),
        true,
        false,
        false,
    );

    // Simulate 10s+ passing: both last_output_at and last_status_line_at
    // age beyond the threshold (in real life, wall-clock time handles this).
    let past = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    s.last_output_at = past;
    s.last_status_line_at = Some(past);

    // Mode-line-only ticks keep coming — they must NOT refresh either timer.
    for _ in 0..10 {
        s.on_chunk(false, None, true, true, false);
    }

    // After 10s+ of silence, the question MUST be detected even though
    // mode-line ticks kept coming in.
    assert_eq!(
        s.check_silence(),
        Some("Vuoi fare un commit?".to_string()),
        "mode-line-only ticks must not keep is_spinner_active() alive"
    );
}

#[test]
fn test_silence_state_mode_line_ticks_do_not_stale_question() {
    // Regression: mode-line timer ticks (status_line_only=true) were incrementing
    // output_chunks_after_question, clearing the pending question as "stale"
    // before the silence timer could detect it.
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Procedo?".to_string()), true, false, false);

    // Simulate 15 mode-line ticks (> STALE_QUESTION_CHUNKS=10)
    for _ in 0..15 {
        s.on_chunk(false, None, true, true, false);
    }

    // pending_question_line must still be present — mode-line ticks are not real output
    assert_eq!(
        s.pending_question_line.as_deref(),
        Some("Procedo?"),
        "mode-line-only ticks must not count toward staleness"
    );

    // Backdate to simulate silence threshold reached
    let past = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    s.last_output_at = past;
    s.last_status_line_at = Some(past);

    assert_eq!(
        s.check_silence(),
        Some("Procedo?".to_string()),
        "question must be detectable after mode-line-only ticks"
    );
}

#[test]
fn test_silence_state_regular_chunk_resets_silence() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    // Backdate to simulate 10s silence
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Regular (non-status-line) chunk resets the timer
    s.on_chunk(false, None, false, false, false);
    // Now we need to wait another 10s — should NOT fire yet
    assert_eq!(
        s.check_silence(),
        None,
        "regular chunk should reset silence timer"
    );
}

#[test]
fn test_silence_state_suggest_only_does_not_stale_question() {
    // A suggest-only chunk (protocol token, not real output) must not
    // increment output_chunks_after_question or reset the silence timer.
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    // 15 suggest-only chunks — should NOT stale the pending question
    for _ in 0..15 {
        s.on_chunk(false, None, false, false, true);
    }
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_silence(),
        Some("Continue?".to_string()),
        "suggest-only chunks must not count toward question staleness"
    );
}

/// The rank decision behind every `*_recovers_long_lived_shell_busy` test
/// (#745-8ff1), pinned directly so the next person to change it fails a test
/// rather than a review.
///
/// OSC 133 is shell integration, not agent instrumentation: `133;C` fires when
/// a foreground command starts and `133;D` when it exits, so on a long-lived
/// TUI agent it is set once at launch and cleared only at death. It is
/// process-granularity evidence that knows nothing about turns, so it records
/// at Screen rank and a stable Ready screen is allowed to close it. An agent
/// hook (OSC 7770) does know about turns, records at Protocol rank, and holds.
///
/// Rank is about what the evidence knows, not how it travelled — arriving in an
/// escape sequence does not make something Protocol rank.
#[test]
fn osc133_busy_is_screen_rank_and_yields_to_a_ready_screen() {
    let aged = || Some(std::time::Instant::now() - AGENT_READY_CONFIRM);

    // Shell integration: Screen rank, and the Ready screen closes the turn.
    let mut shell = SilenceState::new();
    shell.note_explicit_state(SHELL_BUSY, false);
    assert_eq!(
        shell.evidence.busy.map(|b| (b.rank, b.source)),
        Some((EvidenceRank::Screen, "osc133-busy")),
        "OSC 133 knows a process started, never that a turn started"
    );
    shell.note_real_activity();
    shell.screen_ready_pending_since = aged();
    assert!(
        shell.note_ready_screen(),
        "a stable Ready screen must close an osc133-held turn (#535-d4f5)"
    );
    assert!(shell.idle_confirmed());

    // Agent hook: Protocol rank, and the identical Ready screen does NOT close it.
    let mut hooked = SilenceState::new();
    hooked.note_explicit_state(SHELL_BUSY, true);
    assert_eq!(
        hooked.evidence.busy.map(|b| (b.rank, b.source)),
        Some((EvidenceRank::Protocol, "hook-busy")),
        "an agent hook is turn-granular and outranks the screen"
    );
    hooked.note_real_activity();
    hooked.screen_ready_pending_since = aged();
    assert!(
        !hooked.note_ready_screen(),
        "a Ready screen must never close a turn a protocol signal holds"
    );
    assert!(!hooked.idle_confirmed());

    // `explicit_busy()` spans both ranks on purpose: it reports provenance
    // (an explicit marker set this), NOT authority. Anything deciding whether
    // evidence may hold a turn must read the rank instead.
    let mut provenance = SilenceState::new();
    provenance.note_explicit_state(SHELL_BUSY, false);
    assert!(provenance.explicit_busy());
    assert_eq!(
        provenance.evidence.busy.map(|b| b.rank),
        Some(EvidenceRank::Screen),
        "explicit_busy() is true here at Screen rank — it is not a rank test"
    );
}

#[test]
fn test_agent_ready_requires_stable_observation() {
    let mut silence = SilenceState::new();
    assert!(!silence.note_ready_screen());
    assert!(!silence.idle_confirmed());
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(silence.idle_confirmed());
}

#[test]
fn test_grok_ready_composer_recovers_long_lived_shell_busy() {
    let rows = vec![
        "Finished the response.".to_string(),
        "❯ Ask anything".to_string(),
        "⌘ Grok 4.3 OpenRouter · Medium effort".to_string(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("grok"), &rows),
        AgentScreenActivity::Ready
    );

    let mut silence = SilenceState::new();
    // OSC 133 marks the long-lived `grok` shell command busy. Without a
    // Grok ready-screen adapter this bit survived for the whole process.
    //
    // These three assertions were inverted by e17c79b8 and restored by
    // #745-8ff1. They are byte-identical in setup to the goose and opencode
    // cases below/above, and a `SilenceState` carries no agent type, so all
    // three MUST agree — see `osc133_busy_is_screen_rank_and_yields_to_a_ready_screen`
    // for the rank decision they rest on. If you are here because one of the
    // three is red, the answer is never to make this trio disagree again.
    silence.note_explicit_state(SHELL_BUSY, false);
    silence.note_real_activity();
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(!silence.explicit_busy());
    assert!(silence.idle_confirmed());
}

#[test]
fn test_opencode_ready_screen_recovers_long_lived_shell_busy() {
    assert_eq!(classify_agent("opencode"), Some("opencode"));
    assert!(has_ready_screen_adapter(Some("opencode")));

    let mut silence = SilenceState::new();
    // OSC 133 marks the long-lived `opencode` foreground command busy. Without a
    // ready-screen adapter this bit survived for the whole process (#535-d4f5).
    silence.note_explicit_state(SHELL_BUSY, false);
    silence.note_real_activity();
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(!silence.explicit_busy());
    assert!(silence.idle_confirmed());
}

#[test]
fn protocol_submission_requires_post_submit_consumption_before_ready_screen() {
    let mut silence = SilenceState::new();
    silence.note_user_submission(true);
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(!silence.note_ready_screen());
    assert!(silence.explicit_busy());
    assert!(!silence.idle_confirmed());

    silence.note_real_activity();
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(!silence.explicit_busy());
    assert!(silence.idle_confirmed());
}

#[test]
fn stable_ready_prompt_does_not_recover_a_missed_hook_idle() {
    let mut silence = SilenceState::new();
    silence.note_user_submission(true);
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.note_real_activity();
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(!silence.note_ready_screen());
    assert!(silence.explicit_busy());
    assert!(silence.hook_busy());
    assert!(!silence.idle_confirmed());
}

#[test]
fn test_hook_busy_cannot_be_overridden_before_turn_activity() {
    let mut silence = SilenceState::new();
    silence.note_user_submission(true);
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(!silence.note_ready_screen());
    assert!(silence.explicit_busy());
    assert!(silence.hook_busy());
    assert!(!silence.idle_confirmed());
}

#[test]
fn test_fresh_hook_busy_blocks_ready_after_prior_recovery() {
    let mut silence = SilenceState::new();
    silence.note_user_submission(true);
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.note_real_activity();
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(!silence.note_ready_screen());

    silence.note_explicit_state(SHELL_BUSY, true);
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(!silence.note_ready_screen());
    assert!(silence.explicit_busy());
    assert!(silence.hook_busy());
    assert!(!silence.idle_confirmed());
}

#[test]
fn screen_only_submission_keeps_ready_adapter_fallback() {
    let mut silence = SilenceState::new();
    silence.note_user_submission(false);
    silence.note_real_activity();
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(silence.idle_confirmed());
}

#[test]
fn protocol_busy_requires_both_old_signal_and_old_output_to_be_stale() {
    let mut silence = SilenceState::new();
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.evidence.busy.as_mut().unwrap().at = std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT);
    assert!(
        !silence.protocol_busy_is_stale(),
        "fresh output keeps the hold"
    );
    silence.last_output_at = std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
    assert!(silence.protocol_busy_is_stale());
}

#[test]
fn protocol_stale_recovery_requires_a_ready_screen_for_the_full_timeout() {
    let mut silence = SilenceState::new();
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.evidence.busy.as_mut().unwrap().at = std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
    silence.last_output_at = std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
    assert!(!silence.protocol_busy_is_stale(), "no Ready observation");
    assert!(
        !silence.note_ready_screen(),
        "the first Ready starts the clock"
    );
    assert!(!silence.protocol_busy_is_stale(), "fresh Ready observation");
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT);
    assert!(silence.protocol_busy_is_stale());
}

#[test]
fn protocol_busy_parks_suggest_without_downgrading_working_screen() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "protocol-suggest";
    agent_session(&state, session_id, SHELL_BUSY);
    let lifecycle = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
    {
        let mut lifecycle = lifecycle.lock();
        lifecycle.note_explicit_state(SHELL_BUSY, true);
        lifecycle.mark_suggest_candidate(vec!["Review diff".into()], 0);
    }
    assert_eq!(
        completion_adjusted_screen_activity(
            &state,
            &lifecycle,
            session_id,
            AgentScreenActivity::Working,
        ),
        AgentScreenActivity::Working
    );
    assert_eq!(
        lifecycle.lock().drain_pending_suggest(),
        Some(vec!["Review diff".into()])
    );
}

/// #745-8ff1 AC1, at the level the rule actually has to hold: the silence
/// timer.
///
/// The unit tests around `note_ready_screen` prove a Ready *screen* cannot
/// close a protocol-held turn, but the timer is a second, independent way in —
/// it reaches `IdleDecision` through `try_timer_idle_transition` and can idle a
/// session with no screen evidence at all. The guard is
/// `silence.explicit_busy() && !nested_prompt` (pty.rs:4305); nothing asserted
/// it, so removing it would have left every `note_ready_screen` test green
/// while a hook-held turn quietly went idle on the timer.
///
/// Both ways in are checked: a Ready screen the protocol hold refuses to
/// confirm, and no screen classification at all, which is the pure silence
/// path.
#[test]
fn the_silence_timer_cannot_idle_a_protocol_held_turn() {
    for screen in [AgentScreenActivity::Ready, AgentScreenActivity::Unknown] {
        let state = crate::state::tests_support::make_test_app_state();
        let session_id = "protocol-held-timer";
        agent_session(&state, session_id, SHELL_BUSY);
        state
            .session_maps
            .session_states
            .get_mut(session_id)
            .unwrap()
            .agent_type = Some("codex".into());
        let silence = state
            .session_maps
            .silence_states
            .get(session_id)
            .unwrap()
            .clone();
        {
            let mut silence = silence.lock();
            silence.note_explicit_state(SHELL_BUSY, true);
            assert!(silence.hook_busy(), "precondition: Protocol-rank busy");
            // Aged well past the confirm window: the turn is held by rank, not
            // by the screen being too fresh to trust.
            silence.screen_ready_pending_since =
                Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
        }

        let transition =
            try_timer_idle_transition(&state, &silence, session_id, screen, Some("codex"), Some(0));

        assert!(
            !transition.transitioned,
            "{screen:?}: the timer must not close a turn a protocol signal holds"
        );
        assert_eq!(
            state
                .session_maps
                .shell_states
                .get(session_id)
                .unwrap()
                .load(std::sync::atomic::Ordering::Acquire),
            SHELL_BUSY,
            "{screen:?}: session must still read BUSY"
        );
        assert!(
            silence.lock().hook_busy(),
            "{screen:?}: the protocol evidence must survive the timer tick"
        );
    }
}

#[test]
fn test_explicit_busy_suppresses_silence_question_until_idle() {
    let mut silence = SilenceState::new();
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD;
    silence.pending_question_line = Some("Continue?".into());
    assert!(!silence.is_silent());
    silence.note_explicit_state(SHELL_IDLE, true);
    assert!(silence.is_silent());
}

fn replay_sanitized_agent_trace(agent: &str, steps: &[SanitizedTraceStep]) -> SilenceState {
    let mut silence = SilenceState::new();
    silence.confirm_idle();
    for step in steps {
        match step {
            SanitizedTraceStep::Submit => silence.note_user_submission(true),
            SanitizedTraceStep::RealActivity => silence.note_real_activity(),
            SanitizedTraceStep::WorkingScreen => {
                let rows = if agent == "codex" {
                    vec!["• Working".to_string(), "› sanitized prompt".to_string()]
                } else {
                    vec!["sanitized animated output".to_string()]
                };
                if detect_agent_screen_activity(Some(agent), &rows) == AgentScreenActivity::Working
                {
                    silence.note_working_screen();
                }
            }
            SanitizedTraceStep::ReadyScreen => {
                let rows = match agent {
                    "codex" => vec!["sanitized final".into(), "› sanitized prompt".into()],
                    "claude" => vec!["sanitized final".into(), "❯".into()],
                    "gemini" => vec!["> Type your message".into()],
                    "aider" => vec![">".into()],
                    _ => Vec::new(),
                };
                if detect_agent_screen_activity(Some(agent), &rows) == AgentScreenActivity::Ready {
                    silence.screen_ready_pending_since =
                        Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
                    silence.note_ready_screen();
                }
            }
            SanitizedTraceStep::UnknownScreen => silence.note_unknown_screen(),
        }
    }
    silence
}

/// Build a codex agent session held BUSY by a Protocol-rank submitted line
/// that has since produced output, with a ready screen already stable for
/// `AGENT_READY_CONFIRM`. That is the exact state in which the foreground
/// probe — and nothing else — decides whether the turn ends (#771-4733).
fn probe_evidence_fixture(
    state: &crate::state::AppState,
    session_id: &str,
) -> std::sync::Arc<parking_lot::Mutex<SilenceState>> {
    agent_session(state, session_id, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .agent_type = Some("codex".into());
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
    {
        let mut sl = silence.lock();
        sl.note_user_submission(true);
        sl.note_real_activity();
        sl.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    }
    silence
}

#[test]
fn explicit_agent_idle_waits_for_newer_snapshot_and_repairs_working() {
    for (session_id, hook_state) in [
        ("background-hook-idle", true),
        ("background-osc133-idle", false),
    ] {
        let state = crate::state::tests_support::make_test_app_state();
        let parent_id = format!("{session_id}-parent");
        agent_session(&state, session_id, SHELL_BUSY);
        state
            .session_maps
            .session_states
            .get_mut(session_id)
            .unwrap()
            .agent_type = Some("codex".into());
        state
            .session_maps
            .session_parent
            .insert(session_id.to_string(), parent_id.clone());
        state.agent_inbox.entry(parent_id.clone()).or_default();
        state
            .process_snapshot_cache
            .store(Some(vec![process(10, 1, "codex", "codex")]));

        transition_explicit_shell_state_with_hook(
            &state,
            session_id,
            SHELL_IDLE,
            "idle",
            hook_state,
            || {},
        );

        assert_eq!(
            state
                .session_maps
                .shell_states
                .get(session_id)
                .unwrap()
                .load(Ordering::Acquire),
            SHELL_IDLE
        );
        assert!(state.agent_inbox.get(&parent_id).unwrap().is_empty());
        assert!(!refresh_background_work_from_cached_snapshot(
            &state,
            session_id,
            10,
            "codex",
            0,
            state.process_snapshot_cache.load(),
        ));
        assert!(state.agent_inbox.get(&parent_id).unwrap().is_empty());

        state.process_snapshot_cache.store(Some(vec![
            process(10, 1, "codex", "codex"),
            process(11, 10, "cargo", "cargo test --locked"),
        ]));
        assert!(refresh_background_work_from_cached_snapshot(
            &state,
            session_id,
            10,
            "codex",
            0,
            state.process_snapshot_cache.load(),
        ));
        let snapshot = state.session_state_with_shell(session_id).unwrap();
        assert_eq!(snapshot.shell_state.as_deref(), Some("idle"));
        assert_eq!(snapshot.agent_state.as_deref(), Some("working"));
        assert!(snapshot.background_work);
        assert!(state.agent_inbox.get(&parent_id).unwrap().is_empty());
    }
}

#[test]
fn explicit_non_agent_idle_keeps_immediate_shell_semantics() {
    for (session_id, hook_state) in [("plain-hook-idle", true), ("plain-osc133-idle", false)] {
        let state = crate::state::tests_support::make_test_app_state();
        state.session_maps.session_states.insert(
            session_id.to_string(),
            crate::state::SessionState::default(),
        );
        state.session_maps.shell_states.insert(
            session_id.to_string(),
            std::sync::atomic::AtomicU8::new(SHELL_BUSY),
        );
        state.session_maps.silence_states.insert(
            session_id.to_string(),
            Arc::new(Mutex::new(SilenceState::new())),
        );

        transition_explicit_shell_state_with_hook(
            &state,
            session_id,
            SHELL_IDLE,
            "idle",
            hook_state,
            || {},
        );

        assert_eq!(
            state
                .session_maps
                .shell_states
                .get(session_id)
                .unwrap()
                .load(Ordering::Acquire),
            SHELL_IDLE
        );
        let session = state.session_maps.session_states.get(session_id).unwrap();
        assert_eq!(session.background_probe_turn_epoch, None);
        assert_eq!(session.background_probe_after_generation, None);
    }
}

// --- Staleness counter tests ---

#[test]
fn test_silence_state_stale_after_many_output_chunks() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    // Simulate 15 non-`?` chunks (well beyond STALE_QUESTION_CHUNKS)
    for _ in 0..15 {
        s.on_chunk(false, None, false, false, false);
    }
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_silence(),
        None,
        "stale question after many chunks should not fire"
    );
}

#[test]
fn test_silence_state_few_decoration_chunks_still_fires() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    // 3 decoration chunks (mode line, separator, prompt) — within threshold
    s.on_chunk(false, None, false, false, false);
    s.on_chunk(false, None, false, false, false);
    s.on_chunk(false, None, false, false, false);
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_silence(),
        Some("Continue?".to_string()),
        "few decoration chunks should still fire"
    );
}

#[test]
fn test_silence_state_counter_resets_on_new_question() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("First?".to_string()), false, false, false);
    // Many non-`?` chunks → stale
    for _ in 0..15 {
        s.on_chunk(false, None, false, false, false);
    }
    // New `?` line resets the counter
    s.on_chunk(false, Some("Second?".to_string()), false, false, false);
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_silence(),
        Some("Second?".to_string()),
        "new question should reset staleness"
    );
}

// --- Clear stale question tests ---

#[test]
fn test_silence_state_clear_stale_resets_pending() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    s.clear_stale_question();
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(s.check_silence(), None, "cleared stale should not fire");
}

#[test]
fn test_silence_state_clear_stale_allows_new_question() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Old?".to_string()), false, false, false);
    s.clear_stale_question();
    // New question after clear
    s.on_chunk(false, Some("New?".to_string()), false, false, false);
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_silence(),
        Some("New?".to_string()),
        "new question after clear should fire"
    );
}

#[test]
fn test_silence_state_repaint_same_question_does_not_refire() {
    let mut s = SilenceState::new();
    // Question arrives, silence fires, mark emitted
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(s.check_silence().is_some());
    assert!(s.question_already_emitted);

    // Terminal repaint: same `?` line re-appears as a changed row.
    // This must NOT reset question_already_emitted.
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    assert!(
        s.question_already_emitted,
        "repaint of same question must not reset emitted flag"
    );
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(
        s.check_silence().is_none(),
        "same question repaint must not re-fire"
    );
}

#[test]
fn test_silence_state_stale_same_question_scroll_does_not_refire() {
    let mut s = SilenceState::new();
    let past = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Question fires via chunk-based detection (Strategy 2)
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    s.last_output_at = past;
    assert!(s.check_silence().is_some());

    // Agent resumes: 15 non-`?` chunks (above STALE_QUESTION_CHUNKS)
    for _ in 0..15 {
        s.on_chunk(false, None, false, false, false);
    }
    assert!(
        s.pending_question_line.is_none(),
        "pending should be cleared by staleness"
    );

    // Same "Continue?" reappears in changed_rows because new output scrolled it
    // to a different row. This is NOT a new question — must not re-fire.
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    assert!(
        s.question_already_emitted,
        "scroll of previously emitted question must not reset emitted flag"
    );
    s.last_output_at = past;
    assert!(
        s.check_silence().is_none(),
        "same question text from scroll must not re-fire"
    );
}

#[test]
fn test_silence_state_stale_same_question_does_not_refire_after_user_input() {
    let mut s = SilenceState::new();
    let past = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Question fires
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    s.last_output_at = past;
    assert!(s.check_silence().is_some());

    // Agent resumes: 15 non-`?` chunks
    for _ in 0..15 {
        s.on_chunk(false, None, false, false, false);
    }

    // User provides input → new conversation cycle
    s.suppress_user_input();
    // Expire the echo suppression window so the next `?` line is not ignored
    s.suppress_echo_until = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));

    // The same historical row is moved by a repaint after the answer. Text
    // alone cannot prove that the agent asked it again, so it must remain
    // suppressed; current-turn screen position/protocol evidence owns a
    // genuinely repeated prompt.
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    s.last_output_at = past;
    assert!(
        s.check_silence().is_none(),
        "historical question repaint after user input must not re-arm awaiting"
    );
}

#[test]
fn test_silence_state_screen_emitted_question_scroll_does_not_refire() {
    let mut s = SilenceState::new();
    let past = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);

    // Question arrives in a chunk
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);

    // 15 non-? chunks → pending cleared by staleness
    for _ in 0..15 {
        s.on_chunk(false, None, false, false, false);
    }
    assert!(s.pending_question_line.is_none());

    // Silence timer (Strategy 1) finds "Continue?" on screen and emits.
    s.last_output_at = past;
    s.mark_emitted("Continue?");

    // New output causes scroll → same "Continue?" appears in changed_rows
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);

    // Must NOT reset question_already_emitted — it's a scroll artifact
    assert!(
        s.question_already_emitted,
        "scroll of screen-emitted question must not reset emitted flag"
    );
    s.last_output_at = past;
    assert!(
        !s.is_silent(),
        "same question after screen emission must not allow re-detection"
    );
}

#[test]
fn test_find_last_chat_question_rejects_user_prompt_line() {
    let rows: Vec<String> = vec![
        "❯ hai cambiato qualcosa?".into(),
        "────────────────────────────────────────────────".into(),
        "❯".into(),
        "────────────────────────────────────────────────".into(),
    ];
    assert_eq!(find_last_chat_question(&rows), None);
}

#[test]
fn test_find_last_chat_question_agent_question_after_user_input() {
    let rows: Vec<String> = vec![
        "❯ tell me about this".into(),
        "Would you like me to continue?".into(),
        "────────────────────────────────────────────────".into(),
        "❯".into(),
        "────────────────────────────────────────────────".into(),
    ];
    assert_eq!(
        find_last_chat_question(&rows),
        Some("Would you like me to continue?".to_string())
    );
}

#[test]
fn test_resize_grace_expires_after_threshold() {
    let mut s = SilenceState::new();
    s.on_resize();
    // Backdating the resize timestamp past the grace period
    s.last_resize_at =
        Some(std::time::Instant::now() - RESIZE_GRACE - std::time::Duration::from_millis(100));
    assert!(!s.is_resize_grace(), "grace period should have expired");
}

#[test]
fn test_resize_grace_refreshed_on_second_resize() {
    let mut s = SilenceState::new();
    s.on_resize();
    // Expire the first grace period
    s.last_resize_at =
        Some(std::time::Instant::now() - RESIZE_GRACE - std::time::Duration::from_millis(100));
    assert!(!s.is_resize_grace());
    // Second resize refreshes the timer
    s.on_resize();
    assert!(
        s.is_resize_grace(),
        "second resize should restart grace period"
    );
}

#[test]
fn test_startup_grace_safety_cap() {
    let mut s = SilenceState::new();
    // Created long ago, but output is recent — safety cap should force settle
    s.created_at =
        std::time::Instant::now() - STARTUP_GRACE_MAX - std::time::Duration::from_secs(1);
    s.last_output_at = std::time::Instant::now(); // output still flowing
    s.check_startup_settle();
    assert!(
        !s.is_startup_grace(),
        "startup grace should end at safety cap"
    );
}

#[test]
fn test_startup_grace_idempotent_after_settle() {
    let mut s = SilenceState::new();
    s.last_output_at =
        std::time::Instant::now() - STARTUP_SETTLE_SILENCE - std::time::Duration::from_millis(100);
    s.check_startup_settle();
    assert!(s.startup_settled);
    // Calling again doesn't change anything
    s.check_startup_settle();
    assert!(s.startup_settled);
}

/// End-to-end: VtLogBuffer → extract_question_line → SilenceState → check_silence.
/// Question + mode line arrive together → fires at 10s.
#[test]
fn test_e2e_question_detection_with_mode_line() {
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let mut silence = SilenceState::new();

    let changed = vt_log.process(b"Le committo?\r\n\r\n\xe2\x8f\xb5\xe2\x8f\xb5 Reading files");
    silence.on_chunk(false, extract_question_line(&changed), false, false, false);

    assert_eq!(
        silence.pending_question_line.as_deref(),
        Some("Le committo?")
    );

    silence.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(silence.check_silence(), Some("Le committo?".to_string()));
}

/// End-to-end: question in chunk 1, mode line in chunk 2, then silence.
/// Non-`?` output must NOT prevent the question from firing at 10s.
#[test]
fn test_e2e_question_then_decoration_then_silence() {
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let mut silence = SilenceState::new();

    let changed = vt_log.process(b"Le committo?");
    silence.on_chunk(false, extract_question_line(&changed), false, false, false);

    // Mode line / prompt decoration arrives in a separate chunk
    let changed = vt_log.process(b"\r\n\xe2\x8f\xb5\xe2\x8f\xb5 Idle");
    silence.on_chunk(false, extract_question_line(&changed), false, false, false);

    // 10s silence → fires
    silence.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(silence.check_silence(), Some("Le committo?".to_string()));
}

#[test]
fn test_backup_idle_blocked_by_chrome_only_ticks() {
    // Chrome-only ticks (status-line) MUST block the backup idle timer
    // because they prove the reader thread is active and the agent is alive.
    // Regression: f5c07388 changed has_recent_chunks() to use last_output_at,
    // which let the backup timer fire during tool calls (>3s of no real output
    // while status-line ticks every ~1s), causing false busy→idle oscillation.
    let mut silence = SilenceState::new();
    // Backdate real output to 5s ago (simulates a tool call in progress)
    silence.last_output_at = std::time::Instant::now() - std::time::Duration::from_secs(5);
    // Chrome-only tick just arrived (status-line timer tick)
    silence.on_chunk(false, None, true, true, false);
    assert!(
        silence.has_recent_chunks(),
        "backup idle MUST be blocked when chrome-only ticks are arriving — agent is alive"
    );
}

/// Kill and reap a probe child without blocking the test: `wait()` on a live
/// PTY child does not return while the pair is still open in this process.
fn reap(mut child: Box<dyn portable_pty::Child + Send + Sync>) {
    let _ = child.kill();
    for _ in 0..100 {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn probe_size() -> PtySize {
    PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }
}

#[test]
fn transient_allocation_recovers_and_uses_bounded_backoff() {
    let mut attempts = 0;
    let mut sleeps = Vec::new();
    let result = retry_transient(
        || {
            attempts += 1;
            if attempts < 3 {
                Err("busy")
            } else {
                Ok("pair")
            }
        },
        |_| true,
        |attempt| sleeps.push(attempt),
    );

    assert_eq!(result, Ok("pair"));
    assert_eq!(attempts, 3);
    assert_eq!(sleeps, vec![1, 2]);
}

#[test]
fn transient_allocation_stops_after_the_attempt_limit() {
    let mut attempts = 0;
    let result = retry_transient(
        || {
            attempts += 1;
            Err::<(), _>("busy")
        },
        |_| true,
        |_| {},
    );

    assert_eq!(result, Err((PTY_SPAWN_ATTEMPTS, "busy")));
    assert_eq!(attempts, PTY_SPAWN_ATTEMPTS);
}

#[cfg(unix)]
#[test]
fn pty_allocation_classifier_retries_resource_pressure_not_permissions() {
    let exhausted = anyhow::Error::new(std::io::Error::from_raw_os_error(libc::EMFILE));
    let denied = anyhow::Error::new(std::io::Error::from_raw_os_error(libc::EACCES));

    assert!(is_transient_pty_open_error(&exhausted));
    assert!(!is_transient_pty_open_error(&denied));
}

/// A spawn that works first time must be built exactly once.
#[test]
fn a_working_spawn_is_built_once() {
    let attempts = std::cell::Cell::new(0);
    let (_pair, child) = spawn_pty_pair_with_retry(probe_size(), || {
        attempts.set(attempts.get() + 1);
        trivial_command()
    })
    .expect("echo must spawn");

    assert_eq!(attempts.get(), 1);
    reap(child);
}

/// Read whatever a short-lived probe child writes to its pty, bounded so a
/// misbehaving probe can never hang the test suite. Mirrors the spawn-
/// thread + bounded-wait shape `tuic-hook`'s own `read_stdin_bounded` uses
/// for the identical reason (`crates/tuic-hook/src/main.rs:226-260`): the
/// read runs on a thread that is simply abandoned on timeout, never joined.
#[cfg(unix)]
// A handful of short lines from a test probe — not worth a bytecount
// dependency for what's already a single-digit byte count per call.
#[allow(clippy::naive_bytecount)]
fn read_pty_output_bounded(
    mut reader: Box<dyn std::io::Read + Send>,
    expected_lines: usize,
    timeout: std::time::Duration,
) -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut out = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    out.extend_from_slice(&buf[..n]);
                    // Stop as soon as the full expected payload has
                    // arrived, rather than waiting on EOF — EOF depends
                    // on every fd to the slave (including this test's
                    // own) already being closed.
                    if out.iter().filter(|&&b| b == b'\n').count() >= expected_lines {
                        break;
                    }
                }
            }
        }
        let _ = tx.send(out);
    });
    let bytes = rx.recv_timeout(timeout).unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// `bind_pty_identity` (:96) and `sanitize_pty_parent_env` (:72) are the two
/// functions that mutate a spawned child's environment before the
/// `TUIC_PTY_TTY` fix (F1) adds a third mutation to the same `CommandBuilder`
/// in `spawn_pty_pair_with_retry`. Neither had any direct coverage —
/// `bind_pty_identity`'s own doc comment describes an entire bug class it
/// exists to prevent, unverified. This observes the *real* child environment
/// through a live pty rather than inspecting the `CommandBuilder`, so it
/// cannot pass on a build-string bug that never actually reaches the process.
///
/// Also covers the `TUIC_PTY_TTY` fix itself (F1): unlike the other two
/// fields, this one isn't set inside `build_command()` — it's stamped by
/// `spawn_pty_pair_with_retry` itself after the closure returns — so this
/// is the only place that can observe it end-to-end. And an ordering
/// regression guard: `TUIC_PTY_TTY` is injected *after* every caller's own
/// `cmd.env(...)` calls, which is only safe because `CommandBuilder::env`
/// is a keyed insert — confirms an unrelated caller-set var survives
/// alongside it rather than the injection clobbering the whole map.
#[cfg(unix)]
#[test]
fn bind_pty_identity_env_reaches_the_real_child() {
    let state = crate::state::tests_support::make_test_app_state();

    let (pair, child) = spawn_pty_pair_with_retry(probe_size(), || {
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.env("NO_COLOR", "1"); // sanitize_pty_parent_env must strip this
        sanitize_pty_parent_env(&mut cmd);
        bind_pty_identity(&state, &mut cmd, "probe-session", Some("probe-identity"));
        // An unrelated caller-set var (mirrors spawn_config.env in
        // create_pty) must survive the later TUIC_PTY_TTY injection.
        cmd.env("TUIC_PROBE_UNRELATED", "caller-value");
        cmd.arg("-c");
        cmd.arg(
            r#"printf '%s\n%s\n%s\n%s\n%s\n' "$TUIC_SESSION" "$TUIC_CONFIG_DIR" "${NO_COLOR-unset}" "$TUIC_PTY_TTY" "$TUIC_PROBE_UNRELATED""#,
        );
        cmd
    })
    .expect("sh must spawn");

    let expected_tty = pair.master.tty_name();
    let reader = pair.master.try_clone_reader().expect("clone reader");
    // Our own copy of the slave must close so nothing but the child holds
    // it open — the same ordering `create_pty` relies on in production.
    drop(pair.slave);

    let output = read_pty_output_bounded(reader, 5, std::time::Duration::from_secs(5));
    let mut lines = output.lines();
    assert_eq!(
        lines.next(),
        Some("probe-identity"),
        "TUIC_SESSION must carry the caller's identity: {output:?}"
    );
    assert_eq!(
        lines.next(),
        Some(crate::config::config_dir().to_string_lossy().as_ref()),
        "TUIC_CONFIG_DIR must be set: {output:?}"
    );
    assert_eq!(
        lines.next(),
        Some("unset"),
        "sanitize_pty_parent_env must strip NO_COLOR: {output:?}"
    );
    let expected_tty = expected_tty.expect("tty_name() must resolve for a freshly opened pty");
    assert_eq!(
        lines.next(),
        Some(expected_tty.to_string_lossy().as_ref()),
        "TUIC_PTY_TTY must carry this pty's own device path so a detached hook \
         subprocess can find it without walking process ancestry: {output:?}"
    );
    assert_eq!(
        lines.next(),
        Some("caller-value"),
        "injecting TUIC_PTY_TTY must not clobber an unrelated caller-set env var: {output:?}"
    );

    reap(child);
}

/// Write a full `config.json` under `dir` — a fresh `AppConfig::default()` with
/// only `custom_pty_env` overridden. A *partial* JSON object would work too only
/// if every other field had a serde default; writing the whole struct avoids
/// relying on that and matches how `config.rs`'s own tests do this.
fn write_custom_pty_env_config(dir: &std::path::Path, entries: &[(&str, &str)]) {
    let cfg = crate::config::AppConfig {
        custom_pty_env: entries
            .iter()
            .map(|(k, v)| crate::config::CustomEnvVarEntry {
                key: k.to_string(),
                value: v.to_string(),
            })
            .collect(),
        ..crate::config::AppConfig::default()
    };
    std::fs::write(
        dir.join("config.json"),
        serde_json::to_string(&cfg).unwrap(),
    )
    .unwrap();
}

/// The load-bearing test for the `custom_pty_env` feature: proves an entry set in
/// `AppConfig` actually reaches a real spawned child's environment through
/// `spawn_pty_pair_with_retry`'s own injection (`apply_custom_pty_env`), not just
/// that the config round-trips or that the function compiles against a mock
/// `CommandBuilder`.
#[cfg(unix)]
#[test]
fn custom_pty_env_reaches_the_real_child() {
    let tmp = tempfile::tempdir().unwrap();
    let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
    write_custom_pty_env_config(tmp.path(), &[("TUIC_PROBE_CUSTOM", "custom-value")]);

    let (pair, child) = spawn_pty_pair_with_retry(probe_size(), || {
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.arg("-c");
        cmd.arg(r#"printf '%s\n' "${TUIC_PROBE_CUSTOM-unset}""#);
        cmd
    })
    .expect("sh must spawn");

    let reader = pair.master.try_clone_reader().expect("clone reader");
    drop(pair.slave);
    let output = read_pty_output_bounded(reader, 1, std::time::Duration::from_secs(5));
    assert_eq!(
        output.lines().next(),
        Some("custom-value"),
        "a configured custom_pty_env entry must reach the real child: {output:?}"
    );
    reap(child);
}

/// Proves the full precedence order documented on `spawn_pty_pair_with_retry`:
/// `custom_pty_env` overrides a caller-set var (the motivating "force this var"
/// use case), but the internal `TUIC_PTY_TTY` stamp always wins even if the user
/// names a custom entry after it.
#[cfg(unix)]
#[test]
fn custom_pty_env_overrides_caller_env_but_never_tuic_pty_tty() {
    let tmp = tempfile::tempdir().unwrap();
    let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
    write_custom_pty_env_config(
        tmp.path(),
        &[
            ("TUIC_PROBE_OVERRIDE", "custom-wins"),
            ("TUIC_PTY_TTY", "user-cannot-override-this"),
        ],
    );

    let (pair, child) = spawn_pty_pair_with_retry(probe_size(), || {
        let mut cmd = CommandBuilder::new("/bin/sh");
        // Mirrors a caller (e.g. PtyConfig::env) setting its own value first.
        cmd.env("TUIC_PROBE_OVERRIDE", "caller-value");
        cmd.arg("-c");
        cmd.arg(r#"printf '%s\n%s\n' "$TUIC_PROBE_OVERRIDE" "$TUIC_PTY_TTY""#);
        cmd
    })
    .expect("sh must spawn");

    let expected_tty = pair
        .master
        .tty_name()
        .expect("tty_name() must resolve for a freshly opened pty");
    let reader = pair.master.try_clone_reader().expect("clone reader");
    drop(pair.slave);
    let output = read_pty_output_bounded(reader, 2, std::time::Duration::from_secs(5));
    let mut lines = output.lines();
    assert_eq!(
        lines.next(),
        Some("custom-wins"),
        "custom_pty_env must override a caller-supplied env var: {output:?}"
    );
    assert_eq!(
        lines.next(),
        Some(expected_tty.to_string_lossy().as_ref()),
        "TUIC_PTY_TTY must never be overridable via custom_pty_env: {output:?}"
    );
    reap(child);
}

/// Regression guard for the specific "worst bypass" the plan named:
/// `POST /agents` (`agent_routes.rs`) builds its `CommandBuilder` with nothing but
/// `sanitize_pty_parent_env` — no identity, no worktree env, no shell integration.
/// This mirrors that exact minimal closure to prove `custom_pty_env` still reaches
/// the child even for the site that injects nothing else, since the injection
/// lives in the shared `spawn_pty_pair_with_retry` choke point rather than in any
/// per-site closure.
#[cfg(unix)]
#[test]
fn custom_pty_env_reaches_a_minimal_agent_binary_style_spawn() {
    let tmp = tempfile::tempdir().unwrap();
    let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
    write_custom_pty_env_config(tmp.path(), &[("TUIC_PROBE_MINIMAL", "reached")]);

    let (pair, child) = spawn_pty_pair_with_retry(probe_size(), || {
        let mut cmd = CommandBuilder::new("/bin/sh");
        sanitize_pty_parent_env(&mut cmd); // the entirety of POST /agents's own env work
        cmd.arg("-c");
        cmd.arg(r#"printf '%s\n' "${TUIC_PROBE_MINIMAL-unset}""#);
        cmd
    })
    .expect("sh must spawn");

    let reader = pair.master.try_clone_reader().expect("clone reader");
    drop(pair.slave);
    let output = read_pty_output_bounded(reader, 1, std::time::Duration::from_secs(5));
    assert_eq!(
        output.lines().next(),
        Some("reached"),
        "custom_pty_env must reach even the minimal-env agent-binary spawn shape: {output:?}"
    );
    reap(child);
}

#[cfg(unix)]
#[test]
fn custom_pty_env_skips_invalid_keys_but_applies_valid_ones() {
    let tmp = tempfile::tempdir().unwrap();
    let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
    write_custom_pty_env_config(
        tmp.path(),
        &[
            ("1BAD_START", "should-be-skipped"),
            ("BAD KEY", "should-be-skipped"),
            ("GOOD_KEY", "should-apply"),
        ],
    );

    let mut cmd = CommandBuilder::new("/bin/sh");
    apply_custom_pty_env(&mut cmd);
    assert!(
        cmd.get_env("1BAD_START").is_none(),
        "a key starting with a digit must be rejected"
    );
    assert!(
        cmd.get_env("BAD KEY").is_none(),
        "a key containing a space must be rejected"
    );
    assert_eq!(
        cmd.get_env("GOOD_KEY")
            .map(|v| v.to_string_lossy().to_string()),
        Some("should-apply".to_string()),
        "a structurally valid key must still be applied"
    );
}

/// `inject_worktree_env` (:111) is meant to be called at every
/// `bind_pty_identity` site — this observes the real child environment
/// through a live pty, same rationale as the identity test above: a
/// build-string bug in `ScriptContext::apply_pty` should fail here, not
/// just in `script_env.rs`'s own unit tests against a `Vec` of pairs.
#[cfg(unix)]
#[test]
fn worktree_env_reaches_the_real_child() {
    let repo = tempfile::TempDir::new().expect("temp dir");
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.email", "test@test.com"],
        vec!["config", "user.name", "Test"],
    ] {
        std::process::Command::new("git")
            .args(&args)
            .current_dir(repo.path())
            .output()
            .expect("git command");
    }
    std::fs::write(repo.path().join("README.md"), "x").unwrap();
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(repo.path())
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(repo.path())
        .output()
        .unwrap();

    let cwd = repo.path().to_string_lossy().to_string();
    let (pair, child) = spawn_pty_pair_with_retry(probe_size(), || {
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.cwd(&cwd);
        inject_worktree_env(&mut cmd, Some(cwd.as_str()));
        cmd.arg("-c");
        cmd.arg(r#"printf '%s\n%s\n' "$TUIC_BRANCH" "$TUIC_MAIN_REPO_PATH""#);
        cmd
    })
    .expect("sh must spawn");

    let reader = pair.master.try_clone_reader().expect("clone reader");
    drop(pair.slave);
    let output = read_pty_output_bounded(reader, 2, std::time::Duration::from_secs(5));
    let mut lines = output.lines();
    assert_eq!(lines.next(), Some("main"), "TUIC_BRANCH: {output:?}");
    assert_eq!(
        lines.next(),
        Some(
            repo.path()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .as_ref()
        ),
        "TUIC_MAIN_REPO_PATH: {output:?}"
    );
    reap(child);
}

#[cfg(unix)]
#[test]
fn non_repo_cwd_gets_no_worktree_vars() {
    // Not a temp dir: the test temp root lives inside this checkout, and the
    // TUIC_* derivation walks up to the owning repo (`git::find_repo_root`).
    // `/` has no `.git` above it, so it is an honest "not a repo" cwd.
    let cwd = "/".to_string();
    let (pair, child) = spawn_pty_pair_with_retry(probe_size(), || {
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.cwd(&cwd);
        inject_worktree_env(&mut cmd, Some(cwd.as_str()));
        cmd.arg("-c");
        cmd.arg(r#"printf '%s\n%s\n' "${TUIC_MAIN_REPO_PATH-unset}" "$TUIC_SCRIPT_KIND""#);
        cmd
    })
    .expect("sh must spawn");

    let reader = pair.master.try_clone_reader().expect("clone reader");
    drop(pair.slave);
    let output = read_pty_output_bounded(reader, 2, std::time::Duration::from_secs(5));
    let mut lines = output.lines();
    assert_eq!(
        lines.next(),
        Some("unset"),
        "a non-repo cwd must not get TUIC_MAIN_REPO_PATH: {output:?}"
    );
    assert_eq!(
        lines.next(),
        Some("run"),
        "the universal TUIC_SCRIPT_KIND must still be set: {output:?}"
    );
    reap(child);
}

#[test]
fn test_claude_reopening_a_premature_stop_hook_preserves_declared_background_work() {
    // Regression for the `ai-usage` Agent-Teams incident (2026-09-16): a
    // Claude session that dispatched teammates and ended its own turn
    // (Stop hook: state=idle + bgtasks=running) later wakes up to poll
    // them, producing renewed screen activity. That poll must NOT erase
    // the still-accurate `declared_background_work` claim — the
    // teammates haven't necessarily finished just because the parent's
    // own foreground turn reopened. Only a genuine new user-submitted
    // turn (see `note_submitted_input_with_hook`) or the next real
    // `Stop` hook's own `bgtasks` report may change it.
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "claude-agent-teams-poll";
    state.session_maps.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("claude".into()),
            ..Default::default()
        },
    );
    state
        .session_maps
        .shell_states
        .insert(session_id.into(), AtomicU8::new(SHELL_IDLE));
    state
        .session_maps
        .last_output_ms
        .insert(session_id.into(), AtomicU64::new(1));
    let mut lifecycle = SilenceState::new();
    // Mirrors the real sequence: a Stop hook set state=idle (explicit_idle)
    // and, via the `bgtasks` OSC verb, declared background work for the
    // current turn epoch (0).
    lifecycle.note_explicit_state(SHELL_IDLE, true);
    lifecycle.set_declared_background_work(true, 0);
    let lifecycle = Arc::new(Mutex::new(lifecycle));
    state
        .session_maps
        .silence_states
        .insert(session_id.into(), lifecycle.clone());

    let screen = vec![
        "✻ Simmering… (5m 48s · ↓ 20.7k tokens)".to_string(),
        "❯".to_string(),
    ];
    let activity = detect_agent_screen_activity(Some("claude"), &screen);
    assert_eq!(activity, AgentScreenActivity::Working);

    apply_working_evidence(
        &state,
        &lifecycle,
        session_id,
        now_epoch_ms(),
        "working-screen",
    );

    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY,
        "renewed screen evidence must still reopen the turn"
    );
    assert!(
        lifecycle.lock().declared_background_work_for_epoch(0),
        "reopening a stale idle/completed turn must not clear an unrelated \
         declared_background_work claim about still-running teammates"
    );
}

#[test]
fn declared_background_work_defers_parent_idle_notification() {
    // The hook-declared sibling of `background_work_defers_parent_idle_until_descendants_finish`
    // above — same suppression, driven by `SilenceState::declared_background_work`
    // instead of the OS-heuristic `SessionState::background_work`.
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-declared-background-sess";
    let parent_id = "parent-declared-background-sess";
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    silence.lock().set_declared_background_work(true, 0);
    state.session_maps.silence_states.insert(child_id.to_string(), silence);

    assert!(try_shell_transition(
        &state, child_id, SHELL_BUSY, SHELL_IDLE, true
    ));
    assert!(
        state.agent_inbox.get(parent_id).unwrap().is_empty(),
        "declared background work must defer the idle notification exactly like background_work"
    );
}

#[test]
fn declared_background_work_defers_suggest_publication() {
    // The hook-declared sibling of `background_work_defers_declared_completion_without_generic_idle`
    // above — `emit_pending_suggest_if_idle` must not publish a `suggest:`
    // completion while `declared_background_work` is set, and must once cleared.
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-declared-background-completed";
    let parent_id = "parent-declared-background-completed";
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    let mut silence = SilenceState::new();
    silence.mark_suggest_candidate(vec!["Review result".to_string()], 0);
    silence.set_declared_background_work(true, 0);
    let silence = Arc::new(Mutex::new(silence));
    state
        .session_maps
        .silence_states
        .insert(child_id.to_string(), silence.clone());

    assert!(!emit_pending_suggest_if_idle(&state, &silence, child_id));
    silence.lock().set_declared_background_work(false, 0);
    assert!(emit_pending_suggest_if_idle(&state, &silence, child_id));
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(inbox.len(), 1);
    let content: serde_json::Value =
        serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "completed");
}

#[test]
fn pending_background_probe_defers_suggest_publication() {
    // Closes a gap a code-review pass found: `emit_pending_suggest_if_idle`
    // used to check only `background_work`/`declared_background_work`, not
    // `has_pending_background_probe()` — unlike the ladder in
    // `session_state_with_shell` and the parent-idle suppression in
    // `try_shell_transition_locked`, which both already treat an
    // unconfirmed probe as work. An open question (ready screen seen, no
    // newer snapshot yet) must defer "completed" the same way a confirmed
    // positive does — publishing early would race a real descendant that
    // just hasn't been confirmed yet.
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-probe-pending-completed";
    let parent_id = "parent-probe-pending-completed";
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_probe_turn_epoch: Some(0),
            turn_epoch: 0,
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    let mut silence = SilenceState::new();
    silence.mark_suggest_candidate(vec!["Review result".to_string()], 0);
    let silence = Arc::new(Mutex::new(silence));
    state
        .session_maps
        .silence_states
        .insert(child_id.to_string(), silence.clone());

    assert!(
        !emit_pending_suggest_if_idle(&state, &silence, child_id),
        "an unresolved probe must defer publication, not just a confirmed background_work"
    );

    // Resolve the probe (as if a newer snapshot confirmed no live descendant).
    state
        .session_maps
        .session_states
        .get_mut(child_id)
        .unwrap()
        .background_probe_turn_epoch = None;
    assert!(emit_pending_suggest_if_idle(&state, &silence, child_id));
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(inbox.len(), 1);
    let content: serde_json::Value =
        serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "completed");
}

#[test]
fn a_genuine_new_turn_clears_declared_background_work() {
    // Complements `test_claude_reopening_a_premature_stop_hook_preserves_declared_background_work`:
    // a real new turn (a line actually submitted to the agent) is the one
    // thing that SHOULD invalidate a stale `bgtasks` declaration from the
    // previous turn.
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "new-turn-clears-bgwork";
    agent_session(&state, child_id, SHELL_IDLE);
    state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .lock()
        .set_declared_background_work(true, 0);
    assert!(
        state
            .session_maps
            .silence_states
            .get(child_id)
            .unwrap()
            .lock()
            .declared_background_work_for_epoch(0)
    );

    note_submitted_input(&state, child_id);

    assert_eq!(state.session_maps.session_states.get(child_id).unwrap().turn_epoch, 1);
    let silence = state.session_maps.silence_states.get(child_id).unwrap().clone();
    assert!(
        !silence.lock().declared_background_work_for_epoch(0),
        "a genuine new turn must clear a stale declared_background_work claim"
    );
    assert!(!silence.lock().declared_background_work_for_epoch(1));
}

#[test]
fn submitted_input_with_no_detected_agent_type_still_clears_declared_background_work() {
    // Covers `note_submitted_input_with_hook`'s OTHER branch — a session
    // with no `agent_type` detected yet (e.g. very early in a session's
    // life, before the first foreground-process poll). This branch never
    // touches `turn_epoch` (unlike the known-agent branch covered by
    // `a_genuine_new_turn_clears_declared_background_work`), so the
    // declaration must clear for the SAME epoch it was set at.
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "unknown-agent-submits-input";
    state
        .session_maps
        .session_states
        .insert(session_id.into(), crate::state::SessionState::default());
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    silence.lock().set_declared_background_work(true, 0);
    state
        .session_maps
        .silence_states
        .insert(session_id.into(), silence.clone());

    note_submitted_input(&state, session_id);

    assert_eq!(
        state.session_maps.session_states.get(session_id).unwrap().turn_epoch,
        0,
        "this branch must not touch turn_epoch"
    );
    assert!(
        !silence.lock().declared_background_work_for_epoch(0),
        "a real submission must clear a stale declaration even with no detected agent_type"
    );
}

#[cfg(unix)]
#[test]
fn standby_refuses_session_with_declared_background_work() {
    // The hook-declared sibling of `standby_refuses_session_with_background_work`
    // above, exercised through `standby_session`'s already-locked path
    // (`background_activity_blocks_standby_with_silence(..., Some(&guard))`).
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "declared-background-standby";
    state.session_maps.session_states.insert(
        session_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            ..Default::default()
        },
    );
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    silence.lock().set_declared_background_work(true, 0);
    state.session_maps.silence_states.insert(session_id.to_string(), silence);

    assert_eq!(standby_session(&state, session_id), Ok(false));
    assert!(!state.session_maps.standby_sessions.contains_key(session_id));
}

#[test]
fn tuic_osc_bgtasks_sets_declared_background_work_for_current_epoch() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-bgtasks-set";
    agent_session(&state, session_id, SHELL_IDLE);
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    let silence = state.session_maps.silence_states.get(session_id).unwrap().clone();
    let mut processor = ChunkProcessor::new(None, None);

    assert!(!silence.lock().declared_background_work_for_epoch(0));
    processor.process_chunk(
        "\x1b]7770;bgtasks=running\x07",
        &silence,
        session_id,
        &state,
    );
    assert!(
        silence.lock().declared_background_work_for_epoch(0),
        "a bgtasks=running OSC event must declare background work for the current turn"
    );
}

#[test]
fn end_to_end_stop_hook_bgtasks_survives_a_subsequent_screen_poll() {
    // Full-pipeline regression for the `ai-usage` Agent-Teams incident:
    // replays the real byte sequence through `ChunkProcessor::process_chunk`
    // (not a direct call into `apply_working_evidence`) — a real `Stop`
    // hook's `state=idle` + `bgtasks=running` OSC pair (this exact order
    // confirmed empirically against the installed `tuic-hook` binary),
    // followed by the orchestrator's screen showing renewed spinner
    // activity as it polls its teammates (no new user input submitted).
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "e2e-agent-teams-poll";
    agent_session(&state, session_id, SHELL_IDLE);
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    let silence = state.session_maps.silence_states.get(session_id).unwrap().clone();
    let mut processor = ChunkProcessor::new(None, None);

    // A prompt row so `detect_claude_screen_activity` has a prompt anchor
    // once the spinner appears above it.
    processor.process_chunk("❯", &silence, session_id, &state);

    // The real Stop hook's OSC pair, in the order `tuic-hook` emits it.
    processor.process_chunk(
        "\x1b]7770;bgtasks=running\x07\x1b]7770;state=idle\x07",
        &silence,
        session_id,
        &state,
    );
    assert!(
        silence.lock().declared_background_work_for_epoch(0),
        "the Stop hook's own bgtasks report must declare background work"
    );

    // The orchestrator wakes up to poll its teammates: a real spinner row
    // repaints above the prompt. No new turn was submitted — turn_epoch
    // stays 0 — this is exactly the renewed-screen-evidence reopen path,
    // not a genuine new turn.
    processor.process_chunk(
        "\r\n✻ Simmering… (5m 48s · ↓ 20.7k tokens)\r\n❯",
        &silence,
        session_id,
        &state,
    );

    assert_eq!(
        state.session_maps.session_states.get(session_id).unwrap().turn_epoch,
        0,
        "no real turn was submitted"
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(std::sync::atomic::Ordering::Acquire),
        SHELL_BUSY,
        "renewed screen evidence must still reopen the turn"
    );
    assert!(
        silence.lock().declared_background_work_for_epoch(0),
        "the poll must not have erased the still-accurate declared_background_work claim"
    );
}

#[test]
fn tuic_osc_bgtasks_clears_on_zero_running_statuses() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-bgtasks-clear";
    agent_session(&state, session_id, SHELL_IDLE);
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    let silence = state.session_maps.silence_states.get(session_id).unwrap().clone();
    silence.lock().set_declared_background_work(true, 0);
    let mut processor = ChunkProcessor::new(None, None);

    // No status in the (percent-decoded) comma-joined payload equals
    // "running" — e.g. every backgrounded task already completed.
    processor.process_chunk(
        "\x1b]7770;bgtasks=completed\x07",
        &silence,
        session_id,
        &state,
    );
    assert!(
        !silence.lock().declared_background_work_for_epoch(0),
        "a bgtasks payload naming no running task must clear the declaration"
    );
}

#[test]
fn tuic_osc_bgtasks_unknown_status_is_treated_as_still_running() {
    // Fail-safe classification: Claude Code's background_tasks[].status
    // vocabulary is not a documented closed set anywhere — only
    // "completed"/"failed" are confirmed terminal. A future or
    // unrecognized status (e.g. a transient "pending"/"queued" this
    // binary has never seen) must NOT silently clear a real declaration.
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-bgtasks-unknown-status";
    agent_session(&state, session_id, SHELL_IDLE);
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    let silence = state.session_maps.silence_states.get(session_id).unwrap().clone();
    let mut processor = ChunkProcessor::new(None, None);

    processor.process_chunk(
        "\x1b]7770;bgtasks=pending\x07",
        &silence,
        session_id,
        &state,
    );
    assert!(
        silence.lock().declared_background_work_for_epoch(0),
        "an unrecognized status must be treated as still active, not silently cleared"
    );
}

#[test]
fn tuic_osc_bgtasks_empty_payload_clears_declaration() {
    // tuic-hook emits `bgtasks=` (empty payload) when Claude Code's
    // background_tasks array is present but empty — a real "nothing
    // outstanding" observation, not absence of the verb entirely.
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-bgtasks-empty";
    agent_session(&state, session_id, SHELL_IDLE);
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    let silence = state.session_maps.silence_states.get(session_id).unwrap().clone();
    silence.lock().set_declared_background_work(true, 0);
    let mut processor = ChunkProcessor::new(None, None);

    processor.process_chunk("\x1b]7770;bgtasks=\x07", &silence, session_id, &state);
    assert!(!silence.lock().declared_background_work_for_epoch(0));
}

/// Replay `chunk` through a fresh hook-instrumented session's
/// `ChunkProcessor` with an isolated config dir and return the pending
/// wrap-prompt map's agents.
fn userwrap_pending_after(chunk: &str) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-userwrap";
    agent_session(&state, session_id, SHELL_IDLE);
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
    let mut processor = ChunkProcessor::new(None, None);
    processor.process_chunk(chunk, &silence, session_id, &state);
    let mut agents: Vec<String> = state
        .agent_wrap_pending
        .iter()
        .map(|e| e.key().clone())
        .collect();
    agents.sort();
    agents
}

#[test]
#[serial_test::serial]
fn tuic_osc_userwrap_recognized_agent_opens_a_pending_prompt() {
    // End-to-end regression for the zsh deferred integration's `ask` branch
    // (shell_integration.rs) — a real `\e]7770;userwrap=claude:<fp>\a` byte
    // sequence replayed through `ChunkProcessor::process_chunk` must reach
    // `agent_wrap_prompt::request` and open exactly one pending prompt.
    assert_eq!(
        userwrap_pending_after("\x1b]7770;userwrap=claude:1234567-89\x07"),
        vec!["claude".to_string()],
        "a recognized userwrap payload must open a pending prompt for that agent"
    );
}

/// Batch 36 review: any program's output (`cat` of a file, remote SSH
/// output) could forge the verb. The shell sends it only from precmd, so it
/// is dropped while OSC 133 says a foreground command is running.
#[test]
#[serial_test::serial]
fn tuic_osc_userwrap_is_ignored_while_a_foreground_command_runs() {
    assert!(
        userwrap_pending_after(
            "\x1b]133;A\x07\x1b]133;B\x07\x1b]133;C\x07forged:\x1b]7770;userwrap=claude:1234567-89\x07"
        )
        .is_empty(),
        "a userwrap printed by a running command must not open a prompt"
    );
}

#[test]
#[serial_test::serial]
fn tuic_osc_userwrap_is_honoured_again_once_the_command_finished() {
    assert_eq!(
        userwrap_pending_after(
            "\x1b]133;C\x07\x1b]133;D;0\x07\x1b]7770;userwrap=claude:1234567-89\x07\x1b]133;A\x07"
        ),
        vec!["claude".to_string()],
        "precmd's userwrap (after D, before A) is the genuine one"
    );
}

#[test]
#[serial_test::serial]
fn tuic_osc_userwrap_unrecognized_payload_is_ignored() {
    // Terminal output is untrusted — any process can print an OSC sequence.
    // An agent outside the allow-list, a missing or malformed fingerprint, or
    // shell text in either half must be silently ignored: no prompt, no panic.
    for chunk in [
        "\x1b]7770;userwrap=rm -rf ~:1-1\x07",
        "\x1b]7770;userwrap=grok:1-1\x07",
        "\x1b]7770;userwrap=claude\x07",
        "\x1b]7770;userwrap=claude:\x07",
        "\x1b]7770;userwrap=claude:1-1 x\x07",
        "\x1b]7770;userwrap=claude:$(id)\x07",
    ] {
        assert!(
            userwrap_pending_after(chunk).is_empty(),
            "{chunk:?} must not open a prompt"
        );
    }
}

#[test]
fn declared_background_work_self_expires_on_a_new_turn() {
    // Mirrors `stale_background_clear_cannot_emit_after_new_turn`'s shape
    // for the OS-heuristic `background_work` field: a declaration made at
    // one turn epoch must not read as active once the turn moves on, with
    // no explicit clear required — the epoch check alone must do it.
    let silence = SilenceState::new();
    assert!(!silence.declared_background_work_for_epoch(7));

    let mut silence = silence;
    silence.set_declared_background_work(true, 7);
    assert!(silence.declared_background_work_for_epoch(7));
    assert!(
        !silence.declared_background_work_for_epoch(8),
        "a declaration stamped for epoch 7 must not apply to epoch 8"
    );
}

#[test]
fn reset_suggest_memory_no_longer_touches_declared_background_work() {
    // `reset_suggest_memory` is also called from `apply_working_evidence`'s
    // reopen path, which is not a real new-turn boundary (see
    // `reset_declared_background_work`'s doc comment) — a still-accurate
    // background-task declaration must survive it.
    let mut silence = SilenceState::new();
    silence.set_declared_background_work(true, 3);
    assert!(silence.declared_background_work_for_epoch(3));
    silence.reset_suggest_memory();
    assert!(
        silence.declared_background_work_for_epoch(3),
        "reset_suggest_memory must NOT clear a declared_background_work claim — only reset_declared_background_work does"
    );
}

#[test]
fn reset_declared_background_work_clears_the_declaration() {
    let mut silence = SilenceState::new();
    silence.set_declared_background_work(true, 3);
    assert!(silence.declared_background_work_for_epoch(3));
    silence.reset_declared_background_work();
    assert!(!silence.declared_background_work_for_epoch(3));
}

#[test]
fn declared_background_work_alone_makes_agent_state_working() {
    // No OS-level `background_work`, no pending probe — only the
    // hook-declared signal — must still resolve `agent_state` to
    // "working", mirroring `test_completion_does_not_override_confirmed_or_pending_background_work`'s
    // shape for the OS-heuristic case.
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-bgtasks-ladder";
    agent_session(&state, session_id, SHELL_IDLE);
    state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .lock()
        .set_declared_background_work(true, 0);
    let snapshot = state
        .session_state_with_shell(session_id)
        .expect("session state");
    assert_eq!(snapshot.agent_state.as_deref(), Some("working"));
    assert!(snapshot.declared_background_work);
    assert!(
        !snapshot.background_work,
        "OS-level flag must stay untouched"
    );
}

// ── `declared_background_work` edge cases (characterization) ────────────
//
// Pin the `bgtasks` OSC arm's payload classification, epoch stamping and
// missing-state tolerance, plus each reader's stale-epoch behavior, so the
// teammate/subagent split can change the write side without silently
// changing what readers see.

/// A Claude agent session with a VT log buffer, ready for `process_chunk`.
fn bgtasks_test_session(
    state: &crate::state::AppState,
    session_id: &str,
) -> Arc<Mutex<SilenceState>> {
    agent_session(state, session_id, SHELL_IDLE);
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone()
}

fn feed_bgtasks(
    state: &crate::state::AppState,
    session_id: &str,
    silence: &Arc<Mutex<SilenceState>>,
    payload: &str,
) {
    let mut processor = ChunkProcessor::new(None, None);
    processor.process_chunk(
        &format!("\x1b]7770;bgtasks={payload}\x07"),
        silence,
        session_id,
        state,
    );
}

#[test]
fn tuic_osc_bgtasks_classifies_percent_decoded_status_lists() {
    // `tuic-hook` percent-encodes the comma-joined statuses, so the comma arrives
    // as `%2C`. Any non-terminal entry anywhere in the list keeps the declaration
    // active; only an all-terminal (or empty-element) list clears it.
    let cases: &[(&str, bool)] = &[
        ("running", true),
        ("running%2Ccompleted", true),
        ("completed%2Crunning", true),
        ("completed%2Cfailed", false),
        ("failed%2Ccompleted%2Cfailed", false),
        ("completed%2Cpending", true),
        ("completed%2C", false),
        ("%2C", false),
        ("completed", false),
        ("failed", false),
        ("", false),
    ];
    for (payload, expect_running) in cases {
        let state = crate::state::tests_support::make_test_app_state();
        let session_id = "test-bgtasks-list";
        let silence = bgtasks_test_session(&state, session_id);
        // Seed the opposite value so a no-op would be caught.
        silence
            .lock()
            .set_declared_background_work(!*expect_running, 0);
        feed_bgtasks(&state, session_id, &silence, payload);
        assert_eq!(
            silence.lock().declared_background_work_for_epoch(0),
            *expect_running,
            "bgtasks payload {payload:?}"
        );
    }
}

#[test]
fn tuic_osc_bgtasks_stamps_the_sessions_current_turn_epoch() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-bgtasks-epoch-stamp";
    let silence = bgtasks_test_session(&state, session_id);
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .turn_epoch = 5;

    feed_bgtasks(&state, session_id, &silence, "running");
    assert!(silence.lock().declared_background_work_for_epoch(5));
    assert!(
        !silence.lock().declared_background_work_for_epoch(0),
        "the declaration is stamped with the session's epoch, not a constant"
    );
    assert!(!silence.lock().declared_background_work_for_epoch(6));
}

#[test]
fn tuic_osc_bgtasks_restamps_a_stale_declaration_to_the_current_epoch() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-bgtasks-restamp";
    let silence = bgtasks_test_session(&state, session_id);
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .turn_epoch = 3;
    // A declaration left over from an earlier epoch.
    silence.lock().set_declared_background_work(true, 2);
    assert!(!silence.lock().declared_background_work_for_epoch(3));

    feed_bgtasks(&state, session_id, &silence, "running");
    assert!(silence.lock().declared_background_work_for_epoch(3));
    assert!(
        !silence.lock().declared_background_work_for_epoch(2),
        "a re-assertion overwrites the stored epoch rather than accumulating"
    );

    feed_bgtasks(&state, session_id, &silence, "completed");
    assert!(!silence.lock().declared_background_work_for_epoch(3));
}

#[test]
fn tuic_osc_bgtasks_without_session_state_leaves_declaration_untouched() {
    // The arm needs the session's turn epoch; with no SessionState it must not
    // guess (and must not panic).
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-bgtasks-no-session-state";
    let silence = bgtasks_test_session(&state, session_id);
    state.session_maps.session_states.remove(session_id);

    feed_bgtasks(&state, session_id, &silence, "running");
    assert!(!silence.lock().declared_background_work_for_epoch(0));
}

#[test]
fn tuic_osc_bgtasks_without_registered_silence_state_does_not_create_one() {
    // The arm writes through the map's own entry, not the `process_chunk`
    // argument; with no registered entry it must neither panic nor insert one.
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-bgtasks-no-silence-entry";
    let silence = bgtasks_test_session(&state, session_id);
    state.session_maps.silence_states.remove(session_id);

    feed_bgtasks(&state, session_id, &silence, "running");
    assert!(!state.session_maps.silence_states.contains_key(session_id));
    assert!(!silence.lock().declared_background_work_for_epoch(0));
}

#[test]
fn tuic_osc_bgtasks_and_os_background_work_do_not_touch_each_other() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-bgtasks-os-independence";
    let silence = bgtasks_test_session(&state, session_id);
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .background_work = true;

    feed_bgtasks(&state, session_id, &silence, "completed");
    assert!(
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .background_work,
        "a bgtasks clear must not clear the OS-heuristic field"
    );

    feed_bgtasks(&state, session_id, &silence, "running");
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .background_work = false;
    assert!(
        silence.lock().declared_background_work_for_epoch(0),
        "clearing the OS-heuristic field must not clear the declaration"
    );
}

#[test]
fn stale_declared_background_work_does_not_defer_parent_idle_notification() {
    // Counterpart of `declared_background_work_defers_parent_idle_notification`:
    // a declaration from a previous turn epoch no longer applies, so the idle
    // edge must be reported to the parent as usual.
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-stale-declared-sess";
    let parent_id = "parent-stale-declared-sess";
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            turn_epoch: 1,
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    silence.lock().set_declared_background_work(true, 0);
    state
        .session_maps
        .silence_states
        .insert(child_id.to_string(), silence);

    assert!(try_shell_transition(
        &state, child_id, SHELL_BUSY, SHELL_IDLE, true
    ));
    assert_eq!(
        state.agent_inbox.get(parent_id).unwrap().len(),
        1,
        "a declaration stamped for epoch 0 must not defer an idle edge on epoch 1"
    );
}

#[test]
fn stale_declared_background_work_does_not_defer_suggest_publication() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-stale-declared-completed";
    let parent_id = "parent-stale-declared-completed";
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            turn_epoch: 1,
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    let mut silence = SilenceState::new();
    silence.mark_suggest_candidate(vec!["Review result".to_string()], 1);
    silence.set_declared_background_work(true, 0);
    let silence = Arc::new(Mutex::new(silence));
    state
        .session_maps
        .silence_states
        .insert(child_id.to_string(), silence.clone());

    assert!(
        emit_pending_suggest_if_idle(&state, &silence, child_id),
        "a declaration from epoch 0 must not defer a completion on epoch 1"
    );
    assert_eq!(state.agent_inbox.get(parent_id).unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn standby_gate_reads_declared_work_for_the_current_epoch_only() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "declared-background-standby-gate";
    state.session_maps.session_states.insert(
        session_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            turn_epoch: 2,
            ..Default::default()
        },
    );
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(session_id.to_string(), silence.clone());

    assert!(!background_activity_blocks_standby(&state, session_id));

    silence.lock().set_declared_background_work(true, 1);
    assert!(
        !background_activity_blocks_standby(&state, session_id),
        "a stale declaration must not block standby"
    );

    silence.lock().set_declared_background_work(true, 2);
    assert!(background_activity_blocks_standby(&state, session_id));

    // The already-locked variant used by `standby_session` agrees with the
    // self-locking one (and must not re-lock the held guard).
    let guard = silence.lock();
    assert!(background_activity_blocks_standby_with_silence(
        &state,
        session_id,
        Some(&guard)
    ));
    drop(guard);

    silence.lock().set_declared_background_work(false, 2);
    assert!(!background_activity_blocks_standby(&state, session_id));
}

#[cfg(unix)]
#[test]
fn standby_gate_unknown_session_does_not_block() {
    let state = crate::state::tests_support::make_test_app_state();
    assert!(!background_activity_blocks_standby(
        &state,
        "no-such-session"
    ));
}

#[cfg(unix)]
#[test]
fn standby_gate_os_background_work_blocks_without_any_declaration() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "os-background-standby-gate";
    state.session_maps.session_states.insert(
        session_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            background_work: true,
            ..Default::default()
        },
    );
    state.session_maps.silence_states.insert(
        session_id.to_string(),
        Arc::new(Mutex::new(SilenceState::new())),
    );
    assert!(
        background_activity_blocks_standby(&state, session_id),
        "the OS-heuristic signal blocks standby on its own, independent of the declaration"
    );
}

/// Fullscreen-mode fix: a busy/idle edge that lands while the alternate
/// screen buffer is active (Claude Code's default renderer) must still
/// emit its `AgentBlock` (so `CommandOverview`'s prompt/duration/exit
/// metadata keeps working) but tag it `on_alt_screen: true` so
/// row-anchored consumers (gutter, scrollbar, fold, jump-nav) know `line`
/// is not a valid scrollback anchor.
#[test]
fn handle_tuic_state_alt_screen_active_tags_both_start_and_end() {
    let session_id = "test-block-alt-screen";
    let state = setup_idle_session(session_id);
    state
        .session_maps
        .last_prompts
        .insert(session_id.to_string(), "do the thing".to_string());
    let proc = ChunkProcessor::new(None, None);

    let (_, start_event) = proc.handle_tuic_state("busy", session_id, 3, true, &state);
    match start_event {
        Some(ParsedEvent::AgentBlock {
            action,
            prompt_text,
            on_alt_screen,
            ..
        }) => {
            assert_eq!(action, "start");
            assert!(
                on_alt_screen,
                "a busy edge recorded during fullscreen mode must be tagged"
            );
            // Metadata must still be populated — CommandOverview depends on it.
            assert_eq!(prompt_text, Some("do the thing".to_string()));
        }
        other => panic!("expected AgentBlock start, got {other:?}"),
    }

    let (_, end_event) = proc.handle_tuic_state("idle", session_id, 1, true, &state);
    match end_event {
        Some(ParsedEvent::AgentBlock {
            action,
            on_alt_screen,
            ..
        }) => {
            assert_eq!(action, "end");
            assert!(
                on_alt_screen,
                "the matching idle edge must be tagged too, even though \
                 its `line` (1) is numerically less than the start \
                 edge's `line` (3) — non-monotonic `line` values across \
                 a single block are exactly what alt-screen tagging \
                 exists to make safe to ignore downstream"
            );
        }
        other => panic!("expected AgentBlock end, got {other:?}"),
    }
}

/// A block that starts on the primary screen but whose matching idle
/// edge lands after the agent has entered fullscreen mode mid-turn must
/// still tag only the mistagged (alt-screen) half — the start event,
/// already emitted and consumed by the frontend, is not retroactively
/// touched. This documents the current, correct behavior: each edge is
/// tagged independently, at the instant it is observed.
#[test]
fn handle_tuic_state_tags_each_edge_independently_across_a_screen_transition() {
    let session_id = "test-block-mixed-screen";
    let state = setup_idle_session(session_id);
    let proc = ChunkProcessor::new(None, None);

    let (_, start_event) = proc.handle_tuic_state("busy", session_id, 10, false, &state);
    assert!(
        matches!(
            start_event,
            Some(ParsedEvent::AgentBlock {
                on_alt_screen: false,
                ..
            })
        ),
        "start observed on the primary screen must not be tagged"
    );

    let (_, end_event) = proc.handle_tuic_state("idle", session_id, 4, true, &state);
    assert!(
        matches!(
            end_event,
            Some(ParsedEvent::AgentBlock {
                on_alt_screen: true,
                ..
            })
        ),
        "end observed after entering fullscreen mode must be tagged"
    );
}

/// Fullscreen-mode fix: `on_alt_screen` is tagged on every event this
/// fallback emits — start, the end that closes a previous block, and the
/// teardown-close path — not just the primary-screen case every other
/// test in this group exercises.
#[test]
fn cc_block_alt_screen_tags_start_end_and_teardown() {
    let mut last = None;
    let rows = vec![header_row(3, "Read"), header_row(9, "Edit")];
    let events = synthesize_cc_block_events(&rows, 100, true, 0, true, &mut last);
    assert_eq!(events.len(), 3, "start, end, start");
    for evt in &events {
        match evt {
            ParsedEvent::AgentBlock { on_alt_screen, .. } => {
                assert!(*on_alt_screen, "every synthesized event must be tagged")
            }
            other => panic!("expected AgentBlock, got {other:?}"),
        }
    }

    // Teardown-close path (agent_active flips false) must tag too.
    let events = synthesize_cc_block_events(&[], 100, false, 180, true, &mut last);
    assert_eq!(events.len(), 1);
    match &events[0] {
        ParsedEvent::AgentBlock {
            action,
            on_alt_screen,
            ..
        } => {
            assert_eq!(action, "end");
            assert!(*on_alt_screen);
        }
        other => panic!("expected AgentBlock end, got {other:?}"),
    }
}

fn dump_row(row_index: usize, text: &str) -> ChangedRow {
    ChangedRow {
        row_index,
        text: text.to_string(),
    }
}

#[test]
fn transcript_dump_requires_claude_and_hook_instrumentation() {
    let rows = vec![dump_row(0, "\u{276F} do the thing")];
    let mut last = None;

    // Wrong agent type.
    let events = synthesize_transcript_dump_block_events(
        &rows,
        100,
        Some("codex"),
        true,
        200,
        &mut last,
        false,
    );
    assert!(events.is_empty());

    // Not hook-instrumented.
    let events = synthesize_transcript_dump_block_events(
        &rows,
        100,
        Some("claude"),
        false,
        200,
        &mut last,
        false,
    );
    assert!(events.is_empty());

    // No agent type at all.
    let events =
        synthesize_transcript_dump_block_events(&rows, 100, None, true, 200, &mut last, false);
    assert!(events.is_empty());
}

/// Code-review finding: a still-open dump block must be closed the
/// moment the gate stops holding (agent exits / hook state clears),
/// mirroring `synthesize_cc_block_events`'s `!agent_active` teardown —
/// otherwise `last_dump_block_line` stays `Some` forever and can
/// mis-close whatever block opens next.
#[test]
fn transcript_dump_gate_dropping_closes_a_still_open_block() {
    let rows = vec![dump_row(3, "\u{276F} do the thing")];
    let mut last = None;
    let events = synthesize_transcript_dump_block_events(
        &rows,
        100,
        Some("claude"),
        true,
        200,
        &mut last,
        false,
    );
    assert_eq!(events.len(), 1, "opens the block");
    assert!(last.is_some());

    // Gate drops (agent exited / hook state cleared) before any `done`
    // marker ever appeared on screen.
    let events =
        synthesize_transcript_dump_block_events(&[], 100, None, true, 250, &mut last, false);
    assert_eq!(events.len(), 1);
    match &events[0] {
        ParsedEvent::AgentBlock {
            action,
            line,
            on_alt_screen,
            ..
        } => {
            assert_eq!(action, "end");
            assert_eq!(*line, 250);
            assert!(!on_alt_screen);
        }
        other => panic!("expected AgentBlock end, got {other:?}"),
    }
    assert_eq!(last, None, "must clear the tracker, not just emit an end");

    // A later call with the gate still down must stay a no-op — nothing
    // left to close.
    let events =
        synthesize_transcript_dump_block_events(&[], 100, None, true, 300, &mut last, false);
    assert!(events.is_empty());
}

#[test]
fn transcript_dump_prompt_line_starts_a_real_primary_screen_block() {
    let rows = vec![dump_row(3, "\u{276F} do the thing")];
    let mut last = None;
    let events = synthesize_transcript_dump_block_events(
        &rows,
        100,
        Some("claude"),
        true,
        200,
        &mut last,
        false,
    );
    assert_eq!(events.len(), 1);
    match &events[0] {
        ParsedEvent::AgentBlock {
            action,
            line,
            prompt_text,
            on_alt_screen,
            new_dump_generation,
            ..
        } => {
            assert_eq!(action, "start");
            assert_eq!(*line, 103);
            assert_eq!(prompt_text, &Some("do the thing".to_string()));
            assert!(
                !on_alt_screen,
                "the dump is real primary-screen content, never alt-screen"
            );
            assert!(
                !new_dump_generation,
                "is_new_generation was false, so this must not signal a prune"
            );
        }
        other => panic!("expected AgentBlock start, got {other:?}"),
    }
    assert_eq!(last, Some(103));
}

/// Scrollback-ring eviction fix's sibling for #11: `is_new_generation:
/// true` (a visit back to the alternate screen since the last dump
/// activity) must tag exactly the first `start` event with
/// `new_dump_generation: true` — the signal the frontend prunes stale
/// dump blocks on — and never a later one in the same chunk.
#[test]
fn transcript_dump_new_generation_tags_only_the_first_start_event() {
    let rows = vec![
        dump_row(3, "\u{276F} first turn"),
        dump_row(9, "\u{276F} second turn"),
    ];
    let mut last = None;
    let events = synthesize_transcript_dump_block_events(
        &rows,
        100,
        Some("claude"),
        true,
        200,
        &mut last,
        true,
    );
    assert_eq!(events.len(), 3, "start, end, start");
    let generation_flags: Vec<bool> = events
        .iter()
        .map(|e| match e {
            ParsedEvent::AgentBlock {
                new_dump_generation,
                ..
            } => *new_dump_generation,
            _ => panic!("expected AgentBlock"),
        })
        .collect();
    assert_eq!(
        generation_flags,
        vec![true, false, false],
        "only the very first start event may carry the signal"
    );
}

/// `is_new_generation: false` (no alternate-screen visit since the last
/// dump activity — e.g. the dump is still being incrementally repainted)
/// must never tag anything, even the first event.
#[test]
fn transcript_dump_no_new_generation_when_not_flagged() {
    let rows = vec![dump_row(3, "\u{276F} first turn")];
    let mut last = None;
    let events = synthesize_transcript_dump_block_events(
        &rows,
        100,
        Some("claude"),
        true,
        200,
        &mut last,
        false,
    );
    assert_eq!(events.len(), 1);
    match &events[0] {
        ParsedEvent::AgentBlock {
            new_dump_generation,
            ..
        } => assert!(!new_dump_generation),
        other => panic!("expected AgentBlock start, got {other:?}"),
    }
}

#[test]
fn transcript_dump_turn_end_marker_closes_the_open_block() {
    let rows = vec![
        dump_row(3, "\u{276F} do the thing"),
        dump_row(6, "\u{273B} Churned for 1s \u{b7} done 1:54 AM"),
    ];
    let mut last = None;
    let events = synthesize_transcript_dump_block_events(
        &rows,
        100,
        Some("claude"),
        true,
        200,
        &mut last,
        false,
    );
    assert_eq!(events.len(), 2);
    match &events[1] {
        ParsedEvent::AgentBlock {
            action,
            line,
            on_alt_screen,
            ..
        } => {
            assert_eq!(action, "end");
            assert_eq!(*line, 106);
            assert!(!on_alt_screen);
        }
        other => panic!("expected AgentBlock end, got {other:?}"),
    }
    assert_eq!(last, None, "closing must clear the open-block tracker");
}

#[test]
fn transcript_dump_second_prompt_closes_the_first_block_first() {
    let rows = vec![
        dump_row(3, "\u{276F} first turn"),
        dump_row(9, "\u{276F} second turn"),
    ];
    let mut last = None;
    let events = synthesize_transcript_dump_block_events(
        &rows,
        100,
        Some("claude"),
        true,
        200,
        &mut last,
        false,
    );
    assert_eq!(events.len(), 3, "start, end, start");
    let actions: Vec<&str> = events
        .iter()
        .map(|e| match e {
            ParsedEvent::AgentBlock { action, .. } => action.as_str(),
            _ => panic!("expected AgentBlock"),
        })
        .collect();
    assert_eq!(actions, vec!["start", "end", "start"]);
}

#[test]
fn transcript_dump_turn_end_with_no_open_block_is_a_noop() {
    let rows = vec![dump_row(3, "\u{273B} Churned for 1s \u{b7} done 1:54 AM")];
    let mut last = None;
    let events = synthesize_transcript_dump_block_events(
        &rows,
        100,
        Some("claude"),
        true,
        200,
        &mut last,
        false,
    );
    assert!(events.is_empty());
}

#[test]
fn transcript_dump_prompt_detector_ignores_plain_output_lines() {
    assert_eq!(transcript_dump_prompt_text("just some output"), None);
    assert_eq!(transcript_dump_prompt_text("\u{23FA} Read(file.rs)"), None);
    assert_eq!(transcript_dump_prompt_text("\u{276F} hello"), Some("hello"));
}

/// Edge cases: a bare `❯` with no trailing space (not the dump's own
/// format, so it must not match) and `❯ ` with nothing after it (must
/// match with an empty string, not panic on the slice).
#[test]
fn transcript_dump_prompt_detector_edge_cases() {
    assert_eq!(transcript_dump_prompt_text("\u{276F}"), None);
    assert_eq!(transcript_dump_prompt_text("\u{276F}no_space"), None);
    assert_eq!(transcript_dump_prompt_text("\u{276F} "), Some(""));
}

/// An empty prompt (`❯ ` with nothing else on the line) must start a
/// block with `prompt_text: None`, not `Some("")` — mirrors the
/// heuristic fallback's own "no text source" convention rather than
/// leaking an empty-but-present string into the frontend.
#[test]
fn transcript_dump_empty_prompt_text_becomes_none_not_empty_string() {
    let rows = vec![dump_row(3, "\u{276F} ")];
    let mut last = None;
    let events = synthesize_transcript_dump_block_events(
        &rows,
        100,
        Some("claude"),
        true,
        200,
        &mut last,
        false,
    );
    assert_eq!(events.len(), 1);
    match &events[0] {
        ParsedEvent::AgentBlock { prompt_text, .. } => {
            assert_eq!(prompt_text, &None);
        }
        other => panic!("expected AgentBlock start, got {other:?}"),
    }
}

#[test]
fn transcript_dump_turn_end_detector_requires_both_glyph_and_marker() {
    assert!(!is_transcript_dump_turn_end("just some output"));
    assert!(!is_transcript_dump_turn_end(
        "\u{273B} still working, no done marker yet"
    ));
    assert!(is_transcript_dump_turn_end(
        "\u{273B} Churned for 1s \u{b7} done 1:54 AM"
    ));
}

fn feed_tuic_verb(
    state: &crate::state::AppState,
    session_id: &str,
    silence: &Arc<Mutex<SilenceState>>,
    verb: &str,
    payload: &str,
) {
    let mut processor = ChunkProcessor::new(None, None);
    processor.process_chunk(
        &format!("\x1b]7770;{verb}={payload}\x07"),
        silence,
        session_id,
        state,
    );
}

#[test]
fn parse_bg_task_summary_splits_teammates_from_other_running_work() {
    let cases: &[(&str, u32, u32)] = &[
        // (decoded payload, non_teammate_running, teammate_running)
        ("", 0, 0),
        ("teammate/running", 0, 1),
        ("teammate/running*3", 0, 3),
        ("shell/running", 1, 0),
        ("shell/running*2,subagent/running", 3, 0),
        ("teammate/running*2,shell/running", 1, 2),
        ("teammate/completed,shell/failed", 0, 0),
        ("teammate/pending", 0, 1),
        ("/running", 1, 0),
        ("running", 1, 0),
        ("teammate/running*x", 0, 1),
        ("teammate/running*0", 0, 0),
        ("teammate/,shell/", 0, 0),
        ("monitor/running,workflow/running,MCP task/running", 3, 0),
    ];
    for (payload, other, mates) in cases {
        let got = parse_bg_task_summary(payload);
        assert_eq!(
            (got.non_teammate_running, got.teammate_running),
            (*other, *mates),
            "payload {payload:?}"
        );
    }
}

#[test]
fn bgtasksummary_makes_an_idle_teammate_stop_counting_but_keeps_other_work() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-bgsummary-teammate";
    let silence = bgtasks_test_session(&state, session_id);

    // Only a teammate is declared running.
    feed_bgtasks(&state, session_id, &silence, "running");
    feed_tuic_verb(
        &state,
        session_id,
        &silence,
        "bgtasksummary",
        "teammate%2Frunning",
    );
    {
        let sl = silence.lock();
        assert!(
            sl.declared_background_work_for_epoch(0),
            "conservative read counts it"
        );
        assert!(
            !sl.declared_background_work_for_epoch_with(0, |_| false),
            "idle teammate"
        );
        assert!(
            sl.declared_background_work_for_epoch_with(0, |_| true),
            "busy teammate"
        );
    }

    // Real background work alongside the teammate holds it regardless.
    feed_bgtasks(&state, session_id, &silence, "running%2Crunning");
    feed_tuic_verb(
        &state,
        session_id,
        &silence,
        "bgtasksummary",
        "teammate%2Frunning%2Cshell%2Frunning",
    );
    assert!(
        silence
            .lock()
            .declared_background_work_for_epoch_with(0, |_| false)
    );
}

#[test]
fn bgtasksummary_is_lazy_about_the_teammate_check() {
    let state = crate::state::tests_support::make_test_app_state();
    let silence = bgtasks_test_session(&state, "test-bgsummary-lazy");
    feed_bgtasks(&state, "test-bgsummary-lazy", &silence, "running");
    feed_tuic_verb(
        &state,
        "test-bgsummary-lazy",
        &silence,
        "bgtasksummary",
        "shell%2Frunning",
    );
    // Non-teammate work is decisive, so the closure must never run (it would read
    // other sessions' state while a SilenceState lock is held).
    assert!(
        silence
            .lock()
            .declared_background_work_for_epoch_with(0, |_| {
                panic!("teammate check must not run when non-teammate work is declared")
            })
    );
}

#[test]
fn bgtasksummary_is_authoritative_for_its_fire_and_repairs_a_truncated_bgtasks() {
    // `bgtasks` joins every status and is cut at the OSC payload cap, so with a long
    // list of finished tasks a late `running` is lost and `bgtasks` reads
    // all-terminal. The grouped summary is bounded and must repair that.
    let state = crate::state::tests_support::make_test_app_state();
    let silence = bgtasks_test_session(&state, "test-bgsummary-repair");
    feed_bgtasks(&state, "test-bgsummary-repair", &silence, "completed");
    assert!(!silence.lock().declared_background_work_for_epoch(0));

    feed_tuic_verb(
        &state,
        "test-bgsummary-repair",
        &silence,
        "bgtasksummary",
        "shell%2Fcompleted%2A200%2Cteammate%2Frunning",
    );
    let sl = silence.lock();
    assert!(
        sl.declared_background_work_for_epoch(0),
        "a running task the truncated bgtasks lost must still be declared"
    );
    assert_eq!(
        sl.declared_task_summary_for_epoch(0),
        Some(DeclaredTaskSummary {
            non_teammate_running: 0,
            teammate_running: 1
        })
    );
}

#[test]
fn bgtasksummary_reporting_nothing_running_clears_an_overreported_declaration() {
    let state = crate::state::tests_support::make_test_app_state();
    let silence = bgtasks_test_session(&state, "test-bgsummary-clear");
    feed_bgtasks(&state, "test-bgsummary-clear", &silence, "running");
    assert!(silence.lock().declared_background_work_for_epoch(0));
    feed_tuic_verb(
        &state,
        "test-bgsummary-clear",
        &silence,
        "bgtasksummary",
        "shell%2Fcompleted%2Cteammate%2Fcompleted",
    );
    assert!(!silence.lock().declared_background_work_for_epoch(0));
    // An empty summary (field present, no entries) says nothing is outstanding.
    feed_tuic_verb(
        &state,
        "test-bgsummary-clear",
        &silence,
        "bgtasksummary",
        "",
    );
    assert!(!silence.lock().declared_background_work_for_epoch(0));
}

#[test]
fn bgtasksummary_is_stamped_with_the_sessions_current_turn_epoch() {
    let state = crate::state::tests_support::make_test_app_state();
    let silence = bgtasks_test_session(&state, "test-bgsummary-epoch");
    state
        .session_maps
        .session_states
        .get_mut("test-bgsummary-epoch")
        .unwrap()
        .turn_epoch = 4;
    feed_bgtasks(&state, "test-bgsummary-epoch", &silence, "completed");
    feed_tuic_verb(
        &state,
        "test-bgsummary-epoch",
        &silence,
        "bgtasksummary",
        "shell%2Frunning",
    );
    let sl = silence.lock();
    assert!(sl.declared_background_work_for_epoch(4));
    assert!(
        !sl.declared_background_work_for_epoch(5),
        "self-expires on a new turn"
    );
}

#[test]
fn bgtasksummary_without_a_bgtasks_observation_for_this_epoch_is_ignored() {
    // Orphan: nothing recorded yet.
    let state = crate::state::tests_support::make_test_app_state();
    let silence = bgtasks_test_session(&state, "test-bgsummary-orphan");
    feed_tuic_verb(
        &state,
        "test-bgsummary-orphan",
        &silence,
        "bgtasksummary",
        "shell%2Frunning",
    );
    {
        let sl = silence.lock();
        assert!(!sl.declared_background_work_for_epoch(0));
        assert!(sl.declared_task_summary_for_epoch(0).is_none());
    }

    // Epoch race: the two verbs of one hook fire are dispatched separately, and a new
    // prompt can land between them (bumping the epoch and wiping the observation). The
    // late summary must not stamp the NEW turn with the previous turn's data.
    let mut sl = SilenceState::new();
    sl.set_declared_background_work(false, 3);
    sl.reset_declared_background_work();
    sl.set_declared_task_summary(
        DeclaredTaskSummary {
            non_teammate_running: 1,
            teammate_running: 0,
        },
        4,
    );
    assert!(!sl.declared_background_work_for_epoch(4));
    assert!(sl.declared_task_summary_for_epoch(4).is_none());

    // ...and one for an older epoch than the recorded observation is ignored too.
    sl.set_declared_background_work(true, 5);
    sl.set_declared_task_summary(
        DeclaredTaskSummary {
            non_teammate_running: 0,
            teammate_running: 0,
        },
        4,
    );
    assert!(
        sl.declared_background_work_for_epoch(5),
        "a summary for another epoch must not clear this epoch's declaration"
    );
}

#[test]
fn a_new_bgtasks_without_a_summary_falls_back_to_counting_every_running_task() {
    // Mixed-version safety: an older hook sends only `bgtasks`. A leftover summary
    // from an earlier fire must not keep hiding a teammate that is now the only
    // information we have.
    let state = crate::state::tests_support::make_test_app_state();
    let silence = bgtasks_test_session(&state, "test-bgsummary-fallback");
    feed_bgtasks(&state, "test-bgsummary-fallback", &silence, "running");
    feed_tuic_verb(
        &state,
        "test-bgsummary-fallback",
        &silence,
        "bgtasksummary",
        "teammate%2Frunning",
    );
    assert!(
        !silence
            .lock()
            .declared_background_work_for_epoch_with(0, |_| false)
    );

    feed_bgtasks(&state, "test-bgsummary-fallback", &silence, "running");
    assert!(
        silence
            .lock()
            .declared_background_work_for_epoch_with(0, |_| false),
        "no fresh summary -> legacy behavior, every running task counts"
    );
}

#[test]
fn reset_declared_background_work_also_drops_the_summary() {
    let mut sl = SilenceState::new();
    sl.set_declared_background_work(true, 3);
    sl.set_declared_task_summary(
        DeclaredTaskSummary {
            non_teammate_running: 0,
            teammate_running: 2,
        },
        3,
    );
    assert!(sl.declared_task_summary_for_epoch(3).is_some());
    assert!(
        sl.declared_task_summary_for_epoch(4).is_none(),
        "stale epoch"
    );
    sl.reset_declared_background_work();
    assert!(sl.declared_task_summary_for_epoch(3).is_none());
    assert!(!sl.declared_background_work_for_epoch(3));
}

// --- explain: declared background work breakdown and swarm linkage ---

/// Lead `lead` (claude, idle shell) declaring `teammate_running` teammates,
/// with one linked teammate terminal `mate` in the given shell state.
fn explain_lead_with_teammate(
    state: &crate::state::AppState,
    lead: &str,
    mate: &str,
    mate_shell: u8,
    non_teammate_running: u32,
    teammate_running: u32,
) {
    let silence = bgtasks_test_session(state, lead);
    state
        .session_maps
        .session_states
        .get_mut(lead)
        .unwrap()
        .agent_type = Some("claude".into());
    {
        let mut sl = silence.lock();
        sl.set_declared_background_work(true, 0);
        sl.set_declared_task_summary(
            DeclaredTaskSummary {
                non_teammate_running,
                teammate_running,
            },
            0,
        );
    }
    state
        .session_maps
        .session_states
        .insert(mate.into(), crate::state::SessionState::default());
    state
        .session_maps
        .shell_states
        .insert(mate.into(), std::sync::atomic::AtomicU8::new(mate_shell));
    // Every real terminal has a SilenceState, and `explain` requires one.
    state.session_maps.silence_states.insert(
        mate.into(),
        std::sync::Arc::new(parking_lot::Mutex::new(SilenceState::new())),
    );
    crate::mcp_http::tmux_routes::link_teammate_for_test(state, "explain-swarm", lead, mate);
}

#[test]
fn explain_an_idle_teammate_is_declared_but_does_not_count() {
    // The shape of the stuck "Agent is busy" tab: a raw declaration that applies to
    // the current turn, yet nothing is actually working.
    let state = crate::state::tests_support::make_test_app_state();
    explain_lead_with_teammate(
        &state,
        "explain-lead-idle",
        "explain-mate-idle",
        SHELL_IDLE,
        0,
        1,
    );

    let explain = explain_session_state_impl(&state, "explain-lead-idle").expect("explain");
    let flag = &explain.epoch_flags.declared_background_work;
    assert!(flag.flag.declared);
    assert!(
        flag.flag.applies_now,
        "raw declaration still applies to this turn"
    );
    assert_eq!(flag.breakdown_source, "summary");
    assert_eq!(flag.teammate_running, Some(1));
    assert_eq!(flag.non_teammate_running, Some(0));
    assert!(!flag.teammates_busy);
    assert_eq!(
        flag.unlinked_teammates,
        Some(0),
        "every declared teammate is linked"
    );
    assert!(
        !flag.counts_now,
        "an idle teammate must not hold the lead working"
    );
    assert!(flag.age_ms.is_some());
    assert_eq!(
        flag.counts_now, explain.visible.declared_background_work,
        "counts_now is exactly what the state ladder used"
    );

    let swarm = explain
        .swarm
        .expect("a lead with a teammate reports its swarm");
    assert_eq!(swarm.lead_session_id, None);
    assert_eq!(swarm.teammates.len(), 1);
    assert_eq!(swarm.teammates[0].session_id, "explain-mate-idle");
    assert_eq!(swarm.teammates[0].shell_state, Some("idle"));
    assert!(!swarm.teammates[0].busy);
}

#[test]
fn explain_a_busy_teammate_counts_and_matches_the_visible_state() {
    let state = crate::state::tests_support::make_test_app_state();
    explain_lead_with_teammate(
        &state,
        "explain-lead-busy",
        "explain-mate-busy",
        SHELL_BUSY,
        0,
        1,
    );

    let explain = explain_session_state_impl(&state, "explain-lead-busy").expect("explain");
    let flag = &explain.epoch_flags.declared_background_work;
    assert!(flag.teammates_busy);
    assert!(flag.counts_now);
    assert_eq!(flag.counts_now, explain.visible.declared_background_work);
    assert_eq!(explain.visible.agent_state_rung, "background_work");
    assert_eq!(
        explain.swarm.unwrap().teammates[0].shell_state,
        Some("busy")
    );
}

#[test]
fn explain_on_a_teammate_reports_its_lead() {
    let state = crate::state::tests_support::make_test_app_state();
    explain_lead_with_teammate(
        &state,
        "explain-lead-of",
        "explain-mate-of",
        SHELL_IDLE,
        0,
        1,
    );

    let explain = explain_session_state_impl(&state, "explain-mate-of").expect("explain");
    let swarm = explain.swarm.expect("a teammate reports its lead");
    assert_eq!(swarm.lead_session_id.as_deref(), Some("explain-lead-of"));
    assert!(swarm.teammates.is_empty());
}

#[test]
fn explain_without_a_breakdown_reports_statuses_only_and_counts_everything() {
    // An older `tuic-hook` sends only `bgtasks`: every running task counts, and the
    // payload says so instead of implying a teammate split that was never received.
    let state = crate::state::tests_support::make_test_app_state();
    let silence = bgtasks_test_session(&state, "explain-legacy");
    state
        .session_maps
        .session_states
        .get_mut("explain-legacy")
        .unwrap()
        .agent_type = Some("claude".into());
    silence.lock().set_declared_background_work(true, 0);

    let explain = explain_session_state_impl(&state, "explain-legacy").expect("explain");
    let flag = &explain.epoch_flags.declared_background_work;
    assert_eq!(flag.breakdown_source, "statuses_only");
    assert_eq!(flag.teammate_running, None);
    assert_eq!(flag.non_teammate_running, None);
    assert!(flag.counts_now);
    assert!(
        explain.swarm.is_none(),
        "no linkage recorded -> no swarm section"
    );
}

#[test]
fn explain_with_no_declaration_reports_none() {
    let state = crate::state::tests_support::make_test_app_state();
    bgtasks_test_session(&state, "explain-none");
    let explain = explain_session_state_impl(&state, "explain-none").expect("explain");
    let flag = &explain.epoch_flags.declared_background_work;
    assert!(!flag.flag.declared);
    assert_eq!(flag.breakdown_source, "none");
    assert_eq!(flag.age_ms, None);
    assert!(!flag.counts_now);
}

#[test]
fn explain_serializes_the_declared_work_keys_the_old_shape_had_plus_the_new_ones() {
    let state = crate::state::tests_support::make_test_app_state();
    explain_lead_with_teammate(&state, "explain-ser", "explain-mate-ser", SHELL_IDLE, 1, 2);
    let v =
        serde_json::to_value(explain_session_state_impl(&state, "explain-ser").unwrap()).unwrap();
    let w = &v["epoch_flags"]["declared_background_work"];
    // Pre-existing keys, flattened, must stay where consumers read them.
    assert_eq!(w["declared"], true);
    assert_eq!(w["declared_turn_epoch"], 0);
    assert_eq!(w["applies_now"], true);
    // New keys.
    assert_eq!(w["breakdown_source"], "summary");
    assert_eq!(w["non_teammate_running"], 1);
    assert_eq!(w["teammate_running"], 2);
    assert_eq!(w["teammates_busy"], false);
    assert_eq!(w["unlinked_teammates"], 1, "two declared, one linked pane");
    assert_eq!(w["counts_now"], true, "real background work still counts");
    assert_eq!(v["swarm"]["teammates"][0]["session_id"], "explain-mate-ser");
}

#[test]
fn explain_declared_teammates_no_linked_pane_accounts_for_are_reported_and_count() {
    // Two teammates declared running but only one has a linked pane (and it is idle):
    // the other cannot be shown idle, so the declaration counts — and the payload says
    // why, instead of leaving `counts_now: true` unexplained next to an idle teammate.
    let state = crate::state::tests_support::make_test_app_state();
    explain_lead_with_teammate(
        &state,
        "explain-unlinked",
        "explain-mate-un",
        SHELL_IDLE,
        0,
        2,
    );

    let explain = explain_session_state_impl(&state, "explain-unlinked").expect("explain");
    let flag = &explain.epoch_flags.declared_background_work;
    assert_eq!(flag.teammate_running, Some(2));
    assert_eq!(flag.unlinked_teammates, Some(1));
    assert!(!flag.teammates_busy, "the one linked teammate is idle");
    assert!(
        flag.counts_now,
        "the unaccounted-for teammate keeps the lead working (fail-safe)"
    );
    assert_eq!(flag.counts_now, explain.visible.declared_background_work);
    assert!(
        flag.consistent_with_visible,
        "no concurrent flip, so the capture is not torn"
    );
}

// --- teammate-aware declared-work read: the three pty.rs consumers ---

/// A Claude lead whose `Stop` declared `declared_teammates` teammates running,
/// with (optionally) one linked teammate terminal in `mate_shell`. The lead has a
/// parent so the parent-idle notification path has somewhere to deliver.
fn teammate_gate_fixture(
    state: &crate::state::AppState,
    lead: &str,
    parent: &str,
    lead_shell: u8,
    declared_teammates: u32,
    linked_mate_shell: Option<u8>,
) -> Arc<Mutex<SilenceState>> {
    state
        .session_maps
        .session_parent
        .insert(lead.to_string(), parent.to_string());
    state.agent_inbox.entry(parent.to_string()).or_default();
    state.session_maps.session_states.insert(
        lead.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            turn_epoch: 0,
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        lead.to_string(),
        std::sync::atomic::AtomicU8::new(lead_shell),
    );
    let mut silence = SilenceState::new();
    silence.set_declared_background_work(true, 0);
    silence.set_declared_task_summary(
        DeclaredTaskSummary {
            non_teammate_running: 0,
            teammate_running: declared_teammates,
        },
        0,
    );
    let silence = Arc::new(Mutex::new(silence));
    state
        .session_maps
        .silence_states
        .insert(lead.to_string(), silence.clone());
    if let Some(shell) = linked_mate_shell {
        let mate = format!("{lead}-mate");
        state
            .session_maps
            .shell_states
            .insert(mate.clone(), std::sync::atomic::AtomicU8::new(shell));
        crate::mcp_http::tmux_routes::link_teammate_for_test(
            state,
            &format!("gate-{lead}"),
            lead,
            &mate,
        );
    }
    silence
}

#[test]
fn an_idle_teammate_does_not_defer_the_leads_parent_idle_notification() {
    let state = crate::state::tests_support::make_test_app_state();
    teammate_gate_fixture(
        &state,
        "lead-pi-idle",
        "parent-pi-idle",
        SHELL_BUSY,
        1,
        Some(SHELL_IDLE),
    );
    assert!(try_shell_transition(
        &state,
        "lead-pi-idle",
        SHELL_BUSY,
        SHELL_IDLE,
        true
    ));
    assert_eq!(
        state.agent_inbox.get("parent-pi-idle").unwrap().len(),
        1,
        "only an idle teammate is declared: the lead's idle edge must be reported"
    );
}

#[test]
fn a_busy_or_unlinked_teammate_defers_the_leads_parent_idle_notification() {
    for (name, declared, mate) in [
        ("busy", 1, Some(SHELL_BUSY)),
        ("unlinked", 1, None),
        ("surplus", 2, Some(SHELL_IDLE)),
    ] {
        let state = crate::state::tests_support::make_test_app_state();
        let lead = format!("lead-pi-{name}");
        let parent = format!("parent-pi-{name}");
        teammate_gate_fixture(&state, &lead, &parent, SHELL_BUSY, declared, mate);
        assert!(try_shell_transition(
            &state, &lead, SHELL_BUSY, SHELL_IDLE, true
        ));
        assert_eq!(
            state.agent_inbox.get(&parent).unwrap().len(),
            0,
            "{name}: a teammate that may still be working must defer the idle report"
        );
    }
}

#[test]
fn an_idle_teammate_does_not_defer_suggest_publication_but_a_busy_or_unlinked_one_does() {
    let publish = |name: &str, declared: u32, mate: Option<u8>| -> (bool, usize) {
        let state = crate::state::tests_support::make_test_app_state();
        let lead = format!("lead-sg-{name}");
        let parent = format!("parent-sg-{name}");
        let silence = teammate_gate_fixture(&state, &lead, &parent, SHELL_IDLE, declared, mate);
        silence
            .lock()
            .mark_suggest_candidate(vec!["Review result".to_string()], 0);
        let emitted = emit_pending_suggest_if_idle(&state, &silence, &lead);
        (emitted, state.agent_inbox.get(&parent).unwrap().len())
    };
    assert_eq!(publish("idle", 1, Some(SHELL_IDLE)), (true, 1));
    assert_eq!(publish("busy", 1, Some(SHELL_BUSY)), (false, 0));
    assert_eq!(publish("unlinked", 1, None), (false, 0));
    assert_eq!(publish("surplus", 2, Some(SHELL_IDLE)), (false, 0));
}

#[cfg(unix)]
#[test]
fn an_idle_teammate_does_not_block_standby_but_a_busy_or_unlinked_one_does() {
    let blocks = |name: &str, declared: u32, mate: Option<u8>| -> (bool, bool) {
        let state = crate::state::tests_support::make_test_app_state();
        let lead = format!("lead-sb-{name}");
        let silence = teammate_gate_fixture(
            &state,
            &lead,
            &format!("parent-sb-{name}"),
            SHELL_IDLE,
            declared,
            mate,
        );
        let guard = silence.lock();
        let locked = background_activity_blocks_standby_with_silence(&state, &lead, Some(&guard));
        drop(guard);
        // The self-locking variant must agree with the already-locked one.
        (background_activity_blocks_standby(&state, &lead), locked)
    };
    assert_eq!(blocks("idle", 1, Some(SHELL_IDLE)), (false, false));
    assert_eq!(blocks("busy", 1, Some(SHELL_BUSY)), (true, true));
    assert_eq!(blocks("unlinked", 1, None), (true, true));
    assert_eq!(blocks("surplus", 2, Some(SHELL_IDLE)), (true, true));
}

#[test]
fn explain_a_closed_teammate_is_not_listed_and_does_not_account_for_a_declared_one() {
    // Lead declares one teammate running; that teammate's terminal has since closed but
    // its pane is still in the lazily reconciled topology. It must not vouch (U2): the
    // lead keeps reading working until its next Stop refreshes the list, and the payload
    // shows it as an unaccounted-for teammate instead of a linked idle one.
    let state = crate::state::tests_support::make_test_app_state();
    explain_lead_with_teammate(
        &state,
        "explain-lead-closed",
        "explain-mate-closed",
        SHELL_IDLE,
        0,
        1,
    );
    state
        .session_maps
        .shell_states
        .remove("explain-mate-closed");

    let explain = explain_session_state_impl(&state, "explain-lead-closed").expect("explain");
    let flag = &explain.epoch_flags.declared_background_work;
    assert!(
        explain.swarm.is_none(),
        "the closed terminal is not a linked teammate"
    );
    assert_eq!(flag.unlinked_teammates, Some(1));
    assert!(flag.counts_now);
    assert!(flag.consistent_with_visible);
}

#[tokio::test(flavor = "current_thread")]
async fn async_spawn_wrapper_does_not_block_the_runtime_worker() {
    let started = std::time::Instant::now();
    let spawn = tokio::spawn(run_pty_spawn_blocking(|| {
        std::thread::sleep(std::time::Duration::from_millis(100));
        Ok::<_, String>(())
    }));

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    assert!(
        started.elapsed() < std::time::Duration::from_millis(80),
        "blocking spawn work occupied the async runtime"
    );
    spawn.await.unwrap().unwrap();
}

/// Save/restore a real process env var for a test's duration. `resolve_shell`/
/// `default_shell` read `$SHELL`/`$COMSPEC` directly with no test-only override
/// hook (unlike `config_dir()`'s `CONFIG_DIR_OVERRIDE`) — same minimal guard
/// shape as `github_auth.rs`'s private `EnvVar` and `pty.rs`'s own
/// `inject_unix_terminal_env_tests::TestEnvVar`, duplicated locally rather than
/// shared, matching how this codebase already keeps this kind of tiny
/// per-module test helper un-shared elsewhere.
struct ShellEnvVar(&'static str, Option<String>);

impl ShellEnvVar {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        unsafe { std::env::set_var(key, value) };
        Self(key, previous)
    }

    fn unset(key: &'static str) -> Self {
        let previous = std::env::var(key).ok();
        unsafe { std::env::remove_var(key) };
        Self(key, previous)
    }
}

impl Drop for ShellEnvVar {
    fn drop(&mut self) {
        match self.1.take() {
            Some(previous) => unsafe { std::env::set_var(self.0, previous) },
            None => unsafe { std::env::remove_var(self.0) },
        }
    }
}

#[test]
fn resolve_shell_prefers_an_explicit_override_over_the_environment() {
    let _guard = ShellEnvVar::set("SHELL", "/bin/env-shell");
    assert_eq!(
        super::resolve_shell(Some("/bin/explicit-shell".to_string())),
        "/bin/explicit-shell"
    );
}

#[test]
fn resolve_shell_expands_a_tilde_in_an_explicit_override() {
    let expanded = super::resolve_shell(Some("~/bin/myshell".to_string()));
    assert!(
        !expanded.starts_with('~'),
        "resolve_shell must expand a leading ~ via expand_tilde: {expanded}"
    );
    assert!(
        expanded.ends_with("/bin/myshell"),
        "expansion must preserve the rest of the path: {expanded}"
    );
}

#[cfg(not(windows))]
#[test]
fn resolve_shell_falls_back_to_the_shell_env_var_when_no_override_is_given() {
    let _guard = ShellEnvVar::set("SHELL", "/bin/from-env");
    assert_eq!(super::resolve_shell(None), "/bin/from-env");
}

#[cfg(not(windows))]
#[test]
fn default_shell_falls_back_to_bin_bash_when_shell_is_unset() {
    let _guard = ShellEnvVar::unset("SHELL");
    assert_eq!(super::default_shell(), "/bin/bash");
}

#[test]
fn pty_parent_env_sanitizer_removes_no_color_and_allows_override() {
    let mut cmd = CommandBuilder::new("/bin/sh");
    // Simulate CommandBuilder's inherited parent snapshot without mutating
    // the process-global environment used by other tests.
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("NO_COLOR", "1");

    sanitize_pty_parent_env(&mut cmd);

    assert_eq!(
        cmd.get_env("TERM"),
        Some(std::ffi::OsStr::new("xterm-256color"))
    );
    assert_eq!(
        cmd.get_env("COLORTERM"),
        Some(std::ffi::OsStr::new("truecolor"))
    );
    assert_eq!(cmd.get_env("NO_COLOR"), None);

    cmd.env("NO_COLOR", "intentional");
    assert_eq!(
        cmd.get_env("NO_COLOR"),
        Some(std::ffi::OsStr::new("intentional"))
    );
}

#[test]
fn pty_spawn_env_child_checks_inherited_build_context() {
    if std::env::var_os("TUIC_TEST_PTY_BUILD_ENV").is_none() {
        return;
    }
    assert_eq!(
        std::env::var("CARGO_TARGET_DIR").unwrap(),
        "/tuic-dev-build-target"
    );
    let mut agent = CommandBuilder::new("agent");
    assert_eq!(
        agent.get_env("CARGO_TARGET_DIR"),
        Some(std::ffi::OsStr::new("/tuic-dev-build-target"))
    );
    sanitize_pty_parent_env(&mut agent);
    let shell = build_shell_command("/bin/sh");
    for cmd in [&agent, &shell] {
        for key in [
            "CARGO_TARGET_DIR",
            "CARGO_MANIFEST_DIR",
            "CARGO_MANIFEST_PATH",
            "CARGO_MANIFEST_LINKS",
            "CARGO_PKG_NAME",
            "OUT_DIR",
            "RUSTDOC",
            "CARGO_INCREMENTAL",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "HOST_CC",
            "HOST_CXX",
            "MBX_SOCKET",
            "MBX_FUTURE_BUILD_KEY",
            "CARGO",
            "CARGO_PRIMARY_PACKAGE",
            "CARGO_BIN_NAME",
            "CARGO_CRATE_NAME",
            "CARGO_MAKEFLAGS",
            "CARGO_TARGET_TMPDIR",
            "CARGO_BIN_EXE_tuicommander",
            "CARGO_FEATURE_DESKTOP",
            "CARGO_CFG_TARGET_OS",
            "RUSTFLAGS",
            "RUSTC",
            "RUSTC_LINKER",
            "DEP_TUIC_NATIVE_PATH",
            "CARGO_ENCODED_RUSTFLAGS",
            "TARGET",
            "HOST",
            "PROFILE",
            "NUM_JOBS",
            "OPT_LEVEL",
            "DEBUG",
        ] {
            assert_eq!(cmd.get_env(key), None, "{key} leaked into a PTY");
        }
        assert_eq!(
            cmd.get_env("CARGO_HOME"),
            Some(std::ffi::OsStr::new("/user/cargo"))
        );
        assert_eq!(
            cmd.get_env("CARGO_TERM_COLOR"),
            Some(std::ffi::OsStr::new("always"))
        );
        assert_eq!(
            cmd.get_env("TUIC_TEST_USER_ENV"),
            Some(std::ffi::OsStr::new("keep-me"))
        );
    }
    agent.env("CARGO_TARGET_DIR", "/intentional-agent-target");
    agent.env("CARGO_INCREMENTAL", "1");
    agent.env("RUSTC_WRAPPER", "/intentional-agent-wrapper");
    agent.env("MBX_SOCKET", "/intentional-agent-mbx.sock");
    assert_eq!(
        agent.get_env("CARGO_TARGET_DIR"),
        Some(std::ffi::OsStr::new("/intentional-agent-target"))
    );
    assert_eq!(
        agent.get_env("CARGO_INCREMENTAL"),
        Some(std::ffi::OsStr::new("1"))
    );
    assert_eq!(
        agent.get_env("RUSTC_WRAPPER"),
        Some(std::ffi::OsStr::new("/intentional-agent-wrapper"))
    );
    assert_eq!(
        agent.get_env("MBX_SOCKET"),
        Some(std::ffi::OsStr::new("/intentional-agent-mbx.sock"))
    );
}

#[cfg(unix)]
#[test]
fn standby_refuses_session_with_background_work() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "background-standby";
    state.session_maps.session_states.insert(
        session_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
            ..Default::default()
        },
    );
    assert_eq!(standby_session(&state, session_id), Ok(false));
    assert!(!state.session_maps.standby_sessions.contains_key(session_id));
}

#[cfg(unix)]
#[test]
fn standby_refuses_session_with_pending_background_probe() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "background-probe-standby";
    state.session_maps.session_states.insert(
        session_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_probe_turn_epoch: Some(3),
            turn_epoch: 3,
            ..Default::default()
        },
    );

    assert!(background_activity_blocks_standby(&state, session_id));
    assert_eq!(standby_session(&state, session_id), Ok(false));
    assert!(!state.session_maps.standby_sessions.contains_key(session_id));
}

#[test]
fn tombstone_transient_cleanup_removes_swarm_maps() {
    // F3: all per-child swarm state must be cleaned on exit.
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "sess-cleanup";
    let mcp_sid = "mcp-sess-cleanup";

    state
        .session_maps
        .session_parent
        .insert(sid.to_string(), "parent-sess".to_string());
    state
        .session_maps
        .shell_state_since_ms
        .insert(sid.to_string(), std::sync::atomic::AtomicU64::new(42));
    state
        .mcp
        .to_session
        .insert(mcp_sid.to_string(), sid.to_string());
    state
        .mcp
        .session_to_mcp
        .insert(sid.to_string(), vec![mcp_sid.to_string()]);
    state.peer_agents.insert(
        sid.to_string(),
        crate::state::PeerAgent {
            tuic_session: sid.to_string(),
            mcp_session_id: mcp_sid.to_string(),
            name: "worker".to_string(),
            project: None,
            registered_at: 1,
        },
    );
    state.agent_inbox.entry(sid.to_string()).or_default();
    state.agent_inbox_evictions.insert(sid.to_string(), 2);
    state.session_maps.has_osc133_integration.insert(sid.to_string(), ());
    state.session_maps.has_tuic_state_integration.insert(sid.to_string(), ());
    state.session_maps.turn_error_flags.insert(sid.to_string(), ());

    tombstone_transient_cleanup(sid, &state);

    assert!(
        !state.session_maps.session_parent.contains_key(sid),
        "session_parent must be removed"
    );
    assert!(
        !state.session_maps.shell_state_since_ms.contains_key(sid),
        "shell_state_since_ms must be removed"
    );
    assert!(
        !state.mcp.to_session.contains_key(mcp_sid),
        "mcp_to_session entry must be removed"
    );
    assert!(
        !state.mcp.session_to_mcp.contains_key(sid),
        "session_to_mcp entry must be removed"
    );
    assert!(!state.peer_agents.contains_key(sid));
    assert!(!state.agent_inbox.contains_key(sid));
    assert!(!state.agent_inbox_evictions.contains_key(sid));
    // A session that exits normally (as opposed to explicit close/kill)
    // must not leave these markers behind — each is keyed by UUID with
    // no other reaper, so a leak here is permanent for the process
    // lifetime, not just until the next session reuses the id.
    assert!(
        !state.session_maps.has_osc133_integration.contains_key(sid),
        "must not leak a permanent entry per session UUID on normal exit"
    );
    assert!(
        !state.session_maps.has_tuic_state_integration.contains_key(sid),
        "must not leak a permanent entry per session UUID on normal exit"
    );
    assert!(
        !state.session_maps.turn_error_flags.contains_key(sid),
        "must not leak a pending failure flag past a normal session exit"
    );
}

fn completed_agent_session(state: &crate::state::AppState, sid: &str) {
    agent_session(state, sid, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .suggested_actions = Some(vec!["Review result".to_string()]);
    state
        .session_maps
        .silence_states
        .get(sid)
        .unwrap()
        .lock()
        .mark_suggest_candidate(vec!["Review result".to_string()], 0);
}

fn assert_new_turn_silence_evidence(silence: &SilenceState) {
    assert!(silence.explicit_busy());
    assert!(!silence.hook_busy());
    assert!(!silence.explicit_idle());
    assert!(!silence.idle_confirmed());
    assert!(silence.last_status_line_at.is_some());
    assert!(silence.screen_ready_pending_since.is_none());
    assert!(silence.interrupt_requested_at.is_none());
    assert!(silence.turn_started_by_input());
    assert!(!silence.evidence.activity_seen);
    assert!(!silence.completion_declared);
    assert!(silence.pending_suggest_items.is_none());
}

#[test]
fn submitted_epoch_and_busy_transition_are_one_critical_section() {
    use std::sync::atomic::Ordering;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let session_id = "submitted-atomic";
    completed_agent_session(&state, session_id);
    let (start_tx, start_rx) = std::sync::mpsc::channel();
    let (lock_held_tx, lock_held_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let observer_state = Arc::clone(&state);
    let observer = std::thread::spawn(move || {
        start_rx.recv().unwrap();
        let silence = observer_state
            .session_maps
            .silence_states
            .get(session_id)
            .unwrap()
            .clone();
        lock_held_tx.send(silence.try_lock().is_none()).unwrap();
        let transitioned =
            try_shell_transition(&observer_state, session_id, SHELL_IDLE, SHELL_IDLE, false);
        finished_tx.send(transitioned).unwrap();
    });

    note_submitted_input_with_hook(&state, session_id, || {
        assert_eq!(
            state
                .session_maps
                .session_states
                .get(session_id)
                .unwrap()
                .turn_epoch,
            1
        );
        start_tx.send(()).unwrap();
        assert!(
            lock_held_rx.recv().unwrap(),
            "epoch mutation must retain the lifecycle lock until BUSY"
        );
    });

    assert!(
        !finished_rx.recv().unwrap(),
        "observer must see BUSY after the submitted-turn reservation"
    );
    observer.join().unwrap();
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
}

#[test]
fn new_turn_wins_before_queued_old_idle_transition() {
    use std::sync::atomic::Ordering;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let child_id = "inverse-idle-child";
    let parent_id = "inverse-idle-parent";
    agent_session(&state, child_id, SHELL_BUSY);
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();

    let (snapshotted_tx, snapshotted_rx) = std::sync::mpsc::channel();
    let (continue_tx, continue_rx) = std::sync::mpsc::channel();
    let old_transition_state = Arc::clone(&state);
    let old_transition = std::thread::spawn(move || {
        let observed_turn_epoch = old_transition_state
            .session_maps
            .session_states
            .get(child_id)
            .map(|session| session.turn_epoch);
        try_shell_transition_with_hooks(
            ShellTransitionRequest {
                state: &old_transition_state,
                session_id: child_id,
                expected: SHELL_BUSY,
                new: SHELL_IDLE,
                notify_parent: true,
                observed_turn_epoch,
            },
            ShellTransitionHooks {
                after_epoch_snapshot: || {
                    snapshotted_tx.send(()).unwrap();
                    continue_rx.recv().unwrap();
                },
                after_cas: || {},
                before_parent_dispatch: || {},
            },
        )
    });

    snapshotted_rx.recv().unwrap();
    note_submitted_input(&state, child_id);
    continue_tx.send(()).unwrap();

    assert!(
        !old_transition.join().unwrap(),
        "an idle transition from the prior epoch must not publish"
    );
    assert_eq!(
        state
            .session_maps
            .session_states
            .get(child_id)
            .unwrap()
            .turn_epoch,
        1
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(child_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());
}

#[test]
fn explicit_idle_evidence_from_prior_turn_cannot_idle_new_submission() {
    use std::sync::atomic::Ordering;

    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "explicit-idle-epoch";
    agent_session(&state, session_id, SHELL_BUSY);

    transition_explicit_shell_state_with_hook(&state, session_id, SHELL_IDLE, "idle", true, || {
        note_submitted_input(&state, session_id)
    });

    assert_eq!(
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .turn_epoch,
        1
    );
    assert_new_turn_silence_evidence(
        &state
            .session_maps
            .silence_states
            .get(session_id)
            .unwrap()
            .lock(),
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY,
        "an explicit idle marker observed before the new input must be discarded"
    );
}

#[test]
fn silence_idle_decision_from_prior_turn_cannot_idle_new_submission() {
    use std::sync::atomic::{AtomicU64, Ordering};

    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "silence-idle-epoch";
    agent_session(&state, session_id, SHELL_BUSY);
    state.session_maps.last_output_ms.insert(
        session_id.to_string(),
        AtomicU64::new(now_epoch_ms().saturating_sub(AGENT_IDLE_MS + 1)),
    );

    let decision = should_transition_idle_with_hook(&state, session_id, || {
        note_submitted_input(&state, session_id);
    });
    assert!(decision.should_transition);
    assert_eq!(decision.turn_epoch, Some(0));

    assert!(!try_shell_transition_for_epoch(
        &state,
        session_id,
        SHELL_BUSY,
        SHELL_IDLE,
        true,
        decision.turn_epoch,
    ));
    assert_eq!(
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .turn_epoch,
        1
    );
    assert_new_turn_silence_evidence(
        &state
            .session_maps
            .silence_states
            .get(session_id)
            .unwrap()
            .lock(),
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
}

#[test]
fn parent_dispatch_runs_after_child_lifecycle_lock_release() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "dispatch-child";
    let parent_id = "dispatch-parent";
    agent_session(&state, child_id, SHELL_BUSY);
    agent_session(&state, parent_id, SHELL_IDLE);
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    let observed_turn_epoch = state
        .session_maps
        .session_states
        .get(child_id)
        .map(|session| session.turn_epoch);

    assert!(try_shell_transition_with_hooks(
        ShellTransitionRequest {
            state: &state,
            session_id: child_id,
            expected: SHELL_BUSY,
            new: SHELL_IDLE,
            notify_parent: true,
            observed_turn_epoch,
        },
        ShellTransitionHooks {
            after_epoch_snapshot: || {},
            after_cas: || {},
            before_parent_dispatch: || {
                let silence = state
                    .session_maps
                    .silence_states
                    .get(child_id)
                    .unwrap()
                    .clone();
                assert!(
                    silence.try_lock().is_some(),
                    "child lifecycle lock must be released before parent PTY dispatch"
                );
            },
        },
    ));
    assert_eq!(state.agent_inbox.get(parent_id).unwrap().len(), 1);
}

#[test]
fn codex_heuristic_idle_is_not_safe_for_injection_or_standby() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "codex-heuristic", SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut("codex-heuristic")
        .unwrap()
        .agent_type = Some("codex".to_string());
    state
        .session_maps
        .silence_states
        .get("codex-heuristic")
        .unwrap()
        .lock()
        .force_idle_unconfirmed();

    assert!(!idle_is_confirmed(&state, "codex-heuristic"));
    assert!(!should_inject_now(&state, "codex-heuristic"));

    state
        .session_maps
        .silence_states
        .get("codex-heuristic")
        .unwrap()
        .lock()
        .confirm_idle();
    assert!(idle_is_confirmed(&state, "codex-heuristic"));
    assert!(should_inject_now(&state, "codex-heuristic"));
}

#[test]
fn uncertain_injection_cannot_be_cleared_by_a_stale_ready_screen() {
    let mut silence = SilenceState::new();
    let token = silence.begin_injection_claim(true);
    silence.mark_injection_uncertain(token);
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);

    assert!(!silence.note_ready_screen());
    assert!(silence.injection_delivery_uncertain);
    assert!(!silence.idle_confirmed());
}

#[test]
fn deliver_reenqueue_recovers_message_when_idle_races_enqueue() {
    // CONC-A (story 101-20e3): the sender's should_inject_now read and its
    // pending_injections push are not atomic vs a concurrent BUSY→IDLE flush. If the
    // silence timer transitions to idle and drains the (still-empty) queue between
    // them, the message would be stranded until the NEXT idle cycle. The post-enqueue
    // re-check must recover it. Stress the race with a barrier: after both the sender
    // and the transition+flush complete with the session ending idle, the queue MUST
    // be empty (message delivered) regardless of interleaving. Pre-fix this fails in
    // the bug window (sender reads busy, timer flushes empty, sender enqueues with no
    // recovery). With no live PTY in this unit test, the recovered delivery
    // must remain queued exactly once rather than being lost or duplicated.
    use std::sync::{Arc, Barrier};
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    for i in 0..500 {
        agent_session(&state, "race", SHELL_BUSY);
        state.pending_injections.remove("race");

        let barrier = Arc::new(Barrier::new(2));
        let (s1, b1) = (Arc::clone(&state), Arc::clone(&barrier));
        let sender = std::thread::spawn(move || {
            b1.wait();
            deliver_notice_to_pty(&s1, "race", "[TUIC message from lead] go");
        });
        let (s2, b2) = (Arc::clone(&state), Arc::clone(&barrier));
        let timer = std::thread::spawn(move || {
            b2.wait();
            // Silence timer: BUSY→IDLE also runs flush_pending_injections on idle.
            s2.session_maps
                .silence_states
                .get("race")
                .unwrap()
                .lock()
                .confirm_idle();
            try_shell_transition(&s2, "race", SHELL_BUSY, SHELL_IDLE, false);
            emit_shell_state(&s2, "race", "idle");
            flush_pending_injections(&s2, "race")
        });
        sender.join().unwrap();
        if let Some(worker) = timer.join().unwrap() {
            worker.join().unwrap();
        }

        let queued = state
            .pending_injections
            .get("race")
            .map(|q| q.len())
            .unwrap_or(0);
        assert_eq!(queued, 1, "iteration {i}: message lost or duplicated");
    }
}

// ---- CONC-B (story 100-e303 / commit 5410cc3d): resize_session_core ----
// resize_session_core serializes the whole grid+PTY resize for a session
// under one per-session lock so two concurrent differing resizes can never
// interleave and leave grid and PTY at mismatched dimensions. These cover
// the invalid-dims edge, the no-op guard, the (0,0) startup-dims seed, and
// the concurrent-race invariant (mirrors the CONC-A barrier test above).

/// Insert a live VtLogBuffer at the given dims so the grid path in
/// resize_session_core runs against a real grid.
fn seed_vt_grid(state: &crate::state::AppState, sid: &str, rows: u16, cols: u16) {
    state.grid.vt_log_buffers.insert(
        sid.to_string(),
        Mutex::new(VtLogBuffer::new(rows, cols, 1000)),
    );
}

/// The reflow rewraps the whole ring and serializes a full frame under the VT
/// mutex. Run inline in the IPC handler — on macOS, the main thread — a
/// drag-resize froze the WebView for the length of every reflow it fired.
#[tokio::test]
async fn a_resize_reflows_off_the_calling_thread() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "resize-off-thread";
    seed_vt_grid(&state, sid, 24, 80);

    let caller = std::thread::current().id();
    // The session has no PTY, so this ends in "Session not found" — after the
    // grid reflow, which is precisely the work that must not run here.
    let _ = resize_session_off_thread(&state, sid.to_string(), 40, 120, None, None).await;

    assert!(resize_thread(sid).is_some(), "the reflow never ran at all");
    assert_ne!(
        resize_thread(sid),
        Some(caller),
        "the reflow ran on the calling thread"
    );
}

#[test]
fn resize_rejects_zero_dims() {
    let state = crate::state::tests_support::make_test_app_state();
    // rows==0 / cols==0 are rejected before any lock, grid, or PTY work — the
    // (0,0) pair is reserved as the "never applied" sentinel inside the lock.
    assert!(resize_session_core(&state, "s", 0, 80, None, None).is_err());
    assert!(resize_session_core(&state, "s", 24, 0, None, None).is_err());
    // A rejected resize must not even create a resize_locks entry.
    assert!(!state.session_maps.resize_locks.contains_key("s"));
}

#[test]
fn resize_noop_guard_returns_none_on_matching_dims() {
    let state = crate::state::tests_support::make_test_app_state();
    // Pre-seed the last-applied dims, as if a prior resize reached the PTY.
    state
        .session_maps
        .resize_locks
        .insert("s".to_string(), Arc::new(Mutex::new((24, 80))));
    // Same dims → no-op returning None WITHOUT touching the (absent) session.
    // Without the guard this would fall through to sessions.get and fail with
    // "Session not found", so Ok(None) proves the guard short-circuited first.
    assert_eq!(resize_session_core(&state, "s", 24, 80, None, None), Ok(None));
}

#[test]
fn resize_seeds_applied_from_grid_and_noops_at_startup_dims() {
    let state = crate::state::tests_support::make_test_app_state();
    // Grid exists at the startup dims but resize_locks is empty → the lock
    // opens at the (0,0) never-applied sentinel.
    seed_vt_grid(&state, "s", 24, 80);
    // A first resize matching only the startup dims must seed *applied from
    // the live grid and then no-op — no gratuitous SIGWINCH, no session touch.
    assert_eq!(resize_session_core(&state, "s", 24, 80, None, None), Ok(None));
    // The seed must have populated resize_locks with the live grid dims.
    assert_eq!(
        *state.session_maps.resize_locks.get("s").unwrap().lock(),
        (24, 80),
        "first resize must seed the last-applied dims from the live grid"
    );
}

/// Build a real PTY session (openpty + a long-lived child) at the given dims,
/// plus a matching VtLogBuffer, so resize_session_core reaches the real
/// master.resize() ioctl and get_size() reflects it.
#[cfg(unix)]
fn spawn_real_pty_session(state: &crate::state::AppState, sid: &str, rows: u16, cols: u16) {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    let mut cmd = CommandBuilder::new("/bin/sh");
    cmd.args(["-c", "sleep 30"]);
    let child = pair.slave.spawn_command(cmd).expect("spawn shell");
    let master = pair.master;
    let writer = master.take_writer().expect("writer");
    state.session_maps.sessions.insert(
        sid.to_string(),
        Mutex::new(PtySession {
            launch_receipt: None,
            writer: Arc::new(Mutex::new(writer)),
            master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            initial_cwd: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: false,
            shell: "/bin/sh".to_string(),
        }),
    );
    seed_vt_grid(state, sid, rows, cols);
}

/// CONC-B invariant: two concurrent resizes with different dims must leave the
/// grid AND the PTY at the same dimensions — both equal to whichever call
/// acquired the per-session lock last — never a grid/PTY mismatch. Stress the
/// race with a barrier over many iterations, like the CONC-A test above.
#[cfg(unix)]
#[test]
fn concurrent_differing_resizes_leave_grid_and_pty_consistent() {
    use std::sync::Barrier;
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "resize-race";
    spawn_real_pty_session(&state, sid, 24, 80);

    const A: (u16, u16) = (30, 100);
    const B: (u16, u16) = (40, 120);

    for i in 0..100 {
        let barrier = Arc::new(Barrier::new(2));
        let (s1, b1) = (Arc::clone(&state), Arc::clone(&barrier));
        let t1 = std::thread::spawn(move || {
            b1.wait();
            let _ = resize_session_core(&s1, sid, A.0, A.1, None, None);
        });
        let (s2, b2) = (Arc::clone(&state), Arc::clone(&barrier));
        let t2 = std::thread::spawn(move || {
            b2.wait();
            let _ = resize_session_core(&s2, sid, B.0, B.1, None, None);
        });
        t1.join().unwrap();
        t2.join().unwrap();

        // The recorded applied dims, the live grid dims, and the real PTY size
        // must all agree, and agree on one of the two racing targets.
        let applied = *state.session_maps.resize_locks.get(sid).unwrap().lock();
        let (grid_rows, grid_cols) = {
            let vt = state.grid.vt_log_buffers.get(sid).unwrap();
            let vt = vt.lock();
            (vt.grid_screen_lines() as u16, vt.grid_columns() as u16)
        };
        let pty_size = state
            .session_maps
            .sessions
            .get(sid)
            .unwrap()
            .lock()
            .master
            .get_size()
            .expect("get_size");
        assert_eq!(
            (grid_rows, grid_cols),
            applied,
            "iter {i}: grid dims must match recorded applied dims"
        );
        assert_eq!(
            (pty_size.rows, pty_size.cols),
            applied,
            "iter {i}: PTY size must match recorded applied dims"
        );
        assert!(
            applied == A || applied == B,
            "iter {i}: applied {applied:?} must be one of the racing targets"
        );
    }
}

#[test]
fn idle_transition_emits_before_submitting_one_pending_message() {
    use std::collections::VecDeque;
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "sess", SHELL_BUSY);
    let mut q = VecDeque::new();
    q.push_back(crate::state::PendingInjection::notice("msg-1"));
    q.push_back(crate::state::PendingInjection::notice("msg-2"));
    state.pending_injections.insert("sess".to_string(), q);

    // The transition is driven by verified ready-screen/Stop evidence in
    // production. Model that evidence before testing its delivery side effect.
    state
        .session_maps
        .silence_states
        .get("sess")
        .unwrap()
        .lock()
        .confirm_idle();

    let mut events = state.event_bus.subscribe();
    assert!(try_shell_transition(
        &state, "sess", SHELL_BUSY, SHELL_IDLE, false
    ));
    emit_shell_state(&state, "sess", "idle");
    flush_one_pending_as_submitted(&state, "sess");

    assert_eq!(
        state.pending_injections.get("sess").map(|q| q.len()),
        Some(1),
        "only one queued message may be submitted per idle turn"
    );
    let states: Vec<String> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            crate::state::AppEvent::PtyParsed { parsed, .. }
                if parsed.get("type").and_then(|v| v.as_str()) == Some("shell-state") =>
            {
                parsed
                    .get("state")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            }
            _ => None,
        })
        .collect();
    assert_eq!(states, vec!["idle", "busy"]);
}

#[test]
fn flush_keeps_pending_while_question_confident() {
    use std::collections::VecDeque;
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    use std::sync::atomic::AtomicU8;
    state
        .session_maps
        .shell_states
        .insert("sess".to_string(), AtomicU8::new(SHELL_BUSY));
    state.session_maps.session_states.insert(
        "sess".to_string(),
        crate::state::SessionState {
            spawn_root_role: crate::state::SpawnRootRole::DirectProgram,
            agent_type: Some("claude".to_string()),
            awaiting_input: true,
            question_confident: true,
            ..Default::default()
        },
    );
    let mut q = VecDeque::new();
    q.push_back(crate::state::PendingInjection::notice("later"));
    state.pending_injections.insert("sess".to_string(), q);

    state.session_maps.silence_states.insert(
        "sess".to_string(),
        Arc::new(Mutex::new(SilenceState::new())),
    );
    state
        .session_maps
        .silence_states
        .get("sess")
        .unwrap()
        .lock()
        .confirm_idle();

    try_shell_transition(&state, "sess", SHELL_BUSY, SHELL_IDLE, false);
    emit_shell_state(&state, "sess", "idle");
    flush_pending_injections(&state, "sess")
        .expect("pending flush starts a session worker")
        .join()
        .expect("session worker finishes before question clears");
    assert_eq!(
        state.pending_injections.get("sess").map(|q| q.len()),
        Some(1),
        "must not answer a confident user prompt — keep queued until it clears"
    );

    // The question clears (user answered) while the session is already idle:
    // the unblock flush must drain the queue with no further transition.
    state
        .session_maps
        .session_states
        .get_mut("sess")
        .unwrap()
        .question_confident = false;
    flush_one_pending_as_submitted(&state, "sess");
    assert_eq!(
        state.pending_injections.get("sess").map(|q| q.len()),
        Some(0),
        "unblock flush must submit once the confident question clears"
    );
}

#[cfg(unix)]
impl std::io::Write for TimedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.writes
            .lock()
            .unwrap()
            .push((std::time::Instant::now(), bytes.to_vec()));
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn state_change_to_parent_without_managed_pty_stays_inbox_only() {
    // Logical agent state alone is not proof of a managed PTY. A child state
    // change must remain available in the inbox without creating a phantom
    // terminal injection for an external peer.
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    agent_session(&state, "parent", SHELL_BUSY);
    state
        .session_maps
        .session_parent
        .insert("child".to_string(), "parent".to_string());

    push_state_change_to_parent(
        &state,
        "child",
        serde_json::json!({"type":"state_change","state":"idle","session_id":"child"}),
    );
    // The wake is dispatched on the injection worker; drain it, or "no terminal
    // input happened" would pass merely by asserting too early.
    wait_for_injection_queue();

    // Inbox got the JSON payload…
    assert_eq!(
        state.agent_inbox.get("parent").map(|q| q.len()),
        Some(1),
        "parent inbox must receive the state_change"
    );
    assert!(
        !state.pending_injections.contains_key("parent"),
        "an external peer without a managed PTY must not receive terminal input"
    );
    let message_id = state
        .agent_inbox
        .get("parent")
        .unwrap()
        .front()
        .unwrap()
        .id
        .clone();
    assert_eq!(state.agent_delivery_owner("parent", &message_id), None);
}

#[cfg(unix)]
fn run_progress_intent_case(
    chunks: &[&str],
    cols: u16,
    agent: bool,
    timer_idle: bool,
) -> (Vec<(String, Option<String>)>, Vec<String>) {
    run_progress_intent_case_ending(chunks, cols, agent, timer_idle, None, true, false)
}

#[cfg(unix)]
fn run_progress_intent_case_ending(
    chunks: &[&str],
    cols: u16,
    agent: bool,
    timer_idle: bool,
    teardown: Option<&str>,
    idle_is_quiet: bool,
    fail_journal: bool,
) -> (Vec<(String, Option<String>)>, Vec<String>) {
    run_progress_intent_case_grid(
        chunks,
        cols,
        agent,
        timer_idle,
        teardown,
        idle_is_quiet,
        fail_journal,
        2000,
        true,
    )
}

#[test]
fn protocol_idle_survives_ready_timer() {
    let mut silence = SilenceState::new();
    silence.note_explicit_state(SHELL_IDLE, true);
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(
        silence.explicit_idle(),
        "Ready must not downgrade a completion hook"
    );
    assert!(
        !silence
            .evidence
            .record_busy(EvidenceRank::Screen, "real-activity")
    );
    assert!(silence.explicit_idle());
}

/// The whole point of the adapter: goose can now recover to idle from the
/// screen, exactly as opencode does.
#[test]
fn goose_ready_screen_recovers_long_lived_shell_busy() {
    assert!(has_ready_screen_adapter(Some("goose")));

    let mut silence = SilenceState::new();
    silence.note_explicit_state(SHELL_BUSY, false);
    silence.note_real_activity();
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(!silence.explicit_busy());
    assert!(silence.idle_confirmed());
}

/// 744-138c baseline/after evidence: dump the exact `ParsedEvent` sequence
/// produced by replaying every committed `.tcap` fixture, one line per event.
/// Run before and after the SilenceState/decide() refactor and diff the two
/// captures — an empty diff is the "bit for bit" proof the story requires.
/// `#[ignore]` because it is a evidence-capture harness, not a pass/fail gate;
/// `replay_capture` never touches SilenceState (see its doc comment), so this
/// is expected to be stable across that refactor by construction.
#[test]
#[ignore = "744-138c evidence capture — run manually before/after the refactor"]
fn dump_committed_tcap_fixture_event_sequences_744() {
    let mut names: Vec<_> = std::fs::read_dir(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/agent_prompts"),
    )
    .expect("readable fixtures dir")
    .filter_map(|entry| {
        let path = entry.ok()?.path();
        (path.extension().and_then(|e| e.to_str()) == Some("tcap"))
            .then(|| path.file_name().unwrap().to_string_lossy().into_owned())
    })
    .collect();
    names.sort();
    for name in names {
        let bytes = agent_prompt_fixture(&name);
        let events = replay_capture(&bytes, false);
        println!("=== {name} ===");
        for event in &events {
            println!("{event:?}");
        }
    }
}

#[test]
fn codex_ready_prompt_remains_idle_and_deliverable() {
    let sid = "codex-ready-no-question";
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, sid, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("codex".into());
    state
        .session_maps
        .silence_states
        .get(sid)
        .unwrap()
        .lock()
        .confirm_idle();
    let rows = vec![
        "• Done. Updated Cargo.toml.".to_string(),
        "› Improve documentation in @filename".to_string(),
        "  gpt-5.5 high · ~/Gits/LS/agent2".to_string(),
    ];
    assert_eq!(
        detect_codex_screen_activity(&rows),
        AgentScreenActivity::Ready
    );
    assert!(
        !state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .awaiting_input
    );
    assert!(
        should_inject_now(&state, sid),
        "the idle composer must accept the next turn"
    );
}

/// Closing a tab must kill the agent grandchild, not just the shell.
///
/// Mirrors `claude` launched inside the PTY's shell: shell → grandchild,
/// both ignoring SIGINT/SIGTERM/SIGHUP so only the SIGKILL on the foreground
/// process group can reap them. Before the killpg fix, `close_pty_core`
/// SIGKILLed the shell alone and the grandchild was orphaned to init.
#[cfg(unix)]
#[test]
fn close_pty_core_kills_agent_grandchild() {
    use std::time::{Duration, Instant};

    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let pidfile = scratch.path().join("grandchild.pid");

    let pty = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");

    // Outer shell ignores the catchable signals and backgrounds a grandchild
    // that also ignores them, recording the grandchild PID for the probe.
    let script = format!(
        "trap '' INT TERM HUP; sh -c 'trap \"\" INT TERM HUP; sleep 30' & echo $! > {}; wait",
        pidfile.display()
    );
    let mut cmd = CommandBuilder::new("/bin/sh");
    cmd.args(["-c", &script]);
    let child = pty.slave.spawn_command(cmd).expect("spawn shell");

    let master = pty.master;
    let writer = master.take_writer().expect("writer");

    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-pgkill";
    state
        .metrics
        .active_sessions
        .fetch_add(1, Ordering::Relaxed);
    state.session_maps.sessions.insert(
        sid.to_string(),
        Mutex::new(PtySession {
            launch_receipt: None,
            writer: Arc::new(Mutex::new(writer)),
            master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            initial_cwd: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: false,
            shell: "/bin/sh".to_string(),
        }),
    );

    let pid_alive = |pid: libc::pid_t| unsafe { libc::kill(pid, 0) } == 0;
    let read_pid = || {
        std::fs::read_to_string(&pidfile)
            .ok()
            .and_then(|s| s.trim().parse::<libc::pid_t>().ok())
    };

    // Wait for the grandchild to come up and record its PID (up to ~3s).
    let mut grandchild = None;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if let Some(pid) = read_pid() {
            grandchild = Some(pid);
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let grandchild = grandchild.expect("grandchild PID should be written");
    assert!(
        pid_alive(grandchild),
        "grandchild should be alive before close"
    );

    close_pty_core(&state, sid, false);

    // killpg(SIGKILL) is untrappable: the grandchild must be gone shortly.
    let mut dead = false;
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if !pid_alive(grandchild) {
            dead = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        dead,
        "grandchild {grandchild} survived tab close — orphaned process tree"
    );
}

/// Populate the per-session maps that no teardown phase used to own, plus the
/// two the post-mortem read needs. Deliberately independent of the production
/// enumeration: a teardown test that reuses the list it verifies proves nothing.
fn populate_unowned_session_maps(state: &crate::state::AppState, sid: &str) {
    use std::sync::atomic::{AtomicBool, AtomicU64};
    state
        .session_maps
        .slash_mode
        .insert(sid.to_string(), AtomicBool::new(true));
    state
        .session_maps
        .last_input_ms
        .insert(sid.to_string(), AtomicU64::new(7));
    state
        .session_maps
        .has_osc133_integration
        .insert(sid.to_string(), ());
    state.agent_read_cursor.insert(sid.to_string(), 3);
    state
        .session_maps
        .marker_stats
        .insert(sid.to_string(), crate::state::MarkerStats::default());
    state.ai.session_knowledge.insert(
        sid.to_string(),
        Mutex::new(crate::ai_agent::knowledge::SessionKnowledge::new()),
    );
    state.session_maps.session_visibility.insert(
        sid.to_string(),
        std::collections::HashMap::from([("test-viewer".to_string(), 0u64)]),
    );
    state
        .session_maps
        .term_aliases
        .insert(sid.to_string(), "tc-9".to_string());
}

/// The swarm/identity maps. `tombstone_transient_cleanup` has always reaped
/// these; `cleanup_session` never did, which is the divergence F8 removes.
fn populate_swarm_session_maps(state: &crate::state::AppState, sid: &str, mcp_sid: &str) {
    state
        .session_maps
        .session_parent
        .insert(sid.to_string(), "parent-sess".to_string());
    state
        .session_maps
        .shell_state_since_ms
        .insert(sid.to_string(), std::sync::atomic::AtomicU64::new(42));
    state
        .mcp
        .to_session
        .insert(mcp_sid.to_string(), sid.to_string());
    state
        .mcp
        .session_to_mcp
        .insert(sid.to_string(), vec![mcp_sid.to_string()]);
    state.peer_agents.insert(
        sid.to_string(),
        crate::state::PeerAgent {
            tuic_session: sid.to_string(),
            mcp_session_id: mcp_sid.to_string(),
            name: "worker".to_string(),
            project: None,
            registered_at: 1,
        },
    );
    state.agent_inbox.entry(sid.to_string()).or_default();
    state.agent_inbox_evictions.insert(sid.to_string(), 2);
}

#[test]
fn closing_a_session_reaps_the_swarm_maps_too() {
    // The two teardowns were enumerated by hand and drifted: an explicit close
    // over HTTP DELETE goes straight to cleanup_session, which left every peer
    // identity, inbox and mcp mapping behind for the life of the process.
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "close-swarm";
    let mcp_sid = "mcp-close-swarm";
    populate_swarm_session_maps(&state, sid, mcp_sid);

    cleanup_session(sid, &state, "closed");

    assert!(!state.session_maps.session_parent.contains_key(sid));
    assert!(!state.session_maps.shell_state_since_ms.contains_key(sid));
    assert!(!state.mcp.to_session.contains_key(mcp_sid));
    assert!(!state.mcp.session_to_mcp.contains_key(sid));
    assert!(!state.peer_agents.contains_key(sid));
    assert!(!state.agent_inbox.contains_key(sid));
    assert!(!state.agent_inbox_evictions.contains_key(sid));
}

#[test]
fn closing_a_session_reaps_the_maps_no_phase_owned() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "close-unowned";
    populate_unowned_session_maps(&state, sid);

    cleanup_session(sid, &state, "closed");

    assert!(!state.session_maps.slash_mode.contains_key(sid));
    assert!(!state.session_maps.last_input_ms.contains_key(sid));
    assert!(!state.session_maps.has_osc133_integration.contains_key(sid));
    assert!(!state.agent_read_cursor.contains_key(sid));
    assert!(!state.session_maps.marker_stats.contains_key(sid));
    assert!(!state.session_maps.session_visibility.contains_key(sid));
    assert!(!state.session_maps.term_aliases.contains_key(sid));

    // Owned elsewhere, deliberately untouched — see the DEFERRED note on
    // remove_post_mortem_session_state. Knowledge is what the next session
    // inherits.
    assert!(state.ai.session_knowledge.contains_key(sid));
}

#[test]
fn a_tombstone_drops_live_process_state_and_keeps_the_post_mortem_maps() {
    // A tombstone is still readable for TOMBSTONE_TTL_MS, so the split is not
    // "reap everything": what the dead process owned goes, what a post-mortem
    // read needs stays.
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "tombstone-split";
    populate_unowned_session_maps(&state, sid);

    tombstone_transient_cleanup(sid, &state);

    assert!(
        !state.session_maps.slash_mode.contains_key(sid),
        "input mode belongs to the dead process"
    );
    assert!(!state.session_maps.last_input_ms.contains_key(sid));
    assert!(
        !state.session_maps.has_osc133_integration.contains_key(sid),
        "shell integration belongs to the dead shell"
    );
    assert!(!state.agent_read_cursor.contains_key(sid));

    assert!(
        state.session_maps.marker_stats.contains_key(sid),
        "marker tallies are exactly what a post-mortem question asks for"
    );
    assert!(
        state.ai.session_knowledge.contains_key(sid),
        "knowledge is flushed to disk by a 2s task — reaping it here loses it"
    );
    assert!(
        state.session_maps.term_aliases.contains_key(sid),
        "the tab still shows"
    );
    assert!(state.session_maps.session_visibility.contains_key(sid));
}

#[test]
fn reaping_a_tombstone_leaves_no_session_state_behind() {
    // The normal exit path is tombstone → sweeper, and cleanup_session is never
    // called on it. Anything the sweeper's list forgot therefore leaked for the
    // life of the process, not for TOMBSTONE_TTL_MS — which is what happened to
    // the terminal alias.
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "tombstone-reaped";
    populate_unowned_session_maps(&state, sid);
    populate_swarm_session_maps(&state, sid, "mcp-tombstone-reaped");

    tombstone_transient_cleanup(sid, &state);
    remove_post_mortem_session_state(sid, &state);

    assert!(!state.session_maps.term_aliases.contains_key(sid));
    assert!(!state.session_maps.marker_stats.contains_key(sid));
    assert!(!state.session_maps.session_visibility.contains_key(sid));
    assert!(!state.session_maps.last_output_ms.contains_key(sid));
    assert!(!state.session_maps.slash_mode.contains_key(sid));
    assert!(!state.peer_agents.contains_key(sid));
}

/// Stamp a tombstone that is already older than the TTL.
fn stamp_aged_tombstone(state: &crate::state::AppState, sid: &str, now_ms: u64) {
    state.session_maps.last_output_ms.insert(
        sid.to_string(),
        AtomicU64::new(now_ms - TOMBSTONE_TTL_MS - 1),
    );
}

#[test]
fn a_timestamp_left_without_buffers_is_still_reaped() {
    // An explicit DELETE runs the full cleanup, and the reader thread can then
    // reach EOF and re-stamp last_output_ms through the tombstone path. The
    // buffers are already gone, so a sweeper that discovers candidates by
    // walking output_buffers never sees that lone entry again.
    let state = crate::state::tests_support::make_test_app_state();
    let now_ms = 10 * TOMBSTONE_TTL_MS;
    stamp_aged_tombstone(&state, "orphan-stamp", now_ms);

    assert_eq!(
        aged_out_tombstones(&state, now_ms),
        vec!["orphan-stamp".to_string()],
        "a stamp with no buffers is still session state to reap"
    );
}

#[cfg(unix)]
#[test]
fn a_session_id_reused_before_the_sweep_is_not_reaped() {
    // The HTTP spawn path accepts a caller-supplied id, so an aged tombstone's
    // id can come back to life between candidate selection and removal. Reaping
    // it then deletes the LIVE session's buffers, alias and visibility.
    let state = crate::state::tests_support::make_test_app_state();
    let now_ms = 10 * TOMBSTONE_TTL_MS;
    let sid = "reused-id";
    stamp_aged_tombstone(&state, sid, now_ms);
    state
        .session_maps
        .term_aliases
        .insert(sid.to_string(), "tc-1".to_string());
    let candidates = aged_out_tombstones(&state, now_ms);
    assert_eq!(candidates, vec![sid.to_string()]);

    // The race: a client recreates the id after selection, before removal.
    crate::state::tests_support::insert_dummy_session(&state, sid);
    reap_tombstones(&state, &candidates);

    assert!(
        state.session_maps.term_aliases.contains_key(sid),
        "the live session that reclaimed this id must keep its state"
    );
}

#[cfg(unix)]
#[test]
fn cleanup_session_removes_session_and_decrements_metrics() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "cleanup-real";
    spawn_short_session(&state, sid);
    let before = state.metrics.active_sessions.load(Ordering::Relaxed);
    assert!(state.session_maps.sessions.contains_key(sid));

    cleanup_session(sid, &state, "closed");

    assert!(
        !state.session_maps.sessions.contains_key(sid),
        "the live session entry must be removed"
    );
    assert_eq!(
        state.metrics.active_sessions.load(Ordering::Relaxed),
        before - 1,
        "removing a live session must decrement the active-session gauge"
    );
}

/// Regression test for the bug fixed 2026-09-29: `cleanup_session` used to
/// remove the session with a bare `.remove(session_id).is_some()`, dropping
/// the returned `Mutex<PtySession>` (and its `Box<dyn portable_pty::Child>`,
/// which on Unix wraps a plain `std::process::Child`) with no `kill()` and no
/// wait — `std::process::Child` does NOT terminate its process on drop, so
/// the real OS process leaked for as long as it happened to keep running on
/// its own. `sleep 5` proves this isn't "the process happened to exit before
/// we checked": 5s comfortably outlasts `terminate_child_with_grace`'s
/// ~100-200ms grace-then-SIGKILL window, so a passing test here means the
/// SIGKILL fallback path actually fired, not that the process exited by
/// coincidence.
#[cfg(unix)]
#[test]
fn cleanup_session_actually_kills_a_process_that_ignores_everything_but_sigkill() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "cleanup-must-kill";
    spawn_short_session(&state, sid);
    let pid = state
        .session_maps
        .sessions
        .get(sid)
        .unwrap()
        .lock()
        ._child
        .process_id()
        .expect("a freshly spawned child has a pid");

    cleanup_session(sid, &state, "closed");

    // `kill -0` reports whether the process exists at all, without sending a
    // real signal — the standard "is this pid still alive" check.
    let still_alive = std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .expect("kill -0 runs")
        .success();
    assert!(
        !still_alive,
        "cleanup_session must actually terminate the child process (pid {pid}), \
         not just remove it from the sessions map and drop the handle"
    );
}

/// Insert a minimal real PTY session (short-lived `sleep`) so functions that
/// require a live `PtySession` can be exercised. Mirrors `create_pty`'s
/// active-session bookkeeping.
#[cfg(unix)]
fn spawn_short_session(state: &crate::state::AppState, sid: &str) {
    let pty = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    let mut cmd = CommandBuilder::new("/bin/sh");
    cmd.args(["-c", "sleep 5"]);
    let child = pty.slave.spawn_command(cmd).expect("spawn");
    let master = pty.master;
    let writer = master.take_writer().expect("writer");
    state
        .metrics
        .active_sessions
        .fetch_add(1, Ordering::Relaxed);
    state.session_maps.sessions.insert(
        sid.to_string(),
        Mutex::new(PtySession {
            launch_receipt: None,
            writer: Arc::new(Mutex::new(writer)),
            master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            initial_cwd: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: false,
            shell: "/bin/sh".to_string(),
        }),
    );
}

/// End the 120 s startup grace so grace-suppression of low-confidence
/// questions, rate limits and API errors does not mask what the chrome
/// cutoff decided. Mirrors `test_startup_grace_safety_cap`.
fn settle_startup_grace(silence: &Arc<Mutex<SilenceState>>) {
    let mut sl = silence.lock();
    sl.created_at =
        std::time::Instant::now() - STARTUP_GRACE_MAX - std::time::Duration::from_secs(1);
    sl.last_output_at = std::time::Instant::now();
    sl.check_startup_settle();
    assert!(!sl.is_startup_grace(), "startup grace must be settled");
}

fn shell_of(state: &AppState, sid: &str) -> u8 {
    state
        .session_maps
        .shell_states
        .get(sid)
        .unwrap()
        .load(Ordering::Acquire)
}

/// Catches: `finish_session_tasks` dropping the `error` message of a failed exit
/// or the `result` payload of a clean one, so a polled task shows no outcome.
#[test]
fn session_exit_records_task_error_or_result_from_the_exit_code() {
    use crate::tasks::{TaskKind, TaskStatus};
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let cases: [(&str, Option<i32>); 3] =
        [("failed", Some(3)), ("clean", Some(0)), ("unknown", None)];
    for (sid, code) in cases {
        let task = state
            .tasks
            .create(TaskKind::AgentSpawn, "orchestrator", Some(sid));
        if let Some(code) = code {
            state.session_maps.exit_codes.insert(sid.to_string(), code);
        }
        mark_session_exited(sid, &state);
        let rec = state.tasks.get(&task).expect("task");
        if code.is_some_and(|c| c != 0) {
            assert_eq!(rec.status, TaskStatus::Failed, "{sid}");
            assert_eq!(
                rec.error.as_deref(),
                Some("agent session exited with code 3")
            );
            assert_eq!(rec.result, None);
        } else {
            assert_eq!(rec.status, TaskStatus::Completed, "{sid}");
            assert_eq!(
                rec.result,
                Some(serde_json::json!({ "session_id": sid, "exit_code": code })),
                "{sid}"
            );
            assert_eq!(rec.error, None);
        }
    }
}

#[cfg(unix)]
impl std::io::Write for WorkingOnEnterWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.state
            .grid
            .vt_log_buffers
            .get(self.sid)
            .unwrap()
            .lock()
            .process(
                b"\x1b[2J\x1b[H\xe2\x80\xa2 Working (2s \xe2\x80\xa2 esc to interrupt)\r\n\r\n\xe2\x80\xba run the next step please\r\n\r\n  gpt-5 \xc2\xb7 ~/repo",
            );
        self.state
            .session_maps
            .output_buffers
            .get(self.sid)
            .unwrap()
            .lock()
            .write(b"working");
        match self.fault {
            RetryWriterFault::Write => Err(std::io::Error::other("write failed")),
            _ => Ok(bytes.len()),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self.fault {
            RetryWriterFault::Flush => Err(std::io::Error::other("flush failed")),
            _ => Ok(()),
        }
    }
}
