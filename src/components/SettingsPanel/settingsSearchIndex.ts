/**
 * Static search corpus for "search across all settings" (SettingsPanel's
 * search box, and the Command Palette's "Settings" entries). Hand-maintained
 * rather than scraped from the rendered DOM: scraping would mean mounting
 * every tab (including ones with real onMount side effects — network
 * fetches, etc.) just to build an index, which is far riskier than keeping
 * this list in sync by hand when a tab's controls change.
 *
 * Scope: only GLOBAL tabs, and only their STATIC controls — settings that
 * exist exactly once per tab load. Deliberately excludes:
 *  - Per-repo settings (RepoWorktreeTab/RepoScriptsTab) — there's no single
 *    "the" repo to jump to; searching those needs a repo already selected,
 *    which is a different feature than a global settings search.
 *  - Per-instance rows inside a `<For>` loop (per-agent toggles in AgentsTab,
 *    per-account rows in GitHubTab, per-provider slots in ProvidersTab,
 *    per-plugin entries in PluginsTab, per-prompt rows in SmartPromptsTab) —
 *    the same label repeats per row, so there's no single stable control to
 *    land on. `SmartPromptsTab` and `ProvidersTab` are 100% dynamic-list UI
 *    and so contribute zero entries here.
 *  - `ServicesTab`'s "HTTP API Server"/"Tailscale HTTPS"/"Self-Signed
 *    HTTPS"/"TUIC Tools" sections and a few other pockets across tabs were
 *    not captured in the initial pass — this is a useful v1 subset, not an
 *    exhaustive one. Extend it opportunistically when touching a tab file.
 *
 * `controlId` MUST match what `settingSlugId()` (SettingFields.tsx) stamps on
 * the corresponding `<SettingToggle>`/`<SettingSelect>`/`<SettingSlider>`/
 * `<SettingInput>` — both sides derive the same id from the same label text
 * so they can't drift silently; if a label changes, update it here too.
 */

import { buildIndex } from "../../utils/bm25";
import { settingSlugId } from "./SettingFields";

export interface SettingsSearchItem {
	/** Nav key from BASE_GLOBAL_TABS, e.g. "terminal" */
	tab: string;
	/** Display label from BASE_GLOBAL_TABS, e.g. "Terminal" */
	tabLabel: string;
	/** The `<h3>` section heading the control lives under */
	section: string;
	/** The control's own label text (must match the rendered label exactly) */
	label: string;
	hint?: string;
}

export interface SettingsSearchResult extends SettingsSearchItem {
	/** DOM id to scrollIntoView + highlight once the target tab is active */
	controlId: string;
}

/** Category name for the dynamic Command Palette entries built from this index. */
export const SETTINGS_SEARCH_CATEGORY = "Settings";

