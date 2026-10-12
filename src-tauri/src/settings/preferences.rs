//! Preferences owner seam: `ui-prefs.json` is written only through `ConfigFile`,
//! so a settings edit takes the same locks as every other writer of that file.
use super::{
    request::*,
    service::Store,
    transaction::{self, Prepared},
};
use crate::config::{ConfigFile, UIPrefsConfig};
use serde_json::{Map, Value};

const ID: &str = "preferences";

pub struct PreferencesSettingsStore {
    // Raw document: keys the typed struct does not model must survive a settings edit.
    file: ConfigFile<Value>,
}

impl Default for PreferencesSettingsStore {
    fn default() -> Self {
        Self {
            file: ConfigFile::new("ui-prefs.json"),
        }
    }
}

impl PreferencesSettingsStore {
    #[cfg(test)]
    pub(crate) fn at_path(path: std::path::PathBuf) -> Self {
        Self {
            file: ConfigFile::at_path(path),
        }
    }
}

/// A missing file reads as `Null`; anything but an object is not a preferences document.
fn modelled(document: &Value) -> Result<Value, Failure> {
    let object = match document {
        Value::Null => Map::new(),
        Value::Object(object) => object.clone(),
        _ => return Err(Error::new(ID, "store_failed")),
    };
    let config: UIPrefsConfig = serde_json::from_value(Value::Object(object))
        .map_err(|_| Error::new(ID, "store_failed"))?;
    to_value(&config)
}

fn to_value(config: &UIPrefsConfig) -> Result<Value, Failure> {
    serde_json::to_value(config).map_err(|_| Error::new(ID, "store_failed"))
}

impl Store for PreferencesSettingsStore {
    fn id(&self) -> &str {
        ID
    }
    fn target(&self) -> Target {
        Target::Instance
    }

    fn read(&self) -> Result<Value, Failure> {
        // Strict: a corrupt file is an error, never silently the defaults.
        self.file
            .update_with_strict(|document| Ok((modelled(document), false)))
            .map_err(|_| Error::new(ID, "store_failed"))?
    }

    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure> {
        let mut failure = None;
        let result = self.file.update_with_strict(|document| {
            let latest = modelled(document).map_err(|e| {
                failure = Some(e);
                "unreadable".to_owned()
            })?;
            let Prepared { value, mut reply } = prepare(&latest).map_err(|e| {
                failure = Some(e);
                "rejected".to_owned()
            })?;
            let next: UIPrefsConfig = serde_json::from_value(value).map_err(|_| {
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
                // Overlay the modelled keys; keys this struct does not know stay as found.
                let mut merged = document.as_object().cloned().unwrap_or_default();
                if let Value::Object(fields) = canonical {
                    merged.extend(fields);
                }
                *document = Value::Object(merged);
            }
            Ok((reply, changed))
        });
        result.map_err(|_| failure.unwrap_or_else(|| Error::new(ID, "store_failed")))
    }

    fn apply(
        &self,
        _: &CommitReply,
    ) -> impl std::future::Future<Output = Result<bool, Failure>> + Send {
        // Live-only: the UI reads ui-prefs.json; no backend runtime state caches it.
        std::future::ready(Ok(true))
    }
}
