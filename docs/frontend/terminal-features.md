# Terminal Features & Keyboard Shortcuts

Consolidated reference for all terminal behaviors, keyboard shortcuts, and configurable features.

## Keyboard Shortcuts

### Terminal Management

| Shortcut | Action | Notes |
|----------|--------|-------|
| Cmd+T | New terminal tab | |
| Cmd+W | Close tab/pane | Closes active split pane, or tab if no split |
| Cmd+Shift+T | Reopen closed tab | Restores last 10 closed tabs |
| Cmd+1–9 | Switch to tab N | First 9 tabs only |
| Ctrl+Tab | Next tab | NSEvent monitor on macOS |
| Ctrl+Shift+Tab | Previous tab | NSEvent monitor on macOS |

### Terminal Content

| Shortcut | Action | Notes |
|----------|--------|-------|
| Cmd+L | Clear terminal | Sends Ctrl+L to shell (clear screen) |
| Cmd+K | Clear scrollback | Clears entire scrollback buffer (iTerm2 convention) |
| Cmd+C | Copy selection | |
| Cmd+V | Paste | |
| Cmd+F | Find in terminal | Search overlay with match highlighting |
| Cmd+G | Find next match | |
| Shift+Cmd+G | Find previous match | |

### Scrolling

| Shortcut | Action |
|----------|--------|
| Cmd+Home | Scroll to top of scrollback |
| Cmd+End | Scroll to bottom |
| Shift+PageUp | Scroll one page up |
| Shift+PageDown | Scroll one page down |
| Wheel / two-finger | Scroll the scrollback (smooth) |
| Shift+Wheel | Force scrollback scroll, never sent to the app |

**Wheel vs. mouse-reporting apps.** When an app enables mouse tracking, the wheel
is forwarded to it as SGR mouse codes whenever mouse reporting is on, in **either**
buffer — the alternate screen (vim, lazygit, htop) and inline fullscreen TUIs in the
main buffer alike (e.g. `grok --no-alt-screen`, or Claude Code, which enables mouse
tracking without switching to the alt screen). Hold **Shift** to force scrollback
scrolling regardless of mouse mode (consistent with Shift bypassing mouse reporting
for clicks/selection). The scrollbar thumb always moves TUIC history.

**Quantization.** A physical wheel notch or trackpad flick delivers a burst of many
small-delta DOM wheel events (macOS reports the whole momentum decay, not just the
initial motion). Both the forwarded and the scrollback path accumulate those pixel
deltas and only act once a full line height has been crossed — one SGR notch, or one
line of scrollback, per line height — capped at 3 notches per event to match native
terminal behavior for a single discrete mouse click. `deltaMode` (line/page-reporting
devices) is normalized the same way, so non-pixel-reporting mice and touchpads track
correctly too.

### Split Panes

| Shortcut | Action | Notes |
|----------|--------|-------|
| Cmd+\ | Split vertically | Side-by-side, max 4 panes |
| Cmd+Alt+\ | Split horizontally | Stacked |
| Cmd+Shift+Enter | Maximize/restore pane | Toggle zoom on active pane |
| Alt+Arrow Left/Right | Navigate vertical panes | |
| Alt+Arrow Up/Down | Navigate horizontal panes | |

### Panels

