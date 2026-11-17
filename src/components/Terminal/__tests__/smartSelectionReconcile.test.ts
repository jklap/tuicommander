import { describe, expect, it } from "vitest";
import { DEFAULT_SMART_SELECTION_RULES } from "../smartSelectionDefaults";
import { reconcileBuiltInRules, ruleContentHash, ruleContentKey } from "../smartSelectionReconcile";
import type { SmartSelectionRule } from "../smartSelectionTypes";

function rule(overrides: Partial<SmartSelectionRule> = {}): SmartSelectionRule {
	return {
		id: "dev-thing",
		name: "Thing",
		regex: "thing",
		precision: "normal",
		enabled: true,
		actions: [{ kind: "copy", title: "Copy", parameter: "\\0", isDefault: false }],
		...overrides,
	};
}

/** What a release ships, and the next release's changed version of the same rule. */
const v1 = rule();
const v2 = rule({ regex: "thing+", name: "Thing (better)" });

/** A saved copy as reconcile leaves it: stamped with the shipped version it was reconciled against. */
const saved = (shipped: SmartSelectionRule, overrides: Partial<SmartSelectionRule> = {}): SmartSelectionRule => ({
	...shipped,
	builtInHash: ruleContentHash(shipped),
	...overrides,
});

describe("ruleContentHash", () => {
	it("is pinned: changing the algorithm would make every saved rule look edited", () => {
		expect(ruleContentHash(v1)).toBe("0d5c4df7");
	});

	it("ignores `enabled` and `builtInHash`, but sees every editable field", () => {
		const base = ruleContentHash(v1);
		expect(ruleContentHash({ ...v1, enabled: false })).toBe(base);
		expect(ruleContentHash({ ...v1, builtInHash: "zzzz" })).toBe(base);
		expect(ruleContentHash({ ...v1, name: "x" })).not.toBe(base);
		expect(ruleContentHash({ ...v1, regex: "x" })).not.toBe(base);
		expect(ruleContentHash({ ...v1, precision: "high" })).not.toBe(base);
		expect(ruleContentHash({ ...v1, actions: [] })).not.toBe(base);
		const act = v1.actions[0];
		for (const patch of [{ kind: "open_url" as const }, { title: "T" }, { parameter: "x" }, { isDefault: true }]) {
			expect(ruleContentHash({ ...v1, actions: [{ ...act, ...patch }] })).not.toBe(base);
		}
	});

	it("the per-action options change the hash only when SET, never as a placeholder (the persisted contract)", () => {
		const base = ruleContentHash(v1);
		const act = v1.actions[0];
		// Absent, explicitly undefined: exactly the pre-option hash, so no saved rule's baseline moves.
		expect(ruleContentHash({ ...v1, actions: [{ ...act }] })).toBe(base);
		expect(ruleContentHash({ ...v1, actions: [{ ...act, sendReturn: undefined, runIn: undefined }] })).toBe(base);
		// Set: a different hash, and different values differ from each other.
		const withReturn = ruleContentHash({ ...v1, actions: [{ ...act, sendReturn: "when_safe" }] });
		const withRunIn = ruleContentHash({ ...v1, actions: [{ ...act, runIn: "new_terminal" }] });
		expect(withReturn).not.toBe(base);
		expect(withRunIn).not.toBe(base);
		expect(withReturn).not.toBe(withRunIn);
	});

	it("reads the older run_command as send_text + when safe: the same behaviour hashes the same", () => {
		const act = v1.actions[0];
		const older = { ...v1, actions: [{ ...act, kind: "run_command" as const }] };
		const current = { ...v1, actions: [{ ...act, kind: "send_text" as const, sendReturn: "when_safe" as const }] };
		expect(ruleContentHash(older)).toBe(ruleContentHash(current));
		expect(ruleContentKey(older)).toBe(ruleContentKey(current));
		// ...and it is still different from Send Text that does NOT press Enter.
		const never = { ...v1, actions: [{ ...act, kind: "send_text" as const }] };
		expect(ruleContentHash(older)).not.toBe(ruleContentHash(never));
	});

	it("is 8 hex characters", () => {
		expect(ruleContentHash(v1)).toMatch(/^[0-9a-f]{8}$/);
	});

	it("ruleContentKey is independent of object key order", () => {
		const reordered = {
			actions: v1.actions,
			precision: v1.precision,
			regex: v1.regex,
			name: v1.name,
			id: v1.id,
			enabled: true,
		};
		expect(ruleContentKey(reordered as SmartSelectionRule)).toBe(ruleContentKey(v1));
	});
});

