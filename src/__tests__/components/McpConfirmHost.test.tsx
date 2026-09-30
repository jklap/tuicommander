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

import { McpConfirmHost } from "../../components/McpConfirmHost/McpConfirmHost";
import { __resetMcpConfirmQueue } from "../../stores/mcpConfirm";
import { rpc } from "../../transport";

const mockRpc = vi.mocked(rpc);

afterEach(cleanup);

function emitRequest(id = "r1") {
	handlers["mcp-confirm"]({ request_id: id, title: "Force push?", message: "to main" });
}

describe("McpConfirmHost", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		__resetMcpConfirmQueue();
	});

	it("renders nothing until an agent asks", () => {
		const { container } = render(() => <McpConfirmHost />);
		expect(container.textContent).toBe("");
	});

	it("shows the agent's question", async () => {
		const { findByText } = render(() => <McpConfirmHost />);
		emitRequest();
		expect(await findByText("Force push?")).toBeTruthy();
		expect(await findByText("to main")).toBeTruthy();
	});

	it("sends confirm when the human approves", async () => {
		const { findByText } = render(() => <McpConfirmHost />);
		emitRequest();
		(await findByText("Confirm")).click();
		expect(mockRpc).toHaveBeenCalledWith("mcp_confirm_response", { requestId: "r1", confirmed: true });
	});

	it("sends a refusal when the human cancels", async () => {
		const { findByText } = render(() => <McpConfirmHost />);
		emitRequest();
		(await findByText("Cancel")).click();
		expect(mockRpc).toHaveBeenCalledWith("mcp_confirm_response", { requestId: "r1", confirmed: false });
	});

	it("lets Enter fall to Cancel, never to the destructive action", async () => {
		// The agent asks before something destructive. An accidental Enter must
		// take the safe path, so the default button is Cancel.
		const { findByText } = render(() => <McpConfirmHost />);
		emitRequest();
		await findByText("Force push?");
		document.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
		expect(mockRpc).toHaveBeenCalledWith("mcp_confirm_response", { requestId: "r1", confirmed: false });
	});

	it("Escape collapses to the same confirmed:false as an explicit Cancel click", async () => {
		// Found via a test-coverage audit (2026-09-25, agent-wrap-prompt work):
		// the component's own doc comment already documents this ("Cancel is
		// the safe answer for the overlay click and Escape"), and ConfirmDialog
		// wires Escape to onClose via registerModal — but nothing here proved
		// it before this test. Worth keeping as the concrete "old" behavior
		// AgentWrapPromptHost's tests deliberately contrast against (a dismiss
		// there does NOT collapse to the same outcome as an explicit No).
		const { findByText } = render(() => <McpConfirmHost />);
		emitRequest();
		await findByText("Force push?");
		document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
		expect(mockRpc).toHaveBeenCalledWith("mcp_confirm_response", { requestId: "r1", confirmed: false });
	});
});
