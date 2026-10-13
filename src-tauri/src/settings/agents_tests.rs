//! Agents adapter regressions at the real `agents.json` owner boundary, driven through the
//! registry the other adapters plug into.
use super::{
    agents::AgentsSettingsStore, native::native_catalogue, registry::Registry, request::*,
    service::Service,
};
use serde_json::{Value, json};
use std::sync::Arc;

const CANARY: &str = "canary-agent-env-9b2e";
const RUN: &str = "/agents/codex/run_configs";

struct Fixture {
    // Declared first: the override guard drops before the directory it points at.
    _guard: Box<dyn std::any::Any>,
    dir: tempfile::TempDir,
    registry: Registry,
}

fn initial() -> Value {
    json!({
        "agents": {
            "codex": {
                "codex_bypass_migrated": true,
                "idle_close_minutes": 9,
                "run_configs": [
                    {"name": "Alpha", "command": "codex", "args": ["--a"],
                     "env": {"API_KEY": CANARY}, "is_default": true},
                    {"name": "Beta", "command": "codex", "args": ["--b"], "is_default": false},
                ],
            },
            "claude": {"run_configs": [{"name": "Gamma", "command": "claude"}]},
        },
        "headless_agent": "claude",
    })
}

fn fixture(config: Option<Value>) -> Fixture {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    // Launch-instruction refresh and the codex migration resolve the global config dir.
    let guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    std::fs::write(dir.path().join("codex-bypass-migrated"), "1").unwrap();
    if let Some(config) = config {
        std::fs::write(dir.path().join("agents.json"), config.to_string()).unwrap();
    }
    let mut registry = Registry::default();
    registry
        .register(Service::new(
            Arc::new(native_catalogue().unwrap()),
            Arc::new(AgentsSettingsStore::at_path(dir.path().join("agents.json"))),
        ))
        .unwrap();
    Fixture {
        _guard: Box::new(guard),
        dir,
        registry,
    }
}

async fn get(f: &Fixture, paths: &[&str]) -> Result<Snapshot, Failure> {
    match f
        .registry
        .execute(Request::Get {
            target: Target::Machine,
            store: "agents".into(),
            paths: paths.iter().map(|p| (*p).into()).collect(),
        })
        .await?
    {
        Reply::Snapshot(s) => Ok(s),
        Reply::Commit(_) => panic!("get returned a commit"),
    }
}

async fn set(
    f: &Fixture,
    revision: String,
    path: &str,
    value: Value,
) -> Result<CommitReply, Failure> {
    match f
        .registry
        .execute(Request::Set {
            target: Target::Machine,
            store: "agents".into(),
            path: path.into(),
            value,
            expected_revision: revision,
        })
        .await?
    {
        Reply::Commit(c) => Ok(c),
        Reply::Snapshot(_) => panic!("set returned a snapshot"),
    }
}

async fn revision(f: &Fixture) -> String {
    get(f, &["/headless_agent"]).await.unwrap().revision
}

fn on_disk(f: &Fixture) -> Value {
    serde_json::from_str(&std::fs::read_to_string(f.dir.path().join("agents.json")).unwrap())
        .unwrap()
}

/// Catches: a patch to one run config replacing the whole launch collection, losing the
/// sibling run config, its secret env, or another agent's configs.
#[tokio::test]
async fn run_config_patch_preserves_siblings() {
    let f = fixture(Some(initial()));
    let reply = set(
        &f,
        revision(&f).await,
        &format!("{RUN}/Beta/command"),
        json!("codex-next"),
    )
    .await
    .unwrap();
    assert!(reply.persisted && reply.applied);
    assert_eq!(reply.restart, vec!["next_session"]);
    let disk = on_disk(&f);
    let runs = &disk["agents"]["codex"]["run_configs"];
    assert_eq!(runs.as_array().unwrap().len(), 2);
    assert_eq!(runs[1]["name"], json!("Beta"));
    assert_eq!(runs[1]["command"], json!("codex-next"));
    assert_eq!(runs[1]["args"], json!(["--b"]));
    assert_eq!(runs[0]["name"], json!("Alpha"));
    assert_eq!(runs[0]["command"], json!("codex"));
    assert_eq!(runs[0]["env"], json!({"API_KEY": CANARY}));
    assert_eq!(runs[0]["is_default"], json!(true));
    assert_eq!(disk["agents"]["codex"]["idle_close_minutes"], json!(9));
    assert_eq!(
        disk["agents"]["claude"]["run_configs"][0]["command"],
        json!("claude")
    );
    assert_eq!(disk["headless_agent"], json!("claude"));
}

