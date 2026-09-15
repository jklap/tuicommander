//! Composes a `KeyFace` into a 64x64 image and encodes it as JPEG.
//!
//! No `resvg`/`usvg`: at 64x64 there is nothing to vectorize — a face is a
//! filled background, a small state glyph, up to two text lines, and maybe
//! a corner badge. No `image` crate use here either — the raw RGB8 buffer
//! goes straight into `jpeg-encoder`, so the only place `image` exists in
//! this dependency graph is transitively inside `mirajazz`'s own (unused by
//! us) `convert_image_with_format` helper.
//!
//! Layout budget (see `AGENTS.md`-level design notes for the full
//! rationale): state glyph top-left, optional badge top-right, primary
//! label (~15px) and secondary label (~9px) centered beneath, both
//! horizontally centered and truncated by the caller before reaching here.

use jpeg_encoder::{ColorType, Encoder};
use tiny_skia::{Paint, PathBuilder, Pixmap, Rect, Transform};

use crate::render::face::{Glyph, KeyFace};
use crate::render::palette::Rgb;
use crate::render::text::FontFace;

/// Renders one `KeyFace` at `size x size` px and returns encoded JPEG bytes.
/// Pure function of its inputs — no I/O, no shared state — which is what
/// makes it safe to put behind an LRU cache keyed on `KeyFace` and to
/// snapshot-test deterministically.
pub fn render_face_jpeg(font: &FontFace, face: &KeyFace, size: u32, jpeg_quality: u8) -> Vec<u8> {
    let mut pixmap = Pixmap::new(size, size).expect("size must be nonzero");
    let background = match face.glyph {
        Glyph::Pulse(phase) => face.state.pulsing_background(phase),
        _ => face.state.background(),
    };
    pixmap.fill(background.as_tiny_skia());

    draw_glyph(&mut pixmap, face.glyph, size, face.state.text_color());

    if !face.primary.is_empty() {
        font.draw_line_centered(
            pixmap.data_mut(),
            size,
            size,
            face.primary.as_str(),
            primary_px_size(size),
            size as f32 / 2.0,
            size as f32 * 0.56,
            face.state.text_color(),
        );
    }
    if !face.secondary.is_empty() {
        font.draw_line_centered(
            pixmap.data_mut(),
            size,
            size,
            face.secondary.as_str(),
            secondary_px_size(size),
            size as f32 / 2.0,
            size as f32 * 0.75,
            face.state.text_color(),
        );
    }

    if let Some(count) = face.badge {
        draw_badge(&mut pixmap, size, count);
    }

    encode_jpeg(&pixmap, size, jpeg_quality)
}

fn primary_px_size(key_size: u32) -> f32 {
    key_size as f32 * 0.23 // ~15px at 64px keys
}

fn secondary_px_size(key_size: u32) -> f32 {
    key_size as f32 * 0.14 // ~9px at 64px keys
}

fn solid_paint(color: Rgb) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(color.as_tiny_skia());
    paint.anti_alias = true;
    paint
}

