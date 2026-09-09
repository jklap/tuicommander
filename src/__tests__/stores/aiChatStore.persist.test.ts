import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Desktop regression coverage: the shared invoke wrapper delegates to Tauri core.
const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
	invoke: mockInvoke,
	Channel: class {
		onmessage: ((_msg: unknown) => void) | null = null;
	},
}));

// Force isTauri() = true so persistence paths execute.
vi.mock("../../transport", () => ({
	isTauri: () => true,
}));

vi.mock("../../stores/appLogger", () => ({
	appLogger: {
		warn: vi.fn(),
		info: vi.fn(),
		debug: vi.fn(),
		error: vi.fn(),
	},
}));

describe("conversationStore persistence (1385-87c6)", () => {
	let store: typeof import("../../stores/conversationStore").conversationStore;

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.resetModules();
		mockInvoke.mockReset();
		globalThis.localStorage?.clear();
		store = (await import("../../stores/conversationStore")).conversationStore;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("initFromDisk assigns a new conversation id when no tuicSession", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "new_conversation_id") return Promise.resolve("fresh-id");
			return Promise.resolve();
		});

		await store.initFromDisk();
		expect(store.chatId()).toBe("fresh-id");
	});

	it("addAssistantMessage triggers a debounced save_conversation invoke", async () => {
		mockInvoke.mockResolvedValue(undefined);
		store.addAssistantMessage("hello world");
		// Before debounce window elapses, no save yet.
		expect(mockInvoke.mock.calls.filter((c) => c[0] === "save_conversation").length).toBe(0);
		await vi.advanceTimersByTimeAsync(600);
		const saves = mockInvoke.mock.calls.filter((c) => c[0] === "save_conversation");
		expect(saves.length).toBe(1);
		const conv = saves[0]?.[1]?.conversation;
		expect(conv.messages.length).toBe(1);
		expect(conv.messages[0].role).toBe("assistant");
		// The schema version is stamped by the backend (705-57fa), not the store.
		expect(conv.schema_version).toBeUndefined();
	});

	it("clearHistory resets streaming state and deletes from disk", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "new_conversation_id") return Promise.resolve("next-id");
			return Promise.resolve();
		});
		store.addUserMessage("question");
		store.appendStreamChunk("partial");
		store.setStreaming(true);
		expect(store.messages().length).toBe(1);
		expect(store.streamingText()).toBe("partial");

		const prevId = store.chatId();
		store.clearHistory();
		expect(store.messages().length).toBe(0);
		expect(store.streamingText()).toBe("");
		expect(store.isStreaming()).toBe(false);

		await vi.runAllTimersAsync();
		const deletes = mockInvoke.mock.calls.filter((c) => c[0] === "delete_conversation");
		expect(deletes.length).toBe(1);
		expect(deletes[0]?.[1]).toEqual({ id: prevId });
		expect(store.chatId()).toBe("next-id");
	});

	it("initFromDisk loads messages with missing content as empty string (1405-3464)", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_conversations") {
				return Promise.resolve([
					{ id: "corrupt-123", title: "t", session_id: "sess-corrupt", created: 1, updated: 2, message_count: 2 },
				]);
			}
			if (cmd === "load_conversation") {
				return Promise.resolve({
					meta: { id: "corrupt-123", title: "t", session_id: "sess-corrupt", created: 1, updated: 2, message_count: 2 },
					messages: [
						{ role: "user", content: "hello", timestamp: 1 },
						{ role: "assistant", timestamp: 2 }, // missing content
					],
					schema_version: 1,
				});
			}
			return Promise.resolve();
		});

		await store.initFromDisk("sess-corrupt");
		expect(store.messages().length).toBe(2);
		expect(store.messages()[1]?.content).toBe("");
	});

	it("round-trip: message with empty content persists without serde error (1405-3464)", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_conversations") {
				return Promise.resolve([
					{ id: "rt-123", title: "t", session_id: "sess-rt", created: 1, updated: 2, message_count: 1 },
				]);
			}
			if (cmd === "load_conversation") {
				return Promise.resolve({
					meta: { id: "rt-123", title: "t", session_id: "sess-rt", created: 1, updated: 2, message_count: 1 },
					messages: [{ role: "user", timestamp: 1 }], // missing content
					schema_version: 1,
				});
			}
			return Promise.resolve();
		});

		await store.initFromDisk("sess-rt");
		// Trigger persist — should call save_conversation with content: ""
		store.addAssistantMessage("reply");
		await vi.advanceTimersByTimeAsync(600);

		const saves = mockInvoke.mock.calls.filter((c) => c[0] === "save_conversation");
		expect(saves.length).toBe(1);
		const msgs = saves[0]?.[1]?.conversation?.messages as Array<{ role: string; content: string }>;
		const userMsg = msgs.find((m) => m.role === "user");
		expect(userMsg?.content).toBe("");
	});
});

