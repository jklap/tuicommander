//! Elide inline-image payload bytes from diagnostic-only sinks (color-tools
//! plan, Phase 8 — "diagnostics-ring elision").
//!
//! The PTY flight-recorder ring (`pty_raw_rings`, `PTY_RAW_RING_CAP`) and
//! `.tcap` captures (`pty_capture.rs`) exist purely so a human/agent can dump
//! or replay what a session actually saw — see AGENTS.md's "capture before
//! you theorise" workflow. A single ~1 MB image transmission's base64
//! payload would otherwise evict most of a 2 MB ring, or overflow a 512 KB
//! capture outright, the moment anyone runs an image tool. This module
//! replaces just the payload portion of an OSC 1337 / Kitty APC graphics
//! sequence with a short placeholder before bytes reach either sink — the
//! REAL parser (vte/alacritty_terminal) is never touched by this and always
//! sees the original, unelided bytes.
//!
//! **Explicitly NOT applied to `output_buffers`/`broadcast_to_ws_clients`**
//! (the ring backing a browser/remote client's live stream + reconnect
//! replay) — unlike the flight recorder and `.tcap`, that ring is a real,
//! functional replay mechanism for an actual client, not a debugging aid;
//! eliding there would visibly break image display for whatever consumes
//! that raw stream. Only genuinely diagnostic-only sinks are in scope here.
//!
//! **Scope, deliberately narrow**: only sequences fully contained within the
//! bytes passed to a single call are elided. A sequence split across a PTY
//! read boundary (a large image happening to straddle a 64 KB `read()`) is
//! passed through unelided — exactly today's behavior, so this is a
//! strict improvement with no new failure mode, not a partial fix that
//! could corrupt anything. Diagnostic sinks tolerate an occasional
//! unelided payload; they must never receive corrupted framing.

const OSC_1337_PREFIX: &[u8] = b"\x1b]1337;";
const KITTY_APC_PREFIX: &[u8] = b"\x1b_G";
const ST: &[u8] = b"\x1b\\";
const BEL: u8 = 0x07;
const PLACEHOLDER_PREFIX: &[u8] = b"<image ";
const PLACEHOLDER_SUFFIX: &[u8] = b" bytes elided>";

/// Elide any OSC 1337 / Kitty APC graphics payload found in `bytes`. Returns
/// `None` (no allocation) when nothing needed eliding — the common case for
/// the vast majority of PTY output — so callers on a hot path only pay for
/// the scan, not a copy, unless there was actually something to change.
pub(crate) fn elide_image_payloads(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out: Option<Vec<u8>> = None;
    let mut i = 0;
    while i < bytes.len() {
        if let Some(span) = match_osc_1337_payload(&bytes[i..]) {
            push_elided_span(&mut out, bytes, i, span);
            i += span.end;
            continue;
        }
        if let Some(span) = match_kitty_apc_payload(&bytes[i..]) {
            push_elided_span(&mut out, bytes, i, span);
            i += span.end;
            continue;
        }
        if let Some(ref mut buf) = out {
            buf.push(bytes[i]);
        }
        i += 1;
    }
    out
}

/// The payload region within one matched sequence, relative to its own
/// start (`base`) — `payload_start..payload_end` is what gets replaced;
/// `end` is where the whole matched sequence ends (resume scanning there).
#[derive(Clone, Copy)]
struct PayloadSpan {
    payload_start: usize,
    payload_end: usize,
    end: usize,
}

/// `\x1b]1337;<args>:<base64>` + `BEL` or `ST`. `args` may itself contain no
/// `:` at all (e.g. a bare `FileEnd` with nothing after it) — in that case
/// there is no payload to elide, so this returns `None` and the caller's
/// generic byte-copy loop passes the (harmless, short) sequence through
/// untouched.
fn match_osc_1337_payload(rest: &[u8]) -> Option<PayloadSpan> {
    if !rest.starts_with(OSC_1337_PREFIX) {
        return None;
    }
    let after_prefix = OSC_1337_PREFIX.len();
    let colon = rest[after_prefix..].iter().position(|&b| b == b':')?;
    let payload_start = after_prefix + colon + 1;
    let (terminator_offset, terminator_len) = find_terminator(&rest[payload_start..])?;
    let payload_end = payload_start + terminator_offset;
    Some(PayloadSpan {
        payload_start,
        payload_end,
        end: payload_end + terminator_len,
    })
}

/// `\x1b_G<control-data>;<base64>` + `ST` (Kitty APC is always ST-terminated
/// — see the vte patch's Phase 0 containment fix). No `;` at all (malformed,
/// or a non-graphics APC use that happens to start `G`) means nothing to
/// elide.
fn match_kitty_apc_payload(rest: &[u8]) -> Option<PayloadSpan> {
    if !rest.starts_with(KITTY_APC_PREFIX) {
        return None;
    }
    let after_prefix = KITTY_APC_PREFIX.len();
    let semi = rest[after_prefix..].iter().position(|&b| b == b';')?;
    let payload_start = after_prefix + semi + 1;
    let st_offset = find_bytes(&rest[payload_start..], ST)?;
    let payload_end = payload_start + st_offset;
    Some(PayloadSpan {
        payload_start,
        payload_end,
        end: payload_end + ST.len(),
    })
}

