import { t } from "../../i18n";
import { settingsStore } from "../../stores/settings";
import { isTauri } from "../../transport";
import type { SettingsShellTab } from "./SettingsShell";

/** Global pages grouped by task; each group renders as a static label row above its pages.
 *
 * Lives outside `SettingsPanel.tsx` so the Command Palette's per-setting
 * actions (`useCommandPaletteActions`) and the search index can name a tab
 * without importing the whole lazily-loaded panel. */
export const GLOBAL_TAB_GROUPS: { key: string; label: string; tabs: SettingsShellTab[] }[] = [
	{
		key: "application",
		label: t("settings.group.application", "Application"),
		tabs: [
			{ key: "general", label: t("settings.general", "General") },
			{ key: "appearance", label: t("settings.appearance", "Appearance") },
			{ key: "notifications", label: t("settings.notifications", "Notifications") },
		],
	},
	{
		key: "workspace",
		label: t("settings.group.workspace", "Workspace"),
		tabs: [
			{ key: "terminal", label: t("settings.terminal", "Terminal") },
			{ key: "selection", label: t("settings.selection", "Smart Selection") },
			{ key: "keyboard-shortcuts", label: t("settings.keyboardShortcuts", "Keyboard Shortcuts") },
			{ key: "github", label: "Git & GitHub" },
		],
	},
	{
		key: "ai",
		label: t("settings.group.ai", "AI"),
		tabs: [
			{ key: "agents", label: t("settings.agents", "Agents") },
			{ key: "ai-chat", label: t("settings.aiChat", "AI Chat") },
			{ key: "dictation", label: t("settings.voice", "Voice") },
			{ key: "smart-prompts", label: t("settings.smartPrompts", "Smart Prompts") },
		],
	},
	{
		key: "integrations",
		label: t("settings.group.integrations", "Integrations"),
		tabs: [
			{ key: "mcp", label: t("settings.mcp", "MCP") },
			{ key: "remote-access", label: t("settings.remoteAccess", "Remote Access") },
			{ key: "remote-servers", label: t("settings.remoteServers", "Remote Servers") },
			{ key: "streamdock", label: t("settings.streamdock", "StreamDock") },
			{ key: "telegram", label: "Telegram" },
			{ key: "plugins", label: t("settings.plugins", "Plugins") },
		],
	},
];

/** Tabs whose feature is switched off right now, so their nav entry is noise. */
export function hiddenTabs(): Set<string> {
	const hidden = new Set<string>();
	// Dictation is no longer desktop-only: a browser holds a hands-free
	// conversation through its own microphone and speaker over a WS audio
	// socket (#832-e730). The controls that really are local — the global
	// hotkey and this machine's input devices — are hidden inside the tab
	// rather than by hiding the whole tab.
	// AI Chat configures ego, and ego is reachable only from the AI Chat panel.
	// While that panel is behind the experimental toggle, this tab would let a
	// person set a default model for an engine they cannot open.
	if (!settingsStore.isAiChatEnabled()) hidden.add("ai-chat");
	// The StreamDock macropad is a USB device on the desktop machine; the
	// backend supervisor only exists in the desktop build.
	if (!isTauri()) hidden.add("streamdock");
	return hidden;
}

/** The global tabs this build actually offers, in nav order. */
export function getGlobalTabs(): SettingsShellTab[] {
	const hidden = hiddenTabs();
	return GLOBAL_TAB_GROUPS.flatMap((group) => group.tabs.filter((tab) => !hidden.has(tab.key)));
}

/** Display label of a global tab, availability aside — search text and palette
 * labels need a name even for a tab the current build then filters out. */
export function globalTabLabel(key: string): string {
	for (const group of GLOBAL_TAB_GROUPS) {
		const tab = group.tabs.find((candidate) => candidate.key === key);
		if (tab) return tab.label;
	}
	return key;
}
