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
