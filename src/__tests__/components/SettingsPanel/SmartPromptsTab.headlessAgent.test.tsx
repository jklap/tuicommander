import { render, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

vi.mock("../../../stores/promptLibrary", () => ({
	promptLibraryStore: {
		getAllPrompts: () => [],
		updatePrompt: vi.fn(),
		createPrompt: vi.fn(),
		deletePrompt: vi.fn(),
		resetToDefault: vi.fn(),
		isOverridden: () => false,
		hasUpdate: () => false,
	},
}));

vi.mock("../../../data/smartPromptsBuiltIn", () => ({ SMART_PROMPTS_BUILTIN: [] }));

// Backs the mocked `useAgentDetection` below — starts empty on every render
// (matching the real hook's behavior before its async `detectAll()` IPC call
// resolves), and lets a test flip an agent to "available" afterward to
// reproduce detection resolving after mount.
//
// The signal itself is created inside the async mock factory (via a dynamic
// `import("solid-js")`, which resolves to the same module instance the rest
// of the app uses) rather than up here at the top of the file — creating it
// with a plain top-level `import` binding instead compiles to a *different*
// solid-js module instance under this project's Vite/Vitest config, so
// `<For>` silently never re-renders: the signal updates, but against a
// reactive-tracking context `<For>` isn't reading from. `detectionBox` only
// holds the resulting accessors (mutated in place by the factory) and is
// created via vi.hoisted so it exists before the mocked import resolves — a
// plain module-scope `let` hits a TDZ ReferenceError there instead.
const detectionBox = vi.hoisted(() => ({
	availableAgentTypes: (): string[] => [],
	setAvailableAgentTypes: (_types: string[]) => {},
}));

vi.mock("../../../hooks/useAgentDetection", async () => {
	const { createSignal } = await import("solid-js");
	const [availableAgentTypes, setAvailableAgentTypes] = createSignal<string[]>([]);
	detectionBox.availableAgentTypes = availableAgentTypes;
	detectionBox.setAvailableAgentTypes = setAvailableAgentTypes;
	return {
		useAgentDetection: () => ({
			detectAll: vi.fn().mockResolvedValue(undefined),
			getAvailable: () =>
				detectionBox
					.availableAgentTypes()
					.map((type: string) => ({ type, available: true, path: "/usr/bin/x", version: null })),
			loading: () => false,
		}),
	};
});

function setAvailableAgentTypes(types: string[]) {
	detectionBox.setAvailableAgentTypes(types);
}

import { SmartPromptsTab } from "../../../components/SettingsPanel/tabs/SmartPromptsTab";
import { agentConfigsStore } from "../../../stores/agentConfigs";

function headlessSelect(container: HTMLElement): HTMLSelectElement {
	return Array.from(container.querySelectorAll("select")).find((sel) =>
		Array.from(sel.options).some((o) => o.textContent === "— Not configured —"),
	) as HTMLSelectElement;
}

describe("SmartPromptsTab — Headless Agent select", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		setAvailableAgentTypes([]);
		agentConfigsStore.setHeadlessAgent(null);
	});

	it("shows the persisted headless agent once async agent detection resolves after mount", async () => {
		// Regression: same root cause as ProvidersTab's headless-agent select —
		// a `value=` binding on the <select> only applies once, before any
		// option exists (detection is async and starts empty), so the stored
		// selection was silently lost and the select got stuck showing
		// "— Not configured —". Fixed by binding `selected` per-option.
		agentConfigsStore.setHeadlessAgent("claude");
		const { container } = render(() => <SmartPromptsTab />);
		const select = headlessSelect(container);

		setAvailableAgentTypes(["claude"]);
		await waitFor(() => expect(select.value).toBe("claude"));
	});

	it("persists a named run config's composite value instead of discarding it back to 'not configured'", async () => {
		// Regression: the onChange handler guarded with
		// `AGENT_TYPES.includes(val)`, but named run configs render as
		// composite "agentType:configName" values, which aren't in
		// AGENT_TYPES — so selecting one silently reset the setting to null.
		agentConfigsStore.setHeadlessAgent(null);
		setAvailableAgentTypes(["claude"]);
		await agentConfigsStore.addRunConfig("claude", {
			name: "My Config",
			command: "claude",
			args: ["{prompt}"],
			env: {},
			is_default: false,
		});

		const { container } = render(() => <SmartPromptsTab />);
		const select = headlessSelect(container);
		await waitFor(() => expect(select.querySelector('option[value="claude:My Config"]')).toBeTruthy());

		select.value = "claude:My Config";
		select.dispatchEvent(new Event("change", { bubbles: true }));

		expect(agentConfigsStore.getHeadlessAgent()).toBe("claude:My Config");
	});
});
