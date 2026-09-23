import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), error: vi.fn(), info: vi.fn(), warn: vi.fn() },
}));
vi.mock("../../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../../transport", () => ({ isTauri: vi.fn(() => true) }));

import { McpIntegrationsSection } from "../../../components/SettingsPanel/tabs/services/McpIntegrationsSection";
import { invoke } from "../../../invoke";
import { isTauri } from "../../../transport";

afterEach(() => {
	vi.clearAllMocks();
	vi.mocked(isTauri).mockReturnValue(true);
});

describe("McpIntegrationsSection", () => {
	it("lists the clients that carry the bridge and removes them all", async () => {
		let installed = ["claude", "codex"];
		vi.mocked(invoke).mockImplementation(async (command: string) => {
			if (command === "list_installed_mcp_integrations") return installed;
			if (command === "remove_all_mcp_integrations") {
				const removed = installed;
				installed = [];
				return removed;
			}
			throw new Error(`unexpected ${command}`);
		});
		const { findByText, queryByText } = render(() => <McpIntegrationsSection />);
		expect(await findByText("claude, codex")).toBeTruthy();

		fireEvent.click(await findByText("Remove all MCP integrations"));

		await waitFor(() => expect(queryByText("Remove all MCP integrations")).toBeNull());
		expect(vi.mocked(invoke)).toHaveBeenCalledWith("remove_all_mcp_integrations");
	});

	it("shows a partial failure and still refreshes the list", async () => {
		let listCalls = 0;
		vi.mocked(invoke).mockImplementation(async (command: string) => {
			if (command === "list_installed_mcp_integrations") return ++listCalls === 1 ? ["claude", "codex"] : ["codex"];
			if (command === "remove_all_mcp_integrations") throw new Error("codex: permission denied");
			throw new Error(`unexpected ${command}`);
		});
		const { findByText } = render(() => <McpIntegrationsSection />);
		fireEvent.click(await findByText("Remove all MCP integrations"));

		expect(await findByText("Error: codex: permission denied")).toBeTruthy();
		expect(await findByText("codex")).toBeTruthy();
		expect(listCalls).toBe(2);
	});

	it("renders nothing in browser mode", () => {
		vi.mocked(isTauri).mockReturnValue(false);
		const { container } = render(() => <McpIntegrationsSection />);
		expect(container.textContent).toBe("");
		expect(vi.mocked(invoke)).not.toHaveBeenCalled();
	});
});
