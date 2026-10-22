//! Keeps a `tuic-hook` OSC 7770 from landing in the middle of a UTF-8 character.
//!
//! Two processes write to one PTY: the agent (Claude Code) and the hook it
//! spawns, which writes `ESC ] 7770 ; verb=payload ESC \` straight to the tty.
//! The kernel can cut a large agent write at any byte, and the hook's own
//! write slips into the gap. Seen live (2026-10-22, "Claude Code" tab of an
//! srtmon session, no resize involved): the bottom separator row
//! `───…` was cut after the first byte of a `─` (`E2 94 80`), four OSC 7770
//! sequences followed, then `94 80` and the rest of the row.
//!
//! [`crate::state::Utf8ReadBuffer`] turns bytes into a `String` before the
//! grid parser ever sees them, replacing every invalid sequence with U+FFFD:
//! `E2 ESC` became one replacement and the two orphan continuation bytes became
//! two more. One cell became three, the 146-column row overflowed by two, the
//! overflow wrapped, and every row below slid down by one. Claude redraws with
//! relative cursor moves, so each later repaint then landed one row off.
//!
//! This guard runs in front of that conversion. When a complete OSC 7770 arrives
//! while a character is still owed continuation bytes, the OSC is held until the
//! character is finished and released right after it, so the converter sees the
//! character contiguous. The OSC is metadata (state, cwd, transcript), so
//! applying it a few bytes later changes nothing observable.
//!
//! Only OSC 7770 is reordered. It is the only sequence TUIC itself injects into
//! a PTY it does not own; reordering other sequences would silently rewrite what
//! an arbitrary program wrote.

use std::borrow::Cow;

const OSC_PREFIX: &[u8] = b"\x1b]7770;";

/// Total bytes held across back-to-back sequences. A typical hook write is a few
/// hundred bytes, but the ceiling is far higher: `tuic-hook` cuts each free-text
/// payload at 512 bytes *before* percent-encoding, which can triple it to 1536,
/// and one call can emit ten such verbs (`ccsession`, `cwd`, `transcript`,
/// `cctitle`, `tool`, `notify`, `notifytype`, `bgtasks`, `bgtasksummary`,
/// `ccend`), about 15.6 KB. Past this the bytes are not a hook write, so they are
/// released in their original order.
const MAX_HELD_BYTES: usize = 16 * 1024;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Bytes pass straight through.
    #[default]
    Pass,
    /// Inside a held OSC 7770, looking for its terminator.
    Collect,
    /// A held OSC is complete; waiting for the character it interrupted to finish.
    Await,
}

#[derive(Debug, Default)]
pub(crate) struct OscInterleaveGuard {
    /// Continuation bytes the last character forwarded to the converter still owes.
    owed: u8,
    phase: Phase,
    /// One or more complete OSC 7770 sequences (plus the one being collected).
    held: Vec<u8>,
    /// The last held byte was ESC, so the next one decides whether it is `ESC \`.
    esc_pending: bool,
    /// A suffix of a read that may be the start of an OSC 7770 prefix (at most
    /// `OSC_PREFIX.len() - 1` bytes). Kept back so a read boundary inside the
    /// prefix does not hide the sequence; prepended to the next read.
    pending_prefix: Vec<u8>,
}

