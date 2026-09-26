import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../../../../__tests__/mocks/tauri";

vi.mock("../../../../stores/ui", async () => {
	const { createStore } = await import("solid-js/store");
	const [state, setState] = createStore({ settingsExpertMode: false });
	return {
		uiStore: {
			state,
			setSettingsExpertMode: (enabled: boolean) => setState("settingsExpertMode", enabled),
		},
	};
});

const settingsState = vi.hoisted(() => ({ intentTabTitle: true, suggestFollowups: true, progressTracking: true }));
vi.mock("../../../../stores/settings", () => ({
	settingsStore: {
		state: settingsState,
		isAgentEnabled: () => true,
		toggleAgent: vi.fn(),
		setIntentTabTitle: vi.fn(),
		setSuggestFollowups: vi.fn(),
		setProgressTracking: vi.fn(),
	},
}));

vi.mock("../../../../hooks/useAgentDetection", () => ({
	useAgentDetection: () => ({
		detectAll: vi.fn(),
		detectVersion: vi.fn(),
		isAvailable: () => true,
		getDetection: () => ({ available: true }),
	}),
}));

vi.mock("../../../../plugins", () => ({ setClaudeUsageEnabled: vi.fn() }));
vi.mock("../../../../plugins/pluginLoader", () => ({
	isPluginDisabled: () => false,
	setPluginEnabled: vi.fn(),
}));

import { AGENTS } from "../../../../agents";
import { agentConfigsStore } from "../../../../stores/agentConfigs";
import { settingsExpertStore } from "../../../../stores/settingsExpert";
import { uiStore } from "../../../../stores/ui";
import { AgentsTab } from "../AgentsTab";

// The serialized `AppConfig`/`AgentSettings` defaults these controls compare against.
// The optional overrides are written as explicit nulls: the Rust side omits them
// (`skip_serializing_if`), and a missing key inside a present domain resolves to
// null — the same comparison either way.
const DEFAULTS = {
	app: { progress_tracking: true },
	notifications: {},
	agent_settings: {
		run_configs: [],
		auto_retry_on_error: false,
		headless_template: null,
		native_status_signals: null,
		prevent_alt_screen: null,
		skip_trust_dialog: null,
		hook_instrumentation: null,
		intent_tab_title: null,
		progress_tracking: null,
		suggest_followups: null,
		env_flags: null,
	},
	agents: { headless_agent: null },
};

/** Per-agent override rows: label, the agent whose row carries it, a stored override. */
const OVERRIDES: Array<[string, "claude" | "gemini", Record<string, unknown>]> = [
	["Native status signals", "claude", { native_status_signals: false }],
	["Prevent alternate screen", "gemini", { prevent_alt_screen: false }],
	["Accept workspace trust for managed spawns", "claude", { skip_trust_dialog: false }],
	["Install hooks globally", "gemini", { hook_instrumentation: true }],
	["Track agent intent", "claude", { intent_tab_title: false }],
	["Collect progress", "claude", { progress_tracking: false }],
	["Show suggested follow-ups", "claude", { suggest_followups: false }],
	["Environment Flags", "claude", { env_flags: { CLAUDE_CODE_SIMPLE: "1" } }],
];

let agentsConfig: Record<string, unknown> = {};

async function setup(agents: Record<string, unknown> = {}) {
	agentsConfig = agents;
	await agentConfigsStore.hydrate();
	await settingsExpertStore.open();
}

/** Render the tab and expand one agent's row, where the per-agent controls live. */
function renderExpanded(agent: "claude" | "gemini" = "claude") {
	const result = render(() => <AgentsTab />);
	const header = [...result.container.querySelectorAll("[role='button']")].find((el) =>
		el.textContent?.includes(AGENTS[agent].name),
	) as HTMLElement;
	fireEvent.click(header);
	return result;
}

const has = (container: HTMLElement, text: string) => container.textContent?.includes(text) ?? false;

