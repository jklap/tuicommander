import { type Component, createSignal, onMount, Show } from "solid-js";
import { invoke } from "../../../../invoke";
import { appLogger } from "../../../../stores/appLogger";
import { isTauri } from "../../../../transport";
import s from "../../Settings.module.css";
import a from "../AgentsTab.module.css";

// ---------------------------------------------------------------------------
// MCP integrations cleanup (moved here from the Agents tab)
// ---------------------------------------------------------------------------

/**
 * One place to see — and undo — every MCP bridge entry TUICommander wrote.
 *
 * Without it, uninstalling TUIC leaves a dangling `tuic-bridge` entry in each
 * client it ever configured, and the user has to know which ones those were to
 * clean up (issue #115). `SettingsPanel` renders it on the MCP page, after
 * `UpstreamMcpPanel`.
 */
export const McpIntegrationsSection: Component = () => {
	const [installed, setInstalled] = createSignal<string[]>([]);
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);

	const refresh = async () => {
		if (!isTauri()) return;
		try {
			setInstalled(await invoke<string[]>("list_installed_mcp_integrations"));
		} catch (err) {
			appLogger.error("config", "Failed to list MCP integrations", err);
		}
	};

	onMount(refresh);

	const handleRemoveAll = async () => {
		if (busy()) return;
		setBusy(true);
		setError(null);
		try {
			await invoke<string[]>("remove_all_mcp_integrations");
		} catch (err) {
			// The sweep removes what it can and reports the rest, so refresh
			// regardless — some entries are gone even on a partial failure.
			setError(String(err));
			appLogger.error("config", "Failed to remove MCP integrations", err);
		} finally {
			await refresh();
			setBusy(false);
		}
	};

	return (
		<Show when={isTauri() && installed().length > 0}>
			<div class={a.expandedSection}>
				<div class={a.expandedLabel}>MCP integrations</div>
				<p class={s.hint} style={{ "margin-bottom": "8px" }}>
					The TUICommander bridge is configured in: <strong>{installed().join(", ")}</strong>. Remove them before
					uninstalling TUICommander, or each client will report a missing MCP server.
				</p>
				<div class={a.actionsRow}>
					<button class={a.actionBtn} onClick={handleRemoveAll} disabled={busy()}>
						{busy() ? "Removing..." : "Remove all MCP integrations"}
					</button>
				</div>
				<Show when={error()}>
					<p class={a.remoteError}>{error()}</p>
				</Show>
			</div>
		</Show>
	);
};
