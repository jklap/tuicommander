# TUICommander — Terminal Component (`src/components/Terminal/`)

Repo-wide and general-frontend rules live in the root `AGENTS.md` and
`src/AGENTS.md` — read those first. This file covers what's specific to the
canvas-based terminal renderer, its selection/input handling, and its
consumption of the PTY/agent-signal backend documented in `src-tauri/AGENTS.md`.

## Smart Selection Drag Anchor

A double/quad-click "smart" match (a path, URL, etc. — see Smart Selection in
`docs/frontend/terminal-features.md`) must never reuse `"word"` mode's drag
anchor (`wordAnchor`/`extendSelectionDrag`'s `"word"` branch,
`canvasTerminalSelection.ts`). That branch re-derives its live drag edge from
the *plain* word-boundary resolver (`wordBoundsAt`/`getWordBoundaryResolver`)
at the current mouse position — which is narrower than a multi-segment smart
match at every point except its exact extent, since punctuation like `/`,
`.`, `:` splits it. Any `mousemove` before `mouseup` — including ordinary
pointer jitter during the double-click itself, well within the double-click
window, with zero real displacement — recomputed that narrower boundary and
collapsed the whole match down to just the sub-word under the cursor, before
the user ever got to drag anywhere. This shipped and went unnoticed because
the only existing test for a URL double-click (`canvasTerminalGestures.pin
.test.ts`) fired `mousedown`×2 → `mouseup` with no intervening `mousemove` —
exactly the one sequence real double-clicking never manages in practice.

Fixed by giving smart matches their own `"smart"` `SelectionMode` and
`smartAnchor` (full `{start, end}` coordinate pair) in `DragAnchor`, with a
dedicated `extendSelectionDrag` branch: while the live drag position stays
within `[start, end]` (inclusive both ends — a drag landing exactly on either
edge must not shrink either), the full match is kept untouched; only once the
drag genuinely moves past an edge does it extend outward, by whole word at
the new position (not the raw point) — this last part matters because the
built-in smart-selection rule set includes a low-precision catch-all `\S+`
rule (`smartSelectionDefaults.ts`), so *every* plain double-clicked word is
also technically a "smart match" under the hood, and dragging one across
several words must keep pulling in whole words the way plain `"word"`-mode
dragging always did — an early version of this fix used the raw drag point
for that extension and broke that exact case (`canvasTerminalGestures.pin
.test.ts`'s pre-existing "dragging after a double-click" tests caught it).

If you add a fourth drag-extension mode, check whether its live boundary can
ever be narrower than what was selected at mousedown — that mismatch, not
row-span (a single-row smart match hit this identically to a multi-row one),
is the actual failure condition.


## Canvas-Terminal Frame Protocol Bit Layout

The bit layout is defined **once**, in `terminal_grid.rs`'s `serialize_dirty_rows` doc
comment (backend). Any bit added there MUST be mirrored in
`canvasTerminalUtils.ts::decodeBinaryFrame` (this directory) AND in
`docs/frontend/canvas-terminal-audit.md`'s bit-layout paragraph, not just the two code
sites — three separate bugs (the DECCKM `app_cursor` bit, the DECSCUSR `cursor_steady`
bit, and a stale scrollbar-visibility condition next to them) have traced back to that
three-way split staying out of sync. Full backend-side rationale:
`src-tauri/AGENTS.md`.

## Command Blocks — Frontend Consumer Rules

Full mechanism (why a block's `line` can be a meaningless alt-screen cursor row, or
alias past scrollback-ring eviction): `src-tauri/AGENTS.md`'s "Command Blocks" section.
Frontend-side rules that follow from it:

- **Every row-anchored block consumer must read blocks through `rowAnchoredBlocks()`**
  (`src/stores/terminals.ts`), which drops `onAltScreen: true` entries, rather than
  rendering a mark at a meaningless row.
- **`CommandOverview.tsx`'s `getCommandText` falls back to slicing the grid**
  (`ref.getBufferLines(commandLine, executionLine)`) whenever `promptText` is null — a
  real shell OSC133 block with no hook-driven prompt text. That fallback DOES depend on
  row validity, so it must check `block.onAltScreen` too and return `""` rather than
  read a row that no longer means what it once did.
- A `CommandBlock`'s stored `line` is eviction-stable (`Grid::total_scrolled()`), not a
  live viewport/buffer-line row — convert with `evictionStableToGridRelative`
  (`canvasTerminalUtils.ts`) before using it to index into the live grid. See the
  `CommandBlock` doc comment in `terminals.ts`.

## Terminal Keydown vs. Global Shortcuts

`keyToSequence()` (`terminalInput.ts`) excludes `metaKey` but not `ctrlKey` from its
printable-character PTY-forwarding fallback — so a global shortcut whose Windows/Linux
form uses Ctrl+&lt;printable&gt; (macOS form: Cmd+&lt;printable&gt;) will be silently
swallowed and typed into the terminal instead of bubbling to the document-level
shortcut listener, unless the terminal's keydown handler explicitly bails out first
(see `isGlobalShortcutPassthrough`, same file). When adding or rebinding a global
shortcut that uses Ctrl/Cmd + a printable key, verify the Windows/Linux (Ctrl) form is
special-cased the same way.
