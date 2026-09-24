import { render, waitFor } from "@solidjs/testing-library";
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

vi.mock("../../../../hooks/useAgentDetection", () => ({
	useAgentDetection: () => ({ detectAll: vi.fn(), getAvailable: () => [], loading: () => false }),
}));

import { agentConfigsStore } from "../../../../stores/agentConfigs";
import { settingsExpertStore } from "../../../../stores/settingsExpert";
import { uiStore } from "../../../../stores/ui";
import { SmartPromptsTab } from "../SmartPromptsTab";

// `AgentsConfig::default()` serializes no headless agent; it is not configured.
const DEFAULTS = { app: {}, notifications: {}, agent_settings: {}, agents: { headless_agent: null } };

let headlessAgent: string | undefined;

async function setup(agent?: string) {
	headlessAgent = agent;
	await agentConfigsStore.hydrate();
	await settingsExpertStore.open();
}

const hasHeadlessAgent = (container: HTMLElement) =>
	[...container.querySelectorAll("label")].some((el) => el.textContent === "Headless Agent");

describe("SmartPromptsTab expert controls", () => {
	beforeEach(() => {
		uiStore.setSettingsExpertMode(false);
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "get_config_defaults") return Promise.resolve(DEFAULTS);
			if (cmd === "load_agents_config") return Promise.resolve({ agents: {}, headless_agent: headlessAgent });
			return Promise.resolve(undefined);
		});
	});

	afterEach(() => {
		settingsExpertStore._resetForTests();
		uiStore.setSettingsExpertMode(false);
	});

	it("hides Headless Agent in basic mode while none is configured", async () => {
		await setup();
		const { container, getByText } = render(() => <SmartPromptsTab />);
		expect(getByText("Smart Prompts")).toBeDefined();
		expect(hasHeadlessAgent(container)).toBe(false);
	});

	it("shows Headless Agent in basic mode once one is configured", async () => {
		await setup("claude");
		const { container } = render(() => <SmartPromptsTab />);
		expect(hasHeadlessAgent(container)).toBe(true);
	});

	it("shows Headless Agent in expert mode while none is configured", async () => {
		await setup();
		uiStore.setSettingsExpertMode(true);
		const { container } = render(() => <SmartPromptsTab />);
		await waitFor(() => expect(hasHeadlessAgent(container)).toBe(true));
	});
});
