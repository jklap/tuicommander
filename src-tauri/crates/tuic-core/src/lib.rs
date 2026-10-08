//! Shared, application-independent primitives for TUICommander domain crates.

pub mod app_instance;
#[cfg(feature = "audio-output")]
pub mod audio_output;
pub mod cli;
pub mod credentials;
pub mod error_classification;
pub mod jsonc_edit;
pub mod path_spelling;
pub mod process_env;
pub mod process_tree;
pub mod redaction;
pub mod text_rank;

pub mod config_dir;
