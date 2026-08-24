import { fireEvent, render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

const { mockSetEnabled, mockUninstall, mockFetch, mockRefresh, mockHasUpdate, mockSetAutoUpdatePluginsEnabled } =
	vi.hoisted(() => ({
		mockSetEnabled: vi.fn().mockResolvedValue(undefined),
		mockUninstall: vi.fn().mockResolvedValue(undefined),
		mockFetch: vi.fn().mockResolvedValue(undefined),
		mockRefresh: vi.fn().mockResolvedValue(undefined),
		mockHasUpdate: vi.fn().mockReturnValue(null),
		mockSetAutoUpdatePluginsEnabled: vi.fn(),
	}));

// isTauri() false throughout — keeps the README-lazy-fetch and file-install-dialog
// branches (which call real Tauri plugins) out of scope for this render-level test.
vi.mock("../../../transport", () => ({ isTauri: () => false }));

vi.mock("../../../stores/settings", () => ({
	settingsStore: {
		state: { autoUpdatePluginsEnabled: true },
		setAutoUpdatePluginsEnabled: mockSetAutoUpdatePluginsEnabled,
	},
}));

let pluginList: import("../../../stores/pluginStore").PluginState[] = [];

vi.mock("../../../stores/pluginStore", () => ({
	pluginStore: {
		getAll: () => pluginList,
		getPlugin: (id: string) => pluginList.find((p) => p.id === id),
		setEnabled: mockSetEnabled,
		uninstall: mockUninstall,
		installFromUrl: vi.fn(),
		installFromZip: vi.fn(),
		installFromFolder: vi.fn(),
	},
}));

vi.mock("../../../stores/registryStore", () => ({
	registryStore: {
		state: { loading: false, error: null, entries: [] },
		fetch: mockFetch,
		refresh: mockRefresh,
		hasUpdate: mockHasUpdate,
	},
}));

vi.mock("../../../plugins/dashboardRegistry", () => ({
	dashboardRegistry: { get: () => undefined },
}));

import { PluginsTab } from "../../../components/SettingsPanel/tabs/PluginsTab";
import { PluginLogger } from "../../../plugins/pluginLogger";
import type { PluginState } from "../../../stores/pluginStore";

function makePlugin(overrides: Partial<PluginState> = {}): PluginState {
	return {
		id: "csv-preview",
		manifest: { name: "CSV Preview", version: "1.0.0", description: "Preview CSV files", capabilities: [] } as never,
		builtIn: false,
		enabled: true,
		loaded: true,
		paused: false,
		error: null,
		logger: new PluginLogger(),
		...overrides,
	};
}

describe("PluginsTab", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		pluginList = [];
	});

	it("renders the Plugins heading and the Installed/Browse sub-tabs", () => {
		const { getByText } = render(() => <PluginsTab />);
		expect(getByText("Plugins")).toBeTruthy();
		expect(getByText("Installed")).toBeTruthy();
		expect(getByText("Browse")).toBeTruthy();
	});

	it("shows the empty-state message when no plugins are installed", () => {
		const { getByText } = render(() => <PluginsTab />);
		expect(getByText("No plugins installed. Use the buttons below to install from a folder or ZIP file.")).toBeTruthy();
	});

	it("renders an installed plugin's name, version, and enabled toggle", () => {
		pluginList = [makePlugin({ id: "csv-preview", enabled: true })];
		const { getByText, container } = render(() => <PluginsTab />);
		expect(getByText("CSV Preview")).toBeTruthy();
		expect(getByText("v1.0.0")).toBeTruthy();
		const checkbox = container.querySelector(".pluginActions input[type=checkbox]") as HTMLInputElement;
		expect(checkbox.checked).toBe(true);
	});

	it("shows a Built-in badge for built-in plugins and no Uninstall button", () => {
		pluginList = [makePlugin({ id: "core", builtIn: true })];
		const { getByText, queryByText } = render(() => <PluginsTab />);
		expect(getByText("Built-in")).toBeTruthy();
		expect(queryByText("Uninstall")).toBeNull();
	});

	it("calls pluginStore.setEnabled when a plugin's toggle changes", async () => {
		pluginList = [makePlugin({ id: "csv-preview", enabled: true })];
		const { container } = render(() => <PluginsTab />);
		// The plugin row's own toggle, not the "Check for plugin updates" SettingToggle above it.
		const checkbox = container.querySelector(".pluginActions input[type=checkbox]") as HTMLInputElement;
		fireEvent.change(checkbox, { target: { checked: false } });
		await Promise.resolve();
		expect(mockSetEnabled).toHaveBeenCalledWith("csv-preview", false);
	});

	it("shows an error-count badge when the plugin's logger has recorded errors", () => {
		const logger = new PluginLogger();
		logger.error("boom");
		logger.error("boom again");
		pluginList = [makePlugin({ logger })];
		const { getByText } = render(() => <PluginsTab />);
		expect(getByText("2 errors")).toBeTruthy();
	});

	it("calls setAutoUpdatePluginsEnabled when the auto-update toggle changes", () => {
		const { getByText } = render(() => <PluginsTab />);
		const label = getByText("Check for plugin updates");
		const checkbox = label.closest(".toggle")?.querySelector('input[type="checkbox"]') as HTMLInputElement;
		fireEvent.change(checkbox, { target: { checked: false } });
		expect(mockSetAutoUpdatePluginsEnabled).toHaveBeenCalledWith(false);
	});

	it("switches to the Browse sub-tab and fetches the registry", () => {
		const { getByText, queryByText } = render(() => <PluginsTab />);
		fireEvent.click(getByText("Browse"));
		expect(mockFetch).toHaveBeenCalled();
		expect(getByText("Discover plugins from the community registry.")).toBeTruthy();
		expect(queryByText("Check for plugin updates")).toBeNull();
	});

	it("shows the registry-empty message on Browse when the registry has no entries", () => {
		const { getByText } = render(() => <PluginsTab />);
		fireEvent.click(getByText("Browse"));
		expect(getByText("No plugins available in the registry yet.")).toBeTruthy();
	});
});
