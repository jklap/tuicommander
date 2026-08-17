import { onCleanup } from "solid-js";

/**
 * `setInterval` that only ticks while the page is visible.
 *
 * A backgrounded PWA has nothing to render, so every tick is pure battery and
 * radio cost. When the page becomes visible again the callback fires at once,
 * so the UI is never stale by up to a full interval.
 *
 * Must be called inside a reactive root — the timer and the listener are
 * released via `onCleanup`.
 */
export function createVisibilityInterval(tick: () => void, intervalMs: number): void {
	let timer: ReturnType<typeof setInterval> | null = null;

	const start = () => {
		if (timer === null) timer = setInterval(tick, intervalMs);
	};
	const stop = () => {
		if (timer !== null) {
			clearInterval(timer);
			timer = null;
		}
	};

	const sync = () => {
		if (document.visibilityState === "hidden") {
			stop();
		} else if (timer === null) {
			// Catch up on whatever happened while hidden, then resume ticking.
			tick();
			start();
		}
	};

	if (document.visibilityState !== "hidden") start();
	document.addEventListener("visibilitychange", sync);

	onCleanup(() => {
		stop();
		document.removeEventListener("visibilitychange", sync);
	});
}