/// Catches: an edit clobbering sibling fields, or touching another store's file.
#[tokio::test]
async fn adapter_preserves_other_fields() {
    let f = fixture(Some(initial()));
    let neighbour = f.dir.path().join("notifications.json");
    std::fs::write(&neighbour, "{\"sentinel\":true}").unwrap();
    set(&f, revision(&f).await, "/headless_agent", json!("codex"))
        .await
        .unwrap();
    let disk = on_disk(&f);
    assert_eq!(disk["headless_agent"], json!("codex"));
    assert_eq!(
        disk["agents"]["codex"]["run_configs"][0]["env"],
        json!({"API_KEY": CANARY})
    );
    assert_eq!(
        disk["agents"]["codex"]["run_configs"][1]["name"],
        json!("Beta")
    );
    assert_eq!(
        disk["agents"]["claude"]["run_configs"][0]["name"],
        json!("Gamma")
    );
    assert_eq!(disk["agents"]["codex"]["idle_close_minutes"], json!(9));
    assert_eq!(
        std::fs::read_to_string(&neighbour).unwrap(),
        "{\"sentinel\":true}"
    );
}

/// Catches: a run-config env value (a credential) appearing in a read, reply or error.
#[tokio::test]
async fn adapter_secret_fields_presence_only() {
    let f = fixture(Some(initial()));
    let env = format!("{RUN}/Alpha/env/API_KEY");
    let snapshot = get(&f, &[&env, &format!("{RUN}/Alpha/command")])
        .await
        .unwrap();
    assert_eq!(snapshot.values[&env].stored, json!({"status": "set"}));
    let mut texts = vec![serde_json::to_string(&snapshot).unwrap()];
    for result in [
        set(&f, "stale".into(), &env, json!(CANARY)).await,
        set(&f, snapshot.revision.clone(), &env, json!(CANARY)).await,
        set(
            &f,
            snapshot.revision.clone(),
            &format!("{RUN}/Alpha/nope"),
            json!(CANARY),
        )
        .await,
        set(
            &f,
            snapshot.revision.clone(),
            &format!("{RUN}/Alpha/is_default"),
            json!(CANARY),
        )
        .await,
    ] {
        texts.push(match result {
            Ok(reply) => serde_json::to_string(&reply).unwrap(),
            Err(error) => serde_json::to_string(&error).unwrap(),
        });
    }
    let reply = set(
        &f,
        revision(&f).await,
        &format!("{RUN}/Beta/args"),
        json!(["--c"]),
    )
    .await
    .unwrap();
    texts.push(serde_json::to_string(&reply).unwrap());
    for text in texts {
        assert!(!text.contains("canary-agent-env"), "{text}");
    }
}

/// Reality: a patch persists via the owner file, refreshes the launch-instruction files the
/// way `save_agents_config` does, and an immediate readback shows it.
#[tokio::test]
async fn patch_persists_refreshes_launch_files_and_reads_back() {
    let f = fixture(Some(initial()));
    let before = get(&f, &["/launch_instructions"]).await.unwrap();
    assert_eq!(before.values["/launch_instructions"].stored, Value::Null);
    let reply = set(
        &f,
        before.revision.clone(),
        "/launch_instructions",
        json!("MARKER-launch-4821"),
    )
    .await
    .unwrap();
    assert!(reply.persisted);
    assert_eq!(
        on_disk(&f)["launch_instructions"],
        json!("MARKER-launch-4821")
    );
    let after = get(&f, &["/launch_instructions"]).await.unwrap();
    assert_eq!(
        after.values["/launch_instructions"].stored,
        json!("MARKER-launch-4821")
    );
    assert_eq!(after.revision, reply.revision);
    let written = std::fs::read_dir(f.dir.path().join("agent-hooks"))
        .unwrap()
        .any(|e| {
            std::fs::read_to_string(e.unwrap().path())
                .is_ok_and(|text| text.contains("MARKER-launch-4821"))
        });
    assert!(written, "launch instruction files follow the edit");
    let stale = set(&f, before.revision, "/launch_instructions", json!("x"))
        .await
        .unwrap_err();
    assert_eq!(stale.code, "conflict");
}

/// Catches: corrupt JSON read as defaults, then overwritten by the next edit.
#[tokio::test]
async fn corrupt_file_is_an_error_not_defaults() {
    let f = fixture(None);
    std::fs::write(f.dir.path().join("agents.json"), "{not json").unwrap();
    let error = get(&f, &["/headless_agent"]).await.unwrap_err();
    assert_eq!(error.code, "store_failed");
}
