import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ToolCallEntry } from "../../stores/conversationStore";

const {
	mockSubscribe,
	mockUnsubscribe,
	mockChatId,
	mockDetachPanel,
	mockReattachPanel,
	mockClosePanel,
	mockReasoningChunks,
	mockIsThinking,
	mockMessages,
	mockSendMessage,
	mockToolCalls,
	mockWriteClipboard,
} = vi.hoisted(() => ({
	mockSubscribe: vi.fn().mockResolvedValue(undefined),
	mockUnsubscribe: vi.fn().mockResolvedValue(undefined),
	mockChatId: vi.fn(() => "chat-abc123"),
	mockDetachPanel: vi.fn().mockResolvedValue(undefined),
	mockReattachPanel: vi.fn().mockResolvedValue(undefined),
	mockClosePanel: vi.fn().mockResolvedValue(undefined),
	mockReasoningChunks: vi.fn(() => ""),
	mockIsThinking: vi.fn(() => false),
	mockMessages: vi.fn(() => [] as Array<{ role: string; content: string }>),
	mockSendMessage: vi.fn(),
	mockToolCalls: vi.fn(() => [] as ToolCallEntry[]),
	mockWriteClipboard: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../utils/clipboard", () => ({ writeClipboard: mockWriteClipboard }));

vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
	Channel: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../panelRouter", () => ({
	detachPanel: mockDetachPanel,
	reattachPanel: mockReattachPanel,
	closePanel: mockClosePanel,
}));

vi.mock("../../stores/conversationStore", () => ({
	conversationStore: {
		messages: mockMessages,
		isStreaming: () => false,
		streamingText: () => "",
		error: () => null,
		chatId: mockChatId,
		sessionUsage: () => null,
		sendMessage: mockSendMessage,
		cancelStream: vi.fn(),
		clearHistory: vi.fn(),
		subscribeToRegistry: mockSubscribe,
		unsubscribeFromRegistry: mockUnsubscribe,
		listAllConversations: vi.fn().mockResolvedValue([]),
		loadConversation: vi.fn(),
		resetChatId: vi.fn(),
		agentState: () => "idle",
		textChunks: () => null,
		unrestricted: () => false,
		setUnrestricted: vi.fn(),
		startAgent: vi.fn(),
		pauseAgent: vi.fn(),
		resumeAgent: vi.fn(),
		cancelAgent: vi.fn(),
		pendingApproval: () => null,
		approveAction: vi.fn(),
		currentIteration: () => 0,
		reset: vi.fn(),
		reasoningChunks: mockReasoningChunks,
		isThinking: mockIsThinking,
		toolCalls: mockToolCalls,
	},
}));

// `activeId` is mutable so a test can reproduce a DETACHED window, where this
// store is never hydrated and therefore holds no active terminal at all.
const terminalsState = vi.hoisted(() => ({ activeId: "t1" as string | undefined, terminals: {} }));

vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		state: terminalsState,
		getIds: () => (terminalsState.activeId ? ["t1"] : []),
		get: () =>
			terminalsState.activeId
				? { sessionId: "sess-1", tuicSession: "sess-1", name: "Terminal 1", ref: null }
				: undefined,
	},
}));

vi.mock("../../stores/appLogger", () => ({
	appLogger: {
		info: vi.fn(),
		warn: vi.fn(),
		error: vi.fn(),
		debug: vi.fn(),
	},
}));

vi.mock("../../utils/sendCommand", () => ({
	sendCommand: vi.fn(),
	getShellFamily: vi.fn(() => "posix"),
}));

vi.mock("../../stores/ui", () => ({
	uiStore: {
		state: { detachedPanels: {} },
		isDetached: vi.fn(() => false),
		setDetached: vi.fn(),
		clearDetached: vi.fn(),
	},
}));

vi.mock("../../transport", () => ({
	isTauri: () => true,
}));

vi.mock("../../components/ui/ContentRenderer", () => ({
	ContentRenderer: (props: { content: string }) => <div>{props.content}</div>,
}));

import { AIChatPanel, copyToClipboard } from "../../components/AIChatPanel/AIChatPanel";
import { appLogger } from "../../stores/appLogger";

