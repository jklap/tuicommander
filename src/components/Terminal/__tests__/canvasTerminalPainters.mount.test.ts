/**
 * Pixel/draw-call-level coverage for the overlay canvas painters that render
 * Command Blocks: `paintGutterMarkers`, `paintFoldChevrons`, `paintFoldedBlocks`,
 * and `paintBlockTimestamps` (all in `CanvasTerminal.tsx`). Before this file,
 * these had only data/logic-level coverage (does the right block survive
 * `rowAnchoredBlocks()` filtering) — nothing asserted on what actually gets
 * drawn, at what color, or at what row. Mounts the real component and spies
 * on the real overlay `<canvas>`'s 2D context (via `createMockCtx2D`), the
 * same technique `mountCanvasTerminal.tsx`'s own helpers use elsewhere.
 */

import { fireEvent, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeTerminal } from "../../../__tests__/helpers/store";
import { settingsStore } from "../../../stores/settings";
import type { CommandBlock } from "../../../stores/terminals";
import { terminalsStore } from "../../../stores/terminals";
import { GUTTER_PX } from "../canvasTerminalUtils";
import { buildTextFrame } from "./helpers/frameFixture";
import {
	createFakeTransport,
	createMockCtx2D,
	FIXED_CELL_METRICS,
	mountCanvasTerminal,
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

const TERM_ID = "painters-t1";
const SESSION_ID = "painters-s1";

/** Same seam `stubCanvasEnvironment()` patches, plus a `WeakMap` recording
 *  the mock context handed back for each canvas element — the shared helper
 *  has no way to hand a created context back to the caller, and every
 *  method on it is a fresh `vi.fn()` per `getContext()` call, so a test
 *  that wants to inspect draw calls MUST capture the exact instance the
 *  component itself received. */
function stubCanvasEnvironmentWithCapture(): {
	restore: () => void;
	contexts: WeakMap<HTMLCanvasElement, CanvasRenderingContext2D>;
} {
	const contexts = new WeakMap<HTMLCanvasElement, CanvasRenderingContext2D>();
	const originalGetContext = HTMLCanvasElement.prototype.getContext;
	const originalGetRect = Element.prototype.getBoundingClientRect;
	const hadFonts = "fonts" in document;
	const originalFonts = (document as unknown as { fonts?: unknown }).fonts;

	HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement, type: string) {
		if (type !== "2d") return null;
		const ctx = createMockCtx2D(this);
		contexts.set(this, ctx);
		return ctx;
	} as typeof HTMLCanvasElement.prototype.getContext;

	const fixedRect = {
		left: 0,
		top: 0,
		width: 800,
		height: 600,
		right: 800,
		bottom: 600,
		x: 0,
		y: 0,
	};
	Element.prototype.getBoundingClientRect = () => ({ ...fixedRect, toJSON: () => fixedRect }) as DOMRect;

	Object.defineProperty(document, "fonts", {
		value: { load: vi.fn().mockResolvedValue(undefined), ready: Promise.resolve() },
		configurable: true,
	});

	return {
		contexts,
		restore: () => {
			HTMLCanvasElement.prototype.getContext = originalGetContext;
			Element.prototype.getBoundingClientRect = originalGetRect;
			if (hadFonts) {
				Object.defineProperty(document, "fonts", { value: originalFonts, configurable: true });
			} else {
				delete (document as unknown as { fonts?: unknown }).fonts;
			}
		},
	};
}

/** The overlay canvas is the 6th (last) `<canvas>` CanvasTerminal.tsx renders
 *  — overscan, interactive, below-text-image, glyph, image, THEN overlay
 *  (verified by reading the JSX: `overlayCanvasRef` is the last of six). */
function overlayCtx(
	container: HTMLElement,
	contexts: WeakMap<HTMLCanvasElement, CanvasRenderingContext2D>,
): CanvasRenderingContext2D {
	const canvases = container.querySelectorAll("canvas");
	const overlay = canvases[canvases.length - 1] as HTMLCanvasElement;
	const ctx = contexts.get(overlay);
	if (!ctx) throw new Error("overlay canvas has no captured 2D context");
	return ctx;
}

/** Center-of-cell client coordinates for grid (col, row), matching canvasToGrid's math
 *  — same formula `canvasTerminalGestures.pin.test.ts`'s own `cellPoint` uses. */
function cellPoint(col: number, row: number) {
	return {
		clientX: GUTTER_PX + col * FIXED_CELL_METRICS.cellWidth + FIXED_CELL_METRICS.cellWidth / 2,
		clientY: row * FIXED_CELL_METRICS.cellHeight + FIXED_CELL_METRICS.cellHeight / 2,
	};
}

