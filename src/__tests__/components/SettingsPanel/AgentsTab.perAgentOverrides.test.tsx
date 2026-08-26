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

	function triGroup(container: HTMLElement, label: string): HTMLElement {
		return container.querySelector(`[role="radiogroup"][aria-label="${label}"]`) as HTMLElement;
	}
	function triSegment(group: HTMLElement, kind: "off" | "global" | "on"): HTMLElement {
		return group.querySelector(`[data-kind="${kind}"]`) as HTMLElement;
	}

	it("renders both rows defaulted to 'use global' when the agent has no override", async () => {
		const { container } = await renderExpandedRow();
		expect(triSegment(triGroup(container, "Track agent intent"), "global").getAttribute("aria-checked")).toBe("true");
		expect(triSegment(triGroup(container, "Show suggested follow-ups"), "global").getAttribute("aria-checked")).toBe(
			"true",
		);
	});

	it("selecting On/Off for intent_tab_title persists a concrete boolean, not undefined", async () => {
		const { container, store } = await renderExpandedRow();
		fireEvent.click(triSegment(triGroup(container, "Track agent intent"), "off"));
		expect(store.getIntentTabTitle("claude")).toBe(false);
	});

	it("selecting 'Global' resets the override back to undefined (inherit)", async () => {
		const { container, store } = await renderExpandedRow();
		fireEvent.click(triSegment(triGroup(container, "Show suggested follow-ups"), "on"));
		expect(store.getSuggestFollowups("claude")).toBe(true);

		fireEvent.click(triSegment(triGroup(container, "Show suggested follow-ups"), "global"));
		expect(store.getSuggestFollowups("claude")).toBeUndefined();
	});
});