describe("reconcileBuiltInRules", () => {
	it("does nothing for an empty saved list (the shipped set is used live)", () => {
		const input: SmartSelectionRule[] = [];
		const result = reconcileBuiltInRules(input, [v2]);
		expect(result).toEqual({ rules: input, updated: [], conflicts: [], changed: false });
		expect(result.rules).toBe(input);
	});

	it("does nothing when every saved built-in is already reconciled against the shipped version", () => {
		const input = [saved(v1)];
		const result = reconcileBuiltInRules(input, [v1]);
		expect(result.changed).toBe(false);
		expect(result.rules).toBe(input);
	});

	describe("a rule saved before versions existed (no builtInHash)", () => {
		it("is stamped with the current shipped hash and otherwise untouched, silently", () => {
			const result = reconcileBuiltInRules([v1], [v1]);
			expect(result.rules).toEqual([{ ...v1, builtInHash: ruleContentHash(v1) }]);
			expect(result.changed).toBe(true);
			expect(result.updated).toEqual([]);
			expect(result.conflicts).toEqual([]);
		});

		it("an edited one is stamped too and its edit kept, with no copy added", () => {
			const edited = rule({ regex: "mine" });
			const result = reconcileBuiltInRules([edited], [v1]);
			expect(result.rules).toEqual([{ ...edited, builtInHash: ruleContentHash(v1) }]);
			expect(result.conflicts).toEqual([]);
		});

		it("then behaves correctly when a LATER release changes the rule: untouched is updated, edited gets a copy", () => {
			const stamped = reconcileBuiltInRules(
				[v1, { ...v1, id: "dev-other", regex: "mine" }],
				[v1, { ...v1, id: "dev-other" }],
			).rules;
			const next = reconcileBuiltInRules(stamped, [v2, { ...v2, id: "dev-other" }]);
			expect(next.updated).toEqual([v2.name]);
			expect(next.conflicts).toEqual([v2.name]);
		});
	});

	describe("the shipped rule changed", () => {
		it("replaces an untouched rule in place with the new version and records it", () => {
			const before = rule({ id: "dev-first" });
			const after = [before, saved(v1), rule({ id: "dev-last" })];
			const result = reconcileBuiltInRules(after, [v2]);
			expect(result.updated).toEqual([v2.name]);
			expect(result.conflicts).toEqual([]);
			expect(result.changed).toBe(true);
			// Same slot, so rule order (which breaks score ties) is unchanged.
			expect(result.rules.map((r) => r.id)).toEqual(["dev-first", "dev-thing", "dev-last"]);
			expect(result.rules[1]).toEqual({ ...v2, enabled: true, builtInHash: ruleContentHash(v2) });
		});

		it("keeps an untouched rule's on/off switch: disabling is a preference, not an edit", () => {
			const result = reconcileBuiltInRules([saved(v1, { enabled: false })], [v2]);
			expect(result.updated).toEqual([v2.name]);
			expect(result.rules[0].enabled).toBe(false);
			expect(result.rules[0].regex).toBe("thing+");
		});

		it("leaves an edited rule exactly as the user wrote it and adds the new version after it, disabled", () => {
			const mine = saved(v1, { regex: "my-thing" });
			const result = reconcileBuiltInRules([mine], [v2]);
			expect(result.updated).toEqual([]);
			expect(result.conflicts).toEqual([v2.name]);
			expect(result.rules).toHaveLength(2);
			// Theirs: content untouched, now recorded as reconciled against the new version.
			expect(result.rules[0]).toEqual({ ...mine, builtInHash: ruleContentHash(v2) });
			// The new version: a distinct id, a changed title, off, and not tracked as a built-in.
			const copy = result.rules[1];
			expect(copy.id).toBe(`dev-thing-update-${ruleContentHash(v2)}`);
			expect(copy.name).toBe("Thing (better) (updated)");
			expect(copy.enabled).toBe(false);
			expect(copy.regex).toBe("thing+");
			expect(copy.builtInHash).toBeUndefined();
		});

		it("an edited rule whose edit already equals the new shipped version needs no copy", () => {
			const handMatched = saved(v1, { ...v2 });
			const result = reconcileBuiltInRules([handMatched], [v2]);
			expect(result.conflicts).toEqual([]);
			expect(result.updated).toEqual([]);
			expect(result.rules).toEqual([{ ...handMatched, builtInHash: ruleContentHash(v2) }]);
		});

		it("turning an option on counts as editing an untouched built-in, so a release adds a copy instead of overwriting it", () => {
			const act = v1.actions[0];
			const mine = saved(v1, { actions: [{ ...act, sendReturn: "when_safe" }] });
			const result = reconcileBuiltInRules([mine], [v2]);
			expect(result.updated).toEqual([]);
			expect(result.conflicts).toEqual([v2.name]);
			expect(result.rules[0].actions[0].sendReturn).toBe("when_safe");
		});

		it("a saved copy spelled run_command is unedited when the shipped rule says send_text + when safe", () => {
			const act = v1.actions[0];
			const shippedThen = rule({ actions: [{ ...act, kind: "send_text", sendReturn: "when_safe" }] });
			const savedOlder = saved(shippedThen, { actions: [{ ...act, kind: "run_command" }] });
			const shippedNow = rule({ ...shippedThen, regex: "thing+", actions: shippedThen.actions });
			const result = reconcileBuiltInRules([savedOlder], [shippedNow]);
			expect(result.updated).toEqual([shippedNow.name]);
			expect(result.conflicts).toEqual([]);
		});

		it("is idempotent: reconciling the result again changes nothing", () => {
			const first = reconcileBuiltInRules(
				[saved(v1), saved(v1, { id: "dev-edited", regex: "mine" })],
				[v2, { ...v2, id: "dev-edited" }],
			);
			const second = reconcileBuiltInRules(first.rules, [v2, { ...v2, id: "dev-edited" }]);
			expect(second.changed).toBe(false);
			expect(second.rules).toBe(first.rules);
		});

		it("a second release after a conflict adds another copy and keeps the first", () => {
			const v3 = rule({ regex: "thing++", name: "Thing (best)" });
			const afterV2 = reconcileBuiltInRules([saved(v1, { regex: "mine" })], [v2]).rules;
			const afterV3 = reconcileBuiltInRules(afterV2, [v3]);
			expect(afterV3.conflicts).toEqual([v3.name]);
			expect(afterV3.rules.map((r) => r.id)).toEqual([
				"dev-thing",
				`dev-thing-update-${ruleContentHash(v3)}`,
				`dev-thing-update-${ruleContentHash(v2)}`,
			]);
		});

		it("does not add a second copy if one with that id is already there", () => {
			const copyId = `dev-thing-update-${ruleContentHash(v2)}`;
			const existing = rule({ id: copyId, name: "kept by user" });
			const result = reconcileBuiltInRules([saved(v1, { regex: "mine" }), existing], [v2]);
			expect(result.rules.filter((r) => r.id === copyId)).toEqual([existing]);
			expect(result.conflicts).toEqual([]);
		});
	});

	describe("rules the shipped set does not govern", () => {
		it("leaves a user-written rule alone", () => {
			const userRule = rule({ id: "sel-abc", regex: "mine" });
			const input = [userRule, saved(v1)];
			const result = reconcileBuiltInRules(input, [v1]);
			expect(result.changed).toBe(false);
			expect(result.rules).toBe(input);
		});

		it("leaves a saved built-in alone once it is no longer shipped", () => {
			const input = [saved(v1)];
			expect(reconcileBuiltInRules(input, [rule({ id: "dev-other" })]).changed).toBe(false);
		});

		it("never brings back a shipped rule the user deleted", () => {
			const input = [rule({ id: "sel-abc" })];
			const result = reconcileBuiltInRules(input, [v2]);
			expect(result.rules.map((r) => r.id)).toEqual(["sel-abc"]);
		});
	});

	it("handles several rules in one pass and reports each by name", () => {
		const a1 = rule({ id: "dev-a", name: "A" });
		const a2 = rule({ id: "dev-a", name: "A", regex: "a2" });
		const b1 = rule({ id: "dev-b", name: "B" });
		const b2 = rule({ id: "dev-b", name: "B", regex: "b2" });
		const result = reconcileBuiltInRules([saved(a1), saved(b1, { regex: "mine" })], [a2, b2]);
		expect(result.updated).toEqual(["A"]);
		expect(result.conflicts).toEqual(["B"]);
		expect(result.rules.map((r) => r.id)).toEqual(["dev-a", "dev-b", `dev-b-update-${ruleContentHash(b2)}`]);
	});
});

