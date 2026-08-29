import { describe, expect, it } from "vitest";
import type { SmartSelectionAction, SmartSelectionRule } from "../../components/Terminal/smartSelectionTypes";
import {
	buildRulesExportFile,
	classifyRuleImport,
	differsFromDefaultRule,
	isRuleModified,
	mergeImportedRules,
	parseRulesExportFile,
	RULES_EXPORT_KIND,
	RULES_EXPORT_SCHEMA_VERSION,
	ruleRunsCode,
	selectRulesForExport,
} from "../../utils/smartSelectionExport";

function makeAction(overrides: Partial<SmartSelectionAction> = {}): SmartSelectionAction {
	return { kind: "copy", title: "Copy", parameter: "\\0", isDefault: false, ...overrides };
}

function makeRule(overrides: Partial<SmartSelectionRule> = {}): SmartSelectionRule {
	return {
		id: "iterm-word",
		name: "Word",
		regex: "[\\w-]+",
		precision: "normal",
		enabled: true,
		actions: [makeAction()],
		...overrides,
	};
}

describe("differsFromDefaultRule", () => {
	it("returns false for an untouched default", () => {
		expect(differsFromDefaultRule(makeRule(), makeRule())).toBe(false);
	});

	it("detects a regex edit", () => {
		expect(differsFromDefaultRule(makeRule({ regex: "different" }), makeRule())).toBe(true);
	});

	it("detects an enabled-only change", () => {
		expect(differsFromDefaultRule(makeRule({ enabled: false }), makeRule())).toBe(true);
	});

	it("detects a precision change", () => {
		expect(differsFromDefaultRule(makeRule({ precision: "high" }), makeRule())).toBe(true);
	});

	it("detects an action added", () => {
		const def = makeRule();
		const current = makeRule({ actions: [makeAction(), makeAction({ kind: "open_url" })] });
		expect(differsFromDefaultRule(current, def)).toBe(true);
	});

	it("detects reordered actions — order is the menu order, not a set", () => {
		const def = makeRule({ actions: [makeAction({ title: "A" }), makeAction({ title: "B" })] });
		const current = makeRule({ actions: [makeAction({ title: "B" }), makeAction({ title: "A" })] });
		expect(differsFromDefaultRule(current, def)).toBe(true);
	});

	it("detects moving which action is the default", () => {
		const def = makeRule({
			actions: [makeAction({ title: "A", isDefault: true }), makeAction({ title: "B", isDefault: false })],
		});
		const current = makeRule({
			actions: [makeAction({ title: "A", isDefault: false }), makeAction({ title: "B", isDefault: true })],
		});
		expect(differsFromDefaultRule(current, def)).toBe(true);
	});

	it("treats isDefault: undefined as equal to isDefault: false", () => {
		const def = makeRule({ actions: [makeAction({ isDefault: false })] });
		const current = makeRule({ actions: [{ ...makeAction(), isDefault: undefined as unknown as boolean }] });
		expect(differsFromDefaultRule(current, def)).toBe(false);
	});
});

describe("isRuleModified / selectRulesForExport", () => {
	const untouched = makeRule();
	const editedDefault = makeRule({ id: "iterm-path", regex: "edited" });
	const custom = makeRule({ id: "my-custom", name: "My Custom" });
	const all = [untouched, editedDefault, custom];
	const defaultsById = new Map<string, SmartSelectionRule>([
		["iterm-word", makeRule()],
		["iterm-path", makeRule({ id: "iterm-path" })],
	]);

	it("is false for an unmodified default", () => {
		expect(isRuleModified(untouched, defaultsById)).toBe(false);
	});

	it("is true for an edited default", () => {
		expect(isRuleModified(editedDefault, defaultsById)).toBe(true);
	});

	it("is true for any custom rule, even if identical in shape to a default", () => {
		expect(isRuleModified(makeRule({ id: "my-custom" }), defaultsById)).toBe(true);
	});

	it("'all' returns everything", () => {
		expect(selectRulesForExport(all, "all", defaultsById)).toEqual(all);
	});

	it("'custom' returns only non-default ids", () => {
		expect(selectRulesForExport(all, "custom", defaultsById)).toEqual([custom]);
	});

	it("'modified' returns changed defaults plus all custom rules", () => {
		expect(selectRulesForExport(all, "modified", defaultsById)).toEqual([editedDefault, custom]);
	});
});

