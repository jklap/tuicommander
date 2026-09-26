import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const stylesheet = readFileSync("src/components/ActivityDashboard/ActivityDashboard.module.css", "utf8");

function dashboardWidthAt(viewportWidth: number): number {
	const rule = stylesheet.match(/\.dashboard\s*\{([^}]+)\}/)?.[1];
	const preferred = Number(rule?.match(/\bwidth:\s*(\d+)px/)?.[1]);
	const viewportLimit = Number(rule?.match(/\bmax-width:\s*(\d+)vw/)?.[1]);
	expect(Number.isFinite(preferred)).toBe(true);
	expect(Number.isFinite(viewportLimit)).toBe(true);
	return Math.min(preferred, (viewportWidth * viewportLimit) / 100);
}

describe("Activity Dashboard layout", () => {
	it("keeps the inline overlay close to the detached window's compact width", () => {
		// The detached panel opens at 550px and the shared 90vw cap makes its
		// visible dashboard 495px. The inline version should stay near that size.
		const detachedWidth = dashboardWidthAt(550);
		expect(detachedWidth).toBe(495);
		expect(dashboardWidthAt(1440)).toBeLessThanOrEqual(detachedWidth + 5);
	});

	it("fits a narrow main window and restores compact sizing after reattachment", () => {
		const firstInlineWidth = dashboardWidthAt(1440);
		expect(dashboardWidthAt(320)).toBeLessThanOrEqual(320 * 0.9);
		// Detaching changes the viewport, not the component's shared CSS rule.
		expect(dashboardWidthAt(550)).toBe(495);
		expect(dashboardWidthAt(1440)).toBe(firstInlineWidth);
	});
});
