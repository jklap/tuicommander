//! iTerm2 inline-images protocol (`OSC 1337 ; File=...` /
//! `MultipartFile=...`/`FilePart=...`/`FileEnd`) — color-tools plan, Phase 2.
//!
//! Pure parsing and cell-footprint geometry only. Grid mutation (reserving
//! blank cells, attaching `ImageCellRef`s) stays in `term/mod.rs`'s `Handler`
//! impl, the only place with `&mut Grid` access — keeping this module
//! trivially unit-testable without a full `Term`.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as Base64;

/// One `File=`/`MultipartFile=` argument list, defaults per iTerm2's docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileArgs {
    pub name: Option<String>,
    pub size: Option<u64>,
    pub width: Dimension,
    pub height: Dimension,
    pub preserve_aspect_ratio: bool,
    /// Display inline vs. save-to-downloads. Only the inline path is
    /// implemented (Phase 2 scope); `inline=0` is treated as a no-op rather
    /// than writing anything to disk.
    pub inline: bool,
}

impl Default for FileArgs {
    fn default() -> Self {
        Self {
            name: None,
            size: None,
            width: Dimension::Auto,
            height: Dimension::Auto,
            preserve_aspect_ratio: true,
            inline: false,
        }
    }
}

/// A `width=`/`height=` value: a bare number of cells, a `px` pixel count, a
/// `%` fraction of the viewport, or `auto`/absent (resolved from the image's
/// own intrinsic size).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    Cells(u32),
    Pixels(u32),
    Percent(u32),
    Auto,
}

impl Dimension {
    fn parse(s: &str) -> Self {
        if s.is_empty() || s.eq_ignore_ascii_case("auto") {
            Dimension::Auto
        } else if let Some(n) = s.strip_suffix("px") {
            n.parse().map(Dimension::Pixels).unwrap_or(Dimension::Auto)
        } else if let Some(n) = s.strip_suffix('%') {
            n.parse().map(Dimension::Percent).unwrap_or(Dimension::Auto)
        } else {
            s.parse().map(Dimension::Cells).unwrap_or(Dimension::Auto)
        }
    }
}

/// Parse a `;`-separated `key=value` argument list — the part of `File=`/
/// `MultipartFile=` before the `:` payload separator, with that prefix
/// already stripped by the caller. Unknown keys are ignored; a malformed
/// value falls back to that field's default rather than aborting the whole
/// parse — a client sending one bad argument shouldn't lose the image.
pub fn parse_file_args(args: &str) -> FileArgs {
    let mut result = FileArgs::default();
    for part in args.split(';') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key {
            "name" => {
                if let Ok(decoded) = Base64.decode(value.as_bytes())
                    && let Ok(name) = String::from_utf8(decoded)
                {
                    result.name = Some(name);
                }
            }
            "size" => result.size = value.parse().ok(),
            "width" => result.width = Dimension::parse(value),
            "height" => result.height = Dimension::parse(value),
            "preserveAspectRatio" => result.preserve_aspect_ratio = value != "0",
            "inline" => result.inline = value == "1",
            _ => {}
        }
    }
    result
}

