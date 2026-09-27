import { createSignal } from "solid-js";
import type { AcpContentBlock } from "../../types/acp";

export const MAX_PASTED_IMAGE_BYTES = 10 * 1024 * 1024;
const IMAGE_TYPES = new Set(["image/png", "image/jpeg", "image/gif", "image/webp"]);

export interface StagedImage {
	src: string;
	size: number;
	block: Extract<AcpContentBlock, { type: "image" }>;
}

/**
 * What the person has typed but not sent yet.
 *
 * Module scope rather than component state so the panel can be closed and
 * reopened without losing a half-written question, and so something outside the
 * panel — the terminal context menu — can hand it a selection to ask about.
 */
const [text, setText] = createSignal("");
const [images, setImages] = createSignal<StagedImage[]>([]);
let revision = 0;
let pendingBytes = 0;

function readImage(file: File): Promise<string> {
	return new Promise((resolve, reject) => {
		const reader = new FileReader();
		reader.onload = () => resolve(String(reader.result));
		reader.onerror = () => reject(reader.error ?? new Error("Could not read image"));
		reader.readAsDataURL(file);
	});
}

export const aiChatDraft = {
	text,
	set: setText,
	images,

	/** Validate bytes before FileReader expands them into base64. */
	async stageImage(file: File, supported: boolean): Promise<string | null> {
		const stagedAt = revision;
		if (!supported) return "This agent does not support images.";
		if (!IMAGE_TYPES.has(file.type)) return `Unsupported image type: ${file.type || "unknown"}.`;
		if (file.size > MAX_PASTED_IMAGE_BYTES) {
			return `Image is ${(file.size / (1024 * 1024)).toFixed(1)} MiB; the limit is 10 MiB.`;
		}
		if (images().reduce((sum, image) => sum + image.size, pendingBytes) + file.size > MAX_PASTED_IMAGE_BYTES) {
			return "Images exceed the 10 MiB total limit.";
		}
		pendingBytes += file.size;
		try {
			const src = await readImage(file);
			if (stagedAt !== revision) return null;
			const data = src.slice(src.indexOf(",") + 1);
			setImages((current) => [
				...current,
				{ src, size: file.size, block: { type: "image", mimeType: file.type, data } },
			]);
			return null;
		} catch {
			return "Could not read image from clipboard.";
		} finally {
			pendingBytes -= file.size;
		}
	},

	removeImage(image: StagedImage): void {
		setImages((current) => current.filter((item) => item !== image));
	},

	/** Add text to the draft and leave the cursor after it. */
	append(addition: string): void {
		const current = text();
		setText(current ? `${current.replace(/\s+$/, "")}\n\n${addition}` : addition);
	},

	clear(): void {
		revision += 1;
		setText("");
		setImages([]);
	},
};
