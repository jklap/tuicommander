/**
 * Phase 0c of the Smart Selection plan (see
 * `~/.claude/plans/currently-when-you-double-unified-minsky.md`): pin
 * CanvasTerminal's mouse-gesture behavior through a real mount + real DOM
 * events. Originally written against pre-Phase-4 behavior; the
 * double-click-on-URL-fragment case below was updated in the same commit
 * that wired smart selection into the mousedown handler (Phase 4) — smart
 * selection is enabled with `doubleClickAction: "smart"` by default, so a
 * double-click inside "https" now expands across the whole URL via the
 * built-in `iterm-http-url` rule instead of stopping at the scheme. See that
 * test's own comment for the before/after.
 *
 * Known gap, deliberately deferred rather than silently skipped:
 *  - Gutter click (select block output) needs a synthesized `commandBlocks`
 *    entry via OSC 133 plumbing that's out of scope for a mouse-gesture pin.
 */

import { fireEvent, waitFor, within } from "@solidjs/testing-library";
import { openUrl as tauriOpenUrl } from "@tauri-apps/plugin-opener";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { settingsStore } from "../../../stores/settings";
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

/** Center-of-cell client coordinates for grid (col, row), matching canvasToGrid's math. */
function cellPoint(col: number, row: number) {
	return {
		clientX: GUTTER_PX + col * FIXED_CELL_METRICS.cellWidth + FIXED_CELL_METRICS.cellWidth / 2,
		clientY: row * FIXED_CELL_METRICS.cellHeight + FIXED_CELL_METRICS.cellHeight / 2,
	};
}

function click(canvas: HTMLCanvasElement, col: number, row: number, opts: Partial<MouseEventInit> = {}) {
	const point = cellPoint(col, row);
	fireEvent.mouseDown(canvas, { button: 0, ...point, ...opts });
	fireEvent.mouseUp(canvas, { button: 0, ...point, ...opts });
}

function doubleClick(canvas: HTMLCanvasElement, col: number, row: number, opts: Partial<MouseEventInit> = {}) {
	const point = cellPoint(col, row);
	fireEvent.mouseDown(canvas, { button: 0, ...point, ...opts });
	fireEvent.mouseDown(canvas, { button: 0, ...point, ...opts });
	fireEvent.mouseUp(canvas, { button: 0, ...point, ...opts });
}

function tripleClick(canvas: HTMLCanvasElement, col: number, row: number) {
	const point = cellPoint(col, row);
	fireEvent.mouseDown(canvas, { button: 0, ...point });
	fireEvent.mouseDown(canvas, { button: 0, ...point });
	fireEvent.mouseDown(canvas, { button: 0, ...point });
	fireEvent.mouseUp(canvas, { button: 0, ...point });
}

async function selectionText(ref: { getSelectionText: () => string }, expected: string) {
	await waitFor(() => expect(ref.getSelectionText()).toBe(expected));
}

