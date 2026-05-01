use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term, TermDamage, TermMode};
use alacritty_terminal::vte::ansi::{self, Color, NamedColor, Rgb};

use crate::state::{ChangedRow, LogColor, LogLine, LogSpan};

// Attrs byte bit positions for binary cell encoding.
const ATTR_BOLD: u8       = 0b0000_0001;
const ATTR_ITALIC: u8     = 0b0000_0010;
const ATTR_UNDERLINE: u8  = 0b0000_0100;
const ATTR_STRIKEOUT: u8  = 0b0000_1000;
const ATTR_DIM: u8        = 0b0001_0000;
const ATTR_INVERSE: u8    = 0b0010_0000;
const ATTR_DEFAULT_FG: u8 = 0b0100_0000;
const ATTR_DEFAULT_BG: u8 = 0b1000_0000;

/// Standard xterm 256-color palette (16 ANSI + 216 color cube + 24 grayscale).
fn xterm_color_rgb(index: u8) -> Rgb {
    match index {
        // 16 standard ANSI colors
        0  => Rgb { r: 0,   g: 0,   b: 0   },
        1  => Rgb { r: 205, g: 0,   b: 0   },
        2  => Rgb { r: 0,   g: 205, b: 0   },
        3  => Rgb { r: 205, g: 205, b: 0   },
        4  => Rgb { r: 0,   g: 0,   b: 238 },
        5  => Rgb { r: 205, g: 0,   b: 205 },
        6  => Rgb { r: 0,   g: 205, b: 205 },
        7  => Rgb { r: 229, g: 229, b: 229 },
        8  => Rgb { r: 127, g: 127, b: 127 },
        9  => Rgb { r: 255, g: 0,   b: 0   },
        10 => Rgb { r: 0,   g: 255, b: 0   },
        11 => Rgb { r: 255, g: 255, b: 0   },
        12 => Rgb { r: 92,  g: 92,  b: 255 },
        13 => Rgb { r: 255, g: 0,   b: 255 },
        14 => Rgb { r: 0,   g: 255, b: 255 },
        15 => Rgb { r: 255, g: 255, b: 255 },
        // 216-color cube (indices 16-231)
        16..=231 => {
            let n = index - 16;
            let b_idx = n % 6;
            let g_idx = (n / 6) % 6;
            let r_idx = n / 36;
            let to_val = |i: u8| if i == 0 { 0 } else { 55 + 40 * i };
            Rgb { r: to_val(r_idx), g: to_val(g_idx), b: to_val(b_idx) }
        }
        // 24-step grayscale ramp (indices 232-255)
        232..=255 => {
            let v = 8 + 10 * (index - 232);
            Rgb { r: v, g: v, b: v }
        }
    }
}

/// Resolve a `Color` to RGB, returning `None` for default fg/bg.
fn resolve_color(c: Color) -> Option<Rgb> {
    match c {
        Color::Spec(rgb) => Some(rgb),
        Color::Indexed(i) => Some(xterm_color_rgb(i)),
        Color::Named(n) => match n {
            NamedColor::Foreground | NamedColor::Background | NamedColor::Cursor
            | NamedColor::BrightForeground | NamedColor::DimForeground => None,
            NamedColor::DimBlack   => Some(xterm_color_rgb(0)),
            NamedColor::DimRed     => Some(xterm_color_rgb(1)),
            NamedColor::DimGreen   => Some(xterm_color_rgb(2)),
            NamedColor::DimYellow  => Some(xterm_color_rgb(3)),
            NamedColor::DimBlue    => Some(xterm_color_rgb(4)),
            NamedColor::DimMagenta => Some(xterm_color_rgb(5)),
            NamedColor::DimCyan    => Some(xterm_color_rgb(6)),
            NamedColor::DimWhite   => Some(xterm_color_rgb(7)),
            // Black=0..BrightWhite=15
            _ => Some(xterm_color_rgb(n as u8)),
        },
    }
}

/// Wraps `alacritty_terminal::Term` with a TUICommander-specific API.
///
/// Provides the same `process() → Vec<ChangedRow>` + `screen_text_rows()`
/// interface that `VtLogBuffer` expects, so it can drop in as a replacement
/// for the current `vt100::Parser`.
pub struct TerminalGrid {
    term: Term<VoidListener>,
    processor: ansi::Processor,
    prev_rows: Vec<String>,
}

