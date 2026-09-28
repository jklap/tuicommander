# Components

All components are SolidJS functional components in `src/components/`.

## Component Tree

```
App.tsx (central orchestrator)
├── Toolbar/                  # Window drag region, repo/branch display
├── Sidebar/                  # Repository tree with branches
│   ├── GroupSection          # Collapsible repo group with color + drag-reorder
│   ├── RepoSection           # Single repo entry with branches
│   ├── ParkedReposPopover    # Popover to recall parked (hidden) repos
│   ├── CiRing               # CI status ring per branch
│   ├── StatusBadge           # Git status badge (clean/dirty/conflict)
│   └── PrDetailPopover/      # PR details popup (CI, reviews, labels)
├── main
│   ├── TabBar/               # Terminal tabs with drag-to-reorder
│   ├── Terminal/             # Native terminal renderer (never unmounted)
│   ├── TerminalArea/         # Terminal + split pane layout (up to 6 panes)
│   ├── SuggestOverlay/       # suggest: follow-up action chips
│   ├── GitPanel/             # Git panel (6 tabs)
│   │   ├── ChangesTab        # Staged/unstaged file list with stage/unstage/discard
│   │   ├── LogTab            # Commit log with expandable diffs
│   │   ├── StashesTab        # Stash list with apply/pop/drop/show
│   │   ├── BranchesTab       # Branch CRUD, prefix folding, search, checkout
│   │   ├── BlameTab          # Line-by-line git blame viewer
│   │   ├── HistoryTab        # Per-file commit history
│   │   ├── CommitGraph       # Visual commit graph with lane assignments
│   │   └── SyncRow           # Push/pull/fetch action bar
│   ├── DiffTab/              # Individual file diff tab (with Cmd+F search)
│   │   └── BranchDiffScrollView  # All-files scroll view (scroll mode)
│   ├── PrDiffTab/            # PR diff viewer tab
│   ├── CodeEditorPanel/      # CodeMirror 6 code editor tab
│   ├── MarkdownPanel/        # Markdown file browser
│   │   └── ContentRenderer  # Markdown to HTML (DOMPurify), interactive checkboxes, tweak highlights
│   ├── HtmlPreviewTab/       # Multi-format preview tab (HTML, PDF, images, video, audio, text)
│   ├── MarkdownTab/          # Markdown tab (checkboxes, tweak comments, queued agent review, search)
│   ├── IdeasPanel/           # Ideas panel with edit, send, delete
│   ├── FileBrowserPanel/     # File tree browser with content search
│   │   └── TreeNode          # Recursive tree node (lazy-loaded)
│   ├── PluginPanel/          # Plugin HTML panel (sandboxed iframe)
│   ├── ClaudeUsageDashboard/ # Claude API usage dashboard (SolidJS)
│   ├── CodexUsageDashboard/  # Codex App Server usage dashboard
│   ├── GrokUsageDashboard/   # Grok ACP billing dashboard
│   ├── ErrorLogPanel/        # Application error log viewer
│   └── StatusBar/            # Status messages, agent badge, toggles
│       └── ZoomIndicator     # Font size display
├── TabBar/                   # Ordering, overflow, drag/drop, and menus
│   └── TabViews              # Shared terminal, diff, Markdown, and editor tab views
├── SettingsPanel/            # Settings overlay; nav grouped by task (GLOBAL_TAB_GROUPS)
│   ├── ExpertSetting         # ExpertSetting / ExpertSection / ExpertModeSwitch
│   ├── DictationSettings     # Voice page
│   ├── tabs/GeneralTab       # Language, context bar, confirmations, power management, updates, experimental, TUIC CLI, Code Intelligence, ego executable, IDE, custom launchers
│   ├── tabs/AppearanceTab    # Tabs, repository groups, layout, UI legend
│   ├── tabs/NotificationsTab # Sound and notification prefs
│   ├── tabs/TerminalTab      # Theme, shell, font, cursor, clipboard, blocks
│   ├── tabs/KeyboardShortcutsTab # Rebindable keyboard shortcuts, global hotkey
│   ├── tabs/GitHubTab        # Git & GitHub: OAuth login, PR/issue display, repo + worktree defaults
│   ├── tabs/AgentsTab        # Agent detection, run configs, Claude Usage toggle
│   ├── tabs/AiChatTab        # default model, providers
│   ├── tabs/SmartPromptsTab  # Smart Prompts library
│   ├── tabs/RemoteMachinesTab # Remote Machines page (wraps services/RemoteMachinesPanel)
│   ├── tabs/PluginsTab       # Plugin management, logs
│   ├── tabs/services/        # LocalMcpPanel + UpstreamMcpPanel (MCP page), RemoteAccessPanel, RemoteMachinesPanel
│   ├── tabs/RepoScriptsTab   # Per-repo scripts
│   └── tabs/RepoWorktreeTab  # Per-repo worktree options
├── HelpPanel/                # Keyboard shortcuts documentation
├── TaskQueuePanel/           # Agent task queue
├── PromptOverlay/            # Agent prompt interception
├── PromptDrawer/             # Prompt library management
├── CommandPalette/           # Cmd+P / browser-toolbar palette with transport-safe actions
├── ActivityDashboard/        # Compact inline or detached terminal activity list
├── BranchSwitcher/           # Quick branch switcher (held-key overlay)
├── BranchPopover/            # Branch selection popover
├── TipOfTheDay/              # Startup tip notification
├── DictationToast/           # Dictation recording/transcribing indicator
├── ConfirmDialog/            # Reusable in-app confirmation dialog
├── RenameBranchDialog/       # Branch rename dialog
├── CreateWorktreeDialog/     # Worktree creation dialog
├── PostMergeCleanupDialog/   # Post-merge cleanup (switch base, pull, delete)
├── PromptDialog/             # Text input prompt dialog
├── RunCommandDialog/         # Configure terminal commands
├── WorktreeManager/          # Overlay panel for worktree management
├── MergePostActionDialog/    # Dialog for post-merge actions (keep/delete branch)
├── ContextMenu/              # Shared right-click menu (all panels). `separator:true` on an item = trailing divider AFTER it; empty-label item = standalone divider; a trailing separator on the LAST item is suppressed
└── IdeLauncher/              # Open repository in IDE
```

