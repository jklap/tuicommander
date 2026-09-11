//! Kitty graphics protocol (`ESC _ G ... ST`) — color-tools plan, Phase 3.
//!
//! Pure parsing only, mirroring `iterm2.rs`'s split: grid mutation stays in
//! `term/mod.rs`'s `Handler` impl.
//!
//! # Scope (deliberately narrower than the full spec)
//!
//! Implemented: `a=t/T/p/d/q`, `f=24/32/100`, all four transmission mediums
//! (`t=d` direct, `t=f` file, `t=t` temp-file-delete-after-read, `t=s`
//! shared memory — the actual file/shared-memory I/O is delegated to the
//! embedding application via `EventListener::read_file_medium`/
//! `read_shm_medium`, see `terminal_images.rs` in the app crate for the
//! safety guards), `o=z` zlib decompression, `m=` chunking,
//! `i=`/`p=`/`c=`/`r=`/`z=`/`C=`/`q=`, the capability probe response, and
//! (color-tools plan, Phase 7) `U=1` Unicode virtual placeholders — the
//! diacritic table below (`ROWCOL_DIACRITICS`) is transcribed verbatim from
//! Kitty's own authoritative `rowcolumn-diacritics.txt`, verified against
//! the protocol docs' own worked examples (2x2 grid, most-significant-byte,
//! inheritance) via real end-to-end tests in `terminal_grid.rs` before being
//! trusted, not just unit-tested in isolation.

use core::str;

/// What `a=` requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Action {
    /// `a=t`: transmit only, do not display.
    Transmit,
    /// `a=T`: transmit and display in one step.
    #[default]
    TransmitAndDisplay,
    /// `a=p`: create a placement for an already-transmitted image.
    Place,
    /// `a=d`: delete.
    Delete,
    /// `a=q`: capability query — must not display or store anything, only
    /// acknowledge.
    Query,
    /// Anything else (`a=f`/`a=c`, animation frames/composition) — not
    /// implemented; treated as a no-op rather than an error, since these
    /// don't have a well-defined "reject" response of their own.
    Unsupported,
}

/// `f=`: pixel data format for direct/file transmission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    /// `f=24`: raw 24-bit RGB, no container.
    Rgb,
    /// `f=32`: raw 32-bit RGBA, no container.
    Rgba,
    /// `f=100` (the default): a PNG file.
    #[default]
    Png,
}

/// `t=`: transmission medium.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Medium {
    /// `t=d` (the default): payload rides in the escape sequence itself.
    #[default]
    Direct,
    /// `t=f`: a plain file path — read via `EventListener::read_file_medium`.
    File,
    /// `t=t`: a temp file the terminal must delete after reading — same
    /// reader as `File`, with `delete_after=true`; the actual delete-after-
    /// read guard (only within the system temp dir) lives in the app crate.
    TempFile,
    /// `t=s`: POSIX/Windows shared memory — read via
    /// `EventListener::read_shm_medium`.
    SharedMemory,
}

/// One Kitty control-data key=value set — the part of an APC payload
/// before the `;` that introduces the base64 payload (if any).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ControlData {
    pub action: Action,
    pub format: Format,
    pub medium: Medium,
    pub image_id: u32,
    pub placement_id: u32,
    /// `s=`: source pixel width, required for raw (`f=24`/`f=32`) formats.
    pub width_px: u32,
    /// `v=`: source pixel height, required for raw formats.
    pub height_px: u32,
    /// `c=`: placement width in cells, 0 if unspecified (resolve from
    /// intrinsic size).
    pub cols: u32,
    /// `r=`: placement height in cells, 0 if unspecified.
    pub rows: u32,
    pub z_index: i32,
    /// `C=`: `true` means do not move the cursor after display (default
    /// `false` — move it, like iTerm2 always does).
    pub no_move_cursor: bool,
    /// `q=`: 0 = send OK and errors, 1 = suppress OK only, 2 = suppress both.
    pub quiet: u8,
    /// `m=1`: more chunks follow; `m=0`/absent: this is the last (or only)
    /// chunk.
    pub more_chunks: bool,
    /// `U=1`: this placement uses the Unicode virtual-placeholder scheme —
    /// see the module doc comment for what's actually implemented.
    pub unicode_placeholder: bool,
    /// `o=z`: the payload is zlib-compressed; decompressed via `flate2` in
    /// `term/mod.rs`'s `kitty_process` before format dispatch.
    pub compressed: bool,
    /// `d=`: delete-target letter, if `action == Delete`.
    pub delete_target: Option<char>,
}