const RAW_ITEMS: SettingsSearchItem[] = [
	// ── General ──
	{
		tab: "general",
		tabLabel: "General",
		section: "Window",
		label: "Restore window size and position on launch",
		hint: "Reopen the app window at the same size and position as when it was last closed",
	},
	{
		tab: "general",
		tabLabel: "General",
		section: "Confirmations",
		label: "Confirm before quitting",
		hint: "Show a confirmation dialog when closing the app",
	},
	{
		tab: "general",
		tabLabel: "General",
		section: "Confirmations",
		label: "Confirm before closing a tab",
		hint: "Show a confirmation dialog when closing a terminal tab",
	},
	{
		tab: "general",
		tabLabel: "General",
		section: "Power Management",
		label: "Prevent sleep when busy",
		hint: "Keep the system awake while scripts are running",
	},
	{
		tab: "general",
		tabLabel: "General",
		section: "Power Management",
		label: "Auto-Standby Timeout",
		hint: "Pause idle background sessions after this duration to save resources. 0 = disabled.",
	},
	{
		tab: "general",
		tabLabel: "General",
		section: "Power Management",
		label: "Content Indexing",
		hint: "When to build search indexes. Set to Disabled to turn off background indexing entirely.",
	},
	{
		tab: "general",
		tabLabel: "General",
		section: "Updates",
		label: "Automatically check for updates",
		hint: "Download and install updates in the background",
	},
	{
		tab: "general",
		tabLabel: "General",
		section: "Updates",
		label: "Update Channel",
		hint: "Choose which release channel to receive updates from",
	},
	{
		tab: "general",
		tabLabel: "General",
		section: "Updates",
		label: "Default IDE",
		hint: "IDE used to open repositories",
	},
	{
		tab: "general",
		tabLabel: "General",
		section: "Experimental Features",
		label: "Enable experimental features",
		hint: "Opt in to features under active development. Individual options appear below when enabled.",
	},
	{
		tab: "general",
		tabLabel: "General",
		section: "Experimental Features",
		label: "AI Chat",
		hint: "Enable the AI Chat panel, keyboard shortcut, and command palette entry.",
	},
	{
		tab: "general",
		tabLabel: "General",
		section: "Experimental Features",
		label: "AI Triage",
		hint: "Enable AI-powered diff triage to classify changed files by relevance and risk.",
	},
	{
		tab: "general",
		tabLabel: "General",
		section: "Experimental Features",
		label: "AI Watchers",
		hint: "Enable terminal watchers that trigger AI actions on shell events (idle, busy, errors).",
	},

	// ── Appearance ──
	{
		tab: "appearance",
		tabLabel: "Appearance",
		section: "Theme",
		label: "Terminal Theme",
		hint: "Color theme for terminal output and app chrome",
	},
	{
		tab: "appearance",
		tabLabel: "Appearance",
		section: "Tabs",
		label: "Split Tab Mode",
		hint: "How worktree tabs are arranged in the tab bar",
	},
	{
		tab: "appearance",
		tabLabel: "Appearance",
		section: "Tabs",
		label: "Tab Ordering",
		hint: "How tabs are ordered: grouped by type, terminals first, or freely interleaved",
	},
	{
		tab: "appearance",
		tabLabel: "Appearance",
		section: "Tabs",
		label: "Cycle All Tab Types",
		hint: "Next/previous tab shortcuts cycle through diff, markdown and editor tabs too — not just terminals",
	},
	{
		tab: "appearance",
		tabLabel: "Appearance",
		section: "Tabs",
		label: "Nested Terminal Tabs",
		hint: "Show a branch's open terminals as a collapsible list under its sidebar row — only when the branch has more than one terminal",
	},
	{
		tab: "appearance",
		tabLabel: "Appearance",
		section: "Tabs",
		label: "Max Tab Name Length",
		hint: "Maximum characters shown in tab names before truncating",
	},
	{
		tab: "appearance",
		tabLabel: "Appearance",
		section: "Bell",
		label: "Bell Style",
		hint: "How the terminal bell (\\a / BEL) is signaled",
	},

	// ── Terminal ──
	{
		tab: "terminal",
		tabLabel: "Terminal",
		section: "Shell",
		label: "Shell",
		hint: "Shell used in terminals (leave blank for system default)",
	},
	{
		tab: "terminal",
		tabLabel: "Terminal",
		section: "Rendering",
		label: "Terminal Font",
		hint: "Monospace font for terminals",
	},
	{
		tab: "terminal",
		tabLabel: "Terminal",
		section: "Rendering",
		label: "Default Font Size",
		hint: "Default font size for new terminals",
	},
	{
		tab: "terminal",
		tabLabel: "Terminal",
		section: "Rendering",
		label: "Font Weight",
		hint: "Terminal font weight (200 = ExtraLight, 400 = Regular, 700 = Bold)",
	},
	{
		tab: "terminal",
		tabLabel: "Terminal",
		section: "Rendering",
		label: "Cursor Style",
		hint: "Shape of the terminal cursor. Applies immediately to all terminals.",
	},
	{
		tab: "terminal",
		tabLabel: "Terminal",
		section: "Behavior",
		label: "Copy on select",
		hint: "Automatically copy selected text to clipboard",
	},
	{
		tab: "terminal",
		tabLabel: "Terminal",
		section: "Behavior",
		label: "Allow OSC 52 clipboard writes",
		hint: "Let terminal programs set the system clipboard (OSC 52). A notice appears on each write. Disable to ignore clipboard writes from terminal output.",
	},
	{
		tab: "terminal",
		tabLabel: "Terminal",
		section: "Behavior",
		label: "Show agent context bar",
		hint: "Display the model's current intent, its orchestrator-assigned task, and the last prompt sent to an agent",
	},
	{ tab: "terminal", tabLabel: "Terminal", section: "Behavior", label: "Open links on" },
	{
		tab: "terminal",
		tabLabel: "Terminal",
		section: "Blocks",
		label: "Show block timestamps",
		hint: 'When each command block started, as relative time. "Hold Ctrl+Cmd" shows it only while both are held; "Always" keeps it visible.',
	},
	{
		tab: "terminal",
		tabLabel: "Terminal",
		section: "Blocks",
		label: "Show block marks",
		hint: "Tick marks on the scrollbar for each command block — red when the command failed.",
	},
	{
		tab: "terminal",
		tabLabel: "Terminal",
		section: "Blocks",
		label: "Show prompt marks",
		hint: "A green tick mark on the scrollbar for each prompt you sent.",
	},
	{
		tab: "terminal",
		tabLabel: "Terminal",
		section: "Blocks",
		label: "Enable block folding",
		hint: "Allow collapsing a command block's output with Cmd+Shift+. or a gutter click.",
	},
	{
		tab: "terminal",
		tabLabel: "Terminal",
		section: "Session Restore",
		label: "Restore open terminals on launch",
		hint: "Reopen plain shell tabs (not just agent tabs) in their saved directory when you relaunch",
	},
	{
		tab: "terminal",
		tabLabel: "Terminal",
		section: "Session Restore",
		label: "Save terminal scrollback",
		hint: "Show a restored terminal's recent output above a fresh prompt. Saved as plain text in the app's config directory — off by default.",
	},
	{
		tab: "terminal",
		tabLabel: "Terminal",
		section: "Session Restore",
		label: "Scrollback lines to save",
		hint: "Maximum lines of output saved per terminal when scrollback saving is on",
	},

	// ── Smart Selection ──
	{
		tab: "selection",
		tabLabel: "Smart Selection",
		section: "Behavior",
		label: "Double-click performs",
		hint: "Word selection expands to the character-class boundary below. Smart selection tries the rule list first, falling back to word selection when nothing matches. Quad-click (4 rapid clicks) and the right-click smart-selection menu always try the rule list, regardless of this setting.",
	},
	{
		tab: "selection",
		tabLabel: "Smart Selection",
		section: "Word Boundaries",
		label: "Word boundaries",
		hint: "Character list: a literal set of characters that BREAK a word (today's punctuation set, by default). Regular expression: alternates — the longest match at each position joins onto the adjacent word, e.g. adding https:// lets a double-click on a URL's host include the scheme.",
	},
	{
		tab: "selection",
		tabLabel: "Smart Selection",
		section: "Word Boundaries",
		label: "Word separators",
		hint: "Characters that break a word for double-click selection. Whitespace and control characters are always separators regardless of this list.",
	},
	{
		tab: "selection",
		tabLabel: "Smart Selection",
		section: "Word Boundaries",
		label: "Word pattern",
		hint: "Alternates. Plain letters/digits/underscore are always word characters; add alternates here to join punctuation-containing spans onto them.",
	},

	// ── Notifications ──
	{
		tab: "notifications",
		tabLabel: "Notifications",
		section: "Notification Settings",
		label: "Enable audio notifications",
	},
	{
		tab: "notifications",
		tabLabel: "Notifications",
		section: "Notification Settings",
		label: "Master Volume",
		hint: "Overall volume for all notification sounds — release the slider to hear a preview",
	},
	{
		tab: "notifications",
		tabLabel: "Notifications",
		section: "Notification Settings",
		label: "Audio Output Device",
		hint: "Choose which speaker or output to use for notification sounds",
	},
	{
		tab: "notifications",
		tabLabel: "Notifications",
		section: "Notification Settings",
		label: "Silence completions from MCP sessions",
		hint: "Sessions started by an agent orchestrator (session create, agent spawn) finish without a chime. They still appear in Activity and update the badge.",
	},
	{
		tab: "notifications",
		tabLabel: "Notifications",
		section: "Notification Settings",
		label: "Keep toasts in the bell",
		hint: "Toasts fade on their own, often while you look at another window. Mirroring them into the bell keeps them readable afterwards. Turn this off to leave toasts transient.",
	},

	// ── Dictation ──
	{
		tab: "dictation",
		tabLabel: "Dictation",
		section: "Dictation Settings",
		label: "Enable Dictation",
		hint: "Enable voice-to-text dictation",
	},
	{
		tab: "dictation",
		tabLabel: "Dictation",
		section: "Dictation Settings",
		label: "Whisper Model",
		hint: "Choose a model. Larger models are more accurate but slower.",
	},
	{
		tab: "dictation",
		tabLabel: "Dictation",
		section: "Dictation Settings",
		label: "Hotkey",
		hint: "Hold the hotkey to start recording, release to stop. Short presses pass through as normal input.",
	},
	{
		tab: "dictation",
		tabLabel: "Dictation",
		section: "Dictation Settings",
		label: "Long-press threshold",
		hint: "How long to hold the key before dictation starts. 0 = instant (no short-press pass-through), higher = fewer accidental triggers.",
	},
	{
		tab: "dictation",
		tabLabel: "Dictation",
		section: "Dictation Settings",
		label: "Auto-send",
		hint: "Automatically press Enter after inserting transcribed text",
	},
	{
		tab: "dictation",
		tabLabel: "Dictation",
		section: "Dictation Settings",
		label: "Language",
		hint: "Auto-detect works well for most languages.",
	},
	{
		tab: "dictation",
		tabLabel: "Dictation",
		section: "Dictation Settings",
		label: "Microphone",
		hint: "Select the input device to use for dictation.",
	},
	{
		tab: "dictation",
		tabLabel: "Dictation",
		section: "Dictation Settings",
		label: "Auto-Corrections",
		hint: "Automatically replace dictation output. Useful for technical terms.",
	},

	// ── Git & GitHub ──
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Pull Requests",
		label: "Auto-show PR popover",
		hint: "Automatically open the PR panel when a branch has an associated pull request",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Pull Requests",
		label: "Hide Draft PRs",
		hint: "Exclude draft pull requests from the Pull Requests list",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Pull Requests",
		label: "Hide Conflicting PRs",
		hint: "Exclude pull requests with merge conflicts from the Pull Requests list",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Pull Requests",
		label: "Hide CI Failing PRs",
		hint: "Exclude pull requests with failing CI checks from the Pull Requests list",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Pull Requests",
		label: "Auto-Delete on PR Close",
		hint: "Delete local branch when its PR is merged or closed on GitHub",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Issues",
		label: "Show issues",
		hint: "Display the Issues section in the GitHub panel",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Issues",
		label: "Issue Filter",
		hint: "Which issues to show in the GitHub panel",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Repository Defaults",
		label: "Default Base Branch",
		hint: "Default base branch for new worktrees",
	},
	{ tab: "github", tabLabel: "Git & GitHub", section: "Repository Defaults", label: "Copy ignored files" },
	{ tab: "github", tabLabel: "Git & GitHub", section: "Repository Defaults", label: "Copy untracked files" },
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Repository Defaults",
		label: "Default Setup Script",
		hint: "Shell script run when creating a new worktree",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Repository Defaults",
		label: "Default Run Script",
		hint: "Shell script run when launching the worktree",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Repository Defaults",
		label: "Default Archive Script",
		hint: "Shell script run before archiving or deleting a worktree",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Worktree Defaults",
		label: "Storage Strategy",
		hint: "Where to create worktree directories",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Worktree Defaults",
		label: "Prompt for branch name during creation",
		hint: "Show dialog when creating worktrees from '+' button. When off, creates instantly with auto-generated name",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Worktree Defaults",
		label: "Delete local branch when removing worktree",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Worktree Defaults",
		label: "Auto-archive merged worktrees",
		hint: "Move worktree to archive directory when its PR is merged",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Worktree Defaults",
		label: "Orphan Worktree Cleanup",
		hint: "A worktree whose branch was deleted out from under it. 'Auto-archive' and 'Ask' move it aside (recoverable) since detection is a heuristic that can misfire; 'Auto-remove' deletes it outright with no recovery.",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Worktree Defaults",
		label: "PR Merge Strategy",
		hint: "Default merge strategy for worktree branches",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Worktree Defaults",
		label: "After Merge Behavior",
		hint: "What to do with the worktree after merging its branch",
	},
	{
		tab: "github",
		tabLabel: "Git & GitHub",
		section: "Worktree Defaults",
		label: "Auto-Fetch Interval",
		hint: "Periodically fetch from remote to detect upstream changes",
	},

	// ── Services & MCP ──
	{
		tab: "services",
		tabLabel: "Services & MCP",
		section: "Remote Access",
		label: "Enable remote access",
		hint: "Warning: exposes a web interface on your local network. Secure with a strong password.",
	},
	{
		tab: "services",
		tabLabel: "Services & MCP",
		section: "Remote Access",
		label: "Session Token Duration",
		hint: "How long remote sessions stay authenticated. Token always resets on app restart.",
	},
	{
		tab: "services",
		tabLabel: "Services & MCP",
		section: "Remote Access",
		label: "Enable IPv6 (dual-stack)",
		hint: "Binds the server to both IPv4 and IPv6 addresses. Requires save + server restart.",
	},
	{
		tab: "services",
		tabLabel: "Services & MCP",
		section: "Remote Access",
		label: "Allow LAN access without authentication",
		hint: "Skips authentication for private/LAN IP addresses (RFC1918, Tailscale, IPv6 ULA)",
	},
	{
		tab: "services",
		tabLabel: "Services & MCP",
		section: "Cloud Relay",
		label: "Enable cloud relay",
		hint: "Connect from anywhere via an encrypted WebSocket relay. No port forwarding or VPN needed. Note: traffic is encrypted in transit, but the relay operator can derive the key — this is not end-to-end encryption.",
	},
	{ tab: "services", tabLabel: "Services & MCP", section: "Cloud Relay", label: "Relay Server URL" },
	{
		tab: "services",
		tabLabel: "Services & MCP",
		section: "Cloud Relay",
		label: "Bearer Token",
		hint: "Obtained from the relay server's /register endpoint. Used for both authentication and encryption key derivation — because the relay receives this token, it can derive the key, so traffic is not end-to-end encrypted.",
	},

	// ── Agents ── (mostly per-agent loops; these two are the only static ones)
	{
		tab: "agents",
		tabLabel: "Agents",
		section: "Agents",
		label: "Show agent intent as tab title",
		hint: "When agents declare their current work phase, update the tab name with a short title",
	},
	{
		tab: "agents",
		tabLabel: "Agents",
		section: "Agents",
		label: "Show suggested follow-up actions",
		hint: "Display actionable suggestions from agents after completing a task",
	},

	// ── AI Chat ──
	{
		tab: "ai-chat",
		tabLabel: "AI Chat",
		section: "Parameters",
		label: "Temperature",
		hint: "Controls randomness of responses (0.0 = deterministic, 1.0 = creative)",
	},
	{
		tab: "ai-chat",
		tabLabel: "AI Chat",
		section: "Parameters",
		label: "Extended thinking",
		hint: 'Streams the model\'s reasoning into a collapsible "Thinking" block. Only models that support extended thinking (Claude Opus 4.7+) are affected; higher effort costs more tokens and latency.',
	},
];

export const SETTINGS_SEARCH_RESULTS: SettingsSearchResult[] = RAW_ITEMS.map((item) => ({
	...item,
	controlId: settingSlugId(item.label),
}));

const searchIndex = buildIndex(
	SETTINGS_SEARCH_RESULTS.map((item) => ({
		item,
		text: `${item.label} ${item.hint ?? ""} ${item.section} ${item.tabLabel}`,
	})),
);

/** Ranked settings matching `query`, best match first. Empty query → empty results. */
export function searchSettings(query: string): SettingsSearchResult[] {
	if (!query.trim()) return [];
	return searchIndex.score(query).map((r) => r.item);
}
