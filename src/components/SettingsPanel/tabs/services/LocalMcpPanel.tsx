import { type Component, createSignal, For, onMount, Show } from "solid-js";
import { t } from "../../../../i18n";
import { appLogger } from "../../../../stores/appLogger";
import { rpc } from "../../../../transport";
import { writeClipboard } from "../../../../utils/clipboard";
import { StatusBadge } from "../../../ui/StatusBadge";
import { ExpertSetting } from "../../ExpertSetting";
import s from "../../Settings.module.css";
import { SectionCard, SettingPageHeader, SettingRow } from "../../SettingsPrimitives";
import layout from "./ServiceRestyle.module.css";
import { type AppConfig, saveConfigField, useMcpStatusPoll } from "./servicesShared";

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
		<div class={`${s.section} ${layout.page}`}>
			<SettingPageHeader
				title="MCP"
				description="Local MCP connection, tool visibility and upstream servers."
				scope="This machine"
				breadcrumb="Integrations / MCP"
			/>
			<SectionCard title={t("services.heading.httpApiServer", "HTTP API Server")}>
				<div class={s.group}>
					<p class={s.hint}>
						{t(
							"services.hint.httpDescription",
							"Serves the REST API and MCP protocol for AI agents and automation tools",
						)}
					</p>
				</div>

				<SettingRow label={t("services.label.serverStatus", "Server Status")}>
					{() => (
						<>
							<StatusBadge
								label={
									status()?.running
										? t("services.status.running", "Running")
										: t("services.status.starting", "Starting...")
								}
								variant={status()?.running ? "success" : undefined}
							/>
						</>
					)}
				</SettingRow>

				<SettingRow
					label={t("services.label.mcpConnection", "MCP Connection")}
					hint={t(
						"services.hint.mcpConnection",
						"AI agents connect via the tuic-bridge sidecar. MCP configs are auto-installed in supported agents (Claude Code, Cursor, etc.).",
					)}
				>
					{() => <StatusBadge label="tuic-bridge" />}
				</SettingRow>

				{/* ── TUIC Tools ── */}
			</SectionCard>
			<SectionCard title="TUIC Tools">
				<div class={s.group}>
					<p class={s.hint}>Native tools exposed via MCP. Disable tools to restrict what AI agents can access.</p>
				</div>

				<div class={s.group}>
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
									onClick={() => {
										writeClipboard(bridgeInfo()!.config_snippet)
											.then(() => {
												setSnippetCopied(true);
												setTimeout(() => setSnippetCopied(false), 2000);
											})
											.catch((err) => {
												appLogger.warn("settings", "Clipboard write failed", { error: String(err) });
											});
									}}
								>
									{snippetCopied() ? "Copied" : "Copy"}
								</button>
							</div>
						</div>
					</Show>
				</div>

				<ExpertSetting configKey="app.collapse_tools" value={collapseTools()}>
					<SettingRow
						label="Collapse tools — Speakeasy MCP (reduces AI context ~98%)"
						hint="Discover tools on demand through search_tools, get_tool_schema and call_tool."
					>
						{(id, descriptionId) => (
							<input
								id={id}
								aria-describedby={descriptionId()}
								class={s.rowSwitch}
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
						)}
					</SettingRow>
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
								<SettingRow label={tool.name} hint={tool.summary} help={tool.description}>
									{(id, descriptionId) => (
										<input
											id={id}
											aria-describedby={descriptionId()}
											class={s.rowSwitch}
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
									)}
								</SettingRow>
							);
						}}
					</For>
				</Show>

				<p class={s.hint} style={{ "margin-top": "16px", color: "var(--text-dimmed)" }}>
					{t("services.hint.autoSave", "Settings are saved automatically when changed")}
				</p>
			</SectionCard>
		</div>
	);
};
