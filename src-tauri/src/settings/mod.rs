//! Rust-owned settings schemas and descriptors.
pub mod catalogue;
pub mod model;

pub mod native;
pub mod notifications;
pub mod registry;
pub mod request;
pub mod service;
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
