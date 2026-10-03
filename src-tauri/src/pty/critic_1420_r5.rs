//! Round-5 critic test for story 1420-f3de: shells missing from the shell list.

use super::*;
use crate::test_support::ForegroundIdentityProbe;

type Flags = (Option<String>, bool, bool);

/// Run one `refresh_session_agent` per foreground name, carrying the identity
/// provenance across the replaced probe PTYs. Returns the stored flags after
/// each step.
#[cfg(unix)]
fn run(sid: &str, initial: Flags, foregrounds: &[&str]) -> Vec<Flags> {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let mut carried = initial;
    let mut out = Vec::new();
    for name in foregrounds {
        let probe = ForegroundIdentityProbe::new(state.clone(), sid, name);
        {
            let mut s = state.session_maps.session_states.get_mut(sid).unwrap();
            s.agent_type = carried.0.clone();
            s.agent_type_from_run_config = carried.1;
            s.agent_foreground_observed = carried.2;
        }
        refresh_session_agent(&state, sid);
        let s = state.session_maps.session_states.get(sid).unwrap();
        carried = (
            s.agent_type.clone(),
            s.agent_type_from_run_config,
            s.agent_foreground_observed,
        );
        drop(s);
        out.push(carried.clone());
        drop(probe);
    }
    out
}

/// Catches: a login shell missing from `SHELLS` (busybox/Alpine `ash`, common
/// on remote Linux) is read as a non-shell helper, so the discovered agent's
/// identity is retained forever after exit and the returned shell is submittable.
#[cfg(unix)]
#[test]
fn busybox_ash_after_an_agent_revokes_identity() {
    let steps = run(
        "critic-1420r5-ash",
        (None, false, false),
        &["claude", "ash"],
    );
    assert_eq!(
        steps[1].0, None,
        "ash treated as a helper; agent identity stuck"
    );
}