impl OscInterleaveGuard {
    /// Reorder `data` as described in the module docs. Borrows `data` unchanged
    /// on the overwhelmingly common path (no OSC 7770 anywhere near an
    /// incomplete character).
    pub(crate) fn filter<'a>(&mut self, data: &'a [u8]) -> Cow<'a, [u8]> {
        if self.phase == Phase::Pass
            && self.pending_prefix.is_empty()
            && self.passes_untouched(data)
        {
            self.owed = owed_after(self.owed, data);
            return Cow::Borrowed(data);
        }
        let joined;
        let data = if self.pending_prefix.is_empty() {
            data
        } else {
            let mut v = std::mem::take(&mut self.pending_prefix);
            v.extend_from_slice(data);
            joined = v;
            &joined[..]
        };
        let mut out = Vec::with_capacity(data.len() + self.held.len());
        self.run(data, &mut out);
        Cow::Owned(out)
    }

    /// Everything still held, in the order it arrived. For EOF, where no
    /// continuation bytes are coming.
    pub(crate) fn take_held(&mut self) -> Vec<u8> {
        let mut bytes = std::mem::take(&mut self.held);
        bytes.extend_from_slice(&std::mem::take(&mut self.pending_prefix));
        self.reset();
        bytes
    }

    fn reset(&mut self) {
        self.owed = 0;
        self.phase = Phase::Pass;
        self.esc_pending = false;
    }

    /// True when `data` needs no reordering: it holds no OSC 7770, and does not
    /// end in what could be the start of one right after an incomplete character.
    fn passes_untouched(&self, data: &[u8]) -> bool {
        if find_prefix(data).is_some() {
            return false;
        }
        let partial = partial_prefix_len(data);
        partial == 0 || owed_after(self.owed, &data[..data.len() - partial]) == 0
    }

    fn forward(&mut self, out: &mut Vec<u8>, bytes: &[u8]) {
        out.extend_from_slice(bytes);
        self.owed = owed_after(self.owed, bytes);
    }

    /// Give up on reordering: emit what is held in its original order.
    fn release(&mut self, out: &mut Vec<u8>) {
        out.append(&mut self.held);
        self.reset();
    }

    fn run(&mut self, data: &[u8], out: &mut Vec<u8>) {
        let mut i = 0;
        while i < data.len() {
            match self.phase {
                Phase::Pass => {
                    let rest = &data[i..];
                    let Some(at) = find_prefix(rest) else {
                        // No OSC in sight. A tail that might be the start of one,
                        // right after an incomplete character, waits for the next read.
                        let partial = partial_prefix_len(rest);
                        let head = &rest[..rest.len() - partial];
                        if partial > 0 && owed_after(self.owed, head) > 0 {
                            self.forward(out, head);
                            self.pending_prefix
                                .extend_from_slice(&rest[rest.len() - partial..]);
                        } else {
                            self.forward(out, rest);
                        }
                        return;
                    };
                    self.forward(out, &rest[..at]);
                    i += at + OSC_PREFIX.len();
                    if self.owed > 0 {
                        self.held.extend_from_slice(OSC_PREFIX);
                        self.phase = Phase::Collect;
                    } else {
                        // Nothing is half-written: the OSC goes through as it is.
                        self.forward(out, OSC_PREFIX);
                    }
                }
                Phase::Collect => {
                    let b = data[i];
                    if self.esc_pending {
                        self.esc_pending = false;
                        if b == b'\\' {
                            self.held.push(b);
                            self.phase = Phase::Await;
                            i += 1;
                        } else {
                            // `ESC` + something else is not a terminator: not ours.
                            self.release(out);
                        }
                    } else if self.held.len() >= MAX_HELD_BYTES {
                        self.release(out);
                    } else {
                        self.held.push(b);
                        i += 1;
                        match b {
                            0x07 => self.phase = Phase::Await,
                            0x1b => self.esc_pending = true,
                            _ => {}
                        }
                    }
                }
                Phase::Await => {
                    let rest = &data[i..];
                    if rest.starts_with(OSC_PREFIX) {
                        // The hook sends several sequences back to back.
                        self.held.extend_from_slice(OSC_PREFIX);
                        self.phase = Phase::Collect;
                        i += OSC_PREFIX.len();
                    } else if self.owed > 0 && is_continuation(rest[0]) {
                        out.push(rest[0]);
                        i += 1;
                        self.owed -= 1;
                        if self.owed == 0 {
                            out.append(&mut self.held);
                            self.reset();
                        }
                    } else if rest.len() < OSC_PREFIX.len() && OSC_PREFIX.starts_with(rest) {
                        // Maybe another hook sequence, cut by this read boundary.
                        self.pending_prefix.extend_from_slice(rest);
                        return;
                    } else {
                        // Not a continuation byte: the character was truncated for real.
                        self.release(out);
                    }
                }
            }
        }
    }
}

