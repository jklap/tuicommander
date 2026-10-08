import { type Component, For, Show } from "solid-js";
import { registerModal } from "../../stores/modalStack";
import { remoteConnectionsStore } from "../../stores/remoteConnections";
import d from "./dialog.module.css";

/**
 * Confirmation for SSH remote-daemon provisioning (`ssh_provision.rs`): start
 * `tuic-remote` on the remote host, or give an unconfigured daemon this
 * connection's saved password. It shows the plan the backend built — the
 * destination and every remote command exactly as it will run — and nothing
 * runs unless the user presses Accept; Cancel, Escape and a click outside all
 * decline. The accept sends back only the plan's digest, which the backend
 * checks against the stored connection before contacting anything.
 *
 * Renders nothing unless `remoteConnectionsStore.provision()` is waiting, so
 * it is mounted once, unconditionally, in `RemoteServersTab`.
 */
export const ProvisionConfirmDialog: Component = () => {
	const pending = () => remoteConnectionsStore.getPendingProvisionConfirmation();
	const decline = () => remoteConnectionsStore.resolveProvisionConfirmation(false);

	return (
		<Show when={pending()}>
			{(p) => {
				registerModal(decline);
				return (
					<div class={d.overlay} onClick={(e) => e.target === e.currentTarget && decline()}>
						<div
							class={d.popover}
							style={{ width: "560px", "max-width": "calc(100vw - 32px)" }}
							role="dialog"
							aria-modal="true"
							aria-labelledby="provision-confirm-title"
						>
							<div class={d.header}>
								<h4 id="provision-confirm-title">
									{p().action === "start" ? "Start remote daemon" : "Set remote daemon password"}
								</h4>
							</div>
							<div
								class={d.body}
								style={{
									display: "flex",
									"flex-direction": "column",
									gap: "10px",
									"max-height": "60vh",
									"overflow-y": "auto",
								}}
							>
								<p>
									<strong>{p().connection_name}</strong>: {p().summary}
								</p>
								<p>
									These commands will run on <code>{p().destination}</code> over this connection's SSH settings, in this
									order:
								</p>
								<ol
									style={{ margin: 0, "padding-left": "18px", display: "flex", "flex-direction": "column", gap: "8px" }}
								>
									<For each={p().steps}>
										{(step) => (
											<li>
												<div>{step.description}</div>
												<Show when={step.command}>
													<pre
														data-testid="provision-command"
														style={{
															margin: "4px 0 0",
															"font-size": "11px",
															"white-space": "pre-wrap",
															"word-break": "break-all",
															background: "var(--bg-secondary, rgba(255,255,255,0.03))",
															padding: "6px",
															"border-radius": "4px",
														}}
													>
														{step.command}
													</pre>
												</Show>
											</li>
										)}
									</For>
								</ol>
								<p class={d.error} style={{ margin: 0 }}>
									Nothing runs until you accept. If the connection is edited before you do, the plan is refused and
									shown again.
								</p>
							</div>
							<div class={d.actions}>
								<button type="button" class={d.cancelBtn} onClick={decline}>
									Cancel
								</button>
								<button
									type="button"
									class={d.primaryBtn}
									onClick={() => remoteConnectionsStore.resolveProvisionConfirmation(true)}
								>
									Accept and run
								</button>
							</div>
						</div>
					</div>
				);
			}}
		</Show>
	);
};
