import { readFileSync } from "node:fs";
import { join } from "node:path";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The "?" badge on the General tab's ego heading, rendered through the real
// GeneralTab: the assertions read the DOM a user hovers, not the InfoBadge module.

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));

vi.mock("../../../invoke", () => ({
	invoke: mockInvoke,
	listen: vi.fn().mockResolvedValue(vi.fn()),
}));

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { GeneralTab } from "../tabs/GeneralTab";

// vitest returns no CSS for modules, so read the stylesheet the app ships.
const settingsCss = readFileSync(join(__dirname, "../Settings.module.css"), "utf8");

const VIEWPORT_WIDTH = 1024;
const TIP_WIDTH = 320;

const rect = (left: number, width: number, top = 100, height = 16) =>
	({ left, right: left + width, top, bottom: top + height, width, height, x: left, y: top }) as DOMRect;

/** Render General and return the ego heading's badge and tip. */
async function renderEgoBadge() {
	const { container } = render(() => <GeneralTab />);
	await waitFor(() => expect(container.querySelector("#settings-ego")).not.toBeNull());
	const badge = container.querySelector("#settings-ego > span") as HTMLElement;
	return { badge, tip: badge.lastElementChild as HTMLElement };
}

describe("General tab ego info badge", () => {
	let originalWidth: number;
	beforeEach(() => {
		mockInvoke.mockResolvedValue(undefined);
		originalWidth = window.innerWidth;
		Object.defineProperty(window, "innerWidth", { value: VIEWPORT_WIDTH, configurable: true });
	});
	afterEach(() => {
		cleanup();
		for (const el of document.head.querySelectorAll("style[data-test-css]")) el.remove();
		Object.defineProperty(window, "innerWidth", { value: originalWidth, configurable: true });
	});

	/** Hover a badge whose layout box is faked: happy-dom has no layout engine. */
	async function hoverBadgeAt(left: number) {
		const { badge, tip } = await renderEgoBadge();
		badge.getBoundingClientRect = () => rect(left, 16);
		tip.getBoundingClientRect = () => rect(0, TIP_WIDTH, 0, 60);
		fireEvent.mouseEnter(badge);
		const tipLeft = Number.parseFloat(tip.style.left);
		return { tipLeft, tipRight: tipLeft + TIP_WIDTH };
	}

	it("keeps the tip inside the viewport for a badge near the right edge", async () => {
		// catches: tip anchored at the badge's left edge, overflowing right and clipped by the scroller
		const { tipRight } = await hoverBadgeAt(VIEWPORT_WIDTH - 40);
		expect(tipRight).toBeLessThanOrEqual(VIEWPORT_WIDTH);
	});

	it("keeps the tip inside the viewport for a badge near the left edge", async () => {
		// catches: a right-anchored fix that brings back the left-edge clipping of 341414d4f
		const { tipLeft } = await hoverBadgeAt(4);
		expect(tipLeft).toBeGreaterThanOrEqual(0);
	});

	it("anchors the tip at the badge when there is room", async () => {
		// catches: an over-eager clamp that moves every tip to the viewport edge
		const { tipLeft } = await hoverBadgeAt(300);
		expect(tipLeft).toBe(300);
	});

	it("shows the tip while the badge is hovered and hides it otherwise", async () => {
		// Catches: measured hover placement never exposes the tip or leaves it visible after exit.
		const style = document.createElement("style");
		style.setAttribute("data-test-css", "");
		style.textContent = settingsCss;
		document.head.appendChild(style);
		const { badge, tip } = await renderEgoBadge();
		const shown = () => {
			const css = getComputedStyle(tip);
			return css.visibility !== "hidden" && css.display !== "none";
		};
		expect(shown()).toBe(false);
		fireEvent.mouseEnter(badge);
		expect(shown()).toBe(true);
		fireEvent.mouseLeave(badge);
		expect(shown()).toBe(false);
	});
});
