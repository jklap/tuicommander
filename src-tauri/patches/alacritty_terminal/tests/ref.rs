#![cfg(feature = "serde")]
use serde::Deserialize;
use serde_json as json;

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Grid};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi;

macro_rules! ref_tests {
    ($($name:ident)*) => {
        $(
            #[test]
            fn $name() {
                let test_dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/ref"));
                let test_path = test_dir.join(stringify!($name));
                ref_test(&test_path);
            }
        )*
    };
}

ref_tests! {
    alt_reset
    clear_underline
    colored_reset
    colored_underline
    csi_rep
    decaln_reset
    deccolm_reset
    delete_chars_reset
    delete_lines
    erase_chars_reset
    fish_cc
    grid_reset
    history
    hyperlinks
    indexed_256_colors
    insert_blank_reset
    issue_855
    ll
    newline_with_cursor_beyond_scroll_region
    region_scroll_down
    row_reset
    saved_cursor
    saved_cursor_alt
    scroll_up_reset
    selective_erasure
    sgr
    tab_rendering
    tmux_git_log
    tmux_htop
    underline
    vim_24bitcolors_bce
    vim_large_window_scroll
    vim_simple_edit
    vttest_cursor_movement_1
    vttest_insert
    vttest_origin_mode_1
    vttest_origin_mode_2
    vttest_scroll
    vttest_tab_clear_set
    wrapline_alt_toggle
    zerowidth
    zsh_tab_completion
    erase_in_line
    scroll_in_region_up_preserves_history
    origin_goto
}

fn read_u8<P>(path: P) -> Vec<u8>
where
    P: AsRef<Path>,
{
    let mut res = Vec::new();
    File::open(path.as_ref())
        .unwrap()
        .read_to_end(&mut res)
        .unwrap();

    res
}

#[derive(Deserialize, Default)]
struct RefConfig {
    history_size: u32,
}

#[derive(Copy, Clone)]
struct Mock;

impl EventListener for Mock {
    fn send_event(&self, _event: Event) {}
}

fn ref_test(dir: &Path) {
    let recording = read_u8(dir.join("alacritty.recording"));
    let serialized_size = fs::read_to_string(dir.join("size.json")).unwrap();
    let serialized_grid = fs::read_to_string(dir.join("grid.json")).unwrap();
    let serialized_cfg = fs::read_to_string(dir.join("config.json")).unwrap();

    let size: TermSize = json::from_str(&serialized_size).unwrap();
    let grid: Grid<Cell> = json::from_str(&serialized_grid).unwrap();
    let ref_config: RefConfig = json::from_str(&serialized_cfg).unwrap();

    let options = Config {
        scrolling_history: ref_config.history_size as usize,
        ..Default::default()
    };

    let mut terminal = Term::new(options, &size, Mock);
    let mut parser: ansi::Processor = ansi::Processor::new();

    parser.advance(&mut terminal, &recording);

    // Truncate invisible lines from the grid.
    let mut term_grid = terminal.grid().clone();
    term_grid.initialize_all();
    term_grid.truncate();

    if grid != term_grid {
        if std::env::var("ALACRITTY_REGEN_FIXTURES").is_ok() {
            let new_json = json::to_string(&term_grid).unwrap();
            fs::write(dir.join("grid.json"), new_json).unwrap();
            println!("Regenerated {}", dir.display());
            return;
        }

        if grid.total_lines() != term_grid.total_lines() {
            println!(
                "Dimensions differ: expected total_lines={} vs. actual total_lines={} \
                 (columns expected={} actual={}) — replay produced a different scrollback/reflow \
                 size than the fixture, so the per-cell diff below only covers the overlap.",
                grid.total_lines(),
                term_grid.total_lines(),
                grid.columns(),
                term_grid.columns(),
            );
        }

        let lines = grid.total_lines().min(term_grid.total_lines());
        let columns = grid.columns().min(term_grid.columns());
        // Some checked-in fixtures carry a `raw.visible_lines` one (or more)
        // short of their own `total_lines`/`lines` — harmless for the
        // `PartialEq` comparison above (which never indexes by `Line`), but
        // `Index<Line>` enforces `requested < visible_lines` via a
        // `debug_assert`, so indexing a fixture at its own recorded
        // `total_lines` can panic. This is diagnostic best-effort output for
        // an already-failing test, not the failure itself (that's the
        // `assert_eq!` below), so swallow the panic rather than let it hide
        // the real one under a `compute_index` message and location.
        let diffed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for i in 0..lines {
                for j in 0..columns {
                    let cell = &term_grid[Line(i as i32)][Column(j)];
                    let original_cell = &grid[Line(i as i32)][Column(j)];
                    if original_cell != cell {
                        println!("[{i}][{j}] {original_cell:?} => {cell:?}",);
                    }
                }
            }
        }));
        if diffed.is_err() {
            println!(
                "Per-cell diff aborted: indexing one of the grids panicked (likely the \
                 `visible_lines`-vs-`total_lines` fixture quirk noted above)."
            );
        }

        panic!("Ref test failed; grid doesn't match");
    }

    assert_eq!(grid, term_grid);
}
