//! `KeyFace`: a hashable description of what one key should show.
//!
//! This is the coalescing boundary the whole render pipeline exists to
//! serve. `render(face) -> Arc<[u8]>` is a pure function of `KeyFace`, and
//! the coordinator only ever re-renders/re-transfers a slot when its face
//! actually changes — so `KeyFace` must exclude anything that changes every
//! tick (raw timestamps) and bucket anything continuous (elapsed time,
//! animation phase) into a small number of discrete values. This mirrors
//! `SessionState`'s own `PartialEq` impl in the main crate excluding
//! `last_activity_ms` for exactly the same reason: a field that always
//! differs turns a dedup check into a no-op.

use crate::render::palette::FaceState;

/// The small state glyph drawn in the top-left corner.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Glyph {
    None,
    Dot,
    /// One of 4 discrete phase buckets of the needs-input pulse (see
    /// `palette::FaceState::pulses`). Never a raw float — that would defeat
    /// the whole point of hashing this struct.
    Pulse(u8),
    Question,
    Bang,
}

/// A short label, ASCII-only and length-capped by the caller (the
/// coordinator truncates before constructing a `KeyFace`, not this type) —
/// kept as a fixed small inline buffer rather than `String` so a `KeyFace` is
/// cheap to hash and clone at 4Hz.
pub type Label = arrayvec_lite::CompactAscii;

/// Ready-to-render content for one 64x64 key. Two levels of dedup key off
/// this: the coordinator's `last_pushed[slot]` (skip even a cache lookup)
/// and the LRU cache inside `RenderPipeline` (skip re-encoding a face seen
/// before, e.g. a session flipping between two states repeatedly).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyFace {
    pub state: FaceState,
    pub glyph: Glyph,
    pub primary: Label,
    pub secondary: Label,
    /// Overflow/unread count badge, top-right corner. `None` = no badge.
    pub badge: Option<u8>,
}

impl KeyFace {
    pub fn empty() -> Self {
        Self {
            state: FaceState::Empty,
            glyph: Glyph::None,
            primary: Label::new(),
            secondary: Label::new(),
            badge: None,
        }
    }
}

/// A tiny fixed-capacity ASCII string, so `KeyFace` stays `Copy`-cheap-ish
/// (no heap alloc) and trivially `Hash`/`Eq`. Capacity 12 covers the
/// design's own budget (≤8 char primary, ≤11 char secondary; the 12th slot
/// is a one-character margin, not a promise of a 12th visible glyph).
pub mod arrayvec_lite {
    use std::fmt;

    const CAP: usize = 12;

    #[derive(Copy, Clone, PartialEq, Eq, Hash)]
    pub struct CompactAscii {
        len: u8,
        bytes: [u8; CAP],
    }

    impl CompactAscii {
        pub fn new() -> Self {
            Self {
                len: 0,
                bytes: [0; CAP],
            }
        }

        /// Truncates silently at `CAP` bytes and replaces any non-ASCII
        /// byte with `?` — this type exists specifically to make a
        /// `KeyFace` cheap and total, not to do full Unicode-aware
        /// wrapping. Session names/aliases are expected to be ASCII
        /// already (`term_aliases` in the main crate assigns short
        /// machine-generated ids like `tc-1`).
        pub fn from_str_truncated(s: &str) -> Self {
            let mut bytes = [0u8; CAP];
            let mut len = 0usize;
            for b in s.bytes() {
                if len >= CAP {
                    break;
                }
                bytes[len] = if b.is_ascii_graphic() || b == b' ' {
                    b
                } else {
                    b'?'
                };
                len += 1;
            }
            Self {
                len: len as u8,
                bytes,
            }
        }

        pub fn as_str(&self) -> &str {
            // Safety net rather than `unsafe`: every byte written by
            // `from_str_truncated` is guaranteed ASCII, so this can never
            // actually hit the lossy path, but using `from_utf8` (not
            // `_unchecked`) keeps that a guarantee enforced by the type,
            // not by careful callers.
            std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
        }

        pub fn is_empty(&self) -> bool {
            self.len == 0
        }
    }

    impl Default for CompactAscii {
        fn default() -> Self {
            Self::new()
        }
    }

    impl fmt::Debug for CompactAscii {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{:?}", self.as_str())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_face_is_stably_hashable() {
        use std::collections::HashSet;
        let a = KeyFace {
            state: FaceState::Working,
            glyph: Glyph::Dot,
            primary: Label::from_str_truncated("tc-7"),
            secondary: Label::from_str_truncated("refactor"),
            badge: None,
        };
        let b = a.clone();
        let mut set = HashSet::new();
        set.insert(a);
        assert!(
            set.contains(&b),
            "two equal KeyFaces must hash and compare equal"
        );
    }

    #[test]
    fn compact_ascii_truncates_and_sanitizes() {
        let too_long = Label::from_str_truncated("way-too-long-for-a-key-label");
        assert_eq!(too_long.as_str().len(), 12);

        let with_emoji = Label::from_str_truncated("hi\u{1F600}");
        // Multi-byte UTF-8 bytes are each individually non-ASCII-graphic,
        // so each becomes its own '?' rather than corrupting the buffer.
        assert!(with_emoji.as_str().starts_with("hi"));
    }

    #[test]
    fn empty_face_is_distinct_from_any_occupied_face() {
        let empty = KeyFace::empty();
        let occupied = KeyFace {
            state: FaceState::Idle,
            glyph: Glyph::None,
            primary: Label::new(),
            secondary: Label::new(),
            badge: None,
        };
        assert_ne!(empty, occupied);
    }
}
