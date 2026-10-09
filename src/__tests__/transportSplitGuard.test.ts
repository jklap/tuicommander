import { readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";
import { reachableModules } from "./helpers/importGraph";

/**
 * mobile.html never imports transportExtended.ts (that is the point of the
 * split — see its header), so a command mobile can call MUST be in
 * transport.ts's core COMMAND_TABLE: an extended-only command throws
 * "No HTTP mapping for command" at its first call on a phone.
 *
 * This walks mobile's FULL module graph (lazy `import()` screens included)
 * and fails on any command-name literal passed to an invoke/rpc helper or a
 * config-delta writer that only the extended table maps.
 *
 * Whole-statement `import type` edges are not followed: they are erased at
 * compile time. That is why the plugin runtime (pluginStore -> pluginRegistry,
 * which calls the extended-only `plugin_write_file_base64`) is NOT in mobile's
 * graph — its only route in was `ActivityScreen`'s `import type` of
 * `plugins/types`. If a real import ever pulls it in, this test names it.
 */
const ROOT = process.cwd();
const read = (p: string) => readFileSync(p, "utf8");

/** Top-level keys of `const <name> ... = { ... };` (one tab of indent). */
function tableKeys(src: string, name: string): Set<string> {
	const start = src.indexOf(`const ${name}`);
	expect(start).toBeGreaterThanOrEqual(0);
	const body = src.slice(start, src.indexOf("\n};", start));
	return new Set([...body.matchAll(/^\t([a-z][a-z_0-9]*): /gm)].map((m) => m[1]));
}

/** `invoke("x"`, `rpc<T>("x"`, `safeInvoke("x"`, … — a literal command name as the first argument. */
const CALL = /\b(?:rpc|invoke|[A-Za-z]*Invoke[A-Za-z]*)\s*(?:<[^()]*>)?\(\s*["'`]([a-z][a-z_0-9]+)["'`]/g;
/** `createConfigDeltaWriter<T>("save_x"` — the writer invokes this name later via a computed call. */
const DELTA_WRITER = /createConfigDeltaWriter\s*(?:<[^()]*>)?\(\s*["'`]([a-z][a-z_0-9]+)["'`]/g;

describe("mobile never calls an extended-only command", () => {
	const extended = tableKeys(read(join(ROOT, "src/transportExtended.ts")), "EXTENDED_COMMAND_TABLE");
	const core = tableKeys(read(join(ROOT, "src/transport.ts")), "COMMAND_TABLE");
	const files = reachableModules(join(ROOT, "src/mobile/index.tsx"), { includeDynamic: true });

	it("parses both tables and walks the mobile graph (sanity)", () => {
		expect(extended.size).toBeGreaterThan(50);
		expect(core.size).toBeGreaterThan(50);
		expect(core.has("plugin_write_file")).toBe(true);
		// Lazy screens count: they are loaded on demand, but still on a phone.
		expect(files.map((f) => relative(ROOT, f))).toContain("src/mobile/screens/SettingsScreen.tsx");
	});

	it("never imports transportExtended or the desktop entry", () => {
		expect(files.map((f) => relative(ROOT, f)).filter((f) => /transportExtended|appEntry/.test(f))).toEqual([]);
	});

	it("no mobile-reachable call site names an EXTENDED_COMMAND_TABLE-only command", () => {
		const bad: string[] = [];
		for (const file of files) {
			if (/\.test\.|\/transport\.ts$|\.json$/.test(file)) continue;
			const src = read(file);
			for (const re of [CALL, DELTA_WRITER]) {
				for (const m of src.matchAll(re)) {
					if (extended.has(m[1]) && !core.has(m[1])) bad.push(`${m[1]} @ ${relative(ROOT, file)}`);
				}
			}
		}
		expect(bad).toEqual([]);
	});
});
