import type { DecodedRow } from "./canvasTerminalUtils";

export interface SelectionPoint {
	col: number;
	row: number;
}

/** What a drag extends by: plain cells, whole words (started by a double-click),
 *  or whole lines (started by a triple-click). */
export type SelectionMode = "char" | "word" | "line";

/** Separator characters for word-boundary scans — mirrors the double-click word
 *  predicate that has always lived inline in CanvasTerminal's mousedown handler;
 *  pulled out here so word-mode drag extension can reuse the identical rule. */
const WORD_SEPARATOR_RE = /[\s\t\x00-\x1f\x7f "'`(){}[\]<>|;:,.!?@#$%^&*~=+/\\]/;

function isWordCodepoint(cp: number): boolean {
	if (cp === 0 || cp === 32) return false;
	return !WORD_SEPARATOR_RE.test(String.fromCodePoint(cp));
}

/**
 * Word boundaries at `col` on `row`, or null when `col` isn't on a word
 * character (matches a click landing on whitespace/punctuation: falls back to
 * a bare caret rather than a bogus zero-width "word").
 */
export function wordBoundsAt(row: DecodedRow, col: number): { left: number; right: number } | null {
	if (col < 0 || col >= row.count) return null;
	if (!isWordCodepoint(row.codepoints[col])) return null;
	let left = col;
	let right = col;
	while (left > 0 && isWordCodepoint(row.codepoints[left - 1])) left--;
	while (right < row.count - 1 && isWordCodepoint(row.codepoints[right + 1])) right++;
	return { left, right };
}

/** Anchor captured at mousedown for a word/line-mode drag — the edge of the original
 *  double/triple-click that must stay included no matter which way the drag goes. */
export interface DragAnchor {
	wordAnchor: { row: number; left: number; right: number } | null;
	lineAnchorRow: number | null;
}

/** Live drag position, re-derived each frame by the caller (grid position, the word
 *  bounds under it if any, and the last selectable column for line mode). */
export interface DragPosition {
	row: number;
	col: number;
	bounds: { left: number; right: number } | null;
	maxCol: number;
}

/**
 * Re-derives the selection start/end for a drag frame, given the mode set at mousedown
 * and its anchor. Word/line mode union the live drag boundary with whichever edge of the
 * anchor sits away from the drag direction, so the original double/triple-clicked
 * word/line stays fully included regardless of drag direction — matching double-click-drag
 * / triple-click-drag in every mainstream terminal. Falls back to plain cell-wise extension
 * for "char" mode, or when the relevant anchor is missing (e.g. a double-click that landed
 * on whitespace/punctuation never sets `wordAnchor`) — in which case `start` is left as
 * whatever was set at mousedown and only `end` moves, exactly like a plain drag.
 */
export function extendSelectionDrag(
	mode: SelectionMode,
	anchor: DragAnchor,
	drag: DragPosition,
	currentStart: SelectionPoint,
): { start: SelectionPoint; end: SelectionPoint } {
	const { row: absRow, col } = drag;
	if (mode === "word" && anchor.wordAnchor) {
		const { wordAnchor } = anchor;
		const dragLeft = drag.bounds?.left ?? col;
		const dragRight = drag.bounds?.right ?? col;
		const draggingForward = absRow > wordAnchor.row || (absRow === wordAnchor.row && dragLeft >= wordAnchor.left);
		if (draggingForward) {
			return { start: { row: wordAnchor.row, col: wordAnchor.left }, end: { row: absRow, col: dragRight } };
		}
		return { start: { row: absRow, col: dragLeft }, end: { row: wordAnchor.row, col: wordAnchor.right } };
	}
	if (mode === "line" && anchor.lineAnchorRow !== null) {
		const lineAnchorRow = anchor.lineAnchorRow;
		if (absRow >= lineAnchorRow) {
			return { start: { row: lineAnchorRow, col: 0 }, end: { row: absRow, col: drag.maxCol } };
		}
		return { start: { row: absRow, col: 0 }, end: { row: lineAnchorRow, col: drag.maxCol } };
	}
	return { start: currentStart, end: { row: absRow, col } };
}

export interface SearchMatch {
	row: number;
	col_start: number;
	col_end: number;
}

export interface SearchViewport {
	historySize: number;
	displayOffset: number;
	screenRows: number;
}

export interface CanvasSelectionController {
	selecting: boolean;
	start: SelectionPoint | null;
	end: SelectionPoint | null;
	cachedText: string;
	/** Granularity a drag extends by. Set at mousedown (char/word/line for a
	 *  single/double/triple click); consulted only by the drag-extend path. */
	mode: SelectionMode;
	clear: () => void;
	hasRange: () => boolean;
	spansOffscreen: (toViewportRow: (absoluteRow: number) => number | null) => boolean;
	getLocalText: (getRow: (absoluteRow: number) => DecodedRow | null) => string;
}

export interface CanvasSearchController {
	readonly matches: SearchMatch[];
	readonly activeIndex: number;
	replace: (matches: SearchMatch[], viewport?: SearchViewport) => SearchMatch | null;
	clear: () => void;
	dropRows: (rows: Set<number>) => boolean;
	next: () => SearchMatch | null;
	previous: () => SearchMatch | null;
}

export function createCanvasSelectionController(): CanvasSelectionController {
	let selecting = false;
	let start: SelectionPoint | null = null;
	let end: SelectionPoint | null = null;
	let cachedText = "";
	let mode: SelectionMode = "char";

	return {
		get selecting() {
			return selecting;
		},
		set selecting(value) {
			selecting = value;
		},
		get start() {
			return start;
		},
		set start(value) {
			start = value;
		},
		get end() {
			return end;
		},
		set end(value) {
			end = value;
		},
		get cachedText() {
			return cachedText;
		},
		set cachedText(value) {
			cachedText = value;
		},
		get mode() {
			return mode;
		},
		set mode(value) {
			mode = value;
		},
		clear() {
			selecting = false;
			start = null;
			end = null;
			cachedText = "";
			mode = "char";
		},
		hasRange() {
			return Boolean(start && end && (start.row !== end.row || start.col !== end.col));
		},
		spansOffscreen(toViewportRow) {
			if (!start || !end) return false;
			const firstRow = Math.min(start.row, end.row);
			const lastRow = Math.max(start.row, end.row);
			for (let row = firstRow; row <= lastRow; row++) {
				if (toViewportRow(row) === null) return true;
			}
			return false;
		},
		getLocalText(getRow) {
			if (!start || !end) return "";
			const firstRow = Math.min(start.row, end.row);
			const lastRow = Math.max(start.row, end.row);
			const forward = start.row <= end.row;
			const lines: string[] = [];

			for (let rowIndex = firstRow; rowIndex <= lastRow; rowIndex++) {
				const row = getRow(rowIndex);
				if (!row) {
					lines.push("");
					continue;
				}

				let startCol = 0;
				let endCol = row.count - 1;
				if (firstRow === lastRow) {
					startCol = Math.min(start.col, end.col);
					endCol = Math.max(start.col, end.col);
				} else if (rowIndex === firstRow) {
					startCol = forward ? start.col : end.col;
				} else if (rowIndex === lastRow) {
					endCol = forward ? end.col : start.col;
				}

				let text = "";
				for (let col = startCol; col <= endCol; col++) {
					const codepoint = row.codepoints[col];
					text += codepoint === 0 ? " " : String.fromCodePoint(codepoint);
				}
				lines.push(text.replace(/\s+$/, ""));
			}

			while (lines.length > 0 && lines[lines.length - 1] === "") lines.pop();
			return lines.join("\n");
		},
	};
}

export function createCanvasSearchController(): CanvasSearchController {
	let matches: SearchMatch[] = [];
	let activeIndex = -1;

	const active = () => (activeIndex >= 0 ? (matches[activeIndex] ?? null) : null);

	return {
		get matches() {
			return matches;
		},
		get activeIndex() {
			return activeIndex;
		},
		replace(nextMatches, viewport) {
			matches = nextMatches;
			activeIndex = matches.length > 0 ? nearestVisibleMatch(matches, viewport) : -1;
			return active();
		},
		clear() {
			matches = [];
			activeIndex = -1;
		},
		dropRows(rows) {
			if (matches.length === 0 || rows.size === 0) return false;
			const previouslyActive = active();
			const kept = matches.filter((match) => !rows.has(match.row));
			if (kept.length === matches.length) return false;
			matches = kept;
			// Keep the cursor on the same match when it survived, so a re-render
			// mid-search doesn't teleport the user to a different hit.
			const survivingIndex = previouslyActive ? kept.indexOf(previouslyActive) : -1;
			activeIndex = survivingIndex >= 0 ? survivingIndex : kept.length > 0 ? 0 : -1;
			return true;
		},
		next() {
			if (matches.length === 0) return null;
			activeIndex = (activeIndex + 1) % matches.length;
			return active();
		},
		previous() {
			if (matches.length === 0) return null;
			activeIndex = (activeIndex - 1 + matches.length) % matches.length;
			return active();
		},
	};
}

function nearestVisibleMatch(matches: SearchMatch[], viewport?: SearchViewport): number {
	if (!viewport) return 0;
	const viewportTop = viewport.historySize - viewport.displayOffset;
	const viewportBottom = viewportTop + viewport.screenRows;
	for (let index = matches.length - 1; index >= 0; index--) {
		const row = matches[index].row;
		if (row >= viewportTop && row < viewportBottom) return index;
	}
	return 0;
}
