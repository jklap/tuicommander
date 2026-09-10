interface SearchMatch {
	row: number;
	col_start: number;
	col_end: number;
}

export interface BlockRange {
	promptLine: number;
	endLine: number | null;
}

/**
 * The block "Search in Block" resolves to for a given viewport center — the
 * same predicate `filterMatchesToBlock` filters matches against. Exported
 * separately (issue #4) so `CanvasTerminal.tsx` can paint a visible indicator
 * on the block search actually scoped to, instead of resolving it silently.
 */
export function resolveScopedBlock<T extends BlockRange>(
	blocks: readonly T[],
	viewportCenter: number,
	historyBase = 0,
): T | undefined {
	const line = viewportCenter + historyBase;
	return blocks.find((b) => line >= b.promptLine && (b.endLine == null || line < b.endLine));
}

export function filterMatchesToBlock(
	matches: SearchMatch[],
	blocks: readonly BlockRange[],
	viewportCenter: number,
	historyBase = 0,
): SearchMatch[] {
	const block = resolveScopedBlock(blocks, viewportCenter, historyBase);
	if (!block) return matches;
	const end = block.endLine;
	if (end == null) {
		return matches.filter((m) => m.row + historyBase >= block.promptLine);
	}
	return matches.filter((m) => m.row + historyBase >= block.promptLine && m.row + historyBase < end);
}
