//! Shared whitespace/case-normalizing diff engine used by both Session Diff
//! Review (the app crate's `session_review.rs` `unified_patch`) and Branch Diff
//! (this crate's `git::get_git_diff_with_options_blocking` /
//! `get_file_diff_with_options_blocking`). Lives in `tuic-git` (Tauri-free) so
//! both consumers share one engine; the app crate re-exports it as
//! `crate::diff_options`.
//!
//! `git diff` itself only has flags for trailing whitespace
//! (`--ignore-space-at-eol`) and whitespace *amount*
//! (`--ignore-space-change`/`-b`) — it has no flag for leading-only whitespace
//! or case-insensitive comparison. Session Diff never shells out to `git`
//! at all (it diffs in-memory transcript reconstructions), so it needs a
//! from-scratch engine regardless. Rather than have the two views behave
//! differently for the options git *does* support, both go through this one
//! engine for every option, always producing identical results.
//!
//! The approach: intern each line under a *normalized* comparison key (so the
//! diff algorithm treats two lines as equal whenever the active options say
//! they're not a "real" change), while a custom printer emits the original,
//! un-normalized text for every line it prints. Only whitespace/case
//! differences the active options ignore disappear from the diff — anything
//! else still shows up, with real content.

use std::borrow::Cow;
use std::fmt;
use std::hash::{Hash, Hasher};

use gix::diff::blob::{
    Algorithm, BasicLineDiffPrinter, InternedInput, Interner, Token, TokenSource,
    UnifiedDiffConfig, UnifiedDiffPrinter, diff_with_slider_heuristics, sources,
};
use serde::{Deserialize, Serialize};

/// Whitespace/case normalization options for a diff comparison. All four are
/// independent and can be combined; `Default` (all `false`) is the ordinary,
/// byte-exact diff every existing caller already gets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffOptions {
    pub ignore_leading_ws: bool,
    pub ignore_trailing_ws: bool,
    pub ignore_ws_amount: bool,
    pub ignore_case: bool,
}

impl DiffOptions {
    /// True iff every option is off — the ordinary byte-exact diff path.
    pub fn is_noop(&self) -> bool {
        !(self.ignore_leading_ws
            || self.ignore_trailing_ws
            || self.ignore_ws_amount
            || self.ignore_case)
    }

    /// Split a line into its content and its line-ending (`"\r\n"`, `"\n"`,
    /// or `""` for a final line with none). The ending is never touched by
    /// normalization — none of these options are about line-ending style.
    fn split_ending(line: &str) -> (&str, &str) {
        if let Some(stripped) = line.strip_suffix("\r\n") {
            (stripped, "\r\n")
        } else if let Some(stripped) = line.strip_suffix('\n') {
            (stripped, "\n")
        } else {
            (line, "")
        }
    }

    /// The comparison key for one line: content normalized per the active
    /// options, with its original line-ending reattached unchanged. Two lines
    /// with equal keys are treated as unchanged by the diff algorithm; the
    /// printer still emits each line's real, un-normalized text.
    ///
    /// `ignore_ws_amount` collapses runs of whitespace to a single space via
    /// `split_whitespace`, which — as a side effect — also drops leading and
    /// trailing runs entirely (there's nothing meaningful to "collapse them
    /// to" at the edges). That's an intentional, reasonable reading of
    /// "collapse runs of whitespace," not a bug: turning it on alone already
    /// behaves like leading+trailing trimming too.
    fn normalize_key<'a>(&self, line: &'a str) -> Cow<'a, str> {
        if self.is_noop() {
            return Cow::Borrowed(line);
        }
        let (content, ending) = Self::split_ending(line);

        let after_leading = if self.ignore_leading_ws {
            content.trim_start()
        } else {
            content
        };
        let after_trailing = if self.ignore_trailing_ws {
            after_leading.trim_end()
        } else {
            after_leading
        };

        let collapsed: Cow<str> = if self.ignore_ws_amount {
            Cow::Owned(
                after_trailing
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        } else {
            Cow::Borrowed(after_trailing)
        };

        let cased: Cow<str> = if self.ignore_case {
            Cow::Owned(collapsed.to_lowercase())
        } else {
            collapsed
        };

        if ending.is_empty() {
            cased
        } else {
            Cow::Owned(format!("{cased}{ending}"))
        }
    }
}

