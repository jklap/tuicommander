//! Kitty graphics protocol (`ESC _ G ... ST`) — color-tools plan, Phase 3.
//!
//! Pure parsing only, mirroring `iterm2.rs`'s split: grid mutation stays in
//! `term/mod.rs`'s `Handler` impl.
//!
//! # Scope (deliberately narrower than the full spec)
//!
//! Implemented: `a=t/T/p/d/q`, `f=24/32/100`, `t=d` (direct transmission),
//! `m=` chunking, `i=`/`p=`/`c=`/`r=`/`z=`/`C=`/`q=`, and the capability
//! probe response. **Not implemented, and returns a protocol-correct error
//! response rather than silently misbehaving:** `t=f`/`t=t`/`t=s`
//! (file/temp-file/shared-memory transmission — `t=d` alone still covers
//! most real clients, several of which fall back to it) and `o=z` (zlib
//! compression). **Registered but not yet wired to cell text:** `U=1`
//! Unicode virtual placeholders — the `a=p,U=1` registration exists, but
//! decoding the actual `U+10EEEE` diacritic-encoded placeholder characters
//! an app prints is a separate, not-yet-implemented step (the diacritic
//! table is large and error-prone to transcribe without a canonical
//! reference to check against; shipping a wrong mapping would silently
//! misplace image tiles, which is worse than not supporting it yet).

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
    /// `t=f`: a plain file path — not implemented.
    File,
    /// `t=t`: a temp file the terminal must delete after reading — not
    /// implemented (the delete-after-read semantics are exactly the
    /// arbitrary-file-deletion risk the color-tools plan flags).
    TempFile,
    /// `t=s`: POSIX/Windows shared memory — not implemented.
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
    /// `o=z` was requested (compression) — not implemented; the caller
    /// should respond with an error rather than attempt to display
    /// mis-decoded pixel data.
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
}
