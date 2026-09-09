import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// A detached AI Chat window is a fresh WebView: its conversation store starts
// empty and generates its own chat id. The only thing tying it back to the
// conversation it was detached from is the `chatId` query param, so the mount
// path MUST both adopt that id and read the conversation off disk. Setting the
// id without loading leaves the window blank; loading without setting the id
// first leaves it writing into an id nobody else opens, because
// `loadConversation` takes the id from the file it manages to read and a brand
// new conversation has no file yet.

const h = vi.hoisted(() => ({
	setChatId: vi.fn(),
	setActiveTerminal: vi.fn(),
	invalidateTerminal: vi.fn(),
	loadConversation: vi.fn(),
	chatId: vi.fn(() => "current-chat"),
	isStreaming: vi.fn(() => false),
	initPanelWindow: vi.fn(),
	calls: [] as string[],
	terminal: { activeId: "t1" as string | undefined },
	/** The detached window's OWN conversation state, which the projection writes into. */
	local: null as unknown,
	/** Set by the test file once solid-js is importable; read at render time. */
	projection: (() => null) as () => unknown,
}));

// The detached window subscribes to the projection through this. Stubbing it
// keeps the real `getCurrentWebviewWindow()` out of the test AND hands the test
// the channel the main window would push on.
vi.mock("../../utils/panelSync", () => ({
	createPanelSyncReceiver: () => ({ state: h.projection, emitAction: vi.fn(), destroy: vi.fn() }),
}));

vi.mock("../../stores/conversationStore", () => ({
	conversationStore: {
		setChatId: (id: string) => {
			h.calls.push("setChatId");
			return h.setChatId(id);
		},
		setActiveTerminal: (key: string) => {
			h.calls.push("setActiveTerminal");
			return h.setActiveTerminal(key);
		},
		invalidateTerminal: (key: string) => h.invalidateTerminal(key),
		loadConversation: (id: string) => {
			h.calls.push("loadConversation");
			return h.loadConversation(id);
		},
		chatId: () => h.chatId(),
		isStreaming: () => h.isStreaming(),
		activeConversation: () => h.local,
		getOrCreate: () => h.local,
	},
}));

vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		state: h.terminal,
		get: (id: string) =>
			id
				? { sessionId: `sess-${id.slice(1)}`, tuicSession: `tuic-${id.slice(1)}`, name: `Terminal ${id.slice(1)}` }
				: undefined,
	},
}));

vi.mock("../../hooks/initPanelWindow", () => ({ initPanelWindow: h.initPanelWindow }));

// Render the binding the adapter hands down, so a test can assert on what the
// detached window will actually send with.
vi.mock("../../components/AIChatPanel/AIChatPanel", () => ({
	AIChatPanel: (props: { terminal?: () => { sessionId: string | null; name: string | null; attached: boolean } }) => (
		<div
			data-testid="ai-chat-panel"
			data-session={props.terminal?.().sessionId ?? ""}
			data-name={props.terminal?.().name ?? ""}
			data-attached={String(props.terminal?.().attached ?? false)}
		/>
	),
}));

import { createSignal } from "solid-js";
import { aiChatPanelAdapter } from "../../panelAdapters/aiChat";
import type { AgentState } from "../../stores/conversationStore";
import { uiStore } from "../../stores/ui";
import type { AiChatSnapshot } from "../../utils/aiChatSnapshot";

/** Let the mount's async work settle. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** The detached window's OWN conversation state — what the projection writes into. */
function makeLocalState(chatId = "conv-42") {
	const [messages, setMessages] = createSignal<{ role: string; content: string; timestamp: number }[]>([]);
	const [isStreaming, setIsStreaming] = createSignal(false);
	const [streamingText, setStreamingText] = createSignal("");
	const [isThinking, setIsThinking] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);
	const [agentState, setAgentState] = createSignal<AgentState>("idle");
	const [textChunks, setTextChunks] = createSignal("");
	const [toolCalls, setToolCalls] = createSignal<unknown[]>([]);
	return {
		chatId: () => chatId,
		messages,
		setMessages,
		isStreaming,
		setIsStreaming,
		streamingText,
		setStreamingText,
		isThinking,
		setIsThinking,
		error,
		setError,
		agentState,
		setAgentState,
		textChunks,
		setTextChunks,
		toolCalls,
		setToolCalls,
	};
}
type LocalState = ReturnType<typeof makeLocalState>;

