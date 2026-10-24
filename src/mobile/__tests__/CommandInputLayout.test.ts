import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const menuCss = readFileSync(resolve(__dirname, "../components/MobileComposerMenu.module.css"), "utf-8");
const css = readFileSync(resolve(__dirname, "../components/CommandInput.module.css"), "utf-8");
// All blocks for a selector (including the shared `.send, .dictate` one), in source order.
const rule = (name: string) =>
	[...css.matchAll(new RegExp(`\\.${name}\\s*\\{([^}]*)\\}`, "g"))].map((m) => m[1]).join(";\n");
// Last declaration wins, as in the cascade.
const px = (block: string, prop: string) =>
	Number([...block.matchAll(new RegExp(`(?:^|[;\\s])${prop}:\\s*(-?\\d+)px`, "g"))].at(-1)?.[1] ?? Number.NaN);
// `inset: top right bottom left` as plain px numbers
const inset = (block: string) => {
	const v = (block.match(/inset:\s*([^;]+);/)?.[1] ?? "")
		.trim()
		.split(/\s+/)
		.map((n) => Number.parseInt(n, 10));
	const [top, right = top, bottom = top, left = right] = v;
	return { top, right, bottom, left };
};
// `padding: top right bottom left` -> [left, right]; falls back to padding-left/right.
const sides = (block: string) => {
	const v = (block.match(/(?:^|[;\s])padding:\s*([^;]+);/)?.[1] ?? "")
		.trim()
		.split(/\s+/)
		.map((n) => Number.parseInt(n, 10));
	if (v.length === 4) return { left: v[3], right: v[1] };
	return { left: px(block, "padding-left"), right: px(block, "padding-right") };
};
// `margin: top right bottom left` -> left
const marginLeft = (block: string) => {
	const v = (block.match(/(?:^|[;\s])margin:\s*([^;]+);/)?.[1] ?? "0")
		.trim()
		.split(/\s+/)
		.map((n) => Number.parseInt(n, 10));
	const [top, right = top, , left = right] = v;
	return left;
};

const send = rule("send");
const sendHit = css.match(/\.send::before\s*\{([^}]*)\}/)?.[1] ?? "";
const dictate = rule("dictate");
const form = rule("form");

describe("mobile command input layout (send outside the box)", () => {
	// Catches: shrinking the send touch target below 44px tall.
	it("keeps a send hit area of at least 44px tall", () => {
		const i = inset(sendHit);
		expect(px(send, "height") - i.top - i.bottom).toBeGreaterThanOrEqual(44);
	});

	// Catches: the send hit area growing left over the mic inside the box.
	it("does not let the send hit area reach left past the gap to the box", () => {
		const gap = px(form, "gap") + marginLeft(send);
		expect(-inset(sendHit).left).toBeLessThanOrEqual(gap);
	});

	// Catches: growing the combined button and spacing budget at the expense of the textarea.
	it("keeps the horizontal space around the text within the 114px budget", () => {
		const budget =
			px(menuCss.match(/\.trigger\s*\{([^}]*)\}/)?.[1] ?? "", "width") +
			sides(rule("input")).left +
			sides(rule("input")).right +
			px(rule("composer"), "padding-right") +
			px(dictate, "width") +
			px(form, "gap") +
			marginLeft(send) +
			px(send, "width");
		expect(budget).toBeLessThanOrEqual(114);
	});

	// Catches: a fixed px width/max-width on the textarea or its box, which would stop it filling
	// the row on wide viewports (iPad 820/1024px) - the 286px figure is an iPhone result, not a value.
	it("keeps the textarea and its box fluid in the row", () => {
		for (const name of ["input", "composer"]) {
			const block = rule(name);
			expect(block, name).not.toMatch(/(?:^|[;\s])(?:max-)?width:/);
			expect(block, name).toMatch(/flex:\s*1\b/);
			expect(block, name).toMatch(/min-width:\s*0\b/);
		}
		expect(form).not.toMatch(/(?:^|[;\s])(?:max-)?width:/);
	});
});
