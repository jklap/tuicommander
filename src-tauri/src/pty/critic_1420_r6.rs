//! Round-6 critic tests for story 1420-f3de: spawn-root role contract.

use super::*;
use crate::test_support::ForegroundIdentityProbe;

/// Catches: a Shell-role session whose root process was `exec`'d into the
/// agent (`exec claude`, same pid, no job-control child) is read as "foreground
/// == root pid, so the shell returned": the live agent is never detected and
/// its identity is revoked, so submit/mail and state parsing are refused for a
/// running agent.
#[cfg(unix)]
#[test]
fn shell_root_exec_ed_into_an_agent_is_still_detected() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "critic-1420r6-exec-root";
    let _probe = ForegroundIdentityProbe::shell_root(state.clone(), sid, "claude");
    let returned = refresh_session_agent(&state, sid);
    let stored = state
        .session_maps
        .session_states
        .get(sid)
        .unwrap()
        .agent_type
        .clone();
    assert_eq!(
        (returned.as_deref(), stored.as_deref()),
        (Some("claude"), Some("claude")),
        "a Shell-role root running claude was treated as a returned shell"
    );
}
