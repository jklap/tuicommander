import { subscribeEvents, type Unsubscribe } from "../transport";
import { handleOpenUrl } from "../utils/openUrl";

interface PtyOpenUrlPayload {
	session_id: string;
	url: string;
}

/**
 * Listen for OSC 1337 `OpenURL` requests the backend already confirmed with
 * the human (via the same `mcp-confirm` flow `ui(action=confirm)` uses — see
 * `confirm_open_url` in `mcp_http/mod.rs`). By the time this event fires the
 * answer was already "yes"; this just performs the open.
 *
 * Every client subscribes — desktop, browser and mobile PWA alike — the same
 * way `subscribeMcpConfirm` does, so whichever client answered "yes" (or any
 * other connected one) can open it in its own browser.
 */
export function subscribePtyOpenUrl(): Promise<Unsubscribe> {
	return subscribeEvents({
		"pty-open-url": (payload) => handleOpenUrl((payload as PtyOpenUrlPayload).url),
	});
}