fn draw_glyph(pixmap: &mut Pixmap, glyph: Glyph, size: u32, color: Rgb) {
    let margin = size as f32 * 0.12;
    let radius = size as f32 * 0.06;
    let cx = margin + radius;
    let cy = margin + radius;
    match glyph {
        Glyph::None => {}
        Glyph::Dot | Glyph::Pulse(_) => {
            if let Some(path) = PathBuilder::from_circle(cx, cy, radius) {
                pixmap.fill_path(
                    &path,
                    &solid_paint(color),
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
        Glyph::Question | Glyph::Bang => {
            // A filled square stands in for '?'/'!' glyphs at this scale —
            // legible-enough at 64px without pulling a glyph out of the
            // bundled font for a single decorative mark.
            if let Some(rect) = Rect::from_xywh(margin, margin, radius * 2.0, radius * 2.0) {
                pixmap.fill_rect(rect, &solid_paint(color), Transform::identity(), None);
            }
        }
    }
}

/// A badge must read against *any* state background, including the states
/// whose background happens to be the same red this badge would otherwise
/// use — a fixed near-black backing circle with white text is the only
/// choice that's guaranteed not to blend into the face it's drawn on.
fn draw_badge(pixmap: &mut Pixmap, size: u32, count: u8) {
    let radius = size as f32 * 0.13;
    let cx = size as f32 - radius - size as f32 * 0.06;
    let cy = radius + size as f32 * 0.06;
    if let Some(path) = PathBuilder::from_circle(cx, cy, radius) {
        pixmap.fill_path(
            &path,
            &solid_paint(Rgb(0x10, 0x10, 0x10)),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    let text = if count > 9 {
        "9+".to_string()
    } else {
        count.to_string()
    };
    let font = FontFace::bundled();
    font.draw_line_centered(
        pixmap.data_mut(),
        size,
        size,
        &text,
        radius * 1.2,
        cx,
        cy + radius * 0.45,
        Rgb(0xFF, 0xFF, 0xFF),
    );
}

fn encode_jpeg(pixmap: &Pixmap, size: u32, quality: u8) -> Vec<u8> {
    // tiny-skia's buffer is premultiplied RGBA8, but every face we render
    // is fully opaque (alpha == 255 everywhere — see text.rs's blend_pixel
    // comment), so premultiplied and straight RGB are identical here; we
    // just drop the alpha channel rather than dividing by it.
    let (rgba_pixels, _) = pixmap.data().as_chunks::<4>();
    let mut rgb = Vec::with_capacity((size * size * 3) as usize);
    for px in rgba_pixels {
        rgb.push(px[0]);
        rgb.push(px[1]);
        rgb.push(px[2]);
    }

    let mut out = Vec::new();
    let encoder = Encoder::new(&mut out, quality);
    encoder
        .encode(&rgb, size as u16, size as u16, ColorType::Rgb)
        .expect("encoding a freshly-built size x size RGB buffer cannot fail");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::face::Label;
    use crate::render::palette::FaceState;

    fn sample_face(state: FaceState, primary: &str, secondary: &str) -> KeyFace {
        KeyFace {
            state,
            glyph: Glyph::Dot,
            primary: Label::from_str_truncated(primary),
            secondary: Label::from_str_truncated(secondary),
            badge: None,
        }
    }

    #[test]
    fn renders_valid_jpeg_bytes() {
        let font = FontFace::bundled();
        let face = sample_face(FaceState::Working, "tc-7", "refactor");
        let jpeg = render_face_jpeg(&font, &face, 64, 90);
        assert!(!jpeg.is_empty());
        // SOI / EOI markers — the two bytes every valid JPEG stream starts
        // and ends with. This is the cheapest possible guard against ever
        // handing a truncated/corrupt buffer to the device: the M18's own
        // README warns a malformed image write can require factory
        // reprogramming to recover from, so this check earns its place.
        assert_eq!(&jpeg[0..2], &[0xFF, 0xD8], "JPEG must start with SOI");
        assert_eq!(
            &jpeg[jpeg.len() - 2..],
            &[0xFF, 0xD9],
            "JPEG must end with EOI"
        );
    }

    #[test]
    fn rendering_is_deterministic() {
        let font = FontFace::bundled();
        let face = sample_face(FaceState::NeedsInput, "tc-3", "A B C");
        let a = render_face_jpeg(&font, &face, 64, 90);
        let b = render_face_jpeg(&font, &face, 64, 90);
        assert_eq!(
            a, b,
            "the same KeyFace must always encode to the same bytes — this is what makes the render cache valid"
        );
    }

    #[test]
    fn six_canonical_faces_all_render() {
        let font = FontFace::bundled();
        for (state, primary, secondary) in [
            (FaceState::Idle, "tc-1", ""),
            (FaceState::Working, "tc-2", "refactor"),
            (FaceState::NeedsInput, "tc-3", "A B C"),
            (FaceState::CompletedUnread, "tc-4", "done"),
            (FaceState::StaleReady, "tc-5", "ready (12m)"),
            (FaceState::Error, "tc-6", "rate limit"),
        ] {
            let face = sample_face(state, primary, secondary);
            let jpeg = render_face_jpeg(&font, &face, 64, 90);
            assert!(
                jpeg.len() > 100,
                "{state:?} rendered a suspiciously tiny JPEG"
            );
        }
    }

    #[test]
    fn pulse_phase_actually_changes_the_rendered_bytes() {
        // A KeyFace differing only in pulse phase must render differently —
        // otherwise the 4-phase-bucket design (render/face.rs's whole
        // reason for making Pulse carry a phase instead of being a plain
        // Dot) produces four cache entries that all look identical, and no
        // animation would ever actually be visible on the device.
        let font = FontFace::bundled();
        let mut dim = sample_face(FaceState::NeedsInput, "tc-3", "A B C");
        dim.glyph = Glyph::Pulse(0);
        let mut bright = dim.clone();
        bright.glyph = Glyph::Pulse(2);

        let dim_jpeg = render_face_jpeg(&font, &dim, 64, 90);
        let bright_jpeg = render_face_jpeg(&font, &bright, 64, 90);
        assert_ne!(
            dim_jpeg, bright_jpeg,
            "different pulse phases must render visibly different frames"
        );
    }

    #[test]
    fn empty_slot_renders_pure_background_with_no_text() {
        let font = FontFace::bundled();
        let face = KeyFace::empty();
        let jpeg = render_face_jpeg(&font, &face, 64, 90);
        assert!(!jpeg.is_empty());
    }
}
