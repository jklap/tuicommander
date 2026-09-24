import { createSignal } from "solid-js";
import { invoke } from "../invoke";
import { appLogger } from "./appLogger";
import { uiStore } from "./ui";

/** Response of `get_config_defaults`: each config domain's Rust `Default`, in
 * its serialized (snake_case) shape. `dictation` is absent outside desktop. */
export interface ConfigDefaults {
	app: Record<string, unknown>;
	notifications: Record<string, unknown>;
	agent_settings: Record<string, unknown>;
	dictation?: Record<string, unknown>;
}

/**
 * Visibility of Settings "expert" controls.
 *
 * ## configKey convention
 *
 * `<domain>.<field>[.<field>…]` — the domain is a top-level key of
 * `ConfigDefaults` (`app`, `notifications`, `agent_settings`, `dictation`), the
 * rest is the serialized snake_case path inside it, exactly as the Rust config
 * file spells it: `app.osc52_clipboard`, `app.services.auth.session_token_duration_secs`.
 * The value compared against it must use the same serialized shape.
 *
 * Every lookup that cannot answer — defaults still loading, the call failed,
 * the domain is absent on this host, the path does not exist — counts as
 * "not at default", so the control stays visible. Hiding a user's override
 * because of a race or a typo would be the one unacceptable failure.
 */
function createSettingsExpertStore() {
	const [defaults, setDefaults] = createSignal<ConfigDefaults | null>(null);
	/** configKeys a search result opened during the current Settings open */
	const [revealed, setRevealed] = createSignal<ReadonlySet<string>>(new Set());
	/** configKeys shown with a known non-default value during the current open.
	 * Plain Set, not a signal: a key only joins while it is already visible, so
	 * membership never has to trigger a re-render on its own. */
	const modifiedThisOpen = new Set<string>();
	let loadSeq = 0;
	const warnedKeys = new Set<string>();

	function lookup(configKey: string): { found: boolean; value?: unknown } {
		let node: unknown = defaults();
		for (const part of configKey.split(".")) {
			if (node === null || typeof node !== "object" || !(part in node)) return { found: false };
			node = (node as Record<string, unknown>)[part];
		}
		return { found: true, value: node };
	}

	/** True only when defaults are known and `value` equals the default. */
	function isAtDefault(configKey: string, value: unknown): boolean {
		const loaded = defaults();
		if (!loaded) return false;
		const { found, value: fallback } = lookup(configKey);
		if (!found) {
			const domain = configKey.split(".")[0];
			// An absent domain is expected (dictation on a non-desktop host); a
			// missing path inside a present domain is a wrong configKey.
			if (domain in loaded && !warnedKeys.has(configKey)) {
				warnedKeys.add(configKey);
				appLogger.warn("config", `Expert setting "${configKey}" has no config default`);
			}
			return false;
		}
		return deepEqual(value, fallback);
	}

	return {
		/** Start a Settings open: forget reveals and refresh the defaults. The
		 * previous defaults stay in use meanwhile — they are compile-time Rust
		 * values, and clearing them would flash every expert control on open. */
		async open(): Promise<void> {
			setRevealed(new Set<string>());
			modifiedThisOpen.clear();
			const seq = ++loadSeq;
			try {
				const loaded = await invoke<ConfigDefaults>("get_config_defaults");
				if (seq === loadSeq) setDefaults(loaded);
			} catch (err) {
				appLogger.warn("config", "Failed to load config defaults; expert settings stay visible", err);
			}
		},

		/** Show an expert control for the rest of this Settings open, without
		 * touching the persisted expert-mode pref. */
		reveal(configKey: string): void {
			setRevealed((prev) => new Set(prev).add(configKey));
		},

		isAtDefault,

		/** The single visibility rule for an expert control. A control shown
		 * because its value was modified stays shown until the next open(), so
		 * resetting it to the default does not remove it under the cursor. */
		isVisible(configKey: string, value: unknown): boolean {
			if (uiStore.state.settingsExpertMode || revealed().has(configKey)) return true;
			if (modifiedThisOpen.has(configKey)) return true;
			if (isAtDefault(configKey, value)) return false;
			// Only a known difference pins: "defaults still loading" must not
			// keep every expert control on screen for the whole open.
			if (defaults() && lookup(configKey).found) modifiedThisOpen.add(configKey);
			return true;
		},

		/** Exposed for tests only — do not use in production code. */
		_resetForTests(): void {
			loadSeq++;
			setDefaults(null);
			setRevealed(new Set<string>());
			modifiedThisOpen.clear();
			warnedKeys.clear();
		},
	};
}

/** Structural equality for JSON-shaped config values. */
export function deepEqual(a: unknown, b: unknown): boolean {
	if (Object.is(a, b)) return true;
	if (a === null || b === null || typeof a !== "object" || typeof b !== "object") return false;
	if (Array.isArray(a) !== Array.isArray(b)) return false;
	const aKeys = Object.keys(a);
	const bKeys = Object.keys(b);
	if (aKeys.length !== bKeys.length) return false;
	return aKeys.every(
		(key) => key in b && deepEqual((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key]),
	);
}

export const settingsExpertStore = createSettingsExpertStore();
