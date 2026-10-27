import { describe, expect, it } from "vitest";
import {
	compileHighlightRules,
	findHighlightSpans,
	highlightRules,
	mergeSpans,
	subtractSpans,
} from "../smartHighlight";
import type { SmartSelectionRule } from "../smartSelectionTypes";

function rule(over: Partial<SmartSelectionRule> & { id: string; regex: string }): SmartSelectionRule {
	return { name: over.id, precision: "normal", enabled: true, actions: [], ...over };
}

describe("highlightRules: only a rule that asked for it highlights", () => {
	// Catches: every rule highlighting (a `\S+` catch-all would underline the whole screen).
	it("excludes rules that did not opt in, including when the flag is absent", () => {
		const optedIn = rule({ id: "a", regex: "a", highlightOnModifier: true });
		const off = rule({ id: "b", regex: "b", highlightOnModifier: false });
		const absent = rule({ id: "c", regex: "c" });
		expect(highlightRules([optedIn, off, absent]).map((r) => r.id)).toEqual(["a"]);
	});

	// Catches: a disabled rule still highlighting because its flag was left on.
	it("excludes a disabled rule even if it opted in", () => {
		expect(highlightRules([rule({ id: "a", regex: "a", enabled: false, highlightOnModifier: true })])).toEqual([]);
	});
});

describe("findHighlightSpans", () => {
	const compiled = compileHighlightRules([
		rule({ id: "sha", regex: "[0-9a-f]{7,40}", highlightOnModifier: true }),
		rule({ id: "issue", regex: "#\\d+", highlightOnModifier: true }),
		rule({ id: "off", regex: "fix", highlightOnModifier: false }),
	]);

	it("returns every match of every opted-in rule, in order, and ignores the others", () => {
		const text = "fix a1b2c3d for #42";
		expect(findHighlightSpans(text, compiled)).toEqual([
			{ start: 4, end: 11 },
			{ start: 16, end: 19 },
		]);
	});

	// Catches: two rules matching the same text producing two overlapping underlines.
	it("merges overlapping matches from different rules into one span", () => {
		const both = compileHighlightRules([
			rule({ id: "x", regex: "abc", highlightOnModifier: true }),
			rule({ id: "y", regex: "bcd", highlightOnModifier: true }),
		]);
		expect(findHighlightSpans("abcd", both)).toEqual([{ start: 0, end: 4 }]);
	});

	// Catches: an empty match (e.g. `x*`) looping forever or highlighting nothing-width spans.
	it("terminates on a pattern that can match the empty string and emits no empty span", () => {
		const empty = compileHighlightRules([rule({ id: "e", regex: "x*", highlightOnModifier: true })]);
		expect(findHighlightSpans("axxb", empty)).toEqual([{ start: 1, end: 3 }]);
	});

	it("skips an invalid pattern without breaking the rest", () => {
		const mixed = compileHighlightRules([
			rule({ id: "bad", regex: "(", highlightOnModifier: true }),
			rule({ id: "good", regex: "ok", highlightOnModifier: true }),
		]);
		expect(findHighlightSpans("ok", mixed)).toEqual([{ start: 0, end: 2 }]);
	});
});

describe("mergeSpans", () => {
	it("joins touching and overlapping spans and drops empty ones", () => {
		expect(
			mergeSpans([
				{ start: 5, end: 8 },
				{ start: 0, end: 2 },
				{ start: 2, end: 4 },
				{ start: 7, end: 9 },
				{ start: 20, end: 20 },
			]),
		).toEqual([
			{ start: 0, end: 4 },
			{ start: 5, end: 9 },
		]);
	});
});

describe("subtractSpans: a rule highlight takes precedence over a link underline", () => {
	// Catches: the link underline still drawn under text the rule highlights (two visuals on one string).
	it("removes a link span entirely when a rule covers all of it", () => {
		expect(subtractSpans([{ start: 4, end: 10 }], [{ start: 4, end: 10 }])).toEqual([]);
		expect(subtractSpans([{ start: 4, end: 10 }], [{ start: 2, end: 12 }])).toEqual([]);
	});

	// Catches: a rule match inside a long link erasing the whole link's affordance.
	it("keeps the parts of a link the rule does not cover", () => {
		expect(subtractSpans([{ start: 0, end: 20 }], [{ start: 5, end: 8 }])).toEqual([
			{ start: 0, end: 5 },
			{ start: 8, end: 20 },
		]);
	});

	it("trims a link that only partly overlaps the rule", () => {
		expect(subtractSpans([{ start: 0, end: 10 }], [{ start: 6, end: 14 }])).toEqual([{ start: 0, end: 6 }]);
		expect(subtractSpans([{ start: 6, end: 14 }], [{ start: 0, end: 10 }])).toEqual([{ start: 10, end: 14 }]);
	});

	it("leaves a link alone when nothing covers it, and handles several covers", () => {
		expect(subtractSpans([{ start: 0, end: 4 }], [{ start: 6, end: 9 }])).toEqual([{ start: 0, end: 4 }]);
		expect(subtractSpans([{ start: 0, end: 4 }], [])).toEqual([{ start: 0, end: 4 }]);
		expect(
			subtractSpans(
				[{ start: 0, end: 12 }],
				[
					{ start: 2, end: 4 },
					{ start: 8, end: 10 },
				],
			),
		).toEqual([
			{ start: 0, end: 2 },
			{ start: 4, end: 8 },
			{ start: 10, end: 12 },
		]);
	});

	it("does not mutate its inputs", () => {
		const links = [{ start: 0, end: 20 }];
		const cover = [{ start: 5, end: 8 }];
		subtractSpans(links, cover);
		expect(links).toEqual([{ start: 0, end: 20 }]);
		expect(cover).toEqual([{ start: 5, end: 8 }]);
	});
});
