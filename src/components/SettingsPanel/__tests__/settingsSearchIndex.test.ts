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
	terminal: "tabs/TerminalTab.tsx",
	"keyboard-shortcuts": "tabs/KeyboardShortcutsTab.tsx",
	dictation: "DictationSettings.tsx",
	github: "tabs/GitHubTab.tsx",
	mcp: "tabs/services/LocalMcpPanel.tsx",
	"remote-access": "tabs/services/RemoteAccessPanel.tsx",
	"remote-machines": "tabs/RemoteMachinesTab.tsx",
	plugins: "tabs/PluginsTab.tsx",
	"smart-prompts": "tabs/SmartPromptsTab.tsx",
	agents: "tabs/AgentsTab.tsx",
	"ai-chat": "tabs/AiChatTab.tsx",
	"developer-tools": "tabs/DeveloperToolsTab.tsx",
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
	terminal: { dynamic: 0, orphans: 0 },
	// The `<label>{section.title}</label>` in the shortcut-conflict list — the
	// section title is computed at runtime and has no static scroll target.
	"keyboard-shortcuts": { dynamic: 1, orphans: 0 },
	dictation: { dynamic: 0, orphans: 0 },
	github: { dynamic: 0, orphans: 0 },
	mcp: { dynamic: 0, orphans: 0 },
	"remote-access": { dynamic: 0, orphans: 0 },
	// RemoteMachinesTab is a thin wrapper (heading + <RemoteMachinesPanel/>);
	// the panel's own internal labels are not inlined here, so nothing to
	// index or count. Full indexing of the panel's own content is story 860.
	"remote-machines": { dynamic: 0, orphans: 0 },
	plugins: { dynamic: 1, orphans: 0 },
	"smart-prompts": { dynamic: 4, orphans: 11 },
	// 8th: the per-agent "Native status signals" toggle, which sits in the same
	// runtime-rendered card as "Install hooks globally" and so cannot have a
	// static scroll target either.
	// The orphan is the machine selector's "Configure agents on" label. It is not
	// a setting — it scopes every setting below it to one machine — so it sits
	// above the first heading on purpose and has nothing to scroll to.
	agents: { dynamic: 8, orphans: 1 },
	// The `<optgroup label={provider.name}>` inside the default-model picker. It
	// groups the options by provider and is not a setting anybody can scroll to.
	"ai-chat": { dynamic: 1, orphans: 0 },
	"developer-tools": { dynamic: 0, orphans: 0 },
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
			expect.objectContaining({ tab: "remote-access", section: "Cloud Relay", label: "Relay Server URL" }),
		]);
	});

	it("matches case-insensitively on any word order", () => {
		const hits = searchSettings("THEME terminal", ALL_TABS);
		expect(hits).toEqual([expect.objectContaining({ tab: "terminal", section: "Theme", label: "Terminal Theme" })]);
	});

	it("matches a section heading, and the settings inside it", () => {
		const hits = searchSettings("power management", ALL_TABS);
		// The section entry first, then every setting it contains — searching a
		// heading is how you browse a section you cannot name a field in.
		expect(hits[0]).toEqual(expect.objectContaining({ tab: "terminal", section: "Power Management" }));
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
