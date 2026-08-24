import { describe, expect, it } from "vitest";
import {
	createCanvasSearchController,
	createCanvasSelectionController,
	extendSelectionDrag,
	wordBoundsAt,
} from "../canvasTerminalSelection";
import type { DecodedRow } from "../canvasTerminalUtils";

function row(text: string): DecodedRow {
	const codepoints = new Uint32Array([...text].map((character) => character.codePointAt(0) ?? 0));
	return {
		index: 0,
		count: codepoints.length,
		wrapped: false,
		codepoints,
		fg: new Uint32Array(codepoints.length),
		bg: new Uint32Array(codepoints.length),
		attrs: new Uint8Array(codepoints.length),
	};
}

describe("canvas terminal selection controller", () => {
	it("extracts forward and reverse multi-row selections and trims trailing space", () => {
		const rows = new Map([
			[3, row("alpha  ")],
			[4, row("bravo  ")],
			[5, row("charlie")],
		]);
		const selection = createCanvasSelectionController();
		selection.start = { row: 3, col: 2 };
		selection.end = { row: 5, col: 3 };
		expect(selection.getLocalText((index) => rows.get(index) ?? null)).toBe("pha\nbravo\nchar");

		selection.start = { row: 5, col: 3 };
		selection.end = { row: 3, col: 2 };
		expect(selection.getLocalText((index) => rows.get(index) ?? null)).toBe("pha\nbravo\nchar");
	});

	it("tracks ranges, offscreen rows, cached text, and complete reset", () => {
		const selection = createCanvasSelectionController();
		selection.selecting = true;
		selection.start = { row: 7, col: 1 };
		selection.end = { row: 8, col: 1 };
		selection.cachedText = "selected";
		expect(selection.hasRange()).toBe(true);
		expect(selection.spansOffscreen((absoluteRow) => (absoluteRow === 7 ? 0 : null))).toBe(true);

		selection.clear();
		expect(selection.selecting).toBe(false);
		expect(selection.start).toBeNull();
		expect(selection.end).toBeNull();
		expect(selection.cachedText).toBe("");
	});

	it("defaults mode to char and resets it on clear", () => {
		const selection = createCanvasSelectionController();
		expect(selection.mode).toBe("char");
		selection.mode = "word";
		expect(selection.mode).toBe("word");
		selection.clear();
		expect(selection.mode).toBe("char");
	});
});

describe("wordBoundsAt", () => {
	it("finds the word span around a column landing inside a word", () => {
		expect(wordBoundsAt(row("foo bar baz"), 5)).toEqual({ left: 4, right: 6 });
		expect(wordBoundsAt(row("foo bar baz"), 0)).toEqual({ left: 0, right: 2 });
		expect(wordBoundsAt(row("foo bar baz"), 10)).toEqual({ left: 8, right: 10 });
	});

	it("returns null when the column is whitespace or a separator", () => {
		expect(wordBoundsAt(row("foo bar"), 3)).toBeNull();
		expect(wordBoundsAt(row("a.b.c"), 1)).toBeNull();
	});

	it("returns null out of range and treats NUL cells as non-word", () => {
		expect(wordBoundsAt(row("abc"), -1)).toBeNull();
		expect(wordBoundsAt(row("abc"), 3)).toBeNull();
		const withGap = row("ab\0cd");
		expect(wordBoundsAt(withGap, 2)).toBeNull();
		expect(wordBoundsAt(withGap, 0)).toEqual({ left: 0, right: 1 });
	});

	it("stops at punctuation/quote separators on both sides", () => {
		expect(wordBoundsAt(row(`say "hello" now`), 5)).toEqual({ left: 5, right: 9 });
	});
});

describe("canvas terminal search controller", () => {
	const matches = [
		{ row: 2, col_start: 0, col_end: 2 },
		{ row: 12, col_start: 1, col_end: 3 },
		{ row: 15, col_start: 2, col_end: 4 },
	];

	it("starts at the last visible match and wraps navigation", () => {
		const search = createCanvasSearchController();
		expect(search.replace(matches, { historySize: 20, displayOffset: 10, screenRows: 8 })).toEqual(matches[2]);
		expect(search.activeIndex).toBe(2);
		expect(search.next()).toEqual(matches[0]);
		expect(search.previous()).toEqual(matches[2]);
	});

	it("uses the first match when none is visible and clears atomically", () => {
		const search = createCanvasSearchController();
		expect(search.replace(matches, { historySize: 100, displayOffset: 0, screenRows: 10 })).toEqual(matches[0]);
		search.clear();
		expect(search.matches).toEqual([]);
		expect(search.activeIndex).toBe(-1);
		expect(search.next()).toBeNull();
	});

	// A TUI (ink agents, vim) rewrites its live rows in place. Matches anchored to
	// those absolute rows describe text that no longer exists, and painting them
	// puts a highlight over cells that never matched the query.
	describe("dropRows", () => {
		it("drops matches on rewritten rows and keeps the rest", () => {
			const search = createCanvasSearchController();
			search.replace(matches, { historySize: 20, displayOffset: 10, screenRows: 8 });
			expect(search.dropRows(new Set([12]))).toBe(true);
			expect(search.matches).toEqual([matches[0], matches[2]]);
		});

		it("keeps the cursor on the active match when it survives", () => {
			const search = createCanvasSearchController();
			search.replace(matches, { historySize: 20, displayOffset: 10, screenRows: 8 });
			expect(search.activeIndex).toBe(2); // matches[2]
			search.dropRows(new Set([2]));
			expect(search.activeIndex).toBe(1);
			expect(search.matches[search.activeIndex]).toEqual(matches[2]);
		});

		it("falls back to the first match when the active one is rewritten", () => {
			const search = createCanvasSearchController();
			search.replace(matches, { historySize: 20, displayOffset: 10, screenRows: 8 });
			search.dropRows(new Set([15]));
			expect(search.activeIndex).toBe(0);
			expect(search.matches).toEqual([matches[0], matches[1]]);
		});

		it("resets to no active match when every match is rewritten", () => {
			const search = createCanvasSearchController();
			search.replace(matches, { historySize: 20, displayOffset: 10, screenRows: 8 });
			expect(search.dropRows(new Set([2, 12, 15]))).toBe(true);
			expect(search.matches).toEqual([]);
			expect(search.activeIndex).toBe(-1);
			expect(search.next()).toBeNull();
		});

		it("reports no change when the rewritten rows carry no match", () => {
			const search = createCanvasSearchController();
			search.replace(matches, { historySize: 20, displayOffset: 10, screenRows: 8 });
			expect(search.dropRows(new Set([3, 4, 99]))).toBe(false);
			expect(search.matches).toEqual(matches);
			expect(search.activeIndex).toBe(2);
		});

		it("is a no-op with no matches or no rewritten rows", () => {
			const search = createCanvasSearchController();
			expect(search.dropRows(new Set([1]))).toBe(false);
			search.replace(matches, { historySize: 20, displayOffset: 10, screenRows: 8 });
			expect(search.dropRows(new Set())).toBe(false);
			expect(search.matches).toEqual(matches);
		});
	});
});