describe("AIChatPanel lifecycle", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockMessages.mockReturnValue([]);
		mockReasoningChunks.mockReturnValue("");
		mockIsThinking.mockReturnValue(false);
	});

	afterEach(() => {
		cleanup();
	});

	// The Rust `ChatRegistry` has no producer — nothing calls `fan_out` or any
	// `ConversationState` setter — so `chat_subscribe` answered with an empty
	// default snapshot and then went silent. Applying that snapshot ran
	// `setMessages([])`, one IPC hop after `loadConversation` had filled them, so
	// opening a conversation from history blanked it. Subscribing again requires a
	// producer first.
	it("does not subscribe to the producerless chat registry", async () => {
		const { container, unmount } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);
		// Wait for the panel to be mounted and its effects flushed before asserting
		// a negative — otherwise the assertion passes before anything could run.
		await vi.waitFor(() => {
			expect(container.querySelector('button[title="Open in separate window"]')).not.toBeNull();
		});

		expect(mockSubscribe).not.toHaveBeenCalled();
		unmount();
		expect(mockUnsubscribe).not.toHaveBeenCalled();
	});

	it("renders detach button in main window mode", () => {
		const { container } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);
		const detachBtn = container.querySelector('button[title="Open in separate window"]');
		expect(detachBtn).not.toBeNull();
	});

	it("detach button calls detachPanel", () => {
		const { container } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);
		const detachBtn = container.querySelector('button[title="Open in separate window"]') as HTMLButtonElement;
		detachBtn.click();
		expect(mockDetachPanel).toHaveBeenCalledWith("ai-chat");
	});
});

describe("AIChatPanel extended-thinking disclosure", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		// Reasoning only streams after a user turn exists, so keep a user message present.
		mockMessages.mockReturnValue([{ role: "user", content: "hi" }]);
		mockReasoningChunks.mockReturnValue("");
		mockIsThinking.mockReturnValue(false);
	});

	afterEach(() => {
		cleanup();
	});

	it("does not render the disclosure when there is no reasoning", () => {
		mockReasoningChunks.mockReturnValue("");
		const { container } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);
		expect(container.querySelector("details")).toBeNull();
	});

	it("renders the Thinking disclosure when reasoning is present", async () => {
		mockReasoningChunks.mockReturnValue("planning the steps");
		const { container } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);
		const details = container.querySelector("details");
		expect(details).not.toBeNull();
		expect(details?.querySelector("summary")?.textContent).toBe("Thinking");
		await vi.waitFor(() => expect(details?.textContent).toContain("planning the steps"));
	});

	it("auto-opens the disclosure while the model is thinking", () => {
		mockReasoningChunks.mockReturnValue("still reasoning");
		mockIsThinking.mockReturnValue(true);
		const { container } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);
		expect(container.querySelector("details")?.hasAttribute("open")).toBe(true);
	});

	it("collapses the disclosure once thinking has finished", () => {
		mockReasoningChunks.mockReturnValue("done reasoning");
		mockIsThinking.mockReturnValue(false);
		const { container } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);
		expect(container.querySelector("details")?.hasAttribute("open")).toBe(false);
	});
});

