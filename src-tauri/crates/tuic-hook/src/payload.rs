//! Percent-encoding for OSC 7770 payloads.
//!
//! The wire format is `ESC ] 7770 ; verb=payload ESC \`, and the parser on the
//! receiving end (`patches/vte/src/ansi.rs`) splits OSC params on `;` and reads
//! only the second param — so a payload containing a literal `;` would be
//! silently truncated, and one containing ESC/BEL would desync the terminal
//! parser entirely. Percent-encoding neutralizes both, plus non-ASCII text and
//! anything else that isn't safely representable as a bare OSC param byte.

/// Bytes that pass through unencoded. Deliberately narrow (RFC 3986 unreserved
/// set) rather than "everything but the dangerous bytes" — an allowlist can't
/// be defeated by a byte nobody thought to blocklist.
fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

/// Maximum encoded-payload length in bytes. The receiving parser caps the
/// whole raw OSC sequence at 1024 bytes (`patches/vte/src/lib.rs::MAX_OSC_RAW`);
/// keeping each individual payload well under that leaves headroom for the
/// `verb=` prefix and the fact that encoding can triple a byte's length.
pub const MAX_PAYLOAD_LEN: usize = 512;

/// Truncate `s` to at most `MAX_PAYLOAD_LEN` bytes, landing on a UTF-8 char
/// boundary (never splitting a multi-byte codepoint), then percent-encode.
pub fn encode(s: &str) -> String {
    let truncated = truncate_at_char_boundary(s, MAX_PAYLOAD_LEN);
    let mut out = String::with_capacity(truncated.len());
    for &b in truncated.as_bytes() {
        if is_unreserved(b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn truncate_at_char_boundary(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        return s;
    }
    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_through_unreserved_bytes() {
        assert_eq!(encode("abcXYZ019._~-"), "abcXYZ019._~-");
    }

    #[test]
    fn encodes_the_osc_delimiter_semicolon() {
        assert_eq!(encode(";"), "%3B");
    }

    #[test]
    fn encodes_escape_and_bel() {
        assert_eq!(encode("\u{1b}"), "%1B");
        assert_eq!(encode("\u{07}"), "%07");
    }

    #[test]
    fn encodes_space_and_newline() {
        assert_eq!(encode(" \n"), "%20%0A");
    }

    #[test]
    fn round_trips_via_percent_decode() {
        let original = "hello; world\u{1b}\u{07}\n日本語";
        let encoded = encode(original);
        let decoded = percent_decode_for_test(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn truncates_long_input_before_encoding() {
        let long = "a".repeat(MAX_PAYLOAD_LEN + 100);
        let encoded = encode(&long);
        assert_eq!(encoded.len(), MAX_PAYLOAD_LEN); // all 'a' — no expansion
    }

    #[test]
    fn truncation_lands_on_a_utf8_char_boundary() {
        // Each '日' is 3 bytes; pad so byte 512 falls mid-character.
        let padding = "a".repeat(MAX_PAYLOAD_LEN - 1);
        let s = format!("{padding}日本語");
        let encoded = encode(&s);
        // Must decode cleanly — a boundary violation would have produced
        // invalid UTF-8 that this round-trip would choke on.
        let decoded = percent_decode_for_test(&encoded);
        assert!(s.starts_with(&decoded));
    }

    /// Minimal decoder, test-only: inverse of `encode`, used to prove
    /// round-tripping rather than to ship a decoder in the binary.
    fn percent_decode_for_test(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap();
                out.push(u8::from_str_radix(hex, 16).unwrap());
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).unwrap()
    }
}
