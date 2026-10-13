use super::{ego_fixture::*, request::*};
use serde_json::json;

// Catches: the model owner ignores the configured profile and edits the base profile.
#[tokio::test]
async fn ego_profile_edit_cannot_target_other_profile() {
    let f = fixture();
    let base = disk(&f, "base");
    let beta = disk(&f, "beta");
    let reply = crate::ego_cli::set_default_model(&f.state, "ollama/changed".into())
        .await
        .unwrap();
    assert_eq!(disk(&f, "alpha")["values"]["model"], expected_model());
    assert_eq!(reply.default_model, Some("ollama/changed".into()));
    assert_eq!(disk(&f, "base"), base);
    assert_eq!(disk(&f, "beta"), beta);
    assert!(
        calls(&f)
            .iter()
            .all(|call| call["cwd"] == f.dir.path().to_string_lossy().as_ref())
    );
}

// Catches: an adapter edit clobbers sibling fields, another profile, or another store.
#[tokio::test]
async fn adapter_preserves_other_fields() {
    let f = fixture();
    let before = disk(&f, "alpha");
    let beta = disk(&f, "beta");
    let neighbour = f.dir.path().join("notifications.json");
    std::fs::write(&neighbour, "sentinel").unwrap();
    let snapshot = get(&f, &["/model"]).await.unwrap();
    let reply = set(&f, snapshot.revision, "/model", expected_model())
        .await
        .unwrap();
    assert!(reply.persisted && reply.applied);
    let mut expected = before;
    expected["values"]["model"] = expected_model();
    assert_eq!(disk(&f, "alpha"), expected);
    assert_eq!(disk(&f, "beta"), beta);
    assert_eq!(std::fs::read_to_string(neighbour).unwrap(), "sentinel");
    let after = get(&f, &["/model"]).await.unwrap();
    assert_eq!(after.values["/model"].stored, expected_model());
    assert_eq!(after.revision, reply.revision);
}

