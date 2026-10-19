import { type Component, createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { t } from "../../../../i18n";
import { settingsRefresh } from "../../../../services/settingsRefresh";
import { appLogger } from "../../../../stores/appLogger";
import { rpc } from "../../../../transport";
import { writeClipboard } from "../../../../utils/clipboard";
import { StatusBadge } from "../../../ui/StatusBadge";
import { ContextualHelp } from "../../ContextualHelp";
import { ExpertSetting } from "../../ExpertSetting";
import s from "../../Settings.module.css";
import { SectionCard, SettingPageHeader, SettingRow } from "../../SettingsPrimitives";
import layout from "./ServiceRestyle.module.css";
import { type AppConfig, type NativeMcpTool, saveConfigField, useMcpStatusPoll } from "./servicesShared";
import { UpstreamMcpPanel } from "./UpstreamMcpPanel";

/**
 * Local MCP page: HTTP/MCP server status, the manual bridge configuration
 * snippet, and native-tool visibility controls. Upstream (remote) MCP
 * servers retain their own editor, mounted once inside Server connections.
 */
export const LocalMcpPanel: Component = () => {
	const { status } = useMcpStatusPoll();
	const [nativeTools, setNativeTools] = createStore<NativeMcpTool[]>([]);
	// Poll snapshots contain fresh objects; registry names own the row identity.
	createEffect(() => setNativeTools(reconcile(status()?.native_tools ?? [], { key: "name" })));
	const [disabledNativeTools, setDisabledNativeTools] = createSignal<string[]>([]);
	// The toggles wait for the saved list: the `[]` placeholder would show every
	// tool as enabled until `load_config` answers.
	const [nativeToolsLoaded, setNativeToolsLoaded] = createSignal(false);
	const [settingsEnabled, setSettingsEnabled] = createSignal(false);
	const [collapseTools, setCollapseTools] = createSignal<boolean>(false);
	const [bridgeInfo, setBridgeInfo] = createSignal<{ bridge_path: string; config_snippet: string } | null>(null);
	const [bridgeInfoOpen, setBridgeInfoOpen] = createSignal(false);
	const [snippetCopied, setSnippetCopied] = createSignal(false);

	onCleanup(
		settingsRefresh.register({
			section: "app",
			read: () =>
				rpc<AppConfig>("load_config").catch((error) => {
					appLogger.warn("config", "Failed to load native tool config, using defaults", error);
					setNativeToolsLoaded(true);
					return undefined;
				}),
			apply: (config) => {
				if (!config) return;
				setDisabledNativeTools(config.disabled_native_tools ?? []);
				setSettingsEnabled(config.settings_mcp_enabled ?? false);
				setCollapseTools(config.collapse_tools ?? false);
				setNativeToolsLoaded(true);
			},
		}),
	);

	return (
		<div class={`${s.section} ${layout.page}`}>
			<SettingPageHeader
				title="MCP"
				description="Local MCP connection, tool visibility and upstream servers."
				scope="This machine"
				breadcrumb="Integrations / MCP"
			/>
			<SectionCard title="Server connections">
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

				<SettingRow label={t("services.label.mcpConnection", "MCP Connection")}>
					{() => <StatusBadge label="tuic-bridge" />}
				</SettingRow>

				<UpstreamMcpPanel />
				{/* ── TUIC Tools ── */}
			</SectionCard>
			<SectionCard title="TUIC Tools">
				<div class={s.group}>
					<p class={s.hint}>Native tools exposed via MCP. Disable tools to restrict what AI agents can access.</p>
				</div>

				<div class={s.group}>
					<button
						data-non-setting="Show the bridge integration instructions; not a preference"
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
									data-non-setting="Copy the existing MCP bridge integration snippet"
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
								data-setting-id="app.collapse_tools"
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
						<label>
							Native tools <ContextualHelp label="Native tools" />
						</label>
						<p class={s.hint}>
							Choose which built-in tools MCP clients can call. Disabled tools are hidden from clients.
						</p>
					</div>
					<For each={nativeTools}>
						{(tool) => {
							const disabled = () =>
								disabledNativeTools().includes(tool.name) || (tool.name === "settings" && !settingsEnabled());
							return (
								<SettingRow label={tool.name} hint={tool.summary} help={tool.description}>
									{(id, descriptionId) => (
										<input
											data-setting-id="app.disabled_native_tools"
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
													if (tool.name === "settings") {
														c.settings_mcp_enabled = enabled;
														setSettingsEnabled(enabled);
													}
												});
											}}
										/>
									)}
								</SettingRow>
							);
						}}
					</For>
				</Show>
			</SectionCard>
		</div>
	);
};
