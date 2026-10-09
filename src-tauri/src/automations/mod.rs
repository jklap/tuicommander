//! Scheduled agent automation definitions and persistence.

pub mod definitions;
pub mod model;
pub mod precheck;
pub mod schedule;

#[cfg(test)]
mod tests;

pub mod run;
pub mod store;

#[cfg(test)]
mod mcp_tests;

pub mod actions;
pub mod mcp;