/// Strip the literal `G` marker Kitty's graphics protocol always places
/// right after the APC introducer (`ESC _ G ...` — the accumulated `data`
/// `term/mod.rs`'s `kitty_graphics` receives still has it attached), then
/// split what remains into control-data text and the still-base64-encoded
/// payload, if any (`;`-separated, at most once, since base64 never
/// contains `;`). Returns `None` if `data` doesn't start with `G` — i.e. it
/// isn't a Kitty graphics sequence (some other, unimplemented APC use).
pub fn split_payload(data: &[u8]) -> Option<(&str, &[u8])> {
    let rest = data.strip_prefix(b"G")?;
    Some(match rest.iter().position(|&b| b == b';') {
        Some(idx) => (str::from_utf8(&rest[..idx]).unwrap_or(""), &rest[idx + 1..]),
        None => (str::from_utf8(rest).unwrap_or(""), &[] as &[u8]),
    })
}

/// Parse a comma-separated `key=value` control-data string. Unknown keys
/// and malformed values are ignored (fall back to the default), matching
/// `iterm2::parse_file_args`'s "one bad argument shouldn't lose the image"
/// philosophy.
pub fn parse_control_data(text: &str) -> ControlData {
    let mut cd = ControlData::default();
    for part in text.split(',') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key {
            "a" => {
                cd.action = match value {
                    "t" => Action::Transmit,
                    "T" => Action::TransmitAndDisplay,
                    "p" => Action::Place,
                    "d" => Action::Delete,
                    "q" => Action::Query,
                    _ => Action::Unsupported,
                }
            }
            "f" => {
                cd.format = match value {
                    "24" => Format::Rgb,
                    "32" => Format::Rgba,
                    _ => Format::Png,
                }
            }
            "t" => {
                cd.medium = match value {
                    "f" => Medium::File,
                    "t" => Medium::TempFile,
                    "s" => Medium::SharedMemory,
                    _ => Medium::Direct,
                }
            }
            "i" => cd.image_id = value.parse().unwrap_or(0),
            "p" => cd.placement_id = value.parse().unwrap_or(0),
            "s" => cd.width_px = value.parse().unwrap_or(0),
            "v" => cd.height_px = value.parse().unwrap_or(0),
            "c" => cd.cols = value.parse().unwrap_or(0),
            "r" => cd.rows = value.parse().unwrap_or(0),
            "z" => cd.z_index = value.parse().unwrap_or(0),
            "C" => cd.no_move_cursor = value == "1",
            "q" => cd.quiet = value.parse().unwrap_or(0),
            "m" => cd.more_chunks = value == "1",
            "U" => cd.unicode_placeholder = value == "1",
            "o" => cd.compressed = value == "z",
            "d" => cd.delete_target = value.chars().next(),
            _ => {}
        }
    }
    cd
}

/// State for an in-progress chunked transmission (`m=1` on one or more
/// sequences, `m=0`/absent on the last). Only the *first* chunk carries the
/// full control-data set — per spec, continuation chunks have only `m=`
/// (and optionally `q=`) — so it's captured once, here.
#[derive(Debug, Clone)]
pub struct PendingTransmission {
    pub control: ControlData,
    pub payload_b64: Vec<u8>,
}