describe("AgentsTab expert controls", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		settingsState.progressTracking = true;
		uiStore.setSettingsExpertMode(false);
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "get_config_defaults") return Promise.resolve(DEFAULTS);
			if (cmd === "load_agents_config") return Promise.resolve({ agents: agentsConfig });
			// The intent/progress/follow-up overrides render only with the bridge installed.
			if (cmd === "get_agent_mcp_status")
				return Promise.resolve({ supported: true, installed: true, config_path: null, shared_settings_file: false });
			return Promise.resolve(undefined);
		});
	});

	afterEach(() => {
		settingsExpertStore._resetForTests();
		uiStore.setSettingsExpertMode(false);
	});

	it("hides the expert controls at their defaults in basic mode and keeps the basic ones", async () => {
		await setup();
		const { container } = renderExpanded();
		expect(has(container, "Collect project progress")).toBe(false);
		expect(has(container, "Auto-retry on server errors")).toBe(false);
		expect(has(container, "Headless Command Template")).toBe(false);

		expect(has(container, "Show agent intent as tab title")).toBe(true);
		expect(has(container, "Show suggested follow-up actions")).toBe(true);
		expect(has(container, "Run Configurations")).toBe(true);
	});

	it("shows Collect project progress in basic mode once it is turned off", async () => {
		settingsState.progressTracking = false;
		await setup();
		const { container } = render(() => <AgentsTab />);
		expect(has(container, "Collect project progress")).toBe(true);
	});

	it("shows Auto-retry in basic mode once it is turned on for the agent", async () => {
		await setup({ claude: { run_configs: [], auto_retry_on_error: true } });
		const { container } = renderExpanded();
		expect(has(container, "Auto-retry on server errors")).toBe(true);
	});

	it("shows the Headless Command Template in basic mode once the agent overrides it", async () => {
		await setup({ claude: { run_configs: [], headless_template: 'claude -p "{prompt}"' } });
		const { container } = renderExpanded();
		expect(has(container, "Headless Command Template")).toBe(true);
	});

	it("keeps the Headless Command Template hidden while only the built-in template applies", async () => {
		// The input shows AGENTS.claude.defaultHeadlessTemplate as its value; that
		// fallback is not an override and must not count as a modification.
		await setup({ claude: { run_configs: [] } });
		const { container } = renderExpanded();
		expect(has(container, "Headless Command Template")).toBe(false);
	});

	it.each(OVERRIDES)("hides %s in basic mode while the agent has no override", async (label, agent) => {
		await setup();
		const { container } = renderExpanded(agent);
		// Wait for the bridge status, so the gated rows would be rendered if visible.
		await waitFor(() => expect(has(container, "MCP bridge installed")).toBe(true));
		expect(has(container, label)).toBe(false);
	});

	it.each(OVERRIDES)("shows %s in basic mode once the agent stores an override", async (label, agent, override) => {
		await setup({ [agent]: { run_configs: [], ...override } });
		const { container } = renderExpanded(agent);
		await waitFor(() => expect(has(container, label)).toBe(true));
	});

	it("shows every per-agent override at its default in expert mode", async () => {
		await setup();
		uiStore.setSettingsExpertMode(true);
		for (const [label, agent] of OVERRIDES) {
			const { container, unmount } = renderExpanded(agent);
			await waitFor(() => expect(has(container, label), label).toBe(true));
			unmount();
		}
	});

	it("shows every expert control at its default in expert mode", async () => {
		await setup();
		uiStore.setSettingsExpertMode(true);
		const { container } = renderExpanded();
		await waitFor(() => {
			expect(has(container, "Collect project progress")).toBe(true);
			expect(has(container, "Auto-retry on server errors")).toBe(true);
			expect(has(container, "Headless Command Template")).toBe(true);
		});
	});

	it("shows native scrollback prevention for an agent without a CLI flag", async () => {
		await setup();
		uiStore.setSettingsExpertMode(true);
		const { container } = renderExpanded("gemini");
		await waitFor(() => expect(has(container, "Prevent alternate screen")).toBe(true));
		const label = [...container.querySelectorAll("label")].find((el) =>
			el.textContent?.includes("Prevent alternate screen"),
		);
		const checkbox = label?.querySelector("input[type=checkbox]") as HTMLInputElement;
		expect(checkbox.checked).toBe(true);
		fireEvent.click(checkbox);
		expect(agentConfigsStore.getPreventAltScreen("gemini")).toBe(false);
	});

	it("keeps managed trust acceptance on by default and saves an explicit opt-out", async () => {
		await setup();
		uiStore.setSettingsExpertMode(true);
		const { container } = renderExpanded("claude");
		const label = [...container.querySelectorAll("label")].find((el) =>
			el.textContent?.includes("Accept workspace trust for managed spawns"),
		);
		const checkbox = label?.querySelector("input[type=checkbox]") as HTMLInputElement;
		expect(checkbox.checked).toBe(true);
		fireEvent.click(checkbox);
		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith(
				"save_agents_config",
				expect.objectContaining({
					config: expect.objectContaining({
						agents: expect.objectContaining({ claude: expect.objectContaining({ skip_trust_dialog: false }) }),
					}),
				}),
			),
		);
	});
});