## Application Controllers

Application lifecycles live in focused hooks under `src/hooks/`; `App.tsx`
composes them and owns the top-level layout. Git operations retain the
`useGitOperations` facade for callers, with stateful domains implemented under
`src/hooks/git/`:

- repository refresh and stale-result suppression;
- serialized branch selection;
- terminal/worktree ownership and OSC 7 reassignment;
- worktree creation, setup, recovery, and removal;
- merge, autofix, and conflict-assistance workflows.

Each coordinator owns its timers, queues, generations, or locks. These are
behavioral boundaries rather than generic service wrappers.

`PanelOrchestrator` loads the AI Chat panel after its first inline opening and
keeps it mounted when hidden. The detached AI Chat adapter loads the same panel
when its window opens. Neither path loads its markdown renderer before the
desktop terminal view.
`SessionControls` lists ego's durable `session/list` results by `updatedAt` and
loads a picked session through ACP. `useAcpChat` restores the saved root-to-session
binding from app config after a fresh document opens.
ACP session title updates rename the panel header and picker entry. The usage
footer shows context-window occupancy and the reported cumulative cost.
Untitled sessions use their first prompt or latest activity time in the picker,
with the session ID in the option tooltip. Small single-choice elicitation
forms use direct answer buttons and Cancel. Prompt failures and empty completed
turns appear in the transcript.
The status-bar AI Chat toggle uses the shared `CountBadge` to show pending ACP
questions while the panel is hidden.
The composer stages pasted images in its shared draft and sends ACP image
blocks through `acpClient.prompt`. It checks `promptImage` and the 10 MiB cap
before reading clipboard bytes; each preview can be removed before sending.
During a turn it offers **Queue** beside **Stop**, lists the host-owned queued
prompts, and can remove any queued ID. The list follows ACP snapshots and
events, so another window or a phone sees the same order and cancellations.

## Mobile Screens (`src/mobile/`)

`MobileApp` keeps `SessionDetailScreen` mounted while its header opens the shared
`FilesScreen` at the session's worktree or containing registered repository.
The session's output stream and command draft stay alive while Files is shown;
the regular Files bottom tab still starts at the repository picker.

## Core Components

### PluginPanel (`PluginPanel/`)

