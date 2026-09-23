import { type Component, createSignal, onMount, Show } from "solid-js";
import { t } from "../../../i18n";
import type { DiscoveredSshHost, RemoteConnection } from "../../../stores/remoteConnections";
import { type TunnelProfile, tunnelsStore } from "../../../stores/tunnels";
import { DirectCertConfirmDialog } from "../../shared/DirectCertConfirmDialog";
import s from "../Settings.module.css";
import { RemoteMachinesTab } from "./RemoteMachinesTab";
import { SshTunnelsSection } from "./SshTunnelsSection";
import type { EditorTarget } from "./services/RemoteConnectionEditor";
import { prefillFromDiscoveredHost, RemoteConnectionEditor } from "./services/RemoteConnectionEditor";

export { transportBadgeLabel, transportSummary } from "./services/RemoteMachinesPanel";

/**
 * "Remote Servers" settings tab (story: SSH Tunnels + Remote Servers
 * consolidation, "Settings reorganization" + "Merged connection model").
 *
 * Replaces the separate "Remote Machines" page and the Tunnels overlay's own
 * editor: this page owns BOTH the tunnel-profile list
 * (`SshTunnelsSection` → `TunnelProfileList`) and the remote-connection list
 * (`RemoteMachinesTab` → `RemoteMachinesPanel`,
 * with its discovered hosts, deployment and update controls), and every
 * create/edit goes through one merged Kind-dropdown editor
 * (`RemoteConnectionEditor`). No desktop-only filter: tunnels and remote
 * connections both have full HTTP parity.
 *
 * NOTE for `settingsSearchIndex.test.ts`: its extractor's occurrence scan is a
 * raw-text regex with no comment awareness — writing the literal heading-tag
 * syntax inside a prose comment (rather than a paraphrase like this) confuses
 * it into treating the comment's occurrence as a phantom opening tag.
 */
export const RemoteServersTab: Component = () => {
	const [editorTarget, setEditorTarget] = createSignal<EditorTarget | null>(null);

	const openAdd = () => setEditorTarget({ kind: "new" });
	const openEditTunnel = (profile: TunnelProfile) => setEditorTarget({ kind: "edit-tunnel", profile });
	const openEditConnection = (connection: RemoteConnection) => setEditorTarget({ kind: "edit-connection", connection });
	const openAddFromHost = (host: DiscoveredSshHost) =>
		setEditorTarget({ kind: "new", prefill: prefillFromDiscoveredHost(host) });
	const closeEditor = () => setEditorTarget(null);

	// The Tunnels overlay hydrates the same store; whichever opens first loads it.
	onMount(() => tunnelsStore.hydrate());

	return (
		<>
			{/* Connect asks here before pinning a self-signed Direct certificate. */}
			<DirectCertConfirmDialog />
			<div class={s.section}>
				<h3>{t("remoteServers.heading", "Remote Servers")}</h3>
				{/* A saved SSH connection does NOT create a tunnel at Save time — the
				    encrypted tunnel is opened when you Connect. */}
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

				{/* Keyed on the target: switching straight from one item's editor to
				    another's must rebuild the form, not keep the first one's fields. */}
				<Show when={editorTarget()} keyed>
					{(target) => <RemoteConnectionEditor target={target} onClose={closeEditor} />}
				</Show>
			</div>

			<SshTunnelsSection onEdit={openEditTunnel} />

			<RemoteMachinesTab onEdit={openEditConnection} onAddFromHost={openAddFromHost} />
		</>
	);
};
