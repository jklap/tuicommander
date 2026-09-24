use super::*;
// Production builds a grid through `AppState::new_vt_log_buffer` so it picks up
// the config; tests that only exercise the grid construct it directly.
use crate::state::VtLogBuffer;

/// Closing a workspace's terminals is a loop over `close_pty`, and its body
/// waits on two 100 ms `sleep` deadlines per session before it may also delete
/// a worktree. As a plain `fn` command that ran inline on the IPC thread — the
/// macOS main thread — so the post-merge cleanup dialog froze the WebView for
/// the sum of those waits. See `docs/backend/command-threading.md`.
#[test]
fn close_pty_never_runs_on_the_ipc_thread() {
    let source = include_str!("commands.rs");
    let signature = "pub(crate) async fn close_pty(";
    let at = source
        .find(signature)
        .expect("close_pty must be async: a plain fn command runs on the macOS main thread");
    let body = &source[at..(at + 600).min(source.len())];
    assert!(
        body.contains("spawn_blocking"),
        "close_pty waits on process exit and must hand that wait to the blocking pool"
    );
}

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
        .session_maps
        .silence_states
        .get(&sid)
        .map(|e| Arc::clone(e.value()))
        .expect("silence state is registered before the threads start");
    assert!(
        state.grid.frame_dirty.contains_key(&sid)
            && state.grid.sync_update_active.contains_key(&sid),
        "precondition: the ticker owns both per-session entries"
    );

    detonate.store(true, Ordering::Relaxed);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let ticker_alive = state.grid.frame_dirty.contains_key(&sid)
            || state.grid.sync_update_active.contains_key(&sid);
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

/// `pending_scroll` is the target `terminal_scroll_to_offset` writes on both
/// transports and the ticker consumes. It used to be created by
/// `subscribe_terminal_grid`, a desktop-only Tauri command, so a session nothing
/// desktop had ever rendered had no entry to write to at all — the wheel and the
/// scrollbar drag in browser mode wrote nowhere and answered ok. It belongs to
/// the session, like the `grid_frame_dirty` flag the same handler sets, and the
/// only funnel every creation path shares is `spawn_reader_thread`.
#[tokio::test(flavor = "current_thread", start_paused = false)]
async fn a_session_owns_its_pending_scroll_entry_from_the_start() {
    struct EofReader;
    impl Read for EofReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Ok(0)
        }
    }

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "pending-scroll-owner".to_string();
    spawn_reader_thread(
        Box::new(EofReader),
        Arc::new(AtomicBool::new(false)),
        sid.clone(),
        state.clone(),
        None,
    );

    assert!(
        state.grid.pending_scroll.contains_key(&sid),
        "a session with no desktop grid subscriber has nowhere to record a scroll"
    );
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

/// `amp`/`cursor`/`droid` have NO ready-screen adapter and will latch busy
/// forever on their long-lived foreground process (documented gap, checked
/// 2026-09-07, tracked in `has_ready_screen_adapter`'s own doc comment). This
/// was previously pinned only by 5 *positive* per-agent assertions; nothing
/// asserted the negative side, which the state-explain feature's payload now
/// surfaces as `no_adapter_for_agent`.
#[test]
fn has_ready_screen_adapter_false_for_agents_with_no_adapter() {
    for agent in ["amp", "cursor", "droid"] {
        assert!(
            !has_ready_screen_adapter(Some(agent)),
            "{agent} must have no ready-screen adapter (documented gap)"
        );
    }
}

#[test]
fn has_ready_screen_adapter_false_for_none_and_unknown_agent() {
    assert!(!has_ready_screen_adapter(None));
    assert!(!has_ready_screen_adapter(Some("some-future-agent")));
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

// --- TurnEvidence / EvidenceRank / decide() tests ---
//
// `record_busy`/`record_idle`/`record_awaiting`/`force_idle` are the rank-gate
// primitives every busy/idle/awaiting transition routes through (#744-138c);
// `decide()` is the sole arbiter reading the result. Both were previously
// exercised only indirectly through `SilenceState` in integration-shaped
// tests — no test asserted the `bool` a recorder returns, which is exactly
// the rejection path the state-explain feature's decision trail needs to
// report faithfully.

#[test]
fn record_busy_accepts_when_no_idle_or_busy_held() {
    let mut ev = TurnEvidence::default();
    assert!(ev.record_busy(EvidenceRank::Screen, "working-screen"));
}

#[test]
fn record_busy_rejected_by_strictly_higher_idle() {
    let mut ev = TurnEvidence::default();
    ev.record_idle(EvidenceRank::Protocol, "hook-idle");
    assert!(
        !ev.record_busy(EvidenceRank::Screen, "working-screen"),
        "a Screen-rank busy must not reopen a Protocol-rank idle turn"
    );
}

#[test]
fn record_busy_accepted_at_equal_rank_and_clears_idle() {
    let mut ev = TurnEvidence::default();
    ev.record_idle(EvidenceRank::Protocol, "hook-idle");
    assert!(
        ev.record_busy(EvidenceRank::Protocol, "hook-busy"),
        "equal-rank busy must be accepted, not just strictly-higher"
    );
    assert!(
        !ev.idle_confirmed(),
        "busy must clear the held idle evidence"
    );
}

#[test]
fn record_busy_rejected_by_strictly_higher_busy_no_downgrade() {
    let mut ev = TurnEvidence::default();
    ev.record_busy(EvidenceRank::Protocol, "hook-busy");
    assert!(
        !ev.record_busy(EvidenceRank::Screen, "working-screen"),
        "a weaker busy source must not downgrade already-held busy evidence"
    );
}

#[test]
fn record_idle_rejected_by_strictly_higher_busy() {
    let mut ev = TurnEvidence::default();
    ev.record_busy(EvidenceRank::Protocol, "hook-busy");
    assert!(
        !ev.record_idle(EvidenceRank::Screen, "agent-ready-screen"),
        "a Screen-rank idle must not close a Protocol-rank busy turn"
    );
}

#[test]
fn record_idle_accepted_at_equal_rank_and_clears_busy() {
    let mut ev = TurnEvidence::default();
    ev.record_busy(EvidenceRank::Protocol, "hook-busy");
    assert!(ev.record_idle(EvidenceRank::Protocol, "hook-idle"));
    assert!(ev.idle_confirmed());
}

#[test]
fn record_idle_preserves_at_across_repeated_same_rank_source() {
    // The `AGENT_READY_CONFIRM` debounce clock: repeated observations of the
    // identical (rank, source) must not reset the "since when" timestamp, or
    // a ready screen that keeps repainting would never accumulate enough age
    // to confirm.
    let mut ev = TurnEvidence::default();
    ev.record_idle(EvidenceRank::Screen, "agent-ready-screen");
    let first_at = ev.idle.expect("idle evidence recorded").at;
    std::thread::sleep(std::time::Duration::from_millis(5));
    ev.record_idle(EvidenceRank::Screen, "agent-ready-screen");
    let second_at = ev.idle.expect("idle evidence recorded").at;
    assert_eq!(
        first_at, second_at,
        "repeated same (rank, source) idle observation must preserve its original `at`"
    );
}

#[test]
fn record_idle_new_source_at_same_rank_resets_at() {
    let mut ev = TurnEvidence::default();
    ev.record_idle(EvidenceRank::Screen, "agent-ready-screen");
    let first_at = ev.idle.expect("idle evidence recorded").at;
    std::thread::sleep(std::time::Duration::from_millis(5));
    ev.record_idle(EvidenceRank::Screen, "interrupted-screen");
    let second_at = ev.idle.expect("idle evidence recorded").at;
    assert!(
        second_at > first_at,
        "a different source at the same rank is a new observation, not a repeat"
    );
}

#[test]
fn force_idle_bypasses_the_busy_rank_gate() {
    let mut ev = TurnEvidence::default();
    ev.record_busy(EvidenceRank::Protocol, "hook-busy");
    let evidence = ev.force_idle(EvidenceRank::Screen, "restored-confirmed-idle");
    assert_eq!(evidence.rank, EvidenceRank::Screen);
    assert_eq!(evidence.source, "restored-confirmed-idle");
    assert!(
        ev.idle_confirmed(),
        "force_idle must record idle unconditionally, bypassing record_idle's busy-rank gate"
    );
}

#[test]
fn force_idle_preserves_at_across_repeated_same_rank_source() {
    let mut ev = TurnEvidence::default();
    let first = ev.force_idle(EvidenceRank::Silence, "silence-timeout-agent");
    std::thread::sleep(std::time::Duration::from_millis(5));
    let second = ev.force_idle(EvidenceRank::Silence, "silence-timeout-agent");
    assert_eq!(first.at, second.at);
}

#[test]
fn clear_idle_drops_idle_without_recording_busy() {
    let mut ev = TurnEvidence::default();
    ev.record_idle(EvidenceRank::Screen, "agent-ready-screen");
    ev.clear_idle();
    assert!(!ev.idle_confirmed());
    assert!(
        ev.busy.is_none(),
        "clear_idle must not record busy evidence"
    );
}

#[test]
fn record_awaiting_rejected_by_strictly_higher_rank() {
    let mut ev = TurnEvidence::default();
    ev.record_awaiting(EvidenceRank::Protocol, "hook-awaiting");
    assert!(
        !ev.record_awaiting(EvidenceRank::Screen, "footer-heuristic"),
        "a low-confidence screen-scrape must not overwrite a confident protocol question"
    );
    assert_eq!(ev.awaiting_rank(), Some(EvidenceRank::Protocol));
}

#[test]
fn record_awaiting_accepted_at_equal_rank_updates_source() {
    let mut ev = TurnEvidence::default();
    ev.record_awaiting(EvidenceRank::Protocol, "hook-awaiting");
    assert!(
        ev.record_awaiting(EvidenceRank::Protocol, "hook-awaiting-updated"),
        "same-rank awaiting must update, not just strictly-higher"
    );
}

#[test]
fn clear_awaiting_drops_it_regardless_of_rank() {
    let mut ev = TurnEvidence::default();
    ev.record_awaiting(EvidenceRank::Protocol, "hook-awaiting");
    ev.clear_awaiting();
    assert_eq!(ev.awaiting_rank(), None);
}

#[test]
fn idle_confirmed_true_for_silence_timeout_shell_at_silence_rank() {
    // A plain (non-agent) shell's silence timeout IS confirmed idle — the one
    // exception at `Silence` rank, which is otherwise the weakest evidence.
    let mut ev = TurnEvidence::default();
    ev.force_idle(EvidenceRank::Silence, "silence-timeout-shell");
    assert!(
        ev.idle_confirmed(),
        "silence-timeout-shell at Silence rank must read as confirmed idle"
    );
}

#[test]
fn idle_confirmed_false_for_silence_timeout_agent_at_silence_rank() {
    // An agent silence timeout with no screen confirmation is explicitly NOT
    // confirmed (mirrors the pre-#744-138c `idle_confirmed`) — silence alone
    // is not enough evidence for an agent, only for a plain shell.
    let mut ev = TurnEvidence::default();
    ev.force_idle(EvidenceRank::Silence, "silence-timeout-agent");
    assert!(
        !ev.idle_confirmed(),
        "silence-timeout-agent at Silence rank must NOT read as confirmed idle"
    );
}

#[test]
fn idle_confirmed_true_for_any_source_above_silence_rank() {
    let mut ev = TurnEvidence::default();
    ev.record_idle(EvidenceRank::Screen, "agent-ready-screen");
    assert!(ev.idle_confirmed());
}

#[test]
fn idle_confirmed_false_with_no_idle_evidence() {
    let ev = TurnEvidence::default();
    assert!(!ev.idle_confirmed());
}

// --- decide() truth table ---
//
// The single arbiter every busy/idle transition site routes through
// (pty.rs's `decide()`) had zero direct tests before this — only 4 production
// call sites and one doc-comment mention. Pin its full 4-cell truth table.

#[test]
fn decide_returns_to_idle_when_shell_busy_and_idle_evidence_held() {
    let mut ev = TurnEvidence::default();
    ev.record_idle(EvidenceRank::Protocol, "hook-idle");
    match decide(&ev, true, std::time::Instant::now()) {
        Some(Transition::ToIdle(evidence)) => assert_eq!(evidence.source, "hook-idle"),
        other => panic!("expected ToIdle, got {other:?}"),
    }
}

#[test]
fn decide_returns_none_when_shell_busy_and_no_idle_evidence() {
    let ev = TurnEvidence::default();
    assert!(decide(&ev, true, std::time::Instant::now()).is_none());
}

#[test]
fn decide_returns_to_busy_when_shell_idle_and_busy_evidence_held() {
    let mut ev = TurnEvidence::default();
    ev.record_busy(EvidenceRank::Protocol, "hook-busy");
    match decide(&ev, false, std::time::Instant::now()) {
        Some(Transition::ToBusy(evidence)) => assert_eq!(evidence.source, "hook-busy"),
        other => panic!("expected ToBusy, got {other:?}"),
    }
}

#[test]
fn decide_returns_none_when_shell_idle_and_no_busy_evidence() {
    let ev = TurnEvidence::default();
    assert!(decide(&ev, false, std::time::Instant::now()).is_none());
}

#[test]
fn decide_ignores_busy_evidence_while_shell_already_busy() {
    // decide() only ever proposes a transition AWAY from the current shell
    // state — held busy evidence while the shell already reads busy must not
    // produce a (redundant) ToBusy.
    let mut ev = TurnEvidence::default();
    ev.record_busy(EvidenceRank::Protocol, "hook-busy");
    assert!(decide(&ev, true, std::time::Instant::now()).is_none());
}

#[test]
fn decide_ignores_idle_evidence_while_shell_already_idle() {
    let mut ev = TurnEvidence::default();
    ev.record_idle(EvidenceRank::Protocol, "hook-idle");
    assert!(decide(&ev, false, std::time::Instant::now()).is_none());
}

// --- SilenceState tests ---

#[test]
fn test_silence_state_no_pending_returns_none() {
    let mut s = SilenceState::new();
    assert!(s.check_silence().is_none());
}

// --- DecisionTrail tests (state-explain feature) ---
//
// The trail's single most valuable field is a REJECTED attempt's
// `outranked_by` — otherwise invisible, since every pre-existing call site
// discarded `record_busy`/`record_idle`'s returned `bool`. Reproduce the
// actual failure mode a stuck badge comes from: a weak attempt rejected by
// already-held stronger evidence.

#[test]
fn trail_records_a_rejected_busy_attempt_with_outranked_by() {
    let mut s = SilenceState::new();
    s.record_idle(EvidenceRank::Protocol, "hook-idle");
    let accepted = s.record_busy(EvidenceRank::Screen, "working-screen");
    assert!(
        !accepted,
        "Screen-rank busy must not reopen a Protocol-rank idle turn"
    );

    let last = s.trail.entries().last().expect("a trail entry was pushed");
    assert_eq!(last.kind, TrailKind::Busy);
    assert_eq!(last.rank, Some(EvidenceRank::Screen));
    assert_eq!(last.source, Some("working-screen"));
    assert!(!last.accepted);
    assert_eq!(
        last.outranked_by,
        Some((EvidenceRank::Protocol, "hook-idle")),
        "a rejection must name the evidence that outranked it"
    );
}

/// Code-review-caught gap: `record_awaiting`/`clear_awaiting` were not
/// wrapped with trail logging like their busy/idle siblings, so a rejected
/// awaiting attempt — a low-confidence screen-scrape question blocked by an
/// already-held confident one — never showed up in the trail at all.
#[test]
fn trail_records_a_rejected_awaiting_attempt_with_outranked_by() {
    let mut s = SilenceState::new();
    s.record_awaiting(EvidenceRank::Protocol, "hook-awaiting");
    let accepted = s.record_awaiting(EvidenceRank::Screen, "footer-heuristic");
    assert!(
        !accepted,
        "a low-confidence screen-scrape must not overwrite a confident protocol question"
    );

    let last = s.trail.entries().last().expect("a trail entry was pushed");
    assert_eq!(last.kind, TrailKind::Awaiting);
    assert_eq!(last.rank, Some(EvidenceRank::Screen));
    assert_eq!(last.source, Some("footer-heuristic"));
    assert!(!last.accepted);
    assert_eq!(
        last.outranked_by,
        Some((EvidenceRank::Protocol, "hook-awaiting"))
    );
}

#[test]
fn trail_records_clear_awaiting() {
    let mut s = SilenceState::new();
    s.record_awaiting(EvidenceRank::Protocol, "hook-awaiting");
    s.clear_awaiting();

    let last = s.trail.entries().last().expect("a trail entry was pushed");
    assert_eq!(last.kind, TrailKind::ClearAwaiting);
    assert!(last.accepted);
    assert_eq!(last.outranked_by, None);
}

#[test]
fn trail_records_an_accepted_attempt_with_no_outranked_by() {
    let mut s = SilenceState::new();
    let accepted = s.record_busy(EvidenceRank::Protocol, "hook-busy");
    assert!(accepted);

    let last = s.trail.entries().last().expect("a trail entry was pushed");
    assert!(last.accepted);
    assert_eq!(last.outranked_by, None);
}

/// Regression for a real bug a review pass caught: `note_busy_evidence()`'s
/// `clear_idle()` call (and repeated `record_busy` re-affirmation of the
/// same evidence) fires on essentially every PTY chunk of a busy streaming
/// turn, so without deduplication the bounded ring fills with identical
/// no-op entries within a handful of calls — evicting the one rejected
/// attempt someone opened the dump to find. A repeated identical outcome
/// must collapse into the existing entry, not push a new one.
#[test]
fn trail_collapses_repeated_identical_clear_idle_into_one_entry() {
    let mut s = SilenceState::new();
    for _ in 0..10 {
        s.clear_idle();
    }
    let entries: Vec<&TrailEntry> = s.trail.entries().collect();
    assert_eq!(
        entries.len(),
        1,
        "10 identical no-op clear_idle calls must collapse to a single trail entry"
    );
    assert_eq!(entries[0].kind, TrailKind::ClearIdle);
}

#[test]
fn trail_collapses_repeated_identical_busy_reaffirmation_into_one_entry() {
    let mut s = SilenceState::new();
    for _ in 0..10 {
        s.record_busy(EvidenceRank::Screen, "real-activity");
    }
    let entries: Vec<&TrailEntry> = s.trail.entries().collect();
    assert_eq!(
        entries.len(),
        1,
        "10 identical accepted busy re-affirmations must collapse to a single trail entry"
    );
}

/// A rejection that keeps repeating (the same weak attempt blocked by the
/// same held evidence, on every chunk) must ALSO collapse — repeating it 50
/// times isn't more informative than showing it once, and it would
/// otherwise flood the ring exactly like the accepted-evidence case above.
#[test]
fn trail_collapses_repeated_identical_rejection_into_one_entry() {
    let mut s = SilenceState::new();
    s.record_idle(EvidenceRank::Protocol, "hook-idle");
    for _ in 0..10 {
        s.record_busy(EvidenceRank::Screen, "working-screen");
    }
    let entries: Vec<&TrailEntry> = s.trail.entries().collect();
    // One entry for the initial accepted idle, one for the (collapsed)
    // repeated rejection.
    assert_eq!(entries.len(), 2);
    let rejection = entries.last().unwrap();
    assert!(!rejection.accepted);
    assert_eq!(
        rejection.outranked_by,
        Some((EvidenceRank::Protocol, "hook-idle"))
    );
}

/// A genuinely DIFFERENT outcome right after a repeat must still push a new
/// entry — dedup only collapses an exact repeat of the immediately
/// preceding one, it must never suppress a real transition.
#[test]
fn trail_does_not_collapse_a_genuinely_different_outcome() {
    let mut s = SilenceState::new();
    s.record_busy(EvidenceRank::Screen, "real-activity");
    s.record_busy(EvidenceRank::Screen, "real-activity"); // repeat, collapses
    s.record_idle(EvidenceRank::Protocol, "hook-idle"); // genuinely different
    let entries: Vec<&TrailEntry> = s.trail.entries().collect();
    assert_eq!(
        entries.len(),
        2,
        "the real transition must not be swallowed by dedup"
    );
    assert_eq!(entries[0].kind, TrailKind::Busy);
    assert_eq!(entries[1].kind, TrailKind::Idle);
}

#[test]
fn trail_marks_force_idle_entries_as_forced() {
    let mut s = SilenceState::new();
    s.force_idle(EvidenceRank::Screen, "agent-ready-screen");

    let last = s.trail.entries().last().expect("a trail entry was pushed");
    assert_eq!(last.kind, TrailKind::Idle);
    assert!(
        last.forced,
        "force_idle entries must be distinguishable from record_idle"
    );
    assert!(last.accepted, "force_idle always accepts");
}

#[test]
fn trail_records_user_submit_as_a_turn_boundary_marker() {
    let mut s = SilenceState::new();
    s.note_user_submission(true);

    let kinds: Vec<TrailKind> = s.trail.entries().map(|e| e.kind).collect();
    assert!(
        kinds.contains(&TrailKind::UserSubmit),
        "note_user_submission must push a turn-boundary marker: {kinds:?}"
    );
}

#[test]
fn trail_ring_is_bounded_and_drops_oldest_first() {
    let mut s = SilenceState::new();
    // Push well past capacity with alternating busy/idle so every call is
    // accepted (no rank gate blocks it) and therefore actually pushes.
    for i in 0..(TRAIL_CAPACITY * 2) {
        if i % 2 == 0 {
            s.record_busy(EvidenceRank::Protocol, "hook-busy");
        } else {
            s.record_idle(EvidenceRank::Protocol, "hook-idle");
        }
    }
    let entries: Vec<&TrailEntry> = s.trail.entries().collect();
    assert_eq!(
        entries.len(),
        TRAIL_CAPACITY,
        "ring must stay bounded at capacity"
    );
}

// --- explain_session_state_impl tests (state-explain feature, Phase 3) ---

#[test]
fn explain_session_state_decide_now_matches_an_independent_decide_call() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "explain-test-session";

    state.session_maps.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("claude".into()),
            turn_epoch: 1,
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        session_id.into(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    let mut silence = SilenceState::new();
    silence.record_idle(EvidenceRank::Protocol, "hook-idle");
    let evidence_snapshot = silence.evidence.clone();
    state.session_maps.silence_states.insert(
        session_id.into(),
        std::sync::Arc::new(parking_lot::Mutex::new(silence)),
    );

    let explain = explain_session_state_impl(&state, session_id)
        .expect("explain_session_state_impl should find the session");

    // Independent decide() call on the same evidence, for comparison — the
    // whole point of `decide_now` is that it can never disagree with a
    // fresh call against the same inputs.
    let expected = match decide(&evidence_snapshot, true, std::time::Instant::now()) {
        Some(Transition::ToBusy(_)) => Some("to_busy"),
        Some(Transition::ToIdle(_)) => Some("to_idle"),
        None => None,
    };
    assert_eq!(explain.evidence.decide_now, expected);
    assert_eq!(explain.evidence.decide_now, Some("to_idle"));
    assert_eq!(
        explain.evidence.idle,
        Some(EvidenceSnapshot {
            rank: "protocol",
            source: "hook-idle",
            age_ms: explain.evidence.idle.as_ref().unwrap().age_ms,
        })
    );

    // agent_state_rung must match session_state_with_shell_detailed's own
    // answer for the identical inputs — the two must never disagree.
    let (_, rung, ..) = state
        .session_state_with_shell_detailed(session_id)
        .expect("session_state_with_shell_detailed should find the session");
    assert_eq!(explain.visible.agent_state_rung, rung);
}

#[test]
fn explain_session_state_surfaces_a_rejected_trail_entry() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "explain-test-rejection";

    state.session_maps.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("claude".into()),
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        session_id.into(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    let mut silence = SilenceState::new();
    silence.record_idle(EvidenceRank::Protocol, "hook-idle");
    silence.record_busy(EvidenceRank::Screen, "working-screen");
    state.session_maps.silence_states.insert(
        session_id.into(),
        std::sync::Arc::new(parking_lot::Mutex::new(silence)),
    );

    let explain = explain_session_state_impl(&state, session_id)
        .expect("explain_session_state_impl should find the session");

    let rejected = explain
        .trail
        .iter()
        .find(|e| !e.accepted)
        .expect("the rejected busy attempt must appear in the trail");
    assert_eq!(rejected.kind, "busy");
    assert_eq!(rejected.source, Some("working-screen"));
    assert_eq!(
        rejected.outranked_by,
        Some(RankedSource {
            rank: "protocol",
            source: "hook-idle",
        })
    );
}

#[test]
fn explain_session_state_returns_none_for_an_unknown_session() {
    let state = crate::state::tests_support::make_test_app_state();
    assert!(explain_session_state_impl(&state, "no-such-session").is_none());
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
fn osc133_b_is_a_noop() {
    // 'B' (command start) is new, real production traffic as of
    // shell_integration.rs emitting 133;B — it falls into handle_osc133_event's
    // `_ => {}` catch-all, correctly, but nothing pinned that before this test.
    // A future edit that accidentally gives 'B' a state effect (or drops it
    // before the Osc133Event forwarding, which happens unconditionally for
    // every marker right after this call) should fail here.
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-osc133-b";
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));
    state
        .session_maps
        .has_osc133_integration
        .insert(session_id.to_string(), ());

    let mut rx = state.event_bus.subscribe();
    let mut proc = ChunkProcessor::new(None, None);

    // Establish a pending command via C first, so a wrongly-implemented 'B'
    // has state it could plausibly (and wrongly) clear or overwrite.
    proc.handle_osc133_event('C', "", session_id, &state);
    let _ = rx.try_recv(); // drain C's own busy-transition event, not under test
    proc.handle_osc133_event('B', "", session_id, &state);

    let current = state
        .session_maps
        .shell_states
        .get(session_id)
        .map(|s| s.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap();
    assert_eq!(current, SHELL_BUSY, "B must not change shell state");
    assert!(
        rx.try_recv().is_err(),
        "B must not emit a shell-state event onto the bus"
    );
    assert!(
        proc.pending_command.is_some(),
        "B must not clear the pending command C started"
    );

    proc.handle_osc133_event('D', "0", session_id, &state);
    let recorded = state
        .ai
        .session_knowledge
        .get(session_id)
        .map(|k| k.lock().commands.len())
        .unwrap_or(0);
    assert_eq!(
        recorded, 1,
        "the C→B→D sequence should still record exactly one outcome — B is transparent"
    );
}

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
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(!silence.explicit_busy());
    assert!(silence.idle_confirmed());
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn test_only_interpreters_take_the_argv0_detour() {
    assert!(is_script_interpreter("node"));
    assert!(is_script_interpreter("bun"));
    assert!(!is_script_interpreter("claude"));
    assert!(!is_script_interpreter("zsh"));
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

#[test]
fn test_working_row_cannot_relatch_a_declared_completed_turn() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "completed-working-row";
    state.session_maps.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("codex".into()),
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
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
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(std::sync::atomic::Ordering::Acquire),
        SHELL_IDLE
    );
    assert!(lifecycle.lock().idle_confirmed());
}

