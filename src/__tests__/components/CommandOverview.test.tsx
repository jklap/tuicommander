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
});