/// One line: its real text (what gets printed) plus its normalized
/// comparison key (what decides diff-algorithm equality). `Hash`/`Eq` compare
/// only the key, so the interner deduplicates lines that are "the same" under
/// the active options even when their original bytes differ.
#[derive(Clone)]
struct NormLine<'a> {
    original: &'a str,
    key: Cow<'a, str>,
}

impl PartialEq for NormLine<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl Eq for NormLine<'_> {}
impl Hash for NormLine<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}
/// Required by `diff_with_slider_heuristics`'s `T: AsRef<[u8]>` bound (the
/// slider heuristics inspect surrounding line bytes for indentation). Uses
/// the *original* bytes — the heuristics should see real content, not the
/// normalized key.
impl AsRef<[u8]> for NormLine<'_> {
    fn as_ref(&self) -> &[u8] {
        self.original.as_bytes()
    }
}

/// A `TokenSource` that tokenizes a string into [`NormLine`]s the same way
/// `&str`'s own default `TokenSource` impl does (one token per line,
/// newline included) — but under `opts`'s normalization key.
struct NormSource<'a> {
    data: &'a str,
    opts: DiffOptions,
}

struct NormLines<'a> {
    inner: sources::Lines<'a>,
    opts: DiffOptions,
}

impl<'a> Iterator for NormLines<'a> {
    type Item = NormLine<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(move |line| NormLine {
            original: line,
            key: self.opts.normalize_key(line),
        })
    }
}

impl<'a> TokenSource for NormSource<'a> {
    type Token = NormLine<'a>;
    type Tokenizer = NormLines<'a>;

    fn tokenize(&self) -> Self::Tokenizer {
        NormLines {
            inner: sources::lines(self.data),
            opts: self.opts,
        }
    }

    fn estimate_tokens(&self) -> u32 {
        sources::lines(self.data).estimate_tokens()
    }
}

/// Prints hunks using each token's *original* text, never its normalized
/// key — the counterpart to `BasicLineDiffPrinter`, which can't be reused
/// here because it expects the interner's token type to itself be a
/// reference (`Interner<&'a T>`), not our owned `NormLine`.
struct OriginalTextPrinter<'a>(&'a Interner<NormLine<'a>>);

impl UnifiedDiffPrinter for OriginalTextPrinter<'_> {
    fn display_header(
        &self,
        mut f: impl fmt::Write,
        start_before: u32,
        start_after: u32,
        len_before: u32,
        len_after: u32,
    ) -> fmt::Result {
        writeln!(
            f,
            "@@ -{},{} +{},{} @@",
            start_before + 1,
            len_before,
            start_after + 1,
            len_after
        )
    }

    fn display_context_token(&self, mut f: impl fmt::Write, token: Token) -> fmt::Result {
        let line = self.0[token].original;
        write!(f, " {line}")?;
        if !line.ends_with('\n') {
            writeln!(f)?;
        }
        Ok(())
    }

    fn display_hunk(
        &self,
        mut f: impl fmt::Write,
        before: &[Token],
        after: &[Token],
    ) -> fmt::Result {
        if let Some(&last) = before.last() {
            for &t in before {
                write!(f, "-{}", self.0[t].original)?;
            }
            if !self.0[last].original.ends_with('\n') {
                writeln!(f)?;
            }
        }
        if let Some(&last) = after.last() {
            for &t in after {
                write!(f, "+{}", self.0[t].original)?;
            }
            if !self.0[last].original.ends_with('\n') {
                writeln!(f)?;
            }
        }
        Ok(())
    }
}