/// Cap on accumulated base64 across all chunks of one `m=1` transmission —
/// mirrors `iterm2::MAX_MULTIPART_B64_BYTES`'s "fail closed" pattern for the
/// same reason: each individual APC dispatch is already bounded by vte's own
/// `MAX_OSC_RAW_STD`, but nothing previously stopped a client from streaming
/// an unbounded *number* of chunks with `m=1` set forever, growing
/// `PendingTransmission::payload_b64` without limit before the app-level
/// `MAX_SESSION_IMAGE_BYTES` cap ever got a chance to reject it.
pub const MAX_CHUNKED_B64_BYTES: usize = 96 * 1024 * 1024;

/// Format a success response: `\x1b_Gi=<id>[,p=<placement>];OK\x1b\\`.
pub fn ok_response(image_id: u32, placement_id: u32) -> String {
    if placement_id != 0 {
        format!("\x1b_Gi={image_id},p={placement_id};OK\x1b\\")
    } else {
        format!("\x1b_Gi={image_id};OK\x1b\\")
    }
}

/// Format an error response: `\x1b_Gi=<id>;<code>:<message>\x1b\\`.
pub fn error_response(image_id: u32, code: &str, message: &str) -> String {
    format!("\x1b_Gi={image_id};{code}:{message}\x1b\\")
}

/// Unicode virtual placeholders (`U=1`) — color-tools plan, Phase 7.
///
/// The wire mechanism (see
/// <https://sw.kovidgoyal.net/kitty/graphics-protocol/#unicode-placeholders>):
/// an app prints `U+10EEEE` as an ordinary character, immediately followed by
/// up to three zero-width Unicode combining-mark diacritics encoding, in
/// order, the tile's row, column, and (optionally) the image id's most-
/// significant byte — a true-color or 256-color SGR foreground encodes the
/// low 24/8 bits of the image id, and the underline color encodes the
/// placement id. This is pure terminal *text*; the terminal's job is only to
/// recognize the pattern during ordinary cell writes (`Term::input`, in
/// `term/mod.rs`) and attach an `ImageCellRef` instead of storing a glyph —
/// never to reserve space or move the cursor, unlike the OSC 1337/ordinary
/// Kitty placement path.
///
/// The diacritic table below is transcribed verbatim from Kitty's own
/// authoritative `rowcolumn-diacritics.txt` (linked from the page above),
/// preserving file order exactly — verified against that page's own worked
/// examples before being trusted: index 0 is `U+305` (the diacritic the docs
/// give for row/column value 0), index 1 is `U+30D` (value 1), and index 2
/// is `U+30E` (value 2, used in the docs' most-significant-byte example).
pub const UNICODE_PLACEHOLDER: char = '\u{10EEEE}';

