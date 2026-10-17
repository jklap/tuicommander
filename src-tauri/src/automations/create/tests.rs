use super::*;
use crate::automations::api;

fn minimal() -> Value {
    json!({"name":"Recheck","prompt":"Check again","in_secs":3600})
}
fn config() -> crate::config::AgentsConfig {
    serde_json::from_value(json!({"agents":{
        "codex":{"run_configs":[{"name":"Caller profile","command":"codex","is_default":true}]},
        "claude":{"run_configs":[{"name":"Other profile","command":"claude","is_default":true}]}
    }}))
    .unwrap()
}
fn complete(mut input: Value) -> Value {
    apply_defaults(
        &mut input,
        Some("/caller/repo".into()),
        Some("codex"),
        &config(),
    )
    .unwrap();
    input
}

// Catches: copying a sibling's repository/profile instead of resolving the bound caller.
#[test]
fn defaults_pick_callers_repo_not_another_session() {
    let state = crate::state::tests_support::make_test_app_state();
    for (peer, project) in [("caller", "/caller/repo"), ("sibling", "/other/repo")] {
        state.peer_agents.insert(
            peer.into(),
            crate::state::PeerAgent {
                tuic_session: peer.into(),
                mcp_session_id: format!("mcp-{peer}"),
                name: peer.into(),
                project: Some(project.into()),
                registered_at: 0,
            },
        );
    }
    let mut input = minimal();
    input["run_config"] = json!("Explicit saved profile");
    apply_caller_defaults(
        &mut input,
        &state,
        &api::Caller::Agent(Some("caller".into())),
    )
    .unwrap();
    assert_eq!(input["repository"], "/caller/repo");
    assert_eq!(input["run_config"], "Explicit saved profile");
    let mut full = minimal();
    apply_defaults(
        &mut full,
        Some("/caller/repo".into()),
        Some("codex"),
        &config(),
    )
    .unwrap();
    assert_eq!(full["run_config"], "Caller profile");
    assert_eq!(full["workspace"], json!({"mode":"existing"}));
    assert_eq!(full["enabled"], true);
    assert!(full["id"].as_str().unwrap().starts_with("recheck-"));
    assert_ne!(full["id"], complete(minimal())["id"]);
}

// Catches: unidentified callers inheriting another session's ambient defaults.
#[test]
fn missing_caller_fields_name_repository_and_run_config() {
    let mut input = minimal();
    assert!(
        apply_defaults(&mut input, None, Some("codex"), &config())
            .unwrap_err()
            .contains("repository")
    );
    assert!(
        apply_defaults(&mut input, Some("/repo".into()), None, &config())
            .unwrap_err()
            .contains("run_config")
    );
    let state = crate::state::tests_support::make_test_app_state();
    let mut input = minimal();
    apply_caller_defaults(&mut input, &state, &api::Caller::Client).unwrap();
    assert_eq!(input, minimal());
    assert!(resolve(input, Utc::now()).unwrap_err().contains("id"));
}

// Catches: mixed schedules silently selecting a different occurrence or invalid delays overflowing.
#[test]
fn in_secs_plus_cron_or_once_is_rejected_without_persistence() {
    let now = "2099-10-10T08:00:00Z".parse().unwrap();
    for (field, value) in [
        ("cron", json!("0 9 * * *")),
        ("once_local", json!("2099-10-10T10:00:00")),
        ("once_at", json!("2099-10-10T10:00:00Z")),
    ] {
        let mut input = complete(minimal());
        input[field] = value;
        assert!(
            resolve(input, now)
                .unwrap_err()
                .contains("mutually exclusive")
        );
    }
    for delay in [
        json!(0),
        json!(-1),
        json!(0.5),
        json!("3600"),
        json!(u64::MAX),
        json!(i64::MAX),
        Value::Null,
    ] {
        let mut input = complete(minimal());
        input["in_secs"] = delay;
        assert!(resolve(input, now).unwrap_err().contains("in_secs"));
    }
}

