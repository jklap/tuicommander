import { type Component, createSignal, For, onMount, Show } from "solid-js";
import { appLogger } from "../../../../stores/appLogger";
import {
	type ConnectionState,
	type RemoteConnection,
	type RemoteTransport,
	remoteConnectionsStore,
} from "../../../../stores/remoteConnections";
import { randomId } from "../../../../utils/randomId";
import s from "../../Settings.module.css";

// ---------------------------------------------------------------------------
// Remote Machines panel
// ---------------------------------------------------------------------------

/** Status dot color for remote connection state */
export function remoteStatusColor(status: string): string {
	switch (status) {
		case "connected":
			return "var(--accent-green, #22c55e)";
		case "connecting":
			return "var(--fg-warning, #e5a100)";
		case "error":
			return "var(--accent-red, #ef4444)";
		default:
			return "var(--fg-muted)";
	}
}

/** Human-readable label for remote connection status */
export function remoteStatusLabel(status: string): string {
	switch (status) {
		case "connected":
			return "Connected";
		case "connecting":
			return "Connecting...";
		case "error":
			return "Error";
		default:
			return "Disconnected";
	}
}

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

/** Blank form state for adding/editing a remote machine.
 *
 * `sshServerAliveInterval`/`sshServerAliveCountMax`/`sshStrictHostKeyChecking`
 * have no UI in this form yet (the shared `SshConnectionFields` editor lands
 * in a later phase of the SSH Tunnels + Remote Servers consolidation plan) —
 * they're carried through here only so editing an existing connection
 * round-trips its real values instead of silently resetting them to these
 * defaults on every save. */
export function emptyRemoteForm() {
	return {
		name: "",
		transportType: "Ssh" as "Ssh" | "Direct",
		sshHost: "",
		sshPort: 22,
		sshUser: "",
		identityFile: "",
		sshServerAliveInterval: 15,
		sshServerAliveCountMax: 3,
		sshStrictHostKeyChecking: "Yes" as "Yes" | "AcceptNew",
		// Matches `RemoteConnection::new_ssh`'s Rust default and what a freshly
		// started `tuic-remote` binary actually listens on out of the box
		// (`tuic_remote.rs`'s `TUIC_PORT` fallback) — 9876 was this form's bug.
		remoteDaemonPort: 9877,
		directUrl: "",
		authUsername: "",
	};
}

