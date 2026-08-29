import { describe, expect, it } from "vitest";
import {
	type ClickCounterState,
	classifyClick,
	DOUBLE_CLICK_WINDOW_MS,
	decideMousedownSelection,
} from "../canvasTerminalGestures";

describe("classifyClick", () => {
	function freshState(): ClickCounterState {
		return { count: 0, lastClickTime: 0 };
	}

	it("classifies a first click as single", () => {
		const state = freshState();
		expect(classifyClick(1000, state).kind).toBe("single");
		expect(state.count).toBe(1);
	});

	it("classifies a click within the window as double", () => {
		const state = freshState();
		classifyClick(1000, state);
		const { kind } = classifyClick(1000 + DOUBLE_CLICK_WINDOW_MS - 1, state);
		expect(kind).toBe("double");
		expect(state.count).toBe(2);
	});

	it("classifies a third rapid click as triple", () => {
		const state = freshState();
		classifyClick(1000, state);
		classifyClick(1010, state);
		const { kind } = classifyClick(1020, state);
		expect(kind).toBe("triple");
		expect(state.count).toBe(3);
	});

	it("classifies a fourth rapid click as quad", () => {
		const state = freshState();
		classifyClick(1000, state);
		classifyClick(1010, state);
		classifyClick(1020, state);
		const { kind } = classifyClick(1030, state);
		expect(kind).toBe("quad");
		expect(state.count).toBe(4);
	});

	it("clamps a 5th+ rapid click at quad rather than accumulating", () => {
		const state = freshState();
		classifyClick(1000, state);
		classifyClick(1010, state);
		classifyClick(1020, state);
		classifyClick(1030, state);
		const { kind } = classifyClick(1040, state);
		expect(kind).toBe("quad");
		expect(state.count).toBe(4);
	});

	it("resets to single once the window has elapsed", () => {
		const state = freshState();
		classifyClick(1000, state);
		classifyClick(1010, state);
		const { kind } = classifyClick(1010 + DOUBLE_CLICK_WINDOW_MS, state);
		expect(kind).toBe("single");
		expect(state.count).toBe(1);
	});

	it("treats exactly the window boundary as elapsed (strict less-than)", () => {
		const state = freshState();
		classifyClick(1000, state);
		const { kind } = classifyClick(1000 + DOUBLE_CLICK_WINDOW_MS, state);
		expect(kind).toBe("single");
	});
});

describe("decideMousedownSelection", () => {
	const absPos = { col: 5, row: 12 };

	it("single click sets a caret with no end and char mode", () => {
		const result = decideMousedownSelection({ clickKind: "single", absPos, wordBounds: null, maxCol: 78 });
		expect(result).toEqual({ start: absPos, end: null, mode: "char", wordAnchor: null, lineAnchorRow: null });
	});

	it("double click with word bounds selects the word and sets a word anchor", () => {
		const result = decideMousedownSelection({
			clickKind: "double",
			absPos,
			wordBounds: { left: 4, right: 6 },
			maxCol: 78,
		});
		expect(result).toEqual({
			start: { col: 4, row: 12 },
			end: { col: 6, row: 12 },
			mode: "word",
			wordAnchor: { row: 12, left: 4, right: 6 },
			lineAnchorRow: null,
		});
	});

	it("double click with no word bounds (whitespace/punctuation) falls back to a caret", () => {
		const result = decideMousedownSelection({ clickKind: "double", absPos, wordBounds: null, maxCol: 78 });
		expect(result).toEqual({ start: absPos, end: absPos, mode: "char", wordAnchor: null, lineAnchorRow: null });
	});

	it("triple click selects the whole row up to maxCol and sets a line anchor", () => {
		const result = decideMousedownSelection({ clickKind: "triple", absPos, wordBounds: null, maxCol: 78 });
		expect(result).toEqual({
			start: { col: 0, row: 12 },
			end: { col: 78, row: 12 },
			mode: "line",
			wordAnchor: null,
			lineAnchorRow: 12,
		});
	});

	it("quad click (smart-selection fallback, no rule matched) selects the whole row same as triple", () => {
		const result = decideMousedownSelection({ clickKind: "quad", absPos, wordBounds: null, maxCol: 78 });
		expect(result).toEqual({
			start: { col: 0, row: 12 },
			end: { col: 78, row: 12 },
			mode: "line",
			wordAnchor: null,
			lineAnchorRow: 12,
		});
	});
});
