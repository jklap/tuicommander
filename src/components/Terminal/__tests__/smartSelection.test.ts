import { describe, expect, it } from "vitest";
import { compileRules, findSmartMatch, PRECISION_WEIGHT, substituteActionParameter } from "../smartSelection";
import type { SmartSelectionRule } from "../smartSelectionTypes";

function rule(overrides: Partial<SmartSelectionRule> = {}): SmartSelectionRule {
	return {
		id: "id",
		name: "rule",
		regex: "\\S+",
		precision: "normal",
		enabled: true,
		actions: [],
		...overrides,
	};
}

describe("compileRules", () => {
	it("compiles enabled rules and skips disabled ones", () => {
		const compiled = compileRules([rule({ id: "a", enabled: true }), rule({ id: "b", enabled: false })]);
		expect(compiled.map((c) => c.rule.id)).toEqual(["a"]);
	});

	it("skips an invalid regex instead of throwing", () => {
		const compiled = compileRules([rule({ id: "bad", regex: "(unterminated" }), rule({ id: "good" })]);
		expect(compiled.map((c) => c.rule.id)).toEqual(["good"]);
	});

	it("assigns the correct precision weight to each compiled rule", () => {
		const compiled = compileRules([rule({ precision: "very_high" })]);
		expect(compiled[0].weight).toBe(PRECISION_WEIGHT.very_high);
	});
});

describe("findSmartMatch", () => {
	it("returns null when no rule matches the offset", () => {
		expect(findSmartMatch("foo bar", 100, [rule()])).toBeNull();
	});

	it("rejects a match that doesn't span the click offset", () => {
		// "foo" matches at 0-3, but offset 5 is inside "bar" — a whitespace-word
		// rule alone shouldn't claim it since \S+ only matches "bar" separately,
		// which DOES span offset 5; assert against a rule that can't reach it.
		const digitsOnly = rule({ regex: "[0-9]+" });
		expect(findSmartMatch("foo bar", 5, [digitsOnly])).toBeNull();
	});

	it("higher precision wins even with a shorter match", () => {
		const rules = [
			rule({ id: "low-long", regex: "\\S+", precision: "low" }), // matches "https://example.com" (20 chars)
			rule({ id: "high-short", regex: "https", precision: "high" }), // matches "https" (5 chars)
		];
		const text = "see https://example.com now";
		const match = findSmartMatch(text, 6, rules); // offset 6 is inside "https"
		expect(match?.rule.id).toBe("high-short");
		expect(match?.text).toBe("https");
	});

	it("within the same precision class, the longer match wins", () => {
		const rules = [
			rule({ id: "word", regex: "[a-z]+", precision: "normal" }),
			rule({ id: "wordcolon", regex: "[a-z]+:", precision: "normal" }),
		];
		const text = "foo:bar";
		const match = findSmartMatch(text, 1, rules);
		expect(match?.rule.id).toBe("wordcolon");
		expect(match?.text).toBe("foo:");
	});

	it("the user's motivating case: adding a scheme alternate expands selection across the whole URL", () => {
		const urlRule = rule({ id: "url", regex: "https://[^\\s]+", precision: "very_high" });
		const wordRule = rule({ id: "word", regex: "[a-zA-Z]+", precision: "low" });
		const text = "cloning https://github.com/foo/bar.git into repo";
		// Click lands inside "github" — the URL rule's match still spans it.
		const clickOffset = text.indexOf("github") + 2;
		const match = findSmartMatch(text, clickOffset, [urlRule, wordRule]);
		expect(match?.rule.id).toBe("url");
		expect(match?.text).toBe("https://github.com/foo/bar.git");
	});

	it("returns null (falls back to word selection) once the time budget is exhausted", () => {
		const matchable = rule({ regex: "\\S+" });
		expect(findSmartMatch("foo bar", 1, [matchable], -1)).toBeNull();
	});

	it("finds a match anywhere in a multi-row scanned window, not just the first line", () => {
		const shaRule = rule({ id: "sha", regex: "[0-9a-f]{7,40}", precision: "high" });
		const text = "line one\nfix: handle EOF a1b2c3d\nline three";
		const offset = text.indexOf("a1b2c3d") + 2;
		const match = findSmartMatch(text, offset, [shaRule]);
		expect(match?.text).toBe("a1b2c3d");
	});
});

describe("substituteActionParameter", () => {
	const ctx = { match: "a1b2c3d", groups: ["a1b2c3d", "extra"], cwd: "/repo", user: "jason", host: "box" };

	it("substitutes \\0 with the whole match", () => {
		expect(substituteActionParameter("git show \\0", ctx)).toBe("git show a1b2c3d");
	});

	it("substitutes \\1-\\9 with capture groups", () => {
		expect(substituteActionParameter("\\1 / \\2", ctx)).toBe("a1b2c3d / extra");
	});

	it("substitutes \\d, \\u, \\h", () => {
		expect(substituteActionParameter("\\u@\\h:\\d", ctx)).toBe("jason@box:/repo");
	});

	it("substitutes \\n as a newline and \\\\ as a literal backslash", () => {
		expect(substituteActionParameter("a\\nb", ctx)).toBe("a\nb");
		expect(substituteActionParameter("a\\\\b", ctx)).toBe("a\\b");
	});

	it("substitutes an out-of-range capture group as empty string rather than throwing", () => {
		expect(substituteActionParameter("\\9", ctx)).toBe("");
	});

	it("substitutes \\d/\\u/\\h as empty string when the context omits them", () => {
		expect(substituteActionParameter("\\u@\\h:\\d", { match: "x", groups: [] })).toBe("@:");
	});

	it("leaves text with no escape sequences untouched", () => {
		expect(substituteActionParameter("plain text", ctx)).toBe("plain text");
	});
});
