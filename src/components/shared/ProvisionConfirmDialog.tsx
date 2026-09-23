import { type Component, Show } from "solid-js";
import { registerModal } from "../../stores/modalStack";
import { remoteConnectionsStore } from "../../stores/remoteConnections";
import d from "./dialog.module.css";

/**
 * Remote-daemon-provisioning confirmation (Phase 5): a generalized version of
 * `DirectCertConfirmDialog`'s one-at-a-time modal pattern, for SSH connect's
 * "install/start/set-password" steps — each of which mutates state on the
 * remote host and must always be confirmed explicitly, never silent (see
 * `remoteConnections.ts`'s `requestProvisionConfirmation`).
 *
 * Renders nothing when no connection is mid-Connect and awaiting a decision.
 */
export const ProvisionConfirmDialog: Component = () => {
	const pending = () => remoteConnectionsStore.getPendingProvisionConfirmation();

	return (
		<Show when={pending()}>
			{(p) => {
				registerModal(() => remoteConnectionsStore.resolveProvisionConfirmation(false));
				return (
					<div
						class={d.overlay}
						onClick={(e) => e.target === e.currentTarget && remoteConnectionsStore.resolveProvisionConfirmation(false)}
					>
						<div class={d.popover} style={{ width: "440px" }}>
							<div class={d.header}>
								<h4>{p().connectionName}</h4>
							</div>
							<div class={d.body}>
								<p>{p().message}</p>
							</div>
							<div class={d.actions}>
								<button class={d.cancelBtn} onClick={() => remoteConnectionsStore.resolveProvisionConfirmation(false)}>
									Cancel
								</button>
								<button class={d.primaryBtn} onClick={() => remoteConnectionsStore.resolveProvisionConfirmation(true)}>
									{p().confirmLabel}
								</button>
							</div>
						</div>
					</div>
				);
			}}
		</Show>
	);
};
