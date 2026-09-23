import { type Component, createSignal, For, onMount, Show } from "solid-js";
import { t } from "../../../i18n";
import { appLogger } from "../../../stores/appLogger";
import type { ConnectionState, RemoteConnection, RemoteTransport } from "../../../stores/remoteConnections";
import { remoteConnectionsStore } from "../../../stores/remoteConnections";
import type { TunnelProfile } from "../../../stores/tunnels";
import {
	ConnectionStatusBadge,
	remoteConnectionStatusColor,
	remoteConnectionStatusLabel,
} from "../../shared/ConnectionStatusBadge";
import { DirectCertConfirmDialog } from "../../shared/DirectCertConfirmDialog";
import { TunnelProfileList } from "../../TunnelsPanel/TunnelProfileList";
import s from "../Settings.module.css";
import type { EditorTarget } from "./services/RemoteConnectionEditor";
import { RemoteConnectionEditor } from "./services/RemoteConnectionEditor";

/** Transport summary string for the connection list. */
export function transportSummary(transport: RemoteTransport): string {
	if (transport.type === "Ssh") {
		return `${transport.ssh.user}@${transport.ssh.host}:${transport.ssh.port}`;
	}
	if (transport.type === "Local") {
		return transport.instance_id ? `local: ${transport.instance_id}` : `local: 127.0.0.1:${transport.port ?? "?"}`;
	}
	return transport.url;
}

/** Human-readable badge text for a transport's kind. */
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

/**
 * "Remote Servers" settings tab (story: SSH Tunnels + Remote Servers
 * consolidation, "Settings reorganization" + "Merged connection model").
 *
 * Replaces `RemoteMachinesPanel`'s previous home inside "Services & MCP" —
 * this tab now owns BOTH the tunnel-profile list (formerly the Tunnels
 * overlay's only editing surface) and the remote-connection list (formerly
 * `RemoteMachinesPanel`), through one merged Kind-dropdown editor
 * (`RemoteConnectionEditor`). No desktop-only filter: tunnels and remote
 * connections both have full HTTP parity.
 *
 * This file carries its own real top-level heading element directly (rather
 * than only inside an imported subcomponent) so `settingsSearchIndex.test.ts`'s
 * single-file extractor (`extractTab`, which reads only the ONE file a tab
 * key maps to, never following imports) can see it — same convention
 * `RemoteAccessTab.tsx` established in the prior phase of this plan. NOTE:
 * that extractor's own occurrence scan is a raw-text regex with no comment
 * awareness — writing the literal heading-tag syntax inside a prose comment
 * (rather than a paraphrase like this) confuses it into treating the comment's
 * occurrence as a phantom opening tag paired with the real closing tag below.
 */
