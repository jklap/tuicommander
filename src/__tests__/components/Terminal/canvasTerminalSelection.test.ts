import { describe, expect, it } from "vitest";
import {
	createCanvasSearchController,
	extendSelectionDrag,
	type SearchMatch,
	wordBoundsAt,
} from "../../../components/Terminal/canvasTerminalSelection";
import type { DecodedRow } from "../../../components/Terminal/canvasTerminalUtils";

/**
 * `createCanvasSearchController` (replace/dropRows/next/previous/nearestVisibleMatch)
 * had zero tests anywhere in the suite before this file — confirmed by grep. It backs
 * the terminal's Cmd+F search, including the "Search in Block" toggle in TerminalSearch.tsx.
 */

function match(row: number, col_start = 0, col_end = 5): SearchMatch {
	return { row, col_start, col_end };
}

describe("createCanvasSearchController", () => {
	describe("replace", () => {
		it("starts with no matches and activeIndex -1", () => {
			const search = createCanvasSearchController();
			expect(search.matches).toEqual([]);
			expect(search.activeIndex).toBe(-1);
		});

		it("selects the nearest match to the viewport when one is given", () => {
			const search = createCanvasSearchController();
			const matches = [match(0), match(50), match(100)];
			// viewport covers rows [40, 60)
			const active = search.replace(matches, { historySize: 50, displayOffset: 0, screenRows: 20 });
			expect(active).toEqual(match(50));
			expect(search.activeIndex).toBe(1);
		});

		it("defaults to the first match when no viewport is given", () => {
			const search = createCanvasSearchController();
			const matches = [match(0), match(50)];
			const active = search.replace(matches);
			expect(active).toEqual(match(0));
			expect(search.activeIndex).toBe(0);
		});

		it("returns null and activeIndex -1 when replacing with an empty match list", () => {
			const search = createCanvasSearchController();
			search.replace([match(0)]);
			const active = search.replace([]);
			expect(active).toBeNull();
			expect(search.activeIndex).toBe(-1);
		});

		/**
		 * Issue #8 fix: toggling "Search in Block" re-runs the search and calls
		 * `replace()` with a BRAND-NEW match array from a fresh `terminal_search` IPC
		 * call — not the same array `dropRows` filters in place — so the previously
		 * active match must be found by VALUE (row/col_start/col_end), not reference.
		 * When it survives in the new list, `replace()` keeps the cursor on it instead
		 * of recomputing from viewport proximity, even if the viewport moved meanwhile.
		 */
		it("keeps the active match (by value) across a replace() with a new but value-equal list", () => {
			const search = createCanvasSearchController();
			search.replace([match(0), match(50), match(100)], { historySize: 50, displayOffset: 0, screenRows: 4 });
			expect(search.matches[search.activeIndex]).toEqual(match(50));

			// A fresh IPC round-trip returns value-equal matches (new array/object
			// identity), and the viewport has scrolled up near the top by the time the
			// toggle's search resolves — viewport proximity alone would now pick row 0.
			const fresh = [match(0), match(50), match(100)].map((m) => ({ ...m }));
			const active = search.replace(fresh, { historySize: 0, displayOffset: 0, screenRows: 4 });
			expect(active).toEqual(match(50));
			expect(search.activeIndex).toBe(1);
		});

		it("falls back to viewport proximity when the previously active match is genuinely gone", () => {
			const search = createCanvasSearchController();
			search.replace([match(0), match(50), match(100)], { historySize: 50, displayOffset: 0, screenRows: 4 });
			expect(search.matches[search.activeIndex]).toEqual(match(50));

			// Narrower scope (e.g. block-scoped search) drops row 50 entirely.
			const active = search.replace([match(0), match(100)], { historySize: 0, displayOffset: 0, screenRows: 4 });
			expect(active).toEqual(match(0));
		});

		/**
		 * Issue #9 (code-review follow-up on #8's fix): the by-value preservation
		 * must be opt-in via `preserveActiveByValue`, not automatic on every call —
		 * otherwise a genuinely NEW, unrelated query could coincidentally inherit
		 * "active" from an old match landing at the same row/col by chance, silently
		 * overriding what should be a fresh nearestVisibleMatch pick.
		 */
		it("does NOT preserve by value when preserveActiveByValue is false (a genuinely new query)", () => {
			const search = createCanvasSearchController();
			search.replace([match(0), match(50), match(100)], { historySize: 50, displayOffset: 0, screenRows: 4 });
			expect(search.matches[search.activeIndex]).toEqual(match(50));

			// A totally different, unrelated query happens to also match at row 50 —
			// coincidence, not continuity.
			const active = search.replace([match(0), match(50)], { historySize: 0, displayOffset: 0, screenRows: 4 }, false);
			// Recomputed from viewport proximity (row 0), not carried over from the
			// unrelated previous query's match at row 50.
			expect(active).toEqual(match(0));
		});

		it("preserves by value when preserveActiveByValue is omitted (defaults to true)", () => {
			const search = createCanvasSearchController();
			search.replace([match(0), match(50)], { historySize: 50, displayOffset: 0, screenRows: 4 });
			expect(search.matches[search.activeIndex]).toEqual(match(50));

			const active = search.replace([match(0), match(50)], { historySize: 0, displayOffset: 0, screenRows: 4 });
			expect(active).toEqual(match(50));
		});
	});

	describe("dropRows", () => {
		it("keeps the cursor on the same match (by reference) when it survives", () => {
			const search = createCanvasSearchController();
			const matches = [match(10), match(20), match(30)];
			search.replace(matches, { historySize: 20, displayOffset: 0, screenRows: 4 });
			expect(search.matches[search.activeIndex]).toEqual(match(20));

			const changed = search.dropRows(new Set([10]));
			expect(changed).toBe(true);
			expect(search.matches).toHaveLength(2);
			expect(search.matches[search.activeIndex]).toEqual(match(20));
		});

		it("falls back to the first surviving match when the active one is dropped", () => {
			const search = createCanvasSearchController();
			search.replace([match(10), match(20), match(30)], { historySize: 20, displayOffset: 0, screenRows: 4 });
			expect(search.matches[search.activeIndex]).toEqual(match(20));

			search.dropRows(new Set([20]));
			expect(search.matches).toHaveLength(2);
			expect(search.activeIndex).toBe(0);
		});

		it("clears activeIndex to -1 when every match is dropped", () => {
			const search = createCanvasSearchController();
			search.replace([match(10)]);
			search.dropRows(new Set([10]));
			expect(search.matches).toEqual([]);
			expect(search.activeIndex).toBe(-1);
		});

		it("returns false and leaves state untouched when no row in the set matches", () => {
			const search = createCanvasSearchController();
			search.replace([match(10), match(20)]);
			const changed = search.dropRows(new Set([999]));
			expect(changed).toBe(false);
			expect(search.matches).toHaveLength(2);
		});

		it("returns false for an empty matches list or an empty rows set", () => {
			const search = createCanvasSearchController();
			expect(search.dropRows(new Set([1]))).toBe(false);
			search.replace([match(10)]);
			expect(search.dropRows(new Set())).toBe(false);
		});
	});

	describe("next / previous", () => {
		it("wraps forward and backward through the match list", () => {
			const search = createCanvasSearchController();
			search.replace([match(0), match(10), match(20)]);
			expect(search.activeIndex).toBe(0);

			expect(search.next()).toEqual(match(10));
			expect(search.next()).toEqual(match(20));
			expect(search.next()).toEqual(match(0)); // wraps
			expect(search.previous()).toEqual(match(20)); // wraps backward
		});

		it("returns null when there are no matches", () => {
			const search = createCanvasSearchController();
			expect(search.next()).toBeNull();
			expect(search.previous()).toBeNull();
		});
	});

	describe("clear", () => {
		it("resets matches and activeIndex", () => {
			const search = createCanvasSearchController();
			search.replace([match(0), match(10)]);
			search.clear();
			expect(search.matches).toEqual([]);
			expect(search.activeIndex).toBe(-1);
		});
	});
});

