import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const handlers: Record<string, (payload: unknown) => void> = {};

vi.mock("../../transport", () => ({
	rpc: vi.fn(async () => undefined),
	subscribeEvents: vi.fn(async (h: Record<string, (payload: unknown) => void>) => {
		Object.assign(handlers, h);
		return () => {};
	}),
}));

import { AgentWrapPromptHost } from "../../components/AgentWrapPromptHost/AgentWrapPromptHost";
import { __resetAgentWrapPromptQueue } from "../../stores/agentWrapPrompt";
import { rpc } from "../../transport";

const mockRpc = vi.mocked(rpc);

afterEach(cleanup);

function emitPrompt(agentType = "claude", id = "r1") {
	handlers["agent-wrap-prompt"]({ request_id: id, agent_type: agentType });
}

function emitResolved(agentType = "claude", id = "r1") {
	handlers["agent-wrap-prompt-resolved"]({ request_id: id, agent_type: agentType, decision: null });
}

describe("AgentWrapPromptHost", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		__resetAgentWrapPromptQueue();
	});

	it("renders nothing until a shell detects the condition", () => {
		const { container } = render(() => <AgentWrapPromptHost />);
		expect(container.textContent).toBe("");
	});

	it("shows the prompt for the detected agent", async () => {
		const { findByText } = render(() => <AgentWrapPromptHost />);
		emitPrompt("claude");
		expect(await findByText(/Wrap your claude function\?/)).toBeTruthy();
	});

	it("sends decision:true when the human chooses to wrap", async () => {
		const { findByText } = render(() => <AgentWrapPromptHost />);
		emitPrompt("claude", "r1");
		(await findByText("Wrap my function")).click();
		// A non-null decision resolves agentConfigsStore via a lazy `import()`
		// before calling rpc — no longer synchronous within the click handler.
		await vi.waitFor(() =>
			expect(mockRpc).toHaveBeenCalledWith("agent_wrap_prompt_response", {
				requestId: "r1",
				agentType: "claude",
				decision: true,
			}),
		);
	});

	it("sends decision:false when the human chooses to leave it alone", async () => {
		const { findByText } = render(() => <AgentWrapPromptHost />);
		emitPrompt("claude", "r1");
		(await findByText("Leave it alone")).click();
		await vi.waitFor(() =>
			expect(mockRpc).toHaveBeenCalledWith("agent_wrap_prompt_response", {
				requestId: "r1",
				agentType: "claude",
				decision: false,
			}),
		);
	});

	it("sends decision:null (not the same as an explicit No) on dismiss", async () => {
		// This is the exact distinction the whole feature exists to keep —
		// see agent_wrap_prompt.rs's module doc comment on why this is a
		// separate mechanism from McpConfirm rather than reusing its
		// bool-only shape.
		const { findByText } = render(() => <AgentWrapPromptHost />);
		emitPrompt("claude", "r1");
		(await findByText("Not now")).click();
		expect(mockRpc).toHaveBeenCalledWith("agent_wrap_prompt_response", {
			requestId: "r1",
			agentType: "claude",
			decision: null,
		});
	});

	it("lets Enter fall to the no-op dismiss, never silently opt in", async () => {
		const { findByText } = render(() => <AgentWrapPromptHost />);
		emitPrompt("claude", "r1");
		await findByText(/Wrap your claude function\?/);
		document.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
		expect(mockRpc).toHaveBeenCalledWith("agent_wrap_prompt_response", {
			requestId: "r1",
			agentType: "claude",
			decision: null,
		});
	});

	it("a Resolved event from another client closes the dialog without a local click", async () => {
		const { findByText, queryByText } = render(() => <AgentWrapPromptHost />);
		emitPrompt("claude", "r1");
		await findByText(/Wrap your claude function\?/);
		emitResolved("claude", "r1");
		expect(queryByText(/Wrap your claude function\?/)).toBeNull();
	});

	it("shows independent prompts for two different agents at once", async () => {
		const { findByText } = render(() => <AgentWrapPromptHost />);
		emitPrompt("claude", "r1");
		emitPrompt("codex", "r2");
		expect(await findByText(/Wrap your claude function\?/)).toBeTruthy();
		expect(await findByText(/Wrap your codex function\?/)).toBeTruthy();
	});

	it("with two prompts open, Enter resolves only the top-most one, not both", async () => {
		// Regression test: ConfirmDialog's Enter handler used to be a bare
		// document keydown listener with no notion of "which dialog is on
		// top" — with claude's and codex's prompts both mounted at once, a
		// single Enter press fired BOTH dialogs' default action. Fixed via
		// stores/modalStack's isTopModal — see ConfirmDialog.tsx.
		const { findByText } = render(() => <AgentWrapPromptHost />);
		emitPrompt("claude", "r1");
		emitPrompt("codex", "r2");
		await findByText(/Wrap your claude function\?/);
		await findByText(/Wrap your codex function\?/);

		document.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));

		expect(mockRpc).toHaveBeenCalledTimes(1);
		expect(mockRpc).toHaveBeenCalledWith("agent_wrap_prompt_response", {
			requestId: "r2",
			agentType: "codex",
			decision: null,
		});
	});
});
