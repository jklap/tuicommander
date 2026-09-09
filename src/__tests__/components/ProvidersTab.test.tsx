import { fireEvent, render } from "@solidjs/testing-library";
import { Suspense } from "solid-js";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";

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

describe("ProvidersTab", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockStore.state.registry.providers = [];
		mockStore.state.registry.models = [];
		mockStore.state.registry.slots = {};
		mockStore.state.keyStatus = {};
		mockInvoke.mockResolvedValue(undefined);
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

	// -- Ollama availability --

	const ollamaProvider = { id: "ollama-local", type: "ollama", label: "Ollama", base_url: null };

	function renderWithOllamaStatus(status: unknown) {
		mockStore.state.registry.providers = [ollamaProvider] as unknown as (typeof anthropic)[];
		mockInvoke.mockImplementation(async (cmd: string) => (cmd === "check_ollama_models" ? status : undefined));
		return render(() => <ProvidersTab />);
	}

	it("renders a positive availability indicator when the provider is reachable", async () => {
		const { findByTestId, queryByTestId } = renderWithOllamaStatus({
			available: true,
			models: [{ name: "llama3.3:8b", size: 1 }],
			detail: null,
		});
		const badge = await findByTestId("availability-ollama-local");
		expect(badge.dataset.available).toBe("true");
		expect(badge.textContent).toContain("Reachable");
		// No reason line: nothing is wrong.
		expect(queryByTestId("availability-detail-ollama-local")).toBeNull();
	});

	it("renders a negative indicator plus the backend's reason when the provider is unreachable", async () => {
		const { findByTestId } = renderWithOllamaStatus({
			available: false,
			models: [],
			detail: "Cannot reach http://localhost:11434 — is Ollama running?",
		});
		const badge = await findByTestId("availability-ollama-local");
		expect(badge.dataset.available).toBe("false");
		expect(badge.textContent).toContain("Not detected");
		const detail = await findByTestId("availability-detail-ollama-local");
		expect(detail.textContent).toBe("Cannot reach http://localhost:11434 — is Ollama running?");
	});

	it("draws the indicator as an inline currentColor SVG, never an emoji", async () => {
		for (const available of [true, false]) {
			const { findByTestId, unmount } = renderWithOllamaStatus({
				available,
				models: [],
				detail: available ? null : "Cannot reach http://localhost:11434 — is Ollama running?",
			});
			const badge = await findByTestId("availability-ollama-local");
			const icon = badge.querySelector("svg");
			expect(icon).toBeTruthy();
			expect(icon?.getAttribute("fill")).toBe("currentColor");
			expect(badge.textContent).not.toMatch(/\p{Extended_Pictographic}/u);
			unmount();
		}
	});

	it("shows no availability badge for providers with no detection", async () => {
		mockStore.state.registry.providers = [anthropic];
		const { queryByTestId } = render(() => <ProvidersTab />);
		await Promise.resolve();
		expect(queryByTestId("availability-anthropic-main")).toBeNull();
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
