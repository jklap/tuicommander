import { appLogger } from "../../stores/appLogger";
import type { SmartSelectionPrecision, SmartSelectionRule } from "./smartSelectionTypes";

/** Precision weights, verbatim from iTerm2's `SmartSelectionController.m`
 *  (`SmartSelectionVeryLowPrecision` .. `SmartSelectionVeryHighPrecision`).
 *  Deliberately exponential: a higher-precision rule almost always wins over
 *  a lower-precision one, unless the lower one matches dramatically more
 *  text (see `findSmartMatches`'s `score = weight * matchLength`). */
export const PRECISION_WEIGHT: Record<SmartSelectionPrecision, number> = {
	very_low: 0.00001,
	low: 0.001,
	normal: 1.0,
	high: 1000.0,
	very_high: 1000000.0,
};

/** Rows above/below the click to include in the scanned text window —
 *  iTerm2's default `smartSelectionRadius`. Bounds the regex scan to a few
 *  thousand characters, never the whole scrollback. */
export const SMART_SELECTION_RADIUS = 2;

/** Wall-clock budget for the whole `findSmartMatches` call. JS can't preempt a
 *  single catastrophic regex mid-exec, so this only protects against many
 *  cheap-but-numerous rules; the real defense against a single ReDoS pattern
 *  is validating rules at save time (see the rule editor). */
export const DEFAULT_MATCH_BUDGET_MS = 8;

export interface CompiledRule {
	rule: SmartSelectionRule;
	regex: RegExp;
	weight: number;
}

function now(): number {
	return typeof performance !== "undefined" ? performance.now() : Date.now();
}

/**
 * Compile every enabled rule's regex once. Invalid patterns are logged and
 * skipped rather than thrown — one bad user-authored rule must not break
 * every other rule's matching (mirrors iTerm2's `RegexAtomIterator.compileRegexes`).
 */
export function compileRules(rules: SmartSelectionRule[]): CompiledRule[] {
	const compiled: CompiledRule[] = [];
	for (const rule of rules) {
		if (!rule.enabled) continue;
		try {
			const regex = new RegExp(rule.regex, "g");
			compiled.push({ rule, regex, weight: PRECISION_WEIGHT[rule.precision] });
		} catch (error) {
			appLogger.warn("terminal", `Smart selection: invalid regex in rule "${rule.name}"`, { error });
		}
	}
	return compiled;
}

export interface SmartMatch {
	/** Offset into the scanned text where the match starts (inclusive). */
	startOffset: number;
	/** Offset into the scanned text where the match ends (exclusive). */
	endOffset: number;
	/** The full matched text (substitution's `\0`). */
	text: string;
	/** Capture groups, 0-indexed (`groups[0]` is `\1`, etc.) — undefined entries for unmatched optional groups. */
	groups: (string | undefined)[];
	rule: SmartSelectionRule;
}

/**
 * Every rule that matches across `targetOffset`, best first — one entry per rule. Port of
 * `iTermTextExtractor.m`'s `smartSelectionAt:withRules:` scoring: a rule's match that contains the
 * click offset is a candidate, `score = precisionWeight * matchLength`, the highest score wins, and
 * equal scores keep rule order (matches iTerm2's strict `>` compare).
 *
 * Uses each compiled regex's own global-match enumeration (`exec` in a loop) rather than iTerm2's
 * manual substring-at-each-offset scan — the two are equivalent (both find the leftmost match at or
 * after any given position) but the JS version is far simpler and lets the engine reuse ordinary
 * compiled `RegExp` objects.
 *
 * Gestures (double/quad-click, Alt+double-click) use only the first entry: precision decides what
 * they select or run. The right-click menu lists all of them, each rule's actions under its own
 * header, because a menu is a choice and no matching rule should be hidden.
 *
 * Returns `[]` if no rule matches the offset. If the time budget runs out, the rules not yet tried
 * are skipped (so an empty result may mean "out of time"; the caller falls back to plain
 * word-boundary selection either way).
 */
