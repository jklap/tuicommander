//! Scheduled agent automation definitions and persistence.

pub mod definitions;
pub mod model;
pub mod precheck;
pub mod schedule;

#[cfg(test)]
mod tests;

pub mod run;
pub mod store;

pub mod scheduler;