URL tabs and inline plugin panels mount their sandboxed iframe only while their tab is visible. Hiding a tab, switching repositories or pane tabs, or covering split panes with an orphan tab removes the iframe from the DOM. Showing it again loads the page anew, so iframe scroll, focus and JavaScript state do not persist. This prevents hidden page timers from blocking terminal input on the shared WebContent main thread. Native `tuic://edit` and `tuic://open` tabs use their own components and are unaffected.

### Terminal (`Terminal/`)

Native terminal renderer with full PTY integration.

**Responsibilities:**
- Creates and manages CanvasTerminal instance backed by `alacritty_terminal`
- Renders grid frames to HTML canvas for GPU-accelerated display
- Subscribes to PTY output events
- Handles terminal resize (with debouncing)
- Applies font, theme, and zoom settings
- Link detection for clickable URLs
- Selection management for copy operations

`CanvasTerminal` keeps frame decode, reconciliation, scheduling, and paint in
one imperative hot path. Sibling controllers own selection/search state, link
verification cancellation and caches, fractional scroll/cache handoff, and DOM
input-listener cleanup. These controllers do not use reactive state.

**Key behavior:** Terminals are **never unmounted** — they stay in the DOM when switching tabs. Only visibility is toggled. This preserves terminal state (scroll position, content, active processes).

### Sidebar (`Sidebar/`)

Repository tree with branch management.

**Features:**
- Expandable/collapsible repository entries
- Icon-only collapsed mode
- Branch list with active branch highlight
- CI ring indicator per branch (from githubStore)
- PR status badge
- Compact diff stats (additions/deletions) with exact tooltip counts
- Workspace lifecycle badge from the backend (`Dirty`, `Merged`, or `Unknown`), plus a compact unmerged-commits mark,
  keyed by workspace id; `Dirty` and `Unknown` expose their meaning and removal
  consequence in a WebView-compatible hover/focus tooltip
- Context menu (right-click) for repo/branch operations
- Resizable width via drag handle (200-500px)
- Keyboard redirect to active terminal

Shared PR presentation (`PrStateBadge`) and merge eligibility are leaf modules
below the sidebar views. `RepoSection`, `GitHubPanel`, `PrSection`, and
`RemoteOnlyPrPopover` do not import back through one another.

### TabBar (`TabBar/`)

Terminal tab management.

**Features:**
- Tabs filtered to active branch only
- Drag-to-reorder tabs
- Tab rename (double-click)
- Close button per tab
- Activity indicator (dot) for background terminals
- Awaiting input indicator (question/error icons)
- Context menu: Close, Close Others, Close to Right
- Context menu, debug builds only (`isPerfDebug()`): **Capture Session** — arms the raw PTY
  capture tap on that session so a state-detection bug can be recorded as it happens. See
  `docs/frontend/utilities.md` → `ptyCapture.ts`.

### PromptDrawer (`PromptDrawer/`)

Creates, edits, and executes custom and built-in Smart Prompts. A normal click or
keyboard Enter follows the prompt's `autoExecute` value. Double-click and
**Insert & Run** explicitly submit once; **Insert** explicitly keeps the resolved
text editable. The drawer delays a single pointer click until the double-click
window closes so one gesture cannot trigger both delivery paths.

### SettingsPanel (`SettingsPanel/`)

Settings overlay. The nav groups the global pages by task
(`GLOBAL_TAB_GROUPS` in `SettingsPanel.tsx`); each group renders as a static
label row above its pages, and the configured repositories follow under
**Repositories**.

**Pages:**
- **Application**
  - **General** (`GeneralTab`) — Language, agent context bar, confirmations, power management, updates, Experimental Features, TUIC CLI, Code Intelligence, ego executable (always shown), default IDE, custom launchers
  - **Appearance** (`AppearanceTab`) — Tabs, repository groups, layout reset, UI legend
  - **Notifications** (`NotificationsTab`) — Sound and notification preferences
- **Workspace**
  - **Terminal** (`TerminalTab`) — Theme, shell, font, font size and weight, cursor style, clipboard, command blocks, scrollback reflow
  - **Keyboard Shortcuts** (`KeyboardShortcutsTab`) — Rebindable shortcuts (auto-populated from `actionRegistry.ts`) and the global hotkey. The Help panel reuses the same editor
  - **Git & GitHub** (`GitHubTab`) — GitHub OAuth login (Device Flow), token management, diagnostics, PR and issue display, repository and worktree defaults, additional accounts, repository bindings
