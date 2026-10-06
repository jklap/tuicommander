import { afterEach, beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import {
	createFrameStarvationWatchdog,
	FRAME_STARVATION_MAX_ATTEMPTS,
	FRAME_STARVATION_MS,
} from "../canvasTerminalUtils";

/**
 * A visible terminal whose grid channel has silently died receives no frames and
 * never finds out: Rust keeps sending into a callback the webview no longer has,
 * so the tab shows its cursor and gutter forever. The watchdog is the receiver's
 * only way to notice — it asks for a frame and rebuilds the subscription if
 * nothing at all comes back.
 */
describe("createFrameStarvationWatchdog", () => {
	let received: number;
	let onStarved: Mock<() => void | Promise<void>>;
	let onGiveUp: Mock<() => void>;

	function make(extra: { onStarved?: () => void | Promise<void> } = {}) {
		return createFrameStarvationWatchdog({
			getReceived: () => received,
			onStarved: extra.onStarved ?? onStarved,
			onGiveUp,
			timeoutMs: 1000,
			maxAttempts: 3,
		});
	}

	beforeEach(() => {
		vi.useFakeTimers();
		received = 0;
		onStarved = vi.fn<() => void | Promise<void>>();
		onGiveUp = vi.fn<() => void>();
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("does nothing before the timeout", () => {
		make().arm();
		vi.advanceTimersByTime(999);
		expect(onStarved).not.toHaveBeenCalled();
	});

	it("resubscribes when no frame arrived within the timeout", () => {
		make().arm();
		vi.advanceTimersByTime(1000);
		expect(onStarved).toHaveBeenCalledTimes(1);
	});

	it("leaves a healthy terminal alone when a frame arrives in time", () => {
		const w = make();
		w.arm();
		vi.advanceTimersByTime(500);
		received++;
		vi.advanceTimersByTime(500);

		expect(onStarved).not.toHaveBeenCalled();
		// ...and stops: no timer left to fire a false alarm later.
		vi.advanceTimersByTime(10_000);
		expect(onStarved).not.toHaveBeenCalled();
	});

	it("compares against the count at arm time, not zero", () => {
		received = 40; // a long-running terminal
		make().arm();
		vi.advanceTimersByTime(1000);
		expect(onStarved).toHaveBeenCalledTimes(1);
	});

	it("a second arm while waiting does not restart or double the timer", () => {
		const w = make();
		w.arm();
		vi.advanceTimersByTime(600);
		w.arm();
		vi.advanceTimersByTime(400);
		expect(onStarved).toHaveBeenCalledTimes(1);
	});

	it("keeps trying after a resubscribe that did not help, then gives up", () => {
		make().arm();
		vi.advanceTimersByTime(1000);
		vi.advanceTimersByTime(1000);
		vi.advanceTimersByTime(1000);
		expect(onStarved).toHaveBeenCalledTimes(3);
		expect(onGiveUp).not.toHaveBeenCalled();

		vi.advanceTimersByTime(1000);
		expect(onGiveUp).toHaveBeenCalledTimes(1);
		expect(onStarved).toHaveBeenCalledTimes(3);

		// Given up means no more timers — no resubscribe loop at timer rate.
		vi.advanceTimersByTime(60_000);
		expect(onStarved).toHaveBeenCalledTimes(3);
		expect(onGiveUp).toHaveBeenCalledTimes(1);
	});

	it("stops retrying as soon as a resubscribe works", () => {
		onStarved = vi.fn<() => void | Promise<void>>(() => {
			// The fresh subscription resets the counter, then delivers a frame.
			received = 0;
			setTimeout(() => received++, 100);
		});
		make().arm();
		vi.advanceTimersByTime(1000);
		expect(onStarved).toHaveBeenCalledTimes(1);

		vi.advanceTimersByTime(1000);
		expect(onStarved).toHaveBeenCalledTimes(1);
		expect(onGiveUp).not.toHaveBeenCalled();
	});

	it("re-reads the counter after onStarved, because resubscribing resets it to zero", () => {
		received = 7;
		onStarved = vi.fn<() => void | Promise<void>>(() => {
			received = 0; // framesReceived = 0 happens synchronously in resubscribe
		});
		make().arm();
		vi.advanceTimersByTime(1000);
		expect(onStarved).toHaveBeenCalledTimes(1);

		// One frame on the fresh subscription (1 > 0). A baseline captured BEFORE
		// onStarved ran would be 7, and 1 > 7 is false: a false starvation verdict
		// against a channel that just started working.
		received = 1;
		vi.advanceTimersByTime(1000);
		expect(onStarved).toHaveBeenCalledTimes(1);
	});

	it("cancel stops a pending check", () => {
		const w = make();
		w.arm();
		w.cancel();
		vi.advanceTimersByTime(10_000);
		expect(onStarved).not.toHaveBeenCalled();
	});

	it("cancel forgets spent attempts, so the next show gets a fresh budget", () => {
		const w = make();
		w.arm();
		vi.advanceTimersByTime(3000);
		expect(onStarved).toHaveBeenCalledTimes(3);
		w.cancel();

		w.arm();
		vi.advanceTimersByTime(1000);
		expect(onStarved).toHaveBeenCalledTimes(4);
		expect(onGiveUp).not.toHaveBeenCalled();
	});

	it("a rejected resubscribe does not escape as an unhandled rejection", async () => {
		const w = make({ onStarved: () => Promise.reject(new Error("ipc down")) });
		w.arm();
		await vi.advanceTimersByTimeAsync(1000);
		w.cancel();
		// Reaching here without vitest flagging an unhandled rejection is the assertion.
		expect(true).toBe(true);
	});

	it("exports a timeout that clears the backend's own recovery", () => {
		// 500 ms abandon + 1 s stuck pause (pty.rs MAX_IN_FLIGHT_MS / STUCK_PAUSE_MS).
		expect(FRAME_STARVATION_MS).toBeGreaterThan(500 + 1000);
		expect(FRAME_STARVATION_MAX_ATTEMPTS).toBeGreaterThanOrEqual(2);
	});
});
