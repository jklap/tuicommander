//! Rust-owned settings schemas and descriptors.
pub mod catalogue;
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
mod repositories_tests;

#[cfg(test)]
mod remote_tests;