/// OSC sequences accept either terminator; returns `(offset, terminator_len)`.
fn find_terminator(haystack: &[u8]) -> Option<(usize, usize)> {
    let bel = haystack.iter().position(|&b| b == BEL).map(|p| (p, 1));
    let st = find_bytes(haystack, ST).map(|p| (p, ST.len()));
    match (bel, st) {
        (Some(b), Some(s)) => Some(if b.0 <= s.0 { b } else { s }),
        (Some(b), None) => Some(b),
        (None, Some(s)) => Some(s),
        (None, None) => None,
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Append `bytes[base..base+span.end]` to `out` (lazily allocating and
/// backfilling everything copied so far on first use), with the payload
/// region replaced by a short byte-count placeholder.
fn push_elided_span(out: &mut Option<Vec<u8>>, bytes: &[u8], base: usize, span: PayloadSpan) {
    let buf = out.get_or_insert_with(|| bytes[..base].to_vec());
    buf.extend_from_slice(&bytes[base..base + span.payload_start]);
    buf.extend_from_slice(PLACEHOLDER_PREFIX);
    buf.extend_from_slice(
        (span.payload_end - span.payload_start)
            .to_string()
            .as_bytes(),
    );
    buf.extend_from_slice(PLACEHOLDER_SUFFIX);
    buf.extend_from_slice(&bytes[base + span.payload_end..base + span.end]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_ordinary_output_completely_untouched() {
        assert_eq!(elide_image_payloads(b"hello world\r\n"), None);
    }

    #[test]
    fn elides_a_bel_terminated_osc_1337_file_payload() {
        let input = b"before\x1b]1337;File=inline=1:aGVsbG8gd29ybGQ=\x07after".to_vec();
        let out = elide_image_payloads(&input).expect("must elide");
        let out = String::from_utf8(out).unwrap();
        assert_eq!(
            out,
            "before\x1b]1337;File=inline=1:<image 16 bytes elided>\x07after"
        );
    }

    #[test]
    fn elides_an_st_terminated_osc_1337_payload() {
        let input = b"\x1b]1337;File=inline=1:QUJD\x1b\\".to_vec();
        let out = elide_image_payloads(&input).expect("must elide");
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "\x1b]1337;File=inline=1:<image 4 bytes elided>\x1b\\"
        );
    }

    #[test]
    fn elides_a_kitty_apc_payload() {
        let input = b"\x1b_Gi=1,a=T,f=24;aGVsbG8=\x1b\\tail".to_vec();
        let out = elide_image_payloads(&input).expect("must elide");
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "\x1b_Gi=1,a=T,f=24;<image 8 bytes elided>\x1b\\tail"
        );
    }

    #[test]
    fn leaves_a_payload_free_osc_1337_sequence_untouched() {
        // FileEnd carries no `:<base64>` at all.
        let input = b"\x1b]1337;FileEnd\x07".to_vec();
        assert_eq!(elide_image_payloads(&input), None);
    }

    #[test]
    fn leaves_a_payload_free_kitty_apc_sequence_untouched() {
        let input = b"\x1b_Ga=q\x1b\\".to_vec();
        assert_eq!(elide_image_payloads(&input), None);
    }

    #[test]
    fn elides_multiple_sequences_in_one_chunk() {
        let input = b"\x1b]1337;File=inline=1:QUJD\x07mid\x1b_Gi=2,a=T;WFla\x1b\\end".to_vec();
        let out = elide_image_payloads(&input).expect("must elide");
        let out = String::from_utf8(out).unwrap();
        assert_eq!(
            out,
            "\x1b]1337;File=inline=1:<image 4 bytes elided>\x07mid\x1b_Gi=2,a=T;<image 4 bytes elided>\x1b\\end"
        );
    }

    #[test]
    fn an_unterminated_sequence_at_the_end_of_a_chunk_is_left_unelided() {
        // Simulates a payload straddling a PTY read boundary — no
        // terminator has arrived yet in THIS chunk. Passed through as-is;
        // never corrupted, never partially elided.
        let input = b"before\x1b]1337;File=inline=1:aGVsbG8".to_vec();
        assert_eq!(elide_image_payloads(&input), None);
    }

    #[test]
    fn real_tuic_divider_cli_output_is_elided_and_placeholder_length_matches_the_real_payload() {
        // Same real captured bytes used in terminal_grid.rs's end-to-end
        // parser test — proves this module's framing assumptions match the
        // actual wire format a real client produces, not just hand-built
        // test fixtures.
        let hex = "1b5d313333373b46696c653d696e6c696e653d313b77696474683d313030253b6865696768743d313b7072657365727665417370656374526174696f3d303a6147567362473867643239796247513d070a";
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let out = elide_image_payloads(&bytes).expect("must elide");
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("<image 16 bytes elided>"), "got: {out:?}");
        assert!(!out.contains("aGVsbG8"), "base64 payload must be gone");
    }
}
