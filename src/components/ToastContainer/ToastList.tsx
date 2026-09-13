import { type Component, For, Show } from "solid-js";
import { type Toast, toastsStore } from "../../stores/toasts";
import { onClickKeyDown } from "../../utils/a11y";
import styles from "./ToastContainer.module.css";

interface ToastListProps {
	onDismiss: (toast: Toast) => void;
	repoName: (toast: Toast) => string | null;
}

/** Shared toast presentation. Navigation stays in the shell-specific wrapper. */
export const ToastList: Component<ToastListProps> = (props) => (
	<div class={styles.container}>
		<For each={toastsStore.toasts}>
			{(toast) => (
				<div
					class={styles.toast}
					data-level={toast.level}
					role="button"
					tabIndex={0}
					onClick={() => props.onDismiss(toast)}
					onKeyDown={onClickKeyDown(() => props.onDismiss(toast))}
				>
					<span class={styles.level} data-level={toast.level} />
					<span class={styles.body}>
						<span class={styles.titleRow}>
							<Show when={props.repoName(toast)}>{(name) => <span class={styles.repo}>{name()}</span>}</Show>
							<span class={styles.title}>{toast.title}</span>
						</span>
						{toast.message && <span class={styles.message}>{toast.message}</span>}
					</span>
					<Show when={toast.action}>
						<button
							class={styles.action}
							onClick={(event) => {
								event.stopPropagation();
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
