import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../../mocks/tauri";

vi.mock("../../../stores/settings", () => ({
	settingsStore: {
		state: { suggestFollowups: true },
		isAgentEnabled: () => true,
	},
}));

vi.mock("../../../stores/repositories", () => ({
	repositoriesStore: { state: { activeRepoPath: "" } },
}));

vi.mock("../../../stores/editorTabs", () => ({
	editorTabsStore: { add: vi.fn() },
}));

import { AgentRow } from "../../../components/SettingsPanel/tabs/AgentsTab";
import { AgentConfigProvider } from "../../../components/SettingsPanel/tabs/agentConfigContext";
import { createAgentConfigsStore } from "../../../stores/agentConfigs";

/**
 * "Prefer TUICommander agent spawning" / "Prefer TUICommander messaging" are
 * plain boolean overrides (unlike intent_tab_title/suggest_followups, which are
 * tri-state) — default-on, off means an explicit `false`. Both must grey out
 * and force-uncheck when the `agent` MCP tool itself is disabled, since
 * build_mcp_instructions collapses both preferences to false in that case
 * regardless of what's stored (see AgentsTab.tsx's `agentMcpToolDisabled`,
 * populated from `load_config().disabled_native_tools` on row-expand).
 */
describe("AgentRow prefer-TUIC spawning/messaging overrides", () => {
	beforeEach(() => {
		mockInvoke.mockReset().mockImplementation(async (cmd: string) => {
			if (cmd === "get_agent_mcp_status") return { installed: true };
			if (cmd === "get_agent_hook_state") return "notInstalled";
			if (cmd === "load_config") return { disabled_native_tools: [] };
			return undefined;
		});
	});

	async function renderExpandedRow() {
		const store = createAgentConfigsStore({
			load: vi.fn().mockResolvedValue({ agents: {} }),
			save: vi.fn().mockResolvedValue(undefined),
		});
		await store.hydrate();

		const result = render(() => (
			<AgentConfigProvider value={store}>
				<AgentRow
					agentType="claude"
					detection={{ type: "claude", available: true, path: "/usr/local/bin/claude", version: "1.0.0" }}
				/>
			</AgentConfigProvider>
		));

		fireEvent.click(result.getByRole("button"));
		await waitFor(() => expect(result.getByText("Track agent intent")).toBeTruthy());
		return { ...result, store };
	}

	it("both checkboxes default to checked (on) when the agent has no override and the tool is enabled", async () => {
		const { getByLabelText } = await renderExpandedRow();
		await waitFor(() => {
			expect((getByLabelText("Prefer TUICommander agent spawning") as HTMLInputElement).checked).toBe(true);
			expect((getByLabelText("Prefer TUICommander messaging") as HTMLInputElement).checked).toBe(true);
		});
	});

	it("unchecking spawning persists an explicit false, independent of messaging", async () => {
		const { getByLabelText, store } = await renderExpandedRow();
		const spawning = getByLabelText("Prefer TUICommander agent spawning") as HTMLInputElement;
		await waitFor(() => expect(spawning.checked).toBe(true));

		fireEvent.click(spawning);

		await waitFor(() => expect(store.getPreferTuicSpawning("claude")).toBe(false));
		expect(store.getPreferTuicMessaging("claude")).toBeUndefined();
	});

	it("unchecking messaging persists an explicit false, independent of spawning", async () => {
		const { getByLabelText, store } = await renderExpandedRow();
		const messaging = getByLabelText("Prefer TUICommander messaging") as HTMLInputElement;
		await waitFor(() => expect(messaging.checked).toBe(true));

		fireEvent.click(messaging);

		await waitFor(() => expect(store.getPreferTuicMessaging("claude")).toBe(false));
		expect(store.getPreferTuicSpawning("claude")).toBeUndefined();
	});

	it("both checkboxes grey out and force-uncheck when the agent MCP tool is disabled", async () => {
		mockInvoke.mockReset().mockImplementation(async (cmd: string) => {
			if (cmd === "get_agent_mcp_status") return { installed: true };
			if (cmd === "get_agent_hook_state") return "notInstalled";
			if (cmd === "load_config") return { disabled_native_tools: ["agent"] };
			return undefined;
		});

		const { getByLabelText, getByText } = await renderExpandedRow();

		await waitFor(() => {
			const spawning = getByLabelText("Prefer TUICommander agent spawning") as HTMLInputElement;
			const messaging = getByLabelText("Prefer TUICommander messaging") as HTMLInputElement;
			expect(spawning.disabled).toBe(true);
			expect(messaging.disabled).toBe(true);
			expect(spawning.checked).toBe(false);
			expect(messaging.checked).toBe(false);
		});
		expect(getByText(/neither preference has anything to do while that's the case/)).toBeTruthy();
	});

	it("does not grey out when a different tool (not `agent`) is disabled", async () => {
		mockInvoke.mockReset().mockImplementation(async (cmd: string) => {
			if (cmd === "get_agent_mcp_status") return { installed: true };
			if (cmd === "get_agent_hook_state") return "notInstalled";
			if (cmd === "load_config") return { disabled_native_tools: ["session"] };
			return undefined;
		});

		const { getByLabelText } = await renderExpandedRow();

		await waitFor(() => {
			const spawning = getByLabelText("Prefer TUICommander agent spawning") as HTMLInputElement;
			expect(spawning.disabled).toBe(false);
			expect(spawning.checked).toBe(true);
		});
	});
});
