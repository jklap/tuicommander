import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, resolve } from "node:path";

/**
 * Static (eager) import/re-export specifiers in `src`: `import x from "y"`,
 * `import "y"`, `export { x } from "y"`. Whole-statement `import type` /
 * `export type` lines are skipped because the compiler erases them.
 */
const STATIC_FROM = /^\s*(?:import|export)\s+(type\s+)?[^;'"`]*?\bfrom\s+["']([^"']+)["']/gm;
const SIDE_EFFECT = /^\s*import\s+["']([^"']+)["']/gm;
const DYNAMIC = /\bimport\s*\(\s*["']([^"']+)["']\s*\)/g;

function resolveSpec(from: string, spec: string): string | null {
	if (!spec.startsWith(".")) return null;
	const base = resolve(dirname(from), spec);
	for (const c of [base, `${base}.ts`, `${base}.tsx`, `${base}/index.ts`, `${base}/index.tsx`]) {
		if (existsSync(c) && statSync(c).isFile() && /\.(tsx?|json)$/.test(c)) return c;
	}
	return null;
}

/**
 * Every local module reachable from `entry` (absolute path). With
 * `includeDynamic: false` only eager edges are followed, which is what lands in
 * a Vite entry's initial `<script>`/`<link>` set; with `true`, lazy `import()`
 * edges are followed too (every module the entry can ever load).
 */
export function reachableModules(entry: string, opts: { includeDynamic: boolean }): string[] {
	const seen = new Set<string>();
	const queue = [entry];
	while (queue.length > 0) {
		const file = queue.pop() as string;
		if (seen.has(file)) continue;
		seen.add(file);
		if (file.endsWith(".json")) continue;
		const src = readFileSync(file, "utf8");
		const specs: string[] = [];
		for (const m of src.matchAll(STATIC_FROM)) if (!m[1]) specs.push(m[2]);
		for (const m of src.matchAll(SIDE_EFFECT)) specs.push(m[1]);
		if (opts.includeDynamic) for (const m of src.matchAll(DYNAMIC)) specs.push(m[1]);
		for (const spec of specs) {
			const r = resolveSpec(file, spec);
			if (r) queue.push(r);
		}
	}
	return [...seen];
}