// Catches: adding wall-clock hours across a gap, or firing on the earlier side of a DST fold.
#[test]
fn relative_delay_preserves_exact_instant_across_dst_gap_and_fold() {
    for (now, local, utc) in [
        (
            "2099-03-29T00:30:00Z",
            "2099-03-29T03:30:00",
            "2099-03-29T01:30:00Z",
        ),
        (
            "2099-10-25T00:30:00Z",
            "2099-10-25T02:30:00",
            "2099-10-25T01:30:00Z",
        ),
    ] {
        let now: DateTime<Utc> = now.parse().unwrap();
        let mut input = complete(minimal());
        input["timezone"] = json!("Europe/Madrid");
        let definition = resolve(input, now).unwrap();
        assert_eq!(definition.timezone, "Europe/Madrid");
        assert_eq!(
            definition.once_local.unwrap().to_string(),
            local.replace('T', " ")
        );
        assert_eq!(
            definition
                .schedule()
                .unwrap()
                .next_after(now)
                .unwrap()
                .unwrap(),
            utc.parse::<DateTime<Utc>>().unwrap()
        );
        let stored = serde_json::to_value(&definition).unwrap();
        let reloaded: AutomationDefinition = serde_json::from_value(stored).unwrap();
        assert_eq!(
            reloaded
                .schedule()
                .unwrap()
                .next_after(now)
                .unwrap()
                .unwrap(),
            now + TimeDelta::seconds(3600)
        );
    }
}

// Catches: returned computed metadata making a second save/preview fail strict parsing.
#[test]
fn create_reply_can_be_round_tripped_without_persisting_next_run() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let definitions = crate::automations::definitions::DefinitionStore::at_path(
        dir.path().join("automations.json"),
    );
    let now: DateTime<Utc> = "2099-10-10T08:00:00Z".parse().unwrap();
    let owner = crate::automations::store::RunOwner::acquire_at(
        &dir.path().join("runs.sqlite3"),
        now.timestamp_millis(),
    )
    .unwrap();
    let saved = api::execute_stored_as(
        &definitions,
        owner.store(),
        api::parse(json!({"action":"create","definition":complete(minimal())})).unwrap(),
        now,
        api::Caller::Agent(Some("caller".into())),
    )
    .unwrap();
    assert_eq!(saved["next_run_ms"], now.timestamp_millis() + 3_600_000);
    let mut update = saved.clone();
    update["prompt"] = json!("Updated prompt");
    api::execute_stored(
        &definitions,
        owner.store(),
        api::parse(json!({"action":"update","definition":update})).unwrap(),
        now,
    )
    .unwrap();
    assert_eq!(
        definitions
            .get(saved["id"].as_str().unwrap())
            .unwrap()
            .prompt,
        "Updated prompt"
    );
    assert!(
        !std::fs::read_to_string(dir.path().join("automations.json"))
            .unwrap()
            .contains("next_run_ms")
    );
}

// Catches: malformed exact occurrences bypassing the same persisted schedule validation.
#[test]
fn exact_relative_occurrence_must_match_the_stored_local_time_and_zone() {
    let now = "2099-10-10T08:00:00Z".parse().unwrap();
    let mut input = complete(minimal());
    input["timezone"] = json!("UTC");
    let definition = resolve(input, now).unwrap();
    definition.validate().unwrap();
    for field in ["cron", "once_local", "timezone"] {
        let mut changed = serde_json::to_value(&definition).unwrap();
        changed[field] = match field {
            "cron" => json!("0 9 * * *"),
            "once_local" => json!("2099-10-10T11:00:00"),
            _ => json!("Europe/Madrid"),
        };
        let changed: AutomationDefinition = serde_json::from_value(changed).unwrap();
        assert!(changed.validate().unwrap_err().contains("once_at"));
    }
}