describe("buildRulesExportFile", () => {
	it("wraps rules in the export envelope with no field stripping", () => {
		const rule = makeRule();
		const file = buildRulesExportFile([rule], "all", "1.2.3");

		expect(file.kind).toBe(RULES_EXPORT_KIND);
		expect(file.schemaVersion).toBe(RULES_EXPORT_SCHEMA_VERSION);
		expect(file.scope).toBe("all");
		expect(file.appVersion).toBe("1.2.3");
		expect(file.rules).toEqual([rule]);
	});
});

describe("parseRulesExportFile", () => {
	it("rejects non-JSON", () => {
		const result = parseRulesExportFile("not json");
		expect(result.error).toBeTruthy();
		expect(result.rules).toEqual([]);
	});

	it("rejects a valid Smart Prompts export (cross-kind)", () => {
		const result = parseRulesExportFile(
			JSON.stringify({ kind: "tuicommander-smart-prompts", schemaVersion: 1, scope: "all", prompts: [] }),
		);
		expect(result.error).toBeTruthy();
	});

	it("rejects a schemaVersion newer than this app understands", () => {
		const result = parseRulesExportFile(
			JSON.stringify({ kind: RULES_EXPORT_KIND, schemaVersion: RULES_EXPORT_SCHEMA_VERSION + 1, rules: [] }),
		);
		expect(result.error).toBeTruthy();
	});

	it("rejects a top-level value that parses but isn't an object", () => {
		expect(parseRulesExportFile("42").error).toBeTruthy();
		expect(parseRulesExportFile("null").error).toBeTruthy();
	});

	it("rejects a file whose rules field isn't an array", () => {
		const result = parseRulesExportFile(
			JSON.stringify({ kind: RULES_EXPORT_KIND, schemaVersion: RULES_EXPORT_SCHEMA_VERSION, rules: "oops" }),
		);
		expect(result.error).toBeTruthy();
	});

	it("parses a well-formed export", () => {
		const file = buildRulesExportFile([makeRule()], "all");
		const result = parseRulesExportFile(JSON.stringify(file));
		expect(result.error).toBeUndefined();
		expect(result.rules).toHaveLength(1);
		expect(result.rules[0].id).toBe("iterm-word");
	});

	it("drops an entry missing an id and warns", () => {
		const file = buildRulesExportFile([makeRule()], "all");
		const broken = { ...file, rules: [...file.rules, { name: "No id" }] };
		const result = parseRulesExportFile(JSON.stringify(broken));
		expect(result.rules).toHaveLength(1);
		expect(result.warnings.some((w) => w.includes("missing an id"))).toBe(true);
	});

	it("drops an entry missing a name and warns", () => {
		const file = buildRulesExportFile([makeRule()], "all");
		const broken = { ...file, rules: [...file.rules, { id: "r2", regex: "x" }] };
		const result = parseRulesExportFile(JSON.stringify(broken));
		expect(result.rules).toHaveLength(1);
		expect(result.warnings.some((w) => w.includes("missing a name"))).toBe(true);
	});

	it("drops an entry with a missing or empty regex and warns", () => {
		const file = buildRulesExportFile([makeRule()], "all");
		const broken = { ...file, rules: [...file.rules, { id: "r2", name: "No regex", regex: "" }] };
		const result = parseRulesExportFile(JSON.stringify(broken));
		expect(result.rules).toHaveLength(1);
		expect(result.warnings.some((w) => w.includes("missing a pattern"))).toBe(true);
	});

	it("drops an entry with an uncompilable regex and warns", () => {
		const file = buildRulesExportFile([makeRule()], "all");
		const broken = { ...file, rules: [...file.rules, { id: "r2", name: "Bad regex", regex: "(" }] };
		const result = parseRulesExportFile(JSON.stringify(broken));
		expect(result.rules).toHaveLength(1);
		expect(result.warnings.some((w) => w.includes("invalid regular expression"))).toBe(true);
	});

	it("drops a non-object entry (null) and warns", () => {
		const file = buildRulesExportFile([makeRule()], "all");
		const broken = { ...file, rules: [...file.rules, null] };
		const result = parseRulesExportFile(JSON.stringify(broken));
		expect(result.rules).toHaveLength(1);
		expect(result.warnings.some((w) => w.includes("not a valid rule"))).toBe(true);
	});

	it("clamps an unknown precision to normal and warns instead of rejecting", () => {
		const file = buildRulesExportFile([makeRule()], "all");
		const raw = JSON.parse(JSON.stringify(file));
		raw.rules[0].precision = "extreme";
		const result = parseRulesExportFile(JSON.stringify(raw));
		expect(result.rules[0].precision).toBe("normal");
		expect(result.warnings.some((w) => w.includes("unknown precision"))).toBe(true);
	});

	it("clamps an unknown action kind to copy and warns instead of rejecting", () => {
		const file = buildRulesExportFile([makeRule()], "all");
		const raw = JSON.parse(JSON.stringify(file));
		raw.rules[0].actions[0].kind = "delete_everything";
		const result = parseRulesExportFile(JSON.stringify(raw));
		expect(result.rules[0].actions[0].kind).toBe("copy");
		expect(result.warnings.some((w) => w.includes("unknown action kind"))).toBe(true);
	});

	it("warns on a completely missing action kind, same as an explicitly invalid one", () => {
		const file = buildRulesExportFile([makeRule()], "all");
		const raw = JSON.parse(JSON.stringify(file));
		delete raw.rules[0].actions[0].kind;
		const result = parseRulesExportFile(JSON.stringify(raw));
		expect(result.rules[0].actions[0].kind).toBe("copy");
		expect(result.warnings.some((w) => w.includes("missing action kind"))).toBe(true);
	});

	it('treats a stringly is_default of "false" as false, not a Boolean()-coerced true', () => {
		const file = buildRulesExportFile([makeRule()], "all");
		const raw = JSON.parse(JSON.stringify(file));
		raw.rules[0].actions[0].is_default = "false";
		const result = parseRulesExportFile(JSON.stringify(raw));
		expect(result.rules[0].actions[0].isDefault).toBe(false);
		expect(result.warnings.some((w) => w.includes('"isDefault" was not a real boolean'))).toBe(true);
	});

	it("treats a numeric enabled of 0 as false, not a strict-inequality-coerced true", () => {
		const file = buildRulesExportFile([makeRule()], "all");
		const raw = JSON.parse(JSON.stringify(file));
		raw.rules[0].enabled = 0;
		const result = parseRulesExportFile(JSON.stringify(raw));
		expect(result.rules[0].enabled).toBe(false);
		expect(result.warnings.some((w) => w.includes('"enabled" was not a real boolean'))).toBe(true);
	});

	it("defaults enabled to true when genuinely absent, without warning", () => {
		const file = buildRulesExportFile([makeRule()], "all");
		const raw = JSON.parse(JSON.stringify(file));
		delete raw.rules[0].enabled;
		const result = parseRulesExportFile(JSON.stringify(raw));
		expect(result.rules[0].enabled).toBe(true);
		expect(result.warnings.some((w) => w.includes("enabled"))).toBe(false);
	});

	it("accepts a snake_case is_default field (a config.json slice, not our own export)", () => {
		const raw = {
			kind: RULES_EXPORT_KIND,
			schemaVersion: 1,
			exportedAt: 0,
			scope: "all",
			rules: [
				{
					id: "r1",
					name: "R1",
					regex: "x",
					precision: "normal",
					enabled: true,
					actions: [{ kind: "copy", title: "Copy", parameter: "\\0", is_default: true }],
				},
			],
		};
		const result = parseRulesExportFile(JSON.stringify(raw));
		expect(result.rules[0].actions[0].isDefault).toBe(true);
	});

	it("keeps the first of multiple default actions and warns", () => {
		const file = buildRulesExportFile(
			[
				makeRule({
					actions: [makeAction({ title: "A", isDefault: true }), makeAction({ title: "B", isDefault: true })],
				}),
			],
			"all",
		);
		const result = parseRulesExportFile(JSON.stringify(file));
		expect(result.rules[0].actions[0].isDefault).toBe(true);
		expect(result.rules[0].actions[1].isDefault).toBe(false);
		expect(result.warnings.some((w) => w.includes("more than one default action"))).toBe(true);
	});

	it("dedupes a duplicate id within the same file, keeping the last occurrence in place", () => {
		const file = buildRulesExportFile(
			[
				makeRule({ id: "dup", name: "First" }),
				makeRule({ id: "other", name: "Other" }),
				makeRule({
					id: "dup",
					name: "Second",
				}),
			],
			"all",
		);
		const result = parseRulesExportFile(JSON.stringify(file));
		expect(result.rules).toHaveLength(2);
		expect(result.rules[0].name).toBe("Second");
		expect(result.rules[1].name).toBe("Other");
		expect(result.warnings.some((w) => w.includes("duplicate id"))).toBe(true);
	});

	it("warns when actions is missing entirely but still imports the rule", () => {
		const file = buildRulesExportFile([makeRule()], "all");
		const raw = JSON.parse(JSON.stringify(file));
		delete raw.rules[0].actions;
		const result = parseRulesExportFile(JSON.stringify(raw));
		expect(result.rules[0].actions).toEqual([]);
		expect(result.warnings.some((w) => w.includes("has no actions"))).toBe(true);
	});
});

