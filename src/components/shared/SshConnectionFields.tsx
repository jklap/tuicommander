import { type Component, createSignal, createUniqueId, For, onMount, Show } from "solid-js";
import { invoke } from "../../invoke";
import type { SshConnectionParams } from "../../stores/tunnels";
import s from "../SettingsPanel/Settings.module.css";
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
	value: SshConnectionParams;
	onChange: (patch: Partial<SshConnectionParams>) => void;
}

/**
 * Shared SSH connection form — host (with `~/.ssh/config` host-alias
 * autocomplete), port, user, identity file (with a Browse button and live
 * SSH-agent-detection display, including each key's fingerprint), keepalive
 * tuning (ServerAliveInterval/ServerAliveCountMax), and StrictHostKeyChecking.
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
 * desktop-only gate) — `@tauri-apps/plugin-dialog` is therefore imported
 * dynamically here, matching the established pattern in `jsonFileTransfer.ts`,
 * rather than the static top-level import `TunnelEditorModal.tsx` used to have.
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
		const { open: openFileDialog } = await import("@tauri-apps/plugin-dialog");
		const selected = await openFileDialog({
			title: "Select SSH Identity File",
			defaultPath: home ?? undefined,
			multiple: false,
		});
		if (selected) props.onChange({ identity_file: selected as string });
	};

	const parseNumber = (value: string, fallback: number): number => {
		const parsed = Number.parseInt(value, 10);
		return Number.isFinite(parsed) ? parsed : fallback;
	};

	return (
		<>
			<div style={{ display: "flex", gap: "8px" }}>
				<div class={s.group} style={{ flex: "1" }}>
					<label class={s.label}>Host</label>
					<input
						value={props.value.host}
						onInput={(e) => props.onChange({ host: e.currentTarget.value })}
						list={hostsListId}
					/>
					<datalist id={hostsListId}>
						<For each={sshHosts()}>{(h) => <option value={h} />}</For>
					</datalist>
				</div>
				<div class={s.group} style={{ width: "80px" }}>
					<label class={s.label}>Port</label>
					<input
						type="number"
						value={props.value.port}
						onInput={(e) => props.onChange({ port: parseNumber(e.currentTarget.value, 22) })}
					/>
				</div>
			</div>

			<div class={s.group}>
				<label class={s.label}>User</label>
				<input value={props.value.user} onInput={(e) => props.onChange({ user: e.currentTarget.value })} />
			</div>

			<div class={s.group}>
				<label class={s.label}>Identity / Authentication</label>
				<div style={{ display: "flex", gap: "6px" }}>
					<input
						style={{ flex: "1" }}
						value={props.value.identity_file ?? ""}
						onInput={(e) => props.onChange({ identity_file: e.currentTarget.value || null })}
						placeholder="Leave empty to use SSH agent"
					/>
					<button
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
				<div class={s.group} style={{ flex: "1" }}>
					<label class={s.label}>ServerAliveInterval</label>
					<input
						type="number"
						value={props.value.server_alive_interval}
						onInput={(e) => props.onChange({ server_alive_interval: parseNumber(e.currentTarget.value, 15) })}
					/>
				</div>
				<div class={s.group} style={{ flex: "1" }}>
					<label class={s.label}>ServerAliveCountMax</label>
					<input
						type="number"
						value={props.value.server_alive_count_max}
						onInput={(e) => props.onChange({ server_alive_count_max: parseNumber(e.currentTarget.value, 3) })}
					/>
				</div>
				<div class={s.group} style={{ width: "160px" }}>
					<label class={s.label}>StrictHostKeyChecking</label>
					<select
						value={props.value.strict_host_key_checking}
						onChange={(e) => props.onChange({ strict_host_key_checking: e.currentTarget.value as "Yes" | "AcceptNew" })}
					>
						<option value="AcceptNew">AcceptNew</option>
						<option value="Yes">Yes</option>
					</select>
				</div>
			</div>
		</>
	);
};
