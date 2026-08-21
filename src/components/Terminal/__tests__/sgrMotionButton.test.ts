import { describe, expect, it } from "vitest";

import { sgrMotionButton } from "../canvasTerminalUtils";

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