#[test]
fn test_codex_moving_working_row_reopens_completed_internal_continuation() {
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "codex-completed-internal-continuation";
    state.session_maps.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("codex".into()),
            suggested_actions: Some(vec!["Review diff".into()]),
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
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
    assert!(
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .suggested_actions
            .is_none()
    );
    let lifecycle = lifecycle.lock();
    assert!(!lifecycle.completion_declared_for_epoch(0));
    assert!(!lifecycle.idle_confirmed());
}

#[test]
fn test_claude_active_marker_reopens_premature_stop_hook_completion() {
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "claude-blocking-stop-hook";
    state.session_maps.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("claude".into()),
            suggested_actions: Some(vec!["Premature follow-up".into()]),
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
    lifecycle.mark_suggest_candidate(vec!["Premature follow-up".into()], 0);
    lifecycle.note_explicit_state(SHELL_IDLE, true);
    let lifecycle = Arc::new(Mutex::new(lifecycle));
    state
        .session_maps
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
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
    assert!(
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .suggested_actions
            .is_none()
    );
    let lifecycle = lifecycle.lock();
    assert!(!lifecycle.completion_declared_for_epoch(0));
    assert!(!lifecycle.explicit_idle());
    assert!(!lifecycle.idle_confirmed());
}

#[test]
fn test_declared_completion_turns_stale_working_screen_into_ready_evidence() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "completed-working-timer";
    agent_session(&state, session_id, SHELL_BUSY);
    {
        let mut session = state
            .session_maps
            .session_states
            .get_mut(session_id)
            .unwrap();
        session.agent_type = Some("codex".into());
        session.background_probe_satisfied_turn_epoch = Some(session.turn_epoch);
    }
    let lifecycle = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
    {
        let mut lifecycle = lifecycle.lock();
        lifecycle.mark_suggest_candidate(vec!["Review diff".into()], 0);
        lifecycle.screen_ready_pending_since =
            Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
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
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(std::sync::atomic::Ordering::Acquire),
        SHELL_IDLE
    );
}

/// The measurement `SCREEN_CLASSIFY_CALLS` was built for and never got.
///
/// #744-138c claimed the reader chunk path and the silence timer no longer each
/// classify the screen independently, and added a counter plus a `#[cfg(test)]`
/// accessor to prove it. The accessor had zero callers, so the claim went
/// unmeasured and `--all-targets` clippy reported the accessor as dead code.
/// Deleting it was the wrong fix: it is not dead, it is the only surviving
/// trace that a de-duplication was asserted and never checked.
///
/// Two halves, because the claim has two:
/// 1. the reader classifies at most once for one chunk, and publishes the
///    verdict to `cached_screen_activity`;
/// 2. the timer *reads* that field instead of calling the classifier again —
///    a structural property of `spawn_silence_timer`, which is an async loop
///    with no practical unit-test entry point, so it is asserted against the
///    source the same way `close_pty_never_runs_on_the_ipc_thread` does.
///
/// The counter is a process-wide static, so the delta is only meaningful under
/// a process-per-test runner. That is nextest, which is what this project runs
/// (src-tauri/AGENTS.md); the bound is deliberately one-sided so a shared-process runner
/// cannot make it flaky in the other direction.
#[test]
fn the_screen_is_classified_once_per_chunk_not_once_per_reader_and_once_per_timer() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "screen-classify-once";
    agent_session(&state, session_id, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .agent_type = Some("codex".into());
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 2000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();

    let before = screen_classify_calls();
    let mut processor = ChunkProcessor::new(None, None);
    processor.process_chunk("working on it\r\n", &silence, session_id, &state);
    let delta = screen_classify_calls() - before;
    assert!(
        delta <= 1,
        "one chunk must not classify the screen more than once, saw {delta}"
    );

    // The timer must reuse that verdict rather than produce its own.
    let source = include_str!("../pty.rs");
    let at = source
        .find("fn spawn_silence_timer(")
        .expect("spawn_silence_timer must exist");
    let body = &source[at..];
    let end = body
        .find("\n}\n")
        .expect("spawn_silence_timer must be a closed function");
    let body = &body[..end];
    assert!(
        body.contains("cached_screen_activity"),
        "the silence timer must read the reader's cached verdict"
    );
    assert!(
        !body.contains("detect_agent_screen_activity("),
        "the silence timer must NOT classify the screen itself — that is the \
         second call #744-138c removed, and the counter above cannot see it \
         from a unit test"
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
fn test_interrupt_request_plus_interrupted_screen_confirms_idle() {
    let mut silence = SilenceState::new();
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.note_interrupt_requested();
    assert!(silence.note_interrupted_screen());
    assert!(!silence.explicit_busy());
    assert!(silence.idle_confirmed());
}

#[test]
fn test_ctrl_c_alone_never_confirms_idle() {
    let mut silence = SilenceState::new();
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.note_interrupt_requested();
    assert!(silence.explicit_busy());
    assert!(!silence.idle_confirmed());
}

#[test]
fn test_working_screen_recovers_idle_to_busy() {
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "codex-false-idle";
    state
        .session_maps
        .shell_states
        .insert(sid.into(), AtomicU8::new(SHELL_IDLE));
    state
        .session_maps
        .last_output_ms
        .insert(sid.into(), AtomicU64::new(1));
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(sid.into(), silence.clone());

    apply_working_evidence(&state, &silence, sid, now_epoch_ms(), "working-screen");

    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
    assert!(!silence.lock().idle_confirmed());
}

#[test]
fn test_explicit_idle_outvotes_stale_working_screen() {
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "codex-stale-working";
    state
        .session_maps
        .shell_states
        .insert(sid.into(), AtomicU8::new(SHELL_IDLE));
    state
        .session_maps
        .last_output_ms
        .insert(sid.into(), AtomicU64::new(1));
    let mut sl = SilenceState::new();
    sl.note_explicit_state(SHELL_IDLE, true);
    let silence = Arc::new(Mutex::new(sl));
    state
        .session_maps
        .silence_states
        .insert(sid.into(), silence.clone());

    apply_working_evidence(&state, &silence, sid, now_epoch_ms(), "working-screen");

    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_IDLE
    );
    assert!(silence.lock().idle_confirmed());
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
        state.session_maps.session_states.insert(
            sid.into(),
            crate::state::SessionState {
                agent_type: Some(agent.into()),
                ..Default::default()
            },
        );
        state
            .session_maps
            .shell_states
            .insert(sid.into(), AtomicU8::new(SHELL_IDLE));
        state
            .session_maps
            .last_output_ms
            .insert(sid.into(), AtomicU64::new(1));
        let lifecycle = Arc::new(Mutex::new(SilenceState::new()));
        state
            .session_maps
            .silence_states
            .insert(sid.into(), lifecycle.clone());

        for iteration in 0..256 {
            state
                .session_maps
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
                state
                    .session_maps
                    .shell_states
                    .get(sid)
                    .unwrap()
                    .load(Ordering::Acquire),
                SHELL_BUSY,
                "{sid} failed to repair false idle at iteration {iteration}"
            );
            assert!(!lifecycle.lock().idle_confirmed());
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
        assert!(completed.idle_confirmed(), "{agent} completed trace");

        let silent = replay_sanitized_agent_trace(
            agent,
            &[
                SanitizedTraceStep::Submit,
                SanitizedTraceStep::ReadyScreen,
                SanitizedTraceStep::ReadyScreen,
            ],
        );
        assert!(
            !silent.idle_confirmed(),
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
            !partial_redraw.idle_confirmed(),
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
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state
        .session_maps
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());
    state.session_maps.last_output_ms.insert(
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
    state.session_maps.session_states.insert(
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
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_IDLE));
    assert!(!prompt_probe_applies(&state, sid));
}

#[test]
fn prompt_probe_demands_the_process_snapshot() {
    let sid = "s";
    let state = busy_plain_shell(sid, SHELL_PROMPT_PROBE_SILENCE_MS + 500);
    state
        .session_maps
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
        assert!(
            silence.idle_confirmed(),
            "{agent} composer is terminal-ready"
        );

        let state = crate::state::tests_support::make_test_app_state();
        let sid = format!("background-{agent}");
        state.session_maps.session_states.insert(
            sid.clone(),
            crate::state::SessionState {
                agent_type: Some(agent.to_string()),
                background_work: true,
                ..Default::default()
            },
        );
        state
            .session_maps
            .shell_states
            .insert(sid.clone(), std::sync::atomic::AtomicU8::new(SHELL_IDLE));
        state
            .session_maps
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
    let mut processes = vec![
        process(10, 1, "codex", "codex"),
        process(11, 10, "tuic-bridge", "tuic-bridge"),
        process(12, 10, "mdkb", "mdkb serve"),
        // Descendants owned by helper plumbing are ignored with the helper.
        process(14, 12, "sqlite-worker", "sqlite-worker"),
    ];
    // Recognised by its command line rather than its name, which only holds
    // where the process snapshot reports one — Toolhelp gives the executable
    // name alone, so `node` stays meaningful work on Windows by design. See
    // `windows_helper_classification_does_not_guess_node_arguments`.
    if cfg!(not(windows)) {
        processes.push(process(13, 10, "node", "node /opt/codex/node_repl.js"));
    }
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
    state
        .session_maps
        .session_states
        .get_mut(child_id)
        .unwrap()
        .agent_type = Some("codex".into());
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .lock()
        .screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);

    let first_ready = try_timer_idle_transition(
        &state,
        &state
            .session_maps
            .silence_states
            .get(child_id)
            .unwrap()
            .clone(),
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
        &state
            .session_maps
            .silence_states
            .get(child_id)
            .unwrap()
            .clone(),
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
    state
        .session_maps
        .session_states
        .get_mut(child_id)
        .unwrap()
        .agent_type = Some("codex".into());
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    let silence = state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .clone();

    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    silence.lock().screen_ready_pending_since =
        Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
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
            .session_maps
            .session_states
            .get(child_id)
            .unwrap()
            .background_probe_satisfied_turn_epoch,
        Some(0)
    );

    apply_working_evidence(&state, &silence, child_id, now_epoch_ms(), "working-screen");
    {
        let session = state.session_maps.session_states.get(child_id).unwrap();
        assert_eq!(session.background_probe_satisfied_turn_epoch, None);
        assert_eq!(session.background_probe_turn_epoch, None);
        assert_eq!(session.background_probe_after_generation, None);
        assert_eq!(session.background_snapshot_generation, 2);
        assert!(!session.background_work);
    }

    silence.lock().screen_ready_pending_since =
        Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
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
        let session = state.session_maps.session_states.get(child_id).unwrap();
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
fn the_foreground_probe_records_process_rank_idle_evidence() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "foreground-probe-records-evidence";
    let silence = probe_evidence_fixture(&state, session_id);

    // Nothing has been reconciled yet: the probe arms itself and holds the
    // turn open, contributing no evidence at all.
    let armed = try_timer_idle_transition(
        &state,
        &silence,
        session_id,
        AgentScreenActivity::Ready,
        Some("codex"),
        Some(0),
    );
    assert!(!armed.transitioned);
    assert!(
        armed.evidence.is_none(),
        "an unreconciled probe must not produce evidence"
    );

    // A snapshot in which the agent stands alone — no meaningful descendant.
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
    assert!(
        !state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .background_work
    );

    let closed = try_timer_idle_transition(
        &state,
        &silence,
        session_id,
        AgentScreenActivity::Ready,
        Some("codex"),
        Some(0),
    );
    assert!(closed.transitioned);
    let evidence = closed.evidence.expect("a close must carry its evidence");
    // These two assertions are the whole point of the story: reduce the probe
    // back to a boolean gate and the turn still closes, but on the ready
    // screen that asked for the probe (`Screen`/`agent-ready-screen`) instead
    // of on the process observation that answered it.
    assert_eq!(
        evidence.rank,
        EvidenceRank::Process,
        "the probe read the process table, so its evidence is Process rank"
    );
    assert_eq!(
        evidence.source, "process",
        "activity_source must name the probe, not the screen, and must stay \
         distinguishable from protocol-stale and from a silence timeout"
    );
}

#[test]
fn a_probe_that_still_sees_work_does_not_claim_process_evidence() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "foreground-probe-still-working";
    let silence = probe_evidence_fixture(&state, session_id);

    assert!(
        !try_timer_idle_transition(
            &state,
            &silence,
            session_id,
            AgentScreenActivity::Ready,
            Some("codex"),
            Some(0),
        )
        .transitioned
    );

    // Same reconciliation, but the tree still holds a build under the agent.
    state.process_snapshot_cache.store(Some(vec![
        process(10, 1, "codex", "codex"),
        process(11, 10, "cargo", "cargo test"),
    ]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        session_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    assert!(
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .background_work
    );

    let closed = try_timer_idle_transition(
        &state,
        &silence,
        session_id,
        AgentScreenActivity::Ready,
        Some("codex"),
        Some(0),
    );
    // The transition itself is unchanged by this story — a reconciled probe
    // opened the gate before it and still does. What must NOT happen is the
    // close claiming a process observation it did not make.
    assert!(closed.transitioned);
    let evidence = closed.evidence.expect("a close must carry its evidence");
    assert_eq!(evidence.rank, EvidenceRank::Screen);
    assert_eq!(
        evidence.source, "agent-ready-screen",
        "a tree that still shows work says nothing about this turn ending"
    );
}

/// Raising the close from `Screen` to `Process` raises what the NEXT turn has
/// to outrank, and a working screen is only `Screen` rank. The reopen survives
/// because `note_busy_evidence` clears the held idle evidence before
/// `record_busy` meets the rank gate — delete that `clear_idle` and a session
/// closed by the probe can never go busy again without a Protocol-rank signal
/// (#771-4733).
#[test]
fn a_process_rank_close_does_not_strand_the_next_turn_idle() {
    use std::sync::atomic::Ordering;
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "foreground-probe-reopen";
    let silence = probe_evidence_fixture(&state, session_id);

    assert!(
        !try_timer_idle_transition(
            &state,
            &silence,
            session_id,
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
        session_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    let closed = try_timer_idle_transition(
        &state,
        &silence,
        session_id,
        AgentScreenActivity::Ready,
        Some("codex"),
        Some(0),
    );
    assert!(closed.transitioned);
    assert_eq!(
        closed.evidence.map(|e| e.rank),
        Some(EvidenceRank::Process),
        "this test is only meaningful after a Process-rank close"
    );

    // No completion and no hook idle, so this is the ordinary `Screen`-rank
    // reopen — the weakest evidence that must still be able to start a turn.
    apply_working_evidence(
        &state,
        &silence,
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
        "a working screen must reopen a turn the process probe closed"
    );
}

#[test]
fn already_busy_working_evidence_invalidates_only_probe_boundaries() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "background-already-busy";
    agent_session(&state, child_id, SHELL_BUSY);
    {
        let mut session = state.session_maps.session_states.get_mut(child_id).unwrap();
        session.agent_type = Some("codex".into());
        session.background_work = true;
        session.background_snapshot_generation = 9;
        session.background_probe_turn_epoch = Some(0);
        session.background_probe_after_generation = Some(8);
        session.background_probe_satisfied_turn_epoch = Some(0);
    }
    let silence = state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .clone();

    apply_working_evidence(&state, &silence, child_id, now_epoch_ms(), "working-screen");
    {
        let session = state.session_maps.session_states.get(child_id).unwrap();
        assert_eq!(session.background_probe_turn_epoch, None);
        assert_eq!(session.background_probe_after_generation, None);
        assert_eq!(session.background_probe_satisfied_turn_epoch, None);
        assert!(session.background_work);
        assert_eq!(session.background_snapshot_generation, 9);
    }

    {
        let mut session = state.session_maps.session_states.get_mut(child_id).unwrap();
        session.background_probe_turn_epoch = Some(0);
        session.background_probe_after_generation = Some(9);
        session.background_probe_satisfied_turn_epoch = Some(0);
    }
    transition_explicit_shell_state_with_hook(&state, child_id, SHELL_BUSY, "busy", true, || {});
    let session = state.session_maps.session_states.get(child_id).unwrap();
    assert_eq!(session.background_probe_turn_epoch, None);
    assert_eq!(session.background_probe_after_generation, None);
    assert_eq!(session.background_probe_satisfied_turn_epoch, None);
    assert!(session.background_work);
    assert_eq!(session.background_snapshot_generation, 9);
    assert_eq!(
        state
            .session_maps
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
    state
        .session_maps
        .session_states
        .get_mut(child_id)
        .unwrap()
        .agent_type = Some("codex".into());
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .lock()
        .mark_suggest_candidate(vec!["Review result".to_string()], 0);
    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .lock()
        .screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);

    assert!(
        !try_timer_idle_transition(
            &state,
            &state
                .session_maps
                .silence_states
                .get(child_id)
                .unwrap()
                .clone(),
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
            &state
                .session_maps
                .silence_states
                .get(child_id)
                .unwrap()
                .clone(),
            child_id,
            AgentScreenActivity::Ready,
            Some("codex"),
            Some(0),
        )
        .transitioned
    );
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());
    let silence = state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .clone();
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
    state
        .session_maps
        .session_states
        .get_mut(child_id)
        .unwrap()
        .agent_type = Some("claude".into());
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state
        .session_maps
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
fn explicit_agent_idle_child_absent_restores_api_state_and_notifies_once() {
    for (session_id, declare_completion, expected_state) in [
        ("background-explicit-idle", false, "idle"),
        ("background-explicit-completed", true, "completed"),
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
        if declare_completion {
            state
                .session_maps
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
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .agent_type = Some("codex".into());
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .background_probe_turn_epoch = Some(0);
    state
        .session_maps
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
            .session_maps
            .silence_states
            .insert(sid.to_string(), silence.clone());
        state.session_maps.shell_states.insert(
            sid.to_string(),
            std::sync::atomic::AtomicU8::new(SHELL_NULL),
        );
        // Six rows: a screenful plus one line is enough to scroll and grow
        // the durable log, which is what "real output" means in primary.
        state
            .grid
            .vt_log_buffers
            .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(6, 40, 1000)));
        state
            .session_maps
            .output_buffers
            .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
        state
            .session_maps
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

/// Same raw-stream invisibility applies to every non-normal state, not
/// just the original state=1 case above — error/indeterminate/warning
/// are just as absent from the vt100-rendered clean rows.
#[test]
fn test_osc94_error_indeterminate_warning_from_raw_stream() {
    use crate::output_parser::{ParsedEvent, parse_osc94};

    for raw in [
        "\x1b]9;4;2;50\x07", // error
        "\x1b]9;4;3;\x07",   // indeterminate, no value
        "\x1b]9;4;4;30\x07", // warning
    ] {
        let event = parse_osc94(raw);
        assert!(
            matches!(event, Some(ParsedEvent::Progress { .. })),
            "expected Progress from raw OSC 9;4 sequence {:?}, got: {:?}",
            raw,
            event
        );
    }
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
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_NULL));
    state.session_maps.last_output_ms.insert(
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
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Relaxed),
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
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state
        .session_maps
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());

    // Set last output to 600ms ago (> SHELL_IDLE_MS)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .session_maps
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
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Relaxed),
        SHELL_IDLE
    );
}

#[test]
fn test_shell_state_no_idle_with_subtasks() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

    state.session_maps.session_states.insert(
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
        .session_maps
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
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

    state.session_maps.session_states.insert(
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
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 31_000));

    assert!(
        should_transition_idle(&state, sid).should_transition,
        "should transition idle when active_sub_tasks > 0 but elapsed >= SUBTASK_STALE_MS"
    );
    // Verify the stale counter was force-cleared
    let sub = state
        .session_maps
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
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state.session_maps.session_states.insert(
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
        .session_maps
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
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state
        .session_maps
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .session_maps
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
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

    // Agent session: agent_type is set
    state.session_maps.session_states.insert(
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
        .session_maps
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
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

    // Agent session: agent_type is set
    state.session_maps.session_states.insert(
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
        .session_maps
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
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state
        .session_maps
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .session_maps
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
        .session_maps
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
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Relaxed),
        SHELL_IDLE
    );
}

