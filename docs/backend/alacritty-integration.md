# Alacritty Terminal Integration

TUICommander uses `alacritty_terminal` 0.26.0 as its terminal emulation backend. We maintain a local patch at `src-tauri/patches/alacritty_terminal/` referenced via `[patch.crates-io]` in `Cargo.toml`.

## Why a local patch

`alacritty_terminal` is designed for the Alacritty GUI app. Several methods and fields needed by an embedded terminal backend are private. Rather than forking the entire repo, we patch the crate locally — minimal changes, easy to audit, easy to rebase on upstream updates.

## Our patches

| File | Change | Why |
|------|--------|-----|
| `src/term/mod.rs` | `pub fn resize_reflow(size, reflow: bool)` | Disable reflow on resize. Ink/Claude Code uses CUU cursor positioning that breaks when reflow merges/splits lines. |
| `src/term/mod.rs` | `pub fn mark_fully_damaged()` (was `fn`) | Lets us force full-frame damage directly instead of maintaining a parallel flag. |
| `src/term/color.rs` | `pub fn named_color_to_index(NamedColor) -> Option<u8>` | Maps named colors to xterm-256 indices. Eliminates 30-line match duplication in our serializer. |

## Upstream API we use directly (no patch needed)

| API | Usage |
|-----|-------|
| `Term::new(config, dimensions, event_proxy)` | Create terminal grid |
| `Processor::advance(&mut term, data)` | Feed PTY bytes |
| `term.grid()` / `term.grid_mut()` | Read cell grid, cursor, history |
| `term.damage()` / `term.reset_damage()` | Dirty-row tracking for incremental serialization |
| `term.scroll_display(Scroll::Delta)` | Viewport scrolling |
| `term.mode()` | Check TermMode flags (ALT_SCREEN, SHOW_CURSOR, kitty keyboard) |
| `term.cursor_style()` | Cursor shape (block/beam/underline) |
| `term.colors()` | Dynamic color palette (OSC 4/10/11/12 overrides) |
| `term.selection` / `term.selection_to_string()` | Native selection API |
| `RegexSearch::new(query)` + `term.regex_search_right()` | Native DFA regex search across grid + scrollback |
| `EventListener` trait | Capture bell, title, clipboard, PTY write-back events |

## Notable forks and patches (external)

### Zed Editor (zed-industries/alacritty)

Zed maintains branches on their fork with patches not yet upstream:

| Branch | What | Relevance |
|--------|------|-----------|
| `osc-133` | Semantic cell tagging — cells get `Osc133CellType` (Prompt/Input/Output) from OSC 133 sequences. Fires `Event::Osc133`. Requires Zed's VTE fork (`osc-133-2` branch). | **High** — would replace our regex-based `extract_osc133()` pre-parser. Enables prompt zone rendering. See story 1552. |
| `v0.16-child-exit-patch` | Uses `exit_status.into_raw()` instead of `.code()` for `ChildExit`, so signal-killed processes are distinguishable from normal exits. | **Medium** — improves crash detection for shell sessions. See story 1553. |
| `use-zed-vte` | Pins to Zed's VTE fork which adds `Serialize`/`Deserialize` to VTE parser state. | Prerequisite for OSC 133 branch and terminal state snapshotting. |
| `scrollback` | Added scrollback buffer — already merged into upstream alacritty. | None (already upstream). |
| `scroll/fix-alt-grid-size` | Alt screen gets zero scrollback — already merged upstream. | None (already upstream). |

## Update procedure

### Checking for upstream updates

```bash
# Check latest version on crates.io
cargo search alacritty_terminal

# Compare with our pinned version
grep "alacritty_terminal" src-tauri/Cargo.toml
```

### Rebasing our patch on a new upstream version

1. Download the new version:
   ```bash
   cargo download alacritty_terminal@<new_version> -o /tmp/alacritty_new
   ```
   Or copy from `~/.cargo/registry/src/` after adding the new version to Cargo.toml.

2. Diff our patches against the old upstream:
   ```bash
   diff -ru ~/.cargo/registry/src/*/alacritty_terminal-0.26.0/src/term/mod.rs \
            src-tauri/patches/alacritty_terminal/src/term/mod.rs
   ```

3. Apply patches to the new version. Our changes are small and isolated:
   - `resize_reflow` in `term/mod.rs` — add method, modify `resize()` to call it
   - `mark_fully_damaged` visibility in `term/mod.rs` — `fn` → `pub fn`
   - `named_color_to_index` in `term/color.rs` — new function, no existing code modified

4. Update `Cargo.toml` version and the `patches/` directory.

5. Run tests: `cargo test terminal_grid && cargo test vt_log`

### Checking Zed's fork for new patches

```bash
# List branches on Zed's fork
gh api repos/zed-industries/alacritty/branches --jq '.[].name'

# Compare a specific branch
# https://github.com/zed-industries/alacritty/compare/master...<branch>
```

### Periodic review cadence

- **Monthly:** Check crates.io for new alacritty_terminal releases.
- **Quarterly:** Review Zed fork branches for new patches relevant to our use case.
- **On major issues:** If we hit terminal emulation bugs, check if upstream or Zed has a fix before writing our own.

## Planned patches (stories)

| Story | Priority | Description |
|-------|----------|-------------|
| 1552-02ff | P2 | Port Zed OSC 133 semantic cell tagging (requires VTE fork) |
| 1550-64b1 | P3 | Move OSC 133 extraction into VTE handler (blocked by 1552) |
| 1553-5e8c | P3 | Port Zed child-exit raw waitpid status |
