/** A snapshot of an xterm buffer row — just the parts the overlay cares about. */
export interface RowSnapshot {
	text: string;
	isWrapped: boolean;
}

/** Re-declared here instead of imported: keeps the helper self-contained and
 *  avoids pulling the full Terminal module into unit tests. Must stay in
 *  sync with the patterns used in Terminal.tsx. */
export const SUGGEST_ANCHOR_RE = /^[\s●⏺]*suggest:\s+\S/;
/** Stop condition for the continuation walk: a new `intent:` token at column 0. */
const INTENT_RE = /^intent:\s+\S/;
/**
 * Which rows get the intent highlight. Deliberately looser than [`INTENT_RE`]:
 * an agent renders its own intent line behind a bullet (`⏺ intent: …`), and that
 * row must still be tinted. It is NOT a walk stop condition — ending a suggest
 * block on an indented mention would swallow the block's closing bracket.
 */
export const INTENT_HIGHLIGHT_RE = /^[\s●⏺]*intent:\s+/;
/**
 * The answer marker an agent prefixes to every sentence that directly answers
 * the user (💬). Matched on row text, which carries no escape sequences — bold,
 * colour and the like live in cell attributes — so styling cannot hide it.
 * Like `intent:`, it may sit behind the agent's own bullet.
 */
export const ANSWER_MARKER_RE = /^[\s●⏺]*💬/;
/** Match a NEW `suggest:` anchor for stop-detection during a continuation
 *  walk. Does NOT require `|` on the same row — the Rust parser allows the
 *  first `|` to arrive on a wrapped continuation line, so a row like
 *  `suggest: long item that wraps...` (with the pipe on the next row) is
 *  still a new block boundary and the walk MUST stop here. (#1380-3b9c) */
const SUGGEST_STOP_RE = /^[\t ]*(?:[●⏺][\t ]+)?suggest:\s+\S/;

/**
 * Given a suggest anchor row at `anchorIndex`, return the 0-based indexes of
 * subsequent rows that should be visually hidden as continuations of the same
 * `suggest: [ … ]` block.
 *
 * The bracket pair bounds the token: the closing `]` is a hard terminator, so
 * the walk simply hides every row after the anchor up to and including the row
 * that carries `]`. A single-line suggest closes on the anchor itself → nothing
 * extra to hide. A new `suggest:`/`intent:` token before the `]` stops the walk
 * defensively. Because the `]` bounds the block, stray pipe rows (Makefile /
 * mermaid / tables) can never be swallowed.
 */
export function continuationRowsAfterSuggest(
	anchorIndex: number,
	totalRows: number,
	getRow: (i: number) => RowSnapshot | null,
): number[] {
	const anchor = getRow(anchorIndex);
	// Single-line bracketed suggest closes on the anchor row.
	if (!anchor || anchor.text.includes("]")) return [];
	const hidden: number[] = [];
	for (let i = anchorIndex + 1; i < totalRows; i++) {
		const row = getRow(i);
		if (!row) break;
		// A new token begins a different block — stop before it.
		if (SUGGEST_STOP_RE.test(row.text) || INTENT_RE.test(row.text)) break;
		hidden.push(i);
		// The closing `]` ends the bracketed token — hide it, then stop.
		if (row.text.includes("]")) break;
	}
	return hidden;
}

/**
 * Determine whether the row at `anchorIndex` is the start of a `suggest: [ … ]`
 * block — i.e. one the Rust parser would accept and render as chips.
 *
 * Requires the bracketed form: a `suggest:` anchor at column 0 that opens a `[`
 * and contains a `|` separator. When the terminal is wide enough both land on
 * the anchor row; on narrow terminals the first `|` may wrap onto a continuation
 * row, so wrapped rows are checked too.
 */
export function isSuggestBlock(
	anchorIndex: number,
	totalRows: number,
	getRow: (i: number) => RowSnapshot | null,
): boolean {
	const row = getRow(anchorIndex);
	if (!row) return false;

	// Must look like a bracketed suggest anchor at column 0.
	if (!SUGGEST_ANCHOR_RE.test(row.text) || !row.text.includes("[")) return false;

	// Fast path: pipe on the same line — classic case.
	if (row.text.includes("|")) return true;

	// Otherwise the first `|` may have wrapped onto a continuation row.
	for (let i = anchorIndex + 1; i < totalRows; i++) {
		const next = getRow(i);
		if (!next?.isWrapped) break;
		if (next.text.includes("|")) return true;
	}

	return false;
}

/** One row the overlay masks, and why. */
export interface OverlayBlock {
	row: number;
	kind: "suggest" | "continuation" | "intent" | "answer" | "collapsed";
}

