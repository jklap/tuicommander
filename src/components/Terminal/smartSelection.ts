import { appLogger } from "../../stores/appLogger";
import type { SmartSelectionPrecision, SmartSelectionRule } from "./smartSelectionTypes";

/** Precision weights, verbatim from iTerm2's `SmartSelectionController.m`
 *  (`SmartSelectionVeryLowPrecision` .. `SmartSelectionVeryHighPrecision`).
 *  Deliberately exponential: a higher-precision rule almost always wins over
 *  a lower-precision one, unless the lower one matches dramatically more
 *  text (see `findSmartMatch`'s `score = weight * matchLength`). */
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

/** Wall-clock budget for the whole `findSmartMatch` call. JS can't preempt a
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
 * Find the best rule match spanning `targetOffset` in `text`. Port of
 * `iTermTextExtractor.m`'s `smartSelectionAt:withRules:` scoring: every
 * rule's every match that contains the click offset is a candidate;
 * `score = precisionWeight * matchLength`; the highest score wins. Ties keep
 * whichever candidate was found first (matches iTerm2's strict `>` compare).
 *
 * Uses each compiled regex's own global-match enumeration (`exec` in a loop)
 * to collect all non-overlapping matches, rather than iTerm2's manual
 * substring-at-each-offset scan — the two are equivalent (both find the
 * leftmost match at or after any given position) but the JS version is far
 * simpler and lets the engine reuse ordinary compiled `RegExp` objects.
 *
 * Returns `null` if no rule matches the offset, or if the time budget is
 * exceeded before every rule has been tried (the caller should fall back to
 * plain word-boundary selection in that case).
 */
export function findSmartMatch(
	text: string,
	targetOffset: number,
	rules: SmartSelectionRule[],
	budgetMs: number = DEFAULT_MATCH_BUDGET_MS,
): SmartMatch | null {
	const compiled = compileRules(rules);
	const start = now();
	let best: SmartMatch | null = null;
	let bestScore = -Infinity;

	for (const { rule, regex, weight } of compiled) {
		if (now() - start > budgetMs) {
			appLogger.warn("terminal", "Smart selection: match budget exceeded, remaining rules skipped");
			break;
		}
		regex.lastIndex = 0;
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
				const score = weight * matchLength;
				if (score > bestScore) {
					bestScore = score;
					best = { startOffset: matchStart, endOffset: matchEnd, text: match[0], groups: match.slice(1), rule };
				}
			}
			if (regex.lastIndex <= matchStart) regex.lastIndex = matchStart + 1;
			match = regex.exec(text);
		}
	}

	return best;
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

/**
 * Expand `\0`-`\9`, `\d`, `\u`, `\h`, `\n`, `\\` in an action's parameter
 * template — iTerm2's legacy (non-interpolated-string) substitution syntax.
 * An out-of-range or unmatched capture group substitutes as empty string
 * rather than throwing.
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
