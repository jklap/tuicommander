import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

const {
	mockSetSmartSelectionEnabled,
	mockSetDoubleClickAction,
	mockSetWordSelectionMode,
	mockSetWordSeparators,
	mockSetWordSelectionRegex,
	mockSetSmartSelectionRules,
	DEFAULT_SEPARATORS,
} = vi.hoisted(() => ({
	mockSetSmartSelectionEnabled: vi.fn(),
	mockSetDoubleClickAction: vi.fn(),
	mockSetWordSelectionMode: vi.fn(),
	mockSetWordSeparators: vi.fn(),
	mockSetWordSelectionRegex: vi.fn(),
	mockSetSmartSelectionRules: vi.fn(),
	DEFAULT_SEPARATORS: " \"'`(){}[]<>|;:,.!?@#$%^&*~=+/\\",
}));

let mockState = {
	smartSelectionEnabled: true,
	doubleClickAction: "smart" as "smart" | "word",
	wordSelectionMode: "characters" as "characters" | "regex",
	wordSeparators: DEFAULT_SEPARATORS,
	wordSelectionRegex: "",
	smartSelectionRules: [] as unknown[],
};

vi.mock("../../../stores/settings", () => ({
	DEFAULT_WORD_SEPARATORS: DEFAULT_SEPARATORS,
	get settingsStore() {
		return {
			state: mockState,
			setSmartSelectionEnabled: mockSetSmartSelectionEnabled,
			setDoubleClickAction: mockSetDoubleClickAction,
			setWordSelectionMode: mockSetWordSelectionMode,
			setWordSeparators: mockSetWordSeparators,
			setWordSelectionRegex: mockSetWordSelectionRegex,
			setSmartSelectionRules: mockSetSmartSelectionRules,
		};
	},
}));

import { SelectionTab } from "../../../components/SettingsPanel/tabs/SelectionTab";

