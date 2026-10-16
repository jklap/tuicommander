//! Agents owner seam: `agents.json` is written only through `ConfigFile`, so a settings edit takes
//! the same locks as every other writer, then refreshes the launch-instruction files the same way
//! `save_agents_config` does.
use super::{config_file::ConfigFileStore, request::*, service::Store, transaction::Prepared};
use crate::config::{AGENTS_CONFIG_FILE, AgentsConfig, ConfigFile};
use serde_json::Value;

const ID: &str = "agents";

pub struct AgentsSettingsStore {
    file: ConfigFileStore<AgentsConfig>,
}

impl Default for AgentsSettingsStore {
    fn default() -> Self {
        Self {
            file: ConfigFileStore::new(ID, ConfigFile::new(AGENTS_CONFIG_FILE)),
        }
    }
}

impl AgentsSettingsStore {
    #[cfg(test)]
    pub(crate) fn at_path(path: std::path::PathBuf) -> Self {
        Self {
            file: ConfigFileStore::new(ID, ConfigFile::at_path(path)),
        }
    }
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
        self.file.read()
    }

    async fn refresh_choices(&self, resolver: &str, path: &str) -> Result<Vec<Value>, Failure> {
        if resolver != "agent_models" {
            return Err(Error::new(ID, "unavailable"));
        }
        let parts = super::pointer::segments(path, ID)?;
        let [agents, agent, configs, name, model] = parts.as_slice() else {
            return Err(Error::new(ID, "unknown_path"));
        };
        if agents != "agents" || configs != "run_configs" || model != "model" {
            return Err(Error::new(ID, "unknown_path"));
        }
        // Claude publishes model aliases in --help. Other CLIs have no verified
        // discovery contract here; typed model edits remain available for all owners.
        if agent != "claude" {
            return Err(Error::new(ID, "unavailable"));
        }
        let state: AgentsConfig =
            serde_json::from_value(self.read()?).map_err(|_| Error::new(ID, "store_failed"))?;
        let run = state
            .agents
            .get(agent)
            .and_then(|settings| settings.run_configs.iter().find(|run| run.name == *name))
            .ok_or_else(|| Error::new(ID, "not_found"))?;
        let mut command = tokio::process::Command::new(&run.command);
        command.arg("--help").envs(&run.env).kill_on_drop(true);
        let output = tokio::time::timeout(std::time::Duration::from_secs(10), command.output())
            .await
            .map_err(|_| Error::new(ID, "unavailable"))?
            .map_err(|_| Error::new(ID, "unavailable"))?;
        if !output.status.success() {
            return Err(Error::new(ID, "unavailable"));
        }
        model_aliases(&String::from_utf8_lossy(&output.stdout))
    }

    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure> {
        self.file.commit(prepare, single_default)
    }

    async fn apply(&self, _: &CommitReply) -> Result<bool, Failure> {
        // Launches read the instruction files; the config itself is re-read on every launch.
        tokio::task::spawn_blocking(crate::config::refresh_launch_instructions)
            .await
            .map_err(|_| Error::new(ID, "store_failed"))?;
        Ok(true)
    }
}

/// Only aliases published by the selected CLI are advertised, never an invented model list.
fn model_aliases(help: &str) -> Result<Vec<Value>, Failure> {
    let section = help
        .split_once("--model <model>")
        .and_then(|(_, rest)| rest.split_once("model's full name."))
        .map(|(section, _)| section)
        .ok_or_else(|| Error::new(ID, "unavailable"))?;
    let values: Vec<_> = section
        .split('\'')
        .enumerate()
        .filter(|(index, _)| index % 2 == 1)
        .map(|(_, alias)| Value::String(alias.to_owned()))
        .collect();
    if values.is_empty() {
        return Err(Error::new(ID, "unavailable"));
    }
    Ok(values)
}
