/**
 * Visibility lifecycle rules for `Terminal.tsx`, kept out of the component so
 * they can be tested — mounting the component itself needs the whole
 * canvas/transport mock stack, and `Terminal.tsx` is excluded from coverage for
 * exactly that reason.
 */

/**
 * How many frames to wait for a zero-size container before giving up.
 * ~2 s at 60 fps: longer than any layout pass, and bounded.
 */
export const SIZE_RETRY_MAX_FRAMES = 120;

/**
 * Wait for a terminal container to get a real box, then run `onSized` once.
 *
 * A container can mount with a zero box — a tab that becomes visible one frame
 * before layout runs, a pane mid-resize — so a retry is needed. It must be
 * bounded and cancellable: a pane the user leaves collapsed never gets a size,
 * and an unbounded loop re-arms a frame every ~16 ms for the lifetime of the
 * page, once per terminal, with no handle to cancel when the component goes
 * away.
 *
 * Returns a disposer. After it runs, neither callback can fire again.
 */
export function retryUntilSized(
	isSized: () => boolean,
	onSized: () => void,
	onExhausted?: () => void,
	maxFrames: number = SIZE_RETRY_MAX_FRAMES,
): () => void {
	let handle = 0;
	let framesLeft = maxFrames;
	let disposed = false;

	const tick = () => {
		handle = 0;
		if (disposed) return;
		if (isSized()) {
			onSized();
			return;
		}
		if (--framesLeft <= 0) {
			onExhausted?.();
			return;
		}
		handle = requestAnimationFrame(tick);
	};

	handle = requestAnimationFrame(tick);

	return () => {
		disposed = true;
		if (handle) cancelAnimationFrame(handle);
		handle = 0;
	};
}

/** What the visibility effect carries between runs. */
export interface ReattachPhase {
	/** Was the terminal visible on the previous run? */
	visible: boolean;
	/** Has a detach been seen since the terminal was last visible? */
	sawDetach: boolean;
}

export const REATTACH_PHASE_INITIAL: ReattachPhase = { visible: false, sawDetach: false };

/**
 * True only for a real reattach: the tab was detached into a floating window —
 * whose own Terminal took over the session's grid channel — and is visible here
 * again.
 *
 * A plain tab switch also flips visibility false → true, but nothing touched
 * this terminal's subscription while it was hidden. Resubscribing there bought
 * nothing and cost a visible paint → wipe → paint, because `refresh()` drops the
 * current frame before the replacement arrives.
 */
export function needsGridResubscribe(prev: ReattachPhase, visible: boolean, detached: boolean): boolean {
	return visible && !detached && !prev.visible && prev.sawDetach;
}

/**
 * Advance the phase and say whether this run is a reattach.
 *
 * `sawDetach` is a latch, not an edge on the previous run: `reattach()` and the
 * tab selection that follows it are two separate store writes, so the effect can
 * run once with the detach already cleared and the tab not yet active. An edge
 * test would lose the reattach in that gap.
 */
export function stepReattachPhase(
	prev: ReattachPhase,
	visible: boolean,
	detached: boolean,
): { phase: ReattachPhase; resubscribe: boolean } {
	if (detached) return { phase: { visible: false, sawDetach: true }, resubscribe: false };
	if (!visible) return { phase: { visible: false, sawDetach: prev.sawDetach }, resubscribe: false };
	return { phase: { visible: true, sawDetach: false }, resubscribe: needsGridResubscribe(prev, visible, detached) };
}
