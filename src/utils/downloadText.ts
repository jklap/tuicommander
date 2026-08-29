/**
 * Trigger a browser download of a text file.
 *
 * Works in both browser mode and the Tauri webview (WKWebView honors the
 * synthetic `<a download>` click + object URL). Pure DOM — no Tauri plugin
 * dependency, so callers don't need a fs-save permission.
 */
export function downloadText(filename: string, text: string, mimeType = "text/plain;charset=utf-8"): void {
	const blob = new Blob([text], { type: mimeType });
	const url = URL.createObjectURL(blob);
	const anchor = document.createElement("a");
	anchor.href = url;
	anchor.download = filename;
	anchor.click();
	URL.revokeObjectURL(url);
}
