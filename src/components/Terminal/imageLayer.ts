// Inline-image rendering (iTerm2 OSC 1337 / Kitty graphics protocol) —
// color-tools plan, Phase 5 (+ z-order-compositing and raw-format follow-up).
//
// Z-order: placements paint into one of two bands — "below text" (any
// zIndex < 0, e.g. image.nvim's z=-1) or "above text" (zIndex >= 0, iTerm2
// always and Kitty by default) — via `paintBelowText`/`paintAboveText`,
// each targeting its own canvas layer so an image can be sandwiched between
// the background-fill and glyph-fill passes. `CanvasTerminal.tsx` only pays
// for the extra below-text canvas + the split background/glyph rendering
// once a session has ever registered a negative-zIndex placement (see
// `hasNegativeZ`); until then it stays on the original single fused canvas.
// Kitty's most-extreme `z < INT32_MIN/2` band ("below even a non-default
// background") isn't given separate treatment — it folds into the same
// below-text band as ordinary z<0 — since no in-scope target tool uses it.
//
// Decoding: PNG/GIF/JPEG (everything `imgcat`/`imgls`/`divider` and Kitty's
// default `f=100` produce) decode via the browser's `createImageBitmap`.
// Kitty's raw `f=24`/`f=32` formats (no container, used by mpv/blackcat) are
// reconstructed manually from `terminal_image_meta`'s width/height/mime,
// since `createImageBitmap` can't sniff dimensions from headerless pixel
// data the way it can for a real image container.
//
// Overwrite detection: a placement's cells being overwritten by ordinary
// text (not an explicit Kitty `a=d` or an alt-screen switch, both of which
// already notify this layer) has no dedicated wire signal — there's no
// spare bit in the grid frame for "this cell's image ref was just dropped".
// Instead, `verifyOverlapping` re-checks a placement's top-left cell via the
// existing single-cell `terminal_image_ref_at` lookup whenever an ordinary
// dirty-row update touches one of its rows, dropping it if that cell no
// longer reports the same image/placement. This is a heuristic, not a
// precise per-cell diff: a partial overwrite of only SOME of a placement's
// cells (its top-left cell surviving unchanged) is not caught. Full
// precision would need a wire-format change (a per-cell "image ref changed"
// signal); this catches the common cases — the whole placement cleared,
// scrolled over, or replaced — cheaply, with existing infrastructure.

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

/** `[mime, intrinsicWidth, intrinsicHeight]` — `terminal_image_meta`'s wire
 * shape, identical on both transports. */
type MetaTuple = [string, number, number];

/** Convert a raw (headerless) Kitty `f=24`/`f=32` pixel buffer into a real
 * `ImageBitmap` via the canvas `ImageData` path — the only formats
 * `createImageBitmap(Blob)` cannot decode unaided, since there is no
 * container to sniff width/height/channel-count from. Returns `null` on any
 * shape mismatch (e.g. a truncated raw payload) rather than throwing. */
