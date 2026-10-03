//! Round-4 critic tests for story 1420-f3de: preset arming and revocation.

use super::*;
use crate::test_support::ForegroundIdentityProbe;

#[cfg(unix)]
fn flags(state: &AppState, sid: &str) -> (Option<String>, bool, bool) {
    let s = state.session_maps.session_states.get(sid).unwrap();
    (
        s.agent_type.clone(),
        s.agent_type_from_run_config,
        s.agent_foreground_observed,
    )
}

#[cfg(unix)]
fn restore(state: &AppState, sid: &str, f: (Option<String>, bool, bool)) {
    let mut s = state.session_maps.session_states.get_mut(sid).unwrap();
    s.agent_type = f.0;
    s.agent_type_from_run_config = f.1;
    s.agent_foreground_observed = f.2;
}

/// Catches: a configured wrapper whose process name `classify_agent` does not
/// recognise (alias/script/symlink) is never "observed", so the preset stays
/// armed after the wrapper exits and unattended submit/mail write into the
/// returned shell. Revocation must treat a non-shell foreground that carried
/// the preset as evidence the agent started.
#[cfg(unix)]
#[test]
fn unrecognised_configured_wrapper_is_revoked_when_the_shell_returns() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "critic-1420r4-wrapper";
    let probe = ForegroundIdentityProbe::new(state.clone(), sid, "mywrapper");
    restore(&state, sid, (Some("claude".into()), true, false));
    assert_eq!(
        refresh_session_agent(&state, sid).as_deref(),
        Some("claude")
    );
    let carried = flags(&state, sid);
    drop(probe);
    let _shell = ForegroundIdentityProbe::new(state.clone(), sid, "bash");
    restore(&state, sid, carried);
    refresh_session_agent(&state, sid);
    assert_eq!(
        flags(&state, sid).0,
        None,
        "wrapper exited to a shell but the never-classified preset stayed armed"
    );
}

/// Catches: the preset is disarmed during shell startup (first poll sees the
/// shell before the agent is launched), or revoked on the second shell poll.
#[cfg(unix)]
#[test]
fn preset_survives_repeated_shell_polls_before_the_agent_starts() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "critic-1420r4-startup";
    let _shell = ForegroundIdentityProbe::new(state.clone(), sid, "bash");
    restore(&state, sid, (Some("claude".into()), true, false));
    for _ in 0..3 {
        refresh_session_agent(&state, sid);
    }
    assert_eq!(flags(&state, sid), (Some("claude".into()), true, false));
}

/// Catches: after the observed preset agent exits and the identity is revoked,
/// further shell polls or a hand-launched agent leave stale provenance: the
/// relaunched agent must be discovered (not preset) and revocable again.
#[cfg(unix)]
#[test]
fn revoked_preset_session_can_rediscover_and_revoke_a_hand_launched_agent() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "critic-1420r4-cycle";
    let agent = ForegroundIdentityProbe::new(state.clone(), sid, "claude");
    restore(&state, sid, (Some("claude".into()), true, false));
    refresh_session_agent(&state, sid);
    let after_agent = flags(&state, sid);
    assert_eq!(after_agent, (Some("claude".into()), true, true));
    drop(agent);

    let shell = ForegroundIdentityProbe::new(state.clone(), sid, "bash");
    restore(&state, sid, after_agent);
    refresh_session_agent(&state, sid);
    refresh_session_agent(&state, sid);
    assert_eq!(flags(&state, sid), (None, false, true));
    let revoked = flags(&state, sid);
    drop(shell);

    let again = ForegroundIdentityProbe::new(state.clone(), sid, "claude");
    restore(&state, sid, revoked);
    assert_eq!(
        refresh_session_agent(&state, sid).as_deref(),
        Some("claude")
    );
    let relaunched = flags(&state, sid);
    assert!(!relaunched.1, "a rediscovered agent must not be a preset");
    drop(again);

    let _shell = ForegroundIdentityProbe::new(state.clone(), sid, "bash");
    restore(&state, sid, relaunched);
    refresh_session_agent(&state, sid);
    assert_eq!(flags(&state, sid).0, None);
}

/// Catches: a preset for one agent (codex) replaced by a different discovered
/// agent (claude) stays flagged as run-config and unrevocable.
#[cfg(unix)]
#[test]
fn different_discovered_agent_drops_preset_and_is_revoked_on_exit() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "critic-1420r4-swap";
    let probe = ForegroundIdentityProbe::new(state.clone(), sid, "claude");
    restore(&state, sid, (Some("codex".into()), true, false));
    refresh_session_agent(&state, sid);
    let seen = flags(&state, sid);
    assert_eq!(seen, (Some("claude".into()), false, true));
    drop(probe);
    let _shell = ForegroundIdentityProbe::new(state.clone(), sid, "bash");
    restore(&state, sid, seen);
    refresh_session_agent(&state, sid);
    assert_eq!(flags(&state, sid).0, None);
}

/// Catches: `(deleted)` handling that over-matches (a non-agent or shell path
/// with the suffix classified as an agent), misses a bare agent path, or only
/// strips the suffix from the versioned-claude layout.
#[test]
fn deleted_suffix_normalisation_is_exact() {
    assert_eq!(
        classify_agent_name_or_path("/usr/local/bin/claude (deleted)"),
        Some("claude")
    );
    assert_eq!(
        classify_agent_name_or_path("/usr/local/bin/codex (deleted)"),
        Some("codex")
    );
    assert_eq!(classify_agent_name_or_path("/usr/bin/bash (deleted)"), None);
    assert_eq!(
        classify_agent_name_or_path("/home/u/claude/versions/not-a-version (deleted)"),
        None
    );
    assert_eq!(
        classify_agent_name_or_path("/home/u/claude/other/2.1.5 (deleted)"),
        None
    );
}