const pushed = (over: Partial<AiChatSnapshot> = {}): AiChatSnapshot => ({
	chatId: "conv-42",
	isStreaming: false,
	streamingText: "",
	isThinking: false,
	error: null,
	agentState: "idle",
	textChunks: "",
	toolCalls: [],
	lastAssistantText: null,
	lastAssistantAt: null,
	...over,
});

const DETACHED_PARAMS = "chatId=conv-42&terminalKey=tuic-1&sessionId=sess-1&terminalName=Term";

describe("detached AI Chat panel adapter", () => {
	let setProjection: (snapshot: AiChatSnapshot | null) => void;
	let localState: LocalState;

	beforeEach(() => {
		const [projection, setter] = createSignal<AiChatSnapshot | null>(null);
		h.projection = projection;
		setProjection = (snapshot) => setter(() => snapshot);
		localState = makeLocalState();
		h.local = localState;

		h.setChatId.mockReset();
		h.setActiveTerminal.mockReset();
		h.invalidateTerminal.mockReset();
		h.loadConversation.mockReset();
		h.loadConversation.mockResolvedValue(undefined);
		h.initPanelWindow.mockReset();
		h.initPanelWindow.mockResolvedValue(undefined);
		h.isStreaming.mockReset();
		h.isStreaming.mockReturnValue(false);
		h.calls.length = 0;
		h.terminal.activeId = "t1";
	});

	afterEach(() => {
		uiStore._testCancelPendingSave();
	});

	it("adopts the chat id and loads that conversation from disk on mount", async () => {
		const { getByTestId } = render(() => (
			<aiChatPanelAdapter.Component params={new URLSearchParams("chatId=conv-42")} />
		));
		await settle();

		expect(getByTestId("ai-chat-panel")).toBeTruthy();
		expect(h.setChatId).toHaveBeenCalledWith("conv-42");
		expect(h.loadConversation).toHaveBeenCalledWith("conv-42");
		// Order matters: the id is authoritative, the disk read is best effort.
		expect(h.calls).toEqual(["setChatId", "loadConversation"]);
	});

	it("opens empty without an error when the id has nothing saved under it", async () => {
		// `loadConversation` already swallows a missing conversation, so the
		// adapter must not add a rejection of its own on top of it.
		h.loadConversation.mockResolvedValue(undefined);

		const { getByTestId } = render(() => (
			<aiChatPanelAdapter.Component params={new URLSearchParams("chatId=never-saved")} />
		));
		await settle();

		expect(getByTestId("ai-chat-panel")).toBeTruthy();
		expect(h.setChatId).toHaveBeenCalledWith("never-saved");
		expect(h.loadConversation).toHaveBeenCalledWith("never-saved");
	});

	it("leaves the store alone when no chat id was passed", async () => {
		render(() => <aiChatPanelAdapter.Component params={new URLSearchParams()} />);
		await settle();

		expect(h.setChatId).not.toHaveBeenCalled();
		expect(h.loadConversation).not.toHaveBeenCalled();
	});

	it("detaches with the same param keys the mount path reads back", () => {
		// The keys the window is opened with and the keys the window reads have
		// to match, or the load silently never happens.
		expect(aiChatPanelAdapter.id).toBe("ai-chat");
		expect(aiChatPanelAdapter.detachParams?.()).toEqual({
			chatId: "current-chat",
			terminalKey: "tuic-1",
			sessionId: "sess-1",
			terminalName: "Terminal 1",
		});
	});

	// Detaching with no terminal focused has no session to hand over. Empty
	// strings, not the words "null"/"undefined", which URLSearchParams would
	// hand back as a perfectly valid session id.
	it("hands over empty terminal params when no terminal is focused", () => {
		h.terminal.activeId = undefined;

		expect(aiChatPanelAdapter.detachParams?.()).toEqual({
			chatId: "current-chat",
			terminalKey: "",
			sessionId: "",
			terminalName: "",
		});
	});

	// conversationStore keys its state per terminal. Adopting the terminal AFTER
	// the chat id would swap in a fresh empty state and drop the id, and every
	// later save would go out under the default key — which persists
	// `session_id: null` and orphans the conversation from its terminal.
	it("adopts the terminal before the chat id", async () => {
		render(() => (
			<aiChatPanelAdapter.Component
				params={new URLSearchParams("chatId=conv-42&terminalKey=tuic-1&sessionId=sess-1&terminalName=Term")}
			/>
		));
		await settle();

		expect(h.setActiveTerminal).toHaveBeenCalledWith("tuic-1");
		expect(h.calls).toEqual(["setActiveTerminal", "setChatId", "loadConversation"]);
	});

	// Without a binding the detached chat is read-only: terminalsStore is never
	// hydrated in a panel window, so the panel cannot find a session on its own.
	it("hands the panel the terminal binding it was opened with", async () => {
		const { getByTestId } = render(() => (
			<aiChatPanelAdapter.Component
				params={new URLSearchParams("chatId=conv-42&terminalKey=tuic-1&sessionId=sess-9&terminalName=Term%207")}
			/>
		));
		await settle();

		const panel = getByTestId("ai-chat-panel");
		expect(panel.getAttribute("data-session")).toBe("sess-9");
		expect(panel.getAttribute("data-name")).toBe("Term 7");
		expect(panel.getAttribute("data-attached")).toBe("true");
	});

	it("hands the panel an unattached binding when it was detached with no terminal", async () => {
		const { getByTestId } = render(() => (
			<aiChatPanelAdapter.Component
				params={new URLSearchParams("chatId=conv-42&terminalKey=&sessionId=&terminalName=")}
			/>
		));
		await settle();

		const panel = getByTestId("ai-chat-panel");
		expect(panel.getAttribute("data-session")).toBe("");
		expect(panel.getAttribute("data-attached")).toBe("false");
		expect(h.setActiveTerminal).not.toHaveBeenCalled();
	});

	// Criterion: messages sent from the detached window must be there when the
	// panel is reopened in the main window. The main window's store is frozen
	// at the instant it detached — only the detached copy persisted anything —
	// so coming home has to re-read the conversation off disk.
	it("re-reads the conversation when the panel comes back to the main window", () => {
		aiChatPanelAdapter.detachParams?.();

		aiChatPanelAdapter.onReattach?.();

		expect(h.loadConversation).toHaveBeenCalledWith("current-chat");
	});

	// Switching terminals in the main window while the chat is detached moves
	// `chatId()` onto the OTHER terminal's conversation. Re-reading that one
	// would leave the detached terminal's cached state stale forever, because
	// `initFromDisk` skips a state it has already initialized — so the user
	// would switch back and see the conversation as it was before detaching.
	it("invalidates the detached terminal instead when the main window moved on", () => {
		aiChatPanelAdapter.detachParams?.(); // detached while "tuic-1" was active
		h.terminal.activeId = "t2";

		aiChatPanelAdapter.onReattach?.();

		expect(h.invalidateTerminal).toHaveBeenCalledWith("tuic-1");
		expect(h.loadConversation).not.toHaveBeenCalled();
	});

	// The main window keeps streaming into its own store while the chat is
	// detached — a watcher rule, an automation goal or a terminal context action
	// can all start one, and `PanelOrchestrator` renders no panel to show it. The
	// reply exists only in memory: `loadConversation` replaces the whole state,
	// blanking `streamingText` and `isStreaming`, so re-reading disk on the way
	// home threw the partial answer away and left the panel dead until the stream
	// finished. Mark the conversation stale instead, and let the next switch back
	// to that terminal re-read it.
	it("keeps a live stream instead of re-reading disk over it", () => {
		aiChatPanelAdapter.detachParams?.();
		h.isStreaming.mockReturnValue(true);

		aiChatPanelAdapter.onReattach?.();

		expect(h.loadConversation).not.toHaveBeenCalled();
		expect(h.invalidateTerminal).toHaveBeenCalledWith("tuic-1");
	});

	// Criterion: streaming output reaches a detached AI Chat window. The reply
	// being mirrored is one the MAIN window is running — `PanelOrchestrator`
	// unmounts the docked panel while detached, so `watcherFire`, the automation
	// bridge and the terminal context menu all streamed into a store with no UI
	// attached to it at either end.
	it("shows a stream the main window is running", async () => {
		render(() => <aiChatPanelAdapter.Component params={new URLSearchParams(DETACHED_PARAMS)} />);
		await settle();

		setProjection(pushed({ isStreaming: true, streamingText: "half a rep" }));
		await settle();

		expect(localState.isStreaming()).toBe(true);
		expect(localState.streamingText()).toBe("half a rep");
	});

	// The main window clears `streamingText` on completion, so the last mirrored
	// chunk is a tick short of the answer. Without the finalize step the reply
	// would stream in and then vanish at the exact moment it finished.
	it("keeps the finished reply on screen when the mirrored stream ends", async () => {
		render(() => <aiChatPanelAdapter.Component params={new URLSearchParams(DETACHED_PARAMS)} />);
		await settle();

		setProjection(pushed({ isStreaming: true, streamingText: "half a rep" }));
		await settle();
		// The timestamp is the reply's identity: it is what proves this stream
		// produced an answer rather than leaving the previous one in place.
		setProjection(pushed({ lastAssistantText: "half a reply, then the rest", lastAssistantAt: 1000 }));
		await settle();

		expect(localState.isStreaming()).toBe(false);
		expect(localState.streamingText()).toBe("");
		expect(localState.messages()).toEqual([
			expect.objectContaining({ role: "assistant", content: "half a reply, then the rest" }),
		]);
	});

	// The dual-writer guard, at the wiring level rather than the reducer's. A
	// reply the user asked for in THIS window must survive a projection tick.
	it("does not paint over a stream the detached window started itself", async () => {
		render(() => <aiChatPanelAdapter.Component params={new URLSearchParams(DETACHED_PARAMS)} />);
		await settle();

		localState.setIsStreaming(true);
		localState.setStreamingText("what the user asked for here");
		setProjection(pushed({ isStreaming: true, streamingText: "from the main window" }));
		await settle();

		expect(localState.streamingText()).toBe("what the user asked for here");
	});

	// This window stays pinned to the terminal it was detached with, so a stream
	// the main window runs for a DIFFERENT terminal belongs on another screen.
	it("ignores a stream belonging to another conversation", async () => {
		render(() => <aiChatPanelAdapter.Component params={new URLSearchParams(DETACHED_PARAMS)} />);
		await settle();

		setProjection(pushed({ chatId: "conv-other", isStreaming: true, streamingText: "not yours" }));
		await settle();

		expect(localState.isStreaming()).toBe(false);
		expect(localState.streamingText()).toBe("");
	});

	// The bridge only builds a provider for an adapter that declares BOTH, which
	// is exactly why this panel got no projection at all before.
	it("declares the projection the detached bridge requires", () => {
		expect(aiChatPanelAdapter.serialize).toBeTypeOf("function");
		expect(aiChatPanelAdapter.syncIntervalMs).toBeGreaterThan(0);
	});

	// Nothing was handed over, so there is no conversation to project and the
	// serializer must not invent one.
	it("serializes nothing when it was detached with no terminal", () => {
		h.terminal.activeId = undefined;
		aiChatPanelAdapter.detachParams?.();

		expect(aiChatPanelAdapter.serialize?.()).toBeNull();
	});

	// `bringPanelHome` toggles the panel to bring it back, which is only correct
	// if detaching turned it off first — the reference adapter (`activity`) does
	// exactly that in `onDetach`. Without it the visible flag was still true while
	// detached, so the toggle on the way home flipped it OFF and the panel never
	// reappeared: whatever the detached window was showing had nowhere to land.
	it("comes back visible after a detach and reattach round trip", () => {
		uiStore.setAiChatPanelVisible(true);

		aiChatPanelAdapter.onDetach?.();
		expect(uiStore.state.aiChatPanelVisible).toBe(false);

		aiChatPanelAdapter.toggle?.(); // what `bringPanelHome` calls
		expect(uiStore.state.aiChatPanelVisible).toBe(true);
	});
});
