//! Remote adapter regressions at the real `connections.json` / `tunnels/*.toml` owner boundaries
//! and the real credential vault, driven through the registry the other adapters plug into.
use super::{
    native::native_catalogue,
    registry::Registry,
    remote::{ConnectionsSettingsStore, OnConnectionRemoved, OnRemoved, TunnelsSettingsStore},
    request::*,
    service::Service,
};
use crate::remote_connection::{
    RemoteConnection, connection_password_exists, pairing_token, set_connection_password,
    set_pairing_token,
};
use crate::tunnels::{profile::TunnelProfile, storage::ProfileStore};
use parking_lot::Mutex;
use serde_json::{Value, json};
use std::sync::Arc;

const CANARY: &str = "canary-remote-secret-5e8b";

struct Fixture {
    // Declared first: the override guard drops before the directory it points at.
    _guard: Box<dyn std::any::Any>,
    dir: tempfile::TempDir,
    removed: Arc<Mutex<Vec<String>>>,
    registry: Registry,
    state: Arc<crate::AppState>,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    // The vault resolves under the global config dir.
    let guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    let removed = Arc::new(Mutex::new(Vec::new()));
    let sink = removed.clone();
    let on_removed: OnConnectionRemoved =
        Arc::new(move |record| sink.lock().push(record.id.clone()));
    let sink = removed.clone();
    let on_tunnel_removed: OnRemoved = Arc::new(move |id| sink.lock().push(id.to_owned()));
    let catalogue = Arc::new(native_catalogue().unwrap());
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let mut registry = Registry::with_policy(state.settings_policy.clone());
    registry
        .register(Service::new(
            catalogue.clone(),
            Arc::new(ConnectionsSettingsStore::new(
                dir.path().to_path_buf(),
                on_removed.clone(),
            )),
        ))
        .unwrap();
    registry
        .register(Service::new(
            catalogue,
            Arc::new(TunnelsSettingsStore::new(
                dir.path().to_path_buf(),
                on_tunnel_removed,
            )),
        ))
        .unwrap();
    Fixture {
        _guard: Box::new(guard),
        dir,
        removed,
        registry,
        state,
    }
}

fn ssh(name: &str) -> RemoteConnection {
    RemoteConnection::new_ssh(name, "host.example", "alice")
}

fn seed_connections(f: &Fixture, connections: &[RemoteConnection]) {
    crate::remote_connection::RemoteConnectionStore::save(f.dir.path(), connections).unwrap();
}

async fn get(f: &Fixture, store: &str, paths: &[&str]) -> Result<Snapshot, Failure> {
    match f
        .registry
        .execute(Request::Get {
            target: Target::Machine,
            store: store.into(),
            paths: paths.iter().map(|p| (*p).into()).collect(),
        })
        .await?
    {
        Reply::Snapshot(s) => Ok(s),
        Reply::Commit(_) => panic!("get returned a commit"),
    }
}

async fn patch(
    f: &Fixture,
    store: &str,
    revision: String,
    operations: Vec<Edit>,
) -> Result<CommitReply, Failure> {
    let request = Request::Patch {
        target: Target::Machine,
        store: store.into(),
        expected_revision: revision,
        operations,
    };
    let mut result = f.registry.execute(request.clone()).await;
    if let Err(error) = &result
        && error.code == "confirmation_required"
    {
        let id = error.proposal_id.clone().unwrap();
        let caller = super::policy::Caller::native();
        assert_eq!(
            super::policy_tests::answer(f.state.clone(), caller.clone(), id.clone(), true).await["confirmed"],
            json!(true)
        );
        result = f.registry.execute_as(request, &caller, Some(&id)).await;
    }
    match result? {
        Reply::Commit(c) => Ok(c),
        Reply::Snapshot(_) => panic!("patch returned a snapshot"),
    }
}

async fn set(
    f: &Fixture,
    store: &str,
    revision: String,
    path: &str,
    value: Value,
) -> Result<CommitReply, Failure> {
    patch(
        f,
        store,
        revision,
        vec![Edit::Set {
            path: path.into(),
            value,
        }],
    )
    .await
}

async fn revision(f: &Fixture, store: &str, probe: &str) -> String {
    get(f, store, &[probe]).await.unwrap().revision
}

fn connections_on_disk(f: &Fixture) -> Value {
    serde_json::from_str(&std::fs::read_to_string(f.dir.path().join("connections.json")).unwrap())
        .unwrap()
}