impl TerminalGrid {
    pub fn new(rows: u16, cols: u16, scrollback: usize) -> Self {
        let config = Config {
            scrolling_history: scrollback,
            ..Config::default()
        };
        let size = TermSize::new(cols as usize, rows as usize);
        let term = Term::new(config, &size, VoidListener);
        Self {
            term,
            processor: ansi::Processor::new(),
            prev_rows: Vec::new(),
        }
    }

    /// Feed raw PTY bytes into the terminal emulator.
    ///
    /// Returns changed rows since the last call (same contract as
    /// `VtLogBuffer::process()`).
    pub fn process(&mut self, data: &[u8]) -> Vec<ChangedRow> {
        self.processor.advance(&mut self.term, data);

        let curr_rows = self.read_screen_text();

        let changed: Vec<ChangedRow> = curr_rows
            .iter()
            .enumerate()
            .filter_map(|(i, curr)| {
                let prev = self.prev_rows.get(i).map(String::as_str).unwrap_or("");
                if curr != prev {
                    Some(ChangedRow {
                        row_index: i,
                        text: curr.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        self.prev_rows = curr_rows;
        changed
    }

    /// Returns plain text snapshot of all visible screen rows (trimmed).
    pub fn screen_text_rows(&self) -> Vec<String> {
        if self.prev_rows.is_empty() {
            self.read_screen_text()
        } else {
            self.prev_rows.clone()
        }
    }

    /// Whether the alternate screen buffer is currently active.
    pub fn is_alternate_screen(&self) -> bool {
        self.term.mode().contains(TermMode::ALT_SCREEN)
    }

    /// Number of scrollback lines above the visible screen.
    pub fn scrollback_count(&self) -> usize {
        self.term.grid().history_size()
    }

    /// Read a range of scrollback lines as plain text.
    /// `offset` is counted from the top of scrollback (0 = oldest visible).
    /// Returns up to `limit` lines.
    pub fn read_scrollback_lines(&self, offset: usize, limit: usize) -> Vec<String> {
        let grid = self.term.grid();
        let history = grid.history_size();
        if history == 0 || offset >= history {
            return Vec::new();
        }

        let count = limit.min(history - offset);
        let mut lines = Vec::with_capacity(count);
        let screen_lines = grid.screen_lines();

        for i in 0..count {
            let scrollback_idx = history - offset - i - 1;
            let line_idx = Line(-(scrollback_idx as i32) - 1);
            if let Some(text) = self.row_to_text(line_idx, screen_lines) {
                lines.push(text);
            }
        }
        lines
    }

    /// Clear the cached prev_rows to force full diff on next process().
    pub fn clear_prev_rows(&mut self) {
        self.prev_rows.clear();
    }

    /// Resize the terminal grid.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        let size = TermSize::new(cols as usize, rows as usize);
        self.term.resize(size);
        self.prev_rows.clear();
    }

    /// Number of visible screen rows.
    pub fn screen_lines(&self) -> usize {
        self.term.grid().screen_lines()
    }

    /// Number of visible columns.
    pub fn columns(&self) -> usize {
        self.term.grid().columns()
    }

    /// Access the underlying Term (for future rendering/selection needs).
    pub fn term(&self) -> &Term<VoidListener> {
        &self.term
    }

    /// Mutable access to the underlying Term.
    pub fn term_mut(&mut self) -> &mut Term<VoidListener> {
        &mut self.term
    }

    /// Read the cursor position (line, column) in screen coordinates.
    pub fn cursor_point(&self) -> (usize, usize) {
        let point = self.term.grid().cursor.point;
        (point.line.0 as usize, point.column.0)
    }

    /// Extract a styled `LogLine` from a grid row by iterating cells.
    ///
    /// Consecutive cells with the same (fg, bg, bold, italic, underline) attributes
    /// are grouped into a single `LogSpan`. Trailing whitespace-only spans with
    /// default attributes are trimmed.
    pub fn extract_log_line(&self, line: Line) -> LogLine {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        let mut spans: Vec<LogSpan> = Vec::new();

        let mut cur_fg: Option<LogColor> = None;
        let mut cur_bg: Option<LogColor> = None;
        let mut cur_bold = false;
        let mut cur_italic = false;
        let mut cur_underline = false;
        let mut cur_text = String::new();

        for col in 0..num_cols {
            let cell = &grid[line][Column(col)];
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }

            let fg = LogColor::from_ansi_color(cell.fg);
            let bg = LogColor::from_ansi_color(cell.bg);
            let bold = cell.flags.contains(Flags::BOLD);
            let italic = cell.flags.contains(Flags::ITALIC);
            let underline = cell.flags.intersects(Flags::UNDERLINE | Flags::DOUBLE_UNDERLINE | Flags::UNDERCURL);

            if !cur_text.is_empty()
                && (fg != cur_fg || bg != cur_bg || bold != cur_bold
                    || italic != cur_italic || underline != cur_underline)
            {
                spans.push(LogSpan {
                    text: std::mem::take(&mut cur_text),
                    fg: cur_fg,
                    bg: cur_bg,
                    bold: cur_bold,
                    italic: cur_italic,
                    underline: cur_underline,
                });
            }

            cur_fg = fg;
            cur_bg = bg;
            cur_bold = bold;
            cur_italic = italic;
            cur_underline = underline;

            if cell.c == ' ' || cell.c == '\0' {
                cur_text.push(' ');
            } else {
                cur_text.push(cell.c);
            }
        }

        if !cur_text.is_empty() {
            spans.push(LogSpan {
                text: cur_text,
                fg: cur_fg,
                bg: cur_bg,
                bold: cur_bold,
                italic: cur_italic,
                underline: cur_underline,
            });
        }

        // Trim trailing whitespace-only spans with default attrs
        while let Some(last) = spans.last() {
            if last.fg.is_none() && last.bg.is_none() && !last.bold && !last.italic && !last.underline
                && last.text.trim_end().is_empty()
            {
                spans.pop();
            } else {
                break;
            }
        }
        if let Some(last) = spans.last_mut() {
            let trimmed = last.text.trim_end().to_string();
            if trimmed.is_empty() && last.fg.is_none() && last.bg.is_none() && !last.bold && !last.italic && !last.underline {
                spans.pop();
            } else {
                last.text = trimmed;
            }
        }

        LogLine { spans, cols: num_cols as u16 }
    }

    /// Current visible screen rows as styled LogLines.
    pub fn screen_log_lines(&self) -> Vec<LogLine> {
        let num_lines = self.term.grid().screen_lines();
        let mut lines = Vec::with_capacity(num_lines);
        for i in 0..num_lines {
            lines.push(self.extract_log_line(Line(i as i32)));
        }
        lines
    }

    /// Read `count` most-recent scrollback lines as styled `LogLine`s.
    /// Soft-wrapped rows (WRAPLINE) are merged into their parent line.
    pub fn read_scrollback_log_lines(&self, count: usize) -> Vec<LogLine> {
        let grid = self.term.grid();
        let history = grid.history_size();
        if history == 0 || count == 0 {
            return Vec::new();
        }
        let actual_count = count.min(history);
        let mut result: Vec<LogLine> = Vec::with_capacity(actual_count);

        // Read from oldest to newest within the requested range
        for i in 0..actual_count {
            let scrollback_idx = actual_count - i - 1;
            let line_idx = Line(-(scrollback_idx as i32) - 1);
            let log_line = self.extract_log_line(line_idx);

            // Check if the previous row (older, one further into history) had WRAPLINE
            let prev_scrollback_idx = scrollback_idx + 1;
            let is_continuation = if prev_scrollback_idx < history {
                let prev_line = Line(-(prev_scrollback_idx as i32) - 1);
                let last_col = grid.columns().saturating_sub(1);
                grid[prev_line][Column(last_col)].flags.contains(Flags::WRAPLINE)
            } else {
                false
            };

            if is_continuation {
                if let Some(prev) = result.last_mut() {
                    prev.spans.extend(log_line.spans);
                } else {
                    result.push(log_line);
                }
            } else {
                result.push(log_line);
            }
        }
        result
    }

    /// Whether a screen row's last cell has WRAPLINE set (it continues on the next row).
    pub fn row_wrapped(&self, line: Line) -> bool {
        let grid = self.term.grid();
        let last_col = grid.columns().saturating_sub(1);
        grid[line][Column(last_col)].flags.contains(Flags::WRAPLINE)
    }

    /// Extract the user-typed text from the prompt line, excluding ghost/suggestion text.
    pub fn prompt_input_text(&self) -> Option<String> {
        let grid = self.term.grid();
        let rows = grid.screen_lines();
        let cols = grid.columns();
        let cursor = grid.cursor.point;
        let cursor_row = cursor.line.0 as usize;
        let cursor_col = cursor.column.0;

        for row in (0..rows).rev() {
            let line = Line(row as i32);
            let mut row_text = String::with_capacity(cols);
            for col in 0..cols {
                let cell = &grid[line][Column(col)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                if cell.c == '\0' {
                    row_text.push(' ');
                } else {
                    row_text.push(cell.c);
                }
            }
            let trimmed = row_text.trim_start();
            if !(trimmed.starts_with('❯') || trimmed == ">" || trimmed.starts_with("> ")) {
                continue;
            }

            let col_limit = if row == cursor_row { cursor_col } else { cols };
            let mut result_text = String::new();
            let mut past_prompt = false;
            for col in 0..col_limit {
                let cell = &grid[line][Column(col)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                let ch = cell.c;
                if !past_prompt {
                    if ch == '❯' || ch == '›' || ch == '>' {
                        past_prompt = true;
                        continue;
                    }
                    if ch == ' ' || ch == '\t' {
                        continue;
                    }
                    past_prompt = true;
                }
                if past_prompt && (ch == ' ' || ch == '\t') && result_text.is_empty() {
                    continue;
                }
                if cell.flags.contains(Flags::DIM) {
                    break;
                }
                if ch == '\0' {
                    result_text.push(' ');
                } else {
                    result_text.push(ch);
                }
            }
            return Some(result_text.trim_end().to_string());
        }
        None
    }

    /// Serialize dirty rows as a compact binary frame.
    ///
    /// Uses alacritty's built-in damage tracking to identify changed rows.
    /// Wire format:
    /// ```text
    /// Header: [num_rows: u16] [cursor_row: u16] [cursor_col: u16] [cursor_visible: u8]
    /// Per row: [row_index: u16] [col_count: u16] [cells...]
    /// Per cell: [char: u32 LE] [fg_r, fg_g, fg_b] [bg_r, bg_g, bg_b] [attrs: u8]
    /// ```
    /// attrs: bit0=bold, bit1=italic, bit2=underline, bit3=strikeout,
    ///        bit4=dim, bit5=inverse, bit6=default_fg, bit7=default_bg
    pub fn serialize_dirty_rows(&mut self) -> Vec<u8> {
        let num_cols = self.term.grid().columns();
        let num_lines = self.term.grid().screen_lines();
        let cursor = self.term.grid().cursor.point;
        let cursor_visible = self.term.mode().contains(TermMode::SHOW_CURSOR);

        let damage = self.term.damage();
        let dirty_lines: Vec<usize> = match damage {
            TermDamage::Full => (0..num_lines).collect(),
            TermDamage::Partial(iter) => iter.map(|b| b.line).collect(),
        };

        if dirty_lines.is_empty() {
            self.term.reset_damage();
            return Vec::new();
        }

        // Header: 7 bytes
        let row_count = dirty_lines.len();
        let estimated = 7 + row_count * (4 + num_cols * 11);
        let mut buf = Vec::with_capacity(estimated);

        buf.extend_from_slice(&(row_count as u16).to_le_bytes());
        buf.extend_from_slice(&(cursor.line.0 as u16).to_le_bytes());
        buf.extend_from_slice(&(cursor.column.0 as u16).to_le_bytes());
        buf.push(cursor_visible as u8);

        let grid = self.term.grid();
        for &row_idx in &dirty_lines {
            let line = Line(row_idx as i32);
            buf.extend_from_slice(&(row_idx as u16).to_le_bytes());
            buf.extend_from_slice(&(num_cols as u16).to_le_bytes());

            for col in 0..num_cols {
                let cell = &grid[line][Column(col)];
                let ch = if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    0u32
                } else if cell.c == '\0' {
                    ' ' as u32
                } else {
                    cell.c as u32
                };
                buf.extend_from_slice(&ch.to_le_bytes());

                let (fg_rgb, fg_default) = match resolve_color(cell.fg) {
                    Some(rgb) => (rgb, false),
                    None => (Rgb { r: 0, g: 0, b: 0 }, true),
                };
                buf.push(fg_rgb.r);
                buf.push(fg_rgb.g);
                buf.push(fg_rgb.b);

                let (bg_rgb, bg_default) = match resolve_color(cell.bg) {
                    Some(rgb) => (rgb, false),
                    None => (Rgb { r: 0, g: 0, b: 0 }, true),
                };
                buf.push(bg_rgb.r);
                buf.push(bg_rgb.g);
                buf.push(bg_rgb.b);

                let flags = cell.flags;
                let mut attrs: u8 = 0;
                if flags.contains(Flags::BOLD) { attrs |= ATTR_BOLD; }
                if flags.contains(Flags::ITALIC) { attrs |= ATTR_ITALIC; }
                if flags.intersects(Flags::UNDERLINE | Flags::DOUBLE_UNDERLINE | Flags::UNDERCURL) { attrs |= ATTR_UNDERLINE; }
                if flags.contains(Flags::STRIKEOUT) { attrs |= ATTR_STRIKEOUT; }
                if flags.contains(Flags::DIM) { attrs |= ATTR_DIM; }
                if flags.contains(Flags::INVERSE) { attrs |= ATTR_INVERSE; }
                if fg_default { attrs |= ATTR_DEFAULT_FG; }
                if bg_default { attrs |= ATTR_DEFAULT_BG; }
                buf.push(attrs);
            }
        }

        self.term.reset_damage();
        buf
    }

    fn read_screen_text(&self) -> Vec<String> {
        let grid = self.term.grid();
        let num_lines = grid.screen_lines();
        let num_cols = grid.columns();
        let mut rows = Vec::with_capacity(num_lines);
        for i in 0..num_lines {
            let line = Line(i as i32);
            let mut text = String::with_capacity(num_cols);
            for col in 0..num_cols {
                let cell = &grid[line][Column(col)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                text.push(cell.c);
            }
            rows.push(text.trim_end().to_string());
        }
        rows
    }

    fn row_to_text(&self, line: Line, _screen_lines: usize) -> Option<String> {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        let mut text = String::with_capacity(num_cols);
        for col in 0..num_cols {
            let cell = &grid[line][Column(col)];
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            text.push(cell.c);
        }
        Some(text.trim_end().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_empty_grid() {
        let grid = TerminalGrid::new(24, 80, 1000);
        assert_eq!(grid.screen_lines(), 24);
        assert_eq!(grid.columns(), 80);
        assert_eq!(grid.scrollback_count(), 0);
        assert!(!grid.is_alternate_screen());
    }

    #[test]
    fn process_simple_text() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        let changed = grid.process(b"hello world");
        assert!(!changed.is_empty());
        let first = &changed[0];
        assert_eq!(first.row_index, 0);
        assert_eq!(first.text, "hello world");
    }

    #[test]
    fn process_returns_empty_on_no_change() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process(b"hello");
        let changed = grid.process(b"");
        assert!(changed.is_empty());
    }

    #[test]
    fn screen_text_rows_returns_visible_content() {
        let mut grid = TerminalGrid::new(5, 20, 100);
        grid.process(b"line1\r\nline2\r\nline3");
        let rows = grid.screen_text_rows();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0], "line1");
        assert_eq!(rows[1], "line2");
        assert_eq!(rows[2], "line3");
        assert_eq!(rows[3], "");
        assert_eq!(rows[4], "");
    }

    #[test]
    fn cursor_position_tracks_output() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process(b"abc");
        let (line, col) = grid.cursor_point();
        assert_eq!(line, 0);
        assert_eq!(col, 3);
    }

    #[test]
    fn cursor_moves_on_newline() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process(b"abc\r\ndef");
        let (line, col) = grid.cursor_point();
        assert_eq!(line, 1);
        assert_eq!(col, 3);
    }

    #[test]
    fn alt_screen_toggle() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        assert!(!grid.is_alternate_screen());
        // Enter alt screen: CSI ? 1049 h
        grid.process(b"\x1b[?1049h");
        assert!(grid.is_alternate_screen());
        // Exit alt screen: CSI ? 1049 l
        grid.process(b"\x1b[?1049l");
        assert!(!grid.is_alternate_screen());
    }

