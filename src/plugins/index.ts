import { destroyAgentUsage, initAgentUsage } from "../features/agentUsage";
import { pluginStore } from "../stores/pluginStore";
import { planPlugin } from "./planPlugin";
import { isPluginDisabled, loadUserPlugins, registerBuiltInPlugin, syncDisabledList } from "./pluginLoader";
import { pluginRegistry } from "./pluginRegistry";
import { storiesTickerPlugin } from "./storiesTickerPlugin";
import type { TuiPlugin } from "./types";

/**
 * Built-in plugins shipped with TUICommander.
 * Session prompts moved to native Rust (last_prompts in AppState, displayed in Activity Dashboard).
 * Claude Usage Dashboard was moved from plugin to native feature (src/features/claudeUsage.ts).
 */
const BUILTIN_PLUGINS: TuiPlugin[] = [planPlugin, storiesTickerPlugin];

/**
 * Register all built-in plugins, then discover and load user plugins.
 * Also initializes native features that use the disabled_plugin_ids toggle.
 * Call once at app startup.
 */
export async function initPlugins(): Promise<void> {
	// Sync disabled list before checking built-in plugin state
	await syncDisabledList();

	await Promise.all(
		BUILTIN_PLUGINS.map(async (plugin) => {
			registerBuiltInPlugin(plugin);
			const enabled = !isPluginDisabled(plugin.id);
			pluginStore.registerPlugin(plugin.id, { builtIn: true, enabled });
			if (enabled) {
				await pluginRegistry.register(plugin);
			}
		}),
	);

	// Native agent usage ticker (Claude + Codex) — uses the same
	// disabled_plugin_ids toggle, still keyed "claude-usage".
	if (!isPluginDisabled("claude-usage")) {
		initAgentUsage();
	}

	await loadUserPlugins(false);
}

/**
 * Toggle the native agent usage ticker (Claude + Codex).
 * Called from AgentsTab when user flips the dashboard toggle.
 */
export function setClaudeUsageEnabled(enabled: boolean): void {
	if (enabled) {
		initAgentUsage();
	} else {
		destroyAgentUsage();
	}
}