/// Catches: a connection edit or removal orphaning the vault secret that is bound to its id. An
/// edit must keep the password and pairing token reachable; only a removal deletes them, and the
/// runtime is told so it stops what it holds for the id.
#[tokio::test]
async fn remote_edit_preserves_credential_reference() {
    let f = fixture();
    let conn = ssh("work");
    let id = conn.id.clone();
    seed_connections(&f, &[conn]);
    set_connection_password(&id, CANARY).unwrap();
    set_pairing_token(&id, CANARY).unwrap();
    let probe = format!("/connections/{id}/name");
    let rev = revision(&f, "connections", &probe).await;

    let reply = patch(
        &f,
        "connections",
        rev,
        vec![
            Edit::Set {
                path: probe.clone(),
                value: json!("renamed"),
            },
            Edit::Set {
                path: format!("/connections/{id}/transport/ssh_port"),
                value: json!(2222),
            },
        ],
    )
    .await
    .unwrap();
    assert!(reply.persisted && reply.applied);
    assert_eq!(connections_on_disk(&f)[0]["id"], json!(id));
    assert!(connection_password_exists(&id).unwrap());
    assert!(pairing_token(&id).unwrap().is_some());
    assert!(f.removed.lock().is_empty(), "an edit tears nothing down");

    let id_edit = set(
        &f,
        "connections",
        reply.revision.clone(),
        &format!("/connections/{id}/id"),
        json!(uuid::Uuid::new_v4().to_string()),
    )
    .await;
    assert!(
        id_edit.is_err(),
        "the id is the vault binding and is immutable"
    );
    assert!(connection_password_exists(&id).unwrap());

    let reply = patch(
        &f,
        "connections",
        reply.revision,
        vec![Edit::Remove {
            path: "/connections".into(),
            id: id.clone(),
        }],
    )
    .await
    .unwrap();
    assert!(reply.persisted && reply.applied);
    assert_eq!(*f.removed.lock(), vec![id.clone()]);
    assert!(!connection_password_exists(&id).unwrap());
    assert!(pairing_token(&id).unwrap().is_none());
    assert_eq!(connections_on_disk(&f), json!([]));
}

/// Catches: an edit clobbering a sibling connection, a sibling field, or another store's file.
#[tokio::test]
async fn adapter_preserves_other_fields() {
    let f = fixture();
    let (a, b) = (ssh("a"), ssh("b"));
    let (a_id, b_id) = (a.id.clone(), b.id.clone());
    seed_connections(&f, &[a, b]);
    let profile = TunnelProfile::new("tun", "t.example", "bob");
    ProfileStore::save(f.dir.path(), &profile).unwrap();
    let tunnel_file = f
        .dir
        .path()
        .join("tunnels")
        .join(format!("{}.toml", profile.id));
    let tunnel_before = std::fs::read_to_string(&tunnel_file).unwrap();
    let neighbour = f.dir.path().join("notifications.json");
    std::fs::write(&neighbour, "{\"sentinel\":true}").unwrap();

    let probe = format!("/connections/{b_id}/name");
    let rev = revision(&f, "connections", &probe).await;
    set(&f, "connections", rev, &probe, json!("b2"))
        .await
        .unwrap();

    let disk = connections_on_disk(&f);
    assert_eq!(disk[0]["id"], json!(a_id));
    assert_eq!(disk[0]["name"], json!("a"));
    assert_eq!(disk[0]["transport"]["ssh_host"], json!("host.example"));
    assert_eq!(disk[1]["name"], json!("b2"));
    assert_eq!(disk[1]["auth_username"], json!("alice"));
    assert_eq!(
        std::fs::read_to_string(&tunnel_file).unwrap(),
        tunnel_before
    );
    assert_eq!(
        std::fs::read_to_string(&neighbour).unwrap(),
        "{\"sentinel\":true}"
    );
}

