//! What is left of the embedded agent after the engine was deleted (#784-0aec).
//!
//! Neither module below has anything to do with an LLM. `tui_detect` classifies a
//! terminal as a shell or a fullscreen TUI, and `knowledge` records how commands
//! turned out per session. Both are read by `pty.rs`, which is why they kept the
//! module path the engine used to own.
pub mod knowledge;
pub mod tui_detect;
