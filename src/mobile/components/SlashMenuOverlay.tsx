import { createMemo, For, Show } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { rpc } from "../../transport";
import type { SlashMenuItem } from "../useSessions";
import styles from "./SlashMenuOverlay.module.css";

interface SlashMenuOverlayProps {
	items: SlashMenuItem[];
	sessionId: string;
	onSelect: (command: string) => void;
	onClose?: () => void;
	local?: boolean;
}

/** Compact dropup that renders above the input area. Items come pre-filtered
 *  from the backend (Claude Code's own slash menu filtering). */
export function SlashMenuOverlay(props: SlashMenuOverlayProps) {
	const availableItems = createMemo(() =>
		props.items.filter((item) => !/\(removed\)/i.test(`${item.command} ${item.description}`)),
	);
	function navigate(direction: "up" | "down") {
		const key = direction === "up" ? "\x1b[A" : "\x1b[B";
		rpc("write_pty", { sessionId: props.sessionId, data: key }).catch((err: unknown) => {
			appLogger.warn("network", "SlashMenu navigate failed", { error: err });
		});
	}

	return (
		<div class={styles.dropup} role="toolbar" aria-label="Commands" onTouchMove={(e) => e.stopPropagation()}>
			<button class={styles.control} aria-label="Close slash menu" onClick={() => props.onClose?.()}>
				<svg
					width="14"
					height="14"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					stroke-width="2"
					aria-hidden="true"
				>
					<path d="M18 6 6 18M6 6l12 12" />
				</svg>
			</button>
			<Show when={!props.local}>
				<button class={styles.control} onClick={() => navigate("up")} aria-label="Previous">
					<svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
						<path d="M7.41 15.41L12 10.83l4.59 4.58L18 14l-6-6-6 6z" />
					</svg>
				</button>
				<button class={styles.control} onClick={() => navigate("down")} aria-label="Next">
					<svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
						<path d="M7.41 8.59L12 13.17l4.59-4.58L18 10l-6 6-6-6z" />
					</svg>
				</button>
			</Show>
			<For each={availableItems()}>
				{(item) => (
					<button
						class={styles.item}
						classList={{ [styles.itemHighlighted]: item.highlighted }}
						title={item.description || undefined}
						onClick={() => props.onSelect(item.command)}
					>
						{item.command}
					</button>
				)}
			</For>
		</div>
	);
}
