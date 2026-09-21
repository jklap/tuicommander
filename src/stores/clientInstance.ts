/**
 * A stable id for THIS client (this browser profile/app install, or this
 * desktop app instance), persisted in `localStorage` so it survives reloads.
 *
 * Two independent features key state per-client with this id, at two
 * different granularities:
 *
 * - B.5 (`savedTerminalsByClient`, `stores/repositories.ts`): restart-recovery
 *   snapshots keyed one-per-install, so two clients' snapshots merge instead
 *   of clobbering each other. Use `CLIENT_INSTANCE_ID` directly.
 * - B.8 (per-viewer `session_visibility`): two desktop windows/panels of the
 *   SAME install are genuinely independent viewers for standby purposes —
 *   suffix with a window label (`${CLIENT_INSTANCE_ID}:${windowLabel}`)
 *   rather than using the bare id.
 *
 * `localStorage` can throw (private window, cleared/blocked site data) or come
 * back empty — every access is wrapped so a missing/broken store degrades to
 * "mint a fresh id every load" rather than throwing.
 */

import { randomId } from "../utils/randomId";

const STORAGE_KEY = "tuic.clientInstanceId";

function readStoredId(): string | null {
	try {
		return localStorage.getItem(STORAGE_KEY);
	} catch {
		return null;
	}
}

function writeStoredId(id: string): void {
	try {
		localStorage.setItem(STORAGE_KEY, id);
	} catch {
		// Best-effort only — a client that can't persist just mints a new id
		// next load, which is a correctness no-op for both B.5 (that client's
		// own snapshot key just changes) and B.8 (that client's viewer entry
		// simply re-registers under a new id, which the TTL already tolerates).
	}
}

/** Exported for tests only — lets a test exercise the resolution logic
 *  directly against a mocked `localStorage` without depending on how a
 *  bundler/test-runner caches a module's top-level side effect across
 *  repeated dynamic `import()` calls. */
export function resolveClientInstanceId(): string {
	const existing = readStoredId();
	if (existing) return existing;
	// No prefix: callers that key a wire payload on this value (B.8's
	// `viewer_id`) should not have to strip one first.
	const fresh = randomId("");
	writeStoredId(fresh);
	return fresh;
}

/** Stable per-client-install id. Resolved once at module load — every call
 *  site in one page load sees the same value, by construction. */
export const CLIENT_INSTANCE_ID: string = resolveClientInstanceId();
