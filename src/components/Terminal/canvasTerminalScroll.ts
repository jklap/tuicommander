import type { DecodedRow } from "./canvasTerminalUtils";

/**
 * How many decoded rows the smooth-scroll cache may hold. At roughly 2.6 KB per
 * row in the base typed arrays (three `Uint32Array(cols)` plus a
 * `Uint8Array(cols)`) this is ~15 MB of scrollback available to paint locally.
 * Sparse cell extras and lazily cached UTF-16 layouts add content-dependent
 * memory beyond that base estimate.
 */
export const ROW_CACHE_MAX = 6000;

/**
 * Rows per prefetch chunk. Lives here because the cache and the "already asked
 * for it" set are keyed by it: eviction has to release the chunk ids it drops
 * rows from, or the band fetch never asks for them again.
 */
export const ROW_CACHE_CHUNK = 64;

export interface CanvasScrollController {
	readonly rowCache: Map<number, DecodedRow>;
	readonly requestedChunks: Set<number>;
	readonly cacheGeneration: number;
	position: number | null;
	pendingOffset: number | null;
	inFlight: boolean;
	scrolling: boolean;
	settleTarget: number | null;
	gestureDistancePx: number;
	clearCache: () => void;
	/**
	 * Write decoded rows into the cache, evicting the oldest ones past
	 * `ROW_CACHE_MAX`. The ONLY way to fill it: the key is an eviction-stable
	 * all-time row index, so a line scrolling in always takes a fresh slot and
	 * nothing is ever overwritten — an unbounded writer grows for the session's
	 * whole life.
	 */
	cacheRows: (rows: Iterable<{ abs: number; row: DecodedRow }>) => void;
	isCacheGenerationCurrent: (generation: number) => boolean;
	/** Counts `commitLiveRows` calls; a fetch compares it to the value it started with. */
	readonly liveEpoch: number;
	/**
	 * Rows `[fromAbs, toAbs)` were live screen rows and are history now, their
	 * content final. Whatever the cache or a fetch in flight holds for them was
	 * read while they were still being redrawn (#1264-89c8), so drop it and let
	 * the chunks that hold them be asked for again.
	 */
	commitLiveRows: (fromAbs: number, toAbs: number) => void;
	/**
	 * Cache a fetched chunk. When rows were committed while it was in flight, its
	 * rows from `liveFromAbs` up are stale and are discarded. Returns false then,
	 * so the caller releases the chunk for a new fetch.
	 */
	cacheFetchedRows: (
		rows: Array<{ abs: number; row: DecodedRow }>,
		startedAtEpoch: number,
		liveFromAbs: number,
	) => boolean;
	applyDelta: (deltaLines: number, currentOffset: number, historySize: number) => number;
	/**
	 * Output pushed `grownLines` into history. A scrolled-back grid raises its
	 * display offset by the same amount so the viewport keeps its lines; the
	 * gesture offsets must follow, or the next flush scrolls the backend forward
	 * over the new output and those lines are never shown (#1264-89c8). Returns
	 * true when the backend at `backendOffset` has to be sent the rebased offset.
	 */
	followHistory: (grownLines: number, historySize: number, backendOffset: number) => boolean;
	snap: () => number | null;
	acceptSettledFrame: (displayOffset: number) => boolean;
	cancel: () => void;
}

