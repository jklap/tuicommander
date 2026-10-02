import { type Component, For, Show } from "solid-js";
import type { AnswersTurn } from "./answersTurn";

interface AnswersPanelProps {
	view: AnswersTurn;
	fontFamily: string;
	fontSize: number;
}

/**
 * The answers-only view: an opaque layer over the terminal canvas with the user's
 * last prompt and the 💬 answers of its turn, packed together. Plain DOM text, so
 * it is selectable and copyable, and it scrolls when the turn has many answers.
 */
export const AnswersPanel: Component<AnswersPanelProps> = (props) => (
	<div
		data-answers-only
		style={{
			position: "absolute",
			inset: "0",
			"z-index": "30",
			overflow: "auto",
			background: "var(--bg-secondary)",
			color: "var(--fg-primary)",
			"font-family": props.fontFamily,
			"font-size": `${props.fontSize}px`,
			"line-height": "1.4",
			padding: "8px 12px",
			"white-space": "pre-wrap",
			"overflow-wrap": "anywhere",
			"user-select": "text",
			"-webkit-user-select": "text",
			cursor: "text",
		}}
	>
		<Show when={props.view.prompt}>
			{(prompt) => <div style={{ color: "var(--fg-muted)", "margin-bottom": "8px" }}>{prompt()}</div>}
		</Show>
		<Show
			when={props.view.answers.length > 0}
			fallback={<div style={{ color: "var(--fg-muted)" }}>No 💬 answers in the last turn.</div>}
		>
			<For each={props.view.answers}>
				{(answer) => (
					<div
						data-answer
						style={{
							padding: "2px 0 2px 8px",
							"margin-bottom": "6px",
							"border-left": "3px solid rgba(94,190,140,0.9)",
							background: "rgba(94,190,140,0.14)",
						}}
					>
						{answer}
					</div>
				)}
			</For>
		</Show>
	</div>
);
