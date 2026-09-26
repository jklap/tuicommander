/**
 * Coverage for `updateKeyboardLift`/`kbLiftRef` (CanvasTerminal.tsx), the
 * consumer of `keyboardViewport.ts`'s `keyboardOcclusion` signal. Found
 * alongside `keyboardOcclusion` while investigating an unrelated "bottom of
 * terminal not visible" report as having zero test coverage anywhere — this
 * mechanism turned out not to be the cause there (it was a desktop-browser
 * report, no on-screen keyboard involved), but the gap was real on its own.
 *
 * Exercises the real, documented invariant: the lift is anchored to the
 * CURSOR row, not the pane's bottom edge — "the mirror of the original
 * occlusion bug" per the component's own comment — so it must clear when
 * unfocused, when the keyboard is closed, and when scrolled back into
 * history (cursor off-screen), and must track the cursor's actual position
 * when none of those hold.
 */

import { fireEvent, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeTerminal } from "../../../__tests__/helpers/store";
import { terminalsStore } from "../../../stores/terminals";
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

const TERM_ID = "kb-lift-t1";
const SESSION_ID = "kb-lift-s1";
const ROWS = 30;

/** A minimal fake VisualViewport — same shape `keyboardViewport.test.ts` uses,
 *  duplicated locally rather than shared: that file resets modules per test
 *  (this one deliberately does NOT — see the single-test-body note below). */
function makeFakeVisualViewport(offsetTop: number, height: number) {
	const listeners: Array<() => void> = [];
	return {
		offsetTop,
		height,
		addEventListener(_type: string, listener: () => void) {
			listeners.push(listener);
		},
		removeEventListener() {},
		fire() {
			for (const l of listeners) l();
		},
	};
}

/** The single `div[style*="will-change: transform"]` `kbLiftRef` wraps a
 *  second, identically-styled inner `stageRef` div — `querySelectorAll`
 *  returns document order, so the outer (kbLiftRef) is always index 0. */
function kbLiftDiv(container: HTMLElement): HTMLDivElement {
	const matches = container.querySelectorAll('div[style*="will-change: transform"]');
	if (matches.length < 1) throw new Error("kbLiftRef div not found");
	return matches[0] as HTMLDivElement;
}

let restoreCanvasEnv: () => void;

beforeEach(() => {
	restoreCanvasEnv = stubCanvasEnvironment();
	fakeTransport.current = createFakeTransport();
	Object.defineProperty(navigator, "maxTouchPoints", { value: 1, configurable: true });
	Object.defineProperty(window, "innerHeight", { value: 800, writable: true, configurable: true });
});

afterEach(() => {
	restoreCanvasEnv();
	terminalsStore.remove(TERM_ID);
	Object.defineProperty(navigator, "maxTouchPoints", { value: 0, configurable: true });
	Object.defineProperty(window, "visualViewport", { value: undefined, writable: true, configurable: true });
});

describe("CanvasTerminal keyboard lift (kbLiftRef)", () => {
	it("lifts only the cursor row above the keyboard, anchored to the cursor not the pane bottom", async () => {
		// No keyboard yet (visualViewport covers the full layout viewport) —
		// installed once, at mount, by CanvasTerminal's own effect.
		const vv = makeFakeVisualViewport(0, 800);
		Object.defineProperty(window, "visualViewport", { value: vv, writable: true, configurable: true });

		terminalsStore.register(TERM_ID, makeTerminal({ sessionId: SESSION_ID }));
		const mounted = await mountCanvasTerminal({ sessionId: SESSION_ID, terminalId: TERM_ID });
		const input = mounted.container.querySelector("input");
		if (!input) throw new Error("hidden keyboard-input element not found");

		const rows = Array.from({ length: ROWS }, (_, i) => `row ${i}`);
		// cursorRow=29 (last row): cursorBottomY = (29+1)*20 = 600.
		fakeTransport.current!.pushFrame(buildTextFrame(rows, 40, { cursorRow: 29, displayOffset: 0 }));

		fireEvent.focus(input);
		await waitFor(
			() => {
				expect(kbLiftDiv(mounted.container).style.transform).toBe("");
			},
			{ timeout: 2000 },
		);

		// Keyboard opens: occludes the bottom 300px → keyboardTop = 800-300 = 500.
		// lift = round(600 - 500) = 100.
		vv.height = 500;
		vv.fire();
		await waitFor(
			() => {
				expect(kbLiftDiv(mounted.container).style.transform).toBe("translateY(-100px)");
			},
			{ timeout: 2000 },
		);

		// Scrolled back into history: cursor is off-screen (displayOffset > 0) —
		// must clear even though focused and the keyboard is still open.
		fakeTransport.current!.pushFrame(buildTextFrame(rows, 40, { cursorRow: 29, displayOffset: 5 }));
		await waitFor(() => {
			expect(kbLiftDiv(mounted.container).style.transform).toBe("");
		});

		// Back at the bottom of history: the lift must resume, proving it's
		// reactive to cursor movement, not a one-shot computation.
		fakeTransport.current!.pushFrame(buildTextFrame(rows, 40, { cursorRow: 29, displayOffset: 0 }));
		await waitFor(() => {
			expect(kbLiftDiv(mounted.container).style.transform).toBe("translateY(-100px)");
		});

		// Losing focus clears it even though the keyboard is still open and the
		// cursor is still on-screen — only the focused terminal lifts.
		fireEvent.blur(input);
		await waitFor(() => {
			expect(kbLiftDiv(mounted.container).style.transform).toBe("");
		});

		await mounted.dispose();
	});
});