const ROWCOL_DIACRITICS: [char; 297] = [
    '\u{305}',
    '\u{30D}',
    '\u{30E}',
    '\u{310}',
    '\u{312}',
    '\u{33D}',
    '\u{33E}',
    '\u{33F}',
    '\u{346}',
    '\u{34A}',
    '\u{34B}',
    '\u{34C}',
    '\u{350}',
    '\u{351}',
    '\u{352}',
    '\u{357}',
    '\u{35B}',
    '\u{363}',
    '\u{364}',
    '\u{365}',
    '\u{366}',
    '\u{367}',
    '\u{368}',
    '\u{369}',
    '\u{36A}',
    '\u{36B}',
    '\u{36C}',
    '\u{36D}',
    '\u{36E}',
    '\u{36F}',
    '\u{483}',
    '\u{484}',
    '\u{485}',
    '\u{486}',
    '\u{487}',
    '\u{592}',
    '\u{593}',
    '\u{594}',
    '\u{595}',
    '\u{597}',
    '\u{598}',
    '\u{599}',
    '\u{59C}',
    '\u{59D}',
    '\u{59E}',
    '\u{59F}',
    '\u{5A0}',
    '\u{5A1}',
    '\u{5A8}',
    '\u{5A9}',
    '\u{5AB}',
    '\u{5AC}',
    '\u{5AF}',
    '\u{5C4}',
    '\u{610}',
    '\u{611}',
    '\u{612}',
    '\u{613}',
    '\u{614}',
    '\u{615}',
    '\u{616}',
    '\u{617}',
    '\u{657}',
    '\u{658}',
    '\u{659}',
    '\u{65A}',
    '\u{65B}',
    '\u{65D}',
    '\u{65E}',
    '\u{6D6}',
    '\u{6D7}',
    '\u{6D8}',
    '\u{6D9}',
    '\u{6DA}',
    '\u{6DB}',
    '\u{6DC}',
    '\u{6DF}',
    '\u{6E0}',
    '\u{6E1}',
    '\u{6E2}',
    '\u{6E4}',
    '\u{6E7}',
    '\u{6E8}',
    '\u{6EB}',
    '\u{6EC}',
    '\u{730}',
    '\u{732}',
    '\u{733}',
    '\u{735}',
    '\u{736}',
    '\u{73A}',
    '\u{73D}',
    '\u{73F}',
    '\u{740}',
    '\u{741}',
    '\u{743}',
    '\u{745}',
    '\u{747}',
    '\u{749}',
    '\u{74A}',
    '\u{7EB}',
    '\u{7EC}',
    '\u{7ED}',
    '\u{7EE}',
    '\u{7EF}',
    '\u{7F0}',
    '\u{7F1}',
    '\u{7F3}',
    '\u{816}',
    '\u{817}',
    '\u{818}',
    '\u{819}',
    '\u{81B}',
    '\u{81C}',
    '\u{81D}',
    '\u{81E}',
    '\u{81F}',
    '\u{820}',
    '\u{821}',
    '\u{822}',
    '\u{823}',
    '\u{825}',
    '\u{826}',
    '\u{827}',
    '\u{829}',
    '\u{82A}',
    '\u{82B}',
    '\u{82C}',
    '\u{82D}',
    '\u{951}',
    '\u{953}',
    '\u{954}',
    '\u{F82}',
    '\u{F83}',
    '\u{F86}',
    '\u{F87}',
    '\u{135D}',
    '\u{135E}',
    '\u{135F}',
    '\u{17DD}',
    '\u{193A}',
    '\u{1A17}',
    '\u{1A75}',
    '\u{1A76}',
    '\u{1A77}',
    '\u{1A78}',
    '\u{1A79}',
    '\u{1A7A}',
    '\u{1A7B}',
    '\u{1A7C}',
    '\u{1B6B}',
    '\u{1B6D}',
    '\u{1B6E}',
    '\u{1B6F}',
    '\u{1B70}',
    '\u{1B71}',
    '\u{1B72}',
    '\u{1B73}',
    '\u{1CD0}',
    '\u{1CD1}',
    '\u{1CD2}',
    '\u{1CDA}',
    '\u{1CDB}',
    '\u{1CE0}',
    '\u{1DC0}',
    '\u{1DC1}',
    '\u{1DC3}',
    '\u{1DC4}',
    '\u{1DC5}',
    '\u{1DC6}',
    '\u{1DC7}',
    '\u{1DC8}',
    '\u{1DC9}',
    '\u{1DCB}',
    '\u{1DCC}',
    '\u{1DD1}',
    '\u{1DD2}',
    '\u{1DD3}',
    '\u{1DD4}',
    '\u{1DD5}',
    '\u{1DD6}',
    '\u{1DD7}',
    '\u{1DD8}',
    '\u{1DD9}',
    '\u{1DDA}',
    '\u{1DDB}',
    '\u{1DDC}',
    '\u{1DDD}',
    '\u{1DDE}',
    '\u{1DDF}',
    '\u{1DE0}',
    '\u{1DE1}',
    '\u{1DE2}',
    '\u{1DE3}',
    '\u{1DE4}',
    '\u{1DE5}',
    '\u{1DE6}',
    '\u{1DFE}',
    '\u{20D0}',
    '\u{20D1}',
    '\u{20D4}',
    '\u{20D5}',
    '\u{20D6}',
    '\u{20D7}',
    '\u{20DB}',
    '\u{20DC}',
    '\u{20E1}',
    '\u{20E7}',
    '\u{20E9}',
    '\u{20F0}',
    '\u{2CEF}',
    '\u{2CF0}',
    '\u{2CF1}',
    '\u{2DE0}',
    '\u{2DE1}',
    '\u{2DE2}',
    '\u{2DE3}',
    '\u{2DE4}',
    '\u{2DE5}',
    '\u{2DE6}',
    '\u{2DE7}',
    '\u{2DE8}',
    '\u{2DE9}',
    '\u{2DEA}',
    '\u{2DEB}',
    '\u{2DEC}',
    '\u{2DED}',
    '\u{2DEE}',
    '\u{2DEF}',
    '\u{2DF0}',
    '\u{2DF1}',
    '\u{2DF2}',
    '\u{2DF3}',
    '\u{2DF4}',
    '\u{2DF5}',
    '\u{2DF6}',
    '\u{2DF7}',
    '\u{2DF8}',
    '\u{2DF9}',
    '\u{2DFA}',
    '\u{2DFB}',
    '\u{2DFC}',
    '\u{2DFD}',
    '\u{2DFE}',
    '\u{2DFF}',
    '\u{A66F}',
    '\u{A67C}',
    '\u{A67D}',
    '\u{A6F0}',
    '\u{A6F1}',
    '\u{A8E0}',
    '\u{A8E1}',
    '\u{A8E2}',
    '\u{A8E3}',
    '\u{A8E4}',
    '\u{A8E5}',
    '\u{A8E6}',
    '\u{A8E7}',
    '\u{A8E8}',
    '\u{A8E9}',
    '\u{A8EA}',
    '\u{A8EB}',
    '\u{A8EC}',
    '\u{A8ED}',
    '\u{A8EE}',
    '\u{A8EF}',
    '\u{A8F0}',
    '\u{A8F1}',
    '\u{AAB0}',
    '\u{AAB2}',
    '\u{AAB3}',
    '\u{AAB7}',
    '\u{AAB8}',
    '\u{AABE}',
    '\u{AABF}',
    '\u{AAC1}',
    '\u{FE20}',
    '\u{FE21}',
    '\u{FE22}',
    '\u{FE23}',
    '\u{FE24}',
    '\u{FE25}',
    '\u{FE26}',
    '\u{10A0F}',
    '\u{10A38}',
    '\u{1D185}',
    '\u{1D186}',
    '\u{1D187}',
    '\u{1D188}',
    '\u{1D189}',
    '\u{1D1AA}',
    '\u{1D1AB}',
    '\u{1D1AC}',
    '\u{1D1AD}',
    '\u{1D242}',
    '\u{1D243}',
    '\u{1D244}',
];