describe("CanvasTerminal mouse gestures — pinned behavior (Phase 0)", () => {
	let restoreEnv: () => void;

	beforeEach(() => {
		restoreEnv = stubCanvasEnvironment();
		fakeTransport.current = createFakeTransport();
	});

	afterEach(() => {
		restoreEnv();
		settingsStore.setLinkActivation("click");
		settingsStore.setDoubleClickAction("smart");
	});

	it("single click sets a caret with no selectable range", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "s1", terminalId: "t1" });
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

		click(mounted.canvas, 5, 0);

		// No range was ever copied — cachedText stays at its initial "".
		expect(mounted.ref.getSelectionText()).toBe("");
		expect(fakeTransport.current!.invokeCalls.some((c) => c.cmd === "terminal_get_selection_text")).toBe(false);
		await mounted.dispose();
	});

	it("double-click selects the word under the cursor", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "s2", terminalId: "t2" });
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

		doubleClick(mounted.canvas, 5, 0); // "bar" spans cols 4-6
		await selectionText(mounted.ref, "bar");
		await mounted.dispose();
	});

	it("double-click on the scheme of a URL selects the whole URL (smart selection, the default)", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "s3", terminalId: "t3" });
		fakeTransport.current!.pushFrame(buildTextFrame(["open https://example.com/a now"], 40));

		// doubleClickAction defaults to "smart" — the built-in iterm-http-url
		// rule matches the whole URL, so a click anywhere in "https" expands
		// across the scheme, host, and path, not just the "S"eparator-bounded
		// scheme token word-mode selection would give.
		doubleClick(mounted.canvas, 7, 0);
		await selectionText(mounted.ref, "https://example.com/a");
		await mounted.dispose();
	});

	it("double-click on the scheme of a URL selects only the scheme token when doubleClickAction is 'word'", async () => {
		settingsStore.setDoubleClickAction("word");
		const mounted = await mountCanvasTerminal({ sessionId: "s3b", terminalId: "t3b" });
		fakeTransport.current!.pushFrame(buildTextFrame(["open https://example.com/a now"], 40));

		// "https" spans cols 5-9; "/", ":", "." are separators in word mode.
		doubleClick(mounted.canvas, 7, 0);
		await selectionText(mounted.ref, "https");
		await mounted.dispose();
	});

	it("triple-click selects the whole line", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "s4", terminalId: "t4" });
		// A real row is always padded to the full screen width (80 here, comfortably
		// wider than the harness's fixed 800px mount rect implies via lastGridCol) —
		// trailing space cells are trimmed by getLocalText's `.replace(/\s+$/, "")`.
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 80));

		tripleClick(mounted.canvas, 5, 0);
		await selectionText(mounted.ref, "foo bar baz");
		await mounted.dispose();
	});

	it("shift+click extends the selection from the existing anchor", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "s5", terminalId: "t5" });
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

		const anchor = cellPoint(0, 0);
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...anchor });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...anchor });

		const extend = cellPoint(6, 0);
		fireEvent.mouseDown(mounted.canvas, { button: 0, shiftKey: true, ...extend });
		fireEvent.mouseUp(mounted.canvas, { button: 0, shiftKey: true, ...extend });

		await selectionText(mounted.ref, "foo bar");
		await mounted.dispose();
	});

	it("double-click landing on whitespace falls back to a caret (no range)", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "s6", terminalId: "t6" });
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

		doubleClick(mounted.canvas, 3, 0); // the space between "foo" and "bar"
		expect(mounted.ref.getSelectionText()).toBe("");
		await mounted.dispose();
	});

	it("dragging after a double-click keeps the anchored word selected when dragging forward", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "s7", terminalId: "t7" });
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz qux"], 40));

		const wordPoint = cellPoint(5, 0); // inside "bar" (cols 4-6)
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...wordPoint });
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...wordPoint });
		// Drag forward into "qux" (cols 12-14).
		const dragPoint = cellPoint(13, 0);
		fireEvent.mouseMove(document, { buttons: 1, ...dragPoint });
		await new Promise((r) => requestAnimationFrame(r));
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...dragPoint });

		await selectionText(mounted.ref, "bar baz qux");
		await mounted.dispose();
	});

	it("dragging after a double-click keeps the anchored word selected when dragging backward", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "s8", terminalId: "t8" });
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz qux"], 40));

		const wordPoint = cellPoint(9, 0); // inside "baz" (cols 8-10)
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...wordPoint });
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...wordPoint });
		// Drag backward into "foo" (cols 0-2).
		const dragPoint = cellPoint(1, 0);
		fireEvent.mouseMove(document, { buttons: 1, ...dragPoint });
		await new Promise((r) => requestAnimationFrame(r));
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...dragPoint });

		await selectionText(mounted.ref, "foo bar baz");
		await mounted.dispose();
	});

	describe("link click activation (linkActivation setting)", () => {
		function primeLinkRow(text: string) {
			fakeTransport.current!.setInvokeHandler("terminal_get_row_text", () => text);
			fakeTransport.current!.setInvokeHandler("terminal_hyperlink_span", () => null);
		}

		async function hoverLink(col: number, row: number) {
			const point = cellPoint(col, row);
			fireEvent.mouseMove(document, point);
			// checkLinksAtRow is debounced 100ms then awaits two invoke round-trips.
			await new Promise((r) => setTimeout(r, 150));
		}

		it("click mode opens the link on a plain click", async () => {
			settingsStore.setLinkActivation("click");
			const mounted = await mountCanvasTerminal({ sessionId: "s9", terminalId: "t9" });
			fakeTransport.current!.pushFrame(buildTextFrame(["see https://example.com/a here"], 40));
			primeLinkRow("see https://example.com/a here");

			await hoverLink(6, 0);
			const point = cellPoint(6, 0);
			fireEvent.click(mounted.canvas, point);

			await waitFor(() => expect(tauriOpenUrl).toHaveBeenCalledWith("https://example.com/a"));
			expect(mounted.ref.getSelectionText()).toBe("");
			await mounted.dispose();
		});

		it("modifier mode does not open the link on a plain click", async () => {
			settingsStore.setLinkActivation("modifier");
			const mounted = await mountCanvasTerminal({ sessionId: "s10", terminalId: "t10" });
			fakeTransport.current!.pushFrame(buildTextFrame(["see https://example.com/a here"], 40));
			primeLinkRow("see https://example.com/a here");

			// In "modifier" mode, hover resolution itself is gated on the modifier
			// being held (CanvasTerminal.tsx onMouseMove) — without it, no link is
			// ever resolved, so a plain click has nothing to open.
			const point = cellPoint(6, 0);
			fireEvent.mouseMove(document, point);
			await new Promise((r) => setTimeout(r, 150));
			fireEvent.click(mounted.canvas, point);

			expect(fakeTransport.current!.invokeCalls.some((c) => c.cmd === "terminal_get_row_text")).toBe(false);
			await mounted.dispose();
		});
	});

	describe("mouse-reporting forward vs. local selection", () => {
		function buildMouseModeFrame(text: string, mouseMode: 0 | 1 | 2 | 3) {
			// bit 0x20 = sgrMouse — without it writePtyNoScroll's `if (currentFrame.sgrMouse)`
			// guard suppresses the report entirely even though the gesture is still claimed.
			return buildTextFrame([text], 40, { frameFlags: (mouseMode << 3) | 0x20 });
		}

		it("forwards a plain drag as a mouse report when the app has mouse-reporting on", async () => {
			const mounted = await mountCanvasTerminal({ sessionId: "s11", terminalId: "t11" });
			fakeTransport.current!.pushFrame(buildMouseModeFrame("foo bar baz", 1));

			click(mounted.canvas, 5, 0);

			expect(fakeTransport.current!.invokeCalls.some((c) => c.cmd === "write_pty")).toBe(true);
			expect(mounted.ref.getSelectionText()).toBe("");
			await mounted.dispose();
		});

		it("shift+click still selects locally even when the app has mouse-reporting on", async () => {
			const mounted = await mountCanvasTerminal({ sessionId: "s12", terminalId: "t12" });
			fakeTransport.current!.pushFrame(buildMouseModeFrame("foo bar baz", 1));

			// Shift is the escape hatch (shouldForwardMouseGesture) that keeps a
			// gesture local regardless of mouseMode — anchor then extend, exactly
			// like the plain shift+click case, just with mouse-reporting turned on.
			const anchor = cellPoint(0, 0);
			fireEvent.mouseDown(mounted.canvas, { button: 0, shiftKey: true, ...anchor });
			fireEvent.mouseUp(mounted.canvas, { button: 0, shiftKey: true, ...anchor });
			const extend = cellPoint(6, 0);
			fireEvent.mouseDown(mounted.canvas, { button: 0, shiftKey: true, ...extend });
			fireEvent.mouseUp(mounted.canvas, { button: 0, shiftKey: true, ...extend });

			await selectionText(mounted.ref, "foo bar");
			expect(fakeTransport.current!.invokeCalls.some((c) => c.cmd === "write_pty")).toBe(false);
			await mounted.dispose();
		});
	});

	describe("right-click / link context menu", () => {
		it("right-clicking a detected link opens the Open/Copy link menu", async () => {
			const mounted = await mountCanvasTerminal({ sessionId: "s13", terminalId: "t13" });
			fakeTransport.current!.pushFrame(buildTextFrame(["see https://example.com/a here"], 40));
			fakeTransport.current!.setInvokeHandler("terminal_get_row_text", () => "see https://example.com/a here");
			fakeTransport.current!.setInvokeHandler("terminal_hyperlink_span", () => null);

			const point = cellPoint(6, 0); // inside "https"
			fireEvent.contextMenu(mounted.canvas, point);

			await waitFor(() => expect(within(mounted.container).getByText("Open")).toBeTruthy());
			expect(within(mounted.container).getByText("Copy link")).toBeTruthy();
			await mounted.dispose();
		});

		it("right-clicking elsewhere does not open the link menu", async () => {
			const mounted = await mountCanvasTerminal({ sessionId: "s14", terminalId: "t14" });
			fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

			const point = cellPoint(1, 0); // inside "foo" — no link there
			fireEvent.contextMenu(mounted.canvas, point);

			expect(within(mounted.container).queryByText("Open")).toBeNull();
			await mounted.dispose();
		});
	});
});
