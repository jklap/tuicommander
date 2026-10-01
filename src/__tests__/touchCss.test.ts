import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

// jsdom cannot scroll or hit-test a finger, so the touch CSS contract is read
// from the stylesheets (#1329-a31a). Each case names the tablet bug it catches.

const css = (file: string) => readFileSync(resolve(__dirname, "..", file), "utf8");

/** Body of the first `@media (hover: none) { ... }` block in `source`. */
const hoverNoneBlock = (source: string) => {
	const start = source.indexOf("@media (hover: none)");
	if (start < 0) throw new Error("no (hover: none) block");
	return source.slice(start, source.indexOf("\n}\n", start));
};

describe("touch stylesheet contract (#1329-a31a)", () => {
	// Catches: the global touch-action turning the side panels into non-scrolling
	// surfaces on iOS (`manipulation` blocks pan-scroll recognition).
	it("global touch-action on touch devices keeps panning and does not use manipulation", () => {
		const block = hoverNoneBlock(css("global.css"));
		expect(block).toMatch(/touch-action:\s*pan-x pan-y/);
		expect(block).not.toMatch(/touch-action:\s*manipulation/);
	});

	// Catches: the branch + (and its long-press agent list) hidden behind :hover,
	// unreachable on a tablet.
	it("sidebar branch actions are shown on touch devices", () => {
		const block = hoverNoneBlock(css("components/Sidebar/Sidebar.module.css"));
		expect(block).toContain(".branchActions");
		expect(block).toContain("pointer-events: auto");
		expect(block).toContain("opacity: 1");
	});
});