/// Compute a unified-diff *body* (hunks only — no `diff --git`/`---`/`+++`
/// headers, matching what callers already build themselves) between `old`
/// and `new`, honoring `opts`. Returns `(body, additions, deletions)`; an
/// empty body with `(0, 0)` means no difference under the active options.
///
/// When `opts.is_noop()`, this takes the exact same path the pre-existing
/// callers already used (plain `InternedInput::new(old, new)` +
/// `BasicLineDiffPrinter`) — byte-for-byte identical output, not merely
/// equivalent, so the default (overwhelmingly common) case never changes
/// behavior or slows down.
pub fn unified_diff(old: &str, new: &str, ctx: u32, opts: DiffOptions) -> (String, u32, u32) {
    if old == new {
        return (String::new(), 0, 0);
    }

    let mut config = UnifiedDiffConfig::default();
    config.context_len(ctx);

    if opts.is_noop() {
        let input = InternedInput::new(old, new);
        let diff = diff_with_slider_heuristics(Algorithm::Histogram, &input);
        let additions = diff.count_additions();
        let deletions = diff.count_removals();
        if additions == 0 && deletions == 0 {
            return (String::new(), 0, 0);
        }
        let printer = BasicLineDiffPrinter(&input.interner);
        let body = diff.unified_diff(&printer, config, &input).to_string();
        return (body, additions, deletions);
    }

    let input = InternedInput::new(
        NormSource { data: old, opts },
        NormSource { data: new, opts },
    );
    let diff = diff_with_slider_heuristics(Algorithm::Histogram, &input);
    let additions = diff.count_additions();
    let deletions = diff.count_removals();
    if additions == 0 && deletions == 0 {
        return (String::new(), 0, 0);
    }
    let printer = OriginalTextPrinter(&input.interner);
    let body = diff.unified_diff(&printer, config, &input).to_string();
    (body, additions, deletions)
}

