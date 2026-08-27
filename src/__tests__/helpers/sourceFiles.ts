import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

/** All `.ts`/`.tsx` files under `dir`, recursing but skipping `__tests__`. */
export function sourceFiles(dir: string): string[] {
	const out: string[] = [];
	for (const name of readdirSync(dir)) {
		const full = join(dir, name);
		if (statSync(full).isDirectory()) {
			if (name === "__tests__") continue;
			out.push(...sourceFiles(full));
			continue;
		}
		if (name.endsWith(".ts") || name.endsWith(".tsx")) out.push(full);
	}
	return out;
}

/** `text` with `//` and `/* *\/` comments blanked out (kept as whitespace, so
 *  line numbers and match offsets are unaffected), for source scans that must
 *  not trip on a comment merely mentioning the pattern they're guarding against. */
export function stripComments(text: string): string {
	return text
		.replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "))
		.replace(/\/\/[^\n]*/g, (m) => " ".repeat(m.length));
}

/** Read `file` and return it with comments stripped (see {@link stripComments}). */
export function readSourceWithoutComments(file: string): string {
	return stripComments(readFileSync(file, "utf8"));
}
