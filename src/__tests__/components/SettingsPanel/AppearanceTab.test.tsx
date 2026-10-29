import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";
import { fireEvent, render, waitFor } from "@solidjs/testing-library";

const {
	mockSetTheme,
	mockSetSplitTabMode,
	mockSetTabOrderingMode,
	mockSetTabCyclingAllTypes,
	mockSetTabTreeEnabled,
	mockSetMaxTabNameLength,
	mockSetBellStyle,
	mockSetIndicatorColor,
	mockClearIndicatorOverride,
	mockResetAllIndicators,
	mockResetLayout,
	mockRenameGroup,
	mockDeleteGroup,
	mockSetGroupColor,
	mockCreateGroup,
} = vi.hoisted(() => ({
	mockSetTheme: vi.fn(),
	mockSetSplitTabMode: vi.fn(),
	mockSetTabOrderingMode: vi.fn(),
	mockSetTabCyclingAllTypes: vi.fn(),
	mockSetTabTreeEnabled: vi.fn(),
	mockSetMaxTabNameLength: vi.fn(),
	mockSetBellStyle: vi.fn(),
	mockSetIndicatorColor: vi.fn(),
	mockClearIndicatorOverride: vi.fn(),
	mockResetAllIndicators: vi.fn(),
	mockResetLayout: vi.fn(),
	mockRenameGroup: vi.fn().mockReturnValue(true),
	mockDeleteGroup: vi.fn(),
	mockSetGroupColor: vi.fn(),
	mockCreateGroup: vi.fn(),
}));

vi.mock("../../../stores/settings", () => ({
	settingsStore: {
		state: {
			theme: "vscode-dark",
			splitTabMode: "separate",
			tabOrderingMode: "grouped-by-type",
			tabCyclingAllTypes: false,
			tabTreeEnabled: true,
			maxTabNameLength: 30,
			bellStyle: "visual",
			indicatorOverrides: [],
		},
		setTheme: mockSetTheme,
		setSplitTabMode: mockSetSplitTabMode,
		setTabOrderingMode: mockSetTabOrderingMode,
		setTabCyclingAllTypes: mockSetTabCyclingAllTypes,
		setTabTreeEnabled: mockSetTabTreeEnabled,
		setMaxTabNameLength: mockSetMaxTabNameLength,
		setBellStyle: mockSetBellStyle,
		setIndicatorColor: mockSetIndicatorColor,
		clearIndicatorOverride: mockClearIndicatorOverride,
		resetAllIndicators: mockResetAllIndicators,
	},
}));

vi.mock("../../../stores/ui", () => ({
	uiStore: { resetLayout: mockResetLayout },
}));

vi.mock("../../../stores/repositories", () => ({
	repositoriesStore: {
		state: {
			groupOrder: ["g1"],
			groups: { g1: { id: "g1", name: "Backend", color: "#58a6ff" } },
		},
		renameGroup: mockRenameGroup,
		deleteGroup: mockDeleteGroup,
		setGroupColor: mockSetGroupColor,
		createGroup: mockCreateGroup,
	},
}));

vi.mock("../../../themes", () => ({
	getThemeNames: () => ({ "vscode-dark": "VS Code Dark", "vscode-light": "VS Code Light" }),
}));

import { AppearanceTab } from "../../../components/SettingsPanel/tabs/AppearanceTab";
import { settingControl } from "../../helpers/settingControl";

describe("AppearanceTab", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("renders the Tabs, Chat, Repository Groups, Layout, Bell, and UI Legend headings in order", () => {
		const { container } = render(() => <AppearanceTab />);
		const headings = Array.from(container.querySelectorAll("h3")).map((h) => h.textContent);
		// The terminal theme select lives in the Terminal tab (main moved it there), not
		// here; main's Chat section (new-text indicator, muted intermediate text) follows Tabs.
		expect(headings).toEqual(["Tabs", "Chat", "Repository Groups", "Layout", "Bell", "UI Legend"]);
	});

	it("calls setSplitTabMode when the split-tab-mode select changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const select = settingControl<HTMLSelectElement>(container, "app.split_tab_mode");
		fireEvent.change(select, { target: { value: "unified" } });
		expect(mockSetSplitTabMode).toHaveBeenCalledWith("unified");
	});

	it("calls setTabOrderingMode when the tab-ordering select changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const select = settingControl<HTMLSelectElement>(container, "app.tab_ordering_mode");
		fireEvent.change(select, { target: { value: "free" } });
		expect(mockSetTabOrderingMode).toHaveBeenCalledWith("free");
	});

	it("calls setTabCyclingAllTypes when its toggle changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const checkbox = settingControl<HTMLInputElement>(container, "app.tab_cycling_all_types");
		fireEvent.change(checkbox, { target: { checked: true } });
		expect(mockSetTabCyclingAllTypes).toHaveBeenCalledWith(true);
	});

	it("calls setTabTreeEnabled when its toggle changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const checkbox = settingControl<HTMLInputElement>(container, "app.tab_tree_enabled");
		fireEvent.change(checkbox, { target: { checked: false } });
		expect(mockSetTabTreeEnabled).toHaveBeenCalledWith(false);
	});

	it("calls setMaxTabNameLength when the slider changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const slider = settingControl<HTMLInputElement>(container, "app.max_tab_name_length");
		fireEvent.input(slider, { target: { value: "45" } });
		expect(mockSetMaxTabNameLength).toHaveBeenCalledWith(45);
	});

	it("shows the bell style select with the current value", () => {
		const { container } = render(() => <AppearanceTab />);
		const select = settingControl<HTMLSelectElement>(container, "app.bell_style");
		expect(select.value).toBe("visual");
	});

	it("calls setBellStyle when the bell style select changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const select = settingControl<HTMLSelectElement>(container, "app.bell_style");
		fireEvent.change(select, { target: { value: "both" } });
		expect(mockSetBellStyle).toHaveBeenCalledWith("both");
	});

	it("renders existing repository groups and adds a new one via the Add Group button", () => {
		const { getByText } = render(() => <AppearanceTab />);
		expect(getByText("Backend")).toBeTruthy();
		fireEvent.click(getByText("Add Group"));
		expect(mockCreateGroup).toHaveBeenCalledWith("New Group");
	});

	it("calls deleteGroup when a group's delete button is clicked", () => {
		const { getByTitle } = render(() => <AppearanceTab />);
		fireEvent.click(getByTitle("Delete group"));
		expect(mockDeleteGroup).toHaveBeenCalledWith("g1");
	});

	it("calls resetLayout when Reset Sidebar Widths is clicked", () => {
		const { getByText } = render(() => <AppearanceTab />);
		fireEvent.click(getByText("Reset Sidebar Widths"));
		expect(mockResetLayout).toHaveBeenCalledOnce();
	});

	it("focuses the rename input on double-click (autofocus is inert on this long-lived, Show-gated item)", async () => {
		const { getByText, container } = render(() => <AppearanceTab />);
		const findRenameInput = () => Array.from(container.querySelectorAll("input")).find((el) => el.value === "Backend");
		expect(findRenameInput()).toBeUndefined();

		fireEvent.dblClick(getByText("Backend"));

		const renameInput = await waitFor(() => {
			const input = findRenameInput();
			expect(input).not.toBeUndefined();
			return input!;
		});
		await waitFor(() => expect(document.activeElement).toBe(renameInput));
	});
});
