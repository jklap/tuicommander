//! Round-2 critic cases for 1423: what the shared injection worker costs other
//! sessions now that notices wait for submit confirmation.
use super::*;
use crate::state::VtLogBuffer;
use crate::test_support::{agent_session, insert_recording_session};
use parking_lot::Mutex;

const CODEX_READY: &[u8] = b"\x1b[22;1H\xe2\x80\xba Ask Codex to do anything";

/// Catches: a lifecycle/mail notice to a SILENT agent parking the single FIFO
/// `tuic-injection` thread for the whole confirmation window (6 s for Codex and
/// Claude). Every other session's notice, queued behind it on the same worker,
/// is then late by that window; `flush_pending_injections` already moved to a
/// worker per session for this reason.
#[cfg(unix)]
#[test]
fn silent_agent_notice_does_not_hold_the_shared_injection_worker() {
    let sid = "critic2-silent-codex-parent";
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    agent_session(&state, sid, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("codex".into());
    let mut vt = VtLogBuffer::new(24, 80, 1000);
    vt.process(CODEX_READY);
    state.grid.vt_log_buffers.insert(sid.into(), Mutex::new(vt));
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), Mutex::new(OutputRingBuffer::new(1 << 16)));
    let bytes = insert_recording_session(&state, sid);

    let worker_state = std::sync::Arc::clone(&state);
    spawn_injection_job(move || {
        let _ = deliver_notice_to_managed_pty(&worker_state, sid, "[TUIC] child is now idle");
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while bytes.lock().unwrap().last() != Some(&b'\r') {
        assert!(
            std::time::Instant::now() < deadline,
            "notice never wrote Enter"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    let started = std::time::Instant::now();
    wait_for_injection_queue();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "another session's job waited {:?} behind a silent agent's confirmation window",
        started.elapsed()
    );
}
