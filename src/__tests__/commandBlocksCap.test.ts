import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const MAX_BLOCKS = 500;

describe("CommandBlocks cap logic", () => {
	it("evicts oldest blocks when exceeding MAX_BLOCKS", () => {
		const blocks = Array.from({ length: MAX_BLOCKS + 10 }, (_, i) => ({
			promptLine: i,
			commandLine: null,
			executionLine: null,
			endLine: i + 1,
			exitCode: 0,
			startedAt: Date.now() - (MAX_BLOCKS + 10 - i) * 1000,
			endedAt: Date.now() - (MAX_BLOCKS + 9 - i) * 1000,
		}));

		const capped = blocks.slice(-MAX_BLOCKS);
		expect(capped.length).toBe(MAX_BLOCKS);
		expect(capped[0].promptLine).toBe(10);
	});

	it("cleans foldedBlocks for evicted entries", () => {
		const foldedBlocks = new Set([0, 5, 10, 499, 500, 505]);
		const evictedPromptLines = new Set([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
		for (const line of evictedPromptLines) {
			foldedBlocks.delete(line);
		}
		expect(foldedBlocks.has(0)).toBe(false);
		expect(foldedBlocks.has(5)).toBe(false);
		expect(foldedBlocks.has(10)).toBe(true);
		expect(foldedBlocks.has(500)).toBe(true);
	});
});

// The tests above re-implement the cap/eviction arithmetic inline — useful as
// a spec, but they never touch the real store's own eviction path
// (`_scheduleOsc133Flush`'s `commandBlocks` setState in `terminals.ts`, which
// also prunes `foldedBlocks` for evicted lines). This exercises that real
// path end to end.
describe("CommandBlocks cap — real store", () => {
	let store: typeof import("../stores/terminals").terminalsStore;
	const rafCallbacks = new Map<number, FrameRequestCallback>();
	let nextHandle = 0;

	function flushPendingRaf(): void {
		for (const [handle, cb] of rafCallbacks) {
			cb(0);
			rafCallbacks.delete(handle);
		}
	}

	beforeEach(async () => {
		vi.resetModules();
		localStorage.clear();
		rafCallbacks.clear();
		nextHandle = 0;
		const { terminalsStore } = await import("../stores/terminals");
		const { makeTerminal } = await import("./helpers/store");
		store = terminalsStore;
		vi.stubGlobal(
			"requestAnimationFrame",
			vi.fn((cb: FrameRequestCallback) => {
				nextHandle += 1;
				rafCallbacks.set(nextHandle, cb);
				return nextHandle;
			}),
		);
		vi.stubGlobal("cancelAnimationFrame", vi.fn());
		store.add(makeTerminal());
	});

	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it("evicts oldest real blocks past MAX_BLOCKS and prunes their fold state", () => {
		const id = "term-1";
		// Fold the very first block so eviction has something real to prune.
		store.handleOsc133(id, "A", 0);
		store.toggleBlockFold(id, 0);
		store.handleOsc133(id, "D", 1, 0);
		flushPendingRaf();

		for (let i = 1; i < MAX_BLOCKS + 10; i++) {
			store.handleOsc133(id, "A", i * 10);
			store.handleOsc133(id, "D", i * 10 + 1, 0);
			flushPendingRaf();
		}

		const term = store.get(id)!;
		expect(term.commandBlocks.length).toBe(MAX_BLOCKS);
		expect(term.commandBlocks[0].promptLine).toBe(100); // block for i=10, the 11th pushed
		expect(term.foldedBlocks.has(0)).toBe(false);
	});
});
