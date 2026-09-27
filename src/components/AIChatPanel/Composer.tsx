/**
 * Where a turn is written and stopped.
 *
 * Enter sends and Shift+Enter breaks the line, which is what every chat in this
 * app does. While a turn is running the same corner holds Stop instead of Send:
 * the next thing a person wants during a turn they regret is not a second turn.
 */

import { type Component, createSignal, For, Show } from "solid-js";
import s from "./AIChatPanel.module.css";
import { aiChatDraft } from "./draft";
import type { AcpChat } from "./useAcpChat";

export const Composer: Component<{ chat: AcpChat }> = (props) => {
	const [pasteError, setPasteError] = createSignal<string | null>(null);
	const send = () => {
		const text = aiChatDraft.text();
		const images = aiChatDraft.images().map((image) => image.block);
		if ((!text.trim() && images.length === 0) || props.chat.busy()) return;
		aiChatDraft.clear();
		setPasteError(null);
		void props.chat.send(text, images);
	};

	const onPaste = (event: ClipboardEvent) => {
		// DEFERRED (2026-09-28) — image drop needs Boss's explicit approval for drag/drop handlers.
		const files = [...(event.clipboardData?.items ?? [])]
			.filter((item) => item.type.startsWith("image/"))
			.map((item) => item.getAsFile())
			.filter((file): file is File => file !== null);
		if (files.length === 0) return;
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
		if (event.key !== "Enter" || event.shiftKey) return;
		event.preventDefault();
		send();
	};

	return (
		<div class={s.inputArea}>
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
				<Show when={pasteError()}>
					<span class={s.pasteError} role="alert">
						{pasteError()}
					</span>
				</Show>
				<textarea
					class={s.textarea}
					placeholder="Ask ego about this repository"
					value={aiChatDraft.text()}
					onInput={(event) => aiChatDraft.set(event.currentTarget.value)}
					onKeyDown={onKeyDown}
					onPaste={onPaste}
					rows={1}
				/>
			</div>
			<Show
				when={props.chat.busy()}
				fallback={
					<button
						type="button"
						class={s.sendBtn}
						disabled={!aiChatDraft.text().trim() && aiChatDraft.images().length === 0}
						onClick={send}
					>
						Send
					</button>
				}
			>
				<button type="button" class={s.stopBtn} onClick={() => void props.chat.cancel()}>
					Stop
				</button>
			</Show>
		</div>
	);
};
