import { render, waitFor } from "@solidjs/testing-library";
import { vi } from "vitest";
import CanvasTerminal, { type CanvasTerminalProps, type CanvasTerminalRef } from "../../CanvasTerminal";
import type { TerminalTransport } from "../../canvasTerminalTransport";
import type { CellMetrics } from "../../canvasTerminalUtils";

/**
 * Mount harness for CanvasTerminal — the component has never been rendered in
 * a test before this (grep for `@solidjs/testing-library` + `CanvasTerminal`
 * turns up nothing). It has real DOM dependencies (canvas 2D context, layout
 * rects, webfont loading) that happy-dom doesn't implement, plus a real
 * transport that would try to reach Tauri IPC. This module stubs exactly
 * those seams so the component mounts deterministically; everything else
 * (SolidJS reactivity, the real mouse-gesture code under test, the real
 * settingsStore) runs unmodified.
 *
 * Usage in a test file (transport + glyphCache MUST be mocked by the test
 * file itself via `vi.mock`, hoisted per-file — see
 * `canvasTerminalMount.test.ts` for the canonical setup):
 *
 *   const fakeTransport = createFakeTransport();
 *   vi.mock("../../canvasTerminalTransport", () => ({
 *     createTransport: () => fakeTransport,
 *   }));
 *   vi.mock("../../glyphCache", () => ({
 *     getSharedMetrics: () => FIXED_CELL_METRICS,
 *     acquireCache: vi.fn(),
 *     releaseCache: vi.fn(),
 *     invalidateGlyphCache: vi.fn(),
 *   }));
 */

export const FIXED_CELL_METRICS: CellMetrics = {
	cellWidth: 10,
	cellHeight: 20,
	baseline: 15,
	fontSize: 14,
	dpr: 1,
	scaledCellWidth: 10,
	scaledCellHeight: 20,
};

/** Fixed layout rect returned for every element's getBoundingClientRect() —
 *  happy-dom always returns an all-zero rect, which starves `remeasure()`
 *  (it bails out early when width/height <= 0). Wide/tall enough to hold a
 *  multi-row, multi-column fixture at FIXED_CELL_METRICS. */
export const MOUNT_RECT = { left: 0, top: 0, width: 800, height: 600, right: 800, bottom: 600, x: 0, y: 0 };

function fixedRect(): DOMRect {
	return { ...MOUNT_RECT, toJSON: () => MOUNT_RECT } as DOMRect;
}

/** Recording 2D context double covering every method CanvasTerminal and
 *  gridRenderer.ts call on ctx/octx/octxOverscan (verified by grepping both
 *  files for `ctx.<method>` — see the Phase 0 plan). `canvas` is the real
 *  backing element so `ctx.canvas.width/height` (gridRenderer.ts:1328/1331)
 *  reflect whatever `remeasure()` assigned. */
export function createMockCtx2D(canvas: HTMLCanvasElement): CanvasRenderingContext2D {
	return {
		canvas,
		fillStyle: "",
		strokeStyle: "",
		font: "",
		lineWidth: 1,
		globalAlpha: 1,
		fillRect: vi.fn(),
		clearRect: vi.fn(),
		fillText: vi.fn(),
		strokeRect: vi.fn(),
		beginPath: vi.fn(),
		closePath: vi.fn(),
		moveTo: vi.fn(),
		lineTo: vi.fn(),
		arc: vi.fn(),
		quadraticCurveTo: vi.fn(),
		stroke: vi.fn(),
		fill: vi.fn(),
		scale: vi.fn(),
		translate: vi.fn(),
		setTransform: vi.fn(),
		setLineDash: vi.fn(),
		measureText: vi.fn(() => ({ width: 10 }) as TextMetrics),
	} as unknown as CanvasRenderingContext2D;
}

/**
 * Patch the DOM/canvas seams CanvasTerminal's onMount needs, that happy-dom
 * doesn't provide (document.fonts) or only provides as all-zero (layout
 * rects). Returns a restore function — call it in `afterEach`.
 */
export function stubCanvasEnvironment(): () => void {
	const originalGetContext = HTMLCanvasElement.prototype.getContext;
	const originalGetRect = Element.prototype.getBoundingClientRect;
	const hadFonts = "fonts" in document;
	const originalFonts = (document as unknown as { fonts?: unknown }).fonts;

	HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement, type: string) {
		if (type === "2d") return createMockCtx2D(this);
		return null;
	} as typeof HTMLCanvasElement.prototype.getContext;

	Element.prototype.getBoundingClientRect = fixedRect;

	Object.defineProperty(document, "fonts", {
		value: { load: vi.fn().mockResolvedValue(undefined), ready: Promise.resolve() },
		configurable: true,
	});

	return () => {
		HTMLCanvasElement.prototype.getContext = originalGetContext;
		Element.prototype.getBoundingClientRect = originalGetRect;
		if (hadFonts) {
			Object.defineProperty(document, "fonts", { value: originalFonts, configurable: true });
		} else {
			delete (document as unknown as { fonts?: unknown }).fonts;
		}
	};
}