#[test]
fn test_shell_state_idle_to_busy_on_real_output() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_IDLE));

    assert!(
        try_shell_transition(&state, sid, SHELL_IDLE, SHELL_BUSY, true),
        "should transition idle → busy on real output"
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Relaxed),
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
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state
        .session_maps
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());

    // last_output_ms is 600ms ago (stale — would normally trigger idle)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .session_maps
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
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
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
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
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
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));
    state.session_maps.session_states.insert(
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
        .session_maps
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
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
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

/// Regression for the 2026-09-01 fix: `ChoicePrompt` screen-scraping must be
/// suppressed for a hook-instrumented session, mirroring the suppression
/// `suppress_heuristic_question` already gives the generic `Question` type.
/// Before this fix, `parse_choice_prompt` ran unconditionally regardless of
/// hook instrumentation — live-observed producing a confusing duplicate
/// signal: a hook-instrumented Claude session logged both `[ChoicePrompt]
/// ... title="Do you want to proceed?"` (screen-scrape) AND a hook-driven
/// confident `question — awaitingInput transition` for the exact same
/// dialog, seconds apart.
#[test]
fn test_chunk_processor_choice_prompt_suppressed_when_hook_instrumented() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "test-cp-choice-hook-suppressed";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));
    state.session_maps.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            hook_instrumented: true,
            agent_type: Some("claude".to_string()),
            ..Default::default()
        },
    );

    let mut cp = ChunkProcessor::new(None, None);
    let mut utf8_buf = Utf8ReadBuffer::new();
    let mut esc_buf = EscapeAwareBuffer::new();
    let mut feed = |cp: &mut ChunkProcessor, bytes: &[u8]| {
        let utf8_data = utf8_buf.push(bytes);
        let esc_data = esc_buf.push(&utf8_data);
        let _ = cp.process_chunk(&esc_data, &silence, sid, state.as_ref());
    };

    // Establish hook_state_seen=true the same way production does — a real
    // OSC 7770 `state=` event — rather than reaching into SilenceState's
    // private field directly.
    feed(&mut cp, b"\x1b]7770;state=busy\x07");

    let mut rx = state.event_bus.subscribe();

    // The same edit-confirm screen `test_chunk_processor_dedup_choice_prompt`
    // proves DOES fire a ChoicePrompt for a non-hook-instrumented session.
    let screen_bytes = b"Do you want to make this edit to CLAUDE.md?\r\n\
          \xe2\x9d\xaf 1. Yes\r\n\
          \x20\x20 2. Yes, allow all edits (shift+tab)\r\n\
          \x20\x20 3. No\r\n\
          \r\n\
          Esc to cancel \xc2\xb7 Tab to amend\r\n";
    feed(&mut cp, screen_bytes);

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
        "a hook-instrumented session must never screen-scrape a ChoicePrompt — \
         the hook's own state=awaiting already covers this dialog"
    );
    assert!(
        cp.last_choice_prompt_sig.is_none(),
        "no dedup signature should be recorded for a signal that was never detected"
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
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
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
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
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
    // Joined and normalised with the host separator, so compare in one spelling.
    assert_eq!(
        resolved.as_deref().map(crate::test_support::slashed),
        Some("/home/user/repo/plans/foo.md".to_string())
    );
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
    assert_eq!(
        resolved.as_deref().map(crate::test_support::slashed),
        Some("/home/user/repo/plans/foo.md".to_string())
    );
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

/// A command that spawns and exits at once, whatever the host is. `/bin/echo`
/// is not a path Windows can start.
fn trivial_command() -> CommandBuilder {
    let (shell, flag) = crate::test_support::host_shell();
    let mut command = CommandBuilder::new(shell);
    command.arg(flag);
    command.arg("echo tuic-spawn-probe");
    command
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

/// Neither existing `build_shell_command` test asserts the `-l` (login shell)
/// flag or any env var — both would pass unnoticed if `inject_unix_terminal_env`
/// or the unconditional `-l` append were ever dropped from this function.
#[cfg(not(windows))]
#[test]
fn build_shell_command_appends_login_flag_on_unix() {
    let cmd = super::build_shell_command("/bin/zsh");
    let argv = cmd.as_unix_command_line().unwrap();
    assert!(
        argv.split_whitespace().any(|token| token == "-l"),
        "build_shell_command must append -l (login shell) on unix: {argv}"
    );
}

#[cfg(not(windows))]
#[test]
fn build_shell_command_applies_terminal_env() {
    let cmd = super::build_shell_command("/bin/sh");
    assert_eq!(
        cmd.get_env("TERM_PROGRAM")
            .map(|v| v.to_string_lossy().to_string()),
        Some("ghostty".to_string()),
        "build_shell_command must call inject_unix_terminal_env"
    );
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
        .session_maps
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
    let line = describe_lifecycle_payload(SUMMARY_CHILD, &payload);
    assert!(line.starts_with("child agent 8c261794 "), "{line}");
    assert!(
        line.contains("queued"),
        "the notice must not read as a final failure when the prompt is still pending: {line}"
    );
    assert!(!line.contains('\n'), "the line is typed into a composer");
}

#[test]
fn try_shell_transition_busy_to_idle_pushes_state_change_to_parent_inbox() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-idle-sess";
    let parent_id = "parent-idle-sess";

    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    // Must have a session_state with agent_type to qualify for idle notification
    let ss = crate::state::SessionState {
        agent_type: Some("claude".to_string()),
        ..Default::default()
    };
    state
        .session_maps
        .session_states
        .insert(child_id.to_string(), ss);
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state.session_maps.silence_states.insert(
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
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state.session_maps.silence_states.insert(
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
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
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

    assert!(!emit_pending_suggest_if_idle(&state, &silence, child_id));
    assert!(set_background_work_for_epoch(&state, child_id, 0, 1, false));
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(inbox.len(), 1);
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "completed");
    assert!(!emit_pending_suggest_if_idle(&state, &silence, child_id));
}

#[cfg(unix)]
#[test]
fn background_probe_settlement_retries_pending_orchestrator_mail_wake() {
    let state = crate::state::tests_support::make_test_app_state();
    let parent_id = "parent-background-probe-mail";
    let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        &state,
        parent_id,
        Box::new(RecordingWriter {
            bytes: Arc::clone(&bytes),
        }),
        TtyMode::Raw,
    );
    agent_session(&state, parent_id, SHELL_IDLE);
    {
        let mut session = state
            .session_maps
            .session_states
            .get_mut(parent_id)
            .expect("parent session state");
        session.agent_type = Some("codex".to_string());
        session.background_probe_turn_epoch = Some(0);
        session.background_probe_after_generation = Some(0);
    }
    state.orchestrator_peers.insert(parent_id.to_string());
    state.agent_inbox.insert(
        parent_id.to_string(),
        std::collections::VecDeque::from([crate::state::AgentMessage {
            id: "peer-result".to_string(),
            from_tuic_session: "child".to_string(),
            from_name: "worker".to_string(),
            content: "secret child result".to_string(),
            timestamp: 1,
            delivered_via_channel: false,
        }]),
    );

    assert_eq!(
        route_registered_orchestrator_mail(&state, parent_id, "peer-result", 1),
        Some(crate::state::OrchestratorDeliveryAssignment::InboxOnly),
        "the pending probe must keep the parent working and the payload inbox-only"
    );
    assert_eq!(state.orchestrator_wake_needed_through(parent_id), Some(1));

    assert!(set_background_work_for_epoch(
        &state, parent_id, 0, 1, false
    ));

    let written = String::from_utf8(bytes.lock().unwrap().clone()).expect("UTF-8 wake");
    assert!(
        written.contains("message available"),
        "wake was not submitted: {written:?}"
    );
    assert!(
        written.contains("agent action=inbox"),
        "wake omitted the inbox command: {written:?}"
    );
    assert!(
        !written.contains("secret child result"),
        "the child payload escaped the inbox: {written:?}"
    );
    assert_eq!(
        state.agent_inbox.get(parent_id).unwrap()[0].content,
        "secret child result"
    );
    assert_eq!(
        state.orchestrator_wake_needed_through(parent_id),
        Some(1),
        "the submitted generic notice remains pending until the parent reads the inbox"
    );
}

/// The canonical lifecycle can say idle while the composer still refuses an
/// injection — a draft, an open question, an unconfirmed idle. That first
/// attempt starts no write and burns the wake budget, and before the idle-edge
/// re-arm nothing ever announced the mail again: the notice was owed forever
/// and never typed.
#[cfg(unix)]
#[test]
fn a_composer_that_refused_the_first_wake_still_gets_one_at_the_next_idle() {
    let state = crate::state::tests_support::make_test_app_state();
    let parent_id = "parent-refused-first-wake";
    agent_session(&state, parent_id, SHELL_IDLE);
    let bytes = insert_recording_session(&state, parent_id);
    state.orchestrator_peers.insert(parent_id.to_string());
    state.agent_inbox.insert(
        parent_id.to_string(),
        std::collections::VecDeque::from([crate::state::AgentMessage {
            id: "peer-result".to_string(),
            from_tuic_session: "child".to_string(),
            from_name: "worker".to_string(),
            content: "secret child result".to_string(),
            timestamp: 1,
            delivered_via_channel: false,
        }]),
    );

    let mut buffer = InputLineBuffer::new();
    buffer.feed("Boss draft");
    state
        .session_maps
        .input_buffers
        .insert(parent_id.to_string(), parking_lot::Mutex::new(buffer));

    assert_eq!(
        route_registered_orchestrator_mail(&state, parent_id, "peer-result", 1),
        Some(crate::state::OrchestratorDeliveryAssignment::InboxOnly),
        "a draft in the composer must not be spliced into"
    );
    assert!(
        bytes.lock().unwrap().is_empty(),
        "a refused claim must write nothing"
    );
    assert_eq!(state.orchestrator_wake_needed_through(parent_id), Some(1));

    // The draft is gone and the turn settles — a real idle edge, not a repeat of
    // the lifecycle that refused the claim.
    state.session_maps.input_buffers.remove(parent_id);
    {
        let mut session = state
            .session_maps
            .session_states
            .get_mut(parent_id)
            .expect("parent session state");
        session.background_probe_turn_epoch = Some(0);
        session.background_probe_after_generation = Some(0);
    }
    assert!(set_background_work_for_epoch(
        &state, parent_id, 0, 1, false
    ));

    let written = String::from_utf8(bytes.lock().unwrap().clone()).expect("UTF-8 wake");
    assert!(
        written.contains("agent action=inbox"),
        "the idle edge owed the parent a wake: {written:?}"
    );
    assert!(
        !written.contains("secret child result"),
        "the child payload escaped the inbox: {written:?}"
    );
}

#[test]
fn cursor_prefix_completion_preserves_background_epoch_release() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-background-cursor-completed";
    agent_session(&state, child_id, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(child_id)
        .unwrap()
        .background_work = true;
    state.grid.vt_log_buffers.insert(
        child_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .clone();
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
        state.grid.vt_log_buffers.insert(
            child_id.clone(),
            Mutex::new(crate::state::VtLogBuffer::new(10, 80, 1000)),
        );
        let silence = state
            .session_maps
            .silence_states
            .get(&child_id)
            .unwrap()
            .clone();
        let mut processor = ChunkProcessor::new(None, None);

        note_submitted_input(&state, &child_id);
        processor.process_chunk(
            &"x".repeat(wrap_count * 80 + 1),
            &silence,
            &child_id,
            &state,
        );
        {
            let mut session = state
                .session_maps
                .session_states
                .get_mut(&child_id)
                .unwrap();
            session.background_probe_turn_epoch = Some(1);
            session.background_probe_after_generation = Some(0);
        }
        assert!(set_background_work_for_epoch(&state, &child_id, 1, 1, true));
        assert!(
            state
                .session_maps
                .session_states
                .get(&child_id)
                .unwrap()
                .background_work
        );

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
    state.grid.vt_log_buffers.insert(
        child_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .clone();
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
    assert_eq!(
        state
            .session_maps
            .session_states
            .get(child_id)
            .unwrap()
            .turn_epoch,
        1
    );

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
        state.grid.vt_log_buffers.insert(
            child_id.clone(),
            Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
        );
        let silence = state
            .session_maps
            .silence_states
            .get(&child_id)
            .unwrap()
            .clone();
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
        state.grid.vt_log_buffers.insert(
            child_id.clone(),
            Mutex::new(crate::state::VtLogBuffer::new(24, 14, 1000)),
        );
        let silence = state
            .session_maps
            .silence_states
            .get(&child_id)
            .unwrap()
            .clone();
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
        state.grid.vt_log_buffers.insert(
            child_id.to_string(),
            Mutex::new(crate::state::VtLogBuffer::new(24, columns, 1000)),
        );
        let silence = state
            .session_maps
            .silence_states
            .get(child_id)
            .unwrap()
            .clone();
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
    state.grid.vt_log_buffers.insert(
        child_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 10, 1000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .clone();
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
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
            turn_epoch: 7,
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state.session_maps.silence_states.insert(
        child_id.to_string(),
        Arc::new(Mutex::new(SilenceState::new())),
    );

    note_submitted_input(&state, child_id);
    assert!(!set_background_work_for_epoch(
        &state, child_id, 7, 1, false
    ));
    assert!(
        state
            .session_maps
            .session_states
            .get(child_id)
            .unwrap()
            .background_work
    );
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());
}

#[test]
fn background_snapshot_teardown_does_not_recreate_lifecycle_or_notify() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "background-teardown";
    let parent_id = "background-teardown-parent";
    agent_session(&state, child_id, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(child_id)
        .unwrap()
        .background_work = true;
    state
        .session_maps
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
            state.session_maps.silence_states.remove(child_id);
            state.session_maps.shell_states.remove(child_id);
            state.session_maps.session_states.remove(child_id);
        },
    ));
    assert!(!state.session_maps.silence_states.contains_key(child_id));
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());
}

#[test]
fn failed_or_invalid_cached_snapshot_preserves_background_work() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "background-snapshot-failure";
    state.session_maps.session_states.insert(
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
            .session_maps
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
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
            turn_epoch: 4,
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state.session_maps.silence_states.insert(
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
    assert!(
        !state
            .session_maps
            .session_states
            .get(child_id)
            .unwrap()
            .background_work
    );
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "idle");
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
fn declared_completion_does_not_emit_ambiguous_idle_lifecycle() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-completed-sess";
    let parent_id = "parent-completed-sess";

    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    let mut silence = SilenceState::new();
    silence.mark_suggest_candidate(vec!["Review result".to_string()], 0);
    state
        .session_maps
        .silence_states
        .insert(child_id.to_string(), Arc::new(Mutex::new(silence)));

    assert!(try_shell_transition(
        &state, child_id, SHELL_BUSY, SHELL_IDLE, true
    ));

    assert!(
        state.agent_inbox.get(parent_id).unwrap().is_empty(),
        "the suggest drain must publish completed instead of an earlier idle"
    );
    let silence = state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .clone();
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
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.pending_initial_prompts.insert(
        child_id.to_string(),
        crate::state::PendingInitialPrompt::new("do the task"),
    );

    assert!(notify_initial_prompt_timeout_if_pending(&state, child_id));
    assert!(!notify_initial_prompt_timeout_if_pending(&state, child_id));

    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(inbox.len(), 1, "timeout notification must be emitted once");
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["type"], "prompt_delivery_failed");
    assert_eq!(content["reason"], "timeout");
    assert_eq!(content["session_id"], child_id);
    // The regression: the watchdog used to report the failure AND drop the
    // prompt, so nothing retried and the child sat idle as if spawned with no
    // work. The prompt is preserved, and it rides along for re-delivery.
    assert_eq!(content["prompt"], "do the task");
    assert_eq!(content["retrying"], true);
    assert_eq!(
        state
            .pending_initial_prompts
            .get(child_id)
            .map(|pending| pending.prompt.clone()),
        Some("do the task".to_string()),
        "a reported timeout must not discard the child's task"
    );
}

/// Dialog detection, which the fixed 30s timeout had none of: a child parked on
/// "Do you trust the contents of this directory?" is not a child whose task is
/// void, and the parent is told which of the two it is.
#[test]
fn pending_initial_prompt_names_a_startup_dialog_as_the_cause() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let child_id = "child-prompt-dialog";
    let parent_id = "parent-prompt-dialog";
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            question_confident: true,
            ..Default::default()
        },
    );
    state.pending_initial_prompts.insert(
        child_id.to_string(),
        crate::state::PendingInitialPrompt::new("review the draft"),
    );

    assert!(notify_initial_prompt_timeout_if_pending(&state, child_id));

    let inbox = state.agent_inbox.get(parent_id).unwrap();
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["reason"], "startup_dialog");
    assert!(
        describe_lifecycle_payload(child_id, &content).contains("startup dialog"),
        "the typed one-liner must say what the parent is waiting on"
    );
}

/// The retry half: once the dialog is answered the child reaches a ready prompt,
/// the queued entry is typed, and the parent that was warned is told so.
#[cfg(unix)]
#[test]
fn a_prompt_that_lands_after_the_warning_closes_the_loop() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-prompt-late";
    let parent_id = "parent-prompt-late";
    agent_session(&state, child_id, SHELL_IDLE);
    insert_recording_session(&state, child_id);
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.pending_initial_prompts.insert(
        child_id.to_string(),
        crate::state::PendingInitialPrompt {
            prompt: "review the draft".to_string(),
            notified: true,
        },
    );
    state
        .pending_injections
        .entry(child_id.to_string())
        .or_default()
        .push_back(crate::state::PendingInjection::initial_prompt(
            "review the draft",
        ));

    flush_pending_injections_blocking(&state, child_id);

    assert!(
        !state.pending_initial_prompts.contains_key(child_id),
        "delivery must clear the marker"
    );
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(
        content["type"], "prompt_delivered",
        "a parent told the prompt failed must not be left believing it"
    );
}

#[test]
fn try_shell_transition_non_agent_session_does_not_push_idle_notification() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "non-agent-sess";
    let parent_id = "parent-non-agent-sess";

    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    // No agent_type set — plain shell session
    state
        .session_maps
        .session_states
        .insert(child_id.to_string(), crate::state::SessionState::default());
    state.session_maps.shell_states.insert(
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
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    let ss = crate::state::SessionState {
        agent_type: Some("claude".to_string()),
        ..Default::default()
    };
    state
        .session_maps
        .session_states
        .insert(child_id.to_string(), ss);
    state.session_maps.shell_states.insert(
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
    state
        .session_maps
        .has_osc133_integration
        .insert(sid.to_string(), ());
    state
        .session_maps
        .has_tuic_state_integration
        .insert(sid.to_string(), ());
    state
        .session_maps
        .turn_error_flags
        .insert(sid.to_string(), ());

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
        !state
            .session_maps
            .has_tuic_state_integration
            .contains_key(sid),
        "must not leak a permanent entry per session UUID on normal exit"
    );
    assert!(
        !state.session_maps.turn_error_flags.contains_key(sid),
        "must not leak a pending failure flag past a normal session exit"
    );
}

// ── PTY-injection message delivery (Step 2) ─────────────────────

fn agent_session(state: &crate::state::AppState, sid: &str, shell: u8) {
    use std::sync::atomic::AtomicU8;
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(shell));
    state.session_maps.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            ..Default::default()
        },
    );
    let mut silence = SilenceState::new();
    if shell == SHELL_IDLE {
        silence.confirm_idle();
    }
    state
        .session_maps
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
fn submitted_input_lifecycle_peer_injection_starts_new_turn_and_clears_completion() {
    let state = crate::state::tests_support::make_test_app_state();
    completed_agent_session(&state, "completed");
    state.pending_injections.insert(
        "completed".to_string(),
        std::collections::VecDeque::from([crate::state::PendingInjection::notice("follow up")]),
    );

    flush_one_pending_as_submitted(&state, "completed");

    let snapshot = state.session_state_with_shell("completed").unwrap();
    assert_eq!(snapshot.shell_state.as_deref(), Some("busy"));
    assert_eq!(snapshot.agent_state.as_deref(), Some("working"));
    assert!(snapshot.suggested_actions.is_none());
    assert!(
        !state
            .session_maps
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
fn idle_parent_notification_finishes_before_new_turn_reservation() {
    use std::sync::atomic::Ordering;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let child_id = "idle-race-child";
    let parent_id = "idle-race-parent";
    agent_session(&state, child_id, SHELL_BUSY);
    state
        .session_maps
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
            .session_maps
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
            .session_maps
            .shell_states
            .get(child_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
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
            &state
                .session_maps
                .silence_states
                .get(session_id)
                .unwrap()
                .clone(),
            session_id,
            activity,
            Some("claude"),
            evidence_turn_epoch,
        );

        assert!(!transition.transitioned);
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
fn submitted_input_lifecycle_ready_before_status_line_is_not_stale_completed() {
    let state = crate::state::tests_support::make_test_app_state();
    completed_agent_session(&state, "quick-turn");
    state.pending_injections.insert(
        "quick-turn".to_string(),
        std::collections::VecDeque::from([crate::state::PendingInjection::notice(
            "quick follow up",
        )]),
    );
    flush_one_pending_as_submitted(&state, "quick-turn");

    state
        .session_maps
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
            .session_maps
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
        .session_maps
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
            .session_maps
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
    state.session_maps.input_buffers.insert(
        "submit-partial".to_string(),
        parking_lot::Mutex::new(buffer),
    );

    assert_eq!(
        write_agent_submission_to_pty(&state, "submit-partial", "new command"),
        AgentSubmissionWrite::Rejected {
            reason: "partial_composer",
            composer_state: "partial",
            pending: Vec::new(),
        }
    );
    assert!(bytes.lock().unwrap().is_empty());
    assert_eq!(
        state
            .session_maps
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
        .push_back(crate::state::PendingInjection::notice("older notice"));

    // FIFO still holds: the submission does not jump the queue.
    let rejection = write_agent_submission_to_pty(&state, "submit-queued", "new command");
    assert!(
        matches!(&rejection, AgentSubmissionWrite::Rejected { .. }),
        "{rejection:?}"
    );
    let written = String::from_utf8_lossy(&bytes.lock().unwrap().clone()).to_string();
    assert!(
        !written.contains("new command"),
        "the submission must not overtake the parked entry: {written:?}"
    );
    // ...but the queue drains instead of standing still. A ready agent that is
    // already idle never sees another BUSY→IDLE edge, so the only thing that
    // could move this queue is the submit itself.
    assert!(
        written.contains("older notice"),
        "the parked entry must be typed, not left to block the composer forever: {written:?}"
    );
    assert!(
        state
            .pending_injections
            .get("submit-queued")
            .is_none_or(|queue| queue.is_empty()),
        "the queue must be empty once its entry reached the composer"
    );
}

/// The regression: `queued_commands_pending` was reported for a session that
/// could never drain, so the caller retried submit for minutes against a queue
/// that by construction could not move — and the reason named the symptom
/// rather than the cause.
#[cfg(unix)]
#[test]
fn a_queue_that_cannot_drain_reports_the_agent_not_the_queue() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "submit-unready", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "submit-unready");
    // Unconfirmed idle: exactly what a ready-screen agent looks like before its
    // adapter has proof. `flush_pending_injections` is gated on the same
    // predicate, so this queue cannot move until that changes.
    state.session_maps.silence_states.insert(
        "submit-unready".to_string(),
        Arc::new(Mutex::new(SilenceState::new())),
    );
    state
        .pending_injections
        .entry("submit-unready".to_string())
        .or_default()
        .push_back(crate::state::PendingInjection::notice(PEER_MAIL_WAKE));

    let rejection = write_agent_submission_to_pty(&state, "submit-unready", "new command");
    assert_eq!(
        rejection,
        AgentSubmissionWrite::Rejected {
            reason: "agent_not_ready",
            composer_state: "empty",
            pending: vec![crate::pty::PendingInjectionSummary {
                id: state.pending_injections.get("submit-unready").unwrap()[0].id(),
                kind: "notice",
                preview: PEER_MAIL_WAKE.to_string(),
            }],
        },
        "the blocker must name itself: which entry, of what kind"
    );
    assert!(bytes.lock().unwrap().is_empty());
}

/// Whatever is parked is listable, countable and deletable — of every kind.
/// A server entry that no surface reported is what made the stuck queue
/// undiagnosable: an empty composer, an empty Compose list, and submit
/// rejected anyway.
#[test]
fn every_parked_entry_is_observable_and_drainable() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "queue-visible", SHELL_BUSY);
    {
        let mut queue = state
            .pending_injections
            .entry("queue-visible".to_string())
            .or_default();
        queue.push_back(crate::state::PendingInjection::notice(PEER_MAIL_WAKE));
        queue.push_back(crate::state::PendingInjection::initial_prompt(
            "do the task",
        ));
        queue.push_back(crate::state::PendingInjection::user_command("git status"));
    }

    let listed = list_queued_commands(&state, "queue-visible");
    assert_eq!(
        listed.iter().map(|entry| entry.kind).collect::<Vec<_>>(),
        vec!["notice", "initial_prompt", "user_command"]
    );
    assert_eq!(queued_command_count(&state, "queue-visible"), 3);

    // Every kind deletes by id, not just the user's own.
    assert!(remove_queued_command(&state, "queue-visible", listed[0].id));
    assert!(remove_queued_command(&state, "queue-visible", listed[1].id));
    assert_eq!(queued_command_count(&state, "queue-visible"), 1);
    assert_eq!(clear_queued_commands(&state, "queue-visible"), 1);
    assert_eq!(queued_command_count(&state, "queue-visible"), 0);
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
    let peer = deliver_notice_to_pty(&state, "submit-race", "peer command");
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
    queue.push_back(crate::state::PendingInjection::notice("wake up"));
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
fn should_inject_now_false_for_shell_and_confident_question() {
    use std::sync::atomic::AtomicU8;
    let state = crate::state::tests_support::make_test_app_state();
    // Plain shell (no agent_type) — must never be injected into.
    state
        .session_maps
        .shell_states
        .insert("shell".to_string(), AtomicU8::new(SHELL_IDLE));
    state
        .session_maps
        .session_states
        .insert("shell".to_string(), crate::state::SessionState::default());
    assert!(!should_inject_now(&state, "shell"), "shell → never inject");

    // Agent idle but blocked on a CONFIDENT user-facing question (Ink menu,
    // cliclack prompt, "Action Required" title) — never answer it.
    state
        .session_maps
        .shell_states
        .insert("q".to_string(), AtomicU8::new(SHELL_IDLE));
    state.session_maps.session_states.insert(
        "q".to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            awaiting_input: true,
            question_confident: true,
            ..Default::default()
        },
    );
    let mut ready_silence = SilenceState::new();
    ready_silence.confirm_idle();
    state
        .session_maps
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
        .session_maps
        .shell_states
        .insert("ready".to_string(), AtomicU8::new(SHELL_IDLE));
    state.session_maps.session_states.insert(
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
    let outcome = deliver_notice_to_pty(&state, "busy", "[TUIC message from lead] go");
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
        TtyMode::Raw,
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
    assert!(queue.iter().all(|entry| entry.kind() == "user_command"));
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
        .session_maps
        .shell_states
        .insert("shell".to_string(), AtomicU8::new(SHELL_IDLE));
    state
        .session_maps
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
        crate::state::PendingInjection::notice("[TUIC message from lead] first peer"),
    );
    enqueue_user_command(&state, "busy", "two").expect("enqueued");
    state.pending_injections.get_mut("busy").unwrap().push_back(
        crate::state::PendingInjection::notice("[TUIC message from worker] second peer"),
    );

    // Clear empties the queue, server notices included. Leaving them behind is
    // what let "Clear" empty the visible list while the composer stayed blocked
    // on what was left — and a dropped wake costs nothing: the mail it points at
    // never left the inbox.
    assert_eq!(queued_command_count(&state, "busy"), 4);
    assert_eq!(clear_queued_commands(&state, "busy"), 4);
    assert_eq!(queued_command_count(&state, "busy"), 0);
    assert!(
        state
            .pending_injections
            .get("busy")
            .is_none_or(|queue| queue.is_empty())
    );
    assert_eq!(
        clear_queued_commands(&state, "busy"),
        0,
        "clearing an empty queue is a no-op, not an error"
    );
}

/// A recipient that cannot be woken must not accumulate one parked wake per
/// sender: the pointer covers the whole inbox, and every extra copy is another
/// idle window in which `submit` is rejected `queued_commands_pending`.
#[cfg(unix)]
#[test]
fn repeated_mail_to_a_busy_recipient_parks_a_single_wake() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "busy-mail", SHELL_BUSY);
    insert_recording_session(&state, "busy-mail");

    for _ in 0..3 {
        assert_eq!(
            deliver_notice_to_pty(&state, "busy-mail", PEER_MAIL_WAKE),
            PtyDelivery::Queued
        );
    }

    assert_eq!(
        state
            .pending_injections
            .get("busy-mail")
            .map(|queue| queue.len()),
        Some(1),
        "one pointer covers the whole inbox"
    );
}

