import { terminalsStore } from "../stores/terminals";

/**
 * Switch the active terminal to the one whose PTY matches `sessionId`.
 * No-op if no terminal owns that session.
 */
export function switchToTerminalBySession(sessionId: string): void {
	for (const id of terminalsStore.getIds()) {
		const t = terminalsStore.get(id);
		if (t?.sessionId === sessionId) {
			terminalsStore.setActive(id);
			return;
		}
	}
}
