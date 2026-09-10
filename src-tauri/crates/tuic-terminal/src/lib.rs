pub mod chrome;
pub mod grid_gate;
pub mod input_line_buffer;
pub mod output_parser;
pub mod output_watchers;
pub mod terminal_grid;
// Storage seam for the color-tools inline-image plan; Phase 2 (OSC 1337) and
// Phase 3 (Kitty graphics) are the real callers of `ImageStore::store`. Fully
// covered by its own unit tests (including the refcount-eviction property the
// whole design exists for) in the meantime.
#[allow(dead_code)]
pub mod terminal_images;

pub mod vt_log;

pub mod pty_capture;
