import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CellMetrics, DecodedFrame } from "../canvasTerminalUtils";
import { ImageLayer } from "../imageLayer";

function frame(overrides: Partial<DecodedFrame> = {}): DecodedFrame {
	return {
		cursorRow: 0,
		cursorCol: 0,
		cursorVisible: true,
		cursorShape: "block",
		displayOffset: 0,
		historySize: 0,
		historyBase: 0,
		hasSelection: false,
		keyboardFlags: 0,
		altScreen: false,
		appCursor: false,
		cursorSteady: false,
		bell: false,
		mouseMode: 0,
		sgrMouse: false,
		focusReporting: false,
		bracketedPaste: false,
		screenRows: 24,
		screenCols: 80,
		rows: [],
		needsFullFrame: false,
		...overrides,
	};
}

function metrics(): CellMetrics {
	return {
		cellWidth: 9,
		cellHeight: 18,
		baseline: 14,
		dpr: 1,
		scaledCellWidth: 9,
		scaledCellHeight: 18,
	} as CellMetrics;
}

function fakeCtx() {
	return { drawImage: vi.fn() } as unknown as CanvasRenderingContext2D;
}

const PNG_BYTES = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 1, 2, 3, 4]).buffer;

describe("ImageLayer", () => {
	let fakeBitmap: ImageBitmap;

	beforeEach(() => {
		fakeBitmap = {} as ImageBitmap;
		vi.stubGlobal(
			"createImageBitmap",
			vi.fn(async () => fakeBitmap),
		);
	});

	it("draws a placement at the row the absRow/historyBase/historySize/displayOffset formula resolves to", async () => {
		const invoke = vi.fn(async (cmd: string) => {
			if (cmd === "terminal_image_bytes") return PNG_BYTES;
			return [];
		});
		const onSettled = vi.fn();
		const layer = new ImageLayer("s1", invoke, onSettled);
		layer.upsert({ placementId: 1, imageId: 7, absRow: 5, col: 2, rows: 3, cols: 4, zIndex: 0 });

		const ctx = fakeCtx();
		// First paint: bitmap not decoded yet, nothing drawn, but a load kicks off.
		layer.paint(ctx, frame(), metrics());
		expect(ctx.drawImage).not.toHaveBeenCalled();

		await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));

		layer.paint(ctx, frame(), metrics());
		expect(ctx.drawImage).toHaveBeenCalledWith(fakeBitmap, 2 * 9, 5 * 18, 4 * 9, 3 * 18);
	});

	it("shifts the drawn row as historyBase/displayOffset change, matching DecodedFrame's documented formula", async () => {
		const invoke = vi.fn(async (_cmd: string) => PNG_BYTES);
		const onSettled = vi.fn();
		const layer = new ImageLayer("s1", invoke, onSettled);
		layer.upsert({ placementId: 1, imageId: 7, absRow: 100, col: 0, rows: 1, cols: 1, zIndex: 0 });
		const ctx = fakeCtx();
		// On-screen under this frame (screenRow = 100 - 100 - 0 + 0 = 0) so the
		// first paint actually starts the bitmap load.
		layer.paint(ctx, frame({ historyBase: 100 }), metrics());
		expect(ctx.drawImage).not.toHaveBeenCalled(); // still loading

		await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));

		// screenRow = absRow - historyBase - historySize + displayOffset
		// = 100 - 40 - 70 + 10 = 0
		layer.paint(ctx, frame({ historyBase: 40, historySize: 70, displayOffset: 10 }), metrics());
		expect(ctx.drawImage).toHaveBeenCalledWith(fakeBitmap, 0, 0, 9, 18);
	});

	it("skips a placement scrolled entirely above or below the current viewport", async () => {
		const invoke = vi.fn(async (_cmd: string) => PNG_BYTES);
		const onSettled = vi.fn();
		const layer = new ImageLayer("s1", invoke, onSettled);
		// screenRow = 5 - 0 - 1000 + 0 = -995, rows=2 -> fully scrolled off the top.
		layer.upsert({ placementId: 1, imageId: 7, absRow: 5, col: 0, rows: 2, cols: 1, zIndex: 0 });
		const ctx = fakeCtx();
		const off = frame({ displayOffset: 0, historyBase: 0, historySize: 1000 });

		layer.paint(ctx, off, metrics());
		expect(ctx.drawImage).not.toHaveBeenCalled();
		// No bitmap fetch should even be attempted for an off-screen placement.
		expect(invoke.mock.calls.filter((c) => c[0] === "terminal_image_bytes")).toHaveLength(0);
	});

	it("caches a decoded bitmap across multiple placements sharing one image id", async () => {
		const invoke = vi.fn(async (cmd: string) => {
			if (cmd === "terminal_image_bytes") return PNG_BYTES;
			return [];
		});
		const onSettled = vi.fn();
		const layer = new ImageLayer("s1", invoke, onSettled);
		layer.upsert({ placementId: 1, imageId: 42, absRow: 0, col: 0, rows: 1, cols: 1, zIndex: 0 });
		layer.upsert({ placementId: 2, imageId: 42, absRow: 1, col: 0, rows: 1, cols: 1, zIndex: 0 });
		const ctx = fakeCtx();
		layer.paint(ctx, frame(), metrics());
		await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));
		layer.paint(ctx, frame(), metrics());

		const bytesCalls = invoke.mock.calls.filter((c) => c[0] === "terminal_image_bytes");
		expect(bytesCalls).toHaveLength(1);
		expect(ctx.drawImage).toHaveBeenCalledTimes(2);
	});

	it("marks an image errored (and never retries) when the bytes are empty", async () => {
		const invoke = vi.fn(async (cmd: string) => (cmd === "terminal_image_bytes" ? new ArrayBuffer(0) : []));
		const onSettled = vi.fn();
		const layer = new ImageLayer("s1", invoke, onSettled);
		layer.upsert({ placementId: 1, imageId: 9, absRow: 0, col: 0, rows: 1, cols: 1, zIndex: 0 });
		const ctx = fakeCtx();
		layer.paint(ctx, frame(), metrics());
		await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));

		layer.paint(ctx, frame(), metrics());
		layer.paint(ctx, frame(), metrics());
		expect(ctx.drawImage).not.toHaveBeenCalled();
		expect(invoke.mock.calls.filter((c) => c[0] === "terminal_image_bytes")).toHaveLength(1);
	});

	it("hydrate() populates placements from the terminal_image_placements tuple shape", async () => {
		const invoke = vi.fn(async (cmd: string) => {
			if (cmd === "terminal_image_placements") return [[1, 7, 5, 2, 3, 4, -1]];
			return PNG_BYTES;
		});
		const layer = new ImageLayer("s1", invoke, vi.fn());
		expect(layer.size).toBe(0);
		await layer.hydrate();
		expect(layer.size).toBe(1);
	});

	it("clearAndRehydrate() drops stale placements before re-fetching", async () => {
		let placementsResponse: unknown[] = [[1, 7, 0, 0, 1, 1, 0]];
		const invoke = vi.fn(async (cmd: string) => {
			if (cmd === "terminal_image_placements") return placementsResponse;
			return PNG_BYTES;
		});
		const layer = new ImageLayer("s1", invoke, vi.fn());
		await layer.hydrate();
		expect(layer.size).toBe(1);

		placementsResponse = [];
		await layer.clearAndRehydrate();
		expect(layer.size).toBe(0);
	});

	it("hydrate() tolerates a failed invoke rather than throwing", async () => {
		const invoke = vi.fn(async () => {
			throw new Error("network down");
		});
		const layer = new ImageLayer("s1", invoke, vi.fn());
		await expect(layer.hydrate()).resolves.toBeUndefined();
		expect(layer.size).toBe(0);
	});
});