// Regression coverage for 26a3d449: double/triple-click-drag used to degrade to plain
// character selection instead of extending by word/line.
describe("extendSelectionDrag", () => {
	const currentStart = { row: 0, col: 0 };

	it("word mode, dragging forward: anchor's left stays the start, drag word's right is the end", () => {
		const result = extendSelectionDrag(
			"word",
			{ wordAnchor: { row: 5, left: 2, right: 6 }, lineAnchorRow: null },
			{ row: 7, col: 10, bounds: { left: 8, right: 12 }, maxCol: 79 },
			currentStart,
		);
		expect(result).toEqual({ start: { row: 5, col: 2 }, end: { row: 7, col: 12 } });
	});

	it("word mode, dragging backward: drag word's left is the start, anchor's right is the end", () => {
		const result = extendSelectionDrag(
			"word",
			{ wordAnchor: { row: 5, left: 2, right: 6 }, lineAnchorRow: null },
			{ row: 3, col: 1, bounds: { left: 0, right: 1 }, maxCol: 79 },
			currentStart,
		);
		expect(result).toEqual({ start: { row: 3, col: 0 }, end: { row: 5, col: 6 } });
	});

	it("word mode, same row with dragLeft === anchor.left, is treated as forward", () => {
		const result = extendSelectionDrag(
			"word",
			{ wordAnchor: { row: 5, left: 2, right: 6 }, lineAnchorRow: null },
			{ row: 5, col: 2, bounds: { left: 2, right: 6 }, maxCol: 79 },
			currentStart,
		);
		expect(result).toEqual({ start: { row: 5, col: 2 }, end: { row: 5, col: 6 } });
	});

	it("word mode over whitespace mid-drag falls back to the raw column without losing the anchor", () => {
		const result = extendSelectionDrag(
			"word",
			{ wordAnchor: { row: 5, left: 2, right: 6 }, lineAnchorRow: null },
			{ row: 7, col: 20, bounds: null, maxCol: 79 },
			currentStart,
		);
		expect(result).toEqual({ start: { row: 5, col: 2 }, end: { row: 7, col: 20 } });
	});

	it("line mode dragging forward selects full rows from the anchor through the drag row", () => {
		const result = extendSelectionDrag(
			"line",
			{ wordAnchor: null, lineAnchorRow: 4 },
			{ row: 8, col: 30, bounds: null, maxCol: 79 },
			currentStart,
		);
		expect(result).toEqual({ start: { row: 4, col: 0 }, end: { row: 8, col: 79 } });
	});

	it("line mode dragging backward selects full rows from the drag row through the anchor", () => {
		const result = extendSelectionDrag(
			"line",
			{ wordAnchor: null, lineAnchorRow: 8 },
			{ row: 4, col: 30, bounds: null, maxCol: 79 },
			currentStart,
		);
		expect(result).toEqual({ start: { row: 4, col: 0 }, end: { row: 8, col: 79 } });
	});

	it("char mode extends by plain cell, leaving start untouched", () => {
		const result = extendSelectionDrag(
			"char",
			{ wordAnchor: null, lineAnchorRow: null },
			{ row: 2, col: 9, bounds: null, maxCol: 79 },
			{ row: 1, col: 3 },
		);
		expect(result).toEqual({ start: { row: 1, col: 3 }, end: { row: 2, col: 9 } });
	});

	it("word mode with a null wordAnchor (double-click landed on punctuation) falls back to plain cell-wise extension", () => {
		const result = extendSelectionDrag(
			"word",
			{ wordAnchor: null, lineAnchorRow: null },
			{ row: 2, col: 9, bounds: null, maxCol: 79 },
			{ row: 1, col: 3 },
		);
		expect(result).toEqual({ start: { row: 1, col: 3 }, end: { row: 2, col: 9 } });
	});

	it("line mode with a null lineAnchorRow falls back to plain cell-wise extension", () => {
		const result = extendSelectionDrag(
			"line",
			{ wordAnchor: null, lineAnchorRow: null },
			{ row: 2, col: 9, bounds: null, maxCol: 79 },
			{ row: 1, col: 3 },
		);
		expect(result).toEqual({ start: { row: 1, col: 3 }, end: { row: 2, col: 9 } });
	});
});
