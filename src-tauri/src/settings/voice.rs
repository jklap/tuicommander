//! Voice owner seam: `dictation-config.json` is written only through `ConfigFile`, and a save
//! refreshes the running voice through the same `refresh_voice_runtime` as `save_dictation_config`,
//! so no writer can leave the old voice speaking or a stale model snapshot cached.
use super::{config_file::ConfigFileStore, request::*, service::Store, transaction::Prepared};
use crate::config::{ConfigFile, DictationConfig};
use crate::dictation::{
    commands::{DICTATION_CONFIG_FILE, refresh_voice_runtime},
    speaker,
};
use parking_lot::Mutex;
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};

const ID: &str = "voice";

/// Fields that make a queued reply wrong; keep in step with `save_dictation_config`.
const VOICE_FIELDS: [&str; 5] = [
    "voice.language",
    "voice.speech_engine",
    "voice.speech_command",
    "voice.speech_voice",
    "voice.speech_edge_voice",
];

pub struct VoiceSettingsStore {
    path: PathBuf,
    /// The live speaker slot of `DictationState`; `None` before startup has one.
    speaker: Option<Arc<Mutex<Option<speaker::Armed>>>>,
}

impl VoiceSettingsStore {
    pub fn new(speaker: Option<Arc<Mutex<Option<speaker::Armed>>>>) -> Self {
        Self {
            path: crate::config::config_dir().join(DICTATION_CONFIG_FILE),
            speaker,
        }
    }

    #[cfg(test)]
    pub(crate) fn at_path(
        path: PathBuf,
        speaker: Option<Arc<Mutex<Option<speaker::Armed>>>>,
    ) -> Self {
        Self { path, speaker }
    }

    fn file(&self) -> ConfigFile<DictationConfig> {
        ConfigFile::at_path(self.path.clone())
    }
}

/// Strict, lock-taking read: a corrupt file is an error, never silently the defaults.
fn read_strict(file: &ConfigFile<DictationConfig>) -> Result<DictationConfig, Failure> {
    file.update_with_strict(|config| Ok((config.clone(), false)))
        .map_err(|_| Error::new(ID, "store_failed"))
}

impl Store for VoiceSettingsStore {
    fn id(&self) -> &str {
        ID
    }
    fn target(&self) -> Target {
        Target::Machine
    }

    fn read(&self) -> Result<Value, Failure> {
        ConfigFileStore::new(ID, self.file()).read()
    }

    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure> {
        ConfigFileStore::new(ID, self.file()).commit(prepare, |_, next| {
            // A read-response diagnostic, never persisted (as in `save_dictation_config`).
            next.recovered_from_corruption = false;
        })
    }

    fn apply(
        &self,
        reply: &CommitReply,
    ) -> impl std::future::Future<Output = Result<bool, Failure>> + Send {
        let file = self.file();
        let speaker = self.speaker.clone();
        let voice_changed = reply
            .changed_ids
            .iter()
            .any(|id| VOICE_FIELDS.contains(&id.as_str()));
        async move {
            tokio::task::spawn_blocking(move || {
                let config = read_strict(&file)?;
                refresh_voice_runtime(voice_changed, &config, speaker.as_deref());
                Ok(true)
            })
            .await
            .map_err(|_| Error::new(ID, "store_failed"))?
        }
    }
}
