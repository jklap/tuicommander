import { t } from "../../i18n";
import { settingsStore } from "../../stores/settings";
import { isTauri } from "../../transport";
import type { SettingsShellTab } from "./SettingsShell";

/** Global (non-repo) Settings tabs, in nav order.
 *
 * Lives outside `SettingsPanel.tsx` so the Command Palette's per-setting
 * actions (`useCommandPaletteActions`) can name a tab without importing the
 * whole lazily-loaded panel. */
export const BASE_GLOBAL_TABS: SettingsShellTab[] = [
	{ key: "general", label: t("settings.general", "General") },
	{ key: "appearance", label: t("settings.appearance", "Appearance") },
	{ key: "terminal", label: t("settings.terminal", "Terminal") },
	{ key: "selection", label: t("settings.selection", "Smart Selection") },
	{ key: "notifications", label: t("settings.notifications", "Notifications") },
	{ key: "dictation", label: t("settings.dictation", "Dictation") },
	{ key: "streamdock", label: t("settings.streamdock", "StreamDock") },
	{ key: "github", label: "Git & GitHub" },
	{ key: "services", label: t("settings.services", "Services & MCP") },
	{ key: "remote-access", label: t("settings.remoteAccess", "Remote Access") },
	{ key: "remote-servers", label: t("settings.remoteServers", "Remote Servers") },
	{ key: "plugins", label: t("settings.plugins", "Plugins") },
	{ key: "smart-prompts", label: t("settings.smartPrompts", "Smart Prompts") },
	{ key: "providers", label: "Providers" },
	{ key: "agents", label: t("settings.agents", "Agents") },
];

const AI_CHAT_TAB: SettingsShellTab = { key: "ai-chat", label: "AI Chat" };

/** The global tabs this build actually offers: Dictation and StreamDock are
 * desktop-only, AI Chat sits behind its experimental flag. */
export function getGlobalTabs(): SettingsShellTab[] {
	const tabs = isTauri()
		? BASE_GLOBAL_TABS
		: BASE_GLOBAL_TABS.filter((tab) => tab.key !== "dictation" && tab.key !== "streamdock");
	if (settingsStore.isAiChatEnabled()) {
		return [...tabs, AI_CHAT_TAB];
	}
	return tabs;
}

/** Display label of a global tab, availability aside — search text and palette
 * labels need a name even for a tab the current build then filters out. */
export function globalTabLabel(key: string): string {
	if (key === AI_CHAT_TAB.key) return AI_CHAT_TAB.label;
	return BASE_GLOBAL_TABS.find((tab) => tab.key === key)?.label ?? key;
}