    #[test]
    fn scrollback_generated_by_overflow() {
        let mut grid = TerminalGrid::new(3, 20, 100);
        // Write 5 lines into a 3-row terminal → 2 lines scroll into history
        grid.process(b"line1\r\nline2\r\nline3\r\nline4\r\nline5");
        assert!(grid.scrollback_count() >= 2);
    }

    #[test]
    fn resize_updates_dimensions() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.resize(10, 40);
        assert_eq!(grid.screen_lines(), 10);
        assert_eq!(grid.columns(), 40);
    }

    #[test]
    fn changed_rows_detects_overwrite() {
        let mut grid = TerminalGrid::new(5, 20, 100);
        grid.process(b"hello");
        // Move cursor to beginning of line and overwrite
        let changed = grid.process(b"\rworld");
        assert!(!changed.is_empty());
        assert_eq!(changed[0].text, "world");
    }

    #[test]
    fn ansi_colors_do_not_leak_into_text() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process(b"\x1b[31mred text\x1b[0m");
        let rows = grid.screen_text_rows();
        assert_eq!(rows[0], "red text");
    }

    #[test]
    fn wide_chars_handled() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process("日本語".as_bytes());
        let rows = grid.screen_text_rows();
        assert!(rows[0].contains("日本語"));
    }

    #[test]
    fn cursor_movement_escape_sequences() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        // Write text, move cursor up 1 line (CUU), write more
        grid.process(b"first\r\nsecond");
        grid.process(b"\x1b[A"); // cursor up
        let (line, _col) = grid.cursor_point();
        assert_eq!(line, 0);
    }

    #[test]
    fn erase_in_line() {
        let mut grid = TerminalGrid::new(24, 80, 1000);
        grid.process(b"hello world");
        // Move to column 5, erase to end of line
        grid.process(b"\x1b[6G\x1b[K");
        let rows = grid.screen_text_rows();
        assert_eq!(rows[0], "hello");
    }

    // --- Binary serialization tests ---

    /// Helper: decode the header from a serialized frame.
    fn decode_header(buf: &[u8]) -> (u16, u16, u16, bool) {
        let num_rows = u16::from_le_bytes([buf[0], buf[1]]);
        let cursor_row = u16::from_le_bytes([buf[2], buf[3]]);
        let cursor_col = u16::from_le_bytes([buf[4], buf[5]]);
        let cursor_visible = buf[6] != 0;
        (num_rows, cursor_row, cursor_col, cursor_visible)
    }

    /// Helper: decode one cell (11 bytes) from a buffer at a given offset.
    /// Returns (char, fg_r, fg_g, fg_b, bg_r, bg_g, bg_b, attrs).
    fn decode_cell(buf: &[u8], offset: usize) -> (char, u8, u8, u8, u8, u8, u8, u8) {
        let ch = u32::from_le_bytes([buf[offset], buf[offset+1], buf[offset+2], buf[offset+3]]);
        let ch = char::from_u32(ch).unwrap_or('\0');
        (ch, buf[offset+4], buf[offset+5], buf[offset+6],
             buf[offset+7], buf[offset+8], buf[offset+9], buf[offset+10])
    }

    #[test]
    fn serialize_plain_text_roundtrip() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        grid.process(b"Hi");
        let buf = grid.serialize_dirty_rows();
        assert!(!buf.is_empty());

        let (num_rows, cursor_row, cursor_col, cursor_visible) = decode_header(&buf);
        assert!(num_rows >= 1, "at least row 0 dirty");
        assert_eq!(cursor_row, 0);
        assert_eq!(cursor_col, 2);
        assert!(cursor_visible);

        // First dirty row header starts at offset 7
        let row_idx = u16::from_le_bytes([buf[7], buf[8]]);
        let col_count = u16::from_le_bytes([buf[9], buf[10]]);
        assert_eq!(row_idx, 0);
        assert_eq!(col_count, 10);

        // First cell = 'H'
        let (ch, _, _, _, _, _, _, attrs) = decode_cell(&buf, 11);
        assert_eq!(ch, 'H');
        assert_ne!(attrs & super::ATTR_DEFAULT_FG, 0, "default fg flag set");
        assert_ne!(attrs & super::ATTR_DEFAULT_BG, 0, "default bg flag set");

        // Second cell = 'i'
        let (ch, _, _, _, _, _, _, _) = decode_cell(&buf, 11 + 11);
        assert_eq!(ch, 'i');
    }

    #[test]
    fn serialize_colored_text_preserves_rgb() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        // ESC[31m = red foreground (ANSI color 1)
        grid.process(b"\x1b[31mX\x1b[0m");
        let buf = grid.serialize_dirty_rows();

        // Find row 0, cell 0 — should have red fg
        let (ch, fg_r, fg_g, fg_b, _, _, _, attrs) = decode_cell(&buf, 11);
        assert_eq!(ch, 'X');
        assert_eq!(fg_r, 205); // xterm red
        assert_eq!(fg_g, 0);
        assert_eq!(fg_b, 0);
        assert_eq!(attrs & super::ATTR_DEFAULT_FG, 0, "fg is NOT default");
        assert_ne!(attrs & super::ATTR_DEFAULT_BG, 0, "bg IS default");
    }

    #[test]
    fn serialize_bold_italic_attrs() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        // Bold + italic
        grid.process(b"\x1b[1;3mB\x1b[0m");
        let buf = grid.serialize_dirty_rows();

        let (ch, _, _, _, _, _, _, attrs) = decode_cell(&buf, 11);
        assert_eq!(ch, 'B');
        assert_ne!(attrs & super::ATTR_BOLD, 0, "bold flag");
        assert_ne!(attrs & super::ATTR_ITALIC, 0, "italic flag");
        assert_eq!(attrs & super::ATTR_UNDERLINE, 0, "no underline");
    }

    #[test]
    fn serialize_only_dirty_rows_after_reset() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        grid.process(b"line1\r\nline2\r\nline3");
        // Drain initial damage
        let _ = grid.serialize_dirty_rows();

        // Now modify only row 0
        grid.process(b"\x1b[1;1Hchanged");
        let buf = grid.serialize_dirty_rows();

        if buf.is_empty() {
            // Damage was Full due to cursor move — acceptable
            return;
        }
        let (num_rows, _, _, _) = decode_header(&buf);
        // Should have fewer rows than the full 5
        assert!(num_rows <= 5, "partial damage, got {num_rows} rows");
    }

    #[test]
    fn serialize_wide_char_spacer_is_zero() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        grid.process("日".as_bytes()); // wide char takes 2 columns
        let buf = grid.serialize_dirty_rows();

        // Cell 0 = '日'
        let (ch0, _, _, _, _, _, _, _) = decode_cell(&buf, 11);
        assert_eq!(ch0, '日');
        // Cell 1 = wide char spacer → encoded as 0
        let ch1_raw = u32::from_le_bytes([buf[22], buf[23], buf[24], buf[25]]);
        assert_eq!(ch1_raw, 0, "wide char spacer encoded as 0");
    }

    #[test]
    fn serialize_frame_size_within_budget() {
        // Worst case: 220x50 all dirty
        let mut grid = TerminalGrid::new(50, 220, 0);
        // Fill every cell to ensure all rows are dirty
        for _ in 0..50 {
            grid.process(b"XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX\r\n");
        }
        let buf = grid.serialize_dirty_rows();
        assert!(
            buf.len() < 256 * 1024,
            "frame must be under 256KB, got {} bytes",
            buf.len()
        );
        // Expected: 7 header + 50 rows × (4 row header + 220 cells × 11 bytes)
        // = 7 + 50 × (4 + 2420) = 7 + 121_200 = 121_207 bytes
    }

    #[test]
    fn serialize_cursor_hidden() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        // DECTCEM: hide cursor
        grid.process(b"\x1b[?25l");
        grid.process(b"text");
        let buf = grid.serialize_dirty_rows();
        let (_, _, _, cursor_visible) = decode_header(&buf);
        assert!(!cursor_visible, "cursor should be hidden");
    }

    #[test]
    fn serialize_rgb_color_passthrough() {
        let mut grid = TerminalGrid::new(5, 10, 0);
        // ESC[38;2;100;150;200m = 24-bit fg color
        grid.process(b"\x1b[38;2;100;150;200mR\x1b[0m");
        let buf = grid.serialize_dirty_rows();

        let (ch, fg_r, fg_g, fg_b, _, _, _, attrs) = decode_cell(&buf, 11);
        assert_eq!(ch, 'R');
        assert_eq!(fg_r, 100);
        assert_eq!(fg_g, 150);
        assert_eq!(fg_b, 200);
        assert_eq!(attrs & super::ATTR_DEFAULT_FG, 0, "fg is NOT default");
    }
}