describe("classifyRuleImport", () => {
	it("marks an unknown id as new and a known id as conflict", () => {
		const existingIds = new Set(["iterm-word"]);
		const incoming = [makeRule(), makeRule({ id: "my-custom" })];
		const result = classifyRuleImport(incoming, existingIds);
		expect(result.find((c) => c.rule.id === "iterm-word")?.status).toBe("conflict");
		expect(result.find((c) => c.rule.id === "my-custom")?.status).toBe("new");
	});

	it.each(["run_command", "run_command_new_terminal", "send_text"] as const)(
		"flags a rule with a %s action as needing review",
		(kind) => {
			const result = classifyRuleImport(
				[makeRule({ actions: [makeAction({ kind }) as SmartSelectionAction] })],
				new Set(),
			);
			expect(result[0].needsReview).toBe(true);
		},
	);

	it.each(["copy", "open_url", "open_file", "ask_ai"] as const)("does not flag a %s-only rule", (kind) => {
		const result = classifyRuleImport(
			[makeRule({ actions: [makeAction({ kind }) as SmartSelectionAction] })],
			new Set(),
		);
		expect(result[0].needsReview).toBe(false);
	});
});

describe("ruleRunsCode", () => {
	it("is true when any action (not just the first) is risky", () => {
		const rule = makeRule({ actions: [makeAction(), makeAction({ kind: "run_command" })] });
		expect(ruleRunsCode(rule)).toBe(true);
	});
});

