import { describe, expect, it } from "vitest";
import type { RustSmartSelectionAction, RustSmartSelectionRule } from "../smartSelectionTypes";
import { actionFromWire, actionToWire, canonicalAction, ruleFromWire, ruleToWire } from "../smartSelectionTypes";

describe("per-action options (send_return, run_in)", () => {
	const base: RustSmartSelectionAction = { kind: "send_text", title: "t", parameter: "p", is_default: false };

	it("round-trips a non-default value of each option, snake_case on the wire", () => {
		const wire: RustSmartSelectionAction = { ...base, send_return: "when_safe", run_in: "new_terminal" };
		const app = actionFromWire(wire);
		expect(app.sendReturn).toBe("when_safe");
		expect(app.runIn).toBe("new_terminal");
		expect(actionToWire(app)).toEqual(wire);
	});

	it("never stores a default: 'never' and 'background' are read as absent, so a rule stays byte-identical to before", () => {
		const app = actionFromWire({ ...base, send_return: "never", run_in: "background" });
		expect("sendReturn" in app).toBe(false);
		expect("runIn" in app).toBe(false);
	});

	it("drops an unrecognised value (a newer build's) instead of passing it through", () => {
		const app = actionFromWire({ ...base, send_return: "always", run_in: "somewhere" });
		expect("sendReturn" in app).toBe(false);
		expect("runIn" in app).toBe(false);
	});

	it("leaves the keys off the wire when unset, including an explicit undefined", () => {
		const wire = actionToWire({ kind: "copy", title: "t", parameter: "p", isDefault: false, sendReturn: undefined });
		expect("send_return" in wire).toBe(false);
		expect("run_in" in wire).toBe(false);
	});

	it("an action with options survives a whole rule round trip", () => {
		const rule = ruleFromWire({
			id: "r",
			name: "n",
			regex: "x",
			precision: "normal",
			enabled: true,
			actions: [{ ...base, run_in: "new_terminal" }],
		});
		expect(rule.actions[0].runIn).toBe("new_terminal");
		expect(ruleToWire(rule).actions[0].run_in).toBe("new_terminal");
	});
});

describe("run_background kind", () => {
	it("is a real kind, kept as written (an unknown kind would be coerced to copy)", () => {
		const wire: RustSmartSelectionAction = {
			kind: "run_background",
			title: "Open",
			parameter: "imark \\0",
			is_default: true,
		};
		expect(actionFromWire(wire).kind).toBe("run_background");
		expect(actionToWire(actionFromWire(wire))).toEqual(wire);
	});
});

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

	it("carries the built-in baseline hash both ways (snake_case on the wire)", () => {
		const stamped: RustSmartSelectionRule = { ...wire, built_in_hash: "0d5c4df7" };
		expect(ruleFromWire(stamped).builtInHash).toBe("0d5c4df7");
		expect(ruleToWire(ruleFromWire(stamped))).toEqual(stamped);
	});

	it("leaves the key off entirely when a rule has no baseline hash (so the config stays free of nulls)", () => {
		expect("builtInHash" in ruleFromWire(wire)).toBe(false);
		expect("built_in_hash" in ruleToWire(ruleFromWire(wire))).toBe(false);
		expect("built_in_hash" in ruleToWire({ ...ruleFromWire(wire), builtInHash: undefined })).toBe(false);
	});

	it("treats an empty-string hash like none", () => {
		expect("builtInHash" in ruleFromWire({ ...wire, built_in_hash: "" })).toBe(false);
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

describe("canonicalAction (the older run_command folded into Send Text)", () => {
	const legacy = { kind: "run_command" as const, title: "Show", parameter: "git show \\0", isDefault: true };

	it("reads run_command as send_text with Press Enter: when safe, keeping every other field", () => {
		expect(canonicalAction(legacy)).toEqual({
			kind: "send_text",
			title: "Show",
			parameter: "git show \\0",
			isDefault: true,
			sendReturn: "when_safe",
		});
	});

	it("does not touch the saved action", () => {
		const before = structuredClone(legacy);
		canonicalAction(legacy);
		expect(legacy).toEqual(before);
	});

	it("returns every other action as the very same object (so unchanged rows keep their identity)", () => {
		for (const kind of [
			"copy",
			"open_url",
			"open_file",
			"send_text",
			"run_background",
			"run_command_new_terminal",
			"ask_ai",
		] as const) {
			const action = { kind, title: "t", parameter: "p", isDefault: false };
			expect(canonicalAction(action)).toBe(action);
		}
	});

	it("an explicit Send Text with when safe is already canonical", () => {
		const action = {
			kind: "send_text" as const,
			title: "t",
			parameter: "p",
			isDefault: false,
			sendReturn: "when_safe" as const,
		};
		expect(canonicalAction(action)).toBe(action);
	});
});