fn is_continuation(b: u8) -> bool {
    (0x80..=0xBF).contains(&b)
}

fn find_prefix(data: &[u8]) -> Option<usize> {
    let mut from = 0;
    while let Some(at) = data[from..].iter().position(|&b| b == 0x1b) {
        let at = from + at;
        if data[at..].starts_with(OSC_PREFIX) {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

/// Length of the longest proper prefix of `OSC_PREFIX` that `data` ends with.
fn partial_prefix_len(data: &[u8]) -> usize {
    (1..OSC_PREFIX.len())
        .rev()
        .find(|&m| data.len() >= m && data[data.len() - m..] == OSC_PREFIX[..m])
        .unwrap_or(0)
}

/// Continuation bytes still owed once `bytes` have been appended to a stream that
/// owed `owed` already.
fn owed_after(owed: u8, bytes: &[u8]) -> u8 {
    let mut owed = owed;
    let mut idx = 0;
    while owed > 0 && idx < bytes.len() && is_continuation(bytes[idx]) {
        owed -= 1;
        idx += 1;
    }
    if owed > 0 {
        // Either every byte was a continuation (still owed), or a non-continuation
        // ended the character early (the converter will emit one U+FFFD for it).
        return if idx == bytes.len() { owed } else { 0 };
    }
    tail_owed(&bytes[idx..])
}

/// Continuation bytes owed by an incomplete character at the very end of `bytes`.
fn tail_owed(bytes: &[u8]) -> u8 {
    for k in 1..=bytes.len().min(3) {
        let b = bytes[bytes.len() - k];
        if is_continuation(b) {
            continue;
        }
        let need = match b {
            0xC2..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF4 => 4,
            _ => return 0,
        };
        return if need > k { (need - k) as u8 } else { 0 };
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Utf8ReadBuffer;

    const BAR: &str = "─";
    const OSC_BUSY: &[u8] = b"\x1b]7770;state=busy\x1b\\";
    const OSC_CWD: &[u8] = b"\x1b]7770;cwd=%2Ftmp\x1b\\";

    /// The stream seen live: a row of `─` cut after the first byte of one, the
    /// hook's sequences, the rest of the character, then more row.
    fn interleaved_stream(bars_before: usize, bars_after: usize) -> Vec<u8> {
        let bar = BAR.as_bytes();
        let mut s = bar.repeat(bars_before);
        s.push(bar[0]);
        s.extend_from_slice(OSC_CWD);
        s.extend_from_slice(OSC_BUSY);
        s.extend_from_slice(&bar[1..]);
        s.extend_from_slice(&bar.repeat(bars_after));
        s.extend_from_slice(b"\r\nB");
        s
    }

    /// What the parser must see: the character whole, the OSCs right after it.
    fn expected_text(bars_before: usize, bars_after: usize) -> String {
        let mut s = BAR.repeat(bars_before + 1);
        s.push_str(std::str::from_utf8(OSC_CWD).unwrap());
        s.push_str(std::str::from_utf8(OSC_BUSY).unwrap());
        s.push_str(&BAR.repeat(bars_after));
        s.push_str("\r\nB");
        s
    }

    fn run_chunks(chunks: &[&[u8]]) -> String {
        let mut buf = Utf8ReadBuffer::new();
        let mut text = String::new();
        for c in chunks {
            text.push_str(&buf.push(c));
        }
        text.push_str(&buf.flush());
        text
    }

    #[test]
    fn an_osc_inside_a_character_in_one_read_is_moved_after_it() {
        let stream = interleaved_stream(5, 5);
        assert_eq!(run_chunks(&[&stream]), expected_text(5, 5));
    }

    #[test]
    fn the_live_split_a_read_ending_on_the_first_byte_is_repaired() {
        let stream = interleaved_stream(39, 106);
        let cut = 39 * 3 + 1; // read 1 ends on the lead byte, as in the capture
        let text = run_chunks(&[&stream[..cut], &stream[cut..]]);
        assert_eq!(text, expected_text(39, 106));
        assert!(!text.contains('\u{FFFD}'));
    }

    #[test]
    fn every_two_way_read_split_gives_the_same_text() {
        let stream = interleaved_stream(3, 3);
        let want = expected_text(3, 3);
        for cut in 0..=stream.len() {
            let got = run_chunks(&[&stream[..cut], &stream[cut..]]);
            assert_eq!(got, want, "split at byte {cut}");
        }
    }

    #[test]
    fn every_three_way_read_split_gives_the_same_text() {
        let stream = interleaved_stream(2, 2);
        let want = expected_text(2, 2);
        for a in 0..=stream.len() {
            for b in a..=stream.len() {
                let got = run_chunks(&[&stream[..a], &stream[a..b], &stream[b..]]);
                assert_eq!(got, want, "splits at bytes {a} and {b}");
            }
        }
    }

    #[test]
    fn a_four_byte_character_cut_after_two_bytes_is_repaired() {
        let crab = "🦀".as_bytes();
        let mut s = b"x".to_vec();
        s.extend_from_slice(&crab[..2]);
        s.extend_from_slice(OSC_BUSY);
        s.extend_from_slice(&crab[2..]);
        s.extend_from_slice(b"y");
        assert_eq!(
            run_chunks(&[&s]),
            format!("x🦀{}y", std::str::from_utf8(OSC_BUSY).unwrap())
        );
    }

    #[test]
    fn a_bel_terminated_osc_is_reordered_too() {
        let bar = BAR.as_bytes();
        let mut s = vec![bar[0]];
        s.extend_from_slice(b"\x1b]7770;state=idle\x07");
        s.extend_from_slice(&bar[1..]);
        assert_eq!(run_chunks(&[&s]), format!("{BAR}\x1b]7770;state=idle\x07"));
    }

    #[test]
    fn an_osc_between_whole_characters_passes_through_unchanged() {
        let mut guard = OscInterleaveGuard::default();
        let mut s = BAR.as_bytes().to_vec();
        s.extend_from_slice(OSC_BUSY);
        s.extend_from_slice(BAR.as_bytes());
        assert_eq!(&*guard.filter(&s), &s[..]);
    }

    #[test]
    fn plain_text_is_borrowed_not_copied() {
        let mut guard = OscInterleaveGuard::default();
        let out = guard.filter(b"hello \x1b[31mworld\x1b[0m");
        assert!(matches!(out, Cow::Borrowed(_)));
    }

    #[test]
    fn another_sequence_after_a_truncated_character_is_not_reordered() {
        // A CSI is not ours to move: the original order (and its U+FFFD) stays.
        let mut s = vec![BAR.as_bytes()[0]];
        s.extend_from_slice(b"\x1b[31mx");
        let mut guard = OscInterleaveGuard::default();
        assert_eq!(&*guard.filter(&s), &s[..]);
    }

    #[test]
    fn a_character_that_never_finishes_releases_the_osc_in_original_order() {
        let mut s = vec![BAR.as_bytes()[0]];
        s.extend_from_slice(OSC_BUSY);
        s.extend_from_slice(b"abc");
        let mut guard = OscInterleaveGuard::default();
        assert_eq!(&*guard.filter(&s), &s[..]);
        assert!(guard.take_held().is_empty());
    }

    /// The largest write `tuic-hook` can make: ten free-text verbs, each payload at
    /// its 512-byte cut and fully percent-encoded (three bytes per byte).
    fn maximal_hook_write() -> Vec<u8> {
        let verbs = [
            "ccsession",
            "cwd",
            "transcript",
            "cctitle",
            "tool",
            "notify",
            "notifytype",
            "bgtasks",
            "bgtasksummary",
            "ccend",
        ];
        let mut w = Vec::new();
        for verb in verbs {
            w.extend_from_slice(format!("\x1b]7770;{verb}=").as_bytes());
            w.extend_from_slice("%00".repeat(512).as_bytes());
            w.extend_from_slice(b"\x1b\\");
        }
        w
    }

    #[test]
    fn the_largest_possible_hook_write_still_fits_under_the_cap() {
        let write = maximal_hook_write();
        assert!(
            write.len() < MAX_HELD_BYTES,
            "a maximal hook write is {} bytes; the cap is {MAX_HELD_BYTES}",
            write.len()
        );
    }

    #[test]
    fn a_maximal_hook_write_inside_a_character_is_reordered_not_released() {
        let bar = BAR.as_bytes();
        let hook = maximal_hook_write();
        let mut stream = vec![bar[0]];
        stream.extend_from_slice(&hook);
        stream.extend_from_slice(&bar[1..]);
        stream.extend_from_slice(b"z");

        let mut want = BAR.to_string();
        want.push_str(std::str::from_utf8(&hook).unwrap());
        want.push('z');

        assert_eq!(run_chunks(&[&stream]), want);
        // The same write arriving over several reads, as a 64 KB read buffer can split it.
        let (a, b) = (1 + 700, 1 + 9000);
        assert_eq!(
            run_chunks(&[&stream[..a], &stream[a..b], &stream[b..]]),
            want
        );
    }

    #[test]
    fn an_unterminated_osc_is_released_once_it_exceeds_the_cap() {
        let mut s = vec![BAR.as_bytes()[0]];
        s.extend_from_slice(OSC_PREFIX);
        s.extend(std::iter::repeat_n(b'a', MAX_HELD_BYTES + 10));
        let mut guard = OscInterleaveGuard::default();
        let out = guard.filter(&s).into_owned();
        assert_eq!(out, s, "nothing is lost, and the order is the original one");
        assert!(guard.held.len() <= MAX_HELD_BYTES);
    }

    #[test]
    fn eof_releases_a_held_osc() {
        let mut s = vec![BAR.as_bytes()[0]];
        s.extend_from_slice(OSC_BUSY);
        let mut buf = Utf8ReadBuffer::new();
        let first = buf.push(&s);
        assert!(!first.contains("7770"), "held while the character is open");
        let flushed = buf.flush();
        assert!(
            flushed.contains("7770;state=busy"),
            "released at EOF: {flushed:?}"
        );
    }

    #[test]
    fn several_hooks_back_to_back_all_move_together() {
        let bar = BAR.as_bytes();
        let mut s = vec![bar[0]];
        for _ in 0..4 {
            s.extend_from_slice(OSC_BUSY);
        }
        s.extend_from_slice(&bar[1..]);
        let want = format!("{BAR}{}", std::str::from_utf8(&OSC_BUSY.repeat(4)).unwrap());
        assert_eq!(run_chunks(&[&s]), want);
    }

    /// End to end through the grid: the row that wrapped in the capture stays one row.
    #[test]
    fn the_grid_does_not_wrap_a_row_a_hook_interrupted() {
        let cols: u16 = 30;
        let stream = {
            let mut s = b"A\r\n".to_vec();
            s.extend_from_slice(&interleaved_stream(29, 0));
            s.extend_from_slice(b"\r\nC");
            s
        };
        let cut = 3 + 29 * 3 + 1;

        let mut buf = Utf8ReadBuffer::new();
        let mut grid = crate::terminal_grid::TerminalGrid::new(8, cols, 100);
        for chunk in [&stream[..cut], &stream[cut..]] {
            grid.process(buf.push(chunk).as_bytes());
        }
        let rows = grid.screen_text_rows();
        assert_eq!(rows[0], "A");
        assert_eq!(rows[1], BAR.repeat(30), "one full row, no overflow");
        assert_eq!(rows[2], "B", "no row slid in below it");
        assert_eq!(rows[3], "C");
    }
}
