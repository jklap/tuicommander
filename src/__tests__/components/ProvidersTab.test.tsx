import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { Suspense } from "solid-js";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";

// Backs the mocked `useAgentDetection` below — starts empty on every render
// (matching the real hook's behavior before its async `detectAll()` IPC call
// resolves), and lets a test flip an agent to "available" afterward to
// reproduce detection resolving after mount.
//
// The signal itself is created inside the async mock factory (via a dynamic
// `import("solid-js")`, which resolves to the same module instance the rest
// of the app uses) rather than up here at the top of the file — creating it
// here with a plain top-level `import` binding instead compiles to a
// *different* solid-js module instance under this project's Vite/Vitest
// config, so `<For>` silently never re-renders: the signal updates, but
// against a reactive-tracking context `<For>` isn't reading from. `detectBox`
// only holds the resulting accessors (mutated in place by the factory) and
// is created via vi.hoisted so it exists before the mocked import resolves —
// a plain module-scope `let` hits a TDZ ReferenceError there instead.
const detectionBox = vi.hoisted(() => ({
	availableAgentTypes: (): string[] => [],
	setAvailableAgentTypes: (_types: string[]) => {},
}));

vi.mock("../../hooks/useAgentDetection", async () => {
	const { createSignal } = await import("solid-js");
	const [availableAgentTypes, setAvailableAgentTypes] = createSignal<string[]>([]);
	detectionBox.availableAgentTypes = availableAgentTypes;
	detectionBox.setAvailableAgentTypes = setAvailableAgentTypes;
	return {
		useAgentDetection: () => ({
			detectAll: vi.fn().mockResolvedValue(undefined),
			isAvailable: (type: string) => detectionBox.availableAgentTypes().includes(type),
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

const anthropic = {
	id: "anthropic-main",
	type: "anthropic" as const,
	label: "Anthropic",
	base_url: null,
};

const sonnet = {
	id: "model-sonnet",
	provider_id: "anthropic-main",
	model_name: "claude-sonnet-4-5",
	tier: "standard" as const,
};

const mockStore = vi.hoisted(() => ({
	state: {
		registry: {
			schema_version: 1,
			providers: [] as (typeof anthropic)[],
			models: [] as (typeof sonnet)[],
			slots: {} as Record<string, string>,
			features: {},
		},
		keyStatus: {} as Record<string, boolean>,
		loaded: true,
	},
	addProvider: vi.fn(),
	removeProvider: vi.fn(),
	addModel: vi.fn(),
	removeModel: vi.fn(),
	setSlot: vi.fn(),
	clearSlot: vi.fn(),
	saveKey: vi.fn(),
	deleteKey: vi.fn(),
	resolveSlot: vi.fn(() => null),
	_reset: vi.fn(),
}));

vi.mock("../../stores/providerRegistry", () => ({
	providerRegistryStore: mockStore,
}));

import { ProvidersTab } from "../../components/SettingsPanel/tabs/ProvidersTab";
import { agentConfigsStore } from "../../stores/agentConfigs";

describe("ProvidersTab", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockStore.state.registry.providers = [];
		mockStore.state.registry.models = [];
		mockStore.state.registry.slots = {};
		mockStore.state.keyStatus = {};
		mockInvoke.mockResolvedValue(undefined);
		setAvailableAgentTypes([]);
		agentConfigsStore.setHeadlessAgent(null);
	});

	// -- Provider list --

	it("renders empty state when no providers", () => {
		const { getByText } = render(() => <ProvidersTab />);
		expect(getByText(/No providers configured/)).toBeTruthy();
	});

	it("renders provider cards for each provider", () => {
		mockStore.state.registry.providers = [anthropic];
		const { getByTestId } = render(() => <ProvidersTab />);
		expect(getByTestId("provider-card-anthropic-main")).toBeTruthy();
	});

	it("shows provider label and type", () => {
		mockStore.state.registry.providers = [anthropic];
		const { getByText } = render(() => <ProvidersTab />);
		expect(getByText("Anthropic")).toBeTruthy();
	});

	it("shows model count", () => {
		mockStore.state.registry.providers = [anthropic];
		mockStore.state.registry.models = [sonnet];
		const { getByText } = render(() => <ProvidersTab />);
		expect(getByText(/Models \(1\)/)).toBeTruthy();
	});

	it("shows key status indicator", () => {
		mockStore.state.registry.providers = [anthropic];
		mockStore.state.keyStatus = { "anthropic-main": true };
		const { getByTestId } = render(() => <ProvidersTab />);
		expect(getByTestId("key-status-anthropic-main").textContent).toContain("✓ key");
	});

	it("shows 'no key' when key missing", () => {
		mockStore.state.registry.providers = [anthropic];
		mockStore.state.keyStatus = { "anthropic-main": false };
		const { getByTestId } = render(() => <ProvidersTab />);
		expect(getByTestId("key-status-anthropic-main").textContent).toContain("no key");
	});

	// -- Remove provider --

	it("calls removeProvider when × clicked", () => {
		mockStore.state.registry.providers = [anthropic];
		const { getByTestId } = render(() => <ProvidersTab />);
		fireEvent.click(getByTestId("remove-provider-anthropic-main"));
		expect(mockStore.removeProvider).toHaveBeenCalledWith("anthropic-main");
	});

	// -- Add provider form --

	it("shows add provider form when + Add clicked", () => {
		const { getByTestId, getByText } = render(() => <ProvidersTab />);
		fireEvent.click(getByTestId("add-provider-btn"));
		expect(getByTestId("add-provider-form")).toBeTruthy();
		expect(getByText("Add Provider")).toBeTruthy();
	});

	it("cancels add provider form", () => {
		const { getByTestId, queryByTestId, getByText } = render(() => <ProvidersTab />);
		fireEvent.click(getByTestId("add-provider-btn"));
		fireEvent.click(getByText("Cancel"));
		expect(queryByTestId("add-provider-form")).toBeNull();
	});

	// -- Model CRUD --

	it("renders model entries", () => {
		mockStore.state.registry.providers = [anthropic];
		mockStore.state.registry.models = [sonnet];
		const { getByTestId } = render(() => <ProvidersTab />);
		expect(getByTestId("model-entry-model-sonnet")).toBeTruthy();
	});

	it("calls removeModel when model × clicked", () => {
		mockStore.state.registry.providers = [anthropic];
		mockStore.state.registry.models = [sonnet];
		const { getByTestId } = render(() => <ProvidersTab />);
		fireEvent.click(getByTestId("remove-model-model-sonnet"));
		expect(mockStore.removeModel).toHaveBeenCalledWith("model-sonnet");
	});

	it("shows add model form when + Add model clicked", () => {
		mockStore.state.registry.providers = [anthropic];
		const { getByTestId } = render(() => <ProvidersTab />);
		fireEvent.click(getByTestId("add-model-btn-anthropic-main"));
		expect(getByTestId("add-model-form")).toBeTruthy();
	});

	// -- Slot assignments --

	it("renders slot assignment section", () => {
		const { getByTestId } = render(() => <ProvidersTab />);
		expect(getByTestId("slot-assignments")).toBeTruthy();
	});

	it("renders all 3 slot rows", () => {
		const { getByTestId } = render(() => <ProvidersTab />);
		for (const slot of ["main", "triage", "headless"]) {
			expect(getByTestId(`slot-row-${slot}`)).toBeTruthy();
		}
		// headless slot-select is only shown when External API is active
		for (const slot of ["main", "triage"]) {
			expect(getByTestId(`slot-select-${slot}`)).toBeTruthy();
		}
	});

	it("shows the persisted headless agent once async agent detection resolves after mount", async () => {
		// Regression: the headless-agent <select>'s options come from
		// useAgentDetection, which starts empty and resolves asynchronously.
		// A `value=` binding on the <select> itself only applies once, before
		// any option exists, so it silently loses the stored selection — the
		// browser falls back to the first inserted option ("— Not configured
		// —") and it never re-syncs. The fix binds `selected` per-option so a
		// late-arriving option still lands correctly.
		agentConfigsStore.setHeadlessAgent("claude");
		const { getByTestId } = render(() => <ProvidersTab />);
		const select = getByTestId("slot-row-headless").querySelector("select") as HTMLSelectElement;

		setAvailableAgentTypes(["claude"]);
		await waitFor(() => expect(select.value).toBe("claude"));
	});

	it("calls setSlot when slot dropdown changes", () => {
		mockStore.state.registry.providers = [anthropic];
		mockStore.state.registry.models = [sonnet];
		const { getByTestId } = render(() => <ProvidersTab />);
		fireEvent.change(getByTestId("slot-select-main"), { target: { value: "model-sonnet" } });
		expect(mockStore.setSlot).toHaveBeenCalledWith("main", "model-sonnet");
	});

	it("calls clearSlot when empty option selected", () => {
		mockStore.state.registry.providers = [anthropic];
		mockStore.state.registry.models = [sonnet];
		mockStore.state.registry.slots = { main: "model-sonnet" };
		const { getByTestId } = render(() => <ProvidersTab />);
		fireEvent.change(getByTestId("slot-select-main"), { target: { value: "" } });
		expect(mockStore.clearSlot).toHaveBeenCalledWith("main");
	});

	it("shows test button when slot is configured", () => {
		mockStore.state.registry.providers = [anthropic];
		mockStore.state.registry.models = [sonnet];
		mockStore.state.registry.slots = { main: "model-sonnet" };
		const { getByTestId } = render(() => <ProvidersTab />);
		expect(getByTestId("test-slot-main")).toBeTruthy();
	});

	// -- Suspense isolation --

	it("does not collapse an ancestor Suspense while ollama models load", async () => {
		const ollama = { id: "ollama-local", type: "ollama", label: "Ollama", base_url: null };
		mockStore.state.registry.providers = [ollama] as unknown as (typeof anthropic)[];
		// Keep every invoke (incl. check_ollama_models) pending forever — the
		// tab must still render instead of suspending the whole settings dialog.
		let resolveInvoke!: (value: unknown) => void;
		mockInvoke.mockReturnValue(
			new Promise((resolve) => {
				resolveInvoke = resolve;
			}),
		);
		const { queryByTestId } = render(() => (
			<Suspense fallback={<div data-testid="suspense-fallback" />}>
				<ProvidersTab />
			</Suspense>
		));
		expect(queryByTestId("suspense-fallback")).toBeNull();
		expect(queryByTestId("provider-card-ollama-local")).toBeTruthy();
		resolveInvoke(undefined);
		await Promise.resolve();
	});
});
