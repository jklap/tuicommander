import { describe, expect, it } from "vitest";

import {
	ATTR_BOLD,
	ATTR_DEFAULT_BG,
	ATTR_DEFAULT_FG,
	ATTR_INVERSE,
	ATTR_ITALIC,
	ATTR_UNDERLINE,
	type CellMetrics,
	type DecodedRow,
} from "../canvasTerminalUtils";
import { createGridRenderer } from "../gridRenderer";

// resolveFg/resolveBg/buildFontStyle never touch the 2D context, so a stub ctx
// is fine — these tests lock the pure color/font logic that was moved out of
// CanvasTerminal (pixel paint parity is verified live, not here).
function makeRenderer(fontWeight: number | string = 400) {
	const ctx = {} as unknown as CanvasRenderingContext2D;
	const gr = createGridRenderer(ctx, { fontWeight: () => fontWeight, getFontFamily: () => "monospace" });
	gr.setTheme(DEF_BG, DEF_FG);
	return gr;
}

const DEF_BG = "#101010";
const DEF_FG = "#eeeeee";
const RED = 0xff0000; // packed r<<16|g<<8|b
const BLUE = 0x0000ff;

describe("gridRenderer color resolution", () => {
	it("returns the default fg/bg when the default-attr bit is set", () => {
		const gr = makeRenderer();
		expect(gr.resolveFg(RED, BLUE, ATTR_DEFAULT_FG)).toBe(DEF_FG);
		expect(gr.resolveBg(RED, BLUE, ATTR_DEFAULT_BG)).toBe(DEF_BG);
	});

	it("returns explicit packed colors as rgb() strings", () => {
		const gr = makeRenderer();
		expect(gr.resolveFg(RED, BLUE, 0)).toBe("rgb(255,0,0)");
		expect(gr.resolveBg(RED, BLUE, 0)).toBe("rgb(0,0,255)");
	});

	it("swaps fg/bg under the inverse attribute", () => {
		const gr = makeRenderer();
		// inverse fg uses the bg color (and vice-versa)
		expect(gr.resolveFg(RED, BLUE, ATTR_INVERSE)).toBe("rgb(0,0,255)");
		expect(gr.resolveBg(RED, BLUE, ATTR_INVERSE)).toBe("rgb(255,0,0)");
	});

	// GH #111: `printf '\e[7m \e[0m'` — a reverse-video space with BOTH defaults.
	// The swapped fallbacks must cross over, otherwise the block paints bg-on-bg
	// and the Pi composer caret is invisible.
	it("inverse with both defaults swaps the fallbacks", () => {
		const gr = makeRenderer();
		const a = ATTR_INVERSE | ATTR_DEFAULT_FG | ATTR_DEFAULT_BG;
		expect(gr.resolveBg(RED, BLUE, a)).toBe(DEF_FG);
		expect(gr.resolveFg(RED, BLUE, a)).toBe(DEF_BG);
	});

	it("inverse with an explicit fg keeps that fg as the painted bg", () => {
		const gr = makeRenderer();
		const a = ATTR_INVERSE | ATTR_DEFAULT_BG;
		expect(gr.resolveBg(RED, BLUE, a)).toBe("rgb(255,0,0)");
		expect(gr.resolveFg(RED, BLUE, a)).toBe(DEF_BG);
	});

	it("inverse with an explicit bg keeps that bg as the painted fg", () => {
		const gr = makeRenderer();
		const a = ATTR_INVERSE | ATTR_DEFAULT_FG;
		expect(gr.resolveFg(RED, BLUE, a)).toBe("rgb(0,0,255)");
		expect(gr.resolveBg(RED, BLUE, a)).toBe(DEF_FG);
	});

	it("leaves non-inverse default cells on their own defaults", () => {
		const gr = makeRenderer();
		const a = ATTR_DEFAULT_FG | ATTR_DEFAULT_BG;
		expect(gr.resolveFg(RED, BLUE, a)).toBe(DEF_FG);
		expect(gr.resolveBg(RED, BLUE, a)).toBe(DEF_BG);
	});
});

describe("gridRenderer font style", () => {
	it("builds a plain font string at the default weight", () => {
		const gr = makeRenderer(300);
		expect(gr.buildFontStyle(0, 14, "JetBrains Mono")).toBe("300 14px JetBrains Mono");
	});

	it("uses bold weight for bold cells", () => {
		const gr = makeRenderer(300);
		expect(gr.buildFontStyle(ATTR_BOLD, 14, "JetBrains Mono")).toBe("bold 14px JetBrains Mono");
	});

	it("prefixes italic for italic cells", () => {
		const gr = makeRenderer(400);
		expect(gr.buildFontStyle(ATTR_ITALIC, 16, "Hack")).toBe("italic 400 16px Hack");
	});
});

