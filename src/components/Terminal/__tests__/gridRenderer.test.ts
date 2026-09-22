import { describe, expect, it, vi } from "vitest";

import {
	ATTR_BOLD,
	ATTR_DEFAULT_BG,
	ATTR_DEFAULT_FG,
	ATTR_INVERSE,
	ATTR_ITALIC,
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

describe("gridRenderer multi-codepoint cells", () => {
	const metrics: CellMetrics = {
		cellWidth: 8,
		cellHeight: 16,
		baseline: 12,
		fontSize: 14,
		dpr: 1,
		scaledCellWidth: 8,
		scaledCellHeight: 16,
	};

	function paint(codepoint: number, extras: string) {
		const fillText = vi.fn();
		const ctx = { fillText, globalAlpha: 1 } as unknown as CanvasRenderingContext2D;
		const renderer = createGridRenderer(ctx, { fontWeight: () => 400, getFontFamily: () => "monospace" });
		renderer.setTheme(DEF_BG, DEF_FG);
		const row: DecodedRow = {
			index: 0,
			count: 1,
			wrapped: false,
			codepoints: Uint32Array.of(codepoint),
			cellExtras: new Map([[0, extras]]),
			fg: new Uint32Array(1),
			bg: new Uint32Array(1),
			attrs: Uint8Array.of(ATTR_DEFAULT_BG | ATTR_DEFAULT_FG),
		};
		renderer.paintRow(row, 0, metrics);
		return fillText;
	}

	it("shapes a decomposed accent as one font glyph string", () => {
		expect(paint(0x65, "\u0301")).toHaveBeenCalledWith("e\u0301", 0, 12);
	});

	it("routes marked box-drawing cells through font shaping", () => {
		expect(paint(0x2500, "\u0301")).toHaveBeenCalledWith("─\u0301", 0, 12);
	});

	it("preserves an explicit emoji variation selector", () => {
		expect(paint(0x25cf, "\uFE0F")).toHaveBeenCalledWith("●\uFE0F", 0, 12);
	});
});
