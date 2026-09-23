import { type Component, createSignal, For, onMount, Show } from "solid-js";
import { t } from "../../../i18n";
import { invoke } from "../../../invoke";
import { appLogger } from "../../../stores/appLogger";
import { isTauri, rpc } from "../../../transport";
import { writeClipboard } from "../../../utils/clipboard";
import { updateAppConfig } from "../../../utils/updateAppConfig";
import s from "../Settings.module.css";
import a from "./AgentsTab.module.css";
import { UpstreamMcpPanel } from "./services/UpstreamMcpPanel";

export { authFromUpstreamForm, shouldShowAuthorize, startAuthorizeFlow } from "./services/UpstreamMcpPanel";

/** Only the slice of the full backend config this tab reads/writes — `save_config`
 *  round-trips whatever `load_config` returned, so a narrower type here is safe
 *  (same pattern as `StreamDockTab.tsx`'s own narrow `AppConfig`). */
interface AppConfig {
	disabled_native_tools: string[];
	collapse_tools: boolean;
}

/** Static definition of native TUIC tools exposed via MCP */
const NATIVE_TOOLS: { name: string; description: string; actions: string }[] = [
	{
		name: "session",
		description: "PTY terminal panes (tmux replacement)",
		actions: "list, create, input, output, resize, close, kill, pause, resume",
	},
	{
		name: "agent",
		description: "AI agents + inter-agent messaging",
		actions: "spawn, detect, stats, metrics, register, list_peers, send, inbox",
	},
	{
		name: "repo",
		description: "Repos, GitHub PRs, worktrees",
		actions: "list, active, prs, status, worktree_list, worktree_create, worktree_remove",
	},
	{ name: "ui", description: "Panel tabs + notifications", actions: "tab, toast, confirm" },
	{
		name: "plugin_dev_guide",
		description: "Plugin authoring reference",
		actions: "Returns full plugin authoring guide",
	},
	{ name: "config", description: "Read and write app config", actions: "get, save" },
	{ name: "knowledge", description: "Cross-repo knowledge base (mdkb)", actions: "search, code_graph, status, setup" },
	{
		name: "debug",
		description: "Diagnostics + plugin guide",
		actions: "agent_detection, logs, invoke_js, plugin_guide",
	},
];

/** Copy the MCP client config snippet to clipboard, logging on failure. Returns
 *  whether the write succeeded so the caller only shows the "Copied" indicator
 *  on actual success. */
export async function copyMcpSnippet(snippet: string): Promise<boolean> {
	try {
		await writeClipboard(snippet);
		return true;
	} catch (err) {
		appLogger.warn("settings", "Clipboard write failed", { error: String(err) });
		return false;
	}
}