// --- Split (background/glyph) compositing regression guard ---
//
// color-tools plan, Phase 5 z-order compositing: `paintRowBackground` +
// `paintRowGlyphs` must always stay an exact decomposition of the fused
// `paintRow` — the split canvases only get exercised once a session sees a
// z<0 image placement, so a silent drift between the two paths would go
// unnoticed by anyone testing the (far more common) fused fast path.

interface RecordedOp {
	op: string;
	args: unknown[];
}

function makeRecordingCtx(): { ctx: CanvasRenderingContext2D; ops: RecordedOp[] } {
	const ops: RecordedOp[] = [];
	const store: Record<string, unknown> = {};
	const ctx = new Proxy(store, {
		get(target, prop) {
			if (typeof prop !== "string") return undefined;
			if (prop === "canvas") return { width: 800, height: 400 };
			// A property previously SET (fillStyle, font, ...) reads back its
			// real value; anything else is treated as a method call and
			// recorded when invoked.
			if (prop in target) return target[prop];
			return (...args: unknown[]) => {
				ops.push({ op: prop, args });
			};
		},
		set(target, prop, value) {
			if (typeof prop === "string") {
				target[prop] = value;
				ops.push({ op: `set:${prop}`, args: [value] });
			}
			return true;
		},
	}) as unknown as CanvasRenderingContext2D;
	return { ctx, ops };
}

function makeRow(cells: Array<{ ch: string; bg?: number; attrs?: number }>): DecodedRow {
	const count = cells.length;
	const codepoints = new Uint32Array(count);
	const fg = new Uint32Array(count);
	const bg = new Uint32Array(count);
	const attrs = new Uint8Array(count);
	cells.forEach((cell, i) => {
		codepoints[i] = cell.ch.codePointAt(0) ?? 0x20;
		bg[i] = cell.bg ?? 0;
		attrs[i] = (cell.attrs ?? 0) | ATTR_DEFAULT_FG | (cell.bg === undefined ? ATTR_DEFAULT_BG : 0);
	});
	return { index: 0, count, wrapped: false, codepoints, fg, bg, attrs };
}

const TEST_METRICS = {
	cellWidth: 9,
	cellHeight: 18,
	baseline: 14,
	dpr: 1,
	scaledCellWidth: 9,
	scaledCellHeight: 18,
	fontSize: 14,
} as unknown as CellMetrics;

describe("gridRenderer split compositing matches the fused pass", () => {
	it("paintGridBackground + paintGridGlyphs together reproduce paintGrid's exact op sequence", () => {
		const rowMap = new Map<number, DecodedRow>([
			[0, makeRow([{ ch: "x", bg: 0x112233 }, { ch: "y" }, { ch: "!", attrs: ATTR_UNDERLINE }])],
		]);

		const fused = makeRecordingCtx();
		const grFused = createGridRenderer(fused.ctx, { fontWeight: () => 400, getFontFamily: () => "monospace" });
		grFused.setTheme(DEF_BG, DEF_FG);
		grFused.paintGrid(rowMap, TEST_METRICS, { fullRepaint: true });

		const split = makeRecordingCtx();
		const grSplit = createGridRenderer(split.ctx, { fontWeight: () => 400, getFontFamily: () => "monospace" });
		grSplit.setTheme(DEF_BG, DEF_FG);
		grSplit.paintGridBackground(rowMap, TEST_METRICS, { fullRepaint: true });
		grSplit.paintGridGlyphs(rowMap, TEST_METRICS, { fullRepaint: true });

		// The background canvas fills its own opaque cachedBgDefault (matching
		// paintGrid's fused clear) and the glyph canvas instead clears
		// (transparent) — so the canvas-wide clear/fill op itself legitimately
		// differs (fillRect for fused and background-split, clearRect for
		// glyph-split, and split has one MORE such op than fused since it's
		// two separate canvases) while every per-cell drawing op must still
		// match exactly. Identify a canvas-wide clear by its height arg (the
		// full canvas height, 400) vs. a per-cell fillRect (one cell tall).
		const dropCanvasWideClears = (ops: RecordedOp[]) =>
			ops.filter((o) => !((o.op === "fillRect" || o.op === "clearRect") && o.args[3] === 400));
		expect(dropCanvasWideClears(split.ops)).toEqual(dropCanvasWideClears(fused.ops));
	});
});
