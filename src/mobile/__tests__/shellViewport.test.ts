import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { shellViewport } from "../shellViewport";

// iPhone standalone PWA, viewport-fit=cover: 874 css px tall, 62 top inset, 34 home-indicator inset.
// Boss's screenshot (2026-10-11) shows the shell ending 62 px early, equal to the top inset:
// visualViewport.height reports 812 there, so a shell sized by it leaves a band under the tabs.
const SCREEN = 874;
const TOP_INSET = 62;

describe("shellViewport", () => {
	it("fills the screen to the bottom edge when only the status-bar inset shortens visualViewport", () => {
		const shell = shellViewport({ height: SCREEN - TOP_INSET, offsetTop: 0 }, SCREEN);
		// `auto` height with top 0 and bottom 0 in CSS reaches the physical bottom.
		expect(shell).toMatchObject({ top: "0px", height: "auto" });
	});

	it("keeps the home-indicator padding while no keyboard is open", () => {
		const shell = shellViewport({ height: SCREEN - TOP_INSET, offsetTop: 0 }, SCREEN);
		expect(shell.bottomInset).toBe("env(safe-area-inset-bottom, 0px)");
	});

	it("follows visualViewport above the keyboard and drops the home-indicator padding", () => {
		const shell = shellViewport({ height: 480, offsetTop: 12 }, SCREEN);
		expect(shell).toEqual({ top: "12px", height: "480px", bottomInset: "0px" });
	});
});

describe("bottom tabs safe area", () => {
	const css = readFileSync(resolve(__dirname, "../components/BottomTabs.module.css"), "utf8");

	it("pads the tab row by the bottom inset so the home indicator does not cover the labels", () => {
		expect(css).toMatch(/padding-bottom:\s*var\(--safe-bottom/);
		expect(css).toMatch(/height:\s*calc\(56px \+ var\(--safe-bottom/);
	});
});
