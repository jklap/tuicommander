# Settings

Open settings with `Cmd+,`. Settings are organized into pages. The navigation
groups the pages by task:

| Group | Pages |
|-------|-------|
| **Application** | General, Appearance, Notifications |
| **Workspace** | Terminal, Keyboard Shortcuts, Git & GitHub |
| **AI** | Agents, AI Chat, Voice, Smart Prompts |
| **Integrations** | MCP, Remote Access, Remote Servers, Plugins |
| **Repositories** | One page for each repository in the sidebar |

The **AI Chat** page is shown only while **Experimental Features** is on (see
[AI Chat](#ai-chat)).

## Search

A search box sits at the top of the page list. Typing filters settings across
every page at once, so you do not have to know which page owns the one you want.
Results are ranked best-match first, and a setting's description text counts too —
searching words that only appear in a hint still finds it. Each result shows the
setting name, the `Page › Section` trail it lives under, and its hint; selecting
one opens that page, scrolls to the field, and briefly flashes it so it is easy to
spot.

Settings also appear in the Command Palette (`Cmd+P`) under the "Settings"
category, so you can jump straight to one without opening the Settings panel
first.

Three limits are deliberate:

- **Repository pages are not searched.** A repository page belongs to one specific
  repository, and a global box has no way to know which one you mean.
- **Per-agent controls in expandable cards are not searched.** Search cannot
  select a specific agent card or scroll to a control inside it. For managed
  workspace trust, open **Settings → Agents**, expand **Claude** or **Codex**,
  then use **Accept workspace trust for managed spawns** in Expert Mode.
- **Settings the current build does not render are not listed.** The same rule
  hides the AI Chat page while Experimental Features is off, so a search does
  not open a page that is not in the navigation.

A result for an expert setting shows an **Expert** badge. See the next section.

## Expert Mode

Some settings have a default that is correct for almost everyone. These are
*expert* settings. The **Expert** switch in the Settings header controls them:

- **Expert off** (basic mode) — an expert setting is hidden while it has its
  default value. When its value is different from the default, it is shown, so
  an override is never hidden from you.
- **Expert on** — all expert settings are shown.

The switch position is kept between restarts (UI preference
`settings_expert_mode`, see [UI Preferences](../backend/config.md#ui-preferences-ui-prefsjson)).

A setting that you change stays shown until you open Settings again. If you
set it back to the default, it does not disappear under the cursor. The next
time you open Settings, it is hidden again if it has its default value.

When you select a search result with the **Expert** badge in basic mode,
Settings opens the page, shows that setting and scrolls to it. The setting stays
shown until you open Settings again. The switch does not change.

If TUICommander cannot read the default values (the `get_config_defaults`
command), all expert settings stay visible.

These settings are expert. All other settings are always shown. A page is
never all expert: in basic mode it would show an empty page.

| Page | Expert settings |
|------|-----------------|
| General | Auto-Standby Timeout, Content Indexing, Ignore leading whitespace, Ignore trailing whitespace, Ignore whitespace amount, Ignore case, Auto-open Session Diff Review, Truncate long changes, Update Channel |
| Notifications | Master Volume, Audio Output Device |
| Terminal | Shell, Font Weight, Allow OSC 52 clipboard writes, Allow terminal focus/attention requests, Block folding, Show block marks, Show prompt marks, Reflow scrollback on resize, Custom Environment Variables (the whole section) |
| Git & GitHub | Auto-Delete on PR Close, Copy ignored files, Copy untracked files, Warm ignored build directories, Storage Strategy, Auto-archive merged worktrees, Orphan Worktree Cleanup and safe cleanup countdown, After Merge Behavior, Auto-Fetch Interval, the **Add another GitHub account** button (shown while no additional account exists) |
| Agents | Collect project progress (global); per agent: Close idle managed child after, Auto-retry on server errors, Prevent alternate screen, Accept workspace trust for managed spawns (Claude and Codex), Native status signals, Install hooks globally, Track agent intent, Collect progress, Show suggested follow-ups, Headless Command Template; Claude only: Environment Flags |
| Voice | Long-press threshold, Auto-send, Input device, Level gate, Speech confidence gate, Hold-back before sending, Notify model when hands-free changes, Start notice |
| MCP | Collapse tools |
| Remote Access | Port, Session Token Duration, Enable IPv6 (dual-stack) |

## Application

### General

| Setting | Description |
|---------|-------------|
| **Language** | UI language. The list offers the locales that ship with a message catalog, each named in its own language, and the pick applies immediately — no reload. A locale without a catalog is never offered, because it would render English while claiming to be translated. The picker is hidden while only one catalog ships — today that is English alone — because a list of one is not a choice. |
| **Show agent context bar** | Show the model's current intent, its orchestrator-assigned task, and the last prompt sent to an agent. |
| **Restore window size and position on launch** | Reopen the app window at the same size and position as when it was last closed (desktop app only) |
| **Confirm before quitting** | Show dialog when closing app with active terminals |
| **Confirm before closing a tab** | Ask before closing terminal tab |
| **Prevent sleep when busy** | Keep the machine awake while agents are working (**Power Management** section) |
| **Auto-Standby Timeout** | Pause idle background sessions after this duration to save resources. Default 5 min; `0` disables it. |
| **Content Indexing** | When to build search indexes: Disabled, Active repo only, Active + on switch, or All repos at boot |
| **Ignore leading whitespace** | (**Diffs** section — applies to Session Diff Review, Branch Diff Scroll and the per-file diff view; the toolbar's diff-options popover sets the same values.) A line that only differs in leading whitespace isn't shown as changed. `git diff` has no native equivalent for this — TUIC computes it with its own diff engine when any of these four options is on. |
| **Ignore trailing whitespace** | Same, for trailing whitespace. |
| **Ignore whitespace amount** | Runs of whitespace compare equal regardless of how many characters they contain — catches reindentation/retabbing with no other content change. |
| **Ignore case** | Case-insensitive line comparison. |
| **Soft-wrap long lines** | Wrap long diff lines instead of scrolling horizontally. |
| **Auto-open Session Diff Review** | When TUIC detects a Claude Code session editing files in a repo: **Off** (never open it automatically), **Ask** (a notice in the notification bell with an "Open Session Diff" action), or **Auto** (open the tab in the background, without switching to it). Skipped if a tab for that session is already open. |
| **Truncate long changes** | Collapse a single change's diff above this many lines behind a "Show all N more lines" button. 0 = never truncate. |
| **Automatically check for updates** | Check for new versions on startup |
| **Update Channel** | Choose which release channel to receive updates from |
| **TUIC CLI** | Install or uninstall the `tuic` command-line tool, with its status. Desktop app only. See [CLI](cli.md). |
| **Finder Integration** (macOS only) | Add/remove the "New TUICommander Tab Here" Finder right-click item — see [Finder integration](finder-integration.md) |
| **Code Intelligence** | Install and manage MDKB, which gives the editor go-to-definition, find references, and symbol outline, and serves as a memory manager for AI agents. Desktop app only. |
| **ego executable** | Path to the ego binary the **AI Chat** panel talks to over ACP (**ego** section, directly after Code Intelligence). Always shown, also while Experimental Features is off. On the desktop it is laid out like the TUIC CLI section: a status line ("Configured at …") with **Select…** (native file picker) and **Clear**. A browser client, which has no native picker, gets a text field. While it is empty, ACP is not configured: every connect is refused in Rust and the panel says so rather than launching nothing. Read at each connect, so a correction takes effect without a restart. |
| **ego profile** | Optional name of a profile in ego's user configuration for the AI Chat panel. Leave empty to use ego's normal profile selection. Names with whitespace or a leading dash are rejected. TUICommander passes the name to `ego acp`; the profile's rules remain in ego's configuration. |
| **Default IDE** | IDE for "Open in..." actions (**IDE** section). Only installed apps are offered, grouped by category: Code Editors (VS Code, Cursor, Zed, Windsurf, Neovim, Xcode, `$EDITOR`), JetBrains (IntelliJ IDEA, PyCharm, WebStorm, GoLand, CLion, PhpStorm, RubyMine, Rider, DataGrip, RustRover, Android Studio, Fleet), Terminals (Ghostty, WezTerm, Alacritty, Kitty, Warp, iTerm2), Git Tools (Sourcetree, GitHub Desktop, Fork, GitKraken, Sublime Merge, Tower), System (Terminal, Finder) |
| **Custom Launchers** | Define your own tools for the "Open in" menu. Each launcher has a name, an executable (bare name resolved on `PATH`, or absolute path), and arguments (one per line). Arguments may use placeholders, expanded at launch: `{path}`/`{file}` (focused file, else repo root), `{fileDir}` (directory of the focused file), `{repo}` (repo/worktree root), `{cwd}` (focused terminal's working directory), `{home}` (your home directory), `{line}`/`{column}` (1-based editor cursor position). Args are passed verbatim (no shell parsing), so paths with spaces are safe. |
| **Experimental Features** | One toggle, no sub-toggles. It opts in to the **AI Chat** panel (ego over ACP, #785-58ca) and shows the **AI Chat** settings page. SSH Tunnels graduated out of it (Settings → **Remote Servers**). The AI Chat, AI Triage and AI Watchers sub-toggles went with the embedded AI engine (#784-0aec). |

### Appearance

| Setting | Default | Description |
|---------|---------|-------------|
| **Split Tab Mode** | — | Separate or unified tab appearance |
| **Tab Ordering** | — | How tabs are ordered: grouped by type, terminals first, or freely interleaved |
| **Cycle All Tab Types** | Off | When on, next/prev-tab shortcuts also cycle file/diff/markdown/editor tabs (ordered like the tab bar). Off cycles terminals only. |
| **Nested Terminal Tabs** | Off | Opt in to collapsible branch activity cards. When off, the sidebar shows no activity caret or nested agent/session rows. When on, branches with open sessions show agent status, current activity, and last-update age. Applies immediately. |
| **Max Tab Name Length** | — | 10–60 slider |
| **Repository Groups** | — | Create, rename, delete, and color-code groups |
| **Reset Panel Sizes** | — | Restore sidebar and panel widths to defaults (**Layout** section) |
| **Bell Style** | Visual | How the terminal bell (BEL) is signaled: None, Visual, Sound, or Both (**Bell** section) |
| **UI Legend** | — | Visual reference for colors, symbols, and badges used in the app, and the editor for them (see below) |

The terminal and app color theme is on the [Terminal](#terminal) page. The mobile PWA offers a separate Dark/Light choice in its Settings screen, saved on the server for connected mobile clients; the screen also shows app and server versions.

At the bottom of the tab, the **UI Legend** documents every color/icon/animation the app uses
(terminal status dots, tab types, tab activity dots — the unseen-content dot on an inactive
Session Diff Review tab — sidebar symbols, branch markers, PR markers, git repo status,
toolbar counts, diff stats) — the sidebar rows are drawn with the real sidebar components — and
doubles as the editor for them. A PR state's override recolors that state's marker on every
branch row as well as its badge in the GitHub panel. Clicking a row's own preview (the real
branch icon or PR marker, or the swatch) opens one combined dialog for that indicator, showing
only the sections it actually has:

- **Color** — the same preset/custom color picker used for repository groups.
- **Icon** — a grid of 18 curated monochrome shapes (dot, star, ring, square, triangle, diamond,
  chevron, spinner arc, bell, question mark, exclamation, pause bars, clock, checkmark, cross, and
  the branch/worktree/shell shapes) — every option renders live so you're picking the actual glyph,
  not a name.
- **Animation** — a list where every option animates its own preview dot. Some indicators (e.g. PR
  badges) offer a narrower set than others — a badge doesn't offer "glow", a spinning-halo effect
  meant for a small dot.

Every change applies immediately, updating the row's own preview as you pick — there's no separate
OK/Cancel. The dialog has a "Reset to default" button once any of its fields is overridden, and the
row itself gets a reset "×"; a "Reset all indicators" button at the bottom of the legend clears
everything at once. Overrides are stored in `indicator_overrides` in `config.json` and apply live —
no restart needed — and survive a theme switch. The read-only reference view in Help → UI Legend
shows the same information without the edit controls. See `src/indicators/registry.ts` for the full
list of customizable entries.

Four of the legend's group headings also carry a show/hide toggle for that whole group, stored
in `config.json` alongside the color/icon/animation overrides:

| Toggle | Default | Effect when off |
|---|---|---|
| **Show tab type highlighting** | On | Tab bar and mini pane-tree bar lose their per-type background tint and border color, but keep each type's icon color |
| **Show PR status badges** | On | Hides the PR marker on a sidebar branch row with a pull request, and the state badge in the GitHub panel's PR list |
| **Show git repo status indicators** | On | Hides the sidebar's rebase/merge/cherry-pick/revert/bisect badge and the Changes tab's conflicts banner |
| **Show diff stats** | On | Hides the sidebar's `+N/-N` diff stat badge next to a branch |

**Git repo status** — when a worktree (including the main checkout) has a rebase, merge,
cherry-pick, revert, or bisect in progress, its sidebar row shows a colored badge naming which
one. The Changes tab (Git panel) separately shows a conflicts banner listing every unmerged file
when the working tree has one, regardless of which operation caused it.

#### Show agents under branches

Nested agent rows are disabled by default. To show them, open **Settings → Appearance → Tabs** and turn on **Nested Terminal Tabs**. No restart is required.

After you enable the setting, a branch gets an activity caret when it has at least one open terminal session. Expand the branch to see its agents and shells. Each detected agent row shows its status, current intent or task, and last-update age; clicking a row switches to that session. The setting only displays existing sessions—it does not start or discover an agent outside a terminal assigned to that branch.

### Notifications

- When the desktop window is unfocused, agent questions and Progress `done`/`blocked` entries can appear in the operating system's Notification Center. TUICommander checks notification permission when the first alert is needed. On macOS, clicking an alert opens the named terminal or Progress project. An open, focused window receives no duplicate system alert.
- **Enable Audio Notifications** — Master toggle
- **Volume** — 0-100% (applied natively by the Rust playback path). Releasing the slider plays a short preview at the new level.
- **Audio Output Device** — defaults to the system output. Click **Choose output device…** to enumerate available outputs and pick a specific one. Enumeration is deferred until you click, because on macOS the audio device scan triggers the microphone-permission prompt — notifications never record audio.
- **Per-event toggles:**
  - Agent asks question
  - Error occurred
  - Task completed
  - Warning
  - Info
  - Attention (agent needs you)
- **Sound source (per event)** — a dropdown next to each per-event toggle picks what plays: **Default** (that event's own built-in tone), another event's tone borrowed as a preset (labeled by its character — Chime, Arpeggio, Low Tone, Double-Tap, Pluck, Callback — rather than which event it's borrowed from), or **Custom file…**, which opens the native file picker (`.wav`/`.mp3`/`.ogg`/`.flac`). A chosen custom file that can't be opened or decoded (moved, deleted, unsupported format) silently falls back to that event's default tone rather than staying silent. Browser/PWA mode can borrow another event's preset but has no filesystem access for a custom file — picking a preset still works there, custom files fall back the same way.
- **Test buttons** — Test each sound individually. Normal notifications share one anti-spam interval across every sound type so a burst cannot play overlapping tones. The Test button bypasses that limit, so rapid A/B volume comparisons always play.
- **Silence orchestration completions** — Remote HTTP/MCP workers still appear in Activity and update their tab state, but do not play a completion chime. The remote classification survives frontend reloads, and each busy cycle can notify at most once even when idle and process exit arrive separately.
- **Reset to Defaults** — Restore default notification settings
- **Keep toasts in the bell** — Retain user-action toasts in the toolbar bell's **MESSAGES** section, including their level and explicit action. Turning this off leaves ordinary user feedback transient; agent/MCP notices and agent spawns still go directly to the bell (identical consecutive notices within five seconds are deduplicated), and overflow from the two-card limit is retained there regardless of this setting. Dedicated-domain toasts that explicitly opt out of Messages remain visible or queued. Transient cards sit top-right of the terminal, clear of docked panels and bottom input; same-kind bursts within five seconds show ×N and restart their dismissal timer. Errors take priority over informational cards, and tall cards scroll to keep actions reachable. This setting is visual and remains available without audio output.

A toast raised with a sound (a failed git operation, a plugin's `tuic.toast()` call, etc.) plays through these same settings, matched by level (info→Info, warn→Warning, error→Error) — the master toggle, per-event toggle, volume, output device, and any preset/custom file you've picked for that event all apply.

**Attention** is the distinct call-back-to-keyboard sound available to agent
toasts. Native playback and the browser fallback share a triangular G4→G4→E5
motif with two short knocks and a longer rise; each engine applies its own
envelope. It remains subject
to the master toggle, configured volume, and its own per-event toggle; native
playback also uses the selected output device.

## Workspace

### Terminal

| Setting | Default | Description |
|---------|---------|-------------|
| **Terminal Theme** | — | Color theme for terminal output and app chrome, with preview swatches (**Theme** section) |
| **Shell** | — | Custom shell path (e.g., `/bin/zsh`, `/usr/local/bin/fish`). Leave empty for system default. |
| **Terminal Font** | JetBrains Mono | 13 bundled monospace fonts: Fira Code, Hack, Cascadia Code, Source Code Pro, IBM Plex Mono, Inconsolata, Ubuntu Mono, Anonymous Pro, Roboto Mono, Space Mono, Monaspace Neon, Geist Mono |
| **Default Font Size** | — | 8–32px slider. Applies to new terminals; existing terminals keep their zoom level. |
| **Font Weight** | — | Terminal font weight (200 = ExtraLight, 400 = Regular, 700 = Bold) |
| **Cursor Style** | — | Bar, Block, or Underline |
| **Copy on select** | On | Auto-copy terminal selection to clipboard. When text is selected in the terminal, it is immediately copied. A "Copied to clipboard" confirmation appears in the status bar. |
| **Allow OSC 52 clipboard writes** | On | Let terminal programs set the system clipboard via the OSC 52 escape sequence (used by tmux, vim, ssh yank-over-SSH, etc.). Because OSC 52 is honored from anywhere in the byte stream, a displayed file or log can also overwrite the clipboard — so a non-blocking "Clipboard updated" notice appears on every successful write (a failed write logs quietly instead). Disable to ignore OSC 52 entirely. |
| **Allow terminal focus/attention requests** | On | Let terminal programs bring the window to the front (iTerm2 `StealFocus`) or bounce the dock icon (`RequestAttention`) via OSC 1337. Disable if a script or log spams either. |
| **Open links on** | Click | How links (URLs, file paths) in terminal output activate. **Click** opens on a plain click. **⌘Click**/**Ctrl+Click** hides the underline until Cmd (macOS) or Ctrl (Windows/Linux) is held, then opens on modifier+click. **Never** disables click-to-open entirely — right-click still offers Open/Copy link. |
| **Show block timestamps** | Hold Ctrl+Cmd | When each command block is labelled at the right edge with how long ago it started: **Never**, only while **Ctrl+Cmd** is held, or **Always**. |
| **Block folding** | On | Let the Toggle Block Fold shortcut (Cmd/Ctrl+Shift+.), its command-palette entry and the fold chevron on a block's header row in the gutter collapse a command block's output (clicking elsewhere in the block's gutter selects its output for copying). Blocks already folded stay collapsed when this is off. |
| **Show block marks** | On | Blue/red scrollbar tick for each command block (red when the command failed). Expert setting. Orange search-match ticks are not affected by this or the next setting: they are the result of a search you just ran, not a display preference. |
| **Show prompt marks** | On | Green scrollbar tick for each prompt you sent. Expert setting. |
| **Reflow scrollback on resize** | On | Re-wrap scrollback history when the terminal changes width, so output written at the old width stays readable after a side panel opens or closes. Turn it off to leave history lines as they were written and truncate them to the new width instead. The visible screen is never reflowed either way — cursor-addressed TUIs redraw themselves. A change applies to sessions already open. |

See [Command Blocks](terminals.md#command-blocks) for what the block settings control.

The Terminal page also has a **Shell Integration** section with copyable bash/fish startup-file
snippets (zsh needs no setup) — see [Shell Integration](terminals.md#shell-integration).

The terminal bell mode (`none`, `visual`, `sound`, `both`) has no control in
Settings. Set `bell_style` in `config.json` (see [Terminal Bell](terminals.md#terminal-bell)).

### Custom Environment Variables

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| **Environment Variables** | list of `KEY = value` | (empty) | Applied to every spawned terminal — shell tabs and agent tabs alike — overriding anything else that sets the same variable, **including `PATH`**: an entry named `PATH` replaces it entirely rather than prepending to it, which can silently break TUICommander's own PATH-dependent shims (`imgcat`/`imgls`/`divider`) if you don't account for the rest of `PATH` yourself. A key must be a legal environment variable name (starts with a letter or underscore, then letters/numbers/underscores only); duplicate keys are rejected, case-insensitively (Windows env var names collide case-insensitively at the OS level). **Reserved names are refused:** `TUIC_*` (TUICommander's own session identity), `ZDOTDIR` (how the zsh integration loads) and dynamic-loader variables (`LD_PRELOAD`, `LD_LIBRARY_PATH`, `LD_AUDIT`, `DYLD_*`); such an entry in a hand-edited `config.json` is dropped on load. An expert setting: hidden in basic mode while the list is empty. Useful for forcing a shell customization's behavior regardless of timing, e.g. `POWERLEVEL9K_DISABLE_CONFIGURATION_WIZARD=true` for Powerlevel10k. See [`pty.md`](../backend/pty.md#custom-pty-environment-variables) for the full precedence order against other env sources (per-agent run-config env, per-agent env flags). |

### Session Restore

| Setting | Default | Description |
|---------|---------|-------------|
| **Restore open terminals on launch** | On | Reopen plain shell tabs (not just agent tabs) in their saved directory when you relaunch. Off restores only agent tabs, matching pre-1.8 behavior. |
| **Save terminal scrollback** | Off | Persist each terminal's recent output to disk and replay it above a fresh prompt when the tab is restored. Stored as plain text in the app's config directory — leave off if that output may contain secrets. |
| **Scrollback lines to save** | 1000 | Maximum lines of output saved per terminal when scrollback saving is on |
| **Clear saved scrollback** button | — | Deletes every saved scrollback file immediately |

See [Session Restore](terminals.md#session-restore) for how restore behaves across a relaunch.

### Smart Selection

Controls double/quad-click word and smart selection in the terminal — see [Smart Selection](../frontend/terminal-features.md#smart-selection) for the underlying model (precision scoring, the built-in rule set, action dispatch).

| Setting | Default | Description |
|---------|---------|-------------|
| **Double-click performs** | Smart | **Word** expands to the character-class boundary below. **Smart** tries the rule list first, falling back to word selection when nothing matches. There is no master on/off switch — quad-click (4 rapid clicks) and the right-click smart-selection menu always try the rule list, regardless of this setting. |
| **Word boundaries** | Character list | **Character list**: a literal set of characters that break a word. **Regular expression**: `\|`-joined alternates — the longest match at each position joins onto the adjacent word, e.g. adding `https://` lets a double-click on a URL's host include the scheme. |
| **Word separators** | `` " \"'`(){}[]<>\|;:,.!?@#$%^&*~=+/\\" `` | (Character-list mode) Characters that break a word for double-click selection. Whitespace and control characters are always separators regardless of this list. A "Restore default separators" button resets it. |
| **Word pattern** | empty | (Regex mode) `\|`-joined alternates. Plain letters/digits/underscore are always word characters; alternates here join punctuation-containing spans onto them. Invalid alternates are flagged inline and skipped. |

**Smart Selection Rules** is a list editor over the rule engine. Each rule is a single-line row (enabled checkbox, name, and precision) that expands into its full editor on click — matching the Smart Prompts settings pattern. The row header shows the **Name** (also shown as a header above its actions in the terminal's right-click menu, so you can tell which rule matched); expanding it reveals the **Pattern** (the regular expression, flagged inline if invalid — that rule is then skipped) and **Precision** (`Very Low`–`Very High`, which resolves overlapping matches — see [Smart Selection](../frontend/terminal-features.md#smart-selection)). Below that, zero or more actions, each with an **Action** kind (Copy, Open URL, Open File, Send Text, Run Command, Run Command in New Terminal, Ask AI), a **Menu label** (the text shown in the right-click menu), a **Parameter** template (supporting `\0`-`\9`/`\d`/`\u`/`\h` substitution), and a **Default** radio — at most one action per rule may be marked default, and that's what Option/Alt+double-click runs. The list starts populated with the built-in default rule set (iTerm2's ten plus dev-terminal extras); editing any rule materializes the full set into your saved configuration. A "Restore built-in defaults" button clears your customizations and reverts to the built-in set.

A toolbar above the rule list mirrors Smart Prompts' Export/Import: a scope dropdown (**All
rules**, **Modified only** — a built-in you've changed plus every custom rule, or **Custom
only**), an **Export…** button that opens the OS Save dialog, and an **Import…** button that
opens a review dialog listing every rule in the file as **NEW** or **CONFLICT**. Because a rule
with no default to compare against (any custom rule, or an id the file doesn't recognize as
built-in) counts as modified by definition, "Modified only" always includes every custom rule.
Importing anything — even a single custom rule — materializes the full built-in set into your
saved configuration, exactly like editing one rule does; "Restore built-in defaults" undoes
this. A rule with a **Run Command**, **Run Command in New Terminal**, or **Send Text** action
imports **disabled**, the same treatment Smart Prompts gives a shell/API prompt — review its
pattern and parameter before enabling it.

### Keyboard Shortcuts

Browse and rebind all app actions. The same editor is also in the **Help panel**
(Help > Keyboard Shortcuts).

- Every registered action is listed with its current keybinding
- Click the pencil icon next to any action and press a new key combination to rebind it
- Custom bindings are stored in `keybindings.json` in the platform config directory
- Auto-populated from the action registry — new actions appear automatically
- **Global Hotkey (Toggle Window)** — an OS-level shortcut to show or hide TUICommander from any application (desktop app only)
- **Plugin Commands** — commands that plugins register, which you can bind like any other action
- Most shortcuts are also listed in the native system menu bar (desktop app only — browser/PWA clients have no native menu)

See [Keyboard Shortcuts](keyboard-shortcuts.md) for the full reference and customization guide.

### Git & GitHub

GitHub authentication, pull request and issue display, and the global
repository and worktree defaults.

| Setting | Description |
|---------|-------------|
| **OAuth Login** | Device Flow login — click "Sign in with GitHub", enter code on github.com. Token stored in OS keyring. (**GitHub Authentication** section) |
| **Auth Status** | Shows current login, avatar, token source (OAuth/env/CLI), and available scopes |
| **Disconnect** | Clear all GitHub tokens (keyring + env cache). Falls back to next available source. |
| **Diagnostics** | Token source details, scope verification, API connectivity check |
| **Auto-show PR popover** | Automatically display PR details when switching branches. Only shows for OPEN pull requests — CLOSED PRs are hidden, and MERGED PRs fade after 5 minutes of user activity. (**Pull Requests** section) |
| **Hide Draft PRs / Hide Conflicting PRs / Hide CI Failing PRs** | Filter the pull requests the app shows. Each can be overridden per repository (**On / Use global / Off**, see [Repositories](#worktree-tab)) |
| **Auto-Delete on PR Close** | Off (default), Ask, or Auto — controls branch cleanup when a PR is merged/closed |
| **Show issues / Issue Filter** | Which issues to show in the GitHub panel: Assigned (default), Created, Mentioned, All, or Disabled (**Issues** section) |
| **Repository Defaults** | Base branch, **Copy ignored files** and **Copy untracked files** (two separate toggles), **Warm ignored build directories** (on by default; expert), setup/run/archive scripts applied to new repos |
| **Worktree Defaults** | Storage strategy, branch-name prompt, branch deletion, auto-archive, orphan cleanup, merge strategy, after-merge behavior, auto-fetch interval. See [Worktrees](worktrees.md). |
| **Additional GitHub Accounts** | Extra github.com or Enterprise logins. See [GitHub Integration](github-integration.md). |
| **CircleCI** | Store or remove a read-only CircleCI token for failed job logs. The saved token is never displayed again. |
| **Repository Bindings** | Which account each workspace repo resolves to |

Token priority: `GH_TOKEN` env → `GITHUB_TOKEN` env → OAuth keyring → `gh` CLI config → `gh auth token` subprocess.

## AI

### Agents

**Close idle managed child after** sets the time a finished orchestrator-spawned
agent stays open for follow-up. The default is 15 minutes; 0 disables automatic
closure. User-created sessions are unaffected. Unread mail and background work
keep a managed child open, and its parent receives an idle-timeout notice before
closure. A child can also be marked keep-open at spawn or through the agent or
session tool.

Each supported agent has an expandable row showing detection status, version, and MCP badge.

The default Codex run configuration includes
`--dangerously-bypass-approvals-and-sandbox` in its editable arguments.
The warning icon on a configuration identifies an active bypass. Remove that
argument through **Edit** to enable normal approvals and sandboxing for new
launches. The removal survives restart. Existing direct Codex defaults receive
the argument once during migration; wrapper configurations keep their own
arguments and the managed-launch wrapper warning.

| Setting | Description |
|---------|-------------|
| **Agent Detection** | Auto-detects running agents from terminal output patterns. Shows "Available" or "Not found" for each agent. |
| **Run Configurations** | Custom launch configs (binary path, args, model, prompt) per agent. Add, set default, edit, or delete configurations (Edit / Delete live under the `···` menu on each row). A config named **"review"** enables the Review button in the PR Detail Popover — its args are interpolated with `{pr_number}`, `{branch}`, `{base_branch}`, `{repo}`, `{pr_url}`. The agent's **default run config** also drives resume: launching / resuming the agent swaps the agent's default binary (e.g. `claude`) for `command` and appends `args` after the resume flag. |
| **MCP Integration** | Install/remove TUICommander as MCP server for supported agents. Shows install status with a dot indicator. |
| **Native status signals** | Claude and Codex only. Enabled by default; injects process-scoped status configuration at launch without changing global agent files. The **Signals: at launch** badge identifies this mode. |
| **Prevent alternate screen** | Enabled by default for every agent. TUIC applies a verified native-scrollback control when the installed CLI supports one. Turning it off leaves new launches without TUIC's screen control. Existing shells need to be reopened to receive the changed policy. |
| **Accept workspace trust for managed spawns** | Claude and Codex only. In Expert Mode, expand the agent's card to change whether agent-to-agent spawns skip that agent's workspace trust question in a new folder. |
| **Install hooks globally** | Gemini, Grok, and OpenCode only. Explicitly installs/removes sentinel-owned lifecycle hooks in the agent's global configuration. Off by default. |
| **Show agent intent as tab title** | When agents declare their current work phase, update the tab name with a short title |
| **Show suggested follow-up actions** | Display actionable suggestions from agents after completing a task |
| **Collect project progress** | Global switch for the Progress journal. On by default. Off removes the `progress` tool from every agent's tool list, stops the reporting obligation being sent, and records nothing — including the `intent:` markers TUICommander writes itself. |
| **Collect progress** (per agent) | Per-agent override, shown with the agent's MCP settings and disabled while the global switch is off. The effective value is *global AND (per-agent, default on)*, so an agent with no opinion follows the global switch. An agent that reports while its own override is off is answered `progress_tracking_disabled` rather than silently ignored. |
| **Claude Usage Dashboard** | (Claude Code only) Toggle under Features when the Claude row is expanded. Enables rate limit monitoring, session analytics, token usage charts, activity heatmap, and per-project breakdowns. Usage data appears in the status bar agent badge and in a dedicated dashboard tab. |

See [AI Agents](ai-agents.md) for details on agent detection, rate limits, and the usage dashboard.

### AI Chat

Shows the configuration of `ego`, the engine behind the AI Chat panel. Offered
only while **Experimental Features** (General page) is on, because that is what
offers the AI Chat panel — the one place ego is reachable from.

The ego binary itself is a TUICommander setting and is on the
[General](#general) page (**ego executable**), like MDKB.

The page shows which providers and models `ego` can use, and which model it
starts from. Everything here is ego's, read and written by running ego:

- **Default model** — a picker over every model ego knows, grouped by provider.
  Choosing one runs `ego config set model="<slug>"` and then re-reads, so what
  you see afterwards is what ego persisted, not what was sent. It survives a
  restart because ego holds it, not TUICommander. A model ego marked unavailable
  cannot be picked.
- **Refresh from providers** — asks ego to re-enumerate its sources
  (`ego models --refresh`). This is the only action anywhere in TUICommander
  that reaches a provider over the network, and it is ego that reaches it.
  Opening the page does not.
- **Providers** — one row per source ego knows, with how many of its models are
  usable and why the rest are not, in ego's own words.
- **Credential state** — read from `ego doctor`. *stored*, *expired* (ego renews
  it on its next run; you do not log in again), *no credential*, or *ego could
  not read its credential store* — the last is not the same as an empty store.

**No API key ever enters TUICommander.** None is stored, none reaches the OS
keyring, and no provider HTTP call is made from this process. **Login** next
to a provider opens a terminal tab that runs `ego auth login <provider>` and
closes Settings so the tab is in front. The browser, device-code or API-key
prompt is ego talking to you directly in that terminal; TUICommander types the
command and never sees the key. The tab stays open after ego exits so a refusal
stays readable; reopening the page shows the credential state `ego doctor` now
reports.

When ego is not configured the page says so and points to the *ego executable*
field in Settings → General instead of rendering an empty list. A path that is
set but cannot be started is reported as its own thing, and also points to
Settings → General. When an ego command fails, what ego
printed is shown verbatim — the command, its exit code, and its output.

The in-chat model switch is a different control: that one is a session option
and changes one conversation. This changes the default every new run starts
from.

#### The old provider registry is gone

The provider registry, its slots and the API keys it put in the OS keyring were
deleted with the embedded AI engine (#784-0aec).
`<config_dir>/providers.json` is no longer read or written, and a file left by
an older version is inert. This page is not its replacement — it edits ego's
configuration and stores nothing of its own.

Agent CLIs (Claude Code, Codex, …) are unaffected — they keep their own
credentials and are configured on the **Agents** page.

**Smart Prompts in API mode** used the old registry's headless slot. It now runs
one unattended ego turn instead (#787-ee50): the model is whatever this page's
default is, the prompt is sent once, and the answer goes to the prompt's output
target. Nothing streams — there is no panel open to stream to. With no ego
binary named, the mode refuses and says to name one and pick a model on this
page. Shell, inject and headless-CLI modes are unaffected.

The turn runs with **no TUICommander tools**, so it cannot open a terminal or
touch a repository, and any permission ego asks for is declined at once: nobody
is watching, and a question nobody answers is a turn that never ends. If a
prompt comes back empty because of that, the refusal is reported rather than the
empty answer.

### Voice

Dictation, speech recognition, spoken replies, and hands-free conversation.
Hands-free conversation uses an accent **Start conversation** action and a distinct **Stop conversation** action. The state row shows a coloured indicator with **Running** or **Stopped**, alongside the backend phase when available.
The global dictation hotkey and this machine's input devices are desktop-only;
a browser client shows the rest of the page. See [Voice Dictation](dictation.md)
for full details.

### Smart Prompts

Manage the AI-powered actions surfaced in the toolbar, context menus, and command palette. Reachable from the nav or directly via "Manage Smart Prompts..." in the Smart Prompts drawer.

- **Export/Import toolbar** — a scope dropdown (**All prompts**, **Modified only**, **Custom
  only**) plus **Export…**/**Import…**; export covers your entire prompt library (a hint under
  the toolbar says so), not just the prompt list below, which shows only prompts tagged "smart"
- **Headless Agent** — default agent for headless prompts; individual prompts can override it
- **Prompt list** — grouped by category, with enable/disable toggle and placement/mode badges
- **Editor** (click a row) — name, description, content with variable insertion, placement checkboxes, Execution Mode, inject target, Auto-execute, output target, system prompt, keyboard shortcut
- Built-in prompts show "Reset to Default" once overridden; custom prompts can be deleted

See [Smart Prompts](smart-prompts.md) for the full guide.

## Integrations

### MCP

#### HTTP API Server

Enable the HTTP API server for external tool integration:
- Serves the REST API and MCP protocol for AI agents and automation tools
- Local MCP connections use a Unix domain socket at `<config_dir>/mcp.sock` — no port configuration needed
- AI agents connect via the `tuic-bridge` sidecar (auto-installed on first launch for every supported agent that is installed on the machine — see [MCP bridge auto-install](../backend/config.md#mcp-bridge-auto-install))
- Shows server status (running/stopped) and active session count

#### TUIC MCP Server

Native tools exposed to AI agents via MCP. Each tool can be individually enabled or disabled to restrict what agents can access.

A note above the manual configuration points to the per-agent auto-configure option: the MCP server can also be configured automatically for a specific agent from that agent's own settings, under **Settings → Agents**.

**Manual MCP configuration** (expandable) — shows the `tuic-bridge` binary path and a ready-to-paste JSON snippet for manually configuring MCP clients that aren't auto-installed. Click "Copy" to copy the snippet to clipboard.

**Collapse tools** (checkbox) — when enabled, replaces the full tool list sent to AI agents with 3 lazy-discovery meta-tools (`search_tools`, `get_tool_schema`, `call_tool`). Cuts the baseline MCP context cost the agent carries every turn. Measured 2026-09-13 against the running desktop instance with 190 tools connected: the full list is 154,117 bytes / 35,104 tokens, the collapsed list 2,810 bytes / 615 tokens — and the collapsed figure does not move with the number of upstream tools, because they are no longer in the list. The agent fetches schemas on demand via BM25-ranked search. (Tokenizer: tiktoken 0.14.0 `o200k_base`, a GPT tokenizer used as a proxy; Anthropic publishes no offline tokenizer. Method and full table: [`mcp-http.md`](../backend/mcp-http.md#measuring-the-surfaces).) Native semantics do not change: a managed command is still one `call_tool` request for `session action=submit`, and its bounded receipt comes back in that response. Default: off. Grok sessions receive this compact surface automatically for compatibility with Grok's tool-name parser, without changing the checkbox or other clients. Toggling emits `notifications/tools/list_changed`; compatible clients refresh automatically, while clients that ignore the notification may require a reconnect.

**Native tools** — the list and descriptions come from the backend MCP registry, including disabled tools. Newly registered tools appear automatically, without a separate Settings list. Each row shows the description's first line; hover over its information badge for the full description and actions. All native tools can be disabled, including `progress`. The progress tool also requires the global **Progress tracking** setting. The list does not include upstream tools or the collapse-mode meta-tools.

#### Upstream MCP Servers

**Manage in Settings** in the MCP popup (**Cmd+Shift+I**) opens the MCP page scrolled to this block — the block sits below the fold, so a plain page switch would look like nothing happened.

OAuth upstreams show **Authorize** when consent is required. TUIC prepares the OAuth request, then displays a blocking in-app confirmation naming the authorization-server origin before opening the system browser. Cancelling that confirmation discards the pending request.

Proxy external MCP servers through TUICommander. Their tools appear prefixed as `{name}__{tool}`:
- Add upstream servers via HTTP (Streamable MCP) or stdio (process) transport
- API keys for HTTP upstreams are stored in the OS keychain
- Live status (connecting, ready, circuit open, failed) with tool count and call metrics
- Reconnect and remove controls per upstream
- Per-repo scoping: each repo can define an allowlist of active upstream servers via **Cmd+Shift+I** popup (or repo settings). Empty/null allowlist = all servers active

See [MCP Proxy](mcp-proxy.md) for the full guide.

#### MCP integrations

Shown (desktop only) when at least one MCP client holds a TUICommander bridge entry: lists those clients and offers **Remove all MCP integrations**, so uninstalling TUICommander does not leave a dangling `tuic-bridge` server behind. It used to sit at the bottom of the Agents page.

### Remote Access

Enable HTTP/WebSocket access from other devices on your network: port,
credentials, network interface, session token duration, IPv6, LAN access
without authentication, **Tailscale HTTPS**, the connect QR code, and the
**Cloud Relay**. See [Remote Access](remote-access.md) for full setup guide.

#### File Access (on the Remote Access page)

**Additional Readable Directories** — absolute directories that web and remote clients may **read** files from, in addition to your registered repositories. Ships with `~/.claude/plans` enabled by default, so clicking a Claude Code plan-file link an agent printed works out of the box in browser/remote mode. Desktop reads are never restricted — this setting only affects the HTTP transport used by browser/PWA/remote clients. It never widens writing, copying, or moving a file — those stay confined to registered repository roots. Remove an entry here if you don't want it readable over HTTP; the setting applies regardless of whether Remote Access itself is enabled, since it also governs the headless `tuic-remote` daemon.

### Remote Servers

SSH tunnel profiles and remote-machine connections on one page (it replaced the
separate **Remote Machines** page; an old `remote-machines` link opens it).
**Add Connection** opens one merged editor: a **Name** and a **Kind** — SSH
Tunnel, Remote Server — SSH, Remote Server — Direct or Remote Server — Local —
then only that kind's fields, a **Test Connection** button and Save. Below it,
**SSH Port-Forwarding Tunnels** lists the tunnel profiles (Start/Stop, Edit, Log, Del) and
**Remote Machines** the `tuic-remote` connections: discovered SSH hosts, then
each connection's status with Connect, Update, Install, Edit and Remove.

- A Remote Server's **StrictHostKeyChecking** is fixed to `AcceptNew` — the
  tunnel opened on your behalf always runs that way; a tunnel profile offers
  `Yes` too
- **Auto-update remote daemons** is a per-connection option, off by default. It
  updates on connect only when the daemon has no live PTY sessions
- The auth password goes to the OS credential vault; leave it blank on edit to
  keep the stored one, or **Clear stored password** to forget it

See [SSH Tunnel Management](remote-access.md#ssh-tunnel-management) and
[Remote Access → Remote Connection Manager](remote-access.md#remote-connection-manager).

### StreamDock

Configure a StreamDock M18 macropad (15-key LCD grid + 3 plain buttons) that mirrors live session
state to its keys. Desktop only.

- **Enable StreamDock integration** — attach to the first connected device (or the one selected below)
- **StreamDock status** — live connection state, polled every 2s
- **Device** — pick which unit to use when more than one is connected, with a rescan button
- **Screen brightness** / **LED brightness** — 0-100%; LED brightness only applies on firmware that reports RGB support
- **Pinned sessions** — a toggle per live session; a pinned session's key is never evicted to make room for another, even one that needs input more urgently

Each key shows a session's status color, its short alias (e.g. `tc-1`), and current task. Tap a
key to answer its pending choice prompt (if any) or focus that tab; double-tap answers the
second option; hold sends an interrupt. The bottom row and the 3 plain buttons are fixed verb
keys (approve/reject/interrupt/jump-to-waiting).

### Plugins

Install, manage, and browse plugins. See [Plugins](plugins.md) for the full guide.

- **Installed** — List all plugins with enable/disable toggle, logs viewer, uninstall
- **Browse** — Discover and install from the community registry
- **Check for plugin updates** — fetch the registry at startup and show available updates

## Repositories

Per-repository settings. Open them from the **Repositories** group in the
Settings navigation, or from the sidebar `⋯` → "Repo Settings".

### Worktree Tab

- **Display Name** — Custom name shown in sidebar
- **Branch From** — Base branch new worktrees are created from: "Use global default", "Automatic"
  (let TUIC detect the repo's default branch), or any of the repo's actual local/remote branches,
  grouped under "Local"/"Remote" — the list is fetched live, so it only ever offers branches that
  really exist in this repo. Drives the Create Worktree dialog's preselected "Start from" branch
  (session picks in the dialog still win for the rest of that session). If a previously-configured
  branch has since been deleted, the dropdown keeps showing it with a "(no longer exists)" marker
  and a warning to pick a different one — this never blocks creating a worktree; the dialog just
  preselects nothing and shows the same warning until a branch is chosen
- **Copy ignored files** — Copy .gitignored files to new worktrees
- **Copy untracked files** — Copy untracked files to new worktrees
- **Warm ignored build directories** — Copy-on-write copy the parent's git-ignored build
  directories (`node_modules`, `target`, …) into a new worktree in the background so it starts
  warm (On / Use global / Off; the global default is On). Off leaves the worktree cold
- **Always Copy These Files/Directories** — a repo-specific list of extra paths (relative to the
  repo root) always copied — or symlinked, to share a single copy across worktrees — into every
  new worktree of this repo, regardless of the two toggles above. There is no global default for
  this list; it only ever lives on the repo
- **Prompt on create**, **Delete branch on remove**, **Auto-archive merged** — override the
  matching global worktree default for this repo (see [Worktrees](worktrees.md#worktree-settings))
- **PR Visibility** — per-repo override of Hide Draft/Conflicting/CI-Failing PRs (see
  [Git & GitHub](#git--github) above)
- **Terminal → Enable Cmd+1-9 terminal hotkeys** (macOS only) — per-repo override; global default
  is always On

Every on/off field in this list uses a three-position **On / Use global / Off** control rather
than a plain checkbox, so "inherit the global setting" is always one of the three choices you can
select directly — not just the state you land in until you touch the control once.

Copying ignored/untracked files and the explicit list both run in the background, after the
worktree itself is created and usable — a toast announces when the copy starts and again when it
finishes (naming how many files were skipped, if any). This can take a while for a large ignored
tree (e.g. `node_modules`), which is exactly why it doesn't block opening the worktree.

### Scripts Tab

- **Setup Script** — Runs once after worktree creation (e.g., `npm install`), in the background after the build-artifact warm copy and then the worktree file sync (`copy_ignored_files`/`copy_untracked_files`/`copy_paths`) have finished — worktree creation itself returns immediately, and the workspace's `warm_artifacts.status` reads `pending` until the script is done. A failure shows in the status bar (via the `worktree-setup-script-completed` event), not in a synchronous response. The new tab's Run Script waits for it.
- **Run Script** — On-demand script launchable from toolbar with `Cmd+R`
- **Archive Script** — Runs before a worktree is archived or deleted; non-zero exit (including a timeout) blocks the operation
- **Dev Server URL** — Optional address of this repository's development server. Design Mode opens it in a dedicated Chrome window; with no URL, Chrome opens `about:blank` so you can navigate manually. See [Design Mode](design-mode.md).

All three scripts run with a `TUIC_*` environment injected (main checkout path, branch, base ref, worktree name, etc. — see the Terminals guide's Environment Variables section) and a fixed 15-minute (900 s) timeout. A script that hits the timeout is stopped together with everything it started (on macOS/Linux its whole process group, on Windows its process tree); a script that finishes but leaves a background job holding its output returns within a couple of seconds, leaving that job running.

### Repo-Local Config (`.tuic.json`)

A `.tuic.json` file in the repository root provides team-shareable settings that override per-repo app settings and global defaults. The file is read-only from TUICommander (edit it in your repo directly).

**Precedence:** `.tuic.json` > per-repo app settings > global defaults

Supported fields: `base_branch`, `copy_ignored_files`, `copy_untracked_files`, `warm_ignored_directories`, `worktree_storage`, `delete_branch_on_remove`, `auto_archive_merged`, `orphan_cleanup`, `pr_merge_strategy`, `after_merge`, `auto_delete_on_pr_close`.

**`setup_script`/`run_script`/`archive_script` are deliberately NOT supported in `.tuic.json`** — executing a repo-committed script with no trust-on-first-use confirmation would let a malicious branch run arbitrary code the moment its worktree is created. Configure these per-repo (Settings) or as a global default instead.

User-specific settings (`promptOnCreate`, `autoFetchIntervalMinutes`, and the Dev Server URL) are intentionally excluded from `.tuic.json`.

### Telegram

Settings → Telegram configures the current machine. Mobile Settings has a **Telegram setup** button with the same controls. Use it on the host running `tuic-remote`.

1. Paste the BotFather token into the password field and select **Save and check bot**. TUIC writes `bot.token` with owner-only permissions and checks `getMe`. Settings shows the bot username and whether a token is set; it never reads the token back.
2. Enable Telegram. Agents opt in with the Telegram MCP tool (`register`/`unregister`); Settings shows the registered agent read-only, or `nessun agent registrato`. Select **Link chat**, then send the displayed six-character code to the bot within ten minutes. The code works once. Alternatively, type a positive private chat ID and select **Add chat ID**. Remove revokes authorization. A bare `/start` does not authorize a chat. An ordinary message after pairing in the same poll uses the new authorization; the pairing code never reaches the agent.
3. Check connection status, safe error category and last accepted message time. Only the headless daemon polls; opening desktop Settings does not start another owner. Config changes restart the daemon adapter and retire its current transient draft state.
