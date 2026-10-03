use alacritty_terminal::grid::{Dimensions, Grid, ReflowMode};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use tuic_terminal::terminal_grid::TerminalGrid;

// Catches history purge inventing a composer origin for retained literal content.
#[test]
fn copied_selection_history_purge_without_erasing_retained_row_keeps_literal_marker() {
    let mut grid = TerminalGrid::new(2, 80, 4);
    let _ = grid.process(b"older\r\npredecessor\r\n");
    let _ = grid.process("❯ literal\r\nlast".as_bytes());
    let _ = grid.process(b"\x1b[3J");
    assert_eq!(grid.get_selection_text(0, 0, 0, 79), "❯ literal");
}

// Catches a full row erase failing to restore known origin after predecessor eviction.
#[test]
fn copied_selection_full_line_erase_restores_origin_after_eviction() {
    let mut grid = TerminalGrid::new(2, 80, 1);
    let _ = grid.process(b"old\r\nolder\r\nbody\r\nlast");
    let _ = grid.process(b"\x1b[3J\x1b[H\x1b[2K");
    let _ = grid.process("❯ new composer".as_bytes());
    assert_eq!(grid.get_selection_text(0, 0, 0, 79), "new composer");
}

// Catches treating a partial erase as a fresh row and stripping retained content.
#[test]
fn copied_selection_partial_line_erase_keeps_unknown_origin_literal() {
    let mut grid = TerminalGrid::new(2, 80, 1);
    let _ = grid.process(b"old\r\nolder\r\n");
    let _ = grid.process("❯ literal\r\nlast".as_bytes());
    let _ = grid.process(b"\x1b[3J\x1b[1;10H\x1b[K");
    assert_eq!(grid.get_selection_text(0, 0, 0, 79), "❯ literal");
}

// Catches ED0 clearing every cell but leaving predecessor-loss provenance behind.
#[test]
fn copied_selection_full_erase_below_restores_known_origin() {
    let mut grid = TerminalGrid::new(2, 80, 1);
    let _ = grid.process("old\r\n❯ literal\r\nlast\r\nmore".as_bytes());
    let _ = grid.process(b"\x1b[3J\x1b[H\x1b[J");
    let _ = grid.process("❯ new composer".as_bytes());
    assert_eq!(grid.get_selection_text(0, 0, 0, 79), "new composer");
}

// Catches ED1 clearing through the last column but failing to start a fresh row origin.
#[test]
fn copied_selection_full_erase_above_restores_known_origin() {
    let mut grid = TerminalGrid::new(2, 80, 1);
    let _ = grid.process("old\r\n❯ literal\r\nlast\r\nmore".as_bytes());
    let _ = grid.process(b"\x1b[3J\x1b[1;80H\x1b[1J\x1b[H");
    let _ = grid.process("❯ new composer".as_bytes());
    assert_eq!(grid.get_selection_text(0, 0, 0, 79), "new composer");
}

// Catches forgetting that a blank-looking WRAPLINE row still carries lost content origin.
#[test]
fn copied_selection_history_removal_distinguishes_blank_content_and_wrap_predecessors() {
    for (predecessor, wrapped, purge_unknown, drop_unknown) in [
        (' ', false, false, false),
        ('x', false, true, false),
        (' ', true, true, true),
    ] {
        for drop_tail in [false, true] {
            let mut grid: Grid<Cell> = Grid::new(2, 10, 2);
            grid[Line(0)][Column(0)].c = predecessor;
            if wrapped {
                grid[Line(0)][Column(9)].flags.insert(Flags::WRAPLINE);
            }
            grid.scroll_up(&(Line(0)..Line(2)), 1);
            grid[Line(0)][Column(0)].c = 's';
            if drop_tail {
                grid.drop_newest_history(1);
            } else {
                grid.clear_history();
            }
            // Reprint-tail drop preserves a known unwrapped boundary; purge
            // conservatively treats removed content as lost predecessor evidence.
            let expected_unknown = if drop_tail {
                drop_unknown
            } else {
                purge_unknown
            };
            assert_eq!(grid[Line(0)].copy_origin_unknown, expected_unknown);
        }
    }
}

// Catches history removal poisoning a fresh blank survivor after real content was lost.
#[test]
fn copied_selection_history_removal_leaves_blank_survivor_known() {
    for drop_tail in [false, true] {
        let mut grid: Grid<Cell> = Grid::new(2, 10, 2);
        grid[Line(0)][Column(0)].c = 'x';
        grid.scroll_up(&(Line(0)..Line(2)), 1);
        if drop_tail {
            grid.drop_newest_history(1);
        } else {
            grid.clear_history();
        }
        assert!(!grid[grid.topmost_line()].copy_origin_unknown);
    }
}

// Catches history-cap trimming poisoning a blank row before a later composer redraw.
#[test]
fn copied_selection_history_cap_trim_then_redraw_keeps_blank_origin_known() {
    for drop_tail in [false, true] {
        let mut grid: Grid<Cell> = Grid::new(2, 10, 4);
        grid.scroll_up(&(Line(0)..Line(2)), 2);
        grid.update_history(1);
        assert!(!grid[grid.topmost_line()].copy_origin_unknown);
        grid[Line(0)][Column(0)].c = 's';
        if drop_tail {
            grid.drop_newest_history(1);
        } else {
            grid.clear_history();
        }
        assert!(!grid[Line(0)].copy_origin_unknown);
    }
}