/// Resolve a cell footprint from parsed args, the image's intrinsic pixel
/// dimensions (0 if unknown — header-sniffing failed or the format is
/// unsupported), the current cell pixel size, and the current viewport size
/// (for `%` specs). Mirrors iTerm2's own semantics: an explicit dimension
/// wins outright; when exactly one of width/height is `auto` and
/// `preserve_aspect_ratio` is set, the auto side is derived from the other
/// plus the image's own aspect ratio, not just its raw intrinsic size.
pub fn resolve_footprint(
    args: &FileArgs,
    intrinsic_width: u32,
    intrinsic_height: u32,
    cell_width_px: u32,
    cell_height_px: u32,
    viewport_cols: u32,
    viewport_rows: u32,
) -> (u32, u32) {
    let cell_width_px = cell_width_px.max(1);
    let cell_height_px = cell_height_px.max(1);

    let to_cells_w = |d: Dimension, auto_px: u32| -> u32 {
        match d {
            Dimension::Cells(n) => n,
            Dimension::Pixels(px) => px.div_ceil(cell_width_px),
            Dimension::Percent(p) => (viewport_cols * p).div_ceil(100),
            Dimension::Auto => auto_px.div_ceil(cell_width_px),
        }
    };
    let to_cells_h = |d: Dimension, auto_px: u32| -> u32 {
        match d {
            Dimension::Cells(n) => n,
            Dimension::Pixels(px) => px.div_ceil(cell_height_px),
            Dimension::Percent(p) => (viewport_rows * p).div_ceil(100),
            Dimension::Auto => auto_px.div_ceil(cell_height_px),
        }
    };

    let width_is_auto = matches!(args.width, Dimension::Auto);
    let height_is_auto = matches!(args.height, Dimension::Auto);
    let has_intrinsic = intrinsic_width > 0 && intrinsic_height > 0;

    if has_intrinsic && args.preserve_aspect_ratio && width_is_auto != height_is_auto {
        if width_is_auto {
            let height_cells = to_cells_h(args.height, intrinsic_height).max(1);
            let height_px = height_cells * cell_height_px;
            let width_px = height_px * intrinsic_width / intrinsic_height;
            let width_cells = width_px.div_ceil(cell_width_px).max(1);
            return (width_cells, height_cells);
        }
        let width_cells = to_cells_w(args.width, intrinsic_width).max(1);
        let width_px = width_cells * cell_width_px;
        let height_px = width_px * intrinsic_height / intrinsic_width;
        let height_cells = height_px.div_ceil(cell_height_px).max(1);
        return (width_cells, height_cells);
    }

    let width_cells = to_cells_w(args.width, intrinsic_width).max(1);
    let height_cells = to_cells_h(args.height, intrinsic_height).max(1);
    (width_cells, height_cells)
}

/// Minimal image-dimension sniffing for `auto` sizing. Deliberately reads
/// only container headers, never decodes pixels — full rasterization stays
/// the WebView's job (color-tools plan, Architecture: "do not rasterize in
/// Rust"). Supports PNG and GIF, the common cases for imgcat/screenshot
/// workflows and what `divider`/`imgls`/plain screenshots produce; returns
/// `None` for anything else (including JPEG — a SOF-marker scan is more
/// involved and no target tool in the color-tools coverage table needs it),
/// which callers treat as "fall back to a small fixed footprint" rather than
/// failing the whole image.
pub fn sniff_image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() >= 24 && bytes[0..8] == PNG_SIGNATURE && &bytes[12..16] == b"IHDR" {
        let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
        if w > 0 && h > 0 {
            return Some((w, h));
        }
        return None;
    }
    if bytes.len() >= 10 && (&bytes[0..6] == b"GIF87a" || &bytes[0..6] == b"GIF89a") {
        let w = u16::from_le_bytes(bytes[6..8].try_into().ok()?) as u32;
        let h = u16::from_le_bytes(bytes[8..10].try_into().ok()?) as u32;
        if w > 0 && h > 0 {
            return Some((w, h));
        }
        return None;
    }
    None
}

/// Ceiling on the accumulated base64 text across an entire multipart
/// transfer (all `FilePart=` chunks summed, not just one). Each individual
/// OSC sequence is already bounded by vte's own `MAX_OSC_RAW_STD`, but that
/// only caps *one* sequence — a multipart transfer strings many together,
/// so without a separate cap here a pathological/malicious stream of many
/// `FilePart=` chunks could grow `PendingMultipart::payload_b64` without
/// limit. Sized somewhat above `ImageStore::MAX_SESSION_IMAGE_BYTES`'s
/// decoded-bytes cap to account for base64 overhead (~4/3), so a legitimate
/// transfer at the image cap doesn't get rejected here first.
pub const MAX_MULTIPART_B64_BYTES: usize = 96 * 1024 * 1024;

/// State for an in-progress `MultipartFile=`/`FilePart=`/`FileEnd` sequence.
/// One at a time per terminal — iTerm2 doesn't define concurrent multipart
/// transfers — so a new `MultipartFile=` arriving while one is already open
/// silently replaces it rather than erroring; a client that abandons a
/// transfer mid-stream (no `FileEnd`) just leaks nothing worse than the
/// accumulated bytes of that one abandoned attempt.
#[derive(Debug, Clone, Default)]
pub struct PendingMultipart {
    pub args: FileArgs,
    pub payload_b64: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typical_imgcat_args() {
        let args = parse_file_args("inline=1;width=100%;height=1;preserveAspectRatio=0");
        assert!(args.inline);
        assert_eq!(args.width, Dimension::Percent(100));
        assert_eq!(args.height, Dimension::Cells(1));
        assert!(!args.preserve_aspect_ratio);
    }