describe("conversationStore terminal lifecycle (1410-1be8)", () => {
	let store: typeof import("../../stores/conversationStore").conversationStore;

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.resetModules();
		mockInvoke.mockReset();
		globalThis.localStorage?.clear();
		store = (await import("../../stores/conversationStore")).conversationStore;
		mockInvoke.mockResolvedValue(undefined);
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("onTerminalClose cancels in-flight stream and frees memory", async () => {
		store.setActiveTerminal("T1");
		await store.sendMessage("hello", "sess-T1");

		// T1 is streaming; close it
		await store.onTerminalClose("T1");

		const cancelCalls = mockInvoke.mock.calls.filter((c) => c[0] === "cancel_conversation");
		expect(cancelCalls.length).toBe(1);

		// State should be freed — getOrCreate returns a fresh empty state
		const state = store.getOrCreate("T1");
		expect(state.messages()).toEqual([]);
		expect(state.isStreaming()).toBe(false);
	});

	it("onTerminalClose while idle frees memory without cancel_ai_chat", async () => {
		store.setActiveTerminal("T1");
		store.addUserMessage("hello");

		await store.onTerminalClose("T1");

		const cancelCalls = mockInvoke.mock.calls.filter((c) => c[0] === "cancel_ai_chat");
		expect(cancelCalls.length).toBe(0);

		// Memory freed
		const state = store.getOrCreate("T1");
		expect(state.messages()).toEqual([]);
	});

	it("onTerminalClose persists partial messages before freeing", async () => {
		store.setActiveTerminal("T1");
		store.addUserMessage("partial question");
		store.appendStreamChunk("partial ans");

		await store.onTerminalClose("T1");

		await vi.advanceTimersByTimeAsync(600);
		const saves = mockInvoke.mock.calls.filter((c) => c[0] === "save_conversation");
		expect(saves.length).toBe(1);
	});
});

describe("conversationStore streaming — per-terminal (1408-a8d8)", () => {
	let store: typeof import("../../stores/conversationStore").conversationStore;
	// Capture Channel instances so we can fire callbacks manually
	const channels: Map<string, { onmessage: ((msg: unknown) => void) | null }> = new Map();

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.resetModules();
		mockInvoke.mockReset();
		channels.clear();
		globalThis.localStorage?.clear();

		mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
			if (cmd === "start_conversation") {
				// Capture the channel by the active terminal key (we don't have chatId here, use a counter)
				const ch = args?.["onEvent"] as { onmessage: ((msg: unknown) => void) | null };
				// Use sessionId as the channel key since that's what start_conversation receives
				if (ch && args?.["sessionId"]) channels.set(args["sessionId"] as string, ch);
				return Promise.resolve();
			}
			if (cmd === "new_conversation_id") return Promise.resolve("new-id");
			return Promise.resolve();
		});

		store = (await import("../../stores/conversationStore")).conversationStore;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("text_chunk from T1 stream updates T1 streamingText, not T2 (1408-a8d8)", async () => {
		store.setActiveTerminal("T1");
		await store.sendMessage("hello from T1", "sess-T1");

		store.setActiveTerminal("T2");
		await store.sendMessage("hello from T2", "sess-T2");

		// Fire text_chunk for T1's channel (keyed by sessionId "sess-T1")
		const ch1 = channels.get("sess-T1");
		expect(ch1).toBeDefined();
		ch1!.onmessage?.({ type: "text_chunk", text: "T1 chunk" });

		// T1 should have streaming text, T2 should not
		store.setActiveTerminal("T1");
		expect(store.streamingText()).toBe("T1 chunk");
		store.setActiveTerminal("T2");
		expect(store.streamingText()).toBe("");
	});

	it("completed event for T1 finalizes T1 messages, T2 unaffected (1408-a8d8)", async () => {
		store.setActiveTerminal("T1");
		await store.sendMessage("q", "sess-T1");

		store.setActiveTerminal("T2");
		await store.sendMessage("q2", "sess-T2");

		// Fire text_chunk then completed for T1 while T2 is active
		const ch1 = channels.get("sess-T1");
		expect(ch1).toBeDefined();
		ch1!.onmessage?.({ type: "text_chunk", text: "T1 response" });
		ch1!.onmessage?.({ type: "completed", reason: "end_turn", usage: null });

		store.setActiveTerminal("T1");
		expect(store.isStreaming()).toBe(false);
		const msgs = store.messages();
		expect(msgs[msgs.length - 1]?.content).toBe("T1 response");

		// T2 should still be streaming
		store.setActiveTerminal("T2");
		expect(store.isStreaming()).toBe(true);
	});
});

