//! Render pipeline: `SessionSnapshot`/policy decision -> `KeyFace` ->
//! encoded JPEG bytes, cached by content. See `draw.rs` for the "why not
//! resvg/`image`" reasoning and `text.rs` for the "why not a system font"
//! reasoning — both load-bearing choices for a 64x64, deterministic,
//! cacheable render.

pub mod cache;
pub mod draw;
pub mod face;
pub mod palette;
pub mod text;

pub use cache::RenderCache;
pub use face::{Glyph, KeyFace, Label};
pub use palette::{FaceState, Rgb};
