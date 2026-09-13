import { t } from "../../i18n";

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
 * `t("key", "Default")`, a string literal, or a leading plain-text run.
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
}

export const SETTINGS_SEARCH_INDEX: SettingsSearchEntry[] = [
	// tabs/GeneralTab.tsx
	{ tab: "general", section: "General", sectionKey: "general.heading.general" },
	{ tab: "general", section: "TUIC CLI", sectionKey: "general.heading.cli" },
	{ tab: "general", section: "Code Intelligence", sectionKey: "general.heading.codeIntelligence" },
	{ tab: "general", section: "Confirmations", sectionKey: "general.heading.confirmations" },
	{ tab: "general", section: "Terminal", sectionKey: "general.heading.terminal" },
	{ tab: "general", section: "Power Management", sectionKey: "general.heading.powerManagement" },
	{ tab: "general", section: "Updates", sectionKey: "general.heading.updates" },
	{ tab: "general", section: "Custom Launchers", sectionKey: "general.heading.customLaunchers" },
	{ tab: "general", section: "Experimental Features", sectionKey: "general.heading.experimental" },
	{ tab: "general", section: "General", label: "Language", labelKey: "general.label.language" },
	{ tab: "general", section: "General", label: "Shell", labelKey: "general.label.shell" },
	{
		tab: "general",
		section: "Confirmations",
		label: "Confirm before quitting",
		labelKey: "general.toggle.confirmBeforeQuit",
	},
	{
		tab: "general",
		section: "Confirmations",
		label: "Confirm before closing a tab",
		labelKey: "general.toggle.confirmBeforeClosingTab",
	},
	{ tab: "general", section: "Terminal", label: "Copy on select", labelKey: "general.toggle.copyOnSelect" },
	{
		tab: "general",
		section: "Terminal",
		label: "Allow OSC 52 clipboard writes",
		labelKey: "general.toggle.osc52Clipboard",
	},
	{ tab: "general", section: "Terminal", label: "Show agent context bar" },
	{
		tab: "general",
		section: "Terminal",
		label: "Show block timestamps",
		labelKey: "general.toggle.showBlockTimestamps",
	},
	{ tab: "general", section: "Terminal", label: "Block folding", labelKey: "general.toggle.blockFolding" },
	{
		tab: "general",
		section: "Terminal",
		label: "Show scrollbar marks",
		labelKey: "general.toggle.showScrollbarMarks",
	},
	{
		tab: "general",
		section: "Terminal",
		label: "Reflow scrollback on resize",
		labelKey: "general.toggle.scrollbackReflow",
	},
	{
		tab: "general",
		section: "Power Management",
		label: "Prevent sleep when busy",
		labelKey: "general.toggle.preventSleepWhenBusy",
	},
	{ tab: "general", section: "Power Management", label: "Auto-Standby Timeout" },
	{ tab: "general", section: "Power Management", label: "Content Indexing" },
	{
		tab: "general",
		section: "Updates",
		label: "Automatically check for updates",
		labelKey: "general.toggle.autoUpdateEnabled",
	},
	{ tab: "general", section: "Updates", label: "Update Channel", labelKey: "general.label.updateChannel" },
	{ tab: "general", section: "Updates", label: "Default IDE", labelKey: "general.label.defaultIde" },
	{ tab: "general", section: "Experimental Features", label: "AI Chat", labelKey: "general.toggle.aiChat" },
	{ tab: "general", section: "Experimental Features", label: "AI Triage", labelKey: "general.toggle.aiTriage" },
	{ tab: "general", section: "Experimental Features", label: "AI Watchers", labelKey: "general.toggle.aiWatchers" },
	{ tab: "general", section: "Experimental Features", label: "Copy-on-write workspaces", labelKey: "general.toggle.cowWorkspaces" },
	// tabs/AppearanceTab.tsx
	{ tab: "appearance", section: "Theme", sectionKey: "appearance.heading.theme" },
	{ tab: "appearance", section: "Terminal", sectionKey: "appearance.heading.terminal" },
	{ tab: "appearance", section: "Tabs", sectionKey: "appearance.heading.tabs" },
	{ tab: "appearance", section: "Repository Groups", sectionKey: "appearance.heading.groups" },
	{ tab: "appearance", section: "Layout", sectionKey: "appearance.heading.layout" },
	{ tab: "appearance", section: "UI Legend", sectionKey: "appearance.heading.uiLegend" },
	{ tab: "appearance", section: "Theme", label: "Terminal Theme", labelKey: "appearance.label.terminalTheme" },
	{ tab: "appearance", section: "Terminal", label: "Terminal Font", labelKey: "appearance.label.terminalFont" },
	{ tab: "appearance", section: "Terminal", label: "Default Font Size", labelKey: "appearance.label.defaultFontSize" },
	{ tab: "appearance", section: "Terminal", label: "Font Weight", labelKey: "appearance.label.fontWeight" },
	{ tab: "appearance", section: "Terminal", label: "Cursor Style", labelKey: "appearance.label.cursorStyle" },
	{ tab: "appearance", section: "Tabs", label: "Split Tab Mode", labelKey: "appearance.label.splitTabMode" },
	{ tab: "appearance", section: "Tabs", label: "Tab Ordering", labelKey: "appearance.label.tabOrderingMode" },
	{ tab: "appearance", section: "Tabs", label: "Cycle All Tab Types", labelKey: "appearance.label.tabCyclingAllTypes" },
	{ tab: "appearance", section: "Tabs", label: "Nested Terminal Tabs", labelKey: "appearance.label.tabTreeEnabled" },
	{ tab: "appearance", section: "Tabs", label: "Max Tab Name Length", labelKey: "appearance.label.maxTabNameLength" },
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
	// `VoiceTuning` renders between Microphone and Auto-Corrections, but it is a
	// sub-component, so `extractSettings` — which reads source order, not the
	// render tree — sees it wherever it is DEFINED. It is defined at the end of
	// the file for exactly that reason, and these entries follow it.
	{ tab: "dictation", section: "Dictation Settings", label: "Voice tuning", labelKey: "dictation.tuningLabel" },
	{ tab: "dictation", section: "Dictation Settings", label: "Level gate", labelKey: "dictation.rmsLabel" },
	{
		tab: "dictation",
		section: "Dictation Settings",
		label: "Speech confidence gate",
		labelKey: "dictation.noSpeechLabel",
	},
	// tabs/GitHubTab.tsx
	{ tab: "github", section: "GitHub Authentication" },
	{ tab: "github", section: "Pull Requests" },
	{ tab: "github", section: "Issues" },
	{ tab: "github", section: "Repository Defaults" },
	{ tab: "github", section: "Worktree Defaults" },
	{ tab: "github", section: "Additional GitHub Accounts" },
	{ tab: "github", section: "Repository Bindings" },
	{ tab: "github", section: "Pull Requests", label: "Auto-show PR popover" },
	{ tab: "github", section: "Pull Requests", label: "Hide Draft PRs" },
	{ tab: "github", section: "Pull Requests", label: "Hide Conflicting PRs" },
	{ tab: "github", section: "Pull Requests", label: "Hide CI Failing PRs" },
	{ tab: "github", section: "Pull Requests", label: "Auto-Delete on PR Close" },
	{ tab: "github", section: "Issues", label: "Show issues" },
	{ tab: "github", section: "Issues", label: "Issue Filter" },
	{ tab: "github", section: "Repository Defaults", label: "Default Base Branch" },
	{ tab: "github", section: "Repository Defaults", label: "File Handling Defaults" },
	{ tab: "github", section: "Repository Defaults", label: "Default Setup Script" },
	{ tab: "github", section: "Repository Defaults", label: "Default Run Script" },
	{ tab: "github", section: "Repository Defaults", label: "Default Archive Script" },
	{ tab: "github", section: "Worktree Defaults", label: "Storage Strategy" },
	{ tab: "github", section: "Worktree Defaults", label: "Prompt for branch name during creation" },
	{ tab: "github", section: "Worktree Defaults", label: "Delete local branch when removing worktree" },
	{ tab: "github", section: "Worktree Defaults", label: "Auto-archive merged worktrees" },
	{ tab: "github", section: "Worktree Defaults", label: "Orphan Worktree Cleanup" },
	{ tab: "github", section: "Worktree Defaults", label: "PR Merge Strategy" },
	{ tab: "github", section: "Worktree Defaults", label: "After Merge Behavior" },
	{ tab: "github", section: "Worktree Defaults", label: "Auto-Fetch Interval" },
	{ tab: "github", section: "Additional GitHub Accounts", label: "Add another github.com account" },
	{ tab: "github", section: "Additional GitHub Accounts", label: "Add Enterprise account" },
	// tabs/ServicesTab.tsx
	{ tab: "services", section: "HTTP API Server", sectionKey: "services.heading.httpApiServer" },
	{ tab: "services", section: "Remote Access", sectionKey: "services.heading.remoteAccess" },
	{ tab: "services", section: "Tailscale HTTPS" },
	{ tab: "services", section: "Cloud Relay", sectionKey: "services.heading.cloudRelay" },
	{ tab: "services", section: "TUIC Tools" },
	{ tab: "services", section: "HTTP API Server", label: "Server Status", labelKey: "services.label.serverStatus" },
	{ tab: "services", section: "HTTP API Server", label: "MCP Connection", labelKey: "services.label.mcpConnection" },
	{
		tab: "services",
		section: "Remote Access",
		label: "Enable remote access",
		labelKey: "services.toggle.enableRemoteAccess",
	},
	{ tab: "services", section: "Remote Access", label: "Port", labelKey: "services.label.port" },
	{ tab: "services", section: "Remote Access", label: "Username", labelKey: "services.label.username" },
	{ tab: "services", section: "Remote Access", label: "Password", labelKey: "services.label.password" },
	{
		tab: "services",
		section: "Remote Access",
		label: "Network Interface",
		labelKey: "services.label.networkInterface",
	},
	{
		tab: "services",
		section: "Remote Access",
		label: "Session Token Duration",
		labelKey: "services.label.tokenDuration",
	},
	{
		tab: "services",
		section: "Remote Access",
		label: "Enable IPv6 (dual-stack)",
		labelKey: "services.toggle.enableIpv6",
	},
	{
		tab: "services",
		section: "Remote Access",
		label: "Allow LAN access without authentication",
		labelKey: "services.toggle.lanAuthBypass",
	},
	{ tab: "services", section: "Cloud Relay", label: "Enable cloud relay", labelKey: "services.toggle.enableRelay" },
	{ tab: "services", section: "Cloud Relay", label: "Relay Server URL", labelKey: "services.label.relayUrl" },
	{ tab: "services", section: "Cloud Relay", label: "Bearer Token", labelKey: "services.label.relayToken" },
	{ tab: "services", section: "Cloud Relay", label: "Session ID", labelKey: "services.label.relaySessionId" },
	// tabs/PluginsTab.tsx
	{ tab: "plugins", section: "Plugins" },
	{ tab: "plugins", section: "Plugins", label: "Check for plugin updates" },
	// tabs/SmartPromptsTab.tsx
	{ tab: "smart-prompts", section: "Smart Prompts" },
	{ tab: "smart-prompts", section: "Smart Prompts", label: "Headless Agent" },
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
	{ tab: "agents", section: "Agents", label: "Show agent intent as tab title" },
	{ tab: "agents", section: "Agents", label: "Show suggested follow-up actions" },
	// tabs/AiChatTab.tsx
	{ tab: "ai-chat", section: "Parameters" },
	{ tab: "ai-chat", section: "Scheduled Tasks" },
	{ tab: "ai-chat", section: "Parameters", label: "Temperature" },
	{ tab: "ai-chat", section: "Parameters", label: "Extended thinking" },
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

/** Every word of `query` must appear somewhere in the entry's rendered text. */
function matches(entry: SettingsSearchEntry, terms: string[]): boolean {
	const haystack = `${entrySection(entry)} ${entryLabel(entry) ?? ""}`.toLowerCase();
	return terms.every((term) => haystack.includes(term));
}

/**
 * Entries matching `query`, restricted to tabs the user can actually open.
 *
 * `availableTabs` is the live nav key set: the Dictation tab is absent in
 * browser mode and AI Chat is absent unless the flag is on, so their settings
 * must not be offered — selecting one would open a tab that does not exist.
 */
export function searchSettings(query: string, availableTabs: ReadonlySet<string>): SettingsSearchEntry[] {
	const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
	if (terms.length === 0) return [];
	return SETTINGS_SEARCH_INDEX.filter((entry) => availableTabs.has(entry.tab) && matches(entry, terms));
}
