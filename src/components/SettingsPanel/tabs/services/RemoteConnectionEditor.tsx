import { type Component, createSignal, onMount, Show } from "solid-js";
import type {
	DeployMode,
	DiscoveredSshHost,
	RemoteConnection,
	RemoteTransport,
	TestConnectionRequest,
} from "../../../../stores/remoteConnections";
import { remoteConnectionsStore } from "../../../../stores/remoteConnections";
import type { ForwardSpec, SshConnectionParams, TunnelProfile } from "../../../../stores/tunnels";
import { tunnelsStore } from "../../../../stores/tunnels";
import { randomId } from "../../../../utils/randomId";
import d from "../../../shared/dialog.module.css";
import { SshConnectionFields } from "../../../shared/SshConnectionFields";
import { normalizeForwardForType, PortForwardsEditor } from "../../../TunnelsPanel/PortForwardsEditor";
import s from "../../Settings.module.css";

export type ConnectionKind = "SshTunnel" | "RemoteSsh" | "RemoteDirect" | "RemoteLocal";

/** A new connection prefilled from a discovered SSH host (Remote Server — SSH). */
export interface ConnectionPrefill {
	name: string;
	ssh: Pick<SshConnectionParams, "host" | "port" | "user">;
}

export type EditorTarget =
	| { kind: "new"; prefill?: ConnectionPrefill }
	| { kind: "edit-tunnel"; profile: TunnelProfile }
	| { kind: "edit-connection"; connection: RemoteConnection };

export interface RemoteConnectionEditorProps {
	target: EditorTarget;
	onClose: () => void;
}

/** New-SSH defaults. `strict_host_key_checking` is a tunnel profile's default;
 * a Remote Server kind always saves `AcceptNew` (see `hostKeyFor`). */
export function defaultSsh(): SshConnectionParams {
	return {
		host: "",
		port: 22,
		user: "",
		identity_file: null,
		server_alive_interval: 15,
		server_alive_count_max: 3,
		strict_host_key_checking: "Yes",
		compression: true,
	};
}

/** `RemoteConnection::new_ssh`'s Rust default / what a freshly-started
 * `tuic-remote` binary actually listens on (`TUIC_PORT` fallback). */
export const DEFAULT_REMOTE_DAEMON_PORT = 9877;

/** A new connection's defaults for the Remote Server fields the SSH form does not cover. */
export const DEFAULT_DEPLOY: DeployMode = "never";
export const DEFAULT_SURVIVE_MINUTES = 30;

/** Editor prefill for a discovered SSH host — matched by the name to connect with. */
export function prefillFromDiscoveredHost(host: DiscoveredSshHost): ConnectionPrefill {
	return { name: host.host, ssh: { host: host.host, port: host.port ?? 22, user: host.user ?? "" } };
}

/**
 * The host-key policy a saved connection of this kind actually runs with. The
 * tunnel TUIC opens for a Remote Server is created on the user's behalf and
 * always uses accept-new (`remote_runtime::ssh_profile`), so a saved Remote
 * Server says what runs; a tunnel profile keeps what the user picked.
 */
export function hostKeyFor(kind: ConnectionKind, picked: SshConnectionParams["strict_host_key_checking"]) {
	return kind === "SshTunnel" ? picked : "AcceptNew";
}

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
	return target.prefill ? "RemoteSsh" : "SshTunnel";
}

function initialSsh(target: EditorTarget): SshConnectionParams {
	if (target.kind === "edit-tunnel") return target.profile.ssh;
	if (target.kind === "edit-connection" && target.connection.transport.type === "Ssh") {
		return target.connection.transport.ssh;
	}
	if (target.kind === "new" && target.prefill) return { ...defaultSsh(), ...target.prefill.ssh };
	return defaultSsh();
}

