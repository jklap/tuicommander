import type { AgentState, PerTerminalConversationState, ToolCallEntry } from "../stores/conversationStore";

/**
 * Cross-window projection for the detached AI Chat window.
 *
 * The window it feeds is not a viewer. Unlike the Activity Dashboard, which
 * mirrors state the main window alone owns, the detached chat owns a
 * conversation store of its own and sends against the terminal it was handed —
 * so one screen has two writers, and the projection has to lose every argument
 * with the local one.
 *
 * Two properties keep that safe:
 *
 * 1. **It is an overlay, never a replacement.** The snapshot carries the live
 *    stream and nothing else — no message history. So the worst a stale snapshot
 *    can say is "nothing is streaming", which writes nothing at all. A
 *    projection that replaced state would instead wipe whatever the user typed
 *    in the detached window, which is the same failure as re-reading disk over a
 *    live stream.
 * 2. **`mirroring` decides ownership, not `isStreaming`.** Painting a mirrored
 *    stream sets `isStreaming` locally, so the local flag alone cannot tell "the
 *    user asked for this here" from "we put it here". Without the extra flag the
 *    second tick of every mirrored stream reads as locally owned and is dropped,
 *    and the text freezes one tick in.
 *
 * What it exists to fix: `PanelOrchestrator` unmounts the docked panel while the
 * chat is detached, yet `watcherFire`, `useAutomationEventBridges` and the
 * terminal context menu all keep starting conversations on the MAIN window's
 * store. Those replies rendered nowhere at all.
 */

/** Live stream state of one conversation, as the main window sees it. */
export interface AiChatSnapshot {
	/** Conversation this stream belongs to — the detached window drops anything else. */
	chatId: string;
	isStreaming: boolean;
	streamingText: string;
	isThinking: boolean;
	error: string | null;
	agentState: AgentState;
	textChunks: string;
	toolCalls: ToolCallEntry[];
	/**
	 * The finished reply. The main window clears `streamingText` the instant a
	 * stream completes, and the projection ticks at `AI_CHAT_SYNC_INTERVAL_MS`, so
	 * the last chunk the detached window mirrored is very likely a tick short of
	 * the whole answer. Finalize from this instead.
	 *
	 * Read it for what it is: the last reply in the main window's HISTORY, not the
	 * reply this stream produced. A stream that ends without producing one — a
	 * cancel, an error — leaves it pointing at the PREVIOUS reply, which is why
	 * finalizing needs `lastAssistantAt` to tell the two apart.
	 */
	lastAssistantText: string | null;
	/**
	 * `timestamp` of that message. Identity for "did this stream actually produce
	 * a reply", compared against the value captured when mirroring began.
	 *
	 * A timestamp rather than the text (two short replies can legitimately read
	 * the same — "Done." from a watcher rule — and suppressing the second would be
	 * a silent loss) and rather than a message count (the main window caps its
	 * history at MAX_MESSAGES, so at the cap the count stops growing and no reply
	 * would ever finalize again).
	 *
	 * Bounded by `Date.now()` resolution: two assistant messages in the same
	 * millisecond would read as one. Considered and accepted — replies arrive
	 * seconds apart, and each is preceded by a round trip to a model.
	 */
	lastAssistantAt: number | null;
}

/** What the detached window's own store looks like right now. */
export interface AiChatProjectionLocal {
	chatId: string;
	isStreaming: boolean;
	agentState: AgentState;
	/** True when the previous tick of this stream was painted here by the projection. */
	mirroring: boolean;
	/**
	 * `lastAssistantAt` as it stood when the stream now being mirrored started.
	 * The stream produced a reply only if it has moved since. Baselining at the
	 * START is what makes one rule cover both duplicate paths: a second stream
	 * that ends without a reply, and a window that mounted on a history already
	 * ending in one. Meaningless while `mirroring` is false.
	 */
	mirroredFromAt: number | null;
}

/** The fields a mirrored tick writes into the local store. */
export interface AiChatOverlay {
	isStreaming: boolean;
	streamingText: string;
	isThinking: boolean;
	error: string | null;
	agentState: AgentState;
	textChunks: string;
	toolCalls: ToolCallEntry[];
}

export interface AiChatProjectionResult {
	/** Null means write nothing — an ignored snapshot and an idle one both land here. */
	overlay: AiChatOverlay | null;
	/** Assistant reply to keep on screen because the mirrored stream just ended. */
	finalize: string | null;
	/** `mirroring` for the next tick. */
	mirroring: boolean;
	/** `mirroredFromAt` for the next tick. */
	mirroredFromAt: number | null;
}

