import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../invoke", () => ({
	invoke: vi.fn(async () => undefined),
}));

import { AgentRow } from "../../components/SettingsPanel/tabs/AgentsTab";
import { AgentConfigProvider } from "../../components/SettingsPanel/tabs/agentConfigContext";
import { invoke } from "../../invoke";
import { createAgentConfigsStore } from "../../stores/agentConfigs";

const mockInvoke = vi.mocked(invoke);

afterEach(cleanup);

/**
 * `AgentRow` reads its config through `useAgentConfig()` context, not the
 * global `agentConfigsStore` singleton — a fresh, isolated store per test
 * (real load/save no-op'd, since nothing here exercises persistence) keeps
 * this test from touching any other test's global state.
 */
function renderExpandedRow(agentType: "claude" | "codex" | "goose" | "gemini") {
	const store = createAgentConfigsStore({
		load: async () => ({ agents: {} }),
		save: async () => {},
	});
	const utils = render(() => (
		<AgentConfigProvider value={store}>
			<AgentRow agentType={agentType} detection={undefined} />
		</AgentConfigProvider>
	));
	utils.getByRole("button").click();
	return utils;
}

describe("AgentRow — wrap-user-function setting", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it.each(["claude", "codex", "goose"] as const)("renders the control for %s", async (agentType) => {
		const { findByText } = renderExpandedRow(agentType);
		expect(await findByText(/If your shell already defines its own/)).toBeTruthy();
	});

	it("does not render the control for an agent with no wrapper (gemini)", async () => {
		const { queryByText, findByText } = renderExpandedRow("gemini");
		// Something else in the expanded section must have rendered, proving
		// the row genuinely expanded and this isn't a false negative from a
		// row that stayed collapsed.
		await findByText(/Auto-retry on server errors/);
		expect(queryByText(/If your shell already defines its own/)).toBeNull();
	});

	it("defaults to 'Ask when detected' when the setting is undecided", async () => {
		const { findByLabelText, findByText } = renderExpandedRow("claude");
		await findByText(/If your shell already defines its own/);
		const select = (await findByLabelText(/If your shell already defines its own/, {
			selector: "select",
		})) as HTMLSelectElement;
		expect(select.value).toBe("ask");
	});

	it("selecting 'Wrap my function' persists value:true", async () => {
		const { findByText } = renderExpandedRow("claude");
		await findByText(/If your shell already defines its own/);
		const select = document.querySelector("select") as HTMLSelectElement;
		select.value = "wrap";
		select.dispatchEvent(new Event("change", { bubbles: true }));
		await vi.waitFor(() => {
			expect(mockInvoke).toHaveBeenCalledWith("set_agent_wrap_user_function", {
				agentType: "claude",
				value: true,
			});
		});
	});

	it("selecting 'Leave my function alone' persists value:false", async () => {
		const { findByText } = renderExpandedRow("codex");
		await findByText(/If your shell already defines its own/);
		const select = document.querySelector("select") as HTMLSelectElement;
		select.value = "leave";
		select.dispatchEvent(new Event("change", { bubbles: true }));
		await vi.waitFor(() => {
			expect(mockInvoke).toHaveBeenCalledWith("set_agent_wrap_user_function", {
				agentType: "codex",
				value: false,
			});
		});
	});

	it("selecting 'Ask when detected' persists value:null", async () => {
		// Expanding the row triggers unrelated side-effect `invoke` calls of
		// its own (e.g. `loadAgentMcpToolStatus`'s "load_config") — assert on
		// the *last* call rather than an exact count, so this test doesn't
		// depend on how many of those happen to fire.
		const { findByText } = renderExpandedRow("goose");
		await findByText(/If your shell already defines its own/);
		const select = document.querySelector("select") as HTMLSelectElement;
		select.value = "wrap";
		select.dispatchEvent(new Event("change", { bubbles: true }));
		await vi.waitFor(() => {
			expect(mockInvoke).toHaveBeenLastCalledWith("set_agent_wrap_user_function", {
				agentType: "goose",
				value: true,
			});
		});
		select.value = "ask";
		select.dispatchEvent(new Event("change", { bubbles: true }));
		await vi.waitFor(() => {
			expect(mockInvoke).toHaveBeenLastCalledWith("set_agent_wrap_user_function", {
				agentType: "goose",
				value: null,
			});
		});
	});
});
