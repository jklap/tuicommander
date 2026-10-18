import { expect, it, vi } from "vitest";
import { ATTR_DEFAULT_BG, ATTR_DEFAULT_FG, type CellMetrics, type DecodedRow } from "../canvasTerminalUtils";
import { createGridRenderer, type PaintGridOptions } from "../gridRenderer";

const metrics: CellMetrics = {
	cellWidth: 10,
	cellHeight: 20,
	baseline: 15,
	fontSize: 14,
	dpr: 2,
	scaledCellWidth: 20,
	scaledCellHeight: 40,
};
function row(index: number, text: string): DecodedRow {
	const count = text.length;
	return {
		index,
		count,
		wrapped: false,
		codepoints: Uint32Array.from(text, (c) => c.codePointAt(0)!),
		fg: new Uint32Array(count),
		bg: new Uint32Array(count),
		attrs: new Uint8Array(count).fill(ATTR_DEFAULT_BG | ATTR_DEFAULT_FG),
	};
}
function setup() {
	const ctx = { canvas: { width: 120, height: 120 }, fillRect: vi.fn(), fillText: vi.fn(), drawImage: vi.fn() };
	const renderer = createGridRenderer(ctx as unknown as CanvasRenderingContext2D, {
		fontWeight: () => 400,
		getFontFamily: () => "monospace",
	});
	return { ctx, renderer };
}
function options(viewportTop: number, fullRepaint = false): PaintGridOptions {
	return { viewportTop, fullRepaint, dirtyIndices: new Set([0, 1, 2]) } as PaintGridOptions;
}
it("history growth copies retained pixels instead of repainting every glyph (2040)", () => {
	const { ctx, renderer } = setup();
	renderer.paintGrid(
		new Map([
			[0, row(0, "AAAA")],
			[1, row(1, "BBBB")],
			[2, row(2, "CCCC")],
		]),
		metrics,
		options(0, true),
	);
	ctx.fillText.mockClear();
	ctx.fillRect.mockClear();
	renderer.paintGrid(
		new Map([
			[0, row(0, "BBBB")],
			[1, row(1, "CCCC")],
			[2, row(2, "DDDD")],
		]),
		metrics,
		options(1),
	);
	expect(ctx.drawImage).toHaveBeenCalledTimes(1);
	expect(ctx.fillText.mock.calls.map((call) => call[0])).toEqual(["D", "D", "D", "D"]);
	expect(ctx.fillRect).toHaveBeenCalledWith(-6, 40, 60, 20);
});

it("pixel scrolling repaints changed retained rows rather than copying stale text (2040)", () => {
	const { ctx, renderer } = setup();
	renderer.paintGrid(
		new Map([
			[0, row(0, "AAAA")],
			[1, row(1, "BBBB")],
			[2, row(2, "CCCC")],
		]),
		metrics,
		options(0, true),
	);
	ctx.fillText.mockClear();
	renderer.paintGrid(
		new Map([
			[0, row(0, "BXBB")],
			[1, row(1, "CCCC")],
			[2, row(2, "DDDD")],
		]),
		metrics,
		options(1),
	);
	expect(ctx.fillText.mock.calls.map((call) => call[0]).join("")).toBe("BXBBDDDD");
});
it.each([0, 3, -1])("non-overlapping or non-forward scroll %s never copies unrelated pixels (2040)", (top) => {
	const { ctx, renderer } = setup();
	const rows = new Map([
		[0, row(0, "AAAA")],
		[1, row(1, "BBBB")],
		[2, row(2, "CCCC")],
	]);
	renderer.paintGrid(rows, metrics, options(0, true));
	renderer.paintGrid(rows, metrics, options(top));
	expect(ctx.drawImage).not.toHaveBeenCalled();
});
it("explicit invalidation prevents history pixels crossing a theme or screen boundary (2040)", () => {
	const { ctx, renderer } = setup();
	const rows = new Map([
		[0, row(0, "AAAA")],
		[1, row(1, "BBBB")],
		[2, row(2, "CCCC")],
	]);
	renderer.paintGrid(rows, metrics, options(0, true));
	ctx.fillText.mockClear();
	renderer.paintGrid(rows, metrics, options(1, true));
	expect(ctx.drawImage).not.toHaveBeenCalled();
	expect(ctx.fillText).toHaveBeenCalledTimes(12);
});
