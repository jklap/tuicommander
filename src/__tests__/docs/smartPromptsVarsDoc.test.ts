import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { CONTEXT_VARIABLES } from "../../data/contextVariables";

/**
 * The user-facing docs promise a complete list of Smart Prompts context
 * variables. This is the guard that would have caught the original drift:
 * both smart-prompts.md and FEATURES.md had fallen behind the actual
 * resolvable set, and prompt-library.md documented a `{{var}}` double-brace
 * syntax no code path has ever supported (`{{diff}}` extracts as the literal
 * variable name `{diff` and never resolves).
 */

function readDoc(relativePath: string): string {
	return readFileSync(join(process.cwd(), relativePath), "utf8");
}

describe("Smart Prompts variable docs", () => {
	const smartPromptsDoc = readDoc("docs/user-guide/smart-prompts.md");
	const featuresDoc = readDoc("docs/FEATURES.md");
	const promptLibraryDoc = readDoc("docs/user-guide/prompt-library.md");

	it("smart-prompts.md documents every registered variable", () => {
		const undocumented = CONTEXT_VARIABLES.filter((v) => !smartPromptsDoc.includes(`{${v.name}}`)).map((v) => v.name);
		expect(undocumented).toEqual([]);
	});

	it("FEATURES.md documents every registered variable", () => {
		const undocumented = CONTEXT_VARIABLES.filter((v) => !featuresDoc.includes(`{${v.name}}`)).map((v) => v.name);
		expect(undocumented).toEqual([]);
	});

	it("no doc uses the unsupported {{var}} double-brace syntax for a registered variable", () => {
		// `{{diff}}` is not a stricter/nested form of `{diff}` — prompt.rs's
		// byte-scanner extracts it as the variable named "{diff" (greedy from
		// the first "{" to the first "}"), which then never resolves.
		const offendingDocs: string[] = [];
		for (const [label, doc] of [
			["smart-prompts.md", smartPromptsDoc],
			["FEATURES.md", featuresDoc],
			["prompt-library.md", promptLibraryDoc],
		] as const) {
			for (const v of CONTEXT_VARIABLES) {
				if (doc.includes(`{{${v.name}}}`)) {
					offendingDocs.push(`${label}: {{${v.name}}}`);
				}
			}
		}
		expect(offendingDocs).toEqual([]);
	});
});