/**
 * Push cadence. The docked panel renders its stream at `STREAM_RENDER_MS` (200ms),
 * so pushing faster than this would cost IPC for frames the receiver throttles
 * away anyway, and pushing at the Activity Dashboard's 1Hz would make the same
 * reply visibly laggier in the detached window than in the docked one. An idle
 * chat costs nothing either way: `createPanelSyncProvider` suppresses a snapshot
 * byte-identical to the last one it delivered.
 */
export const AI_CHAT_SYNC_INTERVAL_MS = 250;

const NOTHING: AiChatProjectionResult = { overlay: null, finalize: null, mirroring: false, mirroredFromAt: null };

function isBusy(state: { isStreaming: boolean; agentState: AgentState }): boolean {
	return state.isStreaming || state.agentState === "running" || state.agentState === "paused";
}

/**
 * What the serializer reads. Narrowed to exactly those fields rather than taking
 * the whole `PerTerminalConversationState`, so a test can hand it a real source
 * without a cast — a hand-built snapshot fixture is free to state a combination
 * production never emits, and then asserts the behaviour we meant instead of the
 * one we wrote.
 */
export type AiChatSnapshotSource = Pick<
	PerTerminalConversationState,
	| "messages"
	| "chatId"
	| "isStreaming"
	| "streamingText"
	| "isThinking"
	| "error"
	| "agentState"
	| "textChunks"
	| "toolCalls"
>;

/** Serialize the live stream of one conversation for the detached window. */
export function buildAiChatSnapshot(state: AiChatSnapshotSource): AiChatSnapshot {
	const messages = state.messages();
	// Reverse scan rather than `findLast`, which this project's TS lib target
	// predates — the same walk `applyConversationEvent` uses for tool results.
	let lastAssistantText: string | null = null;
	let lastAssistantAt: number | null = null;
	for (let i = messages.length - 1; i >= 0; i--) {
		if (messages[i].role === "assistant") {
			lastAssistantText = messages[i].content;
			lastAssistantAt = messages[i].timestamp;
			break;
		}
	}
	return {
		chatId: state.chatId(),
		isStreaming: state.isStreaming(),
		streamingText: state.streamingText(),
		isThinking: state.isThinking(),
		error: state.error(),
		agentState: state.agentState(),
		textChunks: state.textChunks(),
		toolCalls: state.toolCalls(),
		lastAssistantText,
		lastAssistantAt,
	};
}

/** Decide what a snapshot may do to the detached window's own conversation. */
export function projectAiChat(snapshot: AiChatSnapshot | null, local: AiChatProjectionLocal): AiChatProjectionResult {
	if (!snapshot) return NOTHING;

	// A stream the user asked for in THIS window is the one thing the projection
	// must never paint over. `mirroring` is what tells that apart from the local
	// flags we set ourselves on the previous tick.
	if (!local.mirroring && isBusy(local)) return NOTHING;

	// This window stays pinned to the terminal it was detached with, so the main
	// window can be streaming into a different terminal's conversation entirely.
	// That reply belongs on another screen.
	if (snapshot.chatId !== local.chatId) return NOTHING;

	const overlay: AiChatOverlay = {
		isStreaming: snapshot.isStreaming,
		streamingText: snapshot.streamingText,
		isThinking: snapshot.isThinking,
		error: snapshot.error,
		agentState: snapshot.agentState,
		textChunks: snapshot.textChunks,
		toolCalls: snapshot.toolCalls,
	};

	// The baseline is taken once, at the START of the stream, and carried forward
	// untouched while it runs — re-reading it every tick would make the reply the
	// stream is about to produce look like one that was already there.
	if (isBusy(snapshot))
		return {
			overlay,
			finalize: null,
			mirroring: true,
			mirroredFromAt: local.mirroring ? local.mirroredFromAt : snapshot.lastAssistantAt,
		};

	// The stream we were mirroring has ended. Land the overlay once so the window
	// stops showing a spinner, and keep the finished reply on screen. An
	// autonomous run needs no finalize: `textChunks` and the tool cards survive
	// completion on their own, unlike `streamingText`.
	//
	// Finalize only a reply this stream actually produced. `lastAssistantText`
	// still points at the PREVIOUS reply when a stream is cancelled or errors, and
	// appending it again would show the same answer twice under an error banner —
	// permanently, since the mirrored reply is never persisted, so nothing on disk
	// ever contradicts it.
	if (local.mirroring) {
		const produced = snapshot.lastAssistantAt !== null && snapshot.lastAssistantAt !== local.mirroredFromAt;
		return { overlay, finalize: produced ? snapshot.lastAssistantText : null, mirroring: false, mirroredFromAt: null };
	}

	// An idle snapshot with nothing being mirrored writes nothing. This is what
	// makes a stale snapshot harmless rather than destructive.
	return NOTHING;
}
