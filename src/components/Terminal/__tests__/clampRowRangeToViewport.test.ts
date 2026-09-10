import { describe, expect, it } from "vitest";
import { clampRowRangeToViewport } from "../canvasTerminalUtils";

/**
 * Code-review follow-up: two real bugs (`paintFoldedBlocks`, `paintSearchScopeIndicator`
 * in CanvasTerminal.tsx) shipped from mapping each edge of a multi-row range through
 * `absRowToViewport` independently and defaulting a `null` edge — that can't tell "this
 * edge scrolled past the viewport, but the range still overlaps it" apart from "this
 * range doesn't overlap the viewport at all". This module checks overlap first.
 */
describe("clampRowRangeToViewport", () => {
	const viewTop = 100;
	const viewBottom = 124; // 24-row viewport: absolute rows [100, 124)

	it("returns the exact range unclamped when fully inside the viewport", () => {
		expect(clampRowRangeToViewport(105, 110, viewTop, viewBottom)).toEqual({ startVp: 5, endVp: 9 });
	});

	it("clamps the start when the range begins above the viewport but still overlaps it", () => {
		// range [90, 105) — starts 10 rows above viewTop, ends inside the viewport.
		expect(clampRowRangeToViewport(90, 105, viewTop, viewBottom)).toEqual({ startVp: 0, endVp: 4 });
	});

	it("clamps the end when the range extends below the viewport but still overlaps it", () => {
		// range [120, 200) — starts inside the viewport, ends far below it.
		expect(clampRowRangeToViewport(120, 200, viewTop, viewBottom)).toEqual({ startVp: 20, endVp: 23 });
	});

	it("clamps BOTH edges when the range spans past the viewport on both sides", () => {
		// The exact shape that broke both callers: both edges individually map
		// out of range, yet the range still covers the entire viewport.
		expect(clampRowRangeToViewport(0, 1000, viewTop, viewBottom)).toEqual({ startVp: 0, endVp: 23 });
	});

	it("returns null when the range is entirely above the viewport", () => {
		expect(clampRowRangeToViewport(10, 50, viewTop, viewBottom)).toBeNull();
	});

	it("returns null when the range is entirely below the viewport", () => {
		expect(clampRowRangeToViewport(200, 300, viewTop, viewBottom)).toBeNull();
	});

	it("returns null for a range that ends exactly at viewTop (half-open, no overlap)", () => {
		expect(clampRowRangeToViewport(50, viewTop, viewTop, viewBottom)).toBeNull();
	});

	it("returns null for a range that starts exactly at viewBottom (half-open, no overlap)", () => {
		expect(clampRowRangeToViewport(viewBottom, viewBottom + 10, viewTop, viewBottom)).toBeNull();
	});

	it("handles an unbounded (Infinity) end row for a still-open block", () => {
		expect(clampRowRangeToViewport(105, Number.POSITIVE_INFINITY, viewTop, viewBottom)).toEqual({
			startVp: 5,
			endVp: 23,
		});
	});
});
