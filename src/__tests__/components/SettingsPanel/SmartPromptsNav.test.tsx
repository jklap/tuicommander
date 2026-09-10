import { fireEvent, render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";
import type { SavedPrompt } from "../../../stores/promptLibrary";

/** One smart prompt, enough to exercise the row → editor path */
const SMART_PROMPT: SavedPrompt = {
	id: "sp-test",
	name: "Test Smart Prompt",
	content: "Do the thing",
	category: "custom",
	isFavorite: false,
	tags: ["smart", "git"],
	enabled: true,
	placement: ["toolbar"],
	executionMode: "inject",
	injectTarget: "terminal",
	autoExecute: false,
	createdAt: 0,
	updatedAt: 0,
};

const updatePrompt = vi.fn();

vi.mock("../../../stores/promptLibrary", () => ({
	promptLibraryStore: {
		getAllPrompts: () => [SMART_PROMPT],
		updatePrompt: (...args: unknown[]) => updatePrompt(...args),
		createPrompt: vi.fn(),
		deletePrompt: vi.fn(),
		resetToDefault: vi.fn(),
		isOverridden: () => false,
		hasUpdate: () => false,
	},
}));

vi.mock("../../../data/smartPromptsBuiltIn", () => ({ SMART_PROMPTS_BUILTIN: [] }));

vi.mock("../../../hooks/useAgentDetection", () => ({
	useAgentDetection: () => ({
		detectAll: vi.fn(),
		getAvailable: () => [],
		loading: () => false,
	}),
}));

vi.mock("../../../stores/agentConfigs", () => ({
	agentConfigsStore: {
		getHeadlessAgent: () => null,
		setHeadlessAgent: vi.fn(),
		getRunConfigs: () => [],
	},
}));

vi.mock("../../../stores/settings", () => ({
	settingsStore: {
		state: { ide: "vscode", font: "JetBrains Mono", defaultFontSize: 12 },
		isAiChatEnabled: () => false,
	},
	IDE_NAMES: { vscode: "VS Code" },
	FONT_FAMILIES: { "JetBrains Mono": "JetBrains Mono" },
}));

vi.mock("../../../stores/ui", () => ({
	uiStore: {
		state: { settingsNavWidth: 180 },
		setSettingsNavWidth: vi.fn(),
		persistUIPrefs: vi.fn(),
	},
}));

vi.mock("../../../stores/repositories", () => ({
	repositoriesStore: {
		state: { repositories: {}, repoOrder: [] },
		getAllReposOrdered: () => [],
		getConnectionId: () => undefined,
		setDisplayName: vi.fn(),
	},
}));

import { SettingsPanel } from "../../../components/SettingsPanel/SettingsPanel";

const navLabels = (container: HTMLElement) =>
	Array.from(container.querySelectorAll(".navItem")).map((n) => n.textContent);

const clickNav = (container: HTMLElement, label: string) => {
	const item = Array.from(container.querySelectorAll(".navItem")).find((n) => n.textContent === label);
	expect(item, `nav item "${label}" not found`).toBeTruthy();
	fireEvent.click(item!);
};

describe("SettingsPanel — Smart Prompts navigation", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("registers a Smart Prompts nav item", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		expect(navLabels(container)).toContain("Smart Prompts");
	});

	it("renders the prompt list and editor controls when the tab is selected", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		clickNav(container, "Smart Prompts");

		const heading = container.querySelector(".section h3");
		expect(heading?.textContent).toBe("Smart Prompts");
		expect(container.textContent).toContain("Test Smart Prompt");
	});

	it("opens directly on the Smart Prompts tab when requested via initialTab", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} initialTab="smart-prompts" />);
		const active = container.querySelector(".navItem.active");
		expect(active?.textContent).toBe("Smart Prompts");
		expect(container.querySelector(".section h3")?.textContent).toBe("Smart Prompts");
	});

	it("exposes editable Execution Mode and Auto-execute controls in the expanded editor", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} initialTab="smart-prompts" />);

		// Expand the prompt row to reveal the editor
		const header = Array.from(container.querySelectorAll('[role="button"]')).find((el) =>
			el.textContent?.includes("Test Smart Prompt"),
		);
		expect(header).toBeTruthy();
		fireEvent.click(header!);

		const labels = Array.from(container.querySelectorAll("label")).map((l) => l.textContent);
		expect(labels).toContain("Execution Mode");
		expect(labels).toContain("Auto-execute");

		// Execution Mode is editable
		const modeSelect = Array.from(container.querySelectorAll("select")).find((sel) =>
			Array.from(sel.options).some((o) => o.value === "headless"),
		);
		expect(modeSelect).toBeTruthy();
		fireEvent.change(modeSelect!, { target: { value: "headless" } });
		expect(updatePrompt).toHaveBeenCalledWith("sp-test", expect.objectContaining({ executionMode: "headless" }));

		// Auto-execute is editable
		updatePrompt.mockClear();
		const checkbox = Array.from(container.querySelectorAll('input[type="checkbox"]')).find((cb) =>
			cb.closest("label")?.textContent?.includes("Send immediately"),
		) as HTMLInputElement | undefined;
		expect(checkbox).toBeTruthy();
		fireEvent.change(checkbox!, { target: { checked: true } });
		expect(updatePrompt).toHaveBeenCalledWith("sp-test", expect.objectContaining({ autoExecute: true }));
	});

	it("offers an Auto target option alongside Compose and Terminal, and reflects the prompt's current target", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} initialTab="smart-prompts" />);
		const header = Array.from(container.querySelectorAll('[role="button"]')).find((el) =>
			el.textContent?.includes("Test Smart Prompt"),
		);
		fireEvent.click(header!);

		const targetSelect = Array.from(container.querySelectorAll("select")).find((sel) =>
			Array.from(sel.options).some((o) => o.value === "auto"),
		) as HTMLSelectElement | undefined;
		expect(targetSelect).toBeTruthy();
		expect(Array.from(targetSelect!.options).map((o) => o.value)).toEqual(["auto", "compose", "terminal"]);
		// SMART_PROMPT fixture has injectTarget: "terminal" explicitly set.
		expect(targetSelect!.value).toBe("terminal");

		fireEvent.change(targetSelect!, { target: { value: "auto" } });
		expect(updatePrompt).toHaveBeenCalledWith("sp-test", expect.objectContaining({ injectTarget: "auto" }));
	});

	it("shows the Auto-execute control for every inject target, not just explicit Terminal", () => {
		// Regression: this control used to be hidden unless injectTarget was the
		// literal "terminal", even though autoExecute is meaningful (and honored
		// by shouldSubmitInjectPrompt) for "auto" and "compose" targets too, and
		// the Prompt Library dialog's own editor already showed it unconditionally
		// for any inject-mode prompt — the two editors must agree.
		const original = SMART_PROMPT.injectTarget;
		SMART_PROMPT.injectTarget = "auto";
		try {
			const { container } = render(() => (
				<SettingsPanel visible={true} onClose={() => {}} initialTab="smart-prompts" />
			));
			const header = Array.from(container.querySelectorAll('[role="button"]')).find((el) =>
				el.textContent?.includes("Test Smart Prompt"),
			);
			fireEvent.click(header!);

			const labels = Array.from(container.querySelectorAll("label")).map((l) => l.textContent);
			expect(labels).toContain("Auto-execute");
		} finally {
			SMART_PROMPT.injectTarget = original;
		}
	});

	it("shows mapped placement labels, not raw enum values, in the Placement checkbox grid", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} initialTab="smart-prompts" />);
		const header = Array.from(container.querySelectorAll('[role="button"]')).find((el) =>
			el.textContent?.includes("Test Smart Prompt"),
		);
		fireEvent.click(header!);

		const placementLabels = Array.from(container.querySelectorAll(".placementCheck span")).map((el) => el.textContent);
		expect(placementLabels).toContain("Toolbar menu");
		expect(placementLabels).toContain("File right-click menu");
		expect(placementLabels).not.toContain("toolbar");
	});
});
