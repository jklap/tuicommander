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

vi.mock("../../stores/mdTabs", () => ({
	mdTabsStore: { addClaudeUsage: vi.fn(), addCodexUsage: vi.fn(), addGrokUsage: vi.fn() },
}));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn() } }));

interface FakeTerminal {
	agentType: string | null;
	sessionId?: string | null;
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

	it("uses Grok's provider billing surface for a Grok tab", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "grok" } } });

		initAgentUsage();
		await settle();

		expect(invoke).toHaveBeenCalledExactlyOnceWith("get_grok_usage_api");
		expect(addMessage).toHaveBeenCalledWith(expect.objectContaining({ label: "Grok" }));
	});

	it("starts polling when an agent tab becomes active after an empty start", async () => {
		initAgentUsage();
		await settle();
		expect(invoke).not.toHaveBeenCalled();

		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "claude", sessionId: "claude-one" } } });
		await settle();

		expect(invoke).toHaveBeenCalledExactlyOnceWith("get_claude_usage_api", { sessionId: "claude-one" });
	});

	it("repolls when focus moves between Claude sessions and opens the matching dashboard", async () => {
		setTerminals({
			activeId: "t1",
			terminals: {
				t1: { agentType: "claude", sessionId: "private-session" },
				t2: { agentType: "claude", sessionId: "default-session" },
			},
		});
		initAgentUsage();
		await settle();
		expect(invoke).toHaveBeenCalledWith("get_claude_usage_api", { sessionId: "private-session" });

		setTerminals("activeId", "t2");
		await settle();
		expect(invoke).toHaveBeenLastCalledWith("get_claude_usage_api", { sessionId: "default-session" });
		expect(addMessage).toHaveBeenLastCalledWith(expect.objectContaining({ label: "Claude" }));
		const { mdTabsStore } = await import("../../stores/mdTabs");
		addMessage.mock.lastCall?.[0].onClick();
		expect(mdTabsStore.addClaudeUsage).toHaveBeenCalledWith("default-session");
	});

	it("shows unknown when the focused Claude profile cannot be read", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "claude", sessionId: "missing-profile" } } });
		invoke.mockRejectedValueOnce(new Error("Cannot read Claude session profile"));
		initAgentUsage();
		await settle();
		expect(addMessage).toHaveBeenCalledWith(expect.objectContaining({ label: "Claude", text: "unknown" }));
	});

	it("does not open default-account usage for a Claude tab without a backend session", async () => {
		setTerminals({ activeId: "t1", terminals: { t1: { agentType: "claude", sessionId: null } } });
		initAgentUsage();
		await settle();
		expect(invoke).not.toHaveBeenCalled();
		expect(addMessage).toHaveBeenCalledWith(expect.objectContaining({ text: "unknown", onClick: undefined }));
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