describe("conversationStore persistence — per-terminal (1407-56ca)", () => {
	let store: typeof import("../../stores/conversationStore").conversationStore;

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.resetModules();
		mockInvoke.mockReset();
		globalThis.localStorage?.clear();
		store = (await import("../../stores/conversationStore")).conversationStore;
		mockInvoke.mockResolvedValue(undefined);
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("schedulePersist for terminal A fires A's data even after switching to terminal B", async () => {
		store.setActiveTerminal("termA");
		store.addAssistantMessage("hello from A");
		// Timer for A is now scheduled. Switch to B before it fires.
		store.setActiveTerminal("termB");
		store.addAssistantMessage("hello from B");

		await vi.advanceTimersByTimeAsync(600);

		const saves = mockInvoke.mock.calls.filter((c) => c[0] === "save_conversation");
		// Both timers fire — one for A, one for B
		expect(saves.length).toBe(2);
		const contents = saves.map((c) => (c[1]?.conversation?.messages as Array<{ content: string }>)?.[0]?.content);
		expect(contents).toContain("hello from A");
		expect(contents).toContain("hello from B");
	});

	it("two terminals persist with independent session_id in meta", async () => {
		store.setActiveTerminal("termA");
		store.addAssistantMessage("A message");

		store.setActiveTerminal("termB");
		store.addAssistantMessage("B message");

		await vi.advanceTimersByTimeAsync(600);

		const saves = mockInvoke.mock.calls.filter((c) => c[0] === "save_conversation");
		expect(saves.length).toBe(2);
		const sessionIds = saves.map((c) => c[1]?.conversation?.meta?.session_id as string);
		expect(sessionIds).toContain("termA");
		expect(sessionIds).toContain("termB");
	});

	it("initFromDisk(tuicSession) loads conversation filtered by session_id via list_conversations", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_conversations") {
				return Promise.resolve([
					{ id: "conv-old", title: "old", session_id: "sess-X", created: 1, updated: 1, message_count: 1 },
					{ id: "conv-new", title: "new", session_id: "sess-X", created: 2, updated: 5, message_count: 2 },
					{ id: "conv-other", title: "other", session_id: "sess-Y", created: 3, updated: 3, message_count: 1 },
				]);
			}
			if (cmd === "load_conversation") {
				return Promise.resolve({
					meta: { id: "conv-new", title: "new", session_id: "sess-X", created: 2, updated: 5, message_count: 2 },
					messages: [
						{ role: "user", content: "hi", timestamp: 2 },
						{ role: "assistant", content: "hello", timestamp: 3 },
					],
					schema_version: 1,
				});
			}
			return Promise.resolve();
		});

		store.setActiveTerminal("termX");
		await store.initFromDisk("sess-X");

		expect(store.messages().length).toBe(2);
		expect(store.chatId()).toBe("conv-new");
		const loadCalls = mockInvoke.mock.calls.filter((c) => c[0] === "load_conversation");
		expect(loadCalls[0]?.[1]).toEqual({ id: "conv-new" });
	});
});

