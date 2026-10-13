//! Narrow launch-default adapter over the existing AppConfig owner (including vault handling).
use super::{request::*, service::Store, transaction::Prepared};
use crate::{
    AppState,
    config::{AppConfig, settings_app::AppSettingsStore},
};
use serde_json::Value;
use std::sync::Arc;

const ID: &str = "ego_launch";

pub struct EgoLaunchSettingsStore {
    owner: AppSettingsStore,
}

impl EgoLaunchSettingsStore {
    #[allow(dead_code)] // Registered by the settings facade in Step 4.
    pub(crate) fn new(state: Arc<AppState>) -> Self {
        Self {
            owner: AppSettingsStore::new(state),
        }
    }
}

fn scoped(mut failure: Failure) -> Failure {
    failure.store = ID.into();
    failure
}

impl Store for EgoLaunchSettingsStore {
    fn id(&self) -> &str {
        ID
    }
    fn target(&self) -> Target {
        Target::Machine
    }
    fn read(&self) -> Result<Value, Failure> {
        self.owner.read().map_err(scoped)
    }
    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure> {
        self.owner
            .commit(&mut |latest| {
                let prepared = prepare(latest)?;
                let next: AppConfig = serde_json::from_value(prepared.value.clone())
                    .map_err(|_| Error::new(ID, "invalid_type"))?;
                let profile = next.ego_profile.trim();
                if profile.len() > 64
                    || profile.chars().any(char::is_control)
                    || (!next.ai_chat_workspace.trim().is_empty()
                        && !std::path::Path::new(next.ai_chat_workspace.trim()).is_absolute())
                {
                    return Err(Error::new(ID, "out_of_range"));
                }
                Ok(prepared)
            })
            .map_err(scoped)
    }
    async fn apply(&self, reply: &CommitReply) -> Result<bool, Failure> {
        self.owner.apply(reply).await.map_err(scoped)
    }
}