/// The Compose panel lists what waits and deletes one entry — of every kind,
/// in delivery order. Server entries used to be invisible and untouchable here,
/// which is precisely why a single parked one could block `submit` with nothing
/// on screen to explain it.
#[cfg(unix)]
#[test]
fn list_and_remove_expose_every_parked_entry() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "busy", SHELL_BUSY);
    insert_recording_session(&state, "busy");
    enqueue_user_command(&state, "busy", "one").expect("enqueued");
    state
        .pending_injections
        .get_mut("busy")
        .unwrap()
        .push_back(crate::state::PendingInjection::notice(PEER_MAIL_WAKE));
    enqueue_user_command(&state, "busy", "two").expect("enqueued");

    let listed = list_queued_commands(&state, "busy");
    assert_eq!(
        listed.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
        vec!["one", PEER_MAIL_WAKE, "two"],
        "listed in delivery order, nothing hidden"
    );
    assert_eq!(
        listed.iter().map(|c| c.kind).collect::<Vec<_>>(),
        vec!["user_command", "notice", "user_command"],
        "kind is what tells a server entry from the user's own"
    );

    assert!(remove_queued_command(&state, "busy", listed[1].id));
    assert_eq!(
        list_queued_commands(&state, "busy")
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>(),
        vec!["one", "two"],
        "the blocking entry is deletable, and the rest keep their order"
    );
    assert!(
        !remove_queued_command(&state, "busy", listed[1].id),
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
        .push_back(crate::state::PendingInjection::notice("first"));

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

/// The defect this pair pins: `deliver_notice_to_managed_pty` used to return
/// `state.session_maps.sessions.contains_key(session_id)` — "the session exists", not "the
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

    let outcome = deliver_notice_to_pty(&state, "busy-peer", "[TUIC message from lead] go");
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
    let outcome = deliver_notice_to_managed_pty(&state, "ghost-peer", "[TUIC] hi");
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
            crate::state::PendingInjection::notice("first"),
            crate::state::PendingInjection::notice("second"),
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
        .session_maps
        .input_buffers
        .insert("typing".to_string(), Mutex::new(input));

    assert!(
        !should_inject_now(&state, "typing"),
        "partial composer input must block terminal injection"
    );
    deliver_notice_to_pty(&state, "typing", "[TUIC message from child] done");
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
    deliver_notice_to_pty(&state, "ghost", "hi");
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
        deliver_notice_to_managed_pty(&state, "vanished", "message"),
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
    deliver_notice_to_pty(&state, "idle", "now");
    assert!(
        state
            .session_maps
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
        .session_maps
        .silence_states
        .get("active")
        .unwrap()
        .lock()
        .note_real_activity();

    assert!(!rollback_injection_claim(&state, "active", claim));
    assert!(
        state
            .session_maps
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
/// Simulates a stalled/backed-up PTY write (e.g. a SIGSTOP'd standby
/// child) — blocks on the first `write()` until the test sends on
/// `release_rx`, then records the bytes like `RecordingWriter`.
#[cfg(unix)]
struct BlockingWriter {
    bytes: Arc<std::sync::Mutex<Vec<u8>>>,
    release_rx: std::sync::mpsc::Receiver<()>,
}

#[cfg(unix)]
impl std::io::Write for BlockingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let _ = self.release_rx.recv();
        self.bytes.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// `leaf` (and other pagers) query cursor position with a DSR/CPR request
/// (`ESC[6n`) on their first paint and give up waiting for the reply after
/// a short internal deadline, printing it as raw text if it's late.
/// `process_chunk` must flush that reply to the child's stdin itself,
/// not merely produce it internally — regression coverage for the
/// early-flush ordering fixed alongside this test.
#[cfg(unix)]
#[test]
fn dsr_cursor_position_query_is_flushed_by_process_chunk() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "dsr-cpr-query";
    let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        &state,
        sid,
        Box::new(RecordingWriter {
            bytes: Arc::clone(&bytes),
        }),
        TtyMode::Cbreak,
    );

    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));

    let mut cp = ChunkProcessor::new(None, None);
    // Cursor starts at (0,0) on a fresh grid, so the reply is 1-indexed row 1, col 1.
    let _ = cp.process_chunk("\x1b[6n", &silence, sid, &state);

    assert_eq!(
        *bytes.lock().unwrap(),
        b"\x1b[1;1R",
        "the CPR reply must reach the session's real PTY writer"
    );
}

/// Shared setup for the DSR/CPR edge-case tests below: a real PTY session
/// backed by a `RecordingWriter`, plus the per-session state `process_chunk`
/// requires.
#[cfg(unix)]
fn setup_dsr_test_session(
    sid: &str,
) -> (
    crate::state::AppState,
    Arc<std::sync::Mutex<Vec<u8>>>,
    Arc<Mutex<SilenceState>>,
) {
    let state = crate::state::tests_support::make_test_app_state();
    let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        &state,
        sid,
        Box::new(RecordingWriter {
            bytes: Arc::clone(&bytes),
        }),
        TtyMode::Cbreak,
    );
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));
    (state, bytes, silence)
}

/// `EscapeAwareBuffer` (the layer above `process_chunk`, in front of every
/// real PTY read) must hold back an incomplete CSI sequence rather than
/// let it reach `process_chunk` split in two — `n` (0x6E) is a valid CSI
/// final byte, so `ESC[6` with no final byte yet must not be mistaken for
/// a complete, unrelated sequence and must not produce a premature (or
/// duplicate) reply once the final byte arrives in the next PTY read.
#[cfg(unix)]
#[test]
fn dsr_query_split_across_two_pty_reads_is_not_dispatched_early() {
    let sid = "dsr-split-read";
    let (state, bytes, silence) = setup_dsr_test_session(sid);
    let mut cp = ChunkProcessor::new(None, None);
    let mut utf8_buf = Utf8ReadBuffer::new();
    let mut esc_buf = EscapeAwareBuffer::new();

    let utf8_data = utf8_buf.push(b"\x1b[6");
    let esc_data = esc_buf.push(&utf8_data);
    let _ = cp.process_chunk(&esc_data, &silence, sid, &state);
    assert!(
        bytes.lock().unwrap().is_empty(),
        "an incomplete CSI sequence must not be dispatched or answered early"
    );

    let utf8_data2 = utf8_buf.push(b"n");
    let esc_data2 = esc_buf.push(&utf8_data2);
    let _ = cp.process_chunk(&esc_data2, &silence, sid, &state);
    assert_eq!(
        *bytes.lock().unwrap(),
        b"\x1b[1;1R",
        "the reply must land once the split sequence completes"
    );
}

/// Two CPR queries in the same chunk, with a cursor-moving byte between
/// them, must produce two distinct replies in order — each reflecting the
/// cursor position AT THE TIME its own query was dispatched, not the
/// position after the rest of the chunk (including the second query) has
/// been processed. `vt.process()` runs the whole chunk synchronously
/// before any reply is read back out, so this also guards against a
/// future refactor that re-reads cursor state lazily instead of at
/// dispatch time.
#[cfg(unix)]
#[test]
fn multiple_dsr_queries_in_one_chunk_report_distinct_positions_in_order() {
    let sid = "dsr-multi-query";
    let (state, bytes, silence) = setup_dsr_test_session(sid);
    let mut cp = ChunkProcessor::new(None, None);

    let _ = cp.process_chunk("\x1b[6nx\x1b[6n", &silence, sid, &state);
    assert_eq!(
        *bytes.lock().unwrap(),
        b"\x1b[1;1R\x1b[1;2R".to_vec(),
        "each reply must reflect the cursor position at its own query, in order"
    );
}

/// `device_status` only answers arg 5 and 6; the DEC-private `ESC[?6n`
/// variant isn't even routed to it by the vte dispatch table (it requires
/// empty intermediates). Both must be silently ignored — no reply, no
/// panic — confirming a future change to that match can't start emitting
/// a bogus reply for an argument nothing actually asked for.
#[cfg(unix)]
#[test]
fn unsupported_dsr_variants_produce_no_reply_and_do_not_panic() {
    let sid = "dsr-unsupported";
    let (state, bytes, silence) = setup_dsr_test_session(sid);
    let mut cp = ChunkProcessor::new(None, None);

    let _ = cp.process_chunk("\x1b[99n\x1b[?6n", &silence, sid, &state);
    assert!(
        bytes.lock().unwrap().is_empty(),
        "unsupported DSR queries must not produce a reply"
    );
}

/// `process_chunk` must not hold the session's `vt_log` lock while
/// flushing a `PtyWrite` reply — `write_terminal_reply`'s `write_all`/
/// `flush` can block (a SIGSTOP'd/standby child, or a full PTY input
/// queue), and blocking there while still holding the lock would stall
/// every other consumer of this session's grid (the frame ticker,
/// HTTP terminal reads) for as long as the write is stuck. Regression
/// coverage for the lock-scope fix alongside the early-flush ordering
/// fix: hangs a write mid-`process_chunk` and asserts the vt_log lock
/// is still acquirable (non-blocking) while it's stuck.
#[cfg(unix)]
#[test]
fn process_chunk_does_not_hold_the_vt_log_lock_while_flushing_a_reply() {
    let sid = "dsr-lock-scope";
    let state = crate::state::tests_support::make_test_app_state();
    let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    insert_session_with_writer(
        &state,
        sid,
        Box::new(BlockingWriter {
            bytes: Arc::clone(&bytes),
            release_rx,
        }),
        TtyMode::Cbreak,
    );
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));

    let state = Arc::new(state);
    let worker_state = Arc::clone(&state);
    let worker = std::thread::spawn(move || {
        let mut cp = ChunkProcessor::new(None, None);
        let _ = cp.process_chunk("\x1b[6n", &silence, sid, &worker_state);
    });

    // Give the worker time to reach the blocked write.
    std::thread::sleep(std::time::Duration::from_millis(120));
    assert!(
        bytes.lock().unwrap().is_empty(),
        "test setup: the write should still be blocked at this point"
    );

    let vt_log = state.grid.vt_log_buffers.get(sid).expect("vt_log inserted");
    assert!(
        vt_log.try_lock().is_some(),
        "vt_log lock must be free while a PtyWrite reply is stuck in write()"
    );

    release_tx.send(()).unwrap();
    worker.join().unwrap();
    assert_eq!(*bytes.lock().unwrap(), b"\x1b[1;1R");
}

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
/// The three line-discipline states `write_terminal_reply` must tell apart.
/// `Cbreak` is not a curiosity: it is the one where `ICANON` and `ECHO`
/// disagree, so it is the only case that can prove the gate keys on the right
/// flag. A fresh `openpty` is `Cooked`.
#[cfg(unix)]
#[derive(Clone, Copy)]
enum TtyMode {
    /// `ICANON` + `ECHO` — a reply is painted on screen and never delivered.
    Cooked,
    /// `ICANON` off, `ECHO` on — ugly, but the reply IS read immediately.
    Cbreak,
    /// Both off — what an agent sets before it queries.
    Raw,
}

#[cfg(unix)]
fn insert_session_with_writer(
    state: &AppState,
    session_id: &str,
    writer: Box<dyn std::io::Write + Send>,
    mode: TtyMode,
) {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    if !matches!(mode, TtyMode::Cooked) {
        let fd = pair.master.as_raw_fd().expect("master fd");
        let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
        assert_eq!(unsafe { libc::tcgetattr(fd, termios.as_mut_ptr()) }, 0);
        let mut termios = unsafe { termios.assume_init() };
        termios.c_lflag &= !libc::ICANON;
        if matches!(mode, TtyMode::Raw) {
            termios.c_lflag &= !libc::ECHO;
        }
        assert_eq!(
            unsafe { libc::tcsetattr(fd, libc::TCSANOW, &termios) },
            0,
            "setting the line discipline must succeed"
        );
    }
    let mut command = CommandBuilder::new("/bin/sh");
    command.args(["-c", "sleep 30"]);
    let child = pair.slave.spawn_command(command).expect("spawn shell");
    state.session_maps.sessions.insert(
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
        TtyMode::Raw,
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
        TtyMode::Raw,
    );

    write_terminal_reply(&state, "reply-order", b"first", "test");
    write_terminal_reply(&state, "reply-order", b"second", "test");
    assert_eq!(*bytes.lock().unwrap(), b"firstsecond");
}

/// Claude Code emits `ESC[c` before it leaves cooked mode. Answering it there
/// does not reach Claude — a canonical read blocks for a newline the reply
/// never contains, and `ECHO` paints `ESC[?6c` as the literal `^[[?6c`, which
/// is the garbage Boss saw above the startup banner (capture `f2bddfb0`,
/// 2026-09-07). The querier re-asks from raw mode, so nothing is lost.
#[cfg(unix)]
#[test]
fn terminal_reply_is_withheld_while_the_tty_is_canonical() {
    let state = crate::state::tests_support::make_test_app_state();
    let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        &state,
        "echoing-tty",
        Box::new(RecordingWriter {
            bytes: Arc::clone(&bytes),
        }),
        TtyMode::Cooked,
    );

    write_terminal_reply(&state, "echoing-tty", b"\x1b[?6c", "DA1");
    assert!(
        bytes.lock().unwrap().is_empty(),
        "a reply the querier cannot read must not be painted on its screen"
    );
}

/// The case that decides which flag the gate reads. In cbreak the tty still
/// echoes, so keying on `ECHO` would withhold here — but `ICANON` is off, so
/// the querier reads the reply immediately and nothing will ever resend it.
/// Withholding would trade Boss's cosmetic `^[[?6c` for a hung agent.
#[cfg(unix)]
#[test]
fn terminal_reply_is_delivered_in_cbreak_even_though_the_tty_echoes() {
    let state = crate::state::tests_support::make_test_app_state();
    let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        &state,
        "cbreak-tty",
        Box::new(RecordingWriter {
            bytes: Arc::clone(&bytes),
        }),
        TtyMode::Cbreak,
    );

    write_terminal_reply(&state, "cbreak-tty", b"\x1b[?6c", "DA1");
    assert_eq!(
        *bytes.lock().unwrap(),
        b"\x1b[?6c",
        "cbreak delivers the reply; withholding it would hang the querier"
    );
}

#[cfg(unix)]
#[test]
fn shared_pty_write_reports_writer_failure_and_teardown() {
    let state = crate::state::tests_support::make_test_app_state();
    insert_session_with_writer(
        &state,
        "failed-writer",
        Box::new(FailingWriter),
        TtyMode::Raw,
    );

    let error = state
        .write_pty_parts("failed-writer", &[b"reply"])
        .expect_err("writer error must be observable");
    assert!(error.contains("injected PTY failure"), "{error}");

    state.session_maps.sessions.remove("failed-writer");
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
            .session_maps
            .shell_states
            .get("uncertain")
            .is_some_and(|value| value.load(Ordering::Acquire) == SHELL_BUSY)
    );
    assert!(
        state
            .session_maps
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
    assert_eq!(
        resize_session_core(&state, "s", 24, 80, None, None),
        Ok(None)
    );
}

#[test]
fn resize_seeds_applied_from_grid_and_noops_at_startup_dims() {
    let state = crate::state::tests_support::make_test_app_state();
    // Grid exists at the startup dims but resize_locks is empty → the lock
    // opens at the (0,0) never-applied sentinel.
    seed_vt_grid(&state, "s", 24, 80);
    // A first resize matching only the startup dims must seed *applied from
    // the live grid and then no-op — no gratuitous SIGWINCH, no session touch.
    assert_eq!(
        resize_session_core(&state, "s", 24, 80, None, None),
        Ok(None)
    );
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
    q.push_back(crate::state::PendingInjection::notice("later"));
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
        .session_maps
        .shell_states
        .insert("codex".to_string(), AtomicU8::new(SHELL_IDLE));
    state.session_maps.session_states.insert(
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
        .session_maps
        .silence_states
        .insert("codex".to_string(), Arc::new(Mutex::new(silence)));
    deliver_notice_to_pty(&state, "codex", "[TUIC message from lead] go");
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

#[test]
fn mark_session_exited_sends_single_exited_notification() {
    // F1/DATA-1: only one state_change("exited") must reach parent inbox on exit.
    // The BUSY→IDLE transition in the exit path uses notify_parent=false, so the
    // orchestrator must never see a spurious "idle" before "exited".
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let child_id = "child-exit-dedup";
    let parent_id = "parent-exit-dedup";

    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    let ss = crate::state::SessionState {
        agent_type: Some("claude".to_string()),
        ..Default::default()
    };
    state
        .session_maps
        .session_states
        .insert(child_id.to_string(), ss);
    state.session_maps.shell_states.insert(
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
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));

    let proc = ChunkProcessor::new(None, None);
    let _ = proc.handle_tuic_state("busy", session_id, 0, false, &state);

    let current = state
        .session_maps
        .shell_states
        .get(session_id)
        .unwrap()
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(current, SHELL_BUSY);

    let _ = proc.handle_tuic_state("idle", session_id, 0, false, &state);
    let current = state
        .session_maps
        .shell_states
        .get(session_id)
        .unwrap()
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(current, SHELL_IDLE);
}

fn setup_idle_session(session_id: &str) -> crate::state::AppState {
    let state = crate::state::tests_support::make_test_app_state();
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));
    state
}

#[test]
fn short_followup_prompt_clears_stale_last_prompts_entry() {
    // record_submitted_line only inserts into last_prompts for
    // submissions of 10+ words. Without an explicit clear on a short
    // submission, a short follow-up ("fix it") would inherit the
    // previous turn's prompt text for both get_last_prompt and
    // AgentBlock.prompt_text — showing the WRONG prompt, not simply a
    // missing one.
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-short-followup";
    let state = Arc::new(state);
    record_submitted_line(
        &state,
        session_id,
        "please refactor the parser to handle nested brackets correctly now".to_string(),
        -1,
    );
    assert_eq!(
        last_prompt_text(&state, session_id),
        Some("please refactor the parser to handle nested brackets correctly now".to_string())
    );

    record_submitted_line(&state, session_id, "fix it".to_string(), -1);
    assert_eq!(
        last_prompt_text(&state, session_id),
        None,
        "a short follow-up must not inherit the previous turn's stale prompt text"
    );
}

#[test]
fn handle_tuic_state_busy_edge_emits_agent_block_start_with_prompt_text() {
    let session_id = "test-block-start";
    let state = setup_idle_session(session_id);
    state.session_maps.last_prompts.insert(
        session_id.to_string(),
        "please refactor the parser".to_string(),
    );

    let proc = ChunkProcessor::new(None, None);
    let (transitioned, block_event) = proc.handle_tuic_state("busy", session_id, 42, false, &state);
    assert!(transitioned, "idle->busy must be a real transition");
    match block_event {
        Some(ParsedEvent::AgentBlock {
            action,
            line,
            exit_code,
            prompt_text,
            ..
        }) => {
            assert_eq!(action, "start");
            assert_eq!(line, 42);
            assert_eq!(exit_code, None);
            assert_eq!(prompt_text, Some("please refactor the parser".to_string()));
        }
        other => panic!("expected AgentBlock start, got {other:?}"),
    }
}

#[test]
fn handle_tuic_state_idle_edge_emits_agent_block_end_with_no_flag() {
    let session_id = "test-block-end";
    let state = setup_idle_session(session_id);
    let proc = ChunkProcessor::new(None, None);
    let _ = proc.handle_tuic_state("busy", session_id, 10, false, &state);

    let (transitioned, block_event) = proc.handle_tuic_state("idle", session_id, 55, false, &state);
    assert!(transitioned, "busy->idle must be a real transition");
    match block_event {
        Some(ParsedEvent::AgentBlock {
            action,
            line,
            exit_code,
            ..
        }) => {
            assert_eq!(action, "end");
            assert_eq!(line, 55);
            assert_eq!(
                exit_code, None,
                "no turn_error_flags entry means no red tick"
            );
        }
        other => panic!("expected AgentBlock end, got {other:?}"),
    }
}

#[test]
fn handle_tuic_state_idle_edge_reads_and_clears_turn_error_flags() {
    let session_id = "test-block-end-flagged";
    let state = setup_idle_session(session_id);
    let proc = ChunkProcessor::new(None, None);
    let _ = proc.handle_tuic_state("busy", session_id, 10, false, &state);
    state
        .session_maps
        .turn_error_flags
        .insert(session_id.to_string(), ());

    let (_, block_event) = proc.handle_tuic_state("idle", session_id, 55, false, &state);
    match block_event {
        Some(ParsedEvent::AgentBlock { exit_code, .. }) => {
            assert_eq!(
                exit_code,
                Some(1),
                "flagged turn must produce a non-zero exit code"
            );
        }
        other => panic!("expected AgentBlock end, got {other:?}"),
    }
    assert!(
        state
            .session_maps
            .turn_error_flags
            .get(session_id)
            .is_none(),
        "the flag must be cleared after being read, so it doesn't leak into the next turn"
    );

    // Next turn, with no new flag set, must not be flagged.
    let _ = proc.handle_tuic_state("busy", session_id, 60, false, &state);
    let (_, next_end) = proc.handle_tuic_state("idle", session_id, 65, false, &state);
    match next_end {
        Some(ParsedEvent::AgentBlock { exit_code, .. }) => {
            assert_eq!(
                exit_code, None,
                "the cleared flag must not leak into the next turn"
            );
        }
        other => panic!("expected AgentBlock end, got {other:?}"),
    }
}

#[test]
fn handle_tuic_state_busy_edge_clears_a_flag_that_arrived_too_late_for_the_previous_turn() {
    // The ToolError/ApiError fallback tier is gated by a 5s silence
    // threshold that, for a hook-instrumented session, typically fires
    // well after Stop's idle transition already read-and-cleared
    // turn_error_flags (finding it empty). Simulate that: the flag is
    // set *after* the previous turn's idle edge already ran.
    let session_id = "test-stale-flag-race";
    let state = setup_idle_session(session_id);
    let proc = ChunkProcessor::new(None, None);
    let _ = proc.handle_tuic_state("busy", session_id, 10, false, &state);
    let (_, end_event) = proc.handle_tuic_state("idle", session_id, 20, false, &state);
    match end_event {
        Some(ParsedEvent::AgentBlock { exit_code, .. }) => {
            assert_eq!(
                exit_code, None,
                "no flag was set yet, so turn 1 closes clean"
            )
        }
        other => panic!("expected AgentBlock end, got {other:?}"),
    }

    // The delayed silence-timer detection for turn 1's error finally
    // fires, well after turn 1 already closed.
    state
        .session_maps
        .turn_error_flags
        .insert(session_id.to_string(), ());

    // Turn 2 starts — its busy edge must discard the stale flag rather
    // than letting it attach to turn 2's own idle transition.
    let _ = proc.handle_tuic_state("busy", session_id, 30, false, &state);
    let (_, turn2_end) = proc.handle_tuic_state("idle", session_id, 40, false, &state);
    match turn2_end {
        Some(ParsedEvent::AgentBlock { exit_code, .. }) => {
            assert_eq!(
                exit_code, None,
                "a flag that arrived after turn 1 already closed must not bleed into turn 2"
            );
        }
        other => panic!("expected AgentBlock end, got {other:?}"),
    }
}

#[test]
fn handle_tuic_state_same_state_reaffirmation_emits_neither() {
    let session_id = "test-block-noop";
    let state = setup_idle_session(session_id);
    let proc = ChunkProcessor::new(None, None);
    let (first, _) = proc.handle_tuic_state("busy", session_id, 10, false, &state);
    assert!(first);

    // Simulates a redundant busy re-affirmation (e.g. the surviving
    // PostToolUse(AskUserQuestion|ExitPlanMode) entry): same target,
    // already busy — must be a no-op, not a second block start.
    let (second, block_event) = proc.handle_tuic_state("busy", session_id, 20, false, &state);
    assert!(
        !second,
        "same-state re-affirmation must not be a real transition"
    );
    assert!(block_event.is_none(), "must not emit a second block start");
}

#[test]
fn tuic_osc_state_emits_shell_state_event() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-tuic-emit";
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));

    let mut rx = state.event_bus.subscribe();

    let proc = ChunkProcessor::new(None, None);
    let _ = proc.handle_tuic_state("busy", session_id, 0, false, &state);

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
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );

    let proc = ChunkProcessor::new(None, None);
    let _ = proc.handle_tuic_state("thinking", session_id, 0, false, &state);

    let current = state
        .session_maps
        .shell_states
        .get(session_id)
        .unwrap()
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(
        current, SHELL_IDLE,
        "unknown state should not change shell_states"
    );
}

fn heuristic_synthesizes_block_when_no_tuic_state_integration() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-heuristic-active";
    agent_session(&state, session_id, SHELL_BUSY);
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

    processor.process_chunk("⏺ Bash(ls)\r\n", &silence, session_id, &state);
    assert!(
        processor.last_agent_block_line.is_some(),
        "the ⏺ heuristic must synthesize a block when no OSC 7770 state event has arrived"
    );
}