export function findSmartMatches(
	text: string,
	targetOffset: number,
	rules: SmartSelectionRule[],
	budgetMs: number = DEFAULT_MATCH_BUDGET_MS,
): (SmartMatch & { score: number })[] {
	const compiled = compileRules(rules);
	const start = now();
	const found: (SmartMatch & { score: number })[] = [];

	for (const { rule, regex, weight } of compiled) {
		if (now() - start > budgetMs) {
			appLogger.warn("terminal", "Smart selection: match budget exceeded, remaining rules skipped");
			break;
		}
		regex.lastIndex = 0;
		let ruleBest: (SmartMatch & { score: number }) | null = null;
		let match: RegExpExecArray | null = regex.exec(text);
		while (match !== null) {
			const matchStart = match.index;
			const matchLength = match[0].length;
			const matchEnd = matchStart + matchLength;
			if (matchLength === 0) {
				// A zero-length match can't advance lastIndex on its own — bump it
				// by hand or this loop never terminates.
				regex.lastIndex = matchStart + 1;
			} else if (matchStart <= targetOffset && matchEnd > targetOffset) {
				// A global regex's matches never overlap, so at most one of them can
				// span the offset: it is this rule's match, no comparison needed.
				ruleBest = {
					startOffset: matchStart,
					endOffset: matchEnd,
					text: match[0],
					groups: match.slice(1),
					rule,
					score: weight * matchLength,
				};
				break;
			}
			if (regex.lastIndex <= matchStart) regex.lastIndex = matchStart + 1;
			match = regex.exec(text);
		}
		if (ruleBest) found.push(ruleBest);
	}

	// Array.prototype.sort is stable, so equal scores keep rule order.
	return found.sort((a, b) => b.score - a.score);
}

export interface SubstitutionContext {
	/** `\0` — the whole match. */
	match: string;
	/** `\1`-`\9` — capture groups, 0-indexed. */
	groups: (string | undefined)[];
	/** `\d` — current working directory. */
	cwd?: string;
	/** `\u` — current username. */
	user?: string;
	/** `\h` — current hostname. */
	host?: string;
}

/** POSIX single-quote `value` so a POSIX shell reads it as exactly one literal word, whatever it
 *  contains: `$(...)`, backticks, `;`, spaces and newlines all lose their meaning. A single quote
 *  cannot appear inside single quotes, so each one closes the string, adds an escaped quote, and
 *  reopens it (`'\''`). NUL bytes cannot be passed to a program at all and are dropped.
 *
 *  Sound ONLY where the shell parses single quotes the POSIX way: sh, dash, bash, zsh, ksh. Not fish
 *  (`\'` inside single quotes is an escape there, so a value ending in a backslash breaks out), and
 *  unverified in tcsh, csh, nu, elvish, xonsh and pwsh. It is used for exactly one thing — typing a
 *  Run Command line into a new terminal — and only after the backend has confirmed the terminal's
 *  shell is one of the sound ones (`run_command_shell_support`). */
export function shellQuote(value: string): string {
	return `'${value.replaceAll("\0", "").replaceAll("'", "'\\''")}'`;
}

/**
 * Expand `\0`-`\9`, `\d`, `\u`, `\h`, `\n`, `\\` in an action's parameter
 * template — iTerm2's legacy (non-interpolated-string) substitution syntax.
 * An out-of-range or unmatched capture group substitutes as empty string
 * rather than throwing.
 *
 * Plain text substitution, for actions where the result is DATA or is typed for a person to read
 * (copy, open URL/file, send text, the older new-terminal kind). Run Command does not use this: it
 * uses `buildRunCommandScript`, which never puts matched text into the command line at all.
 */