function makeBlock(overrides: Partial<CommandBlock> = {}): CommandBlock {
	return {
		promptLine: 2,
		commandLine: 2,
		executionLine: 2,
		endLine: 3,
		exitCode: 0,
		startedAt: Date.now() - 5000,
		endedAt: Date.now(),
		promptText: null,
		onAltScreen: false,
		fromTranscriptDump: false,
		...overrides,
	};
}

let env: ReturnType<typeof stubCanvasEnvironmentWithCapture>;

beforeEach(() => {
	env = stubCanvasEnvironmentWithCapture();
	fakeTransport.current = createFakeTransport();
});

afterEach(() => {
	env.restore();
	terminalsStore.remove(TERM_ID);
	settingsStore.setBlockTimestampMode("modifier");
	settingsStore.setBlockFoldingEnabled(true);
});

async function mountAndPaint(rows = ["row0", "row1", "row2", "row3", "row4"]) {
	const mounted = await mountCanvasTerminal({ sessionId: SESSION_ID, terminalId: TERM_ID });
	fakeTransport.current!.pushFrame(buildTextFrame(rows, 40, { historySize: 0 }));
	await waitFor(() => {
		const ctx = overlayCtx(mounted.container, env.contexts);
		if (!vi.mocked(ctx.clearRect).mock.calls.length) throw new Error("overlay not painted yet");
	});
	return mounted;
}

describe("paintGutterMarkers (Command Blocks gutter marks)", () => {
	it("draws a green mark for a successfully-closed block", async () => {
		terminalsStore.register(TERM_ID, makeTerminal({ sessionId: SESSION_ID }));
		terminalsStore.update(TERM_ID, { commandBlocks: [makeBlock({ promptLine: 2, exitCode: 0 })] });
		const mounted = await mountAndPaint();

		const ctx = overlayCtx(mounted.container, env.contexts);
		const calls = vi.mocked(ctx.fillRect).mock.calls;
		// fillStyle is a plain property, overwritten synchronously right
		// before each fillRect call rather than recorded per-call, so this
		// asserts the call landed at the expected row (promptLine=2 *
		// cellHeight) rather than trying to also capture the color used.
		expect(calls.some(([x, y]) => x === -GUTTER_PX && y === 2 * FIXED_CELL_METRICS.cellHeight)).toBe(true);

		await mounted.dispose();
	});

	it("draws a red mark for a failed block", async () => {
		terminalsStore.register(TERM_ID, makeTerminal({ sessionId: SESSION_ID }));
		terminalsStore.update(TERM_ID, { commandBlocks: [makeBlock({ promptLine: 2, exitCode: 1 })] });
		const mounted = await mountAndPaint();

		const ctx = overlayCtx(mounted.container, env.contexts);
		const calls = vi.mocked(ctx.fillRect).mock.calls;
		expect(calls.some(([x, y]) => x === -GUTTER_PX && y === 2 * FIXED_CELL_METRICS.cellHeight)).toBe(true);

		await mounted.dispose();
	});

	it("does not draw a mark for a block still running (exitCode null)", async () => {
		terminalsStore.register(TERM_ID, makeTerminal({ sessionId: SESSION_ID }));
		terminalsStore.update(TERM_ID, {
			commandBlocks: [makeBlock({ promptLine: 2, exitCode: null, endLine: null })],
		});
		const mounted = await mountAndPaint();

		const ctx = overlayCtx(mounted.container, env.contexts);
		const calls = vi.mocked(ctx.fillRect).mock.calls;
		expect(calls.some(([x, y]) => x === -GUTTER_PX && y === 2 * FIXED_CELL_METRICS.cellHeight)).toBe(false);

		await mounted.dispose();
	});

	// Fullscreen-mode fix: an onAltScreen block has no valid row — must not
	// draw a mark at whatever meaningless row number it happens to carry.
	it("does not draw a mark for an onAltScreen block", async () => {
		terminalsStore.register(TERM_ID, makeTerminal({ sessionId: SESSION_ID }));
		terminalsStore.update(TERM_ID, {
			commandBlocks: [makeBlock({ promptLine: 2, exitCode: 0, onAltScreen: true })],
		});
		const mounted = await mountAndPaint();

		const ctx = overlayCtx(mounted.container, env.contexts);
		const calls = vi.mocked(ctx.fillRect).mock.calls;
		expect(calls.some(([x, y]) => x === -GUTTER_PX && y === 2 * FIXED_CELL_METRICS.cellHeight)).toBe(false);

		await mounted.dispose();
	});
});

