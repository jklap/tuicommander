import { describe, expect, it } from "vitest";
import { SETTINGS_SEARCH_RESULTS, searchSettings } from "../../../components/SettingsPanel/settingsSearchIndex";

describe("settingsSearchIndex", () => {
	it("returns no results for an empty or whitespace-only query", () => {
		expect(searchSettings("")).toEqual([]);
		expect(searchSettings("   ")).toEqual([]);
	});

	it("finds a setting by its exact label", () => {
		const results = searchSettings("copy on select");
		expect(results.some((r) => r.controlId === "setting-copy-on-select" && r.tab === "terminal")).toBe(true);
	});

	it("finds a setting via a hint word, not just the label", () => {
		// "chime" only appears in this setting's hint text, not its label.
		const results = searchSettings("chime");
		expect(results.some((r) => r.label === "Silence completions from MCP sessions")).toBe(true);
	});

	it("matches on a tab name via bm25's prefix expansion", () => {
		// "term" should prefix-match "terminal" the same way it does in bm25.ts's
		// own doc comment example for the Command Palette.
		const results = searchSettings("term shell");
		expect(results.some((r) => r.tab === "terminal" && r.label === "Shell")).toBe(true);
	});

	it("controlId is derived consistently from the label (matches settingSlugId)", () => {
		const shell = SETTINGS_SEARCH_RESULTS.find((r) => r.tab === "terminal" && r.label === "Shell");
		expect(shell?.controlId).toBe("setting-shell");
	});

	// Duplicate controlIds within the same tab would mean two entries jump to
	// the same DOM element, or (once a control loops per-instance) collide
	// with a non-searchable row's id — this guards the registry itself, not
	// the rendered app, against silently adding a same-label duplicate.
	it("has no duplicate controlId within any single tab", () => {
		const seenPerTab = new Map<string, Set<string>>();
		for (const item of SETTINGS_SEARCH_RESULTS) {
			const seen = seenPerTab.get(item.tab) ?? new Set<string>();
			expect(seen.has(item.controlId)).toBe(false);
			seen.add(item.controlId);
			seenPerTab.set(item.tab, seen);
		}
	});

	it("only indexes known global tab keys", () => {
		const knownTabs = new Set([
			"general",
			"appearance",
			"terminal",
			"selection",
			"notifications",
			"dictation",
			"github",
			"services",
			"plugins",
			"smart-prompts",
			"providers",
			"agents",
			"ai-chat",
		]);
		for (const item of SETTINGS_SEARCH_RESULTS) {
			expect(knownTabs.has(item.tab)).toBe(true);
		}
	});
});
