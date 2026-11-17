import { DEFAULT_SMART_SELECTION_RULES } from "./smartSelectionDefaults";
import { ruleContentKey } from "./smartSelectionReconcile";
import { canonicalAction, type SmartSelectionAction, type SmartSelectionRule } from "./smartSelectionTypes";

/** Shipped definition of each built-in rule, keyed by id. */
const SHIPPED_BY_ID = new Map(DEFAULT_SMART_SELECTION_RULES.map((rule) => [rule.id, rule]));

/**
 * True for a rule that is exactly what TUICommander ships (its on/off switch aside — see
 * `ruleContentKey`). A built-in the user has edited (different pattern, precision, actions, ...)
 * counts as the user's own rule: editing it is a statement that they care about it, so it must not
 * be hidden from the menu or skipped on click like an untouched default.
 *
 * Editing any rule materialises the whole default set into the saved config (see
 * `resolveSmartSelectionRules`), so an id match alone would misclassify every one of them as
 * "built-in" forever. A stale saved copy of a rule a release has since changed does not match the
 * shipped content either, so it counts as the user's own until `reconcileBuiltInRules` brings it up
 * to date at startup.
 */
export function isBuiltInRule(rule: SmartSelectionRule): boolean {
	const shipped = SHIPPED_BY_ID.get(rule.id);
	return shipped !== undefined && ruleContentKey(shipped) === ruleContentKey(rule);
}

/** A rule whose every action is "Copy" — it only ever offers to copy its own match. */
export function isCopyOnly(rule: SmartSelectionRule): boolean {
	return rule.actions.length > 0 && rule.actions.every((action) => action.kind === "copy");
}

/**
 * Which matching rules the right-click menu should list.
 *
 * Built-in, Copy-only rules (`\S+` word, Java/Python include path, IPv4, ...)
 * match almost any text and each offers the same "Copy" over a slightly
 * different extent, so listing all of them buries the actions that matter.
 * They are dropped as soon as anything more specific is on offer — another
 * matching rule (any rule that is not a built-in Copy-only one) or a detected
 * link. When NOTHING more specific exists they are the only thing the menu
 * has, so the best-scoring one is kept (matches arrive best-first) rather than
 * showing an empty menu or the whole redundant pile.
 */
export function selectMenuMatches<T extends { rule: SmartSelectionRule }>(matches: T[], hasLink: boolean): T[] {
	const redundant = (match: T) => isBuiltInRule(match.rule) && isCopyOnly(match.rule);
	const specific = matches.filter((match) => !redundant(match));
	if (specific.length > 0 || hasLink) return specific;
	return matches.slice(0, 1);
}

/**
 * The rule a plain left-click on a detected link should run instead of the
 * built-in link opening: the best-scoring match (matches arrive best-first)
 * that the user wrote or edited and that has a default action.
 *
 * Untouched built-ins are deliberately skipped. Their defaults (Open URL, Open
 * File) do what link opening already does — and link opening is better at it
 * (it verifies a path exists, honours `:line:col`, OSC 8 targets and soft-wrap)
 * — so a click falls back to it whenever no rule of the user's has claimed the
 * span.
 *
 * With a program other than the shell in the foreground (`agentRunning` — the caller's name for
 * "anything but a bare shell prompt": an agent, vim, a REPL, ssh), a rule whose default action TYPES
 * into the terminal (Send Text, or the older `run_command`) is skipped too: that would submit the
 * match to that program as input on a single click.
 */
/** True for an action that TYPES text into the terminal (Send Text, or the older Send Command). */
export function typesIntoTerminal(action: SmartSelectionAction): boolean {
	return canonicalAction(action).kind === "send_text";
}

export function pickLinkClickRule<T extends { rule: SmartSelectionRule }>(
	matches: T[],
	options: { agentRunning?: boolean } = {},
): { match: T; action: SmartSelectionAction } | null {
	for (const match of matches) {
		if (isBuiltInRule(match.rule)) continue;
		const action = match.rule.actions.find((a) => a.isDefault);
		if (!action) continue;
		// A single plain click must never type terminal-derived text into a running agent: with an
		// agent in the foreground Send Text would press Enter on whatever the match is, as a prompt.
		// Such a rule is skipped here (its menu entry still works — that is a deliberate choice), and
		// the click falls through to the next rule, or to the normal link open.
		if (options.agentRunning && typesIntoTerminal(action)) continue;
		return { match, action };
	}
	return null;
}
