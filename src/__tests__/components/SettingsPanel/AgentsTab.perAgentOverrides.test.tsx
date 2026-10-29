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
 * These two rows (`intent_tab_title`, `suggest_followups`) are the per-agent tri-state
 * overrides added alongside RepoWorktreeTab's — same "checkbox that can't get back to
 * inherit" problem, but they use `undefined` as their inherit sentinel instead of `null`.
 * This covers the bridging AgentsTab does at the call site (`?? null` in, `?? undefined`
 * out) since the store side is already covered by agentConfigs.test.ts.
 */
describe("AgentRow per-agent tri-state overrides", () => {
	beforeEach(() => {
		mockInvoke.mockReset().mockImplementation(async (cmd: string) => {
			if (cmd === "get_agent_mcp_status") return { installed: true };
			if (cmd === "get_agent_hook_state") return "notInstalled";
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

	// Main's "Use global default" select replaces wip's cycling TriStateToggle
	// (rebase decision 6); the select carries the same three states.
	function overrideSelect(container: HTMLElement, settingId: string): HTMLSelectElement {
		return container.querySelector(`select[data-setting-id="agents.agents.{key}.${settingId}"]`) as HTMLSelectElement;
	}

	it("renders both rows defaulted to 'use global' when the agent has no override", async () => {
		const { container } = await renderExpandedRow();
		expect(overrideSelect(container, "intent_tab_title").value).toBe("inherit");
		expect(overrideSelect(container, "suggest_followups").value).toBe("inherit");
		expect(overrideSelect(container, "intent_tab_title").options[0].textContent).toMatch(/^Use global default/);
	});

	it("selecting On/Off for intent_tab_title persists a concrete boolean, not undefined", async () => {
		const { container, store } = await renderExpandedRow();
		fireEvent.change(overrideSelect(container, "intent_tab_title"), { target: { value: "false" } });
		expect(store.getIntentTabTitle("claude")).toBe(false);
	});

	it("selecting 'Use global default' resets the override back to undefined (inherit)", async () => {
		const { container, store } = await renderExpandedRow();
		const select = overrideSelect(container, "suggest_followups");
		fireEvent.change(select, { target: { value: "true" } });
		expect(store.getSuggestFollowups("claude")).toBe(true);

		fireEvent.change(select, { target: { value: "inherit" } });
		expect(store.getSuggestFollowups("claude")).toBeUndefined();
	});
});
