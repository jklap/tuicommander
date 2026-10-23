/**
 * A stable id for THIS client (this browser profile/app install, or this
 * desktop app instance), persisted in `localStorage` so it survives reloads.
 *
 * Used by B.5 (`savedTerminalsByClient`, `stores/repositories.ts`):
 * restart-recovery snapshots keyed one-per-install, so two clients' snapshots
 * merge instead of clobbering each other. Standby visibility does not use
 * it: the backend's `session_visibility` is keyed per session, not per viewer.
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
		// next load, which is a correctness no-op for B.5 (that client's own
		// snapshot key just changes).
	}
}

/** Exported for tests only — lets a test exercise the resolution logic
 *  directly against a mocked `localStorage` without depending on how a
 *  bundler/test-runner caches a module's top-level side effect across
 *  repeated dynamic `import()` calls. */
export function resolveClientInstanceId(): string {
	const existing = readStoredId();
	if (existing) return existing;
	// No prefix: the id is used verbatim as a persisted map key
	// (`savedTerminalsByClient`), so callers never have to strip one.
	const fresh = randomId("");
	writeStoredId(fresh);
	return fresh;
}

/** Stable per-client-install id. Resolved once at module load — every call
 *  site in one page load sees the same value, by construction. */
export const CLIENT_INSTANCE_ID: string = resolveClientInstanceId();
