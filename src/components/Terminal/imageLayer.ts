// Inline-image rendering (iTerm2 OSC 1337 / Kitty graphics protocol) —
// color-tools plan, Phase 5.
//
// Scope of this pass: placements at any z-index paint on a single canvas
// layered ABOVE the glyph/background canvas and BELOW the cursor/selection
// overlay — i.e. images always occlude text underneath them, matching
// iTerm2 exactly (which has no z-index concept) and Kitty's default `z=0`.
// Kitty's `z<0` "paint below text" bands are NOT implemented here: doing so
// correctly requires splitting `gridRenderer`'s fused background+glyph paint
// into two separately-paintable layers so an image can be sandwiched between
// them, which is real, separate work (see `to-test.md`). `z<0` placements
// still render in this single layer today — visually wrong only when a
// placement overlaps cells that also carry real text, which no in-scope
// target tool's common usage does.
//
// Decoding is limited to formats the browser's `createImageBitmap` can
// decode directly from bytes: PNG, GIF, JPEG (everything `imgcat`/`imgls`/
// `divider` and Kitty's default `f=100` produce). Kitty's raw `f=24`/`f=32`
// formats (no container, used by mpv/blackcat) need the placement's pixel
// dimensions to interpret the bytes at all, which today's
// `terminal_image_bytes` response does not carry — deferred, documented in
// `to-test.md`, not silently dropped.

import { toBinaryPayload } from "./canvasTerminalTransport";
import type { CellMetrics, DecodedFrame } from "./canvasTerminalUtils";

export interface ImagePlacement {
	placementId: number;
	imageId: number;
	/** Eviction-stable absolute row — see `DecodedFrame.historyBase`'s doc
	 * comment for the formula converting this to an on-screen row. */
	absRow: number;
	col: number;
	rows: number;
	cols: number;
	zIndex: number;
}

type BitmapState = { status: "loading" } | { status: "ready"; bitmap: ImageBitmap } | { status: "error" };

/** One raw `[placementId, imageId, absRow, col, rows, cols, zIndex]` tuple,
 * the wire shape both `terminal_image_placements` (Tauri) and its HTTP twin
 * return identically (color-tools plan's IPC/HTTP parity rule). */
type PlacementTuple = [number, number, number, number, number, number, number];

function tupleToPlacement(t: PlacementTuple): ImagePlacement {
	return {
		placementId: t[0],
		imageId: t[1],
		absRow: t[2],
		col: t[3],
		rows: t[4],
		cols: t[5],
		zIndex: t[6],
	};
}

/** Per-terminal-instance state: placements currently known, and the decoded
 * `ImageBitmap` cache (keyed by image id, shared across every placement that
 * repeats the same image). Not a module singleton — one per `CanvasTerminal`
 * mount, same lifetime as its own row cache. */
export class ImageLayer {
	private placements = new Map<number, ImagePlacement>();
	private bitmaps = new Map<number, BitmapState>();

	constructor(
		private readonly sessionId: string,
		private readonly invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>,
		/** Called once a bitmap that was pending finishes loading (success or
		 * failure), so the caller can trigger a repaint — decode is always
		 * async, so the first paint attempt after a new placement arrives
		 * will usually find nothing to draw yet. */
		private readonly onBitmapSettled: () => void,
	) {}

	/** A new/updated placement arrived over the live event stream. */
	upsert(p: ImagePlacement): void {
		this.placements.set(p.placementId, p);
	}

	/** Every previously-known placement is gone (alt-screen switch, `a=d`);
	 * re-fetch the authoritative current set. */
	async clearAndRehydrate(): Promise<void> {
		this.placements.clear();
		await this.hydrate();
	}

	/** Reconnect / first-mount hydration: fetch every placement that exists
	 * right now, since the live event stream only carries new placements
	 * from the moment a listener attaches. */
	async hydrate(): Promise<void> {
		const raw = (await this.invoke("terminal_image_placements", { sessionId: this.sessionId }).catch(() => [])) as
			| PlacementTuple[]
			| null
			| undefined;
		if (!Array.isArray(raw)) return;
		for (const t of raw) {
			const p = tupleToPlacement(t);
			this.placements.set(p.placementId, p);
		}
	}

	get size(): number {
		return this.placements.size;
	}

	private getBitmapIfReady(imageId: number): ImageBitmap | null {
		const state = this.bitmaps.get(imageId);
		if (state?.status === "ready") return state.bitmap;
		if (state) return null; // loading or errored — nothing to draw
		this.bitmaps.set(imageId, { status: "loading" });
		this.load(imageId);
		return null;
	}

	private async load(imageId: number): Promise<void> {
		try {
			const raw = await this.invoke("terminal_image_bytes", { sessionId: this.sessionId, imageId });
			const buf = toBinaryPayload(raw);
			if (!buf || buf.byteLength === 0) {
				this.bitmaps.set(imageId, { status: "error" });
				return;
			}
			const bitmap = await createImageBitmap(new Blob([buf]));
			this.bitmaps.set(imageId, { status: "ready", bitmap });
		} catch {
			// Raw f=24/f=32 pixel data (no container) lands here today — see
			// the module doc comment. Not retried: the bytes never change.
			this.bitmaps.set(imageId, { status: "error" });
		} finally {
			this.onBitmapSettled();
		}
	}

	/** Paint every currently-known, on-screen placement into `ctx`. `ctx`
	 * must already be scaled/translated exactly like the base grid canvas
	 * (dpr scale + gutter translate) — the caller owns that setup. */
	paint(ctx: CanvasRenderingContext2D, frame: DecodedFrame, m: CellMetrics): void {
		if (this.placements.size === 0) return;
		for (const p of this.placements.values()) {
			const screenRow = p.absRow - frame.historyBase - frame.historySize + frame.displayOffset;
			if (screenRow + p.rows <= 0 || screenRow >= frame.screenRows) continue;
			const bitmap = this.getBitmapIfReady(p.imageId);
			if (!bitmap) continue;
			ctx.drawImage(bitmap, p.col * m.cellWidth, screenRow * m.cellHeight, p.cols * m.cellWidth, p.rows * m.cellHeight);
		}
	}
}
