import { batch, type Component, createEffect, onMount, untrack } from "solid-js";
import { AIChatPanel, type AIChatTerminalBinding } from "../components/AIChatPanel/AIChatPanel";
import { initPanelWindow } from "../hooks/initPanelWindow";
import type { PanelAdapter } from "../panelRouter";
import { conversationStore } from "../stores/conversationStore";
import { terminalsStore } from "../stores/terminals";
import { uiStore } from "../stores/ui";
import {
	AI_CHAT_SYNC_INTERVAL_MS,
	type AiChatSnapshot,
	buildAiChatSnapshot,
	projectAiChat,
} from "../utils/aiChatSnapshot";
import { createPanelSyncReceiver } from "../utils/panelSync";

/**
 * Cap on the mirrored list, mirroring `MAX_MESSAGES` in conversationStore. A
 * finished reply is appended with the raw setter rather than `addAssistantMessage`
 * on purpose — that one schedules a persist, and this window never saw the prompt
 * that produced the reply (the projection carries the stream, not the history), so
 * writing its shorter list back under the same chat id would delete that prompt
 * from disk. The raw setter skips the store's own cap, hence this one.
 */
const MIRRORED_MESSAGE_CAP = 100;

const DetachedAIChatPanel: Component<{ params: URLSearchParams }> = (props) => {
	const chatId = props.params.get("chatId");
	const terminalKey = props.params.get("terminalKey") || null;
	const sessionId = props.params.get("sessionId") || null;
	const terminalName = props.params.get("terminalName") || null;

	// Adopt the terminal BEFORE the chat id. conversationStore keys its state per
	// terminal, so switching terminal afterwards swaps in a fresh empty state and
	// drops both the id and anything loaded into it. It also decides what
	// `persistNow` writes as the conversation's `session_id`: under the default
	// key it writes null, which orphans the conversation from the terminal that
	// owns it, and `initFromDisk` then never finds it again.
	if (terminalKey) conversationStore.setActiveTerminal(terminalKey);

	// `loadConversation` derives the id from the file it reads, so a conversation
	// that has never been saved would leave this window on the id its own store
	// generated — and every message sent from here would land somewhere the main
	// window never opens.
	if (chatId) conversationStore.setChatId(chatId);

	// This window has no terminalsStore of its own, so the terminal it talks to
	// is the one it was opened with. It stays pinned to that terminal for its
	// whole life: following the main window's focus instead would swap the
	// conversation out from under whoever detached it deliberately.
	const binding = (): AIChatTerminalBinding => ({
		sessionId,
		name: terminalName,
		attached: sessionId !== null,
	});

	// Streams the MAIN window runs for this terminal — a watcher rule, an
	// automation goal, a terminal context action — render nowhere while the panel
	// is detached, because `PanelOrchestrator` unmounts the docked copy. Mirror
	// them here. `projectAiChat` decides what a snapshot is allowed to touch; this
	// only carries out the verdict.
	const { state: projection } = createPanelSyncReceiver<AiChatSnapshot | null>("ai-chat");
	let mirroring = false;
	let mirroredFromAt: number | null = null;
	createEffect(() => {
		const snapshot = projection();
		// Track the snapshot and nothing else: the local reads below are this
		// effect's own writes on the previous tick, so tracking them would make it
		// re-run itself forever.
		untrack(() => {
			const s = conversationStore.activeConversation();
			const {
				overlay,
				finalize,
				mirroring: next,
				mirroredFromAt: nextFrom,
			} = projectAiChat(snapshot ?? null, {
				chatId: s.chatId(),
				isStreaming: s.isStreaming(),
				agentState: s.agentState(),
				mirroring,
				mirroredFromAt,
			});
			mirroring = next;
			mirroredFromAt = nextFrom;
			if (!overlay && !finalize) return;
			batch(() => {
				if (overlay) {
					s.setIsStreaming(overlay.isStreaming);
					s.setStreamingText(overlay.streamingText);
					s.setIsThinking(overlay.isThinking);
					s.setError(overlay.error);
					s.setAgentState(overlay.agentState);
					s.setTextChunks(overlay.textChunks);
					s.setToolCalls(overlay.toolCalls);
				}
				if (finalize) {
					s.setMessages((prev) => {
						const next = [...prev, { role: "assistant" as const, content: finalize, timestamp: Date.now() }];
						return next.length > MIRRORED_MESSAGE_CAP ? next.slice(next.length - MIRRORED_MESSAGE_CAP) : next;
					});
				}
			});
		});
	});

	onMount(() => {
		void initPanelWindow();
		// Best effort: a missing conversation is swallowed inside the store, so
		// an unknown id just opens an empty chat under that id.
		if (chatId) void conversationStore.loadConversation(chatId);
	});

	return <AIChatPanel visible={true} onClose={() => window.close()} terminal={binding} />;
};

