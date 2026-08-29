import { describe, expect, it } from "vitest";
import { compileRules, findSmartMatch } from "../smartSelection";
import { DEFAULT_SMART_SELECTION_RULES, resolveSmartSelectionRules } from "../smartSelectionDefaults";

describe("DEFAULT_SMART_SELECTION_RULES", () => {
	it("has no duplicate ids", () => {
		const ids = DEFAULT_SMART_SELECTION_RULES.map((r) => r.id);
		expect(new Set(ids).size).toBe(ids.length);
	});

	it("every rule's regex compiles (no rule silently drops due to an invalid pattern)", () => {
		const compiled = compileRules(DEFAULT_SMART_SELECTION_RULES.map((r) => ({ ...r, enabled: true })));
		expect(compiled.map((c) => c.rule.id).sort()).toEqual([...DEFAULT_SMART_SELECTION_RULES].map((r) => r.id).sort());
	});

	it("at most one action per rule is marked default", () => {
		for (const rule of DEFAULT_SMART_SELECTION_RULES) {
			const defaults = rule.actions.filter((a) => a.isDefault);
			expect(defaults.length, `rule ${rule.id} has ${defaults.length} default actions`).toBeLessThanOrEqual(1);
		}
	});

	/** Each rule's own regex against a line built to contain exactly what it's for. */
	const CASES: Record<string, { line: string; expected: string }> = {
		"iterm-word": { line: "foo bar baz", expected: "bar" },
		"iterm-cpp-namespace": { line: "call std::vector::push_back now", expected: "std::vector::push_back" },
		"iterm-path": { line: "open /usr/local/bin/node please", expected: "/usr/local/bin/node" },
		"iterm-quoted-string": { line: 'say "hello world" now', expected: '"hello world"' },
		"iterm-java-python-include": { line: "import com.example.Foo now", expected: "com.example.Foo" },
		"iterm-mailto": { line: "link mailto:user@example.com here", expected: "mailto:user@example.com" },
		"iterm-objc-selector": { line: "call @selector(doSomething:) now", expected: "@selector(doSomething:)" },
		"iterm-email": { line: "contact user@example.com today", expected: "user@example.com" },
		"iterm-http-url": { line: "cloning https://github.com/foo/bar.git in", expected: "https://github.com/foo/bar.git" },
		"iterm-ssh-url": { line: "connect ssh:user@host.example.com now", expected: "ssh:user@host.example.com" },
		"iterm-telnet-url": { line: "connect telnet:host.example.com now", expected: "telnet:host.example.com" },
		"dev-git-sha": { line: "fix: handle EOF a1b2c3d today", expected: "a1b2c3d" },
		"dev-file-line-col": { line: "error at src/app.ts:42:7 now", expected: "src/app.ts:42:7" },
		"dev-semver": { line: "released v1.2.3-beta.1 today", expected: "v1.2.3-beta.1" },
		"dev-ipv4": { line: "server at 192.168.1.42 now", expected: "192.168.1.42" },
		"dev-ipv6": { line: "server at fe80::1 now", expected: "fe80::1" },
		"dev-uuid": {
			line: "id is 550e8400-e29b-41d4-a716-446655440000 now",
			expected: "550e8400-e29b-41d4-a716-446655440000",
		},
		"dev-issue-key": { line: "see PROJ-1234 for details", expected: "PROJ-1234" },
		"dev-issue-ref": { line: "fixes #1234 today", expected: "#1234" },
	};

	it("covers every default rule in the CASES table (canary against the table going stale)", () => {
		expect(Object.keys(CASES).sort()).toEqual(DEFAULT_SMART_SELECTION_RULES.map((r) => r.id).sort());
	});

	for (const rule of DEFAULT_SMART_SELECTION_RULES) {
		it(`"${rule.id}" matches its representative line`, () => {
			const { line, expected } = CASES[rule.id];
			const offset = line.indexOf(expected) + Math.floor(expected.length / 2);
			// Match against just this rule (forced enabled) so a lower-precision
			// rule elsewhere in the default set can't steal a longer overlapping
			// match — this test is about the rule's own regex, not arbitration.
			const match = findSmartMatch(line, offset, [{ ...rule, enabled: true }]);
			expect(match?.text).toBe(expected);
		});
	}
});

describe("resolveSmartSelectionRules", () => {
	it("returns the defaults when the user has defined no rules", () => {
		expect(resolveSmartSelectionRules([])).toBe(DEFAULT_SMART_SELECTION_RULES);
	});

	it("returns the user's rules verbatim when they've defined any", () => {
		const userRules = [{ ...DEFAULT_SMART_SELECTION_RULES[0], id: "custom" }];
		expect(resolveSmartSelectionRules(userRules)).toBe(userRules);
	});
});
