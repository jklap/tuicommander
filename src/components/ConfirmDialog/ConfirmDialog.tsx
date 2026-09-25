import { type Component, createEffect, createSignal, onCleanup, Show } from "solid-js";
import { isTopModal, popModal, pushModal } from "../../stores/modalStack";
import d from "../shared/dialog.module.css";

export interface ConfirmDialogProps {
	visible: boolean;
	title: string;
	message: string;
	confirmLabel?: string;
	cancelLabel?: string;
	/** Optional middle button (e.g. "Don't Save"). Rendered only when set. */
	discardLabel?: string;
	kind?: "warning" | "info" | "error";
	/** Which button Enter activates. Defaults to "confirm". */
	defaultButton?: "confirm" | "cancel";
	/** When set, auto-clicks the cancel button after this many ms, showing a countdown on its label. */
	autoCancelMs?: number;
	/** When set, confirms after this many ms, showing a countdown on the confirm label. */
	autoConfirmMs?: number;
	onClose: () => void;
	onConfirm: () => void;
	/** Invoked when the middle discard button is clicked. */
	onDiscard?: () => void;
}

/**
 * In-app confirmation dialog — replaces native Tauri ask() dialogs
 * which render as ugly light-mode macOS system sheets.
 * Uses shared dialog CSS module for consistent dark-theme styling.
 */
export const ConfirmDialog: Component<ConfirmDialogProps> = (props) => {
	// Countdown until the configured automatic action.
	const [remaining, setRemaining] = createSignal<number | null>(null);
	let countdownTimer: ReturnType<typeof setInterval> | undefined;
	const stopCountdown = () => {
		if (countdownTimer !== undefined) clearInterval(countdownTimer);
		countdownTimer = undefined;
		setRemaining(null);
	};
	const close = () => {
		stopCountdown();
		props.onClose();
	};
	const confirm = () => {
		stopCountdown();
		props.onConfirm();
	};

	createEffect(() => {
		const duration = props.autoConfirmMs ?? props.autoCancelMs;
		if (!props.visible || !duration || duration <= 0) {
			setRemaining(null);
			return;
		}
		let left = Math.ceil(duration / 1000);
		setRemaining(left);
		countdownTimer = setInterval(() => {
			left -= 1;
			setRemaining(left);
			if (left <= 0) {
				if (props.autoConfirmMs) confirm();
				else close();
			}
		}, 1000);
		onCleanup(stopCountdown);
	});

	createEffect(() => {
		if (!props.visible) return;

		// Escape-to-close is handled centrally (stores/modalStack): pushing here
		// routes Escape to close() AND stops it reaching the terminal
		// underneath. Keep the id so the Enter handler below can check whether
		// THIS instance is the top-most modal — more than one ConfirmDialog can
		// be visible at once (e.g. AgentWrapPromptHost, one per pending agent),
		// and without this check a single Enter press would fire every open
		// instance's default action simultaneously.
		const modalId = pushModal(close);
		onCleanup(() => popModal(modalId));

		const handleKeydown = (e: KeyboardEvent) => {
			if (e.key === "Enter" && isTopModal(modalId)) {
				e.preventDefault();
				// Enter activates the configured default button. Destructive dialogs
				// point it at Cancel so an accidental Enter takes the safe path.
				if ((props.defaultButton ?? "confirm") === "cancel") {
					close();
				} else {
					confirm();
				}
			}
		};

		document.addEventListener("keydown", handleKeydown);
		onCleanup(() => document.removeEventListener("keydown", handleKeydown));
	});

	return (
		<Show when={props.visible}>
			<div class={d.overlay} onClick={close}>
				<div class={d.popover} onClick={(e) => e.stopPropagation()}>
					<div class={d.header}>
						<h4>{props.title}</h4>
					</div>
					<div class={d.body}>
						<p
							style={{
								margin: 0,
								"white-space": "pre-line",
								"overflow-wrap": "anywhere",
								color: "var(--fg-secondary)",
								"font-size": "var(--font-md)",
							}}
						>
							{props.message}
						</p>
					</div>
					<div class={d.actions}>
						<button class={d.cancelBtn} onClick={close}>
							{props.cancelLabel ?? "Cancel"}
							{props.autoCancelMs && remaining() !== null ? ` (${remaining()})` : ""}
						</button>
						<Show when={props.discardLabel}>
							<button class={d.cancelBtn} onClick={() => props.onDiscard?.()}>
								{props.discardLabel}
							</button>
						</Show>
						<button class={d.primaryBtn} onClick={confirm}>
							{props.confirmLabel ?? "OK"}
							{props.autoConfirmMs && remaining() !== null ? ` (${remaining()})` : ""}
						</button>
					</div>
				</div>
			</div>
		</Show>
	);
};

export default ConfirmDialog;