async function decodeRawPixels(
	bytes: Uint8Array,
	mime: string,
	width: number,
	height: number,
): Promise<ImageBitmap | null> {
	if (width <= 0 || height <= 0) return null;
	const channels = mime === "raw-rgba" ? 4 : mime === "raw-rgb" ? 3 : 0;
	if (channels === 0) return null;
	const expected = width * height * channels;
	if (bytes.byteLength < expected) return null;

	const rgba = new Uint8ClampedArray(width * height * 4);
	if (channels === 4) {
		rgba.set(bytes.subarray(0, expected));
	} else {
		// RGB -> RGBA: expand, alpha opaque.
		for (let px = 0; px < width * height; px++) {
			rgba[px * 4] = bytes[px * 3];
			rgba[px * 4 + 1] = bytes[px * 3 + 1];
			rgba[px * 4 + 2] = bytes[px * 3 + 2];
			rgba[px * 4 + 3] = 255;
		}
	}
	const imageData = new ImageData(rgba, width, height);
	return createImageBitmap(imageData);
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

	/** The backend finished deferred-decoding this image (color-tools plan —
	 * Kitty decode moved off the `vt_log` lock, so a placement can be known
	 * before its bytes are ready). `bitmaps` never retries a `"loading"`/
	 * `"error"` entry on its own (see `getBitmapIfReady`), so if this
	 * image's first paint-triggered fetch happened to race the still-
	 * in-flight decode — plausible under IPC/network jitter even though the
	 * window is normally tiny — it would otherwise be marked permanently
	 * failed. Dropping the cache entry here (only if one exists; a fetch
	 * that hasn't started yet needs no help) makes the next paint attempt
	 * try again now that bytes actually exist. Never called for a decode
	 * that ultimately failed — the existing "no bytes, don't retry" default
	 * already matches that outcome, so no signal is needed for it. */
	invalidateImage(imageId: number): void {
		this.bitmaps.delete(imageId);
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

	/** Whether any currently-known placement uses `z<0` — the signal
	 * `CanvasTerminal.tsx` uses to switch (once, one-way) from the fused
	 * single-canvas fast path to full below/above-text split compositing. */
	hasNegativeZ(): boolean {
		for (const p of this.placements.values()) {
			if (p.zIndex < 0) return true;
		}
		return false;
	}

	/** Re-verify placements whose row range overlaps an ordinary dirty-row
	 * update, dropping any whose top-left cell no longer reports the same
	 * image/placement (see the module doc comment's Overwrite Detection
	 * section for what this does and doesn't catch). Returns whether
	 * anything was actually dropped, so the caller knows whether a repaint
	 * is worth triggering. `dirtyAbsRows` and `frame` let the caller pass
	 * screen-relative dirty row indices already converted to the same
	 * eviction-stable absolute space `absRow` uses. */
	async verifyOverlapping(dirtyAbsRows: ReadonlySet<number>, frame: DecodedFrame): Promise<boolean> {
		if (this.placements.size === 0 || dirtyAbsRows.size === 0) return false;
		let changed = false;
		for (const p of Array.from(this.placements.values())) {
			let overlaps = false;
			for (let r = p.absRow; r < p.absRow + p.rows; r++) {
				if (dirtyAbsRows.has(r)) {
					overlaps = true;
					break;
				}
			}
			if (!overlaps) continue;

			const screenRow = p.absRow - frame.historyBase - frame.historySize + frame.displayOffset;
			if (screenRow < 0 || screenRow >= frame.screenRows) continue; // scrolled off-screen — can't verify, leave as-is

			// Sequential is fine here — an overwrite-triggered recheck touches
			// only the small number of placements whose rows overlapped this
			// update's dirty rows, rarely more than one or two.
			const stillValid = await this.topLeftStillMatches(p, screenRow);
			if (!stillValid) {
				this.placements.delete(p.placementId);
				changed = true;
			}
		}
		return changed;
	}

	private async topLeftStillMatches(p: ImagePlacement, screenRow: number): Promise<boolean> {
		try {
			const raw = await this.invoke("terminal_image_ref_at", { sessionId: this.sessionId, row: screenRow, col: p.col });
			if (!Array.isArray(raw)) return false;
			const [imageId, placementId] = raw as [number, number, number, number, number];
			return imageId === p.imageId && placementId === p.placementId;
		} catch {
			return true; // fail open: a failed check must not drop a real placement
		}
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

			const bitmap = await this.decode(imageId, buf);
			if (!bitmap) {
				this.bitmaps.set(imageId, { status: "error" });
				return;
			}
			this.bitmaps.set(imageId, { status: "ready", bitmap });
		} catch {
			this.bitmaps.set(imageId, { status: "error" });
		} finally {
			this.onBitmapSettled();
		}
	}

	private async decode(imageId: number, buf: ArrayBuffer): Promise<ImageBitmap | null> {
		const meta = (await this.invoke("terminal_image_meta", { sessionId: this.sessionId, imageId }).catch(
			() => null,
		)) as MetaTuple | null;
		const mime = meta?.[0];
		if (mime === "raw-rgb" || mime === "raw-rgba") {
			return decodeRawPixels(new Uint8Array(buf), mime, meta?.[1] ?? 0, meta?.[2] ?? 0);
		}
		// A real container (PNG/GIF/JPEG) — let the browser decode it
		// directly. Also the fallback when meta itself couldn't be fetched:
		// most real payloads are PNG, so attempting the container path is
		// more useful than giving up outright.
		try {
			return await createImageBitmap(new Blob([buf]));
		} catch {
			return null;
		}
	}

	private paintFiltered(
		ctx: CanvasRenderingContext2D,
		frame: DecodedFrame,
		m: CellMetrics,
		include: (zIndex: number) => boolean,
	): void {
		if (this.placements.size === 0) return;
		for (const p of this.placements.values()) {
			if (!include(p.zIndex)) continue;
			const screenRow = p.absRow - frame.historyBase - frame.historySize + frame.displayOffset;
			if (screenRow + p.rows <= 0 || screenRow >= frame.screenRows) continue;
			const bitmap = this.getBitmapIfReady(p.imageId);
			if (!bitmap) continue;
			ctx.drawImage(bitmap, p.col * m.cellWidth, screenRow * m.cellHeight, p.cols * m.cellWidth, p.rows * m.cellHeight);
		}
	}

	/** Paint every on-screen placement with `zIndex >= 0` — iTerm2 always,
	 * Kitty by default. Always safe to call regardless of compositing mode:
	 * with no negative-z placements this is every placement there is. */
	paintAboveText(ctx: CanvasRenderingContext2D, frame: DecodedFrame, m: CellMetrics): void {
		this.paintFiltered(ctx, frame, m, (z) => z >= 0);
	}

	/** Paint every on-screen placement with `zIndex < 0` (Kitty only — image
	 * below text). Only meaningful once `CanvasTerminal.tsx` has switched
	 * this session into split-compositing mode; a no-op otherwise since
	 * `hasNegativeZ()` would already be false. */
	paintBelowText(ctx: CanvasRenderingContext2D, frame: DecodedFrame, m: CellMetrics): void {
		this.paintFiltered(ctx, frame, m, (z) => z < 0);
	}
}
