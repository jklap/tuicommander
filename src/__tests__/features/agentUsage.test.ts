import { createStore } from "solid-js/store";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * The ticker must not name a vendor it has not seen. A Codex-only install polling
 * `get_claude_usage_api` at startup shows "Claude — no token", which is the
 * wrong-vendor reading the feature exists to prevent.
 */

const invoke = vi.fn();
vi.mock("../../invoke", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const addMessage = vi.fn();
const removeMessage = vi.fn();
vi.mock("../../stores/statusBarTicker", () => ({
	statusBarTicker: {
		addMessage: (...args: unknown[]) => addMessage(...args),
		removeMessage: (...args: unknown[]) => removeMessage(...args),
	},
}));

vi.mock("../../stores/mdTabs", () => ({ mdTabsStore: { addClaudeUsage: vi.fn(), addCodexUsage: vi.fn() } }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn() } }));

interface FakeTerminal {
	agentType: string | null;
}
const [terminals, setTerminals] = createStore<{
	activeId: string | null;
	terminals: Record<string, FakeTerminal>;
}>({ activeId: null, terminals: {} });

vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		get state() {
			return terminals;
		},
	},
}));

const { destroyAgentUsage, initAgentUsage } = await import("../../features/agentUsage");

/** Let the Solid effect and the async poll settle. */
const settle = async () => {
	await Promise.resolve();
	await Promise.resolve();
};

describe("agent usage ticker — no vendor default", () => {
	beforeEach(() => {
		destroyAgentUsage();
		invoke.mockReset();
		invoke.mockResolvedValue({});
		addMessage.mockReset();
		removeMessage.mockReset();
		setTerminals({ activeId: null, terminals: {} });
	});

	// The poll interval outlives the test otherwise.
	afterEach(() => destroyAgentUsage());

	it("polls nothing and writes no ticker when no terminal is active", async () => {
		initAgentUsage();
		await settle();

		expect(invoke).not.toHaveBeenCalled();
		expect(addMessage).not.toHaveBeenCalled();
	});

	it("polls nothing when the active tab is a shell with no agent", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: null } } });

		initAgentUsage();
		await settle();

		expect(invoke).not.toHaveBeenCalled();
		expect(addMessage).not.toHaveBeenCalled();
	});

	it("polls nothing when the active tab is an agent with no usage API", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "aider" } } });

		initAgentUsage();
		await settle();

		expect(invoke).not.toHaveBeenCalled();
		expect(addMessage).not.toHaveBeenCalled();
	});

	it("never polls Claude on a Codex-only install", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "codex" } } });

		initAgentUsage();
		await settle();

		expect(invoke).toHaveBeenCalledTimes(1);
		expect(invoke).toHaveBeenCalledWith("get_codex_usage_api");
		expect(addMessage).toHaveBeenCalledWith(expect.objectContaining({ label: "Codex" }));
	});

	it("starts polling when an agent tab becomes active after an empty start", async () => {
		initAgentUsage();
		await settle();
		expect(invoke).not.toHaveBeenCalled();

		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "claude" } } });
		await settle();

		expect(invoke).toHaveBeenCalledExactlyOnceWith("get_claude_usage_api");
	});

	it("keeps the detected agent when the active tab moves to a shell", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "codex" } } });
		initAgentUsage();
		await settle();
		invoke.mockClear();

		setTerminals("activeId", null);
		await settle();

		// Sticky: no repoll, and nothing retracts the Codex number already shown.
		expect(invoke).not.toHaveBeenCalled();
		expect(removeMessage).not.toHaveBeenCalled();
	});
});
