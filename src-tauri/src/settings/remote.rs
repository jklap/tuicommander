//! Remote owner seams: `connections.json` and the global `tunnels/*.toml` profiles are written
//! only through their existing owners (`ConfigFile` locks, `ProfileStore`). The catalogue
//! describes each list as a wrapper record keyed by `id`; the files keep their own shapes.
//! Vault secrets (connection password, pairing token) are never read or written here: they are
//! bound to the immutable connection id, so an edit keeps them and only a removal deletes them.
use super::{
    request::*,
    service::Store,
    transaction::{self, Prepared},
};
use crate::config::ConfigFile;
use crate::remote_connection::{RemoteConnection, delete_connection_credentials};
use crate::tunnels::{profile::TunnelProfile, storage::ProfileStore};
use parking_lot::Mutex;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

const CONNECTIONS_ID: &str = "connections";
const TUNNELS_ID: &str = "tunnels";
const CONNECTIONS_FILE: &str = "connections.json";

/// Schema of the `connections` store: the records of `connections.json`.
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize, schemars::JsonSchema)]
pub(crate) struct RemoteConnections {
    #[serde(default)]
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates connections for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}, "x-collection-key" = "id"))]
    pub(crate) connections: Vec<RemoteConnection>,
}

/// Schema of the `tunnels` store: the global-scope profiles. Repository-scope profiles live in
/// each repository and are not part of this store.
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize, schemars::JsonSchema)]
pub(crate) struct TunnelProfiles {
    #[serde(default)]
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates profiles for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}, "x-collection-key" = "id"))]
    pub(crate) profiles: Vec<TunnelProfile>,
}

/// Called with the id of a removed record so the runtime stops what it holds for it.
pub type OnRemoved = Arc<dyn Fn(&str) + Send + Sync>;

/// Records of one list, as the catalogue sees them.
fn document<T: Serialize>(key: &str, records: &[T], id: &str) -> Result<Value, Failure> {
    let records = serde_json::to_value(records).map_err(|_| Error::new(id, "store_failed"))?;
    Ok(json!({ key: records }))
}

struct Plan<T> {
    next: Vec<T>,
    reply: CommitReply,
    changed: bool,
}

/// Run `prepare` on the latest records and settle the reply. `validate` is the owner's own gate
/// and may normalize (trim) the records before they are compared and persisted.
fn plan<T: Serialize + DeserializeOwned>(
    id: &str,
    key: &str,
    latest: &[T],
    prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    validate: impl FnOnce(&mut [T]) -> Result<(), String>,
) -> Result<Plan<T>, Failure> {
    let latest_document = document(key, latest, id)?;
    let Prepared { value, mut reply } = prepare(&latest_document)?;
    let mut next: Vec<T> = serde_json::from_value(value.get(key).cloned().unwrap_or(Value::Null))
        .map_err(|_| Error::new(id, "invalid_type"))?;
    validate(&mut next).map_err(|_| Error::new(id, "out_of_range"))?;
    let canonical = document(key, &next, id)?;
    reply.revision = transaction::revision(&canonical)?;
    if canonical == latest_document {
        reply.changed_ids.clear();
        reply.effects.clear();
        reply.restart.clear();
    }
    let changed = !reply.changed_ids.is_empty();
    reply.persisted = changed;
    Ok(Plan {
        next,
        reply,
        changed,
    })
}

fn removed_ids<T>(latest: &[T], next: &[T], id_of: impl Fn(&T) -> &str) -> Vec<String> {
    latest
        .iter()
        .map(&id_of)
        .filter(|id| !next.iter().any(|n| id_of(n) == *id))
        .map(str::to_owned)
        .collect()
}

pub struct ConnectionsSettingsStore {
    path: PathBuf,
    on_removed: OnRemoved,
    /// Removed ids of each commit, keyed by the revision it produced, until `apply` runs.
    pending: Mutex<HashMap<String, Vec<String>>>,
}

impl ConnectionsSettingsStore {
    pub fn new(data_dir: PathBuf, on_removed: OnRemoved) -> Self {
        Self::at_path(data_dir.join(CONNECTIONS_FILE), on_removed)
    }

    pub(crate) fn at_path(path: PathBuf, on_removed: OnRemoved) -> Self {
        Self {
            path,
            on_removed,
            pending: Mutex::default(),
        }
    }

