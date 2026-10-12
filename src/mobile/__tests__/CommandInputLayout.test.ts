import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(__dirname, "../components/CommandInput.module.css"), "utf-8");
// All blocks for a selector (including the shared `.send, .dictate` one), in source order.
const rule = (name: string) =>
	[...css.matchAll(new RegExp(`\\.${name}\\s*\\{([^}]*)\\}`, "g"))].map((m) => m[1]).join(";\n");
// Last declaration wins, as in the cascade.
const px = (block: string, prop: string) =>
	Number(
		[...block.matchAll(new RegExp(`(?:^|[;\\s])${prop}:\\s*(-?\\d+)px`, "g"))].at(-1)?.[1] ?? Number.NaN,
	);
// `inset: top right bottom left` as plain px numbers
const inset = (block: string) => {
	const v = (block.match(/inset:\s*([^;]+);/)?.[1] ?? "").trim().split(/\s+/).map((n) => Number.parseInt(n, 10));
	const [top, right = top, bottom = top, left = right] = v;
	return { top, right, bottom, left };
};
const lastPaddingRight = (block: string) => {
	const m = block.match(/(?:^|[;\s])padding:\s*([^;]+);/)?.[1].trim().split(/\s+/).map((n) => Number.parseInt(n, 10));
	if (!m) return px(block, "padding-right");
	return m.length === 1 ? m[0] : m[1];
};

const send = rule("send");
const sendHit = css.match(/\.send::before\s*\{([^}]*)\}/)?.[1] ?? "";
const dictate = rule("dictate");

describe("mobile command input right-side layout", () => {
	// Catches: shrinking the send touch target below 44px tall, or letting it grow left over the mic.
	it("keeps a send hit area of at least 44px tall that never reaches over the mic", () => {
		const i = inset(sendHit);
		expect(px(send, "height") - i.top - i.bottom).toBeGreaterThanOrEqual(44);
		expect(i.left).toBeGreaterThanOrEqual(0);
	});

	// Catches: growing paddings, mic or send so the textarea wrap width drops under 286px at 390px.
	it("keeps paddings + mic + send within the 62px right-side budget", () => {
		const budget =
			px(rule("composer"), "padding-right") +
			lastPaddingRight(rule("input")) +
			px(dictate, "width") +
			px(send, "width");
		expect(budget).toBeLessThanOrEqual(62);
	});
});