export const RemoteServersTab: Component = () => {
	const [editorTarget, setEditorTarget] = createSignal<EditorTarget | null>(null);
	const [deleting, setDeleting] = createSignal<string | null>(null);

	onMount(() => {
		remoteConnectionsStore.hydrate();
	});

	const connectionList = (): ConnectionState[] => Object.values(remoteConnectionsStore.getConnections());

	const openAdd = () => setEditorTarget({ kind: "new" });
	const openEditTunnel = (profile: TunnelProfile) => setEditorTarget({ kind: "edit-tunnel", profile });
	const openEditConnection = (connection: RemoteConnection) => setEditorTarget({ kind: "edit-connection", connection });
	const closeEditor = () => setEditorTarget(null);

	async function removeConnection(id: string, name: string) {
		let confirmed: boolean;
		try {
			const { confirm } = await import("@tauri-apps/plugin-dialog");
			confirmed = await confirm(`Remove remote connection "${name}"?`, {
				title: "Remove remote connection",
				kind: "warning",
			});
		} catch {
			confirmed = window.confirm(`Remove remote connection "${name}"?`);
		}
		if (!confirmed) return;
		setDeleting(id);
		try {
			await remoteConnectionsStore.removeConnection(id);
		} catch (err) {
			appLogger.error("settings", "Failed to remove remote connection", { error: String(err) });
		} finally {
			setDeleting(null);
		}
	}

	return (
		<div class={s.section}>
			<DirectCertConfirmDialog />
			<h3>{t("remoteServers.heading", "Remote Servers")}</h3>
			{/* Corrected lifecycle wording (UI/copy fix from the doc audit): a saved
			    SSH connection does NOT create a tunnel at Save time — the encrypted
			    tunnel is opened when you Connect, matching
			    docs/user-guide/remote-access.md's already-corrected wording. */}
			<p class={s.hint}>
				{t(
					"remoteServers.hint",
					"Connect to TUIC instances on other machines (SSH, Direct, or a local named instance), or manage SSH port-forwarding tunnels. An SSH connection's encrypted tunnel is created when you Connect, not when you save it.",
				)}
			</p>

			<div class={s.group} style={{ display: "flex", "justify-content": "flex-end" }}>
				<Show when={!editorTarget()}>
					<button class={s.copyBtn} onClick={openAdd}>
						Add Connection
					</button>
				</Show>
			</div>

			<Show when={editorTarget()}>
				{(target) => <RemoteConnectionEditor target={target()} onClose={closeEditor} />}
			</Show>

			<div style={{ "margin-top": "16px" }}>
				{/* A plain div, deliberately not a form-label element — this isn't a
				    label for any single input, and settingsSearchIndex.test.ts's
				    extractTab treats every such element as an indexable "setting",
				    which this sub-heading isn't. */}
				<div style={{ "font-weight": 500, "font-size": "13px" }}>SSH Tunnels</div>
				<TunnelProfileList onEdit={openEditTunnel} />
			</div>

			<div style={{ "margin-top": "16px" }}>
				<div style={{ "font-weight": 500, "font-size": "13px" }}>Remote Connections</div>
				<Show when={connectionList().length === 0}>
					<p class={s.hint} style={{ color: "var(--text-dimmed)" }}>
						No remote connections configured yet. Click "Add Connection" above to add one.
					</p>
				</Show>
				<For each={connectionList()}>
					{(connState) => {
						const conn = () => connState.connection;
						return (
							<div style={{ "border-bottom": "1px solid var(--border-subtle, rgba(255,255,255,0.06))" }}>
								<div class={s.group} style={{ display: "flex", "align-items": "center", gap: "8px", padding: "8px 0" }}>
									<ConnectionStatusBadge
										color={remoteConnectionStatusColor(connState.status)}
										label={remoteConnectionStatusLabel(connState.status)}
									/>
									<div style={{ flex: 1, "min-width": 0 }}>
										<div style={{ display: "flex", "align-items": "center", gap: "6px", "flex-wrap": "wrap" }}>
											<span style={{ "font-weight": 500, "font-size": "13px" }}>{conn().name}</span>
											<span
												style={{
													"font-size": "10px",
													padding: "1px 5px",
													"border-radius": "3px",
													background: "rgba(97,175,239,0.15)",
													color: "#61afef",
												}}
											>
												{transportBadgeLabel(conn().transport)}
											</span>
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
											<div
												class={s.hint}
												style={{ margin: 0, "font-size": "11px", color: "var(--accent-red, #ef4444)" }}
											>
												{connState.error}
											</div>
										</Show>
									</div>
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
									<button class={s.copyBtn} style={{ "flex-shrink": 0 }} onClick={() => openEditConnection(conn())}>
										Edit
									</button>
									<button
										class={s.copyBtn}
										onClick={() => removeConnection(conn().id, conn().name)}
										disabled={deleting() === conn().id}
										style={{ color: "var(--error, #e06c75)", "flex-shrink": 0 }}
									>
										Delete
									</button>
								</div>
							</div>
						);
					}}
				</For>
			</div>
		</div>
	);
};
