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
export function resolveScopedBlock<T extends BlockRange>(blocks: readonly T[], viewportCenter: number): T | undefined {
	return blocks.find((b) => viewportCenter >= b.promptLine && (b.endLine == null || viewportCenter < b.endLine));
}

export function filterMatchesToBlock(
	matches: SearchMatch[],
	blocks: readonly BlockRange[],
	viewportCenter: number,
): SearchMatch[] {
	const block = resolveScopedBlock(blocks, viewportCenter);
	if (!block) return matches;
	const end = block.endLine;
	if (end == null) {
		return matches.filter((m) => m.row >= block.promptLine);
	}
	return matches.filter((m) => m.row >= block.promptLine && m.row < end);
}
