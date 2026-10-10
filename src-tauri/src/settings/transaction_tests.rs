//! Consumer regressions at the actual AppConfig owner boundary.
use super::{native::native_catalogue, request::*, service::Service};
use crate::{
    config::settings_app::AppSettingsStore,
    config::{self, AppConfig},
};
use serde_json::{Value, json};
use std::sync::Arc;

fn set(path: &str, value: Value) -> Edit {
    Edit::Set {
        path: path.into(),
        value,
    }
}

fn service() -> (Arc<crate::AppState>, Service<AppSettingsStore>) {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let service = Service::new(
        Arc::new(native_catalogue().unwrap()),
        Arc::new(AppSettingsStore::new(state.clone())),
    );
    (state, service)
}

async fn read(service: &Service<AppSettingsStore>) -> Snapshot {
    service
        .get(
            Target::Machine,
            "app".into(),
            vec!["/font_size".into(), "/shell".into()],
        )
        .await
        .unwrap()
}

#[tokio::test]
#[serial_test::serial]
async fn patch_typo_rejected() {
    crate::credentials::reset_test_faults();
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = config::set_config_dir_override(dir.path().to_path_buf());
    let (_, service) = service();
    let snapshot = read(&service).await;
    let error = service
        .patch(
            Target::Machine,
            "app".into(),
            snapshot.revision,
            vec![set("/font_siz", json!(21))],
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "unknown_path");
    assert!(!dir.path().join("config.json").exists());
    let malformed = json!({"action":"patch", "target":{"kind":"machine"}, "store":"app", "expectedRevsion":"r1", "operations":[]});
    assert!(serde_json::from_value::<Request>(malformed).is_err());
    let extra = json!({"action":"patch", "target":{"kind":"machine"}, "store":"app", "expectedRevision":"r1", "operations":[], "confirmed":true});
    assert!(serde_json::from_value::<Request>(extra).is_err());
}

#[tokio::test]
#[serial_test::serial]
async fn stale_revision_rejected() {
    crate::credentials::reset_test_faults();
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = config::set_config_dir_override(dir.path().to_path_buf());
    let (state, service) = service();
    let snapshot = read(&service).await;
    let first = service
        .patch(
            Target::Machine,
            "app".into(),
            snapshot.revision.clone(),
            vec![set("/font_size", json!(21))],
        )
        .await
        .unwrap();
    let error = service
        .patch(
            Target::Machine,
            "app".into(),
            snapshot.revision,
            vec![set("/font_size", json!(23))],
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "conflict");
    assert_eq!(error.revision, Some(first.revision.clone()));
    assert_eq!(state.config.read().font_size, 21);
    assert_eq!(
        read(&service).await.revision,
        first.revision,
        "reply revision must equal persisted readback"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn invalid_batch_writes_nothing() {
    crate::credentials::reset_test_faults();
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = config::set_config_dir_override(dir.path().to_path_buf());
    let (state, service) = service();
    let before = state.config.read().font_size;
    let revision = read(&service).await.revision;
    let error = service
        .patch(
            Target::Machine,
            "app".into(),
            revision.clone(),
            vec![
                set("/font_size", json!(21)),
                set("/font_weight", json!(950)),
            ],
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "out_of_range");
    assert!(!error.persisted);
    assert!(!dir.path().join("config.json").exists());
    assert_eq!(state.config.read().font_size, before);
    assert_eq!(read(&service).await.revision, revision);
}

#[tokio::test]
#[serial_test::serial]
async fn omitted_optional_set_reset_and_other_fields_survive() {
    crate::credentials::reset_test_faults();
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = config::set_config_dir_override(dir.path().to_path_buf());
    let (state, service) = service();
    let snapshot = read(&service).await;
    assert_eq!(snapshot.values["/shell"].stored, Value::Null);
    let reply = service
        .patch(
            Target::Machine,
            "app".into(),
            snapshot.revision,
            vec![
                set("/shell", json!("coordinator")),
                set("/font_size", json!(21)),
            ],
        )
        .await
        .unwrap();
    assert!(reply.persisted && reply.applied);
    assert_eq!(reply.changed_ids, vec!["app.font_size", "app.shell"]);
    assert!(reply.restart.contains(&"next_session".into()));
    let reply = service
        .patch(
            Target::Machine,
            "app".into(),
            reply.revision,
            vec![Edit::Reset {
                path: "/shell".into(),
            }],
        )
        .await
        .unwrap();
    assert!(state.config.read().shell.is_none());
    assert_eq!(state.config.read().font_size, 21);
    let same = service
        .patch(
            Target::Machine,
            "app".into(),
            reply.revision.clone(),
            vec![set("/font_size", json!(21))],
        )
        .await
        .unwrap();
    assert!(!same.persisted);
    assert!(same.changed_ids.is_empty());
    assert_eq!(same.revision, reply.revision);
}

#[tokio::test]
#[serial_test::serial]
async fn wrong_owner_malformed_paths_and_secret_objects_never_commit() {
    crate::credentials::reset_test_faults();
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = config::set_config_dir_override(dir.path().to_path_buf());
    let (_, service) = service();
    let revision = read(&service).await.revision;
    let error = service
        .patch(
            Target::Instance,
            "app".into(),
            revision.clone(),
            vec![set("/font_size", json!(21))],
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "not_found");
    for path in [
        "",
        "font_size",
        "/",
        "/services//port",
        "/__proto__/font_size",
        "/../font_size",
        "/font~2size",
    ] {
        let error = service
            .patch(
                Target::Machine,
                "app".into(),
                revision.clone(),
                vec![set(path, json!(21))],
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, "unknown_path", "{path}");
    }
    let canary = "private-value-must-not-appear";
    let error = service
        .patch(
            Target::Machine,
            "app".into(),
            revision,
            vec![set("/services/relay/token", json!({"status":canary}))],
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "unavailable");
    assert!(!serde_json::to_string(&error).unwrap().contains(canary));
    assert!(!dir.path().join("config.json").exists());
}

#[path = "failure_tests.rs"]
mod failures;
