/**
 * `SelectionTab.test.tsx` stubs `settingsStore` with a plain mocked object, which
 * proves the tab calls the right setter with the right argument but can't observe
 * a genuine re-render — its mock state isn't a real Solid store, so Solid never
 * reactively re-invokes `rules()`. That leaves the actual regression this session's
 * fix guards against untested: `rules()` must return `r` itself (not a fresh copy)
 * for a rule whose `actions` field is already normalized, or `<For>` sees a
 * brand-new object reference on every edit and remounts every `RuleRow` — silently
 * collapsing whichever one the user had open, mid-keystroke.
 *
 * This suite renders against the REAL `settingsStore` instead, so
 * `setSmartSelectionRules` triggers real reactivity and the collapse-on-edit bug
 * would actually reproduce here if the fix regressed.
 */
import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import "../../mocks/tauri";
import { SelectionTab } from "../../../components/SettingsPanel/tabs/SelectionTab";
import { DEFAULT_SMART_SELECTION_RULES } from "../../../components/Terminal/smartSelectionDefaults";
import { ruleContentHash } from "../../../components/Terminal/smartSelectionReconcile";
import { settingsStore } from "../../../stores/settings";

function expandRule(container: HTMLElement, index = 0) {
	const headers = Array.from(container.querySelectorAll('[data-testid="smart-rule-header"]')) as HTMLElement[];
	fireEvent.click(headers[index]);
}