/** Sanity coverage for two other exports in this module referenced by canvas gutter/drag
 *  logic, so the file's other pure helpers aren't left completely untested either. */
describe("wordBoundsAt", () => {
	function row(text: string): DecodedRow {
		return {
			index: 0,
			count: text.length,
			wrapped: false,
			codepoints: Uint32Array.from(text, (c) => c.codePointAt(0) ?? 0),
			fg: new Uint32Array(text.length),
			bg: new Uint32Array(text.length),
			attrs: new Uint8Array(text.length),
		};
	}

	it("finds word boundaries around a click column", () => {
		expect(wordBoundsAt(row("foo bar"), 5)).toEqual({ left: 4, right: 6 });
	});

	it("returns null when the click lands on a separator", () => {
		expect(wordBoundsAt(row("foo bar"), 3)).toBeNull();
	});
});

describe("extendSelectionDrag", () => {
	it("plain char mode extends only the end point", () => {
		const result = extendSelectionDrag(
			"char",
			{ wordAnchor: null, lineAnchorRow: null },
			{ row: 5, col: 10, bounds: null, maxCol: 80 },
			{ row: 3, col: 2 },
		);
		expect(result).toEqual({ start: { row: 3, col: 2 }, end: { row: 5, col: 10 } });
	});
});
