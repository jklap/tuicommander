import { For } from "solid-js";
import type { ChoicePrompt } from "../useSessions";
import styles from "./ChoicePromptOverlay.module.css";

interface ChoicePromptOverlayProps {
	prompt: ChoicePrompt;
	onSelect: (key: string) => void;
}

/** Overlay that surfaces an agent's choice dialog as tappable buttons. The
 *  caller follows the backend's selection mode when sending PTY keys. */
export function ChoicePromptOverlay(props: ChoicePromptOverlayProps) {
	return (
		<div class={styles.dropup}>
			<div class={styles.title}>{props.prompt.title}</div>
			<For each={props.prompt.options}>
				{(option) => (
					<button
						class={styles.item}
						classList={{
							[styles.itemHighlighted]: option.highlighted,
							[styles.itemDestructive]: option.destructive,
						}}
						onClick={() => props.onSelect(option.key)}
					>
						<span class={styles.key}>{option.key}</span>
						<span class={styles.label}>{option.label}</span>
						{option.hint && <span class={styles.hint}>{option.hint}</span>}
					</button>
				)}
			</For>
		</div>
	);
}
