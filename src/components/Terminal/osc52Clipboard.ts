import { appLogger } from "../../stores/appLogger";
import { toastsStore } from "../../stores/toasts";
import { writeClipboard } from "../../utils/clipboard";

/**
 * Handle an OSC 52 clipboard-store escape sequence: write to the system
 * clipboard, then toast only once the write actually succeeds — a rejected
 * write (e.g. clipboard permission denied in browser mode) must not claim
 * success to the user.
 */
export function handleOsc52ClipboardStore(text: string, terminalName: string): void {
	writeClipboard(text)
		.then(() => toastsStore.add("Clipboard updated", `by ${terminalName}`, "info"))
		.catch((err) => appLogger.error("terminal", "Failed to write OSC 52 clipboard update", err));
}
