//! Font loading and glyph rasterization for key faces.
//!
//! Deliberately not a system font. `fontdb`-style system-font enumeration is
//! slow, differs per machine, and makes rendering non-deterministic — which
//! would break both the `KeyFace` content-hash cache (a face that renders
//! differently across two calls is not actually cacheable) and the render
//! snapshot tests in `draw.rs`. Bundling a small subsetted font is the only
//! option that keeps both of those properties, and there was no font crate
//! in the main TUICommander repo to reuse, so this is a new dependency
//! either way.
//!
//! `ab_glyph` was chosen over `cosmic-text`/`rustybuzz` because there is no
//! shaping to do — every label here is plain ASCII with no ligatures,
//! bidi, or complex scripts, so a per-glyph outline + advance-width loop is
//! both correct and far smaller than pulling in a full shaping engine.

use ab_glyph::{Font, FontRef, Glyph as AbGlyph, PxScale, ScaleFont};

use crate::render::palette::Rgb;

/// The bundled, printable-ASCII-only subset of JetBrains Mono (OFL-1.1;
/// already vendored and license-vetted in the main TUICommander repo as a
/// webfont — see `src/render/assets/LICENSE.md` for provenance). Monospace
/// on purpose: a `KeyFace`'s character budget (≤8/≤11 chars) is then a
/// fixed-width layout computation, not a font-metrics guess per string.
pub static FONT_BYTES: &[u8] = include_bytes!("assets/JetBrainsMono-Subset.ttf");

pub struct FontFace {
    font: FontRef<'static>,
}

impl FontFace {
    pub fn bundled() -> Self {
        Self::from_bytes(FONT_BYTES)
            .expect("bundled font must be valid — this is a build-time asset, not user input")
    }

    pub fn from_bytes(bytes: &'static [u8]) -> Result<Self, ab_glyph::InvalidFont> {
        Ok(Self {
            font: FontRef::try_from_slice(bytes)?,
        })
    }

    /// Total advance width of `text` at the given pixel size, for centering.
    pub fn measure(&self, text: &str, px_size: f32) -> f32 {
        let scaled = self.font.as_scaled(PxScale::from(px_size));
        text.chars()
            .map(|c| scaled.h_advance(self.font.glyph_id(c)))
            .sum()
    }

    /// Draws one line of text into an RGBA8 buffer (`width * height * 4`
    /// bytes, row-major, straight — not premultiplied — alpha), horizontally
    /// centered around `center_x`, with its baseline at `baseline_y`, alpha
    /// composited over whatever is already in `buf` (expected to be fully
    /// opaque, since every `KeyFace` background is opaque).
    ///
    /// Nine plain parameters rather than a bundled "layout" struct: every
    /// call site (draw.rs's two text lines and its badge digit) passes a
    /// different combination computed inline, so a struct would just move
    /// the same nine values into field-assignment noise at each call site
    /// without reducing what any one caller has to specify.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_line_centered(
        &self,
        buf: &mut [u8],
        buf_w: u32,
        buf_h: u32,
        text: &str,
        px_size: f32,
        center_x: f32,
        baseline_y: f32,
        color: Rgb,
    ) {
        let width = self.measure(text, px_size);
        let mut cursor_x = center_x - width / 2.0;
        let scaled = self.font.as_scaled(PxScale::from(px_size));

        for c in text.chars() {
            let id = self.font.glyph_id(c);
            let advance = scaled.h_advance(id);
            let glyph: AbGlyph =
                id.with_scale_and_position(px_size, ab_glyph::point(cursor_x, baseline_y));
            if let Some(outlined) = self.font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|x, y, coverage| {
                    if coverage <= 0.0 {
                        return;
                    }
                    let px = bounds.min.x as i32 + x as i32;
                    let py = bounds.min.y as i32 + y as i32;
                    if px < 0 || py < 0 || px as u32 >= buf_w || py as u32 >= buf_h {
                        return;
                    }
                    blend_pixel(buf, buf_w, px as u32, py as u32, color, coverage.min(1.0));
                });
            }
            cursor_x += advance;
        }
    }
}

/// Simple source-over alpha blend of `color` at `coverage` onto an opaque
/// RGBA8 buffer pixel. The buffer stays opaque throughout (alpha untouched
/// at 255) because every caller only ever blends text/glyphs onto an
/// already-fully-opaque background fill — there is no transparency anywhere
/// in a key face by design (the M18's LCD has no notion of it either).
fn blend_pixel(buf: &mut [u8], buf_w: u32, x: u32, y: u32, color: Rgb, coverage: f32) {
    let idx = ((y * buf_w + x) * 4) as usize;
    if idx + 3 >= buf.len() {
        return;
    }
    let inv = 1.0 - coverage;
    buf[idx] = (color.0 as f32 * coverage + buf[idx] as f32 * inv).round() as u8;
    buf[idx + 1] = (color.1 as f32 * coverage + buf[idx + 1] as f32 * inv).round() as u8;
    buf[idx + 2] = (color.2 as f32 * coverage + buf[idx + 2] as f32 * inv).round() as u8;
    buf[idx + 3] = 255;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_font_loads() {
        let _ = FontFace::bundled();
    }

    #[test]
    fn measure_is_positive_and_monotonic_in_length() {
        let font = FontFace::bundled();
        let short = font.measure("tc", 15.0);
        let long = font.measure("tc-777", 15.0);
        assert!(short > 0.0);
        assert!(long > short, "a longer string must measure wider");
    }

    #[test]
    fn draw_line_darkens_some_pixels_in_an_opaque_buffer() {
        let font = FontFace::bundled();
        let (w, h) = (64u32, 64u32);
        let mut buf = [0x2Bu8, 0x2F, 0x36, 255].repeat((w * h) as usize);
        font.draw_line_centered(
            &mut buf,
            w,
            h,
            "tc-7",
            15.0,
            32.0,
            40.0,
            Rgb(0xEB, 0xEB, 0xEB),
        );
        let (pixels, _) = buf.as_chunks::<4>();
        let changed = pixels
            .iter()
            .filter(|p| p[0] != 0x2B || p[1] != 0x2F || p[2] != 0x36)
            .count();
        assert!(changed > 0, "drawing text must change at least some pixels");
        // Every pixel must stay fully opaque — a key face never has
        // transparency.
        assert!(pixels.iter().all(|p| p[3] == 255));
    }
}
