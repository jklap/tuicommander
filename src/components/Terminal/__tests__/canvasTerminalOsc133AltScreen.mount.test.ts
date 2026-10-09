/**
 * Audit C5: the backend tags every OSC 133 marker with `on_alt_screen`
 * (captured with `line` inside the alacritty fork's handler), but
 * CanvasTerminal's `osc133` listener used to drop the field, so a shell block
 * recorded inside a fullscreen TUI kept `onAltScreen: false` and every
 * row-anchored consumer (gutter marks, scrollbar ticks, fold, jump-nav)
 * anchored a mark at a meaningless alt-screen cursor row.
 */

import { waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeTerminal } from "../../../__tests__/helpers/store";
import { rowAnchoredBlocks, terminalsStore } from "../../../stores/terminals";
import {
	createFakeTransport,
	FIXED_CELL_METRICS,
	mountCanvasTerminal,
	stubCanvasEnvironment,
} from "./helpers/mountCanvasTerminal";

const fakeTransport = vi.hoisted(() => ({ current: null as ReturnType<typeof createFakeTransport> | null }));

vi.mock("../canvasTerminalTransport", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../canvasTerminalTransport")>();
	return { ...actual, createTransport: () => fakeTransport.current! };
});

vi.mock("../glyphCache", () => ({
	getSharedMetrics: () => FIXED_CELL_METRICS,
	acquireCache: vi.fn(),
	releaseCache: vi.fn(),
	invalidateGlyphCache: vi.fn(),
}));

const TERMINAL_ID = "osc133-alt-t1";
let restoreEnv: () => void;

beforeEach(() => {
	restoreEnv = stubCanvasEnvironment();
	fakeTransport.current = createFakeTransport();
	terminalsStore.register(TERMINAL_ID, makeTerminal({ sessionId: "s1" }));
});

afterEach(() => {
	restoreEnv();
	terminalsStore.remove(TERMINAL_ID);
});

async function runBlock(onAltScreen: boolean | undefined) {
	const mounted = await mountCanvasTerminal({ sessionId: "s1", terminalId: TERMINAL_ID });
	const transport = fakeTransport.current!;
	const extra = onAltScreen === undefined ? {} : { on_alt_screen: onAltScreen };
	transport.emitEvent("osc133", { marker: "A", line: 3, exit_code: null, ...extra });
	transport.emitEvent("osc133", { marker: "D", line: 5, exit_code: 0, ...extra });
	await waitFor(() => expect(terminalsStore.get(TERMINAL_ID)?.commandBlocks).toHaveLength(1));
	const blocks = terminalsStore.get(TERMINAL_ID)!.commandBlocks;
	await mounted.dispose();
	return blocks;
}

describe("CanvasTerminal osc133 on_alt_screen", () => {
	it("tags a block recorded on the alternate screen so row-anchored consumers skip it", async () => {
		const blocks = await runBlock(true);
		expect(blocks[0].onAltScreen).toBe(true);
		expect(rowAnchoredBlocks(blocks)).toHaveLength(0);
	});

	it("keeps a primary-screen block row-anchored", async () => {
		const blocks = await runBlock(false);
		expect(blocks[0]).toMatchObject({ promptLine: 3, endLine: 5, exitCode: 0, onAltScreen: false });
		expect(rowAnchoredBlocks(blocks)).toHaveLength(1);
	});

	it("treats a payload without the field (older backend) as primary screen", async () => {
		const blocks = await runBlock(undefined);
		expect(blocks[0].onAltScreen).toBe(false);
	});
});