describe("a copy materialised from the real shipped defaults, reconciled under a LATER release", () => {
	// The user edits one rule in Settings under release N: the whole default set is saved. Release N+1
	// then changes rule X. N+1's first startup must see the other rules as untouched and X's copy as
	// unedited (it still has N's content and N's baseline) — not stamp it with N+1's hash and call it edited.
	const releaseN = structuredClone(DEFAULT_SMART_SELECTION_RULES);
	const changed = "iterm-word";
	const releaseN1 = DEFAULT_SMART_SELECTION_RULES.map((r) => ({
		...r,
		...(r.id === changed ? { regex: "\\S+x" } : {}),
	}));

	it("updates the unedited rule the release changed, silently stamps nothing else, and adds no copies", () => {
		const result = reconcileBuiltInRules(releaseN, releaseN1);
		expect(result.updated).toEqual([releaseN1.find((r) => r.id === changed)!.name]);
		expect(result.conflicts).toEqual([]);
		expect(result.rules).toHaveLength(releaseN.length);
		expect(result.rules.find((r) => r.id === changed)!.regex).toBe("\\S+x");
		// Every other rule is byte-for-byte what was saved.
		for (const saved of releaseN.filter((r) => r.id !== changed)) {
			expect(result.rules.find((r) => r.id === saved.id)).toEqual(saved);
		}
	});

	it("keeps the user's edit and offers the new version beside it when they had edited that rule", () => {
		const edited = releaseN.map((r) => (r.id === changed ? { ...r, regex: "\\w+" } : r));
		const result = reconcileBuiltInRules(edited, releaseN1);
		expect(result.updated).toEqual([]);
		expect(result.conflicts).toEqual([releaseN1.find((r) => r.id === changed)!.name]);
		expect(result.rules.find((r) => r.id === changed)!.regex).toBe("\\w+");
		expect(result.rules.some((r) => r.name.endsWith("(updated)") && r.enabled === false)).toBe(true);
	});

	it("is a no-op when nothing shipped changed (the common restart)", () => {
		expect(reconcileBuiltInRules(releaseN, structuredClone(DEFAULT_SMART_SELECTION_RULES)).changed).toBe(false);
	});
});