describe("SelectionTab against the real settingsStore (reactivity regression)", () => {
	afterEach(() => {
		settingsStore.setSmartSelectionRules([]);
	});

	it("keeps a rule's editor expanded while its own fields are edited", async () => {
		settingsStore.setSmartSelectionRules([
			{ id: "r1", name: "Custom", regex: "x", precision: "normal", enabled: true, actions: [] },
		]);
		const { container, getByPlaceholderText } = render(() => <SelectionTab />);
		expandRule(container, 0);

		const patternInput = getByPlaceholderText("Regular expression") as HTMLInputElement;
		fireEvent.input(patternInput, { target: { value: "xy" } });
		await waitFor(() => expect(settingsStore.state.smartSelectionRules[0].regex).toBe("xy"));

		// Still expanded — the Pattern field only renders inside the expanded
		// body, so it having vanished would mean the row remounted and reset
		// to collapsed instead of staying open through the edit.
		const patternAfterEdit = getByPlaceholderText("Regular expression") as HTMLInputElement;
		expect(patternAfterEdit).toBeTruthy();
		expect(patternAfterEdit.value).toBe("xy");
	});

	it("editing one rule's field doesn't collapse another rule's already-expanded editor", async () => {
		settingsStore.setSmartSelectionRules([
			{ id: "r1", name: "First", regex: "a", precision: "normal", enabled: true, actions: [] },
			{ id: "r2", name: "Second", regex: "b", precision: "normal", enabled: true, actions: [] },
		]);
		const { container } = render(() => <SelectionTab />);
		expandRule(container, 0);
		expandRule(container, 1);

		// Both expanded — the Precision <select> only renders inside the expanded body.
		expect(container.querySelectorAll('[data-testid="smart-rule"] select').length).toBe(2);

		const nameInputs = Array.from(container.querySelectorAll('input[placeholder="Name"]')) as HTMLInputElement[];
		expect(nameInputs).toHaveLength(2);
		fireEvent.input(nameInputs[1], { target: { value: "Second, renamed" } });
		await waitFor(() => expect(settingsStore.state.smartSelectionRules[1].name).toBe("Second, renamed"));

		// Rule 1's editor is still open too — editing rule 2 must not have remounted it.
		expect(container.querySelectorAll('[data-testid="smart-rule"] select').length).toBe(2);
	});

	it("toggling a rule's enabled checkbox doesn't collapse its own already-expanded editor", async () => {
		settingsStore.setSmartSelectionRules([
			{ id: "r1", name: "Custom", regex: "x", precision: "normal", enabled: true, actions: [] },
		]);
		const { container, getByPlaceholderText } = render(() => <SelectionTab />);
		expandRule(container, 0);
		expect(getByPlaceholderText("Regular expression")).toBeTruthy();

		const checkbox = container.querySelector('[data-testid="smart-rule"] input[type="checkbox"]') as HTMLInputElement;
		fireEvent.click(checkbox);
		await waitFor(() => expect(settingsStore.state.smartSelectionRules[0].enabled).toBe(false));

		expect(getByPlaceholderText("Regular expression")).toBeTruthy();
	});

	it("keeps focus in the Menu label input across a keystroke", async () => {
		// Regression: `updateAction` spread-copies the edited action on every
		// keystroke (`{ ...a, ...patch }`), and a reference-keyed `<For>` would
		// dispose and recreate this exact <input> in response — moving focus
		// away mid-type. `<Index>` keeps the DOM node and just updates its value.
		settingsStore.setSmartSelectionRules([
			{
				id: "r1",
				name: "Custom",
				regex: "x",
				precision: "normal",
				enabled: true,
				actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: true }],
			},
		]);
		const { container } = render(() => <SelectionTab />);
		expandRule(container, 0);

		const menuLabelInput = container.querySelector('input[placeholder="Menu label"]') as HTMLInputElement;
		menuLabelInput.focus();
		expect(document.activeElement).toBe(menuLabelInput);

		fireEvent.input(menuLabelInput, { target: { value: "Copy text" } });
		await waitFor(() => expect(settingsStore.state.smartSelectionRules[0].actions[0].title).toBe("Copy text"));

		expect(document.activeElement).toBe(menuLabelInput);
	});

	it("keeps focus in the Menu label input when the first edit materializes built-in defaults", async () => {
		// The empty→materialized transition swaps every rule's object reference
		// at once (built-in module objects → store proxies), which a
		// reference-keyed `<For>` would see as every row changing identity
		// simultaneously — remounting the entire list, including whatever the
		// user was mid-keystroke into. `<Index>` compares old vs. new by
		// position, so a same-length swap is a value update, not a remount.
		settingsStore.setSmartSelectionRules([]);
		const { container } = render(() => <SelectionTab />);
		expandRule(container, 0);

		const menuLabelInput = container.querySelector('input[placeholder="Menu label"]') as HTMLInputElement;
		menuLabelInput.focus();
		expect(document.activeElement).toBe(menuLabelInput);

		fireEvent.input(menuLabelInput, { target: { value: "Renamed action" } });
		await waitFor(() => expect(settingsStore.state.smartSelectionRules.length).toBeGreaterThan(0));
		expect(settingsStore.state.smartSelectionRules[0].actions[0].title).toBe("Renamed action");

		expect(document.activeElement).toBe(menuLabelInput);
	});

	it("keeps focus in the Name input across a keystroke (same remount hazard as Menu label)", async () => {
		settingsStore.setSmartSelectionRules([
			{ id: "r1", name: "Custom", regex: "x", precision: "normal", enabled: true, actions: [] },
		]);
		const { container, getByPlaceholderText } = render(() => <SelectionTab />);
		expandRule(container, 0);

		const nameInput = getByPlaceholderText("Name") as HTMLInputElement;
		nameInput.focus();
		expect(document.activeElement).toBe(nameInput);

		fireEvent.input(nameInput, { target: { value: "Renamed rule" } });
		await waitFor(() => expect(settingsStore.state.smartSelectionRules[0].name).toBe("Renamed rule"));

		expect(document.activeElement).toBe(nameInput);
	});

	it("Add rule opens the new rule's editor, ready to fill in", async () => {
		settingsStore.setSmartSelectionRules([
			{ id: "r1", name: "Existing", regex: "a", precision: "normal", enabled: true, actions: [] },
		]);
		const { container, getByText, getByPlaceholderText } = render(() => <SelectionTab />);
		// Nothing is expanded yet: the editor's Pattern field only renders inside an open row.
		expect(container.querySelectorAll('input[placeholder="Regular expression"]').length).toBe(0);

		fireEvent.click(getByText("Add rule"));

		await waitFor(() => expect(settingsStore.state.smartSelectionRules).toHaveLength(2));
		const added = settingsStore.state.smartSelectionRules[1];
		expect(added.name).toBe("New rule");
		// Exactly one editor is open, and it is the new rule's (its pattern starts empty).
		const patterns = container.querySelectorAll('input[placeholder="Regular expression"]');
		expect(patterns.length).toBe(1);
		expect((getByPlaceholderText("Regular expression") as HTMLInputElement).value).toBe("");
		expect((container.querySelector('input[placeholder="Name"]') as HTMLInputElement).value).toBe("New rule");
	});

	it("Add rule leaves a rule you already had open open", async () => {
		settingsStore.setSmartSelectionRules([
			{ id: "r1", name: "Existing", regex: "a", precision: "normal", enabled: true, actions: [] },
		]);
		const { container, getByText } = render(() => <SelectionTab />);
		expandRule(container, 0);
		expect(container.querySelectorAll('input[placeholder="Regular expression"]').length).toBe(1);

		fireEvent.click(getByText("Add rule"));

		await waitFor(() => expect(settingsStore.state.smartSelectionRules).toHaveLength(2));
		expect(container.querySelectorAll('input[placeholder="Regular expression"]').length).toBe(2);
	});

	it("adding rules from the stored-empty (built-in defaults) state opens only the new one, not every default", async () => {
		settingsStore.setSmartSelectionRules([]);
		const { container, getByText } = render(() => <SelectionTab />);

		fireEvent.click(getByText("Add rule"));

		await waitFor(() => expect(settingsStore.state.smartSelectionRules.length).toBeGreaterThan(1));
		expect(container.querySelectorAll('input[placeholder="Regular expression"]').length).toBe(1);
	});

	it("saving the defaults by editing one rule keeps every rule's baseline: untouched ones match it, the edited one still points at the shipped version", async () => {
		settingsStore.setSmartSelectionRules([]);
		const { container } = render(() => <SelectionTab />);
		expandRule(container, 0);

		const nameInput = container.querySelector('input[placeholder="Name"]') as HTMLInputElement;
		fireEvent.input(nameInput, { target: { value: "My word rule" } });
		await waitFor(() =>
			expect(settingsStore.state.smartSelectionRules.length).toBe(DEFAULT_SMART_SELECTION_RULES.length),
		);

		const saved = settingsStore.state.smartSelectionRules;
		const [shippedFirst] = DEFAULT_SMART_SELECTION_RULES;
		// Edited: content moved on, baseline did not — that is exactly what marks it as edited later.
		expect(saved[0].name).toBe("My word rule");
		expect(saved[0].builtInHash).toBe(ruleContentHash(shippedFirst));
		expect(ruleContentHash(saved[0])).not.toBe(saved[0].builtInHash);
		// Untouched: the baseline equals the hash of the saved content.
		for (const rule of saved.slice(1)) {
			expect(rule.builtInHash, rule.id).toBeDefined();
			expect(ruleContentHash(rule), rule.id).toBe(rule.builtInHash);
		}
	});
});
