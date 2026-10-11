import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { InfoBadge } from "../InfoBadge";

const VIEWPORT_WIDTH = 1024;
const TIP_WIDTH = 320;

const rect = (left: number, width: number, top = 100, height = 16) =>
	({ left, right: left + width, top, bottom: top + height, width, height, x: left, y: top }) as DOMRect;

/** Hover a badge whose layout box is faked: jsdom has no layout engine. */
function hoverBadgeAt(left: number) {
	const { getByRole } = render(() => <InfoBadge>Tip text</InfoBadge>);
	const tip = getByRole("tooltip");
	const badge = tip.parentElement as HTMLElement;
	badge.getBoundingClientRect = () => rect(left, 16);
	tip.getBoundingClientRect = () => rect(0, TIP_WIDTH, 0, 60);
	fireEvent.mouseEnter(badge);
	const tipLeft = Number.parseFloat(tip.style.left);
	return { tipLeft, tipRight: tipLeft + TIP_WIDTH };
}

describe("InfoBadge tip placement", () => {
	let originalWidth: number;
	beforeEach(() => {
		originalWidth = window.innerWidth;
		Object.defineProperty(window, "innerWidth", { value: VIEWPORT_WIDTH, configurable: true });
	});
	afterEach(() => {
		Object.defineProperty(window, "innerWidth", { value: originalWidth, configurable: true });
	});

	it("keeps the tip inside the viewport for a badge near the right edge", () => {
		// catches: tip anchored at the badge's left edge, overflowing right and clipped by the scroller
		const { tipRight } = hoverBadgeAt(VIEWPORT_WIDTH - 40);
		expect(tipRight).toBeLessThanOrEqual(VIEWPORT_WIDTH);
	});

	it("keeps the tip inside the viewport for a badge near the left edge", () => {
		// catches: a right-anchored fix that brings back the left-edge clipping of 341414d4f
		const { tipLeft } = hoverBadgeAt(4);
		expect(tipLeft).toBeGreaterThanOrEqual(0);
	});

	it("anchors the tip at the badge when there is room", () => {
		// catches: an over-eager clamp that moves every tip to the viewport edge
		const { tipLeft } = hoverBadgeAt(300);
		expect(tipLeft).toBe(300);
	});
});
