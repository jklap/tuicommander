import type { SavedPrompt } from "../stores/promptLibrary";
import { sanitizePrompt } from "./promptSanitize";

/** Envelope kind embedded in every export file, so import can reject a file that isn't a
 *  Smart Prompts export (e.g. a keybindings or theme export with the same `.json` extension). */
export const PROMPT_EXPORT_KIND = "tuicommander-smart-prompts";

/** Bump when the export shape changes in a way older versions of this app can't read. */
export const PROMPT_EXPORT_SCHEMA_VERSION = 1;

export type ExportScope = "all" | "modified" | "custom";

export interface PromptExportFile {
	kind: typeof PROMPT_EXPORT_KIND;
	schemaVersion: number;
	exportedAt: number;
	appVersion?: string;
	scope: ExportScope;
	prompts: SavedPrompt[];
}

export type ImportStatus = "new" | "conflict";

export interface ImportCandidate {
	prompt: SavedPrompt;
	status: ImportStatus;
	/** True when the prompt runs code (`shell`/`api`) and should be reviewed before enabling. */
	needsReview: boolean;
}

export interface ParsedImport {
	prompts: SavedPrompt[];
	warnings: string[];
	error?: string;
}

/** Fields compared when deciding whether a prompt differs from its built-in default.
 *  Deliberately excludes volatile bookkeeping — `createdAt`, `updatedAt`, `lastUsed`,
 *  `builtInVersion` — none of which reflect a user's deliberate change. */
const COMPARED_FIELDS = [
	"content",
	"name",
	"description",
	"placement",
	"enabled",
	"shortcut",
	"executionMode",
	"injectTarget",
	"autoExecute",
	"requiresIdle",
	"preferredAgent",
	"outputTarget",
	"systemPrompt",
	"icon",
	"isFavorite",
] as const satisfies ReadonlyArray<keyof SavedPrompt>;

/** Arrays (e.g. `placement`) compare order-insensitively; a copy is sorted so the store's
 *  own array is never mutated. */
function normalizeField(value: unknown): unknown {
	if (Array.isArray(value)) return [...value].sort();
	return value;
}

function fieldsEqual(a: SavedPrompt, b: SavedPrompt, field: keyof SavedPrompt): boolean {
	const av = normalizeField(a[field]);
	const bv = normalizeField(b[field]);
	if (Array.isArray(av) && Array.isArray(bv)) {
		return av.length === bv.length && av.every((v, i) => v === bv[i]);
	}
	return av === bv;
}

/** True if `prompt` differs from its built-in `def` on any field a user could have changed. */
export function differsFromBuiltIn(prompt: SavedPrompt, def: SavedPrompt): boolean {
	return COMPARED_FIELDS.some((field) => !fieldsEqual(prompt, def, field));
}

/** True if `prompt` counts as "modified" for export purposes: a built-in whose meaningful
 *  fields diverge from its default, or any custom (non-built-in) prompt — a prompt the user
 *  created has no default to compare against, so it is modified by definition. */
export function isModified(prompt: SavedPrompt, builtinById: Map<string, SavedPrompt>): boolean {
	if (!prompt.builtIn) return true;
	const def = builtinById.get(prompt.id);
	return def ? differsFromBuiltIn(prompt, def) : true;
}

/** Select the prompts to include for a given export scope. */
export function selectForExport(
	all: SavedPrompt[],
	scope: ExportScope,
	builtinById: Map<string, SavedPrompt>,
): SavedPrompt[] {
	switch (scope) {
		case "all":
			return all;
		case "custom":
			return all.filter((p) => !p.builtIn);
		case "modified":
			return all.filter((p) => isModified(p, builtinById));
	}
}

/** Build the exportable envelope for a set of prompts. Strips `lastUsed` — it's machine-local
 *  usage data that has no business being in a file handed to someone else. */
export function buildExportFile(prompts: SavedPrompt[], scope: ExportScope, appVersion?: string): PromptExportFile {
	return {
		kind: PROMPT_EXPORT_KIND,
		schemaVersion: PROMPT_EXPORT_SCHEMA_VERSION,
		exportedAt: Date.now(),
		appVersion,
		scope,
		prompts: prompts.map(({ lastUsed: _lastUsed, ...rest }) => rest),
	};
}

/** Parse and validate an imported export file's raw text. Rejects non-JSON, a mismatched
 *  `kind`, and a `schemaVersion` newer than this app understands. Each surviving prompt is run
 *  through the same security-relevant sanitization as prompts loaded from local storage — an
 *  import file is untrusted input that may have come from another machine or another person. */
export function parseExportFile(text: string): ParsedImport {
	let raw: unknown;
	try {
		raw = JSON.parse(text);
	} catch {
		return { prompts: [], warnings: [], error: "File is not valid JSON" };
	}

	if (!raw || typeof raw !== "object") {
		return { prompts: [], warnings: [], error: "File does not contain a Smart Prompts export" };
	}
	const file = raw as Partial<PromptExportFile>;
	if (file.kind !== PROMPT_EXPORT_KIND) {
		return { prompts: [], warnings: [], error: "File is not a Smart Prompts export" };
	}
	if (typeof file.schemaVersion !== "number" || file.schemaVersion > PROMPT_EXPORT_SCHEMA_VERSION) {
		return {
			prompts: [],
			warnings: [],
			error: `File was exported by a newer version of the app (schema ${file.schemaVersion}) and can't be imported here`,
		};
	}
	if (!Array.isArray(file.prompts)) {
		return { prompts: [], warnings: [], error: "File does not contain any prompts" };
	}

	const prompts: SavedPrompt[] = [];
	const warnings: string[] = [];
	file.prompts.forEach((entry, index) => {
		if (!entry || typeof entry !== "object") {
			warnings.push(`Entry ${index + 1} is not a valid prompt and was skipped`);
			return;
		}
		const candidate = entry as SavedPrompt;
		if (!candidate.id || typeof candidate.id !== "string") {
			warnings.push(`Entry ${index + 1} is missing an id and was skipped`);
			return;
		}
		if (!candidate.name || typeof candidate.name !== "string") {
			warnings.push(`Prompt "${candidate.id}" is missing a name and was skipped`);
			return;
		}
		if (typeof candidate.content !== "string") {
			warnings.push(`Prompt "${candidate.id}" is missing content and was skipped`);
			return;
		}
		const { prompt, warnings: sanitizeWarnings } = sanitizePrompt(candidate);
		warnings.push(...sanitizeWarnings);
		prompts.push(prompt);
	});

	return { prompts, warnings };
}

/** Classify each parsed prompt against the current library: an unfamiliar `id` is "new"; an
 *  `id` already present would overwrite an existing prompt on import ("conflict"). Prompts
 *  that execute code (`shell`/`api`) are flagged `needsReview` so the import dialog can warn
 *  before anything runs. */
export function classifyImport(incoming: SavedPrompt[], existing: Record<string, SavedPrompt>): ImportCandidate[] {
	return incoming.map((prompt) => ({
		prompt,
		status: existing[prompt.id] ? "conflict" : "new",
		needsReview: prompt.executionMode === "shell" || prompt.executionMode === "api",
	}));
}