export const RemoteMachinesPanel: Component = () => {
	const [showAdd, setShowAdd] = createSignal(false);
	const [form, setForm] = createSignal(emptyRemoteForm());
	const [saving, setSaving] = createSignal(false);
	const [error, setError] = createSignal("");
	const [editingId, setEditingId] = createSignal<string | null>(null);
	const [editForm, setEditForm] = createSignal(emptyRemoteForm());

	onMount(() => {
		remoteConnectionsStore.hydrate();
	});

	function connectionList(): ConnectionState[] {
		const conns = remoteConnectionsStore.getConnections();
		return Object.values(conns);
	}

	async function addConnection() {
		const f = form();
		const name = f.name.trim();
		if (!name) {
			setError("Name is required");
			return;
		}
		if (f.transportType === "Ssh" && !f.sshHost.trim()) {
			setError("SSH host is required");
			return;
		}
		if (f.transportType === "Direct" && !f.directUrl.trim()) {
			setError("URL is required");
			return;
		}

		const transport: RemoteTransport =
			f.transportType === "Ssh"
				? {
						type: "Ssh",
						ssh: {
							host: f.sshHost.trim(),
							port: f.sshPort,
							user: f.sshUser.trim(),
							identity_file: f.identityFile.trim() || null,
							server_alive_interval: f.sshServerAliveInterval,
							server_alive_count_max: f.sshServerAliveCountMax,
							strict_host_key_checking: f.sshStrictHostKeyChecking,
						},
						remote_daemon_port: f.remoteDaemonPort,
					}
				: { type: "Direct", url: f.directUrl.trim() };

		const conn: RemoteConnection = {
			// No prefix: the backend's `validate()` requires `id` to be a bare
			// UUID (`Uuid::parse_str`), rejecting anything else.
			id: randomId(""),
			name,
			transport,
			auth_username: f.authUsername.trim() || null,
			enabled: true,
		};

		setSaving(true);
		setError("");
		try {
			await remoteConnectionsStore.addConnection(conn);
			setForm(emptyRemoteForm());
			setShowAdd(false);
		} catch (e) {
			setError(String(e));
		} finally {
			setSaving(false);
		}
	}

	function startEdit(conn: RemoteConnection) {
		setEditingId(conn.id);
		const empty = emptyRemoteForm();
		setEditForm({
			name: conn.name,
			// "Local" isn't creatable/editable through this form yet (Phase 3
			// of the consolidation plan) — fall back to "Ssh" rather than widen
			// `transportType`'s type for a kind this form can't actually render.
			transportType: conn.transport.type === "Direct" ? "Direct" : "Ssh",
			sshHost: conn.transport.type === "Ssh" ? conn.transport.ssh.host : "",
			sshPort: conn.transport.type === "Ssh" ? conn.transport.ssh.port : 22,
			sshUser: conn.transport.type === "Ssh" ? conn.transport.ssh.user : "",
			identityFile: conn.transport.type === "Ssh" ? (conn.transport.ssh.identity_file ?? "") : "",
			sshServerAliveInterval:
				conn.transport.type === "Ssh" ? conn.transport.ssh.server_alive_interval : empty.sshServerAliveInterval,
			sshServerAliveCountMax:
				conn.transport.type === "Ssh" ? conn.transport.ssh.server_alive_count_max : empty.sshServerAliveCountMax,
			sshStrictHostKeyChecking:
				conn.transport.type === "Ssh" ? conn.transport.ssh.strict_host_key_checking : empty.sshStrictHostKeyChecking,
			remoteDaemonPort: conn.transport.type === "Ssh" ? conn.transport.remote_daemon_port : empty.remoteDaemonPort,
			directUrl: conn.transport.type === "Direct" ? conn.transport.url : "",
			authUsername: conn.auth_username ?? "",
		});
	}

	async function saveEdit(connState: ConnectionState) {
		const f = editForm();
		const transport: RemoteTransport =
			f.transportType === "Ssh"
				? {
						type: "Ssh",
						ssh: {
							host: f.sshHost.trim(),
							port: f.sshPort,
							user: f.sshUser.trim(),
							identity_file: f.identityFile.trim() || null,
							server_alive_interval: f.sshServerAliveInterval,
							server_alive_count_max: f.sshServerAliveCountMax,
							strict_host_key_checking: f.sshStrictHostKeyChecking,
						},
						remote_daemon_port: f.remoteDaemonPort,
					}
				: { type: "Direct", url: f.directUrl.trim() };

		const updated: RemoteConnection = {
			...connState.connection,
			name: f.name.trim(),
			transport,
			auth_username: f.authUsername.trim() || null,
		};

		setSaving(true);
		setError("");
		try {
			// Disconnect first if connected, then save the updated connection
			if (connState.status === "connecting" || connState.status === "connected") {
				await remoteConnectionsStore.disconnect(connState.connection.id);
			}
			await remoteConnectionsStore.addConnection(updated);
			setEditingId(null);
		} catch (e) {
			setError(String(e));
		} finally {
			setSaving(false);
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

	/** Render transport-specific form fields */
	function TransportFields(props: {
		formData: ReturnType<typeof emptyRemoteForm>;
		setFormData: (updater: (f: ReturnType<typeof emptyRemoteForm>) => ReturnType<typeof emptyRemoteForm>) => void;
	}) {
		return (
			<>
				<select
					class={s.input}
					value={props.formData.transportType}
					onChange={(e) =>
						props.setFormData((f) => ({ ...f, transportType: e.currentTarget.value as "Ssh" | "Direct" }))
					}
				>
					<option value="Ssh">SSH</option>
					<option value="Direct">Direct</option>
				</select>
				<Show when={props.formData.transportType === "Ssh"}>
					<input
						type="text"
						class={s.input}
						placeholder="Host (e.g. 192.168.1.100)"
						value={props.formData.sshHost}
						onInput={(e) => props.setFormData((f) => ({ ...f, sshHost: e.currentTarget.value }))}
					/>
					<div style={{ display: "flex", gap: "8px" }}>
						<div style={{ flex: 1 }}>
							<label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Port</label>
							<input
								type="number"
								class={s.input}
								value={props.formData.sshPort}
								min={1}
								max={65535}
								onInput={(e) =>
									props.setFormData((f) => ({ ...f, sshPort: parseInt(e.currentTarget.value, 10) || 22 }))
								}
							/>
						</div>
						<div style={{ flex: 2 }}>
							<label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>User</label>
							<input
								type="text"
								class={s.input}
								placeholder="SSH user"
								value={props.formData.sshUser}
								onInput={(e) => props.setFormData((f) => ({ ...f, sshUser: e.currentTarget.value }))}
							/>
						</div>
					</div>
					<input
						type="text"
						class={s.input}
						placeholder="Identity file (optional, e.g. ~/.ssh/id_rsa)"
						value={props.formData.identityFile}
						onInput={(e) => props.setFormData((f) => ({ ...f, identityFile: e.currentTarget.value }))}
					/>
					<div style={{ display: "flex", gap: "8px", "align-items": "center" }}>
						<label style={{ "font-size": "12px", color: "var(--text-dimmed)", "white-space": "nowrap" }}>
							Remote daemon port
						</label>
						<input
							type="number"
							class={s.input}
							value={props.formData.remoteDaemonPort}
							min={1}
							max={65535}
							style={{ width: "90px" }}
							onInput={(e) =>
								props.setFormData((f) => ({ ...f, remoteDaemonPort: parseInt(e.currentTarget.value, 10) || 9876 }))
							}
						/>
					</div>
				</Show>
				<Show when={props.formData.transportType === "Direct"}>
					<input
						type="text"
						class={s.input}
						placeholder="URL (e.g. http://192.168.1.100:9876)"
						value={props.formData.directUrl}
						onInput={(e) => props.setFormData((f) => ({ ...f, directUrl: e.currentTarget.value }))}
					/>
				</Show>
				<input
					type="text"
					class={s.input}
					placeholder="Auth username (for TUIC API authentication)"
					value={props.formData.authUsername}
					onInput={(e) => props.setFormData((f) => ({ ...f, authUsername: e.currentTarget.value }))}
				/>
			</>
		);
	}

	return (
		<div style={{ "margin-top": "24px", "border-top": "1px solid var(--border)", "padding-top": "16px" }}>
			<div class={s.group}>
				<label style={{ display: "flex", "align-items": "center", gap: "8px", "justify-content": "space-between" }}>
					<span>Remote Machines</span>
					<button
						class={s.copyBtn}
						onClick={() => {
							setShowAdd((v) => !v);
							setError("");
						}}
						title="Add remote machine"
						style={{ "font-size": "18px", "line-height": 1 }}
					>
						{showAdd() ? "−" : "+"}
					</button>
				</label>
				<p class={s.hint}>
					Connect to TUIC instances running on other machines. SSH connections create an encrypted tunnel automatically.
				</p>
			</div>

			{/* Add form */}
			<Show when={showAdd()}>
				<div
					class={s.group}
					style={{ background: "var(--bg-secondary, rgba(255,255,255,0.03))", padding: "12px", "border-radius": "6px" }}
				>
					<div style={{ display: "grid", gap: "8px" }}>
						<input
							type="text"
							class={s.input}
							placeholder="Name (e.g. dev-server, staging)"
							value={form().name}
							onInput={(e) => setForm((f) => ({ ...f, name: e.currentTarget.value }))}
						/>
						<TransportFields formData={form()} setFormData={setForm} />
						<div style={{ display: "flex", gap: "8px", "justify-content": "flex-end" }}>
							<button class={s.copyBtn} onClick={addConnection} disabled={saving()}>
								{saving() ? "Saving..." : "Save"}
							</button>
							<button
								class={s.copyBtn}
								onClick={() => {
									setShowAdd(false);
									setForm(emptyRemoteForm());
									setError("");
								}}
							>
								Cancel
							</button>
						</div>
						<Show when={error()}>
							<p class={s.hint} style={{ color: "var(--error, #e06c75)" }}>
								{error()}
							</p>
						</Show>
					</div>
				</div>
			</Show>

			{/* Empty state */}
			<Show when={connectionList().length === 0 && !showAdd()}>
				<p class={s.hint} style={{ color: "var(--text-dimmed)" }}>
					No remote machines configured. Click <strong>+</strong> to add one.
				</p>
			</Show>

			{/* Connection list */}
			<For each={connectionList()}>
				{(connState) => {
					const conn = () => connState.connection;
					const isEditing = () => editingId() === conn().id;
					return (
						<div style={{ "border-bottom": "1px solid var(--border-subtle, rgba(255,255,255,0.06))" }}>
							<div class={s.group} style={{ display: "flex", "align-items": "center", gap: "8px", padding: "8px 0" }}>
								{/* Status dot */}
								<span
									style={{
										display: "inline-block",
										width: "8px",
										height: "8px",
										"border-radius": "50%",
										background: remoteStatusColor(connState.status),
										"flex-shrink": "0",
									}}
									title={remoteStatusLabel(connState.status)}
								/>
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
													conn().transport.type === "Ssh" ? "rgba(97,175,239,0.15)" : "rgba(152,195,121,0.15)",
												color: conn().transport.type === "Ssh" ? "#61afef" : "#98c379",
											}}
										>
											{transportBadgeLabel(conn().transport)}
										</span>
										<Show when={connState.status !== "disconnected"}>
											<span style={{ "font-size": "11px", color: remoteStatusColor(connState.status) }}>
												{remoteStatusLabel(connState.status)}
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
										<div class={s.hint} style={{ margin: 0, "font-size": "11px", color: "var(--accent-red, #ef4444)" }}>
											{connState.error}
										</div>
									</Show>
								</div>
								{/* Connect / Disconnect */}
								<button
									class={s.copyBtn}
									style={{ "flex-shrink": 0, "white-space": "nowrap" }}
									onClick={() => {
										if (connState.status === "connected" || connState.status === "connecting") {
											remoteConnectionsStore.disconnect(conn().id);
										} else {
											remoteConnectionsStore.connect(conn().id);
										}
									}}
								>
									{connState.status === "connected" || connState.status === "connecting" ? "Disconnect" : "Connect"}
								</button>
								{/* Edit */}
								<button
									class={s.copyBtn}
									style={{ "flex-shrink": 0 }}
									title="Edit"
									onClick={() => (isEditing() ? setEditingId(null) : startEdit(conn()))}
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
							{/* Edit inline panel */}
							<Show when={isEditing()}>
								<div
									style={{
										background: "var(--bg-secondary, rgba(255,255,255,0.03))",
										padding: "12px",
										"border-radius": "6px",
										"margin-bottom": "8px",
									}}
								>
									<div style={{ display: "grid", gap: "8px" }}>
										<TransportFields formData={editForm()} setFormData={setEditForm} />
										<div style={{ display: "flex", gap: "8px", "justify-content": "flex-end" }}>
											<button class={s.copyBtn} onClick={() => saveEdit(connState)} disabled={saving()}>
												{saving() ? "Saving..." : "Save"}
											</button>
											<button
												class={s.copyBtn}
												onClick={() => {
													setEditingId(null);
													setError("");
												}}
											>
												Cancel
											</button>
										</div>
										<Show when={error()}>
											<p class={s.hint} style={{ color: "var(--error, #e06c75)", margin: "4px 0 0" }}>
												{error()}
											</p>
										</Show>
									</div>
								</div>
							</Show>
						</div>
					);
				}}
			</For>
		</div>
	);
};
