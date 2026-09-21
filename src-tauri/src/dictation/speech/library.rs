//! The installed speech engines, and the rule that keeps installing from
//! racing speaking.
//!
//! One engine per language, built on first use and kept: opening the graphs
//! costs about a quarter of a second, which is most of a short reply's latency.
//! Keeping them is also what makes replacement dangerous — the files under a
//! loaded engine are memory-mapped by onnxruntime, and renaming a directory out
//! from under it is undefined rather than merely stale.
//!
//! So the order is fixed, and it is the whole point of this module:
//!
//! ```text
//! download and verify into .staging   <- no lock; the engine may be speaking
//! unload the engine for this language <- waits for the sentence in flight
//! rename .staging into place          <- microseconds, nothing can speak
//! next synthesis loads the new files  <- lazily, as it always did
//! ```
//!
//! The long part holds nothing. The part that excludes synthesis is a rename.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;

use super::SpeechCancel;
use super::assets::{self, Asset, InstallError, Status};
use super::pocket::PocketSpeech;

#[derive(Default)]
pub struct SpeechLibrary {
    /// Built lazily, one per language, dropped on replacement, deletion and
    /// shutdown.
    engines: Mutex<HashMap<String, Arc<PocketSpeech>>>,
    /// The cancel handle of each download in flight, by asset id. A download
    /// that finishes or fails takes its own entry out.
    downloads: Mutex<HashMap<String, SpeechCancel>>,
}

