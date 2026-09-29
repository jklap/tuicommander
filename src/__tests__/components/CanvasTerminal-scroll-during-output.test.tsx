import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * A wheel gesture that is still running when output arrives (#1264-89c8).
 *
 * The oracle is the terminal contract, not the implementation: while the user
 * is scrolled back, new output must not move the lines they are reading. The
 * backend grid (alacritty) honours this by raising `display_offset` by exactly
 * the number of lines that scrolled into history. So when the frontend next
 * asks for a viewport, the offset it sends must include those lines — otherwise
 * the view jumps forward by the output and the lines in between are never shown.
 */

const { invoke, frameSink } = vi.hoisted(() => ({
	invoke: vi.fn().mockResolvedValue(undefined),
	frameSink: { current: null as ((data: ArrayBuffer) => void) | null },
}));

vi.mock("../../components/Terminal/canvasTerminalTransport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../components/Terminal/canvasTerminalTransport")>()),
	createTransport: () => ({
		onEvent: vi.fn().mockResolvedValue(undefined),
		subscribe: vi.fn(async (onFrame: (data: ArrayBuffer) => void) => {
			frameSink.current = onFrame;
		}),
		resubscribe: vi.fn().mockResolvedValue(undefined),
		unsubscribe: vi.fn(),
		ackFrame: vi.fn(),
		invoke,
	}),
}));
vi.mock("../../components/Terminal/gridRenderer", () => ({
	createGridRenderer: () => ({ setTheme: vi.fn(), invalidateCaches: vi.fn(), paintGrid: vi.fn(), paintRow: vi.fn() }),
}));

import CanvasTerminal from "../../components/Terminal/CanvasTerminal";

const ROWS = 4;
const COLS = 8;
const HEADER_SIZE = 26;
const CELL_SIZE = 11;

/** A full-screen frame of `ROWS` rows, each labelled with its all-time line number. */
function fullFrame(opts: { historySize: number; displayOffset: number; historyBase?: number }): ArrayBuffer {
	const historyBase = opts.historyBase ?? 0;
	const top = historyBase + opts.historySize - opts.displayOffset;
	const buffer = new ArrayBuffer(HEADER_SIZE + ROWS * (4 + COLS * CELL_SIZE));
	const view = new DataView(buffer);
	view.setUint16(0, ROWS, true);
	view.setUint8(6, 1);
	view.setUint32(7, opts.displayOffset, true);
	view.setUint32(11, opts.historySize, true);
	view.setUint16(18, ROWS, true);
	view.setUint16(20, COLS, true);
	view.setUint32(22, historyBase, true);
	let offset = HEADER_SIZE;
	for (let r = 0; r < ROWS; r++) {
		const text = `L${top + r}`.padEnd(COLS, " ");
		view.setUint16(offset, r, true);
		view.setUint16(offset + 2, COLS, true);
		offset += 4;
		for (const char of text) {
			view.setUint32(offset, char.codePointAt(0) ?? 0, true);
			offset += CELL_SIZE;
		}
	}
	return buffer;
}

function sentOffsets(): number[] {
	return invoke.mock.calls
		.filter(([cmd]) => cmd === "terminal_scroll_to_offset")
		.map(([, args]) => (args as { offset: number }).offset);
}

function wheel(target: Element, deltaY: number) {
	target.dispatchEvent(new WheelEvent("wheel", { deltaY, bubbles: true, cancelable: true }));
}

describe("CanvasTerminal scroll gesture during output", () => {
	beforeEach(() => {
		invoke.mockClear();
		frameSink.current = null;
		Object.defineProperty(document, "fonts", {
			configurable: true,
			value: { load: () => Promise.resolve([]), ready: Promise.resolve() },
		});
		vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
			clearRect: vi.fn(),
			setTransform: vi.fn(),
			fillRect: vi.fn(),
			measureText: () => ({ width: 8 }),
		} as unknown as CanvasRenderingContext2D);
		vi.stubGlobal(
			"ResizeObserver",
			class {
				observe() {}
				disconnect() {}
			},
		);
		vi.stubGlobal(
			"IntersectionObserver",
			class {
				observe() {}
				disconnect() {}
			},
		);
	});

	afterEach(() => {
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	// catches: the gesture keeps its distance-from-bottom while the grid pushes
	// output into history, so the next flush scrolls the backend forward by the
	// output and those lines are skipped.
	it("keeps the lines under the user when output lands mid-gesture", async () => {
		const view = render(() => <CanvasTerminal sessionId="scroll-1264" terminalId="scroll-1264" />);
		try {
			await waitFor(() => expect(frameSink.current).not.toBeNull());
			frameSink.current?.(fullFrame({ historySize: 100, displayOffset: 0 }));
			const canvas = view.container.querySelector('canvas[tabindex="0"]');
			if (!canvas) throw new Error("terminal canvas not mounted");

			wheel(canvas, -80);
			await waitFor(() => expect(sentOffsets().length).toBe(1));
			const firstOffset = sentOffsets()[0];
			expect(firstOffset).toBeGreaterThan(0);

			// The backend applied that offset, then 5 lines of output scrolled into
			// history. Alacritty keeps the viewport still: offset and history both +5.
			frameSink.current?.(fullFrame({ historySize: 105, displayOffset: firstOffset + 5 }));

			// Still the same gesture: scroll up a little more.
			wheel(canvas, -80);
			await waitFor(() => expect(sentOffsets().length).toBe(2));
			const secondOffset = sentOffsets()[1];

			// Scrolling UP can only move the top line to an older one. The line the
			// user was reading is now `firstOffset + 5` from the bottom, so anything
			// smaller jumps the view forward over the new output.
			expect(secondOffset).toBeGreaterThan(firstOffset + 5);
		} finally {
			view.unmount();
		}
	});
});