    fn file(&self) -> ConfigFile<Vec<RemoteConnection>> {
        ConfigFile::at_path(self.path.clone())
    }
}

impl Store for ConnectionsSettingsStore {
    fn id(&self) -> &str {
        CONNECTIONS_ID
    }
    fn target(&self) -> Target {
        Target::Machine
    }

    fn read(&self) -> Result<Value, Failure> {
        // Strict: a corrupt file is an error, never silently an empty list.
        let records = self
            .file()
            .update_with_strict(|records| Ok((records.clone(), false)))
            .map_err(|_| Error::new(CONNECTIONS_ID, "store_failed"))?;
        document("connections", &records, CONNECTIONS_ID)
    }

    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure> {
        let mut failure = None;
        let result = self.file().update_with_strict(|records| {
            let settled = plan(CONNECTIONS_ID, "connections", records, prepare, |next| {
                next.iter().try_for_each(RemoteConnection::validate)
            })
            .map_err(|e| {
                failure = Some(e);
                "rejected".to_owned()
            })?;
            let Plan {
                next,
                reply,
                changed,
            } = settled;
            if changed {
                let removed = removed_ids(records, &next, |c| c.id.as_str());
                self.pending.lock().insert(reply.revision.clone(), removed);
                *records = next;
            }
            Ok((reply, changed))
        });
        result.map_err(|_| failure.unwrap_or_else(|| Error::new(CONNECTIONS_ID, "store_failed")))
    }

    async fn apply(&self, reply: &CommitReply) -> Result<bool, Failure> {
        let removed = self.pending.lock().remove(&reply.revision);
        let mut complete = true;
        for id in removed.into_iter().flatten() {
            (self.on_removed)(&id);
            // The vault key is the connection id: a secret left behind names nothing reachable.
            complete &= delete_connection_credentials(&id).is_ok();
        }
        Ok(complete)
    }
}

pub struct TunnelsSettingsStore {
    data_dir: PathBuf,
    on_removed: OnRemoved,
    /// `ProfileStore` has no lock of its own; this one orders settings commits among themselves.
    lock: Mutex<()>,
    pending: Mutex<HashMap<String, Vec<String>>>,
}

impl TunnelsSettingsStore {
    pub fn new(data_dir: PathBuf, on_removed: OnRemoved) -> Self {
        Self {
            data_dir,
            on_removed,
            lock: Mutex::new(()),
            pending: Mutex::default(),
        }
    }

    fn load(&self) -> Result<Vec<TunnelProfile>, Failure> {
        let mut profiles = ProfileStore::load_all(&self.data_dir, None)
            .map_err(|_| Error::new(TUNNELS_ID, "store_failed"))?;
        profiles.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(profiles)
    }
}

impl Store for TunnelsSettingsStore {
    fn id(&self) -> &str {
        TUNNELS_ID
    }
    fn target(&self) -> Target {
        Target::Machine
    }

    fn read(&self) -> Result<Value, Failure> {
        document("profiles", &self.load()?, TUNNELS_ID)
    }

    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure> {
        let _ordered = self.lock.lock();
        let latest = self.load()?;
        let Plan {
            mut next,
            reply,
            changed,
        } = plan(TUNNELS_ID, "profiles", &latest, prepare, |next| {
            next.iter_mut().try_for_each(TunnelProfile::validate)
        })?;
        if changed {
            next.sort_by(|a, b| a.id.cmp(&b.id));
            let before = |id: &str| latest.iter().find(|p| p.id == id);
            for profile in &next {
                let same = before(&profile.id).is_some_and(|old| {
                    serde_json::to_value(old).ok() == serde_json::to_value(profile).ok()
                });
                if !same {
                    ProfileStore::save(&self.data_dir, profile)
                        .map_err(|_| Error::new(TUNNELS_ID, "store_failed"))?;
                }
            }
            let removed = removed_ids(&latest, &next, |p| p.id.as_str());
            for id in &removed {
                ProfileStore::delete(&self.data_dir, None, id)
                    .map_err(|_| Error::new(TUNNELS_ID, "store_failed"))?;
            }
            self.pending.lock().insert(reply.revision.clone(), removed);
        }
        Ok(reply)
    }

    async fn apply(&self, reply: &CommitReply) -> Result<bool, Failure> {
        for id in self
            .pending
            .lock()
            .remove(&reply.revision)
            .into_iter()
            .flatten()
        {
            (self.on_removed)(&id);
        }
        Ok(true)
    }
}
