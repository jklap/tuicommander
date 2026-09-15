import { createEffect, onCleanup, onMount } from "solid-js";
import { listen } from "../invoke";
import type { ActionName } from "../keybindingDefaults";
import { appLogger } from "../stores/appLogger";
import { dispatchAction, type ShortcutHandlers, useKeyboardShortcuts } from "./useKeyboardShortcuts";

/** Re-registers global shortcuts when their reactive keybinding inputs change. */
export function useShortcutRegistration(handlers: ShortcutHandlers): void {
	createEffect(() => {
		const cleanup = useKeyboardShortcuts(handlers);
		onCleanup(cleanup);
	});

	// A hardware controller (StreamDock macropad, etc.) asking the UI to run
	// a named action — see `AppEvent::UiActionRequested`'s doc comment
	// (state.rs) for why this rides the same dispatch table as real keydown
	// events rather than a separate handler list. The Rust side already
	// restricts which names can ever arrive here (`UI_ACTION_ALLOWLIST` in
	// mcp_http/session.rs); `dispatchAction` itself safely no-ops on an
	// unrecognized name regardless.
	onMount(() => {
		listen<{ name: string }>("ui-action-requested", (event) => {
			const ran = dispatchAction(event.payload.name as ActionName, handlers);
			if (!ran) {
				appLogger.warn("app", `ui-action-requested: unrecognized action '${event.payload.name}'`);
			}
		}).catch((err) => appLogger.error("app", "Failed to register ui-action-requested listener", err));
	});
}