export interface FakeTransport extends TerminalTransport {
	/** Feed a binary frame to the component as if it arrived over the wire
	 *  (Tauri channel or WS) — build bytes with `frameFixture.ts`. */
	pushFrame: (data: ArrayBuffer) => void;
	/** Every invoke() call the component made, in order. */
	invokeCalls: { cmd: string; args: Record<string, unknown> }[];
	/** Every ackFrame() call the component made, in order. */
	ackCalls: number[];
	/** Fire a named transport event (e.g. "cwd", "osc133", "output") to a
	 *  handler registered via onEvent(), if any. */
	emitEvent: (type: string, payload: unknown) => void;
	/** Install a canned response for one invoke() command — e.g.
	 *  `setInvokeHandler("terminal_get_row_text", () => "foo bar baz")` so the
	 *  async link-hover pipeline (checkLinksAtRow) has real row text to run
	 *  its regexes against instead of `undefined`. */
	setInvokeHandler: (cmd: string, handler: (args: Record<string, unknown>) => unknown) => void;
}

/** A `TerminalTransport` double: no IPC, no network. `subscribe` captures the
 *  component's onFrame callback so the test can drive frames synchronously. */
export function createFakeTransport(): FakeTransport {
	let onFrameHandler: ((data: ArrayBuffer) => void) | null = null;
	const eventHandlers = new Map<string, (payload: unknown) => void>();
	const invokeHandlers = new Map<string, (args: Record<string, unknown>) => unknown>();
	const invokeCalls: { cmd: string; args: Record<string, unknown> }[] = [];
	const ackCalls: number[] = [];

	return {
		invokeCalls,
		ackCalls,
		async subscribe(onFrame) {
			onFrameHandler = onFrame;
		},
		async resubscribe() {},
		unsubscribe() {
			onFrameHandler = null;
		},
		async invoke(cmd, args) {
			invokeCalls.push({ cmd, args });
			return invokeHandlers.get(cmd)?.(args);
		},
		ackFrame(received) {
			ackCalls.push(received);
		},
		async onEvent(type, handler) {
			eventHandlers.set(type, handler);
		},
		pushFrame(data: ArrayBuffer) {
			onFrameHandler?.(data);
		},
		emitEvent(type, payload) {
			eventHandlers.get(type)?.(payload);
		},
		setInvokeHandler(cmd, handler) {
			invokeHandlers.set(cmd, handler);
		},
	};
}

export interface MountedCanvasTerminal {
	container: HTMLElement;
	canvas: HTMLCanvasElement;
	ref: CanvasTerminalRef;
	/** Flushes one animation frame (so a pending `scheduleRepaint()` rAF fires
	 *  and clears its id) before unmounting — otherwise onCleanup's
	 *  `cancelAnimationFrame(rafId)` trips happy-dom's async-task leak
	 *  detector (`vitest.config.ts`'s `detectAsyncLeaks`), which per
	 *  `src/__tests__/setup.ts` can misattribute the leak to an unrelated
	 *  later test. */
	dispose: () => Promise<void>;
}

/**
 * Render a real CanvasTerminal and wait for its async onMount to settle
 * (font "load" + transport.subscribe both resolve on the same microtask
 * queue this awaits). Requires `stubCanvasEnvironment()` and the module-level
 * transport/glyphCache mocks described in this file's docblock to already be
 * in effect.
 */
export async function mountCanvasTerminal(
	props: Partial<CanvasTerminalProps> & { sessionId: string; terminalId: string },
): Promise<MountedCanvasTerminal> {
	let capturedRef: CanvasTerminalRef | undefined;
	const { container, unmount } = render(() => <CanvasTerminal {...props} onRef={(r) => (capturedRef = r)} />);
	// onMount is one long async function (font load, transport.subscribe, three
	// transport.onEvent awaits, then onRef) — wait for it to actually reach the
	// end rather than guessing a fixed number of microtask ticks.
	await waitFor(() => {
		if (!capturedRef) throw new Error("onRef not fired yet");
	});

	// Three <canvas> elements are rendered (overscan, base/interactive, overlay);
	// only the interactive one — the one mousedown/click/contextmenu listeners
	// are bound to — carries tabIndex={0}. The other two are pointer-events:none
	// overlays and must NOT be the dispatch target.
	const canvas = container.querySelector('canvas[tabindex="0"]') as HTMLCanvasElement;
	if (!canvas) throw new Error("mountCanvasTerminal: no interactive <canvas> found after mount");

	const dispose = async () => {
		await new Promise<void>((r) => requestAnimationFrame(() => r()));
		unmount();
	};

	return { container, canvas, ref: capturedRef!, dispose };
}