/** The answers-only view: which rows of the screen belong to the turn to collapse. */
export interface AnswersOnlyScope {
	/** First row of the turn (inclusive). */
	startRow: number;
	/** End of the turn (exclusive) — the row of the live cursor, so the prompt stays visible. */
	endRow: number;
}

/**
 * Screen row where the last turn begins: the row after the last user prompt on
 * screen, or 0 when the prompt has scrolled off the top (or none is known).
 *
 * `promptLines` are grid-relative lines (history included); the screen shows
 * lines `historySize - displayOffset` onwards.
 */
export function lastTurnStartRow(
	promptLines: readonly number[],
	historySize: number,
	displayOffset: number,
	totalRows: number,
): number {
	const firstVisibleLine = historySize - displayOffset;
	let start = 0;
	for (const line of promptLines) {
		const row = line - firstVisibleLine;
		if (row >= 0 && row < totalRows) start = Math.max(start, row + 1);
	}
	return start;
}

/**
 * Which rows the suggest/intent overlay must mask on this screen, plus a key
 * that changes exactly when that set does.
 *
 * The key is the point: the overlay rebuilds its DOM only when the plan differs
 * from the last one, and most repaints do not move a suggest block. Deciding
 * that from freshly built `<div>`s meant creating and dropping the whole overlay
 * on every frame to discover it was unchanged, so the plan is computed first and
 * the elements are built only once the key says they are needed.
 */
export function planSuggestOverlay(
	totalRows: number,
	getRow: (i: number) => RowSnapshot | null,
	answersOnly?: AnswersOnlyScope,
): { key: string; blocks: OverlayBlock[] } {
	const blocks: OverlayBlock[] = [];
	const parts: string[] = [];
	for (let row = 0; row < totalRows; row++) {
		const snapshot = getRow(row);
		if (!snapshot) continue;
		const text = snapshot.text;

		if (SUGGEST_ANCHOR_RE.test(text) && isSuggestBlock(row, totalRows, getRow)) {
			blocks.push({ row, kind: "suggest" });
			parts.push(`s${row}`);
			const hiddenRows = continuationRowsAfterSuggest(row, totalRows, getRow);
			for (const contRow of hiddenRows) {
				blocks.push({ row: contRow, kind: "continuation" });
				parts.push(`c${contRow}`);
			}
			if (hiddenRows.length > 0) row = hiddenRows[hiddenRows.length - 1];
		} else if (!snapshot.isWrapped && ANSWER_MARKER_RE.test(text)) {
			// The marker line and every row it wraps onto are one answer. A wrapped
			// row is never a line start, so an emoji the wrap happens to land on
			// is not a marker.
			blocks.push({ row, kind: "answer" });
			parts.push(`a${row}`);
			while (getRow(row + 1)?.isWrapped) {
				row++;
				blocks.push({ row, kind: "answer" });
				parts.push(`a${row}`);
			}
		} else if (answersOnly && row >= answersOnly.startRow && row < answersOnly.endRow && text.trim() !== "") {
			blocks.push({ row, kind: "collapsed" });
			parts.push(`x${row}`);
		} else if (INTENT_HIGHLIGHT_RE.test(text)) {
			blocks.push({ row, kind: "intent" });
			parts.push(`i${row}`);
		}
	}
	return { key: parts.join(","), blocks };
}

function overlayDiv(top: number, height: number, background: string): HTMLDivElement {
	const div = document.createElement("div");
	div.style.cssText = `position:absolute;left:0;right:0;top:${top}px;height:${height}px;background:${background}`;
	return div;
}

/**
 * Replace the contents of `container` with one absolutely positioned strip per
 * planned block. Masks (`suggest`, `continuation`, `collapsed`) paint the terminal
 * background over the row; `intent` and `answer` are translucent tints, so the
 * row's text stays readable underneath. An answer also gets a solid gutter bar.
 */
export function paintOverlayBlocks(
	container: HTMLElement,
	blocks: readonly OverlayBlock[],
	cellHeight: number,
	bg: string,
): void {
	container.textContent = "";
	for (const block of blocks) {
		const top = block.row * cellHeight;
		if (block.kind === "answer") {
			const div = overlayDiv(top, cellHeight, "rgba(94,190,140,0.14)");
			div.style.boxShadow = "inset 3px 0 0 rgba(94,190,140,0.9)";
			container.appendChild(div);
		} else {
			container.appendChild(overlayDiv(top, cellHeight, block.kind === "intent" ? "rgba(181,147,90,0.12)" : bg));
		}
	}
}