/** conversationStore's key for the terminal focused right now, null when none is. */
function activeTerminalKey(): string | null {
	const activeId = terminalsStore.state.activeId;
	if (!activeId) return null;
	return terminalsStore.get(activeId)?.tuicSession ?? activeId;
}

/**
 * Terminal the detached window was handed, remembered so the main window knows
 * whose conversation came back. Set on every open of that window — `detachPanel`
 * and the restore after a restart both go through `detachParams`.
 */
let handedOverKey: string | null = null;

export const aiChatPanelAdapter: PanelAdapter = {
	id: "ai-chat",
	title: "AI Chat",
	defaultSize: { width: 500, height: 700 },
	toggle: () => uiStore.toggleAiChatPanel(),
	// `bringPanelHome` toggles the panel back on, so detaching has to turn it off
	// — the same contract the activity adapter follows. Left visible, the toggle
	// on the way home flipped it off instead and the panel never came back.
	onDetach: () => uiStore.setAiChatPanelVisible(false),
	detachParams: () => {
		const activeId = terminalsStore.state.activeId;
		const terminal = activeId ? terminalsStore.get(activeId) : undefined;
		handedOverKey = activeId ? (terminal?.tuicSession ?? activeId) : null;
		// Empty string, never "null"/"undefined": URLSearchParams would hand those
		// back as a perfectly valid session id.
		return {
			chatId: conversationStore.chatId(),
			terminalKey: handedOverKey ?? "",
			sessionId: terminal?.sessionId ?? "",
			terminalName: terminal?.name ?? "",
		};
	},
	// Scoped to the terminal this window was handed, not to whatever the main
	// window has focused now — the detached chat stays pinned to its terminal, so
	// a stream belonging to another one belongs on another screen. `projectAiChat`
	// re-checks that by chat id at the receiving end.
	syncIntervalMs: AI_CHAT_SYNC_INTERVAL_MS,
	serialize: () => (handedOverKey ? buildAiChatSnapshot(conversationStore.getOrCreate(handedOverKey)) : null),
	// Everything typed in the detached window was persisted by ITS store, not
	// this one — the main window's copy of that conversation is frozen at the
	// moment it detached, so coming home has to re-read it.
	//
	// Which conversation, though, depends on where the main window is now. Its
	// `chatId()` follows the FOCUSED terminal, so re-reading that after the user
	// switched terminals would refresh the wrong one and leave the detached
	// terminal stale for good — `initFromDisk` skips a state it has already
	// initialized, so switching back would still show the pre-detach messages.
	//
	// DEFERRED (2026-08-18) — the detached store saves on a 500ms debounce
	// (PERSIST_DEBOUNCE_MS), so closing that window inside 500ms of the last
	// message races this read and the message is lost. Flushing needs a save
	// that survives webview teardown; a `pagehide` handler cannot await the
	// invoke. Left alone: 500ms is far below the time a human takes to read a
	// reply and reach for the close button.
	onReattach: () => {
		const key = handedOverKey;
		handedOverKey = null;
		if (key && key !== activeTerminalKey()) {
			conversationStore.invalidateTerminal(key);
			return;
		}
		// A reply still arriving lives only in memory. `loadConversation` replaces
		// the whole state, so re-reading disk on top of it blanks `streamingText`
		// and `isStreaming` while the backend keeps writing into them — the panel
		// then shows nothing until the stream ends. Mark it stale instead: the next
		// switch back to this terminal re-reads it, same as the branch above.
		if (conversationStore.isStreaming()) {
			if (key) conversationStore.invalidateTerminal(key);
			return;
		}
		void conversationStore.loadConversation(conversationStore.chatId());
	},
	Component: DetachedAIChatPanel,
};
