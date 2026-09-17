import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { SETTINGS_SEARCH_INDEX, type SettingsSearchEntry, searchSettings } from "../settingsSearchIndex";
import { extractRenderedTabKeys, extractTab } from "./extractSettings";

/** Nav key → the source file that renders that tab's settings. */
const TAB_SOURCES: Record<string, string> = {
	general: "tabs/GeneralTab.tsx",
	appearance: "tabs/AppearanceTab.tsx",
	notifications: "tabs/NotificationsTab.tsx",
	dictation: "DictationSettings.tsx",
	github: "tabs/GitHubTab.tsx",
	services: "tabs/ServicesTab.tsx",
	plugins: "tabs/PluginsTab.tsx",
	"smart-prompts": "tabs/SmartPromptsTab.tsx",
	providers: "tabs/ProvidersTab.tsx",
	agents: "tabs/AgentsTab.tsx",
	"ai-chat": "tabs/AiChatTab.tsx",
};

/** Occurrences the extraction rule cannot index, pinned so a new one is loud.
 *
 * `dynamic` = heading or label text computed at runtime (per-agent cards,
 * per-plugin rows). `orphans` = a label with no `<h3>` above it, i.e. a modal
 * form field. Neither has a stable scroll target. If a count moves, look at
 * what was added: give it a static heading if it is a real setting, otherwise
 * update the number here. */
const UNINDEXABLE: Record<string, { dynamic: number; orphans: number }> = {
	general: { dynamic: 0, orphans: 0 },
	appearance: { dynamic: 0, orphans: 0 },
	notifications: { dynamic: 0, orphans: 0 },
	dictation: { dynamic: 0, orphans: 0 },
	github: { dynamic: 0, orphans: 0 },
	services: { dynamic: 0, orphans: 0 },
	plugins: { dynamic: 1, orphans: 0 },
	"smart-prompts": { dynamic: 4, orphans: 11 },
	providers: { dynamic: 3, orphans: 0 },
	// 8th: the per-agent "Native status signals" toggle, which sits in the same
	// runtime-rendered card as "Install hooks globally" and so cannot have a
	// static scroll target either. 9th: the per-agent "Collect progress"
	// override, in that same card.
	agents: { dynamic: 9, orphans: 1 },
	"ai-chat": { dynamic: 2, orphans: 0 },
};

const readTab = (file: string) => fs.readFileSync(path.join(__dirname, "..", file), "utf8");

/** Rebuild the index for one tab straight from its JSX, in file order. */
function derive(tab: string): SettingsSearchEntry[] {
	const extracted = extractTab(readTab(TAB_SOURCES[tab]));
	const entries: SettingsSearchEntry[] = [];
	for (const section of extracted.sections) {
		entries.push({ tab, section: section.text, ...(section.key ? { sectionKey: section.key } : {}) });
	}
	for (const setting of extracted.settings) {
		if (!setting.section) continue;
		entries.push({
			tab,
			section: setting.section,
			label: setting.text,
			...(setting.key ? { labelKey: setting.key } : {}),
		});
	}
	return entries;
}

describe("settings search index — drift guard", () => {
	it("indexes every tab SettingsPanel can open", () => {
		const rendered = extractRenderedTabKeys(readTab("SettingsPanel.tsx")).sort();
		expect(rendered).toEqual(Object.keys(TAB_SOURCES).sort());
	});

	it.each(Object.keys(TAB_SOURCES))("matches the settings rendered by %s", (tab) => {
		const indexed = SETTINGS_SEARCH_INDEX.filter((entry) => entry.tab === tab);
		expect(indexed).toEqual(derive(tab));
	});

	it.each(Object.keys(TAB_SOURCES))("has no new unindexable occurrence in %s", (tab) => {
		const extracted = extractTab(readTab(TAB_SOURCES[tab]));
		expect({
			dynamic: extracted.dynamic,
			orphans: extracted.settings.filter((s) => !s.section).length,
		}).toEqual(UNINDEXABLE[tab]);
	});

	it("indexes no tab the panel cannot open", () => {
		const tabs = new Set(SETTINGS_SEARCH_INDEX.map((entry) => entry.tab));
		expect([...tabs].sort()).toEqual(Object.keys(TAB_SOURCES).sort());
	});
});

const ALL_TABS = new Set(Object.keys(TAB_SOURCES));

describe("searchSettings", () => {
	it("returns nothing for an empty query", () => {
		expect(searchSettings("", ALL_TABS)).toEqual([]);
		expect(searchSettings("   ", ALL_TABS)).toEqual([]);
	});

	it("finds a setting in a tab that is not mounted", () => {
		const hits = searchSettings("relay server url", ALL_TABS);
		expect(hits).toEqual([
			expect.objectContaining({ tab: "services", section: "Cloud Relay", label: "Relay Server URL" }),
		]);
	});

	it("matches case-insensitively on any word order", () => {
		const hits = searchSettings("THEME terminal", ALL_TABS);
		expect(hits).toEqual([expect.objectContaining({ tab: "appearance", section: "Theme", label: "Terminal Theme" })]);
	});

	it("matches a section heading, and the settings inside it", () => {
		const hits = searchSettings("power management", ALL_TABS);
		// The section entry first, then every setting it contains — searching a
		// heading is how you browse a section you cannot name a field in.
		expect(hits[0]).toEqual(expect.objectContaining({ tab: "general", section: "Power Management" }));
		expect(hits[0].label).toBeUndefined();
		expect(hits.map((e) => e.label)).toEqual([
			undefined,
			"Prevent sleep when busy",
			"Auto-Standby Timeout",
			"Content Indexing",
		]);
	});

	it("spans tabs — one query, several tabs", () => {
		const tabs = new Set(searchSettings("default", ALL_TABS).map((e) => e.tab));
		expect(tabs.size).toBeGreaterThan(1);
	});

	it("hides settings whose tab is not available", () => {
		expect(searchSettings("whisper model", ALL_TABS)).toHaveLength(1);
		const withoutDictation = new Set([...ALL_TABS].filter((k) => k !== "dictation"));
		expect(searchSettings("whisper model", withoutDictation)).toEqual([]);
	});

	it("returns nothing for a query that matches no setting", () => {
		expect(searchSettings("zzzzz no such setting", ALL_TABS)).toEqual([]);
	});
});
