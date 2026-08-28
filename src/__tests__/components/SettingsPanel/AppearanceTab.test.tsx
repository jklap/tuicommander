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

/** Find a SettingToggle's checkbox by the visible label text next to it
 *  (SettingFields.tsx renders `<input type="checkbox"/><span>{label}</span>`
 *  as siblings under one `.group` div) — not by position, which breaks
 *  silently whenever a control is inserted ahead of the one under test. */
function toggleByLabel(container: HTMLElement, label: string): HTMLInputElement {
	const span = Array.from(container.querySelectorAll("span")).find((el) => el.textContent === label);
	const checkbox = span?.parentElement?.querySelector('input[type="checkbox"]');
	if (!checkbox) throw new Error(`No toggle found for label "${label}"`);
	return checkbox as HTMLInputElement;
}

/** Find a SettingSlider's range input by its `<label>` text (SettingFields.tsx
 *  renders `<label>{label}</label>` as a sibling of `.slider > input[type=range]`). */
function sliderByLabel(container: HTMLElement, label: string): HTMLInputElement {
	const labelEl = Array.from(container.querySelectorAll("label")).find((el) => el.textContent === label);
	const slider = labelEl?.parentElement?.querySelector('input[type="range"]');
	if (!slider) throw new Error(`No slider found for label "${label}"`);
	return slider as HTMLInputElement;
}

/** Find a SettingSelect's <select> by its <label> text. */
function selectByLabel(container: HTMLElement, label: string): HTMLSelectElement {
	const labelEl = Array.from(container.querySelectorAll("label")).find((el) => el.textContent === label);
	const select = labelEl?.parentElement?.querySelector("select");
	if (!select) throw new Error(`No select found for label "${label}"`);
	return select as HTMLSelectElement;
}

describe("AppearanceTab", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("renders the Theme, Tabs, Repository Groups, Layout, Bell, and UI Legend headings in order", () => {
		const { container } = render(() => <AppearanceTab />);
		const headings = Array.from(container.querySelectorAll("h3")).map((h) => h.textContent);
		expect(headings).toEqual(["Theme", "Tabs", "Repository Groups", "Layout", "Bell", "UI Legend"]);
	});

	it("shows the terminal theme select with the current value", () => {
		const { container } = render(() => <AppearanceTab />);
		const select = selectByLabel(container, "Terminal Theme");
		expect(select.value).toBe("vscode-dark");
	});

	it("calls setTheme when the theme select changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const select = selectByLabel(container, "Terminal Theme");
		fireEvent.change(select, { target: { value: "vscode-light" } });
		expect(mockSetTheme).toHaveBeenCalledWith("vscode-light");
	});

	it("calls setSplitTabMode when the split-tab-mode select changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const select = selectByLabel(container, "Split Tab Mode");
		fireEvent.change(select, { target: { value: "unified" } });
		expect(mockSetSplitTabMode).toHaveBeenCalledWith("unified");
	});

	it("calls setTabOrderingMode when the tab-ordering select changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const select = selectByLabel(container, "Tab Ordering");
		fireEvent.change(select, { target: { value: "free" } });
		expect(mockSetTabOrderingMode).toHaveBeenCalledWith("free");
	});

	it("calls setTabCyclingAllTypes when its toggle changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const checkbox = toggleByLabel(container, "Cycle All Tab Types");
		fireEvent.change(checkbox, { target: { checked: true } });
		expect(mockSetTabCyclingAllTypes).toHaveBeenCalledWith(true);
	});

	it("calls setTabTreeEnabled when its toggle changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const checkbox = toggleByLabel(container, "Nested Terminal Tabs");
		fireEvent.change(checkbox, { target: { checked: false } });
		expect(mockSetTabTreeEnabled).toHaveBeenCalledWith(false);
	});

	it("calls setMaxTabNameLength when the slider changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const slider = sliderByLabel(container, "Max Tab Name Length");
		fireEvent.input(slider, { target: { value: "45" } });
		expect(mockSetMaxTabNameLength).toHaveBeenCalledWith(45);
	});

	it("shows the bell style select with the current value", () => {
		const { container } = render(() => <AppearanceTab />);
		const select = selectByLabel(container, "Bell Style");
		expect(select.value).toBe("visual");
	});

	it("calls setBellStyle when the bell style select changes", () => {
		const { container } = render(() => <AppearanceTab />);
		const select = selectByLabel(container, "Bell Style");
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

	it("calls resetLayout when Reset Panel Sizes is clicked", () => {
		const { getByText } = render(() => <AppearanceTab />);
		fireEvent.click(getByText("Reset Panel Sizes"));
		expect(mockResetLayout).toHaveBeenCalledOnce();
	});
});
