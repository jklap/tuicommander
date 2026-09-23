import { type Component, Show } from "solid-js";
import { registerModal } from "../../stores/modalStack";
import { remoteConnectionsStore } from "../../stores/remoteConnections";
import d from "./dialog.module.css";

/**
 * First-connect confirmation for a Direct connection whose `https://` daemon
 * presents a certificate no system root vouches for (typically the daemon's own
 * self-signed one). Accepting pins the SHA-256 fingerprint on the connection;
 * the backend then reaches the daemon only through a relay that accepts exactly
 * that certificate, and refuses to connect if it ever changes.
 *
 * Renders nothing unless `remoteConnectionsStore.connect()` is waiting for a
 * decision, so it is mounted once, unconditionally, in `RemoteServersTab`.
 */
export const DirectCertConfirmDialog: Component = () => {
	const pending = () => remoteConnectionsStore.getPendingFingerprintConfirmation();
	const decline = () => remoteConnectionsStore.resolveFingerprintConfirmation(false);

	return (
		<Show when={pending()}>
			{(p) => {
				registerModal(decline);
				return (
					<div class={d.overlay} onClick={(e) => e.target === e.currentTarget && decline()}>
						<div
							class={d.popover}
							style={{ width: "480px" }}
							role="dialog"
							aria-modal="true"
							aria-labelledby="direct-cert-confirm-title"
						>
							<div class={d.header}>
								<h4 id="direct-cert-confirm-title">Verify certificate</h4>
							</div>
							<div class={d.body} style={{ display: "flex", "flex-direction": "column", gap: "10px" }}>
								<p>
									<strong>{p().connectionName}</strong> (<code>{p().url}</code>) presented a certificate that no
									certificate authority trusted by this system has signed. That is expected for a TUICommander daemon
									using its own self-signed certificate.
								</p>
								<p>
									Compare this SHA-256 fingerprint with the one the remote machine shows under Settings → Remote Access
									→ Self-Signed HTTPS before accepting:
								</p>
								<p
									data-testid="direct-cert-fingerprint"
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
									Accept only if you recognise the server. If the certificate changes later, TUICommander refuses to
									connect instead of trusting the new one.
								</p>
							</div>
							<div class={d.actions}>
								<button type="button" class={d.cancelBtn} onClick={decline}>
									Cancel
								</button>
								<button
									type="button"
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
