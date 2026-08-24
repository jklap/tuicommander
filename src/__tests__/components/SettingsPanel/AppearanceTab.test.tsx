import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";
import { fireEvent, render } from "@solidjs/testing-library";

const {
	mockSetTheme,
	mockSetSplitTabMode,
	mockSetTabOrderingMode,
	mockSetTabCyclingAllTypes,
	mockSetTabTreeEnabled,
	mockSetMaxTabNameLength,
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
		},
		setTheme: mockSetTheme,
		setSplitTabMode: mockSetSplitTabMode,
		setTabOrderingMode: mockSetTabOrderingMode,
		setTabCyclingAllTypes: mockSetTabCyclingAllTypes,
		setTabTreeEnabled: mockSetTabTreeEnabled,
		setMaxTabNameLength: mockSetMaxTabNameLength,
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

describe("AppearanceTab", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("renders the Theme, Tabs, Repository Groups, Layout, and UI Legend headings in order", () => {
		const { container } = render(() => <AppearanceTab />);
		const headings = Array.from(container.querySelectorAll("h3")).map((h) => h.textContent);
		expect(headings).toEqual(["Theme", "Tabs", "Repository Groups", "Layout", "UI Legend"]);
	});

	it("shows the terminal theme select with the current value", () => {
		const { container } = render(() => <AppearanceTab />);
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		const themeSelect = selects.find((sel) => Array.from(sel.options).some((o) => o.value === "vscode-dark"))!;
		expect(themeSelect.value).toBe("vscode-dark");
	});

	it("calls setTheme when the theme select changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		const themeSelect = selects.find((sel) => Array.from(sel.options).some((o) => o.value === "vscode-light"))!;
		fireEvent.change(themeSelect, { target: { value: "vscode-light" } });
		expect(mockSetTheme).toHaveBeenCalledWith("vscode-light");
	});

	it("calls setSplitTabMode when the split-tab-mode select changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		const select = selects.find((sel) => Array.from(sel.options).some((o) => o.value === "unified"))!;
		fireEvent.change(select, { target: { value: "unified" } });
		expect(mockSetSplitTabMode).toHaveBeenCalledWith("unified");
	});

	it("calls setTabOrderingMode when the tab-ordering select changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		const select = selects.find((sel) => Array.from(sel.options).some((o) => o.value === "free"))!;
		fireEvent.change(select, { target: { value: "free" } });
		expect(mockSetTabOrderingMode).toHaveBeenCalledWith("free");
	});

	it("calls setTabCyclingAllTypes when its toggle changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const checkboxes = Array.from(container.querySelectorAll('input[type="checkbox"]')) as HTMLInputElement[];
		fireEvent.change(checkboxes[0], { target: { checked: true } });
		expect(mockSetTabCyclingAllTypes).toHaveBeenCalledWith(true);
	});

	it("calls setTabTreeEnabled when its toggle changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const checkboxes = Array.from(container.querySelectorAll('input[type="checkbox"]')) as HTMLInputElement[];
		fireEvent.change(checkboxes[1], { target: { checked: false } });
		expect(mockSetTabTreeEnabled).toHaveBeenCalledWith(false);
	});

	it("calls setMaxTabNameLength when the slider changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const slider = container.querySelector('input[type="range"]') as HTMLInputElement;
		fireEvent.input(slider, { target: { value: "45" } });
		expect(mockSetMaxTabNameLength).toHaveBeenCalledWith(45);
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

	it("calls resetLayout when Reset Panel Sizes is clicked", () => {
		const { getByText } = render(() => <AppearanceTab />);
		fireEvent.click(getByText("Reset Panel Sizes"));
		expect(mockResetLayout).toHaveBeenCalledOnce();
	});
});