// Catches: a caller-selected executable is probed or a foreign profile is accepted.
#[tokio::test]
async fn ego_uses_trusted_binary_and_profile() {
    let f = fixture();
    let snapshot = get(&f, &["/model"]).await.unwrap();
    let before = calls(&f).len();
    let error = f
        .registry
        .execute(Request::Set {
            target: Target::Profile { id: "beta".into() },
            store: "ego".into(),
            path: "/model".into(),
            value: expected_model(),
            expected_revision: snapshot.revision.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, "not_found");
    assert_eq!(
        calls(&f).len(),
        before,
        "foreign target must not launch a probe"
    );
    let error = set(
        &f,
        snapshot.revision,
        "/executable",
        json!("/untrusted/program"),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "unknown_path");
    assert!(
        !calls(&f)
            .iter()
            .any(|c| c.to_string().contains("/untrusted/program"))
    );
}

// Catches: secret diagnostics or unknown owner fields escaping through reads or failures.
#[tokio::test]
async fn adapter_secret_fields_presence_only() {
    let f = fixture();
    let mut config = disk(&f, "alpha");
    config["values"]["unknown_secret"] = json!("canary-ego-secret-in-diagnostic");
    std::fs::write(f.dir.path().join("alpha.json"), config.to_string()).unwrap();
    std::fs::write(
        f.dir.path().join("auth.txt"),
        include_str!("fixtures/ego/auth-present.txt"),
    )
    .unwrap();
    let snapshot = get(
        &f,
        &[
            "/model",
            "/credentials/openrouter",
            "/credentials/anthropic",
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        snapshot.values["/credentials/openrouter"].stored,
        json!({"status":"set"})
    );
    assert_eq!(
        snapshot.values["/credentials/anthropic"].stored,
        json!({"status":"unset"})
    );
    let rejected = set(
        &f,
        snapshot.revision.clone(),
        "/credentials/openrouter",
        json!("canary-ego-secret-in-diagnostic"),
    )
    .await
    .unwrap_err();
    assert_eq!(rejected.code, "unavailable");
    assert!(
        !serde_json::to_string(&rejected)
            .unwrap()
            .contains("canary-ego-secret")
    );
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("canary-ego-secret")
    );
    std::fs::write(f.dir.path().join("fail"), "").unwrap();
    let error = get(&f, &["/model"]).await.unwrap_err();
    assert_eq!(error.code, "store_failed");
    assert!(
        !serde_json::to_string(&error)
            .unwrap()
            .contains("canary-ego-secret")
    );
}

// Catches: stale revisions and multi-key patches publish a partial edit.
#[tokio::test]
async fn stale_or_multi_key_patch_writes_nothing() {
    let f = fixture();
    let snapshot = get(&f, &["/model"]).await.unwrap();
    let before = disk(&f, "alpha");
    let error = set(&f, "stale".into(), "/model", expected_model())
        .await
        .unwrap_err();
    assert_eq!(error.code, "conflict");
    let error = f
        .registry
        .execute(Request::Patch {
            target: Target::Profile { id: "alpha".into() },
            store: "ego".into(),
            expected_revision: snapshot.revision,
            operations: vec![
                Edit::Set {
                    path: "/model".into(),
                    value: expected_model(),
                },
                Edit::Set {
                    path: "/network".into(),
                    value: json!("on"),
                },
            ],
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, "unavailable");
    assert_eq!(disk(&f, "alpha"), before);
    assert!(!calls(&f).iter().any(|c| c["args"][1] == "set"));
}

// Catches: unset is stored as a literal null, or explicit [] becomes absent/inherited roots.
#[tokio::test]
async fn reset_unsets_while_empty_roots_remain_explicit() {
    let f = fixture();
    let snapshot = get(&f, &["/model"]).await.unwrap();
    let reply = f
        .registry
        .execute(Request::Reset {
            target: Target::Profile { id: "alpha".into() },
            store: "ego".into(),
            path: "/roots".into(),
            expected_revision: snapshot.revision,
        })
        .await
        .unwrap();
    assert!(
        !disk(&f, "alpha")["values"]
            .as_object()
            .unwrap()
            .contains_key("roots")
    );
    let snapshot = get(&f, &["/roots"]).await.unwrap();
    assert_eq!(snapshot.values["/roots"].stored, json!(null));
    set(&f, snapshot.revision, "/roots", json!([]))
        .await
        .unwrap();
    assert_eq!(disk(&f, "alpha")["values"]["roots"], json!([]));
    assert!(
        !serde_json::to_string(&reply)
            .unwrap()
            .contains("canary-ego-secret")
    );
}

// Catches: malformed model/enum values or base-only keys reach an owner write.
#[tokio::test]
async fn invalid_values_and_wrong_scope_are_rejected_before_write() {
    let f = fixture();
    let snapshot = get(&f, &["/model"]).await.unwrap();
    for (path, value) in [
        ("/model", json!("-flag")),
        ("/model", json!("missing-provider")),
        ("/network", json!("offline")),
        ("/bare", json!(true)),
        ("/session_dir", json!("/tmp/caller")),
    ] {
        assert!(
            set(&f, snapshot.revision.clone(), path, value)
                .await
                .is_err()
        );
    }
    assert!(!calls(&f).iter().any(|c| c["args"][1] == "set"));
    f.state.config.write().ego_profile = "beta".into();
    let before = calls(&f).len();
    let error = get(&f, &["/model"]).await.unwrap_err();
    assert_eq!(error.code, "not_found");
    assert_eq!(calls(&f).len(), before);
}

// Catches: a changed workspace/process authority is accepted under an earlier revision.
#[tokio::test]
async fn authority_change_invalidates_revision_and_missing_binary_does_not_probe() {
    let f = fixture();
    let snapshot = get(&f, &["/model"]).await.unwrap();
    let workspace = f.dir.path().join("new-workspace");
    std::fs::create_dir(&workspace).unwrap();
    f.state.config.write().ai_chat_workspace = workspace.to_string_lossy().into_owned();
    let error = set(&f, snapshot.revision, "/model", expected_model())
        .await
        .unwrap_err();
    assert_eq!(error.code, "conflict");
    f.state.config.write().ego_executable.clear();
    let before = calls(&f).len();
    assert_eq!(get(&f, &["/model"]).await.unwrap_err().code, "unavailable");
    assert_eq!(calls(&f).len(), before);
}

// Catches: managed keys are misrouted, root paths become TOML injection, or unset stops working.
#[tokio::test]
async fn managed_keys_roundtrip_as_closed_assignments() {
    let f = fixture();
    for (path, value) in [
        ("/thinking", json!("high")),
        ("/mode", json!("edits")),
        ("/sandbox", json!("adaptive")),
        ("/network", json!("on")),
        ("/budget", json!("400k")),
        ("/permissions.judge", json!("ollama/judge")),
        ("/exec.env", json!("allowlist")),
        ("/exec.env_allowlist", json!(["PATH", "HOME"])),
        (
            "/roots",
            json!([{"path":"/data/a\"},network=\"on", "access":"read"}]),
        ),
    ] {
        let before = get(&f, &[path]).await.unwrap();
        let reply = set(&f, before.revision, path, value.clone()).await.unwrap();
        let after = get(&f, &[path]).await.unwrap();
        assert_eq!(after.values[path].stored, value);
        assert_eq!(after.revision, reply.revision);
    }
    let disk = disk(&f, "alpha");
    assert_eq!(disk["values"]["network"], json!("on"));
    assert_eq!(
        disk["values"]["roots"][0]["path"],
        json!("/data/a\"},network=\"on")
    );
}

// Catches: the owner writes successfully but a failed readback invites replay or hides durability.
#[tokio::test]
async fn readback_failure_reports_durable_write() {
    let f = fixture();
    let before = get(&f, &["/model"]).await.unwrap();
    std::fs::write(f.dir.path().join("fail-after-write"), "").unwrap();
    let error = set(&f, before.revision, "/model", expected_model())
        .await
        .unwrap_err();
    assert!(error.persisted);
    assert_eq!(error.next_action, "get");
    assert_eq!(disk(&f, "alpha")["values"]["model"], expected_model());
    assert!(
        !serde_json::to_string(&error)
            .unwrap()
            .contains("canary-ego-secret")
    );
}

// Catches: TUIC launch edits replace unrelated AppConfig fields or execute a supplied binary.
#[tokio::test]
async fn launch_defaults_preserve_owner_and_wait_for_next_session() {
    use super::{ego_launch::EgoLaunchSettingsStore, native::native_catalogue, service::Service};
    use std::sync::Arc;
    let f = fixture();
    let _guard = crate::config::set_config_dir_override(f.dir.path().to_path_buf());
    crate::credentials::reset_test_faults();
    let path = f.dir.path().join("config.json");
    let mut config = crate::config::AppConfig::default();
    config.ego_executable = f.state.config.read().ego_executable.clone();
    config.ego_profile = "alpha".into();
    config.font_size = 19;
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let service = Service::new(
        Arc::new(native_catalogue().unwrap()),
        Arc::new(EgoLaunchSettingsStore::new(f.state.clone())),
    );
    let snapshot = service
        .get(
            Target::Machine,
            "ego_launch".into(),
            vec!["/ego_profile".into()],
        )
        .await
        .unwrap();
    let reply = service
        .patch(
            Target::Machine,
            "ego_launch".into(),
            snapshot.revision,
            vec![Edit::Set {
                path: "/ego_profile".into(),
                value: json!("beta"),
            }],
        )
        .await
        .unwrap();
    assert!(reply.persisted && reply.applied);
    assert_eq!(reply.restart, vec!["next_session"]);
    let config: crate::config::AppConfig =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(config.font_size, 19);
    assert_eq!(config.ego_profile, "beta");
    assert_eq!(f.state.config.read().ego_profile, "beta");
    let after = service
        .get(
            Target::Machine,
            "ego_launch".into(),
            vec!["/ego_profile".into()],
        )
        .await
        .unwrap();
    assert_eq!(after.values["/ego_profile"].stored, json!("beta"));
    assert_eq!(after.revision, reply.revision);
    let error = service
        .patch(
            Target::Machine,
            "ego_launch".into(),
            after.revision,
            vec![Edit::Set {
                path: "/ego_executable".into(),
                value: json!("/untrusted/program"),
            }],
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "unavailable");
    assert!(
        calls(&f).is_empty(),
        "launch settings never probe or restart Ego"
    );
}
