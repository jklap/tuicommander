import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

const {
	mockSetDoubleClickAction,
	mockSetWordSelectionMode,
	mockSetWordSeparators,
	mockSetWordSelectionRegex,
	mockSetSmartSelectionRules,
	DEFAULT_SEPARATORS,
} = vi.hoisted(() => ({
	mockSetDoubleClickAction: vi.fn(),
	mockSetWordSelectionMode: vi.fn(),
	mockSetWordSeparators: vi.fn(),
	mockSetWordSelectionRegex: vi.fn(),
	mockSetSmartSelectionRules: vi.fn(),
	DEFAULT_SEPARATORS: " \"'`(){}[]<>|;:,.!?@#$%^&*~=+/\\",
}));

let mockState = {
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
			setDoubleClickAction: mockSetDoubleClickAction,
			setWordSelectionMode: mockSetWordSelectionMode,
			setWordSeparators: mockSetWordSeparators,
			setWordSelectionRegex: mockSetWordSelectionRegex,
			setSmartSelectionRules: mockSetSmartSelectionRules,
		};
	},
}));

import { SelectionTab } from "../../../components/SettingsPanel/tabs/SelectionTab";

/** Rule rows render collapsed; expand the Nth one (by DOM order) before poking at its fields. */
function expandRule(container: HTMLElement, index = 0) {
	const headers = Array.from(container.querySelectorAll('[data-testid="smart-rule-header"]')) as HTMLElement[];
	fireEvent.click(headers[index]);
}