describe("paintFoldChevrons (Command Blocks fold indicator)", () => {
	it("draws the unfolded chevron (▾) on a closed block's header row", async () => {
		terminalsStore.register(TERM_ID, makeTerminal({ sessionId: SESSION_ID }));
		terminalsStore.update(TERM_ID, {
			commandBlocks: [makeBlock({ promptLine: 1, executionLine: 1, endLine: 3 })],
		});
		const mounted = await mountAndPaint();

		const ctx = overlayCtx(mounted.container, env.contexts);
		const textCalls = vi.mocked(ctx.fillText).mock.calls;
		expect(textCalls.some(([text]) => text === "▾")).toBe(true);

		await mounted.dispose();
	});

	it("draws the folded chevron (▸) once the block is folded", async () => {
		terminalsStore.register(TERM_ID, makeTerminal({ sessionId: SESSION_ID }));
		terminalsStore.update(TERM_ID, {
			commandBlocks: [makeBlock({ promptLine: 1, executionLine: 1, endLine: 3 })],
		});
		terminalsStore.toggleBlockFold(TERM_ID, 1);
		const mounted = await mountAndPaint();

		const ctx = overlayCtx(mounted.container, env.contexts);
		const textCalls = vi.mocked(ctx.fillText).mock.calls;
		expect(textCalls.some(([text]) => text === "▸")).toBe(true);
		expect(textCalls.some(([text]) => text === "▾")).toBe(false);

		await mounted.dispose();
	});

	it("does not draw a chevron for an onAltScreen block", async () => {
		terminalsStore.register(TERM_ID, makeTerminal({ sessionId: SESSION_ID }));
		terminalsStore.update(TERM_ID, {
			commandBlocks: [makeBlock({ promptLine: 1, executionLine: 1, endLine: 3, onAltScreen: true })],
		});
		const mounted = await mountAndPaint();

		const ctx = overlayCtx(mounted.container, env.contexts);
		const textCalls = vi.mocked(ctx.fillText).mock.calls;
		expect(textCalls.some(([text]) => text === "▾" || text === "▸")).toBe(false);

		await mounted.dispose();
	});
});

describe("paintFoldedBlocks (Command Blocks fold summary)", () => {
	it("draws an opaque rect plus the 'N lines folded' label for a folded block", async () => {
		terminalsStore.register(TERM_ID, makeTerminal({ sessionId: SESSION_ID }));
		terminalsStore.update(TERM_ID, {
			commandBlocks: [makeBlock({ promptLine: 1, executionLine: 1, endLine: 3 })],
		});
		terminalsStore.toggleBlockFold(TERM_ID, 1);
		const mounted = await mountAndPaint();

		const ctx = overlayCtx(mounted.container, env.contexts);
		const textCalls = vi.mocked(ctx.fillText).mock.calls;
		expect(textCalls.some(([text]) => typeof text === "string" && text.includes("lines folded"))).toBe(true);

		await mounted.dispose();
	});

	it("draws nothing for a folded onAltScreen block", async () => {
		terminalsStore.register(TERM_ID, makeTerminal({ sessionId: SESSION_ID }));
		terminalsStore.update(TERM_ID, {
			commandBlocks: [makeBlock({ promptLine: 1, executionLine: 1, endLine: 3, onAltScreen: true })],
		});
		terminalsStore.toggleBlockFold(TERM_ID, 1);
		const mounted = await mountAndPaint();

		const ctx = overlayCtx(mounted.container, env.contexts);
		const textCalls = vi.mocked(ctx.fillText).mock.calls;
		expect(textCalls.some(([text]) => typeof text === "string" && text.includes("lines folded"))).toBe(false);

		await mounted.dispose();
	});
});

describe("paintBlockTimestamps (Command Blocks relative-time labels)", () => {
	it("draws a relative-time label for a closed block when mode is 'always'", async () => {
		settingsStore.setBlockTimestampMode("always");
		terminalsStore.register(TERM_ID, makeTerminal({ sessionId: SESSION_ID }));
		terminalsStore.update(TERM_ID, {
			commandBlocks: [makeBlock({ promptLine: 2, startedAt: Date.now() - 5000 })],
		});
		const mounted = await mountAndPaint();

		const ctx = overlayCtx(mounted.container, env.contexts);
		const textCalls = vi.mocked(ctx.fillText).mock.calls;
		expect(textCalls.some(([text]) => typeof text === "string" && /^\d+[smhd]$/.test(text))).toBe(true);

		await mounted.dispose();
	});

	it("draws nothing when mode is 'off'", async () => {
		settingsStore.setBlockTimestampMode("off");
		terminalsStore.register(TERM_ID, makeTerminal({ sessionId: SESSION_ID }));
		terminalsStore.update(TERM_ID, {
			commandBlocks: [makeBlock({ promptLine: 2, startedAt: Date.now() - 5000 })],
		});
		const mounted = await mountAndPaint();

		const ctx = overlayCtx(mounted.container, env.contexts);
		const textCalls = vi.mocked(ctx.fillText).mock.calls;
		expect(textCalls.some(([text]) => typeof text === "string" && /^\d+[smhd]$/.test(text))).toBe(false);

		await mounted.dispose();
	});

	it("draws nothing for an onAltScreen block even when mode is 'always'", async () => {
		settingsStore.setBlockTimestampMode("always");
		terminalsStore.register(TERM_ID, makeTerminal({ sessionId: SESSION_ID }));
		terminalsStore.update(TERM_ID, {
			commandBlocks: [makeBlock({ promptLine: 2, startedAt: Date.now() - 5000, onAltScreen: true })],
		});
		const mounted = await mountAndPaint();

		const ctx = overlayCtx(mounted.container, env.contexts);
		const textCalls = vi.mocked(ctx.fillText).mock.calls;
		expect(textCalls.some(([text]) => typeof text === "string" && /^\d+[smhd]$/.test(text))).toBe(false);

		await mounted.dispose();
	});
});