describe("conversationStore history panel (1412-ae57)", () => {
	let store: typeof import("../../stores/conversationStore").conversationStore;

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.resetModules();
		mockInvoke.mockReset();
		globalThis.localStorage?.clear();
		store = (await import("../../stores/conversationStore")).conversationStore;
		mockInvoke.mockResolvedValue(undefined);
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("listAllConversations returns all conversations from backend", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_conversations") {
				return Promise.resolve([
					{ id: "c1", title: "First chat", session_id: "s1", created: 1, updated: 1, message_count: 1 },
					{ id: "c2", title: "Second chat", session_id: "s2", created: 2, updated: 2, message_count: 3 },
				]);
			}
			return Promise.resolve();
		});

		const convs = await store.listAllConversations();
		expect(convs).toHaveLength(2);
		expect(convs[0]?.id).toBe("c1");
		expect(convs[1]?.title).toBe("Second chat");
		const listCalls = mockInvoke.mock.calls.filter((c) => c[0] === "list_conversations");
		expect(listCalls.length).toBe(1);
	});

	it("loadConversation replaces active terminal messages with loaded conversation", async () => {
		store.addUserMessage("old message");
		expect(store.messages()).toHaveLength(1);

		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "load_conversation") {
				return Promise.resolve({
					meta: { id: "past-conv", title: "Past", created: 1, updated: 2, message_count: 2 },
					messages: [
						{ role: "user", content: "hello", timestamp: 1 },
						{ role: "assistant", content: "hi", timestamp: 2 },
					],
					schema_version: 1,
				});
			}
			return Promise.resolve();
		});

		await store.loadConversation("past-conv");
		expect(store.messages()).toHaveLength(2);
		expect(store.messages()[0]?.content).toBe("hello");
		expect(store.messages()[1]?.content).toBe("hi");
		expect(store.chatId()).toBe("past-conv");
		expect(store.isStreaming()).toBe(false);
	});

	it("loadConversation does not affect other terminal states", async () => {
		store.setActiveTerminal("T1");
		store.addUserMessage("T1 message");

		store.setActiveTerminal("T2");
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "load_conversation") {
				return Promise.resolve({
					meta: { id: "loaded-conv", title: "Loaded", created: 1, updated: 1, message_count: 1 },
					messages: [{ role: "user", content: "loaded", timestamp: 1 }],
					schema_version: 1,
				});
			}
			return Promise.resolve();
		});
		await store.loadConversation("loaded-conv");

		expect(store.messages()).toHaveLength(1);

		store.setActiveTerminal("T1");
		expect(store.messages()).toHaveLength(1);
		expect(store.messages()[0]?.content).toBe("T1 message");
	});
});

// A detached AI Chat window fires its disk hydration on mount and does not wait
// for it — the input is live straight away, because the terminal it sends to
// came in as a param and needs no IPC. So a send can overtake a slow read, and
// the read then arrives holding the conversation as it was BEFORE that send.
// Applying it would erase the user's turn and drop the streaming flag, leaving
// the reply to land on a history that never asked the question.
describe("conversationStore detached hand-over (624-a6c3)", () => {
	let store: typeof import("../../stores/conversationStore").conversationStore;

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.resetModules();
		mockInvoke.mockReset();
		globalThis.localStorage?.clear();
		store = (await import("../../stores/conversationStore")).conversationStore;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("drops a disk read that a send overtook", async () => {
		let releaseLoad: (value: unknown) => void = () => {};
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "load_conversation") {
				return new Promise((resolve) => {
					releaseLoad = resolve;
				});
			}
			return Promise.resolve();
		});

		const loading = store.loadConversation("conv-on-disk");
		// Let the read reach the backend and block there.
		await vi.advanceTimersByTimeAsync(0);

		// Fired, not awaited — the panel's send handler does not await either, and
		// the user turn plus the streaming flag are set before the first await.
		void store.sendMessage("a question typed before the read came back", "sess-1");

		expect(store.isStreaming()).toBe(true);
		expect(store.messages()).toHaveLength(1);

		releaseLoad({
			meta: { id: "conv-on-disk", title: "Past", created: 1, updated: 2, message_count: 1 },
			messages: [{ role: "user", content: "an older turn", timestamp: 1 }],
			schema_version: 1,
		});
		await loading;

		expect(store.messages()).toHaveLength(1);
		expect(store.messages()[0]?.content).toBe("a question typed before the read came back");
		expect(store.isStreaming()).toBe(true);
	});

	// The ordinary case must keep working: nothing happened during the read, so
	// what came off disk is the conversation.
	it("applies a disk read that nothing overtook", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "load_conversation") {
				return Promise.resolve({
					meta: { id: "conv-on-disk", title: "Past", created: 1, updated: 2, message_count: 1 },
					messages: [{ role: "user", content: "an older turn", timestamp: 1 }],
					schema_version: 1,
				});
			}
			return Promise.resolve();
		});

		await store.loadConversation("conv-on-disk");

		expect(store.messages()).toHaveLength(1);
		expect(store.messages()[0]?.content).toBe("an older turn");
		expect(store.chatId()).toBe("conv-on-disk");
	});
});

