import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { TIPS } from "../../data/tips";

/** Page labels of the global Settings nav, read from `GLOBAL_TAB_GROUPS` in the
 * panel's tab list (`settingsTabs.ts`, shared with the Command Palette) so a
 * renamed or removed page fails here, not in front of a user. */
function settingsPageLabels(): Set<string> {
	const src = fs.readFileSync(path.join(__dirname, "../../components/SettingsPanel/settingsTabs.ts"), "utf8");
	const start = src.indexOf("const GLOBAL_TAB_GROUPS");
	const body = src.slice(start, src.indexOf("\n];", start));
	const labels = new Set<string>();
	for (const m of body.matchAll(/\{\s*key:\s*"[^"]+",\s*label:\s*(?:t\("[^"]+",\s*"([^"]+)"\)|"([^"]+)")\s*\}/g)) {
		labels.add(m[1] ?? m[2]);
	}
	return labels;
}

/** Every "Settings → Page" / "Settings > Page" a tip names, with its page. */
function settingsPaths(): { feature: string; page: string }[] {
	return TIPS.flatMap((tip) =>
		[...tip.description.matchAll(/Settings\s*(?:→|>)\s*([A-Z][\w&]*(?:\s(?:&\s)?[A-Z][\w&]*)*)/g)].map((m) => ({
			feature: tip.feature,
			page: m[1],
		})),
	);
}

describe("tips", () => {
	it("reads the Settings pages from the panel source", () => {
		// Guards the parser: an empty set would make the next test pass vacuously.
		expect([...settingsPageLabels()]).toEqual(expect.arrayContaining(["General", "Agents", "MCP", "Git & GitHub"]));
		expect(settingsPaths().length).toBeGreaterThan(0);
	});

	it("names only Settings pages that exist in the navigation", () => {
		const pages = settingsPageLabels();
		const stale = settingsPaths().filter(({ page }) => !pages.has(page));
		expect(stale).toEqual([]);
	});

	// The protocol is always on (`kitty_keyboard: true` in the terminal grid) and
	// an agent turns it on by requesting it. No Settings page has ever had a
	// toggle for it, so a tip that sends the user looking for one is wrong.
	it("does not send the user to Settings for the Kitty keyboard protocol", () => {
		const kitty = TIPS.find((tip) => tip.feature === "Kitty Keyboard Protocol");
		expect(kitty).toBeDefined();
		expect(kitty?.description).not.toMatch(/Settings/);
	});
});