/// Catches: a remote edit changing local-only state — the connection store writing the tunnel
/// profiles, or the tunnel store writing connections — or an edit that merely renames something
/// stopping the live runtime for it.
#[tokio::test]
async fn remote_write_cannot_touch_local() {
    let f = fixture();
    let conn = ssh("work");
    let profile = TunnelProfile::new("tun", "t.example", "bob");
    seed_connections(&f, std::slice::from_ref(&conn));
    ProfileStore::save(f.dir.path(), &profile).unwrap();
    let connections_before =
        std::fs::read_to_string(f.dir.path().join("connections.json")).unwrap();
    let tunnel_file = f
        .dir
        .path()
        .join("tunnels")
        .join(format!("{}.toml", profile.id));
    let tunnel_before = std::fs::read_to_string(&tunnel_file).unwrap();

    let probe = format!("/profiles/{}/name", profile.id);
    let rev = revision(&f, "tunnels", &probe).await;
    set(&f, "tunnels", rev, &probe, json!("tun2"))
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join("connections.json")).unwrap(),
        connections_before
    );

    let probe = format!("/connections/{}/name", conn.id);
    let rev = revision(&f, "connections", &probe).await;
    set(&f, "connections", rev, &probe, json!("work2"))
        .await
        .unwrap();
    let tunnel_after = std::fs::read_to_string(&tunnel_file).unwrap();
    assert_ne!(tunnel_after, tunnel_before, "the tunnel edit itself landed");
    assert!(tunnel_after.contains("tun2"));
    assert!(f.removed.lock().is_empty());
}

/// Catches: a vault secret appearing in a read, reply or error.
#[tokio::test]
async fn adapter_secret_fields_presence_only() {
    let f = fixture();
    let conn = ssh("work");
    let id = conn.id.clone();
    seed_connections(&f, &[conn]);
    set_connection_password(&id, CANARY).unwrap();
    set_pairing_token(&id, CANARY).unwrap();
    let probe = format!("/connections/{id}/name");
    let snapshot = get(&f, "connections", &[&probe]).await.unwrap();
    let mut texts = vec![serde_json::to_string(&snapshot).unwrap()];
    for result in [
        set(&f, "connections", "stale".into(), &probe, json!(CANARY)).await,
        set(
            &f,
            "connections",
            snapshot.revision.clone(),
            &format!("/connections/{id}/password"),
            json!(CANARY),
        )
        .await,
        set(
            &f,
            "connections",
            snapshot.revision.clone(),
            &format!("/connections/{id}/transport/ssh_port"),
            json!(CANARY),
        )
        .await,
    ] {
        texts.push(match result {
            Ok(reply) => serde_json::to_string(&reply).unwrap(),
            Err(error) => serde_json::to_string(&error).unwrap(),
        });
    }
    let reply = set(&f, "connections", snapshot.revision, &probe, json!("ok"))
        .await
        .unwrap();
    texts.push(serde_json::to_string(&reply).unwrap());
    for text in texts {
        assert!(!text.contains("canary-remote-secret"), "{text}");
    }
    assert!(
        !std::fs::read_to_string(f.dir.path().join("connections.json"))
            .unwrap()
            .contains("canary-remote-secret"),
        "secrets stay in the vault"
    );
}

/// Reality: a patch persists via the owner files, a create is validated by the owner, and an
/// immediate readback shows both.
#[tokio::test]
async fn patch_persists_and_reads_back() {
    let f = fixture();
    let conn = ssh("work");
    seed_connections(&f, std::slice::from_ref(&conn));
    let probe = format!("/connections/{}/enabled", conn.id);
    let before = get(&f, "connections", &[&probe]).await.unwrap();
    let reply = set(
        &f,
        "connections",
        before.revision.clone(),
        &probe,
        json!(false),
    )
    .await
    .unwrap();
    assert!(reply.persisted);
    assert_eq!(connections_on_disk(&f)[0]["enabled"], json!(false));
    let after = get(&f, "connections", &[&probe]).await.unwrap();
    assert_eq!(after.values[&probe].stored, json!(false));
    assert_eq!(after.revision, reply.revision);
    let stale = set(&f, "connections", before.revision, &probe, json!(true))
        .await
        .unwrap_err();
    assert_eq!(stale.code, "conflict");

    // Tunnel profile: edit lands in its own TOML file and reads back.
    let profile = TunnelProfile::new("tun", "t.example", "bob");
    ProfileStore::save(f.dir.path(), &profile).unwrap();
    let probe = format!("/profiles/{}/port", profile.id);
    let rev = revision(&f, "tunnels", &probe).await;
    set(&f, "tunnels", rev, &probe, json!(2200)).await.unwrap();
    let saved = ProfileStore::load_all(f.dir.path(), None).unwrap();
    assert_eq!(saved[0].port, 2200);
    assert_eq!(
        get(&f, "tunnels", &[&probe]).await.unwrap().values[&probe].stored,
        json!(2200)
    );
}

