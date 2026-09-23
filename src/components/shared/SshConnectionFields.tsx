import { type Component, createSignal, createUniqueId, For, onMount, Show } from "solid-js";
import { invoke } from "../../invoke";
import { settingOptions } from "../../services/settingsCatalogue";
import type { SshConnectionParams } from "../../stores/tunnels";
import { isTauri } from "../../transport";
import { openDialog } from "../../utils/nativeDialog";
import s from "../SettingsPanel/Settings.module.css";
import layout from "../TunnelsPanel/TunnelEditorModal.module.css";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import d from "./dialog.module.css";

interface AgentKey {
	fingerprint: string;
	comment: string;
	key_type: string;
}

interface SshAgentInfo {
	keys: AgentKey[];
	agent_type: string;
}

export interface SshConnectionFieldsProps {
	/** Which settings owner the edited params belong to: a tunnel profile
	 * (`tunnels.profiles.{id}.ssh.*`) or a remote connection's SSH transport
	 * (`connections.connections.{id}.transport.ssh.*`). Names each control's
	 * catalogue identity (`data-setting-id`). */
	owner: "tunnel" | "connection";
	value: SshConnectionParams;
	onChange: (patch: Partial<SshConnectionParams>) => void;
	/**
	 * `"enforced-accept-new"` for a Remote Server connection: the tunnel TUIC
	 * opens on the user's behalf always runs with `accept-new`
	 * (`remote_runtime::ssh_profile`), so offering `Yes` there would be a
	 * choice the runtime silently ignores. A tunnel profile honours both values
	 * and keeps the selectable default.
	 */
	hostKeyChecking?: "selectable" | "enforced-accept-new";
}

/**
 * Shared SSH connection form — host (with `~/.ssh/config` host-alias
 * autocomplete), port, user, identity file (with a Browse button and live
 * SSH-agent-detection display, including each key's fingerprint), keepalive
 * tuning (ServerAliveInterval/ServerAliveCountMax), StrictHostKeyChecking and
 * channel compression (`ssh -C`).
 *
 * Extracted from `TunnelEditorModal.tsx` (story: SSH Tunnels + Remote Servers
 * consolidation, "Merged connection model") so identical SSH capability is
 * available everywhere SSH config is edited: both "SSH Tunnel" (port
 * forwarding profiles) and "Remote Server — SSH" kinds share this exact
 * component rather than two independently-drifting copies. Operates on the
 * `SshConnectionParams` shape shared by `TunnelProfile.ssh` and
 * `RemoteTransport::Ssh.ssh`.
 *
 * Must work in browser mode too (the merged Remote Servers editor has no
 * desktop-only gate). The key-file picker is a native panel
 * (`utils/nativeDialog`, never `@tauri-apps/plugin-dialog`'s `open` — see that
 * module's doc), so Browse… only renders on desktop; the path field itself
 * works everywhere.
 */
