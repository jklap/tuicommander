import { type Component, Show } from "solid-js";
import { getModifierSymbol } from "../../platform";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import s from "./DiffTab.module.css";

export interface CommentBoxProps {
	value: string;
	error: string | null;
	onInput: (value: string) => void;
	onSend: () => void;
	onCancel: () => void;
	setTextareaRef?: (el: HTMLTextAreaElement) => void;
}

/** The comment textarea + Cmd/Ctrl+Enter-to-send / Escape-to-cancel UX,
 *  shared by `DiffTab` and `SessionDiffTab`'s "comment on selected lines"
 *  action. */
export const CommentBox: Component<CommentBoxProps> = (props) => {
	function handleKeyDown(e: KeyboardEvent) {
		if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
			e.preventDefault();
			props.onSend();
		}
		if (e.key === "Escape") {
			e.preventDefault();
			props.onCancel();
		}
	}

	return (
		<div class={s.commentBox}>
			<Textarea
				ref={(el) => props.setTextareaRef?.(el)}
				class={s.commentTextarea}
				placeholder="Write a comment about the selected lines..."
				value={props.value}
				onInput={(e) => props.onInput(e.currentTarget.value)}
				onKeyDown={handleKeyDown}
				rows={3}
			/>
			<Show when={props.error}>
				<div class={s.commentErrorMsg}>{props.error}</div>
			</Show>
			<div class={s.commentActions}>
				<span class={s.commentHint}>{getModifierSymbol()}+Enter to send</span>
				<Button variant="ghost" onClick={props.onCancel}>
					Cancel
				</Button>
				<Button variant="primary" disabled={!props.value.trim()} onClick={props.onSend}>
					Send
				</Button>
			</div>
		</div>
	);
};
