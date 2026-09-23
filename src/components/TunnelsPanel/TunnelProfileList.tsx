import { type Component, createSignal, For, Show } from "solid-js";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import type { TunnelProfile } from "../../stores/tunnels";
import { tunnelsStore } from "../../stores/tunnels";
import { TunnelStatusBadge } from "./TunnelStatusBadge";
import s from "./TunnelsPanel.module.css";

interface AuditEntry {
	tunnel_id: string;
	timestamp: string;
	kind: string;
	message: string | null;
}

export interface TunnelProfileListProps {
	/**
	 * Rendered as an extra "Edit" action per row, opening the merged
	 * Kind-dropdown editor pre-filled with this profile. Omitted in the
	 * Tunnels overlay (`TunnelsPanel.tsx`) — Settings now owns tunnel
	 * creation/editing entirely (story: SSH Tunnels + Remote Servers
	 * consolidation, decision #1); the overlay keeps only live
	 * status/control (Start/Stop/Log/Del).
	 */
	onEdit?: (profile: TunnelProfile) => void;
}

/**
 * The tunnel-profile list — rows with Start/Stop/(Edit)/Log/Del + inline
 * audit-log expansion. Shared by BOTH the Tunnels overlay panel
 * (`TunnelsPanel.tsx`) and the Settings "Remote Servers" tab's "SSH Tunnel"
 * kind section, so both render the identical list/controls instead of two
 * copies (previously this was inline in `TunnelsPanel.tsx`, mixed with
 * overlay-only chrome like the full-screen backdrop and close button).
 */
export const TunnelProfileList: Component<TunnelProfileListProps> = (props) => {
	const [expandedId, setExpandedId] = createSignal<string | null>(null);
	const [auditEntries, setAuditEntries] = createSignal<AuditEntry[]>([]);

	const toggleExpand = async (id: string) => {
		if (expandedId() === id) {
			setExpandedId(null);
			setAuditEntries([]);
			return;
		}
		setExpandedId(id);
		try {
			const entries = await invoke<AuditEntry[]>("get_tunnel_audit", { id, limit: 20 });
			if (expandedId() !== id) return;
			setAuditEntries(entries ?? []);
		} catch {
			if (expandedId() !== id) return;
			setAuditEntries([]);
		}
	};

	const handleDelete = async (id: string) => {
		try {
			await tunnelsStore.deleteProfile(id);
		} catch (err) {
			appLogger.error("store", "TunnelProfileList delete failed", err);
		}
	};

	const handleToggleTunnel = async (id: string) => {
		const active = tunnelsStore.getTunnelStatus(id);
		try {
			if (active && active.type !== "stopped" && active.type !== "error") {
				await tunnelsStore.stopTunnel(id);
			} else {
				await tunnelsStore.startTunnel(id);
			}
		} catch (err) {
			appLogger.error("store", "TunnelProfileList toggle tunnel failed", err);
		}
	};

	const isRunning = (id: string): boolean => {
		const status = tunnelsStore.getTunnelStatus(id);
		return !!status && status.type !== "stopped" && status.type !== "error";
	};

	return (
		<div class={s.list}>
			<Show
				when={tunnelsStore.getProfiles().length > 0}
				fallback={<div class={s.empty}>No tunnel profiles yet. Click "Add Connection" to create one.</div>}
			>
				<For each={tunnelsStore.getProfiles()}>
					{(profile) => (
						<>
							<div class={s.row}>
								<div class={s.rowInfo}>
									<span class={s.rowName}>{profile.name}</span>
									<span class={s.rowMeta}>
										{profile.ssh.user}@{profile.ssh.host}:{profile.ssh.port}
									</span>
									<TunnelStatusBadge status={tunnelsStore.getTunnelStatus(profile.id)} />
								</div>
								<div class={s.rowActions}>
									<button
										class={s.actionBtn}
										onClick={() => handleToggleTunnel(profile.id)}
										title={isRunning(profile.id) ? "Stop" : "Start"}
									>
										{isRunning(profile.id) ? "Stop" : "Start"}
									</button>
									<Show when={props.onEdit}>
										<button class={s.actionBtn} onClick={() => props.onEdit?.(profile)} title="Edit">
											Edit
										</button>
									</Show>
									<button class={s.actionBtn} onClick={() => toggleExpand(profile.id)} title="Audit log">
										{expandedId() === profile.id ? "Hide" : "Log"}
									</button>
									<button class={s.actionBtn} onClick={() => handleDelete(profile.id)} title="Delete">
										Del
									</button>
								</div>
							</div>
							<Show when={expandedId() === profile.id}>
								<div class={s.auditTimeline}>
									<Show when={auditEntries().length > 0} fallback={<span class={s.auditEmpty}>No audit entries</span>}>
										<For each={auditEntries()}>
											{(entry) => (
												<div class={s.auditEntry}>
													<span class={s.auditTime}>{new Date(entry.timestamp).toLocaleTimeString()}</span>
													<span class={s.auditKind}>{entry.kind}</span>
													<Show when={entry.message}>
														<span class={s.auditMsg}>{entry.message}</span>
													</Show>
												</div>
											)}
										</For>
									</Show>
								</div>
							</Show>
						</>
					)}
				</For>
			</Show>
		</div>
	);
};
