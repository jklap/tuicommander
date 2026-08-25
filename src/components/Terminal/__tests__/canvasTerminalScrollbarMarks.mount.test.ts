/**
 * Live diagnosis (command-block-review session, 2026-08-25) found that a
 * real Claude Code tab that never scrolled past one screen — the common
 * case for a short turn — has `historySize === 0`, and `updateScrollbar`
 * (`CanvasTerminal.tsx`) hides the whole scrollbar track whenever that's
 * true, before it ever reaches `paintScrollbarMarks`. Since the marks
 * overlay is a child of that same track element, block/prompt marks never
 * appear for any such tab even with real data and both toggles on — this is
 * the direct mechanism behind "no gutter/scrollmarks obvious to me."
 *
 * Pins `shouldShowScrollbar` (`canvasTerminalMarks.ts`) wired into the real
 * component: the track becomes visible whenever there's something to mark,
 * even with zero scrollback, while a plain terminal with neither keeps its
 * prior no-scrollbar look.
 */

import { waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeTerminal } from "../../../__tests__/helpers/store";
import { settingsStore } from "../../../stores/settings";
import { terminalsStore } from "../../../stores/terminals";
import { buildTextFrame } from "./helpers/frameFixture";
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

let restoreEnv: () => void;

beforeEach(() => {
	restoreEnv = stubCanvasEnvironment();
	fakeTransport.current = createFakeTransport();
});

afterEach(() => {
	restoreEnv();
	terminalsStore.remove("scrollbar-marks-t1");
	settingsStore.setShowBlockMarks(true);
	settingsStore.setShowPromptMarks(true);
});

function scrollbarEl(container: HTMLElement): HTMLElement {
	const el = container.querySelector('[data-testid="terminal-scrollbar"]');
	if (!el) throw new Error("no [data-testid=terminal-scrollbar] found");
	return el as HTMLElement;
}

describe("scrollbar track visibility with zero scrollback", () => {
	it("shows the track (and paints marks) with a real block, zero history, toggle on", async () => {
		terminalsStore.register("scrollbar-marks-t1", makeTerminal({ sessionId: "s1" }));
		terminalsStore.update("scrollbar-marks-t1", {
			commandBlocks: [
				{
					promptLine: 2,
					commandLine: null,
					executionLine: null,
					endLine: 5,
					exitCode: 0,
					startedAt: 0,
					endedAt: 1,
					promptText: null,
				},
			],
		});

		const mounted = await mountCanvasTerminal({ sessionId: "s1", terminalId: "scrollbar-marks-t1" });
		fakeTransport.current!.pushFrame(buildTextFrame(["a", "b", "c", "d"], 40, { historySize: 0 }));
		await waitFor(() => expect(scrollbarEl(mounted.container).style.display).toBe("block"));

		await mounted.dispose();
	});

	it("shows the track with a real prompt mark, zero history, toggle on", async () => {
		terminalsStore.register("scrollbar-marks-t1", makeTerminal({ sessionId: "s1" }));
		terminalsStore.update("scrollbar-marks-t1", { userPromptLines: [1] });

		const mounted = await mountCanvasTerminal({ sessionId: "s1", terminalId: "scrollbar-marks-t1" });
		fakeTransport.current!.pushFrame(buildTextFrame(["a", "b"], 40, { historySize: 0 }));
		await waitFor(() => expect(scrollbarEl(mounted.container).style.display).toBe("block"));

		await mounted.dispose();
	});

	it("keeps the track hidden with a block mark whose toggle is OFF", async () => {
		settingsStore.setShowBlockMarks(false);
		terminalsStore.register("scrollbar-marks-t1", makeTerminal({ sessionId: "s1" }));
		terminalsStore.update("scrollbar-marks-t1", {
			commandBlocks: [
				{
					promptLine: 2,
					commandLine: null,
					executionLine: null,
					endLine: 5,
					exitCode: 0,
					startedAt: 0,
					endedAt: 1,
					promptText: null,
				},
			],
		});

		const mounted = await mountCanvasTerminal({ sessionId: "s1", terminalId: "scrollbar-marks-t1" });
		fakeTransport.current!.pushFrame(buildTextFrame(["a", "b"], 40, { historySize: 0 }));
		// Give the frame a moment to be processed, then assert it stayed hidden.
		await new Promise((r) => setTimeout(r, 0));
		expect(scrollbarEl(mounted.container).style.display).toBe("none");

		await mounted.dispose();
	});

	it("keeps the track hidden with zero history and no marks — preserves the plain-terminal look", async () => {
		terminalsStore.register("scrollbar-marks-t1", makeTerminal({ sessionId: "s1" }));

		const mounted = await mountCanvasTerminal({ sessionId: "s1", terminalId: "scrollbar-marks-t1" });
		fakeTransport.current!.pushFrame(buildTextFrame(["a", "b"], 40, { historySize: 0 }));
		await new Promise((r) => setTimeout(r, 0));
		expect(scrollbarEl(mounted.container).style.display).toBe("none");

		await mounted.dispose();
	});

	it("still shows the track when there IS scrollback, even with no marks (existing behavior)", async () => {
		terminalsStore.register("scrollbar-marks-t1", makeTerminal({ sessionId: "s1" }));

		const mounted = await mountCanvasTerminal({ sessionId: "s1", terminalId: "scrollbar-marks-t1" });
		fakeTransport.current!.pushFrame(buildTextFrame(["a", "b"], 40, { historySize: 500 }));
		await waitFor(() => expect(scrollbarEl(mounted.container).style.display).toBe("block"));

		await mounted.dispose();
	});
});
