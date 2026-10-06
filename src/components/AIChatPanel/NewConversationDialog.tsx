import { type Component, createSignal, For, Show } from "solid-js";
import type { ChatOpenRequest } from "../../services/acpClient";
import { registerModal } from "../../stores/modalStack";
import d from "../shared/dialog.module.css";
import type { AcpChat } from "./useAcpChat";

export const NewConversationDialog: Component<{ chat: AcpChat; onClose: () => void }> = (props) => {
	registerModal(props.onClose);
	const [options, setOptions] = createSignal<ChatOpenRequest>({});
	const [opening, setOpening] = createSignal(false);
	const fields = [
		{ key: "profile", label: "Ego profile" },
		{ key: "workspace", label: "Workspace" },
		{ key: "executable", label: "Ego executable" },
	] as const;
	return (
		<div class={d.overlay} onClick={props.onClose}>
			<div
				class={d.popover}
				role="dialog"
				aria-modal="true"
				aria-labelledby="new-chat-title"
				onClick={(event) => event.stopPropagation()}
			>
				<div class={d.header}>
					<h4 id="new-chat-title">New conversation</h4>
				</div>
				<div class={d.body}>
					<p>Leave fields empty to use the defaults. Options apply only to this conversation.</p>
					<For each={fields}>
						{(field) => (
							<label>
								{field.label}
								<input
									aria-label={field.label}
									placeholder="Use default"
									disabled={opening()}
									onInput={(event) => {
										const value = event.currentTarget.value.trim();
										setOptions((current) => {
											const next = { ...current };
											if (value) next[field.key] = value;
											else delete next[field.key];
											return next;
										});
									}}
								/>
							</label>
						)}
					</For>
					<Show when={props.chat.error()}>
						<p class={d.error} role="alert">
							{props.chat.error()}
						</p>
					</Show>
				</div>
				<div class={d.actions}>
					<button type="button" class={d.cancelBtn} disabled={opening()} onClick={props.onClose}>
						Cancel
					</button>
					<button
						type="button"
						class={d.primaryBtn}
						disabled={opening()}
						onClick={async () => {
							setOpening(true);
							try {
								await props.chat.startSession(options());
								if (!props.chat.error()) props.onClose();
							} finally {
								setOpening(false);
							}
						}}
					>
						Create
					</button>
				</div>
			</div>
		</div>
	);
};
