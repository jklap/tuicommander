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
 * block `Cmd+Shift+.` folds when pressed with no more specific selection, and
 * (at `halfViewportRows=0`) which block a gutter click targets. `halfViewportRows`
 * is `screenRows >> 1`; a block "contains" viewTop if its header is at or above
 * the mid-viewport line and its end is exclusive of viewTop — i.e. `endLine`
 * itself belongs to the NEXT block, not this one.
 *
 * The end bound must be exclusive: a zsh precmd cycle emits OSC 133 D (close)
 * then A/B (open) on the same buffer line, so a block's `endLine` is numerically
 * equal to the next block's `promptLine` (see `terminals.ts`'s `handleOsc133`
 * and `shell_integration.rs`'s doc comment). An inclusive `>=` end bound made
 * both blocks match at that shared boundary row, and `.find()` (blocks stored
 * oldest-first) returned the OLDER block — the gutter-click bug where clicking a
 * block's gutter copied the PREVIOUS block's output instead.
 */
export function findBlockAtViewport<T extends { promptLine: number; endLine: number | null }>(
	blocks: readonly T[],
	viewTop: number,
	halfViewportRows: number,
): T | undefined {
	return blocks.find((b) => b.promptLine <= viewTop + halfViewportRows && (b.endLine ?? Infinity) > viewTop);
}
