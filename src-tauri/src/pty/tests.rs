use super::*;

fn fixture_rows(fixture: &str) -> Vec<String> {
    fixture
        .trim_end_matches('\n')
        .lines()
        .map(str::to_string)
        .collect()
}

/// The interactive-path threads raise their QoS to USER_INTERACTIVE. Verify
/// the syscall actually takes effect by reading the class back on the same
/// thread (default QoS for a fresh test thread is *not* USER_INTERACTIVE).
#[cfg(target_os = "macos")]
#[test]
fn raises_thread_to_user_interactive_qos() {
    // Run on a dedicated thread so we don't leave the test runner's worker
    // permanently bumped.
    let observed = std::thread::spawn(|| {
        raise_thread_for_interactive_io();
        thread_qos::current_qos_class()
    })
    .join()
    .expect("qos probe thread panicked");
    // QOS_CLASS_USER_INTERACTIVE == 0x21.
    assert_eq!(
        observed, 0x21,
        "thread QoS was not raised to USER_INTERACTIVE"
    );
}

/// A keystroke borrows a thread from the shared tokio blocking pool and gives
/// it back. Bumping that thread's QoS without putting it back promotes the
/// pool itself: the next git walk, content-index build, or config write to
/// land on that thread runs in the interactive band forever after — the one
/// band the keystroke path needs kept clear.
#[cfg(target_os = "macos")]
#[test]
fn a_keystroke_gives_the_shared_blocking_thread_back_at_its_original_qos() {
    // The whole pair, not just the class: restoring the band while dropping
    // the relative priority is still handing back a thread that is not the
    // one we borrowed, and a class-only assertion cannot see it.
    let (before, during, after) = std::thread::spawn(|| {
        let before = thread_qos::current_qos_pair();
        let during = {
            let _boost = interactive_io_boost();
            thread_qos::current_qos_pair()
        };
        (before, during, thread_qos::current_qos_pair())
    })
    .join()
    .expect("qos probe thread panicked");
    // QOS_CLASS_USER_INTERACTIVE == 0x21.
    assert_eq!(
        during.0, 0x21,
        "keystroke did not run in the interactive band"
    );
    assert_ne!(before.0, 0x21, "probe thread started already bumped");
    assert_eq!(
        after, before,
        "the blocking-pool thread stayed promoted after the keystroke"
    );
}

/// The restore is a `Drop`, so the path that matters most is the one nobody
/// writes on purpose: a panic inside the write. `spawn_blocking` catches it
/// and returns the thread to the pool either way, so a boost that only
/// unwound on the happy path would promote the pool exactly when something
/// is already going wrong.
#[cfg(target_os = "macos")]
#[test]
fn a_panicking_keystroke_still_gives_the_thread_back_at_its_original_qos() {
    let (before, after) = std::thread::spawn(|| {
        let before = thread_qos::current_qos_pair();
        let panicked = std::panic::catch_unwind(|| {
            let _boost = interactive_io_boost();
            panic!("write failed mid-keystroke");
        });
        assert!(panicked.is_err(), "the probe did not actually panic");
        (before, thread_qos::current_qos_pair())
    })
    .join()
    .expect("qos probe thread panicked");
    assert_ne!(before.0, 0x21, "probe thread started already bumped");
    assert_eq!(
        after, before,
        "a panicking keystroke left the blocking-pool thread promoted"
    );
}

/// The reader thread runs its whole body inside `catch_unwind`, and the
/// `running.store(false)` that stops the frame ticker and the 1 Hz silence
/// timer sits INSIDE that closure. A panic skips it: the ticker keeps waking
/// ~62 times a second and the tokio timer keeps ticking for the life of the
/// process, and the ticker never reaches the code after its loop that removes
/// the session's `grid_frame_dirty` / `sync_update_active` entries.
///
/// Liveness is read off the two things each loop owns: the ticker removes its
/// map entries only after the loop ends, and the silence timer holds a clone
/// of the `SilenceState` Arc that teardown drops from `silence_states`.
#[tokio::test(flavor = "current_thread", start_paused = false)]
async fn a_reader_panic_stops_the_ticker_and_the_silence_timer() {
    const PAYLOAD: &str = "simulated PTY reader panic";

    struct PanicOnRead(Arc<AtomicBool>);
    impl Read for PanicOnRead {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            while !self.0.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            panic!("{PAYLOAD}");
        }
    }

    // The injected panic is the fixture, not a failure. Swallow that one
    // payload so the suite prints no stray backtrace, record that it fired so
    // the test still proves the panic path ran, and delegate anything else.
    let observed = Arc::new(AtomicBool::new(false));
    let seen = observed.clone();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if info
            .payload()
            .downcast_ref::<String>()
            .is_some_and(|s| s == PAYLOAD)
        {
            seen.store(true, Ordering::Relaxed);
        } else {
            previous(info);
        }
    }));

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "reader-panic-session".to_string();
    let detonate = Arc::new(AtomicBool::new(false));
    spawn_reader_thread(
        Box::new(PanicOnRead(detonate.clone())),
        Arc::new(AtomicBool::new(false)),
        sid.clone(),
        state.clone(),
        None,
    );

    // Take the handle BEFORE the panic: teardown removes the map entry, so
    // afterwards the only clones left are this one and the timer's.
    let silence = state
        .silence_states
        .get(&sid)
        .map(|e| Arc::clone(e.value()))
        .expect("silence state is registered before the threads start");
    assert!(
        state.grid_frame_dirty.contains_key(&sid) && state.sync_update_active.contains_key(&sid),
        "precondition: the ticker owns both per-session entries"
    );

    detonate.store(true, Ordering::Relaxed);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let ticker_alive = state.grid_frame_dirty.contains_key(&sid)
            || state.sync_update_active.contains_key(&sid);
        let timer_alive = Arc::strong_count(&silence) > 1;
        if !ticker_alive && !timer_alive {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "reader panic leaked: ticker still live={ticker_alive}, \
                 1 Hz timer still live={timer_alive} ({} silence refs)",
            Arc::strong_count(&silence)
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    assert!(
        observed.load(Ordering::Relaxed),
        "the injected reader panic never fired — the test proved nothing"
    );
    let _ = std::panic::take_hook();
}

#[test]
fn grid_send_min_interval_policy() {
    // Short burst, no typing → no floor: full-speed for low latency.
    assert_eq!(grid_send_min_interval_ms(false, 0), 0);
    assert_eq!(grid_send_min_interval_ms(false, 5), 0);
    // Sustained animation (dirty ≥ 6 ticks), no typing → ~30 fps floor.
    assert_eq!(grid_send_min_interval_ms(false, 6), 33);
    assert_eq!(grid_send_min_interval_ms(false, 1000), 33);
    // Typing under load → ~20 fps floor, regardless of dirty_run (even a
    // short burst), because keystroke latency is what we protect.
    assert_eq!(grid_send_min_interval_ms(true, 0), 50);
    assert_eq!(grid_send_min_interval_ms(true, 1000), 50);
    // Typing floor must be the more aggressive (larger interval) of the two.
    assert!(grid_send_min_interval_ms(true, 1000) > grid_send_min_interval_ms(false, 1000));
}

#[test]
fn system_load_per_core_is_non_negative_and_finite() {
    // Links libc getloadavg/sysconf on Unix; returns 0.0 elsewhere. Either
    // way it must be a sane, non-negative, finite ratio.
    let v = system_load_per_core();
    assert!(v.is_finite());
    assert!(v >= 0.0);
}

#[test]
fn clean_action_required_title_strips_marker_spinner_and_separators() {
    // Real grok permission-prompt title (captured live).
    assert_eq!(
        clean_action_required_title(
            "⚠ Action Required - ⠙ - Running: echo hello - Execute Shell Command"
        ),
        "Running: echo hello - Execute Shell Command"
    );
}

#[test]
fn clean_action_required_title_handles_each_spinner_frame() {
    // Title repaints with a different braille frame each tick; cleaned output is stable.
    for frame in ["⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇"] {
        assert_eq!(
            clean_action_required_title(&format!("⚠ Action Required - {frame} - Running: ls")),
            "Running: ls"
        );
    }
}

#[test]
fn clean_action_required_title_fallback_when_empty() {
    assert_eq!(
        clean_action_required_title("⚠ Action Required - ⠙ - "),
        "grok is awaiting approval"
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

#[test]
fn test_parse_osc133_exit_code() {
    assert_eq!(parse_osc133_exit_code('D', "0"), Some(0));
    assert_eq!(parse_osc133_exit_code('D', "127"), Some(127));
    assert_eq!(parse_osc133_exit_code('D', ""), None);
    assert_eq!(parse_osc133_exit_code('A', "0"), None);
}

#[test]
fn test_classify_agent_claude() {
    assert_eq!(classify_agent("claude"), Some("claude"));
}

#[test]
fn test_classify_agent_gemini() {
    assert_eq!(classify_agent("gemini"), Some("gemini"));
}

#[test]
fn test_classify_agent_aider() {
    assert_eq!(classify_agent("aider"), Some("aider"));
}

#[test]
fn test_classify_agent_codex() {
    assert_eq!(classify_agent("codex"), Some("codex"));
}

#[test]
fn test_classify_agent_opencode() {
    assert_eq!(classify_agent("opencode"), Some("opencode"));
}

#[test]
fn test_classify_agent_goose() {
    assert_eq!(classify_agent("goose"), Some("goose"));
}

#[test]
fn test_classify_agent_droid() {
    assert_eq!(classify_agent("droid"), Some("droid"));
}

#[test]
fn test_classify_agent_unknown_returns_none() {
    assert_eq!(classify_agent("bash"), None);
    assert_eq!(classify_agent("zsh"), None);
    assert_eq!(classify_agent("node"), None);
    assert_eq!(classify_agent("python"), None);
    assert_eq!(classify_agent("vim"), None);
}

/// grok 1.0.5 ships `~/.grok/bin/grok` as a symlink to `grok-1.0.5`, and
/// `proc_pidpath` resolves the link. Missing the versioned basename left the
/// session with no `agent_type`, hence no ready-screen adapter, hence a tab
/// that never left BUSY.
#[test]
fn test_classify_agent_versioned_basename() {
    assert_eq!(classify_agent("grok-1.0.5"), Some("grok"));
    assert_eq!(classify_agent("claude-2.1.81"), Some("claude"));
    assert_eq!(classify_agent("codex-0.116"), Some("codex"));
    assert_eq!(
        classify_agent_name_or_path("/Users/me/.grok/bin/grok-1.0.5"),
        Some("grok")
    );
}

/// The suffix must start with a digit, so a hyphenated tool name keeps its
/// own identity and an unrelated binary is not promoted to an agent.
#[test]
fn test_classify_agent_version_strip_does_not_overreach() {
    assert_eq!(classify_agent("cursor-agent"), Some("cursor"));
    assert_eq!(classify_agent("grok-wrapper"), None);
    assert_eq!(classify_agent("not-grok"), None);
    assert_eq!(classify_agent("postgres-16"), None);
}

/// The ready-screen adapter is the whole point of detecting the agent: grok
/// runs as one long-lived foreground command, so OSC 133 marks the shell busy
/// once and only the screen can take it back to idle.
#[test]
fn test_grok_minimal_screen_is_ready_once_classified() {
    assert!(has_ready_screen_adapter(classify_agent("grok-1.0.5")));
    // Captured live from `grok --minimal` 1.0.5: no composer box, no
    // separators — a bare prompt glyph above the model status row.
    let rows: Vec<String> = vec![
        "Abbiamo scritto l’analisi completa in `grok-report.md`.".to_string(),
        "minimal · /help".to_string(),
        "\u{276F}".to_string(),
        "Grok 4.6 (high) · always-approve · 186K / 500K (37%) · ctrl+o transcript".to_string(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("grok"), &rows),
        AgentScreenActivity::Ready
    );
}

// --- parse_osc7_cwd tests (story 1558-81bb) ---

#[test]
fn osc7_simple_path() {
    assert_eq!(
        parse_osc7_cwd("file://hostname/Users/me"),
        Ok("/Users/me".into())
    );
}

#[test]
fn osc7_empty_hostname() {
    assert_eq!(parse_osc7_cwd("file:///home/user"), Ok("/home/user".into()));
}

#[test]
fn osc7_localhost() {
    assert_eq!(
        parse_osc7_cwd("file://localhost/tmp/foo"),
        Ok("/tmp/foo".into())
    );
}

#[test]
fn osc7_trailing_slash_stripped() {
    assert_eq!(
        parse_osc7_cwd("file:///home/user/"),
        Ok("/home/user".into())
    );
}

#[test]
fn osc7_root_path_preserved() {
    assert_eq!(parse_osc7_cwd("file:///"), Ok("/".into()));
}

#[test]
fn osc7_percent_encoded_space() {
    assert_eq!(
        parse_osc7_cwd("file:///home/user/my%20project"),
        Ok("/home/user/my project".into()),
    );
}

#[test]
fn osc7_percent_encoded_special_chars() {
    assert_eq!(
        parse_osc7_cwd("file:///tmp/%E2%9C%93"),
        Ok("/tmp/\u{2713}".into()),
    );
}

#[test]
fn osc7_missing_scheme() {
    assert!(parse_osc7_cwd("/home/user").is_err());
}

#[test]
fn osc7_wrong_scheme() {
    assert!(parse_osc7_cwd("http://localhost/foo").is_err());
}

#[test]
fn osc7_invalid_percent_encoding() {
    assert!(parse_osc7_cwd("file:///home/%GG").is_err());
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

// --- SilenceState tests ---

#[test]
fn test_silence_state_no_pending_returns_none() {
    let mut s = SilenceState::new();
    assert!(s.check_silence().is_none());
}

#[test]
fn test_tool_error_no_candidate_returns_none() {
    let mut s = SilenceState::new();
    assert!(s.check_tool_error().is_none());
}

#[test]
fn test_tool_error_fires_after_silence_threshold() {
    let mut s = SilenceState::new();
    s.mark_tool_error_candidate("Error: Exit code 128".to_string());
    // Force last_output_at past the threshold to simulate silence.
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_tool_error(),
        Some("Error: Exit code 128".to_string())
    );
    // Dedup: second call returns None (already emitted).
    assert!(s.check_tool_error().is_none());
}

#[test]
fn test_tool_error_recovery_clears_candidate() {
    let mut s = SilenceState::new();
    s.mark_tool_error_candidate("Error: Exit code 1".to_string());
    s.clear_tool_error_on_recovery();
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(
        s.check_tool_error().is_none(),
        "recovery must clear pending tool error"
    );
}

#[test]
fn test_tool_error_does_not_refire_same_line_after_recovery() {
    // Reproduces the scroll-induced re-fire bug: once an error has been
    // surfaced, scrolling the Ink TUI viewport re-introduces the error line
    // in `changed_rows`. `clear_tool_error_on_recovery` must NOT re-enable
    // notification for a line the user already saw.
    let mut s = SilenceState::new();
    s.mark_tool_error_candidate("Error: Exit code 1".to_string());
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_tool_error(),
        Some("Error: Exit code 1".to_string()),
        "first occurrence must fire"
    );

    // Agent produced real output → recovery.
    s.clear_tool_error_on_recovery();

    // Viewport scrolls, same error line reappears in changed_rows.
    s.mark_tool_error_candidate("Error: Exit code 1".to_string());
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(
        s.check_tool_error().is_none(),
        "same error line must not refire after recovery (scroll-induced)"
    );
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
fn test_tool_error_refires_after_memory_reset() {
    // After the user submits a line (explicit re-engagement), a recurrence
    // of the same failure in a new turn must notify again.
    let mut s = SilenceState::new();
    s.mark_tool_error_candidate("Error: Exit code 1".to_string());
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(s.check_tool_error().is_some());

    s.reset_tool_error_memory();

    s.mark_tool_error_candidate("Error: Exit code 1".to_string());
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_tool_error(),
        Some("Error: Exit code 1".to_string()),
        "after user input, same error text must be allowed to notify again"
    );
}

#[test]
fn test_is_retry_line_matches_connection_retries() {
    // Claude subagent SDK retry loop (the reported false-idle scenario).
    assert!(is_retry_line(
        "  Unable to connect to API (ECONNRESET) · Retrying in 0s · attempt 6/10"
    ));
    assert!(is_retry_line(
        "Teammate @spinach-mail-validate failed: API Error: Unable to connect to API (ConnectionRefused)"
    ));
    // Goose/Aider stream-error retry form.
    assert!(is_retry_line(
        "⚠  stream error: exceeded retry limit, last status: 401; retrying 5/5 in 3s…"
    ));
    // Non-retry prose / code must NOT latch busy — the N/M counter is required.
    assert!(!is_retry_line(
        "I'll be retrying the request in a moment if it fails."
    ));
    assert!(!is_retry_line(
        "let retrying = true; // attempt to reconnect"
    ));
    assert!(!is_retry_line(
        "Successfully connected to the API endpoint."
    ));
}

#[test]
fn test_api_retry_hold_active_then_expires() {
    let mut s = SilenceState::new();
    assert!(!s.is_api_retry_active(), "no hold armed initially");
    s.mark_api_retry();
    assert!(s.is_api_retry_active(), "hold active right after arming");
    // Simulate the hold window elapsing.
    s.api_retry_hold_until = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));
    assert!(
        !s.is_api_retry_active(),
        "hold self-expires after AGENT_RETRY_HOLD"
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
    s.ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM * 2);
    assert!(
        !s.note_ready_screen(),
        "ready screen must not confirm idle while an API retry is in flight"
    );

    // Once the hold expires, the same stable ready prompt confirms idle.
    s.api_retry_hold_until = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));
    s.ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM * 2);
    assert!(
        s.note_ready_screen(),
        "ready screen confirms idle after the retry hold expires"
    );
}

#[test]
fn test_api_retry_hold_cleared_on_recovery_and_user_input() {
    let mut s = SilenceState::new();
    s.mark_api_retry();
    // Real non-error output → agent recovered.
    s.clear_tool_error_on_recovery();
    assert!(!s.is_api_retry_active(), "recovery releases the retry hold");

    s.mark_api_retry();
    // User re-engages (submitted a line / Ctrl+C).
    s.reset_tool_error_memory();
    assert!(
        !s.is_api_retry_active(),
        "user input releases the retry hold"
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

// --- is_tool_error_line tests ---

#[test]
fn test_tool_error_matches_claude_code_format() {
    // Claude Code prefixes tool-result rows with `⎿ `.
    assert!(is_tool_error_line("⎿  Error: Exit code 1"));
    assert!(is_tool_error_line("  ⎿  Error: Exit code 127"));
}

#[test]
fn test_tool_error_matches_bare_format() {
    assert!(is_tool_error_line("Error: Exit code 1"));
    assert!(is_tool_error_line("  Error: Exit code 128"));
}

#[test]
fn test_tool_error_rejects_source_code_literal() {
    // Exact string that triggered the false-positive in Boss's session:
    // the test file's own content displayed in a terminal armed a red
    // notification because the unanchored regex matched inside a string
    // literal. These must never fire.
    assert!(!is_tool_error_line(
        r#"s.mark_tool_error_candidate("Error: Exit code 2".to_string());"#
    ));
    assert!(!is_tool_error_line(
        r#"assert_eq!(s.check_tool_error(), Some("Error: Exit code 1".to_string()));"#
    ));
    assert!(!is_tool_error_line(
        r#"3895          s.mark_tool_error_candidate("Error: Exit code 2".to_string"#
    ));
}

#[test]
fn test_tool_error_rejects_markdown_mention() {
    // Commit messages, docs, release notes that quote the error text.
    assert!(!is_tool_error_line(
        r#"fix: resolve "Error: Exit code 1" in claude tool pipeline"#
    ));
}

#[test]
fn test_tool_error_allows_box_drawing_variations() {
    // Other box-drawing chars Claude uses for tool-call hierarchy rows.
    assert!(is_tool_error_line("╰  Error: Exit code 2"));
    assert!(is_tool_error_line("│  Error: Exit code 5"));
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
fn test_suggest_drain_consumes_items() {
    let mut s = SilenceState::new();
    s.mark_suggest_candidate(vec!["a".to_string()], 0);
    let _ = s.drain_pending_suggest();
    assert!(
        s.drain_pending_suggest().is_none(),
        "second drain must return None — single-shot semantics"
    );
}

#[test]
fn test_suggest_drain_none_when_nothing_parked() {
    let mut s = SilenceState::new();
    assert!(s.drain_pending_suggest().is_none());
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
fn test_suggest_reset_on_user_input() {
    let mut s = SilenceState::new();
    s.mark_suggest_candidate(vec!["stale".to_string()], 0);
    s.reset_suggest_memory();
    assert!(
        s.drain_pending_suggest().is_none(),
        "user input must drop pending suggest so it doesn't fire across turns"
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
fn test_silence_state_pending_but_too_soon() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    // Just set — not enough time has passed
    assert!(s.check_silence().is_none());
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
fn test_silence_state_no_double_emission() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(s.check_silence().is_some());
    // Second check should return None (already emitted)
    assert!(s.check_silence().is_none());
}

#[test]
fn test_silence_state_regex_suppresses_timer() {
    let mut s = SilenceState::new();
    // regex_found_question = true means instant detection already fired
    s.on_chunk(
        true,
        Some("Would you like to proceed?".to_string()),
        false,
        false,
        false,
    );
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(s.check_silence().is_none());
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
fn test_silence_state_suppress_user_input() {
    let mut s = SilenceState::new();
    // User types a line ending with `?` — PTY will echo it back
    s.on_chunk(
        false,
        Some("c'è ancora una storia?".to_string()),
        false,
        false,
        false,
    );
    // write_pty detects user input and suppresses
    s.suppress_user_input();
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Should NOT fire — the question was typed by the user
    assert!(s.check_silence().is_none());
}

#[test]
fn test_silence_state_suppress_echo_after_user_input() {
    let mut s = SilenceState::new();
    // write_pty detects user input and suppresses BEFORE the echo arrives
    s.suppress_user_input();
    // PTY echoes the user's text back — this should NOT re-enable detection
    s.on_chunk(
        false,
        Some("lo hai mai provato?".to_string()),
        false,
        false,
        false,
    );
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Should NOT fire — the echo window blocks re-enabling
    assert!(
        s.check_silence().is_none(),
        "PTY echo after suppress should not trigger question detection"
    );
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

// --- is_chrome_row / chrome_only classification tests ---

#[test]
fn test_chrome_only_empty_changed_rows_is_chrome() {
    // Empty changed_rows means the chunk produced no visible change
    // (cursor blink, OSC title update, mouse report). It must count as
    // chrome-only so periodic re-emits don't latch the shell to busy.
    let rows: Vec<ChangedRow> = vec![];
    assert!(
        compute_chrome_only(&rows, false, false, false),
        "empty changed_rows should be chrome_only (no real output)"
    );
}

#[test]
fn test_chrome_only_plain_text_is_not_chrome() {
    let rows = make_rows(&["I will edit the file for you."]);
    let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
    assert!(
        !chrome_only,
        "plain text without chrome markers is not chrome"
    );
}

#[test]
fn test_chrome_only_statusline_with_text_rows_is_not_chrome() {
    let rows = make_rows(&[
        "\u{23F5}\u{23F5} auto mode",
        "Here is the code change:",
        "  fn main() {",
        "    println!(\"hello\");",
    ]);
    let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
    assert!(!chrome_only, "mode-line + text rows should not be chrome");
}

#[test]
fn test_chrome_only_single_statusline_row_is_chrome() {
    let rows = make_rows(&["\u{23F5}\u{23F5} auto mode"]);
    let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
    assert!(chrome_only, "single mode-line row should be chrome");
}

#[test]
fn test_chrome_only_wrapped_statusline_is_chrome() {
    let rows = make_rows(&[
        "\u{23F5}\u{23F5} bypass permissions on",
        "\u{273B} Cogitated 3m 47s",
    ]);
    let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
    assert!(chrome_only, "wrapped mode-line rows should all be chrome");
}

#[test]
fn test_chrome_only_subtasks_row_is_chrome() {
    let rows = make_rows(&["\u{203A}\u{203A} bypass permissions on \u{00B7} 1 local agent"]);
    let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
    assert!(chrome_only, "subtask mode-line row should be chrome");
}

#[test]
fn test_chrome_only_codex_spinner_is_chrome() {
    let rows = make_rows(&["\u{2022} Boot"]);
    let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
    assert!(chrome_only, "Codex spinner row should be chrome");
}

#[test]
fn test_chrome_only_gemini_braille_spinner_is_chrome() {
    // Gemini braille spinner chars (U+2800-28FF) are now in is_chrome_row
    let rows = make_rows(&["\u{280B} Connecting to MCP servers..."]);
    let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
    assert!(chrome_only, "Gemini braille spinner should be chrome");
}

#[test]
fn test_chrome_only_tool_progress_spinner_is_chrome() {
    let rows = make_rows(&["\u{25D0} Bash: .../b... | \u{2713} Bash \u{00D7}9"]);
    let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
    assert!(chrome_only, "CC tool progress spinner should be chrome");
    assert!(
        crate::chrome::is_spinner_row(&rows[0].text),
        "CC tool progress spinner should be detected as spinner (keepalive)"
    );
}

// --- chrome_only full formula tests (mirrors process_chunk logic) ---

/// Helper: compute chrome_only using the same formula as process_chunk.
fn compute_chrome_only(
    rows: &[ChangedRow],
    has_status_line: bool,
    regex_found_question: bool,
    last_q_line: bool,
) -> bool {
    let all_chrome_markers = rows.iter().all(|r| is_chrome_row(&r.text));
    let no_real_output = rows.iter().all(|r| {
        is_chrome_row(&r.text)
            || r.text.trim().is_empty()
            || crate::chrome::is_separator_line(&r.text)
            || crate::chrome::is_prompt_line(&r.text)
    });
    !regex_found_question
        && !last_q_line
        && (rows.is_empty() || all_chrome_markers || (has_status_line && no_real_output))
}

#[test]
fn test_chrome_only_formula_timer_tick_only() {
    // CC timer tick: only the timer row changed
    let rows = make_rows(&["\u{273B} Cogitated 3m 47s"]);
    assert!(
        compute_chrome_only(&rows, true, false, false),
        "timer-only tick should be chrome_only"
    );
}

#[test]
fn test_chrome_only_formula_timer_plus_separator() {
    // CC timer tick + separator repaint (ESC[2J full redraw)
    let rows = make_rows(&[
        "────────────────────────────────────",
        "\u{273B} Cogitated 3m 48s",
    ]);
    assert!(
        compute_chrome_only(&rows, true, false, false),
        "timer + separator should be chrome_only"
    );
}

#[test]
fn test_chrome_only_formula_timer_plus_prompt_and_separator() {
    // CC timer tick + prompt + separator (full bottom chrome zone)
    let rows = make_rows(&[
        "────────────────────────────────────",
        "❯",
        "────────────────────────────────────",
        "\u{23F5}\u{23F5} auto mode",
        "\u{273B} Cogitated 3m 48s",
    ]);
    assert!(
        compute_chrome_only(&rows, true, false, false),
        "timer + prompt + separator + mode-line should be chrome_only"
    );
}

#[test]
fn test_chrome_only_formula_timer_plus_blank_rows() {
    // CC timer tick with blank rows (padding in TUI)
    let rows = make_rows(&["", "\u{273B} Cogitated 3m 48s", ""]);
    assert!(
        compute_chrome_only(&rows, true, false, false),
        "timer + blank rows should be chrome_only"
    );
}

#[test]
fn test_chrome_only_formula_real_output_not_chrome() {
    // Real agent output mixed with status line
    let rows = make_rows(&["I will edit the file for you.", "\u{273B} Cogitated 3m 48s"]);
    assert!(
        !compute_chrome_only(&rows, true, false, false),
        "real text + timer should NOT be chrome_only"
    );
}

#[test]
fn test_chrome_only_formula_question_line_not_chrome() {
    // Even if all chrome, a pending question line disables chrome_only
    let rows = make_rows(&["\u{273B} Cogitated 3m 48s"]);
    assert!(
        !compute_chrome_only(&rows, true, false, true),
        "chrome with pending question should NOT be chrome_only"
    );
}

// --- Spinner → busy gate tests (mirrors process_chunk transition logic) ---

/// The busy transition gate `(!chrome_only || has_spinner)` must be true
/// when changed_rows contain an active spinner, even when chrome_only is true.
#[test]
fn test_spinner_only_chunk_can_trigger_busy() {
    let rows = make_rows(&["\u{2022} Working (1m 31s \u{2022} esc to interrupt)"]);
    let chrome_only = compute_chrome_only(&rows, false, false, false);
    let has_spinner = chrome_only && rows.iter().any(|r| crate::chrome::is_spinner_row(&r.text));
    assert!(chrome_only, "Codex spinner is chrome_only");
    assert!(has_spinner, "Codex spinner is detected as spinner");
    assert!(
        !chrome_only || has_spinner,
        "spinner-only chunk must pass the busy transition gate"
    );
}

#[test]
fn test_static_chrome_cannot_trigger_busy() {
    let rows = make_rows(&["\u{23F5}\u{23F5} auto mode"]);
    let chrome_only = compute_chrome_only(&rows, false, false, false);
    let has_spinner = chrome_only && rows.iter().any(|r| crate::chrome::is_spinner_row(&r.text));
    assert!(chrome_only, "mode-line is chrome_only");
    assert!(!has_spinner, "mode-line is NOT a spinner");
    assert!(
        chrome_only && !has_spinner,
        "static chrome must NOT pass the busy transition gate"
    );
}

/// Build ChangedRows for a subset of a full screen, mirroring how the VT
/// reader reports only the rows a chunk actually repainted (`row_index`
/// preserved against the full screen).
fn changed_at(screen: &[&str], indices: &[usize]) -> Vec<ChangedRow> {
    indices
        .iter()
        .map(|&i| ChangedRow {
            row_index: i,
            text: screen[i].to_string(),
        })
        .collect()
}

/// Mirror process_chunk's chrome-cutoff filter (pty.rs ~1996): drop changed
/// rows at or below the footer cutoff, keeping only the content zone.
fn filter_below_cutoff(screen: &[&str], changed: Vec<ChangedRow>) -> Vec<ChangedRow> {
    if changed.is_empty() {
        return changed;
    }
    match crate::chrome::find_chrome_cutoff(screen) {
        Some(cutoff) => changed
            .into_iter()
            .filter(|r| r.row_index < cutoff)
            .collect(),
        None => changed,
    }
}

/// Regression for the false-busy "flap" (root cause + agnostic fix,
/// 2026-06-17). Claude Code repaints its whole input area periodically.
/// Positionally, EVERYTHING below the second separator of the input box is
/// chrome — status bar, mode line, usage gauge — regardless of its glyphs.
/// The user's custom HUD renders a context gauge (`█░`) and a mode line
/// (`·`) there; `is_spinner_row` reads those glyphs as a live spinner.
///
/// The fix is positional, not glyph-based: spinner keepalive runs on the
/// SAME post-cutoff `changed_rows` as everything else. A repaint that only
/// touches footer rows below the cutoff yields an EMPTY post-filter set →
/// chrome_only with no spinner → no busy transition. Agnostic to whatever
/// the user puts in their status bar. These are the exact rows captured
/// from the live flapping instance.
#[test]
fn test_statusbar_repaint_below_cutoff_does_not_flap_busy() {
    let sep = "────────────────────────────────────────────────────────────────────────";
    let screen: Vec<&str> = vec![
        "Here is the answer to your question.",
        "",
        sep,
        "❯",
        sep,
        "[Opus 4.8 (1M) | Team] █░░░░░░░░░ 6% | gh-metrics git:(master)",
        "5h: 21% (50m) | 7d: 23% | $31.00 | 📅 $199.70 | 13h",
        "⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents",
    ];
    // The periodic repaint re-emits only the footer rows (indices 5-7).
    let changed = changed_at(&screen, &[5, 6, 7]);
    let filtered = filter_below_cutoff(&screen, changed);
    assert!(
        filtered.is_empty(),
        "all-footer repaint must yield an empty post-cutoff set"
    );
    let chrome_only = compute_chrome_only(&filtered, true, false, false);
    let has_spinner = chrome_only
        && filtered
            .iter()
            .any(|r| crate::chrome::is_spinner_row(&r.text));
    assert!(chrome_only, "empty post-cutoff set is chrome_only");
    assert!(
        chrome_only && !has_spinner,
        "statusbar repaint must NOT pass the busy transition gate (no flap)"
    );
}

/// Companion to the flap regression: a REAL working spinner renders ABOVE
/// the input separator (in the content zone), so it survives the chrome
/// cutoff and the post-filter spinner check fires — keeping the agent alive.
/// Otherwise the agent false-idles mid-think (the dangerous direction the
/// single-path design prevents). This is what makes the positional fix safe:
/// no supported agent renders a genuine working spinner below the separator.
#[test]
fn test_content_zone_spinner_still_keeps_alive() {
    let cc_sep = "────────────────────────────────────────────────────────────────────────";
    let gem_sep =
        "─────────────────────────────────────────────────────────────────────────────────";
    let gem_top = "▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀";
    let gem_bot = "▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▀▀";
    // (full screen, index of the working spinner row). Spinner is ABOVE the
    // input separator in every case — verified against the cutoff tests.
    let cases: Vec<(Vec<&str>, usize)> = vec![
        // Claude Code: dingbat thinking spinner in the transcript zone.
        (
            vec![
                "✻ Cogitating… (3m 47s · ↓ 2.2k tokens)",
                "",
                cc_sep,
                "❯",
                cc_sep,
                "[Opus 4.8 (1M) | Team] █░░░░░░░░░ 6% | gh-metrics git:(master)",
            ],
            0,
        ),
        // Gemini CLI: braille spinner above the separator (live layout).
        (
            vec![
                "✦ I will read the package.json file.",
                " ⠴ Check tool-specific usage stats… (esc to cancel, 14s)",
                gem_sep,
                " Shift+Tab to accept edits",
                gem_top,
                " >   Type your message or @path/to/file",
                gem_bot,
                " workspace (/directory)          branch          sandbox",
            ],
            1,
        ),
    ];
    for (screen, spinner_idx) in &cases {
        let changed = changed_at(screen, &[*spinner_idx]);
        let filtered = filter_below_cutoff(screen, changed);
        assert!(
            filtered.iter().any(|r| r.row_index == *spinner_idx),
            "content-zone spinner at row {spinner_idx} must survive the cutoff: {:?}",
            screen[*spinner_idx]
        );
        let chrome_only = compute_chrome_only(&filtered, false, false, false);
        let has_spinner = chrome_only
            && filtered
                .iter()
                .any(|r| crate::chrome::is_spinner_row(&r.text));
        assert!(
            !chrome_only || has_spinner,
            "content-zone spinner {:?} must pass the busy transition gate",
            screen[*spinner_idx]
        );
    }
}

/// Aider during generation has NO bottom input box (prompt_toolkit has
/// returned), so `find_chrome_cutoff` finds no separator/prompt and returns
/// None → nothing is filtered → the Knight Rider spinner survives and keeps
/// the agent alive. This is why the positional fix does not false-idle Aider
/// even though its spinner is a bare block run.
#[test]
fn test_aider_generation_spinner_keeps_alive() {
    let screen: Vec<&str> = vec![
        "Applied edit to src/main.rs",
        "█░  Waiting for openrouter/anthropic/claude-sonnet-4.5",
    ];
    assert_eq!(
        crate::chrome::find_chrome_cutoff(&screen),
        None,
        "Aider generation view has no input box → no cutoff"
    );
    let changed = changed_at(&screen, &[1]);
    let filtered = filter_below_cutoff(&screen, changed);
    assert!(
        filtered.iter().any(|r| r.row_index == 1),
        "Knight Rider spinner must survive (no cutoff to drop it)"
    );
    let chrome_only = !filtered.is_empty() && filtered.iter().all(|r| is_chrome_row(&r.text));
    // Aider's Knight Rider bar leads its row, so the structural is_spinner_row
    // matches it (#446-596f).
    let has_spinner = chrome_only
        && filtered
            .iter()
            .any(|r| crate::chrome::is_spinner_row(&r.text));
    assert!(
        !chrome_only || has_spinner,
        "Aider Knight Rider spinner must pass the busy transition gate"
    );
}

// --- Presence-driven working-status keepalive (Codex frozen-TUI false-idle) ---

/// Codex freezes its TUI during a child subprocess (long `cargo`/`git`): the
/// grid stops changing for minutes, so the change-driven spinner keepalive
/// cannot refresh `last_output_ms` and the idle timer would falsely flip
/// idle. The presence guard must still see the `• Working (… esc to
/// interrupt)` line in the content zone and hold the agent busy.
#[test]
fn test_codex_frozen_working_line_holds_busy() {
    // Real layout (mirrors the live capture): the working line sits directly
    // above the `›` input prompt, with the model footer below it.
    let screen: Vec<String> = vec![
        "• Ran cargo test -p agent2-transport --locked".into(),
        "  └     Blocking waiting for file lock on package cache".into(),
        "    … +30 lines (ctrl + t to view transcript)".into(),
        "• Working (14m 56s • esc to interrupt)".into(),
        "› Improve documentation in @filename".into(),
        "  gpt-5.5 high · ~/Gits/LS/agent2".into(),
    ];
    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Working,
        "a frozen Codex working line above the prompt must keep the agent busy"
    );
}

/// When Codex finishes a turn the working line is gone (only the ready
/// prompt remains) → the presence guard must NOT hold busy, so the idle
/// timer is free to transition busy→idle normally.
#[test]
fn test_codex_ready_prompt_allows_idle() {
    let screen: Vec<String> = vec![
        "• Done. Added deny.toml and updated Cargo.toml.".into(),
        "› Improve documentation in @filename".into(),
        "  gpt-5.5 high · ~/Gits/LS/agent2".into(),
    ];
    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Ready,
        "a ready prompt with no working line must allow the idle transition"
    );
}

/// Regression: Codex separators divide tool output from the answer; they are
/// not prompt-box chrome. The old presence helper applied find_chrome_cutoff,
/// chose this separator over the later prompt, and discarded Working.
#[test]
fn test_codex_working_after_tool_separator_is_detected() {
    let screen: Vec<String> = vec![
        "• Ran cargo test --workspace".into(),
        "────────────────────────────────────────────────────────".into(),
        "• I am checking the remaining failures.".into(),
        "• Working (2m 55s • esc to interrupt)".into(),
        "› Add tests for the activity detector".into(),
        "  gpt-5.5 high · ~/repo".into(),
    ];
    let refs: Vec<&str> = screen.iter().map(String::as_str).collect();
    assert_eq!(
        crate::chrome::find_chrome_cutoff(&refs),
        Some(1),
        "fixture must reproduce the misleading generic cutoff"
    );
    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Working
    );
}

/// Regression (live capture, session "Native Closure"): while a background
/// terminal runs Codex swaps the status verb to `Waiting for background
/// terminal`. The turn is still interruptible, but the verb-keyed presence
/// check read Ready and the session showed a green idle dot for minutes.
#[test]
fn test_codex_background_terminal_wait_holds_busy() {
    let screen: Vec<String> = vec![
        "• Il secondo pre-push ha già superato nuovamente check, Clippy e audit root/plugin.".into(),
        String::new(),
        "• Waiting for background terminal (41s • esc to interrupt) · 1 background terminal running · /ps to view · …".into(),
        "  └ rtk git fetch origin POC-00002-BLADES-REFINEMENT && rtk git rev-parse origin/POC-00002…".into(),
        String::new(),
        String::new(),
        "› Use /skills to list available skills".into(),
        "  gpt-5.6-sol medium · ~/Gits/CC_Playground/itview · master · Context 67% left".into(),
    ];
    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Working,
        "a running background terminal must keep the Codex session busy"
    );
}

/// Live 2026-07-28 regression: while a background command is running Codex
/// v0.145 renders the current composer with `»`, while submitted transcript
/// prompts still use `›`. Looking only for `›` selected the historical row,
/// missed the later Working marker, and flipped the session idle every few
/// seconds until the next user submission.
#[test]
fn test_codex_guillemet_composer_finds_later_working_status() {
    let screen = fixture_rows(include_str!(
        "../../../tests/terminal-stress/fixtures/codex-background-working.txt"
    ));
    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Working,
        "the lowest current composer must anchor the working neighborhood"
    );
}

/// Live 2026-07-29 regression: Codex may begin an internal continuation
/// after the previous task emitted `suggest:`. Its persistent goal HUD still
/// says `Goal achieved`, but the interruptible Working row is authoritative.
#[test]
fn test_codex_goal_achieved_hud_does_not_hide_current_working_status() {
    let screen = fixture_rows(include_str!(
        "../../../tests/terminal-stress/fixtures/codex-completed-internal-working.txt"
    ));
    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Working
    );
}

#[test]
fn test_codex_historical_working_far_from_prompt_does_not_latch_busy() {
    let mut screen = vec!["• Working (1m • esc to interrupt)".to_string()];
    screen.extend((0..8).map(|n| format!("old transcript row {n}")));
    screen.push("› Ready for the next request".into());
    screen.push("  gpt-5.5 high · ~/repo".into());
    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Ready
    );
}

#[test]
fn historical_codex_prompt_outside_current_chrome_is_not_ready() {
    let mut screen = vec!["› an old submitted request".to_string()];
    screen.extend((0..8).map(|n| format!("current output row {n}")));

    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Unknown
    );
}

#[test]
fn codex_draft_prompt_above_tall_hud_is_current_chrome() {
    let mut screen = vec![
        "current output".to_string(),
        "─".repeat(80),
        "› Run /review on my current changes".to_string(),
        "─".repeat(80),
    ];
    screen.extend((0..30).map(|n| format!("custom HUD row {n}")));

    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Ready
    );
}

#[test]
fn gemini_markdown_quote_in_history_is_not_a_ready_prompt() {
    let mut screen = vec!["> quoted user prose".to_string()];
    screen.extend((0..8).map(|n| format!("current output row {n}")));

    assert_eq!(
        detect_gemini_screen_activity(&screen),
        AgentScreenActivity::Unknown
    );
}

#[test]
fn gemini_prompt_above_tall_hud_is_current_chrome() {
    let mut screen = vec![
        "current output".to_string(),
        "─".repeat(80),
        "> Type your message".to_string(),
        "─".repeat(80),
    ];
    screen.extend((0..30).map(|n| format!("custom HUD row {n}")));

    assert_eq!(
        detect_gemini_screen_activity(&screen),
        AgentScreenActivity::Ready
    );
}

// ---------------------------------------------------------------------
// Stuck-busy battery (#446-596f).
//
// Symptom history: sessions pinned BUSY forever by STATIC glyphs the screen
// classifier read as a live spinner — a completed-turn summary (`✻ Sautéed
// for 1m 25s`), a `· run /mcp` hint, a wiz HUD `░░` bar. Each glyph fix
// regressed differently (a hash-based liveness gate blocked re-latching but
// never demoted the Working classification, so the idle path stayed
// unreachable).
//
// Definitive design: "if the text above the input area moves, the agent is
// active — period." BUSY is latched/kept ONLY by movement (post-cutoff
// `changed_rows` are text-equality diffed, so a frozen glyph produces no
// ChangedRow and is inert by construction), by user submission, and by
// hooks. The Claude/Gemini/Aider screen classifiers are PROMPT-based only
// (Ready/Unknown, never Working), so a static glyph can never mask the
// ready prompt or hold the idle path hostage. Codex is the one deliberate
// exception: its presence-based `• Working (… esc to interrupt)` line holds
// BUSY while its TUI legitimately freezes during a child process — accepted
// policy: for Codex we prefer false-BUSY over false-IDLE.
// ---------------------------------------------------------------------

/// A representative Claude idle screen: assistant output, a summary/spinner
/// line, a blank gap, then the input prompt.
fn claude_screen_with(mid_line: &str) -> Vec<String> {
    vec![
        "⏺ Fixed the bug and ran the tests — all green.".into(),
        String::new(),
        "  Searched for 1 pattern, read 1 file (ctrl+o to expand)".into(),
        String::new(),
        mid_line.into(),
        String::new(),
        "❯ ".into(),
        String::new(),
    ]
}

/// Completed summaries and inert decoration above an empty composer remain
/// Ready. Active phase names are covered separately because current Claude
/// versions can leave the composer visible during long tool calls.
#[test]
fn claude_completed_decorations_remain_ready() {
    for mid in [
        "✻ Sautéed for 1m 25s", // completed-turn summary
        "✳ Ideated for 2m 9s · 1 local agent still running",
        "· Proofed for 1m 14s (↓ 1.6k tokens)",
        "✽ Sautéed for 12s",
    ] {
        let screen = claude_screen_with(mid);
        assert_eq!(
            detect_claude_screen_activity(&screen),
            AgentScreenActivity::Ready,
            "{mid:?}: a visible empty ❯ composer is Ready — no glyph can mask it"
        );
    }
}

/// Live capture from session "DB corruption": Claude kept the empty `❯`
/// composer on screen throughout a long tool call. The active phase marker
/// must outrank that composer even if load/coalescing freezes its text.
#[test]
fn claude_active_phase_with_visible_composer_holds_busy() {
    let screen = fixture_rows(include_str!(
        "../../../tests/terminal-stress/fixtures/claude-blocking-stop-hook.txt"
    ));
    assert_eq!(
        detect_claude_screen_activity(&screen),
        AgentScreenActivity::Working
    );
}

/// A semantic active phase is Working with or without a visible composer.
/// This presence fallback is required when repaint movement freezes while a
/// long child or blocking hook still owns the turn.
#[test]
fn claude_active_phase_without_prompt_holds_busy() {
    let screen: Vec<String> = vec![
        "⏺ Editing src/main.rs…".into(),
        String::new(),
        "✻ Sautéing… (12s · esc to interrupt)".into(),
        String::new(),
    ];
    assert_eq!(
        detect_claude_screen_activity(&screen),
        AgentScreenActivity::Working
    );
}

/// Live 2026-07-19 regression: Claude echoes the submitted argv prompt as a
/// `❯ task` transcript row. While the turn is still running that historical
/// row can remain inside the bottom scan window beside an animated spinner;
/// it is not the empty composer and must never confirm idle.
#[test]
fn claude_submitted_prompt_row_is_not_a_ready_composer() {
    for prompt in [
        "❯ Read-only review the Windows native smoke scope",
        "  ❯ draft text not yet submitted",
    ] {
        let screen = vec![
            prompt.to_string(),
            "⏺ Reading 1 file…".into(),
            "✻ Boogieing…".into(),
        ];
        assert_eq!(
            detect_claude_screen_activity(&screen),
            AgentScreenActivity::Unknown,
            "only Claude's empty composer is Ready: {prompt:?}"
        );
    }
}

/// THE core invariant of the movement design: a byte-identical repaint of a
/// frozen "spinner" line produces NO ChangedRow (text-equality diff in
/// `TerminalGrid::process`), so it can never pass the reader's busy gate —
/// while a genuinely animating frame always does.
#[test]
fn frozen_summary_repaint_produces_no_movement() {
    let mut grid = crate::terminal_grid::TerminalGrid::new(24, 80, 1000);
    let frame = "\x1b[H\x1b[2K\u{273B} Saut\u{00E9}ed for 1m 25s";
    let first = grid.process(frame.as_bytes());
    assert!(
        first.iter().any(|r| crate::chrome::is_spinner_row(&r.text)),
        "first paint of the line IS movement"
    );
    let repaint = grid.process(frame.as_bytes());
    assert!(
        repaint.is_empty(),
        "byte-identical repaint must produce no ChangedRow → no busy evidence"
    );
    let animated = grid.process("\x1b[H\x1b[2K\u{273B} Saut\u{00E9}ing\u{2026} (13s)".as_bytes());
    assert!(
        animated
            .iter()
            .any(|r| crate::chrome::is_spinner_row(&r.text)),
        "an animating spinner frame IS movement and keeps/latches BUSY"
    );
}

/// A real captured Claude idle screen: the `▐▛███▜▌` welcome banner (█ art),
/// the empty `❯` input box framed by separators, and a wiz status-line HUD
/// whose progress bar is a run of `░`/`█` block glyphs. Nothing here is an
/// animated spinner — the turn is over and Claude waits for input.
fn claude_idle_with_banner_and_hud() -> Vec<String> {
    vec![
        "╭─── Claude Code v2.1.202 ──────────────────────────────╮".into(),
        "│                   ▐▛███▜▌                   │ What's new".into(),
        "│                  ▝▜█████▛▘                  │ Forked subagents".into(),
        "│      Opus 4.8 (1M context) · Claude Team    │           ".into(),
        "╰───────────────────────────────────────────────────────╯".into(),
        String::new(),
        " ⚠ 2 MCP servers need authentication · run /mcp".into(),
        String::new(),
        "───────────────────────────────────────────────────────────".into(),
        "❯ ".into(),
        "───────────────────────────────────────────────────────────".into(),
        "  [Opus 4.8 (1M) | Team] ░░░░░░░░░░ 0% | cerebro | [C1 S33]".into(),
        "  5h: 52% (52m) | 7d: 18% (22h) | $0 | 📅 $124.01 | 13m".into(),
        "  ⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents".into(),
    ]
}

/// #446-596f regression: block-glyph art (the welcome banner) and a status-
/// line HUD progress bar (`░░░░`) are NOT Claude's animated spinner. Claude's
/// spinner is dingbats (✻ ✳ ✶) / middle-dot `·`; solid blocks appear only in
/// static art. Before the fix, `is_spinner_row` matched `█`/`░`, so an idle
/// Claude prompt read Working and the session never returned to idle.
#[test]
fn claude_idle_with_wiz_hud_is_ready_not_working() {
    let screen = claude_idle_with_banner_and_hud();
    assert_eq!(
        detect_claude_screen_activity(&screen),
        AgentScreenActivity::Ready,
        "an idle Claude prompt under banner art, a `· run /mcp` hint and a \
             live wiz HUD is Ready, not Working"
    );
}

/// The wiz HUD ticks every second (elapsed timer, token counts), so the old
/// frozen-signature liveness gate could not save us — the bar is genuinely
/// changing. The only robust cut is that block glyphs are not a spinner.
#[test]
fn wiz_hud_progress_bar_is_not_a_claude_spinner() {
    let hud = "  [Opus 4.8 (1M) | Team] ██░░░░░░░░ 17% | cerebro".to_string();
    assert!(
        !crate::chrome::is_spinner_row(&hud),
        "a status-line progress bar is not an animated spinner"
    );
}

/// Guardrail: the fix must NOT break Aider, whose real "Knight Rider" spinner
/// IS a run of block glyphs that LEADS its row, so the structural
/// `is_spinner_row` still matches it — its movement latches/keeps BUSY via
/// the reader gate. Classification stays prompt-based: mid-generation Aider
/// has no input box, so the screen is Unknown (never a false Ready).
#[test]
fn aider_knight_rider_block_spinner_still_movement_evidence() {
    assert!(
        crate::chrome::is_spinner_row("░░░█░░░░░░"),
        "Aider's Knight Rider block spinner leads the row → still a spinner"
    );
    let generating: Vec<String> = vec!["Applied edit to src/main.rs".into(), "░░░█░░░░░░".into()];
    assert_eq!(
        detect_aider_screen_activity(&generating),
        AgentScreenActivity::Unknown,
        "no input box during generation → Unknown, BUSY held by movement"
    );
}

#[test]
fn codex_working_is_presence_based_by_policy() {
    // Codex Working comes from the "esc to interrupt" status line presence,
    // NOT from movement: its TUI legitimately freezes for minutes while a
    // child process (cargo, git) runs. Accepted policy: prefer false-BUSY
    // over false-IDLE for Codex.
    let working = vec![
        "• Ran cargo test --workspace".to_string(),
        "• Working (2m 55s • esc to interrupt)".into(),
        "› Add tests".into(),
        "  gpt-5.6 high · ~/repo".into(),
    ];
    assert_eq!(
        detect_codex_screen_activity(&working),
        AgentScreenActivity::Working
    );
}

/// Codex v0.146.0 grew its status row from `<model> <effort> · <N>% left · <dir>` to
/// `<model> <effort> · <dir> · <branch> · Context <N>% left · <N>K window`. The adapter
/// must stay blind to that row: it anchors on the `›` prompt plus the interrupt hint,
/// both branch- and gauge-independent. Rows transcribed from a live v0.146.0 session
/// captured 2026-08-02.
#[test]
fn test_codex_v0_146_status_row_with_git_branch_does_not_change_detection() {
    const STATUS_ROW: &str = "  gpt-5.6-luna xhigh \u{00B7} ~/Gits/personal/tuicommander \u{00B7} main \u{00B7} Context 96% left \u{00B7} 247K window";

    let working = vec![
        "\u{203A} Run this shell command with your tool: sleep 25 && echo hello".to_string(),
        "\u{2022} Boss, eseguo il comando richiesto.".to_string(),
        "\u{2022} Working (3s \u{2022} esc to interrupt) \u{00B7} 1 background terminal running \u{00B7} /ps to view".to_string(),
        "\u{203A} Explain this codebase".to_string(),
        STATUS_ROW.to_string(),
    ];
    assert_eq!(
        detect_codex_screen_activity(&working),
        AgentScreenActivity::Working
    );

    // Finished turn: separators and `• Output:` sit in the prompt neighborhood, and the
    // status row still carries the branch. Nothing there is an interrupt hint.
    let finished = vec![
        "\u{2022} Ran sleep 25 && echo hello".to_string(),
        "  \u{2514} hello".to_string(),
        "\u{2500}".repeat(120),
        "\u{2022} Output:".to_string(),
        "  hello".to_string(),
        "\u{2500}".repeat(120),
        "\u{203A} Explain this codebase".to_string(),
        STATUS_ROW.to_string(),
    ];
    assert_eq!(
        detect_codex_screen_activity(&finished),
        AgentScreenActivity::Ready
    );

    // The branch field itself must never be mistaken for chrome that holds BUSY.
    assert!(!crate::chrome::is_working_status_row(STATUS_ROW));
}

#[test]
fn test_agent_ready_requires_stable_observation() {
    let mut silence = SilenceState::new();
    assert!(!silence.note_ready_screen());
    assert!(!silence.idle_confirmed);
    silence.ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(silence.idle_confirmed);
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
    silence.note_explicit_state(SHELL_BUSY, false);
    silence.note_real_activity();
    silence.ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(!silence.explicit_busy);
    assert!(silence.idle_confirmed);
}

/// Captured live from grok 0.2.114: the composer moved inside a rounded box, so the old
/// bare-`❯` match never fired and the tab stayed BUSY minutes after the turn finished.
#[test]
fn test_grok_boxed_composer_is_ready_and_spinner_still_wins() {
    let finished = vec![
        "     ❯ List the numbers 1 to 60, one per line.                    5:42 PM".to_string(),
        "     1 2 3 4 5 6 7 8 9 10                                                ".to_string(),
        "     Worked for 2.6s                                   stop  [hooks: 1]  ".to_string(),
        "  ╭────────────────────────────────────────────────────────────────────╮".to_string(),
        "  │ ❯                                                                  │".to_string(),
        "  ╰─────────────────────────────── Grok 4.5 (high) · always-approve ───╯".to_string(),
        "  Shift+Tab:mode  │  Ctrl+.:shortcuts".to_string(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("grok"), &finished),
        AgentScreenActivity::Ready
    );

    // Mid-turn grok keeps the same composer box on screen, so the spinner must outrank it.
    let mut running = finished.clone();
    running.insert(
        2,
        "    ⠋ Waiting for response… 1.1s                     1.1s ⇣6.98k [stop]".to_string(),
    );
    assert_eq!(
        detect_agent_screen_activity(Some("grok"), &running),
        AgentScreenActivity::Working
    );
}

/// Captured live from pi 0.83.0. pi's composer is a bare reverse-video cursor block with
/// no prompt glyph, so readiness rests on the status row plus the absence of a spinner.
#[test]
fn test_pi_finished_turn_is_ready_and_working_row_wins() {
    let separator = "─".repeat(100);
    let finished = vec![
        " Count from 1 to 40, one number per line, no tools, no commentary.".to_string(),
        " 1".to_string(),
        " 2".to_string(),
        separator.clone(),
        "                                                                  ".to_string(),
        separator.clone(),
        "~/Gits/personal/tuicommander (main)".to_string(),
        "↑1.3k ↓1.8k R15k W6.0k CH88.5% $0.104 3.4%/272k (auto)      (openai) gpt-5.6-sol • medium"
            .to_string(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("pi"), &finished),
        AgentScreenActivity::Ready
    );

    // Mid-turn pi keeps the same separators and status row; only the composer row swaps.
    let mut running = finished.clone();
    running[4] = " ⠏ Working...".to_string();
    assert_eq!(
        detect_agent_screen_activity(Some("pi"), &running),
        AgentScreenActivity::Working
    );
}

/// A pi screen must be identified by its own status row, not by any bottom row: without
/// this the adapter would report Ready for whatever happens to be on screen after pi exits.
#[test]
fn test_pi_screen_without_status_row_is_unknown() {
    let rows = vec![
        "$ ls".to_string(),
        "README.md  src".to_string(),
        "$ ".to_string(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("pi"), &rows),
        AgentScreenActivity::Unknown
    );
}

#[test]
fn test_pi_status_row_needs_the_context_gauge_and_model_separator() {
    assert!(is_pi_status_row(
        "0.0%/272k (auto)                          (openai) gpt-5.6-sol • medium"
    ));
    // Prose carrying a bullet but no context gauge is not chrome.
    assert!(!is_pi_status_row("read the file • then summarise it"));
    // A percentage that is not the context gauge must not qualify.
    assert!(!is_pi_status_row("coverage 88.5% • done"));
    assert!(!is_pi_status_row(""));
}

#[test]
fn test_pi_is_recognised_as_an_agent() {
    assert_eq!(classify_agent("pi"), Some("pi"));
    assert!(has_ready_screen_adapter(Some("pi")));
}

/// Rows below are transcribed from live opencode v1.18.5 screens captured on
/// 2026-08-02 in this repo (welcome, mid-turn, and finished-turn). The frame glyphs
/// were confirmed against a `script(1)` byte log: `┃` U+2503, `╹` U+2579, `▀` U+2580.
fn opencode_finished_screen() -> Vec<String> {
    vec![
        "     VT100 is dead. The terminal it defined will be with us for a long time.".into(),
        "     \u{25A3}  Build \u{00B7} Big Pickle \u{00B7} 31.3s".into(),
        "  \u{2503}".into(),
        "  \u{2503}".into(),
        "  \u{2503}".into(),
        "  \u{2503}  Build \u{00B7} Big Pickle OpenCode Zen".into(),
        format!("  \u{2579}{}", "\u{2580}".repeat(98)),
        "   /Users/stefano.straus/Gits/personal/tuicommander       18.1K (9%)  ctrl+p commands"
            .into(),
    ]
}

#[test]
fn test_opencode_finished_turn_is_ready_and_interrupt_hint_wins() {
    assert_eq!(
        detect_agent_screen_activity(Some("opencode"), &opencode_finished_screen()),
        AgentScreenActivity::Ready
    );

    // Mid-turn opencode keeps the very same composer frame on screen; only the status
    // bar swaps the cwd for a `⬝`/`■` progress bar plus the interrupt hint.
    let mut working = opencode_finished_screen();
    let last = working.len() - 1;
    working[last] = "   \u{2B1D}\u{2B1D}\u{2B1D}\u{2B1D}\u{2B1D}\u{25A0}\u{25A0}\u{25A0}  esc interrupt        18.1K (9%)  ctrl+p commands".into();
    assert_eq!(
        detect_agent_screen_activity(Some("opencode"), &working),
        AgentScreenActivity::Working
    );
}

/// The welcome screen (before any turn) carries a different status bar — `tab agents`
/// plus a tip row and a `path:branch … version` row — and must still read Ready.
#[test]
fn test_opencode_welcome_screen_is_ready() {
    let rows = vec![
        "                    \u{2588}\u{2580}\u{2580}\u{2588} \u{2588}\u{2580}\u{2580}\u{2588} \u{2588}\u{2580}\u{2580}\u{2588}".into(),
        "                       \u{2503}".into(),
        "                       \u{2503}  Ask anything... \"Fix a TODO in the codebase\"".into(),
        "                       \u{2503}".into(),
        "                       \u{2503}  Build \u{00B7} Big Pickle OpenCode Zen".into(),
        format!("                       \u{2579}{}", "\u{2580}".repeat(78)),
        "                       tab agents  ctrl+p commands".into(),
        "                                \u{25CF} Tip Run /connect to add an AI provider".into(),
        "  ~/Gits/personal/tuicommander:main                                        1.18.5".into(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("opencode"), &rows),
        AgentScreenActivity::Ready
    );
}

/// The interrupt hint survives a tool phase — captured while opencode ran
/// `sleep 20 && echo done` — which is precisely when a false idle would let
/// auto-standby SIGSTOP the session.
#[test]
fn test_opencode_tool_phase_is_working() {
    let mut rows = opencode_finished_screen();
    rows.insert(2, "  \u{2503}  \u{283C} sleep 20 && echo done".into());
    rows.insert(3, "     \u{25A3}  Build \u{00B7} Big Pickle".into());
    let last = rows.len() - 1;
    rows[last] = "   \u{2B1D}\u{2B1D}\u{2B1D}\u{2B1D}\u{25A0}\u{25A0}\u{25A0}\u{25A0}  esc interrupt        18.1K (9%)  ctrl+p commands".into();
    assert_eq!(
        detect_agent_screen_activity(Some("opencode"), &rows),
        AgentScreenActivity::Working
    );
}

/// Readiness must rest on OpenCode's own frame, not on whatever happens to be on
/// screen: a plain shell — and a frame whose status bar has not been painted — are
/// both Unknown rather than Ready.
#[test]
fn test_opencode_requires_its_own_frame_and_status_bar() {
    let shell = vec![
        "$ ls".to_string(),
        "README.md  src".to_string(),
        "$ ".to_string(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("opencode"), &shell),
        AgentScreenActivity::Unknown
    );

    // Frame close row without any `┃` frame row above it is not an OpenCode composer.
    let close_only = vec![
        format!("  \u{2579}{}", "\u{2580}".repeat(98)),
        "   /Users/x  ctrl+p commands".to_string(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("opencode"), &close_only),
        AgentScreenActivity::Unknown
    );

    // Half-painted screen: frame present, status bar not yet drawn.
    let mut unpainted = opencode_finished_screen();
    unpainted.pop();
    assert_eq!(
        detect_agent_screen_activity(Some("opencode"), &unpainted),
        AgentScreenActivity::Unknown
    );
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
    silence.ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(!silence.explicit_busy);
    assert!(silence.idle_confirmed);
}

#[test]
fn test_only_interpreters_take_the_argv0_detour() {
    assert!(is_script_interpreter("node"));
    assert!(is_script_interpreter("bun"));
    assert!(!is_script_interpreter("claude"));
    assert!(!is_script_interpreter("zsh"));
}

#[test]
fn test_old_ready_prompt_cannot_cancel_new_submission_before_activity() {
    let mut silence = SilenceState::new();
    silence.note_user_submission(true);
    silence.ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(!silence.note_ready_screen());
    assert!(silence.explicit_busy);
    assert!(!silence.idle_confirmed);

    silence.note_real_activity();
    silence.ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(silence.idle_confirmed);
}

#[test]
fn test_stable_ready_prompt_recovers_missed_hook_idle_after_activity() {
    let mut silence = SilenceState::new();
    silence.note_user_submission(true);
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.note_real_activity();
    silence.ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(!silence.explicit_busy);
    assert!(!silence.hook_busy);
    assert!(silence.idle_confirmed);
}

#[test]
fn test_hook_busy_cannot_be_overridden_before_turn_activity() {
    let mut silence = SilenceState::new();
    silence.note_user_submission(true);
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(!silence.note_ready_screen());
    assert!(silence.explicit_busy);
    assert!(silence.hook_busy);
    assert!(!silence.idle_confirmed);
}

#[test]
fn test_fresh_hook_busy_blocks_ready_after_prior_recovery() {
    let mut silence = SilenceState::new();
    silence.note_user_submission(true);
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.note_real_activity();
    silence.ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());

    silence.note_explicit_state(SHELL_BUSY, true);
    silence.ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(!silence.note_ready_screen());
    assert!(silence.explicit_busy);
    assert!(silence.hook_busy);
    assert!(!silence.idle_confirmed);
}

#[test]
fn test_working_row_cannot_relatch_a_declared_completed_turn() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "completed-working-row";
    state.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("codex".into()),
            ..Default::default()
        },
    );
    state.shell_states.insert(
        session_id.into(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    let mut lifecycle = SilenceState::new();
    lifecycle.confirm_idle();
    lifecycle.mark_suggest_candidate(vec!["Review diff".into()], 0);
    let lifecycle = Arc::new(Mutex::new(lifecycle));

    apply_working_evidence(
        &state,
        &lifecycle,
        session_id,
        now_epoch_ms(),
        "working-screen",
    );

    assert_eq!(
        state
            .shell_states
            .get(session_id)
            .unwrap()
            .load(std::sync::atomic::Ordering::Acquire),
        SHELL_IDLE
    );
    assert!(lifecycle.lock().idle_confirmed);
}

#[test]
fn test_codex_moving_working_row_reopens_completed_internal_continuation() {
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "codex-completed-internal-continuation";
    state.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("codex".into()),
            suggested_actions: Some(vec!["Review diff".into()]),
            ..Default::default()
        },
    );
    state
        .shell_states
        .insert(session_id.into(), AtomicU8::new(SHELL_IDLE));
    state
        .last_output_ms
        .insert(session_id.into(), AtomicU64::new(1));
    let mut lifecycle = SilenceState::new();
    lifecycle.confirm_idle();
    lifecycle.mark_suggest_candidate(vec!["Review diff".into()], 0);
    let lifecycle = Arc::new(Mutex::new(lifecycle));

    apply_working_evidence(
        &state,
        &lifecycle,
        session_id,
        now_epoch_ms(),
        "working-screen-movement",
    );

    assert_eq!(
        state
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
    assert!(
        state
            .session_states
            .get(session_id)
            .unwrap()
            .suggested_actions
            .is_none()
    );
    let lifecycle = lifecycle.lock();
    assert!(!lifecycle.completion_declared_for_epoch(0));
    assert!(!lifecycle.idle_confirmed);
}

#[test]
fn test_claude_active_marker_reopens_premature_stop_hook_completion() {
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "claude-blocking-stop-hook";
    state.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("claude".into()),
            suggested_actions: Some(vec!["Premature follow-up".into()]),
            ..Default::default()
        },
    );
    state
        .shell_states
        .insert(session_id.into(), AtomicU8::new(SHELL_IDLE));
    state
        .last_output_ms
        .insert(session_id.into(), AtomicU64::new(1));
    let mut lifecycle = SilenceState::new();
    lifecycle.mark_suggest_candidate(vec!["Premature follow-up".into()], 0);
    lifecycle.note_explicit_state(SHELL_IDLE, true);
    let lifecycle = Arc::new(Mutex::new(lifecycle));
    state
        .silence_states
        .insert(session_id.into(), lifecycle.clone());

    let screen = vec![
        "✽ Nucleating… (8m 47s · ↓ 29.0k tokens)".to_string(),
        "❯".to_string(),
    ];
    let activity = detect_agent_screen_activity(Some("claude"), &screen);
    assert_eq!(activity, AgentScreenActivity::Working);
    assert_eq!(
        completion_adjusted_screen_activity(&state, &lifecycle, session_id, activity,),
        AgentScreenActivity::Working,
        "premature completion must not downgrade current Claude work"
    );

    apply_working_evidence(
        &state,
        &lifecycle,
        session_id,
        now_epoch_ms(),
        "working-screen",
    );

    assert_eq!(
        state
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
    assert!(
        state
            .session_states
            .get(session_id)
            .unwrap()
            .suggested_actions
            .is_none()
    );
    let lifecycle = lifecycle.lock();
    assert!(!lifecycle.completion_declared_for_epoch(0));
    assert!(!lifecycle.explicit_idle);
    assert!(!lifecycle.idle_confirmed);
}

#[test]
fn test_declared_completion_turns_stale_working_screen_into_ready_evidence() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "completed-working-timer";
    agent_session(&state, session_id, SHELL_BUSY);
    {
        let mut session = state.session_states.get_mut(session_id).unwrap();
        session.agent_type = Some("codex".into());
        session.background_probe_satisfied_turn_epoch = Some(session.turn_epoch);
    }
    let lifecycle = state.silence_states.get(session_id).unwrap().clone();
    {
        let mut lifecycle = lifecycle.lock();
        lifecycle.mark_suggest_candidate(vec!["Review diff".into()], 0);
        lifecycle.ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    }

    let screen_activity = completion_adjusted_screen_activity(
        &state,
        &lifecycle,
        session_id,
        AgentScreenActivity::Working,
    );
    let transition = try_timer_idle_transition(
        &state,
        &lifecycle,
        session_id,
        screen_activity,
        Some("codex"),
        Some(0),
    );

    assert!(transition.screen_confirms_idle);
    assert!(transition.transitioned);
    assert_eq!(
        state
            .shell_states
            .get(session_id)
            .unwrap()
            .load(std::sync::atomic::Ordering::Acquire),
        SHELL_IDLE
    );
}

#[test]
fn test_interrupt_request_plus_interrupted_screen_confirms_idle() {
    let mut silence = SilenceState::new();
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.note_interrupt_requested();
    assert!(silence.note_interrupted_screen());
    assert!(!silence.explicit_busy);
    assert!(silence.idle_confirmed);
}

#[test]
fn test_ctrl_c_alone_never_confirms_idle() {
    let mut silence = SilenceState::new();
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.note_interrupt_requested();
    assert!(silence.explicit_busy);
    assert!(!silence.idle_confirmed);
}

#[test]
fn test_working_screen_recovers_idle_to_busy() {
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "codex-false-idle";
    state
        .shell_states
        .insert(sid.into(), AtomicU8::new(SHELL_IDLE));
    state.last_output_ms.insert(sid.into(), AtomicU64::new(1));
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state.silence_states.insert(sid.into(), silence.clone());

    apply_working_evidence(&state, &silence, sid, now_epoch_ms(), "working-screen");

    assert_eq!(
        state.shell_states.get(sid).unwrap().load(Ordering::Acquire),
        SHELL_BUSY
    );
    assert!(!silence.lock().idle_confirmed);
}

#[test]
fn test_explicit_idle_outvotes_stale_working_screen() {
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "codex-stale-working";
    state
        .shell_states
        .insert(sid.into(), AtomicU8::new(SHELL_IDLE));
    state.last_output_ms.insert(sid.into(), AtomicU64::new(1));
    let mut sl = SilenceState::new();
    sl.note_explicit_state(SHELL_IDLE, true);
    let silence = Arc::new(Mutex::new(sl));
    state.silence_states.insert(sid.into(), silence.clone());

    apply_working_evidence(&state, &silence, sid, now_epoch_ms(), "working-screen");

    assert_eq!(
        state.shell_states.get(sid).unwrap().load(Ordering::Acquire),
        SHELL_IDLE
    );
    assert!(silence.lock().idle_confirmed);
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

#[test]
fn test_agent_screen_adapter_baselines() {
    // Claude and Codex have presence-based active markers because both can
    // freeze or leave a composer visible during long tools. Gemini/Aider
    // remain prompt-based and use movement to hold BUSY.
    let claude_working = vec!["✻ Cogitating… (12s)".into()];
    let claude_ready = vec!["Answer complete".into(), "❯".into()];
    let gemini_working_prompt_visible = vec![
        "⠴ Checking files… (esc to cancel, 14s)".into(),
        "────────────────────────".into(),
        "> Type your message".into(),
    ];
    let gemini_ready = vec!["> Type your message".into()];
    let aider_working = vec!["█░  Waiting for model".into()];
    let aider_ready = vec!["Tokens: 10k sent".into(), ">".into()];
    let grok_working = vec![
        "❯ Ask anything".into(),
        "⠹ Responding… 12s [stop]".into(),
        "⌘ Grok 4.3 OpenRouter · Medium effort".into(),
    ];
    let grok_ready = vec![
        "Done.".into(),
        "❯ Ask anything".into(),
        "⌘ Grok 4.3 OpenRouter · Medium effort".into(),
    ];

    for (agent, rows, expected) in [
        ("claude", claude_working, AgentScreenActivity::Working),
        ("claude", claude_ready, AgentScreenActivity::Ready),
        (
            "gemini",
            gemini_working_prompt_visible,
            AgentScreenActivity::Ready,
        ),
        ("gemini", gemini_ready, AgentScreenActivity::Ready),
        ("aider", aider_working, AgentScreenActivity::Unknown),
        ("aider", aider_ready, AgentScreenActivity::Ready),
        ("grok", grok_working, AgentScreenActivity::Working),
        ("grok", grok_ready, AgentScreenActivity::Ready),
    ] {
        assert_eq!(detect_agent_screen_activity(Some(agent), &rows), expected);
    }
}

#[test]
fn active_screen_matrix_is_stable_and_repairs_false_idle_repeatedly() {
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

    let state = crate::state::tests_support::make_test_app_state();
    let cases = [
        (
            "codex-chevron",
            "codex",
            vec![
                "• Working (7s • esc to interrupt)".to_string(),
                "› Use /skills to list available skills".to_string(),
            ],
        ),
        (
            "codex-guillemet-background",
            "codex",
            vec![
                "› historical submitted prompt".to_string(),
                "• Waiting for background terminal (41s • esc to interrupt) · 1 background terminal running".to_string(),
                String::new(),
                "» Use /skills to list available skills".to_string(),
            ],
        ),
        (
            "claude-visible-composer",
            "claude",
            vec![
                "✽ Nucleating… (3m 50s · ↓ 7.8k tokens)".to_string(),
                String::new(),
                "────────────────────────────────────────".to_string(),
                "❯".to_string(),
                "────────────────────────────────────────".to_string(),
            ],
        ),
        (
            "claude-frozen-tool",
            "claude",
            vec![
                "✻ Sautéing… (12s · esc to interrupt)".to_string(),
                String::new(),
                "❯".to_string(),
            ],
        ),
        (
            "grok-responding",
            "grok",
            vec![
                "❯ Ask anything".to_string(),
                "⠴ Responding… 12s [stop]".to_string(),
                "⌘ Grok 4.3 OpenRouter · Medium effort".to_string(),
            ],
        ),
    ];

    for (sid, agent, rows) in cases {
        state.session_states.insert(
            sid.into(),
            crate::state::SessionState {
                agent_type: Some(agent.into()),
                ..Default::default()
            },
        );
        state
            .shell_states
            .insert(sid.into(), AtomicU8::new(SHELL_IDLE));
        state.last_output_ms.insert(sid.into(), AtomicU64::new(1));
        let lifecycle = Arc::new(Mutex::new(SilenceState::new()));
        state.silence_states.insert(sid.into(), lifecycle.clone());

        for iteration in 0..256 {
            state
                .shell_states
                .get(sid)
                .unwrap()
                .store(SHELL_IDLE, Ordering::Release);
            lifecycle.lock().confirm_idle();

            let activity = detect_agent_screen_activity(Some(agent), &rows);
            assert_eq!(
                activity,
                AgentScreenActivity::Working,
                "{sid} iteration {iteration}"
            );
            apply_working_evidence(&state, &lifecycle, sid, now_epoch_ms(), "working-screen");
            assert_eq!(
                state.shell_states.get(sid).unwrap().load(Ordering::Acquire),
                SHELL_BUSY,
                "{sid} failed to repair false idle at iteration {iteration}"
            );
            assert!(!lifecycle.lock().idle_confirmed);
        }
    }
}

#[test]
fn completed_and_lookalike_screen_matrix_never_latches_working() {
    let claude_completed = fixture_rows(include_str!(
        "../../../tests/terminal-stress/fixtures/claude-completed.txt"
    ));
    let cases = [
        (
            "codex completed output",
            "codex",
            vec![
                "• Waited for background terminal · cargo test".to_string(),
                "» Use /skills to list available skills".to_string(),
            ],
            AgentScreenActivity::Ready,
        ),
        (
            "claude completed summary",
            "claude",
            claude_completed,
            AgentScreenActivity::Ready,
        ),
        (
            "claude hud progress",
            "claude",
            vec![
                "  [Opus | Team] ██░░░░░░░░ 17%".to_string(),
                "❯".to_string(),
            ],
            AgentScreenActivity::Ready,
        ),
        (
            "claude source lookalike below composer",
            "claude",
            vec![
                "Completed normally".to_string(),
                "❯".to_string(),
                "✻ source_example… (not live)".to_string(),
            ],
            AgentScreenActivity::Ready,
        ),
    ];

    for iteration in 0..256 {
        for (name, agent, rows, expected) in &cases {
            assert_eq!(
                detect_agent_screen_activity(Some(agent), rows),
                *expected,
                "{name} iteration {iteration}"
            );
        }
    }
}

#[derive(Clone, Copy)]
enum SanitizedTraceStep {
    Submit,
    RealActivity,
    WorkingScreen,
    ReadyScreen,
    UnknownScreen,
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
                    silence.ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
                    silence.note_ready_screen();
                }
            }
            SanitizedTraceStep::UnknownScreen => silence.note_unknown_screen(),
        }
    }
    silence
}

#[test]
fn sanitized_codex_and_claude_trace_replay_requires_post_submit_consumption() {
    // Sanitized from the 2026-07-18 live sequence: ready prompt → injected
    // checkpoint → working/real output → final protocol text → ready prompt.
    // Repository paths, prompts, and response content are intentionally omitted.
    for agent in ["codex", "claude"] {
        let completed = replay_sanitized_agent_trace(
            agent,
            &[
                SanitizedTraceStep::Submit,
                SanitizedTraceStep::WorkingScreen,
                SanitizedTraceStep::RealActivity,
                SanitizedTraceStep::ReadyScreen,
            ],
        );
        assert!(completed.idle_confirmed, "{agent} completed trace");

        let silent = replay_sanitized_agent_trace(
            agent,
            &[
                SanitizedTraceStep::Submit,
                SanitizedTraceStep::ReadyScreen,
                SanitizedTraceStep::ReadyScreen,
            ],
        );
        assert!(
            !silent.idle_confirmed,
            "{agent} silent/no-op submission must remain conservative without positive consumption"
        );

        let partial_redraw = replay_sanitized_agent_trace(
            agent,
            &[
                SanitizedTraceStep::Submit,
                SanitizedTraceStep::UnknownScreen,
                SanitizedTraceStep::ReadyScreen,
            ],
        );
        assert!(
            !partial_redraw.idle_confirmed,
            "{agent} partial/alternate-screen redraw must not prove consumption"
        );
    }
}

fn process(pid: u32, parent_pid: u32, name: &str, command: &str) -> ProcessTreeEntry {
    ProcessTreeEntry {
        pid,
        parent_pid,
        name: name.to_string(),
        command: command.to_string(),
        age_seconds: None,
    }
}

/// `sudo su` as the OS actually reports it: sudo re-execs itself, and on
/// macOS the second hop allocates its own PTY, so the inner shell is only
/// reachable through the parent chain.
fn sudo_su_tree() -> Vec<ProcessTreeEntry> {
    vec![
        process(100, 1, "zsh", "/bin/zsh"),
        process(200, 100, "sudo", "sudo su"),
        process(201, 200, "sudo", "sudo su"),
        process(202, 201, "su", "su"),
        process(203, 202, "sh", "sh"),
    ]
}

#[test]
fn a_bare_shell_reads_as_a_prompt() {
    assert!(is_prompt_shell_process(&process(1, 0, "sh", "sh")));
    assert!(is_prompt_shell_process(&process(1, 0, "bash", "bash -l")));
    // A login shell reports its name with a leading dash.
    assert!(is_prompt_shell_process(&process(1, 0, "-zsh", "-zsh")));
}

#[test]
fn a_shell_handed_a_command_is_work() {
    assert!(!is_prompt_shell_process(&process(
        1,
        0,
        "sh",
        "sh -c 'while true; do sleep 1; done'"
    )));
    assert!(!is_prompt_shell_process(&process(
        1,
        0,
        "bash",
        "bash -c make"
    )));
}

#[test]
fn a_non_shell_is_never_a_prompt() {
    assert!(!is_prompt_shell_process(&process(
        1,
        0,
        "dd",
        "dd if=/dev/rdisk11 of=/tmp/x.img"
    )));
    assert!(!is_prompt_shell_process(&process(1, 0, "vim", "vim a.txt")));
}

#[test]
fn nested_interactive_shell_is_a_prompt_not_work() {
    // The regression: `sudo su` latches the outer shell BUSY via OSC 133 and
    // the inner `sh` never emits the closing marker, so the tab reported
    // working for as long as the root shell lived.
    assert!(foreground_group_at_prompt(200, &sudo_su_tree()));
    assert!(foreground_group_at_prompt(
        300,
        &[process(300, 100, "sh", "sh")]
    ));
}

#[test]
fn a_wrapper_running_real_work_is_not_a_prompt() {
    let mut tree = sudo_su_tree();
    tree.push(process(204, 203, "dd", "dd if=/dev/rdisk11 of=/tmp/x.img"));
    assert!(
        !foreground_group_at_prompt(200, &tree),
        "work anywhere under the wrapper must keep the session busy"
    );
    assert!(!foreground_group_at_prompt(
        400,
        &[
            process(400, 100, "sudo", "sudo dd if=/dev/rdisk11"),
            process(401, 400, "dd", "dd if=/dev/rdisk11"),
        ]
    ));
}

#[test]
fn an_unknown_root_is_not_a_prompt() {
    // No snapshot entry for the pid means no evidence; fail toward busy.
    assert!(!foreground_group_at_prompt(999, &sudo_su_tree()));
}

fn busy_plain_shell(sid: &str, silent_for_ms: u64) -> AppState {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    state
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());
    state.last_output_ms.insert(
        sid.to_string(),
        AtomicU64::new(now_epoch_ms() - silent_for_ms),
    );
    state
}

#[test]
fn prompt_probe_waits_for_real_silence() {
    let sid = "s";
    assert!(prompt_probe_applies(
        &busy_plain_shell(sid, SHELL_PROMPT_PROBE_SILENCE_MS + 500),
        sid
    ));
    assert!(
        !prompt_probe_applies(&busy_plain_shell(sid, 200), sid),
        "a command that just printed is running, not parked at a prompt"
    );
}

#[test]
fn prompt_probe_leaves_agents_alone() {
    let sid = "s";
    let state = busy_plain_shell(sid, SHELL_PROMPT_PROBE_SILENCE_MS + 500);
    state.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            ..Default::default()
        },
    );
    assert!(
        !prompt_probe_applies(&state, sid),
        "agents own a ready-screen adapter; this probe must not second-guess it"
    );
}

#[test]
fn prompt_probe_ignores_an_idle_session() {
    use std::sync::atomic::AtomicU8;
    let sid = "s";
    let state = busy_plain_shell(sid, SHELL_PROMPT_PROBE_SILENCE_MS + 500);
    state
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_IDLE));
    assert!(!prompt_probe_applies(&state, sid));
}

#[test]
fn prompt_probe_demands_the_process_snapshot() {
    let sid = "s";
    let state = busy_plain_shell(sid, SHELL_PROMPT_PROBE_SILENCE_MS + 500);
    state
        .silence_states
        .insert(sid.to_string(), Arc::new(Mutex::new(SilenceState::new())));
    assert!(
        process_snapshot_is_demanded(&state),
        "without demand the cache stays empty and the probe can never fire"
    );
}

/// A process the snapshot could age. Ageless entries keep the name list as
/// the only rule, which is what every pre-existing case here asserts.
fn aged_process(
    pid: u32,
    parent_pid: u32,
    name: &str,
    command: &str,
    age_seconds: u64,
) -> ProcessTreeEntry {
    ProcessTreeEntry {
        age_seconds: Some(age_seconds),
        ..process(pid, parent_pid, name, command)
    }
}

#[test]
fn sanitized_background_command_keeps_agent_working_across_adapters() {
    // Sanitized from the 2026-07-19 live Codex sequence. The same lifecycle
    // contract applies to Claude: a ready composer is not proof that an
    // autonomous background command has completed.
    for (index, agent) in ["codex", "claude"].into_iter().enumerate() {
        let session_root = 100 + index as u32 * 100;
        let agent_pid = session_root + 1;
        let processes = vec![
            process(session_root, 1, "zsh", "zsh"),
            process(agent_pid, session_root, agent, agent),
            process(
                agent_pid + 1,
                agent_pid,
                "rtk",
                "rtk env CARGO_BUILD_JOBS=4 cargo test --locked -p agent2-core",
            ),
            process(agent_pid + 2, agent_pid + 1, "cargo", "cargo test --locked"),
            process(
                agent_pid + 3,
                agent_pid + 2,
                "agent2_core-test",
                "target/debug/deps/agent2_core-test",
            ),
        ];
        let root = agent_process_root(session_root, agent, &processes).unwrap();
        assert_eq!(root, agent_pid, "{agent} adapter root");
        assert!(
            has_meaningful_descendant(root, &processes),
            "{agent} must retain autonomous work while cargo descendants live"
        );

        let silence = replay_sanitized_agent_trace(
            agent,
            &[
                SanitizedTraceStep::Submit,
                SanitizedTraceStep::RealActivity,
                SanitizedTraceStep::ReadyScreen,
            ],
        );
        assert!(silence.idle_confirmed, "{agent} composer is terminal-ready");

        let state = crate::state::tests_support::make_test_app_state();
        let sid = format!("background-{agent}");
        state.session_states.insert(
            sid.clone(),
            crate::state::SessionState {
                agent_type: Some(agent.to_string()),
                background_work: true,
                ..Default::default()
            },
        );
        state
            .shell_states
            .insert(sid.clone(), std::sync::atomic::AtomicU8::new(SHELL_IDLE));
        state
            .silence_states
            .insert(sid.clone(), Arc::new(Mutex::new(silence)));

        let snapshot = state.session_state_with_shell(&sid).unwrap();
        assert_eq!(snapshot.shell_state.as_deref(), Some("idle"));
        assert_eq!(snapshot.agent_state.as_deref(), Some("working"));
        assert!(
            should_inject_now(&state, &sid),
            "terminal-ready must remain usable independently of task lifecycle"
        );
    }
}

#[test]
fn persistent_helpers_are_not_background_work() {
    let processes = vec![
        process(10, 1, "codex", "codex"),
        process(11, 10, "tuic-bridge", "tuic-bridge"),
        process(12, 10, "mdkb", "mdkb serve"),
        process(13, 10, "node", "node /opt/codex/node_repl.js"),
        // Descendants owned by helper plumbing are ignored with the helper.
        process(14, 12, "sqlite-worker", "sqlite-worker"),
    ];
    assert!(!has_meaningful_descendant(10, &processes));

    let mut with_real_child = processes;
    with_real_child.push(process(20, 10, "cargo", "cargo test --locked"));
    assert!(has_meaningful_descendant(10, &with_real_child));
}

#[test]
fn daemons_started_with_the_agent_are_not_background_work() {
    // Sanitized from a live 14-session instance on 2026-08-23, where every
    // agent reported `working` forever. Neither name here can go on the
    // helper list: `codex-code-mode-host` shipped with Codex 0.149.0 and the
    // next release may rename it, and `npm` must keep meaning work.
    let agent_age = 129_050;
    let processes = vec![
        aged_process(10, 1, "codex", "codex", agent_age),
        aged_process(
            11,
            10,
            "codex-code-mode-host",
            "/opt/homebrew/Caskroom/codex/0.149.0/bin/codex-code-mode-host",
            agent_age - 18,
        ),
        aged_process(
            12,
            10,
            "npm",
            "npm exec @upstash/context7-mcp",
            agent_age - 1,
        ),
    ];
    assert!(
        !has_meaningful_descendant(10, &processes),
        "daemons that came up with the agent are plumbing"
    );

    // The same two names, spawned by a turn instead of at startup.
    let mut spawned_by_a_turn = processes.clone();
    spawned_by_a_turn.push(aged_process(20, 10, "npm", "npm run build", 12));
    assert!(
        has_meaningful_descendant(10, &spawned_by_a_turn),
        "work must stay visible under a name the startup window also sees"
    );

    // One second past the window is already work.
    let mut just_outside = processes;
    just_outside.push(aged_process(
        21,
        10,
        "cargo",
        "cargo test",
        agent_age - AGENT_STARTUP_WINDOW_SECS - 1,
    ));
    assert!(has_meaningful_descendant(10, &just_outside));
}

#[test]
fn missing_ages_leave_the_helper_name_list_in_charge() {
    // Windows reports no creation time. The rule must then behave exactly as
    // it did before ages existed — erring toward reporting work.
    let ageless = vec![
        process(10, 1, "codex", "codex"),
        process(11, 10, "codex-code-mode-host", "codex-code-mode-host"),
    ];
    assert!(has_meaningful_descendant(10, &ageless));

    // A descendant older than its own agent cannot be work that agent
    // spawned; a skewed `ps` sample must not invent background work.
    let skewed = vec![
        aged_process(10, 1, "codex", "codex", 100),
        aged_process(11, 10, "mystery", "mystery", 400),
    ];
    assert!(!has_meaningful_descendant(10, &skewed));
}

#[cfg(not(windows))]
#[test]
fn elapsed_time_field_parses_every_ps_shape() {
    assert_eq!(parse_elapsed_time("05:12"), Some(312));
    assert_eq!(parse_elapsed_time("01:00:00"), Some(3600));
    assert_eq!(parse_elapsed_time("2-13:45:02"), Some(222_302));
    // `ps` never emits a bare second count, so one is not a valid reading.
    assert_eq!(parse_elapsed_time("42"), None);
    assert_eq!(parse_elapsed_time("-"), None);
    assert_eq!(parse_elapsed_time("1:2:3:4"), None);
    assert_eq!(parse_elapsed_time("aa:bb"), None);
}

#[cfg(not(windows))]
#[test]
fn timed_caffeinate_is_not_background_work_with_authoritative_argv() {
    let processes = vec![
        process(10, 1, "claude", "claude"),
        process(11, 10, "caffeinate", "caffeinate -i -t 300"),
    ];
    assert!(!has_meaningful_descendant(10, &processes));
}

#[test]
fn timed_caffeinate_helper_does_not_hide_wrapped_work() {
    for command in ["caffeinate -i -t 300", "/usr/bin/caffeinate -t 300 -i"] {
        assert!(is_standalone_timed_caffeinate(command));
        assert!(is_persistent_agent_helper_with_command_line(
            &process(11, 10, "caffeinate", command),
            true
        ));
    }
    for command in [
        "caffeinate -i -t 0",
        "caffeinate -i",
        "caffeinate -i cargo test",
        "caffeinate -i -t 300 cargo test",
    ] {
        assert!(!is_standalone_timed_caffeinate(command));
    }

    let wrapped_work = vec![
        process(10, 1, "claude", "claude"),
        process(11, 10, "caffeinate", "caffeinate -i cargo test"),
        process(12, 11, "cargo", "cargo test"),
    ];
    assert!(has_meaningful_descendant(10, &wrapped_work));
}

#[cfg(not(windows))]
#[test]
fn background_snapshot_macos_truncated_comm_fixture_excludes_helpers() {
    // Sanitized from macOS `ps -ww -axo pid=,ppid=,etime=,comm=,args=`
    // output. Darwin may truncate `comm` while unlimited-width `args`
    // retains the executable path needed to identify persistent integration
    // helpers. `codex-code-mode-host` is on no name list and is excluded
    // purely by having started with the agent.
    const MACOS_PS: &str = r#"
  700     1    01:00:05 /bin/zsh         /bin/zsh
  701   700    01:00:00 /Applications/C  /Applications/Codex.app/Contents/MacOS/codex
  702   701       59:58 /Users/boss/.lo  /Users/boss/.local/bin/mdkb serve
  703   701       59:58 /Users/boss/.ca  /Users/boss/.cache/tuic/tuic-bridge --stdio
  704   701       59:58 /opt/homebrew/b  /opt/homebrew/bin/node /Users/boss/.cache/tuic/node_repl.js
  705   702       59:57 sqlite-worker    sqlite-worker
  706   701       59:45 /opt/homebrew/Ca /opt/homebrew/Caskroom/codex/0.149.0/bin/codex-code-mode-host
"#;
    let processes = parse_process_tree_snapshot(true, MACOS_PS).unwrap();
    assert_eq!(
        processes[0].age_seconds,
        Some(3605),
        "the elapsed column must survive the truncated-comm layout"
    );
    assert_eq!(agent_process_root(700, "codex", &processes), Some(701));
    assert!(!has_meaningful_descendant(701, &processes));

    let mut with_turn_work = processes;
    with_turn_work.push(aged_process(707, 701, "cargo", "cargo test", 30));
    assert!(has_meaningful_descendant(701, &with_turn_work));
}

#[test]
fn version_named_claude_path_is_the_agent_root() {
    let processes = vec![
        process(10, 1, "zsh", "zsh"),
        process(
            11,
            10,
            "/Users/test/.local/share/claude/versions/2.1.87",
            "2.1.87",
        ),
        process(12, 11, "cargo", "cargo test"),
    ];
    assert_eq!(agent_process_root(10, "claude", &processes), Some(11));
    assert_eq!(
        background_work_from_snapshot(10, "claude", &processes),
        Some(true)
    );
}

#[test]
fn wrapper_is_not_counted_as_permanent_agent_work() {
    let idle = vec![
        process(20, 1, "claude-wrapper", "claude-wrapper"),
        process(21, 20, "/opt/claude/versions/2.1.87", "2.1.87"),
    ];
    assert_eq!(agent_process_root(20, "claude", &idle), Some(21));
    assert_eq!(
        background_work_from_snapshot(20, "claude", &idle),
        Some(false)
    );

    let custom_alias = vec![
        process(30, 1, "C2", "C2"),
        process(31, 30, "mdkb", "mdkb serve"),
    ];
    assert_eq!(agent_process_root(30, "claude", &custom_alias), Some(30));
    assert_eq!(
        background_work_from_snapshot(30, "claude", &custom_alias),
        Some(false)
    );
    let mut active_alias = custom_alias;
    active_alias.push(process(32, 30, "cargo", "cargo test"));
    assert_eq!(
        background_work_from_snapshot(30, "claude", &active_alias),
        Some(true)
    );
}

#[test]
fn windows_helper_classification_does_not_guess_node_arguments() {
    let node = process(40, 10, "node.exe", "node.exe node_repl.js");
    assert!(is_persistent_agent_helper_with_command_line(&node, true));
    assert!(
        !is_persistent_agent_helper_with_command_line(&node, false),
        "Toolhelp exposes only the executable name, so node.exe remains meaningful"
    );
    let dedicated = process(41, 10, "node_repl.exe", "");
    assert!(is_persistent_agent_helper_with_command_line(
        &dedicated, false
    ));
}

#[cfg(not(windows))]
#[test]
fn process_snapshot_rejects_nonzero_and_malformed_output() {
    assert!(parse_process_tree_snapshot(false, "10 1 05:12 zsh zsh").is_none());
    assert!(parse_process_tree_snapshot(true, "10 invalid 05:12 zsh zsh").is_none());
    assert!(parse_process_tree_snapshot(true, "10 1 05:12").is_none());
    // An unreadable elapsed column costs the age, not the whole snapshot:
    // the name list still has to work.
    let ageless = parse_process_tree_snapshot(true, "10 1 ? zsh zsh").unwrap();
    assert_eq!(ageless[0].age_seconds, None);
}

#[test]
fn failed_first_process_entry_is_not_a_valid_snapshot() {
    assert!(valid_process_snapshot(false, vec![process(10, 1, "zsh", "zsh")]).is_none());
    assert!(valid_process_snapshot(true, Vec::new()).is_none());
}

#[test]
fn process_snapshot_cache_is_shared_across_sessions() {
    let cache = ProcessSnapshotCache::default();
    cache.store(Some(vec![
        process(10, 1, "codex", "codex"),
        process(11, 10, "cargo", "cargo test"),
        process(20, 1, "claude", "claude"),
    ]));
    let (first_generation, first) = cache.load().unwrap();
    let (second_generation, second) = cache.load().unwrap();
    assert_eq!(first_generation, second_generation);
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(
        background_work_from_snapshot(10, "codex", &first),
        Some(true)
    );
    assert_eq!(
        background_work_from_snapshot(20, "claude", &second),
        Some(false)
    );
}

#[test]
fn background_snapshot_ready_waits_for_newer_generation_and_repairs_working() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "background-ready-generation";
    let parent_id = "background-ready-parent";
    agent_session(&state, child_id, SHELL_BUSY);
    state.session_states.get_mut(child_id).unwrap().agent_type = Some("codex".into());
    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    state
        .silence_states
        .get(child_id)
        .unwrap()
        .lock()
        .ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);

    let first_ready = try_timer_idle_transition(
        &state,
        &state.silence_states.get(child_id).unwrap().clone(),
        child_id,
        AgentScreenActivity::Ready,
        Some("codex"),
        Some(0),
    );
    assert!(!first_ready.transitioned);
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());

    state.process_snapshot_cache.store(Some(vec![
        process(10, 1, "codex", "codex"),
        process(11, 10, "cargo", "cargo test --locked"),
    ]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    let snapshot = state.session_state_with_shell(child_id).unwrap();
    assert_eq!(snapshot.agent_state.as_deref(), Some("working"));
    assert!(snapshot.background_work);
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());

    let reconciled_ready = try_timer_idle_transition(
        &state,
        &state.silence_states.get(child_id).unwrap().clone(),
        child_id,
        AgentScreenActivity::Ready,
        Some("codex"),
        Some(0),
    );
    assert!(reconciled_ready.transitioned);
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());

    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "idle");
}

#[test]
fn same_epoch_working_evidence_requires_a_new_ready_probe_boundary() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "background-same-epoch-ready";
    let parent_id = "background-same-epoch-parent";
    agent_session(&state, child_id, SHELL_BUSY);
    state.session_states.get_mut(child_id).unwrap().agent_type = Some("codex".into());
    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    let silence = state.silence_states.get(child_id).unwrap().clone();

    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    silence.lock().ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(
        !try_timer_idle_transition(
            &state,
            &silence,
            child_id,
            AgentScreenActivity::Ready,
            Some("codex"),
            Some(0),
        )
        .transitioned
    );

    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    assert!(
        try_timer_idle_transition(
            &state,
            &silence,
            child_id,
            AgentScreenActivity::Ready,
            Some("codex"),
            Some(0),
        )
        .transitioned
    );
    assert_eq!(state.agent_inbox.get(parent_id).unwrap().len(), 1);
    assert_eq!(
        state
            .session_states
            .get(child_id)
            .unwrap()
            .background_probe_satisfied_turn_epoch,
        Some(0)
    );

    apply_working_evidence(&state, &silence, child_id, now_epoch_ms(), "working-screen");
    {
        let session = state.session_states.get(child_id).unwrap();
        assert_eq!(session.background_probe_satisfied_turn_epoch, None);
        assert_eq!(session.background_probe_turn_epoch, None);
        assert_eq!(session.background_probe_after_generation, None);
        assert_eq!(session.background_snapshot_generation, 2);
        assert!(!session.background_work);
    }

    silence.lock().ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(
        !try_timer_idle_transition(
            &state,
            &silence,
            child_id,
            AgentScreenActivity::Ready,
            Some("codex"),
            Some(0),
        )
        .transitioned
    );
    {
        let session = state.session_states.get(child_id).unwrap();
        assert_eq!(session.background_probe_turn_epoch, Some(0));
        assert_eq!(session.background_probe_after_generation, Some(2));
        assert_eq!(session.background_probe_satisfied_turn_epoch, None);
    }

    state.process_snapshot_cache.store(Some(vec![
        process(10, 1, "codex", "codex"),
        process(11, 10, "rtk", "rtk cargo test"),
        process(12, 11, "cargo", "cargo test"),
        process(13, 12, "rustc", "rustc --crate-name tuicommander"),
    ]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    let working = state.session_state_with_shell(child_id).unwrap();
    assert_eq!(working.agent_state.as_deref(), Some("working"));
    assert!(working.background_work);

    assert!(
        try_timer_idle_transition(
            &state,
            &silence,
            child_id,
            AgentScreenActivity::Ready,
            Some("codex"),
            Some(0),
        )
        .transitioned
    );
    assert_eq!(state.agent_inbox.get(parent_id).unwrap().len(), 1);

    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    assert_eq!(state.agent_inbox.get(parent_id).unwrap().len(), 2);
    let content: serde_json::Value = serde_json::from_str(
        &state
            .agent_inbox
            .get(parent_id)
            .unwrap()
            .back()
            .unwrap()
            .content,
    )
    .unwrap();
    assert_eq!(content["state"], "idle");

    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    assert!(!refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    assert_eq!(state.agent_inbox.get(parent_id).unwrap().len(), 2);
}

#[test]
fn already_busy_working_evidence_invalidates_only_probe_boundaries() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "background-already-busy";
    agent_session(&state, child_id, SHELL_BUSY);
    {
        let mut session = state.session_states.get_mut(child_id).unwrap();
        session.agent_type = Some("codex".into());
        session.background_work = true;
        session.background_snapshot_generation = 9;
        session.background_probe_turn_epoch = Some(0);
        session.background_probe_after_generation = Some(8);
        session.background_probe_satisfied_turn_epoch = Some(0);
    }
    let silence = state.silence_states.get(child_id).unwrap().clone();

    apply_working_evidence(&state, &silence, child_id, now_epoch_ms(), "working-screen");
    {
        let session = state.session_states.get(child_id).unwrap();
        assert_eq!(session.background_probe_turn_epoch, None);
        assert_eq!(session.background_probe_after_generation, None);
        assert_eq!(session.background_probe_satisfied_turn_epoch, None);
        assert!(session.background_work);
        assert_eq!(session.background_snapshot_generation, 9);
    }

    {
        let mut session = state.session_states.get_mut(child_id).unwrap();
        session.background_probe_turn_epoch = Some(0);
        session.background_probe_after_generation = Some(9);
        session.background_probe_satisfied_turn_epoch = Some(0);
    }
    transition_explicit_shell_state_with_hook(&state, child_id, SHELL_BUSY, "busy", true, || {});
    let session = state.session_states.get(child_id).unwrap();
    assert_eq!(session.background_probe_turn_epoch, None);
    assert_eq!(session.background_probe_after_generation, None);
    assert_eq!(session.background_probe_satisfied_turn_epoch, None);
    assert!(session.background_work);
    assert_eq!(session.background_snapshot_generation, 9);
    assert_eq!(
        state
            .shell_states
            .get(child_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
}

#[test]
fn background_snapshot_child_absent_releases_declared_completion() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "background-ready-completed";
    let parent_id = "background-ready-completed-parent";
    agent_session(&state, child_id, SHELL_BUSY);
    state.session_states.get_mut(child_id).unwrap().agent_type = Some("codex".into());
    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state
        .silence_states
        .get(child_id)
        .unwrap()
        .lock()
        .mark_suggest_candidate(vec!["Review result".to_string()], 0);
    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    state
        .silence_states
        .get(child_id)
        .unwrap()
        .lock()
        .ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);

    assert!(
        !try_timer_idle_transition(
            &state,
            &state.silence_states.get(child_id).unwrap().clone(),
            child_id,
            AgentScreenActivity::Ready,
            Some("codex"),
            Some(0),
        )
        .transitioned
    );
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());

    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    assert!(
        try_timer_idle_transition(
            &state,
            &state.silence_states.get(child_id).unwrap().clone(),
            child_id,
            AgentScreenActivity::Ready,
            Some("codex"),
            Some(0),
        )
        .transitioned
    );
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());
    let silence = state.silence_states.get(child_id).unwrap().clone();
    assert!(emit_pending_suggest_if_idle(&state, &silence, child_id));
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "completed");
}

#[cfg(target_os = "macos")]
#[test]
fn claude_timed_caffeinate_does_not_delay_declared_completion() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "background-claude-caffeinate-completed";
    let parent_id = "background-claude-caffeinate-completed-parent";
    agent_session(&state, child_id, SHELL_BUSY);
    state.session_states.get_mut(child_id).unwrap().agent_type = Some("claude".into());
    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state
        .silence_states
        .get(child_id)
        .unwrap()
        .lock()
        .mark_suggest_candidate(vec!["Review result".to_string()], 0);
    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "claude", "claude")]));

    transition_explicit_shell_state_with_hook(&state, child_id, SHELL_IDLE, "idle", true, || {});
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());

    state.process_snapshot_cache.store(Some(vec![
        process(10, 1, "claude", "claude"),
        process(11, 10, "mdkb", "mdkb mcp"),
        process(12, 10, "tuic-bridge", "tuic-bridge"),
        process(13, 10, "caffeinate", "caffeinate -i -t 300"),
    ]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "claude",
        0,
        state.process_snapshot_cache.load(),
    ));

    let resolved = state.session_state_with_shell(child_id).unwrap();
    assert_eq!(resolved.shell_state.as_deref(), Some("idle"));
    assert_eq!(resolved.agent_state.as_deref(), Some("completed"));
    assert!(!resolved.background_work);
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(inbox.len(), 1);
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "completed");
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
        state.session_states.get_mut(session_id).unwrap().agent_type = Some("codex".into());
        state
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
fn explicit_agent_idle_child_absent_restores_api_state_and_notifies_once() {
    for (session_id, declare_completion, expected_state) in [
        ("background-explicit-idle", false, "idle"),
        ("background-explicit-completed", true, "completed"),
    ] {
        let state = crate::state::tests_support::make_test_app_state();
        let parent_id = format!("{session_id}-parent");
        agent_session(&state, session_id, SHELL_BUSY);
        state.session_states.get_mut(session_id).unwrap().agent_type = Some("codex".into());
        state
            .session_parent
            .insert(session_id.to_string(), parent_id.clone());
        state.agent_inbox.entry(parent_id.clone()).or_default();
        if declare_completion {
            state
                .silence_states
                .get(session_id)
                .unwrap()
                .lock()
                .mark_suggest_candidate(vec!["Review result".to_string()], 0);
        }
        state
            .process_snapshot_cache
            .store(Some(vec![process(10, 1, "codex", "codex")]));

        transition_explicit_shell_state_with_hook(
            &state,
            session_id,
            SHELL_IDLE,
            "idle",
            true,
            || {},
        );
        assert!(state.agent_inbox.get(&parent_id).unwrap().is_empty());
        let pending = state.session_state_with_shell(session_id).unwrap();
        assert_eq!(pending.shell_state.as_deref(), Some("idle"));
        assert_eq!(pending.agent_state.as_deref(), Some("working"));
        assert!(pending.has_pending_background_probe());

        state
            .process_snapshot_cache
            .store(Some(vec![process(10, 1, "codex", "codex")]));
        assert!(refresh_background_work_from_cached_snapshot(
            &state,
            session_id,
            10,
            "codex",
            0,
            state.process_snapshot_cache.load(),
        ));
        let resolved = state.session_state_with_shell(session_id).unwrap();
        assert_eq!(resolved.agent_state.as_deref(), Some(expected_state));
        assert!(!resolved.has_pending_background_probe());
        let inbox = state.agent_inbox.get(&parent_id).unwrap();
        assert_eq!(inbox.len(), 1);
        let content: serde_json::Value =
            serde_json::from_str(&inbox.front().unwrap().content).unwrap();
        assert_eq!(content["state"], expected_state);
        drop(inbox);

        state
            .process_snapshot_cache
            .store(Some(vec![process(10, 1, "codex", "codex")]));
        assert!(!refresh_background_work_from_cached_snapshot(
            &state,
            session_id,
            10,
            "codex",
            0,
            state.process_snapshot_cache.load(),
        ));
        assert_eq!(state.agent_inbox.get(&parent_id).unwrap().len(), 1);
    }
}

#[test]
fn explicit_non_agent_idle_keeps_immediate_shell_semantics() {
    for (session_id, hook_state) in [("plain-hook-idle", true), ("plain-osc133-idle", false)] {
        let state = crate::state::tests_support::make_test_app_state();
        state.session_states.insert(
            session_id.to_string(),
            crate::state::SessionState::default(),
        );
        state.shell_states.insert(
            session_id.to_string(),
            std::sync::atomic::AtomicU8::new(SHELL_BUSY),
        );
        state.silence_states.insert(
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
                .shell_states
                .get(session_id)
                .unwrap()
                .load(Ordering::Acquire),
            SHELL_IDLE
        );
        let session = state.session_states.get(session_id).unwrap();
        assert_eq!(session.background_probe_turn_epoch, None);
        assert_eq!(session.background_probe_after_generation, None);
    }
}

#[test]
fn background_snapshot_refresher_is_demand_gated_without_sleeping() {
    let state = crate::state::tests_support::make_test_app_state();
    let calls = std::sync::atomic::AtomicUsize::new(0);
    assert!(!refresh_process_snapshot_if_demanded(&state, || {
        calls.fetch_add(1, Ordering::Relaxed);
        None
    }));
    assert_eq!(calls.load(Ordering::Relaxed), 0);

    let session_id = "background-demand";
    agent_session(&state, session_id, SHELL_BUSY);
    state.session_states.get_mut(session_id).unwrap().agent_type = Some("codex".into());
    state
        .session_states
        .get_mut(session_id)
        .unwrap()
        .background_probe_turn_epoch = Some(0);
    state
        .session_states
        .get_mut(session_id)
        .unwrap()
        .background_probe_after_generation = Some(0);

    assert!(refresh_process_snapshot_if_demanded(&state, || {
        calls.fetch_add(1, Ordering::Relaxed);
        Some(vec![process(10, 1, "codex", "codex")])
    }));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        session_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    assert!(!process_snapshot_is_demanded(&state));
    assert!(!refresh_process_snapshot_if_demanded(&state, || {
        calls.fetch_add(1, Ordering::Relaxed);
        None
    }));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
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

// --- Screen verification tests ---

#[test]
fn test_verify_question_on_screen_found() {
    let screen = vec![
        String::new(),
        "Some output".to_string(),
        "Do you want to proceed?".to_string(),
        "⏵⏵ task_name".to_string(),
        String::new(),
    ];
    assert!(verify_question_on_screen(
        &screen,
        "Do you want to proceed?",
        5
    ));
}

#[test]
fn test_verify_question_on_screen_ink_indented() {
    // Ink agents indent text with leading whitespace. extract_question_line
    // captures "  Want me to do that?" (with spaces), screen_rows also has
    // the same. Verification must match despite leading whitespace.
    let screen = vec![
        "⏺ Boss, this is a plan file".to_string(),
        "  Is that right?".to_string(),
        String::new(),
        "  Want me to do that?".to_string(),
        String::new(),
    ];
    // Question stored with leading whitespace from extract_question_line
    assert!(verify_question_on_screen(
        &screen,
        "  Want me to do that?",
        5
    ));
    // Also works if question was stored without whitespace
    assert!(verify_question_on_screen(&screen, "Want me to do that?", 5));
}

#[test]
fn test_verify_question_on_screen_scrolled_away() {
    // Question is NOT among the last 5 rows
    let screen: Vec<String> = (0..24).map(|i| format!("line {i}")).collect();
    assert!(!verify_question_on_screen(
        &screen,
        "Do you want to proceed?",
        5
    ));
}

#[test]
fn test_verify_question_on_screen_empty() {
    let screen: Vec<String> = vec![];
    assert!(!verify_question_on_screen(&screen, "Continue?", 5));
}

#[test]
fn test_verify_question_on_screen_partial_match() {
    let screen = vec![
        "This is not a question? but has more text".to_string(),
        String::new(),
    ];
    // The stored question is just "question?" — substring should not match
    assert!(!verify_question_on_screen(&screen, "question?", 5));
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
fn current_chat_question_distinguishes_history_from_missing_prompt_anchor() {
    let rows = vec![
        "Confermi questa rimozione?".to_string(),
        "› si".to_string(),
        "Removed the worktree successfully.".to_string(),
        "› ".to_string(),
    ];
    assert_eq!(
        current_chat_question(&rows),
        CurrentChatQuestion::PromptAnchored(None),
        "later answer and completion must make the old question historical"
    );
    assert_eq!(
        current_chat_question(&["Confermi questa rimozione?".to_string()]),
        CurrentChatQuestion::NoPromptAnchor,
        "headless/incomplete rendering may still use the bounded fallback"
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
fn test_silence_state_different_question_after_emitted_does_fire() {
    let mut s = SilenceState::new();
    // First question fires
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(s.check_silence().is_some());

    // Different question arrives — this IS a new question, must fire
    s.on_chunk(
        false,
        Some("Are you sure?".to_string()),
        false,
        false,
        false,
    );
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(s.check_silence(), Some("Are you sure?".to_string()));
}

// --- find_last_chat_question tests ---

fn screen(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|s| s.to_string()).collect()
}

#[test]
fn test_find_last_chat_question_basic() {
    let rows = screen(&[
        "Do you want to proceed?",
        "",
        "────────────────────────────────",
        "> ",
        "────────────────────────────────",
        "⏵⏵ bypass permissions on",
    ]);
    assert_eq!(
        find_last_chat_question(&rows),
        Some("Do you want to proceed?".to_string()),
    );
}

#[test]
fn test_find_last_chat_question_trailing_disclaimer_blocks_detection() {
    // When the agent emits trailing text AFTER the suggest block (e.g.
    // Claude Code's "(stopping here — waiting for your answer)" footer),
    // the last chat line is the disclaimer, not the question. We
    // deliberately do NOT scavenge past it — accepting this edge case
    // false negative in exchange for not crossing the agent-turn boundary
    // and matching the user's own previous `?`-ending input.
    let rows = screen(&[
        "⏺ TUICommander v1.0.2 is connected.",
        "  intent: await handshake then relay fixed response (Await ACK)",
        "  Do you want me to proceed with this fix?",
        "  suggest: 1) Screenshot overview panel | 2) Fix suggest scroll flicker | 3)",
        "   Fix Cmd+Shift+M keybinding collision | 4) Manual test OSC 133",
        "  (stopping here — waiting for your answer)",
        "────────",
        "❯ ",
        "────────",
        "  [Opus 4.6 | Max]",
        "  ⏵⏵ bypass permissions on",
    ]);
    assert_eq!(find_last_chat_question(&rows), None);
}

#[test]
fn test_find_last_chat_question_does_not_cross_previous_input() {
    // The user previously typed `tutto ok?` (ending with a `?`), the agent
    // replied with a plain statement, then arrives at an empty prompt.
    // The walker MUST NOT scavenge past the agent statement to pick up
    // the user's own prior input — doing so fires a phantom question
    // notification 10s after the reply.
    let rows = screen(&[
        "❯ tutto ok?",
        "────────",
        "⏺ Sì, tutto funziona correttamente.",
        "  Il fix è stato verificato.",
        "────────",
        "❯ ",
        "────────",
        "  ⏵⏵ bypass permissions on",
    ]);
    assert_eq!(find_last_chat_question(&rows), None);
}

#[test]
fn test_find_last_chat_question_skips_wrapped_suggest_block() {
    // Wrapped suggest between question and prompt must not block detection.
    let rows = screen(&[
        "Should I implement this approach?",
        "suggest: 1) Opzione A | 2) Opzione B | 3) Opzione molto lunga che continua",
        "su una seconda riga | 4) Quarta opzione",
        "────────────────────────────────",
        "> ",
        "────────────────────────────────",
        "⏵⏵ bypass permissions on",
    ]);
    assert_eq!(
        find_last_chat_question(&rows),
        Some("Should I implement this approach?".to_string()),
    );
}

#[test]
fn test_find_last_chat_question_no_question() {
    // Agent statement (not a question) above prompt → None.
    let rows = screen(&[
        "I have completed the refactor.",
        "",
        "────────────────────────────────",
        "> ",
        "────────────────────────────────",
        "⏵⏵ bypass permissions on",
    ]);
    assert_eq!(find_last_chat_question(&rows), None);
}

#[test]
fn test_find_last_chat_question_only_checks_first_chat_line() {
    // With multiple chat lines above the prompt, only the immediately
    // preceding one is considered — even if an older line ends with `?`.
    let rows = screen(&[
        "Old question from earlier?",
        "Here is some context.",
        "Do you agree with this plan?",
        "",
        "────────────────────────────────",
        "> ",
        "────────────────────────────────",
        "⏵⏵ bypass permissions on",
    ]);
    // Last chat line is the empty line (skipped), then "Do you agree…?" → detected
    assert_eq!(
        find_last_chat_question(&rows),
        Some("Do you agree with this plan?".to_string()),
    );
}

#[test]
fn test_find_last_chat_question_non_question_last_line_blocks() {
    // If the last chat line above the prompt is not a question, we do NOT
    // keep walking upward to find an older question.
    let rows = screen(&[
        "Shall I proceed?",
        "Here is some unrelated follow-up text.",
        "────────────────────────────────",
        "> ",
        "────────────────────────────────",
        "⏵⏵ bypass permissions on",
    ]);
    assert_eq!(find_last_chat_question(&rows), None);
}

#[test]
fn test_find_last_chat_question_rejects_code_syntax() {
    // `?` in code syntax must not be treated as a question.
    let rows = screen(&[
        "let x = map.get(&key)?",
        "",
        "────────────────────────────────",
        "> ",
        "────────────────────────────────",
        "⏵⏵ bypass permissions on",
    ]);
    assert_eq!(find_last_chat_question(&rows), None);
}

#[test]
fn test_find_last_chat_question_codex_layout() {
    // Codex has no separator lines — the walk must still find the question.
    let rows = screen(&[
        "Do you want me to proceed?",
        "",
        "› ",
        "",
        "  gpt-5.3-codex high · 100% left · ~/project",
    ]);
    assert_eq!(
        find_last_chat_question(&rows),
        Some("Do you want me to proceed?".to_string()),
    );
}

// --- extract_question_line content filter tests ---

fn make_rows(texts: &[&str]) -> Vec<ChangedRow> {
    texts
        .iter()
        .enumerate()
        .map(|(i, t)| ChangedRow {
            row_index: i,
            text: t.to_string(),
        })
        .collect()
}

#[test]
fn test_extract_question_line_rejects_code_comment() {
    let rows = make_rows(&["// What is this?"]);
    assert_eq!(extract_question_line(&rows), None);
}

#[test]
fn test_extract_question_line_rejects_markdown_header() {
    let rows = make_rows(&["## FAQ?"]);
    assert_eq!(extract_question_line(&rows), None);
}

#[test]
fn test_extract_question_line_rejects_diff_context() {
    assert_eq!(extract_question_line(&make_rows(&["+  if x?"])), None);
    assert_eq!(extract_question_line(&make_rows(&["-  if x?"])), None);
    assert_eq!(extract_question_line(&make_rows(&[">  quoted?"])), None);
}

#[test]
fn test_extract_question_line_rejects_numbered_diff_context() {
    assert_eq!(
        extract_question_line(&make_rows(&["1 +Run the following release checklist?"])),
        None,
        "a numbered diff row must not seed awaiting-question silence detection"
    );
}

#[test]
fn test_extract_question_line_rejects_code_syntax() {
    assert_eq!(
        extract_question_line(&make_rows(&["fn foo() -> Option<bool>?"])),
        None
    );
    assert_eq!(
        extract_question_line(&make_rows(&["map.entry(key)?"])),
        None
    );
    assert_eq!(extract_question_line(&make_rows(&["let x = a::b?"])), None);
}

#[test]
fn test_extract_question_line_accepts_real_question() {
    let rows = make_rows(&["Do you want to proceed?"]);
    assert_eq!(
        extract_question_line(&rows),
        Some("Do you want to proceed?".to_string())
    );
}

#[test]
fn test_extract_question_line_accepts_yn_prompt() {
    // Y/n prompt ends with `]`, not `?` — extract_question_line only matches `?`-ending.
    // The actual question before the Y/n suffix ends with `?`:
    let rows = make_rows(&["Continue?"]);
    assert_eq!(extract_question_line(&rows), Some("Continue?".to_string()));
}

#[test]
fn test_extract_question_line_accepts_short_natural_question() {
    // Boss confirmed: "continuo?" is a valid question
    let rows = make_rows(&["continuo?"]);
    assert_eq!(extract_question_line(&rows), Some("continuo?".to_string()));
}

#[test]
fn test_extract_question_line_rejects_asterisk_comment() {
    let rows = make_rows(&["* What is this?"]);
    assert_eq!(extract_question_line(&rows), None);
}

#[test]
fn test_extract_question_line_accepts_parenthetical_options() {
    let rows = make_rows(&["Continue (yes/no)?"]);
    assert_eq!(
        extract_question_line(&rows),
        Some("Continue (yes/no)?".to_string())
    );
}

#[test]
fn test_extract_question_line_accepts_yn_parens() {
    let rows = make_rows(&["Procedo (s/n)?"]);
    assert_eq!(
        extract_question_line(&rows),
        Some("Procedo (s/n)?".to_string())
    );
}

#[test]
fn test_extract_question_line_accepts_option_prompt() {
    let rows = make_rows(&["Apply changes (y)?"]);
    assert_eq!(
        extract_question_line(&rows),
        Some("Apply changes (y)?".to_string())
    );
}

#[test]
fn test_extract_question_line_rejects_rust_try() {
    assert_eq!(extract_question_line(&make_rows(&["foo.bar()?"])), None);
}

#[test]
fn test_extract_question_line_rejects_generic_try() {
    // Also caught by `::` filter
    assert_eq!(extract_question_line(&make_rows(&["Vec::new()?"])), None);
}

#[test]
fn test_extract_question_line_rejects_method_chain_try() {
    assert_eq!(
        extract_question_line(&make_rows(&["iter().map(|x| x)?"])),
        None
    );
}

// --- Prompt-prefixed user input rejection ---

#[test]
fn test_extract_question_line_rejects_claude_prompt() {
    assert_eq!(extract_question_line(&make_rows(&["❯ tutto ok?"])), None);
}

#[test]
fn test_extract_question_line_rejects_codex_prompt() {
    assert_eq!(
        extract_question_line(&make_rows(&["› is this done?"])),
        None
    );
}

#[test]
fn test_extract_question_line_rejects_gemini_prompt() {
    assert_eq!(
        extract_question_line(&make_rows(&["> are you sure?"])),
        None
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

// --- Resize grace period tests ---

#[test]
fn test_resize_grace_active_immediately_after_resize() {
    let mut s = SilenceState::new();
    s.on_resize();
    assert!(
        s.is_resize_grace(),
        "grace period should be active right after resize"
    );
}

#[test]
fn test_resize_grace_inactive_before_resize() {
    let s = SilenceState::new();
    assert!(
        !s.is_resize_grace(),
        "grace period should be inactive with no resize"
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

/// The grace re-arm reads "the durable log did not grow" as "this chunk was a
/// SIGWINCH repaint". In the ALTERNATE screen that reading is always wrong:
/// `VtLogBuffer::process` skips log capture entirely while alt is active, so
/// `total_lines()` is frozen no matter how much the agent writes. Every chunk
/// therefore re-armed the grace, and one resize suppressed low-confidence
/// questions, rate-limit and API-error events plus the BUSY transition until
/// the agent went quiet for a full second.
///
/// The probe reads `last_resize_at` directly instead of sleeping out the
/// window: the re-arm IS that assignment, so whether the stamp moved is the
/// behaviour, not a proxy for it.
///
/// This latch cannot become a `.tcap` fixture. A capture records PTY output
/// and user input bytes, and `replay_capture` drives `VtLogBuffer::process` +
/// `raw_stream_events` + `parse_clean_lines` with no `SilenceState` at all —
/// but the trigger here is `resize_pty` calling `on_resize()`, which is
/// out-of-band and appears nowhere in the byte stream. The grace is only
/// reachable through `process_chunk`, so that is where the test has to sit.
#[test]
fn resize_grace_re_arms_only_on_a_repaint_never_on_agent_output() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    /// Feeds `prelude` then `chunk` through the real `process_chunk` with the
    /// grace armed and 200 ms left to run, and reports whether `chunk`
    /// pushed the grace deadline forward.
    fn re_armed_by(sid: &str, prelude: &str, chunk: &str) -> bool {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let silence = Arc::new(Mutex::new(SilenceState::new()));
        state
            .silence_states
            .insert(sid.to_string(), silence.clone());
        state.shell_states.insert(
            sid.to_string(),
            std::sync::atomic::AtomicU8::new(SHELL_NULL),
        );
        // Six rows: a screenful plus one line is enough to scroll and grow
        // the durable log, which is what "real output" means in primary.
        state
            .vt_log_buffers
            .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(6, 40, 1000)));
        state
            .output_buffers
            .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
        state
            .last_output_ms
            .insert(sid.to_string(), AtomicU64::new(0));

        let mut cp = ChunkProcessor::new(None, None);
        cp.process_chunk(prelude, &silence, sid, &state);

        // Arm the grace with 200 ms left: a re-arm is visible as a moved
        // stamp, and no sleep is needed to tell the two apart.
        let armed_at =
            std::time::Instant::now() - RESIZE_GRACE + std::time::Duration::from_millis(200);
        silence.lock().last_resize_at = Some(armed_at);
        assert!(
            silence.lock().is_resize_grace(),
            "precondition: the grace must still be running when the chunk lands"
        );

        cp.process_chunk(chunk, &silence, sid, &state);
        silence.lock().last_resize_at != Some(armed_at)
    }

    // Primary screen, pure repaint: no new line scrolled in, so this is the
    // post-SIGWINCH reflow the extension exists for. It must still re-arm.
    assert!(
        re_armed_by(
            "grace-primary-repaint",
            "one\r\ntwo\r\n",
            "\x1b[H\x1b[2Kone"
        ),
        "a primary-screen repaint must still extend the grace"
    );

    // Primary screen, real output: eight lines on a six-row screen scroll the
    // oldest into the durable log. Genuine work must end the extension.
    assert!(
        !re_armed_by(
            "grace-primary-growth",
            "boot\r\n",
            "l1\r\nl2\r\nl3\r\nl4\r\nl5\r\nl6\r\nl7\r\nl8\r\n"
        ),
        "growing output must not extend the grace"
    );

    // Alternate screen, the same real output. The log cannot grow here, so
    // the unfixed check calls it a repaint and latches the grace forever.
    assert!(
        !re_armed_by(
            "grace-alt-output",
            "\x1b[?1049h",
            "l1\r\nl2\r\nl3\r\nl4\r\nl5\r\nl6\r\nl7\r\nl8\r\n"
        ),
        "alternate-screen output must not extend the grace — the durable log \
             is frozen there, so a frozen total is not evidence of a repaint"
    );
}

// --- Startup grace period tests ---

#[test]
fn test_startup_grace_active_on_new_session() {
    let s = SilenceState::new();
    assert!(
        s.is_startup_grace(),
        "startup grace should be active on new session"
    );
}

#[test]
fn test_startup_grace_settles_after_silence() {
    let mut s = SilenceState::new();
    // Simulate output stopping long enough ago
    s.last_output_at =
        std::time::Instant::now() - STARTUP_SETTLE_SILENCE - std::time::Duration::from_millis(100);
    s.check_startup_settle();
    assert!(
        !s.is_startup_grace(),
        "startup grace should end after output silence"
    );
}

#[test]
fn test_startup_grace_persists_during_output() {
    let mut s = SilenceState::new();
    // Output is recent — grace should persist
    s.last_output_at = std::time::Instant::now();
    s.check_startup_settle();
    assert!(
        s.is_startup_grace(),
        "startup grace should persist while output is flowing"
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

// --- VtLogBuffer + parse_clean_lines pipeline tests ---

/// VtLogBuffer changed rows feed parse_clean_lines and produce a StatusLine event
/// for normal screen output.
#[test]
fn test_vt_log_pipeline_status_line_normal_screen() {
    use crate::output_parser::{OutputParser, ParsedEvent};
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let mut parser = OutputParser::new();

    let changed = vt_log.process(b"* Reading files...");
    let events = parser.parse_clean_lines(&changed, true);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
        "expected StatusLine from normal screen, got: {:?}",
        events
    );
}

/// VtLogBuffer changed rows feed parse_clean_lines and produce an Intent event
/// during alternate screen (e.g. Claude Code / Ink).
#[test]
fn test_vt_log_pipeline_intent_alternate_screen() {
    use crate::output_parser::{OutputParser, ParsedEvent};
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let mut parser = OutputParser::new();

    // Enter alternate screen (smcup: ESC[?1049h)
    let _ = vt_log.process(b"\x1b[?1049h");
    let changed = vt_log.process(b"intent: Doing work (Test)");
    let events = parser.parse_clean_lines(&changed, true);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ParsedEvent::Intent { .. })),
        "expected Intent from alternate screen, got: {:?}",
        events
    );
}

/// parse_osc94 is called on raw data (OSC 9;4 is invisible in clean rows).
#[test]
fn test_osc94_from_raw_stream() {
    use crate::output_parser::{ParsedEvent, parse_osc94};

    let raw = "\x1b]9;4;1;50\x07"; // OSC 9;4 progress 50%
    let event = parse_osc94(raw);
    assert!(
        matches!(event, Some(ParsedEvent::Progress { .. })),
        "expected Progress from raw OSC 9;4, got: {:?}",
        event
    );
}

/// extract_question_line finds `?`-ending rows from VtLogBuffer output.
#[test]
fn test_extract_question_line_basic() {
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let changed = vt_log.process(b"Would you like to proceed?");
    assert_eq!(
        extract_question_line(&changed).as_deref(),
        Some("Would you like to proceed?")
    );
}

/// Question row must be found even when a mode line with a higher row index
/// arrives in the same chunk (e.g. Claude Code question + ⏵⏵ status line).
#[test]
fn test_extract_question_line_with_mode_line_same_chunk() {
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let data = b"Le committo?\r\n\r\n\xe2\x8f\xb5\xe2\x8f\xb5 Reading files";
    let changed = vt_log.process(data);
    assert_eq!(
        extract_question_line(&changed).as_deref(),
        Some("Le committo?"),
        "question must be found even when mode line is on a later row; changed_rows: {:?}",
        changed
            .iter()
            .map(|r| format!("[{}] {:?}", r.row_index, r.text))
            .collect::<Vec<_>>()
    );
}

/// Question must be found in alternate screen with cursor-positioned rows.
#[test]
fn test_extract_question_line_alternate_screen() {
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let _ = vt_log.process(b"\x1b[?1049h");
    let data = b"\x1b[5;1HDo you want to proceed?\x1b[23;1H* Thinking...";
    let changed = vt_log.process(data);
    assert_eq!(
        extract_question_line(&changed).as_deref(),
        Some("Do you want to proceed?"),
        "question must be found in alternate screen; changed_rows: {:?}",
        changed
            .iter()
            .map(|r| format!("[{}] {:?}", r.row_index, r.text))
            .collect::<Vec<_>>()
    );
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

// --- Headless reader structured event tests ---

/// The headless reader logic: after process(), parse_clean_lines produces events.
/// This verifies the core data flow without spawning a full AppState.
#[test]
fn test_headless_reader_intent_event_logic() {
    use crate::output_parser::{OutputParser, ParsedEvent};
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let mut parser = OutputParser::new();

    let changed = vt_log.process(b"intent: Testing headless reader");
    let events = parser.parse_clean_lines(&changed, true);

    assert!(
        events
            .iter()
            .any(|e| matches!(e, ParsedEvent::Intent { .. })),
        "expected Intent from headless reader logic, got: {:?}",
        events
    );
}

/// The headless reader emits events for alternate screen content (e.g. Claude Code).
#[test]
fn test_headless_reader_alternate_screen_events() {
    use crate::output_parser::{OutputParser, ParsedEvent};
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let mut parser = OutputParser::new();

    let _ = vt_log.process(b"\x1b[?1049h"); // enter alternate screen
    let changed = vt_log.process(b"* Reading files...");
    let events = parser.parse_clean_lines(&changed, true);

    assert!(
        events
            .iter()
            .any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
        "headless reader must detect StatusLine during alternate screen, got: {:?}",
        events
    );
}

// --- Escape sequence handling diagnostics (using TerminalGrid) ---

/// Verify that `\x1b[<n>F` (CPL — Cursor Previous Line) is handled
/// and does NOT leak parameter digits into screen cell text.
#[test]
fn test_cpl_sequence_does_not_leak() {
    let mut grid = crate::terminal_grid::TerminalGrid::new(24, 80, 0);
    grid.process(b"\n");
    grid.process(b"old content here\n");
    grid.process(b"\x1b[1F");
    grid.process(b"new content");
    let row1 = grid.get_row_text(1);
    assert_eq!(
        row1.trim_end(),
        "new content here",
        "CPL should move cursor up; row1 = {:?}",
        row1
    );
    assert!(
        !row1.contains("1F"),
        "escape param '1F' leaked into screen text: {:?}",
        row1
    );
}

/// Verify that `\x1b[<n>E` (CNL — Cursor Next Line) is handled.
#[test]
fn test_cnl_sequence_does_not_leak() {
    let mut grid = crate::terminal_grid::TerminalGrid::new(24, 80, 0);
    grid.process(b"line0");
    grid.process(b"\x1b[1E");
    grid.process(b"line1");
    let row0 = grid.get_row_text(0);
    let row1 = grid.get_row_text(1);
    assert_eq!(
        row0.trim_end(),
        "line0",
        "row0 should be unchanged; got {:?}",
        row0
    );
    assert_eq!(
        row1.trim_end(),
        "line1",
        "CNL should move cursor down; got {:?}",
        row1
    );
}

/// Simulate Ink-style rendering: write intent, then use CPL to update it.
/// This is what Claude Code does when updating its status line.
#[test]
fn test_vt100_ink_style_intent_with_cpl() {
    use crate::output_parser::{OutputParser, ParsedEvent};
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let mut parser = OutputParser::new();

    // Simulate Ink render: write placeholder, then CPL + overwrite with intent
    let _ = vt_log.process(b"\x1b[?1049h"); // alternate screen
    let _ = vt_log.process(b"placeholder text\r\n");
    // Ink update: go up, clear line, write intent
    let changed =
        vt_log.process(b"\x1b[1F\x1b[2Kintent: Fix all 34 documentation gaps (Fixing gaps)");
    let events = parser.parse_clean_lines(&changed, true);
    let intent = events.iter().find_map(|e| match e {
        ParsedEvent::Intent { text, title, .. } => Some((text.clone(), title.clone())),
        _ => None,
    });
    assert!(
        intent.is_some(),
        "intent must be detected after CPL overwrite; changed={:?}, events={:?}",
        changed,
        events
    );
    let (text, title) = intent.unwrap();
    assert_eq!(
        text, "Fix all 34 documentation gaps",
        "intent text must be clean (no '1F' leak); got: {:?}",
        text
    );
    assert_eq!(title.as_deref(), Some("Fixing gaps"));
}

/// Chunked delivery: CSI split across two process() calls.
/// Verifies the vt100 parser buffers incomplete escapes correctly.
#[test]
fn test_vt100_chunked_csi_does_not_leak() {
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let _ = vt_log.process(b"\x1b[?1049h"); // alternate screen
    let _ = vt_log.process(b"old line\r\n");

    // Chunk 1: partial CSI (just the introducer)
    let changed1 = vt_log.process(b"\x1b[");
    // Chunk 2: parameter + final byte completing CPL, then text
    let changed2 = vt_log.process(b"1Fintent: Fix all gaps");

    // Check that no row contains literal "1F" as text
    for row in changed1.iter().chain(changed2.iter()) {
        assert!(
            !row.text.contains("1F"),
            "chunked CSI leaked '1F' into row text: {:?}",
            row.text
        );
    }
}

/// Test what happens when CSI is aborted by an unexpected byte.
#[test]
fn test_aborted_csi_does_not_leak_digits() {
    let mut grid = crate::terminal_grid::TerminalGrid::new(24, 80, 0);
    // \x1b[1\x1b[2K — the first CSI is aborted by the second ESC
    grid.process(b"\x1b[1\x1b[2KHello");
    let row = grid.get_row_text(0);
    eprintln!("aborted CSI row: {:?}", row);
    assert!(
        !row.starts_with('1'),
        "aborted CSI parameter '1' should not appear in cell text: {:?}",
        row
    );
}

/// Test that unknown private CSI sequences don't leak.
#[test]
fn test_unknown_private_csi_does_not_leak() {
    let mut grid = crate::terminal_grid::TerminalGrid::new(24, 80, 0);
    // \x1b[?1234z — fictional private sequence with unknown final byte 'z'
    grid.process(b"\x1b[?1234zVisible text");
    let row = grid.get_row_text(0);
    eprintln!("unknown private CSI row: {:?}", row);
    assert_eq!(
        row.trim_end(),
        "Visible text",
        "unknown private CSI should not leak; got: {:?}",
        row
    );
}

/// Simulate realistic Ink output with SGR + cursor movement + text.
/// This mimics what Claude Code actually sends through the PTY.
#[test]
fn test_vt100_realistic_ink_render_cycle() {
    use crate::output_parser::{OutputParser, ParsedEvent};
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let mut parser = OutputParser::new();

    let _ = vt_log.process(b"\x1b[?1049h"); // alternate screen

    // Frame 1: Ink renders initial content with colors
    let _ = vt_log.process(
        b"\x1b[1;1H\x1b[38;2;128;128;128m\xe2\x97\x8f\x1b[0m \x1b[1mintent: Reading codebase structure (Reading code)\x1b[0m"
    );

    // Frame 2: Ink updates — cursor up, erase line, rewrite
    // This is how Ink typically does incremental updates
    let changed = vt_log.process(
        b"\x1b[1F\x1b[2K\x1b[38;2;128;128;128m\xe2\x97\x8f\x1b[0m \x1b[1mintent: Fix all 34 documentation gaps (Fixing gaps)\x1b[0m"
    );

    let events = parser.parse_clean_lines(&changed, true);
    let intent = events.iter().find_map(|e| match e {
        ParsedEvent::Intent { text, title, .. } => Some((text.clone(), title.clone())),
        _ => None,
    });

    // Print all changed rows for debugging
    eprintln!("changed rows:");
    for r in &changed {
        eprintln!("  row[{}]: {:?}", r.row_index, r.text);
    }
    eprintln!("events: {:?}", events);

    assert!(
        intent.is_some(),
        "intent must be detected in realistic Ink render; events={:?}",
        events
    );
    let (text, title) = intent.unwrap();
    assert!(
        !text.contains("1F"),
        "intent text must not contain escape leak '1F'; got: {:?}",
        text
    );
    assert_eq!(text, "Fix all 34 documentation gaps");
    assert_eq!(title.as_deref(), Some("Fixing gaps"));
}

/// Multi-chunk Ink render: data arrives in small fragments.
#[test]
fn test_vt100_fragmented_ink_output() {
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let _ = vt_log.process(b"\x1b[?1049h");

    // Simulate fragmented delivery of: \x1b[1F\x1b[2Kintent: Fix all gaps
    let fragments: Vec<&[u8]> = vec![
        b"\x1b[", // CSI introducer
        b"1",     // parameter
        b"F",     // final byte (CPL)
        b"\x1b[", // CSI introducer
        b"2K",    // erase line
        b"intent: Fix all gaps",
    ];

    let mut all_changed = Vec::new();
    for frag in fragments {
        let changed = vt_log.process(frag);
        all_changed.extend(changed);
    }

    // Check no row contains '1F' leak
    for row in &all_changed {
        eprintln!("fragmented row[{}]: {:?}", row.row_index, row.text);
        assert!(
            !row.text.contains("1F"),
            "fragmented delivery leaked '1F': {:?}",
            row.text
        );
    }
}

// --- Shell state transition tests ---

#[test]
fn test_shell_state_busy_on_real_output() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_NULL));
    state.last_output_ms.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU64::new(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        ),
    );

    // Transition null → busy
    assert!(
        try_shell_transition(&state, sid, SHELL_NULL, SHELL_BUSY, true),
        "should transition null → busy"
    );
    assert_eq!(
        state.shell_states.get(sid).unwrap().load(Ordering::Relaxed),
        SHELL_BUSY
    );

    // Transition busy → busy should fail (already busy, no re-emit)
    assert!(
        !try_shell_transition(&state, sid, SHELL_NULL, SHELL_BUSY, true),
        "should NOT re-transition to busy"
    );
}

#[test]
fn test_shell_state_idle_after_500ms() {
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());

    // Set last output to 600ms ago (> SHELL_IDLE_MS)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 600));

    assert!(
        should_transition_idle(&state, sid).should_transition,
        "should be ready to transition idle (600ms elapsed, no sub-tasks)"
    );
    assert!(
        try_shell_transition(&state, sid, SHELL_BUSY, SHELL_IDLE, true),
        "should transition busy → idle"
    );
    assert_eq!(
        state.shell_states.get(sid).unwrap().load(Ordering::Relaxed),
        SHELL_IDLE
    );
}

#[test]
fn test_shell_state_no_idle_with_subtasks() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

    state.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            active_sub_tasks: 2,
            ..Default::default()
        },
    );

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 600));

    assert!(
        !should_transition_idle(&state, sid).should_transition,
        "should NOT transition idle when active_sub_tasks > 0 and elapsed < SUBTASK_STALE_MS"
    );
}

#[test]
fn test_shell_state_idle_stale_subtasks_force_cleared() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

    state.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            active_sub_tasks: 2,
            ..Default::default()
        },
    );

    // Set last output to 31s ago (> SUBTASK_STALE_MS)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 31_000));

    assert!(
        should_transition_idle(&state, sid).should_transition,
        "should transition idle when active_sub_tasks > 0 but elapsed >= SUBTASK_STALE_MS"
    );
    // Verify the stale counter was force-cleared
    let sub = state
        .session_states
        .get(sid)
        .map(|s| s.active_sub_tasks)
        .unwrap_or(999);
    assert_eq!(sub, 0, "active_sub_tasks should be force-cleared to 0");
}

// ---- Activity pulse (story 625-56b0) ----
//
// What these protect: commit cda39f31 deleted the `pty-output` emit and left
// the frontend listener subscribed to it, so desktop `lastDataAt` and the
// background-tab unread flag silently froze for a commit. Nothing failed,
// because no test tied a producer to a consumer.
//
// LIMIT, stated rather than papered over: these drive `ActivityPulse`
// directly. They prove the pulse throttles and reaches the bus, and the
// frontend suite (`transport.test.ts`) proves both transports route the
// event to `onActivity`. Neither proves the PTY reader loop still CALLS
// `pulse()` — that needs a live PTY, and this crate has no harness that
// spawns one. Deleting the call site would still pass; deleting or renaming
// either end of the signal would not.

/// A session that produces output must announce it on the bus.
#[test]
fn activity_pulse_emits_on_first_output() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut rx = state.event_bus.subscribe();
    let mut pulse = ActivityPulse::new();

    pulse.pulse(&state, "sess-a");

    match rx.try_recv().expect("bus must receive the activity pulse") {
        crate::state::AppEvent::PtyActivity { session_id } => {
            assert_eq!(session_id, "sess-a");
        }
        other => panic!("unexpected event variant: {other:?}"),
    }
}

/// Repeated output inside the window collapses to one pulse. Dropping is the
/// intended behaviour here — the signal is payload-free and idempotent, so a
/// suppressed pulse carries nothing a later one does not.
#[test]
fn activity_pulse_suppresses_repeats_inside_window() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut rx = state.event_bus.subscribe();
    let mut pulse = ActivityPulse::new();

    for _ in 0..50 {
        pulse.pulse(&state, "sess-a");
    }

    assert!(
        matches!(
            rx.try_recv(),
            Ok(crate::state::AppEvent::PtyActivity { .. })
        ),
        "first pulse must go out"
    );
    assert!(
        rx.try_recv().is_err(),
        "a burst inside one window must collapse to a single pulse"
    );
}

/// ...but the session must not go quiet forever: once the window has passed,
/// the next chunk pulses again. A latch here would freeze `lastDataAt` at the
/// first byte of a long-running command, which is the bug in a new costume.
#[test]
fn activity_pulse_resumes_after_window() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut rx = state.event_bus.subscribe();
    let mut pulse = ActivityPulse::new();

    pulse.pulse(&state, "sess-a");
    let _ = rx.try_recv();
    // Reach back past the window instead of sleeping through it.
    pulse.last = Some(std::time::Instant::now() - ACTIVITY_PULSE_WINDOW);
    pulse.pulse(&state, "sess-a");

    assert!(
        matches!(
            rx.try_recv(),
            Ok(crate::state::AppEvent::PtyActivity { .. })
        ),
        "a chunk after the window must pulse again"
    );
}

// The matching guarantee — that the pulse must NOT restamp
// `SessionState.last_activity_ms` — is asserted in `state.rs`, next to the
// accumulator that owns that field.

/// Story 1366-2b3e/H1: when the stale-subtasks recovery path force-clears
/// the in-memory counter, the caller must emit ActiveSubtasks{count:0}
/// so the frontend store and notification gate also reset.
#[test]
fn test_force_cleared_subtasks_signal_propagates() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            active_sub_tasks: 3,
            ..Default::default()
        },
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 31_000));

    let decision = should_transition_idle(&state, sid);
    assert!(
        decision.should_transition,
        "stale path must transition idle"
    );
    assert!(
        decision.force_cleared_subtasks,
        "stale path must signal force-clear so caller emits count=0"
    );

    // Subscribe BEFORE emitting so the broadcast is captured.
    let mut rx = state.event_bus.subscribe();
    emit_active_subtasks(&state, sid, 0, "");

    let event = rx.try_recv().expect("event bus must receive PtyParsed");
    match event {
        crate::state::AppEvent::PtyParsed { session_id, parsed } => {
            assert_eq!(session_id, sid);
            let kind = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("");
            assert_eq!(kind, "active-subtasks", "wrong event variant: {parsed}");
            let count = parsed.get("count").and_then(|v| v.as_u64()).unwrap_or(999);
            assert_eq!(count, 0, "count must be 0 to clear the badge");
        }
        other => panic!("unexpected event variant: {other:?}"),
    }
}

/// Inverse: the normal idle path (no sub-tasks at all) must NOT signal
/// force_cleared_subtasks — otherwise we would emit redundant count=0
/// events on every healthy busy→idle.
#[test]
fn test_normal_idle_does_not_signal_force_clear() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 600));

    let decision = should_transition_idle(&state, sid);
    assert!(decision.should_transition);
    assert!(
        !decision.force_cleared_subtasks,
        "no-sub-tasks idle must not request a redundant count=0 emission"
    );
}

#[test]
fn test_shell_state_no_idle_agent_session_under_agent_threshold() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

    // Agent session: agent_type is set
    state.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            ..Default::default()
        },
    );

    // 600ms elapsed — would trigger idle for a shell, but NOT for an agent session
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 600));

    assert!(
        !should_transition_idle(&state, sid).should_transition,
        "agent session should NOT transition idle at 600ms (under AGENT_IDLE_MS)"
    );
}

#[test]
fn test_shell_state_idle_agent_session_over_agent_threshold() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

    // Agent session: agent_type is set
    state.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            ..Default::default()
        },
    );

    // 3000ms elapsed — over the 2500ms agent threshold
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 3000));

    assert!(
        should_transition_idle(&state, sid).should_transition,
        "agent session SHOULD transition idle after agent threshold"
    );
}

#[test]
fn test_shell_state_no_idle_before_500ms() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 200));

    assert!(
        !should_transition_idle(&state, sid).should_transition,
        "should NOT transition idle when only 200ms elapsed"
    );
}

#[test]
fn test_shell_state_cas_prevents_duplicate_idle() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

    // First CAS succeeds
    assert!(try_shell_transition(
        &state, sid, SHELL_BUSY, SHELL_IDLE, true
    ));
    // Second CAS fails (already idle)
    assert!(
        !try_shell_transition(&state, sid, SHELL_BUSY, SHELL_IDLE, true),
        "second idle transition must fail — already idle"
    );
    assert_eq!(
        state.shell_states.get(sid).unwrap().load(Ordering::Relaxed),
        SHELL_IDLE
    );
}

#[test]
fn test_shell_state_idle_to_busy_on_real_output() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_IDLE));

    assert!(
        try_shell_transition(&state, sid, SHELL_IDLE, SHELL_BUSY, true),
        "should transition idle → busy on real output"
    );
    assert_eq!(
        state.shell_states.get(sid).unwrap().load(Ordering::Relaxed),
        SHELL_BUSY
    );
}

// --- Backup idle guard: has_recent_chunks ---

#[test]
fn test_has_recent_chunks_true_after_any_chunk() {
    let mut s = SilenceState::new();
    // Any chunk (including chrome-only) updates last_chunk_at
    s.on_chunk(false, None, true, true, false);
    assert!(
        s.has_recent_chunks(),
        "has_recent_chunks should be true right after any chunk"
    );
}

#[test]
fn test_has_recent_chunks_true_after_real_chunk() {
    let mut s = SilenceState::new();
    s.on_chunk(false, None, false, false, false);
    assert!(
        s.has_recent_chunks(),
        "has_recent_chunks should be true right after a real output chunk"
    );
}

#[test]
fn test_has_recent_chunks_false_when_no_chunks_for_2s() {
    let mut s = SilenceState::new();
    s.on_chunk(false, None, true, true, false);
    // Backdate last_chunk_at to 3 seconds ago
    s.last_chunk_at = std::time::Instant::now() - std::time::Duration::from_secs(3);
    assert!(
        !s.has_recent_chunks(),
        "has_recent_chunks should be false when last chunk was 3s ago"
    );
}

#[test]
fn test_backup_idle_blocked_when_chunks_arriving() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());

    // last_output_ms is 600ms ago (stale — would normally trigger idle)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 600));

    // Any chunk just arrived (real or chrome-only)
    let mut silence = SilenceState::new();
    silence.on_chunk(false, None, false, false, false); // chunk just arrived

    // should_transition_idle says yes (based on last_output_ms alone)
    assert!(
        should_transition_idle(&state, sid).should_transition,
        "should_transition_idle sees stale last_output_ms"
    );
    // But has_recent_chunks blocks the backup timer (recent chunk activity)
    assert!(
        silence.has_recent_chunks(),
        "backup idle must be blocked because chunks are arriving"
    );
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

// Status-line idle transition: covered by test_backup_idle_blocked_by_chrome_only_ticks.
// Status-line ticking proves the agent is alive — the reader thread's !has_status_line
// guard blocks idle, and has_recent_chunks() (using last_chunk_at) blocks the backup timer.

#[test]
fn test_is_spinner_row_distinguishes_spinner_from_static_chrome() {
    // Spinner rows prove agent is alive
    assert!(crate::chrome::is_spinner_row("✻ Cogitated for 3m 47s"));
    assert!(crate::chrome::is_spinner_row("⠋ Generating..."));
    // Tool progress spinners prove agent is alive
    assert!(crate::chrome::is_spinner_row("◐ Bash: .../b..."));
    assert!(crate::chrome::is_spinner_row("◑ Read: src/main.rs"));
    // Static chrome does NOT prove agent is alive
    assert!(!crate::chrome::is_spinner_row("⏵ auto mode"));
    assert!(!crate::chrome::is_spinner_row("▀▀▀▀▀▀▀▀"));
}

// --- ChunkProcessor tests ---

#[test]
fn test_chunk_processor_new_has_correct_defaults() {
    let cp = ChunkProcessor::new(Some("/home/user/repo".to_string()), None);
    assert_eq!(cp.session_cwd, Some("/home/user/repo".to_string()));
    assert!(cp.last_status_task.is_none());
    assert!(cp.last_question_text.is_none());
    assert!(cp.last_choice_prompt_sig.is_none());
}

#[test]
fn test_chunk_processor_dedup_status_task() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "test-cp-dedup";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));

    let mut cp = ChunkProcessor::new(None, None);
    let mut utf8_buf = Utf8ReadBuffer::new();
    let mut esc_buf = EscapeAwareBuffer::new();

    // First chunk with status line "* Reading files..."
    let raw = b"* Reading files...";
    let utf8_data = utf8_buf.push(raw);
    let esc_data = esc_buf.push(&utf8_data);
    let result1 = cp.process_chunk(&esc_data, &silence, sid, &state);

    // Count how many PtyParsed events were sent with StatusLine
    let mut rx = state.event_bus.subscribe();
    // Second chunk with same status — should be deduped
    let raw2 = b"\r\n* Reading files...";
    let utf8_data2 = utf8_buf.push(raw2);
    let esc_data2 = esc_buf.push(&utf8_data2);
    let _result2 = cp.process_chunk(&esc_data2, &silence, sid, &state);

    // Collect events from the second call
    let mut status_count = 0;
    while let Ok(evt) = rx.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
            && parsed.get("type").and_then(|t| t.as_str()) == Some("status-line")
        {
            status_count += 1;
        }
    }
    assert_eq!(
        status_count, 0,
        "duplicate StatusLine with same task_name should be deduped"
    );

    // Verify the result contains data
    assert!(result1, "first chunk should report data");
}

/// The api-error dedup must reopen when the user submits a line. The reset
/// used to live in `parse_clean_lines`, keyed on a `ParsedEvent::UserInput`
/// no output parser ever produces — that event is emitted on the INPUT
/// thread by `record_submitted_line` — so the branch was dead and the first
/// API error of a session silenced every later one for the whole session.
#[test]
fn user_submission_rearms_the_api_error_dedup() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    const API_ERROR: &str = "API Error: 500 Internal server error.\r\n";

    /// Drain the bus and count the api-error notifications it carried.
    fn api_errors(rx: &mut tokio::sync::broadcast::Receiver<crate::state::AppEvent>) -> usize {
        let mut count = 0;
        while let Ok(evt) = rx.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
                && parsed.get("type").and_then(|t| t.as_str()) == Some("api-error")
            {
                count += 1;
            }
        }
        count
    }

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "api-error-rearm";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    // The startup grace drops ApiError outright; this test is about dedup.
    silence.lock().startup_settled = true;
    state
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));

    let mut cp = ChunkProcessor::new(None, None);
    let mut rx = state.event_bus.subscribe();

    cp.process_chunk("boot\r\n", &silence, sid, &state);
    cp.process_chunk(API_ERROR, &silence, sid, &state);
    assert_eq!(api_errors(&mut rx), 1, "the first API error must notify");

    // Same error text, same turn: still on screen, so it stays deduped.
    cp.process_chunk("retrying\r\n", &silence, sid, &state);
    cp.process_chunk(API_ERROR, &silence, sid, &state);
    assert_eq!(
        api_errors(&mut rx),
        0,
        "a repaint of the same error inside one turn must stay deduped"
    );

    // The user answers. That is the signal that reopens the dedup.
    record_submitted_line(&state, sid, "try again".to_string(), -1);
    api_errors(&mut rx); // drop the submission bookkeeping events

    cp.process_chunk(API_ERROR, &silence, sid, &state);
    assert_eq!(
        api_errors(&mut rx),
        1,
        "the same API error after a user submission is a NEW failure and must notify"
    );
}

/// A new turn must re-emit its status line even when the task name is
/// identical to the previous turn's. Codex names every turn "Working", so a
/// session-lifetime dedup swallows the status line of every turn after the
/// first. Nothing then clears the prior turn's `suggested_actions`, which
/// `session_state_with_shell` reads as a completion marker — a busy agent is
/// reported completed/idle for the rest of the session.
#[test]
fn test_chunk_processor_status_dedup_is_scoped_to_turn() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "test-cp-dedup-turn";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));
    state.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            turn_epoch: 1,
            ..Default::default()
        },
    );

    let mut cp = ChunkProcessor::new(None, None);
    let mut utf8_buf = Utf8ReadBuffer::new();
    let mut esc_buf = EscapeAwareBuffer::new();

    let feed = |cp: &mut ChunkProcessor,
                utf8_buf: &mut Utf8ReadBuffer,
                esc_buf: &mut EscapeAwareBuffer,
                raw: &[u8]|
     -> usize {
        let mut rx = state.event_bus.subscribe();
        let utf8_data = utf8_buf.push(raw);
        let esc_data = esc_buf.push(&utf8_data);
        cp.process_chunk(&esc_data, &silence, sid, &state);
        let mut count = 0;
        while let Ok(evt) = rx.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
                && parsed.get("type").and_then(|t| t.as_str()) == Some("status-line")
            {
                count += 1;
            }
        }
        count
    };

    let turn1 = feed(
        &mut cp,
        &mut utf8_buf,
        &mut esc_buf,
        "• Working (1s • esc to interrupt)".as_bytes(),
    );
    assert_eq!(turn1, 1, "first turn must emit its status line");

    // Spinner rotation inside the SAME turn stays deduped.
    let same_turn = feed(
        &mut cp,
        &mut utf8_buf,
        &mut esc_buf,
        "\r\n• Working (2s • esc to interrupt)".as_bytes(),
    );
    assert_eq!(same_turn, 0, "spinner rotation within a turn must dedup");

    // The user submits again: a new turn begins.
    state
        .session_states
        .get_mut(sid)
        .expect("session state")
        .turn_epoch = 2;

    let turn2 = feed(
        &mut cp,
        &mut utf8_buf,
        &mut esc_buf,
        "\r\n• Working (1s • esc to interrupt)".as_bytes(),
    );
    assert_eq!(
        turn2, 1,
        "a new turn must re-emit the status line even with an identical task name"
    );
}

#[test]
fn test_chunk_processor_dedup_choice_prompt() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "test-cp-choice-dedup";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));

    let mut cp = ChunkProcessor::new(None, None);
    let mut utf8_buf = Utf8ReadBuffer::new();
    let mut esc_buf = EscapeAwareBuffer::new();

    // Paint a Claude Code edit-confirm screen into the terminal.
    let screen_bytes = b"Do you want to make this edit to CLAUDE.md?\r\n\
              \xe2\x9d\xaf 1. Yes\r\n\
              \x20\x20 2. Yes, allow all edits (shift+tab)\r\n\
              \x20\x20 3. No\r\n\
              \r\n\
              Esc to cancel \xc2\xb7 Tab to amend\r\n";
    let utf8_data = utf8_buf.push(screen_bytes);
    let esc_data = esc_buf.push(&utf8_data);
    let _ = cp.process_chunk(&esc_data, &silence, sid, &state);

    // Drain events from the first chunk and count ChoicePrompt emits.
    let mut rx = state.event_bus.subscribe();

    // Second chunk: add an innocuous repaint (cursor home + re-emit same dialog).
    // Same (title, option keys) signature → must be deduped.
    let utf8_data2 = utf8_buf.push(screen_bytes);
    let esc_data2 = esc_buf.push(&utf8_data2);
    let _ = cp.process_chunk(&esc_data2, &silence, sid, &state);

    let mut choice_count = 0;
    while let Ok(evt) = rx.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
            && parsed.get("type").and_then(|t| t.as_str()) == Some("choice-prompt")
        {
            choice_count += 1;
        }
    }
    assert_eq!(
        choice_count, 0,
        "second chunk with identical ChoicePrompt (same title + option keys) must be deduped",
    );
    assert!(
        cp.last_choice_prompt_sig.is_some(),
        "signature must be stored after first emission"
    );
}

/// Every Ink menu footer is byte-identical, so a session-lifetime question
/// dedup made the awaiting badge a one-shot: the first menu of a session
/// silently swallowed every later one. The marker must retire as soon as the
/// prompt leaves the screen.
#[test]
fn test_chunk_processor_question_dedup_retires_when_prompt_leaves_screen() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "test-cp-question-dedup";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));

    let mut cp = ChunkProcessor::new(None, None);
    let mut utf8_buf = Utf8ReadBuffer::new();
    let mut esc_buf = EscapeAwareBuffer::new();
    let mut feed = |cp: &mut ChunkProcessor, bytes: &[u8]| {
        let utf8_data = utf8_buf.push(bytes);
        let esc_data = esc_buf.push(&utf8_data);
        let _ = cp.process_chunk(&esc_data, &silence, sid, state.as_ref());
    };
    let count_questions = |rx: &mut tokio::sync::broadcast::Receiver<crate::state::AppEvent>| {
        let mut n = 0;
        while let Ok(evt) = rx.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
                && parsed.get("type").and_then(|t| t.as_str()) == Some("question")
            {
                n += 1;
            }
        }
        n
    };

    // Ink menu footer — identical bytes for every Claude Code menu.
    const FOOTER: &[u8] =
        "\x1b[2J\x1b[HEnter to select · ↑/↓ to navigate · Esc to cancel\r\n".as_bytes();

    let mut rx = state.event_bus.subscribe();
    feed(&mut cp, FOOTER);
    assert_eq!(count_questions(&mut rx), 1, "first menu must be detected");

    // Repaint while the prompt is still on screen: must stay deduped.
    feed(&mut cp, "\x1b[H".as_bytes());
    feed(&mut cp, FOOTER);
    assert_eq!(
        count_questions(&mut rx),
        0,
        "a repaint of the same on-screen prompt must not re-notify"
    );

    // The user answers: the prompt leaves the screen and the agent works.
    feed(&mut cp, "\x1b[2J\x1b[Hrunning the fix\r\n".as_bytes());
    assert!(
        cp.last_question_text.is_none(),
        "dedup marker must retire once the prompt is off screen"
    );

    // A second menu, byte-identical footer: must be detected again.
    feed(&mut cp, FOOTER);
    assert_eq!(
        count_questions(&mut rx),
        1,
        "a later menu with the same footer must be detected again"
    );
}

/// Same one-shot trap as the question dedup: a dialog the user answers must be
/// detectable again the next time the agent raises it.
#[test]
fn test_chunk_processor_choice_prompt_dedup_retires_when_dialog_leaves_screen() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "test-cp-choice-retire";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));

    let mut cp = ChunkProcessor::new(None, None);
    let mut utf8_buf = Utf8ReadBuffer::new();
    let mut esc_buf = EscapeAwareBuffer::new();
    let mut feed = |cp: &mut ChunkProcessor, bytes: &[u8]| {
        let utf8_data = utf8_buf.push(bytes);
        let esc_data = esc_buf.push(&utf8_data);
        let _ = cp.process_chunk(&esc_data, &silence, sid, state.as_ref());
    };
    let count_choices = |rx: &mut tokio::sync::broadcast::Receiver<crate::state::AppEvent>| {
        let mut n = 0;
        while let Ok(evt) = rx.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
                && parsed.get("type").and_then(|t| t.as_str()) == Some("choice-prompt")
            {
                n += 1;
            }
        }
        n
    };

    const DIALOG: &[u8] = "\x1b[2J\x1b[HDo you want to make this edit to CLAUDE.md?\r\n\
              ❯ 1. Yes\r\n\
                2. Yes, allow all edits (shift+tab)\r\n\
                3. No\r\n\
              \r\n\
              Esc to cancel · Tab to amend\r\n"
        .as_bytes();

    let mut rx = state.event_bus.subscribe();
    feed(&mut cp, DIALOG);
    assert_eq!(count_choices(&mut rx), 1, "first dialog must be detected");

    // Answered: the dialog leaves the screen while the agent applies the edit.
    feed(&mut cp, "\x1b[2J\x1b[Happlying the edit\r\n".as_bytes());
    assert!(
        cp.last_choice_prompt_sig.is_none(),
        "signature must retire once the dialog is off screen"
    );

    // The agent raises the identical dialog again.
    feed(&mut cp, DIALOG);
    assert_eq!(
        count_choices(&mut rx),
        1,
        "the same dialog raised again must be detected again"
    );
}

#[test]
fn test_chunk_processor_planfile_resolution() {
    let cp = ChunkProcessor::new(Some("/home/user/repo".to_string()), None);
    // Test that resolve_planfile_path resolves relative paths
    let resolved = cp.resolve_planfile_path("plans/foo.md");
    assert_eq!(resolved, Some("/home/user/repo/plans/foo.md".to_string()));
}

#[test]
fn test_chunk_processor_planfile_resolution_absolute_passthrough() {
    let cp = ChunkProcessor::new(Some("/home/user/repo".to_string()), None);
    let resolved = cp.resolve_planfile_path("/absolute/path/plan.md");
    assert_eq!(resolved, Some("/absolute/path/plan.md".to_string()));
}

#[test]
fn test_chunk_processor_planfile_resolution_no_cwd() {
    let cp = ChunkProcessor::new(None, None);
    // Relative path with no CWD should return None
    let resolved = cp.resolve_planfile_path("plans/foo.md");
    assert_eq!(resolved, None);
}

#[test]
fn test_chunk_processor_planfile_normalizes_dotdot() {
    let cp = ChunkProcessor::new(Some("/home/user/repo__wt/feat".to_string()), None);
    let resolved = cp.resolve_planfile_path("../../repo/plans/foo.md");
    assert_eq!(resolved, Some("/home/user/repo/plans/foo.md".to_string()));
}

// --- transform_xterm tests ---

#[test]
fn test_transform_xterm_no_token_passes_through() {
    let mut cp = ChunkProcessor::new(None, None);
    let result = cp.transform_xterm("just regular output");
    assert_eq!(result.as_deref(), Some("just regular output"));
}

#[test]
fn test_transform_xterm_intent_passes_through() {
    // Intent coloring is now handled by the frontend MutationObserver.
    let mut cp = ChunkProcessor::new(None, None);
    let result = cp.transform_xterm("intent: Fix the bug\n");
    assert!(result.is_some());
    let data = result.unwrap();
    assert!(
        data.contains("intent: Fix the bug"),
        "intent must pass through to frontend"
    );
}

#[test]
fn test_transform_xterm_suggest_passes_through() {
    // Suggest lines are no longer concealed in Rust — the frontend handles it.
    let mut cp = ChunkProcessor::new(None, None);
    let result = cp.transform_xterm("suggest: A | B | C\n");
    assert!(result.is_some());
    let data = result.unwrap();
    assert!(
        data.contains("suggest:"),
        "suggest must pass through to frontend"
    );
}

#[test]
fn test_transform_xterm_incomplete_intent_passes_through() {
    let mut cp = ChunkProcessor::new(None, None);
    let r1 = cp.transform_xterm("intent: doing so");
    assert!(r1.is_some(), "incomplete intent must pass through");
}

// --- alt buffer clear injection tests ---

#[test]
fn test_transform_xterm_alt_buffer_injects_clear() {
    let mut cp = ChunkProcessor::new(None, None);
    // Enter alt buffer
    cp.transform_xterm("\x1b[?1049h");
    assert!(cp.in_alt_buffer);
    // Cursor home should get ESC[2J injected
    let result = cp.transform_xterm("\x1b[Hcontent").unwrap();
    assert!(
        result.contains("\x1b[2J\x1b[H"),
        "clear should be injected before cursor home"
    );
}

#[test]
fn test_inline_tui_mouse_mode_sets_fullscreen_without_1049() {
    let mut cp = ChunkProcessor::new(None, None);
    cp.apply_inline_tui_mode(false, true, Some("grok"));
    assert!(cp.terminal_mode.is_fullscreen());
    match &cp.terminal_mode {
        crate::ai_agent::tui_detect::TerminalMode::FullscreenTui { app_hint, depth } => {
            assert_eq!(app_hint.as_deref(), Some("grok"));
            assert_eq!(*depth, 1);
        }
        other => panic!("expected FullscreenTui, got {other:?}"),
    }
    cp.apply_inline_tui_mode(false, false, Some("grok"));
    assert!(!cp.terminal_mode.is_fullscreen());
}

#[test]
fn test_inline_tui_does_not_override_alt_screen_mode() {
    let mut cp = ChunkProcessor::new(None, None);
    cp.transform_xterm("\x1b[?1049h");
    cp.apply_inline_tui_mode(true, true, Some("grok"));
    match &cp.terminal_mode {
        crate::ai_agent::tui_detect::TerminalMode::FullscreenTui { depth, .. } => {
            assert_eq!(*depth, 1, "must not nest on top of 1049");
        }
        other => panic!("expected FullscreenTui, got {other:?}"),
    }
}

#[test]
fn test_transform_xterm_normal_buffer_no_inject() {
    let mut cp = ChunkProcessor::new(None, None);
    // NOT in alt buffer — no injection
    let result = cp.transform_xterm("\x1b[Hcontent").unwrap();
    assert!(
        !result.contains("\x1b[2J"),
        "should not inject clear in normal buffer"
    );
}

#[test]
fn test_transform_xterm_alt_buffer_exit_stops_inject() {
    let mut cp = ChunkProcessor::new(None, None);
    // Enter then exit alt buffer
    cp.transform_xterm("\x1b[?1049h");
    cp.transform_xterm("\x1b[?1049l");
    assert!(!cp.in_alt_buffer);
    let result = cp.transform_xterm("\x1b[Hcontent").unwrap();
    assert!(
        !result.contains("\x1b[2J"),
        "should not inject after leaving alt buffer"
    );
}

#[test]
fn test_transform_xterm_alt_buffer_no_clear_on_subsequent_redraws() {
    let mut cp = ChunkProcessor::new(None, None);
    // Enter alt buffer — first cursor-home gets clear
    cp.transform_xterm("\x1b[?1049h");
    let r1 = cp.transform_xterm("\x1b[Hfirst redraw").unwrap();
    assert!(r1.contains("\x1b[2J"), "first redraw must inject clear");

    // Subsequent redraws must NOT inject clear (prevents per-keystroke flicker)
    let r2 = cp.transform_xterm("\x1b[Hsecond redraw").unwrap();
    assert!(
        !r2.contains("\x1b[2J"),
        "subsequent redraws must not inject clear"
    );
}

#[test]
fn test_transform_xterm_alt_buffer_clear_on_shrink() {
    let mut cp = ChunkProcessor::new(None, None);
    // Enter alt buffer, consume initial clear
    cp.transform_xterm("\x1b[?1049h");
    cp.transform_xterm("\x1b[Hinit"); // consumes one-shot

    // Simulate growing content: cursor-up 50 lines
    cp.transform_xterm("\x1b[50A redraw tall");
    assert_eq!(cp.last_cursor_up_n, 50);

    // Simulate shrink: cursor-up only 20 lines (content got shorter)
    let r = cp.transform_xterm("\x1b[20A\x1b[Hredraw short").unwrap();
    assert!(
        r.contains("\x1b[2J"),
        "clear must be injected when content shrinks"
    );
    assert_eq!(cp.last_cursor_up_n, 20);

    // Next redraw at same height — no clear
    let r2 = cp.transform_xterm("\x1b[20A\x1b[Hsame height").unwrap();
    assert!(!r2.contains("\x1b[2J"), "no clear when height stays same");
}

#[test]
fn test_transform_xterm_alt_buffer_clear_on_growth() {
    let mut cp = ChunkProcessor::new(None, None);
    // Enter alt buffer, consume initial clear via cursor-home
    cp.transform_xterm("\x1b[?1049h");
    cp.transform_xterm("\x1b[Hinit");

    // Establish baseline height
    cp.transform_xterm("\x1b[20Aredraw");
    assert_eq!(cp.last_cursor_up_n, 20);

    // Height grows — clear must fire (chrome shifted down, old top row is ghost)
    let r = cp.transform_xterm("\x1b[25A\x1b[Hredraw taller").unwrap();
    assert!(
        r.contains("\x1b[2J"),
        "clear must be injected when content grows"
    );
}

#[test]
fn test_transform_xterm_cursor_up_fallback_on_entry() {
    let mut cp = ChunkProcessor::new(None, None);
    // Enter alt buffer (sets alt_buffer_needs_clear)
    cp.transform_xterm("\x1b[?1049h");

    // Ink re-renders with cursor-up only, no cursor-home.
    // The fallback must inject ESC[2J before the cursor-up.
    let r = cp.transform_xterm("\x1b[30Acontent").unwrap();
    assert!(
        r.contains("\x1b[2J\x1b[30A"),
        "clear must inject before cursor-up fallback"
    );
    assert!(!cp.alt_buffer_needs_clear, "flag must be consumed");
}

#[test]
fn test_transform_xterm_cursor_up_fallback_on_shrink() {
    let mut cp = ChunkProcessor::new(None, None);
    cp.transform_xterm("\x1b[?1049h");
    cp.transform_xterm("\x1b[Hinit"); // consume entry flag

    // Establish height
    cp.transform_xterm("\x1b[40Aredraw");

    // Shrink with cursor-up only (no cursor-home) — fallback path
    let r = cp.transform_xterm("\x1b[25Aredraw short").unwrap();
    assert!(
        r.contains("\x1b[2J\x1b[25A"),
        "cursor-up fallback must fire on shrink"
    );
}

#[test]
fn test_transform_xterm_no_clear_on_normal_buffer_cursor_up() {
    let mut cp = ChunkProcessor::new(None, None);
    // NOT in alt buffer — cursor-up must NOT trigger clear injection
    let r = cp.transform_xterm("\x1b[10Acontent").unwrap();
    assert!(!r.contains("\x1b[2J"), "must not inject in normal buffer");
}

#[test]
fn test_extract_largest_cursor_up() {
    assert_eq!(extract_largest_cursor_up("\x1b[5A"), Some(5));
    assert_eq!(extract_largest_cursor_up("\x1b[10Afoo\x1b[3A"), Some(10));
    assert_eq!(extract_largest_cursor_up("no cursor up here"), None);
    assert_eq!(extract_largest_cursor_up("\x1b[H"), None); // cursor home, not up
}

// --- inject_clear_before_cursor_up tests ---

#[test]
fn test_inject_clear_before_cursor_up_basic() {
    let result = inject_clear_before_cursor_up("\x1b[20Acontent");
    assert_eq!(result, "\x1b[2J\x1b[20Acontent");
}

#[test]
fn test_inject_clear_before_cursor_up_preserves_prefix() {
    let result = inject_clear_before_cursor_up("prefix\x1b[10Acontent");
    assert_eq!(result, "prefix\x1b[2J\x1b[10Acontent");
}

#[test]
fn test_inject_clear_before_cursor_up_no_match() {
    let input = "no cursor up \x1b[H here";
    let result = inject_clear_before_cursor_up(input);
    assert_eq!(
        result, input,
        "cursor-home must NOT match cursor-up injection"
    );
}

#[test]
fn test_inject_clear_before_cursor_up_bare_esc_a_ignored() {
    // ESC[A (no number) means cursor-up 1, but has no digit before A
    let input = "\x1b[Acontent";
    let result = inject_clear_before_cursor_up(input);
    assert_eq!(
        result, input,
        "bare ESC[A (no n) should not trigger injection"
    );
}

// --- log_anomalous_sequences tests ---

#[test]
fn log_anomalous_detects_clear_screen() {
    let found = detect_anomalous_sequences("\x1b[2J");
    assert_eq!(found, vec!["ESC[2J (Clear Screen)"]);
}

#[test]
fn log_anomalous_detects_cursor_home() {
    let found = detect_anomalous_sequences("\x1b[H");
    assert_eq!(found, vec!["ESC[H (Cursor Home)"]);
}

#[test]
fn log_anomalous_detects_cursor_home_explicit() {
    let found = detect_anomalous_sequences("\x1b[1;1H");
    assert_eq!(found, vec!["ESC[1;1H (Cursor Home)"]);
}

#[test]
fn log_anomalous_detects_clear_scrollback() {
    let found = detect_anomalous_sequences("\x1b[3J");
    assert_eq!(found, vec!["ESC[3J (Clear Scrollback)"]);
}

#[test]
fn log_anomalous_detects_alt_screen_enter() {
    let found = detect_anomalous_sequences("\x1b[?1049h");
    assert_eq!(found, vec!["ESC[?1049h (Alt Screen Enter)"]);
}

#[test]
fn log_anomalous_detects_alt_screen_exit() {
    let found = detect_anomalous_sequences("\x1b[?1049l");
    assert_eq!(found, vec!["ESC[?1049l (Alt Screen Exit)"]);
}

#[test]
fn log_anomalous_multiple_in_one_chunk() {
    let found = detect_anomalous_sequences("hello\x1b[2J\x1b[Hworld\x1b[3J");
    assert_eq!(
        found,
        vec![
            "ESC[2J (Clear Screen)",
            "ESC[H (Cursor Home)",
            "ESC[3J (Clear Scrollback)",
        ]
    );
}

#[test]
fn log_anomalous_ignores_normal_sequences() {
    let found = detect_anomalous_sequences("\x1b[5A\x1b[10B\x1b[32mhello\x1b[0m");
    assert!(found.is_empty());
}

#[test]
fn log_anomalous_ignores_cursor_position_not_home() {
    // ESC[5;10H is a regular cursor position, not anomalous
    let found = detect_anomalous_sequences("\x1b[5;10H");
    assert!(found.is_empty());
}

// --- inject_clear_before_cursor_home tests ---

#[test]
fn inject_clear_no_cursor_home() {
    let data = "hello world\x1b[5A\x1b[32mgreen\x1b[0m";
    assert_eq!(inject_clear_before_cursor_home(data), data);
}

#[test]
fn inject_clear_before_bare_home() {
    let data = "\x1b[Hcontent after home";
    assert_eq!(
        inject_clear_before_cursor_home(data),
        "\x1b[2J\x1b[Hcontent after home"
    );
}

#[test]
fn inject_clear_before_explicit_home() {
    let data = "\x1b[1;1Hcontent";
    assert_eq!(
        inject_clear_before_cursor_home(data),
        "\x1b[2J\x1b[1;1Hcontent"
    );
}

#[test]
fn inject_clear_preserves_prefix() {
    let data = "prefix output\x1b[Hredraw content";
    assert_eq!(
        inject_clear_before_cursor_home(data),
        "prefix output\x1b[2J\x1b[Hredraw content"
    );
}

#[test]
fn inject_clear_only_first_home() {
    // Only one ESC[2J should be injected, before the first ESC[H
    let data = "\x1b[Hline1\x1b[Hline2";
    let result = inject_clear_before_cursor_home(data);
    assert_eq!(result, "\x1b[2J\x1b[Hline1\x1b[Hline2");
    // Count occurrences of ESC[2J
    assert_eq!(result.matches("\x1b[2J").count(), 1);
}

#[test]
fn inject_clear_ignores_non_home_cursor_position() {
    // ESC[5;10H is a regular cursor position, not home — should NOT inject
    let data = "\x1b[5;10Hcontent";
    assert_eq!(inject_clear_before_cursor_home(data), data);
}

#[test]
fn inject_clear_preserves_utf8() {
    let data = "héllo → \x1b[Hworld 🌍";
    assert_eq!(
        inject_clear_before_cursor_home(data),
        "héllo → \x1b[2J\x1b[Hworld 🌍"
    );
}

// --- is_wsl_shell tests ---

#[test]
fn is_wsl_shell_bare() {
    assert!(super::is_wsl_shell("wsl.exe"));
    assert!(super::is_wsl_shell("WSL.EXE"));
    assert!(super::is_wsl_shell("wsl"));
}

#[test]
fn is_wsl_shell_with_args() {
    assert!(super::is_wsl_shell("wsl.exe -d Ubuntu"));
    assert!(super::is_wsl_shell(
        "wsl.exe --distribution Debian -- /bin/zsh"
    ));
}

#[test]
fn is_wsl_shell_full_path() {
    assert!(super::is_wsl_shell("C:\\Windows\\System32\\wsl.exe"));
    assert!(super::is_wsl_shell(
        "C:\\Windows\\System32\\wsl.exe -d Ubuntu"
    ));
}

#[test]
fn is_wsl_shell_non_wsl() {
    assert!(!super::is_wsl_shell("powershell.exe"));
    assert!(!super::is_wsl_shell("/bin/zsh"));
    assert!(!super::is_wsl_shell("cmd.exe"));
    assert!(!super::is_wsl_shell("wslconfig.exe"));
}

// --- PTY spawn retry parity (#493-fce6) ---

/// A binary that cannot exist, so `spawn_command` fails on every attempt
/// without depending on the host's PATH.
fn unspawnable_command() -> CommandBuilder {
    CommandBuilder::new("/nonexistent/tuic-spawn-retry-probe")
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

#[test]
fn permanent_allocation_failure_is_not_retried() {
    let mut attempts = 0;
    let result = retry_transient(
        || {
            attempts += 1;
            Err::<(), _>("permission denied")
        },
        |_| false,
        |_| panic!("permanent failure must not sleep"),
    );

    assert_eq!(result, Err((1, "permission denied")));
    assert_eq!(attempts, 1);
}

#[cfg(unix)]
#[test]
fn pty_allocation_classifier_retries_resource_pressure_not_permissions() {
    let exhausted = anyhow::Error::new(std::io::Error::from_raw_os_error(libc::EMFILE));
    let denied = anyhow::Error::new(std::io::Error::from_raw_os_error(libc::EACCES));

    assert!(is_transient_pty_open_error(&exhausted));
    assert!(!is_transient_pty_open_error(&denied));
}

#[test]
fn permanent_command_spawn_failure_builds_once() {
    let attempts = std::cell::Cell::new(0);
    let Err(error) = spawn_pty_pair_with_retry(probe_size(), || {
        attempts.set(attempts.get() + 1);
        unspawnable_command()
    }) else {
        panic!("a nonexistent binary must not spawn");
    };

    assert_eq!(attempts.get(), 1);
    assert!(error.contains("Failed to spawn shell"), "{error}");
}

/// A spawn that works first time must be built exactly once.
#[test]
fn a_working_spawn_is_built_once() {
    let attempts = std::cell::Cell::new(0);
    let (_pair, child) = spawn_pty_pair_with_retry(probe_size(), || {
        attempts.set(attempts.get() + 1);
        CommandBuilder::new("/bin/echo")
    })
    .expect("echo must spawn");

    assert_eq!(attempts.get(), 1);
    reap(child);
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

// --- build_shell_command arg splitting tests ---

#[test]
fn build_shell_command_splits_args() {
    let cmd = super::build_shell_command("wsl.exe -d Ubuntu");
    let argv = cmd.as_unix_command_line().unwrap();
    // The command line should contain the args as separate tokens
    assert!(argv.contains("-d"), "Expected -d in: {}", argv);
    assert!(argv.contains("Ubuntu"), "Expected Ubuntu in: {}", argv);
}

#[test]
fn build_shell_command_single_exe() {
    // Single executable should still work (no extra empty args)
    let cmd = super::build_shell_command("/bin/zsh");
    let argv = cmd.as_unix_command_line().unwrap();
    assert!(argv.contains("/bin/zsh"), "Expected /bin/zsh in: {}", argv);
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

// --- windows_to_wsl_path tests ---

#[test]
fn wsl_path_drive_letter_backslash() {
    assert_eq!(
        super::windows_to_wsl_path("C:\\Users\\foo\\repos"),
        "/mnt/c/Users/foo/repos"
    );
}

#[test]
fn wsl_path_drive_letter_forward_slash() {
    assert_eq!(
        super::windows_to_wsl_path("C:/Users/foo/repos"),
        "/mnt/c/Users/foo/repos"
    );
}

#[test]
fn wsl_path_lowercase_drive() {
    assert_eq!(super::windows_to_wsl_path("d:\\work"), "/mnt/d/work");
}

#[test]
fn wsl_path_already_linux() {
    assert_eq!(
        super::windows_to_wsl_path("/home/user/repos"),
        "/home/user/repos"
    );
}

#[test]
fn wsl_path_unc_unchanged() {
    // UNC paths are not drive-letter paths — returned as-is
    assert_eq!(
        super::windows_to_wsl_path("\\\\server\\share"),
        "\\\\server\\share"
    );
}

#[test]
fn wsl_path_root_drive() {
    assert_eq!(super::windows_to_wsl_path("C:\\"), "/mnt/c/");
}

// ---- Layer 3: state_change auto-notifications (#1164-2571) ----

#[test]
fn mark_session_exited_pushes_state_change_to_parent_inbox() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let child_id = "child-sess";
    let parent_id = "parent-sess";

    // Register parent-child relationship
    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    // Pre-init parent inbox
    state.agent_inbox.entry(parent_id.to_string()).or_default();

    mark_session_exited(child_id, &state);

    let inbox = state
        .agent_inbox
        .get(parent_id)
        .expect("parent inbox must exist");
    assert!(
        !inbox.is_empty(),
        "parent inbox must have received state_change message"
    );
    let msg = inbox.front().unwrap();
    let content: serde_json::Value =
        serde_json::from_str(&msg.content).expect("content must be valid JSON");
    assert_eq!(content["type"], "state_change");
    assert_eq!(content["state"], "exited");
}

// ---- Self-acknowledging orchestrator lifecycle summary ----

fn lifecycle_inbox_message(
    id: &str,
    child: &str,
    timestamp: u64,
    content: serde_json::Value,
) -> crate::state::AgentMessage {
    crate::state::AgentMessage {
        id: id.to_string(),
        from_tuic_session: child.to_string(),
        from_name: "tuic".to_string(),
        content: content.to_string(),
        timestamp,
        delivered_via_channel: false,
    }
}

const SUMMARY_CHILD: &str = "8c261794-91e5-44a4-bf63-ec8afafd2adc";

fn idle_payload() -> serde_json::Value {
    serde_json::json!({"type": "state_change", "state": "idle", "session_id": SUMMARY_CHILD})
}

fn exited_payload() -> serde_json::Value {
    serde_json::json!({
        "type": "state_change",
        "state": "exited",
        "session_id": SUMMARY_CHILD,
        "exit_code": 0,
    })
}

#[test]
fn lifecycle_summary_carries_every_event_in_the_window() {
    let state = crate::state::tests_support::make_test_app_state();
    let parent = "parent-summary";
    state.push_agent_inbox(
        parent,
        lifecycle_inbox_message("tuic-auto-idle", SUMMARY_CHILD, 10, idle_payload()),
    );
    state.push_agent_inbox(
        parent,
        lifecycle_inbox_message("tuic-auto-exit", SUMMARY_CHILD, 20, exited_payload()),
    );

    let summary = summarize_lifecycle_group(
        &state,
        parent,
        crate::state::OrchestratorWakeGroup {
            observed_through: 0,
            wake_through: 20,
        },
    )
    .expect("a lifecycle-only window must summarize");

    assert!(
        summary.contains("child agent 8c261794 is now idle"),
        "{summary}"
    );
    assert!(
        summary.contains("child agent 8c261794 exited (exit 0)"),
        "{summary}"
    );
    assert!(
        !summary.contains("action=inbox"),
        "a self-acknowledging notice must not send the reader to the inbox: {summary}"
    );
    assert_eq!(summary.lines().count(), 1, "must stay one composer line");
}

#[test]
fn peer_payload_in_the_window_forces_the_generic_wake() {
    let state = crate::state::tests_support::make_test_app_state();
    let parent = "parent-mixed";
    state.push_agent_inbox(
        parent,
        lifecycle_inbox_message("tuic-auto-idle", SUMMARY_CHILD, 10, idle_payload()),
    );
    state.push_agent_inbox(
        parent,
        crate::state::AgentMessage {
            id: "peer-1".to_string(),
            from_tuic_session: "peer".to_string(),
            from_name: "sender".to_string(),
            content: "secret peer payload".to_string(),
            timestamp: 20,
            delivered_via_channel: false,
        },
    );

    assert!(
        summarize_lifecycle_group(
            &state,
            parent,
            crate::state::OrchestratorWakeGroup {
                observed_through: 0,
                wake_through: 20,
            },
        )
        .is_none(),
        "one peer message must disqualify the whole group, not be skipped"
    );
}

#[test]
fn lifecycle_summary_ignores_messages_outside_the_reserved_window() {
    let state = crate::state::tests_support::make_test_app_state();
    let parent = "parent-window";
    state.push_agent_inbox(
        parent,
        lifecycle_inbox_message("tuic-auto-old", SUMMARY_CHILD, 10, idle_payload()),
    );
    state.push_agent_inbox(
        parent,
        lifecycle_inbox_message("tuic-auto-covered", SUMMARY_CHILD, 20, exited_payload()),
    );
    // Arrived after the reservation: neither described nor disqualifying.
    state.push_agent_inbox(
        parent,
        crate::state::AgentMessage {
            id: "peer-late".to_string(),
            from_tuic_session: "peer".to_string(),
            from_name: "sender".to_string(),
            content: "later peer payload".to_string(),
            timestamp: 30,
            delivered_via_channel: false,
        },
    );

    let summary = summarize_lifecycle_group(
        &state,
        parent,
        crate::state::OrchestratorWakeGroup {
            observed_through: 10,
            wake_through: 20,
        },
    )
    .expect("the reserved window is lifecycle-only");

    assert!(summary.contains("exited (exit 0)"), "{summary}");
    assert!(
        !summary.contains("is now idle"),
        "an already-observed message must not be repeated: {summary}"
    );
    assert!(!summary.contains("later peer payload"), "{summary}");
}

#[test]
fn oversize_lifecycle_summary_falls_back_to_the_generic_wake() {
    let state = crate::state::tests_support::make_test_app_state();
    let parent = "parent-oversize";
    for index in 0..12u64 {
        state.push_agent_inbox(
            parent,
            lifecycle_inbox_message(
                &format!("tuic-auto-{index}"),
                SUMMARY_CHILD,
                index + 1,
                idle_payload(),
            ),
        );
    }

    assert!(
        summarize_lifecycle_group(
            &state,
            parent,
            crate::state::OrchestratorWakeGroup {
                observed_through: 0,
                wake_through: 12,
            },
        )
        .is_none(),
        "a burst too long to type must fall back to the generic wake"
    );
}

#[test]
fn prompt_delivery_failure_reads_the_same_in_both_paths() {
    let payload = serde_json::json!({
        "type": "prompt_delivery_failed",
        "reason": "timeout",
        "session_id": SUMMARY_CHILD,
    });
    assert_eq!(
        describe_lifecycle_payload(SUMMARY_CHILD, &payload),
        "child agent 8c261794 initial prompt delivery timed out"
    );
}

#[test]
fn try_shell_transition_busy_to_idle_pushes_state_change_to_parent_inbox() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-idle-sess";
    let parent_id = "parent-idle-sess";

    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    // Must have a session_state with agent_type to qualify for idle notification
    let ss = crate::state::SessionState {
        agent_type: Some("claude".to_string()),
        ..Default::default()
    };
    state.session_states.insert(child_id.to_string(), ss);
    state.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state.silence_states.insert(
        child_id.to_string(),
        Arc::new(Mutex::new(SilenceState::new())),
    );

    let transitioned = try_shell_transition(&state, child_id, SHELL_BUSY, SHELL_IDLE, true);
    assert!(transitioned, "transition must succeed");

    let inbox = state
        .agent_inbox
        .get(parent_id)
        .expect("parent inbox must exist");
    assert!(
        !inbox.is_empty(),
        "parent inbox must have received state_change message"
    );
    let msg = inbox.front().unwrap();
    let content: serde_json::Value =
        serde_json::from_str(&msg.content).expect("content must be valid JSON");
    assert_eq!(content["type"], "state_change");
    assert_eq!(content["state"], "idle");
}

#[test]
fn background_work_defers_parent_idle_until_descendants_finish() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-background-sess";
    let parent_id = "parent-background-sess";
    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
            ..Default::default()
        },
    );
    state.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state.silence_states.insert(
        child_id.to_string(),
        Arc::new(Mutex::new(SilenceState::new())),
    );

    assert!(try_shell_transition(
        &state, child_id, SHELL_BUSY, SHELL_IDLE, true
    ));
    assert!(
        state.agent_inbox.get(parent_id).unwrap().is_empty(),
        "a ready composer must not announce autonomous completion"
    );

    assert!(set_background_work_for_epoch(&state, child_id, 0, 1, false));
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(inbox.len(), 1);
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "idle");
}

#[test]
fn background_work_defers_declared_completion_without_generic_idle() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-background-completed";
    let parent_id = "parent-background-completed";
    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
            ..Default::default()
        },
    );
    state.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    let mut silence = SilenceState::new();
    silence.mark_suggest_candidate(vec!["Review result".to_string()], 0);
    let silence = Arc::new(Mutex::new(silence));
    state
        .silence_states
        .insert(child_id.to_string(), silence.clone());

    assert!(!emit_pending_suggest_if_idle(&state, &silence, child_id));
    assert!(set_background_work_for_epoch(&state, child_id, 0, 1, false));
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(inbox.len(), 1);
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "completed");
    assert!(!emit_pending_suggest_if_idle(&state, &silence, child_id));
}

#[test]
fn cursor_prefix_completion_preserves_background_epoch_release() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-background-cursor-completed";
    agent_session(&state, child_id, SHELL_IDLE);
    state
        .session_states
        .get_mut(child_id)
        .unwrap()
        .background_work = true;
    state.vt_log_buffers.insert(
        child_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    let silence = state.silence_states.get(child_id).unwrap().clone();
    let mut processor = ChunkProcessor::new(None, None);

    processor.process_chunk("........................| C ]", &silence, child_id, &state);
    processor.process_chunk("\rsuggest: [ A | B", &silence, child_id, &state);
    assert!(!silence.lock().completion_declared_for_epoch(0));
    processor.process_chunk("\r\x1b[", &silence, child_id, &state);
    assert!(!silence.lock().completion_declared_for_epoch(0));
    processor.process_chunk("Ksuggest: [ A | B | C ]", &silence, child_id, &state);

    {
        let guard = silence.lock();
        assert!(guard.completion_declared_for_epoch(0));
        assert_eq!(
            guard.pending_suggest_items.as_deref(),
            Some(&["A".to_string(), "B".to_string(), "C".to_string()][..])
        );
        assert_eq!(guard.pending_suggest_turn_epoch, 0);
    }
    assert!(try_shell_transition(
        &state, child_id, SHELL_BUSY, SHELL_IDLE, false
    ));
    let deferred = state.session_state_with_shell(child_id).unwrap();
    assert_eq!(deferred.agent_state.as_deref(), Some("working"));
    assert!(deferred.background_work);
    assert!(set_background_work_for_epoch(&state, child_id, 0, 1, false));
    let snapshot = state.session_state_with_shell(child_id).unwrap();
    assert_eq!(snapshot.agent_state.as_deref(), Some("completed"));
    assert!(!snapshot.background_work);
}

#[test]
fn physical_cursor_suggest_completes_after_wrapped_background_probe_clears() {
    for wrap_count in 1..=5 {
        let state = crate::state::tests_support::make_test_app_state();
        let child_id = format!("wrapped-background-physical-suggest-{wrap_count}");
        agent_session(&state, &child_id, SHELL_IDLE);
        state.vt_log_buffers.insert(
            child_id.clone(),
            Mutex::new(crate::state::VtLogBuffer::new(10, 80, 1000)),
        );
        let silence = state.silence_states.get(&child_id).unwrap().clone();
        let mut processor = ChunkProcessor::new(None, None);

        note_submitted_input(&state, &child_id);
        processor.process_chunk(
            &"x".repeat(wrap_count * 80 + 1),
            &silence,
            &child_id,
            &state,
        );
        {
            let mut session = state.session_states.get_mut(&child_id).unwrap();
            session.background_probe_turn_epoch = Some(1);
            session.background_probe_after_generation = Some(0);
        }
        assert!(set_background_work_for_epoch(&state, &child_id, 1, 1, true));
        assert!(state.session_states.get(&child_id).unwrap().background_work);

        assert!(try_shell_transition(
            &state, &child_id, SHELL_BUSY, SHELL_IDLE, false,
        ));
        assert!(set_background_work_for_epoch(
            &state, &child_id, 1, 2, false
        ));
        assert_eq!(
            state
                .session_state_with_shell(&child_id)
                .unwrap()
                .agent_state
                .as_deref(),
            Some("idle"),
            "wrap_count={wrap_count}"
        );

        processor.process_chunk(
            "\r\x1b[2Ksuggest: [ background cleared | lifecycle complete | close smoke ]",
            &silence,
            &child_id,
            &state,
        );

        let snapshot = state.session_state_with_shell(&child_id).unwrap();
        assert_eq!(
            snapshot.agent_state.as_deref(),
            Some("completed"),
            "wrap_count={wrap_count}"
        );
        assert!(!snapshot.background_work, "wrap_count={wrap_count}");
        assert!(
            silence.lock().completion_declared_for_epoch(1),
            "wrap_count={wrap_count}"
        );
    }
}

#[test]
fn identical_suggest_reopens_only_after_fresh_work_in_a_new_turn() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "suggest-multiple-turns";
    agent_session(&state, child_id, SHELL_IDLE);
    state.vt_log_buffers.insert(
        child_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    let silence = state.silence_states.get(child_id).unwrap().clone();
    let mut processor = ChunkProcessor::new(None, None);
    let marker = "suggest: [ lifecycle fixed | close smoke | continue parity ]";

    processor.process_chunk("first response\r\n", &silence, child_id, &state);
    processor.process_chunk(marker, &silence, child_id, &state);
    assert_eq!(
        silence.lock().drain_pending_suggest(),
        Some(vec![
            "lifecycle fixed".to_string(),
            "close smoke".to_string(),
            "continue parity".to_string(),
        ])
    );

    note_submitted_input(&state, child_id);
    assert_eq!(state.session_states.get(child_id).unwrap().turn_epoch, 1);

    // A previous-turn row can repaint as the input scrolls. Submission by
    // itself must not reopen the content deduplication boundary.
    processor.process_chunk(&format!("\r\n{marker}"), &silence, child_id, &state);
    assert_eq!(silence.lock().drain_pending_suggest(), None);
    processor.process_chunk(&format!("\r\n{marker}"), &silence, child_id, &state);
    assert_eq!(silence.lock().drain_pending_suggest(), None);

    // Real output proves the next response started. The identical marker
    // is now a valid completion, but a second repaint in the same turn is
    // still suppressed.
    processor.process_chunk("\r\nsecond response\r\n", &silence, child_id, &state);
    processor.process_chunk(marker, &silence, child_id, &state);
    assert_eq!(
        silence.lock().drain_pending_suggest(),
        Some(vec![
            "lifecycle fixed".to_string(),
            "close smoke".to_string(),
            "continue parity".to_string(),
        ])
    );
    processor.process_chunk(&format!("\r\n{marker}"), &silence, child_id, &state);
    assert_eq!(silence.lock().drain_pending_suggest(), None);
    assert!(silence.lock().completion_declared_for_epoch(1));
}

#[test]
fn cursor_prefix_rejects_stale_suffix_then_emits_real_completion_once() {
    for (index, bullet) in ["●", "⏺", "•", "◦"].into_iter().enumerate() {
        let state = crate::state::tests_support::make_test_app_state();
        let child_id = format!("cursor-stale-suffix-{index}");
        agent_session(&state, &child_id, SHELL_IDLE);
        state.vt_log_buffers.insert(
            child_id.clone(),
            Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
        );
        let silence = state.silence_states.get(&child_id).unwrap().clone();
        let mut processor = ChunkProcessor::new(None, None);

        processor.process_chunk(
            "............................| C ]",
            &silence,
            &child_id,
            &state,
        );
        processor.process_chunk(
            &format!("\r{bullet} suggest: [ A | B"),
            &silence,
            &child_id,
            &state,
        );
        assert_eq!(silence.lock().drain_pending_suggest(), None, "{bullet}");

        processor.process_chunk("\r\x1b[", &silence, &child_id, &state);
        assert_eq!(silence.lock().drain_pending_suggest(), None, "{bullet}");
        processor.process_chunk(
            &format!("K{bullet} suggest: [ A | B | C ]"),
            &silence,
            &child_id,
            &state,
        );
        assert_eq!(
            silence.lock().drain_pending_suggest(),
            Some(vec!["A".to_string(), "B".to_string(), "C".to_string()]),
            "{bullet}"
        );

        processor.process_chunk(
            &format!("\r\x1b[K{bullet} suggest: [ A | B | C ]"),
            &silence,
            &child_id,
            &state,
        );
        assert_eq!(silence.lock().drain_pending_suggest(), None, "{bullet}");
    }
}

#[test]
fn wrapped_suggest_reconstructs_unchanged_anchor_across_chunks() {
    for (index, bullet) in ["●", "⏺", "•", "◦"].into_iter().enumerate() {
        let state = crate::state::tests_support::make_test_app_state();
        let child_id = format!("wrapped-suggest-across-chunks-{index}");
        agent_session(&state, &child_id, SHELL_IDLE);
        state.vt_log_buffers.insert(
            child_id.clone(),
            Mutex::new(crate::state::VtLogBuffer::new(24, 14, 1000)),
        );
        let silence = state.silence_states.get(&child_id).unwrap().clone();
        let mut processor = ChunkProcessor::new(None, None);

        processor.process_chunk(
            &format!("{bullet} suggest: [ A"),
            &silence,
            &child_id,
            &state,
        );
        assert_eq!(silence.lock().drain_pending_suggest(), None, "{bullet}");
        processor.process_chunk("界 | B | C ]", &silence, &child_id, &state);

        assert_eq!(
            silence.lock().drain_pending_suggest(),
            Some(vec!["A界".to_string(), "B".to_string(), "C".to_string()]),
            "{bullet}"
        );
    }
}

#[test]
fn bounded_cursor_prefix_refusal_suppresses_structured_completion() {
    for (child_id, columns, token) in [
        (
            "cursor-prefix-over-512-bytes",
            700,
            format!("suggest: [ A | B | C ]{}", "x".repeat(520)),
        ),
        (
            "cursor-prefix-over-four-wraps",
            20,
            format!(
                "suggest: [ {} | {} | {} | {} ]",
                "a".repeat(30),
                "b".repeat(30),
                "c".repeat(30),
                "d".repeat(30)
            ),
        ),
    ] {
        let state = crate::state::tests_support::make_test_app_state();
        agent_session(&state, child_id, SHELL_IDLE);
        state.vt_log_buffers.insert(
            child_id.to_string(),
            Mutex::new(crate::state::VtLogBuffer::new(24, columns, 1000)),
        );
        let silence = state.silence_states.get(child_id).unwrap().clone();
        let mut processor = ChunkProcessor::new(None, None);

        processor.process_chunk(&token, &silence, child_id, &state);

        assert_eq!(silence.lock().drain_pending_suggest(), None, "{child_id}");
    }
}

#[test]
fn wrapped_suggest_requires_complete_non_nested_prefix() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "wrapped-suggest-incomplete";
    agent_session(&state, child_id, SHELL_IDLE);
    state.vt_log_buffers.insert(
        child_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 10, 1000)),
    );
    let silence = state.silence_states.get(child_id).unwrap().clone();
    let mut processor = ChunkProcessor::new(None, None);

    processor.process_chunk("suggest: [", &silence, child_id, &state);
    processor.process_chunk(" A | B", &silence, child_id, &state);
    assert_eq!(silence.lock().drain_pending_suggest(), None);

    processor.process_chunk("\r\x1b[2K\x1b[1A\r\x1b[2K", &silence, child_id, &state);
    processor.process_chunk("suggest: [", &silence, child_id, &state);
    processor.process_chunk(" A | EP[\"node\"] | C ]", &silence, child_id, &state);
    assert_eq!(silence.lock().drain_pending_suggest(), None);
}

#[test]
fn stale_background_clear_cannot_emit_after_new_turn() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-background-race";
    let parent_id = "parent-background-race";
    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
            turn_epoch: 7,
            ..Default::default()
        },
    );
    state.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state.silence_states.insert(
        child_id.to_string(),
        Arc::new(Mutex::new(SilenceState::new())),
    );

    note_submitted_input(&state, child_id);
    assert!(!set_background_work_for_epoch(
        &state, child_id, 7, 1, false
    ));
    assert!(state.session_states.get(child_id).unwrap().background_work);
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());
}

#[test]
fn background_snapshot_teardown_does_not_recreate_lifecycle_or_notify() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "background-teardown";
    let parent_id = "background-teardown-parent";
    agent_session(&state, child_id, SHELL_IDLE);
    state
        .session_states
        .get_mut(child_id)
        .unwrap()
        .background_work = true;
    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();

    assert!(!set_background_work_for_epoch_with_hook(
        &state,
        child_id,
        0,
        1,
        false,
        || {
            state.silence_states.remove(child_id);
            state.shell_states.remove(child_id);
            state.session_states.remove(child_id);
        },
    ));
    assert!(!state.silence_states.contains_key(child_id));
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());
}

#[test]
fn failed_or_invalid_cached_snapshot_preserves_background_work() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "background-snapshot-failure";
    state.session_states.insert(
        session_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
            turn_epoch: 3,
            ..Default::default()
        },
    );

    assert!(!refresh_background_work_from_cached_snapshot(
        &state, session_id, 10, "codex", 3, None,
    ));
    let invalid = Arc::new(vec![process(20, 1, "codex", "codex")]);
    assert!(!refresh_background_work_from_cached_snapshot(
        &state,
        session_id,
        10,
        "codex",
        3,
        Some((1, invalid)),
    ));
    assert!(
        state
            .session_states
            .get(session_id)
            .unwrap()
            .background_work
    );
}

#[test]
fn cached_snapshot_detects_background_process_exit() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-background-exit";
    let parent_id = "parent-background-exit";
    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
            turn_epoch: 4,
            ..Default::default()
        },
    );
    state.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state.silence_states.insert(
        child_id.to_string(),
        Arc::new(Mutex::new(SilenceState::new())),
    );
    let exited = Arc::new(vec![process(10, 1, "codex", "codex")]);

    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "codex",
        4,
        Some((2, exited)),
    ));
    assert!(!state.session_states.get(child_id).unwrap().background_work);
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "idle");
}

#[cfg(unix)]
#[test]
fn standby_refuses_session_with_background_work() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "background-standby";
    state.session_states.insert(
        session_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
            ..Default::default()
        },
    );
    assert_eq!(standby_session(&state, session_id), Ok(false));
    assert!(!state.standby_sessions.contains_key(session_id));
}

#[cfg(unix)]
#[test]
fn standby_refuses_session_with_pending_background_probe() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "background-probe-standby";
    state.session_states.insert(
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
    assert!(!state.standby_sessions.contains_key(session_id));
}

#[test]
fn declared_completion_does_not_emit_ambiguous_idle_lifecycle() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-completed-sess";
    let parent_id = "parent-completed-sess";

    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            ..Default::default()
        },
    );
    state.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    let mut silence = SilenceState::new();
    silence.mark_suggest_candidate(vec!["Review result".to_string()], 0);
    state
        .silence_states
        .insert(child_id.to_string(), Arc::new(Mutex::new(silence)));

    assert!(try_shell_transition(
        &state, child_id, SHELL_BUSY, SHELL_IDLE, true
    ));

    assert!(
        state.agent_inbox.get(parent_id).unwrap().is_empty(),
        "the suggest drain must publish completed instead of an earlier idle"
    );
    let silence = state.silence_states.get(child_id).unwrap().clone();
    assert!(emit_pending_suggest_if_idle(&state, &silence, child_id));
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(inbox.len(), 1);
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "completed");
}

#[test]
fn pending_initial_prompt_timeout_notifies_parent_once() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let child_id = "child-prompt-timeout";
    let parent_id = "parent-prompt-timeout";
    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state
        .pending_initial_prompts
        .insert(child_id.to_string(), "do the task".to_string());

    assert!(notify_initial_prompt_timeout_if_pending(&state, child_id));
    assert!(!notify_initial_prompt_timeout_if_pending(&state, child_id));

    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(inbox.len(), 1, "timeout notification must be emitted once");
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["type"], "prompt_delivery_failed");
    assert_eq!(content["reason"], "timeout");
    assert_eq!(content["session_id"], child_id);
    assert!(!state.pending_initial_prompts.contains_key(child_id));
}

#[test]
fn try_shell_transition_non_agent_session_does_not_push_idle_notification() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "non-agent-sess";
    let parent_id = "parent-non-agent-sess";

    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    // No agent_type set — plain shell session
    state
        .session_states
        .insert(child_id.to_string(), crate::state::SessionState::default());
    state.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );

    try_shell_transition(&state, child_id, SHELL_BUSY, SHELL_IDLE, true);

    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert!(
        inbox.is_empty(),
        "non-agent sessions must not send idle notifications to parent"
    );
}

#[test]
fn try_shell_transition_exit_path_does_not_push_idle_to_parent() {
    // notify_parent=false (exit path): orchestrator must NOT receive spurious "idle"
    // before the "exited" message from mark_session_exited.
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-exit-path";
    let parent_id = "parent-exit-path";

    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    let ss = crate::state::SessionState {
        agent_type: Some("claude".to_string()),
        ..Default::default()
    };
    state.session_states.insert(child_id.to_string(), ss);
    state.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );

    let transitioned = try_shell_transition(&state, child_id, SHELL_BUSY, SHELL_IDLE, false);
    assert!(transitioned, "transition must succeed");

    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert!(
        inbox.is_empty(),
        "exit path must not push idle notification — mark_session_exited sends exited"
    );
}

#[test]
fn tombstone_transient_cleanup_removes_swarm_maps() {
    // F3: all per-child swarm state must be cleaned on exit.
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "sess-cleanup";
    let mcp_sid = "mcp-sess-cleanup";

    state
        .session_parent
        .insert(sid.to_string(), "parent-sess".to_string());
    state
        .shell_state_since_ms
        .insert(sid.to_string(), std::sync::atomic::AtomicU64::new(42));
    state
        .mcp_to_session
        .insert(mcp_sid.to_string(), sid.to_string());
    state
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

    tombstone_transient_cleanup(sid, &state);

    assert!(
        !state.session_parent.contains_key(sid),
        "session_parent must be removed"
    );
    assert!(
        !state.shell_state_since_ms.contains_key(sid),
        "shell_state_since_ms must be removed"
    );
    assert!(
        !state.mcp_to_session.contains_key(mcp_sid),
        "mcp_to_session entry must be removed"
    );
    assert!(
        !state.session_to_mcp.contains_key(sid),
        "session_to_mcp entry must be removed"
    );
    assert!(!state.peer_agents.contains_key(sid));
    assert!(!state.agent_inbox.contains_key(sid));
    assert!(!state.agent_inbox_evictions.contains_key(sid));
}

// ── PTY-injection message delivery (Step 2) ─────────────────────

fn agent_session(state: &crate::state::AppState, sid: &str, shell: u8) {
    use std::sync::atomic::AtomicU8;
    state
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(shell));
    state.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            ..Default::default()
        },
    );
    let mut silence = SilenceState::new();
    silence.idle_confirmed = shell == SHELL_IDLE;
    state
        .silence_states
        .insert(sid.to_string(), Arc::new(Mutex::new(silence)));
}

fn flush_one_pending_as_submitted(state: &crate::state::AppState, sid: &str) {
    let claim = claim_idle_for_injection(state, sid).expect("idle claim");
    let injection = state
        .pending_injections
        .get_mut(sid)
        .and_then(|mut queue| queue.pop_front())
        .expect("pending message");
    apply_claimed_injection_outcome(
        state,
        sid,
        injection.text(),
        claim,
        InjectionOutcome::Submitted,
        ClaimedInjectionKind::Message,
    );
}

fn completed_agent_session(state: &crate::state::AppState, sid: &str) {
    agent_session(state, sid, SHELL_IDLE);
    state.session_states.get_mut(sid).unwrap().suggested_actions =
        Some(vec!["Review result".to_string()]);
    state
        .silence_states
        .get(sid)
        .unwrap()
        .lock()
        .mark_suggest_candidate(vec!["Review result".to_string()], 0);
}

fn assert_new_turn_silence_evidence(silence: &SilenceState) {
    assert!(silence.explicit_busy);
    assert!(!silence.hook_busy);
    assert!(!silence.explicit_idle);
    assert!(!silence.idle_confirmed);
    assert!(silence.last_status_line_at.is_some());
    assert!(silence.ready_since.is_none());
    assert!(silence.interrupt_requested_at.is_none());
    assert!(silence.turn_started_by_input);
    assert!(!silence.turn_activity_seen);
    assert!(!silence.completion_declared);
    assert!(silence.pending_suggest_items.is_none());
}

#[test]
fn submitted_input_lifecycle_peer_injection_starts_new_turn_and_clears_completion() {
    let state = crate::state::tests_support::make_test_app_state();
    completed_agent_session(&state, "completed");
    state.pending_injections.insert(
        "completed".to_string(),
        std::collections::VecDeque::from([crate::state::PendingInjection::peer_message(
            "follow up",
        )]),
    );

    flush_one_pending_as_submitted(&state, "completed");

    let snapshot = state.session_state_with_shell("completed").unwrap();
    assert_eq!(snapshot.shell_state.as_deref(), Some("busy"));
    assert_eq!(snapshot.agent_state.as_deref(), Some("working"));
    assert!(snapshot.suggested_actions.is_none());
    assert!(
        !state
            .silence_states
            .get("completed")
            .unwrap()
            .lock()
            .completion_declared()
    );
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
        assert_eq!(state.session_states.get(session_id).unwrap().turn_epoch, 1);
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
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
}

#[test]
fn idle_parent_notification_finishes_before_new_turn_reservation() {
    use std::sync::atomic::Ordering;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let child_id = "idle-race-child";
    let parent_id = "idle-race-parent";
    agent_session(&state, child_id, SHELL_BUSY);
    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();

    let (start_tx, start_rx) = std::sync::mpsc::channel();
    let (lock_held_tx, lock_held_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let submitter_state = Arc::clone(&state);
    let submitter = std::thread::spawn(move || {
        start_rx.recv().unwrap();
        let silence = submitter_state
            .silence_states
            .get(child_id)
            .unwrap()
            .clone();
        lock_held_tx.send(silence.try_lock().is_none()).unwrap();
        note_submitted_input(&submitter_state, child_id);
        finished_tx.send(()).unwrap();
    });

    assert!(try_shell_transition_with_hook(
        &state,
        child_id,
        SHELL_BUSY,
        SHELL_IDLE,
        true,
        || {
            start_tx.send(()).unwrap();
            assert!(
                lock_held_rx.recv().unwrap(),
                "BUSY→IDLE must retain the lifecycle lock through parent enqueue"
            );
        },
    ));
    finished_rx.recv().unwrap();
    submitter.join().unwrap();

    assert_eq!(
        state
            .shell_states
            .get(child_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
    assert_eq!(state.session_states.get(child_id).unwrap().turn_epoch, 1);
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(inbox.len(), 1);
    let payload: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(payload["state"], "idle");
}

#[test]
fn new_turn_wins_before_queued_old_idle_transition() {
    use std::sync::atomic::Ordering;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let child_id = "inverse-idle-child";
    let parent_id = "inverse-idle-parent";
    agent_session(&state, child_id, SHELL_BUSY);
    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();

    let (snapshotted_tx, snapshotted_rx) = std::sync::mpsc::channel();
    let (continue_tx, continue_rx) = std::sync::mpsc::channel();
    let old_transition_state = Arc::clone(&state);
    let old_transition = std::thread::spawn(move || {
        let observed_turn_epoch = old_transition_state
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
    assert_eq!(state.session_states.get(child_id).unwrap().turn_epoch, 1);
    assert_eq!(
        state
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

    assert_eq!(state.session_states.get(session_id).unwrap().turn_epoch, 1);
    assert_new_turn_silence_evidence(&state.silence_states.get(session_id).unwrap().lock());
    assert_eq!(
        state
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY,
        "an explicit idle marker observed before the new input must be discarded"
    );
}

#[test]
fn timer_idle_evidence_from_prior_turn_cannot_mutate_new_submission() {
    use std::sync::atomic::Ordering;

    for (session_id, activity) in [
        ("ready-idle-epoch", AgentScreenActivity::Ready),
        ("interrupted-idle-epoch", AgentScreenActivity::Interrupted),
        ("unknown-idle-epoch", AgentScreenActivity::Unknown),
    ] {
        let state = crate::state::tests_support::make_test_app_state();
        agent_session(&state, session_id, SHELL_BUSY);
        let evidence_turn_epoch = Some(0);
        note_submitted_input(&state, session_id);

        let transition = try_timer_idle_transition(
            &state,
            &state.silence_states.get(session_id).unwrap().clone(),
            session_id,
            activity,
            Some("claude"),
            evidence_turn_epoch,
        );

        assert!(!transition.transitioned);
        assert_new_turn_silence_evidence(&state.silence_states.get(session_id).unwrap().lock());
        assert_eq!(
            state
                .shell_states
                .get(session_id)
                .unwrap()
                .load(Ordering::Acquire),
            SHELL_BUSY
        );
    }
}

#[test]
fn silence_idle_decision_from_prior_turn_cannot_idle_new_submission() {
    use std::sync::atomic::{AtomicU64, Ordering};

    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "silence-idle-epoch";
    agent_session(&state, session_id, SHELL_BUSY);
    state.last_output_ms.insert(
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
    assert_eq!(state.session_states.get(session_id).unwrap().turn_epoch, 1);
    assert_new_turn_silence_evidence(&state.silence_states.get(session_id).unwrap().lock());
    assert_eq!(
        state
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
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    let observed_turn_epoch = state
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
                let silence = state.silence_states.get(child_id).unwrap().clone();
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
fn submitted_input_lifecycle_ready_before_status_line_is_not_stale_completed() {
    let state = crate::state::tests_support::make_test_app_state();
    completed_agent_session(&state, "quick-turn");
    state.pending_injections.insert(
        "quick-turn".to_string(),
        std::collections::VecDeque::from([crate::state::PendingInjection::peer_message(
            "quick follow up",
        )]),
    );
    flush_one_pending_as_submitted(&state, "quick-turn");

    state
        .silence_states
        .get("quick-turn")
        .unwrap()
        .lock()
        .confirm_idle();
    assert!(try_shell_transition(
        &state,
        "quick-turn",
        SHELL_BUSY,
        SHELL_IDLE,
        false,
    ));

    let snapshot = state.session_state_with_shell("quick-turn").unwrap();
    assert_eq!(snapshot.shell_state.as_deref(), Some("idle"));
    assert_eq!(snapshot.agent_state.as_deref(), Some("idle"));
    assert!(snapshot.suggested_actions.is_none());
}

#[test]
fn submitted_input_lifecycle_no_new_input_retains_completion() {
    let state = crate::state::tests_support::make_test_app_state();
    completed_agent_session(&state, "unchanged");

    let snapshot = state.session_state_with_shell("unchanged").unwrap();
    assert_eq!(snapshot.shell_state.as_deref(), Some("idle"));
    assert_eq!(snapshot.agent_state.as_deref(), Some("completed"));
    assert_eq!(
        snapshot.suggested_actions,
        Some(vec!["Review result".to_string()])
    );
    assert!(
        state
            .silence_states
            .get("unchanged")
            .unwrap()
            .lock()
            .completion_declared()
    );
}

#[test]
fn should_inject_now_only_for_idle_agent() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "idle-agent", SHELL_IDLE);
    agent_session(&state, "busy-agent", SHELL_BUSY);
    assert!(
        should_inject_now(&state, "idle-agent"),
        "idle agent → inject"
    );
    assert!(
        !should_inject_now(&state, "busy-agent"),
        "busy agent → queue"
    );
    assert!(
        !should_inject_now(&state, "unknown"),
        "unknown session → never inject"
    );
}

#[test]
fn injection_claim_rechecks_idle_atomically_after_delivery_decision() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "race-agent", SHELL_IDLE);
    assert!(should_inject_now(&state, "race-agent"));

    assert!(try_shell_transition(
        &state,
        "race-agent",
        SHELL_IDLE,
        SHELL_BUSY,
        false,
    ));

    assert!(
        claim_idle_for_injection(&state, "race-agent").is_none(),
        "a sender that observed idle before the agent became busy must queue instead of writing into the active composer"
    );
}

/// Typing into an idle agent must block injection outright — and a rejected
/// claim must leave the shell atom exactly as it found it, so the user's
/// half-typed line is never followed by a stray busy state. The post-CAS
/// re-check inside `claim_idle_for_injection` uses this same predicate for
/// the case where typing starts after the delivery decision.
#[test]
fn injection_claim_is_refused_while_the_user_is_typing() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "typing-agent", SHELL_IDLE);
    assert!(should_inject_now(&state, "typing-agent"));

    let mut buffer = InputLineBuffer::new();
    buffer.feed("half typed prompt");
    state
        .input_buffers
        .insert("typing-agent".to_string(), parking_lot::Mutex::new(buffer));

    assert!(has_partial_user_input(&state, "typing-agent"));
    assert!(!should_inject_now(&state, "typing-agent"));
    assert!(
        claim_idle_for_injection(&state, "typing-agent").is_none(),
        "a partially typed composer must never be written into"
    );
    assert_eq!(
        state
            .shell_states
            .get("typing-agent")
            .map(|a| a.load(std::sync::atomic::Ordering::Relaxed)),
        Some(SHELL_IDLE),
        "a refused claim must not leave the session marked busy"
    );
}

#[cfg(unix)]
#[test]
fn agent_submission_rejects_partial_composer_without_writing() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "submit-partial", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "submit-partial");
    let mut buffer = InputLineBuffer::new();
    buffer.feed("Boss draft");
    state.input_buffers.insert(
        "submit-partial".to_string(),
        parking_lot::Mutex::new(buffer),
    );

    assert_eq!(
        write_agent_submission_to_pty(&state, "submit-partial", "new command"),
        AgentSubmissionWrite::Rejected {
            reason: "partial_composer",
            composer_state: "partial",
        }
    );
    assert!(bytes.lock().unwrap().is_empty());
    assert_eq!(
        state
            .input_buffers
            .get("submit-partial")
            .unwrap()
            .lock()
            .content(),
        "Boss draft",
        "a receipt request must preserve the user's draft verbatim"
    );
}

#[cfg(unix)]
#[test]
fn agent_submission_does_not_overtake_existing_queue() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "submit-queued", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "submit-queued");
    state
        .pending_injections
        .entry("submit-queued".to_string())
        .or_default()
        .push_back(crate::state::PendingInjection::peer_message("older peer"));

    assert_eq!(
        write_agent_submission_to_pty(&state, "submit-queued", "new command"),
        AgentSubmissionWrite::Rejected {
            reason: "queued_commands_pending",
            composer_state: "empty",
        }
    );
    assert!(bytes.lock().unwrap().is_empty());
    assert_eq!(
        state
            .pending_injections
            .get("submit-queued")
            .unwrap()
            .front()
            .map(crate::state::PendingInjection::text),
        Some("older peer")
    );
}

#[cfg(unix)]
#[test]
fn agent_submission_claim_prevents_concurrent_peer_splicing() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    agent_session(&state, "submit-race", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "submit-race");
    let submit_state = Arc::clone(&state);
    let submit = std::thread::spawn(move || {
        write_agent_submission_to_pty(&submit_state, "submit-race", "atomic command")
    });

    for _ in 0..100 {
        if !bytes.lock().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let peer = deliver_message_to_pty(&state, "submit-race", "peer command");
    let submitted = submit.join().unwrap();

    assert!(matches!(submitted, AgentSubmissionWrite::Complete { .. }));
    assert_eq!(peer, PtyDelivery::Queued);
    assert_eq!(
        String::from_utf8(bytes.lock().unwrap().clone()).unwrap(),
        "\u{15}atomic command\r",
        "the peer payload must not land between the split payload and Enter"
    );
    assert_eq!(
        state
            .pending_injections
            .get("submit-race")
            .unwrap()
            .front()
            .map(crate::state::PendingInjection::text),
        Some("peer command")
    );
}

#[cfg(unix)]
#[test]
fn agent_submission_writer_lock_prevents_raw_input_splicing() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    agent_session(&state, "submit-raw-race", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "submit-raw-race");
    let submit_state = Arc::clone(&state);
    let submit = std::thread::spawn(move || {
        write_agent_submission_to_pty(&submit_state, "submit-raw-race", "atomic command")
    });

    for _ in 0..100 {
        if !bytes.lock().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let raw_state = Arc::clone(&state);
    let raw = std::thread::spawn(move || {
        let writer = raw_state.pty_writer("submit-raw-race").unwrap();
        let mut writer = writer.lock();
        writer.write_all(b"raw input").unwrap();
        writer.flush().unwrap();
    });
    let submitted = submit.join().unwrap();
    raw.join().unwrap();

    assert!(matches!(submitted, AgentSubmissionWrite::Complete { .. }));
    assert_eq!(
        String::from_utf8(bytes.lock().unwrap().clone()).unwrap(),
        "\u{15}atomic command\rraw input",
        "a raw writer may follow the submission but cannot land before its Enter"
    );
}

/// `INJECT_ENTER_GAP` is 50 ms of REAL time that cannot be shortened, and it is
/// held under the session writer mutex on purpose — `agent_submission_writer_
/// lock_prevents_raw_input_splicing` pins that exact byte sequence. So the only
/// way a caller stops paying it is to stop being the thread that waits.
/// `flush_pending_injections` runs on the session-state accumulator and on the
/// silence timer, both tokio workers; it must hand the write to the injection
/// worker and return, while the queued message still reaches the composer whole.
#[cfg(unix)]
#[test]
fn flush_hands_the_enter_gap_to_the_injection_worker_not_the_caller() {
    use std::collections::VecDeque;
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    agent_session(&state, "detached-flush", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "detached-flush");
    let mut queue = VecDeque::new();
    queue.push_back(crate::state::PendingInjection::peer_message("wake up"));
    state
        .pending_injections
        .insert("detached-flush".to_string(), queue);

    let started = std::time::Instant::now();
    flush_pending_injections(&state, "detached-flush");
    let returned_in = started.elapsed();
    assert!(
        returned_in < INJECT_ENTER_GAP / 2,
        "the caller must not wait out the injection's Enter gap; returned in {returned_in:?}"
    );

    // Deferred, not dropped: the same framing must still land, payload then Enter.
    for _ in 0..300 {
        if bytes.lock().unwrap().ends_with(b"\r") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        String::from_utf8(bytes.lock().unwrap().clone()).unwrap(),
        "\u{15}wake up\r",
        "the deferred injection must be byte-identical to the inline one"
    );
    assert_eq!(
        state
            .pending_injections
            .get("detached-flush")
            .map(|queue| queue.len()),
        Some(0),
        "a delivered message must not stay queued"
    );
}

#[test]
fn codex_heuristic_idle_is_not_safe_for_injection_or_standby() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "codex-heuristic", SHELL_IDLE);
    state
        .session_states
        .get_mut("codex-heuristic")
        .unwrap()
        .agent_type = Some("codex".to_string());
    state
        .silence_states
        .get("codex-heuristic")
        .unwrap()
        .lock()
        .idle_confirmed = false;

    assert!(!idle_is_confirmed(&state, "codex-heuristic"));
    assert!(!should_inject_now(&state, "codex-heuristic"));

    state
        .silence_states
        .get("codex-heuristic")
        .unwrap()
        .lock()
        .confirm_idle();
    assert!(idle_is_confirmed(&state, "codex-heuristic"));
    assert!(should_inject_now(&state, "codex-heuristic"));
}

#[test]
fn should_inject_now_false_for_shell_and_confident_question() {
    use std::sync::atomic::AtomicU8;
    let state = crate::state::tests_support::make_test_app_state();
    // Plain shell (no agent_type) — must never be injected into.
    state
        .shell_states
        .insert("shell".to_string(), AtomicU8::new(SHELL_IDLE));
    state
        .session_states
        .insert("shell".to_string(), crate::state::SessionState::default());
    assert!(!should_inject_now(&state, "shell"), "shell → never inject");

    // Agent idle but blocked on a CONFIDENT user-facing question (Ink menu,
    // cliclack prompt, "Action Required" title) — never answer it.
    state
        .shell_states
        .insert("q".to_string(), AtomicU8::new(SHELL_IDLE));
    state.session_states.insert(
        "q".to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            awaiting_input: true,
            question_confident: true,
            ..Default::default()
        },
    );
    let mut ready_silence = SilenceState::new();
    ready_silence.idle_confirmed = true;
    state
        .silence_states
        .insert("ready".to_string(), Arc::new(Mutex::new(ready_silence)));
    assert!(
        !should_inject_now(&state, "q"),
        "confident question agent → do not answer its prompt"
    );

    // Agent idle at a mere ready prompt: the low-confidence silence heuristic
    // sets awaiting_input WITHOUT question_confident (codex parks here
    // permanently — story 091). Injection must proceed or delivery starves.
    state
        .shell_states
        .insert("ready".to_string(), AtomicU8::new(SHELL_IDLE));
    state.session_states.insert(
        "ready".to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            awaiting_input: true,
            question_confident: false,
            ..Default::default()
        },
    );
    assert!(
        should_inject_now(&state, "ready"),
        "awaiting_input-only (ready prompt) agent → inject, do not starve"
    );
}

#[test]
fn deliver_queues_pending_for_busy_agent() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "busy", SHELL_BUSY);
    let outcome = deliver_message_to_pty(&state, "busy", "[TUIC message from lead] go");
    assert_eq!(
        outcome,
        PtyDelivery::Queued,
        "a busy composer parks the message; nothing reached the terminal"
    );
    let q = state.pending_injections.get("busy").expect("queued");
    assert_eq!(q.len(), 1);
    assert_eq!(
        q.front().map(crate::state::PendingInjection::text),
        Some("[TUIC message from lead] go")
    );
}

/// A live PTY whose every byte is recorded, so a test can assert both what
/// reached the composer and what deliberately did not.
#[cfg(unix)]
fn insert_recording_session(state: &AppState, session_id: &str) -> Arc<std::sync::Mutex<Vec<u8>>> {
    let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        state,
        session_id,
        Box::new(RecordingWriter {
            bytes: Arc::clone(&bytes),
        }),
    );
    bytes
}

/// Compose enqueue on a busy agent: nothing may reach the composer, or the
/// user's queued note would steer the turn they deliberately did not interrupt.
#[cfg(unix)]
#[test]
fn enqueue_parks_command_while_agent_is_busy() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "busy", SHELL_BUSY);
    let bytes = insert_recording_session(&state, "busy");

    let first = enqueue_user_command(&state, "busy", "run the tests").expect("enqueued");
    assert_eq!((first.typed, first.queued), (false, 1));
    let second = enqueue_user_command(&state, "busy", "then push").expect("enqueued");
    assert_eq!((second.typed, second.queued), (false, 2));

    let queue = state.pending_injections.get("busy").expect("queue");
    assert_eq!(
        queue.iter().map(|entry| entry.text()).collect::<Vec<_>>(),
        vec!["run the tests", "then push"],
        "queued in the order the user composed them"
    );
    assert!(queue.iter().all(|entry| entry.is_user_command()));
    assert_eq!(
        state
            .session_state_with_shell("busy")
            .expect("snapshot")
            .queued_commands,
        2,
        "queue depth is visible to the polling UI"
    );
    assert!(
        bytes.lock().unwrap().is_empty(),
        "a busy composer receives nothing"
    );
}

#[cfg(unix)]
#[test]
fn enqueue_refuses_shells_and_dead_sessions() {
    use std::sync::atomic::AtomicU8;
    let state = crate::state::tests_support::make_test_app_state();
    state
        .shell_states
        .insert("shell".to_string(), AtomicU8::new(SHELL_IDLE));
    state
        .session_states
        .insert("shell".to_string(), crate::state::SessionState::default());
    insert_recording_session(&state, "shell");
    assert_eq!(
        enqueue_user_command(&state, "shell", "ls").unwrap_err(),
        "Session is not running an agent"
    );

    agent_session(&state, "gone", SHELL_IDLE);
    assert_eq!(
        enqueue_user_command(&state, "gone", "hi").unwrap_err(),
        "Session not found",
        "a tombstoned agent still has session_states — the PTY is what decides"
    );

    agent_session(&state, "blank", SHELL_IDLE);
    insert_recording_session(&state, "blank");
    assert_eq!(
        enqueue_user_command(&state, "blank", "   \n ").unwrap_err(),
        "Command text is empty"
    );
    assert_eq!(queued_command_count(&state, "blank"), 0);
}

#[cfg(unix)]
#[test]
fn clear_queued_commands_preserves_peer_deliveries() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "busy", SHELL_BUSY);
    insert_recording_session(&state, "busy");
    enqueue_user_command(&state, "busy", "one").expect("enqueued");
    state.pending_injections.get_mut("busy").unwrap().push_back(
        crate::state::PendingInjection::peer_message("[TUIC message from lead] first peer"),
    );
    enqueue_user_command(&state, "busy", "two").expect("enqueued");
    state.pending_injections.get_mut("busy").unwrap().push_back(
        crate::state::PendingInjection::peer_message("[TUIC message from worker] second peer"),
    );

    assert_eq!(queued_command_count(&state, "busy"), 2);
    assert_eq!(clear_queued_commands(&state, "busy"), 2);
    assert_eq!(queued_command_count(&state, "busy"), 0);
    assert_eq!(
        state.pending_injections.get("busy").map(|queue| queue
            .iter()
            .map(|entry| entry.text().to_string())
            .collect::<Vec<_>>()),
        Some(vec![
            "[TUIC message from lead] first peer".to_string(),
            "[TUIC message from worker] second peer".to_string(),
        ]),
        "Compose clear must retain peer delivery ownership and order"
    );
    assert_eq!(
        clear_queued_commands(&state, "busy"),
        0,
        "clearing an empty queue is a no-op, not an error"
    );
}

/// The Compose panel lists what waits and deletes one entry: peer wake
/// messages stay invisible and untouchable, exactly as for count and clear.
#[cfg(unix)]
#[test]
fn list_and_remove_expose_only_user_commands() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "busy", SHELL_BUSY);
    insert_recording_session(&state, "busy");
    enqueue_user_command(&state, "busy", "one").expect("enqueued");
    state.pending_injections.get_mut("busy").unwrap().push_back(
        crate::state::PendingInjection::peer_message("[TUIC message from lead] peer"),
    );
    enqueue_user_command(&state, "busy", "two").expect("enqueued");

    let listed = list_queued_commands(&state, "busy");
    assert_eq!(
        listed.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
        vec!["one", "two"],
        "listed in delivery order, peer message excluded"
    );

    assert!(remove_queued_command(&state, "busy", listed[0].id));
    assert_eq!(
        list_queued_commands(&state, "busy")
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>(),
        vec!["two"],
    );
    assert_eq!(
        state
            .pending_injections
            .get("busy")
            .map(|queue| queue.len()),
        Some(2),
        "the peer message survives a Compose delete"
    );
    assert!(
        !remove_queued_command(&state, "busy", listed[0].id),
        "removing an id that already drained is a no-op, not an error"
    );
}

/// The idle path, end to end against a real PTY: an idle agent gets the text
/// typed and submitted at once (Ctrl-U prefix, CR in a separate write), so
/// enqueueing costs nothing when there is no turn to protect.
#[cfg(unix)]
#[test]
fn enqueue_types_immediately_when_agent_is_idle() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "idle-now", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "idle-now");

    let outcome = enqueue_user_command(&state, "idle-now", "ship it").expect("enqueued");
    assert_eq!((outcome.typed, outcome.queued), (true, 0));
    assert_eq!(
        String::from_utf8(bytes.lock().unwrap().clone()).unwrap(),
        "\u{15}ship it\r"
    );
}

/// FIFO under the idle path: a user command enqueued behind a peer delivery
/// must not jump ahead of it. The flush types the shared head and leaves the
/// session busy, so the user command stays parked for the next idle window.
#[cfg(unix)]
#[test]
fn enqueue_never_overtakes_a_command_already_waiting() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "fifo", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "fifo");
    state
        .pending_injections
        .entry("fifo".to_string())
        .or_default()
        .push_back(crate::state::PendingInjection::peer_message("first"));

    let outcome = enqueue_user_command(&state, "fifo", "second").expect("enqueued");
    assert_eq!((outcome.typed, outcome.queued), (false, 1));
    assert_eq!(
        String::from_utf8(bytes.lock().unwrap().clone()).unwrap(),
        "\u{15}first\r",
        "the older command is the one that reached the composer"
    );
    assert_eq!(
        state
            .pending_injections
            .get("fifo")
            .expect("queue")
            .front()
            .map(crate::state::PendingInjection::text),
        Some("second")
    );
}

/// The defect this pair pins: `deliver_message_to_managed_pty` used to return
/// `state.sessions.contains_key(session_id)` — "the session exists", not "the
/// message was typed". Every call site read that as delivery and marked the
/// message `TerminalDispatched`, and the waiter filter hides Terminal-owned
/// messages, so a queued-but-never-typed message became invisible to
/// `agent wait` while still sitting unread in the inbox.
#[test]
fn queued_message_is_not_claimed_as_dispatched() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "busy-peer", SHELL_BUSY);
    let msg = "m-queued";
    assert_eq!(
        state.assign_agent_delivery("busy-peer", msg, true),
        crate::state::AgentDeliveryAssignment::Terminal
    );

    let outcome = deliver_message_to_pty(&state, "busy-peer", "[TUIC message from lead] go");
    settle_terminal_delivery(&state, "busy-peer", msg, outcome);

    assert_eq!(outcome, PtyDelivery::Queued);
    assert_eq!(
        state.agent_delivery_owner("busy-peer", msg),
        Some(crate::state::AgentDeliveryOwner::TerminalPending),
        "still owned by the terminal — but pending, never dispatched"
    );
}

/// `settle_terminal_delivery` is the decision this fix introduced, so pin all
/// three branches directly. Driving `Typed` end-to-end would need a live PTY
/// (without one the claim path always reports `NotStarted` and requeues — see
/// `ready_prompt_delivery_attempts_and_requeues_when_pty_is_missing`), and a
/// fake PTY would only prove the fake.
#[test]
fn settle_maps_each_outcome_to_the_right_ownership() {
    let state = crate::state::tests_support::make_test_app_state();
    for (msg, outcome, expected) in [
        (
            "m-typed",
            PtyDelivery::Typed,
            Some(crate::state::AgentDeliveryOwner::TerminalDispatched),
        ),
        (
            "m-queued",
            PtyDelivery::Queued,
            Some(crate::state::AgentDeliveryOwner::TerminalPending),
        ),
        ("m-gone", PtyDelivery::Unavailable, None),
    ] {
        assert_eq!(
            state.assign_agent_delivery("peer", msg, true),
            crate::state::AgentDeliveryAssignment::Terminal
        );
        settle_terminal_delivery(&state, "peer", msg, outcome);
        assert_eq!(
            state.agent_delivery_owner("peer", msg),
            expected,
            "{outcome:?} must not claim more or less than it achieved"
        );
    }
}

#[test]
fn dead_session_reports_unavailable_and_releases_ownership() {
    let state = crate::state::tests_support::make_test_app_state();
    let msg = "m-dead";
    assert_eq!(
        state.assign_agent_delivery("ghost-peer", msg, true),
        crate::state::AgentDeliveryAssignment::Terminal
    );

    // No PTY was ever registered for this id.
    let outcome = deliver_message_to_managed_pty(&state, "ghost-peer", "[TUIC] hi");
    settle_terminal_delivery(&state, "ghost-peer", msg, outcome);

    assert_eq!(outcome, PtyDelivery::Unavailable);
    assert_eq!(
        state.agent_delivery_owner("ghost-peer", msg),
        None,
        "ownership handed back so `agent wait` can still surface the inbox copy"
    );
}

#[test]
fn idle_flush_submits_only_one_queued_message_per_turn() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "idle", SHELL_IDLE);
    state.pending_injections.insert(
        "idle".to_string(),
        std::collections::VecDeque::from([
            crate::state::PendingInjection::peer_message("first"),
            crate::state::PendingInjection::peer_message("second"),
        ]),
    );

    flush_one_pending_as_submitted(&state, "idle");

    assert_eq!(
        state.pending_injections.get("idle").map(|queue| queue
            .iter()
            .map(|entry| entry.text().to_string())
            .collect::<Vec<_>>()),
        Some(vec!["second".to_string()]),
        "submitting the first message makes the agent busy; later messages must wait for its next idle transition"
    );
}

#[test]
fn deliver_queues_for_idle_agent_with_partial_user_input() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "typing", SHELL_IDLE);
    let mut input = crate::input_line_buffer::InputLineBuffer::new();
    input.feed("draft in progress");
    state
        .input_buffers
        .insert("typing".to_string(), Mutex::new(input));

    assert!(
        !should_inject_now(&state, "typing"),
        "partial composer input must block terminal injection"
    );
    deliver_message_to_pty(&state, "typing", "[TUIC message from child] done");
    let pending = state.pending_injections.get("typing").unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending.front().map(crate::state::PendingInjection::text),
        Some("[TUIC message from child] done")
    );
}

#[test]
fn delivery_gate_assigns_waiter_without_touching_terminal_queue() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "waiting", SHELL_IDLE);
    let lease = state.begin_agent_wait("waiting");
    state.push_agent_inbox(
        "waiting",
        crate::state::AgentMessage {
            id: "wait-owned".to_string(),
            from_tuic_session: "sender".to_string(),
            from_name: "sender".to_string(),
            content: "done".to_string(),
            timestamp: 1,
            delivered_via_channel: false,
        },
    );

    assert_eq!(
        state.assign_agent_delivery("waiting", "wait-owned", true),
        crate::state::AgentDeliveryAssignment::Waiter
    );

    assert!(
        !state.pending_injections.contains_key("waiting"),
        "active wait owns delivery; terminal injection must not be queued"
    );
    state.finish_agent_wait("waiting", lease, 0, true);
}

#[test]
fn deliver_noop_for_non_agent() {
    let state = crate::state::tests_support::make_test_app_state();
    // No session_states entry → not an agent.
    deliver_message_to_pty(&state, "ghost", "hi");
    assert!(
        !state.pending_injections.contains_key("ghost"),
        "non-agent must never queue"
    );
}

#[test]
fn managed_delivery_rejects_stale_agent_state_without_pty() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "vanished", SHELL_BUSY);

    assert_eq!(
        deliver_message_to_managed_pty(&state, "vanished", "message"),
        PtyDelivery::Unavailable
    );
    assert!(!state.pending_injections.contains_key("vanished"));
}

#[test]
fn failed_not_started_injection_rolls_back_claim_and_requeues() {
    // The test state has no live PTY session, so composer lookup fails before
    // any byte can be written. The delivery claim must be rolled back and the
    // message kept pending instead of leaving a false BUSY state.
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "idle", SHELL_IDLE);
    deliver_message_to_pty(&state, "idle", "now");
    assert!(
        state
            .shell_states
            .get("idle")
            .is_some_and(|state| state.load(Ordering::Acquire) == SHELL_IDLE),
        "a claim that never reached PTY I/O must restore IDLE"
    );
    assert_eq!(
        state
            .pending_injections
            .get("idle")
            .and_then(|queue| queue.front().map(|entry| entry.text().to_string())),
        Some("now".to_string()),
        "a not-started delivery must remain retryable"
    );
}

#[test]
fn real_activity_invalidates_injection_rollback_ownership() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "active", SHELL_IDLE);
    let claim = claim_idle_for_injection(&state, "active").expect("claim");
    state
        .silence_states
        .get("active")
        .unwrap()
        .lock()
        .note_real_activity();

    assert!(!rollback_injection_claim(&state, "active", claim));
    assert!(
        state
            .shell_states
            .get("active")
            .is_some_and(|value| value.load(Ordering::Acquire) == SHELL_BUSY),
        "rollback must not erase genuine post-claim activity"
    );
}

#[test]
fn partial_write_is_uncertain_not_not_started() {
    struct PartialThenError {
        wrote_once: bool,
    }
    impl std::io::Write for PartialThenError {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.wrote_once {
                Err(std::io::Error::other("injected failure"))
            } else {
                self.wrote_once = true;
                Ok(bytes.len().min(2))
            }
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut writer = PartialThenError { wrote_once: false };
    let failure = write_all_with_progress(&mut writer, b"payload", 0).unwrap_err();
    assert_eq!(failure.0, 2, "partial progress must be retained");
    assert!(failure.1.contains("injected failure"));
}

#[cfg(unix)]
struct RecordingWriter {
    bytes: Arc<std::sync::Mutex<Vec<u8>>>,
}

#[cfg(unix)]
impl std::io::Write for RecordingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(unix)]
struct FailingWriter;

#[cfg(unix)]
impl std::io::Write for FailingWriter {
    fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("injected PTY failure"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(unix)]
fn insert_session_with_writer(
    state: &AppState,
    session_id: &str,
    writer: Box<dyn std::io::Write + Send>,
) {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    let mut command = CommandBuilder::new("/bin/sh");
    command.args(["-c", "sleep 30"]);
    let child = pair.slave.spawn_command(command).expect("spawn shell");
    state.sessions.insert(
        session_id.to_string(),
        Mutex::new(PtySession {
            writer: Arc::new(Mutex::new(writer)),
            master: pair.master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            is_remote: false,
            shell: "/bin/sh".to_string(),
        }),
    );
}

#[cfg(unix)]
#[test]
fn concurrent_user_input_and_terminal_reply_are_both_serialized() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        &state,
        "serialized-writes",
        Box::new(RecordingWriter {
            bytes: Arc::clone(&bytes),
        }),
    );

    let writer = state.pty_writer("serialized-writes").unwrap();
    let held = writer.lock();
    let input_state = Arc::clone(&state);
    let input = std::thread::spawn(move || {
        input_state
            .write_pty_parts("serialized-writes", &[b"input"])
            .unwrap();
    });
    let reply_state = Arc::clone(&state);
    let reply = std::thread::spawn(move || {
        write_terminal_reply(&reply_state, "serialized-writes", b"reply", "test");
    });

    drop(held);
    input.join().unwrap();
    reply.join().unwrap();
    let output = bytes.lock().unwrap().clone();
    assert!(
        output == b"inputreply" || output == b"replyinput",
        "{output:?}"
    );
}

#[cfg(unix)]
#[test]
fn multiple_terminal_replies_keep_reader_order() {
    let state = crate::state::tests_support::make_test_app_state();
    let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        &state,
        "reply-order",
        Box::new(RecordingWriter {
            bytes: Arc::clone(&bytes),
        }),
    );

    write_terminal_reply(&state, "reply-order", b"first", "test");
    write_terminal_reply(&state, "reply-order", b"second", "test");
    assert_eq!(*bytes.lock().unwrap(), b"firstsecond");
}

#[cfg(unix)]
#[test]
fn shared_pty_write_reports_writer_failure_and_teardown() {
    let state = crate::state::tests_support::make_test_app_state();
    insert_session_with_writer(&state, "failed-writer", Box::new(FailingWriter));

    let error = state
        .write_pty_parts("failed-writer", &[b"reply"])
        .expect_err("writer error must be observable");
    assert!(error.contains("injected PTY failure"), "{error}");

    state.sessions.remove("failed-writer");
    let error = state
        .write_pty_parts("failed-writer", &[b"late reply"])
        .expect_err("removed session must reject writes");
    assert_eq!(error, "Session not found");
    write_terminal_reply(&state, "failed-writer", b"late reply", "test");
}

#[test]
fn uncertain_injection_preserves_busy_and_surfaces_status_flag() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "uncertain", SHELL_IDLE);
    let claim = claim_idle_for_injection(&state, "uncertain").expect("claim");
    mark_injection_uncertain(&state, "uncertain", claim);

    assert!(
        state
            .shell_states
            .get("uncertain")
            .is_some_and(|value| value.load(Ordering::Acquire) == SHELL_BUSY)
    );
    assert!(
        state
            .silence_states
            .get("uncertain")
            .unwrap()
            .lock()
            .injection_delivery_uncertain
    );
    assert!(!state.pending_injections.contains_key("uncertain"));
}

#[test]
fn uncertain_injection_cannot_be_cleared_by_a_stale_ready_screen() {
    let mut silence = SilenceState::new();
    let token = silence.begin_injection_claim(true);
    silence.mark_injection_uncertain(token);
    silence.ready_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);

    assert!(!silence.note_ready_screen());
    assert!(silence.injection_delivery_uncertain);
    assert!(!silence.idle_confirmed);
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
            deliver_message_to_pty(&s1, "race", "[TUIC message from lead] go");
        });
        let (s2, b2) = (Arc::clone(&state), Arc::clone(&barrier));
        let timer = std::thread::spawn(move || {
            b2.wait();
            // Silence timer: BUSY→IDLE also runs flush_pending_injections on idle.
            s2.silence_states.get("race").unwrap().lock().idle_confirmed = true;
            try_shell_transition(&s2, "race", SHELL_BUSY, SHELL_IDLE, false);
            emit_shell_state(&s2, "race", "idle");
            flush_pending_injections(&s2, "race");
        });
        sender.join().unwrap();
        timer.join().unwrap();
        // The timer's flush is dispatched to the injection worker, so joining the
        // thread only proves it was enqueued. Drain before counting, or the next
        // iteration's enqueue lands on top of a flush that never ran.
        wait_for_injection_queue();

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
    state.vt_log_buffers.insert(
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
    let _ = resize_session_off_thread(&state, sid.to_string(), 40, 120).await;

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
    assert!(resize_session_core(&state, "s", 0, 80).is_err());
    assert!(resize_session_core(&state, "s", 24, 0).is_err());
    // A rejected resize must not even create a resize_locks entry.
    assert!(!state.resize_locks.contains_key("s"));
}

#[test]
fn resize_noop_guard_returns_none_on_matching_dims() {
    let state = crate::state::tests_support::make_test_app_state();
    // Pre-seed the last-applied dims, as if a prior resize reached the PTY.
    state
        .resize_locks
        .insert("s".to_string(), Arc::new(Mutex::new((24, 80))));
    // Same dims → no-op returning None WITHOUT touching the (absent) session.
    // Without the guard this would fall through to sessions.get and fail with
    // "Session not found", so Ok(None) proves the guard short-circuited first.
    assert_eq!(resize_session_core(&state, "s", 24, 80), Ok(None));
}

#[test]
fn resize_seeds_applied_from_grid_and_noops_at_startup_dims() {
    let state = crate::state::tests_support::make_test_app_state();
    // Grid exists at the startup dims but resize_locks is empty → the lock
    // opens at the (0,0) never-applied sentinel.
    seed_vt_grid(&state, "s", 24, 80);
    // A first resize matching only the startup dims must seed *applied from
    // the live grid and then no-op — no gratuitous SIGWINCH, no session touch.
    assert_eq!(resize_session_core(&state, "s", 24, 80), Ok(None));
    // The seed must have populated resize_locks with the live grid dims.
    assert_eq!(
        *state.resize_locks.get("s").unwrap().lock(),
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
    state.sessions.insert(
        sid.to_string(),
        Mutex::new(PtySession {
            writer: Arc::new(Mutex::new(writer)),
            master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
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
            let _ = resize_session_core(&s1, sid, A.0, A.1);
        });
        let (s2, b2) = (Arc::clone(&state), Arc::clone(&barrier));
        let t2 = std::thread::spawn(move || {
            b2.wait();
            let _ = resize_session_core(&s2, sid, B.0, B.1);
        });
        t1.join().unwrap();
        t2.join().unwrap();

        // The recorded applied dims, the live grid dims, and the real PTY size
        // must all agree, and agree on one of the two racing targets.
        let applied = *state.resize_locks.get(sid).unwrap().lock();
        let (grid_rows, grid_cols) = {
            let vt = state.vt_log_buffers.get(sid).unwrap();
            let vt = vt.lock();
            (vt.grid_screen_lines() as u16, vt.grid_columns() as u16)
        };
        let pty_size = state
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
    q.push_back(crate::state::PendingInjection::peer_message("msg-1"));
    q.push_back(crate::state::PendingInjection::peer_message("msg-2"));
    state.pending_injections.insert("sess".to_string(), q);

    // The transition is driven by verified ready-screen/Stop evidence in
    // production. Model that evidence before testing its delivery side effect.
    state
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
        .shell_states
        .insert("sess".to_string(), AtomicU8::new(SHELL_BUSY));
    state.session_states.insert(
        "sess".to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            awaiting_input: true,
            question_confident: true,
            ..Default::default()
        },
    );
    let mut q = VecDeque::new();
    q.push_back(crate::state::PendingInjection::peer_message("later"));
    state.pending_injections.insert("sess".to_string(), q);

    state.silence_states.insert(
        "sess".to_string(),
        Arc::new(Mutex::new(SilenceState::new())),
    );
    state
        .silence_states
        .get("sess")
        .unwrap()
        .lock()
        .confirm_idle();

    try_shell_transition(&state, "sess", SHELL_BUSY, SHELL_IDLE, false);
    emit_shell_state(&state, "sess", "idle");
    flush_pending_injections(&state, "sess");
    wait_for_injection_queue();
    assert_eq!(
        state.pending_injections.get("sess").map(|q| q.len()),
        Some(1),
        "must not answer a confident user prompt — keep queued until it clears"
    );

    // The question clears (user answered) while the session is already idle:
    // the unblock flush must drain the queue with no further transition.
    state
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

#[test]
fn injection_payload_single_line_ctrl_u_only() {
    // Single-line: Ctrl-U prefix clears pending input; no paste wrapper.
    assert_eq!(injection_payload("hello"), "\x15hello");
}

#[test]
fn injection_payload_multiline_bracketed_paste() {
    // Multiline MUST ride in a bracketed paste — raw newlines prefill an
    // Ink/codex TUI without submitting; the paste-end marker makes the
    // separately-written CR a genuine Enter (story 091, verified live).
    assert_eq!(
        injection_payload("line1\nline2"),
        "\x15\x1b[200~line1\nline2\x1b[201~"
    );
}

#[test]
fn flush_noop_while_busy() {
    // flush_pending_injections is self-guarded: a direct call against a busy
    // agent (e.g. the user-input unblock path firing while the agent already
    // went back to work) must leave the queue untouched.
    use std::collections::VecDeque;
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    agent_session(&state, "busy", SHELL_BUSY);
    let mut q = VecDeque::new();
    q.push_back(crate::state::PendingInjection::peer_message("later"));
    state.pending_injections.insert("busy".to_string(), q);

    flush_pending_injections(&state, "busy");
    wait_for_injection_queue();
    assert_eq!(
        state.pending_injections.get("busy").map(|q| q.len()),
        Some(1),
        "busy agent → flush must be a no-op"
    );
}

#[test]
fn ready_prompt_delivery_attempts_and_requeues_when_pty_is_missing() {
    // codex idles at its ready prompt with awaiting_input=true (low-confidence
    // silence heuristic) — delivery must inject, not queue forever (story 091).
    use std::sync::atomic::AtomicU8;
    let state = crate::state::tests_support::make_test_app_state();
    state
        .shell_states
        .insert("codex".to_string(), AtomicU8::new(SHELL_IDLE));
    state.session_states.insert(
        "codex".to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            awaiting_input: true,
            question_confident: false,
            ..Default::default()
        },
    );
    let mut silence = SilenceState::new();
    silence.confirm_idle();
    state
        .silence_states
        .insert("codex".to_string(), Arc::new(Mutex::new(silence)));
    deliver_message_to_pty(&state, "codex", "[TUIC message from lead] go");
    assert_eq!(
        state
            .pending_injections
            .get("codex")
            .and_then(|queue| queue.front().map(|entry| entry.text().to_string())),
        Some("[TUIC message from lead] go".to_string()),
        "ready-prompt delivery must stay retryable when PTY lookup fails"
    );
}

#[test]
fn state_change_to_parent_without_managed_pty_stays_inbox_only() {
    // Logical agent state alone is not proof of a managed PTY. A child state
    // change must remain available in the inbox without creating a phantom
    // terminal injection for an external peer.
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    agent_session(&state, "parent", SHELL_BUSY);
    state
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

#[test]
fn mark_session_exited_sends_single_exited_notification() {
    // F1/DATA-1: only one state_change("exited") must reach parent inbox on exit.
    // The BUSY→IDLE transition in the exit path uses notify_parent=false, so the
    // orchestrator must never see a spurious "idle" before "exited".
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let child_id = "child-exit-dedup";
    let parent_id = "parent-exit-dedup";

    state
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    let ss = crate::state::SessionState {
        agent_type: Some("claude".to_string()),
        ..Default::default()
    };
    state.session_states.insert(child_id.to_string(), ss);
    state.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );

    // Simulate exit path: transition (notify_parent=false) + mark_session_exited.
    try_shell_transition(&state, child_id, SHELL_BUSY, SHELL_IDLE, false);
    // mark_session_exited needs a sessions entry to attempt exit-code capture
    // (it's OK if there's none — it just skips the exit code).
    push_state_change_to_parent(
        &state,
        child_id,
        serde_json::json!({
            "type": "state_change",
            "state": "exited",
            "session_id": child_id,
            "exit_code": null,
        }),
    );

    let inbox = state
        .agent_inbox
        .get(parent_id)
        .expect("parent inbox must exist");
    assert_eq!(inbox.len(), 1, "inbox must have exactly one message");
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(
        content["state"], "exited",
        "the single message must be 'exited'"
    );
}

#[test]
fn tuic_osc_suggest_parsed_from_pty_stream() {
    use crate::terminal_grid::TerminalGrid;
    let mut grid = TerminalGrid::new(24, 80, 1000);
    grid.process(b"\x1b]7770;suggest=Fix bug|Run tests|Deploy\x07");
    let events = grid.drain_events();
    let tuic: Vec<_> = events
        .into_iter()
        .filter(|e| matches!(e, crate::terminal_grid::TermEvent::Tuic { .. }))
        .collect();
    assert_eq!(tuic.len(), 1);
    if let crate::terminal_grid::TermEvent::Tuic { verb, payload, .. } = &tuic[0] {
        assert_eq!(verb, "suggest");
        let items: Vec<String> = payload.split('|').map(|s| s.trim().to_string()).collect();
        assert_eq!(items, vec!["Fix bug", "Run tests", "Deploy"]);
    }
}

#[test]
fn tuic_osc_intent_with_title_parsed() {
    use crate::terminal_grid::TerminalGrid;
    let mut grid = TerminalGrid::new(24, 80, 1000);
    grid.process(b"\x1b]7770;intent=Refactoring auth (Auth)\x07");
    let events = grid.drain_events();
    let tuic: Vec<_> = events
        .into_iter()
        .filter(|e| matches!(e, crate::terminal_grid::TermEvent::Tuic { .. }))
        .collect();
    assert_eq!(tuic.len(), 1);
    if let crate::terminal_grid::TermEvent::Tuic { verb, payload, .. } = &tuic[0] {
        assert_eq!(verb, "intent");
        assert_eq!(payload, "Refactoring auth (Auth)");
    }
}

#[test]
fn tuic_osc_block_start_parsed() {
    use crate::terminal_grid::TerminalGrid;
    let mut grid = TerminalGrid::new(24, 80, 1000);
    grid.process(b"\x1b]7770;block=start\x07");
    let events = grid.drain_events();
    let tuic: Vec<_> = events
        .into_iter()
        .filter(|e| matches!(e, crate::terminal_grid::TermEvent::Tuic { .. }))
        .collect();
    assert_eq!(tuic.len(), 1);
    if let crate::terminal_grid::TermEvent::Tuic { verb, payload, .. } = &tuic[0] {
        assert_eq!(verb, "block");
        assert_eq!(payload, "start");
    }
}

#[test]
fn tuic_osc_block_end_with_exit_code_parsed() {
    let payload = "end;1".to_string();
    let (action, exit_code) = if let Some(rest) = payload.strip_prefix("end;") {
        ("end".to_string(), rest.parse::<i32>().ok())
    } else {
        (payload.clone(), None)
    };
    assert_eq!(action, "end");
    assert_eq!(exit_code, Some(1));
}

#[test]
fn tuic_osc_block_end_without_exit_code() {
    let payload = "end".to_string();
    let (action, exit_code) = if let Some(rest) = payload.strip_prefix("end;") {
        ("end".to_string(), rest.parse::<i32>().ok())
    } else {
        (payload.clone(), None)
    };
    assert_eq!(action, "end");
    assert_eq!(exit_code, None);
}

#[test]
fn tuic_osc_block_invalid_action_ignored() {
    let payload = "invalid".to_string();
    let (action, _exit_code) = if let Some(rest) = payload.strip_prefix("end;") {
        ("end".to_string(), rest.parse::<i32>().ok())
    } else {
        (payload.clone(), None)
    };
    let is_valid = action == "start" || action == "end";
    assert!(!is_valid, "invalid action should not produce an event");
}

#[test]
fn tuic_osc_state_transitions_shell_state() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-tuic-state";
    state.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));

    let proc = ChunkProcessor::new(None, None);
    proc.handle_tuic_state("busy", session_id, &state);

    let current = state
        .shell_states
        .get(session_id)
        .unwrap()
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(current, SHELL_BUSY);

    proc.handle_tuic_state("idle", session_id, &state);
    let current = state
        .shell_states
        .get(session_id)
        .unwrap()
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(current, SHELL_IDLE);
}

#[test]
fn tuic_osc_state_emits_shell_state_event() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-tuic-emit";
    state.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));

    let mut rx = state.event_bus.subscribe();

    let proc = ChunkProcessor::new(None, None);
    proc.handle_tuic_state("busy", session_id, &state);

    let evt = rx.try_recv();
    assert!(
        evt.is_ok(),
        "event_bus should have received a shell state event"
    );
    if let Ok(crate::state::AppEvent::PtyParsed {
        session_id: sid,
        parsed,
    }) = evt
    {
        assert_eq!(sid, session_id);
        assert_eq!(parsed["type"], "shell-state");
        assert_eq!(parsed["state"], "busy");
    } else {
        panic!("expected PtyParsed event with shell-state");
    }
}

#[test]
fn tuic_osc_state_unknown_verb_ignored() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-tuic-unknown";
    state.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );

    let proc = ChunkProcessor::new(None, None);
    proc.handle_tuic_state("thinking", session_id, &state);

    let current = state
        .shell_states
        .get(session_id)
        .unwrap()
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(
        current, SHELL_IDLE,
        "unknown state should not change shell_states"
    );
}

#[test]
fn tuic_state_awaiting_yields_confident_question() {
    match tuic_state_awaiting_event("awaiting", 0) {
        Some(ParsedEvent::Question {
            confident,
            prompt_text,
        }) => {
            assert!(confident, "hook awaiting must be a confident question");
            assert_eq!(prompt_text, "", "hook awaiting carries no prompt text");
        }
        other => panic!("expected confident Question, got {other:?}"),
    }
}

#[test]
fn tuic_state_busy_yields_userinput_clear_with_prompt_line() {
    // The busy transition's absolute prompt row (history_size + cursor row,
    // here 42) must reach the UserInput event so the frontend can mark the
    // user-prompt line on the scrollbar.
    match tuic_state_awaiting_event("busy", 42) {
        Some(ParsedEvent::UserInput { content, line }) => {
            assert_eq!(content, "", "busy clear must not overwrite last_prompt");
            assert_eq!(line, 42, "busy UserInput must carry the prompt row");
        }
        other => panic!("expected UserInput clear, got {other:?}"),
    }
}

#[test]
fn tuic_state_idle_yields_no_awaiting_event() {
    assert!(
        tuic_state_awaiting_event("idle", 0).is_none(),
        "idle only transitions shell_state; it pushes no awaiting event"
    );
}

#[test]
fn tuic_state_unknown_yields_no_awaiting_event() {
    assert!(
        tuic_state_awaiting_event("thinking", 0).is_none(),
        "unknown verb must push no awaiting event"
    );
}

#[test]
fn question_suppress_filters_only_questions_when_instrumented() {
    let q_low = ParsedEvent::Question {
        prompt_text: "?".into(),
        confident: false,
    };
    let q_high = ParsedEvent::Question {
        prompt_text: "Proceed?".into(),
        confident: true,
    };
    let other = ParsedEvent::UserInput {
        content: "hi".into(),
        line: -1,
    };
    // Instrumented: every Question (silence + regex) is suppressed.
    assert!(suppress_heuristic_question(true, &q_low));
    assert!(suppress_heuristic_question(true, &q_high));
    // Non-questions are never suppressed (idle/busy/etc. pass through).
    assert!(!suppress_heuristic_question(true, &other));
    // Not instrumented: nothing is suppressed.
    assert!(!suppress_heuristic_question(false, &q_low));
}

// --- Raw-stream OSC reassembly ----------------------------------------

/// The failure the fixture caught: a PTY read ends mid-`ESC]777;…`. Each
/// half alone matches nothing, so without a carry the awaiting signal is
/// lost — silently, which is how it reached Boss's screen.
#[test]
fn raw_stream_reassembles_an_osc_split_across_reads() {
    let whole = "\x1b]777;notify;Claude Code;Claude needs your permission\x07";
    for split in [1, 8, 20, whole.len() - 1] {
        let mut carry = String::new();
        let mut events = Vec::new();
        raw_stream_events(&mut carry, &whole[..split], &mut events);
        raw_stream_events(&mut carry, &whole[split..], &mut events);
        assert_eq!(
            awaiting_prompts(&events),
            vec!["Claude needs your permission".to_string()],
            "split at {split} must yield exactly one awaiting signal"
        );
    }
}

/// A sequence that arrived whole leaves nothing behind, so the next chunk
/// cannot re-match it. One notification, one badge.
#[test]
fn raw_stream_does_not_refire_a_complete_sequence() {
    let mut carry = String::new();
    let mut events = Vec::new();
    raw_stream_events(
        &mut carry,
        "\x1b]777;notify;Claude Code;Claude needs your permission\x07",
        &mut events,
    );
    assert!(carry.is_empty(), "complete sequence must not be carried");
    raw_stream_events(&mut carry, "ordinary output\r\n", &mut events);
    assert_eq!(awaiting_prompts(&events).len(), 1);
}

/// An OSC that never terminates (or one whose payload we do not parse, like
/// a long OSC 52 clipboard blob) must not pin memory for the session.
#[test]
fn raw_stream_carry_is_bounded() {
    let mut carry = String::new();
    let mut events = Vec::new();
    let huge = format!("\x1b]52;c;{}", "A".repeat(MAX_RAW_CARRY * 4));
    raw_stream_events(&mut carry, &huge, &mut events);
    assert!(
        carry.len() <= MAX_RAW_CARRY,
        "carry grew to {} bytes",
        carry.len()
    );
}

/// ST-terminated sequences close the carry too — otherwise every agent that
/// ends OSC with ESC-backslash would carry its whole stream forward.
#[test]
fn raw_stream_carry_recognises_st_termination() {
    assert!(unterminated_osc_tail("\x1b]777;notify;Codex;approval\x1b\\").is_empty());
    assert!(!unterminated_osc_tail("\x1b]777;notify;Codex;approval").is_empty());
}

// --- Awaiting-signal fixtures -----------------------------------------
//
// Captures of real agent output, replayed through the SAME composition the
// PTY hot path runs. Unit tests cover each parser in isolation; what kept
// breaking was the pipeline around them — which signals survive the hook
// suppression, and which never reach a parser at all. That gap is what a
// fixture closes: one file per observed failure, byte-for-byte off a live
// session, no mocks.
//
// Capturing a new one:
//   POST /diagnostics/capture with {"enabled":true,"session_id":"<id>"}
//   BEFORE reproducing, then POST {"enabled":false}. Copy the exact `.tcap`
//   file reported by GET /diagnostics/capture from the config-dir captures/
//   directory into src/fixtures/agent_prompts/. `/sessions/:id/output` is a
//   rendered/ring-buffer snapshot and is not valid raw-stream evidence.

fn agent_prompt_fixture(name: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/fixtures/agent_prompts")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("missing fixture {}: {e}", path.display()))
}

#[test]
fn historical_scenario_matrix_is_well_formed_and_fixture_backed() {
    let bytes = agent_prompt_fixture("scenario-matrix.json");
    let matrix: serde_json::Value = serde_json::from_slice(&bytes).expect("valid matrix JSON");
    let scenarios = matrix["scenarios"].as_array().expect("scenario array");
    assert!(
        scenarios
            .iter()
            .any(|scenario| scenario["agent"] == "claude")
    );
    assert!(
        scenarios
            .iter()
            .any(|scenario| scenario["agent"] == "codex")
    );

    let mut ids = std::collections::HashSet::new();
    for scenario in scenarios {
        let id = scenario["id"].as_str().expect("scenario id");
        assert!(ids.insert(id), "duplicate scenario id: {id}");
        let set = scenario["set"].as_str().expect("SET expectation");
        let clears = scenario["clear"].as_array().expect("CLEAR expectations");
        assert!(
            set == "none" || !clears.is_empty(),
            "every state SET needs at least one CLEAR path: {id}"
        );
        if scenario["source"] == "raw-fixture" {
            let fixture = scenario["fixture"].as_str().expect("raw fixture name");
            assert!(
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("src/fixtures/agent_prompts")
                    .join(fixture)
                    .is_file(),
                "matrix references missing fixture: {fixture}"
            );
        }
    }
}

/// What a replay saw on the way through, beyond the events it produced.
///
/// The counter that matters is `ticks_without_cutoff`: `find_chrome_cutoff`
/// returning `None` means NO trim, so on that tick every status-line row
/// reached every parser. It is the fail-open branch, it is silent, and the
/// only way to know how often it fires on a real agent is to count it over
/// real bytes.
#[derive(Default)]
struct CaptureStats {
    output_records: usize,
    input_records: usize,
    bytes: usize,
    /// Output ticks whose screen held at least one non-blank row.
    ticks_with_content: usize,
    /// …of those, the ticks where no chrome anchor was found.
    ticks_without_cutoff: usize,
    rows_offered: usize,
    rows_trimmed: usize,
}

/// Replay framed captures using their original PTY read/write boundaries.
/// Legacy `.raw` files decode as one output record; they retain parser value
/// but cannot prove boundary-sensitive or input-state behavior.
fn replay_capture(bytes: &[u8], hook_instrumented: bool) -> Vec<ParsedEvent> {
    replay_capture_measured(bytes, hook_instrumented).0
}

fn replay_capture_measured(
    bytes: &[u8],
    hook_instrumented: bool,
) -> (Vec<ParsedEvent>, CaptureStats) {
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(41, 128, 2000);
    let mut parser = crate::output_parser::OutputParser::new();
    let mut input = crate::input_line_buffer::InputLineBuffer::new();
    let mut carry = String::new();
    let mut events = Vec::new();
    let mut stats = CaptureStats::default();
    for record in crate::pty_capture::decode(bytes).expect("valid capture") {
        stats.bytes += record.data.len();
        match record.direction {
            crate::pty_capture::CaptureDirection::Output => {
                stats.output_records += 1;
                let mut changed = vt_log.process(&record.data);
                let offered = changed.len();
                let screen = vt_log.screen_rows();
                let refs: Vec<&str> = screen.iter().map(String::as_str).collect();
                let cutoff = crate::chrome::find_chrome_cutoff(&refs);
                if refs.iter().any(|row| !row.trim().is_empty()) {
                    stats.ticks_with_content += 1;
                    if cutoff.is_none() {
                        stats.ticks_without_cutoff += 1;
                    }
                }
                if let Some(cutoff) = cutoff {
                    changed.retain(|row| row.row_index < cutoff);
                }
                stats.rows_offered += offered;
                stats.rows_trimmed += offered - changed.len();
                let data = String::from_utf8_lossy(&record.data);
                raw_stream_events(&mut carry, &data, &mut events);
                events.extend(
                    parser
                        .parse_clean_lines(&changed, true)
                        .into_iter()
                        .filter(|e| !suppress_heuristic_question(hook_instrumented, e)),
                );
            }
            crate::pty_capture::CaptureDirection::Input => {
                stats.input_records += 1;
                if let Ok(text) = std::str::from_utf8(&record.data) {
                    for action in input.feed(text) {
                        match action {
                            crate::input_line_buffer::InputAction::Line(content) => {
                                events.push(ParsedEvent::UserInput { content, line: -1 });
                            }
                            crate::input_line_buffer::InputAction::Interrupt => {
                                events.push(ParsedEvent::UserInput {
                                    content: String::new(),
                                    line: -1,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    (events, stats)
}

/// An agent quoting an Ink dialog footer inside its own output, captured off a
/// live PTY on 2026-08-30. The line is byte-identical to the one a real
/// `AskUserQuestion` draws; only the indentation of the agent's frame differs.
///
/// This ran through the pipeline and set `question_confident`, which no clear
/// path retracts — the tab reported the agent as blocked on the user while it
/// was working, for the rest of the turn. `hook_instrumented` is false on
/// purpose: that is the state of a Claude session that has not yet raised an
/// `AskUserQuestion`, so `suppress_heuristic_question` was not covering it.
#[test]
fn quoted_ink_footer_in_agent_output_raises_no_question() {
    let events = replay_capture(
        &agent_prompt_fixture("claude-quoted-ink-footer.tcap"),
        false,
    );
    let questions: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, ParsedEvent::Question { .. }))
        .collect();
    assert!(
        questions.is_empty(),
        "an echoed footer is not a dialog, got: {questions:?}"
    );
}

/// grok in `screen_mode = "minimal"` — the mode it is actually run in —
/// replayed byte-for-byte off a live 1.0.5 session (40x120, one full turn:
/// prompt → thinking → answer → ready).
///
/// This is the whole grok chain in one assertion, because every link of it
/// has failed independently:
///   1. the foreground binary reports as `grok-1.0.5` (a resolved symlink),
///      and an exact-match table answers `None` — no `agent_type`, so
///      `session_is_agent` is false and NOTHING can ever be typed into the
///      composer: no peer message, no orchestrator mail wake;
///   2. minimal mode draws no composer box, so a boxed-prompt matcher never
///      fires `Ready` and the session stays BUSY for the whole process;
///   3. `completed` needs the `suggest:` marker to survive the chrome trim.
///
/// Ready must be the LAST verdict and Working must have occurred: a screen
/// adapter that only ever answers `Unknown` leaves `idle_confirmed` false,
/// which reads as "idle" to `agent_state` but blocks `should_inject_now` —
/// the mismatch that burns the orchestrator wake budget permanently.
#[test]
fn grok_minimal_capture_reaches_ready_and_declares_completion() {
    use crate::state::VtLogBuffer;

    assert_eq!(classify_agent("grok-1.0.5"), Some("grok"));
    assert!(has_ready_screen_adapter(classify_agent("grok-1.0.5")));

    let bytes = agent_prompt_fixture("grok-1.0.5-minimal-turn.tcap");
    let mut vt_log = VtLogBuffer::new(40, 120, 2000);
    let mut parser = crate::output_parser::OutputParser::new();
    let mut saw_working = false;
    let mut last_activity = AgentScreenActivity::Unknown;
    let mut suggested = None;

    for record in crate::pty_capture::decode(&bytes).expect("valid capture") {
        if record.direction != crate::pty_capture::CaptureDirection::Output {
            continue;
        }
        let mut changed = vt_log.process(&record.data);
        let screen = vt_log.screen_rows();
        let refs: Vec<&str> = screen.iter().map(String::as_str).collect();
        if let Some(cutoff) = crate::chrome::find_chrome_cutoff(&refs) {
            changed.retain(|row| row.row_index < cutoff);
        }
        for event in parser.parse_clean_lines(&changed, true) {
            if let ParsedEvent::Suggest { items } = event {
                suggested = Some(items);
            }
        }
        match detect_agent_screen_activity(Some("grok"), &screen) {
            AgentScreenActivity::Unknown => {}
            activity => {
                saw_working |= activity == AgentScreenActivity::Working;
                last_activity = activity;
            }
        }
    }

    assert!(
        saw_working,
        "grok's turn-status spinner must mark the session working, or a busy \
             turn reads as idle and a peer message is typed into a live composer"
    );
    assert_eq!(
        last_activity,
        AgentScreenActivity::Ready,
        "the bare `❯` composer row of minimal mode must end the turn Ready"
    );
    assert_eq!(
        suggested.as_deref(),
        Some(
            &[
                "Altra richiesta".to_string(),
                "Fermati".to_string(),
                "Ripeti il conteggio".to_string(),
            ][..]
        ),
        "the `suggest:` marker must survive the chrome trim — it is the only \
             thing that promotes grok from `idle` to `completed`"
    );
}

/// Replay a whole directory of real `.tcap` captures through the production
/// composition and report what the detection pipeline made of them.
///
/// Ignored by default: the corpus is whatever the operator recorded through
/// `POST /diagnostics/capture`, and those files hold real session content —
/// prompts, source, paths — so they are deliberately NOT committed. This is a
/// measurement harness, not a regression test. What it surfaces becomes
/// either a code fix or a single committed fixture, chosen deliberately.
///
/// ```text
/// TUIC_CAPTURE_CORPUS="$HOME/Library/Application Support/com.tuic.commander/captures" \
///   cargo test -p tuicommander detection_over_capture_corpus -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs a capture corpus; see TUIC_CAPTURE_CORPUS"]
fn detection_over_capture_corpus() {
    let Ok(dir) = std::env::var("TUIC_CAPTURE_CORPUS") else {
        panic!("set TUIC_CAPTURE_CORPUS to a directory of .tcap/.raw captures");
    };
    let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .expect("readable corpus directory")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("tcap" | "raw")
            )
            .then_some(path)
        })
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "corpus held no captures");

    for path in &paths {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let bytes = std::fs::read(path).expect("readable capture");
        let Ok(records) = crate::pty_capture::decode(&bytes) else {
            println!("{name:<42} UNDECODABLE");
            continue;
        };
        if records.is_empty() {
            println!("{name:<42} empty");
            continue;
        }

        let (events, stats) = replay_capture_measured(&bytes, false);
        let fail_open = if stats.ticks_with_content == 0 {
            0.0
        } else {
            100.0 * stats.ticks_without_cutoff as f64 / stats.ticks_with_content as f64
        };
        println!(
            "\n{name}\n  {:>6} out / {:>4} in records, {:>7} bytes | rows offered {:>6}, \
                 trimmed {:>6} | no-cutoff {:>5.1}% of {} content ticks",
            stats.output_records,
            stats.input_records,
            stats.bytes,
            stats.rows_offered,
            stats.rows_trimmed,
            fail_open,
            stats.ticks_with_content,
        );

        let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
        for event in &events {
            let kind = serde_json::to_value(event)
                .ok()
                .and_then(|v| v["type"].as_str().map(str::to_string))
                .unwrap_or_else(|| "?".to_string());
            *kinds.entry(kind).or_default() += 1;
        }
        if kinds.is_empty() {
            println!("  events: none");
        } else {
            let rendered: Vec<String> = kinds.iter().map(|(k, n)| format!("{k}×{n}")).collect();
            println!("  events: {}", rendered.join(", "));
        }

        // Awaiting is sticky by construction: whatever SETs it owns nothing
        // until something retracts it. Walk the sequence and report the
        // badge a tab would still be rendering at the end of the capture.
        let mut awaiting: Option<String> = None;
        let mut sets = 0usize;
        let mut clears = 0usize;
        for event in &events {
            match event {
                ParsedEvent::Question { prompt_text, .. } => {
                    sets += 1;
                    awaiting = Some(prompt_text.clone());
                }
                ParsedEvent::QuestionCleared | ParsedEvent::UserInput { .. } => {
                    clears += usize::from(awaiting.take().is_some());
                }
                _ => {}
            }
        }
        match awaiting {
            Some(prompt) => {
                println!("  awaiting: {sets} set / {clears} cleared → STILL SET at end: {prompt:?}")
            }
            None if sets > 0 => println!("  awaiting: {sets} set / {clears} cleared → clear"),
            None => {}
        }
    }
}

fn awaiting_prompts(events: &[ParsedEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            ParsedEvent::Question {
                prompt_text,
                confident: true,
            } => Some(prompt_text.clone()),
            _ => None,
        })
        .collect()
}

/// Every prompt the pipeline reported, whatever its confidence. A signal
/// that badges the tab but stays retractable — Claude's `is waiting for
/// your input` notify — is invisible to `awaiting_prompts`, so asserting
/// "the notify survived the pipeline" needs this view instead.
fn awaiting_prompts_any_confidence(events: &[ParsedEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            ParsedEvent::Question { prompt_text, .. } => Some(prompt_text.clone()),
            _ => None,
        })
        .collect()
}

/// Regression, captured 2026-08-08 from a live session parked on a plan
/// picker while its tab showed a "working" dot.
///
/// The session is hook-instrumented Claude, so every regex Question is
/// dropped by design — and the picker is not `PreToolUse(AskUserQuestion)`,
/// so the hook emitted no `state=awaiting` either. Both channels silent, the
/// agent blocked. The one thing Claude did say is in these bytes:
/// `ESC]777;notify;Claude Code;Claude is waiting for your input BEL`.
/// Before that sequence was parsed this assertion found nothing.
#[test]
fn hook_instrumented_session_still_reports_awaiting_via_osc777() {
    let events = replay_capture(&agent_prompt_fixture("claude-plan-picker.raw"), true);
    // Retractable on purpose: the same body arrives on Claude's 60s idle
    // timer after a finished turn. The picker keeps the badge because the
    // prompt stays on screen, not because the notify is trusted forever.
    let prompts = awaiting_prompts_any_confidence(&events);

    assert!(
        prompts
            .iter()
            .any(|p| p == "Claude is waiting for your input"),
        "hook suppression must not swallow the agent's own notification; \
             questions seen: {prompts:?}"
    );
}

/// The same capture with hook instrumentation off: the notify is a property
/// of the agent's output, not of our suppression, so it must survive either
/// way. Guards against "fixed it by disabling the filter".
#[test]
fn osc777_awaiting_does_not_depend_on_hook_instrumentation() {
    for hook in [true, false] {
        let events = replay_capture(&agent_prompt_fixture("claude-plan-picker.raw"), hook);
        assert!(
            awaiting_prompts_any_confidence(&events)
                .iter()
                .any(|p| p == "Claude is waiting for your input"),
            "notify lost with hook_instrumented={hook}"
        );
    }
}

/// Regression for the other observed Claude notification payload. OSC 777
/// is a desktop-notification transport, so the generic "needs your
/// attention" body is not proof that the composer awaits a response. Treating
/// it as a confident question latched awaiting after completion indefinitely.
#[test]
fn generic_osc777_attention_does_not_report_awaiting() {
    for hook in [true, false] {
        let events = replay_capture(&agent_prompt_fixture("claude-generic-attention.raw"), hook);
        assert!(
            awaiting_prompts_any_confidence(&events).is_empty(),
            "generic notification became awaiting with hook_instrumented={hook}: {events:?}"
        );
    }
}

// --- Awaiting RETRACTION -----------------------------------------------
//
// Why the fixtures above could not catch the stuck "question" badge: they
// replay bytes through the PARSERS and assert which events come out. The
// badge is not an event, it is `SessionState.awaiting_input` — and this
// failure was the ABSENCE of any event, so no capture can express it. The
// tests below close that gap by driving the real accumulator instead of
// the parser output: they assert the state a tab actually renders.

/// Wait for the event-bus accumulator to apply what we emitted. Polls
/// rather than sleeping a fixed amount so it neither flakes nor stalls.
async fn await_session<F: Fn(&crate::state::SessionState) -> bool>(
    state: &Arc<AppState>,
    session_id: &str,
    pred: F,
) -> bool {
    for _ in 0..200 {
        if state
            .session_states
            .get(session_id)
            .is_some_and(|s| pred(&s))
        {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    false
}

fn accumulating_state(session_id: &str) -> Arc<AppState> {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    state.session_states.insert(
        session_id.to_string(),
        crate::state::SessionState::default(),
    );
    crate::state::AppState::spawn_session_state_accumulator(state.clone());
    state
}

fn heuristic_question(session_id: &str, prompt: &str) -> crate::state::AppEvent {
    crate::state::AppEvent::PtyParsed {
        session_id: session_id.to_string(),
        parsed: serde_json::json!({
            "type": "question",
            "prompt_text": prompt,
            "confident": false,
        })
        .into(),
    }
}

/// Regression, observed 2026-08-10 on a live codex tab: the turn had
/// finished, the approval dialog was long gone, and the tab still read
/// "question".
///
/// The sequence, end to end: codex prints "Would you like to make the
/// following edits?", the silence heuristic verifies it on screen and emits
/// a low-confidence Question, Boss answers with a bare Enter. That Enter
/// produces no `user-input` (that arm needs a non-empty typed line), codex
/// goes busy through screen movement rather than a parsed `status-line`,
/// and no `choice-prompt` was ever set to resolve. Every existing clear
/// needs an event that never arrives — so the badge latched.
#[tokio::test(flavor = "current_thread", start_paused = false)]
async fn stale_heuristic_awaiting_is_retracted_when_the_prompt_leaves_the_screen() {
    use crate::state::VtLogBuffer;

    let question = "Would you like to make the following edits?";
    let mut vt = VtLogBuffer::new(41, 128, 2000);
    vt.process(format!("\x1b[2J\x1b[H{question}\r\n").as_bytes());
    assert!(
        verify_question_on_screen(&vt.screen_rows(), question, SCREEN_VERIFY_ROWS),
        "precondition: the prompt is on screen, so the heuristic fires"
    );

    let state = accumulating_state("s1");
    state.emit_pty_event(heuristic_question("s1", question));
    assert!(
        await_session(&state, "s1", |s| s.awaiting_input).await,
        "the low-confidence question must badge the tab"
    );

    // Bare Enter: codex repaints the finished turn, prompt gone.
    vt.process(b"\x1b[2J\x1b[HDone. 2 files changed.\r\n");
    assert!(
        !verify_question_on_screen(&vt.screen_rows(), question, SCREEN_VERIFY_ROWS),
        "the prompt must be gone — this is the branch that retracts"
    );

    emit_question_cleared_if_stale(&state, "s1");
    assert!(
        await_session(&state, "s1", |s| !s.awaiting_input
            && s.question_text.is_none())
        .await,
        "the badge must drop once the question left the screen"
    );
}

/// Regression, observed 2026-08-11 on a live Claude tab: the turn had ended
/// 17h earlier, its recap was the last thing on screen, no prompt anywhere —
/// and the tab still read "question". The session carried
/// `question_text = "Claude is waiting for your input"`, which is what Claude
/// notifies on its 60s idle timer as well as on a blocked picker. Parsed as
/// confident, it was retractable by nothing but a typed line, and there was
/// nothing to type.
///
/// Both bodies go through the real parser here: hard-coding the JSON would
/// let the test keep passing after the parser stopped agreeing with it.
#[tokio::test(flavor = "current_thread")]
async fn osc777_notify_retraction_follows_the_wording() {
    for (body, survives) in [
        ("Claude is waiting for your input", false),
        ("Claude needs your permission", true),
    ] {
        let raw = format!("\x1b]777;notify;Claude Code;{body}\x07");
        let notify = crate::output_parser::parse_osc777_notify(&raw)
            .unwrap_or_else(|| panic!("{body:?} must still report awaiting"));

        let state = accumulating_state("s1");
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: "s1".to_string(),
            parsed: serde_json::to_value(&notify).expect("serialisable").into(),
        });
        assert!(
            await_session(&state, "s1", |s| s.awaiting_input).await,
            "{body:?} must badge the tab"
        );

        // The screen is quiet and carries no prompt — the recap case.
        emit_question_cleared_if_stale(&state, "s1");
        let cleared = await_session(&state, "s1", |s| !s.awaiting_input).await;
        assert_eq!(
            cleared, !survives,
            "{body:?}: expected survives={survives}, badge cleared={cleared}"
        );
    }
}

/// grok repaints while it waits, so "not on screen this tick" is not proof
/// that a confident prompt was answered. Retracting it would drop a real
/// approval request — the same reason the status-line arm keeps it sticky.
#[tokio::test(flavor = "current_thread")]
async fn confident_awaiting_is_never_retracted() {
    let state = accumulating_state("s1");
    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
        session_id: "s1".to_string(),
        parsed: serde_json::json!({
            "type": "question",
            "prompt_text": "Run echo x",
            "confident": true,
        })
        .into(),
    });
    assert!(await_session(&state, "s1", |s| s.awaiting_input).await);

    emit_question_cleared_if_stale(&state, "s1");
    assert!(
        !await_session(&state, "s1", |s| !s.awaiting_input).await,
        "a confident question must survive the retraction"
    );
}

/// A live choice prompt owns its own resolution (`resolve_choice_prompt_input`
/// fires on the option keypress). Retracting under it would clear the badge
/// while the dialog is still on screen waiting for a key.
#[test]
fn retraction_skips_a_session_with_a_live_choice_prompt() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let mut session = crate::state::SessionState {
        awaiting_input: true,
        question_confident: false,
        ..Default::default()
    };
    session.choice_prompt = Some(crate::output_parser::ChoicePromptPayload {
        title: "Which approach should I use?".to_string(),
        options: vec![],
        dismiss_key: None,
        amend_key: None,
    });
    state.session_states.insert("s1".to_string(), session);

    let mut rx = state.event_bus.subscribe();
    emit_question_cleared_if_stale(&state, "s1");
    assert!(
        rx.try_recv().is_err(),
        "no retraction may be emitted while a choice prompt is live"
    );
}

/// The producer (here) and the consumer (state.rs) agree on one wire name.
/// A rename on either side would silently disable the retraction, which is
/// exactly the failure mode this whole path exists to prevent.
#[test]
fn retraction_is_emitted_as_question_cleared() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    state.session_states.insert(
        "s1".to_string(),
        crate::state::SessionState {
            awaiting_input: true,
            question_confident: false,
            ..Default::default()
        },
    );

    let mut rx = state.event_bus.subscribe();
    emit_question_cleared_if_stale(&state, "s1");
    match rx.try_recv() {
        Ok(crate::state::AppEvent::PtyParsed { parsed, .. }) => {
            assert_eq!(
                parsed.get("type").and_then(|t| t.as_str()),
                Some("question-cleared")
            );
        }
        other => panic!("expected a PtyParsed retraction, got {other:?}"),
    }
}

/// Nothing to retract must stay silent — an idle session emitting a clear
/// on every silence tick would flood the bus and every WS client with it.
#[test]
fn retraction_is_silent_when_the_session_is_not_awaiting() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    state
        .session_states
        .insert("s1".to_string(), crate::state::SessionState::default());

    let mut rx = state.event_bus.subscribe();
    emit_question_cleared_if_stale(&state, "s1");
    assert!(rx.try_recv().is_err(), "idle session must emit nothing");
}

/// A multi-question `AskUserQuestion` as Claude renders it: a tab bar of
/// sub-questions, the current one's title and options, and the Ink footer.
/// `answered` moves the ⊠ and swaps the body — everything except the footer.
fn askuserquestion_wizard_screen(answered: usize) -> Vec<String> {
    let tabs = ["CLI.md", "Exit codes", "Provider row"];
    let bar = tabs
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{} {t}", if i < answered { "⊠" } else { "□" }))
        .collect::<Vec<_>>()
        .join("  ");
    vec![
        format!("←  {bar}  ✓ Submit  →"),
        String::new(),
        format!("Sub-question {}: what should step 14 send?", answered + 1),
        String::new(),
        format!("› 1. Option A for {}", tabs[answered.min(2)]),
        format!("  2. Option B for {}", tabs[answered.min(2)]),
        "  3. Type something.".to_string(),
        String::new(),
        "Enter to select · Tab/Arrow keys to navigate · Esc to cancel".to_string(),
    ]
}

/// Regression, observed 2026-08-21 on a live Claude tab: a multi-question
/// AskUserQuestion was on screen waiting on Boss and the tab read "working".
///
/// The first sub-question badges the tab. Answering it clears the badge — and
/// the second sub-question repaints its title and options while the footer row
/// stays byte-identical, so the changed-rows parser never fires again and no
/// clear path is at fault: the SET simply never came back. The re-arm is the
/// only thing standing between that and a tab that lies for the rest of the
/// wizard.
#[tokio::test(flavor = "current_thread", start_paused = false)]
async fn open_dialog_rearms_awaiting_after_a_sub_question_is_answered() {
    let first = askuserquestion_wizard_screen(0);
    let footer = crate::output_parser::ink_dialog_footer(&first)
        .expect("precondition: the Ink footer anchors the dialog");

    let state = accumulating_state("s1");
    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
        session_id: "s1".to_string(),
        parsed: serde_json::json!({
            "type": "question", "prompt_text": footer, "confident": true,
        })
        .into(),
    });
    assert!(
        await_session(&state, "s1", |s| s.awaiting_input).await,
        "the first sub-question must badge the tab"
    );

    // Boss answers it. Whatever cleared the badge — a typed line here — the
    // wizard is still open on its next sub-question.
    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
        session_id: "s1".to_string(),
        parsed: serde_json::json!({ "type": "user-input", "content": "1" }).into(),
    });
    assert!(
        await_session(&state, "s1", |s| !s.awaiting_input).await,
        "precondition: answering clears the badge"
    );

    let second = askuserquestion_wizard_screen(1);
    assert_ne!(second[2], first[2], "the body moved on");
    assert_eq!(
        second.last(),
        first.last(),
        "…but the footer did not — this is why the changed-rows parser is blind"
    );

    let evt = rearm_awaiting_for_open_dialog(&second, false, false, false, false)
        .expect("an open dialog with the badge off must re-arm");
    let ParsedEvent::Question {
        prompt_text,
        confident,
    } = &evt
    else {
        panic!("re-arm must be a Question, got {evt:?}");
    };
    assert_eq!(prompt_text, footer);
    assert!(confident, "an Ink footer is not a guess");

    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
        session_id: "s1".to_string(),
        parsed: serde_json::json!({
            "type": "question", "prompt_text": prompt_text, "confident": confident,
        })
        .into(),
    });
    assert!(
        await_session(&state, "s1", |s| s.awaiting_input).await,
        "the tab must read awaiting again while the wizard is open"
    );
}

/// The re-arm must not fire per repaint, must not fight OSC 7770, and must not
/// step on a live choice prompt — each of those was a separate storm in the
/// history of this file.
#[test]
fn rearm_stays_silent_unless_the_badge_is_actually_off() {
    let screen = askuserquestion_wizard_screen(1);
    assert!(
        rearm_awaiting_for_open_dialog(&screen, false, true, false, false).is_none(),
        "already awaiting — re-arming every repaint would storm"
    );
    assert!(
        rearm_awaiting_for_open_dialog(&screen, true, false, false, false).is_none(),
        "hook-instrumented sessions get awaiting from OSC 7770"
    );
    assert!(
        rearm_awaiting_for_open_dialog(&screen, false, false, true, false).is_none(),
        "a live choice prompt owns awaiting through its own resolve path"
    );
    let no_dialog = vec!["· Gallivanting… (15m 12s)".to_string(), "❯".to_string()];
    assert!(
        rearm_awaiting_for_open_dialog(&no_dialog, false, false, false, false).is_none(),
        "no dialog on screen, no badge"
    );
    let quoted = vec!["+  Enter to select · Esc to cancel".to_string()];
    assert!(
        rearm_awaiting_for_open_dialog(&quoted, false, false, false, false).is_none(),
        "a diff line quoting the footer is not a dialog"
    );
}

/// The opening frame of a non-hook `AskUserQuestion`: the footer row genuinely
/// changed, so `parse_clean_lines` already parsed the real question this tick.
/// `SessionState.awaiting_input` is still false — this tick's events have not
/// reached it — so the badge guard alone lets the re-arm fire as well. The
/// accumulator keeps the LAST `prompt_text`, so the second event replaces the
/// question with the footer and the tab reads `⊠ … ✓ Submit`. It also resets
/// `last_question_text`, so the real question re-emits on the next repaint.
#[test]
fn rearm_yields_to_a_question_already_parsed_in_the_same_tick() {
    let screen = askuserquestion_wizard_screen(0);
    assert!(
        rearm_awaiting_for_open_dialog(&screen, false, false, false, false).is_some(),
        "precondition: this screen re-arms when nothing else spoke"
    );
    assert!(
        rearm_awaiting_for_open_dialog(&screen, false, false, false, true).is_none(),
        "the parsed question is the better text; the footer must not overwrite it"
    );
}

#[test]
fn question_suppress_resolves_from_agent_config() {
    use crate::config::{AgentSettings, AgentsConfig};
    let mut agents = AgentsConfig::default();
    let enabled = AgentSettings {
        hook_instrumentation: Some(true),
        ..Default::default()
    };
    agents.agents.insert("claude".into(), enabled);
    let disabled = AgentSettings {
        hook_instrumentation: Some(false),
        ..Default::default()
    };
    agents.agents.insert("codex".into(), disabled);

    assert!(hook_instrumented_for(&agents, Some("claude")));
    assert!(
        !hook_instrumented_for(&agents, Some("codex")),
        "explicit false"
    );
    assert!(
        !hook_instrumented_for(&agents, Some("gemini")),
        "no override"
    );
    assert!(!hook_instrumented_for(&agents, None), "no agent type");
}

#[test]
fn osc133_a_transitions_to_idle_immediately() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-osc133-idle";
    state.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));
    state
        .has_osc133_integration
        .insert(session_id.to_string(), ());

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('A', "", session_id, &state);

    let current = state
        .shell_states
        .get(session_id)
        .unwrap()
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(
        current, SHELL_IDLE,
        "OSC 133 A should transition to idle immediately"
    );
}

#[test]
fn osc133_c_transitions_to_busy_immediately() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-osc133-busy";
    state.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));
    state
        .has_osc133_integration
        .insert(session_id.to_string(), ());

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('C', "", session_id, &state);

    let current = state
        .shell_states
        .get(session_id)
        .unwrap()
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(
        current, SHELL_BUSY,
        "OSC 133 C should transition to busy immediately"
    );
}

#[test]
fn osc133_a_emits_shell_state_event() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-osc133-emit";
    state.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));

    // Subscribe to event bus before transition
    let mut rx = state.event_bus.subscribe();

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('A', "", session_id, &state);

    // Check event_bus received a state change
    let evt = rx.try_recv();
    assert!(
        evt.is_ok(),
        "event_bus should have received a shell state event"
    );
    if let Ok(crate::state::AppEvent::PtyParsed {
        session_id: sid,
        parsed,
    }) = evt
    {
        assert_eq!(sid, session_id);
        assert_eq!(parsed["type"], "shell-state");
        assert_eq!(parsed["state"], "idle");
    } else {
        panic!("expected PtyParsed event with shell-state");
    }
}

#[test]
fn osc133_d_does_not_transition_alone() {
    // D means "command finished" but idle only happens when A arrives (prompt shown)
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-osc133-d";
    state.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));
    state
        .has_osc133_integration
        .insert(session_id.to_string(), ());

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('D', "0", session_id, &state);

    let current = state
        .shell_states
        .get(session_id)
        .unwrap()
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(
        current, SHELL_BUSY,
        "OSC 133 D alone should NOT transition — wait for A"
    );
}

#[test]
fn osc133_d_without_c_records_no_outcome() {
    // A 'D' (command finished) without a preceding 'C' (command started) —
    // e.g. Enter on an empty prompt — must NOT record a phantom outcome
    // (empty command, "unknown" error) that would pollute the knowledge
    // panel and the agent's injected prompt.
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-osc133-d-no-c";
    state
        .has_osc133_integration
        .insert(session_id.to_string(), ());

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('D', "1", session_id, &state);

    let recorded = state
        .session_knowledge
        .get(session_id)
        .map(|k| k.lock().commands.len())
        .unwrap_or(0);
    assert_eq!(recorded, 0, "D without C must not record an outcome");
}

#[test]
fn osc133_c_then_d_records_outcome() {
    // Regression guard: the normal path still records — C captures the
    // command start, D finalizes the outcome.
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-osc133-c-then-d";
    state.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));
    state
        .has_osc133_integration
        .insert(session_id.to_string(), ());

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('C', "", session_id, &state);
    proc.handle_osc133_event('D', "0", session_id, &state);

    let recorded = state
        .session_knowledge
        .get(session_id)
        .map(|k| k.lock().commands.len())
        .unwrap_or(0);
    assert_eq!(recorded, 1, "C→D should record exactly one outcome");
}

// --- is_cc_tool_call_header tests ---

#[test]
fn cc_tool_call_bash() {
    assert!(is_cc_tool_call_header(
        "⏺ Bash(curl -s 'http://localhost:9876/logs')"
    ));
}

#[test]
fn cc_tool_call_read() {
    assert!(is_cc_tool_call_header("⏺ Read(src/foo.rs)"));
}

#[test]
fn cc_tool_call_edit() {
    assert!(is_cc_tool_call_header("⏺ Edit(file_path=/tmp/a.rs)"));
}

#[test]
fn cc_tool_call_mcp() {
    assert!(is_cc_tool_call_header(
        "⏺ mcp__tuicommander__ui(action=tab)"
    ));
}

#[test]
fn cc_tool_call_with_leading_whitespace() {
    assert!(is_cc_tool_call_header("  ⏺ Bash(ls)"));
}

#[test]
fn cc_prose_not_tool_call() {
    assert!(!is_cc_tool_call_header("⏺ Boss, ci sono molti tipi di OSC"));
}

#[test]
fn cc_prose_with_paren_not_tool_call() {
    assert!(!is_cc_tool_call_header(
        "⏺ Nessun errore (tutti i log puliti)"
    ));
}

#[test]
fn cc_calling_collapsed_not_tool_call() {
    assert!(!is_cc_tool_call_header(
        "⏺ Calling tuicommander 2 times… (ctrl+o to expand)"
    ));
}

#[test]
fn cc_mission_control_not_tool_call() {
    assert!(!is_cc_tool_call_header(
        "⏺ Mission Control: opened in TUIC tab"
    ));
}

#[test]
fn cc_empty_after_bullet_not_tool_call() {
    assert!(!is_cc_tool_call_header("⏺ "));
    assert!(!is_cc_tool_call_header("⏺"));
}

#[test]
fn cc_no_bullet_not_tool_call() {
    assert!(!is_cc_tool_call_header("Bash(ls)"));
    assert!(!is_cc_tool_call_header("plain text"));
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

    let pidfile = std::env::temp_dir().join(format!("tuic_pgkill_{}.pid", std::process::id()));
    let _ = std::fs::remove_file(&pidfile);

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
    state.sessions.insert(
        sid.to_string(),
        Mutex::new(PtySession {
            writer: Arc::new(Mutex::new(writer)),
            master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
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
    let _ = std::fs::remove_file(&pidfile);
    assert!(
        dead,
        "grandchild {grandchild} survived tab close — orphaned process tree"
    );
}

// ── process_kitty_actions ───────────────────────────────────────

#[test]
fn process_kitty_actions_empty_is_noop() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "kitty-empty";
    process_kitty_actions(&[], sid, &state);
    assert!(
        !state.kitty_states.contains_key(sid),
        "empty action list must not allocate per-session kitty state"
    );
}

#[test]
fn process_kitty_actions_push_pop_query_tracks_flag_stack() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "kitty-stack";

    // Two pushes: current flags follow the top of the stack.
    process_kitty_actions(&[KittyAction::Push(1), KittyAction::Push(5)], sid, &state);
    assert_eq!(
        state.kitty_states.get(sid).unwrap().lock().current_flags(),
        5
    );

    // Pop returns to the first pushed value.
    process_kitty_actions(&[KittyAction::Pop], sid, &state);
    assert_eq!(
        state.kitty_states.get(sid).unwrap().lock().current_flags(),
        1
    );

    // Query with no live PTY session must not panic (writer path is skipped)
    // and must leave the flag stack untouched.
    process_kitty_actions(&[KittyAction::Query], sid, &state);
    assert_eq!(
        state.kitty_states.get(sid).unwrap().lock().current_flags(),
        1
    );
}

// ── cleanup_session ─────────────────────────────────────────────

#[test]
fn cleanup_session_clears_transient_session_maps() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "cleanup-maps";
    state
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state.vt_log_buffers.insert(
        sid.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    state
        .kitty_states
        .insert(sid.to_string(), Mutex::new(KittyKeyboardState::new()));
    state
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_IDLE));
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));
    state
        .term_aliases
        .insert(sid.to_string(), "alias".to_string());
    state.exit_codes.insert(sid.to_string(), 0);

    cleanup_session(sid, &state);

    assert!(!state.output_buffers.contains_key(sid));
    assert!(!state.vt_log_buffers.contains_key(sid));
    assert!(!state.kitty_states.contains_key(sid));
    assert!(!state.shell_states.contains_key(sid));
    assert!(!state.last_output_ms.contains_key(sid));
    assert!(!state.term_aliases.contains_key(sid));
    assert!(!state.exit_codes.contains_key(sid));
}

/// Populate the per-session maps that no teardown phase used to own, plus the
/// two the post-mortem read needs. Deliberately independent of the production
/// enumeration: a teardown test that reuses the list it verifies proves nothing.
fn populate_unowned_session_maps(state: &crate::state::AppState, sid: &str) {
    use std::sync::atomic::{AtomicBool, AtomicU64};
    state
        .slash_mode
        .insert(sid.to_string(), AtomicBool::new(true));
    state
        .last_input_ms
        .insert(sid.to_string(), AtomicU64::new(7));
    state.has_osc133_integration.insert(sid.to_string(), ());
    state.agent_read_cursor.insert(sid.to_string(), 3);
    state
        .marker_stats
        .insert(sid.to_string(), crate::state::MarkerStats::default());
    state.session_knowledge.insert(
        sid.to_string(),
        Mutex::new(crate::ai_agent::knowledge::SessionKnowledge::new()),
    );
    state.session_visibility.insert(sid.to_string(), true);
    state.ai_suggestions_enabled.insert(sid.to_string(), true);
    state.file_sandboxes.insert(
        sid.to_string(),
        crate::ai_agent::sandbox::FileSandbox::new(std::env::temp_dir()).expect("sandbox"),
    );
    state.unrestricted_sessions.insert(sid.to_string(), ());
    state
        .term_aliases
        .insert(sid.to_string(), "tc-9".to_string());
}

/// The swarm/identity maps. `tombstone_transient_cleanup` has always reaped
/// these; `cleanup_session` never did, which is the divergence F8 removes.
fn populate_swarm_session_maps(state: &crate::state::AppState, sid: &str, mcp_sid: &str) {
    state
        .session_parent
        .insert(sid.to_string(), "parent-sess".to_string());
    state
        .shell_state_since_ms
        .insert(sid.to_string(), std::sync::atomic::AtomicU64::new(42));
    state
        .mcp_to_session
        .insert(mcp_sid.to_string(), sid.to_string());
    state
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

    cleanup_session(sid, &state);

    assert!(!state.session_parent.contains_key(sid));
    assert!(!state.shell_state_since_ms.contains_key(sid));
    assert!(!state.mcp_to_session.contains_key(mcp_sid));
    assert!(!state.session_to_mcp.contains_key(sid));
    assert!(!state.peer_agents.contains_key(sid));
    assert!(!state.agent_inbox.contains_key(sid));
    assert!(!state.agent_inbox_evictions.contains_key(sid));
}

#[test]
fn closing_a_session_reaps_the_maps_no_phase_owned() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "close-unowned";
    populate_unowned_session_maps(&state, sid);

    cleanup_session(sid, &state);

    assert!(!state.slash_mode.contains_key(sid));
    assert!(!state.last_input_ms.contains_key(sid));
    assert!(!state.has_osc133_integration.contains_key(sid));
    assert!(!state.agent_read_cursor.contains_key(sid));
    assert!(!state.marker_stats.contains_key(sid));
    assert!(!state.session_visibility.contains_key(sid));
    assert!(!state.ai_suggestions_enabled.contains_key(sid));
    assert!(!state.term_aliases.contains_key(sid));

    // Owned elsewhere, deliberately untouched — see the DEFERRED note on
    // remove_post_mortem_session_state. A sandbox belongs to a conversation
    // that outlives the PTY; knowledge is what the next session inherits.
    assert!(state.file_sandboxes.contains_key(sid));
    assert!(state.unrestricted_sessions.contains_key(sid));
    assert!(state.session_knowledge.contains_key(sid));
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
        !state.slash_mode.contains_key(sid),
        "input mode belongs to the dead process"
    );
    assert!(!state.last_input_ms.contains_key(sid));
    assert!(
        !state.has_osc133_integration.contains_key(sid),
        "shell integration belongs to the dead shell"
    );
    assert!(!state.agent_read_cursor.contains_key(sid));

    assert!(
        state.marker_stats.contains_key(sid),
        "marker tallies are exactly what a post-mortem question asks for"
    );
    assert!(
        state.session_knowledge.contains_key(sid),
        "knowledge is flushed to disk by a 2s task — reaping it here loses it"
    );
    assert!(state.term_aliases.contains_key(sid), "the tab still shows");
    assert!(state.session_visibility.contains_key(sid));
    assert!(state.ai_suggestions_enabled.contains_key(sid));
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

    assert!(!state.term_aliases.contains_key(sid));
    assert!(!state.marker_stats.contains_key(sid));
    assert!(!state.session_visibility.contains_key(sid));
    assert!(!state.ai_suggestions_enabled.contains_key(sid));
    assert!(!state.last_output_ms.contains_key(sid));
    assert!(!state.slash_mode.contains_key(sid));
    assert!(!state.peer_agents.contains_key(sid));
}

/// Stamp a tombstone that is already older than the TTL.
fn stamp_aged_tombstone(state: &crate::state::AppState, sid: &str, now_ms: u64) {
    state.last_output_ms.insert(
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
        .term_aliases
        .insert(sid.to_string(), "tc-1".to_string());
    let candidates = aged_out_tombstones(&state, now_ms);
    assert_eq!(candidates, vec![sid.to_string()]);

    // The race: a client recreates the id after selection, before removal.
    crate::state::tests_support::insert_dummy_session(&state, sid);
    reap_tombstones(&state, &candidates);

    assert!(
        state.term_aliases.contains_key(sid),
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
    assert!(state.sessions.contains_key(sid));

    cleanup_session(sid, &state);

    assert!(
        !state.sessions.contains_key(sid),
        "the live session entry must be removed"
    );
    assert_eq!(
        state.metrics.active_sessions.load(Ordering::Relaxed),
        before - 1,
        "removing a live session must decrement the active-session gauge"
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
    state.sessions.insert(
        sid.to_string(),
        Mutex::new(PtySession {
            writer: Arc::new(Mutex::new(writer)),
            master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            is_remote: false,
            shell: "/bin/sh".to_string(),
        }),
    );
}

// ── ChunkProcessor::check_pending_planfiles ─────────────────────

#[test]
fn check_pending_planfiles_empty_is_noop() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut cp = ChunkProcessor::new(None, None);
    cp.check_pending_planfiles("sid", &state);
    assert!(cp.pending_planfiles.is_empty());
}

#[test]
fn check_pending_planfiles_drops_expired_and_tombstones() {
    use std::time::{Duration, Instant};
    let state = crate::state::tests_support::make_test_app_state();
    let mut cp = ChunkProcessor::new(None, None);
    let missing = "/no/such/planfile/expired.md".to_string();
    // Deadline in the (immediate) past: the internal `Instant::now()` runs
    // after the sleep, so `now > deadline` holds.
    cp.pending_planfiles.push((missing.clone(), Instant::now()));
    std::thread::sleep(Duration::from_millis(2));

    cp.check_pending_planfiles("sid", &state);

    assert!(
        cp.pending_planfiles.is_empty(),
        "an expired retry must be dropped from the queue"
    );
    assert!(
        cp.gaveup_planfiles.contains(&missing),
        "a dropped retry must be tombstoned so it is not re-queued forever"
    );
}

#[test]
fn check_pending_planfiles_keeps_missing_file_until_deadline() {
    use std::time::{Duration, Instant};
    let state = crate::state::tests_support::make_test_app_state();
    let mut cp = ChunkProcessor::new(None, None);
    let missing = "/no/such/planfile/pending.md".to_string();
    cp.pending_planfiles
        .push((missing, Instant::now() + Duration::from_secs(30)));

    cp.check_pending_planfiles("sid", &state);

    assert_eq!(
        cp.pending_planfiles.len(),
        1,
        "a not-yet-existing file with a live deadline stays queued"
    );
}

#[test]
fn check_pending_planfiles_emits_when_file_appears() {
    use std::time::{Duration, Instant};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "planfile-emit";
    let mut cp = ChunkProcessor::new(None, None);

    let dir = std::env::temp_dir().join(format!("tuic_planfile_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let file = dir.join("plan.md");
    std::fs::write(&file, "# plan").expect("write plan file");
    let path = file.to_string_lossy().to_string();

    cp.pending_planfiles
        .push((path.clone(), Instant::now() + Duration::from_secs(30)));
    let mut rx = state.event_bus.subscribe();

    cp.check_pending_planfiles(sid, &state);

    assert!(
        cp.pending_planfiles.is_empty(),
        "a resolved file must leave the retry queue"
    );
    assert!(
        cp.emitted_planfiles.contains(&path),
        "a resolved path must be recorded as emitted"
    );
    let mut got = false;
    while let Ok(evt) = rx.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
            && parsed.get("type").and_then(|t| t.as_str()) == Some("plan-file")
            && parsed.get("path").and_then(|p| p.as_str()) == Some(path.as_str())
        {
            got = true;
        }
    }
    assert!(
        got,
        "a resolved plan file must emit a plan-file PtyParsed event"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ── wake_session ────────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn wake_session_returns_false_when_not_in_standby() {
    let state = crate::state::tests_support::make_test_app_state();
    assert_eq!(wake_session(&state, "not-parked"), Ok(false));
}

#[cfg(unix)]
#[test]
fn wake_session_errors_and_consumes_entry_when_session_missing() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "parked-but-gone";
    state.standby_sessions.insert(sid.to_string(), 0);

    let res = wake_session(&state, sid);

    assert!(
        res.is_err(),
        "a standby entry without a live session must error"
    );
    assert!(res.unwrap_err().contains("Session not found"));
    assert!(
        !state.standby_sessions.contains_key(sid),
        "the standby entry is consumed even on the error path"
    );
}

// ── process-stats helpers ───────────────────────────────────────

#[cfg(not(windows))]
#[test]
fn query_process_stats_reports_own_process() {
    let own = std::process::id();
    let map = query_process_stats(&[own]);
    assert!(map.contains_key(&own), "ps must report our own pid");
    let (rss, _cpu) = map[&own];
    assert!(
        rss > 0,
        "resident set size of a live process must be positive"
    );
}

#[cfg(not(windows))]
#[test]
fn query_process_stats_empty_input_is_empty() {
    assert!(query_process_stats(&[]).is_empty());
}

#[cfg(not(windows))]
#[test]
fn process_parent_map_covers_the_live_process_table() {
    let parent_map = process_parent_map().expect("walking the live process table must succeed");
    let own = std::process::id();
    assert!(
        parent_map.values().any(|children| children.contains(&own)),
        "the shared map must place our own pid under its parent"
    );
}

#[cfg(not(windows))]
#[test]
fn parse_process_parent_map_skips_unreadable_rows() {
    let parent_map = parse_process_parent_map(
        "  PID  PPID\n    1     0\n  100     1\nbogus row\n  101   100\n  102\n  103   100\n",
    );
    assert_eq!(
        parent_map.get(&1).map(Vec::as_slice),
        Some([100].as_slice())
    );
    assert_eq!(
        parent_map.get(&100).map(Vec::as_slice),
        Some([101, 103].as_slice()),
        "a header, a word row and a truncated row must not drop the rows around them"
    );
    assert!(
        !parent_map.contains_key(&102),
        "a row without a parent column contributes nothing"
    );
}

/// The refresh queries `ps` once and walks one subtree per session, so the
/// walk must be a pure function of the shared map: each root gets its own
/// transitive closure, and an extra root costs no extra query.
#[cfg(not(windows))]
#[test]
fn descendants_are_transitive_and_distributed_per_root() {
    let parent_map = parse_process_parent_map(
        "  PID  PPID\n  100     1\n  101   100\n  102   101\n  200     1\n  201   200\n",
    );
    let mut first = descendants_from_parent_map(&parent_map, 100);
    first.sort_unstable();
    assert_eq!(first, vec![101, 102], "the walk must reach grandchildren");
    let mut second = descendants_from_parent_map(&parent_map, 200);
    second.sort_unstable();
    assert_eq!(second, vec![201], "a second root sees only its own subtree");
    assert!(
        descendants_from_parent_map(&parent_map, 999).is_empty(),
        "an unknown root has no descendants"
    );
}

#[cfg(not(windows))]
#[test]
fn descendants_walk_terminates_on_a_self_parented_row() {
    let parent_map = parse_process_parent_map("  0     0\n  100     0\n");
    assert_eq!(
        descendants_from_parent_map(&parent_map, 0),
        vec![100],
        "a self-parented row must neither spin the walk nor make the root its own descendant"
    );
}

#[cfg(not(windows))]
#[test]
fn process_tree_snapshot_reports_own_process() {
    let own = std::process::id();
    let snapshot = process_tree_snapshot().expect("ps process-tree snapshot");
    let mine = snapshot
        .iter()
        .find(|process| process.pid == own)
        .expect("the process-tree parser must preserve live PIDs");
    // The startup window silently degrades to the old name-only rule when
    // ages are missing, so this platform's `etime` column has to be proven
    // readable here rather than inferred from the fixture tests.
    assert!(
        mine.age_seconds.is_some(),
        "this platform's ps must yield a parsable elapsed time"
    );
}

// --- Chunk-path characterization ---------------------------------------
//
// The chunk path is a refactor target: cutoff placement, snapshot reuse,
// lock coalescing and clone removal must all be observationally silent.
// These tests pin what a session actually sees — the emitted `PtyParsed`
// payloads, the shell state and the awaiting badge — for a real capture
// replayed through the real `process_chunk`, so a regression shows up as a
// diff in the recorded trace rather than as a subtle live-session bug.

/// Everything a refactor of `process_chunk` is allowed to leave unchanged.
#[derive(Debug, PartialEq)]
struct ChunkTrace {
    /// One entry per chunk: the parsed-event payloads it emitted, in order.
    per_chunk_events: Vec<Vec<serde_json::Value>>,
    /// Shell state after the last chunk.
    shell_state: Option<u8>,
    /// Non-event side effects the chunk path writes directly.
    last_question_text: Option<String>,
    last_choice_prompt_sig: Option<String>,
    terminal_mode_fullscreen: bool,
    /// Bytes the ring buffer accumulated (proves the passthrough is intact).
    ring_len: usize,
}

/// Build the exact set of per-session maps `process_chunk` touches.
fn chunk_trace_state(sid: &str) -> (Arc<AppState>, Arc<Mutex<SilenceState>>) {
    use crate::state::VtLogBuffer;
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(41, 128, 2000)));
    state.output_buffers.insert(
        sid.to_string(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );
    state
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));
    state
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());
    (state, silence)
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

/// Replay a capture's OUTPUT records through the production `process_chunk`,
/// preserving the original chunk boundaries, and record everything observable.
fn trace_capture_through_process_chunk(bytes: &[u8], agent_type: Option<&str>) -> ChunkTrace {
    let sid = "chunk-trace";
    let (state, silence) = chunk_trace_state(sid);
    if let Some(agent) = agent_type
        && let Some(mut entry) = state.session_states.get_mut(sid)
    {
        entry.agent_type = Some(agent.to_string());
    }
    let mut rx = state.event_bus.subscribe();
    let mut cp = ChunkProcessor::new(None, None);
    let mut utf8_buf = Utf8ReadBuffer::new();
    let mut esc_buf = EscapeAwareBuffer::new();
    let mut per_chunk_events = Vec::new();

    for record in crate::pty_capture::decode(bytes).expect("valid capture") {
        if record.direction != crate::pty_capture::CaptureDirection::Output {
            continue;
        }
        let utf8_data = utf8_buf.push(&record.data);
        let esc_data = esc_buf.push(&utf8_data);
        let (kitty_clean, _actions) = crate::state::strip_kitty_sequences(&esc_data);
        let _ = cp.process_chunk(&kitty_clean, &silence, sid, state.as_ref());
        // Drain per chunk: the bus holds 256 messages and a long capture
        // would otherwise lag and silently drop the evidence.
        let mut this_chunk = Vec::new();
        while let Ok(evt) = rx.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt {
                this_chunk.push((*parsed).clone());
            }
        }
        per_chunk_events.push(this_chunk);
    }

    ChunkTrace {
        per_chunk_events,
        shell_state: state
            .shell_states
            .get(sid)
            .map(|a| a.load(std::sync::atomic::Ordering::Acquire)),
        last_question_text: cp.last_question_text.clone(),
        last_choice_prompt_sig: cp.last_choice_prompt_sig.clone(),
        terminal_mode_fullscreen: cp.terminal_mode.is_fullscreen(),
        ring_len: state
            .output_buffers
            .get(sid)
            .map(|r| r.lock().len())
            .unwrap_or(0),
    }
}

/// The trace is the refactor's contract. Replaying the same bytes twice
/// through two independent pipelines must produce the identical trace —
/// if this is flaky, every characterization assertion below is worthless.
#[test]
fn chunk_trace_is_deterministic_for_a_real_capture() {
    for fixture in [
        "grok-1.0.5-minimal-turn.tcap",
        "claude-quoted-ink-footer.tcap",
    ] {
        let bytes = agent_prompt_fixture(fixture);
        let a = trace_capture_through_process_chunk(&bytes, Some("claude"));
        let b = trace_capture_through_process_chunk(&bytes, Some("claude"));
        assert_eq!(a, b, "{fixture}: chunk trace must be reproducible");
        assert!(
            a.per_chunk_events.iter().any(|c| !c.is_empty()),
            "{fixture}: a capture that emits nothing cannot characterize anything"
        );
    }
}

/// The golden numbers below were recorded against the pre-refactor chunk
/// path (2026-09-05). They are deliberately concrete: a refactor that
/// changes WHICH chunk emits an event, or how many, breaks this.
#[test]
fn chunk_trace_matches_recorded_baseline() {
    for (fixture, agent) in [
        ("grok-1.0.5-minimal-turn.tcap", "grok"),
        ("claude-quoted-ink-footer.tcap", "claude"),
        ("claude-plan-picker.raw", "claude"),
        ("claude-generic-attention.raw", "claude"),
    ] {
        let bytes = agent_prompt_fixture(fixture);
        let trace = trace_capture_through_process_chunk(&bytes, Some(agent));
        let summary: Vec<(usize, Vec<String>)> = trace
            .per_chunk_events
            .iter()
            .enumerate()
            .filter(|(_, evts)| !evts.is_empty())
            .map(|(i, evts)| {
                (
                    i,
                    evts.iter()
                        .map(|e| {
                            e.get("type")
                                .and_then(|t| t.as_str())
                                .unwrap_or("?")
                                .to_string()
                        })
                        .collect(),
                )
            })
            .collect();
        let actual = format!(
            "{summary:?} shell={:?} q={:?} sig={:?} alt={} ring={}",
            trace.shell_state,
            trace.last_question_text,
            trace.last_choice_prompt_sig,
            trace.terminal_mode_fullscreen,
            trace.ring_len
        );
        let expected = match fixture {
            "grok-1.0.5-minimal-turn.tcap" => {
                "[(0, [\"shell-state\"]), (225, [\"status-line\"])] \
                 shell=Some(1) q=None sig=None alt=false ring=28282"
            }
            "claude-quoted-ink-footer.tcap" => {
                "[(1, [\"shell-state\"]), (3, [\"shell-state\"]), (115, [\"shell-state\"]), \
                 (118, [\"shell-state\"])] shell=Some(1) q=None sig=None alt=false ring=1902"
            }
            "claude-plan-picker.raw" => {
                "[(0, [\"status-line\", \"shell-state\"])] \
                 shell=Some(1) q=None sig=None alt=false ring=8196"
            }
            "claude-generic-attention.raw" => "[] shell=Some(0) q=None sig=None alt=false ring=59",
            other => panic!("no recorded baseline for {other}"),
        };
        // The literals above wrap with `\` continuations; normalise the run of
        // spaces that produces before comparing.
        let normalise = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(
            normalise(&actual),
            normalise(expected),
            "{fixture}: the chunk path changed what a session observes"
        );
    }
}

/// Screens the fixtures do not contain: a choice dialog, an Ink question
/// footer, a status-line HUD BELOW the input box, and a slash menu. Each is
/// fed through the real `process_chunk`; the assertion is the set of event
/// types the session observes.
///
/// The HUD case is criterion 1's real subject: rows under the input-box
/// separator must never reach a parser, and moving the cutoff earlier must
/// not change that verdict in either direction.
#[test]
fn chunk_path_scenarios_emit_the_same_events() {
    // (name, screen bytes, slash_mode, expected event types in order)
    let scenarios: &[(&str, &str, bool, &[&str])] = &[
        (
            "choice dialog",
            "\x1b[2J\x1b[HDo you want to make this edit to CLAUDE.md?\r\n\
                 \x20\u{276f} 1. Yes\r\n\
                 \x20\x20 2. Yes, allow all edits\r\n\
                 \x20\x20 3. No\r\n",
            false,
            &["choice-prompt", "shell-state"],
        ),
        (
            "ink question footer",
            "\x1b[2J\x1b[HEnter to select \u{b7} \u{2191}/\u{2193} to navigate \u{b7} Esc to cancel\r\n",
            false,
            &["question", "shell-state"],
        ),
        (
            // A rate-limit line ABOVE the input box: real agent output, the
            // cutoff must keep it.
            "output above the input box",
            "\x1b[2J\x1b[HError: rate_limit_error\r\n\
                 \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\r\n\
                 \u{276f}\r\n\
                 \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\r\n\
                 [Opus 5] 5h: 0% | $15.48\r\n",
            false,
            &["rate-limit", "shell-state"],
        ),
        (
            // The SAME status line, with nothing above the box. Everything
            // that changed is chrome, so nothing may be parsed.
            "status line below the input box only",
            "\x1b[2J\x1b[H\r\n\
                 \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\r\n\
                 \u{276f}\r\n\
                 \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\r\n\
                 Error: rate_limit_error\r\n",
            false,
            &[],
        ),
        (
            "slash menu",
            "\x1b[2J\x1b[H\u{276f} /rev\r\n\
                 \x20 /review    Review a pull request\r\n\
                 \x20 /revert    Undo the last change\r\n",
            true,
            &["slash-menu"],
        ),
    ];

    let mut mismatches: Vec<String> = Vec::new();
    let mut observed: Vec<(&str, Vec<String>)> = Vec::new();
    for (name, screen, slash_on, expected) in scenarios {
        let sid = "chunk-scenario";
        let (state, silence) = chunk_trace_state(sid);
        if *slash_on {
            state
                .slash_mode
                .insert(sid.to_string(), std::sync::atomic::AtomicBool::new(true));
        }
        settle_startup_grace(&silence);
        let mut rx = state.event_bus.subscribe();
        let mut cp = ChunkProcessor::new(None, None);
        cp.process_chunk(screen, &silence, sid, state.as_ref());
        let mut kinds: Vec<String> = Vec::new();
        while let Ok(evt) = rx.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt {
                kinds.push(
                    parsed
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("?")
                        .to_string(),
                );
            }
        }
        observed.push((*name, kinds.clone()));
        if kinds != *expected {
            mismatches.push(format!("{name:?}: expected {expected:?}, got {kinds:?}"));
        }
    }
    assert!(
        mismatches.is_empty(),
        "chunk-path scenarios changed:\n  {}\nfull trace: {observed:?}",
        mismatches.join("\n  ")
    );
}

/// `find_chrome_cutoff` fails OPEN: no anchor means NO trim. A screen with
/// no input box must therefore still deliver every changed row to the
/// parsers — the failure mode of moving the cutoff earlier is turning that
/// "parse everything" into "parse nothing".
#[test]
fn no_chrome_anchor_still_parses_every_row() {
    let sid = "chunk-no-cutoff";
    let (state, silence) = chunk_trace_state(sid);
    let mut rx = state.event_bus.subscribe();
    let mut cp = ChunkProcessor::new(None, None);

    // Plain scrolling output: no separator, no prompt, no input box at all.
    let screen = "\x1b[2J\x1b[HError: rate_limit_error\r\n";
    let refs: Vec<String> = {
        use crate::state::VtLogBuffer;
        let mut vt = VtLogBuffer::new(41, 128, 2000);
        vt.process(screen.as_bytes());
        vt.screen_rows()
    };
    let borrowed: Vec<&str> = refs.iter().map(String::as_str).collect();
    assert!(
        crate::chrome::find_chrome_cutoff(&borrowed).is_none(),
        "precondition: this screen has no chrome anchor"
    );

    settle_startup_grace(&silence);
    cp.process_chunk(screen, &silence, sid, state.as_ref());
    let mut kinds = Vec::new();
    while let Ok(evt) = rx.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt {
            kinds.push(
                parsed
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("?")
                    .to_string(),
            );
        }
    }
    assert!(
        kinds.iter().any(|k| k == "rate-limit"),
        "a cutoff of None must mean parse everything, got {kinds:?}"
    );
}

/// Repeatable CPU measurement for the chunk path. Ignored by default —
/// run with `cargo test --lib -- --ignored --nocapture bench_chunk_path`.
///
/// Per-session setup (`make_test_app_state`) costs milliseconds and would
/// swamp the measurement, so it is hoisted out of the timed region: the
/// loop reuses one session and replays the capture into it, which is also
/// what a long-lived agent tab actually does.
#[test]
#[ignore = "benchmark: run explicitly with --nocapture"]
fn bench_chunk_path_replay() {
    const ITERATIONS: u32 = 200;
    for (fixture, agent) in [
        ("grok-1.0.5-minimal-turn.tcap", "grok"),
        ("claude-plan-picker.raw", "claude"),
        ("claude-quoted-ink-footer.tcap", "claude"),
    ] {
        let bytes = agent_prompt_fixture(fixture);
        let chunks: Vec<Vec<u8>> = crate::pty_capture::decode(&bytes)
            .expect("valid capture")
            .into_iter()
            .filter(|r| r.direction == crate::pty_capture::CaptureDirection::Output)
            .map(|r| r.data)
            .collect();
        let total_bytes: usize = chunks.iter().map(Vec::len).sum();

        let sid = "chunk-bench";
        let (state, silence) = chunk_trace_state(sid);
        if let Some(mut entry) = state.session_states.get_mut(sid) {
            entry.agent_type = Some(agent.to_string());
        }
        let mut rx = state.event_bus.subscribe();
        let mut cp = ChunkProcessor::new(None, None);
        let mut utf8_buf = Utf8ReadBuffer::new();
        let mut esc_buf = EscapeAwareBuffer::new();

        let mut run = |cp: &mut ChunkProcessor,
                       utf8_buf: &mut Utf8ReadBuffer,
                       esc_buf: &mut EscapeAwareBuffer| {
            for chunk in &chunks {
                let utf8_data = utf8_buf.push(chunk);
                let esc_data = esc_buf.push(&utf8_data);
                let (kitty_clean, _) = crate::state::strip_kitty_sequences(&esc_data);
                let _ = cp.process_chunk(&kitty_clean, &silence, sid, state.as_ref());
            }
            while rx.try_recv().is_ok() {}
        };

        // Warm the lazy_static regexes and the grid allocations.
        for _ in 0..3 {
            run(&mut cp, &mut utf8_buf, &mut esc_buf);
        }
        // Report the MINIMUM, not the mean: this machine runs many parallel
        // builds, and a mean is dominated by scheduler interference. The
        // fastest observed replay is the one least contaminated by it.
        let mut best = std::time::Duration::MAX;
        let mut total = std::time::Duration::ZERO;
        for _ in 0..ITERATIONS {
            let start = std::time::Instant::now();
            run(&mut cp, &mut utf8_buf, &mut esc_buf);
            let elapsed = start.elapsed();
            best = best.min(elapsed);
            total += elapsed;
        }
        eprintln!(
            "BENCH {fixture}: {ITERATIONS} x {} chunks / {total_bytes} B \
                 = min {:.3} ms, mean {:.3} ms per replay ({:.3} us per chunk at min)",
            chunks.len(),
            best.as_secs_f64() * 1000.0,
            total.as_secs_f64() * 1000.0 / f64::from(ITERATIONS),
            best.as_secs_f64() * 1e6 / chunks.len() as f64,
        );
    }
}

#[cfg(not(windows))]
#[test]
fn collect_process_stats_includes_tuicommander_itself() {
    let state = crate::state::tests_support::make_test_app_state();
    let stats = collect_process_stats(&state);
    let own = std::process::id();
    assert!(
        stats
            .iter()
            .any(|s| s.session_id.is_none() && s.pid == own && s.name == "TUICommander"),
        "TUIC's own process must appear with no session id"
    );
}

#[cfg(test)]
mod grid_subscriber_tests {
    use super::*;

    /// F28. The frame ticker used to take the vt lock and run a full
    /// `serialize_dirty_rows` on every dirty tick, and only then discover in
    /// `send_grid_frame` that there was no channel and no watch receiver to hand
    /// the bytes to. A session whose tab is closed but whose PTY still runs — an
    /// agent working in an unmounted pane — paid a whole encode per tick for
    /// nothing. This is the check that now runs first, so it has to agree
    /// exactly with what `send_grid_frame` treats as a consumer.

    #[test]
    fn nobody_is_subscribed_to_a_session_with_neither_channel_nor_watch() {
        let state = crate::state::tests_support::make_test_app_state();
        assert!(!grid_has_subscriber(&state, "s1"));
    }

    #[test]
    fn a_watch_whose_receivers_have_all_gone_is_not_a_subscriber() {
        let state = crate::state::tests_support::make_test_app_state();
        // The sender outlives its clients: a browser tab that closed leaves the
        // entry behind. Counting the entry rather than its receivers would keep
        // every such session serializing forever.
        state
            .grid_watch
            .insert("s1".to_string(), crate::grid_gate::new_grid_watch());
        assert!(!grid_has_subscriber(&state, "s1"));
    }

    #[test]
    fn a_live_watch_receiver_is_a_subscriber() {
        let state = crate::state::tests_support::make_test_app_state();
        let tx = crate::grid_gate::new_grid_watch();
        let rx = tx.subscribe();
        state.grid_watch.insert("s1".to_string(), tx);

        assert!(grid_has_subscriber(&state, "s1"));

        drop(rx);
        assert!(
            !grid_has_subscriber(&state, "s1"),
            "the last receiver going away must close the session again"
        );
    }

    #[test]
    fn one_session_having_a_subscriber_says_nothing_about_another() {
        let state = crate::state::tests_support::make_test_app_state();
        let tx = crate::grid_gate::new_grid_watch();
        let _rx = tx.subscribe();
        state.grid_watch.insert("watched".to_string(), tx);

        assert!(grid_has_subscriber(&state, "watched"));
        assert!(!grid_has_subscriber(&state, "unwatched"));
    }
}

#[cfg(test)]
mod vt_read_tests {
    use super::*;

    // Grid reads take the VT mutex, and the PTY reader holds that same mutex
    // through a whole `serialize_dirty_rows`. Waiting for it inline in the IPC
    // handler — the macOS main thread — freezes the WebView for the length of
    // someone else's serialize. `vt_read` is the one door they all go through.

    #[tokio::test]
    async fn a_read_against_a_live_session_returns_the_buffers_answer() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.vt_log_buffers.insert(
            "s1".to_string(),
            parking_lot::Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
        );

        let lines = vt_read(&state, "s1".to_string(), |vt| vt.grid_screen_lines())
            .await
            .unwrap();

        assert_eq!(lines, 24);
    }

    // A tab can be closed while a hover or a selection read is in flight. That
    // is not an error to surface — the caller gets the empty answer it would
    // have got from an empty grid.
    #[tokio::test]
    async fn a_read_against_a_session_that_is_gone_is_the_default() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let text: String = vt_read(&state, "nope".to_string(), |vt| vt.grid_get_cursor_line())
            .await
            .unwrap();

        assert!(text.is_empty());
    }

    // The lock is taken inside the closure, on the pool thread. If it were taken
    // before the hop, the caller would wait for it on the thread it is trying to
    // keep free — so two reads must be able to overlap without deadlocking.
    #[tokio::test]
    async fn two_reads_on_the_same_session_do_not_deadlock() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.vt_log_buffers.insert(
            "s1".to_string(),
            parking_lot::Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
        );

        let (a, b) = tokio::join!(
            vt_read(&state, "s1".to_string(), |vt| vt.grid_total_lines()),
            vt_read(&state, "s1".to_string(), |vt| vt.grid_total_lines()),
        );

        assert_eq!(a.unwrap(), b.unwrap());
    }

    // The point of the whole change: the closure — and therefore the wait for
    // the vt mutex — must not run on the thread that called the command. This
    // is the assertion that fails if someone "simplifies" the helper back into
    // a direct lock.
    #[tokio::test]
    async fn the_read_does_not_run_on_the_calling_thread() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.vt_log_buffers.insert(
            "s1".to_string(),
            parking_lot::Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
        );
        let caller = std::thread::current().id();

        let worker = vt_try_read(&state, "s1".to_string(), |_| std::thread::current().id())
            .await
            .unwrap();

        assert_ne!(worker, Some(caller));
        assert!(worker.is_some());
    }

    // `vt_try_read` keeps the distinction the HTTP routes answer 404 with;
    // `vt_read` is the same call with the miss folded into the default.
    #[tokio::test]
    async fn a_missing_session_is_none_rather_than_an_error() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let seen = vt_try_read(&state, "nope".to_string(), |vt| vt.grid_screen_lines())
            .await
            .unwrap();

        assert_eq!(seen, None);
    }
}

#[cfg(all(test, unix))]
mod standby_tests {
    use super::*;

    /// timeout=0 must wake ALL parked sessions. Entries with no live session
    /// (the "killed between listing and wake" case) must not panic — each is
    /// removed from the map before wake_session errors on the missing session.
    #[test]
    fn wake_all_standby_clears_every_parked_session() {
        let state = crate::state::tests_support::make_test_app_state();
        state.standby_sessions.insert("gone-1".to_string(), 111);
        state.standby_sessions.insert("gone-2".to_string(), 222);
        state.standby_sessions.insert("gone-3".to_string(), 333);

        let attempted = wake_all_standby(&state);

        assert_eq!(attempted, 3, "wake attempted for every parked session");
        assert!(
            state.standby_sessions.is_empty(),
            "standby map must be empty after wake-all even when the sessions are gone"
        );
    }

    /// Empty standby map is a no-op — no panic, nothing to wake.
    #[test]
    fn wake_all_standby_empty_is_noop() {
        let state = crate::state::tests_support::make_test_app_state();
        assert_eq!(wake_all_standby(&state), 0);
        assert!(state.standby_sessions.is_empty());
    }

    /// After a timeout=0 wake-all, standby can re-arm normally when the user
    /// sets a positive timeout again — wake_all_standby sets no persistent
    /// "disabled" flag, it only clears the current standby set.
    #[test]
    fn wake_all_standby_leaves_map_ready_to_rearm() {
        let state = crate::state::tests_support::make_test_app_state();
        state.standby_sessions.insert("gone-1".to_string(), 111);
        wake_all_standby(&state);
        assert!(state.standby_sessions.is_empty());

        // Re-arming (as the checker would on the next tick with timeout>0) works.
        state.standby_sessions.insert("re-armed".to_string(), 444);
        assert_eq!(state.standby_sessions.len(), 1);
    }
}

#[cfg(test)]
mod normalize_path_tests {
    use super::normalize_path;
    use std::path::Path;

    #[test]
    fn resolves_parent_segments() {
        let p = normalize_path(Path::new("/a/b/../../c/d"));
        assert_eq!(p, Path::new("/c/d"));
    }

    #[test]
    fn resolves_worktree_relative_plan() {
        let p = normalize_path(Path::new(
            "/home/user/repo__wt/feat/../../repo/plans/foo.md",
        ));
        assert_eq!(p, Path::new("/home/user/repo/plans/foo.md"));
    }

    #[test]
    fn strips_dot_segments() {
        let p = normalize_path(Path::new("/a/./b/./c"));
        assert_eq!(p, Path::new("/a/b/c"));
    }

    #[test]
    fn preserves_clean_path() {
        let p = normalize_path(Path::new("/home/user/plans/bar.md"));
        assert_eq!(p, Path::new("/home/user/plans/bar.md"));
    }
}
