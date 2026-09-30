/**
 * Where a turn is written and stopped.
 *
 * Enter sends and Shift+Enter breaks the line, which is what every chat in this
 * app does. While a turn is running the same corner holds Stop instead of Send:
 * the next thing a person wants during a turn they regret is not a second turn.
 */

import { type Component, createEffect, createSignal, For, Show } from "solid-js";
import { uploadAttachment } from "../../services/uploadAttachment";
import s from "./AIChatPanel.module.css";
import { aiChatDraft } from "./draft";
import type { AcpChat } from "./useAcpChat";

export const Composer: Component<{
	chat: AcpChat;
	mobileAttachments?: boolean;
	sharedFile?: File | null;
	onSharedFileConsumed?: () => void;
}> = (props) => {
	const [pasteError, setPasteError] = createSignal<string | null>(null);
	const [uploading, setUploading] = createSignal(false);
	let textarea: HTMLTextAreaElement | undefined;
	let fileInput: HTMLInputElement | undefined;
	const hasContent = () =>
		!!aiChatDraft.text().trim() || aiChatDraft.images().length > 0 || aiChatDraft.files().length > 0;
	const parkLabel = () =>
		!aiChatDraft.parked() ? "Park draft" : hasContent() ? "Swap parked draft" : "Restore parked draft";
	createEffect(() => aiChatDraft.activate(props.chat.sessionId() ?? ""));
	const resize = () => {
		if (!textarea) return;
		textarea.style.height = "auto";
		textarea.style.height = `${Math.min(Math.max(textarea.scrollHeight, 36), 150)}px`;
	};
	createEffect(() => {
		aiChatDraft.text();
		queueMicrotask(resize);
	});
	const send = () => {
		const text = aiChatDraft.expandedText();
		const images = aiChatDraft.images().map((image) => image.block);
		const files = aiChatDraft.files();
		if (!text.trim() && images.length === 0 && files.length === 0) return;
		aiChatDraft.restoreAfterSend();
		queueMicrotask(resize);
		setPasteError(null);
		void props.chat.send(text, images, files);
	};

	const attachFile = async (file: File) => {
		setPasteError(null);
		if (file.type.startsWith("image/")) {
			setPasteError(await aiChatDraft.stageImage(file, props.chat.capabilities()?.promptImage === true));
			return;
		}
		setUploading(true);
		try {
			await props.chat.ensureStarted();
			const connectionId = props.chat.connectionId();
			if (!connectionId) throw new Error("AI Chat is not connected");
			const receipt = await uploadAttachment(file, { kind: "acp", id: connectionId });
			aiChatDraft.stageFile({ name: file.name, path: receipt.path });
		} catch (error) {
			setPasteError(error instanceof Error ? error.message : String(error));
		} finally {
			setUploading(false);
			if (fileInput) fileInput.value = "";
		}
	};
	createEffect(() => {
		const shared = props.sharedFile;
		if (!shared) return;
		props.onSharedFileConsumed?.();
		void attachFile(shared);
	});

	const onPaste = (event: ClipboardEvent) => {
		// DEFERRED (2026-09-28) — image drop needs Boss's explicit approval for drag/drop handlers.
		const files = [...(event.clipboardData?.items ?? [])]
			.filter((item) => item.type.startsWith("image/"))
			.map((item) => item.getAsFile())
			.filter((file): file is File => file !== null);
		if (files.length === 0) {
			const value = event.clipboardData?.getData("text/plain") ?? "";
			if (!textarea) return;
			const cursor = aiChatDraft.stageTextPaste(value, textarea.selectionStart, textarea.selectionEnd);
			if (cursor === null) return;
			event.preventDefault();
			queueMicrotask(() => {
				textarea?.setSelectionRange(cursor, cursor);
				resize();
			});
			return;
		}
		event.preventDefault();
		setPasteError(null);
		void (async () => {
			for (const file of files) {
				const error = await aiChatDraft.stageImage(file, props.chat.capabilities()?.promptImage === true);
				if (error) {
					setPasteError(error);
					break;
				}
			}
		})();
	};

	const onKeyDown = (event: KeyboardEvent) => {
		if (event.key.toLowerCase() === "s" && event.ctrlKey && !event.metaKey && !event.altKey) {
			event.preventDefault();
			if (event.repeat) return;
			aiChatDraft.parkOrSwap();
			queueMicrotask(resize);
			return;
		}
		if (event.key !== "Enter" || event.shiftKey) return;
		event.preventDefault();
		send();
	};

	return (
		<div class={s.inputArea}>
			<Show when={props.mobileAttachments}>
				<input
					ref={fileInput}
					type="file"
					accept="image/*,application/pdf,text/*"
					aria-label="Choose attachment"
					hidden
					onChange={(event) => {
						const file = event.currentTarget.files?.[0];
						if (file) void attachFile(file);
					}}
				/>
				<button
					type="button"
					class={s.attachBtn}
					aria-label="Attach file"
					disabled={uploading()}
					onClick={() => fileInput?.click()}
				>
					<svg
						width="20"
						height="20"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						aria-hidden="true"
					>
						<path d="M20 11.5l-8.6 8.6a5 5 0 0 1-7.1-7.1L13 4.3a3.5 3.5 0 0 1 5 5l-8.7 8.7a2 2 0 0 1-2.8-2.8l8-8" />
					</svg>
				</button>
			</Show>
			<Show when={props.chat.queuedPrompts().length > 0}>
				<div class={s.queueList} aria-label="Queued prompts">
					<div class={s.queueHeader}>
						Queued <span class={s.queueBadge}>{props.chat.queuedPrompts().length}</span>
					</div>
					<For each={props.chat.queuedPrompts()}>
						{(queued) => {
							const label = queued.summary;
							return (
								<div class={s.queueRow}>
									<span class={s.queueText}>{label}</span>
									<button
										type="button"
										class={s.queueCancel}
										aria-label={`Cancel queued prompt ${label}`}
										onClick={() => void props.chat.cancelQueued(queued.turnId)}
									>
										<svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
											<path d="M2.8 2l3.2 3.2L9.2 2l.8.8L6.8 6l3.2 3.2-.8.8L6 6.8 2.8 10l-.8-.8L5.2 6 2 2.8z" />
										</svg>
									</button>
								</div>
							);
						}}
					</For>
				</div>
			</Show>
			<div class={s.inputBody}>
				<Show when={aiChatDraft.images().length > 0}>
					<div class={s.imagePreviews}>
						<For each={aiChatDraft.images()}>
							{(image) => (
								<div class={s.imagePreview}>
									<img src={image.src} alt="Pasted image" />
									<button type="button" aria-label="Remove pasted image" onClick={() => aiChatDraft.removeImage(image)}>
										<svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
											<path d="M2.8 2l3.2 3.2L9.2 2l.8.8L6.8 6l3.2 3.2-.8.8L6 6.8 2.8 10l-.8-.8L5.2 6 2 2.8z" />
										</svg>
									</button>
								</div>
							)}
						</For>
					</div>
				</Show>
				<Show when={aiChatDraft.files().length > 0 || uploading()}>
					<div class={s.filePreviews}>
						<For each={aiChatDraft.files()}>
							{(file) => (
								<span class={s.filePreview}>
									{file.name}
									<button type="button" aria-label={`Remove ${file.name}`} onClick={() => aiChatDraft.removeFile(file)}>
										×
									</button>
								</span>
							)}
						</For>
						<Show when={uploading()}>
							<span class={s.filePreview} role="status">
								Uploading…
							</span>
						</Show>
					</div>
				</Show>
				<Show when={pasteError()}>
					<span class={s.pasteError} role="alert">
						{pasteError()}
					</span>
				</Show>
				<Show when={aiChatDraft.storageError()}>
					<span class={s.pasteError} role="alert">
						Browser storage is unavailable; reload may lose it.
					</span>
				</Show>
				<textarea
					ref={textarea}
					class={s.textarea}
					placeholder="Ask ego about this repository"
					value={aiChatDraft.text()}
					onInput={(event) => {
						aiChatDraft.set(event.currentTarget.value);
						resize();
					}}
					onKeyDown={onKeyDown}
					onPaste={onPaste}
					rows={1}
				/>
			</div>
			<Show when={props.chat.busy()}>
				<button type="button" class={s.stopBtn} onClick={() => void props.chat.cancel()}>
					Stop
				</button>
			</Show>
			<button
				type="button"
				class={s.parkBtn}
				aria-label={parkLabel()}
				disabled={!aiChatDraft.parked() && !hasContent()}
				onClick={() => aiChatDraft.parkOrSwap()}
			>
				{aiChatDraft.parked() ? "Parked draft" : "Park"}
			</button>
			<button
				type="button"
				class={s.sendBtn}
				disabled={!aiChatDraft.text().trim() && aiChatDraft.images().length === 0 && aiChatDraft.files().length === 0}
				onClick={send}
			>
				{props.chat.busy() ? "Queue" : "Send"}
			</button>
		</div>
	);
};
