import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../../__tests__/mocks/tauri";
import { fireEvent, render, waitFor } from "@solidjs/testing-library";

vi.mock("../../../stores/settings", () => ({
	settingsStore: {
		state: {
			ide: "vscode",
			font: "JetBrains Mono",
			defaultFontSize: 12,
			confirmBeforeQuit: true,
			confirmBeforeClosingTab: true,
		},
		setIde: vi.fn(),
		setFont: vi.fn(),
		setConfirmBeforeQuit: vi.fn(),
		setConfirmBeforeClosingTab: vi.fn(),
		isAiChatEnabled: vi.fn().mockReturnValue(false),
	},
	IDE_NAMES: { vscode: "VS Code", cursor: "Cursor" },
	FONT_FAMILIES: { "JetBrains Mono": "JetBrains Mono" },
}));

vi.mock("../../../stores/notifications", () => ({
	notificationsStore: {
		state: {
			isAvailable: true,
			config: {
				enabled: true,
				volume: 0.5,
				sounds: { question: true, error: true, completion: true, warning: true },
				sound_choices: {
					question: { preset: "question", custom_path: null },
					error: { preset: "error", custom_path: null },
					completion: { preset: "completion", custom_path: null },
					warning: { preset: "warning", custom_path: null },
					info: { preset: "info", custom_path: null },
					attention: { preset: "attention", custom_path: null },
				},
			},
		},
		setEnabled: vi.fn(),
		setVolume: vi.fn(),
		setSoundEnabled: vi.fn(),
		testSound: vi.fn(),
		reset: vi.fn(),
	},
}));

vi.mock("../../../stores/ui", () => ({
	uiStore: { state: { settingsNavWidth: 180 }, setSettingsNavWidth: vi.fn(), persistUIPrefs: vi.fn() },
}));

vi.mock("../../../stores/repositories", () => ({
	repositoriesStore: {
		state: { repositories: {}, repoOrder: [] },
		setDisplayName: vi.fn(),
		getGroupForRepo: vi.fn(() => undefined),
		getAllReposOrdered: vi.fn(() => []),
		getConnectionId: vi.fn(() => undefined),
	},
}));

vi.mock("../../../stores/repoSettings", () => ({
	repoSettingsStore: { get: vi.fn(() => undefined), getOrCreate: vi.fn(), update: vi.fn(), reset: vi.fn() },
}));

import { SettingsPanel } from "../SettingsPanel";

const open = () => render(() => <SettingsPanel visible={true} onClose={() => {}} />);

const searchInput = (container: HTMLElement) => container.querySelector("nav input[type='text']") as HTMLInputElement;

const resultRows = (container: HTMLElement) =>
	[...container.querySelectorAll(`[class*="searchResult"] button, button[class*="searchResult"]`)] as HTMLElement[];

describe("SettingsPanel search", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("shows no results until something is typed", () => {
		const { container } = open();
		expect(searchInput(container).value).toBe("");
		expect(resultRows(container)).toHaveLength(0);
		// The active tab is still rendered
		expect(container.textContent).toContain("Power Management");
	});

	it("finds a setting that lives in a tab which was never opened", () => {
		const { container } = open();
		fireEvent.input(searchInput(container), { target: { value: "master volume" } });
		const rows = resultRows(container);
		expect(rows).toHaveLength(1);
		expect(rows[0].textContent).toContain("Master Volume");
		expect(rows[0].textContent).toContain("Notifications");
		// The General tab it replaced is gone while the query stands
		expect(container.textContent).not.toContain("Power Management");
	});

	it("opens the result's tab, clears the query and scrolls to the setting", async () => {
		const { container } = open();
		fireEvent.input(searchInput(container), { target: { value: "master volume" } });
		fireEvent.click(resultRows(container)[0]);

		expect(searchInput(container).value).toBe("");
		const active = container.querySelector("nav button[class*='active']");
		expect(active?.textContent).toBe("Notifications");
		expect(container.textContent).toContain("Notification Settings");

		const setting = [...container.querySelectorAll("label")].find((el) => el.textContent === "Master Volume");
		const heading = [...container.querySelectorAll("h3")].find((h) => h.textContent === "Notification Settings");
		expect(setting).toBeDefined();
		const onSetting = vi.fn();
		const onHeading = vi.fn();
		if (setting) setting.scrollIntoView = onSetting;
		if (heading) heading.scrollIntoView = onHeading;

		// The scroll waits a frame for the new tab body to enter the document
		await waitFor(() => expect(onSetting).toHaveBeenCalled());
		// ...and it lands on the setting, not merely on the section that holds it
		expect(onHeading).not.toHaveBeenCalled();
	});

	it("scrolls to and flashes a deep-linked control (a palette Settings action)", async () => {
		const { container } = render(() => (
			<SettingsPanel
				visible={true}
				onClose={() => {}}
				initialTab="notifications"
				initialTarget={{ section: "Notification Settings", label: "Master Volume" }}
			/>
		));
		const setting = [...container.querySelectorAll("label")].find((el) => el.textContent === "Master Volume");
		expect(setting).toBeDefined();
		const onSetting = vi.fn();
		if (setting) setting.scrollIntoView = onSetting;
		await waitFor(() => expect(onSetting).toHaveBeenCalled());
		expect(setting?.classList.contains("searchHighlight")).toBe(true);
	});

	it("restores the tab list when the query is cleared", () => {
		const { container } = open();
		fireEvent.input(searchInput(container), { target: { value: "master volume" } });
		expect(resultRows(container)).toHaveLength(1);
		fireEvent.input(searchInput(container), { target: { value: "" } });
		expect(resultRows(container)).toHaveLength(0);
		expect(container.textContent).toContain("Power Management");
	});

	it("tells the user when nothing matches", () => {
		const { container } = open();
		fireEvent.input(searchInput(container), { target: { value: "zzzz nothing" } });
		expect(resultRows(container)).toHaveLength(0);
		expect(container.textContent).toContain("No settings match");
	});
});
