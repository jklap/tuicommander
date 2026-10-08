/**
 * Palette actions whose bodies are not a plain `ShortcutHandlers` callback.
 * Shared by the Command Palette registry (`actionRegistry.ts`) and the
 * keybinding dispatcher (`useKeyboardShortcuts.ts`'s `dispatchAction`), so a
 * user-bound key runs exactly what the palette entry runs.
 */
import { stateExplainStore } from "../stores/stateExplain";
import { terminalsStore } from "../stores/terminals";
import { toastsStore } from "../stores/toasts";
import { ptyCaptureStore } from "../utils/ptyCapture";

/** Toggle the PTY diagnostics capture for the active terminal's session. */
export function toggleDiagnosticsCaptureForActive(): void {
	const id = terminalsStore.getActive()?.sessionId;
	if (id) {
		void ptyCaptureStore.toggle(id);
	} else {
		// A freshly-spawned tab (PTY not assigned a sessionId yet) or a
		// non-terminal active tab — ptyCaptureStore.toggle() itself always
		// surfaces failure via a toast, so this no-op needs the same rather
		// than silently doing nothing with no feedback at all.
		toastsStore.add("Diagnostics capture", "No active terminal session to capture.", "warn");
	}
}

/** Open the Explain State view for the active terminal's live session. */
export function explainActiveSessionState(): void {
	const active = terminalsStore.getActive();
	if (active?.sessionId && active.shellState !== "exited") {
		stateExplainStore.open(active.id);
	} else {
		// Mirrors toggleDiagnosticsCaptureForActive's guard: give feedback rather
		// than silently no-op-ing when there's no live active terminal session.
		toastsStore.add("Explain session state", "No active terminal session to explain.", "warn");
	}
}