#[test]
fn heuristic_suppressed_once_tuic_state_integration_observed() {
    // The primary turn-level source (idle<->busy edge) is authoritative
    // once a session has ever received an OSC 7770 state event — the
    // heuristic must not also run and produce a second, conflicting
    // block source.
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-heuristic-suppressed";
    agent_session(&state, session_id, SHELL_BUSY);
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    state
        .session_maps
        .has_tuic_state_integration
        .insert(session_id.to_string(), ());
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
    let mut processor = ChunkProcessor::new(None, None);

    processor.process_chunk("⏺ Bash(ls)\r\n", &silence, session_id, &state);
    assert!(
        processor.last_agent_block_line.is_none(),
        "the ⏺ heuristic must not fire once has_tuic_state_integration is set"
    );
}

#[test]
fn heuristic_block_open_before_suppression_is_closed_not_orphaned() {
    // Suppression can activate mid-turn: a `⏺` header opens a heuristic
    // block before the first OSC 7770 state event sets the flag (the
    // common path is UserPromptSubmit firing before any tool call, but
    // this isn't guaranteed). Without an explicit close, that block would
    // stay open forever — never folded, no exit code, no scrollbar tick.
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-heuristic-orphan";
    agent_session(&state, session_id, SHELL_BUSY);
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

    // Open a heuristic block before integration is detected.
    processor.process_chunk("⏺ Bash(ls)\r\n", &silence, session_id, &state);
    assert!(processor.last_agent_block_line.is_some());

    // Integration is now detected (simulating a hook event arriving late).
    state
        .session_maps
        .has_tuic_state_integration
        .insert(session_id.to_string(), ());

    let mut rx = state.event_bus.subscribe();
    processor.process_chunk("some more output\r\n", &silence, session_id, &state);

    assert!(
        processor.last_agent_block_line.is_none(),
        "the dangling block must be closed, not left open forever"
    );
    let mut saw_end = false;
    while let Ok(crate::state::AppEvent::PtyParsed { parsed, .. }) = rx.try_recv() {
        if parsed["type"] == "agent-block" && parsed["action"] == "end" {
            saw_end = true;
        }
    }
    assert!(
        saw_end,
        "must emit an AgentBlock end event for the orphaned block"
    );
}

#[test]
fn api_error_sets_turn_error_flag() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-api-error-flag";
    agent_session(&state, session_id, SHELL_BUSY);
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
    silence.lock().startup_settled = true;
    let mut processor = ChunkProcessor::new(None, None);

    let input = "API Error: 500 {\"type\":\"error\",\"error\":{\"type\":\"api_error\",\"message\":\"Internal server error\"},\"request_id\":\"req_1\"}\r\n";
    processor.process_chunk(input, &silence, session_id, &state);
    assert!(
        state
            .session_maps
            .turn_error_flags
            .get(session_id)
            .is_some(),
        "an ApiError event must flag the session's turn as failed"
    );
}

#[test]
fn tool_error_sets_turn_error_flag() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-tool-error-flag";
    let silence = Mutex::new(SilenceState::new());
    {
        let mut sl = silence.lock();
        sl.mark_tool_error_candidate("Error: Exit code 1".to_string());
        sl.last_output_at = std::time::Instant::now()
            - (SILENCE_TOOL_ERROR_THRESHOLD + std::time::Duration::from_secs(1));
    }
    fire_tool_error_if_ready(&silence, session_id, &state);
    assert!(
        state
            .session_maps
            .turn_error_flags
            .get(session_id)
            .is_some(),
        "a genuinely turn-ending tool error must flag the session's turn as failed"
    );
}

#[test]
fn recovered_tool_error_does_not_set_turn_error_flag() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-tool-error-recovered";
    let silence = Mutex::new(SilenceState::new());
    {
        let mut sl = silence.lock();
        sl.mark_tool_error_candidate("Error: Exit code 1".to_string());
        // The agent recovers before the silence threshold elapses.
        sl.clear_tool_error_on_recovery();
        sl.last_output_at = std::time::Instant::now()
            - (SILENCE_TOOL_ERROR_THRESHOLD + std::time::Duration::from_secs(1));
    }
    fire_tool_error_if_ready(&silence, session_id, &state);
    assert!(
        state
            .session_maps
            .turn_error_flags
            .get(session_id)
            .is_none(),
        "a recovered error must not flag the block"
    );
}

#[test]
fn tuic_osc_toolfail_sets_turn_error_flag() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-toolfail";
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

    assert!(
        state
            .session_maps
            .turn_error_flags
            .get(session_id)
            .is_none()
    );
    processor.process_chunk("\x1b]7770;toolfail=1\x07", &silence, session_id, &state);
    assert!(
        state
            .session_maps
            .turn_error_flags
            .get(session_id)
            .is_some(),
        "a toolfail OSC event must flag the session's turn as failed"
    );
}

#[test]
fn tuic_osc_toolfail_with_non_numeric_payload_still_sets_the_flag() {
    // The Rust dispatch never parses the payload — presence of the event
    // is the whole signal (see tuic-hook's `DERIVATIONS`/`toolfail_from_exit_code`
    // in crates/tuic-hook/src/main.rs) — so a malformed/non-numeric payload
    // must not be silently dropped.
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-toolfail-garbage";
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

    processor.process_chunk(
        "\x1b]7770;toolfail=not-a-number\x07",
        &silence,
        session_id,
        &state,
    );
    assert!(
        state
            .session_maps
            .turn_error_flags
            .get(session_id)
            .is_some()
    );
}

/// One test per new verb, table-driven: each must reach the event bus as
/// `ParsedEvent::AgentMetadata` with the right `field` name and a
/// correctly percent-decoded `value` — mirrors `tuic_osc_toolfail_sets_turn_error_flag`
/// above, but asserting on the emitted event rather than a state side table,
/// since `AgentMetadata` (unlike `toolfail`) has none.
#[test]
fn tuic_osc_metadata_verbs_decode_and_emit_agent_metadata() {
    let cases = [
        ("ccsession", "abc123", "session_id", "abc123"),
        ("cwd", "%2FUsers%2Fme%2Fproject", "cwd", "/Users/me/project"),
        (
            "transcript",
            "%2Ftmp%2Ft.jsonl",
            "transcript_path",
            "/tmp/t.jsonl",
        ),
        ("tool", "Bash", "tool_name", "Bash"),
        (
            "notify",
            "needs%20your%20input%3B%20now",
            "message",
            "needs your input; now",
        ),
    ];

    for (verb, wire_payload, expected_field, expected_value) in cases {
        let state = crate::state::tests_support::make_test_app_state();
        let session_id = format!("test-metadata-{verb}");
        agent_session(&state, &session_id, SHELL_IDLE);
        state.grid.vt_log_buffers.insert(
            session_id.clone(),
            Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
        );
        let silence = state
            .session_maps
            .silence_states
            .get(&session_id)
            .unwrap()
            .clone();
        let mut processor = ChunkProcessor::new(None, None);

        let mut rx = state.event_bus.subscribe();
        processor.process_chunk(
            &format!("\x1b]7770;{verb}={wire_payload}\x07"),
            &silence,
            &session_id,
            &state,
        );

        let event = rx
            .try_recv()
            .unwrap_or_else(|_| panic!("event bus must receive a PtyParsed for verb {verb}"));
        match event {
            crate::state::AppEvent::PtyParsed { parsed, .. } => {
                assert_eq!(
                    parsed.get("type").and_then(|v| v.as_str()),
                    Some("agent-metadata"),
                    "verb {verb}: wrong event type: {parsed}"
                );
                assert_eq!(
                    parsed.get("field").and_then(|v| v.as_str()),
                    Some(expected_field),
                    "verb {verb}: wrong field: {parsed}"
                );
                assert_eq!(
                    parsed.get("value").and_then(|v| v.as_str()),
                    Some(expected_value),
                    "verb {verb}: payload not decoded correctly: {parsed}"
                );
            }
            other => panic!("verb {verb}: unexpected event variant: {other:?}"),
        }
    }
}

#[test]
fn percent_decode_osc_payload_passes_through_plain_text() {
    assert_eq!(percent_decode_osc_payload("Bash"), "Bash");
}

#[test]
fn percent_decode_osc_payload_decodes_escapes() {
    assert_eq!(
        percent_decode_osc_payload("needs%20your%20input%3B%20now"),
        "needs your input; now"
    );
}

#[test]
fn percent_decode_osc_payload_tolerates_a_trailing_lone_percent() {
    // Must not panic on a truncated/malformed escape at the end of input —
    // an OSC payload is attacker-adjacent (an agent's own hook stdin), so
    // decoding must degrade gracefully, never panic.
    assert_eq!(percent_decode_osc_payload("abc%"), "abc%");
    assert_eq!(percent_decode_osc_payload("abc%2"), "abc%2");
}

#[test]
fn percent_decode_osc_payload_tolerates_invalid_hex() {
    assert_eq!(percent_decode_osc_payload("abc%ZZdef"), "abc%ZZdef");
}

#[test]
fn tuic_state_busy_yields_userinput_clear_with_prompt_line() {
    // The busy transition's absolute prompt row (history_size + cursor row,
    // here 42) must reach the UserInput event so the frontend can mark the
    // user-prompt line on the scrollbar. Only on a real transition.
    match tuic_state_awaiting_event("busy", 42, true, false, None, None, false) {
        Some(ParsedEvent::UserInput { content, line }) => {
            assert_eq!(content, "", "busy clear must not overwrite last_prompt");
            assert_eq!(line, 42, "busy UserInput must carry the prompt row");
        }
        other => panic!("expected UserInput clear, got {other:?}"),
    }
}

#[test]
fn tuic_state_busy_without_transition_yields_no_userinput() {
    // The green-tick pollution regression guard: a redundant busy
    // re-affirmation (e.g. from the surviving PostToolUse(AskUserQuestion|
    // ExitPlanMode) entry in claude_hook_map, or any other agent's own
    // per-tool busy hook) must not duplicate the scrollbar tick — as long as
    // there is nothing pending to clear (`currently_awaiting: false`).
    assert!(
        tuic_state_awaiting_event("busy", 42, false, false, None, None, false).is_none(),
        "a same-state re-affirmation with nothing awaiting must not emit UserInput"
    );
}

#[test]
fn tuic_state_idle_yields_no_awaiting_event() {
    assert!(
        tuic_state_awaiting_event("idle", 0, true, true, None, None, false).is_none(),
        "idle only transitions shell_state; it pushes no awaiting event \
         regardless of currently_awaiting"
    );
}

#[test]
fn tuic_state_unknown_yields_no_awaiting_event() {
    assert!(
        tuic_state_awaiting_event("thinking", 0, false, true, None, None, false).is_none(),
        "unknown verb must push no awaiting event regardless of currently_awaiting"
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
    std::fs::read(&path).unwrap_or_else(|primary_error| {
        let supplied = std::env::var_os("TUIC_CAPTURE_FIXTURE_DIR")
            .map(std::path::PathBuf::from)
            .map(|dir| dir.join(name));
        supplied
            .as_deref()
            .and_then(|fallback| std::fs::read(fallback).ok())
            .unwrap_or_else(|| panic!("missing fixture {}: {primary_error}", path.display()))
    })
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

/// The screen a capture leaves behind, rendered through the same VT the PTY hot
/// path uses. A screen-activity adapter is a function of the rendered grid, not
/// of the byte stream, so replaying to the grid is the only way a fixture can
/// prove one: goose repaints its footer in place with `\r\x1b[2K`, and asserting
/// on raw bytes would pass on output no terminal would ever display.
fn replay_final_screen(bytes: &[u8]) -> Vec<String> {
    let capture = crate::pty_capture::decode_capture(bytes).expect("valid capture");
    let (rows, cols) = capture.geometry.unwrap_or((41, 128));
    let mut vt_log = crate::state::VtLogBuffer::new(rows, cols, 2000);
    for record in capture.records {
        if record.direction == crate::pty_capture::CaptureDirection::Output {
            vt_log.process(&record.data);
        }
    }
    vt_log.screen_rows()
}

/// goose 1.49.0, captured live (#699-c6e0): the composer footer is on screen and
/// nothing is running, so the session must read Ready. Without this the OSC 133
/// busy bit set once by the long-lived `goose session` command survives for the
/// whole process and the tab never leaves "working".
#[test]
fn goose_idle_capture_reads_ready() {
    let screen = replay_final_screen(&agent_prompt_fixture("goose-1.49.0-idle.tcap"));
    assert_eq!(
        detect_agent_screen_activity(Some("goose"), &screen),
        AgentScreenActivity::Ready,
        "screen: {screen:#?}"
    );
}

/// The same session mid-turn. The spinner glyph cycles `◐◓◒` and the message
/// beside it is whimsical, so the assertion rests on the interrupt hint — the
/// one part of that row goose is not free to reword without changing what it
/// offers the user.
#[test]
fn goose_mid_turn_capture_reads_working() {
    let screen = replay_final_screen(&agent_prompt_fixture("goose-1.49.0-mid-turn.tcap"));
    assert_eq!(
        detect_agent_screen_activity(Some("goose"), &screen),
        AgentScreenActivity::Working,
        "screen: {screen:#?}"
    );
}

/// A goose screen whose footer has not been painted yet must read Unknown, not
/// Ready. Ready is the expensive direction to get wrong: it is what lets
/// auto-standby SIGSTOP a live turn.
#[test]
fn goose_screen_without_a_footer_is_unknown() {
    let rows: Vec<String> = [
        "  __( O)>  ● new session · ollama gemma4:12b-mlx",
        "   \\____)",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    assert_eq!(
        detect_agent_screen_activity(Some("goose"), &rows),
        AgentScreenActivity::Unknown
    );
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

    let capture = crate::pty_capture::decode_capture(bytes).expect("valid capture");
    let (rows, cols) = capture.geometry.unwrap_or((41, 128));
    let mut vt_log = VtLogBuffer::new(rows, cols, 2000);
    let mut parser = crate::output_parser::OutputParser::new();
    let mut input = crate::input_line_buffer::InputLineBuffer::new();
    let mut carry = String::new();
    let mut events = Vec::new();
    let mut stats = CaptureStats::default();
    for record in capture.records {
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

/// Replay a real Codex turn at its recorded 63x160 geometry and at the
/// capture's monotonic timestamps. A Ready classification is only known to be
/// false retrospectively, when a later frame restores Codex's Working row.
/// Protocol-ranked submission evidence must keep the turn BUSY throughout
/// that interval, regardless of the adapter's transient verdict.
fn assert_codex_false_ready_capture(name: &str, expected_variant: &str) {
    let bytes = agent_prompt_fixture(name);
    let capture = crate::pty_capture::decode_capture(&bytes).expect("valid capture");
    let geometry = capture.geometry.unwrap_or((63, 160));
    assert_eq!(geometry, (63, 160), "{name}: wrong capture geometry");
    let mut vt = crate::state::VtLogBuffer::new(geometry.0, geometry.1, 2000);
    let mut silence = SilenceState::new();
    silence.note_user_submission(true);
    let mut ready_candidate: Option<(u64, Vec<String>)> = None;
    let mut false_ready_screen = None;
    let mut saw_variant = false;

    for record in capture.records {
        if record.direction != crate::pty_capture::CaptureDirection::Output {
            continue;
        }
        vt.process(&record.data);
        let screen = vt.screen_rows();
        saw_variant |= screen.iter().any(|row| row.contains(expected_variant));
        match detect_agent_screen_activity(Some("codex"), &screen) {
            AgentScreenActivity::Working => {
                if let Some((_, candidate)) = ready_candidate.take() {
                    false_ready_screen = Some(candidate);
                }
                silence.note_working_screen();
            }
            AgentScreenActivity::Ready if false_ready_screen.is_none() => {
                let (first_ready_us, _) =
                    ready_candidate.get_or_insert_with(|| (record.elapsed_us, screen.clone()));
                let stable_for = record.elapsed_us.saturating_sub(*first_ready_us);
                silence.screen_ready_pending_since =
                    Some(std::time::Instant::now() - std::time::Duration::from_micros(stable_for));
                assert!(
                    !silence.note_ready_screen(),
                    "{name}: a Ready screen overrode Protocol busy at {record:?}"
                );
            }
            AgentScreenActivity::Ready | AgentScreenActivity::Unknown => {}
            AgentScreenActivity::Interrupted => {
                panic!("{name}: capture unexpectedly contains an interrupted turn")
            }
        }
        if false_ready_screen.is_none() {
            assert!(silence.explicit_busy(), "{name}: lost BUSY during replay");
        }
    }

    let false_ready_screen = false_ready_screen.unwrap_or_else(|| {
        panic!("{name}: no Ready classification was followed by resumed Working")
    });
    assert!(
        saw_variant,
        "{name}: expected variant {expected_variant:?}; false Ready rows: {false_ready_screen:#?}"
    );
}

/// Measure, for one capture, the longest UNBROKEN run of Ready frames — the
/// same window `screen_ready_pending_since` accumulates in production.
///
/// The geometry is a caller argument because TUICCAP1 carries none and both
/// false-ready fixtures predate TUICCAP2. Read it off the capture: the highest
/// CSI CUP row and the widest horizontal box rule.
fn longest_ready_run_us(bytes: &[u8], rows: u16, cols: u16) -> u64 {
    let capture = crate::pty_capture::decode_capture(bytes).expect("valid capture");
    let mut vt = crate::state::VtLogBuffer::new(rows, cols, 2000);
    let mut ready_since: Option<u64> = None;
    let mut longest = 0u64;

    for record in capture.records {
        if record.direction != crate::pty_capture::CaptureDirection::Output {
            continue;
        }
        vt.process(&record.data);
        match detect_agent_screen_activity(Some("codex"), &vt.screen_rows()) {
            AgentScreenActivity::Ready => {
                let first = *ready_since.get_or_insert(record.elapsed_us);
                longest = longest.max(record.elapsed_us.saturating_sub(first));
            }
            // Anything that is not Ready ends the window, because production
            // does exactly that: `note_unknown_screen` clears
            // `screen_ready_pending_since`, so an Unknown frame in the middle
            // restarts the AGENT_READY_CONFIRM countdown from zero. Measuring
            // Ready-to-Ready across an Unknown gap reports a stability the
            // production code never sees.
            AgentScreenActivity::Working
            | AgentScreenActivity::Unknown
            | AgentScreenActivity::Interrupted => ready_since = None,
        }
    }
    longest
}

/// The two false-ready fixtures only prove anything while their Ready runs
/// still outlast `AGENT_READY_CONFIRM`: that is what made the pre-fix code
/// declare idle mid-turn (measured 2026-09-14 at f9803b00^: 1.552 s and
/// 1.567 s of stable Ready were enough). If a later change to
/// `detect_codex_screen_activity` shortened those runs below the threshold,
/// `codex_0154_false_ready_real_captures_stay_protocol_busy` would keep
/// passing while proving nothing, which is the same "green by absence" trap as
/// a skipped test.
///
/// This is also the measurement the 2026-09-13 audit of this story got wrong.
/// It timed the FIRST Ready transient — 233 ms and 143 ms — and concluded from
/// it that no capture could ever produce the RED. The longest run is 7.6 s and
/// 7.5 s. Pin the number so nobody has to take it on trust again.
#[test]
fn the_false_ready_fixtures_still_hold_ready_long_enough_to_matter() {
    for name in [
        "codex-0.154-mid-turn-false-ready.tcap",
        "codex-0.154-background-terminal-false-ready.tcap",
    ] {
        let longest = longest_ready_run_us(&agent_prompt_fixture(name), 63, 160);
        assert!(
            std::time::Duration::from_micros(longest) >= AGENT_READY_CONFIRM,
            "{name}: longest Ready run is {:.3}s, under AGENT_READY_CONFIRM, \
             so this fixture can no longer reproduce the false idle",
            longest as f64 / 1e6
        );
    }
}

#[test]
fn codex_0154_false_ready_real_captures_stay_protocol_busy() {
    for (fixture, variant) in [
        (
            "codex-0.154-mid-turn-false-ready.tcap",
            "background terminal running",
        ),
        (
            "codex-0.154-background-terminal-false-ready.tcap",
            "Enter to select",
        ),
    ] {
        assert_codex_false_ready_capture(fixture, variant);
    }
}

/// Real Codex 0.154 `codex exec` PTY transcript captured by the #746 runtime
/// proof. The framed fixture preserves the source transcript byte-for-byte:
/// `codex-pty.typescript` SHA-256
/// f935c77442e3c7e1d29f336021cc5c882c9e855bc43cd87e1a141430713a2124.
/// Its notify payload SHA-256 was
/// 5895c3bb3f9223af1409a6ca2c941d7881bd5ce8d92c5031f6470eb0e48fdf14.
#[test]
fn codex_0154_runtime_hook_idle_reaches_the_pty_state_machine() {
    let bytes = agent_prompt_fixture("codex-0.154-runtime-hook-idle.tcap");
    let capture = crate::pty_capture::decode_capture(&bytes).expect("valid framed capture");
    assert_eq!(capture.geometry, None, "typescript did not record geometry");
    assert_eq!(
        capture.records.len(),
        1,
        "transcript is one observed PTY write"
    );
    assert_eq!(capture.records[0].data.len(), 53_289);
    assert!(
        capture.records[0]
            .data
            .ends_with(b"\x1b]7770;state=idle\x1b\\"),
        "runtime transcript lost its terminal OSC 7770 idle marker"
    );

    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "codex-0154-runtime-hook-idle";
    agent_session(&state, session_id, SHELL_BUSY);
    {
        let mut session = state
            .session_maps
            .session_states
            .get_mut(session_id)
            .unwrap();
        session.agent_type = Some("codex".into());
        session.hook_instrumented = true;
    }
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 2000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
    let mut processor = ChunkProcessor::new(None, None);
    for record in capture.records {
        let chunk = std::str::from_utf8(&record.data).expect("captured Codex PTY is UTF-8");
        processor.process_chunk(chunk, &silence, session_id, &state);
    }

    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_IDLE
    );
    let silence = silence.lock();
    assert!(silence.hook_state_seen);
    assert!(silence.explicit_idle());
}

/// #745-8ff1 AC6(c): a Codex `notify` turn-complete ends in a Protocol-rank
/// idle — and the two halves of that path actually meet.
///
/// Both halves were already covered, separately, and that was the gap.
/// `codex_0154_runtime_hook_idle_reaches_the_pty_state_machine` proves those
/// bytes close the turn, from a real capture. `generated_assets_have_protocol_and_ownership_markers`
/// proves the shipped script mentions the right strings. Neither proves the
/// sequence the script *prints* is the sequence the state machine *accepts*:
/// change the `printf` and both stay green while every Codex turn silently
/// stops closing. So this test takes the literal out of the shipped artifact,
/// decodes it the way `/bin/sh printf` would, and feeds the result to the real
/// `ChunkProcessor` — nothing here restates the escape sequence by hand.
///
/// Running the script is deliberately not how this is done. It resolves its own
/// tty from `$PPID` and writes there, so there is nothing to capture, and a
/// freshly written executable pays a code-signing scan on macOS (src-tauri/AGENTS.md).
#[test]
fn codex_notify_turn_complete_emits_the_idle_bytes_the_state_machine_accepts() {
    let dir = tempfile::TempDir::new().unwrap();
    crate::agent_hook_launch::regenerate_launch_assets(dir.path()).unwrap();
    let script = std::fs::read_to_string(dir.path().join("agent-hooks/codex-notify.sh"))
        .expect("codex notify script must be generated");

    // Exactly one arm may emit, and it must be the turn-complete one: a notify
    // for any other event must not close the turn.
    assert_eq!(
        script.matches("7770;state=idle").count(),
        1,
        "only the agent-turn-complete arm may emit idle"
    );
    let emit_at = script.find("7770;state=idle").unwrap();
    let case_at = script
        .find(r#""type":"agent-turn-complete""#)
        .expect("script must match the agent-turn-complete payload");
    assert!(
        case_at < emit_at,
        "the idle emit must sit inside the agent-turn-complete case arm"
    );

    // Pull the printf literal out of the artifact instead of restating it.
    let start = script
        .find("printf '")
        .expect("script must printf a marker")
        + "printf '".len();
    let end = start
        + script[start..]
            .find('\'')
            .expect("unterminated printf literal");
    let decoded = script[start..end]
        .replace("\\033", "\x1b")
        .replace("\\\\", "\\");

    // That exact byte sequence, through the real chunk path, must close a turn
    // a hook-busy is holding — the thing AC6(c) actually asserts.
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "codex-notify-turn-complete";
    agent_session(&state, session_id, SHELL_BUSY);
    {
        let mut session = state
            .session_maps
            .session_states
            .get_mut(session_id)
            .unwrap();
        session.agent_type = Some("codex".into());
        session.hook_instrumented = true;
    }
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 2000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
    silence.lock().note_explicit_state(SHELL_BUSY, true);
    assert!(
        silence.lock().hook_busy(),
        "precondition: a Protocol-rank hook busy is holding the turn"
    );

    let mut processor = ChunkProcessor::new(None, None);
    processor.process_chunk(&decoded, &silence, session_id, &state);

    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_IDLE,
        "the notify script's own bytes must drive the session idle"
    );
    let silence = silence.lock();
    assert!(
        silence.explicit_idle(),
        "idle must be Protocol rank, not screen"
    );
    assert!(!silence.hook_busy());
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

/// Regression fixture for the tty-resolution fix (commit `4914bb42`):
/// before it, a hook-instrumented Claude tab never emitted OSC 7770 at all
/// because `tuic-hook` couldn't find a tty to write to (see
/// `crates/tuic-hook/src/emit.rs`). Captured live via `POST
/// /diagnostics/capture` from a real hook-instrumented `claude` turn
/// (single `pwd` via the Bash tool, no failures) with the fix applied, then
/// replayed here through the same `TerminalGrid` the PTY hot path uses.
#[test]
fn claude_hook_osc7770_turn_reaches_busy_then_idle() {
    use crate::pty_capture::CaptureDirection;
    use crate::terminal_grid::TerminalGrid;

    let bytes = agent_prompt_fixture("claude-hook-osc7770-basic-turn.tcap");
    let mut grid = TerminalGrid::new(40, 120, 2000);
    let mut states = Vec::new();

    for record in crate::pty_capture::decode(&bytes).expect("valid capture") {
        if record.direction != CaptureDirection::Output {
            continue;
        }
        grid.process(&record.data);
        for event in grid.drain_events() {
            if let crate::terminal_grid::TermEvent::Tuic { verb, payload, .. } = event
                && verb == "state"
            {
                states.push(payload);
            }
        }
    }

    // Two `busy` events are real: the hook fires on both UserPromptSubmit
    // and PreToolUse. What the tty-resolution regression actually broke was
    // ALL of it — `emit()` returned early before a single byte reached the
    // terminal, so this vec would be empty and `last() == idle` would never
    // hold.
    assert_eq!(
        states,
        vec!["busy".to_string(), "busy".to_string(), "idle".to_string()],
        "the hook must resolve a tty and emit busy...busy...idle for this \
         single-tool turn — silently finding zero states is exactly how the \
         tty-resolution regression presented"
    );
}

/// Regression for a real hook-instrumented `claude` session that called
/// `AskUserQuestion` twice, answering each in turn. Live, the session's
/// `awaiting_input` badge lit for the first question and never lit for the
/// second — the tab read "working" while genuinely blocked on the user.
///
/// Root cause: `tuic_state_awaiting_event`'s hook-based `state=awaiting`
/// mapping always emits `ParsedEvent::Question { prompt_text: String::new(), .. }`
/// — there is no real question text to offer for a bare protocol marker.
/// The question-dedup guard a few chunks down keyed off that same
/// `prompt_text` with no regard for where the event came from, so the
/// first `state=awaiting` (empty prompt_text) recorded `last_question_text
/// = Some("")`, and the screen-absence reset that is supposed to retire it
/// (`!screen.iter().any(|row| row.contains(last))`) can never fire for an
/// empty needle — `"".contains("")` and `anything.contains("")` are both
/// trivially true, so `last_question_text` stayed `Some("")` for the rest
/// of the session. Every later hook-based awaiting signal (this fixture's
/// second `AskUserQuestion`, and any subsequent one) matched that stale
/// empty placeholder and was silently swallowed, one chunk before ever
/// reaching `SessionState`.
///
/// This replays only the OUTPUT half of the capture through the real
/// `ChunkProcessor::process_chunk` (the exact production hot path,
/// `last_question_text` and all) and counts "question" events published on
/// the event bus — a direct measurement of what the dedup let through,
/// independent of whether applying it to `SessionState` was itself a
/// no-op. The raw byte stream carries `state=awaiting` FOUR times (two
/// redundant hook-matcher registrations firing per `AskUserQuestion`, an
/// orthogonal quirk this test does not need to fix): before the dedup fix,
/// only the first ever survived (count == 1); after it, all four do.
#[test]
fn claude_double_askuserquestion_both_survive_dedup() {
    use crate::pty_capture::CaptureDirection;
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "test-double-askuserquestion";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(40, 120, 2000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(65536)));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));
    state.session_maps.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            hook_instrumented: true,
            agent_type: Some("claude".to_string()),
            ..Default::default()
        },
    );

    let mut cp = ChunkProcessor::new(None, None);
    let mut rx = state.event_bus.subscribe();
    let mut question_count = 0;

    let bytes = agent_prompt_fixture("claude_double_askuserquestion_second_missed.tcap");
    for record in crate::pty_capture::decode(&bytes).expect("valid capture") {
        if record.direction != CaptureDirection::Output {
            continue;
        }
        let data = String::from_utf8_lossy(&record.data).into_owned();
        let _ = cp.process_chunk(&data, &silence, sid, &state);
        while let Ok(evt) = rx.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
                && parsed.get("type").and_then(|t| t.as_str()) == Some("question")
            {
                question_count += 1;
            }
        }
    }

    assert_eq!(
        question_count, 4,
        "all four raw `state=awaiting` markers (two per AskUserQuestion, from \
         two hook-matcher registrations) must survive the question dedup — \
         a count stuck at 1 is exactly how the second AskUserQuestion's \
         awaiting signal got silently swallowed"
    );
}

