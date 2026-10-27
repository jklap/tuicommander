import { describe, expect, it } from "vitest";
import type { RustSmartSelectionAction, RustSmartSelectionRule } from "../smartSelectionTypes";
import { actionFromWire, actionToWire, ruleFromWire, ruleToWire } from "../smartSelectionTypes";

describe("actionFromWire / actionToWire", () => {
	const wire: RustSmartSelectionAction = {
		kind: "run_command",
		title: "Show commit",
		parameter: "git show \\0",
		is_default: true,
	};

	it("maps snake_case is_default to camelCase isDefault", () => {
		expect(actionFromWire(wire)).toEqual({
			kind: "run_command",
			title: "Show commit",
			parameter: "git show \\0",
			isDefault: true,
		});
	});

	it("round-trips back to the wire shape", () => {
		expect(actionToWire(actionFromWire(wire))).toEqual(wire);
	});

	it("falls back to 'copy' for an unrecognized kind (forward-compat with a newer app version's rule)", () => {
		const unknown: RustSmartSelectionAction = { ...wire, kind: "run_coprocess" };
		expect(actionFromWire(unknown).kind).toBe("copy");
	});
});

describe("ruleFromWire / ruleToWire", () => {
	const wire: RustSmartSelectionRule = {
		id: "r1",
		name: "Git SHA",
		regex: "[0-9a-f]{7,40}",
		precision: "high",
		enabled: true,
		actions: [{ kind: "copy", title: "Copy SHA", parameter: "\\0", is_default: false }],
		highlight_on_modifier: false,
	};

	it("converts a full rule including nested actions", () => {
		expect(ruleFromWire(wire)).toEqual({
			id: "r1",
			name: "Git SHA",
			regex: "[0-9a-f]{7,40}",
			precision: "high",
			enabled: true,
			actions: [{ kind: "copy", title: "Copy SHA", parameter: "\\0", isDefault: false }],
			highlightOnModifier: false,
		});
	});

	it("round-trips back to the wire shape", () => {
		expect(ruleToWire(ruleFromWire(wire))).toEqual(wire);
	});

	// Catches: highlight_on_modifier dropped on load or save, so the checkbox never sticks.
	it("carries highlight_on_modifier both ways", () => {
		const on: RustSmartSelectionRule = { ...wire, highlight_on_modifier: true };
		expect(ruleFromWire(on).highlightOnModifier).toBe(true);
		expect(ruleToWire(ruleFromWire(on)).highlight_on_modifier).toBe(true);
	});

	// Catches: a config.json written before the field existed turning highlighting on or failing to load.
	it("treats a missing or non-boolean highlight_on_modifier as off", () => {
		const { highlight_on_modifier: _omit, ...legacy } = wire;
		expect(ruleFromWire(legacy as RustSmartSelectionRule).highlightOnModifier).toBe(false);
		expect(ruleFromWire({ ...wire, highlight_on_modifier: "true" as unknown as boolean }).highlightOnModifier).toBe(
			false,
		);
		expect(
			ruleToWire({ id: "x", name: "x", regex: "x", precision: "normal", enabled: true, actions: [] })
				.highlight_on_modifier,
		).toBe(false);
	});

	it("falls back to 'normal' precision for an unrecognized value", () => {
		const unknown: RustSmartSelectionRule = { ...wire, precision: "extreme" };
		expect(ruleFromWire(unknown).precision).toBe("normal");
	});

	it("handles a rule with no actions", () => {
		const noActions: RustSmartSelectionRule = { ...wire, actions: [] };
		expect(ruleFromWire(noActions).actions).toEqual([]);
	});

	it("does not throw when actions is missing entirely (hand-edited/corrupted config.json)", () => {
		const { actions: _actions, ...ruleWithoutActionsField } = wire;
		const malformed = ruleWithoutActionsField as RustSmartSelectionRule;
		expect(() => ruleFromWire(malformed)).not.toThrow();
		expect(ruleFromWire(malformed).actions).toEqual([]);
	});
});
