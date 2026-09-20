/**
 * Where a turn is written and stopped.
 *
 * Enter sends and Shift+Enter breaks the line, which is what every chat in this
 * app does. While a turn is running the same corner holds Stop instead of Send:
 * the next thing a person wants during a turn they regret is not a second turn.
 */

import { type Component, Show } from "solid-js";
import s from "./AIChatPanel.module.css";
import { aiChatDraft } from "./draft";
import type { AcpChat } from "./useAcpChat";

export const Composer: Component<{ chat: AcpChat }> = (props) => {
	const send = () => {
		const text = aiChatDraft.text();
		if (!text.trim() || props.chat.busy()) return;
		aiChatDraft.clear();
		void props.chat.send(text);
	};

	const onKeyDown = (event: KeyboardEvent) => {
		if (event.key !== "Enter" || event.shiftKey) return;
		event.preventDefault();
		send();
	};

	return (
		<div class={s.inputArea}>
			<textarea
				class={s.textarea}
				placeholder="Ask ego about this repository"
				value={aiChatDraft.text()}
				onInput={(event) => aiChatDraft.set(event.currentTarget.value)}
				onKeyDown={onKeyDown}
				rows={1}
			/>
			<Show
				when={props.chat.busy()}
				fallback={
					<button type="button" class={s.sendBtn} disabled={!aiChatDraft.text().trim()} onClick={send}>
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
