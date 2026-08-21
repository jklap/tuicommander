import { describe, expect, it } from "vitest";
import {
	createCanvasScrollController,
	GESTURE_ACCEL_MAX,
	GESTURE_ACCEL_MIN,
	gestureAccelFactor,
} from "../canvasTerminalScroll";

describe("canvas terminal scroll controller", () => {
	it("tracks fractional gestures and clamps their pending backend offset", () => {
		const scroll = createCanvasScrollController();
		expect(scroll.applyDelta(-2.5, 0, 10)).toBe(2.5);
		expect(scroll.pendingOffset).toBe(2);
		expect(scroll.applyDelta(-20, 0, 10)).toBe(10);
		expect(scroll.pendingOffset).toBe(10);
		expect(scroll.scrolling).toBe(true);
	});

	it("snaps and hands off only when the matching backend frame arrives", () => {
		const scroll = createCanvasScrollController();
		scroll.applyDelta(-3.6, 0, 20);
		expect(scroll.snap()).toBe(4);
		expect(scroll.acceptSettledFrame(3)).toBe(false);
		expect(scroll.position).toBe(4);
		expect(scroll.acceptSettledFrame(4)).toBe(true);
		expect(scroll.position).toBeNull();
		expect(scroll.settleTarget).toBeNull();
	});

	it("owns cache state and cancels every transient field", () => {
		const scroll = createCanvasScrollController();
		scroll.requestedChunks.add(2);
		scroll.position = 1.5;
		scroll.pendingOffset = 1;
		scroll.scrolling = true;
		scroll.settleTarget = 2;
		scroll.gestureDistancePx = 42;
		scroll.clearCache();
		expect(scroll.requestedChunks.size).toBe(0);

		scroll.cancel();
		expect(scroll.position).toBeNull();
		expect(scroll.pendingOffset).toBeNull();
		expect(scroll.scrolling).toBe(false);
		expect(scroll.settleTarget).toBeNull();
		expect(scroll.gestureDistancePx).toBe(0);
	});

	it("accumulates same-signed gesture distance and restarts the ramp on reversal", () => {
		const scroll = createCanvasScrollController();
		expect(scroll.accumulateGesture(10)).toBe(10);
		expect(scroll.accumulateGesture(5)).toBe(15);
		// Reversal restarts at |dy| instead of adding to the outbound distance.
		expect(scroll.accumulateGesture(-4)).toBe(4);
		expect(scroll.accumulateGesture(-6)).toBe(10);
	});

	it("snap and cancel both reset the gesture sign, not just the distance", () => {
		const scroll = createCanvasScrollController();
		scroll.accumulateGesture(10);
		scroll.snap();
		// If sign weren't reset, this same-direction call would still read as a
		// reversal relative to stale state and incorrectly restart the ramp.
		expect(scroll.accumulateGesture(5)).toBe(5);

		scroll.accumulateGesture(10);
		scroll.cancel();
		expect(scroll.accumulateGesture(5)).toBe(5);
	});

	it("invalidates delayed cache responses when the screen generation changes", () => {
		const scroll = createCanvasScrollController();
		const requestGeneration = scroll.cacheGeneration;
		expect(requestGeneration).toBeTypeOf("number");

		scroll.rowCache.set(7, {} as never);
		scroll.requestedChunks.add(0);
		scroll.clearCache();

		expect(scroll.cacheGeneration).toBe(requestGeneration + 1);
		expect(scroll.isCacheGenerationCurrent(requestGeneration)).toBe(false);
		expect(scroll.rowCache.size).toBe(0);
		expect(scroll.requestedChunks.size).toBe(0);
	});
});

describe("gestureAccelFactor", () => {
	it("starts at the damped floor and reaches 1:1 tracking after two screens", () => {
		expect(gestureAccelFactor(0, 100)).toBe(GESTURE_ACCEL_MIN);
		expect(gestureAccelFactor(100, 100)).toBe(GESTURE_ACCEL_MIN);
		expect(gestureAccelFactor(200, 100)).toBe(1.0);
	});

	it("is capped at the ceiling no matter how long the gesture runs", () => {
		expect(gestureAccelFactor(2000, 100)).toBe(GESTURE_ACCEL_MAX);
	});

	it("falls back to the floor when there is no screen height to ramp against", () => {
		expect(gestureAccelFactor(500, 0)).toBe(GESTURE_ACCEL_MIN);
		expect(gestureAccelFactor(500, -10)).toBe(GESTURE_ACCEL_MIN);
	});
});