/// Regression for the 2026-09-01 stuck-badge bug, live-reproduced on two real
/// unattended/auto-approve Claude sessions (`commerce-journal`'s `publish` tab,
/// `agent-tooling-analysis`'s `tool-scan` tab): a mid-turn `AskUserQuestion`
/// left `awaiting_input` stuck `true` through full task completion.
///
/// Root cause: `PreToolUse(AskUserQuestion)`'s `--state awaiting` override never
/// touches the shell busy/idle bit (`handle_tuic_state`'s doc comment — there is
/// no SHELL_AWAITING), so the shell was already `SHELL_BUSY` — the ordinary case,
/// mid-turn — both before and after. `claude_hook_map()`'s `PostToolUse
/// (AskUserQuestion|ExitPlanMode)` busy re-affirmation exists specifically to
/// clear the badge afterward, but landed on that same already-busy state:
/// `transition_explicit_shell_state` saw no edge, `busy_transitioned` was
/// `false`, and `tuic_state_awaiting_event`'s old signature dropped the clear
/// silently. Nothing else was allowed to touch a confident question besides
/// real user input, which never arrived (the agent answered its own question in
/// auto-mode) — the badge was permanently stuck.
///
/// This constructs the session already `awaiting_input: true, question_confident:
/// true` (the state a real `PreToolUse(AskUserQuestion)` `state=awaiting` fire
/// leaves it in, proven independently by the sibling
/// `claude_double_askuserquestion_both_survive_dedup` test above) and shell
/// already `SHELL_BUSY` (the ordinary mid-turn case), then feeds exactly the
/// `PostToolUse` busy re-affirmation through the real `process_chunk` hot path.
#[test]
fn claude_askuserquestion_midturn_busy_reaffirmation_clears_stuck_awaiting() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "test-midturn-askuserquestion-busy-reaffirmation";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    // Already busy, mid-turn — the ordinary case. `awaiting` never flips this.
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(sid.to_string(), std::sync::atomic::AtomicU64::new(0));
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(40, 120, 2000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(65536)));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));
    state.session_maps.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            hook_instrumented: true,
            agent_type: Some("claude".to_string()),
            awaiting_input: true,
            question_confident: true,
            ..Default::default()
        },
    );

    let mut cp = ChunkProcessor::new(None, None);
    let mut rx = state.event_bus.subscribe();

    // The PostToolUse(AskUserQuestion|ExitPlanMode) busy re-affirmation: a
    // same-state (SHELL_BUSY → SHELL_BUSY) re-affirmation, busy_transitioned
    // == false.
    let _ = cp.process_chunk("\x1b]7770;state=busy\x07", &silence, sid, &state);

    let mut saw_clear = false;
    while let Ok(evt) = rx.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
            && parsed.get("type").and_then(|t| t.as_str()) == Some("user-input")
        {
            saw_clear = true;
        }
    }
    assert!(
        saw_clear,
        "a redundant busy re-affirmation while awaiting_input is set must still \
         emit the UserInput clear — before the 2026-09-01 fix, busy_transitioned \
         being false silently dropped it and the badge stuck forever, surviving \
         even full task completion"
    );
}

/// Shared scaffolding for the `notification_hook_*_through_process_chunk`
/// wire-level tests below: a fresh session past startup grace, ready for
/// `process_chunk` to run real OSC 7770 bytes through.
fn notification_hook_test_session(
    sid: &str,
    shell_state: u8,
) -> (Arc<crate::state::AppState>, Arc<Mutex<SilenceState>>) {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    // Past startup grace — these tests are about steady-state Notification
    // behavior, not "historical output replayed by --continue," which is
    // what the grace period guards against by suppressing non-confident
    // questions outright.
    silence.lock().startup_settled = true;
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(shell_state),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(sid.to_string(), AtomicU64::new(0));
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(40, 120, 2000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(65536)));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));
    state.session_maps.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            hook_instrumented: true,
            agent_type: Some("claude".to_string()),
            ..Default::default()
        },
    );
    (state, silence)
}

/// The last "question" event seen on the bus, if any, plus whether one
/// was seen at all — distinguishing "no question event fired" from "one
/// fired with some confident value," which a bare `Option<bool>` can't.
fn drain_question_confidence(
    rx: &mut tokio::sync::broadcast::Receiver<crate::state::AppEvent>,
) -> Option<bool> {
    let mut confident = None;
    let mut saw_question = false;
    while let Ok(evt) = rx.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
            && parsed.get("type").and_then(|t| t.as_str()) == Some("question")
        {
            saw_question = true;
            confident = parsed.get("confident").and_then(|c| c.as_bool());
        }
    }
    saw_question.then_some(confident).flatten()
}

/// Wire-level regression for the 2026-09-02 `dbsql-test-review` incident
/// (`agent-signal-architecture.html`'s "OSC 777 vs OSC 7770" section): the
/// exact real shape from that session's `hook-debug.log` capture — a
/// `Stop` already put the shell idle, then ~60s later `Notification`
/// fires with `notification_type=idle_prompt`. `tuic-hook` derives
/// `notify=<message>`, `notifytype=<type>`, then `state=awaiting` in one
/// `write_all` for every `Notification` fire; this drives those exact
/// three wire sequences through the real `process_chunk` hot path (not
/// the bare `tuic_state_awaiting_event` unit above) to prove the
/// `notify`/`notifytype` → `state` pairing built into `ChunkProcessor`
/// actually threads both fields through, and that the shell-already-idle
/// case is dropped outright — no badge flash at all, confident or not.
#[test]
fn notification_hook_idle_prompt_after_stop_is_suppressed_through_process_chunk() {
    let sid = "test-notification-idle-prompt-after-stop";
    let (state, silence) = notification_hook_test_session(sid, SHELL_IDLE);

    let mut cp = ChunkProcessor::new(None, None);
    let mut rx = state.event_bus.subscribe();

    let _ = cp.process_chunk(
        "\x1b]7770;notify=Claude%20is%20waiting%20for%20your%20input\x1b\\\
         \x1b]7770;notifytype=idle_prompt\x1b\\\
         \x1b]7770;state=awaiting\x1b\\",
        &silence,
        sid,
        &state,
    );

    assert_eq!(
        drain_question_confidence(&mut rx),
        None,
        "idle_prompt after the shell is already idle (a real Stop already fired) \
         must not badge awaiting at all — a genuinely idle session must not flash \
         'awaiting' through full task completion, not even briefly"
    );
}

/// Sibling of the test above: the same `notification_type=idle_prompt`
/// firing while the shell is still busy (no `Stop` yet — e.g. stuck on an
/// un-hooked plan/skill picker) must still surface, just non-confidently,
/// since it's the only awaiting signal available for that gap.
#[test]
fn notification_hook_idle_prompt_mid_turn_is_non_confident_through_process_chunk() {
    let sid = "test-notification-idle-prompt-mid-turn";
    let (state, silence) = notification_hook_test_session(sid, SHELL_BUSY);

    let mut cp = ChunkProcessor::new(None, None);
    let mut rx = state.event_bus.subscribe();

    let _ = cp.process_chunk(
        "\x1b]7770;notify=Claude%20is%20waiting%20for%20your%20input\x1b\\\
         \x1b]7770;notifytype=idle_prompt\x1b\\\
         \x1b]7770;state=awaiting\x1b\\",
        &silence,
        sid,
        &state,
    );

    assert_eq!(
        drain_question_confidence(&mut rx),
        Some(false),
        "idle_prompt mid-turn (no Stop yet) must still surface, non-confidently"
    );
}

/// A genuine blocking `notification_type` (`permission_prompt`) must stay
/// a confident question, classified deterministically off the type field
/// rather than by sniffing `message` wording.
#[test]
fn notification_hook_permission_prompt_stays_confident_through_process_chunk() {
    let sid = "test-notification-permission-prompt";
    let (state, silence) = notification_hook_test_session(sid, SHELL_BUSY);

    let mut cp = ChunkProcessor::new(None, None);
    let mut rx = state.event_bus.subscribe();

    let _ = cp.process_chunk(
        "\x1b]7770;notify=Claude%20needs%20your%20permission%20to%20run%20this%20command\x1b\\\
         \x1b]7770;notifytype=permission_prompt\x1b\\\
         \x1b]7770;state=awaiting\x1b\\",
        &silence,
        sid,
        &state,
    );

    assert_eq!(
        drain_question_confidence(&mut rx),
        Some(true),
        "a real permission prompt must stay a confident question"
    );
}

/// A purely informational `notification_type` (`agent_completed` — a
/// background session finishing, not the agent asking anything) must
/// never badge awaiting at all, regardless of shell state.
#[test]
fn notification_hook_informational_type_never_badges_through_process_chunk() {
    let sid = "test-notification-informational-type";
    let (state, silence) = notification_hook_test_session(sid, SHELL_IDLE);

    let mut cp = ChunkProcessor::new(None, None);
    let mut rx = state.event_bus.subscribe();

    let _ = cp.process_chunk(
        "\x1b]7770;notify=Background%20task%20finished\x1b\\\
         \x1b]7770;notifytype=agent_completed\x1b\\\
         \x1b]7770;state=awaiting\x1b\\",
        &silence,
        sid,
        &state,
    );

    assert_eq!(
        drain_question_confidence(&mut rx),
        None,
        "a purely informational notification_type must never badge awaiting"
    );
}

/// Stronger than the test above: an informational `notification_type`
/// must push NO `PtyParsed` event at all besides its own metadata scrape
/// — not a `Question`, not a `UserInput` clear, nothing that could
/// disturb an already-confident awaiting session sitting mid-block (e.g.
/// a real `AskUserQuestion` block, with a background session finishing
/// concurrently).
#[test]
fn notification_hook_informational_type_emits_no_awaiting_related_event_at_all() {
    let sid = "test-notification-informational-no-side-effect";
    let (state, silence) = notification_hook_test_session(sid, SHELL_BUSY);

    let mut cp = ChunkProcessor::new(None, None);
    let mut rx = state.event_bus.subscribe();

    let _ = cp.process_chunk(
        "\x1b]7770;notify=Background%20task%20finished\x1b\\\
         \x1b]7770;notifytype=agent_completed\x1b\\\
         \x1b]7770;state=awaiting\x1b\\",
        &silence,
        sid,
        &state,
    );

    let mut other_types = Vec::new();
    while let Ok(evt) = rx.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt {
            let ty = parsed
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            if ty != "agent-metadata" {
                other_types.push(ty);
            }
        }
    }
    assert!(
        other_types.is_empty(),
        "an informational notification_type must emit nothing besides its own \
         metadata scrape — got: {other_types:?}"
    );
}

/// All confidences seen, in order — unlike `drain_question_confidence`
/// (which only keeps the last), this proves a batch of several
/// `Notification` fires in one chunk each got its OWN correctly paired
/// `notify`/`notifytype`, not a stale value left over from an earlier
/// fire in the same chunk.
fn drain_all_question_confidences(
    rx: &mut tokio::sync::broadcast::Receiver<crate::state::AppEvent>,
) -> Vec<Option<bool>> {
    let mut out = Vec::new();
    while let Ok(evt) = rx.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
            && parsed.get("type").and_then(|t| t.as_str()) == Some("question")
        {
            out.push(parsed.get("confident").and_then(|c| c.as_bool()));
        }
    }
    out
}

/// Two `Notification` fires batched into a single chunk (plausible if
/// Claude Code's own hook invocations land close enough together to
/// coalesce into one PTY read) must each classify off their OWN
/// `notify`/`notifytype` pair, not leak state across fires. Regression
/// guard for `pending_notify_message`/`pending_notification_type` being
/// simple `Option<String>` locals reused across the whole chunk's loop —
/// a `.take()` that ran at the wrong point, or an ordering assumption
/// that broke, would show up here as the second fire inheriting the
/// first's classification.
#[test]
fn two_notification_fires_in_one_chunk_classify_independently() {
    let sid = "test-notification-two-fires-one-chunk";
    let (state, silence) = notification_hook_test_session(sid, SHELL_BUSY);

    let mut cp = ChunkProcessor::new(None, None);
    let mut rx = state.event_bus.subscribe();

    let _ = cp.process_chunk(
        "\x1b]7770;notify=Claude%20needs%20your%20permission%20to%20run%20this%20command\x1b\\\
         \x1b]7770;notifytype=permission_prompt\x1b\\\
         \x1b]7770;state=awaiting\x1b\\\
         \x1b]7770;notify=Claude%20is%20waiting%20for%20your%20input\x1b\\\
         \x1b]7770;notifytype=idle_prompt\x1b\\\
         \x1b]7770;state=awaiting\x1b\\",
        &silence,
        sid,
        &state,
    );

    assert_eq!(
        drain_all_question_confidences(&mut rx),
        vec![Some(true), Some(false)],
        "each Notification fire in the batch must classify off its own paired \
         notify/notifytype, not a stale value from the previous fire in the same chunk"
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
            .session_maps
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
    state.session_maps.session_states.insert(
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

#[tokio::test(flavor = "current_thread")]
async fn protocol_awaiting_clears_on_protocol_busy_and_idle() {
    for (session_id, target, label) in [
        ("protocol-clear-busy", SHELL_BUSY, "busy"),
        ("protocol-clear-idle", SHELL_IDLE, "idle"),
    ] {
        let state = accumulating_state(session_id);
        state.session_maps.shell_states.insert(
            session_id.to_string(),
            std::sync::atomic::AtomicU8::new(if target == SHELL_BUSY {
                SHELL_IDLE
            } else {
                SHELL_BUSY
            }),
        );
        state.session_maps.silence_states.insert(
            session_id.to_string(),
            Arc::new(Mutex::new(SilenceState::new())),
        );
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: session_id.to_string(),
            parsed: serde_json::json!({
                "type": "question",
                "prompt_text": "approval required",
                "confident": true,
            })
            .into(),
        });
        assert!(await_session(&state, session_id, |s| s.awaiting_input).await);

        transition_explicit_shell_state_with_hook(&state, session_id, target, label, true, || {});
        assert!(
            await_session(&state, session_id, |s| !s.awaiting_input
                && s.question_text.is_none())
            .await,
            "{label} must clear Protocol awaiting"
        );
        assert_eq!(
            state
                .session_maps
                .silence_states
                .get(session_id)
                .unwrap()
                .lock()
                .awaiting_rank(),
            None
        );
    }
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
    state
        .session_maps
        .session_states
        .insert("s1".to_string(), session);

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
    state.session_maps.session_states.insert(
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
        .session_maps
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
        native_status_signals: Some(false),
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
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));
    state
        .session_maps
        .has_osc133_integration
        .insert(session_id.to_string(), ());

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('A', "", session_id, &state);

    let current = state
        .session_maps
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
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));
    state
        .session_maps
        .has_osc133_integration
        .insert(session_id.to_string(), ());

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('C', "", session_id, &state);

    let current = state
        .session_maps
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
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state
        .session_maps
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
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));
    state
        .session_maps
        .has_osc133_integration
        .insert(session_id.to_string(), ());

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('D', "0", session_id, &state);

    let current = state
        .session_maps
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
        .session_maps
        .has_osc133_integration
        .insert(session_id.to_string(), ());

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('D', "1", session_id, &state);

    let recorded = state
        .ai
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
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));
    state
        .session_maps
        .has_osc133_integration
        .insert(session_id.to_string(), ());

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('C', "", session_id, &state);
    proc.handle_osc133_event('D', "0", session_id, &state);

    let recorded = state
        .ai
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

// --- synthesize_cc_block_events tests (fallback block source) ---

fn header_row(row_index: usize, tool: &str) -> ChangedRow {
    ChangedRow {
        row_index,
        text: format!("⏺ {tool}(args)"),
    }
}

/// Extract `(action, line)` from an `AgentBlock` event, panicking on any
/// other variant — keeps the assertions below readable.
fn agent_block(event: &ParsedEvent) -> (&str, i64) {
    match event {
        ParsedEvent::AgentBlock { action, line, .. } => (action.as_str(), *line),
        other => panic!("expected AgentBlock, got {other:?}"),
    }
}

#[test]
fn cc_block_end_carries_next_header_line_not_previous_start() {
    // The regression this fixes: end used to carry `prev` (the previous
    // block's own start line), giving every synthesized block
    // `endLine === promptLine`.
    let mut last = None;
    let rows = vec![header_row(3, "Read"), header_row(9, "Edit")];
    let events = synthesize_cc_block_events(&rows, 100, true, 0, false, &mut last);
    assert_eq!(events.len(), 3);
    assert_eq!(agent_block(&events[0]), ("start", 103));
    assert_eq!(agent_block(&events[1]), ("end", 109));
    assert_ne!(
        agent_block(&events[1]).1,
        103,
        "end must not be the previous start"
    );
    assert_eq!(agent_block(&events[2]), ("start", 109));
    assert_eq!(last, Some(109));
}

#[test]
fn cc_block_end_is_exclusive_so_fold_count_is_positive() {
    let mut last = Some(50);
    let rows = vec![header_row(4, "Bash")];
    let events = synthesize_cc_block_events(&rows, 100, true, 0, false, &mut last);
    let end_line = events
        .iter()
        .find_map(|e| match e {
            ParsedEvent::AgentBlock { action, line, .. } if action == "end" => Some(*line),
            _ => None,
        })
        .expect("expected an end event");
    // foldStart = promptLine + 1 = 51; must be strictly less than endLine.
    assert!(end_line > 51, "endLine {end_line} must exceed foldStart 51");
}

#[test]
fn cc_block_teardown_uses_cursor_line_not_block_start() {
    let mut last = Some(50);
    let events = synthesize_cc_block_events(&[], 100, false, 180, false, &mut last);
    assert_eq!(events.len(), 1);
    assert_eq!(agent_block(&events[0]), ("end", 180));
    assert_eq!(last, None, "teardown must clear the dangling block");
}

#[test]
fn cc_block_end_never_precedes_its_start_when_abs_lines_regress() {
    // Once scrollback saturates, history_size stops growing while
    // row_index keeps cycling, so abs_line is not globally monotonic.
    let mut last = Some(200);
    let rows = vec![header_row(50, "Bash")]; // abs_line = 100 + 50 = 150 < 200
    let events = synthesize_cc_block_events(&rows, 100, true, 0, false, &mut last);
    assert_eq!(events.len(), 2, "expected an end event and a start event");
    assert_eq!(
        agent_block(&events[0]),
        ("end", 201),
        "must clamp to prev + 1, never end < start"
    );
    // The new block's start must not precede the end just emitted for the
    // block it closed — an unclamped start here would overlap the two.
    assert_eq!(
        agent_block(&events[1]),
        ("start", 201),
        "the new block's start must not regress behind the end it follows"
    );
    assert_eq!(
        last,
        Some(201),
        "last_agent_block_line must track the clamped start, not the raw abs_line"
    );
}

#[test]
fn cc_block_dedups_repeated_header_on_same_line() {
    let mut last = Some(150);
    let rows = vec![header_row(50, "Bash")]; // abs_line = 100 + 50 = 150 == last
    let events = synthesize_cc_block_events(&rows, 100, true, 0, false, &mut last);
    assert!(
        events.is_empty(),
        "repeated header on the same line must not re-start"
    );
    assert_eq!(last, Some(150));
}

#[test]
fn cc_block_teardown_with_no_active_block_emits_nothing() {
    let mut last = None;
    let events = synthesize_cc_block_events(&[], 100, false, 180, false, &mut last);
    assert!(events.is_empty());
    assert_eq!(last, None);
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
    state.session_maps.sessions.insert(
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

    close_pty_core(&state, sid, false, "closed");

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
        !state.session_maps.kitty_states.contains_key(sid),
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
        state
            .session_maps
            .kitty_states
            .get(sid)
            .unwrap()
            .lock()
            .current_flags(),
        5
    );

    // Pop returns to the first pushed value.
    process_kitty_actions(&[KittyAction::Pop], sid, &state);
    assert_eq!(
        state
            .session_maps
            .kitty_states
            .get(sid)
            .unwrap()
            .lock()
            .current_flags(),
        1
    );

    // Query with no live PTY session must not panic (writer path is skipped)
    // and must leave the flag stack untouched.
    process_kitty_actions(&[KittyAction::Query], sid, &state);
    assert_eq!(
        state
            .session_maps
            .kitty_states
            .get(sid)
            .unwrap()
            .lock()
            .current_flags(),
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
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state.grid.vt_log_buffers.insert(
        sid.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    state
        .session_maps
        .kitty_states
        .insert(sid.to_string(), Mutex::new(KittyKeyboardState::new()));
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_IDLE));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));
    state
        .session_maps
        .term_aliases
        .insert(sid.to_string(), "alias".to_string());
    state.session_maps.exit_codes.insert(sid.to_string(), 0);
    state
        .session_maps
        .has_osc133_integration
        .insert(sid.to_string(), ());
    state
        .session_maps
        .has_tuic_state_integration
        .insert(sid.to_string(), ());
    state
        .session_maps
        .turn_error_flags
        .insert(sid.to_string(), ());
    state
        .session_maps
        .pty_accent_colors
        .insert(sid.to_string(), "blue".to_string());

    cleanup_session(sid, &state, "closed");

    assert!(!state.session_maps.output_buffers.contains_key(sid));
    assert!(!state.grid.vt_log_buffers.contains_key(sid));
    assert!(!state.session_maps.kitty_states.contains_key(sid));
    assert!(!state.session_maps.shell_states.contains_key(sid));
    assert!(!state.session_maps.last_output_ms.contains_key(sid));
    assert!(!state.session_maps.term_aliases.contains_key(sid));
    assert!(!state.session_maps.exit_codes.contains_key(sid));
    assert!(
        !state.session_maps.has_osc133_integration.contains_key(sid),
        "must not leak a permanent entry per session UUID"
    );
    assert!(
        !state
            .session_maps
            .has_tuic_state_integration
            .contains_key(sid),
        "must not leak a permanent entry per session UUID"
    );
    assert!(
        !state.session_maps.turn_error_flags.contains_key(sid),
        "must not leak a pending failure flag past session teardown"
    );
    assert!(
        !state.session_maps.pty_accent_colors.contains_key(sid),
        "must not leak a permanent accent-color entry per session UUID — the tmux \
         shim's set-option dispatch is the only writer, but every session that ever \
         gets one must still have it reaped on close"
    );
}

