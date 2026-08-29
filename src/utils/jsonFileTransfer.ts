import { invoke } from "../invoke";
import { toastsStore } from "../stores/toasts";
import { isTauri } from "../transport";
import { downloadText } from "./downloadText";

export interface SaveJsonResult {
	saved: boolean;
	/** Absolute path the file was written to. Only set on the desktop (native Save dialog) path —
	 *  browser mode hands the file to the OS's own download flow with no path to report. */
	path?: string;
}

/**
 * Save arbitrary JSON to disk, letting the user name the file and pick where it goes.
 *
 * Desktop (Tauri): opens the native Save dialog (`@tauri-apps/plugin-dialog`'s `save()`), then
 * writes through the existing `write_external_file` command — already path-validated
 * (`src-tauri/src/fs.rs`'s `validate_external_write_path`: absolute path required, no `..`
 * traversal, parent directory must already exist) and already has full IPC/HTTP parity, so no
 * new backend surface is needed here. `save()` resolves `null` when the user cancels the dialog;
 * that's reported as `{ saved: false }` with no error thrown.
 *
 * Browser mode (`isTauri()` false): the native dialog isn't available, so this falls back to the
 * existing Blob + synthetic `<a download>` flow (`downloadText`) — the user gets the OS's normal
 * "Downloads" behavior instead of a location picker.
 *
 * `@tauri-apps/plugin-dialog` is imported dynamically (matching the pattern already used at
 * `src/plugins/pluginRegistry.ts` and `UpstreamMcpPanel.tsx`) so the browser-mode path never pulls
 * the plugin in, and so the shared test mock (`src/__tests__/mocks/tauri.ts`, which is a static
 * `vi.mock` factory) is honored rather than bypassed by a hoisted static import.
 */
export async function saveJsonFile(suggestedFilename: string, value: unknown, title: string): Promise<SaveJsonResult> {
	const json = JSON.stringify(value, null, 2);

	if (!isTauri()) {
		downloadText(suggestedFilename, json, "application/json");
		return { saved: true };
	}

	const { save } = await import("@tauri-apps/plugin-dialog");
	const target = await save({
		title,
		defaultPath: suggestedFilename,
		filters: [{ name: "JSON", extensions: ["json"] }],
	});
	if (typeof target !== "string") return { saved: false };

	await invoke("write_external_file", { path: target, content: json });
	return { saved: true, path: target };
}

/**
 * `saveJsonFile` plus the success/failure toast every export caller wants. Never toasts on
 * cancel — dismissing the OS dialog isn't an error.
 */
export async function exportJsonWithToast(
	suggestedFilename: string,
	value: unknown,
	title: string,
	successTitle: string,
): Promise<void> {
	try {
		const result = await saveJsonFile(suggestedFilename, value, title);
		if (!result.saved) return;
		toastsStore.add(successTitle, result.path ?? suggestedFilename, "info");
	} catch (error) {
		toastsStore.add("Export failed", String(error), "error");
	}
}

/**
 * Open a native file picker restricted to `.json` and resolve with the selected file's text.
 * Pure DOM (`<input type="file">`) — no Tauri dialog plugin needed, so this works identically
 * in browser mode and the desktop webview.
 *
 * There is no native "cancel" event for `<input type="file">`, so dismissing the OS picker
 * without choosing a file never fires `onchange` — this promise then simply never settles.
 * Callers must not gate a busy/loading indicator on it; treat it as fire-and-forget.
 */
export function pickJsonImportFile(): Promise<string | null> {
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
