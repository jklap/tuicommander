import { describe, expect, it } from "vitest";
import type { CellMetrics } from "../canvasTerminalUtils";
import { computeCursorRect, isWideCursorGlyph, shouldPaintCursor } from "../canvasTerminalUtils";

const m: CellMetrics = {
	cellWidth: 10,
	cellHeight: 20,
	baseline: 15,
	fontSize: 14,
	dpr: 1,
	scaledCellWidth: 10,
	scaledCellHeight: 20,
};

describe("isWideCursorGlyph", () => {
	it("is false for a glyph measured at roughly one cell wide", () => {
		expect(isWideCursorGlyph(9, 10)).toBe(false);
		expect(isWideCursorGlyph(10, 10)).toBe(false);
	});

	it("is true for a glyph measured at roughly two cells wide (CJK, emoji)", () => {
		expect(isWideCursorGlyph(20, 10)).toBe(true);
	});

	it("uses a midpoint threshold so measurement jitter near one cell doesn't flip it", () => {
		expect(isWideCursorGlyph(14, 10)).toBe(false); // 1.4x — still narrow
		expect(isWideCursorGlyph(16, 10)).toBe(true); // 1.6x — wide
	});
});

describe("computeCursorRect — wide-glyph span", () => {
	it("defaults to one cell wide when spanCols is omitted", () => {
		expect(computeCursorRect("block", 0, 3, m)).toEqual({ x: 30, y: 0, w: 10, h: 20 });
		expect(computeCursorRect("underline", 0, 3, m)).toEqual({ x: 30, y: 18, w: 10, h: 2 });
	});

	it("widens a block cursor to two cells when spanCols=2", () => {
		expect(computeCursorRect("block", 0, 3, m, 2)).toEqual({ x: 30, y: 0, w: 20, h: 20 });
	});

	it("widens an underline cursor to two cells when spanCols=2", () => {
		expect(computeCursorRect("underline", 0, 3, m, 2)).toEqual({ x: 30, y: 18, w: 20, h: 2 });
	});

	it("never widens a beam cursor — it stays a fixed-width insertion marker", () => {
		expect(computeCursorRect("beam", 0, 3, m, 1)).toEqual({ x: 30, y: 0, w: 2, h: 20 });
		expect(computeCursorRect("beam", 0, 3, m, 2)).toEqual({ x: 30, y: 0, w: 2, h: 20 });
	});
});

describe("shouldPaintCursor", () => {
	it("follows the blink phase when the app hasn't requested a steady cursor", () => {
		expect(shouldPaintCursor(true, false)).toBe(true);
		expect(shouldPaintCursor(false, false)).toBe(false);
	});

	it("always paints when the app requested a steady cursor, regardless of blink phase", () => {
		expect(shouldPaintCursor(true, true)).toBe(true);
		expect(shouldPaintCursor(false, true)).toBe(true);
	});
});
