import { hiddenMarkerRows, type OverlayBlock, planSuggestOverlay, promptEnd, type RowSnapshot } from "./suggestOverlay";

export interface MarkerPlan {
	hidden: ReadonlySet<number>;
	answerMarkers: ReadonlySet<number>;
	key: string;
	blocks: OverlayBlock[];
	/** Offscreen prompt whose missing prefix must be read before classifying this viewport. */
	pendingPrompt?: number;
	promptBoundary?: PromptBoundary;
}

export interface PromptBoundary {
	end: number;
	/** False when the read ended while still inside the prompt. */
	resolved: boolean;
}

/** Ordinary marker continuation lookback; prompt ownership is independent of this bound. */
export const MARKER_LOOKBACK_ROWS = 32;

export function markerPromptStart(promptRows: readonly number[], base = 0): number | undefined {
	let latest: number | undefined;
	for (const row of promptRows)
		if (row < base - MARKER_LOOKBACK_ROWS && (latest === undefined || row > latest)) latest = row;
	return latest;
}

/** Shared by live-frame and cached smooth-scroll painting; includes the bottom overscan row. */
export function planCanvasMarkers(
	rows: number,
	getRow: (i: number) => RowSnapshot | null,
	promptRows: readonly number[] = [],
	boundary?: PromptBoundary,
): MarkerPlan {
	let firstRow = -MARKER_LOOKBACK_ROWS;
	const start = markerPromptStart(promptRows);
	let promptBoundary: PromptBoundary | undefined;
	if (start !== undefined) {
		let end = boundary?.end;
		if (end === undefined) {
			let missing = !getRow(start);
			end = promptEnd(start, rows + 1, (i) => {
				const row = getRow(i);
				if (!row) missing = true;
				return row;
			});
			// A cache miss is not evidence that the prompt ended. Fetch its prefix
			// rather than guessing from the indentation of the visible marker.
			if (missing) return { hidden: new Set(), answerMarkers: new Set(), key: "", blocks: [], pendingPrompt: start };
		}
		promptBoundary = { end, resolved: boundary?.resolved ?? end < rows + 1 };
		firstRow = Math.max(firstRow, end);
	}
	const plan = planSuggestOverlay(rows + 1, getRow, firstRow);
	return {
		promptBoundary,
		hidden: hiddenMarkerRows(plan.blocks),
		answerMarkers: plan.answerMarkers,
		key: plan.key,
		blocks: plan.blocks.filter((block) => block.row < rows),
	};
}

/** Read only until output starts, not the rest of a potentially long-running turn. */
export async function readMarkerPromptBoundary(
	start: number,
	end: number,
	readRows: (start: number, end: number) => Promise<RowSnapshot[] | null>,
): Promise<PromptBoundary | null> {
	const rows: RowSnapshot[] = [];
	for (let from = start; from < end; from += 256) {
		// Include the previous row so the wire wrap flag at a chunk boundary is preserved.
		const overlap = from > start ? 1 : 0;
		const chunk = await readRows(from - overlap, Math.min(from + 256, end));
		if (!chunk) return null;
		rows.push(...chunk.slice(overlap));
		const next = promptEnd(0, rows.length, (i) => rows[i] ?? null);
		if (next < rows.length) return { end: start + next, resolved: true };
	}
	return { end, resolved: false };
}
