import { describe, expect, it, vi } from "vitest";
import {
	commitSelectionCopy,
	createCanvasSearchController,
	createCanvasSelectionController,
	isSelectionRowsExpiredError,
	selectionRowToGridRow,
	selectionRowToViewport,
	shouldValidateSelectionSnapshot,
	viewportRowToSelectionRow,
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
	it("keeps a selected row stable while parked history grows", () => {
		const before = { historyBase: 40, historySize: 100, displayOffset: 50, screenRows: 24 };
		const selectedRow = viewportRowToSelectionRow(before, 7);
		expect(selectedRow).toBe(97);

		const afterOutput = { historyBase: 40, historySize: 101, displayOffset: 51, screenRows: 24 };
		expect(selectionRowToViewport(afterOutput, selectedRow)).toBe(7);
	});

	it("does not alias an evicted selection onto the replacement grid row", () => {
		const selectedRow = viewportRowToSelectionRow(
			{ historyBase: 40, historySize: 100, displayOffset: 100, screenRows: 24 },
			0,
		);
		expect(selectedRow).toBe(40);

		const afterEviction = { historyBase: 41, historySize: 100, displayOffset: 100, screenRows: 24 };
		expect(selectionRowToViewport(afterEviction, selectedRow)).toBeNull();
		expect(selectionRowToGridRow(afterEviction, selectedRow)).toBeNull();
	});

	it("keeps a retained selected row fixed when eviction and parked offset advance together", () => {
		const before = { historyBase: 40, historySize: 100, displayOffset: 50, screenRows: 24 };
		const selectedRow = viewportRowToSelectionRow(before, 7);
		const afterEviction = { historyBase: 41, historySize: 100, displayOffset: 51, screenRows: 24 };

		expect(selectionRowToViewport(afterEviction, selectedRow)).toBe(7);
		expect(selectionRowToGridRow(afterEviction, selectedRow)).toBe(56);
	});

	it("converts a retained stable selection row back to the backend grid coordinate", () => {
		const viewport = { historyBase: 40, historySize: 100, displayOffset: 50, screenRows: 24 };
		expect(selectionRowToGridRow(viewport, 97)).toBe(57);
	});

	it("recognizes the backend rejection for selection rows evicted before the read lock", () => {
		expect(isSelectionRowsExpiredError(new Error("selection rows are no longer retained"))).toBe(true);
		expect(isSelectionRowsExpiredError(new Error("clipboard unavailable"))).toBe(false);
	});

	it("does not write the clipboard when eviction wins the backend selection-read race", async () => {
		const write = vi.fn(async () => {});
		await expect(
			commitSelectionCopy(async () => {
				throw new Error("selection rows are no longer retained");
			}, write),
		).resolves.toEqual({ kind: "expired" });
		expect(write).not.toHaveBeenCalled();
	});

	it("invalidates copied text when a new selection gesture starts", () => {
		const selection = createCanvasSelectionController();
		selection.start = { row: 7, col: 1 };
		selection.end = { row: 7, col: 4 };
		selection.cachedText = "previous range";

		selection.invalidateSnapshot();

		expect(selection.cachedText).toBe("");
		expect(selection.start).toEqual({ row: 7, col: 1 });
		expect(selection.end).toEqual({ row: 7, col: 4 });
	});

	it("validates replacement text only after the selection gesture is complete", () => {
		const selection = createCanvasSelectionController();
		selection.start = { row: 7, col: 1 };
		selection.end = { row: 7, col: 4 };
		selection.cachedText = "selected";
		selection.selecting = true;

		expect(shouldValidateSelectionSnapshot(selection, true, () => 0)).toBe(false);
		selection.selecting = false;
		expect(shouldValidateSelectionSnapshot(selection, true, () => 0)).toBe(true);
		expect(shouldValidateSelectionSnapshot(selection, false, () => 0)).toBe(false);
	});

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

	it("copies every codepoint attached to a selected cell", () => {
		const selected = row("cafe");
		selected.cellExtras = new Map([[3, "\u0301\uFE0F"]]);
		const selection = createCanvasSelectionController();
		selection.start = { row: 0, col: 0 };
		selection.end = { row: 0, col: 3 };
		expect(selection.getLocalText(() => selected)).toBe("cafe\u0301\uFE0F");
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
