import { type Component, createSignal, For, onMount, Show } from "solid-js";
import { t } from "../../../../i18n";
import { appLogger } from "../../../../stores/appLogger";
import { rpc } from "../../../../transport";
import { cx } from "../../../../utils";
import { writeClipboard } from "../../../../utils/clipboard";
import { ExpertSetting } from "../../ExpertSetting";
import s from "../../Settings.module.css";
import { type AppConfig, saveConfigField, useMcpStatusPoll } from "./servicesShared";

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

/**
 * Local MCP page: HTTP/MCP server status, the manual bridge configuration
 * snippet, and native-tool visibility controls. Upstream (remote) MCP
 * servers are a separate concern — `SettingsPanel` renders `UpstreamMcpPanel`
 * alongside this one under the same "mcp" tab.
 */
export const LocalMcpPanel: Component = () => {
	const { status } = useMcpStatusPoll();
	const [disabledNativeTools, setDisabledNativeTools] = createSignal<string[]>([]);
	// The toggles wait for the saved list: the `[]` placeholder would show every
	// tool as enabled until `load_config` answers.
	const [nativeToolsLoaded, setNativeToolsLoaded] = createSignal(false);
	const [collapseTools, setCollapseTools] = createSignal<boolean>(false);
	const [bridgeInfo, setBridgeInfo] = createSignal<{ bridge_path: string; config_snippet: string } | null>(null);
	const [bridgeInfoOpen, setBridgeInfoOpen] = createSignal(false);
	const [snippetCopied, setSnippetCopied] = createSignal(false);

	onMount(async () => {
		try {
			const config = await rpc<AppConfig>("load_config");
			setDisabledNativeTools(config.disabled_native_tools ?? []);
			setCollapseTools(config.collapse_tools ?? false);
		} catch (e) {
			appLogger.warn("config", "Failed to load native tool config, using defaults", e);
		} finally {
			setNativeToolsLoaded(true);
		}
	});

	return (
		<div class={s.section}>
			<h3>{t("services.heading.httpApiServer", "HTTP API Server")}</h3>

			<div class={s.group}>
				<p class={s.hint}>
					{t(
						"services.hint.httpDescription",
						"Serves the REST API and MCP protocol for AI agents and automation tools",
					)}
				</p>
			</div>

			<div class={s.group}>
				<label>{t("services.label.serverStatus", "Server Status")}</label>
				<div class={s.mcpStatusRow}>
					<span class={cx(s.mcpStatusDot, status()?.running && s.running)} />
					<span class={s.mcpStatusText}>
						{status()?.running ? t("services.status.running", "Running") : t("services.status.starting", "Starting...")}
					</span>
					<Show when={status()?.running}>
						<span class={s.mcpStatusPort}>{t("services.label.socket", "Socket")}</span>
					</Show>
				</div>
			</div>

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

			<ExpertSetting configKey="app.collapse_tools" value={collapseTools()}>
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
						{/* A label element, not a span: search indexes and scrolls to labels. The
						    inline color and margin undo the stacked `.group label` look. */}
						<label style={{ "font-weight": 500, "font-size": "13px", color: "inherit", margin: 0 }}>
							Collapse tools — Speakeasy MCP (reduces AI context ~98%)
						</label>
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
			</ExpertSetting>
			<Show when={nativeToolsLoaded()}>
				{/* Static, unlike the per-tool names: the one label search can index for this group. */}
				<div class={s.group}>
					<label>Native tools</label>
				</div>
				<For each={status()?.native_tools}>
					{(tool) => {
						const disabled = () => disabledNativeTools().includes(tool.name);
						return (
							<div class={s.group} style={{ display: "flex", "align-items": "center", gap: "8px", padding: "4px 0" }}>
								<div class={s.toggle} style={{ "margin-right": "4px" }}>
									<input
										type="checkbox"
										aria-label={tool.name}
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
									<span style={{ "font-weight": 500, "font-size": "13px", "font-family": "monospace" }}>
										{tool.name}
									</span>
									<span class={s.hint} style={{ margin: 0 }}>
										{tool.summary}
									</span>
									<span class={s.infoBadge}>
										?<span class={s.infoBadgeTip}>{tool.description}</span>
									</span>
								</div>
							</div>
						);
					}}
				</For>
			</Show>

			<p class={s.hint} style={{ "margin-top": "16px", color: "var(--text-dimmed)" }}>
				{t("services.hint.autoSave", "Settings are saved automatically when changed")}
			</p>
		</div>
	);
};
