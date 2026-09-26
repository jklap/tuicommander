import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(__dirname, "../screens/SessionDetailScreen.module.css"), "utf-8");

/** Extracts a rule/keyframes block's body, respecting nested `{}` (e.g. `@keyframes` steps). */
function ruleBody(selector: string): string {
	const start = css.indexOf(selector);
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

describe("SessionDetailScreen indeterminate progress sweep", () => {
	// The detail screen no longer has its own header progress bar (progress moved
	// into the top bar's overflow menu), so there is currently no sweep here to keep
	// in sync with TabBar.module.css / SessionCard.module.css. If a sweep is ever
	// re-added, it must use the same wrapping pattern — see TabBar.module.css's test.
	const selector = '.headerProgressFill[data-kind="indeterminate"]';

	it("never carries the old snap-back sweep (single no-repeat band, -50% -> 150%)", () => {
		expect(css).not.toMatch(/background-position:\s*-50%/);
		expect(css).not.toMatch(/background-position:\s*150%/);
	});

	it("uses the wrapping two-band 200% tile if a header sweep exists", () => {
		if (!css.includes(selector)) return;
		const rule = ruleBody(selector);
		expect(rule).toMatch(/background-repeat:\s*repeat-x/);
		expect(rule).toMatch(/background-size:\s*200%\s+100%/);
		expect((rule.match(/var\(--accent\)/g) ?? []).length).toBe(2);
	});
});
