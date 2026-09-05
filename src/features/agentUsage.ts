/**
 * Agent usage ticker — one status bar slot, whichever agent is in the terminal.
 *
 * Claude and Codex both expose rate limits, and showing the wrong one is worse
 * than showing none: a "7d: 12%" that belongs to the other vendor reads as
 * headroom the user does not have. So the ticker follows the active terminal's
 * detected `agentType` and polls only that agent's endpoint.
 *
 * When the active tab is a shell, or an agent with no usage API, the last known
 * agent stays on screen rather than blanking — switching to a shell for one
 * command should not wipe the number you were watching.
 *
 * Called from plugins/index.ts when the `claude-usage` feature is toggled.
 */

import { createEffect, createRoot } from "solid-js";

import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { mdTabsStore } from "../stores/mdTabs";
import { statusBarTicker } from "../stores/statusBarTicker";
import { terminalsStore } from "../stores/terminals";
import { buildTickerText, getTickerPriority, type UsageApiResponse } from "./claudeUsage";
import { buildCodexTickerText, type CodexUsageApiResponse, getCodexTickerPriority } from "./codexUsage";

const FEATURE_ID = "claude-usage";
const TICKER_ID = "claude-usage:rate";

/** Poll every 5 minutes — both Rust backends cache for 5 min. */
const API_POLL_MS = 5 * 60 * 1000;

/** Chart icon (inline SVG, monochrome) */
const CHART_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="M0 11.5a.5.5 0 0 1 .5-.5h4a.5.5 0 0 1 .5.5v4a.5.5 0 0 1-.5.5h-4a.5.5 0 0 1-.5-.5v-4zm6-4a.5.5 0 0 1 .5-.5h4a.5.5 0 0 1 .5.5v8a.5.5 0 0 1-.5.5h-4a.5.5 0 0 1-.5-.5v-8zm6-7a.5.5 0 0 1 .5-.5h3a.5.5 0 0 1 .5.5v15a.5.5 0 0 1-.5.5h-3a.5.5 0 0 1-.5-.5V.5z"/></svg>`;

/** Agents that expose a usage API. Anything else leaves the ticker as it was. */
export type UsageAgent = "claude" | "codex";

interface AgentUsageSpec {
	/** Ticker label — the whole point of the feature: name the agent on screen. */
	label: string;
	command: string;
	buildText: (api: never) => string;
	priority: (api: never) => number;
	/** Substring of the backend's "no credentials" error, for a clearer ticker. */
	missingTokenHint: string;
	/** Opens the agent's usage dashboard tab when the ticker is clicked. */
	openDashboard: () => void;
}

const SPECS: Record<UsageAgent, AgentUsageSpec> = {
	claude: {
		label: "Claude",
		command: "get_claude_usage_api",
		buildText: (api: UsageApiResponse) => buildTickerText(api),
		priority: (api: UsageApiResponse) => getTickerPriority(api),
		missingTokenHint: "No Claude OAuth token",
		openDashboard: () => mdTabsStore.addClaudeUsage(),
	} as AgentUsageSpec,
	codex: {
		label: "Codex",
		command: "get_codex_usage_api",
		buildText: (api: CodexUsageApiResponse) => buildCodexTickerText(api),
		priority: (api: CodexUsageApiResponse) => getCodexTickerPriority(api),
		missingTokenHint: "No Codex OAuth token",
		openDashboard: () => mdTabsStore.addCodexUsage(),
	} as AgentUsageSpec,
};

/** Map a detected agentType onto an agent with a usage API, or null. */
export function toUsageAgent(agentType: string | null | undefined): UsageAgent | null {
	return agentType === "claude" || agentType === "codex" ? agentType : null;
}

/** Classify a poll failure into the short text the ticker shows. */
export function describeUsageError(errStr: string, missingTokenHint: string): string {
	if (errStr.includes(missingTokenHint) || errStr.includes("No Codex credentials")) return "no token";
	if (errStr.includes("401") || errStr.includes("403")) return "token expired";
	if (errStr.includes("Failed to parse")) return "API changed";
	if (errStr.includes("404")) return "API moved";
	return "offline";
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

let pollTimer: ReturnType<typeof setInterval> | null = null;
let disposeEffect: (() => void) | null = null;
let initialized = false;

/** The agent currently on the ticker. Sticky: a shell tab does not clear it. */
let shownAgent: UsageAgent = "claude";

/** Guards against a slow poll for the previous agent overwriting the new one. */
let pollSeq = 0;

function activeUsageAgent(): UsageAgent | null {
	const id = terminalsStore.state.activeId;
	if (!id) return null;
	return toUsageAgent(terminalsStore.state.terminals[id]?.agentType);
}

async function poll(agent: UsageAgent): Promise<void> {
	const spec = SPECS[agent];
	const seq = ++pollSeq;

	const write = (text: string, priority: number) => {
		// A late response from the agent we just switched away from must not win.
		if (seq !== pollSeq) return;
		statusBarTicker.addMessage({
			id: TICKER_ID,
			pluginId: FEATURE_ID,
			label: spec.label,
			text,
			icon: CHART_SVG,
			priority,
			ttlMs: API_POLL_MS + 30_000,
			onClick: spec.openDashboard,
		});
	};

	try {
		const api = await invoke<never>(spec.command);
		write(spec.buildText(api), spec.priority(api));
	} catch (err) {
		const errStr = String(err);
		const text = describeUsageError(errStr, spec.missingTokenHint);
		if (text !== "no token") {
			appLogger.warn("network", `${spec.label} usage poll: ${text}`, errStr);
		}
		write(text, 5);
	}
}

/** Initialize the agent usage ticker (poll + follow the active terminal). */
export function initAgentUsage(): void {
	if (initialized) return;
	initialized = true;

	shownAgent = activeUsageAgent() ?? "claude";
	poll(shownAgent);
	pollTimer = setInterval(() => poll(shownAgent), API_POLL_MS);

	// Repoll immediately when the active tab moves to a different usage agent.
	// Tracked outside a component, so it needs its own reactive root.
	disposeEffect = createRoot((dispose) => {
		createEffect(() => {
			const next = activeUsageAgent();
			if (!next || next === shownAgent) return;
			shownAgent = next;
			poll(next);
		});
		return dispose;
	});
}

/** Tear down the agent usage ticker. */
export function destroyAgentUsage(): void {
	if (!initialized) return;
	initialized = false;

	if (pollTimer) {
		clearInterval(pollTimer);
		pollTimer = null;
	}
	disposeEffect?.();
	disposeEffect = null;

	statusBarTicker.removeMessage(TICKER_ID, FEATURE_ID);
}
