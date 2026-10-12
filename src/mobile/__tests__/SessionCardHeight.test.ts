import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(__dirname, "../components/SessionCard.module.css"), "utf-8");
const rule = (name: string) => css.match(new RegExp(`\\.${name}\\s*\\{([^}]*)\\}`))?.[1] ?? "";
const px = (block: string, prop: string) =>
	Number(block.match(new RegExp(`(?:^|[;\\s])${prop}:\\s*(-?\\d+)px`))?.[1] ?? 0);
const margins = (block: string) => {
	const values = (block.match(/(?:^|[;\s])margin:\s*([^;]+);/)?.[1] ?? "0")
		.trim()
		.split(/\s+/)
		.map((v) => Number.parseInt(v, 10));
	const [top, right = top, bottom = top, left = right] = values;
	return { top, right, bottom, left };
};

describe("session card kill button", () => {
	const kill = rule("killBtn");

	// Catches: the 44px touch target stacked under the timestamp adding about 16px of empty
	// space at the bottom of every card (Boss's iPhone screenshot, 2026-10-12).
	it("takes no more than 28px of card height while keeping a 44px touch target", () => {
		expect(px(kill, "height")).toBe(44);
		expect(px(kill, "width")).toBe(44);
		const m = margins(kill);
		expect(px(kill, "height") + m.top + m.bottom).toBeLessThanOrEqual(28);
	});

	// Catches: the enlarged touch target reaching left over the card's open-session button,
	// so a tap near the card body kills the session.
	it("does not extend its touch target toward the open-session button", () => {
		expect(margins(kill).left).toBeGreaterThanOrEqual(0);
	});
});
