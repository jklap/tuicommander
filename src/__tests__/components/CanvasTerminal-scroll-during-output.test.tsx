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

const { invoke, frameSink, paintGrid } = vi.hoisted(() => ({
	invoke: vi.fn().mockResolvedValue(undefined),
	paintGrid: vi.fn(),
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
	createGridRenderer: () => ({ setTheme: vi.fn(), invalidateCaches: vi.fn(), paintGrid, paintRow: vi.fn() }),
}));

import CanvasTerminal from "../../components/Terminal/CanvasTerminal";

const ROWS = 4;
const COLS = 8;
const HEADER_SIZE = 26;
const CELL_SIZE = 11;

/** A full-screen frame of `ROWS` rows, each labelled with its all-time line number. */
function fullFrame(opts: {
	historySize: number;
	displayOffset: number;
	historyBase?: number;
	textOf?: (abs: number) => string;
}): ArrayBuffer {
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
		const text = (opts.textOf?.(top + r) ?? `L${top + r}`).padEnd(COLS, " ");
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

/** What `terminal_styled_rows` answers: rows `[start, start+count)` that exist, text from `textOf`. */
function styledRange(
	start: number,
	count: number,
	total: number,
	historySize: number,
	textOf: (abs: number) => string,
) {
	const abs = Array.from({ length: count }, (_, i) => start + i).filter((a) => a < total);
	const buffer = new ArrayBuffer(12 + abs.length * (6 + COLS * CELL_SIZE));
	const view = new DataView(buffer);
	view.setUint32(0, start, true);
	view.setUint32(4, historySize, true);
	view.setUint16(8, COLS, true);
	view.setUint16(10, abs.length, true);
	let offset = 12;
	for (const a of abs) {
		view.setUint32(offset, a, true);
		view.setUint16(offset + 4, COLS, true);
		offset += 6;
		for (const char of textOf(a).padEnd(COLS, " ")) {
			view.setUint32(offset, char.codePointAt(0) ?? 0, true);
			offset += CELL_SIZE;
		}
	}
	return buffer;
}

/** Text of every row of the last paint, top to bottom; "?" for a row that was not in the cache. */
function paintedLines(): string[] {
	const rows = paintGrid.mock.calls.at(-1)?.[0] as Map<number, { codepoints: Uint32Array }> | undefined;
	if (!rows) return [];
	const last = Math.max(...rows.keys());
	return Array.from({ length: last + 1 }, (_, r) => {
		const row = rows.get(r);
		return row ? String.fromCodePoint(...Array.from(row.codepoints).map((c) => c || 32)).trimEnd() : "?";
	});
}

/** A pane with a real box and a canvas whose every call is a no-op, so cached-row paints run. */
function layOutPane() {
	vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
		new Proxy(
			{},
			{
				get: (_t, prop) =>
					prop === "measureText" ? () => ({ width: 8, fontBoundingBoxAscent: 10, fontBoundingBoxDescent: 3 }) : vi.fn(),
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

	// catches: a chunk fetched while lines 100-103 were still the agent's live
	// region is cached for good, so once the redraw commits them to history the
	// user scrolling back sees the stale rows instead of the lines that were written.
	it("shows the committed lines, not the live-region rows cached before they were committed", async () => {
		// Backend truth: 100 history lines "H<n>". Lines 100-103 are first the agent's
		// live region ("V<n>"); the redraw then commits lines 100-104 as "F<n>".
		let committed = false;
		const historySize = () => (committed ? 105 : 100);
		const textOf = (abs: number) => (abs < 100 ? `H${abs}` : committed && abs < 105 ? `F${abs}` : `V${abs}`);
		const truth = () => Array.from({ length: historySize() + ROWS }, (_, abs) => textOf(abs));
		invoke.mockImplementation(async (cmd: string, args: { start?: number; count?: number }) => {
			if (cmd !== "terminal_styled_rows") return undefined;
			return styledRange(args.start ?? 0, args.count ?? 0, historySize() + ROWS, historySize(), textOf);
		});
		paintGrid.mockClear();
		layOutPane();
		const view = render(() => <CanvasTerminal sessionId="scroll-1264-live" terminalId="scroll-1264-live" />);
		try {
			await waitFor(() => expect(frameSink.current).not.toBeNull());
			frameSink.current?.(fullFrame({ historySize: 100, displayOffset: 0, textOf }));
			const canvas = view.container.querySelector('canvas[tabindex="0"]');
			if (!canvas) throw new Error("terminal canvas not mounted");

			wheel(canvas, -80);
			await waitFor(() => expect(sentOffsets().length).toBe(1));
			const firstOffset = sentOffsets()[0];
			// The chunk holding lines 64-127 is fetched while 100-103 are still live.
			await waitFor(() => expect(invoke.mock.calls.map(([c]) => c)).toContain("terminal_styled_rows"));
			await new Promise((r) => setTimeout(r, 20));

			// The redraw commits: 5 lines enter history, the viewport keeps its lines.
			committed = true;
			paintGrid.mockClear();
			frameSink.current?.(fullFrame({ historySize: 105, displayOffset: firstOffset + 5, textOf }));

			// Same gesture, still scrolled back, moving toward the bottom over the rows
			// that were live when the chunk was cached.
			wheel(canvas, 40);
			await new Promise((r) => setTimeout(r, 150));

			// A row painted from the cache is either not known yet or one of the real
			// lines of the output — never what it read before the redraw committed.
			const real = new Set(truth());
			for (const [rows] of paintGrid.mock.calls as Array<[Map<number, { codepoints: Uint32Array }>]>) {
				const lines = Array.from(rows.values(), (row) =>
					String.fromCodePoint(...Array.from(row.codepoints).map((c) => c || 32)).trimEnd(),
				);
				expect(lines.filter((line) => !real.has(line))).toEqual([]);
			}
			// Once the refetch lands, the whole viewport is a run of real lines.
			const lines = paintedLines();
			const start = truth().indexOf(lines[0]);
			expect(start).toBeGreaterThanOrEqual(0);
			expect(lines).toEqual(truth().slice(start, start + lines.length));
		} finally {
			view.unmount();
		}
	});

	// catches: frames that rewrite live rows in place while the backend still lags
	// the gesture are not seeded, so the cache keeps painting the spinner/status
	// text the row held before the rewrite.
	it("paints a live row rewritten in place while the gesture is still running", async () => {
		let spinner = "S1";
		const textOf = (abs: number) => (abs < 100 ? `H${abs}` : abs === 102 ? spinner : `V${abs}`);
		invoke.mockImplementation(async (cmd: string, args: { start?: number; count?: number }) => {
			if (cmd !== "terminal_styled_rows") return undefined;
			return styledRange(args.start ?? 0, args.count ?? 0, 100 + ROWS, 100, textOf);
		});
		paintGrid.mockClear();
		layOutPane();
		const view = render(() => <CanvasTerminal sessionId="scroll-1264-inplace" terminalId="scroll-1264-inplace" />);
		try {
			await waitFor(() => expect(frameSink.current).not.toBeNull());
			frameSink.current?.(fullFrame({ historySize: 100, displayOffset: 0, textOf }));
			const canvas = view.container.querySelector('canvas[tabindex="0"]');
			if (!canvas) throw new Error("terminal canvas not mounted");

			wheel(canvas, -80);
			await waitFor(() => expect(invoke.mock.calls.map(([c]) => c)).toContain("terminal_styled_rows"));
			await new Promise((r) => setTimeout(r, 20));

			// The agent redraws its spinner row. History does not grow, and the
			// backend has not reached the gesture offset yet (offset 0 != position).
			spinner = "S2";
			frameSink.current?.(fullFrame({ historySize: 100, displayOffset: 0, textOf }));
			await new Promise((r) => setTimeout(r, 100));

			const lines = paintedLines();
			expect(lines).toContain("S2");
			expect(lines).not.toContain("S1");
		} finally {
			view.unmount();
		}
	});
});