| Shortcut | Action |
|----------|--------|
| Cmd+[ | Toggle sidebar |
| Cmd+, | Settings |
| Cmd+E | File browser |
| Cmd+Shift+M | Markdown panel |
| Cmd+Alt+N | Notes/ideas panel |
| Cmd+O | Open file picker |
| Cmd+N | New file (picker for name + location) |
| Cmd+J | Task queue |
| Cmd+B | Quick branch switch |
| Cmd+G | Branches tab |
| Cmd+Shift+D | Git operations panel |
| Cmd+Shift+E | Error log |
| Cmd+Shift+A | Activity dashboard |
| Cmd+Shift+W | Worktree manager |
| Cmd+Shift+M | MCP servers popup |
| Cmd+Shift+G | Diff scroll view |
| Cmd+? | Help panel |

### Navigation

| Shortcut | Action |
|----------|--------|
| Cmd+P | Command palette |
| Cmd+Shift+K | Prompt library |
| Cmd+R | Run saved command |
| Cmd+Shift+R | Edit saved command |
| Cmd+Shift+F | Search file contents |
| Cmd+Ctrl+1–9 | Quick branch switch (hold Cmd+Ctrl, press number) |

### Zoom

| Shortcut | Action |
|----------|--------|
| Cmd+= / Cmd++ | Zoom in |
| Cmd+- | Zoom out |
| Cmd+0 | Reset zoom |

## Terminal Behaviors

### Copy on Select
Auto-copies selected text to clipboard when text is selected in terminal. Configurable in settings (`copy_on_select`, default: on).

### Link Activation
How a detected link (URL, file path, `file://`, or OSC 8 hyperlink) opens is configurable via the **Open links on** Terminal setting (`terminal_link_activation`, default: `click`):
- **Click** — a plain click opens the link.
- **⌘Click / Ctrl+Click** — the underline is hidden until Cmd (macOS) or Ctrl (Windows/Linux) is held; holding it reveals the underline and pointer cursor, and modifier+click opens the link.
- **Never** — click never opens a link; right-click's Open/Copy-link menu is the only way to activate one.

### OSC 1337 iTerm2 Commands
Backend-parsed (see `docs/backend/alacritty-integration.md`'s OSC 1337 section for the full per-command table); the frontend's role is limited to three of them:
- **`StealFocus`/`RequestAttention`** — no frontend involvement (`pty.rs` calls the Tauri window APIs directly), gated by `osc1337_focus_attention` (default: on).
- **`OpenURL`** — the backend raises a confirm request over the same wire `ui action=confirm` MCP requests use; `McpConfirmHost.tsx` renders it with no OSC-1337-specific frontend code. Only once confirmed does the backend emit `pty-open-url`, picked up globally by `PtyOpenUrlHost`/`stores/ptyOpenUrl.ts` → `handleOpenUrl` (the same allowlisted opener as URL Click, below).
- **`Copy`/`CopyToClipboard`+`EndCopy`** — reuse the existing OSC 52 `pty-clipboard-store-{session_id}` event and `handleOsc52ClipboardStore` path; no separate frontend handling.

### Smart Selection
Double-click word selection is configurable and rule-driven, mirroring iTerm2's Smart Selection. Configurable via the **Smart Selection** settings tab (see [Settings — Smart Selection Tab](../user-guide/settings.md#smart-selection-tab)).

**Word boundaries** — what a double-click expands to when no smart rule matches (`word_selection_mode`, default `characters`):
- **Character list** (`word_separators`) — a literal set of characters that break a word. Whitespace/control characters are always separators regardless of this list.
- **Regular expression** (`word_selection_regex`) — `|`-joined alternates; the longest match at each position joins onto the adjacent alnum/underscore run (e.g. adding `https://` lets a double-click on a URL's host include the scheme).

**Smart selection rules** (`smart_selection_rules`, default: the built-in set) — each rule has a regex and a precision class (`very_low`…`very_high`, weighted `0.00001`…`1000000`). At the click position, every enabled rule's matches spanning it are scored `precision × matchLength`; the highest score wins (`findSmartMatch` in `smartSelection.ts`, ported from iTerm2's `iTermTextExtractor.m`). `double_click_action` (`word`/`smart`, default `smart`) decides whether a plain double-click tries the rule engine first; quad-click (4 rapid clicks) always does, regardless of that setting. The scanned text window spans `SMART_SELECTION_RADIUS` (2) rows above/below the click, joined across soft-wrapped rows.

An empty `smart_selection_rules` means "use the built-in default set" (`DEFAULT_SMART_SELECTION_RULES` in `smartSelectionDefaults.ts`): iTerm2's ten built-in rules (word, C++ `namespace::identifier`, path, quoted string, Java/Python include path, `mailto:` URL, Objective-C selector, email address, HTTP URL, SSH URL, Telnet URL) plus dev-terminal extras (git commit SHA, `file:line:col`, semver, IPv4, IPv6, UUID, issue key like `ABC-123`, and `#NNN` issue reference — the last disabled by default as too generic).

**Rule actions** — a rule may offer actions (Copy, Open URL, Open File, Send Text, Run Command, Run Command in New Terminal, Ask AI), each with a parameter template supporting `\0`-`\9` (match/capture groups), `\d`/`\u`/`\h` (cwd/user/host) substitution (`substituteActionParameter`). Actions surface in the right-click context menu when the click lands on a match (link-detection's own Open/Copy-link pair takes priority when both apply to the same span, to avoid duplicate menu items). When a rule's actions are shown, the matched rule's name appears above them as a non-interactive header (`ContextMenuItem.header`) so it's clear which rule fired — omitted for a rule left with a blank name. At most one action per rule may be marked default — Option/Alt+double-click runs it directly, in addition to selecting the match. `run_command`'s auto-submit is gated by `shouldAutoSubmitSuggestion` (same metacharacter-safety heuristic as OSC 7770 suggestion chips); `send_text` never auto-submits.

**Export/Import** (`smartSelectionExport.ts`) — the Smart Selection tab's toolbar exports the
effective rule set (`all`/`modified`/`custom`, same scope semantics as Smart Prompts) to a
`tuicommander-smart-selection-rules`-kinded JSON file via the native Save dialog, and imports one
through a NEW/CONFLICT review dialog (`RuleImportDialog`, built on the shared
`ImportReviewDialog` — see `docs/user-guide/smart-prompts.md`'s Import & Export section for the
UI this mirrors). Built-in/custom/modified is derived from `DEFAULT_SMART_SELECTION_RULES` id
membership rather than a stored flag; `differsFromDefaultRule`'s `actions` comparison is
positional (order is the menu order and decides which action is default), not set-like. A rule
carrying a `run_command`, `run_command_new_terminal`, or `send_text` action always imports with
`enabled: false`, mirroring how a `shell`/`api` Smart Prompt imports disabled. Because the
stored `smart_selection_rules` is `[]` until the user customizes something (meaning "use the
defaults"), merging an import always writes back the full effective list — importing even one
custom rule permanently materializes every built-in rule into `config.json`, same as editing a
single rule already does; "Restore built-in defaults" remains the escape hatch.

### URL Click
URLs in terminal output open in the system browser (via the allowlisted `openUrl` helper — `http`/`https`/`mailto` only). URL detection is regex-based.

### File Path Click
Clickable file paths in terminal output (absolute and relative paths with known extensions, plus `file://` URLs) open in the in-app editor or markdown viewer. Verified against the filesystem before being underlined.

### Tab Features
- **Middle-click** closes tab
- **Right-click** context menu: Close, Close Other, Close Right, Rename, Detach to Window, Move to Worktree, Pin/Unpin, Copy Path
- **Drag-and-drop** to reorder tabs
- **Double-click** tab title to rename
- **Unseen activity badge** when tab has new output

### Split Panes
- Max 4 panes per branch
- Drag divider to resize
- Flexible ratios preserved across layout changes
- Modes: "separate" (independent tab bars) or "unified" (shared tab bar)

## Inline Images (color-tools plan, Phase 5)

iTerm2's OSC 1337 inline-image protocol and the Kitty graphics protocol are
both parsed and rendered end to end — see `docs/backend/pty.md`'s "iTerm2
Inline Images" and "Kitty Graphics Protocol" sections for the wire-protocol
scope. This section covers the frontend half: how a reserved placement
actually gets pixels on screen.

- **A dedicated canvas layer** (`imageCanvasRef`/`ictx` in `CanvasTerminal.tsx`)
  sits between the glyph/background canvas and the cursor/selection overlay —
  an image occludes text underneath it, but never the cursor or a selection
  highlight drawn on top. Sized and dpr-scaled identically to the overlay
  canvas, repainted from `repaintOverlay` (so every existing overlay-repaint
  trigger — resize, scroll, frame update, hover — refreshes images too, with
  no new call sites to remember).
- **`imageLayer.ts`** (`ImageLayer` class) owns two pieces of per-terminal
  state: the current placement set (`Map<placementId, ImagePlacement>`) and a
  decoded-bitmap cache keyed by image id, so multiple placements of the same
  image share one decode.
  - **Coordinate math**: a placement's `absRow` (eviction-stable — see
    `alacritty_terminal::event::ImagePlacementInfo`'s doc comment) converts to
    an on-screen row via `absRow - frame.historyBase - frame.historySize +
    frame.displayOffset` — the exact formula `DecodedFrame.historyBase`'s own
    doc comment already documents for the scroll row cache. A placement whose
    row range is entirely outside `[0, frame.screenRows)` is skipped without
    even starting a bitmap fetch.
  - **Decode**: bytes come from the existing `terminal_image_bytes` command
    (unchanged), wrapped in a `Blob` and handed to `createImageBitmap` — which
    only decodes a real image container (PNG/GIF/JPEG). Kitty's raw
    `f=24`/`f=32` formats (no container; mpv/blackcat) fail to decode and are
    marked errored (never retried, logged nowhere loud since this is a known,
    documented gap, not a bug) — see `to-test.md`.
- **Transport — a new event pair, not a new binary-frame bit.** The existing
  grid-frame binary header has zero free bits (`canvasTerminalUtils.ts`'s
  per-cell attrs byte and header flag bytes are fully packed), so placements
  ride the same JSON-event mechanism as `cwd`/`osc133`: `image-placement`
  (one new/updated placement) and `image-placements-cleared` (alt-screen
  switch, Kitty `a=d,d=A`/`a=d,d=I` — see `mcp_http/session.rs`'s
  `grid_ws_frame` for the WS JSON shape and the desktop `pty-image-placement-*`
  Tauri event for the IPC twin; both carry identical field names).
- **Reconnect/new-client hydration**: the live event stream only carries
  placements created *after* a listener attaches, so `ImageLayer.hydrate()`
  calls the `terminal_image_placements` command (Tauri + HTTP, identical
  `[placementId, imageId, absRow, col, rows, cols, zIndex]` tuple-array
  shape) once after the initial grid subscribe and again on every
  `resubscribe()` — verified against a live `make dev` instance: a full page
  reload still shows a previously-displayed image with no live event
  involved.
- **Unicode virtual placeholders** (`U=1`; color-tools plan, Phase 7 —
  image.nvim, snacks.nvim, yazi's modern driver) need no frontend changes at
  all: `image_placements()` groups cells by `placement_id` regardless of
  which backend path attached them (terminal-reserved-footprint or a
  placeholder character's own resolved id), so this renderer already covers
  them once the backend attaches the `CellExtra.image` ref.
- **Known, deliberately-scoped gaps** (each documented, not silent):
  1. **Z-index**: Kitty's `z<0` ("paint below text") placements render in
     this same single above-text layer, so they'll visually sit on top of
     text rather than behind it. Correct three-band compositing (per the
     original design) needs splitting `gridRenderer.ts`'s fused
     background+glyph paint into two separately-paintable passes so an image
     layer can be sandwiched between them — real, separate follow-up work.
  2. **Raw pixel formats**: see the Decode bullet above.
  3. A placement's cells being overwritten by ordinary text (rather than an
     explicit Kitty `a=d` or an alt-screen switch, both of which do notify
     the renderer) does not proactively clear the image from this canvas
     layer — there is no spare bit in the grid frame to signal "this cell's
     image ref was just dropped" without adding a new per-cell wire field.

## Configurable Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `copy_on_select` | `true` | Auto-copy terminal selection to clipboard |
| `confirm_before_quit` | `true` | Show dialog when quitting with active terminals |
| `confirm_before_closing_tab` | `true` | Show dialog when closing tab with running process |
| `split_tab_mode` | `"separate"` | Tab bar mode for split panes |
| `tab_ordering_mode` | `"grouped-by-type"` | Tab ordering: "grouped-by-type", "terminals-first", "free" |
| `intent_tab_title` | `true` | Show agent intent as tab title |
| `suggest_followups` | `true` | Show suggested follow-up actions from agents |
| `bell_style` | `"visual"` | Terminal bell: "none", "visual", "sound", "both" |
| `prevent_sleep_when_busy` | `false` | Prevent system sleep while terminal is busy |
