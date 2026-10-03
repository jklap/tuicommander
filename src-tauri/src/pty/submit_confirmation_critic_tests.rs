//! Submit-confirmation gaps found by the 1423 path audit. Each test names the
//! plausible bug it catches; none depends on how the fix is built, only on what a
//! caller or the user observes (toast, uncertainty flag, number of Enters).
use super::*;
use crate::state::VtLogBuffer;
use crate::test_support::{agent_session, insert_recording_session};
use parking_lot::Mutex;

const CODEX_READY: &[u8] = b"\x1b[22;1H\xe2\x80\xba Ask Codex to do anything";

/// An idle agent session with a recording PTY, a VT screen holding `screen`
/// and an output ring, as the queued-delivery tests build it.
fn idle_agent(
    agent_type: &str,
    sid: &str,
    screen: &[u8],
) -> (
    std::sync::Arc<AppState>,
    std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
) {
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    agent_session(&state, sid, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some(agent_type.into());
    let mut vt = VtLogBuffer::new(24, 80, 1000);
    vt.process(screen);
    state.grid.vt_log_buffers.insert(sid.into(), Mutex::new(vt));
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), Mutex::new(OutputRingBuffer::new(1 << 16)));
    let bytes = insert_recording_session(&state, sid);
    (state, bytes)
}

/// Block until the injection's final Enter has reached the PTY.
fn wait_for_enter(bytes: &std::sync::Mutex<Vec<u8>>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while bytes.lock().unwrap().last() != Some(&b'\r') {
        assert!(
            std::time::Instant::now() < deadline,
            "injection never wrote Enter"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn confirmation_toasts(
    alerts: &mut tokio::sync::broadcast::Receiver<crate::state::AppEvent>,
) -> usize {
    std::iter::from_fn(|| alerts.try_recv().ok())
        .filter(|event| {
            matches!(event, crate::state::AppEvent::McpToast { title, .. }
                if title == "Agent input was not confirmed")
        })
        .count()
}

fn enters(bytes: &std::sync::Mutex<Vec<u8>>) -> usize {
    bytes
        .lock()
        .unwrap()
        .iter()
        .filter(|b| **b == b'\r')
        .count()
}

/// Catches: confirmation that reads only the CURRENT screen. Codex accepts the
/// lifecycle wake, shows Working and finishes before the poller looks (fast turn,
/// or a Mac busy with a build): the screen is Ready again and a real submission
/// is reported as "not confirmed".
#[cfg(unix)]
#[test]
fn queued_codex_turn_that_already_finished_when_polled_is_confirmed() {
    let sid = "critic-codex-finished-turn";
    let (state, bytes) = idle_agent("codex", sid, CODEX_READY);
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let mut alerts = state.event_bus.subscribe();

    std::thread::scope(|scope| {
        scope.spawn(|| enqueue_user_command(&state, sid, "wake the agent").unwrap());
        wait_for_enter(&bytes);
        let mut reader = ChunkProcessor::new(None, None);
        reader.process_chunk(
            "\x1b[21;1H\u{2022} Working (1s \u{2022} esc to interrupt)",
            &silence,
            sid,
            &state,
        );
        reader.process_chunk(
            "\x1b[21;1H\x1b[2K\x1b[22;1H\x1b[2K\u{203a} Ask Codex to do anything",
            &silence,
            sid,
            &state,
        );
    });

    assert_eq!(confirmation_toasts(&mut alerts), 0, "false failure toast");
    assert!(!silence.lock().injection_delivery_uncertain);
    assert_eq!(
        enters(&bytes),
        1,
        "a turn that ran must not get a second Enter"
    );
}

/// Catches: a Claude hook busy that is gone again (Stop hook ran) before the
/// poller reads `busy_source_is("hook-busy")`. The pair arrives in one chunk so
/// the test does not race the poller. The idle hook is the recorded busy hook
/// with only its state word changed; the framing is the recorded one.
#[cfg(unix)]
#[test]
fn queued_claude_turn_whose_hook_busy_already_ended_is_confirmed() {
    let sid = "critic-claude-finished-turn";
    let (state, bytes) = idle_agent("claude", sid, b"");
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let mut alerts = state.event_bus.subscribe();
    let capture = String::from_utf8(
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/fixtures/agent_prompts/claude-hooked-missing-question-20260921.raw"
        ))
        .expect("recorded Claude hook stream"),
    )
    .expect("UTF-8");
    let busy = capture.find("state=busy").expect("captured busy hook");
    let start = capture[..busy].rfind('\x1b').expect("hook start");
    let end = busy + capture[busy..].find("\x1b\\").expect("hook end") + 2;
    let busy_hook = &capture[start..end];
    let idle_hook = busy_hook.replacen("state=busy", "state=idle", 1);
    let both = format!("{busy_hook}{idle_hook}");

    std::thread::scope(|scope| {
        scope.spawn(|| enqueue_user_command(&state, sid, "wake the agent").unwrap());
        wait_for_enter(&bytes);
        let mut reader = ChunkProcessor::new(None, None);
        reader.process_chunk(&both, &silence, sid, &state);
    });

    assert_eq!(confirmation_toasts(&mut alerts), 0, "false failure toast");
    assert!(!silence.lock().injection_delivery_uncertain);
}

/// Catches: the idle-claim fast path (lifecycle wake, mail wake, urgent notice,
/// voice) reporting Submitted the moment Enter is flushed. A Codex whose Enter
/// was swallowed (child silent, text still in the composer) is then treated as
/// delivered: the claim is committed as a submitted turn and the orchestrator's
/// wake cursor advances, with nothing left to surface the lost notice.
#[cfg(unix)]
#[test]
fn notice_to_idle_codex_that_never_reacts_is_uncertain() {
    let sid = "critic-notice-silent-codex";
    let (state, _bytes) = idle_agent("codex", sid, CODEX_READY);
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();

    deliver_notice_to_managed_pty(&state, sid, "[TUIC] child agent 5d0dbc39 is now idle");

    assert!(
        silence.lock().injection_delivery_uncertain,
        "a silent child cannot have confirmed a notice written on the idle fast path"
    );
}

/// Catches: `composer_retains_text` matching the submitted text anywhere on the
/// screen. Once Codex has accepted the turn its transcript echoes `› <text>`
/// above an EMPTY composer; reading that echo as "still in the composer" sends a
/// second bare Enter into a live agent, the duplicate submission the toast text
/// warns about.
#[cfg(unix)]
#[test]
fn codex_transcript_echo_above_empty_composer_gets_no_second_enter() {
    let sid = "critic-codex-transcript-echo";
    let (state, bytes) = idle_agent("codex", sid, CODEX_READY);
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();

    std::thread::scope(|scope| {
        scope.spawn(|| enqueue_user_command(&state, sid, "wake the agent").unwrap());
        wait_for_enter(&bytes);
        let mut reader = ChunkProcessor::new(None, None);
        reader.process_chunk(
            "\x1b[2J\x1b[10;1H\u{203a} wake the agent\x1b[22;1H\u{203a} Ask Codex to do anything",
            &silence,
            sid,
            &state,
        );
    });

    assert_eq!(
        enters(&bytes),
        1,
        "text in the transcript is not text in the composer"
    );
}