/// The numeric value (0..297) a diacritic encodes, if it's one of the
/// recognized row/column/most-significant-byte diacritics.
pub fn diacritic_value(c: char) -> Option<u32> {
    ROWCOL_DIACRITICS
        .iter()
        .position(|&d| d == c)
        .map(|i| i as u32)
}

/// One placeholder cell's diacritics, assigned roles purely by *position*
/// among the recognized diacritics present (any non-diacritic zero-width
/// character mixed in is ignored) — row first, then column, then the
/// image id's most-significant byte, matching the order the spec's own
/// worked examples use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlaceholderDiacritics {
    pub row: Option<u32>,
    pub col: Option<u32>,
    pub msb: Option<u32>,
}

/// Scan a cell's zero-width characters for up to three recognized
/// diacritics, in the order they appear.
pub fn parse_placeholder_diacritics(zerowidth: &[char]) -> PlaceholderDiacritics {
    let mut values = zerowidth.iter().filter_map(|&c| diacritic_value(c));
    PlaceholderDiacritics {
        row: values.next(),
        col: values.next(),
        msb: values.next(),
    }
}

/// A previously-resolved placeholder cell's tile position, for applying the
/// spec's left-to-right inheritance rule. The caller is responsible for only
/// constructing one when the candidate left-neighbor cell's foreground and
/// underline colors both match the current cell's — this type doesn't carry
/// colors itself, matching `resolve_placeholder_tile`'s pure, color-agnostic
/// signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedPlaceholder {
    pub row: u32,
    pub col: u32,
    pub msb: u32,
}

