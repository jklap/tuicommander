//! Round-5 critic tests for story 1420-f3de: wrapper observation and revocation.

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

fn preset() -> Flags {
    (Some("claude".into()), true, false)
}

/// Catches: a wrapper that execs the agent (same pid, name changes from the
/// unclassified wrapper to `claude`) keeps the preset armed or loses identity
/// at the name change, so the shell that returns after exit is still submittable.
#[cfg(unix)]
#[test]
fn wrapper_that_execs_the_agent_is_revoked_when_the_shell_returns() {
    let steps = run(
        "critic-1420r5-exec",
        preset(),
        &["mywrapper", "claude", "bash"],
    );
    assert_eq!(steps[0].0.as_deref(), Some("claude"), "wrapper lost preset");
    assert_eq!(steps[1].0.as_deref(), Some("claude"), "exec lost identity");
    assert_eq!(
        steps[2].0, None,
        "shell after exec'd agent stayed submittable"
    );
}

/// Catches: a wrapper that stays foreground (agent is its child) loses identity
/// after repeated polls, or is revoked while still running.
#[cfg(unix)]
#[test]
fn wrapper_staying_foreground_keeps_identity_across_polls_then_revokes() {
    let steps = run(
        "critic-1420r5-stay",
        preset(),
        &["mywrapper", "mywrapper", "mywrapper", "bash", "bash"],
    );
    for step in &steps[..3] {
        assert_eq!(step.0.as_deref(), Some("claude"));
    }
    assert_eq!(steps[3].0, None);
    assert_eq!(steps[4].0, None, "revocation did not stay revoked");
}

/// Catches: a short-lived unclassified non-shell child (git, ssh-askpass) in the
/// foreground during a discovered agent's life revokes or re-arms identity.
#[cfg(unix)]
#[test]
fn transient_helper_during_a_discovered_agent_life_changes_nothing() {
    let steps = run(
        "critic-1420r5-helper",
        (None, false, false),
        &["claude", "git", "claude", "git", "bash"],
    );
    let live = (Some("claude".to_string()), false, true);
    for step in &steps[..4] {
        assert_eq!(step, &live);
    }
    assert_eq!(steps[4].0, None);
}

/// Catches: after the accepted startup-helper disarm (helper -> shell -> agent),
/// the agent that finally starts is not rediscovered as a revocable agent.
#[cfg(unix)]
#[test]
fn agent_started_after_helper_disarm_is_rediscovered_and_revocable() {
    let steps = run(
        "critic-1420r5-recover",
        preset(),
        &["git", "bash", "claude", "bash"],
    );
    assert_eq!(
        steps[1].0, None,
        "accepted trade-off: disarmed at the shell"
    );
    assert_eq!(steps[2], (Some("claude".into()), false, true));
    assert_eq!(steps[3].0, None);
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
