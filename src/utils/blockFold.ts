/** The subset of `CommandBlock` the fold geometry needs — kept minimal so
 *  this module doesn't depend on the stores/terminals import graph. */
export interface FoldableBlock {
	promptLine: number;
	executionLine: number | null;
	endLine: number | null;
}

/**
 * Absolute [start, count) of a folded block's collapsible output, exclusive
 * of the header row itself (`executionLine ?? promptLine`). Returns `null`
 * when the block isn't foldable yet (`endLine` still open) or has nothing to
 * fold (`endLine <= foldStart` — e.g. the degenerate `endLine === promptLine`
 * shape a wrong `⏺`-heuristic endLine used to produce, before it was fixed
 * to always carry an exclusive bound past the header).
 */
export function foldRange(block: FoldableBlock): { foldStart: number; foldedCount: number } | null {
	if (block.endLine == null) return null;
	const foldStart = (block.executionLine ?? block.promptLine) + 1;
	const foldedCount = block.endLine - foldStart;
	if (foldedCount <= 0) return null;
	return { foldStart, foldedCount };
}

/**
 * The block nearest the viewport's vertical center — used to resolve which
 * block `Cmd+Shift+.` folds when pressed with no more specific selection.
 * `halfViewportRows` is `screenRows >> 1`; a block "contains" viewTop if its
 * header is at or above the mid-viewport line and its (exclusive) end hasn't
 * passed viewTop yet.
 */
export function findBlockAtViewport<T extends { promptLine: number; endLine: number | null }>(
	blocks: readonly T[],
	viewTop: number,
	halfViewportRows: number,
): T | undefined {
	return blocks.find((b) => b.promptLine <= viewTop + halfViewportRows && (b.endLine ?? Infinity) >= viewTop);
}
