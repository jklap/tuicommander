import { describe, expect, it } from "vitest";
import { DEFAULT_SMART_SELECTION_RULES } from "../smartSelectionDefaults";
import {
	isBuiltInRule,
	isCopyOnly,
	pickLinkClickRule,
	selectMenuMatches,
	typesIntoTerminal,
} from "../smartSelectionMenu";
import type { SmartSelectionAction, SmartSelectionRule } from "../smartSelectionTypes";

function builtIn(id: string): SmartSelectionRule {
	const found = DEFAULT_SMART_SELECTION_RULES.find((r) => r.id === id);
	if (!found) throw new Error(`no built-in rule ${id}`);
	return structuredClone(found);
}

function userRule(overrides: Partial<SmartSelectionRule> = {}): SmartSelectionRule {
	return {
		id: "sel-user",
		name: "Markdown files",
		regex: "\\S+\\.md",
		precision: "high",
		enabled: true,
		actions: [{ kind: "run_command", title: "Open in Imark", parameter: "imark \\0", isDefault: true }],
		...overrides,
	};
}

const m = (rule: SmartSelectionRule) => ({ rule });

describe("isBuiltInRule", () => {
	it("is true for every untouched shipped rule", () => {
		for (const rule of DEFAULT_SMART_SELECTION_RULES) expect(isBuiltInRule(structuredClone(rule))).toBe(true);
	});

	it("a built-in saved in the older run_command spelling is still the built-in (same behaviour as the shipped send_text)", () => {
		const saved = builtIn("dev-git-sha");
		saved.actions = saved.actions.map((a) =>
			a.kind === "send_text"
				? { kind: "run_command", title: a.title, parameter: a.parameter, isDefault: a.isDefault }
				: a,
		);
		expect(saved.actions[0].kind).toBe("run_command");
		expect(isBuiltInRule(saved)).toBe(true);
	});

	it("ignores the on/off switch: a disabled built-in is still the built-in", () => {
		expect(isBuiltInRule({ ...builtIn("iterm-word"), enabled: false })).toBe(true);
	});

	it("ignores the baseline hash bookkeeping", () => {
		expect(isBuiltInRule({ ...builtIn("iterm-word"), builtInHash: "0d5c4df7" })).toBe(true);
	});

	it("is false for a saved copy of a built-in that a release has since changed (it is the user's until reconciled)", () => {
		expect(isBuiltInRule({ ...builtIn("iterm-path"), regex: "an-older-shipped-pattern" })).toBe(false);
	});

	it("is false for a rule with a user id", () => {
		expect(isBuiltInRule(userRule())).toBe(false);
	});

	it("is false for a built-in the user edited (pattern, precision or actions)", () => {
		expect(isBuiltInRule({ ...builtIn("iterm-word"), regex: "\\w+" })).toBe(false);
		expect(isBuiltInRule({ ...builtIn("iterm-word"), precision: "high" })).toBe(false);
		const withOpen = builtIn("iterm-word");
		withOpen.actions.push({ kind: "open_file", title: "Open", parameter: "\\0", isDefault: true });
		expect(isBuiltInRule(withOpen)).toBe(false);
	});

	it("does not depend on key order (a config round-trip may reorder keys)", () => {
		const shipped = builtIn("iterm-word");
		const reordered = {
			actions: shipped.actions.map((a) => ({
				isDefault: a.isDefault,
				parameter: a.parameter,
				title: a.title,
				kind: a.kind,
			})),
			enabled: shipped.enabled,
			precision: shipped.precision,
			regex: shipped.regex,
			name: shipped.name,
			id: shipped.id,
		} as SmartSelectionRule;
		expect(isBuiltInRule(reordered)).toBe(true);
	});
});

describe("isCopyOnly", () => {
	it("is true only when every action is Copy", () => {
		expect(isCopyOnly(builtIn("iterm-word"))).toBe(true);
		expect(isCopyOnly(builtIn("dev-git-sha"))).toBe(false); // send_text + copy
		expect(isCopyOnly(builtIn("iterm-http-url"))).toBe(false); // open_url + copy
	});

	it("is false for a rule with no actions", () => {
		expect(isCopyOnly(userRule({ actions: [] }))).toBe(false);
	});
});

