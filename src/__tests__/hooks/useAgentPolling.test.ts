import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { makeTerminal, testInScopeAsync } from "../helpers/store";
import { mockInvoke } from "../mocks/tauri";

/** Helper: advance timers and flush all pending microtasks */
async function tick(ms: number) {
	await vi.advanceTimersByTimeAsync(ms);
	await Promise.resolve();
	await Promise.resolve();
}

describe("useAgentPolling", () => {
	let store: typeof import("../../stores/terminals").terminalsStore;

	beforeEach(async () => {
		vi.resetModules();
		vi.useFakeTimers();
		mockInvoke.mockReset();
		store = (await import("../../stores/terminals")).terminalsStore;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("applies authoritative lifecycle transitions without retaining stale working state", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
		mockInvoke.mockResolvedValueOnce([
			{ session_id: "sess-1", state: { shell_state: "idle", agent_state: "working", background_work: true } },
		]);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		await syncAgentLifecycleStates();
		expect(store.get(id)?.agentState).toBe("working");
		expect(store.get(id)?.backgroundWork).toBe(true);
		expect(store.get(id)?.shellState).toBe("idle");

		mockInvoke.mockResolvedValueOnce([
			{ session_id: "sess-1", state: { shell_state: "idle", agent_state: "idle", background_work: false } },
		]);
		await syncAgentLifecycleStates();
		expect(store.get(id)?.agentState).toBe("idle");
		expect(store.get(id)?.backgroundWork).toBe(false);
	});

	it("tracks the queued-command depth, treating an omitted field as an empty queue", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
		mockInvoke.mockResolvedValueOnce([
			{ session_id: "sess-1", state: { shell_state: "busy", agent_state: "working", queued_commands: 3 } },
		]);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		await syncAgentLifecycleStates();
		expect(store.get(id)?.queuedCommands).toBe(3);

		// serde skips the field at zero, so its absence means the queue drained —
		// keeping the previous count would leave a badge offering to clear nothing.
		mockInvoke.mockResolvedValueOnce([{ session_id: "sess-1", state: { shell_state: "idle", agent_state: "idle" } }]);
		await syncAgentLifecycleStates();
		expect(store.get(id)?.queuedCommands).toBe(0);
	});

	it("reconciles awaiting state from the authoritative backend snapshot", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
		mockInvoke.mockResolvedValueOnce([
			{
				session_id: "sess-1",
				state: { agent_state: "awaiting_input", awaiting_input: true, question_confident: true },
			},
		]);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		await syncAgentLifecycleStates();
		expect(store.get(id)?.awaitingInput).toBe("question");
		expect(store.get(id)?.awaitingInputConfident).toBe(true);

		mockInvoke.mockResolvedValueOnce([
			{ session_id: "sess-1", state: { agent_state: "working", awaiting_input: false } },
		]);
		await syncAgentLifecycleStates();
		expect(store.get(id)?.awaitingInput).toBeNull();
		expect(store.get(id)?.awaitingInputConfident).toBe(false);
	});

	it("clears lifecycle state for a terminal omitted from a successful snapshot", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "lost-session" }));
		store.update(id, { agentState: "working", backgroundWork: true });
		mockInvoke.mockResolvedValueOnce([]);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		await syncAgentLifecycleStates();
		expect(store.get(id)?.shellState).toBe("exited");
		expect(store.get(id)?.sessionId).toBeNull();
		expect(store.get(id)?.agentState).toBeNull();
		expect(store.get(id)?.backgroundWork).toBe(false);
	});

	it("does not close an omitted terminal after a newer PTY event", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
		let resolveSnapshot!: (value: unknown) => void;
		mockInvoke.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					resolveSnapshot = resolve;
				}),
		);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		const pending = syncAgentLifecycleStates();
		store.update(id, { shellState: "busy" });
		resolveSnapshot([]);
		await pending;

		expect(store.get(id)?.sessionId).toBe("sess-1");
		expect(store.get(id)?.shellState).toBe("busy");
	});

	it("does not close an omitted terminal after its session is replaced", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "old-session" }));
		let resolveSnapshot!: (value: unknown) => void;
		mockInvoke.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					resolveSnapshot = resolve;
				}),
		);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		const pending = syncAgentLifecycleStates();
		store.update(id, { sessionId: "replacement-session" });
		resolveSnapshot([]);
		await pending;

		expect(store.get(id)?.sessionId).toBe("replacement-session");
	});

	it("does not let a snapshot overwrite a PTY state event that arrived after the request", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
		store.update(id, { shellState: "idle", agentState: "working", backgroundWork: true });
		let resolveSnapshot!: (value: unknown) => void;
		mockInvoke.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					resolveSnapshot = resolve;
				}),
		);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		const pending = syncAgentLifecycleStates();
		store.update(id, { shellState: "busy" });
		resolveSnapshot([
			{ session_id: "sess-1", state: { shell_state: "idle", agent_state: "idle", background_work: false } },
		]);
		await pending;

		expect(store.get(id)?.shellState).toBe("busy");
		expect(store.get(id)?.agentState).toBe("working");
		expect(store.get(id)?.backgroundWork).toBe(true);
	});

	it("recovers polling after a native session-list timeout", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
		let resolveHung!: (value: unknown) => void;
		mockInvoke.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					resolveHung = resolve;
				}),
		);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		const timedOut = syncAgentLifecycleStates();
		await vi.advanceTimersByTimeAsync(5_001);
		await timedOut;

		mockInvoke.mockResolvedValueOnce([
			{ session_id: "sess-1", state: { shell_state: "idle", agent_state: "completed", background_work: false } },
		]);
		await syncAgentLifecycleStates();
		resolveHung([]);

		expect(store.get(id)?.agentState).toBe("completed");
	});

	it("serializes lifecycle polls so an older response cannot overwrite a newer snapshot", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
		let resolveOlder!: (value: unknown) => void;
		const older = new Promise((resolve) => {
			resolveOlder = resolve;
		});
		mockInvoke
			.mockImplementationOnce(() => older)
			.mockResolvedValueOnce([
				{ session_id: "sess-1", state: { shell_state: "idle", agent_state: "completed", background_work: false } },
			]);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		const oldRequest = syncAgentLifecycleStates();
		const coalescedRequest = syncAgentLifecycleStates();
		resolveOlder([
			{ session_id: "sess-1", state: { shell_state: "busy", agent_state: "working", background_work: true } },
		]);
		await oldRequest;
		await coalescedRequest;
		await syncAgentLifecycleStates();

		expect(store.get(id)?.agentState).toBe("completed");
		expect(store.get(id)?.backgroundWork).toBe(false);
		expect(store.get(id)?.shellState).toBe("idle");
	});

	it("polls the active terminal's foreground process", async () => {
		mockInvoke.mockResolvedValue("claude");

		await testInScopeAsync(async () => {
			const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
			store.setActive(id);

			const { useAgentPolling } = await import("../../hooks/useAgentPolling");
			useAgentPolling();

			// Poll fires on first interval tick (30s fallback), not immediately
			await vi.advanceTimersByTimeAsync(30_000);
			await Promise.resolve(); // flush microtasks

			expect(mockInvoke).toHaveBeenCalledWith("get_session_foreground_process", {
				sessionId: "sess-1",
			});
			expect(store.get(id)?.agentType).toBe("claude");
		});
	});

	it("does not poll when no active terminal", async () => {
		await testInScopeAsync(async () => {
			const { useAgentPolling } = await import("../../hooks/useAgentPolling");
			useAgentPolling();

			await vi.advanceTimersByTimeAsync(30_000);

			expect(mockInvoke).not.toHaveBeenCalledWith("get_session_foreground_process", expect.anything());
		});
	});

	it("does not poll when active terminal has no session", async () => {
		await testInScopeAsync(async () => {
			const id = store.add(makeTerminal({ name: "T1" }));
			store.setActive(id);

			const { useAgentPolling } = await import("../../hooks/useAgentPolling");
			useAgentPolling();

			await vi.advanceTimersByTimeAsync(30_000);

			expect(mockInvoke).not.toHaveBeenCalledWith("get_session_foreground_process", expect.anything());
		});
	});

	it("sets agentType to null when result is null", async () => {
		mockInvoke.mockResolvedValue(null);

		await testInScopeAsync(async () => {
			const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
			store.setActive(id);

			const { useAgentPolling } = await import("../../hooks/useAgentPolling");
			useAgentPolling();

			await vi.advanceTimersByTimeAsync(30_000);
			await Promise.resolve();

			expect(store.get(id)?.agentType).toBeNull();
		});
	});

	it("handles invoke errors gracefully", async () => {
		mockInvoke.mockRejectedValue(new Error("Session not found"));

		await testInScopeAsync(async () => {
			const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
			store.setActive(id);

			const { useAgentPolling } = await import("../../hooks/useAgentPolling");
			useAgentPolling();

			// Should not throw
			await vi.advanceTimersByTimeAsync(30_000);
			await Promise.resolve();

			// agentType should remain null (default)
			expect(store.get(id)?.agentType).toBeNull();
		});
	});

	describe("session discovery", () => {
		/** A discovery result with no rebuilt launch command (no readable agent process). */
		const discovered = (sessionId: string) => ({ sessionId, launchCommand: null });

		it("calls discover_agent_session when agentType transitions null→agent and agentSessionId is null", async () => {
			let pollCount = 0;
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") {
					pollCount++;
					return Promise.resolve(pollCount >= 2 ? "claude" : null);
				}
				if (cmd === "get_session_leaf_pid") return Promise.resolve(1234);
				if (cmd === "discover_agent_session") return Promise.resolve(discovered("found-uuid"));
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();

				await tick(30_000); // first poll: null
				expect(store.get(id)?.agentType).toBeNull();

				await tick(30_000); // second poll: claude detected + discovery fires in same cycle
				expect(store.get(id)?.agentType).toBe("claude");
				expect(mockInvoke).toHaveBeenCalledWith(
					"discover_agent_session",
					expect.objectContaining({
						agentType: "claude",
					}),
				);
				expect(store.get(id)?.agentSessionId).toBe("found-uuid");
			});
		});

		it("re-discovers claude session on subsequent polls (tracks /clear)", async () => {
			let discoverCount = 0;
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") return Promise.resolve("claude");
				if (cmd === "get_session_leaf_pid") return Promise.resolve(1234);
				if (cmd === "discover_agent_session") {
					discoverCount++;
					return Promise.resolve(discovered(discoverCount <= 2 ? "uuid-1" : "uuid-2"));
				}
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();

				await tick(30_000); // poll 1 + discovery
				expect(store.get(id)?.agentSessionId).toBe("uuid-1");

				await tick(30_000); // poll 2 + re-discover (same uuid, no store update)
				expect(store.get(id)?.agentSessionId).toBe("uuid-1");

				await tick(30_000); // poll 3 + re-discover (new uuid after /clear)
				expect(store.get(id)?.agentSessionId).toBe("uuid-2");

				const discoveryCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "discover_agent_session");
				expect(discoveryCalls).toHaveLength(3);
			});
		});

		it("re-discovers non-claude agents on subsequent polls too", async () => {
			let discoverCount = 0;
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") return Promise.resolve("gemini");
				if (cmd === "get_session_leaf_pid") return Promise.resolve(1234);
				if (cmd === "discover_agent_session") {
					discoverCount++;
					return Promise.resolve(discovered(discoverCount <= 2 ? "found-uuid" : "new-uuid"));
				}
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();

				await tick(30_000); // poll 1 + discovery
				expect(store.get(id)?.agentSessionId).toBe("found-uuid");

				await tick(30_000); // poll 2 + re-discover (same uuid)
				expect(store.get(id)?.agentSessionId).toBe("found-uuid");

				await tick(30_000); // poll 3 + re-discover (new uuid after /clear)
				expect(store.get(id)?.agentSessionId).toBe("new-uuid");

				const discoveryCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "discover_agent_session");
				expect(discoveryCalls).toHaveLength(3);
			});
		});

		it("discovers claude session even when tuicSession is set", async () => {
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") return Promise.resolve("claude");
				if (cmd === "get_session_leaf_pid") return Promise.resolve(1234);
				if (cmd === "discover_agent_session") return Promise.resolve(discovered("discovered-uuid"));
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
				store.update(id, { tuicSession: "tuic-uuid-123" });

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();

				await tick(30_000);
				expect(store.get(id)?.agentType).toBe("claude");

				const discoveryCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "discover_agent_session");
				expect(discoveryCalls).toHaveLength(1);
				expect(store.get(id)?.agentSessionId).toBe("discovered-uuid");
			});
		});

		it("discovers non-claude agents even when tuicSession is set", async () => {
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") return Promise.resolve("gemini");
				if (cmd === "get_session_leaf_pid") return Promise.resolve(1234);
				if (cmd === "discover_agent_session") return Promise.resolve(discovered("discovered-uuid"));
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
				store.update(id, { tuicSession: "tuic-uuid-123" });

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();

				await tick(30_000);
				expect(store.get(id)?.agentType).toBe("gemini");

				const discoveryCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "discover_agent_session");
				expect(discoveryCalls).toHaveLength(1);
				expect(store.get(id)?.agentSessionId).toBe("discovered-uuid");
			});
		});

		it("skips discovery for agents without sessionDiscovery config (e.g. aider)", async () => {
			mockInvoke.mockResolvedValue("aider");

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();

				await tick(30_000);
				expect(store.get(id)?.agentType).toBe("aider");

				const discoveryCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "discover_agent_session");
				expect(discoveryCalls).toHaveLength(0);
			});
		});

		it("dispatches synthetic shell-state to plugins when agent first detected", async () => {
			// Bug: when agentType transitions null→"claude", structured shell-state events
			// dispatched BEFORE detection completes were filtered out (pluginMatchesSession
			// returned false because agentType was still null). The plugin never learned
			// the current shellState. Fix: after agent-started, replay the current shellState
			// so filtered plugins catch up.
			mockInvoke.mockResolvedValue("claude");

			await testInScopeAsync(async () => {
				const { detectAgentForTerminal } = await import("../../hooks/useAgentPolling");
				const { pluginRegistry } = await import("../../plugins/pluginRegistry");

				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-synth" }));
				// Simulate: shell is idle, but agentType not yet detected
				store.update(id, { shellState: "idle" });

				// Register a plugin with agentTypes: ["claude"] that listens for shell-state
				const shellStateHandler = vi.fn();
				await pluginRegistry.register(
					{
						id: "test-keepalive",
						onload: (host) => {
							host.registerStructuredEventHandler("shell-state", shellStateHandler);
						},
						onunload: () => {},
					},
					["pty:write"],
					["claude"],
				);

				// Before detection: dispatch shell-state directly → should be filtered (agentType null)
				pluginRegistry.dispatchStructuredEvent("shell-state", { state: "idle" }, "sess-synth");
				await new Promise<void>((r) => queueMicrotask(r));
				expect(shellStateHandler).not.toHaveBeenCalled();

				// Now detect agent (null → claude) — should trigger synthetic replay
				await detectAgentForTerminal(id, "idle");
				await new Promise<void>((r) => queueMicrotask(r));

				// Plugin should have received the synthetic shell-state event
				expect(shellStateHandler).toHaveBeenCalledWith(expect.objectContaining({ state: "idle" }), "sess-synth");

				pluginRegistry.unregister("test-keepalive");
			});
		});

		it("fires agent-stopped for filtered plugins on direct agent→agent transitions", async () => {
			// Bug: when agentType switched from claude to codex without first passing
			// through null (user exits claude and immediately runs codex, before the
			// NULL_THRESHOLD idle-streak clears the agent), neither agent-started nor
			// agent-stopped was dispatched. Plugins filtered on agentTypes=["claude"]
			// (e.g. cache-keepalive) kept their internal per-session state and wrote
			// keepalive messages into the now-codex PTY. Fix: emit agent-stopped
			// before the store update (filter still matches old type) and agent-started
			// after (filter matches new type).
			let foregroundReturn: string | null = "claude";
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") return Promise.resolve(foregroundReturn);
				if (cmd === "get_session_leaf_pid") return Promise.resolve(1234);
				if (cmd === "discover_agent_session") return Promise.resolve(null);
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				const { detectAgentForTerminal } = await import("../../hooks/useAgentPolling");
				const { pluginRegistry } = await import("../../plugins/pluginRegistry");

				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-trans" }));
				store.update(id, { shellState: "idle" });

				const claudeEvents: string[] = [];
				await pluginRegistry.register(
					{
						id: "test-claude-only",
						onload: (host) => {
							host.onStateChange((e) => {
								if (e.sessionId === "sess-trans") claudeEvents.push(e.type);
							});
						},
						onunload: () => {},
					},
					["pty:write"],
					["claude"],
				);

				const codexEvents: string[] = [];
				await pluginRegistry.register(
					{
						id: "test-codex-only",
						onload: (host) => {
							host.onStateChange((e) => {
								if (e.sessionId === "sess-trans") codexEvents.push(e.type);
							});
						},
						onunload: () => {},
					},
					["pty:write"],
					["codex"],
				);

				// null → claude: claude-filtered plugin gets agent-started
				await detectAgentForTerminal(id, "busy");
				expect(store.get(id)?.agentType).toBe("claude");
				expect(claudeEvents).toEqual(["agent-started"]);
				expect(codexEvents).toEqual([]);

				// claude → codex (direct): claude plugin MUST receive agent-stopped,
				// codex plugin MUST receive agent-started
				foregroundReturn = "codex";
				await detectAgentForTerminal(id, "busy");
				expect(store.get(id)?.agentType).toBe("codex");
				expect(claudeEvents).toEqual(["agent-started", "agent-stopped"]);
				expect(codexEvents).toEqual(["agent-started"]);

				pluginRegistry.unregister("test-claude-only");
				pluginRegistry.unregister("test-codex-only");
			});
		});

		it("clears agentSessionId on the first definitive idle transition and allows re-discovery", async () => {
			// Only source="idle" can clear — polls never clear (sticky agentType fix).
			let phase: "active1" | "idle" | "active2" = "active1";
			let discoverCount = 0;
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") {
					if (phase === "idle") return Promise.resolve(null);
					return Promise.resolve("claude");
				}
				if (cmd === "get_session_leaf_pid") return Promise.resolve(1234);
				if (cmd === "discover_agent_session") {
					discoverCount++;
					return Promise.resolve(discovered(discoverCount <= 2 ? "uuid-1" : "uuid-2"));
				}
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling, detectAgentForTerminal } = await import("../../hooks/useAgentPolling");
				useAgentPolling();

				await tick(30_000); // poll 1: claude + discovery
				await tick(30_000); // poll 2: still claude + re-discover (same uuid)
				expect(store.get(id)?.agentSessionId).toBe("uuid-1");

				// A shell-idle transition means the prompt has returned, so clear immediately.
				phase = "idle";
				await detectAgentForTerminal(id, "idle");
				expect(store.get(id)?.agentType).toBeNull();
				expect(store.get(id)?.agentSessionId).toBeNull();

				phase = "active2";
				await tick(30_000); // poll 3: re-launched → re-discovery
				expect(store.get(id)?.agentType).toBe("claude");
				expect(store.get(id)?.agentSessionId).toBe("uuid-2");
			});
		});

		it("passes claimed_ids from other terminals to avoid duplicate assignment", async () => {
			let discoverCount = 0;
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") return Promise.resolve("claude");
				if (cmd === "get_session_leaf_pid") return Promise.resolve(1234);
				if (cmd === "discover_agent_session") {
					discoverCount++;
					return Promise.resolve(discovered(discoverCount === 1 ? "uuid-a" : "uuid-b"));
				}
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				const id1 = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
				const id2 = store.add({ sessionId: "sess-2", fontSize: 14, name: "T2", cwd: null, awaitingInput: null });

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();

				await tick(30_000); // both polled sequentially + both discover

				const discoveryCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "discover_agent_session");
				expect(discoveryCalls).toHaveLength(2);

				// Second discovery call must include the first terminal's claimed UUID
				const secondArgs = discoveryCalls[1];
				expect(secondArgs[1]).toHaveProperty("claimedIds");
				expect(secondArgs[1].claimedIds).toContain("uuid-a");

				expect(store.get(id1)?.agentSessionId).toBe("uuid-a");
				expect(store.get(id2)?.agentSessionId).toBe("uuid-b");
			});
		});
	});

	describe("timer lifecycle", () => {
		it("does not restart the 30s fallback poll when a terminal is added mid-cycle", async () => {
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") return Promise.resolve(null);
				// Listing both sessions up front (sess-2 doesn't exist yet) keeps the
				// mount-time lifecycle catch-up from treating either as exited and
				// clearing its sessionId — which would make detectAgentForTerminal bail
				// out before the fallback poll under test ever gets a chance to run.
				if (cmd === "list_active_sessions")
					return Promise.resolve([{ session_id: "sess-1" }, { session_id: "sess-2" }]);
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();
				await tick(0); // flush the initial lifecycle sync
				mockInvoke.mockClear();

				await tick(25_000); // just short of the 30s fallback poll

				// Tab churn mid-cycle must not reset the fallback timer's countdown.
				store.add(makeTerminal({ name: "T2", sessionId: "sess-2" }));

				await tick(5_000); // total 30s since useAgentPolling() started

				expect(mockInvoke).toHaveBeenCalledWith("get_session_foreground_process", { sessionId: "sess-1" });
				expect(mockInvoke).toHaveBeenCalledWith("get_session_foreground_process", { sessionId: "sess-2" });
			});
		});

		/// Supersedes the visibility-gated 1 Hz lifecycle poll (#652-0114). Gating
		/// it on `document.visibilityState` only silenced a hidden window; a
		/// visible idle one kept sampling forever. The backend now pushes
		/// `session-state-changed`, so there is nothing left to gate — and a
		/// visible window is the stricter case to assert.
		it("never re-polls list_active_sessions after the mount-time catch-up", async () => {
			mockInvoke.mockResolvedValue([]);
			expect(document.visibilityState).not.toBe("hidden");

			await testInScopeAsync(async () => {
				store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();
				await tick(0);
				expect(mockInvoke).toHaveBeenCalledWith("list_active_sessions");

				mockInvoke.mockClear();
				await tick(10_000); // 10 lifecycle-poll ticks would have fired

				expect(mockInvoke).not.toHaveBeenCalledWith("list_active_sessions");
			});
		});
	});

	describe("session-state push", () => {
		/** Capture the handlers `subscribeEvents` registers on the Tauri window. */
		async function captureWindowEvents(): Promise<Map<string, (event: { payload: unknown }) => void>> {
			const listeners = new Map<string, (event: { payload: unknown }) => void>();
			const { listen } = await import("@tauri-apps/api/event");
			vi.mocked(listen).mockImplementation(async (name, handler) => {
				listeners.set(name, handler as (event: { payload: unknown }) => void);
				return () => {};
			});
			return listeners;
		}

		/** The mount-time catch-up must see the session, or it reaps the terminal
		 *  and the push below would have no owner to apply to. */
		const catchUpSnapshot = [{ session_id: "sess-1", state: {} }];

		it("converges the store from a session-state-changed push, with no timer and no snapshot", async () => {
			mockInvoke.mockResolvedValue(catchUpSnapshot);
			const listeners = await captureWindowEvents();

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();
				await tick(0); // subscribeEvents resolves its dynamic import

				const push = listeners.get("session-state-changed");
				expect(push, "the hook must subscribe to the backend push").toBeDefined();

				// From here on nothing may reach the backend: no timer is advanced
				// and no snapshot is fetched, so the event is the only input.
				mockInvoke.mockClear();
				push?.({
					payload: {
						session_id: "sess-1",
						state: {
							shell_state: "busy",
							agent_state: "awaiting_input",
							awaiting_input: true,
							question_confident: true,
							queued_commands: 2,
						},
					},
				});

				expect(store.get(id)?.agentState).toBe("awaiting_input");
				expect(store.get(id)?.awaitingInput).toBe("question");
				expect(store.get(id)?.awaitingInputConfident).toBe(true);
				expect(store.get(id)?.shellState).toBe("busy");
				expect(store.get(id)?.queuedCommands).toBe(2);
				expect(mockInvoke).not.toHaveBeenCalled();
			});
		});

		it("applies a later push, so a cleared question does not stay latched", async () => {
			mockInvoke.mockResolvedValue(catchUpSnapshot);
			const listeners = await captureWindowEvents();

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();
				await tick(0);
				const push = listeners.get("session-state-changed");

				push?.({
					payload: {
						session_id: "sess-1",
						state: { awaiting_input: true, question_confident: true, agent_state: "awaiting_input" },
					},
				});
				expect(store.get(id)?.awaitingInput).toBe("question");

				// serde skips `queued_commands` at zero and omits `awaiting_input`
				// when false, so the retraction arrives as an absence.
				push?.({ payload: { session_id: "sess-1", state: { agent_state: "working", shell_state: "busy" } } });
				expect(store.get(id)?.awaitingInput).toBeNull();
				expect(store.get(id)?.awaitingInputConfident).toBe(false);
				expect(store.get(id)?.agentState).toBe("working");
				expect(store.get(id)?.queuedCommands).toBe(0);
			});
		});

		it("ignores a push for a session no terminal owns", async () => {
			mockInvoke.mockResolvedValue(catchUpSnapshot);
			const listeners = await captureWindowEvents();

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();
				await tick(0);
				store.update(id, { agentState: "working" });
				const push = listeners.get("session-state-changed");

				push?.({ payload: { session_id: "someone-elses-session", state: { agent_state: "idle" } } });
				push?.({ payload: null });
				push?.({ payload: { state: { agent_state: "idle" } } });

				expect(store.get(id)?.agentState).toBe("working");
			});
		});
	});
});
