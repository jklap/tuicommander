import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Copy-on-select keeps the highlight (#1534-66ac).
 *
 * Oracle: after mouse-up copies a selection that crosses a soft-wrapped line, the
 * backend answers with the UNWRAPPED text (differs from the row-joined text on
 * screen). The next full-replace frame carries identical rows, so the user's
 * highlight must still be painted.
 */

const { invoke, frameSink, fills } = vi.hoisted(() => ({
	invoke: vi.fn(),
	frameSink: { current: null as ((data: ArrayBuffer) => void) | null },
	fills: { styles: [] as string[] },
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
	createGridRenderer: () => ({
		setTheme: vi.fn(),
		invalidateCaches: vi.fn(),
		paintGrid: vi.fn(),
		paintRow: vi.fn(),
	}),
}));
vi.mock("../../utils/clipboard", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../utils/clipboard")>()),
	writeClipboard: vi.fn().mockResolvedValue(undefined),
}));

import CanvasTerminal from "../../components/Terminal/CanvasTerminal";

const ROWS = 4;
const COLS = 8;
const HEADER_SIZE = 26;
const CELL_SIZE = 11;
const SELECTION_FILL = "rgba(58, 130, 220, 0.35)";

function fullFrame(): ArrayBuffer {
	const buffer = new ArrayBuffer(HEADER_SIZE + ROWS * (4 + COLS * CELL_SIZE));
	const view = new DataView(buffer);
	view.setUint16(0, ROWS, true);
	view.setUint8(6, 1);
	view.setUint16(18, ROWS, true);
	view.setUint16(20, COLS, true);
	let offset = HEADER_SIZE;
	for (let r = 0; r < ROWS; r++) {
		const text = `line${r}`.padEnd(COLS, " ");
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

describe("CanvasTerminal copy-on-select highlight", () => {
	beforeEach(() => {
		invoke.mockReset();
		// Backend text: soft wrap unwrapped, so it differs from the local rows joined by "\n".
		invoke.mockImplementation(async (cmd: string) =>
			cmd === "terminal_get_selection_text" ? "line0line1" : undefined,
		);
		fills.styles = [];
		frameSink.current = null;
		Object.defineProperty(document, "fonts", {
			configurable: true,
			value: { load: () => Promise.resolve([]), ready: Promise.resolve() },
		});
		let style = "";
		vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
			new Proxy(
				{},
				{
					set: (_t, prop, value) => {
						if (prop === "fillStyle") style = String(value);
						return true;
					},
					get: (_t, prop) => {
						if (prop === "measureText")
							return () => ({ width: 8, fontBoundingBoxAscent: 10, fontBoundingBoxDescent: 3 });
						if (prop === "fillRect") return () => fills.styles.push(style);
						return vi.fn();
					},
				},
			) as unknown as CanvasRenderingContext2D,
		);
		vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
			width: 800,
			height: 1000,
			top: 0,
			left: 0,
			right: 800,
			bottom: 1000,
			x: 0,
			y: 0,
			toJSON: () => ({}),
		});
		for (const name of ["ResizeObserver", "IntersectionObserver"]) {
			vi.stubGlobal(
				name,
				class {
					observe() {}
					disconnect() {}
				},
			);
		}
	});

	afterEach(() => {
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	// catches: the post-frame check compares the local rows against the clipboard text
	// (soft wraps unwrapped), so the highlight vanishes on the first frame after the copy.
	it("keeps painting the selection after a full-replace frame follows a wrapped-line copy", async () => {
		const view = render(() => <CanvasTerminal sessionId="hl-1534" terminalId="hl-1534" />);
		try {
			await waitFor(() => expect(frameSink.current).not.toBeNull());
			frameSink.current?.(fullFrame());
			const canvas = view.container.querySelector('canvas[tabindex="0"]');
			if (!canvas) throw new Error("terminal canvas not mounted");

			canvas.dispatchEvent(new MouseEvent("mousedown", { clientX: 12, clientY: 1, bubbles: true, cancelable: true }));
			document.dispatchEvent(new MouseEvent("mousemove", { clientX: 40, clientY: 20, bubbles: true }));
			await new Promise((r) => setTimeout(r, 50));
			document.dispatchEvent(new MouseEvent("mouseup", { clientX: 40, clientY: 20, bubbles: true }));
			await waitFor(() => expect(invoke.mock.calls.map(([c]) => c)).toContain("terminal_get_selection_text"));
			await new Promise((r) => setTimeout(r, 50));

			// Precondition: the selection was painted at all.
			expect(fills.styles).toContain(SELECTION_FILL);

			fills.styles = [];
			frameSink.current?.(fullFrame());
			await new Promise((r) => setTimeout(r, 100));

			expect(fills.styles).toContain(SELECTION_FILL);
		} finally {
			view.unmount();
		}
	});
});