// ── list_active_sessions ────────────────────────────────────────

/// `list_active_sessions` (the desktop IPC twin of
/// `mcp_http::session::list_sessions`) had no test at all — testing the
/// `#[tauri::command]` wrapper directly isn't possible without a real
/// running app (no test anywhere in this codebase constructs a
/// `tauri::State` outside one), so this exercises the extracted
/// `list_active_sessions_impl` instead. The one thing worth proving:
/// `pty_accent_colors` (a side-map, not a `PtySession` field) surfaces
/// per-session, same as the HTTP twin's own
/// `list_sessions_reports_each_sessions_own_accent_color` test.
#[test]
fn list_active_sessions_impl_reports_each_sessions_own_accent_color() {
    let state = crate::state::tests_support::make_test_app_state();
    crate::state::tests_support::insert_dummy_session(&state, "colored");
    crate::state::tests_support::insert_dummy_session(&state, "plain");
    state.set_pty_accent_color("colored", Some("blue".to_string()));

    let sessions = list_active_sessions_impl(&state);
    let colored = sessions.iter().find(|s| s.session_id == "colored").unwrap();
    let plain = sessions.iter().find(|s| s.session_id == "plain").unwrap();
    assert_eq!(colored.accent_color.as_deref(), Some("blue"));
    assert_eq!(plain.accent_color, None);
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
        .ai
        .ai_suggestions_enabled
        .insert(sid.to_string(), true);
    state.ai.file_sandboxes.insert(
        sid.to_string(),
        crate::ai_agent::sandbox::FileSandbox::new(std::env::temp_dir()).expect("sandbox"),
    );
    state.ai.unrestricted_sessions.insert(sid.to_string(), ());
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
    assert!(!state.ai.ai_suggestions_enabled.contains_key(sid));
    assert!(!state.session_maps.term_aliases.contains_key(sid));

    // Owned elsewhere, deliberately untouched — see the DEFERRED note on
    // remove_post_mortem_session_state. A sandbox belongs to a conversation
    // that outlives the PTY; knowledge is what the next session inherits.
    assert!(state.ai.file_sandboxes.contains_key(sid));
    assert!(state.ai.unrestricted_sessions.contains_key(sid));
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
    assert!(state.ai.ai_suggestions_enabled.contains_key(sid));
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
    assert!(!state.ai.ai_suggestions_enabled.contains_key(sid));
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
    state
        .session_maps
        .standby_sessions
        .insert(sid.to_string(), 0);

    let res = wake_session(&state, sid);

    assert!(
        res.is_err(),
        "a standby entry without a live session must error"
    );
    assert!(res.unwrap_err().contains("Session not found"));
    assert!(
        !state.session_maps.standby_sessions.contains_key(sid),
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
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(41, 128, 2000)));
    state.session_maps.output_buffers.insert(
        sid.to_string(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));
    state
        .session_maps
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
        && let Some(mut entry) = state.session_maps.session_states.get_mut(sid)
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
            .session_maps
            .shell_states
            .get(sid)
            .map(|a| a.load(std::sync::atomic::Ordering::Acquire)),
        last_question_text: cp.last_question_text.clone(),
        last_choice_prompt_sig: cp.last_choice_prompt_sig.clone(),
        terminal_mode_fullscreen: cp.terminal_mode.is_fullscreen(),
        ring_len: state
            .session_maps
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
            // `progress` at chunk 0: the fixture carries `ESC]9;4;0;BEL` and
            // `ESC]9;4;3;BEL` (state with no value digits) — parseable since
            // OSC 9;4 gained the error/warning/indeterminate states, whose
            // value is optional. The pre-2026-09 baseline predated that.
            "claude-plan-picker.raw" => {
                "[(0, [\"progress\", \"status-line\", \"shell-state\"])] \
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
                .session_maps
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
        if let Some(mut entry) = state.session_maps.session_states.get_mut(sid) {
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
            .grid
            .watch
            .insert("s1".to_string(), crate::grid_gate::new_grid_watch());
        assert!(!grid_has_subscriber(&state, "s1"));
    }

    #[test]
    fn a_live_watch_receiver_is_a_subscriber() {
        let state = crate::state::tests_support::make_test_app_state();
        let tx = crate::grid_gate::new_grid_watch();
        let rx = tx.subscribe();
        state.grid.watch.insert("s1".to_string(), tx);

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
        state.grid.watch.insert("watched".to_string(), tx);

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
        state.grid.vt_log_buffers.insert(
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
        state.grid.vt_log_buffers.insert(
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
        state.grid.vt_log_buffers.insert(
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
        state
            .session_maps
            .standby_sessions
            .insert("gone-1".to_string(), 111);
        state
            .session_maps
            .standby_sessions
            .insert("gone-2".to_string(), 222);
        state
            .session_maps
            .standby_sessions
            .insert("gone-3".to_string(), 333);

        let attempted = wake_all_standby(&state);

        assert_eq!(attempted, 3, "wake attempted for every parked session");
        assert!(
            state.session_maps.standby_sessions.is_empty(),
            "standby map must be empty after wake-all even when the sessions are gone"
        );
    }

    /// Empty standby map is a no-op — no panic, nothing to wake.
    #[test]
    fn wake_all_standby_empty_is_noop() {
        let state = crate::state::tests_support::make_test_app_state();
        assert_eq!(wake_all_standby(&state), 0);
        assert!(state.session_maps.standby_sessions.is_empty());
    }

    /// After a timeout=0 wake-all, standby can re-arm normally when the user
    /// sets a positive timeout again — wake_all_standby sets no persistent
    /// "disabled" flag, it only clears the current standby set.
    #[test]
    fn wake_all_standby_leaves_map_ready_to_rearm() {
        let state = crate::state::tests_support::make_test_app_state();
        state
            .session_maps
            .standby_sessions
            .insert("gone-1".to_string(), 111);
        wake_all_standby(&state);
        assert!(state.session_maps.standby_sessions.is_empty());

        // Re-arming (as the checker would on the next tick with timeout>0) works.
        state
            .session_maps
            .standby_sessions
            .insert("re-armed".to_string(), 444);
        assert_eq!(state.session_maps.standby_sessions.len(), 1);
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

#[cfg(test)]
mod grid_delivery_tests {
    use super::*;
    use crate::grid_gate::{GridGate, new_grid_watch};

    // --- Frame ordering (670-b9a2) ---
    //
    // Every producer serializes under the vt lock and calls `send_grid_frame`
    // after releasing it, so two of them can reach the transport in the opposite
    // order. The frames are DELTAS whose damage was consumed when they were cut,
    // so the older one carries rows the newer one does not have: painting it last
    // reverts those rows and nothing ever sends them again. The resize path makes
    // it visible — it cuts a FULL frame, and a full frame landing after a delta is
    // the "blank after zoom" the resize flush exists to prevent.
    //
    // These tests inject the reordering directly rather than racing two threads
    // for it: a race that happens to come out in order proves nothing, and one
    // that comes out reversed proves it only on the run where it did.

    /// A session as the spawn paths leave it: a vt buffer, a grid watch and the
    /// ticker's dirty flag. Returns a live watch receiver — without one the watch
    /// has no subscribers and `send_grid_frame` hands it nothing.
    fn grid_session(
        state: &Arc<AppState>,
        session_id: &str,
    ) -> tokio::sync::watch::Receiver<crate::grid_gate::GridWatchFrame> {
        state.grid.vt_log_buffers.insert(
            session_id.to_string(),
            Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
        );
        state
            .grid
            .frame_dirty
            .insert(session_id.to_string(), Arc::new(AtomicBool::new(false)));
        let tx = new_grid_watch();
        let rx = tx.subscribe();
        state.grid.watch.insert(session_id.to_string(), tx);
        rx
    }

    /// Feed the grid and cut the frame that carries what just changed, exactly as
    /// a producer does inside its own vt critical section.
    fn cut_frame(
        state: &Arc<AppState>,
        session_id: &str,
        text: &str,
    ) -> crate::grid_gate::GridFrame {
        let vt = state
            .grid
            .vt_log_buffers
            .get(session_id)
            .expect("session exists");
        let mut vt = vt.lock();
        vt.process(text.as_bytes());
        vt.serialize_dirty_rows()
    }

    /// Rows a frame carries, off the `row_count` header field.
    fn row_count(frame: &[u8]) -> u16 {
        u16::from_le_bytes([frame[0], frame[1]])
    }

    #[test]
    fn a_frame_that_lost_the_ordering_race_does_not_repaint_the_newer_screen() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let rx = grid_session(&state, "reorder");

        // Two producers, cut in this order under the vt lock.
        let older = cut_frame(&state, "reorder", "first\r\n");
        let newer = cut_frame(&state, "reorder", "second\r\n");
        assert!(!older.is_empty() && !newer.is_empty());

        // Both released the lock before sending, and they arrive reversed.
        send_grid_frame(&state, "reorder", newer.clone());
        send_grid_frame(&state, "reorder", older);

        assert_eq!(
            rx.borrow().frame,
            newer.bytes,
            "a frame cut earlier painted over the newer screen"
        );
    }

    #[test]
    fn the_rows_a_dropped_frame_carried_come_back_as_a_full_repaint() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let _rx = grid_session(&state, "repaint");

        let older = cut_frame(&state, "repaint", "first\r\n");
        let newer = cut_frame(&state, "repaint", "second\r\n");
        send_grid_frame(&state, "repaint", newer);
        send_grid_frame(&state, "repaint", older);

        // Dropping the loser silently is not an option: both frames consumed the
        // damage that produced them, so the rows in the dropped one reach nobody
        // unless the grid is damaged again.
        assert!(
            state
                .grid
                .frame_dirty
                .get("repaint")
                .expect("flag exists")
                .load(Ordering::Relaxed),
            "the repair has to be armed or the dropped rows are lost for good"
        );
        let repaint = {
            let vt = state
                .grid
                .vt_log_buffers
                .get("repaint")
                .expect("session exists");
            let mut vt = vt.lock();
            vt.serialize_dirty_rows()
        };
        assert_eq!(
            row_count(&repaint.bytes),
            24,
            "the repair must be a whole screen — a delta cannot name rows nobody tracked"
        );
    }

    #[test]
    fn frames_arriving_in_the_order_they_were_cut_are_all_delivered() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut rx = grid_session(&state, "in-order");

        let first = cut_frame(&state, "in-order", "first\r\n");
        send_grid_frame(&state, "in-order", first.clone());
        assert_eq!(rx.borrow_and_update().frame, first.bytes);

        let second = cut_frame(&state, "in-order", "second\r\n");
        send_grid_frame(&state, "in-order", second.clone());
        assert_eq!(rx.borrow_and_update().frame, second.bytes);

        // The ordering check must not cost a repaint on the path every frame
        // takes: only a genuine reversal may arm one.
        assert!(
            !state
                .grid
                .frame_dirty
                .get("in-order")
                .expect("flag exists")
                .load(Ordering::Relaxed),
            "the ordinary path armed a full repaint it does not need"
        );
    }

    // --- A stalled WebView must not starve the browser (670-b9a2) ---
    //
    // `GridGate` belongs to the desktop IPC channel: it counts frames sent
    // against frames the WebView reported painting. The ticker used to check it
    // before serializing anything, so a WebView blocked on its own main thread
    // stopped the frames going to browser/PWA clients — a different transport,
    // with its own flow control, that had not fallen behind at all.

    /// A reader that keeps the session alive until the test releases it, then
    /// reports EOF so the reader and ticker threads shut down normally.
    struct StopOnFlag(Arc<AtomicBool>);

    impl Read for StopOnFlag {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            while !self.0.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Ok(0)
        }
    }

    /// Start the frame ticker for a session that already has a vt buffer and a
    /// grid watch. Returns the stop flag — set it to tear the threads down.
    fn start_ticker(state: &Arc<AppState>, session_id: &str) -> Arc<AtomicBool> {
        let stop = Arc::new(AtomicBool::new(false));
        spawn_reader_thread(
            Box::new(StopOnFlag(stop.clone())),
            Arc::new(AtomicBool::new(false)),
            session_id.to_string(),
            state.clone(),
            None,
        );
        stop
    }

    /// Outer bound only: it answers "did the ticker ever publish", nothing about
    /// how fast. The ticker runs on a 16 ms interval, so any real delivery is
    /// three orders of magnitude inside this; a timeout means no frame was ever
    /// serialized, which is the defect itself.
    const TICKER_LIVENESS_BOUND: std::time::Duration = std::time::Duration::from_secs(10);

    /// Keep the grid changing and the ticker armed, the way a live PTY reader
    /// does. Returns a handle that stops the feed when the test drops it.
    fn feed_continuously(state: &Arc<AppState>, session_id: &str) -> Arc<AtomicBool> {
        let stop = Arc::new(AtomicBool::new(false));
        let feeder_stop = stop.clone();
        let feeder_state = state.clone();
        let feeder_sid = session_id.to_string();
        std::thread::spawn(move || {
            let mut n = 0u32;
            while !feeder_stop.load(Ordering::Relaxed) {
                if let Some(vt) = feeder_state.grid.vt_log_buffers.get(&feeder_sid) {
                    vt.lock().process(format!("line {n}\r\n").as_bytes());
                }
                if let Some(dirty) = feeder_state.grid.frame_dirty.get(&feeder_sid) {
                    dirty.store(true, Ordering::Relaxed);
                }
                n += 1;
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        });
        stop
    }

    /// Every frame a subscriber was handed, in the order it was handed them.
    #[cfg(feature = "desktop")]
    type RecordedFrames = Arc<Mutex<Vec<Vec<u8>>>>;

    /// A desktop subscriber that records every frame it is handed and acks none —
    /// a WebView whose JS thread is blocked.
    ///
    /// Real, not a stand-in: registering the channel makes the production path
    /// mark the gate sent, so the gate closes the way it closes in the app rather
    /// than being pinned closed by the test, and the recording is what actually
    /// left Rust for that channel.
    #[cfg(feature = "desktop")]
    fn subscribe_a_frozen_webview(
        state: &Arc<AppState>,
        session_id: &str,
    ) -> (Arc<GridGate>, RecordedFrames) {
        let gate = Arc::new(GridGate::new());
        state
            .grid
            .gates
            .insert(session_id.to_string(), gate.clone());
        let received: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = received.clone();
        state.grid.channels.insert(
            session_id.to_string(),
            tauri::ipc::Channel::new(move |body| {
                if let tauri::ipc::InvokeResponseBody::Raw(bytes) = body {
                    sink.lock().push(bytes);
                }
                Ok(())
            }),
        );
        (gate, received)
    }

    /// How long the browser is watched for while the desktop never acks.
    ///
    /// This bound IS the subject: the question is not whether a frame ever
    /// arrives, it is at what rate. Stopping the ticker on a closed gate does not
    /// silence the browser forever — the ticker gives the outstanding frame up
    /// after `MAX_IN_FLIGHT_MS` (500 ms) and the next tick sends one, which the
    /// frozen WebView immediately closes the gate with again. So the browser was
    /// throttled from the 16 ms tick to roughly 2 frames a second, and every third
    /// give-up adds a 1 s pause. Measured over this window: 3 frames on that
    /// path, 33 at the tick rate.
    const BROWSER_FEED_WINDOW: std::time::Duration = std::time::Duration::from_millis(1200);

    /// Between the two: twice what the give-up path produced in
    /// `BROWSER_FEED_WINDOW`, a fifth of what the tick rate produced. Sized for
    /// the gap, not for the expected value, so a machine five times slower than
    /// this one still passes.
    const BROWSER_FRAMES_EXPECTED: u64 = 6;

    #[cfg(feature = "desktop")]
    #[tokio::test(flavor = "current_thread", start_paused = false)]
    async fn a_stalled_desktop_gate_does_not_stop_the_browser_frames() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut rx = grid_session(&state, "stalled-webview");
        let (gate, _received) = subscribe_a_frozen_webview(&state, "stalled-webview");

        let stop = start_ticker(&state, "stalled-webview");
        // `spawn_reader_thread` installs the dirty flag the ticker reads, so the
        // feed can only start once the threads are up.
        let stop_feed = feed_continuously(&state, "stalled-webview");

        let first_seq = rx.borrow_and_update().seq;
        tokio::time::sleep(BROWSER_FEED_WINDOW).await;
        let frames = rx.borrow_and_update().seq - first_seq;

        stop_feed.store(true, Ordering::Relaxed);
        stop.store(true, Ordering::Relaxed);

        assert!(
            !gate.is_open(),
            "the WebView under test has to still be behind, or nothing was throttling"
        );
        assert!(
            frames >= BROWSER_FRAMES_EXPECTED,
            "the browser got {frames} frames in {BROWSER_FEED_WINDOW:?} while the desktop \
             gate was closed; a stalled WebView is still throttling a transport that \
             never fell behind"
        );
    }

    #[cfg(feature = "desktop")]
    #[tokio::test(flavor = "current_thread", start_paused = false)]
    async fn the_desktop_is_owed_a_full_frame_after_it_catches_up() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut rx = grid_session(&state, "caught-up");
        let (gate, received) = subscribe_a_frozen_webview(&state, "caught-up");

        // The WebView has caught up, but frames went out to the WebSocket
        // subscribers while it was behind: those rows left the shared damage
        // without ever reaching its channel, so a delta now would land on a row
        // map with holes in it.
        gate.note_missed();
        assert!(
            gate.is_open(),
            "the debt is only repayable once it catches up"
        );

        // Drain the first frame a fresh grid always owes — it has no previous
        // viewport to diff against, so it is full by construction and would say
        // nothing about the repair below.
        let _ = cut_frame(&state, "caught-up", "already painted\r\n");

        let stop = start_ticker(&state, "caught-up");
        {
            let vt = state
                .grid
                .vt_log_buffers
                .get("caught-up")
                .expect("session exists");
            vt.lock().process(b"one more line\r\n");
        }
        state
            .grid
            .frame_dirty
            .get("caught-up")
            .expect("the ticker owns this flag")
            .store(true, Ordering::Relaxed);

        let delivered = tokio::time::timeout(TICKER_LIVENESS_BOUND, rx.changed()).await;
        stop.store(true, Ordering::Relaxed);
        delivered
            .expect("the frame ticker never took a tick")
            .expect("the watch sender outlives the test");

        // Assert on the FIRST frame the channel got, not the last: the WebView
        // stays frozen, so every later tick finds the gate shut again and the
        // count keeps moving after the assertion is made.
        let first = received
            .lock()
            .first()
            .cloned()
            .expect("the desktop channel got nothing at all");
        assert_eq!(
            row_count(&first),
            24,
            "the desktop's first frame after the gap has to be the whole screen — \
             a delta would paint onto rows it never received"
        );
        // The repair is private to that channel: it consumes no damage, so what
        // the browser got on the same tick is still the ordinary delta.
        assert!(
            row_count(&rx.borrow_and_update().frame) < 24,
            "repairing the desktop must not cost the browser a full frame"
        );
    }
}

#[test]
fn tuic_state_awaiting_with_no_notify_yields_confident_question() {
    // "awaiting" is unrelated to the busy/idle transition edge (see
    // handle_tuic_state's doc comment — there is no SHELL_AWAITING), so
    // it must fire regardless of the busy_transitioned flag. No preceding
    // notify/notifytype means this came from PreToolUse(AskUserQuestion|
    // ExitPlanMode) or Elicitation, both always genuine blocks.
    match tuic_state_awaiting_event("awaiting", 0, false, false, None, None, false) {
        Some(ParsedEvent::Question {
            confident,
            prompt_text,
        }) => {
            assert!(confident, "hook awaiting with no notify must be confident");
            assert_eq!(prompt_text, "", "hook awaiting carries no prompt text");
        }
        other => panic!("expected confident Question, got {other:?}"),
    }
}

#[test]
fn tuic_state_awaiting_idle_prompt_after_stop_is_suppressed_entirely() {
    // The exact 2026-09-02 dbsql-test-review shape, straight from the real
    // hook-debug.log capture: Stop fires, the shell goes idle, then ~60s
    // later Notification fires with notification_type=idle_prompt. The
    // turn already ended — nothing pending — so this must not even flash
    // a non-confident badge, let alone a confident one.
    assert!(
        tuic_state_awaiting_event(
            "awaiting",
            0,
            false,
            false,
            Some("Claude is waiting for your input"),
            Some("idle_prompt"),
            true, // shell already idle — Stop already fired
        )
        .is_none(),
        "idle_prompt after the shell is already idle must be dropped outright, \
         not just made non-confident"
    );
}

#[test]
fn tuic_state_awaiting_idle_prompt_mid_turn_is_non_confident_not_suppressed() {
    // Same notification_type, but the shell is still busy (no Stop yet) —
    // e.g. stuck on an un-hooked plan/skill picker. This is the only
    // awaiting signal available for that gap, so it must still surface,
    // just not confidently (the silence-timer backstop can retract it).
    match tuic_state_awaiting_event(
        "awaiting",
        0,
        false,
        false,
        Some("Claude is waiting for your input"),
        Some("idle_prompt"),
        false, // still mid-turn
    ) {
        Some(ParsedEvent::Question { confident, .. }) => {
            assert!(
                !confident,
                "idle_prompt mid-turn must surface but not confidently"
            );
        }
        other => panic!("expected a (non-confident) Question, got {other:?}"),
    }
}

#[test]
fn tuic_state_awaiting_blocking_notification_types_stay_confident() {
    for notification_type in [
        "permission_prompt",
        "elicitation_dialog",
        "elicitation_url_dialog",
        "agent_needs_input",
        "quota_auto_resume_stale",
    ] {
        match tuic_state_awaiting_event(
            "awaiting",
            0,
            false,
            false,
            Some("irrelevant wording"),
            Some(notification_type),
            false,
        ) {
            Some(ParsedEvent::Question { confident, .. }) => {
                assert!(confident, "{notification_type} must stay confident");
            }
            other => {
                panic!("{notification_type}: expected a confident Question, got {other:?}")
            }
        }
    }
}

#[test]
fn tuic_state_awaiting_informational_notification_types_are_suppressed() {
    for notification_type in [
        "auth_success",
        "elicitation_complete",
        "elicitation_response",
        "agent_completed",
        "quota_auto_resume_fired",
        "quota_auto_resume_disabled",
    ] {
        assert!(
            tuic_state_awaiting_event(
                "awaiting",
                0,
                false,
                false,
                Some("irrelevant wording"),
                Some(notification_type),
                false,
            )
            .is_none(),
            "{notification_type} is purely informational and must never badge awaiting"
        );
    }
}

#[test]
fn tuic_state_awaiting_unrecognized_notification_type_falls_back_to_wording() {
    // A future Claude Code notification_type this binary predates — must
    // not guess confident (could silently re-latch this exact incident
    // under a new name) or guess informational (could silently swallow a
    // real future block). Falls back to the same wording rule used when
    // notification_type is absent entirely.
    match tuic_state_awaiting_event(
        "awaiting",
        0,
        false,
        false,
        Some("Claude needs your permission to run this command"),
        Some("some_future_type_2027"),
        false,
    ) {
        Some(ParsedEvent::Question { confident, .. }) => {
            assert!(confident, "permission wording must still win the fallback");
        }
        other => panic!("expected a confident Question, got {other:?}"),
    }
    match tuic_state_awaiting_event(
        "awaiting",
        0,
        false,
        false,
        Some("just some other text"),
        Some("some_future_type_2027"),
        false,
    ) {
        Some(ParsedEvent::Question { confident, .. }) => {
            assert!(!confident, "generic wording must not win the fallback");
        }
        other => panic!("expected a (non-confident) Question, got {other:?}"),
    }
}

