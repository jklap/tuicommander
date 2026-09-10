/**
 * Regression coverage for the "hand cursor stuck over the prompt" bug: the
 * gutter-hover code added in 10bb1019 (issue #2) sets `canvasRef.style.cursor
 * = "pointer"` directly whenever the mouse is over the leftmost `GUTTER_PX`
 * strip, bypassing the `hoveredLink`-driven reset that used to be the only
 * thing touching cursor style. Two gaps that shipped with zero test coverage
 * (canvasTerminalGutter.test.ts only covers the pure `gutterZoneAt`/
 * `gutterMarkKind`/`canToggleFold` helpers, never the inline hit-test/cursor
 * code in CanvasTerminal.tsx itself):
 *
 *  1. Leaving the gutter for plain text with no link involved used to rely on
 *     the throttled/debounced link-hover check (100ms, gated behind
 *     `hoveredLink` being truthy) to reset the cursor back to "text" — a
 *     mouse that was never over an actual link never clears it.
 *  2. Once the PTY app enables xterm mouse reporting (`mouseMode > 0` — e.g.
 *     Claude Code CLI's own click-to-reposition-cursor feature),
 *     `shouldForwardMouseGesture` makes `onMouseMove` return before reaching
 *     ANY cursor-updating code at all. Whatever `style.cursor` happened to
 *     be at the moment mouse reporting turned on (very plausibly "pointer",
 *     since merely having drifted over the gutter at any earlier point sets
 *     it) is then frozen for the rest of that app's session — this is the
 *     concrete mechanism behind the reported "clicking a Claude Code prompt
 *     shows a hand cursor" regression.
 */

import { fireEvent } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GUTTER_PX } from "../canvasTerminalUtils";
import { buildTextFrame } from "./helpers/frameFixture";
import {
	createFakeTransport,
	FIXED_CELL_METRICS,
	mountCanvasTerminal,
	stubCanvasEnvironment,
} from "./helpers/mountCanvasTerminal";

const fakeTransport = vi.hoisted(() => ({ current: null as ReturnType<typeof createFakeTransport> | null }));

vi.mock("../canvasTerminalTransport", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../canvasTerminalTransport")>();
	return { ...actual, createTransport: () => fakeTransport.current! };
});

vi.mock("../glyphCache", () => ({
	getSharedMetrics: () => FIXED_CELL_METRICS,
	acquireCache: vi.fn(),
	releaseCache: vi.fn(),
	invalidateGlyphCache: vi.fn(),
}));

/** A point squarely inside the gutter strip (left of GUTTER_PX). */
function gutterPoint(row: number) {
	return { clientX: GUTTER_PX / 2, clientY: row * FIXED_CELL_METRICS.cellHeight + FIXED_CELL_METRICS.cellHeight / 2 };
}

/** Center-of-cell client coordinates for grid (col, row), past the gutter. */
function cellPoint(col: number, row: number) {
	return {
		clientX: GUTTER_PX + col * FIXED_CELL_METRICS.cellWidth + FIXED_CELL_METRICS.cellWidth / 2,
		clientY: row * FIXED_CELL_METRICS.cellHeight + FIXED_CELL_METRICS.cellHeight / 2,
	};
}

/** frameFlags bits 3-4 encode mouseMode (`(frameFlags >> 3) & 0x03`) — see
 *  canvasTerminalUtils.ts's decodeBinaryFrame and frameFixture.ts's docblock. */
function frameFlagsForMouseMode(mouseMode: 0 | 1 | 2 | 3): number {
	return mouseMode << 3;
}

describe("CanvasTerminal gutter-hover cursor", () => {
	beforeEach(() => {
		fakeTransport.current = createFakeTransport();
	});

	describe("with stubbed canvas environment", () => {
		let restoreEnv: () => void;

		beforeEach(() => {
			restoreEnv = stubCanvasEnvironment();
		});

		afterEach(() => {
			restoreEnv();
		});

		it("hovering the gutter shows a pointer cursor", async () => {
			const mounted = await mountCanvasTerminal({ sessionId: "s1", terminalId: "t1" });
			fakeTransport.current!.pushFrame(buildTextFrame(["hello world"], 40));

			fireEvent.mouseMove(document, gutterPoint(0));

			expect(mounted.canvas.style.cursor).toBe("pointer");
			await mounted.dispose();
		});

		it("moving from the gutter back onto plain text resets the cursor immediately, without waiting on the link-hover debounce", async () => {
			const mounted = await mountCanvasTerminal({ sessionId: "s2", terminalId: "t2" });
			fakeTransport.current!.pushFrame(buildTextFrame(["hello world"], 40));

			fireEvent.mouseMove(document, gutterPoint(0));
			expect(mounted.canvas.style.cursor).toBe("pointer");

			// No fake timers advanced, no waitFor — the reset must be synchronous
			// with this move, not dependent on the 100ms throttled link check.
			fireEvent.mouseMove(document, cellPoint(3, 0));

			expect(mounted.canvas.style.cursor).toBe("text");
			await mounted.dispose();
		});

		it("does not freeze a 'pointer' cursor once the app enables mouse reporting mid-session", async () => {
			const mounted = await mountCanvasTerminal({ sessionId: "s3", terminalId: "t3" });
			fakeTransport.current!.pushFrame(buildTextFrame(["hello world"], 40));

			// Hover the gutter first — this is the realistic trigger: the mouse
			// merely drifted over the leftmost strip at some point before the app
			// (e.g. Claude Code CLI) turned on mouse tracking.
			fireEvent.mouseMove(document, gutterPoint(0));
			expect(mounted.canvas.style.cursor).toBe("pointer");

			// The app now enables xterm mouse reporting (mode 1) — mirrors Claude
			// Code CLI's own click-to-reposition-cursor feature turning on.
			fakeTransport.current!.pushFrame(buildTextFrame(["hello world"], 40, { frameFlags: frameFlagsForMouseMode(1) }));

			// Any subsequent move — even one that never re-enters the gutter —
			// must not leave the hand cursor frozen from before mouse mode turned on.
			fireEvent.mouseMove(document, cellPoint(3, 0));

			expect(mounted.canvas.style.cursor).toBe("text");
			await mounted.dispose();
		});

		it("hovering the gutter while mouse reporting is active does not show the copy/fold pointer cursor", async () => {
			const mounted = await mountCanvasTerminal({ sessionId: "s4", terminalId: "t4" });
			fakeTransport.current!.pushFrame(buildTextFrame(["hello world"], 40, { frameFlags: frameFlagsForMouseMode(1) }));

			// The app owns click semantics once it has enabled mouse reporting —
			// our own gutter affordance must not paint over it.
			fireEvent.mouseMove(document, gutterPoint(0));

			expect(mounted.canvas.style.cursor).toBe("text");
			await mounted.dispose();
		});
	});
});
