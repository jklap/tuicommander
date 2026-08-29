import type {
	RustSmartSelectionAction,
	RustSmartSelectionRule,
	SmartSelectionAction,
	SmartSelectionActionKind,
	SmartSelectionRule,
} from "../components/Terminal/smartSelectionTypes";
import { ruleFromWire } from "../components/Terminal/smartSelectionTypes";
import { isValidRegex } from "./isValidRegex";
import type { ExportScope } from "./promptExport";

/** Envelope kind embedded in every export file, so import can reject a file that isn't a
 *  Smart Selection rules export (e.g. a Smart Prompts export with the same `.json` extension). */
export const RULES_EXPORT_KIND = "tuicommander-smart-selection-rules";

/** Bump when the export shape changes in a way older versions of this app can't read. */
export const RULES_EXPORT_SCHEMA_VERSION = 1;

export interface RulesExportFile {
	kind: typeof RULES_EXPORT_KIND;
	schemaVersion: number;
	exportedAt: number;
	appVersion?: string;
	scope: ExportScope;
	rules: SmartSelectionRule[];
}

export type RuleImportStatus = "new" | "conflict";

export interface RuleImportCandidate {
	rule: SmartSelectionRule;
	status: RuleImportStatus;
	/** True when the rule has an action that runs a command or sends text to the terminal —
	 *  should be reviewed before enabling. */
	needsReview: boolean;
}

export interface ParsedRuleImport {
	rules: SmartSelectionRule[];
	warnings: string[];
	error?: string;
}

/** Scalar fields compared when deciding whether a rule differs from its built-in default.
 *  `actions` is compared separately (see `actionsEqual`) since its comparison is positional,
 *  not a simple equality check. */
const RULE_COMPARED_FIELDS = ["name", "regex", "precision", "enabled"] as const satisfies ReadonlyArray<
	keyof SmartSelectionRule
>;

/** Actions are positional: their order is the right-click menu order, and which entry has
 *  `isDefault` set decides what Option/Alt+double-click runs. Unlike `promptExport`'s array
 *  fields (e.g. `placement`, a set), this must NOT sort before comparing — reordering two
 *  actions, or moving which one is default, is a real user-visible change and must count as
 *  "modified". `Boolean(...)` coercion means an `isDefault: undefined` (a hand-edited
 *  `config.json`, or a rule that predates the field) compares equal to `isDefault: false`. */
function actionsEqual(a: readonly SmartSelectionAction[] = [], b: readonly SmartSelectionAction[] = []): boolean {
	if (a.length !== b.length) return false;
	return a.every((x, i) => {
		const y = b[i];
		return (
			x.kind === y.kind &&
			x.title === y.title &&
			x.parameter === y.parameter &&
			Boolean(x.isDefault) === Boolean(y.isDefault)
		);
	});
}

/** True if `rule` differs from its built-in `def` on any field a user could have changed. */
export function differsFromDefaultRule(rule: SmartSelectionRule, def: SmartSelectionRule): boolean {
	return RULE_COMPARED_FIELDS.some((field) => rule[field] !== def[field]) || !actionsEqual(rule.actions, def.actions);
}

/** True if `rule` counts as "modified" for export purposes: a built-in whose fields diverge
 *  from its default, or any custom (non-built-in) rule — a rule the user created has no default
 *  to compare against, so it is modified by definition. */
export function isRuleModified(rule: SmartSelectionRule, defaultsById: Map<string, SmartSelectionRule>): boolean {
	const def = defaultsById.get(rule.id);
	return def ? differsFromDefaultRule(rule, def) : true;
}

/** Select the rules to include for a given export scope. `all` must already be the *effective*
 *  list (`resolveSmartSelectionRules(...)`, never the raw possibly-empty stored array) or every
 *  scope silently returns nothing. */
export function selectRulesForExport(
	all: SmartSelectionRule[],
	scope: ExportScope,
	defaultsById: Map<string, SmartSelectionRule>,
): SmartSelectionRule[] {
	switch (scope) {
		case "all":
			return all;
		case "custom":
			return all.filter((r) => !defaultsById.has(r.id));
		case "modified":
			return all.filter((r) => isRuleModified(r, defaultsById));
	}
}

/** Build the exportable envelope for a set of rules. Unlike `SavedPrompt`, `SmartSelectionRule`
 *  carries no machine-local bookkeeping (no `createdAt`/`updatedAt`/`lastUsed`), so there is
 *  nothing to strip before export. */
export function buildRulesExportFile(
	rules: SmartSelectionRule[],
	scope: ExportScope,
	appVersion?: string,
): RulesExportFile {
	return {
		kind: RULES_EXPORT_KIND,
		schemaVersion: RULES_EXPORT_SCHEMA_VERSION,
		exportedAt: Date.now(),
		appVersion,
		scope,
		rules,
	};
}