describe("selectMenuMatches", () => {
	const word = m(builtIn("iterm-word"));
	const include = m(builtIn("iterm-java-python-include"));
	const sha = m(builtIn("dev-git-sha"));
	const mine = m(userRule());

	it("drops built-in Copy-only groups when a user rule matched", () => {
		expect(selectMenuMatches([mine, word, include], false)).toEqual([mine]);
	});

	it("drops built-in Copy-only groups when a built-in rule with a real action matched", () => {
		expect(selectMenuMatches([sha, word], false)).toEqual([sha]);
	});

	it("drops built-in Copy-only groups when the click is on a link, even if no rule is more specific", () => {
		expect(selectMenuMatches([word, include], true)).toEqual([]);
	});

	it("keeps only the best Copy-only group when nothing more specific exists", () => {
		expect(selectMenuMatches([include, word], false)).toEqual([include]);
	});

	it("never hides a user's own Copy-only rule", () => {
		const copyOnlyUser = m(
			userRule({ actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }] }),
		);
		expect(selectMenuMatches([copyOnlyUser, word], false)).toEqual([copyOnlyUser]);
	});

	it("treats an edited built-in as the user's own, so it stays", () => {
		const edited = m({ ...builtIn("iterm-word"), regex: "\\w+" });
		expect(selectMenuMatches([edited, include], false)).toEqual([edited]);
	});

	it("keeps order of what it keeps and handles an empty list", () => {
		expect(selectMenuMatches([mine, sha, word], false)).toEqual([mine, sha]);
		expect(selectMenuMatches([], false)).toEqual([]);
	});
});

describe("typesIntoTerminal", () => {
	it("is true for Send Text and the older Send Command, false for everything else", () => {
		const a = (kind: string) => ({ kind, title: "t", parameter: "p", isDefault: true }) as SmartSelectionAction;
		expect(typesIntoTerminal(a("send_text"))).toBe(true);
		expect(typesIntoTerminal(a("run_command"))).toBe(true);
		for (const kind of ["copy", "open_url", "open_file", "run_background", "run_command_new_terminal", "ask_ai"]) {
			expect(typesIntoTerminal(a(kind)), kind).toBe(false);
		}
	});
});

describe("pickLinkClickRule", () => {
	it("returns the best user rule's default action", () => {
		const mine = m(userRule());
		const picked = pickLinkClickRule([mine, m(builtIn("iterm-path"))]);
		expect(picked?.match).toBe(mine);
		expect(picked?.action.title).toBe("Open in Imark");
	});

	it("skips untouched built-ins so the link's own open handles them", () => {
		expect(pickLinkClickRule([m(builtIn("iterm-http-url")), m(builtIn("iterm-path"))])).toBeNull();
	});

	it("skips a user rule with no default action and takes the next that has one", () => {
		const noDefault = m(
			userRule({ id: "a", actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }] }),
		);
		const withDefault = m(userRule({ id: "b" }));
		expect(pickLinkClickRule([noDefault, withDefault])?.match).toBe(withDefault);
	});

	it("honours an edited built-in that gained a default action", () => {
		const edited = builtIn("iterm-path");
		edited.actions = [{ kind: "run_command", title: "Edit", parameter: "vim \\0", isDefault: true }];
		expect(pickLinkClickRule([m(edited)])?.action.title).toBe("Edit");
	});

	describe("with an agent running in the terminal", () => {
		const typing = (kind: "send_text" | "run_command", extra = {}) =>
			m(
				userRule({
					id: `typing-${kind}`,
					actions: [{ kind, title: "Type", parameter: "x \\0", isDefault: true, ...extra }],
				}),
			);
		const other = (kind: "run_background" | "copy" | "open_file") =>
			m(userRule({ id: `other-${kind}`, actions: [{ kind, title: "Do", parameter: "\\0", isDefault: true }] }));

		it("skips a rule whose default TYPES into the terminal (Send Text and the older run_command)", () => {
			expect(pickLinkClickRule([typing("send_text", { sendReturn: "when_safe" })], { agentRunning: true })).toBeNull();
			expect(pickLinkClickRule([typing("send_text")], { agentRunning: true })).toBeNull();
			expect(pickLinkClickRule([typing("run_command")], { agentRunning: true })).toBeNull();
		});

		it("still runs a rule that does not type into the terminal: Run Command, Copy, Open File", () => {
			for (const kind of ["run_background", "copy", "open_file"] as const) {
				expect(pickLinkClickRule([other(kind)], { agentRunning: true })?.action.kind, kind).toBe(kind);
			}
		});

		it("falls through to the next matching rule instead of giving up", () => {
			const later = other("run_background");
			expect(pickLinkClickRule([typing("send_text"), later], { agentRunning: true })?.match).toBe(later);
		});

		it("types as before when no agent is running, and when the option is omitted", () => {
			const rule = typing("send_text", { sendReturn: "when_safe" });
			expect(pickLinkClickRule([rule], { agentRunning: false })?.match).toBe(rule);
			expect(pickLinkClickRule([rule])?.match).toBe(rule);
		});
	});

	it("returns null when nothing matched", () => {
		expect(pickLinkClickRule([])).toBeNull();
	});
});
