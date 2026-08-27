import { describe, expect, it } from "vitest";
import type { SavedPrompt } from "../../stores/promptLibrary";
import {
	buildExportFile,
	classifyImport,
	differsFromBuiltIn,
	isModified,
	PROMPT_EXPORT_KIND,
	PROMPT_EXPORT_SCHEMA_VERSION,
	parseExportFile,
	selectForExport,
} from "../../utils/promptExport";

function makePrompt(overrides: Partial<SavedPrompt> = {}): SavedPrompt {
	return {
		id: "smart-commit",
		name: "Smart Commit",
		content: "Commit the staged changes.",
		category: "custom",
		isFavorite: false,
		createdAt: 1000,
		updatedAt: 1000,
		builtIn: true,
		enabled: true,
		placement: ["toolbar", "git-changes"],
		...overrides,
	};
}

describe("differsFromBuiltIn", () => {
	it("returns false for an untouched built-in", () => {
		const def = makePrompt();
		const current = makePrompt();
		expect(differsFromBuiltIn(current, def)).toBe(false);
	});

	it("ignores placement order", () => {
		const def = makePrompt({ placement: ["toolbar", "git-changes"] });
		const current = makePrompt({ placement: ["git-changes", "toolbar"] });
		expect(differsFromBuiltIn(current, def)).toBe(false);
	});

	it("ignores volatile bookkeeping fields", () => {
		const def = makePrompt();
		const current = makePrompt({ createdAt: 999, updatedAt: 5000, lastUsed: 6000, builtInVersion: 9 });
		expect(differsFromBuiltIn(current, def)).toBe(false);
	});

	it("detects a content edit", () => {
		const def = makePrompt();
		const current = makePrompt({ content: "Different content" });
		expect(differsFromBuiltIn(current, def)).toBe(true);
	});

	it("detects an enabled-only change (disabled but otherwise untouched)", () => {
		const def = makePrompt({ enabled: true });
		const current = makePrompt({ enabled: false });
		expect(differsFromBuiltIn(current, def)).toBe(true);
	});

	it("detects a placement change", () => {
		const def = makePrompt({ placement: ["toolbar"] });
		const current = makePrompt({ placement: ["toolbar", "pr-popover"] });
		expect(differsFromBuiltIn(current, def)).toBe(true);
	});
});

describe("isModified", () => {
	const builtinById = new Map<string, SavedPrompt>([["smart-commit", makePrompt()]]);

	it("is false for an unmodified built-in", () => {
		expect(isModified(makePrompt(), builtinById)).toBe(false);
	});

	it("is true for a modified built-in", () => {
		expect(isModified(makePrompt({ content: "edited" }), builtinById)).toBe(true);
	});

	it("is true for any custom prompt, even if identical in shape to a built-in", () => {
		const custom = makePrompt({ id: "my-custom", builtIn: false });
		expect(isModified(custom, builtinById)).toBe(true);
	});

	it("is true for a built-in id with no known default (defensive: treat as modified)", () => {
		const orphan = makePrompt({ id: "smart-nonexistent" });
		expect(isModified(orphan, builtinById)).toBe(true);
	});
});

describe("selectForExport", () => {
	const builtinById = new Map<string, SavedPrompt>([["smart-commit", makePrompt()]]);
	const unmodifiedBuiltin = makePrompt();
	const modifiedBuiltin = makePrompt({ id: "smart-review", content: "edited" });
	const custom = makePrompt({ id: "my-custom", builtIn: false, name: "My Custom" });
	const all = [unmodifiedBuiltin, modifiedBuiltin, custom];

	it("'all' returns everything", () => {
		expect(selectForExport(all, "all", builtinById)).toEqual(all);
	});

	it("'custom' returns only non-built-in prompts", () => {
		expect(selectForExport(all, "custom", builtinById)).toEqual([custom]);
	});

	it("'modified' returns changed built-ins plus all custom prompts", () => {
		const builtinByIdFull = new Map<string, SavedPrompt>([
			["smart-commit", makePrompt()],
			["smart-review", makePrompt({ id: "smart-review" })],
		]);
		expect(selectForExport(all, "modified", builtinByIdFull)).toEqual([modifiedBuiltin, custom]);
	});
});

describe("buildExportFile", () => {
	it("wraps prompts in the export envelope and strips lastUsed", () => {
		const prompt = makePrompt({ lastUsed: 12345 });
		const file = buildExportFile([prompt], "all", "1.2.3");

		expect(file.kind).toBe(PROMPT_EXPORT_KIND);
		expect(file.schemaVersion).toBe(PROMPT_EXPORT_SCHEMA_VERSION);
		expect(file.scope).toBe("all");
		expect(file.appVersion).toBe("1.2.3");
		expect(file.prompts).toHaveLength(1);
		expect(file.prompts[0]).not.toHaveProperty("lastUsed");
		expect(file.prompts[0].id).toBe(prompt.id);
	});
});

