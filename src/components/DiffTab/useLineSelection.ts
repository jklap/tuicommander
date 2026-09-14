import { type Accessor, createEffect, createSignal, onCleanup } from "solid-js";

/** One rendered `<tr>`'s position within the diff, cached to avoid a full
 *  DOM walk on every drag mousemove + selection restyle. */
interface RowInfo {
	hunkIdx: number;
	lineIdx: number;
	isChange: boolean;
}

export interface LineSelectionHandlers {
	onMouseDown: (e: MouseEvent) => void;
	onMouseMove: (e: MouseEvent) => void;
	onMouseUp: () => void;
}

export interface LineSelection {
	selectedLines: Accessor<Set<number>>;
	selectedHunkIdx: Accessor<number | null>;
	handlers: LineSelectionHandlers;
	/** Attach the rendered diff's content root — required before selection can work. */
	setContentRef: (el: HTMLElement | undefined) => void;
	/** Call when the rendered diff/mode changes — invalidates the row cache. */
	invalidate: () => void;
	clear: () => void;
}

export interface LineSelectionOptions {
	/** The current diff's hunks, in the same shape `extractHunks()` returns. */
	hunks: Accessor<string[]>;
	/** CSS class applied to a selected row (and removed from a deselected one). */
	selectedClass: string;
	/** A click inside an element matching this selector never starts a drag
	 *  (e.g. a hunk's own revert button). */
	ignoreSelector?: string;
}

/** Detect whether a `<tr>` is an addition, deletion, or neither.
 *  @git-diff-view marks additions with `data-line-new-num` only, deletions
 *  with `data-line-old-num` only, and context lines with both. */
function isChangeLine(row: Element): boolean {
	const hasNew = row.querySelector("[data-line-new-num]");
	const hasOld = row.querySelector("[data-line-old-num]");
	return !!hasNew !== !!hasOld;
}

/**
 * Drag-to-select-lines-within-a-hunk for a rendered unified/split diff, plus
 * the CSS restyle that highlights the current selection. Shared by `DiffTab`
 * (single-file diff) and `SessionDiffTab` (one instance per rendered card).
 */
export function createLineSelection(opts: LineSelectionOptions): LineSelection {
	const [selectedLines, setSelectedLines] = createSignal<Set<number>>(new Set<number>());
	const [selectedHunkIdx, setSelectedHunkIdx] = createSignal<number | null>(null);

	let contentRef: HTMLElement | undefined;
	let rowCache: { rows: HTMLTableRowElement[]; info: Map<HTMLTableRowElement, RowInfo> } | null = null;
	let isDragging = false;
	let dragAnchorLine = -1;
	let dragAnchorHunk = -1;

	function getRowCache() {
		if (rowCache) return rowCache;
		if (!contentRef) return null;
		const rows = Array.from(contentRef.querySelectorAll("tr")) as HTMLTableRowElement[];
		const info = new Map<HTMLTableRowElement, RowInfo>();
		let hunkIdx = -1;
		let lineCount = 0;
		for (const r of rows) {
			if (r.querySelector("[class*='diff-line-hunk']")) {
				hunkIdx++;
				lineCount = 0;
				info.set(r, { hunkIdx, lineIdx: -1, isChange: false });
			} else {
				info.set(r, { hunkIdx, lineIdx: lineCount, isChange: isChangeLine(r) });
				lineCount++;
			}
		}
		rowCache = { rows, info };
		return rowCache;
	}

	function findLineInfo(el: HTMLElement): { hunkIdx: number; lineIdx: number } | null {
		const row = el.closest("tr") as HTMLTableRowElement | null;
		if (!row) return null;
		const cache = getRowCache();
		const ri = cache?.info.get(row);
		if (!ri?.isChange || ri.hunkIdx < 0) return null;
		return { hunkIdx: ri.hunkIdx, lineIdx: ri.lineIdx };
	}

	function selectRange(hunkIdx: number, from: number, to: number) {
		const start = Math.min(from, to);
		const end = Math.max(from, to);
		const next = new Set<number>();
		const h = opts.hunks();
		if (hunkIdx < h.length) {
			const hunkLines = h[hunkIdx].split("\n");
			const bodyStart = hunkLines.findIndex((l) => l.startsWith("@@"));
			if (bodyStart >= 0) {
				const body = hunkLines.slice(bodyStart + 1);
				for (let i = start; i <= end; i++) {
					if (i < body.length && (body[i].startsWith("+") || body[i].startsWith("-"))) {
						next.add(i);
					}
				}
			}
		}
		setSelectedHunkIdx(hunkIdx);
		setSelectedLines(next);
	}

	function applyLineSelectionStyles() {
		const cache = getRowCache();
		if (!cache) return;
		const sel = selectedLines();
		const hIdx = selectedHunkIdx();
		for (const [row, ri] of cache.info) {
			if (ri.lineIdx < 0) continue; // hunk header row
			if (ri.hunkIdx === hIdx && sel.has(ri.lineIdx)) {
				row.classList.add(opts.selectedClass);
			} else {
				row.classList.remove(opts.selectedClass);
			}
		}
	}

	function onMouseDown(e: MouseEvent) {
		if (opts.ignoreSelector && (e.target as HTMLElement).closest(opts.ignoreSelector)) return;
		if (e.button !== 0) return;

		const info = findLineInfo(e.target as HTMLElement);
		if (!info) return;

		e.preventDefault();
		isDragging = true;
		dragAnchorLine = info.lineIdx;
		dragAnchorHunk = info.hunkIdx;

		if (selectedHunkIdx() !== null && selectedHunkIdx() !== info.hunkIdx) {
			setSelectedLines(new Set<number>());
		}
		setSelectedHunkIdx(info.hunkIdx);

		const next = new Set<number>();
		next.add(info.lineIdx);
		setSelectedLines(next);
		applyLineSelectionStyles();
	}

	function onMouseMove(e: MouseEvent) {
		if (!isDragging) return;
		const info = findLineInfo(e.target as HTMLElement);
		if (!info || info.hunkIdx !== dragAnchorHunk) return;
		selectRange(dragAnchorHunk, dragAnchorLine, info.lineIdx);
		applyLineSelectionStyles();
	}

	function onMouseUp() {
		isDragging = false;
	}

	const globalMouseUp = () => {
		isDragging = false;
	};
	document.addEventListener("mouseup", globalMouseUp);
	onCleanup(() => document.removeEventListener("mouseup", globalMouseUp));

	// Re-apply styles whenever the selection changes.
	createEffect(() => {
		selectedLines();
		selectedHunkIdx();
		const raf = requestAnimationFrame(() => applyLineSelectionStyles());
		onCleanup(() => cancelAnimationFrame(raf));
	});

	function clear() {
		setSelectedLines(new Set<number>());
		setSelectedHunkIdx(null);
		if (contentRef) {
			contentRef.querySelectorAll(`.${opts.selectedClass}`).forEach((el) => el.classList.remove(opts.selectedClass));
		}
	}

	function invalidate() {
		rowCache = null;
	}

	return {
		selectedLines,
		selectedHunkIdx,
		handlers: { onMouseDown, onMouseMove, onMouseUp },
		setContentRef: (el) => {
			contentRef = el;
		},
		invalidate,
		clear,
	};
}
