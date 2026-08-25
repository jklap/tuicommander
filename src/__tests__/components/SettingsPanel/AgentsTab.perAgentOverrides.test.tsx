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
 * These two rows (`intent_tab_title`, `suggest_followups`) are the per-agent
 * inheritable overrides — same "checkbox that can't get back to inherit" problem
 * as RepoWorktreeTab's, rendered with the same explicit-inherit select, but the
 * agent store uses `undefined` as its inherit sentinel instead of `null`.
 * This covers the bridging AgentsTab does at the call site (absent -> inherit
 * option, inherit option -> undefined) since the store side is covered by
 * agentConfigs.test.ts.
 */
describe("AgentRow per-agent inheritable overrides", () => {
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

	function overrideSelect(container: HTMLElement, field: string): HTMLSelectElement {
		return container.querySelector(`select[data-setting-id="agents.agents.{key}.${field}"]`) as HTMLSelectElement;
	}

	it("renders both rows defaulted to 'use global' when the agent has no override", async () => {
		const { container } = await renderExpandedRow();
		for (const field of ["intent_tab_title", "suggest_followups"]) {
			const select = overrideSelect(container, field);
			expect(select.value).toBe("__inherit__");
			expect(select.selectedOptions[0].textContent).toBe("Use global default (On)");
		}
	});

	it("selecting On/Off for intent_tab_title persists a concrete boolean, not undefined", async () => {
		const { container, store } = await renderExpandedRow();
		const select = overrideSelect(container, "intent_tab_title");
		fireEvent.change(select, { target: { value: "false" } });
		expect(store.getIntentTabTitle("claude")).toBe(false);
		fireEvent.change(select, { target: { value: "true" } });
		expect(store.getIntentTabTitle("claude")).toBe(true);
	});

	it("selecting 'Use global default' resets the override back to undefined (inherit)", async () => {
		const { container, store } = await renderExpandedRow();
		const select = overrideSelect(container, "suggest_followups");
		fireEvent.change(select, { target: { value: "true" } });
		expect(store.getSuggestFollowups("claude")).toBe(true);

		fireEvent.change(select, { target: { value: "__inherit__" } });
		expect(store.getSuggestFollowups("claude")).toBeUndefined();
	});
});
