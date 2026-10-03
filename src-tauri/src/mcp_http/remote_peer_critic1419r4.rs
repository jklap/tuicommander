//! Round 4 security critic tests for story 1419-ab18: host quota boundary,
//! LRU eviction of departed senders, liveness during eviction, accounting.

use super::*;
use crate::mcp_http::tests::test_state;

fn message(id: &str, from: &str) -> crate::state::AgentMessage {
    crate::state::AgentMessage {
        id: id.to_string(),
        from_tuic_session: from.to_string(),
        from_name: from.to_string(),
        content: "one lifecycle notice".to_string(),
        timestamp: 0,
        delivered_via_channel: false,
    }
}

fn seed(state: &AppState, sender: &str, count: usize) {
    for n in 0..count {
        assert_eq!(
            record_forwarded(
                state,
                &format!("{sender}-r{}", n / 100),
                &message(&format!("{sender}-{n}"), sender)
            ),
            Ok(true)
        );
    }
}

// Catches: the sender being admitted is its own eviction victim, so pressure
// erases the replay windows of the very sender whose message is being queued and
// its earlier ids are delivered again.
#[test]
fn the_admitted_sender_never_evicts_its_own_replay_windows() {
    let state = test_state();
    seed(&state, "mint/s", MAX_HOST_RECORDS);
    let admitted = record_forwarded(&state, "fresh", &message("s-new", "mint/s"));
    let replay = record_forwarded(&state, "mint/s-r0", &message("mint/s-0", "mint/s"));
    assert!(
        admitted.is_err() || replay == Ok(false),
        "admitted={admitted:?} replay={replay:?}: a message was queued while its sender's old id became replayable"
    );
}
