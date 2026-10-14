import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import s from "./MobileComposerMenu.module.css";

/** Both mobile composers share pickers, action order, touch targets and dismissal. */
export function MobileComposerMenu(props: {
	disabled?: boolean;
	uploading?: boolean;
	onAttach?: (file: File) => void;
	onPark?: () => void;
	parkLabel?: string;
	parkDisabled?: boolean;
	onQueue?: () => void;
	onStop?: () => void;
	queueDisabled?: boolean;
	onCommands?: () => void;
	/** Keep the terminal's established Send long-press shortcut. */
	queueOnly?: boolean;
	onClose?: () => void;
}) {
	const [expanded, setExpanded] = createSignal(false);
	const open = () => expanded() || !!props.queueOnly;
	let root: HTMLDivElement | undefined;
	let trigger: HTMLButtonElement | undefined;
	let fileInput: HTMLInputElement | undefined;
	let cameraInput: HTMLInputElement | undefined;
	const close = () => {
		setExpanded(false);
		props.onClose?.();
	};
	const select = (action: (() => void) | undefined) => {
		close();
		action?.();
	};
	createEffect(() => {
		if (!open()) return;
		const outside = (event: PointerEvent) => {
			if (event.target instanceof Node && !root?.contains(event.target)) close();
		};
		const onEscape = (event: KeyboardEvent) => {
			if (event.key !== "Escape") return;
			event.preventDefault();
			event.stopPropagation();
			close();
			trigger?.focus();
		};
		document.addEventListener("pointerdown", outside, true);
		document.addEventListener("keydown", onEscape, true);
		onCleanup(() => {
			document.removeEventListener("pointerdown", outside, true);
			document.removeEventListener("keydown", onEscape, true);
		});
	});
	const pick = (event: Event & { currentTarget: HTMLInputElement }) => {
		const file = event.currentTarget.files?.[0];
		event.currentTarget.value = "";
		if (file) props.onAttach?.(file);
	};
	return (
		<div ref={root} class={s.root}>
			<Show when={props.onAttach}>
				<input
					ref={fileInput}
					type="file"
					accept="image/*,application/pdf,text/*"
					aria-label="Choose attachment"
					hidden
					onChange={pick}
				/>
				<input
					ref={cameraInput}
					type="file"
					accept="image/*"
					capture="environment"
					aria-label="Take photo"
					hidden
					onChange={pick}
				/>
			</Show>
			<button
				ref={trigger}
				type="button"
				class={s.trigger}
				aria-label="More composer actions"
				aria-haspopup="menu"
				aria-expanded={open()}
				disabled={props.disabled}
				onClick={() => (open() ? close() : setExpanded(true))}
			>
				<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
					<path d="M11 4h2v7h7v2h-7v7h-2v-7H4v-2h7z" />
				</svg>
			</button>
			<Show when={open()}>
				<div class={s.menu} role="menu" aria-label="Composer actions">
					<Show when={!props.queueOnly}>
						<Show when={props.onAttach}>
							<button
								type="button"
								role="menuitem"
								disabled={props.disabled || props.uploading}
								onClick={() => select(() => fileInput?.click())}
							>
								Attach file
							</button>
							<button
								type="button"
								role="menuitem"
								disabled={props.disabled || props.uploading}
								onClick={() => select(() => cameraInput?.click())}
							>
								Take photo
							</button>
						</Show>
						<Show when={props.onPark}>
							<button
								type="button"
								role="menuitem"
								disabled={props.disabled || props.uploading || props.parkDisabled}
								onClick={() => select(props.onPark)}
							>
								{props.parkLabel ?? "Park draft"}
							</button>
						</Show>
					</Show>
					<Show when={props.onQueue}>
						<button
							type="button"
							role="menuitem"
							disabled={props.disabled || props.uploading || props.queueDisabled}
							onClick={() => select(props.onQueue)}
						>
							Queue
						</button>
					</Show>
					<Show when={!props.queueOnly && props.onStop}>
						<button type="button" role="menuitem" onClick={() => select(props.onStop)}>
							Stop
						</button>
					</Show>
					<Show when={!props.queueOnly && props.onCommands}>
						<button
							type="button"
							role="menuitem"
							disabled={props.disabled || props.uploading}
							onClick={() => select(props.onCommands)}
						>
							Commands (/)
						</button>
					</Show>
				</div>
			</Show>
		</div>
	);
}
