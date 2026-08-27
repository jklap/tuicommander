import type { PromptExportFile } from "./promptExport";

/**
 * Trigger a browser download of a Smart Prompts export file. Reuses the same pure-DOM
 * Blob + synthetic `<a download>` approach as `downloadText` (see that file's comment) —
 * it works in both browser mode and the Tauri webview with no native dialog dependency.
 */
export function downloadPromptExport(file: PromptExportFile, filename: string): void {
	const json = JSON.stringify(file, null, 2);
	const blob = new Blob([json], { type: "application/json" });
	const url = URL.createObjectURL(blob);
	const anchor = document.createElement("a");
	anchor.href = url;
	anchor.download = filename;
	anchor.click();
	URL.revokeObjectURL(url);
}

/**
 * Open a native file picker restricted to `.json` and resolve with the selected file's text.
 * Pure DOM (`<input type="file">`) — no Tauri dialog plugin needed, so this works identically
 * in browser mode and the desktop webview. Mirrors the pattern already used for
 * dictation-corrections import (`DictationSettings.tsx`).
 *
 * There is no native "cancel" event for `<input type="file">`, so dismissing the OS picker
 * without choosing a file never fires `onchange` — this promise then simply never settles.
 * Callers must not gate a busy/loading indicator on it; treat it as fire-and-forget, exactly
 * like `DictationSettings`' import button does today.
 */
export function pickPromptImportFile(): Promise<string | null> {
	return new Promise((resolve) => {
		const input = document.createElement("input");
		input.type = "file";
		input.accept = ".json";
		input.onchange = async () => {
			const file = input.files?.[0];
			resolve(file ? await file.text() : null);
		};
		input.click();
	});
}