describe("SelectionTab", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockState = {
			smartSelectionEnabled: true,
			doubleClickAction: "smart",
			wordSelectionMode: "characters",
			wordSeparators: DEFAULT_SEPARATORS,
			wordSelectionRegex: "",
			smartSelectionRules: [],
		};
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	it("renders the Behavior, Word Boundaries, and Smart Selection Rules headings in order", () => {
		const { container } = render(() => <SelectionTab />);
		const headings = Array.from(container.querySelectorAll("h3")).map((h) => h.textContent);
		expect(headings).toEqual(["Behavior", "Word Boundaries", "Smart Selection Rules"]);
	});

	it("calls setSmartSelectionEnabled when its toggle changes", () => {
		const { container } = render(() => <SelectionTab />);
		const checkbox = container.querySelector('input[type="checkbox"]') as HTMLInputElement;
		fireEvent.change(checkbox, { target: { checked: false } });
		expect(mockSetSmartSelectionEnabled).toHaveBeenCalledWith(false);
	});

	it("calls setDoubleClickAction when the select changes", () => {
		const { container } = render(() => <SelectionTab />);
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		const doubleClickSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "word"))!;
		fireEvent.change(doubleClickSelect, { target: { value: "word" } });
		expect(mockSetDoubleClickAction).toHaveBeenCalledWith("word");
	});

	it("calls setWordSelectionMode when the select changes", () => {
		const { container } = render(() => <SelectionTab />);
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		const modeSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "regex"))!;
		fireEvent.change(modeSelect, { target: { value: "regex" } });
		expect(mockSetWordSelectionMode).toHaveBeenCalledWith("regex");
	});

	it("shows the word-separators input in characters mode and the word-pattern input in regex mode", () => {
		const { container, unmount } = render(() => <SelectionTab />);
		expect(container.querySelector('input[placeholder="https://|-|_"]')).toBeNull();
		unmount();

		mockState = { ...mockState, wordSelectionMode: "regex" };
		const { container: container2 } = render(() => <SelectionTab />);
		expect(container2.querySelector('input[placeholder="https://|-|_"]')).toBeTruthy();
	});

	it("restores the default separator string", () => {
		mockState = { ...mockState, wordSeparators: "-_" };
		const { getByText } = render(() => <SelectionTab />);
		fireEvent.click(getByText("Restore default separators"));
		expect(mockSetWordSeparators).toHaveBeenCalledWith(DEFAULT_SEPARATORS);
	});

	it("warns when the regex word pattern is invalid", () => {
		mockState = { ...mockState, wordSelectionMode: "regex", wordSelectionRegex: "(unterminated" };
		const { getByText } = render(() => <SelectionTab />);
		expect(getByText(/not valid regular expressions/)).toBeTruthy();
	});

	it("shows the built-in default rules when the stored rule list is empty", () => {
		const { getByDisplayValue } = render(() => <SelectionTab />);
		expect(getByDisplayValue("HTTP URL")).toBeTruthy();
		expect(getByDisplayValue("Git commit SHA")).toBeTruthy();
	});

	it("editing a default rule's name persists the full materialized list, not just that rule", () => {
		const { container } = render(() => <SelectionTab />);
		const nameInputs = Array.from(container.querySelectorAll('input[placeholder="Name"]')) as HTMLInputElement[];
		fireEvent.input(nameInputs[0], { target: { value: "Renamed" } });

		expect(mockSetSmartSelectionRules).toHaveBeenCalledTimes(1);
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { name: string }[];
		expect(persisted.length).toBeGreaterThan(1);
		expect(persisted[0].name).toBe("Renamed");
	});

	it("adds a new blank rule", () => {
		const { getByText } = render(() => <SelectionTab />);
		fireEvent.click(getByText("Add rule"));
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { name: string }[];
		expect(persisted[persisted.length - 1]?.name).toBe("New rule");
	});

	it("removes a rule", () => {
		const { container } = render(() => <SelectionTab />);
		const removeButtons = Array.from(
			container.querySelectorAll('[data-testid="smart-rule-remove"]'),
		) as HTMLButtonElement[];
		const countBefore = container.querySelectorAll('input[placeholder="Name"]').length;
		fireEvent.click(removeButtons[0]);
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as unknown[];
		expect(persisted.length).toBe(countBefore - 1);
	});

	it("labels every rule-card field so none of them are unlabeled mystery boxes", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [
				{
					id: "r1",
					name: "Custom",
					regex: "x",
					precision: "normal",
					enabled: true,
					actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
				},
			],
		};
		const { getByText, getAllByText } = render(() => <SelectionTab />);
		expect(getByText("Enabled")).toBeTruthy();
		expect(getByText("Pattern")).toBeTruthy();
		expect(getByText("Precision")).toBeTruthy();
		expect(getByText("Action")).toBeTruthy();
		expect(getByText("Menu label")).toBeTruthy();
		expect(getByText("Parameter")).toBeTruthy();
		// "Default" labels both the action-grid column header and the per-action radio.
		expect(getAllByText("Default").length).toBe(2);
	});

	it("does not put the rule card's controls inside a .group (which force-fills every descendant select/input to 100% width)", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [
				{ id: "r1", name: "Custom", regex: "x", precision: "normal", enabled: true, actions: [] },
			],
		};
		const { container } = render(() => <SelectionTab />);
		const ruleCard = container.querySelector('[data-testid="smart-rule"]') as HTMLElement;
		expect(ruleCard.className).not.toMatch(/\bgroup\b/);
	});

	it("toggles a rule's enabled checkbox", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [
				{ id: "r1", name: "Custom", regex: "x", precision: "normal", enabled: true, actions: [] },
			],
		};
		const { container } = render(() => <SelectionTab />);
		const ruleCard = container.querySelector('[data-testid="smart-rule"]') as HTMLElement;
		const checkbox = ruleCard.querySelector('input[type="checkbox"]') as HTMLInputElement;
		fireEvent.click(checkbox);
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { enabled: boolean }[];
		expect(persisted[0].enabled).toBe(false);
	});

	it("changes a rule's precision", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [
				{ id: "r1", name: "Custom", regex: "x", precision: "normal", enabled: true, actions: [] },
			],
		};
		const { getByDisplayValue } = render(() => <SelectionTab />);
		const precisionSelect = getByDisplayValue("Normal") as HTMLSelectElement;
		fireEvent.change(precisionSelect, { target: { value: "very_high" } });
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { precision: string }[];
		expect(persisted[0].precision).toBe("very_high");
	});

	it("warns on an individual rule's invalid regex without affecting the word-pattern warning", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [
				{ id: "r1", name: "Custom", regex: "(unterminated", precision: "normal", enabled: true, actions: [] },
			],
		};
		const { getByText } = render(() => <SelectionTab />);
		expect(getByText("Not a valid regular expression — this rule will be skipped.")).toBeTruthy();
	});

	it("restores built-in defaults by clearing the stored rule list", () => {
		mockState = { ...mockState, smartSelectionRules: [{ id: "x" } as never] };
		const { getByText } = render(() => <SelectionTab />);
		fireEvent.click(getByText("Restore built-in defaults"));
		expect(mockSetSmartSelectionRules).toHaveBeenCalledWith([]);
	});

	it("adding an action to a rule persisted without an `actions` field (hand-edited config.json) does not crash", () => {
		// `SmartSelectionRule.actions` is required by the TS type, but a
		// malformed/legacy config.json can still omit it at runtime.
		mockState = { ...mockState, smartSelectionRules: [{ id: "x", name: "Custom" } as never] };
		const { getByText } = render(() => <SelectionTab />);
		expect(() => fireEvent.click(getByText("Add action"))).not.toThrow();
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { actions: unknown[] }[];
		expect(persisted[0].actions.length).toBe(1);
	});

	it("adding an action to a rule with no actions appends a default 'Copy \\0' action", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [{ id: "r1", name: "Custom", regex: "x", precision: "normal", enabled: true, actions: [] }],
		};
		const { getByText } = render(() => <SelectionTab />);
		fireEvent.click(getByText("Add action"));
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as {
			actions: { kind: string; parameter: string }[];
		}[];
		expect(persisted[0].actions).toEqual([{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }]);
	});

	it("marking an action as default clears the default flag on any other action in the same rule", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [
				{
					id: "r1",
					name: "Custom",
					regex: "x",
					precision: "normal",
					enabled: true,
					actions: [
						{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: true },
						{ kind: "open_url", title: "Open", parameter: "\\0", isDefault: false },
					],
				},
			],
		};
		const { container } = render(() => <SelectionTab />);
		const radios = Array.from(container.querySelectorAll('input[type="radio"]')) as HTMLInputElement[];
		fireEvent.click(radios[1]);

		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { actions: { isDefault: boolean }[] }[];
		expect(persisted[0].actions.map((a) => a.isDefault)).toEqual([false, true]);
	});

	it("removes an action from a rule", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [
				{
					id: "r1",
					name: "Custom",
					regex: "x",
					precision: "normal",
					enabled: true,
					actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
				},
			],
		};
		const { container } = render(() => <SelectionTab />);
		const removeButtons = Array.from(
			container.querySelectorAll('[data-testid="smart-action-remove"]'),
		) as HTMLButtonElement[];
		fireEvent.click(removeButtons[0]);
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { actions: unknown[] }[];
		expect(persisted[0].actions).toEqual([]);
	});

	it("changes an action's kind", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [
				{
					id: "r1",
					name: "Custom",
					regex: "x",
					precision: "normal",
					enabled: true,
					actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
				},
			],
		};
		const { container } = render(() => <SelectionTab />);
		const kindSelect = container.querySelector('[data-testid="smart-action-kind"]') as HTMLSelectElement;
		fireEvent.change(kindSelect, { target: { value: "open_url" } });
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { actions: { kind: string }[] }[];
		expect(persisted[0].actions[0].kind).toBe("open_url");
	});

	it("edits an action's menu label", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [
				{
					id: "r1",
					name: "Custom",
					regex: "x",
					precision: "normal",
					enabled: true,
					actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
				},
			],
		};
		const { container } = render(() => <SelectionTab />);
		const titleInput = container.querySelector('input[placeholder="Menu label"]') as HTMLInputElement;
		fireEvent.input(titleInput, { target: { value: "Copy SHA" } });
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { actions: { title: string }[] }[];
		expect(persisted[0].actions[0].title).toBe("Copy SHA");
	});

	it("edits an action's parameter", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [
				{
					id: "r1",
					name: "Custom",
					regex: "x",
					precision: "normal",
					enabled: true,
					actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
				},
			],
		};
		const { getByDisplayValue } = render(() => <SelectionTab />);
		fireEvent.input(getByDisplayValue("\\0"), { target: { value: "\\1" } });
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { actions: { parameter: string }[] }[];
		expect(persisted[0].actions[0].parameter).toBe("\\1");
	});
});