impl SpeechLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    /// The engine for a language, built if this is the first time.
    ///
    /// Construction reads nothing, so this cannot fail and a language that is
    /// not installed becomes a [`super::SpeechError::ModelUnavailable`] at the
    /// first spoken word rather than an error here.
    pub fn engine(&self, language: &str) -> Arc<PocketSpeech> {
        let mut engines = self.engines.lock();
        Arc::clone(
            engines
                .entry(language.to_string())
                .or_insert_with(|| Arc::new(PocketSpeech::for_language(language))),
        )
    }

    /// Drop the loaded graphs for one language, waiting for any synthesis in
    /// flight to finish first.
    ///
    /// The engine handle survives — a caller holding an `Arc` keeps working,
    /// and reloads from disk the next time it speaks.
    pub fn unload(&self, language: &str) {
        let engine = self.engines.lock().get(language).map(Arc::clone);
        if let Some(engine) = engine {
            engine.unload();
        }
    }

    /// Forget every engine. Called when dictation shuts down: an `ort::Session`
    /// holds the whole graph resident, which for one language is about 125 MB.
    pub fn shutdown(&self) {
        // Unload before dropping the handles. Another thread may still hold an
        // `Arc`, in which case dropping ours frees nothing and only `unload`
        // actually releases the memory.
        let engines: Vec<Arc<PocketSpeech>> =
            self.engines.lock().values().map(Arc::clone).collect();
        for engine in engines {
            engine.unload();
        }
        self.engines.lock().clear();
        // Anything still downloading is downloading for a process that is
        // going away.
        for cancel in self.downloads.lock().values() {
            cancel.cancel();
        }
    }

    /// Is this asset usable right now?
    pub fn status(&self, asset: &Asset) -> Status {
        assets::status(asset)
    }

    /// Is a download of this asset in flight?
    pub fn is_downloading(&self, id: &str) -> bool {
        self.downloads.lock().contains_key(id)
    }

    /// Abandon a download in flight. Returns whether there was one — a caller
    /// that cancels a finished download should be told so rather than
    /// answered with a silent success.
    pub fn cancel_download(&self, id: &str) -> bool {
        match self.downloads.lock().get(id) {
            Some(cancel) => {
                cancel.cancel();
                true
            }
            None => false,
        }
    }

    /// Download an asset and put it in place.
    ///
    /// Refuses a second concurrent install of the same asset: two of them would
    /// share one staging directory and each would delete the other's files.
    pub async fn install(
        &self,
        asset: &'static Asset,
        on_progress: impl Fn(u64, u64),
    ) -> Result<PathBuf, InstallError> {
        let cancel = SpeechCancel::new();
        {
            let mut downloads = self.downloads.lock();
            if downloads.contains_key(asset.id) {
                return Err(InstallError::Disk(format!(
                    "{} is already downloading",
                    asset.display_name
                )));
            }
            downloads.insert(asset.id.to_string(), cancel.clone());
        }

        let staged = assets::stage(asset, &cancel, on_progress).await;
        self.downloads.lock().remove(asset.id);
        let staging = staged?;

        // Only now is anything held. `unload` waits for a sentence in flight;
        // `promote` is two renames.
        if let Some(language) = asset.language() {
            self.unload(language);
        }
        let result = assets::promote(asset, &staging);
        if result.is_err() {
            assets::discard(&staging);
        }
        result
    }

    /// Remove an installed asset, releasing whatever it had loaded first.
    pub fn delete(&self, asset: &Asset) -> Result<(), InstallError> {
        if let Some(language) = asset.language() {
            self.unload(language);
            self.engines.lock().remove(language);
        }
        assets::remove(asset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictation::speech::assets::{CATALOGUE, find};

    fn library() -> (tempfile::TempDir, impl Drop, SpeechLibrary) {
        let root = tempfile::tempdir().unwrap();
        let guard = crate::config::set_config_dir_override(root.path().to_path_buf());
        (root, guard, SpeechLibrary::new())
    }

    #[test]
    fn asking_twice_for_a_language_gives_the_same_engine() {
        // Two engines for one language would each load 125 MB of graphs, and
        // unloading one would leave the other holding files that are about to
        // be renamed away.
        let (_root, _guard, library) = library();
        let first = library.engine("italian");
        let second = library.engine("italian");
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn different_languages_get_different_engines() {
        let (_root, _guard, library) = library();
        let italian = library.engine("italian");
        let english = library.engine("english");
        assert!(!Arc::ptr_eq(&italian, &english));
        assert!(italian.bundle_dir().ends_with("italian"));
        assert!(english.bundle_dir().ends_with("english"));
    }

    #[test]
    fn an_engine_handle_survives_being_unloaded() {
        // The caller in `continuous.rs` holds one for the life of a hands-free
        // session. Invalidating it on every model swap would mean re-fetching
        // it on every reply just in case.
        let (_root, _guard, library) = library();
        let engine = library.engine("italian");
        library.unload("italian");
        assert!(Arc::ptr_eq(&engine, &library.engine("italian")));
    }

    #[test]
    fn unloading_a_language_that_was_never_loaded_does_nothing() {
        let (_root, _guard, library) = library();
        library.unload("italian");
        assert!(!library.is_downloading("italian"));
    }

    #[test]
    fn shutdown_lets_go_of_every_engine() {
        let (_root, _guard, library) = library();
        let engine = library.engine("italian");
        library.shutdown();
        // The map is empty, so the next `engine` call builds a new one rather
        // than handing back the old handle.
        assert!(!Arc::ptr_eq(&engine, &library.engine("italian")));
    }

    #[test]
    fn deleting_a_language_forgets_its_engine_as_well_as_its_files() {
        // Keeping the engine would leave a handle pointing at a directory that
        // no longer exists, and the next reply would fail with a missing file
        // instead of reporting the language as not installed.
        let (_root, _guard, library) = library();
        let engine = library.engine("italian");
        let asset = find("italian").unwrap();
        library.delete(asset).unwrap();
        assert!(!Arc::ptr_eq(&engine, &library.engine("italian")));
    }

    #[test]
    fn cancelling_a_download_nobody_started_says_so() {
        let (_root, _guard, library) = library();
        assert!(!library.cancel_download("italian"));
    }

    #[test]
    fn every_catalogue_asset_can_be_deleted_when_it_is_not_installed() {
        // Delete is the recovery path for a half-finished download, so it has
        // to work on every asset in every state, including "no directory".
        let (_root, _guard, library) = library();
        for asset in CATALOGUE {
            assert_eq!(library.delete(asset), Ok(()), "{}", asset.id);
        }
    }
}
