import { type CompiledRule, compileRules, DEFAULT_MATCH_BUDGET_MS } from "./smartSelection";
import type { SmartSelectionRule } from "./smartSelectionTypes";

/** A half-open range: UTF-16 offsets into a row's text, or terminal columns once converted. */
export interface Span {
	start: number;
	end: number;
}

/**
 * The rules whose matches light up while the link modifier is held. Opt-in per rule: a
 * catch-all such as `\S+` would otherwise underline the whole screen, so a rule that is
 * disabled or never asked for it contributes nothing.
 */
export function highlightRules(rules: readonly SmartSelectionRule[]): SmartSelectionRule[] {
	return rules.filter((rule) => rule.enabled && rule.highlightOnModifier === true);
}

/** Merge touching and overlapping spans into a sorted, disjoint list. */
export function mergeSpans(spans: readonly Span[]): Span[] {
	const sorted = spans.filter((s) => s.end > s.start).sort((a, b) => a.start - b.start || a.end - b.end);
	const merged: Span[] = [];
	for (const span of sorted) {
		const last = merged.at(-1);
		if (last && span.start <= last.end) last.end = Math.max(last.end, span.end);
		else merged.push({ ...span });
	}
	return merged;
}

/**
 * Every non-empty match of the compiled rules in `text`, merged into disjoint spans. Stops
 * once `budgetMs` is spent: JS cannot preempt a single slow regex, but this bounds a long
 * list of cheap ones, and the caller caches per row so a slow row is paid for once.
 */
export function findHighlightSpans(
	text: string,
	compiled: readonly CompiledRule[],
	budgetMs: number = DEFAULT_MATCH_BUDGET_MS,
): Span[] {
	const started = performance.now();
	const found: Span[] = [];
	for (const { regex } of compiled) {
		if (performance.now() - started > budgetMs) break;
		regex.lastIndex = 0;
		let match = regex.exec(text);
		while (match !== null) {
			if (match[0].length === 0) regex.lastIndex = match.index + 1;
			else found.push({ start: match.index, end: match.index + match[0].length });
			match = regex.exec(text);
		}
	}
	return mergeSpans(found);
}

/**
 * `spans` with every part covered by `cover` removed. A rule highlight takes precedence over
 * the built-in link underline, so the link keeps only the stretches the rule does not cover:
 * where both match the same text the rule's style is the one drawn, and a link that merely
 * overlaps a rule match still shows its own underline on the rest.
 */
export function subtractSpans(spans: readonly Span[], cover: readonly Span[]): Span[] {
	if (cover.length === 0) return spans.map((s) => ({ ...s }));
	const disjoint = mergeSpans(cover);
	const out: Span[] = [];
	for (const span of spans) {
		let start = span.start;
		for (const c of disjoint) {
			if (c.end <= start) continue;
			if (c.start >= span.end) break;
			if (c.start > start) out.push({ start, end: c.start });
			start = Math.max(start, c.end);
			if (start >= span.end) break;
		}
		if (start < span.end) out.push({ start, end: span.end });
	}
	return out;
}

/** Compile just the opted-in rules, so the per-row scan never touches the rest. */
export function compileHighlightRules(rules: readonly SmartSelectionRule[]): CompiledRule[] {
	return compileRules(highlightRules(rules));
}