#[test]
fn tuic_state_awaiting_with_generic_notify_message_and_no_type_is_not_confident() {
    // An older Claude Code build that predates notification_type entirely
    // — falls back to `parse_osc777_notifies`'s own wording rule.
    match tuic_state_awaiting_event(
        "awaiting",
        0,
        false,
        false,
        Some("Claude is waiting for your input"),
        None,
        false,
    ) {
        Some(ParsedEvent::Question { confident, .. }) => {
            assert!(
                !confident,
                "generic idle-timer wording must not latch a confident question"
            );
        }
        other => panic!("expected a (non-confident) Question, got {other:?}"),
    }
}

#[test]
fn tuic_state_awaiting_with_permission_notify_message_and_no_type_stays_confident() {
    // Unambiguous wording — a real permission prompt — must stay sticky
    // until real user input, same as the OSC 777 classification this
    // mirrors (output_parser.rs::parse_osc777_notifies).
    match tuic_state_awaiting_event(
        "awaiting",
        0,
        false,
        false,
        Some("Claude needs your permission to run this command"),
        None,
        false,
    ) {
        Some(ParsedEvent::Question { confident, .. }) => {
            assert!(confident, "a real permission prompt must stay confident");
        }
        other => panic!("expected a confident Question, got {other:?}"),
    }
}

#[test]
fn tuic_state_awaiting_with_approval_notify_message_and_no_type_stays_confident() {
    match tuic_state_awaiting_event(
        "awaiting",
        0,
        false,
        false,
        Some("approval required before continuing"),
        None,
        false,
    ) {
        Some(ParsedEvent::Question { confident, .. }) => {
            assert!(confident, "approval-required wording must stay confident");
        }
        other => panic!("expected a confident Question, got {other:?}"),
    }
}

#[test]
fn tuic_state_busy_without_transition_but_awaiting_yields_userinput_clear() {
    // Regression for the 2026-09-01 stuck-badge bug: `AskUserQuestion` fires
    // mid-turn (shell already SHELL_BUSY before and after — awaiting never
    // touches the shell bit), so the PostToolUse(AskUserQuestion|ExitPlanMode)
    // busy re-affirmation that exists specifically to clear it lands on
    // `busy_transitioned == false`. Without `currently_awaiting`, this is
    // exactly `tuic_state_busy_without_transition_yields_no_userinput` above —
    // the fix is that a session that IS currently awaiting must still get its
    // clear, edge or no edge.
    match tuic_state_awaiting_event("busy", 42, false, true, None, None, false) {
        Some(ParsedEvent::UserInput { content, line }) => {
            assert_eq!(content, "", "busy clear must not overwrite last_prompt");
            assert_eq!(line, 42, "busy UserInput must carry the prompt row");
        }
        other => panic!(
            "a currently-awaiting session must still get its clear on a redundant \
             busy re-affirmation, got {other:?}"
        ),
    }
}

/// Regression, live-reproduced 2026-09-01 on two real unattended/auto-approve
/// Claude sessions (`commerce-journal`'s `publish` tab, `agent-tooling-
/// analysis`'s `tool-scan` tab): a confident question — correctly confident,
/// a genuine `AskUserQuestion` — stuck `true` through full task completion.
///
/// `confident_awaiting_is_never_retracted` above proves the silence-timer
/// backstop correctly refuses to touch a confident question. This test proves
/// the OTHER side stays reachable: `claude_hook_map()`'s `PostToolUse
/// (AskUserQuestion|ExitPlanMode)` busy re-affirmation — kept specifically to
/// clear a confident question afterward — must still clear it even when it
/// fires mid-turn with no real shell idle↔busy edge (the ordinary case,
/// since `PreToolUse(AskUserQuestion)`'s `awaiting` override never touches
/// the shell busy/idle bit at all). Drives the real `tuic_state_awaiting_event`
/// output through the real accumulator, end to end.
#[tokio::test(flavor = "current_thread")]
async fn confident_awaiting_still_clears_on_a_non_edge_busy_reaffirmation() {
    let state = accumulating_state("s1");
    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
        session_id: "s1".to_string(),
        parsed: serde_json::json!({
            "type": "question",
            "prompt_text": "",
            "confident": true,
        })
        .into(),
    });
    assert!(
        await_session(&state, "s1", |s| s.awaiting_input && s.question_confident).await,
        "PreToolUse(AskUserQuestion)'s state=awaiting must badge the tab confidently"
    );

    // PostToolUse(AskUserQuestion|ExitPlanMode)'s busy re-affirmation, with no
    // real shell edge (busy_transitioned = false) — exactly the mid-turn case.
    // `currently_awaiting = true` is the 2026-09-01 fix.
    let clear_event = tuic_state_awaiting_event("busy", 0, false, true, None, None, false)
        .expect("a currently-awaiting session must still get its clear");
    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
        session_id: "s1".to_string(),
        parsed: serde_json::to_value(&clear_event)
            .expect("serialisable")
            .into(),
    });
    assert!(
        await_session(&state, "s1", |s| !s.awaiting_input && !s.question_confident).await,
        "a confident question must clear on the PostToolUse busy re-affirmation \
         even with no real idle↔busy edge — before the fix, this event was never \
         even emitted (tuic_state_awaiting_event returned None), which is exactly \
         how the badge stuck through full task completion on both live sessions"
    );
}

#[test]
fn attention_level_for_value_maps_every_known_iterm2_value() {
    assert_eq!(
        attention_level_for_value("no"),
        Some(AttentionLevel::Cancel)
    );
    assert_eq!(
        attention_level_for_value("once"),
        Some(AttentionLevel::Informational)
    );
    assert_eq!(
        attention_level_for_value("yes"),
        Some(AttentionLevel::Critical)
    );
    // "fireworks" has no direct Tauri equivalent — mapped to the same
    // continuous-bounce behavior as "yes".
    assert_eq!(
        attention_level_for_value("fireworks"),
        Some(AttentionLevel::Critical)
    );
}

#[test]
fn attention_level_for_value_ignores_unrecognized_values() {
    // Unreachable in production — Term::request_attention already
    // validates to one of the four known values — but the function must
    // still fail safe rather than panic or guess.
    assert_eq!(attention_level_for_value(""), None);
    assert_eq!(attention_level_for_value("YES"), None);
    assert_eq!(attention_level_for_value("bogus"), None);
}

#[test]
fn test_chunk_processor_osc1337_open_url_queues_pending_url() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "test-cp-osc1337-open-url";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));

    let mut cp = ChunkProcessor::new(None, None);
    let mut utf8_buf = Utf8ReadBuffer::new();
    let mut esc_buf = EscapeAwareBuffer::new();

    // base64("https://example.com") == "aHR0cHM6Ly9leGFtcGxlLmNvbQ=="
    let utf8_data = utf8_buf.push(b"\x1b]1337;OpenURL=:aHR0cHM6Ly9leGFtcGxlLmNvbQ==\x07");
    let esc_data = esc_buf.push(&utf8_data);
    let _ = cp.process_chunk(&esc_data, &silence, sid, state.as_ref());

    assert_eq!(
        cp.pending_open_urls,
        vec!["https://example.com".to_string()]
    );
}

#[test]
fn test_chunk_processor_osc1337_steal_focus_and_request_attention_do_not_panic() {
    // No real Tauri AppHandle in a unit test — this exercises the
    // config-gate + app_handle-is-None no-op path, and mainly guards
    // against a panic while wiring TermEvent::RequestFocus/RequestAttention
    // through process_chunk.
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "test-cp-osc1337-focus-attention";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
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

    feed(&mut cp, b"\x1b]1337;StealFocus\x07");
    feed(&mut cp, b"\x1b]1337;RequestAttention=fireworks\x07");
    feed(&mut cp, b"\x1b]1337;RequestAttention=no\x07");

    // config gate off must also stay panic-free.
    state.config.write().osc1337_focus_attention = false;
    feed(&mut cp, b"\x1b]1337;StealFocus\x07");
}

#[cfg(unix)]
fn insert_idle_shell_session(state: &AppState, session_id: &str) {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    // No args, so the shell itself stays the foreground process group leader —
    // a confirmed, non-flaky "sh" match, unlike a spawned grandchild.
    let command = CommandBuilder::new("/bin/sh");
    let child = pair.slave.spawn_command(command).expect("spawn shell");
    let child_pid = child.process_id().expect("spawned child has a pid");

    // Poll the RAW pgid AND name directly (not through
    // `get_session_foreground_process_impl`, whose returned `effective` is
    // trivially `None` before any `session_states` entry exists regardless
    // of whether the OS state has actually settled — that false signal is
    // exactly what let this race slip through once already). Two distinct
    // startup races were reproduced empirically here, and both must clear
    // before any test code touches `session_states`:
    //   1. Immediately after `spawn_command`, before the forked child
    //      finishes `setsid`+`TIOCSCTTY`, `tcgetpgrp` on a brand new pty can
    //      transiently report the *calling* (test) process's own pgid
    //      instead of the child's.
    //   2. Even once the pgid correctly matches the child, the child is
    //      briefly still running a fork()'d copy of the *parent's own
    //      binary image* until its `execve("/bin/sh", …)` actually
    //      completes — `proc_pidpath` on that pid reports the parent's own
    //      path (e.g. the test binary) right up until exec replaces it, at
    //      which point it flips to "sh"/"bash" (macOS resolves `/bin/sh`'s
    //      canonical path through to bash).
    let mut settled = false;
    for _ in 0..150 {
        settled = pair.master.process_group_leader() == Some(child_pid as i32)
            && matches!(
                process_name_from_pid(child_pid).as_deref(),
                Some("sh") | Some("bash")
            );
        if settled {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    // Fail loudly on a timeout rather than silently falling through with
    // stale fg data — an assertion mismatch further down would otherwise
    // look like a logic regression instead of what it actually is: this
    // environment took longer than 3s to settle a freshly spawned shell.
    assert!(
        settled,
        "pty for {session_id:?} never settled to a confirmed foreground \
         shell within 3s (pgid={:?}, name={:?}) — environment timing issue, \
         not a logic regression",
        pair.master.process_group_leader(),
        process_name_from_pid(child_pid)
    );

    state.session_maps.sessions.insert(
        session_id.to_string(),
        Mutex::new(PtySession {
            writer: Arc::new(Mutex::new(Box::new(RecordingWriter {
                bytes: Arc::new(std::sync::Mutex::new(Vec::new())),
            }) as Box<dyn std::io::Write + Send>)),
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

/// Polls `get_session_foreground_process_impl` until `session_states`
/// converges to a stable value (same `agent_type` observed twice in a
/// row), then returns that settled entry. Foreground-pgid sampling right
/// after `spawn_command` is a real, reproduced startup race (job control
/// hasn't necessarily settled yet), and a transient failure inside the
/// function's own `?` chain returns `None` from a completely different
/// code path than the "confirmed shell" clearing branch — so a test must
/// not trust a single call's return value or break on the first `None`
/// it happens to see. Polling `session_states` (the actual thing under
/// test) to a stable fixed point sidesteps both problems.
#[cfg(unix)]
fn settle_agent_type(state: &AppState, session_id: &str) -> Option<String> {
    let mut last = None;
    for _ in 0..200 {
        get_session_foreground_process_impl(state, session_id);
        let current = state
            .session_maps
            .session_states
            .get(session_id)
            .and_then(|e| e.agent_type.clone());
        if current == last {
            return current;
        }
        last = current;
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    last
}

/// A confirmed shell in the foreground, for a session that has actually
/// observed its agent running before (`agent_seen_running`), must clear
/// the sticky `session_states.agent_type` mirror — otherwise
/// `should_transition_idle_with_hook` keeps using the longer agent-idle
/// threshold for a session that no longer has an agent running, and the
/// frontend's `LastPromptBar`/`agentType` stay stuck until the longer
/// window finally elapses. See the "Prompt display ... does not disappear
/// right away" bug this regression test guards against.
#[cfg(unix)]
#[test]
fn get_session_foreground_process_clears_sticky_agent_type_on_confirmed_shell() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "confirmed-shell-clears-agent-type";
    insert_idle_shell_session(&state, session_id);
    state.session_maps.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("claude".into()),
            hook_instrumented: true,
            agent_seen_running: true,
            ..Default::default()
        },
    );

    let settled = settle_agent_type(&state, session_id);

    assert_eq!(
        settled, None,
        "confirmed shell foreground must clear the sticky agent_type mirror \
         once the agent has actually been seen running"
    );
    let entry = state.session_maps.session_states.get(session_id).unwrap();
    assert!(
        !entry.hook_instrumented,
        "hook_instrumented must be cleared alongside agent_type"
    );
    assert!(
        !entry.agent_seen_running,
        "agent_seen_running resets so a subsequent agent re-arms the gate"
    );
}

/// The exact regression a review caught in this fix: a session whose
/// `agent_type` is only a run-config *preset* (`PtyConfig::agent_type`,
/// for a custom/unrecognized launcher) — not yet confirmed by an actual
/// observed foreground process — must NOT have that preset wiped just
/// because the very first foreground read sees a plain shell. In
/// production this races the pending init command: `Terminal.tsx`'s first
/// `shell-state: idle` event fires `detectAgentForTerminal(id, "idle")`
/// (which calls this function) in the same tick it sends the init
/// command, so the shell is still confirmably in the foreground when this
/// runs — before the custom binary has even been exec'd, let alone
/// exited. `classify_agent` can never subsequently recognise a custom
/// binary, so wiping the preset here would be permanent and
/// unrecoverable. `agent_seen_running` (default `false`, unset by this
/// preset-only insert) is exactly what must prevent the clear.
#[cfg(unix)]
#[test]
fn get_session_foreground_process_preserves_unstarted_run_config_preset_on_confirmed_shell() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "unstarted-preset-not-cleared";
    insert_idle_shell_session(&state, session_id);
    state.session_maps.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("claude".into()),
            hook_instrumented: true,
            agent_seen_running: false,
            ..Default::default()
        },
    );

    let settled = settle_agent_type(&state, session_id);

    assert_eq!(
        settled,
        Some("claude".to_string()),
        "a run-config preset that was never confirmed running must survive \
         a confirmed-shell foreground — the custom launcher just hasn't exec'd yet"
    );
}

/// The sticky-preserve behavior must still hold for the actually-flaky
/// case: an *unrecognized* non-shell foreground (a short-lived grandchild
/// like `git`/`sed`/`rg`) must NOT clear a pre-set agent_type, since the
/// agent (e.g. claude) is still the live foreground owner moments later.
#[cfg(unix)]
#[test]
fn get_session_foreground_process_keeps_sticky_agent_type_on_unrecognized_non_shell() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "unrecognized-non-shell-stays-sticky";

    // Spawn `/bin/sleep` directly as the PTY's own child (no shell
    // wrapper), so it is the process group leader from the start —
    // an unrecognized, non-shell foreground, deterministically, unlike
    // relying on a shell's job-control timing to hand off to a grandchild.
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    let mut command = CommandBuilder::new("/bin/sleep");
    command.arg("30");
    let child = pair.slave.spawn_command(command).expect("spawn sleep");
    state.session_maps.sessions.insert(
        session_id.to_string(),
        Mutex::new(PtySession {
            writer: Arc::new(Mutex::new(Box::new(RecordingWriter {
                bytes: Arc::new(std::sync::Mutex::new(Vec::new())),
            }) as Box<dyn std::io::Write + Send>)),
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
    state.session_maps.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("claude".into()),
            hook_instrumented: true,
            agent_seen_running: true,
            ..Default::default()
        },
    );

    // Same startup-race concern as `insert_idle_shell_session` — poll to
    // a stable fixed point rather than trusting the first call.
    let settled = settle_agent_type(&state, session_id);

    assert_eq!(
        settled,
        Some("claude".to_string()),
        "unrecognized non-shell foreground must NOT clear the sticky agent_type mirror"
    );
}

/// The static `SHELLS` list can never be exhaustive (xonsh, elvish, ion,
/// murex, and anything else not on it) — a session's own recorded
/// `PtySession.shell` (set from `resolve_shell()` at PTY creation) must
/// independently confirm a foreground match, so an unlisted shell still
/// gets the confirmed-shell clear instead of reproducing the original
/// "stuck forever" bug for exactly the users that list doesn't cover.
/// Spawns `/bin/sleep` directly (a real, deterministic non-shell
/// foreground) with `session.shell` set to a path whose basename matches
/// what's actually running — proving the match logic works without
/// needing an exotic shell binary installed in CI.
#[cfg(unix)]
#[test]
fn get_session_foreground_process_clears_via_session_shell_when_unlisted() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "unlisted-shell-clears-via-session-shell";

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    let mut command = CommandBuilder::new("/bin/sleep");
    command.arg("30");
    let child = pair.slave.spawn_command(command).expect("spawn sleep");
    state.session_maps.sessions.insert(
        session_id.to_string(),
        Mutex::new(PtySession {
            writer: Arc::new(Mutex::new(Box::new(RecordingWriter {
                bytes: Arc::new(std::sync::Mutex::new(Vec::new())),
            }) as Box<dyn std::io::Write + Send>)),
            master: pair.master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            is_remote: false,
            // Not a real shell — the point is only that its basename
            // ("sleep") matches the actually-running process, which is
            // NOT on the static `SHELLS` list.
            shell: "/opt/homebrew/bin/sleep".to_string(),
        }),
    );
    state.session_maps.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("claude".into()),
            hook_instrumented: true,
            agent_seen_running: true,
            ..Default::default()
        },
    );

    let settled = settle_agent_type(&state, session_id);

    assert_eq!(
        settled, None,
        "a foreground matching session.shell's basename must clear agent_type \
         even though its name isn't on the static SHELLS list"
    );
}

/// The fallback path (unrecognized non-shell, resolved only via the
/// preset — `detected.is_none()`) is genuinely ambiguous: it can't tell
/// "the preset's own launcher" from "an intermediate wrapper hop." It
/// must require the foreground to persist across
/// `AGENT_SEEN_RUNNING_CONFIRM_MS` before confirming `agent_seen_running`,
/// so a wrapper that fails almost immediately (e.g. a failed `direnv
/// exec`) can't falsely confirm-then-strand the preset. A direct
/// `classify_agent` match has no such ambiguity and is covered by the
/// existing `..._clears_sticky_agent_type_on_confirmed_shell` test, which
/// presets `agent_seen_running: true` directly rather than exercising
/// this debounce.
#[cfg(unix)]
#[test]
fn get_session_foreground_process_debounces_ambiguous_confirmation() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "ambiguous-confirmation-debounced";

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    let mut command = CommandBuilder::new("/bin/sleep");
    command.arg("30");
    let child = pair.slave.spawn_command(command).expect("spawn sleep");
    state.session_maps.sessions.insert(
        session_id.to_string(),
        Mutex::new(PtySession {
            writer: Arc::new(Mutex::new(Box::new(RecordingWriter {
                bytes: Arc::new(std::sync::Mutex::new(Vec::new())),
            }) as Box<dyn std::io::Write + Send>)),
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
    state.session_maps.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("claude".into()),
            hook_instrumented: true,
            agent_seen_running: false,
            ..Default::default()
        },
    );

    // Retry until the ambiguous branch is genuinely reached — the same
    // startup race documented on `insert_idle_shell_session` (a freshly
    // spawned child's pgid/process-name can read unsettled for a moment)
    // applies here too; a single untried call could transiently see
    // `process_group_leader()` return `None` via the function's own
    // early `?`, which looks identical to "not yet ambiguous" from the
    // outside but isn't the thing under test. Retrying doesn't disturb
    // the debounce math below: the confirmation window is measured from
    // whenever `agent_seen_running_pending_since_ms` first gets set, not
    // from how many calls preceded that.
    let mut pending_set = false;
    for _ in 0..150 {
        let result = get_session_foreground_process_impl(&state, session_id);
        if result == Some("claude".to_string())
            && state
                .session_maps
                .session_states
                .get(session_id)
                .is_some_and(|e| e.agent_seen_running_pending_since_ms.is_some())
        {
            pending_set = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        pending_set,
        "the ambiguous observation must start the confirmation window"
    );
    assert!(
        !state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .agent_seen_running,
        "a single ambiguous observation must not confirm agent_seen_running yet"
    );

    std::thread::sleep(std::time::Duration::from_millis(
        AGENT_SEEN_RUNNING_CONFIRM_MS + 200,
    ));
    get_session_foreground_process_impl(&state, session_id);
    assert!(
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .agent_seen_running,
        "once the ambiguous foreground has persisted past the confirm window, \
         agent_seen_running must latch"
    );
}

/// If the shell reclaims the foreground *before* an ambiguous
/// confirmation window (`AGENT_SEEN_RUNNING_CONFIRM_MS`) elapses, the
/// pending timestamp must reset rather than surviving into a later,
/// unrelated episode — otherwise a stale `agent_seen_running_pending_since_ms`
/// could let a much later ambiguous observation confirm immediately
/// (using an old timestamp) instead of restarting its own window.
#[cfg(unix)]
#[test]
fn get_session_foreground_process_resets_pending_window_on_shell_reclaim() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "pending-window-resets-on-shell-reclaim";
    insert_idle_shell_session(&state, session_id);
    state.session_maps.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("claude".into()),
            hook_instrumented: true,
            agent_seen_running: false,
            // Simulates a wrapper hop observed moments ago, still well
            // within the confirmation window.
            agent_seen_running_pending_since_ms: Some(now_epoch_ms()),
            ..Default::default()
        },
    );

    let mut settled = false;
    for _ in 0..150 {
        get_session_foreground_process_impl(&state, session_id);
        if state
            .session_maps
            .session_states
            .get(session_id)
            .is_some_and(|e| e.agent_seen_running_pending_since_ms.is_none())
        {
            settled = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    assert!(
        settled,
        "a confirmed shell must reset a pending ambiguous-confirmation window"
    );
    let entry = state.session_maps.session_states.get(session_id).unwrap();
    assert!(
        !entry.agent_seen_running,
        "the window was interrupted before confirming — must not have latched"
    );
    assert_eq!(
        entry.agent_type,
        Some("claude".to_string()),
        "the preset itself is untouched — only the pending bookkeeping resets, \
         since agent_seen_running never confirmed"
    );
}

/// OSC 133's own prompt marker (`'A'`, `hook_state = false`) can only
/// fire once the real shell redraws its prompt — it cannot fire while any
/// foreground child still owns the terminal — so it's a reliable,
/// immediate "the agent has genuinely exited" signal and must clear
/// `agent_type` right away, without waiting for the next
/// `get_session_foreground_process` poll. OSC 7770's `state=idle`
/// (`hook_state = true`) means the hook-instrumented agent finished this
/// turn and is waiting for the next prompt WHILE THE SAME PROCESS STAYS
/// ALIVE — clearing there would wipe `agent_type` on every ordinary turn
/// boundary, not just on exit, so it must NOT clear.
#[test]
fn osc133_idle_clears_agent_type_but_osc7770_idle_does_not() {
    let state = crate::state::tests_support::make_test_app_state();

    let osc133_session = "osc133-idle-clears-agent-type";
    state.session_maps.shell_states.insert(
        osc133_session.into(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state.session_maps.session_states.insert(
        osc133_session.into(),
        crate::state::SessionState {
            agent_type: Some("claude".into()),
            hook_instrumented: true,
            agent_seen_running: true,
            ..Default::default()
        },
    );
    let transitioned =
        transition_explicit_shell_state(&state, osc133_session, SHELL_IDLE, "idle", false);
    assert!(
        transitioned,
        "BUSY->IDLE must be a real edge given the atomic starts at BUSY"
    );
    let entry = state
        .session_maps
        .session_states
        .get(osc133_session)
        .unwrap();
    assert_eq!(
        entry.agent_type, None,
        "OSC 133 idle (hook_state=false) must clear agent_type immediately"
    );
    assert!(!entry.agent_seen_running);
    drop(entry);

    let osc7770_session = "osc7770-idle-does-not-clear-agent-type";
    state.session_maps.shell_states.insert(
        osc7770_session.into(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state.session_maps.session_states.insert(
        osc7770_session.into(),
        crate::state::SessionState {
            agent_type: Some("claude".into()),
            hook_instrumented: true,
            agent_seen_running: true,
            ..Default::default()
        },
    );
    let transitioned =
        transition_explicit_shell_state(&state, osc7770_session, SHELL_IDLE, "idle", true);
    assert!(transitioned);
    let entry = state
        .session_maps
        .session_states
        .get(osc7770_session)
        .unwrap();
    assert_eq!(
        entry.agent_type,
        Some("claude".to_string()),
        "OSC 7770 idle (hook_state=true) must NOT clear agent_type — it means \
         the agent finished a turn, not that the agent process exited"
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
    let dir = tempfile::TempDir::new().expect("temp dir (not a git repo)");
    let cwd = dir.path().to_string_lossy().to_string();
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
    state
        .session_maps
        .silence_states
        .insert(child_id.to_string(), silence);

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
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
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
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
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

    assert_eq!(
        state
            .session_maps
            .session_states
            .get(child_id)
            .unwrap()
            .turn_epoch,
        1
    );
    let silence = state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .clone();
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
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .turn_epoch,
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
    state
        .session_maps
        .silence_states
        .insert(session_id.to_string(), silence);

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
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
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
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
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
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .turn_epoch,
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
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
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
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
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
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
    silence.lock().set_declared_background_work(true, 0);
    let mut processor = ChunkProcessor::new(None, None);

    processor.process_chunk("\x1b]7770;bgtasks=\x07", &silence, session_id, &state);
    assert!(!silence.lock().declared_background_work_for_epoch(0));
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
