import { describe, expect, it } from "vitest";

import { lastGridCol, motionReportButton, sgrMotionButton, shouldForwardMouseGesture } from "../canvasTerminalUtils";

// Regression coverage: a hardcoded "no button" (3) on every motion report
// under ?1003h made a click-drag indistinguishable from a bare hover to any
// app tracking its own drag-selection from motion reports (Claude Code does
// this) — the drag was reported, but with the button state lied about, so
// the app could never tell a drag from the mouse just moving around.
describe("sgrMotionButton", () => {
	it("reports 3 (no button) for a bare hover", () => {
		expect(sgrMotionButton(0)).toBe(3);
	});

	it("reports the actually-held button during a drag", () => {
		expect(sgrMotionButton(1)).toBe(0); // left
		expect(sgrMotionButton(4)).toBe(1); // middle
		expect(sgrMotionButton(2)).toBe(2); // right
	});

	it("prioritizes left, then middle, then right when multiple buttons are held", () => {
		expect(sgrMotionButton(1 | 2)).toBe(0);
		expect(sgrMotionButton(1 | 4)).toBe(0);
		expect(sgrMotionButton(4 | 2)).toBe(1);
	});
});

// Regression coverage for 26a3d449's local-selection latch: a gesture claimed as local at
// mousedown must stay local for its whole lifetime, even if the app flips its mouse-reporting
// mode mid-drag. Re-checking mode on every move/up event could otherwise silently re-forward
// mid-drag, or swallow the mouseup that would have run local teardown (stop autoscroll, clear
// selecting state).
describe("shouldForwardMouseGesture", () => {
	it("never forwards once a gesture is already claimed as a local selection, even at motion mode", () => {
		expect(shouldForwardMouseGesture({ selecting: true, mouseMode: 3, shiftKey: false })).toBe(false);
	});

	it("does not forward when the app has no mouse tracking enabled", () => {
		expect(shouldForwardMouseGesture({ selecting: false, mouseMode: 0, shiftKey: false })).toBe(false);
	});

	it("does not forward while Shift is held (TUICommander's own selection override)", () => {
		expect(shouldForwardMouseGesture({ selecting: false, mouseMode: 3, shiftKey: true })).toBe(false);
	});

	it("forwards when nothing claims the gesture locally and the app tracks the mouse", () => {
		expect(shouldForwardMouseGesture({ selecting: false, mouseMode: 2, shiftKey: false })).toBe(true);
	});
});

describe("motionReportButton", () => {
	it("mode 3 (any-motion) reports on a bare hover", () => {
		expect(motionReportButton(3, 0)).toBe(3);
	});

	it("mode 3 reports the held button during a drag", () => {
		expect(motionReportButton(3, 1)).toBe(0);
	});

	it("mode 2 (drag-tracking) emits nothing for a bare hover", () => {
		expect(motionReportButton(2, 0)).toBeNull();
	});

	it("mode 2 reports the held button once a button is down", () => {
		expect(motionReportButton(2, 1)).toBe(0);
	});

	it("mode 1 (click-only) never emits a motion report", () => {
		expect(motionReportButton(1, 1)).toBeNull();
	});
});

describe("lastGridCol", () => {
	it("subtracts the gutter before dividing by cell width, matching canvasToGrid's maxCol", () => {
		// GUTTER_PX is 6; (800 - 6) / 8 = 99.25 -> floor 99, then -1 for zero-based last column.
		expect(lastGridCol(800, 8)).toBe(98);
	});

	it("never goes negative for a narrower-than-one-cell width", () => {
		expect(lastGridCol(2, 8)).toBe(0);
	});
});