describe("paintLinkUnderline (smart-selection right-click highlight)", () => {
	// "foo" isn't a detected link, but the built-in "iterm-word" rule (`\S+`)
	// always matches it, spanning cols 0-2 on row 0 — same span math as
	// `paintLinkUnderline`'s pre-existing hovered-link solid underline.
	const EXPECTED_Y = 0 * FIXED_CELL_METRICS.cellHeight + FIXED_CELL_METRICS.cellHeight - 1 + 0.5;
	const EXPECTED_X0 = 0;
	const EXPECTED_X1 = 3 * FIXED_CELL_METRICS.cellWidth;

	it("draws a solid underline under the smart-match span while its context menu is open", async () => {
		terminalsStore.register(TERM_ID, makeTerminal({ sessionId: SESSION_ID }));
		const mounted = await mountAndPaint(["foo bar baz", "row1", "row2", "row3", "row4"]);
		const ctx = overlayCtx(mounted.container, env.contexts);

		fireEvent.contextMenu(mounted.canvas, cellPoint(1, 0)); // inside "foo"
		await waitFor(() => expect(within(mounted.container).getByText("Copy")).toBeTruthy());

		await waitFor(() => {
			const moveToCalls = vi.mocked(ctx.moveTo).mock.calls;
			const lineToCalls = vi.mocked(ctx.lineTo).mock.calls;
			expect(moveToCalls.some(([x, y]) => x === EXPECTED_X0 && y === EXPECTED_Y)).toBe(true);
			expect(lineToCalls.some(([x, y]) => x === EXPECTED_X1 && y === EXPECTED_Y)).toBe(true);
		});

		await mounted.dispose();
	});

	it("stops drawing the underline once the context menu closes", async () => {
		terminalsStore.register(TERM_ID, makeTerminal({ sessionId: SESSION_ID }));
		const mounted = await mountAndPaint(["foo bar baz", "row1", "row2", "row3", "row4"]);
		const ctx = overlayCtx(mounted.container, env.contexts);

		fireEvent.contextMenu(mounted.canvas, cellPoint(1, 0)); // inside "foo"
		await waitFor(() => expect(within(mounted.container).getByText("Copy")).toBeTruthy());
		await waitFor(() => {
			expect(vi.mocked(ctx.moveTo).mock.calls.some(([x, y]) => x === EXPECTED_X0 && y === EXPECTED_Y)).toBe(true);
		});

		const clearRectCallsBeforeClose = vi.mocked(ctx.clearRect).mock.calls.length;
		fireEvent.keyDown(document, { key: "Escape" });
		await waitFor(() => expect(within(mounted.container).queryByText("Copy")).toBeNull());
		await waitFor(() =>
			expect(vi.mocked(ctx.clearRect).mock.calls.length).toBeGreaterThan(clearRectCallsBeforeClose),
		);

		// The mock records every draw call cumulatively across every repaint, so
		// "no longer drawn" must be checked against calls made AFTER the repaint
		// that followed the menu closing — not the (still-present) calls from
		// while it was open.
		const lastClearRectOrder = Math.max(...vi.mocked(ctx.clearRect).mock.invocationCallOrder);
		const moveToOrders = vi.mocked(ctx.moveTo).mock.invocationCallOrder;
		const moveToCallsAfterClose = vi.mocked(ctx.moveTo).mock.calls.filter(
			(_, i) => moveToOrders[i] > lastClearRectOrder,
		);
		expect(moveToCallsAfterClose.some(([x, y]) => x === EXPECTED_X0 && y === EXPECTED_Y)).toBe(false);

		await mounted.dispose();
	});
});