export function substituteActionParameter(template: string, ctx: SubstitutionContext): string {
	return template.replace(/\\([0-9dnuh\\])/g, (_whole, code: string) => {
		switch (code) {
			case "0":
				return ctx.match;
			case "\\":
				return "\\";
			case "n":
				return "\n";
			case "d":
				return ctx.cwd ?? "";
			case "u":
				return ctx.user ?? "";
			case "h":
				return ctx.host ?? "";
			default:
				return ctx.groups[Number(code) - 1] ?? "";
		}
	});
}

/** The environment variable that carries each placeholder's value to the command. The backend
 *  accepts exactly these names and no others (`is_allowed_env_name`). */
const PLACEHOLDER_VARIABLES: Readonly<Record<string, string>> = {
	"0": "TUIC_M0",
	"1": "TUIC_M1",
	"2": "TUIC_M2",
	"3": "TUIC_M3",
	"4": "TUIC_M4",
	"5": "TUIC_M5",
	"6": "TUIC_M6",
	"7": "TUIC_M7",
	"8": "TUIC_M8",
	"9": "TUIC_M9",
	d: "TUIC_D",
	u: "TUIC_U",
	h: "TUIC_H",
};

export interface RunCommandScript {
	/** POSIX `sh` syntax. Matched text appears in it only as `${TUIC_…}` references. */
	script: string;
	/** What each referenced variable holds. Only placeholders the template uses are present. */
	values: Record<string, string>;
}

/**
 * Turn a Run Command template into a POSIX `sh` script plus the values it refers to, so that matched
 * terminal text reaches the command as DATA and can never become shell syntax.
 *
 * Each placeholder (`\0`-`\9`, `\d`, `\u`, `\h`) becomes a variable reference, and its value goes in
 * `values` to be set in the command's environment. A shell expands a variable and never parses the
 * result again, so what the template author wrote AROUND the placeholder cannot matter. The form of
 * the reference does follow the quoting around it, so the command keeps meaning what they intended:
 *
 *  - bare word      `imark \0`    -> `imark "${TUIC_M0}"`      (quoted: never split or globbed)
 *  - inside `"…"`   `"<\0>"`      -> `"<${TUIC_M0}>"`          (already quoted by its context)
 *  - inside `'…'`   `'<\0>'`      -> `'<'"${TUIC_M0}"'>'`      (single quotes cannot expand, so the
 *                                                              quote is closed around it)
 *
 * The template's own text is the user's and is otherwise left exactly as written; `\n` and `\\`
 * keep their meaning (a newline, a backslash). Quote tracking handles `'…'`, `"…"` and backslash
 * escapes, and `#` comments (a quote character inside a comment, like the apostrophe in
 * `# don't`, is not a quote, so it must not flip the state — if it did, every later placeholder
 * would be written for the wrong context). It deliberately does not try to model heredocs or nested
 * `$( … )` quoting, and where it is wrong the result is a command that misbehaves (a stray quote,
 * an unsplit or split word) — never a value executed, because no value is ever in the text the
 * shell parses.
 *
 * Things the template author can still do to themselves, since the shell is then told to read the
 * value as code on purpose: put a placeholder inside `$(( … ))`, `[[ … -eq … ]]`, `let` or
 * `declare -i` (bash and ksh evaluate a variable's contents as arithmetic there, including
 * `x[$(cmd)]`), or pass it to `eval` / `sh -c`.
 */
