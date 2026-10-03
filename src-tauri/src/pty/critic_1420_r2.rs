//! Round-2 critic tests for story 1420-f3de. Child of `pty` so they can read
//! `SilenceState` offsets that no public accessor exposes.

use super::*;
use crate::state::VtLogBuffer;
use crate::test_support::ForegroundIdentityProbe;

#[cfg(unix)]
fn discovered_claude_probe(sid: &str, screen: &[&str]) -> (Arc<AppState>, ForegroundIdentityProbe) {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let probe = ForegroundIdentityProbe::new(state.clone(), sid, "claude");
    let mut vt = VtLogBuffer::new(24, 80, 1000);
    vt.process(screen.join("\r\n").as_bytes());
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(vt));
    let mut ring = OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY);
    ring.write(b"already-seen output");
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(ring));
    (state, probe)
}

/// Catches: identity discovery on a quiet WORKING screen caches the verdict
/// but records no offset, so `fresh_working_transition` (submit ack) never
/// sees Ready-then-Working and a submit into a working agent is mis-acked.
#[cfg(unix)]
#[test]
fn discovery_on_a_quiet_working_screen_records_the_working_offset() {
    let sid = "critic-1420r2-working";
    let (state, _probe) =
        discovered_claude_probe(sid, &["✻ Cogitating… (3m 47s · ↓ 2.2k tokens)", "", "❯"]);
    assert_eq!(
        refresh_session_agent(&state, sid).as_deref(),
        Some("claude")
    );
    let total = state
        .session_maps
        .output_buffers
        .get(sid)
        .unwrap()
        .lock()
        .total_written;
    let silence = state.session_maps.silence_states.get(sid).unwrap();
    let silence = silence.lock();
    assert_eq!(silence.cached_screen_activity, AgentScreenActivity::Working);
    assert_eq!(silence.last_working_screen_offset, total);
    assert_eq!(silence.last_ready_screen_offset, 0);
}

/// Catches: the same for a quiet READY screen (offset left at 0, so a later
/// submit offset compares as "Ready happened before").
#[cfg(unix)]
#[test]
fn discovery_on_a_quiet_ready_screen_records_the_ready_offset() {
    let sid = "critic-1420r2-ready";
    let (state, _probe) = discovered_claude_probe(sid, &["done", "", "❯"]);
    assert_eq!(
        refresh_session_agent(&state, sid).as_deref(),
        Some("claude")
    );
    let total = state
        .session_maps
        .output_buffers
        .get(sid)
        .unwrap()
        .lock()
        .total_written;
    let silence = state.session_maps.silence_states.get(sid).unwrap();
    let silence = silence.lock();
    assert_eq!(silence.cached_screen_activity, AgentScreenActivity::Ready);
    assert_eq!(silence.last_ready_screen_offset, total);
}

/// Catches: a repeated refresh (timer tick every second plus HTTP polls) with
/// no identity change re-records the offset, moving "Ready at" forward past a
/// submit that happened in between.
#[cfg(unix)]
#[test]
fn repeat_refresh_does_not_move_the_recorded_offset() {
    let sid = "critic-1420r2-repeat";
    let (state, _probe) = discovered_claude_probe(sid, &["done", "", "❯"]);
    refresh_session_agent(&state, sid);
    let first = state
        .session_maps
        .silence_states
        .get(sid)
        .unwrap()
        .lock()
        .last_ready_screen_offset;
    state
        .session_maps
        .output_buffers
        .get(sid)
        .unwrap()
        .lock()
        .write(b"later bytes");
    refresh_session_agent(&state, sid);
    let second = state
        .session_maps
        .silence_states
        .get(sid)
        .unwrap()
        .lock()
        .last_ready_screen_offset;
    assert_eq!(first, second);
}

/// Catches: a discovered agent that has exited back to a shell stays
/// `agent_type=Some` forever, so automated submit/mail writes text into a bare
/// shell prompt instead of being rejected `not_managed_agent`. (Policy
/// question for the coordinator: preset run-config sessions also show a shell
/// foreground while the shell boots, so clearing must not touch them.)
#[cfg(unix)]
#[test]
fn shell_foreground_after_agent_exit_is_not_submittable() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "critic-1420r2-stale";
    let probe = ForegroundIdentityProbe::new(state.clone(), sid, "bash");
    // What a previous refresh left behind while claude was foreground.
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("claude".into());
    assert_eq!(refresh_session_agent(&state, sid), None);
    assert!(
        matches!(
            write_agent_submission_to_pty(&state, sid, "rm -rf scratch"),
            AgentSubmissionWrite::Rejected {
                reason: "not_managed_agent",
                ..
            }
        ),
        "stale agent_type let a submit through to a shell foreground; bytes={:?}",
        probe.bytes.lock().unwrap()
    );
}

/// Catches: the `claude/versions/<n>` layout is honoured by the macOS path
/// lookup only; Linux `/proc/<pid>/comm` yields `2.1.5`, so a claude started
/// by its versioned path is never classified on the platform the story is about.
/// Negatives: non-numeric leaf, wrong parent, or missing `versions` segment
/// must stay unclassified.
#[cfg(unix)]
#[test]
fn versioned_claude_layout_is_classified_and_lookalikes_are_not() {
    use std::process::{Command, Stdio};
    let scratch = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let run = |rel: &str| -> Option<&'static str> {
        let exe = scratch.path().join(rel);
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::copy("/bin/cat", &exe).unwrap();
        #[cfg(target_os = "macos")]
        assert!(
            Command::new("/usr/bin/codesign")
                .args(["--force", "--sign", "-"])
                .arg(&exe)
                .status()
                .unwrap()
                .success()
        );
        let mut child = Command::new(&exe).stdin(Stdio::piped()).spawn().unwrap();
        let mut name = None;
        for _ in 0..200 {
            name = process_name_from_pid(child.id());
            if name.as_deref().is_some_and(|n| n != "cat") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        classify_agent(&name.expect("process name"))
    };
    assert_eq!(run("claude/versions/2.1.5"), Some("claude"));
    assert_eq!(run("claude/versions/latest"), None);
    assert_eq!(run("xclaude/versions/2.1.5"), None);
    assert_eq!(run("claude/other/2.1.5"), None);
    assert_eq!(run("claude/versions/x/2.1.5"), None);
}
