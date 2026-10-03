//! Round-3 critic tests for story 1420-f3de: provenance of `agent_type`.

use super::*;
use crate::test_support::ForegroundIdentityProbe;

#[cfg(unix)]
fn set_identity(state: &AppState, sid: &str, agent: Option<&str>, from_run_config: bool) {
    let mut s = state.session_maps.session_states.get_mut(sid).unwrap();
    s.agent_type = agent.map(str::to_string);
    s.agent_type_from_run_config = from_run_config;
}

#[cfg(unix)]
fn identity(state: &AppState, sid: &str) -> Option<String> {
    state
        .session_maps
        .session_states
        .get(sid)
        .unwrap()
        .agent_type
        .clone()
}

/// Catches: a discovered agent loses its identity whenever the foreground pgid
/// briefly points at an unrecognised NON-shell program (a `git`/`rg` child of
/// the agent). Revocation must need positive evidence of a shell, not mere
/// absence of an agent name; otherwise `suggest:`/`intent:` parsing and submit
/// flap off while the agent is still alive.
#[cfg(unix)]
#[test]
fn discovered_agent_survives_a_transient_unrecognised_non_shell_foreground() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "critic-1420r3-transient";
    let _probe = ForegroundIdentityProbe::new(state.clone(), sid, "git");
    set_identity(&state, sid, Some("claude"), false);
    refresh_session_agent(&state, sid);
    assert_eq!(
        identity(&state, sid).as_deref(),
        Some("claude"),
        "a transient non-shell foreground revoked a live agent's identity"
    );
}

/// Catches: a run-config preset (claude) overwritten by a hand-launched,
/// different discovered agent (codex) keeps `agent_type_from_run_config=true`,
/// so codex becomes unrevocable: after it exits back to a shell the session
/// still reads `codex` and unattended submit/mail write into the shell.
#[cfg(unix)]
#[test]
fn discovered_agent_replacing_a_preset_is_still_revocable() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "critic-1420r3-replace";
    let probe = ForegroundIdentityProbe::new(state.clone(), sid, "codex");
    set_identity(&state, sid, Some("claude"), true);
    assert_eq!(
        refresh_session_agent(&state, sid).as_deref(),
        Some("codex")
    );
    let flag = state
        .session_maps
        .session_states
        .get(sid)
        .unwrap()
        .agent_type_from_run_config;
    drop(probe);
    let _shell = ForegroundIdentityProbe::new(state.clone(), sid, "bash");
    set_identity(&state, sid, Some("codex"), flag);
    refresh_session_agent(&state, sid);
    assert_eq!(
        identity(&state, sid),
        None,
        "discovered codex stayed pinned by the replaced preset's provenance"
    );
}

/// Catches: Linux `/proc/<pid>/exe` of a running native Claude whose version
/// file was replaced by the updater reads `.../claude/versions/2.1.5 (deleted)`;
/// the numeric-leaf check rejects the suffix and the agent is never classified.
#[test]
fn deleted_versioned_claude_exe_is_still_classified() {
    assert_eq!(
        classify_agent_name_or_path("/home/u/.local/share/claude/versions/2.1.5 (deleted)"),
        Some("claude")
    );
}