// Catches applying immutable blank-history exemption to a live screen continuation.
#[test]
fn copied_selection_eviction_keeps_live_blank_origin_unknown_but_blank_history_known() {
    for (history_cap, expected_unknown) in [(0, true), (1, false)] {
        let mut grid: Grid<Cell> = Grid::new(3, 10, history_cap);
        grid[Line(0)][Column(0)].c = 'x';
        grid[Line(0)][Column(9)].flags.insert(Flags::WRAPLINE);
        grid.scroll_up(&(Line(0)..Line(3)), 1);
        if history_cap != 0 {
            // Evict the wrapped head, leaving an immutable blank history row.
            grid.scroll_up(&(Line(0)..Line(3)), 1);
        }
        let oldest = grid.topmost_line();
        assert_eq!(grid[oldest].copy_origin_unknown, expected_unknown);
        // Screen content can arrive later without a full row reset.
        grid[Line(0)][Column(1)].c = 's';
        assert_eq!(grid[oldest].copy_origin_unknown, expected_unknown);
    }
}

// Catches resize losing a latent boundary, or a full row reset failing to clear it.
#[test]
fn copied_selection_pulled_history_boundary_activates_then_full_reset_restores_known_origin() {
    let mut grid: Grid<Cell> = Grid::new(3, 10, 1);
    grid[Line(0)][Column(0)].c = 'x';
    grid[Line(0)][Column(9)].flags.insert(Flags::WRAPLINE);
    grid.scroll_up(&(Line(0)..Line(3)), 1);
    grid.scroll_up(&(Line(0)..Line(3)), 1);
    assert!(!grid[grid.topmost_line()].copy_origin_unknown);
    grid.resize(ReflowMode::None, 4, 10);
    assert!(grid[Line(0)].copy_origin_unknown);
    grid.reset_region(Line(0)..Line(1));
    grid[Line(0)][Column(0)].c = 's';
    grid.resize(ReflowMode::None, 4, 12);
    assert!(!grid[Line(0)].copy_origin_unknown);
}

// Catches ED1 on row zero resetting rows below its cursor along with the erased row.
#[test]
fn copied_selection_ed1_on_first_row_restores_origin_without_erasing_second_row() {
    let mut grid = TerminalGrid::new(2, 80, 1);
    let _ = grid.process("old\r\nolder\r\n❯ literal\r\nlast\x1b[3J".as_bytes());
    assert_eq!(grid.get_selection_text(0, 0, 0, 79), "❯ literal");
    let _ = grid.process(b"\x1b[1;80H\x1b[1J");
    assert_eq!(grid.get_selection_text(0, 0, 0, 79).trim_end(), "");
    assert_eq!(grid.get_selection_text(1, 0, 1, 79), "last");
    let _ = grid.process("\x1b[1;1H❯ new composer".as_bytes());
    assert_eq!(grid.get_selection_text(0, 0, 0, 79), "new composer");
}

// Catches ED1 on the last row failing to reset all preceding rows or over-erasing its suffix.
#[test]
fn copied_selection_ed1_on_last_row_resets_preceding_rows_and_keeps_cursor_suffix() {
    let mut grid = TerminalGrid::new(3, 80, 1);
    let _ = grid.process("old\r\nolder\r\n❯ literal\r\nmiddle\r\nlast\x1b[3J".as_bytes());
    assert_eq!(grid.get_selection_text(0, 0, 0, 79), "❯ literal");
    let _ = grid.process(b"\x1b[3;1H\x1b[1J");
    for row in 0..2 {
        assert_eq!(grid.get_selection_text(row, 0, row, 79).trim_end(), "");
    }
    assert_eq!(grid.get_selection_text(2, 0, 2, 79), " ast");
    let _ = grid.process("\x1b[1;1H❯ first composer\x1b[2;1H❯ second composer".as_bytes());
    assert_eq!(grid.get_selection_text(0, 0, 0, 79), "first composer");
    assert_eq!(grid.get_selection_text(1, 0, 1, 79), "second composer");
    assert_eq!(grid.get_selection_text(2, 0, 2, 79), " ast");
}

// Catches clear_viewport retaining overflow loss on a newly blank zero-history row.
#[test]
fn copied_selection_full_viewport_reset_clears_both_flags_on_every_live_row() {
    for rows in [2, 3] {
        let mut grid: Grid<Cell> = Grid::new(rows, 10, 0);
        for row in 0..rows {
            grid[Line(row as i32)][Column(0)].c = 'x';
        }
        grid.clear_viewport();
        for row in 0..rows {
            let row = &grid[Line(row as i32)];
            assert!(row.is_clear());
            assert!(!row.copy_origin_unknown);
            assert!(!row.copy_predecessor_lost);
        }
    }
}

// Catches an ED2 provenance reset reaching retained history and stripping literal content later.
#[test]
fn copied_selection_full_viewport_reset_keeps_retained_history_loss_flags() {
    let mut grid: Grid<Cell> = Grid::new(3, 10, 1);
    for row in 0..3 {
        grid[Line(row)][Column(0)].c = 'x';
    }
    grid.scroll_up(&(Line(0)..Line(3)), 2);
    assert!(grid[Line(-1)].copy_origin_unknown);
    assert!(grid[Line(-1)].copy_predecessor_lost);
    grid.update_history(8);
    for row in 0..3 {
        grid[Line(row)][Column(0)].c = 'y';
    }
    grid.clear_viewport();
    assert_eq!(grid.history_size(), 4);
    assert_eq!(grid[Line(-4)][Column(0)].c, 'x');
    assert!(grid[Line(-4)].copy_origin_unknown);
    assert!(grid[Line(-4)].copy_predecessor_lost);
    for row in 0..3 {
        assert!(grid[Line(row)].is_clear());
        assert!(!grid[Line(row)].copy_origin_unknown);
        assert!(!grid[Line(row)].copy_predecessor_lost);
    }
}