const LocalServicesPanel: Component = () => {
	const [disabledNativeTools, setDisabledNativeTools] = createSignal<string[]>([]);
	const [collapseTools, setCollapseTools] = createSignal<boolean>(false);
	const [bridgeInfo, setBridgeInfo] = createSignal<{ bridge_path: string; config_snippet: string } | null>(null);
	const [bridgeInfoOpen, setBridgeInfoOpen] = createSignal(false);
	const [snippetCopied, setSnippetCopied] = createSignal(false);

	const loadToolsConfig = async () => {
		try {
			const config = await rpc<AppConfig>("load_config");
			setDisabledNativeTools(config.disabled_native_tools ?? []);
			setCollapseTools(config.collapse_tools ?? false);
		} catch (e) {
			appLogger.warn("config", "Failed to load tools config, using defaults", e);
		}
	};

	onMount(() => {
		loadToolsConfig();
	});

	/** Save a single config field (load-modify-save pattern matching other tabs) */
	const saveConfigField = async (updater: (config: AppConfig) => void) => {
		try {
			await updateAppConfig<AppConfig>(updater);
		} catch (e) {
			appLogger.error("config", "Failed to save config", e);
		}
	};

	return (
		<>
			{/* ── TUIC MCP Server ── */}
			<h3>TUIC MCP Server</h3>
			<div class={s.group}>
				<p class={s.hint}>Native tools exposed via MCP. Disable tools to restrict what AI agents can access.</p>
			</div>

			<div class={s.group}>
				<p class={s.hint} style={{ margin: "0 0 8px" }}>
					{t(
						"services.hint.perAgentAutoConfigure",
						"The MCP server can also be configured automatically for a specific agent from that agent's own settings, under Settings → Agents.",
					)}
				</p>
				<button
					class={s.mcpDisclosure}
					onClick={() => {
						const opening = !bridgeInfoOpen();
						setBridgeInfoOpen(opening);
						if (opening && !bridgeInfo()) {
							rpc<{ bridge_path: string; config_snippet: string }>("get_mcp_bridge_info")
								.then(setBridgeInfo)
								.catch((e) => appLogger.warn("config", "Failed to fetch bridge info", e));
						}
					}}
				>
					<span class={s.mcpDisclosureArrow}>{bridgeInfoOpen() ? "▼" : "▶"}</span>
					Manual MCP configuration
				</button>
				<Show when={bridgeInfoOpen() && bridgeInfo()}>
					<div class={s.mcpDisclosureBody}>
						<p class={s.hint} style={{ margin: "0 0 4px" }}>
							Bridge path: <code class={s.mcpCode}>{bridgeInfo()!.bridge_path}</code>
						</p>
						<p class={s.hint} style={{ margin: "0 0 6px" }}>
							Add this to your MCP client config (e.g. <code class={s.mcpCode}>~/.claude.json</code> under{" "}
							<code class={s.mcpCode}>mcpServers</code>):
						</p>
						<div class={s.mcpSnippetWrap}>
							<pre class={s.mcpSnippetPre}>{bridgeInfo()!.config_snippet}</pre>
							<button
								class={s.mcpSnippetCopy}
								onClick={async () => {
									if (await copyMcpSnippet(bridgeInfo()!.config_snippet)) {
										setSnippetCopied(true);
										setTimeout(() => setSnippetCopied(false), 2000);
									}
								}}
							>
								{snippetCopied() ? "Copied" : "Copy"}
							</button>
						</div>
					</div>
				</Show>
			</div>

			<div class={s.group} style={{ display: "flex", "align-items": "center", gap: "8px", padding: "4px 0" }}>
				<div class={s.toggle} style={{ "margin-right": "4px" }}>
					<input
						type="checkbox"
						checked={collapseTools()}
						onChange={(e) => {
							const enabled = e.currentTarget.checked;
							setCollapseTools(enabled);
							saveConfigField((c) => {
								c.collapse_tools = enabled;
							});
						}}
					/>
				</div>
				<div style={{ display: "flex", "align-items": "center", gap: "6px" }}>
					<span style={{ "font-weight": 500, "font-size": "13px" }}>
						Collapse tools — Speakeasy MCP (reduces AI context ~98%)
					</span>
					<span class={s.infoBadge}>
						?
						<span class={s.infoBadgeTip}>
							When enabled, MCP clients only see three meta-tools (search_tools, get_tool_schema, call_tool) and
							discover the full tool set on demand. Drastically reduces token usage for clients that don't need every
							tool upfront.
						</span>
					</span>
				</div>
			</div>
			<For each={NATIVE_TOOLS}>
				{(tool) => {
					const disabled = () => disabledNativeTools().includes(tool.name);
					return (
						<div class={s.group} style={{ display: "flex", "align-items": "center", gap: "8px", padding: "4px 0" }}>
							<div class={s.toggle} style={{ "margin-right": "4px" }}>
								<input
									type="checkbox"
									checked={!disabled()}
									onChange={(e) => {
										const enabled = e.currentTarget.checked;
										const updated = enabled
											? disabledNativeTools().filter((n) => n !== tool.name)
											: [...disabledNativeTools(), tool.name];
										setDisabledNativeTools(updated);
										saveConfigField((c) => {
											c.disabled_native_tools = updated;
										});
									}}
								/>
							</div>
							<div style={{ display: "flex", "align-items": "center", gap: "6px" }}>
								<span style={{ "font-weight": 500, "font-size": "13px", "font-family": "monospace" }}>{tool.name}</span>
								<span class={s.hint} style={{ margin: 0 }}>
									{tool.description}
								</span>
								<span class={s.infoBadge}>
									?<span class={s.infoBadgeTip}>{tool.actions}</span>
								</span>
							</div>
						</div>
					);
				}}
			</For>
		</>
	);
};

// ---------------------------------------------------------------------------
// MCP integrations cleanup (moved here from the Agents tab)
// ---------------------------------------------------------------------------

/**
 * One place to see — and undo — every MCP bridge entry TUICommander wrote.
 *
 * Without it, uninstalling TUIC leaves a dangling `tuic-bridge` entry in each
 * client it ever configured, and the user has to know which ones those were to
 * clean up (issue #115).
 */
const McpIntegrationsSection: Component = () => {
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

export const ServicesTab: Component = () => (
	<div class={s.section}>
		<LocalServicesPanel />
		<UpstreamMcpPanel />
		<McpIntegrationsSection />
		<p class={s.hint} style={{ "margin-top": "16px", color: "var(--text-dimmed)" }}>
			{t("services.hint.autoSave", "Settings are saved automatically when changed")}
		</p>
	</div>
);
