import { describe, expect, it } from "vitest";
import { findBlockAtViewport } from "../utils/blockFold";
import { filterMatchesToBlock, resolveScopedBlock } from "../utils/blockSearchFilter";

interface Match {
	row: number;
	col_start: number;
	col_end: number;
}

interface Block {
	promptLine: number;
	endLine: number;
}

describe("filterMatchesToBlock", () => {
	const blocks: Block[] = [
		{ promptLine: 0, endLine: 10 },
		{ promptLine: 10, endLine: 25 },
		{ promptLine: 25, endLine: 50 },
	];

	const allMatches: Match[] = [
		{ row: 3, col_start: 0, col_end: 5 },
		{ row: 7, col_start: 2, col_end: 8 },
		{ row: 12, col_start: 0, col_end: 4 },
		{ row: 20, col_start: 1, col_end: 6 },
		{ row: 30, col_start: 0, col_end: 3 },
		{ row: 45, col_start: 5, col_end: 10 },
	];

	it("returns only matches within the block containing viewport center", () => {
		const result = filterMatchesToBlock(allMatches, blocks, 15);
		expect(result).toEqual([
			{ row: 12, col_start: 0, col_end: 4 },
			{ row: 20, col_start: 1, col_end: 6 },
		]);
	});

	it("returns all matches when viewport is outside any block", () => {
		const result = filterMatchesToBlock(allMatches, blocks, 55);
		expect(result).toEqual(allMatches);
	});

	it("handles viewport at block boundary (promptLine)", () => {
		const result = filterMatchesToBlock(allMatches, blocks, 25);
		expect(result).toEqual([
			{ row: 30, col_start: 0, col_end: 3 },
			{ row: 45, col_start: 5, col_end: 10 },
		]);
	});

	it("returns empty when block has no matches", () => {
		const noMatchBlocks: Block[] = [{ promptLine: 100, endLine: 200 }];
		const result = filterMatchesToBlock(allMatches, noMatchBlocks, 150);
		expect(result).toEqual([]);
	});

	it("handles empty blocks array", () => {
		const result = filterMatchesToBlock(allMatches, [], 15);
		expect(result).toEqual(allMatches);
	});

	it("never selects a degenerate block (endLine === promptLine) as the active block", () => {
		// A degenerate block contains no rows — `viewportCenter < endLine` fails when they're
		// equal, so `find` falls through to the next block that actually contains the line.
		const degenerateBlocks: Block[] = [
			{ promptLine: 0, endLine: 10 },
			{ promptLine: 10, endLine: 10 },
			{ promptLine: 10, endLine: 25 },
		];
		const result = filterMatchesToBlock(allMatches, degenerateBlocks, 10);
		expect(result).toEqual([
			{ row: 12, col_start: 0, col_end: 4 },
			{ row: 20, col_start: 1, col_end: 6 },
		]);
	});

	// `filterMatchesToBlock`'s block-resolution predicate (`viewportCenter >= promptLine &&
	// viewportCenter < endLine`, exclusive end) and `findBlockAtViewport`'s (used by the
	// gutter click and Cmd+Shift+. fold, `>= viewTop` after the issue #3 fix) both resolve
	// "which block does this row belong to" and must agree at a shared boundary row, or
	// block-scoped search and gutter click/fold could silently target different blocks for
	// the same click. `findBlockAtViewport` is called with halfViewportRows=0 here to match
	// its exact single-row resolution shape.
	it("agrees with findBlockAtViewport on which block owns each boundary row", () => {
		for (const boundaryRow of [0, 10, 25]) {
			const viaFilter = blocks.find((b) => boundaryRow >= b.promptLine && boundaryRow < b.endLine);
			const viaViewport = findBlockAtViewport(blocks, boundaryRow, 0);
			expect(viaViewport?.promptLine).toBe(viaFilter?.promptLine);
		}
	});
});

describe("resolveScopedBlock", () => {
	const blocks = [
		{ promptLine: 0, endLine: 10 },
		{ promptLine: 10, endLine: 25 },
		{ promptLine: 25, endLine: null },
	];

	it("resolves the block containing the viewport center — the same block filterMatchesToBlock filters against", () => {
		expect(resolveScopedBlock(blocks, 15)).toBe(blocks[1]);
	});

	it("resolves the still-open block for any center at or past its promptLine", () => {
		expect(resolveScopedBlock(blocks, 100)).toBe(blocks[2]);
	});

	it("returns undefined when the center isn't inside any block", () => {
		expect(resolveScopedBlock([{ promptLine: 50, endLine: 60 }], 10)).toBeUndefined();
	});
});
