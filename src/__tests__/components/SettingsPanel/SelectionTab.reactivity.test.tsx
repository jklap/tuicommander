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
});
