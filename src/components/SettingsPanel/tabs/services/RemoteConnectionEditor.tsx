import { type Component, createSignal, onMount, Show } from "solid-js";
import type { RemoteConnection, RemoteTransport, TestConnectionRequest } from "../../../../stores/remoteConnections";
import { remoteConnectionsStore } from "../../../../stores/remoteConnections";
import type { ForwardSpec, SshConnectionParams, TunnelProfile } from "../../../../stores/tunnels";
import { tunnelsStore } from "../../../../stores/tunnels";
import { randomId } from "../../../../utils/randomId";
import d from "../../../shared/dialog.module.css";
import { SshConnectionFields } from "../../../shared/SshConnectionFields";
import { normalizeForwardForType, PortForwardsEditor } from "../../../TunnelsPanel/PortForwardsEditor";
import s from "../../Settings.module.css";

export type ConnectionKind = "SshTunnel" | "RemoteSsh" | "RemoteDirect" | "RemoteLocal";

export type EditorTarget =
	| { kind: "new" }
	| { kind: "edit-tunnel"; profile: TunnelProfile }
	| { kind: "edit-connection"; connection: RemoteConnection };

export interface RemoteConnectionEditorProps {
	target: EditorTarget;
	onClose: () => void;
}

function defaultSsh(): SshConnectionParams {
	return {
		host: "",
		port: 22,
		user: "",
		identity_file: null,
		server_alive_interval: 15,
		server_alive_count_max: 3,
		strict_host_key_checking: "Yes",
	};
}

/** `RemoteConnection::new_ssh`'s Rust default / what a freshly-started
 * `tuic-remote` binary actually listens on (`TUIC_PORT` fallback). */
const DEFAULT_REMOTE_DAEMON_PORT = 9877;

function initialKind(target: EditorTarget): ConnectionKind {
	if (target.kind === "edit-tunnel") return "SshTunnel";
	if (target.kind === "edit-connection") {
		switch (target.connection.transport.type) {
			case "Ssh":
				return "RemoteSsh";
			case "Direct":
				return "RemoteDirect";
			case "Local":
				return "RemoteLocal";
		}
	}
	return "SshTunnel";
}

function initialSsh(target: EditorTarget): SshConnectionParams {
	if (target.kind === "edit-tunnel") return target.profile.ssh;
	if (target.kind === "edit-connection" && target.connection.transport.type === "Ssh") {
		return target.connection.transport.ssh;
	}
	return defaultSsh();
}

/**
 * The merged connection editor — one form, "Name" + a "Kind" dropdown with
 * exactly four options ("SSH Tunnel" / "Remote Server — SSH" /
 * "Remote Server — Direct" / "Remote Server — Local"); the rest of the form
 * swaps based on Kind, sharing `SshConnectionFields` between "SSH Tunnel" and
 * "Remote Server — SSH". Saves to `tunnels/*.toml` (via `tunnelsStore`) or
 * `connections.json` (via `remoteConnectionsStore`) depending on Kind — an
 * implementation detail invisible to the user.
 *
 * Story: SSH Tunnels + Remote Servers consolidation, "Merged connection model".
 *
 * Editing an existing item fixes the Kind (switching an existing tunnel
 * profile's Kind to a Remote Server kind, or vice versa, doesn't map cleanly
 * onto "Save" — they're different backing stores/id spaces) — a judgment call
 * the plan didn't fully specify. "Add Connection" always starts fresh with
 * Kind selectable, defaulting to "SSH Tunnel".
 */
