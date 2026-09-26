import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(__dirname, "../TabBar.module.css"), "utf-8");

/** Extracts a rule/keyframes block's body, respecting nested `{}` (e.g. `@keyframes` steps). */
function ruleBody(selector: string): string {
	const start = css.indexOf(selector);
	expect(start, `selector ${selector} not found`).toBeGreaterThanOrEqual(0);
	const open = css.indexOf("{", start);
	let depth = 1;
	let i = open + 1;
	while (depth > 0) {
		if (css[i] === "{") depth++;
		else if (css[i] === "}") depth--;
		i++;
	}
	return css.slice(open + 1, i - 1);
}

describe("TabBar indeterminate progress sweep", () => {
	// Regression coverage for a real bug: an earlier version used `background-repeat:
	// no-repeat` with a tile the same width as the container, which made the band slide
	// fully off-screen and snap back to the start every cycle instead of wrapping.
	const rule = ruleBody('.progress[data-kind="indeterminate"]');

	it("repeats the band horizontally instead of using no-repeat", () => {
		expect(rule).toMatch(/background-repeat:\s*repeat-x/);
	});

	it("uses a 200%-wide tile, not a 100%-wide one", () => {
		// A background-size equal to the container's own size (100% 100%) makes
		// percentage-based background-position a no-op per the CSS spec (offset =
		// (container - image) * percent = 0), so the sweep would be motionless.
		expect(rule).toMatch(/background-size:\s*200%\s+100%/);
		expect(rule).not.toMatch(/background-size:\s*100%\s+100%/);
	});

	it("holds two copies of the same fading band inside the tile", () => {
		expect(rule).toMatch(/background-image:\s*linear-gradient\(/);
		const accentStops = rule.match(/var\(--accent\)/g) ?? [];
		expect(accentStops.length).toBe(2);
	});

	const keyframes = ruleBody("@keyframes progressIndeterminateSweep");

	it("animates background-position from 100% down to 0% (left-to-right sweep, seamless loop)", () => {
		expect(keyframes).toMatch(/0%\s*{\s*background-position:\s*100%\s*0;?\s*}/);
		expect(keyframes).toMatch(/100%\s*{\s*background-position:\s*0%\s*0;?\s*}/);
	});
});