/// Catches: an edit the owner would refuse (empty name, port 0, malformed id) being written
/// because the adapter skipped `validate()`.
#[tokio::test]
async fn invalid_records_are_rejected_before_any_write() {
    let f = fixture();
    let conn = ssh("work");
    let profile = TunnelProfile::new("tun", "t.example", "bob");
    seed_connections(&f, std::slice::from_ref(&conn));
    ProfileStore::save(f.dir.path(), &profile).unwrap();
    let before_connections =
        std::fs::read_to_string(f.dir.path().join("connections.json")).unwrap();

    let name = format!("/connections/{}/name", conn.id);
    let rev = revision(&f, "connections", &name).await;
    let error = set(&f, "connections", rev, &name, json!("  "))
        .await
        .unwrap_err();
    assert_eq!(error.code, "out_of_range");
    let port = format!("/profiles/{}/port", profile.id);
    let rev = revision(&f, "tunnels", &port).await;
    let error = set(&f, "tunnels", rev, &port, json!(0)).await.unwrap_err();
    assert_eq!(error.code, "out_of_range");
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join("connections.json")).unwrap(),
        before_connections
    );
    assert_eq!(
        ProfileStore::load_all(f.dir.path(), None).unwrap()[0].port,
        22
    );
}

/// Catches: a tunnel profile edit dropping the forwards (an inventory gap the adapter cannot
/// address yet) or removing the profile file without stopping a running tunnel.
#[tokio::test]
async fn tunnel_edit_keeps_forwards_and_removal_stops_the_tunnel() {
    let f = fixture();
    let mut profile = TunnelProfile::new("tun", "t.example", "bob");
    profile.forwards = vec![crate::tunnels::profile::ForwardSpec::Local {
        bind_port: 8080,
        remote_host: "localhost".into(),
        remote_port: 80,
    }];
    ProfileStore::save(f.dir.path(), &profile).unwrap();
    let probe = format!("/profiles/{}/name", profile.id);
    let rev = revision(&f, "tunnels", &probe).await;
    let reply = set(&f, "tunnels", rev, &probe, json!("tun2"))
        .await
        .unwrap();
    let saved = ProfileStore::load_all(f.dir.path(), None).unwrap();
    assert_eq!(saved[0].name, "tun2");
    assert_eq!(saved[0].forwards.len(), 1);
    assert!(f.removed.lock().is_empty());

    let reply = patch(
        &f,
        "tunnels",
        reply.revision,
        vec![Edit::Remove {
            path: "/profiles".into(),
            id: profile.id.clone(),
        }],
    )
    .await
    .unwrap();
    assert!(reply.persisted && reply.applied);
    assert!(
        ProfileStore::load_all(f.dir.path(), None)
            .unwrap()
            .is_empty()
    );
    assert_eq!(*f.removed.lock(), vec![profile.id]);
}

/// Catches: corrupt JSON read as an empty list, then overwritten by the next edit.
#[tokio::test]
async fn corrupt_file_is_an_error_not_defaults() {
    let f = fixture();
    std::fs::write(f.dir.path().join("connections.json"), "{not json").unwrap();
    let error = get(&f, "connections", &["/connections"]).await.unwrap_err();
    assert_eq!(error.code, "store_failed");
}

// Catches: post-persistence teardown loses the removed connection's daemon authority.
#[tokio::test]
async fn removed_connection_retains_daemon_authority() {
    let f = fixture();
    let mut connection = ssh("ephemeral");
    connection.deploy = crate::remote_connection::DeployMode::OnConnect;
    let path = f.dir.path().to_path_buf();
    let received = Arc::new(Mutex::new(None));
    let sink = received.clone();
    let store = Arc::new(ConnectionsSettingsStore::new(
        path.clone(),
        Arc::new(move |record| {
            assert!(
                crate::remote_connection::RemoteConnectionStore::load(&path)
                    .unwrap()
                    .is_empty()
            );
            *sink.lock() = Some(record.clone());
        }),
    ));
    let service = Service::new(Arc::new(native_catalogue().unwrap()), store);
    seed_connections(&f, std::slice::from_ref(&connection));
    let snapshot = service
        .get(
            Target::Machine,
            "connections".into(),
            vec![format!("/connections/{}/name", connection.id)],
        )
        .await
        .unwrap();
    service
        .patch(
            Target::Machine,
            "connections".into(),
            snapshot.revision,
            vec![Edit::Remove {
                path: "/connections".into(),
                id: connection.id.clone(),
            }],
        )
        .await
        .unwrap();
    assert_eq!(connections_on_disk(&f), json!([]));
    assert_eq!(
        serde_json::to_value(received.lock().as_ref()).unwrap(),
        serde_json::to_value(Some(&connection)).unwrap(),
        "teardown must receive the removed record even after persistence"
    );
}
