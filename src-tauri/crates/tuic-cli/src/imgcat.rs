//! Clean-room `imgcat`/`imgls`/`divider` — color-tools plan, Phase 4.
//!
//! iTerm2's own scripts of the same name are GPLv2; this repo is
//! Apache-2.0, so they can't be vendored. The wire protocol itself (OSC
//! 1337) isn't copyrightable, so these are independent implementations of
//! the same *observable behavior*, verified against the real scripts' text
//! (fetched and read, not guessed) rather than their source code.
//!
//! None of these talk to a running TUICommander instance over IPC — like
//! the real scripts, they just print an escape sequence to stdout. They
//! work in any terminal that understands the protocol, not only ours.
//!
//! Pure sequence-building logic lives here, unit-testable without any file
//! IO; `main.rs`'s command handlers do the file reading and printing.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as Base64;

/// Whether `$TERM` calls for iTerm2's tmux/screen DCS-passthrough wrapping.
/// Matches real `imgcat`'s own check (`screen*`/`tmux*`), needed because a
/// bare OSC 1337 sequence sent to the *outer* terminal through a
/// tmux/screen multiplexer would otherwise be swallowed as pane-title-set
/// noise rather than reaching the real terminal underneath.
pub fn needs_tmux_passthrough(term: &str) -> bool {
    term.starts_with("screen") || term.starts_with("tmux")
}

/// Wrap a sequence for tmux/screen passthrough if `term` calls for it —
/// `\033Ptmux;` prefix with every embedded ESC doubled, `\033\\` suffix,
/// exactly as the real `imgcat` script does.
pub fn wrap_for_passthrough(term: &str, sequence: &str) -> String {
    if !needs_tmux_passthrough(term) {
        return sequence.to_string();
    }
    let doubled = sequence.replace('\x1b', "\x1b\x1b");
    format!("\x1bPtmux;{doubled}\x1b\\")
}

/// One `File=` argument set for `imgcat`/`imgls`.
#[derive(Debug, Clone, Default)]
pub struct FileArgs {
    pub name: Option<String>,
    pub size: Option<usize>,
    /// Cells, `Npx`, `N%`, or `auto` — passed through verbatim, not
    /// validated here (the terminal is the one that interprets it).
    pub width: Option<String>,
    pub height: Option<String>,
    pub preserve_aspect_ratio: bool,
    pub inline: bool,
}

/// Build a single-shot `OSC 1337 ; File=<args>:<base64> BEL` sequence.
/// Real `imgcat` defaults to the multipart form and only uses this
/// single-shot form under `-l`/`--legacy`; this implementation always uses
/// it (a documented simplification — both forms are spec-legal and this
/// terminal's OSC 1337 handler supports both, but a single, simpler code
/// path here is enough for a CLI utility that isn't streaming multi-hundred-
/// megabyte files).
pub fn build_file_sequence(args: &FileArgs, bytes: &[u8]) -> String {
    let mut parts = Vec::new();
    if let Some(name) = &args.name {
        parts.push(format!("name={}", Base64.encode(name)));
    }
    if let Some(size) = args.size {
        parts.push(format!("size={size}"));
    }
    if let Some(width) = &args.width {
        parts.push(format!("width={width}"));
    }
    if let Some(height) = &args.height {
        parts.push(format!("height={height}"));
    }
    parts.push(format!(
        "preserveAspectRatio={}",
        if args.preserve_aspect_ratio { 1 } else { 0 }
    ));
    parts.push(format!("inline={}", if args.inline { 1 } else { 0 }));

    let payload = Base64.encode(bytes);
    format!("\x1b]1337;File={}:{payload}\x07", parts.join(";"))
}

/// `divider`'s exact, fixed sequence — verbatim behavior of the real
/// script: `File=inline=1;width=100%;height=1;preserveAspectRatio=0`, no
/// `name=`/`size=`, terminated with BEL then a newline.
pub fn build_divider_sequence(bytes: &[u8]) -> String {
    let payload = Base64.encode(bytes);
    format!("\x1b]1337;File=inline=1;width=100%;height=1;preserveAspectRatio=0:{payload}\x07\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_passthrough_needed_for_a_plain_terminal() {
        assert_eq!(wrap_for_passthrough("xterm-256color", "SEQ"), "SEQ");
    }

    #[test]
    fn tmux_term_gets_wrapped_with_doubled_escapes() {
        let wrapped = wrap_for_passthrough("tmux-256color", "\x1bfoo\x1bbar");
        assert_eq!(wrapped, "\x1bPtmux;\x1b\x1bfoo\x1b\x1bbar\x1b\\");
    }

    #[test]
    fn screen_term_also_gets_wrapped() {
        assert!(needs_tmux_passthrough("screen"));
        assert!(needs_tmux_passthrough("screen-256color"));
        assert!(needs_tmux_passthrough("tmux"));
        assert!(!needs_tmux_passthrough("xterm-ghostty"));
    }

    #[test]
    fn build_file_sequence_includes_all_provided_args_in_order() {
        let args = FileArgs {
            name: Some("photo.png".to_string()),
            size: Some(4),
            width: Some("10".to_string()),
            height: Some("auto".to_string()),
            preserve_aspect_ratio: true,
            inline: true,
        };
        let seq = build_file_sequence(&args, b"data");
        let expected_name = Base64.encode("photo.png");
        let expected_payload = Base64.encode(b"data");
        assert_eq!(
            seq,
            format!(
                "\x1b]1337;File=name={expected_name};size=4;width=10;height=auto;\
                 preserveAspectRatio=1;inline=1:{expected_payload}\x07"
            )
        );
    }

    #[test]
    fn build_file_sequence_omits_absent_optional_args() {
        let args = FileArgs {
            inline: true,
            preserve_aspect_ratio: false,
            ..Default::default()
        };
        let seq = build_file_sequence(&args, b"x");
        assert_eq!(
            seq,
            format!(
                "\x1b]1337;File=preserveAspectRatio=0;inline=1:{}\x07",
                Base64.encode(b"x")
            )
        );
    }

    #[test]
    fn divider_sequence_matches_the_real_scripts_exact_args() {
        let seq = build_divider_sequence(b"hello");
        assert_eq!(
            seq,
            format!(
                "\x1b]1337;File=inline=1;width=100%;height=1;preserveAspectRatio=0:{}\x07\n",
                Base64.encode(b"hello")
            )
        );
    }
}