- **AI**
  - **Agents** (`AgentsTab`) — Agent detection, run configurations, Claude Usage toggle
  - **AI Chat** (`AiChatTab`) — default model, provider list. Hidden while `isAiChatEnabled()` is false. The ego executable is on General
  - **Voice** (`DictationSettings`) — see below
  - **Smart Prompts** (`SmartPromptsTab`)
- **Integrations**
  - **MCP** (`LocalMcpPanel` + `UpstreamMcpPanel`) — HTTP API server status, TUIC tools, upstream MCP servers
  - **Remote Access** (`RemoteAccessPanel`) — Remote access, Tailscale HTTPS, QR/connect URL, cloud relay
  - **Remote Machines** (`RemoteMachinesTab`) — `tuic-remote` connections
  - **Plugins** (`PluginsTab`) — Plugin management, enable/disable, log viewer
- **Repositories**
  - **Repo Scripts** — Setup and run scripts, plus the optional per-repository Dev Server URL for Design Mode
  - **Repo Worktree** — Base branch, copy ignored/untracked files

**Expert mode** (`ExpertSetting.tsx`, `stores/settingsExpert.ts`). The header
carries `ExpertModeSwitch`, which flips the persisted UI pref
`settings_expert_mode`. `<ExpertSetting configKey value>` hides its children in
basic mode while `value` equals the default at `configKey`, as returned by
`get_config_defaults`. It shows them in expert mode, when the value is
modified, when a search result revealed that `configKey` during the current
Settings open, or while the defaults are unknown. All of this is one rule,
`settingsExpertStore.isVisible`.

The rule is sticky for an edited control: a native `input` or `change` event
from any child of an `ExpertSetting` calls `settingsExpertStore.pin(configKey)`,
and a pinned control stays shown for the current open, so setting it back to
the default does not make it disappear under the cursor.
`settingsExpertStore.open()`, which `SettingsPanel` calls each time Settings
opens, clears the pins and the search reveals. Pinning happens on the edit, not
when `isVisible` sees a non-default value: a tab's pre-load placeholder (a value
that differs from the default until its config loads) is not an edit and must
still hide. The listeners sit in the capture phase on the resolved top-level
child elements — no wrapper element, which would break the `.group + .group`
and `:last-child` rules — so the pin lands before the control's own handler
changes the value. A control that changes its value without `input`/`change`
(a button) does not pin.

`<ExpertSection>` hides a section, heading included, when all of its
`ExpertSetting`s are hidden. No page uses it today, because every section keeps
at least one basic control.

The search index marks labels inside an `ExpertSetting` with `expert: true` and
the `configKey`. `SettingsSearch` shows an **Expert** badge on such a result
(`data-expert-badge`). Selecting it calls `settingsExpertStore.reveal(configKey)`
before `SettingsPanel` scrolls to the label, because a hidden control has
nothing to scroll to.

One index entry carries one `configKey`, so one label must not cover two expert
controls. On **Git & GitHub**, **Copy ignored files** and **Copy untracked files**
are therefore two `SettingToggle` rows, each in its own `ExpertSetting`, and not
one group under a shared label.

#### DictationSettings (`SettingsPanel/DictationSettings.tsx`)

The **Voice** page (nav key `dictation`). One `<h3>` per section. Speech-to-text and
text-to-speech are separate sections, and each keeps its own advanced controls
at its bottom — there is deliberately no shared "Advanced" section:
The Rust dictation crate split does not change these controls or their transport calls.

1. **Dictation** — enable, hotkey, long-press threshold, auto-send
2. **Speech recognition** (`SpeechRecognition`) — input device (desktop only),
   Whisper model, language, then voice tuning: the test recording and both
   speech gates
3. **Auto-Corrections**
4. **Hands-free conversation** (`HandsFreeControls`)
5. **Spoken replies** (`SpeechSetup`)

`SpeechRecognition`, `HandsFreeControls` and `SpeechSetup` each open with their
own heading: the settings search index reads source order, so a sub-component
without one would file its labels under the wrong section.

The two voice sections:

- **Spoken replies** (`SpeechSetup`) — the runtime and language downloads from
  `get_speech_assets` (voices excluded), each with Download / Repair / Cancel /
  delete. Then, while the language ships a voice:
  - a voice `<select>` with the ids from `get_speech_voices` (only voices that
    can speak now), and a **Listen** button that previews the selected voice
    through `previewSpeechVoice` and shows a refusal inline;
  - `VoiceLibrary`, the **Voices** list of the language, in three groups:
    **Installed** (downloaded catalogue voices), **Downloadable** (the others,
    with Download and progress, collapsed by default in a `<details>` whose
    `<summary>` shows the count; both reuse `SpeechAssetRow`) and **Yours** (the
    user's voice files, each with a delete button, and **Add voice file…**, a
    hidden `.safetensors` file input; a refused file shows its reason inline);
  - two `SettingSlider`s: **Voice volume** (-30 to -12 dB) and **Levelling**
    (Off to Strong, stored as 0–1). Each shows the value while it is dragged and
    saves only on release (`onCommit`).
  It offers **no language control**: the spoken language is the Whisper
  language, and a second control would be a second source that disagrees with it
  (see `docs/backend/dictation.md` → "The language of the conversation"). A
  voice row is **Active** when it is the selected voice; only a language row is
  active for the language.
- **Hands-free conversation** (`HandsFreeControls`) — a terminal picker, Start /
  Stop, the polled phase, the activation phrase, the hold-back slider, the
  earcons toggle, the "notify model" toggle and, while that is on, the start
  notice textarea. Its placeholder is `getDefaultHandsFreeStartNotice()` — Rust
  owns the built-in text, the frontend keeps no copy.

**Nothing here arms on mount.** Opening this panel must never open the
microphone, and neither must starting the app: `onMount` only starts a 500 ms
status poll, cleared in `onCleanup`. Push-to-talk (the hotkey) and continuous
mode (Start) are rendered as the two distinct mechanisms they are — there is no
"mode" config field behind them.

Since 832-e730 the tab **renders in browser mode** and `HandsFreeControls` still
has no browser branch — this time because none is needed. `armHandsFree` opens
the tab's own audio socket and passes its own owner id, so the same control runs
a conversation on either transport. Two groups stay behind `isTauri()`:

- the **global hotkey** and its long-press slider, which a browser cannot
  register; and
- the **microphone device** list, which enumerates the devices of the machine
  running TUICommander. A browser user picking from that list would be choosing
  hardware in another building — the browser's own device picker is the
  platform's, not ours.

`SpeechSetup` and `HandsFreeControls` are defined at the **bottom** of the file,
after `VoiceTuning`, because `extractSettings` builds the settings search index
from source order — their entries in `settingsSearchIndex.ts` follow in the same
order.

### PrDetailPopover (`PrDetailPopover/`)

Rich PR detail popup shown on hover/click in sidebar.

**Displays:**
- PR title, number, author
- State (open, merged, closed, draft)
- Merge readiness (ready, conflicts, behind, blocked)
- Review decision (approved, changes requested, review required)
- CI check summary (passed/failed/pending ring)
- Individual CI check details
- Labels with computed colors
- Line change counts (+additions/-deletions)
- Timestamps (created, updated)

### StatusBar (`StatusBar/`)

Status messages, agent badge, CWD display, ticker, and panel toggles.

**Layout (left to right):**
1. **ZoomIndicator** — font size display
2. **Status info** — notification text with pendulum ticker for overflow
3. **CWD** — current working directory (click to copy, shortened with `~/`)
4. **Agent badge** — unified agent + usage display (see below)
5. **Ticker** — rotating plugin messages (hidden when absorbed by agent badge)
6. **GitHub badges** — PR badge + CI badge with popover (center area)
7. **Toggle buttons** — Notes (with badge count), File Browser, Markdown, Diff, Dictation mic

**Agent Badge — display priority:**

The agent badge appears when the active terminal has a recognized agent type. It shows a single integrated element with the agent icon and the most relevant info, following this priority cascade:

| Priority | Condition | Display | Example |
|----------|-----------|---------|---------|
| 1 (highest) | PTY rate limit detected | Icon + warning + countdown | `⚠ 3m 20s` |
| 2 | Usage API available (Claude only) | Icon + usage percentages | `5h: 6% · 7d: 69%` |
| 3 | PTY usage limit parsed | Icon + percentage + limit type | `82% daily` |
| 4 (lowest) | No usage data | Icon + agent name | `claude` |

