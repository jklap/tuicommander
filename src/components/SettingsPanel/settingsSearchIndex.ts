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
 * every `<h3>` opens a section; every `label=` prop and every `<label>` element
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
 * Repo-scoped tabs (`repo:<path>`) are not indexed: their nav key depends on
 * which repository the user means, and a global search box cannot know.
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
	/** Default text of the control's static `hint=` prop, when it has one */
	hint?: string;
	hintKey?: string;
}

/** What a settings deep link scrolls to: rendered heading text, plus the
 * rendered label when it names one control rather than a whole section. */
export interface SettingsSearchTarget {
	section: string;
	label?: string;
}

/** Command Palette category of the per-setting deep-link actions. */
export const SETTINGS_SEARCH_CATEGORY = "Settings";

export const SETTINGS_SEARCH_INDEX: SettingsSearchEntry[] = [
	// tabs/GeneralTab.tsx
	{ tab: "general", section: "General", sectionKey: "general.heading.general" },
	{ tab: "general", section: "TUIC CLI", sectionKey: "general.heading.cli" },
	{ tab: "general", section: "Finder Integration", sectionKey: "general.heading.finderService" },
	{ tab: "general", section: "Code Intelligence", sectionKey: "general.heading.codeIntelligence" },
	{ tab: "general", section: "Window", sectionKey: "general.heading.window" },
	{ tab: "general", section: "Confirmations", sectionKey: "general.heading.confirmations" },
	{ tab: "general", section: "Power Management", sectionKey: "general.heading.powerManagement" },
	{ tab: "general", section: "Updates", sectionKey: "general.heading.updates" },
	{ tab: "general", section: "Custom Launchers", sectionKey: "general.heading.customLaunchers" },
	{ tab: "general", section: "Experimental Features", sectionKey: "general.heading.experimental" },
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
		section: "Window",
		label: "Restore window size and position on launch",
		labelKey: "general.toggle.restoreWindowGeometry",
		hint: "Reopen the app window at the same size and position as when it was last closed",
		hintKey: "general.hint.restoreWindowGeometry",
	},
	{
		tab: "general",
		section: "Confirmations",
		label: "Confirm before quitting",
		labelKey: "general.toggle.confirmBeforeQuit",
		hint: "Show a confirmation dialog when closing the app",
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
		hint: "Pause idle background sessions after this duration to save resources. 0 = disabled.",
	},
	{
		tab: "general",
		section: "Power Management",
		label: "Content Indexing",
		hint: "When to build search indexes. Set to Disabled to turn off background indexing entirely.",
	},
	{
		tab: "general",
		section: "Updates",
		label: "Automatically check for updates",
		labelKey: "general.toggle.autoUpdateEnabled",
		hint: "Download and install updates in the background",
		hintKey: "general.hint.autoUpdateEnabled",
	},
	{ tab: "general", section: "Updates", label: "Update Channel", labelKey: "general.label.updateChannel" },
	{
		tab: "general",
		section: "Updates",
		label: "Default IDE",
		labelKey: "general.label.defaultIde",
		hint: "IDE used to open repositories",
		hintKey: "general.hint.defaultIde",
	},
	{
		tab: "general",
		section: "Experimental Features",
		label: "AI Chat",
		labelKey: "general.toggle.aiChat",
		hint: "Enable the AI Chat panel, keyboard shortcut, and command palette entry.",
		hintKey: "general.hint.aiChat",
	},
	{
		tab: "general",
		section: "Experimental Features",
		label: "AI Triage",
		labelKey: "general.toggle.aiTriage",
		hint: "Enable AI-powered diff triage to classify changed files by relevance and risk.",
		hintKey: "general.hint.aiTriage",
	},
	{
		tab: "general",
		section: "Experimental Features",
		label: "AI Watchers",
		labelKey: "general.toggle.aiWatchers",
		hint: "Enable terminal watchers that trigger AI actions on shell events (idle, busy, errors).",
		hintKey: "general.hint.aiWatchers",
	},
	// tabs/AppearanceTab.tsx
	{ tab: "appearance", section: "Theme", sectionKey: "appearance.heading.theme" },
	{ tab: "appearance", section: "Tabs", sectionKey: "appearance.heading.tabs" },
	{ tab: "appearance", section: "Repository Groups", sectionKey: "appearance.heading.groups" },
	{ tab: "appearance", section: "Layout", sectionKey: "appearance.heading.layout" },
	{ tab: "appearance", section: "Bell", sectionKey: "appearance.heading.bell" },
	{ tab: "appearance", section: "UI Legend", sectionKey: "appearance.heading.uiLegend" },
	{
		tab: "appearance",
		section: "Theme",
		label: "Terminal Theme",
		labelKey: "appearance.label.terminalTheme",
		hint: "Color theme for terminal output and app chrome",
		hintKey: "appearance.hint.terminalTheme",
	},
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
		hint: "Show a branch's open terminals as a collapsible list under its sidebar row — only when the branch has more than one terminal",
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
		section: "Bell",
		label: "Bell Style",
		labelKey: "appearance.label.bellStyle",
		hint: "How the terminal bell (\\a / BEL) is signaled",
		hintKey: "appearance.hint.bellStyle",
	},
	// tabs/TerminalTab.tsx
	{ tab: "terminal", section: "Shell", sectionKey: "terminal.heading.shell" },
	{ tab: "terminal", section: "Rendering", sectionKey: "terminal.heading.rendering" },
	{ tab: "terminal", section: "Behavior", sectionKey: "terminal.heading.behavior" },
	{ tab: "terminal", section: "Blocks", sectionKey: "terminal.heading.blocks" },
	{ tab: "terminal", section: "Shell Integration", sectionKey: "terminal.heading.shellIntegration" },
	{
		tab: "terminal",
		section: "Custom Environment Variables",
		sectionKey: "terminal.heading.customEnv",
	},
	{ tab: "terminal", section: "Session Restore", sectionKey: "terminal.heading.sessionRestore" },
	{
		tab: "terminal",
		section: "Shell",
		label: "Shell",
		labelKey: "terminal.label.shell",
		hint: "Shell used in terminals (leave blank for system default)",
		hintKey: "terminal.hint.shell",
	},
	{
		tab: "terminal",
		section: "Rendering",
		label: "Terminal Font",
		labelKey: "appearance.label.terminalFont",
		hint: "Monospace font for terminals",
		hintKey: "appearance.hint.terminalFont",
	},
	{
		tab: "terminal",
		section: "Rendering",
		label: "Default Font Size",
		labelKey: "appearance.label.defaultFontSize",
		hint: "Default font size for new terminals",
		hintKey: "appearance.hint.defaultFontSize",
	},
	{
		tab: "terminal",
		section: "Rendering",
		label: "Font Weight",
		labelKey: "appearance.label.fontWeight",
		hint: "Terminal font weight (200 = ExtraLight, 400 = Regular, 700 = Bold)",
		hintKey: "appearance.hint.fontWeight",
	},
	{
		tab: "terminal",
		section: "Rendering",
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
		hint: "Let terminal programs set the system clipboard (OSC 52). A notice appears on each write. Disable to ignore clipboard writes from terminal output.",
		hintKey: "general.hint.osc52Clipboard",
	},
	{
		tab: "terminal",
		section: "Behavior",
		label: "Reflow scrollback on resize",
		labelKey: "general.toggle.scrollbackReflow",
		hint: "Re-wrap scrollback history when the terminal changes width, so old output stays readable after a side panel opens. Turn it off to leave history lines as they were written and truncate them instead. The visible screen is never reflowed either way.",
		hintKey: "general.hint.scrollbackReflow",
	},
	{
		tab: "terminal",
		section: "Behavior",
		label: "Allow terminal focus/attention requests",
		labelKey: "general.toggle.osc1337FocusAttention",
		hint: "Let terminal programs bring the window to the front or bounce the dock icon (OSC 1337 StealFocus/RequestAttention). Disable if a script or log spams either.",
		hintKey: "general.hint.osc1337FocusAttention",
	},
	{
		tab: "terminal",
		section: "Behavior",
		label: "Show agent context bar",
		hint: "Display the model's current intent, its orchestrator-assigned task, and the last prompt sent to an agent",
	},
	{ tab: "terminal", section: "Behavior", label: "Open links on", labelKey: "terminal.label.linkActivation" },
	{ tab: "terminal", section: "Blocks", label: "Show block timestamps", labelKey: "terminal.label.blockTimestampMode" },
	{
		tab: "terminal",
		section: "Blocks",
		label: "Show block marks",
		labelKey: "terminal.toggle.showBlockMarks",
		hint: "Tick marks on the scrollbar for each command block — red when the command failed.",
		hintKey: "terminal.hint.showBlockMarks",
	},
	{
		tab: "terminal",
		section: "Blocks",
		label: "Show prompt marks",
		labelKey: "terminal.toggle.showPromptMarks",
		hint: "A green tick mark on the scrollbar for each prompt you sent.",
		hintKey: "terminal.hint.showPromptMarks",
	},
	{
		tab: "terminal",
		section: "Blocks",
		label: "Enable block folding",
		labelKey: "terminal.toggle.blockFoldingEnabled",
		hint: "Allow collapsing a command block's output with Cmd+Shift+. or a gutter click.",
		hintKey: "terminal.hint.blockFoldingEnabled",
	},
	{
		tab: "terminal",
		section: "Custom Environment Variables",
		label: "Environment Variables",
		labelKey: "terminal.label.customEnv",
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
	// tabs/NotificationsTab.tsx
	{ tab: "notifications", section: "Notification Settings", sectionKey: "notifications.heading.notificationSettings" },
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
		hint: "Overall volume for all notification sounds — release the slider to hear a preview",
		hintKey: "notifications.hint.masterVolume",
	},
	{
		tab: "notifications",
		section: "Notification Settings",
		label: "Audio Output Device",
		labelKey: "notifications.label.audioDevice",
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
		label: "Toolbar Bell",
		labelKey: "notifications.label.toolbarBell",
	},
	// DictationSettings.tsx
	{ tab: "dictation", section: "Dictation Settings", sectionKey: "dictation.title" },
	{ tab: "dictation", section: "Dictation Settings", label: "Enable Dictation", labelKey: "dictation.enableLabel" },
	{ tab: "dictation", section: "Dictation Settings", label: "Whisper Model", labelKey: "dictation.modelLabel" },
	{ tab: "dictation", section: "Dictation Settings", label: "Hotkey", labelKey: "dictation.hotkeyLabel" },
	{
		tab: "dictation",
		section: "Dictation Settings",
		label: "Long-press threshold",
		labelKey: "dictation.longPressLabel",
		hint: "How long to hold the key before dictation starts. 0 = instant (no short-press pass-through), higher = fewer accidental triggers.",
		hintKey: "dictation.longPressHint",
	},
	{ tab: "dictation", section: "Dictation Settings", label: "Auto-send", labelKey: "dictation.autoSendLabel" },
	{ tab: "dictation", section: "Dictation Settings", label: "Language", labelKey: "dictation.languageLabel" },
	{ tab: "dictation", section: "Dictation Settings", label: "Microphone", labelKey: "dictation.microphoneLabel" },
	{
		tab: "dictation",
		section: "Dictation Settings",
		label: "Auto-Corrections",
		labelKey: "dictation.correctionsLabel",
	},
	{ tab: "dictation", section: "Dictation Settings", label: "Voice tuning", labelKey: "dictation.tuningLabel" },
	{
		tab: "dictation",
		section: "Dictation Settings",
		label: "Level gate",
		labelKey: "dictation.rmsLabel",
		hint: "Audio quieter than this never reaches Whisper. Raise it until room noise stays below the marker; lower it if quiet speech is rejected.",
		hintKey: "dictation.rmsHint",
	},
	{
		tab: "dictation",
		section: "Dictation Settings",
		label: "Speech confidence gate",
		labelKey: "dictation.noSpeechLabel",
		hint: "Discards a transcript when Whisper itself reports it probably heard no speech. Lower is stricter; 100% turns the gate off.",
		hintKey: "dictation.noSpeechHint",
	},
	// tabs/GitHubTab.tsx
	{ tab: "github", section: "GitHub Authentication" },
	{ tab: "github", section: "Pull Requests" },
	{ tab: "github", section: "Issues" },
	{ tab: "github", section: "Repository Defaults" },
	{ tab: "github", section: "Worktree Defaults" },
	{ tab: "github", section: "Additional GitHub Accounts" },
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
	{ tab: "github", section: "Repository Defaults", label: "File Handling Defaults" },
	{ tab: "github", section: "Repository Defaults", label: "Default Setup Script" },
	{ tab: "github", section: "Repository Defaults", label: "Default Run Script" },
	{ tab: "github", section: "Repository Defaults", label: "Default Archive Script" },
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "Storage Strategy",
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
		hint: "Move worktree to archive directory when its PR is merged",
	},
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "Orphan Worktree Cleanup",
		hint: "A worktree whose branch was deleted out from under it. 'Auto-archive' and 'Ask' move it aside (recoverable) since detection is a heuristic that can misfire; 'Auto-remove' deletes it outright with no recovery.",
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
		hint: "What to do with the worktree after merging its branch",
	},
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "Auto-Fetch Interval",
		hint: "Periodically fetch from remote to detect upstream changes",
	},
	{ tab: "github", section: "Additional GitHub Accounts", label: "Add another github.com account" },
	{ tab: "github", section: "Additional GitHub Accounts", label: "Add Enterprise account" },
	// tabs/ServicesTab.tsx
	{ tab: "services", section: "TUIC MCP Server" },
	// tabs/RemoteAccessTab.tsx
	{ tab: "remote-access", section: "HTTP API Server", sectionKey: "services.heading.httpApiServer" },
	{ tab: "remote-access", section: "File Access", sectionKey: "services.heading.fileAccess" },
	{ tab: "remote-access", section: "Remote Access", sectionKey: "services.heading.remoteAccess" },
	{ tab: "remote-access", section: "Tailscale HTTPS" },
	{ tab: "remote-access", section: "Self-Signed HTTPS", sectionKey: "services.heading.selfSignedHttps" },
	{ tab: "remote-access", section: "Cloud Relay", sectionKey: "services.heading.cloudRelay" },
	{
		tab: "remote-access",
		section: "HTTP API Server",
		label: "Server Status",
		labelKey: "services.label.serverStatus",
	},
	{
		tab: "remote-access",
		section: "File Access",
		label: "Additional Readable Directories",
		labelKey: "services.label.additionalReadableDirs",
	},
	{
		tab: "remote-access",
		section: "Remote Access",
		label: "Enable remote access",
		labelKey: "services.toggle.enableRemoteAccess",
		hint: "Warning: exposes a web interface on your local network. Secure with a strong password.",
		hintKey: "services.hint.remoteAccessWarning",
	},
	{ tab: "remote-access", section: "Remote Access", label: "Port", labelKey: "services.label.port" },
	{ tab: "remote-access", section: "Remote Access", label: "Username", labelKey: "services.label.username" },
	{ tab: "remote-access", section: "Remote Access", label: "Password", labelKey: "services.label.password" },
	{
		tab: "remote-access",
		section: "Remote Access",
		label: "Network Interface",
		labelKey: "services.label.networkInterface",
	},
	{
		tab: "remote-access",
		section: "Remote Access",
		label: "Session Token Duration",
		labelKey: "services.label.tokenDuration",
		hint: "How long remote sessions stay authenticated. Token always resets on app restart.",
		hintKey: "services.hint.tokenDuration",
	},
	{
		tab: "remote-access",
		section: "Remote Access",
		label: "Enable IPv6 (dual-stack)",
		labelKey: "services.toggle.enableIpv6",
		hint: "Binds the server to both IPv4 and IPv6 addresses. Requires save + server restart.",
		hintKey: "services.hint.ipv6Description",
	},
	{
		tab: "remote-access",
		section: "Remote Access",
		label: "Allow LAN access without authentication",
		labelKey: "services.toggle.lanAuthBypass",
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
	// tabs/RemoteServersTab.tsx
	{ tab: "remote-servers", section: "Remote Servers", sectionKey: "remoteServers.heading" },
	// tabs/PluginsTab.tsx
	{ tab: "plugins", section: "Plugins" },
	{
		tab: "plugins",
		section: "Plugins",
		label: "Check for plugin updates",
		hint: "Fetch the registry at startup and show available updates",
	},
	// tabs/SmartPromptsTab.tsx
	{ tab: "smart-prompts", section: "Smart Prompts" },
	{ tab: "smart-prompts", section: "Smart Prompts", label: "Headless Agent" },
	// tabs/SelectionTab.tsx
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
	// tabs/ProvidersTab.tsx
	{ tab: "providers", section: "Add Provider" },
	{ tab: "providers", section: "Slot Assignments" },
	{ tab: "providers", section: "Providers" },
	{ tab: "providers", section: "Add Provider", label: "Type" },
	{ tab: "providers", section: "Add Provider", label: "Label" },
	{ tab: "providers", section: "Add Provider", label: "Base URL (optional)" },
	{ tab: "providers", section: "Add Provider", label: "API Key" },
	{ tab: "providers", section: "Add Provider", label: "Model name" },
	{ tab: "providers", section: "Add Provider", label: "Tier" },
	// tabs/AgentsTab.tsx
	{ tab: "agents", section: "Agents" },
	{
		tab: "agents",
		section: "Agents",
		label: "Show agent intent as tab title",
		hint: "When agents declare their current work phase, update the tab name with a short title",
	},
	{
		tab: "agents",
		section: "Agents",
		label: "Show suggested follow-up actions",
		hint: "Display actionable suggestions from agents after completing a task",
	},
	// tabs/AiChatTab.tsx
	{ tab: "ai-chat", section: "Parameters" },
	{ tab: "ai-chat", section: "Scheduled Tasks" },
	{ tab: "ai-chat", section: "Parameters", label: "Temperature" },
	{ tab: "ai-chat", section: "Parameters", label: "Extended thinking" },
	// tabs/StreamDockTab.tsx
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

function getSearchIndex() {
	const loc = locale();
	if (searchIndex?.loc !== loc) {
		searchIndex = {
			loc,
			...buildIndex(
				SETTINGS_SEARCH_INDEX.map((item) => ({
					item,
					text: `${entryLabel(item) ?? ""} ${entryHint(item) ?? ""} ${entrySection(item)} ${globalTabLabel(item.tab)}`,
				})),
			),
		};
	}
	return searchIndex;
}

/**
 * Entries matching `query`, best match first, restricted to tabs the user can
 * actually open.
 *
 * `availableTabs` is the live nav key set: the Dictation tab is absent in
 * browser mode and AI Chat is absent unless the flag is on, so their settings
 * must not be offered — selecting one would open a tab that does not exist.
 */
export function searchSettings(query: string, availableTabs: ReadonlySet<string>): SettingsSearchEntry[] {
	return getSearchIndex()
		.score(query)
		.map((result) => result.item)
		.filter((entry) => availableTabs.has(entry.tab));
}