export function createCanvasScrollController(): CanvasScrollController {
	const rowCache = new Map<number, DecodedRow>();
	const requestedChunks = new Set<number>();
	let position: number | null = null;
	let pendingOffset: number | null = null;
	let inFlight = false;
	let scrolling = false;
	let settleTarget: number | null = null;
	let gestureDistancePx = 0;
	let cacheGeneration = 0;
	let liveEpoch = 0;

	function cacheRows(rows: Iterable<{ abs: number; row: DecodedRow }>) {
		for (const { abs, row } of rows) rowCache.set(abs, row);
		// Insertion order is Map order, so the oldest keys come first. Evicting
		// them — rather than clearing the whole cache — keeps the rows a gesture
		// is about to paint, and needs no generation bump: a dropped old row says
		// nothing about a chunk still in flight, and invalidating one during
		// steady output would discard a fetch that is still correct.
		if (rowCache.size <= ROW_CACHE_MAX) return;
		const excess = rowCache.size - ROW_CACHE_MAX;
		let dropped = 0;
		for (const key of rowCache.keys()) {
			rowCache.delete(key);
			// `requestedChunks` means "already asked for, never ask again": a
			// successful fetch leaves its id there forever. So an evicted row has
			// to release its chunk, or scrolling back to it paints blanks with no
			// way to refill them. A chunk that lost even one row is not paintable,
			// hence release on the first row dropped, not the last.
			requestedChunks.delete(Math.floor(key / ROW_CACHE_CHUNK));
			if (++dropped === excess) break;
		}
	}

	return {
		rowCache,
		requestedChunks,
		get cacheGeneration() {
			return cacheGeneration;
		},
		get position() {
			return position;
		},
		set position(value) {
			position = value;
		},
		get pendingOffset() {
			return pendingOffset;
		},
		set pendingOffset(value) {
			pendingOffset = value;
		},
		get inFlight() {
			return inFlight;
		},
		set inFlight(value) {
			inFlight = value;
		},
		get scrolling() {
			return scrolling;
		},
		set scrolling(value) {
			scrolling = value;
		},
		get settleTarget() {
			return settleTarget;
		},
		set settleTarget(value) {
			settleTarget = value;
		},
		get gestureDistancePx() {
			return gestureDistancePx;
		},
		set gestureDistancePx(value) {
			gestureDistancePx = value;
		},
		clearCache() {
			cacheGeneration++;
			rowCache.clear();
			requestedChunks.clear();
		},
		cacheRows,
		get liveEpoch() {
			return liveEpoch;
		},
		commitLiveRows(fromAbs, toAbs) {
			if (toAbs <= fromAbs) return;
			liveEpoch++;
			for (let abs = fromAbs; abs < toAbs; abs++) rowCache.delete(abs);
			for (
				let chunk = Math.floor(fromAbs / ROW_CACHE_CHUNK);
				chunk <= Math.floor((toAbs - 1) / ROW_CACHE_CHUNK);
				chunk++
			) {
				requestedChunks.delete(chunk);
			}
		},
		cacheFetchedRows(rows, startedAtEpoch, liveFromAbs) {
			const current = startedAtEpoch === liveEpoch;
			cacheRows(current ? rows : rows.filter(({ abs }) => abs < liveFromAbs));
			return current || !rows.some(({ abs }) => abs >= liveFromAbs);
		},
		isCacheGenerationCurrent(generation) {
			return generation === cacheGeneration;
		},
		applyDelta(deltaLines, currentOffset, historySize) {
			scrolling = true;
			const base = position ?? currentOffset;
			position = Math.max(0, Math.min(historySize, base - deltaLines));
			pendingOffset = Math.floor(position);
			return position;
		},
		followHistory(grownLines, historySize, backendOffset) {
			// At offset 0 the grid follows output instead of holding its lines.
			if (grownLines <= 0 || position === null || position <= 0) return false;
			position = Math.min(historySize, position + grownLines);
			if (settleTarget !== null) settleTarget = position;
			const target = Math.floor(position);
			if (pendingOffset === null && backendOffset === target) return false;
			pendingOffset = target;
			return true;
		},
		snap() {
			scrolling = false;
			gestureDistancePx = 0;
			if (position === null) return null;
			const target = Math.round(position);
			position = target;
			pendingOffset = target;
			settleTarget = target;
			return target;
		},
		acceptSettledFrame(displayOffset) {
			if (settleTarget === null || displayOffset !== settleTarget) return false;
			settleTarget = null;
			position = null;
			return true;
		},
		cancel() {
			position = null;
			pendingOffset = null;
			scrolling = false;
			settleTarget = null;
			gestureDistancePx = 0;
		},
	};
}
