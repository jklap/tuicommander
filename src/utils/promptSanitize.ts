import { AGENT_TYPES } from "../agents";
import type { SavedPrompt, SmartPlacement } from "../stores/promptLibrary";

/** Result of sanitizing a prompt parsed from untrusted JSON. */
export interface SanitizeResult {
	prompt: SavedPrompt;
	/** Human-readable notes about anything that was clamped, stripped, or migrated. */
	warnings: string[];
	/** True if the legacy `tab-context` placement name was rewritten — callers that persist
	 *  the store (e.g. `hydrate()`) use this to decide whether a re-save is needed. */
	migratedPlacement: boolean;
}

/**
 * Validate and clamp the security-relevant fields on a prompt object that came from untrusted
 * JSON — backend storage (`prompt-library.json`, which a local MCP client can write arbitrary
 * `id`/fields into) or an imported export file from another machine.
 *
 * An invalid `executionMode` is reset to `"inject"`, an invalid `injectTarget` or
 * `preferredAgent` is stripped, a non-array `placement` is dropped, and the legacy
 * `tab-context` placement name is migrated to `terminal-context`.
 *
 * Mutates and returns `full` in place. Callers must validate required fields (`id`, `name`,
 * `content`) beforehand — this function assumes they're already present.
 */
export function sanitizePrompt(full: SavedPrompt): SanitizeResult {
	const warnings: string[] = [];

	if (
		full.executionMode &&
		full.executionMode !== "inject" &&
		full.executionMode !== "headless" &&
		full.executionMode !== "api" &&
		full.executionMode !== "shell"
	) {
		warnings.push(`Prompt "${full.id}" has invalid executionMode "${full.executionMode}", resetting to inject`);
		full.executionMode = "inject";
	}
	if (full.injectTarget && full.injectTarget !== "terminal" && full.injectTarget !== "compose") {
		warnings.push(`Prompt "${full.id}" has invalid injectTarget "${full.injectTarget}", removing`);
		full.injectTarget = undefined;
	}
	if (full.preferredAgent && !AGENT_TYPES.includes(full.preferredAgent)) {
		warnings.push(`Prompt "${full.id}" has invalid preferredAgent "${full.preferredAgent}", removing`);
		delete full.preferredAgent;
	}
	if (full.placement && !Array.isArray(full.placement)) {
		full.placement = undefined;
	}

	let migratedPlacement = false;
	if (Array.isArray(full.placement) && full.placement.some((p) => (p as string) === "tab-context")) {
		full.placement = full.placement.map((p) =>
			(p as string) === "tab-context" ? "terminal-context" : p,
		) as SmartPlacement[];
		migratedPlacement = true;
	}

	return { prompt: full, warnings, migratedPlacement };
}
