/**
 * Unified conversation store — merges aiChatStore and aiAgentStore.
 *
 * Per-terminal architecture: each terminal gets its own PerTerminalConversationState
 * keyed by tuicSession/id. Proxy accessors (messages, isStreaming, etc.) route through
 * activeConversation() so all callers remain unchanged.
 *
 * Both chat (assisted) and agent (autonomous) modes now drive the same Tauri command
 * `start_conversation`. Events arrive via Channel<ConversationEvent> and are applied
 * to the matching terminal's state.
 */

import type { Accessor, Setter } from "solid-js";
import { batch, createSignal } from "solid-js";
import { invoke } from "../invoke";
import { isTauri } from "../transport";
import { openConversationStream } from "../utils/aiStream";
import { appLogger } from "./appLogger";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ConversationMessage {
	role: "user" | "assistant" | "system";
	content: string;
	timestamp: number;
}

/** Re-exported as AiChatMessage for backward compat */
export type AiChatMessage = ConversationMessage;

interface ChatUsage {
	promptTokens?: number;
	completionTokens?: number;
	totalTokens?: number;
	cachedTokens?: number;
	cacheCreationTokens?: number;
	costUsd?: number;
}

export type AgentState = "running" | "paused" | "completed" | "cancelled" | "error" | "idle";

export type ToolCallEntry =
	| { status: "pending"; toolName: string; args: Record<string, unknown>; startedAt: number }
	| {
			status: "done";
			toolName: string;
			args: Record<string, unknown>;
			startedAt: number;
			result: { success: boolean; output: string };
			duration: number;
	  };

export interface PendingApproval {
	sessionId: string;
	command: string;
	reason: string;
}

// Backend conversation types (mirror ai_agent::conversation)
interface BackendChatMessage {
	role: "user" | "assistant" | "system";
	content: string;
	timestamp: number;
}
interface BackendConversationMeta {
	id: string;
	title: string;
	session_id?: string | null;
	created: number;
	updated: number;
	message_count: number;
	provider?: string;
	model?: string;
}
/** The agent run as the backend stores it — the panel's own state, verbatim,
 * so neither side reshapes it. Absent in documents written before schema 3. */
interface BackendAgentSnapshot {
	state: AgentState;
	currentIteration: number;
	toolCalls: ToolCallEntry[];
}
interface BackendConversation {
	meta: BackendConversationMeta;
	messages: BackendChatMessage[];
	agent?: BackendAgentSnapshot;
	/** Stamped by the backend on save; never sent from here. */
	schema_version?: number;
}

export type ConversationMeta = BackendConversationMeta;

// ConversationEvent variants from Rust (tag = "type", rename_all = "snake_case")
type ConversationEvent =
	| { type: "thinking"; iteration: number }
	| { type: "text_chunk"; text: string }
	| { type: "reasoning_chunk"; text: string }
	| { type: "tool_call"; tool_name: string; args: Record<string, unknown> }
	| { type: "tool_result"; tool_name: string; success: boolean; output: string }
	| { type: "needs_approval"; tool_name: string; command: string; reason: string }
	| { type: "bypassed"; tool_name: string }
	| { type: "paused" }
	| { type: "resumed" }
	| { type: "rate_limited"; wait_ms: number }
	| { type: "retrying"; attempt: number; wait_ms: number; reason: string }
	| { type: "compacted"; elided: number; before_tokens: number }
	| { type: "error"; message: string }
	| { type: "completed"; reason: string; usage: { input_tokens: number; output_tokens: number } | null };

// Legacy AgentEvent for backward compat with old agent-loop-event listener (removed in 1617)
type LegacyAgentEvent =
	| { type: "started"; session_id: string }
	| { type: "thinking"; session_id: string; iteration: number }
	| { type: "text_chunk"; session_id: string; text: string }
	| { type: "reasoning_chunk"; session_id: string; text: string }
	| { type: "tool_call"; session_id: string; tool_name: string; args: Record<string, unknown> }
	| { type: "tool_result"; session_id: string; tool_name: string; success: boolean; output: string }
	| { type: "needs_approval"; session_id: string; tool_name: string; command: string; reason: string }
	| { type: "paused"; session_id: string }
	| { type: "resumed"; session_id: string }
	| { type: "rate_limited"; session_id: string; wait_ms: number }
	| { type: "error"; session_id: string; message: string }
	| { type: "completed"; session_id: string; iterations: number; reason: string };

/** Conversation mode: "assisted" = chat streaming, "autonomous" = agent with tool cards */
type ConversationMode = "assisted" | "autonomous";