export const RemoteConnectionEditor: Component<RemoteConnectionEditorProps> = (props) => {
	const target = props.target;
	const isEdit = target.kind !== "new";
	const existingProfileId = target.kind === "edit-tunnel" ? target.profile.id : null;
	const existingConnectionId = target.kind === "edit-connection" ? target.connection.id : null;

	const [kind, setKind] = createSignal<ConnectionKind>(initialKind(target));
	const [name, setName] = createSignal(
		target.kind === "edit-tunnel"
			? target.profile.name
			: target.kind === "edit-connection"
				? target.connection.name
				: "",
	);
	const [ssh, setSsh] = createSignal<SshConnectionParams>(initialSsh(target));
	const [forwards, setForwards] = createSignal<ForwardSpec[]>(
		target.kind === "edit-tunnel" ? target.profile.forwards : [],
	);
	const [autoConnect, setAutoConnect] = createSignal(
		target.kind === "edit-tunnel" ? target.profile.auto_connect : false,
	);
	const [remoteDaemonPort, setRemoteDaemonPort] = createSignal(
		target.kind === "edit-connection" && target.connection.transport.type === "Ssh"
			? target.connection.transport.remote_daemon_port
			: DEFAULT_REMOTE_DAEMON_PORT,
	);
	const [startIfNotRunning, setStartIfNotRunning] = createSignal(
		target.kind === "edit-connection" && target.connection.transport.type === "Ssh"
			? target.connection.transport.start_if_not_running
			: false,
	);
	const [leaveRunningOnDisconnect, setLeaveRunningOnDisconnect] = createSignal(
		target.kind === "edit-connection" && target.connection.transport.type === "Ssh"
			? target.connection.transport.leave_running_on_disconnect
			: false,
	);
	const [sshInstanceId, setSshInstanceId] = createSignal(
		target.kind === "edit-connection" && target.connection.transport.type === "Ssh"
			? (target.connection.transport.instance_id ?? "")
			: "",
	);
	const [directUrl, setDirectUrl] = createSignal(
		target.kind === "edit-connection" && target.connection.transport.type === "Direct"
			? target.connection.transport.url
			: "",
	);
	// Never edited directly here — pinned only through the Connect-time
	// confirmation dialog (Phase 4). Carried through on Save so re-saving an
	// existing Direct connection's other fields doesn't silently un-pin it;
	// reset to null whenever the URL changes, since a pin is only valid for
	// the exact URL it was captured against.
	const [directTlsFingerprint, setDirectTlsFingerprint] = createSignal<string | null>(
		target.kind === "edit-connection" && target.connection.transport.type === "Direct"
			? target.connection.transport.tls_fingerprint
			: null,
	);
	const initialLocal =
		target.kind === "edit-connection" && target.connection.transport.type === "Local"
			? target.connection.transport
			: null;
	const [localMode, setLocalMode] = createSignal<"instance" | "port">(initialLocal?.instance_id ? "instance" : "port");
	const [localInstanceId, setLocalInstanceId] = createSignal(initialLocal?.instance_id ?? "");
	const [localPort, setLocalPort] = createSignal(initialLocal?.port ?? DEFAULT_REMOTE_DAEMON_PORT);
	const [authUsername, setAuthUsername] = createSignal(
		target.kind === "edit-connection" ? (target.connection.auth_username ?? "") : "",
	);
	// Plaintext, in-progress only — never pre-filled from a saved connection
	// (the password is never round-tripped out of the keyring for display).
	const [password, setPassword] = createSignal("");
	const [passwordExists, setPasswordExists] = createSignal(false);

	const [saving, setSaving] = createSignal(false);
	const [error, setError] = createSignal("");
	const [testing, setTesting] = createSignal(false);
	const [testResult, setTestResult] = createSignal<{ ok: boolean; text: string } | null>(null);

	onMount(async () => {
		if (existingConnectionId) {
			try {
				setPasswordExists(await remoteConnectionsStore.connectionPasswordExists(existingConnectionId));
			} catch {
				// best-effort — a keyring read failure just means the "Clear stored
				// password" affordance stays hidden, not a blocking error.
			}
		}
	});

	const patchSsh = (patch: Partial<SshConnectionParams>) => setSsh((cur) => ({ ...cur, ...patch }));

	const trimmedSsh = (): SshConnectionParams => ({
		...ssh(),
		host: ssh().host.trim(),
		user: ssh().user.trim(),
		identity_file: ssh().identity_file?.trim() || null,
	});

	/** Build the `RemoteTransport` the current form describes, for both Save
	 * and Test Connection. For `SshTunnel`, `remote_daemon_port` is unused by
	 * either caller (`test_connection_impl`'s `Ssh` arm ignores it; Save never
	 * reads this return value for that Kind at all) — 0 is a safe placeholder. */
	function buildTransport(): RemoteTransport {
		if (kind() === "SshTunnel" || kind() === "RemoteSsh") {
			return {
				type: "Ssh",
				ssh: trimmedSsh(),
				remote_daemon_port: kind() === "RemoteSsh" ? remoteDaemonPort() : 0,
				start_if_not_running: kind() === "RemoteSsh" && startIfNotRunning(),
				leave_running_on_disconnect: kind() === "RemoteSsh" && leaveRunningOnDisconnect(),
				instance_id: kind() === "RemoteSsh" ? sshInstanceId().trim() || null : null,
			};
		}
		if (kind() === "RemoteDirect") {
			return { type: "Direct", url: directUrl().trim(), tls_fingerprint: directTlsFingerprint() };
		}
		return {
			type: "Local",
			port: localMode() === "port" ? localPort() : null,
			instance_id: localMode() === "instance" ? localInstanceId().trim() || null : null,
		};
	}

	const handleClearPassword = async () => {
		if (!existingConnectionId) return;
		try {
			await remoteConnectionsStore.deleteConnectionPassword(existingConnectionId);
			setPasswordExists(false);
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		}
	};

	const handleTest = async () => {
		setTesting(true);
		setTestResult(null);
		try {
			const isRemoteKind = kind() !== "SshTunnel";
			const request: TestConnectionRequest = {
				transport: buildTransport(),
				auth_username: isRemoteKind ? authUsername().trim() || null : null,
				password: isRemoteKind ? password().trim() || null : null,
			};
			const result = await remoteConnectionsStore.testConnection(request);
			setTestResult(describeTestResult(result));
		} catch (err) {
			setTestResult({ ok: false, text: err instanceof Error ? err.message : String(err) });
		} finally {
			setTesting(false);
		}
	};

	const handleSave = async () => {
		const trimmedName = name().trim();
		if (!trimmedName) {
			setError("Name is required");
			return;
		}

		setSaving(true);
		setError("");
		try {
			if (kind() === "SshTunnel") {
				const ssh_ = trimmedSsh();
				if (!ssh_.host || !ssh_.user) {
					setError("Host and user are required");
					setSaving(false);
					return;
				}
				const data = {
					name: trimmedName,
					ssh: ssh_,
					forwards: forwards().map(normalizeForwardForType),
					auto_connect: autoConnect(),
				};
				if (existingProfileId) {
					await tunnelsStore.updateProfile({ id: existingProfileId, ...data });
				} else {
					await tunnelsStore.createProfile(data);
				}
			} else {
				let transport: RemoteTransport;
				if (kind() === "RemoteSsh") {
					const ssh_ = trimmedSsh();
					if (!ssh_.host || !ssh_.user) {
						setError("Host and user are required");
						setSaving(false);
						return;
					}
					transport = {
						type: "Ssh",
						ssh: ssh_,
						remote_daemon_port: remoteDaemonPort(),
						start_if_not_running: startIfNotRunning(),
						leave_running_on_disconnect: leaveRunningOnDisconnect(),
						instance_id: sshInstanceId().trim() || null,
					};
				} else if (kind() === "RemoteDirect") {
					const url = directUrl().trim();
					if (!url) {
						setError("URL is required");
						setSaving(false);
						return;
					}
					transport = { type: "Direct", url, tls_fingerprint: directTlsFingerprint() };
				} else {
					if (localMode() === "instance") {
						const id = localInstanceId().trim();
						if (!id) {
							setError("Instance ID is required");
							setSaving(false);
							return;
						}
						transport = { type: "Local", port: null, instance_id: id };
					} else {
						transport = { type: "Local", port: localPort(), instance_id: null };
					}
				}

				// No prefix: the backend's `validate()` requires `id` to be a bare
				// UUID (`Uuid::parse_str`), rejecting anything else.
				const id = existingConnectionId ?? randomId("");
				const conn: RemoteConnection = {
					id,
					name: trimmedName,
					transport,
					auth_username: authUsername().trim() || null,
					enabled: true,
				};
				await remoteConnectionsStore.addConnection(conn);
				if (password().trim()) {
					await remoteConnectionsStore.saveConnectionPassword(id, password().trim());
				}
			}
			props.onClose();
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setSaving(false);
		}
	};

	const AuthFields: Component = () => (
		<>
			<div class={s.group}>
				<label>Auth username (optional)</label>
				<input value={authUsername()} onInput={(e) => setAuthUsername(e.currentTarget.value)} />
			</div>
			<div class={s.group}>
				<label>Auth password (optional)</label>
				<input
					type="password"
					value={password()}
					onInput={(e) => setPassword(e.currentTarget.value)}
					placeholder={passwordExists() ? "Password set — leave blank to keep it" : "No password set"}
				/>
				<Show when={passwordExists()}>
					<button
						type="button"
						class={d.cancelBtn}
						style={{
							"margin-top": "6px",
							padding: "2px 8px",
							"font-size": "var(--font-sm)",
							border: "none",
							"border-radius": "var(--radius-md)",
							cursor: "pointer",
						}}
						onClick={handleClearPassword}
					>
						Clear stored password
					</button>
				</Show>
			</div>
		</>
	);

	return (
		<div
			class={s.group}
			style={{ background: "var(--bg-secondary, rgba(255,255,255,0.03))", padding: "12px", "border-radius": "6px" }}
		>
			<div style={{ display: "grid", gap: "8px" }}>
				<div class={s.group}>
					<label>Name</label>
					<input value={name()} onInput={(e) => setName(e.currentTarget.value)} />
				</div>

				<div class={s.group}>
					<label>Kind</label>
					<select value={kind()} disabled={isEdit} onChange={(e) => setKind(e.currentTarget.value as ConnectionKind)}>
						<option value="SshTunnel">SSH Tunnel</option>
						<option value="RemoteSsh">Remote Server — SSH</option>
						<option value="RemoteDirect">Remote Server — Direct</option>
						<option value="RemoteLocal">Remote Server — Local</option>
					</select>
				</div>

				<Show when={kind() === "SshTunnel"}>
					<SshConnectionFields value={ssh()} onChange={patchSsh} />
					<PortForwardsEditor forwards={forwards()} onChange={setForwards} defaultRemoteHost={ssh().host.trim()} />
					<label class={s.toggle}>
						<input type="checkbox" checked={autoConnect()} onChange={(e) => setAutoConnect(e.currentTarget.checked)} />
						<span>Connect automatically on startup</span>
					</label>
				</Show>

				<Show when={kind() === "RemoteSsh"}>
					<SshConnectionFields value={ssh()} onChange={patchSsh} />
					<div class={s.group}>
						<label>Remote daemon port</label>
						<input
							type="number"
							value={remoteDaemonPort()}
							onInput={(e) =>
								setRemoteDaemonPort(Number.parseInt(e.currentTarget.value, 10) || DEFAULT_REMOTE_DAEMON_PORT)
							}
						/>
					</div>
					<div class={s.group}>
						<label>Instance ID (optional)</label>
						<input
							placeholder="e.g. dev-box"
							value={sshInstanceId()}
							onInput={(e) => setSshInstanceId(e.currentTarget.value)}
						/>
						<p class={s.hint}>
							Passed as <code>--instance</code> when starting or configuring the remote daemon ourselves — not used to
							discover an existing port, only when this app launches or configures it.
						</p>
					</div>
					<AuthFields />
					<label class={s.toggle}>
						<input
							type="checkbox"
							checked={startIfNotRunning()}
							onChange={(e) => setStartIfNotRunning(e.currentTarget.checked)}
						/>
						<span>Start remote daemon if not running</span>
					</label>
					<Show when={startIfNotRunning()}>
						<label class={s.toggle}>
							<input
								type="checkbox"
								checked={leaveRunningOnDisconnect()}
								onChange={(e) => setLeaveRunningOnDisconnect(e.currentTarget.checked)}
							/>
							<span>Leave daemon running on disconnect</span>
						</label>
					</Show>
				</Show>

				<Show when={kind() === "RemoteDirect"}>
					<div class={s.group}>
						<label>URL</label>
						<input
							placeholder="http://192.168.1.100:9877"
							value={directUrl()}
							onInput={(e) => {
								setDirectUrl(e.currentTarget.value);
								// A pinned fingerprint is only valid for the exact URL it was
								// captured against — editing the URL invalidates it; Connect
								// will re-probe and re-confirm against the new target.
								setDirectTlsFingerprint(null);
							}}
						/>
						<Show when={directTlsFingerprint()}>
							<p class={s.hint}>
								Certificate pinned: <code>{directTlsFingerprint()}</code>
							</p>
						</Show>
					</div>
					<AuthFields />
				</Show>

				<Show when={kind() === "RemoteLocal"}>
					<div class={s.group}>
						<label>Target</label>
						<div style={{ display: "flex", gap: "16px", "align-items": "center" }}>
							<label style={{ display: "flex", gap: "4px", "align-items": "center" }}>
								<input
									type="radio"
									name="remote-local-mode"
									checked={localMode() === "instance"}
									onChange={() => setLocalMode("instance")}
								/>
								Named instance
							</label>
							<label style={{ display: "flex", gap: "4px", "align-items": "center" }}>
								<input
									type="radio"
									name="remote-local-mode"
									checked={localMode() === "port"}
									onChange={() => setLocalMode("port")}
								/>
								Manual port
							</label>
						</div>
					</div>
					<Show when={localMode() === "instance"}>
						<div class={s.group}>
							<label>Instance ID</label>
							<input
								placeholder="e.g. dev-box"
								value={localInstanceId()}
								onInput={(e) => setLocalInstanceId(e.currentTarget.value)}
							/>
						</div>
					</Show>
					<Show when={localMode() === "port"}>
						<div class={s.group}>
							<label>Port</label>
							<input
								type="number"
								value={localPort()}
								onInput={(e) => setLocalPort(Number.parseInt(e.currentTarget.value, 10) || DEFAULT_REMOTE_DAEMON_PORT)}
							/>
						</div>
					</Show>
					<AuthFields />
				</Show>

				<div style={{ display: "flex", gap: "8px", "align-items": "center" }}>
					<button type="button" class={s.copyBtn} onClick={handleTest} disabled={testing()}>
						{testing() ? "Testing..." : "Test Connection"}
					</button>
					<Show when={testResult()}>
						{(result) => (
							<span
								style={{
									"font-size": "var(--font-sm)",
									color: result().ok ? "var(--success)" : "var(--error, #e06c75)",
								}}
							>
								{result().text}
							</span>
						)}
					</Show>
				</div>

				<div style={{ display: "flex", gap: "8px", "justify-content": "flex-end" }}>
					<button class={s.copyBtn} onClick={handleSave} disabled={saving()}>
						{saving() ? "Saving..." : "Save"}
					</button>
					<button class={s.copyBtn} onClick={props.onClose}>
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
	);
};

/** Human-readable classification of a `ConnectionTestResult` for the UI —
 * mirrors `ConnectionTestResult`'s Rust variants exactly (Phase 2's design). */
function describeTestResult(result: Awaited<ReturnType<typeof remoteConnectionsStore.testConnection>>): {
	ok: boolean;
	text: string;
} {
	switch (result.type) {
		case "Reachable":
			return { ok: true, text: "Reachable" };
		case "AuthFailed":
			return { ok: false, text: "Reachable, but authentication failed" };
		case "NotConfigured":
			return { ok: false, text: "Reachable, but not configured yet (no username/password set on the target)" };
		case "InstanceNotFound":
			return { ok: false, text: "Instance not found" };
		default:
			return { ok: false, text: `Unreachable: ${result.reason}` };
	}
}
