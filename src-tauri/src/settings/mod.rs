//! Rust-owned settings schemas and descriptors.
pub mod agents;
pub mod catalogue;
pub mod ego;
mod ego_schema;
pub mod model;

pub mod native;
pub mod notifications;
pub mod preferences;
pub mod registry;
pub mod remote;
pub mod repositories;
pub mod request;
pub mod service;
pub mod telegram;
pub mod transaction;
pub mod upstreams;
#[cfg(feature = "dictation")]
pub mod voice;

mod config_file;
mod pointer;
mod validation;

#[cfg(test)]
mod collection_tests;
#[cfg(test)]
mod transaction_tests;

#[cfg(test)]
mod critic_tests;

#[cfg(test)]
mod validation_tests;

#[cfg(test)]
mod notifications_tests;

#[cfg(test)]
mod preferences_tests;

#[cfg(test)]
mod telegram_tests;

#[cfg(test)]
mod agents_tests;

#[cfg(test)]
mod repositories_tests;

#[cfg(test)]
mod remote_tests;

#[cfg(test)]
mod upstreams_tests;

#[cfg(all(test, feature = "dictation"))]
mod voice_tests;

#[cfg(test)]
mod config_file_tests;

#[cfg(all(test, unix))]
mod ego_fixture;
#[cfg(all(test, unix))]
mod ego_tests;
