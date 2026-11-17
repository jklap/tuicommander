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
	| "run_background"
	| "ask_ai";

/** `send_text` only: press Enter after typing? `"when_safe"` presses it only when that cannot run
 *  something unintended — always with an agent in the foreground, in a plain shell only if the
 *  text is inert (`isInertShellText`: letters, digits, spaces and `_ . / : @ % + = , ~ -`). There
 *  is deliberately no "always": forcing Enter on untrusted
 *  terminal text is the injection that check exists to prevent (Run Command quotes its values and
 *  is the safe way to run something unconditionally). */
export type SmartSelectionSendReturn = "never" | "when_safe";

/** `run_background` ("Run Command") only: where it runs. */
export type SmartSelectionRunIn = "background" | "new_terminal";

export interface SmartSelectionAction {
	kind: SmartSelectionActionKind;
	title: string;
	/** May reference the match via \0-\9 (whole match / capture groups), \d
	 *  (cwd), \u (user), \h (host) — see `substituteActionParameter`. */
	parameter: string;
	/** At most one action per rule may set this — it's what
	 *  Option/Alt+double-click runs. */
	isDefault: boolean;
	/** `send_text` only. Absent means `"never"`: a default is never stored, so a rule that does
	 *  not use the option looks (and hashes) exactly as it did before the option existed. */
	sendReturn?: SmartSelectionSendReturn;
	/** `run_background` only. Absent means `"background"`; same rule as `sendReturn`. */
	runIn?: SmartSelectionRunIn;
}

export interface SmartSelectionRule {
	id: string;
	name: string;
	regex: string;
	precision: SmartSelectionPrecision;
	enabled: boolean;
	actions: SmartSelectionAction[];
	/** Only on a rule that started life as a shipped built-in: the content hash of the
	 *  shipped definition this saved copy was last reconciled against
	 *  (`reconcileBuiltInRules`). Comparing it with a hash of the rule's CURRENT content
	 *  says whether the user has edited it, which is what lets a release update an
	 *  untouched rule and leave an edited one alone. Never exported or imported. */
	builtInHash?: string;
}

/** Wire shape persisted in `config.json` (snake_case, matches serde). */
export interface RustSmartSelectionAction {
	kind: string;
	title: string;
	parameter: string;
	is_default: boolean;
	send_return?: string;
	run_in?: string;
}

export interface RustSmartSelectionRule {
	id: string;
	name: string;
	regex: string;
	precision: string;
	enabled: boolean;
	actions: RustSmartSelectionAction[];
	built_in_hash?: string;
}

const VALID_KINDS: readonly SmartSelectionActionKind[] = [
	"copy",
	"open_url",
	"open_file",
	"send_text",
	"run_command",
	"run_command_new_terminal",
	"run_background",
	"ask_ai",
];
const VALID_PRECISIONS: readonly SmartSelectionPrecision[] = ["very_low", "low", "normal", "high", "very_high"];

function isValidKind(value: string): value is SmartSelectionActionKind {
	return (VALID_KINDS as readonly string[]).includes(value);
}

function isValidPrecision(value: string): value is SmartSelectionPrecision {
	return (VALID_PRECISIONS as readonly string[]).includes(value);
}

/**
 * `run_command` (once labelled "Send Command") is the older spelling of `send_text` with
 * `sendReturn: "when_safe"`: same behaviour, and it was folded into Send Text so there is one kind
 * to understand. Saved rules and exports still carry it, so it stays a valid kind — and is read
 * through THIS function everywhere its meaning matters (the dispatcher, the settings editor, the
 * built-in-update hash and the export comparison), so all of them agree it is Send Text.
 *
 * Deliberately not applied when loading config: a saved file is only rewritten into the new
 * spelling when the user edits that rule in Settings, so a build that does not know the Press
 * Enter option, sharing the same config directory, keeps working with a rule nobody touched.
 */
export function canonicalAction(action: SmartSelectionAction): SmartSelectionAction {
	return action.kind === "run_command" ? { ...action, kind: "send_text", sendReturn: "when_safe" } : action;
}

export function actionFromWire(action: RustSmartSelectionAction): SmartSelectionAction {
	return {
		kind: isValidKind(action.kind) ? action.kind : "copy",
		title: action.title,
		parameter: action.parameter,
		isDefault: action.is_default,
		// Only a non-default, recognised value is kept: an unknown one (a newer build's) falls back
		// to the default rather than being passed through to behave unpredictably.
		...(action.send_return === "when_safe" ? { sendReturn: "when_safe" as const } : {}),
		...(action.run_in === "new_terminal" ? { runIn: "new_terminal" as const } : {}),
	};
}

export function actionToWire(action: SmartSelectionAction): RustSmartSelectionAction {
	return {
		kind: action.kind,
		title: action.title,
		parameter: action.parameter,
		is_default: action.isDefault,
		...(action.sendReturn ? { send_return: action.sendReturn } : {}),
		...(action.runIn ? { run_in: action.runIn } : {}),
	};
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
		...(rule.built_in_hash ? { builtInHash: rule.built_in_hash } : {}),
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
		...(rule.builtInHash ? { built_in_hash: rule.builtInHash } : {}),
	};
}
