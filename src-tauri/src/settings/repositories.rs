//! Repositories owner seam: `repo-settings.json` is written only through `ConfigFile`, so a settings
//! edit takes the same locks as every other writer of that file. An edit can only address a repository
//! that already has an entry; the adapter never creates one and never opens a repository's own files.
use super::{
    request::*,
    service::Store,
    transaction::{self, Prepared},
};
use crate::config::{ConfigFile, REPO_SETTINGS_FILE, RepoSettingsMap};
use serde_json::Value;

const ID: &str = "repositories";

pub struct RepositoriesSettingsStore {
    file: ConfigFile<RepoSettingsMap>,
}

impl Default for RepositoriesSettingsStore {
    fn default() -> Self {
        Self {
            file: ConfigFile::new(REPO_SETTINGS_FILE),
        }
    }
}

impl RepositoriesSettingsStore {
    #[cfg(test)]
    pub(crate) fn at_path(path: std::path::PathBuf) -> Self {
        Self {
            file: ConfigFile::at_path(path),
        }
    }
}

fn to_value(config: &RepoSettingsMap) -> Result<Value, Failure> {
    serde_json::to_value(config).map_err(|_| Error::new(ID, "store_failed"))
}

impl Store for RepositoriesSettingsStore {
    fn id(&self) -> &str {
        ID
    }
    fn target(&self) -> Target {
        Target::Instance
    }

    fn read(&self) -> Result<Value, Failure> {
        // Strict: a corrupt file is an error, never silently the defaults.
        self.file
            .update_with_strict(|config| Ok((to_value(config), false)))
            .map_err(|_| Error::new(ID, "store_failed"))?
    }

    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure> {
        let mut failure = None;
        let result = self.file.update_with_strict(|config| {
            let latest = to_value(config).map_err(|e| {
                failure = Some(e);
                "unreadable".to_owned()
            })?;
            let Prepared { value, mut reply } = prepare(&latest).map_err(|e| {
                failure = Some(e);
                "rejected".to_owned()
            })?;
            let next: RepoSettingsMap = serde_json::from_value(value).map_err(|_| {
                failure = Some(Error::new(ID, "invalid_type"));
                "invalid".to_owned()
            })?;
            let canonical = to_value(&next).map_err(|e| {
                failure = Some(e);
                "unreadable".to_owned()
            })?;
            reply.revision = transaction::revision(&canonical).map_err(|e| {
                failure = Some(e);
                "unreadable".to_owned()
            })?;
            if canonical == latest {
                reply.changed_ids.clear();
                reply.effects.clear();
                reply.restart.clear();
            }
            let changed = !reply.changed_ids.is_empty();
            reply.persisted = changed;
            if changed {
                *config = next;
            }
            Ok((reply, changed))
        });
        result.map_err(|_| failure.unwrap_or_else(|| Error::new(ID, "store_failed")))
    }

    fn apply(
        &self,
        _: &CommitReply,
    ) -> impl std::future::Future<Output = Result<bool, Failure>> + Send {
        // Live-only: every reader loads repo-settings.json on demand; no backend state caches it.
        std::future::ready(Ok(true))
    }
}
