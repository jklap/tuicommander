//! Ego-owned configuration: only its managed CLI mutates the store. Never read its files.
use super::{
    ego_schema::{self, Managed},
    request::*,
    service::Store,
    transaction::{self, Prepared},
};
use crate::{AppState, ego_cli::ConfigContext};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

const ID: &str = "ego";

pub struct EgoSettingsStore {
    state: std::sync::Weak<AppState>,
    context: Option<Arc<ConfigContext>>,
    runtime: tokio::runtime::Handle,
    sequence: Arc<Mutex<()>>,
}

impl EgoSettingsStore {
    /// Construct inside the runtime. `Store` calls run on its blocking pool, while Ego's
    /// existing asynchronous process owner remains the only execution path.
    #[allow(dead_code)] // Registered by the settings facade in Step 4.
    pub(crate) fn new(state: Arc<AppState>) -> Result<Self, Failure> {
        Ok(Self {
            state: Arc::downgrade(&state),
            context: None,
            runtime: tokio::runtime::Handle::try_current()
                .map_err(|_| Error::new(ID, "unavailable"))?,
            sequence: Arc::new(Mutex::new(())),
        })
    }

    fn context(&self) -> Result<Arc<ConfigContext>, Failure> {
        self.context
            .clone()
            .ok_or_else(|| Error::new(ID, "unavailable"))
    }

    fn snapshot(&self, context: &ConfigContext) -> Result<Value, Failure> {
        #[derive(Deserialize)]
        struct Listing {
            v: u8,
            scope: String,
            profile: Option<String>,
            values: Managed,
        }
        let raw = self
            .runtime
            .block_on(context.command("ls", &["--json"]))
            .map_err(|_| Error::new(ID, "store_failed"))?;
        let listing: Listing =
            serde_json::from_str(&raw).map_err(|_| Error::new(ID, "store_failed"))?;
        if listing.v != 1 || listing.scope != "user" || listing.profile != context.profile {
            return Err(Error::new(ID, "store_failed"));
        }
        let raw = self
            .runtime
            .block_on(context.run(&["auth".into(), "status".into()]))
            .map_err(|_| Error::new(ID, "store_failed"))?;
        #[derive(Deserialize)]
        struct Sources {
            sources: Vec<Provider>,
        }
        #[derive(Deserialize)]
        struct Provider {
            provider: String,
        }
        let models = self
            .runtime
            .block_on(context.run(&["models".into(), "--json".into()]))
            .map_err(|_| Error::new(ID, "store_failed"))?;
        let sources: Sources =
            serde_json::from_str(&models).map_err(|_| Error::new(ID, "store_failed"))?;
        let mut credentials = std::collections::BTreeMap::new();
        for source in sources.sources {
            credentials.insert(source.provider, Value::Null);
        }
        credentials.extend(ego_schema::credentials(&raw)?);
        let mut value = ego_schema::value(&listing.values)?;
        value["credentials"] =
            serde_json::to_value(credentials).map_err(|_| Error::new(ID, "store_failed"))?;
        // Bind optimistic revisions to process authority and workspace too, without exposing
        // this internal metadata as a readable or writable setting.
        value["_context"] = json!({"program":context.program,"workspace":context.workspace,"profile":context.profile});
        Ok(value)
    }
}

impl Store for EgoSettingsStore {
    fn id(&self) -> &str {
        ID
    }
    fn target(&self) -> Target {
        self.context
            .as_ref()
            .and_then(|context| context.profile.as_ref())
            .map_or(Target::Machine, |id| Target::Profile { id: id.clone() })
    }
    fn resolve(self: Arc<Self>, target: &Target) -> Result<Arc<Self>, Failure> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| Error::new(ID, "unavailable"))?;
        let context = Arc::new(
            ConfigContext::configured(state.as_ref()).map_err(|_| Error::new(ID, "unavailable"))?,
        );
        let current = context
            .profile
            .as_ref()
            .map_or(Target::Machine, |id| Target::Profile { id: id.clone() });
        if *target != current {
            return Err(Error::new(ID, "not_found"));
        }
        Ok(Arc::new(Self {
            state: self.state.clone(),
            context: Some(context),
            runtime: self.runtime.clone(),
            sequence: self.sequence.clone(),
        }))
    }
    fn read(&self) -> Result<Value, Failure> {
        let _sequence = self.sequence.lock();
        self.snapshot(self.context()?.as_ref())
    }
    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure> {
        let _sequence = self.sequence.lock();
        let context = self.context()?;
        let latest = self.snapshot(&context)?;
        let Prepared { value, mut reply } = prepare(&latest)?;
        let next: Managed =
            serde_json::from_value(value.clone()).map_err(|_| Error::new(ID, "invalid_type"))?;
        let next = ego_schema::value(&next)?;
        let changes: Vec<_> = next
            .as_object()
            .ok_or_else(|| Error::new(ID, "invalid_type"))?
            .iter()
            .filter(|(key, value)| latest.get(*key) != Some(*value))
            .collect();
        // The owner has one atomic assignment per invocation. Refuse a multi-key patch
        // before writing; sequential CLI calls would misrepresent all-or-nothing durability.
        if changes.len() > 1 {
            return Err(Error::new(ID, "unavailable"));
        }
        let Some((key, new_value)) = changes.first().copied() else {
            reply.persisted = false;
            reply.changed_ids.clear();
            reply.effects.clear();
            reply.restart.clear();
            return Ok(reply);
        };
        if key == "model"
            && let Some(model) = new_value.as_str()
        {
            crate::ego_cli::validate_model(model).map_err(|_| Error::new(ID, "out_of_range"))?;
            if !model
                .split_once('/')
                .is_some_and(|(provider, model)| !provider.is_empty() && !model.is_empty())
            {
                return Err(Error::new(ID, "out_of_range"));
            }
        }
        let assignment;
        let (operation, argument) = if new_value.is_null() {
            ("unset", key.as_str())
        } else {
            assignment = format!("{key}={}", toml_value(new_value)?);
            ("set", assignment.as_str())
        };
        self.runtime
            .block_on(context.command(operation, &[argument]))
            .map_err(|_| Error::new(ID, "store_failed"))?;
        let after = self.snapshot(&context).map_err(|mut error| {
            error.persisted = true;
            error.next_action = "get".into();
            error
        })?;
        reply.revision = transaction::revision(&after)?;
        reply.persisted = after != latest;
        if !reply.persisted {
            reply.changed_ids.clear();
            reply.effects.clear();
            reply.restart.clear();
        }
        Ok(reply)
    }
    fn apply(
        &self,
        _: &CommitReply,
    ) -> impl std::future::Future<Output = Result<bool, Failure>> + Send {
        // Settings affect new Ego invocations; existing ACP and PTY sessions are untouched.
        std::future::ready(Ok(true))
    }
}

/// Encode a single closed-key assignment, never shell text or an editor command.
fn toml_value(value: &Value) -> Result<toml_edit::Value, Failure> {
    Ok(match value {
        Value::String(s) => toml_edit::Value::from(s.as_str()),
        Value::Bool(b) => toml_edit::Value::from(*b),
        Value::Array(values) => {
            let mut array = toml_edit::Array::new();
            for value in values {
                array.push(toml_value(value)?);
            }
            toml_edit::Value::Array(array)
        }
        Value::Object(values) => {
            let mut table = toml_edit::InlineTable::new();
            for (key, value) in values {
                table.insert(key, toml_value(value)?);
            }
            toml_edit::Value::InlineTable(table)
        }
        _ => return Err(Error::new(ID, "invalid_type")),
    })
}
