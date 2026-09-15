//! Mirabox StreamDock M18 macropad integration.
//!
//! This crate knows nothing about TUICommander's `AppState`. Everything it
//! needs from a host is the two traits in `port.rs`
//! (`StateSource`/`ActionSink`); everything it needs about the hardware is
//! transcribed as data in `device::model` on top of the `mirajazz` crate,
//! which already implements the Mirabox CRT wire protocol end to end. See
//! each module's doc comment for the reasoning behind its specific
//! choices — `device::model` for the vendor-SDK-derived constants,
//! `dispatch` for the (empirically unconfirmed) hold-gesture detection
//! scheme, `render::draw`/`render::text` for the rendering choices.

pub mod coordinator;
pub mod device;
pub mod dispatch;
pub mod policy;
pub mod port;
pub mod render;

pub use coordinator::Coordinator;
pub use port::{ActionSink, Doorbell, SessionSnapshot, StateSource};
