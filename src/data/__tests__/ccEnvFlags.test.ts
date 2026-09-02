import { describe, expect, it } from "vitest";
import { CC_ENV_FLAGS, ENV_FLAG_CATEGORIES } from "../ccEnvFlags";

describe("CC_ENV_FLAGS", () => {
	it("has no duplicate keys", () => {
		const keys = CC_ENV_FLAGS.map((f) => f.key);
		const unique = new Set(keys);
		expect(unique.size).toBe(keys.length);
	});

	it("every entry has a non-empty key, description, and a category present in ENV_FLAG_CATEGORIES", () => {
		for (const flag of CC_ENV_FLAGS) {
			expect(flag.key.length).toBeGreaterThan(0);
			expect(flag.description.length).toBeGreaterThan(0);
			expect(Object.keys(ENV_FLAG_CATEGORIES)).toContain(flag.category);
		}
	});

	it("enum-typed entries declare an options array (possibly empty, meaning free-form)", () => {
		for (const flag of CC_ENV_FLAGS) {
			if (flag.type === "enum") {
				expect(Array.isArray(flag.options)).toBe(true);
			}
		}
	});

	it("does not list CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS", () => {
		// TUICommander injects this env var unconditionally (src-tauri/src/pty.rs)
		// into every PTY session — it is not a per-agent, user-togglable flag, so
		// it must not appear in this list (the toggle here was a no-op: unchecking
		// it could never actually undo the unconditional injection).
		const keys = CC_ENV_FLAGS.map((f) => f.key);
		expect(keys).not.toContain("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS");
	});
});
