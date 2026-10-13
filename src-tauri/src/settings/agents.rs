//! Agents owner seam: `agents.json` is written only through `ConfigFile`, so a settings edit takes
//! the same locks as every other writer, then refreshes the launch-instruction files the same way
//! `save_agents_config` does.
use super::{
    request::*,
    service::Store,
    transaction::{self, Prepared},
};
use crate::config::{AGENTS_CONFIG_FILE, AgentsConfig, ConfigFile};
use serde_json::Value;

const ID: &str = "agents";

pub struct AgentsSettingsStore {
    file: ConfigFile<AgentsConfig>,
}

impl Default for AgentsSettingsStore {
    fn default() -> Self {
        Self {
            file: ConfigFile::new(AGENTS_CONFIG_FILE),
        }
    }
}

impl AgentsSettingsStore {
    #[cfg(test)]
    pub(crate) fn at_path(path: std::path::PathBuf) -> Self {
        Self {
            file: ConfigFile::at_path(path),
        }
    }
}

fn to_value(config: &AgentsConfig) -> Result<Value, Failure> {
    serde_json::to_value(config).map_err(|_| Error::new(ID, "store_failed"))
}

/// Launches use the first `is_default` run config, so promoting one must demote the rest;
/// the settings UI keeps the same single-default invariant.
fn single_default(latest: &AgentsConfig, next: &mut AgentsConfig) {
    for (id, settings) in &mut next.agents {
        let was_default = |name: &str| {
            latest.agents.get(id).is_some_and(|old| {
                old.run_configs
                    .iter()
                    .any(|rc| rc.is_default && rc.name == name)
            })
        };
        let promoted = settings
            .run_configs
            .iter()
            .position(|rc| rc.is_default && !was_default(&rc.name));
        if let Some(keep) = promoted {
            for (i, rc) in settings.run_configs.iter_mut().enumerate() {
                rc.is_default = i == keep;
            }
        }
    }
}

impl Store for AgentsSettingsStore {
    fn id(&self) -> &str {
        ID
    }
    fn target(&self) -> Target {
        Target::Machine
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
            let mut next: AgentsConfig = serde_json::from_value(value).map_err(|_| {
                failure = Some(Error::new(ID, "invalid_type"));
                "invalid".to_owned()
            })?;
            single_default(config, &mut next);
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

    async fn apply(&self, _: &CommitReply) -> Result<bool, Failure> {
        // Launches read the instruction files; the config itself is re-read on every launch.
        tokio::task::spawn_blocking(crate::config::refresh_launch_instructions)
            .await
            .map_err(|_| Error::new(ID, "store_failed"))?;
        Ok(true)
    }
}
