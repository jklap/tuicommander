import { describe, expect, it } from "vitest";
import { findBlockAtViewport, foldRange } from "../../utils/blockFold";

describe("foldRange", () => {
	it("computes [start, count) after the header row (executionLine ?? promptLine)", () => {
		expect(foldRange({ promptLine: 5, executionLine: null, endLine: 20 })).toEqual({
			foldStart: 6,
			foldedCount: 14,
		});
	});

	it("prefers executionLine over promptLine for the header row", () => {
		expect(foldRange({ promptLine: 5, executionLine: 7, endLine: 20 })).toEqual({
			foldStart: 8,
			foldedCount: 12,
		});
	});

	it("returns null when endLine is still open (block not yet closed)", () => {
		expect(foldRange({ promptLine: 5, executionLine: null, endLine: null })).toBeNull();
	});

	it("returns null for the degenerate endLine === promptLine shape", () => {
		expect(foldRange({ promptLine: 5, executionLine: null, endLine: 5 })).toBeNull();
	});

	it("returns null when endLine leaves nothing to fold (endLine === foldStart)", () => {
		expect(foldRange({ promptLine: 5, executionLine: null, endLine: 6 })).toBeNull();
	});
});

describe("findBlockAtViewport", () => {
	const blocks = [
		{ promptLine: 0, endLine: 10 },
		{ promptLine: 10, endLine: 25 },
		{ promptLine: 25, endLine: null },
	];

	it("finds the block whose header is at or above mid-viewport and whose end hasn't passed viewTop", () => {
		expect(findBlockAtViewport(blocks, 15, 5)).toBe(blocks[1]);
	});

	it("treats an open-ended block (endLine null) as always still containing viewTop", () => {
		expect(findBlockAtViewport(blocks, 100, 5)).toBe(blocks[2]);
	});

	it("returns undefined when no block's header is within halfViewportRows of viewTop", () => {
		expect(findBlockAtViewport([{ promptLine: 100, endLine: 200 }], 10, 5)).toBeUndefined();
	});

	it("returns undefined for an empty blocks array", () => {
		expect(findBlockAtViewport([], 15, 5)).toBeUndefined();
	});

	describe("halfViewportRows=0 (the exact call the gutter click makes)", () => {
		// Issue #3 regression: a zsh precmd cycle emits D (close) then A/B (open) on
		// the SAME buffer line, so a block's endLine is numerically equal to the next
		// block's promptLine (see terminals.ts handleOsc133 and shell_integration.rs's
		// doc comment). CanvasTerminal's gutter mousedown handler calls
		// `findBlockAtViewport(allBlocks, absRow, 0)` with no prior coverage at this
		// exact halfViewportRows=0 boundary shape — the blind spot that let the bug ship.

		it("at the exact boundary row, resolves to the NEWER block (starting there), not the older one ending there", () => {
			// blocks[0] ends at 10, blocks[1] starts at 10 — the shared boundary row.
			expect(findBlockAtViewport(blocks, 10, 0)).toBe(blocks[1]);
		});

		it("resolves to the block a click lands inside, off the boundary", () => {
			expect(findBlockAtViewport(blocks, 12, 0)).toBe(blocks[1]);
			expect(findBlockAtViewport(blocks, 5, 0)).toBe(blocks[0]);
		});

		it("resolves the second boundary (10 -> 25) the same way", () => {
			expect(findBlockAtViewport(blocks, 25, 0)).toBe(blocks[2]);
		});

		it("the still-open block matches any row at or past its promptLine", () => {
			expect(findBlockAtViewport(blocks, 30, 0)).toBe(blocks[2]);
		});
	});
});
