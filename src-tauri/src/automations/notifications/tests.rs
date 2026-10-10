use super::*;
use crate::automations::{
    model::AutomationDefinition,
    run::{RunDetails, RunTrigger},
    store::RunOwner,
};
fn setup() -> (tempfile::TempDir, RunOwner, AutomationRun) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let owner = RunOwner::acquire_at(&dir.path().join("runs.sqlite3"), 0).unwrap();
    let definition: AutomationDefinition = serde_json::from_value(serde_json::json!({
        "id":"notice", "name":"Review", "prompt":"private prompt", "run_config":"codex",
        "repository":"/repo", "workspace":{"mode":"existing"}, "cron":"0 9 * * *",
        "timezone":"UTC", "enabled":true, "grace_secs":43200,
        "overlap":"skip", "max_duration_secs":3600, "precheck":null
    }))
    .unwrap();
    let run = owner
        .store()
        .reserve(&definition, RunTrigger::Manual, 1)
        .unwrap()
        .unwrap();
    let run = owner
        .store()
        .transition(
            &run.id,
            RunStatus::Running,
            RunDetails {
                session_id: Some("owned".into()),
                ..Default::default()
            },
            2,
        )
        .unwrap();
    (dir, owner, run)
}
// Catches repeated awaiting signals and reconnects creating a notification storm.
#[test]
fn live_channels_claim_once_and_restart_never_replays_ambiguous_sends() {
    let (dir, owner, run) = setup();
    let run = owner
        .store()
        .transition(&run.id, RunStatus::NeedsYou, Default::default(), 3)
        .unwrap();
    for channel in ["bell", "native", "push", "telegram"] {
        assert!(claim(owner.store(), &run, channel).unwrap());
        assert!(!claim(owner.store(), &run, channel).unwrap());
    }
    drop(owner);
    let owner = RunOwner::acquire_at(&dir.path().join("runs.sqlite3"), 4).unwrap();
    for channel in ["bell", "native", "push", "telegram"] {
        assert!(!claim(owner.store(), &run, channel).unwrap());
        assert_eq!(
            owner
                .store()
                .notification(&run.id, "needs_you", channel)
                .unwrap()
                .status,
            DeliveryStatus::Unknown
        );
    }
    assert_eq!(
        owner.store().get(&run.id).unwrap().status,
        RunStatus::Interrupted
    );
}
// Catches delivery errors rewriting the execution result or preventing another channel.
#[test]
fn delivery_failure_is_independent_of_execution_and_other_channels() {
    let (_dir, owner, run) = setup();
    let run = owner
        .store()
        .transition(
            &run.id,
            RunStatus::Failed,
            RunDetails {
                stdout: Some("canonical result".into()),
                ..Default::default()
            },
            3,
        )
        .unwrap();
    assert!(claim(owner.store(), &run, "push").unwrap());
    settle(owner.store(), &run, "push", false);
    assert!(claim(owner.store(), &run, "native").unwrap());
    settle(owner.store(), &run, "native", true);
    assert_eq!(
        owner
            .store()
            .notification(&run.id, "failed", "push")
            .unwrap()
            .status,
        DeliveryStatus::Unknown
    );
    assert_eq!(
        owner
            .store()
            .notification(&run.id, "failed", "native")
            .unwrap()
            .status,
        DeliveryStatus::Confirmed
    );
    assert_eq!(owner.store().get(&run.id).unwrap(), run);
}
// Catches historical successes/skips/unknown states producing misleading live notices.
#[test]
fn only_needs_you_and_failed_route_with_existing_preferences() {
    let (_dir, owner, mut run) = setup();
    let mut config = crate::config::NotificationConfig::default();
    for status in [
        RunStatus::Running,
        RunStatus::Completed,
        RunStatus::Unknown,
        RunStatus::Interrupted,
        RunStatus::SkippedPrecheck,
        RunStatus::TimedOut,
    ] {
        run.status = status;
        assert!(!claim(owner.store(), &run, "bell").unwrap());
        assert!(!enabled(&config, &run));
    }
    for status in [RunStatus::NeedsYou, RunStatus::Failed] {
        run.status = status;
        assert!(enabled(&config, &run));
        config.enabled = false;
        assert!(!enabled(&config, &run));
        config.enabled = true;
    }
    run.status = RunStatus::NeedsYou;
    config.sounds.question = false;
    assert!(!enabled(&config, &run));
    run.status = RunStatus::Failed;
    config.sounds.error = false;
    assert!(!enabled(&config, &run));
}