function initialName(target: EditorTarget): string {
	if (target.kind === "edit-tunnel") return target.profile.name;
	if (target.kind === "edit-connection") return target.connection.name;
	return target.prefill?.name ?? "";
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
 * onto "Save" — they're different backing stores/id spaces). "Add Connection"
 * always starts fresh with Kind selectable, defaulting to "SSH Tunnel".
 *
 * A Remote Server keeps every field the connection list can act on: the
 * deployment mode and ephemeral-daemon lifetime (SSH only), auto-update, and
 * the vault password (blank keeps the stored one; "Clear stored password"
 * forgets it). Saving an edited connection that is live disconnects it first,
 * then stores the password once the connection exists — the vault key is its id.
 */
export const RemoteConnectionEditor: Component<RemoteConnectionEditorProps> = (props) => {
	const target = props.target;
	const isEdit = target.kind !== "new";
	const existingProfileId = target.kind === "edit-tunnel" ? target.profile.id : null;
	const existingConnection = target.kind === "edit-connection" ? target.connection : null;

	const [kind, setKind] = createSignal<ConnectionKind>(initialKind(target));
	const [name, setName] = createSignal(initialName(target));
	const [ssh, setSsh] = createSignal<SshConnectionParams>(initialSsh(target));
	const [forwards, setForwards] = createSignal<ForwardSpec[]>(
		target.kind === "edit-tunnel" ? target.profile.forwards : [],
	);
	const [autoConnect, setAutoConnect] = createSignal(
		target.kind === "edit-tunnel" ? target.profile.auto_connect : false,
	);
	const [remoteDaemonPort, setRemoteDaemonPort] = createSignal(
		existingConnection?.transport.type === "Ssh"
			? existingConnection.transport.remote_daemon_port
			: DEFAULT_REMOTE_DAEMON_PORT,
	);
	const [directUrl, setDirectUrl] = createSignal(
		existingConnection?.transport.type === "Direct" ? existingConnection.transport.url : "",
	);
	const initialLocal = existingConnection?.transport.type === "Local" ? existingConnection.transport : null;
	const [localMode, setLocalMode] = createSignal<"instance" | "port">(initialLocal?.instance_id ? "instance" : "port");
	const [localInstanceId, setLocalInstanceId] = createSignal(initialLocal?.instance_id ?? "");
	const [localPort, setLocalPort] = createSignal(initialLocal?.port ?? DEFAULT_REMOTE_DAEMON_PORT);
	const [deploy, setDeploy] = createSignal<DeployMode>(existingConnection?.deploy ?? DEFAULT_DEPLOY);
	const [surviveMinutes, setSurviveMinutes] = createSignal(
		existingConnection ? Math.max(1, Math.round(existingConnection.survive_secs / 60)) : DEFAULT_SURVIVE_MINUTES,
	);
	const [autoUpdate, setAutoUpdate] = createSignal(existingConnection?.auto_update ?? false);
	const [authUsername, setAuthUsername] = createSignal(existingConnection?.auth_username ?? "");
	// Plaintext, in-progress only — never pre-filled from a saved connection
	// (the password is never round-tripped out of the vault for display).
	const [password, setPassword] = createSignal("");
	const [passwordExists, setPasswordExists] = createSignal(false);

	const [saving, setSaving] = createSignal(false);
	const [error, setError] = createSignal("");
	const [testing, setTesting] = createSignal(false);
	const [testResult, setTestResult] = createSignal<{ ok: boolean; text: string } | null>(null);

	onMount(async () => {
		if (existingConnection) {
			// Best-effort: a vault read failure only hides "Clear stored password".
			setPasswordExists(await remoteConnectionsStore.hasPassword(existingConnection.id).catch(() => false));
		}
	});

	const patchSsh = (patch: Partial<SshConnectionParams>) => setSsh((cur) => ({ ...cur, ...patch }));

	const trimmedSsh = (): SshConnectionParams => ({
		...ssh(),
		host: ssh().host.trim(),
		user: ssh().user.trim(),
		identity_file: ssh().identity_file?.trim() || null,
		strict_host_key_checking: hostKeyFor(kind(), ssh().strict_host_key_checking),
	});

	/** Build the `RemoteTransport` the current form describes, for both Save
	 * and Test Connection. For `SshTunnel`, `remote_daemon_port` is unused by
	 * either caller (`test_connection_impl`'s `Ssh` arm ignores it; Save never
	 * reads this return value for that Kind at all) — 0 is a safe placeholder. */
	function buildTransport(): RemoteTransport {
		if (kind() === "SshTunnel" || kind() === "RemoteSsh") {
			return { type: "Ssh", ssh: trimmedSsh(), remote_daemon_port: kind() === "RemoteSsh" ? remoteDaemonPort() : 0 };
		}
		if (kind() === "RemoteDirect") {
			return { type: "Direct", url: directUrl().trim() };
		}
		return {
			type: "Local",
			port: localMode() === "port" ? localPort() : null,
			instance_id: localMode() === "instance" ? localInstanceId().trim() || null : null,
		};
	}

	/** The form's validation error for the current Kind, or null. */
	function validationError(): string | null {
		if (!name().trim()) return "Name is required";
		if (kind() === "SshTunnel" || kind() === "RemoteSsh") {
			const ssh_ = trimmedSsh();
			if (!ssh_.host || !ssh_.user) return "Host and user are required";
		}
		if (kind() === "RemoteDirect" && !directUrl().trim()) return "URL is required";
		if (kind() === "RemoteLocal" && localMode() === "instance" && !localInstanceId().trim()) {
			return "Instance ID is required";
		}
		return null;
	}

	const handleClearPassword = async () => {
		if (!existingConnection) return;
		try {
			// An empty password is the vault's "forget it" request.
			await remoteConnectionsStore.setPassword(existingConnection.id, "");
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
				password: isRemoteKind ? password() || null : null,
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
		const invalid = validationError();
		if (invalid) {
			setError(invalid);
			return;
		}

		setSaving(true);
		setError("");
		try {
			if (kind() === "SshTunnel") {
				const data = {
					name: name().trim(),
					ssh: trimmedSsh(),
					forwards: forwards().map(normalizeForwardForType),
					auto_connect: autoConnect(),
				};
				if (existingProfileId) {
					await tunnelsStore.updateProfile({ id: existingProfileId, ...data });
				} else {
					await tunnelsStore.createProfile(data);
				}
			} else {
				const isSsh = kind() === "RemoteSsh";
				const conn: RemoteConnection = {
					// Edit keeps every field this form does not own (e.g. `enabled`).
					...(existingConnection ?? { enabled: true }),
					// No prefix: the backend's `validate()` requires `id` to be a bare
					// UUID (`Uuid::parse_str`), rejecting anything else.
					id: existingConnection?.id ?? randomId(""),
					name: name().trim(),
					transport: buildTransport(),
					auth_username: authUsername().trim() || null,
					auto_update: autoUpdate(),
					deploy: isSsh ? deploy() : "never",
					survive_secs: Math.max(60, Math.round(surviveMinutes() * 60)),
				};
				if (existingConnection) {
					const live = remoteConnectionsStore.getConnectionState(existingConnection.id)?.status;
					if (live && live !== "disconnected") await remoteConnectionsStore.disconnect(existingConnection.id);
				}
				await remoteConnectionsStore.addConnection(conn);
				// After the connection exists: the vault key is its id. Blank means
				// "keep what is in the vault" — the field can never show it.
				if (password()) await remoteConnectionsStore.setPassword(conn.id, password());
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
			<label style={{ display: "flex", gap: "8px", "align-items": "center" }}>
				Auto-update remote daemons
				<input
					type="checkbox"
					style={{ order: -1 }}
					checked={autoUpdate()}
					onChange={(e) => setAutoUpdate(e.currentTarget.checked)}
				/>
			</label>
			<div class={s.group}>
				<label class={s.label}>Auth username (optional)</label>
				<input value={authUsername()} onInput={(e) => setAuthUsername(e.currentTarget.value)} />
			</div>
			<div class={s.group}>
				<label class={s.label}>Auth password (optional)</label>
				<input
					type="password"
					autocomplete="off"
					value={password()}
					onInput={(e) => setPassword(e.currentTarget.value)}
					placeholder={passwordExists() ? "Password stored — leave blank to keep it" : "No password set"}
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
				<p class={s.hint} style={{ margin: "4px 0 0" }}>
					The password is kept in the OS credential vault, never in connections.json. It is traded for the daemon's
					session token on every connect — <code>tuic-remote</code> authenticates every request, so a connection without
					one reaches only <code>/health</code>.
				</p>
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
					<label class={s.label}>Name</label>
					<input value={name()} onInput={(e) => setName(e.currentTarget.value)} />
				</div>

				<div class={s.group}>
					<label class={s.label}>Kind</label>
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
					<SshConnectionFields value={ssh()} onChange={patchSsh} hostKeyChecking="enforced-accept-new" />
					<div class={s.group}>
						<label class={s.label}>Remote daemon port</label>
						<input
							type="number"
							min={1}
							max={65535}
							value={remoteDaemonPort()}
							onInput={(e) =>
								setRemoteDaemonPort(Number.parseInt(e.currentTarget.value, 10) || DEFAULT_REMOTE_DAEMON_PORT)
							}
						/>
					</div>
					<div class={s.group}>
						<label class={s.label}>Deployment</label>
						<select value={deploy()} onChange={(e) => setDeploy(e.currentTarget.value as DeployMode)}>
							<option value="never">Never deploy</option>
							<option value="on_connect">Deploy on connect</option>
							<option value="installed">Installed service</option>
						</select>
					</div>
					<div class={s.group}>
						<label class={s.label}>Keep ephemeral daemon alive (minutes)</label>
						<input
							type="number"
							min={1}
							value={surviveMinutes()}
							onInput={(e) => setSurviveMinutes(Math.max(1, Number.parseInt(e.currentTarget.value, 10) || 1))}
						/>
					</div>
					<AuthFields />
				</Show>

				<Show when={kind() === "RemoteDirect"}>
					<div class={s.group}>
						<label class={s.label}>URL</label>
						<input
							placeholder="http://192.168.1.100:9877"
							value={directUrl()}
							onInput={(e) => setDirectUrl(e.currentTarget.value)}
						/>
					</div>
					<AuthFields />
				</Show>

				<Show when={kind() === "RemoteLocal"}>
					<div class={s.group}>
						<label class={s.label}>Target</label>
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
							<label class={s.label}>Instance ID</label>
							<input
								placeholder="e.g. dev-box"
								value={localInstanceId()}
								onInput={(e) => setLocalInstanceId(e.currentTarget.value)}
							/>
						</div>
					</Show>
					<Show when={localMode() === "port"}>
						<div class={s.group}>
							<label class={s.label}>Port</label>
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