// An agent run adds no message until it is over, so everything a reload taken
// mid-iteration has to restore — the tool-call log, the loop state, the
// iteration counter — lives in signals that used to reach no payload at all.
// A reload between turns never showed it: prose is the one thing that WAS saved.
describe("conversationStore agent run persistence (705-57fa)", () => {
	let store: typeof import("../../stores/conversationStore").conversationStore;
	const channels: Map<string, { onmessage: ((msg: unknown) => void) | null }> = new Map();

	function mockBackend(saved?: Record<string, unknown>) {
		mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
			if (cmd === "start_conversation") {
				const ch = args?.["onEvent"] as { onmessage: ((msg: unknown) => void) | null };
				if (ch && args?.["sessionId"]) channels.set(args["sessionId"] as string, ch);
				return Promise.resolve();
			}
			if (cmd === "new_conversation_id") return Promise.resolve("new-id");
			if (cmd === "list_conversations") {
				return Promise.resolve(
					saved ? [(saved as { meta: Record<string, unknown> }).meta] : [],
				);
			}
			if (cmd === "load_conversation") return Promise.resolve(saved);
			return Promise.resolve();
		});
	}

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.resetModules();
		mockInvoke.mockReset();
		channels.clear();
		globalThis.localStorage?.clear();
		mockBackend();
		store = (await import("../../stores/conversationStore")).conversationStore;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	/** Run an agent up to the middle of iteration 3, one tool call still open. */
	async function runToMidIteration(): Promise<void> {
		store.setActiveTerminal("termAgent");
		await store.startAgent("sess-agent", "fix the build");
		const ch = channels.get("sess-agent");
		expect(ch).toBeDefined();
		ch!.onmessage?.({ type: "thinking", iteration: 3 });
		ch!.onmessage?.({ type: "tool_call", tool_name: "ai_terminal_send_input", args: { text: "ls\n" } });
		ch!.onmessage?.({
			type: "tool_result",
			tool_name: "ai_terminal_send_input",
			success: true,
			output: "Cargo.toml",
		});
		ch!.onmessage?.({ type: "tool_call", tool_name: "ai_terminal_read", args: {} });
	}

	it("persists the tool-call log, loop state and iteration mid-iteration", async () => {
		await runToMidIteration();
		await vi.advanceTimersByTimeAsync(600);

		const saves = mockInvoke.mock.calls.filter((c) => c[0] === "save_conversation");
		expect(saves.length).toBeGreaterThan(0);
		const conv = saves[saves.length - 1]?.[1]?.conversation;
		expect(conv.agent.state).toBe("running");
		expect(conv.agent.currentIteration).toBe(3);
		expect(conv.agent.toolCalls).toHaveLength(2);
		expect(conv.agent.toolCalls[0].status).toBe("done");
		expect(conv.agent.toolCalls[0].toolName).toBe("ai_terminal_send_input");
		expect(conv.agent.toolCalls[0].result).toEqual({ success: true, output: "Cargo.toml" });
		expect(conv.agent.toolCalls[1].status).toBe("pending");
		expect(conv.agent.toolCalls[1].toolName).toBe("ai_terminal_read");
	});

	it("restores that run after a reload", async () => {
		await runToMidIteration();
		await vi.advanceTimersByTimeAsync(600);
		const saves = mockInvoke.mock.calls.filter((c) => c[0] === "save_conversation");
		const saved = saves[saves.length - 1]?.[1]?.conversation as Record<string, unknown>;
		(saved as { meta: { session_id: string } }).meta.session_id = "sess-agent";

		// Reload: a fresh module holds none of the signals the run just moved.
		vi.resetModules();
		mockInvoke.mockReset();
		channels.clear();
		mockBackend(saved);
		store = (await import("../../stores/conversationStore")).conversationStore;

		store.setActiveTerminal("termAgent");
		await store.initFromDisk("sess-agent");

		expect(store.agentState()).toBe("running");
		expect(store.currentIteration()).toBe(3);
		const restored = store.toolCalls();
		expect(restored).toHaveLength(2);
		expect(restored[0]?.toolName).toBe("ai_terminal_send_input");
		expect(restored[0]?.status).toBe("done");
		expect(restored[0]?.status === "done" && restored[0].result.output).toBe("Cargo.toml");
		expect(restored[1]?.status).toBe("pending");
	});

	// The version belongs to the backend now, so the store stops asserting one.
	it("leaves schema_version to the backend", async () => {
		store.addAssistantMessage("hello");
		await vi.advanceTimersByTimeAsync(600);
		const saves = mockInvoke.mock.calls.filter((c) => c[0] === "save_conversation");
		expect(saves[0]?.[1]?.conversation?.schema_version).toBeUndefined();
	});

	// A v1 document has no agent block at all. It must load as a plain chat with
	// an idle agent, not throw and not wedge the panel on a half-restored run.
	it("loads a v1 document with no agent block as an idle agent", async () => {
		mockBackend({
			meta: { id: "v1-conv", title: "Old", session_id: "sess-v1", created: 1, updated: 2, message_count: 1 },
			messages: [{ role: "user", content: "hi", timestamp: 1 }],
			schema_version: 1,
		});
		store.setActiveTerminal("termV1");
		await store.initFromDisk("sess-v1");

		expect(store.messages()).toHaveLength(1);
		expect(store.agentState()).toBe("idle");
		expect(store.currentIteration()).toBe(0);
		expect(store.toolCalls()).toEqual([]);
	});

	// The engine emits `thinking` for an assisted turn too — it is the same loop.
	// Marking the agent "running" there put the agent banner on a plain chat and
	// left it there (nothing in the assisted path clears it). Persisting that
	// would carry the false banner across every reload, for good.
	it("leaves the agent idle through an assisted turn", async () => {
		store.setActiveTerminal("termChat");
		await store.sendMessage("what is this error?", "sess-chat");
		const ch = channels.get("sess-chat");
		ch!.onmessage?.({ type: "thinking", iteration: 1 });
		expect(store.agentState()).toBe("idle");
		expect(store.currentIteration()).toBe(0);

		ch!.onmessage?.({ type: "text_chunk", text: "because" });
		ch!.onmessage?.({ type: "completed", reason: "end_turn", usage: null });
		await vi.advanceTimersByTimeAsync(600);

		const saves = mockInvoke.mock.calls.filter((c) => c[0] === "save_conversation");
		const conv = saves[saves.length - 1]?.[1]?.conversation;
		expect(conv.agent).toEqual({ state: "idle", currentIteration: 0, toolCalls: [] });
	});

	// An agent run that finished still has to reach disk: the completion is the
	// only record that the loop ever ran, and it adds no message either.
	it("persists a completed run that produced no message", async () => {
		store.setActiveTerminal("termDone");
		await store.startAgent("sess-done", "tidy up");
		const ch = channels.get("sess-done");
		ch!.onmessage?.({ type: "thinking", iteration: 1 });
		ch!.onmessage?.({ type: "completed", reason: "end_turn", usage: null });
		await vi.advanceTimersByTimeAsync(600);

		const saves = mockInvoke.mock.calls.filter((c) => c[0] === "save_conversation");
		expect(saves.length).toBeGreaterThan(0);
		expect(saves[saves.length - 1]?.[1]?.conversation?.agent?.state).toBe("completed");
	});
});

// Criterion 2 rests on this and nothing else: the detached window fires the
// hydration and never looks at the result, so an id with nothing behind it has
// to be the store's problem, not an unhandled rejection in the panel.
describe("conversationStore load of an unknown id (624-a6c3)", () => {
	let store: typeof import("../../stores/conversationStore").conversationStore;

	beforeEach(async () => {
		vi.resetModules();
		mockInvoke.mockReset();
		globalThis.localStorage?.clear();
		store = (await import("../../stores/conversationStore")).conversationStore;
	});

	it("resolves and leaves the conversation empty when the backend read fails", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "load_conversation") return Promise.reject(new Error("conversation not found"));
			return Promise.resolve();
		});

		await expect(store.loadConversation("never-saved")).resolves.toBeUndefined();

		expect(store.messages()).toEqual([]);
		expect(store.error()).toBeNull();
	});
});
