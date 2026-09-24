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
const DEFAULTS = {
	app: { progress_tracking: true },
	notifications: {},
	agent_settings: { run_configs: [], auto_retry_on_error: false, headless_template: null },
};

let agentsConfig: Record<string, unknown> = {};

async function setup(agents: Record<string, unknown> = {}) {
	agentsConfig = agents;
	await agentConfigsStore.hydrate();
	await settingsExpertStore.open();
}

/** Render the tab and expand Claude's row, where the per-agent controls live. */
function renderExpanded() {
	const result = render(() => <AgentsTab />);
	const header = [...result.container.querySelectorAll("[role='button']")].find((el) =>
		el.textContent?.includes(AGENTS.claude.name),
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
});
