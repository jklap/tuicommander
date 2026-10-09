export interface EnvVarEntry {
	key: string;
	value: string;
}

/** Find keys that appear more than once after trimming. Empty/whitespace-only keys are ignored. */
export function findDuplicateEnvKeys(entries: readonly EnvVarEntry[]): string[] {
	const counts = new Map<string, number>();
	for (const { key } of entries) {
		const k = key.trim();
		if (!k) continue;
		counts.set(k, (counts.get(k) ?? 0) + 1);
	}
	const dupes: string[] = [];
	for (const [k, count] of counts) {
		if (count > 1) dupes.push(k);
	}
	return dupes;
}

/** Build an env Record from entries. Throws if duplicate keys detected. Empty/whitespace keys are filtered out. */
export function buildEnvFromEntries(entries: readonly EnvVarEntry[]): Record<string, string> {
	const dupes = findDuplicateEnvKeys(entries);
	if (dupes.length > 0) {
		throw new Error(`Duplicate env keys: ${dupes.join(", ")}`);
	}
	const env: Record<string, string> = {};
	for (const { key, value } of entries) {
		const k = key.trim();
		if (k) env[k] = value;
	}
	return env;
}

/** A legal environment variable name: starts with an ASCII letter or `_`, the
 *  rest ASCII alphanumeric or `_`. Mirrors the backend's `valid_custom_env_key`
 *  (`src-tauri/src/config.rs`) exactly — defense-in-depth only, the backend
 *  independently re-validates at the point of use and is the real enforcement. */
export function isValidEnvVarKey(key: string): boolean {
	return /^[A-Za-z_][A-Za-z0-9_]*$/.test(key);
}

/** A name the backend refuses in Custom Environment Variables
 *  (`reserved_custom_env_key`, `src-tauri/src/config.rs`): TUIC's own `TUIC_*`
 *  identity env, `ZDOTDIR` (how the zsh integration loads) and the dynamic
 *  loader (`LD_PRELOAD`, `LD_LIBRARY_PATH`, `LD_AUDIT`, `DYLD_*`). UI feedback
 *  only — the backend drops these on load/save and never applies them. */
export function isReservedEnvVarKey(key: string): boolean {
	const upper = key.toUpperCase();
	return (
		upper.startsWith("TUIC_") ||
		upper === "ZDOTDIR" ||
		upper === "LD_PRELOAD" ||
		upper === "LD_LIBRARY_PATH" ||
		upper === "LD_AUDIT" ||
		upper.startsWith("DYLD_")
	);
}