/** Read a field from untrusted JSON that should be a boolean. A real `true`/`false` is used as
 *  given; a genuinely absent field (`undefined`/`null`) falls back to `fallbackWhenAbsent`
 *  (`enabled` defaults to true when omitted, `isDefault` to false). Anything else — a stringly
 *  `"false"`, a numeric `0`, or any other JSON type reaching here from a hand-edited or
 *  adversarial file — is `ambiguous`: a naive `Boolean(value)` would silently treat `"false"` and
 *  `0` as `true` (both are non-empty/non-zero-length, hence truthy), inverting the author's
 *  intent with no trace. Resolve ambiguous values to `false` — for both fields, "not
 *  enabled"/"not the default action" is the lower-blast-radius guess — and let the caller decide
 *  whether to warn. */
function readBooleanField(value: unknown, fallbackWhenAbsent: boolean): { value: boolean; ambiguous: boolean } {
	if (value === undefined || value === null) return { value: fallbackWhenAbsent, ambiguous: false };
	if (typeof value === "boolean") return { value, ambiguous: false };
	return { value: false, ambiguous: true };
}

/** Parse and validate an imported export file's raw text. Rejects non-JSON, a mismatched
 *  `kind` (including a valid Smart Prompts export — the two features use distinct envelope
 *  kinds on purpose), and a `schemaVersion` newer than this app understands. Each surviving
 *  entry is normalized through the same wire-shape validation as a rule loaded from
 *  `config.json` (`ruleFromWire`) — an import file is untrusted input that may have come from
 *  another machine or another person. */
export function parseRulesExportFile(text: string): ParsedRuleImport {
	let raw: unknown;
	try {
		raw = JSON.parse(text);
	} catch {
		return { rules: [], warnings: [], error: "File is not valid JSON" };
	}

	if (!raw || typeof raw !== "object") {
		return { rules: [], warnings: [], error: "File does not contain a Smart Selection rules export" };
	}
	const file = raw as Partial<RulesExportFile>;
	if (file.kind !== RULES_EXPORT_KIND) {
		return { rules: [], warnings: [], error: "File is not a Smart Selection rules export" };
	}
	if (typeof file.schemaVersion !== "number" || file.schemaVersion > RULES_EXPORT_SCHEMA_VERSION) {
		return {
			rules: [],
			warnings: [],
			error: `File was exported by a newer version of the app (schema ${file.schemaVersion}) and can't be imported here`,
		};
	}
	if (!Array.isArray(file.rules)) {
		return { rules: [], warnings: [], error: "File does not contain any rules" };
	}

	const rules: SmartSelectionRule[] = [];
	const warnings: string[] = [];
	// Maps id -> index in `rules`, so a duplicate id within the same file replaces the earlier
	// entry in place rather than producing two rows that would fight over the same
	// `expandedIds` key and the same `default-action-${id}` radio-group name in the editor.
	const idToIndex = new Map<string, number>();

	file.rules.forEach((entry, index) => {
		if (!entry || typeof entry !== "object") {
			warnings.push(`Entry ${index + 1} is not a valid rule and was skipped`);
			return;
		}
		// `file.rules` is typed as `SmartSelectionRule[]`, but this is untrusted input that may
		// not actually have that shape at runtime — go through `unknown` so every field below
		// gets a real `typeof` check instead of a false sense of safety from the static type.
		const candidate = entry as unknown as Record<string, unknown>;

		const id = candidate.id;
		if (!id || typeof id !== "string") {
			warnings.push(`Entry ${index + 1} is missing an id and was skipped`);
			return;
		}
		const name = candidate.name;
		if (!name || typeof name !== "string") {
			warnings.push(`Rule "${id}" is missing a name and was skipped`);
			return;
		}
		const regex = candidate.regex;
		if (!regex || typeof regex !== "string") {
			warnings.push(`Rule "${name}" is missing a pattern and was skipped`);
			return;
		}
		if (!isValidRegex(regex)) {
			warnings.push(`Rule "${name}" has an invalid regular expression and was skipped`);
			return;
		}

		const actionsField = candidate.actions;
		const rawActions = Array.isArray(actionsField) ? (actionsField as unknown as Record<string, unknown>[]) : [];
		if (!Array.isArray(actionsField)) {
			warnings.push(`Rule "${name}" has no actions`);
		}

		const wireActions: RustSmartSelectionAction[] = rawActions.map((a) => {
			// Our own export writes the app (camelCase, `isDefault`) shape verbatim, but a user
			// may instead paste in a slice of config.json's `smart_selection_rules`, which is
			// snake_case (`is_default`). Accept either — `ruleFromWire` only reads `is_default`,
			// so a camelCase-only file would otherwise silently lose every default-action flag.
			const isDefault = readBooleanField(a?.is_default ?? a?.isDefault, false);
			if (isDefault.ambiguous) {
				warnings.push(`Rule "${name}": an action's "isDefault" was not a real boolean — treated as false`);
			}
			return {
				kind: typeof a?.kind === "string" ? a.kind : "",
				title: typeof a?.title === "string" ? a.title : "",
				parameter: typeof a?.parameter === "string" ? a.parameter : "",
				is_default: isDefault.value,
			};
		});

		const wirePrecision = typeof candidate.precision === "string" ? candidate.precision : "";
		const enabled = readBooleanField(candidate.enabled, true);
		if (enabled.ambiguous) {
			warnings.push(`Rule "${name}": "enabled" was not a real boolean — treated as false`);
		}
		const wire: RustSmartSelectionRule = {
			id,
			name,
			regex,
			precision: wirePrecision,
			enabled: enabled.value,
			actions: wireActions,
		};

		const rule = ruleFromWire(wire);

		// `ruleFromWire` clamps an unrecognized precision/kind silently; surface that here so an
		// import doesn't quietly change a rule's behavior with no visible trace. Compared
		// unconditionally (not just when `a.kind` is truthy) so a completely missing `kind` warns
		// exactly like an explicitly-invalid one — both silently become "copy" otherwise.
		if (wirePrecision && rule.precision !== wirePrecision) {
			warnings.push(`Rule "${name}": unknown precision "${wirePrecision}" — using Normal`);
		}
		wireActions.forEach((a, i) => {
			if (rule.actions[i]?.kind === a.kind) return;
			const label = a.kind ? `unknown action kind "${a.kind}"` : "a missing action kind";
			warnings.push(`Rule "${name}": ${label} — imported as Copy`);
		});

		// The editor enforces at most one default action per rule via a radio group; a hand-
		// edited or malicious file has no such constraint. Keep the first, clear the rest.
		const defaultIndexes = rule.actions.flatMap((a, i) => (a.isDefault ? [i] : []));
		if (defaultIndexes.length > 1) {
			for (const i of defaultIndexes.slice(1)) rule.actions[i] = { ...rule.actions[i], isDefault: false };
			warnings.push(`Rule "${name}" had more than one default action — keeping the first`);
		}

		const existingIndex = idToIndex.get(id);
		if (existingIndex !== undefined) {
			warnings.push(`Rule "${name}" has a duplicate id "${id}" in this file — keeping the last occurrence`);
			rules[existingIndex] = rule;
			return;
		}
		idToIndex.set(id, rules.length);
		rules.push(rule);
	});

	return { rules, warnings };
}

