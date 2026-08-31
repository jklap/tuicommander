import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * `docs/user-guide/settings.md` documents most tabs at a conceptual/prose level (see e.g.
 * the "Agents Tab" and "GitHub Tab" sections), but the Terminal Tab's "Rendering"/
 * "Behavior"/"Blocks" groups document every individual control in a literal
 * `| **Label** | Type | Default | Description |` table — one bold row per toggle/select/
 * slider, matching the control's own on-screen label. That's exactly the convention
 * `efab3cbe` had to backfill by hand ("documents Font Weight, Cursor Style, and the PTY
 * prompt bar toggle … which had never had doc rows despite existing in the UI"). This test
 * enforces that convention going forward for these tabs: every `SettingToggle`/`SettingSelect`/
 * `SettingSlider` label in the tab's source must appear (case-insensitively — the doc and the
 * UI don't always agree on capitalization) as a bold row in its section, so adding a
 * control without a doc row turns the suite red instead of silently going undocumented.
 *
 * Scoped to these tabs only: the rest of settings.md predates this literal-table
 * convention and documents by concept rather than by exact control label, so a whole-file
 * version of this check would flag long-standing, intentional prose as "missing" rows.
 */

const doc = readFileSync(join(process.cwd(), "docs/user-guide/settings.md"), "utf8");

function extractLabels(source: string): string[] {
	// Two label forms appear in these files: label={t("key", "Text")} (i18n'd) and the
	// plain label="Text" (not yet i18n'd, e.g. most of SelectionTab.tsx).
	const labels: string[] = [];
	for (const match of source.matchAll(/label=\{t\("[^"]+",\s*"([^"]+)"\)\}/g)) {
		labels.push(match[1]);
	}
	for (const match of source.matchAll(/label="([^"]+)"/g)) {
		labels.push(match[1]);
	}
	return labels;
}

function tabSection(heading: string): string {
	const start = doc.indexOf(heading);
	expect(start).toBeGreaterThanOrEqual(0);
	const nextHeading = doc.indexOf("\n## ", start + 1);
	return nextHeading >= 0 ? doc.slice(start, nextHeading) : doc.slice(start);
}

const TABS = [
	{
		name: "Terminal",
		heading: "## Terminal Tab",
		path: "src/components/SettingsPanel/tabs/TerminalTab.tsx",
		minControls: 10,
	},
	{
		name: "Smart Selection",
		heading: "## Smart Selection Tab",
		path: "src/components/SettingsPanel/tabs/SelectionTab.tsx",
		// "Enable smart selection" was removed (folded into "Double-click performs" —
		// there is no longer a master on/off switch), dropping this from 5 to 4.
		minControls: 4,
	},
	{
		name: "Appearance",
		heading: "## Appearance Tab",
		path: "src/components/SettingsPanel/tabs/AppearanceTab.tsx",
		minControls: 7,
	},
];

describe.each(TABS)("settings.md $name Tab reference", ({ heading, path, minControls }) => {
	const tabSource = readFileSync(join(process.cwd(), path), "utf8");

	it("has at least the controls this test was written against (canary against the extraction regex going stale)", () => {
		const labels = extractLabels(tabSource);
		expect(labels.length).toBeGreaterThanOrEqual(minControls);
	});

	it("documents every control as a bold row in its settings.md section", () => {
		const section = tabSection(heading).toLowerCase();
		const undocumented = extractLabels(tabSource).filter((label) => !section.includes(`**${label.toLowerCase()}**`));
		expect(undocumented).toEqual([]);
	});
});