/// Resolve one placeholder cell's `(row, col, msb)` per the spec's
/// three-tier left-neighbor inheritance (quoted in full in this module's
/// doc comment's source page): with both row and column diacritics present,
/// only `msb` may still be inherited; with only row present, column
/// (`left.col + 1`) and `msb` inherit if `left.row` matches — and column
/// defaults to `0` (a fresh run starting at this row) when there is no
/// matching left neighbor to inherit from, matching the spec's own "2 rows
/// by 3 columns" example, where every row's first cell carries only a row
/// diacritic; with neither row nor column present, everything inherits from
/// `left` unconditionally (the caller has already confirmed `left`'s colors
/// match) — this is the one case with no explicit-diacritic fallback, so it
/// returns `(None, None, 0)` (unresolvable) when there is no `left` at all.
pub fn resolve_placeholder_tile(
    diacritics: PlaceholderDiacritics,
    left: Option<ResolvedPlaceholder>,
) -> (Option<u32>, Option<u32>, u32) {
    match (diacritics.row, diacritics.col) {
        (Some(row), Some(col)) => {
            let msb = diacritics.msb.or(left.map(|l| l.msb)).unwrap_or(0);
            (Some(row), Some(col), msb)
        }
        (Some(row), None) => {
            let inherited = left.filter(|l| l.row == row);
            let col = inherited.map(|l| l.col + 1).unwrap_or(0);
            let msb = inherited.map(|l| l.msb).unwrap_or(0);
            (Some(row), Some(col), msb)
        }
        (None, _) => match left {
            Some(l) => (Some(l.row), Some(l.col + 1), l.msb),
            None => (None, None, 0),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_control_data_from_payload() {
        let (text, payload) = split_payload(b"Gi=1,a=T,f=24;aGVsbG8=").expect("has G marker");
        assert_eq!(text, "i=1,a=T,f=24");
        assert_eq!(payload, b"aGVsbG8=");
    }

    #[test]
    fn splits_with_no_payload_at_all() {
        let (text, payload) = split_payload(b"Ga=d,d=A").expect("has G marker");
        assert_eq!(text, "a=d,d=A");
        assert_eq!(payload, b"");
    }

    #[test]
    fn missing_g_marker_is_not_a_kitty_graphics_sequence() {
        assert_eq!(split_payload(b"i=1,a=T"), None);
    }

    #[test]
    fn parses_typical_transmit_and_display() {
        let (text, _) = split_payload(b"Gi=31,a=T,f=100,s=1,v=1,c=10,r=5").unwrap();
        let cd = parse_control_data(text);
        assert_eq!(cd.image_id, 31);
        assert_eq!(cd.action, Action::TransmitAndDisplay);
        assert_eq!(cd.format, Format::Png);
        assert_eq!(cd.cols, 10);
        assert_eq!(cd.rows, 5);
    }

    #[test]
    fn unknown_medium_letter_defaults_to_direct() {
        let cd = parse_control_data("t=x");
        assert_eq!(cd.medium, Medium::Direct);
    }

    #[test]
    fn recognizes_non_direct_mediums() {
        assert_eq!(parse_control_data("t=f").medium, Medium::File);
        assert_eq!(parse_control_data("t=t").medium, Medium::TempFile);
        assert_eq!(parse_control_data("t=s").medium, Medium::SharedMemory);
    }

    #[test]
    fn a_malformed_numeric_value_falls_back_to_zero_not_a_parse_abort() {
        let cd = parse_control_data("i=not-a-number,a=T");
        assert_eq!(cd.image_id, 0);
        assert_eq!(
            cd.action,
            Action::TransmitAndDisplay,
            "one bad key must not drop the rest"
        );
    }

    #[test]
    fn query_action_is_recognized() {
        assert_eq!(
            parse_control_data("a=q,i=5,t=d,f=24,s=1,v=1").action,
            Action::Query
        );
    }

    #[test]
    fn delete_action_and_target() {
        let cd = parse_control_data("a=d,d=I,i=10");
        assert_eq!(cd.action, Action::Delete);
        assert_eq!(cd.delete_target, Some('I'));
        assert_eq!(cd.image_id, 10);
    }

    #[test]
    fn ok_response_format_without_placement() {
        assert_eq!(ok_response(31, 0), "\x1b_Gi=31;OK\x1b\\");
    }

    #[test]
    fn ok_response_format_with_placement() {
        assert_eq!(ok_response(31, 7), "\x1b_Gi=31,p=7;OK\x1b\\");
    }

    #[test]
    fn error_response_format() {
        assert_eq!(
            error_response(31, "EINVAL", "bad format"),
            "\x1b_Gi=31;EINVAL:bad format\x1b\\"
        );
    }

    // --- Unicode placeholder diacritics (color-tools plan, Phase 7) ---

    #[test]
    fn diacritic_table_matches_the_specs_own_worked_examples() {
        // https://sw.kovidgoyal.net/kitty/graphics-protocol/#unicode-placeholders
        // states explicitly: "U+305 is the diacritic corresponding to the
        // number 0 and U+30D corresponds to 1", and separately "U+30E is
        // the diacritic corresponding to the number 2" (used in its
        // most-significant-byte example).
        assert_eq!(diacritic_value('\u{305}'), Some(0));
        assert_eq!(diacritic_value('\u{30D}'), Some(1));
        assert_eq!(diacritic_value('\u{30E}'), Some(2));
        assert_eq!(ROWCOL_DIACRITICS.len(), 297);
    }

    #[test]
    fn an_ordinary_character_is_not_a_diacritic() {
        assert_eq!(diacritic_value('a'), None);
        assert_eq!(diacritic_value('\u{301}'), None); // COMBINING ACUTE ACCENT — not in the table
    }

    #[test]
    fn parses_row_and_column_diacritics_from_the_specs_2x2_example() {
        // The spec's literal `printf` example for a 2x2 grid of image 42:
        //   \U10EEEE\U0305\U0305  (0,0)   \U10EEEE\U0305\U030D  (0,1)
        //   \U10EEEE\U030D\U0305  (1,0)   \U10EEEE\U030D\U030D  (1,1)
        let d = parse_placeholder_diacritics(&['\u{305}', '\u{305}']);
        assert_eq!(
            d,
            PlaceholderDiacritics {
                row: Some(0),
                col: Some(0),
                msb: None
            }
        );
        let d = parse_placeholder_diacritics(&['\u{305}', '\u{30D}']);
        assert_eq!(
            d,
            PlaceholderDiacritics {
                row: Some(0),
                col: Some(1),
                msb: None
            }
        );
        let d = parse_placeholder_diacritics(&['\u{30D}', '\u{305}']);
        assert_eq!(
            d,
            PlaceholderDiacritics {
                row: Some(1),
                col: Some(0),
                msb: None
            }
        );
    }

    #[test]
    fn parses_the_third_msb_diacritic_from_the_specs_own_example() {
        // The spec's own example for image ID 33554474 = 42 + (2 << 24):
        // `\U10EEEE\U0305\U0305\U030E` — row 0, col 0, msb 2.
        let d = parse_placeholder_diacritics(&['\u{305}', '\u{305}', '\u{30E}']);
        assert_eq!(
            d,
            PlaceholderDiacritics {
                row: Some(0),
                col: Some(0),
                msb: Some(2)
            }
        );
    }

    #[test]
    fn ignores_a_non_diacritic_zero_width_character_mixed_in() {
        // A stray combining char the table doesn't recognize must not shift
        // which position row/col/msb land on.
        let d = parse_placeholder_diacritics(&['\u{301}', '\u{305}', '\u{30D}']);
        assert_eq!(
            d,
            PlaceholderDiacritics {
                row: Some(0),
                col: Some(1),
                msb: None
            }
        );
    }

    #[test]
    fn full_diacritics_present_never_needs_inheritance() {
        let d = PlaceholderDiacritics {
            row: Some(3),
            col: Some(4),
            msb: Some(1),
        };
        assert_eq!(resolve_placeholder_tile(d, None), (Some(3), Some(4), 1));
        // A left neighbor's msb is ignored when this cell already states its own.
        let left = ResolvedPlaceholder {
            row: 9,
            col: 9,
            msb: 9,
        };
        assert_eq!(
            resolve_placeholder_tile(d, Some(left)),
            (Some(3), Some(4), 1)
        );
    }

    #[test]
    fn msb_inherits_from_the_left_neighbor_when_row_and_col_are_both_explicit() {
        let d = PlaceholderDiacritics {
            row: Some(3),
            col: Some(4),
            msb: None,
        };
        let left = ResolvedPlaceholder {
            row: 1,
            col: 2,
            msb: 7,
        };
        assert_eq!(
            resolve_placeholder_tile(d, Some(left)),
            (Some(3), Some(4), 7)
        );
    }

    #[test]
    fn row_only_inherits_column_plus_one_and_msb_when_the_left_row_matches() {
        let d = PlaceholderDiacritics {
            row: Some(5),
            col: None,
            msb: None,
        };
        let left = ResolvedPlaceholder {
            row: 5,
            col: 2,
            msb: 7,
        };
        assert_eq!(
            resolve_placeholder_tile(d, Some(left)),
            (Some(5), Some(3), 7)
        );
    }

    #[test]
    fn row_only_starts_a_fresh_column_at_zero_when_the_left_rows_do_not_match() {
        let d = PlaceholderDiacritics {
            row: Some(5),
            col: None,
            msb: None,
        };
        let left = ResolvedPlaceholder {
            row: 4,
            col: 2,
            msb: 7,
        };
        assert_eq!(
            resolve_placeholder_tile(d, Some(left)),
            (Some(5), Some(0), 0)
        );
    }

    #[test]
    fn no_diacritics_at_all_inherits_everything_from_the_left_neighbor() {
        let d = PlaceholderDiacritics::default();
        let left = ResolvedPlaceholder {
            row: 5,
            col: 2,
            msb: 7,
        };
        assert_eq!(
            resolve_placeholder_tile(d, Some(left)),
            (Some(5), Some(3), 7)
        );
    }

    #[test]
    fn no_diacritics_and_no_left_neighbor_is_unresolvable() {
        let d = PlaceholderDiacritics::default();
        assert_eq!(resolve_placeholder_tile(d, None), (None, None, 0));
    }

    #[test]
    fn a_lone_row_diacritic_with_no_left_neighbor_at_all_starts_a_fresh_column_at_zero() {
        // Matches the spec's own "2 rows by 3 columns" example, where every
        // row's first cell carries only a row diacritic and no left
        // neighbor exists yet.
        let d = PlaceholderDiacritics {
            row: Some(5),
            col: None,
            msb: None,
        };
        assert_eq!(resolve_placeholder_tile(d, None), (Some(5), Some(0), 0));
    }
}
