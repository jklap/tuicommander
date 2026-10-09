use super::*;
use crate::automations::{
    model::AutomationDefinition,
    run::{RunDetails, RunStatus},
    store::RunOwner,
};

fn definition(id: &str) -> AutomationDefinition {
    serde_json::from_value(serde_json::json!({
        "id":id,"name":"Review","prompt":"Review changes","run_config":"codex",
        "repository":"/project","workspace":{"mode":"existing"},"cron":"0 * * * *",
        "timezone":"UTC","enabled":true,"grace_secs":1800,"overlap":"skip",
        "max_duration_secs":3600,"precheck":null
    }))
    .unwrap()
}
fn config() -> AutomationsConfig {
    AutomationsConfig {
        definitions: vec![definition("a")],
        ..Default::default()
    }
}
fn owner() -> (tempfile::TempDir, RunOwner) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let owner = RunOwner::acquire_at(&dir.path().join("runs.sqlite3"), 0).unwrap();
    (dir, owner)
}
fn now(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
}

// Catches replay backlog, exclusive grace, rollback replay and loss of cursor after pruning/restart.
#[test]
fn latest_only_inclusive_grace_and_durable_cursor_prevent_replay() {
    let (dir, owner) = owner();
    let cfg = config();
    let at = now("2026-10-09T12:30:00Z");
    let runs = tick(owner.store(), &cfg, at).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, RunStatus::Reserved);
    assert_eq!(
        runs[0].trigger,
        RunTrigger::Scheduled {
            occurrence_ms: now("2026-10-09T12:00:00Z").timestamp_millis()
        }
    );
    assert!(tick(owner.store(), &cfg, at).unwrap().is_empty());
    assert!(
        tick(owner.store(), &cfg, now("2026-10-09T11:00:00Z"))
            .unwrap()
            .is_empty()
    );
    owner
        .store()
        .transition(
            &runs[0].id,
            RunStatus::Completed,
            RunDetails::default(),
            at.timestamp_millis(),
        )
        .unwrap();
    owner
        .store()
        .prune(now("2027-10-09T12:00:00Z").timestamp_millis(), Some(1))
        .unwrap();
    drop(owner);
    let owner =
        RunOwner::acquire_at(&dir.path().join("runs.sqlite3"), at.timestamp_millis()).unwrap();
    assert!(tick(owner.store(), &cfg, at).unwrap().is_empty());
    let late = tick(owner.store(), &cfg, now("2026-10-12T12:30:00.001Z")).unwrap();
    assert_eq!(late.len(), 1);
    assert_eq!(late[0].status, RunStatus::SkippedMissed);
    assert!(owner.store().open_runs().unwrap().is_empty());
}

// Catches paused manual runs being refused, overlap ignoring needs-you, cap ignoring pending, or manual advancing cursor.
#[test]
fn manual_and_scheduled_share_capacity_without_sharing_cursor() {
    let (_dir, owner) = owner();
    let store = owner.store();
    let mut cfg = config();
    cfg.definitions[0].enabled = false;
    let at = now("2026-10-09T12:00:00Z");
    assert!(tick(store, &cfg, at).unwrap().is_empty());
    let first = run_now(store, &cfg, "a", at).unwrap();
    assert_eq!(first.status, RunStatus::Reserved);
    store
        .transition(
            &first.id,
            RunStatus::NeedsYou,
            RunDetails::default(),
            at.timestamp_millis(),
        )
        .unwrap();
    assert_eq!(
        run_now(store, &cfg, "a", at).unwrap().status,
        RunStatus::SkippedOverlap
    );
    cfg.definitions.push(definition("b"));
    assert_eq!(
        run_now(store, &cfg, "b", at).unwrap().status,
        RunStatus::Reserved
    );
    cfg.definitions.push(definition("c"));
    assert_eq!(
        run_now(store, &cfg, "c", at).unwrap().status,
        RunStatus::SkippedConcurrency
    );
    cfg.max_concurrent_runs = 1;
    assert_eq!(store.open_runs().unwrap().len(), 2);
    assert_eq!(
        run_now(store, &cfg, "c", at).unwrap().status,
        RunStatus::SkippedConcurrency
    );
    for run in store.open_runs().unwrap() {
        store
            .transition(
                &run.id,
                RunStatus::Completed,
                RunDetails::default(),
                at.timestamp_millis(),
            )
            .unwrap();
    }
    cfg.definitions[0].enabled = true;
    cfg.definitions.truncate(1);
    assert_eq!(
        tick(store, &cfg, at).unwrap()[0].status,
        RunStatus::Reserved
    );
}

