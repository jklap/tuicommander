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
 * enforces that convention going forward for this tab: every `SettingToggle`/`SettingSelect`/
 * `SettingSlider` label in TerminalTab.tsx must appear (case-insensitively — the doc and the
 * UI don't always agree on capitalization) as a bold row in that section, so adding a
 * control without a doc row turns the suite red instead of silently going undocumented.
 *
 * Scoped to TerminalTab only: the rest of settings.md predates this literal-table
 * convention and documents by concept rather than by exact control label, so a whole-file
 * version of this check would flag long-standing, intentional prose as "missing" rows.
 */
describe("settings.md Terminal Tab reference", () => {
	const tabSource = readFileSync(join(process.cwd(), "src/components/SettingsPanel/tabs/TerminalTab.tsx"), "utf8");
	const doc = readFileSync(join(process.cwd(), "docs/user-guide/settings.md"), "utf8");

	function terminalTabSection(): string {
		const start = doc.indexOf("## Terminal Tab");
		expect(start).toBeGreaterThanOrEqual(0);
		const nextHeading = doc.indexOf("\n## ", start + 1);
		return nextHeading >= 0 ? doc.slice(start, nextHeading) : doc.slice(start);
	}

	function extractLabels(source: string): string[] {
		// Two label forms appear in this file: label={t("key", "Text")} (i18n'd) and the
		// plain label="Text" (not yet i18n'd, e.g. the PTY prompt bar toggle).
		const labels: string[] = [];
		for (const match of source.matchAll(/label=\{t\("[^"]+",\s*"([^"]+)"\)\}/g)) {
			labels.push(match[1]);
		}
		for (const match of source.matchAll(/label="([^"]+)"/g)) {
			labels.push(match[1]);
		}
		return labels;
	}

	it("has at least the controls this test was written against (canary against the extraction regex going stale)", () => {
		const labels = extractLabels(tabSource);
		expect(labels.length).toBeGreaterThanOrEqual(10);
	});

	it("documents every TerminalTab control as a bold row in its settings.md section", () => {
		const section = terminalTabSection().toLowerCase();
		const undocumented = extractLabels(tabSource).filter((label) => !section.includes(`**${label.toLowerCase()}**`));
		expect(undocumented).toEqual([]);
	});
});