    #[test]
    fn parses_name_and_size() {
        // "hello.png" base64-encoded.
        let encoded = Base64.encode(b"hello.png");
        let args = parse_file_args(&format!("name={encoded};size=1234;inline=1"));
        assert_eq!(args.name.as_deref(), Some("hello.png"));
        assert_eq!(args.size, Some(1234));
    }

    #[test]
    fn defaults_are_iterm2_documented_defaults() {
        let args = parse_file_args("");
        assert_eq!(args.width, Dimension::Auto);
        assert_eq!(args.height, Dimension::Auto);
        assert!(args.preserve_aspect_ratio);
        assert!(!args.inline);
    }

    #[test]
    fn a_malformed_value_falls_back_to_default_not_a_parse_abort() {
        let args = parse_file_args("width=not-a-number;inline=1");
        assert_eq!(args.width, Dimension::Auto);
        assert!(args.inline, "one bad arg must not drop the rest");
    }

    #[test]
    fn explicit_cell_dimensions_win_outright() {
        let args = FileArgs {
            width: Dimension::Cells(10),
            height: Dimension::Cells(5),
            ..Default::default()
        };
        assert_eq!(resolve_footprint(&args, 999, 999, 9, 18, 80, 24), (10, 5));
    }

    #[test]
    fn pixel_dimensions_convert_using_cell_size() {
        let args = FileArgs {
            width: Dimension::Pixels(90),
            height: Dimension::Pixels(36),
            ..Default::default()
        };
        // 90px / 9px-per-cell = 10 cells; 36px / 18px-per-cell = 2 cells.
        assert_eq!(resolve_footprint(&args, 0, 0, 9, 18, 80, 24), (10, 2));
    }

    #[test]
    fn percent_dimensions_are_relative_to_the_viewport() {
        let args = FileArgs {
            width: Dimension::Percent(50),
            height: Dimension::Percent(100),
            ..Default::default()
        };
        assert_eq!(resolve_footprint(&args, 0, 0, 9, 18, 80, 24), (40, 24));
    }

    #[test]
    fn both_auto_uses_intrinsic_size_converted_by_cell_px() {
        let args = FileArgs::default();
        // 90x36 intrinsic px at 9x18 px/cell -> 10x2 cells.
        assert_eq!(resolve_footprint(&args, 90, 36, 9, 18, 80, 24), (10, 2));
    }

    #[test]
    fn one_auto_dimension_preserves_aspect_ratio_against_the_explicit_one() {
        // A 200x100 (2:1) image, explicit width=20 cells, height=auto.
        let args = FileArgs {
            width: Dimension::Cells(20),
            height: Dimension::Auto,
            preserve_aspect_ratio: true,
            ..Default::default()
        };
        // width_px = 20*9 = 180; height_px = 180 * 100/200 = 90; height_cells = ceil(90/18) = 5.
        assert_eq!(resolve_footprint(&args, 200, 100, 9, 18, 80, 24), (20, 5));
    }

    #[test]
    fn aspect_ratio_preservation_is_skipped_when_disabled() {
        let args = FileArgs {
            width: Dimension::Cells(20),
            height: Dimension::Auto,
            preserve_aspect_ratio: false,
            ..Default::default()
        };
        // height=auto with no aspect preservation just falls back to intrinsic/cell_px.
        assert_eq!(resolve_footprint(&args, 200, 100, 9, 18, 80, 24), (20, 6));
    }

    #[test]
    fn unknown_intrinsic_size_never_produces_a_zero_footprint() {
        let args = FileArgs::default();
        assert_eq!(resolve_footprint(&args, 0, 0, 9, 18, 80, 24), (1, 1));
    }

    #[test]
    fn sniffs_png_dimensions() {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&[0, 0, 0, 13]); // IHDR length (unused by sniffer)
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&100u32.to_be_bytes());
        bytes.extend_from_slice(&50u32.to_be_bytes());
        assert_eq!(sniff_image_dimensions(&bytes), Some((100, 50)));
    }

    #[test]
    fn sniffs_gif_dimensions() {
        let mut bytes = b"GIF89a".to_vec();
        bytes.extend_from_slice(&200u16.to_le_bytes());
        bytes.extend_from_slice(&80u16.to_le_bytes());
        assert_eq!(sniff_image_dimensions(&bytes), Some((200, 80)));
    }

    #[test]
    fn unrecognized_format_sniffs_to_none() {
        assert_eq!(sniff_image_dimensions(b"not an image"), None);
    }

    #[test]
    fn truncated_png_header_sniffs_to_none_not_a_panic() {
        let bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(sniff_image_dimensions(&bytes), None);
    }
}