**Data sources:**
- **Rate limit (priority 1):** Detected by Rust output parser via regex on PTY output (e.g. "429", "rate limit", "too many requests"). Stored in `rateLimitStore`. Applies to all agents.
- **Usage API (priority 2):** Polled every 5 min from Claude's API by `claudeUsage.ts`. Posted to `statusBarTicker` with pluginId `"claude-usage"`. Claude Code only. When active, the separate ticker message is suppressed to avoid duplication.
- **PTY usage limit (priority 3):** Parsed from terminal output by the output parser (e.g. Claude's `[C1 S30 K26]` status line). Stored on the terminal entry as `usageLimit`. Applies to all agents that emit usage info.
- **Agent name (priority 4):** Fallback — just shows the agent type name.

**Ticker integration:** When the active agent is `claude` and the Claude Usage ticker is active, the ticker message is absorbed into the agent badge (priority 2) and hidden from the separate ticker area. Other ticker messages (from plugins, etc.) display normally.

**Pendulum ticker:** When the status info text overflows its container, a CSS pendulum animation scrolls the text back and forth at ~50px/s. Clicking the text dismisses the notification until the message changes.

**Ideas badge:** The Ideas toggle button shows a count badge (accent-colored) with the number of ideas visible for the current repo filter. Uses `ideasStore.filteredCount()`.

**PR lifecycle in StatusBar:** CLOSED PRs are never shown. MERGED PRs are shown with a 5-minute activity-based grace period (accumulated user activity tracked by `userActivityStore`). OPEN PRs are shown as-is.

### IdeasPanel (`IdeasPanel/`)

Ideas panel with per-repo filtering and terminal integration.

**Features:**
- Add, edit, delete notes
- Send note text to active terminal (marks note as "used")
- Notes filtered by active repo (global notes always visible)
- Reassign notes to different projects via dropdown
- Count badge in panel header and in the StatusBar toggle button
- Used notes shown with a checkmark and dimmed styling

### StoriesDialog (`StoriesDialog/`)

The project-scoped plan and story dialog creates manual plans and stories, presents criteria and dependency status, and sends revision-checked actions through the shared IPC/HTTP transport. It renders the Rust `plan_view` projection, including transitive abandoned dependency labels and the WontFix count or all-cancelled label. Only a direct WontFix dependency on a Backlog story offers the human Remove action; the backend decides whether the dependent becomes Ready. Visible labels use `t()` and English catalog entries, the controls use style-guide tokens and button variants, and the close button takes focus when the dialog opens.

The dialog probes `story_capabilities` before loading. A missing capability receives the restart message; action failures retain their own error details.

### ProgressDialog (`ProgressDialog/`)

The whole Progress UI: one newest-first list for the active PTY by default,
with a selector for other PTYs and the repository aggregate. It has a divider
marking where the last visit ended, and a blocked-only
checkbox. Blocked entries are red, host-written `intent` entries are muted, and
each row can be deleted. There are no pages, no tabs, no workstream projections
and no export — Progress is a thing you glance at, so it is a dialog and not a
panel that competes with the terminal for width.
The toolbar bell always includes Terminal Progress, regardless of unread count.
The command palette and `Cmd/Ctrl+Shift+P` open the same dialog.

Opening asks the shared journal once, for the selected PTY or project. The old panel
fanned out across every registered repository and answered with one red
unavailable block per repository that no longer existed; the dialog shows one
project and one failure line.

Each PTY and the aggregate has a separate divider, frozen while that view is
open. Switching views loads the new scope's mark; closing records every visited
scope without moving a line under the reader's cursor.

A **List | Flow** toggle switches to `ProgressFlow.tsx`, a sequence diagram of
the same journal: one column per participant (terminal or Claude subagent), one
row per event, arrows for delegations, returns and messages, and intents as
muted notes. Every row is the same CSS grid, so lifelines, headers and arrow
ends line up without measurement. Journal text expands in place; a subagent
arrow fetches its full text through `progressStore.fetchFlowDetail`. The
component only renders what `progress_flow` returns.

`embedded` drops the overlay and the floating box so the mobile PWA's Progress
tab can host the same component full-bleed; a whole bottom tab is already the
modal surface a dialog would create.

### ConfirmDialog (`ConfirmDialog/`)

Reusable in-app confirmation dialog that replaces native Tauri `ask()` dialogs (which render as light-mode macOS system sheets). Uses shared `dialog.module.css` for consistent dark-theme styling.

**Props:** `visible`, `title`, `message`, `confirmLabel`, `cancelLabel`, `kind` (warning/info/error), `onClose`, `onConfirm`.

**Keyboard:** Enter confirms, Escape cancels.

### ClaudeUsageDashboard (`ClaudeUsageDashboard/`)

Native SolidJS component (not a plugin) showing Claude API usage data. Displayed as a tab in the markdown/editor area. Features rate bucket gauges, per-model token breakdown, daily usage chart, and project stats. Clicking the Claude Usage ticker opens it for that terminal session's credential profile. The rate-limit API follows `CLAUDE_CONFIG_DIR`; transcript statistics still use the default Claude projects directory.

### CodexUsageDashboard (`CodexUsageDashboard/`)

Native dashboard for the official Codex App Server account snapshot. Shows rate
windows, reset times, daily token buckets, and the lifetime metrics that the
documented surface actually supplies. Unsupported legacy metrics are omitted.

### GrokUsageDashboard (`GrokUsageDashboard/`)

Native dashboard for Grok Build's `_x.ai/billing` ACP extension. Shows the
current billing-period percentage, subscription tier, period end, on-demand
used/cap amounts, and prepaid balance. It shares the usage-dashboard visual
system and never turns an absent provider value into zero.

## UI Primitives (`components/ui/`)

| Component | Description |
|-----------|-------------|
| `AgentIcon` | Agent type icon with consistent sizing and coloring |
| `CiRing` | SVG circular CI status indicator with proportional segments |
| `DiffViewer` | Syntax-highlighted unified diff renderer |
| `Dropdown` | Reusable dropdown select component |
| `ContentRenderer` | Safe markdown-to-HTML rendering with DOMPurify sanitization (including raw form and image-map removal), interactive checkboxes, tweak highlights, and click interception for every rendered link; `MarkdownTab` sends local href resolution to Rust |
| `PanelResizeHandle` | Draggable resize handle for panel boundaries |
| `PromptOption` | Agent prompt multiple-choice option |
| `StatusBadge` | Git status badges (clean/dirty/conflict) |
| `ZoomIndicator` | Terminal font size indicator |

## Shared Components (`components/shared/`)

| Component | Description |
|-----------|-------------|
| `ColorPickerDialog` | Color selection dialog (used by repo groups) |
| `ColorSwatchPicker` | Preset color swatch grid |
| `KeyComboCapture` | Keyboard shortcut capture input (for keybinding editor) |
| `SearchBar` | Reusable search bar with regex/case-sensitive toggles |

## Panel Toggle States

| Panel | Toggle Shortcut | Store |
|-------|-----------------|-------|
| Sidebar | `Cmd+B` | `uiStore.toggleSidebar()` |
| Git Panel | `Cmd+Shift+D` | `uiStore.toggleGitPanel()` |
| Markdown Panel | `Cmd+Shift+M` | `uiStore.toggleMarkdownPanel()` |
| Ideas Panel | `Cmd+Alt+N` | `uiStore.toggleIdeasPanel()` |
| File Browser | `Cmd+E` | `uiStore.toggleFileBrowserPanel()` |
| Settings | `Cmd+,` | Local state in App.tsx |
| Help | `Cmd+?` | Local state in App.tsx |
| Prompt Library | `Cmd+Shift+K` | `promptLibraryStore.toggleDrawer()` |
| Task Queue | — | Local state in App.tsx |
| Command Palette | `Cmd+P`; browser toolbar button | `commandPaletteStore.toggle()`; browser mode filters to explicitly supported web/HTTP actions |
| Activity Dashboard | — | `activityDashboardStore.toggle()` |
| Project Progress | Command palette / bell | `progressStore.toggle()` |
| Worktree Manager | `Cmd+Shift+W` | `worktreeManagerStore.toggle()` |

Activity Dashboard uses a 500px inline overlay and the same compact row layout
in its detached window. On startup and reopening, saved detached geometry cannot
make the Activity window larger than its 550×650 default; smaller saved sizes
remain in effect. `SubAgentIcon` supplies the shared 11px robot marker
for both the dashboard and sidebar; only its tooltip/accessible name contains
the parent name. The session list supplies each parent's live `tuic_session`
when its PTY ID differs from that identity.
