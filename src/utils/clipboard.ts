import { invoke } from "../invoke";
import { shortenHomePath } from "../platform";
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

/**
 * Write text to the clipboard where the text itself isn't known synchronously —
 * e.g. it needs an HTTP round-trip (terminal_get_selection_text, getBufferLines)
 * to fetch a server-cleaned version (wrap-unwrapped, Claude quote-gutters stripped;
 * see docs/backend/pty.md) of what a synchronous local read could produce instead.
 *
 * In Tauri mode this is just `writeClipboard(await textPromise)` — the native
 * clipboard-manager plugin has no user-gesture requirement, so awaiting first is
 * harmless (see writeClipboard's doc comment).
 *
 * In browser mode, awaiting `textPromise` before calling into the Clipboard API
 * would reintroduce exactly the bug this function exists to avoid: the awaited
 * round-trip can outlast the browser's user-activation window over a slow/remote
 * connection (Tailscale, not just localhost), silently breaking both
 * navigator.clipboard.writeText and the execCommand('copy') fallback. Instead,
 * `navigator.clipboard.write()` is called SYNCHRONOUSLY (satisfying the
 * activation requirement immediately) with a `ClipboardItem` whose data is
 * `textPromise` itself — per spec, a ClipboardItem's data may be a Promise
 * that resolves after the call, which is the standard pattern for exactly this
 * "the real data isn't ready yet" situation and is supported by Chrome, Firefox,
 * and Safari. If `ClipboardItem`/`navigator.clipboard.write` aren't available
 * (older browsers) or the write is denied outright, falls back to
 * `writeClipboard(await textPromise)` — which does reintroduce the
 * activation-window risk, but only for that narrower band, not every copy.
 *
 * Throws on failure so callers can surface a "copy failed" status, same as
 * writeClipboard.
 */
export async function writeClipboardAsync(textPromise: Promise<string>): Promise<void> {
	if (isTauri()) {
		await writeClipboard(await textPromise);
		return;
	}
	if (typeof ClipboardItem !== "undefined" && typeof navigator.clipboard?.write === "function") {
		// A derived promise, separate from textPromise itself and from write()'s own
		// returned promise (the only one the try/catch below observes). If write()
		// rejects for a reason unrelated to ever reading the ClipboardItem's data (e.g.
		// permission denied outright, checked before touching it), this promise is left
		// with no other handler — attach a no-op one so a later textPromise rejection
		// can't surface as an unhandled rejection independent of the catch below.
		const blobPromise = textPromise.then((text) => new Blob([text], { type: "text/plain" }));
		blobPromise.catch(() => {});
		try {
			await navigator.clipboard.write([new ClipboardItem({ "text/plain": blobPromise })]);
			return;
		} catch {
			// Unsupported for this data, denied outright, or some other failure
			// unrelated to timing — fall through to the synchronous fallback below.
		}
	}
	await writeClipboard(await textPromise);
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
 * Copy a filesystem path to the clipboard, logging a failure instead of throwing.
 *
 * The path is ABSOLUTE and home-shortened to `~/…`. Both halves are load-bearing:
 * a relative path resolves against whatever cwd the consumer happens to have, and
 * `~` is what Boss wants to read back. Every "Copy Path" action routes through
 * here so the two rules hold in one place — callers pass the full path and do not
 * shorten it themselves.
 */
export function copyPathToClipboard(absolutePath: string): void {
	writeClipboard(shortenHomePath(absolutePath)).catch((err) => appLogger.error("app", "Failed to copy path", err));
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
