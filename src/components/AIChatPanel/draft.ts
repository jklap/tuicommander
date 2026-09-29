import { createSignal } from "solid-js";
import type { AcpContentBlock } from "../../types/acp";
import maxImageBytes from "../../shared/acp-image-limit.json";

export const MAX_PASTED_IMAGE_BYTES = maxImageBytes;
const IMAGE_TYPES = new Set(["image/png", "image/jpeg", "image/gif", "image/webp"]);

export interface StagedImage {
	src: string;
	size: number;
	block: Extract<AcpContentBlock, { type: "image" }>;
}

export interface StagedFile {
	name: string;
	path: string;
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
const [files, setFiles] = createSignal<StagedFile[]>([]);
const drafts = new Map<string, { text: string; images: StagedImage[]; files: StagedFile[] }>();
const pastedText = new Map<string, string>();
let activeSession = "";
let revision = 0;
let pendingBytes = 0;
let pasteNumber = 0;

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
	expandedText(): string {
		return text().replace(/\[Pasted text #\d+ \+\d+ words\]/g, (marker) => pastedText.get(marker) ?? marker);
	},
	stageTextPaste(value: string, start: number, end: number): number | null {
		const words = value.trim().split(/\s+/).filter(Boolean).length;
		if (words <= 200) return null;
		const marker = `[Pasted text #${++pasteNumber} +${words} words]`;
		pastedText.set(marker, value);
		setText((current) => current.slice(0, start) + marker + current.slice(end));
		return start + marker.length;
	},
	images,
	files,
	activate(session: string): void {
		if (session === activeSession) return;
		const previous = { text: text(), images: images(), files: files() };
		if (activeSession) drafts.set(activeSession, previous);
		const next = drafts.get(session) ?? (activeSession === "" && session ? previous : undefined);
		activeSession = session;
		setText(next?.text ?? "");
		setImages(next?.images ?? []);
		setFiles(next?.files ?? []);
		revision += 1;
	},

	/** Validate bytes before FileReader expands them into base64. */
	async stageImage(file: File, supported: boolean): Promise<string | null> {
		const stagedAt = revision;
		if (!supported) return "This agent does not support images.";
		if (!IMAGE_TYPES.has(file.type)) return `Unsupported image type: ${file.type || "unknown"}.`;
		if (file.size > MAX_PASTED_IMAGE_BYTES) {
			return `Image is ${(file.size / (1024 * 1024)).toFixed(1)} MiB; the limit is ${MAX_PASTED_IMAGE_BYTES / (1024 * 1024)} MiB.`;
		}
		if (images().reduce((sum, image) => sum + image.size, pendingBytes) + file.size > MAX_PASTED_IMAGE_BYTES) {
			return `Image is ${(file.size / (1024 * 1024)).toFixed(1)} MiB; the total limit is ${MAX_PASTED_IMAGE_BYTES / (1024 * 1024)} MiB.`;
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

	stageFile(file: StagedFile): void {
		setFiles((current) => [...current, file]);
	},

	removeFile(file: StagedFile): void {
		setFiles((current) => current.filter((item) => item !== file));
	},

	/** Add text to the draft and leave the cursor after it. */
	append(addition: string): void {
		const current = text();
		setText(current ? `${current.replace(/\s+$/, "")}\n\n${addition}` : addition);
	},

	clear(): void {
		revision += 1;
		for (const marker of text().match(/\[Pasted text #\d+ \+\d+ words\]/g) ?? []) pastedText.delete(marker);
		setText("");
		setImages([]);
		setFiles([]);
		drafts.delete(activeSession);
	},

	reset(): void {
		drafts.clear();
		pastedText.clear();
		pasteNumber = 0;
		activeSession = "";
		revision += 1;
		setText("");
		setImages([]);
		setFiles([]);
	},
};
