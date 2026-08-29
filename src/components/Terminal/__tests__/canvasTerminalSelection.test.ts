import { describe, expect, it } from "vitest";
import { DEFAULT_WORD_SEPARATORS } from "../../../stores/settings";
import {
	buildSmartSelectionWindow,
	createCanvasSearchController,
	createCanvasSelectionController,
	createWordBoundaryResolver,
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

describe("buildSmartSelectionWindow", () => {
	function rowsMap(rows: Record<number, DecodedRow>): (absRow: number) => DecodedRow | null {
		return (absRow) => rows[absRow] ?? null;
	}

	it("joins rows with a newline and maps each character back to its grid coordinate", () => {
		const rows = { 9: row("foo"), 10: row("bar"), 11: row("baz") };
		const win = buildSmartSelectionWindow(10, 1, 2, rowsMap(rows));
		expect(win.text).toBe("foo\nbar\nbaz");
		expect(win.coords[0]).toEqual({ row: 9, col: 0 });
		// The middle row's "b" (index 4 in "foo\nbar\nbaz") maps back to row 10, col 0.
		expect(win.coords[win.text.indexOf("bar")]).toEqual({ row: 10, col: 0 });
	});

	it("computes targetOffset at the click's position within the joined text", () => {
		const rows = { 9: row("foo"), 10: row("bar"), 11: row("baz") };
		const win = buildSmartSelectionWindow(10, 1, 2, rowsMap(rows));
		// offset of "bar"'s col 1 ('a') within "foo\nbar\nbaz"
		expect(win.targetOffset).toBe("foo\n".length + 1);
		expect(win.text[win.targetOffset]).toBe("a");
	});

	it("skips missing rows near the edge of scrollback without inserting a blank line", () => {
		const rows = { 0: row("first"), 1: row("second") };
		const win = buildSmartSelectionWindow(0, 0, 2, rowsMap(rows));
		expect(win.text).toBe("first\nsecond");
	});

	it("joins a wrapped row's continuation with no separator", () => {
		const wrappedRow = row("abc");
		wrappedRow.wrapped = true;
		const rows = { 5: wrappedRow, 6: row("def") };
		const win = buildSmartSelectionWindow(5, 1, 1, rowsMap(rows));
		expect(win.text).toBe("abcdef");
		// "def"'s 'd' maps to row 6 col 0, not shifted by a phantom newline.
		expect(win.coords[3]).toEqual({ row: 6, col: 0 });
	});

	it("returns targetOffset -1 when the clicked row itself is missing (defensive; shouldn't happen in practice)", () => {
		const rows = { 10: row("bar") };
		const win = buildSmartSelectionWindow(99, 0, 2, rowsMap(rows));
		expect(win.targetOffset).toBe(-1);
	});

	it("renders NUL cells as spaces, matching getLocalText's convention", () => {
		const withGap = row("ab\0cd");
		const rows = { 0: withGap };
		const win = buildSmartSelectionWindow(0, 0, 0, rowsMap(rows));
		expect(win.text).toBe("ab cd");
	});
});

describe("createWordBoundaryResolver", () => {
	describe("characters mode", () => {
		it("with the default separator string, matches wordBoundsAt exactly across the shared test table", () => {
			const resolver = createWordBoundaryResolver({
				mode: "characters",
				separators: DEFAULT_WORD_SEPARATORS,
				regexAlternates: "",
			});
			const cases: [DecodedRow, number][] = [
				[row("foo bar baz"), 5],
				[row("foo bar baz"), 0],
				[row("foo bar baz"), 10],
				[row("foo bar"), 3],
				[row("a.b.c"), 1],
				[row("abc"), -1],
				[row("abc"), 3],
				[row(`say "hello" now`), 5],
			];
			for (const [r, col] of cases) {
				expect(resolver(r, col)).toEqual(wordBoundsAt(r, col));
			}
		});

		it("with an empty separator string, falls back to wordBoundsAt's identity (not just equivalent behavior)", () => {
			const resolver = createWordBoundaryResolver({ mode: "characters", separators: "", regexAlternates: "" });
			expect(resolver).toBe(wordBoundsAt);
		});

		it("a custom separator string changes what breaks a word — e.g. treating '-' as a separator", () => {
			const resolver = createWordBoundaryResolver({
				mode: "characters",
				separators: `${DEFAULT_WORD_SEPARATORS}-`,
				regexAlternates: "",
			});
			expect(resolver(row("foo-bar"), 1)).toEqual({ left: 0, right: 2 });
		});

		it("a custom separator string can also REMOVE a default separator — e.g. letting '.' join a word", () => {
			const resolver = createWordBoundaryResolver({
				mode: "characters",
				separators: DEFAULT_WORD_SEPARATORS.replace(".", ""),
				regexAlternates: "",
			});
			expect(resolver(row("app.config.ts"), 1)).toEqual({ left: 0, right: 12 });
		});

		it("still treats whitespace and control characters as separators regardless of the custom string", () => {
			const resolver = createWordBoundaryResolver({ mode: "characters", separators: "", regexAlternates: "" });
			expect(resolver(row("foo bar"), 3)).toBeNull();
		});
	});

	describe("regex mode", () => {
		it("the motivating case: adding 'https://' as an alternate makes a double-click on the host include the scheme", () => {
			// Word-boundary regex mode does fine per-character joining (like
			// iTerm2's own "-|+|_|~" defaults) — extending a click's run onto an
			// adjacent literal match. Selecting an entire URL regardless of the
			// "."/"/" separators inside it is the smart-selection RULE engine's
			// job (see smartSelection.test.ts's identically-named case, which
			// uses a full `https://[^\s]+` pattern as a RULE, not a word alternate).
			const text = "cloning https://github now";
			const clickCol = text.indexOf("github") + 2;

			const withoutAlternate = createWordBoundaryResolver({ mode: "regex", separators: "", regexAlternates: "" });
			expect(withoutAlternate(row(text), clickCol)).toEqual({
				left: text.indexOf("github"),
				right: text.indexOf("github") + "github".length - 1,
			});

			const withAlternate = createWordBoundaryResolver({
				mode: "regex",
				separators: "",
				regexAlternates: "https://",
			});
			expect(withAlternate(row(text), clickCol)).toEqual({
				left: text.indexOf("https://"),
				right: text.indexOf("github") + "github".length - 1,
			});
		});

		it("with no alternates configured, falls back to a plain alnum/underscore word class", () => {
			const resolver = createWordBoundaryResolver({ mode: "regex", separators: "", regexAlternates: "" });
			expect(resolver(row("foo.bar_baz"), 1)).toEqual({ left: 0, right: 2 });
			expect(resolver(row("foo.bar_baz"), 5)).toEqual({ left: 4, right: 10 });
		});

		it("returns null when the click lands on a character outside any word/atom class", () => {
			const resolver = createWordBoundaryResolver({ mode: "regex", separators: "", regexAlternates: "" });
			expect(resolver(row("foo.bar"), 3)).toBeNull();
		});

		it("skips an invalid alternate instead of throwing", () => {
			const resolver = createWordBoundaryResolver({
				mode: "regex",
				separators: "",
				regexAlternates: "(unterminated|foo",
			});
			expect(() => resolver(row("foobar"), 0)).not.toThrow();
			expect(resolver(row("foobar"), 0)).toEqual({ left: 0, right: 5 });
		});

		it("multiple alternates all contribute to the word class", () => {
			const resolver = createWordBoundaryResolver({
				mode: "regex",
				separators: "",
				regexAlternates: "https://|-",
			});
			const text = "https://my-site.example.com";
			// Click inside "my" — the "https://" alternate joins the scheme onto
			// the host, and the "-" alternate joins "my" through to "site"; the
			// run still stops at "." (not covered by any alternate or the base
			// alnum/underscore word class).
			expect(resolver(row(text), text.indexOf("my") + 1)).toEqual({
				left: 0,
				right: text.indexOf(".") - 1,
			});
		});
	});
});
