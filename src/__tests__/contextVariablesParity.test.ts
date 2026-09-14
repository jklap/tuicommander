import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { CONTEXT_VARIABLES, REPO_CONTROLLED_VARIABLES, VARIABLE_DESCRIPTIONS } from "../data/contextVariables";

/**
 * Drift guards for the Smart Prompts variable registry (`contextVariables.ts`).
 * This is the test that would have caught the original four-way drift this
 * registry consolidates — see that file's own doc comment for the history.
 */

function readRepoFile(relativePath: string): string {
	return readFileSync(join(process.cwd(), relativePath), "utf8");
}

function sliceBetween(source: string, startMarker: string, endMarker: string): string {
	const start = source.indexOf(startMarker);
	if (start < 0) throw new Error(`Start marker not found: ${startMarker}`);
	const end = source.indexOf(endMarker, start);
	if (end < 0) throw new Error(`End marker not found after start: ${endMarker}`);
	return source.slice(start, end);
}

/** `const ALL_VARS: &[&str] = &[...];` from prompt.rs, as a plain string array. */
function extractAllVars(): string[] {
	const source = readRepoFile("src-tauri/src/prompt.rs");
	const block = sliceBetween(source, "const ALL_VARS: &[&str] = &[", "];");
	return [...block.matchAll(/"([a-z_]+)"/g)].map((m) => m[1]);
}

/** Every `"name" => ...` match arm inside `resolve_single_var`, from prompt.rs. */
function extractResolveSingleVarArms(): string[] {
	const source = readRepoFile("src-tauri/src/prompt.rs");
	const block = sliceBetween(source, "fn resolve_single_var(repo_path: &str, var: &str)", "\n}\n");
	return [...block.matchAll(/"([a-z_]+)"\s*=>/g)].map((m) => m[1]);
}

/** Every `TUIC_*` literal inside `ScriptContext::pairs`, from script_env.rs —
 *  scoped to just that function so the module's own tests (which reference
 *  the same literals) don't inflate the set. */
function extractScriptEnvPairsKeys(): string[] {
	const source = readRepoFile("src-tauri/src/script_env.rs");
	const block = sliceBetween(source, "fn pairs(&self)", "pub(crate) fn apply_std");
	return [...new Set([...block.matchAll(/"(TUIC_[A-Z_]+)"/g)].map((m) => m[1]))];
}

describe("contextVariables.ts <-> prompt.rs parity", () => {
	it("every rust-sourced registry variable appears in ALL_VARS", () => {
		const allVars = new Set(extractAllVars());
		const rustNames = CONTEXT_VARIABLES.filter((v) => v.source === "rust").map((v) => v.name);
		const missing = rustNames.filter((n) => !allVars.has(n));
		expect(missing).toEqual([]);
	});

	it("every ALL_VARS entry is registered with source: rust", () => {
		const allVars = extractAllVars();
		const rustNames = new Set(CONTEXT_VARIABLES.filter((v) => v.source === "rust").map((v) => v.name));
		const unregistered = allVars.filter((n) => !rustNames.has(n));
		expect(unregistered).toEqual([]);
	});

	it("every resolve_single_var match arm is listed in ALL_VARS", () => {
		// Catches a real bug class: the per-call prompt path uses
		// extract_variables (any name found in content), so an arm added to
		// resolve_single_var without a matching ALL_VARS entry works in an
		// ordinary Smart Prompt but is silently absent from
		// resolve_context_variables (the MCP "resolve everything" path).
		const arms = extractResolveSingleVarArms();
		const allVars = new Set(extractAllVars());
		const missing = arms.filter((n) => !allVars.has(n));
		expect(missing).toEqual([]);
	});
});

describe("contextVariables.ts <-> script_env.rs parity", () => {
	it("every script-exposed variable's TUIC_ name follows the TUIC_<UPPERCASE> convention and actually exists in ScriptContext::pairs", () => {
		const pairsKeys = new Set(extractScriptEnvPairsKeys());
		const scriptVars = CONTEXT_VARIABLES.filter((v) => v.script);
		expect(scriptVars.length).toBeGreaterThan(0);
		const missing = scriptVars
			.map((v) => `TUIC_${v.name.toUpperCase()}`)
			.filter((expected) => !pairsKeys.has(expected));
		expect(missing).toEqual([]);
	});

	it("script: true only ever appears on a rust-sourced variable", () => {
		// The contract with the TUIC_* env-injection work: a Setup/Archive
		// script runs with no frontend at all, so a frontend-sourced
		// variable could never actually be resolved for it.
		const offenders = CONTEXT_VARIABLES.filter((v) => v.script && v.source !== "rust").map((v) => v.name);
		expect(offenders).toEqual([]);
	});
});

describe("contextVariables.ts internal consistency", () => {
	it("VARIABLE_DESCRIPTIONS covers every registered variable", () => {
		const missing = CONTEXT_VARIABLES.filter((v) => !(v.name in VARIABLE_DESCRIPTIONS)).map((v) => v.name);
		expect(missing).toEqual([]);
	});

	it("REPO_CONTROLLED_VARIABLES matches every repoControlled entry", () => {
		const expected = new Set(CONTEXT_VARIABLES.filter((v) => v.repoControlled).map((v) => v.name));
		expect(REPO_CONTROLLED_VARIABLES).toEqual(expected);
	});

	it("has no duplicate variable names", () => {
		const names = CONTEXT_VARIABLES.map((v) => v.name);
		expect(new Set(names).size).toBe(names.length);
	});

	it("the two pickers no longer declare their own CONTEXT_VARIABLES copy", () => {
		// Guards against someone reintroducing the drift this registry
		// consolidates — see contextVariables.ts's own doc comment.
		const smartPromptsTab = readRepoFile("src/components/SettingsPanel/tabs/SmartPromptsTab.tsx");
		const promptDrawer = readRepoFile("src/components/PromptDrawer/PromptDrawer.tsx");
		expect(smartPromptsTab).not.toContain("const CONTEXT_VARIABLES");
		expect(promptDrawer).not.toContain("const CONTEXT_VARIABLES");
	});

	it("smartPromptsBuiltIn.ts no longer declares its own VARIABLE_DESCRIPTIONS", () => {
		const source = readRepoFile("src/data/smartPromptsBuiltIn.ts");
		expect(source).not.toContain("export const VARIABLE_DESCRIPTIONS: Record<string, string> = {");
	});
});