/// NUL-byte heuristic shared by every caller that needs to decide whether
/// content read off disk/git is text before handing it to [`unified_diff`].
/// Mirrors `session_review.rs`'s private `is_binary_bytes` (kept separate
/// there to avoid a cross-module dependency for one already-tested helper);
/// this copy is for new callers (`git.rs`) that have no equivalent yet.
pub fn is_binary(data: &[u8]) -> bool {
    data.iter().take(8000).any(|&b| b == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(leading: bool, trailing: bool, amount: bool, case: bool) -> DiffOptions {
        DiffOptions {
            ignore_leading_ws: leading,
            ignore_trailing_ws: trailing,
            ignore_ws_amount: amount,
            ignore_case: case,
        }
    }

    #[test]
    fn is_noop_true_only_when_every_option_is_false() {
        assert!(DiffOptions::default().is_noop());
        assert!(!opts(true, false, false, false).is_noop());
        assert!(!opts(false, true, false, false).is_noop());
        assert!(!opts(false, false, true, false).is_noop());
        assert!(!opts(false, false, false, true).is_noop());
    }

    #[test]
    fn noop_path_is_byte_identical_to_the_direct_gix_diff() {
        let old = "line one\nline two\nline three\n";
        let new = "line one\nLINE TWO\nline three\n";

        let (via_engine, add1, del1) = unified_diff(old, new, 3, DiffOptions::default());

        let input = InternedInput::new(old, new);
        let diff = diff_with_slider_heuristics(Algorithm::Histogram, &input);
        let add2 = diff.count_additions();
        let del2 = diff.count_removals();
        let mut config = UnifiedDiffConfig::default();
        config.context_len(3);
        let printer = BasicLineDiffPrinter(&input.interner);
        let direct = diff.unified_diff(&printer, config, &input).to_string();

        assert_eq!(via_engine, direct);
        assert_eq!((add1, del1), (add2, del2));
    }

    #[test]
    fn identical_strings_produce_no_diff_regardless_of_options() {
        let text = "same\ntext\n";
        assert_eq!(
            unified_diff(text, text, 3, opts(true, true, true, true)),
            (String::new(), 0, 0)
        );
    }

    #[test]
    fn ignore_leading_ws_hides_an_indentation_only_change() {
        let old = "  foo\nbar\n";
        let new = "foo\nbar\n";
        let (body, add, del) = unified_diff(old, new, 3, opts(true, false, false, false));
        assert_eq!((body, add, del), (String::new(), 0, 0));

        // Without the option, the same two strings DO differ.
        let (body2, add2, del2) = unified_diff(old, new, 3, DiffOptions::default());
        assert!(!body2.is_empty());
        assert_eq!((add2, del2), (1, 1));
    }

    #[test]
    fn ignore_leading_ws_does_not_hide_a_trailing_only_change() {
        let old = "foo  \nbar\n";
        let new = "foo\nbar\n";
        let (_, add, del) = unified_diff(old, new, 3, opts(true, false, false, false));
        assert_eq!((add, del), (1, 1));
    }

    #[test]
    fn ignore_trailing_ws_hides_a_trailing_only_change() {
        let old = "foo  \nbar\n";
        let new = "foo\nbar\n";
        let (body, add, del) = unified_diff(old, new, 3, opts(false, true, false, false));
        assert_eq!((body, add, del), (String::new(), 0, 0));
    }

    #[test]
    fn ignore_ws_amount_hides_a_run_length_only_change() {
        let old = "foo    bar\n";
        let new = "foo bar\n";
        let (body, add, del) = unified_diff(old, new, 3, opts(false, false, true, false));
        assert_eq!((body, add, del), (String::new(), 0, 0));
    }

    #[test]
    fn ignore_ws_amount_does_not_hide_a_real_word_change() {
        let old = "foo    bar\n";
        let new = "foo baz\n";
        let (_, add, del) = unified_diff(old, new, 3, opts(false, false, true, false));
        assert_eq!((add, del), (1, 1));
    }

    #[test]
    fn ignore_case_hides_a_case_only_change() {
        let old = "Hello World\n";
        let new = "hello world\n";
        let (body, add, del) = unified_diff(old, new, 3, opts(false, false, false, true));
        assert_eq!((body, add, del), (String::new(), 0, 0));
    }

    #[test]
    fn combined_options_hide_a_leading_trailing_case_change_together() {
        let old = "  Hello World  \n";
        let new = "hello world\n";
        let (body, add, del) = unified_diff(old, new, 3, opts(true, true, false, true));
        assert_eq!((body, add, del), (String::new(), 0, 0));
    }

    #[test]
    fn printed_hunk_shows_original_text_not_the_normalized_key() {
        // "  FOO" vs "  bar" (both indented, both differ from a real neighbor
        // line) — with leading-ws+case ignored, the *unchanged* neighbor
        // lines must print their real (indented, mixed-case) text.
        let old = "  Same\nold line\n";
        let new = "  same\nnew line\n";
        let (body, add, del) = unified_diff(old, new, 3, opts(true, false, false, true));
        assert_eq!((add, del), (1, 1));
        assert!(
            body.contains(" Same\n") || body.contains("  Same\n"),
            "context line should print original text: {body}"
        );
        assert!(body.contains("-old line\n"));
        assert!(body.contains("+new line\n"));
    }

    #[test]
    fn a_change_the_active_options_do_not_ignore_still_shows_up() {
        let old = "foo\nreal change one\n";
        let new = "FOO\nreal change two\n";
        // Case is ignored, so line 1 matches; line 2's content genuinely
        // differs even after normalization, so it must still show.
        let (body, add, del) = unified_diff(old, new, 3, opts(false, false, false, true));
        assert_eq!((add, del), (1, 1));
        assert!(body.contains("-real change one\n"));
        assert!(body.contains("+real change two\n"));
    }

    #[test]
    fn is_binary_detects_a_nul_byte() {
        assert!(is_binary(b"abc\0def"));
        assert!(!is_binary(b"abc def"));
    }
}
