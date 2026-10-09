//! Stored scheduler definitions. Schedule interpretation belongs to `schedule`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationsConfig {
    pub version: u32,
    pub max_concurrent_runs: u32,
    pub definitions: Vec<AutomationDefinition>,
}

impl Default for AutomationsConfig {
    fn default() -> Self {
        Self { version: 1, max_concurrent_runs: 2, definitions: Vec::new() }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationDefinition {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub run_config: String,
    pub repository: String,
    pub workspace: Workspace,
    pub cron: String,
    pub timezone: String,
    pub enabled: bool,
    pub grace_secs: u64,
    pub overlap: Overlap,
    pub max_duration_secs: u64,
    pub precheck: Option<Precheck>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Workspace {
    Existing,
    NewPerRun { base_branch: String },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Overlap { Skip }

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Precheck {
    pub command: String,
    pub timeout_secs: u64,
}