// Catches race oversubscription between Run Now and scheduled admission, and replay after restart.
#[test]
fn concurrent_manual_and_tick_reserve_atomically_and_restart_never_retries() {
    let (dir, owner) = owner();
    let mut cfg = config();
    cfg.max_concurrent_runs = 1;
    cfg.definitions.push(definition("b"));
    let at = now("2026-10-09T12:00:00Z");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let handles: Vec<_> = (0..2)
        .map(|i| {
            let store = owner.store().clone();
            let cfg = cfg.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                if i == 0 {
                    tick(&store, &cfg, at).unwrap()
                } else {
                    vec![run_now(&store, &cfg, "b", at).unwrap()]
                }
            })
        })
        .collect();
    barrier.wait();
    let runs: Vec<_> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    assert_eq!(
        runs.iter()
            .filter(|r| r.status == RunStatus::Reserved)
            .count(),
        1
    );
    assert_eq!(owner.store().open_runs().unwrap().len(), 1);
    drop(owner);
    let owner =
        RunOwner::acquire_at(&dir.path().join("runs.sqlite3"), at.timestamp_millis() + 1).unwrap();
    assert!(owner.store().open_runs().unwrap().is_empty());
    assert!(tick(owner.store(), &cfg, at).unwrap().is_empty());
    assert_eq!(
        run_now(owner.store(), &cfg, "a", at).unwrap().status,
        RunStatus::Reserved
    );
}

// Catches full capacity secretly queueing a scheduled occurrence for a later tick.
#[test]
fn capacity_refusal_is_final_for_that_occurrence() {
    let (_dir, owner) = owner();
    let mut cfg = config();
    cfg.max_concurrent_runs = 1;
    cfg.definitions.push(definition("b"));
    let at = now("2026-10-09T12:00:00Z");
    let runs = tick(owner.store(), &cfg, at).unwrap();
    assert_eq!(
        runs.iter().map(|r| r.status).collect::<Vec<_>>(),
        vec![RunStatus::Reserved, RunStatus::SkippedConcurrency]
    );
    owner
        .store()
        .transition(
            &runs[0].id,
            RunStatus::Completed,
            RunDetails::default(),
            at.timestamp_millis(),
        )
        .unwrap();
    assert!(tick(owner.store(), &cfg, at).unwrap().is_empty());
    assert!(run_now(owner.store(), &cfg, "missing", at).is_err());
    cfg.max_concurrent_runs = 0;
    assert!(tick(owner.store(), &cfg, at).is_err());
}

// Catches cap accounting dropping prechecking/running states or overlap checks using only running.
#[test]
fn every_open_state_blocks_overlap_and_consumes_capacity() {
    for status in [
        RunStatus::Reserved,
        RunStatus::Prechecking,
        RunStatus::Running,
        RunStatus::NeedsYou,
    ] {
        let (_dir, owner) = owner();
        let mut cfg = config();
        cfg.max_concurrent_runs = 1;
        cfg.definitions.push(definition("b"));
        let at = now("2026-10-09T12:00:00Z");
        let run = run_now(owner.store(), &cfg, "a", at).unwrap();
        owner
            .store()
            .transition(
                &run.id,
                status,
                RunDetails::default(),
                at.timestamp_millis(),
            )
            .unwrap();
        assert_eq!(
            run_now(owner.store(), &cfg, "a", at).unwrap().status,
            RunStatus::SkippedOverlap,
            "{status:?}"
        );
        assert_eq!(
            run_now(owner.store(), &cfg, "b", at).unwrap().status,
            RunStatus::SkippedConcurrency,
            "{status:?}"
        );
    }
}

// Catches two simultaneous scheduler ticks returning the same dispatchable occurrence.
#[test]
fn concurrent_ticks_dispatch_an_occurrence_only_once() {
    let (_dir, owner) = owner();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let store = owner.store().clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                tick(&store, &config(), now("2026-10-09T12:00:00Z")).unwrap()
            })
        })
        .collect();
    barrier.wait();
    let runs: Vec<_> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, RunStatus::Reserved);
}