describe("mergeImportedRules", () => {
	it("replaces a conflicting id in place, preserving position", () => {
		const current = [
			makeRule({ id: "a", name: "A" }),
			makeRule({ id: "b", name: "B" }),
			makeRule({
				id: "c",
				name: "C",
			}),
		];
		const incoming = [makeRule({ id: "b", name: "B edited" })];
		const { merged } = mergeImportedRules(current, incoming);
		expect(merged.map((r) => r.id)).toEqual(["a", "b", "c"]);
		expect(merged[1].name).toBe("B edited");
	});

	it("appends a new id", () => {
		const current = [makeRule({ id: "a" })];
		const incoming = [makeRule({ id: "new", name: "New" })];
		const { merged } = mergeImportedRules(current, incoming);
		expect(merged.map((r) => r.id)).toEqual(["a", "new"]);
	});

	it("forces a risky rule to enabled: false and names it in disabled, regardless of the file's own enabled value", () => {
		const current: SmartSelectionRule[] = [];
		const incoming = [
			makeRule({ id: "risky", name: "Risky", enabled: true, actions: [makeAction({ kind: "run_command" })] }),
		];
		const { merged, disabled } = mergeImportedRules(current, incoming);
		expect(merged[0].enabled).toBe(false);
		expect(disabled).toEqual(["Risky"]);
	});

	it("leaves a non-risky rule's enabled state untouched", () => {
		const current: SmartSelectionRule[] = [];
		const incoming = [makeRule({ id: "safe", enabled: false })];
		const { merged, disabled } = mergeImportedRules(current, incoming);
		expect(merged[0].enabled).toBe(false);
		expect(disabled).toEqual([]);
	});

	it("merging a single conflict into the full default set leaves its length unchanged", () => {
		const defaults = Array.from({ length: 19 }, (_, i) => makeRule({ id: `d${i}`, name: `D${i}` }));
		const { merged } = mergeImportedRules(defaults, [makeRule({ id: "d5", name: "D5 edited" })]);
		expect(merged).toHaveLength(19);
		expect(merged[5].name).toBe("D5 edited");
	});
});
