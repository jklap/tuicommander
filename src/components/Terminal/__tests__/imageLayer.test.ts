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
		layer.paintAboveText(ctx, frame(), metrics());
		expect(ctx.drawImage).not.toHaveBeenCalled();

		await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));

		layer.paintAboveText(ctx, frame(), metrics());
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
		layer.paintAboveText(ctx, frame({ historyBase: 100 }), metrics());
		expect(ctx.drawImage).not.toHaveBeenCalled(); // still loading

		await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));

		// screenRow = absRow - historyBase - historySize + displayOffset
		// = 100 - 40 - 70 + 10 = 0
		layer.paintAboveText(ctx, frame({ historyBase: 40, historySize: 70, displayOffset: 10 }), metrics());
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

		layer.paintAboveText(ctx, off, metrics());
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
		layer.paintAboveText(ctx, frame(), metrics());
		await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));
		layer.paintAboveText(ctx, frame(), metrics());

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
		layer.paintAboveText(ctx, frame(), metrics());
		await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));

		layer.paintAboveText(ctx, frame(), metrics());
		layer.paintAboveText(ctx, frame(), metrics());
		expect(ctx.drawImage).not.toHaveBeenCalled();
		expect(invoke.mock.calls.filter((c) => c[0] === "terminal_image_bytes")).toHaveLength(1);
	});

	// color-tools plan: Kitty decode deferred off the vt_log lock means a
	// placement can arrive (and a first paint attempt fire) before the
	// backend's decode job has actually finished — a fetch landing in that
	// window sees empty bytes, same as the permanent-failure case above.
	// `invalidateImage` (driven by the backend's `image-decoded` signal, once
	// the real bytes are ready) is what turns that into a retry instead of a
	// second permanent failure.
	it("invalidateImage clears a failed fetch and the next paint attempt retries", async () => {
		let bytesReady = false;
		const invoke = vi.fn(async (cmd: string) => {
			if (cmd !== "terminal_image_bytes") return [];
			return bytesReady ? PNG_BYTES : new ArrayBuffer(0);
		});
		const onSettled = vi.fn();
		const layer = new ImageLayer("s1", invoke, onSettled);
		layer.upsert({ placementId: 1, imageId: 9, absRow: 0, col: 0, rows: 1, cols: 1, zIndex: 0 });
		const ctx = fakeCtx();

		// First attempt races the still-in-flight decode: empty bytes.
		layer.paintAboveText(ctx, frame(), metrics());
		await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));
		layer.paintAboveText(ctx, frame(), metrics());
		expect(ctx.drawImage).not.toHaveBeenCalled();

		// Decode actually finishes; the backend signals it.
		bytesReady = true;
		layer.invalidateImage(9);
		layer.paintAboveText(ctx, frame(), metrics());
		expect(ctx.drawImage).not.toHaveBeenCalled(); // loading again
		await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(2));

		layer.paintAboveText(ctx, frame(), metrics());
		expect(ctx.drawImage).toHaveBeenCalledTimes(1);
		expect(invoke.mock.calls.filter((c) => c[0] === "terminal_image_bytes")).toHaveLength(2);
	});

	it("invalidateImage on an image with no cache entry is a harmless no-op", () => {
		const invoke = vi.fn(async (_cmd: string) => []);
		const layer = new ImageLayer("s1", invoke, vi.fn());
		expect(() => layer.invalidateImage(999)).not.toThrow();
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

describe("ImageLayer z-band split (full z-order compositing)", () => {
	it("hasNegativeZ reflects only currently-known placements", () => {
		const layer = new ImageLayer("s1", vi.fn(), vi.fn());
		expect(layer.hasNegativeZ()).toBe(false);
		layer.upsert({ placementId: 1, imageId: 1, absRow: 0, col: 0, rows: 1, cols: 1, zIndex: 0 });
		expect(layer.hasNegativeZ()).toBe(false);
		layer.upsert({ placementId: 2, imageId: 1, absRow: 0, col: 0, rows: 1, cols: 1, zIndex: -1 });
		expect(layer.hasNegativeZ()).toBe(true);
	});

	it("paintAboveText and paintBelowText each draw only their own band", async () => {
		const fakeBitmap = {} as ImageBitmap;
		vi.stubGlobal(
			"createImageBitmap",
			vi.fn(async () => fakeBitmap),
		);
		const invoke = vi.fn(async (_cmd: string) => PNG_BYTES);
		const onSettled = vi.fn();
		const layer = new ImageLayer("s1", invoke, onSettled);
		layer.upsert({ placementId: 1, imageId: 1, absRow: 0, col: 0, rows: 1, cols: 1, zIndex: 0 });
		layer.upsert({ placementId: 2, imageId: 2, absRow: 1, col: 0, rows: 1, cols: 1, zIndex: -1 });

		const above = fakeCtx();
		const below = fakeCtx();
		layer.paintAboveText(above, frame(), metrics());
		layer.paintBelowText(below, frame(), metrics());
		await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(2));

		layer.paintAboveText(above, frame(), metrics());
		layer.paintBelowText(below, frame(), metrics());
		expect(above.drawImage).toHaveBeenCalledTimes(1);
		expect(above.drawImage).toHaveBeenCalledWith(fakeBitmap, 0, 0, 9, 18);
		expect(below.drawImage).toHaveBeenCalledTimes(1);
		expect(below.drawImage).toHaveBeenCalledWith(fakeBitmap, 0, 18, 9, 18);
	});
});

describe("ImageLayer.verifyOverlapping (overwrite detection)", () => {
	it("drops a placement whose top-left cell no longer matches", async () => {
		const invoke = vi.fn(async (cmd: string) => {
			if (cmd === "terminal_image_ref_at") return null; // cell no longer shows this image
			return PNG_BYTES;
		});
		const layer = new ImageLayer("s1", invoke, vi.fn());
		layer.upsert({ placementId: 1, imageId: 7, absRow: 5, col: 2, rows: 2, cols: 2, zIndex: 0 });

		const changed = await layer.verifyOverlapping(new Set([5, 6]), frame());
		expect(changed).toBe(true);
		expect(layer.size).toBe(0);
	});

	it("keeps a placement whose top-left cell still matches", async () => {
		const invoke = vi.fn(async (cmd: string) => {
			if (cmd === "terminal_image_ref_at") return [7, 1, 0, 0, 0];
			return PNG_BYTES;
		});
		const layer = new ImageLayer("s1", invoke, vi.fn());
		layer.upsert({ placementId: 1, imageId: 7, absRow: 5, col: 2, rows: 2, cols: 2, zIndex: 0 });

		const changed = await layer.verifyOverlapping(new Set([5]), frame());
		expect(changed).toBe(false);
		expect(layer.size).toBe(1);
	});

	it("ignores a dirty row that doesn't overlap any placement", async () => {
		const invoke = vi.fn(async (_cmd: string) => PNG_BYTES);
		const layer = new ImageLayer("s1", invoke, vi.fn());
		layer.upsert({ placementId: 1, imageId: 7, absRow: 5, col: 2, rows: 2, cols: 2, zIndex: 0 });

		const changed = await layer.verifyOverlapping(new Set([50]), frame());
		expect(changed).toBe(false);
		expect(layer.size).toBe(1);
		expect(invoke.mock.calls.filter((c) => c[0] === "terminal_image_ref_at")).toHaveLength(0);
	});

	it("skips verification for a placement scrolled off-screen (can't check, so leaves it alone)", async () => {
		const invoke = vi.fn(async (_cmd: string) => PNG_BYTES);
		const layer = new ImageLayer("s1", invoke, vi.fn());
		layer.upsert({ placementId: 1, imageId: 7, absRow: 5, col: 2, rows: 2, cols: 2, zIndex: 0 });

		// historyBase huge -> screenRow is deeply negative (off-screen).
		const changed = await layer.verifyOverlapping(new Set([5]), frame({ historyBase: 1000 }));
		expect(changed).toBe(false);
		expect(layer.size).toBe(1);
		expect(invoke.mock.calls.filter((c) => c[0] === "terminal_image_ref_at")).toHaveLength(0);
	});

	it("fails open (keeps the placement) if the verification call itself throws", async () => {
		const invoke = vi.fn(async (cmd: string) => {
			if (cmd === "terminal_image_ref_at") throw new Error("network down");
			return PNG_BYTES;
		});
		const layer = new ImageLayer("s1", invoke, vi.fn());
		layer.upsert({ placementId: 1, imageId: 7, absRow: 5, col: 2, rows: 2, cols: 2, zIndex: 0 });

		const changed = await layer.verifyOverlapping(new Set([5]), frame());
		expect(changed).toBe(false);
		expect(layer.size).toBe(1);
	});
});

describe("ImageLayer raw pixel format decoding", () => {
	it("reconstructs a raw f=24 (RGB) payload via terminal_image_meta", async () => {
		const rawRgb = new Uint8Array([255, 0, 0, 0, 255, 0]).buffer; // 2x1 px RGB
		const invoke = vi.fn(async (cmd: string) => {
			if (cmd === "terminal_image_bytes") return rawRgb;
			if (cmd === "terminal_image_meta") return ["raw-rgb", 2, 1];
			return [];
		});
		let capturedImageData: ImageData | undefined;
		vi.stubGlobal(
			"createImageBitmap",
			vi.fn(async (arg: ImageData) => {
				capturedImageData = arg;
				return {} as ImageBitmap;
			}),
		);
		const onSettled = vi.fn();
		const layer = new ImageLayer("s1", invoke, onSettled);
		layer.upsert({ placementId: 1, imageId: 1, absRow: 0, col: 0, rows: 1, cols: 2, zIndex: 0 });
		layer.paintAboveText(fakeCtx(), frame(), metrics());
		await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));

		expect(capturedImageData?.width).toBe(2);
		expect(capturedImageData?.height).toBe(1);
		// RGB expanded to RGBA, alpha forced opaque.
		expect(Array.from(capturedImageData?.data ?? [])).toEqual([255, 0, 0, 255, 0, 255, 0, 255]);
	});

	it("marks a raw payload shorter than width*height*channels as errored, not corrupted", async () => {
		const tooShort = new Uint8Array([1, 2, 3]).buffer; // 1 byte short of 2x1 RGB
		const invoke = vi.fn(async (cmd: string) => {
			if (cmd === "terminal_image_bytes") return tooShort;
			if (cmd === "terminal_image_meta") return ["raw-rgb", 2, 1];
			return [];
		});
		const onSettled = vi.fn();
		const layer = new ImageLayer("s1", invoke, onSettled);
		layer.upsert({ placementId: 1, imageId: 1, absRow: 0, col: 0, rows: 1, cols: 2, zIndex: 0 });
		const ctx = fakeCtx();
		layer.paintAboveText(ctx, frame(), metrics());
		await vi.waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));

		layer.paintAboveText(ctx, frame(), metrics());
		expect(ctx.drawImage).not.toHaveBeenCalled();
	});
});
