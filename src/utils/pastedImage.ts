import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";

const IMAGE_EXTENSIONS = new Map([
	["image/png", "png"],
	["image/jpeg", "jpg"],
	["image/webp", "webp"],
	["image/gif", "gif"],
]);

/** Convert a Blob to a base64 string */
async function blobToBase64(blob: Blob): Promise<string> {
	const buffer = await blob.arrayBuffer();
	const bytes = new Uint8Array(buffer);
	let binary = "";
	for (const byte of bytes) binary += String.fromCharCode(byte);
	return btoa(binary);
}

/** Save the first accepted image on a paste event and return its path.
 *
 *  Returns null when the paste carries no accepted image (the default text paste
 *  proceeds) or when saving failed (logged). The default action is cancelled
 *  synchronously, before the first await, so the caller may rely on
 *  `event.defaultPrevented` to know the paste was claimed.
 *  `getNoteId` is only called once an image is found: it names the asset
 *  directory (`note-images/<id>/`) on the backend. */
export async function savePastedImage(e: ClipboardEvent, getNoteId: () => string): Promise<string | null> {
	const items = e.clipboardData?.items;
	if (!items) return null;

	for (const item of items) {
		const extension = IMAGE_EXTENSIONS.get(item.type);
		if (!extension) continue;
		const blob = item.getAsFile();
		// An image item without a file must not swallow the rest of the paste.
		if (!blob) continue;
		e.preventDefault();

		const noteId = getNoteId();
		try {
			const dataBase64 = await blobToBase64(blob);
			// `noteId` is the backend argument name and the on-disk asset
			// directory (`note-images/<id>/`). It keeps the old vocabulary
			// on purpose — see the boundary note in `stores/ideas.ts`.
			return await invoke<string>("save_note_image", {
				noteId,
				dataBase64,
				extension,
			});
		} catch (err) {
			appLogger.error("store", "Failed to save pasted image", err);
			return null;
		}
	}
	return null;
}
