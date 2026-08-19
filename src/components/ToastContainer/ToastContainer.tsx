import { type Component, For, Show } from "solid-js";
import { terminalsStore } from "../../stores/terminals";
import { type Toast, toastsStore } from "../../stores/toasts";
import { onClickKeyDown } from "../../utils/a11y";
import styles from "./ToastContainer.module.css";

/**
 * Dismiss, and take the user to the terminal that raised the toast when there
 * is one to go to. An agent's `ui action=toast` says something happened in one
 * of ~25 open tabs; without this the user is told the news and then left to
 * find the speaker. A toast with no session, or one whose tab has since closed,
 * still just dismisses — the lookup happens here rather than in setActive so a
 * closed tab is a silent no-op instead of a warning.
 */
function dismissAndReveal(toast: Toast): void {
	toastsStore.remove(toast.id);
	if (!toast.sessionId) return;
	const terminalId = terminalsStore.findBySessionId(toast.sessionId);
	if (terminalId) terminalsStore.setActive(terminalId);
}

export const ToastContainer: Component = () => {
	return (
		<div class={styles.container}>
			<For each={toastsStore.toasts}>
				{(toast) => (
					<div
						class={styles.toast}
						data-level={toast.level}
						role="button"
						tabIndex={0}
						onClick={() => dismissAndReveal(toast)}
						onKeyDown={onClickKeyDown(() => dismissAndReveal(toast))}
					>
						<span class={styles.level} data-level={toast.level} />
						<span class={styles.body}>
							<span class={styles.title}>{toast.title}</span>
							{toast.message && <span class={styles.message}>{toast.message}</span>}
						</span>
						<Show when={toast.action}>
							<button
								class={styles.action}
								onClick={(e) => {
									e.stopPropagation();
									toast.action!.onClick();
									toastsStore.remove(toast.id);
								}}
							>
								{toast.action!.label}
							</button>
						</Show>
					</div>
				)}
			</For>
		</div>
	);
};