/** Classify each parsed rule against the current effective rule list: an unfamiliar `id` is
 *  "new"; an `id` already present would overwrite an existing rule on import ("conflict").
 *  Rules with an action that runs a command or sends text to the terminal are flagged
 *  `needsReview` so the import dialog can warn before anything runs. */
export function classifyRuleImport(
	incoming: SmartSelectionRule[],
	existingIds: ReadonlySet<string>,
): RuleImportCandidate[] {
	return incoming.map((rule) => ({
		rule,
		status: existingIds.has(rule.id) ? "conflict" : "new",
		needsReview: ruleRunsCode(rule),
	}));
}

/** Action kinds that either run a shell command or write into a live PTY — the smart-selection
 *  analogue of a Smart Prompt's `shell`/`api` execution mode. `send_text` is included because at
 *  a shell prompt it's one Enter keystroke from executing. */
const RISKY_ACTION_KINDS: ReadonlySet<SmartSelectionActionKind> = new Set([
	"run_command",
	"run_command_new_terminal",
	"send_text",
]);

/** True if any of the rule's actions runs a command or sends text to the terminal.
 *  `SmartSelectionAction` has no per-action enabled flag, so a rule mixing a risky action with a
 *  safe one (e.g. `copy` + `run_command`) has no representable "half-disabled" state — the whole
 *  rule imports disabled. */
export function ruleRunsCode(rule: SmartSelectionRule): boolean {
	return (rule.actions ?? []).some((a) => RISKY_ACTION_KINDS.has(a.kind));
}

/** Merge imported rules into the current effective list (already resolved via
 *  `resolveSmartSelectionRules` — never the raw possibly-empty stored array). A conflicting id
 *  is replaced in place, preserving its position; a new id is appended. Any rule that runs a
 *  command or sends text to the terminal is forced `enabled: false` regardless of the file's own
 *  value, and its name is collected into `disabled` for the caller's toast — mirroring
 *  `promptLibraryStore.importPrompts`'s handling of `shell`/`api` prompts.
 *
 *  Note for callers: merging into an empty stored list (the "use built-in defaults" sentinel)
 *  writes back the full effective list, permanently materializing every built-in rule into
 *  `config.json` — exactly as editing a single rule in the Selection tab already does. "Restore
 *  built-in defaults" remains the escape hatch. */
export function mergeImportedRules(
	current: SmartSelectionRule[],
	incoming: SmartSelectionRule[],
): { merged: SmartSelectionRule[]; disabled: string[] } {
	const disabled: string[] = [];
	const sanitize = (rule: SmartSelectionRule): SmartSelectionRule => {
		if (!ruleRunsCode(rule)) return rule;
		disabled.push(rule.name);
		return { ...rule, enabled: false };
	};

	const incomingById = new Map(incoming.map((r) => [r.id, r]));
	const merged = current.map((r) => {
		const replacement = incomingById.get(r.id);
		return replacement ? sanitize(replacement) : r;
	});

	const currentIds = new Set(current.map((r) => r.id));
	for (const rule of incoming) {
		if (!currentIds.has(rule.id)) merged.push(sanitize(rule));
	}

	return { merged, disabled };
}
