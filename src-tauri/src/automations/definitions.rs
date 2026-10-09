use super::model::{AutomationDefinition, AutomationsConfig};
use std::path::PathBuf;

pub struct DefinitionStore;

impl DefinitionStore {
    pub fn at_path(_path: PathBuf) -> Self { Self }
    pub fn load(&self) -> Result<AutomationsConfig, String> { Err("Automation definition storage is not implemented".into()) }
    pub fn create(&self, _definition: AutomationDefinition) -> Result<(), String> { Err("Automation definition storage is not implemented".into()) }
    pub fn update(&self, _id: &str, _definition: AutomationDefinition) -> Result<(), String> { Err("Automation definition storage is not implemented".into()) }
    pub fn remove(&self, _id: &str) -> Result<(), String> { Err("Automation definition storage is not implemented".into()) }
    pub fn set_concurrency(&self, _limit: u32) -> Result<(), String> { Err("Automation definition storage is not implemented".into()) }
}
