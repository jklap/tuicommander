import { describe, expect, it } from "vitest";
import { SMART_PROMPTS_BUILTIN, VARIABLE_DESCRIPTIONS } from "../../data/smartPromptsBuiltIn";

describe("SMART_PROMPTS_BUILTIN", () => {
	it("gives every built-in the command-palette placement", () => {
		// Regression guard for the Command Palette's "Prompts" scope chip always
		// rendering empty: no built-in ever declared this placement, so
		// getSmartByPlacement("command-palette") returned []. A future built-in
		// that forgets it would silently reintroduce the same bug.
		for (const prompt of SMART_PROMPTS_BUILTIN) {
			expect(prompt.placement, `"${prompt.id}" is missing the command-palette placement`).toContain("command-palette");
		}
	});

	it("has unique ids", () => {
		const ids = SMART_PROMPTS_BUILTIN.map((p) => p.id);
		expect(new Set(ids).size).toBe(ids.length);
	});

	it("shares the same builtInVersion across every built-in", () => {
		const versions = new Set(SMART_PROMPTS_BUILTIN.map((p) => p.builtInVersion));
		expect(versions.size).toBe(1);
	});

	it("every variable referenced by a built-in's content has a VARIABLE_DESCRIPTIONS entry", () => {
		// Gap-closing test, part of the Smart Prompts variable-registry review:
		// nothing previously checked that a built-in prompt's own {var} usages
		// are documented in the same list the Settings/PromptDrawer pickers show
		// the user. Uses prompt.rs's own byte-scan semantics (any char until the
		// next "}") rather than a \w+-only regex, so a variable name containing
		// characters outside [a-z0-9_] would still be caught here instead of
		// silently passing this guard while failing to resolve at runtime.
		const missing: string[] = [];
		for (const prompt of SMART_PROMPTS_BUILTIN) {
			const matches = prompt.content.matchAll(/\{([^{}]+)\}/g);
			for (const match of matches) {
				const name = match[1];
				if (!(name in VARIABLE_DESCRIPTIONS)) {
					missing.push(`"${prompt.id}" references undocumented variable {${name}}`);
				}
			}
		}
		expect(missing).toEqual([]);
	});
});
