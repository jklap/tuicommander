use alacritty_terminal::grid::ReflowMode;
use tuic_terminal::terminal_grid::TerminalGrid;

// Catches deleting identical older scrollback merely because a later resize
// redraw prints the same text outside the viewport recorded at resize time.
#[test]
fn resize_reprint_keeps_identical_old_scrollback_outside_owned_viewport_1407() {
    let mut grid = TerminalGrid::new(3, 40, 100);
    let _ = grid.process(b"report.txt\r\narchive.txt\r\nfiller-a\r\nfiller-b\r\nfiller-c");
    grid.resize_with_mode(3, 48, ReflowMode::All);
    let _ = grid.process(
        b"\x1b[?2026h\x1b[H\x1b[2K\x1b[B\x1b[2K\x1b[B\x1b[2K\x1b[Hreport.txt\r\narchive.txt\r\nnew-output\x1b[?2026l",
    );
    let rows = grid.read_rows_in_range(0, grid.total_lines() - 1);
    assert_eq!(
        rows.iter().filter(|row| row.trim() == "report.txt").count(),
        2
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row.trim() == "archive.txt")
            .count(),
        2
    );
}

// Catches assuming an indistinguishable repeated line proves a new event.
// The explicitly authorized suffix rule keeps the older copy, but cannot
// preserve event multiplicity for an identical whole-prefix redraw.
#[test]
fn resize_redraw_identical_new_prefix_keeps_one_retained_copy_1407() {
    let mut grid = TerminalGrid::new(3, 80, 100);
    let _ = grid.process(b"identical result line\r\nOwned viewport first line\r\nOwned viewport second line\r\nOwned viewport last line");
    grid.resize_with_mode(3, 100, ReflowMode::HistoryOnly);
    let _ = grid.process(b"\x1b[?2026h\x1b[H\x1b[2K\x1b[B\x1b[2K\x1b[B\x1b[2K\x1b[Hidentical result line\r\nOwned viewport first line\r\nOwned viewport second line\r\nOwned viewport last line\x1b[?2026l");
    let rows = grid.read_rows_in_range(0, grid.total_lines() - 1);
    assert_eq!(
        rows.iter()
            .filter(|row| row.trim() == "identical result line")
            .count(),
        1
    );
    assert!(
        rows.iter()
            .any(|row| row.trim() == "Owned viewport last line")
    );
}
