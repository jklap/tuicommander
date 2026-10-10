#[cfg(feature = "desktop")]
mod browser;
#[cfg(feature = "desktop")]
pub(crate) mod commands;
pub(crate) mod extension;
#[cfg(feature = "desktop")]
mod legacy;
#[cfg(feature = "desktop")]
pub(crate) mod manager;
mod payload;
mod source;
#[cfg(feature = "desktop")]
pub(crate) mod tauri_commands;
#[cfg(feature = "desktop")]
pub(crate) use legacy::*;
#[cfg(all(test, unix))]
mod demo;
pub(crate) mod send;
