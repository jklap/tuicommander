import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { SETTINGS_SEARCH_RESULTS } from "../../../components/SettingsPanel/settingsSearchIndex";

/**
 * Guards the exact bug a 2026-09-10 code review caught: several
 * `settingsSearchIndex.ts` entries pointed at controls that were rendered via
 * hand-rolled `<div class={s.group}>` markup instead of the shared
 * `SettingToggle`/`SettingSelect`/`SettingSlider`/`SettingInput` components —
 * so they never got the `settingSlugId()`-derived `id` those components stamp
 * automatically, and the search/Command-Palette "jump to and highlight"
 * feature silently no-op'd for them (tab switches correctly, nothing
 * scrolls/highlights). Rendering all 12 tabs with their full store
 * dependency graphs just to check this would be a heavy, brittle test setup;
 * instead this reads each tab's own source (same technique as
 * `settingsDoc.test.ts`'s `extractLabels()`) and confirms every indexed
 * label is *reachable* by an id — either automatically, via a shared
 * component's `label` prop, or explicitly, via a hand-rolled
 * `id={settingSlugId(...)}` on that control's own wrapper.
 */

const TAB_FILES: Record<string, string> = {
	general: "src/components/SettingsPanel/tabs/GeneralTab.tsx",
	appearance: "src/components/SettingsPanel/tabs/AppearanceTab.tsx",
	terminal: "src/components/SettingsPanel/tabs/TerminalTab.tsx",
	selection: "src/components/SettingsPanel/tabs/SelectionTab.tsx",
	notifications: "src/components/SettingsPanel/tabs/NotificationsTab.tsx",
	dictation: "src/components/SettingsPanel/DictationSettings.tsx",
	github: "src/components/SettingsPanel/tabs/GitHubTab.tsx",
	services: "src/components/SettingsPanel/tabs/ServicesTab.tsx",
	agents: "src/components/SettingsPanel/tabs/AgentsTab.tsx",
	"ai-chat": "src/components/SettingsPanel/tabs/AiChatTab.tsx",
};

const sourceCache = new Map<string, string>();
function readTabSource(tab: string): string {
	const cached = sourceCache.get(tab);
	if (cached !== undefined) return cached;
	const path = TAB_FILES[tab];
	expect(path, `no source file mapped for tab "${tab}" — add it to TAB_FILES above`).toBeTruthy();
	const source = readFileSync(join(process.cwd(), path), "utf8");
	sourceCache.set(tab, source);
	return source;
}

/** Labels reaching a DOM id automatically, via a shared SettingFields component's `label` prop.
 *  Same two forms `settingsDoc.test.ts` already extracts: `label={t("key","Text")}` and `label="Text"`. */
function sharedComponentLabels(source: string): Set<string> {
	const labels = new Set<string>();
	for (const m of source.matchAll(/label=\{t\("[^"]+",\s*"([^"]+)"\)\}/g)) labels.add(m[1]);
	for (const m of source.matchAll(/label="([^"]+)"/g)) labels.add(m[1]);
	return labels;
}

/** Labels reaching a DOM id explicitly, via a hand-rolled `id={settingSlugId(...)}` on the control's own wrapper. */
function explicitSlugIdLabels(source: string): Set<string> {
	const labels = new Set<string>();
	for (const m of source.matchAll(/id=\{settingSlugId\(t\("[^"]+",\s*"([^"]+)"\)\)\}/g)) labels.add(m[1]);
	for (const m of source.matchAll(/id=\{settingSlugId\("([^"]+)"\)\}/g)) labels.add(m[1]);
	return labels;
}

describe("settingsSearchIndex coverage — every indexed setting resolves to a real DOM id", () => {
	for (const item of SETTINGS_SEARCH_RESULTS) {
		it(`${item.tab} > ${item.section} > ${item.label}`, () => {
			const source = readTabSource(item.tab);
			const reachable = sharedComponentLabels(source).has(item.label) || explicitSlugIdLabels(source).has(item.label);
			expect(
				reachable,
				`"${item.label}" (${item.tab}) has no shared-component label= prop and no explicit id={settingSlugId(...)} in ${TAB_FILES[item.tab]} — search/Command-Palette selection will switch tabs but silently fail to scroll/highlight it`,
			).toBe(true);
		});
	}
});