export const SshConnectionFields: Component<SshConnectionFieldsProps> = (props) => {
	const hostsListId = createUniqueId();
	const [sshHosts, setSshHosts] = createSignal<string[]>([]);
	const [agentInfo, setAgentInfo] = createSignal<SshAgentInfo>({ keys: [], agent_type: "" });

	onMount(async () => {
		try {
			const [hosts, info] = await Promise.all([
				invoke<string[]>("list_ssh_config_hosts").catch(() => []),
				invoke<SshAgentInfo>("list_ssh_agent_keys").catch(() => ({ keys: [], agent_type: "" })),
			]);
			if (hosts?.length) setSshHosts(hosts);
			setAgentInfo(info);
		} catch {
			// best-effort
		}
	});

	const browseIdentityFile = async () => {
		const home = await invoke<string | null>("resolve_terminal_path", { path: "~/.ssh" }).catch(() => null);
		const selected = await openDialog({
			title: "Select SSH Identity File",
			defaultPath: home ?? undefined,
			multiple: false,
		});
		if (selected) props.onChange({ identity_file: selected });
	};

	const hostKeyEnforced = () => props.hostKeyChecking === "enforced-accept-new";

	const parseNumber = (value: string, fallback: number): number => {
		const parsed = Number.parseInt(value, 10);
		return Number.isFinite(parsed) ? parsed : fallback;
	};

	return (
		<>
			<div style={{ display: "flex", gap: "8px" }}>
				<div class={`${s.group} ${layout.field}`} style={{ flex: "1" }}>
					<label class={s.label}>Host</label>
					<Input
						data-setting-id={
							props.owner === "connection"
								? "connections.connections.{id}.transport.ssh.host"
								: "tunnels.profiles.{id}.ssh.host"
						}
						value={props.value.host}
						onInput={(e) => props.onChange({ host: e.currentTarget.value })}
						list={hostsListId}
					/>
					<datalist id={hostsListId}>
						<For each={sshHosts()}>{(h) => <option value={h} />}</For>
					</datalist>
				</div>
				<div class={`${s.group} ${layout.field}`} style={{ width: "80px" }}>
					<label class={s.label}>Port</label>
					<Input
						data-setting-id={
							props.owner === "connection"
								? "connections.connections.{id}.transport.ssh.port"
								: "tunnels.profiles.{id}.ssh.port"
						}
						type="number"
						value={props.value.port}
						onInput={(e) => props.onChange({ port: parseNumber(e.currentTarget.value, 22) })}
					/>
				</div>
			</div>

			<div class={`${s.group} ${layout.field}`}>
				<label class={s.label}>User</label>
				<Input
					data-setting-id={
						props.owner === "connection"
							? "connections.connections.{id}.transport.ssh.user"
							: "tunnels.profiles.{id}.ssh.user"
					}
					value={props.value.user}
					onInput={(e) => props.onChange({ user: e.currentTarget.value })}
				/>
			</div>

			<div class={`${s.group} ${layout.field}`}>
				<label class={s.label}>Identity / Authentication</label>
				<div style={{ display: "flex", gap: "6px" }}>
					<Input
						data-setting-id={
							props.owner === "connection"
								? "connections.connections.{id}.transport.ssh.identity_file"
								: "tunnels.profiles.{id}.ssh.identity_file"
						}
						style={{ flex: "1" }}
						value={props.value.identity_file ?? ""}
						onInput={(e) => props.onChange({ identity_file: e.currentTarget.value || null })}
						placeholder="Leave empty to use SSH agent"
					/>
					<Show when={isTauri()}>
						<button
							data-non-setting="Choose a key-file path for the SSH connection draft"
							type="button"
							class={d.cancelBtn}
							style={{
								flex: "none",
								padding: "4px 10px",
								"font-size": "var(--font-sm)",
								border: "none",
								"border-radius": "var(--radius-md)",
								cursor: "pointer",
							}}
							onClick={browseIdentityFile}
							title="Browse for key file"
						>
							Browse…
						</button>
					</Show>
				</div>
				<Show when={agentInfo().agent_type}>
					<div style={{ "margin-top": "6px", "font-size": "var(--font-sm)", color: "var(--fg-muted)" }}>
						<span style={{ color: agentInfo().keys.length > 0 ? "var(--success)" : "var(--fg-muted)" }}>
							{agentInfo().agent_type}:
						</span>{" "}
						<Show when={agentInfo().keys.length > 0} fallback="no keys loaded">
							{agentInfo()
								.keys.map((k) => `${k.comment} (${k.key_type}) — ${k.fingerprint}`)
								.join(", ")}
						</Show>
					</div>
				</Show>
			</div>

			<div style={{ display: "flex", gap: "8px" }}>
				<div class={`${s.group} ${layout.field}`} style={{ flex: "1" }}>
					<label class={s.label}>ServerAliveInterval</label>
					<Input
						data-setting-id={
							props.owner === "connection"
								? "connections.connections.{id}.transport.ssh.server_alive_interval"
								: "tunnels.profiles.{id}.ssh.server_alive_interval"
						}
						type="number"
						value={props.value.server_alive_interval}
						onInput={(e) => props.onChange({ server_alive_interval: parseNumber(e.currentTarget.value, 15) })}
					/>
				</div>
				<div class={`${s.group} ${layout.field}`} style={{ flex: "1" }}>
					<label class={s.label}>ServerAliveCountMax</label>
					<Input
						data-setting-id={
							props.owner === "connection"
								? "connections.connections.{id}.transport.ssh.server_alive_count_max"
								: "tunnels.profiles.{id}.ssh.server_alive_count_max"
						}
						type="number"
						value={props.value.server_alive_count_max}
						onInput={(e) => props.onChange({ server_alive_count_max: parseNumber(e.currentTarget.value, 3) })}
					/>
				</div>
				<div class={`${s.group} ${layout.field}`} style={{ width: "160px" }}>
					<label class={s.label}>StrictHostKeyChecking</label>
					<Show
						when={props.owner === "connection"}
						fallback={
							<Select
								data-setting-id="tunnels.profiles.{id}.ssh.strict_host_key_checking"
								value={props.value.strict_host_key_checking}
								onChange={(e) =>
									props.onChange({ strict_host_key_checking: e.currentTarget.value as "Yes" | "AcceptNew" })
								}
							>
								<For
									each={settingOptions("tunnels.profiles.{id}.ssh.strict_host_key_checking", {
										AcceptNew: "AcceptNew",
										Yes: "Yes",
									})}
								>
									{(option) => <option value={option.value}>{option.label}</option>}
								</For>
							</Select>
						}
					>
						<Select
							data-setting-id="connections.connections.{id}.transport.ssh.strict_host_key_checking"
							value={hostKeyEnforced() ? "AcceptNew" : props.value.strict_host_key_checking}
							disabled={hostKeyEnforced()}
							onChange={(e) =>
								props.onChange({ strict_host_key_checking: e.currentTarget.value as "Yes" | "AcceptNew" })
							}
						>
							<For
								each={settingOptions("connections.connections.{id}.transport.ssh.strict_host_key_checking", {
									AcceptNew: "AcceptNew",
									Yes: "Yes",
								}).filter((option) => !hostKeyEnforced() || option.value === "AcceptNew")}
							>
								{(option) => <option value={option.value}>{option.label}</option>}
							</For>
						</Select>
					</Show>
				</div>
			</div>
			<Show when={hostKeyEnforced()}>
				<p class={s.hint} style={{ margin: 0 }}>
					A remote server's tunnel is opened on your behalf, so it always accepts a new host's key on first contact and
					refuses a changed one (accept-new).
				</p>
			</Show>

			<label class={s.toggle}>
				<input
					data-setting-id={
						props.owner === "connection"
							? "connections.connections.{id}.transport.ssh.compression"
							: "tunnels.profiles.{id}.ssh.compression"
					}
					type="checkbox"
					checked={props.value.compression}
					onChange={(e) => props.onChange({ compression: e.currentTarget.checked })}
				/>
				<span>Compress the channel (ssh -C)</span>
			</label>
		</>
	);
};
