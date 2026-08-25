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
			ref: { getBufferLines },
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
});
