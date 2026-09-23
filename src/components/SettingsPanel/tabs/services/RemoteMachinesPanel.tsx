import { type Component, createSignal, For, onMount, Show } from "solid-js";
import { appLogger } from "../../../../stores/appLogger";
import {
	type ConnectionState,
	type DiscoveredSshHost,
	type DiscoveredSshHosts,
	type ProvisionAction,
	type RemoteConnection,
	type RemoteTransport,
	remoteConnectionsStore,
	type SshAgentInfo,
	type SshHostStatus,
} from "../../../../stores/remoteConnections";
import {
	ConnectionStatusBadge,
	remoteConnectionStatusColor,
	remoteConnectionStatusLabel,
} from "../../../shared/ConnectionStatusBadge";
import s from "../../Settings.module.css";

// ---------------------------------------------------------------------------
// Remote Machines panel
// ---------------------------------------------------------------------------

/** Transport summary string */
export function transportSummary(transport: RemoteTransport): string {
	if (transport.type === "Ssh") {
		return `${transport.ssh.user}@${transport.ssh.host}:${transport.ssh.port}`;
	}
	if (transport.type === "Local") {
		return transport.instance_id ? `local: ${transport.instance_id}` : `local: 127.0.0.1:${transport.port ?? "?"}`;
	}
	return transport.url;
}

/** Human-readable badge text for a transport's kind */
export function transportBadgeLabel(transport: RemoteTransport): string {
	switch (transport.type) {
		case "Ssh":
			return "SSH";
		case "Local":
			return "LOCAL";
		default:
			return "DIRECT";
	}
}

/** Probe state of a discovered host, matched by resolved target and port (never by display name). */
export function discoveredHostAuth(host: DiscoveredSshHost, statuses: SshHostStatus[]): SshHostStatus["auth"] | null {
	return statuses.find((st) => st.target === host.target && st.port === host.port)?.auth ?? null;
}

/** Replace the status of the same target and port, or append it. */
export function withStatus(statuses: SshHostStatus[], status: SshHostStatus): SshHostStatus[] {
	return [...statuses.filter((st) => !(st.target === status.target && st.port === status.port)), status];
}

export interface RemoteMachinesPanelProps {
	/** Open the merged connection editor (`RemoteConnectionEditor`) on this connection. */
	onEdit: (connection: RemoteConnection) => void;
	/** Open the merged editor on a new SSH connection prefilled from a discovered host. */
	onAddFromHost: (host: DiscoveredSshHost) => void;
}

/**
 * The remote-connection list of the "Remote Servers" page: discovered SSH
 * hosts, then one row per connection with its live status (pushed by the Rust
 * runtime), Connect/Disconnect, Update & restart, Install/Uninstall, Edit and
 * Remove. Creating and editing go through the page's merged editor, so a
 * connection and a tunnel profile are edited by one form.
 */
