import { type Component, createSignal, For, onMount, Show } from "solid-js";
import { t } from "../../../i18n";
import { appLogger } from "../../../stores/appLogger";
import { settingsStore } from "../../../stores/settings";
import { rpc } from "../../../transport";
import { writeClipboard } from "../../../utils/clipboard";
import { isAbsolutePath } from "../../../utils/pathUtils";
import { updateAppConfig } from "../../../utils/updateAppConfig";
import s from "../Settings.module.css";
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
	// --- Additional HTTP-readable directories ---
	const [newReadableDir, setNewReadableDir] = createSignal("");
	const [readableDirError, setReadableDirError] = createSignal<string | null>(null);
	const readableDirs = (): string[] => settingsStore.state.additionalReadableDirs;
	/** Mirrors the backend's `expand_readable_root` acceptance rule (fs_routes.rs) —
	 *  an absolute path, or `~`/`~/...` — so a client-side entry that can never
	 *  resolve to a real root is rejected here instead of silently sitting in the
	 *  list looking identical to a working entry. Defense-in-depth only: the
	 *  backend re-validates independently and is the actual enforcement point. */
	const isValidReadableDirEntry = (entry: string): boolean =>
		!entry.includes("..") && (entry === "~" || entry.startsWith("~/") || isAbsolutePath(entry));
	const addReadableDir = () => {
		const dir = newReadableDir().trim();
		if (!dir) return;
		if (!isValidReadableDirEntry(dir)) {
			setReadableDirError(
				t("services.readableDirs.invalid", "Must be an absolute path, or start with ~/ for your home directory"),
			);
			return;
		}
		setReadableDirError(null);
		if (readableDirs().includes(dir)) return;
		settingsStore.setAdditionalReadableDirs([...readableDirs(), dir]);
		setNewReadableDir("");
	};
	const removeReadableDir = (dir: string) =>
		settingsStore.setAdditionalReadableDirs(readableDirs().filter((d) => d !== dir));

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
			<h3>{t("services.heading.fileAccess", "File Access")}</h3>

			<div class={s.group}>
				<label>{t("services.label.additionalReadableDirs", "Additional Readable Directories")}</label>
				<p class={s.hint}>
					{t(
						"services.hint.additionalReadableDirs",
						"Absolute directories that web and remote clients may READ files from, in addition to your registered repositories. Desktop reads are never restricted. Never widens writing, copying, or moving. Use ~ for your home directory.",
					)}
				</p>

				<For each={readableDirs()}>
					{(dir) => (
						<div class={s.copyPathRow}>
							<span class={s.copyPathText}>{dir}</span>
							<button type="button" class={s.transferBtn} onClick={() => removeReadableDir(dir)}>
								{t("services.readableDirs.remove", "Remove")}
							</button>
						</div>
					)}
				</For>

				<div class={s.copyPathRow}>
					<input
						type="text"
						class={s.copyPathInput}
						value={newReadableDir()}
						onInput={(e) => {
							setNewReadableDir(e.currentTarget.value);
							setReadableDirError(null);
						}}
						onKeyDown={(e) => {
							if (e.key === "Enter") addReadableDir();
						}}
						placeholder={t("services.readableDirs.placeholder", "e.g. ~/.claude/plans")}
					/>
					<button type="button" class={s.transferBtn} onClick={addReadableDir} disabled={!newReadableDir().trim()}>
						{t("services.readableDirs.add", "Add")}
					</button>
				</div>
				<Show when={readableDirError()}>
					<p class={s.hint} style={{ color: "var(--error)" }}>
						{readableDirError()}
					</p>
				</Show>
			</div>

			{/* ── TUIC Tools ── */}
			<h3>TUIC Tools</h3>
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

export const ServicesTab: Component = () => (
	<div class={s.section}>
		<LocalServicesPanel />
		<UpstreamMcpPanel />
		<p class={s.hint} style={{ "margin-top": "16px", color: "var(--text-dimmed)" }}>
			{t("services.hint.autoSave", "Settings are saved automatically when changed")}
		</p>
	</div>
);
