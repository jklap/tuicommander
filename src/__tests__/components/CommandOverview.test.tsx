import { render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";

const { getBufferLines } = vi.hoisted(() => ({
	getBufferLines: vi.fn().mockResolvedValue(["should not be used"]),
}));

function makeBlock(overrides: Partial<Record<string, unknown>> = {}) {
	return {
		promptLine: 1,
		commandLine: 2,
		executionLine: 3,
		endLine: 4,
		exitCode: 0,
		startedAt: Date.now() - 1000,
		endedAt: Date.now(),
		promptText: null,
		...overrides,
	};
}

const { mockGet } = vi.hoisted(() => ({ mockGet: vi.fn() }));

vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		getIds: vi.fn().mockReturnValue(["term-1"]),
		get: mockGet,
		setActive: vi.fn(),
	},
}));

import { CommandOverview } from "../../components/CommandOverview/CommandOverview";

describe("CommandOverview", () => {
	it("prefers promptText over grid-slicing when present", async () => {
		mockGet.mockReturnValue({
			name: "Terminal 1",
			commandBlocks: [makeBlock({ promptText: "please refactor the parser" })],
			activeBlock: null,
			shellState: "idle",
			ref: { getBufferLines },
		});
		const { container } = render(() => <CommandOverview />);
		// commandText resolves via a microtask (getCommandText is async).
		await Promise.resolve();
		await Promise.resolve();
		const commandDiv = container.querySelector(".command");
		expect(commandDiv?.textContent).toBe("please refactor the parser");
		expect(getBufferLines).not.toHaveBeenCalled();
	});

	it("falls back to grid-slicing when promptText is null", async () => {
		getBufferLines.mockResolvedValueOnce(["echo", "hello"]);
		mockGet.mockReturnValue({
			name: "Terminal 1",
			commandBlocks: [makeBlock({ promptText: null })],
			activeBlock: null,
			shellState: "idle",
			// No eviction in this fixture, so grid-relative === eviction-stable.
			ref: { getBufferLines, getHistoryBase: () => 0 },
		});
		const { container } = render(() => <CommandOverview />);
		await Promise.resolve();
		await Promise.resolve();
		const commandDiv = container.querySelector(".command");
		expect(commandDiv?.textContent).toBe("echo hello");
		expect(getBufferLines).toHaveBeenCalledWith(2, 3);
	});

	// Scrollback-ring eviction fix: commandLine/executionLine are eviction-stable
	// (see CommandBlock's doc comment) — the grid-slice fallback must convert them
	// down through getHistoryBase() before calling getBufferLines, not pass the
	// raw stored values straight through.
	it("converts commandLine/executionLine through getHistoryBase before slicing the grid", async () => {
		getBufferLines.mockClear();
		getBufferLines.mockResolvedValueOnce(["echo", "hello"]);
		mockGet.mockReturnValue({
			name: "Terminal 1",
			commandBlocks: [makeBlock({ promptText: null, commandLine: 1002, executionLine: 1003 })],
			activeBlock: null,
			shellState: "idle",
			// 1000 lines evicted since these rows were recorded.
			ref: { getBufferLines, getHistoryBase: () => 1000 },
		});
		const { container } = render(() => <CommandOverview />);
		await Promise.resolve();
		await Promise.resolve();
		const commandDiv = container.querySelector(".command");
		expect(commandDiv?.textContent).toBe("echo hello");
		expect(getBufferLines).toHaveBeenCalledWith(2, 3);
	});

	// A real shell block predating shell_integration.rs's 133;B emission (or a
	// shell restart that hasn't picked up the updated script yet) has neither
	// promptText (hook-only field) nor commandLine (needs 133;B) — the only
	// state that path could ever have. getCommandText resolves to "", and the
	// component's own shellState fallback ("running..."/"idle") is what
	// actually renders — not a crash, and not literally blank.
	it("falls back to the shellState label (not a crash) when both promptText and commandLine are null", async () => {
		getBufferLines.mockClear();
		mockGet.mockReturnValue({
			name: "Terminal 1",
			commandBlocks: [makeBlock({ promptText: null, commandLine: null, executionLine: null })],
			activeBlock: null,
			shellState: "idle",
			ref: { getBufferLines },
		});
		const { container } = render(() => <CommandOverview />);
		await Promise.resolve();
		await Promise.resolve();
		const commandDiv = container.querySelector(".command");
		expect(commandDiv?.textContent).toBe("idle");
		expect(getBufferLines).not.toHaveBeenCalled();
	});

	// Fullscreen-mode fix: CommandOverview is the deliberate exception to
	// alt-screen filtering — a block recorded during a Claude Code fullscreen
	// turn has no valid row to render gutter/scrollbar/nav against, but its
	// promptText/duration/exit status are still real and must keep showing
	// here, same as any other block.
	it("still renders prompt/duration/exit status for a block recorded on the alternate screen", async () => {
		mockGet.mockReturnValue({
			name: "Terminal 1",
			commandBlocks: [
				makeBlock({
					promptText: "please refactor the parser",
					onAltScreen: true,
					startedAt: Date.now() - 2000,
					endedAt: Date.now(),
					exitCode: 0,
				}),
			],
			activeBlock: null,
			shellState: "idle",
			ref: { getBufferLines },
		});
		const { container } = render(() => <CommandOverview />);
		await Promise.resolve();
		await Promise.resolve();
		const commandDiv = container.querySelector(".command");
		expect(commandDiv?.textContent).toBe("please refactor the parser");
		expect(container.querySelector(".duration")?.textContent).toBe("2s");
		expect(getBufferLines).not.toHaveBeenCalled();
	});

	// Code-review finding: a REAL shell block (no promptText — the grid-slice
	// fallback is its only text source) tagged `onAltScreen: true` must skip
	// the buffer read entirely rather than slicing alt-screen-relative row
	// numbers against the real primary scrollback. Distinct from the test
	// above, which covers a hook-driven block (has promptText, never hits
	// the grid-slice branch at all).
	it("skips the grid-slice fallback for an alt-screen-tainted block with no promptText", async () => {
		getBufferLines.mockClear();
		mockGet.mockReturnValue({
			name: "Terminal 1",
			commandBlocks: [makeBlock({ promptText: null, onAltScreen: true })],
			activeBlock: null,
			shellState: "idle",
			ref: { getBufferLines },
		});
		const { container } = render(() => <CommandOverview />);
		await Promise.resolve();
		await Promise.resolve();
		const commandDiv = container.querySelector(".command");
		expect(commandDiv?.textContent).toBe("idle");
		expect(getBufferLines).not.toHaveBeenCalled();
	});
});
