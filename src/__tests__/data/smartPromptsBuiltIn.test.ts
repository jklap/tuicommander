import { describe, expect, it } from "vitest";
import { SMART_PROMPTS_BUILTIN } from "../../data/smartPromptsBuiltIn";

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
});