export interface PerTerminalConversationState {
	// Chat state (formerly aiChatStore)
	messages: Accessor<ConversationMessage[]>;
	setMessages: Setter<ConversationMessage[]>;
	isStreaming: Accessor<boolean>;
	setIsStreaming: Setter<boolean>;
	streamingText: Accessor<string>;
	setStreamingText: Setter<string>;
	error: Accessor<string | null>;
	setError: Setter<string | null>;
	chatId: Accessor<string>;
	setChatId: Setter<string>;
	sessionUsage: Accessor<ChatUsage | null>;
	setSessionUsage: Setter<ChatUsage | null>;
	// Agent state (formerly aiAgentStore)
	agentState: Accessor<AgentState>;
	setAgentState: Setter<AgentState>;
	currentIteration: Accessor<number>;
	setCurrentIteration: Setter<number>;
	toolCalls: Accessor<ToolCallEntry[]>;
	setToolCalls: Setter<ToolCallEntry[]>;
	textChunks: Accessor<string>;
	setTextChunks: Setter<string>;
	reasoningChunks: Accessor<string>;
	setReasoningChunks: Setter<string>;
	pendingApproval: Accessor<PendingApproval | null>;
	setPendingApproval: Setter<PendingApproval | null>;
	agentError: Accessor<string | null>;
	setAgentError: Setter<string | null>;
	completionReason: Accessor<string | null>;
	setCompletionReason: Setter<string | null>;
	unrestricted: Accessor<boolean>;
	setUnrestricted: Setter<boolean>;
	isThinking: Accessor<boolean>;
	setIsThinking: Setter<boolean>;
	// Bookkeeping
	currentMode: ConversationMode | null;
	activeSessionId: string | null;
	persistTimer: ReturnType<typeof setTimeout> | null;
	initialized: boolean;
	// Browser/PWA only: disposer for the active conversation token-stream WS.
	// Desktop uses a Tauri Channel (auto-cleaned), so this stays null there.
	conversationStreamDispose: (() => void) | null;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MAX_MESSAGES = 100;
const MAX_TOOL_CALLS = 500;
const PERSIST_DEBOUNCE_MS = 500;
const DEFAULT_KEY = "__default__";

/**
 * Ceiling on the serialized tool-call log, in bytes.
 *
 * `MAX_TOOL_CALLS` alone does not bound the file: the two caps compound rather
 * than trade off, because the backend independently caps each captured output
 * at 8192 bytes (`conversation.rs TOOL_RESULT_MAX_BYTES`). Measured against the
 * exact shape `save_conversation` writes — `serde_json::to_string_pretty`, which
 * is what `JSON.stringify(x, null, 2)` produces — one entry at that output cap
 * costs **8574 bytes on disk**, so 500 of them are **4.13 MB**. Persistence is a
 * whole-document rewrite on a 500 ms debounce, so a sustained run rewrote up to
 * **8.26 MB/s**, indefinitely.
 *
 * 512 KB caps that at about 1 MB/s, an 8x cut, and it is not an arbitrary
 * round number: measured at the same shape, a run whose outputs average under
 * roughly 1 KB keeps all 500 entries and is untouched by this cap, which is the
 * ordinary case. Only a run that is genuinely producing megabytes of tool output
 * loses anything, and it loses the oldest.
 */
const MAX_TOOL_CALL_BYTES = 512 * 1024;

/**
 * Apply both caps to the tool-call log, dropping from the OLDEST end.
 *
 * The count cap is cheap and runs first; the byte cap then walks backwards from
 * the newest entry and stops at the first one that would cross the ceiling. The
 * newest entry is always kept even if it alone exceeds the ceiling — a log whose
 * last entry is missing tells the user nothing about where the run got to, which
 * is the only reason the log is persisted at all.
 *
 * Entries are measured pretty-printed, because that is how they are written. The
 * sum still runs a few percent under the bytes the document costs: nesting the
 * array inside the document indents every line further, and that overhead is not
 * knowable from one entry. The ceiling is a policy number, not a contract with
 * the filesystem, so the gap is documented rather than modelled — the tests pin
 * both this sum and the resulting document size.
 */
function trimToolCalls(calls: readonly ToolCallEntry[]): ToolCallEntry[] {
	const capped = calls.length > MAX_TOOL_CALLS ? calls.slice(calls.length - MAX_TOOL_CALLS) : [...calls];
	let bytes = 0;
	for (let i = capped.length - 1; i >= 0; i--) {
		bytes += JSON.stringify(capped[i], null, 2).length;
		if (bytes > MAX_TOOL_CALL_BYTES && i < capped.length - 1) return capped.slice(i + 1);
	}
	return capped;
}

// ---------------------------------------------------------------------------
// Per-terminal state map
// ---------------------------------------------------------------------------

const stateMap = new Map<string, PerTerminalConversationState>();
const [activeKey, setActiveKey] = createSignal<string>(DEFAULT_KEY);

function generateChatId(): string {
	return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
}

function createState(): PerTerminalConversationState {
	const [messages, setMessages] = createSignal<ConversationMessage[]>([]);
	const [isStreaming, setIsStreaming] = createSignal(false);
	const [streamingText, setStreamingText] = createSignal("");
	const [error, setError] = createSignal<string | null>(null);
	const [chatId, setChatId] = createSignal(generateChatId());
	const [sessionUsage, setSessionUsage] = createSignal<ChatUsage | null>(null);
	const [agentState, setAgentState] = createSignal<AgentState>("idle");
	const [currentIteration, setCurrentIteration] = createSignal(0);
	const [toolCalls, setToolCalls] = createSignal<ToolCallEntry[]>([]);
	const [textChunks, setTextChunks] = createSignal("");
	const [reasoningChunks, setReasoningChunks] = createSignal("");
	const [pendingApproval, setPendingApproval] = createSignal<PendingApproval | null>(null);
	const [agentError, setAgentError] = createSignal<string | null>(null);
	const [completionReason, setCompletionReason] = createSignal<string | null>(null);
	const [unrestricted, setUnrestricted] = createSignal(false);
	const [isThinking, setIsThinking] = createSignal(false);
	return {
		messages,
		setMessages,
		isStreaming,
		setIsStreaming,
		streamingText,
		setStreamingText,
		error,
		setError,
		chatId,
		setChatId,
		sessionUsage,
		setSessionUsage,
		agentState,
		setAgentState,
		currentIteration,
		setCurrentIteration,
		toolCalls,
		setToolCalls,
		textChunks,
		setTextChunks,
		reasoningChunks,
		setReasoningChunks,
		pendingApproval,
		setPendingApproval,
		agentError,
		setAgentError,
		completionReason,
		setCompletionReason,
		unrestricted,
		setUnrestricted,
		isThinking,
		setIsThinking,
		currentMode: null,
		activeSessionId: null,
		persistTimer: null,
		initialized: false,
		conversationStreamDispose: null,
	};
}

/** Close the active browser conversation token-stream WS, if any. No-op on
 * desktop (Channel transport) and when no stream is open. */
function closeConversationStream(s: PerTerminalConversationState): void {
	if (s.conversationStreamDispose) {
		s.conversationStreamDispose();
		s.conversationStreamDispose = null;
	}
}

function getOrCreate(key: string): PerTerminalConversationState {
	let s = stateMap.get(key);
	if (!s) {
		s = createState();
		stateMap.set(key, s);
	}
	return s;
}

function activeConversation(): PerTerminalConversationState {
	return getOrCreate(activeKey());
}

function setActiveTerminal(key: string): void {
	setActiveKey(key);
}

// ---------------------------------------------------------------------------
// Convenience accessors — proxy through activeConversation()
// ---------------------------------------------------------------------------

function messages(): ConversationMessage[] {
	return activeConversation().messages();
}
function isStreaming(): boolean {
	return activeConversation().isStreaming();
}
function streamingText(): string {
	return activeConversation().streamingText();
}
function error(): string | null {
	return activeConversation().error();
}
function chatId(): string {
	return activeConversation().chatId();
}
function sessionUsage(): ChatUsage | null {
	return activeConversation().sessionUsage();
}
function agentState(): AgentState {
	return activeConversation().agentState();
}
function currentIteration(): number {
	return activeConversation().currentIteration();
}
function toolCalls(): ToolCallEntry[] {
	return activeConversation().toolCalls();
}
function textChunks(): string {
	return activeConversation().textChunks();
}
function reasoningChunks(): string {
	return activeConversation().reasoningChunks();
}
function pendingApproval(): PendingApproval | null {
	return activeConversation().pendingApproval();
}
function agentError(): string | null {
	return activeConversation().agentError();
}
function completionReason(): string | null {
	return activeConversation().completionReason();
}
function unrestricted(): boolean {
	return activeConversation().unrestricted();
}
function setUnrestricted(value: boolean): void {
	activeConversation().setUnrestricted(value);
}
function isThinking(): boolean {
	return activeConversation().isThinking();
}

// ---------------------------------------------------------------------------
// Message management
// ---------------------------------------------------------------------------

function addMessage(role: ConversationMessage["role"], content: string): void {
	const key = activeKey();
	const s = getOrCreate(key);
	s.setMessages((prev) => {
		const msg: ConversationMessage = { role, content, timestamp: Date.now() };
		const next = [...prev, msg];
		return next.length > MAX_MESSAGES ? next.slice(next.length - MAX_MESSAGES) : next;
	});
	schedulePersist(key);
}

function addUserMessage(content: string): void {
	addMessage("user", content);
}
function addAssistantMessage(content: string): void {
	addMessage("assistant", content);
}
function addSystemMessage(content: string): void {
	addMessage("system", content);
}

function accumulateUsage(usage: ChatUsage): void {
	const s = activeConversation();
	s.setSessionUsage((prev) => ({
		promptTokens: (prev?.promptTokens ?? 0) + (usage.promptTokens ?? 0),
		completionTokens: (prev?.completionTokens ?? 0) + (usage.completionTokens ?? 0),
		totalTokens: (prev?.totalTokens ?? 0) + (usage.totalTokens ?? 0),
		cachedTokens: (prev?.cachedTokens ?? 0) + (usage.cachedTokens ?? 0),
		cacheCreationTokens: (prev?.cacheCreationTokens ?? 0) + (usage.cacheCreationTokens ?? 0),
		costUsd: usage.costUsd != null ? (prev?.costUsd ?? 0) + usage.costUsd : prev?.costUsd,
	}));
}

function clearHistory(): void {
	const s = activeConversation();
	if (s.persistTimer) {
		clearTimeout(s.persistTimer);
		s.persistTimer = null;
	}
	batch(() => {
		s.setMessages([]);
		s.setStreamingText("");
		s.setIsStreaming(false);
		s.setError(null);
		s.setSessionUsage(null);
	});
	const oldId = s.chatId();
	void (async () => {
		try {
			await invoke("delete_conversation", { id: oldId });
			const newId = await invoke<string>("new_conversation_id");
			s.setChatId(newId);
		} catch (e) {
			appLogger.warn("conversation", "clearHistory: backend wipe failed", { error: String(e) });
		}
	})();
}

// ---------------------------------------------------------------------------
// Persistence (debounced autosave + init load)
// ---------------------------------------------------------------------------

function schedulePersist(key?: string): void {
	const resolvedKey = key ?? activeKey();
	const s = getOrCreate(resolvedKey);
	if (s.persistTimer) clearTimeout(s.persistTimer);
	s.persistTimer = setTimeout(() => {
		s.persistTimer = null;
		void persistNow(resolvedKey);
	}, PERSIST_DEBOUNCE_MS);
}

/** Whether there is anything worth writing to disk. An agent run adds no
 * message until it ends, so "no messages" is not "nothing happened". */
function hasPersistableState(s: PerTerminalConversationState): boolean {
	return s.messages().length > 0 || s.toolCalls().length > 0 || s.agentState() !== "idle";
}

async function persistNow(key?: string): Promise<void> {
	const resolvedKey = key ?? activeKey();
	const s = getOrCreate(resolvedKey);
	const msgs = s.messages();
	if (!hasPersistableState(s)) return;
	try {
		const id = s.chatId();
		const now = Date.now();
		const firstUser = msgs.find((m) => m.role === "user");
		const title = firstUser ? firstUser.content.slice(0, 60).replace(/\s+/g, " ").trim() : "New chat";
		let provider: string | undefined;
		let model: string | undefined;
		try {
			const cfg = await invoke<{ provider: string; model: string }>("load_ai_chat_config");
			provider = cfg.provider || undefined;
			model = cfg.model || undefined;
		} catch (e) {
			appLogger.debug("conversation", "load_ai_chat_config unavailable, omitting provider metadata", {
				error: String(e),
			});
		}
		const conv: BackendConversation = {
			meta: {
				id,
				title: title || "New chat",
				session_id: resolvedKey === DEFAULT_KEY ? null : resolvedKey,
				created: msgs[0]?.timestamp ?? now,
				updated: now,
				message_count: msgs.length,
				provider,
				model,
			},
			messages: msgs.map((m) => ({ role: m.role, content: m.content, timestamp: m.timestamp })),
			agent: {
				state: s.agentState(),
				currentIteration: s.currentIteration(),
				toolCalls: s.toolCalls(),
			},
		};
		await invoke("save_conversation", { conversation: conv });
	} catch (e) {
		appLogger.warn("conversation", "persistNow failed", { error: String(e) });
	}
}

/**
 * Apply a conversation read off disk to a terminal's state.
 *
 * Shared by `initFromDisk` and `loadConversation` so a restored run can never
 * come back in one of them and not the other.
 *
 * DEFERRED (2026-09-06) — a snapshot saved while the loop was `running` comes
 * back as `running` even when the backend that ran it is gone (app restart).
 * The banner then shows a run nobody is driving; Stop clears it. Reconciling
 * needs `agent_loop_status` / `ACTIVE_CONVERSATIONS` per session, which is a
 * wider change than restoring the state the reload lost (705-57fa).
 */
function applyLoadedConversation(s: PerTerminalConversationState, conv: BackendConversation): void {
	batch(() => {
		s.setChatId(conv.meta.id);
		s.setMessages(
			conv.messages
				.filter((m) => m.role === "user" || m.role === "assistant" || m.role === "system")
				.map((m) => ({
					role: m.role as ConversationMessage["role"],
					content: m.content ?? "",
					timestamp: m.timestamp,
				}))
				.slice(-MAX_MESSAGES),
		);
		// Absent for anything written before schema 3 — an idle agent, as before.
		const agent = conv.agent;
		s.setAgentState(agent?.state ?? "idle");
		s.setCurrentIteration(agent?.currentIteration ?? 0);
		s.setToolCalls(trimToolCalls(agent?.toolCalls ?? []));
		s.setStreamingText("");
		s.setIsStreaming(false);
		s.setError(null);
	});
}

async function initFromDisk(tuicSession?: string): Promise<void> {
	const s = activeConversation();
	if (s.initialized) return;
	s.initialized = true;
	try {
		if (tuicSession) {
			try {
				const metas = await invoke<BackendConversationMeta[]>("list_conversations");
				const match = metas
					.filter((m) => m.session_id === tuicSession)
					.reduce<BackendConversationMeta | undefined>(
						(best, m) => (!best || m.updated > best.updated ? m : best),
						undefined,
					);
				if (match) {
					const conv = await invoke<BackendConversation>("load_conversation", { id: match.id });
					applyLoadedConversation(s, conv);
					return;
				}
			} catch (e) {
				appLogger.info("conversation", "no saved conversation for session, starting new", {
					tuicSession,
					error: String(e),
				});
			}
			const newId = await invoke<string>("new_conversation_id");
			s.setChatId(newId);
			return;
		}
		const newId = await invoke<string>("new_conversation_id");
		s.setChatId(newId);
	} catch (e) {
		appLogger.warn("conversation", "initFromDisk failed", { error: String(e) });
	}
}

// ---------------------------------------------------------------------------
// ConversationEvent handler (Channel-based, used by both chat and agent)
// ---------------------------------------------------------------------------

/**
 * Events after which the conversation on disk is stale.
 *
 * Token chunks are deliberately absent: the save is debounced, so a per-token
 * schedule would reset the timer on every token and never fire — while writing
 * the file once per pause. What a reload needs back is the run around the
 * chunks, and every event that moves it is here.
 */
const PERSISTED_EVENT_TYPES: ReadonlySet<ConversationEvent["type"]> = new Set([
	"thinking",
	"tool_call",
	"tool_result",
	"paused",
	"resumed",
	"error",
	"completed",
]);

function applyConversationEvent(s: PerTerminalConversationState, event: ConversationEvent, ownerKey?: string): void {
	const mode = s.currentMode ?? "assisted";
	switch (event.type) {
		case "thinking":
			// The engine runs the same loop for both modes, so an assisted turn
			// emits this too. Only an agent has a loop to report: marking one
			// running here put the agent banner on a plain chat, and the assisted
			// path has nothing that clears it again.
			if (mode === "autonomous") {
				s.setCurrentIteration(event.iteration);
				s.setAgentState("running");
			}
			s.setIsThinking(true);
			break;

		case "text_chunk":
			s.setIsThinking(false);
			if (mode === "autonomous") {
				s.setTextChunks((prev) => prev + event.text);
			} else {
				s.setStreamingText((prev) => prev + event.text);
			}
			break;

		case "reasoning_chunk":
			// Extended-thinking stream (Opus 4.7+). Accumulate for the disclosure;
			// reset happens at the start of each new user turn (see sendMessage).
			s.setReasoningChunks((prev) => prev + event.text);
			break;

		case "tool_call": {
			s.setIsThinking(false);
			const entry: ToolCallEntry = {
				status: "pending",
				toolName: event.tool_name,
				args: event.args as Record<string, unknown>,
				startedAt: Date.now(),
			};
			s.setToolCalls((prev) => trimToolCalls([...prev, entry]));
			break;
		}

		case "tool_result":
			s.setToolCalls((prev) => {
				const updated = [...prev];
				for (let i = updated.length - 1; i >= 0; i--) {
					if (updated[i].toolName === event.tool_name && updated[i].status === "pending") {
						updated[i] = {
							...updated[i],
							status: "done",
							result: { success: event.success, output: event.output },
							duration: Date.now() - updated[i].startedAt,
						};
						break;
					}
				}
				// The output only arrives here, so this is where the log grows.
				return trimToolCalls(updated);
			});
			break;

		case "needs_approval":
			if (s.activeSessionId) {
				s.setPendingApproval({ sessionId: s.activeSessionId, command: event.command, reason: event.reason });
			} else {
				appLogger.warn("conversation", "needs_approval event dropped — no active session");
			}
			break;

		case "bypassed":
			// Silently skip — bypassed tools run without approval
			break;

		case "paused":
			s.setAgentState("paused");
			s.setIsThinking(false);
			break;

		case "resumed":
			s.setAgentState("running");
			s.setIsThinking(false);
			break;

		case "rate_limited":
			appLogger.info("conversation", `Rate limited, waiting ${event.wait_ms}ms`);
			break;

		case "retrying":
			appLogger.info(
				"conversation",
				`Retrying LLM call (attempt ${event.attempt}) in ${event.wait_ms}ms: ${event.reason}`,
			);
			break;

		case "compacted":
			appLogger.info(
				"conversation",
				`History compacted: elided ${event.elided} old tool result(s) at ${event.before_tokens} tokens`,
			);
			break;

		case "error":
			batch(() => {
				s.setIsThinking(false);
				if (mode === "autonomous") {
					s.setAgentState("error");
					s.setAgentError(event.message);
				} else {
					s.setIsStreaming(false);
					s.setStreamingText("");
					s.setError(event.message);
				}
			});
			break;

		case "completed": {
			const usage = event.usage;
			s.setIsThinking(false);
			if (usage) {
				appLogger.info("conversation", `usage: input=${usage.input_tokens} output=${usage.output_tokens}`);
				accumulateUsageForState(s, usage);
			}
			if (mode === "autonomous") {
				batch(() => {
					s.setAgentState("completed");
					s.setCompletionReason(event.reason);
				});
			} else {
				const full = s.streamingText();
				batch(() => {
					s.setIsStreaming(false);
					s.setStreamingText("");
					if (full) {
						s.setMessages((prev) => {
							const msg: ConversationMessage = { role: "assistant", content: full, timestamp: Date.now() };
							const next = [...prev, msg];
							return next.length > MAX_MESSAGES ? next.slice(next.length - MAX_MESSAGES) : next;
						});
					}
				});
			}
			break;
		}
	}

	// After the switch, so the save reads the state this event just produced.
	if (PERSISTED_EVENT_TYPES.has(event.type)) {
		schedulePersist(ownerKey ?? activeKey());
	}
}

function accumulateUsageForState(
	s: PerTerminalConversationState,
	usage: { input_tokens: number; output_tokens: number },
): void {
	s.setSessionUsage((prev) => ({
		promptTokens: (prev?.promptTokens ?? 0) + usage.input_tokens,
		completionTokens: (prev?.completionTokens ?? 0) + usage.output_tokens,
		totalTokens: (prev?.totalTokens ?? 0) + usage.input_tokens + usage.output_tokens,
		cachedTokens: prev?.cachedTokens,
		cacheCreationTokens: prev?.cacheCreationTokens,
		costUsd: prev?.costUsd,
	}));
}

// ---------------------------------------------------------------------------
// Conversation control
// ---------------------------------------------------------------------------

async function sendMessage(text: string, sessionId: string | null): Promise<void> {
	const s = activeConversation();
	if (s.isStreaming()) return;
	if (!sessionId) {
		s.setError("No terminal attached — focus a terminal first");
		return;
	}

	const capturedKey = activeKey();
	addUserMessage(text);
	batch(() => {
		s.setError(null);
		s.setIsStreaming(true);
		s.setStreamingText("");
		s.setReasoningChunks("");
	});
	s.currentMode = "assisted";
	s.activeSessionId = sessionId;

	try {
		if (isTauri()) {
			const { invoke: coreInvoke, Channel } = await import("@tauri-apps/api/core");
			const onEvent = new Channel<ConversationEvent>();
			onEvent.onmessage = (event) => applyConversationEvent(s, event, capturedKey);
			await coreInvoke("start_conversation", { sessionId, message: text, autonomy: "assisted", onEvent });
		} else {
			// Browser/PWA: dedicated WS carries the token stream (event-bridge plan Step 5).
			closeConversationStream(s); // drop any orphaned prior stream first
			s.conversationStreamDispose = openConversationStream<ConversationEvent>(
				sessionId,
				{ message: text, autonomy: "assisted" },
				(event) => applyConversationEvent(s, event, capturedKey),
				() => onConversationStreamClosed(s, capturedKey),
			);
		}
	} catch (e) {
		batch(() => {
			s.setIsStreaming(false);
			s.setStreamingText("");
			s.setError(String(e));
		});
		appLogger.warn("conversation", "start_conversation (assisted) failed", { error: String(e) });
	}
}

async function cancelStream(): Promise<void> {
	const s = activeConversation();
	if (!s.isStreaming()) return;
	if (!s.activeSessionId) return;
	try {
		// Wrapper invoke: Tauri IPC on desktop, HTTP (COMMAND_TABLE) in browser.
		// The conversation's terminal event then arrives over the active stream.
		await invoke("cancel_conversation", { sessionId: s.activeSessionId });
	} catch (e) {
		appLogger.warn("conversation", "cancel_conversation failed", { error: String(e) });
	}
}

async function startAgent(sessionId: string, goal: string, isUnrestricted?: boolean): Promise<void> {
	const s = activeConversation();
	const capturedKey = activeKey();
	if (s.agentState() === "running" || s.agentState() === "paused") return;

	batch(() => {
		s.setAgentState("running");
		s.setToolCalls([]);
		s.setTextChunks("");
		s.setReasoningChunks("");
		s.setAgentError(null);
		s.setCompletionReason(null);
		s.setCurrentIteration(0);
		s.setPendingApproval(null);
		s.setIsThinking(false);
	});
	s.currentMode = "autonomous";
	s.activeSessionId = sessionId;

	const bypassed = isUnrestricted ? ["*"] : [];

	try {
		if (isTauri()) {
			const { invoke: coreInvoke, Channel } = await import("@tauri-apps/api/core");
			const onEvent = new Channel<ConversationEvent>();
			onEvent.onmessage = (event) => applyConversationEvent(s, event, capturedKey);
			await coreInvoke("start_conversation", {
				sessionId,
				message: goal,
				autonomy: "autonomous",
				bypassedTools: bypassed,
				onEvent,
			});
		} else {
			// Browser/PWA: dedicated WS carries the agent token stream (event-bridge plan Step 5).
			closeConversationStream(s); // drop any orphaned prior stream first
			s.conversationStreamDispose = openConversationStream<ConversationEvent>(
				sessionId,
				{ message: goal, autonomy: "autonomous", bypassedTools: bypassed },
				(event) => applyConversationEvent(s, event, capturedKey),
				() => onConversationStreamClosed(s, capturedKey),
			);
		}
	} catch (e) {
		batch(() => {
			s.setAgentState("error");
			s.setAgentError(String(e));
		});
		appLogger.warn("conversation", "start_conversation (autonomous) failed", { error: String(e) });
	}
}

/** Handle an unexpected browser WS drop: clear our disposer handle, and if the
 * stream was still active (no Completed/Error frame arrived), surface a synthetic
 * error so the UI doesn't wedge in a perpetual "streaming"/"running" state. A
 * close right after a terminal frame is expected — `isStreaming`/`agentState`
 * are already settled, so we stay quiet. */
function onConversationStreamClosed(s: PerTerminalConversationState, ownerKey: string): void {
	s.conversationStreamDispose = null;
	if (s.isStreaming() || s.agentState() === "running") {
		applyConversationEvent(s, { type: "error", message: "Stream connection lost" }, ownerKey);
	}
}

async function cancelAgent(sessionId: string): Promise<void> {
	const s = activeConversation();
	try {
		await invoke("cancel_conversation", { sessionId });
		s.setAgentState("cancelled");
	} catch (e) {
		s.setAgentState("error");
		s.setAgentError(String(e));
		appLogger.warn("conversation", "cancel_conversation failed", { error: String(e) });
	}
}

async function pauseAgent(sessionId: string): Promise<void> {
	const s = activeConversation();
	try {
		await invoke("pause_conversation", { sessionId });
		s.setAgentState("paused");
	} catch (e) {
		s.setAgentState("error");
		s.setAgentError(String(e));
		appLogger.warn("conversation", "pause_conversation failed", { error: String(e) });
	}
}

async function resumeAgent(sessionId: string): Promise<void> {
	const s = activeConversation();
	try {
		await invoke("resume_conversation", { sessionId });
		s.setAgentState("running");
	} catch (e) {
		s.setAgentState("error");
		s.setAgentError(String(e));
		appLogger.warn("conversation", "resume_conversation failed", { error: String(e) });
	}
}

async function approveAction(sessionId: string, approved: boolean): Promise<void> {
	const s = activeConversation();
	try {
		await invoke("approve_conversation_action", { sessionId, approved });
		s.setPendingApproval(null);
	} catch (e) {
		s.setAgentState("error");
		s.setAgentError(String(e));
		appLogger.warn("conversation", "approve_conversation_action failed", { error: String(e) });
	}
}

function resetAgent(): void {
	const s = activeConversation();
	batch(() => {
		s.setAgentState("idle");
		s.setToolCalls([]);
		s.setTextChunks("");
		s.setReasoningChunks("");
		s.setAgentError(null);
		s.setCompletionReason(null);
		s.setCurrentIteration(0);
		s.setPendingApproval(null);
		s.setIsThinking(false);
	});
}

// ---------------------------------------------------------------------------
// Legacy event processing (backward compat with old agent-loop-event — removed in 1617)
// ---------------------------------------------------------------------------

function isLegacyAgentEvent(v: unknown): v is LegacyAgentEvent {
	return typeof v === "object" && v !== null && "type" in v && typeof (v as { type: unknown }).type === "string";
}

function processEvent(raw: unknown): void {
	if (!isLegacyAgentEvent(raw)) return;
	const s = activeConversation();
	const event = raw;
	switch (event.type) {
		case "started":
			s.setAgentState("running");
			break;
		case "thinking":
			s.setCurrentIteration(event.iteration);
			break;
		case "text_chunk":
			s.setTextChunks((prev) => prev + event.text);
			break;
		case "reasoning_chunk":
			s.setReasoningChunks((prev) => prev + event.text);
			break;
		case "tool_call": {
			const entry: ToolCallEntry = {
				status: "pending",
				toolName: event.tool_name,
				args: event.args,
				startedAt: Date.now(),
			};
			s.setToolCalls((prev) => trimToolCalls([...prev, entry]));
			break;
		}
		case "tool_result":
			s.setToolCalls((prev) => {
				const updated = [...prev];
				for (let i = updated.length - 1; i >= 0; i--) {
					if (updated[i].toolName === event.tool_name && updated[i].status === "pending") {
						updated[i] = {
							...updated[i],
							status: "done",
							result: { success: event.success, output: event.output },
							duration: Date.now() - updated[i].startedAt,
						};
						break;
					}
				}
				// The output only arrives here, so this is where the log grows.
				return trimToolCalls(updated);
			});
			break;
		case "needs_approval":
			s.setPendingApproval({ sessionId: event.session_id, command: event.command, reason: event.reason });
			break;
		case "paused":
			s.setAgentState("paused");
			break;
		case "resumed":
			s.setAgentState("running");
			break;
		case "rate_limited":
			appLogger.info("conversation", `Rate limited, waiting ${event.wait_ms}ms`);
			break;
		case "error":
			batch(() => {
				s.setAgentState("error");
				s.setAgentError(event.message);
			});
			break;
		case "completed":
			batch(() => {
				s.setAgentState("completed");
				s.setCompletionReason(event.reason);
			});
			break;
	}
}

// ---------------------------------------------------------------------------
// Terminal lifecycle
// ---------------------------------------------------------------------------

/**
 * Drop a terminal's cached "already read from disk" mark so the next switch to
 * it re-reads the conversation. Needed when another window owned that
 * conversation and changed it: `initFromDisk` skips a state it has already
 * initialized, so without this the terminal keeps showing what it had before.
 */
function invalidateTerminal(key: string): void {
	const s = stateMap.get(key);
	if (s) s.initialized = false;
}

async function onTerminalClose(key: string): Promise<void> {
	const s = stateMap.get(key);
	if (!s) return;

	if (s.persistTimer) {
		clearTimeout(s.persistTimer);
		s.persistTimer = null;
	}

	// Browser/PWA: the terminal is gone, so close the token-stream WS rather than
	// leak it waiting for a terminal frame that no one will consume.
	closeConversationStream(s);

	if ((s.isStreaming() || s.agentState() === "running") && s.activeSessionId) {
		try {
			await invoke("cancel_conversation", { sessionId: s.activeSessionId });
		} catch (e) {
			appLogger.warn("conversation", "onTerminalClose: cancel_conversation failed", { error: String(e) });
		}
	}

	if (hasPersistableState(s)) {
		await persistNow(key);
	}

	stateMap.delete(key);
}

// ---------------------------------------------------------------------------
// Chat ID helpers
// ---------------------------------------------------------------------------

function resetChatId(): void {
	activeConversation().setChatId(generateChatId());
}

function setChatId(id: string): void {
	activeConversation().setChatId(id);
}

function setError(e: string | null): void {
	activeConversation().setError(e);
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

async function listAllConversations(): Promise<ConversationMeta[]> {
	try {
		return await invoke<BackendConversationMeta[]>("list_conversations");
	} catch (e) {
		appLogger.warn("conversation", "listAllConversations failed", { error: String(e) });
		return [];
	}
}

async function loadConversation(id: string): Promise<void> {
	const s = activeConversation();
	// What the conversation held when the read started. A read only speaks for a
	// conversation that has not moved on since: a detached window hydrates on
	// mount without waiting for the disk, so a send can overtake the read, and
	// applying it afterwards would erase the user's turn and drop the streaming
	// flag — leaving the reply to land on a history that never asked anything.
	const before = s.messages();
	try {
		const conv = await invoke<BackendConversation>("load_conversation", { id });
		if (s.messages() !== before) {
			appLogger.info("conversation", "loadConversation: dropped a read the conversation outran", { id });
			return;
		}
		applyLoadedConversation(s, conv);
	} catch (e) {
		appLogger.warn("conversation", "loadConversation failed", { id, error: String(e) });
	}
}

// ---------------------------------------------------------------------------
// Streaming helpers (backward compat shims for tests)
// ---------------------------------------------------------------------------

function setStreaming(v: boolean): void {
	activeConversation().setIsStreaming(v);
}
function appendStreamChunk(text: string): void {
	activeConversation().setStreamingText((prev) => prev + text);
}
function finalizeStream(fullText: string): void {
	const s = activeConversation();
	batch(() => {
		s.setIsStreaming(false);
		s.setStreamingText("");
		addAssistantMessage(fullText);
	});
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

export const conversationStore = {
	// Per-terminal API
	activeConversation,
	getOrCreate,
	setActiveTerminal,
	invalidateTerminal,
	onTerminalClose,

	// Reactive getters (proxy through activeConversation)
	messages,
	isStreaming,
	streamingText,
	error,
	chatId,
	sessionUsage,
	agentState,
	currentIteration,
	toolCalls,
	textChunks,
	reasoningChunks,
	pendingApproval,
	agentError,
	completionReason,
	unrestricted,
	setUnrestricted,
	isThinking,

	// Chat actions
	addUserMessage,
	addAssistantMessage,
	addSystemMessage,
	accumulateUsage,
	clearHistory,
	setStreaming,
	appendStreamChunk,
	finalizeStream,
	sendMessage,
	cancelStream,
	setError,
	resetChatId,
	setChatId,

	// Agent actions
	startAgent,
	cancelAgent,
	pauseAgent,
	resumeAgent,
	approveAction,
	reset: resetAgent,

	// Legacy event processing (removed in 1617)
	processEvent,

	// Persistence
	initFromDisk,
	persistNow,

	// History
	listAllConversations,
	loadConversation,
};