export const RemoteMachinesPanel: Component<RemoteMachinesPanelProps> = (props) => {
	const [error, setError] = createSignal("");
	const [sshHosts, setSshHosts] = createSignal<SshHostStatus[]>([]);
	const [probingHosts, setProbingHosts] = createSignal(false);
	const [discovered, setDiscovered] = createSignal<DiscoveredSshHosts>({ hosts: [], hashed_count: 0 });
	const [agentInfo, setAgentInfo] = createSignal<SshAgentInfo | null>(null);
	const [serviceBusyId, setServiceBusyId] = createSignal<string | null>(null);
	const [updateErrors, setUpdateErrors] = createSignal<Record<string, string>>({});
	const [provisionNotes, setProvisionNotes] = createSignal<Record<string, string>>({});

	onMount(() => {
		remoteConnectionsStore.hydrate();
		// Reads ~/.ssh/config and known_hosts and asks ssh-add; no host is contacted.
		remoteConnectionsStore
			.discoverSshHosts()
			.then(setDiscovered)
			.catch((e) => appLogger.error("settings", "Failed to discover SSH hosts", { error: String(e) }));
		remoteConnectionsStore
			.sshAgentInfo()
			.then(setAgentInfo)
			.catch((e) => appLogger.error("settings", "Failed to list SSH agent keys", { error: String(e) }));
	});

	function connectionList(): ConnectionState[] {
		const conns = remoteConnectionsStore.getConnections();
		return Object.values(conns);
	}

	async function probeHosts() {
		setProbingHosts(true);
		setError("");
		try {
			setSshHosts(await remoteConnectionsStore.probeSshHosts());
		} catch (e) {
			setError(String(e));
		} finally {
			setProbingHosts(false);
		}
	}

	async function probeOneHost(host: DiscoveredSshHost) {
		setError("");
		try {
			const status = await remoteConnectionsStore.probeSshHost(host);
			setSshHosts((current) => withStatus(current, status));
		} catch (e) {
			setError(String(e));
		}
	}

	async function toggleInstalled(conn: RemoteConnection) {
		setServiceBusyId(conn.id);
		setError("");
		try {
			if (conn.deploy === "installed") await remoteConnectionsStore.uninstall(conn.id);
			else await remoteConnectionsStore.install(conn.id);
		} catch (e) {
			setError(String(e));
		} finally {
			setServiceBusyId(null);
		}
	}

	async function updateRemote(connState: ConnectionState) {
		if (connState.updateInProgress) return;
		const id = connState.connection.id;
		setServiceBusyId(id);
		setError("");
		setUpdateErrors((current) => ({ ...current, [id]: "" }));
		try {
			const { session_count, desktop_build, remote_build, source } = await remoteConnectionsStore.prepareUpdate(id);
			const details =
				`Remote: ${remote_build?.version ?? "unknown"} (${remote_build?.target ?? "unknown target"})\n` +
				`Selected: ${desktop_build.version} (${desktop_build.target}, ${source})\n` +
				`${session_count} live sessions will be lost. Update and restart remote?`;
			if (!window.confirm(details)) return;
			await remoteConnectionsStore.updateAndRestart(id, session_count, desktop_build.sha256);
		} catch (reason) {
			setUpdateErrors((current) => ({ ...current, [id]: String(reason) }));
		} finally {
			setServiceBusyId(null);
		}
	}

	/** Act on the backend's SSH offer: the store shows the plan and runs it only on Accept. */
	async function provision(connState: ConnectionState, action: ProvisionAction) {
		const id = connState.connection.id;
		setServiceBusyId(id);
		setProvisionNotes((current) => ({ ...current, [id]: "" }));
		try {
			const note = await remoteConnectionsStore.provision(id, action);
			if (note) setProvisionNotes((current) => ({ ...current, [id]: note }));
		} catch (reason) {
			setUpdateErrors((current) => ({ ...current, [id]: String(reason) }));
		} finally {
			setServiceBusyId(null);
		}
	}

	async function removeConnection(id: string, name: string) {
		let confirmed: boolean;
		try {
			const { confirm } = await import("@tauri-apps/plugin-dialog");
			confirmed = await confirm(`Remove remote machine "${name}"?`, {
				title: "Remove remote machine",
				kind: "warning",
			});
		} catch {
			confirmed = window.confirm(`Remove remote machine "${name}"?`);
		}
		if (!confirmed) return;
		try {
			await remoteConnectionsStore.removeConnection(id);
		} catch (e) {
			appLogger.error("settings", "Failed to remove remote connection", { error: String(e) });
		}
	}

	return (
		<div>
			<Show when={error()}>
				<p class={s.hint} style={{ color: "var(--error, #e06c75)" }} role="alert">
					{error()}
				</p>
			</Show>

			{/* Discovered SSH hosts */}
			<div class={s.group}>
				<Show when={agentInfo()}>
					{(info) => (
						<p
							class={s.hint}
							style={{ margin: "0 0 6px", color: info().keys.length === 0 ? "var(--warning)" : undefined }}
							role={info().keys.length === 0 ? "alert" : undefined}
						>
							{info().keys.length === 0
								? "No SSH agent identities loaded: key-based connections will fail authentication. Add a key with ssh-add."
								: `SSH agent keys loaded (${info().keys.length}): ${info()
										.keys.map((k) => k.comment || k.fingerprint)
										.join(", ")}`}
						</p>
					)}
				</Show>
				<div style={{ display: "flex", "align-items": "center", gap: "8px", "justify-content": "space-between" }}>
					<span style={{ "font-weight": 500, "font-size": "13px" }}>
						Discovered SSH hosts ({discovered().hosts.length})
					</span>
					<button class={s.textBtn} type="button" onClick={probeHosts} disabled={probingHosts()}>
						{probingHosts() ? "Probing..." : "Probe config hosts"}
					</button>
				</div>
				<p class={s.hint} style={{ margin: "4px 0 0" }}>
					Only ssh config hosts are probed in bulk. A known_hosts host is contacted only when you press its own Probe.
				</p>
				<Show when={discovered().hashed_count > 0}>
					<p class={s.hint} style={{ margin: "4px 0 0" }}>
						{discovered().hashed_count} known_hosts entries are hashed and cannot be listed.
					</p>
				</Show>
				<div style={{ "max-height": "220px", "overflow-y": "auto", "margin-top": "6px" }}>
					<For each={discovered().hosts}>
						{(host) => {
							const auth = () => discoveredHostAuth(host, sshHosts());
							return (
								<div style={{ display: "flex", gap: "4px", "align-items": "center" }}>
									<button
										type="button"
										class={s.textBtn}
										title="Add a connection prefilled with this host"
										style={{ display: "flex", flex: 1, "justify-content": "space-between", gap: "8px" }}
										onClick={() => {
											setError("");
											props.onAddFromHost(host);
										}}
									>
										<span style={{ "font-family": "monospace", "font-size": "11px" }}>
											{host.user ? `${host.user}@` : ""}
											{host.host}
											{host.port ? `:${host.port}` : ""}
										</span>
										<span style={{ "font-size": "11px", color: "var(--text-dimmed)" }}>
											{auth() ? auth()?.replaceAll("_", " ") : host.source === "config" ? "ssh config" : "known_hosts"}
										</span>
									</button>
									<button
										type="button"
										class={s.textBtn}
										title="Contact this host once to check authentication"
										onClick={() => probeOneHost(host)}
									>
										Probe
									</button>
								</div>
							);
						}}
					</For>
				</div>
			</div>

			{/* Empty state */}
			<Show when={connectionList().length === 0}>
				<p class={s.hint} style={{ color: "var(--text-dimmed)" }}>
					No remote machines configured. Use "Add Connection" above to add one.
				</p>
			</Show>

			{/* Connection list */}
			<For each={connectionList()}>
				{(connState) => {
					const conn = () => connState.connection;
					return (
						<div style={{ "border-bottom": "1px solid var(--border-subtle, rgba(255,255,255,0.06))" }}>
							<div class={s.group} style={{ display: "flex", "align-items": "center", gap: "8px", padding: "8px 0" }}>
								{/* Info */}
								<div style={{ flex: 1, "min-width": 0 }}>
									<div style={{ display: "flex", "align-items": "center", gap: "6px", "flex-wrap": "wrap" }}>
										<span style={{ "font-weight": 500, "font-size": "13px" }}>{conn().name}</span>
										<span
											style={{
												"font-size": "10px",
												padding: "1px 5px",
												"border-radius": "3px",
												background:
													conn().transport.type === "Ssh"
														? "color-mix(in srgb, var(--activity) 15%, transparent)"
														: "color-mix(in srgb, var(--success) 15%, transparent)",
												color: conn().transport.type === "Ssh" ? "var(--activity)" : "var(--success)",
											}}
										>
											{transportBadgeLabel(conn().transport)}
										</span>
										<ConnectionStatusBadge
											color={remoteConnectionStatusColor(connState.status)}
											label={remoteConnectionStatusLabel(connState.status, connState.deployStep)}
										/>
										<Show when={connState.outOfDate}>
											<span style={{ "font-size": "11px", color: "var(--attention)" }}>Remote out of date</span>
										</Show>
										<Show
											when={connState.outOfDate && connState.liveSessions !== undefined && connState.liveSessions > 0}
										>
											<span style={{ "font-size": "11px", color: "var(--attention)" }}>
												{connState.liveSessions} live sessions. Update available.
											</span>
										</Show>
									</div>
									<div
										class={s.hint}
										style={{
											margin: 0,
											"font-family": "monospace",
											"font-size": "11px",
											overflow: "hidden",
											"text-overflow": "ellipsis",
											"white-space": "nowrap",
										}}
									>
										{transportSummary(conn().transport)}
									</div>
									<Show when={connState.error}>
										<div class={s.hint} style={{ margin: 0, "font-size": "11px", color: "var(--error)" }}>
											{connState.error}
										</div>
									</Show>
									<Show when={connState.updateNotice}>
										<div class={s.hint} style={{ margin: 0, "font-size": "11px" }} role="status">
											{connState.updateNotice}
										</div>
									</Show>
									<Show when={provisionNotes()[conn().id]}>
										<div class={s.hint} style={{ margin: 0, "font-size": "11px" }} role="status">
											{provisionNotes()[conn().id]}
										</div>
									</Show>
									<Show when={updateErrors()[conn().id]}>
										<div class={s.hint} style={{ margin: 0, "font-size": "11px", color: "var(--error)" }} role="alert">
											{updateErrors()[conn().id]}
										</div>
									</Show>
								</div>
								{/* SSH offers: the backend found the daemon down or unconfigured. */}
								<Show when={connState.provisionOffer === "start" && connState.status !== "connected"}>
									<button
										class={s.textBtn}
										disabled={serviceBusyId() === conn().id}
										onClick={() => provision(connState, "start")}
									>
										Start remote daemon…
									</button>
								</Show>
								<Show when={connState.provisionOffer === "set_password" && connState.status !== "connected"}>
									<button
										class={s.textBtn}
										disabled={serviceBusyId() === conn().id}
										onClick={() => provision(connState, "set_password")}
									>
										Set remote password…
									</button>
								</Show>
								{/* Connect / Disconnect. A Local connection is another install on this
								    machine: its version notice says to update it directly. */}
								<Show when={connState.status === "connected" && conn().transport.type !== "Local"}>
									<button
										class={s.textBtn}
										disabled={serviceBusyId() === conn().id || connState.updateInProgress}
										onClick={() => updateRemote(connState)}
									>
										{serviceBusyId() === conn().id ? "Updating..." : "Update & restart remote"}
									</button>
								</Show>
								<button
									class={s.textBtn}
									onClick={() => {
										// "unauthenticated" still holds a tunnel and a baseUrl, so it
										// disconnects like any live connection rather than re-dialling.
										if (connState.status === "disconnected" || connState.status === "error") {
											remoteConnectionsStore.connect(conn().id);
										} else {
											remoteConnectionsStore.disconnect(conn().id);
										}
									}}
								>
									{connState.status === "disconnected" || connState.status === "error" ? "Connect" : "Disconnect"}
								</button>
								<Show when={conn().transport.type === "Ssh"}>
									<button
										class={s.textBtn}
										disabled={serviceBusyId() === conn().id}
										onClick={() => toggleInstalled(conn())}
										title={conn().deploy === "installed" ? "Remove persistent service" : "Install persistent service"}
									>
										<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
											<path d="M7.25 1h1.5v8.1l2.65-2.65 1.06 1.06L8 11.97 3.54 7.51 4.6 6.45 7.25 9.1zM2 13h12v2H2z" />
										</svg>
										{serviceBusyId() === conn().id
											? " Working..."
											: conn().deploy === "installed"
												? " Uninstall"
												: " Install"}
									</button>
								</Show>
								{/* Edit */}
								<button
									class={s.copyBtn}
									style={{ "flex-shrink": 0 }}
									title="Edit"
									onClick={() => props.onEdit(conn())}
								>
									<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
										<path d="M12.146.146a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1 0 .708l-10 10a.5.5 0 0 1-.168.11l-5 2a.5.5 0 0 1-.65-.65l2-5a.5.5 0 0 1 .11-.168zM11.207 2.5 13.5 4.793 14.793 3.5 12.5 1.207zm1.586 3L10.5 3.207 4 9.707V10h.5a.5.5 0 0 1 .5.5v.5h.5a.5.5 0 0 1 .5.5v.5h.293z" />
									</svg>
								</button>
								{/* Delete */}
								<button
									class={s.copyBtn}
									title="Remove"
									onClick={() => removeConnection(conn().id, conn().name)}
									style={{ color: "var(--error, #e06c75)", "flex-shrink": 0 }}
								>
									<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
										<path d="M5.5 5.5A.5.5 0 0 1 6 6v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5m2.5 0a.5.5 0 0 1 .5.5v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5m3 .5a.5.5 0 0 0-1 0v6a.5.5 0 0 0 1 0z" />
										<path d="M14.5 3a1 1 0 0 1-1 1H13v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V4h-.5a1 1 0 0 1-1-1V2a1 1 0 0 1 1-1H6a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1h3.5a1 1 0 0 1 1 1zM4.118 4 4 4.059V13a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1V4.059L11.882 4zM2.5 3h11V2h-11z" />
									</svg>
								</button>
							</div>
						</div>
					);
				}}
			</For>
		</div>
	);
};
