# Terminal Features & Keyboard Shortcuts

On desktop, returning from Chat to CLI focuses the terminal input. You can type immediately after using the switch, closing Chat Compose, answering in CLI, or falling back from unavailable Chat. Only the active terminal restores focus, and an automatic fallback does not take the caret from an editable field outside that terminal.

Session Chat shows a pulsing **Working…** row after Send and while the terminal reports busy. It disappears when the terminal reports idle or awaits a question or permission answer. Desktop and mobile use the existing session state; Chat does not infer activity from the conversation text.

Consolidated reference for all terminal behaviors, keyboard shortcuts, and configurable features.

The desktop CLI | Chat switch uses shared toolbar density. In both modes it is centred on the Context header and stays above its bottom divider, without adding a separate row. Without Context, both modes reserve one shared control row so content cannot run beneath the switch. Switching modes keeps the switch and content at the same height. At touch widths up to 600px, the Context header and collapsed bar reserve a 48px row for the shared 44px switch; expanded Context keeps the same header clearance.

## Mobile composer focus

The composer border highlights only while its textarea has focus. Tapping the mic releases editor focus without changing the insertion range. Recording uses the red mic icon, not an extra accent border. Keyboard navigation retains the mic focus ring. The blue slash key marks an available action, not retained focus.

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
| Cmd+K | Clear scrollback | Clears entire scrollback buffer (common terminal convention) |
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

**Quantization (forwarded to an app).** A physical wheel notch or trackpad flick
delivers a burst of many small-delta DOM wheel events (macOS reports the whole
momentum decay, not just the initial motion). The forwarded path accumulates those
pixel deltas and emits one SGR notch per full line height crossed, capped at 3 notches
per event in pixel mode (to match native terminal behavior for a single discrete mouse
click); the residual resets on direction reversal and when the gesture goes idle
(200 ms). Line-mode devices (`deltaMode` 1) are rounded to whole lines and clamped to
the viewport row count; page-mode devices (`deltaMode` 2) emit `viewportRows - 1`
notches per event.

**Scrollback scrolling.** The scrollback path (no mouse reporting, or Shift held) is
not quantized: pixel deltas, normalized from line/page mode, drive smooth fractional
scrolling. Gesture acceleration ramps from 0.5x (first screenful of travel) to 1.0x
(after 2 screens) and is capped at 2.0x (after 4 screens); it resets on direction
reversal and when the gesture ends.

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
| Cmd+Shift+R | Run saved command |
| Cmd+R | Reload active web/preview tab |
| Cmd+Shift+F | Search file contents |
| Cmd+Ctrl+1–9 | Quick branch switch (hold Cmd+Ctrl, press number); in the Global Workspace it switches to tab N |

### Zoom

Desktop terminal Chat and AI Chat use the shared scale in `styles/chat-tokens.css`: 14px plus the terminal font-size delta from the configured default, bounded to 8–32px. Terminal Chat reads its own terminal; AI Chat reads the active terminal and uses zero delta when none is active. Reset restores the 14px reading size. Mobile retains its device-local reading preference.

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

### OSC 1337 Commands
Backend-parsed (see `docs/backend/alacritty-integration.md`'s OSC 1337 section for the full per-command table); the frontend's role is limited to three of them:
- **`StealFocus`/`RequestAttention`** — no frontend involvement (`pty.rs` calls the Tauri window APIs directly), gated by `osc1337_focus_attention` (default: on).
- **`OpenURL`** — the backend raises a confirm request over the same wire `ui action=confirm` MCP requests use; `McpConfirmHost.tsx` renders it with no OSC-1337-specific frontend code. Only once confirmed does the backend emit `pty-open-url`, picked up globally by `PtyOpenUrlHost`/`stores/ptyOpenUrl.ts` → `handleOpenUrl` (the same allowlisted opener as URL Click, below).
- **`Copy`/`CopyToClipboard`+`EndCopy`** — reuse the existing OSC 52 `pty-clipboard-store-{session_id}` event and `handleOsc52ClipboardStore` path; no separate frontend handling.

### Smart Selection
Double-click word selection is configurable and rule-driven, mirroring iTerm2's Smart Selection. Configurable via the **Smart Selection** settings tab (see [Settings — Smart Selection](../user-guide/settings.md#smart-selection)).

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
Clickable file paths in terminal output resolve against the terminal working directory when relative. Sentence periods are excluded from heuristic path targets, including paths in soft-wrapped lines. Existing files open in their viewer or editor; directories open in the File Browser. `file://` URLs are accepted too, and every path is verified against the filesystem before it is underlined.

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

### Collapsed Compose queue count

The CLI Compose button shows a muted count pill for all pending FIFO entries,
including user commands and TUIC messages. It hides at zero and follows the
backend session state without opening the editor. Hover or keyboard focus loads
the command/message breakdown; the count remains available if details fail.

### Compose editor size

Terminal Compose uses the shared auto-grow control bounds: one control line when empty (36px desktop, 44px touch), intrinsic CodeMirror growth for newlines and wrapping, and the existing 96px editor maximum. Longer drafts scroll inside the editor. Clearing or successfully submitting a draft restores the minimum. The queue list keeps its independent height cap.

### Compose editor shortcuts

The newline legend shows only Ctrl+Enter. A panel-relative CSS container query hides shortcut hints at widths up to 640px, including split terminals inside a wide window. The footer does not wrap its hints around the action controls.

In the terminal Compose panel, Enter sends the draft. Ctrl+Enter and Shift+Enter insert a new line. Shift+Ctrl+Enter queues for the next idle moment when an agent is detected. Enter used to confirm an IME candidate does not submit the draft. These shortcuts do not change native terminal keybar controls.

## Mobile stream recovery

Mobile CLI output releases a terminal subscription when the transport reports exit, including retry exhaustion. Visibility resume starts a fresh subscription if none remains and the session poll has not confirmed the session ended. Retired stream callbacks cannot close a newer subscription or change its connection error state. Handshake retries retain their three-second initial delay and 30-second backoff cap; hidden pages do not start a scheduled retry.

### Unsized pane diagnostics

Canvas sizing retries run only for a visible pane connected to the layout. A pane
that stays unsized for 120 animation frames logs a warning. Hiding or closing the
pane cancels the retry; showing it starts a new measurement.

Restored PTY sessions attach their event listeners before the first layout pass.
The reconnect resize waits for that attachment and reuses successful listeners
instead of adding another API-error consumer. If the initial handshake failed,
reconnect retries the attachment once. Suspend detaches them; resume attaches the replacement PTY once.