describe("SelectionTab", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockState = {
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

	it("edits the word-separators string directly", () => {
		const { getByDisplayValue } = render(() => <SelectionTab />);
		// DEFAULT_SEPARATORS starts with a literal space — RTL's default text
		// normalizer trims display values before matching, so a plain string
		// match would miss it; disable normalization for this exact-value query.
		const input = getByDisplayValue(DEFAULT_SEPARATORS, { normalizer: (value) => value });
		fireEvent.input(input, { target: { value: "-_" } });
		expect(mockSetWordSeparators).toHaveBeenCalledWith("-_");
	});

	it("warns when the regex word pattern is invalid", () => {
		mockState = { ...mockState, wordSelectionMode: "regex", wordSelectionRegex: "(unterminated" };
		const { getByText } = render(() => <SelectionTab />);
		expect(getByText(/not valid regular expressions/)).toBeTruthy();
	});

	it("edits the word pattern regex directly", () => {
		mockState = { ...mockState, wordSelectionMode: "regex" };
		const { getByPlaceholderText } = render(() => <SelectionTab />);
		fireEvent.input(getByPlaceholderText("https://|-|_"), { target: { value: "https://" } });
		expect(mockSetWordSelectionRegex).toHaveBeenCalledWith("https://");
	});

	it("shows the built-in default rules, collapsed, when the stored rule list is empty", () => {
		const { getByText } = render(() => <SelectionTab />);
		expect(getByText("HTTP URL")).toBeTruthy();
		expect(getByText("Git commit SHA")).toBeTruthy();
	});

	it("editing a default rule's name persists the full materialized list, not just that rule", () => {
		const { container } = render(() => <SelectionTab />);
		expandRule(container, 0);
		const nameInputs = Array.from(container.querySelectorAll('input[placeholder="Name"]')) as HTMLInputElement[];
		fireEvent.input(nameInputs[0], { target: { value: "Renamed" } });

		expect(mockSetSmartSelectionRules).toHaveBeenCalledTimes(1);
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { name: string }[];
		expect(persisted.length).toBeGreaterThan(1);
		expect(persisted[0].name).toBe("Renamed");
	});

	it("collapses an expanded rule on a second click of its header", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [{ id: "r1", name: "Custom", regex: "x", precision: "normal", enabled: true, actions: [] }],
		};
		const { container, queryByPlaceholderText } = render(() => <SelectionTab />);
		const header = container.querySelector('[data-testid="smart-rule-header"]') as HTMLElement;

		fireEvent.click(header);
		expect(queryByPlaceholderText("Regular expression")).toBeTruthy();
		fireEvent.click(header);
		expect(queryByPlaceholderText("Regular expression")).toBeNull();
	});

	it("pressing Space on the Enabled checkbox doesn't also toggle the row's expand state", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [{ id: "r1", name: "Custom", regex: "x", precision: "normal", enabled: true, actions: [] }],
		};
		const { container, queryByPlaceholderText } = render(() => <SelectionTab />);
		const checkbox = container.querySelector('[data-testid="smart-rule"] input[type="checkbox"]') as HTMLInputElement;

		expect(queryByPlaceholderText("Regular expression")).toBeNull();
		fireEvent.keyDown(checkbox, { key: " " });
		// A keydown on the checkbox must not bubble into the header's own
		// keydown handler and expand the row as a side effect of checking it.
		expect(queryByPlaceholderText("Regular expression")).toBeNull();
	});

	it("expands a rule via the Enter key on its header, not just a click", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [{ id: "r1", name: "Custom", regex: "x", precision: "normal", enabled: true, actions: [] }],
		};
		const { container, queryByPlaceholderText } = render(() => <SelectionTab />);
		expect(queryByPlaceholderText("Regular expression")).toBeNull();

		const header = container.querySelector('[data-testid="smart-rule-header"]') as HTMLElement;
		fireEvent.keyDown(header, { key: "Enter" });
		expect(queryByPlaceholderText("Regular expression")).toBeTruthy();
	});

	it("editing a rule's pattern persists the full materialized list", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [{ id: "r1", name: "Custom", regex: "x", precision: "normal", enabled: true, actions: [] }],
		};
		const { container, getByPlaceholderText } = render(() => <SelectionTab />);
		expandRule(container, 0);
		fireEvent.input(getByPlaceholderText("Regular expression"), { target: { value: "xy" } });
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { regex: string }[];
		expect(persisted[0].regex).toBe("xy");
	});

	it("shows a fallback label for a rule with no name and hides the pattern preview for a rule with no regex", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [{ id: "r1", name: "", regex: "", precision: "normal", enabled: true, actions: [] }],
		};
		const { container, getByText } = render(() => <SelectionTab />);
		expect(getByText("Unnamed rule")).toBeTruthy();
		expect(container.querySelector("code")).toBeNull();
	});

	it("adds a new blank rule", () => {
		const { getByText } = render(() => <SelectionTab />);
		fireEvent.click(getByText("Add rule"));
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { name: string }[];
		expect(persisted.at(-1)?.name).toBe("New rule");
	});

	it("removes a rule", () => {
		const { container } = render(() => <SelectionTab />);
		const countBefore = container.querySelectorAll('[data-testid="smart-rule"]').length;
		expandRule(container, 0);
		const removeButton = container.querySelector('[data-testid="smart-rule-remove"]') as HTMLButtonElement;
		fireEvent.click(removeButton);
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
		const { container, getByText, getAllByText } = render(() => <SelectionTab />);
		expandRule(container, 0);
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
			smartSelectionRules: [{ id: "r1", name: "Custom", regex: "x", precision: "normal", enabled: true, actions: [] }],
		};
		const { container } = render(() => <SelectionTab />);
		const ruleCard = container.querySelector('[data-testid="smart-rule"]') as HTMLElement;
		expect(ruleCard.className).not.toMatch(/\bgroup\b/);
	});

	it("toggles a rule's enabled checkbox", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [{ id: "r1", name: "Custom", regex: "x", precision: "normal", enabled: true, actions: [] }],
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
			smartSelectionRules: [{ id: "r1", name: "Custom", regex: "x", precision: "normal", enabled: true, actions: [] }],
		};
		const { container, getByDisplayValue } = render(() => <SelectionTab />);
		expandRule(container, 0);
		const precisionSelect = getByDisplayValue("Normal") as HTMLSelectElement;
		fireEvent.change(precisionSelect, { target: { value: "very_high" } });
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { precision: string }[];
		expect(persisted[0].precision).toBe("very_high");
	});

	it("badges an individual rule's invalid regex on its collapsed row", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [
				{ id: "r1", name: "Custom", regex: "(unterminated", precision: "normal", enabled: true, actions: [] },
			],
		};
		const { getByText } = render(() => <SelectionTab />);
		expect(getByText("Invalid pattern")).toBeTruthy();
	});

	it("warns on an individual rule's invalid regex without affecting the word-pattern warning", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [
				{ id: "r1", name: "Custom", regex: "(unterminated", precision: "normal", enabled: true, actions: [] },
			],
		};
		const { container, getByText } = render(() => <SelectionTab />);
		expandRule(container, 0);
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
		const { container, getByText } = render(() => <SelectionTab />);
		expandRule(container, 0);
		expect(() => fireEvent.click(getByText("Add action"))).not.toThrow();
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { actions: unknown[] }[];
		expect(persisted[0].actions.length).toBe(1);
	});

	it("adding an action to a rule with no actions appends a default 'Copy \\0' action", () => {
		mockState = {
			...mockState,
			smartSelectionRules: [{ id: "r1", name: "Custom", regex: "x", precision: "normal", enabled: true, actions: [] }],
		};
		const { container, getByText } = render(() => <SelectionTab />);
		expandRule(container, 0);
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
		expandRule(container, 0);
		const radios = Array.from(container.querySelectorAll('input[type="radio"]')) as HTMLInputElement[];
		fireEvent.click(radios[1]);

		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { actions: { isDefault: boolean }[] }[];
		expect(persisted[0].actions.map((a) => a.isDefault)).toEqual([false, true]);
	});

	it("editing one action's field leaves a sibling action untouched", () => {
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
						{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false },
						{ kind: "open_url", title: "Open", parameter: "\\0", isDefault: true },
					],
				},
			],
		};
		const { container } = render(() => <SelectionTab />);
		expandRule(container, 0);
		const titleInputs = Array.from(container.querySelectorAll('input[placeholder="Menu label"]')) as HTMLInputElement[];
		fireEvent.input(titleInputs[0], { target: { value: "Copy SHA" } });

		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as {
			actions: { title: string; isDefault: boolean }[];
		}[];
		expect(persisted[0].actions[0]).toEqual({ kind: "copy", title: "Copy SHA", parameter: "\\0", isDefault: false });
		// The sibling wasn't the one edited and its patch carries no `isDefault` —
		// it must pass through completely unchanged, default flag included.
		expect(persisted[0].actions[1]).toEqual({ kind: "open_url", title: "Open", parameter: "\\0", isDefault: true });
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
		expandRule(container, 0);
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
		expandRule(container, 0);
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
		expandRule(container, 0);
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
		const { container, getByDisplayValue } = render(() => <SelectionTab />);
		expandRule(container, 0);
		fireEvent.input(getByDisplayValue("\\0"), { target: { value: "\\1" } });
		const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { actions: { parameter: string }[] }[];
		expect(persisted[0].actions[0].parameter).toBe("\\1");
	});

	describe("action kinds: Send Text vs Run Command", () => {
		const ruleWith = (kind: string, extra: Record<string, unknown> = {}) => ({
			id: "r1",
			name: "Custom",
			regex: "x",
			precision: "normal",
			enabled: true,
			actions: [{ kind, title: "Do it", parameter: "\\0", isDefault: false, ...extra }],
		});
		const kindSelect = (container: HTMLElement) =>
			container.querySelector('[data-testid="smart-action-kind"]') as HTMLSelectElement;
		const optionLabels = (select: HTMLSelectElement) =>
			Object.fromEntries(Array.from(select.options).map((o) => [o.value, o.textContent]));

		it("offers Send Text and Run Command, and no separate 'Send Command' (folded into Send Text)", () => {
			mockState = { ...mockState, smartSelectionRules: [ruleWith("copy")] };
			const { container } = render(() => <SelectionTab />);
			expandRule(container, 0);

			const labels = optionLabels(kindSelect(container));
			expect(labels).toMatchObject({ send_text: "Send Text", run_background: "Run Command" });
			expect(Object.keys(labels)).not.toContain("run_command");
			expect(Object.values(labels)).not.toContain("Send Command");
		});

		it("no longer offers 'Run Command in New Terminal' as a separate kind — it is Run Command's 'In a new terminal'", () => {
			mockState = { ...mockState, smartSelectionRules: [ruleWith("copy")] };
			const { container } = render(() => <SelectionTab />);
			expandRule(container, 0);
			expect(Object.keys(optionLabels(kindSelect(container)))).not.toContain("run_command_new_terminal");
		});

		it("still shows a rule saved with the older kind, under its own name, so the select is never blank", () => {
			mockState = { ...mockState, smartSelectionRules: [ruleWith("run_command_new_terminal")] };
			const { container } = render(() => <SelectionTab />);
			expandRule(container, 0);

			const select = kindSelect(container);
			expect(select.value).toBe("run_command_new_terminal");
			expect(select.selectedOptions[0].textContent).toBe("Run Command in New Terminal (older form)");
		});

		it("shows a rule saved with the older run_command as Send Text with Press Enter: When safe (same behaviour)", () => {
			mockState = { ...mockState, smartSelectionRules: [ruleWith("run_command")] };
			const { container } = render(() => <SelectionTab />);
			expandRule(container, 0);

			const select = kindSelect(container);
			expect(select.value).toBe("send_text");
			expect(select.selectedOptions[0].textContent).toBe("Send Text");
			const pressEnter = container.querySelector('[data-testid="smart-action-send-return"]') as HTMLSelectElement;
			expect(pressEnter.value).toBe("when_safe");
		});

		it("the first edit saves that rule in the new spelling, leaving the behaviour as it was", () => {
			mockState = { ...mockState, smartSelectionRules: [ruleWith("run_command")] };
			const { container, getByDisplayValue } = render(() => <SelectionTab />);
			expandRule(container, 0);

			fireEvent.input(getByDisplayValue("Do it"), { target: { value: "Renamed" } });

			const saved = (mockSetSmartSelectionRules.mock.calls.at(-1)![0] as { actions: Record<string, unknown>[] }[])[0]
				.actions[0];
			expect(saved).toMatchObject({ kind: "send_text", sendReturn: "when_safe", title: "Renamed" });
		});

		it("does not rewrite anything just by being opened (the older spelling stays on disk until an edit)", () => {
			mockState = { ...mockState, smartSelectionRules: [ruleWith("run_command")] };
			const { container } = render(() => <SelectionTab />);
			expandRule(container, 0);
			expect(mockSetSmartSelectionRules).not.toHaveBeenCalled();
		});

		it("choosing Run Command stores the run_background kind", () => {
			mockState = { ...mockState, smartSelectionRules: [ruleWith("copy")] };
			const { container } = render(() => <SelectionTab />);
			expandRule(container, 0);

			fireEvent.change(kindSelect(container), { target: { value: "run_background" } });

			const persisted = mockSetSmartSelectionRules.mock.calls[0][0] as { actions: { kind: string }[] }[];
			expect(persisted[0].actions[0].kind).toBe("run_background");
		});

		it("explains the quoting and the environment, but only on a rule that has a Run Command action", () => {
			mockState = { ...mockState, smartSelectionRules: [ruleWith("run_background")] };
			const { container, unmount } = render(() => <SelectionTab />);
			expandRule(container, 0);
			const hint = container.querySelector('[data-testid="smart-run-command-hint"]');
			expect(hint?.textContent).toContain("as data");
			expect(hint?.textContent).toContain("login environment");
			expect(hint?.textContent).toContain("SSH");
			expect(hint?.textContent).toContain("/bin/sh");
			// Quoting the placeholder is now safe, so the old "don't quote it" advice must be gone.
			expect(hint?.textContent).not.toContain("not imark");
			// The two things that DO make a shell read a value as code are called out.
			expect(hint?.textContent).toContain("eval");
			expect(hint?.textContent).toContain("bash, zsh, ksh, dash or sh");
			expect(hint?.textContent).toContain("1000 bytes");
			// Names the other kind, so the two are not confused.
			expect(hint?.textContent).toContain("Send Text");
			unmount();

			mockState = { ...mockState, smartSelectionRules: [ruleWith("send_text")] };
			const { container: other } = render(() => <SelectionTab />);
			expandRule(other, 0);
			expect(other.querySelector('[data-testid="smart-run-command-hint"]')).toBeNull();
		});
	});

	describe("hints that warn about unsafe choices", () => {
		const open = (kind: string) => {
			mockState = {
				...mockState,
				smartSelectionRules: [
					{
						id: "r1",
						name: "Custom",
						regex: "x",
						precision: "normal",
						enabled: true,
						actions: [{ kind, title: "Do it", parameter: "\\0", isDefault: false }],
					},
				],
			};
			const { container } = render(() => <SelectionTab />);
			expandRule(container, 0);
			return container;
		};

		it("Run Command's hint warns that a match starting with '-' can be read as an option, and suggests --", () => {
			const hint = open("run_background").querySelector('[data-testid="smart-run-command-hint"]');
			expect(hint?.textContent).toContain("starts with");
			expect(hint?.textContent).toContain("option");
			expect(hint?.textContent).toContain("--");
		});

		it("the older new-terminal kind warns that it does not quote matched text, and only that kind does", () => {
			const legacy = open("run_command_new_terminal").querySelector('[data-testid="smart-legacy-new-terminal-hint"]');
			expect(legacy?.textContent).toContain("straight into the command line");
			expect(legacy?.textContent).toContain("In a new terminal");
			for (const kind of ["copy", "send_text", "run_background"]) {
				expect(open(kind).querySelector('[data-testid="smart-legacy-new-terminal-hint"]'), kind).toBeNull();
			}
		});

		it("Press Enter explains what 'When safe' means in plain terms (no 'shell operators' hand-waving)", () => {
			const text = open("send_text").textContent ?? "";
			expect(text).toContain("When safe");
			expect(text).toContain("letters, digits, spaces");
			expect(text).not.toContain("no shell operators");
		});
	});

	describe("per-action options", () => {
		const rule = (kind: string, extra: Record<string, unknown> = {}) => ({
			id: "r1",
			name: "Custom",
			regex: "x",
			precision: "normal",
			enabled: true,
			actions: [{ kind, title: "Do it", parameter: "\\0", isDefault: false, ...extra }],
		});
		const open = (kind: string, extra: Record<string, unknown> = {}) => {
			mockState = { ...mockState, smartSelectionRules: [rule(kind, extra)] };
			const { container } = render(() => <SelectionTab />);
			expandRule(container, 0);
			return container;
		};
		const sendReturn = (c: HTMLElement) =>
			c.querySelector('[data-testid="smart-action-send-return"]') as HTMLSelectElement | null;
		const runIn = (c: HTMLElement) =>
			c.querySelector('[data-testid="smart-action-run-in"]') as HTMLSelectElement | null;
		const persistedAction = () =>
			(mockSetSmartSelectionRules.mock.calls.at(-1)![0] as { actions: Record<string, unknown>[] }[])[0].actions[0];

		it("Send Text offers 'Press Enter: Never / When safe', defaulting to Never", () => {
			const c = open("send_text");
			const select = sendReturn(c)!;
			expect(Array.from(select.options).map((o) => [o.value, o.textContent])).toEqual([
				["never", "Never"],
				["when_safe", "When safe"],
			]);
			expect(select.value).toBe("never");
			expect(runIn(c)).toBeNull();
		});

		it("Run Command offers 'Run: In the background / In a new terminal', defaulting to the background", () => {
			const c = open("run_background");
			const select = runIn(c)!;
			expect(Array.from(select.options).map((o) => [o.value, o.textContent])).toEqual([
				["background", "In the background"],
				["new_terminal", "In a new terminal"],
			]);
			expect(select.value).toBe("background");
			expect(sendReturn(c)).toBeNull();
		});

		it("no other kind shows an option row", () => {
			for (const kind of ["copy", "open_url", "open_file", "ask_ai"]) {
				const c = open(kind);
				expect(sendReturn(c), kind).toBeNull();
				expect(runIn(c), kind).toBeNull();
				c.remove();
			}
		});

		it("shows the saved value", () => {
			expect(sendReturn(open("send_text", { sendReturn: "when_safe" }))!.value).toBe("when_safe");
			expect(runIn(open("run_background", { runIn: "new_terminal" }))!.value).toBe("new_terminal");
		});

		it("choosing a non-default value stores it; choosing the default again stores nothing", () => {
			const c = open("send_text");
			fireEvent.change(sendReturn(c)!, { target: { value: "when_safe" } });
			expect(persistedAction().sendReturn).toBe("when_safe");

			const c2 = open("send_text", { sendReturn: "when_safe" });
			fireEvent.change(sendReturn(c2)!, { target: { value: "never" } });
			expect(persistedAction().sendReturn).toBeUndefined();

			const c3 = open("run_background");
			fireEvent.change(runIn(c3)!, { target: { value: "new_terminal" } });
			expect(persistedAction().runIn).toBe("new_terminal");

			const c4 = open("run_background", { runIn: "new_terminal" });
			fireEvent.change(runIn(c4)!, { target: { value: "background" } });
			expect(persistedAction().runIn).toBeUndefined();
		});

		it("changing the kind drops the old kind's option instead of leaving a hidden one behind", () => {
			const c = open("send_text", { sendReturn: "when_safe" });
			fireEvent.change(c.querySelector('[data-testid="smart-action-kind"]') as HTMLSelectElement, {
				target: { value: "copy" },
			});
			const action = persistedAction();
			expect(action.kind).toBe("copy");
			expect(action.sendReturn).toBeUndefined();
			expect(action.runIn).toBeUndefined();
		});

		it("re-selecting the same kind keeps its option", () => {
			const c = open("run_background", { runIn: "new_terminal" });
			fireEvent.change(c.querySelector('[data-testid="smart-action-kind"]') as HTMLSelectElement, {
				target: { value: "run_background" },
			});
			// An unchanged kind is not a kind change: nothing is cleared. (`persistedAction` throws if
			// nothing was saved at all, so this cannot pass by the handler simply not running.)
			expect(mockSetSmartSelectionRules).toHaveBeenCalled();
			expect(persistedAction().runIn).toBe("new_terminal");
		});
	});
});