export function buildRunCommandScript(template: string, ctx: SubstitutionContext): RunCommandScript {
	const values: Record<string, string> = {};
	let script = "";
	let quoting: "none" | "single" | "double" = "none";
	// True right after a backslash that the shell will take as an escape for the NEXT character.
	let escapes = false;
	// True from a `#` that starts a comment up to the end of its line. Inside it nothing is a quote.
	let inComment = false;
	// `#` only starts a comment at the start of a word: `$#`, `${#x}` and `a#b` are not comments.
	const atWordStart = (): boolean => script === "" || /[\s;&|(]$/.test(script);

	const placeholderValue = (code: string): string => {
		switch (code) {
			case "0":
				return ctx.match;
			case "d":
				return ctx.cwd ?? "";
			case "u":
				return ctx.user ?? "";
			case "h":
				return ctx.host ?? "";
			default:
				return ctx.groups[Number(code) - 1] ?? "";
		}
	};
	const reference = (variable: string): string => {
		if (quoting === "double") return `\${${variable}}`;
		if (quoting === "single") return `'"\${${variable}}"'`;
		return `"\${${variable}}"`;
	};

	for (let i = 0; i < template.length; i++) {
		const ch = template[i];
		if (escapes) {
			// The character a preceding backslash escapes: copied as is, and it never changes the
			// quoting state (an escaped quote is a literal quote).
			escapes = false;
			script += ch;
			continue;
		}
		if (ch === "\\") {
			const next = template[i + 1];
			if (next !== undefined && /[0-9dnuh\\]/.test(next)) {
				i++;
				if (next === "n") {
					script += "\n";
					// A real newline ends a comment.
					inComment = false;
				} else if (next === "\\") {
					script += "\\";
					if (quoting !== "single" && !inComment) escapes = true;
				} else {
					const variable = PLACEHOLDER_VARIABLES[next];
					values[variable] = placeholderValue(next).replaceAll("\0", "");
					script += reference(variable);
				}
				continue;
			}
			// Any other backslash is the shell's own escape: copy it and the character it escapes.
			script += ch;
			// In a comment a backslash escapes nothing.
			if (quoting !== "single" && !inComment && next !== undefined) {
				script += next;
				i++;
			}
			continue;
		}
		if (inComment) {
			if (ch === "\n") inComment = false;
			script += ch;
			continue;
		}
		if (ch === "#" && quoting === "none" && atWordStart()) {
			inComment = true;
			script += ch;
			continue;
		}
		if (ch === "'" && quoting !== "double") quoting = quoting === "single" ? "none" : "single";
		else if (ch === '"' && quoting !== "single") quoting = quoting === "double" ? "none" : "double";
		script += ch;
	}
	return { script, values };
}

/** Longest typed line the new-terminal path will send. Until the shell's line editor exists the
 *  tty is in canonical mode and silently drops everything past `MAX_CANON` (1024 bytes on macOS),
 *  newline included, which leaves half a command at the prompt. The backend's own guard for long
 *  writes (`CANONICAL_LINE_GUARD_BYTES`, 1000) covers only the HTTP write routes, not this path, so
 *  the line is kept under the same figure instead. */
export const MAX_TYPED_RUN_LINE_BYTES = 1000;

/** True when any value holds a C0 control byte, DEL or a C1 control. A typed line goes through the
 *  shell's line editor BEFORE the shell parses it, and the editor acts on control bytes whatever
 *  quotes surround them: `^C` or `^U` discards the line typed so far and the rest of the value is
 *  then read as a fresh command. Matched text can't carry them (terminal cells hold printable
 *  characters), but the working directory, `\d`, comes from OSC 7 and is percent-decoded raw. */
export function hasControlBytes(values: Record<string, string>): boolean {
	return Object.values(values).some((value) => /[\u0000-\u001f\u007f-\u009f]/.test(value));
}

/**
 * The single line to TYPE into a new terminal to run `run` there: the values as one-shot variable
 * assignments, then the script handed to `/bin/sh`. Both the values and the script are POSIX
 * single-quoted, so this is only for a shell where that is sound — see `shellQuote`.
 *
 * Running the script under `sh -c` (rather than typing it bare) is what lets the values stay data
 * in the terminal too: they are variables of the `sh` child, never part of the line the user's
 * shell parses beyond the quoted assignments.
 */
export function typedRunLine(run: RunCommandScript): string {
	const assignments = Object.entries(run.values).map(([name, value]) => `${name}=${shellQuote(value)}`);
	return [...assignments, "/bin/sh", "-c", shellQuote(run.script)].join(" ");
}
