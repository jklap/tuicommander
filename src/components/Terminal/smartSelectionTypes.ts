/** Shared smart-selection types — the wire (snake_case, matches
 *  `src-tauri/src/config.rs`'s `SmartSelectionRule`/`SmartSelectionAction`)
 *  and app (camelCase) shapes, plus the conversion between them. Split out
 *  from `settings.ts` so the matching/engine modules (Phase 2+) can import
 *  the plain types without pulling in the whole settings store. */

export type WordSelectionMode = "characters" | "regex";
export type DoubleClickAction = "word" | "smart";

/** Mirrors iTerm2's five precision classes — see `PRECISION_WEIGHT` in
 *  `smartSelection.ts` for the numeric weights each resolves to. */
export type SmartSelectionPrecision = "very_low" | "low" | "normal" | "high" | "very_high";

export type SmartSelectionActionKind =
	| "copy"
	| "open_url"
	| "open_file"
	| "send_text"
	| "run_command"
	| "run_command_new_terminal"
	| "ask_ai";

export interface SmartSelectionAction {
	kind: SmartSelectionActionKind;
	title: string;
	/** May reference the match via \0-\9 (whole match / capture groups), \d
	 *  (cwd), \u (user), \h (host) — see `substituteActionParameter`. */
	parameter: string;
	/** At most one action per rule may set this — it's what
	 *  Option/Alt+double-click runs. */
	isDefault: boolean;
}

export interface SmartSelectionRule {
	id: string;
	name: string;
	regex: string;
	precision: SmartSelectionPrecision;
	enabled: boolean;
	actions: SmartSelectionAction[];
}

/** Wire shape persisted in `config.json` (snake_case, matches serde). */
export interface RustSmartSelectionAction {
	kind: string;
	title: string;
	parameter: string;
	is_default: boolean;
}

export interface RustSmartSelectionRule {
	id: string;
	name: string;
	regex: string;
	precision: string;
	enabled: boolean;
	actions: RustSmartSelectionAction[];
}

const VALID_KINDS: readonly SmartSelectionActionKind[] = [
	"copy",
	"open_url",
	"open_file",
	"send_text",
	"run_command",
	"run_command_new_terminal",
	"ask_ai",
];
const VALID_PRECISIONS: readonly SmartSelectionPrecision[] = ["very_low", "low", "normal", "high", "very_high"];

function isValidKind(value: string): value is SmartSelectionActionKind {
	return (VALID_KINDS as readonly string[]).includes(value);
}

function isValidPrecision(value: string): value is SmartSelectionPrecision {
	return (VALID_PRECISIONS as readonly string[]).includes(value);
}

export function actionFromWire(action: RustSmartSelectionAction): SmartSelectionAction {
	return {
		kind: isValidKind(action.kind) ? action.kind : "copy",
		title: action.title,
		parameter: action.parameter,
		isDefault: action.is_default,
	};
}

export function actionToWire(action: SmartSelectionAction): RustSmartSelectionAction {
	return { kind: action.kind, title: action.title, parameter: action.parameter, is_default: action.isDefault };
}

export function ruleFromWire(rule: RustSmartSelectionRule): SmartSelectionRule {
	return {
		id: rule.id,
		name: rule.name,
		regex: rule.regex,
		precision: isValidPrecision(rule.precision) ? rule.precision : "normal",
		enabled: rule.enabled,
		// A hand-edited or corrupted config.json can omit `actions` on a rule
		// entirely; `?? []` keeps that one rule actionless instead of throwing
		// inside settings.ts's hydrate(), which would otherwise disable
		// persistence for the rest of the session.
		actions: (rule.actions ?? []).map(actionFromWire),
	};
}

export function ruleToWire(rule: SmartSelectionRule): RustSmartSelectionRule {
	return {
		id: rule.id,
		name: rule.name,
		regex: rule.regex,
		precision: rule.precision,
		enabled: rule.enabled,
		actions: rule.actions.map(actionToWire),
	};
}
