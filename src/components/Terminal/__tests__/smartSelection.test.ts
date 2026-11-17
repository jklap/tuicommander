import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { shVar } from "../../../__tests__/helpers/shellVar";
import {
	buildRunCommandScript,
	compileRules,
	findSmartMatches,
	hasControlBytes,
	MAX_TYPED_RUN_LINE_BYTES,
	PRECISION_WEIGHT,
	shellQuote,
	substituteActionParameter,
	typedRunLine,
} from "../smartSelection";
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

describe("findSmartMatches: the best match (first entry)", () => {
	const best = (text: string, offset: number, rules: SmartSelectionRule[], budgetMs?: number) =>
		findSmartMatches(text, offset, rules, budgetMs)[0] ?? null;

	it("returns null when no rule matches the offset", () => {
		expect(best("foo bar", 100, [rule()])).toBeNull();
	});

	it("rejects a match that doesn't span the click offset", () => {
		// "foo" matches at 0-3, but offset 5 is inside "bar" — a whitespace-word
		// rule alone shouldn't claim it since \S+ only matches "bar" separately,
		// which DOES span offset 5; assert against a rule that can't reach it.
		const digitsOnly = rule({ regex: "[0-9]+" });
		expect(best("foo bar", 5, [digitsOnly])).toBeNull();
	});

	it("higher precision wins even with a shorter match", () => {
		const rules = [
			rule({ id: "low-long", regex: "\\S+", precision: "low" }), // matches "https://example.com" (20 chars)
			rule({ id: "high-short", regex: "https", precision: "high" }), // matches "https" (5 chars)
		];
		const text = "see https://example.com now";
		const match = best(text, 6, rules); // offset 6 is inside "https"
		expect(match?.rule.id).toBe("high-short");
		expect(match?.text).toBe("https");
	});

	it("within the same precision class, the longer match wins", () => {
		const rules = [
			rule({ id: "word", regex: "[a-z]+", precision: "normal" }),
			rule({ id: "wordcolon", regex: "[a-z]+:", precision: "normal" }),
		];
		const text = "foo:bar";
		const match = best(text, 1, rules);
		expect(match?.rule.id).toBe("wordcolon");
		expect(match?.text).toBe("foo:");
	});

	it("the user's motivating case: adding a scheme alternate expands selection across the whole URL", () => {
		const urlRule = rule({ id: "url", regex: "https://[^\\s]+", precision: "very_high" });
		const wordRule = rule({ id: "word", regex: "[a-zA-Z]+", precision: "low" });
		const text = "cloning https://github.com/foo/bar.git into repo";
		// Click lands inside "github" — the URL rule's match still spans it.
		const clickOffset = text.indexOf("github") + 2;
		const match = best(text, clickOffset, [urlRule, wordRule]);
		expect(match?.rule.id).toBe("url");
		expect(match?.text).toBe("https://github.com/foo/bar.git");
	});

	it("returns null (falls back to word selection) once the time budget is exhausted", () => {
		const matchable = rule({ regex: "\\S+" });
		expect(best("foo bar", 1, [matchable], -1)).toBeNull();
	});

	it("finds a match anywhere in a multi-row scanned window, not just the first line", () => {
		const shaRule = rule({ id: "sha", regex: "[0-9a-f]{7,40}", precision: "high" });
		const text = "line one\nfix: handle EOF a1b2c3d\nline three";
		const offset = text.indexOf("a1b2c3d") + 2;
		const match = best(text, offset, [shaRule]);
		expect(match?.text).toBe("a1b2c3d");
	});

	// A zero-length match (e.g. `x*` against text with no "x") can't advance
	// `lastIndex` on its own — regression coverage for the manual bump that
	// keeps this from looping forever, and for the fact a zero-length match
	// can never itself satisfy `matchEnd > targetOffset`, so it's correctly
	// never a candidate.
	it("a rule whose regex can match zero-length text neither hangs nor wins a match", () => {
		const zeroWidth = rule({ id: "zero", regex: "x*", precision: "very_high" });
		expect(best("foo bar", 1, [zeroWidth])).toBeNull();
	});
});

