/**
 * Imperative navigation handle exposed by `SessionDiffList` and `DiffFileList`
 * via their `ref` prop — lets a toolbar's `<`/`>` buttons and a "jump to this
 * turn/change" action move the list without either list needing to know
 * anything about the caller's own row semantics (a row is a file in one mode,
 * a step in another).
 */
export interface DiffListNavHandle {
	/** Scrolls so the row at `index` is visible. Out-of-range indices are
	 *  clamped by the caller, not here — the handle trusts its input. */
	scrollToIndex: (index: number, opts?: { align?: "start" | "center" | "end" | "auto" }) => void;
	/** The row index nearest the top of the current scroll position. Reactive —
	 *  safe to read inside a `createMemo`/JSX expression to drive a disabled
	 *  state, not just at click time. */
	currentIndex: () => number;
	/** Total row count — lets a caller with no row list of its own (e.g.
	 *  `DiffTab`, which doesn't own `BranchDiffScrollView`'s file list) still
	 *  disable a "next" button at the last row. Reactive, same as `currentIndex`. */
	rowCount: () => number;
	/** Row indices currently mounted in the virtualizer's viewport (plus
	 *  overscan) — lets a caller decide whether a live update to a specific
	 *  row should flash-apply in place (visible) or hold behind a "Refresh"
	 *  affordance (not currently rendered). Reactive, same as `currentIndex`. */
	visibleIndices: () => ReadonlySet<number>;
}
