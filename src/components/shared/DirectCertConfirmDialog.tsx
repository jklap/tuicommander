import { type Component, Show } from "solid-js";
import { registerModal } from "../../stores/modalStack";
import { remoteConnectionsStore } from "../../stores/remoteConnections";
import d from "./dialog.module.css";

/**
 * First-connect confirmation for a Direct connection's self-signed/untrusted
 * certificate (story: SSH Tunnels + Remote Servers consolidation, Phase 4).
 * Mirrors `selfsigned.rs`'s existing "compare the fingerprint before
 * accepting" UX for TUICommander's own self-signed HTTPS, applied here to a
 * REMOTE server's certificate — a "click through once" flow, not a new one.
 *
 * Renders nothing when no connection is mid-Connect and awaiting a decision
 * (`remoteConnectionsStore.getPendingFingerprintConfirmation()` is reactive,
 * so this component can be mounted once, unconditionally, in
 * `RemoteServersTab`).
 */
export const DirectCertConfirmDialog: Component = () => {
	const pending = () => remoteConnectionsStore.getPendingFingerprintConfirmation();

	return (
		<Show when={pending()}>
			{(p) => {
				registerModal(() => remoteConnectionsStore.resolveFingerprintConfirmation(false));
				return (
					<div
						class={d.overlay}
						onClick={(e) =>
							e.target === e.currentTarget && remoteConnectionsStore.resolveFingerprintConfirmation(false)
						}
					>
						<div class={d.popover} style={{ width: "480px" }}>
							<div class={d.header}>
								<h4>Verify certificate</h4>
							</div>
							<div class={d.body} style={{ display: "flex", "flex-direction": "column", gap: "10px" }}>
								<p>
									<strong>{p().connectionName}</strong> (<code>{p().url}</code>) presented a certificate that isn't
									signed by a certificate authority your system trusts. This is expected for a self-signed certificate —
									the same situation TUICommander's own remote-access HTTPS handles with a one-time browser warning.
								</p>
								<p>Compare this fingerprint against the one shown on the remote machine before accepting:</p>
								<p
									style={{
										"font-family": "monospace",
										"font-size": "12px",
										"word-break": "break-all",
										background: "var(--bg-secondary, rgba(255,255,255,0.03))",
										padding: "8px",
										"border-radius": "4px",
									}}
								>
									{p().fingerprint}
								</p>
								<p class={d.error} style={{ margin: 0 }}>
									Only accept this if you recognize the server and expect it to be self-signed. If the fingerprint
									changes later without you setting up a new certificate, TUICommander will refuse to connect
									automatically rather than silently trust the new one.
								</p>
							</div>
							<div class={d.actions}>
								<button
									class={d.cancelBtn}
									onClick={() => remoteConnectionsStore.resolveFingerprintConfirmation(false)}
								>
									Cancel
								</button>
								<button
									class={d.primaryBtn}
									onClick={() => remoteConnectionsStore.resolveFingerprintConfirmation(true)}
								>
									Accept and connect
								</button>
							</div>
						</div>
					</div>
				);
			}}
		</Show>
	);
};