describe("findSmartMatches", () => {
	const word = rule({ id: "word", regex: "\\S+", precision: "low" });
	const md = rule({ id: "md", regex: "[A-Za-z0-9._-]+\\.md", precision: "high" });
	const path = rule({ id: "path", regex: "([A-Za-z0-9._-]+/+)*[A-Za-z0-9._-]+\\.md", precision: "normal" });

	it("returns one entry per matching rule, best first", () => {
		const matches = findSmartMatches("see README.md now", 6, [word, md, path]);
		expect(matches.map((m) => m.rule.id)).toEqual(["md", "path", "word"]);
	});

	it("omits rules that do not span the offset", () => {
		const matches = findSmartMatches("see README.md now", 1, [word, md]);
		expect(matches.map((m) => m.rule.id)).toEqual(["word"]);
	});

	it("keeps rule order for equal scores (iTerm2's strict `>` tie rule)", () => {
		const first = rule({ id: "first", regex: "[A-Za-z.]+", precision: "normal" });
		const second = rule({ id: "second", regex: "[A-Za-z.]+", precision: "normal" });
		expect(findSmartMatches("README.md", 2, [first, second]).map((m) => m.rule.id)).toEqual(["first", "second"]);
	});

	it("skips disabled rules", () => {
		expect(findSmartMatches("README.md", 2, [{ ...md, enabled: false }])).toEqual([]);
	});

	it("returns only the occurrence of a repeated pattern that spans the offset, with its capture groups", () => {
		const pair = rule({ id: "pair", regex: "(\\w+)\\.md", precision: "normal" });
		// Two matches in the text; the click is inside the second.
		const matches = findSmartMatches("a.md b.md", 6, [pair]);
		expect(matches).toHaveLength(1);
		expect(matches[0].text).toBe("b.md");
		expect(matches[0].startOffset).toBe(5);
		expect(matches[0].endOffset).toBe(9);
		expect(matches[0].groups).toEqual(["b"]);
	});

	it("reports each rule's own span (rules can match different extents of the same text)", () => {
		const matches = findSmartMatches("see (README.md) now", 8, [word, md]);
		const spans = Object.fromEntries(matches.map((m) => [m.rule.id, m.text]));
		expect(spans).toEqual({ word: "(README.md)", md: "README.md" });
	});

	it("scores a match as precision weight times length", () => {
		const [match] = findSmartMatches("README.md", 2, [md]);
		expect(match.score).toBe(PRECISION_WEIGHT.high * "README.md".length);
	});

	it("returns nothing once the time budget is exhausted before the first rule", () => {
		expect(findSmartMatches("README.md", 2, [md], -1)).toEqual([]);
	});

	it("a zero-length-matching rule neither hangs nor appears", () => {
		const zeroWidth = rule({ id: "zero", regex: "x*", precision: "very_high" });
		expect(findSmartMatches("foo bar", 1, [zeroWidth, word]).map((m) => m.rule.id)).toEqual(["word"]);
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

describe("shellQuote", () => {
	it("wraps in single quotes and escapes embedded single quotes", () => {
		expect(shellQuote("abc")).toBe("'abc'");
		expect(shellQuote("it's")).toBe("'it'\\''s'");
		expect(shellQuote("")).toBe("''");
	});

	it("drops NUL bytes, which no program can receive", () => {
		expect(shellQuote("a\0b")).toBe("'ab'");
	});

	// The property that matters is not the string we produce but what a real shell does with it.
	const hostile = [
		"plain",
		"with space",
		"it's",
		"''",
		"$(echo PWNED)",
		"`echo PWNED`",
		";echo PWNED",
		"a && echo PWNED",
		"a | echo PWNED",
		"a > /dev/null",
		"\"double\" and 'single'",
		"back\\slash \\n",
		"line1\nline2",
		"*",
		"~",
		"-n",
		// biome-ignore lint/suspicious/noTemplateCurlyInString: the point of this case is a literal ${...} reaching the shell
		"${HOME}",
		"!history",
		"#comment",
		"tab\there",
	];
	it.skipIf(process.platform === "win32").each(hostile)(
		"a POSIX shell reads %j back as exactly one literal word",
		(value) => {
			// printf receives the quoted text as a single argument iff the quoting held; any split,
			// expansion or command substitution would change (or add to) what it prints.
			const out = execFileSync("/bin/sh", ["-c", `printf '%s|' ${shellQuote(value)}`], { encoding: "utf8" });
			expect(out).toBe(`${value}|`);
		},
	);

	it.skipIf(process.platform === "win32")("a quoted value cannot run anything", () => {
		const out = execFileSync("/bin/sh", ["-c", `echo ${shellQuote("$(echo PWNED)")} ${shellQuote(";echo PWNED")}`], {
			encoding: "utf8",
		});
		expect(out.trim()).toBe("$(echo PWNED) ;echo PWNED");
	});
});

describe("substituteActionParameter", () => {
	const ctx = { match: "a b", groups: ["g 1"], cwd: "/c d", user: "u", host: "h" };

	it("inserts every value as plain text, unquoted (it is data, or typed for a person to read)", () => {
		expect(substituteActionParameter("x \\0 \\1 \\d \\u \\h ; y", ctx)).toBe("x a b g 1 /c d u h ; y");
	});

	it("expands \\n and \\\\ and treats an unmatched group or missing context as empty", () => {
		expect(substituteActionParameter("a\\nb\\\\c", ctx)).toBe("a\nb\\c");
		expect(substituteActionParameter("[\\2|\\d]", { match: "m", groups: [] })).toBe("[|]");
	});
});

/** Run `sh`-family shells in a scratch directory; skipped where a shell is not installed. */
function findShell(name: string): string | null {
	try {
		return execFileSync("/bin/sh", ["-c", `command -v ${name}`], { encoding: "utf8" }).trim() || null;
	} catch {
		return null;
	}
}

function withScratch<T>(run: (dir: string) => T): T {
	const dir = mkdtempSync(join(tmpdir(), "tuic-run-"));
	try {
		return run(dir);
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
}

const onUnix = process.platform !== "win32";

describe("buildRunCommandScript", () => {
	const ctx = { match: "a b", groups: ["g 1"], cwd: "/c d", user: "u", host: "h" };

	describe("the form of a reference follows the quoting around the placeholder", () => {
		it("a bare placeholder becomes a quoted reference (never split or globbed)", () => {
			expect(buildRunCommandScript("imark \\0", ctx)).toEqual({
				script: `imark "${shVar("TUIC_M0")}"`,
				values: { TUIC_M0: "a b" },
			});
		});

		it("inside double quotes it is just the reference: the context already quotes it", () => {
			expect(buildRunCommandScript('echo "<\\0>"', ctx).script).toBe(`echo "<${shVar("TUIC_M0")}>"`);
		});

		it("inside single quotes the quote is closed around the reference, since '…' cannot expand", () => {
			expect(buildRunCommandScript("echo '<\\0>'", ctx).script).toBe(`echo '<'"\${TUIC_M0}"'>'`);
		});

		it("quoting state returns to normal after a closed quote", () => {
			expect(buildRunCommandScript("echo 'x' \\0 \"y\" \\0", ctx).script).toBe(
				`echo 'x' "\${TUIC_M0}" "y" "\${TUIC_M0}"`,
			);
		});

		it("a backslash-escaped quote is a literal quote and does not change the state", () => {
			expect(buildRunCommandScript("echo it\\'s \\0", ctx).script).toBe(`echo it\\'s "\${TUIC_M0}"`);
			expect(buildRunCommandScript('echo "a \\" \\0"', ctx).script).toBe(`echo "a \\" \${TUIC_M0}"`);
		});

		it("a backslash inside single quotes is literal, so '\\' is a complete string", () => {
			expect(buildRunCommandScript("echo '\\' \\0", ctx).script).toBe(`echo '\\' "\${TUIC_M0}"`);
		});
	});

	describe("values", () => {
		it("every placeholder code has its own variable, and only the ones used are present", () => {
			const run = buildRunCommandScript("\\0 \\1 \\9 \\d \\u \\h", ctx);
			expect(run.values).toEqual({
				TUIC_M0: "a b",
				TUIC_M1: "g 1",
				TUIC_M9: "",
				TUIC_D: "/c d",
				TUIC_U: "u",
				TUIC_H: "h",
			});
			expect(buildRunCommandScript("tool \\0", ctx).values).toEqual({ TUIC_M0: "a b" });
			expect(buildRunCommandScript("tool", ctx).values).toEqual({});
		});

		it("a repeated placeholder shares one variable", () => {
			const run = buildRunCommandScript("cp \\0 \\0.bak", ctx);
			expect(run.script).toBe(`cp "${shVar("TUIC_M0")}" "${shVar("TUIC_M0")}".bak`);
			expect(Object.keys(run.values)).toEqual(["TUIC_M0"]);
		});

		it("an unmatched group or missing context value is an empty string, not a missing argument", () => {
			const run = buildRunCommandScript("a \\3 b \\d", { match: "m", groups: [] });
			expect(run.script).toBe(`a "${shVar("TUIC_M3")}" b "${shVar("TUIC_D")}"`);
			expect(run.values).toEqual({ TUIC_M3: "", TUIC_D: "" });
		});

		it("drops NUL bytes (the backend refuses them, and no program can receive one)", () => {
			expect(buildRunCommandScript("x \\0", { match: "a\0b", groups: [] }).values.TUIC_M0).toBe("ab");
		});

		it("only the names the backend accepts are ever produced", () => {
			const names = Object.keys(buildRunCommandScript("\\0\\1\\2\\3\\4\\5\\6\\7\\8\\9\\d\\u\\h", ctx).values);
			expect(names.every((name) => /^TUIC_(M[0-9]|D|U|H)$/.test(name))).toBe(true);
			expect(names).toHaveLength(13);
		});
	});

	describe("the template's own text", () => {
		it("is left exactly as written: operators, quotes, comments and variables", () => {
			expect(buildRunCommandScript('cd "$HOME" && tool \\0 | tee out; echo done # \\0', ctx).script).toBe(
				`cd "$HOME" && tool "${shVar("TUIC_M0")}" | tee out; echo done # "${shVar("TUIC_M0")}"`,
			);
		});

		it("a quote character inside a # comment is not a quote: later placeholders keep the right form", () => {
			// Without comment handling the apostrophe would open a single-quoted string, and `\0` on
			// the next line would be written for THAT context (`'"${TUIC_M0}"'`).
			expect(buildRunCommandScript("# don't\necho \\0", ctx).script).toBe(`# don't\necho "${shVar("TUIC_M0")}"`);
			expect(buildRunCommandScript('tool # say "hi\n"<\\0>"', ctx).script).toBe(
				`tool # say "hi\n"<${shVar("TUIC_M0")}>"`,
			);
		});

		it("a # that is not at the start of a word is not a comment", () => {
			// `$#` and `a#b` are ordinary text; if they started a comment the double quote after them
			// would be ignored and the placeholder inside it written as if it were bare.
			expect(buildRunCommandScript('echo $# "<\\0>"', ctx).script).toBe(`echo $# "<${shVar("TUIC_M0")}>"`);
			expect(buildRunCommandScript('echo a#b "<\\0>"', ctx).script).toBe(`echo a#b "<${shVar("TUIC_M0")}>"`);
		});

		it("a # inside quotes is not a comment", () => {
			expect(buildRunCommandScript("echo '#' \"<\\0>\"", ctx).script).toBe(`echo '#' "<${shVar("TUIC_M0")}>"`);
		});

		it("a \\n placeholder in the template ends a comment, like the newline it becomes", () => {
			expect(buildRunCommandScript('# note\\necho "<\\0>"', ctx).script).toBe(`# note\necho "<${shVar("TUIC_M0")}>"`);
		});

		it("keeps \\n as a newline and \\\\ as a backslash", () => {
			expect(buildRunCommandScript("echo a\\nb\\\\c", ctx).script).toBe("echo a\nb\\c");
		});

		it("a template with no placeholders is returned unchanged with no values", () => {
			expect(buildRunCommandScript("open -a Foo", ctx)).toEqual({ script: "open -a Foo", values: {} });
		});
	});

	// What a value can do is decided by a real shell, not by the string we built.
	describe.skipIf(!onUnix)("run by a real /bin/sh with the values in the environment", () => {
		const hostile = (marker: string) => [
			`$(touch ${marker})`,
			`\`touch ${marker}\``,
			`a; touch ${marker}`,
			`'; touch ${marker} #`,
			`" ; touch ${marker} #`,
			`\\' ; touch ${marker} #`,
			`x\ntouch ${marker}`,
			`$((0+$(touch ${marker})))`,
			`\${IFS}touch${"$"}{IFS}${marker}`,
			"*",
			"-n",
			"~",
			"café 日本語",
		];
		const templates: [string, string, (v: string) => string][] = [
			["bare", "printf %s \\0 > out", (v) => v],
			["user put it in double quotes", 'printf %s "<\\0>" > out', (v) => `<${v}>`],
			["user put it in single quotes", "printf %s '<\\0>' > out", (v) => `<${v}>`],
		];

		it.each(templates)(
			"%s: the value arrives as one literal argument and nothing runs",
			(_label, template, expected) => {
				withScratch((dir) => {
					const marker = join(dir, "pwned");
					for (const value of hostile(marker)) {
						const run = buildRunCommandScript(template, { match: value, groups: [] });
						execFileSync("/bin/sh", ["-c", run.script], { cwd: dir, env: { ...process.env, ...run.values } });
						expect(existsSync(marker), `executed: ${JSON.stringify(value)}`).toBe(false);
						expect(readFileSync(join(dir, "out"), "utf8"), JSON.stringify(value)).toBe(expected(value));
					}
				});
			},
		);

		it("a heredoc body: the value is expanded as text and never re-read as code", () => {
			withScratch((dir) => {
				const marker = join(dir, "pwned");
				for (const value of hostile(marker)) {
					const run = buildRunCommandScript("cat > out <<EOF\n\\0\nEOF", { match: value, groups: [] });
					execFileSync("/bin/sh", ["-c", run.script], { cwd: dir, env: { ...process.env, ...run.values } });
					expect(existsSync(marker), `executed: ${JSON.stringify(value)}`).toBe(false);
					expect(readFileSync(join(dir, "out"), "utf8")).toContain(value.split("\n")[0]);
				}
			});
		});

		it("an apostrophe in a comment does not change how a later quoted placeholder is passed", () => {
			withScratch((dir) => {
				const run = buildRunCommandScript('# don\'t mind this\nprintf %s "<\\0>" > out', {
					match: "two words",
					groups: [],
				});
				execFileSync("/bin/sh", ["-c", run.script], { cwd: dir, env: { ...process.env, ...run.values } });
				expect(readFileSync(join(dir, "out"), "utf8")).toBe("<two words>");
			});
		});

		it("control: the same text pasted INTO the command line does run", () => {
			withScratch((dir) => {
				const marker = join(dir, "pwned");
				execFileSync("/bin/sh", ["-c", `printf %s $(touch ${marker}) > out`], { cwd: dir });
				expect(existsSync(marker)).toBe(true);
			});
		});
	});
});

describe("hasControlBytes", () => {
	it("flags C0 controls, DEL and C1 controls in any value", () => {
		for (const bad of ["a\x03b", "a\x15b", "\x1b[201~", "a\nb", "a\tb", "a\x7fb", "a\x85b", "\x00"]) {
			expect(hasControlBytes({ TUIC_M0: "fine", TUIC_D: bad }), JSON.stringify(bad)).toBe(true);
		}
	});

	it("passes ordinary text, including spaces, quotes and non-ASCII", () => {
		expect(hasControlBytes({ TUIC_D: "/Users/me/my dir/it's — café 日本語" })).toBe(false);
		expect(hasControlBytes({})).toBe(false);
	});

	it("keeps the typed-line cap under the 1024-byte MAX_CANON of a shell that has no line editor yet", () => {
		expect(MAX_TYPED_RUN_LINE_BYTES).toBeLessThan(1024);
	});
});

describe("typedRunLine", () => {
	it("is the values as one-shot assignments followed by /bin/sh -c <quoted script>", () => {
		const run = buildRunCommandScript("imark \\0", { match: "a b", groups: [] });
		expect(typedRunLine(run)).toBe(`TUIC_M0='a b' /bin/sh -c 'imark "${shVar("TUIC_M0")}"'`);
	});

	it("is just /bin/sh -c '…' when the template has no placeholders", () => {
		expect(typedRunLine(buildRunCommandScript("open -a Foo", { match: "x", groups: [] }))).toBe(
			"/bin/sh -c 'open -a Foo'",
		);
	});

	it("single-quotes both the values and the script, including an apostrophe in either", () => {
		const run = buildRunCommandScript("echo 'hi' \\0", { match: "it's", groups: [] });
		expect(typedRunLine(run)).toBe(`TUIC_M0='it'\\''s' /bin/sh -c 'echo '\\''hi'\\'' "\${TUIC_M0}"'`);
	});

	// This line is TYPED into the user's own shell, so it is only as safe as that shell's reading of
	// POSIX quoting. These are the shells the backend allows (`typed_quoting_is_sound`).
	describe.skipIf(!onUnix)("typed into each installed shell that the backend allows", () => {
		const shells = ["sh", "dash", "bash", "zsh", "ksh"].map((name) => [name, findShell(name)] as const);

		for (const [name, path] of shells) {
			it.skipIf(path === null)(`${name}: values are data, in every quoting context`, () => {
				withScratch((dir) => {
					const marker = join(dir, "pwned");
					const env = { ...process.env, HOME: dir, ZDOTDIR: dir };
					const values = [
						`$(touch ${marker})`,
						`\`touch ${marker}\``,
						`'; touch ${marker} #`,
						`\\' ; touch ${marker} #`,
						`" ; touch ${marker} #`,
						`x\ntouch ${marker}`,
						"it's",
					];
					for (const template of ["printf %s \\0 > out", 'printf %s "<\\0>" > out', "printf %s '<\\0>' > out"]) {
						for (const value of values) {
							const line = typedRunLine(buildRunCommandScript(template, { match: value, groups: [] }));
							execFileSync(path as string, ["-c", line], { cwd: dir, env });
							expect(existsSync(marker), `${name}: executed ${JSON.stringify(value)} via ${template}`).toBe(false);
							const written = readFileSync(join(dir, "out"), "utf8");
							expect(written === value || written === `<${value}>`, `${name}: ${JSON.stringify(written)}`).toBe(true);
						}
					}
				});
			});
		}

		// Why the backend refuses fish for this mode. Depending on the fish version the broken-out
		// line either RUNS the injected command or is rejected as a syntax error before anything
		// runs — both mean fish did not read it as the POSIX shells do. What must never be true is
		// that the value arrives intact, which is the property the supported shells all have.
		it.skipIf(findShell("fish") === null)("control: fish does not read the same line as the POSIX shells do", () => {
			withScratch((dir) => {
				const marker = join(dir, "pwned");
				const value = `\\' /bin/true ; touch ${marker} #`;
				const line = typedRunLine(buildRunCommandScript("printf %s \\0 > out", { match: value, groups: [] }));
				try {
					execFileSync(findShell("fish") as string, ["--no-config", "-c", line], { cwd: dir, stdio: "ignore" });
				} catch {
					// A syntax error is a non-zero exit, which is fine: only the outcome below matters.
				}
				const arrivedIntact = existsSync(join(dir, "out")) && readFileSync(join(dir, "out"), "utf8") === value;
				expect(arrivedIntact, "fish unexpectedly read the line like a POSIX shell").toBe(false);
			});
		});
	});
});
