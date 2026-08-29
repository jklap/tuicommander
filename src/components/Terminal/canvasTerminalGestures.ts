import type { SelectionMode, SelectionPoint } from "./canvasTerminalSelection";

/** How many rapid clicks within the window landed. "quad" always runs smart
 *  selection (see CanvasTerminal's mousedown handler) — a 5th+ click stays
 *  "quad", it doesn't accumulate into some higher mode. */
export type SelectionClickKind = "single" | "double" | "triple" | "quad";

export interface ClickCounterState {
	count: number;
	lastClickTime: number;
}

/** Milliseconds between clicks for them to count as the same click-count
 *  sequence — matches every mainstream terminal's double-click window. */
export const DOUBLE_CLICK_WINDOW_MS = 400;

/**
 * Classify a mousedown against the running click-count timer. Pulled out of
 * CanvasTerminal's mousedown handler 1:1 (previously inline `clickCount`/
 * `lastClickTime` closure variables) so the classification itself is
 * unit-testable without mounting the component.
 *
 * `state` is mutated in place (the caller owns the persistent counters across
 * gestures) and also returned for convenience.
 */
export function classifyClick(
	now: number,
	state: ClickCounterState,
	windowMs: number = DOUBLE_CLICK_WINDOW_MS,
): { state: ClickCounterState; kind: SelectionClickKind } {
	const withinWindow = now - state.lastClickTime < windowMs;
	// Clamp at 4: a 5th+ rapid click must keep re-running quad-click's smart
	// selection, not silently drift into an undefined 5th state.
	state.count = withinWindow ? Math.min(state.count + 1, 4) : 1;
	state.lastClickTime = now;
	const kind: SelectionClickKind =
		state.count === 1 ? "single" : state.count === 2 ? "double" : state.count === 3 ? "triple" : "quad";
	return { state, kind };
}

/** Anchor captured at mousedown for a word/line-mode drag (see
 *  `extendSelectionDrag` in canvasTerminalSelection.ts, which consumes this). */
export interface MousedownSelectionAnchors {
	wordAnchor: { row: number; left: number; right: number } | null;
	lineAnchorRow: number | null;
}

export interface MousedownSelectionInput {
	clickKind: SelectionClickKind;
	/** Absolute grid position of the click (viewport row already resolved to absolute). */
	absPos: SelectionPoint;
	/** `wordBoundsAt(row, col)` at the click position — only consulted for "double". */
	wordBounds: { left: number; right: number } | null;
	/** Last selectable column on this row — only consulted for "triple". */
	maxCol: number;
}

export interface MousedownSelectionResult extends MousedownSelectionAnchors {
	start: SelectionPoint;
	end: SelectionPoint | null;
	mode: SelectionMode;
}

/**
 * Decide the new selection start/end/mode/anchors for a mousedown, given its
 * click-count classification. Pulled out of CanvasTerminal's mousedown
 * handler 1:1 — the shift-extend and gutter-click branches stay inline (they
 * return early before click counting even runs), only the
 * single/double/triple/quad decision moved here.
 *
 * This is the FALLBACK decision for "double" and "quad": the mousedown
 * handler tries smart-selection matching first for those two click kinds
 * (see `findSmartMatch` in `smartSelection.ts`) and only calls this when no
 * rule matched. "quad" falls back to the same whole-line selection as
 * "triple" rather than a distinct 4th selection mode.
 */
export function decideMousedownSelection(input: MousedownSelectionInput): MousedownSelectionResult {
	const { clickKind, absPos, wordBounds, maxCol } = input;

	if (clickKind === "double") {
		if (wordBounds) {
			return {
				start: { col: wordBounds.left, row: absPos.row },
				end: { col: wordBounds.right, row: absPos.row },
				mode: "word",
				wordAnchor: { row: absPos.row, left: wordBounds.left, right: wordBounds.right },
				lineAnchorRow: null,
			};
		}
		// Landed on whitespace/punctuation — nothing to hold fixed on drag, so
		// fall back to plain cell-wise extension for this gesture.
		return { start: absPos, end: absPos, mode: "char", wordAnchor: null, lineAnchorRow: null };
	}

	if (clickKind === "triple" || clickKind === "quad") {
		return {
			start: { col: 0, row: absPos.row },
			end: { col: maxCol, row: absPos.row },
			mode: "line",
			wordAnchor: null,
			lineAnchorRow: absPos.row,
		};
	}

	return { start: absPos, end: null, mode: "char", wordAnchor: null, lineAnchorRow: null };
}
