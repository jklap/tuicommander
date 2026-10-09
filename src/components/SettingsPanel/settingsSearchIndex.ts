import { locale, t } from "../../i18n";
import { buildIndex } from "../../utils/bm25";
import { globalTabLabel } from "./settingsTabs";

/** Search index for the Settings panel — one entry per section heading and per
 * labelled setting, across every global settings tab.
 *
 * ## Design decision (2026-09-05)
 *
 * Three options were on the table for building this index:
 *
 * 1. **DOM scan of mounted tabs.** Rejected: `SettingsPanel` renders exactly one
 *    tab at a time, so a scan sees ~1/11th of the settings. Mounting every tab
 *    to scan it is worse than useless — each tab runs `onMount` side effects
 *    (`get_cli_status`, `mdkb_status`, GitHub auth probes, audio device
 *    enumeration), so a keystroke in the search box would fire a burst of
 *    backend calls.
 * 2. **Vite plugin extracting the index at build time.** Rejected for now: it
 *    makes drift structurally impossible, but it puts JSX parsing in the build
 *    graph, needs a second code path for vitest, and buys only what the drift
 *    test below already guarantees. Revisit if the index grows past a few
 *    hundred entries.
 * 3. **Committed literal index + a drift test that re-derives it from the tab
 *    sources.** Chosen. The array below is generated, not hand-typed;
 *    `__tests__/settingsSearchIndex.test.ts` re-extracts it from the `.tsx`
 *    files and fails on any difference, so the index cannot drift silently.
 *    This is the same contract `src/transport.ts`'s `COMMAND_TABLE` uses.
 *
 * ## Extraction rule
 *
 * Mechanical, so the drift test can reproduce it exactly:
 * every `<h3>` or `<SectionCard title=…>` / `<DangerZone title=…>` opens a section; every `label=` prop and every `<label>` element
 * is a setting inside the nearest preceding `<h3>`. Text comes from
 * `t("key", "Default")`, a string literal, or a leading plain-text run. A
 * static `hint=` prop in the same tag as a `label=` prop rides along as search
 * fodder (and result context) — a dynamic hint is simply absent.
 *
 * Two categories are deliberately outside the index, and the drift test pins
 * their counts so a new one cannot slip in unnoticed:
 * - **dynamic** — text computed at runtime (per-agent cards, per-plugin rows);
 *   there is nothing stable to index or to scroll to.
 * - **orphan** — a label with no `<h3>` above it, i.e. a modal form field (the
 *   Smart Prompts editor); it is not a setting and has no scroll target.
 *
 * A label inside `<ExpertSetting configKey="…">` also carries `expert: true`
 * and that `configKey`, so search can badge it and reveal it on open.
 *
 * Repo-scoped tabs (`repo:<path>`) are not indexed: their nav key depends on
 * which repository the user means, and a global search box cannot know.
 *
 * A label inside `<Show when={isTauri()…}>` carries `platform: "desktop"`, one
 * in that Show's `fallback` carries `platform: "browser"`; search offers each
 * client only what it renders.
 *
 * ## Composed pages (mcp, remote-servers)
 *
 * A page rendered by several components is extracted from all of them, in
 * render order: `mcp` is `LocalMcpPanel` then `UpstreamMcpPanel`,
 * `remote-servers` is `RemoteServersTab` with the merged connection editor, then
 * `SshTunnelsSection`, then `RemoteMachinesTab` wrapping `RemoteMachinesPanel`. The
 * drift test holds the source list and checks it against what `SettingsPanel`
 * renders for each tab.
 */
export interface SettingsSearchEntry {
	/** `SettingsShell` nav key of the tab that renders this entry */
	tab: string;
	/** Default text of the `<h3>` this entry lives under; with `label` it is the
	 * scroll anchor `scrollToSetting` looks for in the rendered tab */
	section: string;
	sectionKey?: string;
	/** Setting label; absent when the entry is the section heading itself */
	label?: string;
	labelKey?: string;
	/** The setting sits inside an `ExpertSetting`: results mark it "Expert" */
	expert?: boolean;
	/** That `ExpertSetting`'s configKey — opening the result reveals it */
	configKey?: string;
	/** Rendered by one client only (an `isTauri()` Show or its fallback) */
	platform?: "desktop" | "browser";
	/** Default text of the control's static `hint=` prop, when it has one */
	hint?: string;
	hintKey?: string;
}

/** What a settings deep link scrolls to: rendered heading text, plus the
 * rendered label when it names one control rather than a whole section. */
export interface SettingsSearchTarget {
	section: string;
	label?: string;
	/** The target's `ExpertSetting` configKey — opening the link reveals it */
	configKey?: string;
}

/** Command Palette category of the per-setting deep-link actions. */
export const SETTINGS_SEARCH_CATEGORY = "Settings";

