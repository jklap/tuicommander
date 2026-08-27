import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { isTauri } from "../transport";

/**
 * Write text to the system clipboard.
 *
 * Inside the Tauri webview we route through the native clipboard-manager plugin
 * instead of navigator.clipboard. WKWebView rejects navigator.clipboard.writeText
 * with NotAllowedError whenever the document isn't focused or the transient user
 * activation has already been consumed by an intervening await — exactly what the
 * terminal copy paths do (they await an IPC round-trip to fetch the selection text
 * before writing). The native command has no focus / user-gesture requirement.
 *
 * Browser mode (the app served over plain HTTP/TLS for remote/Tailscale access,
 * with no Tauri IPC bridge) has no such plugin command, so it uses navigator.clipboard —
 * which a browser can reject for reasons the native path never hits: insecure origin,
 * denied Permissions Policy, or the async clipboard permission simply not having been
 * granted yet. If that happens, fall back to the legacy execCommand("copy") path, which
 * only needs a live selection rather than the async Clipboard permission.
 *
 * Throws on failure so callers can surface a "copy failed" status.
 */
export async function writeClipboard(text: string): Promise<void> {
	if (isTauri()) {
		await invoke("plugin:clipboard-manager|write_text", { text, label: undefined });
		return;
	}
	try {
		await navigator.clipboard.writeText(text);
	} catch (err) {
		if (!execCommandCopyFallback(text)) {
			throw err;
		}
	}
}

/** Legacy selection-based copy fallback for browser contexts where the async
 *  Clipboard API is unavailable or denied. Returns whether the copy succeeded.
 *
 * Focusing the textarea (required for execCommand("copy") to see a selection)
 * steals focus from whatever the user was previously on — e.g. a terminal, which
 * would otherwise silently stop receiving keyboard input. Both the DOM cleanup
 * and the focus restore live in `finally` so a throw from focus()/select() (or
 * an unfocusable previouslyFocused element) can't leak the textarea or leave
 * focus stuck on it.
 */
function execCommandCopyFallback(text: string): boolean {
	const previouslyFocused = document.activeElement as HTMLElement | null;
	const textarea = document.createElement("textarea");
	textarea.value = text;
	// Keep it out of the visible layout and off-screen without affecting scroll.
	textarea.style.position = "fixed";
	textarea.style.top = "0";
	textarea.style.left = "0";
	textarea.style.opacity = "0";
	document.body.appendChild(textarea);
	try {
		textarea.focus();
		textarea.select();
		// .select() alone is unreliable on WebKit (notably iOS Safari, and some desktop
		// Safari builds) — it doesn't always leave a real selection range behind, which
		// makes execCommand("copy") silently return false with nothing copied.
		// setSelectionRange is the standard belt-and-suspenders fix for that.
		textarea.setSelectionRange(0, textarea.value.length);
		// execCommand is deprecated but remains the only selection-based copy path
		// browsers still support; it's the fallback of last resort here.
		return document.execCommand("copy");
	} catch {
		return false;
	} finally {
		document.body.removeChild(textarea);
		previouslyFocused?.focus();
	}
}

/**
 * Read text from the system clipboard.
 *
 * Inside the Tauri webview we route through the native clipboard-manager plugin
 * instead of navigator.clipboard.readText(). On macOS (Sequoia+), the WKWebView
 * Web Clipboard read API surfaces a system "Paste" confirmation pill floating over
 * the page — visually colliding with our own context menu's Paste item. The native
 * command reads the pasteboard directly with no such affordance.
 *
 * Browser mode has no such plugin command, so it keeps navigator.clipboard.
 */
export async function readClipboard(): Promise<string> {
	if (isTauri()) {
		return await invoke<string>("plugin:clipboard-manager|read_text");
	}
	return await navigator.clipboard.readText();
}

/** Shared "Copy Path" context-menu action: copy an already-formatted path to the
 *  clipboard, logging (not throwing) on failure — there is no success/failure UI
 *  at these call sites, so a rejection has nowhere to surface but the log. */
export function copyPathToClipboard(path: string): void {
	writeClipboard(path).catch((err) => appLogger.error("app", "Failed to copy path", err));
}
