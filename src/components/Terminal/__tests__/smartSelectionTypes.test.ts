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
	};

	it("converts a full rule including nested actions", () => {
		expect(ruleFromWire(wire)).toEqual({
			id: "r1",
			name: "Git SHA",
			regex: "[0-9a-f]{7,40}",
			precision: "high",
			enabled: true,
			actions: [{ kind: "copy", title: "Copy SHA", parameter: "\\0", isDefault: false }],
		});
	});

	it("round-trips back to the wire shape", () => {
		expect(ruleToWire(ruleFromWire(wire))).toEqual(wire);
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