describe("parseExportFile", () => {
	it("rejects non-JSON", () => {
		const result = parseExportFile("not json");
		expect(result.error).toBeTruthy();
		expect(result.prompts).toEqual([]);
	});

	it("rejects a file with the wrong kind", () => {
		const result = parseExportFile(JSON.stringify({ kind: "something-else", schemaVersion: 1, prompts: [] }));
		expect(result.error).toBeTruthy();
	});

	it("rejects a schemaVersion newer than this app understands", () => {
		const result = parseExportFile(
			JSON.stringify({ kind: PROMPT_EXPORT_KIND, schemaVersion: PROMPT_EXPORT_SCHEMA_VERSION + 1, prompts: [] }),
		);
		expect(result.error).toBeTruthy();
	});

	it("parses a well-formed export", () => {
		const file = buildExportFile([makePrompt()], "all");
		const result = parseExportFile(JSON.stringify(file));
		expect(result.error).toBeUndefined();
		expect(result.prompts).toHaveLength(1);
		expect(result.prompts[0].id).toBe("smart-commit");
	});

	it("drops an entry missing an id and warns", () => {
		const file = buildExportFile([makePrompt()], "all");
		const broken = { ...file, prompts: [...file.prompts, { name: "No id or content" }] };
		const result = parseExportFile(JSON.stringify(broken));
		expect(result.prompts).toHaveLength(1);
		expect(result.warnings.some((w) => w.includes("missing an id"))).toBe(true);
	});

	it("drops an entry missing a name (id present) and warns", () => {
		const file = buildExportFile([makePrompt()], "all");
		const broken = { ...file, prompts: [...file.prompts, { id: "p2", content: "c" }] };
		const result = parseExportFile(JSON.stringify(broken));
		expect(result.prompts).toHaveLength(1);
		expect(result.warnings.some((w) => w.includes("missing a name"))).toBe(true);
	});

	it("drops an entry missing content (id and name present) and warns", () => {
		const file = buildExportFile([makePrompt()], "all");
		const broken = { ...file, prompts: [...file.prompts, { id: "p2", name: "No content" }] };
		const result = parseExportFile(JSON.stringify(broken));
		expect(result.prompts).toHaveLength(1);
		expect(result.warnings.some((w) => w.includes("missing content"))).toBe(true);
	});

	it("drops a non-object entry (null) and warns", () => {
		const file = buildExportFile([makePrompt()], "all");
		const broken = { ...file, prompts: [...file.prompts, null] };
		const result = parseExportFile(JSON.stringify(broken));
		expect(result.prompts).toHaveLength(1);
		expect(result.warnings.some((w) => w.includes("not a valid prompt"))).toBe(true);
	});

	it("rejects a top-level value that parses but isn't an object", () => {
		expect(parseExportFile("42").error).toBeTruthy();
		expect(parseExportFile("null").error).toBeTruthy();
		expect(parseExportFile('"just a string"').error).toBeTruthy();
	});

	it("rejects a file whose prompts field isn't an array", () => {
		const result = parseExportFile(
			JSON.stringify({ kind: PROMPT_EXPORT_KIND, schemaVersion: PROMPT_EXPORT_SCHEMA_VERSION, prompts: "oops" }),
		);
		expect(result.error).toBeTruthy();
	});

	it("sanitizes an invalid executionMode instead of rejecting the whole file", () => {
		const file = buildExportFile([makePrompt({ executionMode: "shell" })], "all");
		// Corrupt the mode in the serialized payload to something invalid
		const raw = JSON.parse(JSON.stringify(file));
		raw.prompts[0].executionMode = "bogus";
		const result = parseExportFile(JSON.stringify(raw));
		expect(result.prompts[0].executionMode).toBe("inject");
		expect(result.warnings.some((w) => w.includes("invalid executionMode"))).toBe(true);
	});
});

describe("classifyImport", () => {
	it("marks an unknown id as new and a known id as conflict", () => {
		const existing = { "smart-commit": makePrompt() };
		const incoming = [makePrompt(), makePrompt({ id: "my-custom", builtIn: false })];
		const result = classifyImport(incoming, existing);
		expect(result.find((c) => c.prompt.id === "smart-commit")?.status).toBe("conflict");
		expect(result.find((c) => c.prompt.id === "my-custom")?.status).toBe("new");
	});

	it("flags shell and api prompts as needing review", () => {
		const incoming = [
			makePrompt({ executionMode: "shell" }),
			makePrompt({ id: "p2", executionMode: "api" }),
			makePrompt({ id: "p3", executionMode: "inject" }),
		];
		const result = classifyImport(incoming, {});
		expect(result.find((c) => c.prompt.id === "smart-commit")?.needsReview).toBe(true);
		expect(result.find((c) => c.prompt.id === "p2")?.needsReview).toBe(true);
		expect(result.find((c) => c.prompt.id === "p3")?.needsReview).toBe(false);
	});
});