// A detached panel window is a separate WebView: `terminalsStore` is never
// hydrated there (App returns at renderPanelMode before any main-window
// effect), so deriving the terminal from that store left the detached chat
// permanently read-only — it could show a conversation but never add to it.
// The window is therefore handed its terminal binding explicitly, and that
// binding is what every send, agent control and session lookup must use.
describe("AIChatPanel terminal binding", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockMessages.mockReturnValue([]);
		terminalsState.activeId = "t1";
	});

	afterEach(() => {
		cleanup();
		terminalsState.activeId = "t1";
	});

	const typeAndSend = (container: HTMLElement, text: string) => {
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = text;
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		textarea.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
		return textarea;
	};

	it("sends with the handed-over session when the store has no terminals", () => {
		terminalsState.activeId = undefined; // the detached window
		const { container } = render(() => (
			<AIChatPanel
				visible={true}
				onClose={() => {}}
				terminal={() => ({ sessionId: "sess-detached", name: "Terminal 7", attached: true })}
			/>
		));

		const textarea = typeAndSend(container, "hello from the detached window");

		expect(textarea.disabled).toBe(false);
		expect(container.textContent).not.toContain("No terminal focused");
		expect(mockSendMessage).toHaveBeenCalledWith("hello from the detached window", "sess-detached");
	});

	it("names the handed-over terminal in the header", () => {
		terminalsState.activeId = undefined;
		const { container } = render(() => (
			<AIChatPanel
				visible={true}
				onClose={() => {}}
				terminal={() => ({ sessionId: "sess-detached", name: "Terminal 7", attached: true })}
			/>
		));

		expect(container.textContent).toContain("Terminal 7");
	});

	// Detaching while no terminal is focused hands over nothing to send to. The
	// panel must stay read-only rather than send into a null session.
	it("stays read-only when the handed-over binding has no terminal", () => {
		terminalsState.activeId = undefined;
		const { container } = render(() => (
			<AIChatPanel
				visible={true}
				onClose={() => {}}
				terminal={() => ({ sessionId: null, name: null, attached: false })}
			/>
		));

		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		expect(textarea.disabled).toBe(true);
		expect(container.textContent).toContain("No terminal focused");
	});

	// The main window passes no binding and must keep deriving it from the
	// store exactly as before.
	it("falls back to the terminals store when no binding is handed over", () => {
		const { container } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);

		typeAndSend(container, "hello from the main window");

		expect(mockSendMessage).toHaveBeenCalledWith("hello from the main window", "sess-1");
	});
});

describe("copyToClipboard", () => {
	beforeEach(() => {
		mockWriteClipboard.mockReset();
	});

	it("returns true when the clipboard write succeeds", async () => {
		mockWriteClipboard.mockResolvedValue(undefined);
		await expect(copyToClipboard("hello")).resolves.toBe(true);
	});

	it("returns false when the clipboard write is denied", async () => {
		mockWriteClipboard.mockRejectedValue(new DOMException("denied", "NotAllowedError"));
		await expect(copyToClipboard("hello")).resolves.toBe(false);
	});
});

describe("AIChatPanel tool call result copy", () => {
	const doneEntry: ToolCallEntry = {
		status: "done",
		toolName: "bash",
		args: {},
		startedAt: 0,
		result: { success: true, output: "tool output text" },
		duration: 42,
	};

	beforeEach(() => {
		vi.clearAllMocks();
		// The message list's entire body (including tool call cards) sits behind a
		// `messages().length > 0 || isStreaming()` gate, with an empty-state fallback.
		mockMessages.mockReturnValue([{ role: "assistant", content: "ran a tool" }]);
		mockReasoningChunks.mockReturnValue("");
		mockIsThinking.mockReturnValue(false);
		mockToolCalls.mockReturnValue([doneEntry]);
		mockWriteClipboard.mockReset();
	});

	afterEach(() => {
		cleanup();
	});

	function expandAndGetCopyButton(container: HTMLElement): HTMLButtonElement {
		const header = container.querySelector('[role="button"]') as HTMLElement;
		header.click();
		return container.querySelector('button[title="Copy result to clipboard"]') as HTMLButtonElement;
	}

	it("shows the copied state only once the clipboard write resolves", async () => {
		vi.useFakeTimers();
		try {
			mockWriteClipboard.mockResolvedValue(undefined);
			const { container } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);

			const copyBtn = expandAndGetCopyButton(container);
			copyBtn.click();

			await vi.waitFor(() => expect(copyBtn.textContent).toBe("copied"), { timeout: 5000 });
			expect(appLogger.error).not.toHaveBeenCalled();

			// The "copied" state reverts after 1.5s — flush it so no timer leaks past this test.
			await vi.advanceTimersByTimeAsync(1500);
		} finally {
			vi.useRealTimers();
		}
	});

	it("does not show the copied state and logs when the clipboard write is denied", async () => {
		const err = new DOMException("Write permission denied.", "NotAllowedError");
		mockWriteClipboard.mockRejectedValue(err);
		const { container } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);

		const copyBtn = expandAndGetCopyButton(container);
		copyBtn.click();

		await vi.waitFor(() => {
			expect(appLogger.error).toHaveBeenCalledWith("ai-chat", "Failed to copy tool output", err);
		});
		expect(copyBtn.textContent).toBe("copy");
	});
});