export const SETTINGS_SEARCH_INDEX: SettingsSearchEntry[] = [
	// telegram
	{ tab: "telegram", section: "Telegram" },
	{ tab: "telegram", section: "Bot setup" },
	{ tab: "telegram", section: "Chat access" },
	{ tab: "telegram", section: "Agent registration" },
	{ tab: "telegram", section: "Recent messages" },
	{ tab: "telegram", section: "Bot setup", label: "Enable Telegram", labelKey: "telegram.enabled" },
	{ tab: "telegram", section: "Bot setup", label: "Bot token", labelKey: "telegram.token" },
	{ tab: "telegram", section: "Chat access", label: "Add chat by ID", hint: "Private chat ID" },
	{ tab: "telegram", section: "Chat access", label: "Link a chat" },
	{ tab: "telegram", section: "Agent registration", label: "Registered agent" },
	// general
	{ tab: "general", section: "General" },
	{ tab: "general", section: "Local tools", platform: "desktop" },
	{ tab: "general", section: "AI chat defaults" },
	{ tab: "general", section: "General", sectionKey: "general.heading.general" },
	{ tab: "general", section: "Window", sectionKey: "general.heading.window", platform: "desktop" },
	{ tab: "general", section: "Confirmations", sectionKey: "general.heading.confirmations" },
	{ tab: "general", section: "Power Management", sectionKey: "general.heading.powerManagement" },
	{ tab: "general", section: "Resource Limits" },
	{ tab: "general", section: "Diffs", sectionKey: "general.heading.diffs" },
	{ tab: "general", section: "Updates", sectionKey: "general.heading.updates" },
	{ tab: "general", section: "ego", sectionKey: "general.heading.ego" },
	{ tab: "general", section: "IDE", sectionKey: "developerTools.heading.ide" },
	{ tab: "general", section: "Experimental Features", sectionKey: "general.heading.experimental" },
	{ tab: "general", section: "Local tools", label: "TUIC CLI", labelKey: "general.heading.cli", platform: "desktop" },
	{
		tab: "general",
		section: "Local tools",
		label: "Finder Integration",
		labelKey: "general.heading.finderService",
		platform: "desktop",
	},
	{
		tab: "general",
		section: "Local tools",
		label: "Code Intelligence",
		labelKey: "general.heading.codeIntelligence",
		platform: "desktop",
	},
	{
		tab: "general",
		section: "Local tools",
		label: "Custom Launchers",
		labelKey: "general.heading.customLaunchers",
		platform: "desktop",
	},
	{
		tab: "general",
		section: "General",
		label: "Language",
		labelKey: "general.label.language",
		hint: "Language of the TUICommander interface",
		hintKey: "general.hint.language",
	},
	{
		tab: "general",
		section: "General",
		label: "Show agent context bar",
		hint: "Display the model's current intent, its orchestrator-assigned task, and the last prompt sent to an agent",
	},
	{
		tab: "general",
		section: "Window",
		label: "Restore window size and position on launch",
		labelKey: "general.toggle.restoreWindowGeometry",
		platform: "desktop",
		hint: "Reopen the app window at the same size and position as when it was last closed",
		hintKey: "general.hint.restoreWindowGeometry",
	},
	{
		tab: "general",
		section: "Confirmations",
		label: "When closing the main window",
		labelKey: "general.label.closeBehavior",
		platform: "desktop",
		hint: "Background hides visible windows and keeps sessions running. Quit still confirms when enabled.",
		hintKey: "general.hint.closeBehavior",
	},
	{
		tab: "general",
		section: "Confirmations",
		label: "Tray and menu-bar icon",
		labelKey: "general.label.trayIcon",
		platform: "desktop",
		hint: "Never disables Background. Without a tray host, including stock GNOME, closing minimizes the window so it remains reachable.",
		hintKey: "general.hint.trayIcon",
	},
	{
		tab: "general",
		section: "Confirmations",
		label: "Hide Dock icon in background",
		labelKey: "general.toggle.hideDockIcon",
		platform: "desktop",
		hint: "Restore the Dock icon when TUICommander is shown again",
		hintKey: "general.hint.hideDockIcon",
	},
	{
		tab: "general",
		section: "Confirmations",
		label: "Confirm before quitting",
		labelKey: "general.toggle.confirmBeforeQuit",
		hint: "Confirm real quit when terminal sessions are active",
		hintKey: "general.hint.confirmBeforeQuit",
	},
	{
		tab: "general",
		section: "Confirmations",
		label: "Confirm before closing a tab",
		labelKey: "general.toggle.confirmBeforeClosingTab",
		hint: "Show a confirmation dialog when closing a terminal tab",
		hintKey: "general.hint.confirmBeforeClosingTab",
	},
	{
		tab: "general",
		section: "Power Management",
		label: "Prevent sleep when busy",
		labelKey: "general.toggle.preventSleepWhenBusy",
		hint: "Keep the system awake while scripts are running",
		hintKey: "general.hint.preventSleepWhenBusy",
	},
	{
		tab: "general",
		section: "Power Management",
		label: "Auto-Standby Timeout",
		expert: true,
		configKey: "app.standby_timeout_minutes",
		hint: "Pause idle background sessions after this duration to save resources. 0 = disabled.",
	},
	{
		tab: "general",
		section: "Power Management",
		label: "Content Indexing",
		expert: true,
		configKey: "app.index_strategy",
		hint: "When to build search indexes. Set to Disabled to turn off background indexing entirely.",
	},
	{
		tab: "general",
		section: "Resource Limits",
		label: "Content index memory budget (MB)",
		expert: true,
		configKey: "app.index_memory_budget_mb",
		hint: "Total memory available for content search indexes before older indexes are dropped.",
	},
	{
		tab: "general",
		section: "Resource Limits",
		label: "Maximum attachment size (bytes)",
		expert: true,
		configKey: "app.attachment_max_bytes",
		hint: "Maximum size of one attachment. Existing attachments are not changed.",
	},
	{
		tab: "general",
		section: "Resource Limits",
		label: "Background WebView unload delay (minutes)",
		expert: true,
		configKey: "app.background.webview_unload_minutes",
		platform: "desktop",
		hint: "Unload the hidden WebView after this delay. Zero keeps it loaded; terminal sessions keep running.",
	},
	{
		tab: "general",
		section: "Diffs",
		label: "Ignore leading whitespace",
		labelKey: "general.toggle.diffIgnoreLeadingWhitespace",
		expert: true,
		configKey: "app.diff_ignore_leading_whitespace",
		hint: "Don't show a line as changed if it only differs in leading whitespace",
		hintKey: "general.hint.diffIgnoreLeadingWhitespace",
	},
	{
		tab: "general",
		section: "Diffs",
		label: "Ignore trailing whitespace",
		labelKey: "general.toggle.diffIgnoreTrailingWhitespace",
		expert: true,
		configKey: "app.diff_ignore_trailing_whitespace",
		hint: "Don't show a line as changed if it only differs in trailing whitespace",
		hintKey: "general.hint.diffIgnoreTrailingWhitespace",
	},
	{
		tab: "general",
		section: "Diffs",
		label: "Ignore whitespace amount",
		labelKey: "general.toggle.diffIgnoreWhitespaceAmount",
		expert: true,
		configKey: "app.diff_ignore_whitespace_amount",
		hint: "Treat runs of whitespace as equal regardless of how many characters they contain",
		hintKey: "general.hint.diffIgnoreWhitespaceAmount",
	},
	{
		tab: "general",
		section: "Diffs",
		label: "Ignore case",
		labelKey: "general.toggle.diffIgnoreCase",
		expert: true,
		configKey: "app.diff_ignore_case",
		hint: "Compare lines case-insensitively",
		hintKey: "general.hint.diffIgnoreCase",
	},
	{
		tab: "general",
		section: "Diffs",
		label: "Soft-wrap long lines",
		labelKey: "general.toggle.diffSoftWrap",
		hint: "Wrap long diff lines instead of scrolling horizontally",
		hintKey: "general.hint.diffSoftWrap",
	},
	{
		tab: "general",
		section: "Diffs",
		label: "Auto-open Session Diff Review",
		labelKey: "general.label.sessionDiffAutoOpen",
		expert: true,
		configKey: "app.session_diff_auto_open",
		hint: "When TUIC detects an agent editing files: never open Session Diff Review, ask first, or open it automatically",
		hintKey: "general.hint.sessionDiffAutoOpen",
	},
	{
		tab: "general",
		section: "Diffs",
		label: "Truncate long changes",
		labelKey: "general.label.sessionDiffTruncateLines",
		expert: true,
		configKey: "app.session_diff_truncate_lines",
		hint: "Collapse a single change above this many lines behind a 'Show all' button. 0 = never truncate.",
		hintKey: "general.hint.sessionDiffTruncateLines",
	},
	{
		tab: "general",
		section: "Updates",
		label: "Automatically check for updates",
		labelKey: "general.toggle.autoUpdateEnabled",
		hint: "Download and install updates in the background",
		hintKey: "general.hint.autoUpdateEnabled",
	},
	{
		tab: "general",
		section: "Updates",
		label: "Wait for agents before restarting",
		labelKey: "general.toggle.deferUpdateRelaunch",
		hint: "After an update installs, restart only when no session is busy or waiting for input",
		hintKey: "general.hint.deferUpdateRelaunch",
	},
	{
		tab: "general",
		section: "Updates",
		label: "Resume all agents after an update",
		labelKey: "general.toggle.eagerResumeAfterUpdate",
		hint: "After the app restarts for an update, resume every agent tab with a confirmed session on every branch, without visiting each one",
		hintKey: "general.hint.eagerResumeAfterUpdate",
	},
	{
		tab: "general",
		section: "Updates",
		label: "Update Channel",
		labelKey: "general.label.updateChannel",
		expert: true,
		configKey: "app.update_channel",
	},
	{
		tab: "general",
		section: "ego",
		label: "ego executable",
		labelKey: "general.label.egoExecutable",
		platform: "browser",
		hint: "Path to the ego binary the AI Chat panel talks to over ACP",
		hintKey: "general.hint.egoExecutable",
	},
	{
		tab: "general",
		section: "ego",
		label: "ego executable",
		labelKey: "general.label.egoExecutable",
		platform: "desktop",
	},
	{
		tab: "general",
		section: "ego",
		label: "ego profile",
		labelKey: "general.label.egoProfile",
		hint: "Optional profile from ego's user configuration. Use one name without spaces or a leading dash.",
		hintKey: "general.hint.egoProfile",
	},
	{ tab: "general", section: "ego", label: "AI Chat workspace", labelKey: "general.label.aiChatWorkspace" },
	{
		tab: "general",
		section: "IDE",
		label: "Default IDE",
		labelKey: "general.label.defaultIde",
		hint: "IDE used to open repositories",
		hintKey: "general.hint.defaultIde",
	},
	{
		tab: "general",
		section: "Experimental Features",
		label: "Enable experimental features",
		labelKey: "general.toggle.experimentalFeatures",
		hint: "Opt in to features under active development: the AI Chat panel.",
		hintKey: "general.hint.experimentalFeatures",
	},
	// appearance
	{ tab: "appearance", section: "Appearance" },
	{ tab: "appearance", section: "Tabs", sectionKey: "appearance.heading.tabs" },
	{ tab: "appearance", section: "Chat", sectionKey: "appearance.heading.chat" },
	{ tab: "appearance", section: "Repository Groups", sectionKey: "appearance.heading.groups" },
	{ tab: "appearance", section: "Layout", sectionKey: "appearance.heading.layout" },
	{ tab: "appearance", section: "Bell", sectionKey: "appearance.heading.bell" },
	{ tab: "appearance", section: "UI Legend", sectionKey: "appearance.heading.uiLegend" },
	{
		tab: "appearance",
		section: "Tabs",
		label: "Split Tab Mode",
		labelKey: "appearance.label.splitTabMode",
		hint: "How worktree tabs are arranged in the tab bar",
		hintKey: "appearance.hint.splitTabMode",
	},
	{
		tab: "appearance",
		section: "Tabs",
		label: "Tab Ordering",
		labelKey: "appearance.label.tabOrderingMode",
		hint: "How tabs are ordered: grouped by type, terminals first, or freely interleaved",
		hintKey: "appearance.hint.tabOrderingMode",
	},
	{
		tab: "appearance",
		section: "Tabs",
		label: "Cycle All Tab Types",
		labelKey: "appearance.label.tabCyclingAllTypes",
		hint: "Next/previous tab shortcuts cycle through diff, markdown and editor tabs too — not just terminals",
		hintKey: "appearance.hint.tabCyclingAllTypes",
	},
	{
		tab: "appearance",
		section: "Tabs",
		label: "Nested Terminal Tabs",
		labelKey: "appearance.label.tabTreeEnabled",
		hint: "Show each branch's open sessions and agent activity in a collapsible card under its sidebar row",
		hintKey: "appearance.hint.tabTreeEnabled",
	},
	{
		tab: "appearance",
		section: "Tabs",
		label: "Max Tab Name Length",
		labelKey: "appearance.label.maxTabNameLength",
		hint: "Maximum characters shown in tab names before truncating",
		hintKey: "appearance.hint.maxTabNameLength",
	},
	{
		tab: "appearance",
		section: "Chat",
		label: "New text indicator",
		labelKey: "appearance.label.newTextIndicator",
		hint: "Show New text below when a chat receives content while you read earlier messages. Saved on this client.",
		hintKey: "appearance.hint.newTextIndicator",
	},
	{
		tab: "appearance",
		section: "Chat",
		label: "Mute Intermediate Text",
		labelKey: "appearance.label.chatMuteIntermediateText",
		hint: "Show assistant text that is followed by a tool call in the same turn smaller and dimmer, so the final answer stands out",
		hintKey: "appearance.hint.chatMuteIntermediateText",
	},
	{
		tab: "appearance",
		section: "Bell",
		label: "Bell Style",
		labelKey: "appearance.label.bellStyle",
		hint: "How the terminal bell (\\a / BEL) is signaled",
		hintKey: "appearance.hint.bellStyle",
	},
	// notifications
	{ tab: "notifications", section: "Notifications" },
	{ tab: "notifications", section: "Notification Settings", sectionKey: "notifications.heading.notificationSettings" },
	{ tab: "notifications", section: "Visual notifications" },
	{ tab: "notifications", section: "Tasks", sectionKey: "notifications.heading.tasks" },
	{
		tab: "notifications",
		section: "Notification Settings",
		label: "Enable audio notifications",
		labelKey: "notifications.toggle.enableAudio",
	},
	{
		tab: "notifications",
		section: "Notification Settings",
		label: "Master Volume",
		labelKey: "notifications.label.masterVolume",
		expert: true,
		configKey: "notifications.volume",
		hint: "Overall volume for all notification sounds",
		hintKey: "notifications.hint.masterVolume",
	},
	{
		tab: "notifications",
		section: "Notification Settings",
		label: "Audio Output Device",
		labelKey: "notifications.label.audioDevice",
		expert: true,
		configKey: "notifications.audio_device",
		platform: "desktop",
	},
	{
		tab: "notifications",
		section: "Notification Settings",
		label: "Notification Events",
		labelKey: "notifications.label.notificationEvents",
	},
	{
		tab: "notifications",
		section: "Notification Settings",
		label: "Orchestration",
		labelKey: "notifications.label.orchestration",
	},
	{
		tab: "notifications",
		section: "Notification Settings",
		label: "Silence completions from MCP sessions",
		labelKey: "notifications.label.silenceRemoteCompletions",
	},
	{
		tab: "notifications",
		section: "Visual notifications",
		label: "Toolbar Bell",
		labelKey: "notifications.label.toolbarBell",
	},
	{
		tab: "notifications",
		section: "Visual notifications",
		label: "Keep toasts in the bell",
		labelKey: "notifications.toggle.toastsInBell",
	},
	{
		tab: "notifications",
		section: "Visual notifications",
		label: "Show cards from the active terminal",
		labelKey: "notifications.toggle.autoToastCards",
		hint: "Show normal agent toast cards only on their active origin terminal tab. Other tabs receive bell entries only. Urgent notices always show a card. Saved for this client.",
		hintKey: "notifications.hint.autoToastCards",
	},
	{
		tab: "notifications",
		section: "Visual notifications",
		label: "Pull Requests",
		labelKey: "notifications.label.prNotifications",
	},
	{
		tab: "notifications",
		section: "Visual notifications",
		label: "OS notification for PR changes",
		labelKey: "notifications.toggle.prNative",
	},
	{
		tab: "notifications",
		section: "Tasks",
		label: "OS notifications for Tasks",
		labelKey: "notifications.toggle.tasks",
		hint: "Off by default. Notify for failed checks on linked GitHub PRs and new comments on followed tasks during existing detail refreshes. No background item polling. Saved with Tasks preferences.",
		hintKey: "notifications.hint.tasks",
	},
	// terminal
	{ tab: "terminal", section: "Terminal" },
	{ tab: "terminal", section: "Theme", sectionKey: "appearance.heading.theme" },
	{ tab: "terminal", section: "Terminal", sectionKey: "general.heading.terminal" },
	{ tab: "terminal", section: "Preview" },
	{ tab: "terminal", section: "Behavior" },
	{ tab: "terminal", section: "Shell Integration", sectionKey: "terminal.heading.shellIntegration" },
	{
		tab: "terminal",
		section: "Custom Environment Variables",
		sectionKey: "terminal.heading.customEnv",
	},
	{ tab: "terminal", section: "Session Restore", sectionKey: "terminal.heading.sessionRestore" },
	{
		tab: "terminal",
		section: "Theme",
		label: "Terminal Theme",
		labelKey: "appearance.label.terminalTheme",
		hint: "Color theme for terminal output and app chrome",
		hintKey: "appearance.hint.terminalTheme",
	},
	{
		tab: "terminal",
		section: "Terminal",
		label: "Shell",
		labelKey: "general.label.shell",
		expert: true,
		configKey: "app.shell",
		hint: "Shell used in terminals (leave blank for system default)",
		hintKey: "general.hint.shell",
	},
	{
		tab: "terminal",
		section: "Terminal",
		label: "Terminal Font",
		labelKey: "appearance.label.terminalFont",
		hint: "Monospace font for terminals",
		hintKey: "appearance.hint.terminalFont",
	},
	{
		tab: "terminal",
		section: "Terminal",
		label: "Default Font Size",
		labelKey: "appearance.label.defaultFontSize",
		hint: "Default font size for new terminals",
		hintKey: "appearance.hint.defaultFontSize",
	},
	{
		tab: "terminal",
		section: "Terminal",
		label: "Font Weight",
		labelKey: "appearance.label.fontWeight",
		expert: true,
		configKey: "app.font_weight",
		hint: "Terminal font weight (200 = ExtraLight, 400 = Regular, 700 = Bold)",
		hintKey: "appearance.hint.fontWeight",
	},
	{
		tab: "terminal",
		section: "Terminal",
		label: "Cursor Style",
		labelKey: "appearance.label.cursorStyle",
		hint: "Shape of the terminal cursor. Applies immediately to all terminals.",
		hintKey: "appearance.hint.cursorStyle",
	},
	{
		tab: "terminal",
		section: "Behavior",
		label: "Copy on select",
		labelKey: "general.toggle.copyOnSelect",
		hint: "Automatically copy selected text to clipboard",
		hintKey: "general.hint.copyOnSelect",
	},
	{
		tab: "terminal",
		section: "Behavior",
		label: "Allow OSC 52 clipboard writes",
		labelKey: "general.toggle.osc52Clipboard",
		expert: true,
		configKey: "app.osc52_clipboard",
		hint: "Let terminal programs set the system clipboard (OSC 52). A notice appears on each write. Disable to ignore clipboard writes from terminal output.",
		hintKey: "general.hint.osc52Clipboard",
	},
	{
		tab: "terminal",
		section: "Behavior",
		label: "Allow terminal focus/attention requests",
		labelKey: "general.toggle.osc1337FocusAttention",
		expert: true,
		configKey: "app.osc1337_focus_attention",
		hint: "Let terminal programs bring the window to the front or bounce the dock icon (OSC 1337 StealFocus/RequestAttention). Disable if a script or log spams either.",
		hintKey: "general.hint.osc1337FocusAttention",
	},
	{
		tab: "terminal",
		section: "Behavior",
		label: "Open links on",
		labelKey: "terminal.label.linkActivation",
		hint: "How links (URLs, file paths) in terminal output open. Click opens on a plain click; {mod}Click underlines a link only while {key} is held, and opens it on {mod}+click; Never disables click-to-open — right-click still offers Open/Copy link.",
		hintKey: "terminal.hint.linkActivation",
	},
	{
		tab: "terminal",
		section: "Behavior",
		label: "Show block timestamps",
		labelKey: "terminal.label.blockTimestampMode",
	},
	{
		tab: "terminal",
		section: "Behavior",
		label: "Block folding",
		labelKey: "general.toggle.blockFolding",
		expert: true,
		configKey: "app.block_folding_enabled",
		hint: "Let the Toggle Block Fold shortcut collapse a command block's output. Already-folded blocks stay collapsed when this is off.",
		hintKey: "general.hint.blockFolding",
	},
	{
		tab: "terminal",
		section: "Behavior",
		label: "Show block marks",
		labelKey: "general.toggle.showBlockMarks",
		expert: true,
		configKey: "app.show_block_marks",
		hint: "Tick marks on the scrollbar for each command block — red when the command failed.",
		hintKey: "general.hint.showBlockMarks",
	},
	{
		tab: "terminal",
		section: "Behavior",
		label: "Show prompt marks",
		labelKey: "general.toggle.showPromptMarks",
		expert: true,
		configKey: "app.show_prompt_marks",
		hint: "A green tick mark on the scrollbar for each prompt you sent.",
		hintKey: "general.hint.showPromptMarks",
	},
	{
		tab: "terminal",
		section: "Behavior",
		label: "Reflow scrollback on resize",
		labelKey: "general.toggle.scrollbackReflow",
		expert: true,
		configKey: "app.scrollback_reflow",
		hint: "Re-wrap scrollback history when the terminal changes width, so old output stays readable after a side panel opens. Turn it off to leave history lines as they were written and truncate them instead. The visible screen is never reflowed either way.",
		hintKey: "general.hint.scrollbackReflow",
	},
	{
		tab: "terminal",
		section: "Custom Environment Variables",
		label: "Environment Variables",
		labelKey: "terminal.label.customEnv",
		expert: true,
		configKey: "app.custom_pty_env",
	},
	{
		tab: "terminal",
		section: "Session Restore",
		label: "Restore open terminals on launch",
		labelKey: "terminal.toggle.restoreShellTerminals",
		hint: "Reopen plain shell tabs (not just agent tabs) in their saved directory when you relaunch",
		hintKey: "terminal.hint.restoreShellTerminals",
	},
	{
		tab: "terminal",
		section: "Session Restore",
		label: "Save terminal scrollback",
		labelKey: "terminal.toggle.restoreScrollback",
		hint: "Show a restored terminal's recent output above a fresh prompt. Saved as plain text in the app's config directory — off by default.",
		hintKey: "terminal.hint.restoreScrollback",
	},
	{
		tab: "terminal",
		section: "Session Restore",
		label: "Scrollback lines to save",
		labelKey: "terminal.label.restoreScrollbackLines",
		hint: "Maximum lines of output saved per terminal when scrollback saving is on",
		hintKey: "terminal.hint.restoreScrollbackLines",
	},
	// selection
	{ tab: "selection", section: "Behavior" },
	{ tab: "selection", section: "Word Boundaries" },
	{ tab: "selection", section: "Smart Selection Rules" },
	{
		tab: "selection",
		section: "Behavior",
		label: "Double-click performs",
		hint: "Word selection expands to the character-class boundary below. Smart selection tries the rule list first, falling back to word selection when nothing matches. Quad-click (4 rapid clicks) and the right-click smart-selection menu always try the rule list, regardless of this setting.",
	},
	{
		tab: "selection",
		section: "Word Boundaries",
		label: "Word boundaries",
		hint: "Character list: a literal set of characters that BREAK a word (today's punctuation set, by default). Regular expression: `|`-joined alternates — the longest match at each position joins onto the adjacent word, e.g. adding https:// lets a double-click on a URL's host include the scheme.",
	},
	{
		tab: "selection",
		section: "Word Boundaries",
		label: "Word separators",
		hint: "Characters that break a word for double-click selection. Whitespace and control characters are always separators regardless of this list.",
	},
	{
		tab: "selection",
		section: "Word Boundaries",
		label: "Word pattern",
		hint: "`|`-joined alternates. Plain letters/digits/underscore are always word characters; add alternates here to join punctuation-containing spans onto them.",
	},
	// keyboard-shortcuts
	{ tab: "keyboard-shortcuts", section: "Keyboard Shortcuts", sectionKey: "settings.keyboardShortcuts" },
	{ tab: "keyboard-shortcuts", section: "Global Hotkey", platform: "desktop" },
	{ tab: "keyboard-shortcuts", section: "Shortcut groups" },
	{
		tab: "keyboard-shortcuts",
		section: "Global Hotkey",
		label: "Global Hotkey (Toggle Window)",
		labelKey: "settings.globalHotkey",
		platform: "desktop",
		hint: "Set an OS-level shortcut to show/hide TUICommander from any application.",
		hintKey: "settings.globalHotkeyHint",
	},
	{
		tab: "keyboard-shortcuts",
		section: "Shortcut groups",
		label: "Plugin Commands",
		labelKey: "helpPanel.pluginCommands",
	},
	// dictation
	{ tab: "dictation", section: "Voice" },
	{ tab: "dictation", section: "Dictation", sectionKey: "dictation.heading.dictation" },
	{ tab: "dictation", section: "Auto-Corrections", sectionKey: "dictation.heading.corrections" },
	{ tab: "dictation", section: "Speech recognition", sectionKey: "dictation.heading.recognition" },
	{ tab: "dictation", section: "Speech assets" },
	{ tab: "dictation", section: "Spoken replies", sectionKey: "dictation.heading.spokenReplies" },
	{ tab: "dictation", section: "Hands-free conversation", sectionKey: "dictation.heading.handsFree" },
	{
		tab: "dictation",
		section: "Dictation",
		label: "Enable Dictation",
		labelKey: "dictation.enableLabel",
		hint: "Enable voice-to-text dictation",
		hintKey: "dictation.enableHint",
	},
	{
		tab: "dictation",
		section: "Dictation",
		label: "Hotkey",
		labelKey: "dictation.hotkeyLabel",
		platform: "desktop",
		hint: "Hold the hotkey to start recording, release to stop. Short presses pass through as normal input.",
		hintKey: "dictation.hotkeyHint",
	},
	{
		tab: "dictation",
		section: "Dictation",
		label: "Long-press threshold",
		labelKey: "dictation.longPressLabel",
		expert: true,
		configKey: "dictation.long_press_ms",
		platform: "desktop",
		hint: "How long to hold the key before dictation starts. 0 = instant (no short-press pass-through), higher = fewer accidental triggers.",
		hintKey: "dictation.longPressHint",
	},
	{
		tab: "dictation",
		section: "Dictation",
		label: "Auto-send",
		labelKey: "dictation.autoSendLabel",
		expert: true,
		configKey: "dictation.auto_send",
		hint: "Automatically press Enter after inserting transcribed text",
		hintKey: "dictation.autoSendHint",
	},
	{
		tab: "dictation",
		section: "Auto-Corrections",
		label: "Add correction",
		hint: "Replace heard text with the spelling you want.",
	},
	{
		tab: "dictation",
		section: "Speech recognition",
		label: "Input device",
		labelKey: "dictation.inputDeviceLabel",
		expert: true,
		configKey: "dictation.device",
		platform: "desktop",
	},
	{ tab: "dictation", section: "Speech recognition", label: "Language", labelKey: "dictation.languageLabel" },
	{
		tab: "dictation",
		section: "Speech recognition",
		label: "Level gate",
		labelKey: "dictation.rmsLabel",
		expert: true,
		configKey: "dictation.rms_threshold",
		hint: "Audio below this level is ignored.",
		hintKey: "dictation.rmsHint",
	},
	{
		tab: "dictation",
		section: "Speech recognition",
		label: "Speech confidence gate",
		labelKey: "dictation.noSpeechLabel",
		expert: true,
		configKey: "dictation.no_speech_threshold",
		hint: "Discards a transcript when Whisper itself reports it probably heard no speech. Lower is stricter; 100% turns the gate off.",
		hintKey: "dictation.noSpeechHint",
	},
	{
		tab: "dictation",
		section: "Speech assets",
		label: "Whisper Model",
		labelKey: "dictation.modelLabel",
		hint: "Choose a model. Larger models are more accurate but slower.",
		hintKey: "dictation.modelHint",
	},
	{
		tab: "dictation",
		section: "Speech assets",
		label: "Voice tuning",
		labelKey: "dictation.tuningLabel",
		hint: "Record a test phrase and watch where your voice sits against the gates. Text stays in this panel — nothing is sent to a terminal.",
		hintKey: "dictation.tuningHint",
	},
	{
		tab: "dictation",
		section: "Spoken replies",
		label: "Replies are spoken in",
		labelKey: "dictation.speechLanguageLabel",
	},
	{
		tab: "dictation",
		section: "Spoken replies",
		label: "Speech engine",
		labelKey: "dictation.speechEngineLabel",
		expert: true,
		configKey: "dictation.speech_engine",
		hint: "Edge is the default. Pocket TTS runs entirely on this machine after a download. An external command is your own engine.",
		hintKey: "dictation.speechEngineHint",
	},
	{
		tab: "dictation",
		section: "Spoken replies",
		label: "Speech command",
		labelKey: "dictation.speechCommandLabel",
		expert: true,
		configKey: "dictation.speech_command",
	},
	{
		tab: "dictation",
		section: "Spoken replies",
		label: "Voice",
		labelKey: "dictation.voiceLabel",
		hint: "Changing the voice stops any reply already being spoken — a sentence half said in one voice does not finish in another.",
		hintKey: "dictation.voiceHint",
	},
	{
		tab: "dictation",
		section: "Spoken replies",
		label: "Voice volume",
		labelKey: "dictation.voiceVolumeLabel",
		expert: true,
		configKey: "dictation.speech_volume_db",
		hint: "How loud every reply is spoken. Peaks are limited, so a high level never clips. Applies to the next reply.",
		hintKey: "dictation.voiceVolumeHint",
	},
	{
		tab: "dictation",
		section: "Spoken replies",
		label: "Levelling",
		labelKey: "dictation.levellingLabel",
		expert: true,
		configKey: "dictation.speech_levelling",
		hint: "Evens out quiet and loud words within a reply. Off keeps the voice as recorded.",
		hintKey: "dictation.levellingHint",
	},
	{ tab: "dictation", section: "Spoken replies", label: "Voices", labelKey: "dictation.voicesLabel" },
	{ tab: "dictation", section: "Hands-free conversation", label: "Terminal" },
	{
		tab: "dictation",
		section: "Hands-free conversation",
		label: "Activation phrase",
		labelKey: "dictation.activationPhraseLabel",
		hint: "When set, only speech that opens with this phrase is sent, and the phrase itself is removed first. The match runs on this machine, so unrelated speech never leaves it. Try “computer”. Leave empty to send every utterance.",
		hintKey: "dictation.activationPhraseHint",
	},
	{
		tab: "dictation",
		section: "Hands-free conversation",
		label: "Hold-back before sending",
		labelKey: "dictation.holdBackLabel",
		expert: true,
		configKey: "dictation.hands_free_hold_back_ms",
		hint: "How long a finished utterance is shown before it is sent, so you can stop one you did not mean. Applies to the next conversation, not the one already running.",
		hintKey: "dictation.holdBackHint",
	},
	{
		tab: "dictation",
		section: "Hands-free conversation",
		label: "Hands-free listening",
		labelKey: "dictation.handsFreeListeningLabel",
		hint: "Keep the microphone open and send each utterance by itself. Turn off to dictate with push-to-talk only; with spoken replies on, the agent still answers out loud. Applies to the next conversation.",
		hintKey: "dictation.handsFreeListeningHint",
	},
	{
		tab: "dictation",
		section: "Hands-free conversation",
		label: "Spoken replies",
		labelKey: "dictation.spokenRepliesLabel",
		hint: "Let the agent answer out loud. Turn off to keep dictation and receive text replies.",
		hintKey: "dictation.spokenRepliesHint",
	},
	{
		tab: "dictation",
		section: "Hands-free conversation",
		label: "Earcons",
		labelKey: "dictation.earconsLabel",
		hint: "Play a short sound when a turn is sent to the agent, and a lower one when a turn without the activation phrase is dropped. Only the device you talk into plays them.",
		hintKey: "dictation.earconsHint",
	},
	{
		tab: "dictation",
		section: "Hands-free conversation",
		label: "Notify model when hands-free changes",
		labelKey: "dictation.notifyModelLabel",
		expert: true,
		configKey: "dictation.hands_free_notify_model",
		hint: "Tell the agent when a hands-free conversation starts, so it answers out loud, and when it ends, so it goes back to text. Turning this off never leaves speech running: disarming always stops it.",
		hintKey: "dictation.notifyModelHint",
	},
	{
		tab: "dictation",
		section: "Hands-free conversation",
		label: "Start notice",
		labelKey: "dictation.startNoticeLabel",
		expert: true,
		configKey: "dictation.hands_free_start_notice",
		hint: "What the agent reads when a conversation starts. Leave it empty to send the built-in text shown in grey.",
		hintKey: "dictation.startNoticeHint",
	},
	// github
	{ tab: "github", section: "Git & GitHub" },
	{ tab: "github", section: "GitHub Authentication" },
	{ tab: "github", section: "Pull Requests" },
	{ tab: "github", section: "Issues" },
	{ tab: "github", section: "Repository Defaults" },
	{ tab: "github", section: "Worktree Defaults" },
	{ tab: "github", section: "Additional GitHub Accounts" },
	{ tab: "github", section: "CircleCI" },
	{ tab: "github", section: "Repository Bindings" },
	{
		tab: "github",
		section: "Pull Requests",
		label: "Auto-show PR popover",
		hint: "Automatically open the PR panel when a branch has an associated pull request",
	},
	{
		tab: "github",
		section: "Pull Requests",
		label: "Hide Draft PRs",
		hint: "Exclude draft pull requests from the Pull Requests list",
	},
	{
		tab: "github",
		section: "Pull Requests",
		label: "Hide Conflicting PRs",
		hint: "Exclude pull requests with merge conflicts from the Pull Requests list",
	},
	{
		tab: "github",
		section: "Pull Requests",
		label: "Hide CI Failing PRs",
		hint: "Exclude pull requests with failing CI checks from the Pull Requests list",
	},
	{
		tab: "github",
		section: "Pull Requests",
		label: "Auto-Delete on PR Close",
		expert: true,
		configKey: "repo_defaults.auto_delete_on_pr_close",
		hint: "Delete local branch when its PR is merged or closed on GitHub",
	},
	{ tab: "github", section: "Issues", label: "Show issues", hint: "Display the Issues section in the GitHub panel" },
	{ tab: "github", section: "Issues", label: "Issue Filter", hint: "Which issues to show in the GitHub panel" },
	{
		tab: "github",
		section: "Repository Defaults",
		label: "Default Base Branch",
		hint: "Default base branch for new worktrees",
	},
	{
		tab: "github",
		section: "Repository Defaults",
		label: "Copy ignored files",
		expert: true,
		configKey: "repo_defaults.copy_ignored_files",
	},
	{
		tab: "github",
		section: "Repository Defaults",
		label: "Copy untracked files",
		expert: true,
		configKey: "repo_defaults.copy_untracked_files",
	},
	{
		tab: "github",
		section: "Repository Defaults",
		label: "Warm ignored build directories",
		expert: true,
		configKey: "repo_defaults.warm_ignored_directories",
		hint: "Copy-on-write copies node_modules, target, and other git-ignored build directories from the parent repo into a new worktree so it starts warm. Runs in the background after creation.",
	},
	{ tab: "github", section: "Repository Defaults", label: "Default Setup Script" },
	{ tab: "github", section: "Repository Defaults", label: "Default Run Script" },
	{ tab: "github", section: "Repository Defaults", label: "Default Archive Script" },
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "Storage Strategy",
		expert: true,
		configKey: "repo_defaults.worktree_storage",
		hint: "Where to create worktree directories",
	},
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "Prompt for branch name during creation",
		hint: "Show dialog when creating worktrees from '+' button. When off, creates instantly with auto-generated name",
	},
	{ tab: "github", section: "Worktree Defaults", label: "Delete local branch when removing worktree" },
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "Auto-archive merged worktrees",
		expert: true,
		configKey: "repo_defaults.auto_archive_merged",
		hint: "Move worktree to archive directory when its PR is merged",
	},
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "Orphan Worktree Cleanup",
		expert: true,
		configKey: "repo_defaults.orphan_cleanup",
		hint: "Handle worktrees whose branch was deleted",
	},
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "Safe orphan cleanup countdown",
		expert: true,
		configKey: "repo_defaults.orphan_cleanup_countdown_seconds",
		hint: "Automatically remove clean orphaned worktrees after this countdown",
	},
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "PR Merge Strategy",
		hint: "Default merge strategy for worktree branches",
	},
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "After Merge Behavior",
		expert: true,
		configKey: "repo_defaults.after_merge",
		hint: "What to do with the worktree after merging its branch",
	},
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "Auto-Fetch Interval",
		expert: true,
		configKey: "repo_defaults.auto_fetch_interval_minutes",
		hint: "Periodically fetch from remote to detect upstream changes",
	},
	{ tab: "github", section: "Additional GitHub Accounts", label: "Add another github.com account" },
	{ tab: "github", section: "Additional GitHub Accounts", label: "Add Enterprise account" },
	// mcp
	{ tab: "mcp", section: "MCP" },
	{ tab: "mcp", section: "Server connections" },
	{ tab: "mcp", section: "TUIC MCP Server" },
	{ tab: "mcp", section: "Upstream MCP Servers" },
	{ tab: "mcp", section: "Server connections", label: "Server Status", labelKey: "services.label.serverStatus" },
	{
		tab: "mcp",
		section: "TUIC MCP Server",
		label: "Collapse tools — Speakeasy MCP (reduces AI context ~98%)",
		expert: true,
		configKey: "app.collapse_tools",
	},
	{ tab: "mcp", section: "TUIC MCP Server", label: "Capabilities" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Upstreams on" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Authentication" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Timeout (s):" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "URL" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Bearer token" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "OAuth client ID" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Client Secret" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Scopes" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Command" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Args" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Working directory" },
	// remote-access
	{ tab: "remote-access", section: "Remote Access" },
	{ tab: "remote-access", section: "File Access", sectionKey: "services.heading.fileAccess" },
	{ tab: "remote-access", section: "Connection status" },
	{ tab: "remote-access", section: "Tailscale HTTPS" },
	{ tab: "remote-access", section: "Self-Signed HTTPS", sectionKey: "services.heading.selfSignedHttps" },
	{ tab: "remote-access", section: "Danger zone" },
	{ tab: "remote-access", section: "Cloud Relay", sectionKey: "services.heading.cloudRelay" },
	{
		tab: "remote-access",
		section: "File Access",
		label: "Additional Readable Directories",
		labelKey: "services.label.additionalReadableDirs",
		hint: "Absolute directories that web and remote clients may READ files from, in addition to your registered repositories. Desktop reads are never restricted. Never widens writing, copying, or moving. Use ~ for your home directory.",
		hintKey: "services.hint.additionalReadableDirs",
	},
	{
		tab: "remote-access",
		section: "Connection status",
		label: "Enable remote access",
		labelKey: "services.toggle.enableRemoteAccess",
		hint: "Warning: exposes a web interface on your local network. Secure with a strong password.",
		hintKey: "services.hint.remoteAccessWarning",
	},
	{
		tab: "remote-access",
		section: "Connection status",
		label: "Port",
		labelKey: "services.label.port",
		expert: true,
		configKey: "app.services.server.port",
	},
	{ tab: "remote-access", section: "Connection status", label: "Username", labelKey: "services.label.username" },
	{ tab: "remote-access", section: "Connection status", label: "Password", labelKey: "services.label.password" },
	{
		tab: "remote-access",
		section: "Connection status",
		label: "Network Interface",
		labelKey: "services.label.networkInterface",
	},
	{
		tab: "remote-access",
		section: "Connection status",
		label: "Session Token Duration",
		labelKey: "services.label.tokenDuration",
		expert: true,
		configKey: "app.services.auth.session_token_duration_secs",
		hint: "How long a device may stay unused before it must log in again. Every use renews it.",
		hintKey: "services.hint.tokenDuration",
	},
	{
		tab: "remote-access",
		section: "Connection status",
		label: "Enable IPv6 (dual-stack)",
		labelKey: "services.toggle.enableIpv6",
		expert: true,
		configKey: "app.services.server.ipv6_enabled",
		hint: "Binds the server to both IPv4 and IPv6 addresses. Requires save + server restart.",
		hintKey: "services.hint.ipv6Description",
	},
	{ tab: "remote-access", section: "Tailscale HTTPS", label: "Status", labelKey: "services.label.tailscaleStatus" },
	{ tab: "remote-access", section: "Self-Signed HTTPS", label: "Status", labelKey: "services.label.selfSignedStatus" },
	{
		tab: "remote-access",
		section: "Self-Signed HTTPS",
		label: "Fingerprint",
		labelKey: "services.label.selfSignedFingerprint",
	},
	{
		tab: "remote-access",
		section: "Cloud Relay",
		label: "Enable cloud relay",
		labelKey: "services.toggle.enableRelay",
		hint: "Connect from anywhere via an encrypted WebSocket relay. No port forwarding or VPN needed. Note: traffic is encrypted in transit, but the relay operator can derive the key — this is not end-to-end encryption.",
		hintKey: "services.hint.relayDescription",
	},
	{ tab: "remote-access", section: "Cloud Relay", label: "Relay Server URL", labelKey: "services.label.relayUrl" },
	{
		tab: "remote-access",
		section: "Cloud Relay",
		label: "Bearer Token",
		labelKey: "services.label.relayToken",
		hint: "Obtained from the relay server's /register endpoint. Used for both authentication and encryption key derivation — because the relay receives this token, it can derive the key, so traffic is not end-to-end encrypted.",
		hintKey: "services.hint.relayToken",
	},
	{ tab: "remote-access", section: "Cloud Relay", label: "Session ID", labelKey: "services.label.relaySessionId" },
	// remote-servers
	{ tab: "remote-servers", section: "Remote Servers", sectionKey: "remoteServers.heading" },
	{ tab: "remote-servers", section: "SSH Port-Forwarding Tunnels", sectionKey: "remoteServers.sshTunnels" },
	{ tab: "remote-servers", section: "Remote Machines", sectionKey: "settings.remoteMachines" },
	{ tab: "remote-servers", section: "Your machines" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Auto-update remote daemons" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Auth username (optional)" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Auth password (optional)" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Name" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Kind" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Connect automatically on startup" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Remote daemon port" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Deployment" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Keep ephemeral daemon alive (minutes)" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Instance ID (optional)" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Offer to start the remote daemon if it is not running" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Leave it running on disconnect" },
	{ tab: "remote-servers", section: "Remote Servers", label: "URL" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Target" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Instance ID" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Port" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Host" },
	{ tab: "remote-servers", section: "Remote Servers", label: "User" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Identity / Authentication" },
	{ tab: "remote-servers", section: "Remote Servers", label: "ServerAliveInterval" },
	{ tab: "remote-servers", section: "Remote Servers", label: "ServerAliveCountMax" },
	{ tab: "remote-servers", section: "Remote Servers", label: "StrictHostKeyChecking" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Compress the channel (ssh -C)" },
	{ tab: "remote-servers", section: "Remote Servers", label: "Port Forwards" },
	// streamdock
	{ tab: "streamdock", section: "StreamDock" },
	{ tab: "streamdock", section: "StreamDock M18" },
	{ tab: "streamdock", section: "Pinned sessions" },
	{
		tab: "streamdock",
		section: "StreamDock M18",
		label: "Enable StreamDock integration",
		hint: "Attaches to the first connected StreamDock M18 (or the selected device below) and starts mirroring session state to its keys.",
	},
	{ tab: "streamdock", section: "StreamDock M18", label: "StreamDock status" },
	{ tab: "streamdock", section: "StreamDock M18", label: "Device" },
	{ tab: "streamdock", section: "StreamDock M18", label: "Screen brightness" },
	{
		tab: "streamdock",
		section: "StreamDock M18",
		label: "LED brightness",
		hint: "Only applies on firmware that reports RGB support (V3-class M18 units).",
	},
	// plugins
	{ tab: "plugins", section: "Manage plugins" },
	{ tab: "plugins", section: "Installed plugins" },
	{ tab: "plugins", section: "Community registry" },
	{
		tab: "plugins",
		section: "Installed plugins",
		label: "Check for plugin updates",
		hint: "Fetch the registry at startup and show available updates",
	},
	// smart-prompts
	{ tab: "smart-prompts", section: "Delete prompt" },
	{ tab: "smart-prompts", section: "Smart Prompts" },
	{ tab: "smart-prompts", section: "Background agent" },
	{ tab: "smart-prompts", section: "Prompt library" },
	{
		tab: "smart-prompts",
		section: "Background agent",
		label: "Headless Agent",
		hint: "Default agent for headless prompts. Individual prompts can override this in their settings.",
	},
	// agents
	{ tab: "agents", section: "Terminal agents" },
	{ tab: "agents", section: "Agent behavior" },
	{ tab: "agents", section: "ego permissions" },
	{ tab: "agents", section: "Idle agent hibernation", sectionKey: "settings.agents.hibernateHeading" },
	{ tab: "agents", section: "Launch modules" },
	{ tab: "agents", section: "Terminal agents", label: "Configure agents on" },
	{
		tab: "agents",
		section: "Agent behavior",
		label: "Show agent intent as tab title",
		hint: "When agents declare their current work phase, update the tab name with a short title",
	},
	{
		tab: "agents",
		section: "Agent behavior",
		label: "Show suggested follow-up actions",
		hint: "Display actionable suggestions from agents after completing a task",
	},
	{
		tab: "agents",
		section: "Agent behavior",
		label: "Collect project progress",
		expert: true,
		configKey: "app.progress_tracking",
		hint: "Keep a per-project journal of what agents finished, what blocked them, and what they set out to do. Off removes the progress tool from every agent.",
	},
	{ tab: "agents", section: "Agent behavior", label: "Launch instructions" },
	{ tab: "agents", section: "ego permissions", label: "Permissions" },
	{ tab: "agents", section: "ego permissions", label: "Filesystem sandbox" },
	{
		tab: "agents",
		section: "Idle agent hibernation",
		label: "Close abandoned shells after (hours)",
		labelKey: "settings.agents.abandonedHours",
		expert: true,
		configKey: "agents.abandoned_shell_hours",
	},
	{
		tab: "agents",
		section: "Idle agent hibernation",
		label: "Hibernate idle agents after",
		labelKey: "settings.agents.hibernateMinutes",
		expert: true,
		configKey: "agents.hibernate_idle_minutes",
	},
	{
		tab: "agents",
		section: "Idle agent hibernation",
		label: "Hibernate scope",
		labelKey: "settings.agents.hibernateScope",
		expert: true,
		configKey: "agents.hibernate_scope",
	},
	{ tab: "agents", section: "Launch modules", label: "Turn all launch instructions off" },
	{ tab: "agents", section: "Launch modules", label: "Markers" },
	{ tab: "agents", section: "Launch modules", label: "Working with the user" },
	{ tab: "agents", section: "Launch modules", label: "Waiting and long work" },
	{ tab: "agents", section: "Launch modules", label: "Git and worktrees" },
	{ tab: "agents", section: "Launch modules", label: "Process safety" },
	{ tab: "agents", section: "Launch modules", label: "Children (spawned children only)" },
	// ai-chat
	{ tab: "ai-chat", section: "Workspace access" },
	{ tab: "ai-chat", section: "Network and sandbox" },
	{ tab: "ai-chat", section: "Advanced and diagnostics" },
	{ tab: "ai-chat", section: "AI Chat" },
	{ tab: "ai-chat", section: "Agent and model" },
	{ tab: "ai-chat", section: "Workspace access", label: "Workspace" },
	{
		tab: "ai-chat",
		section: "Workspace access",
		label: "Root directory",
		labelKey: "perimeter.label.rootDir",
		hint: "Leave all paths empty for workspace-only roots. An absent roots setting defaults to ~/Gits when that directory exists.",
		hintKey: "perimeter.emptyRoot",
	},
	{ tab: "ai-chat", section: "Workspace access", label: "Root access", labelKey: "perimeter.label.rootAccess" },
	{
		tab: "ai-chat",
		section: "Workspace access",
		label: "Read allowlist",
		labelKey: "perimeter.label.readAllowlist",
		hint: "One absolute path or ~/ path per line. These entries grant read access; they do not revoke a writable parent root.",
	},
	{
		tab: "ai-chat",
		section: "Workspace access",
		label: "Extra writable directories",
		labelKey: "perimeter.label.writableDirs",
		hint: "One absolute path or ~/ path per line. Read-only entries do not revoke access granted by a containing writable root. ego validates roots and system ceilings.",
		hintKey: "perimeter.paths",
	},
	{ tab: "ai-chat", section: "Network and sandbox", label: "Network", hint: "Network access for new ego runs." },
	{ tab: "ai-chat", section: "Agent and model", label: "Default model", labelKey: "providers.label.defaultModel" },
	{ tab: "ai-chat", section: "Agent and model", label: "Provider catalogue" },
];

/** Section heading as rendered, i18n applied. */
export function entrySection(entry: SettingsSearchEntry): string {
	return entry.sectionKey ? t(entry.sectionKey, entry.section) : entry.section;
}

/** Setting label as rendered, i18n applied; undefined for a section entry. */
export function entryLabel(entry: SettingsSearchEntry): string | undefined {
	if (entry.label === undefined) return undefined;
	return entry.labelKey ? t(entry.labelKey, entry.label) : entry.label;
}

/** Hint as rendered, i18n applied; undefined when the control has none. */
export function entryHint(entry: SettingsSearchEntry): string | undefined {
	if (entry.hint === undefined) return undefined;
	return entry.hintKey ? t(entry.hintKey, entry.hint) : entry.hint;
}

/** BM25 index over each entry's rendered text, built on first search and kept
 * until the language changes (the rendered text is what changes with it).
 * BM25 keeps the old AND semantics — every query term must hit — and adds
 * ranking, so "font" puts "Font Size" above a hint that merely mentions fonts. */
let searchIndex: { loc: string; score: (query: string) => { item: SettingsSearchEntry; score: number }[] } | null =
	null;

/** One BM25 document: rendered label + hint + section + tab label. */
function searchDoc(item: SettingsSearchEntry) {
	return {
		item,
		text: `${entryLabel(item) ?? ""} ${entryHint(item) ?? ""} ${entrySection(item)} ${globalTabLabel(item.tab)}`,
	};
}

function getSearchIndex() {
	const loc = locale();
	if (searchIndex?.loc !== loc) {
		searchIndex = {
			loc,
			...buildIndex(SETTINGS_SEARCH_INDEX.map(searchDoc)),
		};
	}
	return searchIndex;
}

/**
 * Entries matching `query`, best match first, restricted to what the user can
 * actually open.
 *
 * `availableTabs` is the live nav key set: AI Chat is absent while its
 * experimental flag is off, so its settings must not be offered — selecting one
 * would open a tab that does not exist. `client` drops the controls the other
 * client renders instead: the global hotkey exists only on the desktop.
 */
export function searchSettings(
	query: string,
	availableTabs: ReadonlySet<string>,
	client: "desktop" | "browser",
	pluginEntries: readonly SettingsSearchEntry[] = [],
): SettingsSearchEntry[] {
	// Plugin settings pages are runtime entries, so they cannot live in the
	// cached static index; with any present, rank them together with it.
	const index =
		pluginEntries.length === 0
			? getSearchIndex()
			: buildIndex([...SETTINGS_SEARCH_INDEX, ...pluginEntries].map(searchDoc));
	return index
		.score(query)
		.map((result) => result.item)
		.filter((entry) => availableTabs.has(entry.tab) && (!entry.platform || entry.platform === client));
}
