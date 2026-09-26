import { createSignal } from "solid-js";
import type { AgentType } from "../agents";
import { rpc, subscribeEvents, type Unsubscribe } from "../transport";
import { appLogger } from "./appLogger";

/**
 * "Your shell already has its own `claude`/`codex`/`goose` function — wrap
 * it?" — see `src-tauri/src/agent_wrap_prompt.rs`'s module doc comment for
 * the full backend mechanism and why this is a separate mechanism from
 * `mcpConfirm.ts` rather than reusing its bool-only shape (a dismiss here
 * must NOT collapse to the same outcome as an explicit "No").
 */
export interface AgentWrapPromptRequest {
	requestId: string;
	agentType: string;
}

interface AgentWrapPromptPayload {
	request_id: string;
	agent_type: string;
}

interface AgentWrapPromptResolvedPayload {
	request_id: string;
	agent_type: string;
	decision: boolean | null;
}

// Keyed by agent type, not a single ordered queue like McpConfirm's — claude,
// codex and goose can each have their own prompt pending at the same time.
const [pending, setPending] = createSignal<Record<string, AgentWrapPromptRequest>>({});

export const pendingAgentWrapPrompts = () => Object.values(pending());

/** Test seam: reset between cases. */
export function __resetAgentWrapPromptQueue() {
	setPending({});
}

function enqueue(payload: AgentWrapPromptPayload) {
	setPending((prev) => {
		// A reconnecting SSE client can be handed the same request twice —
		// keep the first insert's identity instead of clobbering it, same
		// reasoning as mcpConfirm.ts's dedup-by-requestId guard.
		if (prev[payload.agent_type]?.requestId === payload.request_id) return prev;
		return {
			...prev,
			[payload.agent_type]: { requestId: payload.request_id, agentType: payload.agent_type },
		};
	});
}

function drop(agentType: string) {
	setPending((prev) => {
		if (!(agentType in prev)) return prev;
		const next = { ...prev };
		delete next[agentType];
		return next;
	});
}

/**
 * Answer (`decision: true`/`false`) or dismiss (`decision: null`) the prompt
 * for `agentType`.
 *
 * Drops it locally first — same reasoning as `answerMcpConfirm`: every
 * client is shown the same request, so the backend's `Resolved` broadcast
 * may arrive after the user has already moved on, and a dialog that lingers
 * until the round trip completes invites a second click on a question
 * that's already settled. An explicit answer also updates the local
 * `agentConfigsStore` mirror immediately, so Settings → Agents reflects it
 * without waiting on a full config reload.
 *
 * `agentConfigsStore` is imported lazily here — it's a large store with no
 * other reason to be in this module's eager import graph (mounted by both
 * shells, so a static import would ship the whole store into every client's
 * initial bundle just for this rare write-path).
 */
export async function answerAgentWrapPrompt(
	requestId: string,
	agentType: string,
	decision: boolean | null,
): Promise<void> {
	drop(agentType);
	if (decision !== null) {
		const { agentConfigsStore } = await import("./agentConfigs");
		agentConfigsStore.syncWrapUserFunction(agentType as AgentType, decision);
	}
	try {
		await rpc("agent_wrap_prompt_response", { requestId, agentType, decision });
	} catch (err) {
		appLogger.warn(
			"network",
			`Failed to deliver agent-wrap-prompt answer: ${err instanceof Error ? err.message : String(err)}`,
		);
	}
}

/**
 * Listen for "wrap my shell function?" prompts.
 *
 * Every client subscribes, same reasoning as `subscribeMcpConfirm` — the
 * first client to answer wins, and `agent-wrap-prompt-resolved` tells the
 * others to take the dialog down.
 */
export function subscribeAgentWrapPrompt(): Promise<Unsubscribe> {
	return subscribeEvents({
		"agent-wrap-prompt": (payload) => enqueue(payload as AgentWrapPromptPayload),
		"agent-wrap-prompt-resolved": (payload) => drop((payload as AgentWrapPromptResolvedPayload).agent_type),
	});
}
